use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{ipc::Response, Manager, State};

use crate::image_runtime::{
    managed_image_runtime_status, require_execution_gate, MANAGED_ACQUISITION_ENABLED,
};
use crate::project::{OpenProject, IGNORED};

const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:8188";
const DEFAULT_CHECKPOINT: &str = "sd_xl_base_1.0.safetensors";
const SDXL_BASELINE_MODEL_ID: &str = "sdxl-1.0-base";
const CONFIG_FILE: &str = "image-generation.json";
const MINIMUM_GUIDANCE_VRAM_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_PROMPT_CHARS: usize = 4_000;
const MAX_IMAGE_BYTES: usize = 25 * 1024 * 1024;
const MAX_SESSION_IMAGES: usize = 8;
const MAX_SAVED_PATHS: usize = 16;
const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug)]
struct ImageModelDefinition {
    id: &'static str,
    display_name: &'static str,
    architecture: &'static str,
    capabilities: &'static [&'static str],
    engine: &'static str,
    workflow_id: &'static str,
    checkpoint: &'static str,
    supporting_files: &'static [&'static str],
    required_nodes: &'static [&'static str],
    hardware_guidance: &'static str,
    license: &'static str,
    acquisition: &'static str,
}

const SDXL_REQUIRED_NODES: &[&str] = &[
    "KSampler",
    "CheckpointLoaderSimple",
    "EmptyLatentImage",
    "CLIPTextEncode",
    "VAEDecode",
    "PreviewImage",
];

const IMAGE_MODELS: &[ImageModelDefinition] = &[ImageModelDefinition {
    id: SDXL_BASELINE_MODEL_ID,
    display_name: "SDXL 1.0 Base",
    architecture: "SDXL",
    capabilities: &["text-to-image"],
    engine: "ComfyUI core workflow API",
    workflow_id: "comfyui-sdxl-base-v1",
    checkpoint: DEFAULT_CHECKPOINT,
    supporting_files: &[],
    required_nodes: SDXL_REQUIRED_NODES,
    hardware_guidance:
        "8 GB VRAM is the supported acceptance-test floor; hardware detection is advisory.",
    license: "CreativeML Open RAIL++-M",
    acquisition:
        "User-provided external ComfyUI checkpoint; AIIDE does not download it in Stage B.",
}];

fn model_definition(id: &str) -> Option<&'static ImageModelDefinition> {
    IMAGE_MODELS.iter().find(|model| model.id == id)
}

