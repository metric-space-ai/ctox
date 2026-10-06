#![forbid(unsafe_code)]

use crate::cookies::Cookie;
use crate::error::{Error, Result};
use crate::options::OptionSet;
use crate::storage::FileStorage;
use bytes::Bytes;
use futures::StreamExt;
use reqwest::{
    header::{self, HeaderMap, HeaderName, HeaderValue},
    Client, Proxy,
};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::watch;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// C++ `aria2.getPeers` row (ip/port/peerId from BT handshake).
#[derive(Clone, Debug)]
pub struct PeerStat {
    pub ip: String,
    pub port: u16,
    pub peer_id: String,
    pub seeder: bool,
    pub am_choking: bool,
    pub peer_choking: bool,
    pub bitfield: String,
}

#[derive(Clone, Debug)]
pub struct ServerStat {
    pub index: u32,
    pub uri: String,
    pub current_uri: String,
}

#[derive(Clone)]
pub struct HttpProgress {
    pub total: Arc<AtomicU64>,
    pub completed: Arc<AtomicU64>,
    pub halt: Arc<AtomicBool>,
    pub peers: Arc<Mutex<Vec<PeerStat>>>,
    pub servers: Arc<Mutex<Vec<ServerStat>>>,
    pub overall: Arc<OverallLimiter>,
    /// C++ `--max-overall-upload-limit` shared across Session seeders.
    pub overall_up: Arc<OverallLimiter>,
    /// C++ seed-only: true while `--seed-ratio`/`--seed-time` upload loop runs.
    pub seeding: Arc<AtomicBool>,
    /// C++ CheckIntegrityCommand already ran on the still-open dest DiskWriter.
    pub checksum_done: Arc<AtomicBool>,
}

/// C++ `--max-overall-download-limit` (prefs.h PREF_MAX_OVERALL_DOWNLOAD_LIMIT).
/// Shared across Session tasks; no-op when limit is 0.
pub struct OverallLimiter {
    limit: AtomicU64,
    written: AtomicU64,
    start: Mutex<std::time::Instant>,
}

impl OverallLimiter {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            limit: AtomicU64::new(0),
            written: AtomicU64::new(0),
            start: Mutex::new(std::time::Instant::now()),
        })
    }

    pub fn set_limit(&self, n: u64) {
        self.limit.store(n, Ordering::Relaxed);
    }

    pub fn limit(&self) -> u64 {
        self.limit.load(Ordering::Relaxed)
    }

    pub fn download_speed(&self) -> u64 {
        let w = self.written.load(Ordering::Relaxed);
        let elapsed = self.start.lock().unwrap().elapsed().as_secs_f64();
        if elapsed < 0.05 || w == 0 {
            return 0;
        }
        (w as f64 / elapsed) as u64
    }

    pub async fn after(&self, n: u64) {
        let limit = self.limit.load(Ordering::Relaxed);
        if limit == 0 {
            self.written.fetch_add(n, Ordering::Relaxed);
            return;
        }
        let written = self.written.fetch_add(n, Ordering::Relaxed) + n;
        let start = *self.start.lock().unwrap();
        let expected_ms = written.saturating_mul(1000) / limit.max(1);
        let elapsed_ms = start.elapsed().as_millis() as u64;
        if elapsed_ms < expected_ms {
            tokio::time::sleep(Duration::from_millis(expected_ms - elapsed_ms)).await;
        }
    }
}

impl std::fmt::Debug for HttpProgress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpProgress")
            .field("total", &self.total.load(Ordering::Relaxed))
            .field("completed", &self.completed.load(Ordering::Relaxed))
            .finish()
    }
}

impl HttpProgress {
    pub fn new() -> Self {
        Self::with_overall(OverallLimiter::new())
    }

    pub fn with_overall(overall: Arc<OverallLimiter>) -> Self {
        Self::with_limiters(overall, OverallLimiter::new())
    }

    pub fn with_limiters(overall: Arc<OverallLimiter>, overall_up: Arc<OverallLimiter>) -> Self {
        Self {
            total: Arc::new(AtomicU64::new(0)),
            completed: Arc::new(AtomicU64::new(0)),
            halt: Arc::new(AtomicBool::new(false)),
            peers: Arc::new(Mutex::new(Vec::new())),
            servers: Arc::new(Mutex::new(Vec::new())),
            overall,
            overall_up,
            seeding: Arc::new(AtomicBool::new(false)),
            checksum_done: Arc::new(AtomicBool::new(false)),
        }
    }
}

pub fn record_server(progress: &HttpProgress, uri: &str) {
    let mut g = progress.servers.lock().unwrap();
    g.clear();
    g.push(ServerStat {
        index: 1,
        uri: uri.to_string(),
        current_uri: uri.to_string(),
    });
}

impl Default for HttpProgress {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct HttpJob {
    pub uris: Vec<String>,
    pub dest: PathBuf,
    pub opts: OptionSet,
    pub progress: HttpProgress,
    pub piece_length: u32,
    pub cancel: watch::Receiver<bool>,
}

/// C++ `--min-tls-version` (prefs.h PREF_MIN_TLS_VERSION). rustls cannot
/// speak TLS 1.1, so TLSv1.1/TLSv1 floor at 1.2 — same default as C++ (1.2).
fn min_tls_version(opts: &OptionSet) -> reqwest::tls::Version {
    match opts.get("min-tls-version").unwrap_or("TLSv1.2") {
        "TLSv1.3" => reqwest::tls::Version::TLS_1_3,
        _ => reqwest::tls::Version::TLS_1_2,
    }
}

fn load_ca_certs(pem: &[u8]) -> Result<Vec<reqwest::Certificate>> {
    let text = std::str::from_utf8(pem).map_err(|_| Error::Http("ca-certificate not utf-8".into()))?;
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("-----BEGIN CERTIFICATE-----") {
        rest = &rest[i..];
        let Some(end) = rest.find("-----END CERTIFICATE-----") else {
            return Err(Error::Http("truncated ca-certificate PEM".into()));
        };
        let block = rest[..end + "-----END CERTIFICATE-----".len()].as_bytes();
        out.push(reqwest::Certificate::from_pem(block)?);
        rest = &rest[end + 1..];
    }
    if out.is_empty() {
        return Err(Error::Http("no certificates in ca-certificate".into()));
    }
    Ok(out)
}

pub fn client(opts: &OptionSet) -> Result<Client> {
    let timeout = opts.u64("timeout", 60);
    let ka = opts.bool("enable-http-keep-alive", true);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONNECTION,
        HeaderValue::from_static(if ka { "keep-alive" } else { "close" }),
    );
    let mut b = Client::builder()
        .user_agent(opts.user_agent().to_string())
        .connect_timeout(Duration::from_secs(opts.u64("connect-timeout", 60).max(1)))
        .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECT as usize))
        .gzip(opts.bool("http-accept-gzip", true))
        .deflate(opts.bool("http-accept-gzip", true))
        .min_tls_version(min_tls_version(opts))
        .http1_only()
        .default_headers(headers)
        .pool_max_idle_per_host(if ka { 8 } else { 0 });
    b = b.no_proxy();
    if timeout > 0 {
        b = b.timeout(Duration::from_secs(timeout));
    }
    if !opts.bool("check-certificate", true) {
        b = b.danger_accept_invalid_certs(true);
    }
    if let Some(path) = opts.get("ca-certificate").filter(|s| !s.is_empty()) {
        let pem = std::fs::read(path).map_err(|e| Error::Http(format!("ca-certificate: {e}")))?;
        for cert in load_ca_certs(&pem)? {
            b = b.add_root_certificate(cert);
        }
    }
    if let Some(cert_path) = opts.get("certificate").filter(|s| !s.is_empty()) {
        let mut pem = std::fs::read(cert_path)
            .map_err(|e| Error::Http(format!("certificate: {e}")))?;
        if let Some(key_path) = opts.get("private-key").filter(|s| !s.is_empty()) {
            pem.push(b'\n');
            pem.extend(
                std::fs::read(key_path).map_err(|e| Error::Http(format!("private-key: {e}")))?,
            );
        }
        let id = reqwest::Identity::from_pem(&pem)
            .map_err(|e| Error::Http(format!("certificate/private-key: {e}")))?;
        b = b.identity(id);
    }
    b = b.default_headers(extra_headers(opts)?);
    if let Some(ip) = bind_local_ip(opts) {
        b = b.local_address(ip);
    }
    b = apply_proxy(b, opts)?;
    Ok(b.build()?)
}

/// C++ `--interface` / `--multiple-interface` / `--disable-ipv6`.
fn bind_local_ip(opts: &OptionSet) -> Option<IpAddr> {
    let disable_v6 = opts.bool("disable-ipv6", false);
    let mut cands: Vec<&str> = Vec::new();
    if let Some(m) = opts.get("multiple-interface").filter(|s| !s.is_empty()) {
        cands.extend(m.split(',').map(str::trim).filter(|s| !s.is_empty()));
    } else if let Some(i) = opts.get("interface").filter(|s| !s.is_empty()) {
        cands.push(i);
    }
    let mut fallback = None;
    for c in cands {
        if let Some(ip) = resolve_interface(c) {
            if disable_v6 && ip.is_ipv6() {
                continue;
            }
            fallback = Some(ip);
            if local_ip_usable(ip) {
                return Some(ip);
            }
        }
    }
    if fallback.is_some() {
        return fallback;
    }
    if disable_v6 {
        return Some(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    }
    None
}

fn resolve_interface(s: &str) -> Option<IpAddr> {
    if let Ok(ip) = s.parse::<IpAddr>() {
        return Some(ip);
    }
    if s == "lo" || s == "lo0" {
        return Some(IpAddr::V4(Ipv4Addr::LOCALHOST));
    }
    format!("{s}:0")
        .to_socket_addrs()
        .ok()?
        .next()
        .map(|a| a.ip())
}

fn local_ip_usable(ip: IpAddr) -> bool {
    std::net::UdpSocket::bind((ip, 0)).is_ok()
}

fn apply_proxy(b: reqwest::ClientBuilder, opts: &OptionSet) -> Result<reqwest::ClientBuilder> {
    let http = opts.get("http-proxy").filter(|s| !s.is_empty());
    let https = opts.get("https-proxy").filter(|s| !s.is_empty());
    let all = opts.get("all-proxy").filter(|s| !s.is_empty());
    if http.is_none() && https.is_none() && all.is_none() {
        return Ok(b);
    }
    let np = opts.get("no-proxy").filter(|s| !s.is_empty());
    let norm = |raw: &str| {
        if raw.contains("://") {
            raw.to_string()
        } else {
            format!("http://{raw}")
        }
    };

    if https.is_some() && http.is_none() && all.is_none() {
        let url = norm(https.unwrap());
        let mut proxy = Proxy::https(&url).map_err(|e| Error::Http(format!("https-proxy: {e}")))?;
        let user = opts
            .get("https-proxy-user")
            .or_else(|| opts.get("all-proxy-user"))
            .filter(|s| !s.is_empty());
        if let Some(u) = user {
            let pw = opts
                .get("https-proxy-passwd")
                .or_else(|| opts.get("all-proxy-passwd"))
                .unwrap_or("");
            proxy = proxy.basic_auth(u, pw);
        }
        if let Some(np) = np {
            proxy = proxy.no_proxy(reqwest::NoProxy::from_string(np));
        }
        return Ok(b.proxy(proxy));
    }

    if https.is_some() {
        let http_url = http.map(norm);
        let https_url = https.map(norm);
        let all_url = all.map(norm);
        let np = np.map(|s| s.to_string());
        let http_user = opts
            .get("http-proxy-user")
            .or_else(|| opts.get("all-proxy-user"))
            .map(|s| s.to_string());
        let https_user = opts
            .get("https-proxy-user")
            .or_else(|| opts.get("all-proxy-user"))
            .map(|s| s.to_string());
        let http_pw = opts
            .get("http-proxy-passwd")
            .or_else(|| opts.get("all-proxy-passwd"))
            .unwrap_or("")
            .to_string();
        let https_pw = opts
            .get("https-proxy-passwd")
            .or_else(|| opts.get("all-proxy-passwd"))
            .unwrap_or("")
            .to_string();
        let with_auth = |raw: String, user: Option<&str>, pw: &str| {
            if let Some(u) = user.filter(|s| !s.is_empty()) {
                if let Ok(mut p) = url::Url::parse(&raw) {
                    let _ = p.set_username(u);
                    let _ = p.set_password(Some(pw));
                    return p.to_string();
                }
            }
            raw
        };
        let http_url = http_url.map(|u| with_auth(u, http_user.as_deref(), &http_pw));
        let https_url = https_url.map(|u| with_auth(u, https_user.as_deref(), &https_pw));
        let all_url = all_url.map(|u| with_auth(u, http_user.as_deref(), &http_pw));
        let proxy = Proxy::custom(move |url| {
            if let (Some(h), Some(np)) = (url.host_str(), np.as_deref()) {
                if np.split(',').map(|s| s.trim()).any(|p| {
                    !p.is_empty()
                        && (p == "*"
                            || h.eq_ignore_ascii_case(p)
                            || h.to_ascii_lowercase()
                                .ends_with(&format!(".{}", p.to_ascii_lowercase()))
                        )
                }) {
                    return None;
                }
            }
            let chosen = match url.scheme() {
                "https" => https_url.as_deref().or(all_url.as_deref()),
                "http" => http_url.as_deref().or(all_url.as_deref()),
                _ => all_url.as_deref(),
            };
            chosen.and_then(|s| s.parse::<url::Url>().ok())
        });
        return Ok(b.proxy(proxy));
    }

    let raw = http.or(all).unwrap();
    let url = norm(raw);
    let mut proxy = if http.is_some() {
        Proxy::http(&url)
    } else {
        Proxy::all(&url)
    }
    .map_err(|e| Error::Http(format!("proxy: {e}")))?;
    let user = opts
        .get("http-proxy-user")
        .or_else(|| opts.get("all-proxy-user"))
        .filter(|s| !s.is_empty());
    if let Some(u) = user {
        let pw = opts
            .get("http-proxy-passwd")
            .or_else(|| opts.get("all-proxy-passwd"))
            .unwrap_or("");
        proxy = proxy.basic_auth(u, pw);
    }
    if let Some(np) = np {
        proxy = proxy.no_proxy(reqwest::NoProxy::from_string(np));
    }
    Ok(b.proxy(proxy))
}

static LAST_PROXY_METHOD: AtomicU64 = AtomicU64::new(0);

pub fn last_proxy_method() -> &'static str {
    if LAST_PROXY_METHOD.load(Ordering::SeqCst) == 1 {
        "tunnel"
    } else {
        "get"
    }
}

fn record_proxy_method(opts: &OptionSet) {
    let tunnel = opts
        .get("proxy-method")
        .unwrap_or("get")
        .eq_ignore_ascii_case("tunnel");
    LAST_PROXY_METHOD.store(u64::from(tunnel), Ordering::SeqCst);
}

fn http_proxy_raw(opts: &OptionSet) -> Option<&str> {
    opts.get("http-proxy")
        .or_else(|| opts.get("all-proxy"))
        .filter(|s| !s.is_empty())
}

fn want_http_tunnel(uri: &str, opts: &OptionSet) -> bool {
    uri.starts_with("http://")
        && opts
            .get("proxy-method")
            .unwrap_or("get")
            .eq_ignore_ascii_case("tunnel")
        && http_proxy_raw(opts).is_some()
}

fn want_https_socket(uri: &str, opts: &OptionSet) -> bool {
    uri.starts_with("https://")
        && opts.get("certificate").filter(|s| !s.is_empty()).is_none()
        && opts.get("https-proxy").filter(|s| !s.is_empty()).is_none()
        && opts.get("all-proxy").filter(|s| !s.is_empty()).is_none()
        && opts.get("http-proxy").filter(|s| !s.is_empty()).is_none()
}

fn want_https_connect(uri: &str, opts: &OptionSet) -> bool {
    uri.starts_with("https://")
        && opts.get("certificate").filter(|s| !s.is_empty()).is_none()
        && (opts.get("https-proxy").filter(|s| !s.is_empty()).is_some()
            || opts.get("all-proxy").filter(|s| !s.is_empty()).is_some()
            || opts.get("http-proxy").filter(|s| !s.is_empty()).is_some())
}

fn want_http_socket(uri: &str, opts: &OptionSet) -> bool {
    uri.starts_with("http://") && http_proxy_raw(opts).is_none()
}

