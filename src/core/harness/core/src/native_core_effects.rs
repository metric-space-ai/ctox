//! Observations from the actual Core execution paths, not a wire clean-effect flag.
use crate::config::Config;
use crate::features::Feature;
use ctox_protocol::{ThreadId, models::ResponseItem, protocol::Op};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    sync::Mutex,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeCoreEffectReport {
    version: u32,
    session_id: ThreadId,
    source_factory_registered: bool,
    startup_uncertainties: BTreeSet<String>,
    submissions: u64,
    unreconciled_observations: u64,
}

impl NativeCoreEffectReport {
    pub(crate) fn validate_metadata(&self, session: ThreadId) -> io::Result<()> {
        if self.version != 1
            || self.session_id != session
            || self.startup_uncertainties.len() > 32
            || self.startup_uncertainties.iter().any(|s| {
                s.is_empty()
                    || s.len() > 128
                    || !s
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
            })
        {
            return Err(io::Error::other("invalid native Core effect metadata"));
        }
        Ok(())
    }
}

/// Only checked shutdown of this actual Core Session constructs this capture.
/// Deserializing a report or importing checkpoint context cannot construct it.
#[derive(Clone)]
pub struct NativeCoreEffectCapture {
    report: NativeCoreEffectReport,
}
impl NativeCoreEffectCapture {
    pub fn report(&self) -> &NativeCoreEffectReport {
        &self.report
    }
    pub fn session_id(&self) -> ThreadId {
        self.report.session_id
    }
    pub fn requires_reconciliation(&self) -> bool {
        !self.report.source_factory_registered
            || !self.report.startup_uncertainties.is_empty()
            || self.report.unreconciled_observations != 0
    }
}

struct Observations {
    registered: bool,
    startup: BTreeSet<String>,
    submissions: u64,
    mcp_generation: u64,
    unreconciled: u64,
    calls: BTreeMap<String, PlanObservation>,
}

#[derive(Default)]
struct PlanObservation {
    added: u8,
    done: u8,
    dispatched: u8,
    eligible: bool,
    observations: u64,
    completed: bool,
}
pub(crate) struct NativeCoreEffects {
    observations: Mutex<Observations>,
}
impl NativeCoreEffects {
    pub(crate) fn new(config: &Config, fresh_history: bool) -> Self {
        let mut startup = BTreeSet::new();
        if !fresh_history {
            startup.insert("previous-session-effects".into());
        }
        for feature in [
            Feature::ShellSnapshot,
            Feature::ShellZshFork,
            Feature::CodexHooks,
            Feature::MemoryTool,
            Feature::GhostCommit,
            Feature::Plugins,
            Feature::SkillMcpDependencyInstall,
            Feature::SkillEnvVarDependencyPrompt,
            Feature::JsRepl,
            Feature::CodeMode,
            Feature::CodeModeOnly,
            Feature::JsReplToolsOnly,
            Feature::RealtimeConversation,
        ] {
            if config.features.enabled(feature) {
                startup.insert(feature.key().to_owned());
            }
        }
        if config
            .notify
            .as_ref()
            .is_some_and(|args| !args.is_empty() && !args[0].is_empty())
        {
            startup.insert("legacy-notify".into());
        }
        if config.permissions.network.is_some() {
            startup.insert("managed-network-startup".into());
        }
        Self {
            observations: Mutex::new(Observations {
                registered: false,
                startup,
                submissions: 0,
                mcp_generation: 0,
                unreconciled: 0,
                calls: BTreeMap::new(),
            }),
        }
    }

    pub(crate) fn observe_mcp_startup(&self, enabled_servers: usize) {
        if enabled_servers != 0 {
            if let Ok(mut state) = self.observations.lock() {
                // Every fresh connection invalidates the previous receipt.
                state.mcp_generation = state.mcp_generation.saturating_add(1);
                state.startup.insert("mcp-startup".into());
            }
        }
    }

    /// Refresh fences receipts synchronously, before startup's first await.
    pub(crate) fn mcp_generation(&self) -> io::Result<u64> {
        self.observations
            .lock()
            .map(|state| state.mcp_generation)
            .map_err(|_| io::Error::other("native Core effect ledger poisoned"))
    }

