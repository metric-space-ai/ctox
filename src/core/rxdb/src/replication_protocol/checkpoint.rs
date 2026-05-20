//! Port of `src/replication-protocol/checkpoint.ts`.

use serde_json::{json, Value};

use crate::plugins::utils::utils_document::{get_default_revision, get_default_rx_document_meta};
use crate::plugins::utils::utils_revision::create_revision;
use crate::plugins::utils::utils_time::now;
use crate::rx_error::{new_rx_error, RxError, RxResult};
use crate::rx_schema_helper::get_composed_primary_key_of_document_data;
use crate::rx_storage_helper::{get_written_documents_from_bulk_write_response, stack_checkpoints};
use crate::types::{
    BulkWriteRow, RxStorageInstanceReplicationInput, RxStorageInstanceReplicationState,
    RxStorageReplicationDirection,
};

// ref: rxdb/src/replication-protocol/checkpoint.ts:18-43
pub async fn get_last_checkpoint_doc(
    state: &RxStorageInstanceReplicationState,
    direction: RxStorageReplicationDirection,
) -> RxResult<Option<Value>> {
    let checkpoint_doc_id = get_composed_primary_key_of_document_data(
        state.input.meta_instance.schema(),
        &json!({
            "isCheckpoint": "1",
            "itemId": direction.as_str(),
        }),
    )?;
    let checkpoint_result = state
        .input
        .meta_instance
        .find_documents_by_id(&[checkpoint_doc_id], false)
        .await?;

    let checkpoint_doc = checkpoint_result.into_iter().next();
    if let Some(ref doc) = checkpoint_doc {
        state
            .last_checkpoint_doc
            .lock()
            .insert(direction, doc.clone());
        Ok(doc.get("checkpointData").cloned())
    } else {
        Ok(None)
    }
}

// ref: rxdb/src/replication-protocol/checkpoint.ts:46-143
/// Sets the checkpoint, automatically resolving conflicts that appear.
pub async fn set_checkpoint(
    state: &RxStorageInstanceReplicationState,
    direction: RxStorageReplicationDirection,
    checkpoint: Value,
) -> RxResult<()> {
    // Upstream serializes via `state.checkpointQueue = state.checkpointQueue.then(...)`.
    // We use a tokio::sync::Mutex to serialize.
    let _guard = state.checkpoint_queue.lock().await;
    let mut previous_checkpoint_doc = state.last_checkpoint_doc.lock().get(&direction).cloned();

    let canceled = state.events.canceled.get_value();
    let is_changed = match &previous_checkpoint_doc {
        None => true,
        Some(prev) => {
            let prev_cp = prev.get("checkpointData").cloned().unwrap_or(Value::Null);
            serde_json::to_string(&prev_cp).ok() != serde_json::to_string(&checkpoint).ok()
        }
    };

    if checkpoint.is_null() || canceled || !is_changed {
        return Ok(());
    }

    // Build the new meta doc.
    let mut new_doc = json!({
        "id": "",
        "isCheckpoint": "1",
        "itemId": direction.as_str(),
        "_deleted": false,
        "_attachments": {},
        "checkpointData": checkpoint,
        "_meta": get_default_rx_document_meta(),
        "_rev": get_default_revision(),
    });

    // id = composed primary key from meta schema.
    let composed =
        get_composed_primary_key_of_document_data(state.input.meta_instance.schema(), &new_doc)?;
    if let Some(obj) = new_doc.as_object_mut() {
        obj.insert("id".to_string(), Value::String(composed));
    }

    loop {
        if state.events.canceled.get_value() {
            return Ok(());
        }

        // Stack checkpoint over previous (sharding-plugin support).
        if let Some(prev) = previous_checkpoint_doc.as_ref() {
            let prev_cp = prev.get("checkpointData").cloned().unwrap_or(Value::Null);
            let new_cp_inner = new_doc
                .get("checkpointData")
                .cloned()
                .unwrap_or(Value::Null);
            let stacked = stack_checkpoints(&[Some(prev_cp), Some(new_cp_inner)]);
            if let Some(obj) = new_doc.as_object_mut() {
                obj.insert("checkpointData".to_string(), stacked);
            }
        }
        if let Some(obj) = new_doc.as_object_mut() {
            if let Some(meta_obj) = obj.get_mut("_meta").and_then(|v| v.as_object_mut()) {
                meta_obj.insert("lwt".to_string(), json!(now()));
            }
            let prev_rev = previous_checkpoint_doc
                .as_ref()
                .and_then(|p| p.get("_rev"))
                .and_then(|v| v.as_str());
            let rev = create_revision(&state.checkpoint_key, prev_rev).unwrap_or_default();
            obj.insert("_rev".to_string(), Value::String(rev));
        }

        if state.events.canceled.get_value() {
            return Ok(());
        }

        let write_rows = vec![BulkWriteRow {
            previous: previous_checkpoint_doc.clone(),
            document: new_doc.clone(),
        }];
        let result = state
            .input
            .meta_instance
            .bulk_write(write_rows.clone(), "replication-set-checkpoint")
            .await?;
        let success = get_written_documents_from_bulk_write_response(
            &state.primary_path,
            &write_rows,
            &result,
            None,
        );
        if let Some(success_doc) = success.into_iter().next() {
            state
                .last_checkpoint_doc
                .lock()
                .insert(direction, success_doc);
            return Ok(());
        }
        let error = match result.error.first() {
            Some(e) => e.clone(),
            None => {
                return Err(new_rx_error(
                    "SNH",
                    Some(
                        json!({ "message": "checkpoint write produced neither success nor error" }),
                    ),
                ));
            }
        };
        if error.status != 409 {
            return Err(new_rx_error(
                "STO19",
                Some(json!({
                    "message": "non-conflict storage error during set_checkpoint",
                    "writeError": serde_json::to_value(&error).unwrap_or(Value::Null),
                })),
            ));
        }
        // 409 conflict — use the docInDb as the new previous and retry.
        let in_db = match error.document_in_db {
            Some(d) => d,
            None => {
                return Err(new_rx_error(
                    "SNH",
                    Some(json!({ "message": "409 conflict without documentInDb" })),
                ));
            }
        };
        previous_checkpoint_doc = Some(in_db);
        let prev_rev = previous_checkpoint_doc
            .as_ref()
            .and_then(|p| p.get("_rev"))
            .and_then(|v| v.as_str());
        let rev = create_revision(&state.checkpoint_key, prev_rev).unwrap_or_default();
        if let Some(obj) = new_doc.as_object_mut() {
            obj.insert("_rev".to_string(), Value::String(rev));
        }
    }
}

// ref: rxdb/src/replication-protocol/checkpoint.ts:145-154
pub async fn get_checkpoint_key(input: &RxStorageInstanceReplicationInput) -> String {
    let combined = format!(
        "{}||{}||{}",
        input.identifier,
        input.fork_instance.database_name(),
        input.fork_instance.collection_name(),
    );
    let hash = input.hash_function.hash(combined).await;
    format!("rx_storage_replication_{hash}")
}

// quiet unused warning if any
#[allow(dead_code)]
fn _phantom_err_clone() -> RxError {
    new_rx_error("SNH", None)
}
