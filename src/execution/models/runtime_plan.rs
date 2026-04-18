use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use crate::inference::engine;
use crate::inference::model_manifest;
use crate::inference::model_registry;
use crate::inference::resource_state;
use crate::inference::runtime_contract;
use crate::inference::runtime_env;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

const MIN_POLICY_CONTEXT: u32 = 131_072;
const QUALITY_MIN_COMPACTION_TOKENS: u32 = 12_288;
const PERFORMANCE_MIN_COMPACTION_TOKENS: u32 = 8_192;
const DEFAULT_GPU0_DESKTOP_RESERVE_MB: u64 = 1024;
const CHAT_PLAN_RELATIVE_PATH: &str = "runtime/chat_plan.json";
const RUNTIME_FLEET_PLAN_RELATIVE_PATH: &str = "runtime/runtime_fleet_plan.json";
const NCCL_CAPABILITY_OVERRIDE_RELATIVE_PATH: &str = "runtime/nccl_capability_override.json";
const QUANT_ARTIFACTS_RELATIVE_DIR: &str = "runtime/uqff_cache";
const QUANT_ARTIFACTS_ROOT_ENV: &str = "CTOX_UQFF_CACHE_ROOT";
const NVIDIA_SMI_TIMEOUT_SECS: u64 = 10;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatPreset {
    Quality,
    Performance,
}

impl ChatPreset {
    pub fn label(self) -> &'static str {
        match self {
            Self::Quality => "Quality",
            Self::Performance => "Performance",
        }
    }

    pub fn from_label(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "quality" => Self::Quality,
            "performance" | "perf" => Self::Performance,
            _ => Self::Quality,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardwareGpu {
    pub index: usize,
    pub name: String,
    pub total_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardwareProfile {
    pub gpus: Vec<HardwareGpu>,
    pub gpu0_desktop_reserve_mb: u64,
    pub fingerprint: String,
}

#[derive(Debug, Clone)]
struct PlatformFacts {
    hardware: HardwareProfile,
    cuda_available: bool,
    nccl_available: bool,
    flash_attn_available: bool,
    validation_messages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlannedGpuAllocation {
    pub gpu_index: usize,
    pub name: String,
    pub total_mb: u64,
    pub desktop_reserve_mb: u64,
    pub aux_reserve_mb: u64,
    pub chat_budget_mb: u64,
    pub backend_overhead_mb: u64,
    pub activation_overhead_mb: u64,
    pub load_peak_overhead_mb: u64,
    pub repeating_weight_mb: u64,
    pub weight_mb: u64,
    pub kv_cache_mb: u64,
    pub free_headroom_mb: u64,
    pub chat_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TheoreticalResourceBreakdown {
    pub contract_source: String,
    pub effective_total_budget_mb: u64,
    pub kv_budget_cap_mb: u64,
    pub kv_budget_fraction_milli: u32,
    pub weight_residency_mb: u64,
    pub kv_cache_mb: u64,
    pub fixed_runtime_base_overhead_mb: u64,
    pub backend_runtime_overhead_mb: u64,
    pub activation_overhead_mb: u64,
    pub load_peak_overhead_mb: u64,
    pub safety_headroom_mb: u64,
    pub required_effective_total_budget_mb: u64,
    pub required_total_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NcclCapabilityOverride {
    pub detected_at: String,
    pub model: String,
    pub reason: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatRuntimePlan {
    pub model: String,
    pub preset: ChatPreset,
    pub quantization: String,
    pub runtime_isq: Option<String>,
    pub max_seq_len: u32,
    pub compaction_threshold_percent: u8,
    pub compaction_min_tokens: u32,
    pub min_context_floor_applied: bool,
    pub paged_attn: String,
    pub pa_cache_type: Option<String>,
    pub pa_memory_fraction: Option<String>,
    pub pa_context_len: Option<u32>,
    pub disable_nccl: bool,
    pub tensor_parallel_backend: Option<String>,
    pub mn_local_world_size: Option<u32>,
    pub max_batch_size: u32,
    pub max_seqs: u32,
    pub cuda_visible_devices: String,
    pub device_layers: Option<String>,
    pub topology: Option<String>,
    pub allow_device_layers_with_topology: bool,
    pub nm_device_ordinal: Option<u32>,
    pub base_device_ordinal: Option<u32>,
    pub moe_experts_backend: Option<String>,
    pub disable_flash_attn: bool,
    pub force_no_mmap: bool,
    pub force_language_model_only: bool,
    #[serde(default)]
    pub require_prebuilt_uqff_for_chat_start: bool,
    pub isq_singlethread: bool,
    pub isq_cpu_threads: Option<u32>,
    pub expected_tok_s: f64,
    pub hardware_fingerprint: String,
    pub theoretical_breakdown: TheoreticalResourceBreakdown,
    pub rationale: Vec<String>,
    pub gpu_allocations: Vec<PlannedGpuAllocation>,
}

impl ChatRuntimePlan {
    pub fn effective_cache_label(&self) -> &str {
        self.pa_cache_type.as_deref().unwrap_or_else(|| {
            if self.paged_attn.eq_ignore_ascii_case("off") {
                "off"
            } else {
                "auto"
            }
        })
    }
}

pub(crate) fn allocation_required_free_mb(allocation: &PlannedGpuAllocation) -> u64 {
    allocation
        .desktop_reserve_mb
        .saturating_add(allocation.aux_reserve_mb)
        .saturating_add(allocation.chat_budget_mb)
        .saturating_add(allocation.backend_overhead_mb)
        .saturating_add(allocation.activation_overhead_mb)
        .saturating_add(allocation.load_peak_overhead_mb)
}

fn explicit_resource_contract(manifest: &model_manifest::RuntimeModelManifest) -> bool {
    let components = &manifest.sizing.measurement_components;
    components.load_overheads_mb_q4 != model_manifest::ManifestLoadOverheads::default()
        || components.backend_runtime_overheads_mb_q4
            != model_manifest::ManifestBackendRuntimeOverheads::default()
        || components.activation_overheads_mb_q4
            != model_manifest::ManifestActivationOverheads::default()
        || !components
            .kv_cache_mb_per_1k_tokens_by_cache_type_q4
            .is_empty()
}

fn resource_contract_source(manifest: &model_manifest::RuntimeModelManifest) -> String {
    if explicit_resource_contract(manifest) {
        "explicit manifest resource contract".to_string()
    } else {
        "legacy manifest fallback sizing".to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatPresetBundle {
    pub model: String,
    pub hardware: HardwareProfile,
    pub selected_preset: ChatPreset,
    pub selected_plan: ChatRuntimePlan,
    pub plans: Vec<ChatRuntimePlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuxiliaryRuntimePlan {
    pub role: engine::AuxiliaryRole,
    pub display_model: String,
    pub request_model: String,
    pub backend_kind: engine::AuxiliaryBackendKind,
    pub compute_target: engine::ComputeTarget,
    pub port: u16,
    pub visible_devices: Option<String>,
    pub gpu_reserve_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeFleetPlan {
    pub version: u32,
    pub hardware_fingerprint: String,
    pub chat: Option<ChatRuntimePlan>,
    pub embedding: Option<AuxiliaryRuntimePlan>,
    pub transcription: Option<AuxiliaryRuntimePlan>,
    pub speech: Option<AuxiliaryRuntimePlan>,
    pub vision: Option<AuxiliaryRuntimePlan>,
}

impl RuntimeFleetPlan {
    pub fn auxiliary_plan(&self, role: engine::AuxiliaryRole) -> Option<&AuxiliaryRuntimePlan> {
        match role {
            engine::AuxiliaryRole::Embedding => self.embedding.as_ref(),
            engine::AuxiliaryRole::Stt => self.transcription.as_ref(),
            engine::AuxiliaryRole::Tts => self.speech.as_ref(),
            engine::AuxiliaryRole::Vision => self.vision.as_ref(),
        }
    }
}

fn plan_uses_policy_floor(plan: &ChatRuntimePlan) -> bool {
    plan.rationale
        .iter()
        .any(|line| line.contains("policy floor fallback"))
}

fn generated_at_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

fn quant_artifacts_root(root: &Path) -> PathBuf {
    if let Some(override_root) = std::env::var_os(QUANT_ARTIFACTS_ROOT_ENV) {
        let candidate = PathBuf::from(override_root);
        if !candidate.as_os_str().is_empty() {
            return candidate;
        }
    }
    root.join(QUANT_ARTIFACTS_RELATIVE_DIR)
}

fn engine_artifact_build_stamp(root: &Path) -> String {
    let binary = engine::discover_source_layout_paths(root).model_runtime_binary;
    let Ok(metadata) = std::fs::metadata(&binary) else {
        return "missing-engine".to_string();
    };
    let Ok(mut file) = File::open(&binary) else {
        return "missing-engine".to_string();
    };
    let modified_secs = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_secs())
        .unwrap_or(0);
    let mut hasher = Sha256::new();
    hasher.update(metadata.len().to_le_bytes());
    hasher.update(modified_secs.to_le_bytes());
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(read_len) => hasher.update(&buffer[..read_len]),
            Err(_) => {
                return format!(
                    "{:x}",
                    Sha256::digest(format!("{}:{}", metadata.len(), modified_secs))
                );
            }
        }
    }
    format!("{:x}", hasher.finalize())
}

fn quant_artifact_cache_key(plan: &ChatRuntimePlan, engine_stamp: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!(
            "{}|{}|{}|{}|{}|{}",
            plan.model,
            plan.preset.label(),
            plan.quantization,
            plan.max_seq_len,
            plan.hardware_fingerprint,
            engine_stamp
        ))
    )
}

fn chat_quant_artifact_cache_dir(root: &Path, plan: &ChatRuntimePlan) -> Option<PathBuf> {
    let runtime_isq = plan
        .runtime_isq
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let engine_stamp = engine_artifact_build_stamp(root);
    let cache_key = quant_artifact_cache_key(plan, &engine_stamp);
    let _ = runtime_isq;
    Some(
        quant_artifacts_root(root)
            .join(&plan.hardware_fingerprint)
            .join(engine_stamp)
            .join(cache_key),
    )
}

pub fn chat_quant_artifact_write_path(root: &Path, plan: &ChatRuntimePlan) -> Option<PathBuf> {
    let runtime_isq = plan
        .runtime_isq
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(
        chat_quant_artifact_cache_dir(root, plan)?
            .join(format!("{}.uqff", runtime_isq.to_ascii_lowercase())),
    )
}

pub fn chat_quant_artifact_path(root: &Path, plan: &ChatRuntimePlan) -> Option<PathBuf> {
    let runtime_isq = plan
        .runtime_isq
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(
        chat_quant_artifact_cache_dir(root, plan)?
            .join(format!("{}-0.uqff", runtime_isq.to_ascii_lowercase())),
    )
}

pub fn available_chat_quant_artifact(root: &Path, plan: &ChatRuntimePlan) -> Option<PathBuf> {
    let path = chat_quant_artifact_path(root, plan)?;
    let Ok(metadata) = std::fs::metadata(&path) else {
        return None;
    };
    (metadata.is_file() && metadata.len() > 0).then_some(path)
}

fn nccl_capability_override_path(root: &Path) -> PathBuf {
    root.join(NCCL_CAPABILITY_OVERRIDE_RELATIVE_PATH)
}

fn load_nccl_capability_override(root: &Path) -> Result<Option<NcclCapabilityOverride>> {
    let path = nccl_capability_override_path(root);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)
        .with_context(|| format!("failed to read NCCL capability override {}", path.display()))?;
    let override_state = serde_json::from_slice(&bytes).with_context(|| {
        format!(
            "failed to parse NCCL capability override {}",
            path.display()
        )
    })?;
    Ok(Some(override_state))
}

pub(crate) fn persist_nccl_capability_override(
    root: &Path,
    model: &str,
    reason: &str,
    signature: &str,
) -> Result<()> {
    let path = nccl_capability_override_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create NCCL capability override dir {}",
                parent.display()
            )
        })?;
    }
    let override_state = NcclCapabilityOverride {
        detected_at: generated_at_timestamp(),
        model: model.to_string(),
        reason: reason.to_string(),
        signature: signature.to_string(),
    };
    let bytes = serde_json::to_vec_pretty(&override_state)
        .context("failed to encode NCCL capability override")?;
    std::fs::write(&path, bytes).with_context(|| {
        format!(
            "failed to write NCCL capability override {}",
            path.display()
        )
    })
}

fn chat_capacity_contract_from_plan(
    plan: &ChatRuntimePlan,
) -> runtime_contract::ChatCapacityContract {
    let gpus = plan
        .gpu_allocations
        .iter()
        .filter(|allocation| allocation.chat_enabled)
        .map(|allocation| runtime_contract::ChatGpuCapacityRequirement {
            gpu_index: allocation.gpu_index,
            name: allocation.name.clone(),
            total_mb: allocation.total_mb,
            desktop_reserve_mb: allocation.desktop_reserve_mb,
            aux_reserve_mb: allocation.aux_reserve_mb,
            chat_budget_mb: allocation.chat_budget_mb,
            backend_overhead_mb: allocation.backend_overhead_mb,
            activation_overhead_mb: allocation.activation_overhead_mb,
            load_peak_overhead_mb: allocation.load_peak_overhead_mb,
            required_free_mb: allocation_required_free_mb(allocation),
            free_headroom_mb: allocation.free_headroom_mb,
        })
        .collect::<Vec<_>>();
    runtime_contract::ChatCapacityContract {
        model: plan.model.clone(),
        preset: plan.preset.label().to_string(),
        min_context_tokens: MIN_POLICY_CONTEXT,
        max_seq_len: plan.max_seq_len,
        hardware_fingerprint: plan.hardware_fingerprint.clone(),
        generated_at: generated_at_timestamp(),
        rationale: plan.rationale.clone(),
        gpus,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct QualificationRank {
    primary: i64,
    secondary: i64,
    tertiary: i64,
    quaternary: i64,
    quinary: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactionPolicy {
    pub threshold_percent: u8,
    pub min_tokens: u32,
}

pub fn compaction_policy_for_preset(preset: ChatPreset) -> CompactionPolicy {
    match preset {
        ChatPreset::Quality => CompactionPolicy {
            threshold_percent: 75,
            min_tokens: QUALITY_MIN_COMPACTION_TOKENS,
        },
        ChatPreset::Performance => CompactionPolicy {
            threshold_percent: 70,
            min_tokens: PERFORMANCE_MIN_COMPACTION_TOKENS,
        },
    }
}

#[derive(Debug, Clone, Copy)]
struct QuantOption {
    label: &'static str,
    runtime_isq: Option<&'static str>,
    weight_factor_milli: u32,
    speed_factor_milli: u32,
}

#[derive(Debug, Clone, Copy)]
struct EmpiricalSizingProfile {
    // Tensors outside the repeating transformer stack: embeddings, lm_head, norms,
    // router/shared blocks and other per-model fixed tensors. This is measured per model.
    non_repeating_weight_mb_q4: u64,
    // Average footprint per repeating layer at Q4K after the model-specific load path settles.
    repeating_layer_weight_mb_q4: u64,
    // Extra headroom reserved for model-specific load/ISQ spikes observed empirically.
    load_peak_slack_mb_q4: u64,
    #[cfg_attr(not(test), allow(dead_code))]
    kv_mb_per_1k_tokens_q4: u64,
    base_toks_per_sec_q4: f64,
    repeating_layers: u32,
    context_cap: u32,
}

#[derive(Debug, Clone, Copy)]
struct ModelRuntimeHarness {
    paged_attn: &'static str,
    pa_cache_type: Option<&'static str>,
    pa_memory_fraction: Option<&'static str>,
    pa_context_len: Option<u32>,
    force_no_mmap: bool,
    force_language_model_only: bool,
    require_prebuilt_uqff_for_chat_start: bool,
    disable_flash_attn: bool,
    isq_singlethread: bool,
    isq_cpu_threads: Option<u32>,
    moe_experts_backend: Option<&'static str>,
    small_uniform_device_layers_scale: Option<DeviceLayersScaleHint>,
}

#[derive(Debug, Clone, Copy)]
struct ModelHarness {
    model: &'static str,
    sizing: EmpiricalSizingProfile,
    runtime: ModelRuntimeHarness,
}

#[derive(Debug, Clone, Copy)]
struct ResolvedHarnessRuntime {
    fixed_device_layers: Option<&'static str>,
    fixed_cuda_visible_devices: Option<&'static str>,
    topology_rel_path: Option<&'static str>,
    allow_device_layers_with_topology: bool,
    nm_device_ordinal: Option<u32>,
    base_device_ordinal: Option<u32>,
    moe_experts_backend: Option<&'static str>,
    force_no_mmap: bool,
    isq_singlethread: bool,
    isq_cpu_threads: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
struct DeviceLayersScaleHint {
    max_gpu_memory_mb: u64,
    single_gpu_scale: f64,
    dual_gpu_scale: f64,
    multi_gpu_scale: f64,
}

const MXFP4_NATIVE: QuantOption = QuantOption {
    label: "native_mxfp4",
    runtime_isq: None,
    weight_factor_milli: 1000,
    speed_factor_milli: 1000,
};
const Q4K: QuantOption = QuantOption {
    label: "Q4K",
    runtime_isq: Some("Q4K"),
    weight_factor_milli: 1000,
    speed_factor_milli: 1000,
};
const Q5K: QuantOption = QuantOption {
    label: "Q5K",
    runtime_isq: Some("Q5K"),
    weight_factor_milli: 1120,
    speed_factor_milli: 940,
};
const Q6K: QuantOption = QuantOption {
    label: "Q6K",
    runtime_isq: Some("Q6K"),
    weight_factor_milli: 1240,
    speed_factor_milli: 880,
};
#[derive(Debug, Clone, Copy)]
struct PlanSpec {
    quant: QuantOption,
    backend: BackendMode,
    context_target: Option<u32>,
    context_fraction_milli: u32,
    min_context_required: u32,
    per_gpu_headroom_mb: u64,
    max_batch_size: u32,
    max_seqs: u32,
}

pub fn chat_preset_choices() -> Vec<&'static str> {
    vec![ChatPreset::Quality.label(), ChatPreset::Performance.label()]
}

pub fn minimum_supported_chat_context() -> u32 {
    MIN_POLICY_CONTEXT
}

fn required_context_floor_for_manifest(manifest: &model_manifest::RuntimeModelManifest) -> u32 {
    let candidate_floor = manifest
        .quality
        .candidates
        .iter()
        .chain(manifest.performance.candidates.iter())
        .map(|candidate| candidate.min_context_required)
        .filter(|value| *value > 0)
        .min();
    let launch_floor = manifest.launch_contract.required_context_tokens;
    let mut configured_floor = manifest.sizing.context_cap.max(1);
    if launch_floor > 0 {
        configured_floor = configured_floor.min(launch_floor);
    }
    if let Some(candidate_floor) = candidate_floor {
        configured_floor = configured_floor.min(candidate_floor);
    }
    align_context(configured_floor.min(MIN_POLICY_CONTEXT))
}

fn plan_satisfies_required_context_policy(plan: &ChatRuntimePlan, required_context: u32) -> bool {
    plan.max_seq_len >= required_context
        && (required_context < MIN_POLICY_CONTEXT || plan.min_context_floor_applied)
}

pub fn plan_satisfies_context_policy(plan: &ChatRuntimePlan) -> bool {
    plan_satisfies_required_context_policy(plan, MIN_POLICY_CONTEXT)
}

#[allow(dead_code)]
fn manifest_candidate_satisfies_context_policy(
    candidate: &model_manifest::PresetCandidateSpec,
) -> bool {
    candidate.min_context_required >= MIN_POLICY_CONTEXT
        && candidate.context_target_cap.unwrap_or(u32::MAX) >= MIN_POLICY_CONTEXT
}

#[allow(dead_code)]
fn manifest_profile_satisfies_context_policy(
    profile: &model_manifest::ManifestPresetProfile,
) -> bool {
    profile
        .candidates
        .iter()
        .any(manifest_candidate_satisfies_context_policy)
}

fn manifest_satisfies_context_policy(manifest: &model_manifest::RuntimeModelManifest) -> bool {
    let required_context = required_context_floor_for_manifest(manifest);
    manifest_profile_satisfies_context_policy_with_requirement(&manifest.quality, required_context)
        && manifest_profile_satisfies_context_policy_with_requirement(
            &manifest.performance,
            required_context,
        )
}

fn manifest_profile_satisfies_context_policy_with_requirement(
    profile: &model_manifest::ManifestPresetProfile,
    required_context: u32,
) -> bool {
    profile.candidates.iter().any(|candidate| {
        candidate.min_context_required >= required_context
            && candidate.context_target_cap.unwrap_or(u32::MAX) >= required_context
    })
}

fn launch_contract_allows_backend(
    contract: &model_manifest::ManifestLaunchContract,
    backend: BackendMode,
) -> bool {
    match backend {
        BackendMode::DeviceLayers => true,
        BackendMode::Nccl => {
            contract.nccl_qualification == model_manifest::ManifestNcclQualification::Qualified
                && (!contract.require_primary_gpu_anchor
                    || contract.nccl_preserves_primary_gpu_anchor)
        }
    }
}

pub fn local_model_satisfies_context_policy(
    root: &Path,
    model: &str,
    env_map: &BTreeMap<String, String>,
) -> bool {
    let hardware = inspect_hardware_profile(root).ok();
    local_model_satisfies_context_policy_with_hardware(root, model, env_map, hardware.as_ref())
}

pub fn local_models_satisfying_context_policy<'a>(
    root: &Path,
    models: impl IntoIterator<Item = &'a &'static str>,
    env_map: &BTreeMap<String, String>,
) -> Vec<&'static str> {
    let hardware = inspect_hardware_profile(root).ok();
    models
        .into_iter()
        .copied()
        .filter(|model| {
            local_model_satisfies_context_policy_with_hardware(
                root,
                model,
                env_map,
                hardware.as_ref(),
            )
        })
        .collect()
}

fn family_variants(family: engine::ChatModelFamily) -> &'static [&'static str] {
    model_registry::chat_family_catalog_entry(family)
        .map(|entry| entry.planning_variants)
        .expect("chat family registry must cover every ChatModelFamily")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QualifiedGpuSlice {
    gpu_count: usize,
    min_gpu_memory_mb: u64,
    total_gpu_memory_mb: u64,
}

fn qualified_gpu_slice_matches_profile(
    profile: &model_manifest::QualifiedHostProfile,
    slice: QualifiedGpuSlice,
) -> bool {
    if slice.gpu_count < profile.min_gpu_count {
        return false;
    }
    if let Some(max_gpu_count) = profile.max_gpu_count {
        if slice.gpu_count > max_gpu_count {
            return false;
        }
    }
    if slice.min_gpu_memory_mb < profile.min_gpu_memory_mb {
        return false;
    }
    if let Some(min_total_gpu_memory_mb) = profile.min_total_gpu_memory_mb {
        if slice.total_gpu_memory_mb < min_total_gpu_memory_mb {
            return false;
        }
    }
    if let Some(max_total_gpu_memory_mb) = profile.max_total_gpu_memory_mb {
        if slice.total_gpu_memory_mb > max_total_gpu_memory_mb {
            return false;
        }
    }
    true
}

fn available_gpu_slice_for_profile(
    profile: &model_manifest::QualifiedHostProfile,
    hardware: &HardwareProfile,
) -> Option<QualifiedGpuSlice> {
    let mut sorted = hardware
        .gpus
        .iter()
        .map(|gpu| gpu.total_mb)
        .collect::<Vec<_>>();
    sorted.sort_by(|left, right| right.cmp(left));
    if sorted.is_empty() {
        return None;
    }
    let min_gpu_count = profile.min_gpu_count.max(1);
    let max_gpu_count = profile
        .max_gpu_count
        .unwrap_or(sorted.len())
        .min(sorted.len());
    if min_gpu_count > max_gpu_count {
        return None;
    }
    for gpu_count in (min_gpu_count..=max_gpu_count).rev() {
        let selected = &sorted[..gpu_count];
        let slice = QualifiedGpuSlice {
            gpu_count,
            min_gpu_memory_mb: selected.iter().copied().min().unwrap_or(0),
            total_gpu_memory_mb: selected.iter().copied().sum(),
        };
        if qualified_gpu_slice_matches_profile(profile, slice) {
            return Some(slice);
        }
    }
    None
}

fn planned_gpu_slice(plan: &ChatRuntimePlan) -> Option<QualifiedGpuSlice> {
    let selected = plan
        .gpu_allocations
        .iter()
        .filter(|gpu| gpu.chat_enabled)
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return None;
    }
    Some(QualifiedGpuSlice {
        gpu_count: selected.len(),
        min_gpu_memory_mb: selected.iter().map(|gpu| gpu.total_mb).min().unwrap_or(0),
        total_gpu_memory_mb: selected.iter().map(|gpu| gpu.total_mb).sum(),
    })
}

fn qualification_profile_for_model(
    root: Option<&Path>,
    model: &str,
) -> Option<model_manifest::RuntimeModelQualificationProfile> {
    let mut search_roots = Vec::new();
    if let Some(root) = root {
        search_roots.push(root.to_path_buf());
    }
    search_roots.push(Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf());
    search_roots.into_iter().find_map(|root| {
        model_manifest::load_runtime_model_qualification_profile(&root, model)
            .ok()
            .flatten()
    })
}

fn host_profile_supports_hardware(
    profile: &model_manifest::QualifiedHostProfile,
    hardware: &HardwareProfile,
) -> bool {
    available_gpu_slice_for_profile(profile, hardware).is_some()
}

fn qualification_host_profile_for_hardware<'a>(
    profile: &'a model_manifest::RuntimeModelQualificationProfile,
    hardware: &HardwareProfile,
) -> Option<&'a model_manifest::QualifiedHostProfile> {
    matching_available_profile(profile, hardware)
}

#[allow(dead_code)]
fn qualification_rank_for_bundle(
    root: &Path,
    bundle: &ChatPresetBundle,
    hardware: &HardwareProfile,
) -> Option<QualificationRank> {
    let profile = qualification_profile_for_model(Some(root), &bundle.model)?;
    if !plan_gpu_shape_matches_available_hardware(&bundle.selected_plan, hardware) {
        return None;
    }
    let host = matching_plan_profile(&profile, &bundle.selected_plan)
        .or_else(|| qualification_host_profile_for_hardware(&profile, hardware))?;
    let steady_tps_x100 = (host.steady_state_toks_per_sec * 100.0).round() as i64;
    let nccl_bonus = if bundle.selected_plan.tensor_parallel_backend.as_deref() == Some("nccl") {
        host.nccl_uplift_percent
            .map(|value| (value * 10.0).round() as i64)
            .unwrap_or(0)
    } else {
        0
    };
    Some(match bundle.selected_preset {
        ChatPreset::Quality => QualificationRank {
            primary: host.quality_score as i64,
            secondary: host.stability_score as i64,
            tertiary: host.validated_context_cap as i64,
            quaternary: -i64::from(host.cold_start_secs),
            quinary: steady_tps_x100,
        },
        ChatPreset::Performance => QualificationRank {
            primary: host.performance_score as i64,
            secondary: steady_tps_x100 + nccl_bonus,
            tertiary: -i64::from(host.first_token_latency_ms),
            quaternary: host.stability_score as i64,
            quinary: host.validated_context_cap as i64,
        },
    })
}

fn explicit_local_chat_family(
    env_map: &BTreeMap<String, String>,
) -> Option<engine::ChatModelFamily> {
    runtime_env::configured_chat_model_family_from_map(env_map)
        .as_deref()
        .and_then(engine::parse_chat_model_family)
}

fn explicit_local_chat_model(env_map: &BTreeMap<String, String>) -> Option<String> {
    runtime_env::configured_chat_model_from_map(env_map).and_then(|model| {
        (!model.trim().is_empty() && !engine::is_api_chat_model(&model)).then_some(model)
    })
}

fn best_bundle_for_family(
    root: &Path,
    family: engine::ChatModelFamily,
    preset: ChatPreset,
    hardware: Option<&HardwareProfile>,
    env_map: &BTreeMap<String, String>,
) -> Result<Option<ChatPresetBundle>> {
    let variants = family_variants(family);
    let Some(hardware) = hardware else {
        return Ok(variants.first().and_then(|model| {
            build_manifest_bundle_with_root(
                Some(root),
                model,
                preset,
                &HardwareProfile {
                    gpus: Vec::new(),
                    gpu0_desktop_reserve_mb: DEFAULT_GPU0_DESKTOP_RESERVE_MB,
                    fingerprint: "no-gpu-profile".to_string(),
                },
                env_map,
            )
            .ok()
            .flatten()
        }));
    };

    let mut resolved = Vec::new();
    for (index, variant) in variants.iter().enumerate() {
        let bundle = build_bundle_for_model(root, variant, preset, hardware, env_map)?;
        let qualification_shape = qualification_profile_for_model(Some(root), variant)
            .as_ref()
            .and_then(|profile| qualification_host_profile_for_hardware(profile, hardware))
            .and_then(|host| available_gpu_shape_rank(host, hardware));
        resolved.push((
            index,
            qualification_shape,
            family_selection_rank(root, &bundle, hardware),
            bundle,
        ));
    }
    resolved.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| left.0.cmp(&right.0))
            .then_with(|| right.2.cmp(&left.2))
    });
    Ok(resolved.into_iter().next().map(|(_, _, _, bundle)| bundle))
}

fn family_selection_rank(
    root: &Path,
    bundle: &ChatPresetBundle,
    hardware: &HardwareProfile,
) -> (bool, bool, i64, i64, i64, i64, i64, i64, i64, i64) {
    let quality_plan = bundle
        .plans
        .iter()
        .find(|plan| plan.preset == ChatPreset::Quality);
    let performance_plan = bundle
        .plans
        .iter()
        .find(|plan| plan.preset == ChatPreset::Performance);
    let qualification_profile = qualification_profile_for_model(Some(root), &bundle.model);
    let host = qualification_profile
        .as_ref()
        .and_then(|profile| qualification_host_profile_for_hardware(profile, hardware));
    let both_presets_hold_floor = quality_plan
        .zip(performance_plan)
        .map(|(quality, performance)| {
            !plan_uses_policy_floor(quality) && !plan_uses_policy_floor(performance)
        })
        .unwrap_or(false);
    let quality_context = quality_plan
        .map(|plan| plan.max_seq_len as i64)
        .unwrap_or(0);
    let performance_context = performance_plan
        .map(|plan| plan.max_seq_len as i64)
        .unwrap_or(0);
    let quality_score = host
        .map(|profile| profile.quality_score as i64)
        .unwrap_or(0);
    let performance_score = host
        .map(|profile| profile.performance_score as i64)
        .unwrap_or(0);
    let balanced_objective_score = quality_score.min(performance_score);
    let aggregate_objective_score = quality_score + performance_score;
    let steady_tps_x100 = host
        .map(|profile| (profile.steady_state_toks_per_sec * 100.0).round() as i64)
        .or_else(|| performance_plan.map(|plan| (plan.expected_tok_s * 100.0).round() as i64))
        .unwrap_or(0);
    (
        both_presets_hold_floor,
        host.is_some(),
        balanced_objective_score,
        aggregate_objective_score,
        host.map(|profile| profile.stability_score as i64)
            .unwrap_or(0),
        quality_context.min(performance_context),
        quality_score.max(performance_score),
        steady_tps_x100,
        host.map(|profile| -(profile.first_token_latency_ms as i64))
            .unwrap_or(0),
        host.map(|profile| -(profile.cold_start_secs as i64))
            .unwrap_or(0),
    )
}

pub fn local_chat_family_choices(
    root: &Path,
    env_map: &BTreeMap<String, String>,
) -> Vec<&'static str> {
    let hardware = inspect_hardware_profile(root).ok();
    engine::SUPPORTED_LOCAL_CHAT_FAMILIES
        .iter()
        .copied()
        .filter(|family| {
            family.variants().iter().copied().any(|model| {
                local_model_satisfies_context_policy_with_hardware(
                    root,
                    model,
                    env_map,
                    hardware.as_ref(),
                )
            })
        })
        .map(engine::ChatModelFamily::label)
        .collect()
}

pub fn resolve_local_chat_model_from_settings(
    root: &Path,
    env_map: &BTreeMap<String, String>,
) -> Result<Option<String>> {
    if infer_chat_source(env_map).eq_ignore_ascii_case("api") {
        return Ok(runtime_env::configured_chat_model_from_map(env_map));
    }
    let preset = ChatPreset::from_label(
        env_map
            .get("CTOX_CHAT_LOCAL_PRESET")
            .map(String::as_str)
            .unwrap_or(ChatPreset::Quality.label()),
    );
    if let Some(model) = explicit_local_chat_model(env_map) {
        return Ok(Some(model));
    }
    if let Some(family) = explicit_local_chat_family(env_map) {
        let hardware = inspect_hardware_profile(root).ok();
        if let Some(bundle) =
            best_bundle_for_family(root, family, preset, hardware.as_ref(), env_map)?
        {
            return Ok(Some(bundle.model));
        }
        return Ok(family_variants(family)
            .first()
            .map(|model| (*model).to_string()));
    }
    Ok(runtime_env::configured_chat_model_from_map(env_map))
}

