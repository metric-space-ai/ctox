//! Unified MoE experts layer supporting multiple backends and weight formats.
//!
//! This module provides `MoEExperts`, a flexible experts layer that:
//! - Does NOT carry the gate (router) - gate is external
//! - Supports both per-expert and stacked weight formats
//! - Handles backend selection (fused/fast/slow)
//! - Manages tensor parallelism with all-reduce

use candle_core::{DType, Device, Result, Tensor, D};
use engine_quant::{
    FusedExperts, MatMul, PackedExperts, QuantMethod, QuantizedConfig, ShardedVarBuilder,
    SumAllReduce,
};
use std::sync::Arc;

use crate::cuda::moe;
use crate::layers::Activation;
use crate::moe::cache::{CacheConfig, ExpertTriple, MoEExpertCache, Topology};
use crate::moe::shard;

fn qmethod_matmul_autocast(xs: &Tensor, layer: &dyn QuantMethod) -> candle_core::Result<Tensor> {
    let original_dtype = xs.dtype();
    let mut xs = xs.clone();
    if let Some(t) = layer.quantized_act_type() {
        xs = xs.to_dtype(t)?;
    }
    let mut ys = MatMul.qmethod_matmul(&xs, layer)?;
    if layer.quantized_act_type().is_some() && ys.dtype() != original_dtype {
        ys = ys.to_dtype(original_dtype)?;
    }
    Ok(ys)
}

/// Configuration for MoEExperts
pub struct MoEExpertsConfig {
    pub num_experts: usize,
    pub num_experts_per_tok: usize,
    pub hidden_size: usize,
    pub moe_intermediate_size: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoEExecutionPolicy {
    pub backend_override: Option<MoEExpertsBackend>,
    pub allow_slow_backend_on_cuda: bool,
    /// Cache-backed expert swapping. `None` disables the cache; `Some(k)` keeps
    /// `k` experts resident on the device and tiers the rest onto Warm (CPU RAM
    /// on discrete GPUs) or Cold (SSD stub on unified memory) storage.
    ///
    /// When `Some`, `backend_override` is ignored — the cached backend is
    /// always used regardless of device, since it supersedes the slow loop.
    pub cache_capacity: Option<usize>,
    /// Optional cap on the CPU-RAM warm-tier bytes (discrete GPUs only). Build
    /// fails if the warm tier would exceed this budget. Ignored on unified
    /// memory and CPU-only topologies.
    pub cache_warm_tier_budget_bytes: Option<usize>,
    /// Target ISQ type for cached experts. The framework-wide immediate-ISQ
    /// state lives in a `thread_local!` that only the main load thread sets,
    /// so rayon-parallel layer constructors can't read it — without this
    /// explicit copy the Cached backend would silently stage BF16 weights
    /// into the pool (~4x the intended RAM / SSD footprint).
    pub cache_requested_isq: Option<engine_quant::IsqType>,
}

/// Backend selection for MoE experts
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoEExpertsBackend {
    /// Use fused CUDA kernels with raw tensors (fastest for CUDA unquantized)
    Fused,
    /// Use gather-based implementation (good for Metal, ISQ)
    Fast,
    /// Use loop-based implementation (fallback for quantized)
    Slow,
    /// Loop-based backend with LFU-evicted per-expert tiered cache. Used when
    /// total expert weights exceed the device VRAM budget. See
    /// [`crate::moe::cache::MoEExpertCache`].
    Cached,
}

impl MoEExpertsBackend {
    fn backend_allowed_on_device(is_cuda: bool, backend: Self, policy: MoEExecutionPolicy) -> bool {
        // The `Cached` backend is always allowed on any device — it shares the
        // Slow backend's loop but addresses the CUDA-VRAM pressure problem
        // that originally motivated Slow's CUDA exclusion.
        !is_cuda
            || !matches!(backend, Self::Slow)
            || policy.allow_slow_backend_on_cuda
            || matches!(backend, Self::Cached)
    }

    fn select_from_caps(
        is_cuda: bool,
        is_metal: bool,
        loading_isq: bool,
        has_immediate_isq: bool,
        quantized: bool,
    ) -> Self {
        let use_fast = is_metal;
        let use_fast_cuda_isq = is_cuda && !quantized && (loading_isq || has_immediate_isq);
        let use_fused_cuda_quantized = is_cuda && quantized;

        if use_fast {
            Self::Fast
        } else if use_fast_cuda_isq {
            // For CUDA immediate ISQ, the gather/indexed-MoE path can now keep
            // expert tensors in a 3D packed form and remap incompatible K-quants
            // (e.g. Q4K on 704-wide expert inputs) onto indexed-MoE-safe dtypes
            // such as Q5_1. That preserves the fast MoE kernels while still
            // avoiding the raw-GPU-load spikes of the fused path.
            Self::Fast
        } else if use_fused_cuda_quantized {
            Self::Fused
        } else if !quantized && !loading_isq && !has_immediate_isq && is_cuda {
            Self::Fused
        } else {
            Self::Slow
        }
    }

    /// Determine the best backend based on device and quantization settings
    pub fn select(
        device: &Device,
        loading_isq: bool,
        quantization_config: &Option<QuantizedConfig>,
        policy: MoEExecutionPolicy,
    ) -> Self {
        // Cache opt-in wins — it is the only backend that can run models whose
        // total expert weights exceed the device VRAM budget.
        if policy.cache_capacity.is_some() {
            tracing::info!(
                "Using MoE experts backend `cached` for device {:?} (capacity={:?}).",
                device.location(),
                policy.cache_capacity
            );
            return Self::Cached;
        }

        if let Some(backend) = policy.backend_override {
            tracing::info!(
                "Using MoE experts backend override {:?} for device {:?}.",
                match backend {
                    Self::Fused => "fused",
                    Self::Fast => "fast",
                    Self::Slow => "slow",
                    Self::Cached => "cached",
                },
                device.location()
            );
            return backend;
        }

        let has_immediate_isq = engine_quant::get_immediate_isq().is_some();
        let selected = Self::select_from_caps(
            device.is_cuda(),
            device.is_metal(),
            loading_isq,
            has_immediate_isq,
            quantization_config.is_some(),
        );

        tracing::info!(
            "Selected MoE experts backend {:?} for device {:?} (loading_isq={} quantized={} immediate_isq={}).",
            match selected {
                Self::Fused => "fused",
                Self::Fast => "fast",
                Self::Slow => "slow",
                Self::Cached => "cached",
            },
            device.location(),
            loading_isq,
            quantization_config.is_some(),
            has_immediate_isq
        );

        selected
    }
}

#[cfg(test)]
mod tests {
    use super::{MoEExecutionPolicy, MoEExpertsBackend};

