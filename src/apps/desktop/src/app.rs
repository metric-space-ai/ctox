use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    net::ToSocketAddrs,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context as AnyhowContext, Result};
use eframe::{
    egui::{
        self, Align, Button, Color32, Context, FontFamily, FontId, Frame, Id, Key, Layout,
        RichText, ScrollArea, Sense, SidePanel, Stroke, TextEdit, Ui, Visuals,
    },
    CreationContext,
};
use rfd::FileDialog;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    command_catalog::{is_allowed_ctox_args, CommandEntry, CommandGroup, COMMANDS},
    connector::{
        repo_root_from_manifest_dir, InstanceConnector, LocalProcessConnector, SessionKind,
        SessionSpec,
    },
    installations::{
        InstallChannel, Installation, InstallationDesktopState, InstallationMessage,
        InstallationMode, InstallationRegistry, InstallationUser, LaunchTarget,
        RemoteAccessSettings, RemoteHostTarget, RemoteInstanceSource,
    },
    provision::{ProvisionEvent, ProvisionRequest},
    terminal_backend::TerminalSession,
    terminal_emulator::{TerminalSnapshot, TERMINAL_DEFAULT_BG},
    version_check,
};

const LEFT_PANEL_WIDTH: f32 = 342.0;
const RIGHT_PANEL_WIDTH: f32 = 360.0;
const DETAIL_MAX_WIDTH: f32 = 980.0;
const DETAIL_CONTENT_WIDTH: f32 = 820.0;
const TERMINAL_FONT_SIZE_MIN: f32 = 8.6;
const TERMINAL_FONT_SIZE_MAX: f32 = 11.8;
const TERMINAL_ZOOM_MIN: f32 = 0.75;
const TERMINAL_ZOOM_MAX: f32 = 1.25;
const COMMAND_COMPOSER_MIN_HEIGHT: f32 = 28.0;
const COMMAND_COMPOSER_MAX_HEIGHT: f32 = 42.0;
const COMPOSER_MODELS: &[&str] = &["GPT-5.4", "Qwen/Qwen3.5-27B", "Qwen/Qwen3.5-35B-A3B"];
const COMPOSER_PRESETS: &[&str] = &["Quality", "Performance"];
const UI_WINDOW: Color32 = Color32::from_rgb(247, 247, 245);
const UI_PAPER: Color32 = Color32::from_rgb(255, 255, 255);
const UI_RAIL: Color32 = Color32::from_rgb(241, 241, 239);
const UI_LINE: Color32 = Color32::from_rgb(184, 184, 179);
const UI_LINE_SOFT: Color32 = Color32::from_rgb(217, 217, 213);
const UI_TEXT: Color32 = Color32::from_rgb(37, 37, 37);
const UI_MUTED: Color32 = Color32::from_rgb(112, 112, 108);
const UI_BLUE: Color32 = Color32::from_rgb(30, 136, 229);
const UI_GREEN: Color32 = Color32::from_rgb(33, 163, 102);
const UI_AMBER: Color32 = Color32::from_rgb(200, 134, 0);
const UI_RED: Color32 = Color32::from_rgb(189, 59, 53);

pub struct CtoxDesktopApp {
    registry: InstallationRegistry,
    connector: LocalProcessConnector,
    selected_installation_id: Option<String>,
    expanded_installation_id: Option<String>,
    tabs: Vec<DesktopTab>,
    active_tab_id: Option<String>,
    show_add_menu: bool,
    show_settings_view: bool,
    settings_page: Option<SettingsPage>,
    show_right_panel: bool,
    command_run: Option<CommandExecution>,
    command_extra_args: BTreeMap<String, String>,
    custom_command_input: String,
    last_command_result: Option<CommandResult>,
    terminal_focus: bool,
    notice: Option<String>,
    last_frame_terminal_input: Instant,
    composer_attachments: Vec<PathBuf>,
    composer_transcript: String,
    transcript_panel_open: bool,
    selected_model: &'static str,
    selected_preset: &'static str,
    terminal_zoom: f32,
    installation_statuses: BTreeMap<String, InstallationRuntimeStatus>,
    last_installation_status_poll: Instant,
    provision_rx: Option<Receiver<ProvisionEvent>>,
    provisioning_installation_id: Option<String>,
    provision_status: Option<String>,
    provision_log: Vec<String>,
    provision_running: bool,
    version_probe_rx: Option<Receiver<VersionProbeResult>>,
    latest_release_rx: Option<Receiver<Result<version_check::LatestRelease, String>>>,
    latest_release: Option<version_check::LatestRelease>,
    latest_release_error: Option<String>,
    upgrade_rx: Option<Receiver<UpgradeEvent>>,
    upgrading_installation_id: Option<String>,
    upgrade_log: Vec<String>,
    upgrade_running: bool,
    business_os_proxies: BTreeMap<String, BusinessOsProxySession>,
    business_user_inputs: BTreeMap<String, String>,
    business_user_verified_installations: BTreeSet<String>,
    ctox_message_inputs: BTreeMap<String, String>,
    new_user_name_inputs: BTreeMap<String, String>,
    new_user_email_inputs: BTreeMap<String, String>,
    new_user_password_inputs: BTreeMap<String, String>,
    new_user_role_inputs: BTreeMap<String, String>,
    admin_unlock_inputs: BTreeMap<String, String>,
    admin_unlocked_installations: BTreeSet<String>,
    add_flow: AddInstanceFlow,
    connect_draft: ConnectInstanceDraft,
    new_draft: NewInstanceDraft,
    remove_candidate_id: Option<String>,
    remove_delete_confirm: bool,
    admin_candidate_id: Option<String>,
}

struct VersionProbeResult {
    installation_id: String,
    version: Result<String, String>,
}

enum UpgradeEvent {
    Status(String),
    Finished(Result<String, String>),
}

struct DesktopTab {
    id: String,
    installation_id: String,
    title: String,
    kind: SessionKind,
    terminal: TerminalSession,
    last_size: (u16, u16),
}

struct CommandExecution {
    title: String,
    terminal: TerminalSession,
}

struct CommandResult {
    title: String,
    exit_code: Option<i32>,
    output: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AddInstanceFlow {
    Choice,
    Connect,
    New,
}

impl Default for AddInstanceFlow {
    fn default() -> Self {
        Self::Choice
    }
}

#[derive(Default)]
struct ConnectInstanceDraft {
    mode: ConnectMode,
    name: String,
    endpoint: String,
    room: String,
    room_password: String,
    user: String,
    user_password: String,
    remember_login: bool,
    ssh_host: String,
    ssh_user: String,
    ssh_password: String,
    install_root: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConnectMode {
    Local,
    Direct,
    Signaling,
    Ssh,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsPage {
    Identity,
    Address,
    Role,
    Admin,
    Version,
    Remove,
}

impl Default for ConnectMode {
    fn default() -> Self {
        Self::Local
    }
}

#[derive(Default)]
struct NewInstanceDraft {
    name: String,
    local: bool,
    host: String,
    ssh_user: String,
    ssh_password: String,
    install_root: String,
}

struct BusinessOsProxySession {
    url: String,
    tunnel: Option<Child>,
    server: Option<Child>,
}

impl Drop for BusinessOsProxySession {
    fn drop(&mut self) {
        if let Some(child) = self.tunnel.as_mut() {
            let _ = child.kill();
        }
        if let Some(child) = self.server.as_mut() {
            let _ = child.kill();
        }
    }
}

#[derive(Clone)]
struct InstallationRuntimeStatus {
    label: String,
    color: Color32,
}

#[derive(Clone)]
struct InstallationCardData {
    installation_id: String,
    title: String,
    subtitle: String,
    is_selected: bool,
    runtime_status: InstallationRuntimeStatus,
    version_label: Option<String>,
    update_available: bool,
    role_label: String,
    method_label: String,
    admin_unlocked: bool,
}

#[derive(Debug, Deserialize)]
struct DesktopServiceStatusSnapshot {
    running: bool,
    #[serde(default)]
    busy: bool,
    #[serde(default)]
    last_error: Option<String>,
}

impl CtoxDesktopApp {
    pub fn new(cc: &CreationContext<'_>) -> Result<Self> {
        apply_theme(&cc.egui_ctx);
        let mut registry = InstallationRegistry::load().unwrap_or_default();

        if registry.installations.is_empty() {
            if let Some(root) =
                repo_root_from_manifest_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).as_path())
            {
                if root.join("Cargo.toml").is_file() && root.join("src/main.rs").is_file() {
                    let _ = registry.add_installation_path(root);
                    let _ = registry.save();
                }
            }
        }
        if let Some(running_local) = discover_running_local_ctox() {
            let mut changed = false;
            for installation in registry
                .installations
                .iter_mut()
                .filter(|installation| installation.mode == InstallationMode::Local)
            {
                if installation.name.trim().starts_with('v') || installation.name.trim().is_empty()
                {
                    installation.name = "Local CTOX".to_owned();
                    changed = true;
                }
                let should_repoint = installation
                    .root_path
                    .as_ref()
                    .map(|path| {
                        let display = path.display().to_string();
                        display.contains("/.local/lib/ctox/current")
                            || display.contains("/.local/lib/ctox/releases/")
                    })
                    .unwrap_or(false);
                if should_repoint {
                    installation.root_path = Some(running_local.root.clone());
                    installation.preferred_binary = running_local.binary.clone();
                    if installation.name.trim().starts_with('v')
                        || installation.name.trim().is_empty()
                    {
                        installation.name = "Local CTOX".to_owned();
                    }
                    changed = true;
                }
            }
            if changed {
                let _ = registry.save();
            }
        }

        let selected_installation_id = registry.installations.first().map(|entry| entry.id.clone());
        let mut app = Self {
            registry,
            connector: LocalProcessConnector,
            selected_installation_id,
            expanded_installation_id: None,
            tabs: Vec::new(),
            active_tab_id: None,
            show_add_menu: false,
            show_settings_view: false,
            settings_page: None,
            show_right_panel: true,
            command_run: None,
            command_extra_args: BTreeMap::new(),
            custom_command_input: String::new(),
            last_command_result: None,
            terminal_focus: true,
            notice: None,
            last_frame_terminal_input: Instant::now() - Duration::from_secs(1),
            composer_attachments: Vec::new(),
            composer_transcript: String::new(),
            transcript_panel_open: false,
            selected_model: COMPOSER_MODELS[0],
            selected_preset: COMPOSER_PRESETS[0],
            terminal_zoom: 1.0,
            installation_statuses: BTreeMap::new(),
            last_installation_status_poll: Instant::now() - Duration::from_secs(10),
            provision_rx: None,
            provisioning_installation_id: None,
            provision_status: None,
            provision_log: Vec::new(),
            provision_running: false,
            version_probe_rx: None,
            latest_release_rx: None,
            latest_release: None,
            latest_release_error: None,
            upgrade_rx: None,
            upgrading_installation_id: None,
            upgrade_log: Vec::new(),
            upgrade_running: false,
            business_os_proxies: BTreeMap::new(),
            business_user_inputs: BTreeMap::new(),
            business_user_verified_installations: BTreeSet::new(),
            ctox_message_inputs: BTreeMap::new(),
            new_user_name_inputs: BTreeMap::new(),
            new_user_email_inputs: BTreeMap::new(),
            new_user_password_inputs: BTreeMap::new(),
            new_user_role_inputs: BTreeMap::new(),
            admin_unlock_inputs: BTreeMap::new(),
            admin_unlocked_installations: BTreeSet::new(),
            add_flow: AddInstanceFlow::Choice,
            connect_draft: ConnectInstanceDraft::default(),
            new_draft: default_new_instance_draft(),
            remove_candidate_id: None,
            remove_delete_confirm: false,
            admin_candidate_id: None,
        };
        app.spawn_latest_release_probe();
        app.spawn_version_probes_for_all();
        Ok(app)
    }

    /// Kick off a background thread that fetches the latest release from
    /// GitHub and delivers the result via `latest_release_rx`. Overwrites any
    /// in-flight probe.
    fn spawn_latest_release_probe(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.latest_release_rx = Some(rx);
        std::thread::spawn(move || {
            let result = version_check::fetch_latest_release().map_err(|e| e.to_string());
            let _ = tx.send(result);
        });
    }

    /// Spawn a probe for every installation and deliver (id, version) tuples
    /// over `version_probe_rx`.
    fn spawn_version_probes_for_all(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.version_probe_rx = Some(rx);
        for installation in &self.registry.installations {
            let tx = tx.clone();
            let id = installation.id.clone();
            let kind = installation_probe_kind(installation);
            if let Some(kind) = kind {
                std::thread::spawn(move || {
                    let version = match kind {
                        VersionProbeKind::Local { binary } => {
                            version_check::probe_local_version(&binary).map_err(|e| e.to_string())
                        }
                        VersionProbeKind::Ssh {
                            user,
                            host,
                            port,
                            password,
                        } => version_check::probe_remote_version(&user, &host, port, &password)
                            .map_err(|e| e.to_string()),
                    };
                    let _ = tx.send(VersionProbeResult {
                        installation_id: id,
                        version,
                    });
                });
            }
        }
    }

    fn desktop_state_for(&self, installation_id: &str) -> InstallationDesktopState {
        self.registry
            .desktop
            .get(installation_id)
            .cloned()
            .unwrap_or_default()
    }

    fn display_name_for(&self, installation: &Installation) -> String {
        self.registry
            .desktop
            .get(&installation.id)
            .and_then(|state| state.display_name.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| installation.display_name())
    }

    fn role_for(&self, installation: &Installation) -> String {
        if self.admin_unlocked_installations.contains(&installation.id) {
            return "Admin".to_owned();
        }
        if installation.mode == InstallationMode::Local {
            return "Admin".to_owned();
        }
        if installation.mode == InstallationMode::RemoteWebRtc
            && installation.remote.host_target == RemoteHostTarget::Ssh
            && !installation.remote.ssh_password.trim().is_empty()
        {
            return "Admin".to_owned();
        }
        self.registry
            .desktop
            .get(&installation.id)
            .map(|state| state.role.trim())
            .filter(|role| !role.is_empty())
            .unwrap_or("User")
            .to_owned()
    }

    fn method_label_for(installation: &Installation) -> &'static str {
        match installation.mode {
            InstallationMode::Local => "Local",
            InstallationMode::RemoteWebRtc => {
                if installation.remote.host_target == RemoteHostTarget::Ssh {
                    "SSH"
                } else if installation.env.contains_key("CTOX_BUSINESS_OS_URL") {
                    "URL/IP"
                } else {
                    "Peer2Peer"
                }
            }
        }
    }

    fn display_version_for(installation: &Installation) -> Option<String> {
        let cached = installation.cached_version.as_deref().map(str::trim);
        if let Some(version) = cached.filter(|value| !value.is_empty()) {
            if version != "0.1.0" || !installation.name.starts_with('v') {
                return Some(version.to_owned());
            }
        }
        installation
            .name
            .starts_with('v')
            .then(|| installation.name.clone())
    }

    /// Pull any pending version-probe and latest-release updates from their
    /// channels and fold them into app state. Safe to call every frame.
    fn drain_version_signals(&mut self) {
        if let Some(rx) = self.version_probe_rx.as_ref() {
            while let Ok(result) = rx.try_recv() {
                let VersionProbeResult {
                    installation_id,
                    version,
                } = result;
                if let Some(inst) = self
                    .registry
                    .installations
                    .iter_mut()
                    .find(|entry| entry.id == installation_id)
                {
                    match version {
                        Ok(raw) => {
                            inst.cached_version = Some(raw);
                            inst.cached_version_at = Some(unix_now());
                        }
                        Err(_) => {
                            // Keep the last known good version on failure.
                        }
                    }
                }
            }
            let _ = self.registry.save();
        }
        if let Some(rx) = self.latest_release_rx.as_ref() {
            if let Ok(result) = rx.try_recv() {
                match result {
                    Ok(release) => {
                        self.latest_release = Some(release);
                        self.latest_release_error = None;
                    }
                    Err(err) => {
                        self.latest_release_error = Some(err);
                    }
                }
                self.latest_release_rx = None;
            }
        }
        if let Some(rx) = self.upgrade_rx.as_ref() {
            while let Ok(event) = rx.try_recv() {
                match event {
                    UpgradeEvent::Status(line) => self.upgrade_log.push(line),
                    UpgradeEvent::Finished(Ok(summary)) => {
                        self.upgrade_log.push(summary);
                        self.upgrade_running = false;
                    }
                    UpgradeEvent::Finished(Err(err)) => {
                        self.upgrade_log.push(format!("ERROR: {err}"));
                        self.upgrade_running = false;
                    }
                }
            }
            if !self.upgrade_running {
                // Re-probe version once the upgrade run settles so the UI
                // reflects the new version number.
                self.spawn_version_probes_for_all();
            }
        }
    }

    /// Returns true if the cached version for this installation differs from
    /// the resolved latest release tag.
    fn installation_update_available(&self, installation: &Installation) -> bool {
        let Some(latest) = self.latest_release.as_ref() else {
            return false;
        };
        let Some(installed) = installation.cached_version.as_deref() else {
            return false;
        };
        version_check::update_available(installed, &latest.tag_name)
    }

    /// Start an upgrade run against the given installation. Local: spawn
    /// `<binary> upgrade`. Remote SSH: `ssh … ctox upgrade`. Both stream
    /// output lines through `upgrade_rx`.
    fn start_upgrade(&mut self, installation_id: &str) {
        if self.upgrade_running {
            return;
        }
        let Some(installation) = self
            .registry
            .installations
            .iter()
            .find(|entry| entry.id == installation_id)
            .cloned()
        else {
            self.notice = Some("installation not found".to_owned());
            return;
        };
        let kind = installation_probe_kind(&installation);
        let Some(kind) = kind else {
            self.notice = Some("this installation mode has no upgrade action".to_owned());
            return;
        };
        let (tx, rx) = mpsc::channel();
        self.upgrade_rx = Some(rx);
        self.upgrading_installation_id = Some(installation.id.clone());
        self.upgrade_log.clear();
        self.upgrade_running = true;
        std::thread::spawn(move || run_upgrade(kind, tx));
    }

    fn selected_installation(&self) -> Option<&Installation> {
        let selected = self.selected_installation_id.as_deref()?;
        self.registry
            .installations
            .iter()
            .find(|entry| entry.id == selected)
    }

    fn open_folder_dialog(&mut self) {
        let Some(path) = FileDialog::new()
            .set_title("Choose CTOX folder")
            .pick_folder()
        else {
            return;
        };

        match self.registry.add_installation_path(path) {
            Ok(installation) => {
                self.selected_installation_id = Some(installation.id);
                self.expanded_installation_id = None;
                if let Err(error) = self.registry.save() {
                    self.notice = Some(error.to_string());
                } else {
                    self.notice = Some("Installation added.".to_owned());
                }
            }
            Err(error) => self.notice = Some(error.to_string()),
        }
    }

    fn add_remote_installation_with_source(&mut self, source: RemoteInstanceSource) {
        let installation = self.registry.add_remote_installation(None);
        let installation_id = installation.id.clone();
        if let Some(entry) = self
            .registry
            .installations
            .iter_mut()
            .find(|entry| entry.id == installation.id)
        {
            entry.remote.instance_source = source;
            if source == RemoteInstanceSource::InstallNew {
                entry.remote.host_target = RemoteHostTarget::Localhost;
            }
        }
        self.selected_installation_id = Some(installation_id.clone());
        self.expanded_installation_id = Some(installation_id);
        self.show_add_menu = false;
        if let Err(error) = self.registry.save() {
            self.notice = Some(error.to_string());
        }
    }

    fn create_connect_instance_from_draft(&mut self) {
        let endpoint = self.connect_draft.endpoint.trim();
        let room = self.connect_draft.room.trim();
        let room_password = self.connect_draft.room_password.trim();
        let user = self.connect_draft.user.trim();
        let user_password = self.connect_draft.user_password.trim();
        let ssh_host = self.connect_draft.ssh_host.trim();
        let ssh_user = self.connect_draft.ssh_user.trim();
        let ssh_password = self.connect_draft.ssh_password.trim();

        let display_seed = match self.connect_draft.mode {
            ConnectMode::Local => "Local CTOX",
            ConnectMode::Direct | ConnectMode::Signaling => endpoint,
            ConnectMode::Ssh => ssh_host,
        };
        match self.connect_draft.mode {
            ConnectMode::Local => {}
            ConnectMode::Direct => {
                if endpoint.is_empty() {
                    self.notice = Some("URL/IP fehlt.".to_owned());
                    return;
                }
                if user.is_empty() || user_password.is_empty() {
                    self.notice =
                        Some("URL/IP Verbindung braucht User und User-Passwort.".to_owned());
                    return;
                }
            }
            ConnectMode::Signaling => {
                if endpoint.is_empty() {
                    self.notice = Some("Peer2Peer Server URL fehlt.".to_owned());
                    return;
                }
                if room.is_empty() {
                    self.notice = Some("Peer2Peer Room fehlt.".to_owned());
                    return;
                }
                if room_password.is_empty() {
                    self.notice = Some("Peer2Peer Room Password fehlt.".to_owned());
                    return;
                }
                if user.is_empty() || user_password.is_empty() {
                    self.notice =
                        Some("Peer2Peer Verbindung braucht User und User-Passwort.".to_owned());
                    return;
                }
            }
            ConnectMode::Ssh => {
                if ssh_host.is_empty() || ssh_user.is_empty() || ssh_password.is_empty() {
                    self.notice =
                        Some("SSH Verbindung braucht Host, SSH-User und SSH-Passwort.".to_owned());
                    return;
                }
            }
        }

        let ssh_probe = if self.connect_draft.mode == ConnectMode::Ssh {
            match verify_ssh_ctox_connection(
                ssh_host,
                ssh_user,
                ssh_password,
                self.connect_draft.install_root.trim(),
            ) {
                Ok(result) => Some(result),
                Err(error) => {
                    self.notice = Some(format!("SSH Verbindung fehlgeschlagen: {error}"));
                    return;
                }
            }
        } else {
            None
        };

        if self.connect_draft.mode == ConnectMode::Direct {
            let normalized = normalize_business_os_endpoint(endpoint);
            if let Err(error) = verify_direct_business_os_endpoint(&normalized) {
                self.notice = Some(format!("Business OS nicht erreichbar: {error}"));
                return;
            }
        }

        if self.connect_draft.mode == ConnectMode::Local {
            let running_local = discover_running_local_ctox();
            let path = if self.connect_draft.install_root.trim().is_empty() {
                running_local
                    .as_ref()
                    .map(|discovery| discovery.root.clone())
                    .unwrap_or_else(|| {
                        PathBuf::from(shellexpand_tilde(
                            "/Users/michaelwelsch/.local/lib/ctox/current",
                        ))
                    })
            } else {
                PathBuf::from(shellexpand_tilde(self.connect_draft.install_root.trim()))
            };
            match self.registry.add_installation_path(path) {
                Ok(installation) => {
                    let installation_id = installation.id.clone();
                    if let Some(entry) = self
                        .registry
                        .installations
                        .iter_mut()
                        .find(|entry| entry.id == installation_id)
                    {
                        entry.name =
                            non_empty_or(self.connect_draft.name.trim(), &entry.display_name())
                                .to_owned();
                        entry.remote.business_user.clear();
                        entry.remote.business_password.clear();
                        if self.connect_draft.install_root.trim().is_empty() {
                            entry.preferred_binary = running_local
                                .as_ref()
                                .and_then(|discovery| discovery.binary.clone());
                        }
                    }
                    let desktop_state = self
                        .registry
                        .desktop
                        .entry(installation_id.clone())
                        .or_default();
                    desktop_state.display_name =
                        non_empty_opt(self.connect_draft.name.trim()).map(str::to_owned);
                    desktop_state.remember_login = false;
                    desktop_state.logged_in = true;
                    desktop_state.role = "Admin".to_owned();
                    desktop_state.admin_unlocked = true;
                    self.admin_unlocked_installations
                        .insert(installation_id.clone());
                    self.selected_installation_id = Some(installation_id);
                    self.active_tab_id = None;
                    self.show_add_menu = false;
                    self.show_settings_view = false;
                    self.add_flow = AddInstanceFlow::Choice;
                    self.connect_draft = ConnectInstanceDraft::default();
                    if let Err(error) = self.registry.save() {
                        self.notice = Some(error.to_string());
                    } else {
                        self.notice = Some("Verbunden.".to_owned());
                    }
                    self.spawn_version_probes_for_all();
                }
                Err(error) => self.notice = Some(format!("Local nicht verbunden: {error}")),
            }
            return;
        }

        let installation = self.registry.add_remote_installation(Some(
            non_empty_or(self.connect_draft.name.trim(), display_seed).to_owned(),
        ));
        let installation_id = installation.id.clone();
        if let Some(entry) = self
            .registry
            .installations
            .iter_mut()
            .find(|entry| entry.id == installation_id)
        {
            entry.remote.instance_source = RemoteInstanceSource::AttachExisting;
            entry.remote.client_name = user.to_owned();
            if self.connect_draft.remember_login {
                entry.remote.business_user = user.to_owned();
                entry.remote.business_password = user_password.to_owned();
            }
            match self.connect_draft.mode {
                ConnectMode::Local => {}
                ConnectMode::Direct => {
                    entry.remote.host_target = RemoteHostTarget::Localhost;
                    entry.env.insert(
                        "CTOX_BUSINESS_OS_URL".to_owned(),
                        normalize_business_os_endpoint(endpoint),
                    );
                }
                ConnectMode::Signaling => {
                    entry.remote.host_target = RemoteHostTarget::Unspecified;
                    entry.remote.signaling_urls = vec![endpoint.to_owned()];
                    entry.remote.room_id = room.to_owned();
                    entry.remote.password = room_password.to_owned();
                    entry.env.remove("CTOX_BUSINESS_OS_URL");
                }
                ConnectMode::Ssh => {
                    let (detected_root, version) = ssh_probe
                        .clone()
                        .unwrap_or_else(|| ("~/ctox".to_owned(), String::new()));
                    entry.remote.host_target = RemoteHostTarget::Ssh;
                    entry.remote.ssh_host = ssh_host.to_owned();
                    entry.remote.ssh_user = ssh_user.to_owned();
                    entry.remote.ssh_password = ssh_password.to_owned();
                    entry.remote.install_root = detected_root;
                    entry.remote.password = ssh_password.to_owned();
                    if !version.trim().is_empty() {
                        entry.cached_version = Some(version);
                    }
                    entry.env.remove("CTOX_BUSINESS_OS_URL");
                }
            }
        }
        let desktop_state = self
            .registry
            .desktop
            .entry(installation_id.clone())
            .or_default();
        desktop_state.display_name =
            non_empty_opt(self.connect_draft.name.trim()).map(str::to_owned);
        desktop_state.remember_login = self.connect_draft.remember_login;
        desktop_state.logged_in = true;
        desktop_state.role = if self.connect_draft.mode == ConnectMode::Ssh {
            "Admin".to_owned()
        } else {
            "User".to_owned()
        };
        self.selected_installation_id = Some(installation_id.clone());
        self.active_tab_id = None;
        self.show_add_menu = false;
        self.add_flow = AddInstanceFlow::Choice;
        if self.connect_draft.mode == ConnectMode::Ssh {
            self.admin_unlocked_installations
                .insert(installation_id.clone());
            if let Some(desktop_state) = self.registry.desktop.get_mut(&installation_id) {
                desktop_state.admin_unlocked = true;
            }
        }
        self.show_settings_view = false;
        self.connect_draft = ConnectInstanceDraft::default();
        if let Err(error) = self.registry.save() {
            self.notice = Some(error.to_string());
        } else {
            self.notice = Some("Verbunden.".to_owned());
        }
        self.spawn_version_probes_for_all();
    }

