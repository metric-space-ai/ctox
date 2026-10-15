
#[test]
fn invalid_column_binding_cannot_replace_a_working_trigger_or_read_source() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    table(&tx, "a", 1);
    let valid = TableBinding::generic("a", 1, "c_a__g1", 1, "1");
    for is_data in [false, true] {
        let mut bad = valid.clone();
        if is_data { bad.data_column = "missing_data".into(); }
        else { bad.rev_column = "missing_rev".into(); }
        assert!(install_triggers(&tx, &bad).is_err());
    }
    tx.execute("INSERT INTO c_a__g1 VALUES('ok','r',1,'{}',0)", []).unwrap();
    let page = read_page(&tx, "a", 1, 0, 10).unwrap();
    assert_eq!(page.docs.len(), 1);
    assert_eq!(page.docs[0].id, "ok");
    assert_eq!(page.docs[0].rev, "r");
    assert_eq!(page.docs[0].body, Some(json!({})));
    assert_eq!(page.head_seq, 1);
}
use super::*;
use rusqlite::Connection;
use serde_json::json;

fn table(tx: &Transaction<'_>, name: &str, gen: u64) {
    let physical = format!("c_{name}__g{gen}");
    tx.execute_batch(&format!(
        "CREATE TABLE {physical}(
        id TEXT PRIMARY KEY,rev TEXT NOT NULL,schema_ver INTEGER NOT NULL,
        data TEXT NOT NULL,deleted INTEGER NOT NULL DEFAULT 0)"
    ))
    .unwrap();
    install_triggers(tx, &TableBinding::generic(name, gen, &physical, 1, "1.0")).unwrap();
}
fn put(tx: &Transaction<'_>, table: &str, id: &str, rev: &str) {
    tx.execute(
        &format!(
            "INSERT INTO {table}(id,rev,schema_ver,data,deleted) VALUES(?1,?2,1,?3,0)
        ON CONFLICT(id) DO UPDATE SET rev=excluded.rev,data=excluded.data,deleted=0"
        ),
        params![id, rev, json!({"id":id,"rev":rev}).to_string()],
    )
    .unwrap();
}

#[test]
fn sequence_is_global_compacted_and_transactional_with_hard_deletes() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    let first = initialize(&tx).unwrap();
    assert!(Uuid::parse_str(&first.epoch).is_ok());
    table(&tx, "a", 1);
    table(&tx, "b", 1);
    put(&tx, "c_a__g1", "x", "1");
    put(&tx, "c_b__g1", "x", "1");
    put(&tx, "c_a__g1", "x", "2");
    tx.execute("DELETE FROM c_b__g1 WHERE id='x'", []).unwrap();
    assert_eq!(head(&tx).unwrap().head_seq, 4);
    let a = read_page(&tx, "a", 1, 0, 10).unwrap();
    assert_eq!(a.docs.len(), 1);
    assert_eq!(a.docs[0].seq, 3);
    assert_eq!(a.docs[0].rev, "2");
    assert_eq!(a.cursor_to, 4);
    let b = read_page(&tx, "b", 1, 0, 10).unwrap();
    assert_eq!(b.docs[0].op, Operation::Delete);
    assert_eq!(b.docs[0].body, None);
    assert_eq!(b.docs[0].seq, 4);
    tx.commit().unwrap();
    let tx = db.transaction().unwrap();
    assert_eq!(initialize(&tx).unwrap().epoch, first.epoch);
    put(&tx, "c_a__g1", "y", "1");
    tx.rollback().unwrap();
    let tx = db.transaction().unwrap();
    assert_eq!(head(&tx).unwrap().head_seq, 4);
    assert_eq!(read_page(&tx, "a", 1, 0, 10).unwrap().docs.len(), 1);
}

#[test]
fn empty_ranges_and_exact_pages_have_snapshot_progress_and_indexed_backlog() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    initialize(&tx).unwrap();
    table(&tx, "a", 1);
    table(&tx, "b", 1);
    put(&tx, "c_a__g1", "a", "1");
    put(&tx, "c_a__g1", "b", "1");
    put(&tx, "c_b__g1", "c", "1");
    let first = read_page(&tx, "a", 1, 0, 1).unwrap();
    assert_eq!((first.cursor_to, first.head_seq, first.backlog), (1, 3, 1));
    let second = read_page(&tx, "a", 1, 1, 1).unwrap();
    assert_eq!((second.cursor_to, second.backlog), (2, 0));
    let empty = read_page(&tx, "a", 1, 2, 1).unwrap();
    assert!(empty.docs.is_empty());
    assert_eq!((empty.cursor_to, empty.head_seq, empty.backlog), (3, 3, 0));
    assert!(read_page(&tx, "a", 1, 4, 1).is_err());
    assert!(read_page(&tx, "a", 1, 0, 0).is_err());
    let wire = serde_json::to_value(&first).unwrap();
    assert_eq!(wire["cursor_to"], "1");
    assert_eq!(wire["docs"][0]["seq"], "1");
    let mut bad = wire.clone();
    bad["head_seq"] = json!(3);
    assert!(serde_json::from_value::<ChangePage>(bad).is_err());
}

