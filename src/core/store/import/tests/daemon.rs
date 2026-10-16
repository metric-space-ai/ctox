use super::*;
use crate::{registry::Registry, schema, DbId, WriteJob};
use serde_json::json;
use std::sync::Arc;

fn migration(_: &str, _: &Value, body: Value) -> Result<Value> {
    Ok(body)
}
fn definition(name: &str) -> schema::CollectionDefinition {
    schema::CollectionDefinition {
        name: name.into(),
        app_id: "ctox".into(),
        schema_ver: 1,
        schema_json: json!({"version":1,"primaryKey":"id","type":"object","properties":{"id":{"type":"string"}}}),
        class: "full".into(),
        client_limit: 100,
        byte_budget: 1048576,
        rights_ver: 1,
        app_ver: "frozen-v4".into(),
    }
}
fn table(name: &str, collection: &str, mode: Mode) -> Table {
    Table {
        name: name.into(),
        collection: collection.into(),
        version: 1,
        id_column: "id".into(),
        body_column: Some("data".into()),
        entity: cutover::Entity::App,
        mode,
    }
}
fn measurements(source: &mut Source) {
    source.measurement_sha256 = hash(
        &serde_json::to_vec(
            &source
                .tables
                .iter()
                .map(|d| (&d.table.name, &d.measurement))
                .collect::<BTreeMap<_, _>>(),
        )
        .unwrap(),
    );
}
fn fixture(root: &Path) -> Result<PathBuf> {
    let live = root.join("v3-live.sqlite3");
    let copied = root.join("frozen");
    fs::create_dir(&copied)?;
    let source = Connection::open(&live)?;
    source.execute_batch("CREATE TABLE docs(id TEXT PRIMARY KEY,data TEXT,deleted INTEGER,revision TEXT);INSERT INTO docs VALUES('d','{\"id\":\"d\",\"title\":\"retained\"}',0,'4-source');CREATE TABLE internal(id INTEGER PRIMARY KEY AUTOINCREMENT,data TEXT,deleted INTEGER);INSERT INTO internal(data,deleted) VALUES('{\"context\":\"collection\",\"data\":{\"name\":\"known\",\"version\":1,\"schema\":{\"version\":1}}}',0);CREATE TABLE custom(id TEXT PRIMARY KEY,data TEXT,deleted INTEGER,revision TEXT);INSERT INTO custom VALUES('c','{\"id\":\"c\",\"extra\":true}',0,'3-custom');")?;
    let frozen = copied.join("rxdb.sqlite3");
    // Real SQLite backup API, never fs::copy of the source database.
    source.backup(rusqlite::DatabaseName::Main, &frozen, None)?;
    let ro = Connection::open_with_flags(&frozen, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut s = Source {
        choice: FileChoice {
            database: "business-os-rxdb.sqlite3".into(),
            live_file: live,
            frozen_file: frozen,
            lsof_receipt_sha256: "a".repeat(64),
        },
        measurement_sha256: String::new(),
        kind: SourceKind::Rxdb,
        internal_table: Some("internal".into()),
        collectors: vec![],
        tables: vec![
            Decision {
                rule: "E29-R1".into(),
                table: table("docs", "known", Mode::Rows),
                measurement: Some(freeze::measure(&ro, "docs")?),
            },
            Decision {
                rule: "E29-R4".into(),
                table: table("internal", "metadata", Mode::KeepCore),
                measurement: Some(freeze::measure(&ro, "internal")?),
            },
            Decision {
                rule: "E29-R2".into(),
                table: table("custom", "unknown", Mode::Rows),
                measurement: Some(freeze::measure(&ro, "custom")?),
            },
        ],
    };
    measurements(&mut s);
    let snapshot = Snapshot {
        schema: "ctox.v4.cutover_snapshot.v1".into(),
        frozen: true,
        method: "sqlite_backup_api".into(),
        imported_at_ms: 42,
        archive_custodian_user: "owner".into(),
        release: ReleaseFiles {
            v4_root: copied.join("v4"),
            platform_manifest: copied.join("ctox-platform.manifest.json"),
            app_ver: "frozen-v4".into(),
            client_limit: 100,
            byte_budget: 1048576,
            rights_ver: 1,
        },
        databases: vec![s],
        inventory: BTreeMap::from([(
            "business-os-rxdb.sqlite3".into(),
            BTreeSet::from(["docs".into(), "internal".into(), "custom".into()]),
        )]),
        retained: vec![],
    };
    let v4 = copied.join("v4");
    fs::create_dir_all(v4.join("tools/manifest/out"))?;
    fs::create_dir(v4.join("manifests"))?;
    fs::create_dir(v4.join("schemas"))?;
    fs::write(v4.join("tools/manifest/out/ownership.json"), "{}")?;
    let collections: serde_json::Map<String, Value> = ["known", cutover::ARCHIVE, "files"]
        .into_iter()
        .map(|name| {
            (
                name.into(),
                json!({"class":"full","schema":definition(name).schema_json}),
            )
        })
        .collect();
    fs::write(
        v4.join("manifests/ctox.manifest.json"),
        json!({"app_id":"ctox","collections":collections}).to_string(),
    )?;
    fs::write(
        copied.join("ctox-platform.manifest.json"),
        json!({"app_id":"ctox-platform","collections":{"business_commands":{"class":"full"}}})
            .to_string(),
    )?;
    let path = copied.join("cutover.json");
    fs::write(&path, serde_json::to_vec(&snapshot).unwrap())?;
    Ok(path)
}
async fn terminal(receipt: Vec<u8>) -> Value {
    let receipt: Value = serde_json::from_slice(&receipt).unwrap();
    let path = PathBuf::from(receipt["report_path"].as_str().unwrap());
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            if value["state"] != "admitted" {
                break value;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}
async fn send(endpoint: ipc::StoreEndpoint, path: &Path, dry_run: bool) -> Result<Value> {
    Ok(terminal(
        ipc::request(
            endpoint,
            Request {
                request_id: 1,
                kind: if dry_run {
                    JobKind::Read
                } else {
                    JobKind::Write
                },
                method: METHOD.into(),
                payload: serde_json::to_vec(&ImportRequest {
                    snapshot: path.into(),
                    dry_run,
                })
                .unwrap(),
            },
            JobOptions::default(),
        )?
        .await?,
    )
    .await)
}

#[tokio::test]
async fn cutover_daemon_ipc_uses_one_owner_dry_run_is_readonly_and_replay_is_seq_neutral(
) -> Result<()> {
    let root = tempfile::tempdir()?;
    let snapshot = fixture(root.path())?;
    let endpoint = ipc::StoreEndpoint::UnixSocket {
        path: root.path().join("ipc/store.sock"),
    };
    let fixture =
        crate::testkit::StoreFixture::open(DbId::new(root.path().join("destination.sqlite3"))?)
            .await?;
    let writer = fixture.writer();
    let attachment = Attachment::new(
        writer.clone(),
        fixture.readers(),
        vec![writer.db_id().path().into()],
        root.path().join("runtime"),
        tokio::runtime::Handle::current(),
        migration,
        BTreeMap::new(),
    );
    let daemon = ipc::DaemonIpc::start(endpoint.clone(), Arc::new(attachment))?;
    assert!(
        matches!(
            Registry.open(writer.db_id().clone(), Default::default()),
            Err(StoreError::AlreadyOwned)
        ),
        "second writer must never be admitted"
    );
    assert!(
        ipc::MaintenanceOwner::acquire(&endpoint, writer.db_id().clone()).is_err(),
        "offline writer must not bypass the foreground daemon"
    );
    let dry = send(endpoint.clone(), &snapshot, true).await?;
    assert_eq!(dry["state"], "completed", "{dry}");
    assert_eq!(dry["report"]["dry_run"], true);
    assert_eq!(dry["report"]["import"], Value::Null);
    assert!(!root.path().join("runtime/transfers").exists());
    let empty = writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            Ok(tx.query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='_collections'",
                [],
                |r| r.get::<_, u64>(0),
            )?)
        }))?
        .await?;
    assert_eq!(empty, 0, "dry run must not even publish a schema");
    let imported = send(endpoint.clone(), &snapshot, false).await?;
    assert_eq!(imported["state"], "completed", "{imported}");
    assert_eq!(
        imported["report"]["import"]["collections"]["known"]["target_documents"],
        1
    );
    assert_eq!(
        imported["report"]["import"]["collections"][cutover::ARCHIVE]["target_documents"],
        1
    );
    let head = writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            Ok(crate::change_log::head(tx)?.head_seq)
        }))?
        .await?;
    let replay = send(endpoint.clone(), &snapshot, false).await?;
    assert_eq!(replay["state"], "completed", "{replay}");
    let after = writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            Ok(crate::change_log::head(tx)?.head_seq)
        }))?
        .await?;
    assert_eq!(head, after);
    daemon.shutdown()?.await?;
    fixture.shutdown().await?;
    Ok(())
}
#[test]
fn cutover_unmapped_table_and_live_sources_fail_closed() -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = fixture(root.path())?;
    let original = fs::read(&path)?;
    let mut snapshot: Snapshot = serde_json::from_slice(&original).unwrap();
    snapshot.databases[0].tables.pop();
    snapshot
        .inventory
        .get_mut("business-os-rxdb.sqlite3")
        .unwrap()
        .remove("custom");
    measurements(&mut snapshot.databases[0]);
    fs::write(&path, serde_json::to_vec(&snapshot).unwrap())?;
    assert!(
        matches!(prepare(path.clone(),&[],migration),Err(StoreError::Invalid(s)) if s=="cutover_table_without_decision")
    );
    fs::write(&path, &original)?;
    let snapshot: Snapshot = serde_json::from_slice(&original).unwrap();
    assert!(
        matches!(prepare(path.clone(),&[snapshot.databases[0].choice.frozen_file.clone()],migration),Err(StoreError::Invalid(s)) if s=="cutover_live_or_duplicate_source_forbidden")
    );
    let good = prepare(path, &[], migration)?;
    assert_eq!(
        good.tables.len(),
        3,
        "sqlite_sequence is internal, not an unmapped user table"
    );
    assert_eq!(good.tables[0].source_rows, Some(1));
    assert_eq!(good.tables[0].expected_target_rows, 1);
    Ok(())
}
#[test]
fn cutover_cli_is_strict_and_absent_daemon_never_opens_a_writer() -> Result<()> {
    let args = [
        "v4",
        "cutover",
        "import",
        "--snapshot",
        "/snapshot",
        "--dry-run",
    ]
    .map(str::to_owned);
    assert!(cli_request(&args)?.dry_run);
    assert!(
        cli_request(&[args.to_vec(), vec!["--root".into(), "/other".into()]].concat()).is_err()
    );
    let root = tempfile::tempdir()?;
    let endpoint = ipc::StoreEndpoint::UnixSocket {
        path: root.path().join("no-daemon/store.sock"),
    };
    assert!(request(endpoint, cli_request(&args)?).is_err());
    assert!(!root.path().join("destination.sqlite3").exists());
    Ok(())
}

