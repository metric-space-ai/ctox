use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn emit() {
    println!("cargo:rerun-if-env-changed=CTOX_BUILD_VERSION");

    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR missing"));
    emit_git_rerun_hints(&manifest_dir);

    let version = env::var("CTOX_BUILD_VERSION")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| git_describe_version(&manifest_dir))
        .unwrap_or_else(|| env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION missing"));

    println!("cargo:rustc-env=CTOX_BUILD_VERSION={version}");
}

fn emit_git_rerun_hints(manifest_dir: &Path) {
    let Some(git_dir) = resolve_git_dir(manifest_dir) else {
        return;
    };

    println!("cargo:rerun-if-changed={}", git_dir.join("HEAD").display());
    println!(
        "cargo:rerun-if-changed={}",
        git_dir.join("packed-refs").display()
    );
    println!("cargo:rerun-if-changed={}", git_dir.join("refs").display());
}

fn resolve_git_dir(manifest_dir: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .arg("-C")
        .arg(manifest_dir)
        .arg("rev-parse")
        .arg("--git-dir")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if raw.is_empty() {
        return None;
    }

    let path = PathBuf::from(raw);
    Some(if path.is_absolute() {
        path
    } else {
        manifest_dir.join(path)
    })
}

fn git_describe_version(manifest_dir: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(manifest_dir)
        .arg("describe")
        .arg("--tags")
        .arg("--dirty")
        .arg("--match")
        .arg("v[0-9]*")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if raw.is_empty() {
        return None;
    }

    Some(raw.strip_prefix('v').unwrap_or(raw.as_str()).to_string())
}
