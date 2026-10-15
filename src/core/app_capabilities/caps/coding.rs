use super::NativeRoute;
use crate::app_capabilities::schema::PayloadShape;

pub(super) const ROUTES: &[NativeRoute] = &[
    NativeRoute::new(
        "agent.coding",
        "models",
        "ctox.coding.models",
        "business_os::store::handle_workspace_control_command",
        "src/core/business_os/store.rs:15623",
        PayloadShape::Object,
    ),
    NativeRoute::new(
        "agent.coding",
        "turn",
        "ctox.coding.turn",
        "business_os::store::handle_workspace_control_command",
        "src/core/business_os/store.rs:15623",
        PayloadShape::Object,
    ),
];
