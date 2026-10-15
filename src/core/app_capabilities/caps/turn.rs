use super::NativeRoute;
use crate::app_capabilities::schema::PayloadShape;

pub(super) const ROUTES: &[NativeRoute] = &[
    NativeRoute::new(
        "agent.turn",
        "submit",
        "ctox.workjet.project.supervisor.turn.submit",
        "business_os::project_chats::supervisor_turns::control",
        "src/core/business_os/project_chats/supervisor_turns.rs:322",
        PayloadShape::Object,
    ),
    NativeRoute::new(
        "agent.turn",
        "watch",
        "ctox.workjet.project.supervisor.turn.watch",
        "business_os::project_chats::supervisor_turns::control",
        "src/core/business_os/project_chats/supervisor_turns.rs:322",
        PayloadShape::Object,
    ),
    NativeRoute::new(
        "agent.turn",
        "cancel",
        "ctox.workjet.project.supervisor.turn.cancel",
        "business_os::project_chats::supervisor_turns::control",
        "src/core/business_os/project_chats/supervisor_turns.rs:322",
        PayloadShape::Object,
    ),
    NativeRoute::new(
        "agent.turn",
        "input",
        "ctox.workjet.project.supervisor.turn.input",
        "business_os::project_chats::supervisor_turns::control",
        "src/core/business_os/project_chats/supervisor_turns.rs:322",
        PayloadShape::Object,
    ),
    NativeRoute::new(
        "agent.turn",
        "capabilities",
        "ctox.workjet.project.supervisor.turn.capabilities",
        "business_os::project_chats::supervisor_turns::control",
        "src/core/business_os/project_chats/supervisor_turns.rs:322",
        PayloadShape::Object,
    ),
    NativeRoute::new(
        "agent.turn",
        "history",
        "ctox.workjet.project.supervisor.turn.history",
        "business_os::project_chats::supervisor_turns::control",
        "src/core/business_os/project_chats/supervisor_turns.rs:322",
        PayloadShape::Object,
    ),
    NativeRoute::new(
        "agent.turn",
        "route.read.v1",
        "ctox.workjet.project.supervisor.route.read.v1",
        "business_os::project_chats::supervisor_turns::control",
        "src/core/business_os/project_chats/supervisor_turns.rs:322",
        PayloadShape::Object,
    ),
    NativeRoute::new(
        "agent.turn",
        "route.read.v2",
        "ctox.workjet.project.supervisor.route.read.v2",
        "business_os::project_chats::supervisor_turns::control",
        "src/core/business_os/project_chats/supervisor_turns.rs:322",
        PayloadShape::Object,
    ),
    NativeRoute::new(
        "agent.turn",
        "route.capabilities.v1",
        "ctox.workjet.project.supervisor.route.capabilities.v1",
        "business_os::project_chats::supervisor_turns::control",
        "src/core/business_os/project_chats/supervisor_turns.rs:322",
        PayloadShape::Object,
    ),
    NativeRoute::new(
        "agent.turn",
        "route.capabilities.v2",
        "ctox.workjet.project.supervisor.route.capabilities.v2",
        "business_os::project_chats::supervisor_turns::control",
        "src/core/business_os/project_chats/supervisor_turns.rs:322",
        PayloadShape::Object,
    ),
];

use super::task::{self, decode, invalid, native, AppCommand, Backend};
use crate::app_capabilities::{CapCallContext, CapabilityHost, CtoxError, ErrorCode};
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};

