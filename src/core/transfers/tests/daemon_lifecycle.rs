use ctox_transfers::{DaemonWorker, DownloadRequest, PeerRangeSource, PeerSource, Store};
use std::{
    future::Future,
    pin::Pin,
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};

type ResultFuture<'a, T> = Pin<Box<dyn Future<Output = anyhow::Result<T>> + Send + 'a>>;

struct PendingPeer {
    entered: Mutex<Option<mpsc::Sender<()>>>,
    drained: mpsc::Sender<()>,
    fail_shutdown: bool,
}
impl PeerRangeSource for PendingPeer {
    fn authorize<'a>(&'a self, _: &'a DownloadRequest) -> ResultFuture<'a, ()> {
        Box::pin(async move {
            if let Some(entered) = self.entered.lock().unwrap().take() {
                entered.send(()).unwrap();
            }
            std::future::pending().await
        })
    }
    fn read_range<'a>(
        &'a self,
        _: &'a DownloadRequest,
        _: u64,
        _: u64,
    ) -> ResultFuture<'a, Vec<u8>> {
        Box::pin(async { panic!("pending authorization must prevent payload access") })
    }
    fn shutdown(&self) -> ResultFuture<'_, ()> {
        Box::pin(async move {
            // Requires a functioning timer/runtime after the attempt is stopped.
            tokio::time::sleep(Duration::from_millis(10)).await;
            self.drained.send(()).unwrap();
            anyhow::ensure!(!self.fail_shutdown, "injected peer cleanup failure");
            Ok(())
        })
    }
}

#[test]
fn daemon_cancels_authorization_and_drains_peer_before_runtime_exit() {
    assert_active_peer_drained(false);
}

#[test]
fn explicit_shutdown_cancels_authorization_and_drains_peer_before_runtime_exit() {
    assert_active_peer_drained(true);
}

fn assert_active_peer_drained(explicit: bool) {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(
        temp.path().join("state.sqlite"),
        temp.path().join("objects"),
    )
    .unwrap();
    store
        .enqueue(DownloadRequest {
            id: "stopped".into(),
            sources: vec![],
            sha256: "a".repeat(64),
            size: 1,
            storage: None,
            peer_source: Some(PeerSource {
                instance_id: "source".into(),
                public_key: "source-key".into(),
                collection: "desktop_files".into(),
                file_id: "file".into(),
                account_binding: None,
            }),
        })
        .unwrap();
    let (entered, entry) = mpsc::channel();
    let (drained, drain) = mpsc::channel();
    let peer = Arc::new(PendingPeer {
        entered: Mutex::new(Some(entered)),
        drained,
        fail_shutdown: false,
    });
    let daemon = DaemonWorker::start_with_peer(store.clone(), peer).unwrap();
    entry.recv_timeout(Duration::from_secs(5)).unwrap();
    if explicit {
        daemon.shutdown().unwrap();
    } else {
        drop(daemon);
    }
    drain
        .try_recv()
        .expect("shutdown must await asynchronous peer cleanup");
    let job = store.get("stopped").unwrap();
    assert_ne!(job.state, "completed");
    assert!(job.receipt.is_none());
    let _restarted = store
        .worker()
        .expect("shutdown must release the exclusive worker lease");
}

#[test]
fn explicit_shutdown_reports_peer_cleanup_failure_and_releases_lease() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(
        temp.path().join("state.sqlite"),
        temp.path().join("objects"),
    )
    .unwrap();
    let (drained, drain) = mpsc::channel();
    let peer = Arc::new(PendingPeer {
        entered: Mutex::new(None),
        drained,
        fail_shutdown: true,
    });
    let daemon = DaemonWorker::start_with_peer(store.clone(), peer).unwrap();
    let error = daemon.shutdown().unwrap_err();
    assert!(format!("{error:#}").contains("injected peer cleanup failure"));
    drain
        .try_recv()
        .expect("cleanup must finish before returning its error");
    let _restarted = store
        .worker()
        .expect("failed cleanup must still release the worker lease");
}

#[test]
fn terminal_storage_failure_is_observable_by_foreground_owner() {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("state.sqlite");
    let store = Store::open(&database, temp.path().join("objects")).unwrap();
    let daemon = DaemonWorker::start(store).unwrap();
    let connection = rusqlite::Connection::open(database).unwrap();
    connection.busy_timeout(Duration::from_secs(5)).unwrap();
    connection
        .execute_batch("DROP TABLE ctox_transfer_jobs")
        .unwrap();
    drop(connection);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !daemon.is_finished() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(daemon.is_finished(), "storage failure must stop the worker");
    assert!(format!("{:#}", daemon.shutdown().unwrap_err()).contains("ctox_transfer_jobs"));
}
