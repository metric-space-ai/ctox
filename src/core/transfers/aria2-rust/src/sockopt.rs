//! C++ `--socket-recv-buffer-size` (SocketCore SO_RCVBUF). Default 0 = leave kernel default.
#![forbid(unsafe_code)]

use crate::error::{Error, Result};
use crate::options::OptionSet;
use rustix::net::sockopt::{
    ip_tos, ipv6_tclass, set_ip_tos, set_ipv6_tclass, set_socket_recv_buffer_size, set_tcp_nodelay,
    socket_recv_buffer_size, tcp_nodelay,
};
#[cfg(any(target_os = "linux", target_os = "android", target_os = "fuchsia"))]
use rustix::net::sockopt::{set_tcp_quickack, tcp_quickack};
use std::os::fd::AsFd;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static LAST_WANT: AtomicU64 = AtomicU64::new(0);
static LAST_GOT: AtomicU64 = AtomicU64::new(0);
static LAST_DSCP_TOS: AtomicU64 = AtomicU64::new(0);
static LAST_NODELAY: AtomicU64 = AtomicU64::new(0);
static LAST_QUICKACK: AtomicU64 = AtomicU64::new(0);
static LAST_WRITEV: AtomicU64 = AtomicU64::new(0);
static LAST_RECV: AtomicU64 = AtomicU64::new(0);
static LAST_SEND: AtomicU64 = AtomicU64::new(0);

pub fn last_got() -> u64 {
    LAST_GOT.load(Ordering::SeqCst)
}

pub fn last_dscp_tos() -> u64 {
    LAST_DSCP_TOS.load(Ordering::SeqCst)
}

/// C++ SocketCore::setTcpNodelay: always on after connect.
pub fn last_nodelay() -> bool {
    LAST_NODELAY.load(Ordering::SeqCst) == 1
}

/// C++ SocketCore::setTcpQuickAck (Linux TCP_QUICKACK; kernel may clear after one ACK).
pub fn last_quickack() -> bool {
    LAST_QUICKACK.load(Ordering::SeqCst) == 1
}

/// C++ SocketCore::writeVector syscalls (SocketBuffer::send).
pub fn last_writev() -> u64 {
    LAST_WRITEV.load(Ordering::SeqCst)
}

pub fn reset_writev() {
    LAST_WRITEV.store(0, Ordering::SeqCst);
}

/// C++ SocketCore::readData recv() syscalls.
pub fn last_recv() -> u64 {
    LAST_RECV.load(Ordering::SeqCst)
}

pub fn reset_recv() {
    LAST_RECV.store(0, Ordering::SeqCst);
}

/// C++ SocketCore::writeData send() syscalls.
pub fn last_send() -> u64 {
    LAST_SEND.load(Ordering::SeqCst)
}

pub fn reset_send() {
    LAST_SEND.store(0, Ordering::SeqCst);
}

pub fn reset_last() {
    LAST_WANT.store(0, Ordering::SeqCst);
    LAST_GOT.store(0, Ordering::SeqCst);
    LAST_DSCP_TOS.store(0, Ordering::SeqCst);
    LAST_NODELAY.store(0, Ordering::SeqCst);
    LAST_QUICKACK.store(0, Ordering::SeqCst);
    LAST_WRITEV.store(0, Ordering::SeqCst);
    LAST_RECV.store(0, Ordering::SeqCst);
    LAST_SEND.store(0, Ordering::SeqCst);
}

/// C++ SocketCore::setTcpNodelay (always true after connect).
pub fn apply_tcp_nodelay(fd: impl AsFd) -> Result<bool> {
    set_tcp_nodelay(&fd, true).map_err(|e| Error::Other(format!("TCP_NODELAY: {e}")))?;
    let got = tcp_nodelay(&fd).unwrap_or(true);
    LAST_NODELAY.store(u64::from(got), Ordering::SeqCst);
    Ok(got)
}

