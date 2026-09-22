// Origin: CTOX
// License: AGPL-3.0-only

use super::*;

#[test]
fn native_recovery_process_child() -> anyhow::Result<()> {
    let Some(path) = std::env::var_os("CTOX_TEST_PROJECT_CHAT_CRASH_ROOT") else {
        return Ok(());
    };
    let root = Path::new(&path);
    let error = submit(
        root,
        "ctox.workjet.project.worker.add",
        "native-process",
        json!({"project_id":"project","worker_profile_id":"profile-uuid"}),
    )
    .expect_err("injected post-commit publication failure");
    assert!(format!("{error:#}").contains("injected chat projection unavailable"));
    assert!(crate::business_os::domain_effect::contains(
        &open_store(root)?,
        "native-process"
    )?);
    // Terminate without destructors after real command dispatch committed its
    // domain effect but failed to deliver. Recovery runs in the parent process.
    std::process::exit(73);
}

#[test]
fn native_command_recovers_in_a_fresh_process_after_publication_failure() -> anyhow::Result<()> {
    let root = fixture()?;
    drop(fail_chat_publication(root.path())?);
    let name = format!("{}::native_recovery_process_child", module_path!());
    let name = name.split_once("::").context("test module path")?.1;
    let mut child = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env("CTOX_TEST_PROJECT_CHAT_CRASH_ROOT", root.path())
        .spawn()?;
    eprintln!(
        "project-chat test owns child pid={} until exit or 30s deadline",
        child.id()
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            result => {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!("native recovery child did not exit in time: {result:?}");
            }
        }
    };
    assert_eq!(
        status.code(),
        Some(73),
        "child did not reach post-commit failure"
    );
    assert_eq!(count(root.path(), CHATS)?, 2);
    assert_eq!(terminal(root.path(), "native-process")?, "none");
    let publication = Connection::open(store::rxdb_store_path(root.path()))?;
    publication.execute_batch("DROP TRIGGER reject_chat_projection")?;
    drop(publication);
    let result = submit(
        root.path(),
        "ctox.workjet.project.worker.add",
        "native-process",
        json!({"project_id":"project","worker_profile_id":"profile-uuid"}),
    )?;
    assert_eq!(terminal(root.path(), "native-process")?, "completed");
    let first = result["result"]["first_chat_id"]
        .as_str()
        .context("original result")?;
    assert!(store::load_rxdb_collection_record(root.path(), CHATS, first)?.is_some());
    assert_eq!(count(root.path(), CHATS)?, 2);
    assert_eq!(count(root.path(), THREADS)?, 2);
    Ok(())
}

fn submit(root: &Path, kind: &str, id: &str, payload: Value) -> anyhow::Result<Value> {
    submit_as(root, kind, id, payload, "owner")
}

fn submit_as(
    root: &Path,
    kind: &str,
    id: &str,
    payload: Value,
    owner: &str,
) -> anyhow::Result<Value> {
    crate::business_os::command_plane::accept_rxdb_business_command(
        root,
        json!({
            "id": id, "module": "ctox", "command_type": kind, "payload": payload,
            "client_context": {"actor": {"id": owner, "role": "admin", "is_admin": true}}
        }),
    )
}

fn fail_chat_publication(root: &Path) -> anyhow::Result<Connection> {
    let conn = Connection::open(store::rxdb_store_path(root))?;
    let schemas: Value = serde_json::from_str(include_str!("../business_os_schema_contract.json"))?;
    let version = schemas[CHATS]["version"]
        .as_u64()
        .context("chat schema version")?;
    conn.execute_batch(&format!(
        "CREATE TRIGGER reject_chat_projection BEFORE INSERT ON ctox_business_os__{CHATS}__v{version}
         BEGIN SELECT RAISE(ABORT, 'injected chat projection unavailable'); END;"
    ))?;
    Ok(conn)
}

fn terminal(root: &Path, id: &str) -> anyhow::Result<Value> {
    Ok(crate::mission::channels::business_command_projection(root, id)?["terminal_status"].clone())
}

