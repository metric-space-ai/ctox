use anyhow::Context;
use anyhow::Result;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::fs::OpenOptions;
use std::io::{copy, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use crate::inference::engine;
use crate::inference::runtime_env;
use crate::secrets;
use crate::service;

const INSTALL_MANIFEST_FILE_NAME: &str = "install_manifest.json";
const UPDATE_STATE_FILE_NAME: &str = "update_state.json";
const DEFAULT_INSTALL_ROOT_RELATIVE_PATH: &str = ".local/lib/ctox";
const DEFAULT_STATE_ROOT_RELATIVE_PATH: &str = ".local/state/ctox";
const DEFAULT_CACHE_ROOT_RELATIVE_PATH: &str = ".cache/ctox";
const DEFAULT_GITHUB_API_BASE: &str = "https://api.github.com";
const DEFAULT_GITHUB_TOKEN_ENV: &str = "CTOX_UPDATE_GITHUB_TOKEN";
const DEFAULT_RELEASE_REPO: &str = "metric-space-ai/ctox";
const UPGRADE_RUNTIME_ENV_INVARIANT_KEYS: &[&str] = &[
    "CTOX_API_PROVIDER",
    "CTOX_UPSTREAM_BASE_URL",
    "CTOX_AZURE_FOUNDRY_ENDPOINT",
    "CTOX_AZURE_FOUNDRY_DEPLOYMENT_ID",
    "CTOX_CHAT_MODEL",
    "CTOX_CHAT_MODEL_BASE",
    "CTOX_ACTIVE_MODEL",
    "CTOX_CHAT_MODEL_MAX_CONTEXT",
    "CTOX_CHAT_TURN_TIMEOUT_SECS",
];
const UPGRADE_SECRET_INVARIANT_KEYS: &[&str] = &[
    "AZURE_FOUNDRY_API_KEY",
    "OPENAI_API_KEY",
    "OPENROUTER_API_KEY",
    "ANTHROPIC_API_KEY",
    "MINIMAX_API_KEY",
    "CTOX_MISTRAL_API_KEY",
    "MISTRAL_API_KEY",
];
static UPGRADE_PROGRESS_STEP: AtomicUsize = AtomicUsize::new(0);
static UPGRADE_PROGRESS_STARTED: Mutex<Option<Instant>> = Mutex::new(None);

fn progress_reset() {
    UPGRADE_PROGRESS_STEP.store(0, Ordering::SeqCst);
    if let Ok(mut started) = UPGRADE_PROGRESS_STARTED.lock() {
        *started = Some(Instant::now());
    }
}

fn progress_step(label: impl AsRef<str>) {
    let step = UPGRADE_PROGRESS_STEP.fetch_add(1, Ordering::SeqCst) + 1;
    eprintln!(
        "{}",
        format_progress_step(progress_elapsed_secs(), step, label.as_ref())
    );
}

fn progress_info(label: impl AsRef<str>) {
    eprintln!(
        "{}",
        format_progress_info(progress_elapsed_secs(), label.as_ref())
    );
}

fn progress_done(label: impl AsRef<str>, started: Instant) {
    eprintln!(
        "{}",
        format_progress_done(
            progress_elapsed_secs(),
            label.as_ref(),
            started.elapsed().as_secs()
        )
    );
}

fn progress_elapsed_secs() -> u64 {
    UPGRADE_PROGRESS_STARTED
        .lock()
        .ok()
        .and_then(|started| started.as_ref().map(Instant::elapsed))
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

fn format_progress_step(elapsed_secs: u64, step: usize, label: &str) -> String {
    format!(
        "ctox upgrade | {} | step {step:02} | {label}",
        format_elapsed(elapsed_secs)
    )
}

fn format_progress_info(elapsed_secs: u64, label: &str) -> String {
    format!(
        "ctox upgrade | {} | info    | {label}",
        format_elapsed(elapsed_secs)
    )
}

fn format_progress_done(elapsed_secs: u64, label: &str, duration_secs: u64) -> String {
    format!(
        "ctox upgrade | {} | done    | {} | took {}",
        format_elapsed(elapsed_secs),
        label,
        format_duration(duration_secs)
    )
}

fn format_elapsed(total_secs: u64) -> String {
    let hours = total_secs / 3600;
    let minutes = (total_secs / 60) % 60;
    let seconds = total_secs % 60;
    if hours == 0 {
        format!("{minutes:02}:{seconds:02}")
    } else {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    }
}

fn format_duration(total_secs: u64) -> String {
    let minutes = total_secs / 60;
    let seconds = total_secs % 60;
    if minutes == 0 {
        format!("{seconds}s")
    } else {
        format!("{minutes}m {seconds:02}s")
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VersionInfo {
    pub version: String,
    pub install_mode: String,
    pub workspace_root: PathBuf,
    pub active_root: PathBuf,
    pub install_root: Option<PathBuf>,
    pub state_root: PathBuf,
    pub cache_root: PathBuf,
    pub current_release: Option<String>,
    pub previous_release: Option<String>,
    pub release_channel: Option<ReleaseChannelConfig>,
}

#[derive(Debug, Clone)]
pub struct InstallLayout {
    pub workspace_root: PathBuf,
    pub active_root: PathBuf,
    pub install_root: Option<PathBuf>,
    pub state_root: PathBuf,
    pub cache_root: PathBuf,
}

impl InstallLayout {
    pub fn resolve(root: &Path) -> Result<Self> {
        let workspace_root = root.to_path_buf();
        let active_root = resolve_active_root(root);
        let install_root = resolve_install_root(root);
        let state_root = resolve_state_root(root, install_root.as_deref())?;
        let cache_root = resolve_cache_root(root)?;
        Ok(Self {
            workspace_root,
            active_root,
            install_root,
            state_root,
            cache_root,
        })
    }

    pub fn managed(&self) -> bool {
        self.install_root.is_some()
    }

    pub fn install_manifest_path(&self) -> PathBuf {
        if let Some(install_root) = self.install_root.as_ref() {
            install_root.join(INSTALL_MANIFEST_FILE_NAME)
        } else {
            self.state_root.join(INSTALL_MANIFEST_FILE_NAME)
        }
    }

    pub fn update_state_path(&self) -> PathBuf {
        self.state_root.join(UPDATE_STATE_FILE_NAME)
    }

    pub fn update_lock_path(&self) -> PathBuf {
        self.state_root.join("update.lock")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstallManifest {
    pub schema_version: u32,
    pub install_root: PathBuf,
    pub state_root: PathBuf,
    pub current_release: Option<String>,
    pub previous_release: Option<String>,
    pub adopted_from: Option<PathBuf>,
    #[serde(default)]
    pub release_channel: Option<ReleaseChannelConfig>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReleaseChannelConfig {
    #[serde(rename = "github")]
    GitHub {
        repo: String,
        #[serde(default = "default_github_api_base_string")]
        api_base: String,
        #[serde(default)]
        token_env: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ReleaseChannelConfigRepr {
    Tagged {
        kind: ReleaseChannelKind,
        repo: String,
        #[serde(default = "default_github_api_base_string")]
        api_base: String,
        #[serde(default)]
        token_env: Option<String>,
    },
    Legacy {
        #[serde(rename = "GitHub")]
        github: LegacyGitHubReleaseChannel,
    },
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ReleaseChannelKind {
    Github,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyGitHubReleaseChannel {
    repo: String,
    #[serde(default = "default_github_api_base_string")]
    api_base: String,
    #[serde(default)]
    token_env: Option<String>,
}

impl<'de> Deserialize<'de> for ReleaseChannelConfig {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let repr = ReleaseChannelConfigRepr::deserialize(deserializer)?;
        Ok(match repr {
            ReleaseChannelConfigRepr::Tagged {
                kind: ReleaseChannelKind::Github,
                repo,
                api_base,
                token_env,
            } => Self::GitHub {
                repo,
                api_base,
                token_env,
            },
            ReleaseChannelConfigRepr::Legacy { github } => Self::GitHub {
                repo: github.repo,
                api_base: github.api_base,
                token_env: github.token_env,
            },
        })
    }
}

#[derive(Debug, Clone, Serialize)]
struct ResolvedReleaseChannel {
    config: ReleaseChannelConfig,
    source: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateState {
    pub schema_version: u32,
    pub phase: String,
    pub current_version: String,
    pub current_release: Option<String>,
    pub target_release: Option<String>,
    pub previous_release: Option<String>,
    pub source: Option<String>,
    pub state_backup_path: Option<PathBuf>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct UpdateStatus {
    version: String,
    managed_install: bool,
    workspace_root: PathBuf,
    active_root: PathBuf,
    install_root: Option<PathBuf>,
    state_root: PathBuf,
    cache_root: PathBuf,
    release_channel: Option<ReleaseChannelConfig>,
    manifest: Option<InstallManifest>,
    update: Option<UpdateState>,
}

#[derive(Debug, Clone, Serialize)]
struct RemoteUpdateCheck {
    configured: bool,
    status: String,
    reason: Option<String>,
    current_release: Option<String>,
    current_version: String,
    channel: Option<ReleaseChannelConfig>,
    latest_release: Option<String>,
    latest_name: Option<String>,
    published_at: Option<String>,
    release_url: Option<String>,
    source_tarball_url: Option<String>,
    update_available: bool,
}

#[derive(Debug, Clone, Serialize)]
struct AdoptResult {
    installed: bool,
    release: String,
    install_root: PathBuf,
    state_root: PathBuf,
    current_root: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
struct ApplyResult {
    updated: bool,
    release: String,
    current_root: PathBuf,
    previous_release: Option<String>,
    state_backup_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
struct RollbackResult {
    rolled_back: bool,
    current_release: String,
    previous_release: Option<String>,
    current_root: PathBuf,
}

#[derive(Debug)]
struct UpdateOperationLease {
    path: PathBuf,
}

impl Drop for UpdateOperationLease {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubReleaseResponse {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    tarball_url: String,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<GitHubReleaseAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubReleaseAsset {
    name: String,
    browser_download_url: String,
    #[serde(default, rename = "size")]
    _size: Option<u64>,
}

#[derive(Debug, Clone)]
struct DownloadedReleaseSource {
    release: GitHubReleaseResponse,
    extracted_root: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdateSourceKind {
    Source,
    Binary,
}

pub fn version_info(root: &Path) -> Result<VersionInfo> {
    let layout = InstallLayout::resolve(root)?;
    let manifest = load_install_manifest(&layout.install_manifest_path())?;
    let release_channel = resolve_release_channel(&layout, manifest.as_ref());
    Ok(VersionInfo {
        version: build_version().to_string(),
        install_mode: if layout.managed() {
            "managed".to_string()
        } else {
            "workspace".to_string()
        },
        workspace_root: layout.workspace_root,
        active_root: layout.active_root,
        install_root: layout.install_root,
        state_root: layout.state_root,
        cache_root: layout.cache_root,
        current_release: manifest
            .as_ref()
            .and_then(|entry| entry.current_release.clone()),
        previous_release: manifest
            .as_ref()
            .and_then(|entry| entry.previous_release.clone()),
        release_channel: release_channel.map(|entry| entry.config),
    })
}

// `handle_engine_command` was retired alongside `tools/model-runtime/`.
// The Candle-based ctox-engine binary no longer exists; CTOX now calls
// directly into per-model crates under src/inference/models/<model>/.

/// Lightweight health check: prints current versions, engine presence, and a
/// concrete next-step hint when things look off.
pub fn handle_doctor_command(root: &Path) -> Result<()> {
    let layout = InstallLayout::resolve(root)?;
    let manifest = load_install_manifest(&layout.install_manifest_path())?;
    let mut hints: Vec<String> = Vec::new();
    if manifest.is_none() {
        hints.push(
            "managed install manifest missing — run `ctox update adopt` to migrate".to_string(),
        );
    }
    let remote = check_remote_update(root).ok();
    let update_available = remote
        .as_ref()
        .map(|entry| entry.update_available)
        .unwrap_or(false);
    if update_available {
        let tag = remote
            .as_ref()
            .and_then(|entry| entry.latest_release.clone())
            .unwrap_or_default();
        hints.push(format!(
            "update available{}: run `ctox upgrade`",
            if tag.is_empty() {
                String::new()
            } else {
                format!(" ({tag})")
            }
        ));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "cli_version": build_version(),
            "managed_install": layout.managed(),
            "active_root": layout.active_root,
            "update_channel": remote.as_ref().and_then(|entry| entry.channel.clone()),
            "latest_release": remote.as_ref().and_then(|entry| entry.latest_release.clone()),
            "current_release": manifest.as_ref().and_then(|entry| entry.current_release.clone()),
            "hints": hints,
        }))?
    );
    Ok(())
}

pub fn handle_update_command(root: &Path, args: &[String]) -> Result<()> {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        print_update_help(args);
        return Ok(());
    }

    // `ctox update` / `ctox upgrade` one-shot path. Accepts --stable (default)
    // or --dev (track the main branch, source build) as the only args.
    let top_flags_only = !args.is_empty()
        && args
            .iter()
            .all(|arg| matches!(arg.as_str(), "--stable" | "--dev"));
    if args.is_empty() || top_flags_only {
        progress_reset();
        let layout = InstallLayout::resolve(root)?;
        let _lease = acquire_update_operation_lease(
            &layout,
            if args.iter().any(|arg| arg == "--dev") {
                "upgrade-dev"
            } else {
                "upgrade-stable"
            },
            None,
        )?;
        let use_dev = args.iter().any(|arg| arg == "--dev");
        let request = if use_dev {
            RemoteReleaseRequest::Branch("main".to_string())
        } else {
            RemoteReleaseRequest::Latest
        };
        progress_step(if use_dev {
            "start dev upgrade from main"
        } else {
            "start stable upgrade"
        });
        let result = apply_remote_update(root, request, false, false, false)?;
        progress_info("upgrade completed; emitting machine-readable summary on stdout");
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }
    match args.first().map(String::as_str) {
        Some("status") => {
            let layout = InstallLayout::resolve(root)?;
            let manifest = load_install_manifest(&layout.install_manifest_path())?;
            let update = load_update_state(&layout.update_state_path())?;
            let release_channel =
                resolve_release_channel(&layout, manifest.as_ref()).map(|entry| entry.config);
            println!(
                "{}",
                serde_json::to_string_pretty(&UpdateStatus {
                    version: build_version().to_string(),
                    managed_install: layout.managed(),
                    workspace_root: layout.workspace_root,
                    active_root: layout.active_root,
                    install_root: layout.install_root,
                    state_root: layout.state_root,
                    cache_root: layout.cache_root,
                    release_channel,
                    manifest,
                    update,
                })?
            );
            Ok(())
        }
        Some("check") => {
            let result = check_remote_update(root)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("channel") => handle_update_channel_command(root, &args[1..]),
        Some("adopt") => {
            let layout = InstallLayout::resolve(root)?;
            let install_root = find_flag_value(args, "--install-root")
                .map(PathBuf::from)
                .unwrap_or_else(default_install_root);
            let state_root = find_flag_value(args, "--state-root")
                .map(PathBuf::from)
                .unwrap_or_else(default_state_root);
            let release = find_flag_value(args, "--release")
                .map(ToOwned::to_owned)
                .unwrap_or_else(default_release_name);
            let force = has_flag(args, "--force");
            let skip_build = has_flag(args, "--skip-build");
            let _lease = acquire_update_operation_lease(&layout, "adopt", Some(&release))?;
            let result = adopt_installation(
                root,
                &install_root,
                &state_root,
                &release,
                force,
                skip_build,
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("apply") => {
            progress_reset();
            let layout = InstallLayout::resolve(root)?;
            let source = find_flag_value(args, "--source").map(PathBuf::from);
            let requested_release = find_flag_value(args, "--release").map(ToOwned::to_owned);
            let requested_version = find_flag_value(args, "--version").map(ToOwned::to_owned);
            let use_latest = has_flag(args, "--latest");
            let force = has_flag(args, "--force");
            let keep_failed_release = has_flag(args, "--keep-failed-release");
            let from_source = has_flag(args, "--from-source");
            let lock_release = requested_release
                .clone()
                .or_else(|| requested_version.clone())
                .or_else(|| source.as_deref().and_then(release_name_for_source));
            let _lease = acquire_update_operation_lease(&layout, "apply", lock_release.as_deref())?;
            let result = if let Some(source) = source {
                // Local --source always means a source-tree rebuild; binary-mode only
                // applies to remote releases that ship a pre-built bundle.
                let release = requested_release
                    .or_else(|| requested_version)
                    .unwrap_or_else(|| {
                        release_name_for_source(&source).unwrap_or_else(default_release_name)
                    });
                apply_update(
                    root,
                    &source,
                    &release,
                    force,
                    keep_failed_release,
                    UpdateSourceKind::Source,
                )?
            } else {
                let request = if use_latest {
                    RemoteReleaseRequest::Latest
                } else if let Some(version) = requested_version.or(requested_release) {
                    RemoteReleaseRequest::Tag(version)
                } else {
                    anyhow::bail!(
                        "usage: ctox update apply --source <path> [--release <name>] [--force] [--keep-failed-release] | ctox update apply --latest [--force] [--keep-failed-release] [--from-source] | ctox update apply --version <tag> [--force] [--keep-failed-release] [--from-source]"
                    );
                };
                apply_remote_update(root, request, force, keep_failed_release, from_source)?
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("rollback") => {
            let layout = InstallLayout::resolve(root)?;
            let _lease = acquire_update_operation_lease(&layout, "rollback", None)?;
            let result = rollback_update(root)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        _ => anyhow::bail!("{}", UPDATE_USAGE),
    }
}

const UPGRADE_USAGE: &str = "\
usage: ctox upgrade [--stable|--dev]

Options:
  --stable    Upgrade to the latest configured release (default)
  --dev       Upgrade from the main branch source archive
  -h, --help  Show this help without starting an upgrade
";

const UPDATE_APPLY_USAGE: &str = "\
usage: ctox update apply --source <path> [--release <name>] [--force] [--keep-failed-release]
   or: ctox update apply --latest [--force] [--keep-failed-release] [--from-source]
   or: ctox update apply --version <tag> [--force] [--keep-failed-release] [--from-source]

Options:
  --source <path>          Apply a local source tree
  --release <name>         Override the release name
  --latest                 Apply the latest configured release
  --version <tag>          Apply a specific release tag
  --force                  Allow replacing the active release
  --keep-failed-release    Keep failed release directory for inspection
  --from-source            Build a remote release from source
  -h, --help               Show this help without starting an update
";

const UPDATE_USAGE: &str = "\
usage: ctox update status
   or: ctox update check
   or: ctox update channel <show|set-github|clear> ...
   or: ctox update adopt [--install-root <path>] [--state-root <path>] [--release <name>] [--skip-build] [--force]
   or: ctox update apply --source <path> [--release <name>] [--force] [--keep-failed-release]
   or: ctox update apply --latest [--force] [--keep-failed-release] [--from-source]
   or: ctox update apply --version <tag> [--force] [--keep-failed-release] [--from-source]
   or: ctox update rollback
   or: ctox upgrade [--stable|--dev]

Use `ctox upgrade --help` or `ctox update apply --help` for focused help.
";

fn print_update_help(args: &[String]) {
    println!("{}", update_help_text(args));
}

fn update_help_text(args: &[String]) -> &'static str {
    if args.first().is_none_or(|arg| arg.starts_with('-')) {
        UPGRADE_USAGE
    } else if args.first().map(String::as_str) == Some("apply") {
        UPDATE_APPLY_USAGE
    } else {
        UPDATE_USAGE
    }
}

#[derive(Debug, Clone)]
enum RemoteReleaseRequest {
    Latest,
    Tag(String),
    /// Follow the tip of a branch (always source-mode — no release asset exists
    /// for arbitrary branches). Used by `ctox upgrade --dev`.
    Branch(String),
}

fn handle_update_channel_command(root: &Path, args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("show") => {
            let layout = InstallLayout::resolve(root)?;
            let manifest = load_install_manifest(&layout.install_manifest_path())?;
            let channel =
                resolve_release_channel(&layout, manifest.as_ref()).map(|entry| entry.config);
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({ "channel": channel }))?
            );
            Ok(())
        }
        Some("set-github") => {
            let repo = find_flag_value(args, "--repo")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .context("usage: ctox update channel set-github --repo <owner/repo> [--api-base <url>] [--token-env <env-var>]")?;
            let api_base = find_flag_value(args, "--api-base")
                .map(ToOwned::to_owned)
                .unwrap_or_else(default_github_api_base_string);
            let token_env = find_flag_value(args, "--token-env").map(ToOwned::to_owned);
            let layout = InstallLayout::resolve(root)?;
            let mut manifest = load_install_manifest(&layout.install_manifest_path())?
                .unwrap_or_else(|| install_manifest_template(&layout));
            manifest.release_channel = Some(ReleaseChannelConfig::GitHub {
                repo: repo.to_string(),
                api_base,
                token_env,
            });
            manifest.updated_at = now_rfc3339();
            persist_install_manifest(&layout.install_manifest_path(), &manifest)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "configured": true,
                    "channel": manifest.release_channel,
                    "manifest_path": layout.install_manifest_path(),
                }))?
            );
            Ok(())
        }
        Some("clear") => {
            let layout = InstallLayout::resolve(root)?;
            let mut manifest = load_install_manifest(&layout.install_manifest_path())?
                .unwrap_or_else(|| install_manifest_template(&layout));
            manifest.release_channel = None;
            manifest.updated_at = now_rfc3339();
            persist_install_manifest(&layout.install_manifest_path(), &manifest)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "configured": false,
                    "manifest_path": layout.install_manifest_path(),
                }))?
            );
            Ok(())
        }
        _ => anyhow::bail!(
            "usage: ctox update channel show | ctox update channel set-github --repo <owner/repo> [--api-base <url>] [--token-env <env-var>] | ctox update channel clear"
        ),
    }
}

fn check_remote_update(root: &Path) -> Result<RemoteUpdateCheck> {
    let layout = InstallLayout::resolve(root)?;
    let manifest = load_install_manifest(&layout.install_manifest_path())?;
    let Some(channel) = resolve_release_channel(&layout, manifest.as_ref()) else {
        let current_release = manifest
            .as_ref()
            .and_then(|entry| entry.current_release.clone());
        return Ok(RemoteUpdateCheck {
            configured: false,
            status: "remote_unconfigured".to_string(),
            reason: Some("No release channel is configured yet. Use `ctox update channel set-github --repo <owner/repo>` or continue with `ctox update apply --source <path>`.".to_string()),
            current_release,
            current_version: build_version().to_string(),
            channel: None,
            latest_release: None,
            latest_name: None,
            published_at: None,
            release_url: None,
            source_tarball_url: None,
            update_available: false,
        });
    };
    let current_release = manifest
        .as_ref()
        .and_then(|entry| entry.current_release.clone());
    let latest = fetch_remote_release(&channel.config, RemoteReleaseRequest::Latest)?;
    let update_available = current_release
        .as_deref()
        .map(|value| value != latest.tag_name)
        .unwrap_or_else(|| latest.tag_name != build_release_tag());
    Ok(RemoteUpdateCheck {
        configured: true,
        status: if update_available {
            "update_available".to_string()
        } else {
            "up_to_date".to_string()
        },
        reason: Some(format!("release channel resolved from {}", channel.source)),
        current_release,
        current_version: build_version().to_string(),
        channel: Some(channel.config),
        latest_release: Some(latest.tag_name.clone()),
        latest_name: latest.name.clone(),
        published_at: latest.published_at.clone(),
        release_url: latest.html_url.clone(),
        source_tarball_url: Some(latest.tarball_url.clone()),
        update_available,
    })
}

fn apply_remote_update(
    root: &Path,
    request: RemoteReleaseRequest,
    force: bool,
    keep_failed_release: bool,
    from_source: bool,
) -> Result<ApplyResult> {
    let started = Instant::now();
    let layout = InstallLayout::resolve(root)?;
    progress_step(format!(
        "resolved install layout: active={} state={} cache={}",
        layout.active_root.display(),
        layout.state_root.display(),
        layout.cache_root.display()
    ));
    let manifest = load_install_manifest(&layout.install_manifest_path())?;
    let channel = resolve_release_channel(&layout, manifest.as_ref())
        .context("release channel is not configured; use `ctox update channel set-github --repo <owner/repo>` first")?;
    progress_step(format!(
        "release channel: {}",
        match &channel.config {
            ReleaseChannelConfig::GitHub { repo, .. } => repo.as_str(),
        }
    ));
    // Branch requests only have a source tarball — no binary assets for arbitrary branches.
    let is_branch = matches!(request, RemoteReleaseRequest::Branch(_));
    let from_source = is_branch || from_source;
    // Branch HEADs move; always bypass the on-disk cache so `ctox upgrade --dev`
    // genuinely picks up the latest commit.
    let force = force || is_branch;
    let download_started = Instant::now();
    progress_step(if from_source {
        "fetch source archive"
    } else {
        "fetch binary release bundle"
    });
    let (downloaded, kind) = if from_source {
        (
            download_release_source(&layout, &channel.config, request, force)?,
            UpdateSourceKind::Source,
        )
    } else {
        (
            download_release_binary_bundle(&layout, &channel.config, request, force)?,
            UpdateSourceKind::Binary,
        )
    };
    progress_done(
        format!("downloaded {}", downloaded.release.tag_name),
        download_started,
    );
    let result = apply_update(
        root,
        &downloaded.extracted_root,
        &downloaded.release.tag_name,
        force,
        keep_failed_release,
        kind,
    )?;
    progress_done("remote upgrade", started);
    Ok(result)
}

fn adopt_installation(
    root: &Path,
    install_root: &Path,
    state_root: &Path,
    release: &str,
    force: bool,
    skip_build: bool,
) -> Result<AdoptResult> {
    let legacy_layout = InstallLayout::resolve(root)?;
    let legacy_manifest = load_install_manifest(&legacy_layout.install_manifest_path())?;
    validate_release_source(root)?;
    let release_root = install_root.join("releases").join(release);
    ensure_release_slot(&release_root, force)?;
    ensure_dir(install_root)?;
    ensure_dir(&install_root.join("releases"))?;
    migrate_legacy_state(root, state_root, force)?;
    copy_workspace(root, &release_root, UpdateSourceKind::Source)?;
    ensure_runtime_symlink(&release_root, state_root)?;
    if !skip_build {
        run_release_installer(&release_root, state_root)?;
    }
    let current_link = install_root.join("current");
    switch_current_release(&current_link, &release_root)?;
    sync_managed_launch_binaries(install_root, &current_link, state_root)?;
    write_managed_wrapper(install_root, state_root)?;
    refresh_service_unit(&current_link, state_root, Some(install_root))?;
    let manifest = InstallManifest {
        schema_version: 1,
        install_root: install_root.to_path_buf(),
        state_root: state_root.to_path_buf(),
        current_release: Some(release.to_string()),
        previous_release: None,
        adopted_from: Some(root.to_path_buf()),
        release_channel: legacy_manifest.and_then(|entry| entry.release_channel),
        updated_at: now_rfc3339(),
    };
    persist_install_manifest(
        install_root.join(INSTALL_MANIFEST_FILE_NAME).as_path(),
        &manifest,
    )?;
    persist_update_state(
        state_root.join(UPDATE_STATE_FILE_NAME).as_path(),
        &UpdateState {
            schema_version: 1,
            phase: "adopted".to_string(),
            current_version: build_version().to_string(),
            current_release: manifest.current_release.clone(),
            previous_release: None,
            target_release: None,
            source: Some(root.display().to_string()),
            state_backup_path: None,
            started_at: Some(now_rfc3339()),
            finished_at: Some(now_rfc3339()),
            last_error: None,
        },
    )?;
    Ok(AdoptResult {
        installed: true,
        release: release.to_string(),
        install_root: install_root.to_path_buf(),
        state_root: state_root.to_path_buf(),
        current_root: current_link,
    })
}

fn apply_update(
    root: &Path,
    source_root: &Path,
    release: &str,
    force: bool,
    keep_failed_release: bool,
    kind: UpdateSourceKind,
) -> Result<ApplyResult> {
    let update_started = Instant::now();
    progress_step(format!(
        "applying release {release} from {}",
        source_root.display()
    ));
    match kind {
        UpdateSourceKind::Source => validate_release_source(source_root)?,
        UpdateSourceKind::Binary => validate_binary_bundle(source_root)?,
    }
    progress_step("release source validated");
    let layout = InstallLayout::resolve(root)?;
    let install_root = layout
        .install_root
        .clone()
        .context("managed install required; run `ctox update adopt` first")?;
    let current_link = install_root.join("current");
    let releases_dir = install_root.join("releases");
    let release_root = releases_dir.join(release);
    ensure_dir(&releases_dir)?;
    ensure_release_slot(&release_root, force)?;
    let mut manifest = load_install_manifest(&layout.install_manifest_path())?
        .context("managed install manifest missing; run `ctox update adopt` first")?;
    let previous_release = manifest.current_release.clone();
    let previous_release_root = current_link
        .read_link()
        .ok()
        .map(|entry| absolutize_link_target(&current_link, &entry))
        .transpose()?
        .or_else(|| {
            previous_release
                .as_ref()
                .map(|name| releases_dir.join(name))
        });
    let backup_path = backup_state_root(&layout.state_root)?;
    progress_step(format!("state backup created: {}", backup_path.display()));
    let runtime_invariants = RuntimeCredentialSnapshot::capture(&layout.state_root)?;
    persist_update_state(
        &layout.update_state_path(),
        &UpdateState {
            schema_version: 1,
            phase: "preparing".to_string(),
            current_version: build_version().to_string(),
            current_release: previous_release.clone(),
            previous_release: previous_release.clone(),
            target_release: Some(release.to_string()),
            source: Some(source_root.display().to_string()),
            state_backup_path: Some(backup_path.clone()),
            started_at: Some(now_rfc3339()),
            finished_at: None,
            last_error: None,
        },
    )?;
    let copy_started = Instant::now();
    progress_step(format!(
        "copying release workspace to {}",
        release_root.display()
    ));
    copy_workspace(source_root, &release_root, kind)?;
    progress_done("copied release workspace", copy_started);
    if kind == UpdateSourceKind::Binary {
        if let Some(prev) = previous_release_root.as_deref() {
            carry_over_engine_from_previous(prev, &release_root)?;
        }
    }
    ensure_runtime_symlink(&release_root, &layout.state_root)?;
    persist_update_phase(&layout.update_state_path(), "building", None)?;
    if kind == UpdateSourceKind::Source {
        progress_step("running release installer / source build");
        if let Err(err) = run_release_installer(&release_root, &layout.state_root) {
            persist_update_phase(
                &layout.update_state_path(),
                "failed",
                Some(err.to_string().as_str()),
            )?;
            if !keep_failed_release {
                let _ = fs::remove_dir_all(&release_root);
            }
            return Err(err);
        }
        progress_step("release installer finished");
    }
    let pre_switch_status = service::service_status_snapshot(&layout.active_root).ok();
    // If we cannot read the pre-switch status (e.g. the service is in the
    // middle of a migration and answers slowly, or the socket has already
    // been torn down by a previous half-completed upgrade), default to
    // restarting. The previous default of `false` left the daemon dead
    // after exactly that scenario.
    let should_restart = pre_switch_status
        .as_ref()
        .map(|status| status.running || status.autostart_enabled)
        .unwrap_or(true);
    persist_update_phase(&layout.update_state_path(), "switching", None)?;
    progress_step("switching current symlink and restarting service if required");
    let _ = service::stop_background(&layout.active_root);
    if let Err(err) = switch_current_release(&current_link, &release_root) {
        maybe_restart_service(previous_release_root.as_deref())?;
        persist_update_phase(
            &layout.update_state_path(),
            "failed",
            Some(err.to_string().as_str()),
        )?;
        return Err(err);
    }
    sync_managed_launch_binaries(&install_root, &current_link, &layout.state_root)?;
    write_managed_wrapper(&install_root, &layout.state_root)?;
    if let Err(err) = refresh_service_unit(&current_link, &layout.state_root, Some(&install_root)) {
        rollback_to_previous_release(
            &install_root,
            &current_link,
            previous_release_root.as_deref(),
            &layout.state_root,
            &backup_path,
            should_restart,
        )?;
        persist_update_phase(
            &layout.update_state_path(),
            "failed",
            Some(err.to_string().as_str()),
        )?;
        return Err(err);
    }
    if should_restart {
        progress_step("starting CTOX background service");
        // Verify post-start that the daemon is actually `running`. The old
        // logic only checked that the snapshot call succeeded, which it does
        // even when `running == false` — a silent-success path that left
        // production dead after several upgrades. Now any non-running
        // post-start state triggers rollback with a precise error.
        if let Err(err) = service::start_background(&current_link).and_then(|_| {
            let status = service::service_status_snapshot(&current_link)?;
            if status.running {
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "post-upgrade verification: CTOX service did not reach running state on {}",
                    status.listen_addr
                ))
            }
        }) {
            rollback_to_previous_release(
                &install_root,
                &current_link,
                previous_release_root.as_deref(),
                &layout.state_root,
                &backup_path,
                true,
            )?;
            persist_update_phase(
                &layout.update_state_path(),
                "failed",
                Some(err.to_string().as_str()),
            )?;
            return Err(err);
        }
    }
    runtime_invariants.verify_preserved(&layout.state_root)?;
    manifest.previous_release = previous_release.clone();
    manifest.current_release = Some(release.to_string());
    manifest.updated_at = now_rfc3339();
    persist_install_manifest(&layout.install_manifest_path(), &manifest)?;
    prune_old_releases(&releases_dir, &manifest);
    let completed = UpdateState {
        schema_version: 1,
        phase: "completed".to_string(),
        current_version: build_version().to_string(),
        current_release: manifest.current_release.clone(),
        previous_release,
        target_release: Some(release.to_string()),
        source: Some(source_root.display().to_string()),
        state_backup_path: Some(backup_path.clone()),
        started_at: load_update_state(&layout.update_state_path())?
            .and_then(|entry| entry.started_at),
        finished_at: Some(now_rfc3339()),
        last_error: None,
    };
    persist_update_state(&layout.update_state_path(), &completed)?;
    progress_done(format!("applied release {release}"), update_started);
    Ok(ApplyResult {
        updated: true,
        release: release.to_string(),
        current_root: current_link,
        previous_release: manifest.previous_release,
        state_backup_path: backup_path,
    })
}

fn rollback_update(root: &Path) -> Result<RollbackResult> {
    let layout = InstallLayout::resolve(root)?;
    let install_root = layout
        .install_root
        .clone()
        .context("managed install required; run `ctox update adopt` first")?;
    let current_link = install_root.join("current");
    let mut manifest = load_install_manifest(&layout.install_manifest_path())?
        .context("managed install manifest missing; run `ctox update adopt` first")?;
    let previous_release = manifest
        .previous_release
        .clone()
        .context("no previous release recorded for rollback")?;
    let previous_release_root = install_root.join("releases").join(&previous_release);
    let current_release = manifest
        .current_release
        .clone()
        .context("current release missing from manifest")?;
    let update_state = load_update_state(&layout.update_state_path())?;
    let should_restart = service::service_status_snapshot(&layout.active_root)
        .map(|status| status.running || status.autostart_enabled)
        .unwrap_or(false);
    let _ = service::stop_background(&layout.active_root);
    if let Some(backup_path) = update_state.and_then(|entry| entry.state_backup_path) {
        restore_state_backup(&backup_path, &layout.state_root)?;
    }
    switch_current_release(&current_link, &previous_release_root)?;
    sync_managed_launch_binaries(&install_root, &current_link, &layout.state_root)?;
    write_managed_wrapper(&install_root, &layout.state_root)?;
    refresh_service_unit(&current_link, &layout.state_root, Some(&install_root))?;
    if should_restart {
        let _ = service::start_background(&current_link);
    }
    manifest.current_release = Some(previous_release.clone());
    manifest.previous_release = Some(current_release);
    manifest.updated_at = now_rfc3339();
    persist_install_manifest(&layout.install_manifest_path(), &manifest)?;
    prune_old_releases(&install_root.join("releases"), &manifest);
    persist_update_state(
        &layout.update_state_path(),
        &UpdateState {
            schema_version: 1,
            phase: "rolled_back".to_string(),
            current_version: build_version().to_string(),
            current_release: manifest.current_release.clone(),
            previous_release: manifest.previous_release.clone(),
            target_release: None,
            source: None,
            state_backup_path: load_update_state(&layout.update_state_path())?
                .and_then(|entry| entry.state_backup_path),
            started_at: Some(now_rfc3339()),
            finished_at: Some(now_rfc3339()),
            last_error: None,
        },
    )?;
    Ok(RollbackResult {
        rolled_back: true,
        current_release: previous_release,
        previous_release: manifest.previous_release,
        current_root: current_link,
    })
}

fn persist_update_phase(path: &Path, phase: &str, error: Option<&str>) -> Result<()> {
    let mut state = load_update_state(path)?.unwrap_or(UpdateState {
        schema_version: 1,
        phase: "idle".to_string(),
        current_version: build_version().to_string(),
        current_release: None,
        target_release: None,
        previous_release: None,
        source: None,
        state_backup_path: None,
        started_at: None,
        finished_at: None,
        last_error: None,
    });
    state.phase = phase.to_string();
    state.last_error = error.map(ToOwned::to_owned);
    if matches!(phase, "completed" | "failed" | "rolled_back") {
        state.finished_at = Some(now_rfc3339());
    } else {
        state.finished_at = None;
    }
    persist_update_state(path, &state)
}

fn acquire_update_operation_lease(
    layout: &InstallLayout,
    operation: &str,
    target_release: Option<&str>,
) -> Result<UpdateOperationLease> {
    let path = layout.update_lock_path();
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    for _ in 0..2 {
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut handle) => {
                writeln!(handle, "pid={}", std::process::id())?;
                writeln!(handle, "operation={operation}")?;
                if let Some(target_release) = target_release {
                    writeln!(handle, "target_release={target_release}")?;
                }
                writeln!(handle, "started_at={}", now_rfc3339())?;
                return Ok(UpdateOperationLease { path });
            }
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {
                if update_lock_is_stale(&path) {
                    let _ = fs::remove_file(&path);
                    continue;
                }
                let holder = describe_update_lock_holder(&path);
                anyhow::bail!("another update operation is already active{holder}");
            }
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("failed to create update lock {}", path.display()));
            }
        }
    }
    anyhow::bail!(
        "failed to acquire update lock {} after removing a stale holder",
        path.display()
    )
}

fn describe_update_lock_holder(path: &Path) -> String {
    let mut details = Vec::new();
    if let Some(pid) = lock_field(path, "pid") {
        details.push(format!("pid={pid}"));
    }
    if let Some(operation) = lock_field(path, "operation") {
        details.push(format!("operation={operation}"));
    }
    if let Some(target_release) = lock_field(path, "target_release") {
        details.push(format!("target_release={target_release}"));
    }
    if details.is_empty() {
        String::new()
    } else {
        format!(" ({})", details.join(", "))
    }
}

fn update_lock_is_stale(path: &Path) -> bool {
    let Some(pid) = lock_field(path, "pid").and_then(|value| value.parse::<u32>().ok()) else {
        return true;
    };
    if pid == std::process::id() {
        return false;
    }
    !process_is_alive(pid)
}

fn lock_field(path: &Path, key: &str) -> Option<String> {
    let raw = fs::read_to_string(path).ok()?;
    raw.lines().find_map(|line| {
        line.strip_prefix(&format!("{key}="))
            .map(|value| value.trim().to_string())
    })
}

#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    let pid = pid as libc::pid_t;
    let rc = unsafe { libc::kill(pid, 0) };
    if rc == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(windows)]