/// C++ HttpRequestPool / HttpKeepAliveConnection keyed by origin.
struct PooledHttp {
    stream: TcpStream,
    leftover: SocketLeftover,
}

static HTTP_KA_POOL: std::sync::OnceLock<Mutex<HashMap<String, Vec<PooledHttp>>>> =
    std::sync::OnceLock::new();
static LAST_HTTP_KA_REUSE: AtomicU64 = AtomicU64::new(0);

fn ka_pool() -> &'static Mutex<HashMap<String, Vec<PooledHttp>>> {
    HTTP_KA_POOL.get_or_init(|| Mutex::new(HashMap::new()))
}

fn ka_key(host: &str, port: u16) -> String {
    format!("{host}:{port}")
}

fn take_ka(host: &str, port: u16) -> Option<PooledHttp> {
    ka_pool()
        .lock()
        .ok()?
        .get_mut(&ka_key(host, port))
        .and_then(|v| v.pop())
}

fn put_ka(host: &str, port: u16, conn: PooledHttp) {
    if let Ok(mut g) = ka_pool().lock() {
        g.entry(ka_key(host, port)).or_default().push(conn);
    }
}

/// C++ HttpKeepAliveConnection over TLS SocketCore.
type HttpsTls = tokio_rustls::client::TlsStream<TcpStream>;

struct PooledHttps {
    tls: HttpsTls,
    leftover: SocketLeftover,
}

static HTTPS_KA_POOL: std::sync::OnceLock<Mutex<HashMap<String, Vec<PooledHttps>>>> =
    std::sync::OnceLock::new();
static LAST_HTTPS_KA_REUSE: AtomicU64 = AtomicU64::new(0);

fn https_ka_pool() -> &'static Mutex<HashMap<String, Vec<PooledHttps>>> {
    HTTPS_KA_POOL.get_or_init(|| Mutex::new(HashMap::new()))
}

fn take_https_ka(host: &str, port: u16) -> Option<PooledHttps> {
    https_ka_pool()
        .lock()
        .ok()?
        .get_mut(&ka_key(host, port))
        .and_then(|v| v.pop())
}

fn put_https_ka(host: &str, port: u16, conn: PooledHttps) {
    if let Ok(mut g) = https_ka_pool().lock() {
        g.entry(ka_key(host, port)).or_default().push(conn);
    }
}

async fn http_open(
    host: &str,
    port: u16,
    opts: &OptionSet,
    connect: Duration,
    allow_reuse: bool,
) -> Result<(TcpStream, SocketLeftover, bool)> {
    if allow_reuse {
        if let Some(p) = take_ka(host, port) {
            LAST_HTTP_KA_REUSE.fetch_add(1, Ordering::SeqCst);
            return Ok((p.stream, p.leftover, true));
        }
    }
    let stream = connect_http(host, port, opts, connect).await?;
    let _ = crate::sockopt::apply_recv_buffer(&stream, opts);
    let _ = crate::sockopt::apply_tcp_nodelay(&stream);
    let _ = crate::sockopt::apply_tcp_quickack(&stream);
    Ok((stream, SocketLeftover::new(), false))
}

async fn connect_http(
    host: &str,
    port: u16,
    opts: &OptionSet,
    connect: Duration,
) -> Result<TcpStream> {
    let disable_v6 = opts.bool("disable-ipv6", false);
    let mut addrs: Vec<std::net::SocketAddr> = Vec::new();
    if let Ok(ip) = host
        .trim_matches(|c| c == '[' || c == ']')
        .parse::<std::net::IpAddr>()
    {
        if ip.is_ipv6() && disable_v6 {
            return Err(Error::Http("disable-ipv6".into()));
        }
        addrs.push(std::net::SocketAddr::new(ip, port));
    } else {
        addrs.extend(
            tokio::net::lookup_host((host, port))
                .await
                .map_err(|e| Error::Http(format!("http dns {host}: {e}")))?
                .filter(|sa| !(disable_v6 && sa.is_ipv6())),
        );
    }
    // Unique targets (getaddrinfo repeats STREAM/DGRAM).
    let mut uniq = Vec::new();
    for a in addrs {
        if !uniq.contains(&a) {
            uniq.push(a);
        }
    }
    if uniq.is_empty() {
        return Err(Error::Http(format!("http dns empty {host}")));
    }
    connect_happy(uniq, opts, connect).await
}

/// C++ SocketCore Happy Eyeballs (RFC 6555): AAAA first, 300ms later A in
/// parallel; first TCP success wins. Instant AAAA failure starts A immediately.
const HAPPY_EYEBALLS_DELAY: Duration = Duration::from_millis(300);
static LAST_HE_WIN: AtomicU32 = AtomicU32::new(0);
static LAST_HE_V6_TRY: AtomicU64 = AtomicU64::new(0);
static LAST_HE_V4_TRY: AtomicU64 = AtomicU64::new(0);

pub fn last_he_win() -> u32 {
    LAST_HE_WIN.load(Ordering::SeqCst)
}

pub fn last_he_v6_try() -> u64 {
    LAST_HE_V6_TRY.load(Ordering::SeqCst)
}

pub fn last_he_v4_try() -> u64 {
    LAST_HE_V4_TRY.load(Ordering::SeqCst)
}

async fn connect_serial(
    addrs: &[std::net::SocketAddr],
    opts: &OptionSet,
    connect: Duration,
) -> Result<TcpStream> {
    let mut last = Error::Http("http connect".into());
    for target in addrs {
        match connect_bound(*target, opts, connect).await {
            Ok(s) => return Ok(s),
            Err(e) => last = e,
        }
    }
    Err(last)
}

async fn connect_happy(
    addrs: Vec<std::net::SocketAddr>,
    opts: &OptionSet,
    connect: Duration,
) -> Result<TcpStream> {
    let v6: Vec<_> = addrs.iter().copied().filter(|a| a.is_ipv6()).collect();
    let v4: Vec<_> = addrs.iter().copied().filter(|a| a.is_ipv4()).collect();
    if v6.is_empty() || v4.is_empty() {
        return connect_serial(&addrs, opts, connect).await;
    }
    LAST_HE_V6_TRY.fetch_add(1, Ordering::SeqCst);
    let opts6 = opts.clone();
    let opts4 = opts.clone();
    let v6c = v6.clone();
    let v4c = v4.clone();
    tokio::select! {
        r = connect_serial(&v6c, &opts6, connect) => {
            match r {
                Ok(s) => {
                    LAST_HE_WIN.store(6, Ordering::SeqCst);
                    Ok(s)
                }
                Err(_) => {
                    LAST_HE_V4_TRY.fetch_add(1, Ordering::SeqCst);
                    let s = connect_serial(&v4, opts, connect).await?;
                    LAST_HE_WIN.store(4, Ordering::SeqCst);
                    Ok(s)
                }
            }
        }
        r = async {
            tokio::time::sleep(HAPPY_EYEBALLS_DELAY).await;
            LAST_HE_V4_TRY.fetch_add(1, Ordering::SeqCst);
            connect_serial(&v4c, &opts4, connect).await
        } => {
            match r {
                Ok(s) => {
                    LAST_HE_WIN.store(4, Ordering::SeqCst);
                    Ok(s)
                }
                Err(_) => {
                    let s = connect_serial(&v6, opts, connect).await?;
                    LAST_HE_WIN.store(6, Ordering::SeqCst);
                    Ok(s)
                }
            }
        }
    }
}

async fn connect_bound(
    target: std::net::SocketAddr,
    opts: &OptionSet,
    connect: Duration,
) -> Result<TcpStream> {
    let sock = if target.is_ipv4() {
        tokio::net::TcpSocket::new_v4().map_err(|e| Error::Http(e.to_string()))?
    } else {
        tokio::net::TcpSocket::new_v6().map_err(|e| Error::Http(e.to_string()))?
    };
    if let Some(ip) = bind_local_ip(opts) {
        if ip.is_ipv4() != target.is_ipv4() {
            return Err(Error::Http("interface family".into()));
        }
        sock.bind(std::net::SocketAddr::new(ip, 0))
            .map_err(|e| Error::Http(format!("interface: {e}")))?;
    }
    tokio::time::timeout(connect, sock.connect(target))
        .await
        .map_err(|_| Error::Http("http connect timeout".into()))?
        .map_err(|e| Error::Http(e.to_string()))
}

/// C++ SocketCore HttpRequest GET: rustix send/recv then stream pwrite (not slurped).
/// C++ HttpKeepAliveConnection: reuse SocketCore when `--enable-http-keep-alive=true`.
/// C++ `Request::MAX_REDIRECT` / HttpSkipResponseCommand::processRedirect.
const MAX_REDIRECT: u32 = 20;

enum HttpOnce {
    Ok,
    Unauthorized,
    Redirect(String),
}

fn is_http_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

fn resolve_redirect(base: &url::Url, loc: &str) -> Result<String> {
    let loc = loc.trim();
    if loc.is_empty() {
        return Err(Error::Http("redirect without Location".into()));
    }
    base.join(loc)
        .or_else(|_| url::Url::parse(loc))
        .map(|u| u.to_string())
        .map_err(|e| Error::Http(format!("redirect: {e}")))
}

async fn http_fetch(
    uri: &str,
    store: &FileStorage,
    start: u64,
    end: Option<u64>,
    progress: &HttpProgress,
    cancel: &mut watch::Receiver<bool>,
    jar: &CookieJar,
    opts: &OptionSet,
) -> Result<()> {
    let mut uri = uri.to_string();
    let mut redirects = 0u32;
    let mut with_auth = !opts.bool("http-auth-challenge", false);
    loop {
        let url = url::Url::parse(&uri).map_err(|e| Error::Http(e.to_string()))?;
        let host = url
            .host_str()
            .ok_or_else(|| Error::Http("http host".into()))?
            .to_string();
        let port = url.port_or_known_default().unwrap_or(80);
        let connect = Duration::from_secs(opts.u64("connect-timeout", 60).max(1));
        let ka = opts.bool("enable-http-keep-alive", true);
        match http_fetch_once(
            &url,
            &uri,
            &host,
            port,
            store,
            start,
            end,
            progress,
            cancel,
            jar,
            opts,
            connect,
            ka,
            with_auth,
        )
        .await
        {
            Ok(HttpOnce::Ok) => return Ok(()),
            Ok(HttpOnce::Unauthorized)
                if !with_auth
                    && opts.bool("http-auth-challenge", false)
                    && auth_header(opts, &uri).is_some() =>
            {
                with_auth = true;
                continue;
            }
            Ok(HttpOnce::Unauthorized) => return Err(Error::Http("status 401".into())),
            Ok(HttpOnce::Redirect(next)) => {
                if redirects >= MAX_REDIRECT {
                    return Err(Error::Http(format!(
                        "too many redirects: count={redirects}"
                    )));
                }
                redirects += 1;
                uri = next;
                with_auth = !opts.bool("http-auth-challenge", false);
            }
            Err(e) => return Err(e),
        }
    }
}

async fn http_fetch_once(
    url: &url::Url,
    uri: &str,
    host: &str,
    port: u16,
    store: &FileStorage,
    start: u64,
    end: Option<u64>,
    progress: &HttpProgress,
    cancel: &mut watch::Receiver<bool>,
    jar: &CookieJar,
    opts: &OptionSet,
    connect: Duration,
    ka: bool,
    with_auth: bool,
) -> Result<HttpOnce> {
    let mut allow_reuse = ka;
    for _ in 0..2u8 {
        let (stream, mut leftover, reused) =
            http_open(host, port, opts, connect, allow_reuse).await?;
        let req = build_origin_get(url, uri, start, end, jar, opts, with_auth);
        if let Err(e) = crate::sockopt::send_all(&stream, req.as_bytes()).await {
            if reused {
                allow_reuse = false;
                continue;
            }
            return Err(e);
        }
        LAST_HTTP_SEND.fetch_add(1, Ordering::SeqCst);
        let meta = match read_http1_meta(&stream, &mut leftover).await {
            Ok(m) => m,
            Err(_) if reused => {
                allow_reuse = false;
                continue;
            }
            Err(e) => return Err(e),
        };
        LAST_HTTP_RECV.fetch_add(1, Ordering::SeqCst);
        absorb_set_cookie_lines(jar, uri, &meta.set_cookies);
        if meta.status == 401 {
            return Ok(HttpOnce::Unauthorized);
        }
        if is_http_redirect(meta.status) {
            skip_http_body(&stream, &mut leftover, meta.clen).await?;
            let loc = meta.location.as_deref().unwrap_or("");
            let next = resolve_redirect(url, loc)?;
            if ka && !meta.connection_close {
                put_ka(
                    host,
                    port,
                    PooledHttp {
                        stream,
                        leftover,
                    },
                );
            }
            return Ok(HttpOnce::Redirect(next));
        }
        if meta.status != 206 && meta.status != 200 {
            return Err(Error::Http(format!("status {}", meta.status)));
        }
        if start > 0 && meta.status == 200 {
            return Err(Error::Http("resume not possible".into()));
        }
        let want = match end {
            Some(e) => e.saturating_sub(start).saturating_add(1),
            None => meta.clen,
        };
        let left = meta.content_length.or(end.map(|_| want));
        stream_http_body(
            &stream,
            &mut leftover,
            store,
            start,
            left,
            &meta.encoding,
            opts.bool("http-accept-gzip", true),
            progress,
            cancel,
            opts,
            end.is_none(),
        )
        .await?;
        if ka && !meta.connection_close {
            put_ka(
                host,
                port,
                PooledHttp {
                    stream,
                    leftover,
                },
            );
        }
        return Ok(HttpOnce::Ok);
    }
    Err(Error::Http("http keep-alive retry failed".into()))
}

fn build_origin_get(
    url: &url::Url,
    uri: &str,
    start: u64,
    end: Option<u64>,
    jar: &CookieJar,
    opts: &OptionSet,
    with_auth: bool,
) -> String {
    let host = url.host_str().unwrap_or("");
    let port = url.port_or_known_default().unwrap_or(80);
    let path = match url.query() {
        Some(q) => format!("{}?{q}", url.path()),
        None => {
            let p = url.path();
            if p.is_empty() {
                "/".to_string()
            } else {
                p.to_string()
            }
        }
    };
    let host_hdr = if url.port().is_some() {
        format!("{host}:{port}")
    } else {
        host.to_string()
    };
    let ua = opts.get("user-agent").unwrap_or(crate::USER_AGENT);
    let ka = opts.bool("enable-http-keep-alive", true);
    let mut req = format!(
        "GET {path} HTTP/1.1\r\nHost: {host_hdr}\r\nUser-Agent: {ua}\r\nAccept: */*\r\nConnection: {}\r\n",
        if ka { "keep-alive" } else { "close" }
    );
    if opts.bool("http-accept-gzip", true) {
        req.push_str("Accept-Encoding: gzip, deflate\r\n");
    } else {
        req.push_str("Accept-Encoding: identity\r\n");
    }
    if let Some(end) = end {
        req.push_str(&format!("Range: bytes={start}-{end}\r\n"));
    } else if start > 0 {
        req.push_str(&format!("Range: bytes={start}-\r\n"));
    }
    if opts.bool("http-no-cache", true) {
        req.push_str("Cache-Control: no-cache\r\nPragma: no-cache\r\n");
    }
    if with_auth {
        if let Some(a) = auth_header(opts, uri) {
            req.push_str(&format!("Authorization: {a}\r\n"));
        }
    }
    if !opts.bool("no-want-digest-header", false) {
        req.push_str("Want-Digest: SHA-512;q=1, SHA-256;q=1, SHA;q=0.1\r\n");
    }
    if let Some(ims) = opts.get("if-modified-since").filter(|s| !s.is_empty()) {
        req.push_str(&format!("If-Modified-Since: {ims}\r\n"));
    }
    if let Some(c) = cookie_header(jar, uri) {
        req.push_str(&format!("Cookie: {c}\r\n"));
    }
    if let Some(r) = opts.get("referer").filter(|s| !s.is_empty()) {
        req.push_str(&format!("Referer: {r}\r\n"));
    }
    if let Some(raw) = opts.get("header").filter(|s| !s.is_empty()) {
        for line in raw.split('\n') {
            let line = line.trim();
            if line.is_empty() || !line.contains(':') {
                continue;
            }
            req.push_str(line);
            req.push_str("\r\n");
        }
    }
    req.push_str("\r\n");
    req
}

fn cookie_header(jar: &CookieJar, uri: &str) -> Option<String> {
    let g = jar.lock().unwrap();
    crate::cookies::header_for(&g, uri)
}

