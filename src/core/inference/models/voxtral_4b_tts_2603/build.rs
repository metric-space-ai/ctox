use std::{env, path::PathBuf, process::Command};
fn run(cmd: &mut Command) {
    let status = cmd.status().expect("native Voxtral compiler unavailable");
    assert!(
        status.success(),
        "native Voxtral compilation failed: {status}"
    );
}
fn main() {
    println!("cargo:rustc-check-cfg=cfg(voxtral_native)");
    println!("cargo:rustc-check-cfg=cfg(voxtral_cuda)");
    println!("cargo:rerun-if-changed=vendor/voxtral-tts.c");
    println!("cargo:rerun-if-env-changed=NVCC");
    println!("cargo:rerun-if-env-changed=CTOX_CUDA_SM");
    println!("cargo:rerun-if-env-changed=CTOX_VOXTRAL_TTS_BUILD_CUDA");
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if os != "linux" && os != "macos" {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let src =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("vendor/voxtral-tts.c");
    let nvcc = env::var("NVCC").unwrap_or_else(|_| "nvcc".into());
    let requested = env::var_os("CARGO_FEATURE_CUDA").is_some();
    let disabled = matches!(
        env::var("CTOX_VOXTRAL_TTS_BUILD_CUDA").as_deref(),
        Ok("0" | "false" | "no")
    );
    let cuda = requested
        && os == "linux"
        && !disabled
        && Command::new(&nvcc)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success());
    assert!(
        !requested || cuda,
        "CUDA feature requested but nvcc unavailable or disabled"
    );
    let mut objects = Vec::new();
    for name in [
        "voxtral_tts",
        "voxtral_tts_safetensors",
        "voxtral_tts_kernels",
        "voxtral_tts_llm",
        "voxtral_tts_acoustic",
        "voxtral_tts_codec",
        "voxtral_tts_voice",
        "voxtral_tts_wav",
        "voxtral_tts_tokenizer",
        "ctox_bridge",
    ] {
        let obj = out.join(format!("{name}.o"));
        let mut cc = Command::new(env::var("CC").unwrap_or_else(|_| "cc".into()));
        cc.args(["-O3", "-std=c11", "-D_GNU_SOURCE", "-fPIC", "-c"]);
        if cuda {
            cc.arg("-DUSE_CUDA");
        }
        if os == "macos" {
            cc.arg("-DUSE_BLAS");
        }
        cc.arg(src.join(format!("{name}.c"))).arg("-o").arg(&obj);
        run(&mut cc);
        objects.push(obj);
    }
    if cuda {
        let obj = out.join("voxtral_tts_cuda.o");
        run(Command::new(nvcc)
            .args(["-O3", "-DUSE_CUDA", "-Xcompiler", "-fPIC", "-arch"])
            .arg(format!(
                "sm_{}",
                env::var("CTOX_CUDA_SM").unwrap_or_else(|_| "86".into())
            ))
            .arg("-c")
            .arg(src.join("voxtral_tts_cuda.cu"))
            .arg("-o")
            .arg(&obj));
        objects.push(obj);
        println!("cargo:rustc-cfg=voxtral_cuda");
        let cuda_home = env::var("CTOX_CUDA_HOME").unwrap_or_else(|_| "/usr/local/cuda".into());
        println!("cargo:rustc-link-search=native={cuda_home}/lib64");
        for lib in ["cublas", "cudart", "stdc++"] {
            println!("cargo:rustc-link-lib={lib}");
        }
    }
    run(Command::new(env::var("AR").unwrap_or_else(|_| "ar".into()))
        .arg("rcs")
        .arg(out.join("libvoxtral_native.a"))
        .args(objects));
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=voxtral_native");
    println!("cargo:rustc-link-lib=m");
    if os == "macos" {
        println!("cargo:rustc-link-lib=framework=Accelerate");
    }
    println!("cargo:rustc-cfg=voxtral_native");
}
