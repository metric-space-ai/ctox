use aria2_rust::options::OptionSet;
use aria2_rust::session::Session;
use serde_json::{json, Value};
use sha1::Digest;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn spawn_body(body: &'static [u8]) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 2048];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                if req.to_ascii_lowercase().contains("range:") {
                    let hdr = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes 0-{}/{}\r\nAccept-Ranges: bytes\r\n\r\n",
                        body.len(),
                        body.len() - 1,
                        body.len()
                    );
                    let _ = s.write_all(hdr.as_bytes()).await;
                    let _ = s.write_all(body).await;
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

fn strip_csi(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == 0x1b && i + 1 < b.len() && b[i + 1] == b'[' {
            i += 2;
            while i < b.len() && !b[i].is_ascii_alphabetic() {
                i += 1;
            }
            if i < b.len() {
                i += 1;
            }
            continue;
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

#[tokio::test]
async fn add_uri_completes_and_tell_status() {
    let body: &'static [u8] = b"rpc-add-uri-payload";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "r.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/r.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    for _ in 0..80 {
        let st = session.tell_status(gid.as_str()).await.unwrap();
        if st.get("status").and_then(|v| v.as_str()) == Some("complete") {
            let dest = dir.path().join("r.bin");
            assert_eq!(std::fs::read(&dest).unwrap(), body);
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("download did not complete");
}

#[tokio::test]
async fn log_file_http_dest_match() {
    let body: &'static [u8] = b"log-file-dest-match-bytes";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("aria2.log");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "log.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("log", log.display().to_string());
    opts.set("log-level", "info");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/log.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("log.bin")).unwrap(), body);
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(
        text.contains("Download started") && text.contains(&format!("http://127.0.0.1:{port}/log.bin")),
        "log must record start URI: {text}"
    );
    assert!(
        text.contains("Download complete") && text.contains("log.bin"),
        "log must record complete path: {text}"
    );
}

#[tokio::test]
async fn log_level_error_omits_info_dest_match() {
    let body: &'static [u8] = b"log-level-error-payload";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("err.log");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "e.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("log", log.display().to_string());
    opts.set("log-level", "error");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/e.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("e.bin")).unwrap(), body);
    let text = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(
        !text.contains("Download started") && !text.contains("Download complete"),
        "log-level=error must omit INFO lines: {text}"
    );
}

#[tokio::test]
async fn download_result_default_in_log_dest_match() {
    let body: &'static [u8] = b"download-result-default-bytes";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("aria2.log");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "dr.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("log", log.display().to_string());
    opts.set("download-result", "default");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/dr.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("dr.bin")).unwrap(), body);
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(
        text.contains("Download Results:") && text.contains("OK") && text.contains("dr.bin"),
        "default download-result must log GID|OK|path: {text}"
    );
}

#[tokio::test]
async fn download_result_hide_omits_block_dest_match() {
    let body: &'static [u8] = b"download-result-hide-bytes";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("aria2.log");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "hide.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("log", log.display().to_string());
    opts.set("download-result", "hide");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/hide.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("hide.bin")).unwrap(), body);
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(
        !text.contains("Download Results:"),
        "hide must omit Download Results: {text}"
    );
}

#[tokio::test]
async fn download_result_full_includes_uri_dest_match() {
    let body: &'static [u8] = b"download-result-full-bytes";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("aria2.log");
    let uri = format!("http://127.0.0.1:{port}/full.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "full.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("log", log.display().to_string());
    opts.set("download-result", "full");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(vec![uri.clone()], OptionSet::new())
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("full.bin")).unwrap(), body);
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(
        text.contains("Download Results:") && text.contains("URI=") && text.contains(&uri),
        "full download-result must include URI: {text}"
    );
}

#[tokio::test]
async fn summary_interval_writes_during_gated_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0x51u8; 16 * 1024].into_boxed_slice());
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port = spawn_gated(body, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("aria2.log");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "sum.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("log", log.display().to_string());
    opts.set("summary-interval", "1");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/sum.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_completed_at_least(&session, gid.as_str(), 1).await;
    let t0 = tokio::time::Instant::now();
    loop {
        let text = std::fs::read_to_string(&log).unwrap_or_default();
        if text.contains("SUMMARY") && text.contains("completedLength=") {
            break;
        }
        if t0.elapsed() > std::time::Duration::from_secs(4) {
            panic!("no SUMMARY in log: {text}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let mid = session.tell_status(gid.as_str()).await.unwrap();
    assert_ne!(
        mid.get("status").and_then(|s| s.as_str()),
        Some("complete"),
        "SUMMARY must fire before complete"
    );
    open.store(true, std::sync::atomic::Ordering::SeqCst);
    gate.notify_waiters();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("sum.bin")).unwrap(), body);
}

#[tokio::test]
async fn summary_interval_zero_omits_summary_dest_match() {
    let body: &'static [u8] = b"summary-interval-zero-payload";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("aria2.log");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "z.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("log", log.display().to_string());
    opts.set("summary-interval", "0");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/z.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("z.bin")).unwrap(), body);
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(
        !text.contains("SUMMARY"),
        "summary-interval=0 must omit SUMMARY: {text}"
    );
}

#[tokio::test]
async fn show_console_readout_false_omits_console_keeps_log_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0x52u8; 16 * 1024].into_boxed_slice());
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port = spawn_gated(body, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("aria2.log");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "scr-off.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("log", log.display().to_string());
    opts.set("summary-interval", "1");
    opts.set("show-console-readout", "false");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/scr-off.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_completed_at_least(&session, gid.as_str(), 1).await;
    let t0 = tokio::time::Instant::now();
    loop {
        let text = std::fs::read_to_string(&log).unwrap_or_default();
        if text.contains("SUMMARY") && text.contains("completedLength=") {
            break;
        }
        if t0.elapsed() > std::time::Duration::from_secs(4) {
            panic!("no SUMMARY in log: {text}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let cons = session.console_text();
    assert!(
        !cons.contains("SUMMARY"),
        "show-console-readout=false must omit console SUMMARY: {cons}"
    );
    open.store(true, std::sync::atomic::Ordering::SeqCst);
    gate.notify_waiters();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("scr-off.bin")).unwrap(), body);
    let cons = session.console_text();
    assert!(
        cons.contains("Download Results:"),
        "download-result still prints: {cons}"
    );
}

#[tokio::test]
async fn show_console_readout_true_summary_on_console_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0x53u8; 16 * 1024].into_boxed_slice());
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port = spawn_gated(body, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "scr-on.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("summary-interval", "1");
    opts.set("show-console-readout", "true");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/scr-on.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_completed_at_least(&session, gid.as_str(), 1).await;
    let t0 = tokio::time::Instant::now();
    loop {
        let cons = session.console_text();
        if cons.contains("SUMMARY") && cons.contains("completedLength=") {
            break;
        }
        if t0.elapsed() > std::time::Duration::from_secs(4) {
            panic!("no SUMMARY on console: {cons}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    open.store(true, std::sync::atomic::Ordering::SeqCst);
    gate.notify_waiters();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("scr-on.bin")).unwrap(), body);
}

#[tokio::test]
async fn truncate_console_readout_true_clips_summary_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0x54u8; 16 * 1024].into_boxed_slice());
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port = spawn_gated(body, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("aria2.log");
    let out = format!("tcr-{}-TCRMARK.bin", "x".repeat(160));
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", out.clone());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("log", log.display().to_string());
    opts.set("summary-interval", "1");
    opts.set("show-console-readout", "true");
    opts.set("truncate-console-readout", "true");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/{out}")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_completed_at_least(&session, gid.as_str(), 1).await;
    let t0 = tokio::time::Instant::now();
    loop {
        let cons = session.console_text();
        if cons.contains("SUMMARY") {
            break;
        }
        if t0.elapsed() > std::time::Duration::from_secs(4) {
            panic!("no SUMMARY on console: {cons}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let cons = session.console_text();
    for line in cons.lines() {
        let plain = strip_csi(line);
        if let Some(rest) = plain.strip_prefix("[NOTICE] ") {
            if rest.contains("SUMMARY") {
                assert!(
                    rest.len() <= 80,
                    "truncated readout must fit 80 cols: {} ({})",
                    rest.len(),
                    rest
                );
            }
        }
    }
    assert!(
        !cons.contains("TCRMARK"),
        "truncate=true must clip long path: {cons}"
    );
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(
        text.contains("TCRMARK"),
        "file log keeps full path: {text}"
    );
    open.store(true, std::sync::atomic::Ordering::SeqCst);
    gate.notify_waiters();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join(&out)).unwrap(), body);
}

#[tokio::test]
async fn truncate_console_readout_false_keeps_full_path_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0x55u8; 16 * 1024].into_boxed_slice());
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port = spawn_gated(body, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let dir = tempfile::tempdir().unwrap();
    let out = format!("tcr-{}-TCRKEEP.bin", "x".repeat(160));
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", out.clone());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("summary-interval", "1");
    opts.set("show-console-readout", "true");
    opts.set("truncate-console-readout", "false");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/{out}")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_completed_at_least(&session, gid.as_str(), 1).await;
    let t0 = tokio::time::Instant::now();
    loop {
        let cons = session.console_text();
        if cons.contains("SUMMARY") && cons.contains("TCRKEEP") {
            break;
        }
        if t0.elapsed() > std::time::Duration::from_secs(4) {
            panic!("no full SUMMARY on console: {cons}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    open.store(true, std::sync::atomic::Ordering::SeqCst);
    gate.notify_waiters();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join(&out)).unwrap(), body);
}

#[tokio::test]
async fn quiet_omits_console_keeps_file_log_dest_match() {
    let body: &'static [u8] = b"quiet-console-dest-match";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("aria2.log");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "q.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("log", log.display().to_string());
    opts.set("quiet", "true");
    opts.set("download-result", "default");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/q.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("q.bin")).unwrap(), body);
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(
        text.contains("Download Results:"),
        "quiet must not suppress file log: {text}"
    );
    let cons = session.console_text();
    assert!(
        !cons.contains("Download Results:"),
        "quiet must suppress console: {cons}"
    );
}

#[tokio::test]
async fn quiet_false_console_has_download_result_dest_match() {
    let body: &'static [u8] = b"quiet-false-console-bytes";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "nq.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("quiet", "false");
    opts.set("download-result", "default");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/nq.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("nq.bin")).unwrap(), body);
    let cons = session.console_text();
    assert!(
        cons.contains("Download Results:") && cons.contains("nq.bin"),
        "default console must print download-result: {cons}"
    );
}

#[tokio::test]
async fn enable_color_true_ansi_dest_match() {
    let body: &'static [u8] = b"enable-color-true-bytes";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "color.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("enable-color", "true");
    opts.set("download-result", "default");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/color.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("color.bin")).unwrap(), body);
    let cons = session.console_text();
    assert!(
        cons.contains("\x1b[1;36m") && cons.contains("Download Results:") && cons.contains("color.bin"),
        "enable-color=true must ANSI-color NOTICE dest-match: {cons:?}"
    );
}

#[tokio::test]
async fn enable_color_false_plain_dest_match() {
    let body: &'static [u8] = b"enable-color-false-bytes";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "nocolor.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("enable-color", "false");
    opts.set("download-result", "default");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/nocolor.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("nocolor.bin")).unwrap(), body);
    let cons = session.console_text();
    assert!(
        cons.contains("Download Results:") && cons.contains("nocolor.bin"),
        "enable-color=false must print dest-match: {cons}"
    );
    assert!(
        !cons.contains('\u{1b}'),
        "enable-color=false must omit ANSI: {cons:?}"
    );
}