/// Both opt-ins are taken from current Core-resolved manifests, never payload.
pub fn permits_ipc(sender: &Value, receiver: &Value, from: &str, to: &str, kind: &str) -> bool {
    let outgoing = sender
        .get("commands_out")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items.iter().any(|x| {
                x.get("app").and_then(Value::as_str) == Some(to)
                    && x.get("type").and_then(Value::as_str) == Some(kind)
            })
        });
    let incoming = receiver
        .get("commands_in")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items.iter().any(|x| {
                x.get("type").and_then(Value::as_str) == Some(kind)
                    && x.get("from").is_some_and(|froms| {
                        froms.as_str() == Some("*")
                            || froms.as_array().is_some_and(|a| {
                                a.iter()
                                    .any(|x| x.as_str() == Some(from) || x.as_str() == Some("*"))
                            })
                    })
            })
        });
    outgoing && incoming
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandRequest {
    #[serde(alias = "module")]
    app: Option<String>,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    payload: Value,
    on_complete: Option<String>,
    on_failed: Option<String>,
}
/// ctx.command port for the runtime after parent-command admission. Unlike a
/// capability, ctx.command is governed by command rights and commands_in/out.
/// The real host retains the current authority across the complete callback.
pub fn command(
    host: &dyn CapabilityHost,
    backend: &Backend<'_>,
    ctx: &CapCallContext,
    args: Value,
) -> Result<Value, CtoxError> {
    crate::app_capabilities::protocol::validate_context(ctx)?;
    let mut entered = false;
    let mut effect = || {
        if entered {
            return Err(CtoxError::new(
                ErrorCode::Conflict,
                "command effect already entered",
            ));
        }
        entered = true;
        let r: CommandRequest = decode(&args)?;
        task::identifier(&r.kind)?;
        let target = r.app.as_deref().unwrap_or(&ctx.app_id);
        task::identifier(target)?;
        let sender = backend.artifact(ctx)?;
        for kind in [&r.on_complete, &r.on_failed].into_iter().flatten() {
            task::identifier(kind)?;
            if !sender.handler(kind) {
                return Err(invalid());
            }
        }
        if target != ctx.app_id {
            let receiver = backend.resources.artifact(target, None)?;
            if receiver.app_id != target
                || !permits_ipc(
                    &sender.manifest,
                    &receiver.manifest,
                    &ctx.app_id,
                    target,
                    &r.kind,
                )
            {
                return Err(CtoxError::new(
                    ErrorCode::Forbidden,
                    "cross-app command is not declared by both apps",
                ));
            }
            if !receiver.handler(&r.kind) {
                return Err(invalid());
            }
        } else if !sender.handler(&r.kind) {
            return Err(invalid());
        }
        backend.resources.authorize_command(ctx, target, &r.kind)?;
        let id = format!(
            "cap-command:{}",
            task::digest(&json!([
                ctx.app_id,
                ctx.actor.id,
                ctx.command_id,
                ctx.call_id
            ]))
        );
        let command = AppCommand {
            id: id.clone(),
            app_id: target.into(),
            origin_app: ctx.app_id.clone(),
            actor_id: ctx.actor.id.clone(),
            command_type: r.kind,
            payload: r.payload,
        };
        let intent = json!({"command":command,"on_complete":r.on_complete,"on_failed":r.on_failed});
        let conn = crate::mission::channels::open_channel_db(&crate::paths::core_db(backend.root))
            .map_err(native)?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS capability_issued_commands(id TEXT PRIMARY KEY,app_id TEXT NOT NULL,actor_id TEXT NOT NULL,intent_json TEXT NOT NULL,receipt_json TEXT)").map_err(native)?;
        conn.execute("INSERT OR IGNORE INTO capability_issued_commands(id,app_id,actor_id,intent_json) VALUES(?1,?2,?3,?4)",params![id,ctx.app_id,ctx.actor.id,serde_json::to_string(&intent).map_err(native)?]).map_err(native)?;
        let (original, receipt): (String, Option<String>) = conn
            .query_row(
                "SELECT intent_json,receipt_json FROM capability_issued_commands WHERE id=?1",
                [&id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(native)?;
        if serde_json::from_str::<Value>(&original).map_err(native)? != intent {
            return Err(CtoxError::new(
                ErrorCode::Conflict,
                "command replay changed immutable intent",
            ));
        }
        if let Some(receipt) = receipt {
            return serde_json::from_str(&receipt).map_err(native);
        }
        let receipt = backend.resources.emit_command_once(ctx, &command)?;
        conn.execute(
            "UPDATE capability_issued_commands SET receipt_json=?2 WHERE id=?1",
            params![id, serde_json::to_string(&receipt).map_err(native)?],
        )
        .map_err(native)?;
        Ok(receipt)
    };
    let result = host.with_fence(ctx, "command", "enqueue", &args, &mut effect)?;
    if !entered {
        return Err(CtoxError::new(
            ErrorCode::Unavailable,
            "Core command effect not entered",
        ));
    }
    Ok(result)
}
impl Backend<'_> {
    /// Runtime completion event for an issued command. Result comes from the
    /// native command store, never a caller-supplied terminal-status payload.
    pub fn settle_command(
        &self,
        ctx: &CapCallContext,
        command_id: &str,
    ) -> Result<Value, CtoxError> {
        task::identifier(command_id)?;
        let conn = crate::mission::channels::open_channel_db(&crate::paths::core_db(self.root))
            .map_err(native)?;
        let row: Option<(String, String, String)> = conn
            .query_row(
                "SELECT app_id,actor_id,intent_json FROM capability_issued_commands WHERE id=?1",
                [command_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(native)?;
        let Some((app, actor, intent)) = row else {
            return Err(CtoxError::new(
                ErrorCode::NotFound,
                "issued command not found",
            ));
        };
        if app != ctx.app_id || actor != ctx.actor.id {
            return Err(CtoxError::new(
                ErrorCode::Forbidden,
                "foreign command callback",
            ));
        }
        let Some(terminal) = self.resources.terminal_command(command_id)? else {
            return Ok(json!({"pending":true}));
        };
        if !["completed", "failed", "cancelled"].contains(&terminal.status.as_str()) {
            return Err(invalid());
        }
        for path in &terminal.workspace_refs {
            task::relative(path)?;
        }
        let intent: Value = serde_json::from_str(&intent).map_err(native)?;
        let callback = intent
            .get(if terminal.status == "completed" {
                "on_complete"
            } else {
                "on_failed"
            })
            .and_then(Value::as_str);
        self.callback(ctx,"command",command_id,callback,json!({"command_id":command_id,"status":terminal.status,"result":terminal.result,"workspace_refs":terminal.workspace_refs}))
    }
}
