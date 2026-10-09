// Origin: CTOX
// License: AGPL-3.0-only
//! Runs the vendored `@workjet/slide-engine` validator, the same TypeScript schema,
//! edit and canvas logic that Workjet renders with. One bounded Node process per
//! call: no daemon environment, no network, no files besides its own script.
use anyhow::{ensure, Context};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const BUNDLE: &str = include_str!("slide-engine/slide-engine-validator.mjs");
const MAX_REQUEST_BYTES: usize = 24 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(20);

fn bundle_digest() -> String {
    format!("{:x}", Sha256::digest(BUNDLE.as_bytes()))
}

/// Writes the embedded bundle once per content hash; a stale or foreign file is replaced.
fn script(root: &Path) -> anyhow::Result<PathBuf> {
    let digest = bundle_digest();
    let dir = crate::paths::runtime_dir(root).join("slide-engine");
    let path = dir.join(format!("validator-{}.mjs", &digest[..16]));
    if let Ok(existing) = std::fs::read(&path) {
        if format!("{:x}", Sha256::digest(&existing)) == digest {
            return Ok(path);
        }
    }
    std::fs::create_dir_all(&dir)?;
    static STAGING: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let staging = dir.join(format!(
        ".validator-{}.{}.{}.tmp",
        &digest[..16],
        std::process::id(),
        STAGING.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::write(&staging, BUNDLE)?;
    std::fs::rename(&staging, &path)?;
    Ok(path)
}

fn node_major(candidate: &Path) -> Option<u32> {
    let mut child = Command::new(candidate)
        .arg("--version")
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < Duration::from_secs(5) => {
                std::thread::sleep(Duration::from_millis(10))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut text = String::new();
    child.stdout.take()?.read_to_string(&mut text).ok()?;
    text.trim()
        .trim_start_matches('v')
        .split('.')
        .next()?
        .parse()
        .ok()
}

/// First Node.js 20+ from PATH, then the usual system locations. Older Node
/// versions cannot run the bundled engine and are skipped. Only a successful
/// lookup is remembered, so installing Node later needs no daemon restart.
pub(in crate::business_os) fn node() -> anyhow::Result<PathBuf> {
    static RESOLVED: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    if let Some(found) = RESOLVED.get() {
        return Ok(found.clone());
    }
    let name = if cfg!(windows) { "node.exe" } else { "node" };
    let mut candidates: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join(name))
                .collect()
        })
        .unwrap_or_default();
    candidates.extend(
        [
            "/opt/homebrew/bin/node",
            "/usr/local/bin/node",
            "/usr/bin/node",
        ]
        .iter()
        .map(PathBuf::from),
    );
    let found = candidates
        .into_iter()
        .filter(|candidate| candidate.is_file())
        .find(|candidate| node_major(candidate).is_some_and(|major| major >= 20))
        .context("presentation validation needs Node.js 20 or newer on the CTOX host")?;
    Ok(RESOLVED.get_or_init(|| found).clone())
}

/// At most two validator processes run at once; further callers wait their turn.
struct Slot;
static SLOTS: std::sync::Mutex<usize> = std::sync::Mutex::new(0);
static SLOT_FREED: std::sync::Condvar = std::sync::Condvar::new();
const MAX_CONCURRENT: usize = 2;

impl Slot {
    fn acquire() -> anyhow::Result<Self> {
        let deadline = Instant::now() + TIMEOUT;
        let mut busy = SLOTS
            .lock()
            .map_err(|_| anyhow::anyhow!("validator slots poisoned"))?;
        while *busy >= MAX_CONCURRENT {
            let left = deadline.saturating_duration_since(Instant::now());
            anyhow::ensure!(
                !left.is_zero(),
                "presentation validator is busy; retry shortly"
            );
            busy = SLOT_FREED
                .wait_timeout(busy, left)
                .map_err(|_| anyhow::anyhow!("validator slots poisoned"))?
                .0;
        }
        *busy += 1;
        Ok(Slot)
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        if let Ok(mut busy) = SLOTS.lock() {
            *busy = busy.saturating_sub(1);
        }
        SLOT_FREED.notify_one();
    }
}

