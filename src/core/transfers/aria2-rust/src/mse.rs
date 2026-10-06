//! BitTorrent MSE/PE (message stream encryption). Safe DH-768 + RC4.
#![forbid(unsafe_code)]

use crate::error::{Error, Result};
use crate::options::OptionSet;
use num_bigint::BigUint;
use sha1::{Digest, Sha1};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
#[cfg(test)]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub const CRYPTO_PLAIN: u32 = 0x01;
pub const CRYPTO_RC4: u32 = 0x02;

/// RFC 2409 Group 1 (768-bit) prime used by MSE.
const P_HEX: &str = concat!(
    "FFFFFFFFFFFFFFFFC90FDAA22168C234C4C6628B80DC1CD1",
    "29024E088A67CC74020BBEA63B139B22514A08798E3404DD",
    "EF9519B3CD3A431B302B0A6DF25F14374FE1356D6D51C245",
    "E485B576625E7EC6F44C42E9A63A36210000000000090563"
);

fn p() -> BigUint {
    BigUint::parse_bytes(P_HEX.as_bytes(), 16).expect("MSE P")
}

pub fn want_mse(opts: &OptionSet) -> bool {
    opts.bool("bt-force-encryption", false) || opts.bool("bt-require-crypto", false)
}

pub fn crypto_provide(opts: &OptionSet) -> u32 {
    if opts.bool("bt-force-encryption", false) {
        return CRYPTO_RC4;
    }
    match opts.get("bt-min-crypto-level").unwrap_or("plain") {
        "arc4" => CRYPTO_RC4,
        _ => CRYPTO_PLAIN | CRYPTO_RC4,
    }
}

pub struct Rc4 {
    s: [u8; 256],
    i: u8,
    j: u8,
}

impl Rc4 {
    pub fn new(key: &[u8]) -> Self {
        let mut s = [0u8; 256];
        for (i, b) in s.iter_mut().enumerate() {
            *b = i as u8;
        }
        let mut j = 0u8;
        for i in 0..256 {
            j = j
                .wrapping_add(s[i])
                .wrapping_add(key[i % key.len()]);
            s.swap(i, j as usize);
        }
        Self { s, i: 0, j: 0 }
    }

    pub fn apply(&mut self, data: &mut [u8]) {
        for b in data {
            self.i = self.i.wrapping_add(1);
            self.j = self.j.wrapping_add(self.s[self.i as usize]);
            self.s.swap(self.i as usize, self.j as usize);
            let k = self.s[self.s[self.i as usize].wrapping_add(self.s[self.j as usize]) as usize];
            *b ^= k;
        }
    }

    pub fn skip(&mut self, n: usize) {
        let mut tmp = vec![0u8; n.min(1024)];
        let mut left = n;
        while left > 0 {
            let c = left.min(tmp.len());
            self.apply(&mut tmp[..c]);
            left -= c;
        }
    }
}

