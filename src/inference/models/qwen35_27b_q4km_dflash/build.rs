//! Build script for `ctox-qwen35-27b-q4km-dflash`.
//!
//! Links the crate against ggml + ggml-cuda. Two paths:
//!
//!  1. **GGML_LIB_DIR set** — link against pre-built ggml `.so` files in
//!     that directory. This is the dev-box / CI path: the lucebox-built
//!     `build/deps/llama.cpp/ggml/src/` tree has all four libraries
//!     (`libggml-base.so`, `libggml.so`, `libggml-cpu.so`, and
//!     `libggml-cuda.so` in `ggml-cuda/`). We also compile
//!     `vendor/ggml-cuda/f16_convert.cu` so
//!     `dflash27b_launch_{f16,bf16}_to_f32` resolve without the reference's
//!     `libdflash27b.a`.
//!
//!  2. **GGML_LIB_DIR unset** — emit a warning and no link directives.
//!     `cargo check` still passes on a host without the build tree, so the
//!     Rust surface compiles and trips no link step.
//!
//! The crate intentionally does NOT compile the 62 `.cu` files in
//! `vendor/ggml-cuda/` from source. That path is documented in
//! `vendor/llama-cpp.version` and handled by the lucebox reference build;
//! making the crate self-host-compile would pull in ~6K lines of CMake
//! logic (GGML_* CACHE vars, backend dispatch, HIP/ROCm detection, etc.)
//! from llama.cpp's `ggml/src/CMakeLists.txt`. The version pin in
//! `vendor/llama-cpp.version` plus the GGML_LIB_DIR contract is the
//! stable integration point.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=GGML_LIB_DIR");
    println!("cargo:rerun-if-env-changed=NVCC");
    println!("cargo:rerun-if-env-changed=CTOX_CUDA_SM");

    let cuda_feature = env::var("CARGO_FEATURE_CUDA").is_ok();
    let ggml_lib_dir = env::var("GGML_LIB_DIR").ok();

    // PTX stubs must exist before the Rust sources are compiled
    // (include_str! in src/cuda_port/ptx.rs resolves at compile time).
    // On a non-CUDA host we write empty placeholders so the library
    // still compiles; runtime module-load errors take over from
    // there. When the cuda feature is set, the real nvcc compile in
    // `compile_cuda_port_ptx_modules` overwrites these.
    write_ptx_stubs();

    // `cargo check` on a non-CUDA host without GGML_LIB_DIR: just compile
    // the Rust surface and bail out of linker setup.
    if !cuda_feature && ggml_lib_dir.is_none() {
        println!(
            "cargo:warning=ctox-qwen35-27b-q4km-dflash: cuda feature off and GGML_LIB_DIR unset \
             — skipping native compile + link (host-only build). Runtime will fail."
        );
        return;
    }

    if let Some(dir) = ggml_lib_dir {
        link_ggml(&PathBuf::from(dir));
    } else {
        println!(
            "cargo:warning=ctox-qwen35-27b-q4km-dflash: cuda feature on but GGML_LIB_DIR unset. \
             Set GGML_LIB_DIR to the lucebox build tree's \
             `build/deps/llama.cpp/ggml/src/` path. Link step will fail."
        );
    }

    compile_f16_convert();

    // cuda_port bare-metal migration: compile vendored .cu files to PTX
    // for the Rust-side dispatcher port. Each op landed in
    // src/cuda_port/ops/<op>.rs needs the matching kernel source here.
    // List grows as ports land.
    if cuda_feature {
        compile_cuda_port_ptx_modules();
    } else {
        // Without the cuda feature the PTX blobs aren't included, but
        // include_str! still resolves at compile time — write empty
        // stubs so the non-CUDA `cargo check` path compiles.
        write_ptx_stubs();
    }

    // Linux + CUDA needs libstdc++ because nvcc-emitted objects reference
    // C++ runtime symbols (`__cxa_guard_*`, `__gxx_personality_v0`). On
    // macOS + Metal we don't hit this path; the warning above fires first.
    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-lib=dylib=stdc++");
}

