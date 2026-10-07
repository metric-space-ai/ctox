//! Room-password LAN share: HTTP Range dest-match + rejected auth/path.
use aria2_rust::http::{self, HttpJob, HttpProgress};
use aria2_rust::options::OptionSet;
use aria2_rust::room::RoomHub;
use aria2_rust::session::Session;
use std::sync::Arc;
use tokio::sync::watch;

async fn wait_hub(session: &Arc<Session>) -> bool {
    for _ in 0..80 {
        if !session.tell_room_peers().await.is_empty() {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    false
}

#[tokio::test]
async fn room_http_range_dest_match() {
    let dir = tempfile::tempdir().unwrap();
    let body: Vec<u8> = (0..32 * 1024u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(dir.path().join("clip.bin"), &body).unwrap();
    std::fs::create_dir_all(dir.path().join("deep")).unwrap();
    std::fs::write(dir.path().join("deep/nested.bin"), b"nested-bytes").unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("room-password", "kitchen-42");
    opts.set("room-listen-port", "0");
    opts.set("room-udp-port", "0");
    opts.set("room-name", "alpha");
    opts.set("enable-room-share", "true");
    let hub = RoomHub::start(&opts).await.unwrap();
    let port = hub.http_port();
    let url = format!("http://127.0.0.1:{port}/room/v1/file/clip.bin");

    let client = reqwest::Client::new();
    let deny = client.get(&url).send().await.unwrap();
    assert_eq!(deny.status(), 401, "no password must 401");

    let wrong = client
        .get(&url)
        .header("X-Room-Password", "nope")
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), 401);

    let trav = client
        .get(format!("http://127.0.0.1:{port}/room/v1/file/../clip.bin"))
        .header("X-Room-Password", "kitchen-42")
        .send()
        .await
        .unwrap();
    assert!(
        trav.status() == 404 || trav.status() == 400,
        "dotdot must not leak, got {}",
        trav.status()
    );

    let ok = client
        .get(&url)
        .header("X-Room-Password", "kitchen-42")
        .send()
        .await
        .unwrap();
    assert!(ok.status().is_success());
    assert_eq!(ok.bytes().await.unwrap().as_ref(), &body[..]);

    let rng = client
        .get(&url)
        .header("X-Room-Password", "kitchen-42")
        .header("Range", "bytes=100-163")
        .send()
        .await
        .unwrap();
    assert_eq!(rng.status(), 206);
    assert_eq!(rng.bytes().await.unwrap().as_ref(), &body[100..=163]);

    let nested = client
        .get(format!("http://127.0.0.1:{port}/room/v1/file/deep/nested.bin"))
        .header("X-Room-Password", "kitchen-42")
        .send()
        .await
        .unwrap();
    assert_eq!(nested.bytes().await.unwrap().as_ref(), b"nested-bytes");

    let listing_bytes = client
        .get(format!("http://127.0.0.1:{port}/room/v1/files"))
        .header("X-Room-Password", "kitchen-42")
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let listing: Vec<aria2_rust::room::FileOffer> =
        serde_json::from_slice(&listing_bytes).unwrap();
    assert!(listing.iter().any(|f| f.path == "clip.bin" && f.size == body.len() as u64));
    assert!(listing.iter().any(|f| f.path == "deep/nested.bin"));
    hub.stop();
}

#[tokio::test]
async fn room_copy_from_peer_dest_match() {
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let body = b"deep-file-copy-payload-bytes!!".to_vec();
    std::fs::write(src.path().join("movie.bin"), &body).unwrap();

    let mut src_opts = OptionSet::with_defaults();
    src_opts.set("dir", src.path().to_string_lossy().into_owned());
    src_opts.set("room-password", "same-room");
    src_opts.set("room-listen-port", "0");
    src_opts.set("room-udp-port", "0");
    src_opts.set("room-name", "src-pc");
    let hub = RoomHub::start(&src_opts).await.unwrap();
    let port = hub.http_port();

    let mut dst_opts = OptionSet::with_defaults();
    dst_opts.set("dir", dst.path().to_string_lossy().into_owned());
    dst_opts.set("room-password", "same-room");
    dst_opts.set("room-listen-port", "0");
    dst_opts.set("room-udp-port", "0");
    dst_opts.set("room-name", "dst-pc");
    dst_opts.set("file-allocation", "none");
    dst_opts.set("split", "4");
    dst_opts.set("max-connection-per-server", "4");
    dst_opts.set("min-split-size", "1");
    let session = Session::new(dst_opts).unwrap();
    assert!(wait_hub(&session).await, "receiver room hub must start");
    let gids = session
        .copy_from_room("127.0.0.1", port, "movie.bin", false)
        .await
        .unwrap();
    assert!(!gids.is_empty());
    let dest = dst.path().join("movie.bin");
    for _ in 0..80 {
        if dest.exists() && std::fs::read(&dest).ok().as_deref() == Some(body.as_slice()) {
            hub.stop();
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!(
        "deep copy dest mismatch: exists={} bytes={:?}",
        dest.exists(),
        dest.exists().then(|| std::fs::read(&dest).ok())
    );
}

#[tokio::test]
async fn room_http_job_split_dest_match() {
    let dir = tempfile::tempdir().unwrap();
    let body: Vec<u8> = (0..48 * 1024u32).map(|i| (i % 199) as u8).collect();
    let leaked: &'static [u8] = Box::leak(body.clone().into_boxed_slice());
    std::fs::write(dir.path().join("part.bin"), leaked).unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("room-password", "split-room");
    opts.set("room-listen-port", "0");
    opts.set("room-udp-port", "0");
    let hub = RoomHub::start(&opts).await.unwrap();
    let port = hub.http_port();

    let dest_dir = tempfile::tempdir().unwrap();
    let dest = dest_dir.path().join("part.bin");
    let mut job_opts = OptionSet::with_defaults();
    job_opts.set("split", "4");
    job_opts.set("max-connection-per-server", "4");
    job_opts.set("min-split-size", "1024");
    job_opts.set("piece-length", "4096");
    job_opts.set("file-allocation", "none");
    job_opts.set("header", "X-Room-Password: split-room");
    let (_tx, rx) = watch::channel(false);
    let job = HttpJob {
        uris: vec![format!("http://127.0.0.1:{port}/room/v1/file/part.bin")],
        dest: dest.clone(),
        opts: job_opts,
        progress: HttpProgress::new(),
        piece_length: 4096,
        cancel: rx,
    };
    http::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), leaked);
    hub.stop();
}
