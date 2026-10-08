// Origin: CTOX
// License: AGPL-3.0-only

//! Exact observed quorum effects are protected capture input, never a clean-effect permit.
use super::*;
use ctox_sync::authority::Job;
use ctox_sync::contracts::PendingEffect;

/// No wire decoder or renderer can construct an observed authority snapshot.
pub(super) struct SourceEffects {
    job: Job,
    process: Option<GuestProcessEffect>,
    child_stop_observed: bool,
    process_effect_reconciled: bool,
    machine_entries: Vec<ctox_sync::contracts::WorkspaceEntry>,
    core_effects: Option<ctox_core::NativeCoreEffectCapture>,
}

impl SourceEffects {
    pub(super) fn observe(execution: &NativeGuestExecution) -> Result<Self> {
        ensure!(
            tokio::runtime::Handle::try_current().is_err(),
            "native effect capture requires its synchronous source owner"
        );
        super::super::guest_commands::block_on_guest(Self::read_authority(
            execution.registry.authority.as_ref(),
            &execution.binding.spec,
            &execution.binding.ownership,
        ))
    }

    pub(super) async fn read_authority(
        authority: &dyn ExecutionAuthority,
        spec: &ExecutionSpec,
        ownership: &Ownership,
    ) -> Result<Self> {
        let job = tokio::time::timeout(
            Duration::from_secs(15),
            authority.validate_ownership(&spec.job_id, ownership),
        )
        .await
        .context("native source effect authority deadline")??;
        Self::checked(job, spec, ownership)
    }

    fn checked(job: Job, spec: &ExecutionSpec, ownership: &Ownership) -> Result<Self> {
        ensure!(
            job.spec == *spec && job.ownership == *ownership && !job.stopped,
            "native source quorum ownership is no longer current"
        );
        ensure!(
            job.pending_effects.len() <= 256
                && job
                    .pending_effects
                    .iter()
                    .all(|id| id.len() <= 256 && identifier(id))
                && job.pending_effects.is_disjoint(&job.completed_effects),
            "native source effect observation is invalid or exceeds its bound"
        );
        Ok(Self {
            job,
            process: None,
            child_stop_observed: false,
            process_effect_reconciled: false,
            machine_entries: Vec::new(),
            core_effects: None,
        })
    }

    /// Called under the actual source worker/account/policy/controller fences,
    /// after the authority read. It does not stop a guest or complete an effect.
    pub(super) fn verify_controller(&mut self, entry: &Registration) -> Result<()> {
        self.process = None;
        self.child_stop_observed = false;
        self.process_effect_reconciled = false;
        self.machine_entries.clear();
        match (&entry.process_effect, &entry.registered_process) {
            (None, None) => {
                #[cfg(target_os = "linux")]
                ensure!(
                    entry.desktop.is_none()
                        && entry.source_machine.is_none()
                        && entry.stopped_status.is_none(),
                    "native source child has no registered effect"
                );
            }
            (Some(id), Some(process)) => {
                let destination = &entry.assignment.destination;
                ensure!(
                    id == &process.effect_id
                        && process.job_id == self.job.spec.job_id
                        && process.ownership == self.job.ownership
                        && process.controller_id == destination.controller_id
                        && process.controller_generation == destination.controller_generation,
                    "native source child differs from the observed quorum effect"
                );
                #[cfg(target_os = "linux")]
                if let Some(capture) = &entry.source_machine {
                    ensure!(
                        entry.desktop.is_none() && capture.matches(process),
                        "native source export differs from its registered process"
                    );
                    self.machine_entries = capture.entries()?;
                    self.child_stop_observed = true;
                    self.process_effect_reconciled = capture.process_reconciled(process)?;
                } else {
                    ensure!(
                        entry
                            .desktop
                            .as_ref()
                            .is_some_and(|desktop| desktop.process_instance_id()
                                == process.process_instance_id),
                        "native source child differs from its retained process"
                    );
                }
                #[cfg(not(target_os = "linux"))]
                anyhow::bail!("native source child capture requires retained Linux QEMU");
                ensure!(
                    self.job.pending_effects.contains(id)
                        || (self.process_effect_reconciled
                            && self.job.completed_effects.contains(id)),
                    "native source child has no matching current quorum effect"
                );
            }
            _ => anyhow::bail!("native source process effect is incomplete"),
        }
        self.process = entry.registered_process.clone();
        #[cfg(target_os = "linux")]
        {
            self.child_stop_observed |= entry.stopped_status.is_some();
        }
        Ok(())
    }