fn process_is_alive(pid: u32) -> bool {
    let filter = format!("PID eq {pid}");
    let output = Command::new("tasklist")
        .args(["/FI", &filter, "/FO", "CSV", "/NH"])
        .output();
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    !stdout.trim().is_empty() && !stdout.contains("No tasks are running")
}

fn rollback_to_previous_release(
    install_root: &Path,
    current_link: &Path,
    previous_release_root: Option<&Path>,
    state_root: &Path,
    backup_path: &Path,
    should_restart: bool,
) -> Result<()> {
    let _ = service::stop_background(current_link);
    restore_state_backup(backup_path, state_root)?;
    if let Some(previous_release_root) = previous_release_root {
        switch_current_release(current_link, previous_release_root)?;
        sync_managed_launch_binaries(install_root, current_link, state_root)?;
        if should_restart {
            let _ = service::start_background(current_link);
        }
    }
    Ok(())
}

fn maybe_restart_service(previous_release_root: Option<&Path>) -> Result<()> {
    if let Some(previous_release_root) = previous_release_root {
        let _ = service::start_background(previous_release_root);
    }
    Ok(())
}

fn prune_old_releases(releases_dir: &Path, manifest: &InstallManifest) {
    let current = manifest.current_release.as_deref();
    let previous = manifest.previous_release.as_deref();
    let Ok(entries) = fs::read_dir(releases_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        if Some(name) == current || Some(name) == previous {
            continue;
        }
        match fs::remove_dir_all(&path) {
            Ok(()) => progress_step(format!("pruned old release {name}")),
            Err(err) => progress_step(format!(
                "warning: failed to prune old release {name}: {err}"
            )),
        }
    }
}