/// PTX stems to compile. Each entry gets one `<stem>.ptx` file in
/// `$OUT_DIR`, consumed by the Rust side via `include_str!` in
/// `src/cuda_port/ptx.rs`.
const CUDA_PORT_PTX_MODULES: &[&str] = &[
    "norm", "unary", "scale", "fill", "diag", "binbcast", "tri", "pad", "cumsum", "concat", "cpy",
    "solve_tri", "rope", "softmax", "ssm-conv",
];

fn compile_cuda_port_ptx_modules() {
    for stem in CUDA_PORT_PTX_MODULES {
        let ok = compile_kernel_to_ptx(stem);
        if !ok {
            // Don't fail the build — write an empty stub so include_str!
            // still compiles; runtime module-load will fail with a clear
            // error later. Safer than breaking `cargo check` on a CI host
            // with a stale nvcc.
            write_ptx_stub(stem);
        }
        // Always regenerate the entries module (stubbed empty if no PTX).
        generate_ptx_entries_module(stem);
    }
}

fn write_ptx_stubs() {
    for stem in CUDA_PORT_PTX_MODULES {
        write_ptx_stub(stem);
    }
}

fn write_ptx_stub(stem: &str) {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let ptx = out_dir.join(format!("{stem}.ptx"));
    if !ptx.exists() {
        let content = format!(
            "// PTX stub — cuda feature off or nvcc failed during build.\n\
             // Runtime `cuModuleLoadData` will return an error if this\n\
             // file is actually loaded. Rebuild with `--features=cuda`\n\
             // and a working nvcc on PATH to fill this in.\n"
        );
        let _ = std::fs::write(&ptx, content);
    }
    // Matching empty entries-map so `include!` in Rust still resolves.
    let entries = out_dir.join(format!("{stem}_entries.rs"));
    if !entries.exists() {
        let _ = std::fs::write(
            &entries,
            "// auto-generated PTX-entries stub. No CUDA feature / nvcc failed.\n\
             pub const ENTRIES: &[&[u8]] = &[];\n",
        );
    }
}

/// Parse `.entry <mangled>(…)` lines from a PTX file and emit a Rust
/// module (`$OUT_DIR/<stem>_entries.rs`) that exports the full list
/// of mangled kernel names for that PTX blob:
///
/// ```ignore
/// pub const ENTRIES: &[&[u8]] = &[
///     b"<mangled_with_NUL>\0",
///     …
/// ];
/// ```
///
/// The Rust-side `ops::*` modules pick the right entry at runtime
/// via substring match (the unmangled functor name — e.g. `"op_silu"`,
/// `"rms_norm_f32"` — plus any shape discriminator). This bypasses
/// nvcc's per-translation-unit hash mangling for `static` helper
/// functions, which makes hard-coding the full mangled name
/// non-portable.
fn generate_ptx_entries_module(stem: &str) {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let ptx_path = out_dir.join(format!("{stem}.ptx"));
    let out_path = out_dir.join(format!("{stem}_entries.rs"));

    let ptx = match std::fs::read_to_string(&ptx_path) {
        Ok(s) => s,
        Err(e) => {
            println!(
                "cargo:warning=ctox-qwen35-27b-q4km-dflash: can't read {}: {e} — writing empty entries module",
                ptx_path.display()
            );
            let _ = std::fs::write(
                &out_path,
                "pub const ENTRIES: &[&[u8]] = &[];\n",
            );
            return;
        }
    };

    let mut entries: Vec<String> = Vec::new();
    for line in ptx.lines() {
        let t = line.trim_start();
        let Some(rest) = t.strip_prefix(".entry ") else {
            continue;
        };
        let Some(paren) = rest.find('(') else {
            continue;
        };
        let name = rest[..paren].trim().to_string();
        if !name.is_empty() {
            entries.push(name);
        }
    }

    let mut rendered = String::from(
        "// auto-generated by build.rs from PTX `.entry` names.\n\
         // Do not edit; regenerate by rebuilding the crate.\n\
         pub const ENTRIES: &[&[u8]] = &[\n",
    );
    for e in &entries {
        // Null-terminate for direct use with cuModuleGetFunction.
        rendered.push_str("    b\"");
        for b in e.as_bytes() {
            // PTX entry names are ASCII; escape any `\\`/`"` just in case.
            match b {
                b'\\' => rendered.push_str("\\\\"),
                b'"' => rendered.push_str("\\\""),
                _ => rendered.push(*b as char),
            }
        }
        rendered.push_str("\\0\",\n");
    }
    rendered.push_str("];\n");

    if let Err(e) = std::fs::write(&out_path, rendered) {
        println!(
            "cargo:warning=ctox-qwen35-27b-q4km-dflash: failed to write {}: {e}",
            out_path.display()
        );
    } else {
        println!(
            "cargo:warning=ctox-qwen35-27b-q4km-dflash: PTX entries parsed: {} from {}.ptx",
            entries.len(),
            stem
        );
    }
}

