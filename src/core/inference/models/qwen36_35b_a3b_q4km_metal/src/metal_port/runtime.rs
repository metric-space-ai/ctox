// Origin: CTOX
// License: Apache-2.0

//! Minimal Metal runtime: pick the system default `MTLDevice`, hand
//! out a `MTLCommandQueue`, and load the vendored kernel
//! `default.metallib` baked into the binary by `build.rs`.
//!
//! This module is the **only** place we touch the objc2-metal API
//! surface. Per-op dispatchers (`metal_port::ops::*`) take a
//! `&MetalRuntime` and only see `MTLBuffer` + `MTLComputePipelineState`
//! handles.

#![cfg(feature = "metal")]

use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::{NSString, NSURL};
use objc2_metal::{MTLCommandQueue, MTLCreateSystemDefaultDevice, MTLDevice, MTLLibrary};

/// Owned Metal context used by every ported kernel.
pub struct MetalRuntime {
    pub device: Retained<ProtocolObject<dyn MTLDevice>>,
    pub queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    pub library: Retained<ProtocolObject<dyn MTLLibrary>>,
}

impl MetalRuntime {
    /// Open the system default `MTLDevice` and load the vendored
    /// `default.metallib` produced by `build.rs`.
    pub fn new() -> Result<Self> {
        let raw: *mut ProtocolObject<dyn MTLDevice> = unsafe { MTLCreateSystemDefaultDevice() };
        if raw.is_null() {
            return Err(anyhow!("no default MTLDevice"));
        }
        let device: Retained<ProtocolObject<dyn MTLDevice>> = unsafe { Retained::from_raw(raw) }
            .ok_or_else(|| anyhow!("Retained::from_raw(MTLDevice) returned None"))?;
        let queue = device
            .newCommandQueue()
            .ok_or_else(|| anyhow!("device.newCommandQueue() returned nil"))?;
        let metallib_path = metallib_path_from_env();
        let path_ns = NSString::from_str(
            metallib_path
                .to_str()
                .ok_or_else(|| anyhow!("metallib path is not utf-8"))?,
        );
        let url = unsafe { NSURL::fileURLWithPath(&path_ns) };
        let library = unsafe { device.newLibraryWithURL_error(&url) }
            .map_err(|err| anyhow!("device.newLibraryWithURL failed: {err:?}"))
            .with_context(|| {
                format!(
                    "loading vendored metallib at {}",
                    metallib_path.display()
                )
            })?;
        Ok(Self {
            device,
            queue,
            library,
        })
    }
}

fn metallib_path_from_env() -> PathBuf {
    // Set by build.rs at compile time. Embedding the absolute path is
    // fine for the dev-loop bench: stage 5's accepted-profile work
    // will move the metallib to a versioned per-runtime location and
    // override this via the runtime store.
    PathBuf::from(env!("CTOX_QWEN36_METALLIB_PATH"))
}

// ─── Persistent buffer pool ──────────────────────────────────────────────
//
// Every per-op dispatcher in metal_port::ops::* currently calls
// `newBufferWithBytes_length_options` — that copies the bytes into a
// fresh MTLBuffer per dispatch. For weights this is catastrophic
// (we measured 150 MiB / call ≈ 4 ms of pure alloc in the MoE bench
// dominating a kernel that runs in <500 µs). The Stage-4 layer-block
// driver invokes the same kernels hundreds of times per token, so
// the buffer-alloc cost has to disappear.
//
// `BufferPool` keeps `Retained<MTLBuffer>` handles live for the
// session. Two construction paths:
//
//  - `wrap_bytes`: `newBufferWithBytesNoCopy` over CPU memory the
//    caller owns (e.g. an mmap'd Q4_K weight slab). Zero-copy on
//    shared-storage Apple Silicon, page-aligned slices required.
//  - `alloc_zeroed`: `newBufferWithLength` for transient outputs
//    (KV cache rows, attention scratch, residual buffers). Lives
//    as long as the pool.
//
// Lifetimes: `BufferPool` owns the handles via `Retained` so the
// MTLBuffers stay alive across kernel dispatches. Drop the pool
// at the end of the session and Metal's refcount cleanup runs.

use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;

use objc2_metal::MTLBuffer;

/// Cache of named persistent buffers reused across dispatches.
pub struct BufferPool {
    pub device: Retained<ProtocolObject<dyn MTLDevice>>,
    bufs: HashMap<String, Retained<ProtocolObject<dyn MTLBuffer>>>,
}

impl BufferPool {
    pub fn new(rt: &MetalRuntime) -> Self {
        Self {
            device: rt.device.clone(),
            bufs: HashMap::new(),
        }
    }

    /// Copy an existing CPU byte slice into a persistent Metal
    /// buffer ONCE. After this call, kernels can read from the buffer
    /// for free — no per-dispatch alloc. Use this for weight tensors:
    /// pay the copy at session start, amortise it across thousands
    /// of subsequent matmuls.
    ///
    /// (`newBufferWithBytesNoCopy` would be true zero-copy but
    /// requires the `block2` feature for the deallocator block; we
    /// avoid that dep until the copy cost actually shows up in
    /// session-startup measurements. For 150 MiB / 40 GB/s ≈ 4 ms
    /// startup once vs no per-call cost ever again, copy-once wins
    /// massively.)
    pub fn copy_in(&mut self, key: &str, bytes: &[u8]) -> Result<()> {
        use objc2_metal::MTLResourceOptions;
        let opts = MTLResourceOptions::MTLResourceStorageModeShared;
        let nn = NonNull::new(bytes.as_ptr() as *mut c_void)
            .ok_or_else(|| anyhow::anyhow!("bytes ptr null"))?;
        let buf = unsafe {
            self.device
                .newBufferWithBytes_length_options(nn, bytes.len(), opts)
        }
        .ok_or_else(|| anyhow::anyhow!("newBufferWithBytes returned nil"))?;
        self.bufs.insert(key.to_string(), buf);
        Ok(())
    }

    /// Allocate a zero-initialised persistent buffer (caller writes
    /// into `contents()` later, or kernels write into it).
    pub fn alloc_zeroed(&mut self, key: &str, n_bytes: usize) -> Result<()> {
        use objc2_metal::MTLResourceOptions;
        let opts = MTLResourceOptions::MTLResourceStorageModeShared;
        let buf = self
            .device
            .newBufferWithLength_options(n_bytes, opts)
            .ok_or_else(|| anyhow::anyhow!("newBufferWithLength returned nil"))?;
        self.bufs.insert(key.to_string(), buf);
        Ok(())
    }

    /// Borrow a previously inserted buffer. Returns `None` if the
    /// key has never been inserted.
    pub fn get(&self, key: &str) -> Option<&ProtocolObject<dyn MTLBuffer>> {
        self.bufs.get(key).map(|b| &**b)
    }

    /// Mutable handle for setBuffer_offset_atIndex callers.
    pub fn buf(&self, key: &str) -> Result<&Retained<ProtocolObject<dyn MTLBuffer>>> {
        self.bufs
            .get(key)
            .ok_or_else(|| anyhow::anyhow!("BufferPool: no buffer named `{key}`"))
    }

    /// Number of named buffers held — useful for debugging session size.
    pub fn len(&self) -> usize {
        self.bufs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bufs.is_empty()
    }
}
