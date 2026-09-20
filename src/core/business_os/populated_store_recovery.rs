// Origin: CTOX
// License: AGPL-3.0-only

//! Populated-store migration/recovery for the native RxDB cutover.
//!
//! Startup still copies and verifies through
//! `migrate_additive_native_rxdb_collection_versions` and
//! `repair_stale_rxdb_collection_schema_versions`. This module adds the
//! missing whole-store rehearsal: a deterministic supported historical
//! fixture, an immutable backup with pinned provenance, a write-once
//! cutover receipt, and a fail-closed restore after accepted post-cutover
//! writes.

use super::backup_restore::file_sha256;
use super::hashing::hex_sha256;
use super::rxdb_peer::{
    acquire_native_peer_process_lock, expected_rxdb_collection_version,
    migrate_additive_native_rxdb_collection_versions, repair_stale_rxdb_collection_schema_versions,
    rxdb_collection_version_table_name, sqlite_quote_identifier, sqlite_table_exists,
};
use super::store::{now_ms, rxdb_store_path, RXDB_STORE_FILE};
use anyhow::{anyhow, Context};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(test)]
use std::sync::Mutex;

#[cfg(test)]
static AFTER_SQLITE_RELEASE: Mutex<Option<Box<dyn Fn() + Send>>> = Mutex::new(None);

pub const POPULATED_STORE_FIXTURE_SCHEMA: &str = "ctox.populated_store_recovery.fixture.v1";
pub const NATIVE_RXDB_CUTOVER_RECEIPT_SCHEMA: &str = "ctox.native_rxdb.cutover_receipt.v1";
pub const NATIVE_RXDB_IMMUTABLE_BACKUP_KIND: &str = "ctox.native_rxdb.immutable_backup.v1";
pub const NATIVE_RXDB_IMMUTABLE_BACKUP_PROVENANCE_SCHEMA: &str =
    "ctox.native_rxdb.immutable_backup.provenance.v1";
const FIXTURE_JSON: &str = include_str!(
    "../../../tests/fixtures/populated-store-recovery/supported-historical-inventory.json"
);
const PACKAGED_COCKPIT_SCHEMA: &str =
    include_str!("../../apps/business-os/modules/ctox/collections.schema.json");

pub fn native_rxdb_cutover_receipt_path(root: &Path) -> PathBuf {
    root.join("runtime/business-os-rxdb.cutover.json")
}

pub fn default_native_rxdb_immutable_backup_path(root: &Path) -> PathBuf {
    root.join("runtime/business-os-rxdb.immutable-backup.sqlite3")
}

pub fn native_rxdb_immutable_backup_provenance_path(root: &Path) -> PathBuf {
    root.join("runtime/business-os-rxdb.immutable-backup.json")
}

pub fn populated_store_fixture_spec() -> anyhow::Result<Value> {
    serde_json::from_str(FIXTURE_JSON).context("parse populated-store recovery fixture")
}

pub fn supported_historical_rxdb_versions() -> anyhow::Result<Value> {
    let contract: Value = serde_json::from_str(include_str!("business_os_schema_contract.json"))
        .context("parse Business OS schema contract")?;
    let packaged: Value =
        serde_json::from_str(PACKAGED_COCKPIT_SCHEMA).context("parse packaged cockpit schema")?;
    let strategies = packaged
        .get("migration_strategies")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut supported = Map::new();
    for (collection, spec) in strategies {
        let current = contract
            .get(&collection)
            .and_then(|schema| schema.get("version"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let mut steps = BTreeMap::new();
        if let Some(object) = spec.as_object() {
            for (version, step) in object {
                steps.insert(version.clone(), step.clone());
            }
        }
        let declared_steps = declared_migration_step_versions(&steps);
        let historical = supported_source_versions_for_chain(current, &declared_steps);
        supported.insert(
            collection,
            json!({
                "current_version": current,
                "declared_steps": steps,
                "complete_chain": complete_migration_chain(current, &declared_steps),
                "supported_historical_source_versions": historical,
            }),
        );
    }
    Ok(json!({
        "schema": "ctox.native_rxdb.supported_historical_versions.v1",
        "from": "declared migration_strategies plus business_os_schema_contract.json",
        "collections": supported,
        "unsupported": populated_store_fixture_spec()?["unsupported_source_versions"],
    }))
}

fn declared_migration_step_versions(steps: &BTreeMap<String, Value>) -> BTreeSet<i64> {
    steps
        .keys()
        .filter_map(|version| version.parse::<i64>().ok())
        .filter(|version| *version > 0)
        .collect()
}

fn complete_migration_chain(current: i64, declared_steps: &BTreeSet<i64>) -> bool {
    current <= 0 || (1..=current).all(|step| declared_steps.contains(&step))
}

fn supported_source_versions_for_chain(current: i64, declared_steps: &BTreeSet<i64>) -> Vec<i64> {
    if current <= 0 {
        return Vec::new();
    }
    (0..current)
        .filter(|source| (*source + 1..=current).all(|step| declared_steps.contains(&step)))
        .collect()
}

pub fn materialize_supported_historical_rxdb_fixture(root: &Path) -> anyhow::Result<Value> {
    let spec = populated_store_fixture_spec()?;
    anyhow::ensure!(
        spec.get("schema").and_then(Value::as_str) == Some(POPULATED_STORE_FIXTURE_SCHEMA),
        "unsupported populated-store fixture schema"
    );
    let supported = supported_historical_rxdb_versions()?;
    assert_fresh_isolated_rxdb_fixture_root(root)?;
    let database_path = rxdb_store_path(root);
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create native RxDB runtime dir {}", parent.display()))?;
    }
    let conn = open_rxdb_sqlite(&database_path).with_context(|| {
        format!(
            "create historical native RxDB fixture {}",
            database_path.display()
        )
    })?;
    let documents = spec
        .get("documents")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut materialized = Vec::new();
    for document in &documents {
        let collection = document
            .get("collection")
            .and_then(Value::as_str)
            .context("fixture document missing collection")?;
        let source_version = document
            .get("source_version")
            .and_then(Value::as_i64)
            .context("fixture document missing source_version")?;
        ensure_historical_version_is_supported(&supported, collection, source_version)?;
        let table = rxdb_collection_version_table_name(collection, source_version);
        create_rxdb_document_table(&conn, &table)?;
        insert_fixture_row(&conn, &table, document)?;
        materialized.push(json!({
            "collection": collection,
            "source_version": source_version,
            "table": table,
            "id": document.get("id"),
            "deleted": document.get("deleted"),
        }));
    }
    drop(conn);
    let (bytes, sha256) = file_sha256(&database_path)?;
    Ok(json!({
        "ok": true,
        "kind": "ctox.populated_store_recovery.materialize.v1",
        "fixture_schema": POPULATED_STORE_FIXTURE_SCHEMA,
        "database_path": database_path.display().to_string(),
        "bytes": bytes,
        "sha256": sha256,
        "supported_historical": supported,
        "documents": materialized
    }))
}

fn assert_fresh_isolated_rxdb_fixture_root(root: &Path) -> anyhow::Result<()> {
    let database_path = rxdb_store_path(root);
    let mut blocked = vec![
        database_path.clone(),
        native_rxdb_cutover_receipt_path(root),
        native_rxdb_immutable_backup_provenance_path(root),
        default_native_rxdb_immutable_backup_path(root),
        crate::paths::runtime_dir(root).join("ctox.sqlite3"),
        crate::paths::runtime_dir(root).join("business-os.sqlite3"),
    ];
    blocked.extend(sqlite_sidecar_paths(&database_path));
    for path in blocked {
        anyhow::ensure!(
            !path.exists(),
            "refusing to materialize a synthetic historical fixture over existing {} — use a fresh isolated root",
            path.display()
        );
    }
    Ok(())
}

fn ensure_historical_version_is_supported(
    supported: &Value,
    collection: &str,
    source_version: i64,
) -> anyhow::Result<()> {
    let current = expected_rxdb_collection_version(collection);
    if source_version == current {
        return Ok(());
    }
    let historical = supported
        .pointer(&format!(
            "/collections/{collection}/supported_historical_source_versions"
        ))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let allowed = historical
        .iter()
        .any(|value| value.as_i64() == Some(source_version));
    anyhow::ensure!(
        allowed,
        "refusing to materialize unsupported historical collection `{collection}` version {source_version}; current version is {current}"
    );
    Ok(())
}

fn create_rxdb_document_table(conn: &Connection, table: &str) -> anyhow::Result<()> {
    conn.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS {} (
            id TEXT NOT NULL PRIMARY KEY UNIQUE,
            revision TEXT,
            deleted INTEGER NOT NULL CHECK (deleted IN (0, 1)),
            lastWriteTime REAL NOT NULL,
            data TEXT NOT NULL
        )",
        sqlite_quote_identifier(table)
    ))
    .with_context(|| format!("create fixture table {table}"))?;
    Ok(())
}

