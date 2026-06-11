//! GPU probing, sampling and allocation/context estimation for the TUI
//! header and settings views. Pure functions over types owned by the
//! parent module.
use super::*;

/// The uncached probe costs a `runtime_env` SQLite lookup plus an
/// `nvidia-smi` spawn. The settings refresh path consults it several times
/// per tick, but the hardware does not change between key strokes — so the
/// result is cached per root with the same 5s TTL as `runtime_plan`'s
/// hardware-profile cache. `invalidate_runtime_observations` drops the cache
/// after saves and runtime switches.
const LOCAL_GPU_PROBE_CACHE_TTL: Duration = Duration::from_secs(5);

pub(super) fn local_gpu_probe_cache() -> &'static Mutex<Option<(Instant, PathBuf, bool)>> {
    static CACHE: OnceLock<Mutex<Option<(Instant, PathBuf, bool)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

pub(super) fn invalidate_local_gpu_probe_cache() {
    *local_gpu_probe_cache()
        .lock()
        .unwrap_or_else(|err| err.into_inner()) = None;
}

pub(super) fn local_gpu_available(root: &Path) -> bool {
    if let Some((probed_at, cached_root, available)) = local_gpu_probe_cache()
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .as_ref()
    {
        if cached_root.as_path() == root && probed_at.elapsed() < LOCAL_GPU_PROBE_CACHE_TTL {
            return *available;
        }
    }
    let available = local_gpu_available_uncached(root);
    *local_gpu_probe_cache()
        .lock()
        .unwrap_or_else(|err| err.into_inner()) =
        Some((Instant::now(), root.to_path_buf(), available));
    available
}