#[tokio::test]
async fn console_log_level_error_omits_notice_dest_match() {
    let body: &'static [u8] = b"console-log-level-error-bytes";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("aria2.log");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "cl.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("log", log.display().to_string());
    opts.set("console-log-level", "error");
    opts.set("download-result", "default");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/cl.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("cl.bin")).unwrap(), body);
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(
        text.contains("Download Results:"),
        "file log still gets NOTICE: {text}"
    );
    let cons = session.console_text();
    assert!(
        !cons.contains("Download Results:"),
        "console-log-level=error must omit NOTICE: {cons}"
    );
}

#[tokio::test]
async fn stderr_false_download_result_on_stdout_dest_match() {
    let body: &'static [u8] = b"stderr-false-stdout-bytes";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "so.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("stderr", "false");
    opts.set("download-result", "default");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/so.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("so.bin")).unwrap(), body);
    let out = session.stdout_text();
    let err = session.stderr_text();
    assert!(
        out.contains("Download Results:") && out.contains("so.bin"),
        "default console is stdout: {out}"
    );
    assert!(
        !err.contains("Download Results:"),
        "stderr=false must not use stderr: {err}"
    );
}

#[tokio::test]
async fn stderr_true_download_result_on_stderr_dest_match() {
    let body: &'static [u8] = b"stderr-true-err-bytes";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "se.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("stderr", "true");
    opts.set("download-result", "default");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/se.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("se.bin")).unwrap(), body);
    let out = session.stdout_text();
    let err = session.stderr_text();
    assert!(
        err.contains("Download Results:") && err.contains("se.bin"),
        "stderr=true console is stderr: {err}"
    );
    assert!(
        !out.contains("Download Results:"),
        "stderr=true must not use stdout: {out}"
    );
}

#[tokio::test]
async fn human_readable_true_abbrev_size_dest_match() {
    static BODY: [u8; 2048] = [b'H'; 2048];
    let port = spawn_body(&BODY).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "hr.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("human-readable", "true");
    opts.set("download-result", "default");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/hr.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("hr.bin")).unwrap(), BODY);
    let cons = session.console_text();
    assert!(
        cons.contains("Download Results:") && cons.contains("|2.0Ki|") && cons.contains("hr.bin"),
        "human-readable=true must abbrev 2048 as 2.0Ki: {cons}"
    );
    assert!(
        !cons.contains("|2048|"),
        "human-readable=true must not print raw bytes: {cons}"
    );
}

#[tokio::test]
async fn human_readable_false_raw_bytes_dest_match() {
    static BODY: [u8; 2048] = [b'H'; 2048];
    let port = spawn_body(&BODY).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "raw.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("human-readable", "false");
    opts.set("download-result", "default");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/raw.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("raw.bin")).unwrap(), BODY);
    let cons = session.console_text();
    assert!(
        cons.contains("Download Results:") && cons.contains("|2048|") && cons.contains("raw.bin"),
        "human-readable=false must print raw bytes: {cons}"
    );
    assert!(
        !cons.contains("2.0Ki"),
        "human-readable=false must not abbreviate: {cons}"
    );
}

async fn spawn_rpc(session: Arc<Session>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let sess = Arc::clone(&session);
    tokio::spawn(async move {
        let _ = aria2_rust::rpc::serve(sess, false, port).await;
    });
    port
}

async fn rpc_call(url: &str, method: &str, params: Value) -> Value {
    let client = reqwest::Client::new();
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let Ok(res) = client
            .post(url)
            .header("Content-Type", "application/json")
            .body(
                serde_json::to_vec(&json!({
                    "jsonrpc": "2.0",
                    "id": "1",
                    "method": method,
                    "params": params
                }))
                .unwrap(),
            )
            .send()
            .await
        else {
            continue;
        };
        let bytes = res.bytes().await.unwrap();
        return serde_json::from_slice(&bytes).unwrap();
    }
    panic!("rpc {method} did not answer");
}

#[tokio::test]
async fn rpc_secret_rejects_missing_and_wrong_token() {
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("rpc-secret", "s3cret-test");
    let session = Session::new(opts).unwrap();
    let port = spawn_rpc(session).await;
    let url = format!("http://127.0.0.1:{port}/jsonrpc");
    let miss = rpc_call(&url, "aria2.getVersion", json!([])).await;
    let msg = miss
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
        .unwrap_or("");
    assert!(msg.contains("Unauthorized"), "missing token: {miss}");
    let wrong = rpc_call(&url, "aria2.getVersion", json!(["token:nope"])).await;
    let msg = wrong
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
        .unwrap_or("");
    assert!(msg.contains("Unauthorized"), "wrong token: {wrong}");
    let ok = rpc_call(&url, "aria2.getVersion", json!(["token:s3cret-test"])).await;
    assert!(
        ok.get("result").and_then(|r| r.get("version")).is_some(),
        "good token: {ok}"
    );
}

#[tokio::test]
async fn rpc_secret_add_uri_dest_match() {
    let body: &'static [u8] = b"rpc-secret-dest-match";
    let http_port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("rpc-secret", "s3cret-test");
    let session = Session::new(opts).unwrap();
    let rpc_port = spawn_rpc(Arc::clone(&session)).await;
    let url = format!("http://127.0.0.1:{rpc_port}/jsonrpc");
    let v = rpc_call(
        &url,
        "aria2.addUri",
        json!([
            "token:s3cret-test",
            [format!("http://127.0.0.1:{http_port}/sec.bin")],
            {"out": "sec.bin"}
        ]),
    )
    .await;
    let gid = v
        .get("result")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();
    assert!(!gid.is_empty(), "addUri with token: {v}");
    for _ in 0..80 {
        let st = session.tell_status(&gid).await.unwrap();
        if st.get("status").and_then(|s| s.as_str()) == Some("complete") {
            assert_eq!(std::fs::read(dir.path().join("sec.bin")).unwrap(), body);
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("rpc-secret addUri did not dest-match");
}

fn ws_mask_frame(payload: &[u8]) -> Vec<u8> {
    let mask = [0x37u8, 0xfa, 0x21, 0x3d];
    let mut out = Vec::new();
    out.push(0x81);
    let n = payload.len();
    if n < 126 {
        out.push(0x80 | n as u8);
    } else {
        out.push(0x80 | 126);
        out.extend_from_slice(&(n as u16).to_be_bytes());
    }
    out.extend_from_slice(&mask);
    for (i, b) in payload.iter().enumerate() {
        out.push(b ^ mask[i % 4]);
    }
    out
}

async fn ws_read_text(s: &mut tokio::net::TcpStream) -> String {
    let mut hdr = [0u8; 2];
    s.read_exact(&mut hdr).await.unwrap();
    let mut len = (hdr[1] & 0x7f) as usize;
    if len == 126 {
        let mut e = [0u8; 2];
        s.read_exact(&mut e).await.unwrap();
        len = u16::from_be_bytes(e) as usize;
    }
    let mut payload = vec![0u8; len];
    s.read_exact(&mut payload).await.unwrap();
    String::from_utf8(payload).unwrap()
}

async fn ws_jsonrpc(port: u16, method: &str, params: Value) -> Value {
    let mut s = None;
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        if let Ok(c) = tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
            s = Some(c);
            break;
        }
    }
    let mut s = s.expect("ws rpc listen");
    let key = "dGhlIHNhbXBsZSBub25jZQ==";
    let req = format!(
        "GET /jsonrpc HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut buf = vec![0u8; 4096];
    let n = s.read(&mut buf).await.unwrap();
    let head = String::from_utf8_lossy(&buf[..n]);
    assert!(
        head.starts_with("HTTP/1.1 101"),
        "websocket upgrade: {head}"
    );
    let body = serde_json::to_vec(&json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": method,
        "params": params
    }))
    .unwrap();
    s.write_all(&ws_mask_frame(&body)).await.unwrap();
    let text = ws_read_text(&mut s).await;
    serde_json::from_str(&text).unwrap()
}

#[tokio::test]
async fn jsonrpc_websocket_add_uri_dest_match() {
    let body: &'static [u8] = b"rpc-websocket-dest-match";
    let http_port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    let session = Session::new(opts).unwrap();
    let rpc_port = spawn_rpc(Arc::clone(&session)).await;
    let v = ws_jsonrpc(
        rpc_port,
        "aria2.addUri",
        json!([[format!("http://127.0.0.1:{http_port}/ws.bin")], {"out": "ws.bin"}]),
    )
    .await;
    let gid = v
        .get("result")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();
    assert!(!gid.is_empty(), "ws addUri: {v}");
    wait_rpc_complete(&session, &gid).await;
    assert_eq!(
        std::fs::read(dir.path().join("ws.bin")).unwrap(),
        body,
        "JSON-RPC WebSocket addUri must dest-match"
    );
}

#[tokio::test]
async fn xmlrpc_add_uri_dest_match() {
    let body: &'static [u8] = b"xmlrpc-add-uri-dest-match";
    let http_port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    let session = Session::new(opts).unwrap();
    let rpc_port = spawn_rpc(Arc::clone(&session)).await;
    let url = format!("http://127.0.0.1:{rpc_port}/rpc");
    let xml = format!(
        r#"<?xml version="1.0"?>
<methodCall>
<methodName>aria2.addUri</methodName>
<params>
<param><value><array><data>
<value><string>http://127.0.0.1:{http_port}/xml.bin</string></value>
</data></array></value></param>
<param><value><struct>
<member><name>out</name><value><string>xml.bin</string></value></member>
</struct></value></param>
</params>
</methodCall>"#
    );
    let client = reqwest::Client::new();
    let mut gid = String::new();
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let Ok(res) = client
            .post(&url)
            .header("Content-Type", "text/xml")
            .body(xml.clone())
            .send()
            .await
        else {
            continue;
        };
        let text = res.text().await.unwrap();
        assert!(
            text.contains("<methodResponse>"),
            "xml-rpc response: {text}"
        );
        assert!(!text.contains("<fault>"), "xml-rpc fault: {text}");
        let start = text.find("<string>").map(|i| i + 8).unwrap_or(0);
        let end = text[start..].find("</string>").unwrap_or(0);
        gid = text[start..start + end].to_string();
        break;
    }
    assert!(!gid.is_empty(), "xml-rpc addUri gid");
    wait_rpc_complete(&session, &gid).await;
    assert_eq!(
        std::fs::read(dir.path().join("xml.bin")).unwrap(),
        body,
        "XML-RPC addUri must dest-match"
    );
}

#[tokio::test]
async fn rpc_max_request_size_rejects_oversize() {
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("rpc-max-request-size", "64");
    let session = Session::new(opts).unwrap();
    let port = spawn_rpc(session).await;
    let url = format!("http://127.0.0.1:{port}/jsonrpc");
    let pad = "x".repeat(200);
    let (st, v) = rpc_call_auth(&url, "aria2.getVersion", json!([pad]), None).await;
    assert_eq!(st, 413, "oversize RPC must be 413: {st} {v}");
}

#[tokio::test]
async fn rpc_max_request_size_add_uri_dest_match() {
    let body: &'static [u8] = b"rpc-max-request-size-dest";
    let http_port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("rpc-max-request-size", "2K");
    let session = Session::new(opts).unwrap();
    let rpc_port = spawn_rpc(Arc::clone(&session)).await;
    let url = format!("http://127.0.0.1:{rpc_port}/jsonrpc");
    let v = rpc_call(
        &url,
        "aria2.addUri",
        json!([
            [format!("http://127.0.0.1:{http_port}/rms.bin")],
            {"out": "rms.bin"}
        ]),
    )
    .await;
    let gid = v
        .get("result")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();
    assert!(!gid.is_empty(), "addUri under limit: {v}");
    wait_rpc_complete(&session, &gid).await;
    assert_eq!(std::fs::read(dir.path().join("rms.bin")).unwrap(), body);
}

