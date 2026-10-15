#[test]
fn schema_archive_preserves_old_value_but_explicit_remove_erases_it_and_denies_deleted_revision_writes(
) {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    let old = definition("items");
    create_collection(&tx, &old).unwrap();
    put(&tx, "c_items", "one", "1");
    reserve_publish(&tx, &lock()).unwrap();
    prepare_migration(&tx, &lock(), &next(&old)).unwrap();
    let page = change_log::read_page(&tx, "items", 1, 0, 500).unwrap();
    apply_migration_page(
        &tx,
        &lock(),
        "items",
        0,
        &page,
        &[MigrationOutput::Archive {
            reason: "not convertible".into(),
        }],
        100,
    )
    .unwrap();
    assert_eq!(
        count(&tx, "_archive"),
        1,
        "E4 archival survives target tombstone creation"
    );
    remove_document(&tx, "items", 1, "one", "1", "removed").unwrap();
    assert_eq!(count(&tx, "_archive"), 0);
    assert!(dual_write(
        &tx,
        &lock(),
        "items",
        &SourceWrite {
            id: "one".into(),
            base_rev: Some("removed".into()),
            rev: "resurrect".into(),
            body: json!({"value":"new"}),
            deleted: false
        },
        &MigrationOutput::Upsert {
            rev: "new".into(),
            body: json!({"value":42})
        },
        101
    )
    .is_err());
    assert!(tx
        .query_row(
            "SELECT data IS NULL FROM c_items__g2 WHERE id='one'",
            [],
            |r| r.get::<_, bool>(0)
        )
        .unwrap());
}
fn with_online_index() -> CollectionDefinition {
    let mut new = definition("items");
    new.schema_ver = 2;
    new.app_ver = "2".into();
    new.schema_json["indexes"] = json!(["value", "other"]);
    new
}
fn physical_rows(
    tx: &Transaction<'_>,
    table: &str,
) -> Vec<(String, String, u32, Option<String>, bool)> {
    tx.prepare(&format!(
        "SELECT id,rev,schema_ver,data,deleted FROM {table} ORDER BY id"
    ))
    .unwrap()
    .query_map([], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
    })
    .unwrap()
    .collect::<Result<Vec<_>>>()
    .unwrap()
}
#[test]
fn online_index_resumes_and_mirrors_concurrent_writes_without_changing_generation_or_sequences() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("online.sqlite");
    let mut db = Connection::open(&path).unwrap();
    {
        let tx = db.transaction().unwrap();
        create_collection(&tx, &definition("items")).unwrap();
        for n in 0..12 {
            put(&tx, "c_items", &format!("{n:04}"), "1");
        }
        reserve_publish(&tx, &lock()).unwrap();
        update_compatible(&tx, &lock(), &with_online_index()).unwrap();
        let page = build_online_index_page(&tx, &lock(), "items", 3).unwrap();
        assert_eq!(
            (page.cursor.as_deref(), page.copied, page.complete),
            (Some("0002"), 3, false)
        );
        tx.commit().unwrap();
    }
    drop(db);
    let mut db = Connection::open(&path).unwrap();
    {
        let tx = db.transaction().unwrap();
        tx.execute(
            "UPDATE c_items SET rev='updated',data=json_set(data,'$.other',42) WHERE id='0000'",
            [],
        )
        .unwrap();
        tx.execute("UPDATE c_items SET id='Z2' WHERE id='0002'", [])
            .unwrap();
        tx.execute("DELETE FROM c_items WHERE id='0001'", [])
            .unwrap();
        put(&tx, "c_items", "000A", "new");
        remove_document(&tx, "items", 1, "0003", "1", "deleted").unwrap();
        tx.commit().unwrap();
    }
    loop {
        let tx = db.transaction().unwrap();
        let page = build_online_index_page(&tx, &lock(), "items", 3).unwrap();
        assert!(page.copied <= 3);
        tx.commit().unwrap();
        if page.complete {
            break;
        }
    }
    let tx = db.transaction().unwrap();
    put(&tx, "c_items", "late", "latest");
    let before = physical_rows(&tx, "c_items");
    let head = change_log::head(&tx).unwrap();
    activate_publication(&tx, &lock(), 100).unwrap();
    assert_eq!(change_log::head(&tx).unwrap(), head); // No synthetic document mutations during physical swap.
    assert_eq!(physical_rows(&tx, "c_items"), before);
    assert_eq!(active_collection(&tx, "items").unwrap().unwrap().gen, 1);
    assert!(has_index(&tx, "c_items", &["other".into()]).unwrap());
    let column = indexed_column("other").unwrap();
    let detail: String = tx
        .query_row(
            &format!("EXPLAIN QUERY PLAN SELECT id FROM c_items WHERE {column}=42"),
            [],
            |r| r.get(3),
        )
        .unwrap();
    assert!(detail.contains("USING INDEX"), "{detail}");
    let retired: String = tx
        .query_row("SELECT retired_table FROM _index_builds", [], |r| r.get(0))
        .unwrap();
    // Both native remove and a foreign hard delete scrub old physical copies immediately.
    tx.execute("DELETE FROM c_items WHERE id='0000'", [])
        .unwrap();
    let erased: bool = tx
        .query_row(
            &format!("SELECT data IS NULL AND deleted=1 FROM {retired} WHERE id='0000'"),
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(erased);
    // A subsequent physical swap must reattach all older retained-copy privacy
    // triggers to the new active table, even if its immediate predecessor is GC'd.
    let second = PublishLock {
        app_id: "app".into(),
        base_app_ver: "2".into(),
        target_app_ver: "3".into(),
        operation_id: "second-index".into(),
    };
    reserve_publish(&tx, &second).unwrap();
    let mut v3 = with_online_index();
    v3.schema_ver = 3;
    v3.app_ver = "3".into();
    v3.schema_json["indexes"] = json!(["value", "other", "third"]);
    update_compatible(&tx, &second, &v3).unwrap();
    assert!(
        build_online_index_page(&tx, &second, "items", 100)
            .unwrap()
            .complete
    );
    activate_publication(&tx, &second, 101).unwrap();
    assert!(purge_online_index_page(&tx, &second.operation_id, "items", 500).unwrap());
    tx.execute("DELETE FROM c_items WHERE id='0004'", [])
        .unwrap();
    let erased: bool = tx
        .query_row(
            &format!("SELECT data IS NULL AND deleted=1 FROM {retired} WHERE id='0004'"),
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        erased,
        "older copy remains protected after a later copy is retired"
    );

    tx.commit().unwrap();
}

