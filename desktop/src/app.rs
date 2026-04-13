use std::{
    process::Command,
    collections::BTreeMap,
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

use anyhow::Result;
use eframe::{
    CreationContext,
    egui::{
        self, Align, Button, Color32, Context, FontFamily, FontId, Frame, Id, Key, Layout,
        RichText, ScrollArea, Sense, SidePanel, Stroke, TextEdit, TextFormat,
        Ui, Visuals,
        text::LayoutJob,
    },
};
use rfd::FileDialog;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    command_catalog::{COMMANDS, CommandEntry, CommandGroup, is_allowed_ctox_args},
    connector::{InstanceConnector, LocalProcessConnector, SessionKind, SessionSpec, repo_root_from_manifest_dir},
    installations::{
        Installation, InstallationMode, InstallationRegistry, RemoteHostTarget, RemoteInstanceSource,
    },
    provision::{ProvisionEvent, ProvisionRequest},
    terminal_backend::TerminalSession,
    terminal_emulator::{TERMINAL_DEFAULT_BG, TerminalSnapshot},
    views::{self, DataView, DataViewState},
};

const LEFT_PANEL_WIDTH: f32 = 300.0;
const RIGHT_PANEL_WIDTH: f32 = 360.0;
const TERMINAL_FONT_SIZE_MIN: f32 = 8.6;
const TERMINAL_FONT_SIZE_MAX: f32 = 11.8;
const TERMINAL_ZOOM_MIN: f32 = 0.75;
const TERMINAL_ZOOM_MAX: f32 = 1.25;
const COMMAND_COMPOSER_MIN_HEIGHT: f32 = 28.0;
const COMMAND_COMPOSER_MAX_HEIGHT: f32 = 42.0;
const COMPOSER_MODELS: &[&str] = &["GPT-5.4", "openai/gpt-oss-20b", "Qwen/Qwen3.5-35B-A3B"];
const COMPOSER_PRESETS: &[&str] = &["Quality", "Performance"];

pub struct CtoxDesktopApp {
    registry: InstallationRegistry,
    connector: LocalProcessConnector,
    selected_installation_id: Option<String>,
    expanded_installation_id: Option<String>,
    tabs: Vec<DesktopTab>,
    active_tab_id: Option<String>,
    show_add_menu: bool,
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
    data_view_state: DataViewState,
    provision_rx: Option<Receiver<ProvisionEvent>>,
    provisioning_installation_id: Option<String>,
    provision_status: Option<String>,
    provision_log: Vec<String>,
    provision_running: bool,
}

struct DesktopTab {
    id: String,
    installation_id: String,
    title: String,
    kind: SessionKind,
    terminal: TerminalSession,
    last_size: (u16, u16),
    active_tui_view: Option<TuiView>,
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

#[derive(Clone)]
struct InstallationRuntimeStatus {
    label: String,
    color: Color32,
}

#[derive(Debug, Deserialize)]
struct DesktopServiceStatusSnapshot {
    running: bool,
    #[serde(default)]
    busy: bool,
    #[serde(default)]
    last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TuiView {
    Chat,
    Skills,
    SettingsModel,
    SettingsCommunication,
}

impl TuiView {
    const ALL: [Self; 4] = [
        Self::Chat,
        Self::Skills,
        Self::SettingsModel,
        Self::SettingsCommunication,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Chat => "Chat",
            Self::Skills => "Skills",
            Self::SettingsModel => "Settings / Model",
            Self::SettingsCommunication => "Settings / Communication",
        }
    }
}

impl CtoxDesktopApp {
    pub fn new(cc: &CreationContext<'_>) -> Result<Self> {
        apply_theme(&cc.egui_ctx);
        let mut registry = InstallationRegistry::load().unwrap_or_default();

        if registry.installations.is_empty() {
            if let Some(root) = repo_root_from_manifest_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).as_path()) {
                if root.join("Cargo.toml").is_file() && root.join("src/main.rs").is_file() {
                    let _ = registry.add_installation_path(root);
                    let _ = registry.save();
                }
            }
        }

