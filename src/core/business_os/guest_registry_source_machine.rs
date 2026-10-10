// Origin: CTOX
// License: AGPL-3.0-only
//! Actual retained source child -> quiesced machine artifacts. Never a clean-effect grant.
use super::*;
use ctox_sync::contracts::WorkspaceEntry;
use tokio::sync::watch;

pub(super) struct SourceMachineCapture {
    process: GuestProcessEffect,
    retired: watch::Sender<bool>,
    state: Mutex<State>,
}
struct State {
    desktop: super::super::guest_runtime::RetainedQemuDesktop,
    io: Option<Arc<super::machine_io::MachineIo>>,
    attempted: bool,
    entries: Option<Vec<WorkspaceEntry>>,
    completion: Completion,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Completion {
    Virgin,
    Pending,
    Completed,
}
impl SourceMachineCapture {
    /// Successful opaque VM export is required; stop status or PID absence is insufficient.
    pub(super) fn process_reconciled(&self, process: &GuestProcessEffect) -> Result<bool> {
        ensure!(
            self.matches(process) && !*self.retired.borrow(),
            "source machine export retired or foreign"
        );
        let state = self
            .state
            .try_lock()
            .map_err(|_| anyhow::anyhow!("source machine export busy or poisoned"))?;
        Ok(state.entries.is_some() && state.completion == Completion::Completed)
    }

    pub(super) fn begin_reconciliation(&self, process: &GuestProcessEffect) -> Result<()> {
        ensure!(
            self.matches(process) && !*self.retired.borrow(),
            "source machine export retired or foreign"
        );
        let mut state = self
            .state
            .try_lock()
            .map_err(|_| anyhow::anyhow!("source machine export busy or poisoned"))?;
        ensure!(
            state.entries.is_some() && state.completion == Completion::Virgin,
            "source machine has no complete export or effect completion is uncertain"
        );
        state.completion = Completion::Pending;
        Ok(())
    }

    pub(super) fn finish_reconciliation(&self, process: &GuestProcessEffect) -> Result<()> {
        ensure!(
            self.matches(process) && !*self.retired.borrow(),
            "source machine export retired or foreign"
        );
        let mut state = self
            .state
            .try_lock()
            .map_err(|_| anyhow::anyhow!("source machine export busy or poisoned"))?;
        ensure!(
            state.entries.is_some() && state.completion == Completion::Pending,
            "source machine effect completion changed"
        );
        state.completion = Completion::Completed;
        Ok(())
    }

    pub(super) fn retire(&self) {
        self.retired.send_replace(true);
    }

    pub(super) fn matches(&self, process: &GuestProcessEffect) -> bool {
        self.process == *process
    }

    pub(super) fn entries(&self) -> Result<Vec<WorkspaceEntry>> {
        ensure!(!*self.retired.borrow(), "source machine export retired");
        self.state
            .try_lock()
            .map_err(|_| anyhow::anyhow!("source machine export busy or poisoned"))?
            .entries
            .clone()
            .context("source machine export incomplete; reconcile")
    }

    pub(super) fn stop(&self) -> Result<std::process::ExitStatus> {
        self.retire();
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("source machine export poisoned"))?;
        let io = state.io.clone();
        super::machine_io::run(io.as_deref(), state.desktop.stop())
    }

