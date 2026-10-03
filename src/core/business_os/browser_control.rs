// Origin: CTOX
// License: Apache-2.0

//! RxDB-backed browser control plane.
//!
//! Owns browser session/tab/frame projections, command handling, input replay,
//! frame/input garbage collection, and stale-session recovery. Live Chromium
//! process ownership remains in `browser_runtime`.

use super::browser_runtime::{
    browser_runtime_manager, BrowserSessionAutomationRequest, LiveBrowserSession,
};
use super::policy::BusinessOsPermission;
use super::rxdb_peer::*;
use super::rxdb_peer_browser::{
    matching_active_browser_frame, wait_for_browser_frame_capture_slot,
};
use super::store;
use anyhow::Context;
use base64::Engine;
use rxdb::rx_database::RxDatabase;
use rxdb::types::MangoQuery;
use serde_json::{json, Value};
use sha2::Digest;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex as AsyncMutex;

pub(super) static BROWSER_RUNTIME_COMMAND_LOCK: tokio::sync::Mutex<()> =
    tokio::sync::Mutex::const_new(());

pub(super) const BROWSER_RUNTIME_ACTIVE_MAINTENANCE_INTERVAL_MS: u64 = 10;
pub(super) const BROWSER_RUNTIME_IDLE_MAINTENANCE_INTERVAL_SECS: u64 = 10;
pub(super) const BROWSER_RUNTIME_IDLE_BACKOFF_AFTER_TICKS: u32 = 1;
/// Hard upper bound for image generation. A watched session samples at this
/// rate; unchanged samples are discarded before an RxDB frame is written.
pub(super) const BROWSER_FRAME_RATE_TARGET_DEFAULT: u64 = 15;
pub(super) const BROWSER_FRAME_RATE_LIMIT: u64 = 15;
/// Bildrate ohne Interaktion. Fernsteuerung ist kein Videofeed: eine Seite,
/// die vor sich hin animiert, waehrend niemand etwas tut, braucht keine 15
/// Bilder pro Sekunde. Gemessen auf der Produktionsinstanz am 19.08.2026: acht
/// aktive Sitzungen erzeugten 120 Bildschirmfotos je Sekunde und hielten
/// ctox-real dauerhaft bei 150 % CPU, ohne dass jemand den Browser bediente.
pub(super) const BROWSER_FRAME_RATE_IDLE: u64 = 2;
/// Wie lange nach einer Eingabe die volle Bildrate gilt. Lang genug, dass das
/// Ergebnis eines Klicks fluessig sichtbar wird, kurz genug, dass eine
/// unbeaufsichtigte Sitzung sofort wieder in den Ruhetakt faellt.
pub(super) const BROWSER_INPUT_ACTIVE_WINDOW_MS: u64 = 1_500;
/// Ab wann eine Sitzung ohne Betrachter und ohne Regung als verwaist gilt und
/// keine Bilder mehr erzeugt.
pub(super) const BROWSER_SESSION_ABANDONED_AFTER_MS: u64 = 5 * 60 * 1_000;
pub(super) const BROWSER_FRAME_JPEG_QUALITY: u64 = 70;
pub(super) const BROWSER_FRAME_MAX_WIDTH: u64 = 1280;
pub(super) const BROWSER_FRAME_MAX_HEIGHT: u64 = 720;
pub(super) const BROWSER_FRAME_RECENT_KEEP_COUNT: usize = 2;
pub(super) const BROWSER_FRAME_GC_LIMIT: u64 = 256;

/// Aufbewahrungsfrist fuer erledigte `business_commands`. Die Collection hatte
/// bis 19.08.2026 gar keine Grenze; siehe gc_settled_business_commands.
pub(super) const BUSINESS_COMMAND_RETENTION_MS: u64 = 7 * 24 * 60 * 60 * 1000;
/// Obergrenze je Wartungsdurchlauf, damit die Raeumung die Schleife nicht blockiert.
pub(super) const BUSINESS_COMMAND_GC_LIMIT: u64 = 256;
pub(super) const BROWSER_INPUT_EVENT_GC_LIMIT: u64 = 512;
pub(super) const BROWSER_INPUT_EVENT_RETENTION_SECS: u64 = 60 * 60;

