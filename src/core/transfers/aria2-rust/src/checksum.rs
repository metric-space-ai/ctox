//! C++ `--checksum=TYPE=DIGEST` (HTTP(S)/FTP) + Metalink `<hash>` / `<pieces>`.
//! C++ HashFunc: sha-1, sha-224, sha-256, sha-384, sha-512, md5, adler32.
//! Stream dest, never slurp.
#![forbid(unsafe_code)]

use crate::error::{Error, Result};
use crate::options::OptionSet;
use crate::storage::FileStorage;
use md5::Md5;
use sha1::Sha1;
use sha2::{Sha224, Sha256, Sha384, Sha512};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static LAST_CHECK_PREAD: AtomicU64 = AtomicU64::new(0);
static LAST_HELD_CHECK: AtomicU64 = AtomicU64::new(0);
static LAST_OPEN_CHECK: AtomicU64 = AtomicU64::new(0);

pub fn last_check_pread() -> u64 {
    LAST_CHECK_PREAD.load(Ordering::SeqCst)
}

pub fn reset_check_pread() {
    LAST_CHECK_PREAD.store(0, Ordering::SeqCst);
}

pub fn last_held_check() -> u64 {
    LAST_HELD_CHECK.load(Ordering::SeqCst)
}

pub fn reset_held_check() {
    LAST_HELD_CHECK.store(0, Ordering::SeqCst);
}

pub fn last_open_check() -> u64 {
    LAST_OPEN_CHECK.load(Ordering::SeqCst)
}

pub fn reset_open_check() {
    LAST_OPEN_CHECK.store(0, Ordering::SeqCst);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Sha1,
    Sha224,
    Sha256,
    Sha384,
    Sha512,
    Md5,
    Adler32,
}

/// C++ HashFunc names (`MessageDigest::getSupportedHashTypes`).
pub(crate) fn canonicalize_hash_type(ty: &str) -> Option<&'static str> {
    Some(match ty.trim().to_ascii_lowercase().as_str() {
        "sha-1" | "sha1" => "sha-1",
        "sha-224" | "sha224" => "sha-224",
        "sha-256" | "sha256" => "sha-256",
        "sha-384" | "sha384" => "sha-384",
        "sha-512" | "sha512" => "sha-512",
        "md5" | "md-5" => "md5",
        "adler32" => "adler32",
        _ => return None,
    })
}

fn parse_kind(ty: &str) -> Result<Kind> {
    match canonicalize_hash_type(ty) {
        Some("sha-1") => Ok(Kind::Sha1),
        Some("sha-224") => Ok(Kind::Sha224),
        Some("sha-256") => Ok(Kind::Sha256),
        Some("sha-384") => Ok(Kind::Sha384),
        Some("sha-512") => Ok(Kind::Sha512),
        Some("md5") => Ok(Kind::Md5),
        Some("adler32") => Ok(Kind::Adler32),
        _ => Err(Error::Http(format!("checksum type {ty}"))),
    }
}

fn parse_spec(spec: &str) -> Result<(Kind, String)> {
    let (ty, dig) = spec
        .split_once('=')
        .ok_or_else(|| Error::Http("checksum TYPE=DIGEST".into()))?;
    let kind = parse_kind(ty)?;
    let dig = dig.trim().to_ascii_lowercase();
    if dig.is_empty() || !dig.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::Http("checksum digest".into()));
    }
    Ok((kind, dig))
}

pub async fn verify_dest(path: &Path, opts: &OptionSet) -> Result<()> {
    verify_piece_checksums(path, opts).await?;
    let Some(spec) = opts.get("checksum").filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    let (kind, want) = parse_spec(spec)?;
    let got = hash_file(path, kind)?;
    if got != want {
        return Err(Error::Http(format!(
            "checksum mismatch expected {want} got {got}"
        )));
    }
    Ok(())
}