fn insert_fixture_row(conn: &Connection, table: &str, document: &Value) -> anyhow::Result<()> {
    let id = document
        .get("id")
        .and_then(Value::as_str)
        .context("fixture row missing id")?;
    let revision = document
        .get("revision")
        .and_then(Value::as_str)
        .unwrap_or("1-populated-store-recovery");
    let deleted = document.get("deleted").and_then(Value::as_i64).unwrap_or(0);
    let last_write_time = document
        .get("lastWriteTime")
        .and_then(Value::as_f64)
        .context("fixture row missing lastWriteTime")?;
    let data = document
        .get("data")
        .cloned()
        .context("fixture row missing data")?;
    conn.execute(
        &format!(
            "INSERT INTO {} (id, revision, deleted, lastWriteTime, data)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            sqlite_quote_identifier(table)
        ),
        params![id, revision, deleted, last_write_time, data.to_string()],
    )
    .with_context(|| format!("insert fixture row {id} into {table}"))?;
    Ok(())
}

pub fn native_rxdb_store_inventory(root: &Path) -> anyhow::Result<Value> {
    sqlite_store_inventory(&rxdb_store_path(root))
}

fn sqlite_store_inventory(database_path: &Path) -> anyhow::Result<Value> {
    if !database_path.is_file() {
        return Ok(json!({
            "ok": true,
            "database_path": database_path.display().to_string(),
            "present": false,
            "tables": {},
            "inventory_sha256": hex_sha256(b""),
            "bytes": 0
        }));
    }
    let conn = open_rxdb_sqlite(database_path)
        .with_context(|| format!("open native RxDB store {}", database_path.display()))?;
    let mut statement = conn.prepare(
        "SELECT name FROM sqlite_master
         WHERE type = 'table' AND name LIKE 'ctox_business_os__%'
         ORDER BY name ASC",
    )?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut tables = BTreeMap::new();
    let mut digest = Sha256::new();
    for table in names {
        if table.contains("___rxdb_internal__") {
            continue;
        }
        let documents = inventory_rows(&conn, &table)?;
        digest.update(table.as_bytes());
        digest.update([0]);
        for document in &documents {
            digest.update(document.to_string().as_bytes());
            digest.update([0]);
        }
        tables.insert(
            table,
            json!({
                "row_count": documents.len(),
                "documents": documents
            }),
        );
    }
    let (bytes, file_sha) = file_sha256(database_path)?;
    Ok(json!({
        "ok": true,
        "present": true,
        "database_path": database_path.display().to_string(),
        "bytes": bytes,
        "file_sha256": file_sha,
        "tables": tables,
        "inventory_sha256": hex_sha256(digest.finalize().as_slice())
    }))
}

fn inventory_rows(conn: &Connection, table: &str) -> anyhow::Result<Vec<Value>> {
    if !sqlite_table_exists(conn, table)? {
        return Ok(Vec::new());
    }
    let has_revision = sqlite_column_exists(conn, table, "revision")?;
    let has_deleted = sqlite_column_exists(conn, table, "deleted")?;
    let has_lwt = sqlite_column_exists(conn, table, "lastWriteTime")?;
    let revision_sql = if has_revision {
        "revision"
    } else {
        "json_extract(data, '$._rev')"
    };
    let deleted_sql = if has_deleted {
        "COALESCE(deleted, 0)"
    } else {
        "COALESCE(json_extract(data, '$._deleted'), 0)"
    };
    let lwt_sql = if has_lwt {
        "COALESCE(lastWriteTime, 0)"
    } else {
        "CAST(COALESCE(json_extract(data, '$._meta.lwt'), json_extract(data, '$.updated_at_ms'), 0) AS REAL)"
    };
    let mut statement = conn.prepare(&format!(
        "SELECT id, {revision_sql}, {deleted_sql}, {lwt_sql}, data
         FROM {} ORDER BY id ASC",
        sqlite_quote_identifier(table)
    ))?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, f64>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    let mut documents = Vec::new();
    for row in rows {
        let (id, revision, deleted, last_write_time, data) = row?;
        let parsed: Value = serde_json::from_str(&data).unwrap_or(Value::String(data.clone()));
        documents.push(json!({
            "id": id,
            "revision": revision,
            "deleted": deleted,
            "lastWriteTime": last_write_time,
            "data_sha256": hex_sha256(data.as_bytes()),
            "command_id": parsed.get("command_id"),
            "task_id": parsed.get("task_id"),
            "file_id": parsed.get("file_id"),
            "linked_record_id": parsed.get("linked_record_id"),
            "status": parsed.get("status"),
            "inbound_channel": parsed.get("inbound_channel"),
            "history": parsed.get("history"),
            "content_hash": parsed.get("content_hash"),
            "chunk_hash": parsed.get("chunk_hash"),
            "_deleted": parsed.get("_deleted")
        }));
    }
    Ok(documents)
}

fn sqlite_column_exists(conn: &Connection, table: &str, column: &str) -> anyhow::Result<bool> {
    let mut statement = conn.prepare(&format!(
        "PRAGMA table_info({})",
        sqlite_quote_identifier(table)
    ))?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn compare_populated_store_inventories(before: &Value, after: &Value) -> anyhow::Result<Value> {
    let before_docs = flatten_inventory_docs(before);
    let after_docs = flatten_inventory_docs(after);
    let mut missing = Vec::new();
    let mut changed = Vec::new();
    let mut preserved = Vec::new();
    for (id, before_row) in &before_docs {
        match after_docs.get(id) {
            None => missing.push(id.clone()),
            Some(after_row) => {
                let same_deleted = before_row.get("deleted") == after_row.get("deleted");
                let same_history = before_row.get("history") == after_row.get("history");
                let same_hashes = before_row.get("content_hash") == after_row.get("content_hash")
                    && before_row.get("chunk_hash") == after_row.get("chunk_hash");
                let same_refs = before_row.get("command_id") == after_row.get("command_id")
                    && before_row.get("task_id") == after_row.get("task_id")
                    && before_row.get("file_id") == after_row.get("file_id")
                    && before_row.get("linked_record_id") == after_row.get("linked_record_id");
                if same_deleted && same_history && same_hashes && same_refs {
                    preserved.push(id.clone());
                } else {
                    changed.push(json!({
                        "id": id,
                        "before": before_row,
                        "after": after_row
                    }));
                }
            }
        }
    }
    anyhow::ensure!(
        missing.is_empty(),
        "populated-store cutover dropped stable ids: {}",
        missing.join(", ")
    );
    anyhow::ensure!(
        changed.is_empty(),
        "populated-store cutover changed protected fields: {changed:?}"
    );
    Ok(json!({
        "ok": true,
        "preserved_ids": preserved,
        "before_inventory_sha256": before.get("inventory_sha256"),
        "after_inventory_sha256": after.get("inventory_sha256")
    }))
}

fn flatten_inventory_docs(inventory: &Value) -> BTreeMap<String, Value> {
    let mut docs = BTreeMap::new();
    let Some(tables) = inventory.get("tables").and_then(Value::as_object) else {
        return docs;
    };
    for table in tables.values() {
        let Some(documents) = table.get("documents").and_then(Value::as_array) else {
            continue;
        };
        for document in documents {
            if let Some(id) = document.get("id").and_then(Value::as_str) {
                docs.insert(id.to_string(), document.clone());
            }
        }
    }
    docs
}

pub fn backup_native_rxdb_immutable_store(
    root: &Path,
    output: Option<&Path>,
) -> anyhow::Result<Value> {
    let database_path = rxdb_store_path(root);
    anyhow::ensure!(
        database_path.is_file(),
        "native RxDB store {} is missing",
        database_path.display()
    );
    let output = output
        .map(PathBuf::from)
        .unwrap_or_else(|| default_native_rxdb_immutable_backup_path(root));
    anyhow::ensure!(
        !output.exists(),
        "refusing to overwrite existing immutable RxDB backup {}",
        output.display()
    );
    anyhow::ensure!(
        !paths_would_alias(&database_path, &output)?,
        "refusing immutable RxDB backup that aliases the live store {} -> {}",
        database_path.display(),
        output.display()
    );
    let provenance_path = native_rxdb_immutable_backup_provenance_path(root);
    anyhow::ensure!(
        !provenance_path.exists(),
        "refusing to replace pinned immutable RxDB backup provenance {}",
        provenance_path.display()
    );
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let conn = open_rxdb_sqlite(&database_path)
        .with_context(|| format!("open native RxDB store {}", database_path.display()))?;
    let output_text = output.to_string_lossy().to_string();
    conn.execute("VACUUM INTO ?1", params![output_text])
        .with_context(|| format!("write immutable RxDB backup {}", output.display()))?;
    drop(conn);
    fsync_file(&output)?;
    let (bytes, sha256) = file_sha256(&output)?;
    let inventory = sqlite_store_inventory(&output)?;
    let provenance = json!({
        "schema": NATIVE_RXDB_IMMUTABLE_BACKUP_PROVENANCE_SCHEMA,
        "kind": NATIVE_RXDB_IMMUTABLE_BACKUP_KIND,
        "created_at_ms": now_ms() as u64,
        "source": database_path.display().to_string(),
        "backup_path": output.display().to_string(),
        "bytes": bytes,
        "sha256": sha256,
        "inventory_sha256": inventory.get("inventory_sha256")
    });
    if let Some(parent) = provenance_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&provenance_path, serde_json::to_vec_pretty(&provenance)?)
        .with_context(|| format!("write backup provenance {}", provenance_path.display()))?;
    Ok(json!({
        "ok": true,
        "kind": NATIVE_RXDB_IMMUTABLE_BACKUP_KIND,
        "source": database_path.display().to_string(),
        "backup_path": output.display().to_string(),
        "bytes": bytes,
        "sha256": sha256,
        "inventory_sha256": inventory.get("inventory_sha256"),
        "provenance_path": provenance_path.display().to_string()
    }))
}

