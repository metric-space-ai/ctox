// Origin: CTOX
// License: AGPL-3.0-only
//! Component authority observations and an actual retained child; no quorum/VM acceptance claim.
use super::super::source_effects::SourceEffects;
use super::*;

fn observed_job() -> ctox_sync::authority::Job {
    ctox_sync::authority::Job {
        spec: ExecutionSpec {
            job_id: "source-job".into(),
            session_id: uuid::Uuid::new_v4().to_string(),
            scope_id: "native-test-scope".into(),
            harness: ctox_core::native_harness_name().into(),
            harness_version: ctox_core::native_harness_version().into(),
            model_route_id: "openai".into(),
            gateway_account_id: "fixture".into(),
            model_id: "model".into(),
            required_capabilities: BTreeSet::from(["fixture-requirement".into()]),
        },
        ownership: Ownership {
            node_id: 1,
            generation: 1,
        },
        checkpoint: None,
        checkpoint_requires_refresh: true,
        pending_effects: BTreeSet::from(["child-effect".into()]),
        completed_effects: BTreeSet::new(),
        stopped: false,
    }
}

#[tokio::test]
async fn native_source_effect_observation_unavailable_authority_produces_no_capture_input() {
    let (_root, registry, _assignment) = fixture();
    let job = observed_job();
    assert!(
        SourceEffects::read_authority(registry.authority.as_ref(), &job.spec, &job.ownership)
            .await
            .is_err()
    );
}

#[test]
fn native_source_effect_observation_rejects_foreign_stopped_and_inconsistent_authority() {
    let expected = observed_job();
    for change in 0..8 {
        let mut bad = expected.clone();
        match change {
            0 => bad.spec.session_id = uuid::Uuid::new_v4().to_string(),
            1 => bad.spec.gateway_account_id = "foreign-account".into(),
            2 => bad.ownership.generation += 1,
            3 => bad.stopped = true,
            4 => {
                bad.completed_effects.insert("child-effect".into());
            }
            5 => {
                bad.pending_effects.insert("bad\neffect".into());
            }
            6 => {
                bad.pending_effects.insert("x".repeat(257));
            }
            _ => bad.pending_effects = (0..257).map(|i| format!("effect-{i}")).collect(),
        }
        assert!(
            SourceEffects::from_observation_fixture(bad, &expected.spec, &expected.ownership)
                .is_err()
        );
    }
}

