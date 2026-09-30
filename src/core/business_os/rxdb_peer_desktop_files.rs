// Origin: CTOX
// License: Apache-2.0

use super::desktop_files::{
    expected_desktop_file_chunk_total, upsert_desktop_file_with_parent, DesktopFileContentPolicy,
    DesktopFileIndexCandidate, DesktopFileIndexProjectionStamp, DesktopFileIndexScan,
    DesktopFileScanBudget, DesktopFileScanRoot, DESKTOP_FILE_CHUNK_CACHE_STATE_ID,
    DESKTOP_FILE_CHUNK_CACHE_STATE_TABLE, DESKTOP_FILE_CHUNK_CLEANUP_SCAN_LIMIT,
    DESKTOP_FILE_CHUNK_DECODED_SIZE, DESKTOP_FILE_CHUNK_RETAIN_GENERATIONS,
    DESKTOP_FILE_CONTENT_HASH_SCHEME, DESKTOP_FILE_INDEX_MAINTENANCE_CHUNK_DELETE_LIMIT,
    DESKTOP_FILE_INDEX_MAINTENANCE_FILE_LIMIT,
    DESKTOP_FILE_INDEX_MAINTENANCE_FILE_TOMBSTONE_DELETE_LIMIT,
    DESKTOP_FILE_INDEX_UNSAFE_TOMBSTONE_RETENTION_SECS, DESKTOP_FILE_SCAN_MAX_FILES,
};
use super::rxdb_peer::{
    bulk_upsert_or_error, chunk_id_prefix_bounds, collect_files_bounded, collect_files_unbounded,
    demand_file_chunk_rows_for_key_from_sqlite, demand_file_source_error,
    desktop_file_index_projection_stamp, ensure_ctox_desktop_folder_path, hex_sha256,
    is_ctox_internal_desktop_scan_root, is_ctox_internal_path_layout, maintenance_revision,
    metadata_modified_at_ms, now_ms, should_eager_sync_file, sqlite_pragma_u64,
    sqlite_table_exists, sqlite_table_has_column, DemandFileFetchRequestStats,
    DesktopFileChunkCacheCandidate, DesktopFileChunkCacheConfig, DesktopFileChunkCacheEviction,
    DesktopFileChunkCacheState, DesktopFileDemandMetadata, DesktopFileIndexMaintenanceStats,
    NATIVE_RXDB_WRITE_LOCK,
};
use super::store;
use crate::mission::channels;
use anyhow::Context;
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection, OpenFlags, OptionalExtension};
use rxdb::plugins::replication_webrtc::file_fetch_handler::FileRange;
use rxdb::rx_collection::RxCollection;
use rxdb::rx_database::RxDatabase;
use serde_json::{json, Value};
use sha2::Digest;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn desktop_file_chunk_index_window(size_bytes: u64, range: Option<&FileRange>) -> (u64, u64) {
    let expected_total = expected_desktop_file_chunk_total(size_bytes);
    let Some(range) = range else {
        return (0, expected_total);
    };
    if range.length == 0 || range.offset >= size_bytes {
        return (0, 0);
    }
    let end = range.offset.saturating_add(range.length).min(size_bytes);
    if end <= range.offset {
        return (0, 0);
    }
    let start_idx = range.offset / DESKTOP_FILE_CHUNK_DECODED_SIZE;
    let end_idx = end
        .saturating_sub(1)
        .checked_div(DESKTOP_FILE_CHUNK_DECODED_SIZE)
        .unwrap_or(0)
        .saturating_add(1)
        .min(expected_total);
    (start_idx.min(expected_total), end_idx)
}

pub(super) fn active_desktop_file_chunk_rows_from_sqlite(
    root: &Path,
    file_id: &str,
    range: Option<&FileRange>,
    stats: &mut DemandFileFetchRequestStats,
) -> rxdb::rx_error::RxResult<(Vec<Value>, u64)> {
    let database_path = store::rxdb_store_path(root);
    if !database_path.exists() {
        return Ok((Vec::new(), 0));
    }
    let conn = Connection::open_with_flags(&database_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|err| {
            demand_file_source_error(format!(
                "open RxDB store {} for desktop file fetch: {err}",
                database_path.display()
            ))
        })?;
    if !sqlite_table_exists(&conn, "ctox_business_os__desktop_files__v0")
        .map_err(|err| demand_file_source_error(format!("inspect desktop_files table: {err}")))?
        || !sqlite_table_exists(&conn, "ctox_business_os__desktop_file_chunks__v0").map_err(
            |err| demand_file_source_error(format!("inspect desktop_file_chunks table: {err}")),
        )?
    {
        return Ok((Vec::new(), 0));
    }
    let Some(metadata) = active_desktop_file_metadata_from_sqlite(&conn, file_id)? else {
        return Ok((Vec::new(), 0));
    };
    let index_window = desktop_file_chunk_index_window(metadata.size_bytes, range);
    if index_window.0 >= index_window.1 {
        return Ok((Vec::new(), 0));
    }
    let loaded_base_offset = index_window
        .0
        .saturating_mul(DESKTOP_FILE_CHUNK_DECODED_SIZE);
    let canonical = desktop_file_chunk_rows_by_id_from_sqlite(
        &conn,
        file_id,
        &metadata.generation_id,
        metadata.size_bytes,
        index_window,
        stats,
    )?;
    let expected_total = expected_desktop_file_chunk_total(metadata.size_bytes);
    let expected_range_total = index_window.1.saturating_sub(index_window.0);
    let canonical = dedupe_desktop_file_chunks_by_idx(
        canonical,
        expected_total,
        index_window,
        metadata.size_bytes,
        &metadata.content_hash,
    );
    if u64::try_from(canonical.len()).unwrap_or_default() >= expected_range_total {
        return Ok((canonical, loaded_base_offset));
    }

    let fallback = demand_file_chunk_rows_for_key_from_sqlite(
        root,
        "desktop_file_chunks",
        "file_id",
        file_id,
        Some(metadata.generation_id.as_str()),
        Some(index_window),
        stats,
    )?
    .into_iter()
    .filter(|chunk| {
        chunk.get("generation_id").and_then(Value::as_str) == Some(metadata.generation_id.as_str())
            && chunk
                .get("idx")
                .and_then(Value::as_u64)
                .is_some_and(|idx| idx >= index_window.0 && idx < index_window.1)
    })
    .collect::<Vec<_>>();
    let fallback = dedupe_desktop_file_chunks_by_idx(
        fallback,
        expected_total,
        index_window,
        metadata.size_bytes,
        &metadata.content_hash,
    );
    if u64::try_from(fallback.len()).unwrap_or_default() >= expected_range_total {
        return Ok((fallback, loaded_base_offset));
    }

    if !metadata.content_hash.is_empty() {
        let equivalent = equivalent_desktop_file_chunk_rows_from_sqlite(
            &conn,
            file_id,
            &metadata,
            index_window,
            stats,
        )?;
        if u64::try_from(equivalent.len()).unwrap_or_default() >= expected_range_total {
            return Ok((equivalent, loaded_base_offset));
        }
    }
    Ok((canonical, loaded_base_offset))
}

fn dedupe_desktop_file_chunks_by_idx(
    chunks: Vec<Value>,
    expected_total: u64,
    index_window: (u64, u64),
    size_bytes: u64,
    content_hash: &str,
) -> Vec<Value> {
    let mut by_idx: BTreeMap<u64, Value> = BTreeMap::new();
    for chunk in chunks {
        let Some(idx) = chunk.get("idx").and_then(Value::as_u64) else {
            continue;
        };
        if idx < index_window.0 || idx >= index_window.1 || idx >= expected_total {
            continue;
        }
        if desktop_file_chunk_stream_score(&chunk, expected_total, size_bytes, content_hash)
            .is_none()
        {
            continue;
        }
        match by_idx.get(&idx) {
            Some(previous)
                if desktop_file_chunk_stream_score(
                    &chunk,
                    expected_total,
                    size_bytes,
                    content_hash,
                ) >= desktop_file_chunk_stream_score(
                    previous,
                    expected_total,
                    size_bytes,
                    content_hash,
                ) => {}
            _ => {
                by_idx.insert(idx, chunk);
            }
        }
    }
    by_idx.into_values().collect()
}