const CUTOVER_STATE_TABLE: &str = "ctox_native_rxdb_cutover_state";
const CUTOVER_PHASE_IN_PROGRESS: &str = "in_progress";
const CUTOVER_PHASE_ACCEPTED: &str = "accepted";

struct NativeRxdbWriterGuard {
    _peer_lock: File,
    exclusive: Option<Connection>,
}

impl NativeRxdbWriterGuard {
    fn release_sqlite(&mut self) {
        if let Some(conn) = self.exclusive.take() {
            let _ = conn.execute_batch("ROLLBACK;");
            drop(conn);
        }
    }
}

impl Drop for NativeRxdbWriterGuard {
    fn drop(&mut self) {
        self.release_sqlite();
    }
}

fn acquire_exclusive_native_rxdb_writer(root: &Path) -> anyhow::Result<NativeRxdbWriterGuard> {
    let peer_lock = acquire_native_peer_process_lock(root)?.ok_or_else(|| {
        anyhow!(
            "refusing native RxDB restore: an active native peer holds {}",
            root.join("runtime/business-os-rxdb-peer.lock").display()
        )
    })?;
    let live_path = rxdb_store_path(root);
    let exclusive = if live_path.is_file() {
        let conn = open_rxdb_sqlite(&live_path)?;
        conn.busy_timeout(Duration::from_millis(250))?;
        conn.execute_batch("PRAGMA locking_mode=EXCLUSIVE; BEGIN EXCLUSIVE;")
            .with_context(|| {
                format!(
                    "refusing native RxDB exclusive writer: {} is busy",
                    live_path.display()
                )
            })?;
        Some(conn)
    } else {
        None
    };
    Ok(NativeRxdbWriterGuard {
        _peer_lock: peer_lock,
        exclusive,
    })
}

fn acquire_native_rxdb_peer_lock(root: &Path) -> anyhow::Result<File> {
    acquire_native_peer_process_lock(root)?.ok_or_else(|| {
        anyhow!(
            "refusing native RxDB writer: an active native peer holds {}",
            root.join("runtime/business-os-rxdb-peer.lock").display()
        )
    })
}

pub fn mark_cutover_in_progress(root: &Path) -> anyhow::Result<Value> {
    if let Some(existing) = durable_cutover_state(root, None)? {
        if existing.get("phase").and_then(Value::as_str) == Some(CUTOVER_PHASE_ACCEPTED)
            || existing.get("phase").and_then(Value::as_str) == Some(CUTOVER_PHASE_IN_PROGRESS)
        {
            return Ok(existing);
        }
    }
    let Some(provenance) = native_rxdb_backup_provenance(root)? else {
        return Ok(json!({
            "ok": true,
            "skipped": true,
            "reason": "no pinned backup provenance"
        }));
    };
    let payload = json!({
        "schema": NATIVE_RXDB_CUTOVER_RECEIPT_SCHEMA,
        "phase": CUTOVER_PHASE_IN_PROGRESS,
        "recorded_at_ms": now_ms() as u64,
        "store": RXDB_STORE_FILE,
        "backup_sha256": provenance.get("sha256"),
        "backup_path": provenance.get("backup_path"),
        "pre_cutover_inventory_sha256": provenance.get("inventory_sha256")
    });
    persist_cutover_state_to_store(root, &payload)?;
    write_cutover_json(root, &payload)?;
    Ok(payload)
}

pub fn record_native_rxdb_cutover_receipt(root: &Path) -> anyhow::Result<Value> {
    if let Some(existing) = durable_cutover_state(root, None)? {
        if existing.get("phase").and_then(Value::as_str) == Some(CUTOVER_PHASE_ACCEPTED) {
            if !native_rxdb_cutover_receipt_path(root).is_file() {
                write_cutover_json(root, &existing)?;
            }
            return Ok(existing);
        }
    }
    let inventory = native_rxdb_store_inventory(root)?;
    let provenance = native_rxdb_backup_provenance(root).ok().flatten();
    let in_progress = durable_cutover_state(root, None)?;
    let receipt = json!({
        "schema": NATIVE_RXDB_CUTOVER_RECEIPT_SCHEMA,
        "phase": CUTOVER_PHASE_ACCEPTED,
        "recorded_at_ms": now_ms() as u64,
        "store": RXDB_STORE_FILE,
        "database_path": inventory.get("database_path"),
        "inventory_sha256": inventory.get("inventory_sha256"),
        "bytes": inventory.get("bytes"),
        "file_sha256": inventory.get("file_sha256"),
        "backup_sha256": provenance.as_ref().and_then(|value| value.get("sha256")).cloned()
            .or_else(|| in_progress.as_ref().and_then(|value| value.get("backup_sha256")).cloned()),
        "backup_path": provenance.as_ref().and_then(|value| value.get("backup_path")).cloned()
            .or_else(|| in_progress.as_ref().and_then(|value| value.get("backup_path")).cloned()),
        "pre_cutover_inventory_sha256": provenance.as_ref().and_then(|value| value.get("inventory_sha256")).cloned()
            .or_else(|| in_progress.as_ref().and_then(|value| value.get("pre_cutover_inventory_sha256")).cloned()),
        "supported_historical": supported_historical_rxdb_versions()?
    });
    persist_cutover_state_to_store(root, &receipt)?;
    write_cutover_json(root, &receipt)?;
    Ok(receipt)
}

pub fn native_rxdb_cutover_receipt(root: &Path) -> anyhow::Result<Value> {
    let path = native_rxdb_cutover_receipt_path(root);
    match durable_cutover_state(root, None)? {
        Some(receipt) => Ok(json!({
            "ok": true,
            "present": true,
            "path": path.display().to_string(),
            "receipt": receipt
        })),
        None => Ok(json!({
            "ok": true,
            "present": false,
            "path": path.display().to_string()
        })),
    }
}

fn write_cutover_json(root: &Path, payload: &Value) -> anyhow::Result<()> {
    let path = native_rxdb_cutover_receipt_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, serde_json::to_vec_pretty(payload)?)
        .with_context(|| format!("write cutover receipt {}", path.display()))
}

fn native_rxdb_cutover_receipt_document(root: &Path) -> anyhow::Result<Value> {
    let path = native_rxdb_cutover_receipt_path(root);
    serde_json::from_slice(&fs::read(&path)?)
        .with_context(|| format!("parse cutover receipt {}", path.display()))
}

fn native_rxdb_backup_provenance(root: &Path) -> anyhow::Result<Option<Value>> {
    let path = native_rxdb_immutable_backup_provenance_path(root);
    if !path.is_file() {
        return Ok(None);
    }
    let provenance: Value = serde_json::from_slice(&fs::read(&path)?)
        .with_context(|| format!("parse immutable backup provenance {}", path.display()))?;
    anyhow::ensure!(
        provenance.get("schema").and_then(Value::as_str)
            == Some(NATIVE_RXDB_IMMUTABLE_BACKUP_PROVENANCE_SCHEMA),
        "immutable RxDB backup provenance schema is unsupported"
    );
    Ok(Some(provenance))
}