fn sha1_parts(parts: &[&[u8]]) -> [u8; 20] {
    let mut h = Sha1::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

fn dh_pub(priv_be: &[u8]) -> [u8; 96] {
    let g = BigUint::from(2u32);
    let xa = BigUint::from_bytes_be(priv_be);
    let ya = g.modpow(&xa, &p());
    let b = ya.to_bytes_be();
    let mut out = [0u8; 96];
    out[96 - b.len()..].copy_from_slice(&b);
    out
}

fn dh_secret(priv_be: &[u8], peer_pub: &[u8; 96]) -> [u8; 96] {
    let xa = BigUint::from_bytes_be(priv_be);
    let yb = BigUint::from_bytes_be(peer_pub);
    let s = yb.modpow(&xa, &p());
    let b = s.to_bytes_be();
    let mut out = [0u8; 96];
    out[96 - b.len()..].copy_from_slice(&b);
    out
}

fn rand20() -> [u8; 20] {
    use rand::RngCore;
    let mut b = [0u8; 20];
    rand::rng().fill_bytes(&mut b);
    b
}

pub struct MseStream {
    inner: TcpStream,
    enc: Option<Rc4>,
    dec: Option<Rc4>,
    pending: Vec<u8>,
    poff: usize,
    unread: Vec<u8>,
}

impl MseStream {
    fn new(inner: TcpStream, enc: Option<Rc4>, dec: Option<Rc4>, unread: Vec<u8>) -> Self {
        Self {
            inner,
            enc,
            dec,
            pending: Vec::new(),
            poff: 0,
            unread,
        }
    }

    fn drain_unread(&mut self, buf: &mut [u8]) -> usize {
        if self.unread.is_empty() {
            return 0;
        }
        let n = self.unread.len().min(buf.len());
        buf[..n].copy_from_slice(&self.unread[..n]);
        self.unread.drain(..n);
        n
    }

    /// C++ ARC4Encryptor then SocketCore::writeData `send()`.
    pub async fn send_plain(&mut self, buf: &[u8]) -> Result<()> {
        if buf.is_empty() {
            return Ok(());
        }
        if let Some(enc) = self.enc.as_mut() {
            let mut encd = buf.to_vec();
            enc.apply(&mut encd);
            crate::sockopt::send_all(&self.inner, &encd).await
        } else {
            crate::sockopt::send_all(&self.inner, buf).await
        }
    }

    /// C++ SocketCore::readData `recv()` then ARC4Decryptor.
    pub async fn recv_plain(&mut self, buf: &mut [u8]) -> Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let n = self.drain_unread(buf);
        if n == buf.len() {
            return Ok(n);
        }
        let got = crate::sockopt::recv_some(&self.inner, &mut buf[n..]).await?;
        if got > 0 {
            if let Some(dec) = self.dec.as_mut() {
                dec.apply(&mut buf[n..n + got]);
            }
        }
        Ok(n + got)
    }

    pub async fn recv_exact_plain(&mut self, buf: &mut [u8]) -> Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let n0 = self.recv_plain(buf).await?;
        if n0 == 0 {
            return Ok(0);
        }
        let mut n = n0;
        while n < buf.len() {
            let got = self.recv_plain(&mut buf[n..]).await?;
            if got == 0 {
                return Err(Error::Bt("mse: recv eof".into()));
            }
            n += got;
        }
        Ok(n)
    }
}

impl AsyncRead for MseStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if !self.unread.is_empty() {
            let n = self.unread.len().min(buf.remaining());
            buf.put_slice(&self.unread[..n]);
            self.unread.drain(..n);
            return Poll::Ready(Ok(()));
        }
        let filled0 = buf.filled().len();
        match Pin::new(&mut self.inner).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                if let Some(dec) = self.dec.as_mut() {
                    let filled = buf.filled_mut();
                    dec.apply(&mut filled[filled0..]);
                }
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

impl AsyncWrite for MseStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.enc.is_none() {
            return Pin::new(&mut self.inner).poll_write(cx, buf);
        }
        let this = self.get_mut();
        while this.poff < this.pending.len() {
            let rest = &this.pending[this.poff..];
            match Pin::new(&mut this.inner).poll_write(cx, rest) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Ready(Ok(0)) => {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "mse write",
                    )));
                }
                Poll::Ready(Ok(n)) => {
                    this.poff += n;
                }
            }
        }
        this.pending.clear();
        this.poff = 0;
        let mut encd = buf.to_vec();
        if let Some(enc) = this.enc.as_mut() {
            enc.apply(&mut encd);
        }
        this.pending = encd;
        while this.poff < this.pending.len() {
            let rest = &this.pending[this.poff..];
            match Pin::new(&mut this.inner).poll_write(cx, rest) {
                Poll::Pending => return Poll::Ready(Ok(buf.len())),
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Ready(Ok(0)) => {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "mse write",
                    )));
                }
                Poll::Ready(Ok(n)) => {
                    this.poff += n;
                }
            }
        }
        this.pending.clear();
        this.poff = 0;
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.as_mut().get_mut();
        while this.poff < this.pending.len() {
            let rest = &this.pending[this.poff..];
            match Pin::new(&mut this.inner).poll_write(cx, rest) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Ready(Ok(0)) => {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "mse flush",
                    )));
                }
                Poll::Ready(Ok(n)) => {
                    this.poff += n;
                }
            }
        }
        this.pending.clear();
        this.poff = 0;
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