/// Emit `cargo:rustc-link-*` directives for the pre-built ggml `.so` set.
fn link_ggml(base: &PathBuf) {
    println!("cargo:rustc-link-search=native={}", base.display());
    println!("cargo:rustc-link-lib=dylib=ggml-base");
    println!("cargo:rustc-link-lib=dylib=ggml");
    println!("cargo:rustc-link-lib=dylib=ggml-cpu");

    // ggml-cuda lives one level deeper on the lucebox tree.
    let cuda_subdir = base.join("ggml-cuda");
    if cuda_subdir.is_dir() {
        println!("cargo:rustc-link-search=native={}", cuda_subdir.display());
        println!("cargo:rustc-link-lib=dylib=ggml-cuda");
        println!("cargo:rustc-link-lib=dylib=cudart");
    } else {
        println!(
            "cargo:warning=ctox-qwen35-27b-q4km-dflash: expected `ggml-cuda/` subdir under {} \
             but it is missing. Link step will fail if --features=cuda.",
            base.display()
        );
    }
}

/// Compile `vendor/ggml-cuda/f16_convert.cu` into a static archive and
/// emit link directives. Byte-for-byte copy of `lucebox/dflash/src/f16_convert.cu`.
///
/// `NVCC` env var overrides the binary location; `CTOX_CUDA_SM` picks the
/// SM capability (default 86 — matches lucebox's hardcoded `CUDA_ARCHITECTURES "86"`).
fn compile_f16_convert() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let src = manifest.join("vendor/ggml-cuda/f16_convert.cu");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let obj = out_dir.join("f16_convert.o");
    let lib = out_dir.join("libctox_f16_convert.a");

    println!("cargo:rerun-if-changed={}", src.display());

    if !src.exists() {
        println!(
            "cargo:warning=ctox-qwen35-27b-q4km-dflash: vendor/ggml-cuda/f16_convert.cu \
             missing at {} — skipping (host-only build)",
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
                "cargo:warning=ctox-qwen35-27b-q4km-dflash: nvcc failed for f16_convert.cu: \
                 exit {s} — skipping"
            );
            return;
        }
        Err(e) => {
            println!(
                "cargo:warning=ctox-qwen35-27b-q4km-dflash: nvcc not available ({e}) — \
                 skipping f16_convert.cu compile. Fine for `cargo check` on non-CUDA hosts."
            );
            return;
        }
    }

    let ar = env::var("AR").unwrap_or_else(|_| "ar".into());
    let ar_status = Command::new(&ar)
        .args(["rcs"])
        .arg(&lib)
        .arg(&obj)
        .status();
    match ar_status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            println!(
                "cargo:warning=ctox-qwen35-27b-q4km-dflash: ar failed building \
                 libctox_f16_convert.a: exit {s}"
            );
            return;
        }
        Err(e) => {
            println!(
                "cargo:warning=ctox-qwen35-27b-q4km-dflash: ar not available ({e}) — \
                 skipping archive step"
            );
            return;
        }
    }

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=ctox_f16_convert");
}