fn persist_cutover_state_to_store(root: &Path, payload: &Value) -> anyhow::Result<()> {
    let database_path = rxdb_store_path(root);
    if !database_path.is_file() {
        return Ok(());
    }
    let conn = open_rxdb_sqlite(&database_path)?;
    persist_cutover_state_on_connection(&conn, payload)
}

fn persist_cutover_state_on_connection(conn: &Connection, payload: &Value) -> anyhow::Result<()> {
    ensure_cutover_state_table(conn)?;
    if let Some(existing) = read_cutover_state_on_connection(conn)? {
        if existing.get("phase").and_then(Value::as_str) == Some(CUTOVER_PHASE_ACCEPTED) {
            return Ok(());
        }
    }
    conn.execute(
        &format!(
            "INSERT OR REPLACE INTO {} (id, phase, payload) VALUES ('current', ?1, ?2)",
            sqlite_quote_identifier(CUTOVER_STATE_TABLE)
        ),
        params![
            payload
                .get("phase")
                .and_then(Value::as_str)
                .unwrap_or(CUTOVER_PHASE_IN_PROGRESS),
            payload.to_string()
        ],
    )?;
    Ok(())
}

fn ensure_cutover_state_table(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS {} (
            id TEXT PRIMARY KEY,
            phase TEXT NOT NULL,
            payload TEXT NOT NULL
        )",
        sqlite_quote_identifier(CUTOVER_STATE_TABLE)
    ))?;
    Ok(())
}

fn read_cutover_state_on_connection(conn: &Connection) -> anyhow::Result<Option<Value>> {
    if !sqlite_table_exists(conn, CUTOVER_STATE_TABLE)? {
        return Ok(None);
    }
    let payload: Option<String> = conn
        .query_row(
            &format!(
                "SELECT payload FROM {} WHERE id = 'current'",
                sqlite_quote_identifier(CUTOVER_STATE_TABLE)
            ),
            [],
            |row| row.get(0),
        )
        .optional()?;
    match payload {
        Some(raw) => Ok(Some(serde_json::from_str(&raw)?)),
        None => Ok(None),
    }
}

fn durable_cutover_state(
    root: &Path,
    live_conn: Option<&Connection>,
) -> anyhow::Result<Option<Value>> {
    let sqlite_state = if let Some(conn) = live_conn {
        read_cutover_state_on_connection(conn)?
    } else {
        let database_path = rxdb_store_path(root);
        if database_path.is_file() {
            let conn = open_rxdb_sqlite(&database_path)?;
            read_cutover_state_on_connection(&conn)?
        } else {
            None
        }
    };
    if let Some(state) = sqlite_state {
        return Ok(Some(state));
    }
    let path = native_rxdb_cutover_receipt_path(root);
    if !path.is_file() {
        return Ok(None);
    }
    let json_state = native_rxdb_cutover_receipt_document(root)?;
    match json_state.get("phase").and_then(Value::as_str) {
        Some(CUTOVER_PHASE_ACCEPTED) | None
            if json_state.get("inventory_sha256").is_some()
                && json_state.get("phase").and_then(Value::as_str)
                    != Some(CUTOVER_PHASE_IN_PROGRESS) =>
        {
            Ok(Some(json_state))
        }
        Some(CUTOVER_PHASE_IN_PROGRESS) => {
            // JSON in_progress is not independently durable once the live
            // store has diverged. Honor it only as a copy of sqlite state,
            // which was already missing above.
            Ok(None)
        }
        _ => Ok(None),
    }
}

pub fn restore_native_rxdb_immutable_backup(root: &Path, backup: &Path) -> anyhow::Result<Value> {
    anyhow::ensure!(
        backup.is_file(),
        "immutable RxDB backup {} is missing",
        backup.display()
    );
    let mut guard = acquire_exclusive_native_rxdb_writer(root)?;
    if let Some(reason) = refuse_restore(root, backup, guard.exclusive.as_ref())? {
        drop(guard);
        anyhow::bail!("{reason}");
    }
    let live_path = rxdb_store_path(root);
    let (backup_bytes, backup_sha256) = file_sha256(backup)?;
    if let Some(conn) = guard.exclusive.as_ref() {
        checkpoint_live_store(conn, &live_path)?;
    }
    // SQLite must release the live inode before rename, but the peer lock stays
    // held. Native peer bring-up and this restore/cutover path are the production
    // writers of the live file; both take that lock.
    guard.release_sqlite();
    invoke_after_sqlite_release_hook();
    replace_sqlite_store(&live_path, backup)?;
    let receipt_path = native_rxdb_cutover_receipt_path(root);
    if receipt_path.exists() {
        fs::remove_file(&receipt_path)?;
    }
    drop(guard);
    Ok(json!({
        "ok": true,
        "kind": "ctox.native_rxdb.immutable_restore.v1",
        "backup_path": backup.display().to_string(),
        "database_path": live_path.display().to_string(),
        "backup_bytes": backup_bytes,
        "backup_sha256": backup_sha256
    }))
}

fn refuse_restore(
    root: &Path,
    backup: &Path,
    live_conn: Option<&Connection>,
) -> anyhow::Result<Option<String>> {
    let live_path = rxdb_store_path(root);
    let provenance = match native_rxdb_backup_provenance(root) {
        Ok(Some(value)) => value,
        Ok(None) if live_path.is_file() => {
            return Ok(Some(
                "refusing native RxDB restore: live store has no pinned immutable backup provenance"
                    .into(),
            ));
        }
        Ok(None) => {
            return Ok(Some(
                "refusing native RxDB restore: missing immutable backup provenance".into(),
            ));
        }
        Err(err) => {
            return Ok(Some(format!(
                "refusing native RxDB restore: corrupt backup provenance ({err})"
            )));
        }
    };
    let recorded_backup = provenance
        .get("sha256")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let (_bytes, backup_sha256) = file_sha256(backup)?;
    if recorded_backup != backup_sha256 {
        return Ok(Some(format!(
            "refusing native RxDB restore: backup identity does not match pinned provenance (pinned {recorded_backup}, backup {backup_sha256})"
        )));
    }
    let live = if let Some(conn) = live_conn {
        sqlite_store_inventory_on(conn, &live_path)?
    } else {
        native_rxdb_store_inventory(root)?
    };
    let live_hash = live
        .get("inventory_sha256")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let baseline = provenance
        .get("inventory_sha256")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let state = match durable_cutover_state(root, live_conn) {
        Ok(value) => value,
        Err(err) => {
            return Ok(Some(format!(
                "refusing native RxDB restore: corrupt cutover state ({err})"
            )));
        }
    };
    match state.as_ref().and_then(|value| value.get("phase").and_then(Value::as_str)) {
        Some(CUTOVER_PHASE_ACCEPTED) => {
            let receipt = state.unwrap();
            if receipt.get("schema").and_then(Value::as_str)
                != Some(NATIVE_RXDB_CUTOVER_RECEIPT_SCHEMA)
            {
                return Ok(Some(
                    "refusing native RxDB restore: cutover receipt schema is unsupported".into(),
                ));
            }
            if let Some(pinned) = receipt.get("backup_sha256").and_then(Value::as_str) {
                if pinned != backup_sha256 {
                    return Ok(Some(format!(
                        "refusing native RxDB restore: backup is not the cutover-pinned snapshot (receipt {pinned}, backup {backup_sha256})"
                    )));
                }
            }
            let recorded = receipt
                .get("inventory_sha256")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if recorded != live_hash {
                return Ok(Some(format!(
                    "refusing native RxDB restore: post-cutover writes were accepted (receipt {recorded}, live {live_hash})"
                )));
            }
            Ok(None)
        }
        Some(CUTOVER_PHASE_IN_PROGRESS) => Ok(None),
        _ if live_path.is_file() && live_hash == baseline && !baseline.is_empty() => Ok(None),
        _ if live_path.is_file() => Ok(Some(
            "refusing native RxDB restore: missing durable cutover state and live store does not match the pre-cutover baseline"
                .into(),
        )),
        _ => Ok(None),
    }
}

