//! Qwen3.5-0.8B Metal research probe.
//!
//! This crate is deliberately separate from the production 27B and 35B model
//! crates. It captures the shape contract, benchmark gates, and eventual
//! Metal/ANE experiment surfaces for the 0.8B optimization effort.

pub mod artifacts;
pub mod cache_model;
pub mod metalpack;
pub mod model_shape;
pub mod pack_plan;
pub mod research_gates;
pub mod shape_audit;

#[cfg(target_os = "macos")]
pub mod metal;

pub use artifacts::{
    inspect_model_artifacts, inspect_model_artifacts_for_shape, ArtifactError, ArtifactReport,
    ConfigReport, ExtractedQwenConfig, SafetensorShard, SafetensorsReport, TensorHeader,
};
pub use cache_model::{
    format_bytes, qwen35_cache_analysis, CacheCounterPlan, CacheModelConfig, CacheOpAnalysis,
    CacheResidency, CounterPriority,
};
pub use metalpack::{
    open_metalpack, write_metalpack_from_model_dir, write_metalpack_from_report, MetalPack,
    MetalPackEntry, MetalPackReport,
};
pub use model_shape::{
    LayerKind, ModelShape, QWEN35_08B, QWEN35_08B_CANONICAL_MODEL, QWEN35_08B_LAYER_PATTERN,
};
pub use pack_plan::{
    classify_tensor, parse_layer_id, ClassSummary, PackEntry, PackLayout, PackPlan,
    PackPlanWarning, QuantScheme, TensorClass,
};
pub use research_gates::{ExperimentGate, GateStatus, ResearchPlan};
pub use shape_audit::{audit_shape_contract, ShapeAuditRow, ShapeAuditStatus};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exported_shape_is_qwen35_08b() {
        assert_eq!(QWEN35_08B.model, QWEN35_08B_CANONICAL_MODEL);
        assert_eq!(QWEN35_08B.hidden_size, 1024);
        assert_eq!(QWEN35_08B.vocab_size, 248_320);
        assert_eq!(QWEN35_08B.n_layers, 24);
    }

    #[test]
    fn research_plan_starts_with_correctness_and_gpu_locality() {
        let plan = ResearchPlan::default();
        assert_eq!(plan.gates[0].name, "shape-contract");
        assert!(plan
            .gates
            .iter()
            .any(|gate| gate.name == "gpu-local-lm-head-argmax"));
        assert!(plan
            .gates
            .iter()
            .any(|gate| gate.name == "one-cpu-sync-per-token"));
    }
}