/// C++ SocketCore::setTcpQuickAck after recv (disables delayed ACK for the next segment).
pub fn apply_tcp_quickack(fd: impl AsFd) -> Result<bool> {
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "fuchsia"))]
    {
        set_tcp_quickack(&fd, true).map_err(|e| Error::Other(format!("TCP_QUICKACK: {e}")))?;
        let got = tcp_quickack(&fd).unwrap_or(true);
        LAST_QUICKACK.store(u64::from(got), Ordering::SeqCst);
        Ok(got)
    }
    #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "fuchsia")))]
    {
        let _ = fd;
        // QUICKACK is an optional Linux-family optimization, not a portable socket option.
        LAST_QUICKACK.store(0, Ordering::SeqCst);
        Ok(false)
    }
}

/// Darwin has SO_NOSIGPIPE instead of MSG_NOSIGNAL. Configure the socket before
/// writing so a closed peer returns an error rather than a process-wide signal.
fn send_flags(fd: impl AsFd) -> Result<rustix::net::SendFlags> {
    #[cfg(target_vendor = "apple")]
    {
        rustix::net::sockopt::set_socket_nosigpipe(fd, true)
            .map_err(|e| Error::Other(format!("SO_NOSIGPIPE: {e}")))?;
        Ok(rustix::net::SendFlags::empty())
    }
    #[cfg(not(target_vendor = "apple"))]
    {
        let _ = fd;
        Ok(rustix::net::SendFlags::NOSIGNAL)
    }
}

pub fn apply_recv_buffer(fd: impl AsFd, opts: &OptionSet) -> Result<usize> {
    let n =
        crate::storage::parse_size(opts.get("socket-recv-buffer-size").unwrap_or("0")).unwrap_or(0);
    if n == 0 {
        return Ok(0);
    }
    set_socket_recv_buffer_size(&fd, n as usize)
        .map_err(|e| Error::Other(format!("SO_RCVBUF set {n}: {e}")))?;
    let got = socket_recv_buffer_size(&fd).unwrap_or(n as usize);
    LAST_WANT.store(n, Ordering::SeqCst);
    LAST_GOT.store(got as u64, Ordering::SeqCst);
    Ok(got)
}

/// C++ `--dscp` (BitTorrent IP TOS: DSCP << 2). Default 0 = leave TOS alone.
pub fn apply_dscp(fd: impl AsFd, opts: &OptionSet) -> Result<u8> {
    let dscp = opts.u64("dscp", 0).min(63) as u8;
    if dscp == 0 {
        return Ok(0);
    }
    let tos = dscp << 2;
    match set_ip_tos(&fd, tos) {
        Ok(()) => {
            let got = ip_tos(&fd).unwrap_or(tos);
            LAST_DSCP_TOS.store(got as u64, Ordering::SeqCst);
            Ok(got)
        }
        Err(_) => {
            set_ipv6_tclass(&fd, u32::from(tos))
                .map_err(|e| Error::Other(format!("IPV6_TCLASS DSCP {dscp}: {e}")))?;
            let got = ipv6_tclass(&fd).unwrap_or(u32::from(tos)) as u8;
            LAST_DSCP_TOS.store(got as u64, Ordering::SeqCst);
            Ok(got)
        }
    }
}