fn desktop_file_chunk_stream_score(
    chunk: &Value,
    expected_total: u64,
    size_bytes: u64,
    content_hash: &str,
) -> Option<u8> {
    if chunk
        .get("encoding")
        .and_then(Value::as_str)
        .unwrap_or("base64")
        != "base64"
    {
        return None;
    }
    if !content_hash.is_empty()
        && chunk
            .get("content_hash")
            .and_then(Value::as_str)
            .is_some_and(|hash| hash != content_hash)
    {
        return None;
    }
    let data = chunk.get("data").and_then(Value::as_str).unwrap_or("");
    if size_bytes > 0 && data.is_empty() {
        return None;
    }
    if let Some(size) = chunk.get("size_bytes").and_then(Value::as_u64) {
        if size != data.len() as u64 {
            return None;
        }
    }
    if let Some(chunk_hash) = chunk.get("chunk_hash").and_then(Value::as_str) {
        if hex_sha256(data.as_bytes()) != chunk_hash {
            return None;
        }
    }
    let mut score = 0_u8;
    let chunk_total = chunk
        .get("total")
        .and_then(Value::as_u64)
        .unwrap_or(expected_total);
    if chunk_total != expected_total {
        score = score.saturating_add(if chunk_total > expected_total { 1 } else { 8 });
    }
    Some(score)
}

fn equivalent_desktop_file_chunk_rows_from_sqlite(
    conn: &Connection,
    file_id: &str,
    metadata: &DesktopFileDemandMetadata,
    index_window: (u64, u64),
    stats: &mut DemandFileFetchRequestStats,
) -> rxdb::rx_error::RxResult<Vec<Value>> {
    let expected_total = expected_desktop_file_chunk_total(metadata.size_bytes);
    let expected_range_total = index_window.1.saturating_sub(index_window.0);
    let mut stmt = conn
        .prepare(
            "SELECT id, data FROM ctox_business_os__desktop_files__v0 \
             WHERE id != ?1 AND COALESCE(deleted, 0) = 0",
        )
        .map_err(|err| {
            demand_file_source_error(format!("prepare equivalent file lookup: {err}"))
        })?;
    let rows = stmt
        .query_map([file_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|err| demand_file_source_error(format!("query equivalent files: {err}")))?;

    let mut candidates = Vec::new();
    for row in rows {
        let (candidate_id, raw) =
            row.map_err(|err| demand_file_source_error(format!("load equivalent file: {err}")))?;
        let value = serde_json::from_str::<Value>(&raw).map_err(|err| {
            demand_file_source_error(format!("decode equivalent file {candidate_id}: {err}"))
        })?;
        if value.get("content_state").and_then(Value::as_str) != Some("available") {
            continue;
        }
        if value.get("kind").and_then(Value::as_str).unwrap_or("file") != "file" {
            continue;
        }
        if value
            .get("content_hash")
            .and_then(Value::as_str)
            .map(str::trim)
            != Some(metadata.content_hash.as_str())
        {
            continue;
        }
        if value
            .get("size_bytes")
            .and_then(Value::as_u64)
            .unwrap_or_default()
            != metadata.size_bytes
        {
            continue;
        }
        let generation_id = value
            .get("content_generation_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if generation_id.is_empty() {
            continue;
        }
        candidates.push((candidate_id, generation_id));
    }

    for (candidate_id, generation_id) in candidates {
        let chunks = desktop_file_chunk_rows_by_id_from_sqlite(
            conn,
            &candidate_id,
            &generation_id,
            metadata.size_bytes,
            index_window,
            stats,
        )?;
        let chunks = dedupe_desktop_file_chunks_by_idx(
            chunks,
            expected_total,
            index_window,
            metadata.size_bytes,
            &metadata.content_hash,
        );
        if u64::try_from(chunks.len()).unwrap_or_default() >= expected_range_total {
            return Ok(chunks);
        }
    }

    Ok(Vec::new())
}

/// Immutable bytes/generation authority for native transfer admission. Reuse
/// the demand source's live-file predicate inside one read snapshot.
pub(super) fn desktop_file_transfer_metadata(
    root: &Path,
    file_id: &str,
) -> anyhow::Result<Option<DesktopFileDemandMetadata>> {
    let path = store::rxdb_store_path(root);
    if !path.is_file() {
        return Ok(None);
    }
    let mut conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(Duration::from_secs(5))?;
    let tx = conn.transaction()?;
    let Some(metadata) = active_desktop_file_metadata_from_sqlite(&tx, file_id)? else {
        return Ok(None);
    };
    let scheme: Option<String> = tx.query_row(
        "SELECT json_extract(data, '$.content_hash_scheme') FROM ctox_business_os__desktop_files__v0 WHERE id = ?1 AND COALESCE(deleted, 0) = 0",
        [file_id], |row| row.get(0),
    )?;
    if scheme.as_deref() != Some(DESKTOP_FILE_CONTENT_HASH_SCHEME) {
        return Ok(None);
    }
    tx.commit()?;
    Ok(Some(metadata))
}

fn active_desktop_file_metadata_from_sqlite(
    conn: &Connection,
    file_id: &str,
) -> rxdb::rx_error::RxResult<Option<DesktopFileDemandMetadata>> {
    let file_json = conn
        .query_row(
            "SELECT data FROM ctox_business_os__desktop_files__v0 \
             WHERE id = ?1 AND COALESCE(deleted, 0) = 0",
            [file_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|err| demand_file_source_error(format!("read desktop file {file_id}: {err}")))?;
    let Some(file_json) = file_json else {
        return Ok(None);
    };
    let file_row: Value = serde_json::from_str(&file_json).map_err(|err| {
        demand_file_source_error(format!("decode desktop file {file_id} metadata: {err}"))
    })?;
    if file_row.get("content_state").and_then(Value::as_str) != Some("available") {
        return Ok(None);
    }
    let generation_id = file_row
        .get("content_generation_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if generation_id.is_empty() {
        return Ok(None);
    }
    Ok(Some(DesktopFileDemandMetadata {
        generation_id: generation_id.to_string(),
        size_bytes: file_row
            .get("size_bytes")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        content_hash: file_row
            .get("content_hash")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }))
}

fn desktop_file_chunk_rows_by_id_from_sqlite(
    conn: &Connection,
    file_id: &str,
    generation_id: &str,
    size_bytes: u64,
    index_window: (u64, u64),
    stats: &mut DemandFileFetchRequestStats,
) -> rxdb::rx_error::RxResult<Vec<Value>> {
    let expected_total = expected_desktop_file_chunk_total(size_bytes);
    let mut rows = desktop_file_chunk_rows_by_row_id_from_sqlite(
        conn,
        file_id,
        generation_id,
        expected_total,
        index_window,
        stats,
    )?;
    let expected_range_total = index_window.1.saturating_sub(index_window.0);
    if u64::try_from(rows.len()).unwrap_or_default() >= expected_range_total {
        return Ok(rows);
    }
    let start_idx = i64::try_from(index_window.0).map_err(|err| {
        demand_file_source_error(format!(
            "desktop file chunk start overflow for {file_id}: {err}"
        ))
    })?;
    let end_idx = i64::try_from(index_window.1).map_err(|err| {
        demand_file_source_error(format!(
            "desktop file chunk end overflow for {file_id}: {err}"
        ))
    })?;
    let mut stmt = conn
        .prepare(
            "SELECT data FROM ctox_business_os__desktop_file_chunks__v0 \
             WHERE COALESCE(deleted, 0) = 0 \
               AND json_extract(data, '$.file_id') = ?1 \
               AND json_extract(data, '$.generation_id') = ?2 \
               AND json_extract(data, '$.idx') >= ?3 \
               AND json_extract(data, '$.idx') < ?4 \
             ORDER BY json_extract(data, '$.idx') ASC",
        )
        .map_err(|err| demand_file_source_error(format!("prepare chunk lookup: {err}")))?;
    let query_rows = stmt
        .query_map(params![file_id, generation_id, start_idx, end_idx], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|err| demand_file_source_error(format!("query desktop file chunks: {err}")))?;
    rows.reserve(usize::try_from(index_window.1.saturating_sub(index_window.0)).unwrap_or(0));
    for row in query_rows {
        let raw =
            row.map_err(|err| demand_file_source_error(format!("load desktop file chunk: {err}")))?;
        stats.rows_loaded = stats.rows_loaded.saturating_add(1);
        let value = serde_json::from_str::<Value>(&raw).map_err(|err| {
            demand_file_source_error(format!("decode desktop file chunk for {file_id}: {err}"))
        })?;
        rows.push(value);
    }
    rows.retain(|chunk| {
        chunk.get("file_id").and_then(Value::as_str) == Some(file_id)
            && chunk.get("generation_id").and_then(Value::as_str) == Some(generation_id)
            && chunk
                .get("idx")
                .and_then(Value::as_u64)
                .is_some_and(|idx| idx < expected_total)
    });
    Ok(rows)
}

fn desktop_file_chunk_rows_by_row_id_from_sqlite(
    conn: &Connection,
    file_id: &str,
    generation_id: &str,
    expected_total: u64,
    index_window: (u64, u64),
    stats: &mut DemandFileFetchRequestStats,
) -> rxdb::rx_error::RxResult<Vec<Value>> {
    const ROW_ID_BATCH_SIZE: u64 = 256;

    let start_idx = index_window.0.min(expected_total);
    let end_idx = index_window.1.min(expected_total);
    let mut rows =
        Vec::with_capacity(usize::try_from(end_idx.saturating_sub(start_idx)).unwrap_or_default());
    let mut batch_start = start_idx;
    while batch_start < end_idx {
        let batch_end = batch_start.saturating_add(ROW_ID_BATCH_SIZE).min(end_idx);
        let ids = (batch_start..batch_end)
            .map(|idx| SqlValue::Text(format!("{file_id}_{generation_id}_{idx}")))
            .collect::<Vec<_>>();
        let placeholders = vec!["?"; ids.len()].join(", ");
        let mut stmt = conn
            .prepare(&format!(
                "SELECT data FROM ctox_business_os__desktop_file_chunks__v0 \
                 WHERE COALESCE(deleted, 0) = 0 AND id IN ({placeholders})"
            ))
            .map_err(|err| {
                demand_file_source_error(format!("prepare exact chunk row-id lookup: {err}"))
            })?;
        let query_rows = stmt
            .query_map(params_from_iter(ids), |row| row.get::<_, String>(0))
            .map_err(|err| {
                demand_file_source_error(format!(
                    "query desktop file chunks by exact row id: {err}"
                ))
            })?;
        for row in query_rows {
            let raw = row.map_err(|err| {
                demand_file_source_error(format!("load desktop file chunk by exact row id: {err}"))
            })?;
            stats.rows_loaded = stats.rows_loaded.saturating_add(1);
            let value = serde_json::from_str::<Value>(&raw).map_err(|err| {
                demand_file_source_error(format!(
                    "decode desktop file chunk by exact row id for {file_id}: {err}"
                ))
            })?;
            rows.push(value);
        }
        batch_start = batch_end;
    }
    rows.retain(|chunk| {
        chunk.get("file_id").and_then(Value::as_str) == Some(file_id)
            && chunk.get("generation_id").and_then(Value::as_str) == Some(generation_id)
            && chunk.get("idx").and_then(Value::as_u64).is_some_and(|idx| {
                idx < expected_total && idx >= index_window.0 && idx < index_window.1
            })
    });
    Ok(rows)
}

pub(super) fn desktop_file_chunk_rows_for_file_id(
    root: &Path,
    file_id: &str,
) -> anyhow::Result<Vec<Value>> {
    const CHUNKS_TABLE: &str = "\"ctox_business_os__desktop_file_chunks__v0\"";

    let database_path = store::rxdb_store_path(root);
    if !database_path.exists() {
        return Ok(Vec::new());
    }
    let conn = Connection::open_with_flags(&database_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("open RxDB store {}", database_path.display()))?;
    if !sqlite_table_exists(&conn, "ctox_business_os__desktop_file_chunks__v0")? {
        return Ok(Vec::new());
    }
    let (chunk_id_lower, chunk_id_upper) = desktop_file_chunk_id_bounds(file_id);
    let mut stmt = conn.prepare(&format!(
        "SELECT data FROM {CHUNKS_TABLE}
         WHERE id >= ?1
           AND id < ?2
           AND COALESCE(deleted, 0) = 0
         LIMIT ?3"
    ))?;
    let rows = stmt.query_map(
        params![
            chunk_id_lower,
            chunk_id_upper,
            DESKTOP_FILE_CHUNK_CLEANUP_SCAN_LIMIT as i64
        ],
        |row| row.get::<_, String>(0),
    )?;
    let mut chunks = Vec::new();
    for row in rows {
        let raw = row?;
        let Ok(value) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        if value.get("file_id").and_then(Value::as_str) == Some(file_id) {
            chunks.push(value);
        }
    }
    Ok(chunks)
}

pub(super) async fn prune_desktop_file_chunk_generations(
    root: &Path,
    database: &Arc<RxDatabase>,
    file_id: &str,
    active_generation_id: &str,
) -> anyhow::Result<usize> {
    let chunks = database
        .collection("desktop_file_chunks")
        .context("desktop_file_chunks collection is not registered")?;
    let chunk_rows = desktop_file_chunk_rows_for_file_id(root, file_id)?;
    if chunk_rows.is_empty() {
        return Ok(0);
    }

    let mut latest_by_generation: HashMap<String, u64> = HashMap::new();
    for chunk in &chunk_rows {
        let generation = desktop_file_chunk_generation_key(chunk);
        let created_at = chunk
            .get("created_at_ms")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        latest_by_generation
            .entry(generation)
            .and_modify(|existing| *existing = (*existing).max(created_at))
            .or_insert(created_at);
    }

    if latest_by_generation.len() <= DESKTOP_FILE_CHUNK_RETAIN_GENERATIONS {
        return Ok(0);
    }

    let mut generations: Vec<(String, u64)> = latest_by_generation.into_iter().collect();
    generations.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));

    let mut keep = HashSet::from([active_generation_id.to_string()]);
    for (generation, _) in generations {
        if keep.len() >= DESKTOP_FILE_CHUNK_RETAIN_GENERATIONS {
            break;
        }
        keep.insert(generation);
    }

    let stale_chunks: Vec<Value> = chunk_rows
        .into_iter()
        .filter(|chunk| !keep.contains(&desktop_file_chunk_generation_key(chunk)))
        .filter(|chunk| chunk.get("id").and_then(Value::as_str).is_some())
        .collect();
    if stale_chunks.is_empty() {
        return Ok(0);
    }

    let removed = stale_chunks.len();
    let pruned_at_ms = now_ms();
    let mut pruned_chunks = Vec::with_capacity(stale_chunks.len());
    for mut chunk in stale_chunks {
        if let Some(object) = chunk.as_object_mut() {
            object.insert("data".to_string(), Value::String(String::new()));
            object.insert("size_bytes".to_string(), Value::from(0_u64));
            object.insert("_deleted".to_string(), Value::Bool(true));
            object.insert("pruned_at_ms".to_string(), Value::from(pruned_at_ms as u64));
            object.insert(
                "prune_reason".to_string(),
                Value::String("stale_generation".to_string()),
            );
        }
        pruned_chunks.push(chunk);
    }
    bulk_upsert_or_error(&chunks, pruned_chunks, "redact stale desktop file chunks").await?;
    Ok(removed)
}

