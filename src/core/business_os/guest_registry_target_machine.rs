// Origin: CTOX
// License: AGPL-3.0-only
//! Protected checkpoint -> retained paused QEMU -> original guest service.
//! This does not start a Core/provider turn or certify unknown source effects.
use super::protected_import::ProtectedImport;
use super::*;
use ctox_sync::{
    authority::{Command, Job, Receipt, Request},
    checkpoint::CheckpointStore,
};
use tokio::sync::watch;
#[cfg(test)]
#[path = "guest_registry_target_machine_tests.rs"]
mod tests;

pub(super) struct TargetMachine {
    retired: watch::Sender<bool>,
    state: Mutex<State>,
}
struct State {
    attempted: bool,
    staged: Option<super::super::guest_runtime::StagedQemuCheckpoint>,
    desktop: Option<super::super::guest_runtime::RetainedQemuDesktop>,
    loaded: bool,
    // Keep the driver's lifetime until after the retained child is dropped.
    io: MachineIo,
}
struct Attempt {
    machine: Arc<TargetMachine>,
    complete: bool,
}
impl Drop for Attempt {
    fn drop(&mut self) {
        if !self.complete {
            self.machine.retire();
            // Exact retained child only. Failure stays unresolved and owned;
            // neither cancellation nor stop creates a completion certificate.
            let _ = self.machine.stop();
        }
    }
}
use super::machine_io::MachineIo;

impl TargetMachine {
    pub(super) fn retire(&self) {
        self.retired.send_replace(true);
    }
    pub(super) fn stop(&self) -> Result<Option<std::process::ExitStatus>> {
        self.retire();
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("target machine poisoned"))?;
        let State { io, desktop, .. } = &mut *state;
        desktop
            .as_mut()
            .map(|desktop| io.run(|runtime| runtime.block_on(desktop.stop())))
            .transpose()
    }
    fn current(&self) -> Result<()> {
        ensure!(
            !*self.retired.borrow(),
            "target machine attempt retired; reconcile"
        );
        Ok(())
    }
    fn prepare(
        &self,
        store: &CheckpointStore,
        digest: &str,
        config: &NativeGuestMachineConfiguration,
        assignment: &NativeGuestAssignment,
        service: &str,
    ) -> Result<()> {
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("target machine poisoned"))?;
        self.current()?;
        ensure!(
            !state.attempted,
            "target machine preparation already attempted; reconcile"
        );
        state.attempted = true;
        let parent = assignment.runtime_parent();
        // No existing runtime directory is adopted or cleaned up implicitly.
        std::fs::DirBuilder::new().mode(0o700).create(&parent)?;
        private_directory(&parent)?;
        let disk = parent.join("root.qcow2");
        let mut disk_file = std::fs::OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&disk)?;
        let mut memory = tempfile::NamedTempFile::new_in(&parent)?;
        let manifest = store.load(digest)?;
        ensure!(
            manifest.pending_effects.is_empty(),
            "unknown source effects cannot restore a machine"
        );
        let staged = super::super::guest_runtime::StagedQemuCheckpoint::stage(
            store,
            &manifest.provider_state,
            config.prepared(parent.clone(), disk)?,
            &assignment.destination.guest_id,
            service,
            memory.as_file_mut(),
            &mut disk_file,
        )?;
        disk_file.set_permissions(std::fs::Permissions::from_mode(0o400))?;
        disk_file.sync_all()?;
        std::fs::File::open(&parent)?.sync_all()?;
        self.current()?;
        state.staged = Some(staged);
        Ok(())
    }
    fn load(&self) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("target machine poisoned"))?;
        self.current()?;
        let State {
            io,
            staged,
            desktop,
            ..
        } = &mut *state;
        io.run(|runtime| {
            let staged = staged
                .as_mut()
                .context("target machine staging incomplete")?;
            // Retain the exact paused child before awaits, on its own runtime.
            *desktop =
                Some(super::super::guest_runtime::RetainedQemuDesktop::spawn_checkpoint(staged)?);
            let mut retired = self.retired.subscribe();
            self.current()?;
            runtime.block_on(async {
                tokio::select! {
                    biased;
                    _ = retired.changed() => anyhow::bail!("target machine loading revoked"),
                    result = tokio::time::timeout(Duration::from_secs(45),
                        desktop.as_mut().unwrap().load_checkpoint(staged)) =>
                        result.context("target machine load deadline")?,
                }
            })?;
            self.current()
        })?;
        state.loaded = true;
        Ok(())
    }
    fn process_id(&self) -> Result<String> {
        self.current()?;
        let state = self
            .state
            .try_lock()
            .map_err(|_| anyhow::anyhow!("target machine busy or poisoned"))?;
        ensure!(state.loaded, "target machine load incomplete");
        Ok(state
            .desktop
            .as_ref()
            .context("target child not retained")?
            .process_instance_id()
            .into())
    }
    fn activate(&self) -> Result<GuestLiveEndpoint> {
        self.current()?;
        let mut state = self
            .state
            .try_lock()
            .map_err(|_| anyhow::anyhow!("target machine busy or poisoned"))?;
        ensure!(
            state.loaded,
            "target machine not loaded or activation already attempted"
        );
        state.loaded = false;
        let mut retired = self.retired.subscribe();
        self.current()?;
        let State { io, desktop, .. } = &mut *state;
        let desktop = desktop.as_mut().context("target child not retained")?;
        io.run(|runtime| runtime.block_on(async {
            tokio::select! {
                biased;
                _ = retired.changed() => anyhow::bail!("target activation retired"),
                result = tokio::time::timeout(Duration::from_secs(10), desktop.activate_restored()) =>
                    result.context("target activation deadline")?,
            }
        }))
    }
    fn probe(&self) -> Result<GuestLiveEndpoint> {
        self.current()?;
        let mut state = self
            .state
            .try_lock()
            .map_err(|_| anyhow::anyhow!("target machine busy or poisoned"))?;
        let State { io, desktop, .. } = &mut *state;
        let desktop = desktop.as_mut().context("target child not retained")?;
        let mut retired = self.retired.subscribe();
        io.run(|runtime| {
            runtime.block_on(async {
                tokio::select! {
                    biased;
                    _ = retired.changed() => anyhow::bail!("target probe retired"),
                    result = tokio::time::timeout(Duration::from_secs(10), desktop.probe_live()) =>
                        result.context("target probe deadline")?,
                }
            })
        })
    }
}

