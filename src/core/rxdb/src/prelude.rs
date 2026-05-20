//! Port of `src/index.ts` — convenience re-exports.
//!
//! Upstream re-exports every user-facing top-level module so `import * from
//! 'rxdb'` is one statement. In Rust the equivalent idiom is a `prelude`
//! module the user pulls in with `use ctox_rxdb::prelude::*;`. The mapping is
//! file-for-file with the upstream `index.ts` list, with two carveouts:
//!
//! - `rx-document.ts`, `rx-query.ts` (T1, unported) are omitted — they will
//!   land in phase-6 with their own types.
//! - `query-cache.ts`, `doc-cache.ts` (T1, unported) are omitted for the same
//!   reason.
//!
//! When a module is added to the crate, also add its re-export here so
//! callers don't need to remember the full path.

pub use crate::custom_index::*;
pub use crate::doc_cache::*;
pub use crate::event_reduce::*;
pub use crate::hooks::*;
pub use crate::overwritable::*;
pub use crate::plugin::*;
pub use crate::plugin_helpers::*;
pub use crate::plugins::utils;
pub use crate::query_planner::*;
pub use crate::replication_protocol::index_mod::*;
pub use crate::rx_change_event::*;
pub use crate::rx_collection::*;
pub use crate::rx_collection_helper::*;
pub use crate::rx_database::*;
pub use crate::rx_database_internal_store::*;
pub use crate::rx_document::*;
pub use crate::rx_error::*;
pub use crate::rx_query::*;
pub use crate::rx_query_helper::*;
pub use crate::rx_schema::*;
pub use crate::rx_schema_helper::*;
pub use crate::rx_storage_helper::*;
pub use crate::rx_storage_multiinstance::*;
pub use crate::storage;
pub use crate::types;
