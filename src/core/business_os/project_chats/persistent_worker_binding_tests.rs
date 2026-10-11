// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use crate::business_os::project_chats::persistent_worker_binding as native;

const ROOT: &str = "cc6cfe73-2824-4360-9daf-3b3efb079931";
const PARENT: &str = "b8958e94-4c45-4b47-b44b-03b9d8c47123";

fn setup() -> anyhow::Result<TempDir> {
    let root = project_fixture()?;
    store::tests::seed_business_user(root.path(), "owner", "chef")?;
    store::tests::seed_business_user(root.path(), "foreign", "chef")?;
    issue(
        root.path(),
        "supervisor",
        "owner",
        "ctox.workjet.project.supervisor.bind",
        json!({"project_id":"project","thread_id":ROOT}),
    )?;
    Ok(root)
}

fn issue(
    root: &Path,
    operation: &str,
    owner: &str,
    kind: &str,
    payload: Value,
) -> anyhow::Result<Value> {
    let result = crate::business_os::command_plane::accept_rxdb_business_command(
        root,
        json!({
            "id":operation,"module":"ctox","command_type":kind,"payload":payload,
            "client_context":{"actor":{"id":owner,"role":"chef","is_admin":true}}
        }),
    )?;
    ensure!(result["status"] == "completed", "{result}");
    Ok(result["result"].clone())
}

fn payload(revision: u64) -> Value {
    json!({"project_id":"project","supervisor_thread_id":ROOT,"thread_id":PARENT,
        "expected_revision":revision,"title":"Persistent A"})
}

fn bind(
    root: &Path,
    operation: &str,
    revision: u64,
) -> anyhow::Result<native::PersistentWorkerBinding> {
    let result = issue(
        root,
        operation,
        "owner",
        "ctox.workjet.project.persistent_worker.bind",
        payload(revision),
    )?;
    Ok(serde_json::from_value(result["binding"].clone())?)
}

