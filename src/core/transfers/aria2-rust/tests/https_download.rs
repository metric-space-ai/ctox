//! HTTPS rustls dest-match: `--ca-certificate` trusts the fixture CA,
//! `--min-tls-version` changes the handshake. Bytes must match.
#![forbid(unsafe_code)]

use aria2_rust::http::{self, HttpJob, HttpProgress};
use aria2_rust::options::OptionSet;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::ServerConfig;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio_rustls::TlsAcceptor;

const CERT_DER: &[u8] = include_bytes!("fixtures/https-server.der");
const KEY_DER: &[u8] = include_bytes!("fixtures/https-key.der");
const CA_PEM: &str = include_str!("fixtures/https-ca.pem");

fn ca_file() -> (tempfile::NamedTempFile, String) {
    let f = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(f.path(), CA_PEM).unwrap();
    let path = f.path().display().to_string();
    (f, path)
}

fn tls_config(versions: &[&'static rustls::SupportedProtocolVersion]) -> Arc<ServerConfig> {
    let provider = rustls::crypto::ring::default_provider();
    let certs = vec![CertificateDer::from(CERT_DER.to_vec())];
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(KEY_DER.to_vec()));
    let mut cfg = ServerConfig::builder_with_provider(provider.into())
        .with_protocol_versions(versions)
        .expect("tls versions")
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .expect("server cert");
    cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
    Arc::new(cfg)
}

async fn spawn_https(
    body: &'static [u8],
    versions: &[&'static rustls::SupportedProtocolVersion],
) -> u16 {
    let acceptor = TlsAcceptor::from(tls_config(versions));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else { break };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(mut s) = acceptor.accept(tcp).await else { return };
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    return;
                }
                let req = String::from_utf8_lossy(&buf[..n]);
                let range = req
                    .lines()
                    .find(|l| l.to_ascii_lowercase().starts_with("range:"));
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

fn job(uri: String, dest: std::path::PathBuf, opts: OptionSet, piece_length: u32) -> HttpJob {
    let (_tx, rx) = watch::channel(false);
    HttpJob {
        uris: vec![uri],
        dest,
        opts,
        progress: HttpProgress::new(),
        piece_length,
        cancel: rx,
    }
}

#[tokio::test]
async fn https_segmented_dest_match_with_ca() {
    let body: &'static [u8] = Box::leak(vec![0xCDu8; 32 * 1024].into_boxed_slice());
    let port = spawn_https(body, &[&rustls::version::TLS12, &rustls::version::TLS13]).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("blob.bin");
    let (_ca, ca_path) = ca_file();
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "4");
    opts.set("max-connection-per-server", "4");
    opts.set("min-split-size", "1024");
    opts.set("piece-length", "4096");
    opts.set("file-allocation", "none");
    opts.set("ca-certificate", &ca_path);
    opts.set("check-certificate", "true");
    http::reset_https_io();
    http::download(job(
        format!("https://127.0.0.1:{port}/blob.bin"),
        dest.clone(),
        opts,
        4096,
    ))
    .await
    .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert!(
        http::last_https_send() >= 1,
        "C++ SocketCore TLS HTTPS GET must writeData send, got {}",
        http::last_https_send()
    );
    assert!(
        http::last_https_recv() >= 1,
        "C++ SocketCore TLS HTTPS GET must readData recv"
    );
}

async fn spawn_https_redirect(
    body: &'static [u8],
    hops: usize,
    versions: &[&'static rustls::SupportedProtocolVersion],
) -> u16 {
    let acceptor = TlsAcceptor::from(tls_config(versions));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else { break };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(mut s) = acceptor.accept(tcp).await else { return };
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    return;
                }
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
                let hop: usize = path.trim_start_matches("/r").parse().unwrap_or(0);
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
async fn https_redirect_302_dest_match() {
    let body: &'static [u8] = b"https-redirect-dest-match";
    let port = spawn_https_redirect(
        body,
        1,
        &[&rustls::version::TLS12, &rustls::version::TLS13],
    )
    .await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("redir.bin");
    let (_ca, ca_path) = ca_file();
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("ca-certificate", &ca_path);
    opts.set("check-certificate", "true");
    http::reset_https_io();
    http::download(job(
        format!("https://127.0.0.1:{port}/r0"),
        dest.clone(),
        opts,
        1024 * 1024,
    ))
    .await
    .unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "C++ TLS HttpSkipResponseCommand 302 must dest-match"
    );
    assert!(
        http::last_https_send() >= 2,
        "redirect + final must two TLS writeData, got {}",
        http::last_https_send()
    );
}