fn sqlite_store_inventory_on(conn: &Connection, database_path: &Path) -> anyhow::Result<Value> {
    let mut statement = conn.prepare(
        "SELECT name FROM sqlite_master
         WHERE type = 'table' AND name LIKE 'ctox_business_os__%'
         ORDER BY name ASC",
    )?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut tables = BTreeMap::new();
    let mut digest = Sha256::new();
    for table in names {
        if table.contains("___rxdb_internal__") {
            continue;
        }
        let documents = inventory_rows(conn, &table)?;
        digest.update(table.as_bytes());
        digest.update([0]);
        for document in &documents {
            digest.update(document.to_string().as_bytes());
            digest.update([0]);
        }
        tables.insert(
            table,
            json!({
                "row_count": documents.len(),
                "documents": documents
            }),
        );
    }
    let (bytes, file_sha) = file_sha256(database_path)?;
    Ok(json!({
        "ok": true,
        "present": true,
        "database_path": database_path.display().to_string(),
        "bytes": bytes,
        "file_sha256": file_sha,
        "tables": tables,
        "inventory_sha256": hex_sha256(digest.finalize().as_slice())
    }))
}

fn replace_sqlite_store(live_path: &Path, backup: &Path) -> anyhow::Result<()> {
    if let Some(parent) = live_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let staging = live_path.with_extension("sqlite3.restore-staging");
    let preserved = live_path.with_extension("sqlite3.pre-restore");
    let live_sidecars = sqlite_sidecar_paths(live_path);
    let preserved_sidecars = sqlite_sidecar_paths(&preserved);
    let (_backup_bytes, backup_sha256) = file_sha256(backup)?;

    if live_path.is_file() && preserved.is_file() {
        let (_bytes, live_sha) = file_sha256(live_path)?;
        if live_sha == backup_sha256 {
            // The published file already matches the backup. Leftover live WAL/SHM
            // still attach by filename and must not survive this return.
            remove_existing_files(&live_sidecars)?;
            cleanup_completed_restore(&preserved, &preserved_sidecars, &staging)?;
            if let Some(parent) = live_path.parent() {
                fsync_dir(parent)?;
            }
            return Ok(());
        }
        anyhow::bail!(
            "refusing native RxDB restore: unresolved restore evidence at {} and {}; not deleting recovery files",
            live_path.display(),
            preserved.display()
        );
    }

    if !live_path.is_file() && preserved.is_file() {
        adopt_orphaned_live_sidecars(&live_sidecars, &preserved_sidecars)?;
        if let Some(parent) = live_path.parent() {
            fsync_dir(parent)?;
        }
        if staging.is_file() {
            let (_bytes, staging_sha) = file_sha256(&staging)?;
            anyhow::ensure!(
                staging_sha == backup_sha256,
                "refusing native RxDB restore: unresolved staging {} does not match backup; not deleting recovery files",
                staging.display()
            );
            remove_existing_files(&live_sidecars)?;
            if let Some(parent) = live_path.parent() {
                fsync_dir(parent)?;
            }
            fs::rename(&staging, live_path).with_context(|| {
                format!("complete interrupted restore onto {}", live_path.display())
            })?;
            if let Some(parent) = live_path.parent() {
                fsync_dir(parent)?;
            }
            let (_bytes, live_sha) = file_sha256(live_path)?;
            anyhow::ensure!(
                live_sha == backup_sha256,
                "interrupted restore completed but live store hash does not match backup"
            );
            cleanup_completed_restore(&preserved, &preserved_sidecars, &staging)?;
            if let Some(parent) = live_path.parent() {
                fsync_dir(parent)?;
            }
            return Ok(());
        }
        fs::rename(&preserved, live_path).with_context(|| {
            format!(
                "roll interrupted restore back to original {}",
                live_path.display()
            )
        })?;
        for (sidecar, preserved_sidecar) in live_sidecars.iter().zip(preserved_sidecars.iter()) {
            if preserved_sidecar.exists() {
                fs::rename(preserved_sidecar, sidecar)?;
            }
        }
        if let Some(parent) = live_path.parent() {
            fsync_dir(parent)?;
        }
    }

    if staging.is_file() {
        let (_bytes, staging_sha) = file_sha256(&staging)?;
        anyhow::ensure!(
            staging_sha == backup_sha256,
            "refusing native RxDB restore: unresolved staging {} belongs to a different backup; not deleting recovery files",
            staging.display()
        );
    } else {
        fs::copy(backup, &staging).with_context(|| {
            format!(
                "stage immutable backup {} at {}",
                backup.display(),
                staging.display()
            )
        })?;
        fsync_file(&staging)?;
        if let Some(parent) = live_path.parent() {
            fsync_dir(parent)?;
        }
    }

    if live_path.is_file() {
        fs::rename(live_path, &preserved).with_context(|| {
            format!(
                "preserve live RxDB store {} before restore",
                live_path.display()
            )
        })?;
        if let Some(parent) = live_path.parent() {
            fsync_dir(parent)?;
        }
        for (sidecar, preserved_sidecar) in live_sidecars.iter().zip(preserved_sidecars.iter()) {
            if sidecar.exists() {
                fs::rename(sidecar, preserved_sidecar)?;
            }
        }
        if let Some(parent) = live_path.parent() {
            fsync_dir(parent)?;
        }
    }
    adopt_orphaned_live_sidecars(&live_sidecars, &preserved_sidecars)?;
    remove_existing_files(&live_sidecars)?;
    if let Some(parent) = live_path.parent() {
        fsync_dir(parent)?;
    }
    if let Err(err) = fs::rename(&staging, live_path) {
        return Err(err).with_context(|| {
            format!(
                "replace native RxDB store {} from staged backup; original preserved at {}",
                live_path.display(),
                preserved.display()
            )
        });
    }
    if let Some(parent) = live_path.parent() {
        fsync_dir(parent)?;
    }
    let (_bytes, live_sha) = file_sha256(live_path)?;
    anyhow::ensure!(
        live_sha == backup_sha256,
        "restored native RxDB store hash does not match backup"
    );
    cleanup_completed_restore(&preserved, &preserved_sidecars, &staging)?;
    remove_existing_files(&live_sidecars)?;
    if let Some(parent) = live_path.parent() {
        fsync_dir(parent)?;
    }
    Ok(())
}

