use anyhow::Context;
use anyhow::Result;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;

use crate::inference::engine;
use crate::inference::resource_state;
use crate::inference::runtime_contract;
use crate::inference::runtime_env;
use crate::inference::runtime_kernel;
use crate::inference::runtime_plan;

#[derive(Debug, Clone)]
pub struct RuntimeWorkloadDescriptor {
    pub role: runtime_contract::BackendRole,
    pub model: String,
    pub port: u16,
    pub health_path: String,
    pub launcher_kind: runtime_kernel::RuntimeLauncherKind,
    pub compute_target: Option<engine::ComputeTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GpuAdmission {
    pub visible_devices: Option<String>,
    pub reserved_mb_by_gpu: BTreeMap<usize, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GpuBlockers {
    owned_pids: Vec<u32>,
    foreign_pids: Vec<u32>,
}

pub fn resolve_gpu_admission(
    root: &Path,
    descriptor: &RuntimeWorkloadDescriptor,
) -> Result<GpuAdmission> {
    let visible_devices = desired_visible_devices(root, descriptor.role, &descriptor.model)?;
    let reserved_mb_by_gpu = reserved_mb_by_gpu(
        root,
        descriptor.role,
        &descriptor.model,
        descriptor.compute_target,
        visible_devices.as_deref(),
    )?;
    Ok(GpuAdmission {
        visible_devices,
        reserved_mb_by_gpu,
    })
}

pub fn sync_workload_runtime_residency(
    root: &Path,
    descriptor: &RuntimeWorkloadDescriptor,
    admission: &GpuAdmission,
    pid: Option<u32>,
    phase: runtime_contract::RuntimeResidencyPhase,
) -> Result<()> {
    let residency = runtime_contract::BackendRuntimeResidency {
        role: descriptor.role,
        phase,
        model: descriptor.model.clone(),
        pid,
        port: Some(descriptor.port),
        health_path: Some(descriptor.health_path.clone()),
        launcher_kind: Some(launcher_kind_label(descriptor.launcher_kind).to_string()),
        compute_target: descriptor
            .compute_target
            .map(|target| target.as_env_value().to_string()),
        visible_devices: admission
            .visible_devices
            .as_deref()
            .map(parse_visible_devices)
            .unwrap_or_default(),
        reserved_mb_by_gpu: admission.reserved_mb_by_gpu.clone(),
        updated_at_epoch_secs: runtime_contract::current_epoch_secs(),
    };
    runtime_contract::sync_backend_runtime_residency(root, residency)
}

pub fn prepare_workload_launch(
    root: &Path,
    descriptor: &RuntimeWorkloadDescriptor,
    admission: &GpuAdmission,
) -> Result<()> {
    let Some(visible_devices) = admission.visible_devices.as_deref() else {
        if descriptor.role == runtime_contract::BackendRole::Chat {
            validate_primary_generation_budget(root)?;
        }
        return Ok(());
    };
    let gpu_indices = parse_visible_devices(visible_devices);
    if gpu_indices.is_empty() {
        if descriptor.role == runtime_contract::BackendRole::Chat {
            validate_primary_generation_budget(root)?;
        }
        return Ok(());
    }
    let owned_pids = managed_runtime_gpu_holder_pids(root, &gpu_indices)?;
    let Some(processes_after_cleanup) = resource_state::inspect_gpu_process_snapshot() else {
        return Ok(());
    };
    let mut remaining = classify_gpu_blockers(
        &processes_after_cleanup,
        &gpu_indices,
        &owned_pids,
        std::process::id(),
    );
    let speech_cohosts =
        compatible_speech_gpu_holders(root, descriptor, admission, &remaining.owned_pids)?;
    remaining
        .owned_pids
        .retain(|pid| !speech_cohosts.contains(pid));
    if !remaining.owned_pids.is_empty() {
        anyhow::bail!(
            "refusing to launch {} backend because target GPUs [{}] are still held by managed runtime pids {:?}",
            descriptor.role.as_str(),
            visible_devices,
            remaining.owned_pids
        );
    }
    if !remaining.foreign_pids.is_empty() {
        let details = processes_after_cleanup
            .iter()
            .filter(|process| remaining.foreign_pids.contains(&process.pid))
            .map(|process| {
                format!(
                    "gpu{} pid={} used={}MB process={} command={}",
                    process.gpu_index,
                    process.pid,
                    process.used_mb,
                    process.process_name,
                    process.command.as_deref().unwrap_or("<unknown>"),
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");
        anyhow::bail!(
            "refusing to launch {} backend because target GPUs [{}] are still occupied: {}",
            descriptor.role.as_str(),
            visible_devices,
            details
        );
    }
    if descriptor.role == runtime_contract::BackendRole::Chat {
        validate_primary_generation_budget(root)?;
    } else {
        validate_auxiliary_gpu_budget(root, descriptor, admission, !speech_cohosts.is_empty())?;
    }
    Ok(())
}

pub fn parse_visible_devices(value: &str) -> Vec<usize> {
    value
        .split(',')
        .filter_map(|chunk| chunk.trim().parse::<usize>().ok())
        .collect()
}

fn desired_visible_devices(
    root: &Path,
    role: runtime_contract::BackendRole,
    request_model: &str,
) -> Result<Option<String>> {
    if let Some(resolved) = runtime_kernel::InferenceRuntimeKernel::resolve(root).ok() {
        if let Some(binding) = resolved_binding_for_role(&resolved, role) {
            if binding.compute_target == Some(engine::ComputeTarget::Gpu) {
                if let Some(visible_devices) = binding
                    .visible_devices
                    .clone()
                    .filter(|value| !value.trim().is_empty())
                {
                    return Ok(Some(visible_devices));
                }
            }
        }
    }
    match role {
        runtime_contract::BackendRole::Chat => runtime_plan::load_persisted_chat_runtime_plan(root)
            .map(|plan| plan.map(|plan| plan.cuda_visible_devices))
            .or_else(|_| {
                Ok(runtime_env::env_or_config(
                    root,
                    "CTOX_ENGINE_CUDA_VISIBLE_DEVICES",
                ))
            }),
        runtime_contract::BackendRole::Embedding => {
            runtime_plan::resolve_auxiliary_visible_devices(
                root,
                engine::AuxiliaryRole::Embedding,
                request_model,
            )
        }
        runtime_contract::BackendRole::Stt => runtime_plan::resolve_auxiliary_visible_devices(
            root,
            engine::AuxiliaryRole::Stt,
            request_model,
        ),
        runtime_contract::BackendRole::Tts => runtime_plan::resolve_auxiliary_visible_devices(
            root,
            engine::AuxiliaryRole::Tts,
            request_model,
        ),
        runtime_contract::BackendRole::Vision => runtime_plan::resolve_auxiliary_visible_devices(
            root,
            engine::AuxiliaryRole::Vision,
            request_model,
        ),
    }
}

fn reserved_mb_by_gpu(
    root: &Path,
    role: runtime_contract::BackendRole,
    request_model: &str,
    compute_target: Option<engine::ComputeTarget>,
    visible_devices: Option<&str>,
) -> Result<BTreeMap<usize, u64>> {
    let mut reserved_mb_by_gpu = BTreeMap::new();
    match role {
        runtime_contract::BackendRole::Chat => {
            if let Some(plan) = runtime_plan::load_persisted_chat_runtime_plan(root)? {
                for allocation in plan
                    .gpu_allocations
                    .into_iter()
                    .filter(|gpu| gpu.chat_enabled)
                {
                    let reserved_mb = allocation
                        .chat_budget_mb
                        .saturating_add(allocation.backend_overhead_mb)
                        .saturating_add(allocation.activation_overhead_mb)
                        .saturating_add(allocation.load_peak_overhead_mb);
                    reserved_mb_by_gpu.insert(allocation.gpu_index, reserved_mb);
                }
            }
        }
        runtime_contract::BackendRole::Embedding
        | runtime_contract::BackendRole::Stt
        | runtime_contract::BackendRole::Tts
        | runtime_contract::BackendRole::Vision => {
            let devices = visible_devices
                .map(parse_visible_devices)
                .unwrap_or_default();
            if compute_target == Some(engine::ComputeTarget::Gpu) && !devices.is_empty() {
                let reserve_mb = runtime_plan::auxiliary_manifest(Some(root), request_model)
                    .map(|manifest| manifest.gpu_reserve_mb)
                    .unwrap_or_else(|| auxiliary_selection(role, request_model).gpu_reserve_mb());
                let shares = even_shares(reserve_mb, devices.len());
                for (index, gpu_index) in devices.iter().enumerate() {
                    reserved_mb_by_gpu.insert(*gpu_index, *shares.get(index).unwrap_or(&0));
                }
            }
        }
    }
    Ok(reserved_mb_by_gpu)
}

fn even_shares(total: u64, count: usize) -> Vec<u64> {
    if count == 0 {
        return Vec::new();
    }
    let base = total / count as u64;
    let mut shares = vec![base; count];
    let mut remaining = total.saturating_sub(base.saturating_mul(count as u64));
    let mut index = 0usize;
    while remaining > 0 {
        shares[index % count] = shares[index % count].saturating_add(1);
        remaining -= 1;
        index += 1;
    }
    shares
}

fn launcher_kind_label(kind: runtime_kernel::RuntimeLauncherKind) -> &'static str {
    match kind {
        runtime_kernel::RuntimeLauncherKind::Engine => "engine",
    }
}

fn resolved_binding_for_role<'a>(
    resolved: &'a runtime_kernel::InferenceRuntimeKernel,
    role: runtime_contract::BackendRole,
) -> Option<&'a runtime_kernel::ResolvedRuntimeBinding> {
    match role {
        runtime_contract::BackendRole::Chat => resolved.primary_generation.as_ref(),
        runtime_contract::BackendRole::Embedding => resolved.embedding.as_ref(),
        runtime_contract::BackendRole::Stt => resolved.transcription.as_ref(),
        runtime_contract::BackendRole::Tts => resolved.speech.as_ref(),
        runtime_contract::BackendRole::Vision => resolved.vision.as_ref(),
    }
}

fn auxiliary_selection(
    role: runtime_contract::BackendRole,
    request_model: &str,
) -> engine::AuxiliaryModelSelection {
    match role {
        runtime_contract::BackendRole::Embedding => {
            engine::auxiliary_model_selection(engine::AuxiliaryRole::Embedding, Some(request_model))
        }
        runtime_contract::BackendRole::Stt => {
            engine::auxiliary_model_selection(engine::AuxiliaryRole::Stt, Some(request_model))
        }
        runtime_contract::BackendRole::Tts => {
            engine::auxiliary_model_selection(engine::AuxiliaryRole::Tts, Some(request_model))
        }
        runtime_contract::BackendRole::Vision => {
            engine::auxiliary_model_selection(engine::AuxiliaryRole::Vision, Some(request_model))
        }
        runtime_contract::BackendRole::Chat => unreachable!(),
    }
}

fn validate_primary_generation_budget(root: &Path) -> Result<()> {
    let Some(plan) = runtime_plan::load_persisted_chat_runtime_plan(root)? else {
        return Ok(());
    };
    let Some(snapshot) = resource_state::inspect_resource_snapshot() else {
        return Ok(());
    };
    runtime_plan::validate_live_gpu_budget(&plan, &snapshot)
        .with_context(|| format!("launch budget check failed for {}", plan.model))
}

fn validate_auxiliary_gpu_budget(
    root: &Path,
    descriptor: &RuntimeWorkloadDescriptor,
    admission: &GpuAdmission,
    sharing_managed_speech: bool,
) -> Result<()> {
    if descriptor.compute_target != Some(engine::ComputeTarget::Gpu) {
        return Ok(());
    }
    let Some(snapshot) = resource_state::inspect_resource_snapshot() else {
        anyhow::ensure!(
            !sharing_managed_speech,
            "speech GPU cohosting requires a live memory snapshot"
        );
        return Ok(());
    };
    let already_reserved = runtime_contract::reserved_gpu_mb_by_role(root, Some(descriptor.role))?;
    validate_auxiliary_gpu_budget_snapshot(
        descriptor,
        admission,
        &snapshot,
        &already_reserved,
        sharing_managed_speech,
    )
}

fn validate_auxiliary_gpu_budget_snapshot(
    descriptor: &RuntimeWorkloadDescriptor,
    admission: &GpuAdmission,
    snapshot: &resource_state::ResourceSnapshot,
    already_reserved: &BTreeMap<usize, u64>,
    sharing_managed_speech: bool,
) -> Result<()> {
    if sharing_managed_speech {
        anyhow::ensure!(
            admission.reserved_mb_by_gpu.len() == 1
                && admission.reserved_mb_by_gpu.values().all(|mb| *mb > 0),
            "speech GPU cohosting requires a positive single-device memory reservation"
        );
    }
    for (gpu_index, required_mb) in &admission.reserved_mb_by_gpu {
        let Some(gpu) = snapshot.gpu(*gpu_index) else {
            anyhow::bail!(
                "refusing to launch {} backend because gpu{} is not visible in the live snapshot",
                descriptor.role.as_str(),
                gpu_index
            );
        };
        // A resident speech model already consumes physical memory. Its planned
        // reservation is not free space for loading the complementary model.
        let live_available_mb = if sharing_managed_speech {
            gpu.free_mb
        } else {
            gpu.free_mb
                .saturating_add(already_reserved.get(gpu_index).copied().unwrap_or_default())
        };
        if *required_mb > live_available_mb {
            anyhow::bail!(
                "refusing to launch {} backend for {} because gpu{} only has {}MB available for CTOX but {}MB are required",
                descriptor.role.as_str(),
                descriptor.model,
                gpu_index,
                live_available_mb,
                required_mb
            );
        }
    }
    Ok(())
}

fn compatible_speech_gpu_holders(
    root: &Path,
    descriptor: &RuntimeWorkloadDescriptor,
    admission: &GpuAdmission,
    live_owned_pids: &[u32],
) -> Result<BTreeSet<u32>> {
    let ownership = runtime_contract::load_runtime_ownership_state(root)?;
    Ok(compatible_speech_gpu_holders_from_state(
        descriptor,
        admission,
        &ownership,
        live_owned_pids,
        |pid, role| same_root_native_speech_process(root, pid, role),
    ))
}

fn compatible_speech_gpu_holders_from_state(
    descriptor: &RuntimeWorkloadDescriptor,
    admission: &GpuAdmission,
    ownership: &runtime_contract::RuntimeOwnershipState,
    live_owned_pids: &[u32],
    verify_process: impl Fn(u32, runtime_contract::BackendRole) -> bool,
) -> BTreeSet<u32> {
    use runtime_contract::{BackendRole, RuntimeResidencyPhase};
    let complementary = match (descriptor.role, descriptor.model.as_str()) {
        (BackendRole::Stt, "engineai/Voxtral-Mini-4B-Realtime-2602") => {
            (BackendRole::Tts, "engineai/Voxtral-4B-TTS-2603")
        }
        (BackendRole::Tts, "engineai/Voxtral-4B-TTS-2603") => {
            (BackendRole::Stt, "engineai/Voxtral-Mini-4B-Realtime-2602")
        }
        _ => return BTreeSet::new(),
    };
    let devices = admission
        .visible_devices
        .as_deref()
        .map(parse_visible_devices);
    let Some(devices) = devices.filter(|devices| devices.len() == 1) else {
        return BTreeSet::new();
    };
    if descriptor.compute_target != Some(engine::ComputeTarget::Gpu)
        || admission.reserved_mb_by_gpu.len() != 1
        || admission
            .reserved_mb_by_gpu
            .get(&devices[0])
            .copied()
            .unwrap_or(0)
            < auxiliary_selection(descriptor.role, &descriptor.model).gpu_reserve_mb()
    {
        return BTreeSet::new();
    }
    ownership
        .workloads
        .iter()
        .filter(|workload| {
            workload.role == complementary.0
                && workload.model == complementary.1
                && workload.phase == RuntimeResidencyPhase::Active
                && workload.launcher_kind.as_deref() == Some("engine")
                && workload.compute_target.as_deref() == Some("gpu")
                && workload.visible_devices == devices
                && workload.reserved_mb_by_gpu.len() == 1
                && workload
                    .reserved_mb_by_gpu
                    .get(&devices[0])
                    .copied()
                    .unwrap_or(0)
                    >= auxiliary_selection(complementary.0, complementary.1).gpu_reserve_mb()
        })
        .filter_map(|workload| {
            workload
                .pid
                .filter(|pid| live_owned_pids.contains(pid) && verify_process(*pid, workload.role))
        })
        .collect()
}

fn same_root_native_speech_process(
    root: &Path,
    pid: u32,
    role: runtime_contract::BackendRole,
) -> bool {
    #[cfg(target_os = "linux")]
    {
        let command = match role {
            runtime_contract::BackendRole::Stt => "__native-voxtral-stt-service",
            runtime_contract::BackendRole::Tts => "__native-voxtral-tts-service",
            _ => return false,
        };
        let process = Path::new("/proc").join(pid.to_string());
        let (Ok(exe), Ok(expected), Ok(cwd), Ok(root), Ok(args)) = (
            std::fs::read_link(process.join("exe")),
            std::env::current_exe().and_then(std::fs::canonicalize),
            std::fs::read_link(process.join("cwd")),
            std::fs::canonicalize(root),
            std::fs::read(process.join("cmdline")),
        ) else {
            return false;
        };
        exe == expected
            && cwd == root
            && args.split(|byte| *byte == 0).nth(1) == Some(command.as_bytes())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (root, pid, role);
        false
    }
}

fn classify_gpu_blockers(
    processes: &[resource_state::GpuProcessLiveState],
    gpu_indices: &[usize],
    owned_pids: &BTreeSet<u32>,
    self_pid: u32,
) -> GpuBlockers {
    let mut owned = BTreeSet::new();
    let mut foreign = BTreeSet::new();
    for process in processes
        .iter()
        .filter(|process| gpu_indices.contains(&process.gpu_index))
        .filter(|process| process.pid != self_pid)
    {
        if owned_pids.contains(&process.pid) {
            owned.insert(process.pid);
        } else {
            foreign.insert(process.pid);
        }
    }
    GpuBlockers {
        owned_pids: owned.into_iter().collect(),
        foreign_pids: foreign.into_iter().collect(),
    }
}

fn managed_runtime_gpu_holder_pids(root: &Path, gpu_indices: &[usize]) -> Result<BTreeSet<u32>> {
    let ownership = runtime_contract::load_runtime_ownership_state(root)?;
    Ok(managed_runtime_gpu_holder_pids_from_state(
        ownership,
        gpu_indices,
    ))
}

fn managed_runtime_gpu_holder_pids_from_state(
    ownership: runtime_contract::RuntimeOwnershipState,
    gpu_indices: &[usize],
) -> BTreeSet<u32> {
    let mut pids = BTreeSet::new();
    for workload in ownership.workloads {
        let claims_target_gpu = workload
            .visible_devices
            .iter()
            .any(|gpu_index| gpu_indices.contains(gpu_index))
            || workload
                .reserved_mb_by_gpu
                .keys()
                .any(|gpu_index| gpu_indices.contains(gpu_index));
        if claims_target_gpu {
            if let Some(pid) = workload.pid {
                pids.insert(pid);
            }
        }
    }
    pids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::runtime_contract::BackendRole;
    use crate::inference::runtime_plan::ChatPreset;
    use crate::inference::runtime_plan::ChatRuntimePlan;
    use crate::inference::runtime_plan::PlannedGpuAllocation;
    use crate::inference::runtime_plan::TheoreticalResourceBreakdown;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_temp_root() -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("ctox-runtime-gpu-test-{unique}"));
        std::fs::create_dir_all(path.join("runtime")).unwrap();
        path
    }

