// Origin: CTOX
// License: AGPL-3.0-only
//! §16.44 Coding agent in source mode: one bounded pi turn on the caller's own
//! source draft (`app_drafts`), never on an active app version.
//!
//! Contract (ctox sync v4 architecture §16.44, §16.39 Nr. 3, §16.42):
//! - `ctox.coding.turn {app_id, draft_id, base_draft_rev, prompt, model?, turn_id?}`
//!   works only on the draft of the triggering user for that app; right
//!   `apps.source.edit`; missing draft_id/base_draft_rev → `rejected{invalid}`.
//! - The sidecar receives the draft's text files as its isolated working tree.
//!   The result is written back as ONE new draft revision with compare-and-set
//!   on `base_draft_rev`. If the user edited meanwhile, the turn ends in
//!   `conflict`; its changes are kept as a patch (`files` record, §16.32) and the
//!   draft is not touched.
//! - At most one queued/running turn per draft (`rejected{conflict, reason:
//!   'turn_running'}`); `ctox.coding.cancel {turn_id}` kills the sidecar.
//! - Progress lives in the platform collection `coding_turns/<turn_id>`
//!   (throttled ≤ 4 writes/s, retention 30 days).
//! - The result is sent at most once as a callback command to the triggering app
//!   when its command carried `on_complete`/`on_failed`.
//!
//! The canonical store owner injects its WriterHandle (same pattern as the app
//! platform owner); this module never opens a database. Draft file content is
//! resolved through the injected content-addressed store ([`DraftContent`]); the
//! sidecar itself is reached through [`SidecarRunner`] (production:
//! `pi_sidecar::PiSidecarRunner`, one fresh daemon per turn over LocalTransport).

use crate::business_os::policy::{self, BusinessOsActor, BusinessOsPermission, BusinessOsScope};
use ctox_store::{
    schema,
    types::{JobOptions, StoreError, WriteJob},
    writer::WriterHandle,
};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(crate) const COMMAND_TURN: &str = "ctox.coding.turn";
pub(crate) const COMMAND_CANCEL: &str = "ctox.coding.cancel";
pub(crate) const COLLECTION_TURNS: &str = "coding_turns";
pub(crate) const COLLECTION_DRAFTS: &str = "app_drafts";
/// §16.44: retention of finished turns.
pub(crate) const TURN_RETENTION_MS: u64 = 30 * 24 * 60 * 60 * 1000;
/// §16.44: at most 4 progress writes per second per turn.
pub(crate) const PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(250);
const MAX_MESSAGES: usize = 200;
const MAX_MESSAGE_TEXT: usize = 16 * 1024;
const MAX_TOOL_EVENTS: usize = 500;
const MAX_PROMPT_BYTES: usize = 64 * 1024;

pub(crate) fn handles(kind: &str) -> bool {
    matches!(kind, COMMAND_TURN | COMMAND_CANCEL)
}

/// The business command as admitted by the command plane. The calling app comes
/// from the command document (never the payload); callbacks are command-document
/// options of `ctx.command(type, payload, {on_complete, on_failed})` (§16.39 Nr. 3).
#[derive(Clone, Debug)]
pub(crate) struct CommandDocument {
    pub id: String,
    pub module: String,
    pub command_type: String,
    pub payload: Value,
    pub on_complete: Option<String>,
    pub on_failed: Option<String>,
}

/// Content-addressed draft content (`files.ref` = sha256, §16.32). Implemented by
/// the Core CAS owner; never a second CAS.
pub(crate) trait DraftContent: Send + Sync {
    fn get(&self, sha256: &str) -> Result<Vec<u8>, String>;
    /// Stores bytes and returns their lowercase hex sha256 (idempotent).
    fn put(&self, bytes: &[u8]) -> Result<String, String>;
}

/// What the sidecar gets: an isolated working tree of the draft's text files.
#[derive(Clone, Debug)]
pub(crate) struct SidecarTurnInput {
    pub turn_id: String,
    pub app_id: String,
    pub prompt: String,
    pub model_preset: Option<String>,
    pub files: BTreeMap<String, String>,
}

/// Typed turn failure (§16.42); `detail` is the raw message without secrets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TurnFailure {
    pub code: &'static str,
    pub reason: String,
    pub detail: Option<String>,
}
impl TurnFailure {
    pub(crate) fn new(code: &'static str, reason: impl Into<String>) -> Self {
        Self {
            code,
            reason: reason.into(),
            detail: None,
        }
    }
    pub(crate) fn detail(mut self, detail: impl Into<String>) -> Self {
        let mut detail = detail.into();
        detail.truncate(2048);
        self.detail = Some(detail);
        self
    }
    fn to_value(&self) -> Value {
        let mut value = json!({"status":"failed","code":self.code,"reason":self.reason});
        if let Some(detail) = &self.detail {
            value["detail"] = Value::String(detail.clone());
        }
        value
    }
}

/// Runs one bounded sidecar turn and returns the raw `CtoxTurnResponse`. The
/// runner must hand every spawned process to `control` (kill on cancel/drop)
/// and may report liveness through `progress` (throttled by the engine).
pub(crate) trait SidecarRunner: Send + Sync {
    fn run(
        &self,
        input: &SidecarTurnInput,
        control: &TurnControl,
        progress: &dyn Fn(),
    ) -> Result<Value, TurnFailure>;
}

