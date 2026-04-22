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
use crate::tensor::CudaTensor;

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

    /// Decrement `n_filled` by `n` without touching slab contents.
    ///
    /// Used by multi-layer forward passes: each FullAttention layer
    /// today auto-advances `n_filled` inside its own `forward`. When
    /// several FA layers run in a single forward, they all want to see
    /// the same pre-layer `n_filled` as their write offset + attention
    /// length. The target-level forward calls `rewind` between layers
    /// so every FA layer observes the same `prompt_start`, then calls
    /// `advance` once at the end after all layers have written.
    ///
    /// Panics (in debug) on underflow. Slab bytes for the "unfilled"
    /// range are left unchanged — subsequent writes at offset
    /// `n_filled` will overwrite them.
    pub fn rewind(&mut self, n: usize) {
        debug_assert!(n <= self.n_filled, "KvCache rewind underflow");
        self.n_filled -= n;
    }

    pub fn reset(&mut self) {
        self.n_filled = 0;
    }

    pub fn device(&self) -> &Arc<DeviceContext> {
        &self.device
    }

    /// Append a `[n_tokens, n_kv_heads, head_dim]` bf16 tensor into the
    /// K slab for `layer`, starting at row `offset` along the max_ctx
    /// axis. The slab is contiguous along its fastest axis
    /// (`n_kv_heads × head_dim`), so a freshly-produced K projection —
    /// laid out `[n_tokens, n_kv_heads, head_dim]` row-major — can be
    /// blitted as a single contiguous device-to-device memcpy.
    ///
    /// Does NOT advance `n_filled`; callers call [`KvCache::advance`]
    /// after both K and V have been written.
    pub fn append_k(
        &mut self,
        layer: usize,
        offset: usize,
        src: &CudaTensor<bf16>,
    ) -> Result<()> {
        self.append_inner(layer, offset, src, CacheAxis::K)
    }

    /// Same as [`KvCache::append_k`] but for the V slab.
    pub fn append_v(
        &mut self,
        layer: usize,
        offset: usize,
        src: &CudaTensor<bf16>,
    ) -> Result<()> {
        self.append_inner(layer, offset, src, CacheAxis::V)
    }

    fn append_inner(
        &mut self,
        layer: usize,
        offset: usize,
        src: &CudaTensor<bf16>,
        axis: CacheAxis,
    ) -> Result<()> {
        if layer >= self.n_layers {
            return Err(anyhow!(
                "KvCache::append_{:?}: layer {} out of range (n_layers={})",
                axis,
                layer,
                self.n_layers
            ));
        }
        let slot_elems = self.slot_elems();
        let expected_shape = [
            src.shape().first().copied().unwrap_or(0),
            self.n_kv_heads,
            self.head_dim,
        ];
        if src.shape().len() != 3
            || src.shape()[1] != self.n_kv_heads
            || src.shape()[2] != self.head_dim
        {
            return Err(anyhow!(
                "KvCache::append_{:?}: src shape {:?} must be [n_tokens, n_kv_heads={}, head_dim={}]",
                axis,
                src.shape(),
                self.n_kv_heads,
                self.head_dim
            ));
        }
        let n_tokens = expected_shape[0];
        if n_tokens == 0 {
            return Ok(());
        }
        if offset + n_tokens > self.max_ctx {
            return Err(anyhow!(
                "KvCache::append_{:?}: offset {} + n_tokens {} > max_ctx {}",
                axis,
                offset,
                n_tokens,
                self.max_ctx
            ));
        }

        let start = offset * slot_elems;
        let end = (offset + n_tokens) * slot_elems;
        let slab = match axis {
            CacheAxis::K => &mut self.k_slabs[layer],
            CacheAxis::V => &mut self.v_slabs[layer],
        };
        let stream = self.device.raw().default_stream();
        let mut dst_view = slab.slice_mut(start..end);
        stream
            .memcpy_dtod(src.buf(), &mut dst_view)
            .map_err(|e| {
                anyhow!(
                    "KvCache::append_{:?} memcpy_dtod (layer={} offset={} n_tokens={}): {:?}",
                    axis,
                    layer,
                    offset,
                    n_tokens,
                    e
                )
            })?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
enum CacheAxis {
    K,
    V,
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
