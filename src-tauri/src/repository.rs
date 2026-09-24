use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use tauri::State;

use crate::project::{OpenProject, IGNORED};
use candidates::CandidateRegistry;

#[allow(dead_code)] // Some candidate metadata accessors are exercised only by focused validation tests.
pub mod candidates;

pub const MAX_TOOL_CALLS: usize = 8;
pub const MAX_CONTEXT_BYTES: usize = 48_000;
const MAX_READ_BYTES: usize = 12_000;
const MAX_FILE_BYTES: u64 = 256_000;
const MAX_LIST: usize = 120;
const MAX_SEARCH: usize = 30;
const MAX_SCAN_FILES: usize = 2_000;
pub const MAX_PROPOSAL_REPLACEMENTS: usize = 4;
pub const MAX_PROPOSAL_BYTES: usize = 24_000;
pub const AMBIGUOUS_OLD_TEXT_ERROR: &str = "old_text is ambiguous in the current file.";

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProposedReplacement {
    pub path: String,
    pub old_text: String,
    pub new_text: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingChange {
    pub path: String,
    pub before: String,
    pub after: String,
    pub replacements: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingProposal {
    pub summary: String,
    pub changes: Vec<PendingChange>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CandidatePosition { Before, After }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InsertedElement { Paragraph, Heading, Link }

/// Application-owned semantic intent. Candidate IDs are resolved against the request-scoped registry.
#[derive(Clone, Debug, PartialEq)]
pub enum SemanticEdit {
    ReplaceTextCandidate { candidate_id: String, replacement: String, summary: String },
    ReplaceAttribute { candidate_id: String, replacement_value: String, summary: String },
    InsertRelative { anchor_candidate_id: String, position: CandidatePosition, element: InsertedElement, text: String, href: Option<String>, summary: String },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewedFile {
    content: String,
    truncated: bool,
}

#[derive(Default)]
pub struct PendingChanges(pub Mutex<Option<PendingProposal>>);

/// Convert supported semantic intent into an application-built proposal using the same validators.
pub fn validate_semantic_edit(root: &Path, registry: &CandidateRegistry, edit: SemanticEdit) -> Result<PendingProposal, String> {
    match edit {
        SemanticEdit::ReplaceTextCandidate { candidate_id, replacement, summary } => {
            let candidate = registry.candidate(&candidate_id).ok_or("Unknown candidate ID.")?;
            if candidate.role().is_attribute() || candidate.role() == candidates::CandidateRole::Form {
                return Err("Text replacement does not match the candidate role.".into());
            }
            validate_candidate_proposal(root, registry, &candidate_id, summary, replacement)
        }
        SemanticEdit::ReplaceAttribute { candidate_id, replacement_value, summary } => {
            let candidate = registry.candidate(&candidate_id).ok_or("Unknown candidate ID.")?;
            if !candidate.role().is_attribute() {
                return Err("Attribute replacement does not match the candidate role.".into());
            }
            validate_candidate_proposal(root, registry, &candidate_id, summary, replacement_value)
        }
        SemanticEdit::InsertRelative { anchor_candidate_id, position, element, text, href, summary } => {
            validate_candidate_insertion(root, registry, &anchor_candidate_id, position, element, text, href, summary)
        }
    }
}

pub fn stage_pending_proposal(pending: &PendingChanges, proposal: PendingProposal) -> Result<(), String> {
    *pending.0.lock().map_err(|_| "Pending change state unavailable")? = Some(proposal);
    Ok(())
}

#[tauri::command]
pub fn apply_pending_change(open_project: State<'_, OpenProject>, pending: State<'_, PendingChanges>) -> Result<(), String> {
    let root = open_project.0.lock().map_err(|_| "Project state unavailable")?.clone()
        .ok_or_else(|| "No project is open.".to_owned())?;
    let mut state = pending.0.lock().map_err(|_| "Pending change state unavailable")?;
    let proposal = state.as_ref().ok_or_else(|| "There is no pending proposal.".to_owned())?;
    apply_proposal(&root, proposal)?;
    *state = None;
    Ok(())
}

#[tauri::command]
pub fn reject_pending_change(pending: State<'_, PendingChanges>) -> Result<(), String> {
    *pending.0.lock().map_err(|_| "Pending change state unavailable")? = None;
    Ok(())
}

#[tauri::command]
pub fn view_repository_file(path: String, open_project: State<'_, OpenProject>) -> Result<ViewedFile, String> {
    let root = open_project.0.lock().map_err(|_| "Project state unavailable")?.clone()
        .ok_or_else(|| "No project is open.".to_owned())?;
    view_file(&root, &path)
}

#[derive(Deserialize)]
pub struct ToolRequest {
    pub tool: String,
    #[serde(default)] pub path: String,
    #[serde(default)] pub query: String,
}

#[derive(Clone, Serialize)]
pub struct Activity { pub label: String }

fn protected(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        name == ".env" || name.starts_with(".env.") || name.ends_with(".pem") || name.ends_with(".key")
            || name.contains("credential") || matches!(name.as_str(), "id_rsa" | "id_ed25519" | ".ssh" | ".aws" | ".azure" | ".npmrc" | ".pypirc")
    })
}

fn allowed_relative(path: &str) -> Result<PathBuf, String> {
    let candidate = Path::new(path);
    if candidate.is_absolute() || path.starts_with("\\\\") || path.contains(':') || path.contains('\\')
        || candidate.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err("Only project-relative paths are allowed.".into());
    }
    if candidate.components().any(|part| IGNORED.iter().any(|item| part.as_os_str().to_string_lossy().eq_ignore_ascii_case(item))) {
        return Err("Generated or ignored paths are unavailable.".into());
    }
    Ok(candidate.to_path_buf())
}

fn resolve(root: &Path, path: &str) -> Result<PathBuf, String> {
    let relative = allowed_relative(path)?;
    let full = fs::canonicalize(root.join(relative)).map_err(|_| "Project path does not exist.".to_owned())?;
    if !full.starts_with(root) { return Err("Path escapes the opened project.".into()); }
    Ok(full)
}

fn read_text_file(path: &Path) -> Result<String, String> {
    let metadata = fs::metadata(path).map_err(|_| "Project path does not exist.".to_owned())?;
    if !metadata.is_file() { return Err("Target must be an existing file.".into()); }
    if metadata.len() > MAX_FILE_BYTES { return Err("File exceeds the size limit.".into()); }
    let bytes = fs::read(path).map_err(|_| "Cannot read project file.".to_owned())?;
    if bytes.iter().take(8_000).any(|byte| *byte == 0) { return Err("Binary files are unavailable.".into()); }
    String::from_utf8(bytes).map_err(|_| "Non-UTF-8 files are unavailable.".into())
}

pub fn validate_proposal(root: &Path, summary: String, edits: Vec<ProposedReplacement>) -> Result<PendingProposal, String> {
    if summary.trim().is_empty() || summary.chars().count() > 240 { return Err("Proposal summary is missing or too long.".into()); }
    if edits.is_empty() || edits.len() > MAX_PROPOSAL_REPLACEMENTS { return Err(format!("A proposal must contain 1 to {MAX_PROPOSAL_REPLACEMENTS} replacements.")); }
    let total = edits.iter().map(|edit| edit.path.len() + edit.old_text.len() + edit.new_text.len()).sum::<usize>();
    if total > MAX_PROPOSAL_BYTES { return Err("Proposal exceeds the size limit.".into()); }
    let first_path = edits[0].path.clone();
    if edits.iter().any(|edit| edit.path != first_path) { return Err("Milestone 04 proposals may change one file at a time.".into()); }
    let relative = allowed_relative(&first_path)?;
    if protected(&relative) { return Err("Protected files are unavailable.".into()); }
    let full = resolve(root, &first_path)?;
    let before = read_text_file(&full)?;
    let mut after = before.clone();
    let mut seen_old = std::collections::HashSet::new();
    for edit in &edits {
        if edit.old_text.is_empty() { return Err("old_text must not be empty.".into()); }
        if edit.old_text == edit.new_text { return Err("Proposal contains a no-op replacement.".into()); }
        if !seen_old.insert(edit.old_text.as_str()) { return Err("Proposal contains duplicate or conflicting replacements.".into()); }
        let matches = after.match_indices(&edit.old_text).count();
        if matches == 0 { return Err("old_text was not found in the current file.".into()); }
        if matches > 1 { return Err(AMBIGUOUS_OLD_TEXT_ERROR.into()); }
        after = after.replacen(&edit.old_text, &edit.new_text, 1);
    }
    if after == before { return Err("Proposal does not change the file.".into()); }
    Ok(PendingProposal { summary: summary.trim().to_owned(), changes: vec![PendingChange { path: first_path, before, after, replacements: edits.len() }] })
}

fn escape_html_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

pub fn validate_candidate_proposal(root: &Path, registry: &CandidateRegistry, candidate_id: &str, summary: String, replacement: String) -> Result<PendingProposal, String> {
    if summary.trim().is_empty() || summary.chars().count() > 240 { return Err("Proposal summary is missing or too long.".into()); }
    if replacement.trim().is_empty() { return Err("Candidate replacement must not be empty.".into()); }
    let candidate = registry.verify_current(root, candidate_id)?;
    let path = candidate.path().to_owned();
    let range = candidate.range();
    let original = candidate.original().to_owned();
    let before = registry.snapshot_for(candidate_id).ok_or("Unknown candidate ID.")?.to_owned();
    if before.get(range.clone()) != Some(original.as_str()) {
        return Err("Candidate source range is invalid.".into());
    }
    if candidate.role().is_attribute() {
        if replacement.chars().any(char::is_control) {
            return Err("Attribute replacement contains unsupported control characters.".into());
        }
        if matches!(candidate.role(), candidates::CandidateRole::AttributeHref | candidates::CandidateRole::AttributeSrc)
            && !safe_attribute_url(candidate.role(), replacement.trim()) {
            return Err("Attribute URL uses an unsupported or unsafe scheme.".into());
        }
    }
    let replacement = escape_html_text(&replacement);
    if path.len() + original.len() + replacement.len() > MAX_PROPOSAL_BYTES {
        return Err("Proposal exceeds the size limit.".into());
    }
    if replacement == original { return Err("Proposal contains a no-op replacement.".into()); }
    let mut after = String::with_capacity(before.len() - range.len() + replacement.len());
    after.push_str(&before[..range.start]);
    after.push_str(&replacement);
    after.push_str(&before[range.end..]);
    Ok(PendingProposal { summary: summary.trim().to_owned(), changes: vec![PendingChange { path, before, after, replacements: 1 }] })
}

fn safe_attribute_url(role: candidates::CandidateRole, value: &str) -> bool {
    if value.is_empty() || value.chars().any(char::is_control) { return false; }
    let Some((scheme, _)) = value.split_once(':') else { return true; };
    let first_path_delimiter = value.find(|character| matches!(character, '/' | '?' | '#')).unwrap_or(value.len());
    if value.find(':').unwrap_or(value.len()) > first_path_delimiter { return true; }
    if scheme.is_empty() || !scheme.as_bytes()[0].is_ascii_alphabetic()
        || !scheme.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')) {
        return false;
    }
    let scheme = scheme.to_ascii_lowercase();
    let remainder = value.split_once(':').map_or("", |(_, remainder)| remainder);
    match role {
        candidates::CandidateRole::AttributeHref => match scheme.as_str() {
            "http" | "https" => remainder.strip_prefix("//").is_some_and(|authority| !authority.split(|character| matches!(character, '/' | '?' | '#')).next().unwrap_or_default().is_empty()),
            "mailto" | "tel" => !remainder.trim().is_empty(),
            _ => false,
        },
        candidates::CandidateRole::AttributeSrc => matches!(scheme.as_str(), "http" | "https")
            && remainder.strip_prefix("//").is_some_and(|authority| !authority.split(|character| matches!(character, '/' | '?' | '#')).next().unwrap_or_default().is_empty()),
        _ => false,
    }
}

