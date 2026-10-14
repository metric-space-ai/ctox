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
    let parent = PathBuf::from("/tmp");
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
