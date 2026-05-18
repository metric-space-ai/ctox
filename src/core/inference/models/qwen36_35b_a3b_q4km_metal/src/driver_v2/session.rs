// Origin: CTOX
// License: Apache-2.0

//! `Session` — owns the `MetalRuntime`, the `BufferPool` of weights,
//! the KV cache (full-attention layers), and the recurrent state cache
//! (linear-attention layers). Created once per ctox-real connection
//! to the IPC server; reused across many Responses turns to amortise
//! weight upload + KV-cache allocation.

#![cfg(feature = "metal")]

use anyhow::Result;

use crate::metal_port::ops::{
    gated_delta_net::{GatedDeltaNetKernel, GatedDeltaNetNsg},
    get_rows_q4_k::GetRowsQ4KKernel,
    mul_mm_q4_k::MulMmQ4KF32Kernel,
    mul_mv_ext_q4_k::MulMvExtQ4KF32Kernel,
    mul_mv_id_q4_k::MulMvIdQ4KF32Kernel,
    mul_mv_q4_k::MulMvQ4KF32Kernel,
    rms_norm::RmsNormF32Kernel,
    ssm_conv::SsmConvKernel,
};
use crate::metal_port::runtime::{BufferPool, MetalRuntime};
use crate::model::{Qwen36MoeTextConfig, QWEN36_35B_A3B_TEXT_CONFIG};

/// All compiled pipelines the layer-block driver dispatches into.
/// Built once per session; pipeline-state-objects are immutable and
/// safe to share across the token loop.
pub struct SessionKernels {
    pub rms_norm: RmsNormF32Kernel,
    pub mul_mv: MulMvQ4KF32Kernel,
    pub mul_mm: MulMmQ4KF32Kernel,
    pub mul_mv_ext: MulMvExtQ4KF32Kernel,
    pub mul_mv_id: MulMvIdQ4KF32Kernel,
    pub get_rows: GetRowsQ4KKernel,
    pub gated_delta_net: GatedDeltaNetKernel,
    pub ssm_conv: SsmConvKernel,
}

impl SessionKernels {
    pub fn new(rt: &MetalRuntime) -> Result<Self> {
        Ok(Self {
            rms_norm: RmsNormF32Kernel::new(rt)?,
            mul_mv: MulMvQ4KF32Kernel::new(rt, /*nsg=*/ 4)?,
            mul_mm: MulMmQ4KF32Kernel::new(rt)?,
            // Stage-3.4 selector: nxpsg=8, r1ptg=4 won the narrow-M sweep.
            mul_mv_ext: MulMvExtQ4KF32Kernel::new(rt, /*r1ptg=*/ 4, /*nsg=*/ 4, /*nxpsg=*/ 8)?,
            mul_mv_id: MulMvIdQ4KF32Kernel::new(rt, /*nsg=*/ 4)?,
            get_rows: GetRowsQ4KKernel::new(rt)?,
            // Qwen3.6: S_v = linear_value_head_dim = 128, G = 1 (non-KDA).
            gated_delta_net: GatedDeltaNetKernel::new(rt, GatedDeltaNetNsg::N4, 128, 1)?,
            // Qwen3.6's linear_conv_kernel_dim = 4 → vec4 path is exact.
            ssm_conv: SsmConvKernel::new(rt, /*vec4=*/ true)?,
        })
    }
}

/// One inference session.
///
/// Holds:
/// - `runtime`: device + queue + library handles (light)
/// - `kernels`: compiled pipelines (light, ~kB each)
/// - `weights`: BufferPool with every weight tensor uploaded once
/// - `kv_cache`: persistent KV cache for full-attention layers
/// - `recurrent_state`: persistent SSM state for linear-attention layers
pub struct Session {
    pub config: Qwen36MoeTextConfig,
    pub runtime: MetalRuntime,
    pub kernels: SessionKernels,
    pub weights: BufferPool,
    pub kv_cache: BufferPool,
    pub recurrent_state: BufferPool,
}

impl Session {
    /// Open a session. Currently a skeleton — Stage-4 wakeup #2
    /// fills in `weights` (mmap'd Q4_K_M GGUF → BufferPool::copy_in
    /// per tensor), `kv_cache` (10 full-attn layers × 2 KV heads ×
    /// 256 head_dim × max_ctx × 2 bytes f16), and `recurrent_state`
    /// (30 linear-attn layers × 32 v_heads × 128² × 4 bytes f32).
    pub fn new(rt: MetalRuntime) -> Result<Self> {
        let kernels = SessionKernels::new(&rt)?;
        let weights = BufferPool::new(&rt);
        let kv_cache = BufferPool::new(&rt);
        let recurrent_state = BufferPool::new(&rt);
        Ok(Self {
            config: QWEN36_35B_A3B_TEXT_CONFIG.clone(),
            runtime: rt,
            kernels,
            weights,
            kv_cache,
            recurrent_state,
        })
    }
}
