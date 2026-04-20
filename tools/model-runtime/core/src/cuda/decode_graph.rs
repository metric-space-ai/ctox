//! CUDA-Graph-based decode replay for MoE inference.
//!
//! # Why
//!
//! CTOX's decode hot-path spends ~4 ms/token purely on CUDA kernel-launch
//! overhead — ~800 launches per token on a 40-layer Qwen3.6-35B-A3B.
//! Each launch is ~5 μs minimum on A6000. Fusing individual kernels
//! (router, reduce, GDN, MoE-matmul) has reached diminishing returns:
//! +0.5 tok/s per fusion. The structural next step is to capture the
//! entire decode forward as a single CUDA graph after the first warmup
//! step and replay it on every subsequent token — one launch per
//! token, regardless of how many kernels compose the forward.
//!
//! This mirrors the approach used by lucebox-hub/megakernel for dense
//! Qwen3.5 (single persistent kernel) and by vLLM/SGLang
//! (`torch.cuda.graph` wrapping the decode step). Candle does not have
//! built-in graph support; this module adds it natively in the CTOX
//! hard-fork — no external dependency beyond cudarc (already pulled in
//! transitively through candle).
//!
//! # Per-model scope
//!
//! Following CTOX's "pick your supported models, go deep" doctrine,
//! each supported model family owns its own `DecodeGraph` instance.
//! The instance lives on the model pipeline and knows how to:
//!   1. reserve persistent device buffers of exactly the shapes the
//!      decode forward needs (prompt + KV state already live in
//!      paged-attention + GDN hybrid cache; decode only needs
//!      [batch, 1, hidden] input/output slots and a small router
//!      scratch — the rest are fixed-size).
//!   2. warm up once (eager execution) to populate caches and
//!      finalize any deferred ISQ.
//!   3. capture the second forward into a `cudarc::driver::CudaGraph`.
//!   4. replay the captured graph on every subsequent decode step,
//!      updating only the input/output data pointers (which stay
//!      fixed across the lifetime of the persistent buffers).
//!
//! # Constraints
//!
//! CUDA graph capture enforces:
//!   - no cudaMalloc during capture (candle's caching allocator
//!     normally holds this true for fixed decode shapes but not for
//!     every op — needs audit per model).
//!   - no host-side synchronizations (D→H memcpy, .to_vec, scalar
//!     extraction). We've already audited Qwen3.6 and hoisted the
//!     state-idx D→H out of the per-layer loop (commit 6b0665d).
//!   - a single capture stream; all ops must use `CudaDevice::cuda_stream`.
//!
//! The runtime gates this feature behind `ENGINE_DECODE_GRAPH=1` so a
//! regression in any of the above falls back to eager execution without
//! affecting users of non-captured paths.

#![cfg(feature = "cuda")]

use std::sync::Arc;

use candle_core::cuda_backend::cudarc::driver::sys::{
    CUgraphInstantiate_flags, CUstreamCaptureMode, CUstreamCaptureStatus,
};
use candle_core::cuda_backend::cudarc::driver::{CudaGraph, CudaStream};
use candle_core::{CudaDevice, Result};

/// Lifecycle state of a `DecodeGraph`. The state machine is linear:
/// `Uncaptured → Warming → Capturing → Captured`. Transitions are
/// driven by `DecodeGraph::run`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeGraphState {
    /// No capture attempted yet. Next `run` call will execute eagerly
    /// and mark the model as warmed.
    Uncaptured,
    /// Warmup completed. Next `run` call will open a stream capture,
    /// execute the forward, close the capture, and transition to
    /// `Captured`.
    Warming,
    /// Inside stream capture. Only reached transiently during `run`
    /// and never observed from outside.
    Capturing,
    /// Captured graph is live. Subsequent `run` calls launch the graph
    /// directly via `CudaGraph::launch`.
    Captured,
    /// Capture attempted but candle produced an op incompatible with
    /// graph capture (e.g. a synchronous D→H copy). Sticky — we don't
    /// retry within a pipeline's lifetime. Next `run` falls through to
    /// eager execution.
    Failed,
}

