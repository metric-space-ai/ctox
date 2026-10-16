use super::super::file_import::{self, Budget, LegacyCollection, ProfilePaths, SnapshotFile};
use super::*;
use std::{fs, path::Path};
fn source(name: &str, app: &str) -> LegacyCollection {
    LegacyCollection {
        collection: name.into(),
        table: format!("legacy_{name}"),
        owner_app: app.into(),
        owner_user: Some("owner".into()),
    }
}
fn paths(root: &Path) -> ProfilePaths {
    ProfilePaths {
        original_root: "/original".into(),
        copied_root: root.into(),
    }
}
fn budget() -> Budget {
    Budget {
        rows: 100,
        bytes: 1024 * 1024,
    }
}
#[tokio::test]
#[ignore = "qualification: requires copied release inputs, run via scripts/sync-v4/qualify-import.sh"]
async fn copied_profile_files_tracking_and_counting_are_complete_and_replay_is_seq_neutral(
) -> Result<()> {
    let input = std::env::var("IMPORT_INPUT_ROOT").unwrap();
    // Start from the genuine allowlisted daemon copy, extend only this disposable
    // copy with synthetic file/tracking incidents (never customer bytes).
    let dir = tempfile::tempdir()?;
    let profile = dir.path().join("source.sqlite3");
    fs::copy(
        Path::new(&input).join("core-allowlisted-v2.sqlite3"),
        &profile,
    )?;
    let mut source_db = rusqlite::Connection::open(&profile)?;
    source_db.execute_batch("CREATE TABLE legacy_desktop_files(id TEXT,data TEXT,deleted INTEGER);
      INSERT INTO legacy_desktop_files VALUES('same-id','{\"id\":\"same-id\",\"name\":\"desktop.txt\",\"kind\":\"file\",\"local_path\":\"/original/desktop.txt\",\"owner_id\":\"owner\"}',0);
      INSERT INTO legacy_desktop_files VALUES('folder-id','{\"id\":\"folder-id\",\"name\":\"folder\",\"kind\":\"folder\"}',0);
      CREATE TABLE legacy_document_blob_chunks(id TEXT,data TEXT,deleted INTEGER);
      INSERT INTO legacy_document_blob_chunks VALUES('b1','{\"id\":\"b1\",\"blob_id\":\"blob\",\"idx\":1,\"total\":2,\"data\":\"bG8=\",\"encoding\":\"base64\"}',0),('b0','{\"id\":\"b0\",\"blob_id\":\"blob\",\"idx\":0,\"total\":2,\"data\":\"aGVs\",\"encoding\":\"base64\"}',0);
      CREATE TABLE stalwart_mail_tracking_tokens(token TEXT,message_id TEXT,campaign_id TEXT,event_type TEXT,target_url TEXT,created_at INTEGER);
      INSERT INTO stalwart_mail_tracking_tokens VALUES('token','message','campaign','opened',NULL,123);
      CREATE TABLE stalwart_mail_tracking_events(id INTEGER,token TEXT,message_id TEXT,event_type TEXT,occurred_at INTEGER,user_agent TEXT);
      INSERT INTO stalwart_mail_tracking_events VALUES(17,'token','message','opened',123456,'test');")?;
    fs::write(dir.path().join("desktop.txt"), b"hello")?;
    fs::create_dir(dir.path().join("snapshots"))?;
    fs::write(dir.path().join("snapshots/version.source"), b"old version")?;
    let tx = source_db.transaction()?;
    let snapshot = core_import::collect(&tx, &mut NoFiles, 1000, 1024 * 1024)?;
    assert_eq!(
        snapshot
            .documents
            .iter()
            .find(|d| d.collection == "outbound_tracking_tokens")
            .unwrap()
            .body["created_at_ms"],
        123000
    );
    let event = snapshot
        .documents
        .iter()
        .find(|d| d.collection == "outbound_tracking_events")
        .unwrap();
    assert_eq!(event.body["occurred_at_ms"], 123456);
    assert_eq!(event.body["campaign_id"], "campaign");
    let sources = [
        source("desktop_files", "desktop"),
        source("document_blob_chunks", "documents"),
    ];
    let snapshots = [SnapshotFile {
        relative_path: "snapshots/version.source".into(),
        owner_app: "ctox".into(),
        owner_user: Some("owner".into()),
        created_at_ms: 123,
    }];
    let plan = file_import::collect(&tx, &sources, &snapshots, &paths(dir.path()), &budget())?;
    tx.rollback()?;
    let cas = crate::cas::Cas::open(dir.path().join("cas"))?;
    let prepared = file_import::prepare(plan, &cas, dir.path()).await?;
    assert_eq!(prepared.source_rows["document_blob_chunks"], 2);
    assert_eq!(prepared.documents.len(), 4);
    assert_eq!(prepared.source_rows["desktop_files"], 2);
    let desktop = prepared
        .documents
        .iter()
        .find(|d| d.id == "same-id")
        .unwrap();
    let blob = prepared
        .documents
        .iter()
        .find(|d| d.body["name"] == "blob")
        .unwrap();
    assert_eq!(
        desktop.body["ref"],
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
    assert_eq!(blob.body["ref"], desktop.body["ref"]);
    assert_eq!(desktop.body["owner_user"], "owner");
    assert_eq!(blob.body["owner_user"], "owner");
    for doc in &prepared.documents {
        if doc.id == "folder-id" {
            assert_eq!(doc.body["mime"], "inode/directory");
            assert!(doc.body.get("ref").is_none());
        } else {
            cas.verified(
                doc.body["ref"].as_str().unwrap(),
                doc.body["size"].as_u64().unwrap(),
            )?;
        }
    }
    let defaults = schema::BaseReleaseDefaults {
        app_ver: "main@4794af03d+v4".into(),
        client_limit: 100000,
        byte_budget: 33554432,
        rights_ver: 1,
    };
    let mut release = base_release::load(
        &Path::new(&input).join("v4"),
        &Path::new(&input).join("ctox-platform.manifest.json"),
        &defaults,
    )?;
    // Existing frozen input predates OUT3: register the canonical target shapes
    // supplied by the current outbound schema, not a fabricated app collection.
    let outbound: Value = serde_json::from_slice(&fs::read(
        Path::new(&input).join("outbound-tracking.schema.json"),
    )?)
    .unwrap();
    for name in ["outbound_tracking_tokens", "outbound_tracking_events"] {
        let entry = outbound["collections"][name].clone();
        let schema = entry.get("schema").unwrap_or(&entry).clone();
        assert!(schema["properties"].is_object());
        let mut def = definition(name, "main@4794af03d+v4");
        def.app_id = "outbound".into();
        def.schema_ver = schema["version"]
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .expect("canonical schema version");
        def.schema_json = schema;
        release
            .collections
            .entry("outbound".into())
            .or_default()
            .retain(|d| d.name != name);
        release.collections.get_mut("outbound").unwrap().push(def);
    }
    let writer = Registry.open(
        DbId::new(dir.path().join("target.sqlite3"))?,
        WriterOptions::default(),
    )?;
    writer.ready().await?;
    base_release::publish(&writer, &release, &ReleaseMigration, 1).await?;
    let raw_source_rows = prepared.source_rows.clone();
    let snapshot = file_import::attach(snapshot, prepared)?;
    let expected_file_refs: BTreeMap<_, _> = snapshot
        .documents
        .iter()
        .filter(|d| d.collection == "files")
        .map(|d| (d.id.clone(), d.body["ref"].clone()))
        .collect();
    let report =
        core_import::apply(&writer, snapshot, &mut profile::NoTimeseries, "import2").await?;
    report.validate_complete()?;
    let expected = report.collections.clone();
    writer
        .submit(WriteJob::new(JobOptions::default(), move |tx| {
            for (name, c) in expected {
                let Some(active) = sql(schema::active_collection(tx, &name))? else {
                    assert_eq!(c.source_rows, 0, "unregistered nonempty source:{name}");
                    continue;
                };
                let table = sql(crate::change_log::identifier(&active.table))?;
                let actual: i64 = sql(tx.query_row(
                    &format!("SELECT count(*) FROM {table} WHERE deleted=0"),
                    [],
                    |r| r.get(0),
                ))?;
                assert_eq!(
                    c.source_rows, actual as usize,
                    "physical target mismatch:{name}"
                );
            }
            Ok(())
        }))?
        .await?;
    let before = writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            sql(crate::change_log::head(tx))
        }))?
        .await?;
    let tx = source_db.transaction()?;
    let replay_snapshot = core_import::collect(&tx, &mut NoFiles, 1000, 1024 * 1024)?;
    let replay_plan =
        file_import::collect(&tx, &sources, &snapshots, &paths(dir.path()), &budget())?;
    tx.rollback()?;
    let replay_files = file_import::prepare(replay_plan, &cas, dir.path()).await?;
    assert_eq!(
        expected_file_refs,
        replay_files
            .documents
            .iter()
            .map(|d| (d.id.clone(), d.body["ref"].clone()))
            .collect::<BTreeMap<_, _>>()
    );
    let replay_snapshot = file_import::attach(replay_snapshot, replay_files)?;
    let replay = core_import::apply(
        &writer,
        replay_snapshot,
        &mut profile::NoTimeseries,
        "import2",
    )
    .await?;
    let after = writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            sql(crate::change_log::head(tx))
        }))?
        .await?;
    assert_eq!(before.head_seq, after.head_seq);
    assert_eq!(replay.collections["files"].inserted, 0);
    let mut damaged = report.clone();
    damaged
        .collections
        .get_mut("files")
        .unwrap()
        .target_documents -= 1;
    assert!(damaged.validate_complete().is_err());
    let proof = std::env::var("IMPORT_PROOF_DIR").unwrap();
    fs::write(Path::new(&proof).join("import2-files-tracking.json"),serde_json::to_vec_pretty(&json!({"profile":"DevOps#810 copied daemon + isolated synthetic file/tracking incidents","raw_source_rows":raw_source_rows,"report":report,"replay":replay,"head_before":before.head_seq.to_string(),"head_after":after.head_seq.to_string(),"pass":true})).unwrap())?;
    writer.shutdown().await?;
    Ok(())
}
#[test]
fn incomplete_chunks_and_escaping_paths_never_reach_cas() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mut c = rusqlite::Connection::open_in_memory()?;
    c.execute_batch("CREATE TABLE legacy_document_blob_chunks(id TEXT,data TEXT,deleted INTEGER);INSERT INTO legacy_document_blob_chunks VALUES('c','{\"id\":\"c\",\"blob_id\":\"b\",\"idx\":0,\"total\":2,\"encoding\":\"base64\",\"data\":\"YQ==\"}',0);")?;
    let tx = c.transaction()?;
    assert!(file_import::collect(
        &tx,
        &[source("document_blob_chunks", "documents")],
        &[],
        &paths(dir.path()),
        &budget()
    )
    .err()
    .unwrap()
    .to_string()
    .contains("file_chunk_incomplete"));
    let snapshot = [SnapshotFile {
        relative_path: "../escape".into(),
        owner_app: "ctox".into(),
        owner_user: None,
        created_at_ms: 0,
    }];
    assert!(
        file_import::collect(&tx, &[], &snapshot, &paths(dir.path()), &budget())
            .err()
            .unwrap()
            .to_string()
            .contains("file_unsafe_path")
    );
    Ok(())
}
