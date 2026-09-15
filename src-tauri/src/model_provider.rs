use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::time::Duration;

pub(crate) const OLLAMA_PROVIDER_ID: &str = "ollama";
pub(crate) const OPENROUTER_PROVIDER_ID: &str = "openrouter";
const OLLAMA_BASE: &str = "http://127.0.0.1:11434";
const OPENROUTER_BASE: &str = "https://openrouter.ai/api/v1";

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ProviderLocality { Local, Remote }

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ProviderErrorKind {
    MissingCredential,
    Authentication,
    RateLimited,
    InvalidModel,
    Transport,
    Api,
    MalformedResponse,
}

#[derive(Debug)]
pub(crate) struct ProviderFailure {
    pub kind: ProviderErrorKind,
    message: &'static str,
}

impl ProviderFailure {
    pub(crate) fn new(kind: ProviderErrorKind, message: &'static str) -> Self { Self { kind, message } }
}

impl fmt::Display for ProviderFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result { formatter.write_str(self.message) }
}

pub(crate) trait ModelProvider {
    fn metadata(&self) -> ProviderMetadata;
    async fn is_available(&self) -> bool;
    async fn installed_models(&self) -> Result<Vec<String>, ProviderFailure>;
    async fn infer(&self, request: InferenceRequest<'_>) -> Result<InferenceResponse, ProviderFailure>;
}

pub(crate) enum SelectedProvider {
    Ollama(OllamaProvider),
    OpenRouter(OpenRouterProvider),
}

impl SelectedProvider {
    pub fn from_id(id: &str, timeout: Duration) -> Result<Self, ProviderFailure> {
        match id {
            OLLAMA_PROVIDER_ID => OllamaProvider::new(timeout).map(Self::Ollama),
            OPENROUTER_PROVIDER_ID => OpenRouterProvider::from_env(timeout).map(Self::OpenRouter),
            _ => Err(ProviderFailure::new(ProviderErrorKind::Api, "Unknown model provider. Use 'ollama' or 'openrouter'.")),
        }
    }
}

impl ModelProvider for SelectedProvider {
    fn metadata(&self) -> ProviderMetadata {
        match self { Self::Ollama(provider) => provider.metadata(), Self::OpenRouter(provider) => provider.metadata() }
    }

    async fn is_available(&self) -> bool {
        match self { Self::Ollama(provider) => provider.is_available().await, Self::OpenRouter(provider) => provider.is_available().await }
    }

    async fn installed_models(&self) -> Result<Vec<String>, ProviderFailure> {
        match self { Self::Ollama(provider) => provider.installed_models().await, Self::OpenRouter(provider) => provider.installed_models().await }
    }

