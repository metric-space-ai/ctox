//! Installed normal CLI processes; only the signaling relay is in this test process.
use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn copied_profiles(
    url: &str,
) -> anyhow::Result<(Vec<tempfile::TempDir>, BTreeMap<u64, Peer>)> {
    let mut sources = Vec::new();
    let mut owners = Vec::new();
    let mut peers = BTreeMap::new();
    for node in 1..=3 {
        let root = tempfile::Builder::new().prefix("c5s-").tempdir_in("/tmp")?;
        let owner =
            Foreground::start(root.path().to_owned(), tokio::runtime::Handle::current()).await?;
        peers.insert(
            node,
            Peer {
                identity: owner.inputs.instance_signer()?.key_id().into(),
                executor: true,
                data_replica: true,
            },
        );
        sources.push(root);
        owners.push(owner);
    }
    let mut copies = Vec::new();
    for (index, owner) in owners.iter_mut().enumerate() {
        let config = ctox_sync::host_config::HostConfiguration {
            version: 1,
            scope_id: "cutover5-installed".into(),
            local: HostMember::Voter {
                node_id: index as u64 + 1,
            },
            voters: peers.clone(),
            timing: Default::default(),
        };
        config.validate()?;
        let config = serde_json::to_string(&config)?;
        owner.inputs.databases["runtime"].writer.submit(WriteJob::new(Default::default(), move |tx| {
            tx.execute_batch("CREATE TABLE ctox_sync_host(singleton INTEGER PRIMARY KEY CHECK(singleton=1),configuration TEXT NOT NULL)")?;
            tx.execute("INSERT INTO ctox_sync_host VALUES(1,?1)", [config])?;
            Ok(())
        }))?.await?;
        owner.inputs.sync.writer.submit(WriteJob::new(Default::default(), |tx| {
            ctox_store::schema::create_collection(tx, &ctox_store::schema::CollectionDefinition {
                name: "installed_probe".into(), app_id: "probe".into(), schema_ver: 1,
                schema_json: serde_json::json!({"primaryKey":"id","type":"object","properties":{"id":{"type":"string"}}}),
                class: "full".into(), client_limit: 1000, byte_budget: 1 << 20, rights_ver: 1, app_ver: "1.0".into(),
            })?;
            for n in 0..100 {
                let id = format!("synthetic-{n}");
                tx.execute("INSERT INTO c_installed_probe(id,rev,schema_ver,data,deleted) VALUES(?1,'1',1,?2,0)", rusqlite::params![id,serde_json::json!({"id":id}).to_string()])?;
            }
            Ok(())
        }))?.await?;
        let root = owner.inputs.root.clone();
        let transport = serde_json::json!({"signalingUrls":[url],"iceServers":[]}).to_string();
        crate::persistence::run_core_sync(move || {
            crate::secrets::write_secret_record(
                &root,
                crate::sync_host::SIGNING_IDENTITY_SECRET_KEY.0,
                "transport:cutover5-installed",
                &transport,
                None,
                serde_json::json!({"source":"private-installed-fixture"}),
            )
        })
        .await?;
        owner.shutdown().await?;
        let copy = tempfile::Builder::new().prefix("c5i-").tempdir_in("/tmp")?;
        let from = owner.inputs.root.clone();
        let to = copy.path().to_owned();
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let runtime = to.join("runtime");
            std::fs::create_dir(&runtime)?;
            for path in [
                crate::paths::core_db(&from),
                crate::paths::sync_db(&from),
                crate::paths::runtime_config_db(&from),
                crate::paths::secrets_db(&from),
            ] {
                let _offline = ctox_store::ipc::MaintenanceOwner::acquire(
                    &crate::service::queue_store::endpoint(&from),
                    ctox_store::DbId::new(&path)?,
                )?;
                let wal = std::path::PathBuf::from(format!("{}-wal", path.display()));
                anyhow::ensure!(
                    !wal.exists() || wal.metadata()?.len() == 0,
                    "offline fixture retained WAL"
                );
                std::fs::copy(&path, runtime.join(path.file_name().unwrap()))?;
            }
            for name in [
                crate::secrets::SECRET_MASTER_KEY_FILE,
                "business-os-instance-id",
            ] {
                std::fs::copy(from.join("runtime").join(name), runtime.join(name))?;
            }
            std::fs::create_dir_all(to.join("src/core"))?;
            std::fs::write(
                to.join("src/core/main.rs"),
                "// private installed fixture\n",
            )?;
            std::fs::create_dir_all(to.join("contracts/history"))?;
            std::fs::write(
                to.join("contracts/history/creation-ledger.md"),
                "synthetic\n",
            )?;
            std::fs::write(
                to.join("Cargo.toml"),
                "[package]\nname=\"ctox\"\nversion=\"0.0.0\"\nedition=\"2021\"\n",
            )?;
            Ok(())
        })
        .await??;
        copies.push(copy);
    }
    Ok((copies, peers))
}