fn desktop_file_chunk_generation_key(chunk: &Value) -> String {
    chunk
        .get("generation_id")
        .and_then(Value::as_str)
        .filter(|generation| !generation.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            let created_at = chunk
                .get("created_at_ms")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            format!("legacy_{created_at}")
        })
}

pub(super) async fn sync_desktop_file_index_with_database(
    root: &Path,
    database: &Arc<RxDatabase>,
) -> anyhow::Result<usize> {
    sync_desktop_file_scan_roots_with_database(root, database, desktop_file_scan_roots(root)).await
}

pub(super) async fn sync_desktop_file_index_with_database_if_changed(
    root: &Path,
    database: &Arc<RxDatabase>,
    last_projection_stamp: &mut Option<DesktopFileIndexProjectionStamp>,
) -> anyhow::Result<usize> {
    sync_desktop_file_scan_roots_with_database_if_changed(
        root,
        database,
        desktop_file_scan_roots(root),
        last_projection_stamp,
    )
    .await
}

pub(super) async fn sync_desktop_file_scan_roots_with_database(
    root: &Path,
    database: &Arc<RxDatabase>,
    scan_roots: Vec<DesktopFileScanRoot>,
) -> anyhow::Result<usize> {
    let scan = collect_desktop_file_index_scan(scan_roots).await?;
    sync_desktop_file_scan_with_database(root, database, scan).await
}

pub(super) async fn sync_desktop_file_scan_roots_with_database_unbounded(
    root: &Path,
    database: &Arc<RxDatabase>,
    scan_roots: Vec<DesktopFileScanRoot>,
) -> anyhow::Result<usize> {
    let scan = collect_desktop_file_index_scan_unbounded(scan_roots).await?;
    sync_desktop_file_scan_with_database(root, database, scan).await
}

async fn sync_desktop_file_scan_roots_with_database_if_changed(
    root: &Path,
    database: &Arc<RxDatabase>,
    scan_roots: Vec<DesktopFileScanRoot>,
    last_projection_stamp: &mut Option<DesktopFileIndexProjectionStamp>,
) -> anyhow::Result<usize> {
    let scan = collect_desktop_file_index_scan(scan_roots).await?;
    if last_projection_stamp.as_ref() == Some(&scan.stamp) {
        return Ok(0);
    }

    let projection_stamp = scan.stamp.clone();
    let indexed = sync_desktop_file_scan_with_database(root, database, scan).await?;
    *last_projection_stamp = Some(projection_stamp);
    Ok(indexed)
}

