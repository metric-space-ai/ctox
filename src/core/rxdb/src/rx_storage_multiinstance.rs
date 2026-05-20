//! Port of `src/rx-storage-multiinstance.ts`.
//!
//! **T2 single-process stub.** Upstream broadcasts change events between
//! multiple JS contexts (browser tabs, WebWorkers) of the same `databaseName`
//! using the `broadcast-channel` NPM package. CTOX runs in one process, so
//! there is nothing to multi-cast.
//!
//! The functions are kept for API parity but are no-ops. If multi-process
//! coordination ever becomes a CTOX requirement, this is where it goes.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::types::{EventBulk, RxStorageInstanceCreationParams};

// ref: rxdb/src/rx-storage-multiinstance.ts:48-57
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RxStorageMultiInstanceBroadcastType {
    pub storage_name: String,
    pub collection_name: String,
    /// `collection.schema.version`
    pub version: i32,
    pub database_name: String,
    pub event_bulk: EventBulk,
}

/// Refcounted broadcast-channel slot. The Rust port keeps the bookkeeping
/// shape so any future multi-process implementation can plug in here.
pub struct BroadcastChannelState {
    pub refs: HashSet<String>,
}

// ref: rxdb/src/rx-storage-multiinstance.ts:38-45
pub static BROADCAST_CHANNEL_BY_TOKEN: LazyLock<
    Mutex<HashMap<String, Arc<Mutex<BroadcastChannelState>>>>,
> = LazyLock::new(|| Mutex::new(HashMap::new()));

// ref: rxdb/src/rx-storage-multiinstance.ts:59-80
/// Get or create the broadcast-channel state for a database token.
/// Single-process stub: returns the refcount handle but no actual channel.
pub fn get_broadcast_channel_reference(
    _storage_name: &str,
    database_instance_token: &str,
    _database_name: &str,
    ref_object: String,
) -> Arc<Mutex<BroadcastChannelState>> {
    let mut map = BROADCAST_CHANNEL_BY_TOKEN.lock();
    let state = map
        .entry(database_instance_token.to_string())
        .or_insert_with(|| {
            Arc::new(Mutex::new(BroadcastChannelState {
                refs: HashSet::new(),
            }))
        });
    {
        let mut guard = state.lock();
        guard.refs.insert(ref_object);
    }
    Arc::clone(state)
}

// ref: rxdb/src/rx-storage-multiinstance.ts:82-95
pub fn remove_broadcast_channel_reference(database_instance_token: &str, ref_object: &str) {
    let mut map = BROADCAST_CHANNEL_BY_TOKEN.lock();
    let Some(state) = map.get(database_instance_token).cloned() else {
        return;
    };
    let mut guard = state.lock();
    guard.refs.remove(ref_object);
    if guard.refs.is_empty() {
        drop(guard);
        map.remove(database_instance_token);
    }
}

// ref: rxdb/src/rx-storage-multiinstance.ts:98-188
/// Attach multi-instance support to a storage. Upstream wraps `changeStream`,
/// `close`, `remove` to fan events through a `broadcast-channel`.
///
/// **CTOX single-process stub**: if `instance_creation_params.multi_instance`
/// is false (always, in CTOX), this returns immediately. If multi-instance is
/// ever enabled in the future, the implementation must produce a wrapped
/// `RxStorageInstance` whose `change_stream` merges remote events. For now we
/// only honour the contract that the function exists and does nothing
/// dangerous when called.
pub fn add_rx_storage_multi_instance_support(
    _storage_name: &str,
    instance_creation_params: &RxStorageInstanceCreationParams,
) {
    if !instance_creation_params.multi_instance {
        // Single-process: nothing to do. This is the only path CTOX exercises.
        return;
    }
    // multi-instance is not supported in this build.
    // A future port would build a broadcast-channel and wrap the instance's
    // changeStream/close/remove. The error path is intentionally a silent
    // no-op rather than a panic so that downstream code that opportunistically
    // calls this remains compatible.
}