#[test]
fn aborting_an_online_index_preserves_active_data_and_uses_bounded_private_copy_cleanup() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    create_collection(&tx, &definition("items")).unwrap();
    for n in 0..7 {
        put(&tx, "c_items", &n.to_string(), "1");
    }
    reserve_publish(&tx, &lock()).unwrap();
    update_compatible(&tx, &lock(), &with_online_index()).unwrap();
    build_online_index_page(&tx, &lock(), "items", 3).unwrap();
    let target: String = tx
        .query_row("SELECT target_table FROM _index_builds", [], |r| r.get(0))
        .unwrap();
    abort_publication(&tx, &lock(), 100).unwrap();
    assert_eq!(count(&tx, "c_items"), 7);
    tx.execute("DELETE FROM c_items WHERE id='0'", []).unwrap();
    assert!(tx
        .query_row(
            &format!("SELECT data IS NULL FROM {target} WHERE id='0'"),
            [],
            |r| r.get::<_, bool>(0)
        )
        .unwrap());
    assert!(!purge_online_index_page(&tx, &lock().operation_id, "items", 2).unwrap());
    assert_eq!(count(&tx, &target), 1);
    assert!(purge_online_index_page(&tx, &lock().operation_id, "items", 2).unwrap());
    assert_eq!(
        active_collection(&tx, "items").unwrap().unwrap().definition,
        definition("items")
    );
    put(&tx, "c_items", "still-writable", "1");
}
#[test]
fn online_index_physical_swap_rolls_back_when_another_collection_cutover_fails() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    create_collection(&tx, &definition("items")).unwrap();
    create_collection(&tx, &definition("z")).unwrap();
    put(&tx, "c_items", "one", "1");
    reserve_publish(&tx, &lock()).unwrap();
    update_compatible(&tx, &lock(), &with_online_index()).unwrap();
    let mut z = definition("z");
    z.app_ver = "2".into();
    update_compatible(&tx, &lock(), &z).unwrap();
    build_online_index_page(&tx, &lock(), "items", 10).unwrap();
    tx.execute("UPDATE _schema_updates SET gen=99 WHERE collection='z'", [])
        .unwrap();
    assert!(activate_publication(&tx, &lock(), 100).is_err());
    assert_eq!(physical_rows(&tx, "c_items").len(), 1);
    assert!(!has_index(&tx, "c_items", &["other".into()]).unwrap());
    assert_eq!(
        tx.query_row("SELECT state FROM _index_builds", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "ready"
    );
    tx.execute("UPDATE _schema_updates SET gen=1 WHERE collection='z'", [])
        .unwrap();
    activate_publication(&tx, &lock(), 100).unwrap();
    assert!(has_index(&tx, "c_items", &["other".into()]).unwrap());
}
#[test]
#[ignore = "subprocess-only crash fixture"]
fn online_index_crash_writer_child() {
    let path = std::env::var_os("CTOX_TEST_ONLINE_INDEX_CRASH_DB")
        .expect("parent supplies isolated fixture");
    let mut db = Connection::open(path).unwrap();
    db.pragma_update(None, "journal_mode", "WAL").unwrap();
    {
        let tx = db.transaction().unwrap();
        create_collection(&tx, &definition("items")).unwrap();
        for n in 0..5 {
            put(&tx, "c_items", &n.to_string(), "1");
        }
        reserve_publish(&tx, &lock()).unwrap();
        update_compatible(&tx, &lock(), &with_online_index()).unwrap();
        build_online_index_page(&tx, &lock(), "items", 1).unwrap();
        tx.commit().unwrap();
    }
    let tx = db.transaction().unwrap();
    build_online_index_page(&tx, &lock(), "items", 1).unwrap();
    std::process::exit(86); // No SQLite/Transaction destructors: committed cursor must survive, uncommitted page must not.
}
#[test]
fn online_index_survives_a_process_crash_between_batches() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crash.sqlite");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "schema::tests::online_index_crash_writer_child",
            "--ignored",
            "--test-threads=1",
        ])
        .env("CTOX_TEST_ONLINE_INDEX_CRASH_DB", &path)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(86));
    let mut db = Connection::open(path).unwrap();
    let tx = db.transaction().unwrap();
    assert_eq!(
        tx.query_row("SELECT cursor FROM _index_builds", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "0"
    );
    assert!(
        build_online_index_page(&tx, &lock(), "items", 10)
            .unwrap()
            .complete
    );
    activate_publication(&tx, &lock(), 100).unwrap();
    assert_eq!(count(&tx, "c_items"), 5);
    assert!(has_index(&tx, "c_items", &["other".into()]).unwrap());
}
fn release_fixture() -> BaseReleaseInput {
    serde_json::from_str(include_str!("../../../tests/sync-v4-m2/base-release.json")).unwrap()
}
fn release_defaults() -> BaseReleaseDefaults {
    BaseReleaseDefaults {
        app_ver: "base-test".into(),
        client_limit: 1000,
        byte_budget: 16 * 1024 * 1024,
        rights_ver: 1,
    }
}
#[test]
fn imports_real_199_collection_ownership_copy_and_platform_extensions_atomically_and_idempotently()
{
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    let mut input = release_fixture();
    let mut corrected = input
        .manifests
        .iter()
        .find(|m| m["app_id"] == "browser")
        .unwrap()["collections"]["outbound_lead_generation_adapters"]["schema"]
        .clone();
    corrected["properties"]["nullable_correction"] =
        json!({"type":["string","null"],"default":null});
    input.schemas.insert(
        "outbound-lead-generation".into(),
        json!({"collections":{"outbound_lead_generation_adapters":{"schema":corrected.clone(),"class":"full"}}}),
    );
    let command_schema = crate::change_log::subscription::write_contract::command_document_schema();
    input.schemas.insert(
        "ctox-platform".into(),
        json!({"collections":{"business_commands":{"schema":command_schema}}}),
    );
    let report = import_base_release(&tx, &input, &release_defaults()).unwrap();
    assert_eq!(report.source_ownership_count, 199);
    assert_eq!(report.created, 208);
    assert!(report.legacy_catalog_replaced);
    assert_eq!(report.missing_addenda_definitions.len(), 4); // input producer has not supplied these definitions yet
    let adapter = active_collection(&tx, "outbound_lead_generation_adapters")
        .unwrap()
        .unwrap();
    assert_eq!(adapter.definition.app_id, "outbound-lead-generation");
    assert_eq!(adapter.definition.schema_json, corrected);
    let commands = active_collection(&tx, "business_commands")
        .unwrap()
        .unwrap();
    assert_eq!(commands.definition.schema_ver, 3);
    assert_eq!(commands.definition.schema_json, command_schema);
    assert_eq!(commands.definition.app_id, "ctox-platform");
    assert_eq!(commands.definition.app_ver, "base-test");
    for name in [
        "business_commands",
        "ctox_queue_tasks",
        "ctox_runtime_settings",
        "business_apps",
        "business_app_versions",
        "_bench",
    ] {
        assert_eq!(
            active_collection(&tx, name)
                .unwrap()
                .unwrap()
                .definition
                .app_id,
            "ctox-platform"
        );
    }
    assert!(active_collection(&tx, "business_module_catalog")
        .unwrap()
        .is_none());
    let again = import_base_release(&tx, &input, &release_defaults()).unwrap();
    assert_eq!((again.created, again.existing), (0, 208));
    assert_eq!(change_log::head(&tx).unwrap().head_seq, 0);
}
#[test]
fn invalid_late_base_schema_rolls_back_all_tables_triggers_and_registry_rows() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    initialize(&tx).unwrap();
    let mut input = release_fixture();
    let app = input
        .manifests
        .iter_mut()
        .find(|m| m["app_id"] == "browser")
        .unwrap();
    app["collections"]["zz_invalid"] =
        json!({"class":"full","schema":{"type":"object","indexes":["invalid;sql"]}});
    assert!(import_base_release(&tx, &input, &release_defaults()).is_err());
    assert_eq!(count(&tx, "_collections"), 0);
    assert_eq!(
        tx.query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE name LIKE 'c_%'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}