const IMAGE_CONFIGURATION_SCHEMA: u32 = 2;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ImageEngineOwnershipMode {
    External,
    Managed,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExternalImageEngineConfiguration {
    endpoint: String,
    model_id: String,
    checkpoint: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ManagedImageEngineConfiguration {
    installation_id: String,
    runtime_version: String,
    manifest_id: String,
    model_id: String,
    model_version: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImageGenerationConfiguration {
    schema_version: u32,
    ownership_mode: ImageEngineOwnershipMode,
    external: ExternalImageEngineConfiguration,
    #[serde(skip_serializing_if = "Option::is_none")]
    managed: Option<ManagedImageEngineConfiguration>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurrentImageGenerationConfiguration {
    schema_version: u32,
    ownership_mode: ImageEngineOwnershipMode,
    external: ExternalImageEngineConfiguration,
    managed: Option<ManagedImageEngineConfiguration>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyImageGenerationConfiguration {
    endpoint: String,
    model_id: String,
    checkpoint: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum PersistedImageGenerationConfiguration {
    Current(CurrentImageGenerationConfiguration),
    Legacy(LegacyImageGenerationConfiguration),
}

impl ImageGenerationConfiguration {
    fn defaults() -> Self {
        Self {
            schema_version: IMAGE_CONFIGURATION_SCHEMA,
            ownership_mode: ImageEngineOwnershipMode::External,
            external: ExternalImageEngineConfiguration {
                endpoint: DEFAULT_ENDPOINT.into(),
                model_id: SDXL_BASELINE_MODEL_ID.into(),
                checkpoint: DEFAULT_CHECKPOINT.into(),
            },
            managed: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum JobPhase {
    Submitting,
    Queued,
    Generating,
    Ready,
    Failed,
    Cancelled,
}

impl JobPhase {
    fn name(&self) -> &'static str {
        match self {
            Self::Submitting => "submitting",
            Self::Queued => "queued",
            Self::Generating => "generating",
            Self::Ready => "ready",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    fn active(&self) -> bool {
        matches!(self, Self::Submitting | Self::Queued | Self::Generating)
    }
}

#[derive(Clone, Debug)]
struct PendingImage {
    job_id: String,
    provider_prompt_id: Option<String>,
    prompt: String,
    seed: u64,
    model_id: String,
    model_display_name: String,
    checkpoint: String,
    phase: JobPhase,
    status_label: String,
    error: Option<String>,
    temporary_path: Option<PathBuf>,
    cancellation_supported: bool,
}

struct ImageStateData {
    next_job: u64,
    jobs: VecDeque<PendingImage>,
    saved_paths: VecDeque<PathBuf>,
}

impl Default for ImageStateData {
    fn default() -> Self {
        Self { next_job: 0, jobs: VecDeque::new(), saved_paths: VecDeque::new() }
    }
}

pub struct ImageGenerationState {
    inner: Mutex<ImageStateData>,
    temporary_root: PathBuf,
}

impl Default for ImageGenerationState {
    fn default() -> Self {
        let session = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
        Self {
            inner: Mutex::new(ImageStateData::default()),
            temporary_root: std::env::temp_dir()
                .join(format!("aiide-images-{}-{session}", std::process::id())),
        }
    }
}

impl Drop for ImageGenerationState {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.temporary_root);
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageJob {
    job_id: String,
    prompt: String,
    seed: u64,
    model_id: String,
    model_display_name: String,
    checkpoint: String,
    status: &'static str,
    status_label: String,
    preview_available: bool,
    cancellation_supported: bool,
    error: Option<String>,
}

impl From<&PendingImage> for ImageJob {
    fn from(job: &PendingImage) -> Self {
        Self {
            job_id: job.job_id.clone(),
            prompt: job.prompt.clone(),
            seed: job.seed,
            model_id: job.model_id.clone(),
            model_display_name: job.model_display_name.clone(),
            checkpoint: job.checkpoint.clone(),
            status: job.phase.name(),
            status_label: job.status_label.clone(),
            preview_available: job.temporary_path.is_some() && job.phase == JobPhase::Ready,
            cancellation_supported: job.cancellation_supported,
            error: job.error.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageModelSummary {
    id: &'static str,
    display_name: &'static str,
    architecture: &'static str,
    capabilities: &'static [&'static str],
    engine_requirement: &'static str,
    workflow_id: &'static str,
    checkpoint: &'static str,
    supporting_files: &'static [&'static str],
    required_nodes: &'static [&'static str],
    hardware_guidance: &'static str,
    license: &'static str,
    acquisition: &'static str,
}

impl From<&'static ImageModelDefinition> for ImageModelSummary {
    fn from(model: &'static ImageModelDefinition) -> Self {
        Self {
            id: model.id,
            display_name: model.display_name,
            architecture: model.architecture,
            capabilities: model.capabilities,
            engine_requirement: model.engine,
            workflow_id: model.workflow_id,
            checkpoint: model.checkpoint,
            supporting_files: model.supporting_files,
            required_nodes: model.required_nodes,
            hardware_guidance: model.hardware_guidance,
            license: model.license,
            acquisition: model.acquisition,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageEngineStatus {
    state: &'static str,
    ready: bool,
    ownership_mode: &'static str,
    managed_state: &'static str,
    managed_acquisition_enabled: bool,
    managed_message: Option<String>,
    endpoint: String,
    model: ImageModelSummary,
    checkpoint: String,
    checkpoints: Vec<ImageCheckpointSummary>,
    busy: bool,
    engine_status: &'static str,
    model_status: &'static str,
    hardware_status: &'static str,
    missing_nodes: Vec<String>,
    missing_files: Vec<String>,
    hardware_message: Option<String>,
    error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageCheckpointSummary {
    checkpoint: String,
    display_name: String,
    compatibility: &'static str,
    reason: String,
}

#[derive(Clone, Debug, PartialEq)]
enum ReadinessProbeError {
    Unavailable(String),
    Incompatible(String),
}

trait ImageReadinessProvider {
    async fn system_stats(&self) -> Result<Value, ReadinessProbeError>;
    async fn object_info(&self) -> Result<Value, ReadinessProbeError>;
    async fn queue_info(&self) -> Result<Value, ReadinessProbeError>;
}

#[derive(Clone, Debug, PartialEq)]
struct ProviderImageRef {
    filename: String,
    subfolder: String,
    folder_type: String,
}

#[derive(Clone, Debug, PartialEq)]
enum ProviderJobState {
    Queued,
    Running,
    Complete(ProviderImageRef),
    Failed(String),
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ProviderCancellation {
    Cancelled,
    Unsupported,
    NotFound,
}

trait ImageGenerationProvider {
    async fn available(&self) -> Result<bool, String>;
    async fn submit(&self, workflow: Value, client_id: &str) -> Result<String, String>;
    async fn poll(&self, prompt_id: &str) -> Result<ProviderJobState, String>;
    async fn fetch(&self, image: &ProviderImageRef) -> Result<Vec<u8>, String>;
    async fn cancel(&self, prompt_id: &str) -> Result<ProviderCancellation, String>;
}

struct ComfyUiProvider {
    client: Client,
    endpoint: Url,
}

impl ComfyUiProvider {
    fn new(endpoint: &str, timeout: Duration) -> Result<Self, String> {
        let mut endpoint = Url::parse(endpoint)
            .map_err(|_| "The configured ComfyUI endpoint is invalid.".to_owned())?;
        let host_is_loopback = endpoint.host_str().is_some_and(|host| {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|address| address.is_loopback())
        });
        if endpoint.scheme() != "http"
            || !host_is_loopback
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err("ComfyUI must use a local HTTP endpoint without credentials, query parameters, or fragments.".into());
        }
        if endpoint.path() != "/" && !endpoint.path().is_empty() {
            return Err("The ComfyUI endpoint must not include a path.".into());
        }
        endpoint.set_path("/");
        let client = Client::builder()
            .no_proxy()
            .timeout(timeout)
            .build()
            .map_err(|_| "Could not prepare the ComfyUI connection.".to_owned())?;
        Ok(Self { client, endpoint })
    }

    fn url(&self, path: &str) -> Result<Url, String> {
        self.endpoint
            .join(path)
            .map_err(|_| "Could not prepare the ComfyUI request.".to_owned())
    }

    async fn queue_state(&self, prompt_id: &str) -> Result<Option<ProviderJobState>, String> {
        let response = self
            .client
            .get(self.url("queue")?)
            .send()
            .await
            .map_err(comfy_transport_error)?;
        if !response.status().is_success() {
            return Err("ComfyUI could not report its generation queue.".into());
        }
        let value: Value = response
            .json()
            .await
            .map_err(|_| "ComfyUI returned an invalid queue response.".to_owned())?;
        if queue_contains(value.get("queue_running"), prompt_id) {
            return Ok(Some(ProviderJobState::Running));
        }
        if queue_contains(value.get("queue_pending"), prompt_id) {
            return Ok(Some(ProviderJobState::Queued));
        }
        Ok(None)
    }

    async fn readiness_json(&self, path: &str) -> Result<Value, ReadinessProbeError> {
        let url = self.url(path).map_err(ReadinessProbeError::Incompatible)?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|error| ReadinessProbeError::Unavailable(comfy_transport_error(error)))?;
        if !response.status().is_success() {
            return Err(ReadinessProbeError::Incompatible(format!(
                "ComfyUI does not provide the required /{path} API."
            )));
        }
        response.json().await.map_err(|_| {
            ReadinessProbeError::Incompatible(format!(
                "ComfyUI returned a malformed /{path} response."
            ))
        })
    }
}

impl ImageReadinessProvider for ComfyUiProvider {
    async fn system_stats(&self) -> Result<Value, ReadinessProbeError> {
        self.readiness_json("system_stats").await
    }

    async fn object_info(&self) -> Result<Value, ReadinessProbeError> {
        self.readiness_json("object_info").await
    }

    async fn queue_info(&self) -> Result<Value, ReadinessProbeError> {
        self.readiness_json("queue").await
    }
}

fn comfy_transport_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "ComfyUI took too long to respond.".into()
    } else if error.is_connect() {
        "ComfyUI is unavailable at the configured local endpoint.".into()
    } else {
        "The local ComfyUI request failed.".into()
    }
}

fn queue_contains(queue: Option<&Value>, prompt_id: &str) -> bool {
    queue.and_then(Value::as_array).is_some_and(|items| {
        items.iter().any(|item| {
            item.as_array()
                .and_then(|parts| parts.get(1))
                .and_then(Value::as_str)
                == Some(prompt_id)
        })
    })
}

fn execution_failure(entry: &Value) -> Option<String> {
    let messages = entry.get("status")?.get("messages")?.as_array()?;
    messages.iter().find_map(|message| {
        let parts = message.as_array()?;
        let kind = parts.first()?.as_str()?;
        if kind == "execution_interrupted" {
            return Some("cancelled".into());
        }
        if kind != "execution_error" {
            return None;
        }
        let detail = parts.get(1)?;
        Some(
            detail
                .get("exception_message")
                .and_then(Value::as_str)
                .or_else(|| detail.get("exception_type").and_then(Value::as_str))
                .unwrap_or("ComfyUI could not complete the workflow.")
                .to_owned(),
        )
    })
}

fn map_execution_error(detail: &str) -> String {
    let lower = detail.to_ascii_lowercase();
    if lower.contains("out of memory") || lower.contains("cuda") && lower.contains("alloc") {
        "ComfyUI ran out of GPU memory. The 8 GB baseline is tight; close other GPU workloads or start ComfyUI with --lowvram, then try again.".into()
    } else {
        "ComfyUI could not complete the image workflow. Check its console for details and try again.".into()
    }
}

fn workflow_submission_error(body: &Value) -> String {
    let detail = body.to_string().to_ascii_lowercase();
    if detail.contains("ckpt_name")
        || detail.contains("checkpoint")
        || detail.contains("not in list")
    {
        "The configured SDXL checkpoint is not installed in ComfyUI. Add the required file to the ComfyUI models\\checkpoints folder, then retry.".into()
    } else {
        "This ComfyUI installation rejected the required core SDXL workflow. Update ComfyUI and verify its core nodes.".into()
    }
}

fn image_from_history(entry: &Value) -> Option<ProviderImageRef> {
    entry
        .get("outputs")?
        .as_object()?
        .values()
        .find_map(|output| {
            output.get("images")?.as_array()?.iter().find_map(|image| {
                Some(ProviderImageRef {
                    filename: image.get("filename")?.as_str()?.to_owned(),
                    subfolder: image
                        .get("subfolder")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    folder_type: image.get("type")?.as_str()?.to_owned(),
                })
            })
        })
}

impl ImageGenerationProvider for ComfyUiProvider {
    async fn available(&self) -> Result<bool, String> {
        let response = self
            .client
            .get(self.url("system_stats")?)
            .send()
            .await
            .map_err(comfy_transport_error)?;
        Ok(response.status().is_success())
    }

    async fn submit(&self, workflow: Value, client_id: &str) -> Result<String, String> {
        let response = self
            .client
            .post(self.url("prompt")?)
            .json(&json!({ "prompt": workflow, "client_id": client_id }))
            .send()
            .await
            .map_err(comfy_transport_error)?;
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .map_err(|_| "ComfyUI returned an invalid workflow response.".to_owned())?;
        if !status.is_success()
            || !body
                .get("node_errors")
                .is_none_or(|errors| errors.as_object().is_some_and(|value| value.is_empty()))
        {
            return Err(workflow_submission_error(&body));
        }
        body.get("prompt_id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| "ComfyUI did not return a generation job ID.".to_owned())
    }

    async fn poll(&self, prompt_id: &str) -> Result<ProviderJobState, String> {
        let response = self
            .client
            .get(self.url(&format!("history/{prompt_id}"))?)
            .send()
            .await
            .map_err(comfy_transport_error)?;
        if !response.status().is_success() {
            return Err("ComfyUI could not report generation history.".into());
        }
        let history: Value = response
            .json()
            .await
            .map_err(|_| "ComfyUI returned an invalid generation history.".to_owned())?;
        if let Some(entry) = history.get(prompt_id) {
            if let Some(error) = execution_failure(entry) {
                return Ok(if error == "cancelled" {
                    ProviderJobState::Cancelled
                } else {
                    ProviderJobState::Failed(map_execution_error(&error))
                });
            }
            if let Some(image) = image_from_history(entry) {
                return Ok(ProviderJobState::Complete(image));
            }
            if entry
                .get("status")
                .and_then(|status| status.get("completed"))
                .and_then(Value::as_bool)
                == Some(true)
            {
                return Ok(ProviderJobState::Failed(
                    "ComfyUI completed the workflow without a preview image.".into(),
                ));
            }
        }
        Ok(self
            .queue_state(prompt_id)
            .await?
            .unwrap_or(ProviderJobState::Queued))
    }

    async fn fetch(&self, image: &ProviderImageRef) -> Result<Vec<u8>, String> {
        let safe_subfolder = image.subfolder.is_empty()
            || Path::new(&image.subfolder)
                .components()
                .all(|part| matches!(part, Component::Normal(_)));
        if image.folder_type != "temp"
            || image.filename.is_empty()
            || image.filename.contains('/')
            || image.filename.contains('\\')
            || image.filename.contains("..")
            || image.subfolder.contains('\\')
            || !safe_subfolder
        {
            return Err("ComfyUI returned an unsafe preview reference.".into());
        }
        let response = self
            .client
            .get(self.url("view")?)
            .query(&[
                ("filename", image.filename.as_str()),
                ("subfolder", image.subfolder.as_str()),
                ("type", image.folder_type.as_str()),
            ])
            .send()
            .await
            .map_err(comfy_transport_error)?;
        if !response.status().is_success() {
            return Err("ComfyUI could not provide the generated preview.".into());
        }
        if !response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("image/png"))
        {
            return Err(
                "ComfyUI returned an unexpected preview format; M08A accepts PNG only.".into(),
            );
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| "Could not read the ComfyUI preview.".to_owned())?;
        validate_png(&bytes)?;
        Ok(bytes.to_vec())
    }

    async fn cancel(&self, prompt_id: &str) -> Result<ProviderCancellation, String> {
        let targeted = self
            .client
            .post(self.url(&format!("api/jobs/{prompt_id}/cancel"))?)
            .send()
            .await
            .map_err(comfy_transport_error)?;
        if targeted.status().is_success() {
            let value: Value = targeted.json().await.unwrap_or(Value::Null);
            return Ok(
                if value.get("cancelled").and_then(Value::as_bool) == Some(true) {
                    ProviderCancellation::Cancelled
                } else {
                    ProviderCancellation::NotFound
                },
            );
        }
        if targeted.status() != StatusCode::NOT_FOUND {
            return Err("ComfyUI could not cancel this generation.".into());
        }
        match self.queue_state(prompt_id).await? {
            Some(ProviderJobState::Queued) => {
                let response = self
                    .client
                    .post(self.url("queue")?)
                    .json(&json!({ "delete": [prompt_id] }))
                    .send()
                    .await
                    .map_err(comfy_transport_error)?;
                if response.status().is_success() {
                    Ok(ProviderCancellation::Cancelled)
                } else {
                    Err("ComfyUI could not remove the queued generation.".into())
                }
            }
            Some(ProviderJobState::Running) => Ok(ProviderCancellation::Unsupported),
            _ => Ok(ProviderCancellation::NotFound),
        }
    }
}

fn validate_checkpoint(checkpoint: &str) -> Result<(), String> {
    if checkpoint.trim().is_empty()
        || checkpoint.contains("..")
        || checkpoint.contains(':')
        || checkpoint.starts_with(['/', '\\'])
    {
        return Err("The configured checkpoint must be a ComfyUI-relative filename.".into());
    }
    Ok(())
}

fn validate_configuration(
    mut configuration: ImageGenerationConfiguration,
) -> Result<ImageGenerationConfiguration, String> {
    if configuration.schema_version != IMAGE_CONFIGURATION_SCHEMA {
        return Err("The saved image-generation configuration schema is not supported.".into());
    }
    ComfyUiProvider::new(&configuration.external.endpoint, Duration::from_secs(1))?;
    if model_definition(&configuration.external.model_id).is_none() {
        return Err("The selected image model is not supported by this AIIDE build.".into());
    }
    validate_checkpoint(&configuration.external.checkpoint)?;
    if let Some(managed) = &configuration.managed {
        if managed.installation_id.trim().is_empty()
            || managed.runtime_version.trim().is_empty()
            || managed.manifest_id.trim().is_empty()
            || model_definition(&managed.model_id).is_none()
            || managed.model_version.trim().is_empty()
        {
            return Err("The saved managed image-engine selection is invalid.".into());
        }
    }
    configuration.schema_version = IMAGE_CONFIGURATION_SCHEMA;
    Ok(configuration)
}

fn external_configuration(
    configuration: &ImageGenerationConfiguration,
) -> Result<&ExternalImageEngineConfiguration, String> {
    if configuration.ownership_mode == ImageEngineOwnershipMode::Managed {
        require_execution_gate()?;
        return Err("Managed image-engine execution is not available in this build.".into());
    }
    Ok(&configuration.external)
}

fn environment_configuration() -> Result<ImageGenerationConfiguration, String> {
    let mut configuration = ImageGenerationConfiguration::defaults();
    if let Ok(endpoint) = std::env::var("AIIDE_COMFYUI_ENDPOINT") {
        configuration.external.endpoint = endpoint;
    }
    if let Ok(checkpoint) = std::env::var("AIIDE_COMFYUI_CHECKPOINT") {
        configuration.external.checkpoint = checkpoint;
    }
    validate_configuration(configuration)
}

fn load_configuration(path: &Path) -> Result<ImageGenerationConfiguration, String> {
    if !path.exists() {
        return environment_configuration();
    }
    let text = fs::read_to_string(path)
        .map_err(|_| "Could not read the saved image-generation configuration.".to_owned())?;
    let persisted: PersistedImageGenerationConfiguration = serde_json::from_str(&text)
        .map_err(|_| "The saved image-generation configuration is malformed.".to_owned())?;
    let configuration = match persisted {
        PersistedImageGenerationConfiguration::Current(current) => ImageGenerationConfiguration {
            schema_version: current.schema_version,
            ownership_mode: current.ownership_mode,
            external: current.external,
            managed: current.managed,
        },
        PersistedImageGenerationConfiguration::Legacy(legacy) => ImageGenerationConfiguration {
            schema_version: IMAGE_CONFIGURATION_SCHEMA,
            ownership_mode: ImageEngineOwnershipMode::External,
            external: ExternalImageEngineConfiguration {
                endpoint: legacy.endpoint,
                model_id: legacy.model_id,
                checkpoint: legacy.checkpoint,
            },
            managed: None,
        },
    };
    validate_configuration(configuration)
}

fn persist_configuration(
    path: &Path,
    endpoint: String,
    model_id: String,
    checkpoint: Option<String>,
) -> Result<ImageGenerationConfiguration, String> {
    let model = model_definition(&model_id)
        .ok_or("The selected image model is not supported by this AIIDE build.")?;
    let provider = ComfyUiProvider::new(endpoint.trim(), Duration::from_secs(1))?;
    let mut configuration = if path.exists() {
        load_configuration(path)?
    } else {
        ImageGenerationConfiguration::defaults()
    };
    configuration.ownership_mode = ImageEngineOwnershipMode::External;
    let checkpoint = checkpoint.unwrap_or_else(|| model.checkpoint.into());
    if checkpoint != model.checkpoint {
        return Err("This checkpoint has unknown or incompatible architecture metadata and cannot be used with AIIDE's fixed SDXL workflow.".into());
    }
    configuration.external = ExternalImageEngineConfiguration {
        endpoint: provider.endpoint.as_str().trim_end_matches('/').to_owned(),
        model_id,
        checkpoint,
    };
    let configuration = validate_configuration(configuration)?;
    let parent = path
        .parent()
        .ok_or("Could not locate the application configuration directory.")?;
    fs::create_dir_all(parent)
        .map_err(|_| "Could not prepare the application configuration directory.".to_owned())?;
    let contents = serde_json::to_vec_pretty(&configuration)
        .map_err(|_| "Could not encode the image-generation configuration.".to_owned())?;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(|_| "Could not save the image-generation configuration.".to_owned())?;
    file.write_all(&contents)
        .map_err(|_| "Could not save the image-generation configuration.".to_owned())?;
    Ok(configuration)
}

fn configuration_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|directory| directory.join(CONFIG_FILE))
        .map_err(|_| "Could not locate the application configuration directory.".to_owned())
}

fn selected_configuration(app: &tauri::AppHandle) -> Result<ImageGenerationConfiguration, String> {
    load_configuration(&configuration_path(app)?)
}

fn hardware_diagnostics(system_stats: &Value) -> (&'static str, Option<String>) {
    let Some(devices) = system_stats.get("devices").and_then(Value::as_array) else {
        return (
            "unavailable",
            Some("ComfyUI did not report usable GPU memory information. Hardware checks are advisory; a real generation is still required.".into()),
        );
    };
    let maximum = devices
        .iter()
        .filter(|device| {
            device
                .get("type")
                .and_then(Value::as_str)
                .is_none_or(|kind| !kind.eq_ignore_ascii_case("cpu"))
        })
        .filter_map(|device| device.get("vram_total").and_then(Value::as_u64))
        .max();
    match maximum {
        Some(bytes) if bytes < MINIMUM_GUIDANCE_VRAM_BYTES => (
            "potentially_insufficient",
            Some(format!(
                "ComfyUI reports {:.1} GB VRAM; the SDXL acceptance baseline is 8 GB. Detection is advisory.",
                bytes as f64 / 1024_f64.powi(3)
            )),
        ),
        Some(_) => (
            "sufficient",
            Some("Reported GPU memory meets the 8 GB SDXL acceptance-test floor; this does not prove generation will succeed.".into()),
        ),
        None => (
            "unavailable",
            Some("ComfyUI did not report usable GPU memory information. Hardware checks are advisory; a real generation is still required.".into()),
        ),
    }
}

fn probe_failure_status(
    configuration: &ExternalImageEngineConfiguration,
    model: &'static ImageModelDefinition,
    local_busy: bool,
    failure: ReadinessProbeError,
) -> ImageEngineStatus {
    let (state, engine_status, error) = match failure {
        ReadinessProbeError::Unavailable(error) => ("unavailable", "unavailable", error),
        ReadinessProbeError::Incompatible(error) => ("incompatible", "incompatible", error),
    };
    ImageEngineStatus {
        state,
        ready: false,
        ownership_mode: "external",
        managed_state: "disabled",
        managed_acquisition_enabled: MANAGED_ACQUISITION_ENABLED,
        managed_message: Some(managed_image_runtime_status().message.to_owned()),
        endpoint: configuration.endpoint.clone(),
        model: model.into(),
        checkpoint: configuration.checkpoint.clone(),
        checkpoints: vec![],
        busy: local_busy,
        engine_status,
        model_status: "unknown",
        hardware_status: "unavailable",
        missing_nodes: vec![],
        missing_files: vec![],
        hardware_message: None,
        error: Some(error),
    }
}

fn malformed_probe(message: &str) -> ReadinessProbeError {
    ReadinessProbeError::Incompatible(message.into())
}

fn checkpoint_summaries(checkpoints: &[Value], selected: &str) -> Vec<ImageCheckpointSummary> {
    checkpoints
        .iter()
        .filter_map(Value::as_str)
        .map(|checkpoint| {
            let compatible = checkpoint == DEFAULT_CHECKPOINT;
            ImageCheckpointSummary {
                checkpoint: checkpoint.to_owned(),
                display_name: if compatible { "SDXL 1.0 Base".into() } else { checkpoint.to_owned() },
                compatibility: if compatible { "compatible" } else { "unknown" },
                reason: if compatible {
                    "Verified against AIIDE's fixed SDXL Base 1.0 workflow.".into()
                } else {
                    "ComfyUI does not expose reliable architecture metadata for this checkpoint, so AIIDE will not submit it to the SDXL workflow.".into()
                },
            }
        })
        .chain((!checkpoints.iter().any(|value| value.as_str() == Some(selected))).then(|| ImageCheckpointSummary {
            checkpoint: selected.to_owned(),
            display_name: selected.to_owned(),
            compatibility: "unavailable",
            reason: "The selected checkpoint is not installed in this ComfyUI instance.".into(),
        }))
        .collect()
}

async fn readiness_with_provider(
    provider: &impl ImageReadinessProvider,
    configuration: &ExternalImageEngineConfiguration,
    local_busy: bool,
) -> ImageEngineStatus {
    let model = model_definition(&configuration.model_id)
        .expect("validated image model configuration must reference the registry");
    let system_stats = match provider.system_stats().await {
        Ok(value)
            if value.as_object().is_some() && value.get("system").is_some_and(Value::is_object) =>
        {
            value
        }
        Ok(_) => {
            return probe_failure_status(
                configuration,
                model,
                local_busy,
                malformed_probe("ComfyUI returned a malformed /system_stats response."),
            )
        }
        Err(error) => return probe_failure_status(configuration, model, local_busy, error),
    };
    let (hardware_status, hardware_message) = hardware_diagnostics(&system_stats);
    let object_info = match provider.object_info().await {
        Ok(value) if value.as_object().is_some() => value,
        Ok(_) => {
            return probe_failure_status(
                configuration,
                model,
                local_busy,
                malformed_probe("ComfyUI returned a malformed /object_info response."),
            )
        }
        Err(error) => return probe_failure_status(configuration, model, local_busy, error),
    };
    let queue = match provider.queue_info().await {
        Ok(value) => value,
        Err(error) => return probe_failure_status(configuration, model, local_busy, error),
    };
    let Some(running) = queue.get("queue_running").and_then(Value::as_array) else {
        return probe_failure_status(
            configuration,
            model,
            local_busy,
            malformed_probe("ComfyUI returned a malformed /queue response."),
        );
    };
    let Some(pending) = queue.get("queue_pending").and_then(Value::as_array) else {
        return probe_failure_status(
            configuration,
            model,
            local_busy,
            malformed_probe("ComfyUI returned a malformed /queue response."),
        );
    };
    let busy = local_busy || !running.is_empty() || !pending.is_empty();
    let missing_nodes: Vec<String> = model
        .required_nodes
        .iter()
        .filter(|node| object_info.get(**node).is_none())
        .map(|node| (*node).to_owned())
        .collect();
    if !missing_nodes.is_empty() {
        return ImageEngineStatus {
            state: "missing_nodes",
            ready: false,
            ownership_mode: "external",
            managed_state: "disabled",
            managed_acquisition_enabled: MANAGED_ACQUISITION_ENABLED,
            managed_message: Some(managed_image_runtime_status().message.to_owned()),
            endpoint: configuration.endpoint.clone(),
            model: model.into(),
            checkpoint: configuration.checkpoint.clone(),
            checkpoints: vec![],
            busy,
            engine_status: if busy { "busy" } else { "reachable" },
            model_status: "missing_nodes",
            hardware_status,
            missing_nodes,
            missing_files: vec![],
            hardware_message,
            error: Some("This ComfyUI installation is missing nodes required by the selected SDXL workflow.".into()),
        };
    }
    let Some(checkpoints) = object_info
        .get("CheckpointLoaderSimple")
        .and_then(|node| node.get("input"))
        .and_then(|input| input.get("required"))
        .and_then(|required| required.get("ckpt_name"))
        .and_then(Value::as_array)
        .and_then(|definition| definition.first())
        .and_then(Value::as_array)
    else {
        return probe_failure_status(
            configuration,
            model,
            busy,
            malformed_probe("ComfyUI's checkpoint API is incompatible with this AIIDE build."),
        );
    };
    let checkpoint_options = checkpoint_summaries(checkpoints, &configuration.checkpoint);
    let checkpoint_installed = checkpoints
        .iter()
        .filter_map(Value::as_str)
        .any(|checkpoint| checkpoint == configuration.checkpoint);
    if !checkpoint_installed {
        return ImageEngineStatus {
            state: "missing_checkpoint",
            ready: false,
            ownership_mode: "external",
            managed_state: "disabled",
            managed_acquisition_enabled: MANAGED_ACQUISITION_ENABLED,
            managed_message: Some(managed_image_runtime_status().message.to_owned()),
            endpoint: configuration.endpoint.clone(),
            model: model.into(),
            checkpoint: configuration.checkpoint.clone(),
            checkpoints: checkpoint_options,
            busy,
            engine_status: if busy { "busy" } else { "reachable" },
            model_status: "missing_checkpoint",
            hardware_status,
            missing_nodes: vec![],
            missing_files: vec![configuration.checkpoint.clone()],
            hardware_message,
            error: Some("The selected SDXL checkpoint is not installed in ComfyUI.".into()),
        };
    }
    if configuration.checkpoint != model.checkpoint {
        return ImageEngineStatus {
            state: "incompatible",
            ready: false,
            ownership_mode: "external",
            managed_state: "disabled",
            managed_acquisition_enabled: MANAGED_ACQUISITION_ENABLED,
            managed_message: Some(managed_image_runtime_status().message.to_owned()),
            endpoint: configuration.endpoint.clone(),
            model: model.into(),
            checkpoint: configuration.checkpoint.clone(),
            checkpoints: checkpoint_options,
            busy,
            engine_status: if busy { "busy" } else { "reachable" },
            model_status: "unknown",
            hardware_status,
            missing_nodes: vec![],
            missing_files: vec![],
            hardware_message,
            error: Some("ComfyUI does not expose reliable architecture metadata for this checkpoint. AIIDE will only submit the verified SDXL Base 1.0 checkpoint to this workflow.".into()),
        };
    }
    ImageEngineStatus {
        state: if busy { "busy" } else { "ready" },
        ready: true,
        ownership_mode: "external",
        managed_state: "disabled",
        managed_acquisition_enabled: MANAGED_ACQUISITION_ENABLED,
        managed_message: Some(managed_image_runtime_status().message.to_owned()),
        endpoint: configuration.endpoint.clone(),
        model: model.into(),
        checkpoint: configuration.checkpoint.clone(),
        checkpoints: checkpoint_options,
        busy,
        engine_status: if busy { "busy" } else { "reachable" },
        model_status: "ready",
        hardware_status,
        missing_nodes: vec![],
        missing_files: vec![],
        hardware_message,
        error: if busy {
            Some("ComfyUI is busy with another generation. Retry when its queue is clear.".into())
        } else {
            None
        },
    }
}

fn managed_disabled_status(configuration: &ImageGenerationConfiguration) -> ImageEngineStatus {
    let model_id = configuration
        .managed
        .as_ref()
        .map(|managed| managed.model_id.as_str())
        .unwrap_or(configuration.external.model_id.as_str());
    let model = model_definition(model_id).unwrap_or_else(|| {
        model_definition(SDXL_BASELINE_MODEL_ID).expect("baseline image model must exist")
    });
    let managed = managed_image_runtime_status();
    ImageEngineStatus {
        state: "managed_not_installed",
        ready: false,
        ownership_mode: "managed",
        managed_state: managed.state,
        managed_acquisition_enabled: managed.acquisition_enabled,
        managed_message: Some(managed.message.into()),
        endpoint: String::new(),
        model: model.into(),
        checkpoint: model.checkpoint.into(),
        checkpoints: vec![],
        busy: false,
        engine_status: "unavailable",
        model_status: "unknown",
        hardware_status: "unavailable",
        missing_nodes: vec![],
        missing_files: vec![],
        hardware_message: None,
        error: Some(managed.message.into()),
    }
}

fn sdxl_workflow(prompt: &str, checkpoint: &str, seed: u64) -> Value {
    json!({
        "3": { "class_type": "KSampler", "inputs": {
            "seed": seed, "steps": 30, "cfg": 7.0, "sampler_name": "dpmpp_2m", "scheduler": "karras", "denoise": 1.0,
            "model": ["4", 0], "positive": ["6", 0], "negative": ["7", 0], "latent_image": ["5", 0]
        }},
        "4": { "class_type": "CheckpointLoaderSimple", "inputs": { "ckpt_name": checkpoint }},
        "5": { "class_type": "EmptyLatentImage", "inputs": { "width": 1024, "height": 1024, "batch_size": 1 }},
        "6": { "class_type": "CLIPTextEncode", "inputs": { "text": prompt, "clip": ["4", 1] }},
        "7": { "class_type": "CLIPTextEncode", "inputs": { "text": "", "clip": ["4", 1] }},
        "8": { "class_type": "VAEDecode", "inputs": { "samples": ["3", 0], "vae": ["4", 2] }},
        "9": { "class_type": "PreviewImage", "inputs": { "images": ["8", 0] }}
    })
}

fn workflow_for_model(
    model: &ImageModelDefinition,
    prompt: &str,
    checkpoint: &str,
    seed: u64,
) -> Value {
    match model.workflow_id {
        "comfyui-sdxl-base-v1" => sdxl_workflow(prompt, checkpoint, seed),
        _ => unreachable!("registered image models must have an implemented workflow"),
    }
}

fn next_seed() -> u64 {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| value.as_nanos() as u64);
    time ^ NEXT_SESSION.fetch_add(1, Ordering::Relaxed)
}

fn validate_prompt(prompt: &str) -> Result<String, String> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err("Describe the image you want to generate.".into());
    }
    if prompt.chars().count() > MAX_PROMPT_CHARS {
        return Err(format!(
            "Image prompts are limited to {MAX_PROMPT_CHARS} characters."
        ));
    }
    Ok(prompt.to_owned())
}

fn validate_png(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err("The generated preview exceeds the 25 MB safety limit.".into());
    }
    if !bytes.starts_with(PNG_SIGNATURE) {
        return Err("ComfyUI returned an invalid PNG preview.".into());
    }
    Ok(())
}

fn release_failed_reservation(state: &ImageGenerationState, job_id: &str) {
    if let Ok(mut inner) = state.inner.lock() {
        if let Some(index) = inner.jobs.iter().position(|job| job.job_id == job_id) {
            inner.jobs.remove(index);
        }
    }
}

async fn start_with_provider(
    provider: &impl ImageGenerationProvider,
    state: &ImageGenerationState,
    prompt: String,
    model: &ImageModelDefinition,
    checkpoint: &str,
) -> Result<ImageJob, String> {
    let prompt = validate_prompt(&prompt)?;
    let seed = next_seed();
    let job_id = {
        let mut inner = state
            .inner
            .lock()
            .map_err(|_| "Image generation state unavailable")?;
        if inner.jobs.iter().any(|job| job.phase.active()) {
            return Err(
                "Finish or cancel the active image generation before starting another.".into(),
            );
        }
        inner.next_job += 1;
        let job_id = format!("image-{}", inner.next_job);
        inner.jobs.push_back(PendingImage {
            job_id: job_id.clone(),
            provider_prompt_id: None,
            prompt: prompt.clone(),
            seed,
            model_id: model.id.into(),
            model_display_name: model.display_name.into(),
            checkpoint: checkpoint.into(),
            phase: JobPhase::Submitting,
            status_label: "Submitting to ComfyUI…".into(),
            error: None,
            temporary_path: None,
            cancellation_supported: true,
        });
        job_id
    };
    let available = provider.available().await;
    match available {
        Ok(true) => {}
        Ok(false) => {
            release_failed_reservation(state, &job_id);
            return Err("ComfyUI is unavailable at the configured local endpoint.".into());
        }
        Err(error) => {
            release_failed_reservation(state, &job_id);
            return Err(error);
        }
    }
    let prompt_id = match provider
        .submit(
            workflow_for_model(model, &prompt, checkpoint, seed),
            &job_id,
        )
        .await
    {
        Ok(id) => id,
        Err(error) => {
            release_failed_reservation(state, &job_id);
            return Err(error);
        }
    };
    let mut inner = state
        .inner
        .lock()
        .map_err(|_| "Image generation state unavailable")?;
    let job = inner.jobs.iter_mut()
        .find(|job| job.job_id == job_id)
        .ok_or("Image generation state changed unexpectedly.")?;
    job.provider_prompt_id = Some(prompt_id);
    job.phase = JobPhase::Queued;
    job.status_label = "Queued in ComfyUI".into();
    let result = ImageJob::from(&*job);
    let evicted_path = if inner.jobs.len() > MAX_SESSION_IMAGES {
        inner.jobs.pop_front().and_then(|job| job.temporary_path)
    } else {
        None
    };
    drop(inner);
    if let Some(path) = evicted_path { let _ = fs::remove_file(path); }
    Ok(result)
}

fn write_temporary_image(
    state: &ImageGenerationState,
    job_id: &str,
    bytes: &[u8],
) -> Result<PathBuf, String> {
    validate_png(bytes)?;
    fs::create_dir_all(&state.temporary_root)
        .map_err(|_| "Could not prepare temporary image storage.".to_owned())?;
    let path = state.temporary_root.join(format!("{job_id}.png"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|_| "Could not store the generated preview temporarily.".to_owned())?;
    if file.write_all(bytes).is_err() {
        drop(file);
        let _ = fs::remove_file(&path);
        return Err("Could not store the generated preview temporarily.".into());
    }
    Ok(path)
}

fn fail_active_job(
    state: &ImageGenerationState,
    job_id: &str,
    error: String,
) -> Result<ImageJob, String> {
    let mut inner = state
        .inner
        .lock()
        .map_err(|_| "Image generation state unavailable")?;
    let job = inner.jobs.iter_mut()
        .find(|job| job.job_id == job_id)
        .ok_or("This image generation is no longer available.")?;
    if job.phase.active() {
        job.phase = JobPhase::Failed;
        job.status_label = "Generation failed".into();
        job.error = Some(error);
    }
    Ok(ImageJob::from(&*job))
}

async fn refresh_with_provider(
    provider: &impl ImageGenerationProvider,
    state: &ImageGenerationState,
    job_id: &str,
) -> Result<ImageJob, String> {
    let snapshot = {
        let inner = state
            .inner
            .lock()
            .map_err(|_| "Image generation state unavailable")?;
        inner.jobs.iter()
            .find(|job| job.job_id == job_id)
            .cloned()
            .ok_or("This image generation is no longer available.")?
    };
    if !snapshot.phase.active() {
        return Ok(ImageJob::from(&snapshot));
    }
    let prompt_id = snapshot
        .provider_prompt_id
        .as_deref()
        .ok_or("The image generation has not been submitted yet.")?;
    let provider_state = provider.poll(prompt_id).await?;
    let downloaded = if let ProviderJobState::Complete(image) = &provider_state {
        match provider.fetch(image).await {
            Ok(bytes) => Some(bytes),
            Err(error) => return fail_active_job(state, job_id, error),
        }
    } else {
        None
    };
    let temporary_path = if let Some(bytes) = downloaded.as_deref() {
        match write_temporary_image(state, job_id, bytes) {
            Ok(path) => Some(path),
            Err(error) => return fail_active_job(state, job_id, error),
        }
    } else {
        None
    };
    let mut inner = state
        .inner
        .lock()
        .map_err(|_| "Image generation state unavailable")?;
    let job = inner.jobs.iter_mut()
        .find(|job| job.job_id == job_id)
        .ok_or("This image generation is no longer available.")?;
    if !job.phase.active() {
        if let Some(path) = temporary_path {
            let _ = fs::remove_file(path);
        }
        return Ok(ImageJob::from(&*job));
    }
    match provider_state {
        ProviderJobState::Queued => {
            job.phase = JobPhase::Queued;
            job.status_label = "Queued in ComfyUI".into();
        }
        ProviderJobState::Running => {
            job.phase = JobPhase::Generating;
            job.status_label = "Generating in ComfyUI".into();
        }
        ProviderJobState::Complete(_) => {
            job.phase = JobPhase::Ready;
            job.status_label = "Preview ready · not saved".into();
            job.temporary_path = temporary_path;
        }
        ProviderJobState::Failed(error) => {
            job.phase = JobPhase::Failed;
            job.status_label = "Generation failed".into();
            job.error = Some(error);
        }
        ProviderJobState::Cancelled => {
            job.phase = JobPhase::Cancelled;
            job.status_label = "Generation cancelled".into();
        }
    }
    if job.phase != snapshot.phase {
        eprintln!("[AIIDE][image] {job_id} -> {}", job.phase.name());
    }
    Ok(ImageJob::from(&*job))
}

async fn cancel_with_provider(
    provider: &impl ImageGenerationProvider,
    state: &ImageGenerationState,
    job_id: &str,
) -> Result<ImageJob, String> {
    let prompt_id = {
        let inner = state
            .inner
            .lock()
            .map_err(|_| "Image generation state unavailable")?;
        let job = inner.jobs.iter()
            .find(|job| job.job_id == job_id)
            .ok_or("This image generation is no longer available.")?;
        if !job.phase.active() {
            return Ok(ImageJob::from(job));
        }
        job.provider_prompt_id
            .clone()
            .ok_or("The image generation has not been submitted yet.")?
    };
    let result = provider.cancel(&prompt_id).await?;
    let mut inner = state
        .inner
        .lock()
        .map_err(|_| "Image generation state unavailable")?;
    let job = inner.jobs.iter_mut()
        .find(|job| job.job_id == job_id)
        .ok_or("This image generation is no longer available.")?;
    match result {
        ProviderCancellation::Cancelled => {
            job.phase = JobPhase::Cancelled;
            job.status_label = "Generation cancelled".into();
        }
        ProviderCancellation::Unsupported => {
            job.cancellation_supported = false;
            job.error = Some("This ComfyUI version cannot safely cancel a running job. AIIDE will not use the global interrupt; the generation will continue.".into());
        }
        ProviderCancellation::NotFound => {
            job.error = Some(
                "ComfyUI no longer reports this job. Its final state will be checked again.".into(),
            );
        }
    }
    Ok(ImageJob::from(&*job))
}

#[tauri::command]
pub async fn image_generation_status(
    state: State<'_, ImageGenerationState>,
    app: tauri::AppHandle,
) -> Result<ImageEngineStatus, String> {
    let configuration = selected_configuration(&app)?;
    let local_busy = state
        .inner
        .lock()
        .ok()
        .map(|inner| inner.jobs.iter().any(|job| job.phase.active()))
        .unwrap_or(false);
    if configuration.ownership_mode == ImageEngineOwnershipMode::Managed {
        return Ok(managed_disabled_status(&configuration));
    }
    let external = external_configuration(&configuration)?;
    let provider = ComfyUiProvider::new(&external.endpoint, Duration::from_secs(4))?;
    Ok(readiness_with_provider(&provider, external, local_busy).await)
}

#[tauri::command]
pub async fn configure_image_generation(
    endpoint: String,
    model_id: String,
    checkpoint: Option<String>,
    state: State<'_, ImageGenerationState>,
    app: tauri::AppHandle,
) -> Result<ImageEngineStatus, String> {
    let local_busy = state
        .inner
        .lock()
        .map_err(|_| "Image generation state unavailable")?
        .jobs
        .iter()
        .any(|job| job.phase.active());
    if local_busy {
        return Err(
            "Cancel or finish the active image generation before changing its connection.".into(),
        );
    }
    let configuration = persist_configuration(&configuration_path(&app)?, endpoint, model_id, checkpoint)?;
    let external = external_configuration(&configuration)?;
    let provider = ComfyUiProvider::new(&external.endpoint, Duration::from_secs(4))?;
    Ok(readiness_with_provider(&provider, external, false).await)
}

#[tauri::command]
pub async fn start_image_generation(
    prompt: String,
    state: State<'_, ImageGenerationState>,
    app: tauri::AppHandle,
) -> Result<ImageJob, String> {
    let configuration = selected_configuration(&app)?;
    let external = external_configuration(&configuration)?;
    let model = model_definition(&external.model_id)
        .ok_or("The selected image model is not supported by this AIIDE build.")?;
    let provider = ComfyUiProvider::new(&external.endpoint, Duration::from_secs(20))?;
    let readiness = readiness_with_provider(&provider, external, false).await;
    if !readiness.ready {
        return Err(readiness
            .error
            .unwrap_or_else(|| "Image generation is not ready.".into()));
    }
    if readiness.busy {
        return Err(
            "ComfyUI is busy with another generation. Retry when its queue is clear.".into(),
        );
    }
    let result = start_with_provider(&provider, &state, prompt, model, &external.checkpoint).await;
    if let Ok(job) = &result {
        eprintln!("[AIIDE][image] submitted {}", job.job_id);
    }
    result
}

#[tauri::command]
pub async fn regenerate_image_generation(
    job_id: String,
    state: State<'_, ImageGenerationState>,
    app: tauri::AppHandle,
) -> Result<ImageJob, String> {
    let (prompt, model_id, checkpoint) = {
        let inner = state.inner.lock().map_err(|_| "Image generation state unavailable")?;
        let original = inner.jobs.iter().find(|job| job.job_id == job_id)
            .ok_or("This image generation is no longer available.")?;
        if original.phase.active() {
            return Err("Wait for the current generation to finish before regenerating it.".into());
        }
        (original.prompt.clone(), original.model_id.clone(), original.checkpoint.clone())
    };
    let configuration = selected_configuration(&app)?;
    let external = external_configuration(&configuration)?;
    let model = model_definition(&model_id)
        .ok_or("The original image model is no longer supported by this AIIDE build.")?;
    if checkpoint != model.checkpoint {
        return Err("The original checkpoint is no longer verified for AIIDE's SDXL workflow.".into());
    }
    let provider = ComfyUiProvider::new(&external.endpoint, Duration::from_secs(20))?;
    let original_configuration = ExternalImageEngineConfiguration {
        endpoint: external.endpoint.clone(), model_id, checkpoint: checkpoint.clone(),
    };
    let readiness = readiness_with_provider(&provider, &original_configuration, false).await;
    if !readiness.ready || readiness.busy {
        return Err(readiness.error.unwrap_or_else(|| "Image generation is not ready.".into()));
    }
    start_with_provider(&provider, &state, prompt, model, &checkpoint).await
}

#[tauri::command]
pub async fn get_image_generation(
    job_id: String,
    state: State<'_, ImageGenerationState>,
    app: tauri::AppHandle,
) -> Result<ImageJob, String> {
    let configuration = selected_configuration(&app)?;
    let external = external_configuration(&configuration)?;
    let provider = ComfyUiProvider::new(&external.endpoint, Duration::from_secs(30))?;
    refresh_with_provider(&provider, &state, &job_id).await
}

#[tauri::command]
pub async fn cancel_image_generation(
    job_id: String,
    state: State<'_, ImageGenerationState>,
    app: tauri::AppHandle,
) -> Result<ImageJob, String> {
    let configuration = selected_configuration(&app)?;
    let external = external_configuration(&configuration)?;
    let provider = ComfyUiProvider::new(&external.endpoint, Duration::from_secs(10))?;
    let result = cancel_with_provider(&provider, &state, &job_id).await;
    if let Ok(job) = &result {
        eprintln!(
            "[AIIDE][image] cancellation result for {job_id}: {}",
            job.status
        );
    }
    result
}

#[tauri::command]
pub fn get_image_preview(
    job_id: String,
    state: State<'_, ImageGenerationState>,
) -> Result<Response, String> {
    let path = {
        let inner = state
            .inner
            .lock()
            .map_err(|_| "Image generation state unavailable")?;
        let job = inner.jobs.iter()
            .find(|job| job.job_id == job_id)
            .ok_or("This image generation is no longer available.")?;
        if job.phase != JobPhase::Ready {
            return Err("The image preview is not ready.".into());
        }
        job.temporary_path
            .clone()
            .ok_or("The image preview is unavailable.")?
    };
    let bytes =
        fs::read(path).map_err(|_| "The temporary image preview is unavailable.".to_owned())?;
    validate_png(&bytes)?;
    Ok(Response::new(bytes))
}

fn protected(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        name == ".env"
            || name.starts_with(".env.")
            || name.ends_with(".pem")
            || name.ends_with(".key")
            || name.contains("credential")
            || matches!(
                name.as_str(),
                "id_rsa" | "id_ed25519" | ".ssh" | ".aws" | ".azure" | ".npmrc" | ".pypirc"
            )
    })
}

fn validate_asset_path(path: &str) -> Result<PathBuf, String> {
    let candidate = Path::new(path);
    if path.trim().is_empty()
        || candidate.is_absolute()
        || path.starts_with("\\\\")
        || path.contains(':')
        || path.contains('\\')
        || candidate
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("Choose a project-relative PNG path.".into());
    }
    if candidate
        .extension()
        .and_then(|value| value.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("png"))
    {
        return Err("Generated images must be saved with a .png extension.".into());
    }
    if protected(candidate) {
        return Err("Protected paths are unavailable.".into());
    }
    if candidate.components().any(|part| {
        IGNORED.iter().any(|ignored| {
            part.as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(ignored)
        })
    }) {
        return Err("Generated or ignored paths are unavailable.".into());
    }
    Ok(candidate.to_path_buf())
}

fn ensure_safe_parent(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    let canonical_root =
        fs::canonicalize(root).map_err(|_| "The opened project is unavailable.".to_owned())?;
    let parent = relative.parent().unwrap_or_else(|| Path::new(""));
    let mut current = canonical_root.clone();
    for part in parent.components() {
        let Component::Normal(name) = part else {
            return Err("Choose a project-relative PNG path.".into());
        };
        current.push(name);
        if current.exists() {
            let metadata = fs::symlink_metadata(&current)
                .map_err(|_| "Could not inspect the image target directory.".to_owned())?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err("Image target directories must be real project folders.".into());
            }
            let resolved = fs::canonicalize(&current)
                .map_err(|_| "Could not inspect the image target directory.".to_owned())?;
            if !resolved.starts_with(&canonical_root) {
                return Err("The image target escapes the opened project.".into());
            }
        } else {
            fs::create_dir(&current)
                .map_err(|_| "Could not create the image target directory.".to_owned())?;
        }
    }
    Ok(canonical_root.join(relative))
}

