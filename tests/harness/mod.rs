use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

pub struct TestRoot {
    root: PathBuf,
}

#[allow(dead_code)]
impl TestRoot {
    pub fn new(label: &str) -> Self {
        Self::new_at(&unique_test_root(label))
    }

    /// Create a test root at an explicit path. Use for tests that need SHORT
    /// root paths (e.g. unix-socket paths under `<root>/runtime/sockets/`
    /// cap at ~100 bytes).
    pub fn new_at(root: &std::path::Path) -> Self {
        let root = root.to_path_buf();
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
        // `CTOX_ROOT` is only honored when the target passes
        // `looks_like_ctox_root` (main.rs: Cargo.toml + entrypoint +
        // creation ledger). Without these markers the binary silently falls
        // back to CWD-ancestor resolution — which is the REAL repository
        // root when tests run from the package directory. Any test that
        // relies on root-level isolation (service, runtime env, secrets)
        // would then read and write the developer's actual runtime state.
        fs::create_dir_all(root.join("src/core")).expect("failed to create src marker");
        fs::create_dir_all(root.join("contracts/history")).expect("failed to create ledger dir");
        fs::write(root.join("Cargo.toml"), "# ctox test root marker\n")
            .expect("failed to write Cargo.toml marker");
        fs::write(root.join("src/core/main.rs"), "// ctox test root marker\n")
            .expect("failed to write entrypoint marker");
        fs::write(
            root.join("contracts/history/creation-ledger.md"),
            "# ctox test root marker\n",
        )
        .expect("failed to write ledger marker");
        Self { root }
    }

    pub fn run(&self, args: &[&str]) -> CmdOutput {
        let output = Command::new(env!("CARGO_BIN_EXE_ctox"))
            .args(args)
            .env("CTOX_ROOT", &self.root)
            .output()
            .expect("failed to execute ctox binary");
        CmdOutput { output }
    }

    pub fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    /// Spawn a long-running ctox child (e.g. `service --foreground`) against
    /// this root. The caller owns the child and must kill/wait it.
    pub fn spawn(&self, args: &[&str]) -> std::process::Child {
        Command::new(env!("CARGO_BIN_EXE_ctox"))
            .args(args)
            .env("CTOX_ROOT", &self.root)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("failed to spawn ctox binary")
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[allow(dead_code)]
pub struct CmdOutput {
    output: std::process::Output,
}

#[allow(dead_code)]
impl CmdOutput {
    pub fn success(&self) -> &Self {
        assert!(
            self.output.status.success(),
            "command failed\nstatus: {:?}\nstdout:\n{}\nstderr:\n{}",
            self.output.status.code(),
            self.stdout(),
            self.stderr()
        );
        self
    }

    pub fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.output.stdout)
            .trim()
            .to_string()
    }

    pub fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.output.stderr)
            .trim()
            .to_string()
    }

    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.output.stdout).expect("stdout was not valid json")
    }
}

#[allow(dead_code)]
impl TestRoot {
    pub fn db_path(&self) -> PathBuf {
        self.path("runtime/ctox.sqlite3")
    }
}

fn unique_test_root(label: &str) -> PathBuf {
    let mut slug = label
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .to_ascii_lowercase();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    let slug = slug.trim_matches('-');
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("ctox-test-{}-{}-{}", slug, stamp, seq))
}
