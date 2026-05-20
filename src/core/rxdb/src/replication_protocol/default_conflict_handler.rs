//! Port of `src/replication-protocol/default-conflict-handler.ts`.

use async_trait::async_trait;
use serde_json::Value;

use crate::plugins::utils::utils_object::flat_clone;
use crate::plugins::utils::utils_object_deep_equal::deep_equal;
use crate::rx_storage_helper::strip_attachments_data_from_document;
use crate::types::{RxConflictHandler, RxConflictHandlerInput};

// ref: rxdb/src/replication-protocol/default-conflict-handler.ts:8-34
pub struct DefaultConflictHandler;

#[async_trait]
impl RxConflictHandler for DefaultConflictHandler {
    async fn is_equal(&self, a: &Value, b: &Value, _ctx: &str) -> bool {
        let a = add_attachments_if_not_exists(a);
        let b = add_attachments_if_not_exists(b);
        // If the documents are deep equal, we have no conflict.
        // On your custom conflict handler you might only check some
        // properties (like updatedAt) for better performance.
        deep_equal(
            &strip_attachments_data_from_document(&a),
            &strip_attachments_data_from_document(&b),
        )
    }

    // ref: rxdb/src/replication-protocol/default-conflict-handler.ts:27-33
    async fn resolve(&self, input: &RxConflictHandlerInput, _ctx: &str) -> Value {
        // The default conflict handler always drops the fork state and
        // uses the master state instead.
        input.real_master_state.clone()
    }
}

// ref: rxdb/src/replication-protocol/default-conflict-handler.ts:37-43
fn add_attachments_if_not_exists(d: &Value) -> Value {
    let mut copy = flat_clone(d);
    if let Some(obj) = copy.as_object_mut() {
        if obj.get("_attachments").is_none() {
            obj.insert(
                "_attachments".to_string(),
                Value::Object(serde_json::Map::new()),
            );
        }
    }
    copy
}
