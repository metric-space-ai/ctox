use super::*;
use crate::business_os::domain_effect::{AppliedDomainEffect, DomainRecordRef};
use crate::business_os::store::{
    business_command_core_claim, load_rxdb_collection_record, rxdb_store_path,
};
use crate::business_os::store_projections::tests::create_repair_rxdb_tables;
use crate::business_os::store_workjet_projects::tests::create_workjet_rxdb_projection_tables;
use rusqlite::Connection;
use serde_json::json;
use tempfile::TempDir;

fn command() -> BusinessCommand {
    BusinessCommand {
        origin: CommandOrigin::TrustedLocal,
        id: Some("cmd-domain-recovery".into()),
        module: "ctox".into(),
        command_type: "ctox.workjet.project.upsert".into(),
        record_id: Some("domain-project".into()),
        payload: json!({"project_id":"domain-project","name":"Original"}),
        client_context: json!({"actor":{"id":"owner-1","role":"admin","is_admin":true}}),
    }
}

fn document(command: &BusinessCommand) -> Value {
    json!({"id":command.id,"command_id":command.id,"module":command.module,"command_type":command.command_type,
        "record_id":command.record_id,"payload":command.payload,"client_context":command.client_context})
}

fn fixture(applied: bool) -> anyhow::Result<(TempDir, BusinessCommand)> {
    let root = tempfile::tempdir()?;
    drop(create_repair_rxdb_tables(root.path())?);
    create_workjet_rxdb_projection_tables(root.path())?;
    let command = command();
    let claim = business_command_core_claim(command.id.as_deref().unwrap(), &command)?;
    let admission =
        DomainEffectAdmission::newly_claimed(&claim.command_id, &claim.payload_hash, "owner-1")?;
    assert_eq!(
        channels::claim_business_control_command(root.path(), claim)?.disposition,
        "new"
    );
    if applied {
        let mut conn = open_store(root.path())?;
        admission.apply(&mut conn, |tx| {
            upsert_business_record(tx, "workjet_projects", "domain-project", 1000,
                json!({"id":"domain-project","name":"Original","owner_user_id":"owner-1",
                    "created_at_ms":1000,"updated_at_ms":1000,"status":"active","is_deleted":false}))?;
            Ok(AppliedDomainEffect {
                result: json!({"ok":true,"original_result":"retained"}),
                projections: vec![DomainRecordRef { collection:"workjet_projects".into(), id:"domain-project".into() }],
            })
        })?;
    }
    Ok((root, command))
}

