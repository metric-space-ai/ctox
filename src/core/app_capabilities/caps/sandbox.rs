//! E20 — operating-system confinement for `process.run` (§16.47/§16.48).
//!
//! Every allowlisted program runs inside the platform sandbox; there is no
//! unconfined production path. A missing or non-functional sandbox is
//! `unavailable` with a reason, never a fallback.
//!
//! - Linux: bubblewrap. All namespaces are unshared (user, mount, pid, net,
//!   ipc, uts, cgroup), so the child has no network but its own loopback.
//!   The root is an empty read-only tmpfs; only the system trees (/usr, /etc,
//!   /opt, …), the entry's program/script/read paths, and the app workspace
//!   are mounted. The workspace is the only writable host path (its `.tmp`
//!   doubles as `/tmp`). The CTOX root, the state root and the secret store
//!   directory are covered by empty read-only tmpfs mounts wherever a system
//!   tree would expose them. Resource limits are set by `prlimit` INSIDE the
//!   sandbox, after the user namespace exists: RLIMIT_NPROC is counted per
//!   user namespace from there, whereas a limit set before bwrap would count
//!   every process of the daemon user.
//! - macOS (development host): `sandbox-exec` with a deny-default profile and
//!   the same read/write/network rules; limits via setrlimit before exec
//!   (macOS counts RLIMIT_NPROC per user, so no process limit there).
//!
//! Domain-restricted egress (§16.48 "Netz nur auf freigegebene Zieldomain(s)")
//! is not built: a call or entry that names target domains is `unavailable`
//! instead of silently running with or without network.
use super::*;
use serde::Deserialize;
use std::path::PathBuf;

pub const SANDBOX_KEY: &str = "business_os.process_sandbox";
#[cfg(target_os = "linux")]
const BWRAP_CANDIDATES: &[&str] = &["/usr/bin/bwrap", "/bin/bwrap", "/usr/local/bin/bwrap"];
#[cfg(target_os = "linux")]
const PRLIMIT_CANDIDATES: &[&str] = &["/usr/bin/prlimit", "/bin/prlimit"];
#[cfg(target_os = "macos")]
const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";
/// Read-only system trees visible inside the Linux sandbox.
#[cfg(target_os = "linux")]
const SYSTEM_TREES: &[&str] = &[
    "/usr", "/bin", "/sbin", "/lib", "/lib32", "/lib64", "/libx32", "/etc", "/opt", "/nix",
];
/// Interpreters that may never run generated code (§16.48 Nr. 3).
const SHELLS: &[&str] = &[
    "sh", "bash", "dash", "zsh", "ksh", "mksh", "ash", "fish", "csh", "tcsh", "busybox",
];

/// Platform sandbox binaries (runtime config `business_os.process_sandbox`);
/// unset entries use the standard system locations.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)] // each platform reads its own binaries
struct Config {
    bwrap: Option<String>,
    prlimit: Option<String>,
    sandbox_exec: Option<String>,
}

/// Per-entry resource limits (platform allowlist). Unset values use defaults.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub cpu_seconds: Option<u64>,
    pub memory_mb: Option<u64>,
    pub processes: Option<u64>,
    pub file_mb: Option<u64>,
    pub open_files: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Effective {
    pub cpu_seconds: u64,
    pub memory_bytes: u64,
    pub processes: u64,
    pub file_bytes: u64,
    pub open_files: u64,
}

impl Limits {
    pub fn effective(&self, timeout_ms: u64) -> Result<Effective, CtoxError> {
        let pick = |value: Option<u64>, default: u64, range: std::ops::RangeInclusive<u64>| {
            let value = value.unwrap_or(default);
            range
                .contains(&value)
                .then_some(value)
                .ok_or_else(|| unavailable("allowlist entry limits are out of range"))
        };
        const MIB: u64 = 1024 * 1024;
        Ok(Effective {
            cpu_seconds: pick(self.cpu_seconds, timeout_ms.div_ceil(1000), 1..=600)?,
            memory_bytes: pick(self.memory_mb, 1024, 16..=16384)? * MIB,
            processes: pick(self.processes, 64, 1..=4096)?,
            file_bytes: pick(self.file_mb, 256, 1..=16384)? * MIB,
            open_files: pick(self.open_files, 256, 16..=4096)?,
        })
    }
}