    fn create_new_instance_from_draft(&mut self) {
        if self.new_draft.local {
            let root = non_empty_or(
                self.new_draft.install_root.trim(),
                "/Users/michaelwelsch/.local/lib/ctox/current",
            );
            let installation = Installation {
                id: Uuid::new_v4().to_string(),
                name: non_empty_or(self.new_draft.name.trim(), "Local CTOX").to_owned(),
                mode: InstallationMode::Local,
                root_path: Some(PathBuf::from(shellexpand_tilde(root))),
                preferred_binary: None,
                env: BTreeMap::new(),
                remote: RemoteAccessSettings {
                    password: self.new_draft.ssh_password.trim().to_owned(),
                    ..RemoteAccessSettings::default()
                },
                cached_version: None,
                cached_version_at: None,
            };
            let installation_id = installation.id.clone();
            self.registry.installations.push(installation);
            let desktop_state = self
                .registry
                .desktop
                .entry(installation_id.clone())
                .or_default();
            desktop_state.display_name =
                non_empty_opt(self.new_draft.name.trim()).map(str::to_owned);
            desktop_state.role = "Admin".to_owned();
            desktop_state.admin_unlocked = true;
            desktop_state.logged_in = true;
            self.selected_installation_id = Some(installation_id.clone());
            self.admin_unlocked_installations.insert(installation_id);
            self.active_tab_id = None;
            self.show_add_menu = false;
            self.add_flow = AddInstanceFlow::Choice;
            self.new_draft = default_new_instance_draft();
            if let Err(error) = self.registry.save() {
                self.notice = Some(error.to_string());
            } else {
                self.notice = Some("Lokale CTOX Installation angelegt. Du bist Admin.".to_owned());
            }
            self.spawn_version_probes_for_all();
            return;
        }

        let host = self.new_draft.host.trim();
        let ssh_user = self.new_draft.ssh_user.trim();
        let ssh_password = self.new_draft.ssh_password.trim();
        if host.is_empty() {
            self.notice = Some("Host/IP fehlt.".to_owned());
            return;
        }
        if ssh_user.is_empty() {
            self.notice = Some("SSH User fehlt.".to_owned());
            return;
        }
        if ssh_password.is_empty() {
            self.notice = Some("SSH/Sudo Password fehlt.".to_owned());
            return;
        }
        let installation = self.registry.add_remote_installation(Some(
            non_empty_or(self.new_draft.name.trim(), host).to_owned(),
        ));
        let installation_id = installation.id.clone();
        if let Some(entry) = self
            .registry
            .installations
            .iter_mut()
            .find(|entry| entry.id == installation_id)
        {
            entry.remote.instance_source = RemoteInstanceSource::InstallNew;
            entry.remote.host_target = RemoteHostTarget::Ssh;
            entry.remote.ssh_host = host.to_owned();
            entry.remote.ssh_user = ssh_user.to_owned();
            entry.remote.ssh_password = ssh_password.to_owned();
            entry.remote.install_root = non_empty_or(
                self.new_draft.install_root.trim(),
                "~/.local/lib/ctox/current",
            )
            .to_owned();
            entry.remote.install_channel = InstallChannel::Stable;
        }
        let desktop_state = self
            .registry
            .desktop
            .entry(installation_id.clone())
            .or_default();
        desktop_state.display_name = non_empty_opt(self.new_draft.name.trim()).map(str::to_owned);
        desktop_state.role = "Admin".to_owned();
        desktop_state.admin_unlocked = true;
        desktop_state.logged_in = true;
        self.selected_installation_id = Some(installation_id.clone());
        self.active_tab_id = None;
        self.show_add_menu = false;
        self.add_flow = AddInstanceFlow::Choice;
        self.admin_unlocked_installations
            .insert(installation_id.clone());
        self.new_draft = default_new_instance_draft();
        if let Err(error) = self.registry.save() {
            self.notice = Some(error.to_string());
        } else {
            self.notice = Some("Remote CTOX Installation angelegt. Du bist Admin.".to_owned());
        }
        self.spawn_version_probes_for_all();
    }

    fn remove_selected_installation(&mut self) {
        let Some(installation_id) = self.selected_installation_id.clone() else {
            return;
        };
        self.remove_installation_connection(&installation_id);
    }

    fn request_remove_installation(&mut self, installation_id: String) {
        self.remove_candidate_id = Some(installation_id);
        self.remove_delete_confirm = false;
        self.show_add_menu = false;
    }

    fn remove_installation_connection(&mut self, installation_id: &str) {
        let installation_id = installation_id.to_owned();
        self.registry.remove(&installation_id);
        self.tabs
            .retain(|tab| tab.installation_id != installation_id);
        self.active_tab_id = self.tabs.first().map(|tab| tab.id.clone());
        self.business_os_proxies.remove(&installation_id);
        self.business_user_inputs.remove(&installation_id);
        self.business_user_verified_installations
            .remove(&installation_id);
        self.ctox_message_inputs.remove(&installation_id);
        self.admin_unlock_inputs.remove(&installation_id);
        self.admin_unlocked_installations.remove(&installation_id);
        self.selected_installation_id = self
            .registry
            .installations
            .first()
            .map(|entry| entry.id.clone());
        if self.expanded_installation_id.as_deref() == Some(installation_id.as_str()) {
            self.expanded_installation_id = None;
        }
        if self.remove_candidate_id.as_deref() == Some(installation_id.as_str()) {
            self.remove_candidate_id = None;
            self.remove_delete_confirm = false;
        }
        if let Err(error) = self.registry.save() {
            self.notice = Some(error.to_string());
        } else {
            self.notice = Some("Instanz-Verbindung entfernt.".to_owned());
        }
    }

    fn delete_entire_installation(&mut self, installation_id: &str) {
        let Some(installation) = self
            .registry
            .installations
            .iter()
            .find(|entry| entry.id == installation_id)
            .cloned()
        else {
            return;
        };

        match delete_installation_payload(&installation) {
            Ok(message) => {
                self.remove_installation_connection(installation_id);
                self.notice = Some(message);
            }
            Err(error) => {
                self.notice = Some(format!("Instanz konnte nicht gelöscht werden: {error}"));
            }
        }
    }

    fn spawn_tui_tab(&mut self) {
        let Some(installation_id) = self.selected_installation_id.clone() else {
            self.notice = Some("No installation selected.".to_owned());
            return;
        };
        self.focus_or_open_tui(&installation_id);
    }

    fn focus_or_open_tui(&mut self, installation_id: &str) {
        if let Some(existing_tab_id) = self.find_tui_tab_id(installation_id) {
            self.active_tab_id = Some(existing_tab_id);
            self.terminal_focus = true;
            return;
        }

        let Some(installation) = self
            .registry
            .installations
            .iter()
            .find(|entry| entry.id == installation_id)
            .cloned()
        else {
            self.notice = Some("Installation not found.".to_owned());
            return;
        };

        if !installation_ready_for_tui(&installation) {
            self.notice = Some(match installation.mode {
                InstallationMode::Local => "Please choose a local CTOX installation first.".to_owned(),
                InstallationMode::RemoteWebRtc => {
                    "Open CTOX Settings / Communication on the target host and configure Peer2Peer Server, Remote Room, and Remote Password first.".to_owned()
                }
            });
            return;
        }

        if installation.remote.host_target != RemoteHostTarget::Ssh {
            if let Err(error) = ensure_remote_tui_host_started(&installation) {
                self.notice = Some(error.to_string());
                return;
            }
        }

        match self.connector.launch_tui(&installation) {
            Ok(launch) => self.spawn_tab(installation.id, launch.title, launch.kind, launch.spec),
            Err(error) => self.notice = Some(error.to_string()),
        }
    }

    fn run_command(&mut self, command: &CommandEntry) {
        let Some(installation) = self.selected_installation().cloned() else {
            self.notice = Some("No installation selected.".to_owned());
            return;
        };
        if !command.runnable {
            self.notice = Some("Dieser Eintrag ist vorerst nur dokumentiert.".to_owned());
            return;
        }

        let extra_args = self
            .command_extra_args
            .get(command.example)
            .map(|value| parse_extra_args(value))
            .unwrap_or_default();

        match self
            .connector
            .launch_command_with_extra_args(&installation, command, &extra_args)
        {
            Ok(launch) => match TerminalSession::spawn(&launch.spec, 24, 120) {
                Ok(terminal) => {
                    if let Some(previous) = self.command_run.take() {
                        previous.terminal.close();
                    }
                    self.command_run = Some(CommandExecution {
                        title: launch.title,
                        terminal,
                    });
                    self.last_command_result = None;
                    self.notice = Some(format!("{} gestartet.", command.title));
                }
                Err(error) => self.notice = Some(error.to_string()),
            },
            Err(error) => self.notice = Some(error.to_string()),
        }
    }

    fn run_custom_command(&mut self) {
        let Some(installation) = self.selected_installation().cloned() else {
            self.notice = Some("No installation selected.".to_owned());
            return;
        };

        let mut args = parse_extra_args(&self.custom_command_input);
        if args.first().map(String::as_str) == Some("ctox") {
            args.remove(0);
        }
        if args.is_empty() {
            self.notice = Some("Enter a CTOX command first.".to_owned());
            return;
        }
        if installation.is_remote() && !is_allowed_ctox_args(&args) {
            self.notice = Some("Only approved CTOX commands are allowed remotely.".to_owned());
            return;
        }

        match self.connector.launch_custom_command(&installation, &args) {
            Ok(launch) => match TerminalSession::spawn(&launch.spec, 24, 120) {
                Ok(terminal) => {
                    if let Some(previous) = self.command_run.take() {
                        previous.terminal.close();
                    }
                    self.command_run = Some(CommandExecution {
                        title: launch.title,
                        terminal,
                    });
                    self.last_command_result = None;
                    self.notice = Some("CTOX-Befehl gestartet.".to_owned());
                }
                Err(error) => self.notice = Some(error.to_string()),
            },
            Err(error) => self.notice = Some(error.to_string()),
        }
    }

    fn pick_composer_attachments(&mut self) {
        let Some(files) = FileDialog::new().set_title("Attach files").pick_files() else {
            return;
        };
        for file in files {
            if !self
                .composer_attachments
                .iter()
                .any(|existing| existing == &file)
            {
                self.composer_attachments.push(file);
            }
        }
    }

    fn send_composer_message(&mut self) {
        let body = self.custom_command_input.trim();
        let transcript = self.composer_transcript.trim();
        if body.is_empty() && transcript.is_empty() && self.composer_attachments.is_empty() {
            self.notice = Some("Enter a message first.".to_owned());
            return;
        }

        let preset_label = composer_runtime_label(self.selected_model, self.selected_preset);
        let mut payload = String::new();
        payload.push_str(&format!(
            "[Desktop composer]\nModel: {}\nPreset: {}\n",
            self.selected_model, preset_label
        ));
        if !self.composer_attachments.is_empty() {
            payload.push_str("Attachments:\n");
            for file in &self.composer_attachments {
                payload.push_str("- ");
                payload.push_str(&file.display().to_string());
                payload.push('\n');
            }
        }
        if !transcript.is_empty() {
            payload.push_str("Transcript:\n");
            payload.push_str(transcript);
            payload.push('\n');
        }
        if !body.is_empty() {
            payload.push('\n');
            payload.push_str(body);
        }

        if let Some(tab) = self.active_tab_mut() {
            if tab.kind == SessionKind::Tui {
                let _ = tab.terminal.write_input(payload.as_bytes(), true);
                let _ = tab.terminal.write_input(b"\n", true);
                self.notice = Some("Message sent to CTOX.".to_owned());
                self.custom_command_input.clear();
                self.composer_transcript.clear();
                self.composer_attachments.clear();
                self.transcript_panel_open = false;
                self.terminal_focus = true;
                return;
            }
        }

        self.custom_command_input = payload;
        self.run_custom_command();
        self.composer_transcript.clear();
        self.composer_attachments.clear();
        self.transcript_panel_open = false;
    }

    fn open_business_os_proxy_for_selected(&mut self) -> Result<()> {
        let Some(installation) = self.selected_installation().cloned() else {
            anyhow::bail!("No installation selected.");
        };

        if let Some(proxy) = self.business_os_proxies.get(&installation.id) {
            open_external_url(&proxy.url)?;
            return Ok(());
        }

        if let Some(url) = business_os_url_for_installation(&installation) {
            verify_direct_business_os_endpoint(&url)?;
            open_external_url(&url)?;
            self.notice = Some("Business OS".to_owned());
            return Ok(());
        }

        let proxy = start_business_os_proxy(&installation)?;
        let url = proxy.url.clone();
        self.business_os_proxies
            .insert(installation.id.clone(), proxy);
        open_external_url(&url)?;
        self.notice = Some("Business OS".to_owned());
        Ok(())
    }

    fn open_business_os_admin_for_selected(&mut self) -> Result<()> {
        self.open_business_os_proxy_for_selected()?;
        let Some(installation) = self.selected_installation().cloned() else {
            anyhow::bail!("Keine Instanz.");
        };
        let base_url = self
            .business_os_proxies
            .get(&installation.id)
            .map(|proxy| proxy.url.clone())
            .or_else(|| business_os_url_for_installation(&installation))
            .context("Business OS")?;
        let admin_url = format!("{}/admin", base_url.trim_end_matches('/'));
        open_external_url(&admin_url)?;
        self.notice = Some("Business OS Admin".to_owned());
        Ok(())
    }

    fn spawn_tab(
        &mut self,
        installation_id: String,
        title: String,
        kind: SessionKind,
        spec: SessionSpec,
    ) {
        if kind == SessionKind::Tui {
            if let Some(existing_tab_id) = self.find_tui_tab_id(&installation_id) {
                self.active_tab_id = Some(existing_tab_id);
                self.terminal_focus = true;
                return;
            }
        }

        match TerminalSession::spawn(&spec, 36, 140) {
            Ok(terminal) => {
                let tab_id = Uuid::new_v4().to_string();
                self.tabs.push(DesktopTab {
                    id: tab_id.clone(),
                    installation_id,
                    title,
                    kind,
                    terminal,
                    last_size: (36, 140),
                });
                self.active_tab_id = Some(tab_id);
                self.terminal_focus = true;
            }
            Err(error) => self.notice = Some(error.to_string()),
        }
    }

    fn close_tab(&mut self, tab_id: &str) {
        if let Some(index) = self.tabs.iter().position(|tab| tab.id == tab_id) {
            self.tabs[index].terminal.close();
            self.tabs.remove(index);
        }
        self.active_tab_id = self
            .selected_installation_id
            .as_deref()
            .and_then(|installation_id| self.find_tui_tab_id(installation_id))
            .or_else(|| {
                self.tabs
                    .iter()
                    .rev()
                    .find(|tab| tab.kind == SessionKind::Command)
                    .map(|tab| tab.id.clone())
            });
    }

    fn find_tui_tab_id(&self, installation_id: &str) -> Option<String> {
        self.tabs
            .iter()
            .find(|tab| tab.kind == SessionKind::Tui && tab.installation_id == installation_id)
            .map(|tab| tab.id.clone())
    }

    fn select_installation_and_focus(&mut self, installation_id: String) {
        self.selected_installation_id = Some(installation_id.clone());

        self.active_tab_id = self.find_tui_tab_id(&installation_id);
        self.terminal_focus = self.active_tab_id.is_some();
    }

    fn select_installation_overview(&mut self, installation_id: String) {
        self.selected_installation_id = Some(installation_id);
        self.active_tab_id = None;
        self.terminal_focus = false;
        self.show_settings_view = false;
        self.settings_page = None;
    }

    fn toggle_installation_settings(&mut self, installation_id: String) {
        self.selected_installation_id = Some(installation_id.clone());
        if let Some(existing_tab_id) = self.find_tui_tab_id(&installation_id) {
            self.active_tab_id = Some(existing_tab_id);
            self.terminal_focus = true;
        } else {
            self.active_tab_id = None;
            self.terminal_focus = false;
        }
        if self.expanded_installation_id.as_deref() == Some(installation_id.as_str()) {
            self.expanded_installation_id = None;
        } else {
            self.expanded_installation_id = Some(installation_id);
        }
    }

    fn installation_runtime_status(&self, installation_id: &str) -> InstallationRuntimeStatus {
        self.installation_statuses
            .get(installation_id)
            .cloned()
            .or_else(|| {
                self.registry
                    .installations
                    .iter()
                    .find(|entry| entry.id == installation_id)
                    .map(default_installation_runtime_status)
            })
            .unwrap_or(InstallationRuntimeStatus {
                label: "Unknown".to_owned(),
                color: Color32::from_rgb(136, 144, 155),
            })
    }

    fn refresh_installation_statuses(&mut self) {
        if self.last_installation_status_poll.elapsed() < Duration::from_secs(2) {
            return;
        }
        self.last_installation_status_poll = Instant::now();

        let mut statuses = BTreeMap::new();
        for installation in &self.registry.installations {
            let status = if installation.is_local() {
                poll_local_installation_status(installation)
                    .unwrap_or_else(|_| default_installation_runtime_status(installation))
            } else {
                remote_installation_runtime_status(&self.tabs, installation.id.as_str())
                    .unwrap_or_else(|| default_installation_runtime_status(installation))
            };
            statuses.insert(installation.id.clone(), status);
        }
        self.installation_statuses = statuses;
    }

    fn active_tab_mut(&mut self) -> Option<&mut DesktopTab> {
        let active = self.active_tab_id.as_deref()?;
        self.tabs.iter_mut().find(|tab| tab.id == active)
    }

    fn handle_terminal_input(&mut self, ctx: &Context) {
        if !self.terminal_focus {
            return;
        }
        if self.last_frame_terminal_input.elapsed() < Duration::from_millis(12) {
            return;
        }

        let Some(tab) = self.active_tab_mut() else {
            return;
        };
        if tab.kind != SessionKind::Tui {
            return;
        }

        let events = ctx.input(|input| input.events.clone());
        let mut wrote_anything = false;
        for event in events {
            match event {
                egui::Event::Text(text) => {
                    if !text.is_empty() {
                        let _ = tab.terminal.write_input(text.as_bytes(), true);
                        wrote_anything = true;
                    }
                }
                egui::Event::Key {
                    key,
                    pressed,
                    modifiers,
                    ..
                } if pressed => {
                    if let Some(bytes) = key_event_to_bytes(key, modifiers) {
                        let _ = tab.terminal.write_input(&bytes, true);
                        wrote_anything = true;
                    }
                }
                _ => {}
            }
        }

        if wrote_anything {
            self.last_frame_terminal_input = Instant::now();
        }
    }