fn resolve_active_root(root: &Path) -> PathBuf {
    if root
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| name == "current")
    {
        return root.to_path_buf();
    }
    root.to_path_buf()
}

fn resolve_install_root(root: &Path) -> Option<PathBuf> {
    if let Some(install_root) = std::env::var_os("CTOX_INSTALL_ROOT")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        return Some(install_root);
    }
    if let Some(install_root) = runtime_env::env_or_config(root, "CTOX_INSTALL_ROOT")
        .filter(|value| !value.trim().is_empty())
    {
        return Some(PathBuf::from(install_root));
    }
    let file_name = root.file_name().and_then(OsStr::to_str)?;
    if file_name == "current" {
        let parent = root.parent()?.to_path_buf();
        if parent.join("releases").is_dir() {
            return Some(parent);
        }
    }
    None
}

fn resolve_state_root(root: &Path, install_root: Option<&Path>) -> Result<PathBuf> {
    if let Some(install_root) = install_root {
        let manifest_path = install_root.join(INSTALL_MANIFEST_FILE_NAME);
        if let Some(manifest) = load_install_manifest(&manifest_path)? {
            if !manifest.state_root.as_os_str().is_empty() {
                return Ok(manifest.state_root);
            }
        }
    }
    if let Some(state_root) = std::env::var_os("CTOX_STATE_ROOT")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        return Ok(state_root);
    }
    if let Some(state_root) =
        runtime_env::env_or_config(root, "CTOX_STATE_ROOT").filter(|value| !value.trim().is_empty())
    {
        return Ok(PathBuf::from(state_root));
    }
    let runtime_path = root.join("runtime");
    if let Ok(target) = fs::read_link(&runtime_path) {
        return absolutize_link_target(&runtime_path, &target);
    }
    Ok(runtime_path)
}

