//! LAN room share: same-room password, UDP beacon, HTTP Range file copy.
//! Not a C++ aria2 option — extra so two aria2c boxes on one LAN can deep-copy
//! a download (bytes + optional `.aria2` sidecar) as a normal HTTP job.
#![forbid(unsafe_code)]

use crate::error::{Error, Result};
use crate::options::OptionSet;
use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path as FsPath, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};

pub const ROOM_MCAST: Ipv4Addr = Ipv4Addr::new(239, 192, 152, 144);
pub const ROOM_UDP_PORT: u16 = 6942;
pub const ROOM_HTTP_PORT: u16 = 6943;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileOffer {
    pub path: String,
    pub size: u64,
    pub control: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct RoomPeer {
    pub name: String,
    pub addr: String,
    pub port: u16,
    pub local: bool,
    pub files: Vec<FileOffer>,
}

struct Inner {
    dir: PathBuf,
    password: String,
    name: String,
    hash: String,
    cookie: String,
    http_port: std::sync::atomic::AtomicU16,
    peers: Mutex<HashMap<String, (Instant, String, u16, String)>>, // key -> (seen, addr, port, name)
}

pub struct RoomHub {
    inner: Arc<Inner>,
    stop: Arc<AtomicBool>,
    http_port: u16,
    udp_port: u16,
}

impl RoomHub {
    pub fn password(&self) -> &str {
        &self.inner.password
    }

    pub fn dir(&self) -> &FsPath {
        &self.inner.dir
    }

    pub fn http_port(&self) -> u16 {
        self.http_port
    }

    pub fn udp_port(&self) -> u16 {
        self.udp_port
    }

    pub fn name(&self) -> &str {
        &self.inner.name
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    pub async fn start(opts: &OptionSet) -> Result<Arc<Self>> {
        let password = opts.get("room-password").unwrap_or("").to_string();
        if password.is_empty() {
            return Err(Error::Other("room-password empty".into()));
        }
        let dir = opts.dir();
        std::fs::create_dir_all(&dir).map_err(|e| Error::Other(e.to_string()))?;
        let name = opts
            .get("room-name")
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(default_room_name);
        let name = name
            .chars()
            .filter(|c| *c != '\r' && *c != '\n')
            .take(64)
            .collect::<String>();
        let hash = room_hash(&password);
        let cookie = format!("r{}", hex::encode(rand_bytes8()));
        let want_http = opts.u64("room-listen-port", ROOM_HTTP_PORT as u64) as u16;
        let want_udp = opts.u64("room-udp-port", ROOM_UDP_PORT as u64) as u16;

        let inner = Arc::new(Inner {
            dir: dir.clone(),
            password: password.clone(),
            name: name.clone(),
            hash: hash.clone(),
            cookie: cookie.clone(),
            http_port: std::sync::atomic::AtomicU16::new(0),
            peers: Mutex::new(HashMap::new()),
        });
        let stop = Arc::new(AtomicBool::new(false));

        let http_port = spawn_http(Arc::clone(&inner), Arc::clone(&stop), want_http).await?;
        inner.http_port.store(http_port, Ordering::SeqCst);
        let udp_port = spawn_udp(
            Arc::clone(&inner),
            Arc::clone(&stop),
            want_udp,
            http_port,
        )
        .await?;

        Ok(Arc::new(Self {
            inner,
            stop,
            http_port,
            udp_port,
        }))
    }

    pub fn local_files(&self) -> Vec<FileOffer> {
        list_files(&self.inner.dir)
    }

    pub async fn peers(&self) -> Vec<RoomPeer> {
        let local = RoomPeer {
            name: self.inner.name.clone(),
            addr: "127.0.0.1".into(),
            port: self.http_port,
            local: true,
            files: self.local_files(),
        };
        let now = Instant::now();
        let snap: Vec<(String, u16, String)> = {
            let mut g = self.inner.peers.lock().await;
            g.retain(|_, (seen, _, _, _)| now.duration_since(*seen) < Duration::from_secs(12));
            g.values()
                .map(|(_, addr, port, name)| (addr.clone(), *port, name.clone()))
                .collect()
        };
        let pw = self.inner.password.clone();
        let mut out = vec![local];
        for (addr, port, name) in snap {
            let files = fetch_peer_files(&addr, port, &pw).await.unwrap_or_default();
            out.push(RoomPeer {
                name,
                addr,
                port,
                local: false,
                files,
            });
        }
        out
    }

    pub fn file_url(&self, addr: &str, port: u16, rel: &str) -> String {
        format!(
            "http://{addr}:{port}/room/v1/file/{}",
            encode_rel(rel)
        )
    }
}

pub fn room_hash(password: &str) -> String {
    let mut h = sha2::Sha256::new();
    sha2::Digest::update(&mut h, password.as_bytes());
    let out = sha2::Digest::finalize(h);
    hex::encode(&out[..8])
}

pub fn pass_ok(got: &str, want: &str) -> bool {
    if got.len() != want.len() || got.is_empty() {
        return false;
    }
    got.as_bytes()
        .iter()
        .zip(want.as_bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

pub fn encode_beacon(name: &str, port: u16, hash: &str, cookie: &str) -> Vec<u8> {
    format!(
        "ARIA2-ROOM * HTTP/1.1\r\nHost: {ROOM_MCAST}:{ROOM_UDP_PORT}\r\nPort: {port}\r\nName: {name}\r\nRoom: {hash}\r\nCookie: {cookie}\r\n\r\n"
    )
    .into_bytes()
}

pub fn parse_beacon(buf: &[u8]) -> Option<(u16, String, String, String)> {
    let s = std::str::from_utf8(buf).ok()?;
    if !s.starts_with("ARIA2-ROOM") {
        return None;
    }
    let mut port = None;
    let mut name = None;
    let mut hash = None;
    let mut cookie = None;
    for line in s.split(['\r', '\n']) {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Port:") {
            port = rest.trim().parse().ok();
        } else if let Some(rest) = line.strip_prefix("Name:") {
            name = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("Room:") {
            hash = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("Cookie:") {
            cookie = Some(rest.trim().to_string());
        }
    }
    Some((port?, name?, hash?, cookie?))
}

/// Reject `..`, absolute, NUL; stay under `dir`.
pub fn resolve_under(dir: &FsPath, rel: &str) -> Option<PathBuf> {
    if rel.contains('\0') {
        return None;
    }
    let rel = rel.trim_start_matches('/');
    if rel.is_empty() {
        return None;
    }
    let mut out = dir.to_path_buf();
    for part in rel.split(['/', '\\']) {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return None;
        }
        if part.contains(':') && cfg!(windows) {
            return None;
        }
        out.push(part);
    }
    let dir_c = dir.canonicalize().ok()?;
    match out.canonicalize() {
        Ok(c) => {
            if c.starts_with(&dir_c) {
                Some(c)
            } else {
                None
            }
        }
        Err(_) => {
            // dest may not exist yet (listing uses existing files only)
            None
        }
    }
}

pub fn list_files(dir: &FsPath) -> Vec<FileOffer> {
    let mut out = Vec::new();
    walk(dir, dir, 0, &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out.truncate(1000);
    out
}

fn walk(root: &FsPath, cur: &FsPath, depth: u32, out: &mut Vec<FileOffer>) {
    if depth > 8 || out.len() >= 1000 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(cur) else {
        return;
    };
    let mut names: Vec<(PathBuf, bool)> = Vec::new();
    for e in rd.flatten() {
        names.push((e.path(), e.file_type().map(|t| t.is_dir()).unwrap_or(false)));
    }
    let controls: std::collections::HashSet<String> = names
        .iter()
        .filter_map(|(p, is_dir)| {
            if *is_dir {
                return None;
            }
            let n = p.file_name()?.to_string_lossy();
            n.strip_suffix(".aria2").map(|s| s.to_string())
        })
        .collect();
    for (p, is_dir) in names {
        if is_dir {
            walk(root, &p, depth + 1, out);
            continue;
        }
        let Some(name) = p.file_name() else { continue };
        let name = name.to_string_lossy();
        if name.ends_with(".aria2") {
            continue;
        }
        if name.starts_with('.') {
            continue;
        }
        let Ok(meta) = p.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let rel = p.strip_prefix(root).unwrap_or(&p);
        let rel = rel.to_string_lossy().replace('\\', "/");
        let stem = p
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        out.push(FileOffer {
            path: rel,
            size: meta.len(),
            control: controls.contains(&stem),
        });
    }
}

pub fn parse_range(h: &HeaderMap, len: u64) -> Option<(u64, u64)> {
    let v = h.get(header::RANGE)?.to_str().ok()?;
    let spec = v.strip_prefix("bytes=")?;
    let (a, b) = spec.split_once('-')?;
    if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        let n = n.min(len);
        return Some((len.saturating_sub(n), len.saturating_sub(1)));
    }
    let start: u64 = a.parse().ok()?;
    if start >= len {
        return None;
    }
    let end = if b.is_empty() {
        len.saturating_sub(1)
    } else {
        b.parse::<u64>().ok()?.min(len.saturating_sub(1))
    };
    if end < start {
        return None;
    }
    Some((start, end))
}

fn encode_rel(rel: &str) -> String {
    rel.split('/')
        .map(|part| {
            let mut o = String::new();
            for b in part.bytes() {
                match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                        o.push(b as char);
                    }
                    _ => o.push_str(&format!("%{b:02X}")),
                }
            }
            o
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn default_room_name() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "aria2".into())
}

fn rand_bytes8() -> [u8; 8] {
    use rand::RngCore;
    let mut b = [0u8; 8];
    rand::rng().fill_bytes(&mut b);
    b
}

fn auth_ok(req: &HeaderMap, password: &str) -> bool {
    if let Some(v) = req.get("x-room-password").and_then(|v| v.to_str().ok()) {
        return pass_ok(v, password);
    }
    if let Some(v) = req.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        if let Some(rest) = v.strip_prefix("Bearer ") {
            return pass_ok(rest.trim(), password);
        }
    }
    false
}

async fn spawn_http(inner: Arc<Inner>, stop: Arc<AtomicBool>, want: u16) -> Result<u16> {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    let app = Router::new()
        .route("/room/v1/hello", get(hello))
        .route("/room/v1/files", get(files))
        .route("/room/v1/file/{*path}", get(get_file))
        .with_state(inner)
        .layer(cors);
    let listener = match TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], want))).await {
        Ok(l) => l,
        Err(_) if want != 0 => TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 0))).await?,
        Err(e) => return Err(e.into()),
    };
    let port = listener.local_addr()?.port();
    tokio::spawn(async move {
        let srv = axum::serve(listener, app);
        tokio::select! {
            _ = srv => {}
            _ = wait_stop(&stop) => {}
        }
    });
    Ok(port)
}