fn validate_process_job(job: &Job, imported: &GuestImportReceipt, effect: &str) -> Result<()> {
    ensure!(
        job.spec == imported.spec
            && job.ownership == imported.ownership
            && !job.stopped
            && job.pending_effects.len() == 1
            && job.pending_effects.contains(effect)
            && !job.completed_effects.contains(effect)
            && job.completed_effects.contains(&imported.effect_id)
            && job
                .checkpoint
                .as_ref()
                .is_some_and(|checkpoint| checkpoint.digest == imported.checkpoint_digest
                    && checkpoint.sequence == imported.sequence),
        "target process authority differs from the completed protected import"
    );
    Ok(())
}
fn validate_unchanged_process_job(
    current: &Job,
    accepted: &Job,
    imported: &GuestImportReceipt,
    effect: &str,
) -> Result<()> {
    validate_process_job(current, imported, effect)?;
    ensure!(
        current == accepted,
        "target quorum changed during machine restoration"
    );
    Ok(())
}

struct MachineOwner<'a> {
    protected: ProtectedImport<'a>,
}
impl GuestRestoreOwner for MachineOwner<'_> {
    fn resolve_destination(
        &self,
        guest: &str,
        spec: &ExecutionSpec,
        ownership: &Ownership,
    ) -> io::Result<GuestRestoreDestination> {
        if guest != self.protected.guest_id
            || spec != &self.protected.protected.spec
            || ownership != &self.protected.protected.ownership
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "foreign target machine",
            ));
        }
        self.protected
            .with_current_machine(true, |entry, verify| {
                verify()?;
                Ok(entry.assignment.destination.clone())
            })
            .map_err(io_error)
    }
    fn with_current_fence(
        &self,
        _: &GuestRestoreDestination,
        _: &ExecutionSpec,
        _: &Ownership,
        _: &mut dyn FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "machine owner cannot publish another import",
        ))
    }
}
impl GuestReadinessOwner for MachineOwner<'_> {
    fn with_live_guest(
        &self,
        imported: &GuestImportReceipt,
        publish: &mut dyn FnMut(GuestReadyObservation) -> io::Result<()>,
    ) -> io::Result<()> {
        self.protected
            .with_current_machine(true, |entry, verify| {
                ensure!(
                    entry.imported.as_ref() == Some(imported),
                    "foreign target readiness import"
                );
                let process = entry
                    .registered_process
                    .clone()
                    .context("target process not registered")?;
                ensure!(
                    entry.process_effect.as_deref() == Some(process.effect_id.as_str()),
                    "target process registration differs"
                );
                let machine = entry
                    .target_machine
                    .as_ref()
                    .context("target machine owner missing")?;
                machine.current()?;
                verify()?;
                let endpoint = machine.probe()?;
                ensure!(
                    endpoint.guest_session_id == self.protected.protected.service_session
                        && endpoint.process_instance_id == process.process_instance_id,
                    "restored guest service/process differs from protected source"
                );
                verify()?;
                publish(GuestReadyObservation {
                    endpoint,
                    process_effect: process,
                })?;
                Ok(())
            })
            .map_err(io_error)
    }
}

