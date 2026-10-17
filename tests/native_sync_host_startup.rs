#![cfg(unix)]
//! Regression: a storage-backed native host must publish its real listener
//! from the multi-thread async acceptance path, without blocking a worker.
#[allow(dead_code)] // Shared signaling support also serves the larger host suite.
#[path = "../src/core/sync/tests/support/signaling.rs"]
mod signaling;
use ctox_sync::{
    authority::Peer,
    contracts::{SyncIpcOperation, SyncIpcRequest, SyncIpcResponse, SyncIpcResult},
    host_config::{HostConfiguration, HostMember},
    ipc::{IPC_MAX_FRAME_BYTES, IPC_PROTOCOL_VERSION},
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
fn command(root: &Path, action: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ctox"));
    command
        .env_remove("CTOX_STATE_ROOT")
        .args(["sync", action, "--root"])
        .arg(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command
}
async fn cli(root: &Path, action: &str, input: Option<Vec<u8>>) -> Result<Value> {
    let mut child = command(root, action).spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        if let Some(input) = input {
            stdin.write_all(&input).await?;
        }
    }
    let output = tokio::time::timeout(Duration::from_secs(20), child.wait_with_output()).await??;
    if !output.status.success() {
        return Err(format!(
            "fixture {action} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}
fn bundle(root: &Path) -> Result<()> {
    for dir in ["contracts", "src/apps/business-os", "bin"] {
        std::fs::create_dir_all(root.join(dir))?;
    }
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='ctox'\nversion='0.3.22'\n",
    )?;
    std::fs::write(
        root.join("contracts/binary_bundle_manifest.txt"),
        "isolated native host regression\n",
    )?;
    std::fs::write(
        root.join("src/apps/business-os/index.html"),
        "<!-- no browser data -->",
    )?;
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_ctox"), root.join("bin/ctox"))?;
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_host_publishes_listener_from_multithread_runtime() -> Result<()> {
    let fixture = tempfile::Builder::new().prefix("hstart-").tempdir()?;
    let signal = signaling::SignalingFixture::with_roles(["ctox_instance"]).await;
    let mut voters = BTreeMap::new();
    for id in 1..=3 {
        let root = fixture.path().join(id.to_string());
        bundle(&root)?;
        let identity = cli(&root, "init", None).await?;
        assert!(
            !root.join("runtime/ctox.sqlite3").exists(),
            "native CLI created unrelated Core/turn-ledger state"
        );
        voters.insert(
            id,
            Peer {
                identity: identity["identity"]
                    .as_str()
                    .ok_or("missing fixture identity")?
                    .into(),
                executor: true,
                data_replica: true,
            },
        );
    }
    let root = fixture.path().join("1");
    let config = HostConfiguration {
        version: 1,
        scope_id: "host-startup-regression".into(),
        local: HostMember::Voter { node_id: 1 },
        voters,
        timing: Default::default(),
    };
    cli(&root, "configure", Some(serde_json::to_vec(&config)?)).await?;
    cli(&root, "transport", Some(serde_json::to_vec(&json!({"signalingUrls":[format!("{}?role=ctox_instance", signal.url)],"iceServers":[]}))?)).await?;
    let stderr = root.join("host-stderr.log");
    let mut child = command(&root, "run")
        .stdin(Stdio::null())
        .stderr(std::fs::File::create(&stderr)?)
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().ok_or("missing host stdout")?).lines();
    let line = tokio::time::timeout(Duration::from_secs(20), lines.next_line())
        .await??
        .ok_or_else(|| {
            format!(
                "host exited before listener publication: {}",
                std::fs::read_to_string(&stderr).unwrap_or_default()
            )
        })?;
    let ready: Value = serde_json::from_str(&line)?;
    assert_eq!(ready["listener"], "active");
    let endpoint = ready["ipcEndpoint"]
        .as_str()
        .ok_or("missing actual IPC endpoint")?;
    let response = tokio::time::timeout(Duration::from_secs(5), async {
        let mut socket = tokio::net::UnixStream::connect(endpoint).await?;
        let request = SyncIpcRequest {
            version: IPC_PROTOCOL_VERSION,
            request_id: "startup-regression".into(),
            operation: SyncIpcOperation::Hello {},
        };
        let bytes = serde_json::to_vec(&request)?;
        socket.write_u32(bytes.len() as u32).await?;
        socket.write_all(&bytes).await?;
        let length = socket.read_u32().await? as usize;
        if length == 0 || length > IPC_MAX_FRAME_BYTES {
            return Err("invalid listener reply".into());
        }
        let mut bytes = vec![0; length];
        socket.read_exact(&mut bytes).await?;
        Ok::<SyncIpcResponse, Box<dyn std::error::Error + Send + Sync>>(serde_json::from_slice(
            &bytes,
        )?)
    })
    .await??;
    assert_eq!(response.request_id, "startup-regression");
    assert!(
        matches!(response.result, SyncIpcResult::Ready { node_id: 1, .. }),
        "listener did not answer its actual host identity"
    );
    eprintln!("HOST_STARTUP_LISTENER_READY actual_ipc=true");
    // A live host already owns these files. Status must use that owner's IPC,
    // not try to acquire a second Core/Runtime/Secrets ownership lease.
    let owner_status = cli(&root, "status", None).await;
    let pid = child.id().ok_or("host disappeared after publication")?;
    if unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let stopped = tokio::time::timeout(Duration::from_secs(15), child.wait()).await??;
    assert!(
        stopped.success(),
        "native host did not drain after listener publication"
    );
    let owner_status = owner_status?;
    assert_eq!(owner_status["listener"], "active");
    assert_eq!(owner_status["nodeId"], 1);
    eprintln!("HOST_STARTUP_SHUTDOWN clean_shutdown=true status_via_owner=true");
    Ok(())
}