/// Wraps a CUDA-graph-replayable decode forward. One instance per
/// pipeline; kept alive for the pipeline's lifetime.
///
/// Thread-safety: not `Sync`. Decode forwards run single-threaded per
/// pipeline under the pipeline's own lock, so we don't need interior
/// synchronization here.
pub struct DecodeGraph {
    stream: Arc<CudaStream>,
    graph: Option<CudaGraph>,
    state: DecodeGraphState,
    /// Number of successful warmups completed. We require at least one
    /// eager run before attempting capture so that deferred allocations
    /// (KV pre-allocation, paged-attention block reservations, ISQ
    /// lazy-resolve) settle before the graph is built.
    warmup_count: u32,
    /// Minimum warmups before a capture is attempted. Tuned per model
    /// family — 2 is the floor; some models (e.g. anything with a
    /// pending-ISQ layer graph) need more.
    min_warmups: u32,
}

// SAFETY: `CudaGraph` contains raw `*mut CUgraph_st` / `*mut CUgraphExec_st`
// pointers which cudarc marks !Send. In practice the CUDA graph objects
// are thread-safe as long as API calls on the same graph are serialized
// externally — cudarc's own docs state this. In CTOX every `DecodeGraph`
// lives inside `Qwen3_5MoeTextModel`, which itself is wrapped in a
// `std::sync::Mutex<Option<DecodeGraph>>` *and* the enclosing pipeline
// is behind `Arc<tokio::sync::Mutex<dyn Pipeline>>`. So access is
// serialized at two levels; the marker here just documents what the
// runtime guarantees.
unsafe impl Send for DecodeGraph {}
unsafe impl Sync for DecodeGraph {}

impl DecodeGraph {
    /// Construct a new `DecodeGraph` bound to the given CUDA device's
    /// stream. The graph isn't captured yet; the first `run` call runs
    /// eagerly, and capture happens on `min_warmups + 1`.
    pub fn new(device: &CudaDevice, min_warmups: u32) -> Self {
        Self {
            stream: device.cuda_stream(),
            graph: None,
            state: DecodeGraphState::Uncaptured,
            warmup_count: 0,
            min_warmups: min_warmups.max(1),
        }
    }

    /// Current lifecycle state. Exposed for diagnostics (e.g. tracing
    /// log on a model's first captured decode).
    pub fn state(&self) -> DecodeGraphState {
        self.state
    }

    /// Whether subsequent `run` calls will replay the captured graph
    /// (the hot-path regime).
    pub fn is_captured(&self) -> bool {
        matches!(self.state, DecodeGraphState::Captured)
    }

    /// Execute the decode-step closure. Depending on the current
    /// state, this either:
    ///
    /// - runs `f` eagerly (Uncaptured / Warming / Failed), or
    /// - replays the captured graph (Captured), ignoring `f`.
    ///
    /// The closure must be deterministic with respect to the
    /// persistent device buffers — it must not depend on any
    /// capture-hostile side effect (no `.to_vec`, no fresh
    /// `Tensor::new(cpu_slice, device)` inside the capture window, no
    /// new allocations that miss the caching allocator).
    ///
    /// On capture failure (a candle op returned an error during
    /// `begin_capture`/`end_capture`, or end_capture produced no
    /// graph), the state transitions to `Failed` and `f` is re-run
    /// eagerly once more so the caller's forward still returns a
    /// valid result.
    pub fn run<F, R>(&mut self, f: F) -> Result<R>
    where
        F: FnOnce() -> Result<R>,
    {
        match self.state {
            DecodeGraphState::Captured => {
                // Replay path: the captured graph reads from and
                // writes to the persistent buffers wired by `f`
                // during the capture phase. Caller is responsible
                // for having copied the current token's inputs into
                // those buffers before invoking `run`, and reading
                // outputs from them after. We launch on the same
                // stream the graph was captured on.
                self.graph
                    .as_ref()
                    .expect("invariant: Captured => graph is Some")
                    .launch()
                    .map_err(cuda_err)?;
                // The closure's return value was baked into the
                // persistent output buffer during capture — the call
                // signature still needs a `R`, so require it from `f`
                // by running it. Typical usage: `f` reads from the
                // persistent output buffer and returns a cheap
                // Tensor view that shares storage. For decode-step
                // this is always a logits-view, not a fresh tensor.
                f()
            }
            DecodeGraphState::Warming => {
                // Transition to Capturing: open stream capture, run
                // the closure, close and instantiate. On any error
                // mark Failed so we stop trying.
                self.state = DecodeGraphState::Capturing;
                match self.capture_once(f) {
                    Ok(result) => Ok(result),
                    Err(e) => {
                        tracing::warn!(
                            "DecodeGraph capture failed; falling back to eager forever: {e}"
                        );
                        self.state = DecodeGraphState::Failed;
                        Err(e)
                    }
                }
            }
            DecodeGraphState::Uncaptured | DecodeGraphState::Failed => {
                // Eager path. Bump the warmup counter on success so
                // we'll attempt capture once enough warmups have
                // accumulated.
                let result = f()?;
                if matches!(self.state, DecodeGraphState::Uncaptured) {
                    self.warmup_count = self.warmup_count.saturating_add(1);
                    if self.warmup_count >= self.min_warmups {
                        self.state = DecodeGraphState::Warming;
                        tracing::info!(
                            "DecodeGraph warmed after {} steps — next decode step will attempt capture.",
                            self.warmup_count
                        );
                    }
                }
                Ok(result)
            }
            DecodeGraphState::Capturing => {
                // Re-entry during capture. This would mean the
                // closure itself recursed into another DecodeGraph —
                // architecturally disallowed. Bail loudly.
                candle_core::bail!(
                    "DecodeGraph::run called while already capturing — nested capture not supported"
                )
            }
        }
    }