    fn source_root(&self) -> Option<PathBuf> {
        repo_root_from_manifest_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).as_path())
    }

    fn start_provisioning(&mut self) {
        if self.provision_running {
            self.notice = Some("A host setup is already running.".to_owned());
            return;
        }

        let Some(installation) = self.selected_installation().cloned() else {
            self.notice = Some("No installation selected.".to_owned());
            return;
        };
        if installation.mode != InstallationMode::RemoteWebRtc {
            self.notice = Some("Host setup is only available for remote installations.".to_owned());
            return;
        }

        let Some(source_root) = self.source_root() else {
            self.notice = Some("Could not resolve the CTOX source folder.".to_owned());
            return;
        };

        let (tx, rx) = mpsc::channel();
        let request = ProvisionRequest {
            source_root,
            remote: installation.remote,
        };

        self.provision_running = true;
        self.provisioning_installation_id = Some(installation.id.clone());
        self.provision_status = Some("Starting host setup...".to_owned());
        self.provision_log.clear();
        self.provision_log.push("Starting host setup...".to_owned());
        self.provision_rx = Some(rx);

        // Reset host_prepared so a re-provision actually runs the full build
        if let Some(inst) = self
            .registry
            .installations
            .iter_mut()
            .find(|entry| entry.id == installation.id)
        {
            inst.remote.host_prepared = false;
        }

        thread::spawn(move || crate::provision::run(request, tx));
    }

    fn poll_background_events(&mut self) {
        let mut finished = false;
        if let Some(rx) = self.provision_rx.as_ref() {
            while let Ok(event) = rx.try_recv() {
                match event {
                    ProvisionEvent::Status(message) => {
                        self.provision_status = Some(message.clone());
                        self.provision_log.push(message);
                    }
                    ProvisionEvent::Finished(result) => {
                        self.provision_running = false;
                        finished = true;
                        match result {
                            Ok(message) => {
                                self.provision_status = Some(message.clone());
                                self.provision_log.push(message.clone());
                                self.notice = Some("Host setup finished.".to_owned());
                                if let Some(installation_id) =
                                    self.provisioning_installation_id.clone()
                                {
                                    if let Some(installation) = self
                                        .registry
                                        .installations
                                        .iter_mut()
                                        .find(|entry| entry.id == installation_id)
                                    {
                                        installation.remote.host_prepared = true;
                                    }
                                    let _ = self.registry.save();
                                }
                            }
                            Err(error) => {
                                self.provision_status = Some(error.clone());
                                self.provision_log.push(error.clone());
                                self.notice = Some(error);
                            }
                        }
                    }
                }
            }
        }

        if finished {
            // Auto-connect to TUI after successful provisioning
            let auto_connect = self
                .provisioning_installation_id
                .as_ref()
                .and_then(|id| {
                    self.registry
                        .installations
                        .iter()
                        .find(|entry| entry.id == *id)
                })
                .is_some_and(|inst| inst.remote.host_prepared);
            if auto_connect {
                if let Some(id) = self.provisioning_installation_id.clone() {
                    self.selected_installation_id = Some(id);
                    self.provision_log.push("Verbinde via WebRTC...".to_owned());
                    self.spawn_tui_tab();
                }
            }
            self.provision_rx = None;
            self.provisioning_installation_id = None;
        }

        let should_finish = self
            .command_run
            .as_ref()
            .map(|run| {
                let snapshot = run.terminal.snapshot();
                CommandResult {
                    title: run.title.clone(),
                    exit_code: snapshot.exit_code,
                    output: snapshot.output,
                }
            })
            .filter(|result| result.exit_code.is_some());

        if let Some(result) = should_finish {
            if let Some(run) = self.command_run.take() {
                run.terminal.close();
            }
            self.last_command_result = Some(result);
        }
    }

    fn render_left_panel(&mut self, ctx: &Context) {
        SidePanel::left("installations")
            .resizable(true)
            .default_width(LEFT_PANEL_WIDTH)
            .frame(
                Frame::default()
                    .fill(UI_RAIL)
                    .stroke(Stroke::new(1.0, Color32::from_rgb(145, 145, 141)))
                    .inner_margin(egui::Margin::same(0)),
            )
            .width_range(310.0..=430.0)
            .show(ctx, |ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), 58.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.add_space(20.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new("CTOX").size(20.0).strong().color(UI_TEXT));
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(format!("Desktop {}", env!("CARGO_PKG_VERSION")))
                                        .size(12.0)
                                        .color(UI_MUTED),
                                );
                                if self
                                    .latest_release
                                    .as_ref()
                                    .map(|latest| latest.tag_name != env!("CARGO_PKG_VERSION"))
                                    .unwrap_or(false)
                                {
                                    if ui
                                        .add_sized([78.0, 28.0], Button::new("Upgrade"))
                                        .clicked()
                                    {
                                        self.notice = Some(
                                            "Desktop-App Upgrade wird im naechsten Schritt mit dem Release-Installer verbunden."
                                                .to_owned(),
                                        );
                                    }
                                }
                            });
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_space(16.0);
                            if ui
                                .add(
                                    Button::new(RichText::new("+").size(18.0))
                                        .min_size(egui::vec2(34.0, 34.0))
                                        .corner_radius(3.0),
                                )
                                .clicked()
                            {
                                self.show_add_menu = !self.show_add_menu;
                                self.show_settings_view = false;
                                self.settings_page = None;
                                self.remove_candidate_id = None;
                                self.active_tab_id = None;
                                self.notice = None;
                            }
                        });
                    },
                );
                ui.separator();
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("INSTANZEN")
                                .size(12.0)
                                .color(UI_MUTED)
                                .extra_letter_spacing(1.2),
                        );
                    });
                }
                );

                if self.show_add_menu {
                    self.render_add_instance_panel(ui);
                    ui.add_space(10.0);
                }

                if self.remove_candidate_id.is_some() {
                    self.render_remove_instance_panel(ui);
                    ui.add_space(10.0);
                }

                ui.add_space(4.0);
                if self.registry.installations.is_empty() {
                    ui.label("No CTOX installation added yet.");
                    return;
                }

                ScrollArea::vertical().show(ui, |ui| {
                    let cards: Vec<InstallationCardData> = self
                        .registry
                        .installations
                        .iter()
                        .map(|installation| InstallationCardData {
                            installation_id: installation.id.clone(),
                            title: self.display_name_for(installation),
                            subtitle: installation.display_path(),
                            is_selected: self.selected_installation_id.as_deref()
                                == Some(installation.id.as_str()),
                            runtime_status: self.installation_runtime_status(&installation.id),
                            version_label: installation.cached_version.clone(),
                            update_available: self.installation_update_available(installation),
                            role_label: self.role_for(installation),
                            method_label: Self::method_label_for(installation).to_owned(),
                            admin_unlocked: self
                                .admin_unlocked_installations
                                .contains(&installation.id)
                                || self
                                    .registry
                                    .desktop
                                    .get(&installation.id)
                                    .map(|state| state.admin_unlocked)
                                    .unwrap_or(false),
                        })
                        .collect();

                    let mut clicked_installation = None;
                    let mut open_settings_for = None;
                    let mut open_tui_for = None;
                    for card in cards.iter().cloned() {
                        let InstallationCardData {
                            installation_id,
                            title,
                            subtitle,
                            is_selected,
                            runtime_status,
                            version_label,
                            update_available,
                            role_label,
                            method_label,
                            admin_unlocked,
                        } = card;
                        let fill = if is_selected { UI_BLUE } else { Color32::TRANSPARENT };
                        let primary = if is_selected { UI_PAPER } else { UI_TEXT };
                        let muted = if is_selected {
                            Color32::from_rgb(238, 248, 255)
                        } else {
                            UI_MUTED
                        };
                        Frame::default()
                            .fill(fill)
                            .stroke(Stroke::NONE)
                            .corner_radius(4.0)
                            .shadow(egui::epaint::Shadow::NONE)
                            .inner_margin(egui::Margin::symmetric(10, 6))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    draw_instance_logo(ui, 42.0);
                                    ui.add_space(10.0);
                                    ui.vertical(|ui| {
                                        let title_response = ui.add(
                                            egui::Label::new(
                                                RichText::new(title)
                                                    .size(15.5)
                                                    .strong()
                                                    .color(primary),
                                            )
                                            .sense(Sense::click()),
                                        );
                                        if title_response.clicked() {
                                            clicked_installation = Some(installation_id.clone());
                                        }
                                        let subtitle_response = ui.add(
                                            egui::Label::new(
                                                RichText::new(subtitle).size(12.5).color(muted),
                                            )
                                            .sense(Sense::click()),
                                        );
                                        if subtitle_response.clicked() {
                                            clicked_installation = Some(installation_id.clone());
                                        }
                                        ui.add_space(4.0);
                                        ui.horizontal(|ui| {
                                            draw_status_dot(ui, runtime_status.color);
                                            ui.label(
                                                RichText::new(method_label.clone())
                                                    .size(13.0)
                                                    .color(muted),
                                            );
                                            ui.label(
                                                RichText::new("·")
                                                    .size(13.0)
                                                    .color(muted),
                                            );
                                            ui.label(
                                                RichText::new(role_label.clone())
                                                    .size(13.0)
                                                    .color(if is_selected {
                                                        UI_PAPER
                                                    } else {
                                                        role_color(&role_label)
                                                    }),
                                            );
                                            let _ = version_label.as_deref();
                                            let _ = update_available;
                                        });
                                    });
                                    ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                                        if ui
                                            .add(
                                                Button::new(
                                                    RichText::new("⚙")
                                                        .text_style(egui::TextStyle::Small)
                                                        .color(primary),
                                                )
                                                .frame(false)
                                                .min_size(egui::vec2(28.0, 24.0)),
                                            )
                                            .clicked()
                                        {
                                            open_settings_for = Some(installation_id.clone());
                                        }
                                        if admin_unlocked
                                            && ui
                                                .add(
                                                    Button::new(
                                                    RichText::new("▣")
                                                        .text_style(egui::TextStyle::Small)
                                                            .color(primary),
                                                    )
                                                    .frame(false)
                                                    .min_size(egui::vec2(28.0, 24.0)),
                                                )
                                                .clicked()
                                        {
                                            open_tui_for = Some(installation_id.clone());
                                        }
                                    });
                                });
                            });
                        ui.add_space(4.0);
                    }

                    if let Some(installation_id) = open_settings_for {
                        self.select_installation_overview(installation_id);
                        self.show_settings_view = true;
                        self.settings_page = None;
                        self.show_add_menu = false;
                        self.remove_candidate_id = None;
                    }
                    if let Some(installation_id) = open_tui_for {
                        self.select_installation_overview(installation_id.clone());
                        self.focus_or_open_tui(&installation_id);
                    }
                    if let Some(installation_id) = clicked_installation {
                        self.select_installation_overview(installation_id);
                        self.show_settings_view = false;
                        self.settings_page = None;
                        self.notice = None;
                    }
                    if self.upgrade_running || !self.upgrade_log.is_empty() {
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new(if self.upgrade_running {
                                "upgrade running…"
                            } else {
                                "upgrade log"
                            })
                            .size(12.0)
                            .color(Color32::from_rgb(220, 180, 90)),
                        );
                        ScrollArea::vertical()
                            .max_height(120.0)
                            .stick_to_bottom(true)
                            .show(ui, |ui| {
                                for line in self
                                    .upgrade_log
                                    .iter()
                                    .rev()
                                    .take(40)
                                    .collect::<Vec<_>>()
                                    .into_iter()
                                    .rev()
                                {
                                    ui.label(
                                        RichText::new(line)
                                            .size(11.5)
                                            .color(Color32::from_gray(180)),
                                    );
                                }
                            });
                    }
                });
            });
    }

    fn render_add_instance_panel(&mut self, ui: &mut Ui) {
        let mut close = false;
        let mut save_connect = false;
        let mut save_new = false;
        let mut back = false;
        Frame::default()
            .fill(UI_PAPER)
            .stroke(Stroke::new(1.0, UI_LINE))
            .corner_radius(3.0)
            .inner_margin(egui::Margin::symmetric(12, 12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if self.add_flow != AddInstanceFlow::Choice
                        && ui.add_sized([74.0, 28.0], Button::new("Zurück")).clicked()
                    {
                        back = true;
                    }
                    ui.label(
                        RichText::new(match self.add_flow {
                            AddInstanceFlow::Choice => "Instanz hinzufügen",
                            AddInstanceFlow::Connect => "Connect CTOX",
                            AddInstanceFlow::New => "New CTOX",
                        })
                        .size(15.0)
                        .strong()
                        .color(UI_TEXT),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add_sized([86.0, 28.0], Button::new("Schließen"))
                            .clicked()
                        {
                            close = true;
                        }
                    });
                });
                ui.add_space(10.0);
                match self.add_flow {
                    AddInstanceFlow::Choice => {
                        let button_width = (ui.available_width() - 8.0) / 2.0;
                        ui.horizontal(|ui| {
                            if ui
                                .add_sized(
                                    [button_width, 36.0],
                                    Button::new(RichText::new("Connect CTOX").color(UI_TEXT)),
                                )
                                .clicked()
                            {
                                self.add_flow = AddInstanceFlow::Connect;
                                self.notice = None;
                            }
                            if ui
                                .add_sized(
                                    [button_width, 36.0],
                                    Button::new(RichText::new("New CTOX").color(UI_TEXT)),
                                )
                                .clicked()
                            {
                                self.add_flow = AddInstanceFlow::New;
                                self.notice = None;
                            }
                        });
                    }
                    AddInstanceFlow::Connect => {
                        let button_width = (ui.available_width() - 8.0) / 2.0;
                        ui.horizontal(|ui| {
                            if ui
                                .add_sized(
                                    [button_width, 32.0],
                                    Button::new(RichText::new("Local").color(if self.connect_draft.mode == ConnectMode::Local {
                                        UI_PAPER
                                    } else {
                                        UI_TEXT
                                    }))
                                    .fill(if self.connect_draft.mode == ConnectMode::Local {
                                        UI_BLUE
                                    } else {
                                        Color32::TRANSPARENT
                                    }),
                                )
                                .clicked()
                            {
                                self.connect_draft.mode = ConnectMode::Local;
                                self.notice = None;
                            }
                            if ui
                                .add_sized(
                                    [button_width, 32.0],
                                    Button::new(RichText::new("SSH").color(if self.connect_draft.mode == ConnectMode::Ssh {
                                        UI_PAPER
                                    } else {
                                        UI_TEXT
                                    }))
                                    .fill(if self.connect_draft.mode == ConnectMode::Ssh {
                                        UI_BLUE
                                    } else {
                                        Color32::TRANSPARENT
                                    }),
                                )
                                .clicked()
                            {
                                self.connect_draft.mode = ConnectMode::Ssh;
                                self.notice = None;
                            }
                        });
                        ui.horizontal(|ui| {
                            if ui
                                .add_sized(
                                    [button_width, 32.0],
                                    Button::new(RichText::new("URL/IP").color(if self.connect_draft.mode == ConnectMode::Direct {
                                        UI_PAPER
                                    } else {
                                        UI_TEXT
                                    }))
                                    .fill(if self.connect_draft.mode == ConnectMode::Direct {
                                        UI_BLUE
                                    } else {
                                        Color32::TRANSPARENT
                                    }),
                                )
                                .clicked()
                            {
                                self.connect_draft.mode = ConnectMode::Direct;
                                self.notice = None;
                            }
                            if ui
                                .add_sized(
                                    [button_width, 32.0],
                                    Button::new(RichText::new("Peer2Peer").color(if self.connect_draft.mode == ConnectMode::Signaling {
                                        UI_PAPER
                                    } else {
                                        UI_TEXT
                                    }))
                                    .fill(if self.connect_draft.mode == ConnectMode::Signaling {
                                        UI_BLUE
                                    } else {
                                        Color32::TRANSPARENT
                                    }),
                                )
                                .clicked()
                            {
                                self.connect_draft.mode = ConnectMode::Signaling;
                                self.notice = None;
                            }
                        });
                        ui.add_space(8.0);
                        match self.connect_draft.mode {
                            ConnectMode::Local => {
                                ui.label(
                                    RichText::new("Lokale Standardinstallation verbinden.")
                                        .size(13.0)
                                        .color(UI_TEXT),
                                );
                                ui.label(
                                    RichText::new(
                                        "/Users/michaelwelsch/.local/lib/ctox/current",
                                    )
                                    .size(12.0)
                                    .color(UI_MUTED),
                                );
                                ui.add_space(8.0);
                                ui.collapsing("Advanced", |ui| {
                                    add_form_input(
                                        ui,
                                        "Lokaler CTOX Pfad (optional)",
                                        "leer lassen fuer Standardpfad",
                                        &mut self.connect_draft.install_root,
                                        false,
                                    );
                                    add_form_input(
                                        ui,
                                        "Name",
                                        "optional",
                                        &mut self.connect_draft.name,
                                        false,
                                    );
                                });
                            }
                            ConnectMode::Direct => {
                                add_form_input(
                                    ui,
                                    "Name",
                                    "optional",
                                    &mut self.connect_draft.name,
                                    false,
                                );
                                add_form_input(
                                    ui,
                                    "CTOX URL/IP",
                                    "",
                                    &mut self.connect_draft.endpoint,
                                    false,
                                );
                                add_form_input(ui, "User", "", &mut self.connect_draft.user, false);
                                add_form_input(
                                    ui,
                                    "Password",
                                    "",
                                    &mut self.connect_draft.user_password,
                                    true,
                                );
                            }
                            ConnectMode::Signaling => {
                                add_form_input(
                                    ui,
                                    "Name",
                                    "optional",
                                    &mut self.connect_draft.name,
                                    false,
                                );
                                add_form_input(
                                    ui,
                                    "Peer2Peer Server URL",
                                    "",
                                    &mut self.connect_draft.endpoint,
                                    false,
                                );
                                add_form_input(ui, "Room", "", &mut self.connect_draft.room, false);
                                add_form_input(
                                    ui,
                                    "Room Password",
                                    "",
                                    &mut self.connect_draft.room_password,
                                    true,
                                );
                                add_form_input(ui, "User", "", &mut self.connect_draft.user, false);
                                add_form_input(
                                    ui,
                                    "Password",
                                    "",
                                    &mut self.connect_draft.user_password,
                                    true,
                                );
                            }
                            ConnectMode::Ssh => {
                                add_form_input(
                                    ui,
                                    "Name",
                                    "optional",
                                    &mut self.connect_draft.name,
                                    false,
                                );
                                add_form_input(
                                    ui,
                                    "IP / Host",
                                    "",
                                    &mut self.connect_draft.ssh_host,
                                    false,
                                );
                                add_form_input(
                                    ui,
                                    "SSH User",
                                    "",
                                    &mut self.connect_draft.ssh_user,
                                    false,
                                );
                                add_form_input(
                                    ui,
                                    "SSH/Sudo Password",
                                    "",
                                    &mut self.connect_draft.ssh_password,
                                    true,
                                );
                                ui.collapsing("Advanced", |ui| {
                                    ui.label(
                                        RichText::new(
                                            "Optional. Leer lassen, damit CTOX per SSH automatisch gesucht wird.",
                                        )
                                        .size(12.0)
                                        .color(UI_MUTED),
                                    );
                                    add_form_input(
                                        ui,
                                        "Remote CTOX Pfad (optional)",
                                        "z.B. ~/.local/lib/ctox/current",
                                        &mut self.connect_draft.install_root,
                                        false,
                                    );
                                });
                            }
                        }
                        if self.connect_draft.mode != ConnectMode::Local
                            && self.connect_draft.mode != ConnectMode::Ssh
                        {
                            ui.checkbox(&mut self.connect_draft.remember_login, "Login merken");
                        }
                        ui.add_space(8.0);
                        if ui
                            .add_sized(
                                [ui.available_width(), 30.0],
                                Button::new(if self.connect_draft.mode == ConnectMode::Ssh {
                                    "SSH prüfen und verbinden"
                                } else if self.connect_draft.mode == ConnectMode::Local {
                                    "Lokale Instanz verbinden"
                                } else {
                                    "Verbinden"
                                }),
                            )
                            .clicked()
                        {
                            save_connect = true;
                        }
                    }
                    AddInstanceFlow::New => {
                        let button_width = (ui.available_width() - 8.0) / 2.0;
                        ui.horizontal(|ui| {
                            if ui
                                .add_sized(
                                    [button_width, 32.0],
                                    Button::new(RichText::new("Local").color(if self.new_draft.local {
                                        UI_PAPER
                                    } else {
                                        UI_TEXT
                                    }))
                                    .fill(if self.new_draft.local {
                                        UI_BLUE
                                    } else {
                                        Color32::TRANSPARENT
                                    }),
                                )
                                .clicked()
                            {
                                self.new_draft.local = true;
                                self.notice = None;
                            }
                            if ui
                                .add_sized(
                                    [button_width, 32.0],
                                    Button::new(RichText::new("Remote").color(if !self.new_draft.local {
                                        UI_PAPER
                                    } else {
                                        UI_TEXT
                                    }))
                                    .fill(if !self.new_draft.local {
                                        UI_BLUE
                                    } else {
                                        Color32::TRANSPARENT
                                    }),
                                )
                                .clicked()
                            {
                                self.new_draft.local = false;
                                self.notice = None;
                            }
                        });
                        ui.add_space(8.0);
                        add_form_input(ui, "Name", "optional", &mut self.new_draft.name, false);
                        if self.new_draft.local {
                            ui.label(
                                RichText::new("CTOX auf diesem Rechner installieren.")
                                    .size(13.0)
                                    .color(UI_TEXT),
                            );
                            add_form_input(
                                ui,
                                "Admin/Sudo Password",
                                "",
                                &mut self.new_draft.ssh_password,
                                true,
                            );
                            ui.collapsing("Advanced", |ui| {
                                add_form_input(
                                    ui,
                                    "Lokaler Installationsordner",
                                    "leer lassen fuer Standardpfad",
                                    &mut self.new_draft.install_root,
                                    false,
                                );
                            });
                        } else {
                            add_form_input(ui, "IP / Host", "", &mut self.new_draft.host, false);
                            add_form_input(ui, "SSH User", "", &mut self.new_draft.ssh_user, false);
                            add_form_input(
                                ui,
                                "SSH/Sudo Password",
                                "",
                                &mut self.new_draft.ssh_password,
                                true,
                            );
                            ui.collapsing("Advanced", |ui| {
                                ui.label(
                                    RichText::new(
                                        "Optionaler Zielordner auf dem Remote-Rechner, kein lokaler Mac-Pfad.",
                                    )
                                    .size(12.0)
                                    .color(UI_MUTED),
                                );
                                add_form_input(
                                    ui,
                                    "Remote Installationsordner",
                                    "leer lassen fuer CTOX Standard",
                                    &mut self.new_draft.install_root,
                                    false,
                                );
                            });
                        }
                        ui.add_space(8.0);
                        if ui
                            .add_sized(
                                [ui.available_width(), 30.0],
                                Button::new(if self.new_draft.local {
                                    "Lokale Instanz anlegen"
                                } else {
                                    "Remote Installation anlegen"
                                }),
                            )
                            .clicked()
                        {
                            save_new = true;
                        }
                    }
                }
            });
        if close {
            self.show_add_menu = false;
            self.add_flow = AddInstanceFlow::Choice;
        }
        if back {
            self.add_flow = AddInstanceFlow::Choice;
        }
        if save_connect {
            self.create_connect_instance_from_draft();
        }
        if save_new {
            self.create_new_instance_from_draft();
        }
    }

    fn render_remove_instance_panel(&mut self, ui: &mut Ui) {
        let Some(installation_id) = self.remove_candidate_id.clone() else {
            return;
        };
        let Some(installation) = self
            .registry
            .installations
            .iter()
            .find(|entry| entry.id == installation_id)
            .cloned()
        else {
            self.remove_candidate_id = None;
            self.remove_delete_confirm = false;
            return;
        };

        let admin_unlocked = self.admin_unlocked_installations.contains(&installation_id);
        let mut close = false;
        let mut remove_connection = false;
        let mut request_delete = false;
        let mut confirm_delete = false;

        Frame::default()
            .fill(UI_PAPER)
            .stroke(Stroke::new(1.0, UI_LINE))
            .corner_radius(3.0)
            .inner_margin(egui::Margin::symmetric(12, 12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Instanz entfernen")
                            .size(15.0)
                            .strong()
                            .color(UI_TEXT),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Abbrechen").clicked() {
                            close = true;
                        }
                    });
                });
                ui.add_space(6.0);
                ui.label(
                    RichText::new(installation.display_name())
                        .size(13.0)
                        .color(UI_TEXT),
                );
                ui.label(
                    RichText::new(installation.display_path())
                        .size(11.8)
                        .color(UI_MUTED),
                );
                ui.add_space(10.0);

                if ui
                    .add_sized(
                        [ui.available_width(), 30.0],
                        Button::new("Nur Verbindung entfernen"),
                    )
                    .clicked()
                {
                    remove_connection = true;
                }
                ui.label(
                    RichText::new("Löscht nur diesen Eintrag in der Desktop-App.")
                        .size(11.5)
                        .color(UI_MUTED),
                );

                ui.add_space(10.0);
                if admin_unlocked {
                    if !self.remove_delete_confirm {
                        if ui
                            .add_sized(
                                [ui.available_width(), 30.0],
                                Button::new("Installation löschen..."),
                            )
                            .clicked()
                        {
                            request_delete = true;
                        }
                        ui.label(
                            RichText::new("Entfernt zusätzlich den CTOX Installationsordner.")
                                .size(11.5)
                                .color(UI_AMBER),
                        );
                    } else {
                        ui.label(
                            RichText::new(
                                "Diese Aktion löscht die CTOX Installation auf dem Zielsystem.",
                            )
                            .size(11.7)
                            .color(UI_AMBER),
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Endgültig löschen").clicked() {
                                confirm_delete = true;
                            }
                            if ui.button("Zurück").clicked() {
                                self.remove_delete_confirm = false;
                            }
                        });
                    }
                } else {
                    ui.label(
                        RichText::new(
                            "Zum Löschen der Installation erst Admin für diese Instanz entsperren.",
                        )
                        .size(11.8)
                        .color(UI_MUTED),
                    );
                }
            });

        if close {
            self.remove_candidate_id = None;
            self.remove_delete_confirm = false;
        }
        if request_delete {
            self.remove_delete_confirm = true;
        }
        if remove_connection {
            self.remove_installation_connection(&installation_id);
        }
        if confirm_delete {
            self.delete_entire_installation(&installation_id);
        }
    }

    fn render_admin_instance_panel(&mut self, ui: &mut Ui) {
        let Some(installation_id) = self.admin_candidate_id.clone() else {
            return;
        };
        let mut close = false;

        Frame::default()
            .fill(Color32::from_rgb(21, 24, 28))
            .stroke(Stroke::new(1.0, Color32::from_rgb(43, 49, 57)))
            .corner_radius(8.0)
            .inner_margin(egui::Margin::symmetric(12, 12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Admin").size(15.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Schließen").clicked() {
                            close = true;
                        }
                    });
                });
            });

        if close {
            self.admin_candidate_id = None;
            return;
        }
        self.render_installation_settings_inline(ui, &installation_id);
    }

    fn render_right_panel(&mut self, ctx: &Context) {
        if !self.show_right_panel {
            return;
        }
        SidePanel::right("command_catalog")
            .resizable(true)
            .default_width(RIGHT_PANEL_WIDTH)
            .width_range(240.0..=520.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("CLI").size(18.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(
                                Button::new(RichText::new("⟩").size(16.0))
                                    .frame(false)
                                    .min_size(egui::vec2(24.0, 24.0)),
                            )
                            .clicked()
                        {
                            self.show_right_panel = false;
                        }
                    });
                });
                ui.add_space(8.0);

                ScrollArea::vertical().show(ui, |ui| {
                    if let Some(run) = &self.command_run {
                        let snapshot = run.terminal.snapshot();
                        render_command_feedback_box(
                            ui,
                            "Command running",
                            &run.title,
                            Color32::from_rgb(86, 134, 173),
                            &tail_text(&snapshot.output, 10),
                        );
                        ui.add_space(10.0);
                    } else if let Some(result) = &self.last_command_result {
                        let (label, color) = if result.exit_code.unwrap_or(1) == 0 {
                            ("Last command", Color32::from_rgb(94, 164, 116))
                        } else {
                            ("Last command failed", Color32::from_rgb(198, 98, 98))
                        };
                        render_command_feedback_box(
                            ui,
                            label,
                            &result.title,
                            color,
                            &tail_text(&result.output, 10),
                        );
                        ui.add_space(10.0);
                    }

                    let mut run_command = None;
                    for group in CommandGroup::ALL {
                        ui.collapsing(group.label(), |ui| {
                            for command in COMMANDS
                                .iter()
                                .filter(|entry| entry.group == group)
                                .filter(|entry| entry.args != ["tui"])
                            {
                                ui.label(RichText::new(command.title).size(14.2));
                                ui.label(
                                    RichText::new(command.description)
                                        .size(12.5)
                                        .color(Color32::from_gray(135)),
                                );
                                ui.monospace(command.example);
                                if let Some(hint) = command.extra_args_hint {
                                    let extra_args = self
                                        .command_extra_args
                                        .entry(command.example.to_owned())
                                        .or_default();
                                    ui.add(TextEdit::singleline(extra_args).hint_text(hint));
                                }
                                if ui
                                    .add_enabled(command.runnable, Button::new("Run"))
                                    .clicked()
                                {
                                    run_command = Some(*command);
                                }
                                ui.add_space(10.0);
                            }
                        });
                        ui.add_space(2.0);
                    }

                    if let Some(command) = run_command {
                        self.run_command(&command);
                    }

                    ui.separator();
                });
            });
    }

    fn render_installation_settings_inline(&mut self, ui: &mut Ui, installation_id: &str) {
        let installation_id = installation_id.to_owned();
        let provision_running = self.provision_running;
        let provision_status = self.provision_status.clone();
        let update_available = self
            .registry
            .installations
            .iter()
            .find(|entry| entry.id == installation_id)
            .map(|installation| self.installation_update_available(installation))
            .unwrap_or(false);
        let admin_unlocked = self.admin_unlocked_installations.contains(&installation_id);
        let mut admin_input = self
            .admin_unlock_inputs
            .get(&installation_id)
            .cloned()
            .unwrap_or_default();
        let mut admin_input_changed = false;
        let mut admin_unlock_attempt: Option<String> = None;
        let mut persist_registry = false;
        let mut desktop_state = self.desktop_state_for(&installation_id);
        if admin_unlocked {
            desktop_state.admin_unlocked = true;
            desktop_state.role = "Admin".to_owned();
        }
        let mut persist_desktop_state = false;
        let mut open_tui = false;
        let mut start_upgrade = false;
        let mut start_provision = false;
        let mut lock_admin = false;
        let mut remove_connection = false;
        let mut request_delete_installation = false;
        let mut cancel_delete_installation = false;
        let mut delete_installation = false;

        ui.add_space(8.0);
        Frame::default()
            .fill(Color32::from_rgb(31, 34, 39))
            .stroke(Stroke::NONE)
            .corner_radius(14.0)
            .inner_margin(14.0)
            .show(ui, |ui| {
                if let Some(installation) = self
                    .registry
                    .installations
                    .iter_mut()
                    .find(|entry| entry.id == installation_id)
                {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Settings").size(13.0).color(Color32::from_gray(155)));
                    });
                    ui.add_space(12.0);

                    render_settings_section_label(ui, "Darstellung");
                    if admin_unlocked {
                        let mut display_name = desktop_state
                            .display_name
                            .clone()
                            .unwrap_or_else(|| installation.display_name());
                        if ui
                            .add_sized(
                                [ui.available_width(), 28.0],
                                TextEdit::singleline(&mut display_name).hint_text("Name"),
                            )
                            .changed()
                        {
                            installation.name = display_name.clone();
                            desktop_state.display_name =
                                non_empty_opt(&display_name).map(str::to_owned);
                            persist_registry = true;
                            persist_desktop_state = true;
                        }
                        ui.label(RichText::new("Logo").color(Color32::from_gray(135)));
                        let mut logo_path = desktop_state
                            .logo_path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_default();
                        if ui
                            .add_sized(
                                [ui.available_width(), 28.0],
                                TextEdit::singleline(&mut logo_path).hint_text("Pfad zum Logo"),
                            )
                            .changed()
                        {
                            desktop_state.logo_path =
                                non_empty_opt(&logo_path).map(|value| PathBuf::from(shellexpand_tilde(value)));
                            persist_desktop_state = true;
                        }
                    } else {
                        let display_name = desktop_state
                            .display_name
                            .as_deref()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(str::to_owned)
                            .unwrap_or_else(|| installation.display_name());
                        ui.label(RichText::new(display_name).size(13.0));
                        render_settings_hint(ui, "Name und Logo sind nur mit Admin-Rechten editierbar.");
                    }
                    ui.add_space(12.0);

                    render_settings_section_label(ui, "Adresse");
                    match installation.mode {
                        InstallationMode::Local => {
                            ui.label(RichText::new("Path").color(Color32::from_gray(135)));
                            ui.monospace(installation.display_path());
                            ui.add_space(10.0);
                            ui.label(
                                RichText::new("Admin password").color(Color32::from_gray(135)),
                            );
                            if ui
                                .add(
                                    TextEdit::singleline(&mut installation.remote.password)
                                        .password(true)
                                        .hint_text("Required for admin actions"),
                                )
                                .changed()
                            {
                                persist_registry = true;
                            }
                            ui.add_space(10.0);
                            if ui.button("Open Admin TUI").clicked() {
                                open_tui = true;
                            }
                        }
                        InstallationMode::RemoteWebRtc => {
                            let mut remote_changed = false;
                            let instance_source = match installation.remote.instance_source {
                                RemoteInstanceSource::Unspecified => {
                                    installation.remote.instance_source =
                                        RemoteInstanceSource::AttachExisting;
                                    persist_registry = true;
                                    RemoteInstanceSource::AttachExisting
                                }
                                source => source,
                            };

                            ui.label(
                                RichText::new(match instance_source {
                                    RemoteInstanceSource::AttachExisting => "Existing remote CTOX",
                                    RemoteInstanceSource::InstallNew => "Provisioned remote CTOX",
                                    RemoteInstanceSource::Unspecified => "Remote CTOX",
                                })
                                .size(12.8)
                                .color(Color32::from_gray(155)),
                            );
                            ui.add_space(10.0);

                            match instance_source {
                                RemoteInstanceSource::AttachExisting => {
                                    ui.label(RichText::new("Managed in CTOX").color(Color32::from_gray(135)));
                                    ui.label(
                                        RichText::new("Peer2Peer server, room, and password come from CTOX Settings / Communication.")
                                            .size(12.4)
                                            .color(Color32::from_gray(160)),
                                    );
                                    ui.add_space(8.0);
                                    ui.horizontal_wrapped(|ui| {
                                        let local_selected =
                                            installation.remote.host_target == RemoteHostTarget::Localhost;
                                        if ui.selectable_label(local_selected, "This machine").clicked() {
                                            installation.remote.host_target = RemoteHostTarget::Localhost;
                                            persist_registry = true;
                                        }
                                        let ssh_selected =
                                            installation.remote.host_target == RemoteHostTarget::Ssh;
                                        if ui.selectable_label(ssh_selected, "SSH").clicked() {
                                            installation.remote.host_target = RemoteHostTarget::Ssh;
                                            persist_registry = true;
                                        }
                                    });
                                    ui.add_space(8.0);
                                    if installation.remote.host_target == RemoteHostTarget::Ssh {
                                        ui.label(RichText::new("IP or host").color(Color32::from_gray(135)));
                                        if ui
                                            .add(TextEdit::singleline(&mut installation.remote.ssh_host).hint_text("192.168.1.22"))
                                            .changed()
                                        {
                                            persist_registry = true;
                                        }
                                        ui.label(RichText::new("User").color(Color32::from_gray(135)));
                                        if ui
                                            .add(TextEdit::singleline(&mut installation.remote.ssh_user).hint_text("metricspace"))
                                            .changed()
                                        {
                                            persist_registry = true;
                                        }
                                        ui.label(RichText::new("SSH password").color(Color32::from_gray(135)));
                                        if ui
                                            .add(
                                                TextEdit::singleline(&mut installation.remote.ssh_password)
                                                    .password(true)
                                                    .hint_text("SSH password"),
                                            )
                                            .changed()
                                        {
                                            persist_registry = true;
                                        }
                                    }
                                }
                                RemoteInstanceSource::InstallNew => {
                                    ui.label(
                                        RichText::new("Install source")
                                            .size(12.4)
                                            .color(Color32::from_gray(135)),
                                    );
                                    ui.horizontal_wrapped(|ui| {
                                        let stable_selected = installation.remote.install_channel
                                            == InstallChannel::Stable;
                                        if ui
                                            .selectable_label(
                                                stable_selected,
                                                "Stable (latest release, binary)",
                                            )
                                            .clicked()
                                        {
                                            installation.remote.install_channel =
                                                InstallChannel::Stable;
                                            persist_registry = true;
                                        }
                                        let dev_selected = installation.remote.install_channel
                                            == InstallChannel::Dev;
                                        if ui
                                            .selectable_label(dev_selected, "Dev (main, source)")
                                            .clicked()
                                        {
                                            installation.remote.install_channel =
                                                InstallChannel::Dev;
                                            persist_registry = true;
                                        }
                                        let local_checkout_selected =
                                            installation.remote.install_channel
                                                == InstallChannel::LocalCheckout;
                                        if ui
                                            .selectable_label(
                                                local_checkout_selected,
                                                "Local checkout (advanced)",
                                            )
                                            .clicked()
                                        {
                                            installation.remote.install_channel =
                                                InstallChannel::LocalCheckout;
                                            persist_registry = true;
                                        }
                                    });
                                    ui.add_space(8.0);
                                    ui.horizontal_wrapped(|ui| {
                                        let local_selected =
                                            installation.remote.host_target == RemoteHostTarget::Localhost;
                                        if ui.selectable_label(local_selected, "This machine").clicked() {
                                            installation.remote.host_target = RemoteHostTarget::Localhost;
                                            persist_registry = true;
                                        }
                                        let ssh_selected =
                                            installation.remote.host_target == RemoteHostTarget::Ssh;
                                        if ui.selectable_label(ssh_selected, "SSH").clicked() {
                                            installation.remote.host_target = RemoteHostTarget::Ssh;
                                            persist_registry = true;
                                        }
                                    });
                                    ui.add_space(8.0);
                                    if installation.remote.host_target == RemoteHostTarget::Ssh {
                                        ui.label(RichText::new("IP or host").color(Color32::from_gray(135)));
                                        if ui
                                            .add(TextEdit::singleline(&mut installation.remote.ssh_host).hint_text("192.168.1.22"))
                                            .changed()
                                        {
                                            remote_changed = true;
                                            persist_registry = true;
                                        }
                                        ui.label(RichText::new("User").color(Color32::from_gray(135)));
                                        if ui
                                            .add(TextEdit::singleline(&mut installation.remote.ssh_user).hint_text("metricspace"))
                                            .changed()
                                        {
                                            remote_changed = true;
                                            persist_registry = true;
                                        }
                                        ui.label(RichText::new("SSH password").color(Color32::from_gray(135)));
                                        if ui
                                            .add(
                                                TextEdit::singleline(&mut installation.remote.ssh_password)
                                                    .password(true)
                                                    .hint_text("SSH password"),
                                            )
                                            .changed()
                                        {
                                            remote_changed = true;
                                            persist_registry = true;
                                        }
                                    }
                                    ui.label(RichText::new("Room").color(Color32::from_gray(135)));
                                    if ui
                                        .add(TextEdit::singleline(&mut installation.remote.room_id).hint_text("Room"))
                                        .changed()
                                    {
                                        remote_changed = true;
                                        persist_registry = true;
                                    }
                                    ui.label(RichText::new("Admin / WebRTC password").color(Color32::from_gray(135)));
                                    if ui
                                        .add(
                                            TextEdit::singleline(&mut installation.remote.password)
                                                .password(true)
                                                .hint_text("Password"),
                                        )
                                        .changed()
                                    {
                                        remote_changed = true;
                                        persist_registry = true;
                                    }
                                    ui.add_space(4.0);
                                    if ui
                                        .add_enabled(!provision_running, Button::new("Prepare host"))
                                        .clicked()
                                    {
                                        start_provision = true;
                                    }
                                }
                                RemoteInstanceSource::Unspecified => {}
                            }

                            if remote_changed {
                                installation.remote.host_prepared = false;
                            }

                            ui.add_space(8.0);
                            ui.collapsing("Advanced", |ui| {
                                ui.label(RichText::new("Client name").color(Color32::from_gray(135)));
                                if ui
                                    .add(
                                        TextEdit::singleline(&mut installation.remote.client_name)
                                            .hint_text("Optional"),
                                    )
                                .changed()
                                {
                                    persist_registry = true;
                                }
                                if installation.remote.host_target == RemoteHostTarget::Ssh {
                                    ui.label(RichText::new("Port").color(Color32::from_gray(135)));
                                    let mut ssh_port = installation.remote.ssh_port.to_string();
                                    if ui.add(TextEdit::singleline(&mut ssh_port).hint_text("22")).changed() {
                                        if let Ok(port) = ssh_port.parse::<u16>() {
                                            installation.remote.ssh_port = port;
                                            persist_registry = true;
                                            if installation.remote.instance_source == RemoteInstanceSource::InstallNew {
                                                installation.remote.host_prepared = false;
                                            }
                                        }
                                    }
                                }
                                if installation.mode == InstallationMode::RemoteWebRtc {
                                    ui.label(RichText::new("CTOX root").color(Color32::from_gray(135)));
                                    if ui
                                        .add(TextEdit::singleline(&mut installation.remote.install_root).hint_text("~/ctox"))
                                        .changed()
                                    {
                                        persist_registry = true;
                                        if installation.remote.instance_source == RemoteInstanceSource::InstallNew {
                                            installation.remote.host_prepared = false;
                                        }
                                    }
                                }
                            });

                            if let Some(status) = &provision_status {
                                ui.add_space(8.0);
                                ui.label(RichText::new(status).color(Color32::from_gray(160)));
                            }

                            if installation.remote.instance_source == RemoteInstanceSource::InstallNew {
                                let progress = provision_status
                                    .as_deref()
                                    .and_then(parse_provision_progress)
                                    .or_else(|| installation.remote.host_prepared.then_some(1.0))
                                    .unwrap_or(0.0);
                                ui.add_space(8.0);
                                ui.add(
                                    egui::ProgressBar::new(progress)
                                        .desired_width(ui.available_width())
                                        .text(if installation.remote.host_prepared {
                                            "Host prepared"
                                        } else if provision_running {
                                            "Preparing host..."
                                        } else {
                                            "Host not prepared"
                                        }),
                                );
                                if !self.provision_log.is_empty() {
                                    ui.add_space(8.0);
                                    Frame::default()
                                        .fill(Color32::from_rgb(24, 27, 31))
                                        .corner_radius(10.0)
                                        .inner_margin(10.0)
                                        .show(ui, |ui| {
                                            ui.label(
                                                RichText::new("Activity")
                                                    .size(12.5)
                                                    .color(Color32::from_gray(155)),
                                            );
                                            ScrollArea::vertical().max_height(120.0).show(
                                                ui,
                                                |ui| {
                                                    for line in
                                                        self.provision_log.iter().rev().take(8).rev()
                                                    {
                                                        ui.label(
                                                            RichText::new(line)
                                                                .size(12.2)
                                                                .color(Color32::from_gray(180)),
                                                        );
                                                    }
                                                },
                                            );
                                        });
                                }
                            }
                            ui.add_space(10.0);
                            let can_connect = match installation.remote.instance_source {
                                RemoteInstanceSource::AttachExisting => true,
                                RemoteInstanceSource::InstallNew => installation.remote.host_prepared,
                                RemoteInstanceSource::Unspecified => false,
                            };
                            if ui
                                .add_enabled(can_connect, Button::new("Open Admin TUI"))
                                .clicked()
                            {
                                open_tui = true;
                            }
                        }
                    }

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);
                    render_settings_section_label(ui, "Rolle");
                    if admin_unlocked {
                        ui.horizontal_wrapped(|ui| {
                            for role in ["Admin", "Chef", "Founder", "User"] {
                                if ui
                                    .selectable_label(desktop_state.role == role, role)
                                    .clicked()
                                {
                                    desktop_state.role = role.to_owned();
                                    persist_desktop_state = true;
                                }
                            }
                        });
                    } else {
                        let role = desktop_state.role.trim();
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(if role.is_empty() { "User" } else { role })
                                    .color(role_color(role))
                                    .strong(),
                            );
                            render_settings_hint(ui, "Rolle ist nur mit Admin-Rechten bearbeitbar.");
                        });
                    }

                    ui.add_space(12.0);
                    render_settings_section_label(ui, "Admin");
                    let admin_password_configured =
                        !installation.remote.password.trim().is_empty();
                    if !admin_password_configured {
                        ui.label(
                            RichText::new(
                                "Admin-Passwort setzen, bevor Admin-Aktionen verfuegbar sind.",
                            )
                            .size(12.2)
                            .color(Color32::from_rgb(185, 152, 82)),
                        );
                    } else if admin_unlocked {
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Open Admin TUI").clicked() {
                                open_tui = true;
                            }
                            if ui.button("Sperren").clicked() {
                                lock_admin = true;
                            }
                        });
                    } else {
                        ui.label(RichText::new("Admin password").color(Color32::from_gray(135)));
                        if ui
                            .add(
                                TextEdit::singleline(&mut admin_input)
                                    .password(true)
                                    .hint_text("Admin entsperren"),
                            )
                            .changed()
                        {
                            admin_input_changed = true;
                        }
                        if ui.button("Admin entsperren").clicked() {
                            admin_unlock_attempt = Some(admin_input.clone());
                        }
                    }

                    ui.add_space(12.0);
                    render_settings_section_label(ui, "Version");
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Installiert").color(Color32::from_gray(135)));
                        ui.monospace(
                            installation
                                .cached_version
                                .as_deref()
                                .unwrap_or("unbekannt"),
                        );
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Aktuell").color(Color32::from_gray(135)));
                        if let Some(latest) = self.latest_release.as_ref() {
                            ui.monospace(&latest.tag_name);
                        } else if let Some(error) = self.latest_release_error.as_ref() {
                            ui.label(
                                RichText::new(error)
                                    .size(12.0)
                                    .color(Color32::from_rgb(198, 98, 98)),
                            );
                        } else {
                            ui.label(RichText::new("wird geprueft").color(Color32::from_gray(145)));
                        }
                    });
                    if admin_unlocked {
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .add_enabled(
                                    update_available && !self.upgrade_running,
                                    Button::new("Upgrade"),
                                )
                                .clicked()
                            {
                                start_upgrade = true;
                            }
                            ui.add_enabled(false, Button::new("Rollback"));
                        });
                        if !update_available {
                            render_settings_hint(ui, "Kein Upgrade verfuegbar.");
                        }
                        render_settings_hint(
                            ui,
                            "TODO: Rollback anzeigen, sobald ein vorheriger Versionsstand gespeichert wird.",
                        );
                    } else {
                        render_settings_hint(ui, "Upgrade und Rollback brauchen Admin-Rechte.");
                    }

                    ui.add_space(12.0);
                    render_settings_section_label(ui, "Entfernen");
                    if ui
                        .add_sized(
                            [ui.available_width(), 30.0],
                            Button::new("Nur Verbindung entfernen"),
                        )
                        .clicked()
                    {
                        remove_connection = true;
                    }
                    render_settings_hint(ui, "Entfernt nur diesen Eintrag in der Desktop-App.");
                    ui.add_space(6.0);
                    if admin_unlocked {
                        let inline_delete_confirm =
                            self.remove_candidate_id.is_none() && self.remove_delete_confirm;
                        if inline_delete_confirm {
                            ui.label(
                                RichText::new(
                                    "Diese Aktion loescht die CTOX Installation auf dem Zielsystem.",
                                )
                                .size(11.8)
                                .color(Color32::from_rgb(220, 156, 100)),
                            );
                            ui.horizontal_wrapped(|ui| {
                                if ui.button("Endgueltig loeschen").clicked() {
                                    delete_installation = true;
                                }
                                if ui.button("Zurueck").clicked() {
                                    cancel_delete_installation = true;
                                }
                            });
                        } else {
                            if ui
                                .add_sized(
                                    [ui.available_width(), 30.0],
                                    Button::new("Installation loeschen..."),
                                )
                                .clicked()
                            {
                                request_delete_installation = true;
                            }
                            render_settings_hint(
                                ui,
                                "Loescht zusaetzlich den CTOX Installationsordner auf dem Zielsystem.",
                            );
                        }
                    } else {
                        render_settings_hint(
                            ui,
                            "Zum Loeschen der Installation erst Admin entsperren.",
                        );
                    }
                }
            });

        if admin_input_changed {
            self.admin_unlock_inputs
                .insert(installation_id.clone(), admin_input);
        }
        if let Some(attempt) = admin_unlock_attempt {
            let expected = self
                .registry
                .installations
                .iter()
                .find(|entry| entry.id == installation_id)
                .map(|entry| entry.remote.password.trim().to_owned())
                .unwrap_or_default();
            if !expected.is_empty() && attempt.trim() == expected {
                self.admin_unlocked_installations
                    .insert(installation_id.clone());
                self.admin_unlock_inputs.remove(&installation_id);
                self.notice = Some("Admin actions unlocked.".to_owned());
                let desktop_state = self
                    .registry
                    .desktop
                    .entry(installation_id.clone())
                    .or_default();
                desktop_state.admin_unlocked = true;
                desktop_state.role = "Admin".to_owned();
                if let Err(error) = self.registry.save() {
                    self.notice = Some(error.to_string());
                }
            } else {
                self.notice = Some("Admin password is incorrect.".to_owned());
            }
        }
        if persist_desktop_state {
            self.registry
                .desktop
                .insert(installation_id.clone(), desktop_state);
            if let Err(error) = self.registry.save() {
                self.notice = Some(error.to_string());
            }
        }
        if persist_registry {
            if let Err(error) = self.registry.save() {
                self.notice = Some(error.to_string());
            }
        }
        if lock_admin {
            self.admin_unlocked_installations.remove(&installation_id);
            if let Some(state) = self.registry.desktop.get_mut(&installation_id) {
                state.admin_unlocked = false;
                if let Err(error) = self.registry.save() {
                    self.notice = Some(error.to_string());
                }
            }
        }
        if remove_connection {
            self.remove_installation_connection(&installation_id);
        }
        if request_delete_installation {
            self.remove_delete_confirm = true;
        }
        if cancel_delete_installation {
            self.remove_delete_confirm = false;
        }
        if delete_installation {
            self.remove_delete_confirm = false;
            self.delete_entire_installation(&installation_id);
        }
        if start_provision {
            self.start_provisioning();
        }
        if start_upgrade {
            self.start_upgrade(&installation_id);
        }
        if open_tui {
            self.selected_installation_id = Some(installation_id);
            self.spawn_tui_tab();
        }
    }

    fn render_selected_instance_overview(&mut self, ui: &mut Ui) {
        let Some(installation) = self.selected_installation().cloned() else {
            ui.centered_and_justified(|ui| {
                ui.label("");
            });
            return;
        };

        let installation_id = installation.id.clone();
        let mut desktop_state = self.desktop_state_for(&installation_id);
        if self.admin_unlocked_installations.contains(&installation_id) {
            desktop_state.admin_unlocked = true;
            desktop_state.role = "Admin".to_owned();
        }
        let display_version = Self::display_version_for(&installation);
        let display_name = desktop_state
            .display_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| installation.display_name());
        let role = self.role_for(&installation);
        let is_admin_session = desktop_state.admin_unlocked || role == "Admin";
        let logged_in = desktop_state.logged_in
            || self
                .business_user_verified_installations
                .contains(&installation_id)
            || is_admin_session;
        let mut user_value = installation.remote.business_user.clone();
        let mut password_value = installation.remote.business_password.clone();
        let mut remember_login = desktop_state.remember_login;
        let mut new_user_name = self
            .new_user_name_inputs
            .get(&installation_id)
            .cloned()
            .unwrap_or_default();
        let mut new_user_email = self
            .new_user_email_inputs
            .get(&installation_id)
            .cloned()
            .unwrap_or_default();
        let mut new_user_password = self
            .new_user_password_inputs
            .get(&installation_id)
            .cloned()
            .unwrap_or_default();
        let mut new_user_role = self
            .new_user_role_inputs
            .get(&installation_id)
            .cloned()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "User".to_owned());
        let mut message_input = self
            .ctox_message_inputs
            .get(&installation_id)
            .cloned()
            .unwrap_or_default();
        let mut login_changed = false;
        let mut desktop_changed = false;
        let mut open_business_os = false;
        let mut open_business_os_admin = false;
        let mut open_tui = false;
        let mut send_message = false;
        let mut add_user = false;
        let mut remove_user_id: Option<String> = None;
        let mut user_state_changed = false;
        let mut remove_instance = false;

        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
            let origin = ui.next_widget_position();
            let detail_width = DETAIL_MAX_WIDTH.min((ui.available_width() - 48.0).max(620.0));
            let detail_height = (ui.available_height() - 28.0).max(560.0);
            let detail_rect = egui::Rect::from_min_size(
                egui::pos2(origin.x + 32.0, origin.y + 28.0),
                egui::vec2(detail_width, detail_height),
            );
            ui.allocate_ui_at_rect(detail_rect, |ui| {
                ui.set_width(detail_width);
                ui.set_max_width(detail_width);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(display_name.clone())
                            .size(28.0)
                            .color(UI_TEXT),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add_sized([106.0, 34.0], Button::new("Entfernen"))
                            .clicked()
                        {
                            remove_instance = true;
                        }
                    });
                });
                ui.add_space(24.0);
                ui.horizontal(|ui| {
                    draw_instance_logo(ui, 52.0);
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(installation.display_path())
                                .size(20.0)
                                .strong()
                                .color(UI_TEXT),
                        );
                        ui.label(
                            RichText::new(format!(
                                "{} · {}",
                                Self::method_label_for(&installation),
                                role
                            ))
                            .size(15.0)
                            .color(UI_MUTED),
                        );
                        if let Some(version) = display_version.as_deref() {
                            ui.label(RichText::new(version).size(13.0).color(UI_MUTED));
                        }
                    });
                });
                ui.add_space(26.0);
                ui.separator();
                ui.add_space(28.0);

                let content_width = DETAIL_CONTENT_WIDTH.min(detail_width);
                let content_rect = egui::Rect::from_min_size(
                    ui.next_widget_position(),
                    egui::vec2(content_width, (ui.available_height() - 16.0).max(420.0)),
                );
                ui.allocate_ui_at_rect(content_rect, |ui| {
                    ui.set_width(content_width);
                    ui.set_max_width(content_width);
                    if !logged_in {
                        ui.horizontal(|ui| {
                            ui.set_height(34.0);
                            ui.add_sized(
                                [110.0, 34.0],
                                egui::Label::new(RichText::new("User").size(15.0).color(UI_TEXT)),
                            );
                            if ui
                                .add_sized(
                                    [(content_width - 120.0).max(180.0), 34.0],
                                    TextEdit::singleline(&mut user_value).hint_text("User"),
                                )
                                .changed()
                            {
                                login_changed = true;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.set_height(34.0);
                            ui.add_sized(
                                [110.0, 34.0],
                                egui::Label::new(
                                    RichText::new("Passwort").size(15.0).color(UI_TEXT),
                                ),
                            );
                            if ui
                                .add_sized(
                                    [(content_width - 120.0).max(180.0), 34.0],
                                    TextEdit::singleline(&mut password_value)
                                        .password(true)
                                        .hint_text("Passwort"),
                                )
                                .changed()
                            {
                                login_changed = true;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.add_space(120.0);
                            if ui.checkbox(&mut remember_login, "Login merken").changed() {
                                desktop_state.remember_login = remember_login;
                                desktop_changed = true;
                            }
                        });
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.add_space(120.0);
                            if ui
                                .add_sized(
                                    [140.0, 34.0],
                                    Button::new(RichText::new("Login").color(UI_PAPER))
                                        .fill(UI_BLUE),
                                )
                                .clicked()
                            {
                                if user_value.trim().is_empty() || password_value.trim().is_empty()
                                {
                                    self.notice =
                                        Some("User und Passwort sind erforderlich.".to_owned());
                                } else {
                                    self.business_user_verified_installations
                                        .insert(installation_id.clone());
                                    desktop_state.logged_in = remember_login;
                                    desktop_state.remember_login = remember_login;
                                    desktop_changed = true;
                                    login_changed = true;
                                }
                            }
                        });
                    } else {
                        let business_rect = egui::Rect::from_min_size(
                            ui.next_widget_position(),
                            egui::vec2(content_width, 92.0),
                        );
                        ui.allocate_ui_at_rect(business_rect, |ui| {
                            Frame::default()
                                .fill(Color32::from_rgb(247, 249, 251))
                                .stroke(Stroke::new(1.0, UI_LINE))
                                .inner_margin(egui::Margin::symmetric(20, 18))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label(
                                                RichText::new("Business OS")
                                                    .size(21.0)
                                                    .strong()
                                                    .color(UI_TEXT),
                                            );
                                            ui.label(
                                                RichText::new("Webapp dieser CTOX Instanz öffnen.")
                                                    .size(14.0)
                                                    .color(UI_MUTED),
                                            );
                                        });
                                        ui.with_layout(
                                            Layout::right_to_left(Align::Center),
                                            |ui| {
                                                if ui
                                                    .add_sized(
                                                        [210.0, 46.0],
                                                        Button::new(
                                                            RichText::new("Business OS öffnen")
                                                                .size(18.0)
                                                                .strong()
                                                                .color(UI_PAPER),
                                                        )
                                                        .fill(UI_BLUE),
                                                    )
                                                    .clicked()
                                                {
                                                    open_business_os = true;
                                                }
                                                if is_admin_session {
                                                    if ui
                                                        .add_sized([90.0, 34.0], Button::new("TUI"))
                                                        .clicked()
                                                    {
                                                        open_tui = true;
                                                    }
                                                    if ui
                                                        .add_sized(
                                                            [150.0, 34.0],
                                                            Button::new("Business OS Admin"),
                                                        )
                                                        .clicked()
                                                    {
                                                        open_business_os_admin = true;
                                                    }
                                                }
                                            },
                                        );
                                    });
                                });
                        });
                        ui.advance_cursor_after_rect(business_rect);

                        ui.add_space(22.0);
                        if is_admin_session {
                            ui.label(RichText::new("Nutzer").size(18.0).strong().color(UI_TEXT));
                            ui.label(
                                RichText::new("Chef, Founder und User verwalten.")
                                    .size(13.0)
                                    .color(UI_MUTED),
                            );
                            ui.add_space(4.0);
                            if desktop_state.users.is_empty() {
                                Frame::default()
                                    .fill(Color32::from_rgb(251, 251, 250))
                                    .stroke(Stroke::new(1.0, UI_LINE_SOFT))
                                    .inner_margin(egui::Margin::symmetric(11, 9))
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new(
                                                "Noch keine Business-OS Nutzer angelegt.",
                                            )
                                            .size(14.0)
                                            .color(UI_TEXT),
                                        );
                                    });
                                ui.add_space(8.0);
                            }
                            for user in &mut desktop_state.users {
                                ui.horizontal(|ui| {
                                    draw_status_dot(
                                        ui,
                                        if user.active { UI_GREEN } else { UI_LINE },
                                    );
                                    ui.add_sized(
                                        [150.0, 28.0],
                                        egui::Label::new(
                                            RichText::new(user.username.clone())
                                                .size(15.0)
                                                .color(UI_TEXT),
                                        ),
                                    );
                                    ui.add_sized(
                                        [210.0, 28.0],
                                        egui::Label::new(
                                            RichText::new(if user.email.trim().is_empty() {
                                                "keine E-Mail".to_owned()
                                            } else {
                                                user.email.clone()
                                            })
                                            .size(13.0)
                                            .color(UI_MUTED),
                                        ),
                                    );
                                    for role_option in ["Chef", "Founder", "User"] {
                                        if ui
                                            .selectable_label(user.role == role_option, role_option)
                                            .clicked()
                                        {
                                            user.role = role_option.to_owned();
                                            user_state_changed = true;
                                        }
                                    }
                                    if ui
                                        .button(if user.active { "Sperren" } else { "Aktivieren" })
                                        .clicked()
                                    {
                                        user.active = !user.active;
                                        user_state_changed = true;
                                    }
                                    if ui.button("Entfernen").clicked() {
                                        remove_user_id = Some(user.id.clone());
                                    }
                                });
                            }
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    [150.0, 30.0],
                                    TextEdit::singleline(&mut new_user_name).hint_text("User"),
                                );
                                ui.add_sized(
                                    [210.0, 30.0],
                                    TextEdit::singleline(&mut new_user_email).hint_text("E-Mail"),
                                );
                                ui.add_sized(
                                    [150.0, 30.0],
                                    TextEdit::singleline(&mut new_user_password)
                                        .password(true)
                                        .hint_text("Passwort"),
                                );
                                egui::ComboBox::from_id_salt(format!(
                                    "new-user-role-{installation_id}"
                                ))
                                .selected_text(new_user_role.as_str())
                                .width(95.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut new_user_role,
                                        "User".to_owned(),
                                        "User",
                                    );
                                    ui.selectable_value(
                                        &mut new_user_role,
                                        "Founder".to_owned(),
                                        "Founder",
                                    );
                                    ui.selectable_value(
                                        &mut new_user_role,
                                        "Chef".to_owned(),
                                        "Chef",
                                    );
                                });
                                if ui.button("User anlegen").clicked() {
                                    add_user = true;
                                }
                            });
                            ui.add_space(18.0);
                        }

                        ui.label(RichText::new("CTOX").size(18.0).strong().color(UI_TEXT));
                        ui.label(
                            RichText::new("Antwortet auf diesem Kanal.")
                                .size(13.0)
                                .color(UI_MUTED),
                        );
                        ui.add_space(4.0);
                        ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                            if desktop_state.messages.is_empty() {
                                Frame::default()
                                    .fill(Color32::from_rgb(251, 251, 250))
                                    .stroke(Stroke::new(1.0, UI_LINE_SOFT))
                                    .inner_margin(egui::Margin::symmetric(11, 9))
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new(
                                                "Bereit für Anfragen an diese CTOX Instanz.",
                                            )
                                            .size(14.0)
                                            .color(UI_TEXT),
                                        );
                                    });
                            }
                            for message in &desktop_state.messages {
                                Frame::default()
                                    .fill(if message.sender == "CTOX" {
                                        Color32::from_rgb(251, 251, 250)
                                    } else {
                                        Color32::from_rgb(238, 246, 255)
                                    })
                                    .stroke(Stroke::new(1.0, UI_LINE_SOFT))
                                    .inner_margin(egui::Margin::symmetric(11, 9))
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new(&message.sender)
                                                .size(12.0)
                                                .strong()
                                                .color(UI_MUTED),
                                        );
                                        ui.label(
                                            RichText::new(&message.body).size(14.0).color(UI_TEXT),
                                        );
                                    });
                                ui.add_space(6.0);
                            }
                        });
                        ui.add_space(8.0);
                        if ui
                            .add_sized(
                                [content_width, 84.0],
                                TextEdit::multiline(&mut message_input)
                                    .hint_text("Nachricht an CTOX"),
                            )
                            .changed()
                        {
                            self.ctox_message_inputs
                                .insert(installation_id.clone(), message_input.clone());
                        }
                        if ui
                            .add_sized(
                                [110.0, 34.0],
                                Button::new(RichText::new("Senden").color(UI_PAPER)).fill(UI_BLUE),
                            )
                            .clicked()
                        {
                            send_message = true;
                        }
                    }
                });
            });

            if let Some(notice) = &self.notice {
                ui.add_space(14.0);
                ui.label(RichText::new(notice).color(UI_MUTED));
            }
        });

        if login_changed {
            if let Some(entry) = self
                .registry
                .installations
                .iter_mut()
                .find(|entry| entry.id == installation_id)
            {
                entry.remote.business_user = user_value.clone();
                entry.remote.business_password = password_value.clone();
                if let Err(error) = self.registry.save() {
                    self.notice = Some(error.to_string());
                }
            }
        }
        self.new_user_name_inputs
            .insert(installation_id.clone(), new_user_name.clone());
        self.new_user_email_inputs
            .insert(installation_id.clone(), new_user_email.clone());
        self.new_user_password_inputs
            .insert(installation_id.clone(), new_user_password.clone());
        self.new_user_role_inputs
            .insert(installation_id.clone(), new_user_role.clone());
        if add_user {
            let username = new_user_name.trim();
            let email = new_user_email.trim();
            let password = new_user_password.trim();
            if username.is_empty() || password.is_empty() {
                self.notice = Some("User und Passwort sind erforderlich.".to_owned());
            } else if desktop_state
                .users
                .iter()
                .any(|user| user.username == username)
            {
                self.notice = Some("User existiert bereits.".to_owned());
            } else {
                desktop_state.users.push(InstallationUser {
                    id: Uuid::new_v4().to_string(),
                    username: username.to_owned(),
                    email: email.to_owned(),
                    role: new_user_role.trim().to_owned(),
                    active: true,
                });
                self.new_user_name_inputs.remove(&installation_id);
                self.new_user_email_inputs.remove(&installation_id);
                self.new_user_password_inputs.remove(&installation_id);
                self.new_user_role_inputs.remove(&installation_id);
                desktop_changed = true;
            }
        }
        if let Some(remove_id) = remove_user_id {
            desktop_state.users.retain(|user| user.id != remove_id);
            desktop_changed = true;
        }
        if user_state_changed {
            desktop_changed = true;
        }
        if send_message {
            let body = message_input.trim();
            if !body.is_empty() {
                desktop_state.messages.push(InstallationMessage {
                    id: Uuid::new_v4().to_string(),
                    sender: user_value
                        .trim()
                        .is_empty()
                        .then_some("User")
                        .unwrap_or(user_value.trim())
                        .to_owned(),
                    body: body.to_owned(),
                    created_at: None,
                });
                desktop_state.messages.push(InstallationMessage {
                    id: Uuid::new_v4().to_string(),
                    sender: "CTOX".to_owned(),
                    body: "Nachricht empfangen. Die echte CTOX-Antwort wird hier ueber den Instanzkanal angezeigt.".to_owned(),
                    created_at: None,
                });
                self.ctox_message_inputs.remove(&installation_id);
                desktop_changed = true;
            }
        }
        if desktop_changed {
            self.registry
                .desktop
                .insert(installation_id.clone(), desktop_state);
            if let Err(error) = self.registry.save() {
                self.notice = Some(error.to_string());
            }
        }
        if open_business_os {
            if let Err(error) = self.open_business_os_proxy_for_selected() {
                self.notice = Some(error.to_string());
            }
        }
        if open_business_os_admin {
            if let Err(error) = self.open_business_os_admin_for_selected() {
                self.notice = Some(error.to_string());
            }
        }
        if open_tui {
            self.focus_or_open_tui(&installation_id);
        }
        if remove_instance {
            self.request_remove_installation(installation_id);
        }
    }

    fn render_selected_instance_settings(&mut self, ui: &mut Ui) {
        let Some(installation) = self.selected_installation().cloned() else {
            return;
        };
        let installation_id = installation.id.clone();
        let mut desktop_state = self.desktop_state_for(&installation_id);
        let admin_unlocked = self.admin_unlocked_installations.contains(&installation_id)
            || desktop_state.admin_unlocked;
        if admin_unlocked {
            desktop_state.admin_unlocked = true;
            desktop_state.role = "Admin".to_owned();
        }

        let mut display_name = desktop_state
            .display_name
            .clone()
            .unwrap_or_else(|| installation.display_name());
        let mut logo_path = desktop_state
            .logo_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let mut admin_input = self
            .admin_unlock_inputs
            .get(&installation_id)
            .cloned()
            .unwrap_or_default();
        let mut address_path = installation
            .root_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| installation.remote.install_root.clone());
        let mut ssh_host = installation.remote.ssh_host.clone();
        let mut ssh_user = installation.remote.ssh_user.clone();
        let mut endpoint = installation
            .env
            .get("CTOX_BUSINESS_OS_URL")
            .cloned()
            .unwrap_or_default();
        let mut room_id = installation.remote.room_id.clone();
        let mut room_password = installation.remote.password.clone();
        let mut role_value = desktop_state.role.clone();
        let mut desktop_changed = false;
        let mut installation_changed = false;
        let mut admin_input_changed = false;
        let mut unlock_admin = false;
        let mut lock_admin = false;
        let mut start_upgrade = false;
        let mut request_remove = false;
        let mut next_page = self.settings_page;
        let mut close_settings = false;
        let mut open_tui = false;
        let runtime_status = self.installation_runtime_status(&installation_id);
        let update_available = self.installation_update_available(&installation);
        let page_title = match self.settings_page {
            Some(SettingsPage::Identity) => "Darstellung",
            Some(SettingsPage::Address) => "Adresse",
            Some(SettingsPage::Role) => "Rolle",
            Some(SettingsPage::Admin) => "Admin",
            Some(SettingsPage::Version) => "Version",
            Some(SettingsPage::Remove) => "Entfernen",
            None => "Instanz-Konfiguration",
        };
        let address_summary = format!(
            "{} · {}",
            Self::method_label_for(&installation),
            installation.display_path()
        );

        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(10.0, 10.0);
            ui.add_space(30.0);
            ui.horizontal(|ui| {
                ui.add_space(34.0);
                ui.vertical(|ui| {
                    ui.set_max_width(1100.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(page_title)
                                .size(30.0)
                                .strong()
                                .color(UI_TEXT),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if self.settings_page != Some(SettingsPage::Remove)
                                && ui
                                    .add_sized([108.0, 36.0], Button::new("Entfernen"))
                                    .clicked()
                            {
                                next_page = Some(SettingsPage::Remove);
                            }
                            if ui.add_sized([110.0, 36.0], Button::new("Verbinden")).clicked() {
                                close_settings = true;
                            }
                        });
                    });
                    ui.add_space(22.0);
                    ui.horizontal(|ui| {
                        draw_instance_logo(ui, 56.0);
                        ui.add_space(14.0);
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(display_name.clone())
                                    .size(24.0)
                                    .strong()
                                    .color(UI_TEXT),
                            );
                            ui.label(
                                RichText::new(installation.display_path())
                                    .size(15.0)
                                    .color(UI_MUTED),
                            );
                            ui.horizontal(|ui| {
                                draw_status_dot(ui, runtime_status.color);
                                ui.label(
                                    RichText::new(format!(
                                        "{} · {}",
                                        Self::method_label_for(&installation),
                                        desktop_state.role
                                    ))
                                    .size(14.0)
                                    .color(UI_MUTED),
                                );
                            });
                        });
                    });
                    ui.add_space(28.0);
                    ui.separator();
                    ui.add_space(18.0);

                    match self.settings_page {
                        None => {
                            settings_display_row(ui, "Darstellung", &display_name, "Ändern", || {
                                next_page = Some(SettingsPage::Identity);
                            });
                            settings_display_row(ui, "Adresse", &address_summary, "Ändern", || {
                                next_page = Some(SettingsPage::Address);
                            });
                            settings_display_row(ui, "Rolle", &desktop_state.role, "Ändern", || {
                                next_page = Some(SettingsPage::Role);
                            });
                            settings_display_row(
                                ui,
                                "Admin",
                                if admin_unlocked {
                                    "Entsperrt"
                                } else {
                                    "Gesperrt"
                                },
                                if admin_unlocked {
                                    "Sperren"
                                } else {
                                    "Entsperren"
                                },
                                || {
                                    next_page = Some(SettingsPage::Admin);
                                },
                            );
                            let latest = self
                                .latest_release
                                .as_ref()
                                .map(|release| release.tag_name.as_str())
                                .unwrap_or("unbekannt");
                            let version_summary = format!(
                                "Installiert {} · Aktuell {}",
                                installation.cached_version.as_deref().unwrap_or("unbekannt"),
                                latest
                            );
                            settings_display_row(ui, "Version", &version_summary, "Ändern", || {
                                next_page = Some(SettingsPage::Version);
                            });
                        }
                        Some(SettingsPage::Identity) => {
                            settings_row(ui, "Name", |ui| {
                                if ui
                                    .add_enabled(
                                        admin_unlocked,
                                        TextEdit::singleline(&mut display_name),
                                    )
                                    .changed()
                                {
                                    desktop_changed = true;
                                    installation_changed = true;
                                }
                                if !admin_unlocked {
                                    ui.label(
                                        RichText::new(
                                            "Name und Logo sind nur mit Admin-Rechten editierbar.",
                                        )
                                        .size(14.0)
                                        .color(UI_MUTED),
                                    );
                                }
                            });
                            settings_row(ui, "Logo", |ui| {
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(
                                            admin_unlocked,
                                            TextEdit::singleline(&mut logo_path)
                                                .hint_text("Pfad zum CTOX Instanzlogo"),
                                        )
                                        .changed()
                                    {
                                        desktop_changed = true;
                                    }
                                });
                            });
                            if ui.button("Zurück").clicked() {
                                next_page = None;
                            }
                        }
                        Some(SettingsPage::Address) => {
                            match installation.mode {
                                InstallationMode::Local => {
                                    settings_row(ui, "Local", |ui| {
                                        ui.label(
                                            RichText::new("Lokale CTOX Installation")
                                                .size(15.0)
                                                .color(UI_TEXT),
                                        );
                                    });
                                    settings_row(ui, "Pfad", |ui| {
                                        if ui
                                            .add_enabled(
                                                admin_unlocked,
                                                TextEdit::singleline(&mut address_path),
                                            )
                                            .changed()
                                        {
                                            installation_changed = true;
                                        }
                                    });
                                }
                                InstallationMode::RemoteWebRtc => {
                                    if installation.remote.host_target == RemoteHostTarget::Ssh {
                                        settings_row(ui, "Host", |ui| {
                                            if ui
                                                .add_enabled(
                                                    admin_unlocked,
                                                    TextEdit::singleline(&mut ssh_host),
                                                )
                                                .changed()
                                            {
                                                installation_changed = true;
                                            }
                                        });
                                        settings_row(ui, "SSH User", |ui| {
                                            if ui
                                                .add_enabled(
                                                    admin_unlocked,
                                                    TextEdit::singleline(&mut ssh_user),
                                                )
                                                .changed()
                                            {
                                                installation_changed = true;
                                            }
                                        });
                                        settings_row(ui, "Remote CTOX Pfad", |ui| {
                                            if ui
                                                .add_enabled(
                                                    admin_unlocked,
                                                    TextEdit::singleline(&mut address_path)
                                                        .hint_text("leer lassen fuer Auto-Suche"),
                                                )
                                                .changed()
                                            {
                                                installation_changed = true;
                                            }
                                        });
                                    } else if installation.env.contains_key("CTOX_BUSINESS_OS_URL")
                                    {
                                        settings_row(ui, "URL/IP", |ui| {
                                            if ui
                                                .add_enabled(
                                                    admin_unlocked,
                                                    TextEdit::singleline(&mut endpoint),
                                                )
                                                .changed()
                                            {
                                                installation_changed = true;
                                            }
                                        });
                                    } else {
                                        settings_row(ui, "Peer2Peer Server", |ui| {
                                            if ui
                                                .add_enabled(
                                                    admin_unlocked,
                                                    TextEdit::singleline(&mut endpoint),
                                                )
                                                .changed()
                                            {
                                                installation_changed = true;
                                            }
                                        });
                                        settings_row(ui, "Room", |ui| {
                                            if ui
                                                .add_enabled(
                                                    admin_unlocked,
                                                    TextEdit::singleline(&mut room_id),
                                                )
                                                .changed()
                                            {
                                                installation_changed = true;
                                            }
                                        });
                                        settings_row(ui, "Room Password", |ui| {
                                            if ui
                                                .add_enabled(
                                                    admin_unlocked,
                                                    TextEdit::singleline(&mut room_password)
                                                        .password(true),
                                                )
                                                .changed()
                                            {
                                                installation_changed = true;
                                            }
                                        });
                                    }
                                }
                            }
                            if !admin_unlocked {
                                ui.label(
                                    RichText::new("Adresse ist nur mit Admin-Rechten editierbar.")
                                        .size(14.0)
                                        .color(UI_MUTED),
                                );
                            }
                            if ui.button("Zurück").clicked() {
                                next_page = None;
                            }
                        }
                        Some(SettingsPage::Role) => {
                            settings_row(ui, "Rolle", |ui| {
                                ui.horizontal(|ui| {
                                    for role in ["Admin", "Chef", "Founder", "User"] {
                                        if ui
                                            .add_enabled(
                                                admin_unlocked,
                                                egui::SelectableLabel::new(
                                                    role_value == role,
                                                    role,
                                                ),
                                            )
                                            .clicked()
                                        {
                                            role_value = role.to_owned();
                                            desktop_changed = true;
                                        }
                                    }
                                });
                                if !admin_unlocked {
                                    ui.label(
                                        RichText::new(
                                            "Rollen werden nur mit Admin-Rechten geändert.",
                                        )
                                        .size(14.0)
                                        .color(UI_MUTED),
                                    );
                                }
                            });
                            if ui.button("Zurück").clicked() {
                                next_page = None;
                            }
                        }
                        Some(SettingsPage::Admin) => {
                            settings_row(ui, "Admin", |ui| {
                                if admin_unlocked {
                                    ui.label(
                                        RichText::new(
                                            "TUI, Install, Upgrade und Instanzverwaltung sind entsperrt.",
                                        )
                                        .size(15.0)
                                        .color(UI_TEXT),
                                    );
                                    ui.horizontal(|ui| {
                                        if ui.button("TUI öffnen").clicked() {
                                            open_tui = true;
                                        }
                                        if ui.button("Sperren").clicked() {
                                            lock_admin = true;
                                        }
                                    });
                                } else {
                                    ui.label(
                                        RichText::new(
                                            "Admin-Passwort ist das SSH/Sudo Passwort dieser Instanz.",
                                        )
                                        .size(15.0)
                                        .color(UI_MUTED),
                                    );
                                    ui.horizontal(|ui| {
                                        if ui
                                            .add_sized(
                                                [300.0, 30.0],
                                                TextEdit::singleline(&mut admin_input)
                                                    .password(true)
                                                    .hint_text("Admin-Passwort"),
                                            )
                                            .changed()
                                        {
                                            admin_input_changed = true;
                                        }
                                        if ui.button("Entsperren").clicked() {
                                            unlock_admin = true;
                                        }
                                    });
                                }
                            });
                            if ui.button("Zurück").clicked() {
                                next_page = None;
                            }
                        }
                        Some(SettingsPage::Version) => {
                            let latest = self
                                .latest_release
                                .as_ref()
                                .map(|release| release.tag_name.as_str())
                                .unwrap_or("unbekannt");
                            settings_row(ui, "Installiert", |ui| {
                                ui.label(
                                    RichText::new(
                                        installation
                                            .cached_version
                                            .as_deref()
                                            .unwrap_or("unbekannt"),
                                    )
                                    .size(16.0)
                                    .color(UI_TEXT),
                                );
                            });
                            settings_row(ui, "Aktuell", |ui| {
                                ui.label(RichText::new(latest).size(16.0).color(UI_TEXT));
                            });
                            settings_row(ui, "Upgrade", |ui| {
                                if ui
                                    .add_enabled(
                                        admin_unlocked && update_available && !self.upgrade_running,
                                        Button::new("Upgrade"),
                                    )
                                    .clicked()
                                {
                                    start_upgrade = true;
                                }
                                if !admin_unlocked {
                                    ui.label(
                                        RichText::new("Upgrade braucht Admin-Rechte.")
                                            .size(14.0)
                                            .color(UI_MUTED),
                                    );
                                }
                            });
                            if ui.button("Zurück").clicked() {
                                next_page = None;
                            }
                        }
                        Some(SettingsPage::Remove) => {
                            settings_row(ui, "Entfernen", |ui| {
                                ui.label(
                                    RichText::new(
                                        "Verbindung entfernen löscht nur den Desktop-Eintrag.",
                                    )
                                    .size(15.0)
                                    .color(UI_TEXT),
                                );
                                ui.horizontal(|ui| {
                                    if ui.button("Nur Verbindung entfernen").clicked() {
                                        request_remove = true;
                                    }
                                    if ui
                                        .add_enabled(
                                            admin_unlocked,
                                            Button::new("Installation löschen"),
                                        )
                                        .clicked()
                                    {
                                        request_remove = true;
                                        self.remove_delete_confirm = true;
                                    }
                                    if ui.button("Abbrechen").clicked() {
                                        next_page = None;
                                    }
                                });
                                if !admin_unlocked {
                                    ui.label(
                                        RichText::new(
                                            "Installation löschen braucht Admin-Rechte.",
                                        )
                                        .size(14.0)
                                        .color(UI_MUTED),
                                    );
                                }
                            });
                        }
                    }
                });
            });
            if let Some(notice) = &self.notice {
                ui.add_space(16.0);
                ui.label(RichText::new(notice).size(14.0).color(UI_MUTED));
            }
        });

        if admin_input_changed {
            self.admin_unlock_inputs
                .insert(installation_id.clone(), admin_input.clone());
        }
        if unlock_admin {
            match self.verify_admin_password(&installation, &admin_input) {
                Ok(()) => {
                    self.admin_unlocked_installations
                        .insert(installation_id.clone());
                    desktop_state.admin_unlocked = true;
                    desktop_state.role = "Admin".to_owned();
                    self.notice = Some("Admin entsperrt.".to_owned());
                }
                Err(error) => {
                    self.notice = Some(error.to_string());
                }
            }
        }
        if lock_admin {
            self.admin_unlocked_installations.remove(&installation_id);
            desktop_state.admin_unlocked = false;
            desktop_state.role = "User".to_owned();
            self.notice = Some("Admin gesperrt.".to_owned());
        }
        if desktop_changed || unlock_admin || lock_admin {
            desktop_state.display_name = non_empty_opt(&display_name).map(str::to_owned);
            desktop_state.logo_path =
                non_empty_opt(&logo_path).map(|value| PathBuf::from(shellexpand_tilde(value)));
            desktop_state.role = role_value;
        }
        if installation_changed || desktop_changed {
            if let Some(entry) = self
                .registry
                .installations
                .iter_mut()
                .find(|entry| entry.id == installation_id)
            {
                entry.name = non_empty_or(display_name.trim(), &entry.display_name()).to_owned();
                match entry.mode {
                    InstallationMode::Local => {
                        if !address_path.trim().is_empty() {
                            entry.root_path = Some(PathBuf::from(shellexpand_tilde(&address_path)));
                        }
                    }
                    InstallationMode::RemoteWebRtc => {
                        if entry.remote.host_target == RemoteHostTarget::Ssh {
                            entry.remote.ssh_host = ssh_host.trim().to_owned();
                            entry.remote.ssh_user = ssh_user.trim().to_owned();
                            entry.remote.install_root = address_path.trim().to_owned();
                        } else if entry.env.contains_key("CTOX_BUSINESS_OS_URL") {
                            entry.env.insert(
                                "CTOX_BUSINESS_OS_URL".to_owned(),
                                normalize_business_os_endpoint(endpoint.trim()),
                            );
                        } else {
                            entry.remote.signaling_urls = non_empty_opt(endpoint.trim())
                                .map(|value| vec![value.to_owned()])
                                .unwrap_or_default();
                            entry.remote.room_id = room_id.trim().to_owned();
                            entry.remote.password = room_password.trim().to_owned();
                        }
                    }
                }
            }
        }
        if unlock_admin || lock_admin || desktop_changed {
            self.registry
                .desktop
                .insert(installation_id.clone(), desktop_state);
        }
        if unlock_admin || lock_admin || desktop_changed || installation_changed {
            if let Err(error) = self.registry.save() {
                self.notice = Some(error.to_string());
            }
        }
        if start_upgrade {
            self.start_upgrade(&installation_id);
        }
        if request_remove {
            self.request_remove_installation(installation_id.clone());
        }
        if open_tui {
            self.focus_or_open_tui(&installation_id);
        }
        if close_settings {
            self.show_settings_view = false;
            self.settings_page = None;
        } else {
            self.settings_page = next_page;
        }
    }

    fn verify_admin_password(&mut self, installation: &Installation, input: &str) -> Result<()> {
        let password = input.trim();
        anyhow::ensure!(!password.is_empty(), "Admin password required.");
        match installation.mode {
            InstallationMode::RemoteWebRtc => {
                let expected = installation.remote.ssh_password.trim();
                anyhow::ensure!(
                    !expected.is_empty(),
                    "SSH password is not configured for this instance."
                );
                anyhow::ensure!(password == expected, "Admin password rejected.");
            }
            InstallationMode::Local => {
                let expected = installation.remote.password.trim();
                if expected.is_empty() {
                    if let Some(entry) = self
                        .registry
                        .installations
                        .iter_mut()
                        .find(|entry| entry.id == installation.id)
                    {
                        entry.remote.password = password.to_owned();
                        let _ = self.registry.save();
                    }
                } else {
                    anyhow::ensure!(password == expected, "Admin password rejected.");
                }
            }
        }
        Ok(())
    }

    fn render_add_flow_summary(&mut self, ui: &mut Ui) {
        let is_connect = self.add_flow != AddInstanceFlow::New;
        let mut submit = false;
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(10.0, 10.0);
            ui.add_space(28.0);
            ui.horizontal(|ui| {
                ui.add_space(32.0);
                ui.vertical(|ui| {
                    ui.set_max_width(980.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(if is_connect {
                                "CTOX verbinden"
                            } else {
                                "CTOX installieren"
                            })
                            .size(28.0)
                            .color(UI_TEXT),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_enabled(false, Button::new("Entfernen"));
                            if ui
                                .add_sized(
                                    [118.0, 34.0],
                                    Button::new(
                                        RichText::new(if is_connect {
                                            "Verbinden"
                                        } else {
                                            "Installieren"
                                        })
                                        .color(UI_PAPER),
                                    )
                                    .fill(UI_BLUE),
                                )
                                .clicked()
                            {
                                submit = true;
                            }
                        });
                    });
                    ui.add_space(24.0);
                    ui.horizontal(|ui| {
                        draw_instance_logo(ui, 52.0);
                        ui.add_space(12.0);
                        ui.vertical(|ui| {
                            let (name, sub) = if is_connect {
                                self.connect_summary_head()
                            } else {
                                self.new_summary_head()
                            };
                            ui.label(RichText::new(name).size(20.0).strong().color(UI_TEXT));
                            ui.label(RichText::new(sub).size(15.0).color(UI_MUTED));
                        });
                    });
                    ui.add_space(26.0);
                    ui.separator();
                    ui.add_space(16.0);
                    let rows = if is_connect {
                        self.connect_summary_rows()
                    } else {
                        self.new_summary_rows()
                    };
                    for (label, value) in rows {
                        settings_display_row(ui, &label, &value, "Ändern", || {});
                    }
                });
            });
        });

        if submit {
            if is_connect {
                self.create_connect_instance_from_draft();
            } else {
                self.create_new_instance_from_draft();
            }
        }
    }

    fn connect_summary_head(&self) -> (String, String) {
        match self.connect_draft.mode {
            ConnectMode::Local => (
                non_empty_or(self.connect_draft.name.trim(), "Local CTOX").to_owned(),
                "Lokale Installation einbinden.".to_owned(),
            ),
            ConnectMode::Ssh => (
                non_empty_or(self.connect_draft.name.trim(), "Remote CTOX").to_owned(),
                "Über SSH als Admin verbinden.".to_owned(),
            ),
            ConnectMode::Direct => (
                non_empty_or(self.connect_draft.name.trim(), "Direkte CTOX Adresse").to_owned(),
                "Business OS direkt über URL oder IP öffnen.".to_owned(),
            ),
            ConnectMode::Signaling => (
                non_empty_or(self.connect_draft.room.trim(), "CTOX Room").to_owned(),
                "Über Peer2Peer Room verbinden.".to_owned(),
            ),
        }
    }

    fn new_summary_head(&self) -> (String, String) {
        if self.new_draft.local {
            (
                non_empty_or(self.new_draft.name.trim(), "Local CTOX").to_owned(),
                "Installation auf diesem Rechner.".to_owned(),
            )
        } else {
            (
                non_empty_or(self.new_draft.name.trim(), "Remote CTOX").to_owned(),
                "Installation auf Rechner oder VPS per SSH.".to_owned(),
            )
        }
    }

    fn connect_summary_rows(&self) -> Vec<(String, String)> {
        match self.connect_draft.mode {
            ConnectMode::Local => vec![
                (
                    "Name".to_owned(),
                    non_empty_or(self.connect_draft.name.trim(), "Local CTOX").to_owned(),
                ),
                (
                    "Pfad".to_owned(),
                    non_empty_or(
                        self.connect_draft.install_root.trim(),
                        "/Users/michaelwelsch/.local/lib/ctox/current",
                    )
                    .to_owned(),
                ),
            ],
            ConnectMode::Ssh => vec![
                (
                    "Name".to_owned(),
                    non_empty_or(self.connect_draft.name.trim(), "Remote CTOX").to_owned(),
                ),
                (
                    "Host".to_owned(),
                    non_empty_or(self.connect_draft.ssh_host.trim(), "Host/IP fehlt").to_owned(),
                ),
                (
                    "SSH User".to_owned(),
                    non_empty_or(self.connect_draft.ssh_user.trim(), "SSH User fehlt").to_owned(),
                ),
            ],
            ConnectMode::Direct => vec![
                (
                    "URL/IP".to_owned(),
                    non_empty_or(self.connect_draft.endpoint.trim(), "URL/IP fehlt").to_owned(),
                ),
                ("Login".to_owned(), "User und Passwort".to_owned()),
            ],
            ConnectMode::Signaling => vec![
                (
                    "Peer2Peer".to_owned(),
                    non_empty_or(self.connect_draft.endpoint.trim(), "Server URL fehlt").to_owned(),
                ),
                (
                    "Room".to_owned(),
                    non_empty_or(self.connect_draft.room.trim(), "Room fehlt").to_owned(),
                ),
            ],
        }
    }

    fn new_summary_rows(&self) -> Vec<(String, String)> {
        if self.new_draft.local {
            vec![
                (
                    "Name".to_owned(),
                    non_empty_or(self.new_draft.name.trim(), "Local CTOX").to_owned(),
                ),
                (
                    "Pfad".to_owned(),
                    non_empty_or(
                        self.new_draft.install_root.trim(),
                        "/Users/michaelwelsch/.local/lib/ctox/current",
                    )
                    .to_owned(),
                ),
            ]
        } else {
            vec![
                (
                    "Name".to_owned(),
                    non_empty_or(self.new_draft.name.trim(), "Remote CTOX").to_owned(),
                ),
                (
                    "Host".to_owned(),
                    non_empty_or(self.new_draft.host.trim(), "Host/IP fehlt").to_owned(),
                ),
                (
                    "SSH User".to_owned(),
                    non_empty_or(self.new_draft.ssh_user.trim(), "SSH User fehlt").to_owned(),
                ),
                (
                    "Ziel".to_owned(),
                    non_empty_or(
                        self.new_draft.install_root.trim(),
                        "~/.local/lib/ctox/current",
                    )
                    .to_owned(),
                ),
            ]
        }
    }

    fn render_tabs(&mut self, ui: &mut Ui) {
        let command_tabs: Vec<(String, String)> = self
            .tabs
            .iter()
            .filter(|tab| tab.kind == SessionKind::Command)
            .map(|tab| (tab.id.clone(), tab.title.clone()))
            .collect();
        if command_tabs.is_empty() {
            return;
        }

        ui.horizontal_wrapped(|ui| {
            for (tab_id, title) in command_tabs {
                let is_active = self.active_tab_id.as_deref() == Some(tab_id.as_str());
                Frame::default()
                    .fill(if is_active {
                        Color32::from_rgb(32, 36, 43)
                    } else {
                        Color32::from_rgb(20, 22, 26)
                    })
                    .stroke(Stroke::new(1.0, Color32::from_rgb(56, 62, 72)))
                    .corner_radius(6.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.selectable_label(is_active, title).clicked() {
                                self.active_tab_id = Some(tab_id.clone());
                                self.terminal_focus = true;
                            }
                            if ui.small_button("x").clicked() {
                                self.close_tab(&tab_id);
                            }
                        });
                    });
            }
        });
    }

    fn render_terminal_area(&mut self, ui: &mut Ui) {
        let Some(active_tab_id) = self.active_tab_id.clone() else {
            if self.show_add_menu {
                ui.allocate_space(ui.available_size());
                return;
            }
            if self.remove_candidate_id.is_some() {
                return;
            }
            if self.show_settings_view {
                self.render_selected_instance_settings(ui);
                return;
            }
            // Show provisioning stream in the main area when provisioning is active or has logs
            if self.provision_running || !self.provision_log.is_empty() {
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        let title = if self.provision_running {
                            "Installation läuft..."
                        } else if self
                            .provision_status
                            .as_deref()
                            .map_or(false, |s| s.contains("vorbereitet"))
                        {
                            "Installation abgeschlossen"
                        } else {
                            "Installations-Log"
                        };
                        ui.heading(RichText::new(title).color(Color32::from_gray(220)));
                    });
                    ui.add_space(8.0);

                    // Progress bar
                    if self.provision_running {
                        let progress = self
                            .provision_status
                            .as_deref()
                            .and_then(parse_provision_progress)
                            .unwrap_or(0.0);
                        ui.horizontal(|ui| {
                            ui.add_space(12.0);
                            ui.add(
                                egui::ProgressBar::new(progress)
                                    .desired_width(ui.available_width() - 24.0)
                                    .text("Preparing host..."),
                            );
                        });
                        ui.add_space(8.0);
                    }

                    // Full-height terminal-style log view
                    Frame::default()
                        .fill(Color32::from_rgb(16, 18, 22))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(50, 55, 65)))
                        .corner_radius(10.0)
                        .inner_margin(14.0)
                        .outer_margin(egui::Margin::symmetric(12, 0))
                        .show(ui, |ui| {
                            ScrollArea::vertical()
                                .max_height(ui.available_height() - 16.0)
                                .stick_to_bottom(true)
                                .show(ui, |ui| {
                                    for line in &self.provision_log {
                                        ui.label(
                                            RichText::new(line)
                                                .size(13.0)
                                                .family(egui::FontFamily::Monospace)
                                                .color(Color32::from_rgb(180, 200, 180)),
                                        );
                                    }
                                });
                        });
                });
                return;
            }

            self.render_selected_instance_overview(ui);
            return;
        };

        let available = ui.available_size();
        let metrics = responsive_terminal_metrics(ui, available, self.terminal_zoom);
        let cols = metrics.cols;
        let rows = metrics.rows;
        let Some((tab_id, kind, snapshot)) = ({
            let Some(tab) = self.active_tab_mut() else {
                return;
            };
            if tab.last_size != (rows, cols) {
                let _ = tab
                    .terminal
                    .resize(rows, cols, available.x as u16, available.y as u16);
                tab.last_size = (rows, cols);
            }
            Some((tab.id.clone(), tab.kind, tab.terminal.snapshot()))
        }) else {
            return;
        };

        let banner = terminal_banner_message(&snapshot);
        if let Some((message, is_error)) = &banner {
            let fill = if *is_error {
                Color32::from_rgb(70, 24, 24)
            } else {
                Color32::from_rgb(20, 47, 66)
            };
            Frame::default()
                .fill(fill)
                .stroke(Stroke::new(1.0, Color32::from_rgb(92, 104, 122)))
                .corner_radius(10.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.label(RichText::new(message).color(Color32::from_rgb(235, 240, 245)));
                });
            ui.add_space(8.0);
        } else if self
            .tabs
            .iter()
            .find(|tab| tab.id == active_tab_id)
            .map(|tab| tab.kind == SessionKind::Tui && snapshot.output.trim().is_empty())
            .unwrap_or(false)
        {
            Frame::default()
                .fill(Color32::from_rgb(20, 47, 66))
                .stroke(Stroke::new(1.0, Color32::from_rgb(92, 104, 122)))
                .corner_radius(10.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("CTOX wird verbunden oder lädt...")
                            .color(Color32::from_rgb(235, 240, 245)),
                    );
                });
            ui.add_space(8.0);
        }

        let frame = Frame::default()
            .fill(Color32::from_rgb(11, 13, 16))
            .stroke(Stroke::new(1.0, Color32::from_rgb(34, 38, 44)))
            .corner_radius(16.0)
            .shadow(egui::epaint::Shadow::NONE)
            .inner_margin(egui::Margin::same(8));

        frame.show(ui, |ui| {
            let desired = ui.available_size_before_wrap();
            let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
            if response.hovered() {
                ui.output_mut(|output| output.cursor_icon = egui::CursorIcon::Default);
            }
            if response.clicked() {
                self.terminal_focus = true;
            }

            if kind == SessionKind::Tui {
                ui.allocate_ui_at_rect(rect, |ui| {
                    ui.set_clip_rect(rect);
                    if let Some((message, _)) = &banner {
                        if !snapshot.modes.alt_screen {
                            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                                ui.add_space((rect.height() * 0.36).max(36.0));
                                ui.label(
                                    RichText::new(message)
                                        .size(16.0)
                                        .color(Color32::from_gray(200)),
                                );
                            });
                            return;
                        }
                    }
                    render_snapshot(
                        ui,
                        &snapshot,
                        true,
                        metrics.font_size,
                        metrics.line_height,
                        metrics.cell_width,
                    );
                });
            } else {
                ui.allocate_ui_at_rect(rect, |ui| {
                    ui.set_clip_rect(rect);
                    ScrollArea::both()
                        .id_salt(Id::new(("terminal-scroll", &tab_id)))
                        .show(ui, |ui| {
                            render_snapshot(
                                ui,
                                &snapshot,
                                false,
                                metrics.font_size,
                                metrics.line_height,
                                metrics.cell_width,
                            )
                        });
                });
            }
        });
    }

    fn render_command_composer(&mut self, ui: &mut Ui) {
        let selected_name = self
            .selected_installation()
            .map(|installation| installation.display_name())
            .unwrap_or_else(|| "No instance".to_owned());
        let remote = self
            .selected_installation()
            .map(|installation| installation.is_remote())
            .unwrap_or(false);

        Frame::default()
            .fill(Color32::from_rgb(53, 50, 50))
            .stroke(Stroke::new(1.0, Color32::from_rgb(65, 62, 62)))
            .corner_radius(24.0)
            .shadow(egui::epaint::Shadow::NONE)
            .inner_margin(egui::Margin::symmetric(14, 8))
            .show(ui, |ui| {
                if !self.composer_attachments.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for path in &self.composer_attachments {
                            Frame::default()
                                .fill(Color32::from_rgb(64, 60, 60))
                                .corner_radius(12.0)
                                .inner_margin(egui::Margin::symmetric(10, 6))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(
                                            path.file_name()
                                                .and_then(|value| value.to_str())
                                                .unwrap_or("file"),
                                        )
                                        .size(12.4),
                                    );
                                });
                        }
                    });
                    ui.add_space(8.0);
                }

                if self.transcript_panel_open {
                    ui.add(
                        TextEdit::multiline(&mut self.composer_transcript)
                            .desired_rows(3)
                            .hint_text("Paste or dictate transcript text here"),
                    );
                    ui.add_space(8.0);
                }

                let response = ui.add_sized(
                    [ui.available_width(), 22.0],
                    TextEdit::multiline(&mut self.custom_command_input)
                        .desired_rows(1)
                        .frame(false)
                        .hint_text("Message CTOX"),
                );
                if response.has_focus() {
                    self.terminal_focus = false;
                }
                let run_via_enter = response.lost_focus()
                    && ui.input(|input| input.key_pressed(Key::Enter) && !input.modifiers.shift);

                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            Button::new(
                                RichText::new("+").size(23.0).color(Color32::from_gray(196)),
                            )
                            .frame(false)
                            .min_size(egui::vec2(22.0, 22.0)),
                        )
                        .clicked()
                    {
                        self.pick_composer_attachments();
                    }
                    ui.add_space(14.0);
                    ui.label(
                        RichText::new(format!(
                            "{} · {}",
                            selected_name,
                            if remote { "Remote" } else { "Local" }
                        ))
                        .size(13.2)
                        .color(Color32::from_gray(188)),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let run_via_button = ui
                            .add_sized(
                                [40.0, 40.0],
                                Button::new(
                                    RichText::new("⬆")
                                        .size(16.0)
                                        .color(Color32::from_rgb(41, 41, 43)),
                                )
                                .fill(Color32::from_rgb(246, 246, 246))
                                .corner_radius(20.0),
                            )
                            .clicked();
                        ui.add_space(8.0);
                        let mic_label = if self.transcript_panel_open {
                            "●"
                        } else {
                            "🎙"
                        };
                        if ui
                            .add(
                                Button::new(
                                    RichText::new(mic_label)
                                        .size(18.0)
                                        .color(Color32::from_gray(150)),
                                )
                                .frame(false)
                                .min_size(egui::vec2(26.0, 26.0)),
                            )
                            .clicked()
                        {
                            self.transcript_panel_open = !self.transcript_panel_open;
                        }
                        if run_via_enter || run_via_button {
                            self.terminal_focus = false;
                            self.send_composer_message();
                        }
                    });
                });
            });
    }

    fn render_command_status_strip(&mut self, ui: &mut Ui) {
        let message = if let Some(run) = &self.command_run {
            Some((
                format!("{} running...", run.title),
                Color32::from_rgb(111, 167, 204),
            ))
        } else if let Some(result) = &self.last_command_result {
            let code = result.exit_code.unwrap_or_default();
            let color = if code == 0 {
                Color32::from_rgb(110, 188, 120)
            } else {
                Color32::from_rgb(218, 106, 106)
            };
            Some((
                format!("{} beendet mit Exit-Code {}", result.title, code),
                color,
            ))
        } else {
            None
        };

        if let Some((message, color)) = message {
            ui.allocate_ui(egui::vec2(ui.available_width(), 28.0), |ui| {
                ui.label(RichText::new(message).size(12.8).color(color));
            });
        }
    }
}

