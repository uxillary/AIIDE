use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::project::IGNORED;

pub const MAX_TOOL_CALLS: usize = 8;
pub const MAX_CONTEXT_BYTES: usize = 48_000;
const MAX_READ_BYTES: usize = 12_000;
const MAX_FILE_BYTES: u64 = 256_000;
const MAX_LIST: usize = 120;
const MAX_SEARCH: usize = 30;
const MAX_SCAN_FILES: usize = 2_000;

#[derive(Deserialize)]
pub struct ToolRequest {
    pub tool: String,
    #[serde(default)] pub path: String,
    #[serde(default)] pub query: String,
}

#[derive(Serialize)]
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

fn search_files(root: &Path, query: &str) -> Result<String, String> {
    if query.trim().is_empty() || query.len() > 120 { return Err("Search query must be 1–120 characters.".into()); }
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
    #[test] fn bounds() {
        let root = fixture();
        fs::write(root.join("many.txt"), "match\n".repeat(100)).unwrap();
        assert!(search_files(&root, "match").unwrap().contains("Results truncated"));
        for index in 0..MAX_LIST + 5 { fs::write(root.join(format!("{index}.txt")), "x").unwrap(); }
        assert!(list_files(&root, "").unwrap().contains("Listing truncated"));
        fs::remove_dir_all(root).unwrap();
    }
}
