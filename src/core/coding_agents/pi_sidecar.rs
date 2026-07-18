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

/// Project a module's synced app source (`business_module_source_files` records)
/// into a `{path -> content}` map for a `CtoxTurnRequest.files` snapshot. This is
/// the app-source-projection workspace model: the sidecar edits a materialized
/// view of the source records; its writes come back as P0 commits. No host FS.
pub fn project_module_source(
    root: &Path,
    module_id: &str,
) -> anyhow::Result<serde_json::Map<String, Value>> {
    let records = crate::business_os::store::pull_collection_records(
        root,
        "business_module_source_files",
        None,
        None,
    )?;
    let mut files = serde_json::Map::new();
    if let Some(documents) = records.get("documents").and_then(Value::as_array) {
        for document in documents {
            if document.get("module_id").and_then(Value::as_str) != Some(module_id) {
                continue;
            }
            if document.get("_deleted").and_then(Value::as_bool) == Some(true) {
                continue;
            }
            let (Some(path), Some(content)) = (
                document.get("path").and_then(Value::as_str),
                document.get("content").and_then(Value::as_str),
            ) else {
                continue;
            };
            files.insert(path.to_string(), Value::String(content.to_string()));
        }
    }
    Ok(files)
}

/// Apply a turn's returned snapshot back into the module's app source. Each file
/// is written through the same policy-gated source path that records P0
/// versions/commits — the agent proposed, the trusted owner disposes. The
/// sidecar env cwd prefix (`/workspace/`) is stripped to the module-relative
/// path. Returns the paths written.
pub fn apply_turn_snapshot(
    root: &Path,
    module_id: &str,
    snapshot: &[Value],
) -> anyhow::Result<Vec<String>> {
    let mut applied = Vec::new();
    for entry in snapshot {
        if entry.get("kind").and_then(Value::as_str) != Some("file") {
            continue;
        }
        let Some(raw_path) = entry.get("path").and_then(Value::as_str) else {
            continue;
        };
        let path = raw_path
            .strip_prefix("/workspace/")
            .unwrap_or_else(|| raw_path.trim_start_matches('/'));
        let Some(content) = entry.get("content").and_then(Value::as_str) else {
            continue;
        };
        crate::business_os::store::save_module_source_record(
            root,
            crate::business_os::store::ModuleSourceSaveMutation {
                module_id: module_id.to_string(),
                path: path.to_string(),
                content: content.to_string(),
            },
        )?;
        applied.push(path.to_string());
    }
    Ok(applied)
}