impl eframe::App for CtoxDesktopApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.poll_background_events();
        self.drain_version_signals();
        self.refresh_installation_statuses();
        self.handle_terminal_input(ctx);

        self.render_left_panel(ctx);

        egui::CentralPanel::default()
            .frame(
                Frame::default()
                    .fill(UI_PAPER)
                    .inner_margin(egui::Margin::same(0)),
            )
            .show(ctx, |ui| {
                if self.active_tab_id.is_some() {
                    self.render_tabs(ui);
                    ui.add_space(6.0);
                    self.render_mode_tabs(ui);
                    ui.add_space(8.0);
                }

                let available_width = ui.available_width();
                let remaining_height = ui.available_height();
                let status_height =
                    if self.command_run.is_some() || self.last_command_result.is_some() {
                        30.0
                    } else {
                        0.0
                    };
                let footer_spacing = 8.0;
                let content_height = (remaining_height
                    - status_height
                    - footer_spacing
                    - if status_height > 0.0 { 6.0 } else { 0.0 })
                .max(180.0);
                let origin = ui.next_widget_position();
                let mut cursor_y = origin.y;

                let content_rect = egui::Rect::from_min_size(
                    egui::pos2(origin.x, cursor_y),
                    egui::vec2(available_width, content_height),
                );

                ui.allocate_ui_at_rect(content_rect, |ui| {
                    self.render_terminal_area(ui);
                });
                cursor_y += content_height + footer_spacing;

                if status_height > 0.0 {
                    let status_rect = egui::Rect::from_min_size(
                        egui::pos2(origin.x, cursor_y),
                        egui::vec2(available_width, status_height),
                    );
                    ui.allocate_ui_at_rect(status_rect, |ui| {
                        self.render_command_status_strip(ui);
                    });
                    cursor_y += status_height + 6.0;
                }
            });

        ctx.request_repaint_after(Duration::from_millis(33));
    }
}

