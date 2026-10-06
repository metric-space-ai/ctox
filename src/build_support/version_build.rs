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
    for path in git_version_watch_paths(&repo_root) {
        println!("cargo:rerun-if-changed={}", path.display());
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

fn git_version_watch_paths(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    // A linked worktree's .git is a file, not the directory containing HEAD.
    for name in ["HEAD", "refs/tags", "packed-refs"] {
        if let Some(path) = git_metadata_path(root, name).filter(|path| path.exists()) {
            paths.push(path);
        }
    }
    if let Some(head) =
        git_metadata_path(root, "HEAD").and_then(|path| std::fs::read_to_string(path).ok())
    {
        if let Some(reference) = head.trim().strip_prefix("ref: ") {
            if let (Some(mut path), Some(heads)) = (
                git_metadata_path(root, reference),
                git_metadata_path(root, "refs/heads"),
            ) {
                // A packed branch creates a loose ref on its next commit.
                // Watch its nearest existing parent until that ref exists.
                while !path.exists() && path.starts_with(&heads) && path != heads {
                    path.pop();
                }
                if path.exists() {
                    paths.push(path);
                }
            }
        }
    }
    paths.sort();
    paths.dedup();
    paths
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
#[cfg(test)]
mod git_watch_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static FIXTURE: AtomicUsize = AtomicUsize::new(0);

    struct GitFixture {
        base: PathBuf,
        repo: PathBuf,
        linked: PathBuf,
    }

    impl GitFixture {
        fn new() -> Self {
            let base = std::env::temp_dir().join(format!(
                "ctox-version-watch-{}-{}",
                std::process::id(),
                FIXTURE.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&base).unwrap();
            let repo = base.join("repo");
            std::fs::create_dir(&repo).unwrap();
            let fixture = Self {
                linked: base.join("linked"),
                base,
                repo,
            };
            fixture.git(&["init", "-b", "main"]);
            fixture.git(&["commit", "--allow-empty", "-m", "fixture"]);
            fixture.git(&[
                "worktree",
                "add",
                "-b",
                "fixture-branch",
                fixture.linked.to_str().unwrap(),
            ]);
            fixture
        }

        fn git(&self, args: &[&str]) {
            let output = Command::new("git")
                .arg("-C")
                .arg(&self.repo)
                .args([
                    "-c",
                    "commit.gpgsign=false",
                    "-c",
                    "core.hooksPath=/dev/null",
                    "-c",
                    "user.name=Metadata Fixture",
                    "-c",
                    "user.email=metadata-fixture@example.invalid",
                ])
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    impl Drop for GitFixture {
        fn drop(&mut self) {
            if self.linked.exists() {
                let _ = Command::new("git")
                    .arg("-C")
                    .arg(&self.repo)
                    .args(["worktree", "remove"])
                    .arg(&self.linked)
                    .output();
            }
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    #[test]
    fn linked_worktree_watches_real_head_and_active_branch() {
        let fixture = GitFixture::new();
        let git_file = std::fs::read_to_string(fixture.linked.join(".git")).unwrap();
        let directory = PathBuf::from(git_file.trim().strip_prefix("gitdir: ").unwrap());
        let paths = git_version_watch_paths(&fixture.linked);
        assert!(paths.contains(&directory.join("HEAD")));
        assert!(paths.contains(&fixture.repo.join(".git/refs/heads/fixture-branch")));
        assert!(!paths.contains(&fixture.linked.join(".git/HEAD")));
        assert!(!paths.contains(&fixture.repo.join(".git/refs/heads/main")));
        assert!(paths.iter().all(|path| path.exists()));
    }

    #[test]
    fn packed_branch_watches_creation_then_tracks_the_new_loose_ref() {
        let fixture = GitFixture::new();
        fixture.git(&["pack-refs", "--all"]);
        let reference = fixture.repo.join(".git/refs/heads/fixture-branch");
        assert!(!reference.exists());
        let before = git_version_watch_paths(&fixture.linked);
        assert!(before.contains(&fixture.repo.join(".git/packed-refs")));
        assert!(before.contains(&fixture.repo.join(".git/refs/heads")));
        assert!(!before.contains(&reference));
        fixture.git(&[
            "-C",
            fixture.linked.to_str().unwrap(),
            "commit",
            "--allow-empty",
            "-m",
            "advance",
        ]);
        assert!(reference.exists());
        let after = git_version_watch_paths(&fixture.linked);
        assert!(after.contains(&reference));
        assert!(!after.contains(&fixture.repo.join(".git/refs/heads")));
        assert!(after.iter().all(|path| path.exists()));
    }

    #[test]
    fn source_archive_adds_no_missing_git_watch_paths() {
        let base = std::env::temp_dir().join(format!(
            "ctox-version-export-{}-{}",
            std::process::id(),
            FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&base).unwrap();
        let paths = git_version_watch_paths(&base);
        let _ = std::fs::remove_dir(&base);
        assert!(paths.is_empty());
    }
}