/// Compile a single vendored `.cu` file to PTX under `$OUT_DIR/<stem>.ptx`.
///
/// Used by the incremental `cuda_port` bare-metal dispatcher migration —
/// each op dispatcher ported to Rust needs the matching kernel source
/// compiled to PTX so the Rust side can load it at runtime via
/// `cuModuleLoadData`. Flags + defines mirror the ones ggml's own CMake
/// build uses for ggml-cuda (inspected from compile_commands.json on
/// the A6000 dev box):
///
/// ```text
/// nvcc -forward-unknown-to-host-compiler -O3 -DNDEBUG -std=c++17
///   -DGGML_CUDA_PEER_MAX_BATCH_SIZE=128 -DGGML_SCHED_MAX_COPIES=4
///   --generate-code=arch=compute_<SM>,code=[compute_<SM>,sm_<SM>]
///   -use_fast_math -extended-lambda
///   -I vendor/ggml-include -I vendor/ggml-cuda
///   --ptx -o $OUT_DIR/<stem>.ptx  vendor/ggml-cuda/<stem>.cu
/// ```
///
/// Returns true on success, false on any failure (a warning is emitted
/// so the caller can decide whether to bail the whole build or degrade
/// gracefully to the FFI path).
#[allow(dead_code)]
fn compile_kernel_to_ptx(stem: &str) -> bool {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let src = manifest.join("vendor/ggml-cuda").join(format!("{stem}.cu"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let ptx = out_dir.join(format!("{stem}.ptx"));

    println!("cargo:rerun-if-changed={}", src.display());
    if !src.exists() {
        println!(
            "cargo:warning=ctox-qwen35-27b-q4km-dflash: vendor/ggml-cuda/{stem}.cu missing — skipping PTX"
        );
        return false;
    }

    let include_cuda = manifest.join("vendor/ggml-cuda");
    let include_ggml = manifest.join("vendor/ggml-include");
    // Some .cu files (`cumsum.cu`) use `#include "ggml-cuda/common.cuh"`
    // — needs `vendor/` on the include path so the sub-path resolves.
    let include_vendor = manifest.join("vendor");
    let nvcc = env::var("NVCC").unwrap_or_else(|_| "nvcc".into());
    let sm = env::var("CTOX_CUDA_SM").unwrap_or_else(|_| "86".into());
    let gencode = format!("--generate-code=arch=compute_{sm},code=[compute_{sm},sm_{sm}]");

    let status = Command::new(&nvcc)
        .args([
            "-forward-unknown-to-host-compiler",
            "-O3",
            "-DNDEBUG",
            "-std=c++17",
            "-DGGML_CUDA_PEER_MAX_BATCH_SIZE=128",
            "-DGGML_SCHED_MAX_COPIES=4",
            "-use_fast_math",
            "-extended-lambda",
        ])
        .arg(&gencode)
        .arg("-I")
        .arg(&include_cuda)
        .arg("-I")
        .arg(&include_ggml)
        .arg("-I")
        .arg(&include_vendor)
        .arg("--ptx")
        .arg("-o")
        .arg(&ptx)
        .arg(&src)
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("cargo:warning=ctox-qwen35-27b-q4km-dflash: PTX compiled: {stem}.ptx");
            true
        }
        Ok(s) => {
            println!("cargo:warning=ctox-qwen35-27b-q4km-dflash: PTX nvcc failed for {stem}.cu: exit {s}");
            false
        }
        Err(e) => {
            println!("cargo:warning=ctox-qwen35-27b-q4km-dflash: PTX nvcc not available ({e}) — skipping {stem}");
            false
        }
    }
}
