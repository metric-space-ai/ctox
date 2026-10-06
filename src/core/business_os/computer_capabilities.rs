// Origin: CTOX
// License: AGPL-3.0-only

//! Native operational settings for an opaque, assigned Workjet computer.
//! Endpoint references are resolved by the native computer_endpoints registry,
//! shared by build adapters and TransferEngine, never interpreted as identities.

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const COMPUTER_CAPABILITY_CONTRACT: &str = "ctox.computer-capabilities.v1";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ComputerCapability {
    Build(BuildCapability),
    Storage(StorageCapability),
    Gpu(GpuCapability),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildCapability {
    pub ssh_endpoint_ref: String,
    pub slots: u16,
    pub jobs: u16,
    pub lane_root: String,
    pub disk_floor_gib: u32,
    pub toolchains: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StorageCapability {
    pub endpoint_ref: String,
    pub protocol: StorageProtocol,
    pub root: String,
    pub quota_gib: Option<u64>,
    pub purposes: Vec<StoragePurpose>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageProtocol {
    Ssh,
    Smb,
    Nfs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StoragePurpose {
    Artifacts,
    Backups,
    Exchange,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GpuCapability {
    pub model: String,
    pub vram_gib: u32,
}

impl ComputerCapability {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Build(_) => "build",
            Self::Storage(_) => "storage",
            Self::Gpu(_) => "gpu",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RegisteredComputerCapabilities {
    pub computer_id: String,
    pub agentless: bool,
    pub capabilities: Vec<ComputerCapability>,
}

/// Read native owner-scoped settings; capability names in the browser
/// projection are presentation only and are never an execution grant.
pub fn load_registered_computer_capabilities(
    root: &std::path::Path,
    owner_user_id: &str,
) -> anyhow::Result<Vec<RegisteredComputerCapabilities>> {
    bounded_label(owner_user_id, 256)?;
    let conn = super::store::open_store(root)?;
    let records = super::store::outbound_load_records_by_string_field(
        &conn,
        super::store_workjet_computers::COMPUTERS_COLLECTION,
        "owner_user_id",
        owner_user_id,
    )?;
    let mut computers = Vec::new();
    for record in records {
        if record["status"] != "assigned"
            || record["is_deleted"] == true
            || record["hosting_mode"] == "managed_backend"
        {
            continue;
        }
        let Some(config) = record.get("capability_config") else {
            continue;
        };
        let computer_id = record["id"]
            .as_str()
            .context("computer has no opaque identity")?
            .to_owned();
        let agentless = record
            .get("agentless")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let mut capabilities = serde_json::from_value(config.clone())
            .context("invalid persisted computer capability configuration")?;
        validate_capabilities(&mut capabilities, agentless)?;
        computers.push(RegisteredComputerCapabilities {
            computer_id,
            agentless,
            capabilities,
        });
    }
    computers.sort_by(|left, right| left.computer_id.cmp(&right.computer_id));
    Ok(computers)
}

/// Slot observations come from the native build adapter's remote lease probe,
/// not from the replicated computer row or a browser-supplied capacity claim.
#[derive(Debug, Clone)]
pub struct BuildAvailability {
    pub computer_id: String,
    pub free_slots: u16,
    pub observed_at_ms: i64,
}

#[derive(Debug, Clone)]
pub struct BuildTarget {
    pub computer_id: String,
    pub config: BuildCapability,
    pub free_slots: u16,
}

/// Match the prototype's most-free-slots policy without hard-coded host names.
/// The adapter must still acquire the remote flock lease and check disk floors
/// before spawning; a fresh observation alone is not a reservation.
pub fn select_build_target(
    computers: &[RegisteredComputerCapabilities],
    availability: &[BuildAvailability],
    required_toolchain: &str,
    now_ms: i64,
) -> anyhow::Result<Option<BuildTarget>> {
    bounded_label(required_toolchain, 128)?;
    let mut observations = std::collections::BTreeMap::new();
    for observation in availability {
        anyhow::ensure!(
            observations
                .insert(&observation.computer_id, observation)
                .is_none(),
            "duplicate build availability observation"
        );
    }
    let mut targets = Vec::new();
    for computer in computers.iter().filter(|computer| !computer.agentless) {
        let Some(observation) = observations.get(&computer.computer_id) else {
            continue;
        };
        let Some(age) = now_ms.checked_sub(observation.observed_at_ms) else {
            continue;
        };
        if !(0..=30_000).contains(&age) || observation.free_slots == 0 {
            continue;
        }
        for capability in &computer.capabilities {
            let ComputerCapability::Build(build) = capability else {
                continue;
            };
            if observation.free_slots <= build.slots
                && build
                    .toolchains
                    .iter()
                    .any(|toolchain| toolchain == required_toolchain)
            {
                targets.push(BuildTarget {
                    computer_id: computer.computer_id.clone(),
                    config: build.clone(),
                    free_slots: observation.free_slots,
                });
            }
        }
    }
    targets.sort_by(|left, right| {
        right
            .free_slots
            .cmp(&left.free_slots)
            .then_with(|| left.computer_id.cmp(&right.computer_id))
    });
    Ok(targets.into_iter().next())
}
/// Validate and canonicalize before any durable mutation. A capability name alone never authorizes a build.
pub fn validate_capabilities(
    capabilities: &mut Vec<ComputerCapability>,
    agentless: bool,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        capabilities.len() <= 3,
        "at most three typed capabilities are supported"
    );
    let mut kinds = BTreeSet::new();
    for capability in capabilities.iter_mut() {
        anyhow::ensure!(
            kinds.insert(capability.kind()),
            "duplicate computer capability"
        );
        match capability {
            ComputerCapability::Build(build) => {
                endpoint_ref(&build.ssh_endpoint_ref)?;
                anyhow::ensure!((1..=32).contains(&build.slots), "build slots must be 1..32");
                anyhow::ensure!((1..=64).contains(&build.jobs), "build jobs must be 1..64");
                absolute_root(&build.lane_root)?;
                anyhow::ensure!(
                    build.disk_floor_gib > 0,
                    "build disk floor must be positive"
                );
                anyhow::ensure!(
                    !build.toolchains.is_empty() && build.toolchains.len() <= 16,
                    "build toolchains must contain 1..16 entries"
                );
                let mut toolchains = BTreeSet::new();
                for toolchain in &build.toolchains {
                    bounded_label(toolchain, 128).context("invalid build toolchain")?;
                    toolchains.insert(toolchain.clone());
                }
                build.toolchains = toolchains.into_iter().collect();
            }
            ComputerCapability::Storage(storage) => {
                endpoint_ref(&storage.endpoint_ref)?;
                absolute_root(&storage.root)?;
                anyhow::ensure!(
                    storage.quota_gib != Some(0),
                    "storage quota must be positive or null"
                );
                anyhow::ensure!(
                    !storage.purposes.is_empty() && storage.purposes.len() <= 3,
                    "storage requires 1..3 purposes"
                );
                storage.purposes = storage
                    .purposes
                    .iter()
                    .copied()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
            }
            ComputerCapability::Gpu(gpu) => {
                bounded_label(&gpu.model, 256).context("invalid GPU model")?;
                anyhow::ensure!(
                    (1..=16384).contains(&gpu.vram_gib),
                    "GPU VRAM must be 1..16384 GiB"
                );
            }
        }
    }
    if agentless {
        anyhow::ensure!(
            kinds.len() == 1 && kinds.contains("storage"),
            "an agentless computer must have storage as its only capability"
        );
    }
    capabilities.sort_by_key(|capability| capability.kind());
    Ok(())
}

fn bounded_label(value: &str, limit: usize) -> anyhow::Result<()> {
    anyhow::ensure!(
        !value.is_empty()
            && value == value.trim()
            && value.chars().count() <= limit
            && !value.chars().any(char::is_control),
        "invalid bounded label"
    );
    Ok(())
}

fn endpoint_ref(value: &str) -> anyhow::Result<()> {
    bounded_label(value, 256)?;
    anyhow::ensure!(
        value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte)),
        "endpoint_ref must be an opaque endpoint identifier, not a URL, password or shell command"
    );
    Ok(())
}

fn absolute_root(value: &str) -> anyhow::Result<()> {
    bounded_label(value, 4096)?;
    anyhow::ensure!(
        value.starts_with('/') && !value.split('/').any(|part| matches!(part, "." | "..")),
        "computer root must be an absolute path without traversal components"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn storage() -> serde_json::Value {
        json!({"kind":"storage", "endpoint_ref":"nas-storage-1", "protocol":"ssh",
            "root":"/volume1/artifacts", "quota_gib":null, "purposes":["exchange","artifacts","exchange"]})
    }

    #[test]
    fn agentless_storage_is_canonical_and_credentials_are_not_configuration() -> anyhow::Result<()>
    {
        let mut caps: Vec<ComputerCapability> = serde_json::from_value(json!([storage()]))?;
        validate_capabilities(&mut caps, true)?;
        assert_eq!(
            serde_json::to_value(&caps)?[0]["purposes"],
            json!(["artifacts", "exchange"])
        );
        for forbidden in ["password", "private_key", "credential", "hostname"] {
            let mut cap = storage();
            cap[forbidden] = json!("must-not-be-stored");
            assert!(serde_json::from_value::<ComputerCapability>(cap).is_err());
        }
        Ok(())
    }

    #[test]
    fn invalid_capacity_roots_endpoints_and_agentless_execution_fail_closed() -> anyhow::Result<()>
    {
        let build = json!({"kind":"build", "ssh_endpoint_ref":"ssh-gpu4", "slots":2, "jobs":5,
            "lane_root":"/home/metricspace/build-lane", "disk_floor_gib":30, "toolchains":["rust-1.93"]});
        for (field, value) in [
            ("slots", json!(0)),
            ("jobs", json!(65)),
            ("lane_root", json!("/tmp/../etc")),
            ("disk_floor_gib", json!(0)),
            ("ssh_endpoint_ref", json!("ssh://user:password@host")),
            ("toolchains", json!([])),
        ] {
            let mut invalid = build.clone();
            invalid[field] = value;
            let mut caps = serde_json::from_value(json!([invalid]))?;
            assert!(validate_capabilities(&mut caps, false).is_err(), "{field}");
        }
        let mut duplicate = serde_json::from_value(json!([storage(), storage()]))?;
        assert!(validate_capabilities(&mut duplicate, false).is_err());
        let mut execution = serde_json::from_value(json!([build, storage()]))?;
        assert!(validate_capabilities(&mut execution, true).is_err());
        assert!(validate_capabilities(&mut vec![], true).is_err());
        Ok(())
    }
}