fn absorb_set_cookie_lines(jar: &CookieJar, uri: &str, lines: &[String]) {
    let url = url::Url::parse(uri).ok();
    let host = url.as_ref().and_then(|u| u.host_str()).unwrap_or("");
    let path = url.as_ref().map(|u| u.path()).unwrap_or("/");
    let mut g = jar.lock().unwrap();
    for s in lines {
        let Some(c) = crate::cookies::parse_set_cookie(s, host, path) else {
            continue;
        };
        g.retain(|x| !(x.name == c.name && x.domain == c.domain));
        g.push(c);
    }
}

/// C++ SocketBuffer leftover: pwrite/cache from buffer without extra Vec; clone only on write_at.
async fn leftover_write(
    store: &FileStorage,
    leftover: &mut SocketLeftover,
    off: u64,
    take: usize,
) -> Result<()> {
    let take = take.min(leftover.len());
    if take == 0 {
        return Ok(());
    }
    let slice = leftover.available();
    let slice = &slice[..take];
    if store.try_pwrite(off, slice)? || store.try_cache(off, slice)? {
        LAST_LEFTOVER_INLINE.fetch_add(1, Ordering::SeqCst);
        leftover.consume(take);
        return Ok(());
    }
    let chunk = slice.to_vec();
    leftover.consume(take);
    store.write_at(off, &chunk).await
}

/// C++ SocketBuffer recv window: pwrite/cache the socket tmp without leftover.extend.
async fn recv_window_write(store: &FileStorage, off: u64, tmp: &[u8]) -> Result<()> {
    if tmp.is_empty() {
        return Ok(());
    }
    if store.try_pwrite(off, tmp)? || store.try_cache(off, tmp)? {
        LAST_RECV_INLINE.fetch_add(1, Ordering::SeqCst);
        return Ok(());
    }
    store.write_at(off, tmp).await
}

async fn stream_http_body(
    s: &TcpStream,
    leftover: &mut SocketLeftover,
    store: &FileStorage,
    mut off: u64,
    body_length: Option<u64>,
    encoding: &str,
    accept_gzip: bool,
    progress: &HttpProgress,
    cancel: &mut watch::Receiver<bool>,
    opts: &OptionSet,
    update_total: bool,
) -> Result<()> {
    use std::io::Write;
    let enc = encoding.to_ascii_lowercase();
    let decode = accept_gzip && (enc == "gzip" || enc == "deflate" || enc == "x-gzip");
    let mut gz = if decode && enc != "deflate" {
        Some(flate2::write::GzDecoder::new(Vec::<u8>::new()))
    } else {
        None
    };
    let mut zl = if decode && enc == "deflate" {
        Some(flate2::write::ZlibDecoder::new(Vec::<u8>::new()))
    } else {
        None
    };
    let mut rate = RateCtl::new(opts);
    let mut tmp = vec![0u8; 128 * 1024];
    let until_eof = body_length.is_none();
    let mut left = body_length.unwrap_or(0);
    let idle = Duration::from_secs(opts.u64("timeout", 60).max(1));
    let _ = cancel.borrow_and_update();
    let mut watch_cancel = true;
    let mut ticks = 0u32;
    loop {
        ticks = ticks.wrapping_add(1);
        if ticks % 16 == 1 && (progress.halt.load(Ordering::Relaxed) || *cancel.borrow()) {
            return Err(Error::Http("canceled".into()));
        }
        if leftover.is_empty() {
            if !until_eof && left == 0 {
                break;
            }
            let cap = if until_eof {
                tmp.len()
            } else {
                left.min(tmp.len() as u64) as usize
            };
            if cap == 0 {
                break;
            }
            let n = if watch_cancel {
                tokio::select! {
                    biased;
                    r = crate::sockopt::recv_some_idle(s, &mut tmp[..cap], Some(idle)) => {
                        r?
                    }
                    changed = cancel.changed() => {
                        if changed.is_ok() && *cancel.borrow() {
                            return Err(Error::Http("canceled".into()));
                        }
                        if changed.is_err() {
                            watch_cancel = false;
                        }
                        continue;
                    }
                }
            } else {
                crate::sockopt::recv_some_idle(s, &mut tmp[..cap], Some(idle)).await?
            };
            LAST_HTTP_RECV.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                if !until_eof && left > 0 {
                    store.flush().await?;
                    return Err(Error::Http("eof before complete response body".into()));
                }
                break;
            }
            if !decode {
                // C++ SocketBuffer: pwrite the recv window; leftover.extend only the tail past `left`.
                let take = if until_eof {
                    n
                } else {
                    (left as usize).min(n)
                };
                if take == 0 {
                    leftover.extend(&tmp[..n]);
                    break;
                }
                recv_window_write(store, off, &tmp[..take]).await?;
                if take < n {
                    leftover.extend(&tmp[take..n]);
                }
                if !until_eof {
                    left -= take as u64;
                }
                off += take as u64;
                progress
                    .completed
                    .fetch_add(take as u64, Ordering::Relaxed);
                rate.after(take as u64).await?;
                progress.overall.after(take as u64).await;
                if !until_eof && left == 0 {
                    break;
                }
                continue;
            }
            leftover.extend(&tmp[..n]);
        }
        let take = if until_eof {
            leftover.len()
        } else {
            (left as usize).min(leftover.len())
        };
        if take == 0 {
            break;
        }
        if !until_eof {
            left -= take as u64;
        }
        if let Some(d) = gz.as_mut() {
            {
                let avail = leftover.available();
                d.write_all(&avail[..take])
                    .map_err(|e| Error::Http(format!("gzip: {e}")))?;
            }
            leftover.consume(take);
            let decoded = std::mem::take(d.get_mut());
            if !decoded.is_empty() {
                write_decoded(store, &mut off, &decoded, progress, &mut rate).await?;
            }
        } else if let Some(d) = zl.as_mut() {
            {
                let avail = leftover.available();
                d.write_all(&avail[..take])
                    .map_err(|e| Error::Http(format!("deflate: {e}")))?;
            }
            leftover.consume(take);
            let decoded = std::mem::take(d.get_mut());
            if !decoded.is_empty() {
                write_decoded(store, &mut off, &decoded, progress, &mut rate).await?;
            }
        } else {
            leftover_write(store, leftover, off, take).await?;
            off += take as u64;
            progress
                .completed
                .fetch_add(take as u64, Ordering::Relaxed);
            rate.after(take as u64).await?;
            progress.overall.after(take as u64).await;
        }
        if !until_eof && left == 0 {
            break;
        }
    }
    if let Some(d) = gz.as_mut() {
        d.try_finish()
            .map_err(|e| Error::Http(format!("gzip finish: {e}")))?;
        let decoded = std::mem::take(d.get_mut());
        if !decoded.is_empty() {
            write_decoded(store, &mut off, &decoded, progress, &mut rate).await?;
        }
    }
    if let Some(d) = zl.as_mut() {
        d.try_finish()
            .map_err(|e| Error::Http(format!("deflate finish: {e}")))?;
        let decoded = std::mem::take(d.get_mut());
        if !decoded.is_empty() {
            write_decoded(store, &mut off, &decoded, progress, &mut rate).await?;
        }
    }
    if update_total && off > 0 {
        progress.total.store(off, Ordering::Relaxed);
    }
    store.flush().await?;
    Ok(())
}

/// C++ HttpSkipResponseCommand: drain body so keep-alive reuse is not a half-read conn.
async fn skip_http_body(
    stream: &TcpStream,
    leftover: &mut SocketLeftover,
    mut left: u64,
) -> Result<()> {
    let mut tmp = [0u8; 16 * 1024];
    while left > 0 {
        if leftover.is_empty() {
            let cap = left.min(tmp.len() as u64) as usize;
            let n = crate::sockopt::recv_some(stream, &mut tmp[..cap]).await?;
            LAST_HTTP_RECV.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                break;
            }
            leftover.extend(&tmp[..n]);
        }
        let n = leftover.len().min(left as usize);
        leftover.consume(n);
        left -= n as u64;
    }
    Ok(())
}

async fn write_decoded(
    store: &FileStorage,
    off: &mut u64,
    buf: &[u8],
    progress: &HttpProgress,
    rate: &mut RateCtl,
) -> Result<()> {
    if buf.is_empty() {
        return Ok(());
    }
    if !store.try_pwrite(*off, buf)? && !store.try_cache(*off, buf)? {
        store.write_at(*off, buf).await?;
    }
    *off += buf.len() as u64;
    progress
        .completed
        .fetch_add(buf.len() as u64, Ordering::Relaxed);
    rate.after(buf.len() as u64).await?;
    progress.overall.after(buf.len() as u64).await;
    Ok(())
}

/// C++ SocketCore TLS HttpRequest GET: rustls records then stream pwrite (not slurped).
/// C++ HttpSkipResponseCommand::processRedirect over TLS (`Request::MAX_REDIRECT`).
async fn https_fetch(
    uri: &str,
    store: &FileStorage,
    start: u64,
    end: Option<u64>,
    progress: &HttpProgress,
    cancel: &mut watch::Receiver<bool>,
    opts: &OptionSet,
) -> Result<()> {
    let mut uri = uri.to_string();
    let mut redirects = 0u32;
    loop {
        let url = url::Url::parse(&uri).map_err(|e| Error::Http(e.to_string()))?;
        let host = url
            .host_str()
            .ok_or_else(|| Error::Http("https host".into()))?
            .to_string();
        let port = url.port_or_known_default().unwrap_or(443);
        let connect = Duration::from_secs(opts.u64("connect-timeout", 60).max(1));
        let ka = opts.bool("enable-http-keep-alive", true);
        let mut allow_reuse = ka;
        let outcome = loop {
            let (mut tls, mut leftover, reused) =
                https_open(&host, port, opts, connect, allow_reuse).await?;
            match https_roundtrip(
                &mut tls,
                &mut leftover,
                &url,
                store,
                start,
                end,
                progress,
                cancel,
                opts,
            )
            .await
            {
                Ok(o) => {
                    let close = match &o {
                        HttpsOnce::Done(c) | HttpsOnce::Redirect(_, c) => *c,
                    };
                    if ka && !close {
                        put_https_ka(&host, port, PooledHttps { tls, leftover });
                    }
                    break o;
                }
                Err(_) if reused => {
                    allow_reuse = false;
                    continue;
                }
                Err(e) => return Err(e),
            }
        };
        match outcome {
            HttpsOnce::Done(_) => return Ok(()),
            HttpsOnce::Redirect(next, _) => {
                if redirects >= MAX_REDIRECT {
                    return Err(Error::Http(format!(
                        "too many redirects: count={redirects}"
                    )));
                }
                redirects += 1;
                uri = next;
            }
        }
    }
}

async fn https_open(
    host: &str,
    port: u16,
    opts: &OptionSet,
    connect: Duration,
    allow_reuse: bool,
) -> Result<(HttpsTls, SocketLeftover, bool)> {
    if allow_reuse {
        if let Some(p) = take_https_ka(host, port) {
            LAST_HTTPS_KA_REUSE.fetch_add(1, Ordering::SeqCst);
            return Ok((p.tls, p.leftover, true));
        }
    }
    let stream = connect_http(host, port, opts, connect).await?;
    let _ = crate::sockopt::apply_recv_buffer(&stream, opts);
    let _ = crate::sockopt::apply_tcp_nodelay(&stream);
    let _ = crate::sockopt::apply_tcp_quickack(&stream);
    let tls = https_handshake(stream, host, opts).await?;
    Ok((tls, SocketLeftover::new(), false))
}

async fn https_handshake(
    stream: TcpStream,
    host: &str,
    opts: &OptionSet,
) -> Result<HttpsTls> {
    let cfg = crate::tls::client_config(opts)?;
    let name = rustls::pki_types::ServerName::try_from(host)
        .map_err(|_| Error::Http("https SNI".into()))?
        .to_owned();
    tokio_rustls::TlsConnector::from(cfg)
        .connect(name, stream)
        .await
        .map_err(|e| Error::Http(format!("https handshake: {e}")))
}

/// C++ HttpProxyRequestCommand CONNECT + SocketCore TLS GET dest-match.
async fn https_connect_fetch(
    uri: &str,
    store: &FileStorage,
    start: u64,
    end: Option<u64>,
    progress: &HttpProgress,
    cancel: &mut watch::Receiver<bool>,
    opts: &OptionSet,
) -> Result<()> {
    let url = url::Url::parse(uri).map_err(|e| Error::Http(e.to_string()))?;
    let host = url
        .host_str()
        .ok_or_else(|| Error::Http("https host".into()))?
        .to_string();
    let port = url.port_or_known_default().unwrap_or(443);
    let raw = opts
        .get("https-proxy")
        .or_else(|| opts.get("all-proxy"))
        .or_else(|| opts.get("http-proxy"))
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::Http("no https-proxy".into()))?;
    let (phost, pport, mut user, mut pass) = parse_proxy_endpoint(raw)?;
    if user.is_none() {
        user = opts
            .get("https-proxy-user")
            .or_else(|| opts.get("all-proxy-user"))
            .or_else(|| opts.get("http-proxy-user"))
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        pass = opts
            .get("https-proxy-passwd")
            .or_else(|| opts.get("all-proxy-passwd"))
            .or_else(|| opts.get("http-proxy-passwd"))
            .unwrap_or("")
            .to_string();
    }
    let timeout = Duration::from_secs(opts.u64("connect-timeout", 60).max(1));
    let stream = http_connect_tunnel(
        &phost,
        pport,
        &host,
        port,
        user.as_deref(),
        &pass,
        timeout,
    )
    .await?;
    LAST_HTTPS_CONNECT.fetch_add(1, Ordering::SeqCst);
    let _ = crate::sockopt::apply_recv_buffer(&stream, opts);
    https_on_stream(stream, &url, store, start, end, progress, cancel, opts).await
}

async fn https_on_stream(
    stream: TcpStream,
    url: &url::Url,
    store: &FileStorage,
    start: u64,
    end: Option<u64>,
    progress: &HttpProgress,
    cancel: &mut watch::Receiver<bool>,
    opts: &OptionSet,
) -> Result<()> {
    let mut uri = url.to_string();
    let mut redirects = 0u32;
    let host = url
        .host_str()
        .ok_or_else(|| Error::Http("https host".into()))?
        .to_string();
    let mut tls = https_handshake(stream, &host, opts).await?;
    let mut leftover = SocketLeftover::new();
    loop {
        let cur = url::Url::parse(&uri).map_err(|e| Error::Http(e.to_string()))?;
        match https_roundtrip(
            &mut tls,
            &mut leftover,
            &cur,
            store,
            start,
            end,
            progress,
            cancel,
            opts,
        )
        .await?
        {
            HttpsOnce::Done(_) => return Ok(()),
            HttpsOnce::Redirect(next, _) => {
                if redirects >= MAX_REDIRECT {
                    return Err(Error::Http(format!(
                        "too many redirects: count={redirects}"
                    )));
                }
                redirects += 1;
                uri = next;
            }
        }
    }
}

enum HttpsOnce {
    Done(bool),
    Redirect(String, bool),
}

async fn skip_tls_body(
    tls: &mut HttpsTls,
    leftover: &mut SocketLeftover,
    mut left: u64,
) -> Result<()> {
    let mut tmp = [0u8; 16 * 1024];
    while left > 0 {
        if leftover.is_empty() {
            let cap = left.min(tmp.len() as u64) as usize;
            let n = tls
                .read(&mut tmp[..cap])
                .await
                .map_err(|e| Error::Http(format!("https recv: {e}")))?;
            LAST_HTTPS_RECV.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                break;
            }
            leftover.extend(&tmp[..n]);
        }
        let n = leftover.len().min(left as usize);
        leftover.consume(n);
        left -= n as u64;
    }
    Ok(())
}

