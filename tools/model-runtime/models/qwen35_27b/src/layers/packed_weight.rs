//! On-device packed weight carrier.
//!
//! A `[K, N]` weight matrix at GGUF-load time can arrive in several
//! dtypes:
//!
//!   * `bf16` — unpacked (what the first-port layers used everywhere).
//!   * `Q4_K_M`, `Q5_K`, `Q6_K`, `Q8_0` — llama.cpp-style block-quant
//!     bytes (packed on device as `CudaTensor<i8>`).
//!
//! Per-layer forward() code shouldn't have to know which of these it's
//! looking at. `PackedWeight` wraps the carrier + metadata and
//! dispatches to the right mat-vec / mat-mul kernel in a single
//! `matmul_f32` entry point. The residual-stream casts (bf16 → f32
//! at pre-projection, f32 → bf16 after) now flank every projection;
//! they were already present for rmsnorm, so the refactor is
//! essentially "leave the f32 staging tensor in place through the
//! projection rather than round-tripping through bf16".
//!
//! # API contract
//!
//! `matmul_f32(device, x, y)` computes
//!
//! ```text
//!     y[m, n]  ←  x[m, k] · A[k, n]
//! ```
//!
//! where `A` is the weight this `PackedWeight` represents. `x` and `y`
//! are both `CudaTensor<f32>`. Shapes are checked against
//! `self.dims()` (the stored `(k, n)`). `m` is inferred from
//! `x.shape()[0]` (with `y.shape()[0] == m` and `y.shape()[1] == n`).
//!
//! ## Dispatch table
//!
//! | Variant | Path                                                    |
//! |---------|---------------------------------------------------------|
//! | `Bf16`  | cast x → bf16; `launch_matmul_bf16_f32` (cuBLAS GEMM)   |
//! | `Q4K`   | bulk quantize x → q8_1; m view-launches of q4k kernel   |
//! | `IQ4XS` | bulk quantize x → q8_1; m view-launches of iq4_xs kernel|
//! | `Q5K`   | m view-launches of q5k kernel over f32 x-rows           |
//! | `Q6K`   | m view-launches of q6k kernel over f32 x-rows           |
//! | `Q8_0`  | m view-launches of q8_0 kernel over f32 x-rows          |
//! | `Zero`  | memset y to zeros; no kernel launch                     |
//!
//! Earlier iterations used an owned-`CudaTensor` per-row scratch
//! with two `memcpy_dtod` calls per row (input fetch + output
//! scatter). At 1024-token prefill that cost ~393k cudaMemcpy
//! launches per forward — pure orchestration overhead on top of the
//! actual matmul work. The current form skips both memcpys by
//! handing the kernel a `CudaView` into `x` (and `CudaViewMut` into
//! `y`) directly; the only remaining per-layer allocation is the
//! q8_1 scratch for Q4K / IQ4_XS, which is populated in a single
//! quantize launch rather than m.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use half::bf16;

use ctox_cuda_primitives::device::DeviceContext;
use ctox_cuda_primitives::tensor::CudaTensor;

use crate::kernels::{
    launch_cast_f32_to_bf16, launch_matmul_bf16_f32, launch_mmvq_iq4_xs_q8_1_f32_view,
    launch_mmvq_q4k_q8_1_f32_view, launch_mmvq_q5k_f32_view, launch_mmvq_q6k_f32_view,
    launch_mmvq_q8_0_f32_view, launch_quantize_q8_1_f32, q8_1_packed_bytes,
};

/// A weight tensor as it lives on device — carrier type depends on
/// the GGUF dtype the loader saw at read time. Each variant stores the
/// logical `[K, N]` shape alongside the carrier so forward() code can
/// dispatch on the variant without having to know the packed byte
/// count per block.
///
/// `matmul_f32` below routes the call to the right kernel; callers
/// don't match on the variant directly.
pub enum PackedWeight {
    /// `[K, N]` bf16 dense. Used today for RMSNorm-adjacent linear
    /// ops and for smoke tests that synthesize random weights.
    Bf16 {
        t: CudaTensor<bf16>,
        k: usize,
        n: usize,
    },
    /// `[K, N]` Q4_K_M packed bytes (n_elements / 256 × 144 bytes).
    Q4K {
        t: CudaTensor<i8>,
        k: usize,
        n: usize,
    },
    /// `[K, N]` Q5_K packed bytes (n_elements / 256 × 176 bytes).
    Q5K {
        t: CudaTensor<i8>,
        k: usize,
        n: usize,
    },
    /// `[K, N]` Q6_K packed bytes (n_elements / 256 × 210 bytes).
    Q6K {
        t: CudaTensor<i8>,
        k: usize,
        n: usize,
    },
    /// `[K, N]` Q8_0 packed bytes (n_elements / 32 × 34 bytes).
    Q8_0 {
        t: CudaTensor<i8>,
        k: usize,
        n: usize,
    },
    /// `[K, N]` IQ4_XS packed bytes (n_elements / 256 × 136 bytes).
    /// The shipping 27B GGUF ships `ffn_gate.weight` and `ffn_up.weight`
    /// in this format; dispatch goes through `launch_mmvq_iq4_xs_f32`.
    IQ4XS {
        t: CudaTensor<i8>,
        k: usize,
        n: usize,
    },
    /// Zero placeholder — the weight wasn't loaded (missing from GGUF
    /// or unsupported dtype). Forward path still runs; output is all
    /// zeros for this projection. Matches the pre-refactor behavior
    /// of `load_bf16_placeholder` returning a zeroed bf16 tensor.
    Zero { k: usize, n: usize },
}

