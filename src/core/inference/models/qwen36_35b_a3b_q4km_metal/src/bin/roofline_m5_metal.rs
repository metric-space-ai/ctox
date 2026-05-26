// Origin: CTOX
// License: AGPL-3.0-only

//! Measured roofline probe for the M5 Apple Silicon target.
//!
//! Reports **sustained** stream bandwidth via `MTLBlitCommandEncoder
//! copyFromBuffer`, which is the GPU's hardware-optimized DRAM-to-DRAM
//! copy path on unified memory. This is the right ceiling for the
//! Q4_K_M decode hot path — that path is bandwidth-bound (per
//! BASELINE_GGML.md §6: 53 GB/s effective at 31.5 tok/s, 35 % of
//! advertised 150 GB/s peak).
//!
//! Skill: "Capture hardware facts before choosing quantization,
//! SIMD/matrix APIs, storage modes, or kernel layouts."
//! (method-playbook §4 + §1).
//!
//! Why blit not a custom kernel: CLAUDE.md forbids hand-authored
//! kernels in this crate. `MTLBlitCommandEncoder` is an Apple
//! framework primitive, not a kernel — using it does not violate
//! "vendored kernels only".
//!
//! Matrix/tensor-API throughput probe is deferred to the
//! `mul_mat_q4_k_m` port, where we will time the vendored
//! upstream kernel directly under representative shapes.

#![cfg(feature = "metal")]

use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLBlitCommandEncoder, MTLBuffer, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue,
    MTLCreateSystemDefaultDevice, MTLDevice, MTLResourceOptions,
};

/// One blit pair: src → dst copy, then dst → src copy. Measures both
/// directions equally so any asymmetry between read and write paths
/// shows up as variance, not as a biased mean.
struct StreamPair {
    src: Retained<ProtocolObject<dyn MTLBuffer>>,
    dst: Retained<ProtocolObject<dyn MTLBuffer>>,
    bytes: usize,
}

fn make_buffer(
    device: &ProtocolObject<dyn MTLDevice>,
    bytes: usize,
    storage: MTLResourceOptions,
) -> Result<Retained<ProtocolObject<dyn MTLBuffer>>> {
    device
        .newBufferWithLength_options(bytes, storage)
        .ok_or_else(|| anyhow!("newBufferWithLength({bytes}) returned nil"))
}

/// Touch the buffer once to make sure it's actually backed and warm.
/// On unified memory with `StorageModeShared`, a fresh allocation is
/// page-faulted lazily; without this, the first blit pays the page-in
/// cost and reports artificially low BW.
fn warm_buffer(buf: &ProtocolObject<dyn MTLBuffer>, bytes: usize) {
    unsafe {
        let p = buf.contents().as_ptr() as *mut u8;
        // Stride 4 KiB: writes one byte per page → faults each page in.
        let mut off = 0usize;
        while off < bytes {
            std::ptr::write_volatile(p.add(off), (off & 0xff) as u8);
            off += 4096;
        }
    }
}

fn run_one_blit_pair(
    queue: &ProtocolObject<dyn MTLCommandQueue>,
    pair: &StreamPair,
) -> Result<f64> {
    let cmd = queue
        .commandBuffer()
        .ok_or_else(|| anyhow!("commandBuffer() returned nil"))?;
    let blit = cmd
        .blitCommandEncoder()
        .ok_or_else(|| anyhow!("blitCommandEncoder() returned nil"))?;
    unsafe {
        blit.copyFromBuffer_sourceOffset_toBuffer_destinationOffset_size(
            &pair.src,
            0,
            &pair.dst,
            0,
            pair.bytes,
        );
    }
    blit.endEncoding();

    let t0 = Instant::now();
    cmd.commit();
    unsafe { cmd.waitUntilCompleted() };
    let dt = t0.elapsed().as_secs_f64();
    Ok(dt)
}

fn measure_stream_bw(
    device: &ProtocolObject<dyn MTLDevice>,
    queue: &ProtocolObject<dyn MTLCommandQueue>,
    bytes: usize,
    storage: MTLResourceOptions,
    storage_label: &str,
    iters: usize,
    warmup: usize,
) -> Result<()> {
    let src = make_buffer(device, bytes, storage)?;
    let dst = make_buffer(device, bytes, storage)?;
    if matches!(storage, MTLResourceOptions::MTLResourceStorageModeShared) {
        warm_buffer(&src, bytes);
        warm_buffer(&dst, bytes);
    }
    let pair = StreamPair {
        src,
        dst,
        bytes,
    };

    for _ in 0..warmup {
        let _ = run_one_blit_pair(queue, &pair)?;
    }

    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let dt = run_one_blit_pair(queue, &pair)?;
        // Bytes moved per blit: src→dst is `bytes` of read + `bytes`
        // of write. Most BW reporting conventions count the *traffic*
        // (read + write = 2 × bytes); we report both numbers so the
        // reader can pick the one matching their convention.
        samples.push(dt);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = samples[iters / 2];
    let p95 = samples[(iters * 95) / 100];
    let min = samples[0];
    let max = samples[iters - 1];

    let read_only_gbs = (bytes as f64 / 1e9) / median;
    let traffic_gbs = (2.0 * bytes as f64 / 1e9) / median;

    println!(
        "stream  {:>7} {:>26}  median={:7.2} ms  p95={:7.2} ms  min={:7.2} ms  max={:7.2} ms  → {:6.1} GB/s read  ({:6.1} GB/s read+write traffic)",
        format_bytes(bytes),
        storage_label,
        median * 1e3,
        p95 * 1e3,
        min * 1e3,
        max * 1e3,
        read_only_gbs,
        traffic_gbs,
    );
    Ok(())
}