struct Installed {
    child: tokio::process::Child,
    root: std::path::PathBuf,
    log: std::path::PathBuf,
}
impl Installed {
    async fn start(
        binary: &std::path::Path,
        root: &std::path::Path,
        log: std::path::PathBuf,
    ) -> anyhow::Result<Self> {
        let stderr = std::fs::File::create(&log)?;
        let child = tokio::process::Command::new(binary)
            .args(["service", "--foreground", "--root"])
            .arg(root)
            .current_dir(root)
            .env("CTOX_STATE_ROOT", root.join("runtime"))
            .stdout(std::process::Stdio::null())
            .stderr(stderr)
            .kill_on_drop(true)
            .spawn()?;
        let mut node = Self {
            child,
            root: root.into(),
            log,
        };
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                anyhow::ensure!(
                    node.child.try_wait()?.is_none(),
                    "installed foreground exited before ready"
                );
                if tokio::fs::read_to_string(&node.log)
                    .await?
                    .contains("ctox v4 store daemon ready:")
                {
                    return Ok::<_, anyhow::Error>(());
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await??;
        Ok(node)
    }
    async fn endpoint(&self) -> anyhow::Result<std::path::PathBuf> {
        let pid = self.child.id().unwrap();
        tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            let inodes: std::collections::BTreeSet<_> =
                std::fs::read_dir(format!("/proc/{pid}/fd"))?
                    .filter_map(Result::ok)
                    .filter_map(|entry| std::fs::read_link(entry.path()).ok())
                    .filter_map(|link| {
                        link.to_str()?
                            .strip_prefix("socket:[")?
                            .strip_suffix(']')
                            .map(str::to_owned)
                    })
                    .collect();
            let sockets = std::fs::read_to_string("/proc/net/unix")?;
            for line in sockets.lines().skip(1) {
                let parts: Vec<_> = line.split_whitespace().collect();
                if parts.len() > 7 && inodes.contains(parts[6]) && parts[7].starts_with("/tmp/cs4-")
                {
                    return Ok(std::path::PathBuf::from(parts[7]));
                }
            }
            anyhow::bail!("native listener not held by installed process")
        })
        .await?
    }
    async fn membership(&self) -> anyhow::Result<()> {
        let path = self.endpoint().await?;
        tokio::time::timeout(Duration::from_secs(15), async move {
            let mut stream = tokio::net::UnixStream::connect(path).await?;
            let request = serde_json::to_vec(&ctox_sync::contracts::SyncIpcRequest {
                version: ctox_sync::ipc::IPC_PROTOCOL_VERSION,
                request_id: "cutover5-idle".into(),
                operation: ctox_sync::contracts::SyncIpcOperation::WorkerMembership {
                    node_id: 9001,
                },
            })?;
            stream
                .write_all(&(request.len() as u32).to_be_bytes())
                .await?;
            stream.write_all(&request).await?;
            let size = stream.read_u32().await? as usize;
            anyhow::ensure!(size <= 1048576, "IPC frame exceeded bound");
            let mut response = vec![0; size];
            stream.read_exact(&mut response).await?;
            let value: serde_json::Value = serde_json::from_slice(&response)?;
            anyhow::ensure!(
                value["result"]["type"] == "workerMembership"
                    && value["result"]["worker"].is_null(),
                "native quorum read failed: {value}"
            );
            Ok::<_, anyhow::Error>(())
        })
        .await?
    }
    async fn stop(&mut self) -> anyhow::Result<()> {
        let pid = self.child.id().unwrap();
        assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGTERM) }, 0);
        let status = tokio::time::timeout(Duration::from_secs(20), self.child.wait()).await??;
        anyhow::ensure!(
            status.success(),
            "installed foreground did not drain successfully"
        );
        anyhow::ensure!(
            !self.root.join("runtime/ctox_service.pid").exists(),
            "daemon PID survived drain"
        );
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "explicit private namespace acceptance; requires installed normal binary"]
async fn installed_daemons_keep_identity_authority_and_exclusive_writers() -> anyhow::Result<()> {
    let _scope = crate::service::store_startup::isolate_test_roots().await;
    anyhow::ensure!(
        std::env::var_os("CTOX_STATE_ROOT").is_none(),
        "refuses configured state root"
    );
    let host_net = std::env::var("CTOX_C5_HOST_NET")?;
    anyhow::ensure!(
        std::fs::read_link("/proc/self/ns/net")?.to_string_lossy() != host_net,
        "private network namespace required"
    );
    let binary = std::path::PathBuf::from(
        std::env::var_os("CTOX_C5_INSTALLED_BINARY")
            .ok_or_else(|| anyhow::anyhow!("installed binary missing"))?,
    );
    let out = std::path::PathBuf::from(std::env::var_os("CTOX_C5_INSTALLED_OUTPUT").unwrap());
    let signal = Signal::start().await?;
    let (profiles, peers) = copied_profiles(&signal.1).await?;
    let mut nodes = Vec::new();
    for (i, root) in profiles.iter().enumerate() {
        nodes.push(
            Installed::start(&binary, root.path(), out.join(format!("installed-{i}.log"))).await?,
        );
    }
    let mut rows = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        let mut expected_writers = std::collections::BTreeSet::new();
        for (request_id, name) in ["core", "sync", "runtime", "secrets"]
            .into_iter()
            .enumerate()
        {
            let result = ctox_store::ipc::request(
                crate::service::queue_store::endpoint(&node.root),
                ctox_store::ipc::Request {
                    request_id: request_id as u64 + 1,
                    kind: ctox_store::ipc::JobKind::Read,
                    method: "store.inspect".into(),
                    payload: serde_json::to_vec(name)?,
                },
                Default::default(),
            )?
            .await?;
            let inspect: serde_json::Value = serde_json::from_slice(&result)?;
            assert_eq!(inspect["queryOnly"], true);
            let generation = inspect["generation"].as_u64().unwrap();
            anyhow::ensure!(
                expected_writers.insert(format!("ctox-store-{generation}")),
                "two database handles share a writer generation"
            );
            rows.push(serde_json::json!({"node":i+1,"file":name,"inspection":inspect}));
        }
        let second = tokio::process::Command::new(std::env::current_exe()?)
            .args([
                "service::store_native::tests::installed::second_process_write_is_refused",
                "--exact",
                "--ignored",
                "--nocapture",
            ])
            .env("CTOX_C5_OWNED_FILE", crate::paths::sync_db(&node.root))
            .output()
            .await?;
        anyhow::ensure!(
            second.status.success()
                && String::from_utf8_lossy(&second.stdout).contains("C5_SECOND_WRITER_REFUSED"),
            "second process acquired writer"
        );
        let names: Vec<_> = std::fs::read_dir(format!("/proc/{}/task", node.child.id().unwrap()))?
            .filter_map(Result::ok)
            .filter_map(|t| std::fs::read_to_string(t.path().join("comm")).ok())
            .map(|n| n.trim().to_owned())
            .collect();
        anyhow::ensure!(
            expected_writers.iter().all(|expected| names
                .iter()
                .filter(|name| *name == expected)
                .count()
                == 1),
            "each database generation must have exactly one live writer thread"
        );
        anyhow::ensure!(
            !names.iter().any(|n| n.starts_with("business-os-rxd")),
            "V3 peer/background writer started"
        );
        rows.push(serde_json::json!({"node":i+1,"threads":names,"second_process_write_refused":true,"pinned_public_identity":peers[&(i as u64+1)].identity}));
    }
    tokio::time::sleep(Duration::from_secs(60)).await;
    for node in &nodes {
        node.membership().await?;
    }
    nodes[0].stop().await?;
    nodes[0] = Installed::start(
        &binary,
        profiles[0].path(),
        out.join("installed-restart.log"),
    )
    .await?;
    // Listener-ready deliberately precedes discovery/quorum-ready. Observe
    // the restarted node through the real read barrier, with a bounded deadline.
    let restarted = std::time::Instant::now();
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            match nodes[0].membership().await {
                Ok(()) => return Ok::<_, anyhow::Error>(()),
                Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
    })
    .await??;
    for node in &nodes {
        node.membership().await?;
    }
    let restart_quorum_ms = restarted.elapsed().as_millis();
    for node in &mut nodes {
        node.stop().await?;
    }
    tokio::fs::write(out.join("installed-proof.json"),serde_json::to_vec_pretty(&serde_json::json!({
        "normal_cli_processes":true,"separate_namespace":true,"copied_populated_profiles":true,"inspection":rows,
        "native_quorum_after_60s":true,"restart_same_pinned_key":true,"restart_quorum_ms":restart_quorum_ms,"graceful_shutdown":true,
        "scope":"K15 owner/process boundary; no browser K13/K21/K22 or ALL-app claim"
    }))?).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "write-attempt subprocess of installed_daemons_keep_identity_authority_and_exclusive_writers"]
async fn second_process_write_is_refused() -> anyhow::Result<()> {
    let path = std::env::var_os("CTOX_C5_OWNED_FILE")
        .ok_or_else(|| anyhow::anyhow!("owned fixture path missing"))?;
    let writer =
        ctox_store::registry::Registry.open(ctox_store::DbId::new(path)?, Default::default())?;
    // Queue an actual write intent before startup resolves. The file owner
    // must reject this process before either SQLite open or this SQL effect.
    let attempted = writer.submit(WriteJob::new(Default::default(), |tx| {
        tx.execute_batch("CREATE TABLE cutover_forbidden_second_writer(id INTEGER)")?;
        Ok(())
    }));
    match writer.ready().await {
        Err(ctox_store::StoreError::AlreadyOwned) => {
            if let Ok(job) = attempted {
                anyhow::ensure!(job.await.is_err(), "second process write committed");
            }
            println!("C5_SECOND_WRITER_REFUSED");
        }
        other => {
            let _ = writer.shutdown().await;
            anyhow::bail!("second writer was not refused: {other:?}");
        }
    }
    Ok(())
}