pub(super) fn local_gpu_available_uncached(root: &Path) -> bool {
    if let Some(spec) = runtime_env::env_or_config(root, "CTOX_TEST_GPU_TOTALS_MB") {
        return spec
            .split(';')
            .filter_map(|chunk| chunk.split_once(':'))
            .any(|(_, total)| total.trim().parse::<u64>().ok().is_some());
    }
    Command::new("nvidia-smi")
        .args(["--query-gpu=index", "--format=csv,noheader,nounits"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| !String::from_utf8_lossy(&output.stdout).trim().is_empty())
        .unwrap_or(false)
}

pub(super) fn sample_gpu_cards() -> Result<Vec<GpuCardState>> {
    let gpu_output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=index,uuid,name,memory.used,memory.total,utilization.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .context("failed to run nvidia-smi gpu query")?;
    if !gpu_output.status.success() {
        anyhow::bail!("nvidia-smi gpu query failed");
    }

    let mut cards = Vec::new();
    let mut uuid_to_index = HashMap::new();
    for line in String::from_utf8_lossy(&gpu_output.stdout).lines() {
        let parts = line.split(',').map(|part| part.trim()).collect::<Vec<_>>();
        if parts.len() < 6 {
            continue;
        }
        let index = parts[0].parse::<usize>().unwrap_or(0);
        uuid_to_index.insert(parts[1].to_string(), index);
        cards.push(GpuCardState {
            index,
            name: parts[2].to_string(),
            used_mb: parts[3].parse::<u64>().unwrap_or(0),
            total_mb: parts[4].parse::<u64>().unwrap_or(0),
            utilization: parts[5].parse::<u64>().unwrap_or(0),
            allocations: Vec::new(),
        });
    }

    let proc_output = Command::new("nvidia-smi")
        .args([
            "--query-compute-apps=gpu_uuid,pid,used_memory",
            "--format=csv,noheader,nounits",
        ])
        .output();
    let Ok(proc_output) = proc_output else {
        return Ok(cards);
    };
    if !proc_output.status.success() {
        return Ok(cards);
    }

    let mut pid_to_gpu = Vec::new();
    let mut pids = Vec::new();
    for line in String::from_utf8_lossy(&proc_output.stdout).lines() {
        let parts = line.split(',').map(|part| part.trim()).collect::<Vec<_>>();
        if parts.len() < 3 {
            continue;
        }
        let Some(&gpu_index) = uuid_to_index.get(parts[0]) else {
            continue;
        };
        let Ok(pid) = parts[1].parse::<u32>() else {
            continue;
        };
        let used_mb = parts[2].parse::<u64>().unwrap_or(0);
        pid_to_gpu.push((pid, gpu_index, used_mb));
        pids.push(pid.to_string());
    }
    if pids.is_empty() {
        return Ok(cards);
    }

    let ps_output = Command::new("ps")
        .args(["-o", "pid=,command=", "-p", &pids.join(",")])
        .output()
        .context("failed to run ps for gpu processes")?;
    let mut pid_to_command = HashMap::new();
    for line in String::from_utf8_lossy(&ps_output.stdout).lines() {
        let trimmed = line.trim_start();
        let mut split = trimmed.splitn(2, ' ');
        let Some(pid_raw) = split.next() else {
            continue;
        };
        let Some(command) = split.next() else {
            continue;
        };
        if let Ok(pid) = pid_raw.trim().parse::<u32>() {
            pid_to_command.insert(pid, command.trim().to_string());
        }
    }

    for (pid, gpu_index, used_mb) in pid_to_gpu {
        let Some(command) = pid_to_command.get(&pid) else {
            continue;
        };
        let Some(model) = model_name_from_process_command(command) else {
            continue;
        };
        let short_label = short_gpu_label(&model);
        if let Some(card) = cards.iter_mut().find(|card| card.index == gpu_index) {
            if let Some(existing) = card
                .allocations
                .iter_mut()
                .find(|allocation| allocation.model == model)
            {
                existing.used_mb = existing.used_mb.saturating_add(used_mb);
            } else {
                card.allocations.push(GpuModelUsage {
                    model,
                    short_label,
                    used_mb,
                });
            }
        }
    }

    cards.sort_by_key(|card| card.index);
    for card in &mut cards {
        card.allocations
            .sort_by(|left, right| right.used_mb.cmp(&left.used_mb));
    }
    Ok(cards)
}

pub(super) fn model_name_from_process_command(command: &str) -> Option<String> {
    model_registry::process_command_model_name(command).map(str::to_string)
}

pub(super) fn estimated_tokens_per_second(
    model: &str,
    perf_stats: &BTreeMap<String, ModelPerfStats>,
) -> Option<f64> {
    perf_stats
        .get(model.trim())
        .map(|stats| stats.avg_tokens_per_second)
        .or_else(|| model_registry::estimated_tokens_per_second(model))
}

pub(super) fn gpu_cards_from_plan(
    plan: &runtime_plan::ChatRuntimePlan,
    env_map: &BTreeMap<String, String>,
) -> Vec<GpuCardState> {
    plan.gpu_allocations
        .iter()
        .map(|allocation| {
            let mut allocations = Vec::new();
            if allocation.aux_reserve_mb > 0 {
                allocations.extend(estimated_aux_model_usages(
                    allocation.aux_reserve_mb,
                    env_map,
                ));
            }
            if allocation.chat_budget_mb > 0 {
                allocations.push(GpuModelUsage {
                    model: plan.model.clone(),
                    short_label: short_gpu_label(&plan.model),
                    used_mb: allocation.chat_budget_mb,
                });
            }
            GpuCardState {
                index: allocation.gpu_index,
                name: allocation.name.clone(),
                used_mb: allocation
                    .desktop_reserve_mb
                    .saturating_add(allocation.aux_reserve_mb)
                    .saturating_add(allocation.chat_budget_mb),
                total_mb: allocation.total_mb,
                utilization: 0,
                allocations,
            }
        })
        .collect()
}

pub(super) fn estimated_aux_model_usages(
    total_aux_mb: u64,
    env_map: &BTreeMap<String, String>,
) -> Vec<GpuModelUsage> {
    if total_aux_mb == 0 {
        return Vec::new();
    }
    let models = [
        (
            engine::AuxiliaryRole::Embedding,
            engine::auxiliary_model_selection(
                engine::AuxiliaryRole::Embedding,
                env_map.get("CTOX_EMBEDDING_MODEL").map(String::as_str),
            ),
        ),
        (
            engine::AuxiliaryRole::Stt,
            engine::auxiliary_model_selection(
                engine::AuxiliaryRole::Stt,
                env_map.get("CTOX_STT_MODEL").map(String::as_str),
            ),
        ),
        (
            engine::AuxiliaryRole::Tts,
            engine::auxiliary_model_selection(
                engine::AuxiliaryRole::Tts,
                env_map.get("CTOX_TTS_MODEL").map(String::as_str),
            ),
        ),
    ]
    .into_iter()
    .filter_map(|(role, selection)| {
        let reserve_mb = selection.gpu_reserve_mb();
        (reserve_mb > 0).then_some((role, selection.request_model, reserve_mb))
    })
    .collect::<Vec<_>>();
    if models.is_empty() {
        return Vec::new();
    }
    let total_weight = models
        .iter()
        .map(|(_, _, weight)| *weight)
        .sum::<u64>()
        .max(1);
    let mut remaining = total_aux_mb;
    let mut usages = Vec::with_capacity(models.len());
    for (index, (role, model, weight)) in models.iter().enumerate() {
        let share = if index + 1 == models.len() {
            remaining
        } else {
            total_aux_mb.saturating_mul(*weight) / total_weight
        };
        remaining = remaining.saturating_sub(share);
        usages.push(GpuModelUsage {
            model: (*model).to_string(),
            short_label: auxiliary_role_label(*role).to_string(),
            used_mb: share,
        });
    }
    usages.retain(|usage| usage.used_mb > 0);
    usages
}

pub(super) fn estimate_gpu_cards(
    live_cards: &[GpuCardState],
    loaded_model: &str,
    draft_model: &str,
    settings: &BTreeMap<String, String>,
    live_context: usize,
) -> Vec<GpuCardState> {
    let mut cards = live_cards.to_vec();
    let target_isq = settings
        .get("CTOX_ENGINE_ISQ")
        .map(String::as_str)
        .unwrap_or("Q4K");
    let target_context = settings
        .get("CTOX_ENGINE_MAX_SEQ_LEN")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(live_context.max(2048));
    let estimated_total_mb = estimate_chat_model_memory_mb(draft_model, target_isq, target_context);

    let mut aux_by_gpu: HashMap<usize, u64> = HashMap::new();
    for card in &cards {
        let aux_used = card
            .allocations
            .iter()
            .filter(|allocation| allocation.model != loaded_model)
            .map(|allocation| allocation.used_mb)
            .sum::<u64>();
        aux_by_gpu.insert(card.index, aux_used);
    }

    let weights = parse_device_layer_weights(settings.get("CTOX_ENGINE_DEVICE_LAYERS"));
    let total_weight = weights.values().copied().sum::<u64>();
    let selected_gpu_indices = if weights.is_empty() {
        cards.iter().map(|card| card.index).collect::<Vec<_>>()
    } else {
        weights.keys().copied().collect::<Vec<_>>()
    };
    let gpu_count = selected_gpu_indices.len().max(1) as u64;

    for card in &mut cards {
        let aux_used = aux_by_gpu.get(&card.index).copied().unwrap_or(0);
        card.allocations
            .retain(|allocation| allocation.model != loaded_model);
        let chat_used = if let Some(weight) = weights.get(&card.index) {
            if total_weight == 0 {
                estimated_total_mb / gpu_count
            } else {
                estimated_total_mb.saturating_mul(*weight) / total_weight
            }
        } else if weights.is_empty() {
            estimated_total_mb / gpu_count
        } else {
            0
        };
        if chat_used > 0 {
            card.allocations.push(GpuModelUsage {
                model: draft_model.to_string(),
                short_label: short_gpu_label(draft_model),
                used_mb: chat_used,
            });
        }
        card.used_mb = aux_used.saturating_add(chat_used);
        card.allocations
            .sort_by(|left, right| right.used_mb.cmp(&left.used_mb));
    }
    cards
}

pub(super) fn configured_runtime_models(env_map: &BTreeMap<String, String>) -> Vec<String> {
    let mut models = Vec::new();
    if infer_chat_source(env_map).eq_ignore_ascii_case("local") {
        if let Some(value) = env_map
            .get("CTOX_CHAT_MODEL")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            models.push(value.to_string());
        }
    }
    for selection in [
        engine::auxiliary_model_selection(
            engine::AuxiliaryRole::Embedding,
            env_map.get("CTOX_EMBEDDING_MODEL").map(String::as_str),
        ),
        engine::auxiliary_model_selection(
            engine::AuxiliaryRole::Stt,
            env_map.get("CTOX_STT_MODEL").map(String::as_str),
        ),
        engine::auxiliary_model_selection(
            engine::AuxiliaryRole::Tts,
            env_map.get("CTOX_TTS_MODEL").map(String::as_str),
        ),
    ] {
        if !models
            .iter()
            .any(|existing| existing == selection.request_model)
        {
            models.push(selection.request_model.to_string());
        }
    }
    models
}

