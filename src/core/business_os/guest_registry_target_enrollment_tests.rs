// Origin: CTOX
// License: AGPL-3.0-only
//! Policy/codec component tests; no live provider, quorum or VM acceptance.
use super::workspace_tests::grant_workspace;
use super::*;

fn protected_identity(
    root: &Path,
    guest: &str,
) -> super::super::super::guest_runtime::ProtectedGuestIdentity {
    use ctox_sync::contracts::{ArtifactRef, WorkspaceEntry, WorkspaceEntryKind};
    let store =
        ctox_sync::checkpoint::CheckpointStore::open(root.join(guest), 1024 * 1024).unwrap();
    let bytes = serde_json::to_vec(&serde_json::json!({
        "version":1,"profile":"ctox.pc-i440fx-5.1.qemu64-v1.v1","guestId":guest,
        "sourceProcessInstanceId":"fixture-process","sourceEndpointId":"fixture-endpoint",
        "guestServiceSession":"original-service","memoryMib":128,"vcpus":1,
        "base":{"bytes":512,"sha256":"a".repeat(64)},
        "memory":{"bytes":32,"sha256":"b".repeat(64)},
        "disk":{"bytes":64,"sha256":"c".repeat(64)},
    }))
    .unwrap();
    let artifact = ArtifactRef {
        sha256: format!("{:x}", Sha256::digest(&bytes)),
        size_bytes: bytes.len() as u64,
    };
    store.ingest_blob(&artifact, bytes.as_slice()).unwrap();
    super::super::super::guest_runtime::ProtectedGuestIdentity::from_checkpoint(
        &store,
        &[WorkspaceEntry {
            path: "native-guest-machine.json".into(),
            kind: WorkspaceEntryKind::File,
            artifact,
            executable: false,
        }],
    )
    .unwrap()
}

#[test]
fn original_target_guest_is_retained_once_and_cannot_become_a_fresh_job() {
    let (root, registry, fresh) = fixture();
    let (_, facts, _) = worker_store(root.path());
    let _workspace = grant_workspace(root.path(), &fresh);
    let config = serde_json::json!({"version":1,"computerId":"computer",
        "requiredCapabilities":["fixture-requirement"]});
    crate::inference::runtime_env::set_runtime_env_value(
        root.path(),
        "native_guest_host_config",
        &serde_json::to_string(&config).unwrap(),
    )
    .unwrap();
    target_handoff::configure_repository(
        root.path(),
        &serde_json::to_string(&target_handoff::TargetRepositoryAssignment {
            owner_user_id: "owner".into(),
            worker_profile_id: "profile".into(),
            project_id: "project".into(),
            working_copy_id: "copy".into(),
            repository_id: "repo".into(),
        })
        .unwrap(),
    )
    .unwrap();
    let spec = ExecutionSpec {
        job_id: "original-job".into(),
        session_id: facts.provider_session_id,
        scope_id: registry.authority.scope_id().into(),
        harness: ctox_core::native_harness_name().into(),
        harness_version: ctox_core::native_harness_version().into(),
        model_route_id: "openai".into(),
        gateway_account_id: "fixture".into(),
        model_id: "model".into(),
        required_capabilities: BTreeSet::from(["fixture-requirement".into()]),
    };
    let requested = target_handoff::TargetPolicyScope {
        owner_user_id: "owner".into(),
        worker_profile_id: "profile".into(),
        project_id: "project".into(),
        thread_id: "thread".into(),
        working_copy_id: "copy".into(),
        repository_id: "repo".into(),
        policy_revision: String::new(),
    };
    let scope = registry
        .with_policy(|policy| target_handoff::resolve(root.path(), policy, &requested, &spec))
        .unwrap();
    registry.guests.lock().unwrap().clear(); // virgin component fixture only
    let identity = protected_identity(root.path(), "original-guest");
    let ownership = Ownership {
        node_id: 4,
        generation: 2,
    };
    let binding = "a".repeat(64);
    let digest = "b".repeat(64);
    let enroll =
        |binding: &str, identity: &super::super::super::guest_runtime::ProtectedGuestIdentity| {
            registry.with_policy(|policy| {
                registry.enroll_protected_target_in_policy(
                    policy,
                    &scope,
                    identity,
                    binding,
                    &digest,
                    &spec,
                    &ownership,
                    &fresh.destination.import_parent,
                )
            })
        };
    let first = enroll(&binding, &identity).unwrap();
    assert_eq!(first.destination.guest_id, "original-guest");
    assert_ne!(
        first.destination.controller_id,
        fresh.destination.controller_id
    );
    assert_eq!(first.destination.controller_generation, 1);
    let second = enroll(&binding, &identity).unwrap();
    assert_eq!(
        second.destination.controller_id,
        first.destination.controller_id
    );
    assert_eq!(registry.guests.lock().unwrap().len(), 1);
    assert!(registry
        .admission("original-guest")
        .err()
        .unwrap()
        .to_string()
        .contains("never fresh Create"));
    assert!(registry
        .enroll(
            &session("owner"),
            "project",
            "thread",
            "profile",
            &fresh.destination.import_parent
        )
        .is_err());
    assert!(enroll(&"c".repeat(64), &identity).is_err());
    let foreign = protected_identity(root.path(), "foreign-guest");
    assert!(enroll(&binding, &foreign).is_err());
    let entry = registry.registration("original-guest").unwrap();
    {
        let entry = entry.lock().unwrap();
        let original = entry.restoration.as_ref().unwrap();
        assert_eq!(original.spec, spec);
        assert_eq!(original.ownership, ownership);
        assert_eq!(original.service_session, "original-service");
        assert!(entry.provider.is_none() && entry.execution.is_none() && entry.imported.is_none());
        assert!(entry.desktop.is_none() && entry.process_effect.is_none());
    }
    registry
        .revoke(&session("owner"), "original-guest")
        .unwrap();
    assert!(enroll(&binding, &identity).is_err());
    assert_eq!(registry.guests.lock().unwrap().len(), 1);
}
