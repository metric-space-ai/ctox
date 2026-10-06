use aria2_rust::metalink;
use aria2_rust::options::OptionSet;
use aria2_rust::session::Session;
use sha1::{Digest, Sha1};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn spawn_body(body: &'static [u8]) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            let body = body;
            tokio::spawn(async move {
                let mut buf = vec![0u8; 2048];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(body).await;
                let _ = req;
            });
        }
    });
    port
}

fn metalink4(name: &str, url: &str, sha1_hex: &str, size: usize) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<metalink xmlns="urn:ietf:params:xml:ns:metalink">
  <file name="{name}">
    <size>{size}</size>
    <hash type="sha-1">{sha1_hex}</hash>
    <url>{url}</url>
  </file>
</metalink>"#
    )
}

async fn wait_complete(session: &Session, gid: &str) {
    for _ in 0..80 {
        let st = session.tell_status(gid).await.unwrap();
        if st.get("status").and_then(|v| v.as_str()) == Some("complete") {
            return;
        }
        if st.get("status").and_then(|v| v.as_str()) == Some("error") {
            panic!("error: {st}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("timeout {gid}");
}

#[tokio::test]
async fn metalink_file_http_dest_match() {
    let body: &'static [u8] = b"metalink-payload-bytes";
    let port = spawn_body(body).await;
    let url = format!("http://127.0.0.1:{port}/ml.bin");
    let sha = hex::encode(Sha1::digest(body));
    let xml = metalink4("ml.bin", &url, &sha, body.len());
    let files = metalink::parse(&xml).unwrap();
    assert_eq!(files[0].preferred_urls()[0], url);
    let dir = tempfile::tempdir().unwrap();
    let meta_path = dir.path().join("ml.meta4");
    std::fs::write(&meta_path, &xml).unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    let session = Session::new(opts).unwrap();
    let mut extra = OptionSet::new();
    extra.set("metalink-file", meta_path.display().to_string());
    let gid = session.add_uri_and_start(vec![], extra).await.unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("ml.bin")).unwrap(), body);
}

#[tokio::test]
async fn metalink_file_http_sha256_dest_match() {
    use sha2::Digest;
    let body: &'static [u8] = b"metalink-sha256-payload-bytes";
    let port = spawn_body(body).await;
    let url = format!("http://127.0.0.1:{port}/ml256.bin");
    let sha = hex::encode(sha2::Sha256::digest(body));
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<metalink xmlns="urn:ietf:params:xml:ns:metalink">
  <file name="ml256.bin">
    <size>{}</size>
    <hash type="sha-256">{sha}</hash>
    <url>{url}</url>
  </file>
</metalink>"#,
        body.len()
    );
    let files = metalink::parse(&xml).unwrap();
    assert_eq!(
        files[0].checksum_spec().unwrap(),
        format!("sha-256={sha}")
    );
    let dir = tempfile::tempdir().unwrap();
    let meta_path = dir.path().join("ml256.meta4");
    std::fs::write(&meta_path, &xml).unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    let session = Session::new(opts).unwrap();
    let mut extra = OptionSet::new();
    extra.set("metalink-file", meta_path.display().to_string());
    let gid = session.add_uri_and_start(vec![], extra).await.unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("ml256.bin")).unwrap(), body);
}

#[tokio::test]
async fn metalink_show_files_lists_and_skips_dest() {
    let body: &'static [u8] = b"metalink-show-files-bytes";
    let port = spawn_body(body).await;
    let url = format!("http://127.0.0.1:{port}/mlshow.bin");
    let sha = hex::encode(Sha1::digest(body));
    let xml = metalink4("mlshow.bin", &url, &sha, body.len());
    let dir = tempfile::tempdir().unwrap();
    let meta_path = dir.path().join("mlshow.meta4");
    std::fs::write(&meta_path, &xml).unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("show-files", "true");
    let session = Session::new(opts).unwrap();
    let mut extra = OptionSet::new();
    extra.set("metalink-file", meta_path.display().to_string());
    extra.set("show-files", "true");
    let gid = session.add_uri_and_start(vec![], extra).await.unwrap();
    wait_complete(&session, gid.as_str()).await;
    let listing = session.stdout_text();
    assert!(listing.contains("mlshow.bin"), "{listing}");
    assert!(
        !dir.path().join("mlshow.bin").exists(),
        "--show-files must not write metalink dest"
    );
}