async fn https_roundtrip(
    tls: &mut HttpsTls,
    leftover: &mut SocketLeftover,
    url: &url::Url,
    store: &FileStorage,
    start: u64,
    end: Option<u64>,
    progress: &HttpProgress,
    cancel: &mut watch::Receiver<bool>,
    opts: &OptionSet,
) -> Result<HttpsOnce> {
    let host = url.host_str().unwrap_or("");
    let port = url.port_or_known_default().unwrap_or(443);
    let path = match url.query() {
        Some(q) => format!("{}?{q}", url.path()),
        None => {
            let p = url.path();
            if p.is_empty() {
                "/".to_string()
            } else {
                p.to_string()
            }
        }
    };
    let host_hdr = if url.port().is_some() {
        format!("{host}:{port}")
    } else {
        host.to_string()
    };
    let ua = opts.get("user-agent").unwrap_or(crate::USER_AGENT);
    let ka = opts.bool("enable-http-keep-alive", true);
    let mut req = format!(
        "GET {path} HTTP/1.1\r\nHost: {host_hdr}\r\nUser-Agent: {ua}\r\nAccept: */*\r\nConnection: {}\r\n",
        if ka { "keep-alive" } else { "close" }
    );
    if let Some(end) = end {
        req.push_str(&format!("Range: bytes={start}-{end}\r\n"));
    } else if start > 0 {
        req.push_str(&format!("Range: bytes={start}-\r\n"));
    }
    if opts.bool("http-no-cache", true) {
        req.push_str("Cache-Control: no-cache\r\nPragma: no-cache\r\n");
    }
    req.push_str("\r\n");
    tls.write_all(req.as_bytes())
        .await
        .map_err(|e| Error::Http(format!("https send: {e}")))?;
    LAST_HTTPS_SEND.fetch_add(1, Ordering::SeqCst);
    let meta = read_tls_headers(tls, leftover).await?;
    if is_http_redirect(meta.status) {
        skip_tls_body(tls, leftover, meta.clen).await?;
        let loc = meta.location.as_deref().unwrap_or("");
        let next = resolve_redirect(url, loc)?;
        return Ok(HttpsOnce::Redirect(next, meta.connection_close));
    }
    if meta.status != 206 && meta.status != 200 {
        return Err(Error::Http(format!("status {}", meta.status)));
    }
    if start > 0 && meta.status == 200 {
        return Err(Error::Http("resume not possible".into()));
    }
    let want = match end {
        Some(e) => e.saturating_sub(start).saturating_add(1),
        None => meta.clen,
    };
    let mut left = if meta.clen > 0 { meta.clen } else { want };
    if left == 0 {
        store.flush().await?;
        return Ok(HttpsOnce::Done(meta.connection_close));
    }
    let mut off = start;
    let mut rate = RateCtl::new(opts);
    let mut tmp = vec![0u8; 128 * 1024];
    while left > 0 {
        if progress.halt.load(Ordering::Relaxed) || *cancel.borrow() {
            return Ok(HttpsOnce::Done(true));
        }
        if leftover.is_empty() {
            let nread = tls
                .read(&mut tmp)
                .await
                .map_err(|e| Error::Http(format!("https recv: {e}")))?;
            if nread == 0 {
                break;
            }
            LAST_HTTPS_RECV.fetch_add(1, Ordering::SeqCst);
            let take = (nread as u64).min(left) as usize;
            recv_window_write(store, off, &tmp[..take]).await?;
            if take < nread {
                leftover.extend(&tmp[take..nread]);
            }
            off += take as u64;
            left -= take as u64;
            progress
                .completed
                .fetch_add(take as u64, Ordering::Relaxed);
            rate.after(take as u64).await?;
            progress.overall.after(take as u64).await;
            continue;
        }
        let take = leftover.len().min(left as usize);
        leftover_write(store, leftover, off, take).await?;
        off += take as u64;
        left -= take as u64;
        progress
            .completed
            .fetch_add(take as u64, Ordering::Relaxed);
        rate.after(take as u64).await?;
        progress.overall.after(take as u64).await;
    }
    if end.is_none() && off > start {
        progress.total.store(off, Ordering::Relaxed);
    }
    store.flush().await?;
    Ok(HttpsOnce::Done(meta.connection_close))
}

async fn read_tls_headers(
    tls: &mut HttpsTls,
    leftover: &mut SocketLeftover,
) -> Result<Http1Meta> {
    loop {
        if let Some(pos) = leftover.available().windows(4).position(|w| w == b"\r\n\r\n") {
            let text = String::from_utf8_lossy(&leftover.available()[..pos + 4]).into_owned();
            leftover.consume(pos + 4);
            return Ok(parse_http1_meta(&text));
        }
        if leftover.len() > 64 * 1024 {
            return Err(Error::Http("headers too large".into()));
        }
        let mut tmp = [0u8; 512];
        let n = tls
            .read(&mut tmp)
            .await
            .map_err(|e| Error::Http(format!("https recv: {e}")))?;
        if n == 0 {
            return Err(Error::Http("eof before headers".into()));
        }
        LAST_HTTPS_RECV.fetch_add(1, Ordering::SeqCst);
        leftover.extend(&tmp[..n]);
    }
}

fn parse_proxy_endpoint(raw: &str) -> Result<(String, u16, Option<String>, String)> {
    let url = if raw.contains("://") {
        raw.to_string()
    } else {
        format!("http://{raw}")
    };
    let u = url::Url::parse(&url).map_err(|e| Error::Http(format!("proxy-method: {e}")))?;
    let host = u
        .host_str()
        .ok_or_else(|| Error::Http("proxy host".into()))?
        .to_string();
    let port = u.port_or_known_default().unwrap_or(80);
    let user = if u.username().is_empty() {
        None
    } else {
        Some(u.username().to_string())
    };
    let pass = u.password().unwrap_or("").to_string();
    Ok((host, port, user, pass))
}

async fn http_connect_tunnel(
    proxy_host: &str,
    proxy_port: u16,
    target_host: &str,
    target_port: u16,
    user: Option<&str>,
    pass: &str,
    timeout: Duration,
) -> Result<TcpStream> {
    let addr = tokio::net::lookup_host((proxy_host, proxy_port))
        .await?
        .next()
        .ok_or_else(|| Error::Http("proxy dns".into()))?;
    let mut s = tokio::time::timeout(timeout, TcpStream::connect(addr))
        .await
        .map_err(|_| Error::Http("proxy connect-timeout".into()))??;
    let _ = crate::sockopt::apply_tcp_nodelay(&s);
    let _ = crate::sockopt::apply_tcp_quickack(&s);
    let mut req = format!(
        "CONNECT {target_host}:{target_port} HTTP/1.1\r\nHost: {target_host}:{target_port}\r\n"
    );
    if let Some(u) = user.filter(|s| !s.is_empty()) {
        let token = crate::bt::b64_encode(format!("{u}:{pass}").as_bytes());
        req.push_str(&format!("Proxy-Authorization: Basic {token}\r\n"));
    }
    req.push_str("Proxy-Connection: keep-alive\r\n\r\n");
    crate::sockopt::send_all(&s, req.as_bytes()).await?;
    let mut leftover = SocketLeftover::new();
    let (status, _) = read_http1_headers(&mut s, &mut leftover).await?;
    if status != 200 {
        return Err(Error::Http(format!("proxy CONNECT {status}")));
    }
    Ok(s)
}

async fn tunnel_fetch(
    uri: &str,
    store: &FileStorage,
    start: u64,
    end: Option<u64>,
    progress: &HttpProgress,
    cancel: &mut watch::Receiver<bool>,
    opts: &OptionSet,
) -> Result<()> {
    record_proxy_method(opts);
    let url = url::Url::parse(uri).map_err(|e| Error::Http(e.to_string()))?;
    let host = url
        .host_str()
        .ok_or_else(|| Error::Http("no host".into()))?
        .to_string();
    let port = url.port_or_known_default().unwrap_or(80);
    let path = match url.query() {
        Some(q) => format!("{}?{q}", url.path()),
        None => {
            if url.path().is_empty() {
                "/".into()
            } else {
                url.path().to_string()
            }
        }
    };
    let raw = http_proxy_raw(opts).ok_or_else(|| Error::Http("no proxy".into()))?;
    let (phost, pport, mut user, mut pass) = parse_proxy_endpoint(raw)?;
    if user.is_none() {
        user = opts
            .get("http-proxy-user")
            .or_else(|| opts.get("all-proxy-user"))
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        pass = opts
            .get("http-proxy-passwd")
            .or_else(|| opts.get("all-proxy-passwd"))
            .unwrap_or("")
            .to_string();
    }
    let timeout = Duration::from_secs(opts.u64("connect-timeout", 60).max(1));
    let mut s = http_connect_tunnel(
        &phost,
        pport,
        &host,
        port,
        user.as_deref(),
        &pass,
        timeout,
    )
    .await?;
    let mut req = format!("GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\n");
    req.push_str(&format!(
        "User-Agent: {}\r\n",
        opts.get("user-agent").unwrap_or(crate::USER_AGENT)
    ));
    if let Some(end) = end {
        req.push_str(&format!("Range: bytes={start}-{end}\r\n"));
    }
    req.push_str("Connection: close\r\n\r\n");
    crate::sockopt::send_all(&s, req.as_bytes()).await?;
    let mut leftover = SocketLeftover::new();
    let (status, clen) = read_http1_headers(&mut s, &mut leftover).await?;
    if status != 200 && status != 206 {
        return Err(Error::Http(format!("tunnel status {status}")));
    }
    if start > 0 && status == 200 {
        return Err(Error::Http("resume not possible".into()));
    }
    let mut remain = if clen > 0 { Some(clen) } else { None };
    let mut off = start;
    let mut tmp = vec![0u8; 128 * 1024];
    loop {
        if *cancel.borrow() {
            return Err(Error::Http("canceled".into()));
        }
        if leftover.is_empty() {
            let cap = remain.unwrap_or(tmp.len() as u64).min(tmp.len() as u64) as usize;
            if cap == 0 {
                break;
            }
            let n = crate::sockopt::recv_some(&s, &mut tmp[..cap]).await?;
            if n == 0 {
                break;
            }
            let take = remain.map(|r| (r as usize).min(n)).unwrap_or(n);
            if take == 0 {
                leftover.extend(&tmp[..n]);
                break;
            }
            recv_window_write(store, off, &tmp[..take]).await?;
            if take < n {
                leftover.extend(&tmp[take..n]);
            }
            off += take as u64;
            progress.completed.fetch_add(take as u64, Ordering::Relaxed);
            progress.overall.after(take as u64).await;
            if let Some(r) = remain.as_mut() {
                *r = r.saturating_sub(take as u64);
                if *r == 0 {
                    break;
                }
            }
            continue;
        }
        let take = remain
            .map(|r| (r as usize).min(leftover.len()))
            .unwrap_or(leftover.len());
        if take == 0 {
            break;
        }
        leftover_write(store, &mut leftover, off, take).await?;
        off += take as u64;
        progress.completed.fetch_add(take as u64, Ordering::Relaxed);
        progress.overall.after(take as u64).await;
        if let Some(r) = remain.as_mut() {
            *r = r.saturating_sub(take as u64);
            if *r == 0 {
                break;
            }
        }
    }
    store.flush().await?;
    Ok(())
}

type CookieJar = Arc<Mutex<Vec<Cookie>>>;

fn apply_cookies(
    req: reqwest::RequestBuilder,
    jar: &CookieJar,
    uri: &str,
) -> reqwest::RequestBuilder {
    let g = jar.lock().unwrap();
    if let Some(h) = crate::cookies::header_for(&g, uri) {
        req.header(header::COOKIE, h)
    } else {
        req
    }
}

fn basic_auth_value(opts: &OptionSet) -> Option<String> {
    let user = opts.get("http-user").filter(|s| !s.is_empty())?;
    let pass = opts.get("http-passwd").unwrap_or("");
    let token = crate::bt::b64_encode(format!("{user}:{pass}").as_bytes());
    Some(format!("Basic {token}"))
}

fn auth_header(opts: &OptionSet, uri: &str) -> Option<String> {
    if let Some(v) = basic_auth_value(opts) {
        return Some(v);
    }
    let (user, pass) = crate::netrc::credentials_for(opts, uri)?;
    let token = crate::bt::b64_encode(format!("{user}:{pass}").as_bytes());
    Some(format!("Basic {token}"))
}

async fn send_get(
    client: &Client,
    uri: &str,
    jar: &CookieJar,
    opts: &OptionSet,
    range: Option<&str>,
    head: bool,
) -> Result<reqwest::Response> {
    let build = || {
        let mut b = if head {
            client.head(uri)
        } else {
            client.get(uri)
        };
        b = apply_cookies(b, jar, uri);
        if let Some(r) = range {
            b = b.header(header::RANGE, r);
        }
        if !opts.bool("http-auth-challenge", false) {
            if let Some(a) = auth_header(opts, uri) {
                b = b.header(header::AUTHORIZATION, a);
            }
        }
        b
    };
    let r = build().send().await?;
    if r.status() == reqwest::StatusCode::UNAUTHORIZED && opts.bool("http-auth-challenge", false) {
        if let Some(auth) = auth_header(opts, uri) {
            let _ = r.bytes().await;
            return Ok(build().header(header::AUTHORIZATION, auth).send().await?);
        }
    }
    Ok(r)
}

fn absorb_set_cookie(jar: &CookieJar, headers: &HeaderMap, uri: &str) {
    let url = url::Url::parse(uri).ok();
    let host = url.as_ref().and_then(|u| u.host_str()).unwrap_or("");
    let path = url.as_ref().map(|u| u.path()).unwrap_or("/");
    let mut g = jar.lock().unwrap();
    for v in headers.get_all(header::SET_COOKIE) {
        let Ok(s) = v.to_str() else { continue };
        let Some(c) = crate::cookies::parse_set_cookie(s, host, path) else {
            continue;
        };
        g.retain(|x| !(x.name == c.name && x.domain == c.domain));
        g.push(c);
    }
}

fn extra_headers(opts: &OptionSet) -> Result<HeaderMap> {
    let mut h = HeaderMap::new();
    if let Some(r) = opts.get("referer").filter(|s| !s.is_empty()) {
        h.insert(
            header::REFERER,
            r.parse()
                .map_err(|_| Error::Http("referer".into()))?,
        );
    }
    if let Some(user) = opts.get("http-user").filter(|s| !s.is_empty()) {
        if !opts.bool("http-auth-challenge", false) {
            let pass = opts.get("http-passwd").unwrap_or("");
            let token = crate::bt::b64_encode(format!("{user}:{pass}").as_bytes());
            let v = format!("Basic {token}");
            h.insert(
                header::AUTHORIZATION,
                v.parse()
                    .map_err(|_| Error::Http("http-user".into()))?,
            );
        }
    }
    if let Some(raw) = opts.get("header").filter(|s| !s.is_empty()) {
        for line in raw.split('\n') {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Some((k, v)) = line.split_once(':') else {
                return Err(Error::Http("header Name: Value".into()));
            };
            let name = HeaderName::from_bytes(k.trim().as_bytes())
                .map_err(|_| Error::Http("header name".into()))?;
            let val = HeaderValue::from_str(v.trim())
                .map_err(|_| Error::Http("header value".into()))?;
            h.append(name, val);
        }
    }
    if opts.bool("http-no-cache", true) {
        h.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-cache"),
        );
        h.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    }
    if !opts.bool("no-want-digest-header", false) {
        h.insert(
            HeaderName::from_static("want-digest"),
            HeaderValue::from_static("SHA-512;q=1, SHA-256;q=1, SHA;q=0.1"),
        );
    }
    if let Some(ims) = opts.get("if-modified-since").filter(|s| !s.is_empty()) {
        h.insert(
            header::IF_MODIFIED_SINCE,
            ims.parse()
                .map_err(|_| Error::Http("if-modified-since".into()))?,
        );
    }
    Ok(h)
}

/// C++ `--parameterized-uri`: `{a,b}` sets and `[start-end:step]` numeric sequences.
pub fn expand_parameterized(uri: &str) -> Vec<String> {
    let mut cur = vec![uri.to_string()];
    for _ in 0..16 {
        let mut next = Vec::new();
        let mut changed = false;
        for u in cur {
            if let Some(v) = expand_brace_once(&u) {
                changed = true;
                next.extend(v);
            } else if let Some(v) = expand_numeric_once(&u) {
                changed = true;
                next.extend(v);
            } else {
                next.push(u);
            }
        }
        cur = next;
        if !changed {
            break;
        }
        if cur.len() > 1024 {
            break;
        }
    }
    cur
}

fn expand_brace_once(u: &str) -> Option<Vec<String>> {
    let start = u.find('{')?;
    let rel = u[start + 1..].find('}')?;
    let end = start + 1 + rel;
    let inner = &u[start + 1..end];
    if !inner.contains(',') {
        return None;
    }
    let prefix = &u[..start];
    let suffix = &u[end + 1..];
    Some(
        inner
            .split(',')
            .map(|p| format!("{prefix}{p}{suffix}"))
            .collect(),
    )
}

