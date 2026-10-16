use super::super::jour_fixe as meeting_import;
use super::*;
use rusqlite::params;
use std::{io::Read, path::Path};

fn source(root: &Path) -> Result<rusqlite::Connection> {
    let conn = rusqlite::Connection::open(root.join("business-os.sqlite3"))?;
    conn.execute_batch(
        "CREATE TABLE workjet_jour_fixe_meetings (
 meeting_id TEXT PRIMARY KEY, project_id TEXT NOT NULL, owner_user_id TEXT NOT NULL,
 scheduled_at_ms INTEGER NOT NULL, metadata_json TEXT NOT NULL, preparation_task_id TEXT);",
    )?;
    for (id, state, revision) in [("meeting-a", "planned", 0), ("meeting-b", "confirmed", 7)] {
        let large = id == "meeting-b";
        let n = if large { 20 } else { 1 };
        let text = if large {
            "belegter Inhalt ".repeat(1000)
        } else {
            "Belegter Inhalt".into()
        };
        let slides: Vec<_> = (0..n).map(|i| json!({"id":format!("slide-{i}"),"meeting_id":id,"position":i,"title":"Status","body_markdown":text})).collect();
        let comments: Vec<_> = (0..if large {100} else {1}).map(|i| json!({"id":format!("comment-{i}"),"meeting_id":id,"slide_id":"slide-0","deck_revision":2,"x":0.2,"y":0.3,"text":if large {"Kommentar ".repeat(350)} else {"Kommentar".into()},"author_user_id":"owner","created_at_ms":1})).collect();
        let transcript: Vec<_> = (0..n).map(|i| json!({"id":format!("segment-{i}"),"meeting_id":id,"sequence":i,"speaker":"owner","modality":"text","text":text,"started_at_ms":i,"ended_at_ms":i+1})).collect();
        let metadata = json!({"id":id,"project_id":"project","owner_user_id":"owner",
            "scheduled_at_ms":1791802800000i64,"prepare_at_ms":1791795600000i64,
            "supervisor":{"workjet_thread_id":"supervisor","ctox_thread_key":"thread-key"},
            "timezone":"Europe/Berlin","state":state,"revision":revision,"deck_revision":2,
            "previous_goal":{"goal_id":"goal","revision":3},
            "slides":slides,"comments":comments,"transcript":transcript,"todos":null,"error":null});
        let task = if large {
            Some("native-preparation-task")
        } else {
            None
        };
        conn.execute("INSERT INTO workjet_jour_fixe_meetings VALUES (?1,'project','owner',1791802800000,?2,?3)", params![id,metadata.to_string(),task])?;
    }
    Ok(conn)
}

