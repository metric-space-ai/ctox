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
    // §16.44: draft-only turn {app_id, draft_id, base_draft_rev, prompt, model?, turn_id?}.
    NativeRoute::new(
        "agent.coding",
        "turn",
        "ctox.coding.turn",
        "coding_agents::draft_turn::CodingPanel::execute",
        "src/core/coding_agents/draft_turn.rs",
        PayloadShape::Object,
    ),
    NativeRoute::new(
        "agent.coding",
        "cancel",
        "ctox.coding.cancel",
        "coding_agents::draft_turn::CodingPanel::execute",
        "src/core/coding_agents/draft_turn.rs",
        PayloadShape::Object,
    ),
];
