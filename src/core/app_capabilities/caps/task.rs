//! Generic task route contract, independent of daemon dependencies.
use super::NativeRoute;
use crate::app_capabilities::schema::PayloadShape;
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
        "src/core/mission/channels/mod.rs:3874",
    ),
    route(
        "agent.task",
        "cancel",
        "cap.task.cancel",
        "channels::update_queue_task",
        "src/core/mission/channels/mod.rs:3874",
    ),
    route(
        "agent.task",
        "set_metadata",
        "cap.task.set_metadata",
        "channels::set_queue_task_metadata_value",
        "src/core/mission/channels/mod.rs:4196",
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
