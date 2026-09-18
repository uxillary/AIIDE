use super::{allowed_relative, protected, read_text_file, relative, resolve};
use serde::Serialize;
use std::ops::Range;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

pub const MAX_CANDIDATES: usize = 32;
pub const MAX_CANDIDATE_FILES: usize = 8;
pub const MAX_CANDIDATE_SOURCE_BYTES: usize = 4_096;
pub const MAX_CANDIDATE_CONTEXT_BYTES: usize = 120;
pub const MAX_CANDIDATE_EXCERPT_BYTES: usize = 160;
pub const MAX_CANDIDATE_PATH_BYTES: usize = 512;
const MAX_HEADING_INLINE_DEPTH: usize = 8;
const MAX_HEADING_INLINE_TAGS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateRole {
    DocumentTitle,
    HeadingOne,
    Paragraph,
}

impl CandidateRole {
    fn description(self) -> &'static str {
        match self {
            Self::DocumentTitle => "Document title",
            Self::HeadingOne => "Visible h1 heading",
            Self::Paragraph => "Paragraph text",
        }
    }
}

/// Rust-owned source reference. Its range addresses the captured snapshot, not a later file read.
#[derive(Debug)]
pub struct Candidate {
    id: String,
    path: String,
    snapshot_index: usize,
    range: Range<usize>,
    original: String,
    role: CandidateRole,
    description: &'static str,
    before_context: String,
    after_context: String,
}

impl Candidate {
    pub fn id(&self) -> &str { &self.id }
    pub fn path(&self) -> &str { &self.path }
    pub fn range(&self) -> Range<usize> { self.range.clone() }
    pub fn original(&self) -> &str { &self.original }
    pub fn role(&self) -> CandidateRole { self.role }
    pub fn description(&self) -> &str { self.description }
    pub fn before_context(&self) -> &str { &self.before_context }
    pub fn after_context(&self) -> &str { &self.after_context }
}

struct SourceSnapshot {
    path: String,
    text: String,
}

/// A new registry belongs to one discovery request. IDs have meaning only in this instance.
pub struct CandidateRegistry {
    request_id: u64,
    snapshots: Vec<SourceSnapshot>,
    candidates: Vec<Candidate>,
}

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

impl Default for CandidateRegistry {
    fn default() -> Self {
        Self { request_id: NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed), snapshots: Vec::new(), candidates: Vec::new() }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateView {
    pub id: String,
    pub role: CandidateRole,
    pub role_index: usize,
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub description: String,
    pub excerpt: String,
    pub before_context: String,
}

impl CandidateRegistry {
    pub fn new() -> Self { Self::default() }

    /// Capture one verified UTF-8 HTML file. Discovery only reads the filesystem.
    pub fn discover_html(&mut self, root: &Path, path: &str) -> Result<usize, String> {
        if self.snapshots.len() >= MAX_CANDIDATE_FILES {
            return Err("Candidate file limit reached.".into());
        }
        let requested = allowed_relative(path)?;
        if protected(&requested) { return Err("Protected files are unavailable.".into()); }
        if !requested.extension().is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("html") || ext.to_string_lossy().eq_ignore_ascii_case("htm")) {
            return Err("HTML candidate discovery requires an .html or .htm file.".into());
        }
        let full = resolve(root, path)?;
        if protected(&full) { return Err("Protected files are unavailable.".into()); }
        let verified_path = relative(root, &full);
        if verified_path.len() > MAX_CANDIDATE_PATH_BYTES {
            return Err("Candidate path exceeds the size limit.".into());
        }
        if self.snapshots.iter().any(|snapshot| snapshot.path == verified_path) {
            return Err("File was already captured in this request.".into());
        }
        let text = read_text_file(&full)?;
        let found = extract_html(&text, MAX_CANDIDATES - self.candidates.len());
        let count = found.len();
        let snapshot_index = self.snapshots.len();
        for (role, range) in found {
            let original = text[range.clone()].to_owned();
            let before_context = bounded_suffix(&text[..range.start], MAX_CANDIDATE_CONTEXT_BYTES);
            let after_context = bounded_prefix(&text[range.end..], MAX_CANDIDATE_CONTEXT_BYTES);
            self.candidates.push(Candidate {
                id: format!("c{:016x}{:04x}", self.request_id, self.candidates.len() + 1),
                path: verified_path.clone(), snapshot_index, range, original, role,
                description: role.description(), before_context, after_context,
            });
        }
        self.snapshots.push(SourceSnapshot { path: verified_path, text });
        Ok(count)
    }

