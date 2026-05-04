use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=vendor/reference/voxtral.cpp.commit");
    println!("cargo:rerun-if-env-changed=GGML_LIB_DIR");
    println!("cargo:rerun-if-env-changed=CTOX_VOXTRAL_BUILD_GGML");

    if let Ok(dir) = env::var("GGML_LIB_DIR") {
        link_ggml(&PathBuf::from(dir));
        return;
    }

    if env::var("CTOX_VOXTRAL_BUILD_GGML").as_deref() == Ok("0") {
        println!(
            "cargo:warning=ctox-voxtral-mini-4b-realtime-2602: GGML_LIB_DIR unset; \
             vendored ggml build disabled by CTOX_VOXTRAL_BUILD_GGML=0"
        );
    } else {
        build_vendored_ggml();
    }
}

fn build_vendored_ggml() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let src = manifest_dir.join("vendor/ggml");
    let build = out_dir.join("ggml-build");

    let mut configure = Command::new("cmake");
    configure
        .arg("-S")
        .arg(&src)
        .arg("-B")
        .arg(&build)
        .arg("-DGGML_CPU=ON")
        .arg("-DGGML_NATIVE=ON")
        .arg("-DGGML_BUILD_TESTS=OFF")
        .arg("-DGGML_BUILD_EXAMPLES=OFF")
        .arg("-DBUILD_SHARED_LIBS=OFF");

    #[cfg(target_os = "macos")]
    configure.arg("-DGGML_METAL=ON").arg("-DGGML_BLAS=ON");
    #[cfg(target_os = "linux")]
    configure.arg("-DGGML_BLAS=ON");

    assert!(configure.status().expect("run cmake configure").success());
    assert!(Command::new("cmake")
        .arg("--build")
        .arg(&build)
        .arg("--config")
        .arg("Release")
        .arg("-j")
        .status()
        .expect("run cmake build")
        .success());

    link_ggml(&build.join("src"));
}

fn link_ggml(base: &PathBuf) {
    println!("cargo:rustc-link-search=native={}", base.display());
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-lib=c++");
    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-lib=stdc++");
    println!("cargo:rustc-link-lib=static=ggml");
    println!("cargo:rustc-link-lib=static=ggml-base");
    println!("cargo:rustc-link-lib=static=ggml-cpu");

    let blas = base.join("ggml-blas");
    if blas.is_dir() {
        println!("cargo:rustc-link-search=native={}", blas.display());
        println!("cargo:rustc-link-lib=static=ggml-blas");
        #[cfg(target_os = "linux")]
        println!("cargo:rustc-link-lib=dylib=blas");
    }

    let metal = base.join("ggml-metal");
    if metal.is_dir() {
        println!("cargo:rustc-link-search=native={}", metal.display());
        println!("cargo:rustc-link-lib=static=ggml-metal");
        #[cfg(target_os = "macos")]
        println!("cargo:rustc-link-lib=framework=Metal");
        #[cfg(target_os = "macos")]
        println!("cargo:rustc-link-lib=framework=Foundation");
        #[cfg(target_os = "macos")]
        println!("cargo:rustc-link-lib=framework=Accelerate");
    }
}