#[test]
fn base_loader_prefers_optional_corrected_schemas_without_loading_inside_writer() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("tools/manifest/out")).unwrap();
    std::fs::create_dir(root.join("manifests")).unwrap();
    std::fs::create_dir(root.join("schemas")).unwrap();
    std::fs::write(
        root.join("tools/manifest/out/ownership.json"),
        r#"{"ownership":{"items":{"owner":"app"}}}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("manifests/app.manifest.json"),
        r#"{"app_id":"app","collections":{}}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("schemas/app.collections.schema.json"),
        r#"{"collections":{"items":{"type":"object","version":2}}}"#,
    )
    .unwrap();
    let platform = root.join("platform.json");
    std::fs::write(
        &platform,
        r#"{"app_id":"ctox-platform","collections":{"business_commands":{"class":"full"}}}"#,
    )
    .unwrap();
    let loaded = load_base_release(root, &platform).unwrap();
    assert_eq!(loaded.schemas["app"]["collections"]["items"]["version"], 2);
    assert_eq!(loaded.manifests.len(), 2);
}
#[test]
fn remove_is_cas_guarded_payload_free_and_scrubs_revisions_and_archives_atomically() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    create_collection(&tx, &definition("items")).unwrap();
    put(&tx, "c_items", "one", "1");
    tx.execute(
        "INSERT INTO _revs(collection,id,rev,seq,gen,body) VALUES('items','one','1',1,1,?1)",
        [json!({"secret":"old"}).to_string()],
    )
    .unwrap();
    tx.execute("INSERT INTO _archive(app_id,collection,gen,id,rev,app_ver,data,reason,archived_at,expires_at)
        VALUES('app','items',1,'one','1','1',?1,'migration',1,999)",[json!({"secret":"old"}).to_string()]).unwrap();
    assert!(remove_document(&tx, "items", 1, "one", "stale", "2").is_err());
    assert_eq!(count(&tx, "_archive"), 1);
    tx.execute_batch("SAVEPOINT rollback_remove").unwrap();
    remove_document(&tx, "items", 1, "one", "1", "2").unwrap();
    tx.execute_batch("ROLLBACK TO rollback_remove; RELEASE rollback_remove")
        .unwrap();
    assert_eq!(count(&tx, "_archive"), 1);
    let seq = remove_document(&tx, "items", 1, "one", "1", "2").unwrap();
    assert_eq!(count(&tx, "_archive"), 0);
    assert_eq!(count(&tx, "_revs"), 1);
    let body: Option<String> = tx
        .query_row("SELECT body FROM _revs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(body, None);
    let deleted: (bool, Option<String>) = tx
        .query_row("SELECT deleted,data FROM c_items WHERE id='one'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(deleted, (true, None));
    let page = change_log::read_page(&tx, "items", 1, 0, 10).unwrap();
    assert_eq!(page.docs[0].seq, seq);
    assert_eq!(page.docs[0].body, None);
    assert_eq!(page.docs[0].op, Operation::Delete);
    assert!(remove_document(&tx, "items", 1, "one", "2", "3").is_err());
    assert!(tx
        .execute("UPDATE c_items SET data='{}' WHERE id='one'", [])
        .is_err());
}

#[test]
fn foreign_hard_delete_also_scrubs_revision_bodies_and_archives() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    create_collection(&tx, &definition("items")).unwrap();
    put(&tx, "c_items", "one", "1");
    tx.execute(
        "INSERT INTO _revs(collection,id,rev,seq,gen,body) VALUES('items','one','1',1,1,'{}')",
        [],
    )
    .unwrap();
    tx.execute("INSERT INTO _archive(app_id,collection,gen,id,rev,app_ver,data,reason,archived_at,expires_at)
        VALUES('app','items',1,'one','1','1','{}','migration',1,999)",[]).unwrap();
    tx.execute("DELETE FROM c_items WHERE id='one'", [])
        .unwrap();
    assert_eq!(count(&tx, "_archive"), 0);
    assert_eq!(
        tx.query_row("SELECT body IS NULL FROM _revs", [], |r| r
            .get::<_, bool>(0))
            .unwrap(),
        true
    );
}
#[test]
fn platform_window_collection_can_be_installed_and_published_without_a_new_layout() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    let mut def = definition("_bench");
    def.app_id = "ctox-platform".into();
    def.class = "window".into();
    let old = create_collection(&tx, &def).unwrap();
    assert_eq!(old.table, "c__bench");
    let reservation = PublishLock {
        app_id: def.app_id.clone(),
        base_app_ver: "1".into(),
        target_app_ver: "2".into(),
        operation_id: "platform-publish".into(),
    };
    reserve_publish(&tx, &reservation).unwrap();
    def.app_ver = "2".into();
    def.client_limit = 10;
    update_compatible(&tx, &reservation, &def).unwrap();
    assert_eq!(
        active_collection(&tx, "_bench")
            .unwrap()
            .unwrap()
            .definition
            .client_limit,
        1000
    );
    activate_publication(&tx, &reservation, 100).unwrap();
    let active = active_collection(&tx, "_bench").unwrap().unwrap();
    assert_eq!(active.gen, 1);
    assert_eq!(active.definition.class, "window");
    assert_eq!(active.definition.client_limit, 10);
}
#[test]
fn removal_preparation_survives_reopen_and_abort_preserves_the_active_generation() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let mut db = Connection::open(file.path()).unwrap();
    {
        let tx = db.transaction().unwrap();
        create_collection(&tx, &definition("items")).unwrap();
        put(&tx, "c_items", "one", "1");
        reserve_publish(&tx, &lock()).unwrap();
        archive_collection(&tx, &lock(), "items", 1).unwrap();
        archive_collection(&tx, &lock(), "items", 2).unwrap();
        let mut compatible = definition("items");
        compatible.app_ver = "2".into();
        assert!(update_compatible(&tx, &lock(), &compatible).is_err());
        assert!(prepare_migration(&tx, &lock(), &next(&definition("items"))).is_err());
        tx.commit().unwrap();
    }
    drop(db);
    let mut db = Connection::open(file.path()).unwrap();
    let tx = db.transaction().unwrap();
    assert_eq!(active_collection(&tx, "items").unwrap().unwrap().gen, 1);
    assert_eq!(count(&tx, "_archive"), 0);
    tx.execute(
        "UPDATE _schema_apps SET app_ver='changed' WHERE app_id='app'",
        [],
    )
    .unwrap();
    assert!(activate_publication(&tx, &lock(), 100).is_err());
    assert!(active_collection(&tx, "items").unwrap().is_some());
    abort_publication(&tx, &lock(), 101).unwrap();
    assert_eq!(count(&tx, "_schema_removals"), 0);
    assert_eq!(count(&tx, "c_items"), 1);
    assert_eq!(count(&tx, "_archive"), 0);
}