fn resolve_cache_root(root: &Path) -> Result<PathBuf> {
    if let Some(cache_root) = std::env::var_os("CTOX_CACHE_ROOT")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        return Ok(cache_root);
    }
    if let Some(cache_root) =
        runtime_env::env_or_config(root, "CTOX_CACHE_ROOT").filter(|value| !value.trim().is_empty())
    {
        return Ok(PathBuf::from(cache_root));
    }
    Ok(default_cache_root())
}

fn resolve_tools_root(root: &Path) -> PathBuf {
    runtime_env::env_or_config(root, "CTOX_TOOLS_ROOT")
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("runtime/tools"))
}

fn resolve_release_channel(
    _layout: &InstallLayout,
    manifest: Option<&InstallManifest>,
) -> Option<ResolvedReleaseChannel> {
    if let Some(repo) = env::var("CTOX_UPDATE_GITHUB_REPO")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        return Some(ResolvedReleaseChannel {
            config: ReleaseChannelConfig::GitHub {
                repo,
                api_base: env::var("CTOX_UPDATE_GITHUB_API_BASE")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(default_github_api_base_string),
                token_env: env::var("CTOX_UPDATE_GITHUB_TOKEN_ENV")
                    .ok()
                    .filter(|value| !value.trim().is_empty()),
            },
            source: "environment",
        });
    }
    if let Some(config) = manifest.and_then(|entry| entry.release_channel.clone()) {
        return Some(ResolvedReleaseChannel {
            config,
            source: "install_manifest",
        });
    }
    Some(ResolvedReleaseChannel {
        config: ReleaseChannelConfig::GitHub {
            repo: DEFAULT_RELEASE_REPO.to_string(),
            api_base: default_github_api_base_string(),
            token_env: None,
        },
        source: "built_in_default",
    })
}