fn local_model_satisfies_context_policy_with_hardware(
    root: &Path,
    model: &str,
    env_map: &BTreeMap<String, String>,
    hardware: Option<&HardwareProfile>,
) -> bool {
    let model = model.trim();
    let Some(manifest) = runtime_manifest(Some(root), model) else {
        return false;
    };
    if !manifest_satisfies_context_policy(&manifest) {
        return false;
    }
    let hardware = match hardware {
        Some(profile) if !profile.gpus.is_empty() => profile,
        _ => return true,
    };
    let required_context = required_context_floor_for_manifest(&manifest);
    if qualification_profile_for_model(Some(root), model)
        .as_ref()
        .and_then(|profile| qualification_host_profile_for_hardware(profile, hardware))
        .is_some_and(|host| host.validated_context_cap >= required_context)
    {
        return true;
    }
    let bundle = match build_bundle_for_model(root, model, ChatPreset::Quality, hardware, env_map) {
        Ok(bundle) => bundle,
        Err(_) => return false,
    };
    [ChatPreset::Quality, ChatPreset::Performance]
        .iter()
        .all(|preset| {
            bundle
                .plans
                .iter()
                .find(|plan| plan.preset == *preset)
                .is_some_and(|plan| plan_satisfies_required_context_policy(plan, required_context))
        })
}

pub fn preview_chat_preset_bundle(
    root: &Path,
    env_map: &BTreeMap<String, String>,
) -> Result<Option<ChatPresetBundle>> {
    if infer_chat_source(env_map).eq_ignore_ascii_case("api") {
        return Ok(None);
    }
    let hardware = match inspect_hardware_profile(root) {
        Ok(profile) if !profile.gpus.is_empty() => profile,
        _ => return Ok(None),
    };
    let selected_preset = ChatPreset::from_label(
        env_map
            .get("CTOX_CHAT_LOCAL_PRESET")
            .map(String::as_str)
            .unwrap_or(ChatPreset::Quality.label()),
    );
    if let Some(model) = explicit_local_chat_model(env_map) {
        return build_bundle_for_model(root, &model, selected_preset, &hardware, env_map).map(Some);
    }
    if let Some(family) = explicit_local_chat_family(env_map) {
        return best_bundle_for_family(root, family, selected_preset, Some(&hardware), env_map);
    }
    let default_model = engine::default_runtime_config(engine::LocalModelFamily::GptOss).model;
    let model = env_map
        .get("CTOX_CHAT_MODEL")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or(default_model.as_str());
    build_bundle_for_model(root, model, selected_preset, &hardware, env_map).map(Some)
}

pub(crate) fn persisted_chat_gpu_indices(root: &Path) -> Vec<usize> {
    load_persisted_chat_runtime_plan(root)
        .ok()
        .flatten()
        .map(|plan| {
            plan.gpu_allocations
                .into_iter()
                .filter(|allocation| allocation.chat_enabled)
                .map(|allocation| allocation.gpu_index)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn active_chat_preset(root: &Path) -> ChatPreset {
    load_persisted_chat_runtime_plan(root)
        .ok()
        .flatten()
        .map(|plan| plan.preset)
        .unwrap_or_else(|| {
            let env_map = runtime_env::load_runtime_env_map(root).unwrap_or_default();
            ChatPreset::from_label(
                env_map
                    .get("CTOX_CHAT_LOCAL_PRESET")
                    .map(String::as_str)
                    .unwrap_or(ChatPreset::Quality.label()),
            )
        })
}

fn hardware_profile_from_chat_plan(plan: &ChatRuntimePlan) -> HardwareProfile {
    let mut gpus = plan
        .gpu_allocations
        .iter()
        .map(|allocation| HardwareGpu {
            index: allocation.gpu_index,
            name: allocation.name.clone(),
            total_mb: allocation.total_mb,
        })
        .collect::<Vec<_>>();
    gpus.sort_by_key(|gpu| gpu.index);
    gpus.dedup_by_key(|gpu| gpu.index);
    HardwareProfile {
        gpus,
        gpu0_desktop_reserve_mb: gpu0_desktop_reserve_mb(),
        fingerprint: plan.hardware_fingerprint.clone(),
    }
}

fn hardware_profile_from_persisted_chat_plan(root: &Path) -> Option<HardwareProfile> {
    let plan = load_persisted_chat_runtime_plan(root).ok().flatten()?;
    if plan.gpu_allocations.is_empty() {
        return None;
    }
    let mut gpus = plan
        .gpu_allocations
        .iter()
        .map(|allocation| HardwareGpu {
            index: allocation.gpu_index,
            name: allocation.name.clone(),
            total_mb: allocation.total_mb,
        })
        .collect::<Vec<_>>();
    gpus.sort_by_key(|gpu| gpu.index);
    gpus.dedup_by_key(|gpu| gpu.index);
    Some(HardwareProfile {
        fingerprint: plan.hardware_fingerprint,
        gpus,
        gpu0_desktop_reserve_mb: gpu0_desktop_reserve_mb(),
    })
}

pub(crate) fn resolve_auxiliary_visible_devices(
    root: &Path,
    role: engine::AuxiliaryRole,
    request_model: &str,
) -> Result<Option<String>> {
    let selection = engine::auxiliary_model_selection(role, Some(request_model));
    if selection.compute_target == engine::ComputeTarget::Cpu {
        return Ok(None);
    }
    if let Some(plan) =
        load_persisted_runtime_fleet_plan(root)?.and_then(|plan| plan.auxiliary_plan(role).cloned())
    {
        if plan.request_model.eq_ignore_ascii_case(request_model)
            && plan.compute_target == engine::ComputeTarget::Gpu
        {
            return Ok(plan.visible_devices);
        }
    }
    let env_map = runtime_env::load_runtime_env_map(root).unwrap_or_default();
    let role_key = match role {
        engine::AuxiliaryRole::Embedding => "CTOX_EMBEDDING_CUDA_VISIBLE_DEVICES",
        engine::AuxiliaryRole::Stt => "CTOX_STT_CUDA_VISIBLE_DEVICES",
        engine::AuxiliaryRole::Tts => "CTOX_TTS_CUDA_VISIBLE_DEVICES",
        engine::AuxiliaryRole::Vision => "CTOX_VISION_CUDA_VISIBLE_DEVICES",
    };
    let explicit = parse_csv_indices(env_map.get(role_key));
    let shared = parse_csv_indices(env_map.get("CTOX_AUXILIARY_CUDA_VISIBLE_DEVICES"));
    let hardware = hardware_profile_from_persisted_chat_plan(root)
        .or_else(|| inspect_hardware_profile(root).ok())
        .context("failed to resolve hardware profile for auxiliary planner")?;
    let chat_gpu_indices = persisted_chat_gpu_indices(root);
    let chat_preset = active_chat_preset(root);
    let target_devices = if !explicit.is_empty() {
        explicit
    } else if !shared.is_empty() {
        shared
    } else {
        default_aux_distribution(
            Some(root),
            &selection,
            &hardware,
            chat_preset,
            &chat_gpu_indices,
        )
    };
    if target_devices.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        target_devices
            .into_iter()
            .map(|gpu_index| gpu_index.to_string())
            .collect::<Vec<_>>()
            .join(","),
    ))
}

pub fn resolve_runtime_fleet_plan(
    root: &Path,
    env_map: &BTreeMap<String, String>,
    chat_plan: Option<&ChatRuntimePlan>,
) -> Result<RuntimeFleetPlan> {
    let Some(chat_plan) = chat_plan else {
        return Ok(RuntimeFleetPlan {
            version: 1,
            hardware_fingerprint: String::new(),
            chat: None,
            embedding: None,
            transcription: None,
            speech: None,
            vision: None,
        });
    };
    let hardware = hardware_profile_from_chat_plan(chat_plan);
    let chat_gpu_indices = chat_plan
        .gpu_allocations
        .iter()
        .filter(|allocation| allocation.chat_enabled)
        .map(|allocation| allocation.gpu_index)
        .collect::<Vec<_>>();
    let aux_plans = planned_auxiliary_runtime_plans(
        Some(root),
        &hardware,
        env_map,
        chat_plan.preset,
        &chat_gpu_indices,
    );
    let auxiliary_for = |role| aux_plans.iter().find(|plan| plan.role == role).cloned();
    Ok(RuntimeFleetPlan {
        version: 1,
        hardware_fingerprint: hardware.fingerprint.clone(),
        chat: Some(chat_plan.clone()),
        embedding: auxiliary_for(engine::AuxiliaryRole::Embedding),
        transcription: auxiliary_for(engine::AuxiliaryRole::Stt),
        speech: auxiliary_for(engine::AuxiliaryRole::Tts),
        vision: auxiliary_for(engine::AuxiliaryRole::Vision),
    })
}

pub fn apply_chat_runtime_plan(
    root: &Path,
    env_map: &mut BTreeMap<String, String>,
) -> Result<Option<ChatRuntimePlan>> {
    let treat_existing_engine_limits_as_explicit = !env_map
        .get("CTOX_CHAT_RUNTIME_PLAN_ACTIVE")
        .map(|value| value.trim() == "1")
        .unwrap_or(false)
        && !env_map.contains_key("CTOX_CHAT_RUNTIME_PLAN_DIGEST")
        && !env_map.contains_key("CTOX_ENGINE_REALIZED_MAX_SEQ_LEN")
        && !env_map.contains_key("CTOX_CHAT_MODEL_REALIZED_CONTEXT")
        && !env_map.contains_key("CTOX_ENGINE_REALIZED_MODEL");
    let explicit_max_seq_len_cap = treat_existing_engine_limits_as_explicit
        .then(|| {
            env_map
                .get("CTOX_ENGINE_MAX_SEQ_LEN")
                .and_then(|value| value.trim().parse::<u32>().ok())
                .filter(|value| *value > 0)
        })
        .flatten();
    let explicit_max_batch_size_cap = treat_existing_engine_limits_as_explicit
        .then(|| {
            env_map
                .get("CTOX_ENGINE_MAX_BATCH_SIZE")
                .and_then(|value| value.trim().parse::<u32>().ok())
                .filter(|value| *value > 0)
        })
        .flatten();
    let explicit_max_seqs_cap = treat_existing_engine_limits_as_explicit
        .then(|| {
            env_map
                .get("CTOX_ENGINE_MAX_SEQS")
                .and_then(|value| value.trim().parse::<u32>().ok())
                .filter(|value| *value > 0)
        })
        .flatten();
    clear_chat_plan_env(env_map);
    if infer_chat_source(env_map).eq_ignore_ascii_case("api") {
        persist_chat_runtime_plan(root, None)?;
        runtime_contract::clear_chat_capacity_contract(root)?;
        return Ok(None);
    }
    if let Some(plan) = reusable_persisted_chat_runtime_plan(
        root,
        env_map,
        explicit_max_seq_len_cap,
        explicit_max_batch_size_cap,
        explicit_max_seqs_cap,
    )? {
        apply_chat_runtime_plan_env(root, &plan, env_map)?;
        persist_chat_runtime_plan(root, Some(&plan))?;
        runtime_contract::persist_chat_capacity_contract(
            root,
            &chat_capacity_contract_from_plan(&plan),
        )?;
        return Ok(Some(plan));
    }
    let Some(bundle) = preview_chat_preset_bundle(root, env_map)? else {
        persist_chat_runtime_plan(root, None)?;
        runtime_contract::clear_chat_capacity_contract(root)?;
        return Ok(None);
    };
    let required_context_floor = runtime_manifest(Some(root), &bundle.model)
        .as_ref()
        .map(required_context_floor_for_manifest)
        .unwrap_or(MIN_POLICY_CONTEXT);
    let explicit_model_requested = explicit_local_chat_model(env_map).is_some();
    let explicit_family_requested = explicit_local_chat_family(env_map).is_some();
    let enforce_global_context_policy = !explicit_family_requested && !explicit_model_requested;
    if enforce_global_context_policy
        && !local_model_satisfies_context_policy(root, &bundle.model, env_map)
    {
        anyhow::bail!(
            "local chat model {} is unavailable on this system because CTOX now requires at least {} tokens of planned sequence length for both Quality and Performance presets",
            bundle.model,
            MIN_POLICY_CONTEXT
        );
    }
    let mut plan = bundle.selected_plan.clone();
    if let Some(cap) = explicit_max_seq_len_cap {
        if cap < required_context_floor {
            anyhow::bail!(
                "explicit CTOX_ENGINE_MAX_SEQ_LEN cap {} violates the {} token local chat minimum",
                cap,
                required_context_floor
            );
        }
        plan.max_seq_len = plan.max_seq_len.min(cap);
    }
    if let Some(cap) = explicit_max_batch_size_cap {
        plan.max_batch_size = plan.max_batch_size.min(cap);
    }
    if let Some(cap) = explicit_max_seqs_cap {
        plan.max_seqs = plan.max_seqs.min(cap);
    }
    if enforce_global_context_policy && !plan_satisfies_context_policy(&plan) {
        anyhow::bail!(
            "local chat runtime plan for {} / {} does not satisfy the {} token minimum",
            plan.model,
            plan.preset.label(),
            MIN_POLICY_CONTEXT
        );
    }
    if !plan_satisfies_required_context_policy(&plan, required_context_floor) {
        anyhow::bail!(
            "local chat runtime plan for {} / {} does not satisfy the {} token model contract minimum",
            plan.model,
            plan.preset.label(),
            required_context_floor
        );
    }
    apply_chat_runtime_plan_env(root, &plan, env_map)?;
    persist_chat_runtime_plan(root, Some(&plan))?;
    runtime_contract::persist_chat_capacity_contract(
        root,
        &chat_capacity_contract_from_plan(&plan),
    )?;
    Ok(Some(plan))
}

fn reusable_persisted_chat_runtime_plan(
    root: &Path,
    env_map: &BTreeMap<String, String>,
    explicit_max_seq_len_cap: Option<u32>,
    explicit_max_batch_size_cap: Option<u32>,
    explicit_max_seqs_cap: Option<u32>,
) -> Result<Option<ChatRuntimePlan>> {
    let Some(plan) = load_persisted_chat_runtime_plan(root)? else {
        return Ok(None);
    };
    let requested_preset = ChatPreset::from_label(
        env_map
            .get("CTOX_CHAT_LOCAL_PRESET")
            .map(String::as_str)
            .unwrap_or(ChatPreset::Quality.label()),
    );
    if plan.preset != requested_preset {
        return Ok(None);
    }
    let requested_model = explicit_local_chat_model(env_map)
        .or_else(|| env_map.get("CTOX_CHAT_MODEL").cloned())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(requested_model) = requested_model.as_ref() {
        if !plan.model.eq_ignore_ascii_case(requested_model.as_str()) {
            return Ok(None);
        }
    }
    if explicit_max_seq_len_cap.is_some_and(|cap| plan.max_seq_len > cap) {
        return Ok(None);
    }
    if explicit_max_batch_size_cap.is_some_and(|cap| plan.max_batch_size > cap) {
        return Ok(None);
    }
    if explicit_max_seqs_cap.is_some_and(|cap| plan.max_seqs > cap) {
        return Ok(None);
    }
    let fresh_bundle = match inspect_hardware_profile(root) {
        Ok(hardware) if !hardware.gpus.is_empty() => {
            if let Some(requested_model) = requested_model.clone() {
                build_bundle_for_model(root, &requested_model, requested_preset, &hardware, env_map)
                    .ok()
            } else if let Some(family) = explicit_local_chat_family(env_map) {
                best_bundle_for_family(root, family, requested_preset, Some(&hardware), env_map)?
            } else {
                let default_model =
                    engine::default_runtime_config(engine::LocalModelFamily::GptOss).model;
                let model = env_map
                    .get("CTOX_CHAT_MODEL")
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                    .unwrap_or(default_model.as_str());
                build_bundle_for_model(root, model, requested_preset, &hardware, env_map).ok()
            }
        }
        _ => None,
    };
    if fresh_bundle
        .as_ref()
        .is_some_and(|bundle| bundle.selected_plan != plan)
    {
        return Ok(None);
    }
    Ok(Some(plan))
}

pub fn apply_chat_runtime_plan_env(
    root: &Path,
    plan: &ChatRuntimePlan,
    env_map: &mut BTreeMap<String, String>,
) -> Result<()> {
    let plan_json =
        serde_json::to_vec_pretty(plan).context("failed to encode chat runtime plan")?;
    let digest = format!("{:x}", Sha256::digest(&plan_json));
    let platform_status = plan
        .rationale
        .iter()
        .filter(|line| line.starts_with("platform ") || line.contains("platform contract"))
        .cloned()
        .collect::<Vec<_>>()
        .join(" | ");
    env_map.insert(
        "CTOX_CHAT_LOCAL_PRESET".to_string(),
        plan.preset.label().to_string(),
    );
    env_map.insert("CTOX_CHAT_MODEL".to_string(), plan.model.clone());
    env_map.insert("CTOX_CHAT_MODEL_BASE".to_string(), plan.model.clone());
    env_map.insert(
        "CTOX_CHAT_COMPACTION_THRESHOLD_PERCENT".to_string(),
        plan.compaction_threshold_percent.to_string(),
    );
    env_map.insert(
        "CTOX_CHAT_COMPACTION_MIN_TOKENS".to_string(),
        plan.compaction_min_tokens.to_string(),
    );
    env_map.insert("CTOX_ENGINE_MODEL".to_string(), plan.model.clone());
    let runtime_port = engine::runtime_config_for_model(&plan.model)
        .map(|runtime| runtime.port)
        .unwrap_or(1234);
    env_map.insert("CTOX_ENGINE_PORT".to_string(), runtime_port.to_string());
    env_map.insert(
        "CTOX_ENGINE_MAX_SEQ_LEN".to_string(),
        plan.max_seq_len.to_string(),
    );
    env_map.insert(
        "CTOX_ENGINE_REALIZED_MAX_SEQ_LEN".to_string(),
        plan.max_seq_len.to_string(),
    );
    env_map.insert(
        "CTOX_CHAT_MODEL_REALIZED_CONTEXT".to_string(),
        plan.max_seq_len.to_string(),
    );
    env_map.insert("CTOX_ENGINE_REALIZED_MODEL".to_string(), plan.model.clone());
    if !platform_status.is_empty() {
        env_map.insert("CTOX_PLATFORM_CONTRACT_STATUS".to_string(), platform_status);
    }
    if let Some(runtime_isq) = &plan.runtime_isq {
        env_map.insert("CTOX_ENGINE_ISQ".to_string(), runtime_isq.clone());
    }
    if let Some(from_uqff) = available_chat_quant_artifact(root, plan) {
        env_map.insert(
            "CTOX_ENGINE_FROM_UQFF".to_string(),
            from_uqff.display().to_string(),
        );
    }
    env_map.insert(
        "CTOX_ENGINE_PAGED_ATTN".to_string(),
        plan.paged_attn.clone(),
    );
    if let Some(cache_type) = &plan.pa_cache_type {
        env_map.insert("CTOX_ENGINE_PA_CACHE_TYPE".to_string(), cache_type.clone());
    }
    if let Some(memory_fraction) = &plan.pa_memory_fraction {
        env_map.insert(
            "CTOX_ENGINE_PA_MEMORY_FRACTION".to_string(),
            memory_fraction.clone(),
        );
    }
    if let Some(context_len) = plan.pa_context_len {
        env_map.insert(
            "CTOX_ENGINE_PA_CONTEXT_LEN".to_string(),
            context_len.to_string(),
        );
    }
    env_map.insert(
        "CTOX_ENGINE_DISABLE_NCCL".to_string(),
        if plan.disable_nccl { "1" } else { "0" }.to_string(),
    );
    if let Some(backend) = &plan.tensor_parallel_backend {
        env_map.insert(
            "CTOX_ENGINE_TENSOR_PARALLEL_BACKEND".to_string(),
            backend.clone(),
        );
    }
    if let Some(world_size) = plan.mn_local_world_size {
        env_map.insert(
            "CTOX_ENGINE_MN_LOCAL_WORLD_SIZE".to_string(),
            world_size.to_string(),
        );
    }
    env_map.insert(
        "CTOX_ENGINE_MAX_BATCH_SIZE".to_string(),
        plan.max_batch_size.to_string(),
    );
    env_map.insert(
        "CTOX_ENGINE_MAX_SEQS".to_string(),
        plan.max_seqs.to_string(),
    );
    env_map.insert(
        "CTOX_ENGINE_CUDA_VISIBLE_DEVICES".to_string(),
        plan.cuda_visible_devices.clone(),
    );
    if let Some(device_layers) = &plan.device_layers {
        env_map.insert(
            "CTOX_ENGINE_DEVICE_LAYERS".to_string(),
            device_layers.clone(),
        );
    }
    if let Some(topology) = &plan.topology {
        let topology_path = if Path::new(topology).is_absolute() {
            topology.clone()
        } else {
            root.join(topology).display().to_string()
        };
        env_map.insert("CTOX_ENGINE_TOPOLOGY".to_string(), topology_path);
    }
    if plan.allow_device_layers_with_topology {
        env_map.insert(
            "CTOX_ENGINE_ALLOW_DEVICE_LAYERS_WITH_TOPOLOGY".to_string(),
            "1".to_string(),
        );
    }
    if let Some(ordinal) = plan.nm_device_ordinal {
        env_map.insert(
            "CTOX_ENGINE_NM_DEVICE_ORDINAL".to_string(),
            ordinal.to_string(),
        );
    }
    if let Some(ordinal) = plan.base_device_ordinal {
        env_map.insert(
            "CTOX_ENGINE_BASE_DEVICE_ORDINAL".to_string(),
            ordinal.to_string(),
        );
    }
    if let Some(backend) = &plan.moe_experts_backend {
        env_map.insert(
            "CTOX_ENGINE_MOE_EXPERTS_BACKEND".to_string(),
            backend.clone(),
        );
    }
    if plan.disable_flash_attn {
        env_map.insert(
            "CTOX_ENGINE_DISABLE_FLASH_ATTN".to_string(),
            "1".to_string(),
        );
    }
    if plan.force_no_mmap {
        env_map.insert("CTOX_ENGINE_NO_MMAP".to_string(), "1".to_string());
    }
    if plan.force_language_model_only {
        env_map.insert(
            "CTOX_ENGINE_LANGUAGE_MODEL_ONLY".to_string(),
            "1".to_string(),
        );
    }
    if plan.isq_singlethread {
        env_map.insert("CTOX_ENGINE_ISQ_SINGLETHREAD".to_string(), "1".to_string());
    }
    if let Some(cpu_threads) = plan.isq_cpu_threads {
        env_map.insert(
            "CTOX_ENGINE_ISQ_CPU_THREADS".to_string(),
            cpu_threads.to_string(),
        );
    }
    env_map.insert("CTOX_CHAT_RUNTIME_PLAN_DIGEST".to_string(), digest);
    env_map.insert("CTOX_CHAT_RUNTIME_PLAN_ACTIVE".to_string(), "1".to_string());
    env_map.insert(
        "CTOX_CHAT_RUNTIME_PLAN_PATH".to_string(),
        root.join(CHAT_PLAN_RELATIVE_PATH).display().to_string(),
    );
    Ok(())
}

pub fn reconcile_chat_runtime_plan(root: &Path) -> Result<Option<ChatRuntimePlan>> {
    let mut env_map = runtime_env::load_runtime_env_map(root).unwrap_or_default();
    let plan = apply_chat_runtime_plan(root, &mut env_map)?;
    runtime_env::save_runtime_env_map(root, &env_map)?;
    Ok(plan)
}

pub fn load_persisted_chat_runtime_plan(root: &Path) -> Result<Option<ChatRuntimePlan>> {
    let path = root.join(CHAT_PLAN_RELATIVE_PATH);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)
        .with_context(|| format!("failed to read chat runtime plan {}", path.display()))?;
    let plan = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse chat runtime plan {}", path.display()))?;
    Ok(Some(plan))
}

pub fn load_persisted_runtime_fleet_plan(root: &Path) -> Result<Option<RuntimeFleetPlan>> {
    let path = root.join(RUNTIME_FLEET_PLAN_RELATIVE_PATH);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)
        .with_context(|| format!("failed to read runtime fleet plan {}", path.display()))?;
    let plan = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse runtime fleet plan {}", path.display()))?;
    Ok(Some(plan))
}

pub fn clear_persisted_chat_runtime_plan(root: &Path) -> Result<()> {
    persist_chat_runtime_plan(root, None)?;
    runtime_contract::clear_chat_capacity_contract(root)?;
    Ok(())
}

pub fn store_persisted_chat_runtime_plan(
    root: &Path,
    plan: Option<&ChatRuntimePlan>,
) -> Result<()> {
    persist_chat_runtime_plan(root, plan)?;
    if plan.is_none() {
        runtime_contract::clear_chat_capacity_contract(root)?;
    }
    Ok(())
}

pub fn store_persisted_runtime_fleet_plan(
    root: &Path,
    plan: Option<&RuntimeFleetPlan>,
) -> Result<()> {
    persist_runtime_fleet_plan(root, plan)
}

pub fn load_persisted_chat_runtime_plan_digest(root: &Path) -> Result<Option<String>> {
    let Some(plan) = load_persisted_chat_runtime_plan(root)? else {
        return Ok(None);
    };
    let bytes =
        serde_json::to_vec_pretty(&plan).context("failed to encode persisted chat runtime plan")?;
    Ok(Some(format!("{:x}", Sha256::digest(&bytes))))
}

pub fn validate_live_gpu_budget(
    plan: &ChatRuntimePlan,
    snapshot: &resource_state::ResourceSnapshot,
) -> Result<()> {
    let mut violations = Vec::new();
    for allocation in plan.gpu_allocations.iter().filter(|gpu| gpu.chat_enabled) {
        let Some(live_gpu) = snapshot.gpu(allocation.gpu_index) else {
            violations.push(format!(
                "gpu{} missing from live snapshot {}",
                allocation.gpu_index, snapshot.source
            ));
            continue;
        };
        let required_free_mb = allocation_required_free_mb(allocation);
        if live_gpu.free_mb < required_free_mb {
            violations.push(format!(
                "gpu{} free={}MB required={}MB (desktop={}MB aux={}MB chat={}MB backend={}MB activation={}MB load_peak={}MB)",
                allocation.gpu_index,
                live_gpu.free_mb,
                required_free_mb,
                allocation.desktop_reserve_mb,
                allocation.aux_reserve_mb,
                allocation.chat_budget_mb,
                allocation.backend_overhead_mb,
                allocation.activation_overhead_mb,
                allocation.load_peak_overhead_mb,
            ));
        }
    }
    if violations.is_empty() {
        return Ok(());
    }
    anyhow::bail!(
        "live GPU free memory no longer satisfies plan {} / {} on {}: {}",
        plan.model,
        plan.preset.label(),
        snapshot.source,
        violations.join(" | ")
    );
}

pub fn clear_chat_plan_env(env_map: &mut BTreeMap<String, String>) {
    for key in [
        "CTOX_CHAT_RUNTIME_PLAN_DIGEST",
        "CTOX_CHAT_RUNTIME_PLAN_ACTIVE",
        "CTOX_CHAT_RUNTIME_PLAN_PATH",
        "CTOX_ENGINE_FROM_UQFF",
        "CTOX_CHAT_COMPACTION_THRESHOLD_PERCENT",
        "CTOX_CHAT_COMPACTION_MIN_TOKENS",
        "CTOX_ENGINE_ISQ",
        "CTOX_ENGINE_PAGED_ATTN",
        "CTOX_ENGINE_PA_CACHE_TYPE",
        "CTOX_ENGINE_PA_MEMORY_FRACTION",
        "CTOX_ENGINE_PA_CONTEXT_LEN",
        "CTOX_ENGINE_DISABLE_NCCL",
        "CTOX_ENGINE_TENSOR_PARALLEL_BACKEND",
        "CTOX_ENGINE_MN_LOCAL_WORLD_SIZE",
        "CTOX_ENGINE_MAX_BATCH_SIZE",
        "CTOX_ENGINE_MAX_SEQS",
        "CTOX_ENGINE_DISABLE_FLASH_ATTN",
        "CTOX_ENGINE_NO_MMAP",
        "CTOX_ENGINE_LANGUAGE_MODEL_ONLY",
        "CTOX_ENGINE_ISQ_SINGLETHREAD",
        "CTOX_ENGINE_ISQ_CPU_THREADS",
        "CTOX_ENGINE_MAX_SEQ_LEN",
        "CTOX_ENGINE_NUM_DEVICE_LAYERS",
        "CTOX_ENGINE_DEVICE_LAYERS",
        "CTOX_ENGINE_TOPOLOGY",
        "CTOX_ENGINE_ALLOW_DEVICE_LAYERS_WITH_TOPOLOGY",
        "CTOX_ENGINE_NM_DEVICE_ORDINAL",
        "CTOX_ENGINE_BASE_DEVICE_ORDINAL",
        "CTOX_ENGINE_MOE_EXPERTS_BACKEND",
        "CTOX_ENGINE_CUDA_VISIBLE_DEVICES",
        "CTOX_ENGINE_REALIZED_MAX_SEQ_LEN",
        "CTOX_CHAT_MODEL_REALIZED_CONTEXT",
        "CTOX_ENGINE_REALIZED_MODEL",
        "CTOX_LOCAL_ADAPTER_REASONING_CAP",
        "CTOX_LOCAL_ADAPTER_MAX_OUTPUT_TOKENS_CAP",
    ] {
        env_map.remove(key);
    }
}

fn persist_chat_runtime_plan(root: &Path, plan: Option<&ChatRuntimePlan>) -> Result<()> {
    let path = root.join(CHAT_PLAN_RELATIVE_PATH);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create planner dir {}", parent.display()))?;
    }
    match plan {
        Some(plan) => {
            let bytes = serde_json::to_vec_pretty(plan).context("failed to encode planner json")?;
            std::fs::write(&path, bytes)
                .with_context(|| format!("failed to write {}", path.display()))?;
        }
        None => {
            let _ = std::fs::remove_file(&path);
        }
    }
    Ok(())
}

fn persist_runtime_fleet_plan(root: &Path, plan: Option<&RuntimeFleetPlan>) -> Result<()> {
    let path = root.join(RUNTIME_FLEET_PLAN_RELATIVE_PATH);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create fleet planner dir {}", parent.display()))?;
    }
    match plan {
        Some(plan) => {
            let bytes =
                serde_json::to_vec_pretty(plan).context("failed to encode runtime fleet json")?;
            std::fs::write(&path, bytes)
                .with_context(|| format!("failed to write {}", path.display()))?;
        }
        None => {
            let _ = std::fs::remove_file(&path);
        }
    }
    Ok(())
}

fn build_bundle_for_model(
    root: &Path,
    model: &str,
    selected_preset: ChatPreset,
    hardware: &HardwareProfile,
    env_map: &BTreeMap<String, String>,
) -> Result<ChatPresetBundle> {
    build_manifest_bundle_with_root(Some(root), model, selected_preset, hardware, env_map)?
        .ok_or_else(|| anyhow::anyhow!("unsupported runtime planner model: {}", model.trim()))
}

fn build_manifest_bundle_with_root(
    root: Option<&Path>,
    model: &str,
    selected_preset: ChatPreset,
    hardware: &HardwareProfile,
    env_map: &BTreeMap<String, String>,
) -> Result<Option<ChatPresetBundle>> {
    let Some(manifest) = runtime_manifest(root, model) else {
        return Ok(None);
    };
    let platform = collect_platform_facts(root, hardware);
    let harness = harness_from_manifest(&manifest);
    let quality = plan_from_specs(
        root,
        &platform,
        &manifest,
        harness,
        ChatPreset::Quality,
        &plan_specs_from_manifest_profile(&manifest.quality),
        env_map,
    );
    let performance = plan_from_specs(
        root,
        &platform,
        &manifest,
        harness,
        ChatPreset::Performance,
        &plan_specs_from_manifest_profile(&manifest.performance),
        env_map,
    );
    let bundle = bundle_from_plans(
        harness.model,
        selected_preset,
        hardware,
        vec![quality, performance],
    );
    Ok(Some(bundle))
}

fn bundle_from_plans(
    model: &str,
    selected_preset: ChatPreset,
    hardware: &HardwareProfile,
    plans: Vec<ChatRuntimePlan>,
) -> ChatPresetBundle {
    let selected_plan = plans
        .iter()
        .find(|plan| plan.preset == selected_preset)
        .cloned()
        .unwrap_or_else(|| plans[0].clone());
    ChatPresetBundle {
        model: model.to_string(),
        hardware: hardware.clone(),
        selected_preset,
        selected_plan,
        plans,
    }
}

fn plan_from_specs(
    root: Option<&Path>,
    platform: &PlatformFacts,
    manifest: &model_manifest::RuntimeModelManifest,
    harness: ModelHarness,
    preset: ChatPreset,
    specs: &[PlanSpec],
    env_map: &BTreeMap<String, String>,
) -> ChatRuntimePlan {
    let candidates =
        build_feasible_candidates(root, platform, manifest, harness, preset, specs, env_map);
    if let Some(best) = rank_feasible_candidates(root, platform, harness.model, preset, &candidates)
    {
        return best;
    }
    let fallback_spec = select_floor_fallback_spec(specs).unwrap_or(PlanSpec {
        quant: Q4K,
        backend: BackendMode::DeviceLayers,
        context_target: Some(MIN_POLICY_CONTEXT),
        context_fraction_milli: 1000,
        min_context_required: MIN_POLICY_CONTEXT,
        per_gpu_headroom_mb: 0,
        max_batch_size: 1,
        max_seqs: 1,
    });
    build_floor_fallback_plan(
        root,
        platform,
        manifest,
        harness,
        preset,
        fallback_spec,
        env_map,
    )
}

