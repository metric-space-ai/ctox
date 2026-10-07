//! FTP RETR: PASV / PORT (active) + REST resume. Streaming, not slurped.
#![forbid(unsafe_code)]

use crate::error::{Error, Result};
use crate::http::HttpProgress;
use crate::options::OptionSet;
use crate::storage::FileStorage;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use url::Url;

pub struct FtpJob {
    pub uris: Vec<String>,
    pub dest: PathBuf,
    pub opts: OptionSet,
    pub progress: HttpProgress,
    pub cancel: watch::Receiver<bool>,
}

struct Ctrl {
    s: TcpStream,
    leftover: Vec<u8>,
    off: usize,
}

impl Ctrl {
    async fn from_stream(stream: TcpStream) -> Result<Self> {
        let _ = crate::sockopt::apply_tcp_nodelay(&stream);
        let _ = crate::sockopt::apply_tcp_quickack(&stream);
        let mut c = Self {
            s: stream,
            leftover: Vec::new(),
            off: 0,
        };
        let (code, msg) = c.read_reply().await?;
        if code != 220 {
            return Err(Error::Ftp(format!("greeting {code} {msg}")));
        }
        Ok(c)
    }

    fn leftover_avail(&self) -> &[u8] {
        if self.off >= self.leftover.len() {
            &[]
        } else {
            &self.leftover[self.off..]
        }
    }

    fn consume(&mut self, n: usize) {
        self.off = (self.off + n).min(self.leftover.len());
        if self.off >= self.leftover.len() {
            self.leftover.clear();
            self.off = 0;
        } else if self.off >= 1024 {
            self.leftover.copy_within(self.off.., 0);
            let live = self.leftover.len() - self.off;
            self.leftover.truncate(live);
            self.off = 0;
        }
    }

    async fn read_line(&mut self) -> Result<String> {
        loop {
            if let Some(pos) = self.leftover_avail().iter().position(|&b| b == b'\n') {
                let line = String::from_utf8_lossy(&self.leftover_avail()[..=pos]).into_owned();
                self.consume(pos + 1);
                return Ok(line);
            }
            if self.leftover_avail().len() > 64 * 1024 {
                return Err(Error::Ftp("reply too large".into()));
            }
            let mut tmp = [0u8; 512];
            let n = crate::sockopt::recv_some(&self.s, &mut tmp).await?;
            if n == 0 {
                return Err(Error::Ftp("eof".into()));
            }
            if self.off >= self.leftover.len() {
                self.leftover.clear();
                self.off = 0;
            }
            self.leftover.extend_from_slice(&tmp[..n]);
        }
    }

    async fn read_reply(&mut self) -> Result<(u16, String)> {
        let mut line = self.read_line().await?;
        if line.len() < 4 {
            return Err(Error::Ftp(format!("short reply: {line:?}")));
        }
        let code: u16 = line[..3]
            .parse()
            .map_err(|_| Error::Ftp(format!("bad code: {line:?}")))?;
        let mut full = line.clone();
        if line.as_bytes().get(3) == Some(&b'-') {
            let prefix = format!("{code} ");
            loop {
                line = self.read_line().await?;
                if line.is_empty() {
                    break;
                }
                full.push_str(&line);
                if line.starts_with(&prefix) {
                    break;
                }
            }
        }
        Ok((code, full))
    }

    async fn cmd(&mut self, s: &str) -> Result<(u16, String)> {
        let mut msg = String::with_capacity(s.len() + 2);
        msg.push_str(s);
        msg.push_str("\r\n");
        crate::sockopt::send_all(&self.s, msg.as_bytes()).await?;
        self.read_reply().await
    }

    fn local_v4(&self) -> Result<Ipv4Addr> {
        match self.s.local_addr()?.ip() {
            IpAddr::V4(v) => Ok(v),
            IpAddr::V6(_) => Err(Error::Ftp("PORT needs IPv4 control".into())),
        }
    }
}

pub(crate) fn parse_pasv(s: &str) -> Result<(Ipv4Addr, u16)> {
    let nums: Vec<u32> = s
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse().ok())
        .collect();
    if nums.len() < 6 {
        return Err(Error::Ftp(format!("PASV parse: {s}")));
    }
    let n = &nums[nums.len() - 6..];
    let ip = Ipv4Addr::new(n[0] as u8, n[1] as u8, n[2] as u8, n[3] as u8);
    let port = ((n[4] as u16) << 8) | (n[5] as u16);
    Ok((ip, port))
}

fn credentials(u: &Url, opts: &OptionSet) -> (String, String) {
    let user = if !u.username().is_empty() {
        u.username().to_string()
    } else {
        opts.get("ftp-user").unwrap_or("anonymous").to_string()
    };
    let pass = if let Some(p) = u.password() {
        p.to_string()
    } else {
        opts.get("ftp-passwd").unwrap_or("ARIA2USER@").to_string()
    };
    (user, pass)
}

