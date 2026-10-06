//! BitTorrent piece wire (BEP 3): handshake, request/piece, SHA-1 verify.
#![forbid(unsafe_code)]

use crate::bencode::{self, BVal};
use crate::error::{Error, Result};
use crate::http::HttpProgress;
use crate::options::OptionSet;
use crate::storage::FileStorage;
use sha1::{Digest, Sha1};
use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::watch;

pub const PSTR: &[u8] = b"BitTorrent protocol";
pub const BLOCK: u32 = 16 * 1024;
pub const MSG_CHOKE: u8 = 0;
pub const MSG_UNCHOKE: u8 = 1;
pub const MSG_INTERESTED: u8 = 2;
pub const MSG_HAVE: u8 = 4;
pub const MSG_BITFIELD: u8 = 5;
pub const MSG_REQUEST: u8 = 6;
pub const MSG_PIECE: u8 = 7;
pub const MSG_EXT: u8 = 20;
pub const EXT_HANDSHAKE: u8 = 0;
pub const UT_METADATA_ID: u8 = 1;
pub const UT_PEX_ID: u8 = 2;
pub const META_BLOCK: usize = 16 * 1024;

static LAST_PEERS_TRIED: AtomicU64 = AtomicU64::new(0);
static LAST_UPLOAD_LIMIT: AtomicU64 = AtomicU64::new(0);
static LAST_OVERALL_UP: AtomicU64 = AtomicU64::new(0);
static LAST_REQUEST_PEER_SPEED: AtomicU64 = AtomicU64::new(0);
static LAST_LISTEN_PORT: AtomicU16 = AtomicU16::new(0);
static LAST_TRACKER_SEND: AtomicU64 = AtomicU64::new(0);
static LAST_TRACKER_RECV: AtomicU64 = AtomicU64::new(0);
static UDP_TX: AtomicU64 = AtomicU64::new(1);

pub fn last_peers_tried() -> u64 {
    LAST_PEERS_TRIED.load(Ordering::SeqCst)
}

pub fn last_upload_limit() -> u64 {
    LAST_UPLOAD_LIMIT.load(Ordering::SeqCst)
}

pub fn last_overall_upload_limit() -> u64 {
    LAST_OVERALL_UP.load(Ordering::SeqCst)
}

pub fn last_request_peer_speed_limit() -> u64 {
    LAST_REQUEST_PEER_SPEED.load(Ordering::SeqCst)
}

pub fn last_listen_port() -> u16 {
    LAST_LISTEN_PORT.load(Ordering::SeqCst)
}

/// C++ DefaultBtAnnounce SocketCore::writeData send() of HTTP GET.
pub fn last_tracker_send() -> u64 {
    LAST_TRACKER_SEND.load(Ordering::SeqCst)
}

/// C++ DefaultBtAnnounce SocketCore::readData recv() of compact bencode.
pub fn last_tracker_recv() -> u64 {
    LAST_TRACKER_RECV.load(Ordering::SeqCst)
}

pub fn reset_tracker_io() {
    LAST_TRACKER_SEND.store(0, Ordering::SeqCst);
    LAST_TRACKER_RECV.store(0, Ordering::SeqCst);
}

/// C++ `--listen-port=PORT...`: comma list and `lo-hi` ranges (capped).
pub fn parse_listen_ports(spec: &str) -> Vec<u16> {
    let spec = spec.trim();
    if spec.is_empty() {
        return vec![6881];
    }
    let mut out = Vec::new();
    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((a, b)) = part.split_once('-') {
            let Ok(lo0) = a.trim().parse::<u16>() else {
                continue;
            };
            let Ok(hi0) = b.trim().parse::<u16>() else {
                continue;
            };
            let (lo, hi) = if lo0 <= hi0 { (lo0, hi0) } else { (hi0, lo0) };
            let n = u32::from(hi)
                .saturating_sub(u32::from(lo))
                .saturating_add(1)
                .min(256);
            for i in 0..n {
                out.push(lo.saturating_add(i as u16));
            }
        } else if let Ok(p) = part.parse::<u16>() {
            if p != 0 {
                out.push(p);
            }
        }
    }
    if out.is_empty() {
        vec![6881]
    } else {
        out
    }
}

async fn bind_listen(opts: &OptionSet) -> Option<TcpListener> {
    let spec = opts.get("listen-port").unwrap_or("6881");
    for port in parse_listen_ports(spec) {
        if port == 0 {
            continue;
        }
        if let Ok(l) = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).await {
            LAST_LISTEN_PORT.store(port, Ordering::SeqCst);
            return Some(l);
        }
    }
    None
}

pub struct BtJob {
    pub torrent: Vec<u8>,
    pub dest: PathBuf,
    pub peers: Vec<SocketAddr>,
    pub opts: OptionSet,
    pub progress: HttpProgress,
    pub cancel: watch::Receiver<bool>,
}

#[derive(Clone, Debug)]
pub struct BtFile {
    pub path: PathBuf,
    pub length: u64,
    pub offset: u64,
}

#[derive(Clone, Debug)]
pub struct MetaInfo {
    pub info_hash: [u8; 20],
    pub name: String,
    pub piece_length: u32,
    pub length: u64,
    pub pieces: Vec<[u8; 20]>,
    pub announce: Option<String>,
    pub files: Vec<BtFile>,
    files_list: bool,
}

impl MetaInfo {
    pub fn from_torrent(bytes: &[u8]) -> Result<Self> {
        let top = bencode::decode(bytes)?;
        let info = top.dict_get(b"info").ok_or_else(|| Error::Bt("no info".into()))?;
        let raw = bencode::raw_info_dict(bytes)?;
        let mut info_hash = [0u8; 20];
        info_hash.copy_from_slice(&Sha1::digest(raw));
        let name = info
            .dict_get(b"name")
            .and_then(|v| v.as_bytes())
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .unwrap_or_else(|| "index".into());
        let piece_length = info
            .dict_get(b"piece length")
            .and_then(|v| v.as_int())
            .ok_or_else(|| Error::Bt("piece length".into()))? as u32;
        if piece_length == 0 {
            return Err(Error::Bt("piece length 0".into()));
        }
        let (files, files_list) = parse_files(info, &name)?;
        let length: u64 = files.iter().map(|f| f.length).sum();
        if length == 0 {
            return Err(Error::Bt("empty torrent".into()));
        }
        let pb = info
            .dict_get(b"pieces")
            .and_then(|v| v.as_bytes())
            .ok_or_else(|| Error::Bt("pieces".into()))?;
        if pb.len() % 20 != 0 || pb.is_empty() {
            return Err(Error::Bt("pieces len".into()));
        }
        let pieces = pb.chunks(20).map(|c| {
            let mut a = [0u8; 20];
            a.copy_from_slice(c);
            a
        }).collect();
        let announce = top
            .dict_get(b"announce")
            .and_then(|v| v.as_bytes())
            .map(|b| String::from_utf8_lossy(b).into_owned());
        Ok(Self {
            info_hash,
            name,
            piece_length,
            length,
            pieces,
            announce,
            files,
            files_list,
        })
    }

    pub fn num_pieces(&self) -> usize {
        self.pieces.len()
    }

    pub fn piece_size(&self, i: usize) -> u32 {
        let start = i as u64 * self.piece_length as u64;
        let rem = self.length.saturating_sub(start);
        rem.min(self.piece_length as u64) as u32
    }

    pub fn is_multi(&self) -> bool {
        self.files_list
    }
}

fn parse_files(info: &BVal, name: &str) -> Result<(Vec<BtFile>, bool)> {
    if let Some(len) = info.dict_get(b"length").and_then(|v| v.as_int()) {
        if len < 0 {
            return Err(Error::Bt("negative length".into()));
        }
        return Ok((
            vec![BtFile {
                path: PathBuf::from(name),
                length: len as u64,
                offset: 0,
            }],
            false,
        ));
    }
    let list = info
        .dict_get(b"files")
        .and_then(|v| v.as_list())
        .ok_or_else(|| Error::Bt("no length/files".into()))?;
    let mut files = Vec::new();
    let mut offset = 0u64;
    for f in list {
        let length = f
            .dict_get(b"length")
            .and_then(|v| v.as_int())
            .ok_or_else(|| Error::Bt("file length".into()))?;
        if length < 0 {
            return Err(Error::Bt("negative file length".into()));
        }
        let comps = f
            .dict_get(b"path")
            .and_then(|v| v.as_list())
            .ok_or_else(|| Error::Bt("file path".into()))?;
        let mut path = PathBuf::new();
        for c in comps {
            let s = c
                .as_bytes()
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .ok_or_else(|| Error::Bt("path component".into()))?;
            if s.is_empty() || s == "." || s == ".." || s.contains('\0') || s.contains('/') {
                return Err(Error::Bt("bad path component".into()));
            }
            path.push(s);
        }
        if path.as_os_str().is_empty() {
            return Err(Error::Bt("empty file path".into()));
        }
        files.push(BtFile {
            path,
            length: length as u64,
            offset,
        });
        offset += length as u64;
    }
    if files.is_empty() {
        return Err(Error::Bt("no files".into()));
    }
    Ok((files, true))
}

fn dest_path(job: &BtJob, meta: &MetaInfo) -> PathBuf {
    if job.opts.out().is_none() {
        let fname = job
            .dest
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if fname.is_empty() || fname == "index" {
            job.opts.dir().join(&meta.name)
        } else {
            job.dest.clone()
        }
    } else {
        job.dest.clone()
    }
}

fn dest_is_full(meta: &MetaInfo, dest: &Path, opts: &OptionSet) -> bool {
    if meta.is_multi() {
        meta.files.iter().enumerate().all(|(i, f)| {
            let p = file_out_path(opts, dest, meta, i, &f.path);
            std::fs::metadata(&p)
                .map(|m| m.len() == f.length)
                .unwrap_or(false)
        })
    } else {
        std::fs::metadata(dest)
            .map(|m| dest.is_file() && m.len() == meta.length)
            .unwrap_or(false)
    }
}

/// C++ `--index-out=INDEX=PATH` (repeatable, newline- or comma-joined). INDEX is 1-based.
pub fn parse_index_out(spec: Option<&str>) -> HashMap<usize, PathBuf> {
    let mut m = HashMap::new();
    let Some(spec) = spec.filter(|s| !s.is_empty()) else {
        return m;
    };
    for part in spec.split(['\n', ',']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some((i, p)) = part.split_once('=') else {
            continue;
        };
        let Ok(n) = i.trim().parse::<usize>() else {
            continue;
        };
        let p = p.trim();
        if n >= 1 && !p.is_empty() {
            m.insert(n, PathBuf::from(p));
        }
    }
    m
}

fn file_out_path(opts: &OptionSet, dest: &Path, meta: &MetaInfo, index0: usize, rel: &Path) -> PathBuf {
    let map = parse_index_out(opts.get("index-out"));
    if let Some(p) = map.get(&(index0 + 1)) {
        if p.is_absolute() {
            p.clone()
        } else {
            opts.dir().join(p)
        }
    } else if meta.is_multi() {
        dest.join(rel)
    } else {
        dest.to_path_buf()
    }
}

