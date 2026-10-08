// Origin: CTOX
// License: AGPL-3.0-only
//! Native controller/quorum boundary regressions. No installed VM/Core success claim.
use super::*;
#[test]
fn target_activation_requires_the_exact_import_and_sole_registered_process_effect() {
    use ctox_sync::authority::ProtectedCheckpoint;
    let imported = GuestImportReceipt {
        destination: GuestRestoreDestination {
            instance_id: "target".into(),
            guest_id: "original-guest".into(),
            human_owner_id: "owner".into(),
            project_id: "project".into(),
            thread_id: "thread".into(),
            worker_profile_id: "profile".into(),
            controller_id: "controller".into(),
            controller_generation: 1,
            import_parent: PathBuf::from("/unused"),
        },
        spec: ExecutionSpec {
            job_id: "original-job".into(),
            session_id: uuid::Uuid::new_v4().to_string(),
            scope_id: "scope".into(),
            harness: ctox_core::native_harness_name().into(),
            harness_version: ctox_core::native_harness_version().into(),
            model_route_id: "openai".into(),
            gateway_account_id: "fixture".into(),
            model_id: "model".into(),
            required_capabilities: BTreeSet::new(),
        },
        ownership: Ownership {
            node_id: 2,
            generation: 2,
        },
        checkpoint_digest: "ab".repeat(32),
        sequence: 1,
        imported_directory: PathBuf::from("/unused/import"),
        effect_id: "actual-import".into(),
    };
    let job = Job {
        spec: imported.spec.clone(),
        ownership: imported.ownership.clone(),
        checkpoint: Some(ProtectedCheckpoint {
            digest: imported.checkpoint_digest.clone(),
            sequence: imported.sequence,
            replicas: BTreeSet::from([1, 2]),
            receipts: vec![],
            disclosure: None,
        }),
        checkpoint_requires_refresh: true,
        pending_effects: BTreeSet::from(["actual-child".into()]),
        completed_effects: BTreeSet::from([imported.effect_id.clone()]),
        stopped: false,
    };
    validate_process_job(&job, &imported, "actual-child").unwrap();
    for mutation in 0..11 {
        let mut bad = job.clone();
        match mutation {
            0 => bad.spec.job_id = "foreign".into(),
            1 => bad.spec.session_id = uuid::Uuid::new_v4().to_string(),
            2 => bad.ownership.node_id += 1,
            3 => bad.ownership.generation += 1,
            4 => bad.stopped = true,
            5 => bad.pending_effects.clear(),
            6 => {
                bad.pending_effects.insert("unknown-external".into());
            }
            7 => {
                bad.completed_effects.insert("actual-child".into());
            }
            8 => bad.completed_effects.clear(),
            9 => bad.checkpoint.as_mut().unwrap().digest = "cd".repeat(32),
            _ => bad.checkpoint.as_mut().unwrap().sequence += 1,
        }
        assert!(validate_process_job(&bad, &imported, "actual-child").is_err());
    }
    assert!(validate_process_job(&job, &imported, "arbitrary-effect").is_err());
    validate_unchanged_process_job(&job, &job, &imported, "actual-child").unwrap();
    for mutation in 0..3 {
        let mut changed = job.clone();
        match mutation {
            0 => {
                changed
                    .completed_effects
                    .insert("concurrent-completed-effect".into());
            }
            1 => {
                changed.checkpoint.as_mut().unwrap().replicas.insert(3);
            }
            _ => changed.checkpoint_requires_refresh = false,
        }
        // These still bind to our import/process, but are not the accepted
        // BeginEffect state and must fence publication after a machine await.
        validate_process_job(&changed, &imported, "actual-child").unwrap();
        assert!(validate_unchanged_process_job(&changed, &job, &imported, "actual-child").is_err());
    }
}
#[test]
fn cancelled_target_attempt_cannot_load_activate_or_observe_readiness() {
    let (retired, _) = watch::channel(false);
    let machine = Arc::new(TargetMachine {
        retired,
        state: Mutex::new(State {
            io: MachineIo::new().unwrap(),
            attempted: false,
            staged: None,
            desktop: None,
            loaded: false,
        }),
    });
    drop(Attempt {
        machine: machine.clone(),
        complete: false,
    });
    assert!(machine.current().is_err());
    assert!(machine.load().is_err());
    assert!(machine.activate().is_err());
    assert!(machine.probe().is_err());
    assert!(machine.process_id().is_err());
    assert!(!machine.state.lock().unwrap().attempted);
    assert!(machine.stop().unwrap().is_none());
}
#[tokio::test]
async fn real_registry_restore_does_not_convert_fresh_enrollment_to_target_execution() {
    struct Reject;
    impl target_import::NativeGuestImportFence for Reject {
        fn with_current(
            &self,
            _: &Connection,
            _: &ctox_sync::authority::auth::SigningIdentity,
            _: &GuestRestoreDestination,
            _: &mut dyn FnMut() -> io::Result<()>,
        ) -> io::Result<()> {
            panic!("unprotected enrollment must not reach target publication");
        }
    }
    let (root, registry, assignment) = super::super::tests::fixture();
    let store = CheckpointStore::open(root.path().join("received"), 1024).unwrap();
    let result = registry
        .restore_received_machine(
            &assignment.destination.guest_id,
            store,
            &"ab".repeat(32),
            &"cd".repeat(32),
            Arc::new(Reject),
        )
        .await;
    assert!(result
        .err()
        .unwrap()
        .to_string()
        .contains("protected target enrollment"));
    let entry = registry
        .registration(&assignment.destination.guest_id)
        .unwrap();
    let entry = entry.lock().unwrap();
    assert!(
        entry.target_machine.is_none() && entry.process_effect.is_none() && entry.desktop.is_none()
    );
}