#[test]
fn failed_mixed_cutover_rolls_back_an_earlier_collection_removal() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    create_collection(&tx, &definition("a")).unwrap();
    create_collection(&tx, &definition("z")).unwrap();
    reserve_publish(&tx, &lock()).unwrap();
    archive_collection(&tx, &lock(), "a", 1).unwrap();
    archive_collection(&tx, &lock(), "z", 1).unwrap();
    // Corrupt only a prepared binding to simulate a changed source generation.
    tx.execute(
        "UPDATE _schema_removals SET gen=42 WHERE collection='z'",
        [],
    )
    .unwrap();
    assert!(activate_publication(&tx, &lock(), 100).is_err());
    assert!(active_collection(&tx, "a").unwrap().is_some());
    assert!(active_collection(&tx, "z").unwrap().is_some());
    assert_eq!(count(&tx, "_archive"), 0);
    assert_eq!(count(&tx, "_schema_removals"), 2);
    assert_eq!(count(&tx, "publish_lock"), 1);
}
#[test]
fn migration_sequences_are_lossless_canonical_decimal_strings() {
    let seq = 9_007_199_254_740_993_u64;
    let migration = Migration {
        publication_id: "p".into(),
        collection: "items".into(),
        source_gen: 1,
        gen: 2,
        cursor: seq,
        state: "ready".into(),
        epoch: "e".into(),
        migrated_at_seq: Some(seq + 1),
    };
    let mut wire = serde_json::to_value(&migration).unwrap();
    assert_eq!(wire["cursor"], json!(seq.to_string()));
    assert_eq!(wire["migrated_at_seq"], json!((seq + 1).to_string()));
    assert_eq!(
        serde_json::from_value::<Migration>(wire.clone()).unwrap(),
        migration
    );
    wire["migrated_at_seq"] = json!(seq);
    assert!(serde_json::from_value::<Migration>(wire.clone()).is_err());
    wire["migrated_at_seq"] = json!("01");
    assert!(serde_json::from_value::<Migration>(wire.clone()).is_err());
    wire["migrated_at_seq"] = Value::Null;
    assert_eq!(
        serde_json::from_value::<Migration>(wire.clone())
            .unwrap()
            .migrated_at_seq,
        None
    );
    wire["cursor"] = json!(seq);
    assert!(serde_json::from_value::<Migration>(wire).is_err());
    let collection = Collection {
        definition: definition("items"),
        gen: 2,
        table: "c_items__g2".into(),
        state: "active".into(),
        migrated_at_seq: Some(seq),
    };
    let wire = serde_json::to_value(&collection).unwrap();
    assert_eq!(wire["migrated_at_seq"], json!(seq.to_string()));
    assert_eq!(
        serde_json::from_value::<Collection>(wire).unwrap(),
        collection
    );
}
use super::*;
use rusqlite::Connection;
use serde_json::json;

