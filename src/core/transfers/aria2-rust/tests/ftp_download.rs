//! FTP PASV / PORT / REST dest-match + ftp-user/ftp-passwd/ftp-type/ftp-pasv.
#![forbid(unsafe_code)]

use aria2_rust::ftp::{self, FtpJob};
use aria2_rust::http::HttpProgress;
use aria2_rust::options::OptionSet;
use aria2_rust::session::Session;
use sha1::Digest;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

struct Seen {
    pasv: AtomicBool,
    port: AtomicBool,
    rest: AtomicU64,
    type_i: AtomicBool,
    type_a: AtomicBool,
    user: AtomicU64,
}

impl Seen {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            pasv: AtomicBool::new(false),
            port: AtomicBool::new(false),
            rest: AtomicU64::new(u64::MAX),
            type_i: AtomicBool::new(false),
            type_a: AtomicBool::new(false),
            user: AtomicU64::new(0),
        })
    }
}

async fn spawn_ftp(
    body: &'static [u8],
    user: &'static str,
    pass: &'static str,
) -> (u16, Arc<Seen>) {
    let seen = Seen::new();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen_c = Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let Ok((sock, _)) = listener.accept().await else { break };
            let seen = Arc::clone(&seen_c);
            tokio::spawn(async move {
                if let Err(e) = serve_ctrl(sock, body, user, pass, seen).await {
                    eprintln!("ftp test server: {e}");
                }
            });
        }
    });
    (port, seen)
}

async fn serve_ctrl(
    sock: TcpStream,
    body: &'static [u8],
    user: &str,
    pass: &str,
    seen: Arc<Seen>,
) -> std::io::Result<()> {
    let (rh, mut wh) = sock.into_split();
    let mut rh = BufReader::new(rh);
    wh.write_all(b"220 aria2-rust test ftp\r\n").await?;
    let mut authed = false;
    let mut got_user = false;
    let mut rest = 0u64;
    let mut pasv: Option<TcpListener> = None;
    let mut port_addr: Option<std::net::SocketAddr> = None;
    let mut line = String::new();
    loop {
        line.clear();
        let n = rh.read_line(&mut line).await?;
        if n == 0 {
            break;
        }
        let cmd = line.trim_end_matches(['\r', '\n']);
        let (verb, arg) = match cmd.split_once(' ') {
            Some((v, a)) => (v.to_ascii_uppercase(), a),
            None => (cmd.to_ascii_uppercase(), ""),
        };
        match verb.as_str() {
            "USER" => {
                got_user = arg == user;
                seen.user.fetch_add(1, Ordering::SeqCst);
                wh.write_all(b"331 password\r\n").await?;
            }
            "PASS" => {
                if got_user && arg == pass {
                    authed = true;
                    wh.write_all(b"230 ok\r\n").await?;
                } else {
                    wh.write_all(b"530 login incorrect\r\n").await?;
                }
            }
            "TYPE" => {
                if arg.eq_ignore_ascii_case("I") {
                    seen.type_i.store(true, Ordering::SeqCst);
                }
                if arg.eq_ignore_ascii_case("A") {
                    seen.type_a.store(true, Ordering::SeqCst);
                }
                wh.write_all(b"200 type\r\n").await?;
            }
            "SIZE" => {
                wh.write_all(format!("213 {}\r\n", body.len()).as_bytes())
                    .await?;
            }
            "PASV" => {
                seen.pasv.store(true, Ordering::SeqCst);
                let l = TcpListener::bind("127.0.0.1:0").await?;
                let p = l.local_addr()?.port();
                pasv = Some(l);
                port_addr = None;
                let msg = format!(
                    "227 Entering Passive Mode (127,0,0,1,{},{})\r\n",
                    p >> 8,
                    p & 0xff
                );
                wh.write_all(msg.as_bytes()).await?;
            }
            "PORT" => {
                seen.port.store(true, Ordering::SeqCst);
                let nums: Vec<u16> = arg
                    .split(',')
                    .filter_map(|s| s.trim().parse().ok())
                    .collect();
                if nums.len() >= 6 {
                    let ip = std::net::Ipv4Addr::new(
                        nums[0] as u8,
                        nums[1] as u8,
                        nums[2] as u8,
                        nums[3] as u8,
                    );
                    let p = (nums[4] << 8) | nums[5];
                    port_addr = Some(std::net::SocketAddr::from((ip, p)));
                    pasv = None;
                    wh.write_all(b"200 port\r\n").await?;
                } else {
                    wh.write_all(b"501 bad port\r\n").await?;
                }
            }
            "REST" => {
                rest = arg.parse().unwrap_or(0);
                seen.rest.store(rest, Ordering::SeqCst);
                wh.write_all(b"350 restart\r\n").await?;
            }
            "RETR" => {
                if !authed {
                    wh.write_all(b"530 not logged in\r\n").await?;
                    continue;
                }
                wh.write_all(b"150 opening\r\n").await?;
                let start = rest.min(body.len() as u64) as usize;
                rest = 0;
                let slice = &body[start..];
                let mut data = if let Some(l) = pasv.take() {
                    l.accept().await?.0
                } else if let Some(addr) = port_addr.take() {
                    TcpStream::connect(addr).await?
                } else {
                    wh.write_all(b"425 no data conn\r\n").await?;
                    continue;
                };
                data.write_all(slice).await?;
                let _ = data.shutdown().await;
                wh.write_all(b"226 complete\r\n").await?;
            }
            "QUIT" => {
                wh.write_all(b"221 bye\r\n").await?;
                break;
            }
            "NOOP" => wh.write_all(b"200 ok\r\n").await?,
            _ => wh.write_all(b"502 not implemented\r\n").await?,
        }
    }
    Ok(())
}

