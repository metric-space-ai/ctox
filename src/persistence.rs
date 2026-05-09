use anyhow::Context;
use anyhow::Result;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const DEFAULT_SQLITE_RELATIVE_PATH: &str = "runtime/ctox.sqlite3";
const PAYLOAD_TABLE: &str = "ctox_payload_store";
const KV_TABLE: &str = "ctox_kv_store";
const LEASE_TABLE: &str = "ctox_lease_store";

pub fn sqlite_path(root: &Path) -> PathBuf {
    if let Some(state_root) = std::env::var_os("CTOX_STATE_ROOT")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        return state_root.join("ctox.sqlite3");
    }
    root.join(DEFAULT_SQLITE_RELATIVE_PATH)
}

pub fn sqlite_busy_timeout_duration() -> Duration {
    let millis = std::env::var("CTOX_SQLITE_BUSY_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| (1..=120_000).contains(value))
        .unwrap_or(30_000);
    Duration::from_millis(millis)
}

pub fn sqlite_busy_timeout_millis() -> u64 {
    sqlite_busy_timeout_duration().as_millis() as u64
}

pub fn load_json_payload<T>(root: &Path, key: &str) -> Result<Option<T>>
where
    T: DeserializeOwned,
{
    let conn = open_sqlite(root)?;
    let payload_json: Option<String> = conn
        .query_row(
            &format!("SELECT payload_json FROM {PAYLOAD_TABLE} WHERE payload_key = ?1"),
            params![key],
            |row| row.get(0),
        )
        .optional()
        .with_context(|| format!("failed to load payload {key}"))?;
    payload_json
        .map(|payload| {
            serde_json::from_str::<T>(&payload)
                .with_context(|| format!("failed to decode payload {key}"))
        })
        .transpose()
}

pub fn store_json_payload<T>(root: &Path, key: &str, payload: Option<&T>) -> Result<()>
where
    T: Serialize,
{
    let conn = open_sqlite(root)?;
    match payload {
        Some(payload) => {
            let encoded = serde_json::to_string_pretty(payload)
                .with_context(|| format!("failed to encode payload {key}"))?;
            conn.execute(
                &format!(
                    "INSERT INTO {PAYLOAD_TABLE} (payload_key, payload_json, updated_at)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(payload_key) DO UPDATE SET
                       payload_json = excluded.payload_json,
                       updated_at = excluded.updated_at"
                ),
                params![key, encoded, now_epoch_secs()],
            )
            .with_context(|| format!("failed to persist payload {key}"))?;
        }
        None => {
            conn.execute(
                &format!("DELETE FROM {PAYLOAD_TABLE} WHERE payload_key = ?1"),
                params![key],
            )
            .with_context(|| format!("failed to delete payload {key}"))?;
        }
    }
    Ok(())
}

pub fn load_text_value(root: &Path, key: &str) -> Result<Option<String>> {
    let conn = open_sqlite(root)?;
    conn.query_row(
        &format!("SELECT kv_value FROM {KV_TABLE} WHERE kv_key = ?1"),
        params![key],
        |row| row.get(0),
    )
    .optional()
    .with_context(|| format!("failed to load kv value {key}"))
}

pub fn store_text_value(root: &Path, key: &str, value: Option<&str>) -> Result<()> {
    let conn = open_sqlite(root)?;
    match value {
        Some(value) => {
            conn.execute(
                &format!(
                    "INSERT INTO {KV_TABLE} (kv_key, kv_value, updated_at)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(kv_key) DO UPDATE SET
                       kv_value = excluded.kv_value,
                       updated_at = excluded.updated_at"
                ),
                params![key, value, now_epoch_secs()],
            )
            .with_context(|| format!("failed to persist kv value {key}"))?;
        }
        None => {
            conn.execute(
                &format!("DELETE FROM {KV_TABLE} WHERE kv_key = ?1"),
                params![key],
            )
            .with_context(|| format!("failed to delete kv value {key}"))?;
        }
    }
    Ok(())
}

pub fn try_acquire_lease(root: &Path, key: &str, value: &str) -> Result<bool> {
    let conn = open_sqlite(root)?;
    let inserted = conn
        .execute(
            &format!(
                "INSERT OR IGNORE INTO {LEASE_TABLE} (lease_key, lease_value, updated_at)
                 VALUES (?1, ?2, ?3)"
            ),
            params![key, value, now_epoch_secs()],
        )
        .with_context(|| format!("failed to acquire lease {key}"))?;
    Ok(inserted > 0)
}

pub fn load_lease_value(root: &Path, key: &str) -> Result<Option<String>> {
    let conn = open_sqlite(root)?;
    conn.query_row(
        &format!("SELECT lease_value FROM {LEASE_TABLE} WHERE lease_key = ?1"),
        params![key],
        |row| row.get(0),
    )
    .optional()
    .with_context(|| format!("failed to load lease {key}"))
}

pub fn release_lease(root: &Path, key: &str) -> Result<()> {
    let conn = open_sqlite(root)?;
    conn.execute(
        &format!("DELETE FROM {LEASE_TABLE} WHERE lease_key = ?1"),
        params![key],
    )
    .with_context(|| format!("failed to release lease {key}"))?;
    Ok(())
}

fn open_sqlite(root: &Path) -> Result<Connection> {
    let path = sqlite_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create sqlite dir {}", parent.display()))?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open sqlite db {}", path.display()))?;
    conn.busy_timeout(sqlite_busy_timeout_duration())
        .context("failed to configure SQLite busy_timeout")?;
    let busy_timeout_ms = sqlite_busy_timeout_millis();
    conn.execute_batch(&format!(
        "PRAGMA journal_mode=WAL;
         PRAGMA busy_timeout={busy_timeout_ms};
         CREATE TABLE IF NOT EXISTS {PAYLOAD_TABLE} (
             payload_key TEXT PRIMARY KEY,
             payload_json TEXT NOT NULL,
             updated_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS {KV_TABLE} (
             kv_key TEXT PRIMARY KEY,
             kv_value TEXT NOT NULL,
             updated_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS {LEASE_TABLE} (
             lease_key TEXT PRIMARY KEY,
             lease_value TEXT NOT NULL,
             updated_at INTEGER NOT NULL
         );"
    ))
    .context("failed to initialize sqlite persistence schema")?;
    Ok(conn)
}

fn now_epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
