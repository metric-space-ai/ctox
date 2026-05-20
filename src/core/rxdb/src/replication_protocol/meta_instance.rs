//! Port of `src/replication-protocol/meta-instance.ts`.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::plugins::utils::utils_document::get_default_revision;
use crate::plugins::utils::utils_revision::create_revision;
use crate::plugins::utils::utils_time::now;
use crate::rx_error::RxResult;
use crate::rx_schema_helper::{
    fill_with_default_settings, get_composed_primary_key_of_document_data,
    get_length_of_primary_key,
};
use crate::rx_storage_helper::flat_clone_doc_with_meta;
use crate::types::{
    BulkWriteRow, CompositePrimaryKey, JsonSchema, PrimaryKey, RxJsonSchema,
    RxStorageInstanceReplicationState,
};

// ref: rxdb/src/replication-protocol/meta-instance.ts:23
pub const META_INSTANCE_SCHEMA_TITLE: &str = "RxReplicationProtocolMetaData";

// ref: rxdb/src/replication-protocol/meta-instance.ts:25-92
/// Builds the replication-meta schema for a given replicated-documents schema.
pub fn get_rx_replication_meta_instance_schema(
    replicated_documents_schema: &RxJsonSchema,
    encrypted: bool,
) -> RxResult<RxJsonSchema> {
    let parent_primary_key_length = get_length_of_primary_key(replicated_documents_schema)?;

    let mut properties: HashMap<String, JsonSchema> = HashMap::new();
    properties.insert(
        "id".to_string(),
        JsonSchema {
            schema_type: Some("string".to_string()),
            min_length: Some(1),
            // +1 for '|' and +1 for the 'isCheckpoint' flag
            max_length: Some(parent_primary_key_length + 2),
            ..Default::default()
        },
    );
    properties.insert(
        "isCheckpoint".to_string(),
        JsonSchema {
            schema_type: Some("string".to_string()),
            min_length: Some(1),
            max_length: Some(1),
            ..Default::default()
        },
    );
    properties.insert(
        "itemId".to_string(),
        JsonSchema {
            schema_type: Some("string".to_string()),
            // Ensure all values of RxStorageReplicationDirection ('DOWN' has 4 chars) fit.
            max_length: Some(if parent_primary_key_length > 4 {
                parent_primary_key_length
            } else {
                4
            }),
            ..Default::default()
        },
    );
    properties.insert(
        "checkpointData".to_string(),
        JsonSchema {
            schema_type: Some("object".to_string()),
            additional_properties: Some(true),
            ..Default::default()
        },
    );
    properties.insert(
        "docData".to_string(),
        JsonSchema {
            schema_type: Some("object".to_string()),
            properties: replicated_documents_schema.properties.clone(),
            ..Default::default()
        },
    );
    properties.insert(
        "isResolvedConflict".to_string(),
        JsonSchema {
            schema_type: Some("string".to_string()),
            ..Default::default()
        },
    );

    let mut encrypted_fields: Vec<String> = Vec::new();
    if encrypted {
        encrypted_fields.push("docData".to_string());
    }

    let base_schema = RxJsonSchema {
        version: replicated_documents_schema.version,
        primary_key: PrimaryKey::Composite(CompositePrimaryKey {
            key: "id".to_string(),
            fields: vec!["itemId".to_string(), "isCheckpoint".to_string()],
            separator: "|".to_string(),
        }),
        schema_type: "object".to_string(),
        properties,
        required: vec![
            "id".to_string(),
            "isCheckpoint".to_string(),
            "itemId".to_string(),
        ],
        indexes: Vec::new(),
        encrypted: encrypted_fields,
        internal_indexes: Vec::new(),
        key_compression: replicated_documents_schema.key_compression,
        attachments: replicated_documents_schema.attachments.clone(),
        additional_properties: false,
        extra: {
            let mut m = HashMap::new();
            m.insert(
                "title".to_string(),
                Value::String(META_INSTANCE_SCHEMA_TITLE.to_string()),
            );
            m
        },
    };
    Ok(fill_with_default_settings(base_schema))
}

// ref: rxdb/src/replication-protocol/meta-instance.ts:100-137
/// Returns the document states of what the fork instance assumes to be the
/// latest state on the master instance.
pub async fn get_assumed_master_state(
    state: &RxStorageInstanceReplicationState,
    doc_ids: &[String],
) -> RxResult<HashMap<String, AssumedMaster>> {
    let mut composed_ids: Vec<String> = Vec::with_capacity(doc_ids.len());
    for doc_id in doc_ids.iter() {
        let probe = json!({ "itemId": doc_id, "isCheckpoint": "0" });
        let id =
            get_composed_primary_key_of_document_data(state.input.meta_instance.schema(), &probe)?;
        composed_ids.push(id);
    }
    let meta_docs = state
        .input
        .meta_instance
        .find_documents_by_id(&composed_ids, true)
        .await?;
    let mut ret: HashMap<String, AssumedMaster> = HashMap::new();
    for meta_doc in meta_docs.into_iter() {
        let Some(item_id) = meta_doc.get("itemId").and_then(|v| v.as_str()) else {
            continue;
        };
        let doc_data = meta_doc.get("docData").cloned().unwrap_or(Value::Null);
        ret.insert(
            item_id.to_string(),
            AssumedMaster {
                doc_data,
                meta_document: meta_doc.clone(),
            },
        );
    }
    Ok(ret)
}

#[derive(Debug, Clone)]
pub struct AssumedMaster {
    pub doc_data: Value,
    pub meta_document: Value,
}

// ref: rxdb/src/replication-protocol/meta-instance.ts:140-188
pub async fn get_meta_write_row(
    state: &RxStorageInstanceReplicationState,
    new_master_doc_state: &Value,
    previous: Option<&Value>,
    is_resolved_conflict: Option<&str>,
) -> RxResult<BulkWriteRow> {
    let doc_id = new_master_doc_state
        .get(&state.primary_path)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let mut new_meta = if let Some(prev) = previous {
        flat_clone_doc_with_meta(prev)
    } else {
        json!({
            "id": "",
            "isCheckpoint": "0",
            "itemId": doc_id,
            "docData": new_master_doc_state,
            "_attachments": {},
            "_deleted": false,
            "_rev": get_default_revision(),
            "_meta": { "lwt": 0 },
        })
    };
    if let Some(obj) = new_meta.as_object_mut() {
        obj.insert("docData".to_string(), new_master_doc_state.clone());
        if let Some(flag) = is_resolved_conflict {
            obj.insert(
                "isResolvedConflict".to_string(),
                Value::String(flag.to_string()),
            );
        }
        if let Some(meta_obj) = obj.get_mut("_meta").and_then(|v| v.as_object_mut()) {
            meta_obj.insert("lwt".to_string(), json!(now()));
        }
        let composed = get_composed_primary_key_of_document_data(
            state.input.meta_instance.schema(),
            &new_meta,
        )?;
        // Refresh `obj` borrow after the composed-key computation.
        if let Some(obj2) = new_meta.as_object_mut() {
            obj2.insert("id".to_string(), Value::String(composed));
            let prev_rev = previous
                .and_then(|p| p.get("_rev"))
                .and_then(|v| v.as_str());
            let rev = create_revision(&state.checkpoint_key, prev_rev).unwrap_or_default();
            obj2.insert("_rev".to_string(), Value::String(rev));
        }
    }
    Ok(BulkWriteRow {
        previous: previous.cloned(),
        document: new_meta,
    })
}