/// C++ SocketCore::writeVector / SocketBuffer::send: writev of BufferEntry iovecs
/// (epoll-ET: ready + try_io WRITABLE, same as writeData send).
pub async fn writev_all(stream: &tokio::net::TcpStream, bufs: &[&[u8]]) -> Result<()> {
    use tokio::io::Interest;
    if bufs.is_empty() {
        return Ok(());
    }
    let _ = send_flags(stream)?;
    let mut i = 0usize;
    let mut skip = 0usize;
    loop {
        while i < bufs.len() && skip >= bufs[i].len() {
            i += 1;
            skip = 0;
        }
        if i >= bufs.len() {
            return Ok(());
        }
        let mut iov: Vec<std::io::IoSlice<'_>> = Vec::with_capacity(bufs.len() - i);
        iov.push(std::io::IoSlice::new(&bufs[i][skip..]));
        for b in &bufs[i + 1..] {
            iov.push(std::io::IoSlice::new(b));
        }
        stream
            .ready(Interest::WRITABLE)
            .await
            .map_err(|e| Error::Other(e.to_string()))?;
        let r = stream.try_io(Interest::WRITABLE, || {
            match rustix::io::retry_on_intr(|| rustix::io::writev(stream, &iov)) {
                Ok(n) => Ok(n),
                Err(e) if e == rustix::io::Errno::AGAIN || e == rustix::io::Errno::WOULDBLOCK => {
                    Err(std::io::Error::from(std::io::ErrorKind::WouldBlock))
                }
                Err(e) => Err(std::io::Error::from(e)),
            }
        });
        match r {
            Ok(0) => return Err(Error::Other("writev eof".into())),
            Ok(n) => {
                LAST_WRITEV.fetch_add(1, Ordering::Relaxed);
                let mut left = n;
                let rem = bufs[i].len() - skip;
                if left < rem {
                    skip += left;
                    continue;
                }
                left -= rem;
                i += 1;
                while i < bufs.len() && left >= bufs[i].len() {
                    left -= bufs[i].len();
                    i += 1;
                }
                skip = left;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(e) => return Err(Error::Other(format!("writev: {e}"))),
        }
    }
}

/// C++ SocketCore::readData: `recv()` into the caller buffer; TCP_QUICKACK after.
/// Idle timeout is armed only on WouldBlock (C++ epoll wait), not on every filled window.
pub async fn recv_some(stream: &tokio::net::TcpStream, buf: &mut [u8]) -> Result<usize> {
    recv_some_idle(stream, buf, None).await
}

pub async fn recv_some_idle(
    stream: &tokio::net::TcpStream,
    buf: &mut [u8],
    idle: Option<Duration>,
) -> Result<usize> {
    use tokio::io::Interest;
    if buf.is_empty() {
        return Ok(0);
    }
    loop {
        // Try the syscall first: after a partial window the socket is often still
        // readable, so skip an extra poll_ready. WouldBlock falls back to ready().
        let r = stream.try_io(Interest::READABLE, || {
            match rustix::io::retry_on_intr(|| {
                rustix::net::recv(stream, &mut *buf, rustix::net::RecvFlags::empty())
            }) {
                Ok(v) => Ok(v),
                Err(e) if e == rustix::io::Errno::AGAIN || e == rustix::io::Errno::WOULDBLOCK => {
                    Err(std::io::Error::from(std::io::ErrorKind::WouldBlock))
                }
                Err(e) => Err(std::io::Error::from(e)),
            }
        });
        match r {
            Ok((_, n)) => {
                LAST_RECV.fetch_add(1, Ordering::Relaxed);
                let _ = apply_tcp_quickack(stream);
                return Ok(n);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                let ready = stream.ready(Interest::READABLE);
                if let Some(d) = idle {
                    match tokio::time::timeout(d, ready).await {
                        Ok(Ok(_)) => {}
                        Ok(Err(e)) => return Err(Error::Other(e.to_string())),
                        Err(_) => return Err(Error::Http("timeout".into())),
                    }
                } else {
                    ready.await.map_err(|e| Error::Other(e.to_string()))?;
                }
            }
            Err(e) => return Err(Error::Other(format!("recv: {e}"))),
        }
    }
}

/// C++ SocketCore::readData loop: fill `buf` via `recv()` (EOF = 0 on first).
pub async fn recv_exact(stream: &tokio::net::TcpStream, buf: &mut [u8]) -> Result<usize> {
    if buf.is_empty() {
        return Ok(0);
    }
    let n0 = recv_some(stream, buf).await?;
    if n0 == 0 {
        return Ok(0);
    }
    let mut n = n0;
    while n < buf.len() {
        let got = recv_some(stream, &mut buf[n..]).await?;
        if got == 0 {
            return Err(Error::Other("recv eof".into()));
        }
        n += got;
    }
    Ok(n)
}

/// C++ SocketCore::writeData: `send(MSG_NOSIGNAL)` of the caller buffer (ET-safe try_io WRITABLE).
pub async fn send_some(stream: &tokio::net::TcpStream, buf: &[u8]) -> Result<usize> {
    use tokio::io::Interest;
    if buf.is_empty() {
        return Ok(0);
    }
    let flags = send_flags(stream)?;
    loop {
        stream
            .ready(Interest::WRITABLE)
            .await
            .map_err(|e| Error::Other(e.to_string()))?;
        let r = stream.try_io(Interest::WRITABLE, || {
            match rustix::io::retry_on_intr(|| rustix::net::send(stream, buf, flags)) {
                Ok(n) => Ok(n),
                Err(e) if e == rustix::io::Errno::AGAIN || e == rustix::io::Errno::WOULDBLOCK => {
                    Err(std::io::Error::from(std::io::ErrorKind::WouldBlock))
                }
                Err(e) => Err(std::io::Error::from(e)),
            }
        });
        match r {
            Ok(n) => {
                LAST_SEND.fetch_add(1, Ordering::Relaxed);
                return Ok(n);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(e) => return Err(Error::Other(format!("send: {e}"))),
        }
    }
}

/// C++ SocketCore::writeData loop: drain `buf` via `send()`.
pub async fn send_all(stream: &tokio::net::TcpStream, mut buf: &[u8]) -> Result<()> {
    while !buf.is_empty() {
        let n = send_some(stream, buf).await?;
        if n == 0 {
            return Err(Error::Other("send eof".into()));
        }
        buf = &buf[n..];
    }
    Ok(())
}

/// C++ SocketCore UDP writeData: `send(MSG_NOSIGNAL)` on a connected datagram socket.
pub async fn udp_send(sock: &tokio::net::UdpSocket, buf: &[u8]) -> Result<usize> {
    use tokio::io::Interest;
    if buf.is_empty() {
        return Ok(0);
    }
    let flags = send_flags(sock)?;
    loop {
        sock.writable()
            .await
            .map_err(|e| Error::Other(e.to_string()))?;
        let r = sock.try_io(Interest::WRITABLE, || {
            match rustix::io::retry_on_intr(|| rustix::net::send(sock, buf, flags)) {
                Ok(n) => Ok(n),
                Err(e) if e == rustix::io::Errno::AGAIN || e == rustix::io::Errno::WOULDBLOCK => {
                    Err(std::io::Error::from(std::io::ErrorKind::WouldBlock))
                }
                Err(e) => Err(std::io::Error::from(e)),
            }
        });
        match r {
            Ok(n) => {
                LAST_SEND.fetch_add(1, Ordering::Relaxed);
                return Ok(n);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(e) => return Err(Error::Other(format!("udp send: {e}"))),
        }
    }
}

/// C++ SocketCore UDP readData: `recv()` on a connected datagram socket.
pub async fn udp_recv(sock: &tokio::net::UdpSocket, buf: &mut [u8]) -> Result<usize> {
    use tokio::io::Interest;
    if buf.is_empty() {
        return Ok(0);
    }
    loop {
        sock.readable()
            .await
            .map_err(|e| Error::Other(e.to_string()))?;
        let r = sock.try_io(Interest::READABLE, || {
            match rustix::io::retry_on_intr(|| {
                rustix::net::recv(sock, &mut *buf, rustix::net::RecvFlags::empty())
            }) {
                Ok(v) => Ok(v),
                Err(e) if e == rustix::io::Errno::AGAIN || e == rustix::io::Errno::WOULDBLOCK => {
                    Err(std::io::Error::from(std::io::ErrorKind::WouldBlock))
                }
                Err(e) => Err(std::io::Error::from(e)),
            }
        });
        match r {
            Ok((_, n)) => {
                LAST_RECV.fetch_add(1, Ordering::Relaxed);
                return Ok(n);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(e) => return Err(Error::Other(format!("udp recv: {e}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::{TcpListener, TcpStream};

    #[tokio::test]
    async fn set_recv_buffer_roundtrip() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let client = TcpStream::connect(addr).await.unwrap();
        let mut opts = OptionSet::new();
        opts.set("socket-recv-buffer-size", "65536");
        apply_recv_buffer(&client, &opts).unwrap();
        let got = last_got();
        assert!(got >= 65536, "SO_RCVBUF got {got} want >= 65536");
    }

    #[tokio::test]
    async fn set_dscp_tos_roundtrip() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let client = TcpStream::connect(addr).await.unwrap();
        let mut opts = OptionSet::new();
        opts.set("dscp", "46");
        apply_dscp(&client, &opts).unwrap();
        let got = last_dscp_tos();
        assert_eq!(got, 46 << 2, "IP_TOS must be DSCP<<2, got {got}");
    }

    #[tokio::test]
    async fn set_tcp_nodelay_roundtrip() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let client = TcpStream::connect(addr).await.unwrap();
        apply_tcp_nodelay(&client).unwrap();
        assert!(last_nodelay(), "C++ SocketCore TCP_NODELAY must be on");
        assert!(tcp_nodelay(&client).unwrap());
    }

    #[cfg(any(target_os = "linux", target_os = "android", target_os = "fuchsia"))]
    #[tokio::test]
    async fn set_tcp_quickack_roundtrip() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let client = TcpStream::connect(addr).await.unwrap();
        apply_tcp_quickack(&client).unwrap();
        assert!(last_quickack(), "C++ SocketCore TCP_QUICKACK must be on");
        assert!(tcp_quickack(&client).unwrap());
    }

    #[tokio::test]
    async fn writev_two_bufs_dest_match() {
        use tokio::io::AsyncReadExt;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut buf = Vec::new();
            s.read_to_end(&mut buf).await.unwrap();
            buf
        });
        let client = TcpStream::connect(addr).await.unwrap();
        reset_writev();
        writev_all(&client, &[b"AB", b"CD"]).await.unwrap();
        drop(client);
        let got = server.await.unwrap();
        assert_eq!(got, b"ABCD", "C++ SocketCore::writeVector dest-match");
        assert!(
            last_writev() >= 1,
            "C++ SocketBuffer::send must writev (ET try_io WRITABLE)"
        );
    }

    #[tokio::test]
    async fn recv_exact_dest_match() {
        use tokio::io::AsyncWriteExt;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            s.write_all(b"ABCDEFGH").await.unwrap();
        });
        let client = TcpStream::connect(addr).await.unwrap();
        reset_recv();
        let mut buf = [0u8; 8];
        let n = recv_exact(&client, &mut buf).await.unwrap();
        server.await.unwrap();
        assert_eq!(n, 8);
        assert_eq!(&buf, b"ABCDEFGH");
        assert!(last_recv() >= 1, "C++ SocketCore::readData must recv");
    }

    #[tokio::test]
    async fn send_all_dest_match() {
        use tokio::io::AsyncReadExt;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut buf = Vec::new();
            s.read_to_end(&mut buf).await.unwrap();
            buf
        });
        let client = TcpStream::connect(addr).await.unwrap();
        reset_send();
        send_all(&client, b"ABCDEFGH").await.unwrap();
        drop(client);
        let got = server.await.unwrap();
        assert_eq!(got, b"ABCDEFGH", "C++ SocketCore::writeData dest-match");
        assert!(last_send() >= 1, "C++ SocketCore::writeData must send");
    }

    #[tokio::test]
    async fn send_nosignal_closed_peer_err() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (s, _) = listener.accept().await.unwrap();
            drop(s);
        });
        let client = TcpStream::connect(addr).await.unwrap();
        server.await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let mut saw_err = false;
        for _ in 0..16 {
            if send_all(&client, &[0u8; 32 * 1024]).await.is_err() {
                saw_err = true;
                break;
            }
        }
        assert!(
            saw_err,
            "C++ SocketCore::writeData MSG_NOSIGNAL must return EPIPE/ECONNRESET, not SIGPIPE"
        );
    }
}