fn pick_select(provide: u32, allow: u32) -> Result<u32> {
    let both = provide & allow;
    if both & CRYPTO_RC4 != 0 {
        return Ok(CRYPTO_RC4);
    }
    if both & CRYPTO_PLAIN != 0 {
        return Ok(CRYPTO_PLAIN);
    }
    Err(Error::Bt("mse: no common crypto".into()))
}

/// Initiator (leecher) MSE handshake. Subsequent BT bytes go through the returned stream.
pub async fn initiator(s: TcpStream, info_hash: &[u8; 20], opts: &OptionSet) -> Result<MseStream> {
    let xa = rand20();
    let ya = dh_pub(&xa);
    crate::sockopt::send_all(&s, &ya).await?;
    let mut yb = [0u8; 96];
    let n = tokio::time::timeout(
        Duration::from_secs(opts.u64("timeout", 60).max(5)),
        crate::sockopt::recv_exact(&s, &mut yb),
    )
    .await
    .map_err(|_| Error::Bt("mse: yb timeout".into()))??;
    if n != yb.len() {
        return Err(Error::Bt("mse: yb eof".into()));
    }
    let secret = dh_secret(&xa, &yb);
    let req1 = sha1_parts(&[b"req1", &secret]);
    let req2 = sha1_parts(&[b"req2", info_hash]);
    let req3 = sha1_parts(&[b"req3", &secret]);
    let mut xor = req2;
    for (a, b) in xor.iter_mut().zip(req3.iter()) {
        *a ^= *b;
    }
    let key_a = sha1_parts(&[b"keyA", &secret, info_hash]);
    let key_b = sha1_parts(&[b"keyB", &secret, info_hash]);
    let mut enc = Rc4::new(&key_a);
    enc.skip(1024);
    let provide = crypto_provide(opts);
    let mut body = Vec::with_capacity(14);
    body.extend_from_slice(&[0u8; 8]);
    body.extend_from_slice(&provide.to_be_bytes());
    body.extend_from_slice(&0u16.to_be_bytes());
    body.extend_from_slice(&0u16.to_be_bytes());
    enc.apply(&mut body);
    let mut pkt = Vec::with_capacity(40 + body.len());
    pkt.extend_from_slice(&req1);
    pkt.extend_from_slice(&xor);
    pkt.extend_from_slice(&body);
    crate::sockopt::send_all(&s, &pkt).await?;

    let mut acc = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(opts.u64("timeout", 60).max(5));
    let off = loop {
        if tokio::time::Instant::now() > deadline {
            return Err(Error::Bt("mse: vc timeout".into()));
        }
        let mut tmp = [0u8; 256];
        let n = tokio::time::timeout(Duration::from_secs(2), crate::sockopt::recv_some(&s, &mut tmp))
            .await
            .map_err(|_| Error::Bt("mse: vc read timeout".into()))??;
        if n == 0 {
            return Err(Error::Bt("mse: eof before vc".into()));
        }
        acc.extend_from_slice(&tmp[..n]);
        if let Some(found) = find_vc(&acc, &key_b) {
            break found;
        }
        if acc.len() > 512 + 64 {
            return Err(Error::Bt("mse: vc not found".into()));
        }
    };
    let mut dec = Rc4::new(&key_b);
    dec.skip(1024);
    let mut rest = acc[off..].to_vec();
    while rest.len() < 14 {
        let mut tmp = [0u8; 64];
        let n = crate::sockopt::recv_some(&s, &mut tmp).await?;
        if n == 0 {
            return Err(Error::Bt("mse: eof select".into()));
        }
        rest.extend_from_slice(&tmp[..n]);
    }
    dec.apply(&mut rest);
    if rest.len() < 14 || rest[..8] != [0u8; 8] {
        return Err(Error::Bt("mse: bad vc".into()));
    }
    let select = u32::from_be_bytes(rest[8..12].try_into().unwrap());
    if select != CRYPTO_PLAIN && select != CRYPTO_RC4 {
        return Err(Error::Bt(format!("mse: bad select {select}")));
    }
    if provide & select == 0 {
        return Err(Error::Bt("mse: select not provided".into()));
    }
    let pad_d = u16::from_be_bytes(rest[12..14].try_into().unwrap()) as usize;
    let take = 14 + pad_d;
    while rest.len() < take {
        // already decrypted `rest`; extra wire bytes still ciphertext
        return Err(Error::Bt("mse: short padD".into()));
    }
    let leftover = rest[take..].to_vec();
    let (enc, dec, unread) = if select == CRYPTO_RC4 {
        (Some(enc), Some(dec), leftover)
    } else {
        (None, None, leftover)
    };
    Ok(MseStream::new(s, enc, dec, unread))
}

