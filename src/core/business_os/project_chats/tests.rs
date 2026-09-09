// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use crate::business_os::store::CommandOrigin;
use crate::business_os::{mcp_channel, store, threads, worker_profile_bindings};
use std::sync::{Arc, Barrier};
use tempfile::{tempdir, TempDir};

#[path = "recovery_tests.rs"]
mod recovery;

fn command(kind: &str, id: &str, payload: Value) -> BusinessCommand {
    BusinessCommand {
        id: Some(id.to_owned()),
        module: "ctox".to_owned(),
        command_type: kind.to_owned(),
        record_id: None,
        payload,
        client_context: json!({}),
        origin: CommandOrigin::TrustedLocal,
    }
}

// Component coverage retains the original direct-handler shape. The separate
// recovery tests below enter through the real command plane and its Core claim.
fn handle_command(root: &Path, command: &BusinessCommand, owner: &str) -> anyhow::Result<Value> {
    let operation = command.id.as_deref().context("fixture command id")?;
    let intent = format!("{}:{}:{}", command.command_type, owner, command.payload);
    let admission = DomainEffectAdmission::newly_claimed(operation, &intent, owner)?;
    let result = super::handle_command(root, command, owner, &admission)?;
    let conn = open_store(root)?;
    let effect = crate::business_os::domain_effect::load(&conn, operation, &intent, owner)?
        .context("fixture receipt missing")?;
    for reference in effect.projections {
        let record = outbound_load_record(&conn, &reference.collection, &reference.id)?
            .context("fixture source missing")?;
        upsert_rxdb_collection_record(
            root,
            &reference.collection,
            &reference.id,
            record["updated_at_ms"].as_i64().unwrap_or(0),
            record,
        )?;
    }
    Ok(result)
}

fn fixture() -> anyhow::Result<TempDir> {
    let root = tempdir()?;
    super::super::store_workjet_projects::tests::create_workjet_rxdb_projection_tables(
        root.path(),
    )?;
    let rxdb = Connection::open(store::rxdb_store_path(root.path()))?;
    let schemas: Value = serde_json::from_str(include_str!("../business_os_schema_contract.json"))?;
    for collection in [
        MEMBERS,
        worker_profile_bindings::COLLECTION,
        "user_thread_messages",
        "business_commands",
        "ctox_queue_tasks",
        "ctox_runs",
        "ctox_harness_events",
    ] {
        let version = schemas[collection]["version"]
            .as_u64()
            .context("canonical fixture version")?;
        rxdb.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS ctox_business_os__{collection}__v{version} (
            id TEXT PRIMARY KEY NOT NULL, revision TEXT, deleted INTEGER NOT NULL DEFAULT 0,
            lastWriteTime REAL NOT NULL DEFAULT 0, data TEXT NOT NULL);"
        ))?;
    }
    let conn = open_store(root.path())?;
    store::upsert_business_record(
        &conn,
        "workjet_projects",
        "project",
        1,
        json!({"id":"project","name":"Project","owner_user_id":"owner","status":"active","created_at_ms":1,"is_deleted":false}),
    )?;
    store::upsert_business_record(
        &conn,
        "workjet_computers",
        "computer",
        1,
        json!({"id":"computer","owner_user_id":"owner","status":"assigned","hosting_mode":"workstation","is_deleted":false}),
    )?;
    handle_command(
        root.path(),
        &command(
            "ctox.workjet.worker_profile.bind",
            "bind",
            json!({"worker_profile_id":"profile-uuid","computer_id":"computer"}),
        ),
        "owner",
    )?;
    Ok(root)
}

fn add(root: &Path, id: &str) -> anyhow::Result<Value> {
    handle_command(
        root,
        &command(
            "ctox.workjet.project.worker.add",
            id,
            json!({"project_id":"project","worker_profile_id":"profile-uuid"}),
        ),
        "owner",
    )
}

fn count(root: &Path, collection: &str) -> anyhow::Result<i64> {
    Ok(open_store(root)?.query_row(
        "SELECT count(*) FROM business_records WHERE collection=?1 AND deleted=0",
        [collection],
        |row| row.get(0),
    )?)
}

