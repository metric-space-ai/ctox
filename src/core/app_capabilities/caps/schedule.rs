//! Native cron/calendar persistence with an app-independent command envelope.
//! The daemon consumes `dispatch_emitted` before considering an emitted cron
//! message for model execution. This hook does not start a private polling loop.
use super::{
    task::{self, decode, invalid, native, route, AppCommand, Backend},
    NativeRoute,
};
use crate::app_capabilities::{CapCallContext, CtoxError, ErrorCode};
use crate::mission::{channels, schedule as engine};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
const MARKER: &str = "CTOX_APP_COMMAND_V4 ";
pub(super) const ROUTES: &[NativeRoute] = &[
    route(
        "schedule",
        "ensure",
        "cap.schedule.ensure",
        "schedule::ensure_task_with_calendar",
        "src/core/mission/schedule.rs:443",
    ),
    route(
        "schedule",
        "upsert",
        "cap.schedule.ensure",
        "schedule::ensure_task_with_calendar",
        "src/core/mission/schedule.rs:443",
    ),
    route(
        "schedule",
        "list",
        "cap.schedule.list",
        "schedule::list_tasks",
        "src/core/mission/schedule.rs:355",
    ),
    route(
        "schedule",
        "set_enabled",
        "cap.schedule.set_enabled",
        "schedule::set_task_enabled",
        "src/core/mission/schedule.rs:370",
    ),
    route(
        "schedule",
        "remove",
        "cap.schedule.remove",
        "schedule::remove_task",
        "src/core/mission/schedule.rs:395",
    ),
];
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Command {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    payload: Value,
    actor_id: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    app_id: String,
    actor_id: String,
    name: String,
    thread_key: String,
    command: Command,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ensure {
    name: String,
    cron_expr: String,
    command: Command,
    thread_key: Option<String>,
    calendar: Option<engine::ScheduleCalendar>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct List {
    name: Option<String>,
    thread_key: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Enable {
    task_id: String,
    enabled: bool,
}
fn scoped_name(e: &Envelope) -> String {
    format!(
        "cap-schedule:{}",
        task::digest(&json!([e.app_id, e.actor_id, e.name]))
    )
}
fn scoped_thread(e: &Envelope) -> String {
    format!(
        "cap-schedule:{}",
        task::digest(&json!([e.app_id, e.actor_id, e.thread_key]))
    )
}
fn envelope(prompt: &str) -> Option<Envelope> {
    let mut values = prompt.lines().filter_map(|line| line.strip_prefix(MARKER));
    let value = values.next()?;
    if values.next().is_some() {
        return None;
    }
    serde_json::from_str(value).ok()
}
fn owned(ctx: &CapCallContext, t: &engine::ScheduledTaskView) -> Option<Envelope> {
    let e = envelope(&t.prompt)?;
    if e.app_id == ctx.app_id
        && e.actor_id == ctx.actor.id
        && t.name == scoped_name(&e)
        && t.thread_key == scoped_thread(&e)
    {
        Some(e)
    } else {
        None
    }
}
fn view(t: &engine::ScheduledTaskView, e: &Envelope) -> Value {
    json!({"task_id":t.task_id,"name":e.name,"thread_key":e.thread_key,"cron_expr":t.cron_expr,"calendar":t.calendar,"enabled":t.enabled,"next_run_at":t.next_run_at,"last_run_at":t.last_run_at,"command":e.command})
}
fn load(
    backend: &Backend<'_>,
    ctx: &CapCallContext,
    id: &str,
) -> Result<(engine::ScheduledTaskView, Envelope), CtoxError> {
    task::identifier(id)?;
    let t = engine::list_tasks(backend.root)
        .map_err(native)?
        .into_iter()
        .find(|t| t.task_id == id)
        .ok_or_else(|| CtoxError::new(ErrorCode::NotFound, "schedule not found"))?;
    let e =
        owned(ctx, &t).ok_or_else(|| CtoxError::new(ErrorCode::Forbidden, "foreign schedule"))?;
    Ok((t, e))
}
pub(super) fn execute(
    backend: &Backend<'_>,
    ctx: &CapCallContext,
    function: &str,
    args: &Value,
) -> Result<Value, CtoxError> {
    match function {
        "ensure" | "upsert" => {
            let r: Ensure = decode(args)?;
            task::identifier(&r.name)?;
            task::identifier(&r.command.kind)?;
            if r.command
                .actor_id
                .as_deref()
                .is_some_and(|a| a != ctx.actor.id)
            {
                return Err(CtoxError::new(
                    ErrorCode::Forbidden,
                    "schedule actor is Core-bound",
                ));
            }
            let artifact = backend.artifact(ctx)?;
            if !artifact.handler(&r.command.kind) {
                return Err(invalid());
            }
            backend
                .resources
                .authorize_command(ctx, &ctx.app_id, &r.command.kind)?;
            let e = Envelope {
                app_id: ctx.app_id.clone(),
                actor_id: ctx.actor.id.clone(),
                name: r.name,
                thread_key: r.thread_key.unwrap_or_default(),
                command: r.command,
            };
            let t = engine::ensure_task_with_calendar(
                backend.root,
                engine::ScheduleEnsureRequest {
                    name: scoped_name(&e),
                    cron_expr: r.cron_expr,
                    prompt: format!("{MARKER}{}", serde_json::to_string(&e).map_err(native)?),
                    thread_key: scoped_thread(&e),
                    skill: None,
                },
                r.calendar.unwrap_or_default(),
            )
            .map_err(native)?;
            Ok(view(&t, &e))
        }
        "list" => {
            let r: List = decode(args)?;
            let tasks = engine::list_tasks(backend.root)
                .map_err(native)?
                .into_iter()
                .filter_map(|t| {
                    let e = owned(ctx, &t)?;
                    if r.name.as_ref().is_some_and(|n| n != &e.name)
                        || r.thread_key.as_ref().is_some_and(|k| k != &e.thread_key)
                    {
                        None
                    } else {
                        Some(view(&t, &e))
                    }
                })
                .collect::<Vec<_>>();
            Ok(json!({"tasks":tasks}))
        }
        "set_enabled" => {
            let r: Enable = decode(args)?;
            let (_, e) = load(backend, ctx, &r.task_id)?;
            let t =
                engine::set_task_enabled(backend.root, &r.task_id, r.enabled).map_err(native)?;
            Ok(view(&t, &e))
        }
        "remove" => {
            let r: task::TaskId = decode(args)?;
            load(backend, ctx, &r.task_id)?;
            engine::remove_task(backend.root, &r.task_id).map_err(native)?;
            Ok(json!({"task_id":r.task_id,"removed":true}))
        }
        _ => Err(CtoxError::new(
            ErrorCode::NotFound,
            "unknown schedule primitive",
        )),
    }
}
/// Consume an existing native cron message, under a fresh Core host fence.
/// Returns None for non-CAP schedules. Must run before the native model worker.
pub fn dispatch_emitted(
    backend: &Backend<'_>,
    ctx: &CapCallContext,
    message_key: &str,
) -> Result<Option<Value>, CtoxError> {
    let conn = channels::open_channel_db(&crate::paths::core_db(backend.root)).map_err(native)?;
    let row:Option<(String,String,String)>=conn.query_row("SELECT body_text,thread_key,metadata_json FROM communication_messages WHERE message_key=?1 AND channel='cron' AND account_key='cron:system'",[message_key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(native)?;
    let Some((prompt, thread, metadata)) = row else {
        return Ok(None);
    };
    let Some(e) = envelope(&prompt) else {
        return Ok(None);
    };
    let metadata: Value = serde_json::from_str(&metadata).map_err(native)?;
    if e.app_id != ctx.app_id
        || e.actor_id != ctx.actor.id
        || thread != scoped_thread(&e)
        || metadata["source"] != "ctox-schedule"
        || metadata["task_name"] != scoped_name(&e)
    {
        return Err(CtoxError::new(ErrorCode::Forbidden, "cron origin mismatch"));
    }
    let artifact = backend.artifact(ctx)?;
    if !artifact.handler(&e.command.kind) {
        return Err(invalid());
    }
    backend
        .resources
        .authorize_command(ctx, &e.app_id, &e.command.kind)?;
    let mut payload = e.command.payload.as_object().cloned().ok_or_else(invalid)?;
    payload.insert("trigger".into(), json!("schedule"));
    let run = metadata
        .get("run_id")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    let command = AppCommand {
        id: format!("cap-schedule-run:{}", task::digest(&json!([e.app_id, run]))),
        app_id: e.app_id.clone(),
        origin_app: e.app_id,
        actor_id: ctx.actor.id.clone(),
        command_type: e.command.kind,
        payload: Value::Object(payload),
    };
    Ok(Some(backend.resources.emit_command_once(ctx, &command)?))
}
