//! Type stubs for the skipped `pipeline` plugin (gap-item N14).
//!
//! Source: `src/plugins/pipeline/types.ts`. The runtime
//! `RxPipeline` class is omitted — CTOX MVP does not run pipelines.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ref: rxdb/src/plugins/pipeline/types.ts:8-10
/// Handler closure: receives a batch of doc states, returns a future that
/// resolves when the side effect is done. Upstream returns `MaybePromise<any>`;
/// CTOX models the unit-of-work shape.
pub type RxPipelineHandler =
    Arc<dyn Fn(Vec<Value>) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

// ref: rxdb/src/plugins/pipeline/types.ts:12-20
pub struct RxPipelineOptions {
    pub identifier: String,
    /// Destination collection name. Upstream is `RxCollection<any>`; CTOX
    /// stores the name only — the destination is resolved by the runtime when
    /// pipelines land in a later wave.
    pub destination: String,
    pub handler: RxPipelineHandler,
    #[allow(dead_code)]
    pub wait_for_leadership: Option<bool>,
    #[allow(dead_code)]
    pub batch_size: Option<u64>,
}

// ref: rxdb/src/plugins/pipeline/types.ts:23-26
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CheckpointDocData {
    pub checkpoint: Value,
    #[serde(rename = "lastDocTime")]
    pub last_doc_time: i64,
}

// ref: rxdb/src/plugins/pipeline/rx-pipeline.ts class RxPipeline — fields only.
/// Placeholder so `RxCollection.addPipeline` surfaces can reference the type.
pub struct RxPipeline {
    pub identifier: String,
    pub destination_name: String,
    pub canceled: bool,
}