    fn export(&self, store: &ctox_sync::checkpoint::CheckpointStore, parent: &Path) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("source machine export poisoned"))?;
        ensure!(!*self.retired.borrow(), "source machine export retired");
        if state.entries.is_some() {
            return Ok(());
        }
        ensure!(
            !state.attempted,
            "source machine export already attempted; reconcile"
        );
        state.attempted = true; // retained before the first QMP await, including cancellation
        let mut memory = tempfile::NamedTempFile::new_in(parent)?;
        let mut output = tokio::fs::File::from_std(memory.reopen()?);
        let mut retired = self.retired.subscribe();
        // subscribe marks the current value seen: check it before waiting so a
        // retirement between the first check and subscription cannot be lost.
        ensure!(!*retired.borrow(), "source machine export retired");
        let io = state.io.clone();
        let desktop = &mut state.desktop;
        let witness = super::machine_io::run(io.as_deref(), async {
            tokio::select! {
                biased;
                _ = retired.changed() => anyhow::bail!("source machine export revoked"),
                result = tokio::time::timeout(Duration::from_secs(300), async {
                    let endpoint = desktop.probe_live().await?;
                    ensure!(endpoint.process_instance_id == self.process.process_instance_id,
                        "source machine endpoint differs from the retained child");
                    let witness = desktop.save_checkpoint_live(&endpoint, &mut output).await?;
                    output.sync_all().await?;
                    Ok::<_, anyhow::Error>(witness)
                }) => result.context("source machine export deadline")?,
            }
        })?;
        drop(output);
        // Full RAM/disk/base hashing and chunk IO hold only this owned-child
        // mutex, never account/issuer/worker/SQLite/controller publication locks.
        let entries = witness.store(store, memory.as_file_mut())?;
        ensure!(
            !*self.retired.borrow(),
            "source machine export revoked before publication"
        );
        state.entries = Some(entries);
        Ok(())
    }
}
impl NativeGuestExecution {
    pub(super) fn export_source_machine(
        &self,
        source: &crate::channels::NativeProviderCaptureOwner,
    ) -> Result<()> {
        ensure!(
            source.matches_provider(&self.provider),
            "foreign native machine capture owner"
        );
        let mut observed = source_effects::SourceEffects::observe(self)?;
        let prepared = source.with_current_capture_transaction(|worker, facts| {
            self.with_held_worker_policy(worker, facts, |entry, verify, policy| {
                self.registry.require_live_transport()?;
                verify()?;
                let process = match (&entry.process_effect, &entry.registered_process) {
                    (None, None) => {
                        ensure!(
                            entry.desktop.is_none() && entry.source_machine.is_none(),
                            "unregistered native source child"
                        );
                        return Ok(None);
                    }
                    (Some(id), Some(process)) if id == &process.effect_id => process.clone(),
                    _ => anyhow::bail!("native source process effect is incomplete"),
                };
                ensure!(
                    process.job_id == self.binding.spec.job_id
                        && process.ownership == self.binding.ownership
                        && process.controller_id == entry.assignment.destination.controller_id
                        && process.controller_generation
                            == entry.assignment.destination.controller_generation,
                    "native source process ownership changed"
                );
                let parent = entry.assignment.destination.import_parent.clone();
                ensure!(
                    workspaces::snapshot(policy, &entry.assignment.destination)?.is_some(),
                    "native machine export needs its actual assigned workspace"
                );
                let (store, store_root, store_identity) = source_journal::source_store(&parent)?;
                let capture = if let Some(capture) = &entry.source_machine {
                    ensure!(
                        entry.desktop.is_none() && capture.matches(&process),
                        "source machine export belongs to another process"
                    );
                    // Existing failed attempts stay unresolved; never repeat QMP.
                    capture.entries()?;
                    Arc::clone(capture)
                } else {
                    observed.verify_controller(entry, &self.registry)?;
                    let desktop = entry
                        .desktop
                        .take()
                        .context("source child is not retained")?;
                    let (retired, _) = watch::channel(false);
                    let capture = Arc::new(SourceMachineCapture {
                        process,
                        retired,
                        state: Mutex::new(State {
                            desktop,
                            io: entry.desktop_io.take(),
                            attempted: false,
                            entries: None,
                            completion: Completion::Virgin,
                        }),
                    });
                    entry.source_machine = Some(Arc::clone(&capture));
                    self.registry.retire_frame(entry)?;
                    capture
                };
                Ok(Some((capture, store, store_root, store_identity, parent)))
            })
        })?;
        let Some((capture, store, store_root, store_identity, parent)) = prepared else {
            return Ok(());
        };
        capture.export(&store, &parent)?;
        // No IO mutex survives fresh quorum/native authority revalidation.
        let mut current = source_effects::SourceEffects::observe(self)?;
        ensure!(
            observed.same_quorum(&current),
            "source quorum changed during machine export"
        );
        source.with_current_capture_transaction(|worker, facts| {
            self.with_held_worker_policy(worker, facts, |entry, verify, _| {
                self.registry.require_live_transport()?;
                verify()?;
                ensure!(
                    entry
                        .source_machine
                        .as_ref()
                        .is_some_and(|actual| Arc::ptr_eq(actual, &capture))
                        && entry.desktop.is_none()
                        && private_directory(&store_root)? == store_identity,
                    "native source machine/store changed during export"
                );
                current.verify_controller(entry, &self.registry)?;
                Ok(())
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business_os::guest_runtime::{
        PreparedQemuGuest, QemuAcceleration, RetainedQemuDesktop,
    };
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn failed_machine_capture_retains_exact_child_and_retirement_never_waits_for_io() -> Result<()>
    {
        let root = tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()?;
        let program = root.path().join("owned-child");
        std::fs::write(&program, "#!/bin/sh\nexec /bin/sleep 30\n")?;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700))?;
        let base = root.path().join("base.raw");
        std::fs::File::create(&base)?.set_len(2 * 1024 * 1024)?;
        let overlay = root.path().join("disk.qcow2");
        std::fs::write(&overlay, b"owned validation fixture")?;
        let tmp = std::env::temp_dir();
        let socket_parent = if tmp.as_os_str().len() > 48 {
            tmp.parent().context("fixture tmp parent absent")?
        } else {
            tmp.as_path()
        };
        let sockets = tempfile::Builder::new()
            .prefix("child-")
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir_in(socket_parent)?;
        // Match the running host: its process/signal driver remains live while
        // the synchronous capture owner uses block_on_guest on another runtime.
        // A dormant current-thread fixture cannot deliver the original child
        // registration SIGCHLD to wait(), even after SIGKILL succeeds.
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()?;
        let desktop = {
            let _entered = runtime.enter();
            RetainedQemuDesktop::spawn_paused(
                &PreparedQemuGuest {
                    program,
                    runtime_parent: sockets.path().into(),
                    base_raw: base,
                    overlay_qcow2: overlay,
                    memory_mib: 64,
                    vcpus: 1,
                    acceleration: QemuAcceleration::Tcg,
                },
                "source-guest".into(),
            )?
        };
        let pid = desktop.pid();
        let process = GuestProcessEffect {
            effect_id: "owned-effect".into(),
            job_id: "owned-job".into(),
            ownership: Ownership {
                node_id: 1,
                generation: 1,
            },
            controller_id: "controller".into(),
            controller_generation: 1,
            process_instance_id: desktop.process_instance_id().into(),
        };
        let (retired, _) = watch::channel(false);
        let capture = SourceMachineCapture {
            process: process.clone(),
            retired,
            state: Mutex::new(State {
                desktop,
                io: None,
                attempted: false,
                entries: None,
                completion: Completion::Virgin,
            }),
        };
        let (store, _, _) = source_journal::source_store(root.path())?;
        // This actual retained child has never become Ready. The production
        // export must fail without manufacturing a VM witness or dropping it.
        let error = capture.export(&store, root.path()).unwrap_err();
        assert!(error.to_string().contains("guest is not ready"));
        assert!(capture.matches(&process));
        assert!(Path::new(&format!("/proc/{pid}")).exists());
        assert!(capture.entries().is_err());
        assert!(capture.begin_reconciliation(&process).is_err());
        assert!(capture.finish_reconciliation(&process).is_err());
        assert!(!capture.process_reconciled(&process)?);
        assert!(
            capture
                .export(&store, root.path())
                .unwrap_err()
                .to_string()
                .contains("already attempted")
        );
        {
            let state = capture.state.lock().unwrap();
            assert_eq!(state.desktop.pid(), pid);
            assert!(state.attempted && state.entries.is_none());
            assert!(capture.entries().unwrap_err().to_string().contains("busy"));
            // The actual export mutex is held: a synchronous revoke must still
            // retire immediately, and a late subscriber must see that state.
            capture.retire();
            let subscriber = capture.retired.subscribe();
            assert!(*subscriber.borrow());
        }
        capture.stop()?;
        assert!(
            !Path::new(&format!("/proc/{pid}")).exists(),
            "exact child was not reaped"
        );
        assert!(capture.entries().is_err());
        Ok(())
    }
}