pub(super) async fn sync_desktop_file_scan_with_database(
    root: &Path,
    database: &Arc<RxDatabase>,
    scan: DesktopFileIndexScan,
) -> anyhow::Result<usize> {
    let may_mark_missing = desktop_file_scan_may_mark_missing(&scan);
    let mut seen_file_ids = HashSet::with_capacity(scan.candidates.len());
    let mut indexed = 0usize;

    // Acquire write lock specifically for the DB write iteration
    let _write_guard = NATIVE_RXDB_WRITE_LOCK.lock().await;
    for candidate in scan.candidates {
        let path = candidate.path;
        let metadata = match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => metadata,
            _ => continue,
        };
        let policy = if should_eager_sync_file(&path, &metadata) {
            DesktopFileContentPolicy::Eager
        } else {
            DesktopFileContentPolicy::Lazy
        };
        let file_id = desktop_file_id(&path);
        let (folder_components, virtual_path) =
            desktop_file_virtual_location(&candidate.scan_root, &path);
        let parent_id =
            ensure_ctox_desktop_folder_path(database, now_ms(), &folder_components).await?;
        if let Err(err) = upsert_desktop_file_with_parent(
            root,
            database,
            path.clone(),
            policy,
            false,
            parent_id,
            Some(virtual_path),
        )
        .await
        {
            eprintln!(
                "[business-os] failed to index desktop file {}: {err:#}",
                path.display()
            );
            continue;
        }
        seen_file_ids.insert(file_id);
        indexed += 1;
    }
    if may_mark_missing {
        mark_missing_scanned_desktop_files(root, database, &scan.scan_roots, &seen_file_ids)
            .await?;
    }
    Ok(indexed)
}

pub(super) fn desktop_file_scan_may_mark_missing(scan: &DesktopFileIndexScan) -> bool {
    !scan.stamp.truncated
}

pub(super) fn log_desktop_file_index_maintenance_stats(stats: &DesktopFileIndexMaintenanceStats) {
    eprintln!(
        "[business-os] desktop file index maintenance: tombstoned {} unsafe file(s), \
         removed {} unsafe chunk(s), {} stale chunk(s), {} deleted chunk tombstone(s), \
         {} unsafe file tombstone(s), evicted {} cached file(s), removed {} cache chunk(s) \
         ({} byte(s), live {} -> {}, pinned {}, over-quota pinned {}, checkpoint {}, vacuum {})",
        stats.tombstoned_unsafe_files,
        stats.removed_unsafe_chunks,
        stats.removed_stale_chunks,
        stats.removed_deleted_chunks,
        stats.removed_unsafe_file_tombstones,
        stats.evicted_cache_files,
        stats.removed_cache_chunks,
        stats.removed_cache_bytes,
        stats.cache_live_bytes_before,
        stats.cache_live_bytes_after,
        stats.cache_pinned_bytes,
        stats.cache_over_quota_pinned_bytes,
        stats.wal_checkpoint_ran,
        stats.vacuum_ran
    );
}

pub(super) async fn compact_desktop_file_index_store(
    root: &Path,
) -> anyhow::Result<DesktopFileIndexMaintenanceStats> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || compact_desktop_file_index_store_sync(&root, None))
        .await
        .context("join desktop file index maintenance")?
}

pub(super) fn compact_desktop_file_index_store_sync(
    root: &Path,
    home: Option<&Path>,
) -> anyhow::Result<DesktopFileIndexMaintenanceStats> {
    compact_desktop_file_index_store_sync_with_config(
        root,
        home,
        DesktopFileChunkCacheConfig::default(),
    )
}

pub(super) fn compact_desktop_file_index_store_sync_with_config(
    root: &Path,
    home: Option<&Path>,
    cache_config: DesktopFileChunkCacheConfig,
) -> anyhow::Result<DesktopFileIndexMaintenanceStats> {
    const FILES_TABLE: &str = "\"ctox_business_os__desktop_files__v0\"";
    const CHUNKS_TABLE: &str = "\"ctox_business_os__desktop_file_chunks__v0\"";

    let database_path = store::rxdb_store_path(root);
    if !database_path.exists() {
        return Ok(DesktopFileIndexMaintenanceStats::default());
    }
    let mut conn = Connection::open(&database_path)
        .with_context(|| format!("open RxDB store {}", database_path.display()))?;
    conn.busy_timeout(Duration::from_secs(10))
        .context("set RxDB maintenance busy timeout")?;
    let has_tables = sqlite_table_exists(&conn, "ctox_business_os__desktop_files__v0")?
        && sqlite_table_exists(&conn, "ctox_business_os__desktop_file_chunks__v0")?;
    if !has_tables {
        return Ok(DesktopFileIndexMaintenanceStats::default());
    }
    ensure_desktop_file_index_query_indexes(&conn)?;

    let unsafe_files = {
        let unsafe_candidates_sql = unsafe_desktop_file_index_candidates_sql(FILES_TABLE);
        let mut stmt = conn.prepare(&unsafe_candidates_sql)?;
        let rows = stmt.query_map(
            params![DESKTOP_FILE_INDEX_MAINTENANCE_FILE_LIMIT as i64],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )?;
        let mut unsafe_files = Vec::new();
        for row in rows {
            let (id, revision, data) = row?;
            let Ok(document) = serde_json::from_str::<Value>(&data) else {
                continue;
            };
            if desktop_file_index_document_is_unsafe(&document, home) {
                unsafe_files.push((id, revision, document));
                if unsafe_files.len() >= DESKTOP_FILE_INDEX_MAINTENANCE_FILE_LIMIT {
                    break;
                }
            }
        }
        unsafe_files
    };

    let tx = conn.transaction()?;
    let now = now_ms() as f64;
    let mut stats = DesktopFileIndexMaintenanceStats::default();
    for (file_id, revision, document) in &unsafe_files {
        let mut document = document.clone();
        let next_revision = maintenance_revision(
            document
                .get("_rev")
                .and_then(Value::as_str)
                .or(revision.as_deref()),
        );
        prepare_unsafe_desktop_file_tombstone(&mut document, &next_revision, now);
        let data = serde_json::to_string(&document)?;
        let changed = tx.execute(
            &format!(
                "UPDATE {FILES_TABLE}
                 SET revision = ?2, deleted = 1, lastWriteTime = ?3, data = ?4
                 WHERE id = ?1"
            ),
            params![file_id, next_revision, now, data],
        )?;
        if changed > 0 {
            stats.tombstoned_unsafe_files += changed;
        }
    }

    let mut remaining_chunk_delete_limit = DESKTOP_FILE_INDEX_MAINTENANCE_CHUNK_DELETE_LIMIT;
    for (file_id, _, _) in &unsafe_files {
        if remaining_chunk_delete_limit == 0 {
            break;
        }
        let (chunk_id_lower, chunk_id_upper) = desktop_file_chunk_id_bounds(file_id);
        let removed = tx.execute(
            &format!(
                "DELETE FROM {CHUNKS_TABLE}
                 WHERE rowid IN (
                   SELECT rowid FROM {CHUNKS_TABLE}
                   WHERE id >= ?1 AND id < ?2
                   LIMIT ?3
                 )"
            ),
            params![
                chunk_id_lower,
                chunk_id_upper,
                remaining_chunk_delete_limit as i64
            ],
        )?;
        stats.removed_unsafe_chunks += removed;
        remaining_chunk_delete_limit = remaining_chunk_delete_limit.saturating_sub(removed);
    }
    stats.removed_deleted_chunks = tx.execute(
        &format!(
            "DELETE FROM {CHUNKS_TABLE}
             WHERE rowid IN (
               SELECT rowid FROM {CHUNKS_TABLE}
               WHERE COALESCE(deleted, 0) = 1
               LIMIT {DESKTOP_FILE_INDEX_MAINTENANCE_CHUNK_DELETE_LIMIT}
             )"
        ),
        [],
    )?;
    let unsafe_tombstone_cutoff = now
        - Duration::from_secs(DESKTOP_FILE_INDEX_UNSAFE_TOMBSTONE_RETENTION_SECS).as_millis()
            as f64;
    stats.removed_unsafe_file_tombstones = tx.execute(
        &format!(
            "DELETE FROM {FILES_TABLE}
             WHERE rowid IN (
               SELECT rowid FROM {FILES_TABLE}
               INDEXED BY ctox_business_os_desktop_files_deleted_unsafe_idx
               WHERE COALESCE(deleted, 0) = 1
                 AND json_extract(data, '$.source') = 'ctox-core'
                 AND COALESCE(json_extract(data, '$.is_deleted'), 0) = 1
                 AND json_extract(data, '$.tombstone_reason') = 'unsafe_internal_ctox_path'
                 AND COALESCE(lastWriteTime, 0) <= ?1
               ORDER BY lastWriteTime, id
               LIMIT ?2
             )"
        ),
        params![
            unsafe_tombstone_cutoff,
            DESKTOP_FILE_INDEX_MAINTENANCE_FILE_TOMBSTONE_DELETE_LIMIT as i64,
        ],
    )?;
    if stats.tombstoned_unsafe_files == 0
        && stats.removed_unsafe_chunks == 0
        && stats.removed_deleted_chunks == 0
        && stats.removed_unsafe_file_tombstones == 0
    {
        stats.removed_stale_chunks = tx.execute(
            &format!(
                "DELETE FROM {CHUNKS_TABLE}
                 WHERE rowid IN (
                   SELECT c.rowid FROM {CHUNKS_TABLE} AS c
                   WHERE COALESCE(c.deleted, 0) = 0
                     AND NOT EXISTS (
                       SELECT 1 FROM {FILES_TABLE} AS f
                       WHERE f.id = json_extract(c.data, '$.file_id')
                         AND COALESCE(f.deleted, 0) = 0
                         AND COALESCE(json_extract(f.data, '$._deleted'), 0) = 0
                         AND COALESCE(json_extract(f.data, '$.is_deleted'), 0) = 0
                         AND json_extract(f.data, '$.content_generation_id') =
                             json_extract(c.data, '$.generation_id')
                     )
                   LIMIT {DESKTOP_FILE_INDEX_MAINTENANCE_CHUNK_DELETE_LIMIT}
                 )"
            ),
            [],
        )?;
    }
    tx.commit()?;
    apply_desktop_file_chunk_cache_policy(root, &mut conn, &mut stats, cache_config.normalized())?;
    Ok(stats)
}

