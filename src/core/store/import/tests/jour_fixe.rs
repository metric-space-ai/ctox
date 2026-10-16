use super::*;
use rusqlite::params;

fn source() -> Result<rusqlite::Connection> {
    let conn = rusqlite::Connection::open_in_memory()?;
    // Verbatim native schema; populated semantic fields use the wire contract.
    conn.execute_batch(
        "CREATE TABLE workjet_jour_fixe_meetings (
 meeting_id TEXT PRIMARY KEY, project_id TEXT NOT NULL, owner_user_id TEXT NOT NULL,
 scheduled_at_ms INTEGER NOT NULL, metadata_json TEXT NOT NULL, preparation_task_id TEXT
);",
    )?;
    for (id, state, revision) in [("meeting-a", "planned", 0), ("meeting-b", "confirmed", 7)] {
        let metadata = json!({"id":id,"project_id":"project","owner_user_id":"owner",
            "scheduled_at_ms":1791802800000i64,"prepare_at_ms":1791795600000i64,
            "supervisor":{"workjet_thread_id":"supervisor","ctox_thread_key":"thread-key"},
            "timezone":"Europe/Berlin","state":state,"revision":revision,"deck_revision":2,
            "previous_goal":{"goal_id":"goal","revision":3},
            "slides":[],"comments":[],"transcript":[],"todos":null,"error":null});
        conn.execute("INSERT INTO workjet_jour_fixe_meetings VALUES (?1,'project','owner',1791802800000,?2,NULL)", params![id,metadata.to_string()])?;
    }
    Ok(conn)
}

#[tokio::test]
async fn meeting_import_preserves_every_row_hash_and_replay_is_seq_neutral() -> Result<()> {
    let mut conn = source()?;
    let tx = conn.transaction()?;
    let snapshot = core_import::collect(&tx, &mut NoFiles, 100, 100000)?;
    let docs: Vec<_> = snapshot
        .documents
        .iter()
        .filter(|d| d.collection == super::super::jour_fixe::COLLECTION)
        .collect();
    let source_count: usize =
        tx.query_row("SELECT count(*) FROM workjet_jour_fixe_meetings", [], |r| {
            r.get(0)
        })?;
    assert_eq!(docs.len(), source_count);
    assert!(docs.iter().all(|d| d.body["owner_user_id"] == "owner"
        && d.body["previous_goal"] == json!({"goal_id":"goal","revision":3})
        && d.body["timezone"] == "Europe/Berlin"));
    let dir = tempfile::tempdir()?;
    let writer = Registry.open(
        DbId::new(dir.path().join("sync.sqlite3"))?,
        WriterOptions::default(),
    )?;
    writer.ready().await?;
    let mut def = definition(super::super::jour_fixe::COLLECTION, "v4-base");
    def.app_id = "ctox".into();
    // Frozen Shell-v4 schema, not a permissive fixture invented by the importer.
    def.schema_json =
        serde_json::from_str(include_str!("fixtures/jour-fixe-meeting.schema.json")).unwrap();
    def.schema_ver = def.schema_json["version"].as_u64().unwrap() as u32;
    let plan = ReleasePlan {
        version: "v4-base".into(),
        manifests: vec![],
        libs: vec![],
        collections: BTreeMap::from([("ctox".into(), vec![def])]),
        migrations: BTreeMap::new(),
        removed_sync_settings: vec![],
        field_merge_core_rule: vec![],
    };
    base_release::publish(&writer, &plan, &ReleaseMigration, 1).await?;
    let first =
        core_import::apply(&writer, snapshot, &mut profile::NoTimeseries, "jf-import").await?;
    let count = &first.collections[super::super::jour_fixe::COLLECTION];
    assert_eq!(
        (count.source_rows, count.target_documents, count.inserted),
        (source_count, source_count, source_count)
    );
    let hashes = &first.content_hashes[super::super::jour_fixe::COLLECTION];
    assert_eq!(hashes.len(), source_count);
    assert!(hashes
        .values()
        .all(|h| h.target_sha256.as_ref() == Some(&h.source_sha256)));
    let before = writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            sql(crate::change_log::head(tx))
        }))?
        .await?;
    let snapshot = core_import::collect(&tx, &mut NoFiles, 100, 100000)?;
    let replay =
        core_import::apply(&writer, snapshot, &mut profile::NoTimeseries, "jf-import").await?;
    let after = writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            sql(crate::change_log::head(tx))
        }))?
        .await?;
    assert_eq!(before.head_seq, after.head_seq);
    assert_eq!(
        replay.collections[super::super::jour_fixe::COLLECTION].already_present,
        source_count
    );
    assert_eq!(
        replay.collections[super::super::jour_fixe::COLLECTION].inserted,
        0
    );
    if let Ok(path) = std::env::var("JF_IMPORT_PROOF") {
        std::fs::write(path, serde_json::to_vec_pretty(&json!({"source_rows":source_count,"first":first,"replay":replay,"seq_before":before.head_seq.to_string(),"seq_after":after.head_seq.to_string()})).unwrap())?;
    }
    writer.shutdown().await?;
    Ok(())
}

#[test]
fn unsupported_content_and_extra_columns_fail_before_any_write() -> Result<()> {
    let mut conn = source()?;
    conn.execute(
        "UPDATE workjet_jour_fixe_meetings SET preparation_task_id='task'",
        [],
    )?;
    conn.execute("UPDATE workjet_jour_fixe_meetings SET metadata_json=json_set(metadata_json,'$.slides',json('[{\"id\":\"slide\"}]'))", [])?;
    let tx = conn.transaction()?;
    let error = core_import::collect(&tx, &mut NoFiles, 100, 100000)
        .err()
        .expect("schema gaps must stop import")
        .to_string();
    assert!(
        error.contains("preparation_task_id") && error.contains("metadata_json.slides"),
        "{error}"
    );
    tx.rollback()?;
    conn.execute_batch("ALTER TABLE workjet_jour_fixe_meetings ADD COLUMN future_column TEXT;")?;
    let tx = conn.transaction()?;
    let error = core_import::collect(&tx, &mut NoFiles, 100, 100000)
        .err()
        .expect("unmapped column must stop import")
        .to_string();
    assert!(error.contains("extra=future_column"), "{error}");
    Ok(())
}
