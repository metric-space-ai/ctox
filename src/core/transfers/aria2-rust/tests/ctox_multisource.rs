//! Pinned mirrors must assemble one identity and never leave detached writers.
use aria2_rust::{
    http::{self, HttpJob, HttpProgress},
    options::OptionSet,
};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{watch, Barrier, Notify},
    task::JoinHandle,
};

#[derive(Clone)]
enum Reply {
    Body(Arc<Vec<u8>>),
    FailAfter(Arc<Barrier>),
    HoldAfter(Arc<Barrier>, Arc<Notify>, Arc<Notify>, Arc<Vec<u8>>),
}

struct Mirror {
    uri: String,
    ranges: Arc<Mutex<Vec<(usize, usize)>>>,
    task: JoinHandle<()>,
}
impl Drop for Mirror {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Mirror {
    async fn start(length: usize, reply: Reply) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let uri = format!("http://{}/payload", listener.local_addr().unwrap());
        let ranges = Arc::new(Mutex::new(Vec::new()));
        let observed = ranges.clone();
        let task = tokio::spawn(async move {
            let mut handlers = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let (mut stream, _) = accepted.unwrap();
                        let reply = reply.clone();
                        let observed = observed.clone();
                        handlers.spawn(async move {
                            let mut request = Vec::new();
                            while !request.ends_with(b"\r\n\r\n") && request.len() < 16384 {
                                let mut byte = [0];
                                if stream.read_exact(&mut byte).await.is_err() { return; }
                                request.push(byte[0]);
                            }
                            let text = String::from_utf8_lossy(&request);
                            if text.starts_with("HEAD ") {
                                let header = format!("HTTP/1.1 200 OK\r\nContent-Length: {length}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n");
                                let _ = stream.write_all(header.as_bytes()).await;
                                return;
                            }
                            let range = text.lines().find_map(|line| {
                                let (key, value) = line.split_once(':')?;
                                if !key.eq_ignore_ascii_case("range") { return None; }
                                let (start, end) = value.trim().strip_prefix("bytes=")?.split_once('-')?;
                                Some((start.parse::<usize>().ok()?, end.parse::<usize>().ok()?))
                            }).expect("every pinned payload request is bounded");
                            observed.lock().unwrap().push(range);
                            let body = match reply {
                                Reply::Body(body) => body,
                                Reply::FailAfter(barrier) => {
                                    barrier.wait().await;
                                    let _ = stream.write_all(b"HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
                                    return;
                                }
                                Reply::HoldAfter(barrier, release, sent, body) => {
                                    barrier.wait().await;
                                    release.notified().await;
                                    let (start, end) = range;
                                    let header = format!("HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{length}\r\nConnection: close\r\n\r\n", end-start+1);
                                    let _ = stream.write_all(header.as_bytes()).await;
                                    let _ = stream.write_all(&body[start..=end]).await;
                                    let _ = stream.shutdown().await;
                                    sent.notify_one();
                                    return;
                                }
                            };
                            let (start, end) = range;
                            let header = format!("HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{length}\r\nConnection: close\r\n\r\n", end-start+1);
                            let _ = stream.write_all(header.as_bytes()).await;
                            let _ = stream.write_all(&body[start..=end]).await;
                        });
                    }
                    _ = handlers.join_next(), if !handlers.is_empty() => {}
                }
            }
        });
        Self { uri, ranges, task }
    }
}

fn payload() -> Arc<Vec<u8>> {
    Arc::new((0..64 * 1024).map(|i| (i % 239) as u8).collect())
}
fn job(path: &Path, mirrors: &[&Mirror], bytes: &[u8]) -> (HttpJob, watch::Sender<bool>) {
    let mut opts = OptionSet::with_defaults();
    for (key, value) in [
        ("split", "2"),
        ("max-connection-per-server", "2"),
        ("min-split-size", "1024"),
        ("file-allocation", "none"),
        ("disk-cache", "0"),
        ("auto-save-interval", "0"),
        ("use-head", "true"),
        ("uri-selector", "inorder"),
        ("max-tries", "1"),
        ("timeout", "2"),
        ("connect-timeout", "2"),
        ("no-netrc", "true"),
    ] {
        opts.set(key, value);
    }
    opts.set("ctox-expected-length", bytes.len().to_string());
    opts.set(
        "checksum",
        format!("sha-256={}", hex::encode(Sha256::digest(bytes))),
    );
    let (tx, cancel) = watch::channel(false);
    (
        HttpJob {
            uris: mirrors.iter().map(|m| m.uri.clone()).collect(),
            dest: path.to_owned(),
            opts,
            progress: HttpProgress::new(),
            piece_length: 4096,
            cancel,
        },
        tx,
    )
}

#[tokio::test]
async fn distinct_mirrors_assemble_pinned_object_and_exclude_wrong_length() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let bytes = payload();
        let a = Mirror::start(bytes.len(), Reply::Body(bytes.clone())).await;
        let b = Mirror::start(bytes.len(), Reply::Body(bytes.clone())).await;
        let incompatible = Mirror::start(bytes.len() + 1, Reply::Body(bytes.clone())).await;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("payload");
        let (job, _cancel) = job(&path, &[&a, &b, &incompatible], &bytes);
        http::download(job).await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), *bytes);
        let mut ranges = a.ranges.lock().unwrap().clone();
        assert!(!ranges.is_empty(), "first mirror contributes payload");
        let other = b.ranges.lock().unwrap().clone();
        assert!(!other.is_empty(), "second mirror contributes payload");
        ranges.extend(other);
        ranges.sort();
        assert_eq!(ranges.first().unwrap().0, 0);
        assert_eq!(ranges.last().unwrap().1, bytes.len() - 1);
        assert!(ranges.windows(2).all(|r| r[0].1 + 1 == r[1].0));
        assert!(incompatible.ranges.lock().unwrap().is_empty());
    })
    .await
    .expect("bounded mirror fixture");
}