/// C++ `--select-file` 1-based indexes and ranges (`1-3,5`). Absent/empty = all.
pub fn parse_select_file(spec: Option<&str>, nfiles: usize) -> Vec<bool> {
    let mut sel = vec![false; nfiles];
    let Some(spec) = spec.map(str::trim).filter(|s| !s.is_empty()) else {
        return vec![true; nfiles];
    };
    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((a, b)) = part.split_once('-') {
            let Ok(lo) = a.trim().parse::<usize>() else {
                continue;
            };
            let Ok(hi) = b.trim().parse::<usize>() else {
                continue;
            };
            let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
            for i in lo..=hi {
                if i >= 1 && i <= nfiles {
                    sel[i - 1] = true;
                }
            }
        } else if let Ok(i) = part.parse::<usize>() {
            if i >= 1 && i <= nfiles {
                sel[i - 1] = true;
            }
        }
    }
    sel
}

/// C++ `--show-files`: print torrent file listing and do not download.
pub fn format_show_files(torrent: &[u8]) -> Result<String> {
    let meta = MetaInfo::from_torrent(torrent)?;
    Ok(format_show_files_meta(&meta))
}

pub fn format_show_files_meta(meta: &MetaInfo) -> String {
    let mut o = String::from(
        "Files:\nidx|path/length\n===+===========================================================================\n",
    );
    for (i, f) in meta.files.iter().enumerate() {
        o.push_str(&format!(
            "{:>3}.|{}\n   |{}B\n---+---------------------------------------------------------------------------\n",
            i + 1,
            f.path.display(),
            f.length
        ));
    }
    o
}

/// C++ `--bt-prioritize-piece=head[=SIZE],tail[=SIZE]`. Missing SIZE = 1MiB.
pub fn parse_prioritize_piece(spec: Option<&str>) -> (u64, u64) {
    let Some(spec) = spec.map(str::trim).filter(|s| !s.is_empty()) else {
        return (0, 0);
    };
    const DEFAULT: u64 = 1024 * 1024;
    let mut head = 0u64;
    let mut tail = 0u64;
    for part in spec.split(',') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("head") {
            head = if rest.is_empty() {
                DEFAULT
            } else if let Some(sz) = rest.strip_prefix('=') {
                crate::storage::parse_size(sz).unwrap_or(DEFAULT)
            } else {
                DEFAULT
            };
        } else if let Some(rest) = part.strip_prefix("tail") {
            tail = if rest.is_empty() {
                DEFAULT
            } else if let Some(sz) = rest.strip_prefix('=') {
                crate::storage::parse_size(sz).unwrap_or(DEFAULT)
            } else {
                DEFAULT
            };
        }
    }
    (head, tail)
}

fn piece_in_priority(meta: &MetaInfo, i: usize, head: u64, tail: u64) -> bool {
    if head == 0 && tail == 0 {
        return false;
    }
    let start = i as u64 * meta.piece_length as u64;
    let end = start + meta.piece_size(i) as u64;
    for f in &meta.files {
        let f_end = f.offset + f.length;
        if head > 0 {
            let h_end = f.offset + head.min(f.length);
            if start < h_end && end > f.offset {
                return true;
            }
        }
        if tail > 0 {
            let t_start = f_end.saturating_sub(tail.min(f.length));
            if start < f_end && end > t_start {
                return true;
            }
        }
    }
    false
}

fn next_needed_piece(got: &[bool], meta: &MetaInfo, head: u64, tail: u64) -> Option<usize> {
    if head > 0 || tail > 0 {
        if let Some((i, _)) = got.iter().enumerate().find(|(i, g)| {
            !**g && piece_in_priority(meta, *i, head, tail)
        }) {
            return Some(i);
        }
    }
    got.iter().position(|g| !g)
}

fn piece_needed(meta: &MetaInfo, i: usize, selected: &[bool]) -> bool {
    let start = i as u64 * meta.piece_length as u64;
    let end = start + meta.piece_size(i) as u64;
    meta.files.iter().enumerate().any(|(fi, f)| {
        selected.get(fi).copied().unwrap_or(false)
            && start < f.offset + f.length
            && end > f.offset
    })
}

/// Build a single-file .torrent (sorted-key info dict) for a payload.
pub fn build_single_file(name: &str, piece_length: u32, data: &[u8], announce: &str) -> Vec<u8> {
    let mut pieces = Vec::new();
    for chunk in data.chunks(piece_length as usize) {
        pieces.extend_from_slice(&Sha1::digest(chunk));
    }
    let info = BVal::Dict(vec![
        (b"length".to_vec(), BVal::Int(data.len() as i64)),
        (b"name".to_vec(), BVal::Bytes(name.as_bytes().to_vec())),
        (b"piece length".to_vec(), BVal::Int(piece_length as i64)),
        (b"pieces".to_vec(), BVal::Bytes(pieces)),
    ]);
    let top = BVal::Dict(vec![
        (b"announce".to_vec(), BVal::Bytes(announce.as_bytes().to_vec())),
        (b"info".to_vec(), info),
    ]);
    bencode::encode(&top)
}

/// Build a multi-file .torrent. `files` is `(relative path, payload)`.
pub fn build_multi_file(
    name: &str,
    piece_length: u32,
    files: &[(&str, &[u8])],
    announce: &str,
) -> Vec<u8> {
    let mut concat = Vec::new();
    let mut flist = Vec::new();
    for (path, data) in files {
        concat.extend_from_slice(data);
        let comps: Vec<BVal> = path
            .split('/')
            .filter(|c| !c.is_empty())
            .map(|c| BVal::Bytes(c.as_bytes().to_vec()))
            .collect();
        flist.push(BVal::Dict(vec![
            (b"length".to_vec(), BVal::Int(data.len() as i64)),
            (b"path".to_vec(), BVal::List(comps)),
        ]));
    }
    let mut pieces = Vec::new();
    for chunk in concat.chunks(piece_length as usize) {
        pieces.extend_from_slice(&Sha1::digest(chunk));
    }
    let info = BVal::Dict(vec![
        (b"files".to_vec(), BVal::List(flist)),
        (b"name".to_vec(), BVal::Bytes(name.as_bytes().to_vec())),
        (b"piece length".to_vec(), BVal::Int(piece_length as i64)),
        (b"pieces".to_vec(), BVal::Bytes(pieces)),
    ]);
    let top = BVal::Dict(vec![
        (b"announce".to_vec(), BVal::Bytes(announce.as_bytes().to_vec())),
        (b"info".to_vec(), info),
    ]);
    bencode::encode(&top)
}

pub fn peer_id() -> [u8; 20] {
    peer_id_from_opts(&OptionSet::new())
}

/// C++ `--peer-id-prefix`: first 20 bytes of the BT handshake peer ID.
/// Shorter prefixes are padded with random bytes; longer ones are truncated.
pub fn peer_id_from_opts(opts: &OptionSet) -> [u8; 20] {
    match opts.get("peer-id-prefix").filter(|s| !s.is_empty()) {
        Some(p) => {
            let pb = p.as_bytes();
            let mut id = [0u8; 20];
            let n = pb.len().min(20);
            id[..n].copy_from_slice(&pb[..n]);
            if n < 20 {
                rand::RngCore::fill_bytes(&mut rand::rng(), &mut id[n..]);
            }
            id
        }
        None => {
            let mut id = *b"-AR0100-000000000000";
            let mut r = [0u8; 12];
            rand::RngCore::fill_bytes(&mut rand::rng(), &mut r);
            for (i, b) in r.iter().enumerate() {
                id[8 + i] = b"0123456789ABCDEF"[(*b as usize) % 16];
            }
            id
        }
    }
}

fn percent_encode_peer_id(id: &[u8]) -> String {
    let mut s = String::with_capacity(id.len() * 3);
    for &b in id {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                s.push(b as char);
            }
            _ => s.push_str(&format!("%{b:02X}")),
        }
    }
    s
}

/// C++ SocketBuffer::send + SocketCore::readData/writeData on a plaintext peer socket.
trait PeerWire: AsyncReadExt + AsyncWriteExt + Unpin {
    async fn send_bt(&mut self, id: u8, payload: &[u8]) -> Result<()>;
    async fn recv_bt(&mut self) -> Result<Option<(u8, Vec<u8>)>>;
    async fn recv_n(&mut self, buf: &mut [u8]) -> Result<usize>;
    async fn send_n(&mut self, buf: &[u8]) -> Result<()>;
}

impl PeerWire for TcpStream {
    async fn send_bt(&mut self, id: u8, payload: &[u8]) -> Result<()> {
        write_msg_tcp(self, id, payload).await
    }
    async fn recv_bt(&mut self) -> Result<Option<(u8, Vec<u8>)>> {
        read_msg_tcp(self).await
    }
    async fn recv_n(&mut self, buf: &mut [u8]) -> Result<usize> {
        crate::sockopt::recv_exact(self, buf).await
    }
    async fn send_n(&mut self, buf: &[u8]) -> Result<()> {
        crate::sockopt::send_all(self, buf).await
    }
}

impl PeerWire for crate::mse::MseStream {
    async fn send_bt(&mut self, id: u8, payload: &[u8]) -> Result<()> {
        let mut buf = Vec::with_capacity(5 + payload.len());
        buf.extend_from_slice(&((1 + payload.len()) as u32).to_be_bytes());
        buf.push(id);
        buf.extend_from_slice(payload);
        self.send_plain(&buf).await
    }
    async fn recv_bt(&mut self) -> Result<Option<(u8, Vec<u8>)>> {
        let mut lenb = [0u8; 4];
        let n = self.recv_exact_plain(&mut lenb).await?;
        if n == 0 {
            return Ok(None);
        }
        let len = u32::from_be_bytes(lenb);
        if len == 0 {
            return Ok(Some((255, Vec::new())));
        }
        if len > 128 * 1024 + 9 {
            return Err(Error::Bt("msg too large".into()));
        }
        let mut body = vec![0u8; len as usize];
        let got = self.recv_exact_plain(&mut body).await?;
        if got == 0 {
            return Ok(None);
        }
        Ok(Some((body[0], body[1..].to_vec())))
    }
    async fn recv_n(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.recv_exact_plain(buf).await
    }
    async fn send_n(&mut self, buf: &[u8]) -> Result<()> {
        self.send_plain(buf).await
    }
}

pub fn encode_handshake(info_hash: &[u8; 20], peer_id: &[u8; 20]) -> [u8; 68] {
    let mut h = [0u8; 68];
    h[0] = 19;
    h[1..20].copy_from_slice(PSTR);
    h[25] |= 0x10; // BEP 10 extension protocol
    h[28..48].copy_from_slice(info_hash);
    h[48..68].copy_from_slice(peer_id);
    h
}

pub async fn write_msg<W: AsyncWriteExt + Unpin>(w: &mut W, id: u8, payload: &[u8]) -> Result<()> {
    let len = (1 + payload.len()) as u32;
    w.write_all(&len.to_be_bytes()).await?;
    w.write_all(&[id]).await?;
    w.write_all(payload).await?;
    w.flush().await?;
    Ok(())
}

/// C++ SocketBuffer::send / BtPieceMessage: writev of length+id+payload (no concat Vec).
async fn write_msg_tcp(s: &TcpStream, id: u8, payload: &[u8]) -> Result<()> {
    let lenb = ((1 + payload.len()) as u32).to_be_bytes();
    let idb = [id];
    if payload.is_empty() {
        crate::sockopt::writev_all(s, &[&lenb, &idb]).await
    } else {
        crate::sockopt::writev_all(s, &[&lenb, &idb, payload]).await
    }
}

/// C++ BtPieceMessage::write: writev of len/id/index/begin/block (no piece copy).
async fn write_piece_tcp(s: &TcpStream, idx: u32, begin: u32, block: &[u8]) -> Result<()> {
    let lenb = ((9 + block.len()) as u32).to_be_bytes();
    let idb = [MSG_PIECE];
    let idxb = idx.to_be_bytes();
    let begb = begin.to_be_bytes();
    crate::sockopt::writev_all(s, &[&lenb, &idb, &idxb, &begb, block]).await
}

