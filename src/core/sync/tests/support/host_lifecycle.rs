#[tokio::test]
async fn business_database_is_rejected_even_when_replication_list_is_empty() {
    let (config, key) = config();
    let (root, database, mut options) =
        native_fixture::options("ws://127.0.0.1:1".into(), &config.room(), "session").await;
    options.collections.clear();
    let error = host_runtime::run(
        &config,
        root.path(),
        &root.path().join("ipc"),
        key,
        options,
        std::future::pending(),
        |_| panic!("must not publish"),
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    database.close().await.unwrap();
}
use super::{native_fixture, SignalingFixture};
use ctox_sync::{
    authority::{auth::SigningIdentity, Peer},
    host_config::{HostConfiguration, HostMember},
    host_runtime,
};
use std::{
    collections::BTreeMap,
    io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
fn config() -> (HostConfiguration, Arc<SigningIdentity>) {
    let keys: Vec<_> = (0..3)
        .map(|_| {
            Arc::new(
                SigningIdentity::from_pkcs8(&SigningIdentity::generate_pkcs8().unwrap()).unwrap(),
            )
        })
        .collect();
    (
        HostConfiguration {
            version: 1,
            scope_id: "lifecycle".into(),
            local: HostMember::Voter { node_id: 1 },
            voters: keys
                .iter()
                .enumerate()
                .map(|(i, key)| {
                    (
                        (i + 1) as u64,
                        Peer {
                            identity: key.public_identity(),
                            executor: true,
                            data_replica: true,
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>(),
            timing: Default::default(),
        },
        keys[0].clone(),
    )
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_host_loop_withdraws_its_listener_after_stop() {
    let signal = SignalingFixture::start().await;
    let (config, key) = config();
    let (root, database, options) =
        native_fixture::control_options(signal.url.clone(), &config.room(), "session").await;
    std::fs::create_dir_all(config.directory(root.path()).unwrap()).unwrap();
    let ipc = root.path().join("ipc");
    let path = root.path().to_path_buf();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let (started, ready) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        host_runtime::run_with_authority(
            &config,
            &path,
            &ipc,
            key,
            options,
            async { stopped.await.map_err(io::Error::other) },
            move |ready, authority| {
                started
                    .send((ready, authority))
                    .map_err(|_| io::Error::other("startup receiver closed"))
            },
        )
        .await
    });
    let (ready, authority) = tokio::time::timeout(Duration::from_secs(10), ready)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ready.node_id, 1);
    assert_eq!(ready.scope_id, "lifecycle");
    assert_eq!(authority.node_id(), ready.node_id);
    assert_eq!(authority.scope_id(), ready.scope_id);
    assert!(ready.ipc_endpoint.exists());
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(!ready.ipc_endpoint.exists());
    assert!(
        authority.worker_membership(4).await.is_err(),
        "retained native authority must reject after host shutdown"
    );
    database.close().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failed_host_publication_closes_the_native_session_and_listener() {
    let signal = SignalingFixture::start().await;
    let (config, key) = config();
    let (root, database, options) =
        native_fixture::control_options(signal.url.clone(), &config.room(), "session").await;
    std::fs::create_dir_all(config.directory(root.path()).unwrap()).unwrap();
    let endpoint = Arc::new(Mutex::new(None));
    let observed = endpoint.clone();
    let error = host_runtime::run(
        &config,
        root.path(),
        &root.path().join("ipc"),
        key,
        options,
        std::future::pending(),
        move |ready| {
            *observed.lock().unwrap() = Some(ready.ipc_endpoint);
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "cannot publish local listener",
            ))
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert!(!endpoint.lock().unwrap().as_ref().unwrap().exists());
    database.close().await.unwrap();
}
#[tokio::test]
async fn mismatched_host_role_is_rejected_before_signaling() {
    let (config, key) = config();
    let (root, database, mut options) =
        native_fixture::control_options("ws://127.0.0.1:1".into(), &config.room(), "session").await;
    let attempts = Arc::new(AtomicUsize::new(0));
    let seen = attempts.clone();
    options.signaling_urls = Arc::new(move || {
        seen.fetch_add(1, Ordering::SeqCst);
        Vec::new()
    });
    options.peer_role = ctox_sync::native::NativePeerRole::WorkjetExecutor;
    let error = host_runtime::run(
        &config,
        root.path(),
        &root.path().join("ipc"),
        key,
        options,
        std::future::pending(),
        |_| panic!("must not publish"),
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert_eq!(attempts.load(Ordering::SeqCst), 0);
    database.close().await.unwrap();
}
