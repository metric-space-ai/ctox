use super::*;
use crate::service::store_startup::{isolate_test_roots, Foreground};
use std::io::{BufRead, Write};

fn raw_status(root: &std::path::Path) -> anyhow::Result<ServiceStatus> {
    let mut stream = std::os::unix::net::UnixStream::connect(service_socket_path(root))?;
    stream.set_read_timeout(Some(Duration::from_millis(500)))?;
    stream.set_write_timeout(Some(Duration::from_millis(500)))?;
    stream.write_all(b"{\"kind\":\"status\"}\n")?;
    let mut line = String::new();
    std::io::BufReader::new(stream).read_line(&mut line)?;
    match serde_json::from_str::<ServiceIpcResponse>(&line)? {
        ServiceIpcResponse::Status(status) => Ok(status),
        _ => anyhow::bail!("lifecycle did not return owner status"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn core_status_is_running_during_signaling_failure_and_after_restart() -> anyhow::Result<()> {
    let _scope = isolate_test_roots().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let failures = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = failures.clone();
    let task = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            observed.fetch_add(1, Ordering::AcqRel);
            drop(stream);
        }
    });
    struct Server(JoinHandle<()>);
    impl Drop for Server {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let _server = Server(task);
    let (root, mut initial, _) = crate::business_os::launch_context_v4::tests::fixture().await?;
    let config_root = root.path().to_owned();
    crate::persistence::run_core_sync(move || {
        crate::inference::runtime_env::set_runtime_env_value(
            &config_root,
            "CTOX_BUSINESS_OS_WEB_AUTOSTART",
            "0",
        )?;
        crate::inference::runtime_env::set_runtime_env_value(
            &config_root,
            "CTOX_BUSINESS_OS_ICE_SERVERS",
            r#"[{"urls":"stun:127.0.0.1:9"}]"#,
        )
    })
    .await?;
    std::fs::write(
        root.path().join("runtime/business-os-signaling-urls.json"),
        serde_json::to_vec(&vec![format!("ws://{address}/signal")])?,
    )?;
    std::os::unix::fs::symlink(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/apps/business-os"),
        root.path().join("business-os"),
    )?;
    initial.shutdown().await?;
    // Reproduce a manager start request before Core/socket readiness. The
    // independent PID-only contract below covers the stale-PID shortcut.
    let startup_root = root.path().to_owned();
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let startup = tokio::task::spawn_blocking(move || {
        let _ = entered.send(());
        super::super::wait_for_running_status_socket(
            &startup_root,
            std::time::Instant::now() + Duration::from_secs(5),
        )
    });
    waiting.await?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !startup.is_finished(),
        "startup completed before the Core owner"
    );
    let mut owner =
        Foreground::start(root.path().to_owned(), tokio::runtime::Handle::current()).await?;
    let startup_result = startup.await?;
    let generation = owner.inputs.core.writer.generation().0;
    tokio::time::timeout(Duration::from_secs(3), async {
        while failures.load(Ordering::Acquire) == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    let request = ctox_store::ipc::Request {
        request_id: 71,
        kind: ctox_store::ipc::JobKind::Read,
        method: "http.listen".into(),
        payload: serde_json::to_vec(&serde_json::json!({"addr":"127.0.0.1:0"}))?,
    };
    let bytes = ctox_store::ipc::request(
        crate::service::queue_store::endpoint(root.path()),
        request,
        Default::default(),
    )?
    .await?;
    let http = serde_json::from_slice::<serde_json::Value>(&bytes)?["addr"]
        .as_str()
        .unwrap()
        .to_owned();
    let probe_root = root.path().to_owned();
    let measured = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let launch = match ureq::get(&format!("http://{http}/api/business-os/launch-context"))
            .timeout(Duration::from_secs(1))
            .call()
        {
            Ok(r) => r.status(),
            Err(ureq::Error::Status(code, _)) => code,
            Err(_) => 0,
        };
        anyhow::ensure!(
            launch == 503,
            "launch must remain unavailable during signaling failure"
        );
        eprintln!(
            "M7_STATUS_OUTAGE {}",
            serde_json::json!({"launch_http":launch})
        );
        let mut latencies = vec![];
        for _ in 0..20 {
            let at = std::time::Instant::now();
            let status = raw_status(&probe_root)?;
            anyhow::ensure!(
                status.running && status.pid == Some(std::process::id()),
                "Core owner status was not running"
            );
            anyhow::ensure!(
                status.performance["queue_state"] == "unknown",
                "unobserved queue state claimed to be fresh"
            );
            anyhow::ensure!(
                status.performance["browser_ingress_ready"] == false,
                "fixture signaling unexpectedly ready"
            );
            anyhow::ensure!(
                status.performance["core_generation"] == generation,
                "status borrowed a different Core generation"
            );
            latencies.push(at.elapsed().as_secs_f64() * 1000.0);
        }
        super::super::wait_for_running_status_socket(
            &probe_root,
            std::time::Instant::now() + Duration::from_secs(1),
        )?;
        let status = crate::service::service_status_snapshot_with(
            &probe_root,
            &crate::service::StatusProbeOptions {
                reconcile_runtime_switch: false,
                systemd_cache_ttl: None,
                manager_probe: false,
                status_ipc_timeout: Some(Duration::from_millis(500)),
                lifecycle_alerts: false,
                include_business_os: false,
            },
        )?;
        anyhow::ensure!(
            status.running && status.performance["status_only"] == true && status.degraded_probe,
            "status client lost the current owner reply or degraded marker"
        );

        Ok((latencies, launch))
    })
    .await?;
    let unavailable = owner.inputs.browser_ingress_ready().is_err();
    owner.shutdown().await?;
    assert!(
        measured.is_ok(),
        "Core owner status unavailable while signaling is down: {:?}",
        measured.as_ref().err()
    );
    startup_result?;
    let (latencies, launch) = measured?;
    assert!(unavailable, "test did not keep browser ingress unavailable");
    assert!(
        !service_socket_path(root.path()).exists(),
        "retired lifecycle socket was not removed"
    );
    let mut restarted =
        Foreground::start(root.path().to_owned(), tokio::runtime::Handle::current()).await?;
    let next_generation = restarted.inputs.core.writer.generation().0;
    let next_root = root.path().to_owned();
    let next = tokio::task::spawn_blocking(move || raw_status(&next_root)).await?;
    restarted.shutdown().await?;
    let next = next?;
    assert!(
        next.running && next_generation != generation,
        "restart reused retired Core custody"
    );
    assert!(!service_socket_path(root.path()).exists());
    eprintln!(
        "M7_STATUS_METRICS {}",
        serde_json::json!({"status_replies":latencies.len(),"status_max_ms":latencies.iter().copied().fold(0.0,f64::max),"launch_http":launch,"signaling_failures":failures.load(Ordering::Acquire),"restart_running":next.running,"generation_changed":next_generation!=generation})
    );
    Ok(())
}

#[test]
fn starting_pid_is_not_a_running_core_status_socket() -> anyhow::Result<()> {
    let root = tempfile::Builder::new().prefix("m7pid-").tempdir()?;
    std::fs::create_dir_all(root.path().join("runtime"))?;
    super::super::write_pid_file(root.path(), std::process::id())?;
    let result = super::super::wait_for_running_status_socket(
        root.path(),
        std::time::Instant::now() + Duration::from_millis(150),
    );
    assert!(
        result.is_err(),
        "PID presence satisfied startup without an owned status reply"
    );
    Ok(())
}