pub(super) fn load_observation_for_port(root: &Path, port: u16) -> Option<LoadObservation> {
    let path = root
        .join("runtime")
        .join(format!("load_observation_{port}.json"));
    let raw = std::fs::read(path).ok()?;
    serde_json::from_slice(&raw).ok()
}

pub(super) fn collect_runtime_load_observations(
    root: &Path,
    telemetry: Option<&RuntimeTelemetry>,
    env_map: &BTreeMap<String, String>,
) -> Vec<LoadObservation> {
    let mut observations = Vec::new();
    let mut seen = HashSet::new();

    if let Some(observation) = telemetry.and_then(|value| value.load_observation.clone()) {
        if seen.insert((
            observation.port,
            observation.model.clone(),
            observation.role.clone(),
        )) {
            observations.push(observation);
        }
    }

    for (role, port_key, model_value) in [
        (
            engine::AuxiliaryRole::Embedding,
            "CTOX_EMBEDDING_PORT",
            env_map.get("CTOX_EMBEDDING_MODEL").map(String::as_str),
        ),
        (
            engine::AuxiliaryRole::Stt,
            "CTOX_STT_PORT",
            env_map.get("CTOX_STT_MODEL").map(String::as_str),
        ),
        (
            engine::AuxiliaryRole::Tts,
            "CTOX_TTS_PORT",
            env_map.get("CTOX_TTS_MODEL").map(String::as_str),
        ),
    ] {
        let selection = engine::auxiliary_model_selection(role, model_value);
        if selection.compute_target == engine::ComputeTarget::Cpu {
            continue;
        }
        let port = env_map
            .get(port_key)
            .and_then(|value| value.trim().parse::<u16>().ok())
            .unwrap_or(selection.default_port);
        let Some(observation) = load_observation_for_port(root, port) else {
            continue;
        };
        let key = (
            observation.port,
            observation.model.clone(),
            observation.role.clone(),
        );
        if seen.insert(key) {
            observations.push(observation);
        }
    }

    observations
}

