//! KV cache — ring buffer of per-layer K/V slabs.
//!
//! Stored as raw device allocations (one `CudaSlice<bf16>` per slab)
//! with explicit position tracking. No candle `Tensor::cat` growth,
//! no copy-on-append — new tokens are written into slot `[n_filled]`
//! and `n_filled` advances.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use cudarc::driver::CudaSlice;
use half::bf16;

use crate::device::DeviceContext;

/// Per-layer K/V slabs.
///
/// Layout (per layer, K or V separately):
///   `[max_ctx × n_kv_heads × head_dim]` bf16, row-major along max_ctx.
pub struct KvCache {
    k_slabs: Vec<CudaSlice<bf16>>,
    v_slabs: Vec<CudaSlice<bf16>>,
    n_layers: usize,
    max_ctx: usize,
    n_kv_heads: usize,
    head_dim: usize,
    n_filled: usize,
    device: Arc<DeviceContext>,
}

impl KvCache {
    pub fn new(
        device: Arc<DeviceContext>,
        n_layers: usize,
        max_ctx: usize,
        n_kv_heads: usize,
        head_dim: usize,
    ) -> Result<Self> {
        let slot_elems = n_kv_heads * head_dim;
        let slab_elems = max_ctx * slot_elems;
        let stream = device.raw().default_stream();

        let mut k_slabs = Vec::with_capacity(n_layers);
        let mut v_slabs = Vec::with_capacity(n_layers);
        for _ in 0..n_layers {
            k_slabs.push(
                stream
                    .alloc_zeros::<bf16>(slab_elems)
                    .map_err(|e| anyhow!("KvCache: alloc k slab: {:?}", e))?,
            );
            v_slabs.push(
                stream
                    .alloc_zeros::<bf16>(slab_elems)
                    .map_err(|e| anyhow!("KvCache: alloc v slab: {:?}", e))?,
            );
        }
        Ok(Self {
            k_slabs,
            v_slabs,
            n_layers,
            max_ctx,
            n_kv_heads,
            head_dim,
            n_filled: 0,
            device,
        })
    }

    pub fn n_layers(&self) -> usize {
        self.n_layers
    }

    pub fn max_ctx(&self) -> usize {
        self.max_ctx
    }

    pub fn n_kv_heads(&self) -> usize {
        self.n_kv_heads
    }

    pub fn head_dim(&self) -> usize {
        self.head_dim
    }

    pub fn n_filled(&self) -> usize {
        self.n_filled
    }

    pub fn slot_elems(&self) -> usize {
        self.n_kv_heads * self.head_dim
    }

    pub fn k_slab(&self, layer: usize) -> &CudaSlice<bf16> {
        &self.k_slabs[layer]
    }

    pub fn k_slab_mut(&mut self, layer: usize) -> &mut CudaSlice<bf16> {
        &mut self.k_slabs[layer]
    }

    pub fn v_slab(&self, layer: usize) -> &CudaSlice<bf16> {
        &self.v_slabs[layer]
    }

    pub fn v_slab_mut(&mut self, layer: usize) -> &mut CudaSlice<bf16> {
        &mut self.v_slabs[layer]
    }

    pub fn advance(&mut self, n: usize) {
        debug_assert!(self.n_filled + n <= self.max_ctx, "KvCache overflow");
        self.n_filled += n;
    }

    pub fn reset(&mut self) {
        self.n_filled = 0;
    }

    pub fn device(&self) -> &Arc<DeviceContext> {
        &self.device
    }
}

impl std::fmt::Debug for KvCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KvCache")
            .field("n_layers", &self.n_layers)
            .field("max_ctx", &self.max_ctx)
            .field("n_kv_heads", &self.n_kv_heads)
            .field("head_dim", &self.head_dim)
            .field("n_filled", &self.n_filled)
            .finish()
    }
}
