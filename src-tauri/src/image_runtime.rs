// These contracts intentionally remain unreachable from production acquisition/execution until
// the release gate is approved. Tests exercise them directly in the meantime.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MANAGED_RUNTIME_SCHEMA: u32 = 1;
const PINNED_MANIFEST_ID: &str = "comfyui-windows-nvidia-v0.36.0-c1.6";
const PINNED_RUNTIME_ID: &str = "comfyui-windows-portable-nvidia";
const PINNED_RUNTIME_VERSION: &str = "0.36.0";
const PINNED_ASSET_NAME: &str = "ComfyUI_windows_portable_nvidia.7z";
const PINNED_SOURCE_URL: &str =
    "https://github.com/Comfy-Org/ComfyUI/releases/download/v0.36.0/ComfyUI_windows_portable_nvidia.7z";
const PINNED_ASSET_BYTES: u64 = 1_917_442_353;
const PINNED_INSTALLED_BYTES: u64 = 4_166_922_797;
const PINNED_SHA256: &str = "c3c60192840f8b68c9a47cf3e8161ecb108e5ffdf5ea236c1c72a402e442695d";
const PINNED_ROOT: &str = "ComfyUI_windows_portable";
const MANAGED_GATE_REASON: &str = "Managed ComfyUI acquisition and execution are disabled pending manifest-trust, licensing, and release approval.";
static NEXT_RECORD_WRITE: AtomicU64 = AtomicU64::new(1);

pub(crate) const MANAGED_ACQUISITION_ENABLED: bool = false;
pub(crate) const MANAGED_EXECUTION_ENABLED: bool = false;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeManifest {
    pub schema_version: u32,
    pub manifest_id: String,
    pub runtime_id: String,
    pub runtime_version: String,
    pub platform: String,
    pub architecture: String,
    pub asset_name: String,
    pub source_url: String,
    pub asset_bytes: u64,
    pub installed_bytes: u64,
    pub sha256: String,
    pub expected_root: String,
    pub required_files: Vec<String>,
}

impl RuntimeManifest {
    pub(crate) fn pinned() -> Self {
        Self {
            schema_version: MANAGED_RUNTIME_SCHEMA,
            manifest_id: PINNED_MANIFEST_ID.into(),
            runtime_id: PINNED_RUNTIME_ID.into(),
            runtime_version: PINNED_RUNTIME_VERSION.into(),
            platform: "windows".into(),
            architecture: "x86_64".into(),
            asset_name: PINNED_ASSET_NAME.into(),
            source_url: PINNED_SOURCE_URL.into(),
            asset_bytes: PINNED_ASSET_BYTES,
            installed_bytes: PINNED_INSTALLED_BYTES,
            sha256: PINNED_SHA256.into(),
            expected_root: PINNED_ROOT.into(),
            required_files: vec![
                "ComfyUI_windows_portable/python_embeded/python.exe".into(),
                "ComfyUI_windows_portable/ComfyUI/main.py".into(),
            ],
        }
    }