    async fn infer(&self, request: InferenceRequest<'_>) -> Result<InferenceResponse, ProviderFailure> {
        match self { Self::Ollama(provider) => provider.infer(request).await, Self::OpenRouter(provider) => provider.infer(request).await }
    }
}

pub(crate) struct OllamaProvider {
    client: reqwest::Client,
}

impl OllamaProvider {
    pub fn new(timeout: Duration) -> Result<Self, ProviderFailure> {
        let client = reqwest::Client::builder().no_proxy().timeout(timeout).build()
            .map_err(|_| ProviderFailure::new(ProviderErrorKind::Transport, "Could not prepare the local Ollama connection."))?;
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

fn ollama_request_error(error: reqwest::Error) -> ProviderFailure {
    if error.is_timeout() { ProviderFailure::new(ProviderErrorKind::Transport, "Ollama took too long to respond. Try again.") }
    else if error.is_connect() { ProviderFailure::new(ProviderErrorKind::Transport, "Ollama is offline. Start Ollama and retry.") }
    else { ProviderFailure::new(ProviderErrorKind::Transport, "The local Ollama request failed. Try again.") }
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

    async fn installed_models(&self) -> Result<Vec<String>, ProviderFailure> {
        let response = self.client.get(format!("{OLLAMA_BASE}/api/tags")).send().await.map_err(ollama_request_error)?;
        if !response.status().is_success() { return Err(ProviderFailure::new(ProviderErrorKind::Api, "Could not verify installed models. Retry the connection.")); }
        let tags = response.json::<TagsResponse>().await.map_err(|_| ProviderFailure::new(ProviderErrorKind::MalformedResponse, "Ollama returned an invalid model list."))?;
        Ok(tags.models.into_iter().map(|model| model.name).collect())
    }

    async fn infer(&self, request: InferenceRequest<'_>) -> Result<InferenceResponse, ProviderFailure> {
        let response = self.client.post(format!("{OLLAMA_BASE}/api/chat"))
            .json(&ChatPayload {
                model: request.model,
                messages: request.messages,
                stream: false,
                format: request.format,
                options: ChatOptions { temperature: request.temperature, num_predict: 2_048 },
            })
            .send().await.map_err(ollama_request_error)?;
        if !response.status().is_success() { return Err(ProviderFailure::new(ProviderErrorKind::Api, "Ollama could not complete the chat request. Try again.")); }
        let result = response.json::<ChatPayloadResponse>().await.map_err(|_| ProviderFailure::new(ProviderErrorKind::MalformedResponse, "Ollama returned an invalid chat response."))?;
        if !result.done || result.message.role != "assistant" || result.model.is_empty() {
            return Err(ProviderFailure::new(ProviderErrorKind::MalformedResponse, "Ollama returned an incomplete chat response."));
        }
        Ok(InferenceResponse { model: result.model, message: result.message, done: result.done, done_reason: result.done_reason })
    }
}

pub(crate) struct OpenRouterProvider {
    client: reqwest::Client,
    api_key: String,
}

impl OpenRouterProvider {
    fn from_env(timeout: Duration) -> Result<Self, ProviderFailure> {
        Self::from_optional_key(timeout, std::env::var("OPENROUTER_API_KEY").ok())
    }

    fn from_optional_key(timeout: Duration, api_key: Option<String>) -> Result<Self, ProviderFailure> {
        let api_key = api_key.filter(|value| !value.trim().is_empty()).ok_or_else(|| {
            ProviderFailure::new(ProviderErrorKind::MissingCredential, "OpenRouter requires the OPENROUTER_API_KEY environment variable.")
        })?;
        let client = reqwest::Client::builder().timeout(timeout).build()
            .map_err(|_| ProviderFailure::new(ProviderErrorKind::Transport, "Could not prepare the OpenRouter connection."))?;
        Ok(Self { client, api_key })
    }
}

#[derive(Deserialize)]
struct OpenRouterModels { data: Vec<OpenRouterModel> }

#[derive(Deserialize)]
struct OpenRouterModel { id: String }

#[derive(Serialize)]
struct OpenRouterPayload<'a> {
    model: &'a str,
    messages: &'a [ModelMessage],
    stream: bool,
    response_format: Value,
    temperature: f32,
    max_completion_tokens: u16,
}

#[derive(Deserialize)]
struct OpenRouterResponse {
    model: Option<String>,
    choices: Option<Vec<OpenRouterChoice>>,
    error: Option<OpenRouterError>,
}

#[derive(Deserialize)]
struct OpenRouterChoice {
    message: ModelMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenRouterError {
    code: u16,
    message: String,
}

fn openrouter_error(status: u16, message: Option<&str>) -> ProviderFailure {
    let message_mentions_model = message.is_some_and(|value| {
        let value = value.to_ascii_lowercase();
        value.contains("model") || value.contains("endpoint")
    });
    match status {
        401 => ProviderFailure::new(ProviderErrorKind::Authentication, "OpenRouter authentication failed. Check OPENROUTER_API_KEY."),
        403 => ProviderFailure::new(ProviderErrorKind::Api, "OpenRouter rejected the request."),
        429 => ProviderFailure::new(ProviderErrorKind::RateLimited, "OpenRouter rate limit reached. Try again later."),
        404 => ProviderFailure::new(ProviderErrorKind::InvalidModel, "The selected OpenRouter model is unavailable or invalid."),
        400 if message_mentions_model => ProviderFailure::new(ProviderErrorKind::InvalidModel, "The selected OpenRouter model is unavailable or invalid."),
        502 | 503 => ProviderFailure::new(ProviderErrorKind::InvalidModel, "The selected OpenRouter model is currently unavailable."),
        _ => ProviderFailure::new(ProviderErrorKind::Api, "OpenRouter could not complete the request. Try again."),
    }
}

fn openrouter_response_format(schema: Value) -> Value {
    serde_json::json!({
        "type": "json_schema",
        "json_schema": { "name": "aiide_agent_response", "strict": false, "schema": schema }
    })
}

fn normalize_openrouter_response(result: OpenRouterResponse) -> Result<InferenceResponse, ProviderFailure> {
    if let Some(error) = result.error {
        return Err(openrouter_error(error.code, Some(&error.message)));
    }
    let model = result.model.filter(|value| !value.is_empty()).ok_or_else(|| {
        ProviderFailure::new(ProviderErrorKind::MalformedResponse, "OpenRouter returned an invalid response.")
    })?;
    let mut choices = result.choices.ok_or_else(|| {
        ProviderFailure::new(ProviderErrorKind::MalformedResponse, "OpenRouter returned an invalid response.")
    })?;
    if choices.len() != 1 {
        return Err(ProviderFailure::new(ProviderErrorKind::MalformedResponse, "OpenRouter returned an invalid response."));
    }
    let choice = choices.remove(0);
    if choice.message.role != "assistant" || choice.message.content.is_empty() {
        return Err(ProviderFailure::new(ProviderErrorKind::MalformedResponse, "OpenRouter returned an invalid response."));
    }
    Ok(InferenceResponse {
        model,
        message: choice.message,
        done: true,
        done_reason: choice.finish_reason,
    })
}

impl ModelProvider for OpenRouterProvider {
    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata { id: OPENROUTER_PROVIDER_ID, locality: ProviderLocality::Remote }
    }