fn format_bytes(b: usize) -> String {
    if b >= 1 << 30 {
        format!("{:.1} GiB", b as f64 / (1u64 << 30) as f64)
    } else if b >= 1 << 20 {
        format!("{:.0} MiB", b as f64 / (1u64 << 20) as f64)
    } else {
        format!("{} B", b)
    }
}

fn open_device() -> Result<(
    Retained<ProtocolObject<dyn MTLDevice>>,
    Retained<ProtocolObject<dyn MTLCommandQueue>>,
)> {
    let raw: *mut ProtocolObject<dyn MTLDevice> = unsafe { MTLCreateSystemDefaultDevice() };
    if raw.is_null() {
        return Err(anyhow!("no default MTLDevice"));
    }
    let device: Retained<ProtocolObject<dyn MTLDevice>> = unsafe { Retained::from_raw(raw) }
        .ok_or_else(|| anyhow!("Retained::from_raw returned None"))?;
    let queue = device
        .newCommandQueue()
        .ok_or_else(|| anyhow!("newCommandQueue returned nil"))?;
    Ok((device, queue))
}

fn print_header(device: &ProtocolObject<dyn MTLDevice>) {
    let name = device.name();
    let max_buffer = device.maxBufferLength();
    let working_set = device.recommendedMaxWorkingSetSize();
    let unified = device.hasUnifiedMemory();
    println!("# qwen36-35b-a3b-q4km-metal-roofline");
    println!("device                : {}", name);
    println!("hasUnifiedMemory      : {}", unified);
    println!("recommendedMaxWorking : {} ({})", working_set, format_bytes(working_set as usize));
    println!("maxBufferLength       : {} ({})", max_buffer, format_bytes(max_buffer));
    println!();
}

fn main() -> Result<()> {
    let (device, queue) = open_device().context("open MTLDevice")?;
    print_header(&device);

    // We sweep buffer sizes that bracket the working-set sizes of
    // interest:
    //   16  MiB → fits in M5 SLC, hot-cache regime
    //  256  MiB → spills SLC, exercises DRAM
    //   1   GiB → comfortably DRAM-bound
    //   4   GiB → near the GPU working-set ceiling
    //
    // Reported median + p95 over 8 iterations, 2 warmup. Each
    // iteration is ONE blit pair; we count read-only and
    // read+write traffic separately.
    let sizes = [
        16 * 1024 * 1024usize,
        256 * 1024 * 1024,
        1usize << 30,
        4usize << 30,
    ];
    let storages: &[(MTLResourceOptions, &str)] = &[
        (MTLResourceOptions::MTLResourceStorageModeShared, "Shared(unified)"),
        (MTLResourceOptions::MTLResourceStorageModePrivate, "Private(GPU-local)"),
    ];

    println!("{:7} {:>7} {:>26}  {:>11}  {:>9}  {:>9}  {:>9}  {:>14}  {:>26}",
        "phase", "size", "storage",
        "median", "p95", "min", "max",
        "read GB/s", "read+write traffic GB/s");
    for (storage, label) in storages {
        for &b in &sizes {
            // Skip 4 GiB on Private if it exceeds maxBufferLength.
            if b as u64 > device.maxBufferLength() as u64 {
                continue;
            }
            measure_stream_bw(&device, &queue, b, *storage, label, 8, 2)?;
        }
    }

    println!();
    println!("# context");
    println!(
        "# Decode bandwidth math (per BASELINE_GGML §6, pure-Metal numbers):\n\
         #   bytes/token  ≈ 1.69 GB    (3.0 B active params × 0.5625 B/w Q4_K_M)\n\
         #   observed     = 31.5 tok/s\n\
         #   effective    = 53 GB/s\n\
         # If the measured peak above is X GB/s, current decode utilization\n\
         # = 53 / X. Stretch decode ≈ 0.75 × X / 1.69 tok/s."
    );
    Ok(())
}