    pub(crate) fn validate_pinned(&self) -> Result<(), String> {
        let pinned = Self::pinned();
        if self != &pinned {
            return Err(
                "The managed-runtime manifest identity does not match this AIIDE build.".into(),
            );
        }
        validate_https_url(&self.source_url)?;
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("The managed-runtime manifest has an invalid SHA-256 value.".into());
        }
        Ok(())
    }

    pub(crate) fn identity_sha256(&self) -> Result<String, String> {
        self.validate_pinned()?;
        let encoded = serde_json::to_vec(self)
            .map_err(|_| "Could not encode the managed-runtime manifest.".to_owned())?;
        Ok(hex_sha256(&encoded))
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedRuntimeStatus {
    pub(crate) state: &'static str,
    pub(crate) acquisition_enabled: bool,
    pub(crate) execution_enabled: bool,
    pub(crate) manifest_id: &'static str,
    pub(crate) runtime_version: &'static str,
    pub(crate) authenticated_manifest: bool,
    pub(crate) message: &'static str,
}

#[tauri::command]
pub fn managed_image_runtime_status() -> ManagedRuntimeStatus {
    ManagedRuntimeStatus {
        state: "disabled",
        acquisition_enabled: MANAGED_ACQUISITION_ENABLED,
        execution_enabled: MANAGED_EXECUTION_ENABLED,
        manifest_id: PINNED_MANIFEST_ID,
        runtime_version: PINNED_RUNTIME_VERSION,
        // The C1.6 digest is independently recorded evidence, not a signed manifest.
        authenticated_manifest: false,
        message: MANAGED_GATE_REASON,
    }
}

pub(crate) fn require_acquisition_gate() -> Result<(), String> {
    if MANAGED_ACQUISITION_ENABLED {
        Ok(())
    } else {
        Err(MANAGED_GATE_REASON.into())
    }
}

pub(crate) fn require_execution_gate() -> Result<(), String> {
    if MANAGED_EXECUTION_ENABLED {
        Ok(())
    } else {
        Err(MANAGED_GATE_REASON.into())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AcquisitionConsent {
    pub manifest_id: String,
    pub manifest_sha256: String,
    pub accepted_at_unix_ms: u64,
}

impl AcquisitionConsent {
    pub(crate) fn validate(&self, manifest: &RuntimeManifest) -> Result<(), String> {
        manifest.validate_pinned()?;
        if self.manifest_id != manifest.manifest_id
            || self.manifest_sha256 != manifest.identity_sha256()?
            || self.accepted_at_unix_ms == 0
        {
            return Err("Explicit consent does not match this managed-runtime acquisition.".into());
        }
        Ok(())
    }
}

fn validate_https_url(value: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(value)
        .map_err(|_| "The managed-runtime source URL is invalid.".to_owned())?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "Managed-runtime sources must use HTTPS without credentials or fragments.".into(),
        );
    }
    Ok(url)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DownloadPolicy {
    pub maximum_redirects: usize,
}

impl Default for DownloadPolicy {
    fn default() -> Self {
        Self {
            maximum_redirects: 5,
        }
    }
}

impl DownloadPolicy {
    pub(crate) fn validate_chain(
        &self,
        manifest: &RuntimeManifest,
        chain: &[String],
    ) -> Result<(), String> {
        manifest.validate_pinned()?;
        if chain.is_empty() || chain[0] != manifest.source_url {
            return Err("The download did not begin at the pinned runtime source.".into());
        }
        if chain.len().saturating_sub(1) > self.maximum_redirects {
            return Err("The managed-runtime source exceeded the redirect limit.".into());
        }
        for (index, value) in chain.iter().enumerate() {
            let url = validate_https_url(value)?;
            let host = url
                .host_str()
                .ok_or("The managed-runtime source has no host.")?;
            let allowed = if index == 0 {
                value == &manifest.source_url
            } else if host.eq_ignore_ascii_case("github.com") {
                url.query().is_none()
                    && url.path()
                        == "/Comfy-Org/ComfyUI/releases/download/v0.36.0/ComfyUI_windows_portable_nvidia.7z"
            } else if host.eq_ignore_ascii_case("release-assets.githubusercontent.com") {
                url.path()
                    .starts_with("/github-production-release-asset/589831718/")
            } else {
                false
            };
            if !allowed {
                return Err(
                    "The managed-runtime source redirected to an unapproved host or path.".into(),
                );
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PartialDownloadIdentity {
    pub manifest_id: String,
    pub source_url: String,
    pub expected_bytes: u64,
    pub expected_sha256: String,
    pub received_bytes: u64,
    pub etag: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResumeResponseIdentity {
    pub status: u16,
    pub content_range_start: Option<u64>,
    pub content_range_total: Option<u64>,
    pub content_length: Option<u64>,
    pub etag: Option<String>,
    pub redirect_chain: Vec<String>,
}

impl PartialDownloadIdentity {
    pub(crate) fn validate_for_resume(
        &self,
        manifest: &RuntimeManifest,
        actual_partial_bytes: u64,
        response: &ResumeResponseIdentity,
        policy: &DownloadPolicy,
    ) -> Result<(), String> {
        manifest.validate_pinned()?;
        if self.manifest_id != manifest.manifest_id
            || self.source_url != manifest.source_url
            || self.expected_bytes != manifest.asset_bytes
            || self.expected_sha256 != manifest.sha256
            || self.received_bytes != actual_partial_bytes
            || self.received_bytes == 0
            || self.received_bytes >= self.expected_bytes
        {
            return Err("The partial runtime download identity is invalid or stale.".into());
        }
        if self.etag.trim().is_empty() || self.etag.starts_with("W/") {
            return Err("The partial runtime download lacks a strong source validator.".into());
        }
        policy.validate_chain(manifest, &response.redirect_chain)?;
        if response.status != 206
            || response.content_range_start != Some(self.received_bytes)
            || response.content_range_total != Some(self.expected_bytes)
            || response.content_length
                != Some(self.expected_bytes.saturating_sub(self.received_bytes))
            || response.etag.as_deref() != Some(self.etag.as_str())
        {
            return Err(
                "The runtime source changed or did not honor the validated resume request.".into(),
            );
        }
        Ok(())
    }
}

pub(crate) fn copy_and_verify_asset(
    reader: impl Read,
    writer: impl Write,
    manifest: &RuntimeManifest,
) -> Result<(), String> {
    manifest.validate_pinned()?;
    copy_and_verify(reader, writer, manifest.asset_bytes, &manifest.sha256)
}

fn copy_and_verify(
    mut reader: impl Read,
    mut writer: impl Write,
    expected_bytes: u64,
    expected_sha256: &str,
) -> Result<(), String> {
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|_| "Could not read the runtime download.".to_owned())?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or("The runtime download size overflowed.")?;
        if total > expected_bytes {
            return Err("The runtime download exceeded the exact byte limit.".into());
        }
        hasher.update(&buffer[..read]);
        writer
            .write_all(&buffer[..read])
            .map_err(|_| "Could not write the staged runtime download.".to_owned())?;
    }
    if total != expected_bytes {
        return Err("The runtime download byte count did not match the manifest.".into());
    }
    let digest = format!("{:x}", hasher.finalize());
    if digest != expected_sha256 {
        return Err("The runtime download SHA-256 did not match the manifest.".into());
    }
    Ok(())
}

pub(crate) fn verify_asset_file(path: &Path, manifest: &RuntimeManifest) -> Result<(), String> {
    let file =
        File::open(path).map_err(|_| "Could not open the staged runtime archive.".to_owned())?;
    copy_and_verify_asset(file, io::sink(), manifest)
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TransactionState {
    Downloading,
    Verified,
    Extracting,
    Extracted,
    Promoting,
    Installed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InstallationTransaction {
    pub schema_version: u32,
    pub installation_id: String,
    pub manifest_id: String,
    pub manifest_sha256: String,
    pub state: TransactionState,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InstallationRecord {
    pub schema_version: u32,
    pub installation_id: String,
    pub manifest_id: String,
    pub manifest_sha256: String,
    pub runtime_id: String,
    pub runtime_version: String,
    pub archive_sha256: String,
    pub relative_root: String,
    pub selected_model_id: String,
}

#[derive(Clone, Debug)]
pub(crate) struct ManagedStorage {
    app_root: PathBuf,
    runtimes_root: PathBuf,
    models_root: PathBuf,
    staging_root: PathBuf,
}

impl ManagedStorage {
    pub(crate) fn new(app_root: &Path) -> Result<Self, String> {
        if !app_root.is_absolute() {
            return Err("The managed-runtime application-data root must be absolute.".into());
        }
        let app_root = app_root.to_path_buf();
        Ok(Self {
            runtimes_root: app_root.join("image-runtimes"),
            models_root: app_root.join("image-models"),
            staging_root: app_root.join("image-runtime-staging"),
            app_root,
        })
    }

    pub(crate) fn runtime_path(
        &self,
        runtime_version: &str,
        installation_id: &str,
    ) -> Result<PathBuf, String> {
        validate_storage_component(runtime_version)?;
        validate_storage_component(installation_id)?;
        Ok(self
            .runtimes_root
            .join(runtime_version)
            .join(installation_id))
    }

    pub(crate) fn staging_path(&self, installation_id: &str) -> Result<PathBuf, String> {
        validate_storage_component(installation_id)?;
        Ok(self.staging_root.join(installation_id))
    }

    pub(crate) fn models_root(&self) -> &Path {
        &self.models_root
    }

    pub(crate) fn validate_owned_path(&self, path: &Path) -> Result<(), String> {
        if !path.starts_with(&self.app_root)
            || (!path.starts_with(&self.runtimes_root) && !path.starts_with(&self.staging_root))
            || path.starts_with(&self.models_root)
        {
            return Err("The managed-runtime path is outside AIIDE-owned runtime storage.".into());
        }
        Ok(())
    }
}

fn validate_storage_component(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 80
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        || value == "."
        || value == ".."
    {
        return Err("The managed-runtime storage identity is invalid.".into());
    }
    Ok(())
}

impl InstallationRecord {
    pub(crate) fn validate(
        &self,
        manifest: &RuntimeManifest,
        storage: &ManagedStorage,
        install_path: &Path,
    ) -> Result<(), String> {
        manifest.validate_pinned()?;
        validate_storage_component(&self.installation_id)?;
        if self.schema_version != MANAGED_RUNTIME_SCHEMA
            || self.manifest_id != manifest.manifest_id
            || self.manifest_sha256 != manifest.identity_sha256()?
            || self.runtime_id != manifest.runtime_id
            || self.runtime_version != manifest.runtime_version
            || self.archive_sha256 != manifest.sha256
            || self.relative_root != manifest.expected_root
            || self.selected_model_id.trim().is_empty()
            || install_path != storage.runtime_path(&self.runtime_version, &self.installation_id)?
        {
            return Err("The managed-runtime installation record is invalid.".into());
        }
        storage.validate_owned_path(install_path)?;
        let root = install_path.join(&self.relative_root);
        for relative in &manifest.required_files {
            let required = install_path.join(relative);
            if !required.starts_with(&root) {
                return Err(
                    "The managed-runtime manifest contains an invalid required path.".into(),
                );
            }
            let metadata = fs::symlink_metadata(&required)
                .map_err(|_| "The managed-runtime installation is incomplete.".to_owned())?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(
                    "The managed-runtime installation contains an invalid required file.".into(),
                );
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum RecoveryAction {
    RemoveStaging,
    ResumeVerification,
    ValidatePromotedInstallation,
    UseInstalled,
    RecordFailure,
}

pub(crate) fn recovery_action(
    state: &TransactionState,
    staging_exists: bool,
    final_exists: bool,
) -> RecoveryAction {
    match (state, staging_exists, final_exists) {
        (TransactionState::Installed, _, true) => RecoveryAction::UseInstalled,
        (TransactionState::Promoting, _, true) => RecoveryAction::ValidatePromotedInstallation,
        (
            TransactionState::Verified | TransactionState::Extracting | TransactionState::Extracted,
            true,
            false,
        ) => RecoveryAction::ResumeVerification,
        (TransactionState::Downloading, true, false) => RecoveryAction::RemoveStaging,
        (TransactionState::Failed, true, false) => RecoveryAction::RemoveStaging,
        _ => RecoveryAction::RecordFailure,
    }
}

pub(crate) fn write_json_atomic(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or("The transaction record has no parent directory.")?;
    fs::create_dir_all(parent)
        .map_err(|_| "Could not prepare the transaction directory.".to_owned())?;
    let temporary = parent.join(format!(
        ".{}.{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("record"),
        std::process::id(),
        NEXT_RECORD_WRITE.fetch_add(1, Ordering::Relaxed),
    ));
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|_| "Could not encode the transaction record.".to_owned())?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|_| "Could not create the transaction record.".to_owned())?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "Could not persist the transaction record.".to_owned())?;
    if let Err(error) = replace_file_atomic(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("Could not promote the transaction record: {error}"));
    }
    Ok(())
}

#[cfg(windows)]
fn replace_file_atomic(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: both buffers are null-terminated and live for the duration of the call.
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file_atomic(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

pub(crate) fn promote_installation(
    storage: &ManagedStorage,
    staging_path: &Path,
    final_path: &Path,
    transaction: &mut InstallationTransaction,
    record: &InstallationRecord,
) -> Result<(), String> {
    storage.validate_owned_path(staging_path)?;
    storage.validate_owned_path(final_path)?;
    if transaction.state != TransactionState::Extracted
        || final_path.exists()
        || !staging_path.is_dir()
    {
        return Err("The managed-runtime installation is not ready for promotion.".into());
    }
    write_json_atomic(&staging_path.join("aiide-installation.json"), record)?;
    transaction.state = TransactionState::Promoting;
    let transaction_path = storage
        .staging_root
        .join(format!("{}.transaction.json", transaction.installation_id));
    write_json_atomic(&transaction_path, transaction)?;
    let parent = final_path
        .parent()
        .ok_or("The managed-runtime final path has no parent.")?;
    fs::create_dir_all(parent)
        .map_err(|_| "Could not prepare managed-runtime storage.".to_owned())?;
    fs::rename(staging_path, final_path)
        .map_err(|_| "Could not atomically promote the verified managed runtime.".to_owned())?;
    transaction.state = TransactionState::Installed;
    write_json_atomic(&transaction_path, transaction)?;
    Ok(())
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ArchiveLimits {
    pub maximum_entries: usize,
    pub maximum_entry_bytes: u64,
    pub maximum_total_bytes: u64,
    pub expected_total_bytes: Option<u64>,
    pub maximum_path_chars: usize,
    pub maximum_depth: usize,
}

impl ArchiveLimits {
    pub(crate) fn pinned() -> Self {
        Self {
            maximum_entries: 70_000,
            maximum_entry_bytes: 600_000_000,
            maximum_total_bytes: PINNED_INSTALLED_BYTES,
            expected_total_bytes: Some(PINNED_INSTALLED_BYTES),
            maximum_path_chars: 240,
            maximum_depth: 24,
        }
    }
}

#[derive(Clone, Debug)]
struct ValidatedArchiveEntry {
    path: PathBuf,
    is_directory: bool,
    size: u64,
}

fn validate_archive_path(
    name: &str,
    expected_root: &str,
    limits: &ArchiveLimits,
) -> Result<PathBuf, String> {
    if name.is_empty()
        || name.len() > limits.maximum_path_chars
        || name.starts_with(['/', '\\'])
        || name.contains('\\')
        || name.contains(':')
        || name.chars().any(char::is_control)
    {
        return Err("The runtime archive contains an unsafe Windows path.".into());
    }
    let path = Path::new(name);
    let mut depth = 0_usize;
    let mut first = None;
    for component in path.components() {
        let Component::Normal(component) = component else {
            return Err("The runtime archive contains path traversal or an absolute path.".into());
        };
        let component = component
            .to_str()
            .ok_or("The runtime archive contains a non-Unicode path.")?;
        if first.is_none() {
            first = Some(component);
        }
        depth += 1;
        if component.ends_with(['.', ' ']) || is_reserved_windows_name(component) {
            return Err("The runtime archive contains an invalid Windows path component.".into());
        }
    }
    if depth == 0 || depth > limits.maximum_depth || first != Some(expected_root) {
        return Err("The runtime archive does not have the expected bounded root layout.".into());
    }
    Ok(path.to_path_buf())
}

fn is_reserved_windows_name(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or(component)
        .trim_end_matches(['.', ' ']);
    let upper = stem.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && matches!(upper.as_bytes()[3], b'1'..=b'9'))
}

fn validate_archive_entry(
    entry: &sevenz_rust2::ArchiveEntry,
    expected_root: &str,
    limits: &ArchiveLimits,
) -> Result<ValidatedArchiveEntry, String> {
    let path = validate_archive_path(entry.name(), expected_root, limits)?;
    if entry.is_anti_item {
        return Err("The runtime archive contains an anti-item.".into());
    }
    if entry.size > limits.maximum_entry_bytes {
        return Err("The runtime archive contains an oversized entry.".into());
    }
    if entry.is_directory && entry.has_stream {
        return Err("The runtime archive contains an unexpected directory stream.".into());
    }
    if !entry.is_directory && !entry.has_stream && entry.size != 0 {
        return Err("The runtime archive contains an unexpected entry type.".into());
    }
    if entry.has_windows_attributes {
        const FILE_ATTRIBUTE_DEVICE: u32 = 0x40;
        const FILE_ATTRIBUTE_SPARSE_FILE: u32 = 0x200;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        let attributes = entry.windows_attributes;
        if attributes
            & (FILE_ATTRIBUTE_DEVICE | FILE_ATTRIBUTE_SPARSE_FILE | FILE_ATTRIBUTE_REPARSE_POINT)
            != 0
        {
            return Err(
                "The runtime archive contains a device, sparse file, or reparse point.".into(),
            );
        }
        let unix_kind = (attributes >> 16) & 0xf000;
        if unix_kind != 0 && unix_kind != if entry.is_directory { 0x4000 } else { 0x8000 } {
            return Err(
                "The runtime archive contains a link or unsupported Unix entry type.".into(),
            );
        }
    }
    Ok(ValidatedArchiveEntry {
        path,
        is_directory: entry.is_directory,
        size: entry.size,
    })
}

pub(crate) fn extract_verified_archive(
    archive_path: &Path,
    destination: &Path,
    manifest: &RuntimeManifest,
    limits: ArchiveLimits,
) -> Result<(), String> {
    manifest.validate_pinned()?;
    if destination.exists() {
        return Err("The runtime extraction destination must be new and empty.".into());
    }
    let mut reader =
        sevenz_rust2::ArchiveReader::open(archive_path, sevenz_rust2::Password::empty())
            .map_err(|error| format!("Could not open the verified runtime archive: {error}"))?;
    let entries = &reader.archive().files;
    if entries.len() > limits.maximum_entries {
        return Err("The runtime archive exceeds the entry-count limit.".into());
    }
    let mut seen = HashSet::with_capacity(entries.len());
    let mut total = 0_u64;
    let mut validated = HashMap::with_capacity(entries.len());
    for entry in entries {
        let entry = validate_archive_entry(entry, &manifest.expected_root, &limits)?;
        let key = entry.path.to_string_lossy().to_lowercase();
        if !seen.insert(key) {
            return Err("The runtime archive contains duplicate Windows paths.".into());
        }
        if !entry.is_directory {
            total = total
                .checked_add(entry.size)
                .ok_or("The runtime archive size overflowed.")?;
            if total > limits.maximum_total_bytes {
                return Err("The runtime archive exceeds the extraction byte limit.".into());
            }
        }
        validated.insert(entry.path.to_string_lossy().into_owned(), entry);
    }
    if limits
        .expected_total_bytes
        .is_some_and(|expected| total != expected)
    {
        return Err(
            "The runtime archive installed size does not match the manifest evidence.".into(),
        );
    }
    fs::create_dir(destination)
        .map_err(|_| "Could not create the bounded runtime staging directory.".to_owned())?;
    let extraction = reader.for_each_entries(|entry, source| {
        let expected = validated
            .get(entry.name())
            .ok_or_else(|| sevenz_rust2::Error::Other("Unvalidated archive entry".into()))?;
        let output = destination.join(&expected.path);
        if !output.starts_with(destination) {
            return Err(sevenz_rust2::Error::Other(
                "Archive path escaped staging".into(),
            ));
        }
        if expected.is_directory {
            fs::create_dir_all(&output)?;
        } else {
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&output)?;
            let mut bounded = source.take(expected.size.saturating_add(1));
            let written = io::copy(&mut bounded, &mut file)?;
            if written != expected.size {
                return Err(sevenz_rust2::Error::Other(
                    "Archive entry size mismatch".into(),
                ));
            }
            file.sync_data()?;
        }
        Ok(true)
    });
    if let Err(error) = extraction {
        let _ = fs::remove_dir_all(destination);
        return Err(format!(
            "The verified runtime archive could not be safely extracted: {error}"
        ));
    }
    for required in &manifest.required_files {
        let required = destination.join(required);
        let metadata = fs::symlink_metadata(&required)
            .map_err(|_| "The extracted runtime is missing a required file.".to_owned())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            let _ = fs::remove_dir_all(destination);
            return Err("The extracted runtime contains an invalid required file.".into());
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RestartBudget {
    maximum_restarts: u8,
    attempts: u8,
}

impl RestartBudget {
    pub(crate) fn new(maximum_restarts: u8) -> Self {
        Self {
            maximum_restarts,
            attempts: 0,
        }
    }

    pub(crate) fn claim(&mut self) -> Result<(), String> {
        if self.attempts >= self.maximum_restarts {
            return Err("The managed engine restart limit has been reached.".into());
        }
        self.attempts += 1;
        Ok(())
    }
}

#[cfg(windows)]
pub(crate) mod windows_supervisor;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

    fn fixture(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "aiide-runtime-test-{}-{name}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path.canonicalize().unwrap()
    }

    #[test]
    fn pinned_manifest_rejects_tampering_and_is_not_authenticated() {
        let manifest = RuntimeManifest::pinned();
        manifest.validate_pinned().unwrap();
        assert_eq!(manifest.asset_bytes, PINNED_ASSET_BYTES);
        assert_eq!(manifest.sha256, PINNED_SHA256);
        assert!(!managed_image_runtime_status().authenticated_manifest);

        let mut tampered = manifest;
        tampered.asset_bytes += 1;
        assert!(tampered.validate_pinned().is_err());
    }

    #[test]
    fn acquisition_and_execution_fail_closed_at_the_release_gate() {
        assert!(require_acquisition_gate().unwrap_err().contains("disabled"));
        assert!(require_execution_gate().unwrap_err().contains("disabled"));
    }

    #[test]
    fn acquisition_consent_is_bound_to_the_exact_manifest() {
        let manifest = RuntimeManifest::pinned();
        let mut consent = AcquisitionConsent {
            manifest_id: manifest.manifest_id.clone(),
            manifest_sha256: manifest.identity_sha256().unwrap(),
            accepted_at_unix_ms: 1,
        };
        consent.validate(&manifest).unwrap();
        consent.manifest_sha256 = "changed".into();
        assert!(consent.validate(&manifest).is_err());
    }

    #[test]
    fn redirect_policy_is_https_only_and_host_bounded() {
        let manifest = RuntimeManifest::pinned();
        let policy = DownloadPolicy::default();
        policy
            .validate_chain(
                &manifest,
                &[
                    manifest.source_url.clone(),
                    "https://release-assets.githubusercontent.com/github-production-release-asset/589831718/test?sig=temporary".into(),
                ],
            )
            .unwrap();
        for final_url in [
            "http://release-assets.githubusercontent.com/github-production-release-asset/589831718/test",
            "https://example.com/runtime.7z",
            "https://release-assets.githubusercontent.com/another/path",
        ] {
            assert!(policy
                .validate_chain(&manifest, &[manifest.source_url.clone(), final_url.into()])
                .is_err());
        }
    }

    #[test]
    fn exact_byte_and_hash_mismatches_fail_closed() {
        let bytes = b"synthetic runtime";
        let digest = hex_sha256(bytes);
        copy_and_verify(Cursor::new(bytes), io::sink(), bytes.len() as u64, &digest).unwrap();
        assert!(copy_and_verify(
            Cursor::new(bytes),
            io::sink(),
            bytes.len() as u64 + 1,
            &digest,
        )
        .unwrap_err()
        .contains("byte count"));
        assert!(copy_and_verify(
            Cursor::new(bytes),
            io::sink(),
            bytes.len() as u64,
            PINNED_SHA256,
        )
        .unwrap_err()
        .contains("SHA-256"));
    }

    #[test]
    fn invalid_resume_is_rejected() {
        let manifest = RuntimeManifest::pinned();
        let partial = PartialDownloadIdentity {
            manifest_id: manifest.manifest_id.clone(),
            source_url: manifest.source_url.clone(),
            expected_bytes: manifest.asset_bytes,
            expected_sha256: manifest.sha256.clone(),
            received_bytes: 100,
            etag: "strong-etag".into(),
        };
        let valid_chain = vec![
            manifest.source_url.clone(),
            "https://release-assets.githubusercontent.com/github-production-release-asset/589831718/test?sig=x".into(),
        ];
        let mut response = ResumeResponseIdentity {
            status: 206,
            content_range_start: Some(100),
            content_range_total: Some(manifest.asset_bytes),
            content_length: Some(manifest.asset_bytes - 100),
            etag: Some("strong-etag".into()),
            redirect_chain: valid_chain,
        };
        partial
            .validate_for_resume(&manifest, 100, &response, &DownloadPolicy::default())
            .unwrap();
        response.etag = Some("changed".into());
        assert!(partial
            .validate_for_resume(&manifest, 100, &response, &DownloadPolicy::default())
            .is_err());
        assert!(partial
            .validate_for_resume(&manifest, 99, &response, &DownloadPolicy::default())
            .is_err());
    }

    #[test]
    fn runtime_and_model_storage_are_separate_and_contained() {
        let root = fixture("storage");
        let storage = ManagedStorage::new(&root).unwrap();
        let runtime = storage.runtime_path("0.36.0", "install-1").unwrap();
        assert!(runtime.starts_with(root.join("image-runtimes")));
        assert!(!runtime.starts_with(storage.models_root()));
        storage.validate_owned_path(&runtime).unwrap();
        assert!(storage.validate_owned_path(storage.models_root()).is_err());
        assert!(storage.runtime_path("../outside", "install-1").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn transaction_recovery_is_deterministic() {
        assert_eq!(
            recovery_action(&TransactionState::Downloading, true, false),
            RecoveryAction::RemoveStaging
        );
        assert_eq!(
            recovery_action(&TransactionState::Extracted, true, false),
            RecoveryAction::ResumeVerification
        );
        assert_eq!(
            recovery_action(&TransactionState::Promoting, false, true),
            RecoveryAction::ValidatePromotedInstallation
        );
        assert_eq!(
            recovery_action(&TransactionState::Installed, false, true),
            RecoveryAction::UseInstalled
        );
        assert_eq!(
            recovery_action(&TransactionState::Installed, false, false),
            RecoveryAction::RecordFailure
        );
    }

    #[test]
    fn verified_installation_promotes_atomically_and_validates_required_files() {
        let root = fixture("promotion");
        let storage = ManagedStorage::new(&root).unwrap();
        let manifest = RuntimeManifest::pinned();
        let staging = storage.staging_path("install-1").unwrap();
        for required in &manifest.required_files {
            let path = staging.join(required);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"fixture").unwrap();
        }
        fs::create_dir_all(storage.models_root()).unwrap();
        fs::write(
            storage.models_root().join("retained-model.fixture"),
            b"model",
        )
        .unwrap();
        let final_path = storage.runtime_path("0.36.0", "install-1").unwrap();
        let record = InstallationRecord {
            schema_version: MANAGED_RUNTIME_SCHEMA,
            installation_id: "install-1".into(),
            manifest_id: manifest.manifest_id.clone(),
            manifest_sha256: manifest.identity_sha256().unwrap(),
            runtime_id: manifest.runtime_id.clone(),
            runtime_version: manifest.runtime_version.clone(),
            archive_sha256: manifest.sha256.clone(),
            relative_root: manifest.expected_root.clone(),
            selected_model_id: "acceptance-fixture-only".into(),
        };
        let mut transaction = InstallationTransaction {
            schema_version: MANAGED_RUNTIME_SCHEMA,
            installation_id: record.installation_id.clone(),
            manifest_id: record.manifest_id.clone(),
            manifest_sha256: record.manifest_sha256.clone(),
            state: TransactionState::Extracted,
        };
        promote_installation(&storage, &staging, &final_path, &mut transaction, &record).unwrap();
        assert_eq!(transaction.state, TransactionState::Installed);
        assert!(!staging.exists());
        let persisted: InstallationTransaction = serde_json::from_slice(
            &fs::read(root.join("image-runtime-staging/install-1.transaction.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(persisted.state, TransactionState::Installed);
        record.validate(&manifest, &storage, &final_path).unwrap();
        assert!(storage
            .models_root()
            .join("retained-model.fixture")
            .exists());
        fs::remove_file(final_path.join(&manifest.required_files[0])).unwrap();
        assert!(record.validate(&manifest, &storage, &final_path).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn archive_paths_reject_windows_escape_and_alias_classes() {
        let limits = ArchiveLimits {
            maximum_entries: 10,
            maximum_entry_bytes: 100,
            maximum_total_bytes: 100,
            expected_total_bytes: None,
            maximum_path_chars: 100,
            maximum_depth: 5,
        };
        for path in [
            "../escape",
            "/absolute",
            "C:/drive",
            "ComfyUI_windows_portable\\backslash",
            "ComfyUI_windows_portable/../escape",
            "ComfyUI_windows_portable/file:ads",
            "ComfyUI_windows_portable/CON.txt",
            "ComfyUI_windows_portable/trailing. ",
        ] {
            assert!(
                validate_archive_path(path, PINNED_ROOT, &limits).is_err(),
                "{path}"
            );
        }
        validate_archive_path(
            "ComfyUI_windows_portable/ComfyUI/main.py",
            PINNED_ROOT,
            &limits,
        )
        .unwrap();
    }

    #[test]
    fn archive_entry_rejects_links_reparse_sparse_and_oversize() {
        let limits = ArchiveLimits {
            maximum_entries: 10,
            maximum_entry_bytes: 5,
            maximum_total_bytes: 10,
            expected_total_bytes: None,
            maximum_path_chars: 100,
            maximum_depth: 5,
        };
        let mut entry = sevenz_rust2::ArchiveEntry::new_file("ComfyUI_windows_portable/file.bin");
        entry.size = 6;
        assert!(validate_archive_entry(&entry, PINNED_ROOT, &limits).is_err());
        entry.size = 1;
        entry.has_windows_attributes = true;
        entry.windows_attributes = 0x400;
        assert!(validate_archive_entry(&entry, PINNED_ROOT, &limits).is_err());
        entry.windows_attributes = 0xa000 << 16;
        assert!(validate_archive_entry(&entry, PINNED_ROOT, &limits).is_err());
    }

    fn write_archive(path: &Path, entries: &[(&str, &[u8])]) {
        let mut writer = sevenz_rust2::ArchiveWriter::create(path).unwrap();
        writer.set_encrypt_header(false);
        for (name, bytes) in entries {
            writer
                .push_archive_entry(
                    sevenz_rust2::ArchiveEntry::new_file(name),
                    Some(Cursor::new(bytes.to_vec())),
                )
                .unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn tiny_synthetic_archive_extracts_only_after_complete_preflight() {
        let root = fixture("archive-safe");
        let archive = root.join("fixture.7z");
        write_archive(
            &archive,
            &[
                (
                    "ComfyUI_windows_portable/python_embeded/python.exe",
                    b"python",
                ),
                ("ComfyUI_windows_portable/ComfyUI/main.py", b"main"),
            ],
        );
        let destination = root.join("extracted");
        extract_verified_archive(
            &archive,
            &destination,
            &RuntimeManifest::pinned(),
            ArchiveLimits {
                maximum_entries: 4,
                maximum_entry_bytes: 16,
                maximum_total_bytes: 16,
                expected_total_bytes: Some(10),
                maximum_path_chars: 120,
                maximum_depth: 6,
            },
        )
        .unwrap();
        assert_eq!(
            fs::read(destination.join("ComfyUI_windows_portable/ComfyUI/main.py")).unwrap(),
            b"main"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tiny_synthetic_archive_with_dangerous_path_writes_nothing() {
        let root = fixture("archive-dangerous");
        let archive = root.join("fixture.7z");
        let escape_name = format!(
            "aiide-escape-{}-{}.exe",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        );
        let archive_name = format!("../{escape_name}");
        write_archive(&archive, &[(archive_name.as_str(), b"bad")]);
        let destination = root.join("extracted");
        assert!(extract_verified_archive(
            &archive,
            &destination,
            &RuntimeManifest::pinned(),
            ArchiveLimits {
                maximum_entries: 4,
                maximum_entry_bytes: 16,
                maximum_total_bytes: 16,
                expected_total_bytes: None,
                maximum_path_chars: 120,
                maximum_depth: 6,
            },
        )
        .is_err());
        assert!(!destination.exists());
        assert!(!root.parent().unwrap().join(escape_name).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn restart_budget_is_explicitly_bounded() {
        let mut budget = RestartBudget::new(2);
        budget.claim().unwrap();
        budget.claim().unwrap();
        assert!(budget.claim().is_err());
    }
}