    fn sample_plan() -> ChatRuntimePlan {
        ChatRuntimePlan {
            model: "openai/gpt-oss-120b".to_string(),
            preset: ChatPreset::Quality,
            quantization: "q4".to_string(),
            runtime_isq: None,
            max_seq_len: 131_072,
            compaction_threshold_percent: 80,
            compaction_min_tokens: 4096,
            min_context_floor_applied: false,
            paged_attn: "on".to_string(),
            pa_cache_type: None,
            pa_memory_fraction: None,
            pa_context_len: None,
            disable_nccl: false,
            tensor_parallel_backend: None,
            mn_local_world_size: None,
            max_batch_size: 8,
            max_seqs: 8,
            cuda_visible_devices: "0,1".to_string(),
            device_layers: None,
            topology: None,
            allow_device_layers_with_topology: false,
            nm_device_ordinal: None,
            base_device_ordinal: None,
            moe_experts_backend: None,
            disable_flash_attn: false,
            force_no_mmap: false,
            force_language_model_only: false,
            require_prebuilt_uqff_for_chat_start: false,
            isq_singlethread: false,
            isq_cpu_threads: None,
            expected_tok_s: 42.0,
            hardware_fingerprint: "test".to_string(),
            theoretical_breakdown: TheoreticalResourceBreakdown {
                contract_source: "test".to_string(),
                effective_total_budget_mb: 1,
                kv_budget_cap_mb: 1,
                kv_budget_fraction_milli: 1,
                weight_residency_mb: 1,
                kv_cache_mb: 1,
                fixed_runtime_base_overhead_mb: 1,
                backend_runtime_overhead_mb: 2,
                activation_overhead_mb: 3,
                load_peak_overhead_mb: 4,
                safety_headroom_mb: 1,
                required_effective_total_budget_mb: 1,
                required_total_mb: 1,
            },
            rationale: vec!["test".to_string()],
            gpu_allocations: vec![PlannedGpuAllocation {
                gpu_index: 0,
                name: "gpu0".to_string(),
                total_mb: 1,
                desktop_reserve_mb: 0,
                aux_reserve_mb: 0,
                chat_budget_mb: 10,
                backend_overhead_mb: 2,
                activation_overhead_mb: 3,
                load_peak_overhead_mb: 4,
                repeating_weight_mb: 0,
                weight_mb: 0,
                kv_cache_mb: 0,
                free_headroom_mb: 0,
                chat_enabled: true,
            }],
            moe_cache: None,
        }
    }

