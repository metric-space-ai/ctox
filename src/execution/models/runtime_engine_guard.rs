use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use std::path::Path;
use std::process::Command;
use std::process::Output;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use crate::inference::runtime_env;

const NVIDIA_SMI_TIMEOUT_SECS: u64 = 10;
const ENGINE_DOCTOR_TIMEOUT_SECS: u64 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostAccelerationRequirement {
    CpuOnly,
    NvidiaCuda,
    AppleMetal,
}

impl HostAccelerationRequirement {
    fn required_feature(self) -> Option<&'static str> {
        match self {
            Self::CpuOnly => None,
            Self::NvidiaCuda => Some("cuda"),
            Self::AppleMetal => Some("metal"),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::CpuOnly => "cpu-only",
            Self::NvidiaCuda => "nvidia-cuda",
            Self::AppleMetal => "apple-metal",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct EngineBinaryBuildFeatures {
    cuda: bool,
    metal: bool,
    cudnn: bool,
    nccl: bool,
    flash_attn: bool,
    flash_attn_v3: bool,
    accelerate: bool,
    mkl: bool,
}

impl EngineBinaryBuildFeatures {
    fn supports(self, requirement: HostAccelerationRequirement) -> bool {
        match requirement {
            HostAccelerationRequirement::CpuOnly => true,
            HostAccelerationRequirement::NvidiaCuda => self.cuda,
            HostAccelerationRequirement::AppleMetal => self.metal,
        }
    }

    fn label(self) -> String {
        let mut features = Vec::new();
        if self.cuda {
            features.push("cuda");
        }
        if self.metal {
            features.push("metal");
        }
        if self.cudnn {
            features.push("cudnn");
        }
        if self.nccl {
            features.push("nccl");
        }
        if self.flash_attn {
            features.push("flash-attn");
        }
        if self.flash_attn_v3 {
            features.push("flash-attn-v3");
        }
        if self.accelerate {
            features.push("accelerate");
        }
        if self.mkl {
            features.push("mkl");
        }
        if features.is_empty() {
            "cpu-only".to_string()
        } else {
            features.join(" ")
        }
    }
}

#[derive(Debug, Deserialize)]
struct DoctorReport {
    system: DoctorSystem,
}

#[derive(Debug, Deserialize)]
struct DoctorSystem {
    build: DoctorBuild,
}

#[derive(Debug, Deserialize)]
struct DoctorBuild {
    #[serde(default)]
    cuda: bool,
    #[serde(default)]
    metal: bool,
    #[serde(default)]
    cudnn: bool,
    #[serde(default)]
    nccl: bool,
    #[serde(default, rename = "flash_attn")]
    flash_attn: bool,
    #[serde(default, rename = "flash_attn_v3")]
    flash_attn_v3: bool,
    #[serde(default)]
    accelerate: bool,
    #[serde(default)]
    mkl: bool,
}

fn parse_host_acceleration_override(raw: &str) -> Option<HostAccelerationRequirement> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "cpu" | "cpu-only" | "none" => Some(HostAccelerationRequirement::CpuOnly),
        "cuda" | "nvidia" | "nvidia-cuda" => Some(HostAccelerationRequirement::NvidiaCuda),
        "metal" | "apple" | "apple-metal" => Some(HostAccelerationRequirement::AppleMetal),
        _ => None,
    }
}

fn detect_host_acceleration_requirement(root: &Path) -> HostAccelerationRequirement {
    if let Ok(raw) = std::env::var("CTOX_TEST_ENGINE_HOST_ACCELERATION") {
        if let Some(requirement) = parse_host_acceleration_override(&raw) {
            return requirement;
        }
    }

    if nvidia_gpu_present(root) {
        return HostAccelerationRequirement::NvidiaCuda;
    }
    if cfg!(target_os = "macos") {
        return HostAccelerationRequirement::AppleMetal;
    }
    HostAccelerationRequirement::CpuOnly
}

fn nvidia_gpu_present(root: &Path) -> bool {
    if let Some(spec) = runtime_env::env_or_config(root, "CTOX_TEST_GPU_TOTALS_MB") {
        if !spec.trim().is_empty() {
            return true;
        }
    }

    let output = command_output_with_timeout(
        Command::new("nvidia-smi").args(["--query-gpu=name", "--format=csv,noheader"]),
        Duration::from_secs(NVIDIA_SMI_TIMEOUT_SECS),
    );
    match output {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| !line.trim().is_empty()),
        _ => false,
    }
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