pub(super) fn auxiliary_backend_ready(
    resolved_runtime: Option<&runtime_kernel::InferenceRuntimeKernel>,
    role: engine::AuxiliaryRole,
) -> Option<bool> {
    let binding = resolved_runtime?.binding_for_auxiliary_role(role)?;
    Some(binding.transport.probe())
}

pub(super) fn runtime_health_state(
    root: &Path,
    telemetry: Option<&RuntimeTelemetry>,
) -> RuntimeHealthState {
    let resolved_runtime = runtime_kernel::InferenceRuntimeKernel::resolve(root).ok();
    let chat_source_is_api = resolved_runtime
        .as_ref()
        .map(|k| {
            matches!(
                k.state.source,
                crate::execution::models::runtime_state::InferenceSource::Api
            )
        })
        .unwrap_or(false);
    let runtime_ready = telemetry
        .map(|value| value.backend_healthy)
        .unwrap_or_else(|| {
            crate::inference::gateway::current_runtime_telemetry(root).backend_healthy
        });
    // In pure API mode local auxiliaries are optional; keep them unknown
    // instead of degraded when no local runtime is expected.
    let skip_aux = chat_source_is_api;
    RuntimeHealthState {
        runtime_ready,
        embedding_ready: if skip_aux {
            None
        } else {
            auxiliary_backend_ready(resolved_runtime.as_ref(), engine::AuxiliaryRole::Embedding)
        },
        stt_ready: if skip_aux {
            None
        } else {
            auxiliary_backend_ready(resolved_runtime.as_ref(), engine::AuxiliaryRole::Stt)
        },
        tts_ready: if skip_aux {
            None
        } else {
            auxiliary_backend_ready(resolved_runtime.as_ref(), engine::AuxiliaryRole::Tts)
        },
    }
}

pub(super) fn ui_now_epoch_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(super) fn load_observation_is_loading(observation: &LoadObservation) -> bool {
    if observation.startup_healthy {
        return false;
    }
    let observed_until = observation.observed_until_epoch;
    observed_until > 0 && ui_now_epoch_seconds().saturating_sub(observed_until) <= 30
}

pub(super) fn push_gpu_allocation(
    cards: &mut Vec<GpuCardState>,
    gpu_index: usize,
    gpu_name: &str,
    total_mb: u64,
    model: &str,
    short_label: &str,
    used_mb: u64,
) {
    if used_mb == 0 {
        return;
    }
    let card_index = if let Some(position) = cards.iter().position(|card| card.index == gpu_index) {
        position
    } else {
        cards.push(GpuCardState {
            index: gpu_index,
            name: gpu_name.to_string(),
            used_mb: 0,
            total_mb,
            utilization: 0,
            allocations: Vec::new(),
        });
        cards.len().saturating_sub(1)
    };
    let card = cards.get_mut(card_index).expect("gpu card inserted");
    if card.total_mb == 0 {
        card.total_mb = total_mb;
    }
    if card.name.is_empty() {
        card.name = gpu_name.to_string();
    }
    if let Some(existing) = card
        .allocations
        .iter_mut()
        .find(|allocation| allocation.model == model)
    {
        existing.used_mb = existing.used_mb.max(used_mb);
    } else {
        card.allocations.push(GpuModelUsage {
            model: model.to_string(),
            short_label: short_label.to_string(),
            used_mb,
        });
    }
    card.used_mb = card.used_mb.saturating_add(used_mb);
}

pub(super) fn normalize_gpu_cards(cards: &mut Vec<GpuCardState>) {
    cards.sort_by_key(|card| card.index);
    for card in cards.iter_mut() {
        card.allocations
            .sort_by(|left, right| right.used_mb.cmp(&left.used_mb));
        card.used_mb = card
            .allocations
            .iter()
            .map(|allocation| allocation.used_mb)
            .sum();
    }
    cards.retain(|card| !card.allocations.is_empty() || card.total_mb > 0);
}

pub(super) fn merge_gpu_card_layers(
    mut base_cards: Vec<GpuCardState>,
    overlay_cards: Vec<GpuCardState>,
) -> Vec<GpuCardState> {
    for card in overlay_cards {
        for allocation in card.allocations {
            push_gpu_allocation(
                &mut base_cards,
                card.index,
                &card.name,
                card.total_mb,
                &allocation.model,
                &allocation.short_label,
                allocation.used_mb,
            );
        }
    }
    normalize_gpu_cards(&mut base_cards);
    base_cards
}