fn select_floor_fallback_spec(specs: &[PlanSpec]) -> Option<PlanSpec> {
    specs.iter().copied().min_by(|left, right| {
        let backend_rank = |backend: BackendMode| match backend {
            BackendMode::DeviceLayers => 0u8,
            BackendMode::Nccl => 1u8,
        };
        (
            left.quant.weight_factor_milli,
            backend_rank(left.backend),
            left.per_gpu_headroom_mb,
            left.max_batch_size,
            left.max_seqs,
            left.context_target.unwrap_or(u32::MAX),
        )
            .cmp(&(
                right.quant.weight_factor_milli,
                backend_rank(right.backend),
                right.per_gpu_headroom_mb,
                right.max_batch_size,
                right.max_seqs,
                right.context_target.unwrap_or(u32::MAX),
            ))
    })
}

#[cfg(test)]
fn build_gpt_oss_20b_bundle(
    selected_preset: ChatPreset,
    hardware: &HardwareProfile,
    env_map: &BTreeMap<String, String>,
) -> ChatPresetBundle {
    build_manifest_bundle_with_root(
        None,
        "openai/gpt-oss-20b",
        selected_preset,
        hardware,
        env_map,
    )
    .expect("gpt-oss manifest bundle should resolve")
    .expect("gpt-oss manifest bundle should exist")
}

#[cfg(test)]
fn build_qwen35_4b_bundle(
    selected_preset: ChatPreset,
    hardware: &HardwareProfile,
    env_map: &BTreeMap<String, String>,
) -> ChatPresetBundle {
    build_manifest_bundle_with_root(None, "Qwen/Qwen3.5-4B", selected_preset, hardware, env_map)
        .expect("qwen3.5-4b manifest bundle should resolve")
        .expect("qwen3.5-4b manifest bundle should exist")
}

#[cfg(test)]
fn build_qwen35_9b_bundle(
    selected_preset: ChatPreset,
    hardware: &HardwareProfile,
    env_map: &BTreeMap<String, String>,
) -> ChatPresetBundle {
    build_manifest_bundle_with_root(None, "Qwen/Qwen3.5-9B", selected_preset, hardware, env_map)
        .expect("qwen3.5-9b manifest bundle should resolve")
        .expect("qwen3.5-9b manifest bundle should exist")
}

#[cfg(test)]
fn build_qwen35_27b_bundle(
    selected_preset: ChatPreset,
    hardware: &HardwareProfile,
    env_map: &BTreeMap<String, String>,
) -> ChatPresetBundle {
    build_manifest_bundle_with_root(None, "Qwen/Qwen3.5-27B", selected_preset, hardware, env_map)
        .expect("qwen3.5-27b manifest bundle should resolve")
        .expect("qwen3.5-27b manifest bundle should exist")
}

#[cfg(test)]
fn build_qwen35_35b_a3b_bundle(
    selected_preset: ChatPreset,
    hardware: &HardwareProfile,
    env_map: &BTreeMap<String, String>,
) -> ChatPresetBundle {
    build_manifest_bundle_with_root(
        None,
        "Qwen/Qwen3.5-35B-A3B",
        selected_preset,
        hardware,
        env_map,
    )
    .expect("qwen3.5-35b-a3b manifest bundle should resolve")
    .expect("qwen3.5-35b-a3b manifest bundle should exist")
}

#[cfg(test)]
fn build_gemma4_26b_a4b_bundle(
    selected_preset: ChatPreset,
    hardware: &HardwareProfile,
    env_map: &BTreeMap<String, String>,
) -> ChatPresetBundle {
    build_manifest_bundle_with_root(
        None,
        "google/gemma-4-26B-A4B-it",
        selected_preset,
        hardware,
        env_map,
    )
    .expect("gemma-4-26b-a4b manifest bundle should resolve")
    .expect("gemma-4-26b-a4b manifest bundle should exist")
}

#[cfg(test)]
fn build_gemma4_31b_bundle(
    selected_preset: ChatPreset,
    hardware: &HardwareProfile,
    env_map: &BTreeMap<String, String>,
) -> ChatPresetBundle {
    build_manifest_bundle_with_root(
        None,
        "google/gemma-4-31B-it",
        selected_preset,
        hardware,
        env_map,
    )
    .expect("gemma-4-31b manifest bundle should resolve")
    .expect("gemma-4-31b manifest bundle should exist")
}

#[cfg(test)]
fn build_nemotron_cascade_bundle(
    selected_preset: ChatPreset,
    hardware: &HardwareProfile,
    env_map: &BTreeMap<String, String>,
) -> ChatPresetBundle {
    build_nemotron_cascade_bundle_with_root(None, selected_preset, hardware, env_map)
}

#[cfg(test)]
fn build_nemotron_cascade_bundle_with_root(
    root: Option<&Path>,
    selected_preset: ChatPreset,
    hardware: &HardwareProfile,
    env_map: &BTreeMap<String, String>,
) -> ChatPresetBundle {
    build_manifest_bundle_with_root(
        root,
        "nvidia/Nemotron-Cascade-2-30B-A3B",
        selected_preset,
        hardware,
        env_map,
    )
    .expect("nemotron manifest bundle should resolve")
    .expect("nemotron manifest bundle should exist")
}

#[cfg(test)]
fn build_glm47_flash_bundle(
    selected_preset: ChatPreset,
    hardware: &HardwareProfile,
    env_map: &BTreeMap<String, String>,
) -> ChatPresetBundle {
    build_manifest_bundle_with_root(
        None,
        "zai-org/GLM-4.7-Flash",
        selected_preset,
        hardware,
        env_map,
    )
    .expect("glm-4.7-flash manifest bundle should resolve")
    .expect("glm-4.7-flash manifest bundle should exist")
}

fn choose_best_candidate(
    preset: ChatPreset,
    candidates: &[ChatRuntimePlan],
) -> Option<ChatRuntimePlan> {
    let mut sorted = candidates.to_vec();
    sorted.sort_by(|left, right| candidate_rank(preset, right).cmp(&candidate_rank(preset, left)));
    sorted.into_iter().next()
}

fn candidate_rank(preset: ChatPreset, plan: &ChatRuntimePlan) -> (i64, i64, i64, i64, i64, i64) {
    let quant_rank_high = match plan.quantization.as_str() {
        "native_mxfp4" => 4,
        "Q6K" => 3,
        "Q5K" => 2,
        _ => 1,
    };
    let quant_rank_low = match plan.quantization.as_str() {
        "native_mxfp4" => 4,
        "Q4K" => 3,
        "Q5K" => 2,
        _ => 1,
    };
    let shape = planned_gpu_slice(plan);
    let min_gpu_memory_rank = shape
        .as_ref()
        .map(|slice| slice.min_gpu_memory_mb as i64)
        .unwrap_or(0);
    let total_gpu_memory_rank = shape
        .as_ref()
        .map(|slice| slice.total_gpu_memory_mb as i64)
        .unwrap_or(0);
    let gpu_count_rank = shape
        .as_ref()
        .map(|slice| slice.gpu_count as i64)
        .unwrap_or(0);
    let performance_backend_rank = if plan.tensor_parallel_backend.as_deref() == Some("nccl") {
        1
    } else {
        0
    };
    let expected_tps_rank = (plan.expected_tok_s * 100.0).round() as i64;
    match preset {
        ChatPreset::Quality => (
            quant_rank_high,
            plan.max_seq_len as i64,
            min_gpu_memory_rank,
            total_gpu_memory_rank,
            gpu_count_rank,
            expected_tps_rank,
        ),
        ChatPreset::Performance => (
            expected_tps_rank,
            performance_backend_rank,
            min_gpu_memory_rank,
            total_gpu_memory_rank,
            gpu_count_rank,
            quant_rank_low,
        ),
    }
}

fn build_feasible_candidates(
    root: Option<&Path>,
    platform: &PlatformFacts,
    manifest: &model_manifest::RuntimeModelManifest,
    harness: ModelHarness,
    preset: ChatPreset,
    specs: &[PlanSpec],
    env_map: &BTreeMap<String, String>,
) -> Vec<ChatRuntimePlan> {
    let mut candidates = Vec::new();
    for spec in specs {
        if !launch_contract_allows_backend(&manifest.launch_contract, spec.backend) {
            continue;
        }
        if let Some(candidate) =
            build_candidate(root, platform, manifest, harness, preset, *spec, env_map)
        {
            candidates.push(candidate);
        }
    }
    candidates
}

fn rank_feasible_candidates(
    root: Option<&Path>,
    platform: &PlatformFacts,
    model: &str,
    preset: ChatPreset,
    candidates: &[ChatRuntimePlan],
) -> Option<ChatRuntimePlan> {
    let qualification_profile = qualification_profile_for_model(root, model);
    if qualification_profile.is_none() {
        return candidates.first().cloned().map(|mut plan| {
            plan.rationale
                .push("ranking source manifest candidate order".to_string());
            plan
        });
    }

    let has_matching_qualified_candidate = qualification_profile.as_ref().is_some_and(|profile| {
        candidates
            .iter()
            .any(|plan| matching_plan_profile(profile, plan).is_some())
    });
    if !has_matching_qualified_candidate {
        return candidates.first().cloned().map(|mut plan| {
            plan.rationale.push(
                "ranking source manifest candidate order (no matching qualification profile)"
                    .to_string(),
            );
            plan
        });
    }

    if let Some(mut plan) =
        choose_best_subset_candidate(root, model, preset, &platform.hardware, candidates)
    {
        if let Some(host) = qualification_profile
            .as_ref()
            .and_then(|profile| matching_plan_profile(profile, &plan))
        {
            plan.rationale
                .push(format!("ranking source qualification {}", host.host_class));
        } else {
            plan.rationale
                .push("ranking source heuristic candidate ranking".to_string());
        }
        return Some(plan);
    }
    choose_best_candidate(preset, candidates).map(|mut plan| {
        plan.rationale
            .push("ranking source heuristic candidate ranking".to_string());
        plan
    })
}

fn host_profile_rank(
    preset: ChatPreset,
    host: &model_manifest::QualifiedHostProfile,
) -> QualificationRank {
    let steady_tps_x100 = (host.steady_state_toks_per_sec * 100.0).round() as i64;
    let nccl_bonus = host
        .nccl_uplift_percent
        .map(|value| (value * 10.0).round() as i64)
        .unwrap_or(0);
    match preset {
        ChatPreset::Quality => QualificationRank {
            primary: host.quality_score as i64,
            secondary: host.stability_score as i64,
            tertiary: host.validated_context_cap as i64,
            quaternary: -i64::from(host.cold_start_secs),
            quinary: steady_tps_x100,
        },
        ChatPreset::Performance => QualificationRank {
            primary: host.performance_score as i64,
            secondary: steady_tps_x100 + nccl_bonus,
            tertiary: -i64::from(host.first_token_latency_ms),
            quaternary: host.stability_score as i64,
            quinary: host.validated_context_cap as i64,
        },
    }
}

fn choose_best_subset_candidate(
    root: Option<&Path>,
    model: &str,
    preset: ChatPreset,
    hardware: &HardwareProfile,
    candidates: &[ChatRuntimePlan],
) -> Option<ChatRuntimePlan> {
    let qualification_profile = qualification_profile_for_model(root, model);
    let mut sorted = candidates.to_vec();
    sorted.sort_by(|left, right| {
        let left_host = qualification_profile
            .as_ref()
            .and_then(|profile| matching_plan_profile(profile, left));
        let right_host = qualification_profile
            .as_ref()
            .and_then(|profile| matching_plan_profile(profile, right));
        plan_gpu_shape_matches_available_hardware(right, hardware)
            .cmp(&plan_gpu_shape_matches_available_hardware(left, hardware))
            .then_with(|| candidate_rank(preset, right).cmp(&candidate_rank(preset, left)))
            .then_with(|| right_host.is_some().cmp(&left_host.is_some()))
            .then_with(|| {
                right_host
                    .map(|host| host_profile_rank(preset, host))
                    .cmp(&left_host.map(|host| host_profile_rank(preset, host)))
            })
    });
    sorted.into_iter().next()
}

#[derive(Debug, Clone, Copy)]
enum BackendMode {
    Nccl,
    DeviceLayers,
}

fn build_candidate(
    root: Option<&Path>,
    platform: &PlatformFacts,
    manifest: &model_manifest::RuntimeModelManifest,
    harness: ModelHarness,
    preset: ChatPreset,
    spec: PlanSpec,
    env_map: &BTreeMap<String, String>,
) -> Option<ChatRuntimePlan> {
    if !launch_contract_allows_backend(&manifest.launch_contract, spec.backend) {
        return None;
    }
    if !platform_supports_backend(
        platform,
        spec.backend,
        platform.hardware.gpus.len(),
        &manifest.launch_contract,
    ) {
        return None;
    }
    let placement = model_placement_profile(root, harness.model);
    let runtime = resolve_harness_runtime(harness, &platform.hardware);
    let compaction_policy = compaction_policy_for_preset(preset);
    let quant = spec.quant;
    let backend = spec.backend;
    let fixed_device_layers_override = match backend {
        BackendMode::DeviceLayers => runtime.fixed_device_layers,
        _ => None,
    };
    let gpu_index_sets = if matches!(backend, BackendMode::DeviceLayers) {
        if let Some(visible) = runtime.fixed_cuda_visible_devices {
            let fixed = visible
                .split(',')
                .filter_map(|chunk| chunk.trim().parse::<usize>().ok())
                .collect::<Vec<_>>();
            if !fixed.is_empty() {
                vec![fixed]
            } else {
                candidate_chat_gpu_indices(
                    preset,
                    backend,
                    &platform.hardware.gpus,
                    placement.primary_gpu_index,
                    &manifest.launch_contract,
                )
            }
        } else {
            candidate_chat_gpu_indices(
                preset,
                backend,
                &platform.hardware.gpus,
                placement.primary_gpu_index,
                &manifest.launch_contract,
            )
        }
    } else {
        candidate_chat_gpu_indices(
            preset,
            backend,
            &platform.hardware.gpus,
            placement.primary_gpu_index,
            &manifest.launch_contract,
        )
    };
    let mut candidates = Vec::new();
    for gpu_indices in gpu_index_sets {
        if !platform_supports_backend(
            platform,
            backend,
            gpu_indices.len(),
            &manifest.launch_contract,
        ) {
            continue;
        }
        if let Some(plan) = build_candidate_for_gpu_indices(
            root,
            platform,
            manifest,
            harness,
            preset,
            spec,
            env_map,
            &placement,
            runtime,
            compaction_policy,
            quant,
            backend,
            fixed_device_layers_override,
            &gpu_indices,
        ) {
            candidates.push(plan);
        }
    }
    choose_best_subset_candidate(root, harness.model, preset, &platform.hardware, &candidates)
}

