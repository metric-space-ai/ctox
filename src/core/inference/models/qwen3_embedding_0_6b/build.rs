use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=NVCC");
    println!("cargo:rerun-if-env-changed=CTOX_CUDA_SM");
    println!("cargo:rerun-if-env-changed=CTOX_QWEN3_EMBEDDING_BUILD_CUDA");
    println!("cargo:rerun-if-env-changed=CTOX_CUDA_HOME");

    match env::var("CARGO_CFG_TARGET_OS").unwrap_or_default().as_str() {
        "linux" => build_cuda_if_available(),
        "macos" => {
            println!("cargo:rerun-if-changed=vendor/metal/kernels/ctox_qwen3_embedding_glue.metal");
            println!(
                "cargo:warning=ctox-qwen3-embedding-0-6b: Metal shader is vendored; metallib build is not wired yet"
            );
        }
        _ => {}
    }
}

fn build_cuda_if_available() {
    let enabled = env::var("CTOX_QWEN3_EMBEDDING_BUILD_CUDA")
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no"
            )
        })
        .unwrap_or(true);
    if !enabled {
        println!("cargo:warning=ctox-qwen3-embedding-0-6b: CUDA build disabled by CTOX_QWEN3_EMBEDDING_BUILD_CUDA");
        return;
    }

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let src = manifest.join("vendor/cuda/kernels/ctox_qwen3_embedding_glue.cu");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let obj = out_dir.join("ctox_qwen3_embedding_glue.o");
    let lib = out_dir.join("libctox_qwen3_embedding_glue.a");

    println!("cargo:rerun-if-changed={}", src.display());
    if !src.is_file() {
        println!(
            "cargo:warning=ctox-qwen3-embedding-0-6b: missing CUDA glue source {}",
            src.display()
        );
        return;
    }

    let nvcc = env::var("NVCC").unwrap_or_else(|_| "nvcc".to_string());
    let sm = env::var("CTOX_CUDA_SM").unwrap_or_else(|_| "86".to_string());
    let status = Command::new(&nvcc)
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

    match status {
        Ok(status) if status.success() => {}
        Ok(status) => {
            println!(
                "cargo:warning=ctox-qwen3-embedding-0-6b: nvcc failed for CUDA glue: exit {status}"
            );
            return;
        }
        Err(err) => {
            println!(
                "cargo:warning=ctox-qwen3-embedding-0-6b: nvcc unavailable ({err}); skipping CUDA glue archive"
            );
            return;
        }
    }

    let ar = env::var("AR").unwrap_or_else(|_| "ar".to_string());
    let status = Command::new(&ar).args(["rcs"]).arg(&lib).arg(&obj).status();
    match status {
        Ok(status) if status.success() => {}
        Ok(status) => {
            println!(
                "cargo:warning=ctox-qwen3-embedding-0-6b: ar failed for CUDA glue archive: exit {status}"
            );
            return;
        }
        Err(err) => {
            println!(
                "cargo:warning=ctox-qwen3-embedding-0-6b: ar unavailable ({err}); skipping CUDA glue archive"
            );
            return;
        }
    }

    println!(
        "cargo:rustc-env=CTOX_QWEN3_EMBEDDING_CUDA_ARCHIVE={}",
        lib.display()
    );
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=ctox_qwen3_embedding_glue");
    if let Some(cuda_lib_dir) = cuda_lib_dir() {
        println!("cargo:rustc-link-search=native={}", cuda_lib_dir.display());
    }
    println!("cargo:rustc-link-lib=dylib=cudart");
    println!("cargo:rustc-link-lib=dylib=stdc++");
}

fn cuda_lib_dir() -> Option<PathBuf> {
    env::var("CTOX_CUDA_HOME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .map(|path| path.join("lib64"))
        .filter(|path| path.is_dir())
        .or_else(|| {
            ["/usr/local/cuda/lib64", "/usr/lib/x86_64-linux-gnu"]
                .into_iter()
                .map(PathBuf::from)
                .find(|path| path.join("libcudart.so").exists())
        })
}
