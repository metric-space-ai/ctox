//! Segmented HTTP GET + Range dest-match.
use aria2_rust::http::{self, HttpJob, HttpProgress};
use aria2_rust::options::OptionSet;
use sha1::Digest;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;

async fn spawn_static(body: &'static [u8], filename: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let range = req.lines().find(|l| l.to_ascii_lowercase().starts_with("range:"));
                if req.starts_with("HEAD ") {
                    let hdr = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nContent-Disposition: attachment; filename=\"{}\"\r\n\r\n",
                        body.len(),
                        filename
                    );
                    let _ = s.write_all(hdr.as_bytes()).await;
                    return;
                }
                if let Some(r) = range {
                    let spec = r.split(':').nth(1).unwrap_or("").trim();
                    let spec = spec.trim_start_matches("bytes=");
                    let mut parts = spec.split('-');
                    let start: usize = parts.next().unwrap_or("0").parse().unwrap_or(0);
                    let end: usize = parts
                        .next()
                        .filter(|s| !s.is_empty())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(body.len() - 1);
                    let end = end.min(body.len() - 1);
                    let slice = &body[start..=end];
                    let hdr = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nAccept-Ranges: bytes\r\n\r\n",
                        slice.len(),
                        body.len()
                    );
                    let _ = s.write_all(hdr.as_bytes()).await;
                    let _ = s.write_all(slice).await;
                    return;
                }
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(body).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn http_get_and_ranges_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0xABu8; 32 * 1024].into_boxed_slice());
    let port = spawn_static(body, "blob.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("blob.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "4");
    opts.set("max-connection-per-server", "4");
    opts.set("min-split-size", "1024");
    opts.set("piece-length", "4096");
    opts.set("file-allocation", "none");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/blob.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 4096,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn http_socketcore_get_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0xCDu8; 48 * 1024].into_boxed_slice());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            let body = body;
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    return;
                }
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let mut pkt = hdr.into_bytes();
                pkt.extend_from_slice(body);
                let _ = s.write_all(&pkt).await;
            });
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("sc.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/sc.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::reset_http_io();
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert!(
        http::last_http_send() >= 1,
        "C++ SocketCore HttpRequest GET must writeData, got {}",
        http::last_http_send()
    );
    assert!(
        http::last_http_recv() >= 1,
        "C++ SocketCore HttpRequest GET must readData, got {}",
        http::last_http_recv()
    );
    assert!(
        http::last_leftover_inline() >= 1,
        "C++ SocketBuffer leftover must pwrite/cache without per-chunk Vec, got {}",
        http::last_leftover_inline()
    );
    assert!(
        http::last_recv_inline() >= 1,
        "C++ SocketBuffer recv window must pwrite/cache tmp without leftover.extend, got {}",
        http::last_recv_inline()
    );
}

async fn spawn_redirect(body: &'static [u8], hops: usize) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let line = req.lines().next().unwrap_or("");
                let path = line.split_whitespace().nth(1).unwrap_or("/");
                if path == "/final.bin" {
                    let hdr = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let mut pkt = hdr.into_bytes();
                    pkt.extend_from_slice(body);
                    let _ = s.write_all(&pkt).await;
                    return;
                }
                let hop: usize = path
                    .trim_start_matches("/r")
                    .parse()
                    .unwrap_or(0);
                let next = if hop + 1 >= hops {
                    "/final.bin".to_string()
                } else {
                    format!("/r{}", hop + 1)
                };
                let hdr = format!(
                    "HTTP/1.1 302 Found\r\nLocation: {next}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                let _ = s.write_all(hdr.as_bytes()).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn http_redirect_302_dest_match() {
    let body: &'static [u8] = b"redirect-dest-match-bytes";
    let port = spawn_redirect(body, 1).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("redir.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/r0")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "C++ HttpSkipResponseCommand 302 must dest-match"
    );
}

#[tokio::test]
async fn http_redirect_relative_chain_dest_match() {
    let body: &'static [u8] = b"redirect-chain-dest-match";
    let port = spawn_redirect(body, 3).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("chain.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/r0")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn http_redirect_over_max_no_dest() {
    let body: &'static [u8] = b"should-not-write";
    let port = spawn_redirect(body, 21).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("toomany.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/r0")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    let err = http::download(job).await.unwrap_err();
    assert!(
        err.to_string().contains("too many redirects"),
        "C++ Request::MAX_REDIRECT=20 must abort, got {err}"
    );
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap_or_default() != body,
        "too many redirects must not dest-match"
    );
}

const GZIP_PLAIN: &[u8] = b"gzip-dest-match-payload";
const GZIP_WIRE: &[u8] = &[
    0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0x4b, 0xaf, 0xca, 0x2c, 0xd0, 0x4d,
    0x49, 0x2d, 0x2e, 0xd1, 0xcd, 0x4d, 0x2c, 0x49, 0xce, 0xd0, 0x2d, 0x48, 0xac, 0xcc, 0xc9, 0x4f,
    0x4c, 0x01, 0x00, 0x33, 0x13, 0xdc, 0xc7, 0x17, 0x00, 0x00, 0x00,
];

async fn spawn_gzip_only() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    return;
                }
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    GZIP_WIRE.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(GZIP_WIRE).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn http_accept_gzip_true_dest_match() {
    let port = spawn_gzip_only().await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("g.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("http-accept-gzip", "true");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/g.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        GZIP_PLAIN,
        "--http-accept-gzip=true must dest-match decompressed payload"
    );
}

#[tokio::test]
async fn http_accept_gzip_false_keeps_wire_dest_match() {
    let port = spawn_gzip_only().await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("raw.gz");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("http-accept-gzip", "false");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/raw.gz")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    let got = std::fs::read(&dest).unwrap();
    assert_eq!(
        got, GZIP_WIRE,
        "--http-accept-gzip=false must dest-match gzip wire bytes"
    );
    assert_ne!(got, GZIP_PLAIN);
}

#[tokio::test]
async fn disk_cache_zero_http_dest_match() {
    let body: &'static [u8] = b"disk-cache-zero-payload-bytes";
    let port = spawn_static(body, "dc0.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("dc0.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("disk-cache", "0");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/dc0.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn disk_cache_http_segmented_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0xCDu8; 24 * 1024].into_boxed_slice());
    let port = spawn_static(body, "dcc.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("dcc.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "4");
    opts.set("max-connection-per-server", "4");
    opts.set("min-split-size", "1024");
    opts.set("piece-length", "4096");
    opts.set("file-allocation", "none");
    opts.set("disk-cache", "8K");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/dcc.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 4096,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn keep_dest_fd_http_split_pwrite_dest_match() {
    // C++ DefaultDiskWriter: one dest fd, concurrent Range pwrite, dest-match.
    let body: &'static [u8] = Box::leak(
        (0..32 * 1024u32)
            .map(|i| (i % 251) as u8)
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    let port = spawn_static(body, "pwrite.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("pwrite.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "4");
    opts.set("max-connection-per-server", "4");
    opts.set("min-split-size", "1024");
    opts.set("piece-length", "4096");
    opts.set("file-allocation", "trunc");
    opts.set("disk-cache", "0");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/pwrite.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 4096,
        cancel: rx,
    };
    aria2_rust::storage::reset_try_pwrite();
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert!(
        aria2_rust::storage::last_try_pwrite() > 0,
        "C++ DefaultDiskWriter: HTTP body must pwrite without per-chunk await"
    );
}

#[tokio::test]
async fn http_single_connection_dest_match() {
    let body: &'static [u8] = b"hello-aria2-rust-single-get";
    let port = spawn_static(body, "one.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("one.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/one.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn http_continue_skips_prefix() {
    let body: &'static [u8] = Box::leak((0u8..=255).cycle().take(8192).collect::<Vec<_>>().into_boxed_slice());
    let port = spawn_static(body, "c.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("c.bin");
    std::fs::write(&dest, &body[..4096]).unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("continue", "true");
    opts.set("split", "2");
    opts.set("max-connection-per-server", "2");
    opts.set("min-split-size", "1");
    opts.set("piece-length", "4096");
    opts.set("file-allocation", "none");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/c.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 4096,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn http_checksum_sha1_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0xCDu8; 8192].into_boxed_slice());
    let port = spawn_static(body, "sum.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("sum.bin");
    let digest = hex::encode(sha1::Sha1::digest(body));
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "2");
    opts.set("max-connection-per-server", "2");
    opts.set("min-split-size", "1");
    opts.set("piece-length", "4096");
    opts.set("file-allocation", "none");
    opts.set("checksum", format!("sha-1={digest}"));
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/sum.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 4096,
        cancel: rx,
    };
    aria2_rust::checksum::reset_held_check();
    aria2_rust::checksum::reset_open_check();
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert!(
        aria2_rust::checksum::last_held_check() >= 1,
        "C++ CheckIntegrityCommand must pread still-open dest fd"
    );
    assert_eq!(
        aria2_rust::checksum::last_open_check(),
        0,
        "C++ CheckIntegrityCommand must not File::open dest a second time"
    );
}

