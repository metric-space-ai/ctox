//! CUDA device + stream context. One per process (single device for
//! now — multi-GPU comes with NCCL wiring later). All allocations,
//! kernel launches, and memcpys go through a `DeviceContext`.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use cudarc::driver::CudaContext;

/// Handle on a single CUDA device + default stream.
///
/// Clone is cheap: internally `Arc<CudaContext>`.
#[derive(Clone)]
pub struct DeviceContext {
    ctx: Arc<CudaContext>,
    ordinal: usize,
}

impl DeviceContext {
    /// Initialize CUDA on device `ordinal` (0-indexed).
    pub fn new(ordinal: usize) -> Result<Self> {
        let ctx = CudaContext::new(ordinal)
            .map_err(|e| anyhow!("init CUDA context on device {}: {:?}", ordinal, e))?;
        Ok(Self { ctx, ordinal })
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
}

impl std::fmt::Debug for DeviceContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceContext")
            .field("ordinal", &self.ordinal)
            .finish()
    }
}