impl CtoxDesktopApp {
    fn render_mode_tabs(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("Reset").clicked() {
                    self.terminal_zoom = 1.0;
                }
                ui.label(
                    RichText::new(format!("{:.0}%", self.terminal_zoom * 100.0))
                        .size(12.0)
                        .color(Color32::from_gray(180)),
                );
                if ui.small_button("+").clicked() {
                    self.terminal_zoom =
                        (self.terminal_zoom + 0.05).clamp(TERMINAL_ZOOM_MIN, TERMINAL_ZOOM_MAX);
                }
                if ui.small_button("-").clicked() {
                    self.terminal_zoom =
                        (self.terminal_zoom - 0.05).clamp(TERMINAL_ZOOM_MIN, TERMINAL_ZOOM_MAX);
                }
            });
        });
    }
}

fn render_snapshot(
    ui: &mut Ui,
    snapshot: &TerminalSnapshot,
    trim_blank_lines: bool,
    font_size: f32,
    line_height: f32,
    cell_width: f32,
) {
    if snapshot.styled_lines.is_empty() {
        render_plain_snapshot(
            ui,
            &snapshot.output,
            trim_blank_lines,
            font_size,
            line_height,
        );
        return;
    }

    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        let lines: Vec<_> = if trim_blank_lines || snapshot.modes.alt_screen {
            let start = snapshot
                .styled_lines
                .iter()
                .position(|line| !styled_line_is_blank(line))
                .unwrap_or(0);
            let end = snapshot
                .styled_lines
                .iter()
                .rposition(|line| !styled_line_is_blank(line))
                .map(|index| index + 1)
                .unwrap_or(snapshot.styled_lines.len());
            snapshot.styled_lines[start..end].iter().collect()
        } else {
            snapshot.styled_lines.iter().collect()
        };

        let width = ui.available_width().max(1.0);
        let font_id = FontId::monospace(font_size);
        for line in lines {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(width, line_height), Sense::hover());
            let painter = ui.painter();

            for cell in &line.cells {
                let text_width = (cell.text.chars().count().max(1) as f32) * cell_width;
                let x = rect.min.x + (cell.column as f32 * cell_width);
                if x >= rect.max.x {
                    continue;
                }
                let paint_width = text_width.min(rect.max.x - x).max(cell_width);
                let cell_rect = egui::Rect::from_min_size(
                    egui::pos2(x, rect.min.y),
                    egui::vec2(paint_width, line_height),
                );
                if cell.bg != TERMINAL_DEFAULT_BG {
                    painter.rect_filled(cell_rect, 0.0, rgb_u32_to_color(cell.bg));
                }
                if !cell.text.chars().all(char::is_whitespace) {
                    painter.text(
                        egui::pos2(x, rect.min.y),
                        egui::Align2::LEFT_TOP,
                        &cell.text,
                        font_id.clone(),
                        rgb_u32_to_color(cell.fg),
                    );
                }
            }
        }
    });
}

