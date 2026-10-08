// Origin: CTOX
// License: AGPL-3.0-only
//! First actual Core turn reuses the protected target child; it never boots a replacement.
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
    machine: Arc<target_machine::TargetMachine>,
    accepted: Job,
    complete: bool,
}
impl Drop for TargetTurnReady {
    fn drop(&mut self) {
        if self.complete {
            return;
        }
        // Invalidate synchronously before waiting on this owned controller.
        // Stop only the exact transferred child. Never clear a pending effect.
        self.machine.retire();
        if let Ok(registration) = self
            .execution
            .registry
            .registration(&self.execution.guest_id)
        {
            let mut entry = registration
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if entry
                .target_machine
                .as_ref()
                .is_some_and(|m| Arc::ptr_eq(m, &self.machine))
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
impl TargetTurnReady {
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
                        "target machine requires the actual original Core turn"
                    );
                    self.execution.with_held_worker_policy_guarded(
                        worker,
                        facts,
                        identity,
                        |entry, verify, _| {
                            ensure!(
                                entry
                                    .target_machine
                                    .as_ref()
                                    .is_some_and(|m| Arc::ptr_eq(m, &self.machine)),
                                "original target machine changed before turn binding"
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
            core_resume::validate_original_job(
                entry,
                entry
                    .restoration
                    .as_ref()
                    .context("original target enrollment missing")?,
                &current,
            )?;
            ensure!(
                entry.desktop.is_some()
                    && entry.desktop_io.is_some()
                    && entry.source_boot.is_none()
                    && entry.source_machine.is_none(),
                "target turn has no retained original desktop"
            );
            verify()?;
            Ok(entry
                .target_machine
                .clone()
                .context("original target machine missing")?)
        })?;
        Ok(Some(NativeGuestTurnReady::Target(TargetTurnReady {
            execution: self.clone(),
            machine,
            accepted: current,
            complete: false,
        })))
    }
}
