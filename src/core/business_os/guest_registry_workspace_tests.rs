// Origin: CTOX
// License: AGPL-3.0-only
use super::super::workspaces as ws;
use super::*;

pub(super) fn grant_workspace(root: &Path, assignment: &NativeGuestAssignment) -> PathBuf {
    let workspace = root.join("actual-native-workspace");
    std::fs::create_dir(&workspace).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&workspace, std::fs::Permissions::from_mode(0o700)).unwrap();
    let policy = super::super::super::store::open_store(root).unwrap();
    policy.execute(
        "INSERT INTO business_users (user_id,display_name,role,active,created_at_ms,updated_at_ms)
        VALUES ('owner','Owner','user',1,1,1) ON CONFLICT(user_id) DO NOTHING", [],
    ).unwrap();
    put(
        &policy,
        "workjet_working_copies",
        "copy",
        json!({"owner_user_id":"owner","project_id":"project","computer_id":"computer",
            "status":"active","is_deleted":false,"path":"opaque-guest://not/a/host/path"}),
    );
    drop(policy);
    ws::configure_native_guest_assignments(root, "computer", &[], &[input(&workspace)]).unwrap();
    let policy = super::super::super::store::open_store(root).unwrap();
    let actual = ws::require(&policy, &assignment.destination, &workspace).unwrap();
    assert_eq!(actual.path, workspace);
    workspace
}

fn input(workspace: &Path) -> ws::WorkspaceAssignmentInput {
    ws::WorkspaceAssignmentInput {
        owner_user_id: "owner".into(),
        worker_profile_id: "profile".into(),
        project_id: "project".into(),
        working_copy_id: "copy".into(),
        native_workspace: workspace.into(),
    }
}

#[test]
fn native_workspace_requires_explicit_host_assignment_and_never_uses_workjet_path() {
    let (root, _registry, assignment) = fixture();
    let policy = super::super::super::store::open_store(root.path()).unwrap();
    assert!(ws::snapshot(&policy, &assignment.destination)
        .unwrap()
        .is_none());
    assert!(ws::require(&policy, &assignment.destination, root.path()).is_err());
    drop(policy);
    let workspace = grant_workspace(root.path(), &assignment);
    let policy = super::super::super::store::open_store(root.path()).unwrap();
    let resolved = ws::require(&policy, &assignment.destination, &workspace).unwrap();
    assert_eq!(resolved.working_copy_id, "copy");
    assert_eq!(resolved.path, workspace);
    assert!(ws::require(&policy, &assignment.destination, root.path()).is_err());
    let mut foreign = assignment.destination.clone();
    foreign.human_owner_id = "foreign".into();
    assert!(ws::require(&policy, &foreign, &workspace).is_err());
}

#[test]
fn native_workspace_replacement_symlink_and_foreign_writes_are_rejected() {
    let (root, _registry, assignment) = fixture();
    let workspace = grant_workspace(root.path(), &assignment);
    let policy = super::super::super::store::open_store(root.path()).unwrap();
    let retained = ws::require(&policy, &assignment.destination, &workspace).unwrap();
    std::fs::rename(&workspace, root.path().join("old-workspace")).unwrap();
    std::fs::create_dir(&workspace).unwrap();
    assert!(retained.verify().is_err());
    assert!(ws::require(&policy, &assignment.destination, &workspace).is_err());
    drop(policy);
    use std::os::unix::fs::{symlink, PermissionsExt};
    let alias = root.path().join("workspace-alias");
    symlink(&workspace, &alias).unwrap();
    assert!(
        ws::configure_native_guest_assignments(root.path(), "computer", &[], &[input(&alias)])
            .is_err()
    );
    std::fs::set_permissions(&workspace, std::fs::Permissions::from_mode(0o777)).unwrap();
    assert!(ws::configure_native_guest_assignments(
        root.path(),
        "computer",
        &[],
        &[input(&workspace)]
    )
    .is_err());
}

