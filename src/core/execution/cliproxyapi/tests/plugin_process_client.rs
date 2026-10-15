// These integration tests observe real Unix socket lifetimes.
// Windows named-pipe runtime acceptance remains separate.
// Origin: CTOX
// License: AGPL-3.0-only
#![cfg(unix)]

use ctox_cliproxyapi::internal::pluginhost::{
    abi::{PluginArtifact, PluginCall, PluginClientError, PluginLoader},
    callback_contexts::CallbackContextRegistry,
    host_callbacks::HostCallbackRouter,
    process_client::ProcessPluginLoader,
    rpc_client::RpcPluginClient,
};
use std::{path::PathBuf, sync::Arc, time::Duration};

fn fixture(root: &std::path::Path) -> ProcessPluginLoader {
    let callbacks = Arc::new(HostCallbackRouter::new(CallbackContextRegistry::new()));
    ProcessPluginLoader::new(root.to_path_buf(), callbacks).unwrap()
}
fn artifact() -> PluginArtifact {
    PluginArtifact {
        plugin_id: "fixture".into(),
        executable: PathBuf::from(env!("CARGO_BIN_EXE_cliproxy-differential")),
    }
}
fn hanging_call(deadline_unix_ms: Option<u64>) -> PluginCall {
    PluginCall {
        method: "fixture.no_response".into(),
        payload: serde_json::value::to_raw_value(&serde_json::json!({})).unwrap(),
        deadline_unix_ms,
    }
}
fn short_root() -> tempfile::TempDir {
    #[cfg(unix)]
    let parent = PathBuf::from(if cfg!(target_os = "macos") {
        "/Volumes/tmp"
    } else {
        "/tmp"
    });
    #[cfg(windows)]
    let parent = std::env::temp_dir();
    tempfile::Builder::new()
        .prefix("cpa-")
        .tempdir_in(parent)
        .unwrap()
}

#[tokio::test]
async fn process_loader_registers_and_closes_actual_child() {
    let root = short_root();
    let loader = fixture(root.path());
    let client = loader.open(&artifact()).await.unwrap();
    let rpc = RpcPluginClient::new(client);
    let registration = rpc
        .register("plugin.register", Vec::new(), None)
        .await
        .unwrap();
    assert_eq!(registration.metadata.name, "fixture-executor");
    rpc.shutdown().await.unwrap();
    assert_eq!(
        std::fs::read_dir(root.path().join(".cpa")).unwrap().count(),
        0
    );
}