fn expand_numeric_once(u: &str) -> Option<Vec<String>> {
    let start = u.find('[')?;
    let rel = u[start + 1..].find(']')?;
    let end = start + 1 + rel;
    let inner = &u[start + 1..end];
    let (range, step) = match inner.split_once(':') {
        Some((r, s)) => (r, s.parse::<i64>().ok()?),
        None => (inner, 1i64),
    };
    if step == 0 {
        return None;
    }
    let (a, b) = range.split_once('-')?;
    if a.is_empty() || !a.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if b.is_empty() || !b.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let width = a.len();
    let from: i64 = a.parse().ok()?;
    let to: i64 = b.parse().ok()?;
    let prefix = &u[..start];
    let suffix = &u[end + 1..];
    let mut out = Vec::new();
    if from <= to {
        let mut n = from;
        while n <= to {
            out.push(format!("{prefix}{n:0width$}{suffix}"));
            n += step.abs();
        }
    } else {
        let mut n = from;
        while n >= to {
            out.push(format!("{prefix}{n:0width$}{suffix}"));
            n -= step.abs();
        }
    }
    if out.len() <= 1 {
        return None;
    }
    Some(out)
}

/// C++ `--auto-file-renaming` (default true) / `--allow-overwrite` (default false).
/// If dest exists and `--continue` is off: rename, overwrite, or error.
pub fn resolve_existing_dest(dest: PathBuf, opts: &OptionSet) -> Result<PathBuf> {
    if !dest.exists() || opts.bool("continue", false) {
        return Ok(dest);
    }
    if opts.bool("auto-file-renaming", true) {
        return Ok(unique_renamed(&dest));
    }
    if opts.bool("allow-overwrite", false) {
        return Ok(dest);
    }
    Err(Error::Http("file already exists".into()))
}

fn can_start_from_scratch(opts: &OptionSet, failures: u64) -> bool {
    if opts.bool("always-resume", true) {
        return false;
    }
    let n = opts.u64("max-resume-failure-tries", 0);
    n > 0 && failures >= n
}

fn unique_renamed(dest: &std::path::Path) -> PathBuf {
    let parent = dest.parent().unwrap_or_else(|| std::path::Path::new("."));
    let stem = dest
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "index".into());
    let ext = dest.extension().map(|s| s.to_string_lossy().into_owned());
    for i in 1..10_000 {
        let name = match &ext {
            Some(e) => format!("{stem}.{i}.{e}"),
            None => format!("{stem}.{i}"),
        };
        let p = parent.join(name);
        if !p.exists() {
            return p;
        }
    }
    dest.to_path_buf()
}

fn maybe_remote_time(dest: &std::path::Path, opts: &OptionSet, last_modified: Option<&str>) -> Result<()> {
    if !opts.bool("remote-time", false) {
        return Ok(());
    }
    let Some(raw) = last_modified.filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    let Some(t) = parse_http_date(raw) else {
        return Ok(());
    };
    let f = std::fs::OpenOptions::new()
        .write(true)
        .open(dest)
        .map_err(|e| Error::Http(e.to_string()))?;
    f.set_modified(t).map_err(|e| Error::Http(e.to_string()))?;
    Ok(())
}

const HTTP_MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const HTTP_WDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

pub fn fmt_http_date(t: std::time::SystemTime) -> String {
    let secs = t
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let h = rem / 3600;
    let mi = (rem % 3600) / 60;
    let s = rem % 60;
    let (y, m, d) = civil_from_days(days);
    let wday = ((days + 4).rem_euclid(7)) as usize;
    format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
        HTTP_WDAYS[wday],
        d,
        HTTP_MONTHS[(m - 1) as usize],
        y,
        h,
        mi,
        s
    )
}

pub fn parse_http_date(s: &str) -> Option<std::time::SystemTime> {
    let s = s.trim();
    let rest = s.split_once(", ").map(|x| x.1).unwrap_or(s);
    let mut it = rest.split_whitespace();
    let day: u32 = it.next()?.parse().ok()?;
    let mon = it.next()?;
    let month = HTTP_MONTHS
        .iter()
        .position(|m| m.eq_ignore_ascii_case(mon))? as u32
        + 1;
    let year: i32 = it.next()?.parse().ok()?;
    let hms = it.next()?;
    let mut t = hms.split(':');
    let h: u32 = t.next()?.parse().ok()?;
    let mi: u32 = t.next()?.parse().ok()?;
    let sec: u32 = t.next()?.parse().ok()?;
    let days = days_from_civil(year, month, day)?;
    let unix = days * 86400 + h as i64 * 3600 + mi as i64 * 60 + sec as i64;
    if unix < 0 {
        return None;
    }
    Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(unix as u64))
}

fn days_from_civil(y: i32, m: u32, d: u32) -> Option<i64> {
    if !(1..=12).contains(&m) || d == 0 || d > 31 {
        return None;
    }
    let y = y as i64;
    let m = m as i64;
    let d = d as i64;
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = (y - era * 400) as u64;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy as u64;
    Some(era * 146097 + doe as i64 - 719468)
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

fn safe_cd_name(name: &str) -> Option<String> {
    let name = name.replace('\\', "/");
    let base = name.rsplit('/').next().unwrap_or("").trim();
    if base.is_empty() || base == "." || base == ".." {
        return None;
    }
    if base.contains('\0') {
        return None;
    }
    Some(base.to_string())
}

fn percent_decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let h = |c: u8| match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                b'A'..=b'F' => Some(c - b'A' + 10),
                _ => None,
            };
            if let (Some(hi), Some(lo)) = (h(b[i + 1]), h(b[i + 2])) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

fn find_bytes_ci(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    let n0 = needle[0].to_ascii_lowercase();
    'outer: for i in 0..=hay.len() - needle.len() {
        if hay[i].to_ascii_lowercase() != n0 {
            continue;
        }
        for j in 1..needle.len() {
            if hay[i + j].to_ascii_lowercase() != needle[j].to_ascii_lowercase() {
                continue 'outer;
            }
        }
        return Some(i);
    }
    None
}

fn token_end(b: &[u8]) -> usize {
    b.iter()
        .position(|&c| c == b';' || c == b' ' || c == b'\t' || c == b'\r' || c == b'\n')
        .unwrap_or(b.len())
}

/// C++ `--content-disposition-default-utf8`: `filename=` is UTF-8 when true,
/// ISO-8859-1 when false. `filename*` (RFC 5987) always wins.
pub(crate) fn parse_content_disposition(raw: &[u8], utf8_default: bool) -> Option<String> {
    if let Some(idx) = find_bytes_ci(raw, b"filename*") {
        let mut rest = raw[idx + b"filename*".len()..].to_vec();
        while rest.first().is_some_and(|c| c.is_ascii_whitespace()) {
            rest.remove(0);
        }
        if rest.first() == Some(&b'=') {
            rest.remove(0);
        }
        while rest.first().is_some_and(|c| c.is_ascii_whitespace()) {
            rest.remove(0);
        }
        let n = token_end(&rest);
        let mut token = &rest[..n];
        if token.first() == Some(&b'"') && token.last() == Some(&b'"') && token.len() >= 2 {
            token = &token[1..token.len() - 1];
        }
        let token_s: String = token.iter().map(|&b| b as char).collect();
        let mut parts = token_s.splitn(3, '\'');
        let charset = parts.next().unwrap_or("utf-8");
        let _lang = parts.next();
        let value = parts.next().unwrap_or("");
        if value.is_empty() {
            return None;
        }
        let bytes = percent_decode(value);
        let name = if charset.eq_ignore_ascii_case("utf-8") || charset.eq_ignore_ascii_case("utf8")
        {
            String::from_utf8_lossy(&bytes).into_owned()
        } else {
            bytes.iter().map(|&b| b as char).collect()
        };
        return safe_cd_name(&name);
    }
    if let Some(idx) = find_bytes_ci(raw, b"filename") {
        let mut i = idx + b"filename".len();
        while i < raw.len() && raw[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < raw.len() && raw[i] == b'=' {
            i += 1;
        }
        while i < raw.len() && raw[i].is_ascii_whitespace() {
            i += 1;
        }
        let bytes = if i < raw.len() && raw[i] == b'"' {
            i += 1;
            let start = i;
            while i < raw.len() && raw[i] != b'"' {
                i += 1;
            }
            &raw[start..i]
        } else {
            let start = i;
            let n = token_end(&raw[start..]);
            &raw[start..start + n]
        };
        let name = if utf8_default {
            String::from_utf8_lossy(bytes).into_owned()
        } else {
            bytes.iter().map(|&b| b as char).collect()
        };
        return safe_cd_name(&name);
    }
    None
}

struct Probe {
    length: Option<u64>,
    accept_ranges: bool,
    content_disposition: Option<Vec<u8>>,
    last_modified: Option<String>,
    not_modified: bool,
}

/// C++ HttpRequestCommand probe on a keep-alive SocketCore (GET bytes=0-0 or HEAD).
async fn http_socket_probe(
    uri: &str,
    jar: &CookieJar,
    opts: &OptionSet,
    head: bool,
) -> Result<Probe> {
    let url = url::Url::parse(uri).map_err(|e| Error::Http(e.to_string()))?;
    let host = url
        .host_str()
        .ok_or_else(|| Error::Http("http host".into()))?
        .to_string();
    let port = url.port_or_known_default().unwrap_or(80);
    let connect = Duration::from_secs(opts.u64("connect-timeout", 60).max(1));
    let ka = opts.bool("enable-http-keep-alive", true);
    let mut with_auth = !opts.bool("http-auth-challenge", false);
    for attempt in 0..2u8 {
        let mut allow_reuse = ka;
        for _ in 0..2u8 {
            let (stream, mut leftover, reused) =
                http_open(&host, port, opts, connect, allow_reuse).await?;
            let req = if head {
                build_origin_get(&url, uri, 0, None, jar, opts, with_auth)
                    .replacen("GET ", "HEAD ", 1)
            } else {
                build_origin_get(&url, uri, 0, Some(0), jar, opts, with_auth)
            };
            if let Err(e) = crate::sockopt::send_all(&stream, req.as_bytes()).await {
                if reused {
                    allow_reuse = false;
                    continue;
                }
                return Err(e);
            }
            LAST_HTTP_SEND.fetch_add(1, Ordering::SeqCst);
            let meta = match read_http1_meta(&stream, &mut leftover).await {
                Ok(m) => m,
                Err(_) if reused => {
                    allow_reuse = false;
                    continue;
                }
                Err(e) => return Err(e),
            };
            LAST_HTTP_RECV.fetch_add(1, Ordering::SeqCst);
            if meta.status == 401
                && attempt == 0
                && opts.bool("http-auth-challenge", false)
                && auth_header(opts, uri).is_some()
            {
                with_auth = true;
                break;
            }
            if meta.status == 404 {
                return Err(Error::Http("404".into()));
            }
            if meta.status >= 500 {
                return Err(Error::Http(format!("status {}", meta.status)));
            }
            absorb_set_cookie_lines(jar, uri, &meta.set_cookies);
            if !head && meta.status != 304 && meta.clen > 0 {
                skip_http_body(&stream, &mut leftover, meta.clen).await?;
            }
            if ka && !meta.connection_close {
                put_ka(
                    &host,
                    port,
                    PooledHttp {
                        stream,
                        leftover,
                    },
                );
            }
            let length = if let Some(cr) = meta.content_range.as_deref() {
                cr.rsplit('/').next().and_then(|s| s.parse().ok())
            } else {
                meta.content_length
            };
            return Ok(Probe {
                length,
                accept_ranges: meta.accept_ranges,
                content_disposition: meta.content_disposition.map(|s| s.into_bytes()),
                last_modified: meta.last_modified,
                not_modified: meta.status == 304,
            });
        }
    }
    Err(Error::Http("http probe failed".into()))
}

/// C++ HttpRequestCommand TLS probe on keep-alive SocketCore (GET bytes=0-0 or HEAD).
async fn https_socket_probe(
    uri: &str,
    jar: &CookieJar,
    opts: &OptionSet,
    head: bool,
) -> Result<Probe> {
    let url = url::Url::parse(uri).map_err(|e| Error::Http(e.to_string()))?;
    let host = url
        .host_str()
        .ok_or_else(|| Error::Http("https host".into()))?
        .to_string();
    let port = url.port_or_known_default().unwrap_or(443);
    let connect = Duration::from_secs(opts.u64("connect-timeout", 60).max(1));
    let ka = opts.bool("enable-http-keep-alive", true);
    let mut with_auth = !opts.bool("http-auth-challenge", false);
    for attempt in 0..2u8 {
        let mut allow_reuse = ka;
        for _ in 0..2u8 {
            let (mut tls, mut leftover, reused) =
                https_open(&host, port, opts, connect, allow_reuse).await?;
            let req = if head {
                build_origin_get(&url, uri, 0, None, jar, opts, with_auth)
                    .replacen("GET ", "HEAD ", 1)
            } else {
                build_origin_get(&url, uri, 0, Some(0), jar, opts, with_auth)
            };
            if let Err(e) = tls
                .write_all(req.as_bytes())
                .await
                .map_err(|e| Error::Http(format!("https send: {e}")))
            {
                if reused {
                    allow_reuse = false;
                    continue;
                }
                return Err(e);
            }
            LAST_HTTPS_SEND.fetch_add(1, Ordering::SeqCst);
            let meta = match read_tls_headers(&mut tls, &mut leftover).await {
                Ok(m) => m,
                Err(_) if reused => {
                    allow_reuse = false;
                    continue;
                }
                Err(e) => return Err(e),
            };
            if meta.status == 401
                && attempt == 0
                && opts.bool("http-auth-challenge", false)
                && auth_header(opts, uri).is_some()
            {
                with_auth = true;
                break;
            }
            if meta.status == 404 {
                return Err(Error::Http("404".into()));
            }
            if meta.status >= 500 {
                return Err(Error::Http(format!("status {}", meta.status)));
            }
            absorb_set_cookie_lines(jar, uri, &meta.set_cookies);
            if !head && meta.status != 304 && meta.clen > 0 {
                skip_tls_body(&mut tls, &mut leftover, meta.clen).await?;
            }
            if ka && !meta.connection_close {
                put_https_ka(
                    &host,
                    port,
                    PooledHttps { tls, leftover },
                );
            }
            let length = if let Some(cr) = meta.content_range.as_deref() {
                cr.rsplit('/').next().and_then(|s| s.parse().ok())
            } else {
                meta.content_length
            };
            return Ok(Probe {
                length,
                accept_ranges: meta.accept_ranges,
                content_disposition: meta.content_disposition.map(|s| s.into_bytes()),
                last_modified: meta.last_modified,
                not_modified: meta.status == 304,
            });
        }
    }
    Err(Error::Http("https probe failed".into()))
}

async fn probe(client: &Client, uri: &str, jar: &CookieJar, opts: &OptionSet) -> Result<Probe> {
    if want_http_socket(uri, opts) && opts.bool("enable-http-keep-alive", true) {
        return http_socket_probe(uri, jar, opts, false).await;
    }
    if want_https_socket(uri, opts) && opts.bool("enable-http-keep-alive", true) {
        return https_socket_probe(uri, jar, opts, false).await;
    }
    let r = send_get(client, uri, jar, opts, Some("bytes=0-0"), false).await?;
    let status = r.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(Error::Http("404".into()));
    }
    if status.is_server_error() {
        return Err(Error::Http(format!("status {status}")));
    }
    let accept_header = r
        .headers()
        .get(header::ACCEPT_RANGES)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_ascii_lowercase());
    let content_range = r
        .headers()
        .get(header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let content_length = r
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok());
    let accept_ranges = accept_header
        .map(|v| v.contains("bytes"))
        .unwrap_or(false)
        || status == reqwest::StatusCode::PARTIAL_CONTENT;
    let length = if let Some(cr) = content_range.as_deref() {
        cr.rsplit('/').next().and_then(|s| s.parse().ok())
    } else {
        content_length
    };
    // Drain the 1-byte probe body so keep-alive reuse is not a half-read conn.
    absorb_set_cookie(jar, r.headers(), uri);
    let cd = r
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .map(|v| v.as_bytes().to_vec());
    let last_modified = r
        .headers()
        .get(header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let not_modified = status == reqwest::StatusCode::NOT_MODIFIED;
    let _ = r.bytes().await;
    Ok(Probe {
        length,
        accept_ranges,
        content_disposition: cd,
        last_modified,
        not_modified,
    })
}