fn collision_candidate(target: &Path, index: usize) -> PathBuf {
    if index == 1 {
        return target.to_path_buf();
    }
    let stem = target
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("image");
    target.with_file_name(format!("{stem}-{index}.png"))
}

fn save_bytes_to_project(root: &Path, relative_path: &str, bytes: &[u8]) -> Result<String, String> {
    validate_png(bytes)?;
    let relative = validate_asset_path(relative_path)?;
    let target = ensure_safe_parent(root, &relative)?;
    for index in 1..=10_000 {
        let candidate = collision_candidate(&target, index);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut file) => {
                if let Err(error) = file.write_all(bytes) {
                    let _ = fs::remove_file(&candidate);
                    return Err(format!("Could not save the approved image: {error}"));
                }
                return candidate
                    .strip_prefix(fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf()))
                    .map(|path| path.to_string_lossy().replace('\\', "/"))
                    .map_err(|_| "The saved image path is invalid.".to_owned());
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err("Could not create the approved image file.".into()),
        }
    }
    Err("Could not find an available filename for the approved image.".into())
}

fn save_bytes_to_absolute(path: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    validate_png(bytes)?;
    if !path.is_absolute() || path.extension().and_then(|value| value.to_str()).is_none_or(|extension| !extension.eq_ignore_ascii_case("png")) {
        return Err("Choose an absolute .png destination from the Save As dialog.".into());
    }
    if protected(path) {
        return Err("Protected paths are unavailable.".into());
    }
    let parent = path.parent().ok_or("The selected destination has no parent folder.")?;
    let metadata = fs::symlink_metadata(parent).map_err(|_| "The selected destination folder is unavailable.".to_owned())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("The selected destination must be inside a real folder.".into());
    }
    for index in 1..=10_000 {
        let candidate = collision_candidate(path, index);
        match OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(bytes) {
                    let _ = fs::remove_file(&candidate);
                    return Err(format!("Could not save the approved image: {error}"));
                }
                return Ok(candidate);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err("Could not create the approved image file.".into()),
        }
    }
    Err("Could not find an available filename for the approved image.".into())
}

