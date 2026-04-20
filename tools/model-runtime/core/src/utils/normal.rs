#![allow(dead_code, unused)]

use std::{fmt::Display, str::FromStr};

use anyhow::Result;
use candle_core::{DType, Device, Tensor};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::moe::{MoEExecutionPolicy, MoEExpertsBackend};

#[derive(Clone, Copy, Default, Debug, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "pyo3_macros", pyo3::pyclass(eq, eq_int))]
/// DType for the model.
///
/// If the model is quantized, this is ignored so it is reasonable to use the [`Default`] impl.
///
/// Note: When using `Auto`, fallback pattern is: BF16 -> F16 -> 32
pub enum ModelDType {
    #[default]
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "bf16")]
    BF16,
    #[serde(rename = "f16")]
    F16,
    #[serde(rename = "f32")]
    F32,
}

impl Display for ModelDType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::BF16 => write!(f, "bf16"),
            Self::F16 => write!(f, "f16"),
            Self::F32 => write!(f, "f32"),
        }
    }
}

impl FromStr for ModelDType {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "bf16" => Ok(Self::BF16),
            "f16" => Ok(Self::F16),
            "f32" => Ok(Self::F32),
            other => Err(format!("Model DType `{other}` is not supported.")),
        }
    }
}

/// Type which can be converted to a DType
pub trait TryIntoDType {
    fn try_into_dtype(&self, devices: &[&Device]) -> Result<DType>;
}

impl TryIntoDType for DType {
    fn try_into_dtype(&self, _: &[&Device]) -> Result<DType> {
        info!("DType selected is {self:?}.");
        if !matches!(self, DType::BF16 | DType::F32 | DType::F64 | DType::F16) {
            anyhow::bail!("DType must be one of BF16, F16, F32, F64");
        }
        Ok(*self)
    }
}

#[cfg(feature = "cuda")]
fn get_dtypes() -> Vec<DType> {
    use std::process::Command;

    // >= is supported
    const MIN_BF16_CC: usize = 800;
    // >= is supported
    const MIN_F16_CC: usize = 530;

    let raw_out = Command::new("nvidia-smi")
        .arg("--query-gpu=compute_cap")
        .arg("--format=csv")
        .output()
        .expect("Failed to run `nvidia-smi` but CUDA is selected.")
        .stdout;
    let out = String::from_utf8(raw_out).expect("`nvidia-smi` did not return valid utf8");
    // This reduce-min will always return at least one value so unwrap is OK.
    let min_cc = out
        .split('\n')
        .skip(1)
        .filter(|cc| !cc.trim().is_empty())
        .map(|cc| cc.trim().parse::<f32>().unwrap())
        .reduce(|a, b| if a < b { a } else { b })
        .unwrap();
    info!("Detected minimum CUDA compute capability {min_cc}");
    // 7.5 -> 750
    #[allow(clippy::cast_possible_truncation)]
    let min_cc = (min_cc * 100.) as usize;

    let mut dtypes = Vec::new();
    if min_cc >= MIN_BF16_CC {
        dtypes.push(DType::BF16);
    } else {
        info!("Skipping BF16 because CC < 8.0");
    }
    if min_cc >= MIN_F16_CC {
        dtypes.push(DType::F16);
    } else {
        info!("Skipping F16 because CC < 5.3");
    }
    dtypes
}

fn get_dtypes_non_cuda() -> Vec<DType> {
    vec![DType::BF16, DType::F16]
}

#[cfg(not(feature = "cuda"))]
fn get_dtypes() -> Vec<DType> {
    get_dtypes_non_cuda()
}

fn determine_auto_dtype_all(devices: &[&Device]) -> candle_core::Result<DType> {
    // We can safely use bf16 for accelerate because we cast up to f32 in all matmuls anyway.
    #[cfg(feature = "accelerate")]
    return Ok(DType::BF16);
    #[cfg(not(feature = "accelerate"))]
    {
        let dev_dtypes = get_dtypes();
        for dtype in get_dtypes_non_cuda()
            .iter()
            .filter(|x| dev_dtypes.contains(x))
        {
            let mut results = Vec::new();
            for device in devices {
                // Try a matmul
                let x = Tensor::zeros((2, 2), *dtype, device)?;
                results.push(x.matmul(&x));
            }
            if results.iter().all(|x| x.is_ok()) {
                return Ok(*dtype);
            } else {
                for result in results {
                    match result {
                        Ok(_) => (),
                        Err(e) => match e {
                            // For CUDA
                            candle_core::Error::UnsupportedDTypeForOp(_, _) => continue,
                            // Accelerate backend doesn't support f16/bf16
                            // Metal backend doesn't support f16
                            candle_core::Error::Msg(_) => continue,
                            // This is when the metal backend doesn't support bf16
                            candle_core::Error::Metal(_) => continue,
                            // If running with RUST_BACKTRACE=1
                            candle_core::Error::WithBacktrace { .. } => continue,
                            other => return Err(other),
                        },
                    }
                }
            }
        }
        Ok(DType::F32)
    }
}