fn find_vc(buf: &[u8], key_b: &[u8]) -> Option<usize> {
    let max = buf.len().saturating_sub(14).min(512);
    for off in 0..=max {
        let mut rc4 = Rc4::new(key_b);
        rc4.skip(1024);
        let mut probe = [0u8; 8];
        probe.copy_from_slice(&buf[off..off + 8]);
        rc4.apply(&mut probe);
        if probe == [0u8; 8] {
            return Some(off);
        }
    }
    None
}

/// Responder MSE handshake (seeder). `allow` is CRYPTO_RC4 and/or CRYPTO_PLAIN.
pub async fn responder(s: TcpStream, info_hash: &[u8; 20], allow: u32) -> Result<MseStream> {
    let mut ya = [0u8; 96];
    let n = tokio::time::timeout(
        Duration::from_secs(10),
        crate::sockopt::recv_exact(&s, &mut ya),
    )
    .await
    .map_err(|_| Error::Bt("mse: ya timeout".into()))??;
    if n != ya.len() {
        return Err(Error::Bt("mse: ya eof".into()));
    }
    let xb = rand20();
    let yb = dh_pub(&xb);
    crate::sockopt::send_all(&s, &yb).await?;
    let secret = dh_secret(&xb, &ya);
    let req1 = sha1_parts(&[b"req1", &secret]);
    let req2 = sha1_parts(&[b"req2", info_hash]);
    let req3 = sha1_parts(&[b"req3", &secret]);
    let mut expect_xor = req2;
    for (a, b) in expect_xor.iter_mut().zip(req3.iter()) {
        *a ^= *b;
    }
    let mut acc = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let pos = loop {
        if tokio::time::Instant::now() > deadline {
            return Err(Error::Bt("mse: req1 timeout".into()));
        }
        let mut tmp = [0u8; 256];
        let n = tokio::time::timeout(Duration::from_secs(2), crate::sockopt::recv_some(&s, &mut tmp))
            .await
            .map_err(|_| Error::Bt("mse: req1 read timeout".into()))??;
        if n == 0 {
            return Err(Error::Bt("mse: eof before req1".into()));
        }
        acc.extend_from_slice(&tmp[..n]);
        if let Some(p) = acc.windows(20).position(|w| w == req1) {
            break p;
        }
        if acc.len() > 512 + 40 {
            return Err(Error::Bt("mse: req1 not found".into()));
        }
    };
    acc.drain(..pos + 20);
    while acc.len() < 20 {
        let mut tmp = [0u8; 64];
        let n = crate::sockopt::recv_some(&s, &mut tmp).await?;
        if n == 0 {
            return Err(Error::Bt("mse: eof xor".into()));
        }
        acc.extend_from_slice(&tmp[..n]);
    }
    if acc[..20] != expect_xor {
        return Err(Error::Bt("mse: skey mismatch".into()));
    }
    acc.drain(..20);
    let key_a = sha1_parts(&[b"keyA", &secret, info_hash]);
    let key_b = sha1_parts(&[b"keyB", &secret, info_hash]);
    let mut dec = Rc4::new(&key_a);
    dec.skip(1024);
    let mut enc = Rc4::new(&key_b);
    enc.skip(1024);
    while acc.len() < 14 {
        let mut tmp = [0u8; 64];
        let n = crate::sockopt::recv_some(&s, &mut tmp).await?;
        if n == 0 {
            return Err(Error::Bt("mse: eof provide".into()));
        }
        acc.extend_from_slice(&tmp[..n]);
    }
    // decrypt only first 14, then more as needed
    let mut head = acc[..14].to_vec();
    let mut tail = acc[14..].to_vec();
    dec.apply(&mut head);
    if head[..8] != [0u8; 8] {
        return Err(Error::Bt("mse: bad initiator vc".into()));
    }
    let provide = u32::from_be_bytes(head[8..12].try_into().unwrap());
    let pad_c = u16::from_be_bytes(head[12..14].try_into().unwrap()) as usize;
    let need_more = pad_c + 2;
    while tail.len() < need_more {
        let mut tmp = [0u8; 64];
        let n = crate::sockopt::recv_some(&s, &mut tmp).await?;
        if n == 0 {
            return Err(Error::Bt("mse: eof padC".into()));
        }
        tail.extend_from_slice(&tmp[..n]);
    }
    dec.apply(&mut tail[..need_more]);
    let ia_len = u16::from_be_bytes(tail[pad_c..pad_c + 2].try_into().unwrap()) as usize;
    let total_tail = pad_c + 2 + ia_len;
    while tail.len() < total_tail {
        let mut tmp = [0u8; 64];
        let n = crate::sockopt::recv_some(&s, &mut tmp).await?;
        if n == 0 {
            return Err(Error::Bt("mse: eof IA".into()));
        }
        tail.extend_from_slice(&tmp[..n]);
    }
    if total_tail > need_more {
        dec.apply(&mut tail[need_more..total_tail]);
    }
    let leftover = if tail.len() > total_tail {
        let mut extra = tail[total_tail..].to_vec();
        dec.apply(&mut extra);
        extra
    } else {
        Vec::new()
    };
    let select = pick_select(provide, allow)?;
    let mut reply = Vec::with_capacity(14);
    reply.extend_from_slice(&[0u8; 8]);
    reply.extend_from_slice(&select.to_be_bytes());
    reply.extend_from_slice(&0u16.to_be_bytes());
    enc.apply(&mut reply);
    crate::sockopt::send_all(&s, &reply).await?;
    let (enc, dec, unread) = if select == CRYPTO_RC4 {
        (Some(enc), Some(dec), leftover)
    } else {
        (None, None, leftover)
    };
    Ok(MseStream::new(s, enc, dec, unread))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rc4_known_vector() {
        // RC4 key "Key", plaintext "Plaintext" → 0xBBF316E8D940AF0AD3
        let mut r = Rc4::new(b"Key");
        let mut p = b"Plaintext".to_vec();
        r.apply(&mut p);
        assert_eq!(p, hex::decode("bbf316e8d940af0ad3").unwrap());
    }

    #[test]
    fn provide_force_is_rc4_only() {
        let mut o = OptionSet::new();
        o.set("bt-force-encryption", "true");
        assert_eq!(crypto_provide(&o), CRYPTO_RC4);
        assert!(want_mse(&o));
    }

    #[test]
    fn provide_arc4_level() {
        let mut o = OptionSet::new();
        o.set("bt-min-crypto-level", "arc4");
        assert_eq!(crypto_provide(&o), CRYPTO_RC4);
        assert!(!want_mse(&o));
        o.set("bt-require-crypto", "true");
        assert!(want_mse(&o));
    }

    #[tokio::test]
    async fn mse_rc4_roundtrip_bytes() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let ih = [7u8; 20];
        let h = tokio::spawn(async move {
            let (s, _) = listener.accept().await.unwrap();
            let mut r = responder(s, &ih, CRYPTO_RC4).await.unwrap();
            let mut buf = [0u8; 5];
            r.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"hello");
            r.write_all(b"world").await.unwrap();
            r.flush().await.unwrap();
        });
        let s = TcpStream::connect(addr).await.unwrap();
        let mut o = OptionSet::new();
        o.set("bt-require-crypto", "true");
        o.set("bt-min-crypto-level", "arc4");
        let mut i = initiator(s, &ih, &o).await.unwrap();
        i.write_all(b"hello").await.unwrap();
        i.flush().await.unwrap();
        let mut buf = [0u8; 5];
        i.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"world");
        h.await.unwrap();
    }
}