fn mcp_context(actor: &str, role: &str) -> mcp_channel::McpChannelRequestContext {
    mcp_channel::McpChannelRequestContext {
        channel: "test".into(),
        surface: "test".into(),
        actor: actor.into(),
        workspace: "test".into(),
        tool: "business_os.query".into(),
        request_id: "test".into(),
        confirmation_state: mcp_channel::McpConfirmationState::NotRequired,
        trusted_role: Some(role.into()),
        trusted_role_source: Some("test_authenticated_gateway".into()),
    }
}

#[test]
fn concurrent_peer_intents_share_one_group_membership_and_first_private_chat() -> anyhow::Result<()>
{
    let root = fixture()?;
    let barrier = Arc::new(Barrier::new(3));
    let workers = ["peer-a", "peer-b"].map(|id| {
        let path = root.path().to_path_buf();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            add(&path, id)
        })
    });
    barrier.wait();
    let [a, b] = workers;
    let a = a.join().expect("first peer")?;
    let b = b.join().expect("second peer")?;
    assert_eq!(a["group_chat_id"], b["group_chat_id"]);
    assert_eq!(a["first_chat_id"], b["first_chat_id"]);
    assert_eq!(count(root.path(), CHATS)?, 2);
    assert_eq!(count(root.path(), MEMBERS)?, 1);
    assert_eq!(count(root.path(), THREADS)?, 2);
    Ok(())
}

#[test]
fn explicit_chats_are_distinct_but_replaying_each_command_preserves_identity() -> anyhow::Result<()>
{
    let root = fixture()?;
    let initial = add(root.path(), "add")?;
    let create = |id| {
        handle_command(
            root.path(),
            &command(
                "ctox.workjet.project.chat.create",
                id,
                json!({"project_id":"project","worker_profile_id":"profile-uuid","title":"Independent work"}),
            ),
            "owner",
        )
    };
    let a = create("first")?; // May not collide with the first-chat discriminator.
    let b = create("another")?;
    assert_ne!(a["chat_id"], initial["first_chat_id"]);
    assert_ne!(a["chat_id"], b["chat_id"]);
    assert_eq!(a["chat_id"], create("first")?["chat_id"]);
    assert_eq!(count(root.path(), CHATS)?, 4);
    Ok(())
}

#[test]
fn membership_failure_rolls_back_group_and_membership_without_adopting_old_history(
) -> anyhow::Result<()> {
    let root = fixture()?;
    let collision = stable_id(
        "workjet_private",
        &["owner", "project", "profile-uuid", "first"],
    );
    store::upsert_business_record(
        &open_store(root.path())?,
        THREADS,
        &collision,
        1,
        json!({"id":collision,"title":"Existing unrelated history","participant_ids":["owner"],"status":"open"}),
    )?;
    assert!(add(root.path(), "add").is_err());
    assert_eq!(count(root.path(), CHATS)?, 0);
    assert_eq!(count(root.path(), MEMBERS)?, 0);
    assert_eq!(
        outbound_load_record(&open_store(root.path())?, THREADS, &collision)?.unwrap()["title"],
        "Existing unrelated history"
    );
    Ok(())
}

#[test]
fn removing_and_readding_a_worker_keeps_private_history() -> anyhow::Result<()> {
    let root = fixture()?;
    let initial = add(root.path(), "add")?;
    let chat = initial["first_chat_id"].as_str().unwrap();
    let conn = open_store(root.path())?;
    let mut history = outbound_load_record(&conn, THREADS, chat)?.unwrap();
    history["last_message_id"] = json!("existing-message");
    history["title"] = json!("My separate work");
    store::upsert_business_record(&conn, THREADS, chat, 2, history)?;
    handle_command(
        root.path(),
        &command(
            "ctox.workjet.project.worker.remove",
            "remove",
            json!({"project_id":"project","worker_profile_id":"profile-uuid"}),
        ),
        "owner",
    )?;
    assert!(handle_command(
        root.path(),
        &command(
            "ctox.workjet.project.chat.create",
            "new",
            json!({"project_id":"project","worker_profile_id":"profile-uuid","title":"Denied"})
        ),
        "owner"
    )
    .is_err());
    assert_eq!(
        add(root.path(), "again")?["first_chat_id"],
        initial["first_chat_id"]
    );
    let history = outbound_load_record(&conn, THREADS, chat)?.unwrap();
    assert_eq!(history["last_message_id"], "existing-message");
    assert_eq!(history["title"], "My separate work");
    Ok(())
}