fn apply_desktop_file_chunk_cache_policy(
    root: &Path,
    conn: &mut Connection,
    stats: &mut DesktopFileIndexMaintenanceStats,
    config: DesktopFileChunkCacheConfig,
) -> anyhow::Result<()> {
    let live_bytes_before = desktop_file_chunk_cache_live_bytes(conn)?;
    stats.cache_live_bytes_before = live_bytes_before;
    stats.cache_live_bytes_after = live_bytes_before;
    if live_bytes_before <= config.max_live_bytes {
        return Ok(());
    }

    ensure_desktop_file_chunk_cache_state_table(conn)?;
    let mut state = desktop_file_chunk_cache_state(conn)?;
    let now = now_ms() as u64;
    let cutoff_ms = now.saturating_sub(config.active_min_age_secs.saturating_mul(1_000));
    let scan_roots = desktop_file_scan_roots(root);
    let candidates = desktop_file_chunk_cache_candidates(conn, config.max_files_per_pass)?;
    let mut projected_live_bytes = live_bytes_before;
    let mut selected_chunk_count = 0usize;
    let mut selected = Vec::new();
    let mut pinned_bytes = 0u64;

    for candidate in candidates {
        if projected_live_bytes <= config.target_live_bytes {
            break;
        }
        if selected.len() >= config.max_files_per_pass {
            pinned_bytes = pinned_bytes.saturating_add(candidate.bytes);
            continue;
        }
        if selected_chunk_count.saturating_add(candidate.chunk_count) > config.max_chunks_per_pass {
            pinned_bytes = pinned_bytes.saturating_add(candidate.bytes);
            continue;
        }
        if candidate.created_at_ms > cutoff_ms {
            pinned_bytes = pinned_bytes.saturating_add(candidate.bytes);
            continue;
        }
        let Some(metadata) =
            desktop_file_chunk_cache_eviction_metadata(root, &scan_roots, &candidate.document)
        else {
            pinned_bytes = pinned_bytes.saturating_add(candidate.bytes);
            continue;
        };
        selected_chunk_count = selected_chunk_count.saturating_add(candidate.chunk_count);
        projected_live_bytes = projected_live_bytes.saturating_sub(candidate.bytes);
        selected.push(DesktopFileChunkCacheEviction {
            candidate,
            metadata,
        });
    }

    if selected.is_empty() {
        stats.cache_pinned_bytes = pinned_bytes.max(live_bytes_before);
        stats.cache_over_quota_pinned_bytes =
            live_bytes_before.saturating_sub(config.max_live_bytes);
        return Ok(());
    }

    {
        let tx = conn.transaction()?;
        let now_f64 = now as f64;
        for eviction in &selected {
            let next_revision = maintenance_revision(
                eviction
                    .candidate
                    .document
                    .get("_rev")
                    .and_then(Value::as_str)
                    .or(eviction.candidate.revision.as_deref()),
            );
            let mut document = eviction.candidate.document.clone();
            prepare_desktop_file_cache_eviction(
                &mut document,
                &next_revision,
                now,
                &eviction.metadata,
            );
            let data = serde_json::to_string(&document)?;
            let updated = tx.execute(
                "UPDATE \"ctox_business_os__desktop_files__v0\"
                 SET revision = ?2, lastWriteTime = ?3, data = ?4
                 WHERE id = ?1
                   AND COALESCE(deleted, 0) = 0
                   AND json_extract(data, '$.content_generation_id') = ?5",
                params![
                    eviction.candidate.file_id,
                    next_revision,
                    now_f64,
                    data,
                    eviction.candidate.generation_id,
                ],
            )?;
            if updated == 0 {
                continue;
            }
            let (chunk_id_lower, chunk_id_upper) =
                desktop_file_chunk_id_bounds(&eviction.candidate.file_id);
            let removed_chunks = tx.execute(
                "DELETE FROM \"ctox_business_os__desktop_file_chunks__v0\"
                 WHERE rowid IN (
                   SELECT rowid FROM \"ctox_business_os__desktop_file_chunks__v0\"
                   WHERE id >= ?1
                     AND id < ?2
                     AND COALESCE(deleted, 0) = 0
                     AND json_extract(data, '$.generation_id') = ?3
                   LIMIT ?4
                 )",
                params![
                    chunk_id_lower,
                    chunk_id_upper,
                    eviction.candidate.generation_id,
                    config.max_chunks_per_pass as i64,
                ],
            )?;
            stats.evicted_cache_files = stats.evicted_cache_files.saturating_add(1);
            stats.removed_cache_chunks = stats.removed_cache_chunks.saturating_add(removed_chunks);
            stats.removed_cache_bytes = stats
                .removed_cache_bytes
                .saturating_add(eviction.candidate.bytes);
        }
        tx.commit()?;
    }

    let live_bytes_after = desktop_file_chunk_cache_live_bytes(conn)?;
    stats.cache_live_bytes_after = live_bytes_after;
    stats.cache_pinned_bytes = live_bytes_after;
    stats.cache_over_quota_pinned_bytes = live_bytes_after.saturating_sub(config.max_live_bytes);
    if stats.removed_cache_chunks == 0 {
        return Ok(());
    }

    state.last_eviction_at_ms = now;
    state.last_live_bytes = live_bytes_after;
    state.last_pinned_bytes = stats.cache_pinned_bytes;
    state.last_deleted_bytes = stats.removed_cache_bytes;
    state.last_deleted_chunks = stats.removed_cache_chunks as u64;

    if stats.removed_cache_bytes >= config.wal_checkpoint_min_bytes
        && now.saturating_sub(state.last_checkpoint_at_ms)
            >= config.checkpoint_min_interval_secs.saturating_mul(1_000)
    {
        if conn
            .execute_batch("PRAGMA wal_checkpoint(PASSIVE); PRAGMA optimize;")
            .is_ok()
        {
            stats.wal_checkpoint_ran = true;
            state.last_checkpoint_at_ms = now;
        }
    }

    let page_size = sqlite_pragma_u64(conn, "page_size").unwrap_or(0);
    let freelist_count = sqlite_pragma_u64(conn, "freelist_count").unwrap_or(0);
    let reclaimable_bytes = page_size.saturating_mul(freelist_count);
    if reclaimable_bytes >= config.vacuum_min_reclaim_bytes
        && now.saturating_sub(state.last_vacuum_at_ms)
            >= config.vacuum_min_interval_secs.saturating_mul(1_000)
    {
        if conn.execute_batch("VACUUM; PRAGMA optimize;").is_ok() {
            stats.vacuum_ran = true;
            state.last_vacuum_at_ms = now;
        }
    }

    save_desktop_file_chunk_cache_state(conn, &state)?;
    Ok(())
}

fn desktop_file_chunk_cache_live_bytes(conn: &Connection) -> anyhow::Result<u64> {
    let bytes: i64 = conn.query_row(
        "SELECT COALESCE(SUM(
             COALESCE(
               CAST(json_extract(data, '$.size_bytes') AS INTEGER),
               length(COALESCE(json_extract(data, '$.data'), '')),
               0
             )
           ), 0)
         FROM \"ctox_business_os__desktop_file_chunks__v0\"
         WHERE COALESCE(deleted, 0) = 0",
        [],
        |row| row.get(0),
    )?;
    Ok(u64::try_from(bytes).unwrap_or(0))
}

