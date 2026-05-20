//! Port of `src/plugins/attachments/attachments-utils.ts:fillWriteDataForAttachmentsChange`.
//!
//! CTOX represents attachments as out-of-band Parquet files referenced by hash
//! in document data, so the upstream "fill missing attachment.data by reading
//! from storage" behavior is irrelevant. The function shape is preserved so
//! callers in `replication-protocol/{upstream,index}.ts` can be ported without
//! conditional compilation, but it is effectively a no-op: it returns the
//! document unchanged.

use std::sync::Arc;

use serde_json::Value;

use crate::rx_error::{new_rx_error, RxResult};
use crate::types::RxStorageInstance;

// ref: rxdb/src/plugins/attachments/attachments-utils.ts:34-82
/// CTOX stub: returns `new_document` unchanged.
///
/// Upstream walks `new_document._attachments`, compares digests against
/// `original_document._attachments`, and for each newly-added or changed
/// attachment without an inline `.data` field fetches the data via
/// `storage_instance.get_attachment_data(...)`. CTOX does not store
/// attachments inline — Parquet files live next to the SQLite store and
/// callers transfer them out-of-band by hash. The hook is preserved so the
/// replication-protocol call sites do not need feature-gating.
pub async fn fill_write_data_for_attachments_change(
    _primary_path: &str,
    _storage_instance: Arc<dyn RxStorageInstance>,
    new_document: Value,
    original_document: Option<&Value>,
) -> RxResult<Value> {
    let new_atts = new_document.get("_attachments");
    let orig_atts = original_document.and_then(|d| d.get("_attachments"));
    if new_atts.is_none() || (original_document.is_some() && orig_atts.is_none()) {
        return Err(new_rx_error(
            "AT_FILL",
            Some(serde_json::json!({
                "message": "_attachments missing on document passed to fill_write_data_for_attachments_change",
            })),
        ));
    }
    Ok(new_document)
}
