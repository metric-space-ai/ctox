// Origin: CTOX
// License: Apache-2.0

use super::rxdb_peer::*;
use super::store;
use crate::mission::channels;
use anyhow::Context;
use base64::Engine;
use notify::Watcher;
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection, OpenFlags, OptionalExtension};
use rxdb::plugins::replication_webrtc::file_fetch_handler::FileRange;
use rxdb::rx_collection::RxCollection;
use rxdb::rx_database::RxDatabase;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::Value;
use sha2::Digest;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, Mutex as AsyncMutex};

#[path = "desktop_file_stream.rs"]
mod stream;

#[cfg(test)]
pub(super) static DESKTOP_FILE_CHUNK_COMPLETENESS_CHECKS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(super) fn reset_desktop_file_chunk_completeness_checks(_root: &Path) {
    DESKTOP_FILE_CHUNK_COMPLETENESS_CHECKS.store(0, Ordering::Relaxed);
}

#[cfg(test)]
pub(super) fn desktop_file_chunk_completeness_check_count(_root: &Path) -> usize {
    DESKTOP_FILE_CHUNK_COMPLETENESS_CHECKS.load(Ordering::Relaxed)
}

pub(super) const DESKTOP_FILE_CHUNK_SIZE: usize = 16 * 1024;
pub(super) const SPREADSHEET_BLOB_CHUNK_SIZE: usize = 256_000;
pub(super) const SPREADSHEET_CSV_IMPORT_LIMIT_BYTES: u64 = 10 * 1024 * 1024;
pub(super) const SPREADSHEET_CSV_IMPORT_MAX_ROWS: usize = 50_000;
pub(super) const SPREADSHEET_CSV_IMPORT_MAX_COLUMNS: usize = 512;
pub(super) const DESKTOP_FILE_CHUNK_DECODED_SIZE: u64 = (DESKTOP_FILE_CHUNK_SIZE as u64 / 4) * 3;
pub(super) const DESKTOP_FILE_EAGER_LIMIT_BYTES: u64 = 1024 * 1024;
pub(super) const DESKTOP_FILE_SCAN_INTERVAL_SECS: u64 = 15;
pub(super) const DESKTOP_FILE_SCAN_FALLBACK_INTERVAL_SECS: u64 =
    BUSINESS_OS_STANDBY_RECONCILE_INTERVAL_SECS;
pub(super) const DESKTOP_FILE_SCAN_MAX_DEPTH: usize = 6;
pub(super) const DESKTOP_FILE_SCAN_MAX_FILES: usize = 200;
pub(super) const DESKTOP_FILE_SCAN_MAX_DIRECTORIES: usize = 4_096;
pub(super) const DESKTOP_FILE_SCAN_BUDGET: Duration = Duration::from_millis(250);
pub(super) const DESKTOP_FILE_CHUNK_RETAIN_GENERATIONS: usize = 2;
pub(super) const DESKTOP_FILE_CHUNK_CLEANUP_SCAN_LIMIT: u64 = 100_000;
pub(super) const DESKTOP_FILE_INDEX_MAINTENANCE_INTERVAL_SECS: u64 = 10 * 60;
pub(super) const DESKTOP_FILE_INDEX_MAINTENANCE_FILE_LIMIT: usize = 1_000;
pub(super) const DESKTOP_FILE_INDEX_MAINTENANCE_CHUNK_DELETE_LIMIT: usize = 5_000;
pub(super) const DESKTOP_FILE_INDEX_MAINTENANCE_FILE_TOMBSTONE_DELETE_LIMIT: usize = 5_000;
pub(super) const DESKTOP_FILE_INDEX_UNSAFE_TOMBSTONE_RETENTION_SECS: u64 = 24 * 60 * 60;
pub(super) const DESKTOP_FILE_CHUNK_CACHE_MAX_LIVE_BYTES: u64 = 64 * 1024 * 1024;
pub(super) const DESKTOP_FILE_CHUNK_CACHE_TARGET_LIVE_BYTES: u64 = 48 * 1024 * 1024;
pub(super) const DESKTOP_FILE_CHUNK_CACHE_ACTIVE_MIN_AGE_SECS: u64 = 6 * 60 * 60;
pub(super) const DESKTOP_FILE_CHUNK_CACHE_CHECKPOINT_MIN_INTERVAL_SECS: u64 = 30 * 60;
pub(super) const DESKTOP_FILE_CHUNK_CACHE_WAL_CHECKPOINT_MIN_BYTES: u64 = 16 * 1024 * 1024;
pub(super) const DESKTOP_FILE_CHUNK_CACHE_VACUUM_MIN_INTERVAL_SECS: u64 = 24 * 60 * 60;
pub(super) const DESKTOP_FILE_CHUNK_CACHE_VACUUM_MIN_RECLAIM_BYTES: u64 = 32 * 1024 * 1024;
pub(super) const DESKTOP_FILE_CHUNK_CACHE_STATE_TABLE: &str = "ctox_desktop_file_chunk_cache_state";
pub(super) const DESKTOP_FILE_CHUNK_CACHE_STATE_ID: &str = "desktop_file_chunks";
pub(super) const DESKTOP_FILE_CONTENT_HASH_SCHEME: &str = "sha256-bytes-v1";
pub(super) const DESKTOP_FILE_CHUNK_HASH_SCHEME: &str = "sha256-base64-chunk-v1";
pub(super) const CTOX_DESKTOP_FOLDER_ID: &str = "fs_ctox";
pub(super) const CTOX_DESKTOP_FOLDER_PATH: &str = "/CTOX";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DesktopFileContentPolicy {
    Eager,
    Lazy,
}