#[test]
fn domain_receipt_replay_recovers_current_source_without_reapplying() -> anyhow::Result<()> {
    let (root, command) = fixture(true)?;
    let conn = open_store(root.path())?;
    upsert_business_record(
        &conn,
        "workjet_projects",
        "domain-project",
        2000,
        json!({"id":"domain-project","name":"Later confirmed title","owner_user_id":"owner-1",
            "updated_at_ms":2000,"status":"active","is_deleted":false}),
    )?;
    let revision: String = conn.query_row(
        "SELECT rev FROM business_records WHERE collection='workjet_projects'",
        [],
        |row| row.get(0),
    )?;
    let rxdb = Connection::open(rxdb_store_path(root.path()))?;
    rxdb.execute_batch(
        "CREATE TRIGGER refuse_domain_projection BEFORE INSERT ON ctox_business_os__workjet_projects__v0
         BEGIN SELECT RAISE(ABORT, 'domain projection unavailable'); END;")?;
    assert!(accept_rxdb_business_command(root.path(), document(&command)).is_err());
    assert_eq!(
        channels::business_command_projection(root.path(), "cmd-domain-recovery")?
            ["terminal_status"],
        "none"
    );
    rxdb.execute_batch("DROP TRIGGER refuse_domain_projection")?;

    let outcome = accept_rxdb_business_command(root.path(), document(&command))?;
    assert_eq!(outcome["result"]["original_result"], "retained");
    assert_eq!(
        channels::business_command_projection(root.path(), "cmd-domain-recovery")?
            ["terminal_status"],
        "completed"
    );
    let projected = load_rxdb_collection_record(root.path(), "workjet_projects", "domain-project")?
        .context("project missing")?;
    assert_eq!(projected["name"], "Later confirmed title");
    assert_eq!(
        conn.query_row(
            "SELECT rev FROM business_records WHERE collection='workjet_projects'",
            [],
            |row| row.get::<_, String>(0)
        )?,
        revision
    );
    Ok(())
}

#[test]
fn domain_receipt_terminal_replay_repairs_missing_result_storage() -> anyhow::Result<()> {
    let (root, command) = fixture(true)?;
    let rxdb = Connection::open(rxdb_store_path(root.path()))?;
    let (table, schema): (String, String) = rxdb.query_row(
        "SELECT name, sql FROM sqlite_master WHERE type='table' AND name LIKE 'ctox_business_os__business_commands__v%'",
        [], |r| Ok((r.get(0)?,r.get(1)?)))?;
    rxdb.execute_batch(&format!("DROP TABLE {table}"))?;
    assert!(accept_rxdb_business_command(root.path(), document(&command)).is_err());
    assert_eq!(
        channels::business_command_projection(root.path(), "cmd-domain-recovery")?
            ["terminal_status"],
        "completed"
    );
    rxdb.execute_batch(&schema)?;
    accept_rxdb_business_command(root.path(), document(&command))?;
    let result =
        load_rxdb_collection_record(root.path(), "business_commands", "cmd-domain-recovery")?
            .context("result missing")?;
    assert_eq!(result["result"]["original_result"], "retained");
    assert_eq!(result["terminal_status"], "completed");
    Ok(())
}

#[test]
fn domain_receipt_preserves_tombstones_and_removes_obsolete_projection_fields() -> anyhow::Result<()>
{
    let (root, command) = fixture(true)?;
    accept_rxdb_business_command(root.path(), document(&command))?;
    let mut writers = RxdbProjectionWriterCache::new(root.path());
    writers.upsert_required(
        "workjet_projects",
        "domain-project",
        2000,
        json!({"obsolete_private_field":"must disappear"}),
    )?;
    let conn = open_store(root.path())?;
    conn.execute(
        "UPDATE business_records SET deleted=1, rev='later-delete', updated_at_ms=3000,
        payload_json=?1 WHERE collection='workjet_projects'",
        [
            json!({"id":"domain-project","is_deleted":true,"_deleted":true,"updated_at_ms":3000})
                .to_string(),
        ],
    )?;
    accept_rxdb_business_command(root.path(), document(&command))?;
    let rxdb = Connection::open(rxdb_store_path(root.path()))?;
    let (deleted, raw): (bool, String) = rxdb.query_row(
        "SELECT deleted,data FROM ctox_business_os__workjet_projects__v0 WHERE id='domain-project'",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    assert!(deleted);
    let projected: Value = serde_json::from_str(&raw)?;
    assert_eq!(projected["_deleted"], true);
    assert!(projected.get("obsolete_private_field").is_none());
    assert!(projected.get("name").is_none());
    Ok(())
}

#[test]
fn domain_receipt_rejects_changed_actor_payload_and_missing_proof() -> anyhow::Result<()> {
    let (root, command) = fixture(true)?;
    let mut changed = document(&command);
    changed["payload"]["name"] = json!("different intent");
    assert!(accept_rxdb_business_command(root.path(), changed).is_err());
    let mut changed = document(&command);
    changed["client_context"]["actor"]["id"] = json!("other-admin");
    assert!(accept_rxdb_business_command(root.path(), changed).is_err());
    assert_eq!(
        channels::business_command_projection(root.path(), "cmd-domain-recovery")?
            ["terminal_status"],
        "none"
    );
    let (empty_root, empty_command) = fixture(false)?;
    let outcome = accept_rxdb_business_command(empty_root.path(), document(&empty_command))?;
    assert_eq!(outcome["task_status"], "blocked");
    let conn = open_store(empty_root.path())?;
    assert!(!domain_effect::contains(&conn, "cmd-domain-recovery")?);
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM business_records WHERE collection='workjet_projects'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        0
    );
    Ok(())
}

#[tokio::test]
async fn domain_receipt_intake_resumes_an_old_applied_command_with_verified_identity(
) -> anyhow::Result<()> {
    use crate::business_os::rxdb_peer::{collection_creators, tests::open_test_database};
    use crate::business_os::rxdb_peer_intake::{
        business_commands_table_stamp, consume_pending_business_commands,
        pending_business_command_documents_sync,
    };
    let root = tempfile::tempdir()?;
    let token = crate::business_os::store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        "owner-1",
        "Owner",
        "admin",
        now_ms() as i64,
    )?
    .0;
    let mut command = command();
    command.client_context["capability_token"] = json!(token);
    let claim = business_command_core_claim(command.id.as_deref().unwrap(), &command)?;
    let admission =
        DomainEffectAdmission::newly_claimed(&claim.command_id, &claim.payload_hash, "owner-1")?;
    assert_eq!(
        channels::claim_business_control_command(root.path(), claim)?.disposition,
        "new"
    );
    let mut conn = open_store(root.path())?;
    admission.apply(&mut conn, |tx| {
        upsert_business_record(
            tx,
            "workjet_projects",
            "domain-project",
            1,
            json!({"id":"domain-project","name":"Durable original","owner_user_id":"owner-1",
                "created_at_ms":1,"updated_at_ms":1,"status":"active","is_deleted":false}),
        )?;
        Ok(AppliedDomainEffect {
            result: json!({"ok":true,"original_result":"retained"}),
            projections: vec![DomainRecordRef {
                collection: "workjet_projects".into(),
                id: "domain-project".into(),
            }],
        })
    })?;
    // A later committed title must survive automatic receipt recovery.
    upsert_business_record(
        &conn,
        "workjet_projects",
        "domain-project",
        2,
        json!({"id":"domain-project","name":"Later confirmed title","owner_user_id":"owner-1",
            "created_at_ms":1,"updated_at_ms":2,"status":"active","is_deleted":false}),
    )?;
    drop(conn);

    let database = open_test_database(rxdb_store_path(root.path())).await?;
    let mut creators = collection_creators();
    creators.retain(|name, _| matches!(name.as_str(), "business_commands" | "workjet_projects"));
    database
        .add_collections(creators)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut pending = document(&command);
    // Canonical replication strips bearer tokens. A poisoned browser snapshot
    // must not supply the actor, intent or a replacement credential for recovery.
    pending["client_context"] = json!({"actor":{"id":"untrusted-admin","role":"admin"}});
    pending["payload"]["name"] = json!("Untrusted replacement");
    assert!(
        accept_rxdb_business_command_with_origin(
            root.path(),
            pending.clone(),
            CommandOrigin::ReplicatedPeer,
        )
        .is_err(),
        "explicit browser requests still require a valid token"
    );
    pending["status"] = json!("accepted");
    pending["terminal_status"] = json!("none");
    pending["execution_phase"] = json!("accepted");
    pending["created_at_ms"] = json!(1);
    pending["updated_at_ms"] = json!(1);
    database
        .collection("business_commands")
        .context("command collection")?
        .insert(pending)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(business_commands_table_stamp(root.path())?.pending_count, 1);
    assert_eq!(
        pending_business_command_documents_sync(root.path(), 25)?.len(),
        1
    );
    let started = std::time::Instant::now();
    let mut failures = std::collections::HashMap::new();
    let consumed = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        consume_pending_business_commands(root.path(), &database, &mut failures),
    )
    .await??;
    assert_eq!(consumed, 1);
    assert!(failures.is_empty());
    let canonical = channels::business_command_projection(root.path(), "cmd-domain-recovery")?;
    assert_eq!(canonical["terminal_status"], "completed", "{canonical}");
    assert_eq!(canonical["result"]["original_result"], "retained");
    let projected = load_rxdb_collection_record(root.path(), "workjet_projects", "domain-project")?
        .context("project missing")?;
    assert_eq!(projected["name"], "Later confirmed title");
    assert_eq!(business_commands_table_stamp(root.path())?.pending_count, 0);
    assert!(pending_business_command_documents_sync(root.path(), 25)?.is_empty());
    eprintln!(
        "domain_receipt_native_intake recovery_ms={}",
        started.elapsed().as_secs_f64() * 1000.0
    );
    database.close().await.map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}

