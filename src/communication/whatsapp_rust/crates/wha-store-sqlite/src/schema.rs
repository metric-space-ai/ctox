//! Schema migrations for the SQLite-backed store.
//!
//! Ports the relevant `CREATE TABLE` statements from
//! `_upstream/whatsmeow/store/sqlstore/upgrades/00-latest-schema.sql`.
//! Column names are simplified slightly to match the simpler trait surface
//! exposed by `wha-store::traits` (no per-our-jid scoping; the Rust types
//! already imply a single device per store).
//!
//! Migrations are applied idempotently from [`apply`]. New revisions get
//! appended to [`MIGRATIONS`] and the schema version is bumped — older
//! databases pick up the diff automatically on the next [`SqliteStore::open`].
//!
//! [`SqliteStore::open`]: crate::SqliteStore::open

use rusqlite::{Connection, Transaction};

/// Bumped each time a new statement is appended to [`MIGRATIONS`].
pub const SCHEMA_VERSION: i64 = 6;

/// Each entry is a chunk of SQL applied as a single transaction. Adding a new
/// migration is "append a new chunk + bump [`SCHEMA_VERSION`]".
pub const MIGRATIONS: &[&str] = &[
    // v1 — initial layout (mirrors whatsmeow's 00-latest-schema.sql).
    r#"
    CREATE TABLE IF NOT EXISTS whatsmeow_device (
        jid                TEXT PRIMARY KEY,
        lid                TEXT,
        registration_id    INTEGER NOT NULL,
        identity_key_pub   BLOB NOT NULL,
        identity_key_priv  BLOB NOT NULL,
        signed_pre_key_id  INTEGER NOT NULL,
        signed_pre_key_pub BLOB NOT NULL,
        signed_pre_key_priv BLOB NOT NULL,
        signed_pre_key_sig BLOB NOT NULL,
        noise_key_pub      BLOB NOT NULL,
        noise_key_priv     BLOB NOT NULL,
        adv_key            BLOB NOT NULL,
        platform           TEXT NOT NULL DEFAULT '',
        business_name      TEXT NOT NULL DEFAULT '',
        push_name          TEXT NOT NULL DEFAULT ''
    );

    CREATE TABLE IF NOT EXISTS whatsmeow_identity_keys (
        their_id TEXT PRIMARY KEY,
        identity BLOB NOT NULL
    );

    CREATE TABLE IF NOT EXISTS whatsmeow_sessions (
        their_id TEXT PRIMARY KEY,
        session  BLOB NOT NULL
    );

    CREATE TABLE IF NOT EXISTS whatsmeow_pre_keys (
        key_id   INTEGER PRIMARY KEY,
        key      BLOB    NOT NULL,
        uploaded INTEGER NOT NULL DEFAULT 0
    );

    CREATE TABLE IF NOT EXISTS whatsmeow_sender_keys (
        chat_id    TEXT NOT NULL,
        sender_id  TEXT NOT NULL,
        sender_key BLOB NOT NULL,
        PRIMARY KEY (chat_id, sender_id)
    );

    CREATE TABLE IF NOT EXISTS whatsmeow_app_state_sync_keys (
        key_id      BLOB PRIMARY KEY,
        key_data    BLOB    NOT NULL,
        fingerprint BLOB    NOT NULL,
        timestamp   INTEGER NOT NULL,
        inserted_at INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS whatsmeow_app_state_version (
        name    TEXT PRIMARY KEY,
        version INTEGER NOT NULL,
        hash    BLOB    NOT NULL
    );

    CREATE TABLE IF NOT EXISTS whatsmeow_app_state_mutation_macs (
        name      TEXT NOT NULL,
        version   INTEGER NOT NULL,
        index_mac BLOB NOT NULL,
        value_mac BLOB NOT NULL,
        PRIMARY KEY (name, version, index_mac)
    );

    CREATE TABLE IF NOT EXISTS whatsmeow_lid_map (
        lid TEXT PRIMARY KEY,
        pn  TEXT UNIQUE NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_whatsmeow_lid_map_pn ON whatsmeow_lid_map(pn);
    "#,
    // v2 — single-row `device` table holding the `wha-store::persist` blob.
    // Persists the device's identity material across restarts so the example
    // binaries (and any production caller) can resume a paired session
    // without re-running the QR flow. The CHECK keeps it strictly singleton.
    r#"
    CREATE TABLE IF NOT EXISTS device (
        id   INTEGER PRIMARY KEY CHECK(id = 1),
        blob BLOB NOT NULL
    );
    "#,
    // v3 — message-secrets table (`whatsmeow_message_secrets` upstream).
    // Maps a (chat, sender, msg_id) triple → 32-byte master secret. Upstream
    // also has an `our_jid` column for multi-device per-DB scoping; our
    // store is single-device per file, so it's omitted.
    r#"
    CREATE TABLE IF NOT EXISTS msg_secrets (
        chat   TEXT NOT NULL,
        sender TEXT NOT NULL,
        msg_id TEXT NOT NULL,
        secret BLOB NOT NULL,
        PRIMARY KEY (chat, sender, msg_id)
    );
    "#,
    // v4 — privacy-token store. Mirrors `store.PrivacyTokenStore` upstream
    // (`_upstream/whatsmeow/store/sqlstore/store.go::PutPrivacyTokens`) —
    // upserts replace prior rows, the timestamp comes from the originating
    // `<notification type="privacy_token">`.
    r#"
    CREATE TABLE IF NOT EXISTS whatsmeow_privacy_tokens (
        their_id  TEXT PRIMARY KEY,
        token     BLOB NOT NULL,
        timestamp INTEGER NOT NULL
    );
    "#,
    // v5 — Container support. Mirrors
    // `_upstream/whatsmeow/store/sqlstore/container.go`. The original
    // `device` table from v2 was a strict singleton (`CHECK(id = 1)`); the
    // Container model keeps multiple paired devices in one DB, keyed by
    // their AD-JID. We rebuild the table from scratch with the new shape
    // (`jid TEXT PRIMARY KEY`, `lid TEXT`, `blob BLOB`). Any existing
    // singleton row gets a placeholder JID so older data is still
    // discoverable via `Container::list_devices` — `load_device` uses the
    // first row when called against a Container-aware store.
    r#"
    CREATE TABLE IF NOT EXISTS device_v5 (
        jid  TEXT PRIMARY KEY,
        lid  TEXT,
        blob BLOB NOT NULL
    );
    INSERT OR IGNORE INTO device_v5 (jid, lid, blob)
        SELECT '__legacy__:0', NULL, blob FROM device WHERE id = 1;
    DROP TABLE IF EXISTS device;
    ALTER TABLE device_v5 RENAME TO device;
    "#,
    // v6 — newsletter (channel) keys. Maps a channel JID → a 32-byte
    // symmetric key used by `crates/wha-client/src/armadillo_message.rs`.
    // Note: upstream whatsmeow has no equivalent table today (newsletter
    // messages travel as `<plaintext>` on the wire); see
    // `wha-store::traits::NewsletterKeyStore` for the parity caveat.
    r#"
    CREATE TABLE IF NOT EXISTS newsletter_keys (
        channel TEXT PRIMARY KEY,
        key     BLOB NOT NULL
    );
    "#,
];

/// Applies any pending migrations to `conn`. Tracks progress in the
/// `whatsmeow_version` table (created on demand) so re-opens are no-ops.
pub fn apply(conn: &mut Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS whatsmeow_version (
            id      INTEGER PRIMARY KEY CHECK (id = 1),
            version INTEGER NOT NULL
        );
        INSERT OR IGNORE INTO whatsmeow_version (id, version) VALUES (1, 0);",
    )?;

    let current: i64 = conn.query_row(
        "SELECT version FROM whatsmeow_version WHERE id = 1",
        [],
        |row| row.get(0),
    )?;

    if current >= SCHEMA_VERSION {
        return Ok(());
    }

    let tx: Transaction<'_> = conn.transaction()?;
    for (idx, sql) in MIGRATIONS.iter().enumerate() {
        let target_version = (idx + 1) as i64;
        if target_version <= current {
            continue;
        }
        tx.execute_batch(sql)?;
    }
    tx.execute(
        "UPDATE whatsmeow_version SET version = ?1 WHERE id = 1",
        [SCHEMA_VERSION],
    )?;
    tx.commit()?;
    Ok(())
}
