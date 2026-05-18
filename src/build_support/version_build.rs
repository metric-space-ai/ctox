use std::path::{Path, PathBuf};
use std::process::Command;

pub fn emit() {
    let repo_root = repo_root();
    let version = std::env::var("CTOX_BUILD_VERSION")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| git_describe(&repo_root))
        .or_else(|| cargo_manifest_version(&repo_root))
        .unwrap_or_else(|| "0.0.0-dev".to_string());

    println!("cargo:rustc-env=CTOX_BUILD_VERSION={version}");
    println!("cargo:rerun-if-env-changed=CTOX_BUILD_VERSION");
    println!("cargo:rerun-if-changed={}", repo_root.join(".git/HEAD").display());
    println!("cargo:rerun-if-changed={}", repo_root.join("Cargo.toml").display());
}

fn repo_root() -> PathBuf {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()));
    manifest_dir
        .ancestors()
        .find(|path| path.join("Cargo.toml").exists() && path.join("src").exists())
        .map(Path::to_path_buf)
        .unwrap_or(manifest_dir)
}

fn git_describe(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("describe")
        .arg("--tags")
        .arg("--dirty")
        .arg("--always")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if version.is_empty() {
        None
    } else {
        Some(version)
    }
}

fn cargo_manifest_version(root: &Path) -> Option<String> {
    let manifest_path = root.join("Cargo.toml");
    let manifest = std::fs::read_to_string(manifest_path).ok()?;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("version = ") {
            return Some(rest.trim_matches('"').to_string());
        }
    }
    None
}