#[test]
fn project_and_profile_checks_refuse_cross_owner_and_unknown_inputs() -> anyhow::Result<()> {
    let root = fixture()?;
    for payload in [
        json!({"project_id":"foreign-instance-project","worker_profile_id":"profile-uuid"}),
        json!({"project_id":"project","worker_profile_id":"unknown-profile"}),
        json!({"project_id":"project","worker_profile_id":"profile-uuid","owner_user_id":"owner"}),
    ] {
        assert!(handle_command(
            root.path(),
            &command("ctox.workjet.project.worker.add", "bad", payload),
            "owner"
        )
        .is_err());
    }
    assert!(handle_command(
        root.path(),
        &command(
            "ctox.workjet.project.worker.add",
            "other",
            json!({"project_id":"project","worker_profile_id":"profile-uuid"})
        ),
        "other-user"
    )
    .is_err());
    assert_eq!(count(root.path(), CHATS)?, 0);
    Ok(())
}

#[test]
fn unassigned_computer_and_unbound_profile_cannot_join_a_project() -> anyhow::Result<()> {
    let root = fixture()?;
    handle_command(
        root.path(),
        &command(
            "ctox.workjet.worker_profile.unbind",
            "unbind",
            json!({"worker_profile_id":"profile-uuid"}),
        ),
        "owner",
    )?;
    assert!(add(root.path(), "add").is_err());
    handle_command(
        root.path(),
        &command(
            "ctox.workjet.worker_profile.bind",
            "rebind",
            json!({"worker_profile_id":"profile-uuid","computer_id":"computer"}),
        ),
        "owner",
    )?;
    let conn = open_store(root.path())?;
    let mut computer = outbound_load_record(&conn, "workjet_computers", "computer")?.unwrap();
    computer["status"] = json!("unassigned");
    store::upsert_business_record(&conn, "workjet_computers", "computer", 2, computer)?;
    assert!(add(root.path(), "still-denied").is_err());
    Ok(())
}

#[test]
fn binding_uses_existing_core_crew_and_never_creates_an_identity() -> anyhow::Result<()> {
    let root = fixture()?;
    let core = Connection::open(crate::paths::core_db(root.path()))?;
    core.execute_batch(
        "CREATE TABLE IF NOT EXISTS crew_members (
        id TEXT PRIMARY KEY, name TEXT NOT NULL, shape TEXT NOT NULL, color TEXT NOT NULL,
        created_at TEXT NOT NULL, archived INTEGER NOT NULL, soul_json TEXT NOT NULL,
        specialties_json TEXT NOT NULL, stats_json TEXT NOT NULL, updated_at TEXT NOT NULL
    );",
    )?;
    let soul = json!({
        "gruendlichkeit_vs_tempo":50,"vorsicht_vs_mut":50,"knapp_vs_ausfuehrlich":50,
        "regeltreu_vs_kreativ":50,"nachfragen_vs_annehmen":50,"sketch":"Existing identity","voice":"Concise"
    }).to_string();
    for (id, archived, soul) in [
        ("existing-crew", false, soul.as_str()),
        ("archived-crew", true, soul.as_str()),
        ("malformed-crew", false, "invalid-json"),
    ] {
        core.execute("INSERT INTO crew_members
            (id,name,shape,color,created_at,archived,soul_json,specialties_json,stats_json,updated_at)
            VALUES (?1,'Existing','round','#123456','2026-09-09',?2,?3,'{}','{}','2026-09-09')",
            rusqlite::params![id, archived, soul])?;
    }
    let bind = |member| {
        handle_command(
            root.path(),
            &command(
                "ctox.workjet.worker_profile.bind",
                "crew",
                json!({"worker_profile_id":"profile-uuid","computer_id":"computer","crew_member_id":member}),
            ),
            "owner",
        )
    };
    assert!(bind("made-up-crew").is_err());
    assert!(bind("archived-crew").is_err());
    assert!(bind("malformed-crew").is_err());
    bind("existing-crew")?;
    let binding = worker_profile_bindings::require_active(
        &open_store(root.path())?,
        "owner",
        "profile-uuid",
    )?;
    assert_eq!(binding["worker_profile_id"], "profile-uuid");
    assert_eq!(binding["crew_member_id"], "existing-crew");
    let count: i64 = core.query_row("SELECT count(*) FROM crew_members", [], |r| r.get(0))?;
    assert_eq!(count, 3);
    Ok(())
}

