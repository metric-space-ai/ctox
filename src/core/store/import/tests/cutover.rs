use super::super::cutover as c;
use super::*;
use rusqlite::{Connection, OpenFlags};
use std::{io::Read, path::Path};

fn proof(name: &str, value: Value) -> Result<()> {
    if let Some(dir) = std::env::var_os("E29_PROOF_DIR") {
        let path = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&path)?;
        std::fs::write(path.join(format!("{name}.json")), value.to_string())?;
    }
    Ok(())
}
fn plan() -> ReleasePlan {
    let mut normal = definition("known", "v4-base");
    normal.app_id = "ctox".into();
    let mut archive = definition(c::ARCHIVE, "v4-base");
    archive.app_id = "ctox".into();
    archive.schema_ver = 0;
    archive.schema_json =
        serde_json::from_str(include_str!("fixtures/legacy-records.schema.json")).unwrap();
    let mut files = definition("files", "v4-base");
    files.app_id = "ctox".into();
    ReleasePlan {
        version: "v4-base".into(),
        manifests: vec![],
        libs: vec![],
        collections: BTreeMap::from([("ctox".into(), vec![normal, archive, files])]),
        migrations: BTreeMap::new(),
        removed_sync_settings: vec![],
        field_merge_core_rule: vec![],
    }
}
fn seed(root: &Path, name: &str, body: Value) -> Result<c::FileChoice> {
    let path = root.join(name);
    let db = Connection::open(&path)?;
    db.execute_batch("CREATE TABLE records(id TEXT PRIMARY KEY,data TEXT,deleted INTEGER,revision TEXT);CREATE TABLE internal(data TEXT,deleted INTEGER);")?;
    db.execute(
        "INSERT INTO records VALUES('r',?1,0,'1-source')",
        [body.to_string()],
    )?;
    db.execute("INSERT INTO internal VALUES(?1,0)",[json!({"context":"collection","data":{"name":"known","version":1,"schema":{"version":1}}}).to_string()])?;
    drop(db);
    Ok(c::FileChoice {
        database: name.into(),
        live_file: Path::new("/original").join(name),
        frozen_file: path,
        lsof_receipt_sha256: "a".repeat(64),
    })
}
fn ro(f: &c::FileChoice) -> Result<Connection> {
    Ok(Connection::open_with_flags(
        &f.frozen_file,
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?)
}
fn spec(collection: &str, version: u32, entity: c::Entity, mode: c::Mode) -> c::Table {
    c::Table {
        name: "records".into(),
        collection: collection.into(),
        version,
        id_column: "id".into(),
        body_column: Some("data".into()),
        entity,
        mode,
    }
}
fn budget() -> c::Budget {
    c::Budget {
        rows: 100,
        bytes: 4 * 1024 * 1024,
    }
}
fn input<'a, 'b>(
    tx: &'a Transaction<'b>,
    file: &'a c::FileChoice,
    kind: c::SourceKind,
    tables: &'a [c::Table],
) -> c::FrozenInput<'a, 'b> {
    c::FrozenInput {
        tx,
        file,
        copied_from: &file.live_file,
        kind,
        internal_table: if kind == c::SourceKind::Rxdb {
            Some("internal")
        } else {
            None
        },
        tables,
    }
}
struct NoFrame;
impl core_import::DataframeImporter for NoFrame {
    fn import_timeseries(&mut self, _: &str, _: &[Value]) -> Result<usize> {
        Err(invalid("unexpected_dataframe"))
    }
}
async fn writer(root: &Path) -> Result<crate::writer::WriterHandle> {
    let w = Registry.open(
        DbId::new(root.join("sync.sqlite3"))?,
        WriterOptions::default(),
    )?;
    w.ready().await?;
    base_release::publish(&w, &plan(), &ReleaseMigration, 1).await?;
    Ok(w)
}
async fn stored(w: &crate::writer::WriterHandle, collection: &str) -> Result<Vec<Value>> {
    let name = collection.to_owned();
    Ok(w.submit(WriteJob::new(JobOptions::default(), move |tx| {
        let a = sql(schema::active_collection(tx, &name))?.unwrap();
        let table = sql(crate::change_log::identifier(&a.table))?;
        let mut s = sql(tx.prepare(&format!(
            "SELECT data FROM {table} WHERE deleted=0 ORDER BY id"
        )))?;
        let rows = sql(s.query_map([], |r| r.get::<_, String>(0)))?;
        rows.map(|r| serde_json::from_str(&sql(r)?).map_err(|_| invalid("target_json")))
            .collect()
    }))?
    .await?)
}

#[tokio::test]
async fn rule1_declared_app_rows_reach_the_manifest_collection_unchanged() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let original = json!({"id":"r","title":"real app record","nested":{"b":2,"a":1}});
    let f = seed(dir.path(), "business-os-rxdb.sqlite3", original.clone())?;
    let mut db = ro(&f)?;
    let tx = db.transaction()?;
    let tables = [spec("known", 1, c::Entity::App, c::Mode::Rows)];
    let result = c::collect(
        &[input(&tx, &f, c::SourceKind::Rxdb, &tables)],
        &plan(),
        &ReleaseMigration,
        &budget(),
        7,
    )?;
    assert_eq!(
        result.audit.source_rows["business-os-rxdb.sqlite3:records"],
        1
    );
    tx.rollback()?;
    let w = writer(dir.path()).await?;
    let report = core_import::apply(&w, result.snapshot, &mut NoFrame, "freeze").await?;
    report.validate_complete()?;
    assert_eq!(stored(&w, "known").await?, vec![original]);
    assert!(stored(&w, c::ARCHIVE).await?.is_empty());
    let rev = w
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            let a = sql(schema::active_collection(tx, "known"))?.unwrap();
            let t = sql(crate::change_log::identifier(&a.table))?;
            sql(
                tx.query_row(&format!("SELECT rev FROM {t} WHERE id='r'"), [], |r| {
                    r.get::<_, String>(0)
                }),
            )
        }))?
        .await?;
    assert_eq!(rev, "1-source");
    proof(
        "rule1",
        json!({"report":report,"revision":"1-source","actual_target_payload_equal":true}),
    )?;
    w.shutdown().await?;
    Ok(())
}
#[tokio::test]
async fn rule2_unknown_app_preserves_full_sql_row_in_verified_archive_blob_and_replays(
) -> Result<()> {
    let dir = tempfile::tempdir()?;
    let f = seed(
        dir.path(),
        "business-os-rxdb.sqlite3",
        json!({"id":"r","payload":"x".repeat(300000)}),
    )?;
    let mut db = ro(&f)?;
    let tx = db.transaction()?;
    let tables = [spec("customer-app", 9, c::Entity::App, c::Mode::Rows)];
    let result = c::collect(
        &[input(&tx, &f, c::SourceKind::Rxdb, &tables)],
        &plan(),
        &ReleaseMigration,
        &budget(),
        7,
    )?;
    tx.rollback()?;
    let audit = serde_json::to_value(&result.audit).unwrap();
    let original = result.snapshot.documents[0].body["doc"].clone();
    assert_eq!(original["revision"], "1-source");
    assert!(original["data"].as_str().unwrap().contains("payload"));
    let cas = crate::cas::Cas::open(dir.path().join("cas"))?;
    let snapshot = c::prepare_files(result.snapshot, &cas, dir.path(), "freeze-owner").await?;
    let archive = snapshot
        .documents
        .iter()
        .find(|d| d.collection == c::ARCHIVE)
        .unwrap();
    assert_eq!(archive.body["doc_storage"], "blob");
    let blob = cas.verified(
        archive.body["doc"]["blob_ref"].as_str().unwrap(),
        archive.body["doc"]["size"].as_u64().unwrap(),
    )?;
    let mut bytes = Vec::new();
    cas.reader(&blob)?.read_to_end(&mut bytes)?;
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), original);
    assert!(snapshot
        .documents
        .iter()
        .filter(|d| d.collection == "files")
        .all(|d| d.body["owner_user"] == "freeze-owner"));
    let expected = archive.body.clone();
    let w = writer(dir.path()).await?;
    let first = core_import::apply(&w, snapshot, &mut NoFrame, "freeze").await?;
    first.validate_complete()?;
    let before = w
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            sql(crate::change_log::head(tx))
        }))?
        .await?;
    let tx = db.transaction()?;
    let result = c::collect(
        &[input(&tx, &f, c::SourceKind::Rxdb, &tables)],
        &plan(),
        &ReleaseMigration,
        &budget(),
        7,
    )?;
    tx.rollback()?;
    let snapshot = c::prepare_files(result.snapshot, &cas, dir.path(), "freeze-owner").await?;
    let again = core_import::apply(&w, snapshot, &mut NoFrame, "freeze").await?;
    assert_eq!(again.collections[c::ARCHIVE].inserted, 0);
    let after = w
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            sql(crate::change_log::head(tx))
        }))?
        .await?;
    assert_eq!(before.head_seq, after.head_seq);
    assert_eq!(stored(&w, c::ARCHIVE).await?, vec![expected]);
    proof(
        "rule2",
        json!({"audit":audit,"first":first,"replay":again,"sequence_before":before.head_seq,"sequence_after":after.head_seq,"source_equals_verified_blob":true}),
    )?;
    w.shutdown().await?;
    Ok(())
}
#[tokio::test]
async fn rule3_app_rxdb_and_core_entity_authorities_win_mirrors_without_overwrite() -> Result<()> {
    for entity in [c::Entity::App, c::Entity::Core] {
        let dir = tempfile::tempdir()?;
        let rx = seed(
            dir.path(),
            "business-os-rxdb.sqlite3",
            json!({"id":"r","origin":"rxdb"}),
        )?;
        let core = seed(
            dir.path(),
            "ctox.sqlite3",
            json!({"id":"r","origin":"core"}),
        )?;
        for source in [&rx, &core] {
            let db = Connection::open(&source.frozen_file)?;
            db.execute(
                "INSERT INTO records VALUES('eq',?1,0,'1-equal')",
                [json!({"id":"eq","origin":"equal"}).to_string()],
            )?;
        }
        let bos = seed(dir.path(), "business-os.sqlite3", json!({"id":"unused"}))?;
        let db = Connection::open(&bos.frozen_file)?;
        db.execute_batch("CREATE TABLE business_records(collection TEXT,record_id TEXT,rev TEXT,deleted INTEGER,updated_at_ms INTEGER,payload_json TEXT,PRIMARY KEY(collection,record_id));")?;
        for collection in ["known", "unknown"] {
            db.execute(
                "INSERT INTO business_records VALUES(?1,'r','2-mirror',0,3,?2)",
                params![
                    collection,
                    json!({"id":"r","origin":"bos-mirror"}).to_string()
                ],
            )?;
        }
        drop(db);
        let mut a = ro(&rx)?;
        let mut b = ro(&core)?;
        let mut d = ro(&bos)?;
        let ta = a.transaction()?;
        let tb = b.transaction()?;
        let td = d.transaction()?;
        let tables = [spec("known", 1, entity, c::Mode::Rows)];
        let mirrors = [c::business_records_table()];
        let result = c::collect(
            &[
                input(&td, &bos, c::SourceKind::Core, &mirrors),
                input(&tb, &core, c::SourceKind::Core, &tables),
                input(&ta, &rx, c::SourceKind::Rxdb, &tables),
            ],
            &plan(),
            &ReleaseMigration,
            &budget(),
            7,
        )?;
        assert_eq!(result.audit.findings.len(), 3);
        assert!(!result.audit.compared_equal.is_empty());
        let finding = result
            .audit
            .findings
            .iter()
            .find(|f| f.source_table == "records")
            .unwrap();
        assert!(finding.authority_sha256.is_some());
        assert!(result
            .audit
            .findings
            .iter()
            .any(|f| f.collection == "unknown" && f.authority_sha256.is_none()));
        let id = finding.archive_id.clone();
        let audit = serde_json::to_value(&result.audit).unwrap();
        ta.rollback()?;
        tb.rollback()?;
        td.rollback()?;
        let w = writer(dir.path()).await?;
        core_import::apply(&w, result.snapshot, &mut NoFrame, "freeze").await?;
        let expected = match entity {
            c::Entity::App => "rxdb",
            c::Entity::Core => "core",
        };
        let actual = stored(&w, "known").await?;
        assert_eq!(
            actual.iter().find(|d| d["id"] == "r").unwrap()["origin"],
            expected
        );
        assert_eq!(
            actual.iter().find(|d| d["id"] == "eq").unwrap()["origin"],
            "equal"
        );
        let archived = stored(&w, c::ARCHIVE).await?;
        let original = archived.iter().find(|v| v["id"] == id).unwrap();
        assert!(original["doc"]["data"]
            .as_str()
            .unwrap()
            .contains(if expected == "rxdb" { "core" } else { "rxdb" }));
        assert_eq!(
            archived
                .iter()
                .filter(|v| v["source_table"] == "business_records")
                .count(),
            2
        );
        proof(
            if expected == "rxdb" {
                "rule3-app"
            } else {
                "rule3-core"
            },
            json!({"audit":audit,"authority":expected,"actual_target":actual,"archives":archived}),
        )?;
        w.shutdown().await?;
    }
    Ok(())
}
#[test]
fn rule4_metadata_selects_active_version_and_archives_the_higher_old_version() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let f = seed(
        dir.path(),
        "business-os-rxdb.sqlite3",
        json!({"id":"r","version":"active-low"}),
    )?;
    let db = Connection::open(&f.frozen_file)?;
    db.execute_batch("CREATE TABLE ctox_business_os__known__v1 AS SELECT * FROM records;CREATE TABLE ctox_business_os__known__v9 AS SELECT * FROM records;UPDATE ctox_business_os__known__v9 SET data='{\"id\":\"r\",\"version\":\"historical-high\"}';")?;
    db.execute("INSERT INTO internal VALUES(?1,1)",[json!({"context":"collection","data":{"name":"known","version":9,"schema":{"version":9}},"_deleted":true}).to_string()])?;
    drop(db);
    let mut db = ro(&f)?;
    let tx = db.transaction()?;
    let versions = c::active_versions(&tx, "internal")?;
    assert_eq!(versions["known"], 1);
    let tables = c::rxdb_tables(&tx, &std::collections::BTreeSet::new())?;
    assert_eq!(tables.len(), 2);
    let result = c::collect(
        &[input(&tx, &f, c::SourceKind::Rxdb, &tables)],
        &plan(),
        &ReleaseMigration,
        &budget(),
        7,
    )?;
    assert_eq!(
        result
            .snapshot
            .documents
            .iter()
            .find(|d| d.collection == "known")
            .unwrap()
            .body["version"],
        "active-low"
    );
    let doc = &result
        .snapshot
        .documents
        .iter()
        .find(|d| d.collection == c::ARCHIVE)
        .unwrap()
        .body;
    assert_eq!(doc["collection_version"], 9);
    assert!(doc["doc"]["data"]
        .as_str()
        .unwrap()
        .contains("historical-high"));
    proof(
        "rule4",
        json!({"active_metadata":versions,"documents":result.snapshot.documents,"audit":result.audit}),
    )?;
    let core = seed(
        dir.path(),
        "ctox.sqlite3",
        json!({"id":"r","version":"historical-high"}),
    )?;
    let mut core_db = ro(&core)?;
    let core_tx = core_db.transaction()?;
    let mirrors = c::rxdb_tables(&tx, &std::collections::BTreeSet::from(["known".into()]))?;
    let native = [spec("known", 0, c::Entity::Core, c::Mode::Rows)];
    let result = c::collect(
        &[
            input(&tx, &f, c::SourceKind::Rxdb, &mirrors),
            input(&core_tx, &core, c::SourceKind::Core, &native),
        ],
        &plan(),
        &ReleaseMigration,
        &budget(),
        7,
    )?;
    assert!(result
        .snapshot
        .documents
        .iter()
        .any(|d| d.collection == c::ARCHIVE
            && d.body["source_db"] == "business-os-rxdb.sqlite3"
            && d.body["collection_version"] == 9));
    assert_eq!(
        result
            .snapshot
            .documents
            .iter()
            .find(|d| d.collection == "known")
            .unwrap()
            .body["version"],
        "historical-high"
    );
    proof(
        "rule4-core-mirror",
        json!({"documents":result.snapshot.documents,"audit":result.audit}),
    )?;
    core_tx.rollback()?;
    tx.rollback()?;
    let db = Connection::open(&f.frozen_file)?;
    db.execute("INSERT INTO internal VALUES(?1,0)",[json!({"context":"collection","data":{"name":"known","version":9,"schema":{"version":9}}}).to_string()])?;
    drop(db);
    let mut db = ro(&f)?;
    let tx = db.transaction()?;
    assert!(c::active_versions(&tx, "internal")
        .unwrap_err()
        .to_string()
        .contains("cutover_ambiguous_active_version"));
    Ok(())
}
#[test]
fn rule5_partial_projection_archives_all_columns_including_null_and_blob() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let f = seed(dir.path(), "ctox.sqlite3", json!({"id":"r"}))?;
    let db = Connection::open(&f.frozen_file)?;
    db.execute_batch("ALTER TABLE records ADD COLUMN retained_extra BLOB;ALTER TABLE records ADD COLUMN retained_null TEXT;ALTER TABLE records ADD COLUMN retained_real REAL;ALTER TABLE records ADD COLUMN retained_text TEXT;UPDATE records SET retained_text=CAST(x'ff' AS TEXT),retained_real=9e999,retained_extra=x'00ff';")?;
    drop(db);
    let mut db = ro(&f)?;
    let tx = db.transaction()?;
    let tables = [c::Table {
        body_column: None,
        ..spec(
            "known",
            0,
            c::Entity::Core,
            c::Mode::Projection {
                columns: vec!["id".into()],
            },
        )
    }];
    let result = c::collect(
        &[input(&tx, &f, c::SourceKind::Core, &tables)],
        &plan(),
        &ReleaseMigration,
        &budget(),
        7,
    )?;
    assert_eq!(
        result
            .snapshot
            .documents
            .iter()
            .find(|d| d.collection == "known")
            .unwrap()
            .body,
        json!({"id":"r"})
    );
    let full = &result
        .snapshot
        .documents
        .iter()
        .find(|d| d.collection == c::ARCHIVE)
        .unwrap()
        .body["doc"];
    assert_eq!(full["retained_extra"], json!({"sqlite_blob_hex":"00ff"}));
    assert!(full.get("retained_null").unwrap().is_null());
    assert_eq!(full["revision"], "1-source");
    assert_eq!(
        full["retained_real"],
        json!({"sqlite_nonfinite_real":"inf"})
    );
    assert_eq!(full["retained_text"], json!({"sqlite_text_hex":"ff"}));
    proof(
        "rule5",
        json!({"audit":result.audit,"documents":result.snapshot.documents}),
    )?;
    let account = seed(
        dir.path(),
        "accounts.sqlite3",
        json!({"account_key":"account-natural-key","title":"kept"}),
    )?;
    let a = Connection::open(&account.frozen_file)?;
    a.execute("UPDATE internal SET data=?1",[json!({"context":"collection","data":{"name":"communication_accounts","version":1,"schema":{"version":1}}}).to_string()])?;
    drop(a);
    let mut a = ro(&account)?;
    let at = a.transaction()?;
    let tables = [spec(
        "communication_accounts",
        1,
        c::Entity::App,
        c::Mode::Rows,
    )];
    let mut release = plan();
    release.collections.insert(
        "conversations".into(),
        vec![definition("communication_accounts", "v4-base")],
    );
    let result = c::collect(
        &[input(&at, &account, c::SourceKind::Rxdb, &tables)],
        &release,
        &ReleaseMigration,
        &budget(),
        7,
    )?;
    let doc = result
        .snapshot
        .documents
        .iter()
        .find(|d| d.collection == "communication_accounts")
        .unwrap();
    assert_eq!(doc.id, "account-natural-key");
    let full = &result
        .snapshot
        .documents
        .iter()
        .find(|d| d.collection == c::ARCHIVE)
        .unwrap()
        .body;
    assert_eq!(full["record_id"], "r");
    assert!(full["doc"]["data"]
        .as_str()
        .unwrap()
        .contains("account-natural-key"));
    proof(
        "rule5-migration",
        json!({"audit":result.audit,"documents":result.snapshot.documents}),
    )?;
    Ok(())
}
#[test]
fn rule6_core_without_target_is_retained_without_read_or_write() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let f = seed(
        dir.path(),
        "ctox.sqlite3",
        json!({"id":"r","opaque":"retained"}),
    )?;
    let mut db = ro(&f)?;
    let tx = db.transaction()?;
    let tables = [spec("no-target", 0, c::Entity::Core, c::Mode::KeepCore)];
    let before: String = tx.query_row("SELECT data FROM records", [], |r| r.get(0))?;
    let result = c::collect(
        &[input(&tx, &f, c::SourceKind::Core, &tables)],
        &plan(),
        &ReleaseMigration,
        &c::Budget { rows: 0, bytes: 0 },
        7,
    )?;
    assert!(result.snapshot.documents.is_empty());
    assert!(result.audit.source_rows.is_empty());
    assert_eq!(
        result.audit.retained_core_tables,
        vec!["ctox.sqlite3:records"]
    );
    let after: String = tx.query_row("SELECT data FROM records", [], |r| r.get(0))?;
    assert_eq!(before, after);
    proof(
        "rule6",
        json!({"audit":result.audit,"unchanged_source":true}),
    )?;
    Ok(())
}
#[test]
fn rule7_freeze_receipt_selects_the_source_file_never_path_heuristics() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mut f = seed(dir.path(), "ctox.sqlite3", json!({"id":"r"}))?;
    let mut db = ro(&f)?;
    let tx = db.transaction()?;
    let tables = [spec("no-target", 0, c::Entity::Core, c::Mode::KeepCore)];
    let mut extra = serde_json::to_value(&f).unwrap();
    extra["auto_select_latest"] = true.into();
    assert!(serde_json::from_value::<c::FileChoice>(extra).is_err());
    let mut wrong = input(&tx, &f, c::SourceKind::Core, &tables);
    wrong.copied_from = Path::new("/original/runtime/ctox.sqlite3");
    assert!(
        c::collect(&[wrong], &plan(), &ReleaseMigration, &budget(), 7)
            .err()
            .unwrap()
            .to_string()
            .contains("cutover_source_choice_required")
    );
    tx.rollback()?;
    f.lsof_receipt_sha256.clear();
    let tx = db.transaction()?;
    assert!(c::collect(
        &[input(&tx, &f, c::SourceKind::Core, &tables)],
        &plan(),
        &ReleaseMigration,
        &budget(),
        7
    )
    .is_err());
    proof(
        "rule7",
        json!({"wrong_live_source_rejected":true,"missing_custody_rejected":true}),
    )?;
    Ok(())
}