pub(super) fn deployed_gpu_cards_from_live(
    live_cards: &[GpuCardState],
    allowed_models: &[String],
    observations: &[LoadObservation],
) -> Vec<GpuCardState> {
    let loading_models = observations
        .iter()
        .filter(|observation| load_observation_is_loading(observation))
        .map(|observation| observation.model.as_str())
        .collect::<HashSet<_>>();
    let mut cards = filter_gpu_cards_to_models(live_cards, allowed_models);
    for card in cards.iter_mut() {
        card.allocations
            .retain(|allocation| !loading_models.contains(allocation.model.as_str()));
        card.used_mb = card
            .allocations
            .iter()
            .map(|allocation| allocation.used_mb)
            .sum();
    }
    cards
}

pub(super) fn loading_gpu_cards_from_observations(
    target_cards: &[GpuCardState],
    live_cards: &[GpuCardState],
    observations: &[LoadObservation],
) -> Vec<GpuCardState> {
    let mut cards = Vec::new();
    for observation in observations
        .iter()
        .filter(|observation| load_observation_is_loading(observation))
    {
        let short_label = short_gpu_label(&observation.model);
        let mut matched_target = false;
        for card in target_cards {
            for allocation in card
                .allocations
                .iter()
                .filter(|allocation| allocation.model == observation.model)
            {
                push_gpu_allocation(
                    &mut cards,
                    card.index,
                    &card.name,
                    card.total_mb,
                    &allocation.model,
                    &short_label,
                    allocation.used_mb,
                );
                matched_target = true;
            }
        }
        if matched_target {
            continue;
        }
        let mut matched_live = false;
        for card in live_cards {
            for allocation in card
                .allocations
                .iter()
                .filter(|allocation| allocation.model == observation.model)
            {
                push_gpu_allocation(
                    &mut cards,
                    card.index,
                    &card.name,
                    card.total_mb,
                    &allocation.model,
                    &short_label,
                    allocation.used_mb,
                );
                matched_live = true;
            }
        }
        if matched_live {
            continue;
        }
        for gpu in &observation.gpus {
            let observed_mb = gpu
                .current_delta_mb
                .max(gpu.peak_delta_mb)
                .max(gpu.final_used_mb.saturating_sub(gpu.baseline_used_mb));
            push_gpu_allocation(
                &mut cards,
                gpu.gpu_index,
                &gpu.name,
                gpu.total_mb,
                &observation.model,
                &short_label,
                observed_mb,
            );
        }
    }
    normalize_gpu_cards(&mut cards);
    cards
}

pub(super) fn unhealthy_backend_models(
    env_map: &BTreeMap<String, String>,
    runtime_health: &RuntimeHealthState,
) -> HashSet<String> {
    let mut models = HashSet::new();
    for (role, ready, model_value) in [
        (
            engine::AuxiliaryRole::Embedding,
            runtime_health.embedding_ready,
            env_map.get("CTOX_EMBEDDING_MODEL").map(String::as_str),
        ),
        (
            engine::AuxiliaryRole::Stt,
            runtime_health.stt_ready,
            env_map.get("CTOX_STT_MODEL").map(String::as_str),
        ),
        (
            engine::AuxiliaryRole::Tts,
            runtime_health.tts_ready,
            env_map.get("CTOX_TTS_MODEL").map(String::as_str),
        ),
    ] {
        if ready == Some(false) {
            let selection = engine::auxiliary_model_selection(role, model_value);
            models.insert(selection.request_model.to_string());
        }
    }
    models
}

pub(super) fn unhealthy_backend_loading_cards(
    live_cards: &[GpuCardState],
    env_map: &BTreeMap<String, String>,
    runtime_health: &RuntimeHealthState,
) -> Vec<GpuCardState> {
    let unhealthy_models = unhealthy_backend_models(env_map, runtime_health);
    if unhealthy_models.is_empty() {
        return Vec::new();
    }
    let target_cards = aux_gpu_target_cards(live_cards, env_map);
    let mut cards = Vec::new();
    for card in &target_cards {
        for allocation in card
            .allocations
            .iter()
            .filter(|allocation| unhealthy_models.contains(&allocation.model))
        {
            push_gpu_allocation(
                &mut cards,
                card.index,
                &card.name,
                card.total_mb,
                &allocation.model,
                &allocation.short_label,
                allocation.used_mb,
            );
        }
    }
    normalize_gpu_cards(&mut cards);
    cards
}

