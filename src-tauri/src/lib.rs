mod project;
mod ollama;
mod repository;
mod model_profiles;
mod model_provider;
mod benchmark;
mod git;

pub fn run_benchmark_cli() -> Result<(), String> { benchmark::run_cli() }

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(project::OpenProject::default())
        .manage(repository::PendingChanges::default())
        .manage(ollama::AgentDebug::default())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![project::inspect_project, ollama::ollama_status, ollama::ollama_chat, ollama::set_agent_debug, ollama::agent_debug_status, ollama::latest_agent_trace, repository::apply_pending_change, repository::reject_pending_change, repository::view_repository_file, git::git_status, git::git_file_diff, git::git_stage_file, git::git_unstage_file, git::git_commit_preview, git::git_commit, git::git_history, git::git_commit_detail, git::git_suggest_commit_message])
        .run(tauri::generate_context!())
        .expect("failed to start AIIDE");
}