#[tokio::test]
async fn http_checksum_sha256_dest_match() {
    use sha2::Digest;
    let body: &'static [u8] = Box::leak(vec![0xABu8; 8192].into_boxed_slice());
    let port = spawn_static(body, "sum256.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("sum256.bin");
    let digest = hex::encode(sha2::Sha256::digest(body));
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "2");
    opts.set("max-connection-per-server", "2");
    opts.set("min-split-size", "1");
    opts.set("piece-length", "4096");
    opts.set("file-allocation", "none");
    opts.set("checksum", format!("sha-256={digest}"));
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/sum256.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 4096,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn http_checksum_sha256_mismatch_rejected() {
    let body: &'static [u8] = b"checksum-sha256-mismatch-payload";
    let port = spawn_static(body, "bad256.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("bad256.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set(
        "checksum",
        "sha-256=0000000000000000000000000000000000000000000000000000000000000000",
    );
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/bad256.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    let err = http::download(job).await;
    assert!(err.is_err(), "wrong sha-256 checksum must fail");
    assert!(
        err.unwrap_err().to_string().contains("checksum"),
        "error must mention checksum"
    );
}

#[tokio::test]
async fn http_checksum_sha512_dest_match() {
    use sha2::Digest;
    let body: &'static [u8] = Box::leak(vec![0xEFu8; 4096].into_boxed_slice());
    let port = spawn_static(body, "sum512.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("sum512.bin");
    let digest = hex::encode(sha2::Sha512::digest(body));
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "2");
    opts.set("max-connection-per-server", "2");
    opts.set("min-split-size", "1");
    opts.set("piece-length", "2048");
    opts.set("file-allocation", "none");
    opts.set("checksum", format!("sha-512={digest}"));
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/sum512.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 2048,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn http_checksum_adler32_dest_match() {
    let body: &'static [u8] = b"adler32-http-dest-match-bytes";
    let port = spawn_static(body, "adler.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("adler.bin");
    // zlib Adler-32 of the payload (C++ HashFunc adler32).
    let mut a: u64 = 1;
    let mut b: u64 = 0;
    for &x in body {
        a = (a + x as u64) % 65521;
        b = (b + a) % 65521;
    }
    let digest = format!("{:08x}", (b << 16) | a);
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("checksum", format!("adler32={digest}"));
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/adler.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn http_checksum_mismatch_rejected() {
    let body: &'static [u8] = b"checksum-mismatch-payload";
    let port = spawn_static(body, "bad.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("bad.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("checksum", "sha-1=0000000000000000000000000000000000000000");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/bad.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    let err = http::download(job).await;
    assert!(err.is_err(), "wrong checksum must fail");
    assert!(
        err.unwrap_err().to_string().contains("checksum"),
        "error must mention checksum"
    );
}

async fn spawn_auth(body: &'static [u8]) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let low = req.to_ascii_lowercase();
                let ok = low.contains("referer: http://example.test/page")
                    && low.contains("x-token: s3cret")
                    && req.lines().any(|l| {
                        let l = l.trim();
                        l.to_ascii_lowercase().starts_with("authorization:")
                            && l.contains("Basic ")
                    });
                if !ok {
                    let _ = s.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n").await;
                    return;
                }
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(body).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn http_referer_header_user_dest_match() {
    let body: &'static [u8] = b"auth-header-payload";
    let port = spawn_auth(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("auth.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("referer", "http://example.test/page");
    opts.set("header", "X-Token: s3cret");
    opts.set("http-user", "alice");
    opts.set("http-passwd", "secret");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/auth.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn http_missing_auth_headers_rejected() {
    let body: &'static [u8] = b"auth-header-payload";
    let port = spawn_auth(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("nope.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/nope.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    assert!(http::download(job).await.is_err());
}

async fn spawn_conn_cap(body: &'static [u8], cap: usize) -> u16 {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let inflight = Arc::new(AtomicUsize::new(0));
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            let inflight = inflight.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let n = inflight.fetch_add(1, Ordering::SeqCst) + 1;
                if n > cap {
                    inflight.fetch_sub(1, Ordering::SeqCst);
                    let _ = s
                        .write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n")
                        .await;
                    return;
                }
                let range = req.lines().find(|l| l.to_ascii_lowercase().starts_with("range:"));
                let is_probe = range.is_some_and(|r| r.contains("bytes=0-0"));
                if !is_probe {
                    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
                }
                if let Some(r) = range {
                    let spec = r.split(':').nth(1).unwrap_or("").trim();
                    let spec = spec.trim_start_matches("bytes=");
                    let mut parts = spec.split('-');
                    let start: usize = parts.next().unwrap_or("0").parse().unwrap_or(0);
                    let end: usize = parts
                        .next()
                        .filter(|s| !s.is_empty())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(body.len() - 1);
                    let end = end.min(body.len() - 1);
                    let slice = &body[start..=end];
                    let hdr = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nAccept-Ranges: bytes\r\n\r\n",
                        slice.len(),
                        body.len()
                    );
                    let _ = s.write_all(hdr.as_bytes()).await;
                    let _ = s.write_all(slice).await;
                } else {
                    let hdr = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n\r\n",
                        body.len()
                    );
                    let _ = s.write_all(hdr.as_bytes()).await;
                    let _ = s.write_all(body).await;
                }
                inflight.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
    port
}

#[tokio::test]
async fn max_connection_per_server_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0xABu8; 16 * 1024].into_boxed_slice());
    let port = spawn_conn_cap(body, 2).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("cap.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "4");
    opts.set("max-connection-per-server", "2");
    opts.set("min-split-size", "1");
    opts.set("piece-length", "4096");
    opts.set("file-allocation", "none");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/cap.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 4096,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn max_connection_per_server_over_cap_fails() {
    let body: &'static [u8] = Box::leak(vec![0xABu8; 16 * 1024].into_boxed_slice());
    let port = spawn_conn_cap(body, 2).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("over.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "4");
    opts.set("max-connection-per-server", "8");
    opts.set("min-split-size", "1");
    opts.set("piece-length", "4096");
    opts.set("file-allocation", "none");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/over.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 4096,
        cancel: rx,
    };
    assert!(http::download(job).await.is_err());
}

async fn spawn_no_cache(body: &'static [u8]) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_ascii_lowercase();
                let ok = req.contains("cache-control: no-cache") && req.contains("pragma: no-cache");
                if !ok {
                    let _ = s.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n").await;
                    return;
                }
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(body).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn http_no_cache_dest_match() {
    let body: &'static [u8] = b"no-cache-payload";
    let port = spawn_no_cache(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("nc.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/nc.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn http_no_cache_false_rejected() {
    let body: &'static [u8] = b"no-cache-payload";
    let port = spawn_no_cache(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("cache.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("http-no-cache", "false");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/cache.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    assert!(http::download(job).await.is_err());
}

async fn spawn_proxy_origin(body: &'static [u8]) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_ascii_lowercase();
                if !req.contains("x-aria-proxy: 1") {
                    let _ = s
                        .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")
                        .await;
                    return;
                }
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(body).await;
            });
        }
    });
    port
}

async fn spawn_forward_proxy(require_auth: bool) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = s.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    return;
                }
                let req = String::from_utf8_lossy(&buf[..n]);
                if require_auth {
                    let ok = req.lines().any(|l| {
                        let l = l.trim();
                        l.to_ascii_lowercase().starts_with("proxy-authorization:")
                            && l.contains("Basic cHJveHl1c2VyOnByb3h5cGFzcw==")
                    });
                    if !ok {
                        let _ = s
                            .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic realm=\"p\"\r\nContent-Length: 0\r\n\r\n")
                            .await;
                        return;
                    }
                }
                let first = req.lines().next().unwrap_or("");
                let mut parts = first.split_whitespace();
                let _m = parts.next();
                let target = parts.next().unwrap_or("");
                let Ok(u) = url::Url::parse(target) else {
                    let _ = s
                        .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n")
                        .await;
                    return;
                };
                let host = u.host_str().unwrap_or("127.0.0.1").to_string();
                let oport = u.port_or_known_default().unwrap_or(80);
                let path = if u.path().is_empty() {
                    "/".to_string()
                } else {
                    u.path().to_string()
                };
                let path = if let Some(q) = u.query() {
                    format!("{path}?{q}")
                } else {
                    path
                };
                let Ok(mut origin) = tokio::net::TcpStream::connect((host.as_str(), oport)).await
                else {
                    let _ = s
                        .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
                        .await;
                    return;
                };
                let mut out = format!(
                    "GET {path} HTTP/1.1\r\nHost: {host}:{oport}\r\nX-Aria-Proxy: 1\r\nConnection: close\r\n"
                );
                for line in req.lines().skip(1) {
                    let l = line.to_ascii_lowercase();
                    if l.is_empty() {
                        break;
                    }
                    if l.starts_with("host:")
                        || l.starts_with("proxy-authorization:")
                        || l.starts_with("proxy-connection:")
                    {
                        continue;
                    }
                    out.push_str(line);
                    out.push_str("\r\n");
                }
                out.push_str("\r\n");
                if origin.write_all(out.as_bytes()).await.is_err() {
                    return;
                }
                let _ = tokio::io::copy(&mut origin, &mut s).await;
            });
        }
    });
    port
}

async fn spawn_blackhole_proxy() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 1024];
                let _ = s.read(&mut buf).await;
                let _ = s
                    .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
                    .await;
            });
        }
    });
    port
}