impl NativeGuestRegistry {
    pub(crate) async fn restore_received_machine(
        self: &Arc<Self>,
        guest: &str,
        store: CheckpointStore,
        binding: &str,
        digest: &str,
        fence: Arc<dyn target_import::NativeGuestImportFence>,
    ) -> Result<ctox_sync::guest_restore::GuestReadyReceipt> {
        let protected = self
            .registration(guest)?
            .lock()
            .map_err(|_| anyhow::anyhow!("native controller poisoned"))?
            .restoration
            .clone()
            .context("machine restore requires protected target enrollment")?;
        ensure!(
            protected.binding_digest == binding && protected.checkpoint_digest == digest,
            "machine restore differs from enrolled binding/checkpoint"
        );
        let owner = MachineOwner {
            protected: ProtectedImport {
                registry: self.clone(),
                guest_id: guest.into(),
                protected,
                fence: fence.as_ref(),
            },
        };
        let imported = owner
            .protected
            .with_current_machine(true, |entry, verify| {
                verify()?;
                Ok(entry.imported.clone().unwrap())
            })?;
        NativeGuestExecution::validate_import_completion(
            self.authority.as_ref(),
            &imported.spec,
            &imported.ownership,
            &imported,
        )
        .await?;
        let config = self
            .machine_configuration
            .lock()
            .map_err(|_| anyhow::anyhow!("machine configuration poisoned"))?
            .clone()
            .context("native operator has not configured an independent machine base")?;
        let (machine, assignment) =
            owner
                .protected
                .with_current_machine(true, |entry, verify| {
                    ensure!(
                        entry.target_machine.is_none()
                            && entry.desktop.is_none()
                            && entry.source_machine.is_none()
                            && entry.process_effect.is_none()
                            && entry.registered_process.is_none(),
                        "target machine already attempted; reconcile retained owner"
                    );
                    let (retired, _) = watch::channel(false);
                    let machine = Arc::new(TargetMachine {
                        retired,
                        state: Mutex::new(State {
                            io: MachineIo::new()?,
                            attempted: false,
                            staged: None,
                            desktop: None,
                            loaded: false,
                        }),
                    });
                    verify()?;
                    entry.target_machine = Some(machine.clone());
                    Ok((machine, entry.assignment.clone()))
                })?;
        let mut attempt = Attempt {
            machine: machine.clone(),
            complete: false,
        };
        let m = machine.clone();
        let d = digest.to_owned();
        let service = owner.protected.protected.service_session.clone();
        tokio::task::spawn_blocking(move || m.prepare(&store, &d, &config, &assignment, &service))
            .await??;
        let effect = format!("guest-process:{}", uuid::Uuid::new_v4());
        owner
            .protected
            .with_current_machine(true, |entry, verify| {
                machine.current()?;
                ensure!(
                    entry
                        .target_machine
                        .as_ref()
                        .is_some_and(|m| Arc::ptr_eq(m, &machine))
                        && entry.process_effect.is_none(),
                    "target preparation changed"
                );
                verify()?;
                entry.process_effect = Some(effect.clone());
                Ok(())
            })?;
        let receipt = self
            .authority
            .submit(Request {
                request_id: format!("{effect}:begin"),
                actor: self.authority.node_id(),
                command: Command::BeginEffect {
                    job_id: imported.spec.job_id.clone(),
                    ownership: imported.ownership.clone(),
                    effect_id: effect.clone(),
                },
            })
            .await?;
        let Receipt::Applied(job) = receipt else {
            anyhow::bail!("target process BeginEffect uncertain/replayed; reconcile");
        };
        validate_process_job(&job, &imported, &effect)?;
        owner.protected.with_current_machine(true, |_, verify| {
            machine.current()?;
            verify()
        })?;
        let m = machine.clone();
        tokio::task::spawn_blocking(move || m.load()).await??;
        let current = self
            .authority
            .validate_ownership(&imported.spec.job_id, &imported.ownership)
            .await?;
        validate_unchanged_process_job(&current, &job, &imported, &effect)?;
        let registry = self.clone();
        let guest_id = guest.to_owned();
        let protected = owner.protected.protected.clone();
        let activation_fence = fence.clone();
        let activation_machine = machine.clone();
        let activation_import = imported.clone();
        let activation_effect = effect.clone();
        // QMP/service awaits run on a blocking worker, including the whole
        // native publication fence. No CurrentThread runtime nesting and no
        // interval of unguarded activation between acquiring and using it.
        tokio::task::spawn_blocking(move || {
            let owner = ProtectedImport {
                registry,
                guest_id,
                protected,
                fence: activation_fence.as_ref(),
            };
            let machine = activation_machine;
            let imported = activation_import;
            let effect = activation_effect;
            owner.with_current_machine(true, |entry, verify| {
                ensure!(
                    entry.process_effect.as_deref() == Some(effect.as_str())
                        && entry.desktop.is_none()
                        && entry
                            .target_machine
                            .as_ref()
                            .is_some_and(|m| Arc::ptr_eq(m, &machine)),
                    "target process attempt changed"
                );
                verify()?;
                entry.registered_process = Some(GuestProcessEffect {
                    effect_id: effect.clone(),
                    job_id: imported.spec.job_id.clone(),
                    ownership: imported.ownership.clone(),
                    controller_id: imported.destination.controller_id.clone(),
                    controller_generation: imported.destination.controller_generation,
                    process_instance_id: machine.process_id()?,
                });
                let endpoint = machine.activate()?;
                ensure!(
                    endpoint.guest_session_id == owner.protected.service_session,
                    "original guest service did not survive restore"
                );
                verify()?;
                Ok(())
            })
        })
        .await??;
        let registry = self.clone();
        let authority = self.authority.clone();
        let guest_id = guest.to_owned();
        let protected = owner.protected.protected.clone();
        let readiness_fence = fence.clone();
        let readiness_import = imported.clone();
        let ready = tokio::task::spawn_blocking(move || {
            let owner = MachineOwner {
                protected: ProtectedImport {
                    registry,
                    guest_id,
                    protected,
                    fence: readiness_fence.as_ref(),
                },
            };
            MachineIo::new()?.run(|runtime| {
                runtime
                    .block_on(ctox_sync::guest_restore::confirm_guest_ready(
                        authority.as_ref(),
                        &owner,
                        readiness_import,
                    ))
                    .map_err(anyhow::Error::from)
            })
        })
        .await??;
        let current = self
            .authority
            .validate_ownership(&imported.spec.job_id, &imported.ownership)
            .await?;
        validate_unchanged_process_job(&current, &job, &imported, &effect)?;
        owner.protected.with_current_machine(true, |_, verify| {
            machine.current()?;
            verify()
        })?;
        attempt.complete = true;
        Ok(ready)
    }
}