pub fn validate_candidate_insertion(
    root: &Path,
    registry: &CandidateRegistry,
    candidate_id: &str,
    position: CandidatePosition,
    element: InsertedElement,
    text: String,
    href: Option<String>,
    summary: String,
) -> Result<PendingProposal, String> {
    if summary.trim().is_empty() || summary.chars().count() > 240 { return Err("Proposal summary is missing or too long.".into()); }
    if text.trim().is_empty() || text.len() > candidates::MAX_CANDIDATE_SOURCE_BYTES { return Err("Inserted text is empty or exceeds the size limit.".into()); }
    let candidate = registry.verify_current(root, candidate_id)?;
    if !matches!(candidate.role(), candidates::CandidateRole::HeadingOne | candidates::CandidateRole::Paragraph | candidates::CandidateRole::Form) {
        return Err("Insertion anchor must be a verified heading, paragraph, or form.".into());
    }
    let element_range = candidate.element_range().ok_or("Insertion anchor has no verified element boundary.")?;
    let content_range = candidate.range();
    let before = registry.snapshot_for(candidate_id).ok_or("Unknown candidate ID.")?.to_owned();
    if before.get(element_range.clone()).is_none() || before.get(content_range.clone()).is_none()
        || content_range.start < element_range.start || content_range.end > element_range.end {
        return Err("Insertion anchor range is invalid.".into());
    }
    let escaped_text = escape_html_text(&text);
    let html = match (element, href) {
        (InsertedElement::Paragraph, None) => format!("<p>{escaped_text}</p>"),
        (InsertedElement::Heading, None) => format!("<h2>{escaped_text}</h2>"),
        (InsertedElement::Link, Some(href)) if safe_attribute_url(candidates::CandidateRole::AttributeHref, href.trim()) => {
            format!("<a href=\"{}\">{escaped_text}</a>", escape_html_text(&href))
        }
        (InsertedElement::Link, Some(_)) => return Err("Attribute URL uses an unsupported or unsafe scheme.".into()),
        (InsertedElement::Link, None) => return Err("Inserted link requires a safe destination.".into()),
        (_, Some(_)) => return Err("Only inserted links may include a destination.".into()),
    };
    if candidate.path().len() + before.len() + html.len() > MAX_PROPOSAL_BYTES {
        return Err("Proposal exceeds the size limit.".into());
    }
    let offset = match position { CandidatePosition::Before => element_range.start, CandidatePosition::After => element_range.end };
    let line_start = before[..element_range.start].rfind('\n').map_or(0, |index| index + 1);
    let prefix = &before[line_start..element_range.start];
    let newline = if before.contains("\r\n") { "\r\n" } else { "\n" };
    let insertion = if prefix.bytes().all(|byte| matches!(byte, b' ' | b'\t')) {
        let indentation = prefix;
        match position {
            CandidatePosition::Before => format!("{html}{newline}{indentation}"),
            CandidatePosition::After => format!("{newline}{indentation}{html}"),
        }
    } else { html };
    let mut after = String::with_capacity(before.len() + insertion.len());
    after.push_str(&before[..offset]);
    after.push_str(&insertion);
    after.push_str(&before[offset..]);
    Ok(PendingProposal { summary: summary.trim().to_owned(), changes: vec![PendingChange { path: candidate.path().to_owned(), before, after, replacements: 1 }] })
}

