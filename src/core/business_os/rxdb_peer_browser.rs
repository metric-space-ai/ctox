// Origin: CTOX
// License: Apache-2.0

use super::browser_control::{
    browser_context_related_document, browser_context_snapshot_with_database,
    browser_frame_capture_dimensions, browser_frame_capture_request,
    browser_session_automation_with_database, browser_session_status_with_database,
    redact_browser_frame_data, redacted_browser_context_capture, redacted_browser_session_status,
    BROWSER_FRAME_GC_LIMIT, BROWSER_FRAME_JPEG_QUALITY, BROWSER_FRAME_RATE_IDLE,
    BROWSER_FRAME_RATE_LIMIT, BROWSER_FRAME_RATE_TARGET_DEFAULT, BROWSER_FRAME_RECENT_KEEP_COUNT,
    BROWSER_INPUT_ACTIVE_WINDOW_MS, BROWSER_INPUT_EVENT_GC_LIMIT,
    BROWSER_INPUT_EVENT_RETENTION_SECS, BROWSER_RUNTIME_ACTIVE_MAINTENANCE_INTERVAL_MS,
    BROWSER_RUNTIME_COMMAND_LOCK, BROWSER_RUNTIME_IDLE_BACKOFF_AFTER_TICKS,
    BROWSER_RUNTIME_IDLE_MAINTENANCE_INTERVAL_SECS, BROWSER_SESSION_ABANDONED_AFTER_MS,
    BUSINESS_COMMAND_GC_LIMIT, BUSINESS_COMMAND_RETENTION_MS,
};
use super::browser_runtime::{browser_runtime_manager, BrowserSessionAutomationRequest};
use super::policy::BusinessOsPermission;
use super::rxdb_peer::{
    now_ms, record_native_peer_loop_result, rxdb_collection_version_table_name,
    NativePeerLoopMetrics,
};
use super::rxdb_peer_tombstones::{
    sweep_tombstones_once, TOMBSTONE_SWEEP_DRAIN_INTERVAL_SECS, TOMBSTONE_SWEEP_IDLE_INTERVAL_SECS,
};
use super::store;
use crate::mission::channels;
use anyhow::Context;
use base64::Engine;
use rxdb::rx_database::RxDatabase;
use rxdb::types::MangoQuery;
use serde_json::{json, Value};
use sha2::Digest;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::Mutex as AsyncMutex;

pub(super) static BROWSER_RUNTIME_LOOP_METRICS: NativePeerLoopMetrics =
    NativePeerLoopMetrics::new("browser_runtime");
static BROWSER_FRAME_CAPTURE_SLOTS: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
static BROWSER_DIRECT_LIVE_SESSIONS: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
pub(super) const BROWSER_LIVE_WEBRTC_METHOD: &str = "ctox.browser.live.v1";

struct BrowserLiveMetrics {
    received: AtomicU64,
    completed: AtomicU64,
    failed: AtomicU64,
    active: AtomicU64,
    total_duration_ms: AtomicU64,
    max_duration_ms: AtomicU64,
    last_duration_ms: AtomicU64,
}

static BROWSER_LIVE_METRICS: BrowserLiveMetrics = BrowserLiveMetrics {
    received: AtomicU64::new(0),
    completed: AtomicU64::new(0),
    failed: AtomicU64::new(0),
    active: AtomicU64::new(0),
    total_duration_ms: AtomicU64::new(0),
    max_duration_ms: AtomicU64::new(0),
    last_duration_ms: AtomicU64::new(0),
};
static BROWSER_LIVE_LAST_FAILURE: OnceLock<Mutex<Option<(String, String)>>> = OnceLock::new();

pub(super) fn browser_live_metrics_snapshot() -> Value {
    json!({
        "schema": "ctox.browser_live.runtime_counters.v1",
        "received": BROWSER_LIVE_METRICS.received.load(Ordering::Relaxed),
        "completed": BROWSER_LIVE_METRICS.completed.load(Ordering::Relaxed),
        "failed": BROWSER_LIVE_METRICS.failed.load(Ordering::Relaxed),
        "active": BROWSER_LIVE_METRICS.active.load(Ordering::Relaxed),
        "total_duration_ms": BROWSER_LIVE_METRICS.total_duration_ms.load(Ordering::Relaxed),
        "max_duration_ms": BROWSER_LIVE_METRICS.max_duration_ms.load(Ordering::Relaxed),
        "last_duration_ms": BROWSER_LIVE_METRICS.last_duration_ms.load(Ordering::Relaxed),
        "last_failed_operation": BROWSER_LIVE_LAST_FAILURE
            .get_or_init(|| Mutex::new(None))
            .lock()
            .ok()
            .and_then(|failure| failure.as_ref().map(|(operation, _)| operation.clone())),
        "last_failure_kind": BROWSER_LIVE_LAST_FAILURE
            .get_or_init(|| Mutex::new(None))
            .lock()
            .ok()
            .and_then(|failure| failure.as_ref().map(|(_, kind)| kind.clone())),
    })
}

fn browser_live_failure_kind(error: &str) -> &'static str {
    if error.contains("capability is invalid") {
        "capability_invalid"
    } else if error.contains("may not read") {
        "read_not_allowed"
    } else if error.contains("may not control") {
        "control_not_allowed"
    } else if error.contains("session_id is required") {
        "session_id_missing"
    } else if error.contains("controller lease is required") {
        "lease_missing"
    } else if error.contains("session was not found") {
        "session_not_found"
    } else if error.contains("session belongs to another user") {
        "session_owner_mismatch"
    } else if error.contains("lease is missing or expired") {
        "lease_mismatch_or_expired"
    } else if error.contains("runtime is not running") {
        "runtime_not_running"
    } else if error.contains("runtime belongs to another user") {
        "runtime_owner_mismatch"
    } else if error.contains("navigation") {
        "runtime_navigation_failed"
    } else if error.contains("input request") {
        "runtime_input_failed"
    } else if error.contains("unsupported browser live operation") {
        "operation_unsupported"
    } else {
        "other"
    }
}

fn record_browser_live_duration(duration_ms: u64) {
    BROWSER_LIVE_METRICS
        .last_duration_ms
        .store(duration_ms, Ordering::Relaxed);
    BROWSER_LIVE_METRICS
        .total_duration_ms
        .fetch_add(duration_ms, Ordering::Relaxed);
    let mut current = BROWSER_LIVE_METRICS.max_duration_ms.load(Ordering::Relaxed);
    while duration_ms > current {
        match BROWSER_LIVE_METRICS.max_duration_ms.compare_exchange_weak(
            current,
            duration_ms,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(next) => current = next,
        }
    }
}

fn mark_browser_direct_live_session(session_id: &str) {
    if let Ok(mut sessions) = BROWSER_DIRECT_LIVE_SESSIONS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    {
        sessions.insert(
            session_id.to_string(),
            Instant::now() + Duration::from_secs(2),
        );
    }
}

fn reserve_browser_direct_live_session(session_id: &str) {
    if let Ok(mut sessions) = BROWSER_DIRECT_LIVE_SESSIONS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    {
        // A cold browser command plus its session projection has a measured
        // tail near one minute on the tenant. Keep the legacy RxDB frame loop
        // quiet long enough for the direct client to attach; old clients fall
        // back automatically when this reservation expires.
        sessions.insert(
            session_id.to_string(),
            Instant::now() + Duration::from_secs(90),
        );
    }
}

fn browser_direct_live_session_active(session_id: &str) -> bool {
    let Ok(mut sessions) = BROWSER_DIRECT_LIVE_SESSIONS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    else {
        return false;
    };
    let now = Instant::now();
    sessions.retain(|_, expires_at| *expires_at > now);
    sessions.contains_key(session_id)
}

/// Serve one latency-sensitive Browser input/frame exchange directly over the
/// authenticated WebRTC DataChannel. Durable session/navigation projections
/// remain in RxDB, but pointer, keyboard and JPEG payloads no longer wait for
/// two collection replications plus IndexedDB writes on every frame.
pub(super) async fn handle_browser_live_webrtc_request(
    root: &Path,
    database: &Arc<RxDatabase>,
    capability_token: &str,
    params: Vec<Value>,
) -> Result<Value, String> {
    let started = Instant::now();
    let operation = params
        .first()
        .and_then(|request| request.get("op"))
        .and_then(Value::as_str)
        .unwrap_or("live")
        .to_string();
    BROWSER_LIVE_METRICS
        .received
        .fetch_add(1, Ordering::Relaxed);
    BROWSER_LIVE_METRICS.active.fetch_add(1, Ordering::Relaxed);
    let result =
        handle_browser_live_webrtc_request_inner(root, database, capability_token, params).await;
    BROWSER_LIVE_METRICS.active.fetch_sub(1, Ordering::Relaxed);
    record_browser_live_duration(started.elapsed().as_millis() as u64);
    if result.is_ok() {
        BROWSER_LIVE_METRICS
            .completed
            .fetch_add(1, Ordering::Relaxed);
    } else {
        BROWSER_LIVE_METRICS.failed.fetch_add(1, Ordering::Relaxed);
        if let Some(error) = result.as_ref().err() {
            if let Ok(mut last_failure) = BROWSER_LIVE_LAST_FAILURE
                .get_or_init(|| Mutex::new(None))
                .lock()
            {
                *last_failure = Some((operation, browser_live_failure_kind(error).to_string()));
            }
        }
    }
    result
}

