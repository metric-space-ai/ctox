//! `agent.flow.record` — harness process evidence (MAPPING (c) platform).
//! Only platform callers may append to the harness flow ledger; the event is
//! stamped with the Core-bound caller under `metadata._cap`.
use super::*;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    event_kind: String,
    title: String,
    #[serde(default)]
    body_text: String,
    message_key: Option<String>,
    work_id: Option<String>,
    ticket_key: Option<String>,
    attempt_index: Option<i64>,
    #[serde(default)]
    metadata: Value,
}

pub(super) fn execute(root: &Path, ctx: &CapCallContext, args: &Value) -> Result<Value, CtoxError> {
    if !is_platform(ctx) {
        return Err(forbidden("agent.flow.record is a platform capability"));
    }
    let request: Record = decode(args)?;
    text(&request.event_kind, 128, false, "$.event_kind")?;
    text(&request.title, 512, false, "$.title")?;
    if request.body_text.len() > 16 * 1024 {
        return Err(invalid("$.body_text"));
    }
    for (path, value) in [
        ("$.message_key", &request.message_key),
        ("$.work_id", &request.work_id),
        ("$.ticket_key", &request.ticket_key),
    ] {
        if let Some(value) = value {
            crate::app_capabilities::protocol::identifier(value, path)?;
        }
    }
    let mut metadata = match request.metadata {
        Value::Null => serde_json::Map::new(),
        Value::Object(map) => map,
        _ => return Err(invalid("$.metadata")),
    };
    if metadata.contains_key("_cap") || json_size(&Value::Object(metadata.clone())) > 16 * 1024 {
        return Err(invalid("$.metadata"));
    }
    metadata.insert(
        "_cap".into(),
        json!({"app_id":ctx.app_id,"actor_id":ctx.actor.id,"command_id":ctx.command_id,"call_id":ctx.call_id}),
    );
    let event = crate::service::harness_flow::record_harness_flow_event(
        root,
        crate::service::harness_flow::RecordHarnessFlowEventRequest {
            event_kind: &request.event_kind,
            title: &request.title,
            body_text: &request.body_text,
            message_key: request.message_key.as_deref(),
            work_id: request.work_id.as_deref(),
            ticket_key: request.ticket_key.as_deref(),
            attempt_index: request.attempt_index,
            metadata: Value::Object(metadata),
        },
    )
    .map_err(native_error)?;
    Ok(json!({"event_id":event.event_id,"chain_key":event.chain_key}))
}