/// Sends one request to the validator and returns its JSON answer. `ok:false`
/// answers are returned as values; only transport, timeout and protocol errors fail.
pub(in crate::business_os) fn run(root: &Path, request: &Value) -> anyhow::Result<Value> {
    let input = serde_json::to_vec(request)?;
    ensure!(
        input.len() <= MAX_REQUEST_BYTES,
        "presentation validator request exceeds {MAX_REQUEST_BYTES} bytes"
    );
    let script = script(root)?;
    let node = node()?;
    let _slot = Slot::acquire()?;
    let mut child = Command::new(node)
        .arg(&script)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("start presentation validator")?;
    let mut stdin = child.stdin.take().context("validator stdin")?;
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let mut stdout = child.stdout.take().context("validator stdout")?;
    let reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let read = (&mut stdout)
            .take(MAX_RESPONSE_BYTES as u64 + 1)
            .read_to_end(&mut buffer);
        read.map(|_| buffer)
    });
    let mut stderr = child.stderr.take().context("validator stderr")?;
    let errors = std::thread::spawn(move || {
        let mut buffer = String::new();
        let _ = (&mut stderr).take(16 * 1024).read_to_string(&mut buffer);
        buffer
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!(
                "presentation validator timed out after {}s",
                TIMEOUT.as_secs()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let _ = writer.join();
    let output = reader
        .join()
        .map_err(|_| anyhow::anyhow!("validator output reader panicked"))??;
    let message = errors.join().unwrap_or_default();
    ensure!(
        output.len() <= MAX_RESPONSE_BYTES,
        "presentation validator answer exceeds {MAX_RESPONSE_BYTES} bytes"
    );
    ensure!(
        status.success(),
        "presentation validator rejected the request: {}",
        message.trim()
    );
    let answer: Value =
        serde_json::from_slice(&output).context("presentation validator answer is not JSON")?;
    ensure!(
        answer.get("ok").and_then(Value::as_bool).is_some(),
        "presentation validator answer has no ok flag"
    );
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    pub(in crate::business_os) fn node_available() -> bool {
        node().is_ok()
    }

    fn deck() -> Value {
        serde_json::from_str(include_str!("slide-engine/jour-fixe-deck.json")).unwrap()
    }

    #[test]
    fn validator_accepts_the_shared_jour_fixe_deck() {
        if !node_available() {
            eprintln!("SKIP: node not available");
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let answer = run(root.path(), &json!({"op":"validate","document":deck()})).unwrap();
        assert_eq!(answer["ok"], true, "{answer}");
        let outline = run(root.path(), &json!({"op":"outline","document":deck()})).unwrap();
        assert_eq!(outline["ok"], true, "{outline}");
        assert!(outline["slides"].as_array().unwrap().len() >= 4);
    }

    #[test]
    fn validator_reports_repairable_issues_instead_of_failing() {
        if !node_available() {
            eprintln!("SKIP: node not available");
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let mut broken = deck();
        broken["slides"][0]["sourceRefs"] = json!([]);
        let answer = run(root.path(), &json!({"op":"validate","document":broken})).unwrap();
        assert_eq!(answer["ok"], false);
        let issues = answer["issues"].as_array().unwrap();
        assert!(issues.iter().any(|issue| issue["repairHint"].is_string()));
    }

    #[test]
    fn validator_rejects_unknown_operations() {
        if !node_available() {
            eprintln!("SKIP: node not available");
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let error = run(root.path(), &json!({"op":"nope"})).unwrap_err();
        assert!(error.to_string().contains("rejected"), "{error}");
    }

    #[test]
    fn cached_script_is_rewritten_when_tampered() {
        let root = tempfile::tempdir().unwrap();
        let path = script(root.path()).unwrap();
        std::fs::write(&path, "tampered").unwrap();
        let again = script(root.path()).unwrap();
        assert_eq!(path, again);
        assert_eq!(std::fs::read_to_string(again).unwrap(), BUNDLE);
    }
}