const OWNER: &str = "00000000-0000-4000-8000-000000000001";
fn seed_aliases(conn: &Connection) -> Result<()> {
    sql(conn.execute_batch("CREATE TABLE business_users(user_id TEXT PRIMARY KEY,active INTEGER);CREATE TABLE business_user_identity_aliases(alias TEXT PRIMARY KEY,canonical_user_id TEXT,verified_at_ms INTEGER);"))?;
    sql(conn.execute("INSERT INTO business_users VALUES(?1,1)", [OWNER]))?;
    sql(conn.execute(
        "INSERT INTO business_user_identity_aliases VALUES('owner@example.test',?1,1)",
        [OWNER],
    ))?;
    Ok(())
}
#[tokio::test]
async fn e34_cutover_alias_retains_original_hash_count_and_replay() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let f = seed(
        dir.path(),
        "sync.sqlite3",
        json!({"id":"r","owner_user_id":"owner@example.test","untouched":7}),
    )?;
    let bos = dir.path().join("bos.sqlite3");
    let b = Connection::open(&bos)?;
    seed_aliases(&b)?;
    drop(b);
    let identity = c::FileChoice {
        database: "business-os.sqlite3".into(),
        live_file: "/original/bos.sqlite3".into(),
        frozen_file: bos,
        lsof_receipt_sha256: "b".repeat(64),
    };
    let mut b = ro(&identity)?;
    let btx = b.transaction()?;
    let conn = Connection::open(&f.frozen_file)?;
    conn.execute("UPDATE internal SET data=?1",[json!({"context":"collection","data":{"name":"workjet_computers","version":1,"schema":{"version":1}}}).to_string()])?;
    drop(conn);
    let mut db = ro(&f)?;
    let tx = db.transaction()?;
    let mut p = plan();
    let mut definition = definition("workjet_computers", "v4-base");
    definition.app_id = "ctox".into();
    p.collections.get_mut("ctox").unwrap().push(definition);
    let tables = [spec("workjet_computers", 1, c::Entity::App, c::Mode::Rows)];
    let out = c::collect(
        &[
            input(&tx, &f, c::SourceKind::Rxdb, &tables),
            input(&btx, &identity, c::SourceKind::Core, &[]),
        ],
        &p,
        &ReleaseMigration,
        &budget(),
        7,
    )?;
    assert_eq!(out.audit.source_rows["sync.sqlite3:records"], 1);
    assert_eq!(out.audit.source_tables["sync.sqlite3:records"].row_count, 1);
    let original_hash = out.audit.source_row_hashes["sync.sqlite3:records"]
        .values()
        .next()
        .unwrap()
        .clone();
    let doc = out
        .snapshot
        .documents
        .iter()
        .find(|d| d.collection == "workjet_computers")
        .unwrap();
    assert_eq!(doc.body["owner_user_id"], OWNER);
    assert_eq!(doc.body["untouched"], 7);
    let archive = out
        .snapshot
        .documents
        .iter()
        .find(|d| d.collection == c::ARCHIVE)
        .unwrap();
    assert_eq!(archive.body["sha256"], original_hash);
    let w = Registry.open(
        DbId::new(dir.path().join("target.sqlite3"))?,
        WriterOptions::default(),
    )?;
    w.ready().await?;
    base_release::publish(&w, &p, &ReleaseMigration, 1).await?;
    let documents = out.snapshot.documents.clone();
    let initial_report = out.snapshot.report.clone();
    let first = core_import::apply(&w, out.snapshot, &mut NoFrame, "e34").await?;
    first.validate_complete()?;
    let before = w
        .submit(WriteJob::new(JobOptions::default(), |t| {
            sql(t.query_row("SELECT MAX(seq) FROM _changes", [], |r| r.get::<_, u64>(0)))
        }))?
        .await?;
    let replay = core_import::CoreImport {
        documents,
        dataframe_rows: vec![],
        report: initial_report,
    };
    let second = core_import::apply(&w, replay, &mut NoFrame, "e34").await?;
    second.validate_complete()?;
    let after = w
        .submit(WriteJob::new(JobOptions::default(), |t| {
            sql(t.query_row("SELECT MAX(seq) FROM _changes", [], |r| r.get::<_, u64>(0)))
        }))?
        .await?;
    assert_eq!(before, after);
    proof(
        "e34-cutover",
        json!({"report":first,"source_tables":out.audit.source_tables,"before_seq":before,"after_seq":after,"original_row_preserved":true}),
    )?;
    w.shutdown().await
}
#[tokio::test]
async fn e34_startup_is_marker_guarded_and_revocation_safe() -> Result<()> {
    use super::super::owner_alias as a;
    let dir = tempfile::tempdir()?;
    let bos = Registry.open(
        DbId::new(dir.path().join("bos.sqlite3"))?,
        WriterOptions::default(),
    )?;
    bos.ready().await?;
    bos.submit(WriteJob::new(JobOptions::default(), |t| seed_aliases(t)))?
        .await?;
    let readers = crate::read_pool::ReadPool::for_database(
        bos.db_id(),
        bos.generation(),
        crate::config::ConnectionConfig::default(),
        crate::config::AdmissionBudget::default(),
    )
    .await?;
    let core = Registry.open(
        DbId::new(dir.path().join("core.sqlite3"))?,
        WriterOptions::default(),
    )?;
    core.ready().await?;
    let mut p = plan();
    for name in a::COLLECTIONS {
        let mut d = definition(name, "v4-base");
        d.app_id = "ctox".into();
        p.collections.get_mut("ctox").unwrap().push(d);
    }
    let w = Registry.open(
        DbId::new(dir.path().join("sync.sqlite3"))?,
        WriterOptions::default(),
    )?;
    w.ready().await?;
    w.submit(WriteJob::new(JobOptions::default(), |t| {
        sql(schema::initialize(t))
    }))?
    .await?;
    a::startup(&core, &w, &readers).await?;
    let premature = core
        .submit(WriteJob::new(JobOptions::default(), |t| {
            sql(
                t.query_row("SELECT count(*) FROM _workjet_owner_migrations", [], |r| {
                    r.get::<_, usize>(0)
                }),
            )
        }))?
        .await?;
    assert_eq!(premature, 0);
    base_release::publish(&w, &p, &ReleaseMigration, 1).await?;
    w.submit(WriteJob::new(JobOptions::default(), |tx| {
        for collection in a::COLLECTIONS {
            for (id, owner) in [
                ("mine", "owner@example.test"),
                ("foreign", "other@example.test"),
            ] {
                insert_document(
                    tx,
                    &Document {
                        collection: collection.into(),
                        id: id.into(),
                        body: json!({"id":id,"owner_user_id":owner}),
                        revision: None,
                    },
                )?;
            }
        }
        Ok(())
    }))?
    .await?;
    a::startup(&core, &w, &readers).await?;
    for collection in a::COLLECTIONS {
        let docs = stored(&w, collection).await?;
        assert_eq!(docs[0]["owner_user_id"], "other@example.test");
        assert_eq!(docs[1]["owner_user_id"], OWNER);
    }
    let before = w
        .submit(WriteJob::new(JobOptions::default(), |t| {
            sql(t.query_row("SELECT MAX(seq) FROM _changes", [], |r| r.get::<_, u64>(0)))
        }))?
        .await?;
    // Crash after Sync commit but before Core marker: replay changes nothing.
    core.submit(WriteJob::new(JobOptions::default(), |t| {
        sql(t.execute("DELETE FROM _workjet_owner_migrations", []))?;
        Ok(())
    }))?
    .await?;
    a::startup(&core, &w, &readers).await?;
    // A completed startup does not consult a now unavailable identity store.
    readers.shutdown().await.map_err(StoreError::from)?;
    a::startup(&core, &w, &readers).await?;
    let after = w
        .submit(WriteJob::new(JobOptions::default(), |t| {
            sql(t.query_row("SELECT MAX(seq) FROM _changes", [], |r| r.get::<_, u64>(0)))
        }))?
        .await?;
    assert_eq!(before, after);
    proof(
        "e34-startup",
        json!({"before_seq":before,"after_seq":after,"foreign_unchanged":true,"marker_replay_without_identity_read":true}),
    )?;
    bos.shutdown().await?;
    core.shutdown().await?;
    w.shutdown().await
}
#[test]
fn legacy_archive_enforces_schema_bounds_not_just_outer_types() -> Result<()> {
    let mut body = json!({"id":"a".repeat(64),"source_db":"bos","source_table":"t","collection":"unknown","collection_version":0,"record_id":"r","doc":{},"sha256":"44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a","imported_at":1});
    c::document_hash(c::ARCHIVE, &body)?;
    for (field, bad) in [
        ("source_db", json!("x".repeat(257))),
        ("record_id", json!("x".repeat(513))),
        ("id", json!("A".repeat(64))),
        ("sha256", json!("A".repeat(64))),
        ("collection_version", json!(-1)),
        ("doc", json!("lost structure")),
    ] {
        let saved = body[field].clone();
        body[field] = bad;
        assert!(c::document_hash(c::ARCHIVE, &body).is_err(), "{field}");
        body[field] = saved;
    }
    Ok(())
}

