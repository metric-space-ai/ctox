//! Port of `rxdb/src/plugins/migration-schema/` — type stubs only.
//!
//! The full schema-migration runtime is out of scope for CTOX MVP. Only the
//! types referenced by `rx-database-internal-store` and `rx-collection`
//! surfaces are ported so other modules compile.

pub mod types_stub;

pub use types_stub::{
    MigrationStatus, MigrationStatusDocument, MigrationStatusUpdate, MigrationStrategies,
    MigrationStrategy, RxMigrationCount, RxMigrationState, RxMigrationStatus,
    RxMigrationStatusDocument,
};
