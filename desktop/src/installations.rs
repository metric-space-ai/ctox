use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use dirs::{config_dir, data_local_dir};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum InstallationMode {
    #[default]
    Local,
    RemoteWebRtc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteAccessSettings {
    #[serde(default)]
    pub signaling_urls: Vec<String>,
    #[serde(default)]
    pub auth_token: String,
    #[serde(default)]
    pub password: String,
    #[serde(default = "default_room_id")]
    pub room_id: String,
    #[serde(default)]
    pub client_name: String,
    #[serde(default)]
    pub host_target: RemoteHostTarget,
    #[serde(default)]
    pub instance_source: RemoteInstanceSource,
    #[serde(default)]
    pub ssh_host: String,
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,
    #[serde(default)]
    pub ssh_user: String,
    #[serde(default)]
    pub ssh_password: String,
    #[serde(default = "default_install_root")]
    pub install_root: String,
    #[serde(default)]
    pub host_prepared: bool,
    /// Which install.sh invocation to use when provisioning a new remote host.
    #[serde(default)]
    pub install_channel: InstallChannel,
}

impl Default for RemoteAccessSettings {
    fn default() -> Self {
        Self {
            signaling_urls: vec!["wss://api.metricspace.org/signal".to_owned()],
            auth_token: String::new(),
            password: String::new(),
            room_id: default_room_id(),
            client_name: String::new(),
            host_target: RemoteHostTarget::Unspecified,
            instance_source: RemoteInstanceSource::Unspecified,
            ssh_host: String::new(),
            ssh_port: default_ssh_port(),
            ssh_user: String::new(),
            ssh_password: String::new(),
            install_root: default_install_root(),
            host_prepared: false,
            install_channel: InstallChannel::Stable,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteHostTarget {
    #[default]
    Unspecified,
    Localhost,
    Ssh,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteInstanceSource {
    #[default]
    Unspecified,
    AttachExisting,
    InstallNew,
}

/// How a fresh remote install should be fetched. Maps to `install.sh` flags.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum InstallChannel {
    /// Latest release binary (default). `curl … | bash`
    #[default]
    Stable,
    /// `main` branch, source build. `curl … | bash -s -- --dev`
    Dev,
    /// Upload the locally-checked-out sources via SSH and build there (the
    /// legacy flow; keep for air-gap / patched-branch scenarios).
    LocalCheckout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Installation {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub mode: InstallationMode,
    #[serde(default)]
    pub root_path: Option<PathBuf>,
    #[serde(default)]
    pub preferred_binary: Option<PathBuf>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub remote: RemoteAccessSettings,
    /// Last observed `ctox version` output for this installation. Cached so the
    /// UI can show the version label without re-polling on every frame.
    #[serde(default)]
    pub cached_version: Option<String>,
    /// Last successful version-probe time (Unix seconds).
    #[serde(default)]
    pub cached_version_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct LaunchTarget {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstallationRegistry {
    #[serde(default)]
    pub installations: Vec<Installation>,
}

impl InstallationRegistry {
    pub fn load() -> Result<Self> {
        let path = registry_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read installation registry from {}", path.display()))?;
        Ok(serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse installation registry from {}", path.display()))?)
    }

    pub fn save(&self) -> Result<()> {
        let path = registry_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let raw = serde_json::to_string_pretty(self)?;
        fs::write(&path, format!("{raw}\n"))
            .with_context(|| format!("failed to write installation registry to {}", path.display()))
    }

    pub fn add_installation_path(&mut self, root_path: PathBuf) -> Result<Installation> {
        validate_installation_root(&root_path)?;

        let canonical = root_path
            .canonicalize()
            .unwrap_or(root_path.clone());
        if let Some(existing) = self
            .installations
            .iter()
            .find(|entry| {
                entry.root_path
                    .as_deref()
                    .map(|path| paths_eq(path, &canonical))
                    .unwrap_or(false)
            })
            .cloned()
        {
            return Ok(existing);
        }

        let installation = Installation {
            id: Uuid::new_v4().to_string(),
            name: canonical
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("CTOX")
                .to_owned(),
            mode: InstallationMode::Local,
            root_path: Some(canonical),
            preferred_binary: None,
            env: BTreeMap::new(),
            remote: RemoteAccessSettings::default(),
            cached_version: None,
            cached_version_at: None,
        };
        self.installations.push(installation.clone());
        self.installations
            .sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
        Ok(installation)
    }

    pub fn add_remote_installation(&mut self, name: Option<String>) -> Installation {
        let installation = Installation {
            id: Uuid::new_v4().to_string(),
            name: name.unwrap_or_default(),
            mode: InstallationMode::RemoteWebRtc,
            root_path: None,
            preferred_binary: None,
            env: BTreeMap::new(),
            remote: RemoteAccessSettings::default(),
            cached_version: None,
            cached_version_at: None,
        };
        self.installations.push(installation.clone());
        self.installations
            .sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
        installation
    }

    pub fn remove(&mut self, installation_id: &str) {
        self.installations.retain(|entry| entry.id != installation_id);
    }
}

impl Installation {
    pub fn display_name(&self) -> String {
        let name = self.name.trim();
        if !name.is_empty() {
            return name.to_owned();
        }

        match self.mode {
            InstallationMode::Local => self
                .root_path
                .as_ref()
                .and_then(|path| path.file_name())
                .and_then(|value| value.to_str())
                .unwrap_or("CTOX")
                .to_owned(),
            InstallationMode::RemoteWebRtc => {
                let room = self.remote.room_id.trim();
                if !room.is_empty() {
                    room.to_owned()
                } else if !self.remote.ssh_host.trim().is_empty() {
                    self.remote.ssh_host.trim().to_owned()
                } else {
                    "Remote".to_owned()
                }
            }
        }
    }

    pub fn display_path(&self) -> String {
        match self.mode {
            InstallationMode::Local => self
                .root_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "(no path configured)".to_owned()),
            InstallationMode::RemoteWebRtc => {
                if !self.remote.ssh_host.trim().is_empty() {
                    format!("Remote · {}", self.remote.ssh_host.trim())
                } else if self.remote.host_target == RemoteHostTarget::Localhost {
                    "Remote · this machine".to_owned()
                } else {
                    let room = self.remote.room_id.trim();
                    if room.is_empty() {
                        "Remote connection".to_owned()
                    } else {
                        format!("Remote · room {room}")
                    }
                }
            }
        }
    }

    pub fn is_remote(&self) -> bool {
        self.mode == InstallationMode::RemoteWebRtc
    }

    pub fn is_local(&self) -> bool {
        self.mode == InstallationMode::Local
    }

    pub fn local_root_path(&self) -> Result<&Path> {
        self.root_path
            .as_deref()
            .context("local installation has no root path configured")
    }

    pub fn binary_candidates(&self) -> Vec<PathBuf> {
        let Some(root_path) = self.root_path.as_ref() else {
            return Vec::new();
        };
        let mut candidates = Vec::new();
        if let Some(preferred) = self.preferred_binary.clone() {
            candidates.push(preferred);
        }
        candidates.push(root_path.join("target/debug/ctox"));
        candidates.push(root_path.join("target/release/ctox"));
        candidates.push(root_path.join("runtime/local-bin/debug/ctox"));
        candidates.push(root_path.join("runtime/local-bin/release/ctox"));
        candidates
    }

    pub fn resolved_binary(&self) -> Option<PathBuf> {
        self.binary_candidates()
            .into_iter()
            .find(|candidate| candidate.is_file())
    }

    pub fn tui_launch_target(&self) -> Result<LaunchTarget> {
        self.launch_target(&["tui"])
    }

    pub fn command_launch_target(&self, args: &[&str]) -> Result<LaunchTarget> {
        self.launch_target(args)
    }

    fn launch_target(&self, args: &[&str]) -> Result<LaunchTarget> {
        let root_path = self.local_root_path()?.to_path_buf();
        resolve_ctox_launch_from_root(&root_path, &self.preferred_binary, &self.env, args)
    }
}

pub fn resolve_ctox_launch_from_root(
    root_path: &Path,
    preferred_binary: &Option<PathBuf>,
    env: &BTreeMap<String, String>,
    args: &[&str],
) -> Result<LaunchTarget> {
    let root_path = root_path.to_path_buf();
    let mut launch_env = env.clone();
    let mut candidates = Vec::new();
    if let Some(preferred) = preferred_binary.clone() {
        candidates.push(preferred);
    }
    candidates.push(root_path.join("target/debug/ctox"));
    candidates.push(root_path.join("target/release/ctox"));
    candidates.push(root_path.join("runtime/local-bin/debug/ctox"));
    candidates.push(root_path.join("runtime/local-bin/release/ctox"));

    if let Some(binary) = candidates.into_iter().find(|candidate| candidate.is_file()) {
        // The local desktop wrapper runs inside a PTY on top of runtime state that may live on
        // filesystems where SQLite WAL shared-memory mapping is unreliable. Force DELETE mode
        // here so the wrapper starts the real TUI instead of crashing on xShmMap I/O errors.
        launch_env
            .entry("CTOX_LCM_JOURNAL_MODE".to_owned())
            .or_insert_with(|| "delete".to_owned());
        return Ok(LaunchTarget {
            program: binary.display().to_string(),
            args: args.iter().map(|value| (*value).to_owned()).collect(),
            cwd: root_path.clone(),
            env: launch_env,
        });
    }

    anyhow::bail!(
        "No CTOX binary found in this root. Build CTOX first so the desktop app can launch the real TUI instead of compiling in the terminal."
    )
}

pub fn validate_installation_root(path: &Path) -> Result<()> {
    let cargo = path.join("Cargo.toml");
    let main_rs = path.join("src/main.rs");
    if !cargo.is_file() {
        anyhow::bail!("{} is not a CTOX install root: missing Cargo.toml", path.display());
    }
    if !main_rs.is_file() {
        anyhow::bail!("{} is not a CTOX install root: missing src/main.rs", path.display());
    }
    Ok(())
}

pub fn registry_path() -> Result<PathBuf> {
    let base = data_local_dir()
        .or_else(config_dir)
        .context("failed to resolve a local data directory for CTOX Desktop")?;
    Ok(base.join("ctox/desktop/installations.json"))
}

fn paths_eq(left: &Path, right: &Path) -> bool {
    let left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    left == right
}

fn default_room_id() -> String {
    "ctox-default".to_owned()
}

fn default_ssh_port() -> u16 {
    22
}

fn default_install_root() -> String {
    "~/ctox".to_owned()
}