/// The owner's core delegation primitive: one bounded coding turn against a
/// module's app source. Project the source into the request, run the pi turn
/// through the sidecar (`faux` = offline no-model), then apply the resulting
/// snapshot back into the source (recording P0 versions). Returns a summary.
pub fn run_module_coding_turn(
    root: &Path,
    dist: &Path,
    module_id: &str,
    prompt: &str,
    faux: bool,
) -> anyhow::Result<Value> {
    let files = project_module_source(root, module_id)?;
    let request = serde_json::json!({
        "id": module_id,
        "prompt": prompt,
        "files": files,
        "maxAssistantTurns": 8,
    });
    let response = run_pi_turn(dist, &request, faux)?;
    anyhow::ensure!(
        response.get("ok").and_then(Value::as_bool) == Some(true),
        "pi-sidecar turn failed: {}",
        response
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    );
    let empty = Vec::new();
    let snapshot = response
        .get("snapshot")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let applied = apply_turn_snapshot(root, module_id, snapshot)?;
    Ok(serde_json::json!({
        "ok": true,
        "module_id": module_id,
        "applied_files": applied,
        "message_count": response
            .get("messages")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0),
    }))
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

    #[test]
    fn projects_module_source_records_into_a_files_map() -> anyhow::Result<()> {
        use crate::business_os::store::{load_module_source_records, ModuleSourceLoadMutation};

        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let app_root = root.join("src").join("apps").join("business-os");
        std::fs::create_dir_all(app_root.join("modules").join("widget"))?;
        std::fs::write(app_root.join("index.html"), b"<!doctype html>")?;
        std::fs::write(
            app_root.join("modules").join("widget").join("module.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "id": "widget",
                "title": "Widget",
                "entry": "modules/widget/index.html"
            }))?,
        )?;
        std::fs::write(
            app_root.join("modules").join("widget").join("index.js"),
            "export const v = 1;\n",
        )?;

        load_module_source_records(
            root,
            &ModuleSourceLoadMutation {
                module_id: "widget".to_string(),
            },
        )?;

        let files = project_module_source(root, "widget")?;
        assert!(!files.is_empty(), "projected some source files");
        let has_content = files
            .values()
            .any(|value| value.as_str() == Some("export const v = 1;\n"));
        assert!(has_content, "widget source content projected into the files map");
        Ok(())
    }

    #[test]
    fn run_module_coding_turn_records_the_faux_edit() -> anyhow::Result<()> {
        use crate::business_os::store::{load_module_source_records, ModuleSourceLoadMutation};

        let dist = sidecar_dist_path(&repo_root());
        if !dist.exists() {
            eprintln!("SKIP: pi-sidecar bundle not built ({})", dist.display());
            return Ok(());
        }
        if !node_available() {
            eprintln!("SKIP: `node` not on PATH");
            return Ok(());
        }

        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let app_root = root.join("src").join("apps").join("business-os");
        std::fs::create_dir_all(app_root.join("modules").join("widget"))?;
        std::fs::write(app_root.join("index.html"), b"<!doctype html>")?;
        std::fs::write(
            app_root.join("modules").join("widget").join("module.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "id": "widget",
                "title": "Widget",
                "entry": "modules/widget/index.html"
            }))?,
        )?;
        std::fs::write(
            app_root.join("modules").join("widget").join("index.js"),
            "export const v = 1;\n",
        )?;
        load_module_source_records(
            root,
            &ModuleSourceLoadMutation {
                module_id: "widget".to_string(),
            },
        )?;

        let summary = run_module_coding_turn(root, &dist, "widget", "add a marker", true)?;
        assert_eq!(summary["ok"], Value::Bool(true), "owner turn ok");

        // The faux edit must now be part of the module's source records — proving
        // the full owner loop project -> pi turn -> apply -> P0 source records.
        let files = project_module_source(root, "widget")?;
        assert!(
            files.keys().any(|path| path.ends_with("faux-marker.js")),
            "faux edit recorded into module source via the owner loop"
        );
        Ok(())
    }

    #[test]
    fn apply_snapshot_round_trips_a_seeded_file_edit() -> anyhow::Result<()> {
        use crate::business_os::store::{load_module_source_records, ModuleSourceLoadMutation};

        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let app_root = root.join("src").join("apps").join("business-os");
        std::fs::create_dir_all(app_root.join("modules").join("widget"))?;
        std::fs::write(app_root.join("index.html"), b"<!doctype html>")?;
        std::fs::write(
            app_root.join("modules").join("widget").join("module.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "id": "widget",
                "title": "Widget",
                "entry": "modules/widget/index.html"
            }))?,
        )?;
        std::fs::write(
            app_root.join("modules").join("widget").join("index.js"),
            "export const v = 1;\n",
        )?;
        load_module_source_records(
            root,
            &ModuleSourceLoadMutation {
                module_id: "widget".to_string(),
            },
        )?;

        // Learn the projected path key for index.js, then simulate a turn snapshot
        // that edited exactly that file (with the sidecar env cwd prefix).
        let before = project_module_source(root, "widget")?;
        let key = before
            .keys()
            .find(|path| path.ends_with("index.js"))
            .cloned()
            .expect("index.js is projected");
        let snapshot = vec![serde_json::json!({
            "path": format!("/workspace/{key}"),
            "kind": "file",
            "content": "export const v = 2;\n"
        })];
        apply_turn_snapshot(root, "widget", &snapshot)?;

        // The SAME path must now carry the edit — not a nested duplicate.
        let after = project_module_source(root, "widget")?;
        assert_eq!(
            after.get(&key).and_then(Value::as_str),
            Some("export const v = 2;\n"),
            "a real edit round-trips project -> apply to the same module path ({key})"
        );
        Ok(())
    }
}
