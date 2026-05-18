//! Build script for `ctox-qwen35-35b-a3b-dflash`.
//!
//! Dispatches on `cfg(target_os = ...)`:
//!
//!   * **Linux**: compiles the owned CUDA glue kernels under
//!     `vendor/cuda/kernels/`. The 35B-A3B CUDA inference driver is
//!     still a fail-fast Rust stub until the full target/draft/verify
//!     graph is ported onto those kernels.
//!   * **macOS**: compiles every `.metal` shader under
//!     `vendor/metal/shaders/` into `.air` via `xcrun metal`, then
//!     links them into a single `libctox_qwen35_35b_a3b_dflash.metallib`
//!     next to the crate binary via `xcrun metallib`. At runtime the
//!     Rust code loads the metallib through `objc2-metal`'s
//!     `MTLDevice::newLibraryWithURL_error:`.
//!   * **Anything else**: warns and emits nothing.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=NVCC");
    println!("cargo:rerun-if-env-changed=CTOX_CUDA_SM");
    println!("cargo:rerun-if-env-changed=CTOX_METAL_MIN_OS");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    match target_os.as_str() {
        "linux" => build_linux_cuda(),
        "macos" => build_macos_metal(),
        other => {
            println!(
                "cargo:warning=ctox-qwen35-35b-a3b-dflash: unsupported target_os `{other}` — \
                 crate compiles an empty Rust surface and will fail at runtime."
            );
        }
    }
}

// ───────────────────────────────────────────────────────────────────
// Linux + CUDA
// ───────────────────────────────────────────────────────────────────

fn build_linux_cuda() {
    compile_cuda_glue();
    println!(
        "cargo:warning=ctox-qwen35-35b-a3b-dflash: Linux/CUDA full inference is still under port; \
         compiling only the owned CTOX glue kernels if nvcc is available."
    );
}

fn compile_cuda_glue() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let src = manifest.join("vendor/cuda/kernels/ctox_qwen35_35b_glue.cu");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let obj = out_dir.join("ctox_qwen35_35b_glue.o");
    let lib = out_dir.join("libctox_qwen35_35b_glue.a");

    println!("cargo:rerun-if-changed={}", src.display());
    if !src.exists() {
        println!(
            "cargo:warning=ctox-qwen35-35b-a3b-dflash: missing CUDA glue source {}",
            src.display()
        );
        return;
    }

    let nvcc = env::var("NVCC").unwrap_or_else(|_| "nvcc".into());
    let sm = env::var("CTOX_CUDA_SM").unwrap_or_else(|_| "86".into());
    let nvcc_status = Command::new(&nvcc)
        .args([
            "--compile",
            "-arch",
            &format!("sm_{sm}"),
            "-std=c++17",
            "-O3",
            "-Xcompiler",
            "-fPIC",
            "-o",
        ])
        .arg(&obj)
        .arg(&src)
        .status();

    match nvcc_status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            println!(
                "cargo:warning=ctox-qwen35-35b-a3b-dflash: nvcc failed for CUDA glue: exit {s}"
            );
            return;
        }
        Err(e) => {
            println!(
                "cargo:warning=ctox-qwen35-35b-a3b-dflash: nvcc unavailable ({e}); \
                 skipping CUDA glue archive for cargo check."
            );
            return;
        }
    }

    let ar = env::var("AR").unwrap_or_else(|_| "ar".into());
    let ar_status = Command::new(&ar).args(["rcs"]).arg(&lib).arg(&obj).status();
    match ar_status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            println!(
                "cargo:warning=ctox-qwen35-35b-a3b-dflash: ar failed for CUDA glue archive: exit {s}"
            );
            return;
        }
        Err(e) => {
            println!(
                "cargo:warning=ctox-qwen35-35b-a3b-dflash: ar unavailable ({e}); \
                 skipping CUDA glue archive."
            );
            return;
        }
    }

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=ctox_qwen35_35b_glue");
    println!("cargo:rustc-link-lib=dylib=cudart");
    println!("cargo:rustc-link-lib=dylib=stdc++");
}

// ───────────────────────────────────────────────────────────────────
// macOS + Metal
// ───────────────────────────────────────────────────────────────────