fn install_manifest_template(layout: &InstallLayout) -> InstallManifest {
    InstallManifest {
        schema_version: 1,
        install_root: layout
            .install_root
            .clone()
            .unwrap_or_else(|| layout.workspace_root.clone()),
        state_root: layout.state_root.clone(),
        current_release: None,
        previous_release: None,
        adopted_from: None,
        release_channel: None,
        updated_at: now_rfc3339(),
    }
}

fn fetch_remote_release(
    channel: &ReleaseChannelConfig,
    request: RemoteReleaseRequest,
) -> Result<GitHubReleaseResponse> {
    match channel {
        ReleaseChannelConfig::GitHub { repo, api_base, .. } => {
            if let RemoteReleaseRequest::Branch(branch) = &request {
                // Synthesize a GitHubReleaseResponse that points at the branch
                // tarball. GitHub doesn't publish a "release" for a branch, so
                // we can't list release assets here — callers must use the
                // source-mode path for Branch requests.
                let tarball_url =
                    format!("https://codeload.github.com/{repo}/tar.gz/refs/heads/{branch}");
                return Ok(GitHubReleaseResponse {
                    tag_name: format!("branch-{branch}-{}", current_utc().format("%Y%m%dT%H%M%SZ")),
                    name: Some(format!("{branch} (development snapshot)")),
                    tarball_url,
                    html_url: None,
                    published_at: None,
                    assets: Vec::new(),
                });
            }
            let endpoint = match &request {
                RemoteReleaseRequest::Latest => {
                    format!(
                        "{}/repos/{repo}/releases/latest",
                        api_base.trim_end_matches('/')
                    )
                }
                RemoteReleaseRequest::Tag(tag) => format!(
                    "{}/repos/{repo}/releases/tags/{}",
                    api_base.trim_end_matches('/'),
                    tag
                ),
                RemoteReleaseRequest::Branch(_) => unreachable!("handled above"),
            };
            let body = github_api_get_json(channel, &endpoint)?;
            let release: GitHubReleaseResponse =
                serde_json::from_str(&body).with_context(|| {
                    format!("failed to decode GitHub release response from {endpoint}")
                })?;
            Ok(release)
        }
    }
}

fn target_bundle_asset_name() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("ctox-linux-x64.tar.gz"),
        ("linux", "aarch64") => Some("ctox-linux-arm64.tar.gz"),
        ("macos", "x86_64") => Some("ctox-macos-x64.tar.gz"),
        ("macos", "aarch64") => Some("ctox-macos-arm64.tar.gz"),
        _ => None,
    }
}

fn download_release_binary_bundle(
    layout: &InstallLayout,
    channel: &ReleaseChannelConfig,
    request: RemoteReleaseRequest,
    force: bool,
) -> Result<DownloadedReleaseSource> {
    let release = fetch_remote_release(channel, request)?;
    let asset_name = target_bundle_asset_name().with_context(|| {
        format!(
            "no pre-built binary bundle published for {}/{}; retry with `--from-source` to build from source",
            std::env::consts::OS,
            std::env::consts::ARCH,
        )
    })?;
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .cloned()
        .with_context(|| {
            format!(
                "release {} has no asset `{}`; retry with `--from-source` to build from source",
                release.tag_name, asset_name
            )
        })?;
    let repo_key = match channel {
        ReleaseChannelConfig::GitHub { repo, .. } => repo.replace('/', "--"),
    };
    let downloads_dir = layout.cache_root.join("downloads").join(&repo_key);
    let bundles_dir = layout.cache_root.join("bundles").join(&repo_key);
    ensure_dir(&downloads_dir)?;
    ensure_dir(&bundles_dir)?;
    let archive_path = downloads_dir.join(format!("{}-{asset_name}", release.tag_name));
    let extracted_root = bundles_dir.join(&release.tag_name);
    if force {
        let _ = fs::remove_file(&archive_path);
        let _ = fs::remove_dir_all(&extracted_root);
    }
    if !archive_path.exists() {
        download_remote_archive(channel, &asset.browser_download_url, &archive_path)?;
    }
    verify_sha256_asset(channel, &release, asset_name, &archive_path)?;
    if !extracted_root.exists() {
        extract_bundle_to_root(&archive_path, &extracted_root)?;
    }
    Ok(DownloadedReleaseSource {
        release,
        extracted_root,
    })
}

fn verify_sha256_asset(
    channel: &ReleaseChannelConfig,
    release: &GitHubReleaseResponse,
    asset_name: &str,
    archive_path: &Path,
) -> Result<()> {
    let sha_asset_name = format!("{asset_name}.sha256");
    let Some(sha_asset) = release.assets.iter().find(|a| a.name == sha_asset_name) else {
        return Ok(());
    };
    let response = github_request(channel, &sha_asset.browser_download_url)?
        .call()
        .with_context(|| {
            format!(
                "failed to download {sha_asset_name} from {}",
                sha_asset.browser_download_url
            )
        })?;
    let expected_line = response
        .into_string()
        .context("failed to read sha256 checksum body")?;
    let expected = expected_line
        .split_whitespace()
        .next()
        .context("sha256 asset is empty")?
        .to_ascii_lowercase();
    let bytes = fs::read(archive_path)
        .with_context(|| format!("failed to read {}", archive_path.display()))?;
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(&bytes);
    let actual = format!("{:x}", hasher.finalize());
    if actual != expected {
        anyhow::bail!(
            "sha256 mismatch for {}: expected {}, got {}",
            archive_path.display(),
            expected,
            actual
        );
    }
    Ok(())
}

fn extract_bundle_to_root(archive_path: &Path, destination_root: &Path) -> Result<()> {
    ensure_release_slot(destination_root, true)?;
    ensure_dir(destination_root)?;
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(archive_path)
        .arg("-C")
        .arg(destination_root)
        .status()
        .with_context(|| format!("failed to extract {}", archive_path.display()))?;
    if !status.success() {
        anyhow::bail!("failed to extract {}", archive_path.display());
    }
    Ok(())
}

fn download_release_source(
    layout: &InstallLayout,
    channel: &ReleaseChannelConfig,
    request: RemoteReleaseRequest,
    force: bool,
) -> Result<DownloadedReleaseSource> {
    let release = fetch_remote_release(channel, request)?;
    let repo_key = match channel {
        ReleaseChannelConfig::GitHub { repo, .. } => repo.replace('/', "--"),
    };
    let downloads_dir = layout.cache_root.join("downloads").join(&repo_key);
    let sources_dir = layout.cache_root.join("sources").join(&repo_key);
    ensure_dir(&downloads_dir)?;
    ensure_dir(&sources_dir)?;
    let archive_path = downloads_dir.join(format!("{}.tar.gz", release.tag_name));
    let extracted_root = sources_dir.join(&release.tag_name);
    if force {
        let _ = fs::remove_file(&archive_path);
        let _ = fs::remove_dir_all(&extracted_root);
    }
    if !archive_path.exists() {
        download_remote_archive(channel, &release.tarball_url, &archive_path)?;
    }
    if !extracted_root.exists() {
        extract_archive_to_root(&archive_path, &extracted_root)?;
    }
    Ok(DownloadedReleaseSource {
        release,
        extracted_root,
    })
}

fn github_api_get_json(channel: &ReleaseChannelConfig, url: &str) -> Result<String> {
    let response = github_request(channel, url)?
        .call()
        .with_context(|| format!("failed to fetch release metadata from {url}"))?;
    response
        .into_string()
        .context("failed to read release metadata response")
}

fn download_remote_archive(
    channel: &ReleaseChannelConfig,
    url: &str,
    destination: &Path,
) -> Result<()> {
    if let Some(parent) = destination.parent() {
        ensure_dir(parent)?;
    }
    let response = github_request(channel, url)?
        .call()
        .with_context(|| format!("failed to download release archive from {url}"))?;
    let mut reader = response.into_reader();
    let mut file = fs::File::create(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    copy(&mut reader, &mut file)
        .with_context(|| format!("failed to write {}", destination.display()))?;
    Ok(())
}

fn github_request(channel: &ReleaseChannelConfig, url: &str) -> Result<ureq::Request> {
    let agent = ureq::AgentBuilder::new().build();
    let mut request = agent
        .get(url)
        .set("accept", "application/vnd.github+json")
        .set("user-agent", &format!("ctox/{}", build_version()));
    if let Some(token) = github_token(channel) {
        request = request.set("authorization", &format!("Bearer {token}"));
    }
    Ok(request)
}

fn github_token(channel: &ReleaseChannelConfig) -> Option<String> {
    match channel {
        ReleaseChannelConfig::GitHub { token_env, .. } => {
            let env_name = token_env.as_deref().unwrap_or(DEFAULT_GITHUB_TOKEN_ENV);
            env::var(env_name)
                .ok()
                .filter(|value| !value.trim().is_empty())
        }
    }
}

fn extract_archive_to_root(archive_path: &Path, destination_root: &Path) -> Result<()> {
    ensure_release_slot(destination_root, true)?;
    ensure_dir(destination_root)?;
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(archive_path)
        .arg("-C")
        .arg(destination_root)
        .arg("--strip-components")
        .arg("1")
        .status()
        .with_context(|| format!("failed to extract {}", archive_path.display()))?;
    if !status.success() {
        anyhow::bail!("failed to extract {}", archive_path.display());
    }
    Ok(())
}

fn load_install_manifest(path: &Path) -> Result<Option<InstallManifest>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read install manifest {}", path.display()))?;
    let manifest = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to decode install manifest {}", path.display()))?;
    Ok(Some(manifest))
}

fn persist_install_manifest(path: &Path, manifest: &InstallManifest) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(manifest)?;
    fs::write(path, bytes)
        .with_context(|| format!("failed to write install manifest {}", path.display()))
}

fn load_update_state(path: &Path) -> Result<Option<UpdateState>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read update state {}", path.display()))?;
    let state = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to decode update state {}", path.display()))?;
    Ok(Some(state))
}

fn persist_update_state(path: &Path, state: &UpdateState) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(state)?;
    fs::write(path, bytes)
        .with_context(|| format!("failed to write update state {}", path.display()))
}

fn ensure_release_slot(release_root: &Path, force: bool) -> Result<()> {
    if release_root.exists() {
        if !force {
            anyhow::bail!(
                "release path already exists: {} (use --force to replace it)",
                release_root.display()
            );
        }
        if release_root.is_dir() {
            fs::remove_dir_all(release_root)
                .with_context(|| format!("failed to remove {}", release_root.display()))?;
        } else {
            fs::remove_file(release_root)
                .with_context(|| format!("failed to remove {}", release_root.display()))?;
        }
    }
    Ok(())
}

