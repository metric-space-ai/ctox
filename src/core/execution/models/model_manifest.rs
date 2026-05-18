use anyhow::Context;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

use crate::execution::models::model_registry;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeObjectiveLabel {
    Quality,
    Performance,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManifestBackendMode {
    DeviceLayers,
    Nccl,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManifestQuantization {
    Q4k,
    Q5k,
    Q6k,
    NativeMxfp4,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ManifestSizingProfile {
    pub non_repeating_weight_mb_q4: u64,
    pub repeating_layer_weight_mb_q4: u64,
    pub load_peak_slack_mb_q4: u64,
    pub kv_mb_per_1k_tokens_q4: u64,
    pub base_toks_per_sec_q4: f64,
    pub repeating_layers: u32,
    pub context_cap: u32,
    #[serde(default)]
    pub measurement_components: ManifestMeasurementComponents,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ManifestMeasurementComponents {
    #[serde(default)]
    pub load_overheads_mb_q4: ManifestLoadOverheads,
    #[serde(default)]
    pub backend_runtime_overheads_mb_q4: ManifestBackendRuntimeOverheads,
    #[serde(default)]
    pub activation_overheads_mb_q4: ManifestActivationOverheads,
    #[serde(default)]
    pub kv_cache_mb_per_1k_tokens_by_cache_type_q4: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ManifestLoadOverheads {
    pub plain_load_mb: u64,
    pub immediate_isq_mb: u64,
    pub nccl_init_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ManifestBackendRuntimeOverheads {
    pub device_layers_mb: u64,
    pub nccl_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ManifestActivationOverheads {
    pub gpu0_anchor_runtime_mb: u64,
    pub prefill_anchor_mb_at_128k: u64,
    pub decode_per_seq_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestRuntimeDefaults {
    pub paged_attn: String,
    pub pa_cache_type: Option<String>,
    pub pa_memory_fraction: Option<String>,
    #[serde(default)]
    pub pa_context_len: Option<u32>,
    pub force_no_mmap: bool,
    pub force_language_model_only: bool,
    #[serde(default)]
    pub require_prebuilt_uqff_for_chat_start: bool,
    pub disable_flash_attn: bool,
    pub isq_singlethread: bool,
    pub isq_cpu_threads: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ManifestPlannerHints {
    #[serde(default)]
    pub moe_experts_backend: Option<String>,
    #[serde(default)]
    pub small_uniform_device_layers_scale: Option<ManifestDeviceLayersScaleHint>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ManifestDeviceLayersScaleHint {
    pub max_gpu_memory_mb: u64,
    pub single_gpu_scale: f64,
    pub dual_gpu_scale: f64,
    pub multi_gpu_scale: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestPlacementProfile {
    pub primary_gpu_index: usize,
    pub primary_gpu_holds_non_repeating: bool,
    pub primary_gpu_desktop_reserve_mb: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ManifestNcclQualification {
    #[default]
    Unsupported,
    Experimental,
    Qualified,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestLaunchContract {
    pub required_context_tokens: u32,
    pub require_primary_gpu_anchor: bool,
    pub nccl_qualification: ManifestNcclQualification,
    pub nccl_preserves_primary_gpu_anchor: bool,
    #[serde(default)]
    pub allow_subset_anchored_nccl: bool,
}

impl Default for ManifestLaunchContract {
    fn default() -> Self {
        Self {
            required_context_tokens: 32_768,
            require_primary_gpu_anchor: true,
            nccl_qualification: ManifestNcclQualification::Unsupported,
            nccl_preserves_primary_gpu_anchor: false,
            allow_subset_anchored_nccl: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PresetCandidateSpec {
    pub quantization: ManifestQuantization,
    pub backend: ManifestBackendMode,
    pub max_batch_size: u32,
    pub max_seqs: u32,
    pub context_fraction_milli: u32,
    pub context_target_cap: Option<u32>,
    pub min_context_required: u32,
    pub per_gpu_headroom_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestPresetProfile {
    pub objective: RuntimeObjectiveLabel,
    pub candidates: Vec<PresetCandidateSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeModelManifest {
    pub model: String,
    pub sizing: ManifestSizingProfile,
    pub runtime_defaults: ManifestRuntimeDefaults,
    #[serde(default)]
    pub planner_hints: ManifestPlannerHints,
    pub placement: ManifestPlacementProfile,
    #[serde(default)]
    pub launch_contract: ManifestLaunchContract,
    pub quality: ManifestPresetProfile,
    pub performance: ManifestPresetProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeModelQualificationProfile {
    pub model: String,
    pub family: String,
    pub measured_at: String,
    pub host_profiles: Vec<QualifiedHostProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QualifiedHostProfile {
    pub host_class: String,
    pub min_gpu_count: usize,
    #[serde(default)]
    pub max_gpu_count: Option<usize>,
    pub min_gpu_memory_mb: u64,
    #[serde(default)]
    pub min_total_gpu_memory_mb: Option<u64>,
    #[serde(default)]
    pub max_total_gpu_memory_mb: Option<u64>,
    pub validated_context_cap: u32,
    pub quality_score: u8,
    pub performance_score: u8,
    pub steady_state_toks_per_sec: f64,
    pub first_token_latency_ms: u32,
    pub cold_start_secs: u32,
    pub stability_score: u8,
    pub nccl_uplift_percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuxiliaryPlacementProfile {
    pub primary_gpu_index: usize,
    pub use_primary_gpu_by_default: bool,
    pub supports_multi_gpu_expansion: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuxiliaryModelManifest {
    pub model: String,
    pub role: String,
    pub gpu_reserve_mb: u64,
    pub placement: AuxiliaryPlacementProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlatformGpuCapability {
    pub index: usize,
    pub name: String,
    pub total_mb: u64,
    #[serde(default)]
    pub compute_capability: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlatformCapabilities {
    pub generated_at: String,
    pub source: String,
    #[serde(default)]
    pub cuda_available: bool,
    #[serde(default)]
    pub metal_available: bool,
    #[serde(default)]
    pub nccl_available: bool,
    #[serde(default)]
    pub flash_attn_available: bool,
    pub gpus: Vec<PlatformGpuCapability>,
}

pub fn load_runtime_model_manifest(
    root: &Path,
    model: &str,
) -> anyhow::Result<Option<RuntimeModelManifest>> {
    let Some(path) = manifest_path_for_model(root, model) else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)
        .with_context(|| format!("failed to read runtime model manifest {}", path.display()))?;
    let manifest: RuntimeModelManifest = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse runtime model manifest {}", path.display()))?;
    Ok(Some(manifest))
}

pub fn load_runtime_model_qualification_profile(
    root: &Path,
    model: &str,
) -> anyhow::Result<Option<RuntimeModelQualificationProfile>> {
    let Some(path) = qualification_profile_path_for_model(root, model) else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).with_context(|| {
        format!(
            "failed to read runtime qualification profile {}",
            path.display()
        )
    })?;
    let profile: RuntimeModelQualificationProfile =
        serde_json::from_slice(&bytes).with_context(|| {
            format!(
                "failed to parse runtime qualification profile {}",
                path.display()
            )
        })?;
    Ok(Some(profile))
}

pub fn load_auxiliary_model_manifest(
    root: &Path,
    model: &str,
) -> anyhow::Result<Option<AuxiliaryModelManifest>> {
    let Some(path) = auxiliary_manifest_path_for_model(root, model) else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)
        .with_context(|| format!("failed to read auxiliary model manifest {}", path.display()))?;
    let manifest: AuxiliaryModelManifest = serde_json::from_slice(&bytes).with_context(|| {
        format!(
            "failed to parse auxiliary model manifest {}",
            path.display()
        )
    })?;
    Ok(Some(manifest))
}

pub fn load_platform_capabilities(root: &Path) -> anyhow::Result<Option<PlatformCapabilities>> {
    let path = root.join("runtime").join("platform_capabilities.json");
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)
        .with_context(|| format!("failed to read platform capabilities {}", path.display()))?;
    let manifest: PlatformCapabilities = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse platform capabilities {}", path.display()))?;
    Ok(Some(manifest))
}

fn manifest_path_for_model(root: &Path, model: &str) -> Option<std::path::PathBuf> {
    let slug = model_registry::runtime_manifest_slug(model)?;
    Some(
        root.join("contracts")
            .join("models")
            .join("runtime_manifests")
            .join(format!("{slug}.json")),
    )
}

fn qualification_profile_path_for_model(root: &Path, model: &str) -> Option<std::path::PathBuf> {
    let slug = model_registry::runtime_manifest_slug(model)?;
    Some(
        root.join("contracts")
            .join("models")
            .join("qualification_profiles")
            .join(format!("{slug}.json")),
    )
}

fn auxiliary_manifest_path_for_model(root: &Path, model: &str) -> Option<std::path::PathBuf> {
    let slug = model_registry::auxiliary_manifest_slug(model)?;
    Some(
        root.join("contracts")
            .join("models")
            .join("aux_runtime_manifests")
            .join(format!("{slug}.json")),
    )
}

#[cfg(test)]
mod tests {
    use super::load_runtime_model_manifest;
    use std::path::Path;

    #[test]
    fn loads_qwen35_2b_runtime_manifest_from_repo_contracts() {
        let manifest =
            load_runtime_model_manifest(Path::new(env!("CARGO_MANIFEST_DIR")), "Qwen/Qwen3.5-2B")
                .expect("qwen3.5-2b manifest should parse")
                .expect("qwen3.5-2b manifest should exist");
        assert_eq!(manifest.model, "Qwen/Qwen3.5-2B");
    }

    #[test]
    fn loads_gemma4_e2b_runtime_manifest_from_repo_contracts() {
        let manifest = load_runtime_model_manifest(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            "google/gemma-4-E2B-it",
        )
        .expect("gemma4 e2b manifest should parse")
        .expect("gemma4 e2b manifest should exist");
        assert_eq!(manifest.model, "google/gemma-4-E2B-it");
    }

    #[test]
    fn loads_gemma4_e4b_runtime_manifest_from_repo_contracts() {
        let manifest = load_runtime_model_manifest(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            "google/gemma-4-E4B-it",
        )
        .expect("gemma4 e4b manifest should parse")
        .expect("gemma4 e4b manifest should exist");
        assert_eq!(manifest.model, "google/gemma-4-E4B-it");
    }
}
