// Origin: CTOX
// License: AGPL-3.0-only

//! Canonical policy/controller component tests. The authority fixture rejects
//! every operation: these tests cannot claim quorum, provider, guest or VM proof.
use super::*;
use ctox_sync::authority::{Job, Receipt, Request, WorkerMembership};
use serde_json::json;
use std::{future::Future, pin::Pin};

#[test]
fn native_prepared_overlay_accepts_private_helper_layout_and_rejects_foreign_aliases() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let state = tempfile::tempdir().unwrap();
    let runtime = state.path().join("runtime-guest");
    std::fs::create_dir(&runtime).unwrap();
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
    // Match the actual QemuOverlayPreparation directory and retained filename.
    let disk = tempfile::Builder::new()
        .prefix("guest-disk-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir_in(&runtime)
        .unwrap();
    let overlay = disk.path().join("root.qcow2");
    std::fs::write(&overlay, b"native disk").unwrap();
    validate_prepared_guest_overlay(&runtime, &overlay).unwrap();
    assert!(validate_prepared_guest_overlay(state.path(), &overlay).is_err());

    let alias = runtime.join("foreign.qcow2");
    symlink(&overlay, &alias).unwrap();
    assert!(validate_prepared_guest_overlay(&runtime, &alias).is_err());
    std::fs::remove_file(&alias).unwrap();
    std::fs::hard_link(&overlay, &alias).unwrap();
    assert!(validate_prepared_guest_overlay(&runtime, &overlay).is_err());
    std::fs::remove_file(&alias).unwrap();

    std::fs::set_permissions(disk.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(validate_prepared_guest_overlay(&runtime, &overlay).is_err());
    std::fs::set_permissions(disk.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::set_permissions(&overlay, std::fs::Permissions::from_mode(0o666)).unwrap();
    assert!(validate_prepared_guest_overlay(&runtime, &overlay).is_err());
    std::fs::set_permissions(&overlay, std::fs::Permissions::from_mode(0o600)).unwrap();
    validate_prepared_guest_overlay(&runtime, &overlay).unwrap();

    // Existing native reconstruction may place its disk directly in the assignment.
    let restored = runtime.join("restored.qcow2");
    std::fs::write(&restored, b"restored disk").unwrap();
    validate_prepared_guest_overlay(&runtime, &restored).unwrap();
}

struct RejectAuthority;

impl ExecutionAuthority for RejectAuthority {
    fn node_id(&self) -> u64 {
        4
    }
    fn scope_id(&self) -> &str {
        "native-test-scope"
    }
    fn worker_membership<'a, 'f>(
        &'a self,
        _: u64,
    ) -> Pin<Box<dyn Future<Output = io::Result<Option<WorkerMembership>>> + Send + 'f>>
    where
        'a: 'f,
        Self: 'f,
    {
        Box::pin(async { Err(io::Error::other("no quorum in policy component fixture")) })
    }
    fn submit<'a, 'f>(
        &'a self,
        _: Request,
    ) -> Pin<Box<dyn Future<Output = io::Result<Receipt>> + Send + 'f>>
    where
        'a: 'f,
        Self: 'f,
    {
        Box::pin(async { Err(io::Error::other("no quorum in policy component fixture")) })
    }
    fn validate_ownership<'a, 'b, 'c, 'f>(
        &'a self,
        _: &'b str,
        _: &'c Ownership,
    ) -> Pin<Box<dyn Future<Output = io::Result<Job>> + Send + 'f>>
    where
        'a: 'f,
        'b: 'f,
        'c: 'f,
        Self: 'f,
    {
        Box::pin(async { Err(io::Error::other("no quorum in policy component fixture")) })
    }
    fn shutdown<'a, 'f>(&'a self) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'f>>
    where
        'a: 'f,
        Self: 'f,
    {
        Box::pin(async { Err(io::Error::other("no quorum in policy component fixture")) })
    }
}
fn session(owner: &str) -> BusinessOsSession {
    BusinessOsSession {
        ok: true,
        authenticated: true,
        auth_required: true,
        login_url: None,
        reason: None,
        user: Some(super::super::session::BusinessOsSessionUser {
            id: owner.into(),
            display_name: owner.into(),
            role: "member".into(),
            is_admin: false,
        }),
    }
}
fn put(conn: &Connection, collection: &str, id: &str, value: serde_json::Value) {
    super::super::store::upsert_business_record(conn, collection, id, 1, value).unwrap();
}
fn fixture() -> (
    tempfile::TempDir,
    Arc<NativeGuestRegistry>,
    NativeGuestAssignment,
) {
    let directory = tempfile::tempdir().unwrap();
    let conn = super::super::store::open_store(directory.path()).unwrap();
    put(
        &conn,
        "workjet_projects",
        "project",
        json!({"owner_user_id":"owner","status":"active","is_deleted":false}),
    );
    put(
        &conn,
        "workjet_computers",
        "computer",
        json!({"owner_user_id":"owner","status":"assigned","hosting_mode":"self_hosted","is_deleted":false}),
    );
    let profile = super::super::worker_profile_bindings::binding_id("owner", "profile");
    put(
        &conn,
        "workjet_worker_profile_bindings",
        &profile,
        json!({"owner_user_id":"owner","worker_profile_id":"profile","computer_id":"computer","status":"active","is_deleted":false}),
    );
    let member =
        super::super::project_chats::stable_id("workjet_member", &["owner", "project", "profile"]);
    put(
        &conn,
        "workjet_project_workers",
        &member,
        json!({"owner_user_id":"owner","project_id":"project","worker_profile_id":"profile","group_chat_id":"thread","status":"active","is_deleted":false}),
    );
    put(
        &conn,
        "workjet_project_chats",
        "thread",
        json!({"owner_user_id":"owner","project_id":"project","thread_id":"thread","kind":"group","is_deleted":false}),
    );
    put(
        &conn,
        "user_threads",
        "thread",
        json!({"owner_user_id":"owner","status":"open","is_deleted":false,"archived_at_ms":0}),
    );
    drop(conn);
    let parent = directory.path().join("imports");
    std::fs::create_dir(&parent).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
    let registry = NativeGuestRegistry::new(
        directory.path(),
        Arc::new(RejectAuthority),
        BTreeSet::from(["fixture-requirement".into()]),
    )
    .unwrap();
    let assignment = registry
        .enroll(&session("owner"), "project", "thread", "profile", &parent)
        .unwrap();
    (directory, registry, assignment)
}
fn facts() -> NativeProviderFacts {
    NativeProviderFacts {
        schema: "ctox.native.worker_provider_preparation.v1",
        binding_id: "fixture-binding".into(),
        worker_id: "worker".into(),
        attempt_id: "attempt".into(),
        routing_attempts: vec![("task".into(), 1)],
        provider_session_id: uuid::Uuid::new_v4().to_string(),
        model_id: "model".into(),
        model_provider_id: None,
        api_provider_id: None,
        command_provenance: Some(
            json!({"actor":"owner","expires_at_ms":super::super::store::now_ms()+60_000,
            "crew_binding":{"attempt_id":"attempt"}}),
        ),
        checkpoint_contract: Some(crate::channels::NativeProviderCheckpointContract {
            harness: "fixture".into(),
            harness_version: "fixture".into(),
            model_route_id: "fixture".into(),
            gateway_account_id: "fixture".into(),
        }),
    }
}
fn worker_store() -> Connection {
    // ctox-allow-direct-state-write: isolated policy component fixture only
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE communication_routing_state (
        message_key TEXT, route_status TEXT, lease_owner TEXT, lease_worker_id TEXT,
        attempt INTEGER, lease_expires_at TEXT)",
    )
    .unwrap();
    conn.execute("INSERT INTO communication_routing_state VALUES ('task','leased','ctox-service','worker',1,?1)",
        [(chrono::Utc::now()+chrono::Duration::seconds(60)).to_rfc3339()]).unwrap();
    conn
}
#[test]
fn native_registry_enrollment_uses_canonical_owner_project_profile_and_chat() {
    let (_directory, registry, assignment) = fixture();
    assert_eq!(assignment.scope_id, "native-test-scope");
    assert!(registry
        .enroll(
            &session("foreign"),
            "project",
            "thread",
            "profile",
            &assignment.destination.import_parent
        )
        .is_err());
    assert!(registry
        .enroll(
            &session("owner"),
            "project",
            "foreign-thread",
            "profile",
            &assignment.destination.import_parent
        )
        .is_err());
    assert!(registry
        .enroll(
            &session("owner"),
            "project",
            "thread",
            "profile",
            &assignment.destination.import_parent
        )
        .is_err());
    assert!(registry.registration("unregistered").is_err());
    assert!(registry
        .bound_execution(&session("foreign"), &assignment.destination.guest_id)
        .is_err());
    assert!(registry
        .bound_execution(&session("owner"), &assignment.destination.guest_id)
        .is_err());
    assert!(registry
        .bound_execution(&session("owner"), "unregistered")
        .is_err());
}

#[test]
fn native_registry_denies_foreign_worker_root_before_publication() {
    let (_directory, registry, assignment) = fixture();
    let foreign = tempfile::tempdir().unwrap();
    let resolver = NativeGuestAdmissionResolver {
        registry: Arc::clone(&registry),
        guest_id: assignment.destination.guest_id.clone(),
    };
    let mut worker = worker_store();
    let tx = worker
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let mut published = false;
    assert!(resolver
        .with_current_destination(&tx, foreign.path(), &facts(), None, &mut |_| {
            published = true;
            Ok(())
        })
        .is_err());
    assert!(!published);
    resolver
        .with_current_destination(&tx, &registry.runtime_root, &facts(), None, &mut |_| {
            published = true;
            Ok(())
        })
        .unwrap();
    assert!(published);
}

#[test]
fn native_registry_root_replacement_denies_even_with_original_pinned_files() {
    let (directory, registry, _assignment) = fixture();
    let recovery = tempfile::tempdir().unwrap();
    let old = recovery.path().join("retained-original-root");
    std::fs::rename(directory.path(), &old).unwrap();
    std::fs::create_dir(directory.path()).unwrap();
    for path in [&registry.policy_path, &registry.instance_path] {
        let relative = path.strip_prefix(&registry.runtime_root).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::hard_link(old.join(relative), path).unwrap();
    }
    // Both original file pins still match. The retained root descriptor is
    // what rejects this replacement directory, before any policy callback.
    assert!(identity(&registry.policy_path).unwrap() == registry.policy_identity);
    assert!(identity(&registry.instance_path).unwrap() == registry.instance_file_identity);
    let mut published = false;
    assert!(registry
        .with_policy(|_| {
            published = true;
            Ok(())
        })
        .is_err());
    assert!(!published);
    std::fs::remove_dir_all(directory.path()).unwrap();
    std::fs::rename(&old, directory.path()).unwrap();
    registry.with_policy(|_| Ok(())).unwrap();
}

#[test]
fn native_registry_policy_writer_cannot_interleave_and_revocation_denies_publication() {
    let (_directory, registry, assignment) = fixture();
    let resolver = NativeGuestAdmissionResolver {
        registry: Arc::clone(&registry),
        guest_id: assignment.destination.guest_id.clone(),
    };
    let mut worker = worker_store();
    let tx = worker
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let facts = facts();
    let mut destination = None;
    resolver
        .with_current_destination(&tx, &registry.runtime_root, &facts, None, &mut |current| {
            let competing = Connection::open_with_flags(
                &registry.policy_path,
                OpenFlags::SQLITE_OPEN_READ_WRITE,
            )?;
            competing.busy_timeout(Duration::ZERO)?;
            let error = competing
                .execute(
                    "UPDATE business_records SET deleted=1 WHERE collection='workjet_projects'",
                    [],
                )
                .unwrap_err();
            assert_eq!(
                error.sqlite_error_code(),
                Some(rusqlite::ErrorCode::DatabaseBusy)
            );
            assert!(registry
                .registration(&assignment.destination.guest_id)?
                .try_lock()
                .is_err());
            destination = Some(current.clone());
            Ok(())
        })
        .unwrap();
    let previous = destination.unwrap();
    let conn = Connection::open(&registry.policy_path).unwrap();
    put(
        &conn,
        "workjet_projects",
        "project",
        json!({"owner_user_id":"owner","status":"active","name":"changed","is_deleted":false}),
    );
    let mut invoked = false;
    assert!(resolver
        .with_current_destination(
            &tx,
            &registry.runtime_root,
            &facts,
            Some(&previous),
            &mut |_| {
                invoked = true;
                Ok(())
            }
        )
        .is_err());
    assert!(!invoked);
    assert!(registry
        .revoke(&session("foreign"), &assignment.destination.guest_id)
        .is_err());
    registry
        .revoke(&session("owner"), &assignment.destination.guest_id)
        .unwrap();
    assert!(resolver
        .with_current_destination(&tx, &registry.runtime_root, &facts, None, &mut |_| {
            invoked = true;
            Ok(())
        })
        .is_err());
    assert!(!invoked);
}
#[test]
fn native_registry_replaced_policy_store_or_import_parent_never_invokes_callback() {
    let (_directory, registry, assignment) = fixture();
    let old = registry.policy_path.with_extension("retained");
    std::fs::rename(&registry.policy_path, &old).unwrap();
    std::fs::copy(&old, &registry.policy_path).unwrap();
    let mut invoked = false;
    assert!(registry
        .with_policy(|_| {
            invoked = true;
            Ok(())
        })
        .is_err());
    assert!(!invoked);
    assert!(old.exists());
    std::fs::remove_file(&registry.policy_path).unwrap();
    std::fs::rename(&old, &registry.policy_path).unwrap();
    let parent = &assignment.destination.import_parent;
    std::fs::rename(parent, parent.with_extension("retained")).unwrap();
    std::fs::create_dir(parent).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).unwrap();
    let resolver = NativeGuestAdmissionResolver {
        registry: Arc::clone(&registry),
        guest_id: assignment.destination.guest_id,
    };
    let mut worker = worker_store();
    let tx = worker.transaction().unwrap();
    assert!(resolver
        .with_current_destination(&tx, &registry.runtime_root, &facts(), None, &mut |_| {
            invoked = true;
            Ok(())
        })
        .is_err());
    assert!(!invoked);
}
#[test]
fn native_registry_missing_or_replaced_instance_identity_never_recreates_authority() {
    let (_directory, registry, _assignment) = fixture();
    let identity_path = &registry.instance_path;
    let retained = identity_path.with_extension("retained");
    let original = std::fs::read(identity_path).unwrap();
    std::fs::rename(identity_path, &retained).unwrap();
    let mut invoked = false;
    assert!(registry
        .with_policy(|_| {
            invoked = true;
            Ok(())
        })
        .is_err());
    assert!(!invoked);
    assert!(
        !identity_path.exists(),
        "publication must not reinitialize native identity"
    );
    assert_eq!(std::fs::read(&retained).unwrap(), original);

    std::fs::write(identity_path, &original).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(identity_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(registry
        .with_policy(|_| {
            invoked = true;
            Ok(())
        })
        .is_err());
    assert!(
        !invoked,
        "identical identity bytes in a replacement file grant no authority"
    );
    assert_eq!(std::fs::read(&retained).unwrap(), original);

    std::fs::remove_file(identity_path).unwrap();
    std::fs::rename(&retained, identity_path).unwrap();
    registry.with_policy(|_| Ok(())).unwrap();
}

#[test]
fn native_registry_archived_project_removed_member_or_expired_worker_deny_callback() {
    for (collection, id, value) in [
        (
            "workjet_projects",
            "project".to_owned(),
            json!({"owner_user_id":"owner","status":"archived"}),
        ),
        (
            "workjet_project_workers",
            super::super::project_chats::stable_id(
                "workjet_member",
                &["owner", "project", "profile"],
            ),
            json!({"owner_user_id":"owner","project_id":"project","worker_profile_id":"profile","status":"removed"}),
        ),
        (
            "user_threads",
            "thread".to_owned(),
            json!({"owner_user_id":"owner","status":"closed"}),
        ),
    ] {
        let (_directory, registry, assignment) = fixture();
        let conn = Connection::open(&registry.policy_path).unwrap();
        put(&conn, collection, &id, value);
        let resolver = NativeGuestAdmissionResolver {
            registry: registry.clone(),
            guest_id: assignment.destination.guest_id,
        };
        let mut worker = worker_store();
        let tx = worker.transaction().unwrap();
        let mut invoked = false;
        assert!(resolver
            .with_current_destination(&tx, &registry.runtime_root, &facts(), None, &mut |_| {
                invoked = true;
                Ok(())
            })
            .is_err());
        assert!(!invoked);
    }
    let (_directory, registry, assignment) = fixture();
    let resolver = NativeGuestAdmissionResolver {
        registry: registry.clone(),
        guest_id: assignment.destination.guest_id,
    };
    let mut worker = worker_store();
    worker
        .execute(
            "UPDATE communication_routing_state SET lease_expires_at='2000-01-01T00:00:00Z'",
            [],
        )
        .unwrap();
    let tx = worker.transaction().unwrap();
    let mut invoked = false;
    assert!(resolver
        .with_current_destination(&tx, &registry.runtime_root, &facts(), None, &mut |_| {
            invoked = true;
            Ok(())
        })
        .is_err());
    assert!(!invoked);
}
