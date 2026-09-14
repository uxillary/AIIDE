use serde::{Deserialize, Serialize};
use std::time::Duration;

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
pub struct ChatResponse { model: String, content: String }

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
pub async fn ollama_chat(model: String, messages: Vec<ChatMessage>) -> Result<ChatResponse, String> {
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
    let response = client.post(format!("{BASE}/api/chat"))
        .json(&ChatPayload { model: &model, messages: &messages, stream: false })
        .send().await.map_err(request_error)?;
    if !response.status().is_success() {
        return Err(match response.status().as_u16() {
            404 => "This model is no longer available. Retry to refresh the model list.",
            _ => "Ollama could not complete the chat request. Try again.",
        }.to_owned());
    }
    let result = response.json::<ChatPayloadResponse>().await.map_err(|_| "Ollama returned an invalid chat response.".to_owned())?;
    if !result.done || result.message.role != "assistant" || result.message.content.trim().is_empty() || result.model.is_empty() {
        return Err("Ollama returned an incomplete chat response.".to_owned());
    }
    Ok(ChatResponse { model: result.model, content: result.message.content })
}
