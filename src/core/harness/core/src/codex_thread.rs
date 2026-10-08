use crate::agent::AgentStatus;
use crate::codex::Codex;
use crate::codex::SteerInputError;
use crate::config::ConstraintResult;
use crate::error::CodexErr;
use crate::error::Result as CodexResult;
use crate::features::Feature;
use crate::file_watcher::WatchRegistration;
use crate::protocol::Event;
use crate::protocol::Op;
use crate::protocol::Submission;
use ctox_protocol::config_types::ApprovalsReviewer;
use ctox_protocol::config_types::Personality;
use ctox_protocol::config_types::ServiceTier;
use ctox_protocol::models::ContentItem;
use ctox_protocol::models::ResponseInputItem;
use ctox_protocol::models::ResponseItem;
use ctox_protocol::openai_models::ReasoningEffort;
use ctox_protocol::protocol::AskForApproval;
use ctox_protocol::protocol::SandboxPolicy;
use ctox_protocol::protocol::SessionSource;
use ctox_protocol::protocol::TokenUsage;
use ctox_protocol::protocol::W3cTraceContext;
use ctox_protocol::user_input::UserInput;
use std::path::PathBuf;
use tokio::sync::Mutex;
use tokio::sync::watch;

use crate::state_db::StateDbHandle;

#[derive(Clone, Debug)]
pub struct ThreadConfigSnapshot {
    pub model: String,
    pub model_provider_id: String,
    pub service_tier: Option<ServiceTier>,
    pub approval_policy: AskForApproval,
    pub approvals_reviewer: ApprovalsReviewer,
    pub sandbox_policy: SandboxPolicy,
    pub cwd: PathBuf,
    pub ephemeral: bool,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub personality: Option<Personality>,
    pub session_source: SessionSource,
}

pub struct CodexThread {
    pub(crate) codex: Codex,
    rollout_path: Option<PathBuf>,
    out_of_band_elicitation_count: Mutex<u64>,
    _watch_registration: WatchRegistration,
}

/// Conduit for the bidirectional stream of messages that compose a thread
/// (formerly called a conversation) in Codex.
impl CodexThread {
    pub(crate) fn new(
        codex: Codex,
        rollout_path: Option<PathBuf>,
        watch_registration: WatchRegistration,
    ) -> Self {
        Self {
            codex,
            rollout_path,
            out_of_band_elicitation_count: Mutex::new(0),
            _watch_registration: watch_registration,
        }
    }

    /// Factory provenance only, before any submission. This grants no execution
    /// permit and reconciles no startup or unowned tool effects. Actual built-in
    /// Core handlers may subsequently receipt their own scoped effects.
    pub fn register_native_source_factory(&self) -> std::io::Result<()> {
        self.codex.session.native_effects.register_source_factory()
    }

    /// Register against this loaded Core Session, never a caller-supplied label.
    pub fn register_native_mcp_dispatch(
        &self,
        dispatcher: std::sync::Arc<dyn crate::native_mcp_dispatch::NativeMcpDispatch>,
    ) -> Result<crate::native_mcp_dispatch::NativeMcpRegistration, String> {
        crate::native_mcp_dispatch::register_native_mcp_dispatch(&self.codex.session, dispatcher)
    }

    /// Original initialize result from this loaded Session's managed connections.
    /// This is descriptive evidence only: a later ping, tool success or copied
    /// snapshot cannot reconcile startup, and this method changes no effect ledger.
    pub async fn native_original_mcp_startup(
        &self,
    ) -> std::io::Result<crate::native_mcp_startup::NativeMcpStartupSnapshot> {
        let manager = self
            .codex
            .session
            .services
            .mcp_connection_manager
            .read()
            .await;
        let servers = manager
            .native_original_startup(false)
            .await
            .map_err(std::io::Error::other)?;
        Ok(crate::native_mcp_startup::NativeMcpStartupSnapshot::new(
            self.codex.session.conversation_id,
            servers,
        ))
    }

    /// Trusted native factory verification of the original bounded handshake.
    /// The verifier runs after releasing the manager lock. Refresh/submission
    /// fences are checked synchronously afterwards; imported metadata has no path here.
    pub async fn reconcile_native_mcp_startup<F>(&self, verify: F) -> std::io::Result<()>
    where
        F: FnOnce(&crate::native_mcp_startup::NativeMcpStartupSnapshot) -> std::io::Result<()>,
    {
        let generation = self.codex.session.native_effects.mcp_generation()?;
        let servers = {
            let manager = self
                .codex
                .session
                .services
                .mcp_connection_manager
                .read()
                .await;
            manager
                .native_original_startup(true)
                .await
                .map_err(std::io::Error::other)?
        };
        let snapshot = crate::native_mcp_startup::NativeMcpStartupSnapshot::new(
            self.codex.session.conversation_id,
            servers,
        );
        verify(&snapshot)?;
        self.codex
            .session
            .native_effects
            .reconcile_mcp_startup(generation)
    }