#[tokio::test]
async fn meeting_import_preserves_every_row_hash_and_replay_is_seq_neutral() -> Result<()> {
    let dir = tempfile::tempdir()?;
    drop(source(dir.path())?);
    // The actual native producer stores meetings in BOS, not Core. Qualify that
    // exact file role, read-only, with no source transaction held during CAS IO.
    let mut conn = rusqlite::Connection::open_with_flags(
        dir.path().join("business-os.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let tx = conn.transaction()?;
    let snapshot = meeting_import::collect_bos(&tx, 100, 8 * 1024 * 1024)?;
    let source_count: usize =
        tx.query_row("SELECT count(*) FROM workjet_jour_fixe_meetings", [], |r| {
            r.get(0)
        })?;
    let source_metadata: BTreeMap<String, Value> = tx
        .prepare("SELECT meeting_id,metadata_json FROM workjet_jour_fixe_meetings")?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .map(|r| {
            let (id, raw) = r?;
            Ok((id, serde_json::from_str(&raw).unwrap()))
        })
        .collect::<rusqlite::Result<_>>()?;
    tx.rollback()?;
    let cas = crate::cas::Cas::open(dir.path().join("cas"))?;
    let snapshot = meeting_import::prepare_files(snapshot, &cas, dir.path()).await?;
    let docs: Vec<_> = snapshot
        .documents
        .iter()
        .filter(|d| d.collection == meeting_import::COLLECTION)
        .collect();
    assert_eq!(docs.len(), source_count);
    assert_eq!(
        docs.iter().find(|d| d.id == "meeting-b").unwrap().body["preparation_task_id"],
        "native-preparation-task"
    );
    let mut blob_evidence = Vec::new();
    for doc in &docs {
        for key in ["slides", "comments", "transcript"] {
            let value = &doc.body[key];
            let original = &source_metadata[&doc.id][key];
            if doc.id == "meeting-a" {
                assert_eq!(value, original);
                continue;
            }
            assert!(value["size"].as_u64().unwrap() > meeting_import::INLINE_LIMIT as u64);
            assert_eq!(value["blob_ref"], value["sha256"]);
            let blob = cas.verified(
                value["blob_ref"].as_str().unwrap(),
                value["size"].as_u64().unwrap(),
            )?;
            let mut bytes = Vec::new();
            cas.reader(&blob)?.read_to_end(&mut bytes)?;
            assert_eq!(
                serde_json::from_slice::<Value>(&bytes).unwrap(),
                *original,
                "lost content: {key}"
            );
            assert!(snapshot.documents.iter().any(|d| d.collection == "files"
                && d.body["ref"] == value["blob_ref"]
                && d.body["owner_app"] == "ctox"
                && d.body["owner_user"] == "owner"));
            blob_evidence.push(json!({"meeting":doc.id,"field":key,"size":bytes.len(),"sha256":value["sha256"],"source_equals_cas":true}));
        }
    }
    assert_eq!(blob_evidence.len(), 3);
    let writer = Registry.open(
        DbId::new(dir.path().join("sync.sqlite3"))?,
        WriterOptions::default(),
    )?;
    writer.ready().await?;
    let mut def = definition(meeting_import::COLLECTION, "v4-base");
    def.app_id = "ctox".into();
    def.schema_json =
        serde_json::from_str(include_str!("fixtures/jour-fixe-meeting.schema.json")).unwrap();
    assert_eq!(def.schema_json["version"], 1);
    def.schema_ver = 1;
    let mut files = definition("files", "v4-base");
    files.app_id = "ctox-platform".into();
    let plan = ReleasePlan {
        version: "v4-base".into(),
        manifests: vec![],
        libs: vec![],
        collections: BTreeMap::from([
            ("ctox".into(), vec![def]),
            ("ctox-platform".into(), vec![files]),
        ]),
        migrations: BTreeMap::new(),
        removed_sync_settings: vec![],
        field_merge_core_rule: vec![],
    };
    base_release::publish(&writer, &plan, &ReleaseMigration, 1).await?;
    let first =
        core_import::apply(&writer, snapshot, &mut profile::NoTimeseries, "jf-import").await?;
    let count = &first.collections[meeting_import::COLLECTION];
    assert_eq!(
        (count.source_rows, count.target_documents, count.inserted),
        (source_count, source_count, source_count)
    );
    let hashes = &first.content_hashes[meeting_import::COLLECTION];
    assert_eq!(hashes.len(), source_count);
    assert!(hashes
        .values()
        .all(|h| h.target_sha256.as_ref() == Some(&h.source_sha256)));
    let target_rows: usize = writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            let a = sql(schema::active_collection(tx, meeting_import::COLLECTION))?
                .ok_or_else(|| invalid("meeting missing"))?;
            let t = sql(crate::change_log::identifier(&a.table))?;
            sql(tx.query_row(
                &format!("SELECT count(*) FROM {t} WHERE deleted=0"),
                [],
                |r| r.get(0),
            ))
        }))?
        .await?;
    assert_eq!(target_rows, source_count);
    let before = writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            sql(crate::change_log::head(tx))
        }))?
        .await?;
    let tx = conn.transaction()?;
    let snapshot = meeting_import::collect_bos(&tx, 100, 8 * 1024 * 1024)?;
    tx.rollback()?;
    let snapshot = meeting_import::prepare_files(snapshot, &cas, dir.path()).await?;
    let replay =
        core_import::apply(&writer, snapshot, &mut profile::NoTimeseries, "jf-import").await?;
    let after = writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            sql(crate::change_log::head(tx))
        }))?
        .await?;
    assert_eq!(before.head_seq, after.head_seq);
    assert_eq!(
        replay.collections[meeting_import::COLLECTION].already_present,
        source_count
    );
    assert_eq!(replay.collections["files"].inserted, 0);
    if let Ok(path) = std::env::var("JF_IMPORT_PROOF") {
        std::fs::write(path,serde_json::to_vec_pretty(&json!({"source_database":"business-os.sqlite3","source_rows":source_count,"target_rows":target_rows,"first":first,"replay":replay,"blobs":blob_evidence,"seq_before":before.head_seq.to_string(),"seq_after":after.head_seq.to_string()})).unwrap())?;
    }
    writer.shutdown().await?;
    Ok(())
}

#[test]
fn unsupported_content_and_extra_columns_fail_before_any_write() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mut conn = source(dir.path())?;
    conn.execute_batch("ALTER TABLE workjet_jour_fixe_meetings ADD COLUMN future_column TEXT;")?;
    let tx = conn.transaction()?;
    let error = meeting_import::collect_bos(&tx, 100, 8 * 1024 * 1024)
        .err()
        .expect("unmapped column must stop import")
        .to_string();
    assert!(error.contains("extra=future_column"), "{error}");
    tx.rollback()?;
    let mut wrong = rusqlite::Connection::open(dir.path().join("ctox.sqlite3"))?;
    let tx = wrong.transaction()?;
    assert!(meeting_import::collect_bos(&tx, 100, 100000)
        .err()
        .expect("Core must not be mistaken for BOS")
        .to_string()
        .contains("bos_source_table_missing"));
    Ok(())
}
