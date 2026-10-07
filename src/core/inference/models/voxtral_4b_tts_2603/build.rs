use std::{env, path::PathBuf, process::Command};
fn resolve_executable(name: &str) -> PathBuf {
    let path = PathBuf::from(name);
    let path = if path.components().count() > 1 {
        path
    } else {
        env::split_paths(&env::var_os("PATH").unwrap_or_default())
            .map(|dir| dir.join(name))
            .find(|candidate| candidate.is_file())
            .expect("selected nvcc is not on PATH")
    };
    path.canonicalize().expect("cannot resolve selected nvcc")
}
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
    for key in ["NVCC", "CC", "AR", "CTOX_CUDA_HOME"] {
        println!("cargo:rerun-if-env-changed={key}");
    }
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
    let toolkit = if cuda {
        let compiler = resolve_executable(&nvcc);
        let home = compiler
            .parent()
            .and_then(|bin| bin.parent())
            .expect("nvcc must live in toolkit/bin")
            .to_path_buf();
        if let Some(configured) = env::var_os("CTOX_CUDA_HOME") {
            assert_eq!(
                PathBuf::from(configured)
                    .canonicalize()
                    .expect("invalid CTOX_CUDA_HOME"),
                home,
                "CTOX_CUDA_HOME must match the selected nvcc toolkit"
            );
        }
        assert!(
            home.join("include/cuda_runtime.h").is_file(),
            "selected nvcc toolkit headers missing"
        );
        Some(home)
    } else {
        None
    };
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
            cc.arg("-DUSE_CUDA")
                .arg("-I")
                .arg(toolkit.as_ref().unwrap().join("include"));
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
        run(Command::new(resolve_executable(&nvcc))
            .arg("-I")
            .arg(toolkit.as_ref().unwrap().join("include"))
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
        let cuda_home = toolkit.as_ref().unwrap();
        println!(
            "cargo:rustc-link-search=native={}/lib64",
            cuda_home.display()
        );
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