fn remember_saved_path(state: &ImageGenerationState, path: PathBuf) {
    if let Ok(mut inner) = state.inner.lock() {
        if inner.saved_paths.len() >= MAX_SAVED_PATHS { inner.saved_paths.pop_front(); }
        inner.saved_paths.push_back(path);
    }
}

fn save_ready_image(
    state: &ImageGenerationState,
    root: &Path,
    job_id: &str,
    relative_path: &str,
) -> Result<String, String> {
    let temporary_path = {
        let inner = state
            .inner
            .lock()
            .map_err(|_| "Image generation state unavailable")?;
        let job = inner.jobs.iter()
            .find(|job| job.job_id == job_id)
            .ok_or("This image generation is no longer available.")?;
        if job.phase != JobPhase::Ready {
            return Err("Only a ready preview can be saved.".into());
        }
        job.temporary_path
            .clone()
            .ok_or("The image preview is unavailable.")?
    };
    let bytes = fs::read(&temporary_path)
        .map_err(|_| "The temporary image preview is unavailable.".to_owned())?;
    let saved = save_bytes_to_project(root, relative_path, &bytes)?;
    let removed = {
        let mut inner = state
            .inner
            .lock()
            .map_err(|_| "Image generation state unavailable")?;
        inner.jobs.iter().position(|job| job.job_id == job_id).and_then(|index| inner.jobs.remove(index))
    };
    if let Some(path) = removed.and_then(|job| job.temporary_path) {
        let _ = fs::remove_file(path);
    }
    Ok(saved)
}

