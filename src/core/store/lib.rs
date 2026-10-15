//! SQLite ownership and bounded jobs for CTOX Sync v4.
//!
//! Connections never leave their owning blocking thread. Legacy callers are
//! migrated by later adapter packages; declaring this crate does not retire them.

pub mod change_log;
pub mod config;
pub mod ipc;
pub mod publication;
pub mod read_pool;
pub mod registry;
pub mod rxdb_schema_adapter;
pub mod schema;
pub mod testkit;
pub mod types;
pub mod writer;

pub use types::{
    Cancellation, DbGeneration, DbId, JobHandle, JobOptions, ReadJob, StoreError, WriteJob,
};