fn build_candidate_for_gpu_indices(
    root: Option<&Path>,
    platform: &PlatformFacts,
    manifest: &model_manifest::RuntimeModelManifest,
    harness: ModelHarness,
    preset: ChatPreset,
    spec: PlanSpec,
    env_map: &BTreeMap<String, String>,
    placement: &model_manifest::ManifestPlacementProfile,
    runtime: ResolvedHarnessRuntime,
    compaction_policy: CompactionPolicy,
    quant: QuantOption,
    backend: BackendMode,
    fixed_device_layers_override: Option<&str>,
    gpu_indices: &[usize],
) -> Option<ChatRuntimePlan> {
    if gpu_indices.is_empty() {
        return None;
    }
    if !platform_supports_backend(
        platform,
        backend,
        gpu_indices.len(),
        &manifest.launch_contract,
    ) {
        return None;
    }
    let hardware = &platform.hardware;
    let manifest_aux_reserves =
        compute_aux_reserves_mb(root, hardware, env_map, preset, &gpu_indices);
    let live_aux_reserves = root
        .and_then(|path| {
            runtime_contract::reserved_gpu_mb_by_role(
                path,
                Some(runtime_contract::BackendRole::Chat),
            )
            .ok()
        })
        .unwrap_or_default();
    let (aux_reserves, reclaimed_live_aux_on_chat_gpus) = effective_aux_reserves_mb(
        hardware,
        &manifest_aux_reserves,
        &live_aux_reserves,
        preset,
        gpu_indices,
    );
    let mut per_gpu_budgets = Vec::new();
    let mut available_capacity_by_gpu = BTreeMap::new();
    for gpu in &hardware.gpus {
        let desktop_reserve = desktop_reserve_mb_for_gpu(placement, hardware, gpu.index);
        let aux_reserve = *aux_reserves.get(&gpu.index).unwrap_or(&0);
        let available_capacity = gpu.total_mb;
        let usable = gpu
            .total_mb
            .saturating_sub(desktop_reserve)
            .saturating_sub(aux_reserve);
        available_capacity_by_gpu.insert(gpu.index, available_capacity);
        per_gpu_budgets.push((gpu.index, usable));
    }

    let selected_budgets = per_gpu_budgets
        .iter()
        .filter(|(index, _)| gpu_indices.contains(index))
        .map(|(_, usable)| *usable)
        .collect::<Vec<_>>();
    if selected_budgets.is_empty() {
        return None;
    }
    let pa_cache_type =
        engine::resolve_model_pa_cache_type(harness.model, harness.runtime.pa_cache_type, env_map);
    let kv_budget_fraction_milli = parse_fraction_milli(harness.runtime.pa_memory_fraction);
    let immediate_isq = quant.runtime_isq.is_some();
    let nccl_worker_world_size = matches!(backend, BackendMode::Nccl)
        .then(|| nccl_worker_world_size_for_contract(gpu_indices.len(), &manifest.launch_contract))
        .flatten();
    let anchored_subset_nccl = matches!(backend, BackendMode::Nccl)
        && manifest.launch_contract.require_primary_gpu_anchor
        && nccl_worker_world_size.is_some_and(|world_size| world_size < gpu_indices.len());
    let anchor_visible_ordinal = if anchored_subset_nccl {
        Some(nccl_worker_world_size.expect("anchored subset nccl requires worker world size"))
    } else {
        None
    };
    let backend_gpu_count = match backend {
        BackendMode::Nccl => nccl_worker_world_size.unwrap_or(gpu_indices.len()),
        BackendMode::DeviceLayers => gpu_indices.len(),
    };

    let non_repeating_weight_mb = empirical_non_repeating_weight_mb(harness, quant);
    let repeating_weight_mb = empirical_repeating_weight_mb(harness, quant);
    let weight_mb = non_repeating_weight_mb.saturating_add(repeating_weight_mb);
    let fixed_overhead_total_mb = fixed_overhead_mb(backend, backend_gpu_count)
        .saturating_add(measured_backend_runtime_overhead_mb(
            manifest, quant, backend,
        ))
        .saturating_add(measured_load_peak_overhead_mb(
            manifest,
            quant,
            backend,
            immediate_isq,
        ));
    let effective_total_budget_mb = match backend {
        BackendMode::Nccl => {
            if manifest.launch_contract.require_primary_gpu_anchor {
                let anchor_budget = per_gpu_budgets
                    .iter()
                    .find(|(index, _)| *index == placement.primary_gpu_index)
                    .map(|(_, usable)| *usable)
                    .unwrap_or(0);
                let worker_budgets = per_gpu_budgets
                    .iter()
                    .filter(|(index, _)| {
                        gpu_indices.contains(index) && *index != placement.primary_gpu_index
                    })
                    .map(|(_, usable)| *usable)
                    .collect::<Vec<_>>();
                let min_worker_budget = worker_budgets.iter().copied().min().unwrap_or(0);
                anchor_budget.saturating_add(
                    min_worker_budget.saturating_mul(nccl_worker_world_size.unwrap_or(0) as u64),
                )
            } else {
                let min_budget = selected_budgets.iter().copied().min().unwrap_or(0);
                min_budget.saturating_mul(gpu_indices.len() as u64)
            }
        }
        BackendMode::DeviceLayers => selected_budgets.iter().copied().sum(),
    };
    let kv_budget_cap_mb = effective_total_budget_mb
        .saturating_sub(weight_mb)
        .saturating_sub(fixed_overhead_total_mb);
    let safety_headroom_mb = spec
        .per_gpu_headroom_mb
        .saturating_mul(gpu_indices.len() as u64);
    let kv_budget_cap_mb = kv_budget_cap_mb.saturating_sub(safety_headroom_mb);
    let kv_budget_cap_mb = scale_mb(kv_budget_cap_mb, kv_budget_fraction_milli);
    if kv_budget_cap_mb == 0 {
        return None;
    }

    let effective_concurrency = spec.max_seqs.max(spec.max_batch_size).max(1) as u64;
    let kv_mb_per_1k =
        measured_kv_cache_mb_per_1k_tokens(manifest, pa_cache_type.as_deref(), quant)
            .saturating_mul(effective_concurrency);
    let raw_context =
        (((kv_budget_cap_mb as f64) / (kv_mb_per_1k.max(1) as f64)) * 1024.0).floor() as u32;
    let mut plan_context = align_context(raw_context.min(harness.sizing.context_cap));
    plan_context =
        align_context(((plan_context as u64 * spec.context_fraction_milli as u64) / 1000) as u32);
    if let Some(target) = spec.context_target {
        plan_context = plan_context.min(align_context(target));
    }
    let required_context = required_context_floor_for_manifest(manifest);
    let policy_floor_ok = plan_context >= spec.min_context_required.max(required_context);
    let policy_floor_ok = policy_floor_ok && plan_context >= required_context;
    if !policy_floor_ok {
        return None;
    }
    let kv_budget_mb = (((plan_context as u64) * kv_mb_per_1k) + 1023) / 1024;
    if kv_budget_mb > kv_budget_cap_mb {
        return None;
    }
    let activation_overhead_mb =
        measured_activation_overhead_mb(manifest, quant, plan_context, spec.max_seqs);
    let backend_runtime_overhead_mb =
        measured_backend_runtime_overhead_mb(manifest, quant, backend);
    let load_peak_overhead_mb =
        measured_load_peak_overhead_mb(manifest, quant, backend, immediate_isq);
    let fixed_runtime_base_overhead_mb = fixed_overhead_mb(backend, backend_gpu_count);
    let required_kv_residual_mb = (((kv_budget_mb as u128) * 1000u128)
        + (kv_budget_fraction_milli.max(1) as u128 - 1))
        / kv_budget_fraction_milli.max(1) as u128;
    let required_effective_total_budget_mb = weight_mb
        .saturating_add(fixed_runtime_base_overhead_mb)
        .saturating_add(backend_runtime_overhead_mb)
        .saturating_add(load_peak_overhead_mb)
        .saturating_add(safety_headroom_mb)
        .saturating_add(required_kv_residual_mb as u64);
    let theoretical_breakdown = TheoreticalResourceBreakdown {
        contract_source: resource_contract_source(manifest),
        effective_total_budget_mb,
        kv_budget_cap_mb,
        kv_budget_fraction_milli,
        weight_residency_mb: weight_mb,
        kv_cache_mb: kv_budget_mb,
        fixed_runtime_base_overhead_mb,
        backend_runtime_overhead_mb,
        activation_overhead_mb,
        load_peak_overhead_mb,
        safety_headroom_mb,
        required_effective_total_budget_mb,
        required_total_mb: required_effective_total_budget_mb
            .saturating_add(activation_overhead_mb),
    };

    let expected_tok_s = estimate_tok_s(
        harness,
        quant,
        backend,
        backend_gpu_count,
        plan_context,
        spec.max_batch_size,
        spec.max_seqs,
        hardware,
        &gpu_indices,
    );
    let allocations = distribute_allocations(
        backend,
        hardware,
        &gpu_indices,
        &aux_reserves,
        harness,
        placement,
        &runtime,
        fixed_device_layers_override,
        non_repeating_weight_mb,
        repeating_weight_mb,
        kv_budget_mb,
        backend_runtime_overhead_mb,
        activation_overhead_mb,
        load_peak_overhead_mb,
        nccl_worker_world_size,
    );
    let allocations = allocations
        .into_iter()
        .map(|mut allocation| {
            let available_capacity_mb = available_capacity_by_gpu
                .get(&allocation.gpu_index)
                .copied()
                .unwrap_or(allocation.total_mb);
            let required_free_mb = allocation_required_free_mb(&allocation);
            allocation.free_headroom_mb = available_capacity_mb.saturating_sub(required_free_mb);
            allocation
        })
        .collect::<Vec<_>>();
    let overcommitted = allocations.iter().any(|allocation| {
        let required_free_mb = allocation_required_free_mb(allocation);
        let available_capacity_mb = available_capacity_by_gpu
            .get(&allocation.gpu_index)
            .copied()
            .unwrap_or(allocation.total_mb);
        allocation.chat_enabled && required_free_mb > available_capacity_mb
    });
    if overcommitted {
        return None;
    }
    let visible_device_order = if anchored_subset_nccl {
        let mut workers = gpu_indices
            .iter()
            .copied()
            .filter(|gpu| *gpu != placement.primary_gpu_index)
            .collect::<Vec<_>>();
        workers.sort_unstable();
        workers.push(placement.primary_gpu_index);
        workers
    } else {
        gpu_indices.to_vec()
    };
    let cuda_visible_devices = visible_device_order
        .iter()
        .map(|gpu| gpu.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let device_layers = match backend {
        BackendMode::Nccl => None,
        BackendMode::DeviceLayers => Some(device_layers_cli(
            &allocations,
            harness.sizing.repeating_layers,
        )),
    };
    let topology = runtime
        .topology_rel_path
        .map(|rel| Path::new(rel).display().to_string());
    let disable_nccl = !matches!(backend, BackendMode::Nccl);
    let disable_flash_attn = harness.runtime.disable_flash_attn || !platform.flash_attn_available;
    let tensor_parallel_backend = (!disable_nccl).then(|| "nccl".to_string());
    let mn_local_world_size = (!disable_nccl).then(|| backend_gpu_count as u32);
    let nm_device_ordinal = if matches!(backend, BackendMode::Nccl) {
        anchor_visible_ordinal
            .map(|ordinal| ordinal as u32)
            .or(runtime.nm_device_ordinal)
    } else {
        runtime.nm_device_ordinal
    };
    let base_device_ordinal = if matches!(backend, BackendMode::Nccl) {
        anchor_visible_ordinal
            .map(|ordinal| ordinal as u32)
            .or(runtime.base_device_ordinal)
    } else {
        runtime.base_device_ordinal
    };
    let mut rationale = vec![
        format!(
            "platform cuda={} nccl={} flash_attn={}",
            platform.cuda_available, platform.nccl_available, platform.flash_attn_available
        ),
        format!("preset {}", preset.label()),
        format!("quant {}", quant.label),
        format!("context {}", plan_context),
        format!("max_seqs {}", spec.max_seqs),
        format!("max_batch_size {}", spec.max_batch_size),
        match backend {
            BackendMode::Nccl => {
                if manifest.launch_contract.require_primary_gpu_anchor {
                    format!(
                        "backend nccl workers={} anchor_gpu={}",
                        backend_gpu_count, placement.primary_gpu_index
                    )
                } else {
                    format!("backend nccl x{}", backend_gpu_count)
                }
            }
            BackendMode::DeviceLayers => format!("backend device-layers x{}", gpu_indices.len()),
        },
        format!(
            "resource contract {}",
            theoretical_breakdown.contract_source
        ),
    ];
    rationale.extend(platform.validation_messages.iter().cloned());
    if reclaimed_live_aux_on_chat_gpus {
        rationale.push(
            "performance preset reclaims live auxiliary GPU leases on target chat GPUs".to_string(),
        );
    }
    if spec.per_gpu_headroom_mb > 0 {
        rationale.push(format!(
            "reserved {}MB per GPU for load/runtime headroom",
            spec.per_gpu_headroom_mb
        ));
    }
    if disable_flash_attn {
        rationale.push("flash-attn disabled for this model/runtime path".to_string());
    }
    if runtime.force_no_mmap {
        rationale.push("mmap disabled for this model/runtime path".to_string());
    }
    if harness.runtime.force_language_model_only {
        rationale.push("vision tower disabled for text-only runtime path".to_string());
    }
    if harness.runtime.require_prebuilt_uqff_for_chat_start && quant.runtime_isq.is_some() {
        rationale.push("startup requires a reusable UQFF artifact before chat launch".to_string());
    }
    if runtime.isq_singlethread && quant.runtime_isq.is_some() {
        rationale.push("ISQ is serialized to avoid model-load VRAM spikes".to_string());
    }
    if let Some(cpu_threads) = runtime
        .isq_cpu_threads
        .filter(|_| quant.runtime_isq.is_some())
    {
        rationale.push(format!("ISQ cpu threads {cpu_threads}"));
    }
    if let Some(backend_override) = runtime.moe_experts_backend {
        rationale.push(format!("moe experts backend {}", backend_override));
    }
    let paged_attn = engine::resolve_model_paged_attn(
        harness.model,
        harness.runtime.paged_attn,
        pa_cache_type.as_deref(),
    );
    let plan = ChatRuntimePlan {
        model: harness.model.to_string(),
        preset,
        quantization: quant.label.to_string(),
        runtime_isq: quant.runtime_isq.map(str::to_string),
        max_seq_len: plan_context,
        compaction_threshold_percent: compaction_policy.threshold_percent,
        compaction_min_tokens: compaction_policy.min_tokens,
        min_context_floor_applied: true,
        paged_attn,
        pa_cache_type,
        pa_memory_fraction: harness.runtime.pa_memory_fraction.map(str::to_string),
        pa_context_len: harness.runtime.pa_context_len,
        disable_nccl,
        tensor_parallel_backend,
        mn_local_world_size,
        max_batch_size: spec.max_batch_size,
        max_seqs: spec.max_seqs,
        cuda_visible_devices,
        device_layers,
        topology,
        allow_device_layers_with_topology: runtime.allow_device_layers_with_topology,
        nm_device_ordinal,
        base_device_ordinal,
        moe_experts_backend: runtime.moe_experts_backend.map(str::to_string),
        disable_flash_attn,
        force_no_mmap: runtime.force_no_mmap,
        force_language_model_only: harness.runtime.force_language_model_only,
        require_prebuilt_uqff_for_chat_start: harness.runtime.require_prebuilt_uqff_for_chat_start
            && quant.runtime_isq.is_some(),
        isq_singlethread: runtime.isq_singlethread && quant.runtime_isq.is_some(),
        isq_cpu_threads: runtime
            .isq_cpu_threads
            .filter(|_| quant.runtime_isq.is_some()),
        expected_tok_s,
        hardware_fingerprint: hardware.fingerprint.clone(),
        theoretical_breakdown,
        rationale,
        gpu_allocations: allocations,
    };
    Some(plan)
}

fn build_floor_fallback_plan(
    root: Option<&Path>,
    platform: &PlatformFacts,
    manifest: &model_manifest::RuntimeModelManifest,
    harness: ModelHarness,
    preset: ChatPreset,
    fallback_spec: PlanSpec,
    env_map: &BTreeMap<String, String>,
) -> ChatRuntimePlan {
    let hardware = &platform.hardware;
    let dynamic_fallback_spec = PlanSpec {
        backend: match fallback_spec.backend {
            BackendMode::Nccl => BackendMode::DeviceLayers,
            BackendMode::DeviceLayers => BackendMode::DeviceLayers,
        },
        max_batch_size: 1,
        max_seqs: 1,
        ..fallback_spec
    };
    if launch_contract_allows_backend(&manifest.launch_contract, dynamic_fallback_spec.backend) {
        if let Some(mut plan) = build_candidate(
            root,
            platform,
            manifest,
            harness,
            preset,
            dynamic_fallback_spec,
            env_map,
        ) {
            plan.rationale
                .push("dynamic backend fallback from unavailable preferred path".to_string());
            return plan;
        }
    }
    if !launch_contract_allows_backend(&manifest.launch_contract, fallback_spec.backend) {
        return ChatRuntimePlan {
            model: harness.model.to_string(),
            preset,
            quantization: fallback_spec.quant.label.to_string(),
            runtime_isq: fallback_spec.quant.runtime_isq.map(str::to_string),
            max_seq_len: required_context_floor_for_manifest(manifest),
            compaction_threshold_percent: compaction_policy_for_preset(preset).threshold_percent,
            compaction_min_tokens: compaction_policy_for_preset(preset).min_tokens,
            min_context_floor_applied: false,
            paged_attn: harness.runtime.paged_attn.to_string(),
            pa_cache_type: harness.runtime.pa_cache_type.map(str::to_string),
            pa_memory_fraction: harness.runtime.pa_memory_fraction.map(str::to_string),
            pa_context_len: harness.runtime.pa_context_len,
            disable_nccl: true,
            tensor_parallel_backend: None,
            mn_local_world_size: None,
            max_batch_size: 1,
            max_seqs: 1,
            cuda_visible_devices: hardware
                .gpus
                .iter()
                .map(|gpu| gpu.index.to_string())
                .collect::<Vec<_>>()
                .join(","),
            device_layers: None,
            topology: None,
            allow_device_layers_with_topology: false,
            nm_device_ordinal: Some(0),
            base_device_ordinal: Some(0),
            moe_experts_backend: None,
            disable_flash_attn: harness.runtime.disable_flash_attn
                || !platform.flash_attn_available,
            force_no_mmap: harness.runtime.force_no_mmap,
            force_language_model_only: harness.runtime.force_language_model_only,
            require_prebuilt_uqff_for_chat_start: harness
                .runtime
                .require_prebuilt_uqff_for_chat_start,
            isq_singlethread: harness.runtime.isq_singlethread,
            isq_cpu_threads: harness.runtime.isq_cpu_threads,
            expected_tok_s: 0.0,
            hardware_fingerprint: hardware.fingerprint.clone(),
            theoretical_breakdown: TheoreticalResourceBreakdown {
                contract_source: resource_contract_source(manifest),
                effective_total_budget_mb: 0,
                kv_budget_cap_mb: 0,
                kv_budget_fraction_milli: 0,
                weight_residency_mb: 0,
                kv_cache_mb: 0,
                fixed_runtime_base_overhead_mb: 0,
                backend_runtime_overhead_mb: 0,
                activation_overhead_mb: 0,
                load_peak_overhead_mb: 0,
                safety_headroom_mb: 0,
                required_effective_total_budget_mb: 0,
                required_total_mb: 0,
            },
            rationale: {
                let mut rationale = vec![
                    format!(
                        "platform cuda={} nccl={} flash_attn={}",
                        platform.cuda_available,
                        platform.nccl_available,
                        platform.flash_attn_available
                    ),
                    format!("preset {}", preset.label()),
                    "policy floor fallback".to_string(),
                    "launch contract disallowed the preferred backend; no qualifying 128k runtime contract could be proven".to_string(),
                ];
                rationale.extend(platform.validation_messages.iter().cloned());
                rationale
            },
            gpu_allocations: hardware
                .gpus
                .iter()
                .map(|gpu| PlannedGpuAllocation {
                    gpu_index: gpu.index,
                    name: gpu.name.clone(),
                    total_mb: gpu.total_mb,
                    desktop_reserve_mb: desktop_reserve_mb_for_gpu(
                        &model_placement_profile(root, harness.model),
                        hardware,
                        gpu.index,
                    ),
                    aux_reserve_mb: 0,
                    chat_budget_mb: 0,
                    backend_overhead_mb: 0,
                    activation_overhead_mb: 0,
                    load_peak_overhead_mb: 0,
                    repeating_weight_mb: 0,
                    weight_mb: 0,
                    kv_cache_mb: 0,
                    free_headroom_mb: gpu.total_mb,
                    chat_enabled: gpu.index == 0,
                })
                .collect(),
        };
    }
    let runtime = resolve_harness_runtime(harness, hardware);
    let placement = model_placement_profile(root, harness.model);
    let plan = build_candidate(
        root,
        platform,
        manifest,
        harness,
        preset,
        fallback_spec,
        env_map,
    )
    .unwrap_or_else(|| {
        let compaction_policy = compaction_policy_for_preset(preset);
        let pa_cache_type = engine::resolve_model_pa_cache_type(
            harness.model,
            harness.runtime.pa_cache_type,
            env_map,
        );
        let paged_attn = engine::resolve_model_paged_attn(
            harness.model,
            harness.runtime.paged_attn,
            pa_cache_type.as_deref(),
        );
        let fallback_gpu_allocations = hardware
            .gpus
            .iter()
            .map(|gpu| PlannedGpuAllocation {
                gpu_index: gpu.index,
                name: gpu.name.clone(),
                total_mb: gpu.total_mb,
                desktop_reserve_mb: desktop_reserve_mb_for_gpu(&placement, hardware, gpu.index),
                aux_reserve_mb: 0,
                chat_budget_mb: 0,
                backend_overhead_mb: 0,
                activation_overhead_mb: 0,
                load_peak_overhead_mb: 0,
                repeating_weight_mb: 0,
                weight_mb: 0,
                kv_cache_mb: 0,
                free_headroom_mb: gpu.total_mb,
                chat_enabled: true,
            })
            .collect::<Vec<_>>();
        let fallback_device_layers = if matches!(fallback_spec.backend, BackendMode::DeviceLayers)
            && !fallback_gpu_allocations.is_empty()
        {
            let capacity_weights = fallback_gpu_allocations
                .iter()
                .map(|allocation| {
                    allocation
                        .total_mb
                        .saturating_sub(allocation.desktop_reserve_mb)
                })
                .collect::<Vec<_>>();
            let repeating_weight_shares =
                proportional_shares(harness.sizing.repeating_layers as u64, &capacity_weights);
            let layer_allocations = fallback_gpu_allocations
                .iter()
                .zip(repeating_weight_shares.iter())
                .map(|(allocation, share)| PlannedGpuAllocation {
                    repeating_weight_mb: *share,
                    ..allocation.clone()
                })
                .collect::<Vec<_>>();
            Some(device_layers_cli(
                &layer_allocations,
                harness.sizing.repeating_layers,
            ))
        } else {
            None
        };
        ChatRuntimePlan {
            model: harness.model.to_string(),
            preset,
            quantization: fallback_spec.quant.label.to_string(),
            runtime_isq: fallback_spec.quant.runtime_isq.map(str::to_string),
            max_seq_len: required_context_floor_for_manifest(manifest),
            compaction_threshold_percent: compaction_policy.threshold_percent,
            compaction_min_tokens: compaction_policy.min_tokens,
            min_context_floor_applied: false,
            paged_attn,
            pa_cache_type,
            pa_memory_fraction: harness.runtime.pa_memory_fraction.map(str::to_string),
            pa_context_len: harness.runtime.pa_context_len,
            disable_nccl: true,
            tensor_parallel_backend: None,
            mn_local_world_size: None,
            max_batch_size: fallback_spec.max_batch_size,
            max_seqs: fallback_spec.max_seqs,
            cuda_visible_devices: hardware
                .gpus
                .iter()
                .map(|gpu| gpu.index.to_string())
                .collect::<Vec<_>>()
                .join(","),
            device_layers: fallback_device_layers,
            topology: runtime
                .topology_rel_path
                .map(|rel| Path::new(rel).display().to_string()),
            allow_device_layers_with_topology: runtime.allow_device_layers_with_topology,
            nm_device_ordinal: runtime.nm_device_ordinal,
            base_device_ordinal: runtime.base_device_ordinal,
            moe_experts_backend: runtime.moe_experts_backend.map(str::to_string),
            disable_flash_attn: harness.runtime.disable_flash_attn
                || !platform.flash_attn_available,
            force_no_mmap: runtime.force_no_mmap,
            force_language_model_only: harness.runtime.force_language_model_only,
            require_prebuilt_uqff_for_chat_start: harness
                .runtime
                .require_prebuilt_uqff_for_chat_start
                && fallback_spec.quant.runtime_isq.is_some(),
            isq_singlethread: runtime.isq_singlethread && fallback_spec.quant.runtime_isq.is_some(),
            isq_cpu_threads: runtime
                .isq_cpu_threads
                .filter(|_| fallback_spec.quant.runtime_isq.is_some()),
            expected_tok_s: harness.sizing.base_toks_per_sec_q4 * 0.65,
            hardware_fingerprint: hardware.fingerprint.clone(),
            theoretical_breakdown: TheoreticalResourceBreakdown {
                contract_source: resource_contract_source(manifest),
                effective_total_budget_mb: 0,
                kv_budget_cap_mb: 0,
                kv_budget_fraction_milli: 0,
                weight_residency_mb: 0,
                kv_cache_mb: 0,
                fixed_runtime_base_overhead_mb: 0,
                backend_runtime_overhead_mb: 0,
                activation_overhead_mb: 0,
                load_peak_overhead_mb: 0,
                safety_headroom_mb: 0,
                required_effective_total_budget_mb: 0,
                required_total_mb: 0,
            },
            rationale: {
                let mut rationale = vec![
                    format!(
                        "platform cuda={} nccl={} flash_attn={}",
                        platform.cuda_available,
                        platform.nccl_available,
                        platform.flash_attn_available
                    ),
                    "policy floor fallback".to_string(),
                    "preserved runtime topology despite missing qualifying 128k contract"
                        .to_string(),
                ];
                rationale.extend(platform.validation_messages.iter().cloned());
                rationale
            },
            gpu_allocations: fallback_gpu_allocations,
        }
    });
    let mut fallback = plan;
    if !platform_supports_backend(
        platform,
        fallback_spec.backend,
        hardware.gpus.len(),
        &manifest.launch_contract,
    ) {
        fallback.rationale.push(format!(
            "preferred backend {} unavailable on this platform contract",
            match fallback_spec.backend {
                BackendMode::Nccl => "nccl",
                BackendMode::DeviceLayers => "device_layers",
            }
        ));
    }
    fallback.rationale.push(format!(
        "hardware could not satisfy the full preset policy; kept {} and the 128k floor",
        fallback_spec.quant.label
    ));
    fallback
}

fn largest_power_of_two_leq(value: usize) -> usize {
    if value == 0 {
        return 0;
    }
    1usize << ((usize::BITS - 1) - value.leading_zeros())
}

#[allow(dead_code)]
fn nccl_worker_world_size(gpu_count: usize, require_primary_gpu_anchor: bool) -> Option<usize> {
    nccl_worker_world_size_for_contract(
        gpu_count,
        &model_manifest::ManifestLaunchContract {
            require_primary_gpu_anchor,
            ..model_manifest::ManifestLaunchContract::default()
        },
    )
}

fn nccl_worker_world_size_for_contract(
    gpu_count: usize,
    contract: &model_manifest::ManifestLaunchContract,
) -> Option<usize> {
    if contract.require_primary_gpu_anchor {
        if gpu_count >= 4 && gpu_count.is_power_of_two() {
            Some(gpu_count)
        } else if contract.allow_subset_anchored_nccl {
            let workers = largest_power_of_two_leq(gpu_count.saturating_sub(1));
            (workers >= 2).then_some(workers)
        } else {
            None
        }
    } else {
        (gpu_count > 1 && gpu_count.is_power_of_two()).then_some(gpu_count)
    }
}

fn nccl_supported_gpu_count(
    gpu_count: usize,
    contract: &model_manifest::ManifestLaunchContract,
) -> bool {
    nccl_worker_world_size_for_contract(gpu_count, contract).is_some()
}

fn resolve_harness_runtime(
    harness: ModelHarness,
    hardware: &HardwareProfile,
) -> ResolvedHarnessRuntime {
    let fixed_cuda_visible_devices = match harness.model {
        "google/gemma-4-E2B-it" | "google/gemma-4-E4B-it"
            if hardware.gpus.iter().any(|gpu| gpu.index == 0) =>
        {
            Some("0")
        }
        _ => None,
    };
    ResolvedHarnessRuntime {
        fixed_device_layers: None,
        fixed_cuda_visible_devices,
        topology_rel_path: None,
        allow_device_layers_with_topology: false,
        // Keep non-repeating tensors and the base runtime device on the first
        // visible GPU. The planner already sorts visible devices so that the
        // manifest primary GPU comes first.
        nm_device_ordinal: Some(0),
        base_device_ordinal: Some(0),
        moe_experts_backend: harness.runtime.moe_experts_backend,
        force_no_mmap: harness.runtime.force_no_mmap,
        isq_singlethread: harness.runtime.isq_singlethread,
        isq_cpu_threads: harness.runtime.isq_cpu_threads,
    }
}

fn distribute_allocations(
    backend: BackendMode,
    hardware: &HardwareProfile,
    gpu_indices: &[usize],
    aux_reserves: &BTreeMap<usize, u64>,
    harness: ModelHarness,
    placement: &model_manifest::ManifestPlacementProfile,
    resolved_runtime: &ResolvedHarnessRuntime,
    fixed_device_layers: Option<&str>,
    non_repeating_weight_mb: u64,
    repeating_weight_mb: u64,
    kv_budget_mb: u64,
    backend_runtime_overhead_mb: u64,
    activation_overhead_mb: u64,
    load_peak_overhead_mb: u64,
    nccl_worker_world_size: Option<usize>,
) -> Vec<PlannedGpuAllocation> {
    let selected = hardware
        .gpus
        .iter()
        .filter(|gpu| gpu_indices.contains(&gpu.index))
        .collect::<Vec<_>>();
    let fixed_layer_weights = fixed_device_layer_weights(
        fixed_device_layers,
        gpu_indices,
        harness.sizing.repeating_layers,
    );
    let base_weight_gpu = match backend {
        BackendMode::Nccl => placement
            .primary_gpu_holds_non_repeating
            .then_some(placement.primary_gpu_index),
        BackendMode::DeviceLayers => resolved_runtime
            .base_device_ordinal
            .and_then(|gpu| {
                gpu_indices
                    .contains(&(gpu as usize))
                    .then_some(gpu as usize)
            })
            .or_else(|| {
                gpu_indices
                    .contains(&placement.primary_gpu_index)
                    .then_some(placement.primary_gpu_index)
            })
            .or_else(|| gpu_indices.first().copied()),
    };
    let device_layer_capacity_weights = selected
        .iter()
        .map(|gpu| {
            let capacity = gpu
                .total_mb
                .saturating_sub(desktop_reserve_mb_for_gpu(placement, hardware, gpu.index))
                .saturating_sub(*aux_reserves.get(&gpu.index).unwrap_or(&0));
            if matches!(backend, BackendMode::DeviceLayers) && base_weight_gpu == Some(gpu.index) {
                capacity.saturating_sub(non_repeating_weight_mb)
            } else {
                capacity
            }
        })
        .collect::<Vec<_>>();
    let weight_shares = match backend {
        BackendMode::Nccl => {
            if placement.primary_gpu_holds_non_repeating {
                let worker_count = nccl_worker_world_size.unwrap_or(selected.len());
                even_shares(repeating_weight_mb, worker_count)
            } else {
                even_shares(
                    non_repeating_weight_mb.saturating_add(repeating_weight_mb),
                    selected.len(),
                )
            }
        }
        BackendMode::DeviceLayers => match fixed_layer_weights.as_ref() {
            Some(weights) => proportional_shares(repeating_weight_mb, weights),
            None => proportional_shares(repeating_weight_mb, &device_layer_capacity_weights),
        },
    };
    let kv_shares = match backend {
        BackendMode::Nccl => {
            let worker_count = nccl_worker_world_size.unwrap_or(selected.len());
            even_shares(kv_budget_mb, worker_count)
        }
        BackendMode::DeviceLayers => match fixed_layer_weights.as_ref() {
            Some(weights) => proportional_shares(kv_budget_mb, weights),
            None => proportional_shares(kv_budget_mb, &device_layer_capacity_weights),
        },
    };
    let backend_overhead_shares = if selected.is_empty() {
        Vec::new()
    } else {
        let count =
            if matches!(backend, BackendMode::Nccl) && placement.primary_gpu_holds_non_repeating {
                nccl_worker_world_size.unwrap_or(selected.len())
            } else {
                selected.len()
            };
        even_shares(backend_runtime_overhead_mb, count)
    };

    let mut selected_index = 0usize;
    hardware
        .gpus
        .iter()
        .map(|gpu| {
            let desktop_reserve = desktop_reserve_mb_for_gpu(placement, hardware, gpu.index);
            let aux_reserve = *aux_reserves.get(&gpu.index).unwrap_or(&0);
            let chat_enabled = gpu_indices.contains(&gpu.index);
            let (repeating_weight_share, mut weight_share, kv_share, backend_overhead_share) =
                if chat_enabled {
                    if matches!(backend, BackendMode::Nccl)
                        && placement.primary_gpu_holds_non_repeating
                    {
                        if gpu.index == placement.primary_gpu_index {
                            (0, 0, 0, 0)
                        } else {
                            let share = (
                                *weight_shares.get(selected_index).unwrap_or(&0),
                                *weight_shares.get(selected_index).unwrap_or(&0),
                                *kv_shares.get(selected_index).unwrap_or(&0),
                                *backend_overhead_shares.get(selected_index).unwrap_or(&0),
                            );
                            selected_index += 1;
                            share
                        }
                    } else {
                        let share = (
                            *weight_shares.get(selected_index).unwrap_or(&0),
                            *weight_shares.get(selected_index).unwrap_or(&0),
                            *kv_shares.get(selected_index).unwrap_or(&0),
                            *backend_overhead_shares.get(selected_index).unwrap_or(&0),
                        );
                        selected_index += 1;
                        share
                    }
                } else {
                    (0, 0, 0, 0)
                };
            if chat_enabled && base_weight_gpu == Some(gpu.index) {
                weight_share = weight_share.saturating_add(non_repeating_weight_mb);
            }
            let activation_overhead_share =
                if chat_enabled && gpu.index == placement.primary_gpu_index {
                    activation_overhead_mb
                } else {
                    0
                };
            let load_peak_overhead_share =
                if chat_enabled && gpu.index == placement.primary_gpu_index {
                    load_peak_overhead_mb
                } else {
                    0
                };
            let chat_budget_mb = weight_share.saturating_add(kv_share);
            let consumed = desktop_reserve
                .saturating_add(aux_reserve)
                .saturating_add(chat_budget_mb)
                .saturating_add(backend_overhead_share)
                .saturating_add(activation_overhead_share)
                .saturating_add(load_peak_overhead_share);
            PlannedGpuAllocation {
                gpu_index: gpu.index,
                name: gpu.name.clone(),
                total_mb: gpu.total_mb,
                desktop_reserve_mb: desktop_reserve,
                aux_reserve_mb: aux_reserve,
                chat_budget_mb,
                backend_overhead_mb: backend_overhead_share,
                activation_overhead_mb: activation_overhead_share,
                load_peak_overhead_mb: load_peak_overhead_share,
                repeating_weight_mb: repeating_weight_share,
                weight_mb: weight_share,
                kv_cache_mb: kv_share,
                free_headroom_mb: gpu.total_mb.saturating_sub(consumed),
                chat_enabled,
            }
        })
        .collect::<Vec<_>>()
}

fn fixed_device_layer_weights(
    fixed_device_layers: Option<&str>,
    gpu_indices: &[usize],
    total_layers: u32,
) -> Option<Vec<u64>> {
    let map = fixed_device_layers?;
    let mut counts = BTreeMap::new();
    for chunk in map.split(';') {
        let (gpu, layers) = chunk.split_once(':')?;
        let gpu_index = gpu.trim().parse::<usize>().ok()?;
        let layer_count = layers.trim().parse::<u64>().ok()?;
        counts.insert(gpu_index, layer_count);
    }
    let selected = gpu_indices
        .iter()
        .map(|gpu_index| counts.get(gpu_index).copied())
        .collect::<Option<Vec<_>>>()?;
    let sum = selected.iter().copied().sum::<u64>();
    if sum != total_layers as u64 {
        return None;
    }
    Some(selected)
}

fn device_layers_cli(allocations: &[PlannedGpuAllocation], total_layers: u32) -> String {
    let selected = allocations
        .iter()
        .filter(|allocation| allocation.chat_enabled && allocation.repeating_weight_mb > 0)
        .collect::<Vec<_>>();
    let total_budget = selected
        .iter()
        .map(|allocation| allocation.repeating_weight_mb)
        .sum::<u64>()
        .max(1);
    let total_layers = total_layers as u64;
    let mut raw = selected
        .iter()
        .map(|allocation| {
            (
                allocation.gpu_index,
                ((allocation.repeating_weight_mb as f64 / total_budget as f64)
                    * total_layers as f64)
                    .round() as u64,
            )
        })
        .collect::<Vec<_>>();
    let sum_layers = raw.iter().map(|(_, layers)| *layers).sum::<u64>();
    if sum_layers != total_layers {
        let delta = total_layers as i64 - sum_layers as i64;
        if let Some(first) = raw.first_mut() {
            first.1 = ((first.1 as i64) + delta).max(1) as u64;
        }
    }
    raw.into_iter()
        .map(|(gpu, layers)| format!("{gpu}:{layers}"))
        .collect::<Vec<_>>()
        .join(";")
}

fn compute_aux_reserves_mb(
    root: Option<&Path>,
    hardware: &HardwareProfile,
    env_map: &BTreeMap<String, String>,
    preset: ChatPreset,
    chat_gpu_indices: &[usize],
) -> BTreeMap<usize, u64> {
    if config_flag_from_env_map(env_map, "CTOX_DISABLE_AUXILIARY_BACKENDS") {
        return BTreeMap::new();
    }
    let mut reserves: BTreeMap<usize, u64> = BTreeMap::new();
    for plan in planned_auxiliary_runtime_plans(root, hardware, env_map, preset, chat_gpu_indices) {
        if plan.compute_target == engine::ComputeTarget::Cpu || plan.gpu_reserve_mb == 0 {
            continue;
        }
        let target_devices = parse_csv_indices(plan.visible_devices.as_ref());
        if target_devices.is_empty() {
            continue;
        }
        let shares = even_shares(plan.gpu_reserve_mb, target_devices.len());
        for (idx, gpu_index) in target_devices.iter().enumerate() {
            let entry = reserves.entry(*gpu_index).or_insert(0);
            *entry = (*entry).saturating_add(*shares.get(idx).unwrap_or(&0u64));
        }
    }
    reserves
}

fn effective_aux_reserves_mb(
    hardware: &HardwareProfile,
    manifest_aux_reserves: &BTreeMap<usize, u64>,
    live_aux_reserves: &BTreeMap<usize, u64>,
    preset: ChatPreset,
    chat_gpu_indices: &[usize],
) -> (BTreeMap<usize, u64>, bool) {
    let mut reclaimed_live_aux_on_chat_gpus = false;
    let reserves = hardware
        .gpus
        .iter()
        .filter_map(|gpu| {
            let manifest_reserve = *manifest_aux_reserves.get(&gpu.index).unwrap_or(&0);
            let live_reserve = *live_aux_reserves.get(&gpu.index).unwrap_or(&0);
            let effective =
                if preset == ChatPreset::Performance && chat_gpu_indices.contains(&gpu.index) {
                    if live_reserve > manifest_reserve {
                        reclaimed_live_aux_on_chat_gpus = true;
                    }
                    manifest_reserve
                } else {
                    manifest_reserve.max(live_reserve)
                };
            (effective > 0).then_some((gpu.index, effective))
        })
        .collect();
    (reserves, reclaimed_live_aux_on_chat_gpus)
}

fn config_flag_from_env_map(env_map: &BTreeMap<String, String>, key: &str) -> bool {
    env_map
        .get(key)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn default_aux_distribution(
    _root: Option<&Path>,
    _selection: &engine::AuxiliaryModelSelection,
    hardware: &HardwareProfile,
    _preset: ChatPreset,
    chat_gpu_indices: &[usize],
) -> Vec<usize> {
    let total_gpu_count = hardware.gpus.len();
    if total_gpu_count == 0 {
        return Vec::new();
    }
    let aux_only = hardware
        .gpus
        .iter()
        .map(|gpu| gpu.index)
        .filter(|index| !chat_gpu_indices.contains(index))
        .collect::<Vec<_>>();
    if !aux_only.is_empty() {
        return aux_only;
    }
    // When chat already occupies every visible GPU, auxiliary GPU backends
    // must yield instead of falling back onto a chat GPU and destabilizing the
    // main 128k runtime.
    Vec::new()
}

fn planned_auxiliary_runtime_plans(
    root: Option<&Path>,
    hardware: &HardwareProfile,
    env_map: &BTreeMap<String, String>,
    preset: ChatPreset,
    chat_gpu_indices: &[usize],
) -> Vec<AuxiliaryRuntimePlan> {
    let mut available_aux_gpus = default_aux_only_gpu_pool(hardware, chat_gpu_indices);
    [
        engine::AuxiliaryRole::Embedding,
        engine::AuxiliaryRole::Stt,
        engine::AuxiliaryRole::Tts,
    ]
    .into_iter()
    .filter_map(|role| {
        planned_auxiliary_runtime_for_role(
            root,
            hardware,
            env_map,
            preset,
            chat_gpu_indices,
            &mut available_aux_gpus,
            role,
        )
    })
    .collect()
}

fn default_aux_only_gpu_pool(hardware: &HardwareProfile, chat_gpu_indices: &[usize]) -> Vec<usize> {
    hardware
        .gpus
        .iter()
        .map(|gpu| gpu.index)
        .filter(|index| !chat_gpu_indices.contains(index))
        .collect()
}

fn planned_auxiliary_runtime_for_role(
    root: Option<&Path>,
    hardware: &HardwareProfile,
    env_map: &BTreeMap<String, String>,
    preset: ChatPreset,
    chat_gpu_indices: &[usize],
    available_aux_gpus: &mut Vec<usize>,
    role: engine::AuxiliaryRole,
) -> Option<AuxiliaryRuntimePlan> {
    let role_prefix = match role {
        engine::AuxiliaryRole::Embedding => "EMBEDDING",
        engine::AuxiliaryRole::Stt => "STT",
        engine::AuxiliaryRole::Tts => "TTS",
        engine::AuxiliaryRole::Vision => "VISION",
    };
    if config_flag_from_env_map(env_map, &format!("CTOX_DISABLE_{role_prefix}_BACKEND")) {
        return None;
    }
    let configured_model = env_map
        .get(&format!("CTOX_{role_prefix}_MODEL"))
        .map(String::as_str);
    let mut selection = engine::auxiliary_model_selection(role, configured_model);
    let mut visible_devices = None;
    if selection.compute_target == engine::ComputeTarget::Gpu {
        let default_distribution =
            default_aux_distribution(root, &selection, hardware, preset, chat_gpu_indices);
        if let Some(gpu_index) = default_distribution
            .into_iter()
            .find(|gpu_index| available_aux_gpus.contains(gpu_index))
        {
            available_aux_gpus.retain(|candidate| *candidate != gpu_index);
            visible_devices = Some(gpu_index.to_string());
        } else if let Some(cpu_fallback) = cpu_fallback_auxiliary_selection(role) {
            selection = cpu_fallback;
        } else {
            return None;
        }
    }
    if !planned_auxiliary_selection_is_supported(&selection) {
        return None;
    }
    let gpu_reserve_mb = if selection.compute_target == engine::ComputeTarget::Gpu {
        auxiliary_manifest(root, selection.request_model)
            .map(|manifest| manifest.gpu_reserve_mb)
            .unwrap_or_else(|| selection.gpu_reserve_mb())
    } else {
        0
    };
    Some(AuxiliaryRuntimePlan {
        role,
        display_model: selection.choice.to_string(),
        request_model: selection.request_model.to_string(),
        backend_kind: selection.backend_kind,
        compute_target: selection.compute_target,
        port: selection.default_port,
        visible_devices,
        gpu_reserve_mb,
    })
}

fn planned_auxiliary_selection_is_supported(selection: &engine::AuxiliaryModelSelection) -> bool {
    match selection.role {
        engine::AuxiliaryRole::Embedding => true,
        engine::AuxiliaryRole::Stt => selection.compute_target == engine::ComputeTarget::Gpu,
        engine::AuxiliaryRole::Tts => selection.compute_target == engine::ComputeTarget::Gpu,
        engine::AuxiliaryRole::Vision => selection.compute_target == engine::ComputeTarget::Gpu,
    }
}

fn cpu_fallback_auxiliary_selection(
    role: engine::AuxiliaryRole,
) -> Option<engine::AuxiliaryModelSelection> {
    let alias = match role {
        engine::AuxiliaryRole::Embedding => "Qwen/Qwen3-Embedding-0.6B [CPU]",
        engine::AuxiliaryRole::Stt | engine::AuxiliaryRole::Tts | engine::AuxiliaryRole::Vision => {
            return None;
        }
    };
    Some(engine::auxiliary_model_selection(role, Some(alias)))
}

fn estimate_tok_s(
    harness: ModelHarness,
    quant: QuantOption,
    backend: BackendMode,
    gpu_count: usize,
    context: u32,
    max_batch_size: u32,
    max_seqs: u32,
    hardware: &HardwareProfile,
    gpu_indices: &[usize],
) -> f64 {
    let mut tps = harness.sizing.base_toks_per_sec_q4 * (quant.speed_factor_milli as f64 / 1000.0);
    match backend {
        BackendMode::Nccl => {
            tps *= 1.0 + ((gpu_count.saturating_sub(1) as f64) * 0.22);
        }
        BackendMode::DeviceLayers => {
            let device_layers_scale = harness
                .runtime
                .small_uniform_device_layers_scale
                .filter(|hint| {
                    !gpu_indices.is_empty()
                        && gpu_indices
                            .iter()
                            .filter_map(|index| {
                                hardware.gpus.iter().find(|gpu| gpu.index == *index)
                            })
                            .all(|gpu| gpu.total_mb <= hint.max_gpu_memory_mb)
                })
                .map(|hint| match gpu_count {
                    0 | 1 => hint.single_gpu_scale,
                    2 => hint.dual_gpu_scale,
                    _ => hint.multi_gpu_scale,
                })
                .unwrap_or_else(|| 1.0 + ((gpu_count.saturating_sub(1) as f64) * 0.11));
            tps *= device_layers_scale;
        }
    }
    let selected_gpu_totals = gpu_indices
        .iter()
        .filter_map(|index| hardware.gpus.iter().find(|gpu| gpu.index == *index))
        .map(|gpu| gpu.total_mb)
        .collect::<Vec<_>>();
    let mixed_penalty = if selected_gpu_totals
        .windows(2)
        .any(|window| window[0] != window[1])
    {
        let min_total = selected_gpu_totals.iter().copied().min().unwrap_or(0);
        let max_total = selected_gpu_totals.iter().copied().max().unwrap_or(0);
        let heterogeneity_penalty = if min_total == 0 || max_total == 0 {
            0.5
        } else {
            min_total as f64 / max_total as f64
        };
        0.92 * heterogeneity_penalty
    } else {
        1.0
    };
    let context_penalty = if context > 65_536 {
        0.90
    } else if context > 32_768 {
        0.95
    } else {
        1.0
    };
    let concurrency_bonus = (1.0 + (max_seqs.saturating_sub(1) as f64 * 0.08))
        * (1.0 + (max_batch_size.saturating_sub(1) as f64 * 0.05));
    tps * mixed_penalty * context_penalty * concurrency_bonus.min(1.35)
}

fn candidate_chat_gpu_indices(
    preset: ChatPreset,
    backend: BackendMode,
    gpus: &[HardwareGpu],
    primary_gpu_index: usize,
    launch_contract: &model_manifest::ManifestLaunchContract,
) -> Vec<Vec<usize>> {
    if gpus.is_empty() {
        return vec![Vec::new()];
    }
    let mut ordered = gpus.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        right
            .total_mb
            .cmp(&left.total_mb)
            .then_with(|| {
                (left.index != primary_gpu_index).cmp(&(right.index != primary_gpu_index))
            })
            .then_with(|| left.index.cmp(&right.index))
    });
    let counts = match (preset, backend) {
        (ChatPreset::Performance, BackendMode::Nccl) => {
            let mut counts = Vec::new();
            let mut current = ordered.len();
            while current >= 1 {
                if nccl_supported_gpu_count(current, launch_contract) {
                    counts.push(current);
                }
                if current == 1 {
                    break;
                }
                current -= 1;
            }
            counts
        }
        (ChatPreset::Performance, BackendMode::DeviceLayers) => {
            (1..=ordered.len()).rev().collect::<Vec<_>>()
        }
        _ => (1..=ordered.len()).collect::<Vec<_>>(),
    };
    counts
        .into_iter()
        .map(|gpu_count| {
            let mut selected = Vec::with_capacity(gpu_count);
            if ordered.iter().any(|gpu| gpu.index == primary_gpu_index) {
                selected.push(primary_gpu_index);
            }
            selected.extend(
                ordered
                    .iter()
                    .filter(|gpu| gpu.index != primary_gpu_index)
                    .take(gpu_count.saturating_sub(selected.len()))
                    .map(|gpu| gpu.index),
            );
            selected.sort_by_key(|index| (*index != primary_gpu_index, *index));
            selected
        })
        .collect()
}

fn available_gpu_shape_rank(
    profile: &model_manifest::QualifiedHostProfile,
    hardware: &HardwareProfile,
) -> Option<(usize, u64, u64)> {
    let slice = available_gpu_slice_for_profile(profile, hardware)?;
    Some((
        slice.gpu_count,
        slice.total_gpu_memory_mb,
        slice.min_gpu_memory_mb,
    ))
}

fn profile_rank(profile: &model_manifest::QualifiedHostProfile) -> (usize, u64, u64) {
    (
        profile.max_gpu_count.unwrap_or(profile.min_gpu_count),
        profile.min_total_gpu_memory_mb.unwrap_or(0),
        profile.min_gpu_memory_mb,
    )
}

fn qualification_rank_key(
    profile: &model_manifest::QualifiedHostProfile,
) -> (usize, u64, u64, u32) {
    (
        profile.max_gpu_count.unwrap_or(profile.min_gpu_count),
        profile.min_total_gpu_memory_mb.unwrap_or(0),
        profile.min_gpu_memory_mb,
        profile.validated_context_cap,
    )
}

fn profile_matches_plan(
    profile: &model_manifest::QualifiedHostProfile,
    plan: &ChatRuntimePlan,
) -> bool {
    planned_gpu_slice(plan)
        .map(|slice| qualified_gpu_slice_matches_profile(profile, slice))
        .unwrap_or(false)
}

fn matching_plan_profile<'a>(
    profile: &'a model_manifest::RuntimeModelQualificationProfile,
    plan: &ChatRuntimePlan,
) -> Option<&'a model_manifest::QualifiedHostProfile> {
    profile
        .host_profiles
        .iter()
        .filter(|candidate| profile_matches_plan(candidate, plan))
        .max_by_key(|candidate| qualification_rank_key(candidate))
}