    #[test]
    fn cuda_immediate_isq_prefers_fast_backend() {
        let selected = MoEExpertsBackend::select_from_caps(true, false, true, true, false);
        assert!(matches!(selected, MoEExpertsBackend::Fast));
    }

    #[test]
    fn cuda_unquantized_no_isq_prefers_fused_backend() {
        let selected = MoEExpertsBackend::select_from_caps(true, false, false, false, false);
        assert!(matches!(selected, MoEExpertsBackend::Fused));
    }

    #[test]
    fn slow_backend_is_rejected_on_cuda_by_default() {
        assert!(!MoEExpertsBackend::backend_allowed_on_device(
            true,
            MoEExpertsBackend::Slow,
            MoEExecutionPolicy::default(),
        ));
    }

    #[test]
    fn slow_backend_can_be_explicitly_allowed_on_cuda() {
        assert!(MoEExpertsBackend::backend_allowed_on_device(
            true,
            MoEExpertsBackend::Slow,
            MoEExecutionPolicy {
                allow_slow_backend_on_cuda: true,
                ..MoEExecutionPolicy::default()
            },
        ));
    }

    #[test]
    fn cache_capacity_forces_cached_backend_on_any_device() {
        // `MoEExecutionPolicy::cache_capacity = Some(K)` must override the
        // normal capability-based selection and pick `Cached` on every
        // device. Regression guard for the planner → engine contract:
        // when the planner sets `moe_cache: Some(...)`, the engine must
        // honor it rather than falling back to Fused/Fast/Slow based on
        // device caps.
        use candle_core::Device;

        let policy = MoEExecutionPolicy {
            cache_capacity: Some(16),
            ..MoEExecutionPolicy::default()
        };
        // CPU device + cached policy ⇒ Cached.
        let sel = MoEExpertsBackend::select(&Device::Cpu, false, &None, policy);
        assert!(matches!(sel, MoEExpertsBackend::Cached));
        // Even when capability-based selection would pick Fast/Fused, the
        // policy override wins. We can't construct a Metal/Cuda Device in a
        // host-agnostic unit test, but the precedence check in
        // `select` runs before capability detection, so the CPU check
        // above exercises the same code path.
    }

    #[test]
    fn cached_backend_is_allowed_on_cuda_without_slow_flag() {
        // Cached backend is always allowed on any device — unlike `Slow`
        // which is CUDA-rejected by default. The cache is the answer to
        // the VRAM-pressure problem that originally motivated Slow's
        // CUDA rejection, so it doesn't need `allow_slow_backend_on_cuda`.
        assert!(MoEExpertsBackend::backend_allowed_on_device(
            true,
            MoEExpertsBackend::Cached,
            MoEExecutionPolicy::default(),
        ));
    }
}

/// Internal representation of fused expert weights for CUDA kernels
struct FusedExpertsWeights {
    /// gate_up weights: [E, N, K] for standard, [E, K, N] for stacked
    gate_up_w: Tensor,
    /// down weights: [E, N, K] for standard, [E, K, N] for stacked
    down_w: Tensor,
    /// Size of intermediate dimension (after sharding)
    w_size_n: usize,
    /// Whether weights are in stacked format [E, K, N]
    stacked_format: bool,
}

/// Internal representation for gather-based experts (Metal/ISQ)
struct FastExpertsWeights {
    fused_gate_proj: Arc<dyn QuantMethod>,
    fused_up_proj: Arc<dyn QuantMethod>,
    fused_down_proj: Arc<dyn QuantMethod>,
}

/// Internal representation for loop-based experts (quantized fallback)
struct SlowExpertsWeights {
    experts: PackedExperts,
}

/// Internal representation for cache-backed experts with on-demand tier transitions.
///
/// When the configured `capacity` matches `num_experts`, `fused` is populated
/// with a stacked-Q-tensor `FastExpertsWeights`. The forward path then
/// dispatches through candle's grouped-GEMM `qmatmul_indexed_moe_forward`
/// kernel (one kernel launch per projection per layer) instead of the
/// per-expert loop — same performance as the native `Fast` backend, but
/// loaded via the Cached backend's per-expert ISQ path so we don't peak
/// at 60 GiB of BF16 weights during load.
struct CachedExpertsWeights {
    cache: Arc<MoEExpertCache>,
    /// All-resident grouped-GEMM fast path. `Some(_)` iff the cache holds
    /// every expert resident and the per-expert QuantMethods were stackable
    /// into a single rank-3 `(num_experts, out, in)` QTensor.
    fused: Option<FastExpertsWeights>,
}

/// MoE experts layer without gate
///
/// This struct encapsulates the expert weights and forward logic,
/// but does NOT include the routing gate. The caller is responsible
/// for computing routing weights and topk indices.
pub struct MoEExperts {
    backend: MoEExpertsBackendImpl,
    act: Activation,
    num_experts_per_tok: usize,
    all_reduce: SumAllReduce,
    world_size: usize,
}

enum MoEExpertsBackendImpl {
    Fused(FusedExpertsWeights),
    Fast(FastExpertsWeights),
    Slow(SlowExpertsWeights),
    Cached(CachedExpertsWeights),
}

impl MoEExperts {
    fn is_direct_experts_root(vb: &ShardedVarBuilder) -> bool {
        vb.contains_tensor("gate_up_proj")
            || vb.contains_tensor("gate_up_proj.weight")
            || vb.pp("0").contains_tensor("gate_proj")
            || vb.pp("0").contains_tensor("gate_proj.weight")
    }