fn parse_engine_binary_build_features(raw: &[u8]) -> Result<EngineBinaryBuildFeatures> {
    let report: DoctorReport =
        serde_json::from_slice(raw).context("failed to parse ctox-engine doctor json")?;
    Ok(EngineBinaryBuildFeatures {
        cuda: report.system.build.cuda,
        metal: report.system.build.metal,
        cudnn: report.system.build.cudnn,
        nccl: report.system.build.nccl,
        flash_attn: report.system.build.flash_attn,
        flash_attn_v3: report.system.build.flash_attn_v3,
        accelerate: report.system.build.accelerate,
        mkl: report.system.build.mkl,
    })
}

fn inspect_engine_binary_build_features(binary: &Path) -> Result<EngineBinaryBuildFeatures> {
    let mut command = Command::new(binary);
    command
        .args(["doctor", "--json"])
        .env("CUDA_VISIBLE_DEVICES", "")
        .env("NVIDIA_VISIBLE_DEVICES", "void")
        .env("HIP_VISIBLE_DEVICES", "");
    let output = command_output_with_timeout(
        &mut command,
        Duration::from_secs(ENGINE_DOCTOR_TIMEOUT_SECS),
    )
    .with_context(|| format!("failed to run {} doctor --json", binary.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "{} doctor --json failed with status {}: {}",
            binary.display(),
            output.status,
            stderr.trim()
        );
    }
    parse_engine_binary_build_features(&output.stdout)
}

pub fn ensure_engine_binary_matches_host(
    root: &Path,
    binary: &Path,
    allow_cpu_fallback: bool,
) -> Result<()> {
    let requirement = detect_host_acceleration_requirement(root);
    if requirement == HostAccelerationRequirement::CpuOnly {
        return Ok(());
    }

    let features = inspect_engine_binary_build_features(binary)?;
    if allow_cpu_fallback && !features.cuda && !features.metal {
        return Ok(());
    }
    if features.supports(requirement) {
        return Ok(());
    }

    let rebuild_script = root.join("scripts/models/build_engine_for_host.sh");
    let rebuild_hint = if rebuild_script.is_file() {
        format!(
            "Rebuild with {} {}.",
            rebuild_script.display(),
            root.display()
        )
    } else {
        "Rebuild the engine with the host-appropriate acceleration features.".to_string()
    };
    let required_feature = requirement.required_feature().unwrap_or("cpu-only");
    anyhow::bail!(
        "host requires {} support, but {} reports build features [{}]. CTOX refuses CPU-only or wrong-acceleration ctox-engine binaries on GPU hosts. Required feature: {}. {}",
        requirement.label(),
        binary.display(),
        features.label(),
        required_feature,
        rebuild_hint
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_engine_binary_build_features_from_doctor_json() {
        let raw = br#"{
            "system": {
                "build": {
                    "cuda": true,
                    "metal": false,
                    "cudnn": true,
                    "nccl": true,
                    "flash_attn": true,
                    "flash_attn_v3": false,
                    "accelerate": false,
                    "mkl": false
                }
            }
        }"#;
        let features = parse_engine_binary_build_features(raw).unwrap();
        assert!(features.cuda);
        assert!(features.cudnn);
        assert!(features.nccl);
        assert!(features.flash_attn);
        assert_eq!(features.label(), "cuda cudnn nccl flash-attn");
    }

    #[test]
    fn parses_host_acceleration_override_aliases() {
        assert_eq!(
            parse_host_acceleration_override("cuda"),
            Some(HostAccelerationRequirement::NvidiaCuda)
        );
        assert_eq!(
            parse_host_acceleration_override("apple-metal"),
            Some(HostAccelerationRequirement::AppleMetal)
        );
        assert_eq!(
            parse_host_acceleration_override("cpu-only"),
            Some(HostAccelerationRequirement::CpuOnly)
        );
        assert_eq!(parse_host_acceleration_override("bogus"), None);
    }

    #[test]
    fn command_output_with_timeout_times_out_hung_child() {
        let err = command_output_with_timeout(
            Command::new("sh").args(["-c", "sleep 2"]),
            Duration::from_millis(100),
        )
        .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
    }
}