#[test]
fn native_worker_add_recovers_without_restoring_removed_membership() -> anyhow::Result<()> {
    let root = fixture()?;
    let publication = fail_chat_publication(root.path())?;
    let payload = json!({"project_id":"project","worker_profile_id":"profile-uuid"});
    let error = submit(
        root.path(),
        "ctox.workjet.project.worker.add",
        "native-add",
        payload.clone(),
    )
    .expect_err("post-commit publication must fail");
    assert!(format!("{error:#}").contains("injected chat projection unavailable"));
    assert_eq!(count(root.path(), CHATS)?, 2);
    assert_eq!(count(root.path(), MEMBERS)?, 1);
    assert_eq!(terminal(root.path(), "native-add")?, "none");
    let conn = open_store(root.path())?;
    assert!(crate::business_os::domain_effect::contains(
        &conn,
        "native-add"
    )?);
    publication.execute_batch("DROP TRIGGER reject_chat_projection")?;
    drop(publication);
    drop(conn);

    // A later distinct authorized command wins; recovery must not run add again.
    submit(
        root.path(),
        "ctox.workjet.project.worker.remove",
        "native-remove",
        payload.clone(),
    )?;
    let restored = submit(
        root.path(),
        "ctox.workjet.project.worker.add",
        "native-add",
        payload,
    )?;
    assert_eq!(terminal(root.path(), "native-add")?, "completed");
    let membership_id = stable_id("workjet_member", &["owner", "project", "profile-uuid"]);
    let member = outbound_load_record(&open_store(root.path())?, MEMBERS, &membership_id)?
        .context("membership")?;
    assert_eq!(member["status"], "removed");
    assert_eq!(
        store::load_rxdb_collection_record(root.path(), MEMBERS, &membership_id)?
            .context("membership projection")?["status"],
        "removed"
    );
    let chat_id = restored["result"]["first_chat_id"]
        .as_str()
        .context("original chat result")?;
    assert!(store::load_rxdb_collection_record(root.path(), CHATS, chat_id)?.is_some());
    assert_eq!(count(root.path(), CHATS)?, 2);
    assert_eq!(count(root.path(), THREADS)?, 2);
    Ok(())
}

#[test]
fn native_explicit_chat_replay_preserves_original_result_and_distinct_intents() -> anyhow::Result<()>
{
    let root = fixture()?;
    submit(
        root.path(),
        "ctox.workjet.project.worker.add",
        "native-setup",
        json!({"project_id":"project","worker_profile_id":"profile-uuid"}),
    )?;
    let publication = fail_chat_publication(root.path())?;
    let payload =
        json!({"project_id":"project","worker_profile_id":"profile-uuid","title":"Original"});
    assert!(submit(
        root.path(),
        "ctox.workjet.project.chat.create",
        "native-create",
        payload.clone()
    )
    .is_err());
    assert_eq!(count(root.path(), CHATS)?, 3);
    assert_eq!(terminal(root.path(), "native-create")?, "none");
    let chat_id = stable_id(
        "workjet_private",
        &[
            "owner",
            "project",
            "profile-uuid",
            "explicit",
            "native-create",
        ],
    );
    let conn = open_store(root.path())?;
    let mut thread = outbound_load_record(&conn, THREADS, &chat_id)?.context("committed thread")?;
    thread["title"] = json!("Later title");
    store::upsert_business_record(&conn, THREADS, &chat_id, store::now_ms() as i64 + 1, thread)?;
    let revision: String = conn.query_row(
        "SELECT rev FROM business_records WHERE collection=?1 AND record_id=?2",
        [THREADS, &chat_id],
        |row| row.get(0),
    )?;
    drop(conn);
    publication.execute_batch("DROP TRIGGER reject_chat_projection")?;
    drop(publication);

    let repaired = submit(
        root.path(),
        "ctox.workjet.project.chat.create",
        "native-create",
        payload.clone(),
    )?;
    assert_eq!(repaired["result"]["chat_id"], chat_id);
    assert_eq!(terminal(root.path(), "native-create")?, "completed");
    assert_eq!(
        store::load_rxdb_collection_record(root.path(), THREADS, &chat_id)?
            .context("thread projection")?["title"],
        "Later title"
    );
    assert_eq!(
        open_store(root.path())?.query_row(
            "SELECT rev FROM business_records WHERE collection=?1 AND record_id=?2",
            [THREADS, &chat_id],
            |row| row.get::<_, String>(0)
        )?,
        revision
    );
    submit(
        root.path(),
        "ctox.workjet.project.chat.create",
        "native-create",
        payload.clone(),
    )?;
    let separate = submit(
        root.path(),
        "ctox.workjet.project.chat.create",
        "native-other",
        payload.clone(),
    )?;
    assert_ne!(separate["result"]["chat_id"], chat_id);
    assert_eq!(count(root.path(), CHATS)?, 4);

    let mut changed = payload.clone();
    changed["title"] = json!("Changed intent");
    assert!(submit(
        root.path(),
        "ctox.workjet.project.chat.create",
        "native-create",
        changed
    )
    .is_err());
    assert!(submit_as(
        root.path(),
        "ctox.workjet.project.chat.create",
        "native-create",
        payload,
        "foreign-admin"
    )
    .is_err());
    assert_eq!(count(root.path(), CHATS)?, 4);
    Ok(())
}