pub async fn read_msg<R: AsyncReadExt + Unpin>(r: &mut R) -> Result<Option<(u8, Vec<u8>)>> {
    let mut lenb = [0u8; 4];
    if r.read_exact(&mut lenb).await.is_err() {
        return Ok(None);
    }
    let len = u32::from_be_bytes(lenb);
    if len == 0 {
        return Ok(Some((255, Vec::new()))); // keep-alive
    }
    if len > 128 * 1024 + 9 {
        return Err(Error::Bt("msg too large".into()));
    }
    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body).await?;
    Ok(Some((body[0], body[1..].to_vec())))
}

/// C++ SocketCore::readData: recv() BT length+id+payload (no tokio read).
async fn read_msg_tcp(s: &TcpStream) -> Result<Option<(u8, Vec<u8>)>> {
    let mut lenb = [0u8; 4];
    let n = crate::sockopt::recv_exact(s, &mut lenb).await?;
    if n == 0 {
        return Ok(None);
    }
    let len = u32::from_be_bytes(lenb);
    if len == 0 {
        return Ok(Some((255, Vec::new())));
    }
    if len > 128 * 1024 + 9 {
        return Err(Error::Bt("msg too large".into()));
    }
    let mut body = vec![0u8; len as usize];
    let got = crate::sockopt::recv_exact(s, &mut body).await?;
    if got == 0 {
        return Ok(None);
    }
    Ok(Some((body[0], body[1..].to_vec())))
}

/// C++ magnet `x.pe=host:port` extra peers (BEP 9 / aria2 extension).
pub fn peers_from_magnet(uri: &str) -> Vec<SocketAddr> {
    if !uri.starts_with("magnet:") {
        return Vec::new();
    }
    let Some(q) = uri.split_once('?').map(|(_, q)| q) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for part in q.split('&') {
        let Some((k, v)) = part.split_once('=') else { continue };
        if k != "x.pe" {
            continue;
        }
        let v = urlencoding_decode(v);
        if let Ok(a) = v.parse() {
            out.push(a);
        }
    }
    out
}

fn urlencoding_decode(s: &str) -> String {
    let mut o = String::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(x) = u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or(""), 16) {
                o.push(x as char);
                i += 3;
                continue;
            }
        }
        o.push(if b[i] == b'+' { ' ' } else { b[i] as char });
        i += 1;
    }
    o
}

pub fn is_torrent_path(s: &str) -> bool {
    let p = s.split('?').next().unwrap_or(s);
    p.ends_with(".torrent") || std::path::Path::new(p).extension().is_some_and(|e| e == "torrent")
}

pub fn looks_like_bt(uris: &[String], opts: &OptionSet) -> bool {
    if !opts.bool("enable-bittorrent", true) {
        return false;
    }
    opts.get("torrent-file").is_some_and(|s| !s.is_empty())
        || uris.iter().any(|u| {
            u.starts_with("magnet:")
                || (is_torrent_path(u) && !u.starts_with("http://") && !u.starts_with("https://"))
        })
}

pub fn load_torrent_bytes(uris: &[String], opts: &OptionSet) -> Result<Vec<u8>> {
    if let Some(p) = opts.get("torrent-file").filter(|s| !s.is_empty()) {
        return std::fs::read(p).map_err(|e| Error::Bt(format!("torrent-file: {e}")));
    }
    if let Some(p) = uris.iter().find(|u| is_torrent_path(u)) {
        return std::fs::read(p).map_err(|e| Error::Bt(format!("torrent: {e}")));
    }
    if let Some(uri) = uris.iter().find(|u| u.starts_with("magnet:")) {
        if opts.bool("bt-load-saved-metadata", false) {
            if let Ok(ih) = info_hash_from_magnet(uri) {
                let p = opts.dir().join(format!("{}.torrent", hex::encode(ih)));
                if p.exists() {
                    return std::fs::read(&p).map_err(|e| Error::Bt(format!("saved metadata: {e}")));
                }
            }
        }
        return Ok(Vec::new());
    }
    Err(Error::Bt("no torrent-file / .torrent URI / magnet".into()))
}

pub fn collect_peers(uris: &[String]) -> Vec<SocketAddr> {
    uris.iter().flat_map(|u| peers_from_magnet(u)).collect()
}

/// Magnet `xt=urn:btih:` — 40-char hex or 32-char base32 (BEP 9).
pub fn info_hash_from_magnet(uri: &str) -> Result<[u8; 20]> {
    if !uri.starts_with("magnet:") {
        return Err(Error::Bt("not magnet".into()));
    }
    let q = uri.split_once('?').map(|(_, q)| q).unwrap_or("");
    for part in q.split('&') {
        let Some((k, v)) = part.split_once('=') else { continue };
        let k = urlencoding_decode(k);
        if k != "xt" && !k.starts_with("xt.") {
            continue;
        }
        let v = urlencoding_decode(v);
        let lower = v.to_ascii_lowercase();
        let Some(h) = lower.strip_prefix("urn:btih:") else { continue };
        return decode_infohash(h);
    }
    Err(Error::Bt("magnet missing xt=urn:btih".into()))
}

pub fn magnet_dn(uri: &str) -> Option<String> {
    if !uri.starts_with("magnet:") {
        return None;
    }
    let q = uri.split_once('?').map(|(_, q)| q)?;
    for part in q.split('&') {
        let Some((k, v)) = part.split_once('=') else { continue };
        if urlencoding_decode(k) == "dn" {
            let n = urlencoding_decode(v);
            if !n.is_empty() {
                return Some(n);
            }
        }
    }
    None
}

fn decode_infohash(h: &str) -> Result<[u8; 20]> {
    let h = h.trim();
    if h.len() == 40 && h.bytes().all(|c| c.is_ascii_hexdigit()) {
        let v = hex::decode(h).map_err(|_| Error::Bt("infohash hex".into()))?;
        let mut a = [0u8; 20];
        a.copy_from_slice(&v);
        return Ok(a);
    }
    if h.len() == 32 {
        return decode_b32_160(h);
    }
    Err(Error::Bt("infohash length".into()))
}

fn decode_b32_160(s: &str) -> Result<[u8; 20]> {
    let s = s.to_ascii_uppercase();
    if s.len() != 32 {
        return Err(Error::Bt("base32 len".into()));
    }
    let mut acc: u64 = 0;
    let mut nbits = 0u32;
    let mut out = [0u8; 20];
    let mut oi = 0usize;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'2'..=b'7' => c - b'2' + 26,
            _ => return Err(Error::Bt("base32 char".into())),
        } as u64;
        acc = (acc << 5) | v;
        nbits += 5;
        while nbits >= 8 && oi < 20 {
            nbits -= 8;
            out[oi] = (acc >> nbits) as u8;
            acc &= (1u64 << nbits) - 1;
            oi += 1;
        }
    }
    if oi != 20 {
        return Err(Error::Bt("base32 short".into()));
    }
    Ok(out)
}

pub fn wrap_info_dict(info: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(8 + info.len());
    v.extend_from_slice(b"d4:info");
    v.extend_from_slice(info);
    v.push(b'e');
    v
}

fn ltep_handshake_payload(metadata_size: Option<usize>, opts: &OptionSet) -> Vec<u8> {
    let pex = opts.bool("enable-peer-exchange", true);
    let agent = opts
        .get("peer-agent")
        .filter(|s| !s.is_empty())
        .unwrap_or("aria2-rust");
    let mut m = vec![(b"ut_metadata".to_vec(), BVal::Int(UT_METADATA_ID as i64))];
    if pex {
        m.push((b"ut_pex".to_vec(), BVal::Int(UT_PEX_ID as i64)));
    }
    let mut dict = vec![
        (b"m".to_vec(), BVal::Dict(m)),
        (b"v".to_vec(), BVal::Bytes(agent.as_bytes().to_vec())),
    ];
    if let Some(sz) = metadata_size {
        dict.push((b"metadata_size".to_vec(), BVal::Int(sz as i64)));
    }
    let enc = bencode::encode(&BVal::Dict(dict));
    let mut p = Vec::with_capacity(1 + enc.len());
    p.push(EXT_HANDSHAKE);
    p.extend(enc);
    p
}

pub fn parse_compact_peers(b: &[u8]) -> Vec<SocketAddr> {
    let mut out = Vec::new();
    for c in b.chunks(6) {
        if c.len() < 6 {
            break;
        }
        let ip = Ipv4Addr::new(c[0], c[1], c[2], c[3]);
        let port = u16::from_be_bytes([c[4], c[5]]);
        if port == 0 {
            continue;
        }
        out.push(SocketAddr::from((ip, port)));
    }
    out
}

pub fn encode_compact_peers(addrs: &[SocketAddr]) -> Vec<u8> {
    let mut o = Vec::with_capacity(addrs.len() * 6);
    for a in addrs {
        if let SocketAddr::V4(v) = a {
            o.extend_from_slice(&v.ip().octets());
            o.extend_from_slice(&v.port().to_be_bytes());
        }
    }
    o
}

pub fn parse_compact_peers6(b: &[u8]) -> Vec<SocketAddr> {
    let mut out = Vec::new();
    for c in b.chunks(18) {
        if c.len() < 18 {
            break;
        }
        let mut oct = [0u8; 16];
        oct.copy_from_slice(&c[..16]);
        let port = u16::from_be_bytes([c[16], c[17]]);
        if port == 0 {
            continue;
        }
        out.push(SocketAddr::from((Ipv6Addr::from(oct), port)));
    }
    out
}

pub fn encode_compact_peers6(addrs: &[SocketAddr]) -> Vec<u8> {
    let mut o = Vec::with_capacity(addrs.len() * 18);
    for a in addrs {
        if let SocketAddr::V6(v) = a {
            o.extend_from_slice(&v.ip().octets());
            o.extend_from_slice(&v.port().to_be_bytes());
        }
    }
    o
}

enum Swarm {
    Done,
    More(Vec<SocketAddr>),
}

fn ut_metadata_request(peer_ext_id: u8, piece: i64) -> Vec<u8> {
    let enc = bencode::encode(&BVal::Dict(vec![
        (b"msg_type".to_vec(), BVal::Int(0)),
        (b"piece".to_vec(), BVal::Int(piece)),
    ]));
    let mut p = Vec::with_capacity(1 + enc.len());
    p.push(peer_ext_id);
    p.extend(enc);
    p
}