pub(super) fn healthy_backend_models(
    env_map: &BTreeMap<String, String>,
    runtime_health: &RuntimeHealthState,
) -> HashSet<String> {
    let mut models = HashSet::new();
    for (role, ready, model_value) in [
        (
            engine::AuxiliaryRole::Embedding,
            runtime_health.embedding_ready,
            env_map.get("CTOX_EMBEDDING_MODEL").map(String::as_str),
        ),
        (
            engine::AuxiliaryRole::Stt,
            runtime_health.stt_ready,
            env_map.get("CTOX_STT_MODEL").map(String::as_str),
        ),
        (
            engine::AuxiliaryRole::Tts,
            runtime_health.tts_ready,
            env_map.get("CTOX_TTS_MODEL").map(String::as_str),
        ),
    ] {
        if ready == Some(true) {
            let selection = engine::auxiliary_model_selection(role, model_value);
            models.insert(selection.request_model.to_string());
        }
    }
    models
}

pub(super) fn healthy_backend_deployed_cards(
    live_cards: &[GpuCardState],
    env_map: &BTreeMap<String, String>,
    runtime_health: &RuntimeHealthState,
) -> Vec<GpuCardState> {
    let healthy_models = healthy_backend_models(env_map, runtime_health);
    if healthy_models.is_empty() {
        return Vec::new();
    }
    let target_cards = aux_gpu_target_cards(live_cards, env_map);
    let mut cards = Vec::new();
    for card in &target_cards {
        for allocation in card.allocations.iter().filter(|allocation| {
            healthy_models.contains(&allocation.model)
                && !live_cards.iter().any(|live_card| {
                    live_card
                        .allocations
                        .iter()
                        .any(|candidate| candidate.model == allocation.model)
                })
        }) {
            push_gpu_allocation(
                &mut cards,
                card.index,
                &card.name,
                card.total_mb,
                &allocation.model,
                &allocation.short_label,
                allocation.used_mb,
            );
        }
    }
    normalize_gpu_cards(&mut cards);
    cards
}

pub(super) fn parse_csv_gpu_indices(raw: Option<&String>) -> Vec<usize> {
    raw.into_iter()
        .flat_map(|value| value.split(','))
        .filter_map(|chunk| chunk.trim().parse::<usize>().ok())
        .collect()
}

pub(super) fn even_shares(total: u64, count: usize) -> Vec<u64> {
    if count == 0 {
        return Vec::new();
    }
    let base = total / count as u64;
    let remainder = total % count as u64;
    (0..count)
        .map(|index| base + u64::from(index < remainder as usize))
        .collect()
}

pub(super) fn auxiliary_visible_devices_for_role(
    env_map: &BTreeMap<String, String>,
    role: engine::AuxiliaryRole,
) -> Vec<usize> {
    let role_specific_key = match role {
        engine::AuxiliaryRole::Embedding => "CTOX_EMBEDDING_CUDA_VISIBLE_DEVICES",
        engine::AuxiliaryRole::Stt => "CTOX_STT_CUDA_VISIBLE_DEVICES",
        engine::AuxiliaryRole::Tts => "CTOX_TTS_CUDA_VISIBLE_DEVICES",
        engine::AuxiliaryRole::Vision => "CTOX_VISION_CUDA_VISIBLE_DEVICES",
    };
    let explicit = parse_csv_gpu_indices(env_map.get(role_specific_key));
    if !explicit.is_empty() {
        return explicit;
    }
    let shared = parse_csv_gpu_indices(env_map.get("CTOX_AUXILIARY_CUDA_VISIBLE_DEVICES"));
    if !shared.is_empty() {
        return shared;
    }
    vec![0]
}

pub(super) fn aux_gpu_target_cards(
    live_cards: &[GpuCardState],
    env_map: &BTreeMap<String, String>,
) -> Vec<GpuCardState> {
    let mut cards = live_cards
        .iter()
        .map(|card| GpuCardState {
            index: card.index,
            name: card.name.clone(),
            used_mb: 0,
            total_mb: card.total_mb,
            utilization: 0,
            allocations: Vec::new(),
        })
        .collect::<Vec<_>>();

    for (role, model_value) in [
        (
            engine::AuxiliaryRole::Embedding,
            env_map.get("CTOX_EMBEDDING_MODEL").map(String::as_str),
        ),
        (
            engine::AuxiliaryRole::Stt,
            env_map.get("CTOX_STT_MODEL").map(String::as_str),
        ),
        (
            engine::AuxiliaryRole::Tts,
            env_map.get("CTOX_TTS_MODEL").map(String::as_str),
        ),
    ] {
        let selection = engine::auxiliary_model_selection(role, model_value);
        let reserve_mb = selection.gpu_reserve_mb();
        if reserve_mb == 0 {
            continue;
        }
        let gpu_indices = auxiliary_visible_devices_for_role(env_map, role);
        if gpu_indices.is_empty() {
            continue;
        }
        let shares = even_shares(reserve_mb, gpu_indices.len());
        for (position, gpu_index) in gpu_indices.iter().enumerate() {
            let share = *shares.get(position).unwrap_or(&0);
            if share == 0 {
                continue;
            }
            if !cards.iter().any(|card| card.index == *gpu_index) {
                cards.push(GpuCardState {
                    index: *gpu_index,
                    name: String::new(),
                    used_mb: 0,
                    total_mb: 0,
                    utilization: 0,
                    allocations: Vec::new(),
                });
            }
            let card = cards
                .iter_mut()
                .find(|card| card.index == *gpu_index)
                .expect("gpu card inserted");
            card.allocations.push(GpuModelUsage {
                model: selection.request_model.to_string(),
                short_label: auxiliary_role_label(role).to_string(),
                used_mb: share,
            });
            card.used_mb = card.used_mb.saturating_add(share);
        }
    }

    cards.retain(|card| !card.allocations.is_empty());
    cards.sort_by_key(|card| card.index);
    for card in &mut cards {
        card.allocations
            .sort_by(|left, right| right.used_mb.cmp(&left.used_mb));
    }
    cards
}