fn job(uri: String, dest: std::path::PathBuf, opts: OptionSet) -> FtpJob {
    let (_tx, rx) = watch::channel(false);
    FtpJob {
        uris: vec![uri],
        dest,
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    }
}

#[tokio::test]
async fn ftp_pasv_retr_dest_match() {
    let body: &'static [u8] = Box::leak((0u8..=255).cycle().take(24 * 1024).collect::<Vec<_>>().into_boxed_slice());
    let (port, seen) = spawn_ftp(body, "alice", "secret").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("blob.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("ftp-user", "alice");
    opts.set("ftp-passwd", "secret");
    opts.set("ftp-pasv", "true");
    opts.set("ftp-type", "binary");
    opts.set("file-allocation", "none");
    opts.set("disk-cache", "0");
    aria2_rust::storage::reset_try_pwrite();
    aria2_rust::sockopt::reset_recv();
    aria2_rust::sockopt::reset_send();
    ftp::download(job(
        format!("ftp://127.0.0.1:{port}/blob.bin"),
        dest.clone(),
        opts,
    ))
    .await
    .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert!(
        aria2_rust::storage::last_try_pwrite() > 0,
        "C++ DefaultDiskWriter: FTP RETR body must pwrite without per-chunk await"
    );
    assert!(
        aria2_rust::sockopt::last_recv() >= 1,
        "C++ SocketCore::readData recv must read FTP RETR body"
    );
    assert!(
        aria2_rust::sockopt::last_send() >= 1,
        "C++ SocketCore::writeData send must write FTP control commands"
    );
    assert!(seen.pasv.load(Ordering::SeqCst), "client must use PASV");
    assert!(!seen.port.load(Ordering::SeqCst), "PASV path must not send PORT");
    assert!(seen.type_i.load(Ordering::SeqCst), "ftp-type=binary must send TYPE I");
}

#[tokio::test]
async fn ftp_port_retr_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0x5Au8; 12 * 1024].into_boxed_slice());
    let (port, seen) = spawn_ftp(body, "bob", "hunter2").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("active.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("ftp-pasv", "false");
    opts.set("file-allocation", "none");
    ftp::download(job(
        format!("ftp://bob:hunter2@127.0.0.1:{port}/active.bin"),
        dest.clone(),
        opts,
    ))
    .await
    .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert!(seen.port.load(Ordering::SeqCst), "ftp-pasv=false must send PORT");
    assert!(!seen.pasv.load(Ordering::SeqCst), "PORT path must not send PASV");
}

