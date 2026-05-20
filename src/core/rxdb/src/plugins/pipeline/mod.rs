//! Port of `rxdb/src/plugins/pipeline/` — type stubs only (gap-item N14).
//!
//! Source: `src/plugins/pipeline/types.ts`. CTOX MVP does not run pipelines;
//! only the option types are ported so `rx-collection.addPipeline` surfaces
//! compile.

pub mod types_stub;

pub use types_stub::{CheckpointDocData, RxPipeline, RxPipelineHandler, RxPipelineOptions};