fn desktop_file_chunk_cache_candidates(
    conn: &Connection,
    limit: usize,
) -> anyhow::Result<Vec<DesktopFileChunkCacheCandidate>> {
    let mut stmt = conn.prepare(
        "SELECT f.id,
                f.revision,
                f.data,
                json_extract(f.data, '$.content_generation_id') AS generation_id,
                COUNT(c.rowid) AS chunk_count,
                COALESCE(SUM(
                  COALESCE(
                    CAST(json_extract(c.data, '$.size_bytes') AS INTEGER),
                    length(COALESCE(json_extract(c.data, '$.data'), '')),
                    0
                  )
                ), 0) AS byte_count,
                COALESCE(
                  CAST(json_extract(f.data, '$.content_synced_at_ms') AS INTEGER),
                  MIN(CAST(json_extract(c.data, '$.created_at_ms') AS INTEGER)),
                  CAST(f.lastWriteTime AS INTEGER),
                  0
                ) AS created_at_ms
         FROM \"ctox_business_os__desktop_file_chunks__v0\" AS c
         JOIN \"ctox_business_os__desktop_files__v0\" AS f
           ON f.id = json_extract(c.data, '$.file_id')
          AND json_extract(f.data, '$.content_generation_id') =
              json_extract(c.data, '$.generation_id')
         WHERE COALESCE(c.deleted, 0) = 0
           AND COALESCE(f.deleted, 0) = 0
           AND COALESCE(json_extract(f.data, '$._deleted'), 0) = 0
           AND COALESCE(json_extract(f.data, '$.is_deleted'), 0) = 0
           AND json_extract(f.data, '$.source') = 'ctox-core'
           AND json_extract(f.data, '$.kind') = 'file'
           AND json_extract(f.data, '$.content_state') = 'available'
         GROUP BY f.id, generation_id
         ORDER BY created_at_ms ASC, byte_count DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| {
        let data: String = row.get(2)?;
        let document = serde_json::from_str::<Value>(&data).unwrap_or(Value::Null);
        let chunk_count: i64 = row.get(4)?;
        let bytes: i64 = row.get(5)?;
        let created_at_ms: i64 = row.get(6)?;
        Ok(DesktopFileChunkCacheCandidate {
            file_id: row.get(0)?,
            revision: row.get(1)?,
            document,
            generation_id: row.get(3)?,
            chunk_count: usize::try_from(chunk_count).unwrap_or(usize::MAX),
            bytes: u64::try_from(bytes).unwrap_or(0),
            created_at_ms: u64::try_from(created_at_ms).unwrap_or(0),
        })
    })?;
    let mut candidates = Vec::new();
    for row in rows {
        let candidate = row?;
        if candidate.document.is_object()
            && !candidate.generation_id.trim().is_empty()
            && candidate.chunk_count > 0
        {
            candidates.push(candidate);
        }
    }
    Ok(candidates)
}