    pub fn candidate(&self, id: &str) -> Option<&Candidate> {
        self.candidates.iter().find(|candidate| candidate.id == id)
    }

    pub fn snapshot_for(&self, id: &str) -> Option<&str> {
        let candidate = self.candidate(id)?;
        Some(&self.snapshots[candidate.snapshot_index].text)
    }

    /// Recheck the captured file before a candidate ID is reported as selected.
    pub fn verify_current(&self, root: &Path, id: &str) -> Result<&Candidate, String> {
        let candidate = self.candidate(id).ok_or("Unknown candidate ID.")?;
        let full = resolve(root, &candidate.path).map_err(|_| "Candidate source is stale or unavailable.")?;
        if protected(&full) { return Err("Candidate source is stale or unavailable.".into()); }
        let current = read_text_file(&full).map_err(|_| "Candidate source is stale or unavailable.")?;
        if current != self.snapshots[candidate.snapshot_index].text {
            return Err("Candidate source changed since discovery. Inspect it again.".into());
        }
        Ok(candidate)
    }

    pub fn model_view(&self) -> Vec<CandidateView> {
        let mut role_counts = [0; 3];
        self.candidates.iter().map(|candidate| {
            let snapshot = &self.snapshots[candidate.snapshot_index].text;
            let role_slot = match candidate.role {
                CandidateRole::DocumentTitle => 0,
                CandidateRole::HeadingOne => 1,
                CandidateRole::Paragraph => 2,
            };
            role_counts[role_slot] += 1;
            CandidateView {
                id: candidate.id.clone(), role: candidate.role, role_index: role_counts[role_slot], path: candidate.path.clone(),
                start_line: line_at(snapshot, candidate.range.start),
                end_line: line_at(snapshot, candidate.range.end.saturating_sub(1)),
                description: candidate.description.to_owned(),
                excerpt: excerpt(&candidate.original),
                before_context: candidate.before_context.clone(),
            }
        }).collect()
    }
}

fn line_at(text: &str, offset: usize) -> usize {
    1 + text.as_bytes()[..offset].iter().filter(|byte| **byte == b'\n').count()
}

fn bounded_prefix(text: &str, limit: usize) -> String {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) { end -= 1; }
    text[..end].to_owned()
}

fn bounded_suffix(text: &str, limit: usize) -> String {
    let mut start = text.len().saturating_sub(limit);
    while !text.is_char_boundary(start) { start += 1; }
    text[start..].to_owned()
}

fn excerpt(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut short = bounded_prefix(&collapsed, MAX_CANDIDATE_EXCERPT_BYTES);
    if short.len() < collapsed.len() {
        while short.len() + 3 > MAX_CANDIDATE_EXCERPT_BYTES { short.pop(); }
        short.push_str("...");
    }
    short
}

struct Tag<'a> {
    name: &'a str,
    attributes: &'a str,
    end: usize,
    closing: bool,
    self_closing: bool,
}

fn parse_tag(text: &str, start: usize) -> Option<Tag<'_>> {
    let bytes = text.as_bytes();
    let mut cursor = start + 1;
    let closing = bytes.get(cursor) == Some(&b'/');
    if closing { cursor += 1; }
    let name_start = cursor;
    while bytes.get(cursor).is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'-') { cursor += 1; }
    if cursor == name_start || !bytes[name_start].is_ascii_alphabetic() { return None; }
    let name_end = cursor;
    if bytes.get(cursor).is_some_and(|byte| !byte.is_ascii_whitespace() && *byte != b'>' && *byte != b'/') { return None; }
    let mut quote = None;
    while let Some(&byte) = bytes.get(cursor) {
        match (quote, byte) {
            (Some(q), b) if q == b => quote = None,
            (None, b'\'' | b'"') => quote = Some(byte),
            (None, b'>') => break,
            _ => {}
        }
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'>') { return None; }
    let attributes = &text[name_end..cursor];
    if closing && !attributes.trim().is_empty() { return None; }
    Some(Tag {
        name: &text[name_start..name_end], attributes, end: cursor + 1, closing,
        self_closing: attributes.trim_end().ends_with('/'),
    })
}

fn hidden_attribute(attributes: &str) -> bool {
    let lower = attributes.to_ascii_lowercase();
    let compact = lower.chars().filter(|character| !character.is_ascii_whitespace()).collect::<String>();
    lower.split_ascii_whitespace().any(|word| word == "hidden" || word.starts_with("hidden="))
        || compact.contains("aria-hidden=\"true\"") || compact.contains("aria-hidden='true'")
        || compact.contains("aria-hidden=true") || compact.contains("display:none")
        || compact.contains("visibility:hidden")
}

