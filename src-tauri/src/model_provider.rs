use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

pub(crate) const OLLAMA_PROVIDER_ID: &str = "ollama";
const OLLAMA_BASE: &str = "http://127.0.0.1:11434";

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ProviderLocality { Local }

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ProviderMetadata {
    pub id: &'static str,
    pub locality: ProviderLocality,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ModelMessage {
    pub role: String,
    pub content: String,
}

pub(crate) struct InferenceRequest<'a> {
    pub model: &'a str,
    pub messages: &'a [ModelMessage],
    pub format: Value,
    pub temperature: f32,
}

pub(crate) struct InferenceResponse {
    pub model: String,
    pub message: ModelMessage,
    pub done: bool,
    pub done_reason: Option<String>,
}

pub(crate) trait ModelProvider {
    fn metadata(&self) -> ProviderMetadata;
    async fn is_available(&self) -> bool;
    async fn installed_models(&self) -> Result<Vec<String>, String>;
    async fn infer(&self, request: InferenceRequest<'_>) -> Result<InferenceResponse, String>;
}

pub(crate) struct OllamaProvider {
    client: reqwest::Client,
}

impl OllamaProvider {
    pub fn new(timeout: Duration) -> Result<Self, String> {
        let client = reqwest::Client::builder().no_proxy().timeout(timeout).build()
            .map_err(|_| "Could not prepare the local Ollama connection.".to_owned())?;
        Ok(Self { client })
    }
}

#[derive(Deserialize)]
struct VersionResponse { version: String }

#[derive(Deserialize)]
struct TagsResponse { models: Vec<TaggedModel> }

#[derive(Deserialize)]
struct TaggedModel { name: String }

#[derive(Serialize)]
struct ChatPayload<'a> {
    model: &'a str,
    messages: &'a [ModelMessage],
    stream: bool,
    format: Value,
    options: ChatOptions,
}

#[derive(Serialize)]
struct ChatOptions { temperature: f32, num_predict: u16 }

#[derive(Deserialize)]
struct ChatPayloadResponse {
    model: String,
    message: ModelMessage,
    done: bool,
    done_reason: Option<String>,
}

fn request_error(error: reqwest::Error) -> String {
    if error.is_timeout() { "Ollama took too long to respond. Try again.".to_owned() }
    else if error.is_connect() { "Ollama is offline. Start Ollama and retry.".to_owned() }
    else { "The local Ollama request failed. Try again.".to_owned() }
}

impl ModelProvider for OllamaProvider {
    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata { id: OLLAMA_PROVIDER_ID, locality: ProviderLocality::Local }
    }

    async fn is_available(&self) -> bool {
        let Ok(response) = self.client.get(format!("{OLLAMA_BASE}/api/version")).send().await else { return false };
        if !response.status().is_success() { return false; }
        response.json::<VersionResponse>().await.is_ok_and(|value| !value.version.is_empty())
    }

    async fn installed_models(&self) -> Result<Vec<String>, String> {
        let response = self.client.get(format!("{OLLAMA_BASE}/api/tags")).send().await.map_err(request_error)?;
        if !response.status().is_success() { return Err("Could not verify installed models. Retry the connection.".to_owned()); }
        let tags = response.json::<TagsResponse>().await.map_err(|_| "Ollama returned an invalid model list.".to_owned())?;
        Ok(tags.models.into_iter().map(|model| model.name).collect())
    }

    async fn infer(&self, request: InferenceRequest<'_>) -> Result<InferenceResponse, String> {
        let response = self.client.post(format!("{OLLAMA_BASE}/api/chat"))
            .json(&ChatPayload {
                model: request.model,
                messages: request.messages,
                stream: false,
                format: request.format,
                options: ChatOptions { temperature: request.temperature, num_predict: 2_048 },
            })
            .send().await.map_err(request_error)?;
        if !response.status().is_success() { return Err("Ollama could not complete the chat request. Try again.".into()); }
        let result = response.json::<ChatPayloadResponse>().await.map_err(|_| "Ollama returned an invalid chat response.".to_owned())?;
        if !result.done || result.message.role != "assistant" || result.model.is_empty() {
            return Err("Ollama returned an incomplete chat response.".into());
        }
        Ok(InferenceResponse { model: result.model, message: result.message, done: result.done, done_reason: result.done_reason })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ollama_provider_identifies_as_local() {
        let provider = OllamaProvider::new(Duration::from_secs(1)).unwrap();
        assert_eq!(provider.metadata(), ProviderMetadata { id: "ollama", locality: ProviderLocality::Local });
    }
}
