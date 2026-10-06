//! BitTorrent DHT (BEP 5) KRPC: get_peers over UDP.
#![forbid(unsafe_code)]

use crate::bencode::{self, BVal};
use crate::bt::{
    encode_compact_peers, encode_compact_peers6, parse_compact_peers, parse_compact_peers6,
    parse_listen_ports,
};
use crate::error::{Error, Result};
use crate::options::OptionSet;
use sha1::{Digest, Sha1};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::path::Path;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;
use tokio::net::UdpSocket;

static LAST_DHT_LISTEN_PORT: AtomicU16 = AtomicU16::new(0);

pub fn last_dht_listen_port() -> u16 {
    LAST_DHT_LISTEN_PORT.load(Ordering::SeqCst)
}

async fn bind_dht(ip: IpAddr, opts: &OptionSet) -> Result<UdpSocket> {
    let spec = opts.get("dht-listen-port").unwrap_or("6881");
    let mut last = Error::Bt("dht-listen-port: no port".into());
    for port in parse_listen_ports(spec) {
        if port == 0 {
            continue;
        }
        match UdpSocket::bind(SocketAddr::new(ip, port)).await {
            Ok(s) => {
                LAST_DHT_LISTEN_PORT.store(port, Ordering::SeqCst);
                return Ok(s);
            }
            Err(e) => last = Error::Bt(format!("dht-listen-port {port}: {e}")),
        }
    }
    Err(last)
}

fn node_id() -> [u8; 20] {
    Sha1::digest(b"aria2-rust-dht").into()
}

fn parse_listen_addr4(s: &str) -> Result<Ipv4Addr> {
    s.trim()
        .parse()
        .map_err(|_| Error::Bt(format!("dht-listen-addr: {s}")))
}

fn parse_listen_addr6(s: &str) -> Result<Ipv6Addr> {
    let t = s.trim();
    let t = t
        .strip_prefix('[')
        .and_then(|x| x.strip_suffix(']'))
        .unwrap_or(t);
    t.parse()
        .map_err(|_| Error::Bt(format!("dht-listen-addr6: {s}")))
}

fn parse_entry_point(s: &str) -> Result<SocketAddr> {
    s.trim()
        .to_socket_addrs()
        .ok()
        .and_then(|mut i| i.next())
        .ok_or_else(|| Error::Bt(format!("dht-entry-point: {}", s.trim())))
}

fn krpc_get_peers(tid: &[u8], info_hash: &[u8; 20]) -> Vec<u8> {
    bencode::encode(&BVal::Dict(vec![
        (
            b"a".to_vec(),
            BVal::Dict(vec![
                (b"id".to_vec(), BVal::Bytes(node_id().to_vec())),
                (b"info_hash".to_vec(), BVal::Bytes(info_hash.to_vec())),
            ]),
        ),
        (b"q".to_vec(), BVal::Bytes(b"get_peers".to_vec())),
        (b"t".to_vec(), BVal::Bytes(tid.to_vec())),
        (b"y".to_vec(), BVal::Bytes(b"q".to_vec())),
    ]))
}

fn parse_values(v: &BVal) -> Vec<SocketAddr> {
    if let Some(b) = v.as_bytes() {
        return parse_compact_peers(b);
    }
    match v {
        BVal::List(xs) => {
            let mut o = Vec::new();
            for x in xs {
                if let Some(b) = x.as_bytes() {
                    o.extend(parse_compact_peers(b));
                }
            }
            o
        }
        _ => Vec::new(),
    }
}

fn parse_compact_nodes6(b: &[u8]) -> Vec<SocketAddr> {
    let mut o = Vec::new();
    for c in b.chunks(38) {
        if c.len() < 38 {
            break;
        }
        o.extend(parse_compact_peers6(&c[20..38]));
    }
    o
}

fn parse_values6(v: &BVal) -> Vec<SocketAddr> {
    if let Some(b) = v.as_bytes() {
        return parse_compact_peers6(b);
    }
    match v {
        BVal::List(xs) => {
            let mut o = Vec::new();
            for x in xs {
                if let Some(b) = x.as_bytes() {
                    o.extend(parse_compact_peers6(b));
                }
            }
            o
        }
        _ => Vec::new(),
    }
}

fn parse_compact_nodes(b: &[u8]) -> Vec<SocketAddr> {
    let mut o = Vec::new();
    for c in b.chunks(26) {
        if c.len() < 26 {
            break;
        }
        o.extend(parse_compact_peers(&c[20..26]));
    }
    o
}