fn matching_available_profile<'a>(
    profile: &'a model_manifest::RuntimeModelQualificationProfile,
    hardware: &HardwareProfile,
) -> Option<&'a model_manifest::QualifiedHostProfile> {
    profile
        .host_profiles
        .iter()
        .filter(|candidate| host_profile_supports_hardware(candidate, hardware))
        .max_by_key(|candidate| {
            available_gpu_shape_rank(candidate, hardware).unwrap_or_else(|| profile_rank(candidate))
        })
}

fn plan_gpu_shape_matches_available_hardware(
    plan: &ChatRuntimePlan,
    hardware: &HardwareProfile,
) -> bool {
    let Some(slice) = planned_gpu_slice(plan) else {
        return false;
    };
    let available = HardwareProfile {
        gpus: hardware
            .gpus
            .iter()
            .filter(|gpu| gpu.total_mb >= slice.min_gpu_memory_mb)
            .cloned()
            .collect(),
        gpu0_desktop_reserve_mb: hardware.gpu0_desktop_reserve_mb,
        fingerprint: hardware.fingerprint.clone(),
    };
    let mut totals = available
        .gpus
        .iter()
        .map(|gpu| gpu.total_mb)
        .collect::<Vec<_>>();
    totals.sort_by(|left, right| right.cmp(left));
    available.gpus.len() >= slice.gpu_count
        && totals.into_iter().take(slice.gpu_count).sum::<u64>() >= slice.total_gpu_memory_mb
}

fn scale_mb(base_mb: u64, factor_milli: u32) -> u64 {
    ((base_mb as u128 * factor_milli as u128) / 1000u128) as u64
}

fn kv_cache_factor_for_type(cache_type: Option<&str>) -> u32 {
    match cache_type.map(|value| value.trim().to_ascii_lowercase()) {
        Some(cache_type) if cache_type == "turboquant3" => 550,
        Some(cache_type) if cache_type == "f8e4m3" => 780,
        _ => 1000,
    }
}

fn measured_kv_cache_mb_per_1k_tokens(
    manifest: &model_manifest::RuntimeModelManifest,
    cache_type: Option<&str>,
    quant: QuantOption,
) -> u64 {
    let explicit = cache_type
        .map(|value| value.trim().to_ascii_lowercase())
        .and_then(|key| {
            manifest
                .sizing
                .measurement_components
                .kv_cache_mb_per_1k_tokens_by_cache_type_q4
                .get(&key)
                .copied()
        });
    let _ = quant;
    let base = explicit.unwrap_or(manifest.sizing.kv_mb_per_1k_tokens_q4);
    let factor = if explicit.is_some() {
        1000
    } else {
        kv_cache_factor_for_type(cache_type)
    };
    scale_mb(base, factor)
}

fn measured_load_peak_overhead_mb(
    manifest: &model_manifest::RuntimeModelManifest,
    quant: QuantOption,
    backend: BackendMode,
    immediate_isq: bool,
) -> u64 {
    let components = &manifest.sizing.measurement_components;
    let mut base = if immediate_isq {
        components.load_overheads_mb_q4.immediate_isq_mb
    } else {
        components.load_overheads_mb_q4.plain_load_mb
    };
    if matches!(backend, BackendMode::Nccl) {
        base = base.saturating_add(components.load_overheads_mb_q4.nccl_init_mb);
    }
    if base == 0 {
        let base = scale_mb(
            manifest.sizing.load_peak_slack_mb_q4,
            quant.weight_factor_milli,
        );
        return match backend {
            BackendMode::Nccl => base / 2,
            BackendMode::DeviceLayers => base,
        };
    }
    scale_mb(base, quant.weight_factor_milli)
}

fn measured_backend_runtime_overhead_mb(
    manifest: &model_manifest::RuntimeModelManifest,
    quant: QuantOption,
    backend: BackendMode,
) -> u64 {
    let base = match backend {
        BackendMode::DeviceLayers => {
            manifest
                .sizing
                .measurement_components
                .backend_runtime_overheads_mb_q4
                .device_layers_mb
        }
        BackendMode::Nccl => {
            manifest
                .sizing
                .measurement_components
                .backend_runtime_overheads_mb_q4
                .nccl_mb
        }
    };
    if base == 0 {
        0
    } else {
        let _ = quant;
        base
    }
}

fn measured_activation_overhead_mb(
    manifest: &model_manifest::RuntimeModelManifest,
    quant: QuantOption,
    context: u32,
    max_seqs: u32,
) -> u64 {
    let components = &manifest
        .sizing
        .measurement_components
        .activation_overheads_mb_q4;
    let prefill =
        ((components.prefill_anchor_mb_at_128k as u128 * context as u128) / 131_072u128) as u64;
    let decode = components
        .decode_per_seq_mb
        .saturating_mul(max_seqs.max(1) as u64);
    let anchor = components.gpu0_anchor_runtime_mb;
    let _ = quant;
    prefill.saturating_add(decode).saturating_add(anchor)
}

fn parse_fraction_milli(value: Option<&str>) -> u32 {
    let Some(value) = value else {
        return 1000;
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return 1000;
    }
    let parsed = trimmed.parse::<f64>().ok().filter(|value| *value > 0.0);
    parsed
        .map(|value| ((value * 1000.0).round() as u32).clamp(1, 1000))
        .unwrap_or(1000)
}

fn empirical_repeating_weight_mb(harness: ModelHarness, quant: QuantOption) -> u64 {
    scale_mb(
        harness
            .sizing
            .repeating_layer_weight_mb_q4
            .saturating_mul(harness.sizing.repeating_layers as u64),
        quant.weight_factor_milli,
    )
}

fn empirical_non_repeating_weight_mb(harness: ModelHarness, quant: QuantOption) -> u64 {
    scale_mb(
        harness.sizing.non_repeating_weight_mb_q4,
        quant.weight_factor_milli,
    )
}

#[allow(dead_code)]
fn empirical_load_peak_slack_mb(
    harness: ModelHarness,
    quant: QuantOption,
    backend: BackendMode,
) -> u64 {
    let base = scale_mb(
        harness.sizing.load_peak_slack_mb_q4,
        quant.weight_factor_milli,
    );
    match backend {
        BackendMode::Nccl => base / 2,
        BackendMode::DeviceLayers => base,
    }
}

fn fixed_overhead_mb(backend: BackendMode, gpu_count: usize) -> u64 {
    match backend {
        BackendMode::Nccl => 1400u64.saturating_mul(gpu_count as u64),
        BackendMode::DeviceLayers => 900u64.saturating_add((gpu_count as u64) * 180),
    }
}

fn align_context(context: u32) -> u32 {
    if context < 1024 {
        return context;
    }
    (context / 1024) * 1024
}

fn proportional_shares(total: u64, weights: &[u64]) -> Vec<u64> {
    if weights.is_empty() {
        return Vec::new();
    }
    let weight_sum = weights.iter().copied().sum::<u64>().max(1);
    let mut shares = weights
        .iter()
        .map(|weight| total.saturating_mul(*weight) / weight_sum)
        .collect::<Vec<_>>();
    let mut distributed = shares.iter().copied().sum::<u64>();
    let mut idx = 0usize;
    while distributed < total {
        let target = idx % shares.len();
        shares[target] = shares[target].saturating_add(1);
        distributed += 1;
        idx += 1;
    }
    shares
}

fn even_shares(total: u64, count: usize) -> Vec<u64> {
    if count == 0 {
        return Vec::new();
    }
    let base = total / count as u64;
    let mut shares = vec![base; count];
    let mut remaining = total - (base * count as u64);
    let mut idx = 0usize;
    while remaining > 0 {
        shares[idx % count] += 1;
        remaining -= 1;
        idx += 1;
    }
    shares
}

fn infer_chat_source(env_map: &BTreeMap<String, String>) -> String {
    env_map
        .get("CTOX_CHAT_SOURCE")
        .cloned()
        .or_else(|| {
            let provider =
                crate::inference::runtime_state::infer_api_provider_from_env_map(env_map);
            env_map.get("CTOX_CHAT_MODEL").and_then(|value| {
                if engine::is_api_chat_model(value)
                    || (!provider.eq_ignore_ascii_case("local")
                        && engine::api_provider_supports_model(&provider, value))
                {
                    Some("api".to_string())
                } else {
                    None
                }
            })
        })
        .unwrap_or_else(|| "local".to_string())
}

fn parse_csv_indices(raw: Option<&String>) -> Vec<usize> {
    raw.map(|value| {
        value
            .split(',')
            .filter_map(|chunk| chunk.trim().parse::<usize>().ok())
            .collect::<Vec<_>>()
    })
    .unwrap_or_default()
}

fn inspect_hardware_profile(root: &Path) -> Result<HardwareProfile> {
    let empty_profile = || {
        let gpu0_reserve = gpu0_desktop_reserve_mb();
        HardwareProfile {
            fingerprint: hardware_fingerprint(&[], gpu0_reserve),
            gpus: Vec::new(),
            gpu0_desktop_reserve_mb: gpu0_reserve,
        }
    };

    if let Some(spec) = runtime_env::env_or_config(root, "CTOX_TEST_GPU_TOTALS_MB") {
        let gpus = spec
            .split(';')
            .filter_map(|chunk| {
                let (index, total) = chunk.split_once(':')?;
                Some(HardwareGpu {
                    index: index.trim().parse().ok()?,
                    name: format!("Test GPU {}", index.trim()),
                    total_mb: total.trim().parse().ok()?,
                })
            })
            .collect::<Vec<_>>();
        if !gpus.is_empty() {
            return Ok(HardwareProfile {
                fingerprint: hardware_fingerprint(&gpus, DEFAULT_GPU0_DESKTOP_RESERVE_MB),
                gpus,
                gpu0_desktop_reserve_mb: gpu0_desktop_reserve_mb(),
            });
        }
    }

    let output = match command_output_with_timeout(
        Command::new("nvidia-smi").args([
            "--query-gpu=index,name,memory.total",
            "--format=csv,noheader,nounits",
        ]),
        Duration::from_secs(NVIDIA_SMI_TIMEOUT_SECS),
    ) {
        Ok(output) => output,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(empty_profile()),
        Err(err) => return Err(err).context("failed to run nvidia-smi for hardware planner"),
    };
    if !output.status.success() {
        return Ok(empty_profile());
    }

    let mut gpus = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let parts = line.split(',').map(|part| part.trim()).collect::<Vec<_>>();
        if parts.len() < 3 {
            continue;
        }
        let Ok(index) = parts[0].parse::<usize>() else {
            continue;
        };
        let Ok(total_mb) = parts[2].parse::<u64>() else {
            continue;
        };
        gpus.push(HardwareGpu {
            index,
            name: parts[1].to_string(),
            total_mb,
        });
    }
    gpus.sort_by_key(|gpu| gpu.index);
    if gpus.is_empty() {
        return Ok(empty_profile());
    }
    let gpu0_reserve = gpu0_desktop_reserve_mb();
    Ok(HardwareProfile {
        fingerprint: hardware_fingerprint(&gpus, gpu0_reserve),
        gpus,
        gpu0_desktop_reserve_mb: gpu0_reserve,
    })
}

fn gpu0_desktop_reserve_mb() -> u64 {
    if std::env::var("CTOX_HEADLESS")
        .ok()
        .map(|value| value == "1")
        .unwrap_or(false)
    {
        return 0;
    }
    std::env::var("CTOX_GPU0_DESKTOP_RESERVE_MB")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_GPU0_DESKTOP_RESERVE_MB)
}

fn hardware_fingerprint(gpus: &[HardwareGpu], gpu0_desktop_reserve_mb: u64) -> String {
    let raw = format!(
        "gpu0_reserve={gpu0_desktop_reserve_mb};{}",
        gpus.iter()
            .map(|gpu| format!("{}:{}:{}", gpu.index, gpu.name, gpu.total_mb))
            .collect::<Vec<_>>()
            .join("|")
    );
    format!("{:x}", Sha256::digest(raw.as_bytes()))
}

fn command_output_with_timeout(
    command: &mut Command,
    timeout: Duration,
) -> std::io::Result<Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = command.spawn()?;
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait()?.is_some() {
            return child.wait_with_output();
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let reap_deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < reap_deadline {
                if child.try_wait()?.is_some() {
                    return child.wait_with_output();
                }
                thread::sleep(Duration::from_millis(50));
            }
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "command timed out",
            ));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn collect_platform_facts(root: Option<&Path>, hardware: &HardwareProfile) -> PlatformFacts {
    let nccl_override = root.and_then(|path| load_nccl_capability_override(path).ok().flatten());
    let declared = root
        .or_else(|| Some(Path::new(env!("CARGO_MANIFEST_DIR"))))
        .and_then(|path| {
            model_manifest::load_platform_capabilities(path)
                .ok()
                .flatten()
        });
    let cuda_available = declared
        .as_ref()
        .map(|value| value.cuda_available)
        .unwrap_or(!hardware.gpus.is_empty());
    let nccl_available = if nccl_override.is_some() {
        false
    } else {
        declared
            .as_ref()
            .map(|value| value.nccl_available)
            .unwrap_or(hardware.gpus.len() > 1)
    };
    let flash_attn_available = declared
        .as_ref()
        .map(|value| value.flash_attn_available)
        .unwrap_or(cuda_available);
    let mut validation_messages = Vec::new();
    if let Some(override_state) = &nccl_override {
        validation_messages.push(format!(
            "runtime override disabled nccl after {}: {} ({})",
            override_state.model, override_state.reason, override_state.signature
        ));
    }
    if let Some(contract) = &declared {
        if contract.cuda_available != !hardware.gpus.is_empty() {
            validation_messages.push(format!(
                "platform contract says cuda_available={} but live hardware sees {} GPUs",
                contract.cuda_available,
                hardware.gpus.len()
            ));
        }
        if contract.gpus.len() != hardware.gpus.len() {
            validation_messages.push(format!(
                "platform contract records {} GPUs but live hardware sees {}",
                contract.gpus.len(),
                hardware.gpus.len()
            ));
        }
        for gpu in &hardware.gpus {
            let Some(declared_gpu) = contract.gpus.iter().find(|item| item.index == gpu.index)
            else {
                validation_messages.push(format!(
                    "platform contract is missing GPU{} ({}, {}MB)",
                    gpu.index, gpu.name, gpu.total_mb
                ));
                continue;
            };
            if declared_gpu.total_mb != gpu.total_mb {
                validation_messages.push(format!(
                    "platform contract GPU{} total {}MB does not match live {}MB",
                    gpu.index, declared_gpu.total_mb, gpu.total_mb
                ));
            }
        }
    } else if !hardware.gpus.is_empty() {
        validation_messages.push(
            "platform contract missing; inferred platform capabilities from live hardware"
                .to_string(),
        );
    }
    PlatformFacts {
        hardware: hardware.clone(),
        cuda_available,
        nccl_available,
        flash_attn_available,
        validation_messages,
    }
}

fn platform_supports_backend(
    platform: &PlatformFacts,
    backend: BackendMode,
    gpu_count: usize,
    launch_contract: &model_manifest::ManifestLaunchContract,
) -> bool {
    match backend {
        BackendMode::DeviceLayers => platform.cuda_available && gpu_count >= 1,
        BackendMode::Nccl => {
            platform.cuda_available
                && platform.nccl_available
                && nccl_supported_gpu_count(gpu_count, launch_contract)
        }
    }
}

#[cfg(test)]
fn plan_gpt_oss_20b() -> ModelHarness {
    ModelHarness {
        model: "openai/gpt-oss-20b",
        sizing: EmpiricalSizingProfile {
            non_repeating_weight_mb_q4: 3_400,
            repeating_layer_weight_mb_q4: 516,
            load_peak_slack_mb_q4: 512,
            kv_mb_per_1k_tokens_q4: 220,
            base_toks_per_sec_q4: 90.0,
            repeating_layers: 24,
            context_cap: 131_072,
        },
        runtime: ModelRuntimeHarness {
            paged_attn: "auto",
            pa_cache_type: Some("turboquant3"),
            pa_memory_fraction: Some("0.80"),
            pa_context_len: Some(131_072),
            force_no_mmap: false,
            force_language_model_only: false,
            require_prebuilt_uqff_for_chat_start: false,
            disable_flash_attn: false,
            isq_singlethread: false,
            isq_cpu_threads: None,
            moe_experts_backend: None,
            small_uniform_device_layers_scale: Some(DeviceLayersScaleHint {
                max_gpu_memory_mb: 20_480,
                single_gpu_scale: 1.0,
                dual_gpu_scale: 1.02,
                multi_gpu_scale: 0.72,
            }),
        },
    }
}

#[cfg(test)]
fn plan_qwen35_2b() -> ModelHarness {
    ModelHarness {
        model: "Qwen/Qwen3.5-2B",
        sizing: EmpiricalSizingProfile {
            non_repeating_weight_mb_q4: 480,
            repeating_layer_weight_mb_q4: 56,
            load_peak_slack_mb_q4: 256,
            kv_mb_per_1k_tokens_q4: 56,
            base_toks_per_sec_q4: 185.0,
            repeating_layers: 24,
            context_cap: 262_144,
        },
        runtime: ModelRuntimeHarness {
            paged_attn: "auto",
            pa_cache_type: Some("turboquant3"),
            pa_memory_fraction: Some("0.80"),
            pa_context_len: None,
            force_no_mmap: false,
            force_language_model_only: false,
            require_prebuilt_uqff_for_chat_start: false,
            disable_flash_attn: false,
            isq_singlethread: false,
            isq_cpu_threads: None,
            moe_experts_backend: None,
            small_uniform_device_layers_scale: None,
        },
    }
}

#[cfg(test)]
fn plan_qwen35_4b() -> ModelHarness {
    ModelHarness {
        model: "Qwen/Qwen3.5-4B",
        sizing: EmpiricalSizingProfile {
            non_repeating_weight_mb_q4: 720,
            repeating_layer_weight_mb_q4: 90,
            load_peak_slack_mb_q4: 384,
            kv_mb_per_1k_tokens_q4: 78,
            base_toks_per_sec_q4: 140.0,
            repeating_layers: 32,
            context_cap: 262_144,
        },
        runtime: ModelRuntimeHarness {
            paged_attn: "auto",
            pa_cache_type: Some("turboquant3"),
            pa_memory_fraction: Some("0.80"),
            pa_context_len: None,
            force_no_mmap: false,
            force_language_model_only: false,
            require_prebuilt_uqff_for_chat_start: false,
            disable_flash_attn: false,
            isq_singlethread: false,
            isq_cpu_threads: None,
            moe_experts_backend: None,
            small_uniform_device_layers_scale: None,
        },
    }
}

#[cfg(test)]
fn plan_qwen35_9b() -> ModelHarness {
    ModelHarness {
        model: "Qwen/Qwen3.5-9B",
        sizing: EmpiricalSizingProfile {
            non_repeating_weight_mb_q4: 940,
            repeating_layer_weight_mb_q4: 180,
            load_peak_slack_mb_q4: 512,
            kv_mb_per_1k_tokens_q4: 112,
            base_toks_per_sec_q4: 95.0,
            repeating_layers: 32,
            context_cap: 262_144,
        },
        runtime: ModelRuntimeHarness {
            paged_attn: "auto",
            pa_cache_type: Some("turboquant3"),
            pa_memory_fraction: Some("0.80"),
            pa_context_len: None,
            force_no_mmap: false,
            force_language_model_only: false,
            require_prebuilt_uqff_for_chat_start: false,
            disable_flash_attn: false,
            isq_singlethread: false,
            isq_cpu_threads: None,
            moe_experts_backend: None,
            small_uniform_device_layers_scale: None,
        },
    }
}

#[cfg(test)]
fn plan_qwen35_27b() -> ModelHarness {
    ModelHarness {
        model: "Qwen/Qwen3.5-27B",
        sizing: EmpiricalSizingProfile {
            non_repeating_weight_mb_q4: 1_660,
            repeating_layer_weight_mb_q4: 260,
            load_peak_slack_mb_q4: 768,
            kv_mb_per_1k_tokens_q4: 248,
            base_toks_per_sec_q4: 45.0,
            repeating_layers: 64,
            context_cap: 262_144,
        },
        runtime: ModelRuntimeHarness {
            paged_attn: "auto",
            pa_cache_type: Some("turboquant3"),
            pa_memory_fraction: Some("0.80"),
            pa_context_len: None,
            force_no_mmap: false,
            force_language_model_only: false,
            require_prebuilt_uqff_for_chat_start: false,
            disable_flash_attn: false,
            isq_singlethread: false,
            isq_cpu_threads: None,
            moe_experts_backend: None,
            small_uniform_device_layers_scale: None,
        },
    }
}

#[cfg(test)]
fn plan_qwen35_35b_a3b() -> ModelHarness {
    ModelHarness {
        model: "Qwen/Qwen3.5-35B-A3B",
        sizing: EmpiricalSizingProfile {
            non_repeating_weight_mb_q4: 3_500,
            repeating_layer_weight_mb_q4: 450,
            load_peak_slack_mb_q4: 2_200,
            kv_mb_per_1k_tokens_q4: 270,
            base_toks_per_sec_q4: 38.0,
            repeating_layers: 40,
            context_cap: 262_144,
        },
        runtime: ModelRuntimeHarness {
            paged_attn: "auto",
            pa_cache_type: Some("turboquant3"),
            pa_memory_fraction: Some("0.80"),
            pa_context_len: None,
            force_no_mmap: false,
            force_language_model_only: false,
            require_prebuilt_uqff_for_chat_start: false,
            disable_flash_attn: false,
            isq_singlethread: false,
            isq_cpu_threads: None,
            moe_experts_backend: None,
            small_uniform_device_layers_scale: None,
        },
    }
}

#[cfg(test)]
fn plan_gemma4_e2b() -> ModelHarness {
    ModelHarness {
        model: "google/gemma-4-E2B-it",
        sizing: EmpiricalSizingProfile {
            non_repeating_weight_mb_q4: 520,
            repeating_layer_weight_mb_q4: 64,
            load_peak_slack_mb_q4: 320,
            kv_mb_per_1k_tokens_q4: 52,
            base_toks_per_sec_q4: 155.0,
            repeating_layers: 35,
            context_cap: 262_144,
        },
        runtime: ModelRuntimeHarness {
            paged_attn: "off",
            pa_cache_type: None,
            pa_memory_fraction: None,
            pa_context_len: None,
            force_no_mmap: false,
            force_language_model_only: false,
            require_prebuilt_uqff_for_chat_start: true,
            disable_flash_attn: true,
            isq_singlethread: true,
            isq_cpu_threads: None,
            moe_experts_backend: Some("fast"),
            small_uniform_device_layers_scale: None,
        },
    }
}

#[cfg(test)]
fn plan_gemma4_e4b() -> ModelHarness {
    ModelHarness {
        model: "google/gemma-4-E4B-it",
        sizing: EmpiricalSizingProfile {
            non_repeating_weight_mb_q4: 760,
            repeating_layer_weight_mb_q4: 88,
            load_peak_slack_mb_q4: 384,
            kv_mb_per_1k_tokens_q4: 72,
            base_toks_per_sec_q4: 120.0,
            repeating_layers: 42,
            context_cap: 262_144,
        },
        runtime: ModelRuntimeHarness {
            paged_attn: "off",
            pa_cache_type: None,
            pa_memory_fraction: None,
            pa_context_len: None,
            force_no_mmap: false,
            force_language_model_only: false,
            require_prebuilt_uqff_for_chat_start: true,
            disable_flash_attn: true,
            isq_singlethread: true,
            isq_cpu_threads: None,
            moe_experts_backend: Some("fast"),
            small_uniform_device_layers_scale: None,
        },
    }
}

#[cfg(test)]
fn plan_gemma4_26b_a4b() -> ModelHarness {
    ModelHarness {
        model: "google/gemma-4-26B-A4B-it",
        sizing: EmpiricalSizingProfile {
            non_repeating_weight_mb_q4: 2_800,
            repeating_layer_weight_mb_q4: 520,
            load_peak_slack_mb_q4: 2_400,
            kv_mb_per_1k_tokens_q4: 140,
            base_toks_per_sec_q4: 56.0,
            repeating_layers: 30,
            context_cap: 262_144,
        },
        runtime: ModelRuntimeHarness {
            paged_attn: "auto",
            pa_cache_type: Some("f8e4m3"),
            pa_memory_fraction: Some("0.80"),
            pa_context_len: Some(131_072),
            force_no_mmap: false,
            force_language_model_only: false,
            require_prebuilt_uqff_for_chat_start: false,
            disable_flash_attn: false,
            isq_singlethread: true,
            isq_cpu_threads: None,
            moe_experts_backend: Some("fast"),
            small_uniform_device_layers_scale: None,
        },
    }
}

#[cfg(test)]
fn plan_gemma4_31b() -> ModelHarness {
    ModelHarness {
        model: "google/gemma-4-31B-it",
        sizing: EmpiricalSizingProfile {
            non_repeating_weight_mb_q4: 1_700,
            repeating_layer_weight_mb_q4: 330,
            load_peak_slack_mb_q4: 1_400,
            kv_mb_per_1k_tokens_q4: 250,
            base_toks_per_sec_q4: 40.0,
            repeating_layers: 60,
            context_cap: 262_144,
        },
        runtime: ModelRuntimeHarness {
            paged_attn: "auto",
            pa_cache_type: Some("f8e4m3"),
            pa_memory_fraction: Some("0.80"),
            pa_context_len: Some(131_072),
            force_no_mmap: false,
            force_language_model_only: false,
            require_prebuilt_uqff_for_chat_start: false,
            disable_flash_attn: false,
            isq_singlethread: true,
            isq_cpu_threads: None,
            moe_experts_backend: Some("fast"),
            small_uniform_device_layers_scale: None,
        },
    }
}

#[cfg(test)]
fn manifest_candidate(
    quantization: model_manifest::ManifestQuantization,
    backend: model_manifest::ManifestBackendMode,
    max_batch_size: u32,
    max_seqs: u32,
    context_fraction_milli: u32,
    context_target_cap: Option<u32>,
    min_context_required: u32,
    per_gpu_headroom_mb: u64,
) -> model_manifest::PresetCandidateSpec {
    model_manifest::PresetCandidateSpec {
        quantization,
        backend,
        max_batch_size,
        max_seqs,
        context_fraction_milli,
        context_target_cap,
        min_context_required,
        per_gpu_headroom_mb,
    }
}

#[cfg(test)]
fn manifest_profile(
    objective: model_manifest::RuntimeObjectiveLabel,
    candidates: Vec<model_manifest::PresetCandidateSpec>,
) -> model_manifest::ManifestPresetProfile {
    model_manifest::ManifestPresetProfile {
        objective,
        candidates,
    }
}

#[cfg(test)]
fn manifest_from_harness(
    harness: ModelHarness,
    placement: model_manifest::ManifestPlacementProfile,
    quality: model_manifest::ManifestPresetProfile,
    performance: model_manifest::ManifestPresetProfile,
) -> model_manifest::RuntimeModelManifest {
    let launch_contract = launch_contract_for_model(harness.model);
    model_manifest::RuntimeModelManifest {
        model: harness.model.to_string(),
        sizing: model_manifest::ManifestSizingProfile {
            non_repeating_weight_mb_q4: harness.sizing.non_repeating_weight_mb_q4,
            repeating_layer_weight_mb_q4: harness.sizing.repeating_layer_weight_mb_q4,
            load_peak_slack_mb_q4: harness.sizing.load_peak_slack_mb_q4,
            kv_mb_per_1k_tokens_q4: harness.sizing.kv_mb_per_1k_tokens_q4,
            base_toks_per_sec_q4: harness.sizing.base_toks_per_sec_q4,
            repeating_layers: harness.sizing.repeating_layers,
            context_cap: harness.sizing.context_cap,
            measurement_components: model_manifest::ManifestMeasurementComponents::default(),
        },
        runtime_defaults: model_manifest::ManifestRuntimeDefaults {
            paged_attn: harness.runtime.paged_attn.to_string(),
            pa_cache_type: harness.runtime.pa_cache_type.map(str::to_string),
            pa_memory_fraction: harness.runtime.pa_memory_fraction.map(str::to_string),
            pa_context_len: harness.runtime.pa_context_len,
            force_no_mmap: harness.runtime.force_no_mmap,
            force_language_model_only: harness.runtime.force_language_model_only,
            require_prebuilt_uqff_for_chat_start: harness
                .runtime
                .require_prebuilt_uqff_for_chat_start,
            disable_flash_attn: harness.runtime.disable_flash_attn,
            isq_singlethread: harness.runtime.isq_singlethread,
            isq_cpu_threads: harness.runtime.isq_cpu_threads,
        },
        planner_hints: model_manifest::ManifestPlannerHints {
            moe_experts_backend: harness.runtime.moe_experts_backend.map(str::to_string),
            small_uniform_device_layers_scale: harness
                .runtime
                .small_uniform_device_layers_scale
                .map(|hint| model_manifest::ManifestDeviceLayersScaleHint {
                    max_gpu_memory_mb: hint.max_gpu_memory_mb,
                    single_gpu_scale: hint.single_gpu_scale,
                    dual_gpu_scale: hint.dual_gpu_scale,
                    multi_gpu_scale: hint.multi_gpu_scale,
                }),
        },
        placement,
        launch_contract,
        quality,
        performance,
    }
}

#[cfg(test)]
fn default_launch_contract() -> model_manifest::ManifestLaunchContract {
    model_manifest::ManifestLaunchContract::default()
}

#[cfg(test)]
fn gpt_oss_launch_contract() -> model_manifest::ManifestLaunchContract {
    model_manifest::ManifestLaunchContract {
        required_context_tokens: MIN_POLICY_CONTEXT,
        require_primary_gpu_anchor: true,
        nccl_qualification: model_manifest::ManifestNcclQualification::Qualified,
        nccl_preserves_primary_gpu_anchor: true,
        allow_subset_anchored_nccl: false,
    }
}

#[cfg(test)]
fn qwen35_2b_launch_contract() -> model_manifest::ManifestLaunchContract {
    model_manifest::ManifestLaunchContract {
        required_context_tokens: MIN_POLICY_CONTEXT,
        require_primary_gpu_anchor: true,
        nccl_qualification: model_manifest::ManifestNcclQualification::Qualified,
        nccl_preserves_primary_gpu_anchor: true,
        allow_subset_anchored_nccl: true,
    }
}

#[cfg(test)]
fn qwen35_4b_launch_contract() -> model_manifest::ManifestLaunchContract {
    model_manifest::ManifestLaunchContract {
        required_context_tokens: MIN_POLICY_CONTEXT,
        require_primary_gpu_anchor: true,
        nccl_qualification: model_manifest::ManifestNcclQualification::Qualified,
        nccl_preserves_primary_gpu_anchor: true,
        allow_subset_anchored_nccl: true,
    }
}

#[cfg(test)]
fn gemma4_31b_launch_contract() -> model_manifest::ManifestLaunchContract {
    model_manifest::ManifestLaunchContract {
        required_context_tokens: MIN_POLICY_CONTEXT,
        require_primary_gpu_anchor: true,
        nccl_qualification: model_manifest::ManifestNcclQualification::Qualified,
        nccl_preserves_primary_gpu_anchor: true,
        allow_subset_anchored_nccl: false,
    }
}

#[cfg(test)]
fn launch_contract_for_model(model: &str) -> model_manifest::ManifestLaunchContract {
    match model {
        "openai/gpt-oss-20b" => gpt_oss_launch_contract(),
        "Qwen/Qwen3.5-2B" => qwen35_2b_launch_contract(),
        "Qwen/Qwen3.5-4B" => qwen35_4b_launch_contract(),
        "google/gemma-4-E2B-it" => gemma4_31b_launch_contract(),
        "google/gemma-4-E4B-it" => gemma4_31b_launch_contract(),
        "google/gemma-4-31B-it" => gemma4_31b_launch_contract(),
        _ => default_launch_contract(),
    }
}

fn default_placement_profile() -> model_manifest::ManifestPlacementProfile {
    model_manifest::ManifestPlacementProfile {
        primary_gpu_index: 0,
        primary_gpu_holds_non_repeating: true,
        primary_gpu_desktop_reserve_mb: DEFAULT_GPU0_DESKTOP_RESERVE_MB,
    }
}

#[cfg(test)]
fn default_gpt_oss_manifest() -> model_manifest::RuntimeModelManifest {
    manifest_from_harness(
        plan_gpt_oss_20b(),
        default_placement_profile(),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Quality,
            vec![manifest_candidate(
                model_manifest::ManifestQuantization::NativeMxfp4,
                model_manifest::ManifestBackendMode::DeviceLayers,
                1,
                1,
                1000,
                Some(131_072),
                MIN_POLICY_CONTEXT,
                0,
            )],
        ),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Performance,
            vec![
                manifest_candidate(
                    model_manifest::ManifestQuantization::NativeMxfp4,
                    model_manifest::ManifestBackendMode::Nccl,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    0,
                ),
                manifest_candidate(
                    model_manifest::ManifestQuantization::NativeMxfp4,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    0,
                ),
            ],
        ),
    )
}

