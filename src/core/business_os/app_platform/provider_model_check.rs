// Origin: CTOX
// License: AGPL-3.0-only
//! Native inference check; the writer admits/records, never waits on a model.
use super::*;
use crate::execution::cliproxyapi_host::model_probe::Probe;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelCheckRequest {
    pub account_id: String,
    pub expected_account_revision: i64,
    pub model_id: String,
}
fn request(payload: &Value) -> Option<ModelCheckRequest> {
    let r: ModelCheckRequest = serde_json::from_value(payload.clone()).ok()?;
    let id = |s: &str| {
        !s.is_empty() && s.len() <= 256 && s.trim() == s && !s.chars().any(char::is_control)
    };
    (id(&r.account_id)
        && id(&r.model_id)
        && (1..9_007_199_254_740_991).contains(&r.expected_account_revision))
    .then_some(r)
}
pub(super) fn prepare(
    tx: &Transaction<'_>,
    actor: &BusinessOsActor,
    command: &CommandDocument,
    now: u64,
) -> Result<Value, StoreError> {
    if request(&command.payload).is_none() || uuid::Uuid::parse_str(&command.id).is_err() {
        return record(tx,actor,command,failure("rejected","invalid","payload_invalid:model_check","payload",
            "Expected account_id, expected_account_revision and model_id; command id must be a UUID."),now);
    }
    record(tx, actor, command, json!({"status":"preparing"}), now)
}
pub(crate) fn finish(
    tx: &Transaction<'_>,
    actor: &BusinessOsActor,
    command: &CommandDocument,
    probe: Probe,
    now: u64,
) -> Result<Value, StoreError> {
    let saved = get(tx, "business_commands", &command.id)?
        .ok_or_else(|| invalid("missing admitted model check"))?;
    if saved["payload"] != command.payload
        || saved["app_id"] != command.app_id
        || saved["module"] != command.module
        || saved["command_type"] != command.command_type
        || saved.pointer("/client_context/actor/id") != Some(&json!(actor.id))
    {
        return Err(invalid("model check identity differs"));
    }
    if saved["type"].is_string() && saved["status"] != "running" {
        return Err(invalid("model check command is no longer running"));
    }
    if saved["result"]["status"] != "preparing" {
        return Ok(saved["result"].clone());
    }
    let r = request(&command.payload).ok_or_else(|| invalid("invalid admitted model check"))?;
    if (saved["type"].is_string()
        && super::super::runtime_host::actor(tx, actor.id.as_deref().unwrap_or(""))? != *actor)
        || !provider_permission(actor, BusinessOsPermission::IntegrationsManage)
    {
        return record(
            tx,
            actor,
            command,
            failure(
                "rejected",
                "forbidden",
                "permission_denied",
                "command_type",
                "Native provider management permission changed.",
            ),
            now,
        );
    }
    if probe.model_id != r.model_id {
        return Err(invalid("model probe identity differs"));
    }
    // Result is public, closed typed metadata. No response text/secret is saved.
    let outcome = json!({"status":"accepted","version":1,"operationId":command.id,
        "action":"instance.providers.models.check","accountId":r.account_id,
        "accountRevision":r.expected_account_revision,"probe":probe});
    record(tx, actor, command, outcome, now)
}
