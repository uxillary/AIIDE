use serde::{Deserialize, Serialize};
use std::time::Duration;
use std::collections::HashSet;
use tauri::{Emitter, State};
use crate::project::OpenProject;
use crate::repository::{self, Activity, ToolRequest};

const BASE: &str = "http://127.0.0.1:11434";
const MAX_MESSAGES: usize = 40;
const MAX_MESSAGE_CHARS: usize = 12_000;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    state: &'static str,
    models: Vec<ModelInfo>,
    error: Option<ProviderError>,
}

#[derive(Serialize)]
pub struct ModelInfo { id: String, name: String }

#[derive(Serialize)]
pub struct ProviderError { code: &'static str, message: &'static str }

#[derive(Deserialize)]
struct VersionResponse { version: String }

#[derive(Deserialize)]
struct TagsResponse { models: Vec<TaggedModel> }

#[derive(Deserialize)]
struct TaggedModel { name: String }

#[derive(Deserialize, Serialize)]
pub struct ChatMessage { role: String, content: String }

#[derive(Serialize)]
struct ChatPayload<'a> { model: &'a str, messages: &'a [ChatMessage], stream: bool }

#[derive(Deserialize)]
struct ChatPayloadResponse { model: String, message: ChatMessage, done: bool }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatResponse { model: String, content: String, activity: Vec<Activity> }

const SYSTEM: &str = "You are Elma, a local-first AI coding companion inside AIIDE. Your underlying model is the selected Ollama model. You may inspect only the opened project through AIIDE's read-only tools. Never claim to have inspected a file unless its contents were returned by read_file. Do not invent files, code, Git state, tool results, or commands. You cannot modify files, execute commands, commit, or push. Inspect relevant context before repository-specific answers. Respond with exactly one JSON object: {\"tool\":\"list_files\",\"path\":\"\"}, {\"tool\":\"search_files\",\"query\":\"text\"}, {\"tool\":\"read_file\",\"path\":\"relative/path\"}, or {\"answer\":\"your answer\"}. Do not include markdown fences or other text outside the JSON object. When enough evidence is available, answer directly.";

#[derive(Deserialize)]
struct AgentReply { tool: Option<String>, path: Option<String>, query: Option<String>, answer: Option<String> }

fn client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder().no_proxy().timeout(timeout).build()
        .map_err(|_| "Could not prepare the local Ollama connection.".to_owned())
}

fn request_error(error: reqwest::Error) -> String {
    if error.is_timeout() { "Ollama took too long to respond. Try again.".to_owned() }
    else if error.is_connect() { "Ollama is offline. Start Ollama and retry.".to_owned() }
    else { "The local Ollama request failed. Try again.".to_owned() }
}

#[tauri::command]
pub async fn ollama_status() -> ProviderStatus {
    let offline = || ProviderStatus { state: "offline", models: vec![], error: Some(ProviderError { code: "unavailable", message: "Ollama not detected. Start Ollama and try again." }) };
    let Ok(client) = client(Duration::from_secs(4)) else { return offline() };
    let Ok(response) = client.get(format!("{BASE}/api/version")).send().await else { return offline() };
    if !response.status().is_success() { return offline(); }
    let Ok(version) = response.json::<VersionResponse>().await else { return offline() };
    if version.version.is_empty() { return offline(); }
    let Ok(response) = client.get(format!("{BASE}/api/tags")).send().await else {
        return ProviderStatus { state: "error", models: vec![], error: Some(ProviderError { code: "model_list_failed", message: "Ollama is connected, but its model list could not be loaded." }) };
    };
    if !response.status().is_success() {
        return ProviderStatus { state: "error", models: vec![], error: Some(ProviderError { code: "model_list_failed", message: "Ollama is connected, but its model list could not be loaded." }) };
    }
    match response.json::<TagsResponse>().await {
        Ok(tags) => ProviderStatus { state: "connected", models: tags.models.into_iter().filter(|model| !model.name.is_empty()).map(|model| ModelInfo { id: model.name.clone(), name: model.name }).collect(), error: None },
        Err(_) => ProviderStatus { state: "error", models: vec![], error: Some(ProviderError { code: "malformed_response", message: "Ollama returned an invalid model list." }) },
    }
}

