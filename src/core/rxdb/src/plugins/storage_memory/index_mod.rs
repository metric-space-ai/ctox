//! Port of `src/plugins/storage-memory/index.ts` — the storage factory entry.
//!
//! Upstream barrel-exports many submodules at the same time as defining
//! `getRxStorageMemory()`. The Rust port keeps the factory function here and
//! relies on `super::*` re-exports in `mod.rs` for the symbol surface.
//!
//! T1 deviation: upstream's module-level `const COLLECTION_STATES = new Map()`
//! that survives instance-close is replaced by the `collection_states` field
//! on [`crate::plugins::storage_memory::rx_storage_instance_memory::RxStorageMemory`],
//! initialized by [`get_rx_storage_memory`].

use std::sync::Arc;

use crate::plugins::storage_memory::memory_types::RxStorageMemorySettings;
use crate::plugins::storage_memory::rx_storage_instance_memory::{
    RxStorageInstanceMemory, RxStorageMemory,
};
use crate::plugins::utils::utils_rxdb_version::RXDB_VERSION;
use crate::rx_error::RxResult;
use crate::rx_storage_helper::ensure_rx_storage_instance_params_are_correct;
use crate::types::RxStorageInstanceCreationParams;

// ref: rxdb/src/plugins/storage-memory/index.ts:21-45
/// Build an in-memory storage with a fresh `collection_states` registry.
pub fn get_rx_storage_memory(_settings: RxStorageMemorySettings) -> Arc<RxStorageMemory> {
    let _ = RXDB_VERSION; // upstream populates `storage.rxdbVersion`; not exposed via the trait yet
    RxStorageMemory::new()
}

/// Convenience method that mirrors upstream's `storage.createStorageInstance`.
pub async fn create_storage_instance(
    storage: &Arc<RxStorageMemory>,
    params: RxStorageInstanceCreationParams,
    settings: RxStorageMemorySettings,
) -> RxResult<Arc<RxStorageInstanceMemory>> {
    ensure_rx_storage_instance_params_are_correct(&params)?;
    storage.create_storage_instance(params, settings).await
}