async fn handle_browser_live_webrtc_request_inner(
    root: &Path,
    database: &Arc<RxDatabase>,
    capability_token: &str,
    params: Vec<Value>,
) -> Result<Value, String> {
    let request = params.first().cloned().unwrap_or(Value::Null);
    let operation = request.get("op").and_then(Value::as_str).unwrap_or("live");
    let (profile_owner, _) = store::verify_webrtc_capability_actor(root, capability_token)
        .ok_or_else(|| "browser live capability is invalid".to_string())?;
    if !store::webrtc_capability_allows_collection_permission(
        root,
        capability_token,
        "browser_sessions",
        BusinessOsPermission::DataRead,
    ) {
        return Err("browser live capability may not read browser sessions".to_string());
    }
    if operation == "session.list" {
        let sessions = database
            .collection("browser_sessions")
            .ok_or_else(|| "browser_sessions collection is not registered".to_string())?
            .find(Some(MangoQuery {
                selector: Some(json!({ "owner_user_id": { "$eq": profile_owner } })),
                sort: Some(vec![[("updated_at_ms".to_string(), "desc".to_string())]
                    .into_iter()
                    .collect()]),
                limit: Some(12),
                ..Default::default()
            }))
            .map_err(|error| format!("browser session list query failed: {error}"))?
            .exec(false)
            .await
            .map_err(|error| format!("browser session list failed: {error}"))?;
        let sessions = sessions
            .as_array()
            .into_iter()
            .flatten()
            .map(|session| {
                json!({
                    "id": session.get("id").cloned().unwrap_or(Value::Null),
                    "title": session.get("title").cloned().unwrap_or(Value::Null),
                    "current_url": session.get("current_url").cloned().unwrap_or(Value::Null),
                    "status": session.get("status").cloned().unwrap_or(Value::Null),
                    "runtime_status": session.get("runtime_status").cloned().unwrap_or(Value::Null),
                    "profile_mode": session.get("profile_mode").cloned().unwrap_or(Value::Null),
                    "owner_user_id": session.get("owner_user_id").cloned().unwrap_or(Value::Null),
                    "controller_user_id": session.get("controller_user_id").cloned().unwrap_or(Value::Null),
                    "controller_lease_id": session.get("controller_lease_id").cloned().unwrap_or(Value::Null),
                    "controller_lease_expires_at_ms": session.get("controller_lease_expires_at_ms").cloned().unwrap_or(Value::Null),
                    "current_tab_id": session.get("current_tab_id").cloned().unwrap_or(Value::Null),
                    "viewport_w": session.get("viewport_w").cloned().unwrap_or(Value::Null),
                    "viewport_h": session.get("viewport_h").cloned().unwrap_or(Value::Null),
                    "error": session.get("error").cloned().unwrap_or(Value::Null),
                    // The Browser app renders website permissions, HTTP auth,
                    // WebAuthn/dialog prompts, and the web-stack handoff from
                    // this owner-scoped fast path. These states live in the
                    // canonical session payload; omitting it made a healthy
                    // authentication session look like an ordinary browser
                    // window until the slower RxDB projection caught up.
                    // Session payloads contain secret references only. Secret
                    // values are deliberately never persisted in RxDB.
                    "payload": session.get("payload").cloned().unwrap_or(Value::Null),
                    "updated_at_ms": session.get("updated_at_ms").cloned().unwrap_or(Value::Null),
                })
            })
            .collect::<Vec<_>>();
        return Ok(json!({
            "ok": true,
            "sessions": sessions,
        }));
    }
    let session_id = request
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "browser live session_id is required".to_string())?;
    let lease_id = request
        .get("lease_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "browser live controller lease is required".to_string())?;
    // `requestNative()` travels on the authenticated `browser_sessions`
    // replication channel, so its capability is intentionally scoped to that
    // collection. Requiring an additional `browser_input_events` capability
    // here made the direct path impossible: collection-scoped tokens cannot
    // authorize two collections at once, and every client silently fell back
    // to the slow persisted-input/frame loop. Session write permission plus
    // the owner/controller/lease checks below is the authority to operate this
    // user's ephemeral live session; no input-event document is persisted on
    // this path.
    if !store::webrtc_capability_allows_collection_permission(
        root,
        capability_token,
        "browser_sessions",
        BusinessOsPermission::DataWrite,
    ) {
        return Err("browser live capability may not control browser sessions".to_string());
    }
    if operation == "session.start" {
        return start_browser_live_webrtc_session(
            root,
            database,
            &request,
            session_id,
            lease_id,
            &profile_owner,
        )
        .await;
    }
    let session_doc = find_browser_document(database, "browser_sessions", session_id)
        .await
        .map_err(|error| format!("browser live session lookup failed: {error:#}"))?;
    if !session_doc.is_object() {
        return Err("browser live session was not found".to_string());
    }
    let owner = session_doc
        .get("owner_user_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let controller = session_doc
        .get("controller_user_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let current_lease = session_doc
        .get("controller_lease_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let lease_expires_at = session_doc
        .get("controller_lease_expires_at_ms")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if owner != profile_owner {
        return Err("browser live session belongs to another user".to_string());
    }
    if matches!(
        operation,
        "controller.acquire" | "controller.renew" | "controller.release"
    ) {
        let now = now_ms() as u64;
        match operation {
            "controller.acquire" => {
                if !controller.is_empty() && controller != profile_owner && lease_expires_at > now {
                    return Err("browser session is controlled by another user".to_string());
                }
            }
            "controller.renew" | "controller.release" => {
                if controller != profile_owner || current_lease != lease_id {
                    return Err("browser controller lease does not match".to_string());
                }
            }
            _ => unreachable!(),
        }
        let released = operation == "controller.release";
        let access = BrowserSessionAccess {
            tenant_id: None,
            owner_user_id: Some(profile_owner.clone()),
            controller_user_id: Some(if released {
                String::new()
            } else {
                profile_owner.clone()
            }),
            controller_lease_id: Some(if released {
                String::new()
            } else {
                lease_id.to_string()
            }),
            controller_lease_expires_at_ms: Some(if released { 0 } else { now + 120_000 }),
        };
        let tab_id = session_doc
            .get("current_tab_id")
            .and_then(Value::as_str)
            .unwrap_or("browser_tab_default");
        let status = session_doc
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("active");
        let runtime_status = session_doc
            .get("runtime_status")
            .and_then(Value::as_str)
            .unwrap_or(status);
        let url = session_doc
            .get("current_url")
            .and_then(Value::as_str)
            .unwrap_or("https://example.com");
        let title = session_doc
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("Remote Browser");
        let viewport_w = session_doc
            .get("viewport_w")
            .and_then(Value::as_u64)
            .unwrap_or(1280);
        let viewport_h = session_doc
            .get("viewport_h")
            .and_then(Value::as_u64)
            .unwrap_or(720);
        let frame_id = session_doc
            .get("active_frame_id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty());
        let frame_seq = session_doc
            .get("last_frame_seq")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        upsert_browser_session(
            database,
            session_id,
            tab_id,
            status,
            runtime_status,
            url,
            title,
            viewport_w,
            viewport_h,
            frame_id,
            frame_seq,
            operation,
            now,
            None,
            Some(&access),
            None,
        )
        .await
        .map_err(|error| format!("browser controller lease update failed: {error:#}"))?;
        return Ok(json!({
            "ok": true,
            "operation": operation,
            "lease_id": if released { "" } else { lease_id },
            "lease_expires_at_ms": if released { 0 } else { now + 120_000 },
        }));
    }
    if controller != profile_owner
        || current_lease != lease_id
        || lease_expires_at <= now_ms() as u64
    {
        return Err("browser live controller lease is missing or expired".to_string());
    }
    let manager = browser_runtime_manager();
    let session = manager
        .get(session_id)
        .ok_or_else(|| "browser live runtime is not running".to_string())?;
    if session.owner_user_id != profile_owner {
        return Err("browser live runtime belongs to another user".to_string());
    }
    mark_browser_direct_live_session(session_id);
    if matches!(operation, "navigate" | "reload" | "back" | "forward") {
        let target_url = if operation == "navigate" {
            request
                .get("url")
                .and_then(Value::as_str)
                .map(normalize_browser_runtime_url)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "browser live navigation URL is required".to_string())?
        } else {
            session_doc
                .get("current_url")
                .and_then(Value::as_str)
                .unwrap_or("https://example.com")
                .to_string()
        };
        let op_params = if operation == "navigate" {
            json!({ "url": target_url, "timeoutMs": 30_000 })
        } else {
            json!({ "timeoutMs": 30_000 })
        };
        let result = manager
            .request(&session, operation, op_params)
            .await
            .map_err(|error| format!("browser live navigation failed: {error:#}"))?;
        let nav = result.get("nav").cloned().unwrap_or(Value::Null);
        let final_url = nav
            .get("url")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or(&target_url);
        let title = nav
            .get("title")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or("Remote Browser");
        let tab_id = session_doc
            .get("current_tab_id")
            .and_then(Value::as_str)
            .unwrap_or("browser_tab_default");
        let viewport_w = session_doc
            .get("viewport_w")
            .and_then(Value::as_u64)
            .unwrap_or(1280);
        let viewport_h = session_doc
            .get("viewport_h")
            .and_then(Value::as_u64)
            .unwrap_or(720);
        let access = BrowserSessionAccess {
            tenant_id: session_doc
                .get("tenant_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            owner_user_id: Some(profile_owner.clone()),
            controller_user_id: Some(profile_owner.clone()),
            controller_lease_id: Some(lease_id.to_string()),
            controller_lease_expires_at_ms: Some(lease_expires_at),
        };
        upsert_browser_tab(
            database,
            tab_id,
            session_id,
            title,
            final_url,
            "active",
            false,
            nav.get("can_go_back")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            nav.get("can_go_forward")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            None,
            0,
            Some(&access),
        )
        .await
        .map_err(|error| format!("browser live tab projection failed: {error:#}"))?;
        upsert_browser_session(
            database,
            session_id,
            tab_id,
            "active",
            "active",
            final_url,
            title,
            viewport_w,
            viewport_h,
            None,
            session_doc
                .get("last_frame_seq")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            operation,
            now_ms() as u64,
            result.get("error").and_then(Value::as_str),
            Some(&access),
            None,
        )
        .await
        .map_err(|error| format!("browser live session projection failed: {error:#}"))?;
        return Ok(result);
    }
    let events = request
        .get("events")
        .and_then(Value::as_array)
        .map(|events| {
            events
                .iter()
                .take(64)
                .map(browser_runtime_input_event)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let max_seq = request
        .get("events")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|event| event.get("seq").and_then(Value::as_u64))
        .max()
        .unwrap_or(0);
    if operation == "input" {
        let response = manager
            .request(&session, "input", json!({ "events": events }))
            .await
            .map_err(|error| format!("browser live input request failed: {error:#}"))?;
        if max_seq > 0 {
            session.record_input_seq(max_seq);
        }
        return Ok(response);
    }
    // Read the automation script this session is actually running. Operators
    // asked to switch between the live view and the script inside the app,
    // because the source tree can differ from what a release shipped -- today
    // that difference cost a full day: the runner in the pinned build did not
    // implement `live` at all while the checkout did. Read-only: editing the
    // driver of a session that already loaded it would race with the process.
    if operation == "script" {
        // Reading it locks the runtime handle and touches the disk, so it goes
        // to a blocking worker like every other request against the process.
        let session = Arc::clone(&session);
        let (script, path) = tokio::task::spawn_blocking(move || session.runner_script())
            .await
            .map_err(|error| format!("browser live script worker panicked: {error}"))?
            .map_err(|error| format!("browser live script read failed: {error:#}"))?;
        return Ok(json!({
            "ok": true,
            "operation": "script",
            "path": path,
            "bytes": script.len(),
            "script": script,
        }));
    }
    if operation != "live" {
        return Err(format!("unsupported browser live operation: {operation}"));
    }
    let response = manager
        .request(
            &session,
            "live",
            json!({
                "events": events,
                "format": "jpeg",
                "quality": BROWSER_FRAME_JPEG_QUALITY,
                "frameAfterMs": request
                    .get("frame_after_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            }),
        )
        .await
        .map_err(|error| format!("browser live runtime request failed: {error:#}"))?;
    if max_seq > 0 {
        session.record_input_seq(max_seq);
    }
    Ok(response)
}

async fn start_browser_live_webrtc_session(
    root: &Path,
    database: &Arc<RxDatabase>,
    request: &Value,
    session_id: &str,
    lease_id: &str,
    profile_owner: &str,
) -> Result<Value, String> {
    let _guard = BROWSER_RUNTIME_COMMAND_LOCK.lock().await;
    let tab_id = request
        .get("tab_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("browser_tab_default");
    let viewport_w = request
        .get("viewport_w")
        .and_then(Value::as_u64)
        .unwrap_or(1280)
        .clamp(320, 3840);
    let viewport_h = request
        .get("viewport_h")
        .and_then(Value::as_u64)
        .unwrap_or(720)
        .clamp(240, 2160);
    let target_url = request
        .get("url")
        .and_then(Value::as_str)
        .map(normalize_browser_runtime_url)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "https://example.com".to_string());
    let private_profile = request
        .get("profile_mode")
        .and_then(Value::as_str)
        .is_some_and(|mode| mode == "private");
    let existing = find_browser_document(database, "browser_sessions", session_id)
        .await
        .map_err(|error| format!("browser direct start session lookup failed: {error:#}"))?;
    if let Some(owner) = existing
        .get("owner_user_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "ctox")
    {
        if owner != profile_owner {
            return Err("browser session belongs to another user".to_string());
        }
    }
    let tenant_id = store::sync_config(root)
        .map_err(|error| format!("browser command tenant configuration is unavailable: {error:#}"))?
        .instance_id;
    let access = BrowserSessionAccess {
        tenant_id: Some(tenant_id),
        owner_user_id: Some(profile_owner.to_string()),
        controller_user_id: Some(profile_owner.to_string()),
        controller_lease_id: Some(lease_id.to_string()),
        controller_lease_expires_at_ms: Some(now_ms() as u64 + 120_000),
    };
    let manager = browser_runtime_manager();
    let session = manager
        .ensure_session(
            root.to_path_buf(),
            browser_runtime_reference_dir(root),
            session_id,
            viewport_w,
            viewport_h,
            profile_owner,
            private_profile,
            true,
        )
        .await
        .map_err(|error| format!("browser direct runtime start failed: {error:#}"))?;
    let navigation = manager
        .request(
            &session,
            "navigate",
            json!({ "url": target_url, "timeoutMs": 30_000 }),
        )
        .await
        .map_err(|error| format!("browser direct navigation failed: {error:#}"))?;
    let nav = navigation.get("nav").cloned().unwrap_or(Value::Null);
    let final_url = nav
        .get("url")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&target_url);
    let title = nav
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Remote Browser");
    let can_go_back = nav
        .get("can_go_back")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let can_go_forward = nav
        .get("can_go_forward")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let now = now_ms() as u64;
    upsert_browser_tab(
        database,
        tab_id,
        session_id,
        title,
        final_url,
        "active",
        false,
        can_go_back,
        can_go_forward,
        None,
        0,
        Some(&access),
    )
    .await
    .map_err(|error| format!("browser direct tab projection failed: {error:#}"))?;
    upsert_browser_session(
        database,
        session_id,
        tab_id,
        "active",
        "active",
        final_url,
        title,
        viewport_w,
        viewport_h,
        None,
        0,
        "browser.session.start",
        now,
        navigation.get("error").and_then(Value::as_str),
        Some(&access),
        Some(request),
    )
    .await
    .map_err(|error| format!("browser direct session projection failed: {error:#}"))?;
    mark_browser_direct_live_session(session_id);
    Ok(json!({
        "ok": true,
        "operation": "session.start",
        "session_id": session_id,
        "tab_id": tab_id,
        "lease_id": lease_id,
        "lease_expires_at_ms": now + 120_000,
        "nav": {
            "url": final_url,
            "title": title,
            "can_go_back": can_go_back,
            "can_go_forward": can_go_forward
        }
    }))
}

pub(super) fn is_browser_runtime_command(command_type: &str) -> bool {
    matches!(
        command_type,
        "browser.session.start"
            | "browser.navigate"
            | "browser.reload"
            | "browser.back"
            | "browser.forward"
            | "browser.reset"
            | "browser.session.stop"
            | "browser.controller.acquire"
            | "browser.controller.renew"
            | "browser.controller.release"
            | "browser.credential.fill"
            | "browser.tab.open"
            | "browser.tab.activate"
            | "browser.tab.close"
            | "browser.dialog.respond"
            | "browser.permission.respond"
            | "browser.clipboard.copy"
            | "browser.clipboard.paste"
            | "browser.clipboard.clear"
            | "web_stack.auth_assist.complete"
    )
}

fn settle_auth_assist_queue_task(
    root: &Path,
    auth_assist_command_id: &str,
    auth_assist_task_id: &str,
) -> Value {
    let task = if !auth_assist_task_id.is_empty() {
        channels::load_queue_task(root, auth_assist_task_id)
            .ok()
            .flatten()
    } else if !auth_assist_command_id.is_empty() {
        channels::load_queue_task_for_business_os_command(root, auth_assist_command_id)
            .ok()
            .flatten()
    } else {
        None
    };
    let Some(task) = task else {
        return json!({ "status": "not_applicable" });
    };
    if matches!(
        task.route_status.as_str(),
        "handled" | "cancelled" | "failed" | "superseded"
    ) {
        return json!({
            "status": "already_terminal",
            "task_id": task.message_key,
            "route_status": task.route_status,
        });
    }
    match channels::set_queue_task_route_status(root, &task.message_key, "cancelled") {
        Ok(true) => json!({
            "status": "cancelled_after_user_confirmation",
            "task_id": task.message_key,
        }),
        Ok(false) => json!({ "status": "not_found", "task_id": task.message_key }),
        Err(error) => json!({
            "status": "settle_failed",
            "task_id": task.message_key,
            "error": error.to_string(),
        }),
    }
}

fn resume_auth_assist_requesting_task(
    root: &Path,
    requesting_task_id: &str,
    source_id: &str,
    session_id: &str,
) -> Value {
    if requesting_task_id.is_empty() {
        return json!({ "status": "not_requested" });
    }
    let Ok(Some(task)) = channels::load_queue_task(root, requesting_task_id) else {
        return json!({
            "status": "requesting_task_not_found",
            "requesting_task_id": requesting_task_id,
        });
    };
    if matches!(task.route_status.as_str(), "pending" | "leased" | "running") {
        return json!({
            "status": "already_active",
            "task_id": task.message_key,
            "thread_key": task.thread_key,
            "route_status": task.route_status,
        });
    }
    if matches!(
        task.route_status.as_str(),
        "blocked" | "review_rework" | "failed"
    ) {
        let released = channels::update_queue_task(
            root,
            channels::QueueTaskUpdateRequest {
                message_key: task.message_key.clone(),
                route_status: Some("pending".to_string()),
                status_note: Some(format!(
                    "Browser-Anmeldung fuer {source_id} bestaetigt; derselbe Recherchekontext wird fortgesetzt"
                )),
                ..Default::default()
            },
        );
        if let Ok(released) = released {
            return json!({
                "status": "resumed",
                "task_id": released.message_key,
                "thread_key": released.thread_key,
                "route_status": released.route_status,
            });
        }
    }

    // A terminal Business-OS command cannot legally be moved back to pending.
    // Continue in the same durable thread with the original prompt and an
    // explicit browser-auth checkpoint instead of mutating terminal history.
    let prompt = format!(
        "{}\n\nFortsetzung nach manueller Browser-Anmeldung: Die Anmeldung fuer Quelle `{}` wurde in der persistenten CTOX-Browser-Sitzung `{}` vom Benutzer bestaetigt. Setze jetzt die urspruengliche Recherche mit demselben Kontext fort. Verwende die bestehende Browser-Sitzung; frage keine Zugangswerte ab und schreibe keine Secrets in Prompt, Ergebnis, Log oder RxDB.",
        task.prompt.trim(),
        source_id,
        session_id,
    );
    match channels::create_queue_task_with_metadata(
        root,
        channels::QueueTaskCreateRequest {
            title: format!("Fortsetzen: {}", task.title),
            prompt,
            thread_key: task.thread_key.clone(),
            workspace_root: task.workspace_root.clone(),
            priority: task.priority.clone(),
            suggested_skill: task.suggested_skill.clone(),
            parent_message_key: Some(task.message_key.clone()),
            extra_metadata: Some(json!({
                "source": "web-stack-auth-assist",
                "auth_assist_resume": true,
                "requesting_task_id": task.message_key,
                "source_id": source_id,
                "browser_session_id": session_id,
                "secret_value_in_payload": false,
                // Carry the originating Business OS command forward: owner
                // resolution for further auth sessions and the app writeback
                // both follow `business_os_command_id`. Without it the
                // continuation would run without the human's identity.
                "business_os_command_id": task
                    .metadata
                    .get("business_os_command_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                "business_os_module": task
                    .metadata
                    .get("business_os_module")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                "business_os_record_id": task
                    .metadata
                    .get("business_os_record_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            })),
        },
    ) {
        Ok(created) => json!({
            "status": "continued_in_same_thread",
            "task_id": created.message_key,
            "thread_key": created.thread_key,
            "parent_task_id": task.message_key,
            "route_status": created.route_status,
        }),
        Err(error) => json!({
            "status": "resume_failed",
            "requesting_task_id": task.message_key,
            "error": error.to_string(),
        }),
    }
}

pub(super) async fn apply_browser_runtime_command(
    root: &Path,
    database: &Arc<RxDatabase>,
    document: &Value,
    accepted: &Value,
) -> anyhow::Result<()> {
    let _browser_runtime_guard = BROWSER_RUNTIME_COMMAND_LOCK.lock().await;
    let command_type = document
        .get("command_type")
        .or_else(|| document.get("type"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let payload = document.get("payload").cloned().unwrap_or(Value::Null);
    let session_id = payload
        .get("session_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("browser_session_default")
        .to_string();
    let tab_id = payload
        .get("tab_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("browser_tab_default")
        .to_string();
    let command_id = document
        .get("command_id")
        .or_else(|| document.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    eprintln!(
        "{}",
        browser_runtime_command_log_line("received", command_type, command_id, &session_id, None,)
    );
    let viewport_w = payload
        .get("viewport_w")
        .and_then(Value::as_u64)
        .unwrap_or(1280)
        .clamp(320, 3840);
    let viewport_h = payload
        .get("viewport_h")
        .and_then(Value::as_u64)
        .unwrap_or(720)
        .clamp(240, 2160);
    let command_created_at_ms = document
        .get("created_at_ms")
        .or_else(|| document.get("updated_at_ms"))
        .and_then(Value::as_u64)
        .unwrap_or_else(|| now_ms() as u64);
    let capability_token = document
        .get("client_context")
        .and_then(|context| context.get("capability_token"))
        .and_then(Value::as_str)
        .context("browser command capability token is required")?;
    let (profile_owner, _) = store::verify_webrtc_capability_actor(root, capability_token)
        .context("browser command capability token is invalid")?;
    let tenant_id = store::sync_config(root)
        .context("browser command tenant configuration is unavailable")?
        .instance_id;
    anyhow::ensure!(
        !tenant_id.trim().is_empty(),
        "browser command tenant id is empty"
    );
    let private_profile = payload
        .get("profile_mode")
        .and_then(Value::as_str)
        .is_some_and(|mode| mode == "private");
    let requested_lease_id = payload
        .get("lease_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let existing_session = find_browser_document(database, "browser_sessions", &session_id).await?;
    let existing_owner = existing_session
        .get("owner_user_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "ctox");
    if let Some(existing_owner) = existing_owner {
        anyhow::ensure!(
            existing_owner == profile_owner,
            "browser session belongs to another user"
        );
    }
    let mut access = BrowserSessionAccess {
        tenant_id: Some(tenant_id),
        owner_user_id: Some(profile_owner.clone()),
        controller_user_id: requested_lease_id.as_ref().map(|_| profile_owner.clone()),
        controller_lease_id: requested_lease_id.clone(),
        controller_lease_expires_at_ms: requested_lease_id
            .as_ref()
            .map(|_| now_ms() as u64 + 120_000),
    };
    let existing_tab = find_browser_document(database, "browser_tabs", &tab_id).await?;
    let previous_url = existing_tab
        .get("url")
        .or_else(|| existing_session.get("current_url"))
        .and_then(Value::as_str)
        .unwrap_or("https://example.com")
        .to_string();
    let target_url = payload
        .get("url")
        .and_then(Value::as_str)
        .map(normalize_browser_runtime_url)
        .filter(|value| !value.is_empty())
        .unwrap_or(previous_url);

    if matches!(
        command_type,
        "browser.controller.acquire" | "browser.controller.renew" | "browser.controller.release"
    ) {
        anyhow::ensure!(existing_session.is_object(), "browser session not found");
        let lease_id = requested_lease_id
            .as_deref()
            .context("browser controller lease id is required")?;
        let current_controller = existing_session
            .get("controller_user_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let current_lease = existing_session
            .get("controller_lease_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if command_type != "browser.controller.acquire" {
            anyhow::ensure!(
                current_controller == profile_owner && current_lease == lease_id,
                "browser controller lease does not match"
            );
        }
        if command_type == "browser.controller.release" {
            access.controller_user_id = Some(String::new());
            access.controller_lease_id = Some(String::new());
            access.controller_lease_expires_at_ms = Some(0);
        } else {
            access.controller_user_id = Some(profile_owner.clone());
            access.controller_lease_id = Some(lease_id.to_string());
            access.controller_lease_expires_at_ms = Some(now_ms() as u64 + 120_000);
        }
        let status = existing_session
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("active");
        let runtime_status = existing_session
            .get("runtime_status")
            .and_then(Value::as_str)
            .unwrap_or(status);
        let title = existing_session
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("Remote Browser");
        let current_tab_id = existing_session
            .get("current_tab_id")
            .and_then(Value::as_str)
            .unwrap_or(&tab_id);
        let frame_id = existing_session
            .get("active_frame_id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty());
        let frame_seq = existing_session
            .get("last_frame_seq")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        upsert_browser_session(
            database,
            &session_id,
            current_tab_id,
            status,
            runtime_status,
            &target_url,
            title,
            viewport_w,
            viewport_h,
            frame_id,
            frame_seq,
            command_type,
            command_created_at_ms,
            None,
            Some(&access),
            None,
        )
        .await?;
        mark_browser_runtime_command_completed(
            database,
            document,
            accepted,
            json!({
                "ok": true,
                "session_id": session_id,
                "controller_user_id": access.controller_user_id,
                "controller_lease_id": access.controller_lease_id,
                "controller_lease_expires_at_ms": access.controller_lease_expires_at_ms
            }),
        )
        .await?;
        return Ok(());
    }

    if existing_session.is_object() && command_type != "browser.session.start" {
        let current_controller = existing_session
            .get("controller_user_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let current_lease = existing_session
            .get("controller_lease_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let current_lease_expires_at_ms = existing_session
            .get("controller_lease_expires_at_ms")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        anyhow::ensure!(
            current_controller == profile_owner
                && requested_lease_id.as_deref() == Some(current_lease)
                && current_lease_expires_at_ms > now_ms() as u64,
            "browser controller lease is missing or expired"
        );
    }

    if command_type == "web_stack.auth_assist.complete" {
        anyhow::ensure!(existing_session.is_object(), "browser session not found");
        anyhow::ensure!(
            payload.get("confirmed").and_then(Value::as_bool) == Some(true),
            "auth assist completion requires explicit confirmation"
        );
        let session_payload = existing_session
            .get("payload")
            .filter(|value| value.is_object())
            .cloned()
            .unwrap_or_else(|| json!({}));
        anyhow::ensure!(
            session_payload.get("purpose").and_then(Value::as_str) == Some("web_stack_auth"),
            "browser session is not a web-stack authentication session"
        );
        let source_id = payload
            .get("source_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| session_payload.get("source_id").and_then(Value::as_str))
            .unwrap_or_default();
        let auth_assist_command_id = payload
            .get("auth_assist_command_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                session_payload
                    .get("auth_assist_command_id")
                    .and_then(Value::as_str)
            })
            .unwrap_or_default();
        let auth_assist_task_id = payload
            .get("auth_assist_task_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                session_payload
                    .get("auth_assist_task_id")
                    .and_then(Value::as_str)
            })
            .unwrap_or_default();
        let requesting_task_id = payload
            .get("requesting_task_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                session_payload
                    .get("requesting_task_id")
                    .and_then(Value::as_str)
            })
            .unwrap_or_default();
        let current_tab_id = existing_session
            .get("current_tab_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&tab_id);
        let current_url = existing_tab
            .get("url")
            .or_else(|| existing_session.get("current_url"))
            .and_then(Value::as_str)
            .unwrap_or(&target_url);
        let title = existing_tab
            .get("title")
            .or_else(|| existing_session.get("title"))
            .and_then(Value::as_str)
            .unwrap_or("Remote Browser");
        let completed_at_ms = now_ms() as u64;
        let metadata_patch = json!({
            "source_id": source_id,
            "purpose": "web_stack_auth",
            "auth_assist_command_id": auth_assist_command_id,
            "auth_assist_task_id": auth_assist_task_id,
            "requesting_task_id": requesting_task_id,
            "auth_assist_status": "completed",
            "authenticated": true,
            "authenticated_at_ms": completed_at_ms,
            "continuation_status": "resume_requested",
            "secret_value_in_rxdb": false,
        });
        upsert_browser_session(
            database,
            &session_id,
            current_tab_id,
            existing_session
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("active"),
            existing_session
                .get("runtime_status")
                .and_then(Value::as_str)
                .unwrap_or("active"),
            current_url,
            title,
            viewport_w,
            viewport_h,
            existing_session
                .get("active_frame_id")
                .and_then(Value::as_str),
            existing_session
                .get("last_frame_seq")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            command_type,
            command_created_at_ms,
            None,
            Some(&access),
            Some(&metadata_patch),
        )
        .await?;

        let auth_request =
            settle_auth_assist_queue_task(root, auth_assist_command_id, auth_assist_task_id);
        let continuation =
            resume_auth_assist_requesting_task(root, requesting_task_id, source_id, &session_id);
        mark_browser_runtime_command_completed(
            database,
            document,
            accepted,
            json!({
                "ok": true,
                "session_id": session_id,
                "tab_id": current_tab_id,
                "source_id": source_id,
                "authenticated": true,
                "auth_assist_status": "completed",
                "verification": "explicit_user_confirmation",
                "auth_request": auth_request,
                "continuation": continuation,
                "secret_value_in_rxdb": false,
            }),
        )
        .await?;
        return Ok(());
    }

    let credential_fill = if command_type == "browser.credential.fill" {
        anyhow::ensure!(existing_session.is_object(), "browser session not found");
        anyhow::ensure!(
            payload.get("confirmed").and_then(Value::as_bool) == Some(true),
            "credential fill requires explicit confirmation"
        );
        anyhow::ensure!(
            store::webrtc_capability_allows_workspace_permission(
                root,
                capability_token,
                BusinessOsPermission::SecretsManage,
            ),
            "credential fill requires secrets.manage permission"
        );
        let secret_scope = payload
            .get("secret_scope")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        anyhow::ensure!(
            secret_scope == crate::secrets::credential_scope(),
            "credential fill accepts only the credentials scope"
        );
        let secret_name = payload
            .get("secret_name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| is_safe_browser_credential_name(value))
            .context("credential fill secret name is invalid")?;
        let field_role = payload
            .get("field_role")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("password");
        anyhow::ensure!(
            matches!(field_role, "username" | "password" | "both"),
            "credential fill field role must be username, password, or both"
        );
        let selector = payload
            .get("selector")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string();
        let stored_secret = crate::secrets::read_secret_value(root, secret_scope, secret_name)
            .context("configured browser credential is unavailable")?;
        let username = matches!(field_role, "username" | "both")
            .then(|| resolve_browser_credential_field(&stored_secret, "username"))
            .transpose()?;
        let password = matches!(field_role, "password" | "both")
            .then(|| resolve_browser_credential_field(&stored_secret, "password"))
            .transpose()?;
        Some(ResolvedBrowserCredentialFill {
            selector,
            field_role: field_role.to_string(),
            username,
            password,
        })
    } else {
        None
    };

    if command_type == "browser.session.stop" {
        let title = existing_tab
            .get("title")
            .or_else(|| existing_session.get("title"))
            .and_then(Value::as_str)
            .unwrap_or("Remote Browser");
        browser_runtime_manager().stop(&session_id).await;
        upsert_browser_session(
            database,
            &session_id,
            &tab_id,
            "stopped",
            "stopped",
            &target_url,
            title,
            viewport_w,
            viewport_h,
            None,
            0,
            command_type,
            command_created_at_ms,
            None,
            Some(&access),
            None,
        )
        .await?;
        upsert_browser_tab(
            database,
            &tab_id,
            &session_id,
            title,
            &target_url,
            "stopped",
            false,
            false,
            false,
            None,
            0,
            Some(&access),
        )
        .await?;
        mark_browser_runtime_command_completed(
            database,
            document,
            accepted,
            json!({
                "ok": true,
                "browser_stream": "rxdb",
                "session_id": session_id,
                "tab_id": tab_id,
                "status": "stopped"
            }),
        )
        .await?;
        return Ok(());
    }

    // All remaining commands drive a
    // live, persistent Chromium runtime via the session registry.
    let manager = browser_runtime_manager();
    if command_type == "browser.reset" {
        manager.stop(&session_id).await;
    }

    let browser_runtime_dir = browser_runtime_reference_dir(root);
    eprintln!(
        "{}",
        browser_runtime_command_log_line(
            "process_start",
            command_type,
            command_id,
            &session_id,
            None,
        )
    );
    let session = match manager
        .ensure_session(
            root.to_path_buf(),
            browser_runtime_dir,
            &session_id,
            viewport_w,
            viewport_h,
            &profile_owner,
            private_profile,
            matches!(command_type, "browser.session.start" | "browser.reset"),
        )
        .await
    {
        Ok(session) => session,
        Err(err) => {
            let detail = format!("{err:#}");
            eprintln!(
                "{}",
                browser_runtime_command_log_line(
                    "failed",
                    command_type,
                    command_id,
                    &session_id,
                    Some(&detail),
                )
            );
            mark_browser_session_runtime_error(
                database,
                &session_id,
                &tab_id,
                &target_url,
                viewport_w,
                viewport_h,
                command_type,
                command_created_at_ms,
                &detail,
                Some(&access),
            )
            .await?;
            return Err(err);
        }
    };

    // Translate the lifecycle command into a runtime operation.
    let (op, op_params): (&str, Value) = match command_type {
        "browser.navigate" | "browser.session.start" | "browser.reset" => {
            ("navigate", json!({ "url": target_url, "timeoutMs": 30000 }))
        }
        "browser.reload" => ("reload", json!({ "timeoutMs": 30000 })),
        "browser.back" => ("back", json!({ "timeoutMs": 30000 })),
        "browser.forward" => ("forward", json!({ "timeoutMs": 30000 })),
        // The runner has carried tab_open/tab_activate/tab_close all along;
        // only the bridge was missing, so the "new tab" button in the UI wrote
        // a command nothing consumed. `tab_id` comes from the command payload,
        // which is what lets the caller address a specific tab.
        "browser.tab.open" => (
            "tab_open",
            json!({ "tabId": tab_id, "url": target_url, "timeoutMs": 30000 }),
        ),
        "browser.tab.activate" => ("tab_activate", json!({ "tabId": tab_id })),
        "browser.tab.close" => ("tab_close", json!({ "tabId": tab_id })),
        // Ein offener Seitendialog blockiert die ferne Seite vollstaendig --
        // ohne diese Bruecke war der Antwortknopf wirkungslos und die Sitzung
        // stand, bis jemand sie neu startete.
        "browser.dialog.respond" => (
            "dialog_respond",
            json!({
                "accept": payload.get("accept").and_then(Value::as_bool).unwrap_or(false),
                "value": payload.get("value").cloned().unwrap_or(Value::Null),
            }),
        ),
        "browser.permission.respond" => (
            "permission_respond",
            json!({
                "accept": payload.get("accept").and_then(Value::as_bool).unwrap_or(false),
            }),
        ),
        // Zwischenablage. Der kopierte Text bleibt im Arbeitsspeicher der
        // Sitzung (`LiveBrowserSession::clipboard`, mit Verfall) und wird
        // bewusst NICHT in RxDB projiziert: Was jemand im fernen Browser
        // kopiert, kann eine Zugangskennung sein, und replizierte Dokumente
        // liegen auf jedem Peer. Der Speicher war fertig gebaut, nur nie
        // angeschlossen -- die drei Knoepfe schrieben Befehle, die niemand las.
        "browser.clipboard.copy" => ("clipboard_copy", json!({})),
        "browser.clipboard.paste" => (
            "clipboard_paste",
            json!({ "value": session.clipboard().unwrap_or_default() }),
        ),
        "browser.clipboard.clear" => {
            session.clear_clipboard();
            // Kein Runner-Aufruf noetig; nav_state haelt die Antwortform gleich.
            ("nav_state", json!({}))
        }
        "browser.credential.fill" => {
            let fill = credential_fill
                .as_ref()
                .context("credential fill was not resolved")?;
            (
                "credential_fill",
                json!({
                    "selector": fill.selector,
                    "fieldRole": fill.field_role,
                    "usernameValue": fill.username,
                    "passwordValue": fill.password,
                }),
            )
        }
        _ => ("nav_state", json!({})),
    };

    let op_result = match manager.request(&session, op, op_params).await {
        Ok(value) => value,
        Err(err) => {
            // The runtime process is unusable; drop it so the next command
            // respawns, and persist the exact failure on the session.
            manager.drop_session_after_crash(&session_id);
            let detail = format!("{err:#}");
            eprintln!(
                "{}",
                browser_runtime_command_log_line(
                    "failed",
                    command_type,
                    command_id,
                    &session_id,
                    Some(&detail),
                )
            );
            mark_browser_session_runtime_error(
                database,
                &session_id,
                &tab_id,
                &target_url,
                viewport_w,
                viewport_h,
                command_type,
                command_created_at_ms,
                &detail,
                Some(&access),
            )
            .await?;
            return Err(err);
        }
    };

    // Was der ferne Browser als Auswahl meldet, wird hier zur Zwischenablage
    // dieser Sitzung -- nur im Arbeitsspeicher, damit ein kopiertes Passwort
    // nicht ueber die Replikation auf jeden Peer wandert.
    if command_type == "browser.clipboard.copy" {
        if let Some(text) = op_result.get("clipboardText").and_then(Value::as_str) {
            session.set_clipboard(text.to_string());
        }
    }

    // A failed op (e.g. invalid URL) is reported as navigation_error but does
    // not tear the session down; we still capture whatever the page shows.
    let navigation_error = if op_result.get("ok").and_then(Value::as_bool) == Some(false) {
        op_result
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_string)
    } else {
        None
    };
    let nav = op_result.get("nav").cloned().unwrap_or(Value::Null);
    let final_url = nav
        .get("url")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&target_url)
        .to_string();
    let title = nav
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Remote Browser")
        .to_string();
    let can_go_back = nav
        .get("can_go_back")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let can_go_forward = nav
        .get("can_go_forward")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if has_newer_browser_runtime_command(database, &session_id, command_created_at_ms).await? {
        mark_browser_runtime_command_completed(
            database,
            document,
            accepted,
            json!({
                "ok": true,
                "browser_stream": "rxdb",
                "session_id": session_id,
                "tab_id": tab_id,
                "url": final_url,
                "title": title,
                "superseded_by_newer_command": true
            }),
        )
        .await?;
        return Ok(());
    }

    if command_type == "browser.session.start" {
        // Session/tab metadata is durable; the initial JPEG is not. Persisting
        // it here revived the same SQLite -> WebRTC -> IndexedDB transport the
        // direct live channel replaces and delayed the first interactive
        // frame. The client asks ctox.browser.live.v1 for the JPEG immediately
        // after this projection appears.
        upsert_browser_tab(
            database,
            &tab_id,
            &session_id,
            &title,
            &final_url,
            "active",
            false,
            can_go_back,
            can_go_forward,
            None,
            0,
            Some(&access),
        )
        .await?;
        upsert_browser_session(
            database,
            &session_id,
            &tab_id,
            "active",
            "active",
            &final_url,
            &title,
            viewport_w,
            viewport_h,
            None,
            0,
            command_type,
            command_created_at_ms,
            navigation_error.as_deref(),
            Some(&access),
            Some(&payload),
        )
        .await?;
        reserve_browser_direct_live_session(&session_id);
        mark_browser_runtime_command_completed(
            database,
            document,
            accepted,
            json!({
                "ok": true,
                "browser_stream": "webrtc-direct",
                "session_id": session_id,
                "tab_id": tab_id,
                "url": final_url,
                "title": title,
                "can_go_back": can_go_back,
                "can_go_forward": can_go_forward,
                "navigation_error": navigation_error,
                "secret_value_in_rxdb": false
            }),
        )
        .await?;
        return Ok(());
    }

    wait_for_browser_frame_capture_slot(database, &session_id).await?;
    let screenshot = match manager
        .request(
            &session,
            "screenshot",
            browser_frame_capture_request(viewport_w, viewport_h),
        )
        .await
    {
        Ok(value) => value,
        Err(err) => {
            manager.drop_session_after_crash(&session_id);
            let detail = format!("{err:#}");
            mark_browser_session_runtime_error(
                database,
                &session_id,
                &tab_id,
                &final_url,
                viewport_w,
                viewport_h,
                command_type,
                command_created_at_ms,
                &detail,
                Some(&access),
            )
            .await?;
            return Err(err);
        }
    };
    let data = screenshot
        .get("screenshot")
        .and_then(|frame| frame.get("base64"))
        .and_then(Value::as_str)
        .context("browser runtime did not return screenshot data")?
        .to_string();
    let mime_type = screenshot
        .get("screenshot")
        .and_then(|frame| frame.get("mimeType"))
        .and_then(Value::as_str)
        .unwrap_or("image/jpeg")
        .to_string();
    let mut next_seq = existing_session
        .get("last_frame_seq")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        + 1;
    let mut frame_id = format!("browser_frame_{}_{}", session_id, next_seq);
    let frame_hash = browser_frame_hash(&data);
    let size_bytes = base64::engine::general_purpose::STANDARD
        .decode(data.as_bytes())
        .map(|bytes| bytes.len() as u64)
        .unwrap_or_else(|_| data.len() as u64);
    let matching_frame =
        matching_active_browser_frame(database, &existing_session, Some(&frame_hash)).await?;
    if let Some(frame) = matching_frame.as_ref() {
        next_seq = frame.get("seq").and_then(Value::as_u64).unwrap_or(next_seq);
        frame_id = frame
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(&frame_id)
            .to_string();
    }
    if matching_frame.is_none() {
        let (frame_width, frame_height) = browser_frame_capture_dimensions(viewport_w, viewport_h);
        upsert_browser_frame(
            database,
            &frame_id,
            &session_id,
            &tab_id,
            next_seq,
            &mime_type,
            &data,
            frame_width,
            frame_height,
            size_bytes,
            &frame_hash,
            Some(&access),
        )
        .await?;
    }
    upsert_browser_tab(
        database,
        &tab_id,
        &session_id,
        &title,
        &final_url,
        // A closed tab must not stay projected as "active", or it keeps
        // showing up in the tab strip after the runner already dropped it.
        if command_type == "browser.tab.close" {
            "closed"
        } else {
            "active"
        },
        false,
        can_go_back,
        can_go_forward,
        Some(&frame_id),
        next_seq,
        Some(&access),
    )
    .await?;
    let session_metadata_patch = if command_type == "browser.session.start" {
        Some(payload.clone())
    } else {
        credential_fill.as_ref().map(|fill| {
            json!({
                "credential_fill_status": "completed",
                "credential_field_role": fill.field_role,
                "secret_value_in_rxdb": false
            })
        })
    };
    upsert_browser_session(
        database,
        &session_id,
        &tab_id,
        "active",
        "active",
        &final_url,
        &title,
        viewport_w,
        viewport_h,
        Some(&frame_id),
        next_seq,
        command_type,
        command_created_at_ms,
        navigation_error.as_deref(),
        Some(&access),
        session_metadata_patch.as_ref(),
    )
    .await?;
    if command_type == "browser.session.start" {
        reserve_browser_direct_live_session(&session_id);
    }
    mark_browser_runtime_command_completed(
        database,
        document,
        accepted,
        json!({
            "ok": true,
            "browser_stream": "rxdb",
            "session_id": session_id,
            "tab_id": tab_id,
            "frame_id": frame_id,
            "url": final_url,
            "title": title,
            "frame_hash": frame_hash,
            "size_bytes": size_bytes,
            "frame_changed": matching_frame.is_none(),
            "can_go_back": can_go_back,
            "can_go_forward": can_go_forward,
            "navigation_error": navigation_error,
            "credential_fill_status": credential_fill.as_ref().map(|_| "completed"),
            "secret_value_in_rxdb": false
        }),
    )
    .await?;
    eprintln!(
        "[business-os] browser runtime command phase=completed command_type={} command_id={} session_id={} url={} can_go_back={} can_go_forward={}",
        command_type, command_id, session_id, final_url, can_go_back, can_go_forward,
    );
    Ok(())
}

fn is_safe_browser_credential_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

struct ResolvedBrowserCredentialFill {
    selector: String,
    field_role: String,
    username: Option<String>,
    password: Option<String>,
}

fn resolve_browser_credential_field(
    secret_value: &str,
    field_role: &str,
) -> anyhow::Result<String> {
    let parsed = serde_json::from_str::<Value>(secret_value).ok();
    let object = parsed.as_ref().and_then(Value::as_object);
    let keys: &[&str] = match field_role {
        "username" => &["username", "email", "login", "login_hint"],
        "password" => &["password", "credential", "secret", "value"],
        _ => anyhow::bail!("unsupported credential field role"),
    };
    let bundled = object.and_then(|value| {
        keys.iter()
            .find_map(|key| value.get(*key).and_then(Value::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    });
    if let Some(value) = bundled {
        return Ok(value);
    }
    anyhow::ensure!(
        field_role == "password" && object.is_none() && !secret_value.is_empty(),
        "configured credential does not contain the requested field"
    );
    Ok(secret_value.to_string())
}

fn browser_runtime_reference_dir(root: &Path) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("CTOX_WEB_BROWSER_REFERENCE_DIR").map(PathBuf::from) {
        return Some(path);
    }
    let root_candidate = root.join("runtime/browser/interactive-reference");
    if root_candidate.join("package.json").exists() || root_candidate.join("node_modules").is_dir()
    {
        return Some(root_candidate);
    }
    let home_candidate = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".local/state/ctox/browser/interactive-reference"));
    if let Some(path) = home_candidate {
        if path.join("package.json").exists() || path.join("node_modules").is_dir() {
            return Some(path);
        }
    }
    None
}

fn browser_runtime_failure_status(detail: &str) -> &'static str {
    if detail.starts_with("browser crash-loop protection paused")
        || detail.starts_with("browser session limit")
    {
        "blocked"
    } else {
        "error"
    }
}

/// Record the unmodified runtime failure on the session/tab so the user sees
/// the same cause that the native command path logged.
#[allow(clippy::too_many_arguments)]
async fn mark_browser_session_runtime_error(
    database: &Arc<RxDatabase>,
    session_id: &str,
    tab_id: &str,
    url: &str,
    viewport_w: u64,
    viewport_h: u64,
    command_type: &str,
    command_created_at_ms: u64,
    detail: &str,
    access: Option<&BrowserSessionAccess>,
) -> anyhow::Result<()> {
    let existing_session = find_browser_document(database, "browser_sessions", session_id).await?;
    let existing_tab = find_browser_document(database, "browser_tabs", tab_id).await?;
    let title = existing_tab
        .get("title")
        .or_else(|| existing_session.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("Remote Browser")
        .to_string();
    let frame_id = existing_session
        .get("active_frame_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let frame_seq = existing_session
        .get("last_frame_seq")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let failure_status = browser_runtime_failure_status(detail);
    eprintln!(
        "[business-os] browser session runtime failed session_id={session_id} \
         command_type={command_type} status={failure_status}: {detail}"
    );
    upsert_browser_tab(
        database,
        tab_id,
        session_id,
        &title,
        url,
        failure_status,
        false,
        false,
        false,
        frame_id.as_deref(),
        frame_seq,
        access,
    )
    .await?;
    upsert_browser_session(
        database,
        session_id,
        tab_id,
        failure_status,
        failure_status,
        url,
        &title,
        viewport_w,
        viewport_h,
        frame_id.as_deref(),
        frame_seq,
        command_type,
        command_created_at_ms,
        Some(detail),
        access,
        None,
    )
    .await?;

    // `last_error` is the operator-facing contract used by deployed browser
    // stores. Keep it byte-for-byte aligned with `format!("{err:#}")`; the
    // generic command-failure path must not replace it with a reassurance or a
    // shortened top-level context.
    let sessions = database
        .collection("browser_sessions")
        .context("browser_sessions collection is not registered")?;
    let mut marked = find_browser_document(database, "browser_sessions", session_id).await?;
    if let Some(object) = marked.as_object_mut() {
        object.remove("_rev");
        object.remove("_meta");
        object.insert("last_error".to_string(), Value::String(detail.to_string()));
        if let Some(payload) = object.get_mut("payload").and_then(Value::as_object_mut) {
            payload.insert("last_error".to_string(), Value::String(detail.to_string()));
        }
    }
    sessions
        .incremental_upsert(marked)
        .await
        .map_err(|err| anyhow::anyhow!("persist browser session last_error: {err}"))?;
    Ok(())
}

/// Background loop that keeps live browser sessions responsive: it replays
/// pending input events against the real page, refreshes frames after input,
/// and garbage-collects expired frames. Runs under the shared write lock so it
/// never races the command consumer's RxDB writes.
pub(super) async fn browser_runtime_maintenance_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    let mut consecutive_idle_rounds = 0u32;
    // This loop is the daemon's only maintenance tick that runs whether or not a
    // browser session exists, which is why the tombstone sweep rides it. The
    // sweep must sit ABOVE the early return below: tombstones accumulate in every
    // collection, most of all while nobody is browsing.
    let mut last_tombstone_sweep: Option<Instant> = None;
    loop {
        let sweep_due = last_tombstone_sweep.is_none_or(|last| {
            last.elapsed() >= Duration::from_secs(TOMBSTONE_SWEEP_IDLE_INTERVAL_SECS)
        });
        if sweep_due {
            let more_remains = sweep_tombstones_once(&database, &database_write_lock).await;
            // A full batch means backlog. Come back promptly instead of sleeping
            // half an hour on a database with six figures of tombstones.
            last_tombstone_sweep = Some(if more_remains {
                Instant::now()
                    - Duration::from_secs(
                        TOMBSTONE_SWEEP_IDLE_INTERVAL_SECS - TOMBSTONE_SWEEP_DRAIN_INTERVAL_SECS,
                    )
            } else {
                Instant::now()
            });
        }
        // Die Aufraeumer muessen wie der Tombstone-Sweep UEBER dem Ausstieg
        // stehen. Sie hingen darunter und wurden damit genau dann uebersprungen,
        // wenn aufgeraeumt werden koennte: ohne laufende Sitzung. Auf der
        // Produktionsinstanz am 19.08.2026 gemessen — 452 abgelaufene Bilder, 241
        // verbrauchte Eingaben und 5525 erledigte Befehle lagen unberuehrt in
        // einem 930 MB grossen Speicher, waehrend beide Aufraeumer existierten
        // und keiner je lief.
        {
            let _guard = database_write_lock.lock().await;
            if let Err(err) = run_browser_runtime_maintenance(&database).await {
                eprintln!("[business-os] browser storage gc failed: {err:#}");
            }
        }
        if browser_runtime_manager().active_session_ids().is_empty() {
            consecutive_idle_rounds = 0;
            tokio::time::sleep(Duration::from_secs(
                BROWSER_RUNTIME_IDLE_MAINTENANCE_INTERVAL_SECS,
            ))
            .await;
            continue;
        }
        let did_work = {
            let _guard = database_write_lock.lock().await;
            let _browser_guard = BROWSER_RUNTIME_COMMAND_LOCK.lock().await;
            let started = Instant::now();
            let result = run_browser_runtime_maintenance(&database).await;
            record_native_peer_loop_result(
                &BROWSER_RUNTIME_LOOP_METRICS,
                &result,
                started.elapsed(),
            );
            match result {
                Ok(rows) => rows > 0,
                Err(err) => {
                    eprintln!("[business-os] browser runtime maintenance failed: {err:#}");
                    true
                }
            }
        };
        if did_work {
            consecutive_idle_rounds = 0;
        } else {
            consecutive_idle_rounds = consecutive_idle_rounds.saturating_add(1);
        }
        wait_for_browser_runtime_maintenance_wake(&root, consecutive_idle_rounds).await;
    }
}

fn browser_runtime_maintenance_sleep(consecutive_idle_rounds: u32) -> Duration {
    if consecutive_idle_rounds >= BROWSER_RUNTIME_IDLE_BACKOFF_AFTER_TICKS {
        Duration::from_secs(BROWSER_RUNTIME_IDLE_MAINTENANCE_INTERVAL_SECS)
    } else {
        Duration::from_millis(BROWSER_RUNTIME_ACTIVE_MAINTENANCE_INTERVAL_MS)
    }
}

async fn wait_for_browser_runtime_maintenance_wake(root: &Path, consecutive_idle_rounds: u32) {
    let sleep_for = browser_runtime_maintenance_sleep(consecutive_idle_rounds);
    if consecutive_idle_rounds < BROWSER_RUNTIME_IDLE_BACKOFF_AFTER_TICKS {
        tokio::time::sleep(sleep_for).await;
        return;
    }
    let table_name = rxdb_collection_version_table_name("browser_input_events", 0);
    let database_path = store::rxdb_store_path(root);
    let seen_generation = rxdb::storage::sqlite::instance::table_change_generation_for_path(
        &database_path,
        &table_name,
    )
    .unwrap_or(0);
    rxdb::storage::sqlite::instance::wait_for_table_change_for_path(
        &database_path,
        &table_name,
        seen_generation,
        sleep_for,
    )
    .await;
}

async fn run_browser_runtime_maintenance(database: &Arc<RxDatabase>) -> anyhow::Result<usize> {
    let mut rows_touched = 0usize;
    // Interactive video and input use ctox.browser.live.v1 directly on the
    // authenticated DataChannel. Never revive the former persisted transport
    // merely because a client is slow to attach: with three abandoned active
    // sessions that loop wrote hundreds of JPEG rows in under a minute and
    // held SQLite/IndexedDB busy enough to stall the whole Business OS.
    // Retain only bounded garbage collection for documents made by an older
    // client; current clients reconnect the direct channel instead.
    rows_touched = rows_touched.saturating_add(gc_expired_browser_frames(database).await?);
    rows_touched = rows_touched.saturating_add(gc_consumed_browser_input_events(database).await?);
    rows_touched = rows_touched.saturating_add(gc_settled_business_commands(database).await?);
    Ok(rows_touched)
}

/// Decide whether a live session owes the browser a fresh frame.
///
/// A session that has never produced a frame always does, even without a live
/// controller lease: the UI can only show a picture once a first frame exists,
/// and a user can only take the lease by acting on that picture. Gating the
/// first frame on the lease is the deadlock that kept `browser_frames` empty.
/// Afterwards an *expired* lease stops the stream instead of screenshotting an
/// abandoned session forever. A session carrying no lease at all is a different
/// case: several lifecycle writers never record one, and treating "absent" as
/// "expired" would cut the stream off after its very first frame. Those stream
/// as long as the runtime still holds the page.
/// Zeitpunkt der letzten Eingabe je Sitzung, rein lokal im Peer.
///
/// Bewusst kein Feld im Sitzungsdokument: das haette die Wire-Contracts
/// beruehrt, und der Wert ist fluechtig — er steuert nur die Bildrate.
static BROWSER_LAST_INPUT_MS: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();

pub(super) fn note_browser_input_activity(session_id: &str) {
    let map = BROWSER_LAST_INPUT_MS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut map) = map.lock() {
        map.insert(session_id.to_string(), now_ms() as u64);
    }
}

fn browser_last_input_ms(session_id: &str) -> Option<u64> {
    let map = BROWSER_LAST_INPUT_MS.get_or_init(|| Mutex::new(HashMap::new()));
    map.lock().ok().and_then(|map| map.get(session_id).copied())
}

/// Volle Rate waehrend und kurz nach einer Eingabe, sonst Ruhetakt.
///
/// Die bisherige Entscheidung kannte nur die Uhr: sie nahm alle 66 ms ein Bild
/// auf, unabhaengig davon, ob sich etwas geaendert hatte oder ob ueberhaupt
/// jemand zusah. Ein Werbebanner erzeugte damit dieselbe Last wie echte
/// Bedienung.
/// Wird diese Sitzung ueberhaupt angesehen?
///
/// Die Oberflaeche zeigt immer nur EINEN Bildschirm. Bilder von Sitzungen, die
/// niemand betrachtet, sind reine Verschwendung: auf der Produktionsinstanz standen
/// 111 Sitzungen, 110 davon tot, und die Aufnahmeschleife bediente sie weiter.
///
/// Als betrachtet gilt eine Sitzung, wenn ein Direktbetrachter angehaengt ist,
/// oder wenn sie kuerzlich bedient oder bewegt wurde (Navigation aktualisiert
/// das Sitzungsdokument). Das erste Bild einer neuen Sitzung entsteht ohnehin
/// vor dieser Pruefung, sonst saehe der Nutzer nie etwas.
fn browser_session_is_watched(
    session_id: &str,
    now_ms: u64,
    session_updated_at_ms: Option<u64>,
) -> bool {
    if browser_direct_live_session_active(session_id) {
        return true;
    }
    let letzte_regung = [browser_last_input_ms(session_id), session_updated_at_ms]
        .into_iter()
        .flatten()
        .max();
    letzte_regung
        .is_some_and(|zeit| now_ms.saturating_sub(zeit) <= BROWSER_SESSION_ABANDONED_AFTER_MS)
}

fn browser_effective_frame_rate(
    frame_rate_target: u64,
    now_ms: u64,
    last_input_ms: Option<u64>,
) -> u64 {
    let interaktiv = last_input_ms
        .is_some_and(|input_ms| now_ms.saturating_sub(input_ms) <= BROWSER_INPUT_ACTIVE_WINDOW_MS);
    if interaktiv {
        frame_rate_target
    } else {
        BROWSER_FRAME_RATE_IDLE.min(frame_rate_target.max(1))
    }
}

fn browser_frame_capture_due(
    now_ms: u64,
    last_frame_ms: Option<u64>,
    lease_expires_ms: Option<u64>,
    frame_rate_target: u64,
    last_input_ms: Option<u64>,
) -> bool {
    let Some(last_frame_ms) = last_frame_ms else {
        return true;
    };
    if lease_expires_ms.is_some_and(|expires_ms| expires_ms <= now_ms) {
        return false;
    }
    let rate = browser_effective_frame_rate(frame_rate_target, now_ms, last_input_ms);
    let min_interval_ms = browser_frame_capture_interval_ms(rate);
    now_ms.saturating_sub(last_frame_ms) >= min_interval_ms
}

fn browser_frame_capture_interval_ms(frame_rate_target: u64) -> u64 {
    let bounded_rate = frame_rate_target.clamp(1, BROWSER_FRAME_RATE_LIMIT);
    1000_u64.div_ceil(bounded_rate)
}

pub(super) async fn wait_for_browser_frame_capture_slot(
    database: &Arc<RxDatabase>,
    session_id: &str,
) -> anyhow::Result<()> {
    let session = find_browser_document(database, "browser_sessions", session_id).await?;
    let frame_rate_target = session
        .get("frame_rate_target")
        .and_then(Value::as_u64)
        .unwrap_or(BROWSER_FRAME_RATE_TARGET_DEFAULT);
    let min_interval = Duration::from_millis(browser_frame_capture_interval_ms(frame_rate_target));
    let slots = BROWSER_FRAME_CAPTURE_SLOTS.get_or_init(|| Mutex::new(HashMap::new()));
    loop {
        let wait_for = {
            let mut slots = slots
                .lock()
                .map_err(|_| anyhow::anyhow!("browser frame capture slots poisoned"))?;
            let now = Instant::now();
            match slots.get(session_id).copied() {
                Some(last_capture) if now.duration_since(last_capture) < min_interval => {
                    min_interval - now.duration_since(last_capture)
                }
                _ => {
                    slots.insert(session_id.to_string(), now);
                    Duration::ZERO
                }
            }
        };
        if wait_for.is_zero() {
            return Ok(());
        }
        tokio::time::sleep(wait_for).await;
    }
}

pub(super) async fn matching_active_browser_frame(
    database: &Arc<RxDatabase>,
    session: &Value,
    frame_hash: Option<&str>,
) -> anyhow::Result<Option<Value>> {
    let Some(frame_id) = session
        .get("active_frame_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(None);
    };
    let frame = find_browser_document(database, "browser_frames", frame_id).await?;
    if !frame.is_object() {
        return Ok(None);
    }
    if frame_hash.is_some_and(|hash| frame.get("frame_hash").and_then(Value::as_str) != Some(hash))
    {
        return Ok(None);
    }
    Ok(Some(frame))
}

/// Capture a frame for one live session when one is due, so the browser has a
/// picture without the user having to send input first.
async fn refresh_browser_session_frame(
    database: &Arc<RxDatabase>,
    session_id: &str,
) -> anyhow::Result<usize> {
    let manager = browser_runtime_manager();
    let Some(session) = manager.get(session_id) else {
        return Ok(0);
    };
    let session_doc = find_browser_document(database, "browser_sessions", session_id).await?;

    let frames_collection = database
        .collection("browser_frames")
        .context("browser_frames collection is not registered")?;
    let newest = frames_collection
        .find(Some(MangoQuery {
            selector: Some(json!({ "session_id": { "$eq": session_id } })),
            sort: Some(vec![[("captured_at_ms".to_string(), "desc".to_string())]
                .into_iter()
                .collect()]),
            limit: Some(1),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query newest browser_frames: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec newest browser_frames query: {err}"))?;
    let last_frame_ms = newest
        .as_array()
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("captured_at_ms"))
        .and_then(Value::as_u64);

    let lease_expires_ms = session_doc
        .get("controller_lease_expires_at_ms")
        .and_then(Value::as_u64)
        .filter(|expires_ms| *expires_ms > 0);
    let due = browser_frame_capture_due(
        now_ms() as u64,
        last_frame_ms,
        lease_expires_ms,
        session_doc
            .get("frame_rate_target")
            .and_then(Value::as_u64)
            .unwrap_or(BROWSER_FRAME_RATE_TARGET_DEFAULT),
        browser_last_input_ms(session_id),
    ) && browser_session_is_watched(
        session_id,
        now_ms() as u64,
        session_doc.get("updated_at_ms").and_then(Value::as_u64),
    );
    if !due {
        // A live stream remains active between capture slots so the maintenance
        // loop does not back off for ten seconds. Only an expired lease is idle.
        return Ok(usize::from(
            lease_expires_ms.is_none_or(|expires_ms| expires_ms > now_ms() as u64),
        ));
    }

    // A failed capture must not abort the maintenance round for the other
    // sessions; the runtime may simply be mid-navigation.
    if let Err(err) = wait_for_browser_frame_capture_slot(database, session_id).await {
        eprintln!("[business-os] browser frame rate limiting failed for {session_id}: {err:#}");
        return Ok(0);
    }
    if let Err(err) = capture_and_store_browser_frame(database, &session, session_id, None).await {
        eprintln!("[business-os] browser frame capture failed for {session_id}: {err:#}");
        return Ok(0);
    }
    Ok(1)
}

/// Replay all pending `browser_input_events` for one session against its live
/// page, mark them consumed/failed, and refresh the frame if anything applied.
async fn drain_browser_session_inputs(
    database: &Arc<RxDatabase>,
    session_id: &str,
) -> anyhow::Result<usize> {
    let manager = browser_runtime_manager();
    let Some(session) = manager.get(session_id) else {
        return Ok(0);
    };

    let events_collection = database
        .collection("browser_input_events")
        .context("browser_input_events collection is not registered")?;
    let pending = events_collection
        .find(Some(MangoQuery {
            selector: Some(json!({
                "session_id": { "$eq": session_id },
                "status": { "$eq": "pending" }
            })),
            sort: Some(vec![[("seq".to_string(), "asc".to_string())]
                .into_iter()
                .collect()]),
            limit: Some(64),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query pending browser_input_events: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec pending browser_input_events query: {err}"))?;
    let Some(rows) = pending.as_array() else {
        return Ok(0);
    };
    if rows.is_empty() {
        return Ok(0);
    }
    let touched_rows = rows.len();
    // Ab hier gilt die Sitzung als bedient: die Bildrate darf fuer das
    // Rueckmeldefenster hochgehen und faellt danach wieder in den Ruhetakt.
    note_browser_input_activity(session_id);

    let mut events = Vec::with_capacity(rows.len());
    for row in rows {
        let seq = row.get("seq").and_then(Value::as_u64).unwrap_or(0);
        let event = browser_runtime_input_event(row);
        eprintln!(
            "{}",
            browser_input_log_line("received", session_id, seq, &event)
        );
        events.push(event);
    }

    let response = match manager
        .request(&session, "input", json!({ "events": events }))
        .await
    {
        Ok(value) => value,
        Err(err) => {
            // Process is dead: drop it, fail the batch, surface the error.
            manager.drop_session_after_crash(session_id);
            let now = now_ms() as u64;
            for row in rows {
                if let Some(id) = row.get("id").and_then(Value::as_str) {
                    let mut next = row.clone();
                    if let Some(obj) = next.as_object_mut() {
                        obj.insert("status".to_string(), Value::String("failed".to_string()));
                        obj.insert("error".to_string(), Value::String(format!("{err:#}")));
                        obj.insert("updated_at_ms".to_string(), Value::from(now));
                    }
                    let _ = id;
                    events_collection
                        .incremental_upsert(next)
                        .await
                        .map_err(|e| anyhow::anyhow!("mark input event failed: {e}"))?;
                }
            }
            let tab_id = find_browser_document(database, "browser_sessions", session_id)
                .await?
                .get("current_tab_id")
                .and_then(Value::as_str)
                .unwrap_or("browser_tab_default")
                .to_string();
            mark_browser_session_runtime_error(
                database,
                session_id,
                &tab_id,
                "",
                session.viewport_w,
                session.viewport_h,
                "browser.input",
                now,
                &format!("{err:#}"),
                None,
            )
            .await?;
            return Ok(touched_rows);
        }
    };

    let ok = response.get("ok").and_then(Value::as_bool) == Some(true);
    let results = response.get("results").and_then(Value::as_array);
    let now = now_ms() as u64;
    let mut applied_max_seq = 0u64;
    let mut applied_count = 0usize;
    for (index, row) in rows.iter().enumerate() {
        let result = results.and_then(|items| items.get(index));
        let row_ok = ok
            && result
                .and_then(|item| item.get("ok"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
        let mut next = row.clone();
        if let Some(obj) = next.as_object_mut() {
            if row_ok {
                obj.insert("status".to_string(), Value::String("consumed".to_string()));
                obj.insert("consumed_at_ms".to_string(), Value::from(now));
                applied_count = applied_count.saturating_add(1);
                applied_max_seq =
                    applied_max_seq.max(row.get("seq").and_then(Value::as_u64).unwrap_or(0));
            } else {
                obj.insert("status".to_string(), Value::String("failed".to_string()));
                obj.insert(
                    "error".to_string(),
                    Value::String(
                        result
                            .and_then(|item| item.get("error"))
                            .or_else(|| response.get("error"))
                            .and_then(Value::as_str)
                            .unwrap_or("browser runtime did not acknowledge this input event")
                            .to_string(),
                    ),
                );
            }
            obj.insert("updated_at_ms".to_string(), Value::from(now));
        }
        events_collection
            .incremental_upsert(next)
            .await
            .map_err(|err| anyhow::anyhow!("mark input event consumed: {err}"))?;
    }

    if applied_count > 0 {
        let nav = response.get("nav").cloned().unwrap_or(Value::Null);
        eprintln!(
            "[business-os] browser input phase=applied session_id={} events={} applied={} url={}",
            session_id,
            rows.len(),
            applied_count,
            nav.get("url").and_then(Value::as_str).unwrap_or_default(),
        );
        wait_for_browser_frame_capture_slot(database, session_id).await?;
        capture_and_store_browser_frame(database, &session, session_id, Some(&nav)).await?;
        update_browser_session_input_state(database, session_id, applied_max_seq).await?;
    }
    Ok(touched_rows)
}

/// Capture a fresh frame from the live page and persist it plus the derived
/// tab/session navigation state. `nav` may carry the most recent navigation
/// snapshot; otherwise it is read from the screenshot response. The persistent
/// runtime channel is request/response only, so it cannot deliver unsolicited
/// `Page.screencastFrame` events; bounded JPEG snapshots are used here instead.
async fn capture_and_store_browser_frame(
    database: &Arc<RxDatabase>,
    session: &Arc<super::browser_runtime::LiveBrowserSession>,
    session_id: &str,
    nav_hint: Option<&Value>,
) -> anyhow::Result<()> {
    let manager = browser_runtime_manager();
    let screenshot = manager
        .request(
            session,
            "screenshot",
            browser_frame_capture_request(session.viewport_w, session.viewport_h),
        )
        .await?;
    let data = screenshot
        .get("screenshot")
        .and_then(|frame| frame.get("base64"))
        .and_then(Value::as_str)
        .context("browser runtime did not return screenshot data")?
        .to_string();
    let mime_type = screenshot
        .get("screenshot")
        .and_then(|frame| frame.get("mimeType"))
        .and_then(Value::as_str)
        .unwrap_or("image/jpeg")
        .to_string();
    let nav = browser_capture_navigation(&screenshot, nav_hint);

    let session_doc = find_browser_document(database, "browser_sessions", session_id).await?;
    let tab_id = session_doc
        .get("current_tab_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or("browser_tab_default")
        .to_string();
    let url = nav
        .get("url")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| session_doc.get("current_url").and_then(Value::as_str))
        .unwrap_or("about:blank")
        .to_string();
    let title = nav
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| session_doc.get("title").and_then(Value::as_str))
        .unwrap_or("Remote Browser")
        .to_string();
    let can_go_back = nav
        .get("can_go_back")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let can_go_forward = nav
        .get("can_go_forward")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let frame_hash = browser_frame_hash(&data);
    if matching_active_browser_frame(database, &session_doc, Some(&frame_hash))
        .await?
        .is_some()
    {
        return Ok(());
    }
    let next_seq = session_doc
        .get("last_frame_seq")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        + 1;
    let frame_id = format!("browser_frame_{}_{}", session_id, next_seq);
    let size_bytes = base64::engine::general_purpose::STANDARD
        .decode(data.as_bytes())
        .map(|bytes| bytes.len() as u64)
        .unwrap_or_else(|_| data.len() as u64);
    let (frame_width, frame_height) =
        browser_frame_capture_dimensions(session.viewport_w, session.viewport_h);
    upsert_browser_frame(
        database,
        &frame_id,
        session_id,
        &tab_id,
        next_seq,
        &mime_type,
        &data,
        frame_width,
        frame_height,
        size_bytes,
        &frame_hash,
        None,
    )
    .await?;
    upsert_browser_tab(
        database,
        &tab_id,
        session_id,
        &title,
        &url,
        "active",
        false,
        can_go_back,
        can_go_forward,
        Some(&frame_id),
        next_seq,
        None,
    )
    .await?;
    upsert_browser_session(
        database,
        session_id,
        &tab_id,
        "active",
        "active",
        &url,
        &title,
        session.viewport_w,
        session.viewport_h,
        Some(&frame_id),
        next_seq,
        "browser.input",
        now_ms() as u64,
        None,
        None,
        None,
    )
    .await?;
    // The frame and its active pointer are finished work. Retention is
    // best-effort cleanup and must never turn that successful capture into an
    // error returned to the maintenance loop.
    if let Err(err) = prune_browser_session_frames(database, session_id).await {
        eprintln!("[business-os] browser frame pruning failed for {session_id}: {err:#}");
    }
    Ok(())
}

fn browser_capture_navigation(screenshot: &Value, nav_hint: Option<&Value>) -> Value {
    screenshot
        .get("nav")
        .cloned()
        .filter(|value| !value.is_null())
        .or_else(|| nav_hint.cloned().filter(|value| !value.is_null()))
        .unwrap_or(Value::Null)
}

fn browser_runtime_input_event(row: &Value) -> Value {
    let click_count = row
        .get("detail")
        .and_then(Value::as_u64)
        .or_else(|| {
            row.get("payload")
                .and_then(|payload| payload.get("click_count"))
                .and_then(Value::as_u64)
        })
        .unwrap_or(0);
    json!({
        "type": row.get("type").and_then(Value::as_str).unwrap_or_default(),
        "x": row.get("x").and_then(Value::as_f64).unwrap_or(0.0),
        "y": row.get("y").and_then(Value::as_f64).unwrap_or(0.0),
        "detail": click_count,
        "clickCount": click_count,
        "button": row.get("button").and_then(Value::as_str).unwrap_or("left"),
        "buttons": row.get("buttons").and_then(Value::as_u64).unwrap_or(0),
        "dx": row.get("dx").and_then(Value::as_f64).unwrap_or(0.0),
        "dy": row.get("dy").and_then(Value::as_f64).unwrap_or(0.0),
        "key": row.get("key").and_then(Value::as_str).unwrap_or_default(),
        "code": row.get("code").and_then(Value::as_str).unwrap_or_default(),
        "modifiers": row.get("modifiers").cloned().unwrap_or_else(|| json!([])),
        "text": row.get("text").and_then(Value::as_str).unwrap_or_default()
    })
}

fn browser_input_log_line(phase: &str, session_id: &str, seq: u64, event: &Value) -> String {
    let modifiers = event
        .get("modifiers")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("+")
        })
        .unwrap_or_default();
    format!(
        "[business-os] browser input phase={} session_id={} seq={} type={} x={} y={} detail={} button={} buttons={} modifiers={} key={} code={}",
        phase.trim(),
        session_id.trim(),
        seq,
        event.get("type").and_then(Value::as_str).unwrap_or_default(),
        event.get("x").and_then(Value::as_f64).unwrap_or(0.0),
        event.get("y").and_then(Value::as_f64).unwrap_or(0.0),
        event.get("detail").and_then(Value::as_u64).unwrap_or(0),
        event.get("button").and_then(Value::as_str).unwrap_or_default(),
        event.get("buttons").and_then(Value::as_u64).unwrap_or(0),
        modifiers,
        event.get("key").and_then(Value::as_str).unwrap_or_default(),
        event.get("code").and_then(Value::as_str).unwrap_or_default(),
    )
}

/// Recompute `last_input_seq` and the live pending input count on a session
/// after a drain pass.
async fn update_browser_session_input_state(
    database: &Arc<RxDatabase>,
    session_id: &str,
    last_input_seq: u64,
) -> anyhow::Result<()> {
    let events_collection = database
        .collection("browser_input_events")
        .context("browser_input_events collection is not registered")?;
    let pending = events_collection
        .count(Some(MangoQuery {
            selector: Some(json!({
                "session_id": { "$eq": session_id },
                "status": { "$eq": "pending" }
            })),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("count pending browser_input_events: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec count pending browser_input_events: {err}"))?;
    let pending_count = pending.as_u64().unwrap_or(0);

    let sessions = database
        .collection("browser_sessions")
        .context("browser_sessions collection is not registered")?;
    let existing = find_browser_document(database, "browser_sessions", session_id).await?;
    if !existing.is_object() {
        return Ok(());
    }
    let mut next = existing;
    if let Some(obj) = next.as_object_mut() {
        obj.remove("_rev");
        obj.remove("_meta");
        let prev = obj
            .get("last_input_seq")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        obj.insert(
            "last_input_seq".to_string(),
            Value::from(prev.max(last_input_seq)),
        );
        obj.insert(
            "pending_input_count".to_string(),
            Value::from(pending_count),
        );
        obj.insert("updated_at_ms".to_string(), Value::from(now_ms() as u64));
    }
    sessions
        .incremental_upsert(next)
        .await
        .map_err(|err| anyhow::anyhow!("update browser session input state: {err}"))?;
    Ok(())
}

/// Choose which frames from one session fall outside its recent-frame window.
///
/// The active frame always occupies one survivor slot, even if it is older than
/// every other row. Remaining slots go to the highest sequence numbers. Rows
/// from other sessions are ignored so a per-session prune cannot cross session
/// boundaries even if its caller supplies a mixed result set.
fn browser_frame_ids_to_retire(
    rows: &[Value],
    session_id: &str,
    active_frame_id: Option<&str>,
    keep_count: usize,
) -> Vec<String> {
    let mut frames = rows
        .iter()
        .filter(|row| row.get("session_id").and_then(Value::as_str) == Some(session_id))
        .filter_map(|row| {
            let id = row.get("id").and_then(Value::as_str)?.trim();
            if id.is_empty() {
                return None;
            }
            Some((
                row.get("seq").and_then(Value::as_u64).unwrap_or(0),
                row.get("captured_at_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                id.to_string(),
            ))
        })
        .collect::<Vec<_>>();
    frames.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| right.2.cmp(&left.2))
    });

    let active_frame_id = active_frame_id.filter(|id| !id.trim().is_empty());
    let mut survivors = HashSet::new();
    if let Some(active_frame_id) = active_frame_id {
        if frames.iter().any(|(_, _, id)| id == active_frame_id) {
            survivors.insert(active_frame_id.to_string());
        }
    }
    for (_, _, id) in &frames {
        if survivors.len() >= keep_count {
            break;
        }
        survivors.insert(id.clone());
    }

    frames
        .into_iter()
        .filter_map(|(_, _, id)| (!survivors.contains(&id)).then_some(id))
        .collect()
}

/// Retire all but the bounded recent-frame window for one session. The active
/// pointer is read after the frame query so a capture that advanced it while the
/// query was running remains protected.
async fn prune_browser_session_frames(
    database: &Arc<RxDatabase>,
    session_id: &str,
) -> anyhow::Result<usize> {
    let frames = database
        .collection("browser_frames")
        .context("browser_frames collection is not registered")?;
    let session_frames = frames
        .find(Some(MangoQuery {
            selector: Some(json!({ "session_id": { "$eq": session_id } })),
            sort: Some(vec![[("seq".to_string(), "desc".to_string())]
                .into_iter()
                .collect()]),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query browser_frames for session {session_id}: {err}"))?
        .exec(false)
        .await
        .map_err(|err| {
            anyhow::anyhow!("exec browser_frames prune query for session {session_id}: {err}")
        })?;
    let Some(rows) = session_frames.as_array() else {
        return Ok(0);
    };
    let session = find_browser_document(database, "browser_sessions", session_id).await?;
    let active_frame_id = session
        .get("active_frame_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty());
    let ids = browser_frame_ids_to_retire(
        rows,
        session_id,
        active_frame_id,
        BROWSER_FRAME_RECENT_KEEP_COUNT,
    );
    if ids.is_empty() {
        return Ok(0);
    }
    let ids = ids.into_iter().collect::<HashSet<_>>();
    let retired = rows
        .iter()
        .filter(|row| {
            row.get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| ids.contains(id))
        })
        .cloned()
        .collect::<Vec<_>>();
    redact_and_remove_browser_frames(database, &retired, "superseded").await
}

/// Remove expired frames so `browser_frames` does not grow without bound.
/// Erledigte Befehle nach einer Aufbewahrungsfrist entfernen.
///
/// `business_commands` war faktisch append-only: nichts hat je etwas daraus
/// entfernt. Auf der Produktionsinstanz standen am 19.08.2026 6768 Zeilen darin,
/// davon 6742 in einem Endzustand und 3688 volle 47 Tage alt, in einem 929 MB
/// grossen Speicher. Jede Client-Aktion fragt diese Collection erneut ab;
/// gemessen wurden dort 3,7 s, 5,0 s und 12,3 s fuer eine einzelne Abfrage.
/// Der Browserstart lief deshalb in seine Zeitueberschreitung und meldete dem
/// Nutzer "CTOX ist nicht mit dem Browser-Datenkanal verbunden" — ein
/// Verbindungsfehler, den es nie gab.
///
/// Nur Endzustaende werden geraeumt und nur jenseits der Frist; laufende und
/// angenommene Befehle bleiben unangetastet. Die Obergrenze je Durchlauf haelt
/// die Wartung kurz, statt einmalig eine grosse Loeschung zu fahren.
async fn gc_settled_business_commands(database: &Arc<RxDatabase>) -> anyhow::Result<usize> {
    let cutoff = (now_ms() as u64).saturating_sub(BUSINESS_COMMAND_RETENTION_MS);
    let commands = database
        .collection("business_commands")
        .context("business_commands collection is not registered")?;
    let settled = commands
        .find(Some(MangoQuery {
            selector: Some(json!({
                "status": { "$in": ["completed", "failed", "cancelled", "canceled", "blocked"] },
                "updated_at_ms": { "$lt": cutoff }
            })),
            limit: Some(BUSINESS_COMMAND_GC_LIMIT),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query settled business_commands: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec settled business_commands query: {err}"))?;
    let Some(rows) = settled.as_array() else {
        return Ok(0);
    };
    let ids: Vec<String> = rows
        .iter()
        .filter_map(|row| row.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    if ids.is_empty() {
        return Ok(0);
    }
    let removed = ids.len();
    commands
        .bulk_remove_by_ids(ids)
        .await
        .map_err(|err| anyhow::anyhow!("remove settled business_commands: {err}"))?;
    Ok(removed)
}

async fn gc_expired_browser_frames(database: &Arc<RxDatabase>) -> anyhow::Result<usize> {
    let now = now_ms() as u64;
    let frames = database
        .collection("browser_frames")
        .context("browser_frames collection is not registered")?;
    let expired = frames
        .find(Some(MangoQuery {
            selector: Some(json!({ "expires_at_ms": { "$lt": now } })),
            limit: Some(BROWSER_FRAME_GC_LIMIT),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query expired browser_frames: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec expired browser_frames query: {err}"))?;
    let Some(rows) = expired.as_array() else {
        return Ok(0);
    };
    redact_and_remove_browser_frames(database, rows, "expired").await
}

async fn redact_and_remove_browser_frames(
    database: &Arc<RxDatabase>,
    rows: &[Value],
    reason: &str,
) -> anyhow::Result<usize> {
    let ids: Vec<String> = rows
        .iter()
        .filter_map(|row| row.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    if ids.is_empty() {
        return Ok(0);
    }
    let frames = database
        .collection("browser_frames")
        .context("browser_frames collection is not registered")?;
    let redacted = rows
        .iter()
        .filter_map(redacted_browser_frame_for_removal)
        .collect::<Vec<_>>();
    if !redacted.is_empty() {
        frames
            .bulk_upsert(redacted)
            .await
            .map_err(|err| anyhow::anyhow!("redact {reason} browser_frames: {err}"))?;
    }
    frames
        .bulk_remove_by_ids(ids)
        .await
        .map_err(|err| anyhow::anyhow!("remove {reason} browser_frames: {err}"))?;
    Ok(rows.len())
}

fn redacted_browser_frame_for_removal(row: &Value) -> Option<Value> {
    let mut next = row.clone();
    let obj = next.as_object_mut()?;
    obj.remove("_rev");
    obj.remove("_meta");
    obj.insert("data".to_string(), Value::String(String::new()));
    obj.insert(
        "encoding".to_string(),
        Value::String("redacted".to_string()),
    );
    obj.insert("size_bytes".to_string(), Value::from(0));
    obj.insert("frame_hash".to_string(), Value::String(String::new()));
    obj.insert("updated_at_ms".to_string(), Value::from(now_ms() as u64));
    Some(next)
}

/// Drop consumed/failed input events after a retention window. Pending events
/// stay until drained, and the bounded query keeps active browser maintenance
/// from repeatedly touching old input-event history while idle.
async fn gc_consumed_browser_input_events(database: &Arc<RxDatabase>) -> anyhow::Result<usize> {
    let cutoff = (now_ms() as u64).saturating_sub(BROWSER_INPUT_EVENT_RETENTION_SECS * 1_000);
    let events = database
        .collection("browser_input_events")
        .context("browser_input_events collection is not registered")?;
    let mut removed = 0usize;
    for status in ["consumed", "failed"] {
        let stale = events
            .find(Some(MangoQuery {
                selector: Some(json!({
                    "status": { "$eq": status },
                    "created_at_ms": { "$lt": cutoff }
                })),
                sort: Some(vec![[("created_at_ms".to_string(), "asc".to_string())]
                    .into_iter()
                    .collect()]),
                limit: Some(BROWSER_INPUT_EVENT_GC_LIMIT),
                ..Default::default()
            }))
            .map_err(|err| anyhow::anyhow!("query stale browser_input_events: {err}"))?
            .exec(false)
            .await
            .map_err(|err| anyhow::anyhow!("exec stale browser_input_events query: {err}"))?;
        let Some(rows) = stale.as_array() else {
            continue;
        };
        let ids = rows
            .iter()
            .filter_map(|row| row.get("id").and_then(Value::as_str))
            .map(str::to_string)
            .collect::<Vec<_>>();
        if ids.is_empty() {
            continue;
        }
        events
            .bulk_remove_by_ids(ids)
            .await
            .map_err(|err| anyhow::anyhow!("remove stale browser_input_events: {err}"))?;
        removed = removed.saturating_add(rows.len());
    }
    Ok(removed)
}

/// On peer startup, no live processes exist yet. Any session row left `active`
/// from a previous run is stale; mark it disconnected so the UI does not show a
/// dead live session as running.
pub(super) async fn recover_stale_browser_sessions(
    database: &Arc<RxDatabase>,
) -> anyhow::Result<()> {
    let sessions = database
        .collection("browser_sessions")
        .context("browser_sessions collection is not registered")?;
    let active = sessions
        .find(Some(MangoQuery {
            selector: Some(json!({ "status": { "$eq": "active" } })),
            limit: Some(128),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query active browser_sessions: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec active browser_sessions query: {err}"))?;
    let Some(rows) = active.as_array() else {
        return Ok(());
    };
    let manager = browser_runtime_manager();
    let now = now_ms() as u64;
    for row in rows {
        let Some(session_id) = row.get("id").and_then(Value::as_str) else {
            continue;
        };
        if manager.has_session(session_id) {
            continue;
        }
        let status = row
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if status == "error" || status == "disconnected" {
            continue;
        }
        let mut next = row.clone();
        if let Some(obj) = next.as_object_mut() {
            obj.remove("_rev");
            obj.remove("_meta");
            obj.insert(
                "status".to_string(),
                Value::String("disconnected".to_string()),
            );
            obj.insert(
                "runtime_status".to_string(),
                Value::String("disconnected".to_string()),
            );
            obj.insert("updated_at_ms".to_string(), Value::from(now));
        }
        sessions
            .incremental_upsert(next)
            .await
            .map_err(|err| anyhow::anyhow!("mark stale browser session disconnected: {err}"))?;
    }
    Ok(())
}

pub(super) async fn find_browser_document(
    database: &Arc<RxDatabase>,
    collection_name: &str,
    id: &str,
) -> anyhow::Result<Value> {
    let collection = database
        .collection(collection_name)
        .with_context(|| format!("{collection_name} collection is not registered"))?;
    let existing = collection
        .find_one(Some(MangoQuery {
            selector: Some(json!({ "id": { "$eq": id } })),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query {collection_name} {id}: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec {collection_name} {id} query: {err}"))?;
    Ok(existing
        .is_object()
        .then_some(existing)
        .unwrap_or(Value::Null))
}

async fn has_newer_browser_runtime_command(
    database: &Arc<RxDatabase>,
    session_id: &str,
    command_created_at_ms: u64,
) -> anyhow::Result<bool> {
    let collection = database
        .collection("business_commands")
        .context("business_commands collection is not registered")?;
    let rows = collection
        .find(Some(MangoQuery {
            selector: Some(json!({ "module": { "$eq": "browser" } })),
            limit: Some(500),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query newer browser commands: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec newer browser commands query: {err}"))?;
    let Some(commands) = rows.as_array() else {
        return Ok(false);
    };
    Ok(commands.iter().any(|command| {
        let Some(candidate_type) = command
            .get("command_type")
            .or_else(|| command.get("type"))
            .and_then(Value::as_str)
        else {
            return false;
        };
        if !is_browser_runtime_command(candidate_type) {
            return false;
        }
        let candidate_session_id = command
            .get("payload")
            .and_then(|payload| payload.get("session_id"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("browser_session_default");
        if candidate_session_id != session_id {
            return false;
        }
        let candidate_created_at_ms = command
            .get("created_at_ms")
            .or_else(|| command.get("updated_at_ms"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        candidate_created_at_ms > command_created_at_ms
    }))
}

#[derive(Debug, Clone, Default)]
pub(super) struct BrowserSessionAccess {
    tenant_id: Option<String>,
    owner_user_id: Option<String>,
    controller_user_id: Option<String>,
    controller_lease_id: Option<String>,
    controller_lease_expires_at_ms: Option<u64>,
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn upsert_browser_session(
    database: &Arc<RxDatabase>,
    session_id: &str,
    tab_id: &str,
    status: &str,
    runtime_status: &str,
    url: &str,
    title: &str,
    viewport_w: u64,
    viewport_h: u64,
    frame_id: Option<&str>,
    frame_seq: u64,
    command_type: &str,
    command_created_at_ms: u64,
    error: Option<&str>,
    access: Option<&BrowserSessionAccess>,
    metadata_patch: Option<&Value>,
) -> anyhow::Result<()> {
    let now = now_ms() as u64;
    let existing = find_browser_document(database, "browser_sessions", session_id).await?;
    let existing_command_created_at_ms = existing
        .get("payload")
        .and_then(|payload| payload.get("last_command_created_at_ms"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let updates_command_watermark = browser_updates_command_watermark(command_type);
    if updates_command_watermark && existing_command_created_at_ms > command_created_at_ms {
        return Ok(());
    }
    // Carry forward input bookkeeping so a lifecycle/navigation write does not
    // clobber counts maintained by the input-drain loop or the UI.
    let preserved_last_input_seq = existing
        .get("last_input_seq")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let preserved_pending_input_count = existing
        .get("pending_input_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let preserved_tenant_id = existing
        .get("tenant_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let preserved_owner_user_id = existing
        .get("owner_user_id")
        .and_then(Value::as_str)
        .unwrap_or("ctox");
    let preserved_controller_user_id = existing
        .get("controller_user_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let preserved_controller_lease_id = existing
        .get("controller_lease_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let preserved_controller_lease_expires_at_ms = existing
        .get("controller_lease_expires_at_ms")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    // A stored `0` means "never streamed" — the sessions written before the
    // frame loop existed all carry it — so it heals to the default instead of
    // pinning the session at zero frames per second forever.
    let frame_rate_target = existing
        .get("frame_rate_target")
        .and_then(Value::as_u64)
        .filter(|rate| *rate > 0)
        .unwrap_or(BROWSER_FRAME_RATE_TARGET_DEFAULT)
        .min(BROWSER_FRAME_RATE_LIMIT);
    let tenant_id = access
        .and_then(|value| value.tenant_id.as_deref())
        .unwrap_or(preserved_tenant_id);
    let owner_user_id = access
        .and_then(|value| value.owner_user_id.as_deref())
        .unwrap_or(preserved_owner_user_id);
    let controller_user_id = access
        .and_then(|value| value.controller_user_id.as_deref())
        .unwrap_or(preserved_controller_user_id);
    let controller_lease_id = access
        .and_then(|value| value.controller_lease_id.as_deref())
        .unwrap_or(preserved_controller_lease_id);
    let controller_lease_expires_at_ms = access
        .and_then(|value| value.controller_lease_expires_at_ms)
        .unwrap_or(preserved_controller_lease_expires_at_ms);
    let mut payload = existing
        .get("payload")
        .cloned()
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    payload["browser_stream"] = Value::String("rxdb".to_string());
    if updates_command_watermark {
        payload["last_command_type"] = Value::String(command_type.to_string());
        payload["last_command_created_at_ms"] = Value::from(command_created_at_ms);
    }
    payload["runtime"] = Value::String("ctox-web-stack".to_string());
    payload["updated_by"] = Value::String("native-rxdb-peer".to_string());
    if let Some(metadata_patch) = metadata_patch {
        for key in [
            "source_id",
            "purpose",
            "target_url",
            "allowed_domains",
            "capture_script",
            "verify_selector",
            "credential_selector",
            "secret_name",
            "auth_assist_command_id",
            "auth_assist_task_id",
            "requesting_task_id",
            "instruction",
            "auth_assist_status",
            "authenticated",
            "authenticated_at_ms",
            "continuation_status",
            "profile_mode",
            "credential_fill_status",
            "credential_field_role",
            "secret_value_in_rxdb",
        ] {
            if let Some(value) = metadata_patch.get(key) {
                payload[key] = value.clone();
            }
        }
    }
    if let Some(error) = error {
        payload["error"] = Value::String(error.to_string());
    }
    let doc = json!({
        "id": session_id,
        "tenant_id": tenant_id,
        "owner_user_id": owner_user_id,
        "controller_user_id": controller_user_id,
        "controller_lease_id": controller_lease_id,
        "controller_lease_expires_at_ms": controller_lease_expires_at_ms,
        "status": status,
        "runtime_status": runtime_status,
        "current_tab_id": tab_id,
        "current_url": url,
        "title": title,
        "viewport_w": viewport_w,
        "viewport_h": viewport_h,
        "device_scale_factor": 1,
        "frame_rate_target": frame_rate_target,
        "active_frame_id": frame_id.unwrap_or_default(),
        "last_frame_seq": frame_seq,
        "last_input_seq": preserved_last_input_seq,
        "pending_input_count": preserved_pending_input_count,
        "error": error.unwrap_or_default(),
        "payload": payload,
        "created_at_ms": now,
        "updated_at_ms": now
    });
    database
        .collection("browser_sessions")
        .context("browser_sessions collection is not registered")?
        .incremental_upsert(doc)
        .await
        .map_err(|err| anyhow::anyhow!("upsert browser session {session_id}: {err}"))?;
    Ok(())
}

fn browser_updates_command_watermark(command_type: &str) -> bool {
    command_type != "browser.input"
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn upsert_browser_tab(
    database: &Arc<RxDatabase>,
    tab_id: &str,
    session_id: &str,
    title: &str,
    url: &str,
    status: &str,
    loading: bool,
    can_go_back: bool,
    can_go_forward: bool,
    frame_id: Option<&str>,
    frame_seq: u64,
    access: Option<&BrowserSessionAccess>,
) -> anyhow::Result<()> {
    let now = now_ms() as u64;
    let session = find_browser_document(database, "browser_sessions", session_id).await?;
    let tenant_id = access
        .and_then(|value| value.tenant_id.as_deref())
        .or_else(|| session.get("tenant_id").and_then(Value::as_str))
        .unwrap_or_default();
    let owner_user_id = access
        .and_then(|value| value.owner_user_id.as_deref())
        .or_else(|| session.get("owner_user_id").and_then(Value::as_str))
        .unwrap_or_default();
    let controller_user_id = access
        .and_then(|value| value.controller_user_id.as_deref())
        .or_else(|| session.get("controller_user_id").and_then(Value::as_str))
        .unwrap_or_default();
    let doc = json!({
        "id": tab_id,
        "tenant_id": tenant_id,
        "owner_user_id": owner_user_id,
        "controller_user_id": controller_user_id,
        "session_id": session_id,
        "title": title,
        "url": url,
        "status": status,
        "loading": loading,
        "active": true,
        "can_go_back": can_go_back,
        "can_go_forward": can_go_forward,
        "frame_seq": frame_seq,
        "last_frame_id": frame_id.unwrap_or_default(),
        "last_frame_at_ms": frame_id.map(|_| now).unwrap_or(0),
        "error": "",
        "payload": {
            "browser_stream": "rxdb",
            "updated_by": "native-rxdb-peer"
        },
        "created_at_ms": now,
        "updated_at_ms": now
    });
    database
        .collection("browser_tabs")
        .context("browser_tabs collection is not registered")?
        .incremental_upsert(doc)
        .await
        .map_err(|err| anyhow::anyhow!("upsert browser tab {tab_id}: {err}"))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn upsert_browser_frame(
    database: &Arc<RxDatabase>,
    frame_id: &str,
    session_id: &str,
    tab_id: &str,
    seq: u64,
    mime_type: &str,
    data: &str,
    width: u64,
    height: u64,
    size_bytes: u64,
    frame_hash: &str,
    access: Option<&BrowserSessionAccess>,
) -> anyhow::Result<()> {
    let now = now_ms() as u64;
    let session = find_browser_document(database, "browser_sessions", session_id).await?;
    let tenant_id = access
        .and_then(|value| value.tenant_id.as_deref())
        .or_else(|| session.get("tenant_id").and_then(Value::as_str))
        .unwrap_or_default();
    let owner_user_id = access
        .and_then(|value| value.owner_user_id.as_deref())
        .or_else(|| session.get("owner_user_id").and_then(Value::as_str))
        .unwrap_or_default();
    let controller_user_id = access
        .and_then(|value| value.controller_user_id.as_deref())
        .or_else(|| session.get("controller_user_id").and_then(Value::as_str))
        .unwrap_or_default();
    let doc = json!({
        "id": frame_id,
        "tenant_id": tenant_id,
        "owner_user_id": owner_user_id,
        "controller_user_id": controller_user_id,
        "session_id": session_id,
        "tab_id": tab_id,
        "seq": seq,
        "mime_type": mime_type,
        "encoding": "base64",
        "data": data,
        "width": width,
        "height": height,
        "viewport_w": width,
        "viewport_h": height,
        "quality": if mime_type == "image/jpeg" { BROWSER_FRAME_JPEG_QUALITY } else { 0 },
        "size_bytes": size_bytes,
        "frame_hash": frame_hash,
        "captured_at_ms": now,
        "expires_at_ms": now + 15 * 60 * 1000,
        "updated_at_ms": now
    });
    database
        .collection("browser_frames")
        .context("browser_frames collection is not registered")?
        .incremental_upsert(doc)
        .await
        .map_err(|err| anyhow::anyhow!("upsert browser frame {frame_id}: {err}"))?;
    Ok(())
}

async fn mark_browser_runtime_command_completed(
    database: &Arc<RxDatabase>,
    document: &Value,
    accepted: &Value,
    result: Value,
) -> anyhow::Result<()> {
    let command_id = document
        .get("command_id")
        .or_else(|| document.get("id"))
        .and_then(Value::as_str)
        .context("browser command is missing id")?;
    let mut next = document.clone();
    if let Some(object) = next.as_object_mut() {
        object.insert("status".to_string(), Value::String("completed".to_string()));
        object.insert(
            "task_status".to_string(),
            Value::String("completed".to_string()),
        );
        if let Some(task_id) = accepted.get("task_id") {
            object.insert("task_id".to_string(), task_id.clone());
        }
        object.insert("result".to_string(), result);
        object.insert("updated_at_ms".to_string(), Value::from(now_ms() as u64));
    }
    database
        .collection("business_commands")
        .context("business_commands collection is not registered")?
        .incremental_upsert(next)
        .await
        .map_err(|err| anyhow::anyhow!("complete browser command {command_id}: {err}"))?;
    Ok(())
}

pub(super) async fn mark_browser_runtime_command_failed(
    database: &Arc<RxDatabase>,
    command_id: &str,
    _payload: &Value,
    error: &anyhow::Error,
) -> anyhow::Result<()> {
    // The session-specific handler has already persisted the full runtime
    // chain. This generic command failure records the same chain on the command
    // without rewriting the session a second time with `Error::to_string()`.
    let message = format!("{error:#}");
    let commands = database
        .collection("business_commands")
        .context("business_commands collection is not registered")?;
    let existing = commands
        .find_one(Some(MangoQuery {
            selector: Some(json!({ "id": { "$eq": command_id } })),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query failed browser command {command_id}: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec failed browser command {command_id}: {err}"))?;
    let mut next = if existing.is_object() {
        existing
    } else {
        json!({ "id": command_id, "command_id": command_id })
    };
    if let Some(object) = next.as_object_mut() {
        object.insert("status".to_string(), Value::String("failed".to_string()));
        object.insert(
            "task_status".to_string(),
            Value::String("failed".to_string()),
        );
        object.insert("error".to_string(), Value::String(message.clone()));
        object.insert(
            "result".to_string(),
            json!({
                "ok": false,
                "browser_stream": "rxdb",
                "error": message
            }),
        );
        object.insert("updated_at_ms".to_string(), Value::from(now_ms() as u64));
    }
    commands
        .incremental_upsert(next)
        .await
        .map_err(|err| anyhow::anyhow!("mark browser command {command_id} failed: {err}"))?;
    Ok(())
}

fn browser_runtime_command_log_line(
    phase: &str,
    command_type: &str,
    command_id: &str,
    session_id: &str,
    error: Option<&str>,
) -> String {
    let mut line = format!(
        "[business-os] browser runtime command phase={} command_type={} command_id={} session_id={}",
        phase.trim(),
        command_type.trim(),
        command_id.trim(),
        session_id.trim(),
    );
    if let Some(error) = error.map(str::trim).filter(|value| !value.is_empty()) {
        line.push_str(" error=");
        line.push_str(&error.split_whitespace().collect::<Vec<_>>().join(" "));
    }
    line
}

fn normalize_browser_runtime_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    }
}

pub(super) fn browser_frame_hash(data: &str) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(data.as_bytes());
    let digest = hasher.finalize();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

#[cfg(test)]
mod tests {
    use super::super::rxdb_peer::tests::{open_test_database, rxdb_test_table_name};
    use super::super::rxdb_peer::{business_os_schema, sqlite_quote_identifier};
    use super::*;
    use rusqlite::{params, Connection};
    use rxdb::rx_database::RxCollectionCreator;
    use std::collections::HashMap;

    #[test]
    fn browser_runtime_command_trace_is_single_line_and_identifies_start() {
        assert_eq!(
            browser_runtime_command_log_line(
                "received",
                "browser.session.start",
                "browser_cmd_123",
                "browser_session_test_123",
                None,
            ),
            "[business-os] browser runtime command phase=received command_type=browser.session.start command_id=browser_cmd_123 session_id=browser_session_test_123"
        );
        assert_eq!(
            browser_runtime_command_log_line(
                "failed",
                "browser.session.start",
                "browser_cmd_123",
                "browser_session_test_123",
                Some("runner exited\nwithout a frame"),
            ),
            "[business-os] browser runtime command phase=failed command_type=browser.session.start command_id=browser_cmd_123 session_id=browser_session_test_123 error=runner exited without a frame"
        );
    }

    #[test]
    fn browser_capture_prefers_fresh_screenshot_navigation() {
        let screenshot = json!({ "nav": { "url": "https://iana.org/" } });
        let stale_hint = json!({ "url": "https://example.com/" });
        assert_eq!(
            browser_capture_navigation(&screenshot, Some(&stale_hint))["url"],
            "https://iana.org/"
        );
        assert!(!browser_updates_command_watermark("browser.input"));
        assert!(browser_updates_command_watermark("browser.navigate"));
    }

    #[test]
    fn browser_frame_capture_is_bounded_jpeg() {
        assert_eq!(browser_frame_capture_interval_ms(0), 1_000);
        assert_eq!(browser_frame_capture_interval_ms(5), 200);
        assert_eq!(browser_frame_capture_interval_ms(15), 67);
        assert_eq!(browser_frame_capture_interval_ms(120), 67);
        assert_eq!(browser_frame_capture_dimensions(1280, 720), (1280, 720));
        assert_eq!(browser_frame_capture_dimensions(1920, 947), (1280, 631));
        assert_eq!(browser_frame_capture_dimensions(1000, 1000), (720, 720));
        assert_eq!(
            browser_frame_capture_request(1920, 947),
            json!({
                "format": "jpeg",
                "quality": 70,
                "maxWidth": 1280,
                "maxHeight": 631,
                "everyNthFrame": 1,
            })
        );
    }

    #[test]
    fn direct_browser_live_session_suppresses_duplicate_rxdb_streaming() {
        let session_id = format!("browser_direct_live_test_{}", std::process::id());
        assert!(!browser_direct_live_session_active(&session_id));
        mark_browser_direct_live_session(&session_id);
        assert!(browser_direct_live_session_active(&session_id));
    }

    #[test]
    fn browser_input_transport_preserves_click_count_and_modifiers() {
        let row = json!({
            "type": "mouseDown",
            "x": 42.0,
            "y": 84.0,
            "detail": 3,
            "button": "left",
            "buttons": 1,
            "modifiers": ["Meta", "Shift"],
            "key": "a",
            "code": "KeyA",
            "text": ""
        });
        let event = browser_runtime_input_event(&row);
        assert_eq!(event["detail"], 3);
        assert_eq!(event["clickCount"], 3);
        assert_eq!(event["modifiers"], json!(["Meta", "Shift"]));
        assert_eq!(
            browser_input_log_line("received", "browser_session_test", 17, &event),
            "[business-os] browser input phase=received session_id=browser_session_test seq=17 type=mouseDown x=42 y=84 detail=3 button=left buttons=1 modifiers=Meta+Shift key=a code=KeyA"
        );
    }

    #[test]
    fn storing_a_new_browser_frame_retires_older_frames_beyond_the_keep_window() {
        let rows = vec![
            json!({ "id": "frame_1", "session_id": "session_1", "seq": 1 }),
            json!({ "id": "frame_2", "session_id": "session_1", "seq": 2 }),
            json!({ "id": "frame_3", "session_id": "session_1", "seq": 3 }),
        ];

        assert_eq!(
            browser_frame_ids_to_retire(
                &rows,
                "session_1",
                Some("frame_3"),
                BROWSER_FRAME_RECENT_KEEP_COUNT,
            ),
            vec!["frame_1".to_string()]
        );
    }

    #[test]
    fn browser_frame_pruning_never_retires_the_active_frame() {
        let rows = (1..=4)
            .map(|seq| {
                json!({
                    "id": format!("frame_{seq}"),
                    "session_id": "session_1",
                    "seq": seq,
                })
            })
            .collect::<Vec<_>>();

        for active_frame_id in ["frame_1", "frame_2", "frame_3", "frame_4"] {
            let retired = browser_frame_ids_to_retire(
                &rows,
                "session_1",
                Some(active_frame_id),
                BROWSER_FRAME_RECENT_KEEP_COUNT,
            );
            assert!(!retired.iter().any(|id| id == active_frame_id));
            assert_eq!(retired.len(), rows.len() - BROWSER_FRAME_RECENT_KEEP_COUNT);
        }
        assert_eq!(
            browser_frame_ids_to_retire(
                &rows,
                "session_1",
                Some("frame_1"),
                BROWSER_FRAME_RECENT_KEEP_COUNT,
            ),
            vec!["frame_3".to_string(), "frame_2".to_string()]
        );
    }

    #[test]
    fn browser_frame_pruning_does_not_touch_another_sessions_frames() {
        let rows = vec![
            json!({ "id": "session_1_frame_1", "session_id": "session_1", "seq": 1 }),
            json!({ "id": "session_2_frame_1", "session_id": "session_2", "seq": 1 }),
            json!({ "id": "session_1_frame_2", "session_id": "session_1", "seq": 2 }),
            json!({ "id": "session_2_frame_2", "session_id": "session_2", "seq": 2 }),
            json!({ "id": "session_1_frame_3", "session_id": "session_1", "seq": 3 }),
            json!({ "id": "session_2_frame_3", "session_id": "session_2", "seq": 3 }),
        ];

        assert_eq!(
            browser_frame_ids_to_retire(
                &rows,
                "session_1",
                Some("session_1_frame_3"),
                BROWSER_FRAME_RECENT_KEEP_COUNT,
            ),
            vec!["session_1_frame_1".to_string()]
        );
    }

    #[test]
    fn browser_runtime_maintenance_sleep_backs_off_after_idle_round() {
        assert_eq!(
            browser_runtime_maintenance_sleep(0),
            Duration::from_millis(BROWSER_RUNTIME_ACTIVE_MAINTENANCE_INTERVAL_MS)
        );
        assert_eq!(
            browser_runtime_maintenance_sleep(1),
            Duration::from_secs(BROWSER_RUNTIME_IDLE_MAINTENANCE_INTERVAL_SECS)
        );
        assert_eq!(
            browser_runtime_maintenance_sleep(u32::MAX),
            Duration::from_secs(BROWSER_RUNTIME_IDLE_MAINTENANCE_INTERVAL_SECS)
        );
    }

    #[tokio::test]
    async fn browser_runtime_gc_redacts_frames_and_retires_old_input_events() {
        let root = tempfile::tempdir().expect("temp root");
        let database_path = root.path().join("browser-runtime-gc.sqlite3");
        let database = open_test_database(database_path.clone())
            .await
            .expect("open test database");
        database
            .add_collections(HashMap::from([
                (
                    "browser_frames".to_string(),
                    RxCollectionCreator {
                        schema: business_os_schema("browser_frames", "id"),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
                (
                    "browser_input_events".to_string(),
                    RxCollectionCreator {
                        schema: business_os_schema("browser_input_events", "id"),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
            ]))
            .await
            .expect("add browser runtime collections");
        let frames = database
            .collection("browser_frames")
            .expect("browser_frames collection");
        frames
            .incremental_upsert(json!({
                "id": "expired_frame",
                "session_id": "session_1",
                "tab_id": "tab_1",
                "seq": 1,
                "mime_type": "image/png",
                "encoding": "base64",
                "data": "AAAA",
                "width": 10,
                "height": 10,
                "captured_at_ms": 1,
                "expires_at_ms": 1,
                "updated_at_ms": 1,
                "size_bytes": 3,
                "frame_hash": "hash",
            }))
            .await
            .expect("insert expired frame");
        let events = database
            .collection("browser_input_events")
            .expect("browser_input_events collection");
        for (id, status, created_at_ms) in [
            ("old_consumed", "consumed", 1_u64),
            ("old_failed", "failed", 1_u64),
            ("old_pending", "pending", 1_u64),
            (
                "recent_consumed",
                "consumed",
                now_ms() as u64 - (BROWSER_INPUT_EVENT_RETENTION_SECS * 500),
            ),
        ] {
            events
                .incremental_upsert(json!({
                    "id": id,
                    "session_id": "session_1",
                    "tab_id": "tab_1",
                    "seq": 1,
                    "type": "click",
                    "status": status,
                    "created_at_ms": created_at_ms,
                    "updated_at_ms": created_at_ms,
                }))
                .await
                .expect("insert browser input event");
        }

        assert_eq!(gc_expired_browser_frames(&database).await.unwrap(), 1);
        assert_eq!(
            gc_consumed_browser_input_events(&database).await.unwrap(),
            2
        );

        let conn = Connection::open(&database_path).expect("open sqlite");
        let frame_table = rxdb_test_table_name(&conn, "browser_frames");
        let frame: (i64, String, i64, String) = conn
            .query_row(
                &format!(
                    "SELECT deleted, json_extract(data, '$.data'), CAST(json_extract(data, '$.size_bytes') AS INTEGER), json_extract(data, '$.encoding') FROM {} WHERE id = ?1",
                    sqlite_quote_identifier(&frame_table)
                ),
                params!["expired_frame"],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("expired frame row");
        assert_eq!(frame, (1, String::new(), 0, "redacted".to_string()));

        let input_table = rxdb_test_table_name(&conn, "browser_input_events");
        let deleted_state = |id: &str| -> i64 {
            conn.query_row(
                &format!(
                    "SELECT deleted FROM {} WHERE id = ?1",
                    sqlite_quote_identifier(&input_table)
                ),
                params![id],
                |row| row.get(0),
            )
            .expect("input event row")
        };
        assert_eq!(deleted_state("old_consumed"), 1);
        assert_eq!(deleted_state("old_failed"), 1);
        assert_eq!(deleted_state("old_pending"), 0);
        assert_eq!(deleted_state("recent_consumed"), 0);
    }

    #[test]
    fn auth_assist_recovery_browser_confirmation_settles_preserved_request_once() {
        let root = tempfile::tempdir().unwrap();
        let claimed = crate::mission::channels::claim_business_command_with_queue(
            root.path(),
            crate::mission::channels::BusinessCommandClaimRequest {
                command_id: "auth-confirm".into(),
                idempotency_key: "auth-confirm".into(),
                payload_hash: "sha256:auth-confirm".into(),
                module: "ctox".into(),
                command_type: "web_stack.auth_assist.request".into(),
                record_id: "handelsregister.de".into(),
                intent: json!({"payload":{"purpose":"web_stack_auth","owner_user_id":"owner-a"}}),
                created_at_ms: 1_700_000_000_000,
            },
            crate::mission::channels::QueueTaskCreateRequest {
                title: "Confirm login".into(),
                prompt: "Owner completes login".into(),
                thread_key: "business-os/ctox/auth-confirm".into(),
                workspace_root: None,
                priority: "normal".into(),
                suggested_skill: None,
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .unwrap();
        assert_eq!(claimed.task.route_status, "blocked");
        // Settlement runs after explicit confirmation and the existing
        // browser-session/controller checks in the command handler.
        let result =
            settle_auth_assist_queue_task(root.path(), "auth-confirm", &claimed.task.message_key);
        assert_eq!(result["status"], "cancelled_after_user_confirmation");
        assert_eq!(
            settle_auth_assist_queue_task(root.path(), "auth-confirm", &claimed.task.message_key,)
                ["status"],
            "already_terminal",
        );
        assert_eq!(
            crate::mission::channels::recover_auth_assist_requests(root.path()).unwrap(),
            0,
        );
        let command =
            crate::mission::channels::business_command_projection(root.path(), "auth-confirm")
                .unwrap();
        assert_eq!(command["terminal_status"], "cancelled");
    }

    #[tokio::test]
    async fn browser_session_upsert_persists_verified_owner_and_controller_lease() {
        assert!(is_browser_runtime_command("browser.controller.acquire"));
        assert!(is_browser_runtime_command("browser.controller.renew"));
        assert!(is_browser_runtime_command("browser.controller.release"));
        assert!(is_browser_runtime_command("browser.credential.fill"));
        assert!(is_browser_runtime_command("web_stack.auth_assist.complete"));

        let root = tempfile::tempdir().expect("temp root");
        let database = open_test_database(root.path().join("browser-session-access.sqlite3"))
            .await
            .expect("open test database");
        database
            .add_collections(HashMap::from([
                (
                    "browser_sessions".to_string(),
                    RxCollectionCreator {
                        schema: business_os_schema("browser_sessions", "id"),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
                (
                    "browser_tabs".to_string(),
                    RxCollectionCreator {
                        schema: business_os_schema("browser_tabs", "id"),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
                (
                    "browser_frames".to_string(),
                    RxCollectionCreator {
                        schema: business_os_schema("browser_frames", "id"),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
            ]))
            .await
            .expect("add browser_sessions collection");
        let access = BrowserSessionAccess {
            tenant_id: Some("tenant-verified".to_string()),
            owner_user_id: Some("verified-user@example.test".to_string()),
            controller_user_id: Some("verified-user@example.test".to_string()),
            controller_lease_id: Some("lease-verified".to_string()),
            controller_lease_expires_at_ms: Some(123_456),
        };
        let auth_metadata = json!({
            "source_id": "example.test",
            "purpose": "web_stack_auth",
            "secret_name": "EXAMPLE_LOGIN",
            "secret_value_in_rxdb": false
        });

        upsert_browser_frame(
            &database,
            "browser_frame_verified_1",
            "browser_session_verified",
            "browser_tab_verified",
            1,
            "image/png",
            "ZmFrZQ==",
            1280,
            720,
            4,
            "frame-hash",
            Some(&access),
        )
        .await
        .expect("create scoped frame before session");
        upsert_browser_tab(
            &database,
            "browser_tab_verified",
            "browser_session_verified",
            "Example",
            "https://example.com",
            "active",
            false,
            false,
            false,
            Some("browser_frame_verified_1"),
            1,
            Some(&access),
        )
        .await
        .expect("create scoped tab before session");

        upsert_browser_session(
            &database,
            "browser_session_verified",
            "browser_tab_verified",
            "active",
            "active",
            "https://example.com",
            "Example",
            1280,
            720,
            Some("frame_1"),
            1,
            "browser.session.start",
            100,
            None,
            Some(&access),
            Some(&auth_metadata),
        )
        .await
        .expect("create verified browser session");
        upsert_browser_session(
            &database,
            "browser_session_verified",
            "browser_tab_verified",
            "active",
            "active",
            "https://example.com/next",
            "Example next",
            1280,
            720,
            Some("frame_2"),
            2,
            "browser.input",
            200,
            None,
            None,
            None,
        )
        .await
        .expect("update browser session without replacing access state");

        let session =
            find_browser_document(&database, "browser_sessions", "browser_session_verified")
                .await
                .expect("load browser session");
        assert_eq!(
            session.get("tenant_id").and_then(Value::as_str),
            Some("tenant-verified")
        );
        assert_eq!(
            session.get("owner_user_id").and_then(Value::as_str),
            Some("verified-user@example.test")
        );
        assert_eq!(
            session.get("controller_user_id").and_then(Value::as_str),
            Some("verified-user@example.test")
        );
        assert_eq!(
            session.get("controller_lease_id").and_then(Value::as_str),
            Some("lease-verified")
        );
        assert_eq!(
            session
                .get("controller_lease_expires_at_ms")
                .and_then(Value::as_u64),
            Some(123_456)
        );
        assert_eq!(
            session
                .get("payload")
                .and_then(|payload| payload.get("source_id"))
                .and_then(Value::as_str),
            Some("example.test")
        );
        assert_eq!(
            session
                .get("payload")
                .and_then(|payload| payload.get("secret_value_in_rxdb"))
                .and_then(Value::as_bool),
            Some(false)
        );
        for (collection, id) in [
            ("browser_tabs", "browser_tab_verified"),
            ("browser_frames", "browser_frame_verified_1"),
        ] {
            let document = find_browser_document(&database, collection, id)
                .await
                .expect("load scoped browser projection");
            assert_eq!(
                document.get("tenant_id").and_then(Value::as_str),
                Some("tenant-verified")
            );
            assert_eq!(
                document.get("owner_user_id").and_then(Value::as_str),
                Some("verified-user@example.test")
            );
        }
    }

    #[test]
    fn browser_credential_resolution_is_role_scoped() {
        let bundled = r#"{
            "username": "user@example.test",
            "password": "not-a-real-password"
        }"#;
        assert_eq!(
            resolve_browser_credential_field(bundled, "username").unwrap(),
            "user@example.test"
        );
        assert_eq!(
            resolve_browser_credential_field(bundled, "password").unwrap(),
            "not-a-real-password"
        );
        assert!(resolve_browser_credential_field(bundled, "token").is_err());
        assert!(resolve_browser_credential_field(r#"{"username":"user"}"#, "password").is_err());
        assert_eq!(
            resolve_browser_credential_field("legacy-password", "password").unwrap(),
            "legacy-password"
        );
        assert!(resolve_browser_credential_field("legacy-user", "username").is_err());
        assert!(is_safe_browser_credential_name("DNB_HOOVERS_BROWSER_LOGIN"));
        assert!(!is_safe_browser_credential_name("../SECRET"));
        assert!(!is_safe_browser_credential_name("lowercase_secret"));
    }

    #[tokio::test]
    async fn browser_session_recovery_uses_indexed_active_query_without_fallback() {
        let root = tempfile::tempdir().expect("temp root");
        let database_path = root.path().join("browser-session-recovery.sqlite3");
        let database = open_test_database(database_path.clone())
            .await
            .expect("open test database");
        database
            .add_collections(HashMap::from([(
                "browser_sessions".to_string(),
                RxCollectionCreator {
                    schema: business_os_schema("browser_sessions", "id"),
                    conflict_handler: None,
                    options: HashMap::new(),
                },
            )]))
            .await
            .expect("add browser_sessions collection");
        let sessions = database
            .collection("browser_sessions")
            .expect("browser_sessions collection");
        for (id, status, runtime_status) in [
            ("active_stale", "active", "active"),
            ("stopped", "stopped", "stopped"),
            ("already_disconnected", "disconnected", "disconnected"),
            ("requested", "requested", "pending_command"),
            ("synthetic", "synthetic", "not_started"),
        ] {
            sessions
                .incremental_upsert(json!({
                    "id": id,
                    "owner_user_id": "ctox",
                    "controller_user_id": "ctox",
                    "status": status,
                    "runtime_status": runtime_status,
                    "current_tab_id": "tab_1",
                    "current_url": "https://example.com",
                    "title": id,
                    "viewport_w": 1280,
                    "viewport_h": 720,
                    "device_scale_factor": 1,
                    "frame_rate_target": 0,
                    "active_frame_id": "",
                    "last_frame_seq": 0,
                    "last_input_seq": 0,
                    "pending_input_count": 0,
                    "error": "",
                    "payload": {},
                    "created_at_ms": 1,
                    "updated_at_ms": 1,
                }))
                .await
                .expect("insert browser session");
        }

        let fallback_calls_before =
            rxdb::storage::sqlite::instance::sqlite_runtime_counters_snapshot()
                .get("query_fallback_calls")
                .and_then(Value::as_u64)
                .unwrap_or(0);
        recover_stale_browser_sessions(&database)
            .await
            .expect("recover stale browser sessions");
        let fallback_calls_after =
            rxdb::storage::sqlite::instance::sqlite_runtime_counters_snapshot()
                .get("query_fallback_calls")
                .and_then(Value::as_u64)
                .unwrap_or(0);
        assert_eq!(
            fallback_calls_after, fallback_calls_before,
            "browser session recovery must not use the unsupported Mango fallback"
        );

        let conn = Connection::open(&database_path).expect("open sqlite");
        let session_table = rxdb_test_table_name(&conn, "browser_sessions");
        let status_for = |id: &str| -> String {
            conn.query_row(
                &format!(
                    "SELECT json_extract(data, '$.status') FROM {} WHERE id = ?1",
                    sqlite_quote_identifier(&session_table)
                ),
                params![id],
                |row| row.get(0),
            )
            .expect("browser session status")
        };

        assert_eq!(status_for("active_stale"), "disconnected");
        assert_eq!(status_for("stopped"), "stopped");
        assert_eq!(status_for("already_disconnected"), "disconnected");
        assert_eq!(status_for("requested"), "requested");
        assert_eq!(status_for("synthetic"), "synthetic");
    }

    #[test]
    fn browser_frame_capture_breaks_the_cold_start_deadlock() {
        const NOW: u64 = 1_000_000;
        const RATE: u64 = BROWSER_FRAME_RATE_TARGET_DEFAULT; // 15 fps -> 67 ms

        // A session that never produced a frame is captured even though no
        // controller holds a lease. Without this the UI never shows a picture,
        // so nobody can ever take the lease: the deadlock that left every
        // session on the tenant frameless.
        assert!(browser_frame_capture_due(NOW, None, None, RATE, None));
        assert!(browser_frame_capture_due(
            NOW,
            None,
            Some(NOW - 1),
            RATE,
            None
        ));

        // With a frame in place the lease decides. Waehrend der Bedienung gilt
        // die volle Rate: nach 50 ms noch nicht faellig, nach 67 ms schon.
        assert!(!browser_frame_capture_due(
            NOW,
            Some(NOW - 50),
            Some(NOW + 60_000),
            RATE,
            Some(NOW)
        ));
        assert!(browser_frame_capture_due(
            NOW,
            Some(NOW - 67),
            Some(NOW + 60_000),
            RATE,
            Some(NOW)
        ));

        // Ohne Eingabe gilt der Ruhetakt: dieselben 67 ms reichen nicht mehr,
        // erst nach einer halben Sekunde wird wieder aufgenommen. Das ist der
        // Unterschied zwischen Fernsteuerung und Videofeed.
        assert!(!browser_frame_capture_due(
            NOW,
            Some(NOW - 67),
            Some(NOW + 60_000),
            RATE,
            None
        ));
        assert!(browser_frame_capture_due(
            NOW,
            Some(NOW - 500),
            Some(NOW + 60_000),
            RATE,
            None
        ));
        // Eine Eingabe, die laenger zurueckliegt als das Fenster, zaehlt nicht.
        assert!(!browser_frame_capture_due(
            NOW,
            Some(NOW - 67),
            Some(NOW + 60_000),
            RATE,
            Some(NOW - 5_000)
        ));

        // An expired lease stops the stream instead of screenshotting an
        // abandoned session forever.
        assert!(!browser_frame_capture_due(
            NOW,
            Some(NOW - 60_000),
            Some(NOW),
            RATE,
            Some(NOW)
        ));
        assert!(!browser_frame_capture_due(
            NOW,
            Some(NOW - 60_000),
            Some(NOW - 1),
            RATE,
            Some(NOW)
        ));

        // A session that records no lease at all keeps streaming: several
        // lifecycle writers never record one, and reading "absent" as "expired"
        // would end the stream after its very first frame.
        assert!(browser_frame_capture_due(
            NOW,
            Some(NOW - 67),
            None,
            RATE,
            Some(NOW)
        ));
        assert!(!browser_frame_capture_due(
            NOW,
            Some(NOW - 50),
            None,
            RATE,
            Some(NOW)
        ));

        // A stored rate of zero must never divide by zero or freeze the stream.
        assert!(browser_frame_capture_due(
            NOW,
            Some(NOW - 1_000),
            Some(NOW + 60_000),
            0,
            Some(NOW)
        ));
    }
}