/// Cancellation handle of one running turn. Owns the sidecar process: cancel
/// and drop of the guard kill it, so a turn can never leak a live agent.
#[derive(Clone, Default)]
pub(crate) struct TurnControl(Arc<ControlInner>);
#[derive(Default)]
struct ControlInner {
    cancelled: AtomicBool,
    child: Mutex<Option<Child>>,
}
/// Kills and reaps the attached sidecar when the turn ends for any reason.
pub(crate) struct ChildGuard(TurnControl);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.0.kill_child();
    }
}
impl TurnControl {
    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }
    /// Attach the freshly spawned sidecar. A turn cancelled before the spawn
    /// completed kills the new process immediately.
    pub(crate) fn attach(&self, child: Child) -> Result<ChildGuard, TurnFailure> {
        *self.0.child.lock().unwrap_or_else(|e| e.into_inner()) = Some(child);
        let guard = ChildGuard(self.clone());
        if self.is_cancelled() {
            drop(guard);
            return Err(TurnFailure::new("conflict", "turn_cancelled"));
        }
        Ok(guard)
    }
    pub(crate) fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::Release);
        self.kill_child();
    }
    fn kill_child(&self) {
        let child = self
            .0
            .child
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(mut child) = child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[derive(Clone)]
pub(crate) struct CodingPanel {
    writer: WriterHandle,
    content: Arc<dyn DraftContent>,
    runner: Arc<dyn SidecarRunner>,
    running: Arc<Mutex<HashMap<String, TurnControl>>>,
}

/// Admission outcome of `ctox.coding.turn`. `worker` drives the bounded turn to
/// its terminal state; the command plane runs it on its blocking command worker
/// (tests join it). It is `None` for rejections and replays.
pub(crate) struct Admitted {
    pub result: Value,
    pub worker: Option<std::thread::JoinHandle<Value>>,
}

fn rejected(code: &str, reason: &str) -> Value {
    json!({"status":"rejected","code":code,"reason":reason})
}
fn store_invalid(message: impl Into<String>) -> StoreError {
    StoreError::Invalid(message.into())
}
fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
fn table(tx: &Transaction<'_>, name: &str) -> Result<(String, u32), StoreError> {
    let collection = schema::active_collection(tx, name)?
        .ok_or_else(|| store_invalid(format!("platform collection not initialized: {name}")))?;
    Ok((quoted(&collection.table), collection.definition.schema_ver))
}
fn get_with_rev(
    tx: &Transaction<'_>,
    collection: &str,
    id: &str,
) -> Result<Option<(String, Value)>, StoreError> {
    let (table, _) = table(tx, collection)?;
    let row: Option<(String, String)> = tx
        .query_row(
            &format!("SELECT rev,data FROM {table} WHERE id=?1 AND deleted=0"),
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    row.map(|(rev, raw)| {
        serde_json::from_str(&raw)
            .map(|body| (rev, body))
            .map_err(|e| store_invalid(e.to_string()))
    })
    .transpose()
}
fn get(tx: &Transaction<'_>, collection: &str, id: &str) -> Result<Option<Value>, StoreError> {
    Ok(get_with_rev(tx, collection, id)?.map(|(_, body)| body))
}
fn next_rev(base: &str, body: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(base.as_bytes());
    hash.update([0]);
    hash.update(body.as_bytes());
    format!("native:{:x}", hash.finalize())
}
fn put(
    tx: &Transaction<'_>,
    collection: &str,
    id: &str,
    body: &Value,
) -> Result<String, StoreError> {
    let (table, schema_ver) = table(tx, collection)?;
    let base: Option<String> = tx
        .query_row(&format!("SELECT rev FROM {table} WHERE id=?1"), [id], |r| {
            r.get(0)
        })
        .optional()?;
    let body = serde_json::to_string(body).map_err(|e| store_invalid(e.to_string()))?;
    let rev = next_rev(base.as_deref().unwrap_or(""), &body);
    tx.execute(
        &format!(
            "INSERT INTO {table}(id,rev,schema_ver,data,deleted) VALUES(?1,?2,?3,?4,0)
             ON CONFLICT(id) DO UPDATE SET rev=excluded.rev,schema_ver=excluded.schema_ver,
             data=excluded.data,deleted=0"
        ),
        params![id, rev, schema_ver, body],
    )?;
    Ok(rev)
}
/// Insert only if absent; returns whether the row was created.
fn insert_new(
    tx: &Transaction<'_>,
    collection: &str,
    id: &str,
    body: &Value,
) -> Result<bool, StoreError> {
    let (table, schema_ver) = table(tx, collection)?;
    let body = serde_json::to_string(body).map_err(|e| store_invalid(e.to_string()))?;
    let rev = next_rev("", &body);
    Ok(tx.execute(
        &format!(
            "INSERT INTO {table}(id,rev,schema_ver,data,deleted) VALUES(?1,?2,?3,?4,0)
             ON CONFLICT(id) DO NOTHING"
        ),
        params![id, rev, schema_ver, body],
    )? == 1)
}
/// Compare-and-set on the document revision. Returns the new revision, or
/// `None` when the current revision is not `base_rev` (nothing written).
fn put_if_rev(
    tx: &Transaction<'_>,
    collection: &str,
    id: &str,
    base_rev: &str,
    body: &Value,
) -> Result<Option<String>, StoreError> {
    let (table, schema_ver) = table(tx, collection)?;
    let body = serde_json::to_string(body).map_err(|e| store_invalid(e.to_string()))?;
    let rev = next_rev(base_rev, &body);
    let changed = tx.execute(
        &format!(
            "UPDATE {table} SET rev=?1,schema_ver=?2,data=?3
             WHERE id=?4 AND rev=?5 AND deleted=0"
        ),
        params![rev, schema_ver, body, id, base_rev],
    )?;
    Ok((changed == 1).then_some(rev))
}

fn valid_id(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':'))
}
fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 512
        && !path.contains('\\')
        && !path.contains('\0')
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