    fn speech_fixture() -> (
        RuntimeWorkloadDescriptor,
        GpuAdmission,
        runtime_contract::RuntimeOwnershipState,
    ) {
        (
            RuntimeWorkloadDescriptor {
                role: BackendRole::Tts,
                model: "engineai/Voxtral-4B-TTS-2603".to_string(),
                port: 19881,
                health_path: "/health".to_string(),
                launcher_kind: runtime_kernel::RuntimeLauncherKind::Engine,
                compute_target: Some(engine::ComputeTarget::Gpu),
            },
            GpuAdmission {
                visible_devices: Some("2".to_string()),
                reserved_mb_by_gpu: BTreeMap::from([(2, 12_288)]),
            },
            runtime_contract::RuntimeOwnershipState {
                version: 1,
                workloads: vec![runtime_contract::BackendRuntimeResidency {
                    role: BackendRole::Stt,
                    phase: runtime_contract::RuntimeResidencyPhase::Active,
                    model: "engineai/Voxtral-Mini-4B-Realtime-2602".to_string(),
                    pid: Some(22),
                    port: Some(19880),
                    health_path: Some("/health".to_string()),
                    launcher_kind: Some("engine".to_string()),
                    compute_target: Some("gpu".to_string()),
                    visible_devices: vec![2],
                    reserved_mb_by_gpu: BTreeMap::from([(2, 6144)]),
                    updated_at_epoch_secs: 1,
                }],
            },
        )
    }

