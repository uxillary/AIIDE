use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use tauri::State;
use crate::repository::PendingChanges;

const MAX_DEPTH: usize = 3;
const MAX_ENTRIES: usize = 500;
pub(crate) const IGNORED: &[&str] = &[".git", "node_modules", "dist", "build", ".next", ".cache", "target", "coverage"];

#[derive(Default)]
pub struct OpenProject(pub Mutex<Option<PathBuf>>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    name: String,
    path: String,
    repository: Option<RepositoryInfo>,
    tree: Vec<TreeEntry>,
    tree_truncated: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryInfo {
    name: String,
    branch: String,
    changed_files: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TreeEntry {
    name: String,
    relative_path: String,
    kind: &'static str,
    children: Option<Vec<TreeEntry>>,
    truncated: bool,
}

fn name_of(path: &Path) -> String {
    path.file_name().map(|part| part.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

fn git(path: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git").arg("-C").arg(path).args(args).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn repository(path: &Path) -> Option<RepositoryInfo> {
    let top = git(path, &["rev-parse", "--show-toplevel"])?;
    let top = fs::canonicalize(top).ok()?;
    // A chosen subfolder of a repository still belongs to that repository.
    let branch = git(path, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .or_else(|| git(path, &["rev-parse", "--short", "HEAD"]).map(|id| format!("Detached ({id})")))
        .unwrap_or_else(|| "No commits yet".to_owned());
    let status = git(&top, &["status", "--porcelain=v1", "--untracked-files=all"])?;
    Some(RepositoryInfo {
        name: name_of(&top),
        branch,
        changed_files: status.lines().count(),
    })
}

pub fn inspect_metadata(path: &Path) -> String {
    let repo = repository(path);
    format!("Opened project: {}. Branch: {}. Working tree: {}. Repository inspection is bounded; proposed changes require explicit review and Apply.",
        name_of(path), repo.as_ref().map_or("unknown", |r| r.branch.as_str()),
        repo.as_ref().map_or("unknown".into(), |r| if r.changed_files == 0 { "clean".into() } else { format!("{} changed files", r.changed_files) }))
}

fn read_tree(root: &Path, folder: &Path, depth: usize, remaining: &mut usize, any_truncated: &mut bool) -> Result<Vec<TreeEntry>, String> {
    let mut items: Vec<(PathBuf, bool)> = fs::read_dir(folder)
        .map_err(|error| format!("Cannot read {}: {error}", folder.display()))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name();
            if IGNORED.iter().any(|ignored| name.to_string_lossy().eq_ignore_ascii_case(ignored)) { return None; }
            let file_type = entry.file_type().ok()?;
            // Do not follow directory symlinks or junctions outside the chosen tree.
            if file_type.is_symlink() { return None; }
            Some((entry.path(), file_type.is_dir()))
        }).collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| name_of(&a.0).to_lowercase().cmp(&name_of(&b.0).to_lowercase())));
    let mut result = Vec::new();
    for (path, directory) in items {
        if *remaining == 0 { *any_truncated = true; break; }
        *remaining -= 1;
        let mut truncated = false;
        let children = if directory && depth < MAX_DEPTH {
            match read_tree(root, &path, depth + 1, remaining, any_truncated) {
                Ok(children) => Some(children),
                Err(_) => { truncated = true; None }
            }
        } else if directory { truncated = true; *any_truncated = true; None } else { None };
        result.push(TreeEntry {
            name: name_of(&path),
            relative_path: path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/"),
            kind: if directory { "directory" } else { "file" },
            children,
            truncated,
        });
    }
    Ok(result)
}

#[tauri::command]
pub fn inspect_project(path: String, open_project: State<'_, OpenProject>, pending: State<'_, PendingChanges>) -> Result<ProjectInfo, String> {
    let path = fs::canonicalize(path).map_err(|error| format!("Cannot open folder: {error}"))?;
    if !path.is_dir() { return Err("Selected path is not a folder".to_owned()); }
    let mut remaining = MAX_ENTRIES;
    let mut tree_truncated = false;
    let tree = read_tree(&path, &path, 0, &mut remaining, &mut tree_truncated)?;
    *open_project.0.lock().map_err(|_| "Project state unavailable")? = Some(path.clone());
    *pending.0.lock().map_err(|_| "Pending change state unavailable")? = None;
    Ok(ProjectInfo {
        name: name_of(&path),
        path: path.display().to_string(),
        repository: repository(&path),
        tree,
        tree_truncated,
    })
}