pub fn apply_proposal(root: &Path, proposal: &PendingProposal) -> Result<(), String> {
    if proposal.changes.len() != 1 { return Err("Invalid pending proposal.".into()); }
    let change = &proposal.changes[0];
    let relative = allowed_relative(&change.path)?;
    if protected(&relative) { return Err("Protected files are unavailable.".into()); }
    let full = resolve(root, &change.path)?;
    let current = read_text_file(&full)?;
    if current != change.before {
        return Err(format!("{} changed since Elma prepared this proposal. Ask Elma to inspect it again.", change.path));
    }
    fs::write(full, change.after.as_bytes()).map_err(|_| "Could not write the approved change.".to_owned())
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

fn ignored(path: &Path) -> bool {
    path.file_name().is_some_and(|name| IGNORED.iter().any(|item| name.to_string_lossy().eq_ignore_ascii_case(item)))
}

fn walk(root: &Path, folder: &Path, files: &mut Vec<PathBuf>, max: usize) -> Result<bool, String> {
    let mut entries = fs::read_dir(folder).map_err(|_| "Cannot inspect project directory.".to_owned())?
        .filter_map(Result::ok).collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if files.len() >= max { return Ok(true); }
        let path = entry.path();
        if ignored(&path) || protected(&path) { continue; }
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_symlink() { continue; }
        if kind.is_dir() {
            if walk(root, &path, files, max)? { return Ok(true); }
        } else if kind.is_file() && path.starts_with(root) { files.push(path); }
    }
    Ok(false)
}