fn type_cmd(opts: &OptionSet) -> &'static str {
    match opts.get("ftp-type").unwrap_or("binary") {
        "ascii" | "ASCII" | "A" | "a" => "TYPE A",
        _ => "TYPE I",
    }
}

struct FtpProxy {
    host: String,
    port: u16,
    user: Option<String>,
    pass: String,
}

fn host_in_no_proxy(host: &str, np: &str) -> bool {
    np.split(',').map(|s| s.trim()).any(|p| {
        !p.is_empty()
            && (p == "*"
                || host.eq_ignore_ascii_case(p)
                || host
                    .to_ascii_lowercase()
                    .ends_with(&format!(".{}", p.to_ascii_lowercase())))
    })
}

fn ftp_proxy_cfg(opts: &OptionSet, target_host: &str) -> Option<FtpProxy> {
    if let Some(np) = opts.get("no-proxy").filter(|s| !s.is_empty()) {
        if host_in_no_proxy(target_host, np) {
            return None;
        }
    }
    let raw = opts
        .get("ftp-proxy")
        .or_else(|| opts.get("all-proxy"))
        .filter(|s| !s.is_empty())?;
    let url = if raw.contains("://") {
        raw.to_string()
    } else {
        format!("http://{raw}")
    };
    let u = Url::parse(&url).ok()?;
    let host = u.host_str()?.to_string();
    let port = u.port_or_known_default().unwrap_or(80);
    let user = opts
        .get("ftp-proxy-user")
        .or_else(|| opts.get("all-proxy-user"))
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let pass = opts
        .get("ftp-proxy-passwd")
        .or_else(|| opts.get("all-proxy-passwd"))
        .unwrap_or("")
        .to_string();
    Some(FtpProxy {
        host,
        port,
        user,
        pass,
    })
}

async fn http_connect(
    proxy: &FtpProxy,
    target_host: &str,
    target_port: u16,
    timeout: Duration,
) -> Result<TcpStream> {
    let addr = tokio::net::lookup_host((proxy.host.as_str(), proxy.port))
        .await?
        .next()
        .ok_or_else(|| Error::Ftp("ftp-proxy dns".into()))?;
    let s = tokio::time::timeout(timeout, TcpStream::connect(addr))
        .await
        .map_err(|_| Error::Ftp("ftp-proxy connect-timeout".into()))??;
    let _ = crate::sockopt::apply_tcp_nodelay(&s);
    let _ = crate::sockopt::apply_tcp_quickack(&s);
    let mut req = format!(
        "CONNECT {target_host}:{target_port} HTTP/1.1\r\nHost: {target_host}:{target_port}\r\n"
    );
    if let Some(u) = &proxy.user {
        let token = crate::bt::b64_encode(format!("{u}:{}", proxy.pass).as_bytes());
        req.push_str(&format!("Proxy-Authorization: Basic {token}\r\n"));
    }
    req.push_str("Proxy-Connection: keep-alive\r\n\r\n");
    crate::sockopt::send_all(&s, req.as_bytes()).await?;
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1];
    loop {
        let n = crate::sockopt::recv_some(&s, &mut tmp).await?;
        if n == 0 {
            return Err(Error::Ftp("ftp-proxy eof".into()));
        }
        buf.push(tmp[0]);
        if buf.ends_with(b"\r\n\r\n") {
            break;
        }
        if buf.len() > 8192 {
            return Err(Error::Ftp("ftp-proxy header too large".into()));
        }
    }
    let head = String::from_utf8_lossy(&buf);
    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    if status != 200 {
        return Err(Error::Ftp(format!("ftp-proxy CONNECT {status}")));
    }
    Ok(s)
}

async fn open_tcp(
    host: &str,
    port: u16,
    timeout: Duration,
    proxy: Option<&FtpProxy>,
    opts: &OptionSet,
) -> Result<TcpStream> {
    let s = if let Some(p) = proxy {
        http_connect(p, host, port, timeout).await?
    } else {
        let addr = tokio::net::lookup_host((host, port))
            .await?
            .next()
            .ok_or_else(|| Error::Ftp("dns".into()))?;
        tokio::time::timeout(timeout, TcpStream::connect(addr))
            .await
            .map_err(|_| Error::Ftp("connect-timeout".into()))??
    };
    let _ = crate::sockopt::apply_tcp_nodelay(&s);
    let _ = crate::sockopt::apply_tcp_quickack(&s);
    crate::sockopt::apply_recv_buffer(&s, opts)?;
    Ok(s)
}

