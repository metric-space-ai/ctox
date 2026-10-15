//! Generic durable tasks. The native queue remains the execution authority.
use super::{NativeCall, NativeRoute};
use crate::app_capabilities::schema::PayloadShape;
use crate::app_capabilities::{
    CapCallContext, CapabilityBackend, CapabilityHost, CtoxError, ErrorCode,
};
use crate::mission::channels::{
    self, QueueTaskCreateRequest, QueueTaskUpdateRequest, QueueTaskView,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Component, Path};

pub(super) const fn route(
    cap: &'static str,
    function: &'static str,
    command: &'static str,
    rust: &'static str,
    source: &'static str,
) -> NativeRoute {
    NativeRoute::new(cap, function, command, rust, source, PayloadShape::Object)
}
pub(super) const ROUTES: &[NativeRoute] = &[
    route(
        "agent.task",
        "enqueue",
        "cap.task.enqueue",
        "channels::claim_business_command_with_queue",
        "src/core/mission/channels/command_saga.rs:52",
    ),
    route(
        "agent.task",
        "get",
        "cap.task.get",
        "channels::load_queue_task",
        "src/core/mission/channels/mod.rs:3745",
    ),
    route(
        "agent.task",
        "find",
        "cap.task.find",
        "channels::load_queue_task",
        "src/core/mission/channels/mod.rs:3745",
    ),
    route(
        "agent.task",
        "update",
        "cap.task.update",
        "channels::update_queue_task",
        "src/core/mission/channels/mod.rs:3773",
    ),
    route(
        "agent.task",
        "cancel",
        "cap.task.cancel",
        "channels::update_queue_task",
        "src/core/mission/channels/mod.rs:3773",
    ),
    route(
        "agent.task",
        "set_metadata",
        "cap.task.set_metadata",
        "channels::set_queue_task_metadata_value",
        "src/core/mission/channels/mod.rs:4095",
    ),
    route(
        "agent.task",
        "workspace.read",
        "cap.task.workspace.read",
        "channels::load_queue_task",
        "src/core/mission/channels/mod.rs:3745",
    ),
    route(
        "agent.workspace",
        "read",
        "cap.task.workspace.read",
        "channels::load_queue_task",
        "src/core/mission/channels/mod.rs:3745",
    ),
];

pub(super) fn invalid() -> CtoxError {
    CtoxError::invalid("$.args")
}
pub(super) fn native(_: impl std::fmt::Display) -> CtoxError {
    CtoxError::new(ErrorCode::Unavailable, "native task operation failed")
}
pub(super) fn decode<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T, CtoxError> {
    serde_json::from_value(v.clone()).map_err(|_| invalid())
}
pub(super) fn digest(v: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(v).expect("JSON serializes"))
    )
}
pub(super) fn identifier(s: &str) -> Result<(), CtoxError> {
    crate::app_capabilities::protocol::identifier(s, "$.args")
}
pub(super) fn relative(s: &str) -> Result<(), CtoxError> {
    if s.is_empty()
        || s.contains('\\')
        || s.contains('\0')
        || Path::new(s)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(invalid());
    }
    Ok(())
}