struct KrpcPeers {
    values: Vec<SocketAddr>,
    nodes: Vec<SocketAddr>,
}

fn peers_from_krpc(msg: &[u8], tid: &[u8], v6: bool) -> Result<KrpcPeers> {
    let v = bencode::decode(msg).map_err(|e| Error::Bt(format!("dht: {e}")))?;
    let got_t = v.dict_get(b"t").and_then(|x| x.as_bytes()).unwrap_or(b"");
    if got_t != tid {
        return Err(Error::Bt("dht tid mismatch".into()));
    }
    if v.dict_get(b"y").and_then(|x| x.as_bytes()) != Some(b"r") {
        return Err(Error::Bt("dht not a response".into()));
    }
    let r = v.dict_get(b"r").ok_or_else(|| Error::Bt("dht no r".into()))?;
    let values = if v6 {
        r.dict_get(b"values").map(parse_values6).unwrap_or_default()
    } else {
        r.dict_get(b"values").map(parse_values).unwrap_or_default()
    };
    let nodes = if v6 {
        r.dict_get(b"nodes6")
            .and_then(|x| x.as_bytes())
            .map(parse_compact_nodes6)
            .unwrap_or_default()
    } else {
        r.dict_get(b"nodes")
            .and_then(|x| x.as_bytes())
            .map(parse_compact_nodes)
            .unwrap_or_default()
    };
    Ok(KrpcPeers { values, nodes })
}

async fn query(
    sock: &UdpSocket,
    dest: SocketAddr,
    info_hash: &[u8; 20],
    tid: &[u8],
    wait: Duration,
    v6: bool,
) -> Result<KrpcPeers> {
    let q = krpc_get_peers(tid, info_hash);
    sock.send_to(&q, dest).await?;
    let mut buf = [0u8; 2048];
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            return Err(Error::Bt("dht timeout".into()));
        }
        let (n, _) = tokio::time::timeout(left, sock.recv_from(&mut buf))
            .await
            .map_err(|_| Error::Bt("dht timeout".into()))??;
        match peers_from_krpc(&buf[..n], tid, v6) {
            Ok(p) => return Ok(p),
            Err(_) => continue,
        }
    }
}

async fn walk(
    sock: &UdpSocket,
    dests: Vec<SocketAddr>,
    info_hash: &[u8; 20],
    wait: Duration,
    v6: bool,
    opts: &OptionSet,
) -> Result<Vec<SocketAddr>> {
    let mut saved: Vec<SocketAddr> = Vec::new();
    let mut tid = [b'a', b'a'];
    for dest in dests {
        tid[1] = tid[1].wrapping_add(1);
        match query(sock, dest, info_hash, &tid, wait, v6).await {
            Ok(first) => {
                if !saved.contains(&dest) {
                    saved.push(dest);
                }
                if !first.values.is_empty() {
                    persist(opts, &saved, v6);
                    return Ok(first.values);
                }
                for node in first.nodes.into_iter().take(3) {
                    if !saved.contains(&node) {
                        saved.push(node);
                    }
                    tid[1] = tid[1].wrapping_add(1);
                    if let Ok(hop) = query(sock, node, info_hash, &tid, wait, v6).await {
                        if !hop.values.is_empty() {
                            persist(opts, &saved, v6);
                            return Ok(hop.values);
                        }
                    }
                }
            }
            Err(_) => {}
        }
    }
    persist(opts, &saved, v6);
    Ok(Vec::new())
}