fn cleanup_completed_restore(
    preserved: &Path,
    preserved_sidecars: &[PathBuf],
    staging: &Path,
) -> anyhow::Result<()> {
    for path in preserved_sidecars
        .iter()
        .cloned()
        .chain([preserved.to_path_buf(), staging.to_path_buf()])
    {
        if path.exists() {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

fn adopt_orphaned_live_sidecars(
    live_sidecars: &[PathBuf],
    preserved_sidecars: &[PathBuf],
) -> anyhow::Result<()> {
    for (live_side, preserved_side) in live_sidecars.iter().zip(preserved_sidecars.iter()) {
        if !live_side.exists() {
            continue;
        }
        if preserved_side.exists() {
            anyhow::bail!(
                "refusing native RxDB restore: unresolved sidecar evidence at {} and {}; not deleting recovery files",
                live_side.display(),
                preserved_side.display()
            );
        }
        fs::rename(live_side, preserved_side).with_context(|| {
            format!(
                "attach leftover sidecar {} to preserved store {}",
                live_side.display(),
                preserved_side.display()
            )
        })?;
    }
    Ok(())
}

fn remove_existing_files(paths: &[PathBuf]) -> anyhow::Result<()> {
    for path in paths {
        if path.exists() {
            fs::remove_file(path)
                .with_context(|| format!("remove leftover restore file {}", path.display()))?;
        }
    }
    Ok(())
}

fn checkpoint_live_store(conn: &Connection, live_path: &Path) -> anyhow::Result<()> {
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .context("checkpoint native RxDB WAL before restore exchange")?;
    fsync_file(live_path)?;
    if let Some(parent) = live_path.parent() {
        fsync_dir(parent)?;
    }
    Ok(())
}

fn invoke_after_sqlite_release_hook() {
    #[cfg(test)]
    {
        if let Ok(mut hook) = AFTER_SQLITE_RELEASE.lock() {
            if let Some(cb) = hook.take() {
                cb();
            }
        }
    }
}

fn fsync_dir(path: &Path) -> anyhow::Result<()> {
    let dir = File::open(path).with_context(|| format!("open dir {} for fsync", path.display()))?;
    dir.sync_all()
        .with_context(|| format!("fsync dir {}", path.display()))?;
    Ok(())
}

fn sqlite_sidecar_paths(database_path: &Path) -> Vec<PathBuf> {
    ["-wal", "-shm"]
        .into_iter()
        .map(|suffix| PathBuf::from(format!("{}{suffix}", database_path.display())))
        .collect()
}

fn paths_would_alias(source: &Path, dest: &Path) -> anyhow::Result<bool> {
    if source == dest {
        return Ok(true);
    }
    let source = fs::canonicalize(source)?;
    if dest.exists() {
        return Ok(fs::canonicalize(dest)? == source);
    }
    let Some(parent) = dest.parent() else {
        return Ok(false);
    };
    if !parent.exists() {
        return Ok(false);
    }
    let dest_name = dest
        .file_name()
        .context("backup destination missing name")?;
    Ok(fs::canonicalize(parent)?.join(dest_name) == source)
}

fn open_rxdb_sqlite(path: &Path) -> anyhow::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_secs(10))?;
    Ok(conn)
}

fn fsync_file(path: &Path) -> anyhow::Result<()> {
    let file = File::open(path).with_context(|| format!("open {} for fsync", path.display()))?;
    file.sync_all()
        .with_context(|| format!("fsync {}", path.display()))?;
    Ok(())
}

pub fn prepare_current_rxdb_targets_for_cutover(root: &Path) -> anyhow::Result<Value> {
    let spec = populated_store_fixture_spec()?;
    let database_path = rxdb_store_path(root);
    let conn = open_rxdb_sqlite(&database_path)?;
    let internal_table = "ctox_business_os___rxdb_internal__v0";
    conn.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS {} (
            id TEXT PRIMARY KEY,
            data TEXT NOT NULL,
            deleted INTEGER NOT NULL DEFAULT 0
        )",
        sqlite_quote_identifier(internal_table)
    ))?;
    let mut prepared = Vec::new();
    let mut seen = BTreeMap::new();
    if let Some(documents) = spec.get("documents").and_then(Value::as_array) {
        for document in documents {
            let collection = document
                .get("collection")
                .and_then(Value::as_str)
                .context("fixture document missing collection")?;
            seen.insert(
                collection.to_string(),
                expected_rxdb_collection_version(collection),
            );
        }
    }
    for (collection, version) in seen {
        let table = rxdb_collection_version_table_name(&collection, version);
        create_rxdb_document_table(&conn, &table)?;
        conn.execute(
            &format!(
                "INSERT OR REPLACE INTO {} (id, data, deleted) VALUES (?1, ?2, 0)",
                sqlite_quote_identifier(internal_table)
            ),
            params![
                format!("collection|{collection}-{version}"),
                json!({
                    "id": format!("collection|{collection}-{version}"),
                    "key": format!("{collection}-{version}"),
                    "context": "collection",
                    "data": {
                        "name": collection,
                        "version": version,
                        "schemaHash": "populated-store-recovery"
                    },
                    "_deleted": false
                })
                .to_string()
            ],
        )?;
        prepared.push(json!({
            "collection": collection,
            "version": version,
            "table": table
        }));
    }
    Ok(json!({ "ok": true, "targets": prepared }))
}