pub fn discover_html_candidates(root: &Path, registry: &mut CandidateRegistry) -> Result<usize, String> {
    let mut files = Vec::new();
    if walk(root, root, &mut files, MAX_SCAN_FILES)? {
        return Err("Candidate discovery reached the repository scan limit. Narrow the request before editing.".into());
    }
    let html = files.into_iter().filter(|path| path.extension().is_some_and(|extension|
        extension.eq_ignore_ascii_case("html") || extension.eq_ignore_ascii_case("htm")
    )).collect::<Vec<_>>();
    if html.is_empty() { return Err("No HTML file is available for candidate editing.".into()); }
    if html.len() != 1 { return Err("Candidate editing currently requires exactly one HTML file.".into()); }
    registry.discover_html(root, &relative(root, &html[0]))
}

pub fn execute(root: &Path, request: &ToolRequest) -> (String, Activity) {
    let result = match request.tool.as_str() {
        "list_files" => list_files(root, &request.path),
        "search_files" => search_files(root, &request.query),
        "read_file" => read_file(root, &request.path),
        _ => Err("Unknown repository tool.".into()),
    };
    let label = match request.tool.as_str() {
        "list_files" => "Project structure".to_owned(),
        "search_files" => format!("Search: {}", request.query.chars().take(60).collect::<String>()),
        "read_file" => format!("Read: {}", request.path.chars().take(100).collect::<String>()),
        _ => "Invalid tool request".to_owned(),
    };
    (result.unwrap_or_else(|error| format!("Error: {error}")), Activity { label })
}

fn list_files(root: &Path, path: &str) -> Result<String, String> {
    if path.contains('*') || path.contains('?') {
        return Err("list_files expects a project-relative directory, not a glob. Use path='' to list the project root, then use an exact returned path.".into());
    }
    let folder = if path.is_empty() || path == "." { root.to_path_buf() } else { resolve(root, path)? };
    if !folder.is_dir() { return Err("Path is not a directory.".into()); }
    let mut lines = Vec::new();
    let truncated = list_walk(root, &folder, &mut lines)?;
    if truncated { lines.push("[Listing truncated]".into()); }
    Ok(lines.join("\n"))
}

fn list_walk(root: &Path, folder: &Path, lines: &mut Vec<String>) -> Result<bool, String> {
    let mut entries = fs::read_dir(folder).map_err(|_| "Cannot inspect project directory.".to_owned())?
        .filter_map(Result::ok).collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if lines.len() >= MAX_LIST { return Ok(true); }
        let path = entry.path();
        if ignored(&path) || protected(&path) { continue; }
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_symlink() { continue; }
        lines.push(format!("{} ({})", relative(root, &path), if kind.is_dir() { "directory" } else { "file" }));
        if kind.is_dir() && list_walk(root, &path, lines)? { return Ok(true); }
    }
    Ok(false)
}

