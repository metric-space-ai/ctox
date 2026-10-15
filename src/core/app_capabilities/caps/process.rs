//! `process.run` — bounded tool process (CATALOG Nr. 4). The program is a
//! name from the platform allowlist (typed runtime config key, name → absolute
//! path); the app never names a binary path. The child starts with an EMPTY
//! environment plus exactly the variables passed in `env` (never inherited),
//! inside the app's own workspace directory, with a hard timeout. Runs under
//! the registry's held executor fence and always inside the E20 OS sandbox
//! (`sandbox.rs`): no network, only the workspace writable, CTOX state and
//! secrets invisible, resource limits. Without a working sandbox the call is
//! `unavailable`; there is no unconfined path.
use super::*;
use serde::Deserialize;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

pub const ALLOWLIST_KEY: &str = "business_os.process_allowlist";
pub const MAX_TIMEOUT_MS: u64 = 600_000;
pub const MAX_OUTPUT_BYTES: usize = 256 * 1024;
const MAX_STDIN_BYTES: usize = 1024 * 1024;
/// Loader/runtime injection variables are never accepted.
const DENIED_ENV: &[&str] = &[
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "LD_AUDIT",
    "NODE_OPTIONS",
    "PYTHONSTARTUP",
    "PYTHONPATH",
    "BASH_ENV",
    "ENV",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Run {
    program: String,
    #[serde(default)]
    args: Vec<String>,
    cwd_ref: Option<String>,
    stdin: Option<String>,
    stdin_ref: Option<FileRef>,
    #[serde(default)]
    env: std::collections::BTreeMap<String, String>,
    timeout_ms: Option<u64>,
    /// Target domains for egress (§16.48); only when the entry permits them.
    #[serde(default)]
    net_domains: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileRef {
    #[serde(rename = "ref")]
    reference: String,
}
/// One platform allowlist entry (§16.47): a program, or an interpreter plus
/// script path, and the fixed base environment (PATH, CTOX_BIN, LANG, …).
/// The script always comes from here, never from the app or stdin.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum AllowEntry {
    Path(String),
    Spec(ProgramSpec),
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramSpec {
    pub program: Option<String>,
    pub interpreter: Option<String>,
    pub script: Option<String>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    /// §16.48: interpreter + host script that runs generated code (stdin).
    #[serde(default)]
    pub untrusted_code: bool,
    /// Extra read-only paths the program needs inside the sandbox.
    #[serde(default)]
    pub read_paths: Vec<String>,
    #[serde(default)]
    pub limits: super::sandbox::Limits,
    /// Egress domains approved by the platform for this entry.
    #[serde(default)]
    pub net_domains: Vec<String>,
    /// Whether a call may name its own target domains.
    #[serde(default)]
    pub net_from_call: bool,
}
/// A resolved allowlist entry; absolute paths only.
struct Resolved {
    program: String,
    prefix: Vec<String>,
    env: std::collections::BTreeMap<String, String>,
    read_paths: Vec<String>,
    limits: super::sandbox::Limits,
    net_domains: Vec<String>,
    net_from_call: bool,
}
fn resolve_entry(entry: &AllowEntry) -> Option<Resolved> {
    let absolute = |path: &String| Path::new(path).is_absolute();
    match entry {
        AllowEntry::Path(path) => absolute(path).then(|| Resolved {
            program: path.clone(),
            prefix: Vec::new(),
            env: Default::default(),
            read_paths: vec![path.clone()],
            limits: Default::default(),
            net_domains: Vec::new(),
            net_from_call: false,
        }),
        AllowEntry::Spec(spec) => {
            if spec.untrusted_code
                && !super::sandbox::untrusted_entry_ok(
                    spec.interpreter.as_deref(),
                    spec.script.as_deref(),
                )
            {
                return None;
            }
            let (program, prefix) = match (&spec.program, &spec.interpreter, &spec.script) {
                (Some(program), None, None) => (program.clone(), Vec::new()),
                (None, Some(interpreter), Some(script)) if absolute(script) => {
                    (interpreter.clone(), vec![script.clone()])
                }
                _ => return None,
            };
            let read_paths = [program.clone()]
                .into_iter()
                .chain(prefix.iter().cloned())
                .chain(spec.read_paths.iter().cloned())
                .collect();
            absolute(&program).then(|| Resolved {
                program,
                prefix,
                env: spec.env.clone(),
                read_paths,
                limits: spec.limits.clone(),
                net_domains: spec.net_domains.clone(),
                net_from_call: spec.net_from_call,
            })
        }
    }
}
/// A bare DNS host name (no scheme, port, path or wildcard).
fn domain(name: &str) -> bool {
    name.len() <= 253
        && name.contains('.')
        && name.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}
/// Platform allowlist from the runtime config store (JSON object).
pub fn allowlist(root: &Path) -> std::collections::BTreeMap<String, AllowEntry> {
    crate::inference::runtime_env::get_runtime_env_value(root, ALLOWLIST_KEY)
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}
pub fn workspace(root: &Path, ctx: &CapCallContext) -> std::path::PathBuf {
    root.join("runtime/app-workspaces").join(&ctx.app_id)
}
fn env_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 64
        && key.as_bytes()[0].is_ascii_uppercase()
        && key
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        && !DENIED_ENV.contains(&key)
        && !key.starts_with("DYLD_")
        && !crate::secrets::is_secret_key(key)
}
/// A relative path inside the workspace: no root, no `..`, no NUL.
fn relative(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 512
        && !path.starts_with('/')
        && !path.contains('\0')
        && !path.contains('\\')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != ".." && part != ".")
}