#[test]
fn native_project_upsert_recovers_its_default_group() -> anyhow::Result<()> {
    let root = fixture()?;
    let publication = fail_chat_publication(root.path())?;
    let payload = json!({"project_id":"recovery-project","name":"Recovery project"});
    assert!(submit(
        root.path(),
        "ctox.workjet.project.upsert",
        "native-project",
        payload.clone()
    )
    .is_err());
    assert_eq!(count(root.path(), CHATS)?, 1);
    assert!(outbound_load_record(
        &open_store(root.path())?,
        "workjet_projects",
        "recovery-project"
    )?
    .is_some());
    assert_eq!(terminal(root.path(), "native-project")?, "none");
    publication.execute_batch("DROP TRIGGER reject_chat_projection")?;
    drop(publication);
    let recovered = submit(
        root.path(),
        "ctox.workjet.project.upsert",
        "native-project",
        payload,
    )?;
    let group = recovered["result"]["group_chat_id"]
        .as_str()
        .context("group result")?;
    assert!(store::load_rxdb_collection_record(root.path(), CHATS, group)?.is_some());
    let ensured = submit(
        root.path(),
        "ctox.workjet.project.chat.ensure",
        "second-peer-ensure",
        json!({"project_id":"recovery-project"}),
    )?;
    assert_eq!(ensured["result"]["group_chat_id"], group);
    assert_eq!(count(root.path(), CHATS)?, 1);
    Ok(())
}

#[test]
fn native_receipt_abort_rolls_back_all_chat_domain_writes() -> anyhow::Result<()> {
    let root = fixture()?;
    let conn = open_store(root.path())?;
    conn.execute_batch(
        "CREATE TRIGGER reject_receipt BEFORE INSERT ON business_command_domain_effects
         WHEN NEW.command_id='native-before-commit'
         BEGIN SELECT RAISE(ABORT, 'injected receipt unavailable'); END;",
    )?;
    let outcome = submit(
        root.path(),
        "ctox.workjet.project.worker.add",
        "native-before-commit",
        json!({"project_id":"project","worker_profile_id":"profile-uuid"}),
    );
    assert!(outcome.is_err());
    assert!(!crate::business_os::domain_effect::contains(
        &conn,
        "native-before-commit"
    )?);
    assert_eq!(count(root.path(), CHATS)?, 0);
    assert_eq!(count(root.path(), MEMBERS)?, 0);
    assert_eq!(count(root.path(), THREADS)?, 0);
    Ok(())
}