fn migrate_legacy_state(root: &Path, state_root: &Path, force: bool) -> Result<()> {
    ensure_dir(state_root)?;
    let runtime_root = root.join("runtime");
    if !runtime_root.exists() {
        return Ok(());
    }
    let runtime_root = fs::canonicalize(&runtime_root).unwrap_or(runtime_root);
    let state_root_canonical =
        fs::canonicalize(state_root).unwrap_or_else(|_| state_root.to_path_buf());
    if runtime_root == state_root_canonical {
        return Ok(());
    }
    if state_root_has_files(state_root)? && !force {
        anyhow::bail!(
            "state root {} is not empty; pass --force if you want to reuse it",
            state_root.display()
        );
    }
    copy_filtered(&runtime_root, state_root, &|path, _| {
        path.file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name == ".DS_Store")
    })
}

fn run_release_installer(release_root: &Path, state_root: &Path) -> Result<()> {
    let started = Instant::now();
    let script = release_root.join("install.sh");
    let legacy_script = release_root.join("scripts/install/install_ctox.sh");
    let (chosen_script, args) = if script.is_file() {
        (
            &script,
            vec!["--rebuild", release_root.to_str().unwrap_or(".")],
        )
    } else if legacy_script.is_file() {
        (&legacy_script, vec![])
    } else {
        anyhow::bail!(
            "no installer found at {} or {}",
            script.display(),
            legacy_script.display()
        );
    };
    let mut cmd = Command::new(chosen_script);
    cmd.current_dir(release_root)
        .env("CTOX_STATE_ROOT", state_root);
    if let Some(bwrap_source_dir) = resolve_bwrap_source_dir_for_installer(release_root) {
        cmd.env("CODEX_BWRAP_SOURCE_DIR", bwrap_source_dir);
    }
    if chosen_script == &legacy_script {
        cmd.env("CTOX_INSTALL_SKIP_RUNTIME_WIPE", "1")
            .env("CTOX_INSTALL_SKIP_SERVICE_CONTROL", "1")
            .env("CTOX_INSTALL_SKIP_WRAPPER_WRITE", "1");
    }
    for arg in &args {
        cmd.arg(arg);
    }
    progress_step(format!(
        "installer command: {} {}",
        chosen_script.display(),
        args.join(" ")
    ));
    progress_info("installer output follows");
    let status = cmd
        .status()
        .with_context(|| format!("failed to start installer {}", chosen_script.display()))?;
    if !status.success() {
        anyhow::bail!("release installer failed for {}", release_root.display());
    }
    progress_done("installer", started);
    Ok(())
}

fn resolve_bwrap_source_dir_for_installer(release_root: &Path) -> Option<PathBuf> {
    if let Ok(path) = env::var("CODEX_BWRAP_SOURCE_DIR") {
        let candidate = PathBuf::from(path);
        if bubblewrap_checkout_ready(&candidate) {
            return Some(candidate);
        }
    }

    if let Some(candidate) = runtime_env::env_or_config(release_root, "CODEX_BWRAP_SOURCE_DIR")
        .map(PathBuf::from)
        .filter(|candidate| bubblewrap_checkout_ready(candidate))
    {
        return Some(candidate);
    }

    let conventional_candidates = home_dir()
        .into_iter()
        .flat_map(|home| {
            [
                home.join("workspace/ctox-bwrap"),
                home.join("workspace/bubblewrap"),
                home.join("ctox-bwrap"),
                home.join("bubblewrap"),
            ]
        })
        .collect::<Vec<_>>();
    conventional_candidates
        .into_iter()
        .find(|candidate| bubblewrap_checkout_ready(candidate))
}

fn bubblewrap_checkout_ready(path: &Path) -> bool {
    path.join("bubblewrap.c").is_file() && path.join("bind-mount.c").is_file()
}

fn validate_release_source(source_root: &Path) -> Result<()> {
    let outcome = engine::source_layout_status(source_root)?;
    if outcome.ready {
        return Ok(());
    }
    let mut missing = Vec::new();
    for result in outcome.results {
        for path in result.missing_paths {
            missing.push(path.display().to_string());
        }
    }
    missing.sort();
    missing.dedup();
    anyhow::bail!(
        "source checkout is not upgrade-ready: missing required integrated paths: {}",
        missing.join(", ")
    );
}

fn copy_workspace(source_root: &Path, release_root: &Path, kind: UpdateSourceKind) -> Result<()> {
    ensure_dir(release_root)?;
    copy_filtered(source_root, release_root, &|path, is_dir| {
        let Some(name) = path.file_name().and_then(OsStr::to_str) else {
            return false;
        };
        let relative = path.strip_prefix(source_root).ok();
        let top_level_runtime = relative
            .map(|entry| entry.components().count() == 1 && name == "runtime")
            .unwrap_or(false);
        let skip_target = kind == UpdateSourceKind::Source && name == "target";
        name == ".git"
            || skip_target
            || top_level_runtime
            || (is_dir && matches!(name, ".DS_Store"))
            || (!is_dir && name == ".DS_Store")
    })
}

fn validate_binary_bundle(source_root: &Path) -> Result<()> {
    let binary = source_root.join("bin/ctox");
    if !binary.exists() {
        anyhow::bail!(
            "binary bundle is missing the ctox executable at {}",
            binary.display()
        );
    }
    Ok(())
}

fn carry_over_engine_from_previous(previous_root: &Path, release_root: &Path) -> Result<()> {
    let _ = previous_root;
    let _ = release_root;
    Ok(())
}

fn copy_filtered<F>(source_root: &Path, destination_root: &Path, skip: &F) -> Result<()>
where
    F: Fn(&Path, bool) -> bool,
{
    ensure_dir(destination_root)?;
    for entry in fs::read_dir(source_root)
        .with_context(|| format!("failed to read {}", source_root.display()))?
    {
        let entry = entry?;
        let source_path = entry.path();
        let file_type = entry.file_type()?;
        if skip(&source_path, file_type.is_dir()) {
            continue;
        }
        let destination_path = destination_root.join(entry.file_name());
        if file_type.is_dir() {
            copy_filtered(&source_path, &destination_path, skip)?;
            continue;
        }
        // Skip Unix sockets, FIFOs, and other special files that cannot be copied.
        if !file_type.is_file() && !file_type.is_symlink() {
            continue;
        }
        if file_type.is_symlink() {
            let target = fs::read_link(&source_path)
                .with_context(|| format!("failed to read symlink {}", source_path.display()))?;
            create_symlink(&target, &destination_path)?;
            continue;
        }
        if let Some(parent) = destination_path.parent() {
            ensure_dir(parent)?;
        }
        match fs::copy(&source_path, &destination_path) {
            Ok(_) => {}
            Err(err) if err.kind() == ErrorKind::NotFound => continue,
            Err(err) => {
                return Err(err).with_context(|| {
                    format!(
                        "failed to copy {} to {}",
                        source_path.display(),
                        destination_path.display()
                    )
                });
            }
        }
    }
    Ok(())
}

fn ensure_runtime_symlink(release_root: &Path, state_root: &Path) -> Result<()> {
    ensure_dir(state_root)?;
    let runtime_path = release_root.join("runtime");
    if runtime_path.exists() || runtime_path.symlink_metadata().is_ok() {
        if runtime_path.is_dir() && !runtime_path.is_symlink() {
            fs::remove_dir_all(&runtime_path)
                .with_context(|| format!("failed to remove {}", runtime_path.display()))?;
        } else {
            fs::remove_file(&runtime_path)
                .with_context(|| format!("failed to remove {}", runtime_path.display()))?;
        }
    }
    create_symlink(state_root, &runtime_path)
}

fn backup_state_root(state_root: &Path) -> Result<PathBuf> {
    let backup_root = state_root
        .join("backups")
        .join(format!("update-{}", current_utc().format("%Y%m%dT%H%M%SZ")));
    ensure_dir(&backup_root)?;
    copy_filtered(state_root, &backup_root, &|path, is_dir| {
        if path == backup_root {
            return true;
        }
        let Some(name) = path.file_name().and_then(OsStr::to_str) else {
            return false;
        };
        if is_dir && name == "backups" {
            return true;
        }
        if is_dir {
            return false;
        }
        !matches!(
            path.extension().and_then(OsStr::to_str),
            Some("db")
                | Some("sqlite")
                | Some("sqlite3")
                | Some("sqlite3-wal")
                | Some("sqlite3-shm")
                | Some("json")
                | Some("env")
                | Some("md")
        )
    })?;
    let manifest_path = backup_root.join("backup_manifest.json");
    let manifest = json!({
        "created_at": now_rfc3339(),
        "source_root": state_root,
    });
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;
    Ok(backup_root)
}

fn restore_state_backup(backup_root: &Path, state_root: &Path) -> Result<()> {
    if !backup_root.exists() {
        anyhow::bail!("state backup not found: {}", backup_root.display());
    }
    copy_filtered(backup_root, state_root, &|path, _| {
        path.file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name == "backup_manifest.json")
    })
}

fn state_root_has_files(state_root: &Path) -> Result<bool> {
    let mut entries = fs::read_dir(state_root)
        .with_context(|| format!("failed to inspect {}", state_root.display()))?;
    Ok(entries.next().transpose()?.is_some())
}

#[derive(Debug, Clone, Default)]
struct RuntimeCredentialSnapshot {
    env_values: Vec<(String, String)>,
    secret_presence: Vec<String>,
}

impl RuntimeCredentialSnapshot {
    fn capture(root: &Path) -> Result<Self> {
        let env_map = runtime_env::effective_operator_env_map(root)
            .with_context(|| format!("failed to read runtime env from {}", root.display()))?;
        let env_values = UPGRADE_RUNTIME_ENV_INVARIANT_KEYS
            .iter()
            .filter_map(|key| {
                env_map
                    .get(*key)
                    .filter(|value| !value.trim().is_empty())
                    .map(|value| ((*key).to_string(), value.clone()))
            })
            .collect();
        let secret_presence = UPGRADE_SECRET_INVARIANT_KEYS
            .iter()
            .filter(|key| {
                secrets::get_credential(root, key).is_some_and(|value| !value.trim().is_empty())
            })
            .map(|key| (*key).to_string())
            .collect();
        Ok(Self {
            env_values,
            secret_presence,
        })
    }

    fn verify_preserved(&self, root: &Path) -> Result<()> {
        if self.env_values.is_empty() && self.secret_presence.is_empty() {
            return Ok(());
        }
        let after = Self::capture(root)?;
        let mut missing = Vec::new();
        for (key, before_value) in &self.env_values {
            match after
                .env_values
                .iter()
                .find(|(candidate, _)| candidate == key)
            {
                Some((_, after_value)) if after_value == before_value => {}
                Some(_) => missing.push(format!("{key} changed")),
                None => missing.push(format!("{key} missing")),
            }
        }
        for key in &self.secret_presence {
            if !after
                .secret_presence
                .iter()
                .any(|candidate| candidate == key)
            {
                missing.push(format!("{key} secret missing"));
            }
        }
        if !missing.is_empty() {
            anyhow::bail!(
                "post-upgrade runtime credential invariant failed: {}",
                missing.join(", ")
            );
        }
        progress_info(format!(
            "runtime credential invariants preserved ({} env keys, {} secrets)",
            self.env_values.len(),
            self.secret_presence.len()
        ));
        Ok(())
    }
}