#[derive(Debug, Clone)]
pub(super) struct DesktopFileScanRoot {
    pub(super) path: PathBuf,
    pub(super) label: String,
}

#[derive(Debug, Clone)]
pub(super) struct DesktopFileIndexCandidate {
    pub(super) path: PathBuf,
    pub(super) scan_root: DesktopFileScanRoot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DesktopFileIndexProjectionStamp {
    pub(super) scan_root_count: usize,
    pub(super) candidate_count: usize,
    pub(super) truncated: bool,
    pub(super) content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DesktopFileScanRootsStamp {
    pub(super) scan_root_count: usize,
    pub(super) content_hash: String,
}

#[derive(Debug)]
pub(super) struct DesktopFileIndexScan {
    pub(super) scan_roots: Vec<DesktopFileScanRoot>,
    pub(super) candidates: Vec<DesktopFileIndexCandidate>,
    pub(super) stamp: DesktopFileIndexProjectionStamp,
}

pub(super) struct DesktopFileScanBudget {
    pub(super) started: Instant,
    pub(super) visited_directories: usize,
    pub(super) exhausted: bool,
}

impl DesktopFileScanBudget {
    pub(super) fn new() -> Self {
        Self {
            started: Instant::now(),
            visited_directories: 0,
            exhausted: false,
        }
    }

    pub(super) fn can_visit_directory(&mut self) -> bool {
        if self.visited_directories >= DESKTOP_FILE_SCAN_MAX_DIRECTORIES
            || self.started.elapsed() >= DESKTOP_FILE_SCAN_BUDGET
        {
            self.exhausted = true;
            return false;
        }
        self.visited_directories = self.visited_directories.saturating_add(1);
        true
    }

    pub(super) fn has_time_remaining(&mut self) -> bool {
        if self.started.elapsed() >= DESKTOP_FILE_SCAN_BUDGET {
            self.exhausted = true;
            return false;
        }
        true
    }
}

pub(super) struct DesktopFileIndexWatch {
    pub(super) _watcher: notify::RecommendedWatcher,
    pub(super) rx: mpsc::UnboundedReceiver<()>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WatchEventWait {
    Event,
    Timeout,
    Closed,
}

pub(super) fn watch_event_from_drain(saw_event: bool) -> WatchEventWait {
    if saw_event {
        WatchEventWait::Event
    } else {
        WatchEventWait::Timeout
    }
}

pub(super) fn is_sync_relevant_watch_event(event: &notify::Event) -> bool {
    use notify::event::{AccessKind, AccessMode, EventKind, MetadataKind, ModifyKind};

    match event.kind {
        EventKind::Create(_) | EventKind::Remove(_) => true,
        EventKind::Modify(ModifyKind::Data(_))
        | EventKind::Modify(ModifyKind::Name(_))
        | EventKind::Modify(ModifyKind::Any)
        | EventKind::Modify(ModifyKind::Other) => true,
        EventKind::Modify(ModifyKind::Metadata(MetadataKind::AccessTime)) => false,
        EventKind::Modify(ModifyKind::Metadata(_)) => true,
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => true,
        EventKind::Access(_) => false,
        EventKind::Any | EventKind::Other => true,
    }
}

pub(super) fn forward_sync_relevant_watch_event(
    tx: &mpsc::UnboundedSender<()>,
    event: notify::Result<notify::Event>,
) {
    if event.as_ref().is_ok_and(is_sync_relevant_watch_event) {
        let _ = tx.send(());
    }
}

impl DesktopFileIndexWatch {
    pub(super) fn new(scan_roots: &[DesktopFileScanRoot]) -> anyhow::Result<Option<Self>> {
        if scan_roots.is_empty() {
            return Ok(None);
        }
        let roots = scan_roots
            .iter()
            .map(|root| root.path.clone())
            .collect::<Vec<_>>();
        let (tx, rx) = mpsc::unbounded_channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            forward_sync_relevant_watch_event(&tx, event);
        })
        .context("create desktop file index watcher")?;
        for root in &roots {
            watcher
                .watch(root, notify::RecursiveMode::Recursive)
                .with_context(|| format!("watch desktop file scan root {}", root.display()))?;
        }
        Ok(Some(Self {
            _watcher: watcher,
            rx,
        }))
    }

    pub(super) fn drain_pending(&mut self) -> bool {
        let mut saw_event = false;
        while self.rx.try_recv().is_ok() {
            saw_event = true;
        }
        saw_event
    }

    pub(super) async fn wait_for_event(&mut self, timeout: Duration) -> WatchEventWait {
        if timeout.is_zero() {
            return watch_event_from_drain(self.drain_pending());
        }
        tokio::select! {
            _ = tokio::time::sleep(timeout) => watch_event_from_drain(self.drain_pending()),
            event = self.rx.recv() => {
                match event {
                    Some(_) => {
                        let _ = self.drain_pending();
                        WatchEventWait::Event
                    }
                    None => {
                        tokio::time::sleep(timeout).await;
                        WatchEventWait::Closed
                    }
                }
            }
        }
    }
}

fn desktop_file_scan_root_paths(scan_roots: &[DesktopFileScanRoot]) -> Vec<PathBuf> {
    scan_roots
        .iter()
        .map(|scan_root| scan_root.path.clone())
        .collect()
}

pub(super) async fn sync_desktop_file_index_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    let mut last_maintenance_at = SystemTime::UNIX_EPOCH;
    let mut last_projection_stamp: Option<DesktopFileIndexProjectionStamp> = None;
    let mut last_scan_roots_stamp: Option<DesktopFileScanRootsStamp> = None;
    let mut last_full_scan_at: Option<SystemTime> = None;
    let mut dirty_scan_roots = true;
    let mut file_watch: Option<DesktopFileIndexWatch> = None;
    let mut file_watch_roots: Option<Vec<PathBuf>> = None;
    let mut last_watch_error: Option<String> = None;
    loop {
        let started = Instant::now();
        let mut has_scan_roots = false;
        let result: anyhow::Result<usize> = async {
            let run_maintenance = last_maintenance_at
                .elapsed()
                .map(|elapsed| {
                    elapsed >= Duration::from_secs(DESKTOP_FILE_INDEX_MAINTENANCE_INTERVAL_SECS)
                })
                .unwrap_or(true);
            let scan_roots = desktop_file_scan_roots(&root);
            has_scan_roots = !scan_roots.is_empty();
            let scan_root_paths = desktop_file_scan_root_paths(&scan_roots);
            if file_watch_roots.as_ref() != Some(&scan_root_paths) {
                match DesktopFileIndexWatch::new(&scan_roots) {
                    Ok(next_watch) => {
                        file_watch = next_watch;
                        file_watch_roots = Some(scan_root_paths);
                        last_watch_error = None;
                        dirty_scan_roots = true;
                    }
                    Err(err) => {
                        let message = format!("{err:#}");
                        if last_watch_error.as_deref() != Some(message.as_str()) {
                            eprintln!(
                                "[business-os] desktop file index watcher unavailable: {message}"
                            );
                        }
                        last_watch_error = Some(message);
                        file_watch = None;
                        file_watch_roots = None;
                    }
                }
            }
            if file_watch
                .as_mut()
                .map(DesktopFileIndexWatch::drain_pending)
                .unwrap_or(false)
            {
                dirty_scan_roots = true;
            }
            let scan_roots_stamp = desktop_file_scan_roots_stamp(&scan_roots);
            let should_collect_scan = desktop_file_index_should_collect_scan(
                last_scan_roots_stamp.as_ref(),
                last_full_scan_at,
                &scan_roots_stamp,
                dirty_scan_roots,
                SystemTime::now(),
            );
            if !run_maintenance && !should_collect_scan {
                return Ok(0);
            }

            if run_maintenance {
                match compact_desktop_file_index_store(&root).await {
                    Ok(stats) if stats.changed() => {
                        log_desktop_file_index_maintenance_stats(&stats);
                    }
                    Ok(_) => {}
                    Err(err) => {
                        eprintln!("[business-os] desktop file index maintenance failed: {err:#}");
                    }
                }
                last_maintenance_at = SystemTime::now();
            }
            if !should_collect_scan {
                return Ok(0);
            }

            let scan = collect_desktop_file_index_scan(scan_roots).await?;
            last_scan_roots_stamp = Some(scan_roots_stamp);
            last_full_scan_at = Some(SystemTime::now());
            let projection_changed = last_projection_stamp.as_ref() != Some(&scan.stamp);
            if !projection_changed {
                dirty_scan_roots = false;
                return Ok(0);
            }
            let projection_stamp = scan.stamp.clone();
            let _guard = database_write_lock.lock().await;
            let indexed = sync_desktop_file_scan_with_database(&root, &database, scan).await?;
            last_projection_stamp = Some(projection_stamp);
            dirty_scan_roots = false;
            Ok(indexed)
        }
        .await;
        record_desktop_file_index_loop_result(&result, started.elapsed());
        let result_failed = result.is_err();
        if let Err(err) = &result {
            eprintln!("[business-os] native rxdb desktop file index failed: {err:#}");
        }
        let sleep_for = if result_failed {
            Duration::from_secs(DESKTOP_FILE_SCAN_INTERVAL_SECS)
        } else {
            desktop_file_index_sleep_interval(
                has_scan_roots,
                last_maintenance_at,
                last_full_scan_at,
                SystemTime::now(),
            )
        };
        let watch_wait = if let Some(watch) = file_watch.as_mut() {
            watch.wait_for_event(sleep_for).await
        } else {
            tokio::time::sleep(sleep_for).await;
            WatchEventWait::Timeout
        };
        match watch_wait {
            WatchEventWait::Event => {
                dirty_scan_roots = true;
            }
            WatchEventWait::Closed => {
                file_watch = None;
                file_watch_roots = None;
                dirty_scan_roots = true;
            }
            WatchEventWait::Timeout => {}
        }
    }
}