fn render_plain_snapshot(
    ui: &mut Ui,
    output: &str,
    trim_blank_lines: bool,
    font_size: f32,
    line_height: f32,
) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        let all_lines: Vec<&str> = output.lines().collect();
        let lines: Vec<&str> = if trim_blank_lines {
            let start = all_lines
                .iter()
                .position(|line| !line.trim().is_empty())
                .unwrap_or(0);
            let end = all_lines
                .iter()
                .rposition(|line| !line.trim().is_empty())
                .map(|idx| idx + 1)
                .unwrap_or(all_lines.len());
            all_lines[start..end].to_vec()
        } else {
            all_lines
        };

        let width = ui.available_width().max(1.0);
        for line in lines {
            let galley = ui.painter().layout_no_wrap(
                line.to_owned(),
                FontId::monospace(font_size),
                Color32::from_gray(220),
            );
            let (rect, _) = ui.allocate_exact_size(egui::vec2(width, line_height), Sense::hover());
            ui.painter()
                .galley(rect.min, galley, Color32::from_gray(220));
        }
    });
}

fn styled_line_is_blank(line: &crate::terminal_emulator::TerminalStyledLine) -> bool {
    line.cells.is_empty()
        || line
            .cells
            .iter()
            .all(|cell| cell.bg == TERMINAL_DEFAULT_BG && cell.text.trim().is_empty())
}