fn switch_current_release(current_link: &Path, release_root: &Path) -> Result<()> {
    if let Some(parent) = current_link.parent() {
        ensure_dir(parent)?;
    }
    let temporary_link = current_link.with_extension("new");
    if temporary_link.exists() || temporary_link.symlink_metadata().is_ok() {
        let _ = fs::remove_file(&temporary_link);
        let _ = fs::remove_dir_all(&temporary_link);
    }
    create_symlink(release_root, &temporary_link)?;
    fs::rename(&temporary_link, current_link).with_context(|| {
        format!(
            "failed to move {} into place as {}",
            temporary_link.display(),
            current_link.display()
        )
    })
}

fn write_managed_wrapper(install_root: &Path, state_root: &Path) -> Result<()> {
    let wrapper_path = wrapper_path()?;
    if let Some(parent) = wrapper_path.parent() {
        ensure_dir(parent)?;
    }
    let current_root = install_root.join("current");
    let launcher_binary =
        select_launch_binary(&current_root)?.unwrap_or_else(|| current_root.join("bin/ctox"));
    write_launch_wrapper(
        &wrapper_path,
        install_root,
        &current_root,
        state_root,
        &launcher_binary,
    )?;
    ensure_global_command_shim(&wrapper_path);
    Ok(())
}

fn sync_managed_launch_binaries(
    install_root: &Path,
    current_root: &Path,
    state_root: &Path,
) -> Result<()> {
    let bin_dir = install_root.join("bin");
    ensure_dir(&bin_dir)?;
    let current_binary = select_launch_binary(current_root)?.ok_or_else(|| {
        anyhow::anyhow!(
            "no real CTOX launch binary found below {}",
            current_root.display()
        )
    })?;
    write_launch_wrapper(
        &bin_dir.join("ctox"),
        install_root,
        current_root,
        state_root,
        &current_binary,
    )?;
    let current_desktop_host = current_root.join("bin/ctox-desktop-host");
    if current_desktop_host.is_file() {
        copy_launch_binary(&current_desktop_host, &bin_dir.join("ctox-desktop-host"))?;
    }
    if let Ok(wrapper) = wrapper_path() {
        ensure_global_command_shim(&wrapper);
    }
    Ok(())
}

fn select_launch_binary(current_root: &Path) -> Result<Option<PathBuf>> {
    for candidate in [
        current_root.join("bin/ctox-real"),
        current_root.join("bin/ctox"),
    ] {
        if candidate.is_file() && !is_shell_wrapper(&candidate)? {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

fn is_shell_wrapper(path: &Path) -> Result<bool> {
    let mut file =
        fs::File::open(path).with_context(|| format!("failed to inspect {}", path.display()))?;
    let mut marker = [0_u8; 2];
    let read = std::io::Read::read(&mut file, &mut marker)
        .with_context(|| format!("failed to read {}", path.display()))?;
    Ok(read == 2 && marker == *b"#!")
}

fn write_launch_wrapper(
    destination: &Path,
    install_root: &Path,
    current_root: &Path,
    state_root: &Path,
    launcher_binary: &Path,
) -> Result<()> {
    if let Some(parent) = destination.parent() {
        ensure_dir(parent)?;
    }
    let script = format!(
        "#!/usr/bin/env bash\nset -euo pipefail\nunset DYLD_LIBRARY_PATH DYLD_FALLBACK_LIBRARY_PATH DYLD_FRAMEWORK_PATH\nexport CTOX_ROOT=\"{}\"\nexport CTOX_STATE_ROOT=\"{}\"\nexport CTOX_INSTALL_ROOT=\"{}\"\nexec \"{}\" \"$@\"\n",
        current_root.display(),
        state_root.display(),
        install_root.display(),
        launcher_binary.display()
    );
    let mut file = fs::File::create(destination)
        .with_context(|| format!("failed to write {}", destination.display()))?;
    file.write_all(script.as_bytes())
        .with_context(|| format!("failed to populate {}", destination.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = file.metadata()?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(destination, permissions)?;
    }
    Ok(())
}

fn copy_launch_binary(source: &Path, destination: &Path) -> Result<()> {
    let temporary_destination = destination.with_extension("new");
    if temporary_destination.exists() || temporary_destination.symlink_metadata().is_ok() {
        let _ = fs::remove_file(&temporary_destination);
    }
    fs::copy(source, &temporary_destination).with_context(|| {
        format!(
            "failed to copy launch binary {} -> {}",
            source.display(),
            destination.display()
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&temporary_destination)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&temporary_destination, permissions)?;
    }
    if let Err(err) = fs::rename(&temporary_destination, destination) {
        if destination.exists() {
            fs::remove_file(destination).with_context(|| {
                format!(
                    "failed to remove existing launcher {}",
                    destination.display()
                )
            })?;
            fs::rename(&temporary_destination, destination).with_context(|| {
                format!(
                    "failed to move staged launch binary {} into {} after replacing existing file ({err})",
                    temporary_destination.display(),
                    destination.display()
                )
            })?;
        } else {
            return Err(err).with_context(|| {
                format!(
                    "failed to move staged launch binary {} into {}",
                    temporary_destination.display(),
                    destination.display()
                )
            });
        }
    }
    Ok(())
}

fn refresh_service_unit(
    current_root: &Path,
    state_root: &Path,
    install_root: Option<&Path>,
) -> Result<()> {
    if !cfg!(target_os = "linux") {
        return Ok(());
    }
    let Some(home_dir) = home_dir() else {
        return Ok(());
    };
    let service_dir = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir.join(".config"))
        .join("systemd/user");
    ensure_dir(&service_dir)?;
    let service_file = service_dir.join("ctox.service");
    let wrapper = wrapper_path()?;
    let install_root_export = install_root
        .map(|entry| format!("Environment=CTOX_INSTALL_ROOT={}\n", entry.display()))
        .unwrap_or_default();
    let contents = format!(
        "[Unit]\nDescription=CTOX Background Service\nAfter=network-online.target\nWants=network-online.target\nStartLimitIntervalSec=0\n\n[Service]\nType=simple\nWorkingDirectory={}\nEnvironment=CTOX_ROOT={}\nEnvironment=CTOX_STATE_ROOT={}\n{}ExecStart={} service --foreground\nRestart=always\nRestartSec=5\nKillMode=control-group\nTimeoutStopSec=20\n\n[Install]\nWantedBy=default.target\n",
        current_root.display(),
        current_root.display(),
        state_root.display(),
        install_root_export,
        wrapper.display()
    );
    fs::write(&service_file, contents)
        .with_context(|| format!("failed to write {}", service_file.display()))?;
    let marker = current_root.join("runtime/ctox_systemd_user.installed");
    if let Some(parent) = marker.parent() {
        ensure_dir(parent)?;
    }
    fs::write(&marker, "").with_context(|| format!("failed to update {}", marker.display()))?;

    install_ctox_watchdog_units(&service_dir)?;

    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();
    let _ = Command::new("systemctl")
        .args(["--user", "enable", "ctox.service"])
        .status();
    let _ = Command::new("systemctl")
        .args(["--user", "enable", "--now", "ctox-watchdog.timer"])
        .status();
    Ok(())
}

/// Writes the watchdog timer + service that re-starts ctox.service if it ever
/// drops out of the active state. `Restart=always` in the unit file does NOT
/// trigger after an explicit `systemctl stop` (e.g. from an upgrade pipeline
/// that crashed between stop and start, or a misbehaving skill that stopped
/// the service and never re-started it). The watchdog closes that loop with
/// a minutely guard: `is-active --quiet || start`. ConditionPathExists ensures
/// the timer stays dormant if the user uninstalled ctox.service entirely.
fn install_ctox_watchdog_units(service_dir: &Path) -> Result<()> {
    let watchdog_service = service_dir.join("ctox-watchdog.service");
    let watchdog_timer = service_dir.join("ctox-watchdog.timer");

    // The ExecStart guard runs `is-active --quiet ctox.service`; if exit != 0,
    // start it. We use `bash -c` because systemd ExecStart does not natively
    // chain `||`. We do not use `RemainAfterExit` since this is one-shot per
    // tick; the timer keeps re-firing.
    let watchdog_service_contents = "[Unit]\n\
Description=CTOX Background Service Watchdog\n\
ConditionPathExists=%h/.config/systemd/user/ctox.service\n\
\n\
[Service]\n\
Type=oneshot\n\
ExecStart=/bin/bash -c 'systemctl --user is-active --quiet ctox.service || systemctl --user start ctox.service'\n";

    let watchdog_timer_contents = "[Unit]\n\
Description=CTOX Background Service Watchdog Timer\n\
ConditionPathExists=%h/.config/systemd/user/ctox.service\n\
\n\
[Timer]\n\
OnBootSec=30s\n\
OnUnitActiveSec=60s\n\
AccuracySec=5s\n\
Persistent=true\n\
\n\
[Install]\n\
WantedBy=timers.target\n";

    fs::write(&watchdog_service, watchdog_service_contents)
        .with_context(|| format!("failed to write {}", watchdog_service.display()))?;
    fs::write(&watchdog_timer, watchdog_timer_contents)
        .with_context(|| format!("failed to write {}", watchdog_timer.display()))?;
    Ok(())
}

fn wrapper_path() -> Result<PathBuf> {
    let home_dir = home_dir().context("failed to resolve HOME for CTOX wrapper")?;
    Ok(home_dir.join(".local/bin/ctox"))
}

fn ensure_global_command_shim(wrapper_path: &Path) {
    if !cfg!(unix) {
        return;
    }
    let shim = Path::new("/usr/local/bin/ctox");
    if shim == wrapper_path {
        return;
    }
    if let Ok(metadata) = fs::symlink_metadata(shim) {
        if !metadata.file_type().is_symlink() {
            return;
        }
        let _ = fs::remove_file(shim);
    }
    if let Some(parent) = shim.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if create_symlink(wrapper_path, shim).is_ok() {
        return;
    }
    let _ = Command::new("sudo")
        .args([
            "-n",
            "ln",
            "-sfn",
            &wrapper_path.display().to_string(),
            "/usr/local/bin/ctox",
        ])
        .status();
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

fn default_install_root() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(DEFAULT_INSTALL_ROOT_RELATIVE_PATH)
}

fn default_state_root() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(DEFAULT_STATE_ROOT_RELATIVE_PATH)
}

fn default_cache_root() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(DEFAULT_CACHE_ROOT_RELATIVE_PATH)
}

fn default_github_api_base_string() -> String {
    DEFAULT_GITHUB_API_BASE.to_string()
}

fn default_release_name() -> String {
    format!(
        "v{}-{}",
        build_version(),
        current_utc().format("%Y%m%dT%H%M%SZ")
    )
}

fn release_name_for_source(source_root: &Path) -> Option<String> {
    let version =
        git_describe_version(source_root).or_else(|| cargo_manifest_version(source_root))?;
    Some(format!(
        "v{}-{}",
        version,
        current_utc().format("%Y%m%dT%H%M%SZ")
    ))
}

fn build_version() -> &'static str {
    env!("CTOX_BUILD_VERSION")
}

