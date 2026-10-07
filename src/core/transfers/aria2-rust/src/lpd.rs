//! Local Peer Discovery (BEP 14): BT-SEARCH over UDP multicast/unicast.
#![forbid(unsafe_code)]

use crate::error::Result;
use crate::options::OptionSet;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::net::UdpSocket;

pub const LPD_MULTICAST: Ipv4Addr = Ipv4Addr::new(239, 192, 152, 143);
pub const LPD_PORT: u16 = 6771;

pub fn encode_announce(info_hash: &[u8; 20], port: u16, cookie: &str) -> Vec<u8> {
    format!(
        "BT-SEARCH * HTTP/1.1\r\nHost: {LPD_MULTICAST}:{LPD_PORT}\r\nPort: {port}\r\nInfohash: {}\r\nCookie: {cookie}\r\n\r\n",
        hex::encode(info_hash).to_uppercase()
    )
    .into_bytes()
}

pub fn parse_announce(buf: &[u8]) -> Option<([u8; 20], u16, Option<String>)> {
    let s = std::str::from_utf8(buf).ok()?;
    if !s.starts_with("BT-SEARCH") {
        return None;
    }
    let mut port = None;
    let mut hash = None;
    let mut cookie = None;
    for line in s.split(['\r', '\n']) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("Port:") {
            port = rest.trim().parse().ok();
        } else if let Some(rest) = line.strip_prefix("Infohash:") {
            let h = rest.trim();
            if h.len() == 40 {
                if let Ok(b) = hex::decode(h) {
                    if let Ok(arr) = <[u8; 20]>::try_from(b.as_slice()) {
                        hash = Some(arr);
                    }
                }
            }
        } else if let Some(rest) = line.strip_prefix("Cookie:") {
            cookie = Some(rest.trim().to_string());
        }
    }
    Some((hash?, port?, cookie))
}

fn bind_addr(opts: &OptionSet) -> SocketAddr {
    let raw = opts.get("bt-lpd-interface").unwrap_or("0.0.0.0");
    if let Ok(sa) = raw.parse::<SocketAddr>() {
        return sa;
    }
    if let Ok(ip) = raw.parse::<Ipv4Addr>() {
        return SocketAddr::from((ip, LPD_PORT));
    }
    SocketAddr::from(([0, 0, 0, 0], LPD_PORT))
}

/// Listen for a matching BT-SEARCH and return the announcer's TCP peer.
pub async fn discover(info_hash: [u8; 20], opts: &OptionSet) -> Result<Vec<SocketAddr>> {
    if !opts.bool("bt-enable-lpd", false) {
        return Ok(Vec::new());
    }
    let bind = bind_addr(opts);
    let sock = UdpSocket::bind(bind).await?;
    sock.set_broadcast(true).ok();
    if bind.ip().is_unspecified() || bind.ip() == std::net::IpAddr::V4(LPD_MULTICAST) {
        sock.join_multicast_v4(LPD_MULTICAST, Ipv4Addr::UNSPECIFIED)
            .ok();
    }
    let cookie = format!("aria2-rust-{}", hex::encode(&info_hash[..4]));
    let port = opts.u64("listen-port", 6881) as u16;
    let ann = encode_announce(&info_hash, port, &cookie);
    let mcast = SocketAddr::from((LPD_MULTICAST, LPD_PORT));
    let _ = sock.send_to(&ann, mcast).await;
    let wait = Duration::from_secs(opts.u64("timeout", 60).min(15).max(1));
    let deadline = tokio::time::Instant::now() + wait;
    let mut buf = [0u8; 1500];
    let mut out = Vec::new();
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            break;
        }
        match tokio::time::timeout(left, sock.recv_from(&mut buf)).await {
            Ok(Ok((n, from))) => {
                let Some((ih, p, ck)) = parse_announce(&buf[..n]) else {
                    continue;
                };
                if ih != info_hash || p == 0 {
                    continue;
                }
                if ck.as_deref() == Some(cookie.as_str()) {
                    continue;
                }
                let peer = SocketAddr::new(from.ip(), p);
                if !out.contains(&peer) {
                    out.push(peer);
                    break;
                }
            }
            _ => break,
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn announce_roundtrip() {
        let ih = [0xab; 20];
        let msg = encode_announce(&ih, 51413, "cookie1");
        let (got, port, ck) = parse_announce(&msg).unwrap();
        assert_eq!(got, ih);
        assert_eq!(port, 51413);
        assert_eq!(ck.as_deref(), Some("cookie1"));
    }
}