fn definition(name: &str) -> CollectionDefinition {
    CollectionDefinition {
        name: name.into(),
        app_id: "app".into(),
        schema_ver: 1,
        schema_json: json!({"type":"object","properties":{"value":{"type":"string"}},"indexes":["value"]}),
        class: "replicated".into(),
        client_limit: 1000,
        byte_budget: 2_000_000,
        rights_ver: 1,
        app_ver: "1".into(),
    }
}
fn next(old: &CollectionDefinition) -> CollectionDefinition {
    let mut def = old.clone();
    def.schema_ver = 2;
    def.app_ver = "2".into();
    def.schema_json["properties"]["value"]["type"] = json!("number");
    def
}
fn lock() -> PublishLock {
    PublishLock {
        app_id: "app".into(),
        base_app_ver: "1".into(),
        target_app_ver: "2".into(),
        operation_id: "publication-1".into(),
    }
}
fn put(tx: &Transaction<'_>, table: &str, id: &str, rev: &str) {
    tx.execute(
        &format!(
            "INSERT INTO {table}(id,rev,schema_ver,data,deleted) VALUES(?1,?2,1,?3,0)
        ON CONFLICT(id) DO UPDATE SET rev=excluded.rev,data=excluded.data"
        ),
        params![id, rev, json!({"value":id}).to_string()],
    )
    .unwrap();
}
fn count(tx: &Transaction<'_>, table: &str) -> i64 {
    tx.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
fn outputs(page: &ChangePage) -> Vec<MigrationOutput> {
    page.docs
        .iter()
        .map(|doc| {
            if doc.op == Operation::Delete {
                MigrationOutput::Delete
            } else {
                MigrationOutput::Upsert {
                    rev: format!("m-{}", doc.rev),
                    body: json!({"value":42}),
                }
            }
        })
        .collect()
}

#[test]
fn atomic_hotplug_includes_generated_indexes_triggers_and_registry() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    let def = definition("items");
    let created = create_collection(&tx, &def).unwrap();
    assert_eq!(
        (created.table.as_str(), created.gen, created.state.as_str()),
        ("c_items", 1, "active")
    );
    let triggers: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE type='trigger' AND tbl_name='c_items'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(triggers, 3);
    put(&tx, "c_items", "one", "1");
    let value: String = tx
        .query_row("SELECT j_76616c7565 FROM c_items", [], |r| r.get(0))
        .unwrap();
    assert_eq!(value, "one");
    let plan: String = tx
        .query_row(
            "EXPLAIN QUERY PLAN SELECT id FROM c_items WHERE j_76616c7565='one'",
            [],
            |r| r.get(3),
        )
        .unwrap();
    assert!(plan.contains("USING INDEX"), "{plan}");
    assert_eq!(
        change_log::read_page(&tx, "items", 1, 0, 100)
            .unwrap()
            .docs
            .len(),
        1
    );
    tx.rollback().unwrap();
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='_collections')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!exists);
}

#[test]
fn failed_hotplug_rolls_back_ddl_and_app_registration_even_if_error_is_caught() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    initialize(&tx).unwrap();
    tx.execute_batch("CREATE TABLE c_items(unrelated TEXT)")
        .unwrap();
    assert!(create_collection(&tx, &definition("items")).is_err());
    assert_eq!(count(&tx, "_collections"), 0);
    assert_eq!(count(&tx, "_schema_apps"), 0);
    assert_eq!(count(&tx, "_change_sources"), 0);
    assert_eq!(count(&tx, "c_items"), 0);
    tx.commit().unwrap();
}

#[test]
fn compatible_defaults_keep_generation_and_bytes_but_require_held_publication() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    let old = definition("items");
    create_collection(&tx, &old).unwrap();
    put(&tx, "c_items", "one", "1");
    let before = change_log::head(&tx).unwrap();
    let mut new = old.clone();
    new.schema_ver = 2;
    new.app_ver = "2".into();
    new.schema_json["properties"]["optional"] = json!({"type":"string","default":""});
    assert_eq!(
        classify_change(&old, &new).unwrap(),
        SchemaChange::Compatible
    );
    assert!(update_compatible(&tx, &lock(), &new).is_err());
    reserve_publish(&tx, &lock()).unwrap();
    let updated = update_compatible(&tx, &lock(), &new).unwrap();
    assert_eq!(updated.gen, 1);
    assert_eq!(updated.definition.app_ver, "1");
    assert_eq!(change_log::head(&tx).unwrap(), before);
    let page = change_log::read_page(&tx, "items", 1, 0, 10).unwrap();
    assert!(page.docs[0]
        .body
        .as_ref()
        .unwrap()
        .get("optional")
        .is_none());
    activate_publication(&tx, &lock(), 100).unwrap();
    assert_eq!(
        active_collection(&tx, "items")
            .unwrap()
            .unwrap()
            .definition
            .app_ver,
        "2"
    );
    put(&tx, "c_items", "two", "1");
    let page = change_log::read_page(&tx, "items", 1, before.head_seq, 10).unwrap();
    assert_eq!(page.docs[0].app_ver, "2");
}

#[test]
fn populated_compatible_index_requires_online_runner_and_rolls_back() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    let old = definition("items");
    create_collection(&tx, &old).unwrap();
    put(&tx, "c_items", "one", "1");
    reserve_publish(&tx, &lock()).unwrap();
    let mut new = old.clone();
    new.schema_ver = 2;
    new.app_ver = "2".into();
    new.schema_json["indexes"] = json!(["value", ["value", "other"]]);
    update_compatible(&tx, &lock(), &new).unwrap();
    assert!(activate_publication(&tx, &lock(), 100).is_err());
    assert!(
        build_online_index_page(&tx, &lock(), "items", 10)
            .unwrap()
            .complete
    );
    assert_eq!(
        active_collection(&tx, "items").unwrap().unwrap().definition,
        old
    );
    assert!(verify_publish(&tx, &lock()).is_ok());
}

