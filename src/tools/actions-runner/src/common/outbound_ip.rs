//! The host's address towards the internet.
//!
//! act's runner has to tell a job where its artifact and cache servers are, and
//! the answer has to be an address the job can actually reach. The usual case
//! is inside a container, so the address is the *host's*, not the container's.
//!
//! Two strategies, in act's order:
//!
//! 1. Connect a UDP socket to a public address and read the local end. The
//!    kernel picks the source address it would use to leave, so this needs no
//!    traffic to actually be sent — `connect` on UDP only sets the peer.
//! 2. If there is no route at all, rank the interfaces and take the best
//!    candidate.
//!
//! **The single-candidate rule is upstream's and it is preserved:** act only
//! picks from the ranked list when there is *more than one* candidate. A
//! machine with exactly one global unicast address gets `None` rather than that
//! address, and the caller fails to start. The reasoning is that on a one-
//! address host the address is as likely to be a container bridge as a real
//! interface, and guessing wrong is worse than not starting.

use std::fmt;
use std::net::{IpAddr, UdpSocket};

/// Why the outbound address could not be determined.
///
/// Distinct from `HandlerError::NoOutboundIp`, which is what act reports when
/// the lookup *succeeds* and finds nothing. This is the lookup itself failing.
#[derive(Debug)]
pub enum OutboundIpError {
    /// The interface list could not be read.
    Interfaces(String),
}

impl fmt::Display for OutboundIpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Interfaces(message) => write!(f, "unable to list network interfaces: {message}"),
        }
    }
}

impl std::error::Error for OutboundIpError {}

///
/// Port of act's `common.GetOutboundIP`: a connected UDP socket reveals the
/// local address the kernel would use, which needs no traffic to actually be
/// sent. If there is no route, the global unicast addresses of the interfaces
/// are ranked — ethernet first, then IPv4, then by interface name, then by the
/// address itself — and the best one is returned. The one outbound rule that
/// differs is the single-address case: act requires *more* than one candidate
/// before it picks any, and so does this.
pub fn outbound_ip() -> Result<Option<String>, OutboundIpError> {
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                if let IpAddr::V4(ip) = addr.ip() {
                    if !ip.is_unspecified() {
                        return Ok(Some(ip.to_string()));
                    }
                }
            }
        }
    }

    let mut best: Vec<(String, IpAddr)> = Vec::new();
    for interface in
        if_addrs::get_if_addrs().map_err(|err| OutboundIpError::Interfaces(err.to_string()))?
    {
        let IpAddr::V4(ip) = interface.ip() else {
            continue;
        };
        // Go's `IP.IsGlobalUnicast`, narrowed the way act uses it: loopback,
        // link-local, unspecified, multicast and the broadcast address are
        // out. RFC 1918 stays in, because Go counts private space as global
        // unicast and act binds to whatever this returns.
        if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
            continue;
        }
        if ip.is_link_local() || ip.octets()[0] >= 224 || ip.octets()[0] == 0 {
            continue;
        }
        best.push((interface.name, IpAddr::V4(ip)));
    }
    if best.len() <= 1 {
        // act requires more than one candidate before it picks any, and a
        // single address is not worth guessing between.
        return Ok(None);
    }

    // Rank: an interface whose name starts with `e` (ethernet) beats the
    // rest, IPv4 beats IPv6, then the interface name, then the address.
    best.sort_by(|(a_name, a_ip), (b_name, b_ip)| {
        a_name
            .starts_with('e')
            .cmp(&b_name.starts_with('e'))
            .then(a_name.cmp(b_name))
            .then(a_ip.to_string().cmp(&b_ip.to_string()))
    });
    Ok(Some(best[0].1.to_string()))
}