#[test]
fn persistent_worker_native_owner_enrollment_preserves_actual_uuid_history_and_replay(
) -> anyhow::Result<()> {
    let root = setup()?;
    let first = bind(root.path(), "parent-bind", 0)?;
    assert_eq!(first.thread_id, PARENT);
    assert_eq!(first.supervisor_thread_id, ROOT);
    assert_eq!(first.role, "persistent_worker");
    assert_eq!(first.revision, 1);
    let conn = open_store(root.path())?;
    native::require_active(&conn, "owner", "project", PARENT, &first)?;
    let mut thread = outbound_load_record(&conn, THREADS, PARENT)?.unwrap();
    assert_eq!(
        store::load_rxdb_collection_record(root.path(), THREADS, PARENT)?.unwrap()["metadata"],
        thread["metadata"]
    );
    thread["title"] = json!("Owner renamed it");
    thread["last_message_id"] = json!("kept-message");
    thread["last_message_at_ms"] = json!(1234);
    store::upsert_business_record(&conn, THREADS, PARENT, 2000, thread)?;
    let history = outbound_load_record(&conn, THREADS, PARENT)?.unwrap();
    assert_eq!(bind(root.path(), "parent-bind", 0)?, first);
    assert_eq!(bind(root.path(), "parent-confirm", 1)?, first);
    assert_eq!(
        outbound_load_record(&conn, THREADS, PARENT)?.unwrap(),
        history
    );
    let supervisors: i64 = conn.query_row(
        "SELECT count(*) FROM workjet_supervisor_bindings",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(supervisors, 1);
    Ok(())
}

#[test]
fn persistent_worker_revoke_rebind_invalidates_old_revision_without_resetting_history(
) -> anyhow::Result<()> {
    let root = setup()?;
    let original = bind(root.path(), "bind", 0)?;
    let conn = open_store(root.path())?;
    let revoked = issue(
        root.path(),
        "revoke",
        "owner",
        "ctox.workjet.project.persistent_worker.revoke",
        payload(1),
    )?;
    assert_eq!(revoked["binding"]["state"], "revoked");
    assert_eq!(revoked["binding"]["revision"], 2);
    assert!(native::require_active(&conn, "owner", "project", PARENT, &original).is_err());
    assert!(bind(root.path(), "stale-bind", 1).is_err());
    let fresh = bind(root.path(), "fresh-bind", 2)?;
    assert_eq!(fresh.revision, 3);
    native::require_active(&conn, "owner", "project", PARENT, &fresh)?;
    assert!(native::require_active(&conn, "owner", "project", PARENT, &original).is_err());
    assert_eq!(count(root.path(), THREADS)?, 2);
    Ok(())
}

#[test]
fn persistent_worker_rejects_foreign_unregistered_and_forged_provenance() -> anyhow::Result<()> {
    let root = setup()?;
    assert!(issue(
        root.path(),
        "foreign",
        "foreign",
        "ctox.workjet.project.persistent_worker.bind",
        payload(0)
    )
    .is_err());
    for (key, value) in [
        ("supervisor_thread_id", json!(PARENT)),
        ("thread_id", json!(ROOT)),
        ("thread_id", json!("00000000-0000-0000-0000-000000000000")),
        ("expected_revision", json!(4)),
        ("role", json!("supervisor")),
        ("owner_user_id", json!("foreign")),
    ] {
        let mut forged = payload(0);
        forged[key] = value.clone();
        assert!(issue(
            root.path(),
            &format!("forge-{key}-{value}"),
            "owner",
            "ctox.workjet.project.persistent_worker.bind",
            forged
        )
        .is_err());
    }
    let conn = open_store(root.path())?;
    assert!(native::for_thread(&conn, "owner", PARENT)?.is_none());
    assert_eq!(count(root.path(), THREADS)?, 1);
    Ok(())
}

#[test]
fn persistent_worker_metadata_cannot_mint_or_restore_authority() -> anyhow::Result<()> {
    let root = setup()?;
    let current = bind(root.path(), "bind", 0)?;
    let conn = open_store(root.path())?;
    let original = outbound_load_record(&conn, THREADS, PARENT)?.unwrap();
    for (field, value) in [
        ("workjet_team_role", json!("one_shot_worker")),
        ("workjet_team_binding", json!({"role":"persistent_worker"})),
    ] {
        let mut thread = original.clone();
        thread["metadata"][field] = value;
        store::upsert_business_record(&conn, THREADS, PARENT, 2000, thread)?;
        assert!(native::require_active(&conn, "owner", "project", PARENT, &current).is_err());
    }
    store::upsert_business_record(&conn, THREADS, PARENT, 2001, original.clone())?;
    let original = outbound_load_record(&conn, THREADS, PARENT)?.unwrap();
    conn.execute(
        "DELETE FROM workjet_persistent_worker_bindings WHERE thread_id=?1",
        [PARENT],
    )?;
    assert!(native::require_active(&conn, "owner", "project", PARENT, &current).is_err());
    // Even matching-looking metadata cannot be adopted as native provenance.
    assert!(bind(root.path(), "adopt-history", 0).is_err());
    assert_eq!(
        outbound_load_record(&conn, THREADS, PARENT)?.unwrap(),
        original
    );
    Ok(())
}

#[test]
fn persistent_worker_cannot_change_project_or_parent_affiliation() -> anyhow::Result<()> {
    let root = setup()?;
    let current = bind(root.path(), "bind", 0)?;
    let conn = open_store(root.path())?;
    let mut project = outbound_load_record(&conn, "workjet_projects", "project")?.unwrap();
    project["id"] = json!("other");
    store::upsert_business_record(&conn, "workjet_projects", "other", 2, project)?;
    assert!(native::require_active(&conn, "owner", "other", PARENT, &current).is_err());
    assert!(native::require_active(&conn, "foreign", "project", PARENT, &current).is_err());
    let mut change = payload(1);
    change["project_id"] = json!("other");
    assert!(issue(
        root.path(),
        "cross-project",
        "owner",
        "ctox.workjet.project.persistent_worker.bind",
        change
    )
    .is_err());
    Ok(())
}

#[test]
fn persistent_worker_archived_thread_and_project_fail_closed() -> anyhow::Result<()> {
    let root = setup()?;
    let current = bind(root.path(), "bind", 0)?;
    let conn = open_store(root.path())?;
    let original = outbound_load_record(&conn, THREADS, PARENT)?.unwrap();
    let mut archived = original.clone();
    archived["archived_at_ms"] = json!(10);
    store::upsert_business_record(&conn, THREADS, PARENT, 2000, archived)?;
    assert!(native::require_active(&conn, "owner", "project", PARENT, &current).is_err());
    store::upsert_business_record(&conn, THREADS, PARENT, 2001, original)?;
    let mut project = outbound_load_record(&conn, "workjet_projects", "project")?.unwrap();
    project["status"] = json!("archived");
    store::upsert_business_record(&conn, "workjet_projects", "project", 2002, project)?;
    assert!(native::require_active(&conn, "owner", "project", PARENT, &current).is_err());
    // Owner can still revoke archived project access.
    issue(
        root.path(),
        "archived-revoke",
        "owner",
        "ctox.workjet.project.persistent_worker.revoke",
        payload(1),
    )?;
    Ok(())
}