#[tauri::command]
pub async fn ollama_chat(model: String, messages: Vec<ChatMessage>, open_project: State<'_, OpenProject>, app: tauri::AppHandle) -> Result<ChatResponse, String> {
    if model.is_empty() || messages.is_empty() || messages.len() > MAX_MESSAGES || messages.iter().any(|message| {
        !matches!(message.role.as_str(), "user" | "assistant") || message.content.is_empty() || message.content.chars().count() > MAX_MESSAGE_CHARS
    }) || messages.last().is_none_or(|message| message.role != "user") {
        return Err("The chat request is invalid or too long.".to_owned());
    }
    let client = client(Duration::from_secs(120))?;
    let tags = client.get(format!("{BASE}/api/tags")).send().await.map_err(request_error)?;
    if !tags.status().is_success() { return Err("Could not verify installed models. Retry the connection.".to_owned()); }
    let tags = tags.json::<TagsResponse>().await.map_err(|_| "Ollama returned an invalid model list.".to_owned())?;
    if !tags.models.iter().any(|item| item.name == model) { return Err("This model is no longer installed. Retry to refresh the model list.".to_owned()); }
    let root = open_project.0.lock().map_err(|_| "Project state unavailable")?.clone();
    let last_prompt = messages.last().map_or("", |message| message.content.trim());
    if last_prompt.eq_ignore_ascii_case("what is your name?") || last_prompt.eq_ignore_ascii_case("what is your name") {
        return Ok(ChatResponse { model, content: "I'm Elma, your local coding companion in AIIDE. My responses are generated by the selected Ollama model.".into(), activity: vec![] });
    }
    if last_prompt.eq_ignore_ascii_case("do you have access to my files?") || last_prompt.eq_ignore_ascii_case("do you have access to my files") {
        return Ok(ChatResponse { model, content: if root.is_some() { "I have controlled, read-only access to the currently opened project through AIIDE's repository tools. I cannot browse other folders or modify files." } else { "No project is open, so I cannot inspect your files. If you open a project, I can inspect it through controlled read-only tools." }.into(), activity: vec![] });
    }
    let mut exchange = Vec::new();
    exchange.push(ChatMessage { role: "system".into(), content: SYSTEM.into() });
    if let Some(ref root) = root {
        let info = super::project::inspect_metadata(root);
        exchange.push(ChatMessage { role: "system".into(), content: info });
    } else {
        exchange.push(ChatMessage { role: "system".into(), content: "No project is open. Repository tools are unavailable; answer normal chat directly.".into() });
    }
    exchange.extend(messages.into_iter().rev().take(12).collect::<Vec<_>>().into_iter().rev());
    let repository_question = root.is_some() && exchange.last().is_some_and(|message| {
        let prompt = message.content.to_ascii_lowercase();
        !prompt.contains("do you have access") && !prompt.contains("what is your name")
            && ["project", "repository", "website", "front-end", "frontend", "source code", "files", "codebase"].iter().any(|term| prompt.contains(term))
    });
    let mut activity = Vec::new();
    let mut seen = HashSet::new();
    let mut context_bytes = 0;
    for iteration in 0..=repository::MAX_TOOL_CALLS {
        let response = client.post(format!("{BASE}/api/chat"))
            .json(&ChatPayload { model: &model, messages: &exchange, stream: false })
            .send().await.map_err(request_error)?;
        if !response.status().is_success() { return Err("Ollama could not complete the chat request. Try again.".into()); }
        let result = response.json::<ChatPayloadResponse>().await.map_err(|_| "Ollama returned an invalid chat response.".to_owned())?;
        if !result.done || result.message.role != "assistant" || result.model.is_empty() { return Err("Ollama returned an incomplete chat response.".into()); }
        let reply: AgentReply = serde_json::from_str(result.message.content.trim()).map_err(|_| "The model did not return a valid structured response. Try again or choose another model.".to_owned())?;
        if let Some(answer) = reply.answer.filter(|answer| !answer.trim().is_empty()) {
            if repository_question && activity.is_empty() {
                if iteration == 0 {
                    exchange.push(result.message);
                    exchange.push(ChatMessage { role: "user".into(), content: "A project is open. Your answer must be based on actual project inspection. Request list_files, search_files, or read_file in the required JSON format before answering.".into() });
                    continue;
                }
                return Ok(ChatResponse { model: result.model, content: "I couldn't inspect the opened project, so I can't give a file-based answer. Please try again.".into(), activity });
            }
            return Ok(ChatResponse { model: result.model, content: answer, activity });
        }
        if root.is_none() { return Ok(ChatResponse { model: result.model, content: "Open a project to use repository tools.".into(), activity }); }
        if iteration == repository::MAX_TOOL_CALLS { break; }
        let request = ToolRequest { tool: reply.tool.unwrap_or_default(), path: reply.path.unwrap_or_default(), query: reply.query.unwrap_or_default() };
        let key = format!("{}|{}|{}", request.tool, request.path, request.query);
        let (output, event) = if seen.insert(key) { repository::execute(root.as_deref().unwrap(), &request) }
            else { ("This tool request was already answered in this turn; use the earlier result.".into(), Activity { label: "Repeated inspection skipped".into() }) };
        let _ = app.emit("repository-activity", &event);
        let remaining = repository::MAX_CONTEXT_BYTES.saturating_sub(context_bytes);
        if remaining == 0 { break; }
        let output = output.chars().take(remaining).collect::<String>();
        context_bytes += output.len();
        activity.push(event);
        exchange.push(result.message);
        exchange.push(ChatMessage { role: "user".into(), content: format!("Tool result for {}:\n{}\nContinue with another JSON tool request or final JSON answer.", request.tool, output) });
    }
    Ok(ChatResponse { model, content: "I reached the repository inspection limit for this request. Please ask a narrower question.".into(), activity })
}