#[tokio::test]
async fn http_proxy_dest_match() {
    let body: &'static [u8] = b"via-http-proxy-payload";
    let origin = spawn_proxy_origin(body).await;
    let proxy = spawn_forward_proxy(false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("p.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("http-proxy", format!("http://127.0.0.1:{proxy}"));
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{origin}/p.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

async fn spawn_connect_proxy() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = s.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    return;
                }
                let req = String::from_utf8_lossy(&buf[..n]);
                let first = req.lines().next().unwrap_or("");
                if !first.to_ascii_uppercase().starts_with("CONNECT ") {
                    let _ = s
                        .write_all(b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\n\r\n")
                        .await;
                    return;
                }
                let mut parts = first.split_whitespace();
                let _ = parts.next();
                let target = parts.next().unwrap_or("");
                let Some((host, p)) = target.rsplit_once(':') else {
                    let _ = s
                        .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n")
                        .await;
                    return;
                };
                let Ok(oport) = p.parse::<u16>() else {
                    return;
                };
                let Ok(mut origin) = tokio::net::TcpStream::connect((host, oport)).await else {
                    let _ = s
                        .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
                        .await;
                    return;
                };
                if s.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                    .await
                    .is_err()
                {
                    return;
                }
                let _ = tokio::io::copy_bidirectional(&mut s, &mut origin).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn proxy_method_tunnel_dest_match() {
    let body: &'static [u8] = b"via-tunnel-proxy-payload";
    let origin = spawn_static(body, "t.bin").await;
    let proxy = spawn_connect_proxy().await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("t.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("http-proxy", format!("http://127.0.0.1:{proxy}"));
    opts.set("proxy-method", "tunnel");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{origin}/t.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    aria2_rust::sockopt::reset_send();
    aria2_rust::sockopt::reset_recv();
    http::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--proxy-method=tunnel must dest-match via CONNECT"
    );
    assert_eq!(http::last_proxy_method(), "tunnel");
    assert!(
        aria2_rust::sockopt::last_send() >= 1,
        "C++ SocketCore::writeData send must write CONNECT/GET"
    );
    assert!(
        aria2_rust::sockopt::last_recv() >= 1,
        "C++ SocketCore::readData recv must read CONNECT/GET"
    );
}

#[tokio::test]
async fn proxy_method_get_dest_match() {
    let body: &'static [u8] = b"via-get-proxy-payload";
    let origin = spawn_proxy_origin(body).await;
    let proxy = spawn_forward_proxy(false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("g.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("http-proxy", format!("http://127.0.0.1:{proxy}"));
    opts.set("proxy-method", "get");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{origin}/g.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert_eq!(http::last_proxy_method(), "get");
}

#[tokio::test]
async fn proxy_method_get_misses_connect_only_proxy() {
    let body: &'static [u8] = b"via-tunnel-proxy-payload";
    let origin = spawn_static(body, "m.bin").await;
    let proxy = spawn_connect_proxy().await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("m.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("http-proxy", format!("http://127.0.0.1:{proxy}"));
    opts.set("proxy-method", "get");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{origin}/m.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    assert!(
        http::download(job).await.is_err(),
        "--proxy-method=get must not dest-match a CONNECT-only proxy"
    );
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "must not dest-match when GET hits CONNECT-only proxy"
    );
}

#[tokio::test]
async fn http_proxy_required_without_proxy_fails() {
    let body: &'static [u8] = b"via-http-proxy-payload";
    let origin = spawn_proxy_origin(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("np.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{origin}/np.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    assert!(http::download(job).await.is_err());
}

#[tokio::test]
async fn all_proxy_dest_match() {
    let body: &'static [u8] = b"via-all-proxy-payload";
    let origin = spawn_proxy_origin(body).await;
    let proxy = spawn_forward_proxy(false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("a.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("all-proxy", format!("127.0.0.1:{proxy}"));
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{origin}/a.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn all_proxy_user_passwd_dest_match() {
    let body: &'static [u8] = b"via-all-proxy-user-payload";
    let origin = spawn_proxy_origin(body).await;
    let proxy = spawn_forward_proxy(true).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("apu.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("all-proxy", format!("127.0.0.1:{proxy}"));
    opts.set("all-proxy-user", "proxyuser");
    opts.set("all-proxy-passwd", "proxypass");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{origin}/apu.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--all-proxy-user/--all-proxy-passwd must dest-match through auth proxy"
    );
}

#[tokio::test]
async fn all_proxy_wrong_passwd_no_dest() {
    let body: &'static [u8] = b"via-all-proxy-user-payload";
    let origin = spawn_proxy_origin(body).await;
    let proxy = spawn_forward_proxy(true).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("bad.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("all-proxy", format!("127.0.0.1:{proxy}"));
    opts.set("all-proxy-user", "proxyuser");
    opts.set("all-proxy-passwd", "wrong");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{origin}/bad.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    assert!(
        http::download(job).await.is_err(),
        "wrong --all-proxy-passwd must fail auth proxy"
    );
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "wrong --all-proxy-passwd must not dest-match"
    );
}

#[tokio::test]
async fn http_proxy_user_dest_match() {
    let body: &'static [u8] = b"via-auth-proxy-payload";
    let origin = spawn_proxy_origin(body).await;
    let proxy = spawn_forward_proxy(true).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("u.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("http-proxy", format!("http://127.0.0.1:{proxy}"));
    opts.set("http-proxy-user", "proxyuser");
    opts.set("http-proxy-passwd", "proxypass");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{origin}/u.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn no_proxy_skips_blackhole_dest_match() {
    let body: &'static [u8] = b"direct-no-proxy-payload";
    let origin = spawn_static(body, "d.bin").await;
    let proxy = spawn_blackhole_proxy().await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("d.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("http-proxy", format!("http://127.0.0.1:{proxy}"));
    opts.set("no-proxy", "127.0.0.1,localhost");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{origin}/d.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

async fn spawn_head_gate(body: &'static [u8]) -> u16 {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen_head = Arc::new(AtomicBool::new(false));
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            let seen_head = seen_head.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                if req.starts_with("HEAD ") {
                    seen_head.store(true, Ordering::SeqCst);
                    let hdr = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n\r\n",
                        body.len()
                    );
                    let _ = s.write_all(hdr.as_bytes()).await;
                    return;
                }
                if !seen_head.load(Ordering::SeqCst) {
                    let _ = s
                        .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")
                        .await;
                    return;
                }
                let range = req.lines().find(|l| l.to_ascii_lowercase().starts_with("range:"));
                if let Some(r) = range {
                    let spec = r.split(':').nth(1).unwrap_or("").trim();
                    let spec = spec.trim_start_matches("bytes=");
                    let mut parts = spec.split('-');
                    let start: usize = parts.next().unwrap_or("0").parse().unwrap_or(0);
                    let end: usize = parts
                        .next()
                        .filter(|s| !s.is_empty())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(body.len() - 1);
                    let end = end.min(body.len() - 1);
                    let slice = &body[start..=end];
                    let hdr = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nAccept-Ranges: bytes\r\n\r\n",
                        slice.len(),
                        body.len()
                    );
                    let _ = s.write_all(hdr.as_bytes()).await;
                    let _ = s.write_all(slice).await;
                    return;
                }
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(body).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn use_head_dest_match() {
    let body: &'static [u8] = b"use-head-payload-bytes";
    let port = spawn_head_gate(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("h.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("use-head", "true");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/h.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn use_head_false_rejected_without_head() {
    let body: &'static [u8] = b"use-head-payload-bytes";
    let port = spawn_head_gate(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("noh.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("use-head", "false");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/noh.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    assert!(http::download(job).await.is_err());
}

async fn spawn_cookie_gate(body: &'static [u8], set_cookie: bool) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                if !set_cookie {
                    let ok = req.lines().any(|l| {
                        l.to_ascii_lowercase().starts_with("cookie:") && l.contains("session=s3cret")
                    });
                    if !ok {
                        let _ = s
                            .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")
                            .await;
                        return;
                    }
                }
                let extra = if set_cookie {
                    "Set-Cookie: session=s3cret; Path=/\r\n"
                } else {
                    ""
                };
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n{extra}\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(body).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn load_cookies_dest_match() {
    let body: &'static [u8] = b"cookie-payload-bytes";
    let port = spawn_cookie_gate(body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let cookie_path = dir.path().join("cookies.txt");
    std::fs::write(
        &cookie_path,
        "127.0.0.1\tFALSE\t/\tFALSE\t0\tsession\ts3cret\n",
    )
    .unwrap();
    let dest = dir.path().join("c.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("load-cookies", cookie_path.display().to_string());
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/c.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn load_cookies_missing_rejected() {
    let body: &'static [u8] = b"cookie-payload-bytes";
    let port = spawn_cookie_gate(body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("n.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/n.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    assert!(http::download(job).await.is_err());
}

#[tokio::test]
async fn save_cookies_after_dest_match() {
    let body: &'static [u8] = b"set-cookie-payload";
    let port = spawn_cookie_gate(body, true).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("s.bin");
    let saved = dir.path().join("out-cookies.txt");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("save-cookies", saved.display().to_string());
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/s.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    let text = std::fs::read_to_string(&saved).unwrap();
    assert!(text.contains("session"), "save-cookies must keep name: {text}");
    assert!(text.contains("s3cret"), "save-cookies must keep value: {text}");
}

async fn spawn_auth_challenge(body: &'static [u8]) -> u16 {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let challenged = Arc::new(AtomicBool::new(false));
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            let challenged = challenged.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let has_auth = req.lines().any(|l| {
                    l.to_ascii_lowercase().starts_with("authorization:") && l.contains("Basic ")
                });
                if has_auth && !challenged.load(Ordering::SeqCst) {
                    let _ = s
                        .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")
                        .await;
                    return;
                }
                if !has_auth {
                    challenged.store(true, Ordering::SeqCst);
                    let _ = s
                        .write_all(b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"r\"\r\nContent-Length: 0\r\n\r\n")
                        .await;
                    return;
                }
                let range = req.lines().find(|l| l.to_ascii_lowercase().starts_with("range:"));
                if let Some(r) = range {
                    let spec = r.split(':').nth(1).unwrap_or("").trim();
                    let spec = spec.trim_start_matches("bytes=");
                    let mut parts = spec.split('-');
                    let start: usize = parts.next().unwrap_or("0").parse().unwrap_or(0);
                    let end: usize = parts
                        .next()
                        .filter(|s| !s.is_empty())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(body.len() - 1);
                    let end = end.min(body.len() - 1);
                    let slice = &body[start..=end];
                    let hdr = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nAccept-Ranges: bytes\r\n\r\n",
                        slice.len(),
                        body.len()
                    );
                    let _ = s.write_all(hdr.as_bytes()).await;
                    let _ = s.write_all(slice).await;
                    return;
                }
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(body).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn http_auth_challenge_dest_match() {
    let body: &'static [u8] = b"auth-challenge-payload";
    let port = spawn_auth_challenge(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("c.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("http-user", "alice");
    opts.set("http-passwd", "secret");
    opts.set("http-auth-challenge", "true");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/c.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn http_auth_challenge_false_preemptive_rejected() {
    let body: &'static [u8] = b"auth-challenge-payload";
    let port = spawn_auth_challenge(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("p.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("http-user", "alice");
    opts.set("http-passwd", "secret");
    opts.set("http-auth-challenge", "false");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/p.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    assert!(http::download(job).await.is_err());
}

async fn spawn_cd(body: &'static [u8], cd: &'static [u8]) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let range = req.lines().find(|l| l.to_ascii_lowercase().starts_with("range:"));
                let mut hdr = Vec::new();
                if let Some(r) = range {
                    let spec = r.split(':').nth(1).unwrap_or("").trim();
                    let spec = spec.trim_start_matches("bytes=");
                    let mut parts = spec.split('-');
                    let start: usize = parts.next().unwrap_or("0").parse().unwrap_or(0);
                    let end: usize = parts
                        .next()
                        .filter(|s| !s.is_empty())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(body.len() - 1);
                    let end = end.min(body.len() - 1);
                    let slice = &body[start..=end];
                    hdr.extend_from_slice(
                        format!(
                            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nAccept-Ranges: bytes\r\nContent-Disposition: ",
                            slice.len(),
                            body.len()
                        )
                        .as_bytes(),
                    );
                    hdr.extend_from_slice(cd);
                    hdr.extend_from_slice(b"\r\n\r\n");
                    let _ = s.write_all(&hdr).await;
                    let _ = s.write_all(slice).await;
                    return;
                }
                hdr.extend_from_slice(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nContent-Disposition: ",
                        body.len()
                    )
                    .as_bytes(),
                );
                hdr.extend_from_slice(cd);
                hdr.extend_from_slice(b"\r\n\r\n");
                let _ = s.write_all(&hdr).await;
                let _ = s.write_all(body).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn content_disposition_default_utf8_dest_match() {
    let body: &'static [u8] = b"cd-utf8-payload";
    let mut cd = b"attachment; filename=\"".to_vec();
    cd.extend_from_slice("café.bin".as_bytes());
    cd.extend_from_slice(b"\"");
    let cd: &'static [u8] = Box::leak(cd.into_boxed_slice());
    let port = spawn_cd(body, cd).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("dir", dir.path().display().to_string());
    opts.set("content-disposition-default-utf8", "true");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/ignored.bin")],
        dest: dir.path().join("ignored.bin"),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    let dest = dir.path().join("café.bin");
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn content_disposition_latin1_filename_dest_match() {
    let body: &'static [u8] = b"cd-latin1-payload";
    let mut cd = b"attachment; filename=\"".to_vec();
    cd.extend_from_slice("café.bin".as_bytes());
    cd.extend_from_slice(b"\"");
    let cd: &'static [u8] = Box::leak(cd.into_boxed_slice());
    let port = spawn_cd(body, cd).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("dir", dir.path().display().to_string());
    opts.set("content-disposition-default-utf8", "false");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/ignored.bin")],
        dest: dir.path().join("ignored.bin"),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    let dest = dir.path().join("cafÃ©.bin");
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn content_disposition_filename_star_dest_match() {
    let body: &'static [u8] = b"cd-star-payload";
    let port = spawn_cd(body, b"attachment; filename*=UTF-8''caf%C3%A9.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("dir", dir.path().display().to_string());
    opts.set("content-disposition-default-utf8", "false");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/ignored.bin")],
        dest: dir.path().join("ignored.bin"),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    let dest = dir.path().join("café.bin");
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

async fn spawn_want_digest(body: &'static [u8], require: bool) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_ascii_lowercase();
                let has = req.lines().any(|l| l.starts_with("want-digest:"));
                let ok = if require { has } else { !has };
                if !ok {
                    let _ = s
                        .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")
                        .await;
                    return;
                }
                let range = String::from_utf8_lossy(&buf[..n])
                    .lines()
                    .find(|l| l.to_ascii_lowercase().starts_with("range:"))
                    .map(|s| s.to_string());
                if let Some(r) = range {
                    let spec = r.split(':').nth(1).unwrap_or("").trim();
                    let spec = spec.trim_start_matches("bytes=");
                    let mut parts = spec.split('-');
                    let start: usize = parts.next().unwrap_or("0").parse().unwrap_or(0);
                    let end: usize = parts
                        .next()
                        .filter(|s| !s.is_empty())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(body.len() - 1);
                    let end = end.min(body.len().saturating_sub(1));
                    let slice = &body[start..=end];
                    let hdr = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nAccept-Ranges: bytes\r\n\r\n",
                        slice.len(),
                        body.len()
                    );
                    let _ = s.write_all(hdr.as_bytes()).await;
                    let _ = s.write_all(slice).await;
                    return;
                }
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(body).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn want_digest_sent_by_default_dest_match() {
    let body: &'static [u8] = b"want-digest-payload";
    let port = spawn_want_digest(body, true).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("wd.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/wd.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn no_want_digest_header_dest_match() {
    let body: &'static [u8] = b"no-want-digest-payload";
    let port = spawn_want_digest(body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("nwd.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("no-want-digest-header", "true");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/nwd.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn no_want_digest_header_false_rejected() {
    let body: &'static [u8] = b"no-want-digest-payload";
    let port = spawn_want_digest(body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("rej.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("no-want-digest-header", "false");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/rej.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    assert!(http::download(job).await.is_err());
}

#[tokio::test]
async fn auto_file_renaming_dest_match() {
    let body: &'static [u8] = b"auto-rename-new-payload";
    let port = spawn_static(body, "keep.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("keep.bin");
    std::fs::write(&dest, b"OLD-KEEP-BYTES").unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("auto-file-renaming", "true");
    opts.set("allow-overwrite", "false");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/keep.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), b"OLD-KEEP-BYTES");
    let renamed = dir.path().join("keep.1.bin");
    assert_eq!(std::fs::read(&renamed).unwrap(), body);
}

#[tokio::test]
async fn allow_overwrite_dest_match() {
    let body: &'static [u8] = b"overwrite-new-payload!!";
    let port = spawn_static(body, "ow.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("ow.bin");
    std::fs::write(&dest, b"OLD-OVERWRITE").unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("auto-file-renaming", "false");
    opts.set("allow-overwrite", "true");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/ow.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn no_overwrite_no_rename_keeps_old() {
    let body: &'static [u8] = b"should-not-write-this";
    let port = spawn_static(body, "no.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("no.bin");
    std::fs::write(&dest, b"OLD-PRESERVED").unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("auto-file-renaming", "false");
    opts.set("allow-overwrite", "false");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/no.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    assert!(http::download(job).await.is_err());
    assert_eq!(std::fs::read(&dest).unwrap(), b"OLD-PRESERVED");
}

#[tokio::test]
async fn no_overwrite_true_keeps_existing() {
    let body: &'static [u8] = b"no-overwrite-new-bytes";
    let port = spawn_static(body, "now.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("now.bin");
    std::fs::write(&dest, b"OLD-NO-OVERWRITE").unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("auto-file-renaming", "false");
    opts.set("allow-overwrite", "true");
    opts.set("no-overwrite", "true");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/now.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        b"OLD-NO-OVERWRITE",
        "--no-overwrite=true must keep existing dest even if allow-overwrite"
    );
}

#[tokio::test]
async fn no_overwrite_true_missing_dest_match() {
    let body: &'static [u8] = b"no-overwrite-fresh-bytes";
    let port = spawn_static(body, "nowok.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("nowok.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("no-overwrite", "true");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/nowok.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--no-overwrite=true must dest-match when dest is missing"
    );
}

const LM: &str = "Wed, 21 Oct 2015 07:28:00 GMT";

async fn spawn_last_modified(body: &'static [u8]) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let ims = req
                    .lines()
                    .find(|l| l.to_ascii_lowercase().starts_with("if-modified-since:"))
                    .and_then(|l| l.split_once(':').map(|x| x.1.trim().to_string()));
                if let Some(ims) = ims {
                    if let (Some(got), Some(lm)) = (
                        aria2_rust::http::parse_http_date(&ims),
                        aria2_rust::http::parse_http_date(LM),
                    ) {
                        if got >= lm {
                            let _ = s
                                .write_all(b"HTTP/1.1 304 Not Modified\r\nLast-Modified: Wed, 21 Oct 2015 07:28:00 GMT\r\nConnection: close\r\n\r\n")
                                .await;
                            return;
                        }
                    }
                }
                let range = req.lines().find(|l| l.to_ascii_lowercase().starts_with("range:"));
                if let Some(r) = range {
                    let spec = r.split(':').nth(1).unwrap_or("").trim();
                    let spec = spec.trim_start_matches("bytes=");
                    let mut parts = spec.split('-');
                    let start: usize = parts.next().unwrap_or("0").parse().unwrap_or(0);
                    let end: usize = parts
                        .next()
                        .filter(|s| !s.is_empty())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(body.len() - 1);
                    let end = end.min(body.len() - 1);
                    let slice = &body[start..=end];
                    let hdr = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nAccept-Ranges: bytes\r\nLast-Modified: {LM}\r\nConnection: close\r\n\r\n",
                        slice.len(),
                        body.len()
                    );
                    let _ = s.write_all(hdr.as_bytes()).await;
                    let _ = s.write_all(slice).await;
                    return;
                }
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nLast-Modified: {LM}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(body).await;
            });
        }
    });
    port
}

fn set_mtime(path: &std::path::Path, unix: u64) {
    let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(unix);
    let f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    f.set_modified(t).unwrap();
}

fn mtime_unix(path: &std::path::Path) -> u64 {
    std::fs::metadata(path)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[tokio::test]
async fn conditional_get_304_keeps_dest() {
    let body: &'static [u8] = b"conditional-new-payload";
    let port = spawn_last_modified(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("cg.bin");
    std::fs::write(&dest, b"OLD-CONDITIONAL").unwrap();
    set_mtime(&dest, 1_445_412_480);
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("conditional-get", "true");
    opts.set("auto-file-renaming", "false");
    opts.set("allow-overwrite", "true");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/cg.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), b"OLD-CONDITIONAL");
}

#[tokio::test]
async fn conditional_get_stale_dest_match() {
    let body: &'static [u8] = b"conditional-new-payload";
    let port = spawn_last_modified(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("stale.bin");
    std::fs::write(&dest, b"OLD-STALE").unwrap();
    set_mtime(&dest, 1_000_000_000);
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("conditional-get", "true");
    opts.set("auto-file-renaming", "false");
    opts.set("allow-overwrite", "true");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/stale.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn remote_time_sets_dest_mtime() {
    let body: &'static [u8] = b"remote-time-payload-bytes";
    let port = spawn_last_modified(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("rt.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("remote-time", "true");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/rt.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert_eq!(mtime_unix(&dest), 1_445_412_480);
}

async fn spawn_no_range(body: &'static [u8]) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let _ = n;
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(body).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn always_resume_true_cannot_resume_keeps_prefix() {
    let body: &'static [u8] = b"always-resume-full-payload-bytes";
    let port = spawn_no_range(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("ar.bin");
    std::fs::write(&dest, &body[..8]).unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("continue", "true");
    opts.set("always-resume", "true");
    opts.set("auto-file-renaming", "false");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/ar.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    assert!(http::download(job).await.is_err());
    assert_eq!(std::fs::read(&dest).unwrap(), &body[..8]);
}

#[tokio::test]
async fn always_resume_false_scratch_dest_match() {
    let body: &'static [u8] = b"always-resume-full-payload-bytes";
    let port = spawn_no_range(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("sc.bin");
    std::fs::write(&dest, &body[..8]).unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("continue", "true");
    opts.set("always-resume", "false");
    opts.set("max-resume-failure-tries", "1");
    opts.set("auto-file-renaming", "false");
    opts.set("allow-overwrite", "true");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/sc.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn max_resume_failure_tries_zero_never_scratch() {
    let body: &'static [u8] = b"always-resume-full-payload-bytes";
    let port = spawn_no_range(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("n0.bin");
    std::fs::write(&dest, &body[..8]).unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("continue", "true");
    opts.set("always-resume", "false");
    opts.set("max-resume-failure-tries", "0");
    opts.set("auto-file-renaming", "false");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/n0.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    assert!(http::download(job).await.is_err());
    assert_eq!(std::fs::read(&dest).unwrap(), &body[..8]);
}

async fn spawn_only_name(name: &'static str, body: &'static [u8]) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let line = req.lines().next().unwrap_or("");
                if !line.contains(name) {
                    let _ = s.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
                    return;
                }
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(body).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn parameterized_uri_brace_fallback_dest_match() {
    let body: &'static [u8] = b"param-uri-brace-payload";
    let port = spawn_only_name("p.b.bin", body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("out.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("parameterized-uri", "true");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/p.{{a,b}}.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn parameterized_uri_false_literal_braces_fail() {
    let body: &'static [u8] = b"param-uri-brace-payload";
    let port = spawn_only_name("p.b.bin", body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("out.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("parameterized-uri", "false");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/p.{{a,b}}.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    assert!(http::download(job).await.is_err());
}

#[tokio::test]
async fn parameterized_uri_numeric_dest_match() {
    let body: &'static [u8] = b"param-uri-numeric-payload";
    let port = spawn_only_name("n02.bin", body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("num.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("parameterized-uri", "true");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/n[01-02].bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

async fn spawn_fail_then_ok(
    fail_n: u32,
    status: u16,
    body: &'static [u8],
) -> (u16, std::sync::Arc<std::sync::atomic::AtomicU32>) {
    use std::sync::atomic::{AtomicU32, Ordering};
    let hits = std::sync::Arc::new(AtomicU32::new(0));
    let h2 = hits.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            let hits = h2.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let _ = n;
                let n = hits.fetch_add(1, Ordering::SeqCst) + 1;
                if n <= fail_n {
                    let hdr = format!(
                        "HTTP/1.1 {status} Fail\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    );
                    let _ = s.write_all(hdr.as_bytes()).await;
                    return;
                }
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(body).await;
            });
        }
    });
    (port, hits)
}

#[tokio::test]
async fn max_tries_one_fails_on_503() {
    let body: &'static [u8] = b"retry-payload-bytes";
    let (port, hits) = spawn_fail_then_ok(2, 503, body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("t1.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("max-tries", "1");
    opts.set("retry-wait", "0");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/t1.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    assert!(http::download(job).await.is_err());
    assert!(!dest.exists() || std::fs::read(&dest).unwrap() != body);
    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn max_tries_retry_dest_match() {
    let body: &'static [u8] = b"retry-payload-bytes";
    let (port, _) = spawn_fail_then_ok(2, 503, body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("t3.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("max-tries", "3");
    opts.set("retry-wait", "0");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/t3.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn retry_wait_sleeps_between_tries() {
    let body: &'static [u8] = b"retry-wait-payload";
    let (port, _) = spawn_fail_then_ok(1, 503, body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("rw.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("max-tries", "3");
    opts.set("retry-wait", "1");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/rw.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    let t0 = std::time::Instant::now();
    http::download(job).await.unwrap();
    assert!(t0.elapsed() >= std::time::Duration::from_secs(1));
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn max_file_not_found_aborts_before_max_tries() {
    let body: &'static [u8] = b"never-served";
    let (port, hits) = spawn_fail_then_ok(99, 404, body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("fnf.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("max-tries", "5");
    opts.set("max-file-not-found", "2");
    opts.set("retry-wait", "0");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/fnf.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    assert!(http::download(job).await.is_err());
    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[tokio::test]
async fn max_download_limit_dest_match_is_throttled() {
    let body: &'static [u8] = Box::leak(vec![0xABu8; 4000].into_boxed_slice());
    let port = spawn_static(body, "lim.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("lim.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("max-download-limit", "2000");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/lim.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    let t0 = std::time::Instant::now();
    http::download(job).await.unwrap();
    assert!(t0.elapsed() >= std::time::Duration::from_millis(1500));
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

async fn spawn_drip(body: &'static [u8], delay_ms: u64) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                if req.lines().any(|l| l.to_ascii_lowercase().starts_with("range: bytes=0-0")) {
                    let hdr = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: 1\r\nContent-Range: bytes 0-0/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = s.write_all(hdr.as_bytes()).await;
                    let _ = s.write_all(&body[..1]).await;
                    return;
                }
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                for b in body {
                    let _ = s.write_all(&[*b]).await;
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                }
            });
        }
    });
    port
}

#[tokio::test]
async fn lowest_speed_limit_aborts_slow_stream() {
    let body: &'static [u8] = b"0123456789abcdef0123456789abcdef";
    let port = spawn_drip(body, 80).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("slow.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("lowest-speed-limit", "100");
    opts.set("max-tries", "1");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/slow.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    let err = http::download(job).await.unwrap_err();
    assert!(err.to_string().contains("lowest-speed-limit"), "{err}");
}

async fn spawn_basic(user: &'static str, pass: &'static str, body: &'static [u8]) -> u16 {
    let expect = format!(
        "Basic {}",
        aria2_rust::bt::b64_encode(format!("{user}:{pass}").as_bytes())
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            let expect = expect.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let ok = req.lines().any(|l| l.contains(&expect));
                if !ok {
                    let _ = s
                        .write_all(b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"r\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        .await;
                    return;
                }
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(body).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn netrc_path_dest_match() {
    let body: &'static [u8] = b"netrc-payload-bytes";
    let port = spawn_basic("alice", "secret", body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("nr.bin");
    let netrc = dir.path().join("netrc");
    std::fs::write(
        &netrc,
        format!("machine 127.0.0.1 login alice password secret\n"),
    )
    .unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("netrc-path", netrc.to_string_lossy().into_owned());
    opts.set("max-tries", "1");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/nr.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn no_netrc_skips_file_and_fails() {
    let body: &'static [u8] = b"netrc-payload-bytes";
    let port = spawn_basic("alice", "secret", body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("nr.bin");
    let netrc = dir.path().join("netrc");
    std::fs::write(
        &netrc,
        format!("machine 127.0.0.1 login alice password secret\n"),
    )
    .unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("netrc-path", netrc.to_string_lossy().into_owned());
    opts.set("no-netrc", "true");
    opts.set("max-tries", "1");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/nr.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    assert!(http::download(job).await.is_err());
}

#[tokio::test]
async fn dry_run_does_not_write_dest() {
    let body: &'static [u8] = b"dry-run-must-not-land";
    let (port, hits) = spawn_fail_then_ok(0, 200, body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("dry.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("dry-run", "true");
    let progress = HttpProgress::new();
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/dry.bin")],
        dest: dest.clone(),
        opts,
        progress: progress.clone(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert!(!dest.exists(), "dry-run must not create dest");
    assert_eq!(
        progress.total.load(std::sync::atomic::Ordering::Relaxed),
        body.len() as u64
    );
    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn dry_run_false_writes_dest_match() {
    let body: &'static [u8] = b"dry-run-must-not-land";
    let port = spawn_static(body, "dry.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("dry.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("dry-run", "false");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/dry.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn reuse_uri_true_recycles_after_unused_fail_dest_match() {
    let body: &'static [u8] = b"reuse-uri-payload-bytes";
    let (port_a, _) = spawn_fail_then_ok(1, 503, body).await;
    let (port_b, _) = spawn_fail_then_ok(99, 404, body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("ru.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("reuse-uri", "true");
    opts.set("max-tries", "1");
    opts.set("retry-wait", "0");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![
            format!("http://127.0.0.1:{port_a}/a.bin"),
            format!("http://127.0.0.1:{port_b}/b.bin"),
        ],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn reuse_uri_false_does_not_recycle_used() {
    let body: &'static [u8] = b"reuse-uri-payload-bytes";
    let (port_a, hits_a) = spawn_fail_then_ok(1, 503, body).await;
    let (port_b, _) = spawn_fail_then_ok(99, 404, body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("ru.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("reuse-uri", "false");
    opts.set("max-tries", "1");
    opts.set("retry-wait", "0");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![
            format!("http://127.0.0.1:{port_a}/a.bin"),
            format!("http://127.0.0.1:{port_b}/b.bin"),
        ],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    assert!(http::download(job).await.is_err());
    assert_eq!(hits_a.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn uri_selector_feedback_skips_error_host_dest_match() {
    let body: &'static [u8] = b"feedback-selector-payload";
    let (port_bad, hits_bad) = spawn_fail_then_ok(99, 503, body).await;
    let (port_good, _) = spawn_fail_then_ok(0, 200, body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("fb.bin");
    let if_path = dir.path().join("stat.in");
    let of_path = dir.path().join("stat.out");
    std::fs::write(
        &if_path,
        format!(
            "host=127.0.0.1, protocol=http, dl_speed=0, status=ERROR\nhost=localhost, protocol=http, dl_speed=9000, status=OK\n"
        ),
    )
    .unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("uri-selector", "feedback");
    opts.set("server-stat-if", if_path.to_string_lossy().into_owned());
    opts.set("server-stat-of", of_path.to_string_lossy().into_owned());
    opts.set("max-tries", "1");
    opts.set("reuse-uri", "false");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![
            format!("http://127.0.0.1:{port_bad}/bad.bin"),
            format!("http://localhost:{port_good}/good.bin"),
        ],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert_eq!(hits_bad.load(std::sync::atomic::Ordering::SeqCst), 0);
    let dumped = std::fs::read_to_string(&of_path).unwrap();
    assert!(dumped.contains("host=localhost"), "{dumped}");
    assert!(dumped.contains("status=OK"), "{dumped}");
}

#[tokio::test]
async fn uri_selector_default_is_feedback_skips_error_dest_match() {
    let body: &'static [u8] = b"default-feedback-selector-payload";
    let (port_bad, hits_bad) = spawn_fail_then_ok(99, 503, body).await;
    let (port_good, _) = spawn_fail_then_ok(0, 200, body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("def.bin");
    let if_path = dir.path().join("stat.in");
    std::fs::write(
        &if_path,
        "host=127.0.0.1, protocol=http, dl_speed=0, status=ERROR\nhost=localhost, protocol=http, dl_speed=9000, status=OK\n",
    )
    .unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.map.remove("uri-selector");
    opts.set("server-stat-if", if_path.to_string_lossy().into_owned());
    opts.set("max-tries", "1");
    opts.set("reuse-uri", "false");
    opts.set("select-least-used-host", "false");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![
            format!("http://127.0.0.1:{port_bad}/bad.bin"),
            format!("http://localhost:{port_good}/good.bin"),
        ],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert_eq!(
        hits_bad.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "C++ default --uri-selector=feedback must skip ERROR host"
    );
}

#[tokio::test]
async fn uri_selector_inorder_hits_first_uri() {
    let body: &'static [u8] = b"inorder-selector-payload";
    let (port_bad, hits_bad) = spawn_fail_then_ok(99, 503, body).await;
    let (port_good, _) = spawn_fail_then_ok(0, 200, body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("io.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("uri-selector", "inorder");
    opts.set("max-tries", "1");
    opts.set("reuse-uri", "false");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![
            format!("http://127.0.0.1:{port_bad}/bad.bin"),
            format!("http://127.0.0.1:{port_good}/good.bin"),
        ],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert!(hits_bad.load(std::sync::atomic::Ordering::SeqCst) >= 1);
}

#[tokio::test]
async fn uri_selector_adaptive_skips_error_host_dest_match() {
    let body: &'static [u8] = b"adaptive-error-skip-payload";
    let (port_bad, hits_bad) = spawn_fail_then_ok(99, 503, body).await;
    let (port_good, _) = spawn_fail_then_ok(0, 200, body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("ad.bin");
    let if_path = dir.path().join("stat.in");
    std::fs::write(
        &if_path,
        format!(
            "host=127.0.0.1, protocol=http, dl_speed=0, status=ERROR\nhost=localhost, protocol=http, dl_speed=9000, status=OK\n"
        ),
    )
    .unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("uri-selector", "adaptive");
    opts.set("server-stat-if", if_path.to_string_lossy().into_owned());
    opts.set("max-tries", "1");
    opts.set("reuse-uri", "false");
    opts.set("select-least-used-host", "false");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![
            format!("http://127.0.0.1:{port_bad}/bad.bin"),
            format!("http://localhost:{port_good}/good.bin"),
        ],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert_eq!(
        hits_bad.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "adaptive must skip ERROR host"
    );
}

#[tokio::test]
async fn uri_selector_adaptive_probes_untested_before_known_ok_dest_match() {
    let body: &'static [u8] = b"adaptive-probe-untested-payload";
    let (port_known, hits_known) = spawn_fail_then_ok(99, 503, body).await;
    let (port_fresh, _) = spawn_fail_then_ok(0, 200, body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("adp.bin");
    let if_path = dir.path().join("stat.in");
    std::fs::write(
        &if_path,
        "host=localhost, protocol=http, dl_speed=9000, status=OK\n",
    )
    .unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("uri-selector", "adaptive");
    opts.set("server-stat-if", if_path.to_string_lossy().into_owned());
    opts.set("max-tries", "1");
    opts.set("reuse-uri", "false");
    opts.set("select-least-used-host", "false");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![
            format!("http://localhost:{port_known}/known.bin"),
            format!("http://127.0.0.1:{port_fresh}/fresh.bin"),
        ],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert_eq!(
        hits_known.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "adaptive must probe untested host before known OK"
    );
}

#[tokio::test]
async fn server_stat_timeout_ignores_stale_error_dest_match() {
    let body: &'static [u8] = b"stat-timeout-payload-bytes";
    let (port_fresh_err, hits_fresh) = spawn_fail_then_ok(99, 503, body).await;
    let (port_stale, _) = spawn_fail_then_ok(0, 200, body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("to.bin");
    let if_path = dir.path().join("stat.in");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    std::fs::write(
        &if_path,
        format!(
            "host=localhost, protocol=http, dl_speed=0, last_updated={now}, status=ERROR\nhost=127.0.0.1, protocol=http, dl_speed=0, last_updated=1, status=ERROR\n"
        ),
    )
    .unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("uri-selector", "feedback");
    opts.set("server-stat-if", if_path.to_string_lossy().into_owned());
    opts.set("server-stat-timeout", "60");
    opts.set("max-tries", "1");
    opts.set("reuse-uri", "false");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![
            format!("http://localhost:{port_fresh_err}/fresh.bin"),
            format!("http://127.0.0.1:{port_stale}/stale.bin"),
        ],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert_eq!(hits_fresh.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[tokio::test]
async fn select_least_used_host_skips_busy_dest_match() {
    let slow: &'static [u8] = Box::leak(vec![0xABu8; 256].into_boxed_slice());
    let body: &'static [u8] = b"least-used-host-payload";
    let port_busy = spawn_drip(slow, 50).await;
    let (port_busy_fail, hits_busy) = spawn_fail_then_ok(99, 503, body).await;
    let (port_free, _) = spawn_fail_then_ok(0, 200, body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest_a = dir.path().join("busy.bin");
    let dest_b = dir.path().join("free.bin");
    let (tx_a, rx_a) = watch::channel(false);
    let mut opts_a = OptionSet::with_defaults();
    opts_a.set("split", "1");
    opts_a.set("file-allocation", "none");
    opts_a.set("select-least-used-host", "true");
    opts_a.set("uri-selector", "inorder");
    opts_a.set("disk-cache", "0");
    let dest_a2 = dest_a.clone();
    let job_a = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port_busy}/busy.bin")],
        dest: dest_a2,
        opts: opts_a,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx_a,
    };
    let ha = tokio::spawn(async move {
        let _ = http::download(job_a).await;
    });
    for _ in 0..80 {
        if dest_a.exists() {
            if let Ok(b) = std::fs::read(&dest_a) {
                if b.len() >= 2 && aria2_rust::server_stat::host_uses("127.0.0.1") > 0 {
                    break;
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    assert!(
        aria2_rust::server_stat::host_uses("127.0.0.1") > 0,
        "job_a must hold 127.0.0.1 before ranking job_b"
    );
    let mut opts_b = OptionSet::with_defaults();
    opts_b.set("split", "1");
    opts_b.set("file-allocation", "none");
    opts_b.set("select-least-used-host", "true");
    opts_b.set("uri-selector", "inorder");
    opts_b.set("max-tries", "1");
    opts_b.set("reuse-uri", "false");
    opts_b.set("disk-cache", "0");
    let (_tx, rx) = watch::channel(false);
    let job_b = HttpJob {
        uris: vec![
            format!("http://127.0.0.1:{port_busy_fail}/busy.bin"),
            format!("http://localhost:{port_free}/free.bin"),
        ],
        dest: dest_b.clone(),
        opts: opts_b,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job_b).await.unwrap();
    let _ = tx_a.send(true);
    let _ = ha.await;
    assert_eq!(std::fs::read(&dest_b).unwrap(), body);
    assert_eq!(hits_busy.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[tokio::test]
async fn select_least_used_host_false_hits_busy_first() {
    let body: &'static [u8] = b"least-used-off-payload";
    let (port_busy, hits_busy) = spawn_fail_then_ok(99, 503, body).await;
    let (port_free, _) = spawn_fail_then_ok(0, 200, body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("off.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("select-least-used-host", "false");
    opts.set("max-tries", "1");
    opts.set("reuse-uri", "false");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![
            format!("http://127.0.0.1:{port_busy}/busy.bin"),
            format!("http://localhost:{port_free}/free.bin"),
        ],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert!(hits_busy.load(std::sync::atomic::Ordering::SeqCst) >= 1);
}

async fn spawn_v6(body: &'static [u8]) -> u16 {
    let listener = TcpListener::bind("[::1]:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                if req.lines().any(|l| l.to_ascii_lowercase().starts_with("range: bytes=0-0")) {
                    let hdr = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: 1\r\nContent-Range: bytes 0-0/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = s.write_all(hdr.as_bytes()).await;
                    let _ = s.write_all(&body[..1.min(body.len())]).await;
                    return;
                }
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(body).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn disable_ipv6_false_ipv6_dest_match() {
    let body: &'static [u8] = b"ipv6-loopback-payload";
    let port = spawn_v6(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("v6.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("disable-ipv6", "false");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://[::1]:{port}/v6.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn disable_ipv6_true_rejects_ipv6() {
    let body: &'static [u8] = b"ipv6-should-not-land";
    let port = spawn_v6(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("v6no.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("disable-ipv6", "true");
    opts.set("max-tries", "1");
    opts.set("connect-timeout", "2");
    opts.set("timeout", "3");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://[::1]:{port}/v6no.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    let r = http::download(job).await;
    assert!(r.is_err(), "disable-ipv6 must not fetch [::1]");
    assert!(!dest.exists() || std::fs::read(&dest).unwrap() != body);
}

#[tokio::test]
async fn happy_eyeballs_localhost_ipv4_server_dest_match() {
    let body: &'static [u8] = b"happy-eyeballs-v4-fallback-payload";
    let port = spawn_static(body, "he4.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("he4.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("disable-ipv6", "false");
    http::reset_http_io();
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://localhost:{port}/he4.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "C++ Happy Eyeballs must dest-match via IPv4 when [::1] refuses"
    );
    assert_eq!(
        http::last_he_win(),
        4,
        "IPv4-only origin must win HE, got {}",
        http::last_he_win()
    );
    assert!(
        http::last_he_v6_try() >= 1,
        "HE must attempt AAAA first, v6_try={}",
        http::last_he_v6_try()
    );
}

#[tokio::test]
async fn happy_eyeballs_localhost_ipv6_server_dest_match() {
    let body: &'static [u8] = b"happy-eyeballs-v6-preferred-payload";
    let port = spawn_v6(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("he6.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("disable-ipv6", "false");
    http::reset_http_io();
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://localhost:{port}/he6.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "C++ Happy Eyeballs must dest-match via IPv6 when AAAA is live"
    );
    assert_eq!(
        http::last_he_win(),
        6,
        "live [::1] must win HE before A, got {}",
        http::last_he_win()
    );
}

#[tokio::test]
async fn interface_loopback_dest_match() {
    let body: &'static [u8] = b"interface-lo-payload";
    let port = spawn_static(body, "ifc.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("ifc.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("interface", "127.0.0.1");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/ifc.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn interface_unusable_fails() {
    let body: &'static [u8] = b"interface-bad-payload";
    let port = spawn_static(body, "bad.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("bad.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("interface", "192.0.2.1");
    opts.set("max-tries", "1");
    opts.set("connect-timeout", "2");
    opts.set("timeout", "3");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/bad.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    let r = http::download(job).await;
    assert!(r.is_err(), "unusable --interface must fail");
    assert!(!dest.exists() || std::fs::read(&dest).unwrap() != body);
}

#[tokio::test]
async fn multiple_interface_skips_unusable_dest_match() {
    let body: &'static [u8] = b"multi-ifc-payload";
    let port = spawn_static(body, "multi.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("multi.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("multiple-interface", "192.0.2.1,127.0.0.1");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/multi.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn control_file_sparse_resume_dest_match() {
    let body: &'static [u8] = Box::leak(
        (0u8..=255)
            .cycle()
            .take(8192)
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    let port = spawn_static(body, "cf.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("cf.bin");
    let mut stale = vec![0xFFu8; 4096];
    stale.extend_from_slice(&body[4096..]);
    std::fs::write(&dest, &stale).unwrap();
    let mut ctl = aria2_rust::control_file::Control::new(4096, 8192);
    ctl.set(1);
    ctl.save(&dest).unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("continue", "true");
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("piece-length", "4096");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/cf.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 4096,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert!(!aria2_rust::control_file::path_for(&dest).exists());
}

#[tokio::test]
async fn allow_piece_length_change_false_aborts() {
    let body: &'static [u8] = Box::leak(vec![0xABu8; 8192].into_boxed_slice());
    let port = spawn_static(body, "plc.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("plc.bin");
    std::fs::write(&dest, vec![0x11u8; 8192]).unwrap();
    let mut ctl = aria2_rust::control_file::Control::new(4096, 8192);
    ctl.set(0);
    ctl.save(&dest).unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("continue", "true");
    opts.set("allow-piece-length-change", "false");
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("piece-length", "2048");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/plc.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 2048,
        cancel: rx,
    };
    let err = http::download(job).await.unwrap_err();
    assert!(
        err.to_string().contains("piece length"),
        "must abort on control piece-length mismatch: {err}"
    );
    assert_eq!(std::fs::read(&dest).unwrap(), vec![0x11u8; 8192]);
}

#[tokio::test]
async fn allow_piece_length_change_true_dest_match() {
    let body: &'static [u8] = Box::leak(
        (0u8..=255)
            .cycle()
            .take(8192)
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    let port = spawn_static(body, "plct.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("plct.bin");
    std::fs::write(&dest, vec![0x11u8; 8192]).unwrap();
    let mut ctl = aria2_rust::control_file::Control::new(4096, 8192);
    ctl.set(0);
    ctl.save(&dest).unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("continue", "true");
    opts.set("allow-piece-length-change", "true");
    opts.set("allow-overwrite", "true");
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("piece-length", "2048");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/plct.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 2048,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn remove_control_file_refetch_dest_match() {
    let body: &'static [u8] = b"control-file-refetch-payload!!";
    let port = spawn_static(body, "rmcf.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("rmcf.bin");
    std::fs::write(&dest, vec![b'X'; body.len()]).unwrap();
    let mut ctl = aria2_rust::control_file::Control::new(1024 * 1024, body.len() as u64);
    ctl.set(0);
    ctl.save(&dest).unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("continue", "true");
    opts.set("remove-control-file", "true");
    opts.set("allow-overwrite", "true");
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/rmcf.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert!(!aria2_rust::control_file::path_for(&dest).exists());
}

async fn spawn_gated_http(body: &'static [u8], open: std::sync::Arc<std::sync::atomic::AtomicBool>, gate: std::sync::Arc<tokio::sync::Notify>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            let gate = std::sync::Arc::clone(&gate);
            let open = std::sync::Arc::clone(&open);
            tokio::spawn(async move {
                let mut buf = vec![0u8; 2048];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                if req.contains("bytes=0-0") {
                    let hdr = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: 1\r\nContent-Range: bytes 0-0/{}\r\nAccept-Ranges: bytes\r\n\r\n",
                        body.len()
                    );
                    let _ = s.write_all(hdr.as_bytes()).await;
                    let _ = s.write_all(&body[..1]).await;
                    return;
                }
                let range = req.lines().find(|l| l.to_ascii_lowercase().starts_with("range:"));
                let (start, end) = if let Some(r) = range {
                    let spec = r.split(':').nth(1).unwrap_or("").trim();
                    let spec = spec.trim_start_matches("bytes=");
                    let mut parts = spec.split('-');
                    let start: usize = parts.next().unwrap_or("0").parse().unwrap_or(0);
                    let end: usize = parts
                        .next()
                        .filter(|s| !s.is_empty())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(body.len() - 1);
                    (start.min(body.len()), end.min(body.len() - 1))
                } else {
                    (0, body.len() - 1)
                };
                let slice = &body[start..=end];
                let hdr = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nAccept-Ranges: bytes\r\n\r\n",
                    slice.len(),
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                if !open.load(std::sync::atomic::Ordering::SeqCst) && start >= 2048 {
                    while !open.load(std::sync::atomic::Ordering::SeqCst) {
                        gate.notified().await;
                    }
                }
                if open.load(std::sync::atomic::Ordering::SeqCst) || start < 2048 {
                    let _ = s.write_all(slice).await;
                    return;
                }
                let first = slice.len().min(2048);
                let _ = s.write_all(&slice[..first]).await;
                while !open.load(std::sync::atomic::Ordering::SeqCst) {
                    gate.notified().await;
                }
                let _ = s.write_all(&slice[first..]).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn auto_save_interval_zero_no_ctl_until_cancel_then_resume_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0xA1u8; 16 * 1024].into_boxed_slice());
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port = spawn_gated_http(body, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("asi0.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "asi0.bin");
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("timeout", "15");
    opts.set("auto-save-interval", "0");
    let (tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/asi0.bin")],
        dest: dest.clone(),
        opts: opts.clone(),
        progress: HttpProgress::new(),
        piece_length: 1024,
        cancel: rx,
    };
    let h = tokio::spawn(async move { http::download(job).await });
    for _ in 0..80 {
        if dest.exists() && std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0) >= 2048 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert!(
        !aria2_rust::control_file::path_for(&dest).exists(),
        "auto-save-interval=0 must not write .aria2 during download"
    );
    let _ = tx.send(true);
    let err = h.await.unwrap();
    assert!(err.is_err(), "canceled");
    assert!(
        aria2_rust::control_file::path_for(&dest).exists(),
        "control file must be saved on stop even when interval=0"
    );
    open.store(true, std::sync::atomic::Ordering::SeqCst);
    gate.notify_waiters();
    opts.set("continue", "true");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/asi0.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert!(!aria2_rust::control_file::path_for(&dest).exists());
}

#[tokio::test]
async fn auto_save_interval_one_writes_ctl_then_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0xA2u8; 16 * 1024].into_boxed_slice());
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port = spawn_gated_http(body, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("asi1.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "asi1.bin");
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("timeout", "15");
    opts.set("auto-save-interval", "1");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/asi1.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024,
        cancel: rx,
    };
    let h = tokio::spawn(async move { http::download(job).await });
    let t0 = tokio::time::Instant::now();
    let mut saw = false;
    while t0.elapsed() < std::time::Duration::from_secs(4) {
        if aria2_rust::control_file::path_for(&dest).exists() {
            saw = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(saw, "auto-save-interval=1 must write .aria2 during download");
    assert!(
        std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0) < body.len() as u64,
        "control save must happen before dest-match"
    );
    open.store(true, std::sync::atomic::Ordering::SeqCst);
    gate.notify_waiters();
    h.await.unwrap().unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert!(!aria2_rust::control_file::path_for(&dest).exists());
}

async fn spawn_dns(name: &'static str, ip: std::net::IpAddr) -> u16 {
    let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let port = sock.local_addr().unwrap().port();
    tokio::spawn(async move {
        let mut buf = [0u8; 512];
        loop {
            let Ok((n, peer)) = sock.recv_from(&mut buf).await else { break };
            let qname_ok = {
                let mut off = 12usize;
                let mut labels = Vec::new();
                if n < 12 {
                    false
                } else {
                    while off < n && buf[off] != 0 && buf[off] & 0xc0 != 0xc0 {
                        let len = buf[off] as usize;
                        off += 1;
                        if off + len > n {
                            break;
                        }
                        labels.push(String::from_utf8_lossy(&buf[off..off + len]).into_owned());
                        off += len;
                    }
                    labels.join(".").eq_ignore_ascii_case(name)
                }
            };
            if !qname_ok {
                continue;
            }
            if let Some(resp) = aria2_rust::dns::encode_reply(&buf[..n], ip) {
                let _ = sock.send_to(&resp, peer).await;
            }
        }
    });
    port
}

#[tokio::test]
async fn async_dns_server_a_dest_match() {
    let body: &'static [u8] = b"async-dns-a-payload";
    let http_port = spawn_static(body, "dns.bin").await;
    let dns_port = spawn_dns("aria2-async-dns.test", "127.0.0.1".parse().unwrap()).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("dns.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("async-dns-server", format!("127.0.0.1:{dns_port}"));
    opts.set("enable-async-dns6", "false");
    opts.set("async-dns", "true");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://aria2-async-dns.test:{http_port}/dns.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn enable_async_dns6_false_ignores_aaaa() {
    let body: &'static [u8] = b"dns6-should-not-land";
    let http_port = spawn_v6(body).await;
    let dns_port = spawn_dns("aria2-async-dns6.test", "::1".parse().unwrap()).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("dns6no.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("async-dns-server", format!("127.0.0.1:{dns_port}"));
    opts.set("enable-async-dns6", "false");
    opts.set("max-tries", "1");
    opts.set("dns-timeout", "1");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://aria2-async-dns6.test:{http_port}/dns6no.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    assert!(http::download(job).await.is_err());
    assert!(!dest.exists() || std::fs::read(&dest).unwrap_or_default() != body);
}

#[tokio::test]
async fn enable_async_dns6_true_aaaa_dest_match() {
    let body: &'static [u8] = b"async-dns-aaaa-payload";
    let http_port = spawn_v6(body).await;
    let dns_port = spawn_dns("aria2-async-dns6.test", "::1".parse().unwrap()).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("dns6.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("disable-ipv6", "false");
    opts.set("async-dns-server", format!("127.0.0.1:{dns_port}"));
    opts.set("enable-async-dns6", "true");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://aria2-async-dns6.test:{http_port}/dns6.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn dns_timeout_blackhole_fails() {
    let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let dns_port = sock.local_addr().unwrap().port();
    tokio::spawn(async move {
        let _sock = sock;
        std::future::pending::<()>().await;
    });
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("dnsto.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("async-dns-server", format!("127.0.0.1:{dns_port}"));
    opts.set("dns-timeout", "1");
    opts.set("max-tries", "1");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec!["http://aria2-dns-timeout.test/x.bin".into()],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    let t0 = std::time::Instant::now();
    assert!(http::download(job).await.is_err());
    assert!(t0.elapsed() >= std::time::Duration::from_secs(1));
    assert!(t0.elapsed() < std::time::Duration::from_secs(5));
}

#[tokio::test]
async fn async_dns_false_skips_custom_server() {
    let body: &'static [u8] = b"async-dns-false-payload";
    let http_port = spawn_static(body, "dnsf.bin").await;
    let dns_port = spawn_dns("aria2-async-dns-off.test", "127.0.0.1".parse().unwrap()).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("dnsf.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("async-dns", "false");
    opts.set("async-dns-server", format!("127.0.0.1:{dns_port}"));
    opts.set("max-tries", "1");
    opts.set("connect-timeout", "2");
    opts.set("timeout", "3");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://aria2-async-dns-off.test:{http_port}/dnsf.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    assert!(http::download(job).await.is_err());
    assert!(!dest.exists() || std::fs::read(&dest).unwrap_or_default() != body);
}

#[tokio::test]
async fn async_dns_false_ip_dest_match() {
    let body: &'static [u8] = b"async-dns-ip-payload";
    let port = spawn_static(body, "dnsi.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("dnsi.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("async-dns", "false");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/dnsi.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024 * 1024,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

async fn spawn_range_log(
    body: &'static [u8],
    log: std::sync::Arc<std::sync::Mutex<Vec<u64>>>,
) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            let log = std::sync::Arc::clone(&log);
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let range = req.lines().find(|l| l.to_ascii_lowercase().starts_with("range:"));
                if req.starts_with("HEAD ") {
                    let hdr = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = s.write_all(hdr.as_bytes()).await;
                    return;
                }
                if let Some(r) = range {
                    let spec = r.split(':').nth(1).unwrap_or("").trim();
                    let spec = spec.trim_start_matches("bytes=");
                    let mut parts = spec.split('-');
                    let start: usize = parts.next().unwrap_or("0").parse().unwrap_or(0);
                    let end: usize = parts
                        .next()
                        .filter(|x| !x.is_empty())
                        .and_then(|x| x.parse().ok())
                        .unwrap_or(body.len() - 1);
                    let end = end.min(body.len() - 1);
                    if !(start == 0 && end == 0) {
                        log.lock().unwrap().push(start as u64);
                    }
                    let slice = &body[start..=end];
                    let hdr = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                        slice.len(),
                        body.len()
                    );
                    let _ = s.write_all(hdr.as_bytes()).await;
                    let _ = s.write_all(slice).await;
                    return;
                }
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(body).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn stream_piece_selector_inorder_ranges_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0x5Au8; 16 * 1024].into_boxed_slice());
    let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let port = spawn_range_log(body, std::sync::Arc::clone(&log)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("inorder.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("max-connection-per-server", "1");
    opts.set("min-split-size", "1");
    opts.set("piece-length", "4096");
    opts.set("file-allocation", "none");
    opts.set("stream-piece-selector", "inorder");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/inorder.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 4096,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    let starts = log.lock().unwrap().clone();
    assert_eq!(
        starts,
        vec![0, 4096, 8192, 12288],
        "inorder must fetch min-index first: {starts:?}"
    );
}

#[tokio::test]
async fn stream_piece_selector_random_segmented_dest_match() {
    let body: &'static [u8] =
        Box::leak((0u8..=255).cycle().take(16 * 1024).collect::<Vec<_>>().into_boxed_slice());
    let port = spawn_static(body, "rand.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("rand.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "4");
    opts.set("max-connection-per-server", "4");
    opts.set("min-split-size", "1");
    opts.set("piece-length", "4096");
    opts.set("file-allocation", "none");
    opts.set("stream-piece-selector", "random");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/rand.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 4096,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn stream_piece_selector_geom_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0x3Cu8; 16 * 1024].into_boxed_slice());
    let port = spawn_static(body, "geom.bin").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("geom.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("min-split-size", "1");
    opts.set("piece-length", "4096");
    opts.set("file-allocation", "none");
    opts.set("stream-piece-selector", "geom");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/geom.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 4096,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn no_file_allocation_limit_skips_prealloc_then_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0xB2u8; 32 * 1024].into_boxed_slice());
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port = spawn_gated_http(body, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("nfal.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "trunc");
    opts.set("no-file-allocation-limit", "1M");
    opts.set("disk-cache", "0");
    opts.set("timeout", "15");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/nfal.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024,
        cancel: rx,
    };
    let h = tokio::spawn(async move { http::download(job).await });
    let t0 = tokio::time::Instant::now();
    loop {
        if dest.exists() {
            let n = std::fs::metadata(&dest).unwrap().len();
            if n > 0 {
                assert!(
                    n < body.len() as u64,
                    "small file must skip prealloc, got {n}"
                );
                break;
            }
        }
        if t0.elapsed() > std::time::Duration::from_secs(3) {
            panic!("never wrote prefix");
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    open.store(true, std::sync::atomic::Ordering::SeqCst);
    gate.notify_waiters();
    h.await.unwrap().unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn no_file_allocation_limit_zero_preallocs_then_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0xC3u8; 32 * 1024].into_boxed_slice());
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port = spawn_gated_http(body, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("nfal0.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "trunc");
    opts.set("no-file-allocation-limit", "0");
    opts.set("disk-cache", "0");
    opts.set("timeout", "15");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/nfal0.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024,
        cancel: rx,
    };
    let h = tokio::spawn(async move { http::download(job).await });
    let t0 = tokio::time::Instant::now();
    loop {
        if dest.exists() {
            let n = std::fs::metadata(&dest).unwrap().len();
            if n == body.len() as u64 {
                break;
            }
        }
        if t0.elapsed() > std::time::Duration::from_secs(3) {
            panic!(
                "limit=0 must prealloc, got {:?}",
                std::fs::metadata(&dest).map(|m| m.len()).ok()
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    open.store(true, std::sync::atomic::Ordering::SeqCst);
    gate.notify_waiters();
    h.await.unwrap().unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn file_allocation_falloc_then_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0xD4u8; 32 * 1024].into_boxed_slice());
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port = spawn_gated_http(body, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("falloc.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "falloc");
    opts.set("no-file-allocation-limit", "0");
    opts.set("disk-cache", "0");
    opts.set("timeout", "15");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/falloc.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024,
        cancel: rx,
    };
    let h = tokio::spawn(async move { http::download(job).await });
    let t0 = tokio::time::Instant::now();
    loop {
        if dest.exists() {
            let n = std::fs::metadata(&dest).unwrap().len();
            if n == body.len() as u64 {
                break;
            }
        }
        if t0.elapsed() > std::time::Duration::from_secs(3) {
            panic!(
                "falloc must pre-size dest, got {:?}",
                std::fs::metadata(&dest).map(|m| m.len()).ok()
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(
        aria2_rust::storage::last_alloc_kind(),
        "falloc",
        "--file-allocation=falloc must call posix_fallocate"
    );
    open.store(true, std::sync::atomic::Ordering::SeqCst);
    gate.notify_waiters();
    h.await.unwrap().unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn file_allocation_prealloc_zeros_then_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0xE5u8; 32 * 1024].into_boxed_slice());
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port = spawn_gated_http(body, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("prealloc.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "prealloc");
    opts.set("no-file-allocation-limit", "0");
    opts.set("disk-cache", "0");
    opts.set("timeout", "15");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/prealloc.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 1024,
        cancel: rx,
    };
    let h = tokio::spawn(async move { http::download(job).await });
    let t0 = tokio::time::Instant::now();
    loop {
        if dest.exists() {
            let n = std::fs::metadata(&dest).unwrap().len();
            if n == body.len() as u64 {
                let got = std::fs::read(&dest).unwrap();
                assert!(
                    got[2048..].iter().all(|b| *b == 0),
                    "--file-allocation=prealloc must write zeros beyond gated prefix"
                );
                break;
            }
        }
        if t0.elapsed() > std::time::Duration::from_secs(3) {
            panic!(
                "prealloc must write zeros, got {:?}",
                std::fs::metadata(&dest).map(|m| m.len()).ok()
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(aria2_rust::storage::last_alloc_kind(), "prealloc");
    open.store(true, std::sync::atomic::Ordering::SeqCst);
    gate.notify_waiters();
    h.await.unwrap().unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

async fn spawn_wait_n_gets(body: &'static [u8], n: usize) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut tmp = [0u8; 2048];
                loop {
                    let k = s.read(&mut tmp).await.unwrap_or(0);
                    if k == 0 {
                        return;
                    }
                    buf.extend_from_slice(&tmp[..k]);
                    let text = String::from_utf8_lossy(&buf);
                    let reqs: Vec<&str> = text
                        .split("\r\n\r\n")
                        .filter(|r| r.starts_with("GET "))
                        .collect();
                    if reqs.is_empty() {
                        continue;
                    }
                    let probe = reqs.iter().any(|r| r.to_ascii_lowercase().contains("bytes=0-0"));
                    let pieces: Vec<&str> = reqs
                        .iter()
                        .copied()
                        .filter(|r| !r.to_ascii_lowercase().contains("bytes=0-0"))
                        .collect();
                    if probe && pieces.is_empty() {
                        let hdr = format!(
                            "HTTP/1.1 206 Partial Content\r\nContent-Length: 1\r\nContent-Range: bytes 0-0/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = s.write_all(hdr.as_bytes()).await;
                        let _ = s.write_all(&body[..1]).await;
                        return;
                    }
                    if pieces.len() >= n {
                        for r in pieces {
                            let range = r
                                .lines()
                                .find(|l| l.to_ascii_lowercase().starts_with("range:"))
                                .and_then(|l| l.split(':').nth(1))
                                .unwrap_or("")
                                .trim();
                            let spec = range.strip_prefix("bytes=").unwrap_or(range);
                            let mut it = spec.split('-');
                            let start: usize = it.next().unwrap_or("0").parse().unwrap_or(0);
                            let end: usize = it
                                .next()
                                .unwrap_or("0")
                                .parse()
                                .unwrap_or(body.len().saturating_sub(1));
                            let end = end.min(body.len().saturating_sub(1));
                            let slice = &body[start..=end];
                            let hdr = format!(
                                "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nAccept-Ranges: bytes\r\nConnection: keep-alive\r\n\r\n",
                                slice.len(),
                                start,
                                end,
                                body.len()
                            );
                            let _ = s.write_all(hdr.as_bytes()).await;
                            let _ = s.write_all(slice).await;
                        }
                        return;
                    }
                }
            });
        }
    });
    port
}

#[tokio::test]
async fn enable_http_pipelining_two_gets_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0xE1u8; 8192].into_boxed_slice());
    let port = spawn_wait_n_gets(body, 2).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("pipe.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("enable-http-pipelining", "true");
    opts.set("max-http-pipelining", "2");
    opts.set("split", "2");
    opts.set("max-connection-per-server", "1");
    opts.set("min-split-size", "1");
    opts.set("piece-length", "4096");
    opts.set("file-allocation", "none");
    opts.set("timeout", "8");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/pipe.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 4096,
        cancel: rx,
    };
    aria2_rust::storage::reset_try_cache();
    aria2_rust::sockopt::reset_writev();
    aria2_rust::sockopt::reset_recv();
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert!(
        aria2_rust::storage::last_try_cache() > 0,
        "C++ SocketBuffer leftover/window must WrDiskCache without extra Vec"
    );
    assert_eq!(
        http::last_pipelined(),
        2,
        "must write 2 Range GETs before reading"
    );
    assert_eq!(
        http::last_pipe_writes(),
        1,
        "C++ SocketBuffer::writeBuffer: one send of the pipelined GET batch"
    );
    assert!(
        aria2_rust::sockopt::last_writev() >= 1,
        "C++ SocketCore::writeVector must writev the pipelined GETs"
    );
    assert!(
        aria2_rust::sockopt::last_nodelay(),
        "C++ SocketCore TCP_NODELAY must be on pipelined HTTP/1.1"
    );
    assert!(
        aria2_rust::sockopt::last_quickack(),
        "C++ SocketCore TCP_QUICKACK after recv on pipelined HTTP/1.1"
    );
    assert!(
        aria2_rust::sockopt::last_recv() >= 1,
        "C++ SocketCore::readData must recv() pipelined HTTP/1.1"
    );
}

/// C++ SocketBuffer: one TCP write of two 206 bodies; leftover holds next
/// headers+body; dest-match without per-chunk Vec collect.
async fn spawn_wait_n_gets_coalesced(body: &'static [u8], n: usize) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let _ = s.set_nodelay(true);
                let mut buf = Vec::new();
                let mut tmp = [0u8; 2048];
                loop {
                    let k = s.read(&mut tmp).await.unwrap_or(0);
                    if k == 0 {
                        return;
                    }
                    buf.extend_from_slice(&tmp[..k]);
                    let text = String::from_utf8_lossy(&buf);
                    let reqs: Vec<&str> = text
                        .split("\r\n\r\n")
                        .filter(|r| r.starts_with("GET "))
                        .collect();
                    let probe = reqs.iter().any(|r| r.to_ascii_lowercase().contains("bytes=0-0"));
                    let pieces: Vec<&str> = reqs
                        .iter()
                        .copied()
                        .filter(|r| !r.to_ascii_lowercase().contains("bytes=0-0"))
                        .collect();
                    if probe && pieces.is_empty() {
                        let hdr = format!(
                            "HTTP/1.1 206 Partial Content\r\nContent-Length: 1\r\nContent-Range: bytes 0-0/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = s.write_all(hdr.as_bytes()).await;
                        let _ = s.write_all(&body[..1]).await;
                        return;
                    }
                    if pieces.len() >= n {
                        let mut wire = Vec::new();
                        for r in pieces {
                            let range = r
                                .lines()
                                .find(|l| l.to_ascii_lowercase().starts_with("range:"))
                                .and_then(|l| l.split(':').nth(1))
                                .unwrap_or("")
                                .trim();
                            let spec = range.strip_prefix("bytes=").unwrap_or(range);
                            let mut it = spec.split('-');
                            let start: usize = it.next().unwrap_or("0").parse().unwrap_or(0);
                            let end: usize = it
                                .next()
                                .unwrap_or("0")
                                .parse()
                                .unwrap_or(body.len().saturating_sub(1));
                            let end = end.min(body.len().saturating_sub(1));
                            let slice = &body[start..=end];
                            let hdr = format!(
                                "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nAccept-Ranges: bytes\r\nConnection: keep-alive\r\n\r\n",
                                slice.len(),
                                start,
                                end,
                                body.len()
                            );
                            wire.extend_from_slice(hdr.as_bytes());
                            wire.extend_from_slice(slice);
                        }
                        let _ = s.write_all(&wire).await;
                        return;
                    }
                }
            });
        }
    });
    port
}

#[tokio::test]
async fn enable_http_pipelining_coalesced_socketbuffer_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0xE3u8; 8192].into_boxed_slice());
    let port = spawn_wait_n_gets_coalesced(body, 2).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("pipe-coal.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("enable-http-pipelining", "true");
    opts.set("max-http-pipelining", "2");
    opts.set("split", "2");
    opts.set("max-connection-per-server", "1");
    opts.set("min-split-size", "1");
    opts.set("piece-length", "4096");
    opts.set("file-allocation", "none");
    opts.set("disk-cache", "0");
    opts.set("timeout", "8");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/pipe-coal.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 4096,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert_eq!(http::last_pipelined(), 2);
    assert_eq!(
        http::last_pipe_writes(),
        1,
        "C++ SocketBuffer::writeBuffer: one send of two Range GETs"
    );
}

#[tokio::test]
async fn enable_http_pipelining_coalesced_three_socketbuffer_cursor_dest_match() {
    let body: &'static [u8] = Box::leak(
        (0..12 * 1024u32)
            .map(|i| (i % 251) as u8)
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    let port = spawn_wait_n_gets_coalesced(body, 3).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("pipe-coal3.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("enable-http-pipelining", "true");
    opts.set("max-http-pipelining", "3");
    opts.set("split", "3");
    opts.set("max-connection-per-server", "1");
    opts.set("min-split-size", "1");
    opts.set("piece-length", "4096");
    opts.set("file-allocation", "none");
    opts.set("disk-cache", "0");
    opts.set("timeout", "8");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/pipe-coal3.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 4096,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "SocketBuffer leftover cursor must dest-match 3 coalesced 206s"
    );
    assert_eq!(http::last_pipelined(), 3);
    assert_eq!(
        http::last_pipe_writes(),
        1,
        "C++ SocketBuffer: one write of 3 coalesced Range GETs"
    );
}

#[tokio::test]
async fn max_http_pipelining_one_cannot_fill_wait2_server() {
    let body: &'static [u8] = Box::leak(vec![0xE2u8; 8192].into_boxed_slice());
    let port = spawn_wait_n_gets(body, 2).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("pipe1.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("enable-http-pipelining", "true");
    opts.set("max-http-pipelining", "1");
    opts.set("split", "2");
    opts.set("max-connection-per-server", "1");
    opts.set("min-split-size", "1");
    opts.set("piece-length", "4096");
    opts.set("file-allocation", "none");
    opts.set("connect-timeout", "1");
    opts.set("timeout", "1");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/pipe1.bin")],
        dest: dest.clone(),
        opts,
        progress: HttpProgress::new(),
        piece_length: 4096,
        cancel: rx,
    };
    let r = tokio::time::timeout(std::time::Duration::from_secs(3), http::download(job)).await;
    assert!(
        r.is_err() || r.unwrap().is_err(),
        "max-http-pipelining=1 must not send 2 GETs before a response"
    );
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "must not dest-match without pipelining"
    );
}

async fn read_http_headers(
    s: &mut tokio::net::TcpStream,
) -> Option<String> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1];
    loop {
        s.read_exact(&mut tmp).await.ok()?;
        buf.push(tmp[0]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if buf.len() > 16 * 1024 {
            return None;
        }
    }
    Some(String::from_utf8_lossy(&buf).into_owned())
}

async fn spawn_http_accepts(body: &'static [u8]) -> (u16, Arc<AtomicU64>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let accepts = Arc::new(AtomicU64::new(0));
    let seen = Arc::clone(&accepts);
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else {
                break;
            };
            seen.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                s.set_nodelay(true).ok();
                loop {
                    let req = match tokio::time::timeout(
                        std::time::Duration::from_secs(30),
                        read_http_headers(&mut s),
                    )
                    .await
                    {
                        Ok(Some(r)) => r,
                        _ => return,
                    };
                    let close = req.to_ascii_lowercase().contains("connection: close");
                    if req.starts_with("HEAD ") {
                        let hdr = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: keep-alive\r\n\r\n",
                            body.len()
                        );
                        if s.write_all(hdr.as_bytes()).await.is_err() {
                            return;
                        }
                    } else if let Some(r) = req.lines().find(|l| l.to_ascii_lowercase().starts_with("range:")) {
                        let spec = r.split(':').nth(1).unwrap_or("").trim();
                        let spec = spec.trim_start_matches("bytes=");
                        let mut parts = spec.split('-');
                        let start: usize = parts.next().unwrap_or("0").parse().unwrap_or(0);
                        let end: usize = parts
                            .next()
                            .filter(|s| !s.is_empty())
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(body.len() - 1);
                        let end = end.min(body.len() - 1);
                        let slice = &body[start..=end];
                        let hdr = format!(
                            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nAccept-Ranges: bytes\r\nConnection: keep-alive\r\n\r\n",
                            slice.len(),
                            body.len()
                        );
                        if s.write_all(hdr.as_bytes()).await.is_err() {
                            return;
                        }
                        if s.write_all(slice).await.is_err() {
                            return;
                        }
                    } else {
                        let hdr = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: keep-alive\r\n\r\n",
                            body.len()
                        );
                        if s.write_all(hdr.as_bytes()).await.is_err() {
                            return;
                        }
                        if s.write_all(body).await.is_err() {
                            return;
                        }
                    }
                    if close {
                        return;
                    }
                }
            });
        }
    });
    (port, accepts)
}

fn keep_alive_job(port: u16, dest: std::path::PathBuf, ka: &str) -> HttpJob {
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "2");
    opts.set("max-connection-per-server", "1");
    opts.set("min-split-size", "1");
    opts.set("piece-length", "4096");
    opts.set("file-allocation", "none");
    opts.set("enable-http-pipelining", "false");
    opts.set("use-head", "false");
    opts.set("enable-http-keep-alive", ka);
    let (_tx, rx) = watch::channel(false);
    HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/ka.bin")],
        dest,
        opts,
        progress: HttpProgress::new(),
        piece_length: 4096,
        cancel: rx,
    }
}

#[tokio::test]
async fn enable_http_keep_alive_true_one_accept_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0xA1u8; 8192].into_boxed_slice());
    let mut last_n = 0u64;
    let mut dest_ok = false;
    for _ in 0..3 {
        let (port, accepts) = spawn_http_accepts(body).await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("ka.bin");
        http::reset_http_io();
        http::download(keep_alive_job(port, dest.clone(), "true"))
            .await
            .unwrap();
        dest_ok = std::fs::read(&dest).unwrap() == body;
        last_n = accepts.load(Ordering::SeqCst);
        if dest_ok && last_n == 1 && http::last_http_ka_reuse() >= 1 {
            return;
        }
    }
    assert!(
        dest_ok,
        "--enable-http-keep-alive=true dest must match"
    );
    assert_eq!(
        last_n, 1,
        "--enable-http-keep-alive=true must reuse one TCP accept, got {last_n}"
    );
}

#[tokio::test]
async fn enable_http_keep_alive_false_many_accepts_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0xA2u8; 8192].into_boxed_slice());
    let (port, accepts) = spawn_http_accepts(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("ka.bin");
    http::reset_http_io();
    http::download(keep_alive_job(port, dest.clone(), "false"))
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--enable-http-keep-alive=false dest must match"
    );
    let n = accepts.load(Ordering::SeqCst);
    assert!(
        n >= 2,
        "--enable-http-keep-alive=false must open >=2 TCP accepts, got {n}"
    );
    assert!(
        http::last_http_send() >= 2,
        "C++ SocketCore HttpRequest Range GET must writeData per segment, got {}",
        http::last_http_send()
    );
}
