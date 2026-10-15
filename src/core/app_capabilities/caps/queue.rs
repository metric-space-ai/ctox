//! Queue controls reuse the native transactions and lease CAS.
use super::task::{self, decode, invalid, native, route, Backend};
use super::NativeRoute;
use crate::app_capabilities::{CapCallContext, CtoxError, ErrorCode};
use crate::mission::channels::{self, QueueTaskUpdateRequest, QueueTaskView};
use serde::Deserialize;
use serde_json::{json, Value};
use std::cell::RefCell;

pub(super) const ROUTES: &[NativeRoute] = &[
    route(
        "agent.queue",
        "abort_turn",
        "ctox.queue.abort_turn",
        "harness_cockpit::control",
        "src/core/business_os/harness_cockpit.rs:84",
    ),
    route(
        "agent.queue",
        "block",
        "ctox.queue.block",
        "harness_cockpit::control",
        "src/core/business_os/harness_cockpit.rs:84",
    ),
    route(
        "agent.queue",
        "capacity",
        "ctox.queue.capacity",
        "harness_cockpit::control",
        "src/core/business_os/harness_cockpit.rs:84",
    ),
    route(
        "agent.queue",
        "pause",
        "ctox.queue.pause",
        "harness_cockpit::control",
        "src/core/business_os/harness_cockpit.rs:84",
    ),
    route(
        "agent.queue",
        "release",
        "ctox.queue.release",
        "harness_cockpit::control",
        "src/core/business_os/harness_cockpit.rs:84",
    ),
    route(
        "agent.queue",
        "retry",
        "ctox.queue.retry",
        "harness_cockpit::control",
        "src/core/business_os/harness_cockpit.rs:84",
    ),
    route(
        "agent.queue",
        "task.get",
        "cap.queue.get",
        "channels::load_queue_task",
        "src/core/mission/channels/mod.rs:3745",
    ),
    route(
        "agent.queue",
        "task.for_command",
        "cap.queue.for_command",
        "channels::load_queue_task",
        "src/core/mission/channels/mod.rs:3745",
    ),
    route(
        "agent.queue",
        "task.claim",
        "cap.queue.claim",
        "channels::lease_queue_task_if",
        "src/core/mission/channels/mod.rs:4152",
    ),
    route(
        "agent.queue",
        "task.update",
        "cap.queue.update",
        "channels::update_queue_task",
        "src/core/mission/channels/mod.rs:3773",
    ),
    route(
        "agent.queue",
        "task.set_status",
        "cap.queue.set_status",
        "channels::update_queue_task",
        "src/core/mission/channels/mod.rs:3773",
    ),
    route(
        "agent.queue",
        "task.delete",
        "cap.queue.delete",
        "channels::update_queue_task",
        "src/core/mission/channels/mod.rs:3773",
    ),
];
#[derive(Clone)]
struct Predicate {
    ctx: CapCallContext,
    statuses: Vec<String>,
}
thread_local! {static CLAIM:RefCell<Option<Predicate>>=const {RefCell::new(None)};}
struct Reset;
impl Drop for Reset {
    fn drop(&mut self) {
        CLAIM.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}
fn eligible(task: &QueueTaskView) -> bool {
    CLAIM.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|p| task::owns(&p.ctx, task) && p.statuses.contains(&task.route_status))
    })
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Command {
    command_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Claim {
    task_id: String,
    holder: String,
    route_status_in: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    task_id: String,
    title: Option<String>,
    #[serde(alias = "prompt")]
    instruction: Option<String>,
    priority: Option<String>,
    status_note: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Status {
    task_id: String,
    status: String,
    status_note: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Delete {
    task_id: String,
    status_note: Option<String>,
}
pub(super) fn execute(
    backend: &Backend<'_>,
    ctx: &CapCallContext,
    function: &str,
    args: &Value,
) -> Result<Value, CtoxError> {
    match function {
        "task.get" => {
            let r: task::TaskId = decode(args)?;
            Ok(backend.view(&backend.owned(ctx, &r.task_id)?))
        }
        "task.for_command" => {
            let r: Command = decode(args)?;
            task::identifier(&r.command_id)?;
            Ok(backend
                .find(ctx, None, Some(&r.command_id))?
                .as_ref()
                .map(|t| backend.view(t))
                .unwrap_or_else(|| json!({"__cap4_not_found":true})))
        }
        "task.claim" => {
            let r: Claim = decode(args)?;
            task::identifier(&r.holder)?;
            backend.owned(ctx, &r.task_id)?;
            if r.route_status_in.is_empty() || r.route_status_in.len() > 16 {
                return Err(invalid());
            }
            for status in &r.route_status_in {
                task::identifier(status)?;
            }
            let already = CLAIM.with(|slot| {
                let mut slot = slot.borrow_mut();
                if slot.is_some() {
                    true
                } else {
                    *slot = Some(Predicate {
                        ctx: ctx.clone(),
                        statuses: r.route_status_in,
                    });
                    false
                }
            });
            if already {
                return Err(CtoxError::new(ErrorCode::Conflict, "nested queue claim"));
            }
            let _reset = Reset;
            let holder = format!(
                "cap:{}",
                task::digest(&json!([ctx.app_id, ctx.actor.id, r.holder]))
            );
            let task = channels::lease_queue_task_if(backend.root, &r.task_id, &holder, eligible)
                .map_err(native)?;
            Ok(backend.view(&task))
        }
        "task.update" => {
            let r: Change = decode(args)?;
            backend.owned(ctx, &r.task_id)?;
            let task = channels::update_queue_task(
                backend.root,
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
            Ok(backend.view(&task))
        }
        "task.set_status" => {
            let r: Status = decode(args)?;
            backend.owned(ctx, &r.task_id)?;
            let task = channels::update_queue_task(
                backend.root,
                QueueTaskUpdateRequest {
                    message_key: r.task_id,
                    route_status: Some(r.status),
                    status_note: r.status_note,
                    ..Default::default()
                },
            )
            .map_err(native)?;
            backend.settle_task(ctx, &task)?;
            Ok(backend.view(&task))
        }
        "task.delete" => {
            let r: Delete = decode(args)?;
            backend.owned(ctx, &r.task_id)?;
            let task = channels::update_queue_task(
                backend.root,
                QueueTaskUpdateRequest {
                    message_key: r.task_id.clone(),
                    route_status: Some("cancelled".into()),
                    status_note: r.status_note,
                    ..Default::default()
                },
            )
            .map_err(native)?;
            backend.settle_task(ctx, &task)?;
            Ok(
                json!({"task_id":r.task_id,"status":"cancelled","deleted":true,"history_retained":true}),
            )
        }
        _ => Err(CtoxError::new(
            ErrorCode::NotFound,
            "unknown queue primitive",
        )),
    }
}