    /// Create MoEExperts with automatic backend selection
    ///
    /// Automatically detects weight format (stacked vs per-expert) and
    /// selects the appropriate backend based on device and quantization.
    pub fn new(
        cfg: &MoEExpertsConfig,
        vb: ShardedVarBuilder,
        layer_device: Device,
        comm: &Arc<engine_quant::Comm>,
        loading_isq: bool,
        quantization_config: &Option<QuantizedConfig>,
        policy: MoEExecutionPolicy,
        act: Activation,
    ) -> Result<Self> {
        let backend =
            MoEExpertsBackend::select(&layer_device, loading_isq, quantization_config, policy);
        Self::new_with_backend(
            cfg,
            vb,
            layer_device,
            comm,
            backend,
            policy,
            quantization_config,
            act,
        )
    }

    /// Create MoEExperts with explicit backend selection
    pub fn new_with_backend(
        cfg: &MoEExpertsConfig,
        vb: ShardedVarBuilder,
        layer_device: Device,
        comm: &Arc<engine_quant::Comm>,
        backend: MoEExpertsBackend,
        policy: MoEExecutionPolicy,
        quantization_config: &Option<QuantizedConfig>,
        act: Activation,
    ) -> Result<Self> {
        if !MoEExpertsBackend::backend_allowed_on_device(layer_device.is_cuda(), backend, policy) {
            candle_core::bail!(
                "refusing slow MoE backend on CUDA without explicit slow-backend allowance"
            );
        }
        let experts_root = if Self::is_direct_experts_root(&vb) {
            vb.clone()
        } else {
            vb.pp("experts")
        };
        // The target load device:
        // - Slow / Cached always stage on CPU (Cached rematerializes resident
        //   experts onto CUDA later; Slow stays on CPU or whatever the vb
        //   root device already is).
        // - Fused/Fast normally load straight onto `layer_device` — except
        //   when immediate ISQ is pending and `layer_device` is discrete
        //   CUDA. Without this exception the Fused-path peaks at ~60 GiB of
        //   stacked BF16 tensors on the device and OOMs a 48 GiB card before
        //   the ISQ pass can shrink them. Loading on CPU keeps peak device
        //   usage low; the framework's ISQ pass moves each quantized result
        //   to the target device per-tensor.
        let isq_pending_on_discrete_cuda = policy.cache_requested_isq.is_some()
            && layer_device.is_cuda()
            && !crate::utils::normal::is_integrated_gpu(&layer_device);
        let fast_load_device = if isq_pending_on_discrete_cuda {
            Device::Cpu
        } else {
            layer_device.clone()
        };
        let experts_vb = match backend {
            MoEExpertsBackend::Fused | MoEExpertsBackend::Fast => {
                experts_root.clone().set_device(fast_load_device.clone())
            }
            MoEExpertsBackend::Slow => experts_root.clone(),
            // Cached backend MUST load experts on CPU: all 256 experts would
            // otherwise materialize on CUDA, only to be immediately serialized
            // back through the (globally-locked) CUDA D->H memcpy path as the
            // cache stages them into its static pool. That path is gated by
            // libcuda's per-context rwlock, so N parallel rayon workers still
            // serialize through a single lock — turning a 40-layer MoE load
            // into an hours-long stall. Loading on CPU avoids the D->H hop
            // entirely; the cache then materializes only the first `capacity`
            // experts onto CUDA via `deserialize(bytes, cuda_device)`.
            MoEExpertsBackend::Cached => experts_root.clone().set_device(Device::Cpu),
        };

        // Detect format: stacked has "gate_up_proj", per-expert has "0.gate_proj"
        let is_stacked = experts_vb.contains_tensor("gate_up_proj")
            || experts_vb.contains_tensor("gate_up_proj.weight");

        let backend_impl = match backend {
            MoEExpertsBackend::Fused => {
                if is_stacked {
                    MoEExpertsBackendImpl::Fused(Self::load_fused_stacked(cfg, experts_vb, comm)?)
                } else {
                    MoEExpertsBackendImpl::Fused(Self::load_fused_standard(cfg, experts_vb, comm)?)
                }
            }
            MoEExpertsBackend::Fast => {
                // Pass the device-corrected experts_vb (root with CPU device
                // when immediate ISQ is pending) so loading doesn't OOM; the
                // `layer_device` argument remains the post-ISQ target for
                // `FusedExperts::new`.
                let fast_vb = if isq_pending_on_discrete_cuda {
                    experts_root.clone().set_device(Device::Cpu)
                } else {
                    experts_root.clone()
                };
                if is_stacked {
                    MoEExpertsBackendImpl::Fast(Self::load_fast_stacked(
                        cfg,
                        fast_vb,
                        &layer_device,
                        quantization_config,
                    )?)
                } else {
                    MoEExpertsBackendImpl::Fast(Self::load_fast_standard(
                        cfg,
                        fast_vb,
                        &layer_device,
                        quantization_config,
                    )?)
                }
            }
            MoEExpertsBackend::Slow => MoEExpertsBackendImpl::Slow(Self::load_slow(
                cfg,
                experts_vb,
                comm,
                quantization_config,
            )?),
            MoEExpertsBackend::Cached => {
                let capacity = policy.cache_capacity.ok_or_else(|| {
                    candle_core::Error::msg(
                        "Cached MoE backend requires MoEExecutionPolicy::cache_capacity",
                    )
                })?;
                MoEExpertsBackendImpl::Cached(Self::load_cached(
                    cfg,
                    experts_vb,
                    comm,
                    quantization_config,
                    layer_device.clone(),
                    capacity,
                    policy.cache_warm_tier_budget_bytes,
                    policy.cache_requested_isq,
                )?)
            }
        };

        Ok(Self {
            backend: backend_impl,
            act,
            num_experts_per_tok: cfg.num_experts_per_tok,
            all_reduce: SumAllReduce::new(comm),
            world_size: comm.world_size(),
        })
    }

