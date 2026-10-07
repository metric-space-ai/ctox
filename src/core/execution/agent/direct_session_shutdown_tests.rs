//! Real embedded-client/runtime fixtures; no provider/model turn is started.
use super::*;
use std::sync::mpsc as std_mpsc;

struct TaskDropped(std_mpsc::Sender<()>);
impl Drop for TaskDropped {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

fn fixture(root: &Path) -> (PersistentSession, std_mpsc::Receiver<()>) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("owned fixture runtime");
    let harness_home = root.join("harness-home");
    std::fs::create_dir_all(&harness_home).expect("fixture harness home");
    let client = runtime.block_on(async {
        let config = ConfigBuilder::default()
            .codex_home(root.join("harness-home"))
            .build()
            .await
            .expect("isolated harness configuration");
        tokio::time::timeout(
            Duration::from_secs(10),
            InProcessAppServerClient::start(InProcessClientStartArgs {
                arg0_paths: Arg0DispatchPaths::default(),
                config: Arc::new(config),
                cli_overrides: Vec::new(),
                loader_overrides: Default::default(),
                cloud_requirements: Default::default(),
                auth_manager: None,
                thread_manager: None,
                feedback: CodexFeedback::new(),
                config_warnings: Vec::new(),
                session_source: SessionSource::Cli,
                enable_ctox_api_key_env: false,
                client_name: "persistent-shutdown-fixture".to_string(),
                client_version: "fixture".to_string(),
                experimental_api: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: DEFAULT_IN_PROCESS_CHANNEL_CAPACITY,
            }),
        )
        .await
        .expect("bounded embedded-client startup")
        .expect("real embedded client")
    });
    let (drop_tx, drop_rx) = std_mpsc::channel();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    drop(runtime.spawn(async move {
        let _dropped = TaskDropped(drop_tx);
        let _ = ready_tx.send(());
        std::future::pending::<()>().await;
    }));
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), ready_rx)
            .await
            .expect("owned fixture task started")
            .expect("ready signal");
    });
    let session = PersistentSession {
        runtime: Some(runtime),
        client: Some(client),
        thread_id: "fixture-without-model-turn".to_string(),
        seq: RequestIdSeq::new(),
        cwd: root.to_path_buf(),
        model: "fixture".to_string(),
        model_provider: None,
        api_provider: None,
        reasoning_effort: None,
        policy: CompactPolicy::from_settings(None, None, None, None, None, None),
        ctx_log: ContextLogger::open(root),
        root: root.to_path_buf(),
        base_instructions: String::new(),
        disable_active_tools: true,
        disable_mcp_servers: true,
        thread_config: None,
        read_only_sandbox: true,
        additional_writable_roots: Vec::new(),
        additional_readable_roots: Vec::new(),
        persistent_worker: false,
        native_checkpoint_binding: None,
        native_capture_thread: None,
        #[cfg(unix)]
        native_command_session_token: None,
        #[cfg(unix)]
        native_command_context: None,
        #[cfg(unix)]
        native_provider_admission: None,
        #[cfg(unix)]
        native_guest_registry: None,
        #[cfg(unix)]
        native_guest_execution: None,
        #[cfg(unix)]
        native_capture_owner: None,
        poisoned: false,
    };
    (session, drop_rx)
}

#[test]
fn persistent_public_shutdown_returns_checked_result_and_drains_runtime() {
    let home = tempfile::tempdir().expect("fixture home");
    let (session, stopped) = fixture(home.path());
    session.shutdown().expect("real checked client shutdown");
    stopped
        .recv_timeout(Duration::from_secs(1))
        .expect("owned runtime task stopped");
}

#[test]
fn persistent_public_shutdown_rejects_missing_runtime_or_client_owner() {
    for missing_runtime in [false, true] {
        let home = tempfile::tempdir().expect("fixture home");
        let (mut session, stopped) = fixture(home.path());
        if missing_runtime {
            session.runtime.take().unwrap().shutdown_background();
        } else {
            session.client.take().unwrap().abort_now();
        }
        let error = session
            .shutdown()
            .expect_err("partial ownership cannot acknowledge graceful shutdown");
        assert!(error.to_string().contains("ownership is missing"));
        stopped
            .recv_timeout(Duration::from_secs(1))
            .expect("remaining owned cleanup stopped the fixture task");
    }
}

#[test]
fn persistent_public_shutdown_from_async_owner_is_cleanup_without_success() {
    let home = tempfile::tempdir().expect("fixture home");
    let (session, stopped) = fixture(home.path());
    let caller_runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("caller fixture runtime");
    let error = caller_runtime
        .block_on(async move { session.shutdown() })
        .expect_err("forced cleanup is not a graceful shutdown receipt");
    assert!(error.to_string().contains("requires a synchronous owner"));
    stopped
        .recv_timeout(Duration::from_secs(1))
        .expect("owned fixture task was retired without nested-runtime panic");
    caller_runtime.shutdown_timeout(Duration::from_secs(1));
}

#[cfg(unix)]
#[test]
fn persistent_native_capture_rejects_ordinary_and_ambiguous_sources() {
    for poisoned in [false, true] {
        let home = tempfile::tempdir().expect("fixture home");
        let (mut session, stopped) = fixture(home.path());
        session.poisoned = poisoned;
        let error = session
            .quiesce_native_capture()
            .err()
            .expect("ordinary or ambiguous session cannot mint native capture authority");
        assert!(error.to_string().contains(if poisoned {
            "requires reconciliation"
        } else {
            "has not retired to capture authority"
        }));
        stopped
            .recv_timeout(Duration::from_secs(1))
            .expect("denied capture still drains its actual owned runtime");
    }
}

#[test]
fn persistent_capture_callback_runs_after_checked_shutdown_before_runtime_drain() {
    let home = tempfile::tempdir().unwrap();
    let (mut session, stopped) = fixture(home.path());
    let result = session
        .shutdown_inner_with("capturing final state", |runtime| {
            assert!(matches!(
                stopped.try_recv(),
                Err(std_mpsc::TryRecvError::Empty)
            ));
            runtime.block_on(async {
                tokio::task::yield_now().await;
                Ok("final-state")
            })
        })
        .unwrap();
    assert_eq!(result, Some("final-state"));
    stopped
        .recv_timeout(Duration::from_secs(1))
        .expect("capture runtime drained");
}

#[test]
fn persistent_capture_callback_never_runs_after_failed_client_shutdown() {
    let home = tempfile::tempdir().unwrap();
    let (mut session, stopped) = fixture(home.path());
    session.client.take().unwrap().abort_now();
    let mut called = false;
    let result = session.shutdown_inner_with("denied final state", |_| {
        called = true;
        Ok(())
    });
    assert!(result.is_err());
    assert!(!called);
    stopped
        .recv_timeout(Duration::from_secs(1))
        .expect("failed capture runtime drained");
}

#[test]
fn persistent_capture_failure_still_drains_the_owned_runtime() {
    let home = tempfile::tempdir().unwrap();
    let (mut session, stopped) = fixture(home.path());
    let result = session.shutdown_inner_with("failed final state", |_| {
        Err::<(), _>(anyhow::anyhow!("fixture final-state export failure"))
    });
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("final-state export failure"));
    stopped
        .recv_timeout(Duration::from_secs(1))
        .expect("failed export runtime drained");
}