#[tokio::test]
async fn ftp_rest_resume_dest_match() {
    let body: &'static [u8] = Box::leak((0u8..=255).cycle().take(8192).collect::<Vec<_>>().into_boxed_slice());
    let (port, seen) = spawn_ftp(body, "anonymous", "ARIA2USER@").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("c.bin");
    // Wrong prefix so a from-zero overwrite without REST still has to fetch;
    // server only emits body[rest..], so missing REST + write-at-4096 corrupts dest.
    std::fs::write(&dest, &body[..4096]).unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("continue", "true");
    opts.set("ftp-pasv", "true");
    opts.set("file-allocation", "none");
    ftp::download(job(
        format!("ftp://127.0.0.1:{port}/c.bin"),
        dest.clone(),
        opts,
    ))
    .await
    .unwrap();
    assert_eq!(seen.rest.load(Ordering::SeqCst), 4096, "REST 4096 must be sent");
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn ftp_wrong_password_fails() {
    let body: &'static [u8] = b"nope";
    let (port, _) = spawn_ftp(body, "alice", "secret").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("x.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("ftp-user", "alice");
    opts.set("ftp-passwd", "wrong");
    opts.set("file-allocation", "none");
    let err = ftp::download(job(
        format!("ftp://127.0.0.1:{port}/x.bin"),
        dest,
        opts,
    ))
    .await;
    assert!(err.is_err(), "wrong ftp-passwd must fail");
}

#[tokio::test]
async fn ftp_type_ascii_sends_type_a() {
    let body: &'static [u8] = b"hello-ascii\n";
    let (port, seen) = spawn_ftp(body, "anonymous", "ARIA2USER@").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("a.txt");
    let mut opts = OptionSet::with_defaults();
    opts.set("ftp-type", "ascii");
    opts.set("file-allocation", "none");
    ftp::download(job(
        format!("ftp://127.0.0.1:{port}/a.txt"),
        dest.clone(),
        opts,
    ))
    .await
    .unwrap();
    assert!(seen.type_a.load(Ordering::SeqCst));
    assert!(!seen.type_i.load(Ordering::SeqCst));
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn ftp_session_add_uri_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0x21u8; 4096].into_boxed_slice());
    let (port, seen) = spawn_ftp(body, "sess", "ion").await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().display().to_string());
    opts.set("ftp-user", "sess");
    opts.set("ftp-passwd", "ion");
    opts.set("file-allocation", "none");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("ftp://127.0.0.1:{port}/sess.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    let mut ok = false;
    for _ in 0..200 {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let st = session.tell_status(gid.as_str()).await.unwrap();
        match st.get("status").and_then(|v| v.as_str()) {
            Some("complete") => {
                ok = true;
                break;
            }
            Some("error") => panic!("session ftp error: {st}"),
            _ => {}
        }
    }
    assert!(ok, "session ftp download did not complete");
    assert_eq!(std::fs::read(dir.path().join("sess.bin")).unwrap(), body);
    assert!(seen.pasv.load(Ordering::SeqCst));
}

#[tokio::test]
async fn ftp_checksum_sha1_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0x11u8; 4096].into_boxed_slice());
    let (port, _) = spawn_ftp(body, "anonymous", "ARIA2USER@").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("sum.bin");
    let digest = hex::encode(sha1::Sha1::digest(body));
    let mut opts = OptionSet::with_defaults();
    opts.set("ftp-pasv", "true");
    opts.set("file-allocation", "none");
    opts.set("checksum", format!("sha-1={digest}"));
    ftp::download(job(
        format!("ftp://127.0.0.1:{port}/sum.bin"),
        dest.clone(),
        opts,
    ))
    .await
    .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

async fn spawn_connect_proxy(require_auth: bool) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = tokio::io::AsyncReadExt::read(&mut s, &mut buf)
                    .await
                    .unwrap_or(0);
                if n == 0 {
                    return;
                }
                let req = String::from_utf8_lossy(&buf[..n]);
                if require_auth {
                    let ok = req.lines().any(|l| {
                        l.to_ascii_lowercase().starts_with("proxy-authorization:")
                            && l.contains("Basic ")
                    });
                    if !ok {
                        let _ = s
                            .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic realm=\"p\"\r\nContent-Length: 0\r\n\r\n")
                            .await;
                        return;
                    }
                }
                let first = req.lines().next().unwrap_or("");
                if !first.to_ascii_uppercase().starts_with("CONNECT ") {
                    let _ = s
                        .write_all(b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\n\r\n")
                        .await;
                    return;
                }
                let target = first.split_whitespace().nth(1).unwrap_or("");
                let (host, oport) = match target.rsplit_once(':') {
                    Some((h, p)) => (h.to_string(), p.parse::<u16>().unwrap_or(21)),
                    None => (target.to_string(), 21),
                };
                let Ok(origin) = TcpStream::connect((host.as_str(), oport)).await else {
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
                let mut client = s;
                let mut origin = origin;
                let _ = tokio::io::copy_bidirectional(&mut client, &mut origin).await;
            });
        }
    });
    port
}

async fn spawn_connect_blackhole() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 1024];
                let _ = tokio::io::AsyncReadExt::read(&mut s, &mut buf).await;
                let _ = s
                    .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
                    .await;
            });
        }
    });
    port
}

