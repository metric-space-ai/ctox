#[path = "../../tests/support/signaling.rs"]
mod signaling;
use ctox_sync::{
    authority::{ExecutionSpec, Peer, WorkerMembership},
    contracts::{SyncIpcOperation, SyncIpcRequest, SyncIpcResponse, SyncIpcResult},
    host_config::{HostConfiguration, HostMember},
    ipc::{IPC_MAX_FRAME_BYTES, IPC_PROTOCOL_VERSION},
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    process::{Child, Command},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

async fn cli(
    binary: &Path,
    root: &Path,
    arguments: &[&str],
    input: Option<Vec<u8>>,
) -> Result<Value> {
    let mut child = Command::new(binary)
        .env_remove("CTOX_STATE_ROOT")
        .arg("sync")
        .args(arguments)
        .arg("--root")
        .arg(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        if let Some(input) = input {
            stdin.write_all(&input).await?;
        }
    }
    let output = tokio::time::timeout(Duration::from_secs(20), child.wait_with_output())
        .await
        .map_err(|_| {
            format!(
                "ctox sync {} timed out for {}",
                arguments[0],
                root.display()
            )
        })??;
    if !output.status.success() {
        return Err(format!(
            "ctox sync {} failed: {}",
            arguments[0],
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}
async fn start(binary: &Path, root: &Path) -> Result<(Child, PathBuf)> {
    let log = std::fs::File::create(root.join("host-stderr.log"))?;
    let mut child = Command::new(binary)
        .env_remove("CTOX_STATE_ROOT")
        .args(["sync", "run", "--root"])
        .arg(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(log)
        .kill_on_drop(true)
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let line = tokio::time::timeout(Duration::from_secs(20), lines.next_line())
        .await
        .map_err(|_| format!("listener publication timed out for {}", root.display()))??
        .ok_or_else(|| {
            let log = std::fs::read_to_string(root.join("host-stderr.log"))
                .unwrap_or_else(|_| "fixture stderr unavailable".into());
            format!(
                "host exited before listener publication for {}: {log}",
                root.display()
            )
        })?;
    let ready: Value = serde_json::from_str(&line)?;
    assert_eq!(ready["listener"], "active");
    let endpoint = PathBuf::from(
        ready["ipcEndpoint"]
            .as_str()
            .ok_or("missing IPC endpoint")?,
    );
    assert!(endpoint.is_absolute());
    Ok((child, endpoint))
}
async fn stop(child: &mut Child) -> Result<()> {
    let pid = child.id().ok_or("host already exited")?;
    // This PID belongs to the child spawned by this fixture, never a discovered process.
    if unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) } != 0 {
        return Err(io::Error::last_os_error().into());
    }
    let status = tokio::time::timeout(Duration::from_secs(15), child.wait()).await??;
    if !status.success() {
        return Err(format!("native host did not stop cleanly: {status}").into());
    }
    Ok(())
}
async fn ipc(
    endpoint: &Path,
    request_id: &str,
    operation: SyncIpcOperation,
) -> Result<SyncIpcResult> {
    Ok(tokio::time::timeout(Duration::from_secs(15), async {
        let mut socket = UnixStream::connect(endpoint).await?;
        let bytes = serde_json::to_vec(&SyncIpcRequest {
            version: IPC_PROTOCOL_VERSION,
            request_id: request_id.into(),
            operation,
        })?;
        socket.write_u32(bytes.len() as u32).await?;
        socket.write_all(&bytes).await?;
        let length = socket.read_u32().await? as usize;
        if length == 0 || length > IPC_MAX_FRAME_BYTES {
            return Err(io::Error::other("invalid IPC frame"));
        }
        let mut bytes = vec![0; length];
        socket.read_exact(&mut bytes).await?;
        let response: SyncIpcResponse = serde_json::from_slice(&bytes)?;
        assert_eq!(response.version, IPC_PROTOCOL_VERSION);
        assert_eq!(response.request_id, request_id);
        Ok::<_, io::Error>(response.result)
    })
    .await??)
}
async fn await_initial_quorum(endpoint: &Path) -> Result<()> {
    // Listener publication does not imply a connected majority. Use the
    // existing linearizable, nonmutating read before attempting enrollment.
    let mut last = None;
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let response = ipc(
                endpoint,
                "startup-membership",
                SyncIpcOperation::WorkerMembership { node_id: 4 },
            )
            .await?;
            match response {
                SyncIpcResult::WorkerMembership {
                    node_id: 4,
                    worker: None,
                } => return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(()),
                SyncIpcResult::Unavailable { .. } => last = Some(response),
                other => return Err(format!("unexpected initial membership: {other:?}").into()),
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| format!("voter did not confirm startup quorum: {last:?}"))??;
    Ok(())
}

fn bundle(root: &Path, binary: &Path) -> Result<()> {
    for directory in ["contracts", "src/apps/business-os", "bin"] {
        std::fs::create_dir_all(root.join(directory))?;
    }
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='ctox'\nversion='0.3.22'\n",
    )?;
    std::fs::write(
        root.join("contracts/binary_bundle_manifest.txt"),
        "isolated native CLI fixture\n",
    )?;
    std::fs::write(
        root.join("src/apps/business-os/index.html"),
        "<!-- unused by native Sync CLI fixture -->",
    )?;
    // The bundle marker references the tested artifact without mutating its
    // inode/link count on every fixture creation and teardown.
    std::os::unix::fs::symlink(binary, root.join("bin/ctox"))?;
    Ok(())
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
pub async fn run() -> Result<()> {
    let mut arguments = std::env::args_os().skip(1);
    let binary = std::fs::canonicalize(arguments.next().ok_or("expected CTOX binary")?)?;
    let work = std::fs::canonicalize(arguments.next().ok_or("expected work directory")?)?;
    let roots = tempfile::Builder::new()
        .prefix("host-cli-")
        .tempdir_in(&work)?;
    // Existing installations must preserve their legacy encryption key and
    // unrelated business data while new hosts create no business database.
    let legacy_root = roots.path().join("legacy");
    bundle(&legacy_root, &binary)?;
    std::fs::create_dir_all(legacy_root.join("runtime"))?;
    let legacy = rusqlite::Connection::open(legacy_root.join("runtime/ctox.sqlite3"))?;
    legacy.execute_batch("CREATE TABLE ctox_kv_store (kv_key TEXT PRIMARY KEY, kv_value TEXT, updated_at INTEGER); CREATE TABLE retained_business_data (id TEXT); INSERT INTO retained_business_data VALUES ('retained');")?;
    let legacy_key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
    legacy.execute(
        "INSERT INTO ctox_kv_store VALUES ('secret_master_key_b64', ?1, 0)",
        [legacy_key],
    )?;
    cli(&binary, &legacy_root, &["init"], None).await?;
    assert_eq!(
        std::fs::read_to_string(legacy_root.join("runtime/ctox-secrets.key"))?.trim(),
        legacy_key
    );
    assert_eq!(
        legacy.query_row(
            "SELECT COUNT(*) FROM ctox_kv_store WHERE kv_key='secret_master_key_b64'",
            [],
            |row| row.get::<_, u64>(0)
        )?,
        0
    );
    assert_eq!(
        legacy.query_row("SELECT id FROM retained_business_data", [], |row| row
            .get::<_, String>(0))?,
        "retained"
    );
    let signal = signaling::SignalingFixture::with_roles([
        "ctox_instance",
        "ctox_instance",
        "ctox_instance",
        "workjet_executor",
        "workjet_executor",
        "workjet_executor",
    ])
    .await;
    let mut identities = Vec::new();
    for id in 1..=4 {
        let root = roots.path().join(id.to_string());
        bundle(&root, &binary)?;
        let identity = if id == 4 {
            // Workjet's Node/OpenSSL PKCS#8-v1 key must remain the same key.
            let output = Command::new("node").args(["-e", "const c=require('node:crypto');const k=c.generateKeyPairSync('ed25519');console.log(JSON.stringify({identity:'ed25519:'+k.publicKey.export({format:'der',type:'spki'}).subarray(-32).toString('hex'),pkcs8:k.privateKey.export({format:'der',type:'pkcs8'}).toString('base64')}));"]).output().await?;
            assert!(output.status.success());
            let key: Value = serde_json::from_slice(&output.stdout)?;
            let expected = key["identity"].as_str().unwrap();
            let imported = cli(
                &binary,
                &root,
                &["import-key", expected],
                Some(key["pkcs8"].as_str().unwrap().as_bytes().to_vec()),
            )
            .await?;
            assert_eq!(imported["identity"], key["identity"]);
            imported
        } else {
            cli(&binary, &root, &["init"], None).await?
        };
        assert_eq!(cli(&binary, &root, &["identity"], None).await?, identity);
        identities.push(identity["identity"].as_str().unwrap().to_owned());
    }
    let voters: BTreeMap<_, _> = (1..=3)
        .map(|id| {
            (
                id,
                Peer {
                    identity: identities[id as usize - 1].clone(),
                    executor: id != 3,
                    data_replica: id != 3,
                },
            )
        })
        .collect();
    let member = WorkerMembership {
        node_id: 4,
        identity: identities[3].clone(),
        data_replica: true,
        revoked: false,
    };
    let provisioning_started = Instant::now();
    let mut hosts = Vec::new();
    for id in 1..=4 {
        let root = roots.path().join(id.to_string());
        let config = HostConfiguration {
            version: 1,
            scope_id: "cli-network".into(),
            local: if id == 4 {
                HostMember::Worker {
                    member: member.clone(),
                }
            } else {
                HostMember::Voter { node_id: id }
            },
            voters: voters.clone(),
            timing: Default::default(),
        };
        cli(
            &binary,
            &root,
            &["configure"],
            Some(serde_json::to_vec(&config)?),
        )
        .await?;
        let role = if id == 4 {
            "workjet_executor"
        } else {
            "ctox_instance"
        };
        cli(
            &binary,
            &root,
            &["transport"],
            Some(serde_json::to_vec(
                &json!({"signalingUrls":[format!("{}?role={role}", signal.url)],"iceServers":[]}),
            )?),
        )
        .await?;
        let host = start(&binary, &root).await?;
        let status = cli(&binary, &root, &["status"], None).await?;
        assert_eq!(status["listener"], "active");
        assert_eq!(status["nodeId"], id);
        assert_eq!(
            status["ipcEndpoint"].as_str().unwrap(),
            host.1.to_str().unwrap()
        );
        assert!(
            !root.join("runtime/ctox.sqlite3").exists(),
            "native CLI must not open the Business OS/turn-ledger database"
        );
        hosts.push(host);
    }
    let duplicate = cli(&binary, &roots.path().join("1"), &["run"], None)
        .await
        .err()
        .ok_or("duplicate host unexpectedly started")?;
    assert!(
        duplicate
            .to_string()
            .contains("another authority host owns this endpoint"),
        "duplicate host failed for an unexpected reason: {duplicate}"
    );
    let before = ipc(
        &hosts[3].1,
        "before-admission",
        SyncIpcOperation::WorkerMembership { node_id: 4 },
    )
    .await?;
    assert!(matches!(before, SyncIpcResult::Unavailable { .. }));
    await_initial_quorum(&hosts[0].1).await.inspect_err(|_| {
        eprintln!("fixture SDP offers: {:?}", signal.offers.lock().unwrap());
        eprintln!(
            "fixture signal counts: {:?}",
            signal.signals.lock().unwrap()
        );
        for id in 1..=4 {
            if let Ok(log) =
                std::fs::read_to_string(roots.path().join(id.to_string()).join("host-stderr.log"))
            {
                eprintln!(
                    "fixture host {id}: {}",
                    log.chars().take(16384).collect::<String>()
                );
            }
        }
    })?;
    let provisioning_ms = provisioning_started.elapsed().as_secs_f64() * 1000.0;
    let admission = ipc(
        &hosts[0].1,
        "admit",
        SyncIpcOperation::AdmitWorker {
            worker: member.clone(),
        },
    )
    .await?;
    assert!(
        matches!(admission, SyncIpcResult::WorkerApplied { ref worker } | SyncIpcResult::WorkerReplayed { ref worker } if worker == &member),
        "{admission:?}"
    );
    let current = ipc(
        &hosts[3].1,
        "membership",
        SyncIpcOperation::WorkerMembership { node_id: 4 },
    )
    .await?;
    assert!(
        matches!(current, SyncIpcResult::WorkerMembership { worker: Some(ref worker), .. } if worker == &member),
        "{current:?}"
    );
    let spec = ExecutionSpec {
        job_id: "cli-job".into(),
        session_id: "cli-session".into(),
        scope_id: "cli-network".into(),
        harness: "codex".into(),
        harness_version: "fixture".into(),
        model_route_id: "route".into(),
        gateway_account_id: "account".into(),
        model_id: "model".into(),
        required_capabilities: BTreeSet::new(),
    };
    let created = ipc(&hosts[3].1, "create", SyncIpcOperation::Create { spec }).await?;
    let ownership = match created {
        SyncIpcResult::Applied { ownership, .. } | SyncIpcResult::Replayed { ownership, .. } => {
            ownership
        }
        other => return Err(format!("job was not committed: {other:?}").into()),
    };
    assert_eq!(ownership.node_id, 4);
    // These are control-plane quorum reads, not Business OS command latency.
    // Keep every sample; a failed confirmation fails acceptance rather than
    // disappearing from the performance distribution.
    let mut validation_ms = Vec::with_capacity(20);
    for sample in 0..20 {
        let started = Instant::now();
        let response = ipc(
            &hosts[3].1,
            &format!("warm-validate-{sample}"),
            SyncIpcOperation::Validate {
                job_id: "cli-job".into(),
                ownership: ownership.clone(),
            },
        )
        .await?;
        assert!(
            matches!(response, SyncIpcResult::Authorized { .. }),
            "{response:?}"
        );
        validation_ms.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    validation_ms.sort_by(f64::total_cmp);
    let percentile =
        |percent: usize| validation_ms[(validation_ms.len() * percent).div_ceil(100) - 1];
    let reconnect_started = Instant::now();
    let rejoined = signal.disconnect_and_wait_for_rejoin("native000004").await;
    assert_eq!(rejoined, "native000005");
    let authorized = ipc(
        &hosts[3].1,
        "after-reconnect",
        SyncIpcOperation::Validate {
            job_id: "cli-job".into(),
            ownership: ownership.clone(),
        },
    )
    .await?;
    assert!(
        matches!(authorized, SyncIpcResult::Authorized { .. }),
        "{authorized:?}"
    );
    let reconnect_ms = reconnect_started.elapsed().as_secs_f64() * 1000.0;
    let restart_started = Instant::now();
    stop(&mut hosts[3].0).await?;
    assert_eq!(
        cli(&binary, &roots.path().join("4"), &["status"], None).await?["listener"],
        "inactive"
    );
    hosts[3] = start(&binary, &roots.path().join("4")).await?;
    let resumed = ipc(
        &hosts[3].1,
        "after-restart",
        SyncIpcOperation::Validate {
            job_id: "cli-job".into(),
            ownership: ownership.clone(),
        },
    )
    .await?;
    assert!(
        matches!(resumed, SyncIpcResult::Authorized { .. }),
        "{resumed:?}"
    );
    let restart_ms = restart_started.elapsed().as_secs_f64() * 1000.0;
    let revocation = ipc(
        &hosts[0].1,
        "revoke",
        SyncIpcOperation::RevokeWorker { node_id: 4 },
    )
    .await?;
    assert!(
        matches!(revocation, SyncIpcResult::WorkerApplied { ref worker } | SyncIpcResult::WorkerReplayed { ref worker } if worker.revoked),
        "{revocation:?}"
    );
    let denied = ipc(
        &hosts[3].1,
        "after-revocation",
        SyncIpcOperation::Validate {
            job_id: "cli-job".into(),
            ownership,
        },
    )
    .await?;
    assert!(
        matches!(denied, SyncIpcResult::Rejected { .. }),
        "{denied:?}"
    );
    for (child, _) in &mut hosts {
        stop(child).await?;
    }
    assert!(signal
        .offers
        .lock()
        .unwrap()
        .iter()
        .all(|(sender, receiver)| sender < receiver));
    println!(
        "{}",
        json!({"ok":true,"nativeProcesses":4,"legacySecretMigration":true,"importedWorkjetKey":true,"listenerStatus":true,"exclusiveHostLease":true,"confirmedMembership":true,"workerReconnect":true,"workerRestart":true,"revocation":true,"codingHarnessExecuted":false,
        "measurements": {
            "topology": "localhost-three-voters-one-worker",
            "dataset": "control-stores-with-one-job",
            "fourHostProvisioningMs": provisioning_ms,
            "workerReconnectMs": reconnect_ms,
            "workerRestartMs": restart_ms,
            "warmAuthorityValidation": {
                "sampleCount": validation_ms.len(),
                "p50Ms": percentile(50),
                "p95Ms": percentile(95),
                "samplesMs": validation_ms,
            },
            "businessCommandBudgetEvaluated": false
        }})
    );
    Ok(())
}