async fn head_probe(client: &Client, uri: &str, jar: &CookieJar, opts: &OptionSet) -> Result<Probe> {
    if want_http_socket(uri, opts) && opts.bool("enable-http-keep-alive", true) {
        return http_socket_probe(uri, jar, opts, true).await;
    }
    if want_https_socket(uri, opts) && opts.bool("enable-http-keep-alive", true) {
        return https_socket_probe(uri, jar, opts, true).await;
    }
    let r = send_get(client, uri, jar, opts, None, true).await?;
    let status = r.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(Error::Http("404".into()));
    }
    if status.is_server_error() {
        return Err(Error::Http(format!("status {status}")));
    }
    let not_modified = status == reqwest::StatusCode::NOT_MODIFIED;
    if !status.is_success() && !not_modified {
        return Err(Error::Http(format!("HEAD status {status}")));
    }
    let accept_header = r
        .headers()
        .get(header::ACCEPT_RANGES)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_ascii_lowercase());
    let content_length = r
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok());
    let accept_ranges = accept_header
        .map(|v| v.contains("bytes"))
        .unwrap_or(false);
    absorb_set_cookie(jar, r.headers(), uri);
    let cd = r
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .map(|v| v.as_bytes().to_vec());
    let last_modified = r
        .headers()
        .get(header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let _ = r.bytes().await;
    Ok(Probe {
        length: content_length,
        accept_ranges,
        content_disposition: cd,
        last_modified,
        not_modified,
    })
}

async fn write_stream<S>(
    stream: S,
    store: &FileStorage,
    offset: u64,
    progress: &HttpProgress,
    cancel: &mut watch::Receiver<bool>,
    update_total: bool,
    opts: &OptionSet,
) -> Result<()>
where
    S: futures::Stream<Item = reqwest::Result<Bytes>> + Unpin,
{
    let r = write_stream_inner(stream, store, offset, progress, cancel, update_total, opts).await;
    if progress.halt.load(Ordering::Relaxed) && r.is_err() {
        store.discard().await;
    } else {
        store.flush().await?;
    }
    r
}

/// C++ DefaultDiskWriter: pwrite dest fd without awaiting when already open.
async fn put_chunk(store: &FileStorage, offset: u64, chunk: &[u8]) -> Result<()> {
    store.write_body(offset, chunk).await
}

async fn write_stream_inner<S>(
    mut stream: S,
    store: &FileStorage,
    mut offset: u64,
    progress: &HttpProgress,
    cancel: &mut watch::Receiver<bool>,
    update_total: bool,
    opts: &OptionSet,
) -> Result<()>
where
    S: futures::Stream<Item = reqwest::Result<Bytes>> + Unpin,
{
    let mut rate = RateCtl::new(opts);
    loop {
        if *cancel.borrow() {
            return Err(Error::Http("canceled".into()));
        }
        tokio::select! {
            biased;
            result = cancel.changed() => {
                match result {
                    Ok(()) if *cancel.borrow() => {
                        return Err(Error::Http("canceled".into()));
                    }
                    Ok(()) => continue,
                    Err(_) => {
                        while let Some(chunk) = stream.next().await {
                            let chunk: Bytes = chunk?;
                            put_chunk(store, offset, &chunk).await?;
                            offset += chunk.len() as u64;
                            progress.completed.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                            if update_total {
                                progress.total.store(offset, Ordering::Relaxed);
                            }
                            rate.after(chunk.len() as u64).await?;
                            progress.overall.after(chunk.len() as u64).await;
                        }
                        return Ok(());
                    }
                }
            }
            chunk = stream.next() => {
                match chunk {
                    Some(Ok(chunk)) => {
                        let chunk: Bytes = chunk;
                        put_chunk(store, offset, &chunk).await?;
                        offset += chunk.len() as u64;
                        progress.completed.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                        if update_total {
                            progress.total.store(offset, Ordering::Relaxed);
                        }
                        rate.after(chunk.len() as u64).await?;
                        progress.overall.after(chunk.len() as u64).await;
                    }
                    Some(Err(e)) => return Err(e.into()),
                    None => return Ok(()),
                }
            }
        }
    }
}

struct RateCtl {
    limit: u64,
    lowest: u64,
    start: std::time::Instant,
    window: std::time::Instant,
    window_bytes: u64,
    written: u64,
}

impl RateCtl {
    fn new(opts: &OptionSet) -> Self {
        Self {
            limit: parse_speed(opts.get("max-download-limit").unwrap_or("0")).unwrap_or(0),
            lowest: parse_speed(opts.get("lowest-speed-limit").unwrap_or("0")).unwrap_or(0),
            start: std::time::Instant::now(),
            window: std::time::Instant::now(),
            window_bytes: 0,
            written: 0,
        }
    }

    async fn after(&mut self, n: u64) -> Result<()> {
        if self.limit == 0 && self.lowest == 0 {
            return Ok(());
        }
        self.written += n;
        self.window_bytes += n;
        if self.lowest > 0 {
            let elapsed = self.window.elapsed();
            if elapsed >= Duration::from_secs(1) {
                let bps = self.window_bytes.saturating_mul(1000)
                    / elapsed.as_millis().max(1) as u64;
                if bps < self.lowest {
                    return Err(Error::Http("lowest-speed-limit".into()));
                }
                self.window = std::time::Instant::now();
                self.window_bytes = 0;
            }
        }
        if self.limit > 0 {
            let expected_ms = self.written.saturating_mul(1000) / self.limit.max(1);
            let elapsed_ms = self.start.elapsed().as_millis() as u64;
            if elapsed_ms < expected_ms {
                tokio::time::sleep(Duration::from_millis(expected_ms - elapsed_ms)).await;
            }
        }
        Ok(())
    }
}

pub fn parse_speed(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (num, mul) = match s.as_bytes().last() {
        Some(b'K' | b'k') => (&s[..s.len() - 1], 1024u64),
        Some(b'M' | b'm') => (&s[..s.len() - 1], 1024 * 1024),
        Some(b'G' | b'g') => (&s[..s.len() - 1], 1024 * 1024 * 1024),
        _ => (s, 1u64),
    };
    num.trim().parse::<u64>().ok().map(|n| n.saturating_mul(mul))
}

async fn fetch_range(
    client: &Client,
    uri: &str,
    store: &FileStorage,
    start: u64,
    end: u64,
    progress: &HttpProgress,
    cancel: &mut watch::Receiver<bool>,
    jar: &CookieJar,
    opts: &OptionSet,
) -> Result<()> {
    if want_https_socket(uri, opts) {
        return https_fetch(uri, store, start, Some(end), progress, cancel, opts).await;
    }
    if want_https_connect(uri, opts) {
        return https_connect_fetch(uri, store, start, Some(end), progress, cancel, opts).await;
    }
    if want_http_tunnel(uri, opts) {
        return tunnel_fetch(uri, store, start, Some(end), progress, cancel, opts).await;
    }
    if want_http_socket(uri, opts) {
        return http_fetch(uri, store, start, Some(end), progress, cancel, jar, opts).await;
    }
    let r = send_get(
        client,
        uri,
        jar,
        opts,
        Some(&format!("bytes={start}-{end}")),
        false,
    )
    .await?;
    if !r.status().is_success() && r.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(Error::Http(format!("status {}", r.status())));
    }
    if start > 0 && r.status() == reqwest::StatusCode::OK {
        return Err(Error::Http("resume not possible".into()));
    }
    absorb_set_cookie(jar, r.headers(), uri);
    write_stream(r.bytes_stream(), store, start, progress, cancel, false, opts).await
}

async fn fetch_whole(
    client: &Client,
    uri: &str,
    store: &FileStorage,
    progress: &HttpProgress,
    cancel: &mut watch::Receiver<bool>,
    jar: &CookieJar,
    opts: &OptionSet,
) -> Result<()> {
    if want_https_socket(uri, opts) {
        return https_fetch(uri, store, 0, None, progress, cancel, opts).await;
    }
    if want_https_connect(uri, opts) {
        return https_connect_fetch(uri, store, 0, None, progress, cancel, opts).await;
    }
    if want_http_tunnel(uri, opts) {
        return tunnel_fetch(uri, store, 0, None, progress, cancel, opts).await;
    }
    if want_http_socket(uri, opts) {
        return http_fetch(uri, store, 0, None, progress, cancel, jar, opts).await;
    }
    let r = send_get(client, uri, jar, opts, None, false).await?;
    if !r.status().is_success() {
        return Err(Error::Http(format!("status {}", r.status())));
    }
    absorb_set_cookie(jar, r.headers(), uri);
    write_stream(r.bytes_stream(), store, 0, progress, cancel, true, opts).await
}

static LAST_PIPELINED: AtomicU64 = AtomicU64::new(0);
static LAST_PIPE_WRITES: AtomicU64 = AtomicU64::new(0);
static LAST_HTTPS_SEND: AtomicU64 = AtomicU64::new(0);
static LAST_HTTPS_RECV: AtomicU64 = AtomicU64::new(0);
static LAST_HTTPS_CONNECT: AtomicU64 = AtomicU64::new(0);
static LAST_HTTP_SEND: AtomicU64 = AtomicU64::new(0);
static LAST_HTTP_RECV: AtomicU64 = AtomicU64::new(0);
/// C++ SocketBuffer leftover pwrite/cache from buffer without extra Vec.
static LAST_LEFTOVER_INLINE: AtomicU64 = AtomicU64::new(0);
/// C++ SocketBuffer recv window: pwrite/cache socket tmp without leftover.extend.
static LAST_RECV_INLINE: AtomicU64 = AtomicU64::new(0);

pub fn last_pipelined() -> u64 {
    LAST_PIPELINED.load(Ordering::SeqCst)
}

/// C++ SocketBuffer::writeBuffer: one send of the pipelined GET batch.
pub fn last_pipe_writes() -> u64 {
    LAST_PIPE_WRITES.load(Ordering::SeqCst)
}

/// C++ SocketCore TLS writeData of HTTPS GET.
pub fn last_https_send() -> u64 {
    LAST_HTTPS_SEND.load(Ordering::SeqCst)
}

/// C++ SocketCore TLS readData of HTTPS body.
pub fn last_https_recv() -> u64 {
    LAST_HTTPS_RECV.load(Ordering::SeqCst)
}

/// C++ HttpProxyRequestCommand CONNECT through https-proxy.
pub fn last_https_connect() -> u64 {
    LAST_HTTPS_CONNECT.load(Ordering::SeqCst)
}

/// C++ HttpKeepAliveConnection TLS: SocketCore reused for a later HttpRequest.
pub fn last_https_ka_reuse() -> u64 {
    LAST_HTTPS_KA_REUSE.load(Ordering::SeqCst)
}

/// C++ SocketCore writeData of origin HTTP GET.
pub fn last_http_send() -> u64 {
    LAST_HTTP_SEND.load(Ordering::SeqCst)
}

/// C++ SocketCore readData of origin HTTP body.
pub fn last_http_recv() -> u64 {
    LAST_HTTP_RECV.load(Ordering::SeqCst)
}

/// C++ SocketBuffer leftover: pwrite/cache from buffer (no per-chunk Vec).
pub fn last_leftover_inline() -> u64 {
    LAST_LEFTOVER_INLINE.load(Ordering::SeqCst)
}

/// C++ SocketBuffer recv window: pwrite/cache socket tmp (no leftover.extend).
pub fn last_recv_inline() -> u64 {
    LAST_RECV_INLINE.load(Ordering::SeqCst)
}

/// C++ HttpKeepAliveConnection: SocketCore reused for a later HttpRequest.
pub fn last_http_ka_reuse() -> u64 {
    LAST_HTTP_KA_REUSE.load(Ordering::SeqCst)
}

pub fn reset_http_io() {
    LAST_HTTP_SEND.store(0, Ordering::SeqCst);
    LAST_HTTP_RECV.store(0, Ordering::SeqCst);
    LAST_HTTP_KA_REUSE.store(0, Ordering::SeqCst);
    LAST_LEFTOVER_INLINE.store(0, Ordering::SeqCst);
    LAST_RECV_INLINE.store(0, Ordering::SeqCst);
    LAST_HE_WIN.store(0, Ordering::SeqCst);
    LAST_HE_V6_TRY.store(0, Ordering::SeqCst);
    LAST_HE_V4_TRY.store(0, Ordering::SeqCst);
}

pub fn reset_https_io() {
    LAST_HTTPS_SEND.store(0, Ordering::SeqCst);
    LAST_HTTPS_RECV.store(0, Ordering::SeqCst);
    LAST_HTTPS_CONNECT.store(0, Ordering::SeqCst);
    LAST_HTTPS_KA_REUSE.store(0, Ordering::SeqCst);
    LAST_LEFTOVER_INLINE.store(0, Ordering::SeqCst);
    LAST_RECV_INLINE.store(0, Ordering::SeqCst);
}

/// C++ SocketBuffer: consume by offset; compact dead prefix rarely (no per-chunk drain).
struct SocketLeftover {
    buf: Vec<u8>,
    off: usize,
}

impl SocketLeftover {
    fn new() -> Self {
        Self {
            buf: Vec::new(),
            off: 0,
        }
    }

    fn available(&self) -> &[u8] {
        if self.off >= self.buf.len() {
            &[]
        } else {
            &self.buf[self.off..]
        }
    }

    fn is_empty(&self) -> bool {
        self.off >= self.buf.len()
    }

    fn len(&self) -> usize {
        self.buf.len().saturating_sub(self.off)
    }

    fn consume(&mut self, n: usize) {
        self.off = (self.off + n).min(self.buf.len());
        if self.off >= self.buf.len() {
            self.buf.clear();
            self.off = 0;
        } else if self.off >= 16 * 1024 {
            self.buf.copy_within(self.off.., 0);
            let live = self.buf.len() - self.off;
            self.buf.truncate(live);
            self.off = 0;
        }
    }

    fn extend(&mut self, data: &[u8]) {
        if self.is_empty() {
            self.buf.clear();
            self.off = 0;
        }
        self.buf.extend_from_slice(data);
    }
}

/// C++ `--enable-http-pipelining` + `--max-http-pipelining` (default 2, 1..=8).
async fn pipeline_http1(
    uri: &str,
    store: &FileStorage,
    pieces: &[(u32, u64, u64)],
    progress: &HttpProgress,
    cancel: &mut watch::Receiver<bool>,
    opts: &OptionSet,
    ctl: &Arc<Mutex<crate::control_file::Control>>,
) -> Result<()> {
    let url = url::Url::parse(uri).map_err(|e| Error::Http(e.to_string()))?;
    if url.scheme() != "http" {
        return Err(Error::Http("pipelining is HTTP/1.1 only".into()));
    }
    let host = url
        .host_str()
        .ok_or_else(|| Error::Http("no host".into()))?
        .to_string();
    let port = url.port_or_known_default().unwrap_or(80);
    let path = match url.query() {
        Some(q) => format!("{}?{q}", url.path()),
        None => url.path().to_string(),
    };
    let ua = opts.get("user-agent").unwrap_or(crate::USER_AGENT).to_string();
    let n = opts.usize("max-http-pipelining", 2).clamp(1, 8);
    let connect = Duration::from_secs(opts.u64("connect-timeout", 60).max(1));
    let mut stream = tokio::time::timeout(connect, TcpStream::connect((host.as_str(), port)))
        .await
        .map_err(|_| Error::Http("connect timeout".into()))?
        .map_err(|e| Error::Http(e.to_string()))?;
    let _ = crate::sockopt::apply_recv_buffer(&stream, opts);
    let _ = crate::sockopt::apply_tcp_nodelay(&stream);
    let _ = crate::sockopt::apply_tcp_quickack(&stream);
    let mut leftover = SocketLeftover::new();
    let mut rate = RateCtl::new(opts);
    let host_hdr = if url.port().is_some() {
        format!("{host}:{port}")
    } else {
        host.clone()
    };
    for batch in pieces.chunks(n) {
        if progress.halt.load(Ordering::Relaxed) || *cancel.borrow() {
            return Ok(());
        }
        // C++ SocketBuffer::send: writev of N pipelined Range GETs (no concat Vec).
        let reqs: Vec<String> = batch
            .iter()
            .map(|(_, start, end)| {
                let mut req = format!(
                    "GET {path} HTTP/1.1\r\nHost: {host_hdr}\r\nUser-Agent: {ua}\r\nRange: bytes={start}-{end}\r\nConnection: keep-alive\r\nAccept: */*\r\n"
                );
                if opts.bool("http-no-cache", true) {
                    req.push_str("Cache-Control: no-cache\r\nPragma: no-cache\r\n");
                }
                req.push_str("\r\n");
                req
            })
            .collect();
        let iov: Vec<&[u8]> = reqs.iter().map(|s| s.as_bytes()).collect();
        crate::sockopt::writev_all(&stream, &iov).await?;
        LAST_PIPELINED.store(batch.len() as u64, Ordering::SeqCst);
        LAST_PIPE_WRITES.store(1, Ordering::SeqCst);
        let mut tmp = vec![0u8; 128 * 1024];
        for (idx, start, end) in batch {
            if progress.halt.load(Ordering::Relaxed) || *cancel.borrow() {
                return Ok(());
            }
            let want = end.saturating_sub(*start).saturating_add(1);
            let (status, clen) = read_http1_headers(&mut stream, &mut leftover).await?;
            if status != 206 && status != 200 {
                return Err(Error::Http(format!("status {status}")));
            }
            if *start > 0 && status == 200 {
                return Err(Error::Http("resume not possible".into()));
            }
            let mut left = if clen > 0 { clen } else { want };
            let mut off = *start;
            // C++ SocketBuffer: pwrite the leftover/socket window; consume by offset (no drain).
            while left > 0 {
                if leftover.is_empty() {
                    let nread = crate::sockopt::recv_some(&stream, &mut tmp).await?;
                    if nread == 0 {
                        return Err(Error::Http("pipeline eof".into()));
                    }
                    let take = (nread as u64).min(left) as usize;
                    recv_window_write(store, off, &tmp[..take]).await?;
                    if take < nread {
                        leftover.extend(&tmp[take..nread]);
                    }
                    off += take as u64;
                    left -= take as u64;
                    progress
                        .completed
                        .fetch_add(take as u64, Ordering::Relaxed);
                    rate.after(take as u64).await?;
                    progress.overall.after(take as u64).await;
                    continue;
                }
                let take = leftover.len().min(left as usize);
                leftover_write(store, &mut leftover, off, take).await?;
                off += take as u64;
                left -= take as u64;
                progress
                    .completed
                    .fetch_add(take as u64, Ordering::Relaxed);
                rate.after(take as u64).await?;
                progress.overall.after(take as u64).await;
            }
            if let Ok(mut g) = ctl.lock() {
                g.set(*idx);
            }
        }
    }
    store.flush().await?;
    Ok(())
}

