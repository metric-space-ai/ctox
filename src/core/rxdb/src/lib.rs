//! # rxdb — byte-correct Rust port of RxDB 16.20.0
//!
//! This crate is the Rust side of the CTOX ↔ business-os WebRTC P2P sync.
//! The browser keeps the upstream JS bundle; this crate makes CTOX a peer in
//! the same RxDB replication mesh.
//!
//! Upstream pin: see `vendor/rxdb.version`.
//! Porting tracker: see [`PORTING.md`](../../PORTING.md).
//! Conventions: see [`PORT_STYLE.md`](../../PORT_STYLE.md). All ported code
//! must follow it.
//!
//! The port is built in waves. Modules are added one at a time as their rows
//! in `PORTING.md` move from `pending` to `done`.

#![forbid(unsafe_code)]
#![warn(unused_must_use)]

pub mod change_event_buffer;
pub mod custom_index;
pub mod doc_cache;
pub mod event_reduce;
pub mod hooks;
pub mod incremental_write;
pub mod overwritable;
pub mod plugin;
pub mod plugin_helpers;
pub mod plugins;
pub mod prelude;
pub mod query_cache;
pub mod query_fingerprint;
pub mod query_planner;
pub mod replication_protocol;
pub mod rx_change_event;
pub mod rx_collection;
pub mod rx_collection_helper;
pub mod rx_database;
pub mod rx_database_internal_store;
pub mod rx_document;
pub mod rx_document_prototype_merge;
pub mod rx_error;
pub mod rx_query;
pub mod rx_query_helper;
pub mod rx_query_mingo;
pub mod rx_query_single_result;
pub mod rx_schema;
pub mod rx_schema_helper;
pub mod rx_storage_helper;
pub mod rx_storage_multiinstance;
pub mod rxjs_compat;
pub mod storage;
pub mod types;
pub mod util;
