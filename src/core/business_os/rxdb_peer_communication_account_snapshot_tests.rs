use super::*;

fn seed_accounts(root: &Path, count: usize) {
    channels::list_communication_accounts_for_business_os(root).expect("initialize channel schema");
    let mut conn = Connection::open(crate::paths::core_db(root)).expect("open core DB");
    let transaction = conn.transaction().expect("seed transaction");
    for index in 0..count {
        transaction.execute(
            "INSERT INTO communication_accounts (account_key, channel, address, provider, profile_json, created_at, updated_at) VALUES (?1, 'tui', ?1, 'local', '{}', '2026-05-20T08:00:00Z', '2026-05-20T08:00:00Z')",
            params![format!("a-tui:{index:05}")],
        ).expect("seed non-mail account");
    }
    transaction.execute(
        "INSERT INTO communication_accounts (account_key, channel, address, provider, profile_json, created_at, updated_at) VALUES ('email:mailbox@example.test', 'email', 'mailbox@example.test', 'ews', '{}', '2026-05-21T08:00:00Z', '2026-05-21T08:00:00Z')",
        [],
    ).expect("seed newest mailbox beyond the first pull page");
    transaction.commit().expect("commit account seed");
}

fn projected_mailbox(root: &Path) -> Value {
    let conn = Connection::open(store::rxdb_store_path(root)).expect("open projection");
    let raw: String = conn.query_row(
        "SELECT data FROM ctox_business_os__communication_accounts__v0 WHERE json_extract(data, '$.account_key') = 'email:mailbox@example.test'",
        [], |row| row.get(0),
    ).expect("projected mailbox");
    serde_json::from_str(&raw).expect("mailbox JSON")
}

#[test]
fn native_account_snapshot_contains_mailbox_after_first_2000_accounts() {
    let root = tempfile::tempdir().expect("isolated root");
    seed_accounts(root.path(), 2_000);
    let projection = load_channel_state_projection(root.path()).expect("complete snapshot");
    assert_eq!(projection.accounts.len(), 2_001);
    assert!(projection
        .accounts
        .iter()
        .any(|account| account["account_key"] == "email:mailbox@example.test"));
    // The public pull contract remains bounded for actual pagination clients.
    let page =
        channels::pull_communication_accounts_for_business_os(root.path(), Some(0), Some(2_000))
            .expect("bounded pull");
    assert_eq!(page["count"], 2_000);
}

#[test]
fn native_account_projection_keeps_late_mailbox_and_tombstones_real_disconnect() {
    let root = tempfile::tempdir().expect("isolated root");
    seed_accounts(root.path(), 2_000);
    sync_channel_state(root.path()).expect("initial complete projection");
    assert_eq!(projected_mailbox(root.path())["is_deleted"], false);
    sync_channel_state(root.path()).expect("repeat projection");
    assert_eq!(projected_mailbox(root.path())["is_deleted"], false);
    channels::disconnect_communication_account_for_business_os(
        root.path(),
        "email:mailbox@example.test",
    )
    .expect("real disconnect");
    sync_channel_state(root.path()).expect("project disconnect beyond the first target page");
    let mailbox = projected_mailbox(root.path());
    assert_eq!(mailbox["is_deleted"], true);
    assert_eq!(mailbox["status"], "disconnected");
}

#[test]
fn native_account_projection_source_error_does_not_discard_last_snapshot() {
    let root = tempfile::tempdir().expect("isolated root");
    seed_accounts(root.path(), 0);
    sync_channel_state(root.path()).expect("initial projection");
    let before = projected_mailbox(root.path());
    // Fault injection is confined to this test fixture, never a customer DB.
    fs::write(crate::paths::core_db(root.path()), b"not a SQLite database")
        .expect("inject unreadable source");
    assert!(sync_channel_state(root.path()).is_err());
    assert_eq!(projected_mailbox(root.path()), before);
}

#[test]
fn native_account_snapshot_missing_database_is_unavailable_without_creating_source() {
    let root = tempfile::tempdir().expect("isolated root");
    let path = crate::paths::core_db(root.path());
    assert!(!path.exists());
    let error = channels::snapshot_communication_accounts_for_business_os(root.path())
        .expect_err("missing database is not an authoritative empty snapshot");
    assert!(error.to_string().contains("channel database missing"));
    assert!(
        !path.exists(),
        "read-only snapshot must not initialize the source"
    );
}

#[test]
fn native_account_projection_missing_database_preserves_existing_mailbox() {
    let root = tempfile::tempdir().expect("isolated root");
    seed_accounts(root.path(), 0);
    sync_channel_state(root.path()).expect("initial projection");
    let before = projected_mailbox(root.path());
    let path = crate::paths::core_db(root.path());
    let saved = root.path().join("saved-channel-source.sqlite3");
    fs::rename(&path, &saved).expect("temporarily remove isolated source");
    let error =
        sync_channel_state(root.path()).expect_err("missing source must stop reconciliation");
    assert!(format!("{error:#}").contains("channel database missing"));
    assert_eq!(projected_mailbox(root.path()), before);
    assert!(
        !path.exists(),
        "failed projection must not replace the source"
    );
    fs::rename(saved, path).expect("restore isolated source");
    sync_channel_state(root.path()).expect("reconcile after source returns");
    assert_eq!(projected_mailbox(root.path())["is_deleted"], false);
}

#[test]
fn native_account_projection_missing_table_preserves_existing_mailbox() {
    let root = tempfile::tempdir().expect("isolated root");
    seed_accounts(root.path(), 0);
    sync_channel_state(root.path()).expect("initial projection");
    let before = projected_mailbox(root.path());
    let conn = Connection::open(crate::paths::core_db(root.path())).expect("isolated source");
    conn.execute("DROP TABLE communication_accounts", [])
        .expect("remove source table");
    drop(conn);
    let error =
        sync_channel_state(root.path()).expect_err("missing table must stop reconciliation");
    assert!(format!("{error:#}").contains("communication_accounts table missing"));
    assert_eq!(projected_mailbox(root.path()), before);
}

#[test]
fn native_account_projection_authoritative_empty_table_tombstones_removed_mailbox() {
    let root = tempfile::tempdir().expect("isolated root");
    seed_accounts(root.path(), 0);
    sync_channel_state(root.path()).expect("initial projection");
    let conn = Connection::open(crate::paths::core_db(root.path())).expect("isolated source");
    conn.execute("DELETE FROM communication_accounts", [])
        .expect("empty existing source");
    drop(conn);
    assert!(
        channels::snapshot_communication_accounts_for_business_os(root.path())
            .expect("existing empty source is authoritative")
            .is_empty()
    );
    sync_channel_state(root.path()).expect("reconcile authoritative removal");
    assert_eq!(projected_mailbox(root.path())["is_deleted"], true);
}