#[test]
fn freeze_measurement_matches_operator_sqlite_api_and_detects_mutation() -> Result<()> {
    let local = Connection::open_in_memory()?;
    local.execute_batch("CREATE TABLE t(v);INSERT INTO t VALUES(1),(1),('1'),(X'31'),(NULL);")?;
    let before = super::super::freeze::measure(&local, "t")?;
    local.execute("DELETE FROM t WHERE rowid=1", [])?;
    let after = super::super::freeze::measure(&local, "t")?;
    assert_eq!(before.row_count, 5);
    assert_eq!(after.row_count, 4);
    assert_ne!(before.source_table_hash, after.source_table_hash);
    let Some(dir) = std::env::var_os("E34_PROOF_DIR") else {
        return Ok(());
    };
    let path = std::path::PathBuf::from(dir);
    let db = Connection::open_with_flags(
        path.join("freeze.sqlite3"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let expected: Value =
        serde_json::from_slice(&std::fs::read(path.join("freeze.json"))?).unwrap();
    let actual = super::super::freeze::measure(&db, "values_table")?;
    assert_eq!(
        actual.row_count,
        expected["row_count"].as_u64().unwrap() as usize
    );
    assert_eq!(
        actual.source_table_hash,
        expected["source_table_hash"].as_str().unwrap()
    );
    assert_eq!(
        serde_json::to_value(actual.schema_columns).unwrap(),
        expected["schema_columns"]
    );
    Ok(())
}

#[test]
fn e34_native_aliases_never_infer_profiles_or_revoked_owners() -> Result<()> {
    let db = Connection::open_in_memory()?;
    seed_aliases(&db)?;
    let mut original = json!({"id":"r","owner_user_id":"owner@example.test"});
    assert!(
        super::super::owner_alias::Aliases::load(&db)?.migrate("workjet_sessions", &mut original)
    );
    sql(db.execute("UPDATE business_users SET active=0", []))?;
    let mut revoked = json!({"id":"r","owner_user_id":"owner@example.test"});
    assert!(
        !super::super::owner_alias::Aliases::load(&db)?.migrate("workjet_sessions", &mut revoked)
    );
    assert_eq!(revoked["owner_user_id"], "owner@example.test");
    sql(db.execute_batch("ALTER TABLE business_users ADD COLUMN profile_json TEXT;UPDATE business_users SET active=1,profile_json='{\"email\":\"forged@example.test\"}';"))?;
    let mut forged = json!({"id":"r","owner_user_id":"forged@example.test"});
    assert!(
        !super::super::owner_alias::Aliases::load(&db)?.migrate("workjet_computers", &mut forged)
    );
    Ok(())
}