    #[test]
    fn voxtral_speech_pair_can_cohost_without_admitting_foreign_processes() {
        let (descriptor, admission, ownership) = speech_fixture();
        let permitted = compatible_speech_gpu_holders_from_state(
            &descriptor,
            &admission,
            &ownership,
            &[22],
            |pid, role| pid == 22 && role == BackendRole::Stt,
        );
        assert_eq!(permitted, BTreeSet::from([22]));
        let processes = [22, 33].map(|pid| resource_state::GpuProcessLiveState {
            gpu_index: 2,
            gpu_uuid: None,
            pid,
            used_mb: 1000,
            process_name: "process".to_string(),
            command: None,
        });
        let mut blockers = classify_gpu_blockers(&processes, &[2], &BTreeSet::from([22]), 999);
        blockers.owned_pids.retain(|pid| !permitted.contains(pid));
        assert!(blockers.owned_pids.is_empty());
        assert_eq!(blockers.foreign_pids, vec![33]);
        let mut chat = descriptor.clone();
        chat.role = BackendRole::Chat;
        assert!(compatible_speech_gpu_holders_from_state(
            &chat,
            &admission,
            &ownership,
            &[22],
            |_, _| true,
        )
        .is_empty());
    }

    #[test]
    fn speech_cohosting_requires_active_exact_model_and_verified_process() {
        let (descriptor, admission, ownership) = speech_fixture();
        assert!(compatible_speech_gpu_holders_from_state(
            &descriptor,
            &admission,
            &ownership,
            &[22],
            |_, _| false,
        )
        .is_empty());
        for invalid in 0..8 {
            let mut invalid_ownership = ownership.clone();
            let peer = &mut invalid_ownership.workloads[0];
            match invalid {
                0 => peer.phase = runtime_contract::RuntimeResidencyPhase::Starting,
                1 => peer.model = "unrelated-model".to_string(),
                2 => peer.visible_devices = vec![1],
                3 => {
                    peer.reserved_mb_by_gpu.insert(2, 0);
                }
                4 => peer.launcher_kind = Some("unmanaged".to_string()),
                5 => peer.compute_target = Some("cpu".to_string()),
                6 => peer.pid = Some(23),
                _ => {
                    peer.reserved_mb_by_gpu.insert(2, 1);
                }
            }
            assert!(
                compatible_speech_gpu_holders_from_state(
                    &descriptor,
                    &admission,
                    &invalid_ownership,
                    &[22],
                    |_, _| true,
                )
                .is_empty(),
                "invalid residency case {invalid}"
            );
        }
    }