        let selected_installation_id = registry.installations.first().map(|entry| entry.id.clone());
        Ok(Self {
            registry,
            connector: LocalProcessConnector,
            selected_installation_id,
            expanded_installation_id: None,
            tabs: Vec::new(),
            active_tab_id: None,
            show_add_menu: false,
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
            data_view_state: DataViewState::default(),
            provision_rx: None,
            provisioning_installation_id: None,
            provision_status: None,
            provision_log: Vec::new(),
            provision_running: false,
        })
    }

    fn selected_installation(&self) -> Option<&Installation> {
        let selected = self.selected_installation_id.as_deref()?;
        self.registry.installations.iter().find(|entry| entry.id == selected)
    }

    fn open_folder_dialog(&mut self) {
        let Some(path) = FileDialog::new().set_title("Choose CTOX folder").pick_folder() else {
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

    fn remove_selected_installation(&mut self) {
        let Some(installation_id) = self.selected_installation_id.clone() else {
            return;
        };
        self.registry.remove(&installation_id);
        self.tabs.retain(|tab| tab.installation_id != installation_id);
        self.active_tab_id = self.tabs.first().map(|tab| tab.id.clone());
        self.selected_installation_id = self.registry.installations.first().map(|entry| entry.id.clone());
        if self.expanded_installation_id.as_deref() == Some(installation_id.as_str()) {
            self.expanded_installation_id = None;
        }
        if let Err(error) = self.registry.save() {
            self.notice = Some(error.to_string());
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
                    "Open CTOX Settings / Communication on the target host and configure Signaling Server, Remote Room, and Remote Password first.".to_owned()
                }
            });
            return;
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
            if !self.composer_attachments.iter().any(|existing| existing == &file) {
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
                    active_tui_view: matches!(kind, SessionKind::Tui).then_some(TuiView::Chat),
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
            .or_else(|| self.tabs.iter().rev().find(|tab| tab.kind == SessionKind::Command).map(|tab| tab.id.clone()));
    }

    fn find_tui_tab_id(&self, installation_id: &str) -> Option<String> {
        self.tabs
            .iter()
            .find(|tab| tab.kind == SessionKind::Tui && tab.installation_id == installation_id)
            .map(|tab| tab.id.clone())
    }

    fn select_installation_and_focus(&mut self, installation_id: String) {
        self.selected_installation_id = Some(installation_id.clone());

        // When switching installation, reset data view to Terminal
        // so we don't show stale data from the previous installation
        self.data_view_state.active_view = DataView::Terminal;

        let runtime_status = self.installation_runtime_status(&installation_id);
        let can_show_tui = !matches!(runtime_status.label.as_str(), "Offline" | "Error");
        if can_show_tui {
            if let Some(existing_tab_id) = self.find_tui_tab_id(&installation_id) {
                self.active_tab_id = Some(existing_tab_id);
                self.terminal_focus = true;
                return;
            }
        }

        self.active_tab_id = None;
        self.terminal_focus = false;

        let should_open = self
            .registry
            .installations
            .iter()
            .find(|entry| entry.id == installation_id)
            .map(installation_ready_for_tui)
            .unwrap_or(false);
        if should_open && can_show_tui {
            self.focus_or_open_tui(&installation_id);
        }
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

    fn active_tab(&self) -> Option<&DesktopTab> {
        let active = self.active_tab_id.as_deref()?;
        self.tabs.iter().find(|tab| tab.id == active)
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
                egui::Event::Key { key, pressed, modifiers, .. } if pressed => {
                    if let Some(bytes) = key_event_to_bytes(key, modifiers) {
                        let _ = tab.terminal.write_input(&bytes, true);
                        if key == Key::Tab && !modifiers.shift {
                            tab.active_tui_view = tab.active_tui_view.map(next_tui_view);
                        } else if key == Key::Tab && modifiers.shift {
                            tab.active_tui_view = tab.active_tui_view.map(previous_tui_view);
                        }
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
                                if let Some(installation_id) = self.provisioning_installation_id.clone() {
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
            .width_range(260.0..=460.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Instances").size(17.0).strong());
                        ui.label(RichText::new("Local and remote").size(13.0).color(Color32::from_gray(118)));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(
                                Button::new(RichText::new("+").size(18.0))
                                    .min_size(egui::vec2(28.0, 28.0))
                                    .corner_radius(8.0),
                            )
                            .clicked()
                        {
                            self.show_add_menu = !self.show_add_menu;
                        }
                    });
                });

                if self.show_add_menu {
                    Frame::default()
                        .fill(Color32::from_rgb(24, 27, 31))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(40, 44, 50)))
                        .corner_radius(12.0)
                        .inner_margin(12.0)
                        .show(ui, |ui| {
                            ui.label(RichText::new("New instance").strong());
                            ui.add_space(8.0);
                            if ui.button("Choose local CTOX folder").clicked() {
                                self.open_folder_dialog();
                                self.show_add_menu = false;
                            }
                            if ui.button("Connect CTOX").clicked() {
                                self.add_remote_installation_with_source(RemoteInstanceSource::AttachExisting);
                            }
                            if ui.button("New CTOX").clicked() {
                                self.add_remote_installation_with_source(RemoteInstanceSource::InstallNew);
                            }
                        });
                    ui.add_space(10.0);
                }

                ui.add_space(10.0);
                if self.registry.installations.is_empty() {
                    ui.label("No CTOX installation added yet.");
                    return;
                }

                ScrollArea::vertical().show(ui, |ui| {
                    let cards: Vec<(String, String, String, bool, bool, InstallationRuntimeStatus)> = self
                        .registry
                        .installations
                        .iter()
                        .map(|installation| {
                            (
                                installation.id.clone(),
                                installation.display_name(),
                                installation.display_path(),
                                installation.is_remote(),
                                self.selected_installation_id.as_deref() == Some(installation.id.as_str()),
                                self.installation_runtime_status(&installation.id),
                            )
                        })
                        .collect();

                    let mut clicked_installation = None;
                    let mut toggled_installation = None;
                    for (installation_id, title, subtitle, is_remote, is_selected, runtime_status) in cards.iter().cloned() {
                        let fill = if is_selected {
                            Color32::from_rgb(34, 38, 43)
                        } else {
                            Color32::TRANSPARENT
                        };
                        Frame::default()
                            .fill(fill)
                            .stroke(Stroke::NONE)
                            .corner_radius(14.0)
                            .shadow(egui::epaint::Shadow::NONE)
                            .inner_margin(egui::Margin::symmetric(12, 10))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        let title_response = ui.add(
                                            egui::Label::new(
                                                RichText::new(title)
                                                    .size(14.8)
                                                    .color(if is_selected {
                                                        Color32::from_rgb(231, 234, 238)
                                                    } else {
                                                        Color32::from_rgb(203, 207, 213)
                                                    }),
                                            )
                                            .sense(Sense::click()),
                                        );
                                        if title_response.clicked() {
                                            clicked_installation = Some(installation_id.clone());
                                        }
                                        let subtitle_response = ui.add(
                                            egui::Label::new(
                                                RichText::new(subtitle)
                                                    .size(12.5)
                                                    .color(Color32::from_gray(if is_selected { 143 } else { 115 })),
                                            )
                                            .sense(Sense::click()),
                                        );
                                        if subtitle_response.clicked() {
                                            clicked_installation = Some(installation_id.clone());
                                        }
                                        ui.add_space(4.0);
                                        ui.label(
                                            RichText::new(runtime_status.label)
                                                .size(12.1)
                                                .color(runtime_status.color),
                                        );
                                    });
                                    ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                                        let expanded = self.expanded_installation_id.as_deref()
                                            == Some(installation_id.as_str());
                                        if ui
                                            .add(
                                                Button::new(
                                                    RichText::new(if expanded { "Close" } else { "Settings" })
                                                        .text_style(egui::TextStyle::Small)
                                                        .color(Color32::from_gray(150)),
                                                )
                                                .frame(false)
                                                .min_size(egui::vec2(54.0, 18.0)),
                                            )
                                            .clicked()
                                        {
                                            toggled_installation = Some(installation_id.clone());
                                        }
                                        ui.label(
                                            RichText::new(if is_remote { "Remote" } else { "Local" })
                                                .size(12.2)
                                                .color(if is_remote {
                                                    Color32::from_rgb(104, 154, 181)
                                                } else {
                                                    Color32::from_rgb(123, 164, 126)
                                                }),
                                        );
                                    });
                                });
                                if self.expanded_installation_id.as_deref() == Some(installation_id.as_str()) {
                                    ui.add_space(10.0);
                                    self.render_installation_settings_inline(ui, &installation_id);
                                }
                            });
                        ui.add_space(4.0);
                    }

                    if let Some(installation_id) = toggled_installation {
                        self.toggle_installation_settings(installation_id);
                    }
                    if let Some(installation_id) = clicked_installation {
                        self.select_installation_and_focus(installation_id);
                    }
                    if let Some(notice) = &self.notice {
                        ui.add_space(10.0);
                        ui.label(notice);
                    }
                });
            });
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
                                ui.label(RichText::new(command.description).size(12.5).color(Color32::from_gray(135)));
                                ui.monospace(command.example);
                                if let Some(hint) = command.extra_args_hint {
                                    let extra_args = self
                                        .command_extra_args
                                        .entry(command.example.to_owned())
                                        .or_default();
                                    ui.add(
                                        TextEdit::singleline(extra_args)
                                            .hint_text(hint),
                                    );
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
        let mut persist_registry = false;
        let mut open_tui = false;
        let mut start_provision = false;
        let mut remove_selected = false;

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
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button("Remove").clicked() {
                                remove_selected = true;
                            }
                        });
                    });
                    ui.add_space(12.0);

                    match installation.mode {
                        InstallationMode::Local => {
                            ui.label(RichText::new("Path").color(Color32::from_gray(135)));
                            ui.monospace(installation.display_path());
                            ui.add_space(10.0);
                            if ui.button("Open").clicked() {
                                open_tui = true;
                            }
                            ui.add_space(6.0);
                            ui.collapsing("Advanced", |ui| {
                                if ui
                                    .add(TextEdit::singleline(&mut installation.name).hint_text("Name (optional)"))
                                    .changed()
                                {
                                    persist_registry = true;
                                }
                            });
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
                                        RichText::new("Signaling server, room, and password come from CTOX Settings / Communication.")
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
                                    ui.label(RichText::new("Password").color(Color32::from_gray(135)));
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
                                if ui
                                    .add(TextEdit::singleline(&mut installation.name).hint_text("Name (optional)"))
                                    .changed()
                                {
                                    persist_registry = true;
                                }
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
                                            ui.label(RichText::new("Activity").size(12.5).color(Color32::from_gray(155)));
                                            ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                                                for line in self.provision_log.iter().rev().take(8).rev() {
                                                    ui.label(RichText::new(line).size(12.2).color(Color32::from_gray(180)));
                                                }
                                            });
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
                                .add_enabled(can_connect, Button::new("Connect"))
                                .clicked()
                            {
                                open_tui = true;
                            }
                        }
                    }
                }
            });

        if persist_registry {
            if let Err(error) = self.registry.save() {
                self.notice = Some(error.to_string());
            }
        }
        if remove_selected {
            self.remove_selected_installation();
        }
        if start_provision {
            self.start_provisioning();
        }
        if open_tui {
            self.selected_installation_id = Some(installation_id);
            self.spawn_tui_tab();
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

    // render_tui_view_tabs is now integrated into render_mode_tabs

    fn render_terminal_area(&mut self, ui: &mut Ui) {
        let Some(active_tab_id) = self.active_tab_id.clone() else {
            // Show provisioning stream in the main area when provisioning is active or has logs
            if self.provision_running || !self.provision_log.is_empty() {
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        let title = if self.provision_running {
                            "Installation läuft..."
                        } else if self.provision_status.as_deref().map_or(false, |s| s.contains("vorbereitet")) {
                            "Installation abgeschlossen"
                        } else {
                            "Installations-Log"
                        };
                        ui.heading(RichText::new(title).color(Color32::from_gray(220)));
                    });
                    ui.add_space(8.0);

                    // Progress bar
                    if self.provision_running {
                        let progress = self.provision_status.as_deref()
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

            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.add_space(120.0);
                ui.heading("CTOX Desktop");
                if let Some(installation) = self.selected_installation() {
                    match installation.mode {
                        InstallationMode::Local => {
                            ui.label("Choose a local CTOX installation on the left.");
                        }
                        InstallationMode::RemoteWebRtc => {
                            ui.label("Enter room and password on the left.");
                            ui.label("Then open the instance from the list or with Connect.");
                        }
                    }
                } else {
                    ui.label("Choose an instance on the left.");
                }
            });
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
                let _ = tab.terminal.resize(rows, cols, available.x as u16, available.y as u16);
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
                    ui.label(message);
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
                    ui.label("CTOX is connecting or loading...");
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
            let (rect, response) =
                ui.allocate_exact_size(desired, Sense::click());
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
                    render_snapshot(ui, &snapshot, true, metrics.font_size, metrics.line_height);
                });
            } else {
                ui.allocate_ui_at_rect(rect, |ui| {
                    ui.set_clip_rect(rect);
                    ScrollArea::both()
                        .id_salt(Id::new(("terminal-scroll", &tab_id)))
                        .show(ui, |ui| {
                            render_snapshot(ui, &snapshot, false, metrics.font_size, metrics.line_height)
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
                            Button::new(RichText::new("+").size(23.0).color(Color32::from_gray(196)))
                                .frame(false)
                                .min_size(egui::vec2(22.0, 22.0)),
                        )
                        .clicked()
                    {
                        self.pick_composer_attachments();
                    }
                    ui.add_space(14.0);
                    ui.label(
                        RichText::new(format!("{} · {}", selected_name, if remote { "Remote" } else { "Local" }))
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
                        let mic_label = if self.transcript_panel_open { "●" } else { "🎙" };
                        if ui
                            .add(
                                Button::new(RichText::new(mic_label).size(18.0).color(Color32::from_gray(150)))
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
            Some((format!("{} beendet mit Exit-Code {}", result.title, code), color))
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
        self.refresh_installation_statuses();
        self.handle_terminal_input(ctx);

        // Sync data view root with selected installation
        let selected_root = self
            .selected_installation()
            .and_then(|inst| inst.root_path.clone());
        self.data_view_state.set_root(selected_root);

        // Periodic data refresh when a data view is active
        if self.data_view_state.active_view != DataView::Terminal
            && self.data_view_state.needs_refresh()
        {
            self.data_view_state.refresh();
        }

        self.render_left_panel(ctx);
        self.render_right_panel(ctx);

        let window_height = ctx.input(|input| input.screen_rect().height());
        let composer_height = (window_height * 0.16)
            .clamp(96.0, 150.0);
        egui::TopBottomPanel::bottom("desktop-composer-panel")
            .exact_height(composer_height)
            .show(ctx, |ui| {
                self.render_command_composer(ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_tabs(ui);
            ui.add_space(6.0);
            self.render_mode_tabs(ui);
            ui.add_space(8.0);

            let available_width = ui.available_width();
            let remaining_height = ui.available_height();
            let status_height = if self.command_run.is_some() || self.last_command_result.is_some() {
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

            if self.data_view_state.active_view == DataView::Terminal {
                ui.allocate_ui_at_rect(content_rect, |ui| {
                    self.render_terminal_area(ui);
                });
            } else {
                ui.allocate_ui_at_rect(content_rect, |ui| {
                    self.render_data_view_content(ui);
                });
            }
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
        let in_data_mode = self.data_view_state.active_view != DataView::Terminal;

        ui.horizontal(|ui| {
            // TUI mode tabs
            if !in_data_mode {
                // Show TUI sub-tabs (Chat/Skills/Settings) inline
                if let Some(tab) = self.active_tab() {
                    if tab.kind == SessionKind::Tui {
                        let current = tab.active_tui_view.unwrap_or(TuiView::Chat);
                        for view in TuiView::ALL {
                            let selected = current == view;
                            if ui.selectable_label(selected, view.label()).clicked() {
                                self.switch_active_tui_view(view);
                            }
                        }
                    }
                }
            } else {
                // Show Data sub-tabs inline
                for view in DataView::DATA_VIEWS {
                    if ui.selectable_label(self.data_view_state.active_view == view, view.label()).clicked() {
                        self.data_view_state.active_view = view;
                        self.data_view_state.last_refresh = None;
                    }
                }
            }

            // Right-aligned mode switch
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !self.show_right_panel {
                    if ui
                        .add(
                            Button::new(RichText::new("CLI").size(12.0).color(Color32::from_gray(140)))
                                .frame(false),
                        )
                        .clicked()
                    {
                        self.show_right_panel = true;
                    }
                    ui.add_space(8.0);
                }

                if in_data_mode {
                    // Zoom controls don't apply in data mode
                    if ui
                        .add(
                            Button::new(
                                RichText::new("TUI")
                                    .size(12.5)
                                    .color(Color32::from_gray(170)),
                            )
                            .min_size(egui::vec2(42.0, 22.0))
                            .corner_radius(6.0),
                        )
                        .clicked()
                    {
                        self.data_view_state.active_view = DataView::Terminal;
                        self.terminal_focus = true;
                    }
                } else {
                    // Zoom controls + Data button
                    if ui
                        .add(
                            Button::new(
                                RichText::new("Data")
                                    .size(12.5)
                                    .color(Color32::from_gray(170)),
                            )
                            .min_size(egui::vec2(42.0, 22.0))
                            .corner_radius(6.0),
                        )
                        .clicked()
                    {
                        self.data_view_state.active_view = DataView::Tickets;
                        self.terminal_focus = false;
                        self.data_view_state.last_refresh = None;
                    }
                    ui.add_space(8.0);
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
                }
            });
        });
    }

    fn render_data_view_content(&mut self, ui: &mut Ui) {
        Frame::default()
            .fill(Color32::from_rgb(14, 16, 20))
            .stroke(Stroke::new(1.0, Color32::from_rgb(34, 38, 44)))
            .corner_radius(16.0)
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ScrollArea::both()
                    .id_salt("data-view-scroll")
                    .show(ui, |ui| {
                        let dvs = &mut self.data_view_state;
                        match dvs.active_view {
                            DataView::Terminal => {}
                            DataView::Tickets => {
                                let tickets = dvs.ticket_items.clone();
                                let cases = dvs.ticket_cases.clone();
                                let actions = dvs.execution_actions.clone();
                                let root = dvs.root.clone();
                                views::kanban::render(
                                    ui, &tickets, &cases, &actions,
                                    &mut dvs.kanban_state,
                                    root.as_deref(),
                                );
                            }
                            DataView::Queue => {
                                let msgs = dvs.comm_messages.clone();
                                views::queue::render(ui, &msgs, &mut dvs.queue_state);
                            }
                            DataView::Conversations => {
                                let msgs = dvs.lcm_messages.clone();
                                let missions = dvs.mission_states.clone();
                                let docs = dvs.continuity_docs.clone();
                                let root = dvs.root.clone();
                                views::conversations::render(
                                    ui, &msgs, &missions, &docs,
                                    &mut dvs.conversations_state,
                                    root.as_deref(),
                                );
                            }
                            DataView::Threads => {
                                let threads = dvs.threads.clone();
                                views::threads::render(ui, &threads, &mut dvs.threads_state);
                            }
                            DataView::Logs => {
                                let logs = dvs.logs.clone();
                                views::logs::render(ui, &logs, &mut dvs.logs_state);
                            }
                        }
                    });
            });
    }

    fn switch_active_tui_view(&mut self, target: TuiView) {
        let Some(tab) = self.active_tab_mut() else {
            return;
        };
        if tab.kind != SessionKind::Tui {
            return;
        }
        let current = tab.active_tui_view.unwrap_or(TuiView::Chat);
        if current == target {
            return;
        }

        let current_idx = tui_view_index(current);
        let target_idx = tui_view_index(target);
        let forward = (target_idx + 4 - current_idx) % 4;
        let backward = (current_idx + 4 - target_idx) % 4;

        if forward <= backward {
            for _ in 0..forward {
                let _ = tab.terminal.write_input(b"\t", true);
            }
        } else {
            for _ in 0..backward {
                let _ = tab.terminal.write_input(b"\x1b[Z", true);
            }
        }

        tab.active_tui_view = Some(target);
        self.terminal_focus = true;
    }
}

fn render_snapshot(
    ui: &mut Ui,
    snapshot: &TerminalSnapshot,
    trim_blank_lines: bool,
    font_size: f32,
    line_height: f32,
) {
    if snapshot.styled_lines.is_empty() {
        render_plain_snapshot(ui, &snapshot.output, trim_blank_lines, font_size, line_height);
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
        for line in lines {
            let mut job = LayoutJob::default();
            job.wrap.max_width = f32::INFINITY;
            for run in &line.runs {
                let format = TextFormat {
                    font_id: egui::FontId::monospace(font_size),
                    color: rgb_u32_to_color(run.fg),
                    background: if run.bg == TERMINAL_DEFAULT_BG {
                        Color32::TRANSPARENT
                    } else {
                        rgb_u32_to_color(run.bg)
                    },
                    ..Default::default()
                };
                job.append(&run.text, 0.0, format);
            }
            if line.runs.is_empty() {
                job.append(
                    " ",
                    0.0,
                    TextFormat {
                        font_id: egui::FontId::monospace(font_size),
                        color: Color32::from_gray(160),
                        ..Default::default()
                    },
                );
            }
            let galley = ui.painter().layout_job(job);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(width, line_height), Sense::hover());
            ui.painter()
                .galley(rect.min, galley, Color32::from_gray(200));
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
            ui.painter().galley(rect.min, galley, Color32::from_gray(220));
        }
    });
}

fn styled_line_is_blank(line: &crate::terminal_emulator::TerminalStyledLine) -> bool {
    line.runs.is_empty() || line.runs.iter().all(|run| run.text.trim().is_empty())
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
    style.spacing.item_spacing = egui::vec2(7.0, 7.0);
    style.spacing.button_padding = egui::vec2(6.0, 4.0);
    style.spacing.menu_margin = egui::Margin::same(8);
    style.text_styles.insert(
        egui::TextStyle::Heading,
        FontId::new(16.5, FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        FontId::new(14.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        FontId::new(14.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        FontId::new(12.5, FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Monospace,
        FontId::new(13.2, FontFamily::Monospace),
    );

    let mut visuals = Visuals::dark();
    visuals.override_text_color = Some(Color32::from_rgb(218, 221, 226));
    visuals.panel_fill = Color32::from_rgb(28, 30, 35);
    visuals.window_fill = Color32::from_rgb(31, 33, 38);
    visuals.faint_bg_color = Color32::from_rgb(33, 35, 40);
    visuals.extreme_bg_color = Color32::from_rgb(13, 15, 18);
    visuals.code_bg_color = Color32::from_rgb(17, 19, 23);
    visuals.window_corner_radius = 16.into();
    visuals.menu_corner_radius = 12.into();
    visuals.window_shadow = egui::epaint::Shadow::NONE;
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 4],
        blur: 12,
        spread: 0,
        color: Color32::from_black_alpha(24),
    };
    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(28, 30, 35);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(40, 43, 49));
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(59, 58, 60);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(59, 58, 60);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(59, 58, 60));
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(68, 67, 69);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(68, 67, 69);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(68, 67, 69));
    visuals.widgets.active.bg_fill = Color32::from_rgb(71, 107, 130);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(71, 107, 130);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, Color32::from_rgb(71, 107, 130));
    visuals.widgets.open.bg_fill = Color32::from_rgb(38, 40, 45);
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, Color32::from_rgb(38, 40, 45));
    visuals.selection.bg_fill = Color32::from_rgb(44, 88, 113);
    visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(93, 156, 188));
    visuals.hyperlink_color = Color32::from_rgb(112, 170, 199);

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