    /// Load fused weights in standard per-expert format
    fn load_fused_standard(
        cfg: &MoEExpertsConfig,
        experts_vb: ShardedVarBuilder,
        comm: &Arc<engine_quant::Comm>,
    ) -> Result<FusedExpertsWeights> {
        let num_experts = cfg.num_experts;
        let mut gate_up_experts = Vec::with_capacity(num_experts);
        let mut down_experts = Vec::with_capacity(num_experts);

        for i in 0..num_experts {
            let expert_vb = experts_vb.pp(i.to_string());
            // n x k format
            let gate_expert = expert_vb.pp("gate_proj").get_with_hints(
                (cfg.moe_intermediate_size, cfg.hidden_size),
                "weight",
                shard(0, comm.rank(), comm.world_size()),
            )?;
            let up_expert = expert_vb.pp("up_proj").get_with_hints(
                (cfg.moe_intermediate_size, cfg.hidden_size),
                "weight",
                shard(0, comm.rank(), comm.world_size()),
            )?;
            let down_expert = expert_vb.pp("down_proj").get_with_hints(
                (cfg.hidden_size, cfg.moe_intermediate_size),
                "weight",
                shard(1, comm.rank(), comm.world_size()),
            )?;
            // Pack gate_proj and up_proj
            let gate_up_expert = Tensor::cat(&[&gate_expert, &up_expert], 0)?;

            gate_up_experts.push(gate_up_expert);
            down_experts.push(down_expert);
        }

        let gate_up_w = Tensor::stack(&gate_up_experts, 0)?;
        let down_w = Tensor::stack(&down_experts, 0)?;
        let w_size_n = gate_up_w.dim(1)? / 2;

        Ok(FusedExpertsWeights {
            gate_up_w,
            down_w,
            w_size_n,
            stacked_format: false,
        })
    }

    /// Load fused weights in stacked format
    fn load_fused_stacked(
        cfg: &MoEExpertsConfig,
        experts_vb: ShardedVarBuilder,
        comm: &Arc<engine_quant::Comm>,
    ) -> Result<FusedExpertsWeights> {
        let num_experts = cfg.num_experts;

        // Stacked format has two conventions:
        // Convention A: [num_experts, hidden, inter*2] (CUDA kernel format)
        // Convention B (nn.Linear): [num_experts, inter*2, hidden]
        // Try A first, fall back to B with transpose.
        let gate_up_w = experts_vb
            .get_with_hints(
                (num_experts, cfg.hidden_size, cfg.moe_intermediate_size * 2),
                "gate_up_proj",
                shard(2, comm.rank(), comm.world_size()),
            )
            .or_else(|_| {
                experts_vb
                    .get_with_hints(
                        (num_experts, cfg.moe_intermediate_size * 2, cfg.hidden_size),
                        "gate_up_proj",
                        shard(1, comm.rank(), comm.world_size()),
                    )
                    .and_then(|t| t.transpose(1, 2)?.contiguous())
            })?;

        let down_w = experts_vb
            .get_with_hints(
                (num_experts, cfg.moe_intermediate_size, cfg.hidden_size),
                "down_proj",
                shard(1, comm.rank(), comm.world_size()),
            )
            .or_else(|_| {
                experts_vb
                    .get_with_hints(
                        (num_experts, cfg.hidden_size, cfg.moe_intermediate_size),
                        "down_proj",
                        shard(2, comm.rank(), comm.world_size()),
                    )
                    .and_then(|t| t.transpose(1, 2)?.contiguous())
            })?;

        let w_size_n = gate_up_w.dim(2)? / 2;

        Ok(FusedExpertsWeights {
            gate_up_w,
            down_w,
            w_size_n,
            stacked_format: true,
        })
    }

    /// Load fast (gather-based) weights in standard per-expert format
    fn load_fast_standard(
        cfg: &MoEExpertsConfig,
        vb: ShardedVarBuilder,
        layer_device: &Device,
        quantization_config: &Option<QuantizedConfig>,
    ) -> Result<FastExpertsWeights> {
        let FusedExperts {
            fused_gate_proj,
            fused_up_proj,
            fused_down_proj,
        } = FusedExperts::new(
            cfg.hidden_size,
            cfg.moe_intermediate_size,
            cfg.num_experts,
            quantization_config,
            vb,
            Some(layer_device),
        )?;

        Ok(FastExpertsWeights {
            fused_gate_proj,
            fused_up_proj,
            fused_down_proj,
        })
    }

    /// Load fast (gather-based) weights in stacked format
    fn load_fast_stacked(
        cfg: &MoEExpertsConfig,
        vb: ShardedVarBuilder,
        layer_device: &Device,
        quantization_config: &Option<QuantizedConfig>,
    ) -> Result<FastExpertsWeights> {
        // FusedExperts auto-detects stacked format
        let FusedExperts {
            fused_gate_proj,
            fused_up_proj,
            fused_down_proj,
        } = FusedExperts::new(
            cfg.hidden_size,
            cfg.moe_intermediate_size,
            cfg.num_experts,
            quantization_config,
            vb,
            Some(layer_device),
        )?;

        Ok(FastExpertsWeights {
            fused_gate_proj,
            fused_up_proj,
            fused_down_proj,
        })
    }