fn rpc_tls_pem_files(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let fixtures = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let cert = dir.join("rpc.crt");
    let key = dir.join("rpc.key");
    let st = std::process::Command::new("openssl")
        .args([
            "x509",
            "-inform",
            "DER",
            "-in",
        ])
        .arg(fixtures.join("https-server.der"))
        .arg("-out")
        .arg(&cert)
        .status()
        .unwrap();
    assert!(st.success(), "openssl x509 pem");
    let st = std::process::Command::new("openssl")
        .args(["pkcs8", "-inform", "DER", "-nocrypt", "-in"])
        .arg(fixtures.join("https-key.der"))
        .arg("-out")
        .arg(&key)
        .status()
        .unwrap();
    assert!(st.success(), "openssl pkcs8 pem");
    (cert, key)
}

#[tokio::test]
async fn rpc_secure_missing_cert_rejected() {
    let mut opts = OptionSet::with_defaults();
    opts.set("rpc-secure", "true");
    let session = Session::new(opts).unwrap();
    let err = aria2_rust::rpc::serve(session, false, 0)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("rpc-certificate"),
        "missing cert: {err}"
    );
}

#[tokio::test]
async fn rpc_secure_https_add_uri_dest_match() {
    let body: &'static [u8] = b"rpc-secure-https-dest";
    let http_port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let (cert, key) = rpc_tls_pem_files(dir.path());
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("rpc-secure", "true");
    opts.set("rpc-certificate", cert.display().to_string());
    opts.set("rpc-private-key", key.display().to_string());
    let session = Session::new(opts).unwrap();
    let rpc_port = spawn_rpc(Arc::clone(&session)).await;
    let url = format!("https://127.0.0.1:{rpc_port}/jsonrpc");
    let ca = include_bytes!("fixtures/https-ca.pem");
    let client = reqwest::Client::builder()
        .add_root_certificate(reqwest::Certificate::from_pem(ca).unwrap())
        .build()
        .unwrap();
    let mut v = json!(null);
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let Ok(res) = client
            .post(&url)
            .header("Content-Type", "application/json")
            .body(
                serde_json::to_vec(&json!({
                    "jsonrpc": "2.0",
                    "id": "1",
                    "method": "aria2.addUri",
                    "params": [
                        [format!("http://127.0.0.1:{http_port}/tls.bin")],
                        {"out": "tls.bin"}
                    ]
                }))
                .unwrap(),
            )
            .send()
            .await
        else {
            continue;
        };
        v = serde_json::from_slice(&res.bytes().await.unwrap()).unwrap();
        break;
    }
    let gid = v
        .get("result")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();
    assert!(!gid.is_empty(), "https addUri: {v}");
    wait_rpc_complete(&session, &gid).await;
    assert_eq!(std::fs::read(dir.path().join("tls.bin")).unwrap(), body);
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(400))
        .build()
        .unwrap();
    let plain = http_client
        .post(format!("http://127.0.0.1:{rpc_port}/jsonrpc"))
        .header("Content-Type", "application/json")
        .body(br#"{"jsonrpc":"2.0","id":"1","method":"aria2.getVersion","params":[]}"#.to_vec())
        .send()
        .await;
    assert!(
        plain.is_err(),
        "plain HTTP must fail against rpc-secure TLS"
    );
}

#[tokio::test]
async fn stop_zero_http_dest_match() {
    let body: &'static [u8] = b"stop-zero-dest-match";
    let http_port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "z.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("stop", "0");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{http_port}/z.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert!(!session.is_stopped());
    assert_eq!(std::fs::read(dir.path().join("z.bin")).unwrap(), body);
}

#[tokio::test]
async fn stop_secs_halts_gated_incomplete() {
    let body: &'static [u8] = Box::leak(vec![0x5Au8; 32 * 1024].into_boxed_slice());
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port = spawn_gated(body, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "stop.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("stop", "1");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/stop.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_completed_at_least(&session, gid.as_str(), 1).await;
    tokio::time::timeout(std::time::Duration::from_secs(3), session.wait_stopped())
        .await
        .expect("stop=1 must halt");
    let got = std::fs::read(dir.path().join("stop.bin")).unwrap_or_default();
    assert_ne!(got.as_slice(), body, "stop must not finish dest");
    assert!(got.len() < body.len(), "stop prefix: {}", got.len());
}

#[tokio::test]
async fn stop_with_process_alive_dest_match() {
    let body: &'static [u8] = b"stop-with-process-alive";
    let http_port = spawn_body(body).await;
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "swp.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("stop-with-process", child.id().to_string());
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{http_port}/swp.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert!(!session.is_stopped());
    assert_eq!(std::fs::read(dir.path().join("swp.bin")).unwrap(), body);
    let _ = child.kill();
    let _ = child.wait();
}

#[tokio::test]
async fn stop_with_process_dead_halts_gated() {
    let body: &'static [u8] = Box::leak(vec![0x3Cu8; 32 * 1024].into_boxed_slice());
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port = spawn_gated(body, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "swpd.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("stop-with-process", child.id().to_string());
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/swpd.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_completed_at_least(&session, gid.as_str(), 1).await;
    let _ = child.kill();
    let _ = child.wait();
    tokio::time::timeout(std::time::Duration::from_secs(2), session.wait_stopped())
        .await
        .expect("dead parent PID must halt");
    let got = std::fs::read(dir.path().join("swpd.bin")).unwrap_or_default();
    assert_ne!(got.as_slice(), body, "dead PID must not finish dest");
    assert!(got.len() < body.len(), "dead PID prefix: {}", got.len());
}

async fn rpc_call_auth(url: &str, method: &str, params: Value, auth: Option<&str>) -> (u16, Value) {
    let client = reqwest::Client::new();
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let mut req = client
            .post(url)
            .header("Content-Type", "application/json")
            .body(
                serde_json::to_vec(&json!({
                    "jsonrpc": "2.0",
                    "id": "1",
                    "method": method,
                    "params": params
                }))
                .unwrap(),
            );
        if let Some(a) = auth {
            req = req.header("Authorization", a);
        }
        let Ok(res) = req.send().await else {
            continue;
        };
        let status = res.status().as_u16();
        let bytes = res.bytes().await.unwrap();
        let v = serde_json::from_slice(&bytes).unwrap_or(json!(null));
        return (status, v);
    }
    panic!("rpc {method} did not answer");
}

#[tokio::test]
async fn rpc_user_passwd_rejects_missing_and_wrong_basic() {
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("rpc-user", "aria");
    opts.set("rpc-passwd", "pw-test");
    let session = Session::new(opts).unwrap();
    let port = spawn_rpc(session).await;
    let url = format!("http://127.0.0.1:{port}/jsonrpc");
    let (st, _) = rpc_call_auth(&url, "aria2.getVersion", json!([]), None).await;
    assert_eq!(st, 401, "missing Basic");
    let (st, _) = rpc_call_auth(
        &url,
        "aria2.getVersion",
        json!([]),
        Some("Basic YXJpYTp3cm9uZw=="),
    )
    .await;
    assert_eq!(st, 401, "wrong password");
    let (st, v) = rpc_call_auth(
        &url,
        "aria2.getVersion",
        json!([]),
        Some("Basic YXJpYTpwdy10ZXN0"),
    )
    .await;
    assert_eq!(st, 200, "good Basic: {v}");
    assert!(
        v.get("result").and_then(|r| r.get("version")).is_some(),
        "good Basic body: {v}"
    );
}

#[tokio::test]
async fn rpc_user_passwd_add_uri_dest_match() {
    let body: &'static [u8] = b"rpc-user-passwd-dest-match";
    let http_port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("rpc-user", "aria");
    opts.set("rpc-passwd", "pw-test");
    let session = Session::new(opts).unwrap();
    let rpc_port = spawn_rpc(Arc::clone(&session)).await;
    let url = format!("http://127.0.0.1:{rpc_port}/jsonrpc");
    let (st, v) = rpc_call_auth(
        &url,
        "aria2.addUri",
        json!([
            [format!("http://127.0.0.1:{http_port}/user.bin")],
            {"out": "user.bin"}
        ]),
        Some("Basic YXJpYTpwdy10ZXN0"),
    )
    .await;
    assert_eq!(st, 200, "addUri Basic: {v}");
    let gid = v
        .get("result")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();
    assert!(!gid.is_empty(), "addUri with Basic: {v}");
    for _ in 0..80 {
        let st = session.tell_status(&gid).await.unwrap();
        if st.get("status").and_then(|s| s.as_str()) == Some("complete") {
            assert_eq!(std::fs::read(dir.path().join("user.bin")).unwrap(), body);
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("rpc-user addUri did not dest-match");
}

async fn spawn_gated(body: &'static [u8], open: std::sync::Arc<std::sync::atomic::AtomicBool>, gate: std::sync::Arc<tokio::sync::Notify>) -> u16 {
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
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                if open.load(std::sync::atomic::Ordering::SeqCst) {
                    let _ = s.write_all(body).await;
                    return;
                }
                let first = body.len().min(2048);
                let _ = s.write_all(&body[..first]).await;
                while !open.load(std::sync::atomic::Ordering::SeqCst) {
                    gate.notified().await;
                }
                let _ = s.write_all(&body[first..]).await;
            });
        }
    });
    port
}

