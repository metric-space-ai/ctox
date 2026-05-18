// Origin: CTOX
// License: Apache-2.0
//
// Stage-2 build script.
//
// Without `--features metal`, this is a no-op pin emitter: the crate
// compiles cleanly on any host. With `--features metal`, it compiles
// the vendored MSL kernel source (`vendor/ggml-metal/ggml-metal.metal`)
// into a `default.metallib` shipped under `OUT_DIR` so the Rust
// dispatcher in `src/metal_port/` can `MTLDevice::newLibraryWithURL:`
// it at runtime.
//
// The compiler invocation is exactly `xcrun -sdk macosx metal -c
// <src> -o <air>` followed by `xcrun -sdk macosx metallib <air> -o
// <metallib>`. No CMake, no third-party build helpers, no link
// against `libggml-metal.dylib`. The kernel sources are vendored
// text only; the produced binary depends only on the OS Metal
// framework.

use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=vendor/upstream-config/Qwen3.6-35B-A3B.config.json");
    println!("cargo:rerun-if-changed=vendor/llama-cpp.version");
    println!("cargo:rerun-if-changed=vendor/ggml-metal.version");
    println!("cargo:rerun-if-changed=vendor/ggml-metal/ggml-metal.metal");
    println!("cargo:rerun-if-changed=vendor/ggml-metal/ggml-metal-impl.h");
    println!("cargo:rerun-if-changed=vendor/ggml-metal/ggml-common.h");

    // The metal feature is the only switch that drives MSL compilation.
    // Without it, we ship a Rust-only check surface — useful on CI
    // hosts and during early refactors.
    if std::env::var("CARGO_FEATURE_METAL").is_err() {
        return;
    }

    if !cfg!(target_os = "macos") {
        panic!(
            "the `metal` feature is only supported on macOS \
             (target_os == \"macos\")"
        );
    }

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let out_dir = Path::new(&out_dir);
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let manifest_dir = Path::new(&manifest_dir);

    let kernels_src = manifest_dir.join("vendor/ggml-metal/ggml-metal.metal");
    let include_dir = manifest_dir.join("vendor/ggml-metal");
    let air_path = out_dir.join("ggml-metal.air");
    let metallib_path = out_dir.join("default.metallib");

    eprintln!(
        "ctox-qwen36-35b-a3b-q4km-metal: compiling MSL kernels\n  \
         src      = {}\n  \
         include  = {}\n  \
         air      = {}\n  \
         metallib = {}",
        kernels_src.display(),
        include_dir.display(),
        air_path.display(),
        metallib_path.display(),
    );

    // Step 1: source -> AIR (Apple Intermediate Representation).
    //
    // -DGGML_METAL_HAS_TENSOR enables upstream's `mpp::tensor_ops::matmul2d`
    // path for the matmat kernels (kernel_mul_mm_*) and the flash-attention
    // tensor variants. On M5 the device reports has_tensor=true, so this
    // engages the on-chip Apple matrix unit (the GPU equivalent of AMX/SME
    // — Metal 4 + Apple10 family). Without the macro, the kernels fall back
    // to a simdgroup-only legacy path with different tile constants and
    // shared-memory layout.
    let air = Command::new("xcrun")
        .args([
            "-sdk",
            "macosx",
            "metal",
            "-O3",
            // -std=metal4.0 unlocks `mpp::tensor_ops::matmul2d` and
            // `<metal_tensor>` — the on-chip Apple matrix unit. Our
            // M5 reports MTLGPUFamilyMetal4 in the device init, so
            // this compiles AOT here and runs at full perf there.
            // Older Metal standards (metal3.0–3.1) do not expose
            // mpp::tensor_ops at all.
            "-std=metal4.0",
            "-DGGML_METAL_HAS_TENSOR",
            // Match upstream llama.cpp's flags closely. Without
            // -fno-fast-math, fp32 reduce paths can drift from the CPU
            // reference and fail per-op verifiers.
            "-fno-fast-math",
            "-Wno-unused-function",
            // Header lookup: the .metal file `#include`s
            // ggml-common.h and ggml-metal-impl.h relative to itself.
            "-I",
        ])
        .arg(&include_dir)
        .arg("-c")
        .arg(&kernels_src)
        .arg("-o")
        .arg(&air_path)
        .status()
        .expect("failed to spawn xcrun metal — Xcode CLT not on PATH?");
    if !air.success() {
        panic!("xcrun metal exited with status {air}");
    }

    // Step 2: AIR -> metallib.
    let mlib = Command::new("xcrun")
        .args(["-sdk", "macosx", "metallib"])
        .arg(&air_path)
        .arg("-o")
        .arg(&metallib_path)
        .status()
        .expect("failed to spawn xcrun metallib");
    if !mlib.success() {
        panic!("xcrun metallib exited with status {mlib}");
    }

    // Surface the metallib path to Rust code via a generated env var.
    println!(
        "cargo:rustc-env=CTOX_QWEN36_METALLIB_PATH={}",
        metallib_path.display()
    );
}