fn ctrl_pool() -> &'static Mutex<HashMap<String, Vec<Ctrl>>> {
    static P: OnceLock<Mutex<HashMap<String, Vec<Ctrl>>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pool_key(host: &str, port: u16, user: &str) -> String {
    format!("{user}@{host}:{port}")
}

fn take_pooled(key: &str) -> Option<Ctrl> {
    ctrl_pool().lock().unwrap().get_mut(key).and_then(|v| v.pop())
}

fn put_pooled(key: String, ctrl: Ctrl) {
    ctrl_pool().lock().unwrap().entry(key).or_default().push(ctrl);
}

async fn login_ctrl(mut ctrl: Ctrl, user: &str, pass: &str) -> Result<Ctrl> {
    let (code, msg) = ctrl.cmd(&format!("USER {user}")).await?;
    if code == 331 {
        let (code, msg) = ctrl.cmd(&format!("PASS {pass}")).await?;
        if code != 230 && code != 202 {
            return Err(Error::Ftp(format!("PASS {code} {msg}")));
        }
    } else if code != 230 && code != 202 {
        return Err(Error::Ftp(format!("USER {code} {msg}")));
    }
    Ok(ctrl)
}

async fn acquire_ctrl(
    host: &str,
    port: u16,
    user: &str,
    pass: &str,
    timeout: Duration,
    proxy: Option<&FtpProxy>,
    reuse: bool,
    opts: &OptionSet,
) -> Result<Ctrl> {
    let key = pool_key(host, port, user);
    if reuse {
        if let Some(mut c) = take_pooled(&key) {
            match c.cmd("NOOP").await {
                Ok((code, _)) if (200..300).contains(&code) => return Ok(c),
                _ => {}
            }
        }
    }
    let stream = open_tcp(host, port, timeout, proxy, opts).await?;
    let ctrl = Ctrl::from_stream(stream).await?;
    login_ctrl(ctrl, user, pass).await
}

async fn release_ctrl(mut ctrl: Ctrl, reuse: bool, key: String) {
    if reuse {
        put_pooled(key, ctrl);
    } else {
        let _ = ctrl.cmd("QUIT").await;
    }
}

pub async fn download(job: FtpJob) -> Result<()> {
    let dest = job.dest.clone();
    let opts = job.opts.clone();
    let done = std::sync::Arc::clone(&job.progress.checksum_done);
    download_inner(job).await?;
    if done.load(Ordering::Relaxed) {
        return Ok(());
    }
    crate::checksum::verify_dest(&dest, &opts).await
}

