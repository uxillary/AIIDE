mod project;
mod ollama;
mod repository;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(project::OpenProject::default())
        .manage(repository::PendingChanges::default())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![project::inspect_project, ollama::ollama_status, ollama::ollama_chat, repository::apply_pending_change, repository::reject_pending_change])
        .run(tauri::generate_context!())
        .expect("failed to start AIIDE");
}