impl TryIntoDType for ModelDType {
    fn try_into_dtype(&self, devices: &[&Device]) -> Result<DType> {
        let dtype = match self {
            Self::Auto => Ok(determine_auto_dtype_all(devices).map_err(anyhow::Error::msg)?),
            Self::BF16 => Ok(DType::BF16),
            Self::F16 => Ok(DType::F16),
            Self::F32 => Ok(DType::F32),
        };
        info!("DType selected is {:?}.", dtype.as_ref().unwrap());
        dtype
    }
}

/// Returns `true` if the given device has integrated/unified memory where CPU and GPU
/// share the same physical memory. This includes:
/// - Metal (Apple Silicon)
/// - CUDA integrated GPUs (e.g. NVIDIA Grace Hopper, Grace Blackwell)
///
/// On such systems, loading tensors to CPU first provides no memory benefit.
pub fn is_integrated_gpu(device: &Device) -> bool {
    match device {
        #[cfg(feature = "metal")]
        Device::Metal(_) => true,
        #[cfg(feature = "cuda")]
        Device::Cuda(dev) => {
            use candle_core::cuda::cudarc::driver::{result, sys};
            let ordinal = dev.cuda_stream().context().ordinal();
            #[allow(clippy::cast_possible_truncation)]
            let cu_device = match result::device::get(ordinal as i32) {
                Ok(d) => d,
                Err(_) => return false,
            };
            unsafe {
                result::device::get_attribute(
                    cu_device,
                    sys::CUdevice_attribute::CU_DEVICE_ATTRIBUTE_INTEGRATED,
                )
                .map(|v| v != 0)
                .unwrap_or(false)
            }
        }
        _ => false,
    }
}

fn env_flag_enabled(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuntimeLoadPolicy {
    pub parallelize_immediate_isq_on_discrete_cuda: bool,
    pub max_seq_len_override: Option<usize>,
    pub language_model_only: bool,
    pub moe: MoEExecutionPolicy,
}

impl RuntimeLoadPolicy {
    pub fn from_env() -> Self {
        Self {
            parallelize_immediate_isq_on_discrete_cuda: env_flag_enabled(
                "ENGINE_PARALLEL_IMMEDIATE_ISQ",
            ),
            max_seq_len_override: positive_usize_env("ENGINE_MAX_SEQ_LEN_OVERRIDE"),
            language_model_only: env_flag_enabled("ENGINE_LANGUAGE_MODEL_ONLY"),
            moe: MoEExecutionPolicy {
                backend_override: moe_backend_override_from_env(),
                allow_slow_backend_on_cuda: env_flag_enabled("ENGINE_ALLOW_SLOW_MOE_BACKEND"),
                cache_capacity: std::env::var("ENGINE_MOE_CACHE_CAPACITY")
                    .ok()
                    .and_then(|s| s.trim().parse::<usize>().ok())
                    .filter(|&c| c > 0),
                cache_warm_tier_budget_bytes: std::env::var("ENGINE_MOE_CACHE_WARM_MB")
                    .ok()
                    .and_then(|s| s.trim().parse::<usize>().ok())
                    .filter(|&mb| mb > 0)
                    .map(|mb| mb * 1024 * 1024),
                // Populated by the pipeline after it computes immediate_ty
                // from the ISQ config; we can't read it here because that
                // state is per-pipeline, not env-driven.
                cache_requested_isq: None,
            },
        }
    }
}

fn positive_usize_env(key: &str) -> Option<usize> {
    std::env::var(key)
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
}

fn moe_backend_override_from_env() -> Option<MoEExpertsBackend> {
    let raw = std::env::var("ENGINE_MOE_EXPERTS_BACKEND").ok()?;
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "fused" => Some(MoEExpertsBackend::Fused),
        "fast" => Some(MoEExpertsBackend::Fast),
        "slow" => Some(MoEExpertsBackend::Slow),
        other => {
            tracing::warn!(
                "Ignoring invalid ENGINE_MOE_EXPERTS_BACKEND={other:?}; expected fused|fast|slow."
            );
            None
        }
    }
}

/// Returns `true` when immediate ISQ should use a parallel thread pool.
///
/// Peak-averse default:
/// - discrete CUDA GPUs: serialize immediate ISQ to avoid transient VRAM spikes
/// - CPU / unified-memory devices: parallel immediate ISQ remains allowed
/// - explicit policy override may allow parallel immediate ISQ on discrete CUDA
pub fn should_parallelize_immediate_isq(
    device: &Device,
    allow_parallel_for_discrete_cuda: bool,
) -> bool {
    matches!(device, Device::Cuda(_))
        .then(|| is_integrated_gpu(device) || allow_parallel_for_discrete_cuda)
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::RuntimeLoadPolicy;

    #[test]
    fn runtime_load_policy_defaults_to_serial_discrete_cuda_isq() {
        let policy = RuntimeLoadPolicy::default();
        assert!(!policy.parallelize_immediate_isq_on_discrete_cuda);
        assert_eq!(policy.max_seq_len_override, None);
        assert!(!policy.language_model_only);
    }
}