pub async fn download(job: BtJob) -> Result<()> {
    LAST_LISTEN_PORT.store(0, Ordering::SeqCst);
    {
        job.progress.peers.lock().unwrap().clear();
    }
    let info_hash = if !job.torrent.is_empty() {
        MetaInfo::from_torrent(&job.torrent)?.info_hash
    } else if let Some(uri) = job.opts.get("magnet") {
        info_hash_from_magnet(uri)?
    } else {
        return Err(Error::Bt("no torrent and no magnet".into()));
    };
    let existing = if job.torrent.is_empty() {
        None
    } else {
        Some(MetaInfo::from_torrent(&job.torrent)?)
    };
    if let Some(m) = &existing {
        job.progress.total.store(m.length, Ordering::Relaxed);
        if job.opts.bool("check-integrity", false) {
            let dest = dest_path(&job, m);
            let got = verify_existing(m, &dest, &job.opts).await;
            let all_good = !got.is_empty() && got.iter().all(|g| *g);
            if job.opts.bool("hash-check-only", false) {
                if all_good {
                    job.progress.completed.store(m.length, Ordering::Relaxed);
                    return Ok(());
                }
                return Err(Error::Bt("hash-check-only: incomplete".into()));
            }
            if all_good {
                job.progress.completed.store(m.length, Ordering::Relaxed);
                if job.opts.bool("bt-hash-check-seed", true) {
                    if job.opts.bool("bt-enable-hook-after-hash-check", true) {
                        fire_bt_complete(m, &job);
                    }
                    maybe_seed(m, &job).await?;
                }
                return Ok(());
            }
        }
        if job.opts.bool("bt-seed-unverified", false) && dest_is_full(m, &dest_path(&job, m), &job.opts) {
            job.progress.completed.store(m.length, Ordering::Relaxed);
            fire_bt_complete(m, &job);
            maybe_seed(m, &job).await?;
            return Ok(());
        }
    }
    let mut peers = job.peers.clone();
    if peers.is_empty() {
        let mut urls = Vec::new();
        if let Some(m) = &existing {
            if let Some(a) = m.announce.clone().filter(|s| !s.is_empty()) {
                if !tracker_excluded(&a, job.opts.get("bt-exclude-tracker")) {
                    urls.push(a);
                }
            }
        }
        if let Some(extra) = job.opts.get("bt-tracker").filter(|s| !s.is_empty()) {
            for t in extra.split(',') {
                let t = t.trim();
                if !t.is_empty() && !urls.iter().any(|u| u == t) {
                    urls.push(t.to_string());
                }
            }
        }
        let announce_meta = existing.clone().unwrap_or(MetaInfo {
            info_hash,
            name: String::new(),
            piece_length: 16 * 1024,
            length: 0,
            pieces: vec![],
            announce: None,
            files: vec![],
            files_list: false,
        });
        for url in &urls {
            match announce_compact(url, &announce_meta, &job.opts).await {
                Ok(p) if !p.is_empty() => {
                    peers = p;
                    break;
                }
                _ => {}
            }
        }
    }
    if peers.is_empty() {
        match crate::dht::get_peers(info_hash, &job.opts).await {
            Ok(p) if !p.is_empty() => peers = p,
            _ => {}
        }
    }
    if peers.is_empty() {
        match crate::lpd::discover(info_hash, &job.opts).await {
            Ok(p) if !p.is_empty() => peers = p,
            _ => {}
        }
    }
    if peers.is_empty() {
        return Err(Error::Bt("no peers".into()));
    }
    let timeout = Duration::from_secs(job.opts.u64("connect-timeout", 60).max(1));
    let mut last_err = Error::Bt("all peers failed".into());
    let mut seen = HashSet::new();
    let mut queue = peers;
    // C++ `--bt-max-peers` (default 55, 0 = unlimited): cap distinct connects.
    // `--bt-request-peer-speed-limit` (default 50K): if download speed is below
    // SPEED, temporarily increase the cap so more peers can be tried.
    let max = job.opts.u64("bt-max-peers", 55);
    let req_limit = crate::http::parse_speed(
        job.opts.get("bt-request-peer-speed-limit").unwrap_or("50K"),
    )
    .unwrap_or(50 * 1024);
    LAST_REQUEST_PEER_SPEED.store(req_limit, Ordering::SeqCst);
    let pace = tokio::time::Instant::now();
    let mut tried = 0u64;
    LAST_PEERS_TRIED.store(0, Ordering::SeqCst);
    while let Some(addr) = queue.pop() {
        if !seen.insert(addr) {
            continue;
        }
        if max > 0 && tried >= max {
            let elapsed = pace.elapsed().as_secs_f64();
            let done = job.progress.completed.load(Ordering::Relaxed);
            let speed = if elapsed < 0.05 || done == 0 {
                0
            } else {
                (done as f64 / elapsed) as u64
            };
            if req_limit == 0 || speed >= req_limit {
                last_err = Error::Bt("bt-max-peers".into());
                break;
            }
        }
        tried += 1;
        LAST_PEERS_TRIED.store(tried, Ordering::SeqCst);
        match tokio::time::timeout(
            timeout,
            download_from_peer(addr, info_hash, existing.as_ref(), &job),
        )
        .await
        {
            Ok(Ok(Swarm::Done)) => {
                if let Some(m) = existing.as_ref() {
                    fire_bt_complete(m, &job);
                    maybe_seed(m, &job).await?;
                } else if !job.torrent.is_empty() {
                    if let Ok(m) = MetaInfo::from_torrent(&job.torrent) {
                        fire_bt_complete(&m, &job);
                        maybe_seed(&m, &job).await?;
                    }
                }
                return Ok(());
            }
            Ok(Ok(Swarm::More(more))) => {
                last_err = Error::Bt("pex: trying added peers".into());
                for p in more {
                    if !seen.contains(&p) {
                        queue.push(p);
                    }
                }
            }
            Ok(Err(e)) => last_err = e,
            Err(_) => last_err = Error::Bt("connect-timeout".into()),
        }
    }
    Err(last_err)
}

/// C++ DefaultBtAnnounce HTTPS: SocketCore TLS writeData/readData (rustls records).
async fn announce_https_get(url: &str, opts: &OptionSet) -> Result<Vec<u8>> {
    let u = url::Url::parse(url).map_err(|e| Error::Bt(format!("https tracker: {e}")))?;
    let host = u
        .host_str()
        .ok_or_else(|| Error::Bt("https tracker host".into()))?
        .to_string();
    let port = u.port_or_known_default().unwrap_or(443);
    let path = {
        let p = if u.path().is_empty() { "/" } else { u.path() };
        match u.query() {
            Some(q) => format!("{p}?{q}"),
            None => p.to_string(),
        }
    };
    let host_hdr = if u.port().is_some() {
        format!("{host}:{port}")
    } else {
        host.clone()
    };
    let ua = opts.get("user-agent").unwrap_or(crate::USER_AGENT);
    let connect = Duration::from_secs(opts.u64("bt-tracker-connect-timeout", 60).max(1));
    let total = Duration::from_secs(opts.u64("bt-tracker-timeout", 60).max(1));
    let cfg = crate::tls::client_config(opts)?;
    let work = async {
        let stream = tokio::time::timeout(connect, TcpStream::connect((host.as_str(), port)))
            .await
            .map_err(|_| Error::Bt("https tracker connect-timeout".into()))?
            .map_err(|e| Error::Bt(format!("https tracker connect: {e}")))?;
        let _ = crate::sockopt::apply_tcp_nodelay(&stream);
        let _ = crate::sockopt::apply_tcp_quickack(&stream);
        let name = rustls::pki_types::ServerName::try_from(host.as_str())
            .map_err(|_| Error::Bt("https tracker SNI".into()))?
            .to_owned();
        let mut tls = tokio_rustls::TlsConnector::from(cfg)
            .connect(name, stream)
            .await
            .map_err(|e| Error::Bt(format!("https tracker handshake: {e}")))?;
        let req = format!(
            "GET {path} HTTP/1.1\r\nHost: {host_hdr}\r\nUser-Agent: {ua}\r\nAccept: */*\r\nConnection: close\r\n\r\n"
        );
        tls.write_all(req.as_bytes())
            .await
            .map_err(|e| Error::Bt(format!("https tracker send: {e}")))?;
        LAST_TRACKER_SEND.fetch_add(1, Ordering::SeqCst);
        let mut buf = Vec::new();
        let mut tmp = [0u8; 16 * 1024];
        loop {
            let n = tls
                .read(&mut tmp)
                .await
                .map_err(|e| Error::Bt(format!("https tracker recv: {e}")))?;
            if n == 0 {
                break;
            }
            LAST_TRACKER_RECV.fetch_add(1, Ordering::SeqCst);
            buf.extend_from_slice(&tmp[..n]);
            if buf.len() > 1024 * 1024 {
                return Err(Error::Bt("tracker response too large".into()));
            }
            if tracker_http_complete(&buf) {
                break;
            }
        }
        tracker_http_body(&buf)
    };
    tokio::time::timeout(total, work)
        .await
        .map_err(|_| Error::Bt("https tracker timeout".into()))?
}

fn tracker_http_complete(buf: &[u8]) -> bool {
    let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
        return false;
    };
    let headers = &buf[..pos];
    let clen = std::str::from_utf8(headers)
        .unwrap_or("")
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
        .and_then(|l| l.split(':').nth(1))
        .and_then(|s| s.trim().parse::<usize>().ok());
    match clen {
        Some(clen) => buf.len().saturating_sub(pos + 4) >= clen,
        None => false,
    }
}

fn tracker_http_body(buf: &[u8]) -> Result<Vec<u8>> {
    let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
        return Err(Error::Bt("tracker: no headers".into()));
    };
    let status = std::str::from_utf8(&buf[..pos])
        .unwrap_or("")
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    if status != 200 {
        return Err(Error::Bt(format!("tracker status {status}")));
    }
    let body = &buf[pos + 4..];
    let clen = std::str::from_utf8(&buf[..pos])
        .unwrap_or("")
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
        .and_then(|l| l.split(':').nth(1))
        .and_then(|s| s.trim().parse::<usize>().ok());
    if let Some(clen) = clen {
        Ok(body.get(..clen.min(body.len())).unwrap_or(&[]).to_vec())
    } else {
        Ok(body.to_vec())
    }
}

/// C++ DefaultBtAnnounce / HttpRequest: SocketCore writeData send GET + readData recv
/// compact bencode (not slurped via HTTP client). Timeouts match
/// `--bt-tracker-connect-timeout` / `--bt-tracker-timeout`.
async fn announce_http_get(url: &str, opts: &OptionSet) -> Result<Vec<u8>> {
    let u = url::Url::parse(url).map_err(|e| Error::Bt(format!("tracker: {e}")))?;
    let host = u
        .host_str()
        .ok_or_else(|| Error::Bt("tracker host".into()))?
        .to_string();
    let port = u.port_or_known_default().unwrap_or(80);
    let path = {
        let p = if u.path().is_empty() { "/" } else { u.path() };
        match u.query() {
            Some(q) => format!("{p}?{q}"),
            None => p.to_string(),
        }
    };
    let host_hdr = if u.port().is_some() {
        format!("{host}:{port}")
    } else {
        host.clone()
    };
    let ua = opts.get("user-agent").unwrap_or(crate::USER_AGENT);
    let connect = Duration::from_secs(opts.u64("bt-tracker-connect-timeout", 60).max(1));
    let total = Duration::from_secs(opts.u64("bt-tracker-timeout", 60).max(1));
    let work = async {
        let stream = tokio::time::timeout(connect, TcpStream::connect((host.as_str(), port)))
            .await
            .map_err(|_| Error::Bt("tracker connect-timeout".into()))?
            .map_err(|e| Error::Bt(format!("tracker connect: {e}")))?;
        let _ = crate::sockopt::apply_tcp_nodelay(&stream);
        let _ = crate::sockopt::apply_tcp_quickack(&stream);
        let req = format!(
            "GET {path} HTTP/1.1\r\nHost: {host_hdr}\r\nUser-Agent: {ua}\r\nAccept: */*\r\nConnection: close\r\n\r\n"
        );
        crate::sockopt::send_all(&stream, req.as_bytes()).await?;
        LAST_TRACKER_SEND.fetch_add(1, Ordering::SeqCst);
        let mut buf = Vec::new();
        let mut tmp = [0u8; 16 * 1024];
        loop {
            let n = crate::sockopt::recv_some(&stream, &mut tmp).await?;
            if n == 0 {
                break;
            }
            LAST_TRACKER_RECV.fetch_add(1, Ordering::SeqCst);
            buf.extend_from_slice(&tmp[..n]);
            if buf.len() > 1024 * 1024 {
                return Err(Error::Bt("tracker response too large".into()));
            }
            if tracker_http_complete(&buf) {
                break;
            }
        }
        tracker_http_body(&buf)
    };
    tokio::time::timeout(total, work)
        .await
        .map_err(|_| Error::Bt("tracker timeout".into()))?
}