#[test]
fn cutover_specialized_tracking_keeps_raw_rows_and_adds_native_projection() -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = fixture(root.path())?;
    let mut snapshot: Snapshot = serde_json::from_slice(&fs::read(&path)?).unwrap();
    let live = root.path().join("tracking-live.sqlite3");
    let source = Connection::open(&live)?;
    source.execute_batch("CREATE TABLE stalwart_mail_tracking_tokens(token TEXT PRIMARY KEY,message_id TEXT,campaign_id TEXT,event_type TEXT,target_url TEXT,created_at INTEGER);INSERT INTO stalwart_mail_tracking_tokens VALUES('token','message','campaign','opened',NULL,123);CREATE TABLE stalwart_mail_tracking_events(id INTEGER PRIMARY KEY,token TEXT,message_id TEXT,event_type TEXT,occurred_at INTEGER,user_agent TEXT);INSERT INTO stalwart_mail_tracking_events VALUES(17,'token','message','opened',123456,'test');")?;
    let frozen = path.parent().unwrap().join("tracking.sqlite3");
    source.backup(rusqlite::DatabaseName::Main, &frozen, None)?;
    let ro = Connection::open_with_flags(&frozen, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let names = [
        "stalwart_mail_tracking_tokens",
        "stalwart_mail_tracking_events",
    ];
    let mut source = Source {
        choice: FileChoice {
            database: "tracking".into(),
            live_file: live,
            frozen_file: frozen,
            lsof_receipt_sha256: "b".repeat(64),
        },
        kind: SourceKind::Core,
        internal_table: None,
        measurement_sha256: String::new(),
        collectors: vec![super::super::daemon_collectors::Collector::MailTracking],
        tables: names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                Ok(Decision {
                    rule: "E29-R5".into(),
                    table: Table {
                        name: (*name).into(),
                        collection: format!("legacy_{name}"),
                        version: 0,
                        id_column: if i == 0 { "token" } else { "id" }.into(),
                        body_column: None,
                        entity: cutover::Entity::Core,
                        mode: Mode::Rows,
                    },
                    measurement: Some(freeze::measure(&ro, name)?),
                })
            })
            .collect::<Result<_>>()?,
    };
    measurements(&mut source);
    snapshot.inventory.insert(
        "tracking".into(),
        names.map(str::to_owned).into_iter().collect(),
    );
    snapshot.databases.push(source);
    fs::write(&path, serde_json::to_vec(&snapshot).unwrap())?;
    let prepared = prepare(path.clone(), &[], migration)?;
    let docs = &prepared.import.snapshot.documents;
    let token = docs
        .iter()
        .find(|d| d.collection == "outbound_tracking_tokens")
        .expect("specialized collector must run");
    assert_eq!(token.body["created_at_ms"], 123000);
    assert!(docs.iter().any(|d| d.collection == cutover::ARCHIVE
        && d.body["source_table"] == names[0]
        && d.body["doc"]["created_at"] == 123));
    assert!(
        prepared.import.audit.target_documents["tracking:stalwart_mail_tracking_tokens"]
            .contains(&(token.collection.clone(), token.id.clone()))
    );
    let mut snapshot: Snapshot = serde_json::from_slice(&fs::read(&path)?).unwrap();
    snapshot.databases[1]
        .collectors
        .push(super::super::daemon_collectors::Collector::MailTracking);
    fs::write(&path, serde_json::to_vec(&snapshot).unwrap())?;
    assert!(
        matches!(prepare(path, &[], migration), Err(StoreError::Invalid(s)) if s == "cutover_duplicate_collector_source")
    );
    Ok(())
}