#[tauri::command]
pub fn save_generated_image(
    job_id: String,
    relative_path: String,
    open_project: State<'_, OpenProject>,
    state: State<'_, ImageGenerationState>,
) -> Result<String, String> {
    let root = open_project
        .0
        .lock()
        .map_err(|_| "Project state unavailable")?
        .clone()
        .ok_or("Open a project before saving an image.")?;
    let saved = save_ready_image(&state, &root, &job_id, &relative_path)?;
    let absolute = root.join(&saved);
    remember_saved_path(&state, absolute.clone());
    let absolute = absolute.to_string_lossy().into_owned();
    eprintln!("[AIIDE][image] saved {job_id} as {absolute}");
    Ok(absolute)
}

#[tauri::command]
pub fn save_generated_image_as(
    job_id: String,
    absolute_path: String,
    state: State<'_, ImageGenerationState>,
) -> Result<String, String> {
    let temporary_path = {
        let inner = state.inner.lock().map_err(|_| "Image generation state unavailable")?;
        let job = inner.jobs.iter().find(|job| job.job_id == job_id)
            .ok_or("This image generation is no longer available.")?;
        if job.phase != JobPhase::Ready { return Err("Only a ready preview can be saved.".into()); }
        job.temporary_path.clone().ok_or("The image preview is unavailable.")?
    };
    let bytes = fs::read(&temporary_path).map_err(|_| "The temporary image preview is unavailable.".to_owned())?;
    let saved = save_bytes_to_absolute(Path::new(&absolute_path), &bytes)?;
    let removed = {
        let mut inner = state.inner.lock().map_err(|_| "Image generation state unavailable")?;
        inner.jobs.iter().position(|job| job.job_id == job_id).and_then(|index| inner.jobs.remove(index))
    };
    if let Some(path) = removed.and_then(|job| job.temporary_path) { let _ = fs::remove_file(path); }
    remember_saved_path(&state, saved.clone());
    Ok(saved.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn reveal_saved_image(path: String, state: State<'_, ImageGenerationState>) -> Result<(), String> {
    let requested = PathBuf::from(&path);
    let allowed = state.inner.lock().map_err(|_| "Image generation state unavailable")?
        .saved_paths.iter().any(|saved| saved == &requested);
    if !allowed { return Err("Only an image saved in this session can be revealed.".into()); }
    #[cfg(windows)]
    {
        std::process::Command::new("explorer.exe").arg(format!("/select,{}", requested.display()))
            .spawn().map_err(|_| "Could not open File Explorer.".to_owned())?;
        Ok(())
    }
    #[cfg(not(windows))]
    { Err("Reveal in Explorer is available on Windows only.".into()) }
}

fn reject_image(state: &ImageGenerationState, job_id: &str) -> Result<(), String> {
    let removed = {
        let mut inner = state
            .inner
            .lock()
            .map_err(|_| "Image generation state unavailable")?;
        let job = inner.jobs.iter()
            .find(|job| job.job_id == job_id)
            .ok_or("This image generation is no longer available.")?;
        if job.phase.active() {
            return Err("Cancel the active generation before rejecting it.".into());
        }
        let index = inner.jobs.iter().position(|job| job.job_id == job_id)
            .ok_or("This image generation is no longer available.")?;
        inner.jobs.remove(index)
    };
    if let Some(path) = removed.and_then(|job| job.temporary_path) {
        let _ = fs::remove_file(path);
    }
    Ok(())
}

#[tauri::command]
pub fn reject_generated_image(
    job_id: String,
    state: State<'_, ImageGenerationState>,
) -> Result<(), String> {
    reject_image(&state, &job_id)?;
    eprintln!("[AIIDE][image] rejected {job_id}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const DEFAULT_MODEL_ID: &str = SDXL_BASELINE_MODEL_ID;

    struct MockProvider {
        available: bool,
        submitted: Mutex<Vec<Value>>,
        polls: Mutex<VecDeque<ProviderJobState>>,
        image: Vec<u8>,
        cancellation: ProviderCancellation,
        cancel_calls: AtomicUsize,
    }

    impl MockProvider {
        fn new(polls: Vec<ProviderJobState>) -> Self {
            Self {
                available: true,
                submitted: Mutex::new(vec![]),
                polls: Mutex::new(polls.into()),
                image: png(),
                cancellation: ProviderCancellation::Cancelled,
                cancel_calls: AtomicUsize::new(0),
            }
        }
    }

    impl ImageGenerationProvider for MockProvider {
        async fn available(&self) -> Result<bool, String> {
            Ok(self.available)
        }
        async fn submit(&self, workflow: Value, _client_id: &str) -> Result<String, String> {
            self.submitted.lock().unwrap().push(workflow);
            Ok("prompt-1".into())
        }
        async fn poll(&self, _prompt_id: &str) -> Result<ProviderJobState, String> {
            Ok(self.polls.lock().unwrap().pop_front().unwrap())
        }
        async fn fetch(&self, _image: &ProviderImageRef) -> Result<Vec<u8>, String> {
            Ok(self.image.clone())
        }
        async fn cancel(&self, _prompt_id: &str) -> Result<ProviderCancellation, String> {
            self.cancel_calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.cancellation)
        }
    }

    struct MockReadinessProvider {
        system_stats: Result<Value, ReadinessProbeError>,
        object_info: Result<Value, ReadinessProbeError>,
        queue: Result<Value, ReadinessProbeError>,
    }

    impl ImageReadinessProvider for MockReadinessProvider {
        async fn system_stats(&self) -> Result<Value, ReadinessProbeError> {
            self.system_stats.clone()
        }

        async fn object_info(&self) -> Result<Value, ReadinessProbeError> {
            self.object_info.clone()
        }

        async fn queue_info(&self) -> Result<Value, ReadinessProbeError> {
            self.queue.clone()
        }
    }

    fn configuration() -> ExternalImageEngineConfiguration {
        ImageGenerationConfiguration::defaults().external
    }

    fn system_stats() -> Value {
        json!({
            "system": { "os": "nt" },
            "devices": [{ "name": "Test GPU", "type": "cuda", "vram_total": MINIMUM_GUIDANCE_VRAM_BYTES }]
        })
    }

    fn object_info(checkpoint: Option<&str>, omitted_node: Option<&str>) -> Value {
        let mut nodes = serde_json::Map::new();
        for node in SDXL_REQUIRED_NODES {
            if omitted_node == Some(*node) {
                continue;
            }
            let value = if *node == "CheckpointLoaderSimple" {
                json!({
                    "input": {
                        "required": {
                            "ckpt_name": [checkpoint.into_iter().collect::<Vec<_>>(), {}]
                        }
                    }
                })
            } else {
                json!({})
            };
            nodes.insert((*node).into(), value);
        }
        Value::Object(nodes)
    }

    fn readiness_provider(
        checkpoint: Option<&str>,
        omitted_node: Option<&str>,
    ) -> MockReadinessProvider {
        MockReadinessProvider {
            system_stats: Ok(system_stats()),
            object_info: Ok(object_info(checkpoint, omitted_node)),
            queue: Ok(json!({ "queue_running": [], "queue_pending": [] })),
        }
    }

    fn png() -> Vec<u8> {
        [PNG_SIGNATURE.as_slice(), b"test-image"].concat()
    }
    fn image_ref() -> ProviderImageRef {
        ProviderImageRef {
            filename: "preview.png".into(),
            subfolder: "".into(),
            folder_type: "temp".into(),
        }
    }
    fn fixture(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "aiide-image-test-{}-{name}-{}",
            std::process::id(),
            NEXT_SESSION.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path.canonicalize().unwrap()
    }

    #[test]
    fn endpoint_is_loopback_only() {
        assert!(ComfyUiProvider::new("http://127.0.0.1:8188", Duration::from_secs(1)).is_ok());
        assert!(ComfyUiProvider::new("http://localhost:9000", Duration::from_secs(1)).is_ok());
        for endpoint in [
            "https://127.0.0.1:8188",
            "http://example.com:8188",
            "http://user@127.0.0.1:8188",
            "http://127.0.0.1:8188/api",
            "http://127.0.0.1:8188/?token=secret",
            "http://127.0.0.1:8188/#fragment",
        ] {
            assert!(ComfyUiProvider::new(endpoint, Duration::from_secs(1)).is_err());
        }
    }

    #[test]
    fn model_registry_keeps_workflow_requirements_out_of_the_runtime() {
        let model = model_definition(DEFAULT_MODEL_ID).unwrap();
        assert_eq!(model.architecture, "SDXL");
        assert_eq!(model.workflow_id, "comfyui-sdxl-base-v1");
        assert!(model.required_nodes.contains(&"PreviewImage"));
        assert_eq!(IMAGE_MODELS.len(), 1);
    }

    #[test]
    fn successful_engine_and_model_readiness_is_structured() {
        let status = tauri::async_runtime::block_on(readiness_with_provider(
            &readiness_provider(Some(DEFAULT_CHECKPOINT), None),
            &configuration(),
            false,
        ));
        assert_eq!(status.state, "ready");
        assert!(status.ready);
        assert_eq!(status.engine_status, "reachable");
        assert_eq!(status.model_status, "ready");
        assert_eq!(status.hardware_status, "sufficient");
        assert!(!status.busy);
    }

    #[test]
    fn readiness_reports_missing_checkpoint_and_nodes() {
        let missing_checkpoint = tauri::async_runtime::block_on(readiness_with_provider(
            &readiness_provider(Some("another.safetensors"), None),
            &configuration(),
            false,
        ));
        assert_eq!(missing_checkpoint.state, "missing_checkpoint");
        assert_eq!(missing_checkpoint.missing_files, vec![DEFAULT_CHECKPOINT]);

        let missing_nodes = tauri::async_runtime::block_on(readiness_with_provider(
            &readiness_provider(Some(DEFAULT_CHECKPOINT), Some("PreviewImage")),
            &configuration(),
            false,
        ));
        assert_eq!(missing_nodes.state, "missing_nodes");
        assert_eq!(missing_nodes.missing_nodes, vec!["PreviewImage"]);
    }

    #[test]
    fn readiness_exposes_only_verified_sdxl_checkpoint_as_compatible() {
        let object_info = object_info(Some(DEFAULT_CHECKPOINT), None);
        let checkpoints = object_info["CheckpointLoaderSimple"]["input"]["required"]["ckpt_name"][0]
            .as_array().unwrap();
        let summaries = checkpoint_summaries(checkpoints, DEFAULT_CHECKPOINT);
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].compatibility, "compatible");

        let mut unknown_configuration = configuration();
        unknown_configuration.checkpoint = "unknown-model.safetensors".into();
        let status = tauri::async_runtime::block_on(readiness_with_provider(
            &readiness_provider(Some("unknown-model.safetensors"), None),
            &unknown_configuration,
            false,
        ));
        assert_eq!(status.state, "incompatible");
        assert!(!status.ready);
        assert_eq!(status.checkpoints[0].compatibility, "unknown");
    }

    #[test]
    fn readiness_reports_unsupported_unavailable_and_malformed_apis() {
        let unavailable = MockReadinessProvider {
            system_stats: Err(ReadinessProbeError::Unavailable(
                "temporary disconnect".into(),
            )),
            object_info: Ok(Value::Null),
            queue: Ok(Value::Null),
        };
        let status = tauri::async_runtime::block_on(readiness_with_provider(
            &unavailable,
            &configuration(),
            false,
        ));
        assert_eq!(status.state, "unavailable");

        let unsupported = MockReadinessProvider {
            system_stats: Ok(system_stats()),
            object_info: Err(ReadinessProbeError::Incompatible(
                "missing object_info".into(),
            )),
            queue: Ok(Value::Null),
        };
        let status = tauri::async_runtime::block_on(readiness_with_provider(
            &unsupported,
            &configuration(),
            false,
        ));
        assert_eq!(status.state, "incompatible");

        let malformed = MockReadinessProvider {
            system_stats: Ok(system_stats()),
            object_info: Ok(Value::Null),
            queue: Ok(json!({ "queue_running": [], "queue_pending": [] })),
        };
        let status = tauri::async_runtime::block_on(readiness_with_provider(
            &malformed,
            &configuration(),
            false,
        ));
        assert_eq!(status.state, "incompatible");
        assert!(status.error.unwrap().contains("malformed"));
    }

    #[test]
    fn readiness_distinguishes_busy_and_advisory_hardware() {
        let mut provider = readiness_provider(Some(DEFAULT_CHECKPOINT), None);
        provider.queue = Ok(json!({ "queue_running": [[1, "other-prompt"]], "queue_pending": [] }));
        provider.system_stats = Ok(json!({
            "system": { "os": "nt" },
            "devices": [{ "type": "cuda", "vram_total": 4_u64 * 1024 * 1024 * 1024 }]
        }));
        let status = tauri::async_runtime::block_on(readiness_with_provider(
            &provider,
            &configuration(),
            false,
        ));
        assert_eq!(status.state, "busy");
        assert_eq!(status.engine_status, "busy");
        assert_eq!(status.hardware_status, "potentially_insufficient");
        assert!(status.ready);
    }

    #[test]
    fn explicit_configuration_is_persisted_and_wins_over_fallbacks() {
        let root = fixture("configuration");
        let path = root.join(CONFIG_FILE);
        let saved = persist_configuration(
            &path,
            "http://localhost:9000".into(),
            DEFAULT_MODEL_ID.into(),
            None,
        )
        .unwrap();
        assert_eq!(saved.external.endpoint, "http://localhost:9000");
        assert_eq!(saved.ownership_mode, ImageEngineOwnershipMode::External);
        assert_eq!(load_configuration(&path).unwrap(), saved);
        assert!(path.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_configuration_migrates_to_explicit_external_ownership() {
        let root = fixture("legacy-configuration");
        let path = root.join(CONFIG_FILE);
        fs::write(
            &path,
            r#"{"endpoint":"http://localhost:8123","modelId":"sdxl-1.0-base","checkpoint":"sd_xl_base_1.0.safetensors"}"#,
        )
        .unwrap();
        let migrated = load_configuration(&path).unwrap();
        assert_eq!(migrated.schema_version, IMAGE_CONFIGURATION_SCHEMA);
        assert_eq!(migrated.ownership_mode, ImageEngineOwnershipMode::External);
        assert_eq!(migrated.external.endpoint, "http://localhost:8123");
        assert!(migrated.managed.is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn selecting_external_preserves_saved_managed_identity() {
        let root = fixture("ownership-preservation");
        let path = root.join(CONFIG_FILE);
        let mut configuration = ImageGenerationConfiguration::defaults();
        configuration.ownership_mode = ImageEngineOwnershipMode::Managed;
        configuration.managed = Some(ManagedImageEngineConfiguration {
            installation_id: "install-1".into(),
            runtime_version: "0.36.0".into(),
            manifest_id: "comfyui-windows-nvidia-v0.36.0-c1.6".into(),
            model_id: SDXL_BASELINE_MODEL_ID.into(),
            model_version: "acceptance-fixture".into(),
        });
        fs::write(&path, serde_json::to_vec_pretty(&configuration).unwrap()).unwrap();
        let saved = persist_configuration(
            &path,
            "http://localhost:9000".into(),
            SDXL_BASELINE_MODEL_ID.into(),
            None,
        )
        .unwrap();
        assert_eq!(saved.ownership_mode, ImageEngineOwnershipMode::External);
        assert_eq!(
            saved
                .managed
                .as_ref()
                .map(|managed| managed.installation_id.as_str()),
            Some("install-1")
        );
        assert_eq!(saved.external.endpoint, "http://localhost:9000");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn managed_selection_never_falls_back_to_external_process_ownership() {
        let mut configuration = ImageGenerationConfiguration::defaults();
        configuration.ownership_mode = ImageEngineOwnershipMode::Managed;
        assert!(external_configuration(&configuration)
            .unwrap_err()
            .contains("disabled"));
        assert_eq!(configuration.external.endpoint, DEFAULT_ENDPOINT);
    }

    #[test]
    fn fixed_workflow_uses_quality_defaults_and_preview_only() {
        let workflow = sdxl_workflow("A quiet forest", DEFAULT_CHECKPOINT, 42);
        assert_eq!(workflow["3"]["inputs"]["steps"], 30);
        assert_eq!(workflow["3"]["inputs"]["sampler_name"], "dpmpp_2m");
        assert_eq!(workflow["3"]["inputs"]["scheduler"], "karras");
        assert_eq!(workflow["5"]["inputs"]["width"], 1024);
        assert_eq!(workflow["9"]["class_type"], "PreviewImage");
        assert!(!workflow.to_string().contains("SaveImage\""));
    }

    #[test]
    fn history_parsing_maps_preview_failure_and_oom() {
        let ready = json!({ "outputs": { "9": { "images": [{ "filename": "x.png", "subfolder": "", "type": "temp" }] } } });
        assert_eq!(
            image_from_history(&ready),
            Some(ProviderImageRef {
                filename: "x.png".into(),
                subfolder: "".into(),
                folder_type: "temp".into()
            })
        );
        assert!(map_execution_error("CUDA out of memory allocating tensor").contains("8 GB"));
        assert!(map_execution_error("bad node").contains("console"));
        assert!(workflow_submission_error(&json!({ "node_errors": { "4": { "errors": [{ "message": "ckpt_name not in list" }] } } })).contains("not installed"));
        assert!(
            workflow_submission_error(&json!({ "node_errors": { "9": { "errors": [] } } }))
                .contains("core SDXL workflow")
        );
    }

    #[test]
    fn unavailable_provider_releases_the_gpu_slot() {
        let mut provider = MockProvider::new(vec![]);
        provider.available = false;
        let state = ImageGenerationState::default();
        let error = tauri::async_runtime::block_on(start_with_provider(
            &provider,
            &state,
            "cat".into(),
            model_definition(DEFAULT_MODEL_ID).unwrap(),
            DEFAULT_CHECKPOINT,
        ))
        .unwrap_err();
        assert!(error.contains("unavailable"));
        assert!(state.inner.lock().unwrap().jobs.is_empty());
    }

    #[test]
    fn successful_preview_is_temporary_and_single_job_is_enforced() {
        let provider = MockProvider::new(vec![
            ProviderJobState::Running,
            ProviderJobState::Complete(image_ref()),
        ]);
        let state = ImageGenerationState::default();
        let job = tauri::async_runtime::block_on(start_with_provider(
            &provider,
            &state,
            "A lighthouse".into(),
            model_definition(DEFAULT_MODEL_ID).unwrap(),
            DEFAULT_CHECKPOINT,
        ))
        .unwrap();
        let duplicate = tauri::async_runtime::block_on(start_with_provider(
            &provider,
            &state,
            "Another".into(),
            model_definition(DEFAULT_MODEL_ID).unwrap(),
            DEFAULT_CHECKPOINT,
        ))
        .unwrap_err();
        assert!(duplicate.contains("active image"));
        let running =
            tauri::async_runtime::block_on(refresh_with_provider(&provider, &state, &job.job_id))
                .unwrap();
        assert_eq!(running.status, "generating");
        let ready =
            tauri::async_runtime::block_on(refresh_with_provider(&provider, &state, &job.job_id))
                .unwrap();
        assert_eq!(ready.status, "ready");
        assert!(ready.preview_available);
        let path = state
            .inner
            .lock()
            .unwrap()
            .jobs
            .front()
            .unwrap()
            .temporary_path
            .clone()
            .unwrap();
        assert!(path.exists());
        reject_image(&state, &job.job_id).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn regeneration_preserves_original_and_history_is_bounded() {
        let provider = MockProvider::new(vec![ProviderJobState::Complete(image_ref()); MAX_SESSION_IMAGES + 1]);
        let state = ImageGenerationState::default();
        let mut first_path = None;
        let mut previous_seed = None;
        for index in 0..=MAX_SESSION_IMAGES {
            let job = tauri::async_runtime::block_on(start_with_provider(
                &provider,
                &state,
                format!("image {index}"),
                model_definition(DEFAULT_MODEL_ID).unwrap(),
                DEFAULT_CHECKPOINT,
            )).unwrap();
            if let Some(seed) = previous_seed { assert_ne!(seed, job.seed); }
            previous_seed = Some(job.seed);
            tauri::async_runtime::block_on(refresh_with_provider(&provider, &state, &job.job_id)).unwrap();
            if index == 0 {
                first_path = state.inner.lock().unwrap().jobs.front().unwrap().temporary_path.clone();
            }
        }
        let inner = state.inner.lock().unwrap();
        assert_eq!(inner.jobs.len(), MAX_SESSION_IMAGES);
        assert_eq!(inner.jobs.front().unwrap().prompt, "image 1");
        assert!(inner.jobs.iter().all(|job| job.model_id == DEFAULT_MODEL_ID && job.checkpoint == DEFAULT_CHECKPOINT));
        drop(inner);
        assert!(!first_path.unwrap().exists());
    }

    #[test]
    fn failed_new_submission_does_not_evict_reviewable_history() {
        let provider = MockProvider::new(vec![ProviderJobState::Complete(image_ref())]);
        let state = ImageGenerationState::default();
        let job = tauri::async_runtime::block_on(start_with_provider(
            &provider, &state, "kept".into(), model_definition(DEFAULT_MODEL_ID).unwrap(), DEFAULT_CHECKPOINT,
        )).unwrap();
        tauri::async_runtime::block_on(refresh_with_provider(&provider, &state, &job.job_id)).unwrap();
        let path = state.inner.lock().unwrap().jobs.front().unwrap().temporary_path.clone().unwrap();
        let mut unavailable = MockProvider::new(vec![]);
        unavailable.available = false;
        assert!(tauri::async_runtime::block_on(start_with_provider(
            &unavailable, &state, "fails".into(), model_definition(DEFAULT_MODEL_ID).unwrap(), DEFAULT_CHECKPOINT,
        )).is_err());
        assert_eq!(state.inner.lock().unwrap().jobs.len(), 1);
        assert!(path.exists());
    }

    #[test]
    fn cancellation_is_targeted_and_unsupported_running_jobs_remain_active() {
        let mut provider = MockProvider::new(vec![]);
        provider.cancellation = ProviderCancellation::Unsupported;
        let state = ImageGenerationState::default();
        let job = tauri::async_runtime::block_on(start_with_provider(
            &provider,
            &state,
            "cat".into(),
            model_definition(DEFAULT_MODEL_ID).unwrap(),
            DEFAULT_CHECKPOINT,
        ))
        .unwrap();
        let result =
            tauri::async_runtime::block_on(cancel_with_provider(&provider, &state, &job.job_id))
                .unwrap();
        assert_eq!(result.status, "queued");
        assert!(!result.cancellation_supported);
        assert!(result.error.unwrap().contains("global interrupt"));
        assert_eq!(provider.cancel_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn generation_failure_and_successful_cancellation_are_terminal() {
        let failed_provider =
            MockProvider::new(vec![ProviderJobState::Failed("generation failed".into())]);
        let failed_state = ImageGenerationState::default();
        let failed = tauri::async_runtime::block_on(start_with_provider(
            &failed_provider,
            &failed_state,
            "cat".into(),
            model_definition(DEFAULT_MODEL_ID).unwrap(),
            DEFAULT_CHECKPOINT,
        ))
        .unwrap();
        let failed = tauri::async_runtime::block_on(refresh_with_provider(
            &failed_provider,
            &failed_state,
            &failed.job_id,
        ))
        .unwrap();
        assert_eq!(failed.status, "failed");
        assert_eq!(failed.error.as_deref(), Some("generation failed"));
        reject_image(&failed_state, &failed.job_id).unwrap();

        let provider = MockProvider::new(vec![]);
        let state = ImageGenerationState::default();
        let job = tauri::async_runtime::block_on(start_with_provider(
            &provider,
            &state,
            "cat".into(),
            model_definition(DEFAULT_MODEL_ID).unwrap(),
            DEFAULT_CHECKPOINT,
        ))
        .unwrap();
        let cancelled =
            tauri::async_runtime::block_on(cancel_with_provider(&provider, &state, &job.job_id))
                .unwrap();
        assert_eq!(cancelled.status, "cancelled");
        reject_image(&state, &job.job_id).unwrap();
    }

    #[test]
    fn invalid_preview_becomes_a_rejectable_failure() {
        let mut provider = MockProvider::new(vec![ProviderJobState::Complete(image_ref())]);
        provider.image = b"not a png".to_vec();
        let state = ImageGenerationState::default();
        let job = tauri::async_runtime::block_on(start_with_provider(
            &provider,
            &state,
            "cat".into(),
            model_definition(DEFAULT_MODEL_ID).unwrap(),
            DEFAULT_CHECKPOINT,
        ))
        .unwrap();
        let failed =
            tauri::async_runtime::block_on(refresh_with_provider(&provider, &state, &job.job_id))
                .unwrap();
        assert_eq!(failed.status, "failed");
        assert!(failed.error.unwrap().contains("invalid PNG"));
        reject_image(&state, &job.job_id).unwrap();
    }

    #[test]
    fn project_change_preserves_reviewable_preview_until_rejected() {
        let project = fixture("reject");
        let provider = MockProvider::new(vec![ProviderJobState::Complete(image_ref())]);
        let state = ImageGenerationState::default();
        let job = tauri::async_runtime::block_on(start_with_provider(
            &provider,
            &state,
            "cat".into(),
            model_definition(DEFAULT_MODEL_ID).unwrap(),
            DEFAULT_CHECKPOINT,
        ))
        .unwrap();
        tauri::async_runtime::block_on(refresh_with_provider(&provider, &state, &job.job_id))
            .unwrap();
        let temp = state
            .inner
            .lock()
            .unwrap()
            .jobs
            .front()
            .unwrap()
            .temporary_path
            .clone()
            .unwrap();
        assert!(temp.exists());
        reject_image(&state, &job.job_id).unwrap();
        assert!(!temp.exists());
        assert!(fs::read_dir(&project).unwrap().next().is_none());
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn approved_save_is_project_relative_and_collision_safe() {
        let root = fixture("save");
        let first = save_bytes_to_project(&root, "assets/elma.png", &png()).unwrap();
        let second = save_bytes_to_project(&root, "assets/elma.png", &png()).unwrap();
        assert_eq!(first, "assets/elma.png");
        assert_eq!(second, "assets/elma-2.png");
        assert_eq!(fs::read(root.join(&first)).unwrap(), png());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn save_as_is_absolute_collision_safe_and_rejects_protected_paths() {
        let root = fixture("save-as");
        let target = root.join("elma.png");
        let first = save_bytes_to_absolute(&target, &png()).unwrap();
        let second = save_bytes_to_absolute(&target, &png()).unwrap();
        assert_eq!(first, target);
        assert_eq!(second, root.join("elma-2.png"));
        assert!(save_bytes_to_absolute(&root.join(".env").join("image.png"), &png()).unwrap_err().contains("Protected"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn approved_job_save_cleans_temporary_state() {
        let root = fixture("save-cleanup");
        let provider = MockProvider::new(vec![ProviderJobState::Complete(image_ref())]);
        let state = ImageGenerationState::default();
        let job = tauri::async_runtime::block_on(start_with_provider(
            &provider,
            &state,
            "cat".into(),
            model_definition(DEFAULT_MODEL_ID).unwrap(),
            DEFAULT_CHECKPOINT,
        ))
        .unwrap();
        tauri::async_runtime::block_on(refresh_with_provider(&provider, &state, &job.job_id))
            .unwrap();
        let temporary = state
            .inner
            .lock()
            .unwrap()
            .jobs
            .front()
            .unwrap()
            .temporary_path
            .clone()
            .unwrap();
        assert_eq!(
            save_ready_image(&state, &root, &job.job_id, "image.png").unwrap(),
            "image.png"
        );
        assert!(!temporary.exists());
        assert!(state.inner.lock().unwrap().jobs.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_and_protected_paths_are_rejected() {
        for path in [
            "",
            "../image.png",
            "C:/image.png",
            "image.jpg",
            ".env/image.png",
            "target/image.png",
            "folder\\image.png",
        ] {
            assert!(
                validate_asset_path(path).is_err(),
                "{path} should be rejected"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlink_parent_is_rejected() {
        use std::os::unix::fs::symlink;
        let root = fixture("symlink");
        let outside = fixture("outside");
        symlink(&outside, root.join("linked")).unwrap();
        assert!(save_bytes_to_project(&root, "linked/image.png", &png())
            .unwrap_err()
            .contains("real project folders"));
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
}