fn tail_text(text: &str, line_count: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(line_count);
    let tail = lines[start..].join("\n");
    if tail.trim().is_empty() {
        "No output yet.".to_owned()
    } else {
        tail
    }
}

fn render_command_feedback_box(
    ui: &mut Ui,
    label: &str,
    title: &str,
    accent: Color32,
    output: &str,
) {
    Frame::default()
        .fill(Color32::from_rgb(25, 28, 33))
        .stroke(Stroke::new(1.0, Color32::from_rgb(42, 47, 55)))
        .corner_radius(12.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.label(RichText::new(label).size(12.0).color(accent));
            ui.label(RichText::new(title).size(14.0).strong());
            ui.add_space(6.0);
            ui.monospace(output);
        });
}

fn parse_provision_progress(message: &str) -> Option<f32> {
    let trimmed = message.trim();
    let rest = trimmed.strip_prefix('[')?;
    let (current, rest) = rest.split_once('/')?;
    let (total, _) = rest.split_once(']')?;
    let current = current.trim().parse::<f32>().ok()?;
    let total = total.trim().parse::<f32>().ok()?;
    if total <= 0.0 {
        return None;
    }
    Some((current / total).clamp(0.0, 1.0))
}

fn apply_theme(ctx: &Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 4.0);
    style.spacing.menu_margin = egui::Margin::same(8);
    style.text_styles.insert(
        egui::TextStyle::Heading,
        FontId::new(20.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        FontId::new(15.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        FontId::new(15.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        FontId::new(13.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Monospace,
        FontId::new(13.2, FontFamily::Monospace),
    );

    let mut visuals = Visuals::light();
    visuals.override_text_color = Some(UI_TEXT);
    visuals.panel_fill = UI_PAPER;
    visuals.window_fill = UI_PAPER;
    visuals.faint_bg_color = UI_RAIL;
    visuals.extreme_bg_color = Color32::from_rgb(232, 232, 228);
    visuals.code_bg_color = Color32::from_rgb(248, 248, 246);
    visuals.window_corner_radius = 3.into();
    visuals.menu_corner_radius = 3.into();
    visuals.window_shadow = egui::epaint::Shadow::NONE;
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 4],
        blur: 12,
        spread: 0,
        color: Color32::from_black_alpha(24),
    };
    visuals.widgets.noninteractive.bg_fill = UI_PAPER;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, UI_LINE_SOFT);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(246, 246, 244);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(246, 246, 244);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(183, 183, 178));
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(238, 238, 235);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(238, 238, 235);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(142, 142, 137));
    visuals.widgets.active.bg_fill = UI_BLUE;
    visuals.widgets.active.weak_bg_fill = UI_BLUE;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, Color32::from_rgb(25, 118, 201));
    visuals.widgets.open.bg_fill = Color32::from_rgb(236, 236, 234);
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, UI_LINE);
    visuals.selection.bg_fill = UI_BLUE;
    visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(18, 111, 192));
    visuals.hyperlink_color = UI_BLUE;

    style.visuals = visuals;
    ctx.set_style(style);
}

fn rgb_u32_to_color(value: u32) -> Color32 {
    Color32::from_rgb(
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
    )
}

fn add_choice_button(ui: &mut Ui, title: &str, subtitle: &str, on_click: impl FnOnce()) {
    let clicked = Frame::default()
        .fill(Color32::from_rgb(29, 33, 38))
        .stroke(Stroke::new(1.0, Color32::from_rgb(48, 55, 64)))
        .corner_radius(7.0)
        .inner_margin(egui::Margin::symmetric(10, 9))
        .show(ui, |ui| {
            let response = ui
                .horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(title).size(13.5).strong());
                        ui.label(
                            RichText::new(subtitle)
                                .size(11.5)
                                .color(Color32::from_gray(145)),
                        );
                    });
                })
                .response;
            response.interact(Sense::click()).clicked()
        })
        .inner;
    if clicked {
        on_click();
    }
}

fn add_form_input(ui: &mut Ui, label: &str, hint: &str, value: &mut String, password: bool) {
    ui.label(
        RichText::new(label)
            .size(12.0)
            .color(Color32::from_gray(150)),
    );
    let mut edit = TextEdit::singleline(value).hint_text(hint);
    if password {
        edit = edit.password(true);
    }
    ui.add_sized([ui.available_width(), 28.0], edit);
}

fn settings_row(ui: &mut Ui, label: &str, body: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [190.0, 42.0],
            egui::Label::new(RichText::new(label).size(16.0).strong().color(UI_TEXT)),
        );
        ui.vertical(|ui| {
            ui.set_min_width(460.0);
            body(ui);
        });
    });
    ui.add_space(12.0);
    ui.separator();
    ui.add_space(12.0);
}

fn settings_display_row(
    ui: &mut Ui,
    label: &str,
    value: &str,
    action: &str,
    on_action: impl FnOnce(),
) {
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.add_sized(
            [180.0, 34.0],
            egui::Label::new(RichText::new(label).size(16.0).strong().color(UI_TEXT)),
        );
        ui.add_sized(
            [ui.available_width().max(0.0) - 116.0, 34.0],
            egui::Label::new(RichText::new(value).size(16.0).color(UI_TEXT)),
        );
        if ui.add_sized([96.0, 32.0], Button::new(action)).clicked() {
            clicked = true;
        }
    });
    ui.add_space(10.0);
    ui.separator();
    ui.add_space(10.0);
    if clicked {
        on_action();
    }
}

fn render_settings_section_label(ui: &mut Ui, label: &str) {
    ui.label(
        RichText::new(label)
            .size(12.8)
            .strong()
            .color(Color32::from_gray(175)),
    );
    ui.add_space(5.0);
}

fn render_settings_hint(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(11.8)
            .color(Color32::from_gray(140)),
    );
}

fn non_empty_or<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() {
        fallback
    } else {
        value
    }
}

fn non_empty_opt(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn default_new_instance_draft() -> NewInstanceDraft {
    NewInstanceDraft {
        local: true,
        ssh_user: "ubuntu".to_owned(),
        ..Default::default()
    }
}

fn shellexpand_tilde(value: &str) -> String {
    if value == "~" {
        dirs::home_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| value.to_owned())
    } else if let Some(rest) = value.strip_prefix("~/") {
        dirs::home_dir()
            .map(|path| path.join(rest).display().to_string())
            .unwrap_or_else(|| value.to_owned())
    } else {
        value.to_owned()
    }
}

fn installation_role_label(
    is_remote: bool,
    admin_unlocked: bool,
    version: Option<&str>,
) -> &'static str {
    if admin_unlocked {
        "Admin"
    } else if version
        .map(|value| value.to_ascii_lowercase().contains("chef"))
        .unwrap_or(false)
    {
        "Chef"
    } else if version
        .map(|value| value.to_ascii_lowercase().contains("founder"))
        .unwrap_or(false)
    {
        "Founder"
    } else if is_remote {
        "User"
    } else {
        "User"
    }
}

fn role_color(role: &str) -> Color32 {
    match role {
        "Admin" => Color32::from_rgb(226, 184, 93),
        "Chef" => Color32::from_rgb(129, 184, 226),
        "Founder" => Color32::from_rgb(170, 139, 222),
        _ => Color32::from_gray(140),
    }
}

fn draw_status_dot(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.5, color);
}

fn draw_instance_logo(ui: &mut Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), Sense::hover());
    let painter = ui.painter();
    let bg = Color32::from_rgb(238, 238, 232);
    let accent = Color32::from_rgb(236, 162, 30);
    let base = Color32::from_rgb(125, 126, 126);
    painter.rect_filled(rect, 4.0, bg);
    let screen = rect.shrink(size * 0.18);
    painter.rect_filled(screen, 2.0, Color32::from_rgb(250, 250, 247));
    painter.line_segment(
        [
            screen.left_top() + egui::vec2(3.0, screen.height() - 4.0),
            screen.right_top() + egui::vec2(-3.0, 3.0),
        ],
        Stroke::new(4.0, accent),
    );
    let stand_top = rect.center_bottom() + egui::vec2(0.0, -5.0);
    painter.line_segment(
        [stand_top, stand_top + egui::vec2(0.0, 5.0)],
        Stroke::new(3.0, base),
    );
    painter.line_segment(
        [
            rect.center_bottom() + egui::vec2(-8.0, -1.0),
            rect.center_bottom() + egui::vec2(8.0, -1.0),
        ],
        Stroke::new(3.0, base),
    );
}

fn looks_like_signaling_url(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("ws://")
        || lower.starts_with("wss://")
        || lower.contains("/signal")
        || lower.contains("signaling")
}

fn normalize_business_os_endpoint(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_owned()
    } else {
        format!("http://{trimmed}")
    }
}

fn key_event_to_bytes(key: Key, modifiers: egui::Modifiers) -> Option<Vec<u8>> {
    if modifiers.command {
        return None;
    }

    if modifiers.ctrl {
        if let Some(letter) = key_to_ctrl_letter(key) {
            return Some(vec![letter as u8]);
        }
    }

    let bytes = match key {
        Key::Enter => b"\r".to_vec(),
        Key::Tab => b"\t".to_vec(),
        Key::Backspace => vec![0x7f],
        Key::Escape => vec![0x1b],
        Key::ArrowUp => b"\x1b[A".to_vec(),
        Key::ArrowDown => b"\x1b[B".to_vec(),
        Key::ArrowRight => b"\x1b[C".to_vec(),
        Key::ArrowLeft => b"\x1b[D".to_vec(),
        Key::Home => b"\x1b[H".to_vec(),
        Key::End => b"\x1b[F".to_vec(),
        Key::PageUp => b"\x1b[5~".to_vec(),
        Key::PageDown => b"\x1b[6~".to_vec(),
        Key::Delete => b"\x1b[3~".to_vec(),
        _ => return None,
    };
    Some(bytes)
}

fn key_to_ctrl_letter(key: Key) -> Option<char> {
    match key {
        Key::A => Some('\u{01}'),
        Key::B => Some('\u{02}'),
        Key::C => Some('\u{03}'),
        Key::D => Some('\u{04}'),
        Key::E => Some('\u{05}'),
        Key::F => Some('\u{06}'),
        Key::G => Some('\u{07}'),
        Key::H => Some('\u{08}'),
        Key::I => Some('\u{09}'),
        Key::J => Some('\u{0a}'),
        Key::K => Some('\u{0b}'),
        Key::L => Some('\u{0c}'),
        Key::M => Some('\u{0d}'),
        Key::N => Some('\u{0e}'),
        Key::O => Some('\u{0f}'),
        Key::P => Some('\u{10}'),
        Key::Q => Some('\u{11}'),
        Key::R => Some('\u{12}'),
        Key::S => Some('\u{13}'),
        Key::T => Some('\u{14}'),
        Key::U => Some('\u{15}'),
        Key::V => Some('\u{16}'),
        Key::W => Some('\u{17}'),
        Key::X => Some('\u{18}'),
        Key::Y => Some('\u{19}'),
        Key::Z => Some('\u{1a}'),
        _ => None,
    }
}

fn installation_ready_for_tui(installation: &Installation) -> bool {
    match installation.mode {
        InstallationMode::Local => installation.root_path.is_some(),
        InstallationMode::RemoteWebRtc => true,
    }
}

fn terminal_banner_message(snapshot: &TerminalSnapshot) -> Option<(String, bool)> {
    let recent_lines: Vec<&str> = snapshot
        .output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .rev()
        .take(16)
        .collect();

    if recent_lines.is_empty() {
        return snapshot
            .exit_code
            .filter(|code| *code != 0)
            .map(|code| (humanize_exit_code(code), true));
    }

    let mut message = String::new();
    let mut raw_exit_message = String::new();
    let mut previous = "";
    for line in recent_lines.into_iter().rev() {
        if line == previous {
            continue;
        }
        previous = line;
        if is_raw_exit_marker(line) {
            raw_exit_message = line.to_owned();
            continue;
        }
        message = line.to_owned();
    }
    if message.is_empty() {
        if !raw_exit_message.is_empty() {
            return snapshot
                .exit_code
                .filter(|code| *code != 0)
                .map(|code| (humanize_exit_code(code), true))
                .or(Some((raw_exit_message, true)));
        }
        return snapshot
            .exit_code
            .filter(|code| *code != 0)
            .map(|code| (humanize_exit_code(code), true));
    }
    let lower = message.to_lowercase();

    let is_error = snapshot.exit_code.unwrap_or(0) != 0
        || lower.contains("fehlgeschlagen")
        || lower.contains("bitte")
        || lower.contains("keine antwort")
        || lower.contains("abgelehnt")
        || lower.contains("beendet")
        || lower.contains("zurueckgesetzt")
        || lower.contains("zurückgesetzt")
        || lower.contains("error");
    let is_status = lower.contains("verbinde")
        || lower.contains("warte")
        || lower.contains("verbunden")
        || is_error;

    is_status.then_some((message, is_error))
}

fn is_raw_exit_marker(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.starts_with("[session exited with code")
        || lower.starts_with("[session failed to wait for process exit:")
}

fn humanize_exit_code(code: i32) -> String {
    match code {
        101 => "CTOX crashed while starting. Check the error details shown for the actual cause."
            .to_owned(),
        0 => "Session closed.".to_owned(),
        _ => format!("The session stopped unexpectedly (exit code {code})."),
    }
}

struct ResponsiveTerminalMetrics {
    cols: u16,
    rows: u16,
    font_size: f32,
    line_height: f32,
    cell_width: f32,
}

fn responsive_terminal_metrics(
    ui: &Ui,
    available: egui::Vec2,
    zoom: f32,
) -> ResponsiveTerminalMetrics {
    let zoom = zoom.clamp(TERMINAL_ZOOM_MIN, TERMINAL_ZOOM_MAX);
    let font_size = (8.9 * zoom).clamp(TERMINAL_FONT_SIZE_MIN, TERMINAL_FONT_SIZE_MAX);
    let font_id = FontId::monospace(font_size);
    let sample = ui.painter().layout_no_wrap(
        "MMMMMMMMMMMMMMMM".to_owned(),
        font_id.clone(),
        Color32::WHITE,
    );
    let line_height = (font_size * 1.12).max(9.2);
    let cell_width = (sample.size().x / 16.0).max(5.0);
    let cols = (available.x / cell_width).floor().max(20.0) as u16;
    let rows = (available.y / line_height).floor().max(18.0) as u16;
    ResponsiveTerminalMetrics {
        cols,
        rows,
        font_size,
        line_height,
        cell_width,
    }
}

fn composer_runtime_label(model: &str, preset: &str) -> String {
    if model.eq_ignore_ascii_case("GPT-5.4") {
        let reasoning = match preset {
            "Performance" => "Reasoning: low",
            _ => "Reasoning: high",
        };
        format!("{preset} ({reasoning})")
    } else {
        preset.to_owned()
    }
}

fn default_installation_runtime_status(installation: &Installation) -> InstallationRuntimeStatus {
    if installation.is_remote() {
        let remote = &installation.remote;
        if remote.ssh_host.trim().is_empty() || remote.ssh_user.trim().is_empty() {
            return InstallationRuntimeStatus {
                label: "Needs SSH".to_owned(),
                color: Color32::from_rgb(185, 152, 82),
            };
        }
        if installation.cached_version.as_deref().is_some() {
            return InstallationRuntimeStatus {
                label: "SSH reachable".to_owned(),
                color: Color32::from_rgb(99, 184, 123),
            };
        }
        InstallationRuntimeStatus {
            label: "Checking SSH".to_owned(),
            color: Color32::from_rgb(185, 152, 82),
        }
    } else if installation.resolved_binary().is_none() {
        InstallationRuntimeStatus {
            label: "Build required".to_owned(),
            color: Color32::from_rgb(185, 152, 82),
        }
    } else {
        InstallationRuntimeStatus {
            label: "Ready".to_owned(),
            color: Color32::from_rgb(136, 144, 155),
        }
    }
}

fn poll_local_installation_status(
    installation: &Installation,
) -> Result<InstallationRuntimeStatus> {
    let launch = installation.command_launch_target(&["status"])?;
    let output = Command::new(&launch.program)
        .args(&launch.args)
        .current_dir(&launch.cwd)
        .envs(&launch.env)
        .output()?;

    if !output.status.success() {
        return Ok(InstallationRuntimeStatus {
            label: "Error".to_owned(),
            color: Color32::from_rgb(208, 113, 113),
        });
    }

    let parsed: DesktopServiceStatusSnapshot = serde_json::from_slice(&output.stdout)?;
    let (label, color) = if parsed.running && parsed.busy {
        ("Busy".to_owned(), Color32::from_rgb(185, 152, 82))
    } else if parsed.running {
        ("Running".to_owned(), Color32::from_rgb(99, 184, 123))
    } else if parsed
        .last_error
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .is_some()
    {
        ("Error".to_owned(), Color32::from_rgb(208, 113, 113))
    } else {
        ("Stopped".to_owned(), Color32::from_rgb(141, 154, 166))
    };

    Ok(InstallationRuntimeStatus { label, color })
}

fn remote_installation_runtime_status(
    tabs: &[DesktopTab],
    installation_id: &str,
) -> Option<InstallationRuntimeStatus> {
    let tab = tabs
        .iter()
        .find(|tab| tab.kind == SessionKind::Tui && tab.installation_id == installation_id)?;
    let snapshot = tab.terminal.snapshot();

    if snapshot.exit_code.unwrap_or(0) != 0 {
        return Some(InstallationRuntimeStatus {
            label: "Error".to_owned(),
            color: Color32::from_rgb(208, 113, 113),
        });
    }

    let output = snapshot.output.to_lowercase();
    if output.contains("no host online in this room yet") {
        return Some(InstallationRuntimeStatus {
            label: "Offline".to_owned(),
            color: Color32::from_rgb(136, 144, 155),
        });
    }

    if output.contains("connecting to server")
        || output.contains("waiting for host")
        || output.contains("host found")
        || output.contains("host answered")
        || output.contains("opening session")
        || output.contains("checking network path")
        || output.contains("building webrtc connection")
    {
        return Some(InstallationRuntimeStatus {
            label: "Connecting...".to_owned(),
            color: Color32::from_rgb(185, 152, 82),
        });
    }

    if output.contains("ice/turn connection failed")
        || output.contains("webrtc setup failed")
        || output.contains("websocket closed")
        || output.contains("connection closed")
        || output.contains("disconnected")
        || output.contains("rejected")
    {
        return Some(InstallationRuntimeStatus {
            label: "Error".to_owned(),
            color: Color32::from_rgb(208, 113, 113),
        });
    }

    if snapshot.output.trim().is_empty() {
        return Some(InstallationRuntimeStatus {
            label: "Connecting...".to_owned(),
            color: Color32::from_rgb(185, 152, 82),
        });
    }

    Some(InstallationRuntimeStatus {
        label: "Online".to_owned(),
        color: Color32::from_rgb(99, 184, 123),
    })
}

fn parse_extra_args(value: &str) -> Vec<String> {
    value.split_whitespace().map(ToOwned::to_owned).collect()
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone)]
struct LocalCtoxDiscovery {
    root: PathBuf,
    binary: Option<PathBuf>,
}