#[cfg(test)]
fn default_qwen35_2b_manifest() -> model_manifest::RuntimeModelManifest {
    manifest_from_harness(
        plan_qwen35_2b(),
        default_placement_profile(),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Quality,
            vec![
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q5k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    256,
                ),
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q4k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    256,
                ),
            ],
        ),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Performance,
            vec![manifest_candidate(
                model_manifest::ManifestQuantization::Q4k,
                model_manifest::ManifestBackendMode::DeviceLayers,
                1,
                1,
                1000,
                Some(131_072),
                MIN_POLICY_CONTEXT,
                256,
            )],
        ),
    )
}

#[cfg(test)]
fn default_qwen35_4b_manifest() -> model_manifest::RuntimeModelManifest {
    manifest_from_harness(
        plan_qwen35_4b(),
        default_placement_profile(),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Quality,
            vec![
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q4k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    512,
                ),
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q5k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    512,
                ),
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q4k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    512,
                ),
            ],
        ),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Performance,
            vec![
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q4k,
                    model_manifest::ManifestBackendMode::Nccl,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    512,
                ),
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q4k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    512,
                ),
            ],
        ),
    )
}

#[cfg(test)]
fn default_qwen35_9b_manifest() -> model_manifest::RuntimeModelManifest {
    manifest_from_harness(
        plan_qwen35_9b(),
        default_placement_profile(),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Quality,
            vec![
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q4k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    768,
                ),
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q5k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    768,
                ),
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q6k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    768,
                ),
            ],
        ),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Performance,
            vec![manifest_candidate(
                model_manifest::ManifestQuantization::Q4k,
                model_manifest::ManifestBackendMode::DeviceLayers,
                1,
                1,
                1000,
                Some(131_072),
                MIN_POLICY_CONTEXT,
                768,
            )],
        ),
    )
}

#[cfg(test)]
fn default_qwen35_27b_manifest() -> model_manifest::RuntimeModelManifest {
    manifest_from_harness(
        plan_qwen35_27b(),
        default_placement_profile(),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Quality,
            vec![
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q6k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    768,
                ),
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q5k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    768,
                ),
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q6k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    768,
                ),
            ],
        ),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Performance,
            vec![manifest_candidate(
                model_manifest::ManifestQuantization::Q4k,
                model_manifest::ManifestBackendMode::DeviceLayers,
                1,
                1,
                1000,
                Some(131_072),
                MIN_POLICY_CONTEXT,
                768,
            )],
        ),
    )
}

#[cfg(test)]
fn default_qwen35_35b_a3b_manifest() -> model_manifest::RuntimeModelManifest {
    manifest_from_harness(
        plan_qwen35_35b_a3b(),
        default_placement_profile(),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Quality,
            vec![
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q6k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    768,
                ),
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q5k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    512,
                ),
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q4k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    0,
                ),
            ],
        ),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Performance,
            vec![
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q4k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    512,
                ),
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q4k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    0,
                ),
            ],
        ),
    )
}

#[cfg(test)]
fn default_gemma4_e2b_manifest() -> model_manifest::RuntimeModelManifest {
    manifest_from_harness(
        plan_gemma4_e2b(),
        default_placement_profile(),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Quality,
            vec![manifest_candidate(
                model_manifest::ManifestQuantization::Q4k,
                model_manifest::ManifestBackendMode::DeviceLayers,
                1,
                1,
                1000,
                Some(131_072),
                MIN_POLICY_CONTEXT,
                192,
            )],
        ),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Performance,
            vec![manifest_candidate(
                model_manifest::ManifestQuantization::Q4k,
                model_manifest::ManifestBackendMode::DeviceLayers,
                1,
                1,
                1000,
                Some(131_072),
                MIN_POLICY_CONTEXT,
                192,
            )],
        ),
    )
}

#[cfg(test)]
fn default_gemma4_e4b_manifest() -> model_manifest::RuntimeModelManifest {
    manifest_from_harness(
        plan_gemma4_e4b(),
        default_placement_profile(),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Quality,
            vec![manifest_candidate(
                model_manifest::ManifestQuantization::Q4k,
                model_manifest::ManifestBackendMode::DeviceLayers,
                1,
                1,
                1000,
                Some(131_072),
                MIN_POLICY_CONTEXT,
                256,
            )],
        ),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Performance,
            vec![manifest_candidate(
                model_manifest::ManifestQuantization::Q4k,
                model_manifest::ManifestBackendMode::DeviceLayers,
                1,
                1,
                1000,
                Some(131_072),
                MIN_POLICY_CONTEXT,
                256,
            )],
        ),
    )
}

#[cfg(test)]
fn default_gemma4_26b_a4b_manifest() -> model_manifest::RuntimeModelManifest {
    manifest_from_harness(
        plan_gemma4_26b_a4b(),
        default_placement_profile(),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Quality,
            vec![
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q6k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    768,
                ),
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q5k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    512,
                ),
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q4k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    256,
                ),
            ],
        ),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Performance,
            vec![
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q4k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    256,
                ),
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q5k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    512,
                ),
            ],
        ),
    )
}

#[cfg(test)]
fn default_gemma4_31b_manifest() -> model_manifest::RuntimeModelManifest {
    manifest_from_harness(
        plan_gemma4_31b(),
        default_placement_profile(),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Quality,
            vec![
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q6k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    768,
                ),
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q5k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    512,
                ),
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q4k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(131_072),
                    MIN_POLICY_CONTEXT,
                    256,
                ),
            ],
        ),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Performance,
            vec![manifest_candidate(
                model_manifest::ManifestQuantization::Q4k,
                model_manifest::ManifestBackendMode::DeviceLayers,
                1,
                1,
                1000,
                Some(131_072),
                MIN_POLICY_CONTEXT,
                256,
            )],
        ),
    )
}

#[cfg(test)]
fn default_nemotron_manifest() -> model_manifest::RuntimeModelManifest {
    manifest_from_harness(
        plan_nemotron_cascade_from_manifest_seed(),
        default_placement_profile(),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Quality,
            vec![
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q6k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(65_536),
                    65_536,
                    768,
                ),
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q5k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(65_536),
                    65_536,
                    512,
                ),
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q4k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(65_536),
                    65_536,
                    0,
                ),
            ],
        ),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Performance,
            vec![
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q4k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    2,
                    2,
                    1000,
                    Some(32_768),
                    65_536,
                    512,
                ),
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q4k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(65_536),
                    65_536,
                    768,
                ),
            ],
        ),
    )
}

#[cfg(test)]
fn plan_nemotron_cascade_from_manifest_seed() -> ModelHarness {
    ModelHarness {
        model: "nvidia/Nemotron-Cascade-2-30B-A3B",
        sizing: EmpiricalSizingProfile {
            non_repeating_weight_mb_q4: 2_800,
            repeating_layer_weight_mb_q4: 390,
            load_peak_slack_mb_q4: 1_600,
            kv_mb_per_1k_tokens_q4: 140,
            base_toks_per_sec_q4: 42.0,
            repeating_layers: 52,
            context_cap: 262_144,
        },
        runtime: ModelRuntimeHarness {
            paged_attn: "auto",
            pa_cache_type: Some("turboquant3"),
            pa_memory_fraction: Some("0.45"),
            pa_context_len: None,
            force_no_mmap: false,
            force_language_model_only: false,
            require_prebuilt_uqff_for_chat_start: false,
            disable_flash_attn: true,
            isq_singlethread: true,
            isq_cpu_threads: None,
            moe_experts_backend: None,
            small_uniform_device_layers_scale: None,
        },
    }
}

#[cfg(test)]
fn default_glm47_flash_manifest() -> model_manifest::RuntimeModelManifest {
    manifest_from_harness(
        plan_glm47_flash(),
        default_placement_profile(),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Quality,
            vec![
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q4k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(65_536),
                    65_536,
                    768,
                ),
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q6k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(65_536),
                    65_536,
                    768,
                ),
            ],
        ),
        manifest_profile(
            model_manifest::RuntimeObjectiveLabel::Performance,
            vec![
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q4k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(65_536),
                    65_536,
                    768,
                ),
                manifest_candidate(
                    model_manifest::ManifestQuantization::Q6k,
                    model_manifest::ManifestBackendMode::DeviceLayers,
                    1,
                    1,
                    1000,
                    Some(65_536),
                    65_536,
                    768,
                ),
            ],
        ),
    )
}

#[cfg(test)]
fn default_runtime_manifest_for_model(model: &str) -> Option<model_manifest::RuntimeModelManifest> {
    Some(match model.trim() {
        "openai/gpt-oss-20b" => default_gpt_oss_manifest(),
        "Qwen/Qwen3.5-2B" => default_qwen35_2b_manifest(),
        "Qwen/Qwen3.5-4B" => default_qwen35_4b_manifest(),
        "Qwen/Qwen3.5-9B" => default_qwen35_9b_manifest(),
        "Qwen/Qwen3.5-27B" => default_qwen35_27b_manifest(),
        "Qwen/Qwen3.5-35B-A3B" => default_qwen35_35b_a3b_manifest(),
        "google/gemma-4-E2B-it" => default_gemma4_e2b_manifest(),
        "google/gemma-4-E4B-it" => default_gemma4_e4b_manifest(),
        "google/gemma-4-26B-A4B-it" => default_gemma4_26b_a4b_manifest(),
        "google/gemma-4-31B-it" => default_gemma4_31b_manifest(),
        "nvidia/Nemotron-Cascade-2-30B-A3B" => default_nemotron_manifest(),
        "zai-org/GLM-4.7-Flash" => default_glm47_flash_manifest(),
        _ => return None,
    })
}

fn model_placement_profile(
    root: Option<&Path>,
    model: &str,
) -> model_manifest::ManifestPlacementProfile {
    runtime_manifest(root, model)
        .map(|manifest| manifest.placement)
        .unwrap_or_else(default_placement_profile)
}

fn desktop_reserve_mb_for_gpu(
    placement: &model_manifest::ManifestPlacementProfile,
    hardware: &HardwareProfile,
    gpu_index: usize,
) -> u64 {
    if gpu_index == placement.primary_gpu_index {
        placement
            .primary_gpu_desktop_reserve_mb
            .max(hardware.gpu0_desktop_reserve_mb)
    } else {
        0
    }
}

fn runtime_manifest(
    root: Option<&Path>,
    model: &str,
) -> Option<model_manifest::RuntimeModelManifest> {
    let mut search_roots = Vec::new();
    if let Some(root) = root {
        search_roots.push(root.to_path_buf());
    }
    search_roots.push(Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf());
    search_roots.into_iter().find_map(|root| {
        model_manifest::load_runtime_model_manifest(&root, model)
            .ok()
            .flatten()
    })
}

fn default_auxiliary_manifest_for_model(
    model: &str,
) -> Option<model_manifest::AuxiliaryModelManifest> {
    let profile = engine::model_profile_for_model(model).ok()?;
    let (role, gpu_reserve_mb) = match profile.runtime.family {
        engine::LocalModelFamily::Qwen3Embedding => ("embedding", 1_100),
        engine::LocalModelFamily::VoxtralTranscription => ("stt", 4_200),
        engine::LocalModelFamily::Qwen3Speech | engine::LocalModelFamily::VoxtralSpeech => {
            ("tts", 1_400)
        }
        engine::LocalModelFamily::Qwen3VisionAuxiliary => ("vision", 3_500),
        _ => return None,
    };
    Some(model_manifest::AuxiliaryModelManifest {
        model: profile.runtime.model,
        role: role.to_string(),
        gpu_reserve_mb,
        placement: model_manifest::AuxiliaryPlacementProfile {
            primary_gpu_index: 0,
            use_primary_gpu_by_default: true,
            supports_multi_gpu_expansion: false,
        },
    })
}

pub(crate) fn auxiliary_manifest(
    root: Option<&Path>,
    model: &str,
) -> Option<model_manifest::AuxiliaryModelManifest> {
    root.and_then(|root| {
        model_manifest::load_auxiliary_model_manifest(root, model)
            .ok()
            .flatten()
    })
    .or_else(|| default_auxiliary_manifest_for_model(model))
}

fn plan_specs_from_manifest_profile(
    profile: &model_manifest::ManifestPresetProfile,
) -> Vec<PlanSpec> {
    profile
        .candidates
        .iter()
        .map(|candidate| PlanSpec {
            quant: match candidate.quantization {
                model_manifest::ManifestQuantization::Q4k => Q4K,
                model_manifest::ManifestQuantization::Q5k => Q5K,
                model_manifest::ManifestQuantization::Q6k => Q6K,
                model_manifest::ManifestQuantization::NativeMxfp4 => MXFP4_NATIVE,
            },
            backend: match candidate.backend {
                model_manifest::ManifestBackendMode::DeviceLayers => BackendMode::DeviceLayers,
                model_manifest::ManifestBackendMode::Nccl => BackendMode::Nccl,
            },
            context_target: candidate.context_target_cap,
            context_fraction_milli: candidate.context_fraction_milli,
            min_context_required: candidate.min_context_required,
            per_gpu_headroom_mb: candidate.per_gpu_headroom_mb,
            max_batch_size: candidate.max_batch_size,
            max_seqs: candidate.max_seqs,
        })
        .collect()
}

#[cfg(test)]
#[allow(dead_code)]
fn plan_nemotron_cascade() -> ModelHarness {
    plan_nemotron_cascade_from_manifest(&default_nemotron_manifest())
}

#[cfg(test)]
#[allow(dead_code)]
fn plan_nemotron_cascade_from_manifest(
    manifest: &model_manifest::RuntimeModelManifest,
) -> ModelHarness {
    harness_from_manifest(manifest)
}