fn desktop_file_chunk_cache_eviction_metadata(
    root: &Path,
    scan_roots: &[DesktopFileScanRoot],
    document: &Value,
) -> Option<fs::Metadata> {
    let path = document
        .get("local_path")
        .or_else(|| document.get("path"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)?;
    ensure_safe_desktop_file_index_path(&path, "desktop file cache eviction").ok()?;
    let metadata = fs::metadata(&path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    if document.get("size_bytes").and_then(Value::as_u64) != Some(metadata.len()) {
        return None;
    }
    let modified_at_ms = metadata_modified_at_ms(&metadata);
    if document
        .get("mtime_ms")
        .and_then(Value::as_u64)
        .map(u128::from)
        != Some(modified_at_ms)
    {
        return None;
    }
    if scan_roots
        .iter()
        .any(|scan_root| path.starts_with(&scan_root.path))
        && should_eager_sync_file(&path, &metadata)
    {
        return None;
    }
    if !path.starts_with(root)
        && document.get("source").and_then(Value::as_str) != Some("ctox-core")
    {
        return None;
    }
    Some(metadata)
}

fn prepare_desktop_file_cache_eviction(
    document: &mut Value,
    revision: &str,
    now: u64,
    metadata: &fs::Metadata,
) {
    let modified_at_ms = metadata_modified_at_ms(metadata);
    if let Some(object) = document.as_object_mut() {
        object.insert("_rev".to_string(), Value::String(revision.to_string()));
        object.insert("_meta".to_string(), json!({ "lwt": now as f64 }));
        object.insert(
            "content_state".to_string(),
            Value::String("lazy".to_string()),
        );
        object.insert("content_generation_id".to_string(), Value::Null);
        object.insert("chunk_count".to_string(), Value::Null);
        object.insert("generation_verified_at_ms".to_string(), Value::Null);
        object.insert("content_synced_at_ms".to_string(), Value::Null);
        object.insert(
            "content_hash".to_string(),
            Value::String(format!("mtime:{modified_at_ms}:size:{}", metadata.len())),
        );
        object.insert(
            "content_hash_scheme".to_string(),
            Value::String(DESKTOP_FILE_CONTENT_HASH_SCHEME.to_string()),
        );
        object.insert("content_evicted_at_ms".to_string(), Value::from(now));
        object.insert(
            "content_eviction_reason".to_string(),
            Value::String("desktop_file_chunk_cache_quota".to_string()),
        );
        object.insert("updated_at_ms".to_string(), Value::from(now));
    }
}

fn ensure_desktop_file_chunk_cache_state_table(conn: &Connection) -> anyhow::Result<()> {
    conn.execute(
        &format!(
            "CREATE TABLE IF NOT EXISTS {DESKTOP_FILE_CHUNK_CACHE_STATE_TABLE} (
                id TEXT PRIMARY KEY,
                updated_at_ms INTEGER NOT NULL,
                value_json TEXT NOT NULL
            )"
        ),
        [],
    )
    .context("ensure desktop file chunk cache state table")?;
    Ok(())
}

fn desktop_file_chunk_cache_state(conn: &Connection) -> anyhow::Result<DesktopFileChunkCacheState> {
    if !sqlite_table_exists(conn, DESKTOP_FILE_CHUNK_CACHE_STATE_TABLE)? {
        return Ok(DesktopFileChunkCacheState::default());
    }
    let state_json = conn
        .query_row(
            &format!("SELECT value_json FROM {DESKTOP_FILE_CHUNK_CACHE_STATE_TABLE} WHERE id = ?1"),
            [DESKTOP_FILE_CHUNK_CACHE_STATE_ID],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(state_json) = state_json else {
        return Ok(DesktopFileChunkCacheState::default());
    };
    Ok(serde_json::from_str(&state_json).unwrap_or_default())
}

fn save_desktop_file_chunk_cache_state(
    conn: &Connection,
    state: &DesktopFileChunkCacheState,
) -> anyhow::Result<()> {
    let now = now_ms() as u64;
    let state_json = serde_json::to_string(state)?;
    conn.execute(
        &format!(
            "INSERT INTO {DESKTOP_FILE_CHUNK_CACHE_STATE_TABLE}
             (id, updated_at_ms, value_json)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET
               updated_at_ms = excluded.updated_at_ms,
               value_json = excluded.value_json"
        ),
        params![DESKTOP_FILE_CHUNK_CACHE_STATE_ID, now, state_json],
    )
    .context("save desktop file chunk cache state")?;
    Ok(())
}

pub(super) fn unsafe_desktop_file_index_candidates_sql(files_table: &str) -> String {
    const PATH_EXPR: &str =
        "COALESCE(json_extract(data, '$.local_path'), json_extract(data, '$.path'))";
    format!(
        "SELECT id, revision, data FROM {files_table} \
         INDEXED BY ctox_business_os_desktop_files_live_core_idx
         WHERE COALESCE(deleted, 0) = 0
           AND json_extract(data, '$.source') = 'ctox-core'
           AND json_extract(data, '$.kind') = 'file'
           AND COALESCE(json_extract(data, '$.is_deleted'), 0) = 0
           AND (
             ({PATH_EXPR} >= '/tmp/' AND {PATH_EXPR} < '/tmp0')
             OR ({PATH_EXPR} >= '/var/tmp/' AND {PATH_EXPR} < '/var/tmp0')
             OR ({PATH_EXPR} >= '/var/folders/' AND {PATH_EXPR} < '/var/folders0')
             OR ({PATH_EXPR} >= '/private/var/folders/' AND {PATH_EXPR} < '/private/var/folders0')
             OR {PATH_EXPR} GLOB '*/.local/lib/ctox/*'
             OR {PATH_EXPR} GLOB '*/.local/state/ctox/*'
           )
         LIMIT ?1"
    )
}

pub(super) fn desktop_file_chunk_id_bounds(file_id: &str) -> (String, String) {
    chunk_id_prefix_bounds(file_id)
}

fn desktop_file_index_document_is_unsafe(document: &Value, home: Option<&Path>) -> bool {
    if document.get("kind").and_then(Value::as_str) != Some("file") {
        return false;
    }
    if document.get("source").and_then(Value::as_str) != Some("ctox-core") {
        return false;
    }
    let Some(path) = document
        .get("local_path")
        .or_else(|| document.get("path"))
        .and_then(Value::as_str)
        .map(PathBuf::from)
    else {
        return false;
    };
    is_unsafe_desktop_file_index_path(&path, home)
}

fn is_unsafe_desktop_file_index_path(path: &Path, home: Option<&Path>) -> bool {
    if is_ctox_internal_path_layout(path) {
        return true;
    }
    if let Some(home) = home {
        if is_ctox_internal_desktop_scan_root(path, home) {
            return true;
        }
    }
    path.starts_with("/tmp")
        || path.starts_with("/var/tmp")
        || path.starts_with("/var/folders")
        || path.starts_with("/private/var/folders")
}

pub(super) fn ensure_desktop_file_index_query_indexes(conn: &Connection) -> anyhow::Result<()> {
    if !sqlite_table_has_column(conn, "ctox_business_os__desktop_files__v0", "deleted")? {
        return Ok(());
    }
    conn.execute(
        r#"
        CREATE INDEX IF NOT EXISTS ctox_business_os_desktop_files_live_core_idx
        ON "ctox_business_os__desktop_files__v0" (
            json_extract(data, '$.source'),
            json_extract(data, '$.kind'),
            COALESCE(json_extract(data, '$.is_deleted'), 0),
            COALESCE(json_extract(data, '$.local_path'), json_extract(data, '$.path'))
        )
        WHERE COALESCE(deleted, 0) = 0
        "#,
        [],
    )
    .context("ensure desktop_files live ctox-core index")?;
    conn.execute(
        r#"
        CREATE INDEX IF NOT EXISTS ctox_business_os_desktop_files_deleted_unsafe_idx
        ON "ctox_business_os__desktop_files__v0" (
            COALESCE(deleted, 0),
            json_extract(data, '$.tombstone_reason'),
            lastWriteTime,
            id
        )
        WHERE COALESCE(deleted, 0) = 1
          AND json_extract(data, '$.source') = 'ctox-core'
          AND COALESCE(json_extract(data, '$.is_deleted'), 0) = 1
        "#,
        [],
    )
    .context("ensure desktop_files unsafe tombstone cleanup index")?;
    conn.execute(
        r#"
        CREATE INDEX IF NOT EXISTS ctox_business_os_desktop_files_active_generation_idx
        ON "ctox_business_os__desktop_files__v0" (
            json_extract(data, '$.source'),
            json_extract(data, '$.kind'),
            json_extract(data, '$.content_state'),
            json_extract(data, '$.content_generation_id'),
            id
        )
        WHERE COALESCE(deleted, 0) = 0
        "#,
        [],
    )
    .context("ensure desktop_files active generation index")?;
    if sqlite_table_exists(conn, "ctox_business_os__desktop_file_chunks__v0")?
        && sqlite_table_has_column(conn, "ctox_business_os__desktop_file_chunks__v0", "deleted")?
    {
        conn.execute(
            r#"
            CREATE INDEX IF NOT EXISTS ctox_business_os_desktop_file_chunks_deleted_idx
            ON "ctox_business_os__desktop_file_chunks__v0" (
                COALESCE(deleted, 0),
                id
            )
            WHERE COALESCE(deleted, 0) = 1
            "#,
            [],
        )
        .context("ensure desktop_file_chunks deleted index")?;
        conn.execute(
            r#"
            CREATE INDEX IF NOT EXISTS ctox_business_os_desktop_file_chunks_live_owner_idx
            ON "ctox_business_os__desktop_file_chunks__v0" (
                COALESCE(deleted, 0),
                json_extract(data, '$.file_id'),
                json_extract(data, '$.generation_id'),
                CAST(json_extract(data, '$.created_at_ms') AS INTEGER),
                id
            )
            WHERE COALESCE(deleted, 0) = 0
            "#,
            [],
        )
        .context("ensure desktop_file_chunks live owner index")?;
    }
    Ok(())
}

fn prepare_unsafe_desktop_file_tombstone(document: &mut Value, revision: &str, now: f64) {
    let now_u64 = now as u64;
    if let Some(object) = document.as_object_mut() {
        object.insert("_rev".to_string(), Value::String(revision.to_string()));
        object.insert("_deleted".to_string(), Value::Bool(true));
        object.insert("_attachments".to_string(), json!({}));
        object.insert("_meta".to_string(), json!({ "lwt": now }));
        object.insert("is_deleted".to_string(), Value::Bool(true));
        object.insert(
            "content_state".to_string(),
            Value::String("missing".to_string()),
        );
        object.insert("content_generation_id".to_string(), Value::Null);
        object.insert("content_hash".to_string(), Value::String(String::new()));
        object.insert("content_synced_at_ms".to_string(), Value::Null);
        object.insert("deleted_at_ms".to_string(), Value::from(now_u64));
        object.insert("updated_at_ms".to_string(), Value::from(now_u64));
        object.insert(
            "tombstone_reason".to_string(),
            Value::String("unsafe_internal_ctox_path".to_string()),
        );
    }
}

fn collect_desktop_file_index_candidates(
    scan_roots: &[DesktopFileScanRoot],
) -> (Vec<DesktopFileIndexCandidate>, bool) {
    let mut candidates = Vec::new();
    let mut budget = DesktopFileScanBudget::new();
    for scan_root in scan_roots {
        if candidates.len() >= DESKTOP_FILE_SCAN_MAX_FILES || !budget.has_time_remaining() {
            break;
        }
        let mut paths = Vec::new();
        collect_files_bounded(
            &scan_root.path,
            &mut paths,
            DESKTOP_FILE_SCAN_MAX_FILES.saturating_sub(candidates.len()),
            &mut budget,
        );
        candidates.extend(paths.into_iter().map(|path| DesktopFileIndexCandidate {
            path,
            scan_root: scan_root.clone(),
        }));
        if candidates.len() >= DESKTOP_FILE_SCAN_MAX_FILES || budget.exhausted {
            break;
        }
    }
    candidates.truncate(DESKTOP_FILE_SCAN_MAX_FILES);
    let truncated = candidates.len() >= DESKTOP_FILE_SCAN_MAX_FILES || budget.exhausted;
    (candidates, truncated)
}

pub(super) async fn collect_desktop_file_index_scan(
    scan_roots: Vec<DesktopFileScanRoot>,
) -> anyhow::Result<DesktopFileIndexScan> {
    tokio::task::spawn_blocking(move || collect_desktop_file_index_scan_sync(scan_roots))
        .await
        .context("join native desktop file index scan")
}

async fn collect_desktop_file_index_scan_unbounded(
    scan_roots: Vec<DesktopFileScanRoot>,
) -> anyhow::Result<DesktopFileIndexScan> {
    tokio::task::spawn_blocking(move || collect_desktop_file_index_scan_sync_unbounded(scan_roots))
        .await
        .context("join native desktop file index workspace scan")
}

fn collect_desktop_file_index_scan_sync(
    mut scan_roots: Vec<DesktopFileScanRoot>,
) -> DesktopFileIndexScan {
    normalize_desktop_file_scan_roots(&mut scan_roots);
    let (mut candidates, truncated) = collect_desktop_file_index_candidates(&scan_roots);
    candidates.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.scan_root.path.cmp(&right.scan_root.path))
    });
    let stamp = desktop_file_index_projection_stamp(&scan_roots, &candidates, truncated);
    DesktopFileIndexScan {
        scan_roots,
        candidates,
        stamp,
    }
}

fn collect_desktop_file_index_scan_sync_unbounded(
    mut scan_roots: Vec<DesktopFileScanRoot>,
) -> DesktopFileIndexScan {
    normalize_desktop_file_scan_roots(&mut scan_roots);
    let mut candidates = Vec::new();
    for scan_root in &scan_roots {
        let mut paths = Vec::new();
        collect_files_unbounded(&scan_root.path, &mut paths);
        candidates.extend(paths.into_iter().map(|path| DesktopFileIndexCandidate {
            path,
            scan_root: scan_root.clone(),
        }));
    }
    candidates.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.scan_root.path.cmp(&right.scan_root.path))
    });
    let stamp = desktop_file_index_projection_stamp(&scan_roots, &candidates, false);
    DesktopFileIndexScan {
        scan_roots,
        candidates,
        stamp,
    }
}