#[test]
fn migration_reopens_at_persisted_cursor_and_switches_only_when_complete() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schema.sqlite");
    let mut db = Connection::open(&path).unwrap();
    let tx = db.transaction().unwrap();
    let old = definition("items");
    create_collection(&tx, &old).unwrap();
    put(&tx, "c_items", "one", "1");
    put(&tx, "c_items", "two", "1");
    reserve_publish(&tx, &lock()).unwrap();
    let prepared = prepare_migration(&tx, &lock(), &next(&old)).unwrap();
    assert_eq!((prepared.source_gen, prepared.gen), (1, 2));
    let page = change_log::read_page(&tx, "items", 1, 0, 1).unwrap();
    let progress =
        apply_migration_page(&tx, &lock(), "items", 0, &page, &outputs(&page), 100).unwrap();
    assert_eq!(progress.state, "backfilling");
    assert!(activate_publication(&tx, &lock(), 100).is_err());
    assert_eq!(active_collection(&tx, "items").unwrap().unwrap().gen, 1);
    tx.commit().unwrap();
    drop(db);
    let mut db = Connection::open(&path).unwrap();
    let tx = db.transaction().unwrap();
    reserve_publish(&tx, &lock()).unwrap();
    let restored = prepare_migration(&tx, &lock(), &next(&old)).unwrap();
    assert_eq!(restored.cursor, progress.cursor);
    let page = change_log::read_page(&tx, "items", 1, restored.cursor, 500).unwrap();
    let ready = apply_migration_page(
        &tx,
        &lock(),
        "items",
        restored.cursor,
        &page,
        &outputs(&page),
        101,
    )
    .unwrap();
    assert_eq!(ready.state, "ready");
    let mark = activate_publication(&tx, &lock(), 101).unwrap();
    let active = active_collection(&tx, "items").unwrap().unwrap();
    assert_eq!((active.gen, active.migrated_at_seq), (2, Some(mark)));
    assert_eq!(count(&tx, "c_items"), 2);
    assert_eq!(count(&tx, "c_items__g2"), 2);
    assert_eq!(count(&tx, "_archive"), 2);
    let old = collection(&tx, "items", 1).unwrap();
    assert_eq!(old.state, "archived");
    let reserve: i64 = tx
        .query_row(
            "SELECT retire_after-archived_at FROM _collections WHERE name='items' AND gen=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(reserve, GENERATION_RESERVE_SECONDS);
    assert!(verify_publish(&tx, &lock()).is_err());
    assert_eq!(
        migration(&tx, &lock().operation_id, "items").unwrap().state,
        "committed"
    );
}

#[test]
fn stale_or_incomplete_source_page_cannot_advance_or_write_a_generation() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    let old = definition("items");
    create_collection(&tx, &old).unwrap();
    put(&tx, "c_items", "one", "1");
    put(&tx, "c_items", "two", "1");
    reserve_publish(&tx, &lock()).unwrap();
    prepare_migration(&tx, &lock(), &next(&old)).unwrap();
    let page = change_log::read_page(&tx, "items", 1, 0, 500).unwrap();
    let mut omitted = page.clone();
    omitted.docs.pop();
    assert!(
        apply_migration_page(&tx, &lock(), "items", 0, &omitted, &outputs(&omitted), 100).is_err()
    );
    put(&tx, "c_items", "one", "2");
    assert!(apply_migration_page(&tx, &lock(), "items", 0, &page, &outputs(&page), 100).is_err());
    assert_eq!(count(&tx, "c_items__g2"), 0);
    assert_eq!(
        migration(&tx, &lock().operation_id, "items")
            .unwrap()
            .cursor,
        0
    );
}

#[test]
fn bad_transformation_rolls_back_prior_rows_archive_and_sequence_in_its_batch() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    let old = definition("items");
    create_collection(&tx, &old).unwrap();
    put(&tx, "c_items", "one", "1");
    put(&tx, "c_items", "two", "1");
    reserve_publish(&tx, &lock()).unwrap();
    prepare_migration(&tx, &lock(), &next(&old)).unwrap();
    let page = change_log::read_page(&tx, "items", 1, 0, 500).unwrap();
    let before = change_log::head(&tx).unwrap();
    let mut mapped = outputs(&page);
    mapped[1] = MigrationOutput::Upsert {
        rev: "bad".into(),
        body: json!(42),
    };
    assert!(apply_migration_page(&tx, &lock(), "items", 0, &page, &mapped, 100).is_err());
    assert_eq!(count(&tx, "c_items__g2"), 0);
    assert_eq!(count(&tx, "_archive"), 0);
    assert_eq!(change_log::head(&tx).unwrap(), before);
}

#[test]
fn concurrent_old_writes_are_cas_guarded_dual_writes_and_force_new_coverage() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    let old = definition("items");
    create_collection(&tx, &old).unwrap();
    put(&tx, "c_items", "one", "1");
    reserve_publish(&tx, &lock()).unwrap();
    prepare_migration(&tx, &lock(), &next(&old)).unwrap();
    let page = change_log::read_page(&tx, "items", 1, 0, 500).unwrap();
    apply_migration_page(&tx, &lock(), "items", 0, &page, &outputs(&page), 100).unwrap();
    let source = SourceWrite {
        id: "one".into(),
        base_rev: Some("1".into()),
        rev: "2".into(),
        body: json!({"value":"new"}),
        deleted: false,
    };
    let target = MigrationOutput::Upsert {
        rev: "m2".into(),
        body: json!({"value":7}),
    };
    dual_write(&tx, &lock(), "items", &source, &target, 101).unwrap();
    assert!(dual_write(&tx, &lock(), "items", &source, &target, 101).is_err());
    assert_eq!(
        change_log::read_page(&tx, "items", 1, 0, 500).unwrap().docs[0].body,
        Some(json!({"value":"new"}))
    );
    assert_eq!(
        change_log::read_page(&tx, "items", 2, 0, 500).unwrap().docs[0].body,
        Some(json!({"value":7}))
    );
    assert!(activate_publication(&tx, &lock(), 101).is_err());
    let saved = migration(&tx, &lock().operation_id, "items").unwrap();
    let page = change_log::read_page(&tx, "items", 1, saved.cursor, 500).unwrap();
    apply_migration_page(
        &tx,
        &lock(),
        "items",
        saved.cursor,
        &page,
        &outputs(&page),
        102,
    )
    .unwrap();
    activate_publication(&tx, &lock(), 102).unwrap();
}

#[test]
fn hard_delete_after_ready_is_not_lost_at_cutover() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    let old = definition("items");
    create_collection(&tx, &old).unwrap();
    put(&tx, "c_items", "one", "1");
    reserve_publish(&tx, &lock()).unwrap();
    prepare_migration(&tx, &lock(), &next(&old)).unwrap();
    let page = change_log::read_page(&tx, "items", 1, 0, 500).unwrap();
    let saved =
        apply_migration_page(&tx, &lock(), "items", 0, &page, &outputs(&page), 100).unwrap();
    tx.execute("DELETE FROM c_items WHERE id='one'", [])
        .unwrap();
    assert!(activate_publication(&tx, &lock(), 100).is_err());
    let page = change_log::read_page(&tx, "items", 1, saved.cursor, 500).unwrap();
    assert_eq!(page.docs[0].op, Operation::Delete);
    apply_migration_page(
        &tx,
        &lock(),
        "items",
        saved.cursor,
        &page,
        &outputs(&page),
        101,
    )
    .unwrap();
    activate_publication(&tx, &lock(), 101).unwrap();
    assert_eq!(
        change_log::read_page(&tx, "items", 2, 0, 500).unwrap().docs[0].op,
        Operation::Delete
    );
}