#[derive(Clone, Debug)]
struct TurnRequest {
    app_id: String,
    draft_id: String,
    base_draft_rev: String,
    prompt: String,
    model: Option<String>,
    turn_id: Option<String>,
}
fn parse_turn(payload: &Value) -> Result<TurnRequest, Value> {
    let object = payload
        .as_object()
        .ok_or_else(|| rejected("invalid", "payload_invalid"))?;
    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "app_id" | "draft_id" | "base_draft_rev" | "prompt" | "model" | "turn_id"
        ) {
            return Err(rejected("invalid", &format!("payload_invalid:{key}")));
        }
    }
    let text = |name: &str| -> Result<String, Value> {
        object
            .get(name)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
            .ok_or_else(|| rejected("invalid", &format!("payload_invalid:{name}")))
    };
    let optional = |name: &str| -> Result<Option<String>, Value> {
        match object.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(value)) if !value.trim().is_empty() => Ok(Some(value.clone())),
            Some(_) => Err(rejected("invalid", &format!("payload_invalid:{name}"))),
        }
    };
    let request = TurnRequest {
        app_id: text("app_id")?,
        draft_id: text("draft_id")?,
        base_draft_rev: text("base_draft_rev")?,
        prompt: text("prompt")?,
        model: optional("model")?,
        turn_id: optional("turn_id")?,
    };
    if !valid_id(&request.app_id, 128) {
        return Err(rejected("invalid", "payload_invalid:app_id"));
    }
    if request.prompt.len() > MAX_PROMPT_BYTES {
        return Err(rejected("invalid", "payload_invalid:prompt"));
    }
    if request
        .model
        .as_deref()
        .is_some_and(|model| !valid_id(model, 128))
    {
        return Err(rejected("invalid", "payload_invalid:model"));
    }
    if request
        .turn_id
        .as_deref()
        .is_some_and(|turn| !valid_id(turn, 128))
    {
        return Err(rejected("invalid", "payload_invalid:turn_id"));
    }
    Ok(request)
}

fn may_edit_source(
    tx: &Transaction<'_>,
    actor: &BusinessOsActor,
    app_id: &str,
) -> Result<bool, StoreError> {
    let app = match schema::active_collection(tx, "business_apps")? {
        Some(_) => get(tx, "business_apps", app_id)?,
        None => None,
    };
    let assigned = actor.id.as_deref().is_some_and(|id| {
        app.as_ref()
            .and_then(|a| a.pointer("/provenance/owner_id"))
            .and_then(Value::as_str)
            == Some(id)
    });
    Ok(policy::evaluate(
        actor,
        BusinessOsPermission::AppsSourceEdit,
        &BusinessOsScope::module(app_id, assigned),
    )
    .allowed)
}

fn command_status(result: &Value) -> &'static str {
    match result.get("status").and_then(Value::as_str) {
        Some("rejected") | Some("failed") => "failed",
        Some("running") | Some("queued") => "running",
        _ => "completed",
    }
}
fn record_command(
    tx: &Transaction<'_>,
    actor_id: &str,
    command: &CommandDocument,
    result: &Value,
    now: u64,
) -> Result<(), StoreError> {
    if schema::active_collection(tx, "business_commands")?.is_none() {
        return Ok(());
    }
    let mut body = get(tx, "business_commands", &command.id)?.unwrap_or_else(|| {
        json!({
            "id":command.id,"command_id":command.id,"module":command.module,
            "command_type":command.command_type,"payload":command.payload,
            "client_context":{"actor":{"id":actor_id}},
        })
    });
    body["status"] = Value::String(command_status(result).into());
    body["result"] = result.clone();
    body["updated_at_ms"] = json!(now);
    put(tx, "business_commands", &command.id, &body)?;
    Ok(())
}

/// The command result of a turn in its terminal state.
fn terminal_result(turn: &Value) -> Value {
    let state = turn
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("failed");
    let mut result = json!({
        "turn_id": turn["id"], "state": state,
        "draft_rev_after": turn.get("draft_rev_after").cloned().unwrap_or(Value::Null),
        "changed_files": turn.get("changed_files").cloned().unwrap_or_else(|| json!([])),
    });
    if let Some(patch) = turn.get("patch") {
        result["patch"] = patch.clone();
    }
    match state {
        "done" => result["status"] = json!("completed"),
        "conflict" => {
            result["status"] = json!("failed");
            result["code"] = json!("conflict");
            result["reason"] = json!("draft_changed");
        }
        "cancelled" => {
            result["status"] = json!("failed");
            result["code"] = json!("conflict");
            result["reason"] = json!("cancelled");
        }
        _ => {
            let error = turn.get("error").cloned().unwrap_or_else(
                || json!({"status":"failed","code":"unavailable","reason":"turn_failed"}),
            );
            result["status"] = json!("failed");
            for key in ["code", "reason", "path", "detail"] {
                if let Some(value) = error.get(key) {
                    result[key] = value.clone();
                }
            }
        }
    }
    result
}