async fn spawn_udp(
    inner: Arc<Inner>,
    stop: Arc<AtomicBool>,
    want: u16,
    http_port: u16,
) -> Result<u16> {
    let sock = match UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], want))).await {
        Ok(s) => s,
        Err(_) if want != 0 => UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0))).await?,
        Err(e) => return Err(e.into()),
    };
    sock.set_broadcast(true).ok();
    sock.join_multicast_v4(ROOM_MCAST, Ipv4Addr::UNSPECIFIED).ok();
    let port = sock.local_addr()?.port();
    tokio::spawn(async move {
        let mut buf = [0u8; 1500];
        loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let ann = encode_beacon(
                &inner.name,
                http_port,
                &inner.hash,
                &inner.cookie,
            );
            let mcast = SocketAddr::from((ROOM_MCAST, port));
            let _ = sock.send_to(&ann, mcast).await;
            let _ = sock
                .send_to(&ann, SocketAddr::from(([127, 0, 0, 1], port)))
                .await;
            let _ = sock
                .send_to(&ann, SocketAddr::from(([255, 255, 255, 255], port)))
                .await;
            tokio::select! {
                _ = wait_stop(&stop) => break,
                r = tokio::time::timeout(Duration::from_millis(400), sock.recv_from(&mut buf)) => {
                    if let Ok(Ok((n, src))) = r {
                        if let Some((p, name, hash, cookie)) = parse_beacon(&buf[..n]) {
                            if hash == inner.hash && cookie != inner.cookie {
                                let addr = src.ip().to_string();
                                let key = format!("{addr}:{p}");
                                let mut g = inner.peers.lock().await;
                                g.insert(key, (Instant::now(), addr, p, name));
                            }
                        }
                    }
                }
            }
        }
    });
    Ok(port)
}

