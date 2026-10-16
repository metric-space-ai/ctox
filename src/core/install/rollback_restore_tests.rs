//! Real file/SQLite/TLS boundary contracts for the updater, no substitute store.
use super::*;

#[test]
fn rollback_probe_retires_cached_core_handles() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("runtime");
    rollback_probe_closed(|| {
        channels::count_queue_tasks(root.path(), &[])?;
        channels::list_queue_tasks(root.path(), &["leased".to_owned()], 32)?;
        // This proves the fixture exercises the cached-handle defect itself.
        anyhow::ensure!(ensure_no_restore_handles(&state).is_err(), "probe did not retain a Core handle");
        Ok(())
    }).unwrap();
    ensure_no_restore_handles(&state).expect("joining the probe must retire its TLS connections");
}

#[test]
fn rollback_restore_waits_and_receipt_follows_verified_restore() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("runtime");
    let backup = root.path().join("backup");
    fs::create_dir_all(&state).unwrap(); fs::create_dir_all(&backup).unwrap();
    let layout = InstallLayout { workspace_root: root.path().into(), active_root: root.path().into(),
        install_root: None, state_root: state.clone(), cache_root: root.path().join("cache") };
    let source = rusqlite::Connection::open(backup.join("ctox.sqlite3")).unwrap();
    source.execute_batch("CREATE TABLE witness(value INTEGER); INSERT INTO witness VALUES(1);").unwrap();
    drop(source);
    // A historical terminal receipt must never replace the live operation.
    fs::write(backup.join(UPDATE_STATE_FILE_NAME), serde_json::to_vec(&json!({
        "schema_version":1,"phase":"rolled_back","current_version":"historical"})).unwrap()).unwrap();
    let live = rusqlite::Connection::open(state.join("ctox.sqlite3")).unwrap();
    live.execute_batch("CREATE TABLE witness(value INTEGER); INSERT INTO witness VALUES(99); BEGIN EXCLUSIVE;").unwrap();
    let rejected = finish_rollback_receipt(&layout, || {
        assert_eq!(load_update_state(&layout.update_state_path())?.unwrap().phase, "rolling_back");
        restore_rollback_state(&backup, &state, Duration::from_millis(5))
    });
    assert!(rejected.is_err(), "a live DB reader/writer must prevent restore");
    assert_eq!(live.query_row("SELECT value FROM witness", [], |r| r.get::<_,i64>(0)).unwrap(),99,
        "restore must not overwrite the held database before the failure");
    let failed = load_update_state(&layout.update_state_path()).unwrap().unwrap();
    assert_eq!(failed.phase,"failed"); assert!(failed.last_error.is_some());
    drop(live);
    let reopened = rusqlite::Connection::open_with_flags(state.join("ctox.sqlite3"),rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(reopened.query_row("SELECT value FROM witness", [], |r| r.get::<_,i64>(0)).unwrap(),99,
        "failed custody must preserve the on-disk state, not just a held SQLite page cache");
    drop(reopened);
    finish_rollback_receipt(&layout, || {
        assert_eq!(load_update_state(&layout.update_state_path())?.unwrap().phase, "rolling_back");
        restore_rollback_state(&backup, &state, Duration::from_secs(1))?;
        let db=rusqlite::Connection::open_with_flags(state.join("ctox.sqlite3"),rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        anyhow::ensure!(db.query_row("SELECT value FROM witness", [], |r| r.get::<_,i64>(0))?==1,"restore readback differs");
        assert_ne!(load_update_state(&layout.update_state_path())?.unwrap().phase,"rolled_back");
        Ok(())
    }).unwrap();
    let complete=load_update_state(&layout.update_state_path()).unwrap().unwrap();
    assert_eq!(complete.phase,"rolled_back"); assert!(complete.last_error.is_none());
}
