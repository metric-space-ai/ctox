//! C++ c-ares `--async-dns-server` / `--enable-async-dns6` / `--dns-timeout`.
#![forbid(unsafe_code)]

use crate::error::{Error, Result};
use crate::options::OptionSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;
use tokio::net::UdpSocket;

const QTYPE_A: u16 = 1;
const QTYPE_AAAA: u16 = 28;

pub async fn rewrite(uri: &str, opts: &OptionSet) -> Result<String> {
    if !opts.bool("async-dns", true) {
        return Ok(uri.to_string());
    }
    let Some(servers) = opts.get("async-dns-server").filter(|s| !s.is_empty()) else {
        return Ok(uri.to_string());
    };
    let mut u = url::Url::parse(uri).map_err(|e| Error::Http(format!("uri: {e}")))?;
    let host = match u.host_str() {
        Some(h) => h.to_string(),
        None => return Ok(uri.to_string()),
    };
    if host.parse::<IpAddr>().is_ok() {
        return Ok(uri.to_string());
    }
    let want_aaaa = opts.bool("enable-async-dns6", false) && !opts.bool("disable-ipv6", false);
    let timeout = Duration::from_secs(opts.u64("dns-timeout", 30).max(1));
    let ip = lookup(&host, servers, want_aaaa, timeout).await?;
    u.set_ip_host(ip)
        .map_err(|_| Error::Http("dns host".into()))?;
    Ok(u.into())
}

async fn lookup(name: &str, servers: &str, want_aaaa: bool, timeout: Duration) -> Result<IpAddr> {
    let addrs = parse_servers(servers)?;
    let mut last = Error::Http("dns: no server".into());
    for sa in addrs {
        if want_aaaa {
            if let Ok(ip) = query(name, sa, QTYPE_AAAA, timeout).await {
                return Ok(ip);
            }
        }
        match query(name, sa, QTYPE_A, timeout).await {
            Ok(ip) => return Ok(ip),
            Err(e) => last = e,
        }
    }
    Err(last)
}

fn parse_servers(s: &str) -> Result<Vec<SocketAddr>> {
    let mut out = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Ok(sa) = part.parse::<SocketAddr>() {
            out.push(sa);
            continue;
        }
        if let Ok(ip) = part.parse::<IpAddr>() {
            out.push(SocketAddr::new(ip, 53));
            continue;
        }
        return Err(Error::Http(format!("async-dns-server: {part}")));
    }
    if out.is_empty() {
        return Err(Error::Http("async-dns-server empty".into()));
    }
    Ok(out)
}

async fn query(name: &str, server: SocketAddr, qtype: u16, timeout: Duration) -> Result<IpAddr> {
    let req = encode_query(name, qtype);
    let sock = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| Error::Http(format!("dns bind: {e}")))?;
    sock.send_to(&req, server)
        .await
        .map_err(|e| Error::Http(format!("dns send: {e}")))?;
    let mut buf = [0u8; 512];
    let n = tokio::time::timeout(timeout, sock.recv(&mut buf))
        .await
        .map_err(|_| Error::Http("dns timeout".into()))?
        .map_err(|e| Error::Http(format!("dns recv: {e}")))?;
    decode_answer(&buf[..n], qtype).ok_or_else(|| Error::Http("dns: no answer".into()))
}

fn encode_query(name: &str, qtype: u16) -> Vec<u8> {
    let mut o = Vec::new();
    o.extend_from_slice(&0x1234u16.to_be_bytes());
    o.extend_from_slice(&0x0100u16.to_be_bytes());
    o.extend_from_slice(&1u16.to_be_bytes());
    o.extend_from_slice(&0u16.to_be_bytes());
    o.extend_from_slice(&0u16.to_be_bytes());
    o.extend_from_slice(&0u16.to_be_bytes());
    encode_name(&mut o, name);
    o.extend_from_slice(&qtype.to_be_bytes());
    o.extend_from_slice(&1u16.to_be_bytes());
    o
}

fn encode_name(o: &mut Vec<u8>, name: &str) {
    for label in name.trim_end_matches('.').split('.') {
        let b = label.as_bytes();
        o.push(b.len() as u8);
        o.extend_from_slice(b);
    }
    o.push(0);
}