fn next_udp_tx() -> u32 {
    (UDP_TX.fetch_add(1, Ordering::Relaxed) & 0xffff_ffff) as u32
}

/// C++ UDPTrackerClient: send + recv with BEP 15 retransmit (same tx, timeout slices).
async fn udp_send_recv(
    sock: &UdpSocket,
    out: &[u8],
    inp: &mut [u8],
    tries: u32,
    per: Duration,
) -> Result<usize> {
    let mut last = Error::Bt("udp tracker timeout".into());
    for _ in 0..tries.max(1) {
        crate::sockopt::udp_send(sock, out).await?;
        LAST_TRACKER_SEND.fetch_add(1, Ordering::SeqCst);
        match tokio::time::timeout(per, crate::sockopt::udp_recv(sock, inp)).await {
            Ok(Ok(n)) => {
                LAST_TRACKER_RECV.fetch_add(1, Ordering::SeqCst);
                return Ok(n);
            }
            Ok(Err(e)) => last = e,
            Err(_) => last = Error::Bt("udp tracker timeout".into()),
        }
    }
    Err(last)
}

/// C++ DefaultBtAnnounce UDP (BEP 15): SocketCore send/recv on a connected datagram.
async fn announce_udp(url: &str, meta: &MetaInfo, opts: &OptionSet) -> Result<Vec<SocketAddr>> {
    let u = url::Url::parse(url).map_err(|e| Error::Bt(format!("udp tracker: {e}")))?;
    let host = u
        .host_str()
        .ok_or_else(|| Error::Bt("udp tracker host".into()))?
        .to_string();
    let port = u.port().unwrap_or(80);
    let pid = peer_id_from_opts(opts);
    let bound = LAST_LISTEN_PORT.load(Ordering::SeqCst);
    let listen = if bound != 0 {
        bound
    } else {
        parse_listen_ports(opts.get("listen-port").unwrap_or("6881"))
            .first()
            .copied()
            .unwrap_or(6881)
    };
    let mut ipb = [0u8; 4];
    if let Some(raw) = opts.get("bt-external-ip").filter(|s| !s.is_empty()) {
        if let Ok(v4) = raw.parse::<Ipv4Addr>() {
            ipb = v4.octets();
        }
    }
    let connect = Duration::from_secs(opts.u64("bt-tracker-connect-timeout", 60).max(1));
    let total = Duration::from_secs(opts.u64("bt-tracker-timeout", 60).max(1));
    let tries = 8u32;
    let per = Duration::from_millis(
        (opts.u64("bt-tracker-timeout", 60).saturating_mul(1000) / u64::from(tries)).clamp(200, 15_000),
    );
    let work = async {
        let sock = UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|e| Error::Bt(format!("udp bind: {e}")))?;
        tokio::time::timeout(connect, sock.connect((host.as_str(), port)))
            .await
            .map_err(|_| Error::Bt("udp tracker connect-timeout".into()))?
            .map_err(|e| Error::Bt(format!("udp tracker connect: {e}")))?;
        let tx1 = next_udp_tx();
        let mut creq = [0u8; 16];
        creq[0..8].copy_from_slice(&0x4172_7101_980u64.to_be_bytes());
        creq[8..12].copy_from_slice(&0u32.to_be_bytes());
        creq[12..16].copy_from_slice(&tx1.to_be_bytes());
        let mut cresp = [0u8; 16];
        let n = udp_send_recv(&sock, &creq, &mut cresp, tries, per).await?;
        if n < 16 {
            return Err(Error::Bt("udp tracker connect short".into()));
        }
        if u32::from_be_bytes(cresp[0..4].try_into().unwrap()) != 0 {
            return Err(Error::Bt("udp tracker connect action".into()));
        }
        if u32::from_be_bytes(cresp[4..8].try_into().unwrap()) != tx1 {
            return Err(Error::Bt("udp tracker connect tx".into()));
        }
        let conn_id = u64::from_be_bytes(cresp[8..16].try_into().unwrap());
        let tx2 = next_udp_tx();
        let mut ann = Vec::with_capacity(98);
        ann.extend_from_slice(&conn_id.to_be_bytes());
        ann.extend_from_slice(&1u32.to_be_bytes());
        ann.extend_from_slice(&tx2.to_be_bytes());
        ann.extend_from_slice(&meta.info_hash);
        ann.extend_from_slice(&pid);
        ann.extend_from_slice(&0u64.to_be_bytes());
        ann.extend_from_slice(&(meta.length as u64).to_be_bytes());
        ann.extend_from_slice(&0u64.to_be_bytes());
        ann.extend_from_slice(&2u32.to_be_bytes());
        ann.extend_from_slice(&ipb);
        ann.extend_from_slice(&next_udp_tx().to_be_bytes());
        ann.extend_from_slice(&50i32.to_be_bytes());
        ann.extend_from_slice(&listen.to_be_bytes());
        let mut aresp = [0u8; 1024];
        let n = udp_send_recv(&sock, &ann, &mut aresp, tries, per).await?;
        if n < 20 {
            return Err(Error::Bt("udp tracker announce short".into()));
        }
        if u32::from_be_bytes(aresp[0..4].try_into().unwrap()) != 1 {
            return Err(Error::Bt("udp tracker announce action".into()));
        }
        if u32::from_be_bytes(aresp[4..8].try_into().unwrap()) != tx2 {
            return Err(Error::Bt("udp tracker announce tx".into()));
        }
        Ok(parse_compact_peers(&aresp[20..n]))
    };
    tokio::time::timeout(total, work)
        .await
        .map_err(|_| Error::Bt("udp tracker timeout".into()))?
}

async fn announce_compact(url: &str, meta: &MetaInfo, opts: &OptionSet) -> Result<Vec<SocketAddr>> {
    if url.starts_with("udp:") {
        return announce_udp(url, meta, opts).await;
    }
    let pid = peer_id_from_opts(opts);
    let bound = LAST_LISTEN_PORT.load(Ordering::SeqCst);
    let port = if bound != 0 {
        bound
    } else {
        parse_listen_ports(opts.get("listen-port").unwrap_or("6881"))
            .first()
            .copied()
            .unwrap_or(6881)
    };
    let mut ih = String::new();
    for b in &meta.info_hash {
        ih.push_str(&format!("%{b:02X}"));
    }
    let mut pid_q = String::new();
    for b in &pid {
        pid_q.push_str(&format!("%{b:02X}"));
    }
    let sep = if url.contains('?') { '&' } else { '?' };
    let mut req = format!(
        "{url}{sep}info_hash={ih}&peer_id={pid_q}&port={port}&uploaded=0&downloaded=0&left={}&compact=1&numwant=50",
        meta.length
    );
    if let Some(ip) = opts.get("bt-external-ip").filter(|s| !s.is_empty()) {
        req.push_str("&ip=");
        req.push_str(ip);
    }
    let bytes = if req.starts_with("https://") {
        announce_https_get(&req, opts).await?
    } else {
        announce_http_get(&req, opts).await?
    };
    let v = bencode::decode(&bytes).map_err(|e| Error::Bt(format!("tracker: {e}")))?;
    if let Some(msg) = v.dict_get(b"failure reason").and_then(|x| x.as_bytes()) {
        return Err(Error::Bt(format!(
            "tracker: {}",
            String::from_utf8_lossy(msg)
        )));
    }
    let Some(pb) = v.dict_get(b"peers").and_then(|x| x.as_bytes()) else {
        return Ok(Vec::new());
    };
    Ok(parse_compact_peers(pb))
}

fn tracker_announce_urls(meta: &MetaInfo, opts: &OptionSet) -> Vec<String> {
    let mut urls = Vec::new();
    if let Some(a) = meta.announce.clone().filter(|s| !s.is_empty()) {
        if !tracker_excluded(&a, opts.get("bt-exclude-tracker")) {
            urls.push(a);
        }
    }
    if let Some(extra) = opts.get("bt-tracker").filter(|s| !s.is_empty()) {
        for t in extra.split(',') {
            let t = t.trim();
            if !t.is_empty() && !urls.iter().any(|u| u == t) {
                urls.push(t.to_string());
            }
        }
    }
    urls
}

/// C++ `--bt-exclude-tracker`: glob (`*` = all) against torrent announce URIs.
/// `--bt-tracker` extras are appended after exclusion and are never filtered.
fn tracker_excluded(uri: &str, spec: Option<&str>) -> bool {
    let Some(spec) = spec.filter(|s| !s.is_empty()) else {
        return false;
    };
    spec.split(',').any(|p| {
        let p = p.trim();
        !p.is_empty() && glob_match(p, uri)
    })
}

fn glob_match(pat: &str, s: &str) -> bool {
    if pat == "*" {
        return true;
    }
    let parts: Vec<&str> = pat.split('*').collect();
    if parts.len() == 1 {
        return pat == s;
    }
    let mut rest = s;
    if !pat.starts_with('*') {
        if !rest.starts_with(parts[0]) {
            return false;
        }
        rest = &rest[parts[0].len()..];
    }
    for (i, part) in parts.iter().enumerate().skip(1) {
        if part.is_empty() {
            continue;
        }
        if i == parts.len() - 1 && !pat.ends_with('*') {
            return rest.ends_with(part);
        }
        match rest.find(part) {
            Some(pos) => rest = &rest[pos + part.len()..],
            None => return false,
        }
    }
    true
}

async fn download_from_peer(
    addr: SocketAddr,
    info_hash: [u8; 20],
    existing: Option<&MetaInfo>,
    job: &BtJob,
) -> Result<Swarm> {
    let secs = job.opts.u64("peer-connection-timeout", 10).max(1);
    let s = tokio::time::timeout(Duration::from_secs(secs), TcpStream::connect(addr))
        .await
        .map_err(|_| Error::Bt("peer-connection-timeout".into()))??;
    let _ = crate::sockopt::apply_tcp_nodelay(&s);
    let _ = crate::sockopt::apply_tcp_quickack(&s);
    crate::sockopt::apply_recv_buffer(&s, &job.opts)?;
    crate::sockopt::apply_dscp(&s, &job.opts)?;
    if crate::mse::want_mse(&job.opts) {
        let mut s = crate::mse::initiator(s, &info_hash, &job.opts).await?;
        after_wire(&mut s, addr, info_hash, existing, job).await
    } else {
        let mut s = s;
        after_wire(&mut s, addr, info_hash, existing, job).await
    }
}

