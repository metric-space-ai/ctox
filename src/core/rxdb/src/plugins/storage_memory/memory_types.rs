//! Port of `src/plugins/storage-memory/memory-types.ts`.

use std::collections::HashMap;

use parking_lot::Mutex;
use serde_json::Value;

use crate::rxjs_compat::RxSubject;
use crate::types::{EventBulk, RxAttachmentWriteData, RxJsonSchema};

// ref: rxdb/src/plugins/storage-memory/memory-types.ts:13-14
pub type RxStorageMemorySettings = ();
pub type RxStorageMemoryInstanceCreationOptions = ();

// ref: rxdb/src/plugins/storage-memory/memory-types.ts:22-26
pub struct MemoryStorageInternalsByIndex {
    pub index: Vec<String>,
    pub docs_with_index: Vec<DocWithIndexString>,
    pub get_indexable_string: Box<dyn Fn(&Value) -> String + Send + Sync>,
}

impl std::fmt::Debug for MemoryStorageInternalsByIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryStorageInternalsByIndex")
            .field("index", &self.index)
            .field("docs_with_index", &self.docs_with_index)
            .field("get_indexable_string", &"<fn>")
            .finish()
    }
}

// ref: rxdb/src/plugins/storage-memory/memory-types.ts:32-84
/// The internals are shared between multiple storage instances that have been
/// created with the same `[databaseName + collectionName]` combination.
pub struct MemoryStorageInternals {
    pub id: String,
    pub schema: RxJsonSchema,
    /// We reuse the memory state when multiple instances are created with the
    /// same params. If `ref_count` becomes 0, we can delete the state.
    pub ref_count: u32,
    /// If this becomes true, an instance has called `remove()` and all other
    /// instances should also not work anymore.
    pub removed: bool,
    pub documents: HashMap<String, Value>,
    /// Attachments data, indexed by a combined string consisting of
    /// `[documentId + '||' + attachmentId]`.
    pub attachments: HashMap<String, MemoryAttachment>,
    pub by_index: HashMap<String, MemoryStorageInternalsByIndex>,
    pub changes_subject: RxSubject<EventBulk>,
}

// ref: rxdb/src/plugins/storage-memory/memory-types.ts:60-63
#[derive(Debug, Clone)]
pub struct MemoryAttachment {
    pub write_data: RxAttachmentWriteData,
    pub digest: String,
}

// ref: rxdb/src/plugins/storage-memory/memory-types.ts:86-91
/// A `(index_string, document, id)` triple. `index_string` is first because we
/// often only need that one when doing binary searches.
#[derive(Debug, Clone)]
pub struct DocWithIndexString {
    pub index_string: String,
    pub document: Value,
    pub id: String,
}

/// Shared, refcounted handle to a [`MemoryStorageInternals`] state.
///
/// Upstream stores `MemoryStorageInternals<RxDocType>` directly inside a JS
/// `Map<collectionKey, ...>`. Multiple `RxStorageInstance` views can read and
/// write the same state concurrently. In Rust we wrap it in `Arc<Mutex<...>>`
/// to satisfy the shared-mutable-state contract.
pub type SharedMemoryStorageInternals = std::sync::Arc<Mutex<MemoryStorageInternals>>;