/// Immutable package snapshot supplied by the authenticated runtime, never decoded
/// from capability arguments. `version` must be the admitted active app version.
#[derive(Clone)]
pub struct AppArtifact {
    pub app_id: String,
    pub version: String,
    pub manifest: Value,
    pub files: BTreeMap<String, Vec<u8>>,
}
impl AppArtifact {
    pub(super) fn text(&self, path: &str) -> Result<String, CtoxError> {
        relative(path)?;
        let bytes = self
            .files
            .get(path)
            .ok_or_else(|| CtoxError::new(ErrorCode::NotFound, "app artifact file not found"))?;
        if bytes.len() > 1024 * 1024 {
            return Err(CtoxError::new(
                ErrorCode::Quota,
                "app artifact text exceeds 1 MiB",
            ));
        }
        String::from_utf8(bytes.clone()).map_err(|_| invalid())
    }
    pub(super) fn handler(&self, kind: &str) -> bool {
        self.manifest.get("handlers").is_some_and(|v| {
            v.get(kind).is_some()
                || v.as_array().is_some_and(|a| {
                    a.iter()
                        .any(|x| x.get("type").and_then(Value::as_str) == Some(kind))
                })
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileRef {
    #[serde(rename = "ref")]
    pub reference: String,
    pub size: u64,
    pub mime: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppCommand {
    pub id: String,
    pub app_id: String,
    pub origin_app: String,
    pub actor_id: String,
    pub command_type: String,
    pub payload: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerminalResult {
    pub status: String,
    pub result: Value,
    pub workspace_refs: Vec<String>,
}
/// Native runtime ports. No permissive/default implementation is provided.
/// All methods run within the calling Core host's held current policy/fence.
pub trait Resources {
    fn artifact(&self, app: &str, version: Option<&str>) -> Result<AppArtifact, CtoxError>;
    /// Atomically publish CAS bytes AND a files row owned by ctx.app_id.
    fn publish_file(
        &self,
        ctx: &CapCallContext,
        bytes: &[u8],
        mime: &str,
    ) -> Result<FileRef, CtoxError>;
    fn authorize_command(
        &self,
        ctx: &CapCallContext,
        app: &str,
        kind: &str,
    ) -> Result<(), CtoxError>;
    /// Durable command insertion, idempotent by id + immutable full intent.
    fn emit_command_once(
        &self,
        ctx: &CapCallContext,
        command: &AppCommand,
    ) -> Result<Value, CtoxError>;
    fn terminal_command(&self, command_id: &str) -> Result<Option<TerminalResult>, CtoxError>;
}

pub struct Backend<'a> {
    pub(super) root: &'a Path,
    pub(super) resources: &'a dyn Resources,
    fallback: &'a dyn CapabilityBackend,
}
impl<'a> Backend<'a> {
    pub fn new(
        root: &'a Path,
        resources: &'a dyn Resources,
        fallback: &'a dyn CapabilityBackend,
    ) -> Self {
        Self {
            root,
            resources,
            fallback,
        }
    }
    pub(super) fn artifact(&self, ctx: &CapCallContext) -> Result<AppArtifact, CtoxError> {
        let artifact = self.resources.artifact(&ctx.app_id, Some(&ctx.app_ver))?;
        if artifact.app_id != ctx.app_id || artifact.version != ctx.app_ver {
            return Err(CtoxError::new(
                ErrorCode::Revoked,
                "app artifact binding changed",
            ));
        }
        Ok(artifact)
    }
    pub(super) fn owned(
        &self,
        ctx: &CapCallContext,
        task_id: &str,
    ) -> Result<QueueTaskView, CtoxError> {
        identifier(task_id)?;
        let task = channels::load_queue_task(self.root, task_id)
            .map_err(native)?
            .ok_or_else(|| CtoxError::new(ErrorCode::NotFound, "task not found"))?;
        if !owns(ctx, &task) {
            return Err(CtoxError::new(
                ErrorCode::Forbidden,
                "task belongs to another app or actor",
            ));
        }
        Ok(task)
    }
    pub(super) fn view(&self, task: &QueueTaskView) -> Value {
        json!({"task_id":task.message_key,"title":task.title,"instruction":task.prompt,"priority":task.priority,"status":task.route_status,
        "thread_key":task.metadata.pointer("/cap4/thread_key"),"payload":task.metadata.pointer("/cap4/payload"),"metadata":task.metadata.get("app_metadata"),"status_note":task.status_note})
    }
    pub(super) fn find(
        &self,
        ctx: &CapCallContext,
        idempotency_key: Option<&str>,
        command_id: Option<&str>,
    ) -> Result<Option<QueueTaskView>, CtoxError> {
        if idempotency_key.is_some() == command_id.is_some() {
            return Err(invalid());
        }
        let conn = channels::open_channel_db(&crate::paths::core_db(self.root)).map_err(native)?;
        let mut q=conn.prepare("SELECT message_key FROM communication_messages WHERE channel='queue' AND json_extract(metadata_json,'$.cap4.app_id')=?1 AND json_extract(metadata_json,'$.cap4.actor_id')=?2 AND ((?3 IS NOT NULL AND json_extract(metadata_json,'$.cap4.idempotency_key')=?3) OR (?4 IS NOT NULL AND json_extract(metadata_json,'$.cap4.command_id')=?4)) ORDER BY message_key LIMIT 2").map_err(native)?;
        let ids = q
            .query_map(
                params![ctx.app_id, ctx.actor.id, idempotency_key, command_id],
                |r| r.get::<_, String>(0),
            )
            .map_err(native)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(native)?;
        if ids.len() > 1 {
            return Err(CtoxError::new(
                ErrorCode::Conflict,
                "command has multiple tasks; use an idempotency key",
            ));
        }
        ids.first().map(|id| self.owned(ctx, id)).transpose()
    }
    fn enqueue(&self, ctx: &CapCallContext, args: &Value) -> Result<Value, CtoxError> {
        let r: Enqueue = decode(args)?;
        identifier(&r.title)?;
        let artifact = self.artifact(ctx)?;
        for callback in [&r.on_complete, &r.on_failed].into_iter().flatten() {
            identifier(callback)?;
            if !artifact.handler(callback) {
                return Err(invalid());
            }
        }
        let mut prompt = String::new();
        let mut assets = BTreeMap::new();
        if let Some(path) = &r.template {
            relative(path)?;
            if !path.starts_with("prompts/") {
                return Err(invalid());
            }
            assets.insert(path.clone(), artifact.files[path].clone());
            prompt.push_str(&artifact.text(path)?);
            prompt.push('\n');
        }
        if let Some(skill) = &r.skill {
            let name = if let Some((app, name)) = skill.split_once('/') {
                if app != ctx.app_id {
                    return Err(CtoxError::new(ErrorCode::Forbidden, "foreign app skill"));
                }
                name
            } else {
                skill.as_str()
            };
            let declarations = artifact
                .manifest
                .get("skills")
                .and_then(Value::as_array)
                .ok_or_else(invalid)?;
            let path = declarations
                .iter()
                .find(|x| x.get("name").and_then(Value::as_str) == Some(name))
                .and_then(|x| x.get("path"))
                .and_then(Value::as_str)
                .ok_or_else(invalid)?;
            if !path.starts_with("skills/") || !path.ends_with("/SKILL.md") {
                return Err(invalid());
            }
            assets.insert(path.to_string(), artifact.files[path].clone());
            prompt.push_str(&artifact.text(path)?);
            prompt.push('\n');
            let prefix = path.strip_suffix("SKILL.md").expect("suffix checked");
            for (path, bytes) in &artifact.files {
                if path.starts_with(prefix) && !path.ends_with("/SKILL.md") {
                    relative(path)?;
                    assets.insert(path.clone(), bytes.clone());
                    if let Ok(text) = std::str::from_utf8(bytes) {
                        prompt.push_str(&format!("\n## App skill reference: {path}\n{text}\n"));
                    }
                }
            }
        }
        prompt.push_str(&r.instruction);
        prompt.push_str(&format!(
            "\n\n## Task payload\n{}",
            serde_json::to_string(&r.payload).map_err(native)?
        ));
        if r.instruction.trim().is_empty() && r.template.is_none() {
            return Err(invalid());
        }
        if prompt.len() > 1024 * 1024 {
            return Err(CtoxError::new(
                ErrorCode::Quota,
                "resolved task instruction exceeds 1 MiB",
            ));
        }
        let key = r
            .idempotency_key
            .clone()
            .unwrap_or_else(|| format!("call:{}", digest(&json!([ctx.command_id, ctx.call_id]))));
        identifier(&key)?;
        let task_command = format!(
            "cap-task:{}",
            digest(&json!([ctx.app_id, ctx.actor.id, key]))
        );
        let intent = json!({"app_id":ctx.app_id,"app_ver":ctx.app_ver,"actor_id":ctx.actor.id,"args":args,"resolved_instruction":prompt});
        let workspace = self
            .root
            .join("runtime/capability-workspaces")
            .join(digest(&json!([ctx.app_id, ctx.actor.id])))
            .join(digest(&json!(task_command)));
        std::fs::create_dir_all(&workspace).map_err(native)?;
        if self.find(ctx, Some(&key), None)?.is_none() {
            stage_assets(&workspace, &assets)?;
        }
        let claim=channels::claim_business_command_with_queue(self.root,channels::BusinessCommandClaimRequest {
            command_id:task_command.clone(),idempotency_key:task_command.clone(),payload_hash:format!("sha256:{}",digest(&intent)),module:ctx.app_id.clone(),command_type:"agent.task".into(),record_id:String::new(),intent,created_at_ms:chrono::Utc::now().timestamp_millis(),
        },QueueTaskCreateRequest {title:r.title,prompt,thread_key:format!("cap-task:{}",digest(&json!([ctx.app_id,ctx.actor.id,r.thread_key]))),workspace_root:Some(workspace.to_string_lossy().into()),priority:r.priority.unwrap_or_else(||"normal".into()),suggested_skill:None,parent_message_key:None,extra_metadata:Some(json!({"idempotency_key":task_command,"cap4":{"app_id":ctx.app_id,"actor_id":ctx.actor.id,"app_ver":ctx.app_ver,"command_id":ctx.command_id,"idempotency_key":key,"thread_key":r.thread_key,"payload":r.payload,"on_complete":r.on_complete,"on_failed":r.on_failed}}))}).map_err(native)?;
        Ok(json!({"task_id":claim.task.message_key}))
    }
    fn execute_task(
        &self,
        ctx: &CapCallContext,
        function: &str,
        args: &Value,
    ) -> Result<Value, CtoxError> {
        match function {
            "enqueue" => self.enqueue(ctx, args),
            "find" => {
                let r: Find = decode(args)?;
                Ok(self
                    .find(ctx, r.idempotency_key.as_deref(), r.command_id.as_deref())?
                    .as_ref()
                    .map(|t| self.view(t))
                    .unwrap_or_else(|| json!({"__cap4_not_found":true})))
            }
            "get" => {
                let r: TaskId = decode(args)?;
                let t = self.owned(ctx, &r.task_id)?;
                self.settle_task(ctx, &t)?;
                Ok(self.view(&t))
            }
            "update" => {
                let r: Update = decode(args)?;
                self.owned(ctx, &r.task_id)?;
                let t = channels::update_queue_task(
                    self.root,
                    QueueTaskUpdateRequest {
                        message_key: r.task_id,
                        title: r.title,
                        prompt: r.instruction,
                        priority: r.priority,
                        status_note: r.status_note,
                        ..Default::default()
                    },
                )
                .map_err(native)?;
                Ok(self.view(&t))
            }
            "cancel" => {
                let r: Cancel = decode(args)?;
                self.owned(ctx, &r.task_id)?;
                let t = channels::update_queue_task(
                    self.root,
                    QueueTaskUpdateRequest {
                        message_key: r.task_id,
                        route_status: Some("cancelled".into()),
                        status_note: r.status_note,
                        ..Default::default()
                    },
                )
                .map_err(native)?;
                self.settle_task(ctx, &t)?;
                Ok(self.view(&t))
            }
            "set_metadata" => {
                let r: Metadata = decode(args)?;
                self.owned(ctx, &r.task_id)?;
                if r.key.is_empty()
                    || r.key.len() > 64
                    || !r
                        .key
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                {
                    return Err(invalid());
                }
                let mut data =
                    channels::queue_task_metadata_value(self.root, &r.task_id, "app_metadata")
                        .map_err(native)?
                        .unwrap_or_else(|| json!({}));
                data.as_object_mut()
                    .ok_or_else(invalid)?
                    .insert(r.key, r.value);
                channels::set_queue_task_metadata_value(
                    self.root,
                    &r.task_id,
                    "app_metadata",
                    data,
                )
                .map_err(native)?;
                Ok(json!({"task_id":r.task_id}))
            }
            "workspace.read" => {
                let r: Workspace = decode(args)?;
                let task = self.owned(ctx, &r.task_id)?;
                relative(&r.path)?;
                let root = task.workspace_root.as_deref().ok_or_else(|| {
                    CtoxError::new(ErrorCode::NotFound, "task workspace unavailable")
                })?;
                let bytes = read_workspace(Path::new(root), &r.path)?;
                if bytes.len() <= 1024 * 1024 {
                    if let Ok(text) = std::str::from_utf8(&bytes) {
                        return Ok(json!({"text":text}));
                    }
                }
                let mime = "application/octet-stream";
                let file = self.resources.publish_file(ctx, &bytes, mime)?;
                if file.reference != format!("{:x}", Sha256::digest(&bytes))
                    || file.size != bytes.len() as u64
                    || file.mime != mime
                {
                    return Err(CtoxError::uncertain());
                }
                serde_json::to_value(file).map_err(native)
            }
            _ => Err(CtoxError::new(
                ErrorCode::NotFound,
                "unknown task primitive",
            )),
        }
    }
    /// Called by the runtime's terminal queue event under a fresh held host fence.
    /// get/cancel also reconcile the same durable callback outbox.
    pub fn settle_task(
        &self,
        ctx: &CapCallContext,
        task: &QueueTaskView,
    ) -> Result<Value, CtoxError> {
        if !owns(ctx, task) {
            return Err(CtoxError::new(
                ErrorCode::Forbidden,
                "foreign task callback",
            ));
        }
        if !channels::route_status_is_terminal(&task.route_status) {
            return Ok(json!({"pending":true}));
        }
        let success = matches!(task.route_status.as_str(), "handled" | "completed");
        let status = if task.route_status == "cancelled" {
            "cancelled"
        } else if success {
            "completed"
        } else {
            "failed"
        };
        let field = if success { "on_complete" } else { "on_failed" };
        let kind = task
            .metadata
            .get("cap4")
            .and_then(|v| v.get(field))
            .and_then(Value::as_str);
        let result = channels::inspect_business_command_for_task(self.root, &task.message_key)
            .map_err(native)?
            .and_then(|v| v.get("result").cloned())
            .unwrap_or(Value::Null);
        let refs = task
            .metadata
            .get("workspace_refs")
            .cloned()
            .unwrap_or_else(|| json!([]));
        for p in refs.as_array().ok_or_else(invalid)? {
            relative(p.as_str().ok_or_else(invalid)?)?;
        }
        self.callback(ctx,"task",&task.message_key,kind,json!({"task_id":task.message_key,"status":status,"result":result,"workspace_refs":refs}))
    }
    pub(super) fn callback(
        &self,
        ctx: &CapCallContext,
        source: &str,
        id: &str,
        kind: Option<&str>,
        payload: Value,
    ) -> Result<Value, CtoxError> {
        let conn = channels::open_channel_db(&crate::paths::core_db(self.root)).map_err(native)?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS capability_callback_outbox(source TEXT NOT NULL, source_id TEXT NOT NULL, app_id TEXT NOT NULL, actor_id TEXT NOT NULL, command_json TEXT NOT NULL, delivered INTEGER NOT NULL DEFAULT 0, receipt_json TEXT, PRIMARY KEY(source,source_id))").map_err(native)?;
        let command = kind.map(|kind| AppCommand {
            id: format!("cap-callback:{}", digest(&json!([source, id]))),
            app_id: ctx.app_id.clone(),
            origin_app: ctx.app_id.clone(),
            actor_id: ctx.actor.id.clone(),
            command_type: kind.into(),
            payload,
        });
        conn.execute("INSERT OR IGNORE INTO capability_callback_outbox(source,source_id,app_id,actor_id,command_json) VALUES(?1,?2,?3,?4,?5)",params![source,id,ctx.app_id,ctx.actor.id,serde_json::to_string(&command).map_err(native)?]).map_err(native)?;
        let (app,actor,body,delivered,receipt):(String,String,String,bool,Option<String>)=conn.query_row("SELECT app_id,actor_id,command_json,delivered,receipt_json FROM capability_callback_outbox WHERE source=?1 AND source_id=?2",params![source,id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).map_err(native)?;
        if app != ctx.app_id || actor != ctx.actor.id {
            return Err(CtoxError::new(
                ErrorCode::Forbidden,
                "foreign callback origin",
            ));
        }
        if delivered {
            return Ok(receipt
                .and_then(|v| serde_json::from_str(&v).ok())
                .unwrap_or(Value::Null));
        }
        let saved: Option<AppCommand> = serde_json::from_str(&body).map_err(native)?;
        let result = if let Some(command) = saved {
            let artifact = self.artifact(ctx)?;
            if !artifact.handler(&command.command_type) {
                return Err(invalid());
            }
            self.resources
                .authorize_command(ctx, &command.app_id, &command.command_type)?;
            self.resources.emit_command_once(ctx, &command)?
        } else {
            json!({"callback":null})
        };
        conn.execute("UPDATE capability_callback_outbox SET delivered=1,receipt_json=?3 WHERE source=?1 AND source_id=?2",params![source,id,serde_json::to_string(&result).map_err(native)?]).map_err(native)?;
        Ok(result)
    }
}
pub(super) fn owns(ctx: &CapCallContext, task: &QueueTaskView) -> bool {
    task.metadata
        .pointer("/cap4/app_id")
        .and_then(Value::as_str)
        == Some(&ctx.app_id)
        && task
            .metadata
            .pointer("/cap4/actor_id")
            .and_then(Value::as_str)
            == Some(&ctx.actor.id)
}
impl CapabilityBackend for Backend<'_> {
    fn execute(&self, ctx: &CapCallContext, call: &NativeCall) -> Result<Value, CtoxError> {
        if call.route().command_type().starts_with("cap.task.") {
            return self.execute_task(
                ctx,
                call.route().command_type().trim_start_matches("cap.task."),
                call.payload(),
            );
        }
        if call.route().command_type().starts_with("cap.queue.") {
            return super::queue::execute(self, ctx, call.route().function, call.payload());
        }
        if call.route().command_type().starts_with("cap.schedule.") {
            return super::schedule::execute(self, ctx, call.route().function, call.payload());
        }
        self.fallback.execute(ctx, call)
    }
}
/// Flat arguments at the app boundary; the old payload envelope is internal.
pub fn invoke(
    host: &dyn CapabilityHost,
    backend: &dyn CapabilityBackend,
    ctx: &CapCallContext,
    name: &str,
    function: &str,
    args: Value,
) -> Result<Value, CtoxError> {
    let registry = crate::app_capabilities::cap2::registry();
    let routes = || {
        super::all_routes()
            .chain(crate::app_capabilities::cap2::ROUTES)
            .chain(crate::app_capabilities::cap2::mail::ROUTES)
    };
    let path = format!("{name}.{function}");
    let route = routes()
        .filter(|r| {
            (r.capability == name && r.function == function)
                || crate::app_capabilities::cap2::call_path(r) == path
        })
        .max_by_key(|r| r.capability.len());
    let (name, function) = route
        .map(|r| (r.capability, r.function))
        .unwrap_or((name, function));
    let wire = if name == "agent.queue" && function == "control" {
        let mut payload = args.as_object().cloned().ok_or_else(invalid)?;
        let action = payload.remove("action").ok_or_else(invalid)?;
        json!({"action":action,"payload":payload})
    } else {
        json!({"payload":args})
    };
    let result = registry.invoke(host, backend, ctx, name, function, wire)?;
    if result.get("__cap4_not_found") == Some(&Value::Bool(true)) {
        Ok(Value::Null)
    } else {
        Ok(result)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Enqueue {
    title: String,
    #[serde(default)]
    instruction: String,
    skill: Option<String>,
    template: Option<String>,
    thread_key: Option<String>,
    #[serde(default)]
    payload: Value,
    priority: Option<String>,
    idempotency_key: Option<String>,
    on_complete: Option<String>,
    on_failed: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TaskId {
    pub task_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Find {
    idempotency_key: Option<String>,
    command_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Update {
    task_id: String,
    title: Option<String>,
    instruction: Option<String>,
    priority: Option<String>,
    status_note: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Cancel {
    task_id: String,
    status_note: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    task_id: String,
    key: String,
    value: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Workspace {
    task_id: String,
    path: String,
}
#[cfg(unix)]
fn read_workspace(root: &Path, path: &str) -> Result<Vec<u8>, CtoxError> {
    use std::os::fd::{AsRawFd, FromRawFd};
    relative(path)?;
    let mut directory = std::fs::File::open(root).map_err(native)?;
    let parts = Path::new(path)
        .components()
        .map(|c| c.as_os_str())
        .collect::<Vec<_>>();
    for (i, part) in parts.iter().enumerate() {
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::CString::new(part.as_bytes()).map_err(|_| invalid())?;
        let flags = libc::O_RDONLY
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | if i + 1 < parts.len() {
                libc::O_DIRECTORY
            } else {
                0
            };
        let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(CtoxError::new(
                ErrorCode::Forbidden,
                "workspace path cannot be opened safely",
            ));
        }
        directory = unsafe { std::fs::File::from_raw_fd(fd) };
    }
    if !directory.metadata().map_err(native)?.is_file() {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    directory
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(native)?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(CtoxError::new(
            ErrorCode::Quota,
            "workspace file exceeds 64 MiB",
        ));
    }
    Ok(bytes)
}
#[cfg(not(unix))]
fn read_workspace(_: &Path, _: &str) -> Result<Vec<u8>, CtoxError> {
    Err(CtoxError::new(
        ErrorCode::Unavailable,
        "secure workspace read unavailable on this platform",
    ))
}

#[cfg(unix)]
fn stage_assets(root: &Path, files: &BTreeMap<String, Vec<u8>>) -> Result<(), CtoxError> {
    use std::io::Write;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::{ffi::OsStrExt, fs::OpenOptionsExt};
    if files.values().map(Vec::len).sum::<usize>() > 64 * 1024 * 1024 {
        return Err(CtoxError::new(
            ErrorCode::Quota,
            "skill assets exceed 64 MiB",
        ));
    }
    for (path, bytes) in files {
        relative(path)?;
        let path = format!("app-artifact/{path}");
        let mut directory = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(root)
            .map_err(native)?;
        let parts = Path::new(&path)
            .components()
            .map(|c| c.as_os_str())
            .collect::<Vec<_>>();
        for (i, part) in parts.iter().enumerate() {
            let name = std::ffi::CString::new(part.as_bytes()).map_err(|_| invalid())?;
            if i + 1 < parts.len() {
                let created = unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), 0o700) };
                if created < 0
                    && std::io::Error::last_os_error().raw_os_error() != Some(libc::EEXIST)
                {
                    return Err(native("asset directory"));
                }
                let fd = unsafe {
                    libc::openat(
                        directory.as_raw_fd(),
                        name.as_ptr(),
                        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                    )
                };
                if fd < 0 {
                    return Err(CtoxError::new(
                        ErrorCode::Forbidden,
                        "unsafe asset directory",
                    ));
                }
                directory = unsafe { std::fs::File::from_raw_fd(fd) };
            } else {
                let fd = unsafe {
                    libc::openat(
                        directory.as_raw_fd(),
                        name.as_ptr(),
                        libc::O_WRONLY
                            | libc::O_CREAT
                            | libc::O_EXCL
                            | libc::O_NOFOLLOW
                            | libc::O_CLOEXEC,
                        0o600,
                    )
                };
                if fd < 0 {
                    if std::io::Error::last_os_error().raw_os_error() == Some(libc::EEXIST)
                        && read_workspace(root, &path)? == *bytes
                    {
                        continue;
                    }
                    return Err(CtoxError::new(
                        ErrorCode::Conflict,
                        "existing artifact workspace file differs",
                    ));
                }
                let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
                file.write_all(bytes).map_err(native)?;
                file.sync_all().map_err(native)?;
            }
        }
    }
    Ok(())
}
#[cfg(not(unix))]
fn stage_assets(_: &Path, files: &BTreeMap<String, Vec<u8>>) -> Result<(), CtoxError> {
    if files.is_empty() {
        Ok(())
    } else {
        Err(CtoxError::new(
            ErrorCode::Unavailable,
            "secure app asset staging unavailable on this platform",
        ))
    }
}

/// Transport entry preserving request/call identity and the shared Core fence.
pub fn handle(
    host: &dyn CapabilityHost,
    backend: &dyn CapabilityBackend,
    message: crate::app_capabilities::protocol::CapMessage,
) -> Result<crate::app_capabilities::protocol::CapMessage, CtoxError> {
    use crate::app_capabilities::protocol::{CapMessage, CapOutcome};
    let CapMessage::Invoke {
        request_id,
        ctx,
        name,
        function,
        args,
    } = message
    else {
        return Err(invalid());
    };
    crate::app_capabilities::protocol::identifier(&request_id, "$.request_id")?;
    if request_id != ctx.call_id {
        return Err(CtoxError::invalid("$.ctx.call_id"));
    }
    let outcome = match invoke(host, backend, &ctx, &name, &function, args) {
        Ok(value) => CapOutcome::Ok { value },
        Err(error) => CapOutcome::Error { error },
    };
    Ok(CapMessage::Result {
        request_id,
        outcome,
    })
}