struct Http1Meta {
    status: u16,
    clen: u64,
    content_length: Option<u64>,
    encoding: String,
    set_cookies: Vec<String>,
    accept_ranges: bool,
    content_range: Option<String>,
    content_disposition: Option<String>,
    last_modified: Option<String>,
    connection_close: bool,
    location: Option<String>,
}

fn parse_http1_meta(text: &str) -> Http1Meta {
    let status = text
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let content_length = text
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
        .and_then(|l| l.split(':').nth(1))
        .and_then(|s| s.trim().parse().ok());
    let clen = content_length.unwrap_or(0);
    let encoding = text
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("content-encoding:"))
        .and_then(|l| l.split(':').nth(1))
        .map(|s| s.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let set_cookies = text
        .lines()
        .filter(|l| l.to_ascii_lowercase().starts_with("set-cookie:"))
        .filter_map(|l| l.split_once(':').map(|(_, v)| v.trim().to_string()))
        .collect();
    let accept_ranges = text.lines().any(|l| {
        let l = l.to_ascii_lowercase();
        l.starts_with("accept-ranges:") && l.contains("bytes")
    }) || status == 206;
    let content_range = text
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("content-range:"))
        .and_then(|l| l.split_once(':').map(|(_, v)| v.trim().to_string()));
    let content_disposition = text
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("content-disposition:"))
        .and_then(|l| l.split_once(':').map(|(_, v)| v.trim().to_string()));
    let last_modified = text
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("last-modified:"))
        .and_then(|l| l.split_once(':').map(|(_, v)| v.trim().to_string()));
    let connection_close = text.lines().any(|l| {
        let l = l.to_ascii_lowercase();
        l.starts_with("connection:") && l.contains("close")
    });
    let location = text
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("location:"))
        .and_then(|l| l.split_once(':').map(|(_, v)| v.trim().to_string()));
    Http1Meta {
        status,
        clen,
        content_length,
        encoding,
        set_cookies,
        accept_ranges,
        content_range,
        content_disposition,
        last_modified,
        connection_close,
        location,
    }
}

#[test]
fn content_length_preserves_known_zero_and_unknown_length() {
    for (header, expected) in [
        ("Content-Length: 0\r\n", Some(0)),
        ("Content-Length: 17\r\n", Some(17)),
        ("", None),
        ("Content-Length: invalid\r\n", None),
    ] {
        let meta = parse_http1_meta(&format!("HTTP/1.1 200 OK\r\n{header}\r\n"));
        assert_eq!(meta.content_length, expected);
    }
}

async fn read_http1_meta(stream: &TcpStream, leftover: &mut SocketLeftover) -> Result<Http1Meta> {
    loop {
        if let Some(pos) = leftover.available().windows(4).position(|w| w == b"\r\n\r\n") {
            let text = String::from_utf8_lossy(&leftover.available()[..pos + 4]).into_owned();
            leftover.consume(pos + 4);
            return Ok(parse_http1_meta(&text));
        }
        if leftover.len() > 64 * 1024 {
            return Err(Error::Http("headers too large".into()));
        }
        let mut tmp = [0u8; 512];
        let n = crate::sockopt::recv_some(stream, &mut tmp).await?;
        if n == 0 {
            return Err(Error::Http("eof before headers".into()));
        }
        leftover.extend(&tmp[..n]);
    }
}

async fn read_http1_headers(
    stream: &mut TcpStream,
    leftover: &mut SocketLeftover,
) -> Result<(u16, u64)> {
    let m = read_http1_meta(stream, leftover).await?;
    Ok((m.status, m.clen))
}

pub async fn download(job: HttpJob) -> Result<()> {
    record_proxy_method(&job.opts);
    let opts = job.opts.clone();
    let done = Arc::clone(&job.progress.checksum_done);
    let dest = retry_download(job).await?;
    if done.load(Ordering::Relaxed) {
        return Ok(());
    }
    crate::checksum::verify_dest(&dest, &opts).await
}

fn is_retryable(e: &Error) -> bool {
    let s = e.to_string();
    s.contains("404") || s.contains("status 5")
}

async fn retry_download(job: HttpJob) -> Result<PathBuf> {
    let max_tries = job.opts.u64("max-tries", 5);
    let retry_wait = job.opts.u64("retry-wait", 0);
    let max_fnf = job.opts.u64("max-file-not-found", 0);
    let mut tries = 0u64;
    let mut fnf = 0u64;
    loop {
        tries += 1;
        match download_inner(job.clone()).await {
            Ok(dest) => return Ok(dest),
            Err(e) => {
                let not_found = e.to_string().contains("404");
                if not_found {
                    fnf += 1;
                    if max_fnf > 0 && fnf >= max_fnf {
                        return Err(e);
                    }
                }
                let more = (max_tries == 0 || tries < max_tries) && is_retryable(&e);
                if !more {
                    return Err(e);
                }
                if retry_wait > 0 {
                    tokio::time::sleep(Duration::from_secs(retry_wait)).await;
                }
            }
        }
    }
}

/// C++ `--stream-piece-selector`: `default`/`inorder` keep min-index order,
/// `random` shuffles remaining pieces, `geom` prefers later indices (man: probability
/// increases from first to last).
pub(crate) fn apply_stream_piece_selector(sel: &str, pieces: &mut Vec<(u32, u64, u64)>) {
    match sel {
        "random" => {
            use rand::seq::SliceRandom;
            pieces.shuffle(&mut rand::rng());
        }
        "geom" => geom_reorder(pieces),
        _ => {}
    }
}

/// C++ FileEntry split: `--split`/`-x` is N Range GETs (byte slices), not one GET per piece-length.
/// Non-contiguous leftover (random/geom selector) stays one GET per piece.
pub(crate) fn merge_http_ranges(
    pieces: &[(u32, u64, u64)],
    n: usize,
) -> Vec<(Vec<u32>, u64, u64)> {
    if pieces.is_empty() {
        return Vec::new();
    }
    let contiguous = pieces.windows(2).all(|w| w[0].2 + 1 == w[1].1);
    if !contiguous {
        return pieces
            .iter()
            .map(|(i, s, e)| (vec![*i], *s, *e))
            .collect();
    }
    let n = n.min(pieces.len()).max(1);
    let mut out = Vec::with_capacity(n);
    let mut i = 0usize;
    let mut left_groups = n;
    while i < pieces.len() {
        let take = (pieces.len() - i + left_groups - 1) / left_groups;
        let g = &pieces[i..i + take];
        out.push((
            g.iter().map(|x| x.0).collect(),
            g[0].1,
            g[g.len() - 1].2,
        ));
        i += take;
        left_groups -= 1;
    }
    out
}

fn geom_reorder(pieces: &mut Vec<(u32, u64, u64)>) {
    if pieces.len() <= 1 {
        return;
    }
    use rand::Rng;
    let mut rng = rand::rng();
    let mut remaining = std::mem::take(pieces);
    while !remaining.is_empty() {
        // Weight of remaining[i] is 2^i so later pieces are more likely.
        let n = remaining.len();
        let max_exp = (n - 1).min(62);
        let sum: u64 = (0..n).map(|i| 1u64 << i.min(max_exp)).sum();
        let mut r = rng.random_range(0..sum.max(1));
        let mut pick = 0usize;
        for i in 0..n {
            let w = 1u64 << i.min(max_exp);
            if r < w {
                pick = i;
                break;
            }
            r -= w;
        }
        pieces.push(remaining.remove(pick));
    }
}