#[tokio::test]
async fn follow_metalink_http_dest_match() {
    let body: &'static [u8] = b"follow-metalink-payload";
    let port_payload = spawn_body(body).await;
    let url = format!("http://127.0.0.1:{port_payload}/f.bin");
    let sha = hex::encode(Sha1::digest(body));
    let xml = metalink4("f.bin", &url, &sha, body.len());
    let xml_bytes: &'static [u8] = Box::leak(xml.into_bytes().into_boxed_slice());
    let port_meta = spawn_body(xml_bytes).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("follow-metalink", "true");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port_meta}/f.meta4")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("f.bin")).unwrap(), body);
}

#[tokio::test]
async fn follow_metalink_false_keeps_xml() {
    let body: &'static [u8] = b"should-not-download";
    let port_payload = spawn_body(body).await;
    let url = format!("http://127.0.0.1:{port_payload}/x.bin");
    let sha = hex::encode(Sha1::digest(body));
    let xml = metalink4("x.bin", &url, &sha, body.len());
    let xml_bytes: &'static [u8] = Box::leak(xml.clone().into_bytes().into_boxed_slice());
    let port_meta = spawn_body(xml_bytes).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("follow-metalink", "false");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port_meta}/x.meta4")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("x.meta4")).unwrap(), xml.as_bytes());
    assert!(!dir.path().join("x.bin").exists());
}

#[tokio::test]
async fn enable_metalink_false_http_keeps_xml() {
    let body: &'static [u8] = b"should-not-download-metalink";
    let port_payload = spawn_body(body).await;
    let url = format!("http://127.0.0.1:{port_payload}/noml.bin");
    let sha = hex::encode(Sha1::digest(body));
    let xml = metalink4("noml.bin", &url, &sha, body.len());
    let xml_bytes: &'static [u8] = Box::leak(xml.clone().into_bytes().into_boxed_slice());
    let port_meta = spawn_body(xml_bytes).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("follow-metalink", "true");
    opts.set("enable-metalink", "false");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port_meta}/noml.meta4")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(
        std::fs::read(dir.path().join("noml.meta4")).unwrap(),
        xml.as_bytes(),
        "--enable-metalink=false dest-matches .meta4 bytes"
    );
    assert!(
        !dir.path().join("noml.bin").exists(),
        "--enable-metalink=false must not fetch metalink payload"
    );
}

#[tokio::test]
async fn enable_metalink_false_add_metalink_rejected() {
    let body: &'static [u8] = b"reject-metalink";
    let sha = hex::encode(Sha1::digest(body));
    let xml = metalink4("rej.bin", "http://127.0.0.1:1/rej.bin", &sha, body.len());
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("enable-metalink", "false");
    let session = Session::new(opts).unwrap();
    let err = session
        .add_metalink_and_start(xml.into_bytes(), OptionSet::new())
        .await;
    assert!(
        err.is_err(),
        "--enable-metalink=false must reject addMetalink: {err:?}"
    );
    assert!(
        !dir.path().join("rej.bin").exists(),
        "--enable-metalink=false must not dest-match metalink payload"
    );
}

#[tokio::test]
async fn enable_metalink_true_http_dest_match() {
    let body: &'static [u8] = b"enable-metalink-payload";
    let port_payload = spawn_body(body).await;
    let url = format!("http://127.0.0.1:{port_payload}/enml.bin");
    let sha = hex::encode(Sha1::digest(body));
    let xml = metalink4("enml.bin", &url, &sha, body.len());
    let xml_bytes: &'static [u8] = Box::leak(xml.into_bytes().into_boxed_slice());
    let port_meta = spawn_body(xml_bytes).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("follow-metalink", "true");
    opts.set("enable-metalink", "true");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port_meta}/enml.meta4")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(
        std::fs::read(dir.path().join("enml.bin")).unwrap(),
        body,
        "--enable-metalink=true must dest-match payload"
    );
}

#[tokio::test]
async fn add_metalink_rpc_dest_match() {
    let body: &'static [u8] = b"rpc-metalink-payload";
    let port = spawn_body(body).await;
    let url = format!("http://127.0.0.1:{port}/r.bin");
    let sha = hex::encode(Sha1::digest(body));
    let xml = metalink4("r.bin", &url, &sha, body.len());
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    let session = Session::new(opts).unwrap();
    let gids = session
        .add_metalink_and_start(xml.into_bytes(), OptionSet::new())
        .await
        .unwrap();
    assert_eq!(gids.len(), 1);
    wait_complete(&session, gids[0].as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("r.bin")).unwrap(), body);
}