fn discover_running_local_ctox() -> Option<LocalCtoxDiscovery> {
    let output = Command::new("lsof")
        .args(["-a", "-c", "ctox", "-d", "cwd,txt", "-Fn"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut current_pid: Option<String> = None;
    let mut process_paths: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for line in text.lines() {
        if let Some(pid) = line.strip_prefix('p') {
            current_pid = Some(pid.to_owned());
            process_paths.entry(pid.to_owned()).or_default();
        } else if let (Some(pid), Some(path)) = (&current_pid, line.strip_prefix('n')) {
            process_paths
                .entry(pid.clone())
                .or_default()
                .push(PathBuf::from(path));
        }
    }

    process_paths.into_values().find_map(|paths| {
        let root = paths
            .iter()
            .find(|path| path.join("Cargo.toml").is_file())?;
        let binary = paths
            .iter()
            .find(|path| {
                path.file_name()
                    .and_then(|value| value.to_str())
                    .is_some_and(|name| name == "ctox" || name == "ctox-real")
            })
            .cloned();
        Some(LocalCtoxDiscovery {
            root: root.clone(),
            binary,
        })
    })
}

enum VersionProbeKind {
    Local {
        binary: PathBuf,
    },
    Ssh {
        user: String,
        host: String,
        port: u16,
        password: String,
    },
}

fn installation_probe_kind(installation: &Installation) -> Option<VersionProbeKind> {
    match installation.mode {
        InstallationMode::Local => {
            let binary = installation
                .preferred_binary
                .clone()
                .or_else(|| {
                    installation
                        .env
                        .get("CTOX_BIN_DIR")
                        .filter(|value| !value.trim().is_empty())
                        .map(|value| PathBuf::from(value).join("ctox"))
                })
                .or_else(|| {
                    installation
                        .root_path
                        .as_ref()
                        .map(|root| root.join("bin/ctox"))
                })?;
            Some(VersionProbeKind::Local { binary })
        }
        InstallationMode::RemoteWebRtc => {
            let remote = &installation.remote;
            if remote.ssh_host.trim().is_empty()
                || remote.ssh_user.trim().is_empty()
                || remote.ssh_password.trim().is_empty()
            {
                return None;
            }
            Some(VersionProbeKind::Ssh {
                user: remote.ssh_user.clone(),
                host: remote.ssh_host.clone(),
                port: remote.ssh_port,
                password: remote.ssh_password.clone(),
            })
        }
    }
}

fn business_os_url_for_installation(installation: &Installation) -> Option<String> {
    installation
        .env
        .get("CTOX_BUSINESS_OS_URL")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

const BUSINESS_OS_CANDIDATE_PORTS: &[u16] = &[8765, 9876, 3000];

fn first_reachable_business_os_url(urls: impl IntoIterator<Item = String>) -> Option<String> {
    urls.into_iter()
        .find(|url| verify_direct_business_os_endpoint(url).is_ok())
}

fn local_running_business_os_url() -> Option<String> {
    first_reachable_business_os_url(
        BUSINESS_OS_CANDIDATE_PORTS
            .iter()
            .map(|port| format!("http://127.0.0.1:{port}")),
    )
}

fn remote_public_business_os_url(host: &str) -> Option<String> {
    let host = host.trim();
    if host.is_empty() {
        return None;
    }
    first_reachable_business_os_url(
        BUSINESS_OS_CANDIDATE_PORTS
            .iter()
            .map(|port| format!("http://{host}:{port}")),
    )
    .or_else(|| first_reachable_business_os_url([format!("http://{host}")]))
}

fn start_business_os_proxy(installation: &Installation) -> Result<BusinessOsProxySession> {
    match installation.mode {
        InstallationMode::Local => start_local_business_os_proxy(installation),
        InstallationMode::RemoteWebRtc => match installation.remote.host_target {
            RemoteHostTarget::Localhost => start_localhost_remote_business_os_proxy(installation),
            RemoteHostTarget::Ssh => start_ssh_business_os_proxy(installation),
            RemoteHostTarget::Unspecified => anyhow::bail!(
                "Business OS proxy needs either a local installation or SSH host settings."
            ),
        },
    }
}

fn start_local_business_os_proxy(installation: &Installation) -> Result<BusinessOsProxySession> {
    if let Some(url) = local_running_business_os_url() {
        return Ok(BusinessOsProxySession {
            url,
            tunnel: None,
            server: None,
        });
    }

    let help_launch = installation.command_launch_target(&["business-os", "--help"])?;
    ensure_business_os_serve_supported_local(&help_launch)?;

    let local_port = find_free_local_port()?;
    let addr = format!("127.0.0.1:{local_port}");
    let launch = installation.command_launch_target(&["business-os", "serve", "--addr", &addr])?;
    let mut server = spawn_detached_process(
        &launch.program,
        &launch.args,
        Some(&launch.cwd),
        &launch.env,
    )
    .context("failed to start local Business OS server")?;
    if let Err(error) = wait_for_local_port(local_port, Duration::from_secs(4)) {
        let _ = server.kill();
        return Err(error);
    }
    Ok(BusinessOsProxySession {
        url: format!("http://{addr}"),
        tunnel: None,
        server: Some(server),
    })
}

fn start_localhost_remote_business_os_proxy(
    installation: &Installation,
) -> Result<BusinessOsProxySession> {
    let local_port = find_free_local_port()?;
    let addr = format!("127.0.0.1:{local_port}");
    let root = expand_local_path(&installation.remote.install_root)?;
    let program = root
        .join("bin/ctox")
        .is_file()
        .then(|| root.join("bin/ctox").display().to_string())
        .unwrap_or_else(|| "ctox".to_owned());
    ensure_business_os_serve_supported_command(&program, Some(&root), &BTreeMap::new())?;
    let args = vec![
        "business-os".to_owned(),
        "serve".to_owned(),
        "--addr".to_owned(),
        addr.clone(),
    ];
    let mut server = spawn_detached_process(&program, &args, Some(&root), &BTreeMap::new())
        .context("failed to start local-host Business OS server")?;
    if let Err(error) = wait_for_local_port(local_port, Duration::from_secs(4)) {
        let _ = server.kill();
        return Err(error);
    }
    Ok(BusinessOsProxySession {
        url: format!("http://{addr}"),
        tunnel: None,
        server: Some(server),
    })
}

fn start_ssh_business_os_proxy(installation: &Installation) -> Result<BusinessOsProxySession> {
    let remote = &installation.remote;
    if remote.ssh_host.trim().is_empty()
        || remote.ssh_user.trim().is_empty()
        || remote.ssh_password.trim().is_empty()
    {
        anyhow::bail!("Business OS proxy over SSH needs host, user, and SSH password.");
    }

    if let Some(url) = remote_public_business_os_url(&remote.ssh_host) {
        return Ok(BusinessOsProxySession {
            url,
            tunnel: None,
            server: None,
        });
    }

    let local_port = find_free_local_port()?;
    let remote_port = 8765_u16;
    let remote_root = remote_path_expr(&remote.install_root);
    let target = format!("{}@{}", remote.ssh_user.trim(), remote.ssh_host.trim());
    if let Some(proxy) = start_ssh_tunnel_to_running_business_os(remote, &target)? {
        return Ok(proxy);
    }

    ensure_business_os_serve_supported_ssh(remote, &remote_root, &target)?;
    let serve_command = format!(
        "cd {remote_root} && PATH=\"$PWD/bin:$HOME/.local/bin:$HOME/.local/lib/ctox/current/bin:$PATH\" nohup ctox business-os serve --addr 127.0.0.1:{remote_port} >/tmp/ctox-business-os.log 2>&1 &"
    );

    let start_status = Command::new("sshpass")
        .arg("-p")
        .arg(remote.ssh_password.trim())
        .arg("ssh")
        .arg("-o")
        .arg("StrictHostKeyChecking=no")
        .arg("-p")
        .arg(remote.ssh_port.to_string())
        .arg(&target)
        .arg(serve_command)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to start remote Business OS server over SSH")?;
    if !start_status.success() {
        anyhow::bail!("remote Business OS server start failed with {start_status}");
    }

    let mut tunnel = Command::new("sshpass")
        .arg("-p")
        .arg(remote.ssh_password.trim())
        .arg("ssh")
        .arg("-N")
        .arg("-o")
        .arg("StrictHostKeyChecking=no")
        .arg("-o")
        .arg("ExitOnForwardFailure=yes")
        .arg("-L")
        .arg(format!("127.0.0.1:{local_port}:127.0.0.1:{remote_port}"))
        .arg("-p")
        .arg(remote.ssh_port.to_string())
        .arg(&target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to open local Business OS SSH tunnel")?;
    if let Err(error) = wait_for_local_port(local_port, Duration::from_secs(6)) {
        let _ = tunnel.kill();
        return Err(error);
    }

    Ok(BusinessOsProxySession {
        url: format!("http://127.0.0.1:{local_port}"),
        tunnel: Some(tunnel),
        server: None,
    })
}

fn start_ssh_tunnel_to_running_business_os(
    remote: &RemoteAccessSettings,
    target: &str,
) -> Result<Option<BusinessOsProxySession>> {
    for remote_port in BUSINESS_OS_CANDIDATE_PORTS {
        let probe_command = format!(
            "bash -lc {}",
            shell_quote(&format!("</dev/tcp/127.0.0.1/{remote_port}"))
        );
        let reachable = Command::new("sshpass")
            .arg("-p")
            .arg(remote.ssh_password.trim())
            .arg("ssh")
            .arg("-o")
            .arg("StrictHostKeyChecking=no")
            .arg("-o")
            .arg("ConnectTimeout=5")
            .arg("-p")
            .arg(remote.ssh_port.to_string())
            .arg(target)
            .arg(probe_command)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if !reachable {
            continue;
        }

        let local_port = find_free_local_port()?;
        let mut tunnel = Command::new("sshpass")
            .arg("-p")
            .arg(remote.ssh_password.trim())
            .arg("ssh")
            .arg("-N")
            .arg("-o")
            .arg("StrictHostKeyChecking=no")
            .arg("-o")
            .arg("ExitOnForwardFailure=yes")
            .arg("-L")
            .arg(format!("127.0.0.1:{local_port}:127.0.0.1:{remote_port}"))
            .arg("-p")
            .arg(remote.ssh_port.to_string())
            .arg(target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to open local Business OS SSH tunnel")?;
        if wait_for_local_port(local_port, Duration::from_secs(6)).is_ok() {
            return Ok(Some(BusinessOsProxySession {
                url: format!("http://127.0.0.1:{local_port}"),
                tunnel: Some(tunnel),
                server: None,
            }));
        }
        let _ = tunnel.kill();
    }

    Ok(None)
}

fn business_os_serve_unavailable_message() -> &'static str {
    "Diese CTOX Installation kann das Business OS noch nicht als Desktop-Proxy starten. Öffne eine konfigurierte Business-OS URL oder aktualisiere die Instanz auf eine Version mit `ctox business-os serve`."
}

fn ensure_business_os_serve_supported_local(launch: &LaunchTarget) -> Result<()> {
    ensure_business_os_serve_supported_command(&launch.program, Some(&launch.cwd), &launch.env)
}

fn ensure_business_os_serve_supported_command(
    program: &str,
    cwd: Option<&std::path::Path>,
    env: &BTreeMap<String, String>,
) -> Result<()> {
    let mut command = Command::new(program);
    command.args(["business-os", "--help"]);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    for (key, value) in env {
        command.env(key, value);
    }
    let output = command
        .stdin(Stdio::null())
        .output()
        .context("failed to check Business OS support")?;
    let help = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if help.contains("business-os serve") {
        return Ok(());
    }
    anyhow::bail!("{}", business_os_serve_unavailable_message());
}

fn ensure_business_os_serve_supported_ssh(
    remote: &RemoteAccessSettings,
    remote_root: &str,
    target: &str,
) -> Result<()> {
    let help_command = format!(
        "cd {remote_root} && PATH=\"$PWD/bin:$HOME/.local/bin:$HOME/.local/lib/ctox/current/bin:$PATH\" ctox business-os --help 2>&1"
    );
    let output = Command::new("sshpass")
        .arg("-p")
        .arg(remote.ssh_password.trim())
        .arg("ssh")
        .arg("-o")
        .arg("StrictHostKeyChecking=no")
        .arg("-o")
        .arg("ConnectTimeout=8")
        .arg("-p")
        .arg(remote.ssh_port.to_string())
        .arg(target)
        .arg(help_command)
        .stdin(Stdio::null())
        .output()
        .context("failed to check remote Business OS support over SSH")?;
    let help = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if help.contains("business-os serve") {
        return Ok(());
    }
    anyhow::bail!("{}", business_os_serve_unavailable_message());
}

fn ensure_remote_tui_host_started(installation: &Installation) -> Result<()> {
    if installation.mode != InstallationMode::RemoteWebRtc
        || installation.remote.host_target != RemoteHostTarget::Ssh
    {
        return Ok(());
    }

    let remote = &installation.remote;
    if remote.ssh_host.trim().is_empty()
        || remote.ssh_user.trim().is_empty()
        || remote.ssh_password.trim().is_empty()
    {
        anyhow::bail!("Remote TUI over SSH needs host, user, and SSH password.");
    }

    let signal_url = remote
        .signaling_urls
        .iter()
        .find(|value| !value.trim().is_empty())
        .map(String::as_str)
        .unwrap_or("wss://api.metricspace.org/signal");
    if remote.room_id.trim().is_empty() || remote.password.trim().is_empty() {
        anyhow::bail!("Remote TUI needs a room and admin/WebRTC password.");
    }

    let remote_root = remote_path_expr(&remote.install_root);
    let target = format!("{}@{}", remote.ssh_user.trim(), remote.ssh_host.trim());
    let host_name = installation.display_name();
    let remote_cmd = format!(
        "PATH=\"$HOME/.local/bin:$HOME/.local/lib/ctox/current/bin:$PATH\"; \
         pkill -x ctox-desktop-host 2>/dev/null || true; \
         sleep 1; \
         nohup ctox-desktop-host \
           --root {root} \
           --signal {signal} \
           --token {token} \
           --password {password} \
           --room {room} \
           --name {name} \
           >/tmp/ctox-desktop-host.log 2>&1 &",
        root = remote_root,
        signal = shell_quote(signal_url),
        token = shell_quote(remote.auth_token.trim()),
        password = shell_quote(remote.password.trim()),
        room = shell_quote(remote.room_id.trim()),
        name = shell_quote(&host_name),
    );

    let status = Command::new("sshpass")
        .arg("-p")
        .arg(remote.ssh_password.trim())
        .arg("ssh")
        .arg("-o")
        .arg("StrictHostKeyChecking=no")
        .arg("-o")
        .arg("ConnectTimeout=8")
        .arg("-p")
        .arg(remote.ssh_port.to_string())
        .arg(&target)
        .arg(remote_cmd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to start remote CTOX TUI host over SSH")?;
    if !status.success() {
        anyhow::bail!("remote CTOX TUI host start failed with {status}");
    }
    Ok(())
}

fn spawn_detached_process(
    program: &str,
    args: &[String],
    cwd: Option<&std::path::Path>,
    env: &BTreeMap<String, String>,
) -> Result<Child> {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    for (key, value) in env {
        command.env(key, value);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(Into::into)
}

fn find_free_local_port() -> Result<u16> {
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").context("failed to allocate a local port")?;
    Ok(listener.local_addr()?.port())
}

fn wait_for_local_port(port: u16, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(120));
    }
    anyhow::bail!("Business OS proxy did not become reachable on 127.0.0.1:{port}");
}

fn expand_local_path(raw: &str) -> Result<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        anyhow::bail!("missing local CTOX root");
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        let home = dirs::home_dir().context("home directory is not available")?;
        return Ok(home.join(rest));
    }
    Ok(PathBuf::from(trimmed))
}

fn remote_path_expr(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("~/") {
        format!("$HOME/{}", rest)
    } else {
        shell_quote(trimmed)
    }
}

fn shell_quote(value: &str) -> String {
    let escaped = value.replace('\'', "'\"'\"'");
    format!("'{escaped}'")
}

fn verify_direct_business_os_endpoint(url: &str) -> Result<()> {
    let (host, port) = endpoint_host_port(url)?;
    let mut addrs = (host.as_str(), port)
        .to_socket_addrs()
        .with_context(|| format!("{host}:{port}"))?;
    let Some(addr) = addrs.next() else {
        anyhow::bail!("{host}:{port}");
    };
    std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(4))
        .with_context(|| format!("{host}:{port}"))?;
    Ok(())
}

fn endpoint_host_port(raw: &str) -> Result<(String, u16)> {
    let trimmed = raw.trim();
    anyhow::ensure!(!trimmed.is_empty(), "URL");
    let (scheme, rest) = trimmed
        .split_once("://")
        .map(|(scheme, rest)| (scheme.to_ascii_lowercase(), rest))
        .unwrap_or_else(|| ("http".to_owned(), trimmed));
    let authority = rest.split('/').next().unwrap_or(rest).trim();
    anyhow::ensure!(!authority.is_empty(), "URL");
    if let Some(after_bracket) = authority.strip_prefix('[') {
        let (host, rest) = after_bracket.split_once(']').context("IPv6")?;
        let port = rest
            .strip_prefix(':')
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or_else(|| default_port_for_scheme(&scheme));
        return Ok((host.to_owned(), port));
    }
    let (host, port) = authority
        .rsplit_once(':')
        .and_then(|(host, port)| port.parse::<u16>().ok().map(|port| (host, port)))
        .map(|(host, port)| (host.to_owned(), port))
        .unwrap_or_else(|| (authority.to_owned(), default_port_for_scheme(&scheme)));
    Ok((host, port))
}

fn default_port_for_scheme(scheme: &str) -> u16 {
    match scheme {
        "https" | "wss" => 443,
        "ssh" => 22,
        _ => 80,
    }
}

fn verify_ssh_ctox_connection(
    host: &str,
    user: &str,
    password: &str,
    install_root: &str,
) -> Result<(String, String)> {
    let target = format!("{}@{}", user.trim(), host.trim());
    let root_candidates = if install_root.trim().is_empty() {
        "\"$HOME/.local/lib/ctox/current\" \"$HOME/ctox\" \"$HOME/ctox/current\" \"/opt/ctox\""
            .to_owned()
    } else {
        shell_quote(install_root.trim())
    };
    let remote_cmd = format!(
        "set -eu; \
         for root in {roots}; do \
           if [ -x \"$root/bin/ctox\" ]; then \
             version=$(\"$root/bin/ctox\" version 2>/dev/null || \"$root/bin/ctox\" --version 2>/dev/null || true); \
             echo __CTOX_ROOT=\"$root\"; \
             echo __CTOX_VERSION=\"$version\"; \
             exit 0; \
           fi; \
         done; \
         if command -v ctox >/dev/null 2>&1; then \
           bin=$(command -v ctox); \
           root=$(cd \"$(dirname \"$bin\")/..\" && pwd); \
           version=$(ctox version 2>/dev/null || ctox --version 2>/dev/null || true); \
           echo __CTOX_ROOT=\"$root\"; \
           echo __CTOX_VERSION=\"$version\"; \
           exit 0; \
         fi; \
         exit 42",
        roots = root_candidates
    );
    let output = Command::new("sshpass")
        .arg("-p")
        .arg(password.trim())
        .arg("ssh")
        .arg("-o")
        .arg("StrictHostKeyChecking=no")
        .arg("-o")
        .arg("ConnectTimeout=8")
        .arg("-p")
        .arg("22")
        .arg(target)
        .arg(remote_cmd)
        .stdin(Stdio::null())
        .output()
        .context("SSH")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if stderr.is_empty() {
            anyhow::bail!("CTOX nicht gefunden");
        }
        anyhow::bail!("{stderr}");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let root = stdout
        .lines()
        .find_map(|line| line.strip_prefix("__CTOX_ROOT="))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("CTOX nicht gefunden")?
        .to_owned();
    let version = stdout
        .lines()
        .find_map(|line| line.strip_prefix("__CTOX_VERSION="))
        .map(str::trim)
        .unwrap_or_default()
        .to_owned();
    Ok((root, version))
}

fn delete_installation_payload(installation: &Installation) -> Result<String> {
    match installation.mode {
        InstallationMode::Local => {
            let root = installation
                .root_path
                .clone()
                .unwrap_or_else(|| PathBuf::from(installation.remote.install_root.trim()));
            delete_local_installation_path(root)?;
            Ok("Installation gelöscht und Verbindung entfernt.".to_owned())
        }
        InstallationMode::RemoteWebRtc => match installation.remote.host_target {
            RemoteHostTarget::Ssh => {
                delete_remote_installation_path(&installation.remote)?;
                Ok("Remote Installation gelöscht und Verbindung entfernt.".to_owned())
            }
            RemoteHostTarget::Localhost => {
                let root = expand_local_path(&installation.remote.install_root)?;
                delete_local_installation_path(root)?;
                Ok("Lokale Installation gelöscht und Verbindung entfernt.".to_owned())
            }
            RemoteHostTarget::Unspecified => {
                anyhow::bail!("Diese Verbindung kennt keinen löschbaren Installationspfad.")
            }
        },
    }
}

fn delete_local_installation_path(path: PathBuf) -> Result<()> {
    validate_local_delete_path(&path)?;
    if !path.exists() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(&path).with_context(|| format!("failed to remove {}", path.display()))?;
    } else if metadata.is_dir() {
        fs::remove_dir_all(&path)
            .with_context(|| format!("failed to remove directory {}", path.display()))?;
    }
    Ok(())
}

fn validate_local_delete_path(path: &std::path::Path) -> Result<()> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let component_count = path.components().count();
    anyhow::ensure!(
        component_count > 3,
        "refusing to delete a broad system path"
    );
    if let Some(home) = dirs::home_dir() {
        anyhow::ensure!(path != home, "refusing to delete the home directory");
    }
    anyhow::ensure!(
        path != PathBuf::from("/"),
        "refusing to delete filesystem root"
    );
    Ok(())
}

fn delete_remote_installation_path(remote: &RemoteAccessSettings) -> Result<()> {
    let ssh_host = remote.ssh_host.trim();
    let ssh_user = remote.ssh_user.trim();
    let ssh_password = remote.ssh_password.trim();
    anyhow::ensure!(
        !ssh_host.is_empty() && !ssh_user.is_empty() && !ssh_password.is_empty(),
        "Remote-Löschung braucht Host, SSH-User und SSH-Passwort."
    );
    validate_remote_delete_path(&remote.install_root)?;
    let remote_root = remote_path_expr(&remote.install_root);
    let target = format!("{ssh_user}@{ssh_host}");
    let remote_cmd = format!(
        "pkill -f ctox-desktop-host 2>/dev/null || true; \
         pkill -f 'ctox business-os serve' 2>/dev/null || true; \
         rm -rf -- {remote_root}"
    );
    let output = Command::new("sshpass")
        .arg("-p")
        .arg(ssh_password)
        .arg("ssh")
        .arg("-o")
        .arg("StrictHostKeyChecking=no")
        .arg("-o")
        .arg("ConnectTimeout=10")
        .arg("-p")
        .arg(remote.ssh_port.to_string())
        .arg(target)
        .arg(remote_cmd)
        .stdin(Stdio::null())
        .output()
        .context("failed to delete remote CTOX installation over SSH")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        anyhow::bail!(
            "remote delete failed{}",
            if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            }
        );
    }
    Ok(())
}

fn validate_remote_delete_path(path: &str) -> Result<()> {
    let trimmed = path.trim();
    anyhow::ensure!(!trimmed.is_empty(), "remote install path is empty");
    anyhow::ensure!(
        !matches!(trimmed, "/" | "." | "~" | "$HOME" | "${HOME}"),
        "refusing to delete a broad remote path"
    );
    anyhow::ensure!(
        !trimmed.starts_with('-'),
        "remote install path must not start with '-'"
    );
    Ok(())
}

fn open_external_url(url: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg(url).status()?;

    #[cfg(target_os = "windows")]
    let status = Command::new("cmd")
        .args(["/C", "start", "", url])
        .status()?;

    #[cfg(all(unix, not(target_os = "macos")))]
    let status = Command::new("xdg-open").arg(url).status()?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("failed to open {url}");
    }
}

fn run_upgrade(kind: VersionProbeKind, tx: std::sync::mpsc::Sender<UpgradeEvent>) {
    use std::io::{BufRead, BufReader};
    let mut command = match &kind {
        VersionProbeKind::Local { binary } => {
            let _ = tx.send(UpgradeEvent::Status(format!(
                "running `{} upgrade`",
                binary.display()
            )));
            let mut cmd = std::process::Command::new(binary);
            cmd.arg("upgrade");
            cmd
        }
        VersionProbeKind::Ssh {
            user,
            host,
            port,
            password,
        } => {
            let _ = tx.send(UpgradeEvent::Status(format!(
                "running `ctox upgrade` on {user}@{host}"
            )));
            let mut cmd = std::process::Command::new("sshpass");
            cmd.arg("-p")
                .arg(password)
                .arg("ssh")
                .arg("-o")
                .arg("StrictHostKeyChecking=no")
                .arg("-p")
                .arg(port.to_string())
                .arg(format!("{user}@{host}"))
                .arg("PATH=\"$HOME/.local/bin:$PATH\" ctox upgrade 2>&1");
            cmd
        }
    };
    let child = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(err) => {
            let _ = tx.send(UpgradeEvent::Finished(Err(err.to_string())));
            return;
        }
    };
    if let Some(stdout) = child.stdout.take() {
        let tx_line = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let _ = tx_line.send(UpgradeEvent::Status(line));
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let tx_line = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let _ = tx_line.send(UpgradeEvent::Status(line));
            }
        });
    }
    match child.wait() {
        Ok(status) if status.success() => {
            let _ = tx.send(UpgradeEvent::Finished(Ok("upgrade completed".to_owned())));
        }
        Ok(status) => {
            let _ = tx.send(UpgradeEvent::Finished(Err(format!(
                "upgrade exited with {status}"
            ))));
        }
        Err(err) => {
            let _ = tx.send(UpgradeEvent::Finished(Err(err.to_string())));
        }
    }
}