#[test]
fn two_generations_and_primary_key_renames_do_not_lose_old_ids() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    initialize(&tx).unwrap();
    table(&tx, "a", 1);
    table(&tx, "a", 2);
    put(&tx, "c_a__g1", "same", "old");
    put(&tx, "c_a__g2", "same", "new");
    assert_eq!(read_page(&tx, "a", 1, 0, 10).unwrap().docs[0].rev, "old");
    assert_eq!(read_page(&tx, "a", 2, 0, 10).unwrap().docs[0].rev, "new");
    tx.execute(
        "UPDATE c_a__g1 SET id='renamed',rev='2' WHERE id='same'",
        [],
    )
    .unwrap();
    let page = read_page(&tx, "a", 1, 0, 10).unwrap();
    assert_eq!(page.docs.len(), 2);
    assert_eq!(
        (&page.docs[0].id, page.docs[0].op),
        (&"same".to_string(), Operation::Delete)
    );
    assert_eq!(
        (&page.docs[1].id, page.docs[1].op),
        (&"renamed".to_string(), Operation::Upsert)
    );
    assert!(page.docs[0].seq < page.docs[1].seq);
}

#[test]
fn bounded_legacy_backfill_is_resumable_and_preserves_newer_triggered_rows() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    initialize(&tx).unwrap();
    tx.execute_batch(
        "CREATE TABLE legacy(id TEXT PRIMARY KEY,revision TEXT,data TEXT,deleted INTEGER);
      INSERT INTO legacy VALUES('a','old','{}',0),('b','old','{}',0),('c','old','{}',1)",
    )
    .unwrap();
    let mut source = TableBinding::generic("legacy", 1, "legacy", 7, "2.0");
    source.rev_column = "revision".into();
    source.schema_column = None;
    install_triggers(&tx, &source).unwrap();
    let first = backfill_page(&tx, "legacy", 1, None, 1).unwrap();
    assert_eq!(first.cursor.as_deref(), Some("a"));
    assert!(!first.complete);
    tx.execute("UPDATE legacy SET revision='new' WHERE id='b'", [])
        .unwrap();
    let seq = head(&tx).unwrap().head_seq;
    let next = backfill_page(&tx, "legacy", 1, first.cursor.as_deref(), 2).unwrap();
    assert_eq!(next.processed, 2);
    let done = backfill_page(&tx, "legacy", 1, next.cursor.as_deref(), 2).unwrap();
    assert!(done.complete);
    let page = read_page(&tx, "legacy", 1, 0, 10).unwrap();
    let b = page.docs.iter().find(|d| d.id == "b").unwrap();
    assert_eq!((b.rev.as_str(), b.seq, b.schema_ver), ("new", seq, 7));
    let before = head(&tx).unwrap();
    backfill_page(&tx, "legacy", 1, None, 100).unwrap();
    assert_eq!(head(&tx).unwrap(), before);
}

#[test]
fn read_page_uses_one_wal_snapshot_for_head_changes_and_documents() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("snapshot.sqlite");
    let mut reader = Connection::open(&path).unwrap();
    reader.pragma_update(None, "journal_mode", "WAL").unwrap();
    let mut writer = Connection::open(&path).unwrap();
    let tx = writer.transaction().unwrap();
    initialize(&tx).unwrap();
    table(&tx, "a", 1);
    put(&tx, "c_a__g1", "a", "old");
    tx.commit().unwrap();
    let snapshot = reader.transaction().unwrap();
    assert_eq!(head(&snapshot).unwrap().head_seq, 1);
    let tx = writer.transaction().unwrap();
    put(&tx, "c_a__g1", "a", "new");
    tx.commit().unwrap();
    let page = read_page(&snapshot, "a", 1, 0, 10).unwrap();
    assert_eq!((page.head_seq, page.docs[0].rev.as_str()), (1, "old"));
    snapshot.commit().unwrap();
    let snapshot = reader.transaction().unwrap();
    let page = read_page(&snapshot, "a", 1, 1, 10).unwrap();
    assert_eq!((page.head_seq, page.docs[0].rev.as_str()), (2, "new"));
}