#[tokio::test]
async fn rpc_save_upload_metadata_metalink_sha1_meta4_dest_match() {
    let body: &'static [u8] = b"rpc-save-meta4-payload";
    let port = spawn_body(body).await;
    let url = format!("http://127.0.0.1:{port}/m.bin");
    let sha = hex::encode(Sha1::digest(body));
    let xml = metalink4("m.bin", &url, &sha, body.len());
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("rpc-save-upload-metadata", "true");
    let session = Session::new(opts).unwrap();
    let bytes = xml.into_bytes();
    let gids = session
        .add_metalink_and_start(bytes.clone(), OptionSet::new())
        .await
        .unwrap();
    wait_complete(&session, gids[0].as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("m.bin")).unwrap(), body);
    let hex = hex::encode(Sha1::digest(&bytes));
    let saved = dir.path().join(format!("{hex}.meta4"));
    assert!(saved.exists(), "addMetalink must save SHA-1 .meta4");
    assert_eq!(std::fs::read(&saved).unwrap(), bytes);
}

#[tokio::test]
async fn metalink_location_filter_dest_match() {
    let us: &'static [u8] = b"metalink-us-payload";
    let de: &'static [u8] = b"metalink-de-payload!!";
    let p_us = spawn_body(us).await;
    let p_de = spawn_body(de).await;
    let xml = format!(
        r#"<?xml version="1.0"?>
<metalink xmlns="urn:ietf:params:xml:ns:metalink">
  <file name="loc.bin">
    <url location="us" priority="1">http://127.0.0.1:{p_us}/loc.bin</url>
    <url location="de" priority="9">http://127.0.0.1:{p_de}/loc.bin</url>
  </file>
</metalink>"#
    );
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("metalink-location", "de");
    let session = Session::new(opts).unwrap();
    let gids = session
        .add_metalink_and_start(xml.into_bytes(), OptionSet::new())
        .await
        .unwrap();
    wait_complete(&session, gids[0].as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("loc.bin")).unwrap(), de);
}

#[tokio::test]
async fn metalink_language_os_version_dest_match() {
    let en: &'static [u8] = b"metalink-en-payload";
    let de: &'static [u8] = b"metalink-de-linux-1.2";
    let p_en = spawn_body(en).await;
    let p_de = spawn_body(de).await;
    let xml = format!(
        r#"<?xml version="1.0"?>
<metalink xmlns="urn:ietf:params:xml:ns:metalink">
  <file name="en.bin">
    <language>en</language>
    <os>Windows</os>
    <version>1.0</version>
    <url>http://127.0.0.1:{p_en}/en.bin</url>
  </file>
  <file name="app.bin">
    <language>de</language>
    <os>Linux-x86_64</os>
    <version>1.2</version>
    <url>http://127.0.0.1:{p_de}/app.bin</url>
  </file>
</metalink>"#
    );
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("metalink-language", "de");
    opts.set("metalink-os", "Linux-x86_64");
    opts.set("metalink-version", "1.2");
    let session = Session::new(opts).unwrap();
    let gids = session
        .add_metalink_and_start(xml.into_bytes(), OptionSet::new())
        .await
        .unwrap();
    assert_eq!(gids.len(), 1);
    wait_complete(&session, gids[0].as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("app.bin")).unwrap(), de);
    assert!(!dir.path().join("en.bin").exists());
}

#[tokio::test]
async fn metalink_preferred_protocol_unique_dest_match() {
    let http_body: &'static [u8] = b"metalink-http-proto";
    let ftp_body: &'static [u8] = b"metalink-ftp-tagged!!";
    let p_http = spawn_body(http_body).await;
    let p_ftp = spawn_body(ftp_body).await;
    let xml = format!(
        r#"<?xml version="1.0"?>
<metalink xmlns="urn:ietf:params:xml:ns:metalink">
  <file name="proto.bin">
    <url type="ftp" priority="1">http://127.0.0.1:{p_ftp}/proto.bin</url>
    <url type="http" priority="9">http://127.0.0.1:{p_http}/proto.bin</url>
  </file>
</metalink>"#
    );
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("metalink-preferred-protocol", "http");
    opts.set("metalink-enable-unique-protocol", "true");
    let session = Session::new(opts).unwrap();
    let gids = session
        .add_metalink_and_start(xml.into_bytes(), OptionSet::new())
        .await
        .unwrap();
    wait_complete(&session, gids[0].as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("proto.bin")).unwrap(), http_body);
}

fn pieces_xml(name: &str, url: &str, size: usize, plen: usize, piece_hexes: &[&str]) -> String {
    let hashes = piece_hexes
        .iter()
        .map(|h| format!("      <hash>{h}</hash>\n"))
        .collect::<String>();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<metalink xmlns="urn:ietf:params:xml:ns:metalink">
  <file name="{name}">
    <size>{size}</size>
    <pieces length="{plen}" type="sha-1">
{hashes}    </pieces>
    <url>{url}</url>
  </file>
</metalink>"#
    )
}