#[test]
fn mcp_private_records_and_related_results_are_hidden_even_from_other_admins() -> anyhow::Result<()>
{
    let root = fixture()?;
    let result = add(root.path(), "add")?;
    let chat = result["first_chat_id"].as_str().unwrap();
    let mut projections = Vec::new();
    persist(
        &open_store(root.path())?,
        "user_thread_messages",
        "private-message",
        json!({"id":"private-message","thread_id":chat,"body":"Private details","updated_at_ms":1}),
        &mut projections,
    )?;
    persist(
        &open_store(root.path())?,
        "business_commands",
        "private-command",
        json!({"id":"private-command","command_type":"threads.message.create","payload":{"thread_id":chat,"body":"Private details"},"updated_at_ms":1}),
        &mut projections,
    )?;
    publish(root.path(), &projections)?;
    for role in ["user", "admin", "chef", "founder"] {
        let other = mcp_context("other-user", role);
        assert!(mcp_channel::get_record(root.path(), &other, THREADS, chat).is_err());
        assert_eq!(
            mcp_channel::query_records(root.path(), &other, "user_thread_messages", Some(100))?
                .count,
            0
        );
        assert_eq!(
            mcp_channel::query_records(root.path(), &other, "business_commands", Some(100))?.count,
            0
        );
        assert_eq!(
            mcp_channel::list_record_activity(root.path(), &other, THREADS, chat, Some(100))?.count,
            0
        );
        assert!(mcp_channel::get_command_status(root.path(), &other, "private-command").is_err());
    }
    let owner = mcp_context("owner", "admin");
    assert_eq!(
        mcp_channel::get_record(root.path(), &owner, THREADS, chat)?
            .record
            .data["id"],
        chat
    );
    assert_eq!(
        mcp_channel::query_records(root.path(), &owner, "user_thread_messages", Some(100))?.count,
        1
    );
    Ok(())
}

#[test]
fn webrtc_filter_rechecks_project_revocation_and_does_not_trust_snapshot_participants(
) -> anyhow::Result<()> {
    let root = fixture()?;
    let result = add(root.path(), "add")?;
    let chat = result["first_chat_id"].as_str().unwrap();
    let conn = open_store(root.path())?;
    let record = outbound_load_record(&conn, THREADS, chat)?.unwrap();
    let now = i64::try_from(store::now_ms())?;
    let (owner, _) = store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        "owner",
        "Owner",
        "admin",
        now,
    )?;
    let (other, _) = store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        "other",
        "Other",
        "admin",
        now,
    )?;
    let filter = threads::replication_document_filter(root.path(), &owner, THREADS);
    assert!(filter(&record));
    assert!(!threads::may_replicate_document(
        root.path(),
        &other,
        THREADS,
        &record
    ));
    let mut forged = record.clone();
    forged["owner_user_id"] = json!("other");
    forged["participant_ids"] = json!(["other"]);
    assert!(!threads::may_replicate_document(
        root.path(),
        &other,
        THREADS,
        &forged
    ));
    assert!(!threads::may_replicate_document(
        root.path(),
        &owner,
        THREADS,
        &json!({"id":"workjet_private_foreign-instance","participant_ids":["owner"]})
    ));
    let mut project = outbound_load_record(&conn, "workjet_projects", "project")?.unwrap();
    project["owner_user_id"] = json!("new-owner");
    store::upsert_business_record(&conn, "workjet_projects", "project", 2, project)?;
    assert!(!filter(&record));
    assert!(
        mcp_channel::get_record(root.path(), &mcp_context("owner", "admin"), THREADS, chat)
            .is_err()
    );
    Ok(())
}