pub fn run_production_native_rxdb_cutover(root: &Path) -> anyhow::Result<Value> {
    let started = std::time::Instant::now();
    let _lock = acquire_native_rxdb_peer_lock(root)?;
    let in_progress = mark_cutover_in_progress(root)?;
    let prepared = prepare_current_rxdb_targets_for_cutover(root)?;
    let migrated = migrate_additive_native_rxdb_collection_versions(root)?;
    let repaired = repair_stale_rxdb_collection_schema_versions(root)?;
    let receipt = record_native_rxdb_cutover_receipt(root)?;
    Ok(json!({
        "ok": true,
        "kind": "ctox.native_rxdb.production_cutover.v1",
        "in_progress": in_progress,
        "prepared": prepared,
        "migrated": migrated,
        "repaired": repaired,
        "receipt": receipt,
        "elapsed_ms": started.elapsed().as_secs_f64() * 1000.0
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn fixture_root() -> anyhow::Result<tempfile::TempDir> {
        let root = tempfile::tempdir()?;
        fs::create_dir_all(root.path().join("runtime"))?;
        Ok(root)
    }

    #[test]
    fn supported_historical_versions_require_a_complete_migration_chain() -> anyhow::Result<()> {
        let supported = supported_historical_rxdb_versions()?;
        assert_eq!(
            supported.pointer("/collections/business_commands/current_version"),
            Some(&json!(2))
        );
        assert_eq!(
            supported.pointer("/collections/business_commands/complete_chain"),
            Some(&json!(true))
        );
        let historical = supported
            .pointer("/collections/business_commands/supported_historical_source_versions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(historical.iter().any(|value| value.as_i64() == Some(0)));
        assert!(historical.iter().any(|value| value.as_i64() == Some(1)));
        let only_step_two = BTreeSet::from([2_i64]);
        assert_eq!(
            supported_source_versions_for_chain(2, &only_step_two),
            vec![1]
        );
        assert!(!complete_migration_chain(2, &only_step_two));
        Ok(())
    }

    #[test]
    fn unsupported_historical_version_is_refused() {
        let supported = supported_historical_rxdb_versions().expect("supported versions");
        let error = ensure_historical_version_is_supported(&supported, "business_commands", 3)
            .expect_err("v3 is not declared");
        assert!(error
            .to_string()
            .contains("unsupported historical collection"));
    }

    #[test]
    fn materialize_refuses_existing_store_or_receipt() -> anyhow::Result<()> {
        let root = fixture_root()?;
        materialize_supported_historical_rxdb_fixture(root.path())?;
        let error = materialize_supported_historical_rxdb_fixture(root.path())
            .expect_err("second materialize must refuse");
        assert!(
            error.to_string().contains("fresh isolated root"),
            "unexpected error: {error:#}"
        );
        Ok(())
    }

    #[test]
    fn backup_refuses_existing_destination_and_source_alias() -> anyhow::Result<()> {
        let root = fixture_root()?;
        materialize_supported_historical_rxdb_fixture(root.path())?;
        let backup_path = default_native_rxdb_immutable_backup_path(root.path());
        backup_native_rxdb_immutable_store(root.path(), Some(&backup_path))?;
        let error = backup_native_rxdb_immutable_store(root.path(), Some(&backup_path))
            .expect_err("second backup must refuse overwrite");
        assert!(
            error.to_string().contains("overwrite existing immutable"),
            "unexpected error: {error:#}"
        );
        let alias_error =
            backup_native_rxdb_immutable_store(root.path(), Some(&rxdb_store_path(root.path())))
                .expect_err("alias backup must refuse");
        assert!(
            alias_error.to_string().contains("aliases the live store")
                || alias_error.to_string().contains("overwrite existing"),
            "unexpected alias error: {alias_error:#}"
        );
        Ok(())
    }

    #[test]
    fn restore_without_provenance_fails_closed() -> anyhow::Result<()> {
        let root = fixture_root()?;
        materialize_supported_historical_rxdb_fixture(root.path())?;
        let foreign = root.path().join("runtime/foreign-backup.sqlite3");
        fs::copy(rxdb_store_path(root.path()), &foreign)?;
        let error = restore_native_rxdb_immutable_backup(root.path(), &foreign)
            .expect_err("restore without provenance must fail");
        assert!(
            error
                .to_string()
                .contains("no pinned immutable backup provenance"),
            "unexpected error: {error:#}"
        );
        Ok(())
    }

    #[test]
    fn restore_mismatched_backup_fails_closed() -> anyhow::Result<()> {
        let root = fixture_root()?;
        materialize_supported_historical_rxdb_fixture(root.path())?;
        let backup_path = default_native_rxdb_immutable_backup_path(root.path());
        backup_native_rxdb_immutable_store(root.path(), Some(&backup_path))?;
        let other = root.path().join("runtime/other-backup.sqlite3");
        fs::copy(&backup_path, &other)?;
        let conn = Connection::open(&other)?;
        conn.execute("CREATE TABLE mismatch (id INTEGER)", [])?;
        drop(conn);
        let error = restore_native_rxdb_immutable_backup(root.path(), &other)
            .expect_err("mismatched backup must fail");
        assert!(
            error.to_string().contains("backup identity does not match"),
            "unexpected error: {error:#}"
        );
        Ok(())
    }

    #[test]
    fn populated_store_cutover_preserves_ids_history_hashes_and_deletion_markers(
    ) -> anyhow::Result<()> {
        let root = fixture_root()?;
        materialize_supported_historical_rxdb_fixture(root.path())?;
        let before = native_rxdb_store_inventory(root.path())?;
        let cutover = run_production_native_rxdb_cutover(root.path())?;
        assert_eq!(cutover["ok"], true);
        assert_eq!(cutover["migrated"]["ok"], true);
        assert_eq!(cutover["repaired"]["ok"], true);
        let after = native_rxdb_store_inventory(root.path())?;
        compare_populated_store_inventories(&before, &after)?;

        let conn = Connection::open(rxdb_store_path(root.path()))?;
        assert!(!sqlite_table_exists(
            &conn,
            &rxdb_collection_version_table_name("business_commands", 0)
        )?);
        let commands = rxdb_collection_version_table_name("business_commands", 2);
        let inbound: String = conn.query_row(
            &format!(
                "SELECT json_extract(data, '$.inbound_channel') FROM {} WHERE id = 'cmd-history-001'",
                sqlite_quote_identifier(&commands)
            ),
            [],
            |row| row.get(0),
        )?;
        assert_eq!(inbound, "ctox");
        let deleted: i64 = conn.query_row(
            &format!(
                "SELECT deleted FROM {} WHERE id = 'cmd-deleted-001'",
                sqlite_quote_identifier(&commands)
            ),
            [],
            |row| row.get(0),
        )?;
        assert_eq!(deleted, 1);
        let pending: String = conn.query_row(
            &format!(
                "SELECT json_extract(data, '$.status') FROM {} WHERE id = 'cmd-pending-001'",
                sqlite_quote_identifier(&commands)
            ),
            [],
            |row| row.get(0),
        )?;
        assert_eq!(pending, "pending_sync");
        let hash: String = conn.query_row(
            &format!(
                "SELECT json_extract(data, '$.content_hash') FROM {} WHERE id = 'file-attachment-001'",
                sqlite_quote_identifier(&rxdb_collection_version_table_name("desktop_files", 0))
            ),
            [],
            |row| row.get(0),
        )?;
        assert_eq!(
            hash,
            "70b24f43af4f6268646054f87b87b2509adfd9c7e4f7198bd03bf6c281fb01b0"
        );
        Ok(())
    }

    #[test]
    fn interrupted_cutover_restores_from_immutable_backup() -> anyhow::Result<()> {
        let root = fixture_root()?;
        materialize_supported_historical_rxdb_fixture(root.path())?;
        let backup_path = default_native_rxdb_immutable_backup_path(root.path());
        let backup = backup_native_rxdb_immutable_store(root.path(), Some(&backup_path))?;
        let original_sha = backup["sha256"].as_str().unwrap().to_string();
        mark_cutover_in_progress(root.path())?;
        prepare_current_rxdb_targets_for_cutover(root.path())?;
        migrate_additive_native_rxdb_collection_versions(root.path())?;
        let in_progress = durable_cutover_state(root.path(), None)?;
        assert_eq!(
            in_progress
                .as_ref()
                .and_then(|value| value.get("phase"))
                .and_then(Value::as_str),
            Some(CUTOVER_PHASE_IN_PROGRESS)
        );
        restore_native_rxdb_immutable_backup(root.path(), &backup_path)?;
        let (bytes, sha256) = file_sha256(&rxdb_store_path(root.path()))?;
        assert!(bytes > 0);
        let restored = native_rxdb_store_inventory(root.path())?;
        assert_eq!(restored["file_sha256"], original_sha);
        let conn = Connection::open(rxdb_store_path(root.path()))?;
        assert!(sqlite_table_exists(
            &conn,
            &rxdb_collection_version_table_name("business_commands", 0)
        )?);
        Ok(())
    }

    #[test]
    fn unsafe_rollback_after_accepted_writes_fails_closed() -> anyhow::Result<()> {
        let root = fixture_root()?;
        materialize_supported_historical_rxdb_fixture(root.path())?;
        let backup_path = default_native_rxdb_immutable_backup_path(root.path());
        backup_native_rxdb_immutable_store(root.path(), Some(&backup_path))?;
        run_production_native_rxdb_cutover(root.path())?;
        let conn = Connection::open(rxdb_store_path(root.path()))?;
        let table = rxdb_collection_version_table_name("business_commands", 2);
        conn.execute(
            &format!(
                "INSERT INTO {} (id, revision, deleted, lastWriteTime, data)
                 VALUES ('cmd-post-cutover-001', '1-new', 0, 1800000000000, ?1)",
                sqlite_quote_identifier(&table)
            ),
            [json!({
                "id": "cmd-post-cutover-001",
                "command_id": "cmd-post-cutover-001",
                "module": "ctox",
                "command_type": "business_os.test",
                "status": "accepted",
                "updated_at_ms": 1800000000000u64
            })
            .to_string()],
        )?;
        drop(conn);
        let error = restore_native_rxdb_immutable_backup(root.path(), &backup_path)
            .expect_err("post-cutover restore must fail closed");
        assert!(
            error
                .to_string()
                .contains("post-cutover writes were accepted"),
            "unexpected restore error: {error:#}"
        );
        let conn = Connection::open(rxdb_store_path(root.path()))?;
        let count: i64 = conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM {} WHERE id = 'cmd-post-cutover-001'",
                sqlite_quote_identifier(&table)
            ),
            [],
            |row| row.get(0),
        )?;
        assert_eq!(count, 1);
        Ok(())
    }

    #[test]
    fn cutover_receipt_is_write_once_so_restart_cannot_reset_restore_baseline() -> anyhow::Result<()>
    {
        let root = fixture_root()?;
        materialize_supported_historical_rxdb_fixture(root.path())?;
        let backup_path = default_native_rxdb_immutable_backup_path(root.path());
        backup_native_rxdb_immutable_store(root.path(), Some(&backup_path))?;
        run_production_native_rxdb_cutover(root.path())?;
        let first = native_rxdb_cutover_receipt_document(root.path())?;
        let conn = Connection::open(rxdb_store_path(root.path()))?;
        let table = rxdb_collection_version_table_name("business_commands", 2);
        conn.execute(
            &format!(
                "INSERT INTO {} (id, revision, deleted, lastWriteTime, data)
                 VALUES ('cmd-after-restart-001', '1-new', 0, 1800000000001, ?1)",
                sqlite_quote_identifier(&table)
            ),
            [json!({
                "id": "cmd-after-restart-001",
                "command_id": "cmd-after-restart-001",
                "module": "ctox",
                "status": "accepted"
            })
            .to_string()],
        )?;
        drop(conn);
        let second = record_native_rxdb_cutover_receipt(root.path())?;
        assert_eq!(
            first.get("inventory_sha256"),
            second.get("inventory_sha256")
        );
        let error = restore_native_rxdb_immutable_backup(root.path(), &backup_path)
            .expect_err("restart must not reopen rollback");
        assert!(
            error
                .to_string()
                .contains("post-cutover writes were accepted"),
            "unexpected restore error: {error:#}"
        );
        Ok(())
    }

    #[test]
    fn lost_receipt_json_after_accepted_writes_still_fails_closed() -> anyhow::Result<()> {
        let root = fixture_root()?;
        materialize_supported_historical_rxdb_fixture(root.path())?;
        let backup_path = default_native_rxdb_immutable_backup_path(root.path());
        backup_native_rxdb_immutable_store(root.path(), Some(&backup_path))?;
        run_production_native_rxdb_cutover(root.path())?;
        let conn = Connection::open(rxdb_store_path(root.path()))?;
        let table = rxdb_collection_version_table_name("business_commands", 2);
        conn.execute(
            &format!(
                "INSERT INTO {} (id, revision, deleted, lastWriteTime, data)
                 VALUES (cmd-lost-receipt-001, 1-new, 0, 1800000000002, ?1)",
                sqlite_quote_identifier(&table)
            ),
            [json!({"id": "cmd-lost-receipt-001", "status": "accepted"}).to_string()],
        )?;
        drop(conn);
        fs::remove_file(native_rxdb_cutover_receipt_path(root.path()))?;
        let error = restore_native_rxdb_immutable_backup(root.path(), &backup_path)
            .expect_err("deleted receipt json must not reopen rollback");
        assert!(
            error
                .to_string()
                .contains("post-cutover writes were accepted"),
            "unexpected restore error: {error:#}"
        );
        Ok(())
    }

    #[test]
    fn diverged_live_store_without_durable_cutover_state_fails_closed() -> anyhow::Result<()> {
        let root = fixture_root()?;
        materialize_supported_historical_rxdb_fixture(root.path())?;
        let backup_path = default_native_rxdb_immutable_backup_path(root.path());
        backup_native_rxdb_immutable_store(root.path(), Some(&backup_path))?;
        prepare_current_rxdb_targets_for_cutover(root.path())?;
        migrate_additive_native_rxdb_collection_versions(root.path())?;
        let error = restore_native_rxdb_immutable_backup(root.path(), &backup_path)
            .expect_err("diverged live without cutover state must fail");
        assert!(
            error.to_string().contains("missing durable cutover state"),
            "unexpected restore error: {error:#}"
        );
        Ok(())
    }

    #[test]
    fn interrupted_restore_resumes_from_staging_without_deleting_live() -> anyhow::Result<()> {
        let root = fixture_root()?;
        materialize_supported_historical_rxdb_fixture(root.path())?;
        let backup_path = default_native_rxdb_immutable_backup_path(root.path());
        let backup = backup_native_rxdb_immutable_store(root.path(), Some(&backup_path))?;
        let live = rxdb_store_path(root.path());
        let staging = live.with_extension("sqlite3.restore-staging");
        fs::copy(&backup_path, &staging)?;
        assert!(live.is_file());
        replace_sqlite_store(&live, &backup_path)?;
        let restored = native_rxdb_store_inventory(root.path())?;
        assert_eq!(restored["file_sha256"], backup["sha256"]);
        assert!(!staging.exists());
        Ok(())
    }

    #[test]
    fn interrupted_restore_completes_when_live_was_moved_aside() -> anyhow::Result<()> {
        let root = fixture_root()?;
        materialize_supported_historical_rxdb_fixture(root.path())?;
        let backup_path = default_native_rxdb_immutable_backup_path(root.path());
        let backup = backup_native_rxdb_immutable_store(root.path(), Some(&backup_path))?;
        let live = rxdb_store_path(root.path());
        let staging = live.with_extension("sqlite3.restore-staging");
        let preserved = live.with_extension("sqlite3.pre-restore");
        fs::copy(&backup_path, &staging)?;
        fs::rename(&live, &preserved)?;
        replace_sqlite_store(&live, &backup_path)?;
        assert!(live.is_file());
        assert!(!preserved.exists());
        let restored = native_rxdb_store_inventory(root.path())?;
        assert_eq!(restored["file_sha256"], backup["sha256"]);
        Ok(())
    }

    #[test]
    fn unresolved_restore_evidence_is_not_deleted() -> anyhow::Result<()> {
        let root = fixture_root()?;
        materialize_supported_historical_rxdb_fixture(root.path())?;
        let backup_path = default_native_rxdb_immutable_backup_path(root.path());
        backup_native_rxdb_immutable_store(root.path(), Some(&backup_path))?;
        run_production_native_rxdb_cutover(root.path())?;
        let live = rxdb_store_path(root.path());
        let preserved = live.with_extension("sqlite3.pre-restore");
        fs::copy(&live, &preserved)?;
        let error = replace_sqlite_store(&live, &backup_path)
            .expect_err("conflicted restore evidence must fail closed");
        assert!(
            error.to_string().contains("unresolved restore evidence"),
            "unexpected error: {error:#}"
        );
        assert!(live.is_file());
        assert!(preserved.is_file());
        Ok(())
    }

    #[test]
    fn restore_rejects_concurrent_peer_and_sqlite_writers() -> anyhow::Result<()> {
        let root = fixture_root()?;
        materialize_supported_historical_rxdb_fixture(root.path())?;
        let backup_path = default_native_rxdb_immutable_backup_path(root.path());
        backup_native_rxdb_immutable_store(root.path(), Some(&backup_path))?;
        run_production_native_rxdb_cutover(root.path())?;
        let guard = acquire_exclusive_native_rxdb_writer(root.path())?;
        let root_buf = root.path().to_path_buf();
        let backup_buf = backup_path.clone();
        let handle = std::thread::spawn(move || {
            restore_native_rxdb_immutable_backup(&root_buf, &backup_buf)
        });
        let error = handle
            .join()
            .expect("concurrent restore thread")
            .expect_err("concurrent restore must fail");
        assert!(
            error.to_string().contains("active native peer holds")
                || error.to_string().contains("exclusive writer")
                || error.to_string().contains("busy"),
            "unexpected concurrent restore error: {error:#}"
        );
        let writer = Connection::open(rxdb_store_path(root.path()))?;
        writer.busy_timeout(Duration::from_millis(50))?;
        let busy = writer.execute_batch("BEGIN EXCLUSIVE;");
        assert!(
            busy.is_err(),
            "concurrent sqlite writer must be rejected while exclusive restore guard is held: {busy:?}"
        );
        drop(guard);
        Ok(())
    }

    fn enable_populated_wal(root: &Path) -> anyhow::Result<PathBuf> {
        let live = rxdb_store_path(root);
        let conn = Connection::open(&live)?;
        let mode: String = conn.query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))?;
        anyhow::ensure!(
            mode.eq_ignore_ascii_case("wal"),
            "expected WAL journal, got {mode}"
        );
        let table = rxdb_collection_version_table_name("desktop_files", 0);
        conn.execute(
            &format!(
                "UPDATE {} SET lastWriteTime = lastWriteTime + 1",
                sqlite_quote_identifier(&table)
            ),
            [],
        )?;
        drop(conn);
        let wal = PathBuf::from(format!("{}-wal", live.display()));
        anyhow::ensure!(wal.is_file(), "expected populated WAL {}", wal.display());
        Ok(wal)
    }

    #[test]
    fn restore_holds_peer_lock_after_sqlite_release() -> anyhow::Result<()> {
        let root = fixture_root()?;
        materialize_supported_historical_rxdb_fixture(root.path())?;
        let backup_path = default_native_rxdb_immutable_backup_path(root.path());
        backup_native_rxdb_immutable_store(root.path(), Some(&backup_path))?;
        let root_for_hook = root.path().to_path_buf();
        *AFTER_SQLITE_RELEASE.lock().expect("hook lock") = Some(Box::new(move || {
            let peer = acquire_native_peer_process_lock(&root_for_hook)
                .expect("peer lock check during exchange");
            assert!(
                peer.is_none(),
                "legitimate native peer writer must honor the retained peer lock after sqlite release"
            );
            let exclusive = acquire_exclusive_native_rxdb_writer(&root_for_hook);
            assert!(
                exclusive.is_err(),
                "exclusive restore writer must fail while peer lock is held after sqlite release: {exclusive:?}"
            );
        }));
        restore_native_rxdb_immutable_backup(root.path(), &backup_path)?;
        Ok(())
    }

    #[test]
    fn public_restore_detaches_leftover_wal_after_live_was_renamed() -> anyhow::Result<()> {
        let root = fixture_root()?;
        materialize_supported_historical_rxdb_fixture(root.path())?;
        let backup_path = default_native_rxdb_immutable_backup_path(root.path());
        let backup = backup_native_rxdb_immutable_store(root.path(), Some(&backup_path))?;
        mark_cutover_in_progress(root.path())?;
        let wal = enable_populated_wal(root.path())?;
        let live = rxdb_store_path(root.path());
        let staging = live.with_extension("sqlite3.restore-staging");
        let preserved = live.with_extension("sqlite3.pre-restore");
        fs::copy(&backup_path, &staging)?;
        fs::rename(&live, &preserved)?;
        anyhow::ensure!(wal.is_file(), "crash left live-named WAL in place");
        restore_native_rxdb_immutable_backup(root.path(), &backup_path)?;
        assert!(live.is_file());
        assert!(
            !wal.exists(),
            "stale live WAL must not attach to restored store"
        );
        assert!(!preserved.exists());
        let restored = native_rxdb_store_inventory(root.path())?;
        assert_eq!(restored["file_sha256"], backup["sha256"]);
        Ok(())
    }

    #[test]
    fn public_restore_strips_wal_when_live_already_matches_backup() -> anyhow::Result<()> {
        let root = fixture_root()?;
        materialize_supported_historical_rxdb_fixture(root.path())?;
        let backup_path = default_native_rxdb_immutable_backup_path(root.path());
        let backup = backup_native_rxdb_immutable_store(root.path(), Some(&backup_path))?;
        mark_cutover_in_progress(root.path())?;
        let wal = enable_populated_wal(root.path())?;
        let live = rxdb_store_path(root.path());
        let preserved = live.with_extension("sqlite3.pre-restore");
        fs::copy(&live, &preserved)?;
        fs::copy(&backup_path, &live)?;
        anyhow::ensure!(wal.is_file(), "matching-backup crash left live-named WAL");
        restore_native_rxdb_immutable_backup(root.path(), &backup_path)?;
        assert!(
            !wal.exists(),
            "matching-backup restore must detach leftover WAL"
        );
        let restored = native_rxdb_store_inventory(root.path())?;
        assert_eq!(restored["file_sha256"], backup["sha256"]);
        Ok(())
    }
}