#[test]
fn publish_lock_survives_reopen_is_exclusive_and_revalidates_base() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    create_collection(&tx, &definition("items")).unwrap();
    reserve_publish(&tx, &lock()).unwrap();
    reserve_publish(&tx, &lock()).unwrap();
    let mut other = lock();
    other.operation_id = "other".into();
    assert!(reserve_publish(&tx, &other)
        .unwrap_err()
        .to_string()
        .contains("busy"));
    tx.execute(
        "UPDATE _schema_apps SET app_ver='changed' WHERE app_id='app'",
        [],
    )
    .unwrap();
    assert!(activate_publication(&tx, &lock(), 100).is_err());
    assert_eq!(active_collection(&tx, "items").unwrap().unwrap().gen, 1);
    assert_eq!(count(&tx, "publish_lock"), 1);
}

#[test]
fn aborted_preparation_preserves_active_data_and_never_reuses_generation() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    let old = definition("items");
    create_collection(&tx, &old).unwrap();
    put(&tx, "c_items", "one", "1");
    reserve_publish(&tx, &lock()).unwrap();
    prepare_migration(&tx, &lock(), &next(&old)).unwrap();
    abort_publication(&tx, &lock(), 100).unwrap();
    assert_eq!(active_collection(&tx, "items").unwrap().unwrap().gen, 1);
    assert_eq!(count(&tx, "c_items"), 1);
    assert_eq!(
        migration(&tx, &lock().operation_id, "items").unwrap().state,
        "aborted"
    );
    let mut later = lock();
    later.operation_id = "later".into();
    reserve_publish(&tx, &later).unwrap();
    assert_eq!(prepare_migration(&tx, &later, &next(&old)).unwrap().gen, 3);
}

#[test]
fn removing_large_collection_archives_its_generation_without_bulk_copy_or_delete() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    create_collection(&tx, &definition("items")).unwrap();
    for i in 0..600 {
        put(&tx, "c_items", &i.to_string(), "1");
    }
    reserve_publish(&tx, &lock()).unwrap();
    archive_collection(&tx, &lock(), "items", 1).unwrap();
    assert!(active_collection(&tx, "items").unwrap().is_some());
    assert_eq!(count(&tx, "_archive"), 0);
    activate_publication(&tx, &lock(), 100).unwrap();
    assert!(active_collection(&tx, "items").unwrap().is_none());
    assert_eq!(count(&tx, "c_items"), 600);
    assert_eq!(count(&tx, "_archive"), 1);
    let metadata: String = tx
        .query_row("SELECT data FROM _archive", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&metadata).unwrap()["table"],
        "c_items"
    );
    assert_eq!(
        purge_archive(&tx, 100 + change_log::RETENTION_SECONDS).unwrap(),
        0
    );
    assert_eq!(
        purge_archive(&tx, 101 + change_log::RETENTION_SECONDS).unwrap(),
        1
    );
}

#[test]
fn new_collection_is_preparing_and_not_active_until_publication() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    initialize(&tx).unwrap();
    let mut first = lock();
    first.base_app_ver = String::new();
    reserve_publish(&tx, &first).unwrap();
    let mut new = definition("fresh");
    new.app_ver = "2".into();
    let saved = prepare_migration(&tx, &first, &new).unwrap();
    assert_eq!(
        (saved.source_gen, saved.gen, saved.state.as_str()),
        (0, 1, "ready")
    );
    assert!(active_collection(&tx, "fresh").unwrap().is_none());
    activate_publication(&tx, &first, 100).unwrap();
    assert_eq!(active_collection(&tx, "fresh").unwrap().unwrap().gen, 1);
}

#[test]
fn schema_classification_rejects_destructive_edits_and_sql_identifier_injection() {
    let old = definition("items");
    assert_eq!(
        classify_change(&old, &next(&old)).unwrap(),
        SchemaChange::Incompatible
    );
    let mut removed = old.clone();
    removed.schema_ver = 2;
    removed.schema_json["properties"] = json!({});
    assert_eq!(
        classify_change(&old, &removed).unwrap(),
        SchemaChange::Incompatible
    );
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    let mut bad = old.clone();
    bad.name = "x; DROP TABLE _seq".into();
    assert!(create_collection(&tx, &bad).is_err());
    let mut bad = old;
    bad.schema_json["indexes"] = json!(["value;SELECT"]);
    assert!(create_collection(&tx, &bad).is_err());
}

#[test]
fn retired_generation_is_deleted_in_bounded_pages_only_after_reserve() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    create_collection(&tx, &definition("items")).unwrap();
    for i in 0..12 {
        put(&tx, "c_items", &i.to_string(), "1");
    }
    reserve_publish(&tx, &lock()).unwrap();
    archive_collection(&tx, &lock(), "items", 100).unwrap();
    activate_publication(&tx, &lock(), 100).unwrap();
    assert!(
        purge_generation_page(&tx, "items", 1, 100 + change_log::RETENTION_SECONDS, 5).is_err()
    );
    assert!(
        !purge_generation_page(&tx, "items", 1, 101 + change_log::RETENTION_SECONDS, 5).unwrap()
    );
    assert_eq!(count(&tx, "c_items"), 7);
    assert!(change_log::read_page(&tx, "items", 1, 0, 10).is_err());
    assert!(
        !purge_generation_page(&tx, "items", 1, 101 + change_log::RETENTION_SECONDS, 5).unwrap()
    );
    assert!(
        purge_generation_page(&tx, "items", 1, 101 + change_log::RETENTION_SECONDS, 5).unwrap()
    );
    assert_eq!(collection(&tx, "items", 1).unwrap().state, "archived");
}