fn decode_answer(msg: &[u8], want: u16) -> Option<IpAddr> {
    if msg.len() < 12 {
        return None;
    }
    let qd = u16::from_be_bytes(msg[4..6].try_into().ok()?) as usize;
    let an = u16::from_be_bytes(msg[6..8].try_into().ok()?) as usize;
    let mut off = 12usize;
    for _ in 0..qd {
        let (_, n) = read_name(msg, off)?;
        off = n + 4;
    }
    for _ in 0..an {
        let (_, n) = read_name(msg, off)?;
        off = n;
        if off + 10 > msg.len() {
            return None;
        }
        let typ = u16::from_be_bytes(msg[off..off + 2].try_into().ok()?);
        off += 2;
        off += 2; // class
        off += 4; // ttl
        let rdlen = u16::from_be_bytes(msg[off..off + 2].try_into().ok()?) as usize;
        off += 2;
        if off + rdlen > msg.len() {
            return None;
        }
        if typ == want {
            if want == QTYPE_A && rdlen == 4 {
                return Some(IpAddr::V4(Ipv4Addr::new(
                    msg[off],
                    msg[off + 1],
                    msg[off + 2],
                    msg[off + 3],
                )));
            }
            if want == QTYPE_AAAA && rdlen == 16 {
                let mut a = [0u8; 16];
                a.copy_from_slice(&msg[off..off + 16]);
                return Some(IpAddr::V6(Ipv6Addr::from(a)));
            }
        }
        off += rdlen;
    }
    None
}

fn read_name(msg: &[u8], mut off: usize) -> Option<(String, usize)> {
    let mut labels = Vec::new();
    let mut jumped = false;
    let mut end = off;
    let mut hops = 0;
    loop {
        if hops > 10 || off >= msg.len() {
            return None;
        }
        let len = msg[off] as usize;
        if len == 0 {
            if !jumped {
                end = off + 1;
            }
            break;
        }
        if len & 0xc0 == 0xc0 {
            if off + 1 >= msg.len() {
                return None;
            }
            let ptr = ((len & 0x3f) << 8) | msg[off + 1] as usize;
            if !jumped {
                end = off + 2;
            }
            off = ptr;
            jumped = true;
            hops += 1;
            continue;
        }
        off += 1;
        if off + len > msg.len() {
            return None;
        }
        labels.push(String::from_utf8_lossy(&msg[off..off + len]).into_owned());
        off += len;
        if !jumped {
            end = off;
        }
    }
    Some((labels.join("."), end))
}

/// Test helper: DNS reply packet (id copied from query).
pub fn encode_reply(query: &[u8], ip: IpAddr) -> Option<Vec<u8>> {
    if query.len() < 12 {
        return None;
    }
    let mut o = Vec::new();
    o.extend_from_slice(&query[0..2]);
    o.extend_from_slice(&0x8180u16.to_be_bytes());
    o.extend_from_slice(&1u16.to_be_bytes());
    o.extend_from_slice(&1u16.to_be_bytes());
    o.extend_from_slice(&0u16.to_be_bytes());
    o.extend_from_slice(&0u16.to_be_bytes());
    let (_, qend) = read_name(query, 12)?;
    if qend + 4 > query.len() {
        return None;
    }
    o.extend_from_slice(&query[12..qend + 4]);
    o.extend_from_slice(&[0xc0, 0x0c]);
    let (typ, rdata): (u16, Vec<u8>) = match ip {
        IpAddr::V4(v) => (QTYPE_A, v.octets().to_vec()),
        IpAddr::V6(v) => (QTYPE_AAAA, v.octets().to_vec()),
    };
    o.extend_from_slice(&typ.to_be_bytes());
    o.extend_from_slice(&1u16.to_be_bytes());
    o.extend_from_slice(&60u32.to_be_bytes());
    o.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
    o.extend_from_slice(&rdata);
    Some(o)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_server_ip_and_port() {
        let v = parse_servers("127.0.0.1,10.0.0.1:5353").unwrap();
        assert_eq!(v[0], "127.0.0.1:53".parse().unwrap());
        assert_eq!(v[1], "10.0.0.1:5353".parse().unwrap());
    }

    #[test]
    fn query_roundtrip_a() {
        let q = encode_query("dl.test", QTYPE_A);
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        let r = encode_reply(&q, ip).unwrap();
        assert_eq!(decode_answer(&r, QTYPE_A), Some(ip));
    }
}