async fn after_wire<S: PeerWire>(
    s: &mut S,
    addr: SocketAddr,
    info_hash: [u8; 20],
    existing: Option<&MetaInfo>,
    job: &BtJob,
) -> Result<Swarm> {
    let mine = peer_id_from_opts(&job.opts);
    s.send_n(&encode_handshake(&info_hash, &mine)).await?;
    let mut hs = [0u8; 68];
    let n = s.recv_n(&mut hs).await?;
    if n == 0 {
        return Err(Error::Bt("handshake eof".into()));
    }
    if hs[0] != 19 || &hs[1..20] != PSTR {
        return Err(Error::Bt("handshake pstr".into()));
    }
    if hs[28..48] != info_hash {
        return Err(Error::Bt("handshake info_hash".into()));
    }
    {
        let mut g = job.progress.peers.lock().unwrap();
        g.retain(|p| p.ip != addr.ip().to_string() || p.port != addr.port());
        g.push(crate::http::PeerStat {
            ip: addr.ip().to_string(),
            port: addr.port(),
            peer_id: percent_encode_peer_id(&hs[48..68]),
            seeder: true,
            am_choking: false,
            peer_choking: true,
            bitfield: String::new(),
        });
    }
    let peer_ext = hs[25] & 0x10 != 0;
    if let Some(m) = existing {
        if peer_ext {
            s.send_bt(MSG_EXT, &ltep_handshake_payload(None, &job.opts)).await?;
        }
        return download_pieces(s, m, job, false).await;
    }
    if !peer_ext {
        return Err(Error::Bt(
            "peer has no extension protocol (need ut_metadata)".into(),
        ));
    }
    let (info, already_unchoked) = exchange_ut_metadata(s, &info_hash, job).await?;
    let torrent = wrap_info_dict(&info);
    if job.opts.bool("bt-save-metadata", false)
        || job.opts.bool("bt-metadata-only", false)
        || job.opts.bool("pause-metadata", false)
    {
        let path = job.opts.dir().join(format!("{}.torrent", hex::encode(info_hash)));
        if let Some(parent) = path.parent() {
            crate::storage::mkdirs(parent)?;
        }
        std::fs::write(&path, &torrent)?;
        if job.opts.bool("pause-metadata", false) && !job.opts.bool("bt-metadata-only", false) {
            return Err(Error::Bt(format!("pause-metadata:{}", path.display())));
        }
    }
    if job.opts.bool("bt-metadata-only", false) {
        return Ok(Swarm::Done);
    }
    let meta = MetaInfo::from_torrent(&torrent)?;
    job.progress.total.store(meta.length, Ordering::Relaxed);
    download_pieces(s, &meta, job, already_unchoked).await
}