/// §16.39 Nr. 3 / §16.30 Nr. 3: exactly one callback command to the triggering
/// app per command_id. The deterministic document id makes redelivery a no-op.
fn emit_callback_once(tx: &Transaction<'_>, turn: &mut Value, now: u64) -> Result<(), StoreError> {
    if turn
        .get("callback_command_id")
        .is_some_and(|v| !v.is_null())
    {
        return Ok(());
    }
    let state = turn
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("failed");
    let key = if state == "done" {
        "on_complete"
    } else {
        "on_failed"
    };
    let Some(callback_type) = turn
        .pointer(&format!("/callback/{key}"))
        .and_then(Value::as_str)
        .map(str::to_owned)
    else {
        return Ok(());
    };
    let command_id = turn["command_id"].as_str().unwrap_or_default().to_owned();
    let app = turn
        .get("origin_app")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if command_id.is_empty()
        || app.is_empty()
        || schema::active_collection(tx, "business_commands")?.is_none()
    {
        return Ok(());
    }
    let id = format!("coding-callback:{command_id}");
    insert_new(
        tx,
        "business_commands",
        &id,
        &json!({
            "id": id, "command_id": id, "module": app, "command_type": callback_type,
            "status": "queued", "updated_at_ms": now,
            "payload": {"command_id": command_id, "turn_id": turn["id"],
                        "status": state, "result": terminal_result(turn)},
            "client_context": {"actor": {"id": "core"}},
        }),
    )?;
    turn["callback_command_id"] = Value::String(id);
    Ok(())
}

/// Persist a terminal state exactly once: returns false (and writes nothing)
/// when the turn already left `queued|running` (e.g. cancelled meanwhile).
fn finish_turn(
    tx: &Transaction<'_>,
    turn_id: &str,
    apply: impl FnOnce(&mut Value),
    now: u64,
) -> Result<Option<Value>, StoreError> {
    let Some(mut turn) = get(tx, COLLECTION_TURNS, turn_id)? else {
        return Ok(None);
    };
    if !matches!(
        turn.get("state").and_then(Value::as_str),
        Some("queued" | "running")
    ) {
        return Ok(None);
    }
    apply(&mut turn);
    turn["updated_at_ms"] = json!(now);
    turn["finished_at_ms"] = json!(now);
    emit_callback_once(tx, &mut turn, now)?;
    put(tx, COLLECTION_TURNS, turn_id, &turn)?;
    let command = CommandDocument {
        id: turn["command_id"].as_str().unwrap_or_default().to_owned(),
        module: turn
            .get("origin_app")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        command_type: COMMAND_TURN.into(),
        payload: Value::Null,
        on_complete: None,
        on_failed: None,
    };
    if !command.id.is_empty() {
        let actor = turn["user_id"].as_str().unwrap_or_default().to_owned();
        record_command(tx, &actor, &command, &terminal_result(&turn), now)?;
    }
    Ok(Some(turn))
}

enum Admission {
    Done(Value),
    Start {
        result: Value,
        input: SidecarTurnInput,
        files: Map<String, Value>,
        draft_id: String,
        base_rev: String,
    },
}

fn admit_turn(
    tx: &Transaction<'_>,
    actor: &BusinessOsActor,
    command: &CommandDocument,
    turn_id_default: String,
    now: u64,
) -> Result<Admission, StoreError> {
    let actor_id = actor.id.clone().unwrap_or_default();
    let done = |result: Value| -> Result<Admission, StoreError> {
        record_command(tx, &actor_id, command, &result, now)?;
        Ok(Admission::Done(result))
    };
    let request = match parse_turn(&command.payload) {
        Ok(request) => request,
        Err(result) => return done(result),
    };
    if actor_id.is_empty() || !may_edit_source(tx, actor, &request.app_id)? {
        return done(rejected("forbidden", "apps.source.edit"));
    }
    let Some((current_rev, draft)) = get_with_rev(tx, COLLECTION_DRAFTS, &request.draft_id)? else {
        return done(rejected("not_found", "draft_not_found"));
    };
    // Only the triggering user's own draft of exactly this app (§16.44).
    if draft.get("user_id").and_then(Value::as_str) != Some(actor_id.as_str())
        || draft.get("app_id").and_then(Value::as_str) != Some(request.app_id.as_str())
    {
        return done(rejected("forbidden", "draft_not_owned"));
    }
    let turn_id = request.turn_id.clone().unwrap_or(turn_id_default);
    if let Some(existing) = get(tx, COLLECTION_TURNS, &turn_id)? {
        // Redelivery of the same command is a replay, never a second turn.
        if existing.get("command_id").and_then(Value::as_str) == Some(command.id.as_str()) {
            let state = existing["state"].as_str().unwrap_or("failed");
            let result = if matches!(state, "queued" | "running") {
                json!({"status":"running","turn_id":turn_id,"state":state})
            } else {
                terminal_result(&existing)
            };
            return Ok(Admission::Done(result));
        }
        return done(rejected("conflict", "turn_id_taken"));
    }
    if running_turn_for_draft(tx, &request.draft_id)?.is_some() {
        return done(rejected("conflict", "turn_running"));
    }
    if current_rev != request.base_draft_rev {
        return done(rejected("conflict", "stale_draft_rev"));
    }
    let files = draft
        .get("files")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let callback = (command.on_complete.is_some() || command.on_failed.is_some())
        .then(|| json!({"on_complete":command.on_complete,"on_failed":command.on_failed}));
    let turn = json!({
        "id": turn_id, "app_id": request.app_id, "draft_id": request.draft_id,
        "user_id": actor_id, "command_id": command.id, "origin_app": command.module,
        "base_draft_rev": current_rev,
        "state": "queued", "messages": [{"role":"user","text":truncate(&request.prompt),"at_ms":now}],
        "tool_events": [], "created_at_ms": now, "updated_at_ms": now,
        "callback": callback.unwrap_or(Value::Null),
    });
    put(tx, COLLECTION_TURNS, &turn_id, &turn)?;
    let result = json!({"status":"running","turn_id":turn_id,"state":"queued"});
    record_command(tx, &actor_id, command, &result, now)?;
    Ok(Admission::Start {
        result,
        input: SidecarTurnInput {
            turn_id,
            app_id: request.app_id.clone(),
            prompt: request.prompt,
            model_preset: request.model,
            files: BTreeMap::new(),
        },
        files,
        draft_id: request.draft_id,
        base_rev: current_rev,
    })
}

