//! Background data plane for the TUI.
//!
//! The render loop must never wait on IO: every status probe, SQLite read,
//! subprocess spawn and IPC round-trip runs on one of two worker threads,
//! and the UI thread only schedules jobs and applies typed payloads.
//!
//! - The *poll* thread serves the cyclic refreshes (service status, chat
//!   window, communication feed, skills, harness flow, GPU sampling,
//!   telemetry, Jami resolution). It owns its own long-lived `LcmEngine`
//!   read connection and the chat change-detection marker.
//! - The *action* thread serves user-initiated long jobs (update/upgrade
//!   subprocesses, prompt submission, service start/stop) so a
//!   minutes-long engine rebuild cannot starve the cyclic refreshes.
//!
//! Tests and the headless smoke renderers construct the `App` without a
//! worker; `App::refresh` then runs the same collectors inline, so the
//! synchronous behavior stays covered.

use super::*;
use std::sync::mpsc::TryRecvError;

/// Cyclic data refreshes; one in-flight job per kind.
#[derive(Debug)]
pub(super) enum PollJob {
    ServiceStatus {
        probe: service::StatusProbeOptions,
    },
    ChatMessages,
    CommunicationFeed,
    SkillCatalog,
    HarnessFlow,
    GpuCards,
    RuntimeTelemetry,
    JamiResolve {
        refresh_key: String,
        configured_id: String,
        configured_name: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum PollKind {
    ServiceStatus,
    ChatMessages,
    CommunicationFeed,
    SkillCatalog,
    HarnessFlow,
    GpuCards,
    RuntimeTelemetry,
    JamiResolve,
}

impl PollJob {
    fn kind(&self) -> PollKind {
        match self {
            PollJob::ServiceStatus { .. } => PollKind::ServiceStatus,
            PollJob::ChatMessages => PollKind::ChatMessages,
            PollJob::CommunicationFeed => PollKind::CommunicationFeed,
            PollJob::SkillCatalog => PollKind::SkillCatalog,
            PollJob::HarnessFlow => PollKind::HarnessFlow,
            PollJob::GpuCards => PollKind::GpuCards,
            PollJob::RuntimeTelemetry => PollKind::RuntimeTelemetry,
            PollJob::JamiResolve { .. } => PollKind::JamiResolve,
        }
    }
}

/// User-initiated long-running actions, executed serially.
#[derive(Debug)]
pub(super) enum ActionJob {
    UpdateSubprocess {
        args: Vec<String>,
        action: UpdateActionKind,
    },
    SubmitPrompt {
        prompt: String,
        attachment_count: usize,
    },
    ToggleService {
        start: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UpdateActionKind {
    Check,
    Upgrade,
    EngineRebuild,
    Doctor,
}

impl UpdateActionKind {
    pub(super) fn completion_label(self) -> &'static str {
        match self {
            UpdateActionKind::Check => "check",
            UpdateActionKind::Upgrade => "upgrade",
            UpdateActionKind::EngineRebuild => "engine rebuild",
            UpdateActionKind::Doctor => "doctor",
        }
    }
}

pub(super) enum WorkerPayload {
    ServiceStatus {
        status: Option<service::ServiceStatus>,
        lifecycle_included: bool,
    },
    ChatMessages {
        /// `None` when the change marker did not move.
        messages: Option<Vec<lcm::MessageRecord>>,
        mission_state: Option<lcm::MissionStateRecord>,
        error: Option<String>,
    },
    CommunicationFeed(Vec<channels::CommunicationFeedItem>),
    SkillCatalog(Vec<SkillCatalogEntry>),
    HarnessFlow {
        flow: Option<service::harness_flow::HarnessFlow>,
        fallback: String,
    },
    GpuCards(Option<Vec<GpuCardState>>),
    RuntimeTelemetry(Option<RuntimeTelemetry>),
    JamiResolve {
        refresh_key: String,
        configured_id: String,
        configured_name: String,
        resolved: JamiResolveOutcome,
    },
    UpdateAction {
        action: UpdateActionKind,
        result: Result<String, String>,
    },
    PromptSubmitted {
        prompt: String,
        attachment_count: usize,
        result: Result<(), String>,
    },
    ServiceToggled {
        result: Result<String, String>,
    },
}

impl WorkerPayload {
    fn poll_kind(&self) -> Option<PollKind> {
        match self {
            WorkerPayload::ServiceStatus { .. } => Some(PollKind::ServiceStatus),
            WorkerPayload::ChatMessages { .. } => Some(PollKind::ChatMessages),
            WorkerPayload::CommunicationFeed(_) => Some(PollKind::CommunicationFeed),
            WorkerPayload::SkillCatalog(_) => Some(PollKind::SkillCatalog),
            WorkerPayload::HarnessFlow { .. } => Some(PollKind::HarnessFlow),
            WorkerPayload::GpuCards(_) => Some(PollKind::GpuCards),
            WorkerPayload::RuntimeTelemetry(_) => Some(PollKind::RuntimeTelemetry),
            WorkerPayload::JamiResolve { .. } => Some(PollKind::JamiResolve),
            _ => None,
        }
    }
}

pub(super) struct TuiWorker {
    poll_tx: mpsc::Sender<PollJob>,
    action_tx: mpsc::Sender<ActionJob>,
    results_rx: Receiver<WorkerPayload>,
    in_flight: HashSet<PollKind>,
}

impl TuiWorker {
    pub(super) fn spawn(root: PathBuf, db_path: PathBuf) -> Self {
        let (poll_tx, poll_rx) = mpsc::channel::<PollJob>();
        let (action_tx, action_rx) = mpsc::channel::<ActionJob>();
        let (results_tx, results_rx) = mpsc::channel::<WorkerPayload>();

        let poll_results = results_tx.clone();
        let poll_root = root.clone();
        thread::spawn(move || {
            let mut chat_source = ChatWindowSource::new(db_path);
            while let Ok(job) = poll_rx.recv() {
                let payload = run_poll_job(&poll_root, &mut chat_source, job);
                if poll_results.send(payload).is_err() {
                    break;
                }
            }
        });

        let action_root = root;
        thread::spawn(move || {
            while let Ok(job) = action_rx.recv() {
                let payload = run_action_job(&action_root, job);
                if results_tx.send(payload).is_err() {
                    break;
                }
            }
        });

        Self {
            poll_tx,
            action_tx,
            results_rx,
            in_flight: HashSet::new(),
        }
    }

    /// Schedule a cyclic refresh unless the same kind is already running.
    pub(super) fn schedule(&mut self, job: PollJob) {
        let kind = job.kind();
        if self.in_flight.contains(&kind) {
            return;
        }
        if self.poll_tx.send(job).is_ok() {
            self.in_flight.insert(kind);
        }
    }

    pub(super) fn submit_action(&self, job: ActionJob) {
        let _ = self.action_tx.send(job);
    }

    pub(super) fn try_recv(&mut self) -> Option<WorkerPayload> {
        match self.results_rx.try_recv() {
            Ok(payload) => {
                if let Some(kind) = payload.poll_kind() {
                    self.in_flight.remove(&kind);
                }
                Some(payload)
            }
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => None,
        }
    }
}

/// Poll-thread state for the chat window: a long-lived read connection and
/// the change-detection marker (see `LcmEngine::conversation_refresh_marker`).
pub(super) struct ChatWindowSource {
    db_path: PathBuf,
    engine: Option<lcm::LcmEngine>,
    marker: Option<(i64, i64, i64, i64)>,
}

impl ChatWindowSource {
    pub(super) fn new(db_path: PathBuf) -> Self {
        Self {
            db_path,
            engine: None,
            marker: None,
        }
    }

    fn engine(&mut self) -> Result<&lcm::LcmEngine> {
        if self.engine.is_none() {
            let engine = lcm::LcmEngine::open(&self.db_path, lcm::LcmConfig::default())?;
            engine.set_busy_timeout(TUI_LCM_BUSY_TIMEOUT)?;
            self.engine = Some(engine);
        }
        Ok(self.engine.as_ref().expect("engine initialized above"))
    }

    /// Load the chat window; `messages` stays `None` while the marker is
    /// unchanged. A failed read drops the connection for the next attempt.
    pub(super) fn collect(
        &mut self,
    ) -> (
        Option<Vec<lcm::MessageRecord>>,
        Option<lcm::MissionStateRecord>,
        Option<String>,
    ) {
        let previous_marker = self.marker;
        let loaded = self.engine().and_then(|engine| {
            let marker = engine.conversation_refresh_marker(turn_loop::CHAT_CONVERSATION_ID, 80)?;
            let mission_state = engine.stored_mission_state(turn_loop::CHAT_CONVERSATION_ID)?;
            if previous_marker == Some(marker) {
                return Ok((marker, None, mission_state));
            }
            let messages =
                engine.recent_messages_for_conversation(turn_loop::CHAT_CONVERSATION_ID, 80)?;
            Ok((marker, Some(messages), mission_state))
        });
        match loaded {
            Ok((marker, messages, mission_state)) => {
                if messages.is_some() {
                    self.marker = Some(marker);
                }
                (messages, mission_state, None)
            }
            Err(err) => {
                self.engine = None;
                (None, None, Some(err.to_string()))
            }
        }
    }
}

fn run_poll_job(root: &Path, chat: &mut ChatWindowSource, job: PollJob) -> WorkerPayload {
    match job {
        PollJob::ServiceStatus { probe } => WorkerPayload::ServiceStatus {
            lifecycle_included: probe.lifecycle_alerts,
            status: service::service_status_snapshot_with(root, &probe).ok(),
        },
        PollJob::ChatMessages => {
            let (messages, mission_state, error) = chat.collect();
            WorkerPayload::ChatMessages {
                messages,
                mission_state,
                error,
            }
        }
        PollJob::CommunicationFeed => WorkerPayload::CommunicationFeed(
            channels::load_recent_communication_feed(root, 10).unwrap_or_default(),
        ),
        PollJob::SkillCatalog => WorkerPayload::SkillCatalog(load_skill_catalog(root)),
        PollJob::HarnessFlow => {
            let (flow, fallback) = collect_harness_flow(root);
            WorkerPayload::HarnessFlow { flow, fallback }
        }
        PollJob::GpuCards => WorkerPayload::GpuCards(sample_gpu_cards().ok()),
        PollJob::RuntimeTelemetry => {
            WorkerPayload::RuntimeTelemetry(load_runtime_telemetry(root).ok().flatten())
        }
        PollJob::JamiResolve {
            refresh_key,
            configured_id,
            configured_name,
        } => {
            let resolved = resolve_jami_runtime_account(root, &configured_id, &configured_name);
            WorkerPayload::JamiResolve {
                refresh_key,
                configured_id,
                configured_name,
                resolved,
            }
        }
    }
}

fn run_action_job(root: &Path, job: ActionJob) -> WorkerPayload {
    match job {
        ActionJob::UpdateSubprocess { args, action } => WorkerPayload::UpdateAction {
            action,
            result: run_update_subprocess(root, &args).map_err(|err| err.to_string()),
        },
        ActionJob::SubmitPrompt {
            prompt,
            attachment_count,
        } => {
            let result = service::prepare_chat_prompt(root, &prompt)
                .and_then(|prepared| {
                    service::submit_chat_prompt(root, &prepared.prompt).map(|_| prepared.prompt)
                })
                .map(|_| ())
                .map_err(|err| err.to_string());
            WorkerPayload::PromptSubmitted {
                prompt,
                attachment_count,
                result,
            }
        }
        ActionJob::ToggleService { start } => WorkerPayload::ServiceToggled {
            result: if start {
                service::start_background(root).map_err(|err| err.to_string())
            } else {
                service::stop_background(root).map_err(|err| err.to_string())
            },
        },
    }
}

pub(super) fn collect_harness_flow(
    root: &Path,
) -> (Option<service::harness_flow::HarnessFlow>, String) {
    match service::harness_flow::load_latest_flow(root) {
        Ok(flow) => (Some(flow), String::new()),
        Err(err) => (
            None,
            format!(
                "Harness flow unavailable.\n\n{}",
                summarize_inline(&err.to_string(), 140)
            ),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    fn worker_root(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ctox-tui-worker-{label}-{stamp}"));
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        root
    }

    fn wait_for_payload(worker: &mut TuiWorker) -> WorkerPayload {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(payload) = worker.try_recv() {
                return payload;
            }
            assert!(Instant::now() < deadline, "worker payload timed out");
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn poll_jobs_round_trip_and_clear_in_flight() {
        let root = worker_root("telemetry");
        let db_path = root.join("runtime/ctox.sqlite3");
        let mut worker = TuiWorker::spawn(root.clone(), db_path);

        worker.schedule(PollJob::RuntimeTelemetry);
        assert!(worker.in_flight.contains(&PollKind::RuntimeTelemetry));
        // Duplicate kinds are deduped while in flight.
        worker.schedule(PollJob::RuntimeTelemetry);
        assert_eq!(worker.in_flight.len(), 1);

        let payload = wait_for_payload(&mut worker);
        assert!(matches!(payload, WorkerPayload::RuntimeTelemetry(_)));
        assert!(worker.in_flight.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn chat_window_source_reports_empty_conversation_once() {
        let root = worker_root("chat");
        let db_path = root.join("runtime/ctox.sqlite3");
        let mut worker = TuiWorker::spawn(root.clone(), db_path);

        worker.schedule(PollJob::ChatMessages);
        match wait_for_payload(&mut worker) {
            WorkerPayload::ChatMessages {
                messages, error, ..
            } => {
                assert!(error.is_none(), "unexpected error: {error:?}");
                let window = messages.expect("first poll materializes the window");
                assert!(window.is_empty(), "fresh store should be empty: {window:?}");
            }
            _ => panic!("expected a chat payload"),
        }

        // Second poll with an unchanged marker skips rematerializing.
        worker.schedule(PollJob::ChatMessages);
        match wait_for_payload(&mut worker) {
            WorkerPayload::ChatMessages {
                messages, error, ..
            } => {
                assert!(error.is_none(), "unexpected error: {error:?}");
                assert!(messages.is_none());
            }
            _ => panic!("expected a chat payload"),
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}

pub(super) fn run_update_subprocess(root: &Path, args: &[String]) -> Result<String> {
    let exe = std::env::current_exe().context("failed to resolve current ctox executable")?;
    let output = std::process::Command::new(exe)
        .args(args)
        .env("CTOX_ROOT", root)
        .output()
        .context("failed to spawn ctox subprocess")?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if output.status.success() {
        Ok(stdout)
    } else {
        anyhow::bail!("{}{}", stdout, stderr.trim())
    }
}
