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
    // A linked worktree's .git is a file, not the directory containing HEAD.
    for name in ["HEAD", "refs/tags", "packed-refs"] {
        if let Some(path) = git_metadata_path(&repo_root, name).filter(|path| path.exists()) {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    if let Some(head) =
        git_metadata_path(&repo_root, "HEAD").and_then(|path| std::fs::read_to_string(path).ok())
    {
        if let Some(reference) = head.trim().strip_prefix("ref: ") {
            if let (Some(mut path), Some(heads)) = (
                git_metadata_path(&repo_root, reference),
                git_metadata_path(&repo_root, "refs/heads"),
            ) {
                // A packed branch creates a loose ref on its next commit.
                // Watch its nearest existing parent until that ref exists.
                while !path.exists() && path.starts_with(&heads) && path != heads {
                    path.pop();
                }
                if path.exists() {
                    println!("cargo:rerun-if-changed={}", path.display());
                }
            }
        }
    }
    println!(
        "cargo:rerun-if-changed={}",
        repo_root.join("Cargo.toml").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        repo_root
            .join("src/core/business_os/business_os_schema_contract.json")
            .display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        repo_root
            .join("src/core/business_os/business_os_schema_hashes.json")
            .display()
    );
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

fn git_metadata_path(root: &Path, name: &str) -> Option<PathBuf> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--git-path", name])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let path = PathBuf::from(text);
    Some(if path.is_absolute() {
        path
    } else {
        root.join(path)
    })
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
