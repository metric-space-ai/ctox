//! SQLite storage backend for CTOX's Rust RxDB peer.
//!
//! This is the Rust-side counterpart to the browser's IndexedDB/Dexie
//! storage. It implements the standard [`crate::types::RxStorage`] trait and
//! stores RxDB document JSON unchanged in SQLite, with indexed metadata columns
//! for primary-key lookup, cleanup and checkpoint scans.

pub mod cleanup;
pub mod index_mod;
pub mod instance;
pub mod sql;
pub mod types;

pub use index_mod::{create_storage_instance, get_rx_storage_sqlite, RX_STORAGE_NAME_SQLITE};
pub use types::{RxStorageSqlite, RxStorageSqliteSettings, SQLITE_IN_MEMORY_DB_NAME};
