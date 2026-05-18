// Origin: CTOX
// Modified for CTOX from local inference runtime contract work.
// License: Apache-2.0

use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::persistence;

const LEGACY_GPU_LEASE_LEDGER_RELATIVE_PATH: &str = "runtime/backend_gpu_leases.json";
const CHAT_CAPACITY_CONTRACT_STORAGE_KEY: &str = "chat_capacity_contract";
const RUNTIME_OWNERSHIP_STATE_STORAGE_KEY: &str = "runtime_ownership_state";

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
pub struct RuntimeOwnershipState {
    pub version: u32,
    pub workloads: Vec<BackendRuntimeResidency>,
}

impl Default for RuntimeOwnershipState {
    fn default() -> Self {
        Self {
            version: 1,
            workloads: Vec::new(),
        }
    }
}

pub fn persist_chat_capacity_contract(root: &Path, contract: &ChatCapacityContract) -> Result<()> {
    persistence::store_json_payload(root, CHAT_CAPACITY_CONTRACT_STORAGE_KEY, Some(contract))
}

pub fn load_chat_capacity_contract(root: &Path) -> Result<Option<ChatCapacityContract>> {
    persistence::load_json_payload(root, CHAT_CAPACITY_CONTRACT_STORAGE_KEY)
}

pub fn clear_chat_capacity_contract(root: &Path) -> Result<()> {
    persistence::store_json_payload::<ChatCapacityContract>(
        root,
        CHAT_CAPACITY_CONTRACT_STORAGE_KEY,
        None,
    )
}

pub fn load_runtime_ownership_state(root: &Path) -> Result<RuntimeOwnershipState> {
    let legacy_path = legacy_backend_gpu_lease_ledger_path(root);
    let legacy_exists = legacy_path.exists();
    let mut state = persistence::load_json_payload(root, RUNTIME_OWNERSHIP_STATE_STORAGE_KEY)?
        .unwrap_or_else(RuntimeOwnershipState::default);
    let original = state.clone();
    prune_dead_runtime_residency(root, &mut state);
    if legacy_exists || state != original {
        persist_runtime_ownership_state(root, &state)?;
    }
    Ok(state)
}

pub fn persist_runtime_ownership_state(root: &Path, state: &RuntimeOwnershipState) -> Result<()> {
    persistence::store_json_payload(root, RUNTIME_OWNERSHIP_STATE_STORAGE_KEY, Some(state))?;
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

fn prune_dead_runtime_residency(root: &Path, state: &mut RuntimeOwnershipState) {
    state
        .workloads
        .retain(|entry| backend_residency_is_alive(root, entry));
}

fn backend_residency_is_alive(root: &Path, residency: &BackendRuntimeResidency) -> bool {
    let Some(pid) = residency.pid else {
        return false;
    };
    let pid_path = root.join("runtime").join(residency.role.pid_file_name());
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
    fn runtime_ownership_tracks_backend_residency() {
        let root = make_temp_root();
        write_pid_file(&root, BackendRole::Chat.pid_file_name());
        sync_backend_runtime_residency(
            &root,
            BackendRuntimeResidency {
                role: BackendRole::Chat,
                phase: RuntimeResidencyPhase::Active,
                model: "openai/gpt-oss-120b".to_string(),
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
        assert!(persistence::load_json_payload::<RuntimeOwnershipState>(
            &root,
            RUNTIME_OWNERSHIP_STATE_STORAGE_KEY
        )
        .unwrap()
        .is_some());
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

        release_backend_runtime_residency(&root, BackendRole::Tts).unwrap();

        let state = load_runtime_ownership_state(&root).unwrap();
        assert!(state.workloads.is_empty());
    }
}