pub(super) async fn upsert_desktop_file_with_policy(
    root: &Path,
    database: &Arc<RxDatabase>,
    path: PathBuf,
    policy: DesktopFileContentPolicy,
    force_chunk_verification: bool,
) -> anyhow::Result<()> {
    let _write_guard = NATIVE_RXDB_WRITE_LOCK.lock().await;
    upsert_desktop_file_with_parent(
        root,
        database,
        path,
        policy,
        force_chunk_verification,
        CTOX_DESKTOP_FOLDER_ID.to_string(),
        None,
    )
    .await
}

/// Expected number of chunk documents for an eagerly-synced desktop file of
/// `size_bytes`: base64 with padding (4 chars per 3 input bytes), split into
/// DESKTOP_FILE_CHUNK_SIZE-character chunks; empty files still get one empty
/// chunk. Mirrors the write path's `total` computation exactly.
pub(super) fn expected_desktop_file_chunk_total(size_bytes: u64) -> u64 {
    let encoded_len = size_bytes.div_ceil(3) * 4;
    encoded_len.div_ceil(DESKTOP_FILE_CHUNK_SIZE as u64).max(1)
}

pub(super) fn desktop_file_generation_verified_by_metadata(
    document: &Value,
    generation_id: &str,
    size_bytes: u64,
) -> bool {
    if generation_id.is_empty() {
        return false;
    }
    document
        .get("content_generation_id")
        .and_then(Value::as_str)
        == Some(generation_id)
        && document.get("content_state").and_then(Value::as_str) == Some("available")
        && document.get("chunk_count").and_then(Value::as_u64)
            == Some(expected_desktop_file_chunk_total(size_bytes))
        && document
            .get("generation_verified_at_ms")
            .and_then(Value::as_u64)
            .is_some_and(|value| value > 0)
}