/// Bound a capture to the live-view envelope without changing the browser's
/// viewport (and therefore without invalidating input coordinates).
pub(super) fn browser_frame_capture_dimensions(viewport_w: u64, viewport_h: u64) -> (u64, u64) {
    let viewport_w = viewport_w.max(1);
    let viewport_h = viewport_h.max(1);
    if viewport_w <= BROWSER_FRAME_MAX_WIDTH && viewport_h <= BROWSER_FRAME_MAX_HEIGHT {
        return (viewport_w, viewport_h);
    }
    let height_at_max_width = viewport_h
        .saturating_mul(BROWSER_FRAME_MAX_WIDTH)
        .checked_div(viewport_w)
        .unwrap_or(1)
        .max(1);
    if height_at_max_width <= BROWSER_FRAME_MAX_HEIGHT {
        return (BROWSER_FRAME_MAX_WIDTH, height_at_max_width);
    }
    let width_at_max_height = viewport_w
        .saturating_mul(BROWSER_FRAME_MAX_HEIGHT)
        .checked_div(viewport_h)
        .unwrap_or(1)
        .max(1);
    (width_at_max_height, BROWSER_FRAME_MAX_HEIGHT)
}

/// Parameters understood by the persistent browser runner's screenshot
/// operation. The request/response channel does not expose unsolicited CDP
/// events, so `Page.startScreencast` and `screencastFrameAck` cannot be driven
/// through it; this bounded JPEG request is the producer-side fallback.
pub(super) fn browser_frame_capture_request(viewport_w: u64, viewport_h: u64) -> Value {
    let (max_width, max_height) = browser_frame_capture_dimensions(viewport_w, viewport_h);
    json!({
        "format": "jpeg",
        "quality": BROWSER_FRAME_JPEG_QUALITY,
        "maxWidth": max_width,
        "maxHeight": max_height,
        "everyNthFrame": 1,
    })
}

pub fn browser_session_status(root: &Path, session_id: &str) -> anyhow::Result<Value> {
    let session_id = session_id.trim().to_string();
    if session_id.is_empty() {
        anyhow::bail!("session_id is required");
    }
    with_business_os_database(
        root,
        "failed to create Business OS browser session status runtime",
        false,
        TemporaryDatabaseLockScope::TemporaryOnly,
        |_peer, database| async move {
            browser_session_status_with_database(&database, &session_id).await
        },
    )
}

#[derive(Debug, Clone)]
pub struct BrowserContextCaptureRequest {
    pub session_id: String,
    pub source_id: Option<String>,
    pub requesting_task_id: Option<String>,
    pub enqueue_handoff: bool,
}

pub fn browser_context_capture(
    root: &Path,
    request: BrowserContextCaptureRequest,
) -> anyhow::Result<Value> {
    let session_id = request.session_id.trim().to_string();
    if session_id.is_empty() {
        anyhow::bail!("session_id is required");
    }
    let mut outcome = with_business_os_database(
        root,
        "failed to create Business OS browser context capture runtime",
        false,
        TemporaryDatabaseLockScope::TemporaryOnly,
        |_peer, database| async move {
            browser_context_snapshot_with_database(&database, &session_id).await
        },
    )?;
    if request.enqueue_handoff {
        let now = now_ms() as u64;
        let command_id = format!("browser_context_handoff_{now}");
        let browser_context = outcome
            .get("browser_context")
            .cloned()
            .unwrap_or(Value::Null);
        let stored = enqueue_business_command_document(
            root,
            json!({
                "id": command_id,
                "command_id": command_id,
                "command_type": "ctox.browser_context.handoff",
                "type": "ctox.browser_context.handoff",
                "status": "pending_sync",
                "payload": {
                    "browser_context": browser_context,
                    "source_id": request.source_id,
                    "requesting_task_id": request.requesting_task_id,
                    "secret_value_in_payload": false
                },
                "created_at_ms": now,
                "updated_at_ms": now
            }),
        )?;
        if let Some(object) = outcome.as_object_mut() {
            object.insert("handoff_enqueued".to_string(), Value::Bool(true));
            object.insert(
                "handoff_command_id".to_string(),
                stored
                    .get("command_id")
                    .or_else(|| stored.get("id"))
                    .cloned()
                    .unwrap_or(Value::Null),
            );
        }
    }
    Ok(outcome)
}

pub fn browser_session_automation(
    root: &Path,
    request: BrowserSessionAutomationRequest,
) -> anyhow::Result<Value> {
    let session_id = request.session_id.trim().to_string();
    if session_id.is_empty() {
        anyhow::bail!("session_id is required for persistent browser automation");
    }
    if request.source.trim().is_empty() {
        anyhow::bail!("browser automation source is empty");
    }
    let database_path = store::rxdb_store_path(root);
    let root = root.to_path_buf();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create Business OS browser automation runtime")?;
    // Reuse the live peer only when it serves this root — the lifecycle
    // static is process-global (see NativePeer::serves_root).
    if let Some(peer) = current_peer().filter(|peer| peer.serves_root(&root)) {
        return runtime.block_on(async move {
            browser_session_automation_with_database(root, &peer.database, request).await
        });
    }
    let _database_guard = TEMPORARY_RXDB_DATABASE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    runtime.block_on(async move {
        let database = open_database(database_path).await?;
        register_collections_tolerant(&database, collection_creators()).await?;
        let session_id = request.session_id.trim().to_string();
        let output = browser_session_automation_with_database(root, &database, request).await;
        browser_runtime_manager().stop(&session_id).await;
        // This CLI fallback is short-lived. Awaiting RxDB close here can leave
        // browser-automation commands stuck after evidence is already produced,
        // which blocks deployment-audit task execution. Let process teardown
        // reclaim the temporary handle instead of making CLI completion depend
        // on close liveness.
        output
    })
}

