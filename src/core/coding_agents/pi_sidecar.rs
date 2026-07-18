//! P2: native owner of the pi-code coding sidecar
//! (`src/core/coding_agents/pi-sidecar`). Spawns the LocalTransport daemon and
//! drives one bounded turn over a Unix socket, then reaps it.
//!
//! This is the transport client the higher-level owner uses: it projects a
//! module's app source into a `CtoxTurnRequest.files` snapshot, runs one bounded
//! turn, and reads back the `CtoxTurnResponse` snapshot to record as P0 commits.
//! The sidecar is a bounded leaf executor — a fresh daemon per turn, killed on
//! drop; it never shares the daemon's process authority with the CTOX daemon.
use anyhow::Context;
use serde_json::Value;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Path to the built sidecar bundle relative to the repo root.
pub fn sidecar_dist_path(repo_root: &Path) -> PathBuf {
    repo_root.join("src/core/coding_agents/pi-sidecar/dist/ctox-pi-sidecar.mjs")
}

/// A spawned sidecar daemon listening on a Unix socket. Killed + cleaned on drop
/// so a turn can never leak a live agent process.
struct SidecarDaemon {
    child: Child,
    socket_path: PathBuf,
}

impl Drop for SidecarDaemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

fn spawn_sidecar(dist: &Path, socket_path: &Path, faux: bool) -> anyhow::Result<SidecarDaemon> {
    anyhow::ensure!(
        dist.exists(),
        "pi-sidecar bundle is not built: {} (run `npm run build` in pi-sidecar)",
        dist.display()
    );
    let mut command = Command::new("node");
    command
        .arg(dist)
        .arg(socket_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if faux {
        command.env("CTOX_PI_SIDECAR_FAUX", "1");
    }
    let child = command
        .spawn()
        .context("spawn pi-sidecar daemon (is `node` on PATH?)")?;
    Ok(SidecarDaemon {
        child,
        socket_path: socket_path.to_path_buf(),
    })
}

fn connect_with_retry(socket_path: &Path, timeout: Duration) -> anyhow::Result<UnixStream> {
    let deadline = Instant::now() + timeout;
    loop {
        match UnixStream::connect(socket_path) {
            Ok(stream) => return Ok(stream),
            Err(_) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => {
                return Err(error).context("connect to pi-sidecar socket");
            }
        }
    }
}

fn read_line(stream: &mut UnixStream) -> anyhow::Result<Vec<u8>> {
    let mut buffer = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let read = stream.read(&mut byte).context("read turn response")?;
        if read == 0 || byte[0] == b'\n' {
            break;
        }
        buffer.push(byte[0]);
    }
    Ok(buffer)
}

/// Run one bounded turn through a freshly spawned sidecar daemon: send `request`
/// (a `CtoxTurnRequest` JSON), return the `CtoxTurnResponse` JSON. `faux` runs
/// the sidecar's offline no-model mode (owner integration tests).
pub fn run_pi_turn(dist: &Path, request: &Value, faux: bool) -> anyhow::Result<Value> {
    let socket_path = std::env::temp_dir().join(format!("ctox-pi-{}.sock", Uuid::new_v4()));
    let _daemon = spawn_sidecar(dist, &socket_path, faux)?;
    let mut stream = connect_with_retry(&socket_path, Duration::from_secs(10))?;

    let mut line = serde_json::to_string(request).context("serialize turn request")?;
    line.push('\n');
    stream
        .write_all(line.as_bytes())
        .context("write turn request")?;
    stream.flush().ok();

    let response_bytes = read_line(&mut stream)?;
    anyhow::ensure!(!response_bytes.is_empty(), "sidecar closed without a response");
    let response: Value =
        serde_json::from_slice(&response_bytes).context("parse turn response JSON")?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    fn node_available() -> bool {
        Command::new("node")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    #[test]
    fn faux_sidecar_serves_a_turn_over_the_socket() -> anyhow::Result<()> {
        let dist = sidecar_dist_path(&repo_root());
        if !dist.exists() {
            eprintln!("SKIP: pi-sidecar bundle not built ({})", dist.display());
            return Ok(());
        }
        if !node_available() {
            eprintln!("SKIP: `node` not on PATH");
            return Ok(());
        }

        let request = serde_json::json!({
            "id": "rust-1",
            "prompt": "add a marker",
            "files": { "index.js": "export const v = 1;\n" },
            "maxAssistantTurns": 4
        });
        let response = run_pi_turn(&dist, &request, true)?;

        assert_eq!(response["ok"], Value::Bool(true), "turn ok");
        assert_eq!(response["id"], "rust-1", "response echoes id");
        let has_marker = response["snapshot"]
            .as_array()
            .map(|entries| {
                entries.iter().any(|entry| {
                    entry["path"]
                        .as_str()
                        .map(|path| path.ends_with("faux-marker.js"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        assert!(has_marker, "faux write should round-trip over the socket");
        Ok(())
    }
}