#[tokio::test]
async fn ftp_proxy_connect_dest_match() {
    let body: &'static [u8] = b"ftp-connect-proxy-payload";
    let (origin, seen) = spawn_ftp(body, "anonymous", "ARIA2USER@").await;
    let proxy = spawn_connect_proxy(false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("p.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("ftp-pasv", "true");
    opts.set("file-allocation", "none");
    opts.set("ftp-proxy", format!("http://127.0.0.1:{proxy}"));
    aria2_rust::sockopt::reset_send();
    aria2_rust::sockopt::reset_recv();
    ftp::download(job(
        format!("ftp://127.0.0.1:{origin}/p.bin"),
        dest.clone(),
        opts,
    ))
    .await
    .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert!(seen.pasv.load(Ordering::SeqCst));
    assert!(
        aria2_rust::sockopt::last_send() >= 1,
        "C++ SocketCore::writeData send must write FTP CONNECT"
    );
    assert!(
        aria2_rust::sockopt::last_recv() >= 1,
        "C++ SocketCore::readData recv must read FTP CONNECT"
    );
}

#[tokio::test]
async fn ftp_proxy_user_connect_dest_match() {
    let body: &'static [u8] = b"ftp-auth-connect-payload";
    let (origin, _) = spawn_ftp(body, "anonymous", "ARIA2USER@").await;
    let proxy = spawn_connect_proxy(true).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("u.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("ftp-pasv", "true");
    opts.set("file-allocation", "none");
    opts.set("ftp-proxy", format!("http://127.0.0.1:{proxy}"));
    opts.set("ftp-proxy-user", "proxyuser");
    opts.set("ftp-proxy-passwd", "proxypass");
    ftp::download(job(
        format!("ftp://127.0.0.1:{origin}/u.bin"),
        dest.clone(),
        opts,
    ))
    .await
    .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn ftp_proxy_blackhole_fails() {
    let body: &'static [u8] = b"ftp-connect-proxy-payload";
    let (origin, _) = spawn_ftp(body, "anonymous", "ARIA2USER@").await;
    let proxy = spawn_connect_blackhole().await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("fail.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("ftp-pasv", "true");
    opts.set("file-allocation", "none");
    opts.set("ftp-proxy", format!("http://127.0.0.1:{proxy}"));
    let err = ftp::download(job(
        format!("ftp://127.0.0.1:{origin}/fail.bin"),
        dest,
        opts,
    ))
    .await;
    assert!(err.is_err(), "blackhole ftp-proxy must fail");
}

#[tokio::test]
async fn ftp_reuse_connection_true_one_user_two_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0x71u8; 4096].into_boxed_slice());
    let (port, seen) = spawn_ftp(body, "reuse", "pool").await;
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.bin");
    let b = dir.path().join("b.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("ftp-user", "reuse");
    opts.set("ftp-passwd", "pool");
    opts.set("ftp-pasv", "true");
    opts.set("ftp-reuse-connection", "true");
    opts.set("file-allocation", "none");
    ftp::download(job(
        format!("ftp://127.0.0.1:{port}/a.bin"),
        a.clone(),
        opts.clone(),
    ))
    .await
    .unwrap();
    ftp::download(job(
        format!("ftp://127.0.0.1:{port}/b.bin"),
        b.clone(),
        opts,
    ))
    .await
    .unwrap();
    assert_eq!(std::fs::read(&a).unwrap(), body);
    assert_eq!(std::fs::read(&b).unwrap(), body);
    assert_eq!(
        seen.user.load(Ordering::SeqCst),
        1,
        "ftp-reuse-connection=true must USER once"
    );
}

#[tokio::test]
async fn ftp_reuse_connection_false_two_user_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0x72u8; 4096].into_boxed_slice());
    let (port, seen) = spawn_ftp(body, "fresh", "each").await;
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.bin");
    let b = dir.path().join("b.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("ftp-user", "fresh");
    opts.set("ftp-passwd", "each");
    opts.set("ftp-pasv", "true");
    opts.set("ftp-reuse-connection", "false");
    opts.set("file-allocation", "none");
    ftp::download(job(
        format!("ftp://127.0.0.1:{port}/a.bin"),
        a.clone(),
        opts.clone(),
    ))
    .await
    .unwrap();
    ftp::download(job(
        format!("ftp://127.0.0.1:{port}/b.bin"),
        b.clone(),
        opts,
    ))
    .await
    .unwrap();
    assert_eq!(std::fs::read(&a).unwrap(), body);
    assert_eq!(std::fs::read(&b).unwrap(), body);
    assert_eq!(
        seen.user.load(Ordering::SeqCst),
        2,
        "ftp-reuse-connection=false must USER per download"
    );
}

#[tokio::test]
async fn socket_recv_buffer_size_ftp_dest_match() {
    let body: &'static [u8] = b"so-rcvbuf-ftp-dest-match";
    let (port, _) = spawn_ftp(body, "alice", "secret").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("rcv.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("ftp-user", "alice");
    opts.set("ftp-passwd", "secret");
    opts.set("ftp-pasv", "true");
    opts.set("file-allocation", "none");
    opts.set("socket-recv-buffer-size", "65536");
    aria2_rust::sockopt::reset_last();
    ftp::download(job(
        format!("ftp://127.0.0.1:{port}/rcv.bin"),
        dest.clone(),
        opts,
    ))
    .await
    .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    let got = aria2_rust::sockopt::last_got();
    assert!(
        got >= 65536,
        "production FTP socket must SO_RCVBUF >= 65536, got {got}"
    );
}
