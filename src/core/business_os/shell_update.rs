// Origin: CTOX
// License: AGPL-3.0-only

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use chrono::Utc;
use flate2::read::GzDecoder;
use ring::signature::{UnparsedPublicKey, ED25519};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use uuid::Uuid;

const CHANNEL_URL: &str = "https://github.com/metric-space-ai/ctox/releases/download/business-os-shell-channel-stable/business-os-shell-stable.json";
const STATE_SCHEMA: &str = "ctox.business-os-shell.state.v1";
const MAX_CHANNEL_BYTES: usize = 64 * 1024;
const MAX_MANIFEST_BYTES: usize = 8 * 1024 * 1024;
const MAX_ARCHIVE_BYTES: usize = 512 * 1024 * 1024;
const MAX_EXTRACTED_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_EXTRACTED_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const CURRENT_KEY: &str = "MCowBQYDK2VwAyEAZECH2XB0VlZWQ7zUzoChyiRkKtfGNK9HmSMvZQuwGjk=";
const NEXT_KEY: &str = "MCowBQYDK2VwAyEAdAgcqbHB2Sr86KzrWcdYxKCxb6Ofz4sVxhkEhTgvo7s=";
const REQUIRED_SHELL_FILES: &[&str] = &[
    "index.html",
    "app.js",
    "app.css",
    "mobile-host.js",
    "mobile-host.css",
    "system-apps.json",
    "standard-app-bundle.json",
    "shared/shell-release-status.js",
];
type VerifiedReleaseCell = Arc<Mutex<Option<Arc<super::shell_assets::VerifiedRelease>>>>;
static VERIFIED_RELEASES: OnceLock<Mutex<BTreeMap<(PathBuf, String), VerifiedReleaseCell>>> =
    OnceLock::new();

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ChannelPayload {
    r#type: String,
    channel: String,
    version: String,
    manifest_url: String,
    manifest_sha256: String,
    published_at: String,
    signing_key_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ChannelPointer {
    #[serde(flatten)]
    payload: ChannelPayload,
    signature: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Artifact {
    url: String,
    size: u64,
    sha256: String,
    content_type: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Compatibility {
    workjet_min_version: String,
    workjet_max_version: Option<String>,
    ctox_min_version: String,
    ctox_max_version: Option<String>,
    shell_protocol: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReleaseFile {
    path: String,
    size: u64,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Provenance {
    embedded_manifest_sha256: String,
    sbom_url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReleasePayload {
    r#type: String,
    version: String,
    channel: String,
    source_commit: String,
    published_at: String,
    artifact: Artifact,
    compatibility: Compatibility,
    files: Vec<ReleaseFile>,
    provenance: Provenance,
    signing_key_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReleaseManifest {
    #[serde(flatten)]
    payload: ReleasePayload,
    signature: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ShellRollbackTarget {
    NoPreviousTarget,
    BuiltInRecovery,
    Slot { version: String },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellUpdateState {
    schema: String,
    channel: String,
    active_version: Option<String>,
    desired_version: Option<String>,
    current_slot: Option<String>,
    previous_slot: Option<String>,
    // Missing only in legacy state. Never infer a historical recovery target
    // from active_version/current_slot being absent.
    #[serde(default)]
    rollback_target: Option<ShellRollbackTarget>,
    latest_compatible_version: Option<String>,
    last_checked_at: Option<String>,
    last_activated_at: Option<String>,
    phase: String,
    health: String,
    error_code: Option<String>,
    rollback_active: bool,
}

impl Default for ShellUpdateState {
    fn default() -> Self {
        Self {
            schema: STATE_SCHEMA.to_owned(),
            channel: "stable".to_owned(),
            active_version: None,
            desired_version: None,
            current_slot: None,
            previous_slot: None,
            rollback_target: Some(ShellRollbackTarget::NoPreviousTarget),
            latest_compatible_version: None,
            last_checked_at: None,
            last_activated_at: None,
            phase: "recovery".to_owned(),
            health: "unknown".to_owned(),
            error_code: None,
            rollback_active: false,
        }
    }
}

impl ShellUpdateState {
    fn rollback_target(&self) -> ShellRollbackTarget {
        self.rollback_target.clone().unwrap_or_else(|| {
            self.previous_slot
                .clone()
                .map_or(ShellRollbackTarget::NoPreviousTarget, |version| {
                    ShellRollbackTarget::Slot { version }
                })
        })
    }

    fn set_rollback_target(&mut self, target: ShellRollbackTarget) {
        self.previous_slot = match &target {
            ShellRollbackTarget::Slot { version } => Some(version.clone()),
            _ => None,
        };
        self.rollback_target = Some(target);
    }
}

fn update_root(root: &Path) -> PathBuf {
    root.join("runtime").join("business-os-shell")
}

fn slots_root(root: &Path) -> PathBuf {
    update_root(root).join("slots")
}

fn read_state(root: &Path) -> Result<ShellUpdateState> {
    let conn = super::store::open_store(root)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS business_os_shell_update_state (
            singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
            state_json TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );",
    )?;
    let serialized = conn
        .query_row(
            "SELECT state_json FROM business_os_shell_update_state WHERE singleton = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(serialized) = serialized else {
        return Ok(ShellUpdateState::default());
    };
    let state: ShellUpdateState = serde_json::from_str(&serialized)?;
    anyhow::ensure!(
        state.schema == STATE_SCHEMA,
        "unsupported shell update state"
    );
    let target_slot = match state.rollback_target() {
        ShellRollbackTarget::Slot { version } => Some(version),
        _ => None,
    };
    anyhow::ensure!(
        state.previous_slot == target_slot,
        "inconsistent shell rollback target"
    );
    Ok(state)
}

fn write_state(root: &Path, state: &ShellUpdateState) -> Result<()> {
    let conn = super::store::open_store(root)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS business_os_shell_update_state (
            singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
            state_json TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );",
    )?;
    conn.execute(
        "INSERT INTO business_os_shell_update_state(singleton, state_json, updated_at)
         VALUES (1, ?1, ?2)
         ON CONFLICT(singleton) DO UPDATE SET state_json = excluded.state_json,
          updated_at = excluded.updated_at",
        params![serde_json::to_string(state)?, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn read_response(response: ureq::Response, limit: usize) -> Result<Vec<u8>> {
    anyhow::ensure!(
        response.get_url().starts_with("https://"),
        "insecure shell redirect"
    );
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        !bytes.is_empty() && bytes.len() <= limit,
        "shell response exceeds limit"
    );
    Ok(bytes)
}

fn fetch(url: &str, limit: usize) -> Result<Vec<u8>> {
    anyhow::ensure!(url.starts_with("https://"), "shell URL must use HTTPS");
    let response = ureq::get(url)
        .set("Accept", "application/json, application/gzip")
        .set("Cache-Control", "no-store")
        .call()?;
    read_response(response, limit)
}

fn trusted_key(key_id: &str) -> Result<Vec<u8>> {
    let encoded = match key_id {
        "shell-current-2026-08" => CURRENT_KEY,
        "shell-next-2026-08" => NEXT_KEY,
        _ => bail!("shell-release-unknown-key"),
    };
    let spki = base64::engine::general_purpose::STANDARD.decode(encoded)?;
    anyhow::ensure!(spki.len() == 44, "invalid Ed25519 SPKI key");
    Ok(spki[12..].to_vec())
}

fn verify_signature<T: Serialize>(payload: &T, signature: &str, key_id: &str) -> Result<()> {
    anyhow::ensure!(signature.len() == 128, "invalid Ed25519 signature length");
    let signature = (0..signature.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&signature[index..index + 2], 16))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let bytes = serde_json::to_vec(payload)?;
    UnparsedPublicKey::new(&ED25519, trusted_key(key_id)?)
        .verify(&bytes, &signature)
        .map_err(|_| anyhow!("shell-release-invalid-signature"))
}

fn version_tuple(version: &str) -> Result<(u64, u64, u64)> {
    let core = version.split(['-', '+']).next().unwrap_or(version);
    let values = core
        .split('.')
        .map(str::parse::<u64>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    anyhow::ensure!(values.len() == 3, "invalid semantic version");
    Ok((values[0], values[1], values[2]))
}

fn compatible(manifest: &ReleaseManifest) -> Result<bool> {
    if manifest.payload.compatibility.shell_protocol != "workjet.business-os-shell.v1" {
        return Ok(false);
    }
    let current = version_tuple(env!("CARGO_PKG_VERSION"))?;
    let minimum = version_tuple(&manifest.payload.compatibility.ctox_min_version)?;
    let maximum = manifest
        .payload
        .compatibility
        .ctox_max_version
        .as_deref()
        .map(version_tuple)
        .transpose()?;
    Ok(current >= minimum && maximum.is_none_or(|maximum| current <= maximum))
}

fn phase_after_check(
    state: &ShellUpdateState,
    offered_version: &str,
    is_compatible: bool,
) -> &'static str {
    if !is_compatible {
        "incompatible"
    } else if state.active_version.as_deref() != Some(offered_version) {
        "available"
    } else if state.phase == "restart_required" || state.health == "pending_restart" {
        // A release check is not proof that the newly selected slot was served
        // by a restarted backend. Keep the explicit restart gate until
        // `active_shell_root` verifies and consumes the slot on server start.
        "restart_required"
    } else {
        "current"
    }
}

fn resolve_release() -> Result<ReleaseManifest> {
    let channel_bytes = fetch(CHANNEL_URL, MAX_CHANNEL_BYTES)?;
    let channel: ChannelPointer = serde_json::from_slice(&channel_bytes)?;
    anyhow::ensure!(channel.payload.r#type == "ctox.business-os-shell.channel.v1");
    anyhow::ensure!(channel.payload.channel == "stable");
    verify_signature(
        &channel.payload,
        &channel.signature,
        &channel.payload.signing_key_id,
    )?;
    let manifest_bytes = fetch(&channel.payload.manifest_url, MAX_MANIFEST_BYTES)?;
    anyhow::ensure!(sha256(&manifest_bytes) == channel.payload.manifest_sha256);
    let manifest: ReleaseManifest = serde_json::from_slice(&manifest_bytes)?;
    anyhow::ensure!(manifest.payload.r#type == "ctox.business-os-shell.release.v2");
    anyhow::ensure!(manifest.payload.version == channel.payload.version);
    anyhow::ensure!(manifest.payload.channel == channel.payload.channel);
    verify_signature(
        &manifest.payload,
        &manifest.signature,
        &manifest.payload.signing_key_id,
    )?;
    Ok(manifest)
}

fn pinned_release_url(version: &str) -> Result<String> {
    // Only an immutable release in our repository can be selected. Never accept
    // arbitrary URLs, paths or a channel override from the operator.
    let valid = regex::Regex::new(
        r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$",
    )?;
    anyhow::ensure!(
        version.len() <= 128 && valid.is_match(version),
        "invalid pinned shell version"
    );
    Ok(format!("https://github.com/metric-space-ai/ctox/releases/download/business-os-shell-v{version}/ctox-business-os-shell-{version}.release.v2.json"))
}

fn verify_pinned_manifest(bytes: &[u8], version: &str) -> Result<ReleaseManifest> {
    pinned_release_url(version)?;
    let manifest: ReleaseManifest = serde_json::from_slice(bytes)?;
    anyhow::ensure!(manifest.payload.r#type == "ctox.business-os-shell.release.v2");
    anyhow::ensure!(
        manifest.payload.version == version,
        "pinned shell version mismatch"
    );
    anyhow::ensure!(matches!(
        manifest.payload.channel.as_str(),
        "stable" | "beta" | "nightly"
    ));
    verify_signature(
        &manifest.payload,
        &manifest.signature,
        &manifest.payload.signing_key_id,
    )?;
    Ok(manifest)
}

fn resolve_pinned_release(version: &str) -> Result<ReleaseManifest> {
    let bytes = fetch(&pinned_release_url(version)?, MAX_MANIFEST_BYTES)?;
    verify_pinned_manifest(&bytes, version)
}

fn safe_relative(path: &str) -> Result<PathBuf> {
    anyhow::ensure!(!path.is_empty() && !path.contains('\\') && !path.starts_with('/'));
    let candidate = Path::new(path);
    for component in candidate.components() {
        anyhow::ensure!(
            matches!(component, Component::Normal(_)),
            "unsafe archive path"
        );
    }
    Ok(candidate.to_path_buf())
}

fn tar_string(field: &[u8]) -> Result<String> {
    let end = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());
    Ok(std::str::from_utf8(&field[..end])?.to_owned())
}

fn tar_octal(field: &[u8]) -> Result<u64> {
    let value = tar_string(field)?.trim().to_owned();
    Ok(if value.is_empty() {
        0
    } else {
        u64::from_str_radix(&value, 8)?
    })
}

fn verify_tar_header(header: &[u8; 512]) -> Result<()> {
    anyhow::ensure!(&header[257..263] == b"ustar\0", "invalid USTAR header");
    let expected = tar_octal(&header[148..156])?;
    let actual = header
        .iter()
        .enumerate()
        .map(|(index, byte)| {
            if (148..156).contains(&index) {
                u64::from(b' ')
            } else {
                u64::from(*byte)
            }
        })
        .sum::<u64>();
    anyhow::ensure!(actual == expected, "invalid USTAR header checksum");
    Ok(())
}

fn verify_release_inventory(manifest: &ReleaseManifest) -> Result<BTreeMap<String, &ReleaseFile>> {
    let mut expected = BTreeMap::new();
    let mut total = 0u64;
    for file in &manifest.payload.files {
        safe_relative(&file.path)?;
        anyhow::ensure!(file.path != "ctox-shell-manifest.json");
        anyhow::ensure!(
            file.size <= MAX_EXTRACTED_FILE_BYTES,
            "shell file too large"
        );
        total = total
            .checked_add(file.size)
            .context("shell inventory overflow")?;
        anyhow::ensure!(
            expected.insert(file.path.clone(), file).is_none(),
            "duplicate shell inventory path"
        );
    }
    anyhow::ensure!(
        total <= MAX_EXTRACTED_TOTAL_BYTES,
        "shell inventory too large"
    );
    for required in REQUIRED_SHELL_FILES {
        anyhow::ensure!(
            expected.contains_key(*required),
            "shell inventory missing required runtime file: {required}"
        );
    }
    Ok(expected)
}

fn extract_archive(bytes: &[u8], destination: &Path, manifest: &ReleaseManifest) -> Result<()> {
    let expected_root = format!("ctox-business-os-shell-{}", manifest.payload.version);
    let expected = verify_release_inventory(manifest)?;
    let mut observed = BTreeSet::new();
    let mut embedded_manifest_observed = false;
    let mut decoder = GzDecoder::new(Cursor::new(bytes));
    loop {
        let mut header = [0u8; 512];
        decoder.read_exact(&mut header)?;
        if header.iter().all(|byte| *byte == 0) {
            break;
        }
        verify_tar_header(&header)?;
        let name = tar_string(&header[0..100])?;
        let prefix = tar_string(&header[345..500])?;
        let archive_path = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let size = tar_octal(&header[124..136])?;
        let type_flag = header[156];
        let relative = if archive_path == expected_root {
            ""
        } else {
            archive_path
                .strip_prefix(&format!("{expected_root}/"))
                .context("archive root mismatch")?
        };
        if type_flag == b'5' {
            anyhow::ensure!(size == 0, "directory entry contains data");
            if !relative.is_empty() {
                fs::create_dir_all(destination.join(safe_relative(relative)?))?;
            }
            continue;
        }
        anyhow::ensure!(
            type_flag == b'0' || type_flag == 0,
            "non-regular archive entry"
        );
        anyhow::ensure!(!relative.is_empty(), "archive root cannot be a file");
        let relative_path = safe_relative(relative)?;
        if relative == "ctox-shell-manifest.json" {
            anyhow::ensure!(
                size <= MAX_MANIFEST_BYTES as u64,
                "embedded manifest too large"
            );
            anyhow::ensure!(!embedded_manifest_observed, "duplicate embedded manifest");
        } else {
            let record = expected.get(relative).context("unmanifested shell file")?;
            anyhow::ensure!(record.size == size, "shell file size mismatch");
            anyhow::ensure!(!observed.contains(relative), "duplicate shell file");
        }
        let mut content = vec![0u8; usize::try_from(size)?];
        decoder.read_exact(&mut content)?;
        let padding = (512 - (size % 512)) % 512;
        if padding > 0 {
            let mut ignored = vec![0u8; usize::try_from(padding)?];
            decoder.read_exact(&mut ignored)?;
        }
        if relative == "ctox-shell-manifest.json" {
            anyhow::ensure!(
                sha256(&content) == manifest.payload.provenance.embedded_manifest_sha256
            );
            embedded_manifest_observed = true;
        } else {
            let record = expected.get(relative).context("unmanifested shell file")?;
            anyhow::ensure!(record.size == size && record.sha256 == sha256(&content));
            anyhow::ensure!(observed.insert(relative.to_owned()), "duplicate shell file");
        }
        let target = destination.join(relative_path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(target, content)?;
    }
    anyhow::ensure!(
        observed.len() == expected.len(),
        "shell inventory incomplete"
    );
    anyhow::ensure!(embedded_manifest_observed, "embedded manifest missing");
    anyhow::ensure!(
        destination.join("index.html").is_file(),
        "shell smoke test failed"
    );
    Ok(())
}

fn verify_slot(slot: &Path, expected_version: Option<&str>) -> Result<ReleaseManifest> {
    let manifest_path = slot.join(".ctox-shell-release.v2.json");
    let manifest: ReleaseManifest = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    anyhow::ensure!(manifest.payload.r#type == "ctox.business-os-shell.release.v2");
    if let Some(version) = expected_version {
        anyhow::ensure!(
            manifest.payload.version == version,
            "shell slot version mismatch"
        );
    }
    verify_signature(
        &manifest.payload,
        &manifest.signature,
        &manifest.payload.signing_key_id,
    )?;
    anyhow::ensure!(compatible(&manifest)?, "shell slot is incompatible");
    let expected = verify_release_inventory(&manifest)?;
    let mut observed = BTreeSet::new();
    collect_slot_files(slot, slot, &mut observed)?;
    let mut expected_paths = expected.keys().cloned().collect::<BTreeSet<_>>();
    expected_paths.insert("ctox-shell-manifest.json".to_owned());
    expected_paths.insert(".ctox-shell-release.v2.json".to_owned());
    anyhow::ensure!(observed == expected_paths, "shell slot inventory mismatch");
    let embedded_path = slot.join("ctox-shell-manifest.json");
    anyhow::ensure!(embedded_path.is_file(), "embedded manifest missing");
    anyhow::ensure!(
        sha256(&fs::read(embedded_path)?) == manifest.payload.provenance.embedded_manifest_sha256,
        "embedded manifest hash mismatch"
    );
    for (relative, record) in &expected {
        let path = slot.join(safe_relative(relative)?);
        let metadata = fs::metadata(&path).context("shell slot file missing")?;
        anyhow::ensure!(metadata.is_file() && metadata.len() == record.size);
        anyhow::ensure!(
            sha256(&fs::read(path)?) == record.sha256,
            "shell slot hash mismatch"
        );
    }
    anyhow::ensure!(slot.join("index.html").is_file(), "shell smoke test failed");
    Ok(manifest)
}

fn collect_slot_files(root: &Path, directory: &Path, files: &mut BTreeSet<String>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        anyhow::ensure!(
            !metadata.file_type().is_symlink(),
            "shell slot contains symlink"
        );
        if metadata.is_dir() {
            collect_slot_files(root, &entry.path(), files)?;
        } else {
            anyhow::ensure!(metadata.is_file(), "shell slot contains special file");
            let relative = entry
                .path()
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            safe_relative(&relative)?;
            anyhow::ensure!(files.insert(relative), "duplicate shell slot file");
        }
    }
    Ok(())
}

pub(super) fn shell_build_from_app_js(source: &str) -> Option<String> {
    ["const APP_BUILD = '", "const APP_BUILD = \""]
        .into_iter()
        .find_map(|marker| {
            let tail = source.split_once(marker)?.1;
            let quote = marker.chars().last()?;
            let build = tail.split_once(quote)?.0.trim();
            (!build.is_empty()).then(|| build.to_owned())
        })
}

fn shell_build_matches(requested: &str, build: &str) -> bool {
    requested
        .strip_prefix(build)
        .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with('_'))
}

fn shell_slot_candidate_for_build(
    state: &ShellUpdateState,
    slots: &Path,
    requested: &str,
) -> Result<Option<(String, PathBuf)>> {
    let mut candidates = Vec::new();
    for slot in [&state.current_slot, &state.previous_slot]
        .into_iter()
        .flatten()
    {
        if candidates.contains(slot) {
            continue;
        }
        candidates.push(slot.clone());
        let path = slots.join(slot);
        let source = match fs::read_to_string(path.join("app.js")) {
            Ok(source) => source,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        let Some(build) = shell_build_from_app_js(&source) else {
            continue;
        };
        if shell_build_matches(requested, &build) {
            return Ok(Some((build, path)));
        }
    }
    Ok(None)
}

/// Resolve a browser's immutable shell-generation token inside this CTOX
/// instance. Only the atomically tracked current/previous slots participate;
/// ctox.dev and other delivery surfaces never select or own a shell release.
pub(super) fn verified_shell_root_for_build(
    root: &Path,
    requested: &str,
) -> Result<Option<(String, PathBuf)>> {
    let state = read_state(root)?;
    let Some((build, path)) = shell_slot_candidate_for_build(&state, &slots_root(root), requested)?
    else {
        return Ok(None);
    };
    let slot = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("shell slot path has no valid version")?;
    Ok(verified_release(root, slot)?.map(|release| (build, release.root.clone())))
}

/// Only a selected signed slot can establish an immutable document base.
/// Source/recovery trees cannot claim the identity of an installed release.
pub(super) fn verified_release_for_root(
    root: &Path,
    app_root: &Path,
) -> Result<Option<Arc<super::shell_assets::VerifiedRelease>>> {
    if app_root.parent() != Some(slots_root(root).as_path()) {
        return Ok(None);
    }
    let version = app_root
        .file_name()
        .and_then(|value| value.to_str())
        .context("shell slot has no release version")?;
    verified_release(root, version)
}

/// Resolve a signed version independently of the mutable current/previous pointer.
/// All retained slots remain addressable. Only successful admission is cached;
/// bytes are checked against that admitted inventory on every immutable read.
pub(super) fn verified_release(
    root: &Path,
    version: &str,
) -> Result<Option<Arc<super::shell_assets::VerifiedRelease>>> {
    super::shell_assets::validate_version(version)?;
    let path = slots_root(root).join(version);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    anyhow::ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "invalid shell release directory"
    );
    let cache = VERIFIED_RELEASES.get_or_init(|| Mutex::new(BTreeMap::new()));
    let cell = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .entry((root.to_path_buf(), version.to_owned()))
        .or_insert_with(|| Arc::new(Mutex::new(None)))
        .clone();
    // Single admission per version without blocking unrelated instance/releases.
    let mut admitted = cell.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(release) = admitted.as_ref() {
        return Ok(Some(Arc::clone(release)));
    }
    let manifest = verify_slot(&path, Some(version))?;
    let mut files: BTreeMap<_, _> = manifest
        .payload
        .files
        .iter()
        .map(|file| {
            (
                file.path.clone(),
                super::shell_assets::InventoryFile {
                    size: file.size,
                    sha256: file.sha256.clone(),
                },
            )
        })
        .collect();
    files.insert(
        "ctox-shell-manifest.json".to_owned(),
        super::shell_assets::InventoryFile {
            size: fs::metadata(path.join("ctox-shell-manifest.json"))?.len(),
            sha256: manifest.payload.provenance.embedded_manifest_sha256.clone(),
        },
    );
    let release = Arc::new(super::shell_assets::VerifiedRelease::admitted(
        &path,
        manifest.payload.version,
        manifest.payload.source_commit,
        manifest.payload.artifact.sha256,
        files,
    )?);
    *admitted = Some(Arc::clone(&release));
    Ok(Some(release))
}

fn public_status(state: &ShellUpdateState, administrable: bool) -> serde_json::Value {
    let phase = if state.phase == "restart_required" {
        "restart"
    } else {
        state.phase.as_str()
    };
    let health = match state.health.as_str() {
        "healthy" => "healthy",
        "degraded" | "pending_restart" => "degraded",
        _ => "unknown",
    };
    serde_json::json!({
        "activeVersion": state.active_version,
        "desiredVersion": state.desired_version,
        "latestCompatibleVersion": state.latest_compatible_version,
        "channel": state.channel,
        "phase": phase,
        "health": health,
        "administrable": administrable,
        "recoveryShell": state.current_slot.is_none(),
        "currentSlot": state.current_slot,
        "previousSlot": state.previous_slot,
        "rollbackSupported": true,
        "rollbackTarget": state.rollback_target(),
        "rollbackRequiresSlotVerification": matches!(
            state.rollback_target(), ShellRollbackTarget::Slot { .. }),
        "lastCheckedAt": state.last_checked_at,
        "lastActivatedAt": state.last_activated_at,
        "errorCode": state.error_code,
        "pause": null,
    })
}

pub fn status(root: &Path, administrable: bool) -> Result<serde_json::Value> {
    Ok(public_status(&read_state(root)?, administrable))
}

pub fn record_audit(root: &Path, action: &str, outcome: &str, actor_id: &str) -> Result<()> {
    anyhow::ensure!(matches!(
        action,
        "check" | "stage" | "activate" | "rollback"
    ));
    anyhow::ensure!(matches!(outcome, "succeeded" | "failed"));
    anyhow::ensure!(!actor_id.is_empty() && actor_id.len() <= 256);
    let conn = super::store::open_store(root)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS business_os_shell_update_audit (
            id TEXT PRIMARY KEY,
            action TEXT NOT NULL,
            outcome TEXT NOT NULL,
            actor_id TEXT NOT NULL,
            created_at TEXT NOT NULL
        );",
    )?;
    conn.execute(
        "INSERT INTO business_os_shell_update_audit(id, action, outcome, actor_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            Uuid::new_v4().to_string(),
            action,
            outcome,
            actor_id,
            Utc::now().to_rfc3339()
        ],
    )?;
    Ok(())
}

pub fn check(root: &Path) -> Result<serde_json::Value> {
    let mut state = read_state(root)?;
    state.phase = "checking".to_owned();
    state.error_code = None;
    write_state(root, &state)?;
    match (|| {
        let manifest = resolve_release()?;
        let is_compatible = compatible(&manifest)?;
        Ok::<_, anyhow::Error>((manifest, is_compatible))
    })() {
        Ok((manifest, is_compatible)) => {
            state.last_checked_at = Some(Utc::now().to_rfc3339());
            state.latest_compatible_version =
                is_compatible.then(|| manifest.payload.version.clone());
            state.phase =
                phase_after_check(&state, &manifest.payload.version, is_compatible).to_owned();
            write_state(root, &state)?;
            Ok(public_status(&state, true))
        }
        Err(error) => {
            state.phase = "failed".to_owned();
            state.error_code = Some("release_check_failed".to_owned());
            write_state(root, &state)?;
            Err(error)
        }
    }
}

pub fn stage(root: &Path) -> Result<serde_json::Value> {
    stage_selected(root, None)
}

/// Stage one signed release for tenant-scoped acceptance, without publishing or
/// changing the subscribed Stable channel. Activation remains a separate step.
pub fn stage_version(root: &Path, version: &str) -> Result<serde_json::Value> {
    pinned_release_url(version)?;
    stage_selected(root, Some(version))
}

fn stage_selected(root: &Path, version: Option<&str>) -> Result<serde_json::Value> {
    let mut state = read_state(root)?;
    state.phase = "download".to_owned();
    state.error_code = None;
    write_state(root, &state)?;
    match stage_inner(root, version) {
        Ok(state) => Ok(public_status(&state, true)),
        Err(error) => {
            let mut state = read_state(root)?;
            state.phase = "failed".to_owned();
            state.error_code = Some("release_stage_failed".to_owned());
            write_state(root, &state)?;
            Err(error)
        }
    }
}

fn stage_inner(root: &Path, version: Option<&str>) -> Result<ShellUpdateState> {
    let manifest = match version {
        Some(version) => resolve_pinned_release(version)?,
        None => resolve_release()?,
    };
    anyhow::ensure!(compatible(&manifest)?, "shell release is incompatible");
    let bytes = fetch(&manifest.payload.artifact.url, MAX_ARCHIVE_BYTES)?;
    anyhow::ensure!(bytes.len() as u64 == manifest.payload.artifact.size);
    anyhow::ensure!(sha256(&bytes) == manifest.payload.artifact.sha256);
    let mut state = read_state(root)?;
    state.phase = "verify".to_owned();
    write_state(root, &state)?;
    let slots = slots_root(root);
    fs::create_dir_all(&slots)?;
    let staging = slots.join(format!(".stage-{}", Uuid::new_v4()));
    fs::create_dir(&staging)?;
    let result = extract_archive(&bytes, &staging, &manifest);
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    fs::write(
        staging.join(".ctox-shell-release.v2.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    let slot = slots.join(&manifest.payload.version);
    if slot.exists() {
        let existing = verify_slot(&slot, Some(&manifest.payload.version))?;
        fs::remove_dir_all(&staging)?;
        anyhow::ensure!(
            existing.payload.artifact.sha256 == manifest.payload.artifact.sha256,
            "a shell release version cannot be reused for different artifact bytes"
        );
    } else {
        fs::rename(&staging, &slot)?;
    }
    let mut state = read_state(root)?;
    apply_staging(&mut state, manifest.payload.version, version.is_some());
    write_state(root, &state)?;
    Ok(state)
}

fn apply_staging(state: &mut ShellUpdateState, version: String, pinned: bool) {
    state.desired_version = Some(version.clone());
    if !pinned {
        state.latest_compatible_version = Some(version);
    }
    state.last_checked_at = Some(Utc::now().to_rfc3339());
    state.phase = "ready".to_owned();
    state.error_code = None;
}

pub fn activate(root: &Path) -> Result<serde_json::Value> {
    let mut state = read_state(root)?;
    let version = state
        .desired_version
        .clone()
        .context("no staged shell release")?;
    let slot = slots_root(root).join(&version);
    verify_slot(&slot, Some(&version))?;
    apply_activation(&mut state, version);
    write_state(root, &state)?;
    Ok(public_status(&state, true))
}

fn apply_activation(state: &mut ShellUpdateState, version: String) {
    // This records the actual selection at this transition, not a guess about
    // historical state. None is precisely the native built-in fallback selector.
    let previous = state
        .current_slot
        .take()
        .map_or(ShellRollbackTarget::BuiltInRecovery, |version| {
            ShellRollbackTarget::Slot { version }
        });
    state.set_rollback_target(previous);
    state.current_slot = Some(version.clone());
    state.active_version = Some(version);
    state.desired_version = None;
    state.last_activated_at = Some(Utc::now().to_rfc3339());
    state.phase = "restart_required".to_owned();
    state.health = "pending_restart".to_owned();
    state.rollback_active = false;
}

pub fn rollback(root: &Path) -> Result<serde_json::Value> {
    let mut state = read_state(root)?;
    let previous = state.rollback_target();
    match &previous {
        ShellRollbackTarget::NoPreviousTarget => bail!("no previous shell target"),
        ShellRollbackTarget::BuiltInRecovery => {}
        ShellRollbackTarget::Slot { version } => {
            pinned_release_url(version)?;
            let previous_path = slots_root(root).join(version);
            verify_slot(&previous_path, Some(version))?;
        }
    }
    apply_rollback(&mut state, previous);
    write_state(root, &state)?;
    Ok(public_status(&state, true))
}

fn apply_rollback(state: &mut ShellUpdateState, previous: ShellRollbackTarget) {
    let selected = match previous {
        ShellRollbackTarget::Slot { version } => Some(version),
        ShellRollbackTarget::BuiltInRecovery => None,
        ShellRollbackTarget::NoPreviousTarget => unreachable!("rollback target was checked"),
    };
    let current = state
        .current_slot
        .take()
        .map_or(ShellRollbackTarget::BuiltInRecovery, |version| {
            ShellRollbackTarget::Slot { version }
        });
    state.current_slot = selected.clone();
    state.active_version = selected;
    state.set_rollback_target(current);
    state.desired_version = None;
    state.phase = "restart_required".to_owned();
    state.health = "pending_restart".to_owned();
    state.rollback_active = true;
    state.last_activated_at = Some(Utc::now().to_rfc3339());
}

pub fn active_shell_root(root: &Path) -> Result<Option<PathBuf>> {
    let mut state = read_state(root)?;
    let Some(ref slot) = state.current_slot else {
        // Complete a persisted rollback to the native fallback on restart,
        // without claiming that unverified built-in assets passed slot checks.
        if state.phase == "restart_required" && state.rollback_active {
            state.phase = "recovery".to_owned();
            state.health = "unknown".to_owned();
            write_state(root, &state)?;
        }
        return Ok(None);
    };
    let path = slots_root(root).join(&slot);
    verify_slot(&path, Some(&slot))?;
    if state.phase == "restart_required" {
        state.phase = if state.rollback_active {
            "rollback"
        } else {
            "current"
        }
        .to_owned();
        state.health = "healthy".to_owned();
        write_state(root, &state)?;
    }
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest(
        ctox_min_version: &str,
        ctox_max_version: Option<&str>,
        signing_key_id: &str,
    ) -> ReleaseManifest {
        ReleaseManifest {
            payload: ReleasePayload {
                r#type: "ctox.business-os-shell.release.v2".to_owned(),
                version: "1.2.3".to_owned(),
                channel: "stable".to_owned(),
                source_commit: "0123456789abcdef0123456789abcdef01234567".to_owned(),
                published_at: "2026-08-26T00:00:00Z".to_owned(),
                artifact: Artifact {
                    url: "https://example.invalid/shell.tar.gz".to_owned(),
                    size: 1,
                    sha256: "00".repeat(32),
                    content_type: "application/gzip".to_owned(),
                },
                compatibility: Compatibility {
                    workjet_min_version: "0.0.0".to_owned(),
                    workjet_max_version: None,
                    ctox_min_version: ctox_min_version.to_owned(),
                    ctox_max_version: ctox_max_version.map(str::to_owned),
                    shell_protocol: "workjet.business-os-shell.v1".to_owned(),
                },
                files: REQUIRED_SHELL_FILES
                    .iter()
                    .map(|path| ReleaseFile {
                        path: (*path).to_owned(),
                        size: 1,
                        sha256: sha256(b"x"),
                    })
                    .collect(),
                provenance: Provenance {
                    embedded_manifest_sha256: "00".repeat(32),
                    sbom_url: "https://example.invalid/sbom.json".to_owned(),
                },
                signing_key_id: signing_key_id.to_owned(),
            },
            signature: "00".repeat(64),
        }
    }

    #[test]
    fn state_is_persisted_in_the_typed_business_os_store() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let mut state = read_state(temp.path())?;
        state.active_version = Some("1.1.0".to_owned());
        state.current_slot = Some("1.1.0".to_owned());
        state.set_rollback_target(ShellRollbackTarget::Slot {
            version: "1.0.0".to_owned(),
        });
        state.phase = "current".to_owned();
        write_state(temp.path(), &state)?;
        let persisted = read_state(temp.path())?;
        assert_eq!(persisted.active_version.as_deref(), Some("1.1.0"));
        assert_eq!(persisted.current_slot.as_deref(), Some("1.1.0"));
        assert_eq!(persisted.previous_slot.as_deref(), Some("1.0.0"));
        assert_eq!(persisted.phase, "current");
        assert!(!update_root(temp.path()).join("state.json").exists());
        let public = public_status(&persisted, false);
        assert_eq!(public["activeVersion"], "1.1.0");
        assert_eq!(public["administrable"], false);
        assert_eq!(public["recoveryShell"], false);
        Ok(())
    }

    #[test]
    fn unsafe_archive_paths_are_rejected() {
        assert!(safe_relative("../escape").is_err());
        assert!(safe_relative("/absolute").is_err());
        assert!(safe_relative("safe/index.html").is_ok());
    }

    #[test]
    fn ustar_checksum_is_fail_closed() -> Result<()> {
        let mut header = [0u8; 512];
        header[257..263].copy_from_slice(b"ustar\0");
        header[148..156].fill(b' ');
        let checksum = header.iter().map(|byte| u64::from(*byte)).sum::<u64>();
        let encoded = format!("{checksum:06o}\0 ");
        header[148..156].copy_from_slice(encoded.as_bytes());
        verify_tar_header(&header)?;
        header[0] = 1;
        assert!(verify_tar_header(&header).is_err());
        Ok(())
    }

    #[test]
    fn release_signatures_and_unknown_keys_fail_closed() {
        let unknown = sample_manifest("0.0.0", None, "shell-unknown");
        let error = verify_signature(
            &unknown.payload,
            &unknown.signature,
            &unknown.payload.signing_key_id,
        )
        .expect_err("unknown key must fail");
        assert!(format!("{error:#}").contains("shell-release-unknown-key"));

        let tampered = sample_manifest("0.0.0", None, "shell-current-2026-08");
        let error = verify_signature(
            &tampered.payload,
            &tampered.signature,
            &tampered.payload.signing_key_id,
        )
        .expect_err("invalid signature must fail");
        assert!(format!("{error:#}").contains("shell-release-invalid-signature"));
    }

    #[test]
    fn pinned_release_accepts_only_repository_versions() {
        for version in ["0.1.46", "0.1.46-beta.1", "0.1.46-nightly.20260905"] {
            assert!(pinned_release_url(version)
                .unwrap()
                .ends_with(&format!("ctox-business-os-shell-{version}.release.v2.json")));
        }
        for version in [
            "",
            "../0.1.46",
            "https://example.com/0.1.46",
            "v0.1.46",
            "01.1.46",
            "0.1.46/other",
            "0.1.46?x",
            "0.1.46-beta..1",
            "0.1.46\n",
        ] {
            assert!(pinned_release_url(version).is_err(), "accepted {version:?}");
        }
    }

    #[test]
    fn pinned_release_keeps_signature_and_identity_gates() -> Result<()> {
        let manifest = sample_manifest("0.0.0", None, "shell-current-2026-08");
        let bytes = serde_json::to_vec(&manifest)?;
        let mismatch = verify_pinned_manifest(&bytes, "1.2.4").unwrap_err();
        assert!(mismatch.to_string().contains("version mismatch"));
        let invalid = verify_pinned_manifest(&bytes, "1.2.3").unwrap_err();
        assert!(invalid.to_string().contains("invalid-signature"));
        Ok(())
    }

    #[test]
    fn pinned_staging_does_not_activate_or_change_the_stable_offer() {
        let mut state = ShellUpdateState {
            active_version: Some("0.1.44".to_owned()),
            current_slot: Some("0.1.44".to_owned()),
            previous_slot: Some("0.1.43".to_owned()),
            latest_compatible_version: Some("0.1.44".to_owned()),
            ..ShellUpdateState::default()
        };
        apply_staging(&mut state, "0.1.46-beta.1".to_owned(), true);
        assert_eq!(state.desired_version.as_deref(), Some("0.1.46-beta.1"));
        assert_eq!(state.active_version.as_deref(), Some("0.1.44"));
        assert_eq!(state.current_slot.as_deref(), Some("0.1.44"));
        assert_eq!(state.previous_slot.as_deref(), Some("0.1.43"));
        assert_eq!(state.latest_compatible_version.as_deref(), Some("0.1.44"));
        assert_eq!(state.channel, "stable");
        apply_staging(&mut state, "0.1.46".to_owned(), false);
        assert_eq!(state.latest_compatible_version.as_deref(), Some("0.1.46"));
    }

    #[test]
    fn compatibility_honours_minimum_and_maximum_ctox_versions() -> Result<()> {
        let mut unknown = sample_manifest("0.0.0", None, "shell-current-2026-08");
        unknown.payload.compatibility.shell_protocol = "future.unsupported.protocol".to_owned();
        assert!(!compatible(&unknown)?);
        assert!(compatible(&sample_manifest(
            "0.0.0",
            None,
            "shell-current-2026-08"
        ))?);
        assert!(!compatible(&sample_manifest(
            "999.0.0",
            None,
            "shell-current-2026-08"
        ))?);
        assert!(!compatible(&sample_manifest(
            "0.0.0",
            Some("0.0.0"),
            "shell-current-2026-08"
        ))?);
        Ok(())
    }

    #[test]
    fn inventory_rejects_duplicates_and_unsafe_paths() {
        let mut duplicate = sample_manifest("0.0.0", None, "shell-current-2026-08");
        duplicate
            .payload
            .files
            .push(duplicate.payload.files[0].clone());
        assert!(verify_release_inventory(&duplicate).is_err());

        let mut unsafe_path = sample_manifest("0.0.0", None, "shell-current-2026-08");
        unsafe_path.payload.files[0].path = "../index.html".to_owned();
        assert!(verify_release_inventory(&unsafe_path).is_err());
    }

    #[test]
    fn inventory_rejects_an_incomplete_runtime_bundle() {
        let mut incomplete = sample_manifest("0.0.0", None, "shell-current-2026-08");
        incomplete
            .payload
            .files
            .retain(|file| file.path != "app.css");
        let error = verify_release_inventory(&incomplete).expect_err("missing app.css must fail");
        assert!(format!("{error:#}").contains("missing required runtime file: app.css"));
    }

    #[test]
    fn previous_instance_slot_resolves_its_own_generation() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let slots = slots_root(temp.path());
        fs::create_dir_all(slots.join("1.1.0"))?;
        fs::create_dir_all(slots.join("1.0.0"))?;
        fs::write(
            slots.join("1.1.0/app.js"),
            "const APP_BUILD = '20260901-shell-v2-current-v400';\n",
        )?;
        fs::write(
            slots.join("1.0.0/app.js"),
            "const APP_BUILD = '20260831-shell-v2-previous-v399';\n",
        )?;
        let state = ShellUpdateState {
            current_slot: Some("1.1.0".to_owned()),
            previous_slot: Some("1.0.0".to_owned()),
            ..ShellUpdateState::default()
        };

        let (build, path) = shell_slot_candidate_for_build(
            &state,
            &slots,
            "20260831-shell-v2-previous-v399_knowledge-8",
        )?
        .context("previous generation must resolve")?;
        assert_eq!(build, "20260831-shell-v2-previous-v399");
        assert_eq!(path, slots.join("1.0.0"));
        assert!(
            shell_slot_candidate_for_build(&state, &slots, "20260830-shell-v2-unknown-v398")?
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn release_check_never_clears_the_restart_gate() {
        let mut state = ShellUpdateState {
            active_version: Some("1.2.3".to_owned()),
            phase: "restart_required".to_owned(),
            health: "pending_restart".to_owned(),
            ..ShellUpdateState::default()
        };
        assert_eq!(phase_after_check(&state, "1.2.3", true), "restart_required");
        state.phase = "current".to_owned();
        state.health = "healthy".to_owned();
        assert_eq!(phase_after_check(&state, "1.2.3", true), "current");
        assert_eq!(phase_after_check(&state, "1.2.4", true), "available");
        assert_eq!(phase_after_check(&state, "1.2.4", false), "incompatible");
    }

    #[test]
    fn activation_and_rollback_preserve_two_atomic_slots() {
        let mut state = ShellUpdateState {
            active_version: Some("1.0.0".to_owned()),
            current_slot: Some("1.0.0".to_owned()),
            desired_version: Some("1.1.0".to_owned()),
            phase: "ready".to_owned(),
            health: "healthy".to_owned(),
            ..ShellUpdateState::default()
        };
        apply_activation(&mut state, "1.1.0".to_owned());
        assert_eq!(state.active_version.as_deref(), Some("1.1.0"));
        assert_eq!(state.current_slot.as_deref(), Some("1.1.0"));
        assert_eq!(state.previous_slot.as_deref(), Some("1.0.0"));
        assert_eq!(state.phase, "restart_required");
        assert_eq!(state.health, "pending_restart");

        apply_rollback(
            &mut state,
            ShellRollbackTarget::Slot {
                version: "1.0.0".to_owned(),
            },
        );
        assert_eq!(state.active_version.as_deref(), Some("1.0.0"));
        assert_eq!(state.current_slot.as_deref(), Some("1.0.0"));
        assert_eq!(state.previous_slot.as_deref(), Some("1.1.0"));
        assert!(state.rollback_active);
    }

    #[test]
    fn failed_slot_verification_never_changes_the_active_slot() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let state = ShellUpdateState {
            active_version: Some("1.0.0".to_owned()),
            current_slot: Some("1.0.0".to_owned()),
            desired_version: Some("1.1.0".to_owned()),
            phase: "ready".to_owned(),
            health: "healthy".to_owned(),
            ..ShellUpdateState::default()
        };
        write_state(temp.path(), &state)?;
        fs::create_dir_all(slots_root(temp.path()).join("1.1.0"))?;
        fs::write(
            slots_root(temp.path()).join("1.1.0/index.html"),
            b"tampered",
        )?;
        assert!(activate(temp.path()).is_err());
        let persisted = read_state(temp.path())?;
        assert_eq!(persisted.active_version.as_deref(), Some("1.0.0"));
        assert_eq!(persisted.current_slot.as_deref(), Some("1.0.0"));
        assert_eq!(persisted.desired_version.as_deref(), Some("1.1.0"));
        Ok(())
    }

    #[test]
    fn shell_recovery_first_activation_survives_restart_and_rolls_back_to_builtin() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let mut state = read_state(temp.path())?;
        assert_eq!(
            state.rollback_target(),
            ShellRollbackTarget::NoPreviousTarget
        );
        assert!(rollback(temp.path()).is_err());

        // Exercise the transition after the existing activation verifier. The
        // public activation rejection test separately keeps that gate covered.
        apply_activation(&mut state, "1.2.3".to_owned());
        write_state(temp.path(), &state)?;
        drop(state);
        let restarted = read_state(temp.path())?;
        assert_eq!(
            restarted.rollback_target,
            Some(ShellRollbackTarget::BuiltInRecovery)
        );
        let before = status(temp.path(), true)?;
        assert_eq!(before["currentSlot"], "1.2.3");
        assert_eq!(before["rollbackTarget"]["kind"], "built_in_recovery");
        assert_eq!(before["rollbackRequiresSlotVerification"], false);

        let artifact = slots_root(temp.path()).join("1.2.3/retained-evidence");
        fs::create_dir_all(artifact.parent().unwrap())?;
        fs::write(&artifact, b"keep this slot")?;
        let result = rollback(temp.path())?;
        assert!(result["currentSlot"].is_null());
        assert!(result["activeVersion"].is_null());
        assert_eq!(result["recoveryShell"], true);
        assert_eq!(result["phase"], "restart");
        assert_eq!(result["rollbackTarget"]["kind"], "slot");
        assert_eq!(result["rollbackTarget"]["version"], "1.2.3");
        assert!(active_shell_root(temp.path())?.is_none());
        let after_restart = read_state(temp.path())?;
        assert!(after_restart.current_slot.is_none());
        assert!(after_restart.active_version.is_none());
        assert_eq!(after_restart.phase, "recovery");
        assert_eq!(after_restart.health, "unknown");
        assert!(after_restart.rollback_active);
        assert_eq!(fs::read(artifact)?, b"keep this slot");
        Ok(())
    }

    #[test]
    fn shell_recovery_slot_transitions_preserve_persisted_rollback_selection() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let mut state = ShellUpdateState {
            current_slot: Some("1.0.0".to_owned()),
            active_version: Some("1.0.0".to_owned()),
            ..ShellUpdateState::default()
        };
        apply_activation(&mut state, "1.1.0".to_owned());
        write_state(temp.path(), &state)?;
        let mut restarted = read_state(temp.path())?;
        let target = restarted.rollback_target();
        assert_eq!(
            target,
            ShellRollbackTarget::Slot {
                version: "1.0.0".to_owned()
            }
        );
        assert_eq!(restarted.previous_slot.as_deref(), Some("1.0.0"));
        apply_rollback(&mut restarted, target);
        write_state(temp.path(), &restarted)?;
        let restored = read_state(temp.path())?;
        assert_eq!(restored.current_slot.as_deref(), Some("1.0.0"));
        assert_eq!(restored.active_version.as_deref(), Some("1.0.0"));
        assert_eq!(
            restored.rollback_target(),
            ShellRollbackTarget::Slot {
                version: "1.1.0".to_owned()
            }
        );
        assert_eq!(restored.previous_slot.as_deref(), Some("1.1.0"));
        assert!(restored.rollback_active);
        assert_eq!(restored.phase, "restart_required");
        Ok(())
    }

    #[test]
    fn shell_recovery_legacy_state_never_infers_a_builtin_previous_target() -> Result<()> {
        let temp = tempfile::tempdir()?;
        for (current, previous) in [
            (None, None),
            (Some("1.2.3"), None),
            (Some("1.2.3"), Some("1.1.0")),
        ] {
            let state = ShellUpdateState {
                current_slot: current.map(str::to_owned),
                // Deliberately absent: this is NOT historical rollback evidence.
                active_version: None,
                previous_slot: previous.map(str::to_owned),
                rollback_target: None,
                ..ShellUpdateState::default()
            };
            write_state(temp.path(), &state)?;
            let mut legacy = serde_json::to_value(&state)?;
            legacy.as_object_mut().unwrap().remove("rollbackTarget");
            let conn = super::super::store::open_store(temp.path())?;
            conn.execute(
                "UPDATE business_os_shell_update_state SET state_json = ?1 WHERE singleton = 1",
                [serde_json::to_string(&legacy)?],
            )?;
            drop(conn);
            let restored = read_state(temp.path())?;
            let expected = previous.map_or(ShellRollbackTarget::NoPreviousTarget, |version| {
                ShellRollbackTarget::Slot {
                    version: version.to_owned(),
                }
            });
            assert_eq!(restored.rollback_target(), expected);
            assert_ne!(
                restored.rollback_target(),
                ShellRollbackTarget::BuiltInRecovery
            );
            let before = serde_json::to_value(&restored)?;
            assert!(rollback(temp.path()).is_err());
            assert_eq!(serde_json::to_value(read_state(temp.path())?)?, before);
            assert_eq!(
                status(temp.path(), true)?["recoveryShell"],
                current.is_none()
            );
        }
        Ok(())
    }

    #[test]
    fn shell_recovery_invalid_slot_rollback_never_mutates_persisted_state() -> Result<()> {
        let temp = tempfile::tempdir()?;
        for version in ["1.0.0", "../escape", "1.2.3"] {
            let mut state = ShellUpdateState {
                current_slot: Some("2.0.0".to_owned()),
                active_version: Some("2.0.0".to_owned()),
                ..ShellUpdateState::default()
            };
            state.set_rollback_target(ShellRollbackTarget::Slot {
                version: version.to_owned(),
            });
            if version == "1.2.3" {
                let slot = slots_root(temp.path()).join(version);
                fs::create_dir_all(&slot)?;
                fs::write(
                    slot.join(".ctox-shell-release.v2.json"),
                    serde_json::to_vec(&sample_manifest("0.0.0", None, "shell-current-2026-08"))?,
                )?;
            }
            write_state(temp.path(), &state)?;
            let before = serde_json::to_value(read_state(temp.path())?)?;
            let error = rollback(temp.path()).expect_err("invalid target must not select fallback");
            if version == "1.2.3" {
                assert!(format!("{error:#}").contains("invalid-signature"));
            }
            assert_eq!(serde_json::to_value(read_state(temp.path())?)?, before);
            let public = status(temp.path(), true)?;
            assert_eq!(public["rollbackTarget"]["kind"], "slot");
            assert_eq!(public["rollbackRequiresSlotVerification"], true);
            assert_eq!(public["recoveryShell"], false);
        }
        Ok(())
    }

    #[test]
    fn shell_recovery_inconsistent_or_unknown_target_fails_closed() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let state = ShellUpdateState {
            previous_slot: Some("1.0.0".to_owned()),
            rollback_target: Some(ShellRollbackTarget::BuiltInRecovery),
            ..ShellUpdateState::default()
        };
        write_state(temp.path(), &state)?;
        assert!(read_state(temp.path())
            .unwrap_err()
            .to_string()
            .contains("inconsistent"));
        assert!(rollback(temp.path()).is_err());
        let mut unknown = serde_json::to_value(&state)?;
        unknown["rollbackTarget"] = serde_json::json!({"kind": "automatic_recovery"});
        assert!(serde_json::from_value::<ShellUpdateState>(unknown).is_err());
        Ok(())
    }

    #[test]
    fn abandoned_staging_directory_is_never_an_active_slot() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let staging = slots_root(temp.path()).join(".stage-interrupted");
        fs::create_dir_all(&staging)?;
        fs::write(staging.join("index.html"), b"partial")?;
        let state = read_state(temp.path())?;
        assert!(state.active_version.is_none());
        assert!(state.current_slot.is_none());
        assert_eq!(public_status(&state, true)["phase"], "recovery");
        Ok(())
    }
}