#[test]
fn native_source_effect_observation_never_certifies_empty_or_completed_effects() {
    let mut job = observed_job();
    job.pending_effects.clear();
    job.completed_effects.insert("completed-child".into());
    let effects =
        SourceEffects::from_observation_fixture(job.clone(), &job.spec, &job.ownership).unwrap();
    let pending = effects.pending("capture-id").unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].effect_id, "native-effects-capture-id");
    let bytes = effects.bytes(&job.spec, &job.ownership).unwrap();
    let encoded: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(encoded["observedPendingEffects"], json!([]));
    assert_eq!(encoded["externalEffects"], "unknown");
    assert_eq!(encoded["reconciled"], false);
    let mut foreign = job.ownership.clone();
    foreign.generation += 1;
    assert!(effects.bytes(&job.spec, &foreign).is_err());
    job.pending_effects
        .insert("native-effects-capture-id".into());
    let colliding =
        SourceEffects::from_observation_fixture(job.clone(), &job.spec, &job.ownership).unwrap();
    assert!(colliding.pending("capture-id").is_err());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn native_source_effect_observation_matches_actual_retained_child_without_stopping_it(
) -> Result<()> {
    use super::super::super::guest_runtime::{
        PreparedQemuGuest, QemuAcceleration, RetainedQemuDesktop,
    };
    use std::os::unix::fs::PermissionsExt;
    let (root, registry, assignment) = fixture();
    let program = root.path().join("owned-child-fixture");
    std::fs::write(&program, "#!/bin/sh\nexec /bin/sleep 30\n")?;
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700))?;
    let base = root.path().join("base.raw");
    std::fs::File::create(&base)?.set_len(2 * 1024 * 1024)?;
    let overlay = root.path().join("owned-overlay.qcow2");
    std::fs::write(&overlay, b"owned validation fixture")?;
    // AF_UNIX paths are bounded even when the lane's per-run TMPDIR is long.
    // Keep the tiny private socket directory on the same lane tmp storage.
    let tmp_root = std::env::temp_dir();
    let socket_parent = if tmp_root.as_os_str().len() > 48 {
        tmp_root.parent().context("fixture tmp parent absent")?
    } else {
        tmp_root.as_path()
    };
    let socket_root = tempfile::Builder::new()
        .prefix("child-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir_in(socket_parent)?;
    let desktop = RetainedQemuDesktop::spawn_paused(
        &PreparedQemuGuest {
            program,
            runtime_parent: socket_root.path().into(),
            base_raw: base,
            overlay_qcow2: overlay,
            memory_mib: 64,
            vcpus: 1,
            acceleration: QemuAcceleration::Tcg,
        },
        assignment.destination.guest_id.clone(),
    )?;
    let pid = desktop.pid();
    let process_id = desktop.process_instance_id().to_owned();
    let job = observed_job();
    let registration = registry.registration(&assignment.destination.guest_id)?;
    let result = (|| -> Result<()> {
        let mut entry = registration.lock().unwrap();
        entry.desktop = Some(desktop);
        let process = GuestProcessEffect {
            effect_id: "child-effect".into(),
            job_id: job.spec.job_id.clone(),
            ownership: job.ownership.clone(),
            controller_id: assignment.destination.controller_id.clone(),
            controller_generation: assignment.destination.controller_generation,
            process_instance_id: process_id.clone(),
        };
        entry.process_effect = Some(process.effect_id.clone());
        entry.registered_process = Some(process.clone());
        let mut effects =
            SourceEffects::from_observation_fixture(job.clone(), &job.spec, &job.ownership)?;
        effects.verify_controller(&entry, &registry)?;
        let bytes = effects.bytes(&job.spec, &job.ownership)?;
        let encoded: serde_json::Value = serde_json::from_slice(&bytes)?;
        assert_eq!(
            encoded["registeredGuestProcess"]["processInstanceId"],
            process_id
        );
        assert_eq!(
            encoded["registeredGuestProcess"]["childStopObserved"],
            false
        );
        assert_eq!(effects.pending("capture-id")?.len(), 2);
        for change in 0..6 {
            let mut bad = process.clone();
            match change {
                0 => bad.job_id = "foreign-job".into(),
                1 => bad.ownership.generation += 1,
                2 => bad.controller_generation += 1,
                3 => bad.controller_id = "foreign-controller".into(),
                4 => bad.process_instance_id = "foreign-process".into(),
                _ => bad.effect_id = "foreign-effect".into(),
            }
            entry.registered_process = Some(bad);
            assert!(effects.verify_controller(&entry, &registry).is_err());
        }
        entry.registered_process = Some(process);
        let mut no_pending = job.clone();
        no_pending.pending_effects.clear();
        no_pending.completed_effects.insert("child-effect".into());
        let mut completed =
            SourceEffects::from_observation_fixture(no_pending, &job.spec, &job.ownership)?;
        assert!(
            completed.verify_controller(&entry, &registry).is_err(),
            "a live child cannot be certified completed"
        );
        entry.registered_process = None;
        assert!(
            effects.verify_controller(&entry, &registry).is_err(),
            "a partial process claim is not capture input"
        );
        entry.process_effect = None;
        assert!(
            effects.verify_controller(&entry, &registry).is_err(),
            "an unregistered retained child cannot be captured"
        );
        assert!(
            std::path::Path::new(&format!("/proc/{pid}")).exists(),
            "observing must not kill the source session"
        );
        Ok(())
    })();
    // Cleanup also runs on assertion-free failures; the exact real child stays owned.
    let mut desktop = registration.lock().unwrap().desktop.take().unwrap();
    desktop.stop().await?;
    result?;
    assert!(
        !std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "fixture child was not reaped"
    );
    Ok(())
}
