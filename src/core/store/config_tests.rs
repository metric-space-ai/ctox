use super::*;
use rusqlite::{Connection, OpenFlags};

// Exercise the canonical path module without linking the daemon and its model crates.
#[path = "../paths.rs"]
mod paths;

fn integer(connection: &Connection, name: &str) -> i64 {
    connection
        .pragma_query_value(None, name, |row| row.get(0))
        .unwrap()
}

#[test]
fn v4_pragmas_apply_to_each_writer_and_reader() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("policy.sqlite3");
    let config = ConnectionConfig::default();
    for _ in 0..2 {
        let writer = Connection::open(&path).unwrap();
        config.apply_writer(&writer).unwrap();
        writer
            .execute_batch("CREATE TABLE IF NOT EXISTS sample(id INTEGER PRIMARY KEY)")
            .unwrap();
        let reader = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        config.apply_reader(&reader).unwrap();
        for connection in [&writer, &reader] {
            assert_eq!(integer(connection, "synchronous"), 1);
            assert_eq!(integer(connection, "busy_timeout"), 0);
            assert_eq!(integer(connection, "foreign_keys"), 1);
            assert_eq!(integer(connection, "journal_size_limit"), 64 * 1024 * 1024);
            assert_eq!(integer(connection, "wal_autocheckpoint"), 0);
        }
        assert_eq!(integer(&reader, "query_only"), 1);
        assert!(reader.execute("INSERT INTO sample VALUES (1)", []).is_err());
    }
}

#[test]
fn v4_reader_rejects_legacy_journal_without_mutating_it() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("legacy.sqlite3");
    let writer = Connection::open(&path).unwrap();
    writer
        .execute_batch("CREATE TABLE legacy(id INTEGER)")
        .unwrap();
    let reader = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert!(ConnectionConfig::default().apply_reader(&reader).is_err());
    let mode: String = writer
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "delete");
}

#[test]
fn v4_admission_rejects_unbounded_time_budgets() {
    let capacity = NonZeroUsize::new(4).unwrap();
    assert!(AdmissionBudget::new(capacity, Duration::ZERO, Duration::from_secs(1)).is_none());
    assert!(AdmissionBudget::new(capacity, Duration::from_secs(1), Duration::ZERO).is_none());
    assert_eq!(AdmissionBudget::default().capacity.get(), 1024);
}