pub(super) async fn browser_session_status_with_database(
    database: &Arc<RxDatabase>,
    session_id: &str,
) -> anyhow::Result<Value> {
    let sessions = database
        .collection("browser_sessions")
        .context("browser_sessions collection is not registered")?;
    let session = sessions
        .storage_instance
        .find_documents_by_id(&[session_id.to_string()], false)
        .await
        .map_err(|err| anyhow::anyhow!("load browser session {session_id}: {err}"))?
        .into_iter()
        .next()
        .with_context(|| format!("browser session not found: {session_id}"))?;
    Ok(redacted_browser_session_status(&session))
}

pub(super) async fn browser_context_snapshot_with_database(
    database: &Arc<RxDatabase>,
    session_id: &str,
) -> anyhow::Result<Value> {
    let sessions = database
        .collection("browser_sessions")
        .context("browser_sessions collection is not registered")?;
    let session = sessions
        .storage_instance
        .find_documents_by_id(&[session_id.to_string()], false)
        .await
        .map_err(|err| anyhow::anyhow!("load browser session {session_id}: {err}"))?
        .into_iter()
        .next()
        .with_context(|| format!("browser session not found: {session_id}"))?;
    let tab = browser_context_related_document(database, "browser_tabs", "session_id", session_id)
        .await?;
    let frame =
        browser_context_related_document(database, "browser_frames", "session_id", session_id)
            .await?;
    Ok(redacted_browser_context_capture(
        &session,
        tab.as_ref(),
        frame.as_ref(),
    ))
}

pub(super) async fn browser_session_automation_with_database(
    root: PathBuf,
    database: &Arc<RxDatabase>,
    request: BrowserSessionAutomationRequest,
) -> anyhow::Result<Value> {
    let session_id = request.session_id.trim().to_string();
    anyhow::ensure!(
        !session_id.is_empty(),
        "session_id is required for persistent browser automation"
    );
    let source = request.source.trim().to_string();
    anyhow::ensure!(!source.is_empty(), "browser automation source is empty");
    let timeout_ms = request.timeout_ms.unwrap_or(30_000).clamp(1_000, 300_000);
    let command_created_at_ms = now_ms() as u64;
    let profile_owner = request
        .profile_owner
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("ctox")
        .to_string();
    let manager = browser_runtime_manager();
    let session = manager
        .ensure_session(
            root,
            request.dir,
            &session_id,
            1920,
            947,
            &profile_owner,
            false,
            false,
        )
        .await?;
    let mut output = manager
        .request(
            &session,
            "automation",
            json!({
                "source": source,
                "timeoutMs": timeout_ms,
            }),
        )
        .await?;

    let session_doc = find_browser_document(database, "browser_sessions", &session_id).await?;
    let tab_id = session_doc
        .get("current_tab_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("browser_tab_{session_id}"));
    let nav = output.get("nav").cloned().unwrap_or(Value::Null);
    let page_meta = output.get("page").cloned().unwrap_or(Value::Null);
    let url = nav
        .get("url")
        .and_then(Value::as_str)
        .or_else(|| page_meta.get("url").and_then(Value::as_str))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("about:blank")
        .to_string();
    let title = nav
        .get("title")
        .and_then(Value::as_str)
        .or_else(|| page_meta.get("title").and_then(Value::as_str))
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
    let mut next_seq = session_doc
        .get("last_frame_seq")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        + 1;
    wait_for_browser_frame_capture_slot(database, &session_id).await?;
    let screenshot = manager
        .request(
            &session,
            "screenshot",
            browser_frame_capture_request(session.viewport_w, session.viewport_h),
        )
        .await?;
    let data = screenshot
        .get("screenshot")
        .and_then(|frame| frame.get("base64"))
        .and_then(Value::as_str)
        .context("browser runtime did not return screenshot data after automation")?
        .to_string();
    let mime_type = screenshot
        .get("screenshot")
        .and_then(|frame| frame.get("mimeType"))
        .and_then(Value::as_str)
        .unwrap_or("image/jpeg")
        .to_string();
    let mut frame_id = format!("browser_frame_{}_{}", session_id, next_seq);
    let frame_hash = browser_frame_hash(&data);
    let size_bytes = base64::engine::general_purpose::STANDARD
        .decode(data.as_bytes())
        .map(|bytes| bytes.len() as u64)
        .unwrap_or_else(|_| data.len() as u64);
    let matching_frame =
        matching_active_browser_frame(database, &session_doc, Some(&frame_hash)).await?;
    if let Some(frame) = matching_frame.as_ref() {
        next_seq = frame.get("seq").and_then(Value::as_u64).unwrap_or(next_seq);
        frame_id = frame
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(&frame_id)
            .to_string();
    } else {
        let (frame_width, frame_height) =
            browser_frame_capture_dimensions(session.viewport_w, session.viewport_h);
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
            None,
        )
        .await?;
    }
    upsert_browser_tab(
        database,
        &tab_id,
        &session_id,
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
        &session_id,
        &tab_id,
        "active",
        "active",
        &url,
        &title,
        session.viewport_w,
        session.viewport_h,
        Some(&frame_id),
        next_seq,
        "browser.automation",
        command_created_at_ms,
        output.get("error").and_then(Value::as_str),
        None,
        None,
    )
    .await?;
    if let Some(object) = output.as_object_mut() {
        object.insert("session_id".to_string(), Value::String(session_id));
        object.insert("tab_id".to_string(), Value::String(tab_id));
        object.insert("frame_id".to_string(), Value::String(frame_id));
        object.insert("frame_hash".to_string(), Value::String(frame_hash));
        object.insert("size_bytes".to_string(), Value::from(size_bytes));
        object.insert(
            "frame_changed".to_string(),
            Value::Bool(matching_frame.is_none()),
        );
        object.insert(
            "browser_stream".to_string(),
            Value::String("rxdb".to_string()),
        );
        object.insert("timeout_ms".to_string(), Value::from(timeout_ms));
    }
    Ok(output)
}