async fn download_inner(job: FtpJob) -> Result<()> {
    let uri = job
        .uris
        .first()
        .cloned()
        .ok_or_else(|| Error::Ftp("no uri".into()))?;
    crate::http::record_server(&job.progress, &uri);
    let u = Url::parse(&uri).map_err(|e| Error::Ftp(e.to_string()))?;
    if u.scheme() != "ftp" {
        return Err(Error::Ftp(format!("not ftp: {}", u.scheme())));
    }
    let host = u.host_str().ok_or_else(|| Error::Ftp("no host".into()))?;
    let port = u.port_or_known_default().unwrap_or(21);
    let path = if u.path().is_empty() {
        "/".to_string()
    } else {
        u.path().to_string()
    };
    let (user, pass) = credentials(&u, &job.opts);
    let timeout = Duration::from_secs(job.opts.u64("connect-timeout", 60).max(1));
    let proxy = ftp_proxy_cfg(&job.opts, host);
    let reuse = job.opts.bool("ftp-reuse-connection", true);
    let key = pool_key(host, port, &user);
    let mut ctrl = acquire_ctrl(host, port, &user, &pass, timeout, proxy.as_ref(), reuse, &job.opts).await?;

    let (code, msg) = ctrl.cmd(type_cmd(&job.opts)).await?;
    if code != 200 && code != 250 {
        return Err(Error::Ftp(format!("TYPE {code} {msg}")));
    }

    let mut total = 0u64;
    let (code, msg) = ctrl.cmd(&format!("SIZE {path}")).await?;
    if code == 213 {
        total = msg
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
    }
    job.progress.total.store(total, Ordering::Relaxed);

    let mut resume_from = 0u64;
    if job.opts.bool("continue", false) {
        if let Ok(meta) = std::fs::metadata(&job.dest) {
            resume_from = meta.len();
            if total > 0 {
                resume_from = resume_from.min(total);
            }
            job.progress.completed.store(resume_from, Ordering::Relaxed);
        }
    }

    let alloc = crate::storage::alloc_mode_for(&job.opts, total);
    let store = FileStorage::from_opts(job.dest.clone(), total, alloc, &job.opts);
    store.ensure().await?;

    let pasv = job.opts.bool("ftp-pasv", true) || proxy.is_some();
    let mut data = if pasv {
        let (code, msg) = ctrl.cmd("PASV").await?;
        if code != 227 {
            return Err(Error::Ftp(format!("PASV {code} {msg}")));
        }
        let (ip, p) = parse_pasv(&msg)?;
        open_tcp(&ip.to_string(), p, timeout, proxy.as_ref(), &job.opts).await?
    } else {
        let ip = ctrl.local_v4()?;
        let listener = TcpListener::bind(SocketAddr::from((ip, 0))).await?;
        let lp = listener.local_addr()?.port();
        let oct = ip.octets();
        let port_cmd = format!(
            "PORT {},{},{},{},{},{}",
            oct[0],
            oct[1],
            oct[2],
            oct[3],
            lp >> 8,
            lp & 0xff
        );
        let (code, msg) = ctrl.cmd(&port_cmd).await?;
        if code != 200 && code != 250 {
            return Err(Error::Ftp(format!("PORT {code} {msg}")));
        }
        if resume_from > 0 {
            let (code, msg) = ctrl.cmd(&format!("REST {resume_from}")).await?;
            if !(300..400).contains(&code) && code != 200 {
                resume_from = 0;
                job.progress.completed.store(0, Ordering::Relaxed);
                let _ = msg;
            }
        }
        let (code, msg) = ctrl.cmd(&format!("RETR {path}")).await?;
        if code != 150 && code != 125 && code != 250 {
            return Err(Error::Ftp(format!("RETR {code} {msg}")));
        }
        let (sock, _) = tokio::time::timeout(timeout, listener.accept())
            .await
            .map_err(|_| Error::Ftp("PORT accept-timeout".into()))??;
        let mut data = sock;
        crate::sockopt::apply_recv_buffer(&data, &job.opts)?;
        stream_data(&mut data, &store, resume_from, &job.progress).await?;
        let (code, msg) = ctrl.read_reply().await?;
        if code != 226 && code != 250 {
            return Err(Error::Ftp(format!("transfer {code} {msg}")));
        }
        release_ctrl(ctrl, reuse, key).await;
        if total == 0 {
            job.progress
                .total
                .store(job.progress.completed.load(Ordering::Relaxed), Ordering::Relaxed);
        } else {
            job.progress.completed.store(total, Ordering::Relaxed);
        }
        return Ok(());
    };

    if resume_from > 0 {
        let (code, msg) = ctrl.cmd(&format!("REST {resume_from}")).await?;
        if !(300..400).contains(&code) && code != 200 {
            resume_from = 0;
            job.progress.completed.store(0, Ordering::Relaxed);
            let _ = msg;
        }
    }
    let (code, msg) = ctrl.cmd(&format!("RETR {path}")).await?;
    if code != 150 && code != 125 && code != 250 {
        return Err(Error::Ftp(format!("RETR {code} {msg}")));
    }
    stream_data(&mut data, &store, resume_from, &job.progress).await?;
    drop(data);
    let (code, msg) = ctrl.read_reply().await?;
    if code != 226 && code != 250 {
        return Err(Error::Ftp(format!("transfer {code} {msg}")));
    }
    release_ctrl(ctrl, reuse, key).await;
    if total == 0 {
        job.progress
            .total
            .store(job.progress.completed.load(Ordering::Relaxed), Ordering::Relaxed);
    } else {
        job.progress.completed.store(total, Ordering::Relaxed);
    }
    crate::checksum::verify_store(&store, &job.opts)?;
    job.progress.checksum_done.store(true, Ordering::Relaxed);
    Ok(())
}

async fn stream_data(
    data: &mut TcpStream,
    store: &FileStorage,
    mut offset: u64,
    progress: &HttpProgress,
) -> Result<()> {
    let mut buf = vec![0u8; 128 * 1024];
    loop {
        let n = crate::sockopt::recv_some(data, &mut buf).await?;
        if n == 0 {
            break;
        }
        store.write_body(offset, &buf[..n]).await?;
        offset += n as u64;
        progress.completed.fetch_add(n as u64, Ordering::Relaxed);
    }
    store.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_pasv;
    use std::net::Ipv4Addr;

    #[test]
    fn pasv_paren_form() {
        let (ip, p) = parse_pasv("227 Entering Passive Mode (127,0,0,1,20,80)").unwrap();
        assert_eq!(ip, Ipv4Addr::new(127, 0, 0, 1));
        assert_eq!(p, 20 * 256 + 80);
    }

    #[test]
    fn pasv_last_six_digits() {
        let (ip, p) = parse_pasv("227 127,0,0,1,1,2").unwrap();
        assert_eq!(ip, Ipv4Addr::new(127, 0, 0, 1));
        assert_eq!(p, 258);
    }
}