pub(super) async fn mark_desktop_file_chunk_generation_verified(
    files: &Arc<RxCollection>,
    document: &Value,
    expected_total: u64,
    now: u128,
) -> anyhow::Result<()> {
    let mut next = document.clone();
    let Some(object) = next.as_object_mut() else {
        return Ok(());
    };
    object.insert("chunk_count".to_string(), Value::from(expected_total));
    object.insert(
        "generation_verified_at_ms".to_string(),
        Value::from(u64::try_from(now).unwrap_or(u64::MAX)),
    );
    files
        .incremental_upsert(next)
        .await
        .map_err(|err| anyhow::anyhow!("mark desktop file chunks verified: {err}"))?;
    Ok(())
}

/// True when the chunk store holds the complete LIVE chunk set for the given
/// generation. The scan's change-detection fast path must not skip a file
/// whose chunks went missing (crash window, manual cleanup,
/// `ctox.file.materialize` repair) just because the file-doc fingerprint
/// still matches — the index has to stay self-healing.
pub(super) async fn desktop_file_chunk_generation_is_complete(
    database: &Arc<RxDatabase>,
    file_id: &str,
    generation_id: &str,
    size_bytes: u64,
) -> bool {
    #[cfg(test)]
    DESKTOP_FILE_CHUNK_COMPLETENESS_CHECKS.fetch_add(1, Ordering::Relaxed);
    if generation_id.is_empty() {
        return false;
    }
    let Some(chunks) = database.collection("desktop_file_chunks") else {
        return false;
    };
    let expected_total = expected_desktop_file_chunk_total(size_bytes);
    let Ok(expected_total_usize) = usize::try_from(expected_total) else {
        return false;
    };
    // Verification must be bounded too: a republish must not hydrate the
    // complete Base64 generation just to establish chunk presence.
    for start in (0..expected_total_usize).step_by(stream::BATCH_CHUNKS) {
        let end = start
            .saturating_add(stream::BATCH_CHUNKS)
            .min(expected_total_usize);
        let ids: Vec<String> = (start..end)
            .map(|idx| format!("{file_id}_{generation_id}_{idx}"))
            .collect();
        let Ok(documents) = chunks
            .storage_instance
            .find_documents_by_id(&ids, false)
            .await
        else {
            return false;
        };
        if documents.len() != ids.len() {
            return false;
        }
        let mut seen_indices = HashSet::with_capacity(documents.len());
        for document in documents {
            if document.get("file_id").and_then(Value::as_str) != Some(file_id)
                || document.get("generation_id").and_then(Value::as_str) != Some(generation_id)
                || document.get("total").and_then(Value::as_u64) != Some(expected_total)
            {
                return false;
            }
            let Some(idx) = document.get("idx").and_then(Value::as_u64) else {
                return false;
            };
            if idx < start as u64 || idx >= end as u64 || !seen_indices.insert(idx) {
                return false;
            }
        }
    }
    true
}

