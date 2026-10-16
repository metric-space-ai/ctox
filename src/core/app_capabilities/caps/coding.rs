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
    NativeRoute::new(
        "agent.coding",
        "turn",
        "ctox.coding.turn",
        "coding_agents::draft_turn::CodingPanel::execute (§16.44, #848)",
        "src/core/coding_agents/draft_turn.rs",
        PayloadShape::Object,
    ),
];

/// §16.30 Nr. 5 + §16.44: flat JS arguments become the internal CAP-1
/// envelope. `turn` = `{app_id, draft_id, base_draft_rev, prompt, model?,
/// turn_id?}` on the caller's own source draft, `cancel` = `{turn_id}`,
/// `models` = `{module_id}`. Anything else (in particular the V3 shape with
/// `module_id`/`faux`/`preset_id`, or a pre-built envelope for turn/cancel)
/// is never enveloped: the original args reach the registry unchanged and are
/// rejected there as invalid (and audited).
pub(crate) fn flat_envelope(function: &str, args: Value) -> Value {
    let (allowed, required): (&[&str], &[&str]) = match function {
        "models" => (&["module_id"], &["module_id"]),
        "turn" => (
            &[
                "app_id",
                "draft_id",
                "base_draft_rev",
                "prompt",
                "model",
                "turn_id",
            ],
            &["app_id", "draft_id", "base_draft_rev", "prompt"],
        ),
        "cancel" => (&["turn_id"], &["turn_id"]),
        _ => return args,
    };
    let Some(object) = args.as_object() else {
        return args;
    };
    if object.contains_key("payload") {
        // CAP-1 envelope: accepted for models; never for the §16.44 draft
        // commands (an envelope could carry the V3 shape past this check).
        return if function == "models" {
            args
        } else {
            json!({ "rejected_shape": args })
        };
    }
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return args;
    }
    let text = |key: &str, max: usize| {
        object.get(key).map_or(true, |v| {
            v.as_str()
                .is_some_and(|t| !t.trim().is_empty() && t.len() <= max && !t.contains('\0'))
        })
    };
    let ids_ok = [
        "module_id",
        "app_id",
        "draft_id",
        "base_draft_rev",
        "model",
        "turn_id",
    ]
    .into_iter()
    .all(|key| text(key, 256));
    if required.iter().any(|key| !object.contains_key(*key))
        || !ids_ok
        || !text("prompt", 64 * 1024)
    {
        return args;
    }
    let record = ["module_id", "draft_id", "turn_id"]
        .into_iter()
        .find_map(|key| object.get(key).and_then(Value::as_str))
        .map(str::to_owned);
    match record {
        Some(record_id) => json!({ "payload": args, "record_id": record_id }),
        None => json!({ "payload": args }),
    }
}
