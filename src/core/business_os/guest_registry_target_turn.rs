// Origin: CTOX
// License: AGPL-3.0-only
//! Original target Core turns retain quorum/producer fences, with an optional protected machine.
use super::*;
use ctox_sync::authority::Job;

pub(crate) enum NativeGuestTurnReady {
    Source(source_boot::NativeSourceBootReady),
    Target(TargetTurnReady),
}
impl NativeGuestTurnReady {
    pub(crate) async fn commit_started(self, thread: &str, turn: &str) -> Result<()> {
        match self {
            Self::Source(ready) => ready.commit_started(thread, turn).await,
            Self::Target(ready) => ready.commit_started(thread, turn).await,
        }
    }
}
pub(crate) struct TargetTurnReady {
    execution: NativeGuestExecution,
    machine: Option<Arc<target_machine::TargetMachine>>,
    accepted: Job,
    complete: bool,
}
impl TargetTurnReady {
    fn matches_retained(&self, entry: &Registration) -> bool {
        let Some(protected) = &entry.restoration else {
            return false;
        };
        match (&self.machine, &entry.target_machine) {
            (Some(expected), Some(actual)) => {
                protected.service_session.is_some() && Arc::ptr_eq(expected, actual)
            }
            (None, None) => core_resume::require_core_only(entry, protected).is_ok(),
            _ => false,
        }
    }

    async fn commit_started(mut self, thread: &str, turn: &str) -> Result<()> {
        let current = self
            .execution
            .registry
            .authority
            .validate_ownership(
                &self.execution.binding.spec.job_id,
                &self.execution.binding.ownership,
            )
            .await?;
        ensure!(
            current == self.accepted,
            "target quorum changed before actual Core turn binding"
        );
        self.execution.with_native_issuer(|identity| {
            self.execution
                .provider
                .with_live_provider_transaction(|worker, facts, actual_turn| {
                    ensure!(
                        facts.provider_session_id == thread
                            && actual_turn == Some(turn)
                            && thread == self.execution.binding.spec.session_id,
                        "target requires the actual original Core turn"
                    );
                    self.execution.with_held_worker_policy_guarded(
                        worker,
                        facts,
                        identity,
                        |entry, verify, _| {
                            ensure!(
                                self.matches_retained(entry),
                                "original target runtime changed before turn binding"
                            );
                            core_resume::validate_original_job(
                                entry,
                                entry
                                    .restoration
                                    .as_ref()
                                    .context("original target enrollment missing")?,
                                &current,
                            )?;
                            verify()
                        },
                    )
                })
        })?;
        self.complete = true;
        Ok(())
    }
}
impl Drop for TargetTurnReady {
    fn drop(&mut self) {
        if self.complete {
            return;
        }
        // Invalidate synchronously. A Core-only attempt never creates/stops a child.
        if let Some(machine) = &self.machine {
            machine.retire();
        }
        if let Ok(registration) = self
            .execution
            .registry
            .registration(&self.execution.guest_id)
        {
            let mut entry = registration
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if self.matches_retained(&entry)
                && entry.execution.as_ref() == Some(&self.execution.binding)
            {
                entry.revoked = true;
                let _ = self.execution.registry.retire_frame(&mut entry);
                let io = entry.desktop_io.clone();
                if let Some(desktop) = entry.desktop.as_mut() {
                    if let Ok(status) = super::machine_io::run(io.as_deref(), desktop.stop()) {
                        entry.stopped_status = Some(status);
                    }
                }
            }
        }
    }
}
impl NativeGuestExecution {
    pub(crate) async fn start_current_guest_turn(&self) -> Result<Option<NativeGuestTurnReady>> {
        let target = self.with_current(|entry, verify| {
            verify()?;
            Ok(entry.restoration.is_some())
        })?;
        if !target {
            return Ok(self
                .start_configured_source()
                .await?
                .map(NativeGuestTurnReady::Source));
        }
        let current = self
            .registry
            .authority
            .validate_ownership(&self.binding.spec.job_id, &self.binding.ownership)
            .await?;
        let machine = self.with_current(|entry, verify| {
            let protected = entry
                .restoration
                .as_ref()
                .context("original target enrollment missing")?;
            core_resume::validate_original_job(entry, protected, &current)?;
            if protected.service_session.is_none() {
                core_resume::require_core_only(entry, protected)?;
                verify()?;
                return Ok(None);
            }
            ensure!(
                entry.desktop.is_some()
                    && entry.desktop_io.is_some()
                    && entry.source_boot.is_none()
                    && entry.source_machine.is_none(),
                "target turn has no retained original desktop"
            );
            verify()?;
            Ok(Some(
                entry
                    .target_machine
                    .clone()
                    .context("original target machine missing")?,
            ))
        })?;
        Ok(Some(NativeGuestTurnReady::Target(TargetTurnReady {
            execution: self.clone(),
            machine,
            accepted: current,
            complete: false,
        })))
    }
}