#[test]
fn tombstone_retention_rotates_epoch_and_rejects_expired_nonzero_cursors() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    initialize(&tx).unwrap();
    table(&tx, "a", 1);
    put(&tx, "c_a__g1", "old", "1");
    tx.execute("DELETE FROM c_a__g1 WHERE id='old'", [])
        .unwrap();
    put(&tx, "c_a__g1", "fresh", "1");
    tx.execute("DELETE FROM c_a__g1 WHERE id='fresh'", [])
        .unwrap();
    tx.execute("UPDATE _changes SET deleted_at=100 WHERE id='old'", [])
        .unwrap();
    let before = head(&tx).unwrap();
    assert_eq!(purge_tombstones(&tx, 100 + RETENTION_SECONDS).unwrap(), 0);
    assert_eq!(head(&tx).unwrap(), before);
    assert_eq!(purge_tombstones(&tx, 101 + RETENTION_SECONDS).unwrap(), 1);
    let after = head(&tx).unwrap();
    assert_ne!(after.epoch, before.epoch);
    assert_eq!((after.head_seq, after.horizon_seq), (4, 2));
    assert!(read_page(&tx, "a", 1, 1, 10).is_err());
    assert_eq!(read_page(&tx, "a", 1, 0, 10).unwrap().docs.len(), 1);
}

#[test]
fn sequence_overflow_cannot_publish_a_write_and_invalid_identifiers_do_no_ddl() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    initialize(&tx).unwrap();
    table(&tx, "a", 1);
    tx.execute("UPDATE _seq SET v=?1", [i64::MAX]).unwrap();
    assert!(tx
        .execute("INSERT INTO c_a__g1 VALUES('x','r',1,'{}',0)", [])
        .is_err());
    let count: i64 = tx
        .query_row("SELECT COUNT(*) FROM c_a__g1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(head(&tx).unwrap().head_seq, i64::MAX as u64);
    let source = TableBinding::generic("bad", 1, "bad; DROP TABLE _seq", 1, "1");
    assert!(install_triggers(&tx, &source).is_err());
    assert_eq!(head(&tx).unwrap().head_seq, i64::MAX as u64);
}

/// Explicit measurement, never an assertion that noisy timing met the product budget.
#[test]
#[ignore = "opt-in trigger overhead measurement; run with --ignored --nocapture"]
fn measure_trigger_overhead() {
    use std::time::Instant;
    let mut samples = [Vec::new(), Vec::new()];
    let payload = json!({"content":"x".repeat(4096)}).to_string();
    for trial in 0..6 {
        for mode in [trial % 2, 1 - trial % 2] {
            let mut db = Connection::open_in_memory().unwrap();
            let tx = db.transaction().unwrap();
            tx.execute_batch("CREATE TABLE c_measure__g1(id TEXT PRIMARY KEY,rev TEXT,schema_ver INTEGER,data TEXT,deleted INTEGER)").unwrap();
            if mode == 1 {
                install_triggers(
                    &tx,
                    &TableBinding::generic("measure", 1, "c_measure__g1", 1, "1"),
                )
                .unwrap();
            }
            let started = Instant::now();
            {
                let mut insert = tx
                    .prepare("INSERT INTO c_measure__g1 VALUES(?1,'1',1,?2,0)")
                    .unwrap();
                for id in 0..10_000 {
                    insert.execute(params![id.to_string(), payload]).unwrap();
                }
                let mut update = tx
                    .prepare("UPDATE c_measure__g1 SET rev='2' WHERE id=?1")
                    .unwrap();
                for id in 0..10_000 {
                    update.execute([id.to_string()]).unwrap();
                }
            }
            tx.commit().unwrap();
            samples[mode].push(started.elapsed().as_secs_f64());
        }
    }
    for values in &mut samples {
        values.sort_by(f64::total_cmp);
    }
    let baseline = (samples[0][2] + samples[0][3]) / 2.0;
    let triggered = (samples[1][2] + samples[1][3]) / 2.0;
    let overhead = (triggered / baseline - 1.0) * 100.0;
    println!(
        "TRIGGER_OVERHEAD {}",
        json!({"rows":10000,"mutations":20000,"payload_bytes":payload.len(),
        "trials":6,"baseline_median_seconds":baseline,"triggered_median_seconds":triggered,
        "overhead_percent":overhead,"target_percent":15,"target_met":overhead<=15.0,
        "scope":"SQLite in-memory component; not installed M2 writer/WAL acceptance"})
    );
}
