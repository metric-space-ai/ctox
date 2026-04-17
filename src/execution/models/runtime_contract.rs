// Origin: CTOX
// Modified for CTOX from local inference runtime contract work.
// License: Apache-2.0

use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const CHAT_CAPACITY_CONTRACT_RELATIVE_PATH: &str = "runtime/chat_capacity_contract.json";
const RUNTIME_OWNERSHIP_STATE_RELATIVE_PATH: &str = "runtime/runtime_ownership.json";
const LEGACY_GPU_LEASE_LEDGER_RELATIVE_PATH: &str = "runtime/backend_gpu_leases.json";
const PROXY_PID_FILE_NAME: &str = "ctox_proxy.pid";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum BackendRole {
    Chat,
    Embedding,
    Stt,
    Tts,
    Vision,
}

impl BackendRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Embedding => "embedding",
            Self::Stt => "stt",
            Self::Tts => "tts",
            Self::Vision => "vision",
        }
    }

    pub fn pid_file_name(self) -> &'static str {
        match self {
            Self::Chat => "ctox_chat_backend.pid",
            Self::Embedding => "ctox_embedding_backend.pid",
            Self::Stt => "ctox_stt_backend.pid",
            Self::Tts => "ctox_tts_backend.pid",
            Self::Vision => "ctox_vision_backend.pid",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeResidencyPhase {
    Starting,
    Active,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatGpuCapacityRequirement {
    pub gpu_index: usize,
    pub name: String,
    pub total_mb: u64,
    pub desktop_reserve_mb: u64,
    pub aux_reserve_mb: u64,
    pub chat_budget_mb: u64,
    pub backend_overhead_mb: u64,
    pub activation_overhead_mb: u64,
    pub load_peak_overhead_mb: u64,
    pub required_free_mb: u64,
    pub free_headroom_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatCapacityContract {
    pub model: String,
    pub preset: String,
    pub min_context_tokens: u32,
    pub max_seq_len: u32,
    pub hardware_fingerprint: String,
    pub generated_at: String,
    pub rationale: Vec<String>,
    pub gpus: Vec<ChatGpuCapacityRequirement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackendGpuLease {
    pub role: BackendRole,
    pub model: String,
    pub pid: Option<u32>,
    pub visible_devices: Vec<usize>,
    pub reserved_mb_by_gpu: BTreeMap<usize, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BackendGpuLeaseLedger {
    pub leases: Vec<BackendGpuLease>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackendRuntimeResidency {
    pub role: BackendRole,
    pub phase: RuntimeResidencyPhase,
    pub model: String,
    pub pid: Option<u32>,
    pub port: Option<u16>,
    pub health_path: Option<String>,
    pub launcher_kind: Option<String>,
    pub compute_target: Option<String>,
    pub visible_devices: Vec<usize>,
    pub reserved_mb_by_gpu: BTreeMap<usize, u64>,
    pub updated_at_epoch_secs: u64,
}

impl BackendRuntimeResidency {
    pub fn from_lease(lease: BackendGpuLease, phase: RuntimeResidencyPhase) -> Self {
        Self {
            role: lease.role,
            phase,
            model: lease.model,
            pid: lease.pid,
            port: None,
            health_path: None,
            launcher_kind: None,
            compute_target: None,
            visible_devices: lease.visible_devices,
            reserved_mb_by_gpu: lease.reserved_mb_by_gpu,
            updated_at_epoch_secs: current_epoch_secs(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProxyRuntimeResidency {
    pub phase: RuntimeResidencyPhase,
    pub pid: Option<u32>,
    pub host: String,
    pub port: u16,
    pub health_path: String,
    pub updated_at_epoch_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeOwnershipState {
    pub version: u32,
    pub proxy: Option<ProxyRuntimeResidency>,
    pub workloads: Vec<BackendRuntimeResidency>,
}

impl Default for RuntimeOwnershipState {
    fn default() -> Self {
        Self {
            version: 1,
            proxy: None,
            workloads: Vec::new(),
        }
    }
}

pub fn chat_capacity_contract_path(root: &Path) -> PathBuf {
    root.join(CHAT_CAPACITY_CONTRACT_RELATIVE_PATH)
}

pub fn runtime_ownership_state_path(root: &Path) -> PathBuf {
    root.join(RUNTIME_OWNERSHIP_STATE_RELATIVE_PATH)
}

pub fn persist_chat_capacity_contract(root: &Path, contract: &ChatCapacityContract) -> Result<()> {
    let path = chat_capacity_contract_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create chat contract dir {}", parent.display()))?;
    }
    let bytes =
        serde_json::to_vec_pretty(contract).context("failed to encode chat capacity contract")?;
    std::fs::write(&path, bytes)
        .with_context(|| format!("failed to write chat capacity contract {}", path.display()))
}

pub fn load_chat_capacity_contract(root: &Path) -> Result<Option<ChatCapacityContract>> {
    let path = chat_capacity_contract_path(root);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)
        .with_context(|| format!("failed to read chat capacity contract {}", path.display()))?;
    let contract = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse chat capacity contract {}", path.display()))?;
    Ok(Some(contract))
}

pub fn clear_chat_capacity_contract(root: &Path) -> Result<()> {
    let path = chat_capacity_contract_path(root);
    let _ = std::fs::remove_file(&path);
    Ok(())
}

pub fn load_runtime_ownership_state(root: &Path) -> Result<RuntimeOwnershipState> {
    let path = runtime_ownership_state_path(root);
    let legacy_path = legacy_backend_gpu_lease_ledger_path(root);
    let path_exists = path.exists();
    let legacy_exists = legacy_path.exists();
    let mut state = if path.exists() {
        let bytes = std::fs::read(&path).with_context(|| {
            format!("failed to read runtime ownership state {}", path.display())
        })?;
        let mut decoded: RuntimeOwnershipState =
            serde_json::from_slice(&bytes).with_context(|| {
                format!("failed to parse runtime ownership state {}", path.display())
            })?;
        if decoded.version == 0 {
            decoded.version = 1;
        }
        decoded
    } else {
        migrate_legacy_backend_gpu_lease_ledger(root)?
    };
    let original = state.clone();
    prune_dead_runtime_residency(root, &mut state);
    if legacy_exists || (path_exists && state != original) {
        persist_runtime_ownership_state(root, &state)?;
    }
    Ok(state)
}

pub fn persist_runtime_ownership_state(root: &Path, state: &RuntimeOwnershipState) -> Result<()> {
    let path = runtime_ownership_state_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create runtime ownership dir {}",
                parent.display()
            )
        })?;
    }
    let bytes =
        serde_json::to_vec_pretty(state).context("failed to encode runtime ownership state")?;
    std::fs::write(&path, bytes)
        .with_context(|| format!("failed to write runtime ownership state {}", path.display()))?;
    let _ = std::fs::remove_file(legacy_backend_gpu_lease_ledger_path(root));
    Ok(())
}

pub fn sync_backend_runtime_residency(
    root: &Path,
    residency: BackendRuntimeResidency,
) -> Result<()> {
    let mut state = load_runtime_ownership_state(root)?;
    state.workloads.retain(|entry| entry.role != residency.role);
    state.workloads.push(residency);
    state.workloads.sort_by_key(|entry| entry.role);
    persist_runtime_ownership_state(root, &state)
}

pub fn release_backend_runtime_residency(root: &Path, role: BackendRole) -> Result<()> {
    let mut state = load_runtime_ownership_state(root)?;
    state.workloads.retain(|entry| entry.role != role);
    persist_runtime_ownership_state(root, &state)
}

pub fn sync_proxy_runtime_residency(root: &Path, residency: ProxyRuntimeResidency) -> Result<()> {
    let mut state = load_runtime_ownership_state(root)?;
    state.proxy = Some(residency);
    persist_runtime_ownership_state(root, &state)
}

pub fn release_proxy_runtime_residency(root: &Path) -> Result<()> {
    let mut state = load_runtime_ownership_state(root)?;
    state.proxy = None;
    persist_runtime_ownership_state(root, &state)
}

pub fn persist_backend_gpu_lease(root: &Path, lease: BackendGpuLease) -> Result<()> {
    sync_backend_runtime_residency(
        root,
        BackendRuntimeResidency::from_lease(lease, RuntimeResidencyPhase::Active),
    )
}

pub fn release_backend_gpu_lease(root: &Path, role: BackendRole) -> Result<()> {
    release_backend_runtime_residency(root, role)
}

pub fn reserved_gpu_mb_by_role(
    root: &Path,
    excluding: Option<BackendRole>,
) -> Result<BTreeMap<usize, u64>> {
    let state = load_runtime_ownership_state(root)?;
    let mut totals = BTreeMap::new();
    for residency in state.workloads {
        if Some(residency.role) == excluding {
            continue;
        }
        for (gpu_index, reserved_mb) in residency.reserved_mb_by_gpu {
            let entry = totals.entry(gpu_index).or_insert(0u64);
            *entry = (*entry).saturating_add(reserved_mb);
        }
    }
    Ok(totals)
}

pub fn current_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn migrate_legacy_backend_gpu_lease_ledger(root: &Path) -> Result<RuntimeOwnershipState> {
    let path = legacy_backend_gpu_lease_ledger_path(root);
    if !path.exists() {
        return Ok(RuntimeOwnershipState::default());
    }
    let bytes = std::fs::read(&path)
        .with_context(|| format!("failed to read backend GPU lease ledger {}", path.display()))?;
    let ledger: BackendGpuLeaseLedger = serde_json::from_slice(&bytes).with_context(|| {
        format!(
            "failed to parse backend GPU lease ledger {}",
            path.display()
        )
    })?;
    let mut state = RuntimeOwnershipState::default();
    state.workloads = ledger
        .leases
        .into_iter()
        .map(|lease| BackendRuntimeResidency::from_lease(lease, RuntimeResidencyPhase::Active))
        .collect();
    Ok(state)
}

fn prune_dead_runtime_residency(root: &Path, state: &mut RuntimeOwnershipState) {
    state
        .workloads
        .retain(|entry| backend_residency_is_alive(root, entry));
    if state
        .proxy
        .as_ref()
        .map(|entry| !proxy_residency_is_alive(root, entry))
        .unwrap_or(false)
    {
        state.proxy = None;
    }
}

fn backend_residency_is_alive(root: &Path, residency: &BackendRuntimeResidency) -> bool {
    let Some(pid) = residency.pid else {
        return false;
    };
    let pid_path = root.join("runtime").join(residency.role.pid_file_name());
    pid_path.exists() && process_id_is_alive(pid)
}

fn proxy_residency_is_alive(root: &Path, residency: &ProxyRuntimeResidency) -> bool {
    let Some(pid) = residency.pid else {
        return false;
    };
    let pid_path = root.join("runtime").join(PROXY_PID_FILE_NAME);
    pid_path.exists() && process_id_is_alive(pid)
}

fn legacy_backend_gpu_lease_ledger_path(root: &Path) -> PathBuf {
    root.join(LEGACY_GPU_LEASE_LEDGER_RELATIVE_PATH)
}

fn process_id_is_alive(pid: u32) -> bool {
    match Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
    {
        Ok(status) => status.success(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn make_temp_root() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ctox-runtime-contract-test-{unique}-{}",
            TEST_ROOT_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        root
    }

    fn write_pid_file(root: &Path, file_name: &str) {
        std::fs::write(
            root.join("runtime").join(file_name),
            format!("{}\n", std::process::id()),
        )
        .unwrap();
    }

    #[test]
    fn runtime_ownership_tracks_proxy_and_backend_residency() {
        let root = make_temp_root();
        write_pid_file(&root, BackendRole::Chat.pid_file_name());
        write_pid_file(&root, PROXY_PID_FILE_NAME);

        sync_proxy_runtime_residency(
            &root,
            ProxyRuntimeResidency {
                phase: RuntimeResidencyPhase::Active,
                pid: Some(std::process::id()),
                host: "127.0.0.1".to_string(),
                port: 12434,
                health_path: "/ctox/telemetry".to_string(),
                updated_at_epoch_secs: current_epoch_secs(),
            },
        )
        .unwrap();
        sync_backend_runtime_residency(
            &root,
            BackendRuntimeResidency {
                role: BackendRole::Chat,
                phase: RuntimeResidencyPhase::Active,
                model: "openai/gpt-oss-20b".to_string(),
                pid: Some(std::process::id()),
                port: Some(1234),
                health_path: Some("/health".to_string()),
                launcher_kind: Some("engine".to_string()),
                compute_target: None,
                visible_devices: vec![1],
                reserved_mb_by_gpu: BTreeMap::from([(1usize, 4096u64)]),
                updated_at_epoch_secs: current_epoch_secs(),
            },
        )
        .unwrap();

        let state = load_runtime_ownership_state(&root).unwrap();
        assert_eq!(state.proxy.as_ref().map(|entry| entry.port), Some(12434));
        assert_eq!(state.workloads.len(), 1);
        assert_eq!(state.workloads[0].port, Some(1234));
        assert_eq!(
            reserved_gpu_mb_by_role(&root, None)
                .unwrap()
                .get(&1)
                .copied(),
            Some(4096)
        );
    }

    #[test]
    fn legacy_gpu_lease_ledger_migrates_into_runtime_ownership_view() {
        let root = make_temp_root();
        write_pid_file(&root, BackendRole::Embedding.pid_file_name());
        let legacy_path = legacy_backend_gpu_lease_ledger_path(&root);
        std::fs::write(
            &legacy_path,
            serde_json::to_vec_pretty(&BackendGpuLeaseLedger {
                leases: vec![BackendGpuLease {
                    role: BackendRole::Embedding,
                    model: "Qwen/Qwen3-Embedding-0.6B".to_string(),
                    pid: Some(std::process::id()),
                    visible_devices: vec![2],
                    reserved_mb_by_gpu: BTreeMap::from([(2usize, 1024u64)]),
                }],
            })
            .unwrap(),
        )
        .unwrap();

        let state = load_runtime_ownership_state(&root).unwrap();
        assert_eq!(state.workloads.len(), 1);
        assert_eq!(state.workloads[0].role, BackendRole::Embedding);
        assert!(runtime_ownership_state_path(&root).exists());
        assert!(!legacy_path.exists());
        assert_eq!(
            reserved_gpu_mb_by_role(&root, None)
                .unwrap()
                .get(&2)
                .copied(),
            Some(1024)
        );
    }

    #[test]
    fn releasing_runtime_residency_clears_entries() {
        let root = make_temp_root();
        write_pid_file(&root, BackendRole::Tts.pid_file_name());
        write_pid_file(&root, PROXY_PID_FILE_NAME);

        sync_proxy_runtime_residency(
            &root,
            ProxyRuntimeResidency {
                phase: RuntimeResidencyPhase::Starting,
                pid: Some(std::process::id()),
                host: "127.0.0.1".to_string(),
                port: 12434,
                health_path: "/ctox/telemetry".to_string(),
                updated_at_epoch_secs: current_epoch_secs(),
            },
        )
        .unwrap();
        persist_backend_gpu_lease(
            &root,
            BackendGpuLease {
                role: BackendRole::Tts,
                model: "Qwen/Qwen3-TTS-12Hz-0.6B-Base".to_string(),
                pid: Some(std::process::id()),
                visible_devices: vec![0],
                reserved_mb_by_gpu: BTreeMap::from([(0usize, 512u64)]),
            },
        )
        .unwrap();

        release_proxy_runtime_residency(&root).unwrap();
        release_backend_runtime_residency(&root, BackendRole::Tts).unwrap();

        let state = load_runtime_ownership_state(&root).unwrap();
        assert!(state.proxy.is_none());
        assert!(state.workloads.is_empty());
    }
}