#[tokio::test]
async fn process_loader_deadline_kills_unresponsive_child_and_closes_queue() {
    let root = short_root();
    let loader = fixture(root.path());
    let client = loader.open(&artifact()).await.unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    assert!(matches!(
        client.call(hanging_call(Some(now + 50))).await,
        Err(PluginClientError::DeadlineExceeded)
    ));
    tokio::time::timeout(Duration::from_secs(3), async {
        while std::fs::read_dir(root.path().join(".cpa")).unwrap().count() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(matches!(
        client.call(hanging_call(None)).await,
        Err(PluginClientError::Closed)
    ));
}

#[tokio::test]
async fn process_loader_caller_drop_kills_child_and_reload_uses_new_socket() {
    let root = short_root();
    let loader = fixture(root.path());
    let client = loader.open(&artifact()).await.unwrap();
    let second = loader.open(&artifact()).await.unwrap();
    assert_eq!(
        std::fs::read_dir(root.path().join(".cpa")).unwrap().count(),
        2
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), client.call(hanging_call(None)))
            .await
            .is_err()
    );
    tokio::time::timeout(Duration::from_secs(3), async {
        while std::fs::read_dir(root.path().join(".cpa")).unwrap().count() != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(matches!(
        client.call(hanging_call(None)).await,
        Err(PluginClientError::Closed)
    ));
    second.shutdown().await.unwrap();
}

#[tokio::test]
async fn queued_call_respects_deadline_while_another_call_is_running() {
    let root = short_root();
    let loader = fixture(root.path());
    let client = loader.open(&artifact()).await.unwrap();
    let mut first = Box::pin(client.call(hanging_call(None)));
    assert!(tokio::time::timeout(Duration::from_millis(30), &mut first)
        .await
        .is_err());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let second = tokio::time::timeout(
        Duration::from_millis(300),
        client.call(hanging_call(Some(now + 50))),
    )
    .await
    .expect("queued call ignored its deadline");
    assert!(matches!(second, Err(PluginClientError::DeadlineExceeded)));
    drop(first);
    assert!(client.shutdown().await.is_err());
}

#[tokio::test]
async fn malformed_call_does_not_kill_a_healthy_child() {
    let root = short_root();
    let loader = fixture(root.path());
    let client = loader.open(&artifact()).await.unwrap();
    let mut call = hanging_call(None);
    call.method = "NOT-A-VALID-METHOD".into();
    assert!(matches!(
        client.call(call).await,
        Err(PluginClientError::InvalidRequest)
    ));
    let rpc = RpcPluginClient::new(client);
    assert!(rpc
        .register("plugin.register", Vec::new(), None)
        .await
        .is_ok());
    rpc.shutdown().await.unwrap();
}

#[tokio::test]
async fn dropping_last_client_reaps_idle_child() {
    let root = short_root();
    let loader = fixture(root.path());
    let client = loader.open(&artifact()).await.unwrap();
    drop(client);
    tokio::time::timeout(Duration::from_secs(3), async {
        while std::fs::read_dir(root.path().join(".cpa")).unwrap().count() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn process_loader_rejects_invalid_artifact_before_spawn() {
    let root = short_root();
    let loader = fixture(root.path());
    let mut artifact = artifact();
    artifact.plugin_id = "../foreign".into();
    assert!(matches!(
        loader.open(&artifact).await,
        Err(PluginClientError::InvalidRequest)
    ));
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

fn stream_call(mode: &str, deadline_unix_ms: Option<u64>) -> PluginCall {
    PluginCall {
        method: "fixture.stream".into(),
        payload: serde_json::value::to_raw_value(&serde_json::json!({"mode":mode})).unwrap(),
        deadline_unix_ms,
    }
}
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
async fn next_chunk(
    stream: &mut ctox_cliproxyapi::internal::pluginhost::abi::PluginStream,
) -> Option<Result<Box<serde_json::value::RawValue>, PluginClientError>> {
    tokio::time::timeout(Duration::from_secs(3), stream.chunks.recv())
        .await
        .expect("stream did not advance")
}
async fn assert_reaped(root: &std::path::Path) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while std::fs::read_dir(root.join(".cpa")).unwrap().count() != 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("child or private socket was not reaped");
}
fn assert_pid_gone(pid: u64) {
    #[cfg(target_os = "linux")]
    assert!(
        !std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "owned plugin child remains alive or zombie"
    );
    #[cfg(not(target_os = "linux"))]
    let _ = pid;
}

#[tokio::test]
async fn process_stream_delivers_ordered_chunks_and_reuses_healthy_child() {
    let root = short_root();
    let client = fixture(root.path()).open(&artifact()).await.unwrap();
    for _ in 0..2 {
        let mut stream = client
            .call_stream(stream_call("normal", None))
            .await
            .unwrap();
        for index in 0..3 {
            let raw = next_chunk(&mut stream).await.unwrap().unwrap();
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(raw.get()).unwrap()["index"],
                index
            );
        }
        assert!(next_chunk(&mut stream).await.is_none());
    }
    client.shutdown().await.unwrap();
    assert_reaped(root.path()).await;
}

#[tokio::test]
async fn process_stream_application_error_is_visible_and_child_remains_usable() {
    let root = short_root();
    let client = fixture(root.path()).open(&artifact()).await.unwrap();
    let mut stream = client
        .call_stream(stream_call("application_error", None))
        .await
        .unwrap();
    assert!(next_chunk(&mut stream).await.unwrap().is_ok());
    assert!(
        matches!(next_chunk(&mut stream).await, Some(Err(PluginClientError::Plugin { code, http_status:429, retryable:true, .. })) if code=="fixture_rate_limit")
    );
    assert!(next_chunk(&mut stream).await.is_none());
    let rpc = RpcPluginClient::new(client);
    rpc.register("plugin.register", Vec::new(), None)
        .await
        .unwrap();
    rpc.shutdown().await.unwrap();
    assert_reaped(root.path()).await;
}

#[tokio::test]
async fn process_stream_protocol_faults_close_child_and_deliver_terminal_error() {
    for mode in ["bad_sequence", "wrong_id", "truncated", "missing_end"] {
        let root = short_root();
        let client = fixture(root.path()).open(&artifact()).await.unwrap();
        let mut stream = client.call_stream(stream_call(mode, None)).await.unwrap();
        if mode == "missing_end" {
            assert!(next_chunk(&mut stream).await.unwrap().is_ok());
        }
        assert!(
            matches!(
                next_chunk(&mut stream).await,
                Some(Err(
                    PluginClientError::InvalidResponse | PluginClientError::Closed
                ))
            ),
            "{mode}"
        );
        assert!(next_chunk(&mut stream).await.is_none());
        assert_reaped(root.path()).await;
        assert!(matches!(
            client.call(hanging_call(None)).await,
            Err(PluginClientError::Closed)
        ));
    }
}

#[tokio::test]
async fn process_stream_drop_reaps_child_without_a_followup_call() {
    let root = short_root();
    let client = fixture(root.path()).open(&artifact()).await.unwrap();
    let mut stream = client
        .call_stream(stream_call("stall", None))
        .await
        .unwrap();
    let first = next_chunk(&mut stream).await.unwrap().unwrap();
    let pid = serde_json::from_str::<serde_json::Value>(first.get()).unwrap()["pid"]
        .as_u64()
        .unwrap();
    drop(stream);
    assert_reaped(root.path()).await;
    assert_pid_gone(pid);
    assert!(matches!(
        client.call(hanging_call(None)).await,
        Err(PluginClientError::Closed)
    ));
}

#[tokio::test]
async fn process_stream_shutdown_interrupts_active_child_and_acknowledges_cleanup() {
    let root = short_root();
    let client = fixture(root.path()).open(&artifact()).await.unwrap();
    let mut stream = client
        .call_stream(stream_call("stall", None))
        .await
        .unwrap();
    let first = next_chunk(&mut stream).await.unwrap().unwrap();
    let pid = serde_json::from_str::<serde_json::Value>(first.get()).unwrap()["pid"]
        .as_u64()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), client.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        std::fs::read_dir(root.path().join(".cpa")).unwrap().count(),
        0
    );
    assert_pid_gone(pid);
    assert!(matches!(
        next_chunk(&mut stream).await,
        Some(Err(PluginClientError::Cancelled))
    ));
}

#[tokio::test]
async fn process_stream_deadline_keeps_terminal_error_when_data_buffer_is_full() {
    let root = short_root();
    let client = fixture(root.path()).open(&artifact()).await.unwrap();
    let mut stream = client
        .call_stream(stream_call("flood", Some(now_ms() + 500)))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(650)).await;
    assert_reaped(root.path()).await;
    for _ in 0..2 {
        assert!(next_chunk(&mut stream).await.unwrap().is_ok());
    }
    assert!(matches!(
        next_chunk(&mut stream).await,
        Some(Err(PluginClientError::DeadlineExceeded))
    ));
    assert!(next_chunk(&mut stream).await.is_none());
}

#[tokio::test]
async fn queued_stream_deadline_is_honored_behind_an_active_stream() {
    let root = short_root();
    let client = fixture(root.path()).open(&artifact()).await.unwrap();
    let mut first = client
        .call_stream(stream_call("stall", None))
        .await
        .unwrap();
    assert!(next_chunk(&mut first).await.unwrap().is_ok());
    let second = tokio::time::timeout(
        Duration::from_millis(300),
        client.call_stream(stream_call("normal", Some(now_ms() + 50))),
    )
    .await
    .unwrap();
    assert!(matches!(second, Err(PluginClientError::DeadlineExceeded)));
    client.shutdown().await.unwrap();
    assert_reaped(root.path()).await;
}