#[test]
fn archived_collection_name_cannot_be_taken_by_a_foreign_app() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    create_collection(&tx, &definition("items")).unwrap();
    reserve_publish(&tx, &lock()).unwrap();
    archive_collection(&tx, &lock(), "items", 100).unwrap();
    activate_publication(&tx, &lock(), 100).unwrap();
    let other = PublishLock {
        app_id: "other".into(),
        base_app_ver: String::new(),
        target_app_ver: "2".into(),
        operation_id: "other".into(),
    };
    reserve_publish(&tx, &other).unwrap();
    let mut def = definition("items");
    def.app_id = "other".into();
    def.app_ver = "2".into();
    assert!(prepare_migration(&tx, &other, &def).is_err());
    assert_eq!(
        collection(&tx, "items", 1).unwrap().definition.app_id,
        "app"
    );
}

#[test]
fn compatible_preparation_survives_reopen_without_exposing_new_schema_and_abort_discards_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("compatible.sqlite");
    let old = definition("items");
    let mut new = old.clone();
    new.schema_ver = 2;
    new.app_ver = "2".into();
    new.schema_json["properties"]["optional"] = json!({"type":"string","default":""});
    let mut db = Connection::open(&path).unwrap();
    let tx = db.transaction().unwrap();
    create_collection(&tx, &old).unwrap();
    reserve_publish(&tx, &lock()).unwrap();
    update_compatible(&tx, &lock(), &new).unwrap();
    assert_eq!(
        active_collection(&tx, "items").unwrap().unwrap().definition,
        old
    );
    assert!(prepare_migration(&tx, &lock(), &next(&old)).is_err());
    tx.commit().unwrap();
    drop(db);
    let mut db = Connection::open(&path).unwrap();
    let tx = db.transaction().unwrap();
    verify_publish(&tx, &lock()).unwrap();
    assert_eq!(count(&tx, "_schema_updates"), 1);
    abort_publication(&tx, &lock(), 100).unwrap();
    assert_eq!(
        active_collection(&tx, "items").unwrap().unwrap().definition,
        old
    );
    assert_eq!(count(&tx, "_schema_updates"), 0);
}

#[test]
fn all_compatible_collections_switch_together_and_failed_cutover_restores_preparation() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    let a = definition("a");
    let b = definition("b");
    create_collection(&tx, &a).unwrap();
    create_collection(&tx, &b).unwrap();
    reserve_publish(&tx, &lock()).unwrap();
    for old in [&a, &b] {
        let mut new = old.clone();
        new.app_ver = "2".into();
        new.client_limit = 2000;
        update_compatible(&tx, &lock(), &new).unwrap();
    }
    assert!(create_collection(&tx, &definition("third")).is_err());
    assert!(archive_collection(&tx, &lock(), "a", 100).is_err());
    tx.execute("UPDATE _collections SET schema_ver=9 WHERE name='b'", [])
        .unwrap();
    assert!(activate_publication(&tx, &lock(), 100).is_err());
    assert_eq!(active_collection(&tx, "a").unwrap().unwrap().definition, a);
    assert_eq!(count(&tx, "_schema_updates"), 2);
    tx.execute("UPDATE _collections SET schema_ver=1 WHERE name='b'", [])
        .unwrap();
    activate_publication(&tx, &lock(), 100).unwrap();
    for name in ["a", "b"] {
        let active = active_collection(&tx, name).unwrap().unwrap();
        assert_eq!(active.definition.app_ver, "2");
        assert_eq!(active.definition.client_limit, 2000);
    }
}

#[test]
fn index_publish_race_does_not_build_an_unbounded_index_or_expose_a_partial_schema() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    let old = definition("items");
    create_collection(&tx, &old).unwrap();
    reserve_publish(&tx, &lock()).unwrap();
    let mut new = old.clone();
    new.schema_ver = 2;
    new.app_ver = "2".into();
    new.schema_json["indexes"] = json!(["value", ["value", "other"]]);
    update_compatible(&tx, &lock(), &new).unwrap();
    put(&tx, "c_items", "race", "1");
    let error = activate_publication(&tx, &lock(), 100)
        .unwrap_err()
        .to_string();
    assert!(error.contains("online_index_build_required"), "{error}");
    assert_eq!(
        active_collection(&tx, "items").unwrap().unwrap().definition,
        old
    );
    assert_eq!(count(&tx, "_schema_updates"), 1);
    verify_publish(&tx, &lock()).unwrap();
}

#[test]
fn stale_publication_base_can_be_aborted_by_its_exact_reservation_owner() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    create_collection(&tx, &definition("items")).unwrap();
    reserve_publish(&tx, &lock()).unwrap();
    tx.execute(
        "UPDATE _schema_apps SET app_ver='new-owner-version' WHERE app_id='app'",
        [],
    )
    .unwrap();
    let mut wrong = lock();
    wrong.operation_id = "wrong-owner".into();
    assert!(abort_publication(&tx, &wrong, 100).is_err());
    assert_eq!(count(&tx, "publish_lock"), 1);
    abort_publication(&tx, &lock(), 100).unwrap();
    assert_eq!(count(&tx, "publish_lock"), 0);
    assert_eq!(active_collection(&tx, "items").unwrap().unwrap().gen, 1);
}

#[test]
fn populated_aborted_generation_is_revoked_then_retired_in_bounded_pages() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    let old = definition("items");
    create_collection(&tx, &old).unwrap();
    put(&tx, "c_items", "one", "1");
    put(&tx, "c_items", "two", "1");
    reserve_publish(&tx, &lock()).unwrap();
    prepare_migration(&tx, &lock(), &next(&old)).unwrap();
    let page = change_log::read_page(&tx, "items", 1, 0, 500).unwrap();
    apply_migration_page(&tx, &lock(), "items", 0, &page, &outputs(&page), 100).unwrap();
    abort_publication(&tx, &lock(), 100).unwrap();
    assert_eq!(active_collection(&tx, "items").unwrap().unwrap().gen, 1);
    assert_eq!(count(&tx, "c_items__g2"), 2);
    assert!(change_log::read_page(&tx, "items", 2, 0, 500).is_err());
    assert!(!purge_generation_page(&tx, "items", 2, 101, 1).unwrap());
    assert!(purge_generation_page(&tx, "items", 2, 101, 1).unwrap());
}

#[test]
fn zero_based_schema_versions_are_supported_independently_of_generation() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    let mut def = definition("items");
    def.schema_ver = 0;
    let created = create_collection(&tx, &def).unwrap();
    assert_eq!((created.gen, created.definition.schema_ver), (1, 0));
}