async fn download_inner(mut job: HttpJob) -> Result<PathBuf> {
    let candidates = if job.opts.bool("parameterized-uri", false) {
        job.uris
            .iter()
            .flat_map(|u| expand_parameterized(u))
            .collect::<Vec<_>>()
    } else {
        job.uris.clone()
    };
    if candidates.is_empty() {
        return Err(Error::Http("no uri".into()));
    }
    record_server(&job.progress, &candidates[0]);
    let loaded = if let Some(p) = job.opts.get("load-cookies").filter(|s| !s.is_empty()) {
        crate::cookies::load_netscape(std::path::Path::new(p))?
    } else {
        Vec::new()
    };
    let jar: CookieJar = Arc::new(Mutex::new(loaded));
    let save_path = job
        .opts
        .get("save-cookies")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    struct SaveOnDrop {
        path: Option<PathBuf>,
        jar: CookieJar,
    }
    impl Drop for SaveOnDrop {
        fn drop(&mut self) {
            if let Some(p) = &self.path {
                if let Ok(g) = self.jar.lock() {
                    let _ = crate::cookies::save_netscape(p, &g);
                }
            }
        }
    }
    let _save = SaveOnDrop {
        path: save_path,
        jar: jar.clone(),
    };
    if job.opts.bool("conditional-get", false) && job.dest.exists() {
        if let Ok(st) = std::fs::metadata(&job.dest).and_then(|m| m.modified()) {
            job.opts.set("if-modified-since", fmt_http_date(st));
        }
    }
    if job.progress.overall.limit() == 0 {
        if let Some(n) = parse_speed(job.opts.get("max-overall-download-limit").unwrap_or("0")) {
            job.progress.overall.set_limit(n);
        }
    }
    let client = client(&job.opts)?;
    let mut uri = candidates[0].clone();
    let mut probed = Probe {
        length: None,
        accept_ranges: false,
        content_disposition: None,
        last_modified: None,
        not_modified: false,
    };
    let mut last_err = Error::Http("no uri".into());
    let mut found = false;
    let reuse = job.opts.bool("reuse-uri", true);
    let selector = job
        .opts
        .get("uri-selector")
        .unwrap_or("feedback")
        .to_string();
    let mut stats = crate::server_stat::load_from_opts(&job.opts);
    let timeout = crate::server_stat::timeout_secs(&job.opts);
    let least = job.opts.bool("select-least-used-host", true);
    let mut unused = crate::server_stat::rank_uris(&candidates, &selector, &stats, timeout, least);
    let mut used: Vec<String> = Vec::new();
    let mut recycled = false;
    loop {
        if unused.is_empty() {
            if reuse && !recycled && used.len() >= 2 {
                unused = crate::server_stat::rank_uris(&used, &selector, &stats, timeout, least);
                used.clear();
                recycled = true;
            } else {
                break;
            }
        }
        let orig = unused.remove(0);
        let cand = match crate::dns::rewrite(&orig, &job.opts).await {
            Ok(u) => u,
            Err(e) => {
                last_err = e;
                used.push(orig);
                continue;
            }
        };
        let r = if job.opts.bool("use-head", false) {
            head_probe(&client, &cand, &jar, &job.opts).await
        } else {
            probe(&client, &cand, &jar, &job.opts).await
        };
        let (host, proto) = crate::server_stat::host_proto(&orig);
        match r {
            Ok(p) => {
                uri = cand.clone();
                probed = p;
                found = true;
                used.push(orig);
                crate::server_stat::upsert(
                    &mut stats,
                    &host,
                    &proto,
                    crate::server_stat::StatStatus::Ok,
                    1,
                );
                crate::server_stat::persist(&job.opts, &stats);
                record_server(&job.progress, &uri);
                break;
            }
            Err(e) if is_retryable(&e) => {
                last_err = e;
                crate::server_stat::upsert(
                    &mut stats,
                    &host,
                    &proto,
                    crate::server_stat::StatStatus::Error,
                    0,
                );
                used.push(cand);
            }
            Err(e) => {
                crate::server_stat::persist(&job.opts, &stats);
                return Err(e);
            }
        }
    }
    if !found {
        crate::server_stat::persist(&job.opts, &stats);
        return Err(last_err);
    }
    let _host_guard = crate::server_stat::acquire_host(&crate::server_stat::host_proto(&uri).0);
    if probed.not_modified {
        return Ok(job.dest);
    }
    let total = probed.length.unwrap_or(0);
    // CTOX adapter guard: reject changed/unknown identity before allocating any payload.
    if let Some(expected) = job.opts.get("ctox-expected-length") {
        let expected = expected.parse::<u64>().map_err(|_| Error::Http("invalid expected length".into()))?;
        if probed.length != Some(expected) {
            return Err(Error::Http("content length differs from pinned identity".into()));
        }
    }
    // Only a host-pinned full content digest allows different mirrors to
    // contribute bytes to the same destination. Length/range probes exclude
    // incompatible mirrors; the final checksum validates the assembled object.
    let mut range_sources = vec![uri.clone()];
    if job.opts.get("ctox-expected-length").is_some()
        && candidates.len() > 1
        && job.opts.split() > 1
        && probed.accept_ranges
        && total > 0
    {
        let valid_digest = job.opts.get("checksum").and_then(|s| s.split_once('='))
            .map(|(kind, digest)| kind == "sha-256" && digest.len() == 64
                && digest.bytes().all(|b| b.is_ascii_hexdigit()))
            .unwrap_or(false);
        if !valid_digest {
            return Err(Error::Http("multiple pinned sources require SHA-256 identity".into()));
        }
        for candidate in candidates.iter().take(16) {
            let Ok(candidate) = crate::dns::rewrite(candidate, &job.opts).await else { continue; };
            if range_sources.contains(&candidate) { continue; }
            let mut probe_cancel = job.cancel.clone();
            if *probe_cancel.borrow() || job.progress.halt.load(Ordering::Relaxed) {
                return Err(Error::Http("canceled".into()));
            }
            let result = tokio::select! {
                biased;
                _ = probe_cancel.changed() => return Err(Error::Http("canceled".into())),
                result = async {
                    if job.opts.bool("use-head", false) {
                        head_probe(&client, &candidate, &jar, &job.opts).await
                    } else {
                        probe(&client, &candidate, &jar, &job.opts).await
                    }
                } => result,
            };
            if matches!(result, Ok(ref p) if p.length == Some(total)
                && p.accept_ranges && !p.not_modified)
            {
                range_sources.push(candidate);
            }
        }
    }
    job.progress.total.store(total, Ordering::Relaxed);
    let mut dest = job.dest.clone();
    if job.opts.get("out").filter(|s| !s.is_empty()).is_none() {
        if let Some(raw) = probed.content_disposition.as_deref() {
            let utf8 = job.opts.bool("content-disposition-default-utf8", false);
            if let Some(name) = parse_content_disposition(raw, utf8) {
                dest = job.opts.dir().join(name);
            }
        }
    }
    // C++ `--no-overwrite`: skip if dest exists (no resume, no rename, not an error).
    if job.opts.bool("no-overwrite", false) && dest.exists() {
        let n = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
        job.progress.completed.store(n, Ordering::Relaxed);
        if job.progress.total.load(Ordering::Relaxed) == 0 {
            job.progress.total.store(n, Ordering::Relaxed);
        }
        return Ok(dest);
    }
    if !job.opts.bool("conditional-get", false) {
        dest = resolve_existing_dest(dest, &job.opts)?;
    }
    if job.opts.bool("dry-run", false) {
        job.progress.completed.store(total, Ordering::Relaxed);
        return Ok(dest);
    }
    let alloc = crate::storage::alloc_mode_for(&job.opts, total);
    let store = Arc::new(FileStorage::from_opts(dest.clone(), total, alloc, &job.opts));
    store.ensure().await?;
    let mut cancel = job.cancel;

    let dest_len = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
    let want_resume = job.opts.bool("continue", false) && dest_len > 0;

    if total == 0 || !probed.accept_ranges {
        if want_resume && !probed.accept_ranges {
            if !can_start_from_scratch(&job.opts, 1) {
                return Err(Error::Http("cannot resume".into()));
            }
        }
        fetch_whole(&client, &uri, store.as_ref(), &job.progress, &mut cancel, &jar, &job.opts).await?;
        maybe_remote_time(&dest, &job.opts, probed.last_modified.as_deref())?;
        if !job.progress.halt.load(Ordering::Relaxed) {
            crate::control_file::remove(&dest);
        }
        crate::checksum::verify_store(store.as_ref(), &job.opts)?;
        job.progress.checksum_done.store(true, Ordering::Relaxed);
        return Ok(dest);
    }

    let piece_len = job.piece_length.max(1);
    let n_pieces = ((total + piece_len as u64 - 1) / piece_len as u64) as u32;
    let split = job.opts.split();
    let min_split = job.opts.min_split_size();
    let max_conn = job.opts.usize("max-connection-per-server", 1).max(1);
    let connections = if total < min_split {
        1
    } else {
        split.min(n_pieces as usize).min(max_conn).max(1)
    };

    let mut resume_from = 0u64;
    let remove_ctl = job.opts.bool("remove-control-file", false);
    if remove_ctl {
        crate::control_file::remove(&dest);
    }
    let mut ctl = crate::control_file::Control::new(piece_len, total);
    let mut used_bits = false;
    let mut discard_prefix = false;
    if !remove_ctl {
        if let Some(loaded) = crate::control_file::Control::load(&dest) {
            if loaded.piece_length != piece_len {
                if !job.opts.bool("allow-piece-length-change", false) {
                    return Err(Error::Http("piece length changed".into()));
                }
                crate::control_file::remove(&dest);
                discard_prefix = true;
            } else if job.opts.bool("continue", false) && loaded.total_length == total {
                // A valid control file is authoritative even with no completed
                // pieces. Parallel writes may extend a sparse file past holes;
                // its length must never become an assumed contiguous prefix.
                used_bits = true;
                ctl = loaded;
            }
        }
    }
    if job.opts.bool("continue", false) && !remove_ctl && !used_bits && !discard_prefix {
        if let Ok(meta) = std::fs::metadata(&dest) {
            resume_from = meta.len().min(total);
            resume_from = (resume_from / piece_len as u64) * piece_len as u64;
            job.progress.completed.store(resume_from, Ordering::Relaxed);
        }
    }

    let mut pieces: Vec<(u32, u64, u64)> = Vec::new();
    for i in 0..n_pieces {
        if used_bits && ctl.has(i) {
            continue;
        }
        let start = i as u64 * piece_len as u64;
        if start < resume_from {
            continue;
        }
        let end = (start + piece_len as u64 - 1).min(total.saturating_sub(1));
        pieces.push((i, start, end));
    }
    apply_stream_piece_selector(
        job.opts.get("stream-piece-selector").unwrap_or("default"),
        &mut pieces,
    );

    // Persist even an empty bitmap before parallel writes can create holes.
    // After a process interruption, absence of completed pieces must not be
    // mistaken for a legacy contiguous partial download without a control file.
    if connections > 1 {
        ctl.save(&dest).map_err(|error| Error::Other(error.to_string()))?;
    }
    let interval = job.opts.u64("auto-save-interval", 60);
    let ctl = Arc::new(Mutex::new(ctl));
    let saver_stop = spawn_control_auto_save(
        dest.clone(),
        Arc::clone(&ctl),
        interval,
        Arc::clone(&job.progress.halt),
        cancel.clone(),
    );

    if job.opts.bool("enable-http-pipelining", false)
        && uri.starts_with("http://")
        && !pieces.is_empty()
    {
        match pipeline_http1(
            &uri,
            store.as_ref(),
            &pieces,
            &job.progress,
            &mut cancel,
            &job.opts,
            &ctl,
        )
        .await
        {
            Ok(()) => {
                saver_stop.store(true, Ordering::Relaxed);
                if !job.progress.halt.load(Ordering::Relaxed) {
                    job.progress.completed.store(total, Ordering::Relaxed);
                    maybe_remote_time(&dest, &job.opts, probed.last_modified.as_deref())?;
                    crate::control_file::remove(&dest);
                }
                return Ok(dest);
            }
            Err(e) => {
                saver_stop.store(true, Ordering::Relaxed);
                return Err(e);
            }
        }
    }

    let n_groups = if connections > 1 {
        connections
    } else if job.piece_length >= 256 * 1024 && pieces.len() > 1 {
        // C++ FileEntry: one connection still downloads the whole remaining span
        // as byte slices; per-piece GETs are only needed for small test pieces
        // (inorder Range log, gated .aria2).
        1
    } else {
        pieces.len().max(1)
    };
    let groups = merge_http_ranges(&pieces, n_groups);

    if connections <= 1 || groups.len() <= 1 {
        for (idxs, start, end) in groups {
            if job.progress.halt.load(Ordering::Relaxed) {
                if let Ok(g) = ctl.lock() {
                    let _ = g.save(&dest);
                }
                saver_stop.store(true, Ordering::Relaxed);
                return Ok(dest);
            }
            match fetch_range(&client, &uri, store.as_ref(), start, end, &job.progress, &mut cancel, &jar, &job.opts).await {
                Ok(()) => {
                    if let Ok(mut g) = ctl.lock() {
                        for idx in idxs {
                            g.set(idx);
                        }
                    }
                }
                Err(e) if e.to_string().contains("resume not possible") => {
                    saver_stop.store(true, Ordering::Relaxed);
                    if !can_start_from_scratch(&job.opts, 1) {
                        return Err(e);
                    }
                    fetch_whole(&client, &uri, store.as_ref(), &job.progress, &mut cancel, &jar, &job.opts).await?;
                    maybe_remote_time(&dest, &job.opts, probed.last_modified.as_deref())?;
                    crate::control_file::remove(&dest);
                    return Ok(dest);
                }
                Err(e) if e.to_string().contains("canceled") => {
                    if let Ok(g) = ctl.lock() {
                        let _ = g.save(&dest);
                    }
                    saver_stop.store(true, Ordering::Relaxed);
                    return Err(e);
                }
                Err(e) => {
                    saver_stop.store(true, Ordering::Relaxed);
                    return Err(e);
                }
            }
        }
        saver_stop.store(true, Ordering::Relaxed);
        if !job.progress.halt.load(Ordering::Relaxed) {
            job.progress.completed.store(total, Ordering::Relaxed);
            maybe_remote_time(&dest, &job.opts, probed.last_modified.as_deref())?;
            crate::control_file::remove(&dest);
        }
        return Ok(dest);
    }

    let sem = Arc::new(tokio::sync::Semaphore::new(connections));
    // These futures are owned by this download. Returning an error or dropping
    // the download cancels every pending range before the caller can quarantine,
    // retry or publish the destination; no detached writer survives the attempt.
    let mut handles = futures::stream::FuturesUnordered::new();
    for (group_index, (idxs, start, end)) in groups.into_iter().enumerate() {
        if job.progress.halt.load(Ordering::Relaxed) {
            break;
        }
        let sem = sem.clone();
        let client = client.clone();
        let uri = range_sources[group_index % range_sources.len()].clone();
        let progress = job.progress.clone();
        let mut c = cancel.clone();
        let jar = jar.clone();
        let opts = job.opts.clone();
        let ctl = ctl.clone();
        let store = Arc::clone(&store);
        handles.push(async move {
            let _permit = sem.acquire_owned().await.unwrap();
            let r = fetch_range(&client, &uri, store.as_ref(), start, end, &progress, &mut c, &jar, &opts).await;
            if r.is_ok() {
                if let Ok(mut g) = ctl.lock() {
                    for idx in idxs {
                        g.set(idx);
                    }
                }
            }
            r
        });
    }
    while let Some(result) = handles.next().await {
        match result {
            Ok(()) => {}
            Err(e) if e.to_string().contains("canceled") => {
                if let Ok(g) = ctl.lock() {
                    let _ = g.save(&dest);
                }
                saver_stop.store(true, Ordering::Relaxed);
                return Err(e);
            }
            Err(e) => {
                saver_stop.store(true, Ordering::Relaxed);
                return Err(e);
            }
        }
    }
    saver_stop.store(true, Ordering::Relaxed);
    job.progress.completed.store(total, Ordering::Relaxed);
    maybe_remote_time(&dest, &job.opts, probed.last_modified.as_deref())?;
    crate::control_file::remove(&dest);
    crate::checksum::verify_store(store.as_ref(), &job.opts)?;
    job.progress.checksum_done.store(true, Ordering::Relaxed);
    Ok(dest)
}

fn spawn_control_auto_save(
    dest: PathBuf,
    ctl: Arc<Mutex<crate::control_file::Control>>,
    interval: u64,
    halt: Arc<AtomicBool>,
    mut cancel: watch::Receiver<bool>,
) -> Arc<AtomicBool> {
    let stop = Arc::new(AtomicBool::new(false));
    if interval == 0 {
        return stop;
    }
    let stop2 = Arc::clone(&stop);
    tokio::spawn(async move {
        let d = Duration::from_secs(interval);
        loop {
            tokio::select! {
                _ = tokio::time::sleep(d) => {}
                _ = cancel.changed() => {}
            }
            if stop2.load(Ordering::Relaxed) || halt.load(Ordering::Relaxed) || *cancel.borrow() {
                break;
            }
            if let Ok(g) = ctl.lock() {
                let _ = g.save(&dest);
            }
        }
    });
    stop
}

#[cfg(test)]
mod tests {
    use super::parse_content_disposition;

    #[test]
    fn filename_utf8_bytes_honors_default_utf8() {
        let mut raw = b"attachment; filename=\"".to_vec();
        raw.extend_from_slice("café.bin".as_bytes());
        raw.extend_from_slice(b"\"");
        assert_eq!(
            parse_content_disposition(&raw, true).as_deref(),
            Some("café.bin")
        );
        assert_eq!(
            parse_content_disposition(&raw, false).as_deref(),
            Some("cafÃ©.bin")
        );
    }

    #[test]
    fn filename_star_rfc5987_always_utf8() {
        let raw = b"attachment; filename*=UTF-8''caf%C3%A9.bin";
        assert_eq!(
            parse_content_disposition(raw, false).as_deref(),
            Some("café.bin")
        );
        assert_eq!(
            parse_content_disposition(raw, true).as_deref(),
            Some("café.bin")
        );
    }

    #[test]
    fn http_date_roundtrip_known_stamp() {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_445_412_480);
        let s = super::fmt_http_date(t);
        assert_eq!(s, "Wed, 21 Oct 2015 07:28:00 GMT");
        let back = super::parse_http_date(&s).unwrap();
        assert_eq!(
            back.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
            1_445_412_480
        );
    }

    #[test]
    fn parse_speed_kmg() {
        assert_eq!(super::parse_speed("0"), Some(0));
        assert_eq!(super::parse_speed("2000"), Some(2000));
        assert_eq!(super::parse_speed("1K"), Some(1024));
        assert_eq!(super::parse_speed("2M"), Some(2 * 1024 * 1024));
    }

    #[test]
    fn stream_piece_selector_inorder_is_identity() {
        let mut p: Vec<(u32, u64, u64)> = (0u32..8)
            .map(|i| (i, i as u64 * 10, i as u64 * 10 + 9))
            .collect();
        super::apply_stream_piece_selector("inorder", &mut p);
        let idx: Vec<u32> = p.iter().map(|x| x.0).collect();
        assert_eq!(idx, vec![0, 1, 2, 3, 4, 5, 6, 7]);
        super::apply_stream_piece_selector("default", &mut p);
        let idx: Vec<u32> = p.iter().map(|x| x.0).collect();
        assert_eq!(idx, vec![0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn merge_http_ranges_split_not_per_piece() {
        let pieces: Vec<(u32, u64, u64)> = (0u32..8)
            .map(|i| (i, i as u64 * 10, i as u64 * 10 + 9))
            .collect();
        let g = super::merge_http_ranges(&pieces, 4);
        assert_eq!(g.len(), 4, "C++ --split=4 is 4 Range GETs");
        assert_eq!(g[0], (vec![0, 1], 0, 19));
        assert_eq!(g[3].0, vec![6, 7]);
        assert_eq!(g[3].1, 60);
        assert_eq!(g[3].2, 79);
        let one = super::merge_http_ranges(&pieces, 1);
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].1, 0);
        assert_eq!(one[0].2, 79);
        let shuffled = vec![(1, 10, 19), (0, 0, 9)];
        assert_eq!(super::merge_http_ranges(&shuffled, 2).len(), 2);
    }

    #[test]
    fn stream_piece_selector_random_permutes() {
        let orig: Vec<(u32, u64, u64)> = (0u32..16).map(|i| (i, 0, 0)).collect();
        let mut saw_diff = false;
        for _ in 0..40 {
            let mut p = orig.clone();
            super::apply_stream_piece_selector("random", &mut p);
            if p.iter().map(|x| x.0).ne(orig.iter().map(|x| x.0)) {
                saw_diff = true;
                break;
            }
        }
        assert!(saw_diff, "random selector must permute remaining pieces");
    }

    #[test]
    fn expand_brace_and_numeric() {
        assert_eq!(
            super::expand_parameterized("http://h/p.{a,b}.bin"),
            vec!["http://h/p.a.bin", "http://h/p.b.bin"]
        );
        assert_eq!(
            super::expand_parameterized("http://h/n[01-02].bin"),
            vec!["http://h/n01.bin", "http://h/n02.bin"]
        );
        assert_eq!(
            super::expand_parameterized("http://h/s[1-5:2].bin"),
            vec!["http://h/s1.bin", "http://h/s3.bin", "http://h/s5.bin"]
        );
    }

    #[tokio::test]
    async fn happy_eyeballs_v4_wins_before_full_timeout() {
        use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
        use std::sync::atomic::Ordering;
        use std::time::{Duration, Instant};
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        // Kernel completes TCP handshake from the listen backlog; no accept needed.
        // IPv4-mapped TEST-NET hangs; C++ would wait connect-timeout if serial.
        let v6 = SocketAddr::new(std::net::IpAddr::V6(Ipv6Addr::new(
            0, 0, 0, 0, 0, 0xffff, 0xc000, 0x0201,
        )), 9);
        let v4 = SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        let opts = crate::options::OptionSet::with_defaults();
        super::LAST_HE_WIN.store(0, Ordering::SeqCst);
        super::LAST_HE_V6_TRY.store(0, Ordering::SeqCst);
        super::LAST_HE_V4_TRY.store(0, Ordering::SeqCst);
        let t0 = Instant::now();
        super::connect_happy(vec![v6, v4], &opts, Duration::from_secs(5))
            .await
            .expect("v4 must win Happy Eyeballs race");
        let elapsed = t0.elapsed();
        assert!(
            elapsed < Duration::from_secs(2),
            "C++ HE 300ms AAAA-then-A must not wait 5s serial timeout, got {elapsed:?}"
        );
        assert!(
            elapsed >= Duration::from_millis(200),
            "v6 hang must delay v4 ~300ms, got {elapsed:?}"
        );
        assert_eq!(super::last_he_win(), 4, "IPv4 must win hanging AAAA");
        assert!(super::last_he_v6_try() >= 1);
        assert!(super::last_he_v4_try() >= 1);
    }
}
