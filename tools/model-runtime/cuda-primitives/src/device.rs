//! CUDA device + stream context. One per process (single device for
//! now — multi-GPU comes with NCCL wiring later). All allocations,
//! kernel launches, and memcpys go through a `DeviceContext`.
//!
//! The context also owns a lazily-initialized per-device `CudaBlas`
//! handle. cuBLAS is bound to the context's default stream at handle
//! creation, so every kernel that uses it (right now: the dense bf16
//! matmul path) serializes naturally against the rest of our kernels
//! on that stream.

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use cudarc::cublas::{sys as cublas_sys, CudaBlas};
use cudarc::driver::CudaContext;

/// Handle on a single CUDA device + default stream.
///
/// Clone is cheap: internally `Arc<CudaContext>` plus an `Arc<OnceLock>`
/// for the cached cuBLAS handle.
#[derive(Clone)]
pub struct DeviceContext {
    ctx: Arc<CudaContext>,
    ordinal: usize,
    // Lazily created, shared across clones. `OnceLock` gives us "first
    // caller wins, every subsequent caller sees the same handle" with
    // no locking on the common path.
    cublas: Arc<OnceLock<Arc<CudaBlas>>>,
}

impl DeviceContext {
    /// Initialize CUDA on device `ordinal` (0-indexed).
    pub fn new(ordinal: usize) -> Result<Self> {
        let ctx = CudaContext::new(ordinal)
            .map_err(|e| anyhow!("init CUDA context on device {}: {:?}", ordinal, e))?;
        Ok(Self {
            ctx,
            ordinal,
            cublas: Arc::new(OnceLock::new()),
        })
    }

    pub fn ordinal(&self) -> usize {
        self.ordinal
    }

    pub fn raw(&self) -> &Arc<CudaContext> {
        &self.ctx
    }

    /// Block the host until every op queued so far has completed.
    /// Use sparingly — this serializes the pipeline.
    pub fn synchronize(&self) -> Result<()> {
        self.ctx
            .default_stream()
            .synchronize()
            .map_err(|e| anyhow!("synchronize default stream: {:?}", e))?;
        Ok(())
    }

    /// Lazily initialize and return this device's cuBLAS handle.
    ///
    /// The handle is bound to the context's default stream so cuBLAS
    /// calls participate in the same ordering as our hand-rolled
    /// kernels on that stream (no extra syncs needed). The handle is
    /// thread-safe (cudarc marks `CudaBlas: Send + Sync`); callers share
    /// one per device.
    pub fn cublas(&self) -> Result<Arc<CudaBlas>> {
        if let Some(b) = self.cublas.get() {
            return Ok(b.clone());
        }
        let handle = CudaBlas::new(self.ctx.default_stream())
            .map_err(|e| anyhow!("CudaBlas::new on device {}: {:?}", self.ordinal, e))?;
        // Force pedantic math: keep the tensor-core bf16/fp16 path but
        // disable cuBLAS's otherwise-enabled "reduced-precision
        // reduction" optimization inside TC GEMMs. Without this,
        // CUBLAS_DEFAULT_MATH may truncate the per-tile f32 partial
        // sums to bf16 between warp fragments (faster, but visibly
        // less accurate than an ordered f32 kernel). Pedantic mode
        // keeps full f32 accumulation end-to-end, which is what the
        // correctness golden expects.
        unsafe {
            let status = cublas_sys::cublasSetMathMode(
                *handle.handle(),
                cublas_sys::cublasMath_t::CUBLAS_PEDANTIC_MATH,
            );
            if status != cublas_sys::cublasStatus_t::CUBLAS_STATUS_SUCCESS {
                return Err(anyhow!(
                    "cublasSetMathMode(PEDANTIC) on device {}: {:?}",
                    self.ordinal,
                    status
                ));
            }
        }
        let b = Arc::new(handle);
        // If another thread raced us, `set` returns Err and we drop our
        // instance, using theirs instead. Either way we end up with the
        // same single handle across clones.
        let _ = self.cublas.set(b.clone());
        Ok(self.cublas.get().cloned().unwrap_or(b))
    }
}

impl std::fmt::Debug for DeviceContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceContext")
            .field("ordinal", &self.ordinal)
            .field("cublas_initialized", &self.cublas.get().is_some())
            .finish()
    }
}