async fn wait_error(session: &Session, gid: &str) {
    for _ in 0..80 {
        let st = session.tell_status(gid).await.unwrap();
        if st.get("status").and_then(|v| v.as_str()) == Some("error") {
            return;
        }
        if st.get("status").and_then(|v| v.as_str()) == Some("complete") {
            panic!("expected error, got complete: {st}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("timeout waiting error {gid}");
}

#[tokio::test]
async fn realtime_chunk_checksum_true_dest_match() {
    let body: &'static [u8] = b"0123456789abcdef";
    let port = spawn_body(body).await;
    let url = format!("http://127.0.0.1:{port}/chunk.bin");
    let p0 = hex::encode(Sha1::digest(&body[..8]));
    let p1 = hex::encode(Sha1::digest(&body[8..]));
    let xml = pieces_xml("chunk.bin", &url, body.len(), 8, &[&p0, &p1]);
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("realtime-chunk-checksum", "true");
    let session = Session::new(opts).unwrap();
    let gids = session
        .add_metalink_and_start(xml.into_bytes(), OptionSet::new())
        .await
        .unwrap();
    wait_complete(&session, gids[0].as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("chunk.bin")).unwrap(), body);
}

#[tokio::test]
async fn realtime_chunk_checksum_true_rejects_bad_piece() {
    let body: &'static [u8] = b"0123456789abcdef";
    let port = spawn_body(body).await;
    let url = format!("http://127.0.0.1:{port}/bad.bin");
    let p0 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let p1 = hex::encode(Sha1::digest(&body[8..]));
    let xml = pieces_xml("bad.bin", &url, body.len(), 8, &[p0, &p1]);
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("realtime-chunk-checksum", "true");
    let session = Session::new(opts).unwrap();
    let gids = session
        .add_metalink_and_start(xml.into_bytes(), OptionSet::new())
        .await
        .unwrap();
    wait_error(&session, gids[0].as_str()).await;
}

#[tokio::test]
async fn realtime_chunk_checksum_false_skips_bad_piece_dest_match() {
    let body: &'static [u8] = b"0123456789abcdef";
    let port = spawn_body(body).await;
    let url = format!("http://127.0.0.1:{port}/skip.bin");
    let p0 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let p1 = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let xml = pieces_xml("skip.bin", &url, body.len(), 8, &[p0, p1]);
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("realtime-chunk-checksum", "false");
    let session = Session::new(opts).unwrap();
    let gids = session
        .add_metalink_and_start(xml.into_bytes(), OptionSet::new())
        .await
        .unwrap();
    wait_complete(&session, gids[0].as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("skip.bin")).unwrap(), body);
}

#[tokio::test]
async fn metalink_base_uri_relative_dest_match() {
    let body: &'static [u8] = b"metalink-base-uri-bytes";
    let port = spawn_body(body).await;
    let sha = hex::encode(Sha1::digest(body));
    let xml = metalink4("base.bin", "base.bin", &sha, body.len());
    let dir = tempfile::tempdir().unwrap();
    let meta_path = dir.path().join("base.meta4");
    std::fs::write(&meta_path, &xml).unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("metalink-base-uri", format!("http://127.0.0.1:{port}/"));
    let session = Session::new(opts).unwrap();
    let mut extra = OptionSet::new();
    extra.set("metalink-file", meta_path.display().to_string());
    let gid = session.add_uri_and_start(vec![], extra).await.unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(
        std::fs::read(dir.path().join("base.bin")).unwrap(),
        body,
        "--metalink-base-uri must dest-match a relative metalink URL"
    );
}

#[tokio::test]
async fn metalink_base_uri_unset_relative_no_dest() {
    let body: &'static [u8] = b"metalink-base-uri-bytes";
    let sha = hex::encode(Sha1::digest(body));
    let xml = metalink4("miss.bin", "miss.bin", &sha, body.len());
    let dir = tempfile::tempdir().unwrap();
    let meta_path = dir.path().join("miss.meta4");
    std::fs::write(&meta_path, &xml).unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    let session = Session::new(opts).unwrap();
    let mut extra = OptionSet::new();
    extra.set("metalink-file", meta_path.display().to_string());
    let err = session.add_uri_and_start(vec![], extra).await;
    assert!(
        err.is_err(),
        "relative metalink URL without --metalink-base-uri must fail: {err:?}"
    );
    assert!(
        !dir.path().join("miss.bin").exists(),
        "relative metalink without base-uri must not dest-match"
    );
}
