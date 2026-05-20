//! Port of `rxdb/src/plugins/backup/` — type stubs only (gap-item N13).
//!
//! CTOX MVP does not run the in-process backup loop; only the option types are
//! ported so the `rx-database` surface compiles.

pub mod types_stub;

pub use types_stub::{
    BackupMetaFileContent, BackupOptions, RxBackupCollectionState, RxBackupState,
    RxBackupWriteEvent,
};