fn unavailable(reason: &str) -> CtoxError {
    CtoxError::new(
        ErrorCode::Unavailable,
        &format!("process sandbox unavailable: {reason}"),
    )
}

/// §16.48 Nr. 3: generated code runs only through a non-shell interpreter
/// plus a platform host script.
pub fn untrusted_entry_ok(interpreter: Option<&str>, script: Option<&str>) -> bool {
    match (interpreter, script) {
        (Some(interpreter), Some(_)) => Path::new(interpreter)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| !SHELLS.contains(&name)),
        _ => false,
    }
}

/// Everything the confinement needs to know about one run.
pub struct Plan<'a> {
    pub root: &'a Path,
    pub workspace: &'a Path,
    pub cwd: &'a Path,
    pub program: &'a str,
    pub args: Vec<String>,
    /// Program, interpreter, script and entry read paths (absolute).
    pub read_paths: Vec<String>,
    pub limits: Effective,
    pub net_domains: Vec<String>,
}

fn config(root: &Path) -> Result<Config, CtoxError> {
    match crate::inference::runtime_env::get_runtime_env_value(root, SANDBOX_KEY) {
        None => Ok(Config::default()),
        Some(raw) => serde_json::from_str(&raw)
            .map_err(|_| unavailable("sandbox configuration is malformed")),
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn locate(
    configured: Option<&String>,
    candidates: &[&str],
    name: &str,
) -> Result<PathBuf, CtoxError> {
    let found = match configured {
        Some(path) => Some(PathBuf::from(path)).filter(|path| path.is_absolute() && path.is_file()),
        None => candidates
            .iter()
            .map(PathBuf::from)
            .find(|path| path.is_file()),
    };
    found.ok_or_else(|| unavailable(&format!("{name} is not installed")))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
/// State the sandbox must never expose: CTOX root, state root, secret store.
fn hidden(root: &Path) -> Vec<PathBuf> {
    let mut paths = vec![root.to_path_buf(), crate::paths::runtime_dir(root)];
    if let Some(parent) = crate::paths::secrets_db(root).parent() {
        paths.push(parent.to_path_buf());
    }
    let mut out: Vec<PathBuf> = paths
        .into_iter()
        .filter_map(|path| path.canonicalize().ok())
        .collect();
    out.sort();
    out.dedup();
    out
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
/// Canonical read paths; a path that is or contains hidden state, or lies
/// inside the state root, is refused (platform misconfiguration).
fn read_paths(plan: &Plan, hidden: &[PathBuf]) -> Result<Vec<PathBuf>, CtoxError> {
    let state_roots: Vec<PathBuf> = [crate::paths::runtime_dir(plan.root)]
        .into_iter()
        .chain(
            crate::paths::secrets_db(plan.root)
                .parent()
                .map(Path::to_path_buf),
        )
        .filter_map(|path| path.canonicalize().ok())
        .collect();
    let mut out = Vec::new();
    for raw in &plan.read_paths {
        let path = Path::new(raw);
        if !path.is_absolute() {
            return Err(unavailable("allowlist path is not absolute"));
        }
        let path = path
            .canonicalize()
            .map_err(|_| unavailable("allowlist path does not exist"))?;
        if hidden.iter().any(|h| h.starts_with(&path))
            || state_roots.iter().any(|s| path.starts_with(s))
        {
            return Err(unavailable("allowlist path would expose CTOX state"));
        }
        // The configured path may be a symlink; expose its own location too.
        if raw.as_str() != path.to_string_lossy() && Path::new(raw).is_symlink() {
            out.push(PathBuf::from(raw));
        }
        out.push(path);
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// The confined command. The caller sets env (after `env_clear`), stdio and
/// waits; on Unix the child leads its own process group.
pub fn command(plan: &Plan) -> Result<std::process::Command, CtoxError> {
    if !plan.net_domains.is_empty() {
        return Err(unavailable(
            "domain-restricted network is not supported; sandboxed runs have no network",
        ));
    }
    let config = config(plan.root)?;
    platform_command(plan, &config)
}

#[cfg(target_os = "linux")]
fn platform_command(plan: &Plan, config: &Config) -> Result<std::process::Command, CtoxError> {
    let bwrap = locate(
        config.bwrap.as_ref(),
        BWRAP_CANDIDATES,
        "bubblewrap (bwrap)",
    )?;
    let prlimit = locate(config.prlimit.as_ref(), PRLIMIT_CANDIDATES, "prlimit")?;
    probe(&bwrap, &prlimit)?;
    let hidden = hidden(plan.root);
    let reads = read_paths(plan, &hidden)?;
    let tmp = plan.workspace.join(".tmp");
    std::fs::create_dir_all(&tmp).map_err(|_| unavailable("workspace tmp cannot be created"))?;
    let path = |p: &Path| p.to_string_lossy().into_owned();
    let mut args: Vec<String> = [
        "--unshare-all",
        "--die-with-parent",
        "--new-session",
        "--cap-drop",
        "ALL",
    ]
    .map(String::from)
    .to_vec();
    let mut visible = Vec::new();
    for tree in SYSTEM_TREES {
        let tree = Path::new(tree);
        match std::fs::symlink_metadata(tree) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let target = std::fs::read_link(tree).map_err(native_error)?;
                args.extend(["--symlink".into(), path(&target), path(tree)]);
            }
            Ok(meta) if meta.is_dir() => {
                args.extend(["--ro-bind".into(), path(tree), path(tree)]);
                visible.push(tree.canonicalize().map_err(native_error)?);
            }
            _ => {}
        }
    }
    args.extend(["--proc", "/proc", "--dev", "/dev"].map(String::from));
    // Cover state that a system tree would expose (e.g. a root under /opt).
    let covered: Vec<&PathBuf> = hidden
        .iter()
        .filter(|h| visible.iter().any(|tree| h.starts_with(tree)))
        .collect();
    for h in &covered {
        args.extend(["--tmpfs".into(), path(h)]);
    }
    for read in &reads {
        let in_tree = visible.iter().any(|tree| read.starts_with(tree));
        if !in_tree || covered.iter().any(|h| read.starts_with(h)) {
            args.extend(["--ro-bind".into(), path(read), path(read)]);
        }
    }
    args.extend(["--bind".into(), path(&tmp), "/tmp".into()]);
    args.extend(["--bind".into(), path(plan.workspace), path(plan.workspace)]);
    for h in &covered {
        args.extend(["--remount-ro".into(), path(h)]);
    }
    args.extend(["--remount-ro".into(), "/".into()]);
    args.extend(["--chdir".into(), path(plan.cwd), "--".into()]);
    let limits = plan.limits;
    args.push(path(&prlimit));
    for (flag, value) in [
        ("cpu", limits.cpu_seconds),
        ("data", limits.memory_bytes),
        ("fsize", limits.file_bytes),
        ("nofile", limits.open_files),
        ("nproc", limits.processes),
        ("core", 0),
    ] {
        args.push(format!("--{flag}={value}:{value}"));
    }
    args.push("--".into());
    args.push(plan.program.into());
    args.extend(plan.args.iter().cloned());
    let mut command = std::process::Command::new(&bwrap);
    command.args(args);
    Ok(command)
}

/// One cheap confined start per binary pair; success is cached for the
/// process lifetime, failure is re-probed on the next call.
#[cfg(target_os = "linux")]
fn probe(bwrap: &Path, prlimit: &Path) -> Result<(), CtoxError> {
    use std::sync::{Mutex, OnceLock};
    static OK: OnceLock<Mutex<std::collections::BTreeSet<(PathBuf, PathBuf)>>> = OnceLock::new();
    let ok = OK.get_or_init(Default::default);
    let key = (bwrap.to_path_buf(), prlimit.to_path_buf());
    if ok.lock().map(|set| set.contains(&key)).unwrap_or(false) {
        return Ok(());
    }
    let output = std::process::Command::new(bwrap)
        .args(["--unshare-all", "--die-with-parent", "--ro-bind", "/", "/"])
        .args(["--proc", "/proc", "--dev", "/dev", "--"])
        .arg(prlimit)
        .args(["--nproc=16:16", "--", "/bin/true"])
        .env_clear()
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|_| unavailable("bubblewrap cannot be started"))?;
    if !output.status.success() {
        let reason = String::from_utf8_lossy(&output.stderr);
        let reason: String = reason.trim().chars().take(160).collect();
        return Err(unavailable(&format!(
            "bubblewrap cannot create namespaces ({reason})"
        )));
    }
    if let Ok(mut set) = ok.lock() {
        set.insert(key);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn platform_command(plan: &Plan, config: &Config) -> Result<std::process::Command, CtoxError> {
    use std::os::unix::process::CommandExt;
    let sandbox_exec = locate(
        config.sandbox_exec.as_ref(),
        &[SANDBOX_EXEC],
        "sandbox-exec",
    )?;
    let hidden = hidden(plan.root);
    let reads = read_paths(plan, &hidden)?;
    let mut profile = String::from(
        "(version 1)\n(deny default)\n(allow process-fork)\n(allow process-exec)\n\
         (allow signal (target same-sandbox))\n(allow sysctl-read)\n(allow file-read-metadata)\n\
         (allow file-read* (literal \"/\") (subpath \"/usr\") (subpath \"/bin\") (subpath \"/sbin\")\n\
         (subpath \"/System\") (subpath \"/Library\") (subpath \"/private/etc\") (subpath \"/dev\")\n\
         (subpath \"/opt/homebrew\") (subpath \"/private/var/db/timezone\"))\n\
         (allow file-write* (literal \"/dev/null\"))\n",
    );
    let mut params = Vec::new();
    // Later rules win: hide state first, then re-allow reads and the workspace.
    for (index, h) in hidden.iter().enumerate() {
        profile.push_str(&format!(
            "(deny file-read* file-write* (subpath (param \"HIDE{index}\")))\n"
        ));
        params.push((format!("HIDE{index}"), h.clone()));
    }
    for (index, read) in reads.iter().enumerate() {
        profile.push_str(&format!(
            "(allow file-read* (subpath (param \"READ{index}\")))\n"
        ));
        params.push((format!("READ{index}"), read.clone()));
    }
    profile.push_str("(allow file-read* file-write* (subpath (param \"WORKSPACE\")))\n");
    params.push(("WORKSPACE".into(), plan.workspace.to_path_buf()));
    let mut command = std::process::Command::new(sandbox_exec);
    command.arg("-p").arg(profile);
    for (key, value) in params {
        command
            .arg("-D")
            .arg(format!("{key}={}", value.to_string_lossy()));
    }
    command.arg(plan.program).args(&plan.args);
    let limits = plan.limits;
    // SAFETY: setrlimit is async-signal-safe; no allocation after fork.
    unsafe {
        command.pre_exec(move || {
            for (resource, value) in [
                (libc::RLIMIT_CPU, limits.cpu_seconds),
                (libc::RLIMIT_DATA, limits.memory_bytes),
                (libc::RLIMIT_FSIZE, limits.file_bytes),
                (libc::RLIMIT_NOFILE, limits.open_files),
                (libc::RLIMIT_CORE, 0),
            ] {
                let limit = libc::rlimit {
                    rlim_cur: value as libc::rlim_t,
                    rlim_max: value as libc::rlim_t,
                };
                if libc::setrlimit(resource, &limit) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    Ok(command)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn platform_command(_: &Plan, _: &Config) -> Result<std::process::Command, CtoxError> {
    Err(unavailable("no process sandbox on this platform"))
}