fn normalize_desktop_file_scan_roots(roots: &mut Vec<DesktopFileScanRoot>) {
    roots.sort_by(|left, right| left.path.cmp(&right.path));
    roots.dedup_by(|left, right| left.path == right.path);
}

pub(super) fn desktop_file_scan_roots(root: &Path) -> Vec<DesktopFileScanRoot> {
    let mut roots = vec![
        (root.join("runtime/business-os/notes"), "Notes".to_string()),
        (
            root.join("runtime/business-os/documents/generated"),
            "Generated Documents".to_string(),
        ),
        (
            root.join("runtime/business-os-imports"),
            "Imports".to_string(),
        ),
    ];
    // Active harness workspaces are durable queue-owned roots. They must be
    // visible to the background index even before a successful worker turn
    // performs its immediate projection; otherwise a daemon restart or a
    // long-running task can leave the browser with no workspace metadata.
    // Keep discovery bounded and apply the same canonical safe-root gate as
    // every other background scan source.
    let active_statuses = ["pending", "leased", "blocked"]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    if let Ok(tasks) = channels::list_queue_tasks(root, &active_statuses, 512) {
        roots.extend(tasks.into_iter().filter_map(|task| {
            let workspace = task.workspace_root?.trim().to_string();
            if workspace.is_empty() {
                return None;
            }
            let path = PathBuf::from(workspace);
            Some((path.clone(), desktop_file_scan_root_label(&path)))
        }));
    }
    let mut roots = roots
        .into_iter()
        .filter_map(|(path, label)| {
            path.canonicalize()
                .ok()
                .map(|path| DesktopFileScanRoot { path, label })
        })
        .filter(|root| is_safe_desktop_file_scan_root(&root.path))
        .collect::<Vec<_>>();
    normalize_desktop_file_scan_roots(&mut roots);
    roots
}

pub(super) fn desktop_file_scan_root_label(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "Workspace".to_string())
}

pub(super) fn is_safe_desktop_file_scan_root(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    if is_broad_desktop_file_scan_root(path) {
        return false;
    }
    if is_ctox_internal_path_layout(path) {
        return false;
    }
    true
}

pub(super) fn ensure_safe_desktop_file_index_path(path: &Path, kind: &str) -> anyhow::Result<()> {
    if is_ctox_internal_path_layout(path) || is_broad_desktop_file_scan_root(path) {
        anyhow::bail!(
            "{kind} is outside the Business OS file-index boundary: {}",
            path.display()
        );
    }
    Ok(())
}

pub(super) fn is_broad_desktop_file_scan_root(path: &Path) -> bool {
    if path == Path::new("/")
        || path == Path::new("/Users")
        || path == Path::new("/Applications")
        || path == Path::new("/Library")
        || path == Path::new("/System")
        || path == Path::new("/Volumes")
        || path == Path::new("/private")
        || path == Path::new("/var")
        || path == Path::new("/tmp")
        || path == Path::new("/var/tmp")
        || path == Path::new("/var/folders")
        || path == Path::new("/private/var/folders")
    {
        return true;
    }
    let mut components = path
        .components()
        .filter_map(|component| component.as_os_str().to_str());
    matches!(
        (
            components.next(),
            components.next(),
            components.next(),
            components.next()
        ),
        (Some("/"), Some("Users"), Some(_user), None)
    )
}

fn desktop_file_virtual_location(
    scan_root: &DesktopFileScanRoot,
    path: &Path,
) -> (Vec<String>, String) {
    let mut folder_components = vec![scan_root.label.clone()];
    let relative = path.strip_prefix(&scan_root.path).unwrap_or(path);
    if let Some(parent) = relative.parent() {
        folder_components.extend(
            parent
                .components()
                .filter_map(|component| component.as_os_str().to_str())
                .filter(|component| !component.is_empty())
                .map(str::to_string),
        );
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let mut parts = vec!["CTOX".to_string()];
    parts.extend(folder_components.iter().cloned());
    parts.push(file_name.to_string());
    (folder_components, format!("/{}", parts.join("/")))
}

pub(super) fn desktop_file_id(path: &Path) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    format!("ctox_file_{:x}", hasher.finalize())
}

async fn mark_missing_scanned_desktop_files(
    root: &Path,
    database: &Arc<RxDatabase>,
    scan_roots: &[DesktopFileScanRoot],
    seen_file_ids: &HashSet<String>,
) -> anyhow::Result<usize> {
    if scan_roots.is_empty() {
        return Ok(0);
    }
    let files = database
        .collection("desktop_files")
        .context("desktop_files collection is not registered")?;
    ensure_desktop_file_index_query_indexes_for_root(root)
        .await
        .context("ensure ctox-core desktop_files query index")?;
    let rows = load_live_ctox_desktop_file_documents(root)
        .await
        .context("load ctox-core desktop_files for missing scan")?;

    let mut marked = 0usize;
    let now = now_ms();
    for row in &rows {
        let mut document = row.clone();
        let Some(file_id) = document
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        if seen_file_ids.contains(&file_id) {
            continue;
        }
        if document
            .get("is_deleted")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let Some(local_path) = document
            .get("local_path")
            .or_else(|| document.get("path"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let local_path = PathBuf::from(local_path);
        if !scan_roots
            .iter()
            .any(|scan_root| local_path.starts_with(&scan_root.path))
        {
            continue;
        }
        if let Some(object) = document.as_object_mut() {
            object.remove("_rev");
            object.remove("_meta");
            object.insert("_deleted".to_string(), Value::Bool(false));
            object.insert("is_deleted".to_string(), Value::Bool(true));
            object.insert(
                "content_state".to_string(),
                Value::String("missing".to_string()),
            );
            object.insert("deleted_at_ms".to_string(), Value::from(now as u64));
            object.insert(
                "tombstone_reason".to_string(),
                Value::String("missing_from_scan".to_string()),
            );
            object.insert("updated_at_ms".to_string(), Value::from(now as u64));
        }
        files
            .incremental_upsert(document)
            .await
            .map_err(|err| anyhow::anyhow!("mark missing desktop file {file_id}: {err}"))?;
        marked += 1;
    }
    Ok(marked)
}

async fn load_live_ctox_desktop_file_documents(root: &Path) -> anyhow::Result<Vec<Value>> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || load_live_ctox_desktop_file_documents_sync(&root))
        .await
        .context("join ctox-core desktop_files scan")?
}

async fn ensure_desktop_file_index_query_indexes_for_root(root: &Path) -> anyhow::Result<()> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || {
        ensure_desktop_file_index_query_indexes_for_root_sync(&root)
    })
    .await
    .context("join desktop_files query index ensure")?
}

pub(super) fn ensure_desktop_file_index_query_indexes_for_root_sync(
    root: &Path,
) -> anyhow::Result<()> {
    const FILES_TABLE_NAME: &str = "ctox_business_os__desktop_files__v0";

    let database_path = store::rxdb_store_path(root);
    if !database_path.exists() {
        return Ok(());
    }
    let conn = Connection::open(&database_path)
        .with_context(|| format!("open RxDB store {}", database_path.display()))?;
    conn.busy_timeout(Duration::from_secs(10))
        .context("set RxDB index busy timeout")?;
    if !sqlite_table_exists(&conn, FILES_TABLE_NAME)? {
        return Ok(());
    }
    ensure_desktop_file_index_query_indexes(&conn)
}

pub(super) fn load_live_ctox_desktop_file_documents_sync(
    root: &Path,
) -> anyhow::Result<Vec<Value>> {
    const FILES_TABLE: &str = "\"ctox_business_os__desktop_files__v0\"";
    const FILES_TABLE_NAME: &str = "ctox_business_os__desktop_files__v0";

    let database_path = store::rxdb_store_path(root);
    if !database_path.exists() {
        return Ok(Vec::new());
    }
    let conn = Connection::open_with_flags(
        &database_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("open RxDB store {}", database_path.display()))?;
    conn.busy_timeout(Duration::from_secs(10))
        .context("set RxDB read busy timeout")?;
    if !sqlite_table_exists(&conn, FILES_TABLE_NAME)? {
        return Ok(Vec::new());
    }
    let deleted_predicate = if sqlite_table_has_column(&conn, FILES_TABLE_NAME, "deleted")? {
        "COALESCE(deleted, 0) = 0 AND"
    } else {
        ""
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT data FROM {FILES_TABLE}
         WHERE {deleted_predicate}
           json_extract(data, '$.kind') = 'file'
           AND json_extract(data, '$.source') = 'ctox-core'
           AND COALESCE(json_extract(data, '$.is_deleted'), 0) = 0"
    ))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut documents = Vec::new();
    for row in rows {
        let data = row?;
        let Ok(document) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        documents.push(document);
    }
    Ok(documents)
}