impl PackedWeight {
    /// `(K, N)` logical shape of the weight.
    pub fn dims(&self) -> (usize, usize) {
        match self {
            PackedWeight::Bf16 { k, n, .. } => (*k, *n),
            PackedWeight::Q4K { k, n, .. } => (*k, *n),
            PackedWeight::Q5K { k, n, .. } => (*k, *n),
            PackedWeight::Q6K { k, n, .. } => (*k, *n),
            PackedWeight::Q8_0 { k, n, .. } => (*k, *n),
            PackedWeight::IQ4XS { k, n, .. } => (*k, *n),
            PackedWeight::Zero { k, n } => (*k, *n),
        }
    }

    /// `y[m, n]  ←  x[m, k] · A[k, n]`, f32 in/out, dispatching on the
    /// carrier variant. `m` is taken from `x.shape()[0]`.
    ///
    /// Validates:
    ///   * `x.shape() == [m, k]`
    ///   * `y.shape() == [m, n]`
    ///
    /// Returns an error on any mismatch.
    pub fn matmul_f32(
        &self,
        device: &Arc<DeviceContext>,
        x: &CudaTensor<f32>,
        y: &mut CudaTensor<f32>,
    ) -> Result<()> {
        let (k, n) = self.dims();

        if x.shape().len() != 2 {
            return Err(anyhow!(
                "PackedWeight::matmul_f32: x must be 2D [m, k], got {:?}",
                x.shape()
            ));
        }
        let m = x.shape()[0];
        if x.shape()[1] != k {
            return Err(anyhow!(
                "PackedWeight::matmul_f32: x.shape()[1]={} != k={}",
                x.shape()[1],
                k
            ));
        }
        if y.shape() != [m, n] {
            return Err(anyhow!(
                "PackedWeight::matmul_f32: y.shape {:?} != [{}, {}]",
                y.shape(),
                m,
                n
            ));
        }

        match self {
            PackedWeight::Bf16 { t, .. } => matmul_bf16_batched(device, t, x, y, m, k, n),
            PackedWeight::Q4K { t, .. } => matmul_q8_1_rows(device, x, y, m, k, n, |dev, xv, yv| {
                launch_mmvq_q4k_q8_1_f32_view(dev, t, k, n, xv, yv)
            }),
            PackedWeight::IQ4XS { t, .. } => {
                matmul_q8_1_rows(device, x, y, m, k, n, |dev, xv, yv| {
                    launch_mmvq_iq4_xs_q8_1_f32_view(dev, t, k, n, xv, yv)
                })
            }
            PackedWeight::Q5K { t, .. } => {
                matmul_f32_rows(device, x, y, m, k, n, |dev, xv, yv| {
                    launch_mmvq_q5k_f32_view(dev, t, k, n, xv, yv)
                })
            }
            PackedWeight::Q6K { t, .. } => {
                matmul_f32_rows(device, x, y, m, k, n, |dev, xv, yv| {
                    launch_mmvq_q6k_f32_view(dev, t, k, n, xv, yv)
                })
            }
            PackedWeight::Q8_0 { t, .. } => {
                matmul_f32_rows(device, x, y, m, k, n, |dev, xv, yv| {
                    launch_mmvq_q8_0_f32_view(dev, t, k, n, xv, yv)
                })
            }
            PackedWeight::Zero { .. } => zero_fill_f32(y, m * n),
        }
    }
}

/// `y[m, n] ← x[m, k] · A[k, n]` for the Bf16 variant.
///
/// cuBLAS wants bf16 inputs; we stage `x` into a bf16 scratch buffer
/// and call `launch_matmul_bf16_f32` which writes the f32 accumulator
/// directly. One bf16 scratch allocation per call — the caller's `y`
/// is already f32.
fn matmul_bf16_batched(
    device: &Arc<DeviceContext>,
    a_bf16: &CudaTensor<bf16>,
    x: &CudaTensor<f32>,
    y: &mut CudaTensor<f32>,
    m: usize,
    k: usize,
    n: usize,
) -> Result<()> {
    if a_bf16.shape() != [k, n] {
        return Err(anyhow!(
            "PackedWeight::Bf16: a.shape {:?} != [{}, {}]",
            a_bf16.shape(),
            k,
            n
        ));
    }
    let mut x_bf16 = CudaTensor::<bf16>::zeros(device.clone(), vec![m, k])?;
    // Need a bf16 view of x — do a device-side cast from f32 to bf16
    // via the existing kernel. Keeping the `f32` output on `y` lets
    // the caller keep downstream ops (softmax inputs, etc.) in f32
    // when they need the precision.
    launch_cast_f32_to_bf16(device, x, &mut x_bf16)?;
    launch_matmul_bf16_f32(device, &x_bf16, a_bf16, y, m, k, n)?;
    Ok(())
}