/// C++ CheckIntegrityCommand: hash dest via still-open DiskWriter (no extra open).
pub fn verify_store(store: &FileStorage, opts: &OptionSet) -> Result<()> {
    verify_piece_held(store, opts)?;
    let Some(spec) = opts.get("checksum").filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    let (kind, want) = parse_spec(spec)?;
    let got = hash_file_held(store, kind)?;
    if got != want {
        return Err(Error::Http(format!(
            "checksum mismatch expected {want} got {got}"
        )));
    }
    LAST_HELD_CHECK.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// C++ `--realtime-chunk-checksum` (default true): Metalink `<pieces>` hashes.
pub async fn verify_piece_checksums(path: &Path, opts: &OptionSet) -> Result<()> {
    if !opts.bool("realtime-chunk-checksum", true) {
        return Ok(());
    }
    let Some(spec) = opts.get("piece-checksum").filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    let (ty, rest) = spec
        .split_once(':')
        .ok_or_else(|| Error::Http("piece-checksum TYPE:LEN:hex".into()))?;
    let (len_s, hashes) = rest
        .split_once(':')
        .ok_or_else(|| Error::Http("piece-checksum TYPE:LEN:hex".into()))?;
    let kind = parse_kind(ty)?;
    let plen: usize = len_s
        .parse()
        .map_err(|_| Error::Http("piece-checksum length".into()))?;
    if plen == 0 {
        return Err(Error::Http("piece-checksum length".into()));
    }
    let wants: Vec<&str> = hashes
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if wants.is_empty() {
        return Ok(());
    }
    let f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; plen];
    let mut off = 0u64;
    for (i, want) in wants.iter().enumerate() {
        let got = pread_fill(&f, &mut buf, off)?;
        if got == 0 {
            break;
        }
        off += got as u64;
        let mut h = Hasher::new(kind);
        h.update(&buf[..got]);
        let digest = h.finalize();
        if digest != *want {
            return Err(Error::Http(format!(
                "chunk checksum mismatch piece {i} expected {want} got {digest}"
            )));
        }
    }
    Ok(())
}

/// C++ DefaultDiskWriter::readDataInternal: positioned `pread` 16KiB.
fn pread_fill(f: &std::fs::File, buf: &mut [u8], mut off: u64) -> Result<usize> {
    let mut n = 0usize;
    while n < buf.len() {
        let r = rustix::io::retry_on_intr(|| rustix::io::pread(f, &mut buf[n..], off))
            .map_err(std::io::Error::from)?;
        if r == 0 {
            break;
        }
        LAST_CHECK_PREAD.fetch_add(1, Ordering::Relaxed);
        n += r;
        off += r as u64;
    }
    Ok(n)
}

fn hash_file(path: &Path, kind: Kind) -> Result<String> {
    LAST_OPEN_CHECK.fetch_add(1, Ordering::Relaxed);
    let f = std::fs::File::open(path)?;
    let mut buf = [0u8; 16 * 1024];
    let mut off = 0u64;
    let mut h = Hasher::new(kind);
    loop {
        let n = pread_fill(&f, &mut buf, off)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        off += n as u64;
    }
    Ok(h.finalize())
}

fn hash_file_held(store: &FileStorage, kind: Kind) -> Result<String> {
    let mut buf = [0u8; 16 * 1024];
    let mut off = 0u64;
    let mut h = Hasher::new(kind);
    loop {
        let Some(n) = store.pread_held(off, &mut buf)? else {
            return hash_file(store.path(), kind);
        };
        if n == 0 {
            break;
        }
        LAST_CHECK_PREAD.fetch_add(1, Ordering::Relaxed);
        h.update(&buf[..n]);
        off += n as u64;
    }
    Ok(h.finalize())
}