    async fn is_available(&self) -> bool { true }

    async fn installed_models(&self) -> Result<Vec<String>, ProviderFailure> {
        let response = self.client.get(format!("{OPENROUTER_BASE}/models"))
            .bearer_auth(&self.api_key).send().await
            .map_err(|_| ProviderFailure::new(ProviderErrorKind::Transport, "OpenRouter could not be reached. Try again."))?;
        let status = response.status().as_u16();
        if !response.status().is_success() { return Err(openrouter_error(status, None)); }
        let models = response.json::<OpenRouterModels>().await.map_err(|_| {
            ProviderFailure::new(ProviderErrorKind::MalformedResponse, "OpenRouter returned an invalid model list.")
        })?;
        Ok(models.data.into_iter().map(|model| model.id).collect())
    }

    async fn infer(&self, request: InferenceRequest<'_>) -> Result<InferenceResponse, ProviderFailure> {
        let response_format = openrouter_response_format(request.format);
        let response = self.client.post(format!("{OPENROUTER_BASE}/chat/completions"))
            .bearer_auth(&self.api_key)
            .json(&OpenRouterPayload {
                model: request.model,
                messages: request.messages,
                stream: false,
                response_format,
                temperature: request.temperature,
                max_completion_tokens: 2_048,
            })
            .send().await.map_err(|_| ProviderFailure::new(ProviderErrorKind::Transport, "OpenRouter could not be reached. Try again."))?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            let result = response.json::<OpenRouterResponse>().await.ok();
            return Err(openrouter_error(status, result.as_ref().and_then(|value| value.error.as_ref()).map(|error| error.message.as_str())));
        }
        let result = response.json::<OpenRouterResponse>().await.map_err(|_| {
            ProviderFailure::new(ProviderErrorKind::MalformedResponse, "OpenRouter returned an invalid response.")
        })?;
        normalize_openrouter_response(result)
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

    #[test]
    fn default_and_unknown_provider_selection_are_explicit() {
        assert!(matches!(SelectedProvider::from_id("ollama", Duration::from_secs(1)), Ok(SelectedProvider::Ollama(_))));
        let error = match SelectedProvider::from_id("unknown", Duration::from_secs(1)) {
            Err(error) => error,
            Ok(_) => panic!("unknown provider should fail"),
        };
        assert_eq!(error.to_string(), "Unknown model provider. Use 'ollama' or 'openrouter'.");
    }

    #[test]
    fn openrouter_requires_a_nonempty_api_key_without_leaking_it() {
        let missing = match OpenRouterProvider::from_optional_key(Duration::from_secs(1), None) {
            Err(error) => error,
            Ok(_) => panic!("missing key should fail"),
        };
        assert_eq!(missing.kind, ProviderErrorKind::MissingCredential);
        let secret = "sk-or-test-secret-that-must-not-leak";
        let provider = OpenRouterProvider::from_optional_key(Duration::from_secs(1), Some(secret.into())).unwrap();
        let error = openrouter_error(401, Some(secret));
        assert_eq!(error.kind, ProviderErrorKind::Authentication);
        assert!(!error.to_string().contains(secret));
        assert_eq!(provider.metadata(), ProviderMetadata { id: "openrouter", locality: ProviderLocality::Remote });
    }

    #[test]
    fn openrouter_response_is_normalized_for_the_agent_protocol() {
        let response = normalize_openrouter_response(OpenRouterResponse {
            model: Some("vendor/model:free".into()),
            choices: Some(vec![OpenRouterChoice {
                message: ModelMessage { role: "assistant".into(), content: r#"{"action":"answer","answer":"ok"}"#.into() },
                finish_reason: Some("stop".into()),
            }]),
            error: None,
        }).unwrap();
        assert_eq!(response.model, "vendor/model:free");
        assert_eq!(response.message.content, r#"{"action":"answer","answer":"ok"}"#);
        assert_eq!(response.done_reason.as_deref(), Some("stop"));

        let schema = serde_json::json!({"type":"object","required":["action"]});
        let format = openrouter_response_format(schema.clone());
        assert_eq!(format["type"], "json_schema");
        assert_eq!(format["json_schema"]["strict"], false);
        assert_eq!(format["json_schema"]["schema"], schema);
    }

    #[test]
    fn openrouter_errors_are_provider_neutral_and_safe() {
        assert_eq!(openrouter_error(429, None).kind, ProviderErrorKind::RateLimited);
        assert_eq!(openrouter_error(404, None).kind, ProviderErrorKind::InvalidModel);
        assert_eq!(openrouter_error(400, Some("No endpoints for model")).kind, ProviderErrorKind::InvalidModel);
        assert_eq!(openrouter_error(500, Some("private upstream detail")).kind, ProviderErrorKind::Api);
        assert!(!openrouter_error(500, Some("private upstream detail")).to_string().contains("private upstream detail"));
    }
}