    pub(crate) fn reconcile_mcp_startup(&self, generation: u64) -> io::Result<()> {
        let mut state = self
            .observations
            .lock()
            .map_err(|_| io::Error::other("native Core effect ledger poisoned"))?;
        if !state.registered
            || state.submissions != 0
            || state.unreconciled != 0
            || state.mcp_generation != generation
            || !state.startup.contains("mcp-startup")
        {
            return Err(io::Error::other(
                "native MCP startup receipt is late, repeated or stale",
            ));
        }
        state.startup.remove("mcp-startup");
        Ok(())
    }

    pub(crate) fn register_source_factory(&self) -> io::Result<()> {
        let mut state = self
            .observations
            .lock()
            .map_err(|_| io::Error::other("native Core effect ledger poisoned"))?;
        if state.registered || state.submissions != 0 || state.unreconciled != 0 {
            return Err(io::Error::other(
                "native source factory registration is late or repeated",
            ));
        }
        state.registered = true;
        Ok(())
    }

    pub(crate) fn observe_submission(&self, op: &Op) {
        if let Ok(mut state) = self.observations.lock() {
            state.submissions = state.submissions.saturating_add(1);
            if !matches!(
                op,
                Op::UserInput { .. } | Op::UserTurn { .. } | Op::Shutdown
            ) {
                state.unreconciled = state.unreconciled.saturating_add(1);
            }
        }
    }

    /// Called synchronously BEFORE the first await at an unowned boundary.
    /// Cancellation, handler rejection and apparent success never erase it.
    pub(crate) fn observe_unreconciled(&self) {
        if let Ok(mut state) = self.observations.lock() {
            state.unreconciled = state.unreconciled.saturating_add(1);
        }
    }

    pub(crate) fn observe_provider_item(&self, item: &ResponseItem, completed: bool) {
        if let ResponseItem::FunctionCall {
            name,
            namespace,
            call_id,
            ..
        } = item
        {
            self.observe_plan_boundary(
                call_id,
                name == "update_plan" && namespace.is_none(),
                if completed { 1 } else { 0 },
            );
            return;
        }
        if !matches!(
            item,
            ResponseItem::Message { .. } | ResponseItem::Reasoning { .. }
        ) {
            self.observe_unreconciled();
        }
    }

    pub(crate) fn observe_tool_dispatch(&self, call_id: &str, eligible_plan: bool) {
        self.observe_plan_boundary(call_id, eligible_plan, 2);
    }

    fn observe_plan_boundary(&self, call_id: &str, eligible: bool, boundary: u8) {
        if let Ok(mut state) = self.observations.lock() {
            state.unreconciled = state.unreconciled.saturating_add(1);
            // Ordinary sessions retain the conservative counter without allocating
            // per-call state. An imported report cannot register this live ledger.
            if !state.registered
                || call_id.is_empty()
                || call_id.len() > 256
                || (!state.calls.contains_key(call_id) && state.calls.len() >= 4096)
            {
                return;
            }
            let call = state
                .calls
                .entry(call_id.to_owned())
                .or_insert_with(|| PlanObservation {
                    eligible: true,
                    ..Default::default()
                });
            call.eligible &= eligible && !call.completed;
            call.observations = call.observations.saturating_add(1);
            match boundary {
                0 => call.added = call.added.saturating_add(1),
                1 => call.done = call.done.saturating_add(1),
                _ => call.dispatched = call.dispatched.saturating_add(1),
            }
        }
    }

    /// Only the actual built-in plan handler calls this after its typed update.
    /// A tool name, result, copied report, failed handler or cancellation is not
    /// a receipt. Completed IDs stay retained to reject replay/cross-turn reuse.
    pub(crate) fn complete_core_plan(&self, call_id: &str) {
        if let Ok(mut state) = self.observations.lock() {
            let registered = state.registered;
            let Some(call) = state.calls.get_mut(call_id) else {
                return;
            };
            if !registered
                || !call.eligible
                || call.completed
                || call.added > 1
                || call.done != 1
                || call.dispatched != 1
            {
                return;
            }
            call.completed = true;
            let observations = call.observations;
            // Other calls and unowned startup/submission observations are untouched.
            if let Some(remaining) = state.unreconciled.checked_sub(observations) {
                state.unreconciled = remaining;
            }
        }
    }