    pub(super) fn bind_core_state(&mut self, state: &ctox_core::NativeSessionState) -> Result<()> {
        ensure!(
            state.session_id().to_string() == self.job.spec.session_id
                && state.model() == self.job.spec.model_id
                && state.provider_id() == self.job.spec.model_route_id,
            "native Core effects belong to another source"
        );
        self.core_effects = state.core_effect_capture().cloned();
        Ok(())
    }

    pub(super) fn machine_entries(&self) -> &[ctox_sync::contracts::WorkspaceEntry] {
        &self.machine_entries
    }

    pub(super) fn same_quorum(&self, other: &Self) -> bool {
        self.job == other.job
    }

    pub(super) fn quorum_job(&self) -> &Job {
        &self.job
    }

    pub(super) fn same_observation(&self, other: &Self) -> bool {
        self.job == other.job
            && self.process == other.process
            && self.child_stop_observed == other.child_stop_observed
            && self.process_effect_reconciled == other.process_effect_reconciled
            && self.machine_entries == other.machine_entries
            && self.core_effects.as_ref().map(|c| c.report())
                == other.core_effects.as_ref().map(|c| c.report())
    }

    pub(super) fn bytes(&self, spec: &ExecutionSpec, ownership: &Ownership) -> Result<Vec<u8>> {
        ensure!(
            self.job.spec == *spec && self.job.ownership == *ownership,
            "native source effect observation belongs to another capture"
        );
        // Absence of quorum effects proves nothing about shell, MCP or guest
        // external effects. This snapshot deliberately grants no reconciliation.
        let bytes = serde_json::to_vec(&serde_json::json!({
            "version": 1, "jobId": spec.job_id, "sessionId": spec.session_id,
            "ownership": ownership, "observedPendingEffects": self.job.pending_effects,
            "registeredGuestProcess": self.process.as_ref().map(|process| serde_json::json!({
                "effectId": process.effect_id, "jobId": process.job_id,
                "ownership": process.ownership, "controllerId": process.controller_id,
                "controllerGeneration": process.controller_generation,
                "processInstanceId": process.process_instance_id,
                "childStopObserved": self.child_stop_observed,
                "processEffectReconciled": self.process_effect_reconciled
            })),
            "coreEffects": self.core_effects.as_ref().map(|capture| capture.report()),
            "externalEffects": "unknown", "reconciled": false
        }))?;
        ensure!(
            bytes.len() <= 128 * 1024,
            "native source effect snapshot exceeds its bound"
        );
        Ok(bytes)
    }

    pub(super) fn pending(&self, capture_id: &str) -> Result<Vec<PendingEffect>> {
        ensure!(
            identifier(capture_id),
            "invalid native source capture identity"
        );
        let unknown = format!("native-effects-{capture_id}");
        ensure!(
            !self.job.pending_effects.contains(&unknown),
            "native source effect identity collides with capture uncertainty"
        );
        let mut pending: Vec<_> = self
            .job
            .pending_effects
            .iter()
            .map(|id| PendingEffect {
                effect_id: id.clone(),
                idempotency_key: None,
                description: "Observed native quorum effect requires reconciliation".into(),
            })
            .collect();
        pending.push(PendingEffect {
            effect_id: unknown,
            idempotency_key: None,
            description: "Native turn external effects require reconciliation".into(),
        });
        Ok(pending)
    }

    #[cfg(test)]
    pub(super) fn from_observation_fixture(
        job: Job,
        spec: &ExecutionSpec,
        ownership: &Ownership,
    ) -> Result<Self> {
        Self::checked(job, spec, ownership)
    }

    #[cfg(test)]
    pub(super) fn fixture(
        spec: &ExecutionSpec,
        ownership: &Ownership,
        pending: BTreeSet<String>,
    ) -> Self {
        Self::checked(
            Job {
                spec: spec.clone(),
                ownership: ownership.clone(),
                checkpoint: None,
                checkpoint_requires_refresh: true,
                pending_effects: pending,
                completed_effects: BTreeSet::new(),
                stopped: false,
            },
            spec,
            ownership,
        )
        .unwrap()
    }
}