#[test]
fn private_execution_results_follow_native_command_and_task_relationships() -> anyhow::Result<()> {
    use crate::mission::channels;
    let root = fixture()?;
    let initial = add(root.path(), "add")?;
    let chat = initial["first_chat_id"].as_str().unwrap();
    let intent = command(
        "business_os.chat.task",
        "private-intent",
        json!({"thread_id":chat,"instruction":"Private work","title":"Private task"}),
    );
    // Use the real admission transaction used by create_ctox_queue_task.
    // No command/queue mirror rows are seeded into business_records.
    let admitted = channels::claim_business_command_with_queue(
        root.path(),
        store::business_command_core_claim("private-intent", &intent)?,
        channels::QueueTaskCreateRequest {
            title: "Private task".into(),
            prompt: "Private work".into(),
            thread_key: "private-command-thread".into(),
            workspace_root: Some(root.path().display().to_string()),
            priority: "normal".into(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: Some(
                json!({"source":"business-os","business_os_command_id":"private-intent"}),
            ),
        },
    )?;
    let task_id = admitted.task.message_key;
    assert!(channels::load_queue_task(root.path(), &task_id)?.is_some());
    assert_eq!(
        channels::business_command_projection(root.path(), "private-intent")?["payload"]
            ["thread_id"],
        chat
    );
    let conn = open_store(root.path())?;
    assert!(outbound_load_record(&conn, "business_commands", "private-intent")?.is_none());
    assert!(outbound_load_record(&conn, "ctox_queue_tasks", &task_id)?.is_none());
    let records = [
        (
            "ctox_runs",
            "private-run",
            json!({
                "id":"private-run","task_id":task_id,"retrospective":"Private result","updated_at_ms":1
            }),
        ),
        (
            "ctox_harness_events",
            "private-event",
            json!({
                "id":"private-event","task_id":task_id,"title":"Private tool result","updated_at_ms":1
            }),
        ),
    ];
    let mut projections = Vec::new();
    for (collection, id, record) in &records {
        persist(&conn, collection, id, record.clone(), &mut projections)?;
    }
    publish(root.path(), &projections)?;
    for (collection, id, record) in &records {
        assert_eq!(
            document_visible_to_actor(root.path(), collection, record, "owner"),
            Some(true)
        );
        for role in ["user", "admin", "chef", "founder"] {
            assert_eq!(
                document_visible_to_actor(root.path(), collection, record, "other-user"),
                Some(false)
            );
            let other = mcp_context("other-user", role);
            assert!(mcp_channel::get_record(root.path(), &other, collection, id).is_err());
            assert_eq!(
                mcp_channel::query_records(root.path(), &other, collection, Some(100))?.count,
                0
            );
        }
        assert_eq!(
            mcp_channel::get_record(root.path(), &mcp_context("owner", "admin"), collection, id)?
                .record
                .data["id"],
            *id
        );
    }
    // An unresolved typed reference cannot establish that an execution is public.
    assert_eq!(
        document_visible_to_actor(
            root.path(),
            "ctox_runs",
            &json!({"id":"orphan-run","task_id":"missing-task"}),
            "owner"
        ),
        Some(false)
    );

    let public = command(
        "business_os.chat.task",
        "public-intent",
        json!({"instruction":"Ordinary work"}),
    );
    let public_task = channels::claim_business_command_with_queue(
        root.path(),
        store::business_command_core_claim("public-intent", &public)?,
        channels::QueueTaskCreateRequest {
            title: "Ordinary task".into(),
            prompt: "Ordinary work".into(),
            thread_key: "ordinary-command-thread".into(),
            workspace_root: Some(root.path().display().to_string()),
            priority: "normal".into(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: Some(json!({"business_os_command_id":"public-intent"})),
        },
    )?;
    assert_eq!(
        document_visible_to_actor(
            root.path(),
            "ctox_runs",
            &json!({"id":"ordinary-run","task_id":public_task.task.message_key}),
            "owner"
        ),
        None
    );
    let mismatch = command(
        "business_os.chat.task",
        "mismatched-intent",
        json!({"thread_id":chat,"instruction":"Private work"}),
    );
    let mismatched_task = channels::claim_business_command_with_queue(
        root.path(),
        store::business_command_core_claim("mismatched-intent", &mismatch)?,
        channels::QueueTaskCreateRequest {
            title: "Private task".into(),
            prompt: "Private work".into(),
            thread_key: "mismatched-command-thread".into(),
            workspace_root: Some(root.path().display().to_string()),
            priority: "normal".into(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: Some(json!({"business_os_command_id":"mismatched-intent"})),
        },
    )?;
    // Admit each command through its own finite spawn budget first. Then
    // simulate corrupted metadata on an existing task; the privacy reader must
    // reject its pointer to another command without relaxing admission guards.
    channels::set_queue_task_metadata_value(
        root.path(),
        &mismatched_task.task.message_key,
        "business_os_command_id",
        json!("public-intent"),
    )?;
    assert_eq!(
        document_visible_to_actor(
            root.path(),
            "ctox_runs",
            &json!({"id":"mismatched-run","task_id":mismatched_task.task.message_key}),
            "other-user"
        ),
        Some(false)
    );
    Ok(())
}

#[test]
fn private_threads_reject_foreign_human_mentions_and_admin_mutation() -> anyhow::Result<()> {
    let root = fixture()?;
    let initial = add(root.path(), "add")?;
    let chat = initial["first_chat_id"].as_str().unwrap();
    let mut message = command(
        "threads.message.create",
        "message",
        json!({"thread_id":chat,"body":"My private message","target_user_ids":["other-user"]}),
    );
    message.module = "threads".into();
    assert!(command_access_check(root.path(), &message, "owner").is_err());
    message.payload["target_user_ids"] = json!(["owner"]);
    command_access_check(root.path(), &message, "owner")?;
    assert!(command_access_check(root.path(), &message, "other-user").is_err());
    Ok(())
}
#[test]
fn measures_real_mcp_and_replication_pages_with_repeated_execution_references() -> anyhow::Result<()>
{
    use crate::mission::channels;
    use std::time::Instant;
    let root = fixture()?;
    let initial = add(root.path(), "add")?;
    let chat = initial["first_chat_id"].as_str().unwrap();
    let conn = open_store(root.path())?;
    let mut task_ids = Vec::new();
    for index in 0..4 {
        let command_id = format!("page-command-{index}");
        let mut payload = json!({"instruction":"Representative work"});
        if index % 2 == 0 {
            payload["thread_id"] = json!(chat);
        }
        let intent = command("business_os.chat.task", &command_id, payload);
        let admitted = channels::claim_business_command_with_queue(
            root.path(),
            store::business_command_core_claim(&command_id, &intent)?,
            channels::QueueTaskCreateRequest {
                title: "Page measurement".into(),
                prompt: "Representative work".into(),
                thread_key: format!("page-thread-{index}"),
                workspace_root: Some(root.path().display().to_string()),
                priority: "normal".into(),
                suggested_skill: None,
                parent_message_key: None,
                extra_metadata: Some(json!({"business_os_command_id":command_id})),
            },
        )?;
        task_ids.push(admitted.task.message_key);
    }
    let mut projections = Vec::new();
    let mut pages = Vec::new();
    for collection in ["ctox_runs", "ctox_harness_events"] {
        let page = (0..100)
            .map(|index| {
                json!({
                    "id":format!("{collection}-page-{index:03}"),
                    "task_id":task_ids[index % task_ids.len()],
                    "title":"Representative result","updated_at_ms":index + 1,
                })
            })
            .collect::<Vec<_>>();
        for record in &page {
            persist(
                &conn,
                collection,
                record["id"].as_str().unwrap(),
                record.clone(),
                &mut projections,
            )?;
        }
        pages.push((collection, page));
    }
    publish(root.path(), &projections)?;
    let now = i64::try_from(store::now_ms())?;
    let (other, _) = store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        "page-other",
        "Other user",
        "admin",
        now,
    )?;
    let db_path = crate::paths::core_db(root.path());
    for (collection, page) in pages {
        let filter = threads::replication_document_filter(root.path(), &other, collection);
        let mut query_samples = Vec::new();
        let mut replication_samples = Vec::new();
        for sample in 0..10 {
            channels::reset_channel_db_open_count_for_tests(&db_path);
            let started = Instant::now();
            let queried = mcp_channel::query_records(
                root.path(),
                &mcp_context("page-other", "admin"),
                collection,
                Some(100),
            )?;
            let query_ms = started.elapsed().as_secs_f64() * 1000.0;
            let query_opens = channels::channel_db_open_count_for_tests(&db_path);
            assert_eq!(query_opens, 1, "one Core reader per bounded MCP page");
            assert_eq!(
                queried.count, 50,
                "foreign admin must only receive the ordinary executions"
            );

            channels::reset_channel_db_open_count_for_tests(&db_path);
            let started = Instant::now();
            let visible = page.iter().filter(|record| filter(record)).count();
            let replication_ms = started.elapsed().as_secs_f64() * 1000.0;
            let replication_opens = channels::channel_db_open_count_for_tests(&db_path);
            assert_eq!(
                replication_opens,
                usize::from(sample == 0),
                "one reader per filter lifetime"
            );
            assert_eq!(visible, 50);
            query_samples.push(query_ms);
            replication_samples.push(replication_ms);
            eprintln!(
                "workjet_privacy_page_measurement {}",
                json!({
                    "collection":collection,"rows":page.len(),"distinct_tasks":task_ids.len(),"sample":sample,
                    "mcp_visible":queried.count,"mcp_core_opens":query_opens,"mcp_ms":query_ms,
                    "replication_visible":visible,"replication_core_opens":replication_opens,
                    "replication_ms":replication_ms,
                })
            );
        }
        query_samples.sort_by(f64::total_cmp);
        replication_samples.sort_by(f64::total_cmp);
        eprintln!(
            "workjet_privacy_page_percentiles {}",
            json!({
                "collection":collection,"samples":10,"rows":page.len(),
                "mcp_p50_ms":query_samples[4],"mcp_p95_ms":query_samples[9],
                "replication_p50_ms":replication_samples[4],"replication_p95_ms":replication_samples[9],
            })
        );
    }
    // Even inside one page's reference reader, ownership is not cached.
    let private_record = json!({"id":"revocation-run","task_id":task_ids[0]});
    let mut reader = VisibilityReadContext::new(root.path());
    assert_eq!(
        reader.visible("ctox_runs", &private_record, "owner"),
        Some(true)
    );
    let mut project = outbound_load_record(&conn, "workjet_projects", "project")?.unwrap();
    project["owner_user_id"] = json!("new-owner");
    store::upsert_business_record(&conn, "workjet_projects", "project", 2, project)?;
    assert_eq!(
        reader.visible("ctox_runs", &private_record, "owner"),
        Some(false)
    );

    // Both a later page and the same live filter observe changed canonical
    // metadata and fail closed instead of reusing a public result.
    let filter = threads::replication_document_filter(root.path(), &other, "ctox_runs");
    let public_record = json!({"id":"ordinary-run","task_id":task_ids[1]});
    assert!(filter(&public_record));
    channels::set_queue_task_metadata_value(
        root.path(),
        &task_ids[1],
        "business_os_command_id",
        json!("missing-command"),
    )?;
    assert!(
        !filter(&public_record),
        "live filter must reread canonical references"
    );
    assert_eq!(
        mcp_channel::query_records(
            root.path(),
            &mcp_context("page-other", "admin"),
            "ctox_runs",
            Some(100),
        )?
        .count,
        25
    );
    Ok(())
}

#[test]
fn project_crew_admission_uses_native_chat_binding_and_rejects_revocation() -> anyhow::Result<()> {
    use crate::mission::channels;
    let root = fixture()?;
    let added = add(root.path(), "crew-project-add")?;
    let chat = added["first_chat_id"].as_str().unwrap();
    let conn = open_store(root.path())?;
    // Existing legacy profiles have no Crew binding: never pick a random
    // identity and claim it is the worker shown in this private chat.
    assert!(super::super::project_crew::member_for_chat(&conn, "owner", chat).is_err());
    let core = Connection::open(crate::paths::core_db(root.path()))?;
    crate::crew::ensure_schema(&core)?;
    let soul = json!({"gruendlichkeit_vs_tempo":50,"vorsicht_vs_mut":50,
        "knapp_vs_ausfuehrlich":50,"regeltreu_vs_kreativ":50,"nachfragen_vs_annehmen":50,
        "sketch":"Project identity","voice":"Concise"})
    .to_string();
    for member in ["project-crew", "other-project-crew"] {
        core.execute("INSERT INTO crew_members
            (id,name,shape,color,created_at,archived,soul_json,specialties_json,stats_json,updated_at)
            VALUES (?1,?1,'round','#123456','2026-09-09',0,?2,'{}','{}','2026-09-09')",
            rusqlite::params![member,soul])?;
    }
    handle_command(
        root.path(),
        &command(
            "ctox.workjet.worker_profile.bind",
            "bind-project-crew",
            json!({"worker_profile_id":"profile-uuid","computer_id":"computer","crew_member_id":"project-crew"}),
        ),
        "owner",
    )?;
    assert!(super::super::project_crew::member_for_chat(&conn, "other-user", chat).is_err());
    let (_capability, _) = store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        "owner",
        "Owner",
        "admin",
        chrono::Utc::now().timestamp_millis(),
    )?;
    let request = json!({"thread_id":chat,"title":"Project task","instruction":"Work on the project",
        "harness":"codex","timeout_seconds":10,"idempotency_key":"project-start-1",
        "_context":{"actor":"owner","workspace":"project-test"}});
    let start = |request: Value| {
        mcp_channel::call_tool(root.path(), "business_os.start_crew_execution", request)
    };
    let accepted = start(request.clone())?;
    let replay = start(request.clone())?;
    assert_eq!(accepted["command_id"], replay["command_id"]);
    assert_eq!(accepted["task_id"], replay["task_id"]);
    assert_eq!(accepted["executor_id"], "computer");
    assert_eq!(accepted["crew_member_id"], "project-crew");
    let mut changed = request.clone();
    changed["instruction"] = json!("Different intent");
    assert!(start(changed).is_err());
    let mut spoofed = request.clone();
    spoofed["crew_member_id"] = json!("other-project-crew");
    assert!(start(spoofed).is_err());
    let mut foreign = request.clone();
    foreign["_context"]["actor"] = json!("other-user");
    assert!(start(foreign).is_err());
    let canonical = channels::business_command_projection(
        root.path(),
        accepted["command_id"].as_str().unwrap(),
    )?;
    assert_eq!(canonical["command_type"], "business_os.chat.task");
    assert_eq!(canonical["payload"]["thread_id"], chat);
    assert_eq!(
        canonical["payload"]["external_executor"]["executor_id"],
        "computer"
    );
    let task_id = accepted["task_id"]
        .as_str()
        .context("project task missing")?;
    assert_eq!(
        super::super::project_crew_member_for_task(root.path(), task_id)?,
        Some("project-crew".into())
    );
    channels::lease_queue_task(root.path(), task_id, "project-worker")?;
    let prepare = || {
        crate::crew::prepare_attempt(
            root.path(),
            &[task_id.to_owned()],
            "project-worker",
            "project-attempt",
            Some(chat),
            &json!({}),
            None,
            "Work on the project",
            None,
        )
    };
    let selected = prepare()?.context("bound project Crew missing")?;
    assert_eq!(selected.member_id, "project-crew");
    assert_eq!(
        prepare()?
            .context("resumed project Crew missing")?
            .member_id,
        "project-crew"
    );
    core.execute("UPDATE communication_routing_state SET crew_assigned_member_id='other-project-crew' WHERE message_key=?1", [task_id])?;
    assert!(prepare()
        .unwrap_err()
        .to_string()
        .contains("conflicts with the project worker"));
    core.execute(
        "UPDATE communication_routing_state SET crew_assigned_member_id=NULL WHERE message_key=?1",
        [task_id],
    )?;
    handle_command(
        root.path(),
        &command(
            "ctox.workjet.project.worker.remove",
            "remove-project-crew",
            json!({"project_id":"project","worker_profile_id":"profile-uuid"}),
        ),
        "owner",
    )?;
    assert!(super::super::project_crew_member_for_task(root.path(), task_id).is_err());
    assert!(prepare().is_err());
    let stored: String = core.query_row(
        "SELECT member_id FROM crew_attempts WHERE attempt_id='project-attempt'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(
        stored, "project-crew",
        "revocation must not retarget the existing attempt"
    );
    Ok(())
}