#[tokio::test(flavor = "current_thread")]
async fn cancellation_on_host_runtime_reaps_the_exact_retained_child() -> Result<()> {
    use super::super::super::guest_runtime::{
        PreparedQemuGuest, QemuAcceleration, RetainedQemuDesktop,
    };
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir()?;
    let program = root.path().join("owned-process-fixture");
    // Actual child cleanup; no VM readiness or original service success claim.
    std::fs::write(&program, b"#!/bin/sh\nexec /bin/sleep 30\n")?;
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700))?;
    let base = root.path().join("base.raw");
    let overlay = root.path().join("root.qcow2");
    std::fs::File::create(&base)?.set_len(2 * 1024 * 1024)?;
    std::fs::write(&overlay, b"owned pre-spawn placeholder")?;
    let config = PreparedQemuGuest {
        program,
        runtime_parent: root.path().to_owned(),
        base_raw: base,
        overlay_qcow2: overlay,
        memory_mib: 64,
        vcpus: 1,
        acceleration: QemuAcceleration::Tcg,
    };
    let io = MachineIo::new()?;
    let desktop = io.run(|_| RetainedQemuDesktop::spawn_paused(&config, "owned-child".into()))?;
    let pid = desktop.pid();
    let (retired, _) = watch::channel(false);
    let machine = Arc::new(TargetMachine {
        retired,
        state: Mutex::new(State {
            io,
            attempted: true,
            staged: None,
            desktop: Some(desktop),
            loaded: false,
        }),
    });
    drop(Attempt {
        machine: machine.clone(),
        complete: false,
    });
    assert!(machine.current().is_err());
    assert!(
        machine.stop()?.is_some(),
        "cleanup must retain the actual exit status"
    );
    let exists = unsafe { libc::kill(pid as libc::pid_t, 0) };
    assert_eq!(exists, -1, "exact child was not reaped");
    assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn retained_machine_runtime_serves_multiple_operations_without_nested_runtime() -> Result<()>
{
    let io = MachineIo::new()?;
    for _ in 0..2 {
        io.run(|runtime| {
            runtime.block_on(async {
                tokio::time::sleep(Duration::from_millis(1)).await;
                Ok(())
            })
        })?;
    }
    drop(io);
    Ok(())
}
