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
    assert!(AdmissionBudget::new(
        NonZeroUsize::new(1025).unwrap(),
        Duration::from_secs(1),
        Duration::from_secs(1)
    )
    .is_none());
    assert!(AdmissionBudget::new(capacity, Duration::MAX, Duration::from_secs(1)).is_none());
}

#[test]
fn v4_all_canonical_database_helpers_share_one_state_root() {
    let root = std::path::Path::new("install");
    let state = paths::runtime_dir(root);
    for (path, name) in [
        (paths::core_db(root), "ctox.sqlite3"),
        (paths::mission_db(root), "ctox.sqlite3"),
        (paths::lcm_db(root), "ctox.sqlite3"),
        (paths::sync_db(root), "business-os-rxdb.sqlite3"),
        (paths::business_os_db(root), "business-os.sqlite3"),
        (paths::runtime_config_db(root), "ctox-runtime.sqlite3"),
        (paths::secrets_db(root), "ctox-secrets.sqlite3"),
        (paths::office_db(root), "ctox-office.sqlite3"),
        (paths::maintenance_db(root), "ctox-maintenance.sqlite3"),
        (paths::legacy_mission_db(root), "cto_agent.db"),
        (paths::legacy_lcm_db(root), "ctox_lcm.db"),
        (paths::scrape_db(root), "ctox_scraping.db"),
        (paths::ticket_local_db(root), "ticket_local.db"),
        (paths::backup_dir(root), "backup"),
    ] {
        assert_eq!(path, state.join(name));
    }
}