#[test]
fn native_workspace_grants_fence_revocation_regrant_epoch_and_detached_copy() {
    for mutation in ["revoked", "epoch", "detached", "foreign-computer"] {
        let (root, _registry, assignment) = fixture();
        let workspace = grant_workspace(root.path(), &assignment);
        let policy = super::super::super::store::open_store(root.path()).unwrap();
        let initial = ws::snapshot(&policy, &assignment.destination)
            .unwrap()
            .unwrap();
        match mutation {
            "revoked" => {
                drop(policy);
                ws::revoke_workspace_assignment(root.path(), "owner", "profile", "project")
                    .unwrap();
                let policy = super::super::super::store::open_store(root.path()).unwrap();
                assert!(ws::require(&policy, &assignment.destination, &workspace).is_err());
                drop(policy);
                ws::configure_native_guest_assignments(
                    root.path(),
                    "computer",
                    &[],
                    &[input(&workspace)],
                )
                .unwrap();
                let policy = super::super::super::store::open_store(root.path()).unwrap();
                let next = ws::snapshot(&policy, &assignment.destination)
                    .unwrap()
                    .unwrap();
                assert!(next["revision"].as_u64().unwrap() > initial["revision"].as_u64().unwrap());
                ws::require(&policy, &assignment.destination, &workspace).unwrap();
                continue;
            }
            "epoch" => {
                policy.execute("UPDATE business_users SET capability_epoch=capability_epoch+1 WHERE user_id='owner'", []).unwrap();
            }
            "detached" => put(
                &policy,
                "workjet_working_copies",
                "copy",
                json!({
                "owner_user_id":"owner","project_id":"project","computer_id":"computer",
                "status":"detached","is_deleted":false,"path":workspace}),
            ),
            "foreign-computer" => put(
                &policy,
                "workjet_working_copies",
                "copy",
                json!({
                "owner_user_id":"owner","project_id":"project","computer_id":"foreign-computer",
                "status":"active","is_deleted":false,"path":workspace}),
            ),
            _ => unreachable!(),
        }
        assert!(
            ws::require(&policy, &assignment.destination, &workspace).is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn native_workspace_invalid_configuration_rolls_back_provider_and_workspace_grants() {
    let (root, _registry, assignment) = fixture();
    let workspace = grant_workspace(root.path(), &assignment);
    let policy = super::super::super::store::open_store(root.path()).unwrap();
    let before = ws::snapshot(&policy, &assignment.destination).unwrap();
    drop(policy);
    let mut invalid = input(&workspace);
    invalid.working_copy_id = "foreign-copy".into();
    let provider = super::super::accounts::ProviderAssignmentInput {
        owner_user_id: "owner".into(),
        worker_profile_id: "profile".into(),
        gateway_account_id: "fixture-new-account".into(),
        model_id: "fixture-model".into(),
    };
    assert!(ws::configure_native_guest_assignments(
        root.path(),
        "computer",
        &[provider],
        &[invalid]
    )
    .is_err());
    let policy = super::super::super::store::open_store(root.path()).unwrap();
    assert_eq!(
        ws::snapshot(&policy, &assignment.destination).unwrap(),
        before
    );
    assert!(
        super::super::accounts::snapshot(&policy, &assignment.destination)
            .unwrap()
            .is_none()
    );
}

#[test]
fn native_workspace_lease_excludes_another_native_writer_until_stopped_capture_releases_it() {
    let (root, _registry, assignment) = fixture();
    let workspace = grant_workspace(root.path(), &assignment);
    let policy = super::super::super::store::open_store(root.path()).unwrap();
    let lease = ws::acquire_lease(&policy, &assignment.destination)
        .unwrap()
        .unwrap();
    assert!(ws::acquire_lease(&policy, &assignment.destination).is_err());
    ws::require(&policy, &assignment.destination, &workspace).unwrap();
    drop(lease);
    assert!(ws::acquire_lease(&policy, &assignment.destination)
        .unwrap()
        .is_some());
}

#[test]
fn native_core_constructor_resolves_the_assigned_source_workspace() {
    let (root, registry, assignment) = fixture();
    let (_worker, facts, _token) = worker_store(root.path());
    let context = facts.command_provenance.as_ref().unwrap();
    let guest = &assignment.destination.guest_id;
    assert_eq!(
        registry.continuation_workspace(guest, context).unwrap(),
        None
    );
    let workspace = grant_workspace(root.path(), &assignment);
    assert_eq!(
        registry.continuation_workspace(guest, context).unwrap(),
        Some(workspace.clone())
    );
    assert_ne!(workspace, root.path());
    let mut foreign = context.clone();
    foreign["actor"] = json!("foreign");
    assert!(registry.continuation_workspace(guest, &foreign).is_err());
    let mut expired = context.clone();
    expired["expires_at_ms"] = json!(1);
    assert!(registry.continuation_workspace(guest, &expired).is_err());
    ws::revoke_workspace_assignment(root.path(), "owner", "profile", "project").unwrap();
    assert!(registry.continuation_workspace(guest, context).is_err());
}

#[test]
fn native_core_constructor_rejects_a_replaced_source_workspace() {
    let (root, registry, assignment) = fixture();
    let (_worker, facts, _token) = worker_store(root.path());
    let workspace = grant_workspace(root.path(), &assignment);
    let context = facts.command_provenance.as_ref().unwrap();
    let guest = &assignment.destination.guest_id;
    assert!(registry
        .continuation_workspace(guest, context)
        .unwrap()
        .is_some());
    std::fs::rename(&workspace, root.path().join("original-workspace")).unwrap();
    std::fs::create_dir(&workspace).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&workspace, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(registry.continuation_workspace(guest, context).is_err());
}