/// Bootstrap via IPv4 `--dht-entry-point`/`--dht-file-path` then IPv6 `--dht-entry-point6`.
pub async fn get_peers(info_hash: [u8; 20], opts: &OptionSet) -> Result<Vec<SocketAddr>> {
    LAST_DHT_LISTEN_PORT.store(0, Ordering::SeqCst);
    let wait = Duration::from_secs(opts.u64("dht-message-timeout", 10).max(1));
    if opts.bool("enable-dht", true) {
        let mut dests: Vec<SocketAddr> = Vec::new();
        if let Some(ep) = opts.get("dht-entry-point").filter(|s| !s.is_empty()) {
            dests.push(parse_entry_point(ep)?);
        }
        if let Some(path) = opts.get("dht-file-path").filter(|s| !s.is_empty()) {
            for n in load_dht_file(path) {
                if !dests.contains(&n) {
                    dests.push(n);
                }
            }
        }
        if !dests.is_empty() {
            let ip = match opts.get("dht-listen-addr").filter(|s| !s.is_empty()) {
                Some(s) => parse_listen_addr4(s)?,
                None => Ipv4Addr::UNSPECIFIED,
            };
            let sock = bind_dht(IpAddr::V4(ip), opts).await?;
            let found = walk(&sock, dests, &info_hash, wait, false, opts).await?;
            if !found.is_empty() {
                return Ok(found);
            }
        }
    }
    if opts.bool("enable-dht6", false) {
        let mut dests: Vec<SocketAddr> = Vec::new();
        if let Some(ep) = opts.get("dht-entry-point6").filter(|s| !s.is_empty()) {
            dests.push(parse_entry_point(ep)?);
        }
        if let Some(path) = opts.get("dht-file-path6").filter(|s| !s.is_empty()) {
            for n in load_dht_file6(path) {
                if !dests.contains(&n) {
                    dests.push(n);
                }
            }
        }
        if !dests.is_empty() {
            let ip = match opts.get("dht-listen-addr6").filter(|s| !s.is_empty()) {
                Some(s) => parse_listen_addr6(s)?,
                None => Ipv6Addr::UNSPECIFIED,
            };
            let sock = bind_dht(IpAddr::V6(ip), opts).await?;
            let found = walk(&sock, dests, &info_hash, wait, true, opts).await?;
            if !found.is_empty() {
                return Ok(found);
            }
        }
    }
    Ok(Vec::new())
}

fn load_dht_file(path: &str) -> Vec<SocketAddr> {
    let Ok(b) = std::fs::read(path) else {
        return Vec::new();
    };
    parse_compact_peers(&b)
}

fn load_dht_file6(path: &str) -> Vec<SocketAddr> {
    let Ok(b) = std::fs::read(path) else {
        return Vec::new();
    };
    parse_compact_peers6(&b)
}

fn persist(opts: &OptionSet, nodes: &[SocketAddr], v6: bool) {
    if nodes.is_empty() {
        return;
    }
    let key = if v6 { "dht-file-path6" } else { "dht-file-path" };
    let Some(path) = opts.get(key).filter(|s| !s.is_empty()) else {
        return;
    };
    if let Some(parent) = Path::new(path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let bytes = if v6 {
        encode_compact_peers6(nodes)
    } else {
        encode_compact_peers(nodes)
    };
    let _ = std::fs::write(path, bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_values_list_and_bytes() {
        let compact = vec![127, 0, 0, 1, 0x1A, 0xE1];
        let listed = BVal::List(vec![BVal::Bytes(compact.clone())]);
        let p = parse_values(&listed);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].port(), 6881);
        let p2 = parse_values(&BVal::Bytes(compact));
        assert_eq!(p2[0].port(), 6881);
    }

    #[test]
    fn dht_file_compact_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dht.dat");
        let addr = "127.0.0.1:6881".parse().unwrap();
        std::fs::write(&path, encode_compact_peers(&[addr])).unwrap();
        let got = load_dht_file(path.to_str().unwrap());
        assert_eq!(got, vec![addr]);
    }

    #[test]
    fn dht_file6_compact_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dht6.dat");
        let addr: SocketAddr = "[::1]:6881".parse().unwrap();
        std::fs::write(&path, encode_compact_peers6(&[addr])).unwrap();
        let got = load_dht_file6(path.to_str().unwrap());
        assert_eq!(got, vec![addr]);
    }

    #[test]
    fn parse_values6_list() {
        let mut compact = vec![0u8; 16];
        compact[15] = 1;
        compact.extend_from_slice(&6881u16.to_be_bytes());
        let listed = BVal::List(vec![BVal::Bytes(compact)]);
        let p = parse_values6(&listed);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].port(), 6881);
        assert!(p[0].is_ipv6());
    }

    #[test]
    fn parse_listen_addr6_brackets() {
        assert_eq!(parse_listen_addr6("::1").unwrap(), Ipv6Addr::LOCALHOST);
        assert_eq!(parse_listen_addr6("[::1]").unwrap(), Ipv6Addr::LOCALHOST);
        assert!(parse_listen_addr6("127.0.0.1").is_err());
    }

    #[test]
    fn parse_listen_addr4_loopback() {
        assert_eq!(parse_listen_addr4("127.0.0.1").unwrap(), Ipv4Addr::LOCALHOST);
        assert!(parse_listen_addr4("::1").is_err());
        assert!(parse_listen_addr4("not-an-ip").is_err());
    }
}