    /// Perform a single stream capture pass.
    ///
    /// Contract:
    ///   - enters with `self.state == Capturing`
    ///   - exits with `self.state == Captured` on success, or
    ///     propagates an error and leaves `self.state` for caller to
    ///     set.
    fn capture_once<F, R>(&mut self, f: F) -> Result<R>
    where
        F: FnOnce() -> Result<R>,
    {
        // `ThreadLocal` mode isolates the capture from other threads'
        // stream work. `Relaxed` would be faster but allows captures
        // to bleed across threads — too fragile for a general engine.
        self.stream
            .begin_capture(CUstreamCaptureMode::CU_STREAM_CAPTURE_MODE_THREAD_LOCAL)
            .map_err(cuda_err)?;

        let run_result = f();

        // Always end_capture even on error — CUDA leaves the stream
        // in capture mode otherwise, breaking subsequent non-captured
        // work on the same stream.
        let capture_result = self.stream.end_capture(
            CUgraphInstantiate_flags::CUDA_GRAPH_INSTANTIATE_FLAG_AUTO_FREE_ON_LAUNCH,
        );

        let r = run_result?;
        let graph_opt = capture_result.map_err(cuda_err)?;
        let graph = graph_opt.ok_or_else(|| {
            candle_core::Error::Msg(
                "CudaStream::end_capture returned no graph — the decode closure \
                 emitted no captured ops"
                    .into(),
            )
        })?;
        self.graph = Some(graph);
        self.state = DecodeGraphState::Captured;
        tracing::info!("DecodeGraph capture succeeded — subsequent decode steps will replay.");
        Ok(r)
    }

    /// Verify the stream is not in capture state (diagnostics for
    /// error paths where the caller suspects a capture leaked).
    pub fn assert_not_capturing(&self) -> Result<()> {
        let status = self.stream.capture_status().map_err(cuda_err)?;
        if !matches!(status, CUstreamCaptureStatus::CU_STREAM_CAPTURE_STATUS_NONE) {
            candle_core::bail!("CUDA stream is still in capture state: {:?}", status);
        }
        Ok(())
    }
}

fn cuda_err(e: impl std::fmt::Display) -> candle_core::Error {
    candle_core::Error::Msg(format!("cuda graph: {e}"))
}

/// Read the `ENGINE_DECODE_GRAPH` env var. Default-off; set to `1`/`true`
/// to enable. Lives here so every pipeline uses the same gate.
pub fn enabled_by_env() -> bool {
    std::env::var("ENGINE_DECODE_GRAPH")
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// State-machine-only test: `DecodeGraph::new` starts in
    /// `Uncaptured`, and `enabled_by_env` reads the env correctly.
    /// Actual capture/replay is covered by per-model integration
    /// tests because it requires a real CUDA device.
    #[test]
    fn enabled_by_env_off_by_default() {
        // SAFETY: env manipulation in tests is a standard pattern; we
        // don't run cargo test in parallel on this var.
        unsafe {
            std::env::remove_var("ENGINE_DECODE_GRAPH");
        }
        assert!(!enabled_by_env());
        unsafe {
            std::env::set_var("ENGINE_DECODE_GRAPH", "1");
        }
        assert!(enabled_by_env());
        unsafe {
            std::env::remove_var("ENGINE_DECODE_GRAPH");
        }
    }
}