    #[test]
    fn speech_cohosting_also_allows_stt_after_managed_tts() {
        let (mut descriptor, mut admission, mut ownership) = speech_fixture();
        descriptor.role = BackendRole::Stt;
        descriptor.model = "engineai/Voxtral-Mini-4B-Realtime-2602".to_string();
        ownership.workloads[0].role = BackendRole::Tts;
        ownership.workloads[0].model = "engineai/Voxtral-4B-TTS-2603".to_string();
        ownership.workloads[0].reserved_mb_by_gpu.insert(2, 12_288);
        admission.reserved_mb_by_gpu.insert(2, 4200);
        assert_eq!(
            compatible_speech_gpu_holders_from_state(
                &descriptor,
                &admission,
                &ownership,
                &[22],
                |pid, role| pid == 22 && role == BackendRole::Tts,
            ),
            BTreeSet::from([22])
        );
    }

    #[test]
    fn speech_reservation_cannot_replace_physical_free_memory() {
        let (descriptor, admission, _) = speech_fixture();
        let mut snapshot = resource_state::ResourceSnapshot {
            source: "test".to_string(),
            gpus: vec![resource_state::GpuLiveState {
                index: 2,
                uuid: None,
                name: "A4500".to_string(),
                total_mb: 20000,
                used_mb: 14000,
                free_mb: 6000,
            }],
        };
        let other_role_reservation = BTreeMap::from([(2, 6144)]);
        let error = validate_auxiliary_gpu_budget_snapshot(
            &descriptor,
            &admission,
            &snapshot,
            &other_role_reservation,
            true,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("only has 6000MB"));
        snapshot.gpus[0].used_mb = 6000;
        snapshot.gpus[0].free_mb = 14000;
        validate_auxiliary_gpu_budget_snapshot(
            &descriptor,
            &admission,
            &snapshot,
            &other_role_reservation,
            true,
        )
        .unwrap();
        let mut missing_reservation = admission.clone();
        missing_reservation.reserved_mb_by_gpu.clear();
        assert!(validate_auxiliary_gpu_budget_snapshot(
            &descriptor,
            &missing_reservation,
            &snapshot,
            &other_role_reservation,
            true,
        )
        .is_err());
    }