/// Batched per-row dispatch for Q4K / IQ4_XS — quantized weights whose
/// CUDA kernel consumes a pre-quantized q8_1 activation.
///
/// Layout strategy:
///   1. Allocate one q8_1 scratch of `m * q8_1_packed_bytes(k)` bytes.
///   2. Run `launch_quantize_q8_1_f32` **once** over the full `m*k`
///      activation. Since each q8_1 block is independent (32-elem
///      scale/zero), quantizing `m*k` elements contiguously produces
///      exactly the same layout as quantizing each of the `m` rows
///      separately into its own sub-buffer — as long as `k` is a
///      multiple of 32, which every Qwen3.5-27B projection satisfies.
///   3. For each of the `m` output rows, hand the kernel a `CudaView`
///      into the pre-quantized scratch and a `CudaViewMut` into the
///      destination `y` row. Zero D2D memcpys, `m` kernel launches.
///
/// This replaces the previous scheme (per-row `CudaTensor` scratch +
/// two `memcpy_dtod` per row) which was pure orchestration overhead
/// — ~393k cudaMemcpy calls per 1024-token prefill before the
/// refactor. See the top-level audit in the A1 commit message.
fn matmul_q8_1_rows<F>(
    device: &Arc<DeviceContext>,
    x: &CudaTensor<f32>,
    y: &mut CudaTensor<f32>,
    m: usize,
    k: usize,
    n: usize,
    mut launch: F,
) -> Result<()>
where
    F: FnMut(
        &Arc<DeviceContext>,
        &cudarc::driver::CudaView<'_, i8>,
        &mut cudarc::driver::CudaViewMut<'_, f32>,
    ) -> Result<()>,
{
    if m == 0 {
        return Ok(());
    }
    let row_bytes = q8_1_packed_bytes(k);
    let total_elems = m * k;
    let mut x_q8_1_scratch =
        CudaTensor::<i8>::zeros(device.clone(), vec![m * row_bytes])
            .map_err(|e| anyhow!("matmul_q8_1_rows: alloc q8_1 scratch ({}B): {:?}", m * row_bytes, e))?;
    launch_quantize_q8_1_f32(device, x, &mut x_q8_1_scratch, total_elems)?;

    for t in 0..m {
        let x_start = t * row_bytes;
        let x_end = x_start + row_bytes;
        let x_view = x_q8_1_scratch.buf().slice(x_start..x_end);

        let y_start = t * n;
        let y_end = y_start + n;
        let mut y_view = y.buf_mut().slice_mut(y_start..y_end);

        launch(device, &x_view, &mut y_view)
            .map_err(|e| anyhow!("matmul_q8_1_rows: row {} launch: {}", t, e))?;
    }
    Ok(())
}

/// Batched per-row dispatch for Q5K / Q6K / Q8_0 — quantized weights
/// whose CUDA kernel consumes a raw f32 activation (no q8_1
/// intermediate today; see each kernel's module doc for the TODO).
///
/// Layout strategy: just hand the kernel a `CudaView` into the
/// activation `x` for each row and a `CudaViewMut` into the output
/// `y` row. No scratch allocations, no D2D memcpys, `m` kernel
/// launches.
fn matmul_f32_rows<F>(
    device: &Arc<DeviceContext>,
    x: &CudaTensor<f32>,
    y: &mut CudaTensor<f32>,
    m: usize,
    k: usize,
    n: usize,
    mut launch: F,
) -> Result<()>
where
    F: FnMut(
        &Arc<DeviceContext>,
        &cudarc::driver::CudaView<'_, f32>,
        &mut cudarc::driver::CudaViewMut<'_, f32>,
    ) -> Result<()>,
{
    if m == 0 {
        return Ok(());
    }
    for t in 0..m {
        let x_start = t * k;
        let x_end = x_start + k;
        let x_view = x.buf().slice(x_start..x_end);

        let y_start = t * n;
        let y_end = y_start + n;
        let mut y_view = y.buf_mut().slice_mut(y_start..y_end);

        launch(device, &x_view, &mut y_view)
            .map_err(|e| anyhow!("matmul_f32_rows: row {} launch: {}", t, e))?;
    }
    Ok(())
}

/// Overwrite `y`'s first `numel` elements with zeros. Used by the
/// `Zero` variant. Implemented via a host zero-vector upload so we
/// don't need a dedicated memset kernel; `numel = m * n` is tiny
/// compared to any real projection and this runs once per `Zero`
/// forward, not inside the layer loop.
fn zero_fill_f32(y: &mut CudaTensor<f32>, numel: usize) -> Result<()> {
    let stream = y.device().raw().default_stream();
    let zeros = vec![0.0f32; numel];
    stream
        .memcpy_htod(&zeros, y.buf_mut())
        .map_err(|e| anyhow!("PackedWeight::Zero zero_fill htod ({}): {:?}", numel, e))?;
    Ok(())
}

