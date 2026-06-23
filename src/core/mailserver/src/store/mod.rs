// ref: stalwart/src/store/mod.rs:1-10
// ref: ctox-mailserver new code for exposing sqlite store

pub mod sqlite;
pub mod sqlite_schema;

pub use sqlite::{SqliteStore, SqliteStoreChangeStamp};
