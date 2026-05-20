//! Document types.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ref: rxdb/src/types/rx-storage.d.ts RxDocumentData<RxDocType>
//
// T1 decision: `RxDocumentData` is a thin alias over `Value`. Documents on
// the rxdb-rs storage path are untyped JSON objects with the protocol fields
// (`_rev`, `_deleted`, `_meta`, `_attachments`) merged in. User code that
// wants stronger typing deserializes from this `Value` at its own layer.
pub type RxDocumentData = Value;

// ref: rxdb/src/types/rx-storage.d.ts RxDocumentWriteData<RxDocType>
pub type RxDocumentWriteData = Value;

// ref: rxdb/src/types/rx-document.d.ts RxDocumentMeta
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct RxDocumentMeta {
    /// last-write time, unix-ms with two decimals
    pub lwt: f64,
}
