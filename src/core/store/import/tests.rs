use super::base_release::{self, MigrationEvaluator, ReleasePlan};
#[path = "tests/files.rs"]
mod files;
use super::*;
use crate::{registry::Registry, schema, types::DbId, writer::WriterOptions};
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc};
#[path = "tests/legacy.rs"]
mod legacy;
#[path = "tests/profile.rs"]
mod profile;
#[path = "tests/release_input.rs"]
mod release_input;

struct ReleaseMigration;
impl MigrationEvaluator for ReleaseMigration {
    fn migrate(&self, collection: &str, _: &Value, mut body: Value) -> Result<Value> {
        // The production schema artifact supplies this same migration function.
        let key = match collection {
            "communication_accounts" => "account_key",
            "communication_threads" => "thread_key",
            _ => "message_key",
        };
        if body["id"].as_str().is_none_or(str::is_empty) {
            body["id"] = body[key].clone();
        }
        Ok(body)
    }
}
fn definition(name: &str, version: &str) -> schema::CollectionDefinition {
    schema::CollectionDefinition {
        name: name.into(),
        app_id: "conversations".into(),
        schema_ver: 1,
        schema_json: json!({"version":1,"primaryKey":"id","type":"object","properties":{"id":{"type":"string"}},"required":["id"]}),
        class: "full".into(),
        client_limit: 1000,
        byte_budget: 65536,
        rights_ver: 1,
        app_ver: version.into(),
    }
}
#[tokio::test]
async fn publication_replay_and_core_import_are_seq_neutral_and_never_overwrite_app_edits(
) -> Result<()> {
    let dir = tempfile::tempdir()?;
    let writer = Registry.open(
        DbId::new(dir.path().join("sync.sqlite3"))?,
        WriterOptions {
            configure: None,
            head_seq: Some(Arc::new(|c| {
                c.query_row("SELECT v FROM _seq", [], |r| r.get(0))
                    .map_err(Into::into)
            })),
        },
    )?;
    // Initialize before the first commit: head_seq is read inside every commit.
    writer.ready().await?;
    let def = definition("communication_accounts", "v4-base");
    let plan = ReleasePlan {
        version: "v4-base".into(),
        manifests: vec![],
        collections: BTreeMap::from([("conversations".into(), vec![def])]),
        migrations: BTreeMap::new(),
        removed_sync_settings: vec![],
        field_merge_core_rule: vec![],
    };
    base_release::publish(&writer, &plan, &ReleaseMigration, 1).await?;
    let doc = Document {
        revision: None,
        collection: "communication_accounts".into(),
        id: "a".into(),
        body: json!({"id":"a","account_key":"a","title":"before"}),
    };
    write_page(&writer, vec![doc.clone()]).await?;
    let first = writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            sql(crate::change_log::head(tx))
        }))?
        .await?;
    base_release::publish(&writer, &plan, &ReleaseMigration, 2).await?;
    let replay = write_page(&writer, vec![doc.clone()]).await?;
    assert_eq!(
        replay.collections["communication_accounts"].already_present,
        1
    );
    let same = writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            sql(crate::change_log::head(tx))
        }))?
        .await?;
    assert_eq!(first.head_seq, same.head_seq);
    writer.submit(WriteJob::new(JobOptions::default(),|tx| {
        sql(tx.execute("UPDATE c_communication_accounts SET data=json_set(data,'$.title','owner edit'),rev='2-owner' WHERE id='a'",[]))?; Ok(())
    }))?.await?;
    assert!(write_page(&writer, vec![doc])
        .await
        .unwrap_err()
        .to_string()
        .contains("import_target_conflict"));
    let body: String = writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            sql(tx.query_row(
                "SELECT data FROM c_communication_accounts WHERE id='a'",
                [],
                |r| r.get(0),
            ))
        }))?
        .await?;
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["title"],
        "owner edit"
    );
    writer.shutdown().await?;
    Ok(())
}