fn verify_piece_held(store: &FileStorage, opts: &OptionSet) -> Result<()> {
    if !opts.bool("realtime-chunk-checksum", true) {
        return Ok(());
    }
    let Some(spec) = opts.get("piece-checksum").filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    let (ty, rest) = spec
        .split_once(':')
        .ok_or_else(|| Error::Http("piece-checksum TYPE:LEN:hex".into()))?;
    let (len_s, hashes) = rest
        .split_once(':')
        .ok_or_else(|| Error::Http("piece-checksum TYPE:LEN:hex".into()))?;
    let kind = parse_kind(ty)?;
    let plen: usize = len_s
        .parse()
        .map_err(|_| Error::Http("piece-checksum length".into()))?;
    if plen == 0 {
        return Err(Error::Http("piece-checksum length".into()));
    }
    let wants: Vec<&str> = hashes
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if wants.is_empty() {
        return Ok(());
    }
    let mut buf = vec![0u8; plen];
    let mut off = 0u64;
    let mut f_open: Option<std::fs::File> = None;
    for (i, want) in wants.iter().enumerate() {
        let n = if let Some(f) = f_open.as_ref() {
            pread_fill(f, &mut buf, off)?
        } else {
            match store.pread_held(off, &mut buf)? {
                Some(n) => {
                    LAST_CHECK_PREAD.fetch_add(1, Ordering::Relaxed);
                    n
                }
                None => {
                    let f = std::fs::File::open(store.path())?;
                    let n = pread_fill(&f, &mut buf, off)?;
                    f_open = Some(f);
                    n
                }
            }
        };
        if n == 0 {
            break;
        }
        off += n as u64;
        let mut h = Hasher::new(kind);
        h.update(&buf[..n]);
        let digest = h.finalize();
        if digest != *want {
            return Err(Error::Http(format!(
                "chunk checksum mismatch piece {i} expected {want} got {digest}"
            )));
        }
    }
    Ok(())
}

/// zlib Adler-32 (C++ HashFunc `adler32` via zlib).
struct Adler32 {
    a: u32,
    b: u32,
}

impl Adler32 {
    fn new() -> Self {
        Self { a: 1, b: 0 }
    }
    fn update(&mut self, data: &[u8]) {
        const BASE: u64 = 65521;
        let mut a = self.a as u64;
        let mut b = self.b as u64;
        for &x in data {
            a += x as u64;
            b += a;
        }
        self.a = (a % BASE) as u32;
        self.b = (b % BASE) as u32;
    }
    fn finish(self) -> [u8; 4] {
        ((self.b << 16) | self.a).to_be_bytes()
    }
}

enum Hasher {
    Sha1(Sha1),
    Sha224(Sha224),
    Sha256(Sha256),
    Sha384(Sha384),
    Sha512(Sha512),
    Md5(Md5),
    Adler32(Adler32),
}