pub(super) fn filter_gpu_cards_to_models(
    live_cards: &[GpuCardState],
    allowed_models: &[String],
) -> Vec<GpuCardState> {
    if allowed_models.is_empty() {
        return live_cards.to_vec();
    }
    let mut cards = live_cards.to_vec();
    for card in &mut cards {
        card.allocations.retain(|allocation| {
            allowed_models
                .iter()
                .any(|model| model == &allocation.model)
        });
        card.allocations
            .sort_by(|left, right| right.used_mb.cmp(&left.used_mb));
        card.used_mb = card
            .allocations
            .iter()
            .map(|allocation| allocation.used_mb)
            .sum();
    }
    cards
}

#[allow(dead_code)]
pub(super) fn overlay_load_observation_gpu_cards(
    live_cards: &[GpuCardState],
    observations: &[LoadObservation],
    env_map: &BTreeMap<String, String>,
) -> Vec<GpuCardState> {
    let mut cards = live_cards.to_vec();
    if observations.is_empty() {
        return cards;
    }

    for observation in observations {
        if observation.model.trim().is_empty() || engine::is_api_chat_model(&observation.model) {
            continue;
        }
        if infer_chat_source(env_map).eq_ignore_ascii_case("api") && observation.role == "chat" {
            continue;
        }

        let short_label = short_gpu_label(&observation.model);
        for gpu in &observation.gpus {
            let observed_mb = gpu
                .current_delta_mb
                .max(gpu.final_used_mb.saturating_sub(gpu.baseline_used_mb))
                .max(gpu.peak_delta_mb.min(gpu.current_delta_mb));
            if observed_mb == 0 {
                continue;
            }
            let Some(card) = cards.iter_mut().find(|card| card.index == gpu.gpu_index) else {
                cards.push(GpuCardState {
                    index: gpu.gpu_index,
                    name: gpu.name.clone(),
                    used_mb: observed_mb,
                    total_mb: gpu.total_mb,
                    utilization: 0,
                    allocations: vec![GpuModelUsage {
                        model: observation.model.clone(),
                        short_label: short_label.clone(),
                        used_mb: observed_mb,
                    }],
                });
                continue;
            };
            if let Some(existing) = card
                .allocations
                .iter_mut()
                .find(|allocation| allocation.model == observation.model)
            {
                existing.used_mb = existing.used_mb.max(observed_mb);
            } else {
                card.allocations.push(GpuModelUsage {
                    model: observation.model.clone(),
                    short_label: short_label.clone(),
                    used_mb: observed_mb,
                });
            }
            card.used_mb = card.used_mb.max(observed_mb);
            if card.total_mb == 0 {
                card.total_mb = gpu.total_mb;
            }
            if card.name.is_empty() {
                card.name = gpu.name.clone();
            }
        }
    }

    cards.sort_by_key(|card| card.index);
    for card in &mut cards {
        card.allocations
            .sort_by(|left, right| right.used_mb.cmp(&left.used_mb));
        let allocation_total = card
            .allocations
            .iter()
            .map(|allocation| allocation.used_mb)
            .sum::<u64>();
        card.used_mb = card.used_mb.max(allocation_total);
    }
    cards
}

pub(super) fn expected_gpu_aux_labels(env_map: &BTreeMap<String, String>) -> Vec<String> {
    [
        (
            engine::AuxiliaryRole::Embedding,
            engine::auxiliary_model_selection(
                engine::AuxiliaryRole::Embedding,
                env_map.get("CTOX_EMBEDDING_MODEL").map(String::as_str),
            ),
        ),
        (
            engine::AuxiliaryRole::Stt,
            engine::auxiliary_model_selection(
                engine::AuxiliaryRole::Stt,
                env_map.get("CTOX_STT_MODEL").map(String::as_str),
            ),
        ),
        (
            engine::AuxiliaryRole::Tts,
            engine::auxiliary_model_selection(
                engine::AuxiliaryRole::Tts,
                env_map.get("CTOX_TTS_MODEL").map(String::as_str),
            ),
        ),
    ]
    .into_iter()
    .filter_map(|(role, selection)| {
        (selection.compute_target == engine::ComputeTarget::Gpu)
            .then_some(auxiliary_role_label(role).to_string())
    })
    .collect()
}