fn harness_from_manifest(manifest: &model_manifest::RuntimeModelManifest) -> ModelHarness {
    ModelHarness {
        model: Box::leak(manifest.model.clone().into_boxed_str()),
        sizing: EmpiricalSizingProfile {
            non_repeating_weight_mb_q4: manifest.sizing.non_repeating_weight_mb_q4,
            repeating_layer_weight_mb_q4: manifest.sizing.repeating_layer_weight_mb_q4,
            load_peak_slack_mb_q4: manifest.sizing.load_peak_slack_mb_q4,
            kv_mb_per_1k_tokens_q4: manifest.sizing.kv_mb_per_1k_tokens_q4,
            base_toks_per_sec_q4: manifest.sizing.base_toks_per_sec_q4,
            repeating_layers: manifest.sizing.repeating_layers,
            context_cap: manifest.sizing.context_cap,
        },
        runtime: ModelRuntimeHarness {
            paged_attn: Box::leak(
                manifest
                    .runtime_defaults
                    .paged_attn
                    .clone()
                    .into_boxed_str(),
            ),
            pa_cache_type: manifest
                .runtime_defaults
                .pa_cache_type
                .clone()
                .map(|value| Box::leak(value.into_boxed_str()) as &'static str),
            pa_memory_fraction: manifest
                .runtime_defaults
                .pa_memory_fraction
                .clone()
                .map(|value| Box::leak(value.into_boxed_str()) as &'static str),
            pa_context_len: manifest.runtime_defaults.pa_context_len,
            force_no_mmap: manifest.runtime_defaults.force_no_mmap,
            force_language_model_only: manifest.runtime_defaults.force_language_model_only,
            require_prebuilt_uqff_for_chat_start: manifest
                .runtime_defaults
                .require_prebuilt_uqff_for_chat_start,
            disable_flash_attn: manifest.runtime_defaults.disable_flash_attn,
            isq_singlethread: manifest.runtime_defaults.isq_singlethread,
            isq_cpu_threads: manifest.runtime_defaults.isq_cpu_threads,
            moe_experts_backend: manifest
                .planner_hints
                .moe_experts_backend
                .clone()
                .map(|value| Box::leak(value.into_boxed_str()) as &'static str),
            small_uniform_device_layers_scale: manifest
                .planner_hints
                .small_uniform_device_layers_scale
                .map(|hint| DeviceLayersScaleHint {
                    max_gpu_memory_mb: hint.max_gpu_memory_mb,
                    single_gpu_scale: hint.single_gpu_scale,
                    dual_gpu_scale: hint.dual_gpu_scale,
                    multi_gpu_scale: hint.multi_gpu_scale,
                }),
        },
    }
}

#[cfg(test)]
fn plan_glm47_flash() -> ModelHarness {
    ModelHarness {
        model: "zai-org/GLM-4.7-Flash",
        sizing: EmpiricalSizingProfile {
            non_repeating_weight_mb_q4: 3_190,
            repeating_layer_weight_mb_q4: 400,
            load_peak_slack_mb_q4: 1_800,
            kv_mb_per_1k_tokens_q4: 275,
            base_toks_per_sec_q4: 48.0,
            repeating_layers: 47,
            context_cap: 65_536,
        },
        runtime: ModelRuntimeHarness {
            paged_attn: "auto",
            pa_cache_type: Some("turboquant3"),
            pa_memory_fraction: Some("0.65"),
            pa_context_len: None,
            force_no_mmap: true,
            force_language_model_only: false,
            require_prebuilt_uqff_for_chat_start: false,
            disable_flash_attn: true,
            isq_singlethread: false,
            isq_cpu_threads: None,
            moe_experts_backend: None,
            small_uniform_device_layers_scale: None,
        },
    }
}

#[cfg(test)]
#[path = "runtime_plan_boundary_tests.rs"]
mod boundary_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    static TEST_ENV_LOCK: Mutex<()> = Mutex::new(());

    fn hardware(count: usize, total_mb: u64) -> HardwareProfile {
        let gpus = (0..count)
            .map(|index| HardwareGpu {
                index,
                name: format!("GPU {index}"),
                total_mb,
            })
            .collect::<Vec<_>>();
        HardwareProfile {
            fingerprint: "test".to_string(),
            gpus,
            gpu0_desktop_reserve_mb: 1024,
        }
    }

    fn hardware_totals(totals_mb: &[u64]) -> HardwareProfile {
        let gpus = totals_mb
            .iter()
            .enumerate()
            .map(|(index, total_mb)| HardwareGpu {
                index,
                name: format!("GPU {index}"),
                total_mb: *total_mb,
            })
            .collect::<Vec<_>>();
        HardwareProfile {
            fingerprint: "test".to_string(),
            gpus,
            gpu0_desktop_reserve_mb: 1024,
        }
    }

    fn sum_device_layers(spec: &str) -> u32 {
        spec.split(';')
            .filter_map(|entry| entry.split_once(':'))
            .map(|(_, layers)| layers.parse::<u32>().unwrap())
            .sum()
    }

    fn chat_only_env_map() -> BTreeMap<String, String> {
        BTreeMap::from([
            (
                "CTOX_DISABLE_EMBEDDING_BACKEND".to_string(),
                "1".to_string(),
            ),
            ("CTOX_DISABLE_STT_BACKEND".to_string(), "1".to_string()),
            ("CTOX_DISABLE_TTS_BACKEND".to_string(), "1".to_string()),
        ])
    }

    fn temp_root(label: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("ctox-runtime-plan-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        root
    }

    /// Inject a fake GPU totals spec for `inspect_hardware_profile` by
    /// appending `CTOX_TEST_GPU_TOTALS_MB=<spec>` to `<root>/runtime/engine.env`.
    /// The key is not allowlisted for process-env overrides, so writing to the
    /// test-root's engine.env gives each test its own isolated fake hardware
    /// without mutating process state.
    fn write_test_gpu_totals(root: &Path, spec: &str) {
        let runtime_dir = root.join("runtime");
        std::fs::create_dir_all(&runtime_dir).unwrap();
        let env_path = runtime_dir.join("engine.env");
        let mut existing = std::fs::read_to_string(&env_path).unwrap_or_default();
        if !existing.is_empty() && !existing.ends_with('\n') {
            existing.push('\n');
        }
        existing.push_str(&format!("CTOX_TEST_GPU_TOTALS_MB={spec}\n"));
        std::fs::write(&env_path, existing).unwrap();
    }

    fn write_platform_contract(root: &Path, gpu_count: usize, total_mb: u64, nccl_available: bool) {
        let payload = json!({
            "generated_at": "2026-04-05T00:00:00Z",
            "source": "test",
            "cuda_available": gpu_count > 0,
            "nccl_available": nccl_available,
            "flash_attn_available": gpu_count > 0,
            "gpus": (0..gpu_count).map(|index| json!({
                "index": index,
                "name": format!("GPU {index}"),
                "total_mb": total_mb,
                "compute_capability": "8.6"
            })).collect::<Vec<_>>(),
        });
        std::fs::write(
            root.join("runtime").join("platform_capabilities.json"),
            serde_json::to_vec_pretty(&payload).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn aux_reserves_are_manifest_driven() {
        let env_map = BTreeMap::new();
        let reserves = compute_aux_reserves_mb(
            None,
            &hardware(3, 24_576),
            &env_map,
            ChatPreset::Quality,
            &[0, 1, 2],
        );
        assert_eq!(reserves.get(&0).copied(), Some(6_700));
        assert_eq!(reserves.get(&1).copied().unwrap_or(0), 0);
        assert_eq!(reserves.get(&2).copied().unwrap_or(0), 0);
    }

    #[test]
    fn apply_chat_runtime_plan_reuses_matching_persisted_plan() {
        let unique = temp_root("reuse-persisted-plan");
        let env_map = chat_only_env_map();
        let bundle = build_bundle_for_model(
            &unique,
            "Qwen/Qwen3.5-4B",
            ChatPreset::Quality,
            &hardware(3, 20_470),
            &env_map,
        )
        .unwrap();
        let persisted = bundle.selected_plan.clone();
        store_persisted_chat_runtime_plan(&unique, Some(&persisted)).unwrap();

        let mut request_env = chat_only_env_map();
        request_env.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        request_env.insert("CTOX_CHAT_MODEL".to_string(), "Qwen/Qwen3.5-4B".to_string());
        request_env.insert(
            "CTOX_CHAT_LOCAL_PRESET".to_string(),
            ChatPreset::Quality.label().to_string(),
        );

        let resolved = apply_chat_runtime_plan(&unique, &mut request_env)
            .unwrap()
            .expect("persisted plan should be reused");
        assert_eq!(resolved, persisted);
    }

    #[test]
    fn aux_reserves_are_cleared_when_aux_backends_are_disabled() {
        let mut env_map = BTreeMap::new();
        env_map.insert(
            "CTOX_DISABLE_AUXILIARY_BACKENDS".to_string(),
            "1".to_string(),
        );
        let reserves = compute_aux_reserves_mb(
            None,
            &hardware(3, 24_576),
            &env_map,
            ChatPreset::Quality,
            &[0, 1, 2],
        );
        assert!(reserves.is_empty(), "{reserves:?}");
    }

    #[test]
    fn runtime_fleet_plan_prefers_aux_only_gpu_and_cpu_fallbacks_for_remaining_roles() {
        let env_map = BTreeMap::new();
        let chat_plan = ChatRuntimePlan {
            model: "Qwen/Qwen3.5-4B".to_string(),
            preset: ChatPreset::Quality,
            quantization: "Q6K".to_string(),
            runtime_isq: Some("Q6K".to_string()),
            max_seq_len: 131_072,
            compaction_threshold_percent: 75,
            compaction_min_tokens: 12_288,
            min_context_floor_applied: true,
            paged_attn: "auto".to_string(),
            pa_cache_type: Some("turboquant3".to_string()),
            pa_memory_fraction: Some("0.80".to_string()),
            pa_context_len: None,
            disable_nccl: true,
            tensor_parallel_backend: None,
            mn_local_world_size: None,
            max_batch_size: 1,
            max_seqs: 1,
            cuda_visible_devices: "0,1,2".to_string(),
            device_layers: Some("0:10;1:11;2:11".to_string()),
            topology: None,
            allow_device_layers_with_topology: false,
            nm_device_ordinal: Some(0),
            base_device_ordinal: Some(0),
            moe_experts_backend: None,
            disable_flash_attn: true,
            force_no_mmap: false,
            force_language_model_only: false,
            require_prebuilt_uqff_for_chat_start: false,
            isq_singlethread: true,
            isq_cpu_threads: None,
            expected_tok_s: 100.0,
            hardware_fingerprint: "test".to_string(),
            theoretical_breakdown: TheoreticalResourceBreakdown {
                contract_source: "test".to_string(),
                effective_total_budget_mb: 60_000,
                kv_budget_cap_mb: 40_000,
                kv_budget_fraction_milli: 800,
                weight_residency_mb: 4_463,
                kv_cache_mb: 5_504,
                fixed_runtime_base_overhead_mb: 1_440,
                backend_runtime_overhead_mb: 0,
                activation_overhead_mb: 10_368,
                load_peak_overhead_mb: 476,
                safety_headroom_mb: 1_536,
                required_effective_total_budget_mb: 14_795,
                required_total_mb: 25_163,
            },
            rationale: vec!["test".to_string()],
            gpu_allocations: vec![
                PlannedGpuAllocation {
                    gpu_index: 0,
                    name: "GPU 0".to_string(),
                    total_mb: 24_576,
                    desktop_reserve_mb: 1_024,
                    aux_reserve_mb: 0,
                    chat_budget_mb: 16_000,
                    backend_overhead_mb: 0,
                    activation_overhead_mb: 0,
                    load_peak_overhead_mb: 0,
                    repeating_weight_mb: 0,
                    weight_mb: 0,
                    kv_cache_mb: 0,
                    free_headroom_mb: 0,
                    chat_enabled: true,
                },
                PlannedGpuAllocation {
                    gpu_index: 1,
                    name: "GPU 1".to_string(),
                    total_mb: 24_576,
                    desktop_reserve_mb: 0,
                    aux_reserve_mb: 0,
                    chat_budget_mb: 16_000,
                    backend_overhead_mb: 0,
                    activation_overhead_mb: 0,
                    load_peak_overhead_mb: 0,
                    repeating_weight_mb: 0,
                    weight_mb: 0,
                    kv_cache_mb: 0,
                    free_headroom_mb: 0,
                    chat_enabled: true,
                },
                PlannedGpuAllocation {
                    gpu_index: 2,
                    name: "GPU 2".to_string(),
                    total_mb: 24_576,
                    desktop_reserve_mb: 0,
                    aux_reserve_mb: 0,
                    chat_budget_mb: 16_000,
                    backend_overhead_mb: 0,
                    activation_overhead_mb: 0,
                    load_peak_overhead_mb: 0,
                    repeating_weight_mb: 0,
                    weight_mb: 0,
                    kv_cache_mb: 0,
                    free_headroom_mb: 0,
                    chat_enabled: true,
                },
                PlannedGpuAllocation {
                    gpu_index: 3,
                    name: "GPU 3".to_string(),
                    total_mb: 24_576,
                    desktop_reserve_mb: 0,
                    aux_reserve_mb: 0,
                    chat_budget_mb: 0,
                    backend_overhead_mb: 0,
                    activation_overhead_mb: 0,
                    load_peak_overhead_mb: 0,
                    repeating_weight_mb: 0,
                    weight_mb: 0,
                    kv_cache_mb: 0,
                    free_headroom_mb: 0,
                    chat_enabled: false,
                },
            ],
        };

        let fleet =
            resolve_runtime_fleet_plan(Path::new("/tmp"), &env_map, Some(&chat_plan)).unwrap();

        assert_eq!(
            fleet
                .embedding
                .as_ref()
                .and_then(|plan| plan.visible_devices.as_deref()),
            Some("3")
        );
        assert_eq!(
            fleet.transcription.as_ref().map(|plan| plan.compute_target),
            None
        );
        assert_eq!(fleet.speech.as_ref().map(|plan| plan.compute_target), None);
    }

    #[test]
    fn runtime_fleet_plan_moves_all_aux_roles_to_cpu_when_chat_uses_every_gpu() {
        let env_map = BTreeMap::new();
        let chat_plan = ChatRuntimePlan {
            model: "Qwen/Qwen3.5-9B".to_string(),
            preset: ChatPreset::Quality,
            quantization: "Q6K".to_string(),
            runtime_isq: Some("Q6K".to_string()),
            max_seq_len: 131_072,
            compaction_threshold_percent: 75,
            compaction_min_tokens: 12_288,
            min_context_floor_applied: true,
            paged_attn: "auto".to_string(),
            pa_cache_type: Some("turboquant3".to_string()),
            pa_memory_fraction: Some("0.80".to_string()),
            pa_context_len: None,
            disable_nccl: true,
            tensor_parallel_backend: None,
            mn_local_world_size: None,
            max_batch_size: 1,
            max_seqs: 1,
            cuda_visible_devices: "0,1,2,3".to_string(),
            device_layers: Some("0:10;1:11;2:11;3:11".to_string()),
            topology: None,
            allow_device_layers_with_topology: false,
            nm_device_ordinal: Some(0),
            base_device_ordinal: Some(0),
            moe_experts_backend: None,
            disable_flash_attn: true,
            force_no_mmap: false,
            force_language_model_only: false,
            require_prebuilt_uqff_for_chat_start: false,
            isq_singlethread: true,
            isq_cpu_threads: None,
            expected_tok_s: 100.0,
            hardware_fingerprint: "test".to_string(),
            theoretical_breakdown: TheoreticalResourceBreakdown {
                contract_source: "test".to_string(),
                effective_total_budget_mb: 60_000,
                kv_budget_cap_mb: 40_000,
                kv_budget_fraction_milli: 800,
                weight_residency_mb: 4_463,
                kv_cache_mb: 5_504,
                fixed_runtime_base_overhead_mb: 1_440,
                backend_runtime_overhead_mb: 0,
                activation_overhead_mb: 10_368,
                load_peak_overhead_mb: 476,
                safety_headroom_mb: 1_536,
                required_effective_total_budget_mb: 14_795,
                required_total_mb: 25_163,
            },
            rationale: vec!["test".to_string()],
            gpu_allocations: (0..4)
                .map(|gpu_index| PlannedGpuAllocation {
                    gpu_index,
                    name: format!("GPU {gpu_index}"),
                    total_mb: 24_576,
                    desktop_reserve_mb: if gpu_index == 0 { 1_024 } else { 0 },
                    aux_reserve_mb: 0,
                    chat_budget_mb: 16_000,
                    backend_overhead_mb: 0,
                    activation_overhead_mb: 0,
                    load_peak_overhead_mb: 0,
                    repeating_weight_mb: 0,
                    weight_mb: 0,
                    kv_cache_mb: 0,
                    free_headroom_mb: 0,
                    chat_enabled: true,
                })
                .collect(),
        };

        let fleet =
            resolve_runtime_fleet_plan(Path::new("/tmp"), &env_map, Some(&chat_plan)).unwrap();

        assert_eq!(
            fleet.embedding.as_ref().map(|plan| plan.compute_target),
            Some(engine::ComputeTarget::Cpu)
        );
        assert!(fleet.transcription.is_none());
        assert!(fleet.speech.is_none());
    }

    #[test]
    fn live_aux_leases_do_not_double_count_manifest_aux_reserves() {
        let hardware = hardware(3, 24_576);
        let manifest_aux_reserves = BTreeMap::from([(0usize, 6_700u64)]);
        let live_aux_reserves = BTreeMap::from([(0usize, 6_700u64), (1usize, 1_100u64)]);

        let effective = effective_aux_reserves_mb(
            &hardware,
            &manifest_aux_reserves,
            &live_aux_reserves,
            ChatPreset::Quality,
            &[0],
        )
        .0;

        assert_eq!(effective.get(&0).copied(), Some(6_700));
        assert_eq!(effective.get(&1).copied(), Some(1_100));
        assert_eq!(effective.get(&2).copied().unwrap_or(0), 0);
    }

    #[test]
    fn performance_can_reclaim_live_aux_leases_on_target_chat_gpus() {
        let hardware = hardware(3, 20_480);
        let manifest_aux_reserves = BTreeMap::new();
        let live_aux_reserves = BTreeMap::from([(2usize, 6_700u64)]);

        let (effective, reclaimed) = effective_aux_reserves_mb(
            &hardware,
            &manifest_aux_reserves,
            &live_aux_reserves,
            ChatPreset::Performance,
            &[0, 1, 2],
        );

        assert!(effective.is_empty(), "{effective:?}");
        assert!(reclaimed);
    }

    #[test]
    fn aux_default_distribution_prefers_dedicated_non_chat_gpu() {
        let env_map: BTreeMap<String, String> = BTreeMap::new();
        let selection = engine::auxiliary_model_selection(
            engine::AuxiliaryRole::Stt,
            env_map.get("CTOX_STT_MODEL").map(String::as_str),
        );
        let distribution = default_aux_distribution(
            None,
            &selection,
            &hardware(4, 24_576),
            ChatPreset::Performance,
            &[1, 2, 3],
        );
        assert_eq!(distribution, vec![0]);
    }

    #[test]
    fn quality_aux_distribution_yields_no_gpu_when_chat_uses_all_gpus() {
        let env_map: BTreeMap<String, String> = BTreeMap::new();
        let selection = engine::auxiliary_model_selection(
            engine::AuxiliaryRole::Stt,
            env_map.get("CTOX_STT_MODEL").map(String::as_str),
        );
        let distribution = default_aux_distribution(
            None,
            &selection,
            &hardware(3, 20_480),
            ChatPreset::Quality,
            &[0, 1, 2],
        );
        assert!(distribution.is_empty(), "{distribution:?}");
    }

    #[test]
    fn performance_aux_distribution_yields_no_gpu_when_chat_uses_all_gpus() {
        let env_map: BTreeMap<String, String> = BTreeMap::new();
        let selection = engine::auxiliary_model_selection(
            engine::AuxiliaryRole::Stt,
            env_map.get("CTOX_STT_MODEL").map(String::as_str),
        );
        let distribution = default_aux_distribution(
            None,
            &selection,
            &hardware(3, 20_480),
            ChatPreset::Performance,
            &[0, 1, 2],
        );
        assert!(distribution.is_empty(), "{distribution:?}");
    }

    #[test]
    fn resolve_auxiliary_visible_devices_uses_persisted_chat_plan_hardware() {
        let unique = temp_root("aux-visible-devices-persisted-plan");
        let env_map = chat_only_env_map();
        runtime_env::save_runtime_env_map(&unique, &env_map).unwrap();
        let bundle = build_bundle_for_model(
            &unique,
            "Qwen/Qwen3.5-4B",
            ChatPreset::Quality,
            &hardware(4, 20_470),
            &env_map,
        )
        .unwrap();
        store_persisted_chat_runtime_plan(&unique, Some(&bundle.selected_plan)).unwrap();

        let visible_devices = resolve_auxiliary_visible_devices(
            &unique,
            engine::AuxiliaryRole::Embedding,
            "Qwen/Qwen3-Embedding-0.6B",
        )
        .unwrap();
        assert!(
            visible_devices
                .as_deref()
                .map(str::trim)
                .is_none_or(|value| !value.is_empty())
        );
    }

    #[test]
    fn candidate_gpu_subsets_keep_primary_gpu_anchored_on_mixed_hosts() {
        let gpus = vec![
            HardwareGpu {
                index: 0,
                name: "primary".to_string(),
                total_mb: 20_480,
            },
            HardwareGpu {
                index: 1,
                name: "strong-a".to_string(),
                total_mb: 49_152,
            },
            HardwareGpu {
                index: 2,
                name: "strong-b".to_string(),
                total_mb: 49_152,
            },
        ];
        let candidates = candidate_chat_gpu_indices(
            ChatPreset::Performance,
            BackendMode::Nccl,
            &gpus,
            0,
            &qwen35_4b_launch_contract(),
        );
        assert!(!candidates.is_empty());
        for subset in candidates {
            assert_eq!(subset.first().copied(), Some(0), "{subset:?}");
            assert!(subset.contains(&0), "{subset:?}");
        }
    }

    #[test]
    fn performance_on_four_gpus_keeps_primary_gpu_in_the_nccl_set() {
        let env_map = BTreeMap::new();
        let plan =
            build_gpt_oss_20b_bundle(ChatPreset::Performance, &hardware(4, 24_576), &env_map)
                .selected_plan;
        assert_eq!(plan.cuda_visible_devices, "0,1,2,3");
        assert_eq!(plan.tensor_parallel_backend.as_deref(), Some("nccl"));
        assert_eq!(plan.mn_local_world_size, Some(4));
    }

    #[test]
    fn platform_contract_can_disable_nccl_without_disabling_the_model() {
        let root = temp_root("platform-no-nccl");
        write_platform_contract(&root, 4, 24_576, false);
        let env_map = BTreeMap::new();
        let bundle = build_bundle_for_model(
            &root,
            "openai/gpt-oss-20b",
            ChatPreset::Performance,
            &hardware(4, 24_576),
            &env_map,
        )
        .unwrap();
        assert!(
            bundle.selected_plan.disable_nccl,
            "{:#?}",
            bundle.selected_plan
        );
        assert_eq!(bundle.selected_plan.tensor_parallel_backend, None);
        assert!(
            bundle
                .selected_plan
                .rationale
                .iter()
                .any(|line| line.contains("platform cuda=true nccl=false")),
            "{:#?}",
            bundle.selected_plan.rationale
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn runtime_nccl_override_can_disable_nccl_without_disabling_the_model() {
        let root = temp_root("runtime-no-nccl");
        persist_nccl_capability_override(
            &root,
            "openai/gpt-oss-20b",
            "startup failed",
            "ncclInvalidUsage",
        )
        .unwrap();
        let env_map = BTreeMap::new();
        let bundle = build_bundle_for_model(
            &root,
            "openai/gpt-oss-20b",
            ChatPreset::Performance,
            &hardware(4, 24_576),
            &env_map,
        )
        .unwrap();
        assert!(
            bundle.selected_plan.disable_nccl,
            "{:#?}",
            bundle.selected_plan
        );
        assert_eq!(bundle.selected_plan.tensor_parallel_backend, None);
        assert!(
            bundle
                .selected_plan
                .rationale
                .iter()
                .any(|line| line.contains("runtime override disabled nccl")),
            "{:#?}",
            bundle.selected_plan.rationale
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn platform_contract_drift_is_visible_in_plan_rationale() {
        let root = temp_root("platform-drift");
        write_platform_contract(&root, 2, 24_576, true);
        let env_map = BTreeMap::new();
        let bundle = build_bundle_for_model(
            &root,
            "Qwen/Qwen3.5-4B",
            ChatPreset::Quality,
            &hardware(3, 24_576),
            &env_map,
        )
        .unwrap();
        assert!(
            bundle
                .selected_plan
                .rationale
                .iter()
                .any(|line| line
                    .contains("platform contract records 2 GPUs but live hardware sees 3")),
            "{:#?}",
            bundle.selected_plan.rationale
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn gpt_oss_single_gpu_performance_falls_back_without_host_nccl_qualification() {
        let env_map = BTreeMap::new();
        let plan =
            build_gpt_oss_20b_bundle(ChatPreset::Performance, &hardware(1, 24_576), &env_map)
                .selected_plan;
        assert!(plan.disable_nccl);
        assert_eq!(plan.tensor_parallel_backend, None);
    }

    #[test]
    fn gpt_oss_three_gpu_performance_falls_back_to_device_layers() {
        let env_map = BTreeMap::new();
        let plan =
            build_gpt_oss_20b_bundle(ChatPreset::Performance, &hardware(3, 24_576), &env_map)
                .selected_plan;
        assert!(plan_satisfies_context_policy(&plan), "{plan:?}");
        assert!(plan.disable_nccl, "{plan:?}");
        assert_eq!(plan.tensor_parallel_backend, None);
        assert_eq!(plan.mn_local_world_size, None);
        assert!(plan.device_layers.is_some(), "{plan:?}");
        assert_eq!(plan.cuda_visible_devices, "0,1,2");
        assert_eq!(plan.nm_device_ordinal, Some(0));
        assert_eq!(plan.base_device_ordinal, Some(0));
    }

    #[test]
    fn launch_contract_rejects_unqualified_nccl() {
        let contract = model_manifest::ManifestLaunchContract::default();
        assert!(!launch_contract_allows_backend(
            &contract,
            BackendMode::Nccl
        ));
        assert!(launch_contract_allows_backend(
            &contract,
            BackendMode::DeviceLayers
        ));
    }

    #[test]
    fn gpt_oss_launch_contract_qualifies_nccl_with_primary_anchor() {
        let contract = launch_contract_for_model("openai/gpt-oss-20b");
        assert_eq!(
            contract.nccl_qualification,
            model_manifest::ManifestNcclQualification::Qualified
        );
        assert!(contract.require_primary_gpu_anchor);
        assert!(contract.nccl_preserves_primary_gpu_anchor);
        assert!(!contract.allow_subset_anchored_nccl);
        assert!(launch_contract_allows_backend(&contract, BackendMode::Nccl));
    }

    #[test]
    fn qwen4b_launch_contract_qualifies_nccl_with_primary_anchor() {
        let contract = launch_contract_for_model("Qwen/Qwen3.5-4B");
        assert_eq!(
            contract.nccl_qualification,
            model_manifest::ManifestNcclQualification::Qualified
        );
        assert!(contract.require_primary_gpu_anchor);
        assert!(contract.nccl_preserves_primary_gpu_anchor);
        assert!(launch_contract_allows_backend(&contract, BackendMode::Nccl));
    }

    #[test]
    fn qwen4b_performance_prefers_nccl_on_three_gpu_anchor_plus_two_worker_hosts() {
        let env_map = chat_only_env_map();
        let plan = build_qwen35_4b_bundle(ChatPreset::Performance, &hardware(3, 24_576), &env_map)
            .selected_plan;
        assert!(plan_satisfies_context_policy(&plan), "{plan:?}");
        assert_eq!(plan.tensor_parallel_backend.as_deref(), Some("nccl"));
        assert_eq!(plan.mn_local_world_size, Some(2));
        assert!(plan.device_layers.is_none(), "{plan:?}");
        assert_eq!(plan.cuda_visible_devices, "1,2,0");
        assert_eq!(plan.nm_device_ordinal, Some(2));
        assert_eq!(plan.base_device_ordinal, Some(2));
    }

    #[test]
    fn qwen4b_two_gpu_performance_falls_back_without_two_worker_nccl_topology() {
        let env_map = chat_only_env_map();
        let plan = build_qwen35_4b_bundle(ChatPreset::Performance, &hardware(2, 24_576), &env_map)
            .selected_plan;
        assert!(plan_satisfies_context_policy(&plan), "{plan:?}");
        assert!(plan.disable_nccl);
        assert_eq!(plan.tensor_parallel_backend, None);
        assert!(plan.device_layers.is_some(), "{plan:?}");
    }

    #[test]
    fn qwen4b_single_gpu_performance_falls_back_without_nccl_topology() {
        let env_map = chat_only_env_map();
        let plan = build_qwen35_4b_bundle(ChatPreset::Performance, &hardware(1, 24_576), &env_map)
            .selected_plan;
        assert!(plan_satisfies_context_policy(&plan), "{plan:?}");
        assert!(plan.disable_nccl);
        assert_eq!(plan.tensor_parallel_backend, None);
        assert!(plan.device_layers.is_some(), "{plan:?}");
    }

    #[test]
    fn turboquant3_reduces_kv_cache_factor() {
        assert_eq!(kv_cache_factor_for_type(Some("turboquant3")), 550);
        assert_eq!(kv_cache_factor_for_type(Some("f8e4m3")), 780);
        assert_eq!(kv_cache_factor_for_type(Some("off")), 1000);
        assert_eq!(kv_cache_factor_for_type(None), 1000);
    }

    #[test]
    fn explicit_qwen4b_resource_contract_does_not_scale_kv_or_activation_with_weight_quant() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let manifest = model_manifest::load_runtime_model_manifest(root, "Qwen/Qwen3.5-4B")
            .unwrap()
            .unwrap();
        assert_eq!(
            measured_kv_cache_mb_per_1k_tokens(&manifest, Some("turboquant3"), Q4K),
            43
        );
        assert_eq!(
            measured_kv_cache_mb_per_1k_tokens(&manifest, Some("turboquant3"), Q6K),
            43
        );
        assert_eq!(
            measured_activation_overhead_mb(&manifest, Q4K, 131_072, 1),
            10_368
        );
        assert_eq!(
            measured_activation_overhead_mb(&manifest, Q6K, 131_072, 1),
            10_368
        );
    }

    #[test]
    fn apply_chat_runtime_plan_env_prefers_available_uqff_artifact() {
        let root = temp_root("uqff-artifact-env");
        let env_map = BTreeMap::new();
        let plan = build_qwen35_4b_bundle(ChatPreset::Quality, &hardware(4, 20_470), &env_map)
            .selected_plan;
        let artifact_path = chat_quant_artifact_path(&root, &plan)
            .expect("runtime-isq plan should produce quant artifact path");
        std::fs::create_dir_all(artifact_path.parent().expect("artifact parent")).unwrap();
        std::fs::write(&artifact_path, b"uqff-ready").unwrap();

        let mut applied_env = BTreeMap::new();
        apply_chat_runtime_plan_env(&root, &plan, &mut applied_env).unwrap();

        assert_eq!(
            applied_env.get("CTOX_ENGINE_FROM_UQFF"),
            Some(&artifact_path.display().to_string())
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn chat_quant_artifact_path_honors_shared_cache_root_override() {
        let root = temp_root("uqff-artifact-override");
        let shared_cache = temp_root("uqff-artifact-shared");
        let env_map = BTreeMap::new();
        let plan = build_qwen35_4b_bundle(ChatPreset::Quality, &hardware(4, 20_470), &env_map)
            .selected_plan;

        std::env::set_var(QUANT_ARTIFACTS_ROOT_ENV, shared_cache.display().to_string());
        let artifact_path = chat_quant_artifact_path(&root, &plan)
            .expect("runtime-isq plan should produce quant artifact path");
        std::env::remove_var(QUANT_ARTIFACTS_ROOT_ENV);

        assert!(
            artifact_path.starts_with(&shared_cache),
            "{artifact_path:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&shared_cache);
    }

    #[test]
    fn chat_quant_artifact_path_is_stable_across_temp_roots_for_same_engine_binary() {
        let root_a = temp_root("uqff-artifact-stable-a");
        let root_b = temp_root("uqff-artifact-stable-b");
        let shared_cache = temp_root("uqff-artifact-stable-shared");
        let env_map = BTreeMap::new();
        let plan_a = build_qwen35_4b_bundle(ChatPreset::Quality, &hardware(4, 20_470), &env_map)
            .selected_plan;
        let plan_b = build_qwen35_4b_bundle(ChatPreset::Quality, &hardware(4, 20_470), &env_map)
            .selected_plan;

        for root in [&root_a, &root_b] {
            let binary = engine::discover_source_layout_paths(root).model_runtime_binary;
            std::fs::create_dir_all(binary.parent().expect("binary parent")).unwrap();
            std::fs::write(&binary, b"same-engine-binary").unwrap();
        }

        std::env::set_var(QUANT_ARTIFACTS_ROOT_ENV, shared_cache.display().to_string());
        let artifact_path_a = chat_quant_artifact_path(&root_a, &plan_a)
            .expect("runtime-isq plan should produce quant artifact path");
        let artifact_path_b = chat_quant_artifact_path(&root_b, &plan_b)
            .expect("runtime-isq plan should produce quant artifact path");
        std::env::remove_var(QUANT_ARTIFACTS_ROOT_ENV);

        assert_eq!(artifact_path_a, artifact_path_b);
        let _ = std::fs::remove_dir_all(&root_a);
        let _ = std::fs::remove_dir_all(&root_b);
        let _ = std::fs::remove_dir_all(&shared_cache);
    }

    #[test]
    fn pa_memory_fraction_parser_defaults_safely() {
        assert_eq!(parse_fraction_milli(Some("0.80")), 800);
        assert_eq!(parse_fraction_milli(Some("1.2")), 1000);
        assert_eq!(parse_fraction_milli(Some("")), 1000);
        assert_eq!(parse_fraction_milli(None), 1000);
    }

    #[test]
    fn quality_keeps_policy_floor() {
        let env_map = BTreeMap::new();
        let plan = build_qwen35_4b_bundle(ChatPreset::Quality, &hardware(1, 24_576), &env_map)
            .selected_plan;
        assert!(plan.max_seq_len >= MIN_POLICY_CONTEXT);
    }

    #[test]
    fn performance_keeps_policy_floor_for_supported_models() {
        let env_map = BTreeMap::new();
        let gpt_oss =
            build_gpt_oss_20b_bundle(ChatPreset::Performance, &hardware(3, 24_576), &env_map)
                .selected_plan;
        let qwen4 = build_qwen35_4b_bundle(ChatPreset::Performance, &hardware(1, 24_576), &env_map)
            .selected_plan;
        assert!(plan_satisfies_context_policy(&gpt_oss), "{gpt_oss:?}");
        assert!(plan_satisfies_context_policy(&qwen4), "{qwen4:?}");
        assert_eq!(gpt_oss.max_seq_len, MIN_POLICY_CONTEXT);
        assert_eq!(qwen4.max_seq_len, MIN_POLICY_CONTEXT);
    }

    #[test]
    fn gpt_oss_uses_native_quantization_without_isq() {
        let env_map = BTreeMap::new();
        let plan = build_gpt_oss_20b_bundle(ChatPreset::Quality, &hardware(3, 24_576), &env_map)
            .selected_plan;
        assert_eq!(plan.quantization, "native_mxfp4");
        assert_eq!(plan.runtime_isq, None);
        assert_eq!(
            sum_device_layers(plan.device_layers.as_deref().unwrap()),
            24
        );
    }

    #[test]
    fn gpt_oss_quality_runtime_plan_sets_high_reasoning_cap_without_output_cap() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let unique = std::env::temp_dir().join(format!(
            "ctox_gpt_oss_harmony_caps_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(unique.join("runtime")).unwrap();
        std::fs::write(
            unique.join("runtime/engine.env"),
            "CTOX_CHAT_SOURCE=local\nCTOX_ACTIVE_MODEL=openai/gpt-oss-20b\n",
        )
        .unwrap();

        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        env_map.insert(
            "CTOX_ACTIVE_MODEL".to_string(),
            "openai/gpt-oss-20b".to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_MODEL".to_string(),
            "openai/gpt-oss-20b".to_string(),
        );

        write_test_gpu_totals(&unique, "0:24576;1:24576;2:24576");
        let _plan = apply_chat_runtime_plan(&unique, &mut env_map)
            .unwrap()
            .expect("expected runtime plan");

        assert!(!env_map.contains_key("CTOX_LOCAL_ADAPTER_REASONING_CAP"));
        assert!(!env_map.contains_key("CTOX_LOCAL_ADAPTER_MAX_OUTPUT_TOKENS_CAP"));

        let _ = std::fs::remove_dir_all(unique);
    }

    #[test]
    fn gpt_oss_performance_plan_keeps_low_reasoning_cap_without_output_cap() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let unique = std::env::temp_dir().join(format!(
            "ctox_perf_reasoning_cap_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&unique).unwrap();

        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        env_map.insert(
            "CTOX_ACTIVE_MODEL".to_string(),
            "openai/gpt-oss-20b".to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_MODEL".to_string(),
            "openai/gpt-oss-20b".to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_LOCAL_PRESET".to_string(),
            ChatPreset::Performance.label().to_string(),
        );

        write_test_gpu_totals(&unique, "0:24576;1:24576;2:24576");
        let _plan = apply_chat_runtime_plan(&unique, &mut env_map)
            .unwrap()
            .expect("expected runtime plan");

        assert!(!env_map.contains_key("CTOX_LOCAL_ADAPTER_REASONING_CAP"));
        assert!(!env_map.contains_key("CTOX_LOCAL_ADAPTER_MAX_OUTPUT_TOKENS_CAP"));

        let _ = std::fs::remove_dir_all(unique);
    }

    #[test]
    fn qwen_runtime_plan_persists_qwen_engine_port() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let unique = std::env::temp_dir().join(format!(
            "ctox_qwen_runtime_port_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(unique.join("runtime")).unwrap();
        std::fs::write(
            unique.join("runtime/engine.env"),
            "CTOX_CHAT_SOURCE=local\nCTOX_ACTIVE_MODEL=Qwen/Qwen3.5-4B\n",
        )
        .unwrap();

        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        env_map.insert(
            "CTOX_ACTIVE_MODEL".to_string(),
            "Qwen/Qwen3.5-4B".to_string(),
        );
        env_map.insert("CTOX_CHAT_MODEL".to_string(), "Qwen/Qwen3.5-4B".to_string());

        write_test_gpu_totals(&unique, "0:20470;1:20470;2:20470");
        let plan = apply_chat_runtime_plan(&unique, &mut env_map)
            .unwrap()
            .expect("expected runtime plan");

        assert_eq!(plan.model, "Qwen/Qwen3.5-4B");
        assert_eq!(
            env_map.get("CTOX_ENGINE_PORT").map(String::as_str),
            Some("1235")
        );

        let _ = std::fs::remove_dir_all(unique);
    }

    #[test]
    fn stale_materialized_runtime_limits_do_not_cap_replanned_local_chat() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let unique = std::env::temp_dir().join(format!(
            "ctox_replan_ignore_stale_limits_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(unique.join("runtime")).unwrap();
        std::fs::write(
            unique.join("runtime/engine.env"),
            "CTOX_CHAT_SOURCE=local\nCTOX_ACTIVE_MODEL=openai/gpt-oss-20b\n",
        )
        .unwrap();

        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        env_map.insert(
            "CTOX_ACTIVE_MODEL".to_string(),
            "openai/gpt-oss-20b".to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_MODEL".to_string(),
            "openai/gpt-oss-20b".to_string(),
        );
        env_map.insert("CTOX_CHAT_RUNTIME_PLAN_ACTIVE".to_string(), "1".to_string());
        env_map.insert(
            "CTOX_CHAT_RUNTIME_PLAN_DIGEST".to_string(),
            "stale".to_string(),
        );
        env_map.insert("CTOX_ENGINE_MAX_SEQ_LEN".to_string(), "9216".to_string());
        env_map.insert(
            "CTOX_ENGINE_REALIZED_MAX_SEQ_LEN".to_string(),
            "9216".to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_MODEL_REALIZED_CONTEXT".to_string(),
            "9216".to_string(),
        );
        env_map.insert(
            "CTOX_ENGINE_REALIZED_MODEL".to_string(),
            "openai/gpt-oss-20b".to_string(),
        );

        write_test_gpu_totals(&unique, "0:24576;1:24576;2:24576");
        let plan = apply_chat_runtime_plan(&unique, &mut env_map)
            .unwrap()
            .expect("expected runtime plan");

        assert!(plan.max_seq_len > 9_216, "{plan:?}");
        assert_eq!(
            env_map.get("CTOX_ENGINE_MAX_SEQ_LEN").map(String::as_str),
            Some("131072")
        );

        let _ = std::fs::remove_dir_all(unique);
    }

    #[test]
    fn qwen35_moe_uses_aux_aware_runtime_constraints() {
        let env_map = BTreeMap::new();
        let plan = build_qwen35_35b_a3b_bundle(ChatPreset::Quality, &hardware(3, 24_576), &env_map)
            .selected_plan;
        assert_eq!(plan.quantization, "Q4K");
        assert_eq!(plan.paged_attn, "auto");
        assert_eq!(plan.pa_cache_type.as_deref(), Some("turboquant3"));
        assert!(plan.device_layers.is_some(), "{plan:?}");
        let device_layers = plan.device_layers.as_deref().unwrap();
        assert_eq!(sum_device_layers(device_layers), 40);
        assert_eq!(plan.cuda_visible_devices, "0,1,2");
        assert!(device_layers.starts_with("0:"));
        assert!(plan.force_no_mmap);
        assert_eq!(plan.topology, None);
        assert!(!plan.allow_device_layers_with_topology);
        assert!(plan.force_language_model_only);
        assert!(!plan.disable_flash_attn);
        assert!(!plan.isq_singlethread);
    }

    #[test]
    fn qwen35_4b_keeps_serial_immediate_isq() {
        let env_map = BTreeMap::new();
        let bundle = build_manifest_bundle_with_root(
            None,
            "Qwen/Qwen3.5-4B",
            ChatPreset::Quality,
            &hardware(1, 24_576),
            &env_map,
        )
        .expect("qwen3.5-4b manifest bundle should resolve")
        .expect("qwen3.5-4b manifest bundle should exist");
        let plan = bundle.selected_plan;
        assert!(plan_satisfies_context_policy(&plan), "{plan:?}");
        assert_eq!(plan.pa_cache_type.as_deref(), Some("turboquant3"));
        assert!(!plan.disable_flash_attn);
        assert!(plan.isq_singlethread);
    }

    #[test]
    fn qwen35_9b_keeps_serial_immediate_isq() {
        let env_map = BTreeMap::new();
        let bundle = build_manifest_bundle_with_root(
            None,
            "Qwen/Qwen3.5-9B",
            ChatPreset::Quality,
            &hardware(2, 24_576),
            &env_map,
        )
        .expect("qwen3.5-9b manifest bundle should resolve")
        .expect("qwen3.5-9b manifest bundle should exist");
        let plan = bundle.selected_plan;
        assert!(plan_satisfies_context_policy(&plan), "{plan:?}");
        assert_eq!(plan.pa_cache_type.as_deref(), Some("turboquant3"));
        assert!(plan.isq_singlethread);
    }

    #[test]
    fn qwen35_27b_keeps_serial_immediate_isq() {
        let env_map = BTreeMap::new();
        let bundle = build_manifest_bundle_with_root(
            None,
            "Qwen/Qwen3.5-27B",
            ChatPreset::Quality,
            &hardware(3, 24_576),
            &env_map,
        )
        .expect("qwen3.5-27b manifest bundle should resolve")
        .expect("qwen3.5-27b manifest bundle should exist");
        let plan = bundle.selected_plan;
        assert!(plan_satisfies_context_policy(&plan), "{plan:?}");
        assert_eq!(plan.pa_cache_type.as_deref(), Some("turboquant3"));
        assert!(plan.isq_singlethread);
    }

    #[test]
    fn gemma4_dense_keeps_128k_policy_floor() {
        let env_map = BTreeMap::new();
        let plan = build_gemma4_31b_bundle(ChatPreset::Quality, &hardware(3, 24_576), &env_map)
            .selected_plan;
        assert!(plan_satisfies_context_policy(&plan), "{plan:?}");
        assert_eq!(plan.paged_attn, "auto");
        assert_eq!(plan.pa_cache_type.as_deref(), Some("f8e4m3"));
        assert_eq!(plan.moe_experts_backend.as_deref(), Some("fast"));
        assert!(plan.device_layers.is_some(), "{plan:?}");
    }

    #[test]
    fn gemma4_moe_keeps_128k_policy_floor() {
        let env_map = BTreeMap::new();
        let plan =
            build_gemma4_26b_a4b_bundle(ChatPreset::Performance, &hardware(3, 24_576), &env_map)
                .selected_plan;
        assert!(plan_satisfies_context_policy(&plan), "{plan:?}");
        assert_eq!(plan.paged_attn, "auto");
        assert_eq!(plan.pa_cache_type.as_deref(), Some("f8e4m3"));
        assert_eq!(plan.moe_experts_backend.as_deref(), Some("fast"));
        assert!(plan.device_layers.is_some(), "{plan:?}");
    }

    #[test]
    fn tiny_gemma4_models_use_conservative_no_mmap_startup() {
        let env_map = BTreeMap::new();
        let e2b = build_manifest_bundle_with_root(
            None,
            "google/gemma-4-E2B-it",
            ChatPreset::Quality,
            &hardware(1, 24_576),
            &env_map,
        )
        .expect("gemma4 e2b manifest bundle should resolve")
        .expect("gemma4 e2b manifest bundle should exist")
        .selected_plan;
        let e4b = build_manifest_bundle_with_root(
            None,
            "google/gemma-4-E4B-it",
            ChatPreset::Quality,
            &hardware(1, 24_576),
            &env_map,
        )
        .expect("gemma4 e4b manifest bundle should resolve")
        .expect("gemma4 e4b manifest bundle should exist")
        .selected_plan;
        assert!(e2b.force_no_mmap, "{e2b:?}");
        assert!(e4b.force_no_mmap, "{e4b:?}");
        assert_eq!(e2b.quantization, "Q4K");
        assert_eq!(e4b.quantization, "Q4K");
        assert!(e2b.require_prebuilt_uqff_for_chat_start, "{e2b:?}");
        assert!(e4b.require_prebuilt_uqff_for_chat_start, "{e4b:?}");
        assert!(e2b.isq_singlethread, "{e2b:?}");
        assert!(e4b.isq_singlethread, "{e4b:?}");

        let e2b_four_gpu = build_manifest_bundle_with_root(
            None,
            "google/gemma-4-E2B-it",
            ChatPreset::Quality,
            &hardware(4, 20_480),
            &env_map,
        )
        .expect("gemma4 e2b four-gpu manifest bundle should resolve")
        .expect("gemma4 e2b four-gpu manifest bundle should exist")
        .selected_plan;
        let e4b_four_gpu = build_manifest_bundle_with_root(
            None,
            "google/gemma-4-E4B-it",
            ChatPreset::Quality,
            &hardware(4, 20_480),
            &env_map,
        )
        .expect("gemma4 e4b four-gpu manifest bundle should resolve")
        .expect("gemma4 e4b four-gpu manifest bundle should exist")
        .selected_plan;
        assert_eq!(e2b_four_gpu.cuda_visible_devices, "0");
        assert_eq!(e4b_four_gpu.cuda_visible_devices, "0");
        assert_eq!(e2b_four_gpu.device_layers.as_deref(), Some("0:35"));
        assert_eq!(e4b_four_gpu.device_layers.as_deref(), Some("0:42"));
    }

    #[test]
    fn qwen35_moe_keeps_dynamic_multi_gpu_planning_on_four_gpu_hosts() {
        let env_map = BTreeMap::new();
        let plan = build_qwen35_35b_a3b_bundle(ChatPreset::Quality, &hardware(4, 24_576), &env_map)
            .selected_plan;
        assert!(matches!(plan.quantization.as_str(), "Q6K" | "Q5K" | "Q4K"));
        assert_eq!(plan.paged_attn, "auto");
        assert_eq!(plan.pa_cache_type.as_deref(), Some("turboquant3"));
        assert_eq!(plan.cuda_visible_devices, "0,1,2,3");
        assert!(plan.device_layers.is_some(), "{plan:?}");
        assert_eq!(
            sum_device_layers(plan.device_layers.as_deref().unwrap()),
            40
        );
        assert!(plan.device_layers.as_deref().unwrap().starts_with("0:"));
        assert!(plan.force_no_mmap);
        assert_eq!(plan.topology, None);
        assert!(!plan.allow_device_layers_with_topology);
        assert_eq!(plan.nm_device_ordinal, Some(0));
        assert_eq!(plan.base_device_ordinal, Some(0));
        assert_eq!(plan.moe_experts_backend, None);
        assert!(plan.force_language_model_only);
        assert!(!plan.disable_flash_attn);
        assert!(plan.isq_singlethread);
    }

    #[test]
    fn nemotron_uses_conservative_text_runtime_constraints() {
        let env_map = BTreeMap::new();
        let plan =
            build_nemotron_cascade_bundle(ChatPreset::Quality, &hardware(3, 24_576), &env_map)
                .selected_plan;
        assert!(matches!(plan.quantization.as_str(), "Q6K" | "Q5K" | "Q4K"));
        assert_eq!(plan.paged_attn, "auto");
        assert_eq!(plan.pa_cache_type.as_deref(), Some("turboquant3"));
        assert!(!plan.force_no_mmap);
        assert!(!plan.force_language_model_only);
        assert!(plan.disable_flash_attn);
        assert!(plan.isq_singlethread);
        assert!(plan.device_layers.is_some(), "{plan:?}");
        assert_eq!(
            sum_device_layers(plan.device_layers.as_deref().unwrap()),
            52
        );
    }

    #[test]
    fn glm_keeps_public_runtime_constraints() {
        let env_map = BTreeMap::new();
        let plan = build_glm47_flash_bundle(ChatPreset::Quality, &hardware(3, 24_576), &env_map)
            .selected_plan;
        assert!(matches!(plan.quantization.as_str(), "Q6K" | "Q5K" | "Q4K"));
        assert_eq!(plan.paged_attn, "auto");
        assert_eq!(plan.pa_cache_type.as_deref(), Some("turboquant3"));
        assert_eq!(plan.cuda_visible_devices, "0,1,2");
        assert_eq!(
            sum_device_layers(plan.device_layers.as_deref().unwrap()),
            47
        );
        assert!(plan.device_layers.as_deref().unwrap().starts_with("0:"));
        assert!(plan.force_no_mmap);
        assert!(plan.disable_flash_attn);
        assert!(plan.isq_singlethread);
        assert_eq!(plan.isq_cpu_threads, None);
    }

    #[test]
    fn gemma4_26b_performance_prefers_fast_device_layers_on_four_20gb_gpus() {
        let env_map = BTreeMap::new();
        let plan =
            build_gemma4_26b_a4b_bundle(ChatPreset::Performance, &hardware(4, 20_480), &env_map)
                .selected_plan;
        assert!(plan.disable_nccl, "{plan:?}");
        assert_eq!(plan.tensor_parallel_backend, None);
        assert_eq!(plan.pa_cache_type.as_deref(), Some("f8e4m3"));
        assert_eq!(plan.moe_experts_backend.as_deref(), Some("fast"));
        assert!(plan.device_layers.is_some(), "{plan:?}");
        assert_eq!(
            sum_device_layers(plan.device_layers.as_deref().unwrap()),
            30
        );
    }

    #[test]
    fn gemma4_31b_performance_prefers_device_layers_on_four_20gb_gpus() {
        let env_map = BTreeMap::new();
        let plan = build_gemma4_31b_bundle(ChatPreset::Performance, &hardware(4, 20_480), &env_map)
            .selected_plan;
        assert!(plan.disable_nccl, "{plan:?}");
        assert_eq!(plan.tensor_parallel_backend, None);
        assert!(plan.device_layers.is_some(), "{plan:?}");
        assert_eq!(
            sum_device_layers(plan.device_layers.as_deref().unwrap()),
            60
        );
        assert_eq!(plan.pa_cache_type.as_deref(), Some("f8e4m3"));
        assert_eq!(plan.pa_context_len, Some(131_072));
        assert_eq!(plan.moe_experts_backend.as_deref(), Some("fast"));
        assert!(plan.isq_singlethread);
    }

    #[test]
    fn checked_in_runtime_manifests_match_generated_runtime_defaults() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let cases = [
            (
                "contracts/models/runtime_manifests/gpt_oss_20b.json",
                default_gpt_oss_manifest(),
            ),
            (
                "contracts/models/runtime_manifests/qwen3_5_4b.json",
                default_qwen35_4b_manifest(),
            ),
            (
                "contracts/models/runtime_manifests/qwen3_5_9b.json",
                default_qwen35_9b_manifest(),
            ),
            (
                "contracts/models/runtime_manifests/qwen3_5_27b.json",
                default_qwen35_27b_manifest(),
            ),
            (
                "contracts/models/runtime_manifests/qwen3_5_35b_a3b.json",
                default_qwen35_35b_a3b_manifest(),
            ),
            (
                "contracts/models/runtime_manifests/gemma_4_26b_a4b_it.json",
                default_gemma4_26b_a4b_manifest(),
            ),
            (
                "contracts/models/runtime_manifests/gemma_4_31b_it.json",
                default_gemma4_31b_manifest(),
            ),
            (
                "contracts/models/runtime_manifests/glm_4_7_flash.json",
                default_glm47_flash_manifest(),
            ),
        ];
        for (rel_path, generated) in cases {
            let raw = std::fs::read(root.join(rel_path)).unwrap_or_else(|err| {
                panic!("failed to read {rel_path}: {err}");
            });
            let checked_in: model_manifest::RuntimeModelManifest = serde_json::from_slice(&raw)
                .unwrap_or_else(|err| {
                    panic!("failed to parse {rel_path}: {err}");
                });
            assert_eq!(
                checked_in.sizing.context_cap, generated.sizing.context_cap,
                "context_cap drifted in {rel_path}"
            );
            assert_eq!(
                checked_in.runtime_defaults.paged_attn, generated.runtime_defaults.paged_attn,
                "paged_attn drifted in {rel_path}"
            );
            assert_eq!(
                checked_in.runtime_defaults.pa_cache_type, generated.runtime_defaults.pa_cache_type,
                "pa_cache_type drifted in {rel_path}"
            );
            assert_eq!(
                checked_in.quality, generated.quality,
                "quality profile drifted in {rel_path}"
            );
            assert_eq!(
                checked_in.performance, generated.performance,
                "performance profile drifted in {rel_path}"
            );
            assert_eq!(
                checked_in.runtime_defaults.pa_memory_fraction,
                generated.runtime_defaults.pa_memory_fraction,
                "pa_memory_fraction drifted in {rel_path}"
            );
            assert_eq!(
                checked_in.runtime_defaults.pa_context_len,
                generated.runtime_defaults.pa_context_len,
                "pa_context_len drifted in {rel_path}"
            );
            assert_eq!(
                checked_in.runtime_defaults.isq_singlethread,
                generated.runtime_defaults.isq_singlethread,
                "isq_singlethread drifted in {rel_path}"
            );
            assert_eq!(
                checked_in.launch_contract, generated.launch_contract,
                "launch_contract drifted in {rel_path}"
            );
            assert_eq!(
                checked_in.planner_hints, generated.planner_hints,
                "planner_hints drifted in {rel_path}"
            );
        }
    }

    #[test]
    fn runtime_isq_models_keep_serial_peak_averse_load_policy() {
        let manifests = [
            default_qwen35_4b_manifest(),
            default_qwen35_9b_manifest(),
            default_qwen35_27b_manifest(),
            default_qwen35_35b_a3b_manifest(),
            default_gemma4_26b_a4b_manifest(),
            default_gemma4_31b_manifest(),
            default_nemotron_manifest(),
            default_glm47_flash_manifest(),
        ];
        for manifest in manifests {
            assert!(
                manifest.runtime_defaults.isq_singlethread,
                "runtime ISQ load policy drifted for {}",
                manifest.model
            );
        }
    }

    #[test]
    fn qwen4b_presets_are_not_identical() {
        let env_map = BTreeMap::new();
        let bundle = build_qwen35_4b_bundle(ChatPreset::Quality, &hardware(3, 24_576), &env_map);
        let quality = bundle
            .plans
            .iter()
            .find(|plan| plan.preset == ChatPreset::Quality)
            .unwrap();
        let performance = bundle
            .plans
            .iter()
            .find(|plan| plan.preset == ChatPreset::Performance)
            .unwrap();
        let quant_rank = |value: &str| match value {
            "Q6K" => 3,
            "Q5K" => 2,
            "Q4K" => 1,
            _ => 0,
        };
        assert_eq!(quality.max_seq_len, MIN_POLICY_CONTEXT);
        assert_eq!(performance.max_seq_len, MIN_POLICY_CONTEXT);
        assert_eq!(quality.max_seqs, 1);
        assert_eq!(performance.max_seqs, 1);
        assert!(quant_rank(&quality.quantization) >= quant_rank(&performance.quantization));
        assert_eq!(performance.quantization, "Q4K");
        assert_eq!(
            sum_device_layers(quality.device_layers.as_deref().unwrap()),
            32
        );
    }

    #[test]
    fn qwen4b_contract_keeps_performance_faster_than_quality_on_three_gpu_hosts() {
        let env_map = chat_only_env_map();
        let bundle = build_qwen35_4b_bundle(ChatPreset::Quality, &hardware(3, 24_576), &env_map);
        let quality = bundle
            .plans
            .iter()
            .find(|plan| plan.preset == ChatPreset::Quality)
            .unwrap();
        let performance = bundle
            .plans
            .iter()
            .find(|plan| plan.preset == ChatPreset::Performance)
            .unwrap();
        assert!(
            performance.expected_tok_s > quality.expected_tok_s,
            "{} quality={} performance={}",
            bundle.model,
            quality.expected_tok_s,
            performance.expected_tok_s
        );
    }

    #[test]
    fn qwen_family_quality_resolves_balanced_variant_on_supported_three_gpu_hosts() {
        let env_map = BTreeMap::new();
        let bundle = best_bundle_for_family(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            engine::ChatModelFamily::Qwen35,
            ChatPreset::Quality,
            Some(&hardware(3, 24_576)),
            &env_map,
        )
        .unwrap()
        .expect("expected qwen family bundle");
        assert_eq!(bundle.model, "Qwen/Qwen3.5-35B-A3B");
    }

    #[test]
    fn qwen_family_quality_falls_back_to_small_dense_variant_on_single_24gb_hosts() {
        let env_map = BTreeMap::new();
        let bundle = best_bundle_for_family(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            engine::ChatModelFamily::Qwen35,
            ChatPreset::Quality,
            Some(&hardware(1, 24_576)),
            &env_map,
        )
        .unwrap()
        .expect("expected qwen family bundle");
        assert_eq!(bundle.model, "Qwen/Qwen3.5-4B");
    }

    #[test]
    fn qwen_family_performance_keeps_the_same_balanced_variant_on_supported_three_gpu_hosts() {
        let env_map = BTreeMap::new();
        let bundle = best_bundle_for_family(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            engine::ChatModelFamily::Qwen35,
            ChatPreset::Performance,
            Some(&hardware(3, 24_576)),
            &env_map,
        )
        .unwrap()
        .expect("expected qwen family bundle");
        assert_eq!(bundle.model, "Qwen/Qwen3.5-35B-A3B");
    }

    #[test]
    fn qwen_family_resolution_stays_on_the_same_model_across_presets() {
        let env_map = BTreeMap::new();
        let hardware = hardware(3, 24_576);
        let quality = best_bundle_for_family(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            engine::ChatModelFamily::Qwen35,
            ChatPreset::Quality,
            Some(&hardware),
            &env_map,
        )
        .unwrap()
        .expect("expected qwen family quality bundle");
        let performance = best_bundle_for_family(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            engine::ChatModelFamily::Qwen35,
            ChatPreset::Performance,
            Some(&hardware),
            &env_map,
        )
        .unwrap()
        .expect("expected qwen family performance bundle");
        assert_eq!(quality.model, performance.model);
        assert_eq!(quality.model, "Qwen/Qwen3.5-35B-A3B");
    }

    #[test]
    fn qwen_family_quality_falls_back_to_supported_variant_on_three_gpu_20gb_hosts() {
        let env_map = BTreeMap::new();
        let bundle = best_bundle_for_family(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            engine::ChatModelFamily::Qwen35,
            ChatPreset::Quality,
            Some(&hardware(3, 20_480)),
            &env_map,
        )
        .unwrap()
        .expect("expected qwen family bundle");
        assert_eq!(bundle.model, "Qwen/Qwen3.5-4B");
    }

    #[test]
    fn qwen_family_performance_falls_back_to_supported_variant_on_three_gpu_20gb_hosts() {
        let env_map = BTreeMap::new();
        let bundle = best_bundle_for_family(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            engine::ChatModelFamily::Qwen35,
            ChatPreset::Performance,
            Some(&hardware(3, 20_480)),
            &env_map,
        )
        .unwrap()
        .expect("expected qwen family bundle");
        assert_eq!(bundle.model, "Qwen/Qwen3.5-4B");
    }

    #[test]
    fn planner_uses_strong_gpu_subset_on_mixed_hosts() {
        let env_map = BTreeMap::new();
        let bundle = best_bundle_for_family(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            engine::ChatModelFamily::Qwen35,
            ChatPreset::Performance,
            Some(&hardware_totals(&[24_576, 24_576, 24_576, 12_288])),
            &env_map,
        )
        .unwrap()
        .expect("expected qwen family bundle");
        assert_eq!(bundle.model, "Qwen/Qwen3.5-35B-A3B");
        assert_eq!(bundle.selected_plan.cuda_visible_devices, "0,1,2");
    }

    #[test]
    fn local_chat_family_choices_follow_dynamic_feasibility() {
        let root = temp_root("family-choices-dynamic");
        write_test_gpu_totals(&root, "0:24576");
        let env_map = BTreeMap::new();
        let families = local_chat_family_choices(&root, &env_map);
        let _ = std::fs::remove_dir_all(&root);

        assert!(families.contains(&"Qwen 3.5"));
        assert!(families.contains(&"GPT-OSS"));
        assert!(!families.contains(&"Gemma 4"));
        assert!(!families.contains(&"Nemotron Cascade"));
        assert!(!families.contains(&"GLM 4.7 Flash"));
    }

    #[test]
    fn local_chat_family_choices_allow_single_gpu_profiles_on_larger_hosts() {
        let root = temp_root("family-choices-single-gpu");
        write_test_gpu_totals(&root, "0:24576,1:24576,2:24576,3:24576");
        let env_map = BTreeMap::new();
        let families = local_chat_family_choices(&root, &env_map);
        let _ = std::fs::remove_dir_all(&root);

        assert!(families.contains(&"GPT-OSS"));
        assert!(families.contains(&"Qwen 3.5"));
    }

    #[test]
    fn direct_model_runtime_apply_allows_dynamic_3x20_host() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let unique = std::env::temp_dir().join(format!(
            "ctox_runtime_plan_dynamic_3x20_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(unique.join("runtime")).unwrap();
        write_test_gpu_totals(&unique, "0:20470;1:20470;2:20470");
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        env_map.insert(
            "CTOX_CHAT_MODEL_BASE".to_string(),
            "openai/gpt-oss-20b".to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_MODEL".to_string(),
            "openai/gpt-oss-20b".to_string(),
        );
        env_map.insert(
            "CTOX_ACTIVE_MODEL".to_string(),
            "openai/gpt-oss-20b".to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_LOCAL_PRESET".to_string(),
            ChatPreset::Quality.label().to_string(),
        );

        let result = apply_chat_runtime_plan(&unique, &mut env_map)
            .unwrap()
            .expect("expected runtime plan");
        let _ = std::fs::remove_dir_all(&unique);

        assert_eq!(result.model, "openai/gpt-oss-20b");
        assert!(plan_satisfies_context_policy(&result), "{result:?}");
        assert_eq!(result.max_seq_len, MIN_POLICY_CONTEXT);
    }

    #[test]
    fn direct_model_runtime_apply_keeps_explicit_gemma_model() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let unique = std::env::temp_dir().join(format!(
            "ctox_runtime_plan_direct_model_gemma_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(unique.join("runtime")).unwrap();
        write_test_gpu_totals(&unique, "0:20470;1:20470;2:20470");
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        env_map.insert(
            "CTOX_CHAT_MODEL_BASE".to_string(),
            "google/gemma-4-26B-A4B-it".to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_MODEL".to_string(),
            "google/gemma-4-26B-A4B-it".to_string(),
        );
        env_map.insert(
            "CTOX_ACTIVE_MODEL".to_string(),
            "google/gemma-4-26B-A4B-it".to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_LOCAL_PRESET".to_string(),
            ChatPreset::Quality.label().to_string(),
        );

        let result = apply_chat_runtime_plan(&unique, &mut env_map)
            .unwrap()
            .expect("expected runtime plan");
        let _ = std::fs::remove_dir_all(&unique);

        assert_eq!(result.model, "google/gemma-4-26B-A4B-it");
        assert_eq!(
            env_map.get("CTOX_CHAT_MODEL").map(String::as_str),
            Some("google/gemma-4-26B-A4B-it")
        );
    }

    #[test]
    fn explicit_family_runtime_apply_can_still_resolve_family_variant() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let unique = std::env::temp_dir().join(format!(
            "ctox_runtime_plan_explicit_family_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(unique.join("runtime")).unwrap();
        write_test_gpu_totals(&unique, "0:20470;1:20470;2:20470");
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        env_map.insert(
            "CTOX_CHAT_MODEL_FAMILY".to_string(),
            engine::ChatModelFamily::Gemma4.label().to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_MODEL_BASE".to_string(),
            "google/gemma-4-26B-A4B-it".to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_MODEL".to_string(),
            "google/gemma-4-26B-A4B-it".to_string(),
        );
        env_map.insert(
            "CTOX_ACTIVE_MODEL".to_string(),
            "google/gemma-4-26B-A4B-it".to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_LOCAL_PRESET".to_string(),
            ChatPreset::Quality.label().to_string(),
        );

        let result = apply_chat_runtime_plan(&unique, &mut env_map)
            .unwrap()
            .expect("expected runtime plan");
        let _ = std::fs::remove_dir_all(&unique);

        assert_eq!(result.model, "google/gemma-4-31B-it");
    }

    #[test]
    fn explicit_model_overrides_stale_family_selection() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let unique = std::env::temp_dir().join(format!(
            "ctox_runtime_plan_explicit_model_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(unique.join("runtime")).unwrap();
        write_test_gpu_totals(&unique, "0:20470;1:20470;2:20470;3:20470");
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        env_map.insert(
            "CTOX_CHAT_MODEL_FAMILY".to_string(),
            engine::ChatModelFamily::GptOss.label().to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_MODEL_BASE".to_string(),
            "Qwen/Qwen3.5-4B".to_string(),
        );
        env_map.insert("CTOX_CHAT_MODEL".to_string(), "Qwen/Qwen3.5-4B".to_string());
        env_map.insert(
            "CTOX_ACTIVE_MODEL".to_string(),
            "Qwen/Qwen3.5-4B".to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_LOCAL_PRESET".to_string(),
            ChatPreset::Quality.label().to_string(),
        );

        let result = apply_chat_runtime_plan(&unique, &mut env_map)
            .unwrap()
            .expect("expected runtime plan");
        let _ = std::fs::remove_dir_all(&unique);

        assert_eq!(result.model, "Qwen/Qwen3.5-4B");
    }

    #[test]
    fn validate_live_gpu_budget_requires_actual_free_memory() {
        let env_map = BTreeMap::new();
        let plan = build_qwen35_4b_bundle(ChatPreset::Quality, &hardware(1, 24_576), &env_map)
            .selected_plan;
        let allocation = plan
            .gpu_allocations
            .iter()
            .find(|gpu| gpu.chat_enabled)
            .expect("chat allocation");
        let required_free_mb = allocation
            .desktop_reserve_mb
            .saturating_add(allocation.aux_reserve_mb)
            .saturating_add(allocation.chat_budget_mb);
        let snapshot = resource_state::ResourceSnapshot {
            source: "test-snapshot".to_string(),
            gpus: vec![resource_state::GpuLiveState {
                index: allocation.gpu_index,
                uuid: None,
                name: allocation.name.clone(),
                total_mb: allocation.total_mb,
                used_mb: allocation
                    .total_mb
                    .saturating_sub(required_free_mb.saturating_sub(1)),
                free_mb: required_free_mb.saturating_sub(1),
            }],
        };
        let err = validate_live_gpu_budget(&plan, &snapshot).unwrap_err();
        assert!(err.to_string().contains("required="), "{err:#}");
    }

    #[test]
    fn apply_chat_runtime_plan_uses_host_capacity_not_live_free_vram() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let unique = std::env::temp_dir().join(format!(
            "ctox_runtime_plan_uses_live_free_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(unique.join("runtime")).unwrap();
        write_test_gpu_totals(&unique, "0:24576;1:24576;2:24576");
        let previous_snapshot = std::env::var("CTOX_RESOURCE_SNAPSHOT_JSON").ok();
        std::env::set_var(
            "CTOX_RESOURCE_SNAPSHOT_JSON",
            r#"{"source":"test-live-free","gpus":[{"index":0,"uuid":null,"name":"GPU0","total_mb":24576,"used_mb":22528,"free_mb":2048},{"index":1,"uuid":null,"name":"GPU1","total_mb":24576,"used_mb":0,"free_mb":24576},{"index":2,"uuid":null,"name":"GPU2","total_mb":24576,"used_mb":0,"free_mb":24576}]}"#,
        );
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        env_map.insert(
            "CTOX_CHAT_MODEL_BASE".to_string(),
            "openai/gpt-oss-20b".to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_MODEL".to_string(),
            "openai/gpt-oss-20b".to_string(),
        );
        env_map.insert(
            "CTOX_ACTIVE_MODEL".to_string(),
            "openai/gpt-oss-20b".to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_LOCAL_PRESET".to_string(),
            ChatPreset::Quality.label().to_string(),
        );

        let plan = apply_chat_runtime_plan(&unique, &mut env_map)
            .expect("chat runtime plan should resolve from host capacity")
            .expect("chat runtime plan should be present");
        if let Some(previous) = previous_snapshot {
            std::env::set_var("CTOX_RESOURCE_SNAPSHOT_JSON", previous);
        } else {
            std::env::remove_var("CTOX_RESOURCE_SNAPSHOT_JSON");
        }
        let _ = std::fs::remove_dir_all(&unique);

        assert!(plan_satisfies_context_policy(&plan), "{plan:#?}");
        assert_eq!(plan.max_seq_len, MIN_POLICY_CONTEXT);
    }

    #[test]
    fn gpt_oss_three_gpu_20gb_performance_falls_back_to_device_layers() {
        let env_map = BTreeMap::new();
        let plan =
            build_gpt_oss_20b_bundle(ChatPreset::Performance, &hardware(3, 20_480), &env_map)
                .selected_plan;
        assert!(plan_satisfies_context_policy(&plan), "{plan:?}");
        assert!(plan.disable_nccl, "{plan:?}");
        assert_eq!(plan.tensor_parallel_backend, None);
        assert_eq!(plan.mn_local_world_size, None);
        assert_eq!(plan.cuda_visible_devices, "0,1");
        assert_eq!(plan.nm_device_ordinal, Some(0));
        assert_eq!(plan.base_device_ordinal, Some(0));
        assert!(plan.device_layers.is_some(), "{plan:?}");
        assert!(
            plan.gpu_allocations
                .iter()
                .filter(|allocation| allocation.chat_enabled)
                .count()
                == 2,
            "{plan:#?}"
        );
        assert!(
            plan.gpu_allocations
                .iter()
                .filter(|allocation| allocation.chat_enabled)
                .all(|allocation| allocation.aux_reserve_mb == 0),
            "{plan:#?}"
        );
        assert_eq!(
            plan.theoretical_breakdown.contract_source,
            "explicit manifest resource contract"
        );
    }

    #[test]
    fn apply_chat_runtime_plan_falls_back_to_device_layers_on_three_a4500_hosts() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let unique = std::env::temp_dir().join(format!(
            "ctox_runtime_plan_a4500_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(unique.join("runtime")).unwrap();
        write_test_gpu_totals(&unique, "0:20470;1:20470;2:20470");

        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        env_map.insert(
            "CTOX_CHAT_MODEL_BASE".to_string(),
            "openai/gpt-oss-20b".to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_MODEL".to_string(),
            "openai/gpt-oss-20b".to_string(),
        );
        env_map.insert(
            "CTOX_ACTIVE_MODEL".to_string(),
            "openai/gpt-oss-20b".to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_LOCAL_PRESET".to_string(),
            ChatPreset::Performance.label().to_string(),
        );

        let plan = apply_chat_runtime_plan(&unique, &mut env_map)
            .expect("chat runtime plan should resolve on a 3xA4500 host")
            .expect("chat runtime plan should be present");

        let _ = std::fs::remove_dir_all(&unique);

        assert!(plan_satisfies_context_policy(&plan), "{plan:#?}");
        assert!(plan.disable_nccl, "{plan:#?}");
        assert_eq!(plan.tensor_parallel_backend, None);
        assert_eq!(plan.mn_local_world_size, None);
        assert_eq!(plan.cuda_visible_devices, "0,1");
        assert!(plan.device_layers.is_some(), "{plan:#?}");
    }

    #[test]
    fn gpt_oss_explicit_128k_contract_math_is_consistent() {
        let env_map = BTreeMap::new();
        let plan = build_gpt_oss_20b_bundle(ChatPreset::Quality, &hardware(3, 20_480), &env_map)
            .selected_plan;
        let breakdown = &plan.theoretical_breakdown;

        assert_eq!(plan.model, "openai/gpt-oss-20b");
        assert_eq!(plan.max_seq_len, 131_072);
        assert_eq!(breakdown.weight_residency_mb, 15_784);
        assert_eq!(breakdown.kv_cache_mb, 15_488);
        assert_eq!(breakdown.kv_budget_fraction_milli, 800);
        assert_eq!(breakdown.fixed_runtime_base_overhead_mb, 1_440);
        assert_eq!(breakdown.backend_runtime_overhead_mb, 256);
        assert_eq!(breakdown.load_peak_overhead_mb, 512);
        assert_eq!(breakdown.activation_overhead_mb, 0);
        assert_eq!(breakdown.required_effective_total_budget_mb, 37_352);
        assert_eq!(breakdown.required_total_mb, 37_352);
        assert_eq!(plan.cuda_visible_devices, "0,1,2");
    }

    #[test]
    fn explicit_qwen4b_resource_contract_does_not_scale_kv_or_activation_with_weight_quantization()
    {
        let env_map = BTreeMap::new();
        let plan = build_qwen35_4b_bundle(ChatPreset::Quality, &hardware(3, 24_576), &env_map)
            .selected_plan;
        let breakdown = &plan.theoretical_breakdown;

        assert_eq!(plan.quantization, "Q6K");
        assert_eq!(plan.max_seq_len, 131_072);
        assert_eq!(breakdown.kv_cache_mb, 5_504);
        assert_eq!(breakdown.activation_overhead_mb, 10_368);
        assert_eq!(breakdown.load_peak_overhead_mb, 476);
        assert_eq!(breakdown.required_effective_total_budget_mb, 14_795);
        assert_eq!(breakdown.required_total_mb, 25_163);
    }

    #[test]
    fn nemotron_presets_prioritize_quantization_context_and_parallelism() {
        let env_map = BTreeMap::new();
        let bundle =
            build_nemotron_cascade_bundle(ChatPreset::Quality, &hardware(3, 24_576), &env_map);
        let quality = bundle
            .plans
            .iter()
            .find(|plan| plan.preset == ChatPreset::Quality)
            .unwrap();
        let performance = bundle
            .plans
            .iter()
            .find(|plan| plan.preset == ChatPreset::Performance)
            .unwrap();
        assert!(quality.max_seq_len >= MIN_POLICY_CONTEXT);
        assert!(performance.max_seqs >= 2);
        assert_eq!(performance.quantization, "Q4K");
    }

    #[test]
    fn gpt_oss_is_allowed_by_the_128k_policy() {
        let env_map = BTreeMap::new();
        assert!(local_model_satisfies_context_policy(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            "openai/gpt-oss-20b",
            &env_map
        ));
    }

    #[test]
    fn qwen4b_is_allowed_by_the_128k_policy() {
        let env_map = BTreeMap::new();
        assert!(local_model_satisfies_context_policy(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            "Qwen/Qwen3.5-4B",
            &env_map
        ));
    }

    #[test]
    fn compaction_policy_differs_by_preset() {
        let quality = compaction_policy_for_preset(ChatPreset::Quality);
        let performance = compaction_policy_for_preset(ChatPreset::Performance);

        assert_eq!(quality.threshold_percent, 75);
        assert_eq!(quality.min_tokens, 12_288);
        assert_eq!(performance.threshold_percent, 70);
        assert_eq!(performance.min_tokens, 8_192);
    }

    #[test]
    fn runtime_plan_keeps_manifest_cache_type_even_when_env_requests_override() {
        let mut env_map = BTreeMap::new();
        env_map.insert(
            "CTOX_ENGINE_PA_CACHE_TYPE_OVERRIDE".to_string(),
            "f8e4m3".to_string(),
        );
        let plan = build_qwen35_27b_bundle(ChatPreset::Quality, &hardware(3, 24_576), &env_map)
            .selected_plan;
        assert_eq!(plan.pa_cache_type.as_deref(), Some("turboquant3"));
    }

    #[test]
    fn gpt_oss_runtime_plan_keeps_manifest_cache_type_even_when_env_requests_override() {
        let mut env_map = BTreeMap::new();
        env_map.insert(
            "CTOX_ENGINE_PA_CACHE_TYPE_OVERRIDE".to_string(),
            "f8e4m3".to_string(),
        );
        let plan = build_gpt_oss_20b_bundle(ChatPreset::Quality, &hardware(3, 24_576), &env_map)
            .selected_plan;
        assert_eq!(plan.paged_attn, "auto");
        assert_eq!(plan.pa_cache_type.as_deref(), Some("turboquant3"));
    }

    #[test]
    fn glm_runtime_plan_keeps_manifest_cache_type_even_when_env_requests_override() {
        let mut env_map = BTreeMap::new();
        env_map.insert(
            "CTOX_ENGINE_PA_CACHE_TYPE_OVERRIDE".to_string(),
            "f8e4m3".to_string(),
        );
        let plan = build_glm47_flash_bundle(ChatPreset::Quality, &hardware(3, 24_576), &env_map)
            .selected_plan;
        assert_eq!(plan.paged_attn, "auto");
        assert_eq!(plan.pa_cache_type.as_deref(), Some("turboquant3"));
    }

    #[test]
    fn nemotron_turboquant_runtime_plan_is_enabled_by_default() {
        let env_map = BTreeMap::new();
        let plan =
            build_nemotron_cascade_bundle(ChatPreset::Quality, &hardware(3, 24_576), &env_map)
                .selected_plan;
        assert_eq!(plan.paged_attn, "auto");
        assert_eq!(plan.pa_cache_type.as_deref(), Some("turboquant3"));
    }

    #[test]
    fn nemotron_is_allowed_by_its_model_contract_floor() {
        let env_map = BTreeMap::new();
        assert!(local_model_satisfies_context_policy(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            "nvidia/Nemotron-Cascade-2-30B-A3B",
            &env_map
        ));
    }

    #[test]
    fn glm_is_allowed_by_its_model_contract_floor() {
        let env_map = BTreeMap::new();
        assert!(local_model_satisfies_context_policy(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            "zai-org/GLM-4.7-Flash",
            &env_map
        ));
    }

    #[test]
    fn direct_model_runtime_apply_allows_explicit_nemotron_on_4x20gb() {
        let unique = temp_root("explicit-nemotron-4x20");
        write_test_gpu_totals(&unique, "0:20470;1:20470;2:20470;3:20470");
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        env_map.insert(
            "CTOX_CHAT_MODEL_BASE".to_string(),
            "nvidia/Nemotron-Cascade-2-30B-A3B".to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_MODEL".to_string(),
            "nvidia/Nemotron-Cascade-2-30B-A3B".to_string(),
        );
        env_map.insert(
            "CTOX_ACTIVE_MODEL".to_string(),
            "nvidia/Nemotron-Cascade-2-30B-A3B".to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_LOCAL_PRESET".to_string(),
            ChatPreset::Quality.label().to_string(),
        );

        let result = apply_chat_runtime_plan(&unique, &mut env_map)
            .unwrap()
            .expect("expected runtime plan");
        let _ = std::fs::remove_dir_all(&unique);

        assert_eq!(result.model, "nvidia/Nemotron-Cascade-2-30B-A3B");
        assert!(result.max_seq_len >= 65_536, "{result:?}");
    }

    #[test]
    fn apply_chat_runtime_plan_does_not_reuse_stale_nemotron_plan_when_manifest_changes() {
        let unique = temp_root("stale-nemotron-plan");
        write_test_gpu_totals(&unique, "0:20470;1:20470;2:20470;3:20470");

        let stale_plan = ChatRuntimePlan {
            model: "nvidia/Nemotron-Cascade-2-30B-A3B".to_string(),
            preset: ChatPreset::Quality,
            quantization: "Q6K".to_string(),
            runtime_isq: Some("Q6K".to_string()),
            max_seq_len: 262_144,
            compaction_threshold_percent: 75,
            compaction_min_tokens: 12_288,
            min_context_floor_applied: true,
            paged_attn: "auto".to_string(),
            pa_cache_type: Some("turboquant3".to_string()),
            pa_memory_fraction: Some("0.45".to_string()),
            pa_context_len: Some(262_144),
            disable_nccl: true,
            tensor_parallel_backend: None,
            mn_local_world_size: None,
            max_batch_size: 1,
            max_seqs: 1,
            cuda_visible_devices: "0,1,2,3".to_string(),
            device_layers: Some("0:10;1:14;2:14;3:14".to_string()),
            topology: None,
            allow_device_layers_with_topology: false,
            nm_device_ordinal: Some(0),
            base_device_ordinal: Some(0),
            moe_experts_backend: None,
            disable_flash_attn: true,
            force_no_mmap: false,
            force_language_model_only: false,
            require_prebuilt_uqff_for_chat_start: false,
            isq_singlethread: true,
            isq_cpu_threads: None,
            expected_tok_s: 44.24112,
            hardware_fingerprint: "stale-nemotron-test".to_string(),
            theoretical_breakdown: TheoreticalResourceBreakdown {
                contract_source: "stale persisted nemotron test".to_string(),
                effective_total_budget_mb: 0,
                kv_budget_cap_mb: 0,
                kv_budget_fraction_milli: 0,
                weight_residency_mb: 0,
                kv_cache_mb: 0,
                fixed_runtime_base_overhead_mb: 0,
                backend_runtime_overhead_mb: 0,
                activation_overhead_mb: 0,
                load_peak_overhead_mb: 0,
                safety_headroom_mb: 0,
                required_effective_total_budget_mb: 0,
                required_total_mb: 0,
            },
            rationale: vec!["stale persisted nemotron plan".to_string()],
            gpu_allocations: vec![],
        };
        store_persisted_chat_runtime_plan(&unique, Some(&stale_plan)).unwrap();

        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        env_map.insert(
            "CTOX_CHAT_MODEL_BASE".to_string(),
            "nvidia/Nemotron-Cascade-2-30B-A3B".to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_MODEL".to_string(),
            "nvidia/Nemotron-Cascade-2-30B-A3B".to_string(),
        );
        env_map.insert(
            "CTOX_ACTIVE_MODEL".to_string(),
            "nvidia/Nemotron-Cascade-2-30B-A3B".to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_LOCAL_PRESET".to_string(),
            ChatPreset::Quality.label().to_string(),
        );

        let result = apply_chat_runtime_plan(&unique, &mut env_map)
            .unwrap()
            .expect("expected runtime plan");
        let _ = std::fs::remove_dir_all(&unique);

        assert_eq!(result.model, "nvidia/Nemotron-Cascade-2-30B-A3B");
        assert_eq!(result.max_seq_len, 65_536, "{result:?}");
        assert_ne!(result.max_seq_len, stale_plan.max_seq_len);
    }

    #[test]
    fn direct_model_runtime_apply_allows_explicit_glm_on_4x20gb() {
        let unique = temp_root("explicit-glm-4x20");
        write_test_gpu_totals(&unique, "0:20470;1:20470;2:20470;3:20470");
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        env_map.insert(
            "CTOX_CHAT_MODEL_BASE".to_string(),
            "zai-org/GLM-4.7-Flash".to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_MODEL".to_string(),
            "zai-org/GLM-4.7-Flash".to_string(),
        );
        env_map.insert(
            "CTOX_ACTIVE_MODEL".to_string(),
            "zai-org/GLM-4.7-Flash".to_string(),
        );
        env_map.insert(
            "CTOX_CHAT_LOCAL_PRESET".to_string(),
            ChatPreset::Quality.label().to_string(),
        );

        let result = apply_chat_runtime_plan(&unique, &mut env_map)
            .unwrap()
            .expect("expected runtime plan");
        let _ = std::fs::remove_dir_all(&unique);

        assert_eq!(result.model, "zai-org/GLM-4.7-Flash");
        assert_eq!(result.max_seq_len, 65_536);
        assert_eq!(result.quantization, "Q4K");
    }

    #[test]
    fn glm_without_matching_qualification_profile_uses_manifest_candidate_order_on_4x20gb() {
        let env_map = BTreeMap::new();
        let bundle = build_glm47_flash_bundle(ChatPreset::Quality, &hardware(4, 20_480), &env_map);
        let quality = bundle
            .plans
            .iter()
            .find(|plan| plan.preset == ChatPreset::Quality)
            .expect("expected glm quality plan");

        assert_eq!(quality.quantization, "Q4K");
        assert!(
            quality
                .rationale
                .iter()
                .any(|line| line.contains("manifest candidate order")),
            "{quality:#?}"
        );
    }
}