#[test]
fn native_domain_recovery_requires_active_actor_and_matching_core_identity() -> anyhow::Result<()> {
    let (root, command) = fixture(true)?;
    crate::business_os::store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        "owner-1",
        "Owner",
        "admin",
        now_ms() as i64,
    )?;
    let conn = open_store(root.path())?;
    conn.execute(
        "UPDATE business_users SET active=0 WHERE user_id='owner-1'",
        [],
    )?;
    let error =
        recover_applied_domain_effect_for_intake(root.path(), "cmd-domain-recovery").unwrap_err();
    assert!(error.to_string().contains("no longer active"), "{error:#}");
    assert_eq!(
        channels::business_command_projection(root.path(), "cmd-domain-recovery")?
            ["terminal_status"],
        "none"
    );
    conn.execute(
        "UPDATE business_users SET active=1 WHERE user_id='owner-1'",
        [],
    )?;
    conn.execute(
        "UPDATE business_command_domain_effects SET payload_hash='wrong-core-intent'",
        [],
    )?;
    let error =
        recover_applied_domain_effect_for_intake(root.path(), "cmd-domain-recovery").unwrap_err();
    assert!(
        error.to_string().contains("Core intent disagree"),
        "{error:#}"
    );
    assert_eq!(
        channels::business_command_projection(root.path(), "cmd-domain-recovery")?
            ["terminal_status"],
        "none"
    );
    let claim = business_command_core_claim("cmd-domain-recovery", &command)?;
    conn.execute(
        "UPDATE business_command_domain_effects SET payload_hash=?1",
        [claim.payload_hash],
    )?;
    assert!(
        recover_applied_domain_effect_for_intake(root.path(), "cmd-domain-recovery")?.is_some()
    );
    assert_eq!(
        channels::business_command_projection(root.path(), "cmd-domain-recovery")?
            ["terminal_status"],
        "completed"
    );
    assert!(recover_applied_domain_effect_for_intake(root.path(), "no-receipt")?.is_none());
    Ok(())
}

#[test]
fn domain_receipt_survives_intake_exhaustion_and_failure_writers() -> anyhow::Result<()> {
    let (root, command) = fixture(true)?;
    let failure = crate::business_os::store::record_business_command_intake_failure(
        root.path(),
        &document(&command),
        "domain projection unavailable",
        1,
    )?;
    assert_eq!(failure["exhausted"], true);
    assert_eq!(failure["canonical_failure_created"], false);
    assert_eq!(failure["terminal_projection_ready"], false);
    assert!(write_rxdb_control_command_outcome(
        root.path(),
        &command,
        "failed",
        None,
        Some("failed"),
        json!({"error":"projection error"}),
    )
    .is_err());
    assert_eq!(
        channels::business_command_projection(root.path(), "cmd-domain-recovery")?
            ["terminal_status"],
        "none"
    );
    accept_rxdb_business_command(root.path(), document(&command))?;
    assert_eq!(
        channels::business_command_projection(root.path(), "cmd-domain-recovery")?
            ["terminal_status"],
        "completed"
    );
    Ok(())
}
