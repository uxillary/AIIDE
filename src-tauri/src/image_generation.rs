use reqwest::{Client, StatusCode, Url};
use serde::Serialize;
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{ipc::Response, State};

use crate::project::{OpenProject, IGNORED};

const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:8188";
const DEFAULT_CHECKPOINT: &str = "sd_xl_base_1.0.safetensors";
const MAX_PROMPT_CHARS: usize = 4_000;
const MAX_IMAGE_BYTES: usize = 25 * 1024 * 1024;
const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

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
    phase: JobPhase,
    status_label: String,
    error: Option<String>,
    temporary_path: Option<PathBuf>,
    cancellation_supported: bool,
}

#[derive(Default)]
struct ImageStateData {
    next_job: u64,
    current: Option<PendingImage>,
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

impl ImageGenerationState {
    pub fn clear_for_project_change(&self) {
        let removed = self.inner.lock().ok().and_then(|mut state| {
            if state.current.as_ref().is_some_and(|job| job.phase.active()) {
                None
            } else {
                state.current.take()
            }
        });
        if let Some(path) = removed.and_then(|job| job.temporary_path) {
            let _ = fs::remove_file(path);
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageJob {
    job_id: String,
    prompt: String,
    seed: u64,
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
            status: job.phase.name(),
            status_label: job.status_label.clone(),
            preview_available: job.temporary_path.is_some() && job.phase == JobPhase::Ready,
            cancellation_supported: job.cancellation_supported,
            error: job.error.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageEngineStatus {
    state: &'static str,
    endpoint: String,
    checkpoint: String,
    busy: bool,
    error: Option<String>,
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
    fn from_env(timeout: Duration) -> Result<Self, String> {
        let configured =
            std::env::var("AIIDE_COMFYUI_ENDPOINT").unwrap_or_else(|_| DEFAULT_ENDPOINT.into());
        Self::new(&configured, timeout)
    }

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
        "The configured SDXL checkpoint is not installed in ComfyUI. Check AIIDE_COMFYUI_CHECKPOINT and the ComfyUI models\\checkpoints folder.".into()
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

fn configured_checkpoint() -> Result<String, String> {
    let checkpoint =
        std::env::var("AIIDE_COMFYUI_CHECKPOINT").unwrap_or_else(|_| DEFAULT_CHECKPOINT.into());
    if checkpoint.trim().is_empty()
        || checkpoint.contains("..")
        || checkpoint.contains(':')
        || checkpoint.starts_with(['/', '\\'])
    {
        return Err(
            "AIIDE_COMFYUI_CHECKPOINT must be a ComfyUI-relative checkpoint filename.".into(),
        );
    }
    Ok(checkpoint)
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
        if inner
            .current
            .as_ref()
            .is_some_and(|job| job.job_id == job_id)
        {
            inner.current = None;
        }
    }
}

async fn start_with_provider(
    provider: &impl ImageGenerationProvider,
    state: &ImageGenerationState,
    prompt: String,
    checkpoint: &str,
) -> Result<ImageJob, String> {
    let prompt = validate_prompt(&prompt)?;
    let seed = next_seed();
    let job_id = {
        let mut inner = state
            .inner
            .lock()
            .map_err(|_| "Image generation state unavailable")?;
        if inner.current.is_some() {
            return Err(
                "Finish, save, or reject the current image before starting another generation."
                    .into(),
            );
        }
        inner.next_job += 1;
        let job_id = format!("image-{}", inner.next_job);
        inner.current = Some(PendingImage {
            job_id: job_id.clone(),
            provider_prompt_id: None,
            prompt: prompt.clone(),
            seed,
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
        .submit(sdxl_workflow(&prompt, checkpoint, seed), &job_id)
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
    let job = inner
        .current
        .as_mut()
        .filter(|job| job.job_id == job_id)
        .ok_or("Image generation state changed unexpectedly.")?;
    job.provider_prompt_id = Some(prompt_id);
    job.phase = JobPhase::Queued;
    job.status_label = "Queued in ComfyUI".into();
    Ok(ImageJob::from(&*job))
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
    let job = inner
        .current
        .as_mut()
        .filter(|job| job.job_id == job_id)
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
        inner
            .current
            .as_ref()
            .filter(|job| job.job_id == job_id)
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
    let job = inner
        .current
        .as_mut()
        .filter(|job| job.job_id == job_id)
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
        let job = inner
            .current
            .as_ref()
            .filter(|job| job.job_id == job_id)
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
    let job = inner
        .current
        .as_mut()
        .filter(|job| job.job_id == job_id)
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
) -> Result<ImageEngineStatus, String> {
    let checkpoint = match configured_checkpoint() {
        Ok(checkpoint) => checkpoint,
        Err(error) => {
            return Ok(ImageEngineStatus {
                state: "error",
                endpoint: std::env::var("AIIDE_COMFYUI_ENDPOINT")
                    .unwrap_or_else(|_| DEFAULT_ENDPOINT.into()),
                checkpoint: std::env::var("AIIDE_COMFYUI_CHECKPOINT").unwrap_or_default(),
                busy: false,
                error: Some(error),
            })
        }
    };
    let busy = state
        .inner
        .lock()
        .ok()
        .and_then(|inner| inner.current.as_ref().map(|job| job.phase.active()))
        .unwrap_or(false);
    let provider = match ComfyUiProvider::from_env(Duration::from_secs(4)) {
        Ok(provider) => provider,
        Err(error) => {
            return Ok(ImageEngineStatus {
                state: "error",
                endpoint: std::env::var("AIIDE_COMFYUI_ENDPOINT")
                    .unwrap_or_else(|_| DEFAULT_ENDPOINT.into()),
                checkpoint,
                busy,
                error: Some(error),
            })
        }
    };
    let endpoint = provider.endpoint.as_str().trim_end_matches('/').to_owned();
    match provider.available().await {
        Ok(true) => Ok(ImageEngineStatus {
            state: "connected",
            endpoint,
            checkpoint,
            busy,
            error: None,
        }),
        Ok(false) => Ok(ImageEngineStatus {
            state: "offline",
            endpoint,
            checkpoint,
            busy,
            error: Some("ComfyUI is unavailable at the configured local endpoint.".into()),
        }),
        Err(error) => Ok(ImageEngineStatus {
            state: "offline",
            endpoint,
            checkpoint,
            busy,
            error: Some(error),
        }),
    }
}

#[tauri::command]
pub async fn start_image_generation(
    prompt: String,
    state: State<'_, ImageGenerationState>,
) -> Result<ImageJob, String> {
    let provider = ComfyUiProvider::from_env(Duration::from_secs(20))?;
    let result = start_with_provider(&provider, &state, prompt, &configured_checkpoint()?).await;
    if let Ok(job) = &result {
        eprintln!("[AIIDE][image] submitted {}", job.job_id);
    }
    result
}

#[tauri::command]
pub async fn get_image_generation(
    job_id: String,
    state: State<'_, ImageGenerationState>,
) -> Result<ImageJob, String> {
    let provider = ComfyUiProvider::from_env(Duration::from_secs(30))?;
    refresh_with_provider(&provider, &state, &job_id).await
}

#[tauri::command]
pub async fn cancel_image_generation(
    job_id: String,
    state: State<'_, ImageGenerationState>,
) -> Result<ImageJob, String> {
    let provider = ComfyUiProvider::from_env(Duration::from_secs(10))?;
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
        let job = inner
            .current
            .as_ref()
            .filter(|job| job.job_id == job_id)
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
        let job = inner
            .current
            .as_ref()
            .filter(|job| job.job_id == job_id)
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
        if inner
            .current
            .as_ref()
            .is_some_and(|job| job.job_id == job_id)
        {
            inner.current.take()
        } else {
            None
        }
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
    eprintln!("[AIIDE][image] saved {job_id} as {saved}");
    Ok(saved)
}

fn reject_image(state: &ImageGenerationState, job_id: &str) -> Result<(), String> {
    let removed = {
        let mut inner = state
            .inner
            .lock()
            .map_err(|_| "Image generation state unavailable")?;
        let job = inner
            .current
            .as_ref()
            .filter(|job| job.job_id == job_id)
            .ok_or("This image generation is no longer available.")?;
        if job.phase.active() {
            return Err("Cancel the active generation before rejecting it.".into());
        }
        inner.current.take()
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
        ] {
            assert!(ComfyUiProvider::new(endpoint, Duration::from_secs(1)).is_err());
        }
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
            DEFAULT_CHECKPOINT,
        ))
        .unwrap_err();
        assert!(error.contains("unavailable"));
        assert!(state.inner.lock().unwrap().current.is_none());
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
            DEFAULT_CHECKPOINT,
        ))
        .unwrap();
        let duplicate = tauri::async_runtime::block_on(start_with_provider(
            &provider,
            &state,
            "Another".into(),
            DEFAULT_CHECKPOINT,
        ))
        .unwrap_err();
        assert!(duplicate.contains("current image"));
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
            .current
            .as_ref()
            .unwrap()
            .temporary_path
            .clone()
            .unwrap();
        assert!(path.exists());
        reject_image(&state, &job.job_id).unwrap();
        assert!(!path.exists());
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
    fn rejected_image_never_writes_and_project_change_cleans_terminal_preview() {
        let project = fixture("reject");
        let provider = MockProvider::new(vec![ProviderJobState::Complete(image_ref())]);
        let state = ImageGenerationState::default();
        let job = tauri::async_runtime::block_on(start_with_provider(
            &provider,
            &state,
            "cat".into(),
            DEFAULT_CHECKPOINT,
        ))
        .unwrap();
        tauri::async_runtime::block_on(refresh_with_provider(&provider, &state, &job.job_id))
            .unwrap();
        let temp = state
            .inner
            .lock()
            .unwrap()
            .current
            .as_ref()
            .unwrap()
            .temporary_path
            .clone()
            .unwrap();
        state.clear_for_project_change();
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
    fn approved_job_save_cleans_temporary_state() {
        let root = fixture("save-cleanup");
        let provider = MockProvider::new(vec![ProviderJobState::Complete(image_ref())]);
        let state = ImageGenerationState::default();
        let job = tauri::async_runtime::block_on(start_with_provider(
            &provider,
            &state,
            "cat".into(),
            DEFAULT_CHECKPOINT,
        ))
        .unwrap();
        tauri::async_runtime::block_on(refresh_with_provider(&provider, &state, &job.job_id))
            .unwrap();
        let temporary = state
            .inner
            .lock()
            .unwrap()
            .current
            .as_ref()
            .unwrap()
            .temporary_path
            .clone()
            .unwrap();
        assert_eq!(
            save_ready_image(&state, &root, &job.job_id, "image.png").unwrap(),
            "image.png"
        );
        assert!(!temporary.exists());
        assert!(state.inner.lock().unwrap().current.is_none());
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