pub(super) async fn browser_context_related_document(
    database: &Arc<RxDatabase>,
    collection: &str,
    field: &str,
    value: &str,
) -> anyhow::Result<Option<Value>> {
    let collection = database
        .collection(collection)
        .with_context(|| format!("{collection} collection is not registered"))?;
    let document = collection
        .find_one(Some(MangoQuery {
            selector: Some(json!({ field: { "$eq": value } })),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query browser context document: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec browser context document query: {err}"))?;
    Ok(document.is_object().then_some(document))
}

pub(super) fn redacted_browser_session_status(session: &Value) -> Value {
    let payload = session.get("payload").unwrap_or(&Value::Null);
    json!({
        "ok": true,
        "session": {
            "id": session.get("id").and_then(Value::as_str).unwrap_or_default(),
            "status": session.get("status").and_then(Value::as_str).unwrap_or_default(),
            "runtime_status": session.get("runtime_status").and_then(Value::as_str).unwrap_or_default(),
            "current_url": session.get("current_url").and_then(Value::as_str).unwrap_or_default(),
            "updated_at_ms": session.get("updated_at_ms").and_then(Value::as_u64).unwrap_or_default(),
            "payload": {
                "source_id": payload.get("source_id").and_then(Value::as_str).unwrap_or_default(),
                "capture_extract_result": payload.get("capture_extract_result").cloned().unwrap_or(Value::Null),
                "secret_value_in_rxdb": payload.get("secret_value_in_rxdb").and_then(Value::as_bool).unwrap_or(false),
                "browser_stream": payload.get("browser_stream").and_then(Value::as_str).unwrap_or("rxdb")
            }
        }
    })
}

pub(super) fn redacted_browser_context_capture(
    session: &Value,
    tab: Option<&Value>,
    frame: Option<&Value>,
) -> Value {
    let browser_context = json!({
        "session": redacted_browser_session_status(session).get("session").cloned().unwrap_or(Value::Null),
        "tab": tab.cloned().unwrap_or(Value::Null),
        "frame": frame.map(redact_browser_frame_data).unwrap_or(Value::Null),
    });
    json!({
        "ok": true,
        "browser_stream": "rxdb",
        "browser_context": browser_context,
        "captured_at_ms": now_ms() as u64,
    })
}

pub(super) fn redact_browser_frame_data(frame: &Value) -> Value {
    let mut frame = frame.clone();
    if let Some(object) = frame.as_object_mut() {
        object.remove("data");
        object.remove("content");
        object.remove("secret");
    }
    frame
}