    #[test]
    fn a_pid_for_this_test_process_is_not_a_native_speech_service() {
        assert!(!same_root_native_speech_process(
            &std::env::current_dir().unwrap(),
            std::process::id(),
            BackendRole::Stt,
        ));
    }

    #[test]
    fn visible_devices_parser_ignores_invalid_chunks() {
        assert_eq!(parse_visible_devices("0, 2, x, 5"), vec![0, 2, 5]);
    }

    #[test]
    fn primary_generation_admission_uses_planned_gpu_budget() {
        let root = make_temp_root();
        runtime_plan::store_persisted_chat_runtime_plan(&root, Some(&sample_plan())).unwrap();
        let descriptor = RuntimeWorkloadDescriptor {
            role: BackendRole::Chat,
            model: "openai/gpt-oss-120b".to_string(),
            port: 1234,
            health_path: "/health".to_string(),
            launcher_kind: runtime_kernel::RuntimeLauncherKind::Engine,
            compute_target: None,
        };

        let admission = resolve_gpu_admission(&root, &descriptor).unwrap();
        assert_eq!(admission.visible_devices.as_deref(), Some("0,1"));
        assert_eq!(admission.reserved_mb_by_gpu.get(&0).copied(), Some(19));
    }

    #[test]
    fn blocker_classification_keeps_managed_runtime_separate() {
        let processes = vec![
            resource_state::GpuProcessLiveState {
                gpu_index: 0,
                gpu_uuid: None,
                pid: 111,
                used_mb: 1024,
                process_name: "python".to_string(),
                command: Some("python train.py".to_string()),
            },
            resource_state::GpuProcessLiveState {
                gpu_index: 1,
                gpu_uuid: None,
                pid: 222,
                used_mb: 256,
                process_name: "ctox-engine".to_string(),
                command: Some("/home/user/bin/ctox-engine".to_string()),
            },
        ];
        let owned = BTreeSet::from([222u32]);

        let blockers = classify_gpu_blockers(&processes, &[0, 1], &owned, 999);
        assert_eq!(blockers.foreign_pids, vec![111]);
        assert_eq!(blockers.owned_pids, vec![222]);
    }