fn read_file(root: &Path, path: &str) -> Result<String, String> {
    if path.is_empty() { return Err("A file path is required.".into()); }
    let relative_path = allowed_relative(path)?;
    if protected(&relative_path) { return Err("Protected file: contents are unavailable.".into()); }
    let file = resolve(root, path)?;
    if protected(&file) { return Err("Protected file: contents are unavailable.".into()); }
    if !file.is_file() { return Err("Path is not a file.".into()); }
    let size = fs::metadata(&file).map_err(|_| "Cannot inspect file.".to_owned())?.len();
    if size > MAX_FILE_BYTES { return Err(format!("File exceeds the {MAX_FILE_BYTES}-byte read limit.")); }
    let bytes = fs::read(&file).map_err(|_| "Cannot read file.".to_owned())?;
    if bytes.contains(&0) { return Err("Binary file: contents are unavailable.".into()); }
    let content = std::str::from_utf8(&bytes).map_err(|_| "Non-UTF-8 file: contents are unavailable.".to_owned())?;
    let mut output = String::new();
    for (index, line) in content.lines().enumerate() {
        let next = format!("{}: {}\n", index + 1, line);
        if output.len() + next.len() > MAX_READ_BYTES { output.push_str("[File truncated; request a more specific file or search]\n"); break; }
        output.push_str(&next);
    }
    Ok(output)
}

fn view_file(root: &Path, path: &str) -> Result<ViewedFile, String> {
    if path.is_empty() { return Err("A file path is required.".into()); }
    let relative_path = allowed_relative(path)?;
    if protected(&relative_path) { return Err("Protected file: contents are unavailable.".into()); }
    let file = resolve(root, path)?;
    if protected(&file) { return Err("Protected file: contents are unavailable.".into()); }
    let content = read_text_file(&file)?;
    if content.as_bytes().contains(&0) { return Err("Binary file: contents are unavailable.".into()); }
    if content.len() <= MAX_READ_BYTES {
        return Ok(ViewedFile { content, truncated: false });
    }
    let mut end = MAX_READ_BYTES;
    while !content.is_char_boundary(end) { end -= 1; }
    Ok(ViewedFile { content: content[..end].to_owned(), truncated: true })
}

