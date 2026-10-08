// Origin: CTOX
// License: AGPL-3.0-only
//! First source guest boot from the actual admitted worker, not an import shortcut.
use super::super::guest_runtime::{QemuOverlayPreparation, RetainedQemuDesktop};
use super::machine_io::MachineIo;
use super::*;
use ctox_sync::authority::{Command, Job, Receipt, Request};
use tokio::sync::watch;
#[cfg(test)]
#[path = "guest_registry_source_boot_tests.rs"]
mod tests;

pub(super) struct SourceBoot {
    retired: watch::Sender<bool>,
    helper: Mutex<Option<QemuOverlayPreparation>>,
    child: Mutex<Option<RetainedQemuDesktop>>,
    io: Arc<MachineIo>,
}
impl SourceBoot {
    fn new() -> Result<Self> {
        let (retired, _) = watch::channel(false);
        Ok(Self {
            retired,
            helper: Mutex::new(None),
            child: Mutex::new(None),
            io: Arc::new(MachineIo::new()?),
        })
    }
    pub(super) fn retire(&self) {
        self.retired.send_replace(true);
    }
    pub(super) fn current(&self) -> Result<()> {
        ensure!(
            !*self.retired.borrow(),
            "source boot retired; reconcile the retained attempt"
        );
        Ok(())
    }
    // Called with actual native publication guards; retain helper before awaits.
    fn prepare(&self, config: &NativeGuestMachineConfiguration, parent: &Path) -> Result<()> {
        use std::os::unix::fs::DirBuilderExt;
        self.current()?;
        let program = config.image_program()?;
        let prepared = config.prepared(parent.into(), parent.join("root.qcow2"))?;
        let mut helper = self
            .helper
            .lock()
            .map_err(|_| anyhow::anyhow!("source helper poisoned"))?;
        ensure!(
            helper.is_none(),
            "source disk preparation already attempted"
        );
        std::fs::DirBuilder::new().mode(0o700).create(parent)?;
        private_directory(parent)?;
        self.current()?;
        let slot = &mut *helper;
        self.io.run(|_| {
            *slot = Some(QemuOverlayPreparation::start(
                &program,
                parent,
                &prepared.base_raw,
            )?);
            Ok(())
        })
    }
    fn finish(&self) -> Result<PathBuf> {
        let mut helper = self
            .helper
            .lock()
            .map_err(|_| anyhow::anyhow!("source helper poisoned"))?;
        self.current()?;
        let helper = helper.as_mut().context("source disk preparation absent")?;
        let mut retired = self.retired.subscribe();
        self.current()?;
        let path = self.io.run(|runtime| {
            runtime.block_on(async {
                tokio::select! {
                    biased;
                    _ = retired.changed() => anyhow::bail!("source disk preparation revoked"),
                    result = helper.finish() => result,
                }
            })
        })?;
        self.current()?;
        Ok(path)
    }
    pub(super) fn stop_pending_child(&self) -> Result<Option<std::process::ExitStatus>> {
        self.retire();
        let mut child = self
            .child
            .lock()
            .map_err(|_| anyhow::anyhow!("source child poisoned"))?;
        child
            .as_mut()
            .map(|child| self.io.run(|runtime| runtime.block_on(child.stop())))
            .transpose()
    }
    fn retain_child(
        &self,
        config: &super::super::guest_runtime::PreparedQemuGuest,
        guest: &str,
    ) -> Result<String> {
        self.current()?;
        let mut child = self
            .child
            .lock()
            .map_err(|_| anyhow::anyhow!("source child poisoned"))?;
        ensure!(child.is_none(), "source child already attempted");
        let actual = self
            .io
            .run(|_| RetainedQemuDesktop::spawn_paused(config, guest.into()))?;
        let id = actual.process_instance_id().to_owned();
        *child = Some(actual);
        Ok(id)
    }
    // No native account/controller/worker/SQLite lock crosses these machine awaits.
    fn boot(&self) -> Result<GuestLiveEndpoint> {
        let mut child = self
            .child
            .lock()
            .map_err(|_| anyhow::anyhow!("source child poisoned"))?;
        self.current()?;
        let child = child.as_mut().context("source child absent")?;
        let mut retired = self.retired.subscribe();
        self.current()?;
        let endpoint = self.io.run(|runtime| {
            runtime.block_on(async {
                tokio::select! {
                    biased;
                    _ = retired.changed() => anyhow::bail!("source guest boot retired"),
                    result = tokio::time::timeout(Duration::from_secs(60), child.boot()) =>
                        result.context("source guest boot deadline")?,
                }
            })
        })?;
        self.current()?;
        ensure!(
            endpoint.process_instance_id == child.process_instance_id(),
            "source endpoint differs from actual child"
        );
        Ok(endpoint)
    }
    fn publish(&self, entry: &mut Registration, endpoint: &GuestLiveEndpoint) -> Result<()> {
        self.current()?;
        let mut child = self
            .child
            .lock()
            .map_err(|_| anyhow::anyhow!("source child poisoned"))?;
        let actual = child.as_ref().context("source child absent")?;
        ensure!(
            actual.process_instance_id() == endpoint.process_instance_id && entry.desktop.is_none(),
            "source child changed before publication"
        );
        entry.desktop = child.take();
        entry.desktop_io = Some(self.io.clone());
        Ok(())
    }
    pub(super) fn stop_helper(&self) -> Result<()> {
        self.retire();
        let mut helper = self
            .helper
            .lock()
            .map_err(|_| anyhow::anyhow!("source helper poisoned"))?;
        if let Some(helper) = helper.as_mut() {
            self.io.run(|runtime| runtime.block_on(helper.abort()))?;
        }
        Ok(())
    }
}
pub(crate) struct NativeSourceBootReady {
    attempt: BootAttempt,
    accepted: Job,
    effect: String,
    endpoint: GuestLiveEndpoint,
}
impl NativeSourceBootReady {
    /// Keep cancellation ownership until the actual producer binds TurnStart.
    pub(crate) async fn commit_started(mut self, thread: &str, turn: &str) -> Result<()> {
        let execution = &self.attempt.execution;
        let current = execution
            .registry
            .authority
            .validate_ownership(&execution.binding.spec.job_id, &execution.binding.ownership)
            .await?;
        validate_job(
            &current,
            &execution.binding.spec,
            &execution.binding.ownership,
            &self.effect,
        )?;
        ensure!(
            current == self.accepted,
            "source quorum changed before actual TurnStart binding"
        );
        execution
            .provider
            .with_live_provider_transaction(|worker, facts, actual_turn| {
                ensure!(
                    facts.provider_session_id == thread && actual_turn == Some(turn),
                    "source machine cannot be released by a claimed/unbound Core turn"
                );
                execution.with_held_worker(worker, facts, |entry, verify| {
                    matches(entry, &self.attempt.boot, &self.effect)?;
                    ensure!(
                        entry
                            .registered_process
                            .as_ref()
                            .is_some_and(|process| process.process_instance_id
                                == self.endpoint.process_instance_id),
                        "source child changed before actual Core binding"
                    );
                    verify()
                })
            })?;
        self.attempt.complete = true;
        Ok(())
    }
}
struct BootAttempt {
    boot: Arc<SourceBoot>,
    execution: NativeGuestExecution,
    complete: bool,
}
impl Drop for BootAttempt {
    fn drop(&mut self) {
        if self.complete {
            return;
        }
        self.boot.retire(); // invalidate before waiting for any controller/helper lock
        let _ = self.boot.stop_helper();
        let _ = self.boot.stop_pending_child();
        // No worker/account permission is needed to terminate only this actual
        // retained attempt. Neither helper exit nor child stop completes an effect.
        if let Ok(registration) = self
            .execution
            .registry
            .registration(&self.execution.guest_id)
        {
            if let Ok(mut entry) = registration.lock() {
                if entry
                    .source_boot
                    .as_ref()
                    .is_some_and(|boot| Arc::ptr_eq(boot, &self.boot))
                {
                    entry.revoked = true;
                    if let Some(desktop) = entry.desktop.as_mut() {
                        if let Ok(status) =
                            self.boot.io.run(|runtime| runtime.block_on(desktop.stop()))
                        {
                            entry.stopped_status = Some(status);
                        }
                    }
                }
            }
        }
    }
}
fn validate_job(
    job: &Job,
    spec: &ExecutionSpec,
    ownership: &Ownership,
    effect: &str,
) -> Result<()> {
    ensure!(
        job.spec == *spec
            && job.ownership == *ownership
            && !job.stopped
            && job.checkpoint.is_none()
            && job.pending_effects == BTreeSet::from([effect.to_owned()])
            && !job.completed_effects.contains(effect),
        "source boot differs from the actual fresh job/process reservation"
    );
    Ok(())
}
fn matches(entry: &Registration, boot: &Arc<SourceBoot>, effect: &str) -> Result<()> {
    boot.current()?;
    ensure!(
        entry
            .source_boot
            .as_ref()
            .is_some_and(|actual| Arc::ptr_eq(actual, boot))
            && entry.process_effect.as_deref() == Some(effect)
            && entry.restoration.is_none()
            && entry.imported.is_none()
            && entry.source_machine.is_none()
            && entry.target_machine.is_none(),
        "source boot controller/attempt changed"
    );
    Ok(())
}
impl NativeGuestExecution {
    /// A missing optional machine config keeps ordinary native Core execution.
    /// It cannot produce VM capture/readiness. Configured boot failures poison
    /// the actual source attempt; there is no replacement child or legacy fallback.
    pub(crate) async fn start_configured_source(&self) -> Result<Option<NativeSourceBootReady>> {
        let config = self
            .registry
            .machine_configuration
            .lock()
            .map_err(|_| anyhow::anyhow!("machine configuration poisoned"))?
            .clone();
        let Some(config) = config else {
            return Ok(None);
        };
        config.validate()?;
        let effect = format!("guest-process:{}", uuid::Uuid::new_v4());
        let boot = Arc::new(SourceBoot::new()?);
        let assignment = self.with_current(|entry, verify| {
            ensure!(
                entry.restoration.is_none()
                    && entry.imported.is_none()
                    && entry.desktop.is_none()
                    && entry.source_boot.is_none()
                    && entry.source_machine.is_none()
                    && entry.target_machine.is_none()
                    && entry.process_effect.is_none()
                    && entry.registered_process.is_none(),
                "fresh source boot requires an unused actual admitted source"
            );
            verify()?;
            entry.source_boot = Some(boot.clone());
            entry.process_effect = Some(effect.clone());
            Ok(entry.assignment.clone())
        })?;
        let attempt = BootAttempt {
            boot: boot.clone(),
            execution: self.clone(),
            complete: false,
        };
        let receipt = self
            .registry
            .authority
            .submit(Request {
                request_id: format!("{effect}:begin"),
                actor: self.registry.authority.node_id(),
                command: Command::BeginEffect {
                    job_id: self.binding.spec.job_id.clone(),
                    ownership: self.binding.ownership.clone(),
                    effect_id: effect.clone(),
                },
            })
            .await?;
        let Receipt::Applied(accepted) = receipt else {
            anyhow::bail!("source process BeginEffect uncertain/replayed; reconcile");
        };
        validate_job(
            &accepted,
            &self.binding.spec,
            &self.binding.ownership,
            &effect,
        )?;
        let execution = self.clone();
        let b = boot.clone();
        let c = config.clone();
        let parent = assignment.runtime_parent();
        let p = parent.clone();
        let e = effect.clone();
        tokio::task::spawn_blocking(move || {
            execution.with_current(|entry, verify| {
                matches(entry, &b, &e)?;
                verify()?;
                b.prepare(&c, &p)
            })
        })
        .await??;
        let b = boot.clone();
        let overlay = tokio::task::spawn_blocking(move || b.finish()).await??;
        validate_prepared_guest_overlay(&parent, &overlay)?;
        let current = self
            .registry
            .authority
            .validate_ownership(&self.binding.spec.job_id, &self.binding.ownership)
            .await?;
        validate_job(
            &current,
            &self.binding.spec,
            &self.binding.ownership,
            &effect,
        )?;
        ensure!(
            current == accepted,
            "source quorum changed during disk preparation"
        );
        let prepared = config.prepared(parent, overlay)?;
        let execution = self.clone();
        let b = boot.clone();
        let e = effect.clone();
        let process = tokio::task::spawn_blocking(move || {
            execution.with_current(|entry, verify| {
                matches(entry, &b, &e)?;
                ensure!(
                    entry.desktop.is_none() && entry.registered_process.is_none(),
                    "source child already attempted"
                );
                verify()?;
                let process = GuestProcessEffect {
                    effect_id: e.clone(),
                    job_id: execution.binding.spec.job_id.clone(),
                    ownership: execution.binding.ownership.clone(),
                    controller_id: entry.assignment.destination.controller_id.clone(),
                    controller_generation: entry.assignment.destination.controller_generation,
                    process_instance_id: b.retain_child(&prepared, &execution.guest_id)?,
                };
                entry.registered_process = Some(process.clone());
                Ok(process)
            })
        })
        .await??;
        let b = boot.clone();
        let endpoint = tokio::task::spawn_blocking(move || b.boot()).await??;
        ensure!(
            endpoint.process_instance_id == process.process_instance_id,
            "source readiness belongs to another process"
        );
        let current = self
            .registry
            .authority
            .validate_ownership(&self.binding.spec.job_id, &self.binding.ownership)
            .await?;
        validate_job(
            &current,
            &self.binding.spec,
            &self.binding.ownership,
            &effect,
        )?;
        ensure!(current == accepted, "source quorum changed during boot");
        self.with_current(|entry, verify| {
            matches(entry, &boot, &effect)?;
            ensure!(
                entry
                    .registered_process
                    .as_ref()
                    .is_some_and(
                        |process| process.process_instance_id == endpoint.process_instance_id
                    ),
                "source readiness process changed"
            );
            verify()?;
            boot.publish(entry, &endpoint)
        })?;
        Ok(Some(NativeSourceBootReady {
            attempt,
            accepted,
            effect,
            endpoint,
        }))
    }
}