fn running_turn_for_draft(
    tx: &Transaction<'_>,
    draft_id: &str,
) -> Result<Option<String>, StoreError> {
    let (table, _) = table(tx, COLLECTION_TURNS)?;
    Ok(tx
        .query_row(
            &format!(
                "SELECT id FROM {table} WHERE deleted=0
                 AND json_extract(data,'$.draft_id')=?1
                 AND json_extract(data,'$.state') IN ('queued','running') LIMIT 1"
            ),
            [draft_id],
            |r| r.get(0),
        )
        .optional()?)
}

fn truncate(text: &str) -> String {
    if text.len() <= MAX_MESSAGE_TEXT {
        return text.to_owned();
    }
    let mut end = MAX_MESSAGE_TEXT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

/// Panel projection of the sidecar transcript: `{role, text, at_ms}`.
fn project_messages(response: &Value, now: u64) -> (Vec<Value>, Vec<Value>) {
    let mut messages = Vec::new();
    let mut tools = Vec::new();
    for message in response
        .get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let role = match message.get("role").and_then(Value::as_str) {
            Some("user") => "user",
            Some("assistant") => "assistant",
            Some("toolResult") => "tool",
            _ => continue,
        };
        let at_ms = message
            .get("timestamp")
            .and_then(Value::as_u64)
            .unwrap_or(now);
        let mut text = Vec::new();
        match message.get("content") {
            Some(Value::String(content)) => text.push(content.clone()),
            Some(Value::Array(parts)) => {
                for part in parts {
                    match part.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            if let Some(value) = part.get("text").and_then(Value::as_str) {
                                text.push(value.to_owned());
                            }
                        }
                        Some("toolCall") if tools.len() < MAX_TOOL_EVENTS => {
                            let mut event = json!({
                                "kind": part.get("name").and_then(Value::as_str).unwrap_or("tool"),
                                "at_ms": at_ms,
                            });
                            if let Some(path) =
                                part.pointer("/arguments/path").and_then(Value::as_str)
                            {
                                let path = path.strip_prefix("/workspace/").unwrap_or(path);
                                event["path"] = Value::String(truncate(path));
                            }
                            tools.push(event);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        // The user prompt is already recorded at admission; tool results are
        // represented by tool_events, not echoed into the transcript.
        if role == "user" || role == "tool" || text.is_empty() {
            continue;
        }
        if messages.len() < MAX_MESSAGES {
            messages.push(json!({"role":role,"text":truncate(&text.join("\n")),"at_ms":at_ms}));
        }
    }
    (messages, tools)
}

/// Unified diff with one zero-context hunk per file (apply with
/// `git apply --unidiff-zero`). Old/new are full file texts or None.
pub(crate) fn unified_patch(changes: &[(String, Option<String>, Option<String>)]) -> String {
    let mut out = String::new();
    for (path, before, after) in changes {
        let old: Vec<&str> = before
            .as_deref()
            .map(|t| t.lines().collect())
            .unwrap_or_default();
        let new: Vec<&str> = after
            .as_deref()
            .map(|t| t.lines().collect())
            .unwrap_or_default();
        let mut prefix = 0;
        while prefix < old.len() && prefix < new.len() && old[prefix] == new[prefix] {
            prefix += 1;
        }
        let mut suffix = 0;
        while suffix < old.len() - prefix
            && suffix < new.len() - prefix
            && old[old.len() - 1 - suffix] == new[new.len() - 1 - suffix]
        {
            suffix += 1;
        }
        let removed = &old[prefix..old.len() - suffix];
        let added = &new[prefix..new.len() - suffix];
        out.push_str(&format!(
            "--- {}\n+++ {}\n",
            if before.is_some() {
                format!("a/{path}")
            } else {
                "/dev/null".into()
            },
            if after.is_some() {
                format!("b/{path}")
            } else {
                "/dev/null".into()
            },
        ));
        let range = |start: usize, len: usize| {
            if len == 0 {
                format!("{start},0")
            } else {
                format!("{},{len}", start + 1)
            }
        };
        out.push_str(&format!(
            "@@ -{} +{} @@\n",
            range(prefix, removed.len()),
            range(prefix, added.len())
        ));
        for line in removed {
            out.push_str(&format!("-{line}\n"));
        }
        for line in added {
            out.push_str(&format!("+{line}\n"));
        }
    }
    out
}

/// One change of the turn: (path, sha256 after or None = deleted).
struct TurnChanges {
    files: Vec<(String, Option<String>)>,
    patch: String,
}

fn diff_snapshot(
    baseline: &BTreeMap<String, String>,
    response: &Value,
    content: &dyn DraftContent,
) -> Result<TurnChanges, TurnFailure> {
    let mut snapshot = BTreeMap::new();
    for entry in response
        .get("snapshot")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if entry.get("kind").and_then(Value::as_str) != Some("file") {
            continue;
        }
        let (Some(raw), Some(text)) = (
            entry.get("path").and_then(Value::as_str),
            entry.get("content").and_then(Value::as_str),
        ) else {
            continue;
        };
        let path = raw
            .strip_prefix("/workspace/")
            .unwrap_or_else(|| raw.trim_start_matches('/'));
        if !valid_path(path) {
            return Err(TurnFailure::new("invalid", "sidecar_snapshot_path").detail(path));
        }
        if snapshot.insert(path.to_owned(), text.to_owned()).is_some() {
            return Err(TurnFailure::new("invalid", "sidecar_snapshot_duplicate").detail(path));
        }
    }
    // A failed snapshot walk returns an empty list; it must never read as
    // "the agent deleted every file".
    if snapshot.is_empty() && !baseline.is_empty() {
        return Err(TurnFailure::new("unavailable", "sidecar_snapshot_missing"));
    }
    let mut files = Vec::new();
    let mut patch = Vec::new();
    for (path, text) in &snapshot {
        if baseline.get(path) == Some(text) {
            continue;
        }
        let sha = content
            .put(text.as_bytes())
            .map_err(|e| TurnFailure::new("unavailable", "content_store_put").detail(e))?;
        files.push((path.clone(), Some(sha)));
        patch.push((
            path.clone(),
            baseline.get(path).cloned(),
            Some(text.clone()),
        ));
    }
    for (path, text) in baseline {
        if !snapshot.contains_key(path) {
            files.push((path.clone(), None));
            patch.push((path.clone(), Some(text.clone()), None));
        }
    }
    Ok(TurnChanges {
        files,
        patch: unified_patch(&patch),
    })
}

impl CodingPanel {
    pub(crate) fn new(
        writer: WriterHandle,
        content: Arc<dyn DraftContent>,
        runner: Arc<dyn SidecarRunner>,
    ) -> Self {
        Self {
            writer,
            content,
            runner,
            running: Arc::default(),
        }
    }

    fn write<T: Send + 'static>(
        &self,
        job: impl FnOnce(&Transaction<'_>) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T, StoreError> {
        self.writer
            .submit(WriteJob::new(JobOptions::default(), job))?
            .blocking_wait()
    }

    /// Dispatch `ctox.coding.turn` / `ctox.coding.cancel`. Blocking: call from
    /// the command plane's blocking worker, never from an async task.
    pub(crate) fn execute(
        &self,
        actor: BusinessOsActor,
        command: CommandDocument,
        now_ms: u64,
    ) -> Result<Admitted, StoreError> {
        match command.command_type.as_str() {
            COMMAND_TURN => self.start_turn(actor, command, now_ms),
            COMMAND_CANCEL => Ok(Admitted {
                result: self.cancel(actor, command, now_ms)?,
                worker: None,
            }),
            other => Err(store_invalid(format!("not a coding command: {other}"))),
        }
    }

    fn start_turn(
        &self,
        actor: BusinessOsActor,
        command: CommandDocument,
        now: u64,
    ) -> Result<Admitted, StoreError> {
        let default_turn_id = format!("turn-{}", uuid::Uuid::new_v4());
        let admission =
            self.write(move |tx| admit_turn(tx, &actor, &command, default_turn_id, now))?;
        let (result, mut input, files, draft_id, base_rev) = match admission {
            Admission::Done(result) => {
                return Ok(Admitted {
                    result,
                    worker: None,
                })
            }
            Admission::Start {
                result,
                input,
                files,
                draft_id,
                base_rev,
            } => (result, input, files, draft_id, base_rev),
        };
        let control = TurnControl::default();
        self.running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(input.turn_id.clone(), control.clone());
        let panel = self.clone();
        let worker = std::thread::Builder::new()
            .name(format!("coding-turn-{}", input.turn_id))
            .spawn(move || {
                let turn_id = input.turn_id.clone();
                let outcome = panel.drive(&mut input, files, &draft_id, &base_rev, &control);
                panel
                    .running
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&turn_id);
                outcome
            })
            .map_err(|e| store_invalid(format!("spawn coding turn worker: {e}")))?;
        Ok(Admitted {
            result,
            worker: Some(worker),
        })
    }

    /// Runs the turn to a terminal state; returns the final coding_turns document.
    fn drive(
        &self,
        input: &mut SidecarTurnInput,
        files: Map<String, Value>,
        draft_id: &str,
        base_rev: &str,
        control: &TurnControl,
    ) -> Value {
        let turn_id = input.turn_id.clone();
        let outcome = (|| -> Result<(BTreeMap<String, String>, Value), TurnFailure> {
            // The sidecar's working tree: the draft's text files (deleted = null
            // entries and non-UTF-8 content stay outside and are left untouched).
            let mut baseline = BTreeMap::new();
            for (path, sha) in &files {
                let Some(sha) = sha.as_str() else { continue };
                if !valid_path(path) {
                    return Err(TurnFailure::new("invalid", "draft_path_invalid").detail(path));
                }
                let bytes = self
                    .content
                    .get(sha)
                    .map_err(|e| TurnFailure::new("unavailable", "content_store_get").detail(e))?;
                if format!("{:x}", Sha256::digest(&bytes)) != sha {
                    return Err(TurnFailure::new("conflict", "content_hash_mismatch").detail(path));
                }
                if let Ok(text) = String::from_utf8(bytes) {
                    baseline.insert(path.clone(), text);
                }
            }
            input.files = baseline.clone();
            if control.is_cancelled() {
                return Err(TurnFailure::new("conflict", "cancelled"));
            }
            self.progress_running(&turn_id);
            let last = Mutex::new(Instant::now());
            let throttle = || {
                let mut last = last.lock().unwrap_or_else(|e| e.into_inner());
                if last.elapsed() >= PROGRESS_MIN_INTERVAL {
                    *last = Instant::now();
                    self.progress_running(&turn_id);
                }
            };
            let response = self.runner.run(input, control, &throttle)?;
            if control.is_cancelled() {
                return Err(TurnFailure::new("conflict", "cancelled"));
            }
            if response.get("ok").and_then(Value::as_bool) != Some(true) {
                let detail = response
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_owned();
                return Err(TurnFailure::new("unavailable", "sidecar_turn_failed").detail(detail));
            }
            Ok((baseline, response))
        })();
        let now = now_ms();
        let result = match outcome {
            Err(failure) => {
                let failure_value = failure.to_value();
                self.write({
                    let turn_id = turn_id.clone();
                    move |tx| {
                        finish_turn(
                            tx,
                            &turn_id,
                            |turn| {
                                turn["state"] = json!("failed");
                                turn["error"] = failure_value;
                            },
                            now,
                        )
                    }
                })
            }
            Ok((baseline, response)) => self.complete(
                &turn_id, input, &baseline, &response, draft_id, base_rev, now,
            ),
        };
        match result {
            Ok(Some(turn)) => turn,
            // Already terminal (cancelled meanwhile) or store failure: report the
            // persisted state, never invent one.
            _ => self.read_turn(&turn_id).unwrap_or(Value::Null),
        }
    }

    fn complete(
        &self,
        turn_id: &str,
        input: &SidecarTurnInput,
        baseline: &BTreeMap<String, String>,
        response: &Value,
        draft_id: &str,
        base_rev: &str,
        now: u64,
    ) -> Result<Option<Value>, StoreError> {
        let (messages, tool_events) = project_messages(response, now);
        let changes = match diff_snapshot(baseline, response, self.content.as_ref()) {
            Ok(changes) => changes,
            Err(failure) => {
                let failure_value = failure.to_value();
                let turn_id = turn_id.to_owned();
                return self.write(move |tx| {
                    finish_turn(
                        tx,
                        &turn_id,
                        |turn| {
                            turn["state"] = json!("failed");
                            turn["error"] = failure_value;
                        },
                        now,
                    )
                });
            }
        };
        // The conflict patch is stored up front (content-addressed, idempotent);
        // its files record is only created when the CAS actually fails.
        let patch_ref = if changes.files.is_empty() {
            None
        } else {
            self.content
                .put(changes.patch.as_bytes())
                .ok()
                .map(|r| (r, changes.patch.len()))
        };
        let turn_id = turn_id.to_owned();
        let draft_id = draft_id.to_owned();
        let base_rev = base_rev.to_owned();
        let app_id = input.app_id.clone();
        self.write(move |tx| {
            let Some(turn) = get(tx, COLLECTION_TURNS, &turn_id)? else {
                return Ok(None);
            };
            if !matches!(turn["state"].as_str(), Some("queued" | "running")) {
                return Ok(None);
            }
            let changed_files: Vec<Value> = changes
                .files
                .iter()
                .map(|(p, _)| Value::String(p.clone()))
                .collect();
            let mut new_rev = None;
            let mut conflict = false;
            if !changes.files.is_empty() {
                match get_with_rev(tx, COLLECTION_DRAFTS, &draft_id)? {
                    Some((rev, mut draft)) if rev == base_rev => {
                        let files = draft
                            .as_object_mut()
                            .ok_or_else(|| store_invalid("draft is not an object"))?
                            .entry("files")
                            .or_insert_with(|| json!({}))
                            .as_object_mut()
                            .ok_or_else(|| store_invalid("draft files is not an object"))?;
                        for (path, sha) in &changes.files {
                            files.insert(
                                path.clone(),
                                sha.clone().map(Value::String).unwrap_or(Value::Null),
                            );
                        }
                        draft["updated_at_ms"] = json!(now);
                        new_rev = put_if_rev(tx, COLLECTION_DRAFTS, &draft_id, &base_rev, &draft)?;
                        conflict = new_rev.is_none();
                    }
                    _ => conflict = true,
                }
            }
            let mut patch = Value::Null;
            if conflict {
                // Nothing overwritten: the agent's changes survive as a patch file.
                let Some((reference, size)) = patch_ref.clone() else {
                    let failure = TurnFailure::new("unavailable", "content_store_put").to_value();
                    return finish_turn(
                        tx,
                        &turn_id,
                        |turn| {
                            turn["state"] = json!("failed");
                            turn["error"] = failure;
                        },
                        now,
                    );
                };
                let file_id = format!("coding-patch:{turn_id}");
                put(
                    tx,
                    "files",
                    &file_id,
                    &json!({
                        "id": file_id, "name": format!("{turn_id}.patch"), "mime": "text/x-diff",
                        "size": size, "ref": reference, "owner_app": app_id,
                        "owner_user": turn["user_id"], "created_at_ms": now,
                    }),
                )?;
                patch = json!({"ref": reference, "file_id": file_id});
            }
            finish_turn(
                tx,
                &turn_id,
                |turn| {
                    turn["state"] = json!(if conflict { "conflict" } else { "done" });
                    if let Some(array) = turn["messages"].as_array_mut() {
                        array.extend(messages);
                        array.truncate(MAX_MESSAGES);
                    }
                    turn["tool_events"] = Value::Array(tool_events);
                    turn["changed_files"] = Value::Array(changed_files);
                    turn["draft_rev_after"] = if conflict {
                        Value::Null
                    } else {
                        Value::String(new_rev.unwrap_or(base_rev))
                    };
                    if !patch.is_null() {
                        turn["patch"] = patch;
                    }
                },
                now,
            )
        })
    }

    fn progress_running(&self, turn_id: &str) {
        let turn_id = turn_id.to_owned();
        let now = now_ms();
        let _ = self.write(move |tx| {
            let Some(mut turn) = get(tx, COLLECTION_TURNS, &turn_id)? else {
                return Ok(());
            };
            if !matches!(turn["state"].as_str(), Some("queued" | "running")) {
                return Ok(());
            }
            turn["state"] = json!("running");
            turn["updated_at_ms"] = json!(now);
            put(tx, COLLECTION_TURNS, &turn_id, &turn)?;
            Ok(())
        });
    }

    fn read_turn(&self, turn_id: &str) -> Result<Value, StoreError> {
        let turn_id = turn_id.to_owned();
        self.write(move |tx| Ok(get(tx, COLLECTION_TURNS, &turn_id)?.unwrap_or(Value::Null)))
    }

    /// `ctox.coding.cancel {turn_id}`: persist `cancelled` (plus at most one
    /// callback), then kill the sidecar. Only the turn's user may cancel.
    fn cancel(
        &self,
        actor: BusinessOsActor,
        command: CommandDocument,
        now: u64,
    ) -> Result<Value, StoreError> {
        let actor_id = actor.id.clone().unwrap_or_default();
        let turn_id = command
            .payload
            .get("turn_id")
            .and_then(Value::as_str)
            .filter(|t| valid_id(t, 128))
            .map(str::to_owned);
        let result = self.write(move |tx| {
            let result = (|| -> Result<Value, StoreError> {
                let Some(turn_id) = turn_id else {
                    return Ok(rejected("invalid", "payload_invalid:turn_id"));
                };
                let Some(turn) = get(tx, COLLECTION_TURNS, &turn_id)? else {
                    return Ok(rejected("not_found", "turn_not_found"));
                };
                if actor_id.is_empty() || turn["user_id"].as_str() != Some(actor_id.as_str()) {
                    return Ok(rejected("forbidden", "turn_not_owned"));
                }
                match finish_turn(tx, &turn_id, |turn| turn["state"] = json!("cancelled"), now)? {
                    Some(_) => {
                        Ok(json!({"status":"completed","turn_id":turn_id,"state":"cancelled"}))
                    }
                    None => Ok(rejected("conflict", "turn_not_running")),
                }
            })()?;
            record_command(tx, &actor_id, &command, &result, now)?;
            Ok(result)
        })?;
        if result["status"] == "completed" {
            let turn_id = result["turn_id"].as_str().unwrap_or_default();
            if let Some(control) = self
                .running
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(turn_id)
                .cloned()
            {
                control.cancel();
            }
        }
        Ok(result)
    }

    /// Daemon start: turns that were queued/running when the previous process
    /// died have no sidecar any more. Persist them as failed (durable state
    /// first) instead of leaving the panel spinning forever.
    pub(crate) fn recover_interrupted(&self, now: u64) -> Result<usize, StoreError> {
        self.write(move |tx| {
            let (table, _) = table(tx, COLLECTION_TURNS)?;
            let mut statement = tx.prepare(&format!(
                "SELECT id FROM {table} WHERE deleted=0
                 AND json_extract(data,'$.state') IN ('queued','running')"
            ))?;
            let ids = statement
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            drop(statement);
            let mut count = 0;
            for id in ids {
                let failure = TurnFailure::new("unavailable", "daemon_restarted").to_value();
                if finish_turn(
                    tx,
                    &id,
                    |turn| {
                        turn["state"] = json!("failed");
                        turn["error"] = failure;
                    },
                    now,
                )?
                .is_some()
                {
                    count += 1;
                }
            }
            Ok(count)
        })
    }

    /// Retention (§16.44: 30 days) for finished turns, as payload-free
    /// tombstones (§16.23).
    pub(crate) fn purge_expired(&self, now: u64) -> Result<usize, StoreError> {
        self.write(move |tx| {
            let Some(collection) = schema::active_collection(tx, COLLECTION_TURNS)? else {
                return Ok(0);
            };
            let table = quoted(&collection.table);
            let cutoff = now.saturating_sub(TURN_RETENTION_MS);
            let mut statement = tx.prepare(&format!(
                "SELECT id,rev FROM {table} WHERE deleted=0
                 AND json_extract(data,'$.state') NOT IN ('queued','running')
                 AND json_extract(data,'$.finished_at_ms') < ?1"
            ))?;
            let rows = statement
                .query_map([cutoff as i64], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            drop(statement);
            for (id, rev) in &rows {
                schema::remove_document(
                    tx,
                    COLLECTION_TURNS,
                    collection.gen,
                    id,
                    rev,
                    &next_rev(rev, "deleted"),
                )?;
            }
            Ok(rows.len())
        })
    }
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "draft_turn_tests.rs"]
mod tests;