fn build_release_tag() -> String {
    let version = build_version();
    if version.starts_with('v') {
        version.to_string()
    } else {
        format!("v{version}")
    }
}

fn git_describe_version(source_root: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(source_root)
        .arg("describe")
        .arg("--tags")
        .arg("--dirty")
        .arg("--match")
        .arg("v[0-9]*")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if raw.is_empty() {
        return None;
    }

    Some(raw.strip_prefix('v').unwrap_or(raw.as_str()).to_string())
}

fn cargo_manifest_version(source_root: &Path) -> Option<String> {
    let cargo_toml = fs::read_to_string(source_root.join("Cargo.toml")).ok()?;
    cargo_toml
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("version = "))
        .map(|value| value.trim_matches('"').to_string())
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn now_rfc3339() -> String {
    current_utc().to_rfc3339()
}

fn current_utc() -> chrono::DateTime<Utc> {
    std::time::SystemTime::now().into()
}

fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("failed to create {}", path.display()))
}

fn absolutize_link_target(link_path: &Path, target: &Path) -> Result<PathBuf> {
    if target.is_absolute() {
        return Ok(target.to_path_buf());
    }
    let parent = link_path
        .parent()
        .with_context(|| format!("failed to resolve parent for {}", link_path.display()))?;
    Ok(parent.join(target))
}

#[cfg(unix)]
fn create_symlink(target: &Path, link_path: &Path) -> Result<()> {
    use std::os::unix::fs::symlink;

    if let Some(parent) = link_path.parent() {
        ensure_dir(parent)?;
    }
    symlink(target, link_path).with_context(|| {
        format!(
            "failed to create symlink {} -> {}",
            link_path.display(),
            target.display()
        )
    })
}

#[cfg(not(unix))]
fn create_symlink(target: &Path, link_path: &Path) -> Result<()> {
    if target.is_dir() {
        copy_filtered(target, link_path, &|_, _| false)
    } else {
        if let Some(parent) = link_path.parent() {
            ensure_dir(parent)?;
        }
        fs::copy(target, link_path).with_context(|| {
            format!(
                "failed to copy {} to {}",
                target.display(),
                link_path.display()
            )
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn upgrade_progress_lines_are_numbered_and_timed() {
        assert_eq!(
            format_progress_step(8, 3, "fetch binary release bundle"),
            "ctox upgrade | 00:08 | step 03 | fetch binary release bundle"
        );
        assert_eq!(
            format_progress_info(9, "installer output follows"),
            "ctox upgrade | 00:09 | info    | installer output follows"
        );
        assert_eq!(
            format_progress_done(80, "downloaded v1.2.3", 75),
            "ctox upgrade | 01:20 | done    | downloaded v1.2.3 | took 1m 15s"
        );
    }

    #[test]
    fn upgrade_help_is_a_non_mutating_top_level_path() {
        let args = vec!["--help".to_string()];
        let help = update_help_text(&args);

        assert!(help.contains("ctox upgrade [--stable|--dev]"));
        assert!(help.contains("without starting an upgrade"));
    }

    #[test]
    fn update_apply_help_is_a_non_mutating_subcommand_path() {
        let args = vec![
            "apply".to_string(),
            "--latest".to_string(),
            "--from-source".to_string(),
            "--help".to_string(),
        ];
        let help = update_help_text(&args);

        assert!(help.contains("ctox update apply --latest"));
        assert!(help.contains("without starting an update"));
    }

    #[test]
    fn update_lock_blocks_second_active_holder() {
        let temp = tempdir().unwrap();
        let layout = InstallLayout {
            workspace_root: temp.path().join("workspace"),
            active_root: temp.path().join("workspace"),
            install_root: Some(temp.path().join("install")),
            state_root: temp.path().join("state"),
            cache_root: temp.path().join("cache"),
        };
        ensure_dir(&layout.state_root).unwrap();

        let _lease = acquire_update_operation_lease(&layout, "upgrade-dev", Some("branch-main"))
            .expect("first holder should acquire lock");
        let err = acquire_update_operation_lease(&layout, "apply", Some("branch-main-next"))
            .expect_err("second holder should be rejected");
        let text = err.to_string();
        assert!(text.contains("another update operation is already active"));
        assert!(text.contains("operation=upgrade-dev"));
        assert!(text.contains("target_release=branch-main"));
    }

    #[test]
    fn stale_update_lock_is_replaced() {
        let temp = tempdir().unwrap();
        let layout = InstallLayout {
            workspace_root: temp.path().join("workspace"),
            active_root: temp.path().join("workspace"),
            install_root: Some(temp.path().join("install")),
            state_root: temp.path().join("state"),
            cache_root: temp.path().join("cache"),
        };
        ensure_dir(&layout.state_root).unwrap();
        let lock_path = layout.update_lock_path();
        fs::write(
            &lock_path,
            "pid=99999999\noperation=upgrade-dev\ntarget_release=branch-main-old\n",
        )
        .unwrap();

        let _lease = acquire_update_operation_lease(&layout, "apply", Some("branch-main-new"))
            .expect("stale lock should be replaced");
        let contents = fs::read_to_string(&lock_path).unwrap();
        assert!(contents.contains("operation=apply"));
        assert!(contents.contains("target_release=branch-main-new"));
    }

    #[test]
    fn non_terminal_update_phase_clears_finished_at() {
        let temp = tempdir().unwrap();
        let state_path = temp.path().join("update_state.json");
        persist_update_state(
            &state_path,
            &UpdateState {
                schema_version: 1,
                phase: "failed".to_string(),
                current_version: "test".to_string(),
                current_release: Some("branch-main-old".to_string()),
                target_release: Some("branch-main-bad".to_string()),
                previous_release: None,
                source: None,
                state_backup_path: None,
                started_at: Some(now_rfc3339()),
                finished_at: Some(now_rfc3339()),
                last_error: Some("boom".to_string()),
            },
        )
        .unwrap();

        persist_update_phase(&state_path, "building", None).unwrap();
        let state = load_update_state(&state_path).unwrap().unwrap();
        assert_eq!(state.phase, "building");
        assert!(state.finished_at.is_none());
        assert!(state.last_error.is_none());
    }

    #[test]
    fn runtime_credential_snapshot_detects_lost_azure_secret() {
        let temp = tempdir().unwrap();
        let mut env = std::collections::BTreeMap::new();
        env.insert("CTOX_API_PROVIDER".to_string(), "azure_foundry".to_string());
        env.insert(
            "CTOX_UPSTREAM_BASE_URL".to_string(),
            "https://example.openai.azure.com/openai/v1".to_string(),
        );
        env.insert(
            "AZURE_FOUNDRY_API_KEY".to_string(),
            "secret-value".to_string(),
        );
        runtime_env::save_runtime_env_map(temp.path(), &env).unwrap();

        let snapshot = RuntimeCredentialSnapshot::capture(temp.path()).unwrap();
        assert_eq!(snapshot.env_values.len(), 2);
        assert_eq!(snapshot.secret_presence, vec!["AZURE_FOUNDRY_API_KEY"]);

        secrets::delete_credential(temp.path(), "AZURE_FOUNDRY_API_KEY").unwrap();
        let err = snapshot.verify_preserved(temp.path()).unwrap_err();
        assert!(err
            .to_string()
            .contains("AZURE_FOUNDRY_API_KEY secret missing"));
    }

    #[test]
    fn state_backup_includes_sqlite_runtime_state() {
        let temp = tempdir().unwrap();
        let state_root = temp.path().join("state");
        ensure_dir(&state_root).unwrap();
        fs::write(state_root.join("ctox.sqlite3"), "main").unwrap();
        fs::write(state_root.join("ctox.sqlite3-wal"), "wal").unwrap();
        fs::write(state_root.join("ctox.sqlite3-shm"), "shm").unwrap();

        let backup = backup_state_root(&state_root).unwrap();

        assert_eq!(
            fs::read_to_string(backup.join("ctox.sqlite3")).unwrap(),
            "main"
        );
        assert_eq!(
            fs::read_to_string(backup.join("ctox.sqlite3-wal")).unwrap(),
            "wal"
        );
        assert_eq!(
            fs::read_to_string(backup.join("ctox.sqlite3-shm")).unwrap(),
            "shm"
        );
    }

    #[test]
    fn state_backup_tolerates_disappearing_sqlite_sidecars() {
        let temp = tempdir().unwrap();
        let state_root = temp.path().join("state");
        ensure_dir(&state_root).unwrap();
        fs::write(state_root.join("ctox.sqlite3"), "main").unwrap();
        fs::write(state_root.join("ctox.sqlite3-wal"), "wal").unwrap();
        fs::remove_file(state_root.join("ctox.sqlite3-wal")).unwrap();

        let backup = backup_state_root(&state_root).unwrap();

        assert_eq!(
            fs::read_to_string(backup.join("ctox.sqlite3")).unwrap(),
            "main"
        );
        assert!(!backup.join("ctox.sqlite3-wal").exists());
    }

    #[test]
    fn managed_layout_prefers_manifest_state_root_over_current_runtime_symlink() {
        let temp = tempdir().unwrap();
        let install_root = temp.path().join("install");
        let releases = install_root.join("releases");
        let release_root = releases.join("current-release");
        let stable_state_root = temp.path().join("state");
        ensure_dir(&release_root).unwrap();
        ensure_dir(&stable_state_root).unwrap();
        create_symlink(&release_root, &install_root.join("current")).unwrap();
        create_symlink(
            Path::new("/would/be/self/referential/after/switch"),
            &release_root.join("runtime"),
        )
        .unwrap();
        persist_install_manifest(
            &install_root.join(INSTALL_MANIFEST_FILE_NAME),
            &InstallManifest {
                schema_version: 1,
                install_root: install_root.clone(),
                state_root: stable_state_root.clone(),
                current_release: Some("current-release".to_string()),
                previous_release: None,
                adopted_from: None,
                release_channel: None,
                updated_at: now_rfc3339(),
            },
        )
        .unwrap();

        let layout = InstallLayout::resolve(&install_root.join("current")).unwrap();

        assert_eq!(layout.install_root.as_deref(), Some(install_root.as_path()));
        assert_eq!(layout.state_root, stable_state_root);
    }

    #[test]
    fn prune_old_releases_keeps_current_and_previous_only() {
        let temp = tempdir().unwrap();
        let releases = temp.path().join("releases");
        ensure_dir(&releases.join("current")).unwrap();
        ensure_dir(&releases.join("previous")).unwrap();
        ensure_dir(&releases.join("old")).unwrap();
        fs::write(releases.join("note.txt"), "keep non-release files").unwrap();

        let manifest = InstallManifest {
            schema_version: 1,
            install_root: temp.path().join("install"),
            state_root: temp.path().join("state"),
            current_release: Some("current".to_string()),
            previous_release: Some("previous".to_string()),
            adopted_from: None,
            release_channel: None,
            updated_at: now_rfc3339(),
        };

        prune_old_releases(&releases, &manifest);

        assert!(releases.join("current").is_dir());
        assert!(releases.join("previous").is_dir());
        assert!(!releases.join("old").exists());
        assert!(releases.join("note.txt").is_file());
    }
}
