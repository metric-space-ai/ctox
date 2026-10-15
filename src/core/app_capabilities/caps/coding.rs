use super::NativeRoute;
use crate::app_capabilities::schema::PayloadShape;
use serde_json::{json, Value};

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

/// §16.30 Nr. 5 (CAP-3): flat `agent.coding.models({module_id})` and
/// `agent.coding.turn({module_id, prompt?, faux?, preset_id?})` become the
/// internal CAP-1 envelope for the native pi-sidecar command. A raw `model`
/// override or any unknown field is never enveloped: the original args reach
/// the registry unchanged and are rejected there as invalid (and audited).
/// An already enveloped CAP-1 call (`{payload, record_id?}`) passes through.
pub(crate) fn flat_envelope(function: &str, args: Value) -> Value {
    let allowed: &[&str] = match function {
        "models" => &["module_id"],
        "turn" => &["module_id", "prompt", "faux", "preset_id"],
        _ => return args,
    };
    let Some(object) = args.as_object() else {
        return args;
    };
    if object.contains_key("payload") {
        return args;
    }
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return args;
    }
    let Some(module_id) = object.get("module_id").and_then(Value::as_str) else {
        return args;
    };
    let slug = !module_id.is_empty()
        && module_id.len() <= 128
        && module_id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
    let prompt_ok = object
        .get("prompt")
        .map_or(true, |v| v.as_str().is_some_and(|p| p.len() <= 64 * 1024));
    let faux_ok = object.get("faux").map_or(true, Value::is_boolean);
    let preset_ok = object.get("preset_id").map_or(true, |v| {
        v.as_str()
            .is_some_and(|p| !p.is_empty() && p.len() <= 128 && !p.chars().any(char::is_control))
    });
    if !(slug && prompt_ok && faux_ok && preset_ok) {
        return args;
    }
    let record_id = module_id.to_owned();
    json!({ "payload": args, "record_id": record_id })
}
