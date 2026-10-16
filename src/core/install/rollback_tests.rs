use super::*;
use tempfile::tempdir;

#[test]
fn restore_state_backup_keeps_live_lease_and_restores_tenant_data() -> Result<()> {
    let temp = tempdir()?;
    let layout = InstallLayout {
        workspace_root: temp.path().to_path_buf(),
        active_root: temp.path().to_path_buf(),
        install_root: None,
        state_root: temp.path().join("state"),
        cache_root: temp.path().join("cache"),
    };
    let old = begin_maintenance(&layout, "old-update")?;
    let database = layout.state_root.join("tenant.sqlite3");
    let db = rusqlite::Connection::open(&database)?;
    db.execute_batch("CREATE TABLE probe (value TEXT); INSERT INTO probe VALUES ('before');")?;
    drop(db);
    let backup = backup_state_root(&layout.state_root)?;
    let live = begin_maintenance(&layout, "rollback")?;
    assert_ne!(old.lease_id, live.lease_id);
    let db = rusqlite::Connection::open(&database)?;
    db.execute("UPDATE probe SET value='after'", [])?;
    drop(db);
    let control = rusqlite::Connection::open(layout.state_root.join(MAINTENANCE_STORE_FILE_NAME))?;
    control.pragma_update(None, "journal_mode", "WAL")?;
    // Keep a real WAL reader open across restore, as the heartbeat may be.
    control.execute_batch("BEGIN; SELECT * FROM ctox_maintenance_state;")?;
    restore_state_backup(&backup, &layout.state_root)?;
    control.execute_batch("COMMIT;")?;
    let finished = update_maintenance(&layout.state_root, &live.lease_id, |state| {
        state.phase = "rolled_back".into();
        state.status = "rolled_back".into();
    })?;
    assert_eq!(finished.lease_id, live.lease_id);
    assert_eq!(finished.phase, "rolled_back");
    let value: String =
        rusqlite::Connection::open(&database)?
            .query_row("SELECT value FROM probe", [], |row| row.get(0))?;
    assert_eq!(value, "before", "tenant data must actually be restored");
    let failed = begin_maintenance(&layout, "rollback-missing-backup")?;
    let error = restore_state_backup(&temp.path().join("missing"), &layout.state_root)
        .expect_err("a missing backup must remain a real failure");
    assert!(error.to_string().contains("state backup not found"));
    fail_maintenance(&layout.state_root, &failed.lease_id, &error.to_string())?;
    let failure = load_maintenance_state_from_root(&layout.state_root)?.unwrap();
    assert_eq!(failure.lease_id, failed.lease_id);
    assert_eq!(failure.phase, "failed");
    assert!(failure
        .last_error
        .unwrap()
        .contains("state backup not found"));
    Ok(())
}
