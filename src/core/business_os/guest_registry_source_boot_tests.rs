// Origin: CTOX
// License: AGPL-3.0-only
//! Actual helper/retained-child cleanup and native stop permissions.
//! No source OS/Core, clean effects or two-host continuation success claim.
use super::*;
use std::os::unix::fs::PermissionsExt;

fn configuration(root: &Path, image_helper: &str) -> Result<NativeGuestMachineConfiguration> {
    let program = root.join("qemu-system");
    let image = root.join("qemu-img");
    let base = root.join("base.raw");
    std::fs::write(&program, b"#!/bin/sh\nexec /bin/sleep 30\n")?;
    std::fs::write(&image, image_helper)?;
    for path in [&program, &image] {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    std::fs::File::create(&base)?.set_len(2 * 1024 * 1024)?;
    std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o400))?;
    Ok(serde_json::from_value(serde_json::json!({
        "program":program,"baseRaw":base,"memoryMib":64,"vcpus":1,"acceleration":"tcg"
    }))?)
}
fn short_root() -> Result<tempfile::TempDir> {
    Ok(tempfile::Builder::new()
        .prefix("sb-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir_in(std::env::temp_dir())?)
}
#[test]
fn source_boot_requires_exact_fresh_job_and_sole_uncompleted_process_effect() {
    let spec = ExecutionSpec {
        job_id: "original-job".into(),
        session_id: uuid::Uuid::new_v4().to_string(),
        scope_id: "scope".into(),
        harness: ctox_core::native_harness_name().into(),
        harness_version: ctox_core::native_harness_version().into(),
        model_route_id: "openai".into(),
        gateway_account_id: "original-account".into(),
        model_id: "model".into(),
        required_capabilities: BTreeSet::new(),
    };
    let ownership = Ownership {
        node_id: 1,
        generation: 1,
    };
    let job = Job {
        spec: spec.clone(),
        ownership: ownership.clone(),
        checkpoint: None,
        checkpoint_requires_refresh: true,
        pending_effects: BTreeSet::from(["actual-process".into()]),
        completed_effects: BTreeSet::new(),
        stopped: false,
    };
    validate_job(&job, &spec, &ownership, "actual-process").unwrap();
    for mutation in 0..10 {
        let mut bad = job.clone();
        match mutation {
            0 => bad.spec.job_id = "foreign".into(),
            1 => bad.spec.session_id = uuid::Uuid::new_v4().to_string(),
            2 => bad.spec.gateway_account_id = "replacement".into(),
            3 => bad.ownership.generation += 1,
            4 => bad.ownership.node_id += 1,
            5 => bad.stopped = true,
            6 => bad.pending_effects.clear(),
            7 => {
                bad.pending_effects.insert("unknown-tool".into());
            }
            8 => {
                bad.completed_effects.insert("actual-process".into());
            }
            _ => {
                bad.checkpoint = Some(ctox_sync::authority::ProtectedCheckpoint {
                    digest: "ab".repeat(32),
                    sequence: 1,
                    replicas: BTreeSet::from([1, 2]),
                    receipts: vec![],
                    disclosure: None,
                })
            }
        }
        assert!(validate_job(&bad, &spec, &ownership, "actual-process").is_err());
    }
    assert!(validate_job(&job, &spec, &ownership, "claimed-process").is_err());
}
#[tokio::test(flavor = "current_thread")]
async fn actual_image_helper_is_retained_and_reaped_after_revocation() -> Result<()> {
    let root = short_root()?;
    let pid_file = root.path().join("helper.pid");
    let script = format!(
        "#!/bin/sh\nprintf '%s' $$ > '{}'\nexec /bin/sleep 30\n",
        pid_file.display()
    );
    let config = configuration(root.path(), &script)?;
    let boot = Arc::new(SourceBoot::new()?);
    let parent = root.path().join("runtime");
    boot.prepare(&config, &parent)?;
    assert!(
        boot.prepare(&config, &parent).is_err(),
        "a replacement helper cannot be installed"
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !pid_file.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let pid = std::fs::read_to_string(pid_file)?;
    assert!(Path::new(&format!("/proc/{pid}")).exists());
    // Retirement never waits for helper/child IO mutexes.
    {
        let _held = boot.helper.lock().unwrap();
        boot.retire();
        assert!(*boot.retired.subscribe().borrow());
    }
    assert!(boot.finish().is_err());
    boot.stop_helper()?;
    assert!(
        !Path::new(&format!("/proc/{pid}")).exists(),
        "exact helper was not reaped"
    );
    assert_eq!(
        std::fs::read_dir(&parent)?.count(),
        0,
        "unpublished disk survived helper abort"
    );
    assert!(boot.current().is_err());
    Ok(())
}
#[tokio::test(flavor = "current_thread")]
async fn real_qemu_img_disk_remains_private_and_is_not_erased_by_later_abort() -> Result<()> {
    let root = short_root()?;
    let base = root.path().join("base.raw");
    std::fs::File::create(&base)?.set_len(2 * 1024 * 1024)?;
    std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o400))?;
    let config: NativeGuestMachineConfiguration = serde_json::from_value(serde_json::json!({
        "program":"/usr/bin/qemu-system-x86_64","baseRaw":base,"memoryMib":64,"vcpus":1,"acceleration":"tcg"
    }))?;
    let boot = SourceBoot::new()?;
    let parent = root.path().join("runtime");
    boot.prepare(&config, &parent)?;
    let disk = boot.finish()?;
    validate_prepared_guest_overlay(&parent, &disk)?;
    assert_eq!(
        std::fs::metadata(&disk)?.permissions().mode() & 0o777,
        0o600
    );
    assert!(boot.finish().is_err(), "preparation is one-shot");
    boot.stop_helper()?;
    assert!(
        disk.is_file(),
        "published disk must remain for reconciliation"
    );
    assert!(boot.current().is_err());
    Ok(())
}
#[tokio::test(flavor = "current_thread")]
async fn production_human_stop_reaps_only_owned_source_child_and_preserves_pending_effect(
) -> Result<()> {
    let (root, registry, assignment) = super::super::tests::fixture();
    let sockets = short_root()?;
    let config = configuration(root.path(), "#!/bin/sh\nexit 1\n")?;
    let overlay = root.path().join("disk.qcow2");
    std::fs::write(&overlay, b"placeholder, never a VM-ready witness")?;
    let prepared = config.prepared(sockets.path().into(), overlay)?;
    let boot = Arc::new(SourceBoot::new()?);
    let process = boot.retain_child(&prepared, &assignment.destination.guest_id)?;
    let pid = boot.child.lock().unwrap().as_ref().unwrap().pid();
    let registration = registry.registration(&assignment.destination.guest_id)?;
    {
        let mut entry = registration.lock().unwrap();
        entry.source_boot = Some(boot.clone());
        entry.process_effect = Some("actual-process".into());
        entry.registered_process = Some(GuestProcessEffect {
            effect_id: "actual-process".into(),
            job_id: "job".into(),
            ownership: Ownership {
                node_id: 1,
                generation: 1,
            },
            controller_id: assignment.destination.controller_id.clone(),
            controller_generation: 1,
            process_instance_id: process,
        });
    }
    let foreign = super::super::tests::session("foreign");
    assert!(registry
        .stop_owned(&foreign, &assignment.destination.guest_id)
        .is_err());
    assert!(boot.current().is_ok() && Path::new(&format!("/proc/{pid}")).exists());
    let owner = super::super::tests::session("owner");
    registry.stop_owned(&owner, &assignment.destination.guest_id)?;
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    assert!(boot.current().is_err());
    let entry = registration.lock().unwrap();
    assert!(entry.revoked && entry.stopped_status.is_some());
    assert_eq!(entry.process_effect.as_deref(), Some("actual-process"));
    assert!(entry.registered_process.is_some());
    Ok(())
}
#[test]
fn initial_boot_never_adopts_an_existing_runtime_directory() -> Result<()> {
    let root = short_root()?;
    let config = configuration(root.path(), "#!/bin/sh\nexit 1\n")?;
    let parent = root.path().join("existing");
    std::fs::create_dir(&parent)?;
    std::fs::write(parent.join("sentinel"), b"retained")?;
    let boot = SourceBoot::new()?;
    assert!(boot.prepare(&config, &parent).is_err());
    assert!(boot.helper.lock().unwrap().is_none());
    assert_eq!(std::fs::read(parent.join("sentinel"))?, b"retained");
    Ok(())
}