async fn wait_completed_at_least(session: &Session, gid: &str, n: u64) {
    let t0 = tokio::time::Instant::now();
    loop {
        if let Ok(st) = session.tell_status(gid).await {
            let c = st
                .get("completedLength")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            if c >= n {
                return;
            }
        }
        if t0.elapsed() > std::time::Duration::from_secs(4) {
            panic!("never got first chunk");
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn pause_unpause_http_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0xABu8; 16 * 1024].into_boxed_slice());
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port = spawn_gated(body, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("p.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "p.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("timeout", "15");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/p.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_completed_at_least(&session, gid.as_str(), 2048).await;
    session.pause(gid.as_str()).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    let st = session.tell_status(gid.as_str()).await.unwrap();
    assert_eq!(st.get("status").and_then(|v| v.as_str()), Some("paused"));
    let mid = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
    assert!(mid < body.len() as u64, "pause must stop before dest-match");
    open.store(true, std::sync::atomic::Ordering::SeqCst);
    gate.notify_waiters();
    session.unpause(gid.as_str()).await.unwrap();
    for _ in 0..120 {
        let st = session.tell_status(gid.as_str()).await.unwrap();
        if st.get("status").and_then(|v| v.as_str()) == Some("complete") {
            assert_eq!(std::fs::read(&dest).unwrap(), body);
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("unpause did not dest-match");
}

#[tokio::test]
async fn pause_all_unpause_all() {
    let body: &'static [u8] = Box::leak(vec![0x11u8; 16 * 1024].into_boxed_slice());
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port = spawn_gated(body, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "a.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/a.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_completed_at_least(&session, gid.as_str(), 2048).await;
    session.pause_all().await.unwrap();
    let st = session.tell_status(gid.as_str()).await.unwrap();
    assert_eq!(st.get("status").and_then(|v| v.as_str()), Some("paused"));
    open.store(true, std::sync::atomic::Ordering::SeqCst);
    gate.notify_waiters();
    session.unpause_all().await.unwrap();
    for _ in 0..120 {
        let st = session.tell_status(gid.as_str()).await.unwrap();
        if st.get("status").and_then(|v| v.as_str()) == Some("complete") {
            assert_eq!(std::fs::read(dir.path().join("a.bin")).unwrap(), body);
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("unpauseAll did not dest-match");
}

#[tokio::test]
async fn get_version_shape() {
    assert_eq!(aria2_rust::VERSION, "0.1.0");
    assert!(aria2_rust::USER_AGENT.contains("aria2/"));
}

#[tokio::test]
async fn get_option_returns_out_and_dir() {
    let body: &'static [u8] = b"opt-payload";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "opt.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/opt.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    let got = session.get_option(gid.as_str()).await.unwrap();
    let dir_s = dir.path().to_string_lossy().into_owned();
    assert_eq!(got.get("out").and_then(|v| v.as_str()), Some("opt.bin"));
    assert_eq!(got.get("dir").and_then(|v| v.as_str()), Some(dir_s.as_str()));
}

#[tokio::test]
async fn change_option_checksum_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0xCDu8; 16 * 1024].into_boxed_slice());
    let digest = hex::encode(sha1::Sha1::digest(body));
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port = spawn_gated(body, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("sum.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "sum.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("timeout", "15");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/sum.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_completed_at_least(&session, gid.as_str(), 2048).await;
    let mut extra = OptionSet::new();
    extra.set("checksum", format!("sha-1={digest}"));
    session.change_option(gid.as_str(), extra).await.unwrap();
    let got = session.get_option(gid.as_str()).await.unwrap();
    let want = format!("sha-1={digest}");
    assert_eq!(got.get("checksum").and_then(|v| v.as_str()), Some(want.as_str()));
    open.store(true, std::sync::atomic::Ordering::SeqCst);
    gate.notify_waiters();
    for _ in 0..120 {
        let st = session.tell_status(gid.as_str()).await.unwrap();
        if st.get("status").and_then(|v| v.as_str()) == Some("complete") {
            assert_eq!(std::fs::read(&dest).unwrap(), body);
            return;
        }
        if st.get("status").and_then(|v| v.as_str()) == Some("error") {
            panic!("checksum dest-match failed: {st}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("changeOption checksum did not dest-match");
}

#[tokio::test]
async fn change_option_wrong_checksum_rejected() {
    let body: &'static [u8] = Box::leak(vec![0x11u8; 16 * 1024].into_boxed_slice());
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port = spawn_gated(body, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "bad.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("timeout", "15");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/bad.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_completed_at_least(&session, gid.as_str(), 2048).await;
    let mut extra = OptionSet::new();
    extra.set("checksum", "sha-1=0000000000000000000000000000000000000000");
    session.change_option(gid.as_str(), extra).await.unwrap();
    open.store(true, std::sync::atomic::Ordering::SeqCst);
    gate.notify_waiters();
    for _ in 0..120 {
        let st = session.tell_status(gid.as_str()).await.unwrap();
        if st.get("status").and_then(|v| v.as_str()) == Some("error") {
            let msg = st
                .get("errorMessage")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            assert!(msg.contains("checksum"), "error must mention checksum: {msg}");
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("wrong checksum must fail");
}

#[tokio::test]
async fn change_global_option_dir_dest_match() {
    let body: &'static [u8] = b"global-dir-payload";
    let port = spawn_body(body).await;
    let dir1 = tempfile::tempdir().unwrap();
    let dir2 = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir1.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    let session = Session::new(opts).unwrap();
    let d1 = dir1.path().to_string_lossy().into_owned();
    let d2 = dir2.path().to_string_lossy().into_owned();
    let g0 = session.get_global_option().await;
    assert_eq!(g0.get("dir").and_then(|v| v.as_str()), Some(d1.as_str()));
    let mut extra = OptionSet::new();
    extra.set("dir", d2.clone());
    extra.set("out", "g.bin");
    session.change_global_option(extra).await.unwrap();
    let g1 = session.get_global_option().await;
    assert_eq!(g1.get("dir").and_then(|v| v.as_str()), Some(d2.as_str()));
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/g.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    for _ in 0..80 {
        let st = session.tell_status(gid.as_str()).await.unwrap();
        if st.get("status").and_then(|v| v.as_str()) == Some("complete") {
            let dest = dir2.path().join("g.bin");
            assert_eq!(std::fs::read(&dest).unwrap(), body);
            assert!(!dir1.path().join("g.bin").exists());
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("changeGlobalOption dir did not dest-match");
}

#[tokio::test]
async fn get_files_get_uris_http_dest_match() {
    let body: &'static [u8] = b"files-uris-payload-bytes";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("fu.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "fu.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    let session = Session::new(opts).unwrap();
    let used = format!("http://127.0.0.1:{port}/fu.bin");
    let waiting = "http://127.0.0.1:1/unused.bin";
    let gid = session
        .add_uri_and_start(vec![used.clone(), waiting.to_string()], OptionSet::new())
        .await
        .unwrap();
    for _ in 0..80 {
        let st = session.tell_status(gid.as_str()).await.unwrap();
        if st.get("status").and_then(|v| v.as_str()) == Some("complete") {
            let files = session.get_files(gid.as_str()).await.unwrap();
            let arr = files.as_array().expect("files array");
            assert_eq!(arr.len(), 1);
            let f = &arr[0];
            assert_eq!(f.get("index").and_then(|v| v.as_str()), Some("1"));
            assert_eq!(f.get("selected").and_then(|v| v.as_str()), Some("true"));
            let path = f.get("path").and_then(|v| v.as_str()).unwrap();
            let want = dest.to_string_lossy().into_owned();
            assert_eq!(path, want);
            assert_eq!(std::fs::read(path).unwrap(), body);
            let len: u64 = f
                .get("length")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .unwrap();
            let done: u64 = f
                .get("completedLength")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .unwrap();
            assert_eq!(len, body.len() as u64);
            assert_eq!(done, body.len() as u64);
            let uris = session.get_uris(gid.as_str()).await.unwrap();
            let uarr = uris.as_array().expect("uris array");
            assert_eq!(uarr.len(), 2);
            assert_eq!(uarr[0].get("uri").and_then(|v| v.as_str()), Some(used.as_str()));
            assert_eq!(uarr[0].get("status").and_then(|v| v.as_str()), Some("used"));
            assert_eq!(uarr[1].get("uri").and_then(|v| v.as_str()), Some(waiting));
            assert_eq!(uarr[1].get("status").and_then(|v| v.as_str()), Some("waiting"));
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("getFiles dest-match did not complete");
}

#[tokio::test]
async fn get_session_info_id() {
    let opts = OptionSet::with_defaults();
    let session = Session::new(opts).unwrap();
    let info = session.get_session_info();
    let id = info.get("sessionId").and_then(|v| v.as_str()).unwrap();
    assert_eq!(id.len(), 16);
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(session.get_session_info().get("sessionId").and_then(|v| v.as_str()), Some(id));
}

#[tokio::test]
async fn save_session_input_file_resume_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0x3Cu8; 16 * 1024].into_boxed_slice());
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port = spawn_gated(body, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("sess.bin");
    let session_path = dir.path().join("aria2.session");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "sess.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("timeout", "15");
    opts.set("save-session", session_path.display().to_string());
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/sess.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_completed_at_least(&session, gid.as_str(), 2048).await;
    session.pause(gid.as_str()).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    assert!(std::fs::metadata(&dest).unwrap().len() < body.len() as u64);
    session.save_session().await.unwrap();
    let saved = std::fs::read_to_string(&session_path).unwrap();
    assert!(saved.contains("sess.bin") || saved.contains("/sess.bin"));
    assert!(saved.contains("pause=true"));
    assert!(saved.contains(&format!("gid={}", gid.as_str())));

    open.store(true, std::sync::atomic::Ordering::SeqCst);
    gate.notify_waiters();

    let mut opts2 = OptionSet::with_defaults();
    opts2.set("input-file", session_path.display().to_string());
    opts2.set("continue", "true");
    opts2.set("file-allocation", "none");
    opts2.set("split", "1");
    opts2.set("timeout", "15");
    let session2 = Session::new(opts2).unwrap();
    let n = session2.load_input_file().await.unwrap();
    assert_eq!(n, 1);
    let st = session2.tell_status(gid.as_str()).await.unwrap();
    assert_eq!(st.get("status").and_then(|v| v.as_str()), Some("paused"));
    session2.unpause(gid.as_str()).await.unwrap();
    for _ in 0..120 {
        let st = session2.tell_status(gid.as_str()).await.unwrap();
        if st.get("status").and_then(|v| v.as_str()) == Some("complete") {
            assert_eq!(std::fs::read(&dest).unwrap(), body);
            return;
        }
        if st.get("status").and_then(|v| v.as_str()) == Some("error") {
            panic!("input-file resume failed: {st}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("saveSession/input-file did not dest-match");
}

#[tokio::test]
async fn deferred_input_false_queues_waiting() {
    let body_a: &'static [u8] = b"deferred-false-a";
    let body_b: &'static [u8] = b"deferred-false-b";
    let port_a = spawn_body(body_a).await;
    let port_b = spawn_body(body_b).await;
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.txt");
    std::fs::write(
        &input,
        format!(
            "http://127.0.0.1:{port_a}/a.bin\n  out=a.bin\nhttp://127.0.0.1:{port_b}/b.bin\n  out=b.bin\n"
        ),
    )
    .unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("max-concurrent-downloads", "1");
    opts.set("deferred-input", "false");
    opts.set("input-file", input.display().to_string());
    let session = Session::new(opts).unwrap();
    let n = session.load_input_file().await.unwrap();
    assert_eq!(n, 2);
    assert_eq!(session.tell_active().await.len(), 1);
    assert_eq!(session.tell_waiting(0, 10).await.len(), 1);
    assert!(!session.has_deferred().await);
    for _ in 0..80 {
        if session.tell_active().await.is_empty() && session.tell_waiting(0, 10).await.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(std::fs::read(dir.path().join("a.bin")).unwrap(), body_a);
    assert_eq!(std::fs::read(dir.path().join("b.bin")).unwrap(), body_b);
}

#[tokio::test]
async fn deferred_input_true_second_not_queued_then_dest_match() {
    let body_a: &'static [u8] = Box::leak(vec![0xA1u8; 16 * 1024].into_boxed_slice());
    let body_b: &'static [u8] = b"deferred-true-b";
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port_a = spawn_gated(body_a, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let port_b = spawn_body(body_b).await;
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.txt");
    std::fs::write(
        &input,
        format!(
            "http://127.0.0.1:{port_a}/a.bin\n  out=a.bin\nhttp://127.0.0.1:{port_b}/b.bin\n  out=b.bin\n"
        ),
    )
    .unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("max-concurrent-downloads", "1");
    opts.set("deferred-input", "true");
    opts.set("input-file", input.display().to_string());
    let session = Session::new(opts).unwrap();
    let n = session.load_input_file().await.unwrap();
    assert_eq!(n, 1, "only one slot at startup");
    assert_eq!(session.tell_active().await.len(), 1);
    assert!(
        session.tell_waiting(0, 10).await.is_empty(),
        "second URI must stay unread"
    );
    assert!(session.has_deferred().await);
    assert!(!dir.path().join("b.bin").exists());
    open.store(true, std::sync::atomic::Ordering::SeqCst);
    gate.notify_waiters();
    for _ in 0..80 {
        if session.tell_active().await.is_empty()
            && session.tell_waiting(0, 10).await.is_empty()
            && !session.has_deferred().await
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(std::fs::read(dir.path().join("a.bin")).unwrap(), body_a);
    assert_eq!(std::fs::read(dir.path().join("b.bin")).unwrap(), body_b);
}

#[tokio::test]
async fn deferred_input_disabled_by_save_session() {
    let body_a: &'static [u8] = b"deferred-ss-a";
    let body_b: &'static [u8] = b"deferred-ss-b";
    let port_a = spawn_body(body_a).await;
    let port_b = spawn_body(body_b).await;
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.txt");
    std::fs::write(
        &input,
        format!(
            "http://127.0.0.1:{port_a}/a.bin\n  out=a.bin\nhttp://127.0.0.1:{port_b}/b.bin\n  out=b.bin\n"
        ),
    )
    .unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("max-concurrent-downloads", "1");
    opts.set("deferred-input", "true");
    opts.set("save-session", dir.path().join("s").display().to_string());
    opts.set("input-file", input.display().to_string());
    let session = Session::new(opts).unwrap();
    let n = session.load_input_file().await.unwrap();
    assert_eq!(n, 2, "save-session disables deferred-input");
    assert!(!session.has_deferred().await);
    assert_eq!(
        session.tell_active().await.len() + session.tell_waiting(0, 10).await.len(),
        2
    );
    for _ in 0..80 {
        if session.tell_active().await.is_empty() && session.tell_waiting(0, 10).await.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(std::fs::read(dir.path().join("a.bin")).unwrap(), body_a);
    assert_eq!(std::fs::read(dir.path().join("b.bin")).unwrap(), body_b);
}

#[tokio::test]
async fn change_uri_swaps_source_dest_match() {
    let body: &'static [u8] = b"change-uri-real-payload";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("chg.bin");
    let dummy = "http://127.0.0.1:1/missing.bin";
    let real = format!("http://127.0.0.1:{port}/chg.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "chg.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("timeout", "8");
    opts.set("connect-timeout", "1");
    let session = Session::new(opts).unwrap();
    let mut extra = OptionSet::new();
    extra.set("pause", "true");
    let gid = session
        .add_uri_and_start(vec![dummy.to_string()], extra)
        .await
        .unwrap();
    let st = session.tell_status(gid.as_str()).await.unwrap();
    assert_eq!(st.get("status").and_then(|v| v.as_str()), Some("paused"));
    let before = session.get_uris(gid.as_str()).await.unwrap();
    assert_eq!(
        before.as_array().unwrap()[0].get("uri").and_then(|v| v.as_str()),
        Some(dummy)
    );
    let (deleted, added) = session
        .change_uri(
            gid.as_str(),
            1,
            vec![dummy.to_string()],
            vec![real.clone()],
            None,
        )
        .await
        .unwrap();
    assert_eq!((deleted, added), (1, 1));
    let after = session.get_uris(gid.as_str()).await.unwrap();
    let arr = after.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0].get("uri").and_then(|v| v.as_str()), Some(real.as_str()));
    assert_eq!(arr[0].get("status").and_then(|v| v.as_str()), Some("used"));
    session.unpause(gid.as_str()).await.unwrap();
    for _ in 0..80 {
        let st = session.tell_status(gid.as_str()).await.unwrap();
        if st.get("status").and_then(|v| v.as_str()) == Some("complete") {
            assert_eq!(std::fs::read(&dest).unwrap(), body);
            return;
        }
        if st.get("status").and_then(|v| v.as_str()) == Some("error") {
            panic!("changeUri dest-match failed: {st}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("changeUri did not dest-match");
}

#[tokio::test]
async fn get_servers_http_dest_match() {
    let body: &'static [u8] = b"get-servers-payload";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("gs.bin");
    let uri = format!("http://127.0.0.1:{port}/gs.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "gs.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(vec![uri.clone()], OptionSet::new())
        .await
        .unwrap();
    for _ in 0..80 {
        let st = session.tell_status(gid.as_str()).await.unwrap();
        if st.get("status").and_then(|v| v.as_str()) == Some("complete") {
            assert_eq!(std::fs::read(&dest).unwrap(), body);
            let servers = session.get_servers(gid.as_str()).await.unwrap();
            let arr = servers.as_array().expect("files array");
            assert_eq!(arr.len(), 1);
            assert_eq!(arr[0].get("index").and_then(|v| v.as_str()), Some("1"));
            let sv = arr[0]
                .get("servers")
                .and_then(|v| v.as_array())
                .expect("servers");
            assert!(
                sv.iter().any(|s| {
                    s.get("uri").and_then(|v| v.as_str()) == Some(uri.as_str())
                        && s.get("currentUri").and_then(|v| v.as_str()) == Some(uri.as_str())
                }),
                "getServers must list the HTTP URI we dest-matched: {servers}"
            );
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("getServers dest-match did not complete");
}

#[tokio::test]
async fn change_position_max_concurrent_dest_match() {
    let body_a: &'static [u8] = Box::leak(vec![0xAAu8; 16 * 1024].into_boxed_slice());
    let body_b: &'static [u8] = b"front-of-queue-payload";
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port_a = spawn_gated(body_a, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let port_b = spawn_body(body_b).await;
    let dir = tempfile::tempdir().unwrap();
    let dest_a = dir.path().join("a.bin");
    let dest_b = dir.path().join("b.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("timeout", "15");
    opts.set("max-concurrent-downloads", "1");
    let session = Session::new(opts).unwrap();
    let mut extra_a = OptionSet::new();
    extra_a.set("pause", "true");
    extra_a.set("out", "a.bin");
    let gid_a = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port_a}/a.bin")], extra_a)
        .await
        .unwrap();
    let mut extra_b = OptionSet::new();
    extra_b.set("pause", "true");
    extra_b.set("out", "b.bin");
    let gid_b = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port_b}/b.bin")], extra_b)
        .await
        .unwrap();
    let pos = session
        .change_position(gid_b.as_str(), 0, "POS_SET")
        .await
        .unwrap();
    assert_eq!(pos, 0);
    session.unpause_all().await.unwrap();
    for _ in 0..80 {
        let st = session.tell_status(gid_b.as_str()).await.unwrap();
        if st.get("status").and_then(|v| v.as_str()) == Some("complete") {
            assert_eq!(std::fs::read(&dest_b).unwrap(), body_b);
            let sta = session.tell_status(gid_a.as_str()).await.unwrap();
            let sa = sta.get("status").and_then(|v| v.as_str()).unwrap_or("");
            assert!(
                sa == "waiting" || sa == "active" || sa == "paused",
                "A must not finish before B: {sa}"
            );
            assert_ne!(sa, "complete");
            assert!(
                !dest_a.exists()
                    || std::fs::metadata(&dest_a).map(|m| m.len()).unwrap_or(0) < body_a.len() as u64
            );
            open.store(true, std::sync::atomic::Ordering::SeqCst);
            gate.notify_waiters();
            for _ in 0..80 {
                let sta = session.tell_status(gid_a.as_str()).await.unwrap();
                if sta.get("status").and_then(|v| v.as_str()) == Some("complete") {
                    assert_eq!(std::fs::read(&dest_a).unwrap(), body_a);
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            panic!("A did not dest-match after B");
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("changePosition did not dest-match B first");
}

#[tokio::test]
async fn remove_purge_download_result_keeps_dest() {
    let body1: &'static [u8] = b"purge-keep-dest-one";
    let body2: &'static [u8] = b"purge-keep-dest-two";
    let port1 = spawn_body(body1).await;
    let port2 = spawn_body(body2).await;
    let dir = tempfile::tempdir().unwrap();
    let dest1 = dir.path().join("p1.bin");
    let dest2 = dir.path().join("p2.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    let session = Session::new(opts).unwrap();
    let mut e1 = OptionSet::new();
    e1.set("out", "p1.bin");
    let gid1 = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port1}/p1.bin")], e1)
        .await
        .unwrap();
    wait_rpc_complete(&session, gid1.as_str()).await;
    assert_eq!(std::fs::read(&dest1).unwrap(), body1);
    let stopped = session.tell_stopped(0, 100).await;
    assert!(
        stopped.iter().any(|v| v.get("gid").and_then(|g| g.as_str()) == Some(gid1.as_str())),
        "complete gid must be in tellStopped"
    );
    session.remove_download_result(gid1.as_str()).await.unwrap();
    assert!(session.tell_status(gid1.as_str()).await.is_err());
    assert_eq!(std::fs::read(&dest1).unwrap(), body1, "removeDownloadResult must keep dest");
    let mut e2 = OptionSet::new();
    e2.set("out", "p2.bin");
    let gid2 = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port2}/p2.bin")], e2)
        .await
        .unwrap();
    wait_rpc_complete(&session, gid2.as_str()).await;
    assert_eq!(std::fs::read(&dest2).unwrap(), body2);
    session.purge_download_result().await.unwrap();
    assert!(session.tell_status(gid2.as_str()).await.is_err());
    assert!(session.tell_stopped(0, 100).await.is_empty());
    assert_eq!(std::fs::read(&dest1).unwrap(), body1);
    assert_eq!(std::fs::read(&dest2).unwrap(), body2, "purgeDownloadResult must keep dest");
}

async fn spawn_slow_range(body: &'static [u8]) -> u16 {
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
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                        slice.len(),
                        body.len()
                    );
                    let _ = s.write_all(hdr.as_bytes()).await;
                    for chunk in slice.chunks(128) {
                        if s.write_all(chunk).await.is_err() {
                            return;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(15)).await;
                    }
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

async fn wait_dest_at_least(path: &std::path::Path, n: usize) {
    for _ in 0..200 {
        if path.exists() {
            if let Ok(b) = std::fs::read(path) {
                if b.len() >= n {
                    return;
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("dest did not reach {n} bytes: {path:?}");
}

#[tokio::test]
async fn force_remove_aborts_mid_piece() {
    let body: &'static [u8] = Box::leak(vec![0xABu8; 8192].into_boxed_slice());
    let port = spawn_slow_range(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("fr.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "fr.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("min-split-size", "1");
    opts.set("piece-length", "4096");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port}/fr.bin")], OptionSet::new())
        .await
        .unwrap();
    wait_completed_at_least(&session, gid.as_str(), 200).await;
    session.force_remove(gid.as_str()).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    let got = std::fs::read(&dest).unwrap_or_default();
    assert!(got.len() < body.len(), "forceRemove must not finish dest: {}", got.len());
    assert!(got.len() < 4096, "forceRemove aborts current piece: {}", got.len());
    assert!(got.iter().all(|&b| b == 0xAB) || got.is_empty() || got == body[..got.len()], "prefix dest-match");
    let st = session.tell_status(gid.as_str()).await.unwrap();
    assert_eq!(st.get("status").and_then(|v| v.as_str()), Some("removed"));
}

#[tokio::test]
async fn remove_finishes_current_piece_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0xCDu8; 8192].into_boxed_slice());
    let port = spawn_slow_range(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("rm.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "rm.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("min-split-size", "1");
    opts.set("piece-length", "4096");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port}/rm.bin")], OptionSet::new())
        .await
        .unwrap();
    wait_dest_at_least(&dest, 200).await;
    session.remove(gid.as_str()).await.unwrap();
    for _ in 0..80 {
        if dest.exists() {
            if let Ok(b) = std::fs::read(&dest) {
                if b.len() >= 4096 {
                    break;
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let got = std::fs::read(&dest).unwrap();
    assert_eq!(&got[..4096], &body[..4096], "remove finishes current piece dest-match");
    assert!(got.len() < body.len(), "remove must not start next piece: {}", got.len());
    let st = session.tell_status(gid.as_str()).await.unwrap();
    assert_eq!(st.get("status").and_then(|v| v.as_str()), Some("removed"));
}

async fn wait_rpc_complete(session: &Session, gid: &str) {
    for _ in 0..80 {
        let st = session.tell_status(gid).await.unwrap();
        if st.get("status").and_then(|v| v.as_str()) == Some("complete") {
            return;
        }
        if st.get("status").and_then(|v| v.as_str()) == Some("error") {
            panic!("download error: {st}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("download did not complete: {gid}");
}

#[tokio::test]
async fn max_overall_download_limit_two_jobs_dest_match() {
    let body_a: &'static [u8] = Box::leak(vec![0x11u8; 2000].into_boxed_slice());
    let body_b: &'static [u8] = Box::leak(vec![0x22u8; 2000].into_boxed_slice());
    let port_a = spawn_body(body_a).await;
    let port_b = spawn_body(body_b).await;
    let dir = tempfile::tempdir().unwrap();
    let dest_a = dir.path().join("oa.bin");
    let dest_b = dir.path().join("ob.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("max-concurrent-downloads", "2");
    opts.set("max-overall-download-limit", "2000");
    let session = Session::new(opts).unwrap();
    let mut ea = OptionSet::new();
    ea.set("out", "oa.bin");
    let mut eb = OptionSet::new();
    eb.set("out", "ob.bin");
    let t0 = std::time::Instant::now();
    let gid_a = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port_a}/oa.bin")], ea)
        .await
        .unwrap();
    let gid_b = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port_b}/ob.bin")], eb)
        .await
        .unwrap();
    for _ in 0..200 {
        let sa = session.tell_status(gid_a.as_str()).await.unwrap();
        let sb = session.tell_status(gid_b.as_str()).await.unwrap();
        let a_ok = sa.get("status").and_then(|v| v.as_str()) == Some("complete");
        let b_ok = sb.get("status").and_then(|v| v.as_str()) == Some("complete");
        if a_ok && b_ok {
            assert!(
                t0.elapsed() >= std::time::Duration::from_millis(1400),
                "overall limit must serialize 4000 bytes at 2000 B/s, elapsed {:?}",
                t0.elapsed()
            );
            assert_eq!(std::fs::read(&dest_a).unwrap(), body_a);
            assert_eq!(std::fs::read(&dest_b).unwrap(), body_b);
            return;
        }
        if sa.get("status").and_then(|v| v.as_str()) == Some("error") {
            panic!("A error {sa}");
        }
        if sb.get("status").and_then(|v| v.as_str()) == Some("error") {
            panic!("B error {sb}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("overall-limit jobs did not complete");
}

#[tokio::test]
async fn force_sequential_two_uris_two_dest_match() {
    let body_a: &'static [u8] = b"seq-payload-aaa";
    let body_b: &'static [u8] = b"seq-payload-bbb";
    let port_a = spawn_body(body_a).await;
    let port_b = spawn_body(body_b).await;
    let dir = tempfile::tempdir().unwrap();
    let dest_a = dir.path().join("sa.bin");
    let dest_b = dir.path().join("sb.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("force-sequential", "true");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![
                format!("http://127.0.0.1:{port_a}/sa.bin"),
                format!("http://127.0.0.1:{port_b}/sb.bin"),
            ],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    for _ in 0..80 {
        if dest_a.exists() && dest_b.exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(std::fs::read(&dest_a).unwrap(), body_a);
    assert_eq!(std::fs::read(&dest_b).unwrap(), body_b);
}

#[tokio::test]
async fn force_sequential_false_uris_are_mirrors() {
    let body_a: &'static [u8] = b"mirror-payload-aaa";
    let body_b: &'static [u8] = b"mirror-payload-bbb";
    let port_a = spawn_body(body_a).await;
    let port_b = spawn_body(body_b).await;
    let dir = tempfile::tempdir().unwrap();
    let dest_a = dir.path().join("sa.bin");
    let dest_b = dir.path().join("sb.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("force-sequential", "false");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![
                format!("http://127.0.0.1:{port_a}/sa.bin"),
                format!("http://127.0.0.1:{port_b}/sb.bin"),
            ],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(&dest_a).unwrap(), body_a);
    assert!(!dest_b.exists(), "mirrors must not write second dest");
}

#[tokio::test]
async fn on_download_complete_hook_after_dest_match() {
    let body: &'static [u8] = b"hook-complete-payload";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("hook.bin");
    let marker = dir.path().join("done.txt");
    let hook = dir.path().join("on_complete.sh");
    std::fs::write(
        &hook,
        format!("#!/bin/sh\nprintf '%s\\n' \"$3\" > '{}'\n", marker.display()),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("out", "hook.bin");
    opts.set("on-download-complete", hook.to_string_lossy().into_owned());
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port}/hook.bin")], OptionSet::new())
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    let marked = std::fs::read_to_string(&marker).unwrap();
    assert!(marked.contains("hook.bin"), "{marked}");
}

#[tokio::test]
async fn on_download_error_hook_on_404() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 1024];
                let _ = s.read(&mut buf).await;
                let _ = s.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
            });
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("err.bin");
    let marker = dir.path().join("err.txt");
    let hook = dir.path().join("on_error.sh");
    std::fs::write(
        &hook,
        format!("#!/bin/sh\nprintf '%s\\n' \"$1\" > '{}'\n", marker.display()),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("out", "err.bin");
    opts.set("max-tries", "1");
    opts.set("on-download-error", hook.to_string_lossy().into_owned());
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port}/missing.bin")], OptionSet::new())
        .await
        .unwrap();
    for _ in 0..80 {
        let st = session.tell_status(gid.as_str()).await.unwrap();
        if st.get("status").and_then(|v| v.as_str()) == Some("error") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let marked = std::fs::read_to_string(&marker).unwrap();
    assert!(marked.contains(gid.as_str()), "{marked}");
    assert!(!dest.exists() || std::fs::read(&dest).unwrap() != b"never");
}

#[tokio::test]
async fn on_download_start_hook_before_dest_match() {
    let body: &'static [u8] = b"hook-start-payload";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("start.bin");
    let marker = dir.path().join("start.txt");
    let hook = dir.path().join("on_start.sh");
    std::fs::write(
        &hook,
        format!("#!/bin/sh\nprintf '%s\\n' \"$1\" > '{}'\n", marker.display()),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("out", "start.bin");
    opts.set("on-download-start", hook.to_string_lossy().into_owned());
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port}/start.bin")], OptionSet::new())
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    let marked = std::fs::read_to_string(&marker).unwrap();
    assert!(marked.contains(gid.as_str()), "{marked}");
}

#[tokio::test]
async fn on_download_pause_hook_keeps_prefix() {
    let body: &'static [u8] = Box::leak(vec![0xEEu8; 8192].into_boxed_slice());
    let port = spawn_slow_range(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("pause.bin");
    let marker = dir.path().join("pause.txt");
    let hook = dir.path().join("on_pause.sh");
    std::fs::write(
        &hook,
        format!("#!/bin/sh\nprintf '%s\\n' \"$1\" > '{}'\n", marker.display()),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "pause.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("min-split-size", "1");
    opts.set("piece-length", "4096");
    opts.set("on-download-pause", hook.to_string_lossy().into_owned());
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port}/pause.bin")], OptionSet::new())
        .await
        .unwrap();
    wait_dest_at_least(&dest, 200).await;
    session.pause(gid.as_str()).await.unwrap();
    let marked = std::fs::read_to_string(&marker).unwrap();
    assert!(marked.contains(gid.as_str()), "{marked}");
    let got = std::fs::read(&dest).unwrap_or_default();
    assert!(got.len() < body.len(), "pause must not finish dest: {}", got.len());
    assert!(got.iter().all(|&b| b == 0xEE) || got.is_empty() || got == body[..got.len()]);
    let st = session.tell_status(gid.as_str()).await.unwrap();
    assert_eq!(st.get("status").and_then(|v| v.as_str()), Some("paused"));
}

#[tokio::test]
async fn on_download_stop_hook_after_dest_match() {
    let body: &'static [u8] = b"hook-stop-payload";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("stop.bin");
    let marker = dir.path().join("stop.txt");
    let hook = dir.path().join("on_stop.sh");
    std::fs::write(
        &hook,
        format!("#!/bin/sh\nprintf '%s\\n' \"$3\" > '{}'\n", marker.display()),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("out", "stop.bin");
    opts.set("on-download-stop", hook.to_string_lossy().into_owned());
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port}/stop.bin")], OptionSet::new())
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    let marked = std::fs::read_to_string(&marker).unwrap();
    assert!(marked.contains("stop.bin"), "{marked}");
}

#[tokio::test]
async fn on_download_stop_hook_on_remove() {
    let body: &'static [u8] = Box::leak(vec![0xABu8; 8192].into_boxed_slice());
    let port = spawn_slow_range(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("stprm.bin");
    let marker = dir.path().join("stprm.txt");
    let hook = dir.path().join("on_stop_rm.sh");
    std::fs::write(
        &hook,
        format!("#!/bin/sh\nprintf '%s\\n' \"$1\" > '{}'\n", marker.display()),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "stprm.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("min-split-size", "1");
    opts.set("piece-length", "4096");
    opts.set("on-download-stop", hook.to_string_lossy().into_owned());
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port}/stprm.bin")], OptionSet::new())
        .await
        .unwrap();
    wait_dest_at_least(&dest, 200).await;
    session.force_remove(gid.as_str()).await.unwrap();
    let marked = std::fs::read_to_string(&marker).unwrap();
    assert!(marked.contains(gid.as_str()), "{marked}");
    let got = std::fs::read(&dest).unwrap_or_default();
    assert!(got.len() < body.len());
}

#[tokio::test]
async fn max_download_result_drops_oldest_keeps_dest() {
    let body: &'static [u8] = b"max-result-payload";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("max-download-result", "2");
    opts.set("keep-unfinished-download-result", "false");
    let session = Session::new(opts).unwrap();
    let mut extra_a = OptionSet::new();
    extra_a.set("out", "a.bin");
    let ga = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port}/a.bin")], extra_a)
        .await
        .unwrap();
    wait_rpc_complete(&session, ga.as_str()).await;
    let mut extra_b = OptionSet::new();
    extra_b.set("out", "b.bin");
    let gb = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port}/b.bin")], extra_b)
        .await
        .unwrap();
    wait_rpc_complete(&session, gb.as_str()).await;
    let mut extra_c = OptionSet::new();
    extra_c.set("out", "c.bin");
    let gc = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port}/c.bin")], extra_c)
        .await
        .unwrap();
    wait_rpc_complete(&session, gc.as_str()).await;
    let stopped = session.tell_stopped(0, 10).await;
    assert_eq!(stopped.len(), 2, "{stopped:?}");
    let gids: Vec<&str> = stopped
        .iter()
        .filter_map(|v| v.get("gid").and_then(|g| g.as_str()))
        .collect();
    assert!(!gids.contains(&ga.as_str()), "oldest complete must be dropped");
    assert!(gids.contains(&gb.as_str()) && gids.contains(&gc.as_str()));
    assert_eq!(std::fs::read(dir.path().join("a.bin")).unwrap(), body);
    assert_eq!(std::fs::read(dir.path().join("b.bin")).unwrap(), body);
    assert_eq!(std::fs::read(dir.path().join("c.bin")).unwrap(), body);
}

#[tokio::test]
async fn keep_unfinished_download_result_exceeds_max() {
    let body: &'static [u8] = b"keep-unfin-payload";
    let port_ok = spawn_body(body).await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port_404 = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 512];
                let _ = s.read(&mut buf).await;
                let _ = s
                    .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .await;
            });
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("max-tries", "1");
    opts.set("max-download-result", "0");
    opts.set("keep-unfinished-download-result", "true");
    let session = Session::new(opts).unwrap();
    let mut extra_ok = OptionSet::new();
    extra_ok.set("out", "ok.bin");
    let gok = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port_ok}/ok.bin")], extra_ok)
        .await
        .unwrap();
    let dest_ok = dir.path().join("ok.bin");
    for _ in 0..80 {
        if dest_ok.exists() && std::fs::read(&dest_ok).ok().as_deref() == Some(body) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(std::fs::read(&dest_ok).unwrap(), body);
    let _ = gok;
    let mut extra_err = OptionSet::new();
    extra_err.set("out", "err.bin");
    extra_err.set("max-tries", "1");
    let gerr = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port_404}/missing.bin")], extra_err)
        .await
        .unwrap();
    for _ in 0..80 {
        let st = session.tell_status(gerr.as_str()).await.unwrap();
        if st.get("status").and_then(|v| v.as_str()) == Some("error") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let stopped = session.tell_stopped(0, 10).await;
    let gids: Vec<&str> = stopped
        .iter()
        .filter_map(|v| v.get("gid").and_then(|g| g.as_str()))
        .collect();
    assert!(
        gids.contains(&gerr.as_str()),
        "unfinished error must be kept: {gids:?}"
    );
    assert!(
        !gids.contains(&gok.as_str()),
        "complete must be dropped at max-download-result=0: {gids:?}"
    );
    assert_eq!(std::fs::read(dir.path().join("ok.bin")).unwrap(), body);
}

#[tokio::test]
async fn pause_option_starts_paused_then_unpause_dest_match() {
    let body: &'static [u8] = b"pause-option-payload-bytes";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("pause.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "pause.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    let session = Session::new(opts).unwrap();
    let mut extra = OptionSet::new();
    extra.set("pause", "true");
    let gid = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port}/pause.bin")], extra)
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let st = session.tell_status(gid.as_str()).await.unwrap();
    assert_eq!(st.get("status").and_then(|v| v.as_str()), Some("paused"));
    assert!(!dest.exists() || std::fs::read(&dest).unwrap_or_default() != body);
    session.unpause(gid.as_str()).await.unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn force_save_complete_in_session_dest_match() {
    let body: &'static [u8] = b"force-save-payload-bytes";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("fs.bin");
    let session_path = dir.path().join("aria2.session");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "fs.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("save-session", session_path.display().to_string());
    opts.set("force-save", "true");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port}/fs.bin")], OptionSet::new())
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    session.save_session().await.unwrap();
    let saved = std::fs::read_to_string(&session_path).unwrap();
    assert!(saved.contains(gid.as_str()), "{saved}");
    assert!(saved.contains("fs.bin"), "{saved}");
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn force_save_false_omits_complete_keeps_dest() {
    let body: &'static [u8] = b"no-force-save-payload";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("nfs.bin");
    let session_path = dir.path().join("aria2.session");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "nfs.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("save-session", session_path.display().to_string());
    opts.set("force-save", "false");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port}/nfs.bin")], OptionSet::new())
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    session.save_session().await.unwrap();
    let saved = std::fs::read_to_string(&session_path).unwrap_or_default();
    assert!(!saved.contains(gid.as_str()), "{saved}");
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn save_not_found_true_keeps_404_in_session() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 512];
                let _ = s.read(&mut buf).await;
                let _ = s
                    .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .await;
            });
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let session_path = dir.path().join("aria2.session");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "missing.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("max-tries", "1");
    opts.set("save-session", session_path.display().to_string());
    opts.set("save-not-found", "true");
    opts.set("force-save", "false");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port}/missing.bin")], OptionSet::new())
        .await
        .unwrap();
    for _ in 0..80 {
        let st = session.tell_status(gid.as_str()).await.unwrap();
        if st.get("status").and_then(|v| v.as_str()) == Some("error") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    session.save_session().await.unwrap();
    let saved = std::fs::read_to_string(&session_path).unwrap();
    assert!(saved.contains(gid.as_str()), "{saved}");
}

#[tokio::test]
async fn save_not_found_false_omits_404() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 512];
                let _ = s.read(&mut buf).await;
                let _ = s
                    .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .await;
            });
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let session_path = dir.path().join("aria2.session");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "missing.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("max-tries", "1");
    opts.set("save-session", session_path.display().to_string());
    opts.set("save-not-found", "false");
    opts.set("force-save", "false");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port}/missing.bin")], OptionSet::new())
        .await
        .unwrap();
    for _ in 0..80 {
        let st = session.tell_status(gid.as_str()).await.unwrap();
        if st.get("status").and_then(|v| v.as_str()) == Some("error") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    session.save_session().await.unwrap();
    let saved = std::fs::read_to_string(&session_path).unwrap_or_default();
    assert!(!saved.contains(gid.as_str()), "{saved}");
}

#[tokio::test]
async fn save_session_interval_writes_during_download_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0xCDu8; 16 * 1024].into_boxed_slice());
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port = spawn_gated(body, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("ssi.bin");
    let session_path = dir.path().join("aria2.session");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "ssi.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("timeout", "15");
    opts.set("save-session", session_path.display().to_string());
    opts.set("save-session-interval", "1");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/ssi.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    let t0 = tokio::time::Instant::now();
    let mut saw = false;
    while t0.elapsed() < std::time::Duration::from_secs(4) {
        if dest.exists() {
            let n = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
            if n >= 2048 {
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    while t0.elapsed() < std::time::Duration::from_secs(5) {
        if let Ok(saved) = std::fs::read_to_string(&session_path) {
            if saved.contains(gid.as_str()) {
                saw = true;
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(saw, "save-session-interval must write gid while download is active");
    assert!(
        std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0) < body.len() as u64,
        "session file must appear before dest-match"
    );
    open.store(true, std::sync::atomic::Ordering::SeqCst);
    gate.notify_waiters();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn gid_option_dest_match() {
    let body: &'static [u8] = b"gid-option-payload-bytes";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    let session = Session::new(opts).unwrap();
    let mut extra = OptionSet::new();
    extra.set("gid", "0123456789ABCDEF");
    extra.set("out", "g.bin");
    extra.set("file-allocation", "none");
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/g.bin")],
            extra,
        )
        .await
        .unwrap();
    assert_eq!(gid.as_str(), "0123456789abcdef");
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("g.bin")).unwrap(), body);
    let st = session.tell_status(gid.as_str()).await.unwrap();
    assert_eq!(st.get("gid").and_then(|v| v.as_str()), Some("0123456789abcdef"));
}

#[tokio::test]
async fn gid_invalid_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    let session = Session::new(opts).unwrap();
    let mut extra = OptionSet::new();
    extra.set("gid", "not-a-gid");
    extra.set("out", "bad.bin");
    let err = session
        .add_uri_and_start(vec!["http://127.0.0.1/x".into()], extra)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("GID"), "{err}");
}

#[tokio::test]
async fn gid_duplicate_rejected_first_dest_match() {
    let body: &'static [u8] = b"gid-dup-payload-bytes!!";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    let session = Session::new(opts).unwrap();
    let mut extra = OptionSet::new();
    extra.set("gid", "aaaaaaaaaaaaaaaa");
    extra.set("out", "d1.bin");
    extra.set("file-allocation", "none");
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/d1.bin")],
            extra,
        )
        .await
        .unwrap();
    assert_eq!(gid.as_str(), "aaaaaaaaaaaaaaaa");
    let mut extra2 = OptionSet::new();
    extra2.set("gid", "AAAAAAAAAAAAAAAA");
    extra2.set("out", "d2.bin");
    extra2.set("file-allocation", "none");
    let err = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/d2.bin")],
            extra2,
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("already used"), "{err}");
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("d1.bin")).unwrap(), body);
    assert!(!dir.path().join("d2.bin").exists());
}

#[tokio::test]
async fn conf_path_dir_out_dest_match() {
    let body: &'static [u8] = b"conf-path-payload-bytes";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest_dir = dir.path().join("fromconf");
    std::fs::create_dir_all(&dest_dir).unwrap();
    let conf = dir.path().join("aria2.conf");
    std::fs::write(
        &conf,
        format!(
            "# test conf\ndir={}\nout=from.conf.bin\nfile-allocation=none\nsplit=1\n",
            dest_dir.display()
        ),
    )
    .unwrap();
    let mut opts = OptionSet::with_defaults();
    aria2_rust::options::load_conf(&mut opts, Some(conf.to_str().unwrap()), false).unwrap();
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/c.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    let dest = dest_dir.join("from.conf.bin");
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert!(!dir.path().join("from.conf.bin").exists());
}

#[tokio::test]
async fn conf_path_cli_overrides_dest_match() {
    let body: &'static [u8] = b"conf-override-payload!!";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let conf = dir.path().join("aria2.conf");
    std::fs::write(
        &conf,
        format!("dir={}\nout=wrong.bin\nfile-allocation=none\n", dir.path().display()),
    )
    .unwrap();
    let mut opts = OptionSet::with_defaults();
    aria2_rust::options::load_conf(&mut opts, Some(conf.to_str().unwrap()), false).unwrap();
    opts.set("out", "right.bin");
    opts.set("split", "1");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/r.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("right.bin")).unwrap(), body);
    assert!(!dir.path().join("wrong.bin").exists());
}

#[tokio::test]
async fn no_conf_skips_conf_path_dest_match() {
    let body: &'static [u8] = b"no-conf-payload-bytes!!!";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let conf = dir.path().join("aria2.conf");
    std::fs::write(
        &conf,
        format!("dir={}\nout=skipped.bin\nfile-allocation=none\n", dir.path().display()),
    )
    .unwrap();
    let mut opts = OptionSet::with_defaults();
    aria2_rust::options::load_conf(&mut opts, Some(conf.to_str().unwrap()), true).unwrap();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "used.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/n.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("used.bin")).unwrap(), body);
    assert!(!dir.path().join("skipped.bin").exists());
}

#[test]
fn conf_path_missing_errors() {
    let mut opts = OptionSet::with_defaults();
    let err = aria2_rust::options::load_conf(&mut opts, Some("/no/such/aria2.conf"), false).unwrap_err();
    assert!(err.to_string().contains("No such file") || err.to_string().contains("os error"), "{err}");
}

#[tokio::test]
async fn optimize_concurrent_ab_one_slot_then_dest_match() {
    let body_a: &'static [u8] = Box::leak(vec![0xABu8; 16 * 1024].into_boxed_slice());
    let body_b: &'static [u8] = b"opt-conc-second-payload";
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port_a = spawn_gated(body_a, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let port_b = spawn_body(body_b).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("timeout", "15");
    opts.set("max-concurrent-downloads", "5");
    opts.set("optimize-concurrent-downloads", "1:0");
    opts.set("disk-cache", "0");
    let session = Session::new(opts).unwrap();
    let mut extra_a = OptionSet::new();
    extra_a.set("out", "a.bin");
    extra_a.set("file-allocation", "none");
    let gid_a = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port_a}/a.bin")], extra_a)
        .await
        .unwrap();
    let mut extra_b = OptionSet::new();
    extra_b.set("out", "b.bin");
    extra_b.set("file-allocation", "none");
    let gid_b = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port_b}/b.bin")], extra_b)
        .await
        .unwrap();
    let t0 = tokio::time::Instant::now();
    loop {
        if dir.path().join("a.bin").exists()
            && std::fs::metadata(dir.path().join("a.bin")).map(|m| m.len()).unwrap_or(0) >= 2048
        {
            break;
        }
        if t0.elapsed() > std::time::Duration::from_secs(3) {
            panic!("first download never wrote prefix");
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(session.tell_active().await.len(), 1);
    assert_eq!(session.tell_waiting(0, 10).await.len(), 1);
    assert!(!dir.path().join("b.bin").exists(), "1:0 must hold second download");
    open.store(true, std::sync::atomic::Ordering::SeqCst);
    gate.notify_waiters();
    wait_rpc_complete(&session, gid_a.as_str()).await;
    wait_rpc_complete(&session, gid_b.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("a.bin")).unwrap(), body_a);
    assert_eq!(std::fs::read(dir.path().join("b.bin")).unwrap(), body_b);
}

#[tokio::test]
async fn optimize_concurrent_false_two_slots_dest_match() {
    let body_a: &'static [u8] = Box::leak(vec![0xACu8; 16 * 1024].into_boxed_slice());
    let body_b: &'static [u8] = Box::leak(vec![0xADu8; 16 * 1024].into_boxed_slice());
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port_a = spawn_gated(body_a, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let port_b = spawn_gated(body_b, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("timeout", "15");
    opts.set("max-concurrent-downloads", "2");
    opts.set("optimize-concurrent-downloads", "false");
    opts.set("disk-cache", "0");
    let session = Session::new(opts).unwrap();
    let mut extra_a = OptionSet::new();
    extra_a.set("out", "a.bin");
    extra_a.set("file-allocation", "none");
    let gid_a = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port_a}/a.bin")], extra_a)
        .await
        .unwrap();
    let mut extra_b = OptionSet::new();
    extra_b.set("out", "b.bin");
    extra_b.set("file-allocation", "none");
    let gid_b = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port_b}/b.bin")], extra_b)
        .await
        .unwrap();
    let t0 = tokio::time::Instant::now();
    loop {
        let na = std::fs::metadata(dir.path().join("a.bin")).map(|m| m.len()).unwrap_or(0);
        let nb = std::fs::metadata(dir.path().join("b.bin")).map(|m| m.len()).unwrap_or(0);
        if na >= 2048 && nb >= 2048 {
            break;
        }
        if t0.elapsed() > std::time::Duration::from_secs(3) {
            panic!("false must start both, na={na} nb={nb}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(session.tell_active().await.len(), 2);
    open.store(true, std::sync::atomic::Ordering::SeqCst);
    gate.notify_waiters();
    wait_rpc_complete(&session, gid_a.as_str()).await;
    wait_rpc_complete(&session, gid_b.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("a.bin")).unwrap(), body_a);
    assert_eq!(std::fs::read(dir.path().join("b.bin")).unwrap(), body_b);
}

#[tokio::test]
async fn daemon_parent_exits_rpc_dest_match() {
    let body: &'static [u8] = b"daemon-dest-match-bytes";
    let port_http = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rpc_port = listener.local_addr().unwrap().port();
    drop(listener);
    let bin = env!("CARGO_BIN_EXE_aria2c");
    let t0 = std::time::Instant::now();
    let status = std::process::Command::new(bin)
        .args([
            "--daemon",
            "--no-conf",
            "--enable-rpc",
            "--rpc-listen-all=true",
            "--rpc-allow-origin-all",
            &format!("--rpc-listen-port={rpc_port}"),
            &format!("--dir={}", dir.path().display()),
            "--file-allocation=none",
            "--split=1",
            "--quiet",
        ])
        .status()
        .unwrap();
    assert!(status.success(), "daemon parent must exit 0");
    assert!(
        t0.elapsed() < std::time::Duration::from_secs(3),
        "daemon parent must return immediately, took {:?}",
        t0.elapsed()
    );
    let url = format!("http://127.0.0.1:{rpc_port}/jsonrpc");
    let ver = rpc_call(&url, "aria2.getVersion", json!([])).await;
    assert!(ver.get("result").is_some(), "daemon RPC down: {ver}");
    let add = rpc_call(
        &url,
        "aria2.addUri",
        json!([
            [format!("http://127.0.0.1:{port_http}/d.bin")],
            {"out": "d.bin", "file-allocation": "none", "split": "1"}
        ]),
    )
    .await;
    let gid = add
        .get("result")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    assert_eq!(gid.len(), 16, "{add}");
    let dest = dir.path().join("d.bin");
    for _ in 0..80 {
        if dest.exists() && std::fs::read(&dest).ok().as_deref() == Some(body) {
            let _ = rpc_call(&url, "aria2.forceShutdown", json!([])).await;
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let _ = rpc_call(&url, "aria2.forceShutdown", json!([])).await;
    panic!(
        "daemon dest-match failed: {:?}",
        std::fs::read(&dest).ok()
    );
}

#[tokio::test]
async fn pid_file_rpc_dest_match() {
    let body: &'static [u8] = b"pid-file-dest-match-bytes";
    let port_http = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let pid_path = dir.path().join("aria2c.pid");
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rpc_port = listener.local_addr().unwrap().port();
    drop(listener);
    let bin = env!("CARGO_BIN_EXE_aria2c");
    let mut child = std::process::Command::new(bin)
        .args([
            "--no-conf",
            "--enable-rpc",
            "--rpc-listen-all=true",
            "--rpc-allow-origin-all",
            &format!("--rpc-listen-port={rpc_port}"),
            &format!("--dir={}", dir.path().display()),
            "--file-allocation=none",
            "--split=1",
            "--quiet",
            &format!("--pid-file={}", pid_path.display()),
        ])
        .spawn()
        .unwrap();
    let want_pid = child.id();
    let url = format!("http://127.0.0.1:{rpc_port}/jsonrpc");
    let mut pid_ok = false;
    for _ in 0..80 {
        if let Ok(s) = std::fs::read_to_string(&pid_path) {
            if s.trim() == want_pid.to_string() {
                pid_ok = true;
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(
        pid_ok,
        "--pid-file must write getpid, got {:?}",
        std::fs::read_to_string(&pid_path).ok()
    );
    let add = rpc_call(
        &url,
        "aria2.addUri",
        json!([
            [format!("http://127.0.0.1:{port_http}/pid.bin")],
            {"out": "pid.bin", "file-allocation": "none", "split": "1"}
        ]),
    )
    .await;
    assert!(add.get("result").is_some(), "--pid-file addUri: {add}");
    let dest = dir.path().join("pid.bin");
    for _ in 0..80 {
        if dest.exists() && std::fs::read(&dest).ok().as_deref() == Some(body) {
            let _ = rpc_call(&url, "aria2.forceShutdown", json!([])).await;
            let _ = child.wait();
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let _ = rpc_call(&url, "aria2.forceShutdown", json!([])).await;
    let _ = child.kill();
    panic!(
        "--pid-file dest-match failed: {:?}",
        std::fs::read(&dest).ok()
    );
}

#[tokio::test]
async fn pid_file_unset_no_file() {
    let body: &'static [u8] = b"pid-file-unset-bytes";
    let port_http = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let pid_path = dir.path().join("aria2c.pid");
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rpc_port = listener.local_addr().unwrap().port();
    drop(listener);
    let bin = env!("CARGO_BIN_EXE_aria2c");
    let mut child = std::process::Command::new(bin)
        .args([
            "--no-conf",
            "--enable-rpc",
            "--rpc-listen-all=true",
            "--rpc-allow-origin-all",
            &format!("--rpc-listen-port={rpc_port}"),
            &format!("--dir={}", dir.path().display()),
            "--file-allocation=none",
            "--split=1",
            "--quiet",
        ])
        .spawn()
        .unwrap();
    let url = format!("http://127.0.0.1:{rpc_port}/jsonrpc");
    let mut ready = false;
    for _ in 0..80 {
        let ver = rpc_call(&url, "aria2.getVersion", json!([])).await;
        if ver.get("result").is_some() {
            ready = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(ready, "rpc not up without --pid-file");
    let add = rpc_call(
        &url,
        "aria2.addUri",
        json!([
            [format!("http://127.0.0.1:{port_http}/u.bin")],
            {"out": "u.bin", "file-allocation": "none", "split": "1"}
        ]),
    )
    .await;
    assert!(add.get("result").is_some(), "unset pid-file addUri: {add}");
    let dest = dir.path().join("u.bin");
    for _ in 0..80 {
        if dest.exists() && std::fs::read(&dest).ok().as_deref() == Some(body) {
            assert!(
                !pid_path.exists(),
                "unset --pid-file must not write a pid file"
            );
            let _ = rpc_call(&url, "aria2.forceShutdown", json!([])).await;
            let _ = child.wait();
            let _ = add;
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let _ = rpc_call(&url, "aria2.forceShutdown", json!([])).await;
    let _ = child.kill();
    panic!("unset pid-file dest-match failed");
}

fn proc_nofile_soft(pid: u32) -> Option<u64> {
    let t = std::fs::read_to_string(format!("/proc/{pid}/limits")).ok()?;
    for line in t.lines() {
        if line.starts_with("Max open files") {
            let parts: Vec<_> = line.split_whitespace().collect();
            return parts.get(3)?.parse().ok();
        }
    }
    None
}

#[tokio::test]
async fn rlimit_nofile_rpc_dest_match() {
    let body: &'static [u8] = b"rlimit-nofile-dest-match";
    let port_http = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rpc_port = listener.local_addr().unwrap().port();
    drop(listener);
    let bin = env!("CARGO_BIN_EXE_aria2c");
    let want = 3072u64;
    let mut child = std::process::Command::new(bin)
        .args([
            "--no-conf",
            "--enable-rpc",
            "--rpc-listen-all=true",
            "--rpc-allow-origin-all",
            &format!("--rpc-listen-port={rpc_port}"),
            &format!("--dir={}", dir.path().display()),
            "--file-allocation=none",
            "--split=1",
            "--quiet",
            &format!("--rlimit-nofile={want}"),
        ])
        .spawn()
        .unwrap();
    let pid = child.id();
    let url = format!("http://127.0.0.1:{rpc_port}/jsonrpc");
    let mut ready = false;
    for _ in 0..80 {
        if rpc_call(&url, "aria2.getVersion", json!([]))
            .await
            .get("result")
            .is_some()
        {
            ready = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    if !ready {
        let _ = child.kill();
        panic!("rlimit-nofile RPC never came up");
    }
    let soft = proc_nofile_soft(pid).expect("read /proc/pid/limits");
    assert_eq!(soft, want, "child RLIMIT_NOFILE soft must be {want}, got {soft}");
    let add = rpc_call(
        &url,
        "aria2.addUri",
        json!([
            [format!("http://127.0.0.1:{port_http}/r.bin")],
            {"out": "r.bin", "file-allocation": "none", "split": "1"}
        ]),
    )
    .await;
    let gid = add
        .get("result")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    assert_eq!(gid.len(), 16, "{add}");
    let dest = dir.path().join("r.bin");
    for _ in 0..80 {
        if dest.exists() && std::fs::read(&dest).ok().as_deref() == Some(body) {
            let _ = rpc_call(&url, "aria2.forceShutdown", json!([])).await;
            let _ = child.wait();
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let _ = rpc_call(&url, "aria2.forceShutdown", json!([])).await;
    let _ = child.kill();
    panic!(
        "rlimit-nofile dest-match failed: {:?}",
        std::fs::read(&dest).ok()
    );
}

#[tokio::test]
async fn startup_idle_time_delays_then_dest_match() {
    let body: &'static [u8] = b"startup-idle-dest-match";
    let port = spawn_body(body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("idle.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("out", "idle.bin");
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("startup-idle-time", "1");
    let t0 = std::time::Instant::now();
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{port}/idle.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    assert!(
        !dest.exists() || std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0) == 0,
        "must still be idle at 400ms, dest={:?}",
        dest.exists()
    );
    wait_rpc_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(&dest).unwrap(), body, "idle dest must match");
    assert!(
        t0.elapsed() >= std::time::Duration::from_secs(1),
        "startup-idle-time=1 must delay ~1s, elapsed {:?}",
        t0.elapsed()
    );
}

#[tokio::test]
async fn max_downloads_rejects_second_first_dest_match() {
    let body_a: &'static [u8] = b"max-downloads-a-dest";
    let body_b: &'static [u8] = b"max-downloads-b-must-not-write";
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port_a = spawn_gated(body_a, std::sync::Arc::clone(&open), std::sync::Arc::clone(&gate)).await;
    let port_b = spawn_body(body_b).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().to_string_lossy().into_owned());
    opts.set("file-allocation", "none");
    opts.set("split", "1");
    opts.set("timeout", "15");
    opts.set("max-concurrent-downloads", "5");
    opts.set("max-downloads", "1");
    let session = Session::new(opts).unwrap();
    let mut extra_a = OptionSet::new();
    extra_a.set("out", "a.bin");
    let gid_a = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port_a}/a.bin")], extra_a)
        .await
        .unwrap();
    let mut extra_b = OptionSet::new();
    extra_b.set("out", "b.bin");
    let err = session
        .add_uri_and_start(vec![format!("http://127.0.0.1:{port_b}/b.bin")], extra_b)
        .await;
    assert!(err.is_err(), "second add must fail at max-downloads=1: {err:?}");
    let msg = err.unwrap_err().to_string();
    assert!(
        msg.contains("max-downloads"),
        "error must name max-downloads, got {msg}"
    );
    open.store(true, std::sync::atomic::Ordering::SeqCst);
    gate.notify_waiters();
    wait_rpc_complete(&session, gid_a.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("a.bin")).unwrap(), body_a);
    assert!(
        !dir.path().join("b.bin").exists(),
        "rejected download must not write dest"
    );
}