    /// Load cache-backed experts: build `PackedExperts` as for the Slow path,
    /// then hand the per-expert triples to [`MoEExpertCache`], which will
    /// serialize and tier down experts beyond `capacity` so their GPU memory
    /// is released.
    ///
    /// Only the per-expert `PackedExperts` layout is supported — stacked/fused
    /// formats (e.g. AFQ `gate_up_proj`) pack all experts into a single kernel
    /// target and cannot be swapped at the individual-expert granularity.
    #[allow(clippy::too_many_arguments)]
    fn load_cached(
        cfg: &MoEExpertsConfig,
        experts_vb: ShardedVarBuilder,
        comm: &Arc<engine_quant::Comm>,
        quantization_config: &Option<QuantizedConfig>,
        layer_device: Device,
        capacity: usize,
        warm_tier_budget_bytes: Option<usize>,
        requested_isq: Option<engine_quant::IsqType>,
    ) -> Result<CachedExpertsWeights> {
        let packed = PackedExperts::new(
            cfg.num_experts,
            cfg.hidden_size,
            cfg.moe_intermediate_size,
            quantization_config,
            false,
            comm,
            experts_vb,
        )?;

        if packed.gate_proj.len() != cfg.num_experts
            || packed.up_proj.len() != cfg.num_experts
            || packed.down_proj.len() != cfg.num_experts
        {
            candle_core::bail!(
                "Cached MoE backend requires per-expert PackedExperts layout (got gate={}, up={}, down={} for num_experts={})",
                packed.gate_proj.len(),
                packed.up_proj.len(),
                packed.down_proj.len(),
                cfg.num_experts
            );
        }

        let mut triples: Vec<ExpertTriple> = packed
            .gate_proj
            .into_iter()
            .zip(packed.up_proj)
            .zip(packed.down_proj)
            .map(|((gate, up), down)| ExpertTriple { gate, up, down })
            .collect();

        // Apply ISQ to every cached expert right here, on CPU, before the
        // cache sees them. Historically `get_isq_layers()` for the Cached
        // backend returned an empty Vec and the framework-wide ISQ pass
        // skipped expert weights entirely — the pool then stored 60 GiB of
        // BF16 bytes for a 40-layer, 256-expert Qwen3.6 load and the host
        // OOM-killed the engine. Applying ISQ here shrinks the pool to the
        // serialized-Q4K footprint (~15 GiB for the same model) and matches
        // what the non-cached backends do. We use CPU as the target device:
        // every slot except the first `capacity` per layer will be
        // serialized straight to the pool and never touched on CUDA, and the
        // resident slots are promoted to CUDA via `deserialize(bytes, cuda)`
        // inside `MoEExpertCache::new` anyway.
        //
        // IsqType is threaded in via `MoEExecutionPolicy::cache_requested_isq`
        // and falls back to `get_immediate_isq()` for code paths that haven't
        // been updated to pass the policy yet. Thread-local lookup only works
        // on the main load thread — rayon-parallel layer loading makes the
        // thread-local invisible to workers.
        let isq_ty_opt = requested_isq.or_else(|| {
            engine_quant::get_immediate_isq().and_then(|p| p.ty)
        });
        if let Some(isq_ty) = isq_ty_opt {
            {
                let guard = engine_quant::QuantizeOntoGuard::new();
                let counter = std::sync::atomic::AtomicUsize::new(0);
                for t in triples.iter_mut() {
                    t.gate = t.gate.clone().apply_isq(
                        Some(isq_ty),
                        Device::Cpu,
                        &counter,
                        None,
                        guard.clone(),
                    )?;
                    t.up = t.up.clone().apply_isq(
                        Some(isq_ty),
                        Device::Cpu,
                        &counter,
                        None,
                        guard.clone(),
                    )?;
                    t.down = t.down.clone().apply_isq(
                        Some(isq_ty),
                        Device::Cpu,
                        &counter,
                        None,
                        guard.clone(),
                    )?;
                }
                tracing::info!(
                    "Cached MoE: applied ISQ {:?} to {} experts on CPU before cache staging",
                    isq_ty,
                    triples.len()
                );
            }
        }

        let topology = Topology::detect(&layer_device);
        // Engine-internal env vars (not a CTOX user-facing toggle — CTOX
        // translates its typed `PlannedMoECacheAllocation` into these before
        // launching the engine, mirroring how `ENGINE_*` vars are produced
        // elsewhere in the model-runtime).
        let cold_tier_path = std::env::var("ENGINE_MOE_CACHE_COLD_PATH")
            .ok()
            .filter(|s| !s.is_empty())
            .map(std::path::PathBuf::from);
        let cold_tier_min_mbps = std::env::var("ENGINE_MOE_CACHE_COLD_MIN_MBPS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
        // All-resident grouped-GEMM fast path. When every expert stays
        // resident anyway, we'd pay the Cached-loop dispatch overhead for
        // no reason — 8 top-k matmuls per layer per token, each its own
        // kernel launch, for a ~100 tok/s ceiling on A6000. Build a
        // single `(num_experts, out, in)` stacked `GgufMatMul` per
        // projection and route the forward through candle's
        // `qmatmul_indexed_moe_forward` kernel (one launch for all top-k
        // experts). Stacking is done via raw Q-block byte concatenation,
        // so no accuracy loss and no re-quantization.
        //
        // Only meaningful on CUDA with a Q-dtype the MoE kernel supports
        // (Q4K/Q5K/Q6K/Q8_0). On other backends the stacked build is
        // skipped and the forward falls through to the per-expert loop.
        let fused = if capacity >= cfg.num_experts
            && layer_device.is_cuda()
            && triples.first().map(|t| t.gate.name() == "gguf").unwrap_or(false)
        {
            let t_fuse = std::time::Instant::now();
            let gate_arcs: Vec<_> = triples.iter().map(|t| t.gate.clone()).collect();
            let up_arcs: Vec<_> = triples.iter().map(|t| t.up.clone()).collect();
            let down_arcs: Vec<_> = triples.iter().map(|t| t.down.clone()).collect();
            match (
                engine_quant::stack_gguf_experts(&gate_arcs, &layer_device),
                engine_quant::stack_gguf_experts(&up_arcs, &layer_device),
                engine_quant::stack_gguf_experts(&down_arcs, &layer_device),
            ) {
                (Ok(g), Ok(u), Ok(d)) => {
                    tracing::info!(
                        "Cached MoE: built fused grouped-GEMM tensors for {} experts in {}ms \
                         — forward will dispatch via single gather kernel",
                        triples.len(),
                        t_fuse.elapsed().as_millis(),
                    );
                    Some(FastExpertsWeights {
                        fused_gate_proj: g,
                        fused_up_proj: u,
                        fused_down_proj: d,
                    })
                }
                _ => {
                    tracing::warn!(
                        "Cached MoE: grouped-GEMM stacking failed — falling back to per-expert loop"
                    );
                    None
                }
            }
        } else {
            None
        };

        let cache = MoEExpertCache::new(
            triples,
            CacheConfig {
                capacity,
                topology,
                device: layer_device,
                warm_tier_budget_bytes,
                cold_tier_path,
                cold_tier_min_mbps,
            },
            comm.clone(),
        )?;

        Ok(CachedExpertsWeights {
            cache: Arc::new(cache),
            fused,
        })
    }

    /// Load slow (loop-based) weights using PackedExperts
    fn load_slow(
        cfg: &MoEExpertsConfig,
        experts_vb: ShardedVarBuilder,
        comm: &Arc<engine_quant::Comm>,
        quantization_config: &Option<QuantizedConfig>,
    ) -> Result<SlowExpertsWeights> {
        let experts = PackedExperts::new(
            cfg.num_experts,
            cfg.hidden_size,
            cfg.moe_intermediate_size,
            quantization_config,
            false,
            comm,
            experts_vb,
        )?;

        let dummy_gate = experts
            .gate_proj
            .iter()
            .filter(|layer| layer.name() == "dummy")
            .count();
        let dummy_up = experts
            .up_proj
            .iter()
            .filter(|layer| layer.name() == "dummy")
            .count();
        let dummy_down = experts
            .down_proj
            .iter()
            .filter(|layer| layer.name() == "dummy")
            .count();
        if dummy_gate != 0 || dummy_up != 0 || dummy_down != 0 {
            candle_core::bail!(
                "PackedExperts produced dummy layers: gate={dummy_gate} up={dummy_up} down={dummy_down}"
            );
        }

        Ok(SlowExpertsWeights { experts })
    }

    /// Forward pass through experts
    ///
    /// # Arguments
    /// * `xs` - Input tensor of shape [batch, seq_len, hidden_dim]
    /// * `topk_weights` - Top-k routing weights of shape [num_tokens, num_experts_per_tok]
    /// * `topk_ids` - Top-k expert indices of shape [num_tokens, num_experts_per_tok]
    ///
    /// # Returns
    /// Output tensor of shape [batch, seq_len, hidden_dim]
    pub fn forward(&self, xs: &Tensor, topk_weights: Tensor, topk_ids: &Tensor) -> Result<Tensor> {
        let (b_size, seq_len, hidden_dim) = xs.dims3()?;
        // Prefill = processing multiple tokens; Decode = single token generation
        let is_prefill = seq_len > 1;

        let mut ys = match &self.backend {
            MoEExpertsBackendImpl::Fused(weights) => {
                self.forward_fused(xs, &topk_weights, topk_ids, weights, is_prefill)?
            }
            MoEExpertsBackendImpl::Fast(weights) => {
                self.forward_fast(xs, &topk_weights, topk_ids, weights)?
            }
            MoEExpertsBackendImpl::Slow(weights) => {
                self.forward_slow(xs, &topk_weights, topk_ids, weights)?
            }
            MoEExpertsBackendImpl::Cached(weights) => {
                self.forward_cached(xs, &topk_weights, topk_ids, weights)?
            }
        };

        // Apply all-reduce for tensor parallelism
        if self.world_size > 1 {
            ys = self.all_reduce.sum_all_reduce(&ys)?;
        }

        ys.reshape((b_size, seq_len, hidden_dim))
    }

    /// Fused CUDA kernel forward pass
    fn forward_fused(
        &self,
        xs: &Tensor,
        topk_weights: &Tensor,
        topk_ids: &Tensor,
        weights: &FusedExpertsWeights,
        is_prefill: bool,
    ) -> Result<Tensor> {
        let (_b_size, _seq_len, hidden_dim) = xs.dims3()?;
        let xs = xs.reshape(((), hidden_dim))?;
        let (num_tokens, _) = xs.dims2()?;

        // Sort tokens by expert for efficient processing
        let (expert_ids, sorted_token_ids) = if is_prefill {
            #[cfg(feature = "cuda")]
            {
                use crate::ops::ArgSortOp;
                topk_ids.flatten_all()?.sort(true)?
            }
            #[cfg(not(feature = "cuda"))]
            topk_ids.flatten_all()?.sort_last_dim(true)?
        } else {
            topk_ids.flatten_all()?.sort_last_dim(true)?
        };

        // First GEMM: gate_up projection
        let gate_up = if weights.stacked_format {
            moe::moe_gemm_transposed(
                &xs,
                &weights.gate_up_w,
                &None,
                &sorted_token_ids,
                &expert_ids,
                self.num_experts_per_tok,
                is_prefill,
            )?
        } else {
            moe::moe_gemm(
                &xs,
                &weights.gate_up_w,
                &None,
                &sorted_token_ids,
                &expert_ids,
                self.num_experts_per_tok,
                is_prefill,
            )?
        };

        // Split and apply activation
        let gate = gate_up
            .narrow(D::Minus1, 0, weights.w_size_n)?
            .contiguous()?;
        let up = gate_up
            .narrow(D::Minus1, weights.w_size_n, weights.w_size_n)?
            .contiguous()?;

        let down_inputs = (up * gate.apply(&self.act)?)?.reshape(((), weights.w_size_n))?;

        // Second GEMM: down projection with weight aggregation
        let ys = if weights.stacked_format {
            moe::moe_gemm_transposed(
                &down_inputs,
                &weights.down_w,
                &Some(topk_weights.clone()),
                &sorted_token_ids,
                &expert_ids,
                self.num_experts_per_tok,
                is_prefill,
            )?
        } else {
            moe::moe_gemm(
                &down_inputs,
                &weights.down_w,
                &Some(topk_weights.clone()),
                &sorted_token_ids,
                &expert_ids,
                self.num_experts_per_tok,
                is_prefill,
            )?
        };

        ys.reshape((num_tokens, (), hidden_dim))?.sum(D::Minus2)
    }

    /// Gather-based forward pass (Metal/ISQ)
    fn forward_fast(
        &self,
        xs: &Tensor,
        topk_weights: &Tensor,
        topk_ids: &Tensor,
        weights: &FastExpertsWeights,
    ) -> Result<Tensor> {
        let original_dtype = xs.dtype();
        let (b_size, seq_len, hidden_dim) = xs.dims3()?;
        let num_tokens = b_size * seq_len;
        let (_, gate_device) = weights.fused_gate_proj.dtype_and_device();
        let (_, up_device) = weights.fused_up_proj.dtype_and_device();
        let (_, down_device) = weights.fused_down_proj.dtype_and_device();
        if !xs.device().same_device(&gate_device) {
            candle_core::bail!(
                "moe fast fused_gate_proj device mismatch: input={:?} weight={:?}",
                xs.device().location(),
                gate_device.location()
            );
        }
        if !xs.device().same_device(&up_device) {
            candle_core::bail!(
                "moe fast fused_up_proj device mismatch: input={:?} weight={:?}",
                xs.device().location(),
                up_device.location()
            );
        }
        if !xs.device().same_device(&down_device) {
            candle_core::bail!(
                "moe fast fused_down_proj device mismatch: input={:?} weight={:?}",
                xs.device().location(),
                down_device.location()
            );
        }

        let xs_flat = xs.reshape((num_tokens, hidden_dim))?;
        let topk_ids = if topk_ids.device().same_device(xs.device()) {
            topk_ids.clone()
        } else {
            topk_ids.to_device(xs.device())?
        };
        let topk_weights = if topk_weights.device().same_device(xs.device()) {
            topk_weights.clone()
        } else {
            topk_weights.to_device(xs.device())?
        };

        let ys = if xs.device().is_cuda() {
            // CUDA path: use indexed_moe_forward compatible shapes
            let xs = xs_flat.reshape((num_tokens, 1, hidden_dim))?;
            let gate = weights
                .fused_gate_proj
                .gather_forward_autocast(&xs, &topk_ids)
                .map_err(|err| err.with_path("moe fast gate gather"))?;
            let up = weights
                .fused_up_proj
                .gather_forward_autocast(&xs, &topk_ids)
                .map_err(|err| err.with_path("moe fast up gather"))?;
            if !up.device().same_device(gate.device()) {
                candle_core::bail!(
                    "moe fast activation mul device mismatch: up={:?} gate={:?}",
                    up.device().location(),
                    gate.device().location()
                );
            }
            weights
                .fused_down_proj
                .gather_forward_autocast(&(up * gate.apply(&self.act)?)?, &topk_ids)
                .map_err(|err| err.with_path("moe fast down gather"))?
        } else {
            // Metal path: use broadcast gather shapes
            let xs = xs.reshape((b_size, seq_len, 1, 1, hidden_dim))?;
            let indices = topk_ids.reshape((b_size, seq_len, self.num_experts_per_tok))?;
            let gate = weights
                .fused_gate_proj
                .gather_forward_autocast(&xs, &indices)
                .map_err(|err| err.with_path("moe fast gate gather"))?;
            let up = weights
                .fused_up_proj
                .gather_forward_autocast(&xs, &indices)
                .map_err(|err| err.with_path("moe fast up gather"))?;
            if !up.device().same_device(gate.device()) {
                candle_core::bail!(
                    "moe fast activation mul device mismatch: up={:?} gate={:?}",
                    up.device().location(),
                    gate.device().location()
                );
            }
            let xs = weights
                .fused_down_proj
                .gather_forward_autocast(&(up * gate.apply(&self.act)?)?, &indices)
                .map_err(|err| err.with_path("moe fast down gather"))?;
            xs.squeeze(D::Minus2)?
                .reshape((num_tokens, self.num_experts_per_tok, hidden_dim))?
        };

        let topk_weights = topk_weights.unsqueeze(D::Minus1)?;
        if !ys.device().same_device(topk_weights.device()) {
            candle_core::bail!(
                "moe fast routing mul device mismatch: ys={:?} topk_weights={:?}",
                ys.device().location(),
                topk_weights.device().location()
            );
        }
        ys.to_dtype(DType::F32)?
            .broadcast_mul(&topk_weights)?
            .sum(D::Minus2)?
            .to_dtype(original_dtype)
    }

    /// Loop-based forward pass (quantized fallback)
    fn forward_slow(
        &self,
        xs: &Tensor,
        topk_weights: &Tensor,
        topk_ids: &Tensor,
        weights: &SlowExpertsWeights,
    ) -> Result<Tensor> {
        let (b_size, seq_len, hidden_dim) = xs.dims3()?;
        let xs = xs.reshape(((), hidden_dim))?;

        let routing_weights = topk_weights.to_dtype(DType::F32)?.to_vec2::<f32>()?;
        let experts_per_tok = topk_ids.to_vec2::<u32>()?;
        let num_experts = weights.experts.gate_proj.len();

        let mut top_x = vec![vec![]; num_experts];
        let mut selected_experts = vec![vec![]; num_experts];

        for (row_idx, (rw, expert_idxs)) in routing_weights
            .iter()
            .zip(experts_per_tok.iter())
            .enumerate()
        {
            for (&rw, &expert_idx) in rw.iter().zip(expert_idxs.iter()) {
                #[allow(clippy::cast_possible_truncation)]
                top_x[expert_idx as usize].push(row_idx as u32);
                selected_experts[expert_idx as usize].push(rw)
            }
        }

        let mut ys = xs.zeros_like()?;
        for expert_idx in 0..num_experts {
            let top_x_expert = &top_x[expert_idx];
            if top_x_expert.is_empty() {
                continue;
            }
            let top_x_tensor = Tensor::new(top_x_expert.as_slice(), xs.device())?;
            let selected_experts_tensor =
                Tensor::new(selected_experts[expert_idx].as_slice(), xs.device())?
                    .reshape(((), 1))?
                    .to_dtype(xs.dtype())?;
            let current_state = xs
                .index_select(&top_x_tensor, 0)?
                .reshape(((), hidden_dim))?;

            // Forward through expert MLP. Each expert sub-layer may use a different
            // runtime quantization form after immediate ISQ, so align activations per
            // layer instead of assuming gate/up/down share one activation dtype.
            let gate_out =
                qmethod_matmul_autocast(&current_state, &*weights.experts.gate_proj[expert_idx])?
                    .apply(&self.act)?;
            let up_out =
                qmethod_matmul_autocast(&current_state, &*weights.experts.up_proj[expert_idx])?;
            if !gate_out.device().same_device(up_out.device()) {
                candle_core::bail!(
                    "moe slow activation mul device mismatch: gate={:?} up={:?}",
                    gate_out.device().location(),
                    up_out.device().location()
                );
            }
            let current_hidden_states = qmethod_matmul_autocast(
                &(gate_out * up_out)?,
                &*weights.experts.down_proj[expert_idx],
            )?;

            if !current_hidden_states
                .device()
                .same_device(selected_experts_tensor.device())
            {
                candle_core::bail!(
                    "moe slow routing mul device mismatch: hidden={:?} weights={:?}",
                    current_hidden_states.device().location(),
                    selected_experts_tensor.device().location()
                );
            }
            let current_hidden_states =
                current_hidden_states.broadcast_mul(&selected_experts_tensor)?;
            ys = ys.index_add(&top_x_tensor, &current_hidden_states, 0)?;
        }

        ys.reshape((b_size * seq_len, hidden_dim))
    }

    /// Cache-backed forward pass: identical structure to `forward_slow`, but each
    /// `expert_idx` access is routed through [`MoEExpertCache::ensure_resident`]
    /// which may evict an LFU victim and materialize the incoming expert from
    /// the warm or cold tier before the matmul.
    fn forward_cached(
        &self,
        xs: &Tensor,
        topk_weights: &Tensor,
        topk_ids: &Tensor,
        weights: &CachedExpertsWeights,
    ) -> Result<Tensor> {
        // All-resident grouped-GEMM fast path. The cache constructor
        // already built a stacked `FastExpertsWeights` when
        // `capacity >= num_experts`, and at that configuration the cache
        // is guaranteed to always hit — so skip the per-expert loop
        // entirely and dispatch through the Fast backend's single-kernel
        // gather path. Same kernel the native Fast backend uses, just
        // reached via the Cached backend's load pipeline.
        if let Some(fused) = &weights.fused {
            return self.forward_fast(xs, topk_weights, topk_ids, fused);
        }

        let (b_size, seq_len, hidden_dim) = xs.dims3()?;
        let xs = xs.reshape(((), hidden_dim))?;

        let routing_weights = topk_weights.to_dtype(DType::F32)?.to_vec2::<f32>()?;
        let experts_per_tok = topk_ids.to_vec2::<u32>()?;
        let num_experts = weights.cache.num_experts();

        let mut top_x = vec![vec![]; num_experts];
        let mut selected_experts = vec![vec![]; num_experts];

        for (row_idx, (rw, expert_idxs)) in routing_weights
            .iter()
            .zip(experts_per_tok.iter())
            .enumerate()
        {
            for (&rw, &expert_idx) in rw.iter().zip(expert_idxs.iter()) {
                #[allow(clippy::cast_possible_truncation)]
                top_x[expert_idx as usize].push(row_idx as u32);
                selected_experts[expert_idx as usize].push(rw)
            }
        }

        // Kick off SSD prefetch for every active expert in one pass *before*
        // we start the sequential matmul loop. Each `madvise(WILLNEED)` tells
        // the kernel to start pulling the slot's pool bytes into the page
        // cache; by the time a miss hits the materialize path below, the
        // read is already in flight (or done). No-op on RAM backing and for
        // already-resident experts — `prefetch_many` filters both.
        let active: Vec<usize> = top_x
            .iter()
            .enumerate()
            .filter(|(_, v)| !v.is_empty())
            .map(|(i, _)| i)
            .collect();
        weights.cache.prefetch_many(&active);

        let mut ys = xs.zeros_like()?;
        for expert_idx in 0..num_experts {
            let top_x_expert = &top_x[expert_idx];
            if top_x_expert.is_empty() {
                continue;
            }

            // Pull this expert into VRAM (may evict LFU victim + materialize
            // from warm/cold tier). `triple` holds cheap Arc clones that
            // survive even if the slot is later demoted again.
            let triple = weights.cache.ensure_resident(expert_idx)?;

            let top_x_tensor = Tensor::new(top_x_expert.as_slice(), xs.device())?;
            let selected_experts_tensor =
                Tensor::new(selected_experts[expert_idx].as_slice(), xs.device())?
                    .reshape(((), 1))?
                    .to_dtype(xs.dtype())?;
            let current_state = xs
                .index_select(&top_x_tensor, 0)?
                .reshape(((), hidden_dim))?;

            let gate_out =
                qmethod_matmul_autocast(&current_state, &*triple.gate)?.apply(&self.act)?;
            let up_out = qmethod_matmul_autocast(&current_state, &*triple.up)?;
            if !gate_out.device().same_device(up_out.device()) {
                candle_core::bail!(
                    "moe cached activation mul device mismatch: gate={:?} up={:?}",
                    gate_out.device().location(),
                    up_out.device().location()
                );
            }
            let current_hidden_states =
                qmethod_matmul_autocast(&(gate_out * up_out)?, &*triple.down)?;

            if !current_hidden_states
                .device()
                .same_device(selected_experts_tensor.device())
            {
                candle_core::bail!(
                    "moe cached routing mul device mismatch: hidden={:?} weights={:?}",
                    current_hidden_states.device().location(),
                    selected_experts_tensor.device().location()
                );
            }
            let current_hidden_states =
                current_hidden_states.broadcast_mul(&selected_experts_tensor)?;
            ys = ys.index_add(&top_x_tensor, &current_hidden_states, 0)?;
        }

        ys.reshape((b_size * seq_len, hidden_dim))
    }

    /// Get mutable references to quantizable layers for ISQ
    pub fn get_isq_layers(&mut self) -> Vec<&mut Arc<dyn QuantMethod>> {
        match &mut self.backend {
            MoEExpertsBackendImpl::Fused(_) => vec![],
            MoEExpertsBackendImpl::Fast(weights) => {
                vec![
                    &mut weights.fused_gate_proj,
                    &mut weights.fused_up_proj,
                    &mut weights.fused_down_proj,
                ]
            }
            MoEExpertsBackendImpl::Slow(weights) => {
                let mut layers = Vec::new();
                for (gate, (up, down)) in weights.experts.gate_proj.iter_mut().zip(
                    weights
                        .experts
                        .up_proj
                        .iter_mut()
                        .zip(weights.experts.down_proj.iter_mut()),
                ) {
                    layers.push(gate);
                    layers.push(up);
                    layers.push(down);
                }
                layers
            }
            // Cache-backed experts are ISQ'd in-place inside `load_cached`
            // (on CPU, before the cache pool stages them) so that the pool
            // stores Q-format bytes — not BF16 — and cache miss/promote
            // just deserializes those Q-format bytes onto the target
            // device. The framework-wide ISQ pass therefore has nothing
            // left to do for cached experts: we intentionally expose an
            // empty surface here.
            MoEExpertsBackendImpl::Cached(_) => vec![],
        }
    }

    pub fn num_isq_layers(&self) -> usize {
        match &self.backend {
            MoEExpertsBackendImpl::Fused(_) => 0,
            MoEExpertsBackendImpl::Fast(_) => 3,
            MoEExpertsBackendImpl::Slow(weights) => weights.experts.gate_proj.len() * 3,
            MoEExpertsBackendImpl::Cached(_) => 0,
        }
    }
}