pub(super) fn execute(
    root: &Path,
    files: Result<&dyn files::FileStore, CtoxError>,
    ctx: &CapCallContext,
    args: &Value,
) -> Result<Value, CtoxError> {
    let request: Run = decode(args)?;
    let programs = allowlist(root);
    let entry = programs
        .get(&request.program)
        .and_then(resolve_entry)
        .ok_or_else(|| forbidden("program is not allowlisted"))?;
    let base_env = &entry.env;
    if request.net_domains.len() > 16 || !request.net_domains.iter().all(|d| domain(d)) {
        return Err(invalid("$.net_domains"));
    }
    if !request.net_domains.is_empty() && !entry.net_from_call {
        return Err(forbidden("this program may not name network targets"));
    }
    if request.args.len() > 128
        || request
            .args
            .iter()
            .any(|a| a.len() > 8192 || a.contains('\0'))
    {
        return Err(invalid("$.args"));
    }
    if request.env.len() > 32 {
        return Err(invalid("$.env"));
    }
    for (key, value) in &request.env {
        // The app only adds variables; the platform base environment wins.
        if !env_key(key) || base_env.contains_key(key) || value.len() > 4096 || value.contains('\0')
        {
            return Err(invalid("$.env"));
        }
    }
    let timeout = request.timeout_ms.unwrap_or(60_000);
    if !(1..=MAX_TIMEOUT_MS).contains(&timeout) {
        return Err(invalid("$.timeout_ms"));
    }
    let stdin = match (request.stdin, request.stdin_ref) {
        (Some(text), None) => text.into_bytes(),
        (None, Some(FileRef { reference })) => {
            crate::app_capabilities::protocol::identifier(&reference, "$.stdin_ref")?;
            files?
                .get(&ctx.app_id, &reference)?
                .filter(|(meta, bytes)| {
                    meta.owner_app == ctx.app_id && sha256_hex(bytes) == meta.reference
                })
                .map(|(_, bytes)| bytes)
                .ok_or_else(|| CtoxError::new(ErrorCode::NotFound, "stdin file not found"))?
        }
        (None, None) => Vec::new(),
        _ => return Err(invalid("$.stdin")),
    };
    if stdin.len() > MAX_STDIN_BYTES {
        return Err(invalid("$.stdin"));
    }
    let base = workspace(root, ctx);
    std::fs::create_dir_all(&base).map_err(native_error)?;
    let base = base.canonicalize().map_err(native_error)?;
    let cwd = match &request.cwd_ref {
        None => base.clone(),
        Some(path) if relative(path) => {
            let cwd = base.join(path);
            std::fs::create_dir_all(&cwd).map_err(native_error)?;
            let cwd = cwd.canonicalize().map_err(native_error)?;
            // A symlink inside the workspace must not lead out of it.
            if !cwd.starts_with(&base) {
                return Err(forbidden("cwd leaves the app workspace"));
            }
            cwd
        }
        Some(_) => return Err(invalid("$.cwd_ref")),
    };
    let limits = entry.limits.effective(timeout)?;
    let mut net_domains = entry.net_domains.clone();
    net_domains.extend(request.net_domains.iter().cloned());
    let plan = super::sandbox::Plan {
        root,
        workspace: &base,
        cwd: &cwd,
        program: &entry.program,
        args: entry.prefix.iter().chain(&request.args).cloned().collect(),
        read_paths: entry.read_paths.clone(),
        limits,
        net_domains,
    };
    let mut command = super::sandbox::command(&plan)?;
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    command
        .current_dir(&cwd)
        .env_clear()
        .envs(base_env)
        .envs(&request.env)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let started = Instant::now();
    let mut child = command.spawn().map_err(native_error)?;
    let child_stdin = child.stdin.take();
    let writer = std::thread::spawn(move || {
        if let Some(mut pipe) = child_stdin {
            let _ = pipe.write_all(&stdin);
        }
    });
    let capture = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut out = Vec::new();
            if let Some(pipe) = pipe {
                let _ = pipe.take(MAX_OUTPUT_BYTES as u64 + 1).read_to_end(&mut out);
            }
            out
        })
    };
    let stdout = capture(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = capture(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let deadline = Duration::from_millis(timeout);
    let (status, timed_out) = loop {
        match child.try_wait().map_err(native_error)? {
            Some(status) => break (Some(status), false),
            None if started.elapsed() >= deadline => {
                kill_tree(&mut child);
                let _ = child.wait();
                break (None, true);
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    };
    let _ = writer.join();
    let render = |bytes: Vec<u8>| {
        let truncated = bytes.len() > MAX_OUTPUT_BYTES;
        let shown = &bytes[..bytes.len().min(MAX_OUTPUT_BYTES)];
        (String::from_utf8_lossy(shown).into_owned(), truncated)
    };
    let (stdout, stdout_truncated) = render(stdout.join().unwrap_or_default());
    let (stderr, stderr_truncated) = render(stderr.join().unwrap_or_default());
    Ok(json!({
        "exit": status.and_then(|s| s.code()),
        "timed_out": timed_out,
        "duration_ms": started.elapsed().as_millis() as u64,
        "stdout": stdout, "stdout_truncated": stdout_truncated,
        "stderr": stderr, "stderr_truncated": stderr_truncated,
    }))
}

/// Kill the confined process group before it is reaped (the sandbox launcher
/// leads it; on Linux the pid namespace takes every descendant with it).
fn kill_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        // SAFETY: plain kill(2) on the group this module created for the child.
        unsafe {
            libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
        }
    }
    let _ = child.kill();
}