#[tokio::test]
async fn same_length_corrupt_mirror_cannot_satisfy_identity() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let bytes = payload();
        let a = Mirror::start(bytes.len(), Reply::Body(bytes.clone())).await;
        let b = Mirror::start(bytes.len(), Reply::Body(Arc::new(vec![0xff; bytes.len()]))).await;
        let dir = tempfile::tempdir().unwrap();
        let (job, _cancel) = job(&dir.path().join("payload"), &[&a, &b], &bytes);
        let error = http::download(job).await.unwrap_err().to_string();
        assert!(error.contains("checksum mismatch"), "{error}");
        assert!(!a.ranges.lock().unwrap().is_empty());
        assert!(!b.ranges.lock().unwrap().is_empty());
    })
    .await
    .expect("bounded corrupt mirror fixture");
}

#[tokio::test]
async fn pinned_multiple_sources_require_full_hash_before_payload() {
    let bytes = payload();
    let a = Mirror::start(bytes.len(), Reply::Body(bytes.clone())).await;
    let b = Mirror::start(bytes.len(), Reply::Body(bytes.clone())).await;
    let dir = tempfile::tempdir().unwrap();
    let (mut job, _cancel) = job(&dir.path().join("payload"), &[&a, &b], &bytes);
    job.opts.set("checksum", "");
    let error = tokio::time::timeout(Duration::from_secs(10), http::download(job))
        .await
        .unwrap()
        .unwrap_err()
        .to_string();
    assert!(error.contains("require SHA-256 identity"), "{error}");
    assert!(a.ranges.lock().unwrap().is_empty());
    assert!(b.ranges.lock().unwrap().is_empty());
}

async fn resume_bitmap_case(completed_tail: bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        use std::io::{Seek, SeekFrom, Write};
        let bytes = payload();
        let a = Mirror::start(bytes.len(), Reply::Body(bytes.clone())).await;
        let b = Mirror::start(bytes.len(), Reply::Body(bytes.clone())).await;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("payload");
        let tail = bytes.len() - 4096;
        let mut file = std::fs::File::create(&path).unwrap();
        file.seek(SeekFrom::Start(tail as u64)).unwrap();
        file.write_all(&bytes[tail..]).unwrap();
        file.sync_all().unwrap();
        drop(file);
        let mut control = aria2_rust::control_file::Control::new(4096, bytes.len() as u64);
        if completed_tail {
            control.set((tail / 4096) as u32);
        }
        control.save(&path).unwrap();
        let (mut job, _cancel) = job(&path, &[&a, &b], &bytes);
        job.opts.set("continue", "true");
        job.opts.set("allow-overwrite", "true");
        job.opts.set("auto-file-renaming", "false");
        http::download(job).await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), *bytes);
        let mut ranges = a.ranges.lock().unwrap().clone();
        ranges.extend(b.ranges.lock().unwrap().iter().copied());
        ranges.sort();
        assert_eq!(
            ranges.first().unwrap().0,
            0,
            "a sparse hole must be fetched"
        );
        assert!(ranges.windows(2).all(|r| r[0].1 + 1 == r[1].0));
        assert_eq!(
            ranges.last().unwrap().1,
            if completed_tail {
                tail - 1
            } else {
                bytes.len() - 1
            }
        );
        assert!(!aria2_rust::control_file::path_for(&path).exists());
    })
    .await
    .expect("bounded sparse resume fixture");
}

#[tokio::test]
async fn empty_bitmap_does_not_treat_sparse_file_length_as_completed_prefix() {
    resume_bitmap_case(false).await;
}

#[tokio::test]
async fn resume_fetches_only_missing_pieces_and_preserves_completed_tail() {
    resume_bitmap_case(true).await;
}

#[tokio::test]
async fn failed_range_cancels_other_writer_before_returning() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let bytes = payload();
        let barrier = Arc::new(Barrier::new(2));
        let release = Arc::new(Notify::new());
        let sent = Arc::new(Notify::new());
        let a = Mirror::start(bytes.len(), Reply::FailAfter(barrier.clone())).await;
        let b = Mirror::start(
            bytes.len(),
            Reply::HoldAfter(barrier, release.clone(), sent.clone(), bytes.clone()),
        )
        .await;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("payload");
        let (job, _cancel) = job(&path, &[&a, &b], &bytes);
        assert!(http::download(job).await.is_err());
        let control = aria2_rust::control_file::Control::load(&path)
            .expect("parallel writes require a resume bitmap before payload starts");
        assert!(!control.any(), "neither range completed");
        let before = std::fs::read(&path).unwrap();
        release.notify_one();
        sent.notified().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "no other range may write after the attempt returns"
        );
    })
    .await
    .expect("bounded detached writer regression");
}