struct NoFiles;
impl core_import::ImportServices for NoFiles {
    fn member_conversation_id(&self, _: &str) -> i64 {
        42
    }
    fn script(&mut self, _: &str, _: &str, _: &str) -> Result<Option<(String, u64)>> {
        Err(invalid("unexpected script"))
    }
    fn artifact(&mut self, _: &str, _: Option<&str>) -> Result<Option<String>> {
        Err(invalid("unexpected artifact"))
    }
}
#[test]
fn source_snapshot_maps_bindings_tickets_witnesses_and_mail_never_reads_secrets() -> Result<()> {
    let mut conn = rusqlite::Connection::open_in_memory()?;
    // Frozen core schema excerpt, no fake storage implementation.
    conn.execute_batch("CREATE TABLE workjet_supervisor_bindings(project_id TEXT,owner_user_id TEXT,thread_id TEXT,created_at_ms INTEGER);
      INSERT INTO workjet_supervisor_bindings VALUES('project','owner','thread',12);
      CREATE TABLE ticket_items(ticket_key TEXT,title TEXT,metadata_json TEXT,created_at TEXT,updated_at TEXT);
      INSERT INTO ticket_items VALUES('local:t','title','{}','2026-10-15T00:00:00Z','2026-10-15T00:00:00Z');
      CREATE TABLE outbound_native_field_status_witnesses(record_id TEXT,scope TEXT,person_key TEXT,field TEXT,status_json TEXT);
      INSERT INTO outbound_native_field_status_witnesses VALUES('lead','person','p','email','{\"status\":\"no_match\"}');
      CREATE TABLE stalwart_domains(domain_name TEXT,dkim_selector TEXT,spf_record TEXT,dmarc_record TEXT,dkim_private_key TEXT);
      INSERT INTO stalwart_domains VALUES('example.test','selector','spf','dmarc','DO-NOT-READ');
      CREATE TABLE stalwart_users(username TEXT,password_hash TEXT,created_at INTEGER);
      INSERT INTO stalwart_users VALUES('owner','DO-NOT-READ',1);
      CREATE TABLE iot_rulesets(id TEXT,realm TEXT,name TEXT,enabled INTEGER,data TEXT,updated_at TEXT);
      INSERT INTO iot_rulesets VALUES('rule','realm','rule',1,'{}','2026-10-15T00:00:00Z');
      CREATE TABLE iot_condition_state(ruleset_id TEXT,condition_index INTEGER,asset_id TEXT,first_match_ms INTEGER,previously_matched INTEGER,previously_unmatched INTEGER);
      INSERT INTO iot_condition_state VALUES('rule',0,'asset',123,1,0),('rule',1,'asset',NULL,0,0);
      CREATE TABLE iot_recurrence_block(scope_key TEXT,last_fired_ms INTEGER);
      INSERT INTO iot_recurrence_block VALUES('rule:asset',456);")?;
    let tx = conn.transaction()?;
    let imported = core_import::collect(&tx, &mut NoFiles, 100, 100000)?;
    let binding = imported
        .documents
        .iter()
        .find(|d| d.collection == "workjet_supervisor_bindings")
        .unwrap();
    assert_eq!(
        binding.body,
        json!({"id":"project","project_id":"project","owner_user_id":"owner","thread_id":"thread","created_at_ms":12,"updated_at_ms":12,"is_deleted":false})
    );
    let ticket = imported
        .documents
        .iter()
        .find(|d| d.collection == "ctox_ticket_items")
        .unwrap();
    assert_eq!(ticket.body["id"], "local:t");
    assert_eq!(ticket.body["metadata"], json!({}));
    let witness = imported
        .documents
        .iter()
        .find(|d| d.collection.ends_with("field_status_witnesses"))
        .unwrap();
    assert_eq!(witness.body["witnesses"][0]["person_key"], "p");
    let rule = imported
        .documents
        .iter()
        .find(|d| d.collection == "iot_rulesets")
        .unwrap();
    assert_eq!(rule.body["match_state"]["asset"]["matched"], true);
    assert_eq!(rule.body["match_state"]["asset"]["windows"]["0"], 123);
    assert_eq!(rule.body["match_state"]["asset"]["fired_ms"], 456);
    let mail = core_import::mail_server(&tx)?;
    let bytes = serde_json::to_string(&mail).unwrap();
    assert!(!bytes.contains("DO-NOT-READ"));
    assert!(!bytes.contains("password"));
    assert!(!bytes.contains("private_key"));
    assert_eq!(
        mail.iter()
            .find(|d| d.id == "domain:example.test")
            .unwrap()
            .body["data"]["dkim_selector"],
        "selector"
    );
    Ok(())
}