    /// Caller has checked submission-loop/journal shutdown and no active turn.
    pub(crate) fn capture(&self, session_id: ThreadId) -> io::Result<NativeCoreEffectCapture> {
        let state = self
            .observations
            .lock()
            .map_err(|_| io::Error::other("native Core effect ledger poisoned"))?;
        Ok(NativeCoreEffectCapture {
            report: NativeCoreEffectReport {
                version: 1,
                session_id,
                source_factory_registered: state.registered,
                startup_uncertainties: state.startup.clone(),
                submissions: state.submissions,
                unreconciled_observations: state.unreconciled,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn quiet() -> NativeCoreEffects {
        NativeCoreEffects {
            observations: Mutex::new(Observations {
                registered: false,
                startup: BTreeSet::new(),
                submissions: 0,
                mcp_generation: 0,
                unreconciled: 0,
                calls: BTreeMap::new(),
            }),
        }
    }
    #[test]
    fn no_observations_are_not_a_factory_receipt() {
        let ledger = quiet();
        assert!(
            ledger
                .capture(ThreadId::default())
                .unwrap()
                .requires_reconciliation()
        );
        ledger.register_source_factory().unwrap();
        assert!(
            !ledger
                .capture(ThreadId::default())
                .unwrap()
                .requires_reconciliation()
        );
    }
    #[test]
    fn cancellation_success_and_registration_cannot_erase_unknown_effects() {
        let ledger = quiet();
        ledger.register_source_factory().unwrap();
        ledger.observe_unreconciled();
        assert!(
            ledger
                .capture(ThreadId::default())
                .unwrap()
                .requires_reconciliation()
        );
        assert!(ledger.register_source_factory().is_err());
        let early = quiet();
        early.observe_unreconciled();
        assert!(early.register_source_factory().is_err());
    }
    #[test]
    fn startup_and_resume_uncertainty_survives_quiescence() {
        let ledger = quiet();
        ledger.observe_mcp_startup(1);
        ledger.register_source_factory().unwrap();
        let capture = ledger.capture(ThreadId::default()).unwrap();
        assert!(capture.requires_reconciliation());
        assert!(capture.report.startup_uncertainties.contains("mcp-startup"));
    }
    #[test]
    fn raw_shell_and_reconfiguration_are_not_safe_submissions() {
        for op in [
            Op::RunUserShellCommand {
                command: "false".into(),
            },
            Op::ReloadUserConfig,
        ] {
            let ledger = quiet();
            ledger.register_source_factory().unwrap();
            ledger.observe_submission(&op);
            assert!(
                ledger
                    .capture(ThreadId::default())
                    .unwrap()
                    .requires_reconciliation()
            );
        }
        let late = quiet();
        late.observe_submission(&Op::Shutdown);
        assert!(late.register_source_factory().is_err());
    }
    #[test]
    fn native_startup_reconciliation_is_factory_bound_and_refresh_fenced() {
        let ledger = quiet();
        ledger.observe_mcp_startup(1);
        let first = ledger.mcp_generation().unwrap();
        assert!(ledger.reconcile_mcp_startup(first).is_err());
        ledger.register_source_factory().unwrap();
        ledger.observe_mcp_startup(1);
        assert!(ledger.reconcile_mcp_startup(first).is_err());
        let current = ledger.mcp_generation().unwrap();
        ledger.reconcile_mcp_startup(current).unwrap();
        assert!(ledger.reconcile_mcp_startup(current).is_err());
        assert!(
            !ledger
                .capture(ThreadId::default())
                .unwrap()
                .requires_reconciliation()
        );
        ledger.observe_mcp_startup(1);
        assert!(
            ledger
                .capture(ThreadId::default())
                .unwrap()
                .requires_reconciliation()
        );
        ledger.observe_unreconciled();
        assert!(
            ledger
                .reconcile_mcp_startup(ledger.mcp_generation().unwrap())
                .is_err()
        );
        let submitted = quiet();
        submitted.observe_mcp_startup(1);
        submitted.register_source_factory().unwrap();
        submitted.observe_submission(&Op::Shutdown);
        assert!(
            submitted
                .reconcile_mcp_startup(submitted.mcp_generation().unwrap())
                .is_err()
        );
    }

    #[test]
    fn poison_is_a_failed_capture_not_a_clean_snapshot() {
        let ledger = quiet();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _lock = ledger.observations.lock().unwrap();
            panic!("fixture ledger poison");
        }));
        ledger.observe_unreconciled();
        assert!(ledger.capture(ThreadId::default()).is_err());
        assert!(ledger.register_source_factory().is_err());
    }
}