    #[test]
    fn gpu_holder_detection_ignores_cpu_aux_without_gpu_claims() {
        let ownership = runtime_contract::RuntimeOwnershipState {
            version: 1,
            workloads: vec![
                runtime_contract::BackendRuntimeResidency {
                    role: BackendRole::Embedding,
                    phase: runtime_contract::RuntimeResidencyPhase::Active,
                    model: "Qwen/Qwen3-Embedding-0.6B".to_string(),
                    pid: Some(22),
                    port: Some(1237),
                    health_path: Some("/health".to_string()),
                    launcher_kind: Some("engine".to_string()),
                    compute_target: Some("cpu".to_string()),
                    visible_devices: Vec::new(),
                    reserved_mb_by_gpu: BTreeMap::new(),
                    updated_at_epoch_secs: 1,
                },
                runtime_contract::BackendRuntimeResidency {
                    role: BackendRole::Chat,
                    phase: runtime_contract::RuntimeResidencyPhase::Starting,
                    model: "Qwen/Qwen3.5-35B-A3B".to_string(),
                    pid: Some(33),
                    port: Some(1235),
                    health_path: Some("/health".to_string()),
                    launcher_kind: Some("engine".to_string()),
                    compute_target: Some("gpu".to_string()),
                    visible_devices: vec![0, 1, 2, 3],
                    reserved_mb_by_gpu: BTreeMap::from([(0usize, 1024u64)]),
                    updated_at_epoch_secs: 1,
                },
            ],
        };

        let holders = managed_runtime_gpu_holder_pids_from_state(ownership, &[0, 1, 2, 3]);
        assert_eq!(holders, BTreeSet::from([33u32]));
    }
}