    /// Verify this actual native restore input under the protected receiver's
    /// current fences. Clears only previous history, never MCP or new effects.
    pub fn reconcile_native_previous_session<F>(&self, verify: F) -> std::io::Result<()>
    where
        F: FnOnce(&crate::NativePreviousSessionSnapshot) -> std::io::Result<()>,
    {
        let snapshot = self
            .codex
            .session
            .native_effects
            .previous_snapshot(self.codex.session.conversation_id)?;
        verify(&snapshot)?;
        self.codex
            .session
            .native_effects
            .reconcile_previous(&snapshot)
    }

    pub async fn submit(&self, op: Op) -> CodexResult<String> {
        self.codex.submit(op).await
    }

    pub async fn interrupt_turn(&self, turn_id: String) -> CodexResult<bool> {
        self.codex.interrupt_turn(turn_id).await
    }

    /// Obtain the journal from this actual Core Session, never from a path claim.
    /// The returned reader remains sealed until successful recorder shutdown.
    pub async fn retain_native_journal(&self) -> std::io::Result<crate::NativeJournalReader> {
        self.codex.session.retain_native_journal().await
    }

    pub async fn shutdown_and_wait(&self) -> CodexResult<()> {
        self.codex.shutdown_and_wait().await
    }

    pub async fn submit_with_trace(
        &self,
        op: Op,
        trace: Option<W3cTraceContext>,
    ) -> CodexResult<String> {
        self.codex.submit_with_trace(op, trace).await
    }

    pub async fn steer_input(
        &self,
        input: Vec<UserInput>,
        expected_turn_id: Option<&str>,
    ) -> Result<String, SteerInputError> {
        self.codex.steer_input(input, expected_turn_id).await
    }

    pub async fn set_app_server_client_name(
        &self,
        app_server_client_name: Option<String>,
    ) -> ConstraintResult<()> {
        self.codex
            .set_app_server_client_name(app_server_client_name)
            .await
    }

    /// Use sparingly: this is intended to be removed soon.
    pub async fn submit_with_id(&self, sub: Submission) -> CodexResult<()> {
        self.codex.submit_with_id(sub).await
    }

    pub async fn next_event(&self) -> CodexResult<Event> {
        self.codex.next_event().await
    }

    pub async fn agent_status(&self) -> AgentStatus {
        self.codex.agent_status().await
    }

    pub(crate) fn subscribe_status(&self) -> watch::Receiver<AgentStatus> {
        self.codex.agent_status.clone()
    }

    pub(crate) async fn total_token_usage(&self) -> Option<TokenUsage> {
        self.codex.session.total_token_usage().await
    }

    /// Records a user-role session-prefix message without creating a new user turn boundary.
    pub(crate) async fn inject_user_message_without_turn(&self, message: String) {
        let pending_item = ResponseInputItem::Message {
            role: "user".to_string(),
            content: vec![ContentItem::InputText { text: message }],
        };
        let pending_items = vec![pending_item];
        let Err(items_without_active_turn) = self
            .codex
            .session
            .inject_response_items(pending_items)
            .await
        else {
            return;
        };

        let turn_context = self.codex.session.new_default_turn().await;
        let items: Vec<ResponseItem> = items_without_active_turn
            .into_iter()
            .map(ResponseItem::from)
            .collect();
        self.codex
            .session
            .record_conversation_items(turn_context.as_ref(), &items)
            .await;
    }

    pub fn rollout_path(&self) -> Option<PathBuf> {
        self.rollout_path.clone()
    }

    pub fn state_db(&self) -> Option<StateDbHandle> {
        self.codex.state_db()
    }

    pub async fn rollout_materialization_pending(&self) -> bool {
        self.codex.rollout_materialization_pending().await
    }

    /// Actual source state after successful session-loop and recorder shutdown.
    /// A closed channel, path claim or caller-provided JSON cannot construct it.
    pub async fn capture_native_state(
        &self,
    ) -> std::io::Result<(ThreadConfigSnapshot, crate::NativeSessionState)> {
        self.codex.capture_native_state().await
    }

    pub async fn config_snapshot(&self) -> ThreadConfigSnapshot {
        self.codex.thread_config_snapshot().await
    }

    pub fn enabled(&self, feature: Feature) -> bool {
        self.codex.enabled(feature)
    }

    pub async fn increment_out_of_band_elicitation_count(&self) -> CodexResult<u64> {
        let mut guard = self.out_of_band_elicitation_count.lock().await;
        let was_zero = *guard == 0;
        *guard = guard.checked_add(1).ok_or_else(|| {
            CodexErr::Fatal("out-of-band elicitation count overflowed".to_string())
        })?;

        if was_zero {
            self.codex
                .session
                .set_out_of_band_elicitation_pause_state(/*paused*/ true);
        }

        Ok(*guard)
    }

    pub async fn decrement_out_of_band_elicitation_count(&self) -> CodexResult<u64> {
        let mut guard = self.out_of_band_elicitation_count.lock().await;
        if *guard == 0 {
            return Err(CodexErr::InvalidRequest(
                "out-of-band elicitation count is already zero".to_string(),
            ));
        }

        *guard -= 1;
        let now_zero = *guard == 0;
        if now_zero {
            self.codex
                .session
                .set_out_of_band_elicitation_pause_state(/*paused*/ false);
        }

        Ok(*guard)
    }
}
