use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(ctox_qwen35_08b_has_metallib)");
    println!("cargo:rerun-if-changed=vendor/metal/shaders/qwen35_08b");
    println!("cargo:rerun-if-changed=vendor/mps/mps_sidecar.mm");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "macos" {
        build_macos_metal();
        build_macos_mps_sidecar();
    }
}

fn build_macos_metal() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let shader_dir = manifest.join("vendor/metal/shaders/qwen35_08b");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let shader_files = collect_metal_shaders(&shader_dir);

    if shader_files.is_empty() {
        println!(
            "cargo:warning=ctox-qwen35-08b-metal-probe: no .metal shaders under {}",
            shader_dir.display()
        );
        return;
    }

    let mut air_files = Vec::with_capacity(shader_files.len());
    for src in &shader_files {
        println!("cargo:rerun-if-changed={}", src.display());
        let rel = src.strip_prefix(&manifest).unwrap_or(src);
        let stem = rel
            .with_extension("")
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "_");
        let air = out_dir.join(format!("{stem}.air"));
        let output = Command::new("xcrun")
            .args(["-sdk", "macosx", "metal", "-c", "-O3", "-o"])
            .arg(&air)
            .arg(src)
            .output();
        match output {
            Ok(out) if out.status.success() => air_files.push(air),
            Ok(out) => {
                for line in String::from_utf8_lossy(&out.stderr).lines() {
                    println!("cargo:warning=ctox-qwen35-08b-metal-probe: xcrun metal: {line}");
                }
                println!(
                    "cargo:warning=ctox-qwen35-08b-metal-probe: xcrun metal failed for {}",
                    src.display()
                );
                return;
            }
            Err(err) => {
                println!(
                    "cargo:warning=ctox-qwen35-08b-metal-probe: xcrun metal unavailable: {err}"
                );
                return;
            }
        }
    }

    let metallib = out_dir.join("ctox_qwen35_08b_metal_probe.metallib");
    let status = Command::new("xcrun")
        .args(["-sdk", "macosx", "metallib", "-o"])
        .arg(&metallib)
        .args(&air_files)
        .status();
    match status {
        Ok(status) if status.success() => {
            println!(
                "cargo:rustc-env=CTOX_QWEN35_08B_METALLIB={}",
                metallib.display()
            );
            println!("cargo:rustc-cfg=ctox_qwen35_08b_has_metallib");
        }
        Ok(status) => {
            println!("cargo:warning=ctox-qwen35-08b-metal-probe: xcrun metallib failed: {status}");
        }
        Err(err) => {
            println!(
                "cargo:warning=ctox-qwen35-08b-metal-probe: xcrun metallib unavailable: {err}"
            );
        }
    }
}

fn collect_metal_shaders(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(collect_metal_shaders(&path));
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("metal") {
            out.push(path);
        }
    }
    out.sort();
    out
}

fn build_macos_mps_sidecar() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let src = manifest.join("vendor/mps/mps_sidecar.mm");
    if !src.is_file() {
        println!(
            "cargo:warning=ctox-qwen35-08b-metal-probe: missing {}",
            src.display()
        );
        return;
    }
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let obj = out_dir.join("mps_sidecar.o");
    let lib = out_dir.join("libctox_qwen35_mps_sidecar.a");

    let clang = Command::new("xcrun")
        .args([
            "-sdk",
            "macosx",
            "clang++",
            "-std=c++17",
            "-fobjc-arc",
            "-fmodules",
            "-fcxx-modules",
            "-O3",
            "-c",
        ])
        .arg(&src)
        .arg("-o")
        .arg(&obj)
        .status();
    match clang {
        Ok(status) if status.success() => {}
        Ok(status) => {
            println!(
                "cargo:warning=ctox-qwen35-08b-metal-probe: clang++ failed for {}: {status}",
                src.display()
            );
            return;
        }
        Err(err) => {
            println!("cargo:warning=ctox-qwen35-08b-metal-probe: clang++ unavailable: {err}");
            return;
        }
    }

    let ar = Command::new("xcrun")
        .args(["-sdk", "macosx", "ar", "rcs"])
        .arg(&lib)
        .arg(&obj)
        .status();
    match ar {
        Ok(status) if status.success() => {
            println!("cargo:rustc-link-search=native={}", out_dir.display());
            println!("cargo:rustc-link-lib=static=ctox_qwen35_mps_sidecar");
            println!("cargo:rustc-link-lib=framework=Foundation");
            println!("cargo:rustc-link-lib=framework=Metal");
            println!("cargo:rustc-link-lib=framework=MetalPerformanceShaders");
            println!("cargo:rustc-link-lib=c++");
        }
        Ok(status) => {
            println!("cargo:warning=ctox-qwen35-08b-metal-probe: ar failed: {status}");
        }
        Err(err) => {
            println!("cargo:warning=ctox-qwen35-08b-metal-probe: ar unavailable: {err}");
        }
    }
}