async fn wait_stop(stop: &AtomicBool) {
    while !stop.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(120)).await;
    }
}

async fn hello(State(st): State<Arc<Inner>>, req: Request) -> Response {
    if !auth_ok(req.headers(), &st.password) {
        return (StatusCode::UNAUTHORIZED, "room password").into_response();
    }
    Json(serde_json::json!({
        "name": st.name,
        "room": st.hash,
        "port": st.http_port.load(Ordering::SeqCst),
    }))
    .into_response()
}

async fn files(State(st): State<Arc<Inner>>, req: Request) -> Response {
    if !auth_ok(req.headers(), &st.password) {
        return (StatusCode::UNAUTHORIZED, "room password").into_response();
    }
    Json(list_files(&st.dir)).into_response()
}

async fn get_file(
    State(st): State<Arc<Inner>>,
    Path(path): Path<String>,
    headers: HeaderMap,
) -> Response {
    if !auth_ok(&headers, &st.password) {
        return (StatusCode::UNAUTHORIZED, "room password").into_response();
    }
    let rel = percent_decode(&path);
    let Some(abs) = resolve_under(&st.dir, &rel) else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };
    let Ok(meta) = std::fs::metadata(&abs) else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };
    if !meta.is_file() {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let len = meta.len();
    let (start, end, status) = match parse_range(&headers, len) {
        Some((s, e)) => (s, e, StatusCode::PARTIAL_CONTENT),
        None => (0, len.saturating_sub(1), StatusCode::OK),
    };
    if len == 0 {
        return Response::builder()
            .status(status)
            .header(header::CONTENT_LENGTH, "0")
            .header(header::ACCEPT_RANGES, "bytes")
            .body(Body::empty())
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }
    if start > end || start >= len {
        return StatusCode::RANGE_NOT_SATISFIABLE.into_response();
    }
    let take = end.saturating_sub(start).saturating_add(1);
    let Ok(mut f) = tokio::fs::File::open(&abs).await else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };
    if start > 0 {
        if f.seek(std::io::SeekFrom::Start(start)).await.is_err() {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }
    let stream = futures::stream::unfold((f, take), |(mut f, left)| async move {
        if left == 0 {
            return None;
        }
        let cap = left.min(64 * 1024) as usize;
        let mut buf = vec![0u8; cap];
        match f.read(&mut buf).await {
            Ok(0) => None,
            Ok(n) => {
                buf.truncate(n);
                Some((
                    Ok::<_, std::io::Error>(bytes::Bytes::from(buf)),
                    (f, left.saturating_sub(n as u64)),
                ))
            }
            Err(e) => Some((Err(e), (f, 0))),
        }
    });
    let mut builder = Response::builder()
        .status(status)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, take.to_string())
        .header(header::CONTENT_TYPE, "application/octet-stream");
    if status == StatusCode::PARTIAL_CONTENT {
        builder = builder.header(
            header::CONTENT_RANGE,
            format!("bytes {start}-{end}/{len}"),
        );
    }
    builder
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let h = u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or(""), 16);
            if let Ok(v) = h {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

async fn fetch_peer_files(addr: &str, port: u16, password: &str) -> Result<Vec<FileOffer>> {
    let url = format!("http://{addr}:{port}/room/v1/files");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|e| Error::Other(e.to_string()))?;
    let r = client
        .get(&url)
        .header("X-Room-Password", password)
        .send()
        .await
        .map_err(|e| Error::Other(e.to_string()))?;
    if !r.status().is_success() {
        return Err(Error::Other(format!("peer files {}", r.status())));
    }
    let bytes = r
        .bytes()
        .await
        .map_err(|e| Error::Other(e.to_string()))?;
    serde_json::from_slice(&bytes).map_err(|e| Error::Other(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_rejects_dotdot() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ok.bin"), b"abc").unwrap();
        assert!(resolve_under(dir.path(), "ok.bin").is_some());
        assert!(resolve_under(dir.path(), "../ok.bin").is_none());
        assert!(resolve_under(dir.path(), "/etc/passwd").is_none());
        assert!(resolve_under(dir.path(), "ok.bin/../../etc/passwd").is_none());
    }

    #[test]
    fn beacon_roundtrip() {
        let b = encode_beacon("kitchen", 6943, "abcd", "c1");
        let (p, n, h, c) = parse_beacon(&b).unwrap();
        assert_eq!(p, 6943);
        assert_eq!(n, "kitchen");
        assert_eq!(h, "abcd");
        assert_eq!(c, "c1");
    }

    #[test]
    fn pass_ok_rejects_wrong() {
        assert!(pass_ok("secret", "secret"));
        assert!(!pass_ok("secret", "Secret"));
        assert!(!pass_ok("secr", "secret"));
        assert!(!pass_ok("", ""));
    }

    #[test]
    fn range_suffix_and_open_end() {
        let mut h = HeaderMap::new();
        h.insert(header::RANGE, "bytes=2-5".parse().unwrap());
        assert_eq!(parse_range(&h, 10), Some((2, 5)));
        h.insert(header::RANGE, "bytes=8-".parse().unwrap());
        assert_eq!(parse_range(&h, 10), Some((8, 9)));
        h.insert(header::RANGE, "bytes=-3".parse().unwrap());
        assert_eq!(parse_range(&h, 10), Some((7, 9)));
    }
}