fn build_macos_metal() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let dflash_dir = manifest.join("vendor/metal/shaders/dflash");
    let ggml_dir = manifest.join("vendor/metal/shaders/ggml");
    let mlx_root = manifest.join("vendor/mlx");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));

    println!("cargo:rerun-if-changed={}", dflash_dir.display());
    println!("cargo:rerun-if-changed={}", ggml_dir.display());
    println!("cargo:rerun-if-changed={}", mlx_root.display());

    let mut shader_files = collect_metal_shaders(&dflash_dir);
    shader_files.extend(collect_metal_shaders(&ggml_dir));
    shader_files.extend(collect_metal_shaders(&mlx_root));
    if shader_files.is_empty() {
        println!(
            "cargo:warning=ctox-qwen35-35b-a3b-dflash: no .metal shaders under vendor roots — \
             skeleton build. Runtime will fail until shaders are vendored.",
        );
        return;
    }

    // Compile each *.metal → *.air. Metal's frontend takes `-std=foo`
    // glued, not space-separated. Default language-standard bumped to
    // Metal 3.1 (macOS 14+) — matches what `mx.fast.metal_kernel`
    // compiles against in MLX 0.31+.
    let std_flag = env::var("CTOX_METAL_STD").unwrap_or_default();
    let min_os = env::var("CTOX_METAL_MIN_OS").unwrap_or_else(|_| "26.0".into());
    let mut air_files: Vec<PathBuf> = Vec::with_capacity(shader_files.len());

    // NOTE: dflash shader dims (Dk/Dv/Hk/Hv/D/V/M_FIXED) are declared
    // as Metal function_constants in `vendor/metal/shaders/dflash/common.h`
    // — matches MLX's `mx.fast.metal_kernel(template=[...])` path.
    // The Rust dispatcher resolves these at pipeline-build time via
    // `Device::pipeline_with_constants` + `cv_set_int16`.

    for src in &shader_files {
        let rel = src.strip_prefix(&manifest).unwrap_or(src);
        let munged = rel
            .with_extension("")
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "_");
        let air = out_dir.join(format!("{munged}.air"));
        println!("cargo:rerun-if-changed={}", src.display());

        let mut cmd = Command::new("xcrun");
        cmd.args(["-sdk", "macosx", "metal", "-c"])
            .arg(format!("-I{}", mlx_root.display()))
            .arg(format!("-I{}", ggml_dir.display()))
            .arg(format!("-I{}", dflash_dir.display()))
            // Enable bf16 template instantiations in ggml-metal.metal.
            // ref: vendor/metal/shaders/ggml/ggml-metal.metal:34-37
            .args([
                "-DGGML_METAL_HAS_BF16",
                "-Wall",
                "-Wextra",
                "-fno-fast-math",
                "-Wno-c++17-extensions",
                "-Wno-c++20-extensions",
                &format!("-mmacosx-version-min={min_os}"),
                "-O3",
                "-o",
            ]);
        if !std_flag.trim().is_empty() {
            cmd.arg(&std_flag);
        }
        cmd.arg(&air).arg(src);
        let output = cmd.output();

        match output {
            Ok(o) if o.status.success() => air_files.push(air),
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                for line in stderr.lines() {
                    println!("cargo:warning=ctox-qwen35-35b-a3b-dflash: xcrun metal: {line}");
                }
                println!(
                    "cargo:warning=ctox-qwen35-35b-a3b-dflash: xcrun metal failed for {} \
                     (exit {:?}) — aborting metallib link.",
                    src.display(),
                    o.status
                );
                return;
            }
            Err(e) => {
                println!(
                    "cargo:warning=ctox-qwen35-35b-a3b-dflash: xcrun metal unavailable ({e}) — \
                     skipping. `cargo check` will still pass, runtime will not."
                );
                return;
            }
        }
    }

    // Link all .air → one .metallib. The Rust side loads this at runtime.
    let metallib = out_dir.join("ctox_qwen35_35b_a3b_dflash.metallib");
    let status = Command::new("xcrun")
        .args(["-sdk", "macosx", "metallib", "-o"])
        .arg(&metallib)
        .args(&air_files)
        .status();

    match status {
        Ok(s) if s.success() => {
            // Expose the metallib path to the Rust side at build time
            // and emit a cfg so ffi.rs can conditionally `include_bytes!`.
            println!(
                "cargo:rustc-env=CTOX_QWEN35_35B_METALLIB={}",
                metallib.display()
            );
            println!("cargo:rustc-cfg=ctox_has_metallib");
            println!("cargo:rustc-check-cfg=cfg(ctox_has_metallib)");
        }
        Ok(s) => {
            println!("cargo:warning=ctox-qwen35-35b-a3b-dflash: xcrun metallib failed (exit {s})");
            println!("cargo:rustc-check-cfg=cfg(ctox_has_metallib)");
        }
        Err(e) => {
            println!("cargo:warning=ctox-qwen35-35b-a3b-dflash: xcrun metallib unavailable ({e})");
            println!("cargo:rustc-check-cfg=cfg(ctox_has_metallib)");
        }
    }
}

/// Recursively enumerate every `*.metal` file under `root`. Order is
/// sorted by path so the metallib build is deterministic.
fn collect_metal_shaders(root: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(collect_metal_shaders(&path));
        } else if path.extension().and_then(|e| e.to_str()) == Some("metal") {
            out.push(path);
        }
    }
    out.sort();
    out
}
