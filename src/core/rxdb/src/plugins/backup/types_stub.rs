//! Type stubs for the skipped `backup` plugin (gap-item N13).
//!
//! Source: `src/types/plugins/backup.d.ts` + the `RxBackupState` class shape
//! from `src/plugins/backup/index.ts` (the latter is omitted — CTOX MVP does
//! not run an in-process backup loop).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ref: rxdb/src/types/plugins/backup.d.ts:1-18
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackupOptions {
    pub live: bool,
    pub directory: String,
    #[serde(default)]
    pub attachments: bool,
    /// Default: 10.
    #[serde(default, rename = "batchSize", skip_serializing_if = "Option::is_none")]
    pub batch_size: Option<u64>,
    /// `None` ⇒ all collections.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collections: Option<Vec<String>>,
}

// ref: rxdb/src/types/plugins/backup.d.ts:20-28
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RxBackupCollectionState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct BackupMetaFileContent {
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    #[serde(rename = "collectionStates", default)]
    pub collection_states: HashMap<String, RxBackupCollectionState>,
}

// ref: rxdb/src/types/plugins/backup.d.ts:30-35
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RxBackupWriteEvent {
    #[serde(rename = "collectionName")]
    pub collection_name: String,
    #[serde(rename = "documentId")]
    pub document_id: String,
    pub files: Vec<String>,
    pub deleted: bool,
}

// ref: rxdb/src/plugins/backup/index.ts class RxBackupState — fields only.
/// Minimal placeholder so `RxDatabase` surfaces can reference the type.
/// CTOX does not run the backup loop; methods are intentionally absent.
#[derive(Debug)]
pub struct RxBackupState {
    pub options: BackupOptions,
    /// `true` once the live backup has been canceled.
    pub canceled: bool,
}