pub(super) fn auxiliary_role_label(role: engine::AuxiliaryRole) -> &'static str {
    match role {
        engine::AuxiliaryRole::Embedding => "embed",
        engine::AuxiliaryRole::Stt => "stt",
        engine::AuxiliaryRole::Tts => "tts",
        engine::AuxiliaryRole::Vision => "vision",
    }
}

pub(super) fn short_gpu_label(model: &str) -> String {
    model_registry::gpu_short_label(model)
        .map(str::to_string)
        .unwrap_or_else(|| compact_model_name(model, 32))
}

pub(super) fn parse_device_layer_weights(raw: Option<&String>) -> BTreeMap<usize, u64> {
    let mut weights = BTreeMap::new();
    let Some(raw) = raw else {
        return weights;
    };
    for chunk in raw.trim_matches('\'').split(';') {
        let Some((gpu, weight)) = chunk.split_once(':') else {
            continue;
        };
        let Ok(gpu_index) = gpu.trim().parse::<usize>() else {
            continue;
        };
        let Ok(weight_value) = weight.trim().parse::<u64>() else {
            continue;
        };
        weights.insert(gpu_index, weight_value);
    }
    weights
}

pub(super) fn estimate_chat_model_memory_mb(model: &str, isq: &str, target_context: usize) -> u64 {
    let base_mb = model_registry::estimated_chat_base_memory_mb(model).unwrap_or(12_000) as f64;
    let isq_factor = match isq.trim().to_ascii_uppercase().as_str() {
        "Q2K" => 0.72,
        "Q3K" => 0.84,
        "Q4K" => 1.0,
        "Q5K" => 1.12,
        "Q6K" => 1.24,
        "Q8K" => 1.40,
        "FP8" => 1.30,
        _ => 1.0,
    };
    let context_factor = 0.88 + 0.12 * ((target_context.max(1024) as f64) / 4096.0).clamp(0.5, 4.0);
    (base_mb * isq_factor * context_factor).round() as u64
}

#[allow(dead_code)]
pub(super) fn estimate_max_context_window(
    cards: &[GpuCardState],
    loaded_model: &str,
    draft_model: &str,
    settings: &BTreeMap<String, String>,
    live_context: usize,
) -> usize {
    if cards.is_empty() {
        return MAX_CONTEXT_WINDOW;
    }
    let target_isq = settings
        .get("CTOX_ENGINE_ISQ")
        .map(String::as_str)
        .unwrap_or("Q4K");
    let base_context = live_context.max(2048);
    let base_memory_mb =
        estimate_chat_model_memory_mb(draft_model, target_isq, base_context).max(1);
    let weights = parse_device_layer_weights(settings.get("CTOX_ENGINE_DEVICE_LAYERS"));
    let selected_gpu_indices = if weights.is_empty() {
        cards.iter().map(|card| card.index).collect::<Vec<_>>()
    } else {
        weights.keys().copied().collect::<Vec<_>>()
    };
    let selected_cards = cards
        .iter()
        .filter(|card| selected_gpu_indices.contains(&card.index))
        .collect::<Vec<_>>();
    if selected_cards.is_empty() {
        return MAX_CONTEXT_WINDOW;
    }
    let total_budget_mb = selected_cards
        .iter()
        .map(|card| ((card.total_mb as f64) * 0.96).floor() as u64)
        .sum::<u64>();
    let aux_usage_mb = selected_cards
        .iter()
        .map(|card| {
            card.allocations
                .iter()
                .filter(|allocation| allocation.model != loaded_model)
                .map(|allocation| allocation.used_mb)
                .sum::<u64>()
        })
        .sum::<u64>();
    if total_budget_mb <= aux_usage_mb {
        return 2048;
    }
    let usable_chat_budget_mb = total_budget_mb - aux_usage_mb;
    let low_context = 2048usize;
    let high_context = 8192usize;
    let low_memory_mb =
        estimate_chat_model_memory_mb(draft_model, target_isq, low_context).max(base_memory_mb);
    let high_memory_mb =
        estimate_chat_model_memory_mb(draft_model, target_isq, high_context).max(low_memory_mb + 1);
    let slope_mb_per_token =
        (high_memory_mb.saturating_sub(low_memory_mb)) as f64 / (high_context - low_context) as f64;
    let fixed_overhead_mb =
        (low_memory_mb as f64 - slope_mb_per_token * low_context as f64).max(0.0);
    let budget_f = usable_chat_budget_mb as f64;
    let estimated_context = if budget_f <= fixed_overhead_mb {
        low_context
    } else {
        ((budget_f - fixed_overhead_mb) / slope_mb_per_token).round() as usize
    };
    estimated_context.clamp(2048, MAX_CONTEXT_WINDOW)
}