#[tokio::test]
async fn https_redirect_over_max_no_dest() {
    let body: &'static [u8] = b"https-should-not-write";
    let port = spawn_https_redirect(
        body,
        21,
        &[&rustls::version::TLS12, &rustls::version::TLS13],
    )
    .await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("toomany.bin");
    let (_ca, ca_path) = ca_file();
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("ca-certificate", &ca_path);
    let err = http::download(job(
        format!("https://127.0.0.1:{port}/r0"),
        dest.clone(),
        opts,
        1024 * 1024,
    ))
    .await
    .unwrap_err();
    assert!(
        err.to_string().contains("too many redirects"),
        "C++ Request::MAX_REDIRECT=20 must abort TLS hops, got {err}"
    );
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap_or_default() != body,
        "too many HTTPS redirects must not dest-match"
    );
}

#[tokio::test]
async fn https_rejects_unknown_ca_when_checking() {
    let body: &'static [u8] = b"secret-https-body";
    let port = spawn_https(body, &[&rustls::version::TLS12, &rustls::version::TLS13]).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("nope.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("check-certificate", "true");
    let err = http::download(job(
        format!("https://127.0.0.1:{port}/nope.bin"),
        dest,
        opts,
        1024,
    ))
    .await;
    assert!(err.is_err(), "self-signed without ca-certificate must fail");
}

#[tokio::test]
async fn https_min_tls_version_rejects_tls12_only_server() {
    let body: &'static [u8] = Box::leak(vec![0x11u8; 4096].into_boxed_slice());
    let port = spawn_https(body, &[&rustls::version::TLS12]).await;
    let dir = tempfile::tempdir().unwrap();
    let dest_fail = dir.path().join("tls-fail.bin");
    let dest = dir.path().join("tls.bin");
    let (_ca, ca_path) = ca_file();

    let mut too_new = OptionSet::with_defaults();
    too_new.set("split", "1");
    too_new.set("file-allocation", "none");
    too_new.set("ca-certificate", &ca_path);
    too_new.set("min-tls-version", "TLSv1.3");
    let err = http::download(job(
        format!("https://127.0.0.1:{port}/tls.bin"),
        dest_fail,
        too_new,
        4096,
    ))
    .await;
    assert!(err.is_err(), "TLSv1.3 client must not speak TLS 1.2-only server");

    let mut ok = OptionSet::with_defaults();
    ok.set("split", "1");
    ok.set("file-allocation", "none");
    ok.set("ca-certificate", &ca_path);
    ok.set("min-tls-version", "TLSv1.2");
    http::download(job(
        format!("https://127.0.0.1:{port}/tls.bin"),
        dest.clone(),
        ok,
        4096,
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
                let n = s.read(&mut buf).await.unwrap_or(0);
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
                    Some((h, p)) => (
                        h.trim_matches(|c| c == '[' || c == ']').to_string(),
                        p.parse::<u16>().unwrap_or(443),
                    ),
                    None => (target.to_string(), 443),
                };
                let Ok(origin) = tokio::net::TcpStream::connect((host.as_str(), oport)).await else {
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
async fn https_proxy_connect_dest_match() {
    let body: &'static [u8] = b"https-connect-proxy-payload";
    let origin = spawn_https(body, &[&rustls::version::TLS12, &rustls::version::TLS13]).await;
    let proxy = spawn_connect_proxy(false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("p.bin");
    let (_ca, ca_path) = ca_file();
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("ca-certificate", &ca_path);
    opts.set("https-proxy", format!("http://127.0.0.1:{proxy}"));
    http::reset_https_io();
    http::download(job(
        format!("https://127.0.0.1:{origin}/p.bin"),
        dest.clone(),
        opts,
        1024 * 1024,
    ))
    .await
    .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert!(
        http::last_https_connect() >= 1,
        "C++ HttpProxyRequestCommand CONNECT must SocketCore send, got {}",
        http::last_https_connect()
    );
    assert!(
        http::last_https_send() >= 1,
        "HTTPS GET after CONNECT must TLS writeData"
    );
}

#[tokio::test]
async fn https_proxy_user_connect_dest_match() {
    let body: &'static [u8] = b"https-auth-connect-payload";
    let origin = spawn_https(body, &[&rustls::version::TLS12, &rustls::version::TLS13]).await;
    let proxy = spawn_connect_proxy(true).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("u.bin");
    let (_ca, ca_path) = ca_file();
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("ca-certificate", &ca_path);
    opts.set("https-proxy", format!("http://127.0.0.1:{proxy}"));
    opts.set("https-proxy-user", "proxyuser");
    opts.set("https-proxy-passwd", "proxypass");
    http::download(job(
        format!("https://127.0.0.1:{origin}/u.bin"),
        dest.clone(),
        opts,
        1024 * 1024,
    ))
    .await
    .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn https_proxy_blackhole_fails() {
    let body: &'static [u8] = b"https-connect-proxy-payload";
    let origin = spawn_https(body, &[&rustls::version::TLS12, &rustls::version::TLS13]).await;
    let proxy = spawn_connect_blackhole().await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("fail.bin");
    let (_ca, ca_path) = ca_file();
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("ca-certificate", &ca_path);
    opts.set("https-proxy", format!("http://127.0.0.1:{proxy}"));
    let err = http::download(job(
        format!("https://127.0.0.1:{origin}/fail.bin"),
        dest,
        opts,
        1024 * 1024,
    ))
    .await;
    assert!(err.is_err(), "blackhole https-proxy must fail");
}

struct MtlsFiles {
    dir: tempfile::TempDir,
    ca_pem: String,
    client_pem: String,
    client_key: String,
    server_cert: Vec<u8>,
    server_key: Vec<u8>,
}

fn gen_mtls() -> MtlsFiles {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    let run = |args: &[&str]| {
        let st = std::process::Command::new("openssl")
            .current_dir(p)
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("openssl");
        assert!(st.success(), "openssl {args:?} failed");
    };
    run(&[
        "req", "-x509", "-newkey", "rsa:2048", "-keyout", "ca.key", "-out", "ca.pem",
        "-days", "1", "-nodes", "-subj", "/CN=aria2-test-ca",
        "-addext", "basicConstraints=critical,CA:TRUE",
    ]);
    run(&[
        "req", "-newkey", "rsa:2048", "-keyout", "server.key", "-out", "server.csr",
        "-nodes", "-subj", "/CN=127.0.0.1",
    ]);
    std::fs::write(
        p.join("ext.cnf"),
        "subjectAltName=IP:127.0.0.1\nbasicConstraints=CA:FALSE\nkeyUsage=digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n",
    )
    .unwrap();
    run(&[
        "x509", "-req", "-in", "server.csr", "-CA", "ca.pem", "-CAkey", "ca.key",
        "-CAcreateserial", "-out", "server.pem", "-days", "1", "-extfile", "ext.cnf",
    ]);
    run(&[
        "req", "-newkey", "rsa:2048", "-keyout", "client.key", "-out", "client.csr",
        "-nodes", "-subj", "/CN=aria2-client",
    ]);
    std::fs::write(
        p.join("client-ext.cnf"),
        "basicConstraints=CA:FALSE\nkeyUsage=digitalSignature\nextendedKeyUsage=clientAuth\n",
    )
    .unwrap();
    run(&[
        "x509", "-req", "-in", "client.csr", "-CA", "ca.pem", "-CAkey", "ca.key",
        "-CAcreateserial", "-out", "client.pem", "-days", "1", "-extfile", "client-ext.cnf",
    ]);
    run(&["x509", "-in", "server.pem", "-outform", "DER", "-out", "server.der"]);
    run(&[
        "pkcs8", "-topk8", "-nocrypt", "-in", "server.key", "-outform", "DER",
        "-out", "server-key.der",
    ]);
    run(&["x509", "-in", "ca.pem", "-outform", "DER", "-out", "ca.der"]);
    run(&[
        "pkcs8", "-topk8", "-nocrypt", "-in", "client.key", "-out", "client-pkcs8.pem",
    ]);
    MtlsFiles {
        ca_pem: p.join("ca.pem").display().to_string(),
        client_pem: p.join("client.pem").display().to_string(),
        client_key: p.join("client-pkcs8.pem").display().to_string(),
        server_cert: std::fs::read(p.join("server.der")).unwrap(),
        server_key: std::fs::read(p.join("server-key.der")).unwrap(),
        dir,
    }
}

async fn spawn_https_mtls(body: &'static [u8], files: &MtlsFiles) -> u16 {
    use rustls::RootCertStore;
    use rustls::server::WebPkiClientVerifier;
    let mut roots = RootCertStore::empty();
    let ca_der = std::fs::read(files.dir.path().join("ca.der")).unwrap();
    roots.add(CertificateDer::from(ca_der)).unwrap();
    let verifier = WebPkiClientVerifier::builder(Arc::new(roots))
        .build()
        .unwrap();
    let provider = rustls::crypto::ring::default_provider();
    let certs = vec![CertificateDer::from(files.server_cert.clone())];
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(files.server_key.clone()));
    let mut cfg = ServerConfig::builder_with_provider(provider.into())
        .with_safe_default_protocol_versions()
        .expect("tls")
        .with_client_cert_verifier(verifier)
        .with_single_cert(certs, key)
        .expect("mtls server cert");
    cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
    let acceptor = TlsAcceptor::from(Arc::new(cfg));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else { break };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(mut s) = acceptor.accept(tcp).await else { return };
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
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
async fn certificate_private_key_mtls_dest_match() {
    let body: &'static [u8] = b"mtls-client-cert-payload";
    let files = gen_mtls();
    let port = spawn_https_mtls(body, &files).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("m.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("ca-certificate", &files.ca_pem);
    opts.set("certificate", &files.client_pem);
    opts.set("private-key", &files.client_key);
    http::download(job(
        format!("https://127.0.0.1:{port}/m.bin"),
        dest.clone(),
        opts,
        1024 * 1024,
    ))
    .await
    .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    let _ = &files.dir;
}

#[tokio::test]
async fn certificate_missing_mtls_rejected() {
    let body: &'static [u8] = b"mtls-client-cert-payload";
    let files = gen_mtls();
    let port = spawn_https_mtls(body, &files).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("n.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("split", "1");
    opts.set("file-allocation", "none");
    opts.set("ca-certificate", &files.ca_pem);
    let err = http::download(job(
        format!("https://127.0.0.1:{port}/n.bin"),
        dest,
        opts,
        1024 * 1024,
    ))
    .await;
    assert!(err.is_err(), "mTLS without --certificate must fail");
    let _ = &files.dir;
}

async fn read_tls_http_headers(s: &mut tokio_rustls::server::TlsStream<tokio::net::TcpStream>) -> Option<String> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 512];
    loop {
        let n = s.read(&mut tmp).await.ok()?;
        if n == 0 {
            return if buf.is_empty() {
                None
            } else {
                Some(String::from_utf8_lossy(&buf).into_owned())
            };
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            return Some(String::from_utf8_lossy(&buf).into_owned());
        }
        if buf.len() > 64 * 1024 {
            return None;
        }
    }
}

async fn spawn_https_accepts(body: &'static [u8]) -> (u16, Arc<AtomicU64>) {
    let acceptor = TlsAcceptor::from(tls_config(&[&rustls::version::TLS12, &rustls::version::TLS13]));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let accepts = Arc::new(AtomicU64::new(0));
    let seen = Arc::clone(&accepts);
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else { break };
            seen.fetch_add(1, Ordering::SeqCst);
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(mut s) = acceptor.accept(tcp).await else { return };
                loop {
                    let req = match tokio::time::timeout(
                        std::time::Duration::from_secs(30),
                        read_tls_http_headers(&mut s),
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
                    } else if let Some(r) = req
                        .lines()
                        .find(|l| l.to_ascii_lowercase().starts_with("range:"))
                    {
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

#[tokio::test]
async fn https_keep_alive_one_accept_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0xAEu8; 8192].into_boxed_slice());
    let mut last_n = 0u64;
    let mut dest_ok = false;
    let mut reused = 0u64;
    for _ in 0..3 {
        let (port, accepts) = spawn_https_accepts(body).await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("ka.bin");
        let (_ca, ca_path) = ca_file();
        let mut opts = OptionSet::with_defaults();
        opts.set("split", "2");
        opts.set("max-connection-per-server", "1");
        opts.set("min-split-size", "1");
        opts.set("piece-length", "4096");
        opts.set("file-allocation", "none");
        opts.set("enable-http-pipelining", "false");
        opts.set("use-head", "false");
        opts.set("enable-http-keep-alive", "true");
        opts.set("ca-certificate", &ca_path);
        opts.set("check-certificate", "true");
        http::reset_https_io();
        http::download(job(
            format!("https://127.0.0.1:{port}/ka.bin"),
            dest.clone(),
            opts,
            4096,
        ))
        .await
        .unwrap();
        dest_ok = std::fs::read(&dest).unwrap() == body;
        last_n = accepts.load(Ordering::SeqCst);
        reused = http::last_https_ka_reuse();
        if dest_ok && last_n == 1 && reused >= 1 {
            return;
        }
    }
    assert!(dest_ok, "HTTPS keep-alive dest must match");
    assert_eq!(
        last_n, 1,
        "C++ HttpKeepAliveConnection TLS must reuse one TCP accept, got {last_n}"
    );
    assert!(
        reused >= 1,
        "C++ HttpKeepAliveConnection TLS must reuse SocketCore, got {reused}"
    );
}