pub(super) async fn upsert_desktop_file_with_parent(
    root: &Path,
    database: &Arc<RxDatabase>,
    path: PathBuf,
    policy: DesktopFileContentPolicy,
    force_chunk_verification: bool,
    parent_id: String,
    virtual_path: Option<String>,
) -> anyhow::Result<()> {
    let metadata = fs::metadata(&path)
        .with_context(|| format!("failed to read desktop file metadata {}", path.display()))?;
    if !metadata.is_file() {
        anyhow::bail!(
            "desktop file sync only supports regular files: {}",
            path.display()
        );
    }

    let now = now_ms();
    let file_id = desktop_file_id(&path);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file")
        .to_string();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_string();
    let path_string = path.to_string_lossy().into_owned();
    let display_path = virtual_path
        .unwrap_or_else(|| format!("{}/{}", CTOX_DESKTOP_FOLDER_PATH, file_name.as_str()));
    let modified_at_ms = metadata_modified_at_ms(&metadata);

    // Change detection: the desktop-file index rescans every workspace root
    // every DESKTOP_FILE_SCAN_INTERVAL_SECS. Without this check every scan
    // minted a fresh (timestamped) generation id for EVERY file, re-wrote all
    // its chunks and tombstoned the previous generation — a permanent
    // insert/tombstone churn that browser-side replication (batchSize 2 for
    // desktop_file_chunks) could never catch up with. Skip files whose
    // on-disk fingerprint still matches the indexed document; below, reuse
    // the stored generation when only metadata changed but content did not.
    let files = database
        .collection("desktop_files")
        .context("desktop_files collection is not registered")?;
    let existing_file_doc = find_rxdb_document_by_id(database, "desktop_files", &file_id, false)
        .await
        .map_err(|err| anyhow::anyhow!("read desktop file doc {file_id}: {err}"))?;
    // Materialization is sticky: once a file was explicitly materialized
    // (ctox.file.materialize set content_state 'available'), the periodic
    // scan must NOT demote it back to its size/extension policy 'lazy'.
    // Demoting rewrote the file doc with an empty content_generation_id,
    // stranded the already-replicated chunks and reverted the browser file
    // viewer to an unreadable lazy state ~15s after every materialize
    // (rxdb-soak workspace-large-file-viewer-restart). Keep maintaining such
    // files eagerly; a content change re-chunks them below.
    let policy = if policy == DesktopFileContentPolicy::Lazy
        && existing_file_doc
            .as_ref()
            .and_then(|doc| doc.get("content_state"))
            .and_then(Value::as_str)
            == Some("available")
    {
        DesktopFileContentPolicy::Eager
    } else {
        policy
    };
    if let Some(doc) = existing_file_doc.as_ref() {
        let same_location = doc.get("parent_id").and_then(Value::as_str)
            == Some(parent_id.as_str())
            && doc.get("virtual_path").and_then(Value::as_str) == Some(display_path.as_str())
            && doc.get("path").and_then(Value::as_str) == Some(path_string.as_str());
        let same_stat = doc.get("mtime_ms").and_then(Value::as_u64)
            == u64::try_from(modified_at_ms).ok()
            && doc.get("size_bytes").and_then(Value::as_u64) == Some(metadata.len());
        let not_deleted = !doc
            .get("is_deleted")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let content_ready = match policy {
            DesktopFileContentPolicy::Eager => {
                doc.get("content_state").and_then(Value::as_str) == Some("available")
                    && doc
                        .get("content_generation_id")
                        .and_then(Value::as_str)
                        .is_some_and(|generation| !generation.is_empty())
            }
            DesktopFileContentPolicy::Lazy => {
                doc.get("content_state").and_then(Value::as_str) == Some("lazy")
            }
        };
        if same_location && same_stat && not_deleted && content_ready {
            // Self-healing is expensive for large materialized files, so use
            // the persisted verification marker first. Full chunk verification
            // is reserved for generations that have not been verified yet.
            let chunks_complete = match policy {
                DesktopFileContentPolicy::Eager => {
                    let generation = doc
                        .get("content_generation_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if !force_chunk_verification
                        && desktop_file_generation_verified_by_metadata(
                            doc,
                            generation,
                            metadata.len(),
                        )
                    {
                        true
                    } else if desktop_file_chunk_generation_is_complete(
                        database,
                        &file_id,
                        generation,
                        metadata.len(),
                    )
                    .await
                    {
                        mark_desktop_file_chunk_generation_verified(
                            &files,
                            doc,
                            expected_desktop_file_chunk_total(metadata.len()),
                            now,
                        )
                        .await?;
                        true
                    } else {
                        false
                    }
                }
                DesktopFileContentPolicy::Lazy => true,
            };
            if chunks_complete {
                return Ok(());
            }
        }
    }

    let (content_hash, content_generation_id, active_generation_id) = if policy
        == DesktopFileContentPolicy::Eager
    {
        let mut content_file = fs::File::open(&path)
            .with_context(|| format!("failed to read desktop file {}", path.display()))?;
        let (hashed_size, content_hash) = stream::hash_and_rewind(&mut content_file)?;
        anyhow::ensure!(
            hashed_size == metadata.len(),
            "desktop file changed while hashing"
        );
        // Same content as the indexed generation (e.g. touch / metadata
        // change): keep the replicated generation and its chunks instead
        // of rotating a byte-identical copy through the data plane.
        let reused_generation_id = existing_file_doc.as_ref().and_then(|doc| {
            if doc.get("content_hash").and_then(Value::as_str) != Some(content_hash.as_str())
                || doc.get("content_state").and_then(Value::as_str) != Some("available")
            {
                return None;
            }
            doc.get("content_generation_id")
                .and_then(Value::as_str)
                .filter(|generation| !generation.is_empty())
                .map(str::to_string)
        });
        // Reuse only a generation whose chunks are complete; otherwise
        // fall through to a full rewrite (self-healing repair).
        let mut reused_generation_id = reused_generation_id;
        if let Some(generation) = reused_generation_id.as_deref() {
            let metadata_verified = !force_chunk_verification
                && existing_file_doc.as_ref().is_some_and(|doc| {
                    desktop_file_generation_verified_by_metadata(doc, generation, metadata.len())
                });
            if !metadata_verified
                && !desktop_file_chunk_generation_is_complete(
                    database,
                    &file_id,
                    generation,
                    metadata.len(),
                )
                .await
            {
                reused_generation_id = None;
            }
        }
        if let Some(generation_id) = reused_generation_id {
            (
                content_hash,
                Value::String(generation_id.clone()),
                Some(generation_id),
            )
        } else {
            let generation_suffix = content_hash.get(..12).unwrap_or(content_hash.as_str());
            let generation_id = format!("gen_{now}_{generation_suffix}");
            let total = expected_desktop_file_chunk_total(hashed_size);
            let chunks = database
                .collection("desktop_file_chunks")
                .context("desktop_file_chunks collection is not registered")?;

            let mut materialized_hash = sha2::Sha256::new();
            let mut materialized_size = 0_u64;
            let mut chunk_documents = Vec::with_capacity(stream::BATCH_CHUNKS);
            for idx in 0..total {
                // 12KiB is divisible by three, so only the final Base64
                // chunk has padding, exactly as in the existing format.
                let bytes = stream::read_chunk(&mut content_file)?;
                materialized_hash.update(&bytes);
                materialized_size += bytes.len() as u64;
                let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
                let chunk_hash = hex_sha256(data.as_bytes());
                chunk_documents.push(json!({
                    "id": format!("{file_id}_{generation_id}_{idx}"),
                    "file_id": file_id,
                    "generation_id": generation_id.clone(),
                    "content_hash": content_hash.clone(),
                    "content_hash_scheme": DESKTOP_FILE_CONTENT_HASH_SCHEME,
                    "idx": idx,
                    "total": total,
                    "encoding": "base64",
                    "data": data,
                    "chunk_hash": chunk_hash,
                    "chunk_hash_scheme": DESKTOP_FILE_CHUNK_HASH_SCHEME,
                    "size_bytes": data.len() as u64,
                    "created_at_ms": now,
                }));
                if chunk_documents.len() == stream::BATCH_CHUNKS {
                    bulk_upsert_or_error(
                        &chunks,
                        std::mem::take(&mut chunk_documents),
                        "upsert desktop file chunks",
                    )
                    .await?;
                }
            }
            // Partial batches may exist after failure, but no available file
            // generation is published until both passes agree in full.
            anyhow::ensure!(
                stream::read_chunk(&mut content_file)?.is_empty()
                    && materialized_size == hashed_size
                    && format!("{:x}", materialized_hash.finalize()) == content_hash,
                "desktop file changed while materializing"
            );
            bulk_upsert_or_error(&chunks, chunk_documents, "upsert desktop file chunks").await?;
            (
                content_hash,
                Value::String(generation_id.clone()),
                Some(generation_id),
            )
        }
    } else {
        (
            format!("mtime:{modified_at_ms}:size:{}", metadata.len()),
            Value::Null,
            None,
        )
    };

    ensure_ctox_desktop_folder(database, now).await?;
    let now_u64 = u64::try_from(now).unwrap_or(u64::MAX);
    let content_synced_at_ms = if policy == DesktopFileContentPolicy::Eager {
        Value::from(now_u64)
    } else {
        Value::Null
    };
    let content_state = if policy == DesktopFileContentPolicy::Eager {
        "available"
    } else {
        "lazy"
    };
    files
        .incremental_upsert(json!({
            "id": file_id,
            "parent_id": parent_id,
            "path": path_string,
            "local_path": path_string,
            "virtual_path": display_path,
            "name": file_name,
            "kind": "file",
            "mime_type": mime_type_for_path(&path),
            "extension": extension,
            "size_bytes": metadata.len(),
            "owner_id": "ctox",
            "source": "ctox-core",
            "content_ref": file_id,
            "content_state": content_state,
            "content_hash": content_hash,
            "content_hash_scheme": DESKTOP_FILE_CONTENT_HASH_SCHEME,
            "content_generation_id": content_generation_id,
            "chunk_count": if policy == DesktopFileContentPolicy::Eager {
                Value::from(expected_desktop_file_chunk_total(metadata.len()))
            } else {
                Value::Null
            },
            "generation_verified_at_ms": if policy == DesktopFileContentPolicy::Eager {
                Value::from(now_u64)
            } else {
                Value::Null
            },
            "mtime_ms": modified_at_ms,
            "content_synced_at_ms": content_synced_at_ms,
            "sort_index": now,
            "is_deleted": false,
            // Keep the original creation time stable across index updates.
            "created_at_ms": existing_file_doc
                .as_ref()
                .and_then(|doc| doc.get("created_at_ms"))
                .and_then(Value::as_u64)
                .map(Value::from)
                .unwrap_or_else(|| json!(now)),
            "updated_at_ms": now,
        }))
        .await
        .map_err(|err| anyhow::anyhow!("upsert desktop file row: {err}"))?;

    if let Some(active_generation_id) = active_generation_id.as_deref() {
        prune_desktop_file_chunk_generations(root, database, &file_id, active_generation_id)
            .await?;
    }

    if extension.eq_ignore_ascii_case("csv") {
        if let Err(err) =
            upsert_workspace_csv_spreadsheet(database, &path, &file_id, &display_path, now_u64)
                .await
        {
            eprintln!(
                "[business-os] published CSV to Files but skipped automatic Spreadsheet projection for {}: {err:#}",
                path.display()
            );
        }
    }

    Ok(())
}

// MOVED_ITEMS