fn void_element(name: &str) -> bool {
    matches!(name, "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | "input" | "link" | "meta" | "param" | "source" | "track" | "wbr")
}

fn heading_inline_element(name: &str) -> bool {
    matches!(name, "span" | "em" | "strong" | "b" | "i" | "small" | "mark" | "code")
}

struct OpenElement { name: String, hidden: bool }
struct OpenCandidate { role: CandidateRole, start: usize, depth: usize, inline_tags: usize, supported: bool }

fn extract_html(text: &str, limit: usize) -> Vec<(CandidateRole, Range<usize>)> {
    let mut found = Vec::new();
    let mut stack: Vec<OpenElement> = Vec::new();
    let mut active: Option<OpenCandidate> = None;
    let mut cursor = 0;
    while cursor < text.len() && found.len() < limit {
        let Some(next) = text[cursor..].find('<') else { break };
        let start = cursor + next;
        if text[start..].starts_with("<!--") {
            let Some(end) = text[start + 4..].find("-->") else { break };
            if let Some(candidate) = &mut active { candidate.supported = false; }
            cursor = start + 4 + end + 3;
            continue;
        }
        if text[start..].starts_with("<!") || text[start..].starts_with("<?") {
            if let Some(candidate) = &mut active { candidate.supported = false; }
            let Some(end) = text[start..].find('>') else { break };
            cursor = start + end + 1;
            continue;
        }
        let Some(tag) = parse_tag(text, start) else {
            if let Some(candidate) = &mut active { candidate.supported = false; }
            cursor = start + 1;
            continue;
        };
        let name = tag.name.to_ascii_lowercase();
        cursor = tag.end;
        if tag.closing {
            if !stack.last().is_some_and(|open| open.name == name) {
                stack.clear();
                active = None;
                continue;
            }
            if let Some(candidate) = active.take() {
                if candidate.depth == stack.len() {
                    let range = candidate.start..start;
                    if candidate.supported && range.len() <= MAX_CANDIDATE_SOURCE_BYTES
                        && !text[range.clone()].trim().is_empty() {
                        found.push((candidate.role, range));
                    }
                } else {
                    active = Some(candidate);
                }
            }
            stack.pop();
            continue;
        }
        let hidden = hidden_attribute(tag.attributes);
        if let Some(candidate) = &mut active {
            candidate.inline_tags += 1;
            let supported_inline = candidate.role == CandidateRole::HeadingOne
                && heading_inline_element(&name) && !hidden && !tag.self_closing
                && stack.len().saturating_sub(candidate.depth) < MAX_HEADING_INLINE_DEPTH
                && candidate.inline_tags <= MAX_HEADING_INLINE_TAGS;
            if !supported_inline { candidate.supported = false; }
        }
        let excluded = stack.iter().any(|open| open.hidden || matches!(open.name.as_str(), "head" | "script" | "style" | "template" | "noscript" | "svg" | "textarea"));
        let role = match name.as_str() {
            "title" if !stack.iter().any(|open| open.hidden || matches!(open.name.as_str(), "body" | "script" | "style" | "template" | "noscript" | "svg" | "textarea")) => Some(CandidateRole::DocumentTitle),
            "h1" if !excluded => Some(CandidateRole::HeadingOne),
            "p" if !excluded => Some(CandidateRole::Paragraph),
            _ => None,
        };
        if active.is_none() && !hidden && !tag.self_closing {
            if let Some(role) = role {
                active = Some(OpenCandidate { role, start: tag.end, depth: stack.len() + 1, inline_tags: 0, supported: true });
            }
        }
        if !tag.self_closing && !void_element(&name) {
            stack.push(OpenElement { name: name.clone(), hidden });
        }
        if matches!(name.as_str(), "script" | "style" | "textarea") && !tag.self_closing {
            let close = format!("</{name}");
            let rest = &text[cursor..];
            if let Some(offset) = rest.as_bytes().windows(close.len()).position(|window| window.eq_ignore_ascii_case(close.as_bytes())) {
                cursor += offset;
            } else { break; }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    fn fixture(html: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("aiide-candidates-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("page.html"), html).unwrap();
        root.canonicalize().unwrap()
    }

    #[test]
    fn title_and_multiline_heading_keep_distinct_roles_and_exact_ranges() {
        let html = "<html>\n<head><title>Same\n text</title></head>\n<body><h1>Same\n text</h1><p>Body copy</p></body></html>";
        let root = fixture(html);
        let mut registry = CandidateRegistry::new();
        assert_eq!(registry.discover_html(&root, "page.html").unwrap(), 3);
        let views = registry.model_view();
        assert_eq!(views.iter().map(|view| view.role).collect::<Vec<_>>(), [CandidateRole::DocumentTitle, CandidateRole::HeadingOne, CandidateRole::Paragraph]);
        assert_eq!(views[0].excerpt, views[1].excerpt);
        assert_ne!(views[0].id, views[1].id);
        assert_eq!((views[0].start_line, views[0].end_line), (2, 3));
        assert_eq!((views[1].start_line, views[1].end_line), (4, 5));
        for (view, expected) in views.iter().zip(["Same\n text", "Same\n text", "Body copy"]) {
            let candidate = registry.candidate(&view.id).unwrap();
            assert_eq!(candidate.path(), "page.html");
            assert_eq!(&html[candidate.range()], expected);
            assert_eq!(candidate.original(), expected);
            assert_eq!(&registry.snapshot_for(&view.id).unwrap()[candidate.range()], expected);
            assert!(!candidate.description().is_empty());
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn multiline_heading_with_inline_span_keeps_exact_source_and_intro_paragraph() {
        let html = "<head><title>OrbitNote — Notes that stay out of your way</title></head>\n<body>\n<h1>\n  Notes that don't become\n  <span>another unfinished project.</span>\n</h1>\n<p class=\"hero-text\">\n  OrbitNote is a simple place to capture ideas, organise projects,\n  and pretend you definitely remember where you put that important\n  note from three weeks ago.\n</p>\n</body>";
        let root = fixture(html);
        let mut registry = CandidateRegistry::new();
        assert_eq!(registry.discover_html(&root, "page.html").unwrap(), 3);
        let views = registry.model_view();
        assert_eq!(views.iter().map(|view| view.role).collect::<Vec<_>>(), [CandidateRole::DocumentTitle, CandidateRole::HeadingOne, CandidateRole::Paragraph]);
        assert_ne!(views[0].id, views[1].id);
        for (view, original) in views.iter().zip([
            "OrbitNote — Notes that stay out of your way",
            "\n  Notes that don't become\n  <span>another unfinished project.</span>\n",
            "\n  OrbitNote is a simple place to capture ideas, organise projects,\n  and pretend you definitely remember where you put that important\n  note from three weeks ago.\n",
        ]) {
            let candidate = registry.candidate(&view.id).unwrap();
            let start = html.find(original).unwrap();
            assert_eq!(candidate.range(), start..start + original.len());
            assert_eq!(candidate.original(), original);
            assert_eq!(&registry.snapshot_for(&view.id).unwrap()[candidate.range()], original);
        }
        assert_eq!((views[0].start_line, views[1].start_line, views[2].start_line), (1, 3, 7));
        assert_eq!((views[0].role_index, views[1].role_index, views[2].role_index), (1, 1, 1));
        assert!(views[1].excerpt.contains("another unfinished project."));
        assert!(views[2].excerpt.contains("OrbitNote is a simple place"));
        assert!(views[2].before_context.contains("hero-text"));
        assert!(views.iter().all(|view| view.before_context.len() <= MAX_CANDIDATE_CONTEXT_BYTES));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unsupported_hidden_malformed_and_deep_heading_markup_stays_excluded() {
        let deep = format!("<h1>{}deep{}</h1>", "<span>".repeat(MAX_HEADING_INLINE_DEPTH + 1), "</span>".repeat(MAX_HEADING_INLINE_DEPTH + 1));
        let many = format!("<h1>{}</h1>", "<span>x</span>".repeat(MAX_HEADING_INLINE_TAGS + 1));
        let html = format!("<h1>safe <em>emphasis</em></h1><h1>link <a href=\"#\">text</a></h1><h1>hidden <span hidden>text</span></h1><h1>broken <span>text</h1>{deep}{many}");
        let found = extract_html(&html, MAX_CANDIDATES);
        assert_eq!(found.len(), 1);
        assert_eq!(&html[found[0].1.clone()], "safe <em>emphasis</em>");
    }

    #[test]
    fn repeated_text_has_separate_candidates_and_unknown_ids_fail_closed() {
        let html = "<h1>Repeat</h1><p>Repeat</p><p>Repeat</p>";
        let root = fixture(html);
        let mut registry = CandidateRegistry::new();
        assert_eq!(registry.discover_html(&root, "page.html").unwrap(), 3);
        let views = registry.model_view();
        assert_eq!(views.iter().map(|view| view.role).collect::<Vec<_>>(), [CandidateRole::HeadingOne, CandidateRole::Paragraph, CandidateRole::Paragraph]);
        assert_eq!(views.iter().map(|view| view.role_index).collect::<Vec<_>>(), [1, 1, 2]);
        assert!(views.windows(2).all(|pair| pair[0].id != pair[1].id));
        assert!(views.windows(2).all(|pair| registry.candidate(&pair[0].id).unwrap().range().start < registry.candidate(&pair[1].id).unwrap().range().start));
        assert!(registry.candidate("unknown").is_none());
        assert!(registry.snapshot_for("unknown").is_none());
        let mut fresh = CandidateRegistry::new();
        fresh.discover_html(&root, "page.html").unwrap();
        assert!(fresh.candidate(&views[0].id).is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unsupported_and_malformed_elements_are_skipped() {
        let html = "<!-- <h1>comment</h1> --><script>const x = '<h1>script</h1>';</script><div hidden><h1>hidden</h1></div><h1 aria-hidden = 'true'>hidden</h1><h1 style='display: none'>hidden</h1><h1>nested <em>word</em></h1><p>unclosed<h1>valid</h1><p>final</p>";
        let root = fixture(html);
        let mut registry = CandidateRegistry::new();
        assert_eq!(registry.discover_html(&root, "page.html").unwrap(), 1);
        let view = &registry.model_view()[0];
        assert_eq!(view.role, CandidateRole::HeadingOne);
        assert_eq!(registry.candidate(&view.id).unwrap().original(), "nested <em>word</em>");
        fs::remove_dir_all(root).unwrap();

        let html = "<h1>valid</h1><p>text <strong>inside</strong></p><p>final</p>";
        let root = fixture(html);
        let mut registry = CandidateRegistry::new();
        assert_eq!(registry.discover_html(&root, "page.html").unwrap(), 2);
        assert_eq!(registry.model_view().iter().map(|view| view.excerpt.as_str()).collect::<Vec<_>>(), ["valid", "final"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn offsets_and_context_respect_utf8_boundaries() {
        let html = "🌟<h1>café 🌍</h1>🍃<p>écho</p>";
        let root = fixture(html);
        let mut registry = CandidateRegistry::new();
        registry.discover_html(&root, "page.html").unwrap();
        let heading = registry.candidate(&registry.model_view()[0].id).unwrap();
        assert_eq!(heading.range().start, "🌟<h1>".len());
        assert_eq!(heading.range().end, "🌟<h1>café 🌍".len());
        assert_eq!(&html[heading.range()], "café 🌍");
        assert!(heading.before_context().ends_with("<h1>"));
        assert!(heading.after_context().starts_with("</h1>🍃"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn candidate_context_excerpts_and_counts_are_bounded() {
        let html = format!("{}<p>{}</p>{}", "<p>".to_owned() + &"é".repeat(3_000) + "</p>", "a".repeat(500), (0..MAX_CANDIDATES + 10).map(|index| format!("<p>item {index}</p>")).collect::<String>());
        let root = fixture(&html);
        let mut registry = CandidateRegistry::new();
        assert_eq!(registry.discover_html(&root, "page.html").unwrap(), MAX_CANDIDATES);
        assert_eq!(registry.model_view().len(), MAX_CANDIDATES);
        assert_eq!(registry.model_view()[0].excerpt.len(), MAX_CANDIDATE_EXCERPT_BYTES);
        assert!(registry.model_view()[0].excerpt.ends_with("..."));
        for view in registry.model_view() {
            let candidate = registry.candidate(&view.id).unwrap();
            assert!(candidate.before_context().len() <= MAX_CANDIDATE_CONTEXT_BYTES);
            assert!(candidate.after_context().len() <= MAX_CANDIDATE_CONTEXT_BYTES);
            assert!(view.excerpt.len() <= MAX_CANDIDATE_EXCERPT_BYTES);
        }
        assert_eq!(registry.discover_html(&root, "page.html").unwrap_err(), "File was already captured in this request.");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovery_reads_without_writing_and_enforces_path_safety() {
        let html = "<title>Read only</title><h1>Read only</h1>";
        let root = fixture(html);
        let path = root.join("page.html");
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions).unwrap();
        let mut registry = CandidateRegistry::new();
        assert_eq!(registry.discover_html(&root, "page.html").unwrap(), 2);
        assert_eq!(fs::read_to_string(&path).unwrap(), html);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        assert!(registry.discover_html(&root, "../page.html").is_err());
        assert!(registry.discover_html(&root, ".env.html").is_err());
        assert!(registry.discover_html(&root, "page.txt").is_err());
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(false);
        fs::set_permissions(&path, permissions).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
