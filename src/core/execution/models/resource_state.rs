use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::process::Command;
use std::process::Output;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::time::Instant;

const NVIDIA_SMI_TIMEOUT_SECS: u64 = 10;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GpuLiveState {
    pub index: usize,
    pub uuid: Option<String>,
    pub name: String,
    pub total_mb: u64,
    pub used_mb: u64,
    pub free_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceSnapshot {
    pub gpus: Vec<GpuLiveState>,
    pub source: String,
}

impl ResourceSnapshot {
    pub fn gpu(&self, index: usize) -> Option<&GpuLiveState> {
        self.gpus.iter().find(|gpu| gpu.index == index)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GpuProcessLiveState {
    pub gpu_index: usize,
    pub gpu_uuid: Option<String>,
    pub pid: u32,
    pub used_mb: u64,
    pub process_name: String,
    pub command: Option<String>,
}

pub fn inspect_resource_snapshot() -> Option<ResourceSnapshot> {
    if let Ok(override_json) = std::env::var("CTOX_RESOURCE_SNAPSHOT_JSON") {
        if !override_json.trim().is_empty() {
            return serde_json::from_str(&override_json).ok();
        }
    }
    let output = command_output_with_timeout(
        Command::new("nvidia-smi").args([
            "--query-gpu=index,uuid,name,memory.total,memory.used,memory.free",
            "--format=csv,noheader,nounits",
        ]),
        Duration::from_secs(NVIDIA_SMI_TIMEOUT_SECS),
    )
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let mut gpus = Vec::new();
    for line in stdout.lines() {
        let parts = line
            .split(',')
            .map(|chunk| chunk.trim())
            .collect::<Vec<_>>();
        if parts.len() < 6 {
            continue;
        }
        let Ok(index) = parts[0].parse::<usize>() else {
            continue;
        };
        let Ok(total_mb) = parts[3].parse::<u64>() else {
            continue;
        };
        let Ok(used_mb) = parts[4].parse::<u64>() else {
            continue;
        };
        let Ok(free_mb) = parts[5].parse::<u64>() else {
            continue;
        };
        gpus.push(GpuLiveState {
            index,
            uuid: Some(parts[1].to_string()).filter(|value| !value.is_empty()),
            name: parts[2].to_string(),
            total_mb,
            used_mb,
            free_mb,
        });
    }
    if gpus.is_empty() {
        return None;
    }
    Some(ResourceSnapshot {
        gpus,
        source: "nvidia-smi".to_string(),
    })
}

pub fn inspect_gpu_process_snapshot() -> Option<Vec<GpuProcessLiveState>> {
    if let Ok(override_json) = std::env::var("CTOX_GPU_PROCESS_SNAPSHOT_JSON") {
        if !override_json.trim().is_empty() {
            return serde_json::from_str(&override_json).ok();
        }
    }
    let resources = inspect_resource_snapshot()?;
    let uuid_to_index = resources
        .gpus
        .iter()
        .filter_map(|gpu| gpu.uuid.as_ref().map(|uuid| (uuid.clone(), gpu.index)))
        .collect::<BTreeMap<_, _>>();
    let output = command_output_with_timeout(
        Command::new("nvidia-smi").args([
            "--query-compute-apps=gpu_uuid,pid,used_memory,process_name",
            "--format=csv,noheader,nounits",
        ]),
        Duration::from_secs(NVIDIA_SMI_TIMEOUT_SECS),
    )
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let mut processes = Vec::new();
    for line in stdout.lines() {
        let parts = line
            .split(',')
            .map(|chunk| chunk.trim())
            .collect::<Vec<_>>();
        if parts.len() < 4 {
            continue;
        }
        let gpu_uuid = Some(parts[0].to_string()).filter(|value| !value.is_empty());
        let Some(gpu_index) = gpu_uuid
            .as_ref()
            .and_then(|uuid| uuid_to_index.get(uuid).copied())
        else {
            continue;
        };
        let Ok(pid) = parts[1].parse::<u32>() else {
            continue;
        };
        let Ok(used_mb) = parts[2].parse::<u64>() else {
            continue;
        };
        processes.push(GpuProcessLiveState {
            gpu_index,
            gpu_uuid,
            pid,
            used_mb,
            process_name: parts[3].to_string(),
            command: process_command(pid),
        });
    }
    Some(processes)
}

fn process_command(pid: u32) -> Option<String> {
    let output = Command::new("ps")
        .args(["-ww", "-o", "command=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let command = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!command.is_empty()).then_some(command)
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