async fn download_pieces<S: PeerWire>(
    s: &mut S,
    meta: &MetaInfo,
    job: &BtJob,
    mut unchoked: bool,
) -> Result<Swarm> {
    if job.opts.bool("bt-metadata-only", false) {
        return Ok(Swarm::Done);
    }
    s.send_bt(MSG_INTERESTED, &[]).await?;
    let stop = job.opts.u64("bt-stop-timeout", 0);
    let mut last_progress = tokio::time::Instant::now();
    let ka_iv = job.opts.u64("bt-keep-alive-interval", 20);
    let mut last_ka = tokio::time::Instant::now();
    let dest = dest_path(job, meta);
    let selected = parse_select_file(job.opts.get("select-file"), meta.files.len());
    let selected_len: u64 = meta
        .files
        .iter()
        .enumerate()
        .filter(|(i, _)| selected.get(*i).copied().unwrap_or(false))
        .map(|(_, f)| f.length)
        .sum();
    job.progress.total.store(selected_len, Ordering::Relaxed);
    let max_open = job.opts.usize("bt-max-open-files", 100);
    let pool = crate::storage::FilePool::new(max_open);
    crate::storage::reset_open_peak();
    let mut stores: Vec<Option<FileStorage>> = Vec::new();
    if meta.is_multi() {
        crate::storage::mkdirs(&dest)?;
        for (i, f) in meta.files.iter().enumerate() {
            if selected.get(i).copied().unwrap_or(false) {
                let alloc = crate::storage::alloc_mode_for(&job.opts, f.length);
                let st = FileStorage::from_opts(
                    file_out_path(&job.opts, &dest, meta, i, &f.path),
                    f.length,
                    alloc,
                    &job.opts,
                )
                    .with_pool(Arc::clone(&pool));
                st.ensure().await?;
                stores.push(Some(st));
            } else {
                stores.push(None);
            }
        }
    } else {
        let alloc = crate::storage::alloc_mode_for(&job.opts, meta.length);
        let store = FileStorage::from_opts(dest.clone(), meta.length, alloc, &job.opts)
            .with_pool(Arc::clone(&pool));
        store.ensure().await?;
        stores.push(Some(store));
    }
    let mut got = vec![false; meta.num_pieces()];
    for (i, g) in got.iter_mut().enumerate() {
        if !piece_needed(meta, i, &selected) {
            *g = true;
        }
    }
    if job.opts.bool("check-integrity", false) {
        let v = verify_existing(meta, &dest, &job.opts).await;
        for (i, g) in v.iter().enumerate() {
            if *g {
                got[i] = true;
            }
        }
        let done: u64 = got
            .iter()
            .enumerate()
            .filter(|(_, g)| **g)
            .map(|(i, _)| meta.piece_size(i) as u64)
            .sum();
        job.progress.completed.store(done, Ordering::Relaxed);
    }
    let mut pex_peers = Vec::new();
    let pex_on = job.opts.bool("enable-peer-exchange", true);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(job.opts.u64("timeout", 60).max(5));
    while got.iter().any(|g| !g) {
        if *job.cancel.borrow() {
            return Err(Error::Bt("canceled".into()));
        }
        if tokio::time::Instant::now() > deadline {
            return Err(Error::Bt("piece timeout".into()));
        }
        if !unchoked {
            match tokio::time::timeout(Duration::from_millis(500), s.recv_bt()).await {
                Ok(Ok(Some((MSG_UNCHOKE, _)))) => unchoked = true,
                Ok(Ok(Some((MSG_CHOKE, _)))) => unchoked = false,
                Ok(Ok(Some((MSG_EXT, p)))) if !p.is_empty() => {
                    if p[0] == UT_PEX_ID && pex_on {
                        if let Ok(v) = bencode::decode(&p[1..]) {
                            if let Some(a) = v.dict_get(b"added").and_then(|x| x.as_bytes()) {
                                for peer in parse_compact_peers(a) {
                                    if !pex_peers.contains(&peer) {
                                        pex_peers.push(peer);
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(Ok(None)) => {
                    if !pex_peers.is_empty() {
                        return Ok(Swarm::More(pex_peers));
                    }
                    return Err(Error::Bt("peer closed".into()));
                }
                Ok(Err(e)) => return Err(e),
                Err(_) => {
                    if ka_iv > 0 && last_ka.elapsed() >= Duration::from_secs(ka_iv) {
                        s.send_n(&0u32.to_be_bytes()).await?;
                        last_ka = tokio::time::Instant::now();
                    }
                }
                Ok(Ok(Some(_))) => {}
            }
            if !unchoked && !pex_peers.is_empty() {
                return Ok(Swarm::More(pex_peers));
            }
            if stop > 0 && last_progress.elapsed() >= Duration::from_secs(stop) {
                return Err(Error::Bt("bt-stop-timeout".into()));
            }
            continue;
        }
        let (head, tail) = parse_prioritize_piece(job.opts.get("bt-prioritize-piece"));
        let i = next_needed_piece(&got, meta, head, tail).unwrap();
        let bt_to = job.opts.u64("bt-timeout", 60).max(1);
        let (limit, label) = if stop > 0 {
            (
                stop.min(bt_to),
                if stop <= bt_to {
                    "bt-stop-timeout"
                } else {
                    "bt-timeout"
                },
            )
        } else {
            (bt_to, "bt-timeout")
        };
        let piece = match tokio::time::timeout(Duration::from_secs(limit), fetch_piece(s, meta, i, job))
            .await
        {
            Ok(r) => r?,
            Err(_) => return Err(Error::Bt(label.into())),
        };
        if Sha1::digest(&piece).as_slice() != meta.pieces[i] {
            return Err(Error::Bt(format!("piece {i} sha-1 mismatch")));
        }
        let off = i as u64 * meta.piece_length as u64;
        write_piece_bytes(meta, &stores, &selected, off, &piece).await?;
        got[i] = true;
        last_progress = tokio::time::Instant::now();
        job.progress.completed.fetch_add(piece.len() as u64, Ordering::Relaxed);
    }
    for st in stores.iter().flatten() {
        st.flush().await?;
    }
    if job.opts.bool("bt-remove-unselected-file", false) && meta.is_multi() {
        for (i, f) in meta.files.iter().enumerate() {
            if selected.get(i).copied().unwrap_or(false) {
                continue;
            }
            let p = file_out_path(&job.opts, &dest, meta, i, &f.path);
            let _ = crate::storage::unlink(&p);
        }
    }
    job.progress.completed.store(selected_len, Ordering::Relaxed);
    Ok(Swarm::Done)
}

async fn write_piece_bytes(
    meta: &MetaInfo,
    stores: &[Option<FileStorage>],
    selected: &[bool],
    off: u64,
    piece: &[u8],
) -> Result<()> {
    let end = off + piece.len() as u64;
    for (fi, f) in meta.files.iter().enumerate() {
        if !selected.get(fi).copied().unwrap_or(false) {
            continue;
        }
        let Some(store) = stores.get(fi).and_then(|s| s.as_ref()) else {
            continue;
        };
        let a0 = off.max(f.offset);
        let a1 = end.min(f.offset + f.length);
        if a1 <= a0 {
            continue;
        }
        let data_off = (a0 - off) as usize;
        let file_off = a0 - f.offset;
        store
            .write_body(file_off, &piece[data_off..data_off + (a1 - a0) as usize])
            .await?;
    }
    Ok(())
}

/// C++ `--seed-ratio=0` / `--seed-time=0` disable seeding. Unset ratio defaults to 1.0.
fn seed_plan(opts: &OptionSet) -> Option<(f64, Option<Duration>)> {
    let ratio = opts
        .get("seed-ratio")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(1.0);
    let mins = opts.get("seed-time").and_then(|s| s.parse::<f64>().ok());
    if ratio <= 0.0 || mins == Some(0.0) {
        return None;
    }
    let time = mins.map(|m| Duration::from_secs_f64((m * 60.0).max(0.05)));
    Some((ratio, time))
}

fn bitfield_all(n: usize) -> Vec<u8> {
    let mut v = vec![0u8; n.div_ceil(8)];
    for i in 0..n {
        v[i / 8] |= 0x80 >> (i % 8);
    }
    v
}

async fn verify_existing(meta: &MetaInfo, dest: &Path, opts: &OptionSet) -> Vec<bool> {
    let mut got = vec![false; meta.num_pieces()];
    let present = if meta.is_multi() {
        dest.is_dir() || parse_index_out(opts.get("index-out")).values().any(|p| {
            let q = if p.is_absolute() { p.clone() } else { opts.dir().join(p) };
            q.exists()
        })
    } else {
        dest.is_file()
    };
    if !present {
        return got;
    }
    for i in 0..meta.num_pieces() {
        let sz = meta.piece_size(i) as usize;
        if sz == 0 {
            got[i] = true;
            continue;
        }
        let off = i as u64 * meta.piece_length as u64;
        match read_torrent_range(meta, dest, opts, off, sz, None) {
            Ok(buf) if Sha1::digest(&buf).as_slice() == meta.pieces[i] => got[i] = true,
            _ => {}
        }
    }
    got
}

fn read_torrent_range(
    meta: &MetaInfo,
    dest: &Path,
    opts: &OptionSet,
    off: u64,
    len: usize,
    pool: Option<&crate::storage::FilePool>,
) -> Result<Vec<u8>> {
    let mut buf = vec![0u8; len];
    let end = off + len as u64;
    for (i, f) in meta.files.iter().enumerate() {
        let a0 = off.max(f.offset);
        let a1 = end.min(f.offset + f.length);
        if a1 <= a0 {
            continue;
        }
        let path = file_out_path(opts, dest, meta, i, &f.path);
        let n = (a1 - a0) as usize;
        let start = (a0 - off) as usize;
        let slice = &mut buf[start..start + n];
        if let Some(pool) = pool {
            pool.read_at(&path, a0 - f.offset, slice)?;
        } else {
            let got = crate::storage::pread_at(&path, slice, a0 - f.offset)?;
            if got != n {
                return Err(Error::Bt("short dest read".into()));
            }
        }
    }
    Ok(buf)
}

fn fire_bt_complete(meta: &MetaInfo, job: &BtJob) {
    let Some(cmd) = job
        .opts
        .get("on-bt-download-complete")
        .filter(|s| !s.is_empty())
    else {
        return;
    };
    let gid = job.opts.get("gid").unwrap_or("-");
    let dest = dest_path(job, meta);
    let mut parts = cmd.split_whitespace();
    let Some(prog) = parts.next() else {
        return;
    };
    let mut c = std::process::Command::new(prog);
    c.args(parts);
    c.arg(gid);
    c.arg("1");
    c.arg(dest.as_os_str());
    let _ = c.status();
}

async fn maybe_seed(meta: &MetaInfo, job: &BtJob) -> Result<()> {
    let Some((ratio, time_limit)) = seed_plan(&job.opts) else {
        return Ok(());
    };
    if job.opts.bool("bt-metadata-only", false) {
        return Ok(());
    }
    job.progress.seeding.store(true, Ordering::SeqCst);
    let r = maybe_seed_inner(meta, job, ratio, time_limit).await;
    job.progress.seeding.store(false, Ordering::SeqCst);
    r
}

async fn maybe_seed_inner(
    meta: &MetaInfo,
    job: &BtJob,
    ratio: f64,
    time_limit: Option<Duration>,
) -> Result<()> {
    let dest = dest_path(job, meta);
    let pool = crate::storage::FilePool::new(job.opts.usize("bt-max-open-files", 100));
    let listener = match bind_listen(&job.opts).await {
        Some(l) => l,
        None => return Ok(()),
    };
    let target = (ratio * meta.length as f64) as u64;
    let uploaded = AtomicU64::new(0);
    let start = tokio::time::Instant::now();
    let hard = Duration::from_secs(job.opts.u64("timeout", 60).max(1));
    // C++ `--bt-tracker-interval`: override tracker `interval`; 0 uses response (often 60+).
    let iv = job.opts.u64("bt-tracker-interval", 0);
    let mut last_ann = start;
    let urls = tracker_announce_urls(meta, &job.opts);
    loop {
        if *job.cancel.borrow() {
            break;
        }
        if uploaded.load(Ordering::Relaxed) >= target && target > 0 {
            break;
        }
        if let Some(t) = time_limit {
            if start.elapsed() >= t {
                break;
            }
        }
        if start.elapsed() >= hard {
            break;
        }
        if iv > 0 && last_ann.elapsed() >= Duration::from_secs(iv) {
            for url in &urls {
                let _ = announce_compact(url, meta, &job.opts).await;
            }
            last_ann = tokio::time::Instant::now();
        }
        let slice = time_limit
            .map(|t| t.saturating_sub(start.elapsed()))
            .unwrap_or(Duration::from_millis(250))
            .min(Duration::from_millis(250))
            .min(hard.saturating_sub(start.elapsed()));
        if slice.is_zero() {
            break;
        }
        match tokio::time::timeout(slice, listener.accept()).await {
            Ok(Ok((s, _))) => {
                let _ = crate::sockopt::apply_dscp(&s, &job.opts);
                let _ = seed_conn(
                    s,
                    meta,
                    &dest,
                    &uploaded,
                    target,
                    time_limit,
                    start,
                    &job.opts,
                    &job.progress.overall_up,
                    &pool,
                )
                .await;
                if uploaded.load(Ordering::Relaxed) >= target && target > 0 {
                    break;
                }
            }
            Ok(Err(_)) => break,
            Err(_) => {}
        }
    }
    Ok(())
}

async fn seed_conn(
    s: TcpStream,
    meta: &MetaInfo,
    dest: &Path,
    uploaded: &AtomicU64,
    target: u64,
    time_limit: Option<Duration>,
    start: tokio::time::Instant,
    opts: &crate::options::OptionSet,
    overall_up: &crate::http::OverallLimiter,
    pool: &crate::storage::FilePool,
) -> Result<()> {
    let _ = crate::sockopt::apply_tcp_nodelay(&s);
    let _ = crate::sockopt::apply_tcp_quickack(&s);
    let limit = crate::http::parse_speed(opts.get("max-upload-limit").unwrap_or("0")).unwrap_or(0);
    LAST_UPLOAD_LIMIT.store(limit, Ordering::SeqCst);
    if overall_up.limit() == 0 {
        if let Some(n) = crate::http::parse_speed(opts.get("max-overall-upload-limit").unwrap_or("0")) {
            overall_up.set_limit(n);
        }
    }
    LAST_OVERALL_UP.store(overall_up.limit(), Ordering::SeqCst);
    let pace = tokio::time::Instant::now();
    let mut paced = 0u64;
    let mut hs = [0u8; 68];
    let n = crate::sockopt::recv_exact(&s, &mut hs).await?;
    if n == 0 {
        return Err(Error::Bt("seed handshake eof".into()));
    }
    if hs[0] != 19 || &hs[1..20] != PSTR {
        return Err(Error::Bt("seed handshake".into()));
    }
    if hs[28..48] != meta.info_hash {
        return Err(Error::Bt("seed info_hash".into()));
    }
    let mine = peer_id();
    crate::sockopt::send_all(&s, &encode_handshake(&meta.info_hash, &mine)).await?;
    write_msg_tcp(&s, MSG_BITFIELD, &bitfield_all(meta.num_pieces())).await?;
    write_msg_tcp(&s, MSG_UNCHOKE, &[]).await?;
    loop {
        if uploaded.load(Ordering::Relaxed) >= target && target > 0 {
            break;
        }
        if let Some(t) = time_limit {
            if start.elapsed() >= t {
                break;
            }
        }
        match tokio::time::timeout(Duration::from_millis(500), read_msg_tcp(&s)).await {
            Ok(Ok(Some((MSG_REQUEST, p)))) if p.len() >= 12 => {
                let idx = u32::from_be_bytes(p[0..4].try_into().unwrap());
                let begin = u32::from_be_bytes(p[4..8].try_into().unwrap());
                let len = u32::from_be_bytes(p[8..12].try_into().unwrap()) as usize;
                if len == 0 || len > BLOCK as usize + 1 {
                    continue;
                }
                let off = idx as u64 * meta.piece_length as u64 + begin as u64;
                if off + len as u64 > meta.length {
                    continue;
                }
                let block = read_torrent_range(meta, dest, opts, off, len, Some(pool))?;
                if limit > 0 {
                    paced += block.len() as u64;
                    let expected_ms = paced.saturating_mul(1000) / limit.max(1);
                    let elapsed_ms = pace.elapsed().as_millis() as u64;
                    if elapsed_ms < expected_ms {
                        tokio::time::sleep(Duration::from_millis(expected_ms - elapsed_ms)).await;
                    }
                }
                overall_up.after(block.len() as u64).await;
                write_piece_tcp(&s, idx, begin, &block).await?;
                uploaded.fetch_add(block.len() as u64, Ordering::Relaxed);
            }
            Ok(Ok(Some((MSG_INTERESTED, _)))) => {
                write_msg_tcp(&s, MSG_UNCHOKE, &[]).await?;
            }
            Ok(Ok(None)) => break,
            Ok(Err(_)) => break,
            Err(_) => {}
            Ok(Ok(Some(_))) => {}
        }
    }
    Ok(())
}

async fn exchange_ut_metadata<S: PeerWire>(
    s: &mut S,
    info_hash: &[u8; 20],
    job: &BtJob,
) -> Result<(Vec<u8>, bool)> {
    s.send_bt(MSG_EXT, &ltep_handshake_payload(None, &job.opts)).await?;
    let mut peer_ut: Option<u8> = None;
    let mut meta_size: Option<usize> = None;
    let mut unchoked = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(job.opts.u64("timeout", 60).max(5));
    while peer_ut.is_none() || meta_size.is_none() {
        if tokio::time::Instant::now() > deadline {
            return Err(Error::Bt("ut_metadata handshake timeout".into()));
        }
        match tokio::time::timeout(Duration::from_secs(5), s.recv_bt()).await {
            Ok(Ok(Some((MSG_EXT, p)))) if p.first() == Some(&EXT_HANDSHAKE) => {
                let v = bencode::decode(&p[1..]).map_err(|e| Error::Bt(format!("ltep: {e}")))?;
                if let Some(id) = v
                    .dict_get(b"m")
                    .and_then(|m| m.dict_get(b"ut_metadata"))
                    .and_then(|x| x.as_int())
                {
                    if id > 0 && id <= 255 {
                        peer_ut = Some(id as u8);
                    }
                }
                if let Some(sz) = v.dict_get(b"metadata_size").and_then(|x| x.as_int()) {
                    if sz > 0 {
                        meta_size = Some(sz as usize);
                    }
                }
            }
            Ok(Ok(Some((MSG_UNCHOKE, _)))) => unchoked = true,
            Ok(Ok(Some((MSG_CHOKE, _)))) => unchoked = false,
            Ok(Ok(Some(_))) => {}
            Ok(Ok(None)) => return Err(Error::Bt("peer closed during metadata".into())),
            Ok(Err(e)) => return Err(e),
            Err(_) => continue,
        }
    }
    let peer_ut = peer_ut.ok_or_else(|| Error::Bt("peer has no ut_metadata".into()))?;
    let size = meta_size.ok_or_else(|| Error::Bt("no metadata_size".into()))?;
    if size > 4 * 1024 * 1024 {
        return Err(Error::Bt("metadata too large".into()));
    }
    let n = (size + META_BLOCK - 1) / META_BLOCK;
    let mut buf = vec![0u8; size];
    let mut have = vec![false; n];
    for i in 0..n {
        s.send_bt(MSG_EXT, &ut_metadata_request(peer_ut, i as i64)).await?;
    }
    while have.iter().any(|h| !h) {
        if tokio::time::Instant::now() > deadline {
            return Err(Error::Bt("ut_metadata piece timeout".into()));
        }
        match s.recv_bt().await? {
            Some((MSG_EXT, p)) if p.first() == Some(&UT_METADATA_ID) && p.len() > 1 => {
                let (hdr, data) = bencode::parse(&p[1..])?;
                let t = hdr.dict_get(b"msg_type").and_then(|x| x.as_int()).unwrap_or(-1);
                if t == 2 {
                    return Err(Error::Bt("metadata reject".into()));
                }
                if t != 1 {
                    continue;
                }
                let i = hdr
                    .dict_get(b"piece")
                    .and_then(|x| x.as_int())
                    .ok_or_else(|| Error::Bt("metadata piece idx".into()))? as usize;
                if i >= n {
                    continue;
                }
                let start = i * META_BLOCK;
                let end = (start + data.len()).min(size);
                if end < start {
                    continue;
                }
                buf[start..end].copy_from_slice(&data[..end - start]);
                have[i] = true;
            }
            Some((MSG_UNCHOKE, _)) => unchoked = true,
            Some((MSG_CHOKE, _)) => unchoked = false,
            Some(_) => {}
            None => return Err(Error::Bt("eof mid-metadata".into())),
        }
    }
    let digest = Sha1::digest(&buf);
    if digest.as_slice() != info_hash {
        return Err(Error::Bt("metadata info_hash mismatch".into()));
    }
    Ok((buf, unchoked))
}

async fn fetch_piece<S: PeerWire>(
    s: &mut S, meta: &MetaInfo, index: usize, job: &BtJob) -> Result<Vec<u8>> {
    let plen = meta.piece_size(index);
    let mut buf = vec![0u8; plen as usize];
    let mut have = vec![false; plen as usize];
    let cap = job.opts.usize("max-outstanding-request", 16).max(1);
    let mut next_off = 0u32;
    let mut inflight = 0usize;
    let req_to = Duration::from_secs(job.opts.u64("bt-request-timeout", 60).max(1));
    while have.iter().any(|h| !h) {
        while inflight < cap && next_off < plen {
            let n = (plen - next_off).min(BLOCK);
            let mut req = [0u8; 12];
            req[0..4].copy_from_slice(&(index as u32).to_be_bytes());
            req[4..8].copy_from_slice(&next_off.to_be_bytes());
            req[8..12].copy_from_slice(&n.to_be_bytes());
            s.send_bt(MSG_REQUEST, &req).await?;
            next_off += n;
            inflight += 1;
        }
        if inflight == 0 {
            break;
        }
        let msg = match tokio::time::timeout(req_to, s.recv_bt()).await {
            Ok(r) => r?,
            Err(_) => return Err(Error::Bt("bt-request-timeout".into())),
        };
        match msg {
            Some((MSG_PIECE, p)) if p.len() >= 8 => {
                let idx = u32::from_be_bytes(p[0..4].try_into().unwrap());
                let begin = u32::from_be_bytes(p[4..8].try_into().unwrap());
                if idx as usize != index {
                    continue;
                }
                let block = &p[8..];
                let start = begin as usize;
                let end = start + block.len();
                if end > buf.len() {
                    return Err(Error::Bt("piece overflow".into()));
                }
                let already = have.get(start).copied().unwrap_or(false);
                buf[start..end].copy_from_slice(block);
                for h in have.iter_mut().skip(start).take(block.len()) {
                    *h = true;
                }
                if !already {
                    inflight = inflight.saturating_sub(1);
                }
            }
            Some((MSG_CHOKE, _)) => return Err(Error::Bt("choked".into())),
            Some((MSG_UNCHOKE | MSG_HAVE | MSG_BITFIELD | 255, _)) => {}
            Some(_) => {}
            None => return Err(Error::Bt("eof mid-piece".into())),
        }
    }
    Ok(buf)
}

/// RFC 4648 base64 (C++ `aria2.addTorrent` torrent argument).
pub fn b64_decode(s: &str) -> Result<Vec<u8>> {
    fn val(c: u8) -> Result<u8> {
        Ok(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err(Error::Bt("base64".into())),
        })
    }
    let s: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    if s.len() % 4 != 0 {
        return Err(Error::Bt("base64 len".into()));
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    for c in s.chunks(4) {
        let pad = c.iter().filter(|&&x| x == b'=').count();
        let a = val(c[0])?;
        let b = val(c[1])?;
        let d = if c[2] == b'=' { 0 } else { val(c[2])? };
        let e = if c[3] == b'=' { 0 } else { val(c[3])? };
        out.push((a << 2) | (b >> 4));
        if pad < 2 {
            out.push((b << 4) | (d >> 2));
        }
        if pad < 1 {
            out.push((d << 6) | e);
        }
    }
    Ok(out)
}

pub fn b64_encode(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut o = String::new();
    for c in data.chunks(3) {
        let a = c[0] as u32;
        let b = if c.len() > 1 { c[1] as u32 } else { 0 };
        let d = if c.len() > 2 { c[2] as u32 } else { 0 };
        let n = (a << 16) | (b << 8) | d;
        o.push(T[(n >> 18) as usize] as char);
        o.push(T[((n >> 12) & 63) as usize] as char);
        o.push(if c.len() > 1 { T[((n >> 6) & 63) as usize] as char } else { '=' });
        o.push(if c.len() > 2 { T[(n & 63) as usize] as char } else { '=' });
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn torrent_infohash_stable() {
        let data = vec![7u8; 32 * 1024 + 100];
        let t = build_single_file("a.bin", 16 * 1024, &data, "http://127.0.0.1:1/announce");
        let m = MetaInfo::from_torrent(&t).unwrap();
        assert_eq!(m.name, "a.bin");
        assert_eq!(m.length, data.len() as u64);
        assert_eq!(m.num_pieces(), 3);
        let t2 = build_single_file("a.bin", 16 * 1024, &data, "http://127.0.0.1:1/announce");
        assert_eq!(MetaInfo::from_torrent(&t2).unwrap().info_hash, m.info_hash);
    }

    #[test]
    fn magnet_x_pe() {
        let p = peers_from_magnet("magnet:?xt=urn:btih:abc&x.pe=127.0.0.1:6881");
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].port(), 6881);
    }

    #[test]
    fn b64_roundtrip() {
        let v = b"hello torrent";
        assert_eq!(b64_decode(&b64_encode(v)).unwrap(), v);
    }

    #[test]
    fn peer_id_prefix_pad_and_truncate() {
        let mut opts = OptionSet::new();
        opts.set("peer-id-prefix", "GRK-");
        let id = peer_id_from_opts(&opts);
        assert_eq!(&id[..4], b"GRK-");
        let mut long = OptionSet::new();
        long.set("peer-id-prefix", "ABCDEFGHIJKLMNOPQRSTUVWXYZ");
        let id2 = peer_id_from_opts(&long);
        assert_eq!(&id2, b"ABCDEFGHIJKLMNOPQRST");
    }

    #[test]
    fn listen_port_range_and_list_parse() {
        assert_eq!(parse_listen_ports("6881"), vec![6881]);
        assert_eq!(parse_listen_ports("6881-6883"), vec![6881, 6882, 6883]);
        assert_eq!(parse_listen_ports("6885,6881"), vec![6885, 6881]);
        assert_eq!(parse_listen_ports("10-12,20"), vec![10, 11, 12, 20]);
        assert_eq!(parse_listen_ports(""), vec![6881]);
    }

    #[test]
    fn peer_agent_in_ltep_v() {
        let mut opts = OptionSet::new();
        opts.set("peer-agent", "GrokBT");
        opts.set("enable-peer-exchange", "false");
        let p = ltep_handshake_payload(None, &opts);
        assert_eq!(p[0], EXT_HANDSHAKE);
        let v = bencode::decode(&p[1..]).unwrap();
        assert_eq!(v.dict_get(b"v").and_then(|x| x.as_bytes()), Some(&b"GrokBT"[..]));
        let def = ltep_handshake_payload(None, &OptionSet::new());
        let v2 = bencode::decode(&def[1..]).unwrap();
        assert_eq!(
            v2.dict_get(b"v").and_then(|x| x.as_bytes()),
            Some(&b"aria2-rust"[..])
        );
    }

    #[test]
    fn magnet_xt_hex_and_base32() {
        let data = vec![9u8; 1024];
        let t = build_single_file("m.bin", 16 * 1024, &data, "http://127.0.0.1:1/announce");
        let m = MetaInfo::from_torrent(&t).unwrap();
        let hex = hex::encode(m.info_hash);
        let uri = format!("magnet:?xt=urn:btih:{hex}&dn=m.bin");
        assert_eq!(info_hash_from_magnet(&uri).unwrap(), m.info_hash);
        assert_eq!(magnet_dn(&uri).as_deref(), Some("m.bin"));
        let b32 = {
            const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
            let mut out = String::new();
            let mut acc = 0u64;
            let mut nbits = 0u32;
            for b in m.info_hash {
                acc = (acc << 8) | b as u64;
                nbits += 8;
                while nbits >= 5 {
                    nbits -= 5;
                    out.push(A[((acc >> nbits) & 31) as usize] as char);
                    acc &= (1u64 << nbits) - 1;
                }
            }
            if nbits > 0 {
                out.push(A[((acc << (5 - nbits)) & 31) as usize] as char);
            }
            out
        };
        assert_eq!(b32.len(), 32);
        let uri32 = format!("magnet:?xt=urn:btih:{b32}");
        assert_eq!(info_hash_from_magnet(&uri32).unwrap(), m.info_hash);
    }

    #[test]
    fn compact_peers_roundtrip() {
        let a: SocketAddr = "127.0.0.1:6881".parse().unwrap();
        let b: SocketAddr = "10.0.0.9:51413".parse().unwrap();
        let enc = encode_compact_peers(&[a, b]);
        assert_eq!(enc.len(), 12);
        let p = parse_compact_peers(&enc);
        assert_eq!(p, vec![a, b]);
    }

    #[test]
    fn compact_peers6_roundtrip() {
        let a: SocketAddr = "[::1]:6881".parse().unwrap();
        let b: SocketAddr = "[2001:db8::9]:51413".parse().unwrap();
        let enc = encode_compact_peers6(&[a, b]);
        assert_eq!(enc.len(), 36);
        let p = parse_compact_peers6(&enc);
        assert_eq!(p, vec![a, b]);
    }

    #[test]
    fn bt_exclude_tracker_glob() {
        assert!(tracker_excluded("http://a/announce", Some("*")));
        assert!(tracker_excluded("http://bad/announce", Some("http://bad/*")));
        assert!(!tracker_excluded("http://good/announce", Some("http://bad/*")));
        assert!(!tracker_excluded("http://a/announce", None));
        assert!(!tracker_excluded("http://a/announce", Some("")));
    }

    #[test]
    fn select_file_indexes_and_ranges() {
        assert_eq!(parse_select_file(None, 3), vec![true, true, true]);
        assert_eq!(parse_select_file(Some(""), 2), vec![true, true]);
        assert_eq!(parse_select_file(Some("1"), 3), vec![true, false, false]);
        assert_eq!(parse_select_file(Some("2-3,1"), 3), vec![true, true, true]);
        assert_eq!(parse_select_file(Some("2"), 2), vec![false, true]);
        assert_eq!(parse_select_file(Some("9"), 2), vec![false, false]);
    }

    #[test]
    fn index_out_parse() {
        let m = parse_index_out(Some("1=custom.bin"));
        assert_eq!(m.get(&1).unwrap(), &PathBuf::from("custom.bin"));
        let m = parse_index_out(Some("1=a.bin\n2=/tmp/b.bin"));
        assert_eq!(m.get(&1).unwrap(), &PathBuf::from("a.bin"));
        assert_eq!(m.get(&2).unwrap(), &PathBuf::from("/tmp/b.bin"));
        assert!(parse_index_out(None).is_empty());
    }

    #[test]
    fn show_files_lists_multi() {
        let a = vec![1u8; 16];
        let b = vec![2u8; 16];
        let mut concat = a.clone();
        concat.extend_from_slice(&b);
        let t = build_multi_file(
            "sf",
            16,
            &[("one.bin", &concat[..16]), ("two.bin", &concat[16..])],
            "http://127.0.0.1:1/announce",
        );
        let listing = format_show_files(&t).unwrap();
        assert!(listing.contains("one.bin"), "{listing}");
        assert!(listing.contains("two.bin"), "{listing}");
        assert!(listing.contains("16B"), "{listing}");
    }

    #[test]
    fn prioritize_piece_head_tail_parse() {
        assert_eq!(parse_prioritize_piece(None), (0, 0));
        assert_eq!(parse_prioritize_piece(Some("")), (0, 0));
        assert_eq!(parse_prioritize_piece(Some("head")), (1024 * 1024, 0));
        assert_eq!(parse_prioritize_piece(Some("tail=32K")), (0, 32 * 1024));
        assert_eq!(
            parse_prioritize_piece(Some("head=16K,tail=8K")),
            (16 * 1024, 8 * 1024)
        );
    }
}
