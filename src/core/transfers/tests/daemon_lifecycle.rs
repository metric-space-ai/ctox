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
            Ok(())
        })
    }
}

#[test]
fn daemon_cancels_authorization_and_drains_peer_before_runtime_exit() {
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
    });
    let daemon = DaemonWorker::start_with_peer(store.clone(), peer).unwrap();
    entry.recv_timeout(Duration::from_secs(5)).unwrap();
    drop(daemon);
    drain
        .try_recv()
        .expect("drop must await asynchronous peer cleanup");
    let job = store.get("stopped").unwrap();
    assert_ne!(job.state, "completed");
    assert!(job.receipt.is_none());
    let _restarted = store
        .worker()
        .expect("shutdown must release the exclusive worker lease");
}
