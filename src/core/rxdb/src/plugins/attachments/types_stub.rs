//! Type stubs for the skipped `attachments` plugin (gap-item N11).
//!
//! Replication and a few helpers reference `RxAttachment*` types. The real
//! attachment runtime (encode/decode/RxAttachment object methods) is not
//! ported — CTOX uses out-of-band Parquet files. These typed stubs let other
//! modules compile against the upstream shape without dragging in the plugin.
//!
//! Source: `src/types/rx-attachment.d.ts` — the data-only fields are kept;
//! method shapes (`getData`, `getStringData`) are intentionally omitted.

pub use crate::types::storage::{RxAttachmentData, RxAttachmentWriteData};

// ref: rxdb/src/types/rx-attachment.d.ts:11-15
/// Minimal placeholder for the upstream `RxAttachment` object. CTOX does not
/// expose the interactive attachment runtime, so the user-facing methods
/// (`getData`, `getStringData`, `remove`) are absent. The field surface
/// matches `RxAttachmentData`.
pub type RxAttachment = RxAttachmentData;