#[tokio::test]
async fn verification_retries_only_deadlines_and_never_reopens_a_closed_owner() -> Result<()> {
    let root = tempfile::tempdir()?;
    let fixture =
        crate::testkit::StoreFixture::open(DbId::new(root.path().join("verify.sqlite3"))?).await?;
    let pool = fixture.readers();
    let mut calls = 0;
    let result = verified_read(&pool, "fixture", || {
        calls += 1;
        crate::ReadJob::new(
            JobOptions {
                deadline: Some(std::time::Instant::now() - Duration::from_secs(1)),
                ..Default::default()
            },
            |_| Ok(0usize),
        )
    })
    .await;
    assert!(matches!(result, Err(StoreError::DeadlineExceeded)));
    assert_eq!(calls, 3, "verification must be bounded");
    fixture.shutdown().await?;
    let mut closed_calls = 0;
    let result = verified_read(&pool, "fixture", || {
        closed_calls += 1;
        crate::ReadJob::new(JobOptions::default(), |_| Ok(0usize))
    })
    .await;
    assert!(matches!(result, Err(StoreError::Closed)));
    assert_eq!(
        closed_calls, 1,
        "closed generation must not be retried or replaced"
    );
    Ok(())
}

#[test]
fn core_collector_projects_ticket_and_archives_full_raw_row_without_inventing_a_conversation(
) -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = fixture(root.path())?;
    let frozen = path.parent().unwrap();
    let live = root.path().join("core-live.sqlite3");
    let db = Connection::open(&live)?;
    db.execute_batch("CREATE TABLE ticket_items(ticket_key TEXT PRIMARY KEY,title TEXT,metadata_json TEXT,created_at TEXT);INSERT INTO ticket_items VALUES('ticket-1','Preserved ticket','{\"extra\":true}','2026-10-16T00:00:00Z');")?;
    let copy = frozen.join("core.sqlite3");
    db.backup(rusqlite::DatabaseName::Main, &copy, None)?;
    let ro = Connection::open_with_flags(&copy, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut snapshot: Snapshot = serde_json::from_slice(&fs::read(&path)?).unwrap();
    let mut source = Source {
        choice: FileChoice {
            database: "ctox.sqlite3".into(),
            live_file: live,
            frozen_file: copy,
            lsof_receipt_sha256: "c".repeat(64),
        },
        measurement_sha256: String::new(),
        kind: SourceKind::Core,
        internal_table: None,
        tables: vec![Decision {
            rule: "E29-R5".into(),
            table: Table {
                name: "ticket_items".into(),
                collection: "ctox_ticket_items".into(),
                version: 1,
                id_column: "ticket_key".into(),
                body_column: None,
                entity: cutover::Entity::Core,
                mode: Mode::ArchiveOnly,
            },
            measurement: Some(freeze::measure(&ro, "ticket_items")?),
        }],
        collectors: vec![daemon_collectors::Collector::Core {
            tables: vec!["ticket_items".into()],
            original_root: root.path().into(),
            copied_root: frozen.into(),
            owner_user: None,
        }],
    };
    measurements(&mut source);
    snapshot.inventory.insert(
        "ctox.sqlite3".into(),
        BTreeSet::from(["ticket_items".into()]),
    );
    snapshot.databases.push(source);
    fs::write(frozen.join("v4/manifests/tickets.manifest.json"),json!({"app_id":"tickets","collections":{"ctox_ticket_items":{"class":"full","schema":definition("ctox_ticket_items").schema_json}}}).to_string())?;
    fs::write(&path, serde_json::to_vec(&snapshot).unwrap())?;
    assert!(
        matches!(prepare(path.clone(),&[],migration),Err(StoreError::Invalid(s)) if s=="cutover_core_conversation_rule_required")
    );
    fn not_a_crew_fixture(_: &str) -> i64 {
        panic!("ticket projection must not invent crew context")
    }
    let result = prepare_with_core(path.clone(), &[], migration, Some(not_a_crew_fixture))?;
    let doc = result
        .import
        .snapshot
        .documents
        .iter()
        .find(|d| d.collection == "ctox_ticket_items")
        .expect("native Core projection must run");
    assert_eq!(doc.id, "ticket-1");
    assert_eq!(doc.body["title"], "Preserved ticket");
    assert_eq!(doc.body["metadata"]["extra"], true);
    assert!(result
        .import
        .snapshot
        .documents
        .iter()
        .any(|d| d.collection == cutover::ARCHIVE
            && d.body["source_table"] == "ticket_items"
            && d.body["doc"]["metadata_json"] == "{\"extra\":true}"));
    let mut snapshot: Snapshot = serde_json::from_slice(&fs::read(&path)?).unwrap();
    if let daemon_collectors::Collector::Core { tables, .. } =
        &mut snapshot.databases[1].collectors[0]
    {
        tables.clear();
    }
    fs::write(&path, serde_json::to_vec(&snapshot).unwrap())?;
    assert!(
        matches!(prepare_with_core(path.clone(),&[],migration,Some(not_a_crew_fixture)),Err(StoreError::Invalid(s)) if s=="cutover_core_collector_coverage")
    );
    let copy = frozen.join("core.sqlite3");
    let data = Connection::open(&copy)?;
    data.execute_batch("CREATE TABLE iot_datapoints(asset_id TEXT,attribute_name TEXT,timestamp_ms INTEGER,value TEXT);INSERT INTO iot_datapoints VALUES('asset','temperature',1,'42');")?;
    let core = &mut snapshot.databases[1];
    core.tables.push(Decision {
        rule: "E29-R5".into(),
        table: Table {
            name: "iot_datapoints".into(),
            collection: "iot_timeseries".into(),
            version: 0,
            id_column: "$rowid".into(),
            body_column: None,
            entity: cutover::Entity::Core,
            mode: Mode::ArchiveOnly,
        },
        measurement: Some(freeze::measure(&data, "iot_datapoints")?),
    });
    if let daemon_collectors::Collector::Core { tables, .. } = &mut core.collectors[0] {
        *tables = vec!["ticket_items".into(), "iot_datapoints".into()];
    }
    measurements(core);
    snapshot
        .inventory
        .get_mut("ctox.sqlite3")
        .unwrap()
        .insert("iot_datapoints".into());
    fs::write(&path, serde_json::to_vec(&snapshot).unwrap())?;
    assert!(
        matches!(prepare_with_core(path,&[],migration,Some(not_a_crew_fixture)),Err(StoreError::Invalid(s)) if s=="cutover_dataframe_owner_required")
    );
    Ok(())
}
