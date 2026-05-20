//! Port of `src/replication-protocol/conflicts.ts`.

use serde_json::Value;

use crate::plugins::utils::utils_object::flat_clone;
use crate::plugins::utils::utils_revision::create_revision;
use crate::plugins::utils::utils_time::now;
use crate::rx_error::RxResult;
use crate::types::{RxConflictHandlerInput, RxStorageInstanceReplicationState};

// ref: rxdb/src/replication-protocol/conflicts.ts:22-63
/// Resolves a conflict error or determines that the given document states are equal.
/// Returns the resolved document that must be written to the fork.
/// If document is not in conflict, returns `Ok(None)`.
///
/// Conflicts are only solved in the upstream, never in the downstream.
pub async fn resolve_conflict_error(
    state: &RxStorageInstanceReplicationState,
    input: &RxConflictHandlerInput,
    fork_state: &Value,
) -> RxResult<Option<Value>> {
    let conflict_handler = &state.input.conflict_handler;

    let is_equal = conflict_handler
        .is_equal(
            &input.real_master_state,
            &input.new_document_state,
            "replication-resolve-conflict",
        )
        .await;

    if is_equal {
        // Documents are equal — not a conflict.
        return Ok(None);
    }

    let resolved = conflict_handler
        .resolve(input, "replication-resolve-conflict")
        .await;

    // Use the resolved document data, but keep the fork's _meta/_attachments
    // because the resolved doc is being written to the fork.
    let mut resolved_doc = resolved;
    if let Some(obj) = resolved_doc.as_object_mut() {
        let fork_meta = fork_state
            .get("_meta")
            .cloned()
            .unwrap_or(Value::Object(serde_json::Map::new()));
        let fork_attachments = fork_state
            .get("_attachments")
            .cloned()
            .unwrap_or(Value::Object(serde_json::Map::new()));
        obj.insert("_meta".to_string(), flat_clone(&fork_meta));
        obj.insert("_rev".to_string(), Value::String(String::new()));
        obj.insert("_attachments".to_string(), flat_clone(&fork_attachments));
        // _meta.lwt = now()
        if let Some(meta_obj) = obj
            .get_mut("_meta")
            .and_then(|v: &mut Value| v.as_object_mut())
        {
            meta_obj.insert("lwt".to_string(), serde_json::json!(now()));
        }
        // _rev = create_revision(checkpoint_key, fork_state)
        let prev_rev = fork_state.get("_rev").and_then(|v| v.as_str());
        let rev = create_revision(&state.checkpoint_key, prev_rev).unwrap_or_default();
        obj.insert("_rev".to_string(), Value::String(rev));
    }
    Ok(Some(resolved_doc))
}