impl Hasher {
    fn new(kind: Kind) -> Self {
        match kind {
            Kind::Sha1 => Hasher::Sha1(Sha1::default()),
            Kind::Sha224 => Hasher::Sha224(Sha224::default()),
            Kind::Sha256 => Hasher::Sha256(Sha256::default()),
            Kind::Sha384 => Hasher::Sha384(Sha384::default()),
            Kind::Sha512 => Hasher::Sha512(Sha512::default()),
            Kind::Md5 => Hasher::Md5(Md5::default()),
            Kind::Adler32 => Hasher::Adler32(Adler32::new()),
        }
    }
    fn update(&mut self, data: &[u8]) {
        match self {
            Hasher::Sha1(h) => sha1::Digest::update(h, data),
            Hasher::Sha224(h) => sha2::Digest::update(h, data),
            Hasher::Sha256(h) => sha2::Digest::update(h, data),
            Hasher::Sha384(h) => sha2::Digest::update(h, data),
            Hasher::Sha512(h) => sha2::Digest::update(h, data),
            Hasher::Md5(h) => md5::Digest::update(h, data),
            Hasher::Adler32(h) => h.update(data),
        }
    }
    fn finalize(self) -> String {
        match self {
            Hasher::Sha1(h) => hex::encode(sha1::Digest::finalize(h)),
            Hasher::Sha224(h) => hex::encode(sha2::Digest::finalize(h)),
            Hasher::Sha256(h) => hex::encode(sha2::Digest::finalize(h)),
            Hasher::Sha384(h) => hex::encode(sha2::Digest::finalize(h)),
            Hasher::Sha512(h) => hex::encode(sha2::Digest::finalize(h)),
            Hasher::Md5(h) => hex::encode(md5::Digest::finalize(h)),
            Hasher::Adler32(h) => hex::encode(h.finish()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_all_hashfunc_types() {
        let (k, d) = parse_spec("sha-1=AbCd").unwrap();
        assert_eq!(k, Kind::Sha1);
        assert_eq!(d, "abcd");
        parse_spec("md5=00").unwrap();
        parse_spec("sha-256=00").unwrap();
        parse_spec("sha256=00").unwrap();
        parse_spec("sha-224=00").unwrap();
        parse_spec("sha-384=00").unwrap();
        parse_spec("sha-512=00").unwrap();
        parse_spec("adler32=024d0127").unwrap();
        assert!(parse_spec("sha-3=00").is_err());
        assert!(parse_spec("blake2=00").is_err());
    }

    #[test]
    fn canonicalize_cpp_aliases() {
        assert_eq!(canonicalize_hash_type("SHA256"), Some("sha-256"));
        assert_eq!(canonicalize_hash_type("sha512"), Some("sha-512"));
        assert_eq!(canonicalize_hash_type("md-5"), Some("md5"));
        assert_eq!(canonicalize_hash_type("ripe-md"), None);
    }

    #[test]
    fn hash_file_all_kinds_dest_match() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("abc.bin");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(
            hash_file(&p, Kind::Sha1).unwrap(),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hash_file(&p, Kind::Sha224).unwrap(),
            "23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7"
        );
        assert_eq!(
            hash_file(&p, Kind::Sha256).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hash_file(&p, Kind::Sha384).unwrap(),
            "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7"
        );
        assert_eq!(
            hash_file(&p, Kind::Sha512).unwrap(),
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
        assert_eq!(
            hash_file(&p, Kind::Md5).unwrap(),
            "900150983cd24fb0d6963f7d28e17f72"
        );
        assert_eq!(hash_file(&p, Kind::Adler32).unwrap(), "024d0127");
    }

    #[test]
    fn check_integrity_pread_16k_dest_match() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("big.bin");
        let body = vec![0xABu8; 20 * 1024];
        std::fs::write(&p, &body).unwrap();
        reset_check_pread();
        let got = hash_file(&p, Kind::Sha1).unwrap();
        let mut h = Hasher::new(Kind::Sha1);
        h.update(&body);
        assert_eq!(got, h.finalize());
        assert!(
            last_check_pread() >= 2,
            "C++ CheckIntegrityCommand reads dest with 16KiB pread"
        );
    }

    #[tokio::test]
    async fn check_integrity_held_fd_pread_dest_match() {
        use crate::storage::{AllocMode, FileStorage};
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("held.bin");
        let body = vec![0xABu8; 20 * 1024];
        let st = FileStorage::new(p.clone(), body.len() as u64, AllocMode::None);
        st.ensure().await.unwrap();
        st.write_at(0, &body).await.unwrap();
        st.flush().await.unwrap();
        reset_held_check();
        reset_check_pread();
        crate::storage::reset_try_pread();
        let mut h = Hasher::new(Kind::Sha1);
        h.update(&body);
        let dig = h.finalize();
        let mut opts = OptionSet::with_defaults();
        opts.set("checksum", format!("sha-1={dig}"));
        verify_store(&st, &opts).unwrap();
        assert_eq!(
            last_held_check(),
            1,
            "C++ CheckIntegrityCommand hashes still-open dest DiskWriter"
        );
        assert!(
            crate::storage::last_try_pread() >= 2,
            "held dest fd pread"
        );
        assert!(last_check_pread() >= 2);
        assert_eq!(std::fs::read(&p).unwrap(), body);
    }

    #[tokio::test]
    async fn piece_checksum_sha256_dest_match() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("p.bin");
        let body = b"0123456789abcdef";
        std::fs::write(&p, body).unwrap();
        let p0 = {
            let mut h = Hasher::new(Kind::Sha256);
            h.update(&body[..8]);
            h.finalize()
        };
        let p1 = {
            let mut h = Hasher::new(Kind::Sha256);
            h.update(&body[8..]);
            h.finalize()
        };
        let mut opts = OptionSet::with_defaults();
        opts.set("piece-checksum", format!("sha-256:8:{p0},{p1}"));
        verify_piece_checksums(&p, &opts).await.unwrap();
        opts.set("piece-checksum", format!("sha-256:8:00{p0},{p1}"));
        assert!(verify_piece_checksums(&p, &opts).await.is_err());
    }
}