fn next_tui_view(view: TuiView) -> TuiView {
    match view {
        TuiView::Chat => TuiView::Skills,
        TuiView::Skills => TuiView::SettingsModel,
        TuiView::SettingsModel => TuiView::SettingsCommunication,
        TuiView::SettingsCommunication => TuiView::Chat,
    }
}

fn previous_tui_view(view: TuiView) -> TuiView {
    match view {
        TuiView::Chat => TuiView::SettingsCommunication,
        TuiView::Skills => TuiView::Chat,
        TuiView::SettingsModel => TuiView::Skills,
        TuiView::SettingsCommunication => TuiView::SettingsModel,
    }
}

fn tui_view_index(view: TuiView) -> usize {
    match view {
        TuiView::Chat => 0,
        TuiView::Skills => 1,
        TuiView::SettingsModel => 2,
        TuiView::SettingsCommunication => 3,
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
        101 => "CTOX crashed while starting. Check the error details shown for the actual cause.".to_owned(),
        0 => "Session closed.".to_owned(),
        _ => format!("The session stopped unexpectedly (exit code {code})."),
    }
}

struct ResponsiveTerminalMetrics {
    cols: u16,
    rows: u16,
    font_size: f32,
    line_height: f32,
}

fn responsive_terminal_metrics(ui: &Ui, available: egui::Vec2, zoom: f32) -> ResponsiveTerminalMetrics {
    let zoom = zoom.clamp(TERMINAL_ZOOM_MIN, TERMINAL_ZOOM_MAX);
    let font_size = (8.9 * zoom).clamp(TERMINAL_FONT_SIZE_MIN, TERMINAL_FONT_SIZE_MAX);
    let font_id = FontId::monospace(font_size);
    let sample = ui
        .painter()
        .layout_no_wrap("MMMMMMMMMMMMMMMM".to_owned(), font_id.clone(), Color32::WHITE);
    let line_height = (font_size * 1.12).max(9.2);
    let cell_width = (sample.size().x / 16.0).max(5.0);
    let cols = (available.x / cell_width).floor().max(20.0) as u16;
    let rows = (available.y / line_height).floor().max(18.0) as u16;
    ResponsiveTerminalMetrics {
        cols,
        rows,
        font_size,
        line_height,
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
        InstallationRuntimeStatus {
            label: "Offline".to_owned(),
            color: Color32::from_rgb(136, 144, 155),
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

fn poll_local_installation_status(installation: &Installation) -> Result<InstallationRuntimeStatus> {
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
    } else if parsed.last_error.as_deref().map(str::trim).filter(|v| !v.is_empty()).is_some() {
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