fn search_files(root: &Path, query: &str) -> Result<String, String> {
    if query.trim().is_empty() || query.len() > 120 { return Err("Search query must be 1–120 characters.".into()); }
    if query.trim().starts_with("*.") || (query.contains('*') && query.split_whitespace().count() > 1) {
        return Err("Search uses one literal text query, not filename globs. Use list_files to find filenames.".into());
    }
    let mut files = Vec::new();
    let scan_truncated = walk(root, root, &mut files, MAX_SCAN_FILES)?;
    let mut matches = Vec::new();
    let mut scanned_bytes = 0_u64;
    for file in files {
        let Ok(metadata) = fs::metadata(&file) else { continue };
        if metadata.len() > MAX_FILE_BYTES { continue; }
        if scanned_bytes + metadata.len() > 8_000_000 { matches.push("[Search limited to 8 MB of source files]".into()); break; }
        scanned_bytes += metadata.len();
        let Ok(bytes) = fs::read(&file) else { continue };
        if bytes.contains(&0) { continue; }
        let Ok(content) = std::str::from_utf8(&bytes) else { continue };
        for (index, line) in content.lines().enumerate() {
            if line.to_lowercase().contains(&query.to_lowercase()) {
                matches.push(format!("{}:{}: {}", relative(root, &file), index + 1, line.chars().take(160).collect::<String>()));
                if matches.len() >= MAX_SEARCH { matches.push("[Results truncated]".into()); return Ok(matches.join("\n")); }
            }
        }
    }
    if scan_truncated { matches.push("[Scan limited to first 2000 files]".into()); }
    Ok(if matches.is_empty() { "No matches.".into() } else { matches.join("\n") })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    fn fixture() -> PathBuf {
        let root = std::env::temp_dir().join(format!("aiide-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("hello.txt"), "hello\nworld").unwrap();
        root.canonicalize().unwrap()
    }
    #[test] fn read_safety() {
        let root = fixture();
        assert!(read_file(&root, "hello.txt").unwrap().contains("1: hello"));
        assert!(read_file(&root, "../outside").is_err());
        assert!(read_file(&root, "C:/Windows/win.ini").is_err());
        assert!(read_file(&root, "missing.txt").is_err());
        fs::write(root.join(".env"), "secret").unwrap();
        assert!(read_file(&root, ".env").unwrap_err().contains("Protected"));
        fs::write(root.join("binary.bin"), [0, 1, 2]).unwrap();
        assert!(read_file(&root, "binary.bin").unwrap_err().contains("Binary"));
        fs::write(root.join("huge.txt"), vec![b'a'; MAX_FILE_BYTES as usize + 1]).unwrap();
        assert!(read_file(&root, "huge.txt").unwrap_err().contains("limit"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn viewer_uses_repository_boundaries_and_read_limits() {
        let root = fixture();
        let viewed = view_file(&root, "hello.txt").unwrap();
        assert_eq!(viewed.content, "hello\nworld");
        assert!(!viewed.truncated);
        assert!(view_file(&root, "../outside.txt").is_err());
        fs::write(root.join(".env"), "secret").unwrap();
        assert!(view_file(&root, ".env").unwrap_err().contains("Protected"));
        fs::write(root.join("long.txt"), "é".repeat(MAX_READ_BYTES)).unwrap();
        let viewed = view_file(&root, "long.txt").unwrap();
        assert!(viewed.truncated);
        assert!(viewed.content.len() <= MAX_READ_BYTES);
        assert!(std::str::from_utf8(viewed.content.as_bytes()).is_ok());
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn bounds() {
        let root = fixture();
        fs::write(root.join("many.txt"), "match\n".repeat(100)).unwrap();
        assert!(search_files(&root, "match").unwrap().contains("Results truncated"));
        assert!(search_files(&root, "*.html *.css").unwrap_err().contains("globs"));
        for index in 0..MAX_LIST + 5 { fs::write(root.join(format!("{index}.txt")), "x").unwrap(); }
        assert!(list_files(&root, "").unwrap().contains("Listing truncated"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn list_files_accepts_directories_and_rejects_globs() {
        let root = fixture();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src/site.css"), "body {}").unwrap();
        assert!(list_files(&root, "").unwrap().contains("src/site.css (file)"));
        assert!(list_files(&root, "src").unwrap().contains("src/site.css (file)"));
        for glob in ["*.css", "**/*.css", "src/**/*.tsx"] {
            let error = list_files(&root, glob).unwrap_err();
            assert!(error.contains("directory, not a glob"));
            assert!(error.contains("path=''"));
        }
        fs::remove_dir_all(root).unwrap();
    }
    fn edit(path: &str, old_text: &str, new_text: &str) -> ProposedReplacement {
        ProposedReplacement { path: path.into(), old_text: old_text.into(), new_text: new_text.into() }
    }
    #[test] fn validates_exact_replacement_and_applies_approved_change() {
        let root = fixture();
        let proposal = validate_proposal(&root, "Greeting".into(), vec![edit("hello.txt", "hello", "Hello")]).unwrap();
        assert_eq!(proposal.changes[0].after, "Hello\nworld");
        assert_eq!(fs::read_to_string(root.join("hello.txt")).unwrap(), "hello\nworld");
        apply_proposal(&root, &proposal).unwrap();
        assert_eq!(fs::read_to_string(root.join("hello.txt")).unwrap(), "Hello\nworld");
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn candidate_proposal_replaces_only_the_verified_heading_range() {
        let root = fixture();
        let path = root.join("page.html");
        let before = "<head><title>OrbitNote</title><meta name=\"description\" content=\"Keep me\"></head>\n<body><h1 class=\"hero\">\n  Old <span>heading</span>\n</h1><p>Keep this too.</p></body>";
        fs::write(&path, before).unwrap();
        let mut registry = CandidateRegistry::new();
        assert_eq!(discover_html_candidates(&root, &mut registry).unwrap(), 3);
        let heading = registry.model_view().into_iter().find(|candidate| candidate.role == candidates::CandidateRole::HeadingOne).unwrap();
        let proposal = validate_candidate_proposal(&root, &registry, &heading.id, "Update the visible h1 heading.".into(), "Welcome to OrbitNote 2.0".into()).unwrap();
        assert_eq!(proposal.changes[0].before, before);
        assert_eq!(proposal.changes[0].after, "<head><title>OrbitNote</title><meta name=\"description\" content=\"Keep me\"></head>\n<body><h1 class=\"hero\">Welcome to OrbitNote 2.0</h1><p>Keep this too.</p></body>");
        assert_eq!(fs::read_to_string(&path).unwrap(), before, "proposal construction must not write");

        let rejected = Some(proposal.clone());
        drop(rejected);
        assert_eq!(fs::read_to_string(&path).unwrap(), before, "rejecting a proposal must not write");
        apply_proposal(&root, &proposal).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), proposal.changes[0].after);
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn semantic_edits_validate_supported_primitives_and_stage_pending_changes() {
        let root = fixture();
        let before = "<head><title>OrbitNote</title></head><body><h1>Main</h1><p>Intro</p><a href=\"/old\">Join</a></body>";
        fs::write(root.join("page.html"), before).unwrap();
        let mut registry = CandidateRegistry::new();
        registry.discover_html(&root, "page.html").unwrap();
        let views = registry.model_view();
        let id = |role| views.iter().find(|view| view.role == role).unwrap().id.clone();
        let heading_id = id(candidates::CandidateRole::HeadingOne);
        let title_id = id(candidates::CandidateRole::DocumentTitle);
        let href_id = id(candidates::CandidateRole::AttributeHref);
        let text = validate_semantic_edit(&root, &registry, SemanticEdit::ReplaceTextCandidate {
            candidate_id: heading_id.clone(), replacement: "Welcome".into(), summary: "Update heading".into(),
        }).unwrap();
        assert!(text.changes[0].after.contains("<h1>Welcome</h1>"));
        let attribute = validate_semantic_edit(&root, &registry, SemanticEdit::ReplaceAttribute {
            candidate_id: href_id, replacement_value: "/start".into(), summary: "Update destination".into(),
        }).unwrap();
        assert!(attribute.changes[0].after.contains("href=\"/start\""));
        let insertion = validate_semantic_edit(&root, &registry, SemanticEdit::InsertRelative {
            anchor_candidate_id: id(candidates::CandidateRole::Paragraph), position: CandidatePosition::After,
            element: InsertedElement::Heading, text: "Next steps".into(), href: None, summary: "Add heading".into(),
        }).unwrap();
        assert!(insertion.changes[0].after.contains("<h2>Next steps</h2>"));

        let pending = PendingChanges::default();
        stage_pending_proposal(&pending, text.clone()).unwrap();
        assert_eq!(pending.0.lock().unwrap().as_ref().unwrap().changes[0].after, text.changes[0].after);
        assert!(validate_semantic_edit(&root, &registry, SemanticEdit::ReplaceTextCandidate {
            candidate_id: "unknown".into(), replacement: "x".into(), summary: "x".into(),
        }).unwrap_err().contains("Unknown candidate ID"));
        assert!(validate_semantic_edit(&root, &registry, SemanticEdit::ReplaceAttribute {
            candidate_id: heading_id.clone(), replacement_value: "x".into(), summary: "x".into(),
        }).unwrap_err().contains("does not match"));
        assert!(validate_semantic_edit(&root, &registry, SemanticEdit::ReplaceTextCandidate {
            candidate_id: title_id, replacement: "New title".into(), summary: "Update title".into(),
        }).unwrap().changes[0].after.contains("<title>New title</title>"));
        fs::write(root.join("page.html"), before.replace("Main", "Externally changed")).unwrap();
        assert!(validate_semantic_edit(&root, &registry, SemanticEdit::ReplaceTextCandidate {
            candidate_id: heading_id, replacement: "Stale".into(), summary: "Update heading".into(),
        }).unwrap_err().contains("changed since discovery"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn candidate_proposals_escape_text_and_fail_closed() {
        let root = fixture();
        fs::write(root.join("page.html"), "<h1>Original</h1>").unwrap();
        let mut registry = CandidateRegistry::new();
        discover_html_candidates(&root, &mut registry).unwrap();
        let id = registry.model_view()[0].id.clone();
        let escaped = validate_candidate_proposal(&root, &registry, &id, "Heading".into(), "Tea & <code> \"today\"".into()).unwrap();
        assert_eq!(escaped.changes[0].after, "<h1>Tea &amp; &lt;code&gt; &quot;today&quot;</h1>");
        assert_eq!(validate_candidate_proposal(&root, &registry, "unknown", "Heading".into(), "Changed".into()).unwrap_err(), "Unknown candidate ID.");
        assert!(validate_candidate_proposal(&root, &registry, &id, "Heading".into(), "   ".into()).unwrap_err().contains("empty"));
        assert!(validate_candidate_proposal(&root, &registry, &id, "Heading".into(), "Original".into()).unwrap_err().contains("no-op"));
        assert!(validate_candidate_proposal(&root, &registry, &id, "Heading".into(), "x".repeat(MAX_PROPOSAL_BYTES)).unwrap_err().contains("size limit"));
        fs::write(root.join("page.html"), "<h1>Externally changed</h1>").unwrap();
        assert!(validate_candidate_proposal(&root, &registry, &id, "Heading".into(), "Changed".into()).unwrap_err().contains("changed since discovery"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn attribute_candidate_proposal_changes_only_value_and_rejects_unsafe_or_stale_source() {
        let root = fixture();
        let before = "<body><a href=\"/old\" title='Join'>Join</a></body>";
        fs::write(root.join("page.html"), before).unwrap();
        let mut registry = CandidateRegistry::new();
        registry.discover_html(&root, "page.html").unwrap();
        let href = registry.model_view().into_iter().find(|candidate| candidate.role == candidates::CandidateRole::AttributeHref).unwrap();
        let source = registry.candidate(&href.id).unwrap();
        assert_eq!(&before[source.range()], "/old", "Rust owns the exact value range");
        let proposal = validate_candidate_proposal(&root, &registry, &href.id, "Update signup destination".into(), "/register?source=a&next=b".into()).unwrap();
        assert_eq!(proposal.changes[0].after, "<body><a href=\"/register?source=a&amp;next=b\" title='Join'>Join</a></body>");
        assert_eq!(fs::read_to_string(root.join("page.html")).unwrap(), before);
        assert!(validate_candidate_proposal(&root, &registry, &href.id, "Unsafe URL".into(), "javascript:alert(1)".into()).unwrap_err().contains("unsafe scheme"));
        fs::write(root.join("page.html"), before.replace("/old", "/external" )).unwrap();
        assert!(validate_candidate_proposal(&root, &registry, &href.id, "Stale URL".into(), "/new".into()).unwrap_err().contains("changed since discovery"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn candidate_insertion_uses_verified_element_bounds_and_apply_rejects_stale_file() {
        let root = fixture();
        let path = root.join("page.html");
        let before = "<body>\r\n  <h1>Main</h1>\r\n</body>";
        fs::write(&path, before).unwrap();
        let mut registry = CandidateRegistry::new();
        registry.discover_html(&root, "page.html").unwrap();
        let heading = registry.model_view().into_iter().find(|candidate| candidate.role == candidates::CandidateRole::HeadingOne).unwrap();
        let anchor = registry.candidate(&heading.id).unwrap();
        let element_range = anchor.element_range().unwrap();
        assert_eq!(&before[element_range], "<h1>Main</h1>");
        let proposal = validate_candidate_insertion(
            &root, &registry, &heading.id, CandidatePosition::After, InsertedElement::Paragraph,
            "Built <AIIDE>".into(), None, "Insert paragraph".into(),
        ).unwrap();
        assert_eq!(proposal.changes[0].after, "<body>\r\n  <h1>Main</h1>\r\n  <p>Built &lt;AIIDE&gt;</p>\r\n</body>");
        assert_eq!(fs::read_to_string(&path).unwrap(), before, "proposal construction must not write");
        assert!(validate_candidate_insertion(
            &root, &registry, &heading.id, CandidatePosition::After, InsertedElement::Link,
            "Unsafe".into(), Some("javascript:alert(1)".into()), "Unsafe link".into(),
        ).unwrap_err().contains("unsafe scheme"));
        fs::write(&path, before.replace("Main", "Changed elsewhere")).unwrap();
        assert!(apply_proposal(&root, &proposal).unwrap_err().contains("changed since"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn rejects_unsafe_missing_ambiguous_and_noop_proposals() {
        let root = fixture();
        fs::write(root.join(".env"), "secret").unwrap();
        fs::write(root.join("repeat.txt"), "same same").unwrap();
        for (proposal, expected) in [
            (edit("../hello.txt", "hello", "x"), "project-relative"),
            (edit("C:/Windows/win.ini", "hello", "x"), "project-relative"),
            (edit(".env", "secret", "x"), "Protected"),
            (edit("missing.txt", "hello", "x"), "does not exist"),
            (edit("hello.txt", "absent", "x"), "not found"),
            (edit("repeat.txt", "same", "x"), "ambiguous"),
            (edit("hello.txt", "hello", "hello"), "no-op"),
        ] {
            assert!(validate_proposal(&root, "Test".into(), vec![proposal]).unwrap_err().contains(expected));
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn unique_surrounding_context_is_required_without_first_match_selection() {
        let root = fixture();
        let path = root.join("repeated.txt");
        let original = "section-a\nvalue\nsection-b\nvalue\n";
        fs::write(&path, original).unwrap();
        assert_eq!(validate_proposal(&root, "Ambiguous".into(), vec![edit("repeated.txt", "value", "changed")]).unwrap_err(), AMBIGUOUS_OLD_TEXT_ERROR);
        assert_eq!(fs::read_to_string(&path).unwrap(), original, "ambiguous validation must not select the first match");
        let proposal = validate_proposal(&root, "Unique".into(), vec![edit("repeated.txt", "section-b\nvalue", "section-b\nchanged")]).unwrap();
        assert_eq!(proposal.changes[0].after, "section-a\nvalue\nsection-b\nchanged\n");
        assert_eq!(fs::read_to_string(&path).unwrap(), original, "validation must only prepare a preview");
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn rejects_oversized_and_conflicting_proposals_without_partial_write() {
        let root = fixture();
        let original = fs::read_to_string(root.join("hello.txt")).unwrap();
        let oversized = edit("hello.txt", "hello", &"x".repeat(MAX_PROPOSAL_BYTES + 1));
        assert!(validate_proposal(&root, "Large".into(), vec![oversized]).is_err());
        let conflicting = vec![edit("hello.txt", "hello", "Hello"), edit("hello.txt", "missing", "x")];
        assert!(validate_proposal(&root, "Conflict".into(), conflicting).is_err());
        assert_eq!(fs::read_to_string(root.join("hello.txt")).unwrap(), original);
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn stale_proposal_fails_closed() {
        let root = fixture();
        let proposal = validate_proposal(&root, "Greeting".into(), vec![edit("hello.txt", "hello", "Hello")]).unwrap();
        fs::write(root.join("hello.txt"), "external\nworld").unwrap();
        assert!(apply_proposal(&root, &proposal).unwrap_err().contains("changed since"));
        assert_eq!(fs::read_to_string(root.join("hello.txt")).unwrap(), "external\nworld");
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn discarding_a_proposal_writes_nothing() {
        let root = fixture();
        let proposal = validate_proposal(&root, "Greeting".into(), vec![edit("hello.txt", "hello", "Hello")]).unwrap();
        let mut pending = Some(proposal);
        drop(pending.take());
        assert!(pending.is_none());
        assert_eq!(fs::read_to_string(root.join("hello.txt")).unwrap(), "hello\nworld");
        fs::remove_dir_all(root).unwrap();
    }
}
