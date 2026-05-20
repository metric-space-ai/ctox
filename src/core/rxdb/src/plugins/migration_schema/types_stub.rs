//! Type stubs for the skipped `migration-schema` plugin (gap-item N12).
//!
//! Source: `src/plugins/migration-schema/migration-types.ts` + the
//! re-export shape used by `rx-collection.d.ts` and
//! `rx-database-internal-store.d.ts`. CTOX does not run schema migrations;
//! the types exist so the `rx-database` / `rx-collection` surfaces compile.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ref: rxdb/src/types/index.d.ts PlainJsonError — upstream is a runtime-only
// `errorToPlainJson` projection. CTOX models it as an opaque JSON value to
// avoid leaking V8 stack-trace shape into the type system.
pub type PlainJsonError = Value;

// ref: rxdb/src/plugins/migration-schema/migration-types.ts:16-31
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RxMigrationCount {
    pub total: u64,
    pub handled: u64,
    /// 0..100
    pub percent: f64,
}

// ref: rxdb/src/plugins/migration-schema/migration-types.ts:6-32
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RxMigrationStatus {
    #[serde(rename = "collectionName")]
    pub collection_name: String,
    /// `"RUNNING"` | `"DONE"` | `"ERROR"`.
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<PlainJsonError>,
    pub count: RxMigrationCount,
}

/// Convenience alias (some upstream call sites use the shorter name).
pub type MigrationStatus = RxMigrationStatus;

// ref: rxdb/src/plugins/migration-schema/migration-types.ts:39
/// Internal-store wrapper around `RxMigrationStatus`. Upstream uses
/// `InternalStoreDocType<RxMigrationStatus>`; CTOX represents internal-store
/// documents as raw `Value` until phase-6 lands the typed wrapper.
pub type RxMigrationStatusDocument = Value;
pub type MigrationStatusDocument = RxMigrationStatusDocument;

// ref: rxdb/src/plugins/migration-schema/rx-migration-state.ts
/// Placeholder for the plugin-owned migration runtime. The core collection
/// surface exposes plugin-missing methods until `migration-schema` is enabled.
#[derive(Debug, Clone, Default)]
pub struct RxMigrationState;

// ref: rxdb/src/plugins/migration-schema/migration-types.ts:42
/// Closure that transforms a status document. Upstream is
/// `(before: RxMigrationStatus) => RxMigrationStatus`; we model it as an
/// `Arc<dyn Fn>` for trait-object use.
pub type MigrationStatusUpdate = Arc<dyn Fn(&RxMigrationStatus) -> RxMigrationStatus + Send + Sync>;

// ref: rxdb/src/types/plugins/migration.d.ts:7-10
/// Per-version migration closure. Returns `None` to drop the document.
pub type MigrationStrategy =
    Arc<dyn Fn(Value) -> Pin<Box<dyn Future<Output = Option<Value>> + Send>> + Send + Sync>;

// ref: rxdb/src/types/plugins/migration.d.ts:12-14
/// Map from target-version → migration closure.
pub type MigrationStrategies = HashMap<u32, MigrationStrategy>;
