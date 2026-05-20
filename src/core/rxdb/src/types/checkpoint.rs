//! Checkpoint types.

use serde::{Deserialize, Serialize};

// ref: rxdb/src/types/rx-storage.d.ts RxStorageDefaultCheckpoint
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct RxStorageDefaultCheckpoint {
    pub id: String,
    pub lwt: f64,
}
