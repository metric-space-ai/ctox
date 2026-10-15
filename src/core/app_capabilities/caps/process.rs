//! `process.run` — bounded tool process (CATALOG Nr. 4). The program is a
//! name from the platform allowlist (typed runtime config key, name → absolute
//! path); the app never names a binary path. The child starts with an EMPTY
//! environment plus exactly the variables passed in `env` (never inherited),
//! inside the app's own workspace directory, with a hard timeout. Runs under
//! the registry's held executor fence.
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
    stdin_ref: Option<String>,
    #[serde(default)]
    env: std::collections::BTreeMap<String, String>,
    timeout_ms: Option<u64>,
}

/// Platform allowlist from the runtime config store (JSON object).
pub fn allowlist(root: &Path) -> std::collections::BTreeMap<String, String> {
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
    let program = programs
        .get(&request.program)
        .filter(|path| Path::new(path).is_absolute())
        .ok_or_else(|| forbidden("program is not allowlisted"))?;
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
        if !env_key(key) || value.len() > 4096 || value.contains('\0') {
            return Err(invalid("$.env"));
        }
    }
    let timeout = request.timeout_ms.unwrap_or(60_000);
    if !(1..=MAX_TIMEOUT_MS).contains(&timeout) {
        return Err(invalid("$.timeout_ms"));
    }
    let stdin = match (request.stdin, request.stdin_ref) {
        (Some(text), None) => text.into_bytes(),
        (None, Some(reference)) => {
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
    let mut command = std::process::Command::new(program);
    command
        .args(&request.args)
        .current_dir(&cwd)
        .env_clear()
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
                let _ = child.kill();
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
