//! Port of `rxdb/src/plugins/attachments/` — stubs only.
//!
//! CTOX uses out-of-band Parquet dataframes; the upstream attachments plugin
//! (interactive base64 encode/decode, RxAttachment object) is out of scope.
//! Only the wire-format hooks that replication-protocol references are ported.

pub mod stub;
pub mod types_stub;

pub use stub::fill_write_data_for_attachments_change;
