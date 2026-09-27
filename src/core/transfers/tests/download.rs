use ctox_transfers::{DownloadRequest, Store};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

// Actual bounded loopback HTTP server exercises the pinned engine, not a mock downloader.
struct Source {
    url: String,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Source {
    fn new(body: Vec<u8>, delay: Duration) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/artifact", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = std::thread::spawn(move || {
            while !stopped.load(Ordering::Acquire) {
                let (mut stream, _) = match listener.accept() {
                    Ok(v) => v,
                    Err(_) => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while request.len() < 8192 && !request.ends_with(b"\r\n\r\n") {
                    if stream.read(&mut byte).unwrap_or(0) != 1 {
                        break;
                    }
                    request.push(byte[0]);
                }
                let text = String::from_utf8_lossy(&request);
                if text.starts_with("HEAD ") {
                    let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n", body.len());
                    continue;
                }
                let range = text.lines().find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("range: bytes=")
                        .map(str::to_owned)
                });
                let (start, end, status) = match range {
                    Some(r) => {
                        let (s, e) = r.trim().split_once('-').unwrap();
                        (
                            s.parse::<usize>().unwrap(),
                            e.parse::<usize>().unwrap_or(body.len() - 1),
                            "206 Partial Content",
                        )
                    }
                    None => (0, body.len() - 1, "200 OK"),
                };
                if start > end || end >= body.len() {
                    continue;
                }
                let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n", end-start+1, body.len());
                for bytes in body[start..=end].chunks(4096) {
                    if stopped.load(Ordering::Acquire) || stream.write_all(bytes).is_err() {
                        break;
                    }
                    std::thread::sleep(delay);
                }
            }
        });
        Self {
            url,
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.thread.take().unwrap().join().unwrap();
    }
}
fn request(id: &str, url: String, body: &[u8]) -> DownloadRequest {
    DownloadRequest {
        id: id.into(),
        sources: vec![url],
        sha256: format!("{:x}", Sha256::digest(body)),
        size: body.len() as u64,
    }
}
fn open(temp: &tempfile::TempDir) -> Store {
    Store::open(
        temp.path().join("ctox.sqlite3"),
        temp.path().join("transfers"),
    )
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_download_receipt_survives_reopen_and_duplicate_is_noop() {
    let temp = tempfile::tempdir().unwrap();
    let body = vec![0xa5; 180_000];
    let source = Source::new(body.clone(), Duration::ZERO);
    let store = open(&temp);
    let req = request("download", source.url.clone(), &body);
    store.enqueue(req.clone()).unwrap();
    let worker = store.worker().unwrap();
    assert!(
        store.worker().is_err(),
        "second executor must not recover or overwrite active work"
    );
    worker.run_next(&AtomicBool::new(false)).await.unwrap();
    let result = store.get("download").unwrap();
    assert_eq!(result.state, "completed", "{result:?}");
    let receipt = result.receipt.unwrap();
    assert_eq!(
        std::fs::read(temp.path().join("transfers").join(receipt.artifact)).unwrap(),
        body
    );
    drop(worker);
    let reopened = open(&temp);
    assert_eq!(reopened.enqueue(req.clone()).unwrap().state, "completed");
    let mut conflict = req;
    conflict.size += 1;
    assert!(reopened.enqueue(conflict).is_err());
    assert!(!reopened
        .worker()
        .unwrap()
        .run_next(&AtomicBool::new(false))
        .await
        .unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_identity_and_size_never_publish() {
    let temp = tempfile::tempdir().unwrap();
    let source = Source::new(vec![1; 1000], Duration::ZERO);
    let store = open(&temp);
    store
        .enqueue(request("wronghash", source.url.clone(), &vec![2; 1000]))
        .unwrap();
    store
        .enqueue(request("wrongsize", source.url.clone(), &vec![1; 999]))
        .unwrap();
    let worker = store.worker().unwrap();
    for _ in 0..2 {
        worker.run_next(&AtomicBool::new(false)).await.unwrap();
    }
    for id in ["wronghash", "wrongsize"] {
        let state = store.get(id).unwrap();
        assert_eq!(state.state, "failed");
        assert!(state.receipt.is_none());
    }
    assert_eq!(
        std::fs::read_dir(temp.path().join("transfers/objects"))
            .unwrap()
            .count(),
        0
    );
    assert!(!temp
        .path()
        .join("transfers/staging/wrongsize/payload")
        .exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pause_quiesces_writer_and_new_worker_resumes_verified_content() {
    let temp = tempfile::tempdir().unwrap();
    let body = vec![0x6b; 1_000_000];
    let source = Source::new(body.clone(), Duration::from_millis(4));
    let store = open(&temp);
    store
        .enqueue(request("resume", source.url.clone(), &body))
        .unwrap();
    let worker = store.worker().unwrap();
    let control = store.clone();
    let pause = async move {
        tokio::time::sleep(Duration::from_millis(330)).await;
        control.control("resume", "pause").unwrap();
    };
    let stop = AtomicBool::new(false);
    let (result, ()) = tokio::join!(worker.run_next(&stop), pause);
    result.unwrap();
    assert_eq!(store.get("resume").unwrap().state, "paused");
    drop(worker);
    let reopened = open(&temp);
    reopened.control("resume", "resume").unwrap();
    reopened.worker().unwrap().run_next(&stop).await.unwrap();
    assert_eq!(reopened.get("resume").unwrap().state, "completed");
}

#[test]
fn queued_cancel_is_terminal_and_urls_cannot_smuggle_credentials_or_options() {
    let temp = tempfile::tempdir().unwrap();
    let store = open(&temp);
    let req = request("cancel", "http://127.0.0.1:9/file".into(), b"x");
    store.enqueue(req.clone()).unwrap();
    assert_eq!(
        store.control("cancel", "cancel").unwrap().state,
        "cancelled"
    );
    assert_eq!(
        store.control("cancel", "resume").unwrap().state,
        "cancelled"
    );
    assert_eq!(store.enqueue(req).unwrap().state, "cancelled");
    for url in [
        "file:///etc/passwd",
        "http://user:password@example.com/file",
        "https://example.com/file?token=secret",
    ] {
        assert!(store.enqueue(request("invalid", url.into(), b"x")).is_err());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn publication_before_receipt_recovers_without_network() {
    let temp = tempfile::tempdir().unwrap();
    let store = open(&temp);
    let req = request(
        "crash",
        "http://127.0.0.1:9/unreachable".into(),
        b"durable bytes",
    );
    store.enqueue(req.clone()).unwrap();
    std::fs::write(
        temp.path().join("transfers/objects").join(&req.sha256),
        b"durable bytes",
    )
    .unwrap();
    let c = rusqlite::Connection::open(temp.path().join("ctox.sqlite3")).unwrap();
    c.execute(
        "UPDATE ctox_transfer_jobs SET state='running' WHERE id='crash'",
        [],
    )
    .unwrap();
    store
        .worker()
        .unwrap()
        .run_next(&AtomicBool::new(false))
        .await
        .unwrap();
    assert_eq!(store.get("crash").unwrap().state, "completed");
}
