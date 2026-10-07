//! BitTorrent piece wire (BEP 3) dest-match: handshake, request/piece, SHA-1.
#![forbid(unsafe_code)]

use aria2_rust::bencode::{self, BVal};
use aria2_rust::bt::{
    self, encode_compact_peers, encode_compact_peers6, encode_handshake, parse_compact_peers6, peer_id, read_msg, write_msg, BtJob, MetaInfo,
    EXT_HANDSHAKE, META_BLOCK, MSG_EXT, MSG_INTERESTED, MSG_PIECE, MSG_REQUEST, MSG_UNCHOKE, PSTR,
    UT_METADATA_ID, UT_PEX_ID,
};
use aria2_rust::http::HttpProgress;
use aria2_rust::options::OptionSet;
use aria2_rust::session::Session;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::watch;
use tokio_rustls::TlsAcceptor;

const PIECE_LEN: u32 = 32 * 1024;

static LAST_CLIENT_PEER_ID: Mutex<[u8; 20]> = Mutex::new([0u8; 20]);
static LAST_PEER_AGENT: Mutex<String> = Mutex::new(String::new());

fn payload() -> &'static [u8] {
    Box::leak(
        (0u8..=255)
            .cycle()
            .take(50 * 1024 + 77)
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    )
}

fn job_opts() -> OptionSet {
    let mut opts = OptionSet::with_defaults();
    opts.set("file-allocation", "none");
    opts.set("timeout", "15");
    opts.set("connect-timeout", "5");
    opts.set("enable-dht", "false");
    opts.set("dht-listen-port", "0");
    opts.set("seed-ratio", "0");
    opts
}

async fn serve_peer(mut s: TcpStream, info_hash: [u8; 20], payload: &'static [u8], corrupt: bool) {
    s.set_nodelay(true).ok();
    serve_peer_io(&mut s, info_hash, payload, corrupt, None).await;
}

async fn serve_peer_io<S: AsyncReadExt + AsyncWriteExt + Unpin>(
    s: &mut S,
    info_hash: [u8; 20],
    payload: &'static [u8],
    corrupt: bool,
    first_piece: Option<Arc<AtomicU16>>,
) {
    let mut hs = [0u8; 68];
    if s.read_exact(&mut hs).await.is_err() {
        return;
    }
    if hs[0] != 19 || &hs[1..20] != PSTR {
        return;
    }
    if hs[28..48] != info_hash {
        return;
    }
    if let Ok(mut g) = LAST_CLIENT_PEER_ID.lock() {
        g.copy_from_slice(&hs[48..68]);
    }
    let mine = peer_id();
    if s.write_all(&encode_handshake(&info_hash, &mine)).await.is_err() {
        return;
    }
    if write_msg(s, MSG_UNCHOKE, &[]).await.is_err() {
        return;
    }
    loop {
        match tokio::time::timeout(Duration::from_secs(10), read_msg(s)).await {
            Ok(Ok(Some((MSG_REQUEST, p)))) if p.len() >= 12 => {
                let idx = u32::from_be_bytes(p[0..4].try_into().unwrap());
                if let Some(fp) = &first_piece {
                    let _ = fp.compare_exchange(
                        u16::MAX,
                        idx as u16,
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    );
                }
                let begin = u32::from_be_bytes(p[4..8].try_into().unwrap());
                let len = u32::from_be_bytes(p[8..12].try_into().unwrap()) as usize;
                let start = idx as usize * PIECE_LEN as usize + begin as usize;
                let end = start.saturating_add(len);
                if end > payload.len() {
                    return;
                }
                let mut block = payload[start..end].to_vec();
                if corrupt && !block.is_empty() {
                    block[0] ^= 0xFF;
                }
                let mut body = Vec::with_capacity(8 + block.len());
                body.extend_from_slice(&idx.to_be_bytes());
                body.extend_from_slice(&begin.to_be_bytes());
                body.extend_from_slice(&block);
                if write_msg(s, MSG_PIECE, &body).await.is_err() {
                    return;
                }
            }
            Ok(Ok(Some((MSG_INTERESTED, _)))) => {}
            Ok(Ok(Some((MSG_EXT, p)))) if !p.is_empty() && p[0] == EXT_HANDSHAKE => {
                if let Ok(v) = bencode::decode(&p[1..]) {
                    if let Some(agent) = v.dict_get(b"v").and_then(|x| x.as_bytes()) {
                        if let Ok(mut g) = LAST_PEER_AGENT.lock() {
                            *g = String::from_utf8_lossy(agent).into_owned();
                        }
                    }
                }
            }
            Ok(Ok(Some(_))) => {}
            _ => return,
        }
    }
}

async fn spawn_seeder(info_hash: [u8; 20], payload: &'static [u8], corrupt: bool) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((s, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                serve_peer(s, info_hash, payload, corrupt).await;
            });
        }
    });
    port
}

async fn spawn_prio_seeder(
    info_hash: [u8; 20],
    payload: &'static [u8],
    first_piece: Arc<AtomicU16>,
) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else {
                break;
            };
            let first_piece = Arc::clone(&first_piece);
            tokio::spawn(async move {
                s.set_nodelay(true).ok();
                serve_peer_io(&mut s, info_hash, payload, false, Some(first_piece)).await;
            });
        }
    });
    port
}

async fn spawn_http_body(body: &'static [u8]) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 2048];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                if req.is_empty() {
                    return;
                }
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(body).await;
            });
        }
    });
    port
}

async fn spawn_mse_seeder(info_hash: [u8; 20], payload: &'static [u8], allow: u32) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((s, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                s.set_nodelay(true).ok();
                let Ok(mut s) = aria2_rust::mse::responder(s, &info_hash, allow).await else {
                    return;
                };
                serve_peer_io(&mut s, info_hash, payload, false, None).await;
            });
        }
    });
    port
}

struct TrackerSeen {
    port: AtomicU16,
}

async fn spawn_tracker(seeder: u16, seen: Arc<TrackerSeen>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else {
                break;
            };
            let seen = Arc::clone(&seen);
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let line = req.lines().next().unwrap_or("");
                if let Some(q) = line.split('?').nth(1).and_then(|s| s.split(' ').next()) {
                    for part in q.split('&') {
                        if let Some(v) = part.strip_prefix("port=") {
                            if let Ok(p) = v.parse::<u16>() {
                                seen.port.store(p, Ordering::SeqCst);
                            }
                        }
                    }
                }
                let mut peers = Vec::with_capacity(6);
                peers.extend_from_slice(&[127, 0, 0, 1]);
                peers.extend_from_slice(&seeder.to_be_bytes());
                let body = aria2_rust::bencode::encode(&BVal::Dict(vec![
                    (b"interval".to_vec(), BVal::Int(60)),
                    (b"peers".to_vec(), BVal::Bytes(peers)),
                ]));
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(&body).await;
            });
        }
    });
    port
}

async fn spawn_tracker_delayed(seeder: u16, delay: Duration) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let _ = s.read(&mut buf).await;
                tokio::time::sleep(delay).await;
                let mut peers = Vec::with_capacity(6);
                peers.extend_from_slice(&[127, 0, 0, 1]);
                peers.extend_from_slice(&seeder.to_be_bytes());
                let body = aria2_rust::bencode::encode(&BVal::Dict(vec![
                    (b"interval".to_vec(), BVal::Int(60)),
                    (b"peers".to_vec(), BVal::Bytes(peers)),
                ]));
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(&body).await;
            });
        }
    });
    port
}

async fn spawn_tracker_counting(seeder: u16, hits: Arc<AtomicU64>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else {
                break;
            };
            let hits = Arc::clone(&hits);
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                if req.contains("info_hash=") {
                    hits.fetch_add(1, Ordering::SeqCst);
                }
                let mut peers = Vec::with_capacity(6);
                peers.extend_from_slice(&[127, 0, 0, 1]);
                peers.extend_from_slice(&seeder.to_be_bytes());
                let body = aria2_rust::bencode::encode(&BVal::Dict(vec![
                    (b"interval".to_vec(), BVal::Int(60)),
                    (b"peers".to_vec(), BVal::Bytes(peers)),
                ]));
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(&body).await;
            });
        }
    });
    port
}

async fn spawn_tracker_ip(seeder: u16, seen: Arc<Mutex<String>>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else {
                break;
            };
            let seen = Arc::clone(&seen);
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let line = req.lines().next().unwrap_or("");
                if let Some(q) = line.split('?').nth(1).and_then(|s| s.split(' ').next()) {
                    for part in q.split('&') {
                        if let Some(v) = part.strip_prefix("ip=") {
                            if let Ok(mut g) = seen.lock() {
                                *g = v.to_string();
                            }
                        }
                    }
                }
                let mut peers = Vec::with_capacity(6);
                peers.extend_from_slice(&[127, 0, 0, 1]);
                peers.extend_from_slice(&seeder.to_be_bytes());
                let body = aria2_rust::bencode::encode(&BVal::Dict(vec![
                    (b"interval".to_vec(), BVal::Int(60)),
                    (b"peers".to_vec(), BVal::Bytes(peers)),
                ]));
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(&body).await;
            });
        }
    });
    port
}

fn magnet_pe(addr: SocketAddr) -> String {
    format!("magnet:?xt=urn:btih:0000000000000000000000000000000000000000&x.pe={addr}")
}

async fn spawn_http_bytes(body: Vec<u8>, filename: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else {
                break;
            };
            let body = body.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                if req.starts_with("HEAD ") {
                    let hdr = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nContent-Type: application/x-bittorrent\r\nContent-Disposition: attachment; filename=\"{filename}\"\r\n\r\n",
                        body.len()
                    );
                    let _ = s.write_all(hdr.as_bytes()).await;
                    return;
                }
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nContent-Type: application/x-bittorrent\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(&body).await;
            });
        }
    });
    port
}

async fn wait_complete(session: &Session, gid: &str) -> Value {
    for _ in 0..200 {
        let st = session.tell_status(gid).await.unwrap();
        match st.get("status").and_then(|v| v.as_str()) {
            Some("complete") => return st,
            Some("error") => panic!("bt error: {st}"),
            _ => {}
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("bt download did not complete");
}

#[tokio::test]
async fn bt_piece_wire_sha1_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("blob.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    assert!(meta.num_pieces() >= 2, "need multi-piece payload");
    let port = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("blob.bin");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{port}").parse().unwrap()],
        opts: job_opts(),
        progress: HttpProgress::new(),
        cancel: rx,
    };
    aria2_rust::sockopt::reset_writev();
    aria2_rust::sockopt::reset_recv();
    aria2_rust::sockopt::reset_send();
    bt::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body, "dest must match seeder bytes");
    assert!(
        aria2_rust::sockopt::last_writev() >= 1,
        "C++ SocketBuffer::send must writev BT REQUEST"
    );
    assert!(
        aria2_rust::sockopt::last_recv() >= 1,
        "C++ SocketCore::readData recv must read leech handshake/PIECE"
    );
    assert!(
        aria2_rust::sockopt::last_send() >= 1,
        "C++ SocketCore::writeData send must write leech handshake"
    );
}

async fn spawn_wait_n_request_seeder(
    info_hash: [u8; 20],
    payload: &'static [u8],
    need: usize,
) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                s.set_nodelay(true).ok();
                let mut hs = [0u8; 68];
                if s.read_exact(&mut hs).await.is_err() {
                    return;
                }
                if hs[0] != 19 || &hs[1..20] != PSTR {
                    return;
                }
                if hs[28..48] != info_hash {
                    return;
                }
                let mine = peer_id();
                if s.write_all(&encode_handshake(&info_hash, &mine)).await.is_err() {
                    return;
                }
                if write_msg(&mut s, MSG_UNCHOKE, &[]).await.is_err() {
                    return;
                }
                let mut pending: Vec<(u32, u32, u32)> = Vec::new();
                loop {
                    match tokio::time::timeout(Duration::from_secs(10), read_msg(&mut s)).await {
                        Ok(Ok(Some((MSG_REQUEST, p)))) if p.len() >= 12 => {
                            let idx = u32::from_be_bytes(p[0..4].try_into().unwrap());
                            let begin = u32::from_be_bytes(p[4..8].try_into().unwrap());
                            let len = u32::from_be_bytes(p[8..12].try_into().unwrap());
                            pending.push((idx, begin, len));
                            if pending.len() < need {
                                continue;
                            }
                            for (idx, begin, len) in pending.drain(..) {
                                let start = idx as usize * PIECE_LEN as usize + begin as usize;
                                let end = start.saturating_add(len as usize);
                                if end > payload.len() {
                                    return;
                                }
                                let block = &payload[start..end];
                                let mut body = Vec::with_capacity(8 + block.len());
                                body.extend_from_slice(&idx.to_be_bytes());
                                body.extend_from_slice(&begin.to_be_bytes());
                                body.extend_from_slice(block);
                                if write_msg(&mut s, MSG_PIECE, &body).await.is_err() {
                                    return;
                                }
                            }
                        }
                        Ok(Ok(Some((MSG_INTERESTED, _)))) => {}
                        Ok(Ok(Some(_))) => {}
                        _ => return,
                    }
                }
            });
        }
    });
    port
}

#[tokio::test]
async fn max_outstanding_request_two_wait2_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("mor2.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    assert!(
        meta.piece_size(0) > 16 * 1024,
        "need two 16KiB blocks per piece"
    );
    let port = spawn_wait_n_request_seeder(meta.info_hash, body, 2).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("mor2.bin");
    let mut opts = job_opts();
    opts.set("max-outstanding-request", "2");
    opts.set("bt-request-timeout", "5");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{port}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--max-outstanding-request=2 must dest-match wait-2 seeder"
    );
}

#[tokio::test]
async fn max_outstanding_request_one_wait2_miss() {
    let body = payload();
    let torrent = bt::build_single_file("mor1.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let port = spawn_wait_n_request_seeder(meta.info_hash, body, 2).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("mor1.bin");
    let mut opts = job_opts();
    opts.set("max-outstanding-request", "1");
    opts.set("bt-request-timeout", "1");
    opts.set("bt-timeout", "3");
    opts.set("timeout", "4");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{port}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let err = bt::download(job).await;
    assert!(
        err.is_err(),
        "--max-outstanding-request=1 must miss wait-2 seeder: {err:?}"
    );
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "--max-outstanding-request=1 must not dest-match wait-2 seeder"
    );
}

#[tokio::test]
async fn max_outstanding_request_one_serial_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("mor1s.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let port = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("mor1s.bin");
    let mut opts = job_opts();
    opts.set("max-outstanding-request", "1");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{port}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--max-outstanding-request=1 serial dest must match seeder bytes"
    );
}

#[tokio::test]
async fn dscp_bt_piece_wire_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("dscp.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let port = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("dscp.bin");
    let (_tx, rx) = watch::channel(false);
    let mut opts = job_opts();
    opts.set("dscp", "46");
    aria2_rust::sockopt::reset_last();
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{port}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body, "dscp dest must match seeder bytes");
    let tos = aria2_rust::sockopt::last_dscp_tos();
    assert_eq!(tos, 46 << 2, "BT peer socket IP_TOS must be DSCP<<2, got {tos}");
}

#[tokio::test]
async fn bt_piece_sha1_mismatch_rejected() {
    let body = payload();
    let torrent = bt::build_single_file("bad.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let port = spawn_seeder(meta.info_hash, body, true).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("bad.bin");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest,
        peers: vec![format!("127.0.0.1:{port}").parse().unwrap()],
        opts: job_opts(),
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let err = bt::download(job).await;
    assert!(err.is_err(), "corrupt piece must fail SHA-1");
    let msg = err.unwrap_err().to_string();
    assert!(
        msg.contains("sha-1") || msg.contains("mismatch"),
        "error must mention sha-1, got {msg}"
    );
}

#[tokio::test]
async fn bt_tracker_compact_bt_tracker_listen_port() {
    let body = payload();
    let torrent = bt::build_single_file("trk.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let seen = Arc::new(TrackerSeen {
        port: AtomicU16::new(0),
    });
    let trk = spawn_tracker(seed, Arc::clone(&seen)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("trk.bin");
    let mut opts = job_opts();
    opts.set("listen-port", "12345");
    opts.set(
        "bt-tracker",
        format!("http://127.0.0.1:1/nope,http://127.0.0.1:{trk}/announce"),
    );
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    aria2_rust::sockopt::reset_send();
    aria2_rust::sockopt::reset_recv();
    bt::reset_tracker_io();
    bt::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert!(
        bt::last_tracker_send() >= 1,
        "C++ DefaultBtAnnounce must SocketCore::writeData send tracker GET"
    );
    assert!(
        bt::last_tracker_recv() >= 1,
        "C++ DefaultBtAnnounce must SocketCore::readData recv compact peers"
    );
    assert_eq!(
        seen.port.load(Ordering::SeqCst),
        12345,
        "--listen-port must appear in tracker announce"
    );
}

async fn spawn_udp_tracker(seeder: u16) -> u16 {
    spawn_udp_tracker_drops(seeder, 0).await
}

async fn spawn_udp_tracker_drops(seeder: u16, drops: u32) -> u16 {
    let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let port = sock.local_addr().unwrap().port();
    tokio::spawn(async move {
        let mut buf = [0u8; 128];
        let mut left = drops;
        loop {
            let Ok((n, from)) = sock.recv_from(&mut buf).await else {
                break;
            };
            if left > 0 {
                left -= 1;
                continue;
            }
            if n >= 16 {
                let action = u32::from_be_bytes(buf[8..12].try_into().unwrap());
                if action == 0 {
                    let tx = &buf[12..16];
                    let mut r = [0u8; 16];
                    r[4..8].copy_from_slice(tx);
                    r[8..16].copy_from_slice(&0x1122_3344_5566_7788u64.to_be_bytes());
                    let _ = sock.send_to(&r, from).await;
                    continue;
                }
            }
            if n >= 98 {
                let action = u32::from_be_bytes(buf[8..12].try_into().unwrap());
                if action == 1 {
                    let tx = &buf[12..16];
                    let mut r = Vec::with_capacity(26);
                    r.extend_from_slice(&1u32.to_be_bytes());
                    r.extend_from_slice(tx);
                    r.extend_from_slice(&60u32.to_be_bytes());
                    r.extend_from_slice(&0u32.to_be_bytes());
                    r.extend_from_slice(&1u32.to_be_bytes());
                    r.extend_from_slice(&[127, 0, 0, 1]);
                    r.extend_from_slice(&seeder.to_be_bytes());
                    let _ = sock.send_to(&r, from).await;
                }
            }
        }
    });
    port
}

#[tokio::test]
async fn bt_udp_tracker_bep15_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("utrk.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let trk = spawn_udp_tracker(seed).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("utrk.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "utrk.bin");
    opts.set("bt-exclude-tracker", "*");
    opts.set("bt-tracker", format!("udp://127.0.0.1:{trk}/announce"));
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::reset_tracker_io();
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "C++ DefaultBtAnnounce UDP BEP 15 must dest-match compact peers"
    );
    assert!(
        bt::last_tracker_send() >= 2,
        "UDP connect+announce must SocketCore::writeData send"
    );
    assert!(
        bt::last_tracker_recv() >= 2,
        "UDP connect+announce must SocketCore::readData recv"
    );
}

#[tokio::test]
async fn bt_udp_tracker_retransmit_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("urtx.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let trk = spawn_udp_tracker_drops(seed, 1).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("urtx.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "urtx.bin");
    opts.set("bt-exclude-tracker", "*");
    opts.set("bt-tracker", format!("udp://127.0.0.1:{trk}/announce"));
    opts.set("bt-tracker-timeout", "3");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::reset_tracker_io();
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "C++ UDPTrackerClient retransmit must dest-match after dropped datagram"
    );
    assert!(
        bt::last_tracker_send() >= 3,
        "BEP 15 retransmit must extra SocketCore::writeData send, got {}",
        bt::last_tracker_send()
    );
}

const HTTPS_CERT: &[u8] = include_bytes!("fixtures/https-server.der");
const HTTPS_KEY: &[u8] = include_bytes!("fixtures/https-key.der");
const HTTPS_CA: &str = include_str!("fixtures/https-ca.pem");

fn https_ca_file() -> (tempfile::NamedTempFile, String) {
    let f = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(f.path(), HTTPS_CA).unwrap();
    let path = f.path().display().to_string();
    (f, path)
}

async fn spawn_https_tracker(seeder: u16) -> u16 {
    let provider = rustls::crypto::ring::default_provider();
    let certs = vec![rustls::pki_types::CertificateDer::from(HTTPS_CERT.to_vec())];
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(
        rustls::pki_types::PrivatePkcs8KeyDer::from(HTTPS_KEY.to_vec()),
    );
    let mut cfg = rustls::ServerConfig::builder_with_provider(provider.into())
        .with_protocol_versions(&[&rustls::version::TLS12, &rustls::version::TLS13])
        .expect("tls")
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .expect("cert");
    cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
    let acceptor = TlsAcceptor::from(std::sync::Arc::new(cfg));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                break;
            };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(mut s) = acceptor.accept(tcp).await else {
                    return;
                };
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    return;
                }
                let mut peers = Vec::with_capacity(6);
                peers.extend_from_slice(&[127, 0, 0, 1]);
                peers.extend_from_slice(&seeder.to_be_bytes());
                let body = aria2_rust::bencode::encode(&BVal::Dict(vec![
                    (b"interval".to_vec(), BVal::Int(60)),
                    (b"peers".to_vec(), BVal::Bytes(peers)),
                ]));
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(&body).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn bt_https_tracker_socketcore_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("htrk.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let trk = spawn_https_tracker(seed).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("htrk.bin");
    let (_ca, ca_path) = https_ca_file();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "htrk.bin");
    opts.set("bt-exclude-tracker", "*");
    opts.set("bt-tracker", format!("https://127.0.0.1:{trk}/announce"));
    opts.set("ca-certificate", &ca_path);
    opts.set("check-certificate", "true");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::reset_tracker_io();
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "C++ DefaultBtAnnounce HTTPS TLS SocketCore must dest-match compact peers"
    );
    assert!(
        bt::last_tracker_send() >= 1,
        "HTTPS tracker GET must TLS writeData send"
    );
    assert!(
        bt::last_tracker_recv() >= 1,
        "HTTPS tracker GET must TLS readData recv"
    );
}

#[tokio::test]
async fn listen_port_range_skips_busy_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("lpr.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let occupy = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let busy = occupy.local_addr().unwrap().port();
    let alt = if busy < 65535 { busy + 1 } else { busy - 1 };
    match TcpListener::bind(("127.0.0.1", alt)).await {
        Ok(free) => drop(free),
        Err(_) => {
            drop(occupy);
            panic!("listen-port range test needs a free neighbor of {busy}");
        }
    }
    let lo = busy.min(alt);
    let hi = busy.max(alt);
    let dir = tempfile::tempdir().unwrap();
    let dest1 = dir.path().join("lpr.bin");
    let dest2 = dir.path().join("leech.bin");
    let mut opts1 = job_opts();
    opts1.set("dir", dir.path().display().to_string());
    opts1.set("out", "lpr.bin");
    opts1.set("seed-ratio", "1.0");
    opts1.set("listen-port", format!("{lo}-{hi}"));
    opts1.set("timeout", "15");
    let mut opts2 = job_opts();
    opts2.set("dir", dir.path().display().to_string());
    opts2.set("out", "leech.bin");
    opts2.set("seed-ratio", "0");
    opts2.set("connect-timeout", "8");
    opts2.set("timeout", "15");
    let torrent2 = torrent.clone();
    let (_tx1, rx1) = watch::channel(false);
    let job1 = BtJob {
        torrent,
        dest: dest1.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts: opts1,
        progress: HttpProgress::new(),
        cancel: rx1,
    };
    let seeder_task = tokio::spawn(async move { bt::download(job1).await });
    let t0 = tokio::time::Instant::now();
    loop {
        if dest1.exists() && std::fs::read(&dest1).ok().as_deref() == Some(body) {
            break;
        }
        if t0.elapsed() > Duration::from_secs(8) {
            panic!("seeder dest never matched");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let bound = loop {
        let p = bt::last_listen_port();
        if p != 0 && p != busy {
            break p;
        }
        if t0.elapsed() > Duration::from_secs(10) {
            panic!("listen-port range never bound alt, last={}", bt::last_listen_port());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(
        bound, alt,
        "--listen-port={lo}-{hi} must skip busy {busy} and bind {alt}"
    );
    let mut last = None;
    for _ in 0..8 {
        let (_tx, rx) = watch::channel(false);
        let j = BtJob {
            torrent: torrent2.clone(),
            dest: dest2.clone(),
            peers: vec![format!("127.0.0.1:{bound}").parse().unwrap()],
            opts: opts2.clone(),
            progress: HttpProgress::new(),
            cancel: rx,
        };
        match bt::download(j).await {
            Ok(()) => {
                last = None;
                break;
            }
            Err(e) => last = Some(e),
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let _ = seeder_task.await;
    drop(occupy);
    if let Some(e) = last {
        panic!("leecher failed after seed on alt listen-port: {e}");
    }
    assert_eq!(
        std::fs::read(&dest2).unwrap(),
        body,
        "--listen-port range skip-busy dest must match seeder bytes"
    );
}

#[tokio::test]
async fn bt_session_torrent_file_magnet_x_pe_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("sess.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let torrent_path = dir.path().join("sess.torrent");
    std::fs::write(&torrent_path, &torrent).unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("torrent-file", torrent_path.display().to_string());
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![magnet_pe(format!("127.0.0.1:{seed}").parse().unwrap())],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("sess.bin")).unwrap(), body);
}

#[tokio::test]
async fn torrent_file_empty_uri_tracker_dest_match() {
    // C++ `aria2c -T file.torrent` with no URI: session must start from torrent-file.
    let body = payload();
    let torrent = bt::build_single_file("tfile.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let seen = Arc::new(TrackerSeen {
        port: AtomicU16::new(0),
    });
    let trk = spawn_tracker(seed, Arc::clone(&seen)).await;
    let dir = tempfile::tempdir().unwrap();
    let torrent_path = dir.path().join("tfile.torrent");
    std::fs::write(&torrent_path, &torrent).unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("torrent-file", torrent_path.display().to_string());
    opts.set(
        "bt-tracker",
        format!("http://127.0.0.1:{trk}/announce"),
    );
    opts.set("listen-port", "0");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(vec![], OptionSet::new())
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(
        std::fs::read(dir.path().join("tfile.bin")).unwrap(),
        body,
        "C++ -T empty-URI dest must match seeder bytes"
    );
}

#[tokio::test]
async fn bt_rpc_add_torrent_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("rpc.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    let session = Session::new(opts).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rpc_port = listener.local_addr().unwrap().port();
    drop(listener);
    let sess = Arc::clone(&session);
    tokio::spawn(async move {
        let _ = aria2_rust::rpc::serve(sess, false, rpc_port).await;
    });
    let url = format!("http://127.0.0.1:{rpc_port}/jsonrpc");
    let client = reqwest::Client::new();
    let params = json!([
        bt::b64_encode(&torrent),
        [magnet_pe(format!("127.0.0.1:{seed}").parse().unwrap())],
        {"dir": dir.path().display().to_string(), "out": "rpc.bin", "file-allocation": "none", "timeout": "15"}
    ]);
    let mut gid = String::new();
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let Ok(res) = client
            .post(&url)
            .header("Content-Type", "application/json")
            .body(
                serde_json::to_vec(&json!({
                    "jsonrpc": "2.0",
                    "id": "1",
                    "method": "aria2.addTorrent",
                    "params": params
                }))
                .unwrap(),
            )
            .send()
            .await
        else {
            continue;
        };
        let bytes = res.bytes().await.unwrap();
        let v: Value = serde_json::from_slice(&bytes).unwrap();
        if let Some(g) = v.get("result").and_then(|r| r.as_str()) {
            gid = g.to_string();
            break;
        }
        if v.get("error").is_some() {
            panic!("addTorrent rpc error: {v}");
        }
    }
    assert!(!gid.is_empty(), "addTorrent did not return gid");
    wait_complete(&session, &gid).await;
    assert_eq!(std::fs::read(dir.path().join("rpc.bin")).unwrap(), body);
}

#[tokio::test]
async fn rpc_save_upload_metadata_true_writes_sha1_torrent_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("up.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("rpc-save-upload-metadata", "true");
    let session = Session::new(opts).unwrap();
    let mut extra = OptionSet::new();
    extra.set("out", "up.bin");
    extra.set("dir", dir.path().display().to_string());
    extra.set("file-allocation", "none");
    extra.set("timeout", "15");
    extra.set("bt-tracker", "");
    extra.set("enable-dht", "false");
    extra.set("listen-port", "0");
    extra.set("seed-ratio", "0");
    extra.set("enable-peer-exchange", "false");
    extra.set("bt-enable-lpd", "false");
    let gid = session
        .add_torrent_and_start(
            torrent.clone(),
            vec![magnet_xt(
                &meta.info_hash,
                format!("127.0.0.1:{seed}").parse().unwrap(),
                "up.bin",
            )],
            extra,
        )
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("up.bin")).unwrap(), body);
    let hex = hex::encode(Sha1::digest(&torrent));
    let saved = dir.path().join(format!("{hex}.torrent"));
    assert!(saved.exists(), "rpc-save-upload-metadata must write SHA-1 .torrent");
    assert_eq!(std::fs::read(&saved).unwrap(), torrent);
}

#[tokio::test]
async fn rpc_save_upload_metadata_false_omits_sha1_torrent_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("noup.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("rpc-save-upload-metadata", "false");
    let session = Session::new(opts).unwrap();
    let mut extra = OptionSet::new();
    extra.set("out", "noup.bin");
    extra.set("dir", dir.path().display().to_string());
    extra.set("file-allocation", "none");
    extra.set("timeout", "15");
    extra.set("enable-dht", "false");
    extra.set("listen-port", "0");
    extra.set("seed-ratio", "0");
    extra.set("enable-peer-exchange", "false");
    extra.set("bt-enable-lpd", "false");
    let gid = session
        .add_torrent_and_start(
            torrent.clone(),
            vec![magnet_xt(
                &meta.info_hash,
                format!("127.0.0.1:{seed}").parse().unwrap(),
                "noup.bin",
            )],
            extra,
        )
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("noup.bin")).unwrap(), body);
    let hex = hex::encode(Sha1::digest(&torrent));
    let saved = dir.path().join(format!("{hex}.torrent"));
    assert!(!saved.exists(), "rpc-save-upload-metadata=false must not write SHA-1 .torrent");
}

#[tokio::test]
async fn bt_add_torrent_session_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("add.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    let session = Session::new(opts).unwrap();
    let mut extra = OptionSet::new();
    extra.set("out", "add.bin");
    let gid = session
        .add_torrent_and_start(
            torrent,
            vec![magnet_pe(format!("127.0.0.1:{seed}").parse().unwrap())],
            extra,
        )
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("add.bin")).unwrap(), body);
}

fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

async fn serve_peer_meta(
    mut s: TcpStream,
    info_hash: [u8; 20],
    payload: &'static [u8],
    info: Arc<Vec<u8>>,
    corrupt_meta: bool,
) {
    s.set_nodelay(true).ok();
    let mut hs = [0u8; 68];
    if s.read_exact(&mut hs).await.is_err() {
        return;
    }
    if hs[0] != 19 || &hs[1..20] != PSTR || hs[28..48] != info_hash {
        return;
    }
    let mine = peer_id();
    if s.write_all(&encode_handshake(&info_hash, &mine)).await.is_err() {
        return;
    }
    let dict = vec![
        (
            b"m".to_vec(),
            BVal::Dict(vec![(b"ut_metadata".to_vec(), BVal::Int(UT_METADATA_ID as i64))]),
        ),
        (b"metadata_size".to_vec(), BVal::Int(info.len() as i64)),
        (b"v".to_vec(), BVal::Bytes(b"seeder".to_vec())),
    ];
    let enc = bencode::encode(&BVal::Dict(dict));
    let mut hs_pay = Vec::with_capacity(1 + enc.len());
    hs_pay.push(EXT_HANDSHAKE);
    hs_pay.extend(enc);
    if write_msg(&mut s, MSG_EXT, &hs_pay).await.is_err() {
        return;
    }
    if write_msg(&mut s, MSG_UNCHOKE, &[]).await.is_err() {
        return;
    }
    let mut client_ut = UT_METADATA_ID;
    loop {
        match tokio::time::timeout(Duration::from_secs(10), read_msg(&mut s)).await {
            Ok(Ok(Some((MSG_EXT, p)))) if !p.is_empty() => {
                if p[0] == EXT_HANDSHAKE {
                    if let Ok(v) = bencode::decode(&p[1..]) {
                        if let Some(id) = v
                            .dict_get(b"m")
                            .and_then(|m| m.dict_get(b"ut_metadata"))
                            .and_then(|x| x.as_int())
                        {
                            if id > 0 && id <= 255 {
                                client_ut = id as u8;
                            }
                        }
                    }
                    continue;
                }
                if p[0] != UT_METADATA_ID {
                    continue;
                }
                let Ok((hdr, _)) = bencode::parse(&p[1..]) else { continue };
                let t = hdr.dict_get(b"msg_type").and_then(|x| x.as_int()).unwrap_or(-1);
                if t != 0 {
                    continue;
                }
                let i = hdr.dict_get(b"piece").and_then(|x| x.as_int()).unwrap_or(0) as usize;
                let start = i * META_BLOCK;
                if start >= info.len() {
                    continue;
                }
                let end = (start + META_BLOCK).min(info.len());
                let mut chunk = info[start..end].to_vec();
                if corrupt_meta && i == 0 && !chunk.is_empty() {
                    chunk[0] ^= 0xFF;
                }
                let data_hdr = bencode::encode(&BVal::Dict(vec![
                    (b"msg_type".to_vec(), BVal::Int(1)),
                    (b"piece".to_vec(), BVal::Int(i as i64)),
                    (b"total_size".to_vec(), BVal::Int(info.len() as i64)),
                ]));
                let mut body = Vec::with_capacity(1 + data_hdr.len() + chunk.len());
                body.push(client_ut);
                body.extend(data_hdr);
                body.extend(chunk);
                if write_msg(&mut s, MSG_EXT, &body).await.is_err() {
                    return;
                }
            }
            Ok(Ok(Some((MSG_REQUEST, p)))) if p.len() >= 12 => {
                let idx = u32::from_be_bytes(p[0..4].try_into().unwrap());
                let begin = u32::from_be_bytes(p[4..8].try_into().unwrap());
                let len = u32::from_be_bytes(p[8..12].try_into().unwrap()) as usize;
                let start = idx as usize * PIECE_LEN as usize + begin as usize;
                let end = start.saturating_add(len);
                if end > payload.len() {
                    return;
                }
                let mut body = Vec::with_capacity(8 + len);
                body.extend_from_slice(&idx.to_be_bytes());
                body.extend_from_slice(&begin.to_be_bytes());
                body.extend_from_slice(&payload[start..end]);
                if write_msg(&mut s, MSG_PIECE, &body).await.is_err() {
                    return;
                }
            }
            Ok(Ok(Some((MSG_INTERESTED, _)))) => {
                let _ = write_msg(&mut s, MSG_UNCHOKE, &[]).await;
            }
            Ok(Ok(Some(_))) => {}
            _ => return,
        }
    }
}

async fn spawn_seeder_meta(
    info_hash: [u8; 20],
    payload: &'static [u8],
    info: Vec<u8>,
    corrupt_meta: bool,
) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let info = Arc::new(info);
    tokio::spawn(async move {
        loop {
            let Ok((s, _)) = listener.accept().await else {
                break;
            };
            let info = Arc::clone(&info);
            tokio::spawn(async move {
                serve_peer_meta(s, info_hash, payload, info, corrupt_meta).await;
            });
        }
    });
    port
}

fn magnet_xt(ih: &[u8; 20], addr: SocketAddr, dn: &str) -> String {
    format!("magnet:?xt=urn:btih:{}&dn={dn}&x.pe={addr}", to_hex(ih))
}

#[tokio::test]
async fn magnet_ut_metadata_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("mag.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let info = bencode::raw_info_dict(&torrent).unwrap().to_vec();
    let seed = spawn_seeder_meta(meta.info_hash, body, info, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("mag.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "mag.bin");
    opts.set(
        "magnet",
        magnet_xt(&meta.info_hash, format!("127.0.0.1:{seed}").parse().unwrap(), "mag.bin"),
    );
    opts.set("bt-save-metadata", "true");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent: Vec::new(),
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body, "magnet dest must match");
    let saved = dir.path().join(format!("{}.torrent", to_hex(&meta.info_hash)));
    assert!(saved.exists(), "--bt-save-metadata must write .torrent");
    let loaded = MetaInfo::from_torrent(&std::fs::read(&saved).unwrap()).unwrap();
    assert_eq!(loaded.info_hash, meta.info_hash);
}

#[tokio::test]
async fn magnet_ut_metadata_hash_mismatch_rejected() {
    let body = payload();
    let torrent = bt::build_single_file("badm.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let info = bencode::raw_info_dict(&torrent).unwrap().to_vec();
    let seed = spawn_seeder_meta(meta.info_hash, body, info, true).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set(
        "magnet",
        magnet_xt(&meta.info_hash, format!("127.0.0.1:{seed}").parse().unwrap(), "badm.bin"),
    );
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent: Vec::new(),
        dest: dir.path().join("badm.bin"),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let err = bt::download(job).await;
    assert!(err.is_err(), "corrupt metadata must fail info_hash");
    let msg = err.unwrap_err().to_string();
    assert!(
        msg.contains("info_hash") || msg.contains("mismatch"),
        "got {msg}"
    );
}

#[tokio::test]
async fn bt_metadata_only_skips_payload() {
    let body = payload();
    let torrent = bt::build_single_file("only.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let info = bencode::raw_info_dict(&torrent).unwrap().to_vec();
    let seed = spawn_seeder_meta(meta.info_hash, body, info, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("only.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("bt-metadata-only", "true");
    opts.set(
        "magnet",
        magnet_xt(&meta.info_hash, format!("127.0.0.1:{seed}").parse().unwrap(), "only.bin"),
    );
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent: Vec::new(),
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "--bt-metadata-only must not write payload"
    );
    let saved = dir.path().join(format!("{}.torrent", to_hex(&meta.info_hash)));
    assert!(saved.exists(), "metadata-only implies save");
}

#[tokio::test]
async fn bt_load_saved_metadata_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("load.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(format!("{}.torrent", to_hex(&meta.info_hash))),
        &torrent,
    )
    .unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("bt-load-saved-metadata", "true");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![magnet_xt(
                &meta.info_hash,
                format!("127.0.0.1:{seed}").parse().unwrap(),
                "load.bin",
            )],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("load.bin")).unwrap(), body);
}

#[tokio::test]
async fn session_magnet_ut_metadata_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("sessm.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let info = bencode::raw_info_dict(&torrent).unwrap().to_vec();
    let seed = spawn_seeder_meta(meta.info_hash, body, info, false).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![magnet_xt(
                &meta.info_hash,
                format!("127.0.0.1:{seed}").parse().unwrap(),
                "sessm.bin",
            )],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("sessm.bin")).unwrap(), body);
}

#[tokio::test]
async fn pause_metadata_magnet_then_unpause_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("pause-md.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let info = bencode::raw_info_dict(&torrent).unwrap().to_vec();
    let seed = spawn_seeder_meta(meta.info_hash, body, info, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("pause-md.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    let session = Session::new(opts).unwrap();
    let mut extra = OptionSet::new();
    extra.set("pause-metadata", "true");
    extra.set("out", "pause-md.bin");
    let gid = session
        .add_uri_and_start(
            vec![magnet_xt(
                &meta.info_hash,
                format!("127.0.0.1:{seed}").parse().unwrap(),
                "pause-md.bin",
            )],
            extra,
        )
        .await
        .unwrap();
    let mut paused = false;
    for _ in 0..200 {
        let st = session.tell_status(gid.as_str()).await.unwrap();
        match st.get("status").and_then(|v| v.as_str()) {
            Some("paused") => {
                paused = true;
                break;
            }
            Some("error") => panic!("pause-metadata error: {st}"),
            Some("complete") => panic!("pause-metadata must not complete before unpause: {st}"),
            _ => {}
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(paused, "magnet must pause after metadata");
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "payload must not be written while paused after metadata"
    );
    let saved = dir.path().join(format!("{}.torrent", to_hex(&meta.info_hash)));
    assert!(saved.exists(), "pause-metadata must save .torrent");
    session.unpause(gid.as_str()).await.unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn get_peers_seeder_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("peers.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let torrent_path = dir.path().join("peers.torrent");
    std::fs::write(&torrent_path, &torrent).unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "peers.bin");
    opts.set("torrent-file", torrent_path.display().to_string());
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!(
                "magnet:?xt=urn:btih:{}&dn=peers.bin&x.pe=127.0.0.1:{seed}",
                hex::encode(meta.info_hash)
            )],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("peers.bin")).unwrap(), body);
    let peers = session.get_peers(gid.as_str()).await.unwrap();
    let arr = peers.as_array().expect("peers array");
    let port_s = seed.to_string();
    assert!(
        arr.iter().any(|p| {
            p.get("ip").and_then(|v| v.as_str()) == Some("127.0.0.1")
                && p.get("port").and_then(|v| v.as_str()) == Some(port_s.as_str())
                && p.get("seeder").and_then(|v| v.as_str()) == Some("true")
                && p.get("peerId")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| !s.is_empty())
        }),
        "getPeers must list the seeder we dest-matched: {peers}"
    );
}

async fn spawn_pex_introducer(info_hash: [u8; 20], seeder: SocketAddr) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else {
                break;
            };
            let info_hash = info_hash;
            tokio::spawn(async move {
                s.set_nodelay(true).ok();
                let mut hs = [0u8; 68];
                if s.read_exact(&mut hs).await.is_err() {
                    return;
                }
                if hs[0] != 19 || &hs[1..20] != PSTR || hs[28..48] != info_hash {
                    return;
                }
                let mine = peer_id();
                if s.write_all(&encode_handshake(&info_hash, &mine)).await.is_err() {
                    return;
                }
                let enc = bencode::encode(&BVal::Dict(vec![
                    (
                        b"m".to_vec(),
                        BVal::Dict(vec![(b"ut_pex".to_vec(), BVal::Int(UT_PEX_ID as i64))]),
                    ),
                    (b"v".to_vec(), BVal::Bytes(b"pex-intro".to_vec())),
                ]));
                let mut pay = Vec::with_capacity(1 + enc.len());
                pay.push(EXT_HANDSHAKE);
                pay.extend(enc);
                if write_msg(&mut s, MSG_EXT, &pay).await.is_err() {
                    return;
                }
                let added = encode_compact_peers(&[seeder]);
                let body = bencode::encode(&BVal::Dict(vec![(b"added".to_vec(), BVal::Bytes(added))]));
                let mut pex = Vec::with_capacity(1 + body.len());
                pex.push(UT_PEX_ID);
                pex.extend(body);
                if write_msg(&mut s, MSG_EXT, &pex).await.is_err() {
                    return;
                }
                let _ = tokio::time::timeout(Duration::from_millis(200), read_msg(&mut s)).await;
                let _ = AsyncWriteExt::shutdown(&mut s).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn bt_pex_ut_pex_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("pex.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let seeder: SocketAddr = format!("127.0.0.1:{seed}").parse().unwrap();
    let intro = spawn_pex_introducer(meta.info_hash, seeder).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("pex.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "pex.bin");
    opts.set("enable-peer-exchange", "true");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{intro}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "PEX added peer must dest-match seeder bytes"
    );
}

#[tokio::test]
async fn bt_pex_disabled_ignores_added() {
    let body = payload();
    let torrent = bt::build_single_file("nopx.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let seeder: SocketAddr = format!("127.0.0.1:{seed}").parse().unwrap();
    let intro = spawn_pex_introducer(meta.info_hash, seeder).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("nopx.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("connect-timeout", "2");
    opts.set("timeout", "2");
    opts.set("enable-peer-exchange", "false");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{intro}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let err = bt::download(job).await;
    assert!(err.is_err(), "--enable-peer-exchange=false must not use ut_pex added");
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "disabled PEX must not write seeder payload"
    );
}

#[tokio::test]
async fn session_pex_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("spex.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let seeder: SocketAddr = format!("127.0.0.1:{seed}").parse().unwrap();
    let intro = spawn_pex_introducer(meta.info_hash, seeder).await;
    let dir = tempfile::tempdir().unwrap();
    let torrent_path = dir.path().join("spex.torrent");
    std::fs::write(&torrent_path, &torrent).unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("torrent-file", torrent_path.display().to_string());
    opts.set("enable-peer-exchange", "true");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![magnet_xt(
                &meta.info_hash,
                format!("127.0.0.1:{intro}").parse().unwrap(),
                "spex.bin",
            )],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("spex.bin")).unwrap(), body);
}

async fn spawn_dht(seeder: SocketAddr, seen_src: Arc<AtomicU16>) -> u16 {
    let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let port = sock.local_addr().unwrap().port();
    tokio::spawn(async move {
        let mut buf = [0u8; 2048];
        loop {
            let Ok((n, from)) = sock.recv_from(&mut buf).await else {
                break;
            };
            seen_src.store(from.port(), Ordering::SeqCst);
            let Ok(v) = bencode::decode(&buf[..n]) else {
                continue;
            };
            let t = v
                .dict_get(b"t")
                .and_then(|x| x.as_bytes())
                .unwrap_or(b"aa")
                .to_vec();
            let q = v.dict_get(b"q").and_then(|x| x.as_bytes()).unwrap_or(b"");
            if q != b"get_peers" && q != b"find_node" && q != b"ping" {
                continue;
            }
            let mut r = vec![
                (b"id".to_vec(), BVal::Bytes(vec![9u8; 20])),
                (b"token".to_vec(), BVal::Bytes(b"tok".to_vec())),
            ];
            if q == b"get_peers" {
                r.push((
                    b"values".to_vec(),
                    BVal::List(vec![BVal::Bytes(encode_compact_peers(&[seeder]))]),
                ));
            }
            let resp = BVal::Dict(vec![
                (b"t".to_vec(), BVal::Bytes(t)),
                (b"y".to_vec(), BVal::Bytes(b"r".to_vec())),
                (b"r".to_vec(), BVal::Dict(r)),
            ]);
            let _ = sock.send_to(&bencode::encode(&resp), from).await;
        }
    });
    port
}

async fn spawn_dht_delayed(seeder: SocketAddr, seen_src: Arc<AtomicU16>, delay: Duration) -> u16 {
    let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let port = sock.local_addr().unwrap().port();
    tokio::spawn(async move {
        let mut buf = [0u8; 2048];
        loop {
            let Ok((n, from)) = sock.recv_from(&mut buf).await else {
                break;
            };
            seen_src.store(from.port(), Ordering::SeqCst);
            let Ok(v) = bencode::decode(&buf[..n]) else {
                continue;
            };
            let t = v
                .dict_get(b"t")
                .and_then(|x| x.as_bytes())
                .unwrap_or(b"aa")
                .to_vec();
            let q = v.dict_get(b"q").and_then(|x| x.as_bytes()).unwrap_or(b"");
            if q != b"get_peers" {
                continue;
            }
            tokio::time::sleep(delay).await;
            let r = vec![
                (b"id".to_vec(), BVal::Bytes(vec![9u8; 20])),
                (b"token".to_vec(), BVal::Bytes(b"tok".to_vec())),
                (
                    b"values".to_vec(),
                    BVal::List(vec![BVal::Bytes(encode_compact_peers(&[seeder]))]),
                ),
            ];
            let resp = BVal::Dict(vec![
                (b"t".to_vec(), BVal::Bytes(t)),
                (b"y".to_vec(), BVal::Bytes(b"r".to_vec())),
                (b"r".to_vec(), BVal::Dict(r)),
            ]);
            let _ = sock.send_to(&bencode::encode(&resp), from).await;
        }
    });
    port
}

#[tokio::test]
async fn bt_dht_get_peers_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("dht.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let seeder: SocketAddr = format!("127.0.0.1:{seed}").parse().unwrap();
    let seen = Arc::new(AtomicU16::new(0));
    let dht = spawn_dht(seeder, Arc::clone(&seen)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("dht.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "dht.bin");
    opts.set("enable-dht", "true");
    opts.set("dht-listen-port", "0");
    opts.set("dht-message-timeout", "2");
    opts.set("dht-entry-point", format!("127.0.0.1:{dht}"));
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "DHT get_peers values must dest-match seeder bytes"
    );
    assert!(seen.load(Ordering::SeqCst) != 0, "DHT query must hit entry-point");
}

#[tokio::test]
async fn bt_dht_disabled_does_not_query() {
    let body = payload();
    let torrent = bt::build_single_file("nodht.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let seeder: SocketAddr = format!("127.0.0.1:{seed}").parse().unwrap();
    let seen = Arc::new(AtomicU16::new(0));
    let dht = spawn_dht(seeder, Arc::clone(&seen)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("nodht.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("enable-dht", "false");
    opts.set("dht-listen-port", "0");
    opts.set("dht-entry-point", format!("127.0.0.1:{dht}"));
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let err = bt::download(job).await;
    assert!(err.is_err(), "--enable-dht=false must not use DHT values");
    assert_eq!(seen.load(Ordering::SeqCst), 0, "disabled DHT must not send KRPC");
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "disabled DHT must not write seeder payload"
    );
}

#[tokio::test]
async fn bt_dht_listen_port_is_source() {
    let body = payload();
    let torrent = bt::build_single_file("dhtp.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let seeder: SocketAddr = format!("127.0.0.1:{seed}").parse().unwrap();
    let seen = Arc::new(AtomicU16::new(0));
    let dht = spawn_dht(seeder, Arc::clone(&seen)).await;
    let probe = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let listen = probe.local_addr().unwrap().port();
    drop(probe);
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("dhtp.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "dhtp.bin");
    opts.set("enable-dht", "true");
    opts.set("dht-listen-port", listen.to_string());
    opts.set("dht-message-timeout", "2");
    opts.set("dht-entry-point", format!("127.0.0.1:{dht}"));
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        seen.load(Ordering::SeqCst),
        listen,
        "--dht-listen-port must be the KRPC source port"
    );
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn dht_listen_port_range_skips_busy_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("dhtr.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let seeder: SocketAddr = format!("127.0.0.1:{seed}").parse().unwrap();
    let seen = Arc::new(AtomicU16::new(0));
    let dht = spawn_dht(seeder, Arc::clone(&seen)).await;
    let occupy = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let busy = occupy.local_addr().unwrap().port();
    let alt = if busy < 65535 { busy + 1 } else { busy - 1 };
    match UdpSocket::bind(("127.0.0.1", alt)).await {
        Ok(free) => drop(free),
        Err(_) => {
            drop(occupy);
            panic!("dht-listen-port range test needs a free neighbor of {busy}");
        }
    }
    let lo = busy.min(alt);
    let hi = busy.max(alt);
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("dhtr.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "dhtr.bin");
    opts.set("enable-dht", "true");
    opts.set("dht-listen-addr", "127.0.0.1");
    opts.set("dht-listen-port", format!("{lo}-{hi}"));
    opts.set("dht-message-timeout", "2");
    opts.set("dht-entry-point", format!("127.0.0.1:{dht}"));
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    drop(occupy);
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--dht-listen-port range dest must match"
    );
    let src = seen.load(Ordering::SeqCst);
    assert_eq!(
        src, alt,
        "--dht-listen-port={lo}-{hi} must skip busy {busy} KRPC source, got {src}"
    );
    assert_eq!(
        aria2_rust::dht::last_dht_listen_port(),
        alt,
        "bound dht-listen-port must be {alt}"
    );
}

#[tokio::test]
async fn session_dht_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("sdht.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let seeder: SocketAddr = format!("127.0.0.1:{seed}").parse().unwrap();
    let seen = Arc::new(AtomicU16::new(0));
    let dht = spawn_dht(seeder, Arc::clone(&seen)).await;
    let dir = tempfile::tempdir().unwrap();
    let torrent_path = dir.path().join("sdht.torrent");
    std::fs::write(&torrent_path, &torrent).unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("torrent-file", torrent_path.display().to_string());
    opts.set("enable-dht", "true");
    opts.set("dht-listen-port", "0");
    opts.set("dht-message-timeout", "2");
    opts.set("dht-entry-point", format!("127.0.0.1:{dht}"));
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("magnet:?xt=urn:btih:{}&dn=sdht.bin", hex::encode(meta.info_hash))],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("sdht.bin")).unwrap(), body);
}

#[tokio::test]
async fn dht_file_path_persists_then_bootstraps_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("dhtdat.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let seeder: SocketAddr = format!("127.0.0.1:{seed}").parse().unwrap();
    let seen = Arc::new(AtomicU16::new(0));
    let dht = spawn_dht(seeder, Arc::clone(&seen)).await;
    let dir = tempfile::tempdir().unwrap();
    let dat = dir.path().join("dht.dat");
    let dest1 = dir.path().join("first.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "first.bin");
    opts.set("enable-dht", "true");
    opts.set("dht-listen-port", "0");
    opts.set("dht-message-timeout", "2");
    opts.set("dht-entry-point", format!("127.0.0.1:{dht}"));
    opts.set("dht-file-path", dat.display().to_string());
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent: torrent.clone(),
        dest: dest1.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest1).unwrap(), body);
    let stored = bt::parse_compact_peers(&std::fs::read(&dat).unwrap());
    assert!(
        stored.iter().any(|a| a.port() == dht),
        "dht.dat must keep the entry-point node: {stored:?}"
    );

    seen.store(0, Ordering::SeqCst);
    let dest2 = dir.path().join("second.bin");
    let mut opts2 = job_opts();
    opts2.set("dir", dir.path().display().to_string());
    opts2.set("out", "second.bin");
    opts2.set("enable-dht", "true");
    opts2.set("dht-listen-port", "0");
    opts2.set("dht-message-timeout", "2");
    opts2.set("dht-file-path", dat.display().to_string());
    let (_tx2, rx2) = watch::channel(false);
    let job2 = BtJob {
        torrent,
        dest: dest2.clone(),
        peers: vec![],
        opts: opts2,
        progress: HttpProgress::new(),
        cancel: rx2,
    };
    bt::download(job2).await.unwrap();
    assert_eq!(
        std::fs::read(&dest2).unwrap(),
        body,
        "second run must dest-match from dht.dat with no dht-entry-point"
    );
    assert!(seen.load(Ordering::SeqCst) != 0, "loaded dht.dat nodes must be queried");
}

#[tokio::test]
async fn dht_file_path_missing_without_entry_point_no_dest() {
    let body = payload();
    let torrent = bt::build_single_file("nodat.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let seeder: SocketAddr = format!("127.0.0.1:{seed}").parse().unwrap();
    let seen = Arc::new(AtomicU16::new(0));
    let _dht = spawn_dht(seeder, Arc::clone(&seen)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("nodat.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("enable-dht", "true");
    opts.set("dht-listen-port", "0");
    opts.set("dht-file-path", dir.path().join("missing.dat").display().to_string());
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let err = bt::download(job).await;
    assert!(err.is_err(), "missing dht.dat and no entry-point must not find peers");
    assert_eq!(seen.load(Ordering::SeqCst), 0);
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "must not dest-match without DHT nodes"
    );
}

async fn spawn_seeder6(info_hash: [u8; 20], payload: &'static [u8]) -> u16 {
    let listener = TcpListener::bind("[::1]:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((s, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                serve_peer(s, info_hash, payload, false).await;
            });
        }
    });
    port
}

async fn spawn_dht6(seeder: SocketAddr, seen_src: Arc<AtomicU16>) -> u16 {
    let sock = UdpSocket::bind("[::1]:0").await.unwrap();
    let port = sock.local_addr().unwrap().port();
    tokio::spawn(async move {
        let mut buf = [0u8; 2048];
        loop {
            let Ok((n, from)) = sock.recv_from(&mut buf).await else {
                break;
            };
            seen_src.store(from.port(), Ordering::SeqCst);
            let Ok(v) = bencode::decode(&buf[..n]) else {
                continue;
            };
            let t = v
                .dict_get(b"t")
                .and_then(|x| x.as_bytes())
                .unwrap_or(b"aa")
                .to_vec();
            let q = v.dict_get(b"q").and_then(|x| x.as_bytes()).unwrap_or(b"");
            if q != b"get_peers" {
                continue;
            }
            let r = vec![
                (b"id".to_vec(), BVal::Bytes(vec![9u8; 20])),
                (b"token".to_vec(), BVal::Bytes(b"tok".to_vec())),
                (
                    b"values".to_vec(),
                    BVal::List(vec![BVal::Bytes(encode_compact_peers6(&[seeder]))]),
                ),
            ];
            let resp = BVal::Dict(vec![
                (b"t".to_vec(), BVal::Bytes(t)),
                (b"y".to_vec(), BVal::Bytes(b"r".to_vec())),
                (b"r".to_vec(), BVal::Dict(r)),
            ]);
            let _ = sock.send_to(&bencode::encode(&resp), from).await;
        }
    });
    port
}

#[tokio::test]
async fn enable_dht6_entry_point6_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("dht6.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder6(meta.info_hash, body).await;
    let seeder: SocketAddr = format!("[::1]:{seed}").parse().unwrap();
    let seen = Arc::new(AtomicU16::new(0));
    let dht = spawn_dht6(seeder, Arc::clone(&seen)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("dht6.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "dht6.bin");
    opts.set("enable-dht", "false");
    opts.set("enable-dht6", "true");
    opts.set("dht-listen-port", "0");
    opts.set("dht-message-timeout", "2");
    opts.set("dht-entry-point6", format!("[::1]:{dht}"));
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body, "IPv6 DHT values must dest-match seeder");
    assert!(seen.load(Ordering::SeqCst) != 0, "must query dht-entry-point6");
}

#[tokio::test]
async fn enable_dht6_false_does_not_query_entry_point6() {
    let body = payload();
    let torrent = bt::build_single_file("nodht6.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder6(meta.info_hash, body).await;
    let seeder: SocketAddr = format!("[::1]:{seed}").parse().unwrap();
    let seen = Arc::new(AtomicU16::new(0));
    let dht = spawn_dht6(seeder, Arc::clone(&seen)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("nodht6.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("enable-dht", "false");
    opts.set("enable-dht6", "false");
    opts.set("dht-listen-port", "0");
    opts.set("dht-entry-point6", format!("[::1]:{dht}"));
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let err = bt::download(job).await;
    assert!(err.is_err(), "--enable-dht6=false must not use IPv6 DHT");
    assert_eq!(seen.load(Ordering::SeqCst), 0, "disabled DHT6 must not send KRPC");
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "disabled DHT6 must not dest-match"
    );
}

#[tokio::test]
async fn dht_file_path6_persists_then_bootstraps_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("dht6dat.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder6(meta.info_hash, body).await;
    let seeder: SocketAddr = format!("[::1]:{seed}").parse().unwrap();
    let seen = Arc::new(AtomicU16::new(0));
    let dht = spawn_dht6(seeder, Arc::clone(&seen)).await;
    let dir = tempfile::tempdir().unwrap();
    let dat = dir.path().join("dht6.dat");
    let dest1 = dir.path().join("first6.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "first6.bin");
    opts.set("enable-dht", "false");
    opts.set("enable-dht6", "true");
    opts.set("dht-listen-port", "0");
    opts.set("dht-message-timeout", "2");
    opts.set("dht-entry-point6", format!("[::1]:{dht}"));
    opts.set("dht-file-path6", dat.display().to_string());
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent: torrent.clone(),
        dest: dest1.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest1).unwrap(), body);
    let stored = parse_compact_peers6(&std::fs::read(&dat).unwrap());
    assert!(
        stored.iter().any(|a| a.port() == dht && a.is_ipv6()),
        "dht6.dat must keep the IPv6 entry-point: {stored:?}"
    );

    seen.store(0, Ordering::SeqCst);
    let dest2 = dir.path().join("second6.bin");
    let mut opts2 = job_opts();
    opts2.set("dir", dir.path().display().to_string());
    opts2.set("out", "second6.bin");
    opts2.set("enable-dht", "false");
    opts2.set("enable-dht6", "true");
    opts2.set("dht-listen-port", "0");
    opts2.set("dht-message-timeout", "2");
    opts2.set("dht-file-path6", dat.display().to_string());
    let (_tx2, rx2) = watch::channel(false);
    let job2 = BtJob {
        torrent,
        dest: dest2.clone(),
        peers: vec![],
        opts: opts2,
        progress: HttpProgress::new(),
        cancel: rx2,
    };
    bt::download(job2).await.unwrap();
    assert_eq!(
        std::fs::read(&dest2).unwrap(),
        body,
        "second run must dest-match from dht6.dat with no dht-entry-point6"
    );
    assert!(seen.load(Ordering::SeqCst) != 0, "loaded dht6.dat nodes must be queried");
}

#[tokio::test]
async fn dht_file_path6_missing_without_entry_point6_no_dest() {
    let body = payload();
    let torrent = bt::build_single_file("nodat6.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder6(meta.info_hash, body).await;
    let seeder: SocketAddr = format!("[::1]:{seed}").parse().unwrap();
    let seen = Arc::new(AtomicU16::new(0));
    let _dht = spawn_dht6(seeder, Arc::clone(&seen)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("nodat6.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("enable-dht", "false");
    opts.set("enable-dht6", "true");
    opts.set("dht-listen-port", "0");
    opts.set("dht-file-path6", dir.path().join("missing6.dat").display().to_string());
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let err = bt::download(job).await;
    assert!(err.is_err(), "missing dht6.dat and no entry-point6 must not find peers");
    assert_eq!(seen.load(Ordering::SeqCst), 0);
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "must not dest-match without IPv6 DHT nodes"
    );
}

#[tokio::test]
async fn dht_listen_addr6_loopback_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("addr6.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder6(meta.info_hash, body).await;
    let seeder: SocketAddr = format!("[::1]:{seed}").parse().unwrap();
    let seen = Arc::new(AtomicU16::new(0));
    let dht = spawn_dht6(seeder, Arc::clone(&seen)).await;
    let probe = UdpSocket::bind("[::1]:0").await.unwrap();
    let listen = probe.local_addr().unwrap().port();
    drop(probe);
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("addr6.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "addr6.bin");
    opts.set("enable-dht", "false");
    opts.set("enable-dht6", "true");
    opts.set("dht-listen-port", listen.to_string());
    opts.set("dht-listen-addr6", "::1");
    opts.set("dht-message-timeout", "2");
    opts.set("dht-entry-point6", format!("[::1]:{dht}"));
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        seen.load(Ordering::SeqCst),
        listen,
        "--dht-listen-addr6=::1 must be the KRPC source"
    );
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn dht_listen_addr6_unusable_no_dest() {
    let body = payload();
    let torrent = bt::build_single_file("badaddr6.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder6(meta.info_hash, body).await;
    let seeder: SocketAddr = format!("[::1]:{seed}").parse().unwrap();
    let seen = Arc::new(AtomicU16::new(0));
    let dht = spawn_dht6(seeder, Arc::clone(&seen)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("badaddr6.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("enable-dht", "false");
    opts.set("enable-dht6", "true");
    opts.set("dht-listen-port", "0");
    opts.set("dht-listen-addr6", "2001:db8::1");
    opts.set("dht-message-timeout", "1");
    opts.set("dht-entry-point6", format!("[::1]:{dht}"));
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let err = bt::download(job).await;
    assert!(err.is_err(), "unusable dht-listen-addr6 must not find peers: {err:?}");
    assert_eq!(seen.load(Ordering::SeqCst), 0);
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "unusable dht-listen-addr6 must not dest-match"
    );
}

#[tokio::test]
async fn dht_listen_addr_loopback_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("addr4.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let seeder: SocketAddr = format!("127.0.0.1:{seed}").parse().unwrap();
    let seen = Arc::new(AtomicU16::new(0));
    let dht = spawn_dht(seeder, Arc::clone(&seen)).await;
    let probe = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let listen = probe.local_addr().unwrap().port();
    drop(probe);
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("addr4.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "addr4.bin");
    opts.set("enable-dht", "true");
    opts.set("dht-listen-port", listen.to_string());
    opts.set("dht-listen-addr", "127.0.0.1");
    opts.set("dht-message-timeout", "2");
    opts.set("dht-entry-point", format!("127.0.0.1:{dht}"));
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        seen.load(Ordering::SeqCst),
        listen,
        "--dht-listen-addr=127.0.0.1 must be the KRPC source"
    );
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn dht_listen_addr_unusable_no_dest() {
    let body = payload();
    let torrent = bt::build_single_file("badaddr4.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let seeder: SocketAddr = format!("127.0.0.1:{seed}").parse().unwrap();
    let seen = Arc::new(AtomicU16::new(0));
    let dht = spawn_dht(seeder, Arc::clone(&seen)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("badaddr4.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("enable-dht", "true");
    opts.set("dht-listen-port", "0");
    opts.set("dht-listen-addr", "192.0.2.1");
    opts.set("dht-message-timeout", "1");
    opts.set("dht-entry-point", format!("127.0.0.1:{dht}"));
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let err = bt::download(job).await;
    assert!(err.is_err(), "unusable dht-listen-addr must not find peers: {err:?}");
    assert_eq!(seen.load(Ordering::SeqCst), 0);
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "unusable dht-listen-addr must not dest-match"
    );
}

#[tokio::test]
async fn dht_message_timeout_one_misses_delayed_node() {
    let body = payload();
    let torrent = bt::build_single_file("dhtto.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let seeder: SocketAddr = format!("127.0.0.1:{seed}").parse().unwrap();
    let seen = Arc::new(AtomicU16::new(0));
    let dht = spawn_dht_delayed(seeder, Arc::clone(&seen), Duration::from_millis(1500)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("dhtto.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("enable-dht", "true");
    opts.set("dht-listen-port", "0");
    opts.set("dht-message-timeout", "1");
    opts.set("dht-entry-point", format!("127.0.0.1:{dht}"));
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let err = bt::download(job).await;
    assert!(err.is_err(), "--dht-message-timeout=1 must miss a 1.5s DHT reply: {err:?}");
    assert!(seen.load(Ordering::SeqCst) != 0, "must still send get_peers");
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "short DHT timeout must not dest-match"
    );
}

#[tokio::test]
async fn dht_message_timeout_three_dest_match_delayed_node() {
    let body = payload();
    let torrent = bt::build_single_file("dhtok.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let seeder: SocketAddr = format!("127.0.0.1:{seed}").parse().unwrap();
    let seen = Arc::new(AtomicU16::new(0));
    let dht = spawn_dht_delayed(seeder, Arc::clone(&seen), Duration::from_millis(1500)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("dhtok.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "dhtok.bin");
    opts.set("enable-dht", "true");
    opts.set("dht-listen-port", "0");
    opts.set("dht-message-timeout", "3");
    opts.set("dht-entry-point", format!("127.0.0.1:{dht}"));
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--dht-message-timeout=3 must dest-match delayed DHT values"
    );
    assert!(seen.load(Ordering::SeqCst) != 0);
}

async fn spawn_stall_peer(info_hash: [u8; 20]) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else {
                break;
            };
            let ih = info_hash;
            tokio::spawn(async move {
                let mut hs = [0u8; 68];
                if s.read_exact(&mut hs).await.is_err() {
                    return;
                }
                if hs[0] != 19 || &hs[1..20] != PSTR || hs[28..48] != ih {
                    return;
                }
                let mine = peer_id();
                if s.write_all(&encode_handshake(&ih, &mine)).await.is_err() {
                    return;
                }
                if write_msg(&mut s, MSG_UNCHOKE, &[]).await.is_err() {
                    return;
                }
                loop {
                    match tokio::time::timeout(Duration::from_secs(30), read_msg(&mut s)).await {
                        Ok(Ok(Some(_))) => {}
                        _ => return,
                    }
                }
            });
        }
    });
    port
}

#[tokio::test]
async fn bt_stop_timeout_one_stall_no_dest() {
    let body = payload();
    let torrent = bt::build_single_file("stall.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let stall = spawn_stall_peer(MetaInfo::from_torrent(&torrent).unwrap().info_hash).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("stall.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("bt-stop-timeout", "1");
    opts.set("connect-timeout", "10");
    opts.set("timeout", "10");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{stall}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let err = bt::download(job).await;
    assert!(
        err.as_ref().is_err_and(|e| e.to_string().contains("bt-stop-timeout")),
        "stalling peer must hit --bt-stop-timeout=1: {err:?}"
    );
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "stalling peer must not dest-match"
    );
}

#[tokio::test]
async fn bt_stop_timeout_one_seeder_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("stok.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("stok.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "stok.bin");
    opts.set("bt-stop-timeout", "1");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--bt-stop-timeout=1 must dest-match a live seeder"
    );
}

#[tokio::test]
async fn bt_exclude_tracker_star_uses_bt_tracker_dest_match() {
    let body = payload();
    let bad = spawn_tracker(1, Arc::new(TrackerSeen { port: AtomicU16::new(0) })).await;
    let torrent = bt::build_single_file(
        "ex.bin",
        PIECE_LEN,
        body,
        &format!("http://127.0.0.1:{bad}/announce"),
    );
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let good = spawn_tracker(seed, Arc::new(TrackerSeen { port: AtomicU16::new(0) })).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("ex.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "ex.bin");
    opts.set("bt-exclude-tracker", "*");
    opts.set("bt-tracker", format!("http://127.0.0.1:{good}/announce"));
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--bt-exclude-tracker=* must skip torrent announce and dest-match --bt-tracker"
    );
}

#[tokio::test]
async fn bt_exclude_tracker_unset_uses_bad_announce_no_dest() {
    let body = payload();
    let bad = spawn_tracker(1, Arc::new(TrackerSeen { port: AtomicU16::new(0) })).await;
    let torrent = bt::build_single_file(
        "noex.bin",
        PIECE_LEN,
        body,
        &format!("http://127.0.0.1:{bad}/announce"),
    );
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let good = spawn_tracker(seed, Arc::new(TrackerSeen { port: AtomicU16::new(0) })).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("noex.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("bt-tracker", format!("http://127.0.0.1:{good}/announce"));
    opts.set("connect-timeout", "1");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let err = bt::download(job).await;
    assert!(err.is_err(), "without exclude, torrent announce peer :1 must win: {err:?}");
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "bad torrent tracker must not dest-match when not excluded"
    );
}

#[tokio::test]
async fn bt_tracker_timeout_one_misses_delayed() {
    let body = payload();
    let torrent = bt::build_single_file("tto.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let tr = spawn_tracker_delayed(seed, Duration::from_millis(1500)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("tto.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("bt-exclude-tracker", "*");
    opts.set("bt-tracker", format!("http://127.0.0.1:{tr}/announce"));
    opts.set("bt-tracker-timeout", "1");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let err = bt::download(job).await;
    assert!(err.is_err(), "--bt-tracker-timeout=1 must miss a 1.5s tracker: {err:?}");
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "short tracker timeout must not dest-match"
    );
}

#[tokio::test]
async fn bt_tracker_timeout_three_dest_match_delayed() {
    let body = payload();
    let torrent = bt::build_single_file("ttok.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let tr = spawn_tracker_delayed(seed, Duration::from_millis(1500)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("ttok.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "ttok.bin");
    opts.set("bt-exclude-tracker", "*");
    opts.set("bt-tracker", format!("http://127.0.0.1:{tr}/announce"));
    opts.set("bt-tracker-timeout", "3");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--bt-tracker-timeout=3 must dest-match delayed tracker peers"
    );
}

#[tokio::test]
async fn bt_tracker_connect_timeout_one_misses_unroutable() {
    let body = payload();
    let torrent = bt::build_single_file("tcto.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("tcto.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("bt-exclude-tracker", "*");
    opts.set("bt-tracker", "http://192.0.2.1:9/announce");
    opts.set("bt-tracker-connect-timeout", "1");
    opts.set("bt-tracker-timeout", "10");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let t0 = std::time::Instant::now();
    let err = bt::download(job).await;
    let elapsed = t0.elapsed();
    assert!(
        err.is_err(),
        "--bt-tracker-connect-timeout=1 must miss TEST-NET tracker: {err:?}"
    );
    assert!(
        elapsed >= Duration::from_millis(800),
        "connect-timeout must wait ~1s, got {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "connect-timeout must not wait tracker-timeout, got {elapsed:?}"
    );
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "unroutable tracker must not dest-match"
    );
}

#[tokio::test]
async fn bt_tracker_connect_timeout_one_delayed_reply_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("tctok.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let tr = spawn_tracker_delayed(seed, Duration::from_millis(1500)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("tctok.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "tctok.bin");
    opts.set("bt-exclude-tracker", "*");
    opts.set("bt-tracker", format!("http://127.0.0.1:{tr}/announce"));
    opts.set("bt-tracker-connect-timeout", "1");
    opts.set("bt-tracker-timeout", "3");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--bt-tracker-connect-timeout=1 must dest-match after-handshake delayed tracker"
    );
}

#[tokio::test]
async fn peer_connection_timeout_one_misses_unroutable() {
    let body = payload();
    let torrent = bt::build_single_file("pcto.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("pcto.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("connect-timeout", "10");
    opts.set("peer-connection-timeout", "1");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec!["192.0.2.1:9".parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let t0 = std::time::Instant::now();
    let err = bt::download(job).await;
    let elapsed = t0.elapsed();
    assert!(
        err.is_err(),
        "--peer-connection-timeout=1 must miss TEST-NET peer: {err:?}"
    );
    assert!(
        elapsed >= Duration::from_millis(800),
        "peer-connection-timeout must wait ~1s, got {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "peer-connection-timeout must not wait connect-timeout, got {elapsed:?}"
    );
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "unroutable peer must not dest-match"
    );
}

#[tokio::test]
async fn peer_connection_timeout_one_seeder_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("pctok.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("pctok.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "pctok.bin");
    opts.set("peer-connection-timeout", "1");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--peer-connection-timeout=1 must dest-match a reachable seeder"
    );
}

async fn spawn_delayed_piece_seeder(
    info_hash: [u8; 20],
    payload: &'static [u8],
    delay: Duration,
) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                s.set_nodelay(true).ok();
                let mut hs = [0u8; 68];
                if s.read_exact(&mut hs).await.is_err() {
                    return;
                }
                if hs[0] != 19 || &hs[1..20] != PSTR || hs[28..48] != info_hash {
                    return;
                }
                let mine = peer_id();
                if s.write_all(&encode_handshake(&info_hash, &mine)).await.is_err() {
                    return;
                }
                if write_msg(&mut s, MSG_UNCHOKE, &[]).await.is_err() {
                    return;
                }
                let mut delayed = false;
                loop {
                    match tokio::time::timeout(Duration::from_secs(10), read_msg(&mut s)).await {
                        Ok(Ok(Some((MSG_REQUEST, p)))) if p.len() >= 12 => {
                            if !delayed {
                                tokio::time::sleep(delay).await;
                                delayed = true;
                            }
                            let idx = u32::from_be_bytes(p[0..4].try_into().unwrap());
                            let begin = u32::from_be_bytes(p[4..8].try_into().unwrap());
                            let len = u32::from_be_bytes(p[8..12].try_into().unwrap()) as usize;
                            let start = idx as usize * PIECE_LEN as usize + begin as usize;
                            let end = start.saturating_add(len);
                            if end > payload.len() {
                                return;
                            }
                            let mut body = Vec::with_capacity(8 + len);
                            body.extend_from_slice(&idx.to_be_bytes());
                            body.extend_from_slice(&begin.to_be_bytes());
                            body.extend_from_slice(&payload[start..end]);
                            if write_msg(&mut s, MSG_PIECE, &body).await.is_err() {
                                return;
                            }
                        }
                        Ok(Ok(Some(_))) => {}
                        _ => return,
                    }
                }
            });
        }
    });
    port
}

#[tokio::test]
async fn bt_timeout_one_misses_delayed_piece() {
    let body = payload();
    let torrent = bt::build_single_file("bto.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_delayed_piece_seeder(meta.info_hash, body, Duration::from_millis(1500)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("bto.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("connect-timeout", "10");
    opts.set("timeout", "10");
    opts.set("bt-timeout", "1");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let t0 = std::time::Instant::now();
    let err = bt::download(job).await;
    let elapsed = t0.elapsed();
    assert!(
        err.as_ref().is_err_and(|e| e.to_string().contains("bt-timeout")),
        "--bt-timeout=1 must miss 1.5s piece delay: {err:?}"
    );
    assert!(
        elapsed >= Duration::from_millis(800),
        "bt-timeout must wait ~1s, got {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "bt-timeout must not wait connect-timeout, got {elapsed:?}"
    );
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "short bt-timeout must not dest-match delayed piece"
    );
}

#[tokio::test]
async fn bt_timeout_three_delayed_piece_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("btok.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_delayed_piece_seeder(meta.info_hash, body, Duration::from_millis(1500)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("btok.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "btok.bin");
    opts.set("connect-timeout", "10");
    opts.set("timeout", "10");
    opts.set("bt-timeout", "3");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--bt-timeout=3 must dest-match after 1.5s delayed piece"
    );
}

#[tokio::test]
async fn bt_request_timeout_one_misses_delayed_piece() {
    let body = payload();
    let torrent = bt::build_single_file("brto.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_delayed_piece_seeder(meta.info_hash, body, Duration::from_millis(1500)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("brto.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("connect-timeout", "10");
    opts.set("timeout", "10");
    opts.set("bt-timeout", "10");
    opts.set("bt-request-timeout", "1");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let t0 = std::time::Instant::now();
    let err = bt::download(job).await;
    let elapsed = t0.elapsed();
    assert!(
        err.as_ref()
            .is_err_and(|e| e.to_string().contains("bt-request-timeout")),
        "--bt-request-timeout=1 must miss 1.5s piece delay: {err:?}"
    );
    assert!(
        elapsed >= Duration::from_millis(800),
        "bt-request-timeout must wait ~1s, got {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "bt-request-timeout must not wait bt-timeout, got {elapsed:?}"
    );
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "short bt-request-timeout must not dest-match delayed piece"
    );
}

#[tokio::test]
async fn bt_request_timeout_three_delayed_piece_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("brtok.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_delayed_piece_seeder(meta.info_hash, body, Duration::from_millis(1500)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("brtok.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "brtok.bin");
    opts.set("connect-timeout", "10");
    opts.set("timeout", "10");
    opts.set("bt-timeout", "10");
    opts.set("bt-request-timeout", "3");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--bt-request-timeout=3 must dest-match after 1.5s delayed piece"
    );
}

async fn spawn_keep_alive_seeder(info_hash: [u8; 20], payload: &'static [u8]) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                s.set_nodelay(true).ok();
                let mut hs = [0u8; 68];
                if s.read_exact(&mut hs).await.is_err() {
                    return;
                }
                if hs[0] != 19 || &hs[1..20] != PSTR || hs[28..48] != info_hash {
                    return;
                }
                let mine = peer_id();
                if s.write_all(&encode_handshake(&info_hash, &mine)).await.is_err() {
                    return;
                }
                loop {
                    match tokio::time::timeout(Duration::from_secs(8), read_msg(&mut s)).await {
                        Ok(Ok(Some((255, _)))) => break,
                        Ok(Ok(Some(_))) => {}
                        _ => return,
                    }
                }
                if write_msg(&mut s, MSG_UNCHOKE, &[]).await.is_err() {
                    return;
                }
                loop {
                    match tokio::time::timeout(Duration::from_secs(10), read_msg(&mut s)).await {
                        Ok(Ok(Some((MSG_REQUEST, p)))) if p.len() >= 12 => {
                            let idx = u32::from_be_bytes(p[0..4].try_into().unwrap());
                            let begin = u32::from_be_bytes(p[4..8].try_into().unwrap());
                            let len = u32::from_be_bytes(p[8..12].try_into().unwrap()) as usize;
                            let start = idx as usize * PIECE_LEN as usize + begin as usize;
                            let end = start.saturating_add(len);
                            if end > payload.len() {
                                return;
                            }
                            let mut body = Vec::with_capacity(8 + len);
                            body.extend_from_slice(&idx.to_be_bytes());
                            body.extend_from_slice(&begin.to_be_bytes());
                            body.extend_from_slice(&payload[start..end]);
                            if write_msg(&mut s, MSG_PIECE, &body).await.is_err() {
                                return;
                            }
                        }
                        Ok(Ok(Some(_))) => {}
                        _ => return,
                    }
                }
            });
        }
    });
    port
}

#[tokio::test]
async fn bt_keep_alive_interval_sixty_misses_ka_seeder() {
    let body = payload();
    let torrent = bt::build_single_file("bka0.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_keep_alive_seeder(meta.info_hash, body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("bka0.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("connect-timeout", "3");
    opts.set("timeout", "10");
    opts.set("bt-keep-alive-interval", "60");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let err = bt::download(job).await;
    assert!(
        err.is_err(),
        "--bt-keep-alive-interval=60 must not unchoke a KA-gated seeder: {err:?}"
    );
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "long keep-alive interval must not dest-match KA-gated seeder"
    );
}

#[tokio::test]
async fn bt_keep_alive_interval_one_ka_seeder_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("bka.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_keep_alive_seeder(meta.info_hash, body).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("bka.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "bka.bin");
    opts.set("connect-timeout", "10");
    opts.set("timeout", "10");
    opts.set("bt-keep-alive-interval", "1");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--bt-keep-alive-interval=1 must dest-match a seeder that unchokes on keep-alive"
    );
}

#[tokio::test]
async fn bt_tracker_interval_one_reannounces_during_seed() {
    let body = payload();
    let hits = Arc::new(AtomicU64::new(0));
    let torrent = bt::build_single_file("tiv.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let tr = spawn_tracker_counting(seed, Arc::clone(&hits)).await;
    let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let listen = probe.local_addr().unwrap().port();
    drop(probe);
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("tiv.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "tiv.bin");
    opts.set("bt-exclude-tracker", "*");
    opts.set("bt-tracker", format!("http://127.0.0.1:{tr}/announce"));
    opts.set("bt-tracker-interval", "1");
    opts.set("seed-ratio", "99");
    opts.set("seed-time", "0.04");
    opts.set("listen-port", listen.to_string());
    opts.set("timeout", "8");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    let n = hits.load(Ordering::SeqCst);
    assert!(
        n >= 2,
        "--bt-tracker-interval=1 must re-announce during seed, hits={n}"
    );
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--bt-tracker-interval=1 must dest-match"
    );
}

#[tokio::test]
async fn bt_tracker_interval_zero_no_reannounce_during_seed() {
    let body = payload();
    let hits = Arc::new(AtomicU64::new(0));
    let torrent = bt::build_single_file("tiv0.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let tr = spawn_tracker_counting(seed, Arc::clone(&hits)).await;
    let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let listen = probe.local_addr().unwrap().port();
    drop(probe);
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("tiv0.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "tiv0.bin");
    opts.set("bt-exclude-tracker", "*");
    opts.set("bt-tracker", format!("http://127.0.0.1:{tr}/announce"));
    opts.set("seed-ratio", "99");
    opts.set("seed-time", "0.04");
    opts.set("listen-port", listen.to_string());
    opts.set("timeout", "8");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    let n = hits.load(Ordering::SeqCst);
    assert_eq!(
        n, 1,
        "--bt-tracker-interval=0 must not re-announce inside a short seed, hits={n}"
    );
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--bt-tracker-interval=0 must dest-match"
    );
}

#[tokio::test]
async fn peer_id_prefix_handshake_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("pid.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("pid.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "pid.bin");
    opts.set("peer-id-prefix", "GRK-");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--peer-id-prefix=GRK- must dest-match"
    );
    let id = *LAST_CLIENT_PEER_ID.lock().unwrap();
    assert_eq!(&id[..4], b"GRK-", "handshake peer_id must start with prefix, got {id:?}");
}

#[tokio::test]
async fn peer_id_prefix_truncated_handshake_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("pidt.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("pidt.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "pidt.bin");
    opts.set("peer-id-prefix", "ABCDEFGHIJKLMNOPQRSTUVWXYZ");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "truncated --peer-id-prefix must dest-match"
    );
    let id = *LAST_CLIENT_PEER_ID.lock().unwrap();
    assert_eq!(&id, b"ABCDEFGHIJKLMNOPQRST", "prefix over 20 bytes must truncate");
}

#[tokio::test]
async fn peer_agent_ltep_v_dest_match() {
    let body = payload();
    *LAST_PEER_AGENT.lock().unwrap() = String::new();
    let torrent = bt::build_single_file("pag.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("pag.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "pag.bin");
    opts.set("peer-agent", "GrokBT");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--peer-agent=GrokBT must dest-match"
    );
    let agent = LAST_PEER_AGENT.lock().unwrap().clone();
    assert_eq!(agent, "GrokBT", "LTEP v must be --peer-agent, got {agent:?}");
}

#[tokio::test]
async fn peer_agent_default_ltep_v_dest_match() {
    let body = payload();
    *LAST_PEER_AGENT.lock().unwrap() = String::new();
    let torrent = bt::build_single_file("pagd.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("pagd.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "pagd.bin");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "default peer-agent must dest-match"
    );
    let agent = LAST_PEER_AGENT.lock().unwrap().clone();
    assert_eq!(
        agent, "aria2-rust",
        "unset --peer-agent must send default LTEP v, got {agent:?}"
    );
}

#[tokio::test]
async fn bt_external_ip_announce_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("extip.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let seen = Arc::new(Mutex::new(String::new()));
    let tr = spawn_tracker_ip(seed, Arc::clone(&seen)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("extip.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "extip.bin");
    opts.set("bt-exclude-tracker", "*");
    opts.set("bt-tracker", format!("http://127.0.0.1:{tr}/announce"));
    opts.set("bt-external-ip", "203.0.113.7");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--bt-external-ip must dest-match"
    );
    assert_eq!(
        seen.lock().unwrap().as_str(),
        "203.0.113.7",
        "tracker announce must include ip=203.0.113.7"
    );
}

#[tokio::test]
async fn bt_external_ip_unset_omits_ip_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("noextip.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let seen = Arc::new(Mutex::new(String::new()));
    let tr = spawn_tracker_ip(seed, Arc::clone(&seen)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("noextip.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "noextip.bin");
    opts.set("bt-exclude-tracker", "*");
    opts.set("bt-tracker", format!("http://127.0.0.1:{tr}/announce"));
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "unset --bt-external-ip must dest-match"
    );
    assert!(
        seen.lock().unwrap().is_empty(),
        "unset --bt-external-ip must omit ip= from announce"
    );
}

#[tokio::test]
async fn bt_require_crypto_mse_rc4_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("mse.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_mse_seeder(meta.info_hash, body, aria2_rust::mse::CRYPTO_RC4).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("mse.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "mse.bin");
    opts.set("bt-require-crypto", "true");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--bt-require-crypto MSE/RC4 must dest-match"
    );
}

#[tokio::test]
async fn bt_require_crypto_plain_peer_no_dest() {
    let body = payload();
    let torrent = bt::build_single_file("msen.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("msen.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "msen.bin");
    opts.set("bt-require-crypto", "true");
    opts.set("timeout", "3");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let r = bt::download(job).await;
    assert!(r.is_err(), "plaintext seeder must fail under --bt-require-crypto");
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "must not dest-match plaintext peer when crypto required"
    );
}

#[tokio::test]
async fn bt_force_encryption_mse_rc4_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("msef.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_mse_seeder(meta.info_hash, body, aria2_rust::mse::CRYPTO_RC4).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("msef.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "msef.bin");
    opts.set("bt-force-encryption", "true");
    aria2_rust::sockopt::reset_send();
    aria2_rust::sockopt::reset_recv();
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--bt-force-encryption MSE/RC4 must dest-match"
    );
    assert!(
        aria2_rust::sockopt::last_send() >= 5,
        "C++ SocketCore::writeData send must write MSE handshake and BT messages"
    );
    assert!(
        aria2_rust::sockopt::last_recv() >= 5,
        "C++ SocketCore::readData recv must read MSE handshake and BT messages"
    );
}

#[tokio::test]
async fn bt_min_crypto_level_arc4_mse_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("msea.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_mse_seeder(meta.info_hash, body, aria2_rust::mse::CRYPTO_RC4).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("msea.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "msea.bin");
    opts.set("bt-require-crypto", "true");
    opts.set("bt-min-crypto-level", "arc4");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--bt-min-crypto-level=arc4 MSE/RC4 must dest-match"
    );
}

#[tokio::test]
async fn bt_prioritize_piece_unset_first_is_zero_dest_match() {
    let body = payload();
    let first = Arc::new(AtomicU16::new(u16::MAX));
    let torrent = bt::build_single_file("prio0.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    assert!(meta.num_pieces() >= 2);
    let seed = spawn_prio_seeder(meta.info_hash, body, Arc::clone(&first)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("prio0.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "prio0.bin");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "unset --bt-prioritize-piece must dest-match"
    );
    assert_eq!(
        first.load(Ordering::SeqCst),
        0,
        "unset prioritize must request piece 0 first"
    );
}

#[tokio::test]
async fn bt_prioritize_piece_tail_last_first_dest_match() {
    let body = payload();
    let first = Arc::new(AtomicU16::new(u16::MAX));
    let torrent = bt::build_single_file("priot.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let last = (meta.num_pieces() - 1) as u16;
    let seed = spawn_prio_seeder(meta.info_hash, body, Arc::clone(&first)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("priot.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "priot.bin");
    opts.set("bt-prioritize-piece", "tail=16K");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--bt-prioritize-piece=tail=16K must dest-match"
    );
    assert_eq!(
        first.load(Ordering::SeqCst),
        last,
        "tail=16K must request last piece first, got {}",
        first.load(Ordering::SeqCst)
    );
}

async fn spawn_lpd_announcer(dest: SocketAddr, info_hash: [u8; 20], bt_port: u16) {
    tokio::spawn(async move {
        let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let msg = aria2_rust::lpd::encode_announce(&info_hash, bt_port, "seeder");
        for _ in 0..40 {
            let _ = sock.send_to(&msg, dest).await;
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });
}

#[tokio::test]
async fn bt_lpd_bt_search_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("lpd.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let probe = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let lpd_addr = probe.local_addr().unwrap();
    drop(probe);
    spawn_lpd_announcer(lpd_addr, meta.info_hash, seed).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("lpd.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "lpd.bin");
    opts.set("timeout", "2");
    opts.set("bt-enable-lpd", "true");
    opts.set("bt-lpd-interface", lpd_addr.to_string());
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "LPD BT-SEARCH Port must dest-match seeder bytes"
    );
}

#[tokio::test]
async fn bt_lpd_disabled_ignores_announce() {
    let body = payload();
    let torrent = bt::build_single_file("nolpd.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let probe = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let lpd_addr = probe.local_addr().unwrap();
    drop(probe);
    spawn_lpd_announcer(lpd_addr, meta.info_hash, seed).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("nolpd.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("timeout", "2");
    opts.set("bt-enable-lpd", "false");
    opts.set("bt-lpd-interface", lpd_addr.to_string());
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let err = bt::download(job).await;
    assert!(err.is_err(), "--bt-enable-lpd=false must not use LPD peers");
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "disabled LPD must not write seeder payload"
    );
}

#[tokio::test]
async fn session_lpd_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("slpd.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let probe = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let lpd_addr = probe.local_addr().unwrap();
    drop(probe);
    spawn_lpd_announcer(lpd_addr, meta.info_hash, seed).await;
    let dir = tempfile::tempdir().unwrap();
    let torrent_path = dir.path().join("slpd.torrent");
    std::fs::write(&torrent_path, &torrent).unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("torrent-file", torrent_path.display().to_string());
    opts.set("timeout", "2");
    opts.set("bt-enable-lpd", "true");
    opts.set("bt-lpd-interface", lpd_addr.to_string());
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("magnet:?xt=urn:btih:{}&dn=slpd.bin", hex::encode(meta.info_hash))],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("slpd.bin")).unwrap(), body);
}

#[tokio::test]
async fn bt_select_file_first_dest_match() {
    let a = vec![0xa5u8; 40 * 1024];
    let b = vec![0x5au8; 40 * 1024];
    let mut concat = a.clone();
    concat.extend_from_slice(&b);
    let body: &'static [u8] = Box::leak(concat.into_boxed_slice());
    let torrent = bt::build_multi_file(
        "sel",
        PIECE_LEN,
        &[("one.bin", &body[..40 * 1024]), ("two.bin", &body[40 * 1024..])],
        "http://127.0.0.1:1/announce",
    );
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    assert_eq!(meta.files.len(), 2);
    assert!(meta.num_pieces() >= 2);
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("sel");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("select-file", "1");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(dest.join("one.bin")).unwrap(),
        &body[..40 * 1024],
        "select-file=1 must dest-match first file"
    );
    assert!(
        !dest.join("two.bin").exists(),
        "select-file=1 must not write second file"
    );
}

#[tokio::test]
async fn bt_multidisk_nested_file_mkdirs_dest_match() {
    let data: &'static [u8] = Box::leak(vec![0xCCu8; 4 * 1024].into_boxed_slice());
    let torrent = bt::build_multi_file(
        "nest",
        PIECE_LEN,
        &[("sub/dir/leaf.bin", data)],
        "http://127.0.0.1:1/announce",
    );
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, data, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("nest");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    aria2_rust::storage::reset_mkdir();
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(dest.join("sub/dir/leaf.bin")).unwrap(),
        data,
        "C++ MultiDiskAdaptor File::mkdirs must dest-match nested torrent path"
    );
    assert!(
        aria2_rust::storage::last_mkdir() >= 2,
        "torrent root + nested file parents"
    );
}

#[tokio::test]
async fn bt_select_file_second_dest_match() {
    let a = vec![0xa5u8; 40 * 1024];
    let b = vec![0x5au8; 40 * 1024];
    let mut concat = a.clone();
    concat.extend_from_slice(&b);
    let body: &'static [u8] = Box::leak(concat.into_boxed_slice());
    let torrent = bt::build_multi_file(
        "sel2",
        PIECE_LEN,
        &[("one.bin", &body[..40 * 1024]), ("two.bin", &body[40 * 1024..])],
        "http://127.0.0.1:1/announce",
    );
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("sel2");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("select-file", "2");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(dest.join("two.bin")).unwrap(),
        &body[40 * 1024..],
        "select-file=2 must dest-match second file"
    );
    assert!(
        !dest.join("one.bin").exists(),
        "select-file=2 must not write first file"
    );
}

#[tokio::test]
async fn bt_index_out_first_file_dest_match() {
    let a = vec![0xa5u8; 40 * 1024];
    let b = vec![0x5au8; 40 * 1024];
    let mut concat = a.clone();
    concat.extend_from_slice(&b);
    let body: &'static [u8] = Box::leak(concat.into_boxed_slice());
    let torrent = bt::build_multi_file(
        "idx",
        PIECE_LEN,
        &[("one.bin", &body[..40 * 1024]), ("two.bin", &body[40 * 1024..])],
        "http://127.0.0.1:1/announce",
    );
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("idx");
    let custom = dir.path().join("custom.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("index-out", "1=custom.bin");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&custom).unwrap(),
        &body[..40 * 1024],
        "--index-out=1=custom.bin must dest-match under --dir"
    );
    assert!(
        !dest.join("one.bin").exists(),
        "index-out must not write the torrent-relative first file"
    );
    assert_eq!(
        std::fs::read(dest.join("two.bin")).unwrap(),
        &body[40 * 1024..],
        "unmapped file 2 must stay at torrent path"
    );
}

#[tokio::test]
async fn bt_index_out_second_file_dest_match() {
    let a = vec![0xa5u8; 40 * 1024];
    let b = vec![0x5au8; 40 * 1024];
    let mut concat = a.clone();
    concat.extend_from_slice(&b);
    let body: &'static [u8] = Box::leak(concat.into_boxed_slice());
    let torrent = bt::build_multi_file(
        "idx2",
        PIECE_LEN,
        &[("one.bin", &body[..40 * 1024]), ("two.bin", &body[40 * 1024..])],
        "http://127.0.0.1:1/announce",
    );
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("idx2");
    let custom = dir.path().join("renamed2.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("index-out", "2=renamed2.bin");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&custom).unwrap(),
        &body[40 * 1024..],
        "--index-out=2=renamed2.bin must dest-match under --dir"
    );
    assert!(
        !dest.join("two.bin").exists(),
        "index-out must not write the torrent-relative second file"
    );
    assert_eq!(
        std::fs::read(dest.join("one.bin")).unwrap(),
        &body[..40 * 1024],
        "unmapped file 1 must stay at torrent path"
    );
}

#[tokio::test]
async fn bt_show_files_lists_and_skips_dest() {
    let a = vec![0xa5u8; 40 * 1024];
    let b = vec![0x5au8; 40 * 1024];
    let mut concat = a.clone();
    concat.extend_from_slice(&b);
    let body: &'static [u8] = Box::leak(concat.into_boxed_slice());
    let torrent = bt::build_multi_file(
        "shown",
        PIECE_LEN,
        &[("one.bin", &body[..40 * 1024]), ("two.bin", &body[40 * 1024..])],
        "http://127.0.0.1:1/announce",
    );
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("shown");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("show-files", "true");
    let session = Session::new(opts).unwrap();
    let mut extra = OptionSet::new();
    extra.set("show-files", "true");
    let gid = session
        .add_torrent_and_start(
            torrent,
            vec![magnet_pe(format!("127.0.0.1:{seed}").parse().unwrap())],
            extra,
        )
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    let listing = session.stdout_text();
    assert!(listing.contains("one.bin"), "{listing}");
    assert!(listing.contains("two.bin"), "{listing}");
    assert!(
        !dest.exists()
            && !dir.path().join("one.bin").exists()
            && !dir.path().join("two.bin").exists(),
        "--show-files must not write dest"
    );
}

#[tokio::test]
async fn bt_show_files_false_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("showno.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("show-files", "false");
    let session = Session::new(opts).unwrap();
    let mut extra = OptionSet::new();
    extra.set("out", "showno.bin");
    extra.set("show-files", "false");
    let gid = session
        .add_torrent_and_start(
            torrent,
            vec![magnet_pe(format!("127.0.0.1:{seed}").parse().unwrap())],
            extra,
        )
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(
        std::fs::read(dir.path().join("showno.bin")).unwrap(),
        body,
        "--show-files=false must dest-match"
    );
    assert!(!session.stdout_text().contains("Files:"));
}

#[tokio::test]
async fn bt_max_open_files_one_dest_match() {
    let n = PIECE_LEN as usize;
    let a = vec![0x11u8; n];
    let b = vec![0x22u8; n];
    let c = vec![0x33u8; n];
    let mut concat = a.clone();
    concat.extend_from_slice(&b);
    concat.extend_from_slice(&c);
    let body: &'static [u8] = Box::leak(concat.into_boxed_slice());
    let torrent = bt::build_multi_file(
        "mof1",
        PIECE_LEN,
        &[
            ("a.bin", &body[..n]),
            ("b.bin", &body[n..2 * n]),
            ("c.bin", &body[2 * n..]),
        ],
        "http://127.0.0.1:1/announce",
    );
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("mof1");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("bt-max-open-files", "1");
    opts.set("disk-cache", "0");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(std::fs::read(dest.join("a.bin")).unwrap(), &body[..n]);
    assert_eq!(std::fs::read(dest.join("b.bin")).unwrap(), &body[n..2 * n]);
    assert_eq!(std::fs::read(dest.join("c.bin")).unwrap(), &body[2 * n..]);
    let peak = aria2_rust::storage::last_open_peak();
    assert!(
        peak >= 1 && peak <= 1,
        "--bt-max-open-files=1 peak must be 1, got {peak}"
    );
}

#[tokio::test]
async fn bt_max_open_files_three_peak_dest_match() {
    let n = PIECE_LEN as usize;
    let a = vec![0x41u8; n];
    let b = vec![0x42u8; n];
    let c = vec![0x43u8; n];
    let mut concat = a.clone();
    concat.extend_from_slice(&b);
    concat.extend_from_slice(&c);
    let body: &'static [u8] = Box::leak(concat.into_boxed_slice());
    let torrent = bt::build_multi_file(
        "mof3",
        PIECE_LEN,
        &[
            ("a.bin", &body[..n]),
            ("b.bin", &body[n..2 * n]),
            ("c.bin", &body[2 * n..]),
        ],
        "http://127.0.0.1:1/announce",
    );
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("mof3");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("bt-max-open-files", "100");
    opts.set("disk-cache", "0");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(std::fs::read(dest.join("a.bin")).unwrap(), &body[..n]);
    assert_eq!(std::fs::read(dest.join("b.bin")).unwrap(), &body[n..2 * n]);
    assert_eq!(std::fs::read(dest.join("c.bin")).unwrap(), &body[2 * n..]);
    let peak = aria2_rust::storage::last_open_peak();
    assert_eq!(
        peak, 3,
        "--bt-max-open-files=100 must keep 3 files open, peak {peak}"
    );
}

#[tokio::test]
async fn bt_remove_unselected_file_true_dest_match() {
    let a = vec![0xa5u8; 40 * 1024];
    let b = vec![0x5au8; 40 * 1024];
    let mut concat = a.clone();
    concat.extend_from_slice(&b);
    let body: &'static [u8] = Box::leak(concat.into_boxed_slice());
    let torrent = bt::build_multi_file(
        "rmu",
        PIECE_LEN,
        &[("one.bin", &body[..40 * 1024]), ("two.bin", &body[40 * 1024..])],
        "http://127.0.0.1:1/announce",
    );
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("rmu");
    std::fs::create_dir_all(&dest).unwrap();
    std::fs::write(dest.join("two.bin"), b"padding-junk").unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("select-file", "1");
    opts.set("bt-remove-unselected-file", "true");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(dest.join("one.bin")).unwrap(),
        &body[..40 * 1024],
        "--bt-remove-unselected-file must dest-match selected file"
    );
    assert!(
        !dest.join("two.bin").exists(),
        "unselected padding file must be removed"
    );
}

#[tokio::test]
async fn bt_remove_unselected_file_false_keeps_padding_dest_match() {
    let a = vec![0xa5u8; 40 * 1024];
    let b = vec![0x5au8; 40 * 1024];
    let mut concat = a.clone();
    concat.extend_from_slice(&b);
    let body: &'static [u8] = Box::leak(concat.into_boxed_slice());
    let torrent = bt::build_multi_file(
        "rmk",
        PIECE_LEN,
        &[("one.bin", &body[..40 * 1024]), ("two.bin", &body[40 * 1024..])],
        "http://127.0.0.1:1/announce",
    );
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("rmk");
    std::fs::create_dir_all(&dest).unwrap();
    std::fs::write(dest.join("two.bin"), b"padding-junk").unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("select-file", "1");
    opts.set("bt-remove-unselected-file", "false");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(dest.join("one.bin")).unwrap(),
        &body[..40 * 1024],
        "unset-remove must dest-match selected file"
    );
    assert_eq!(
        std::fs::read(dest.join("two.bin")).unwrap(),
        b"padding-junk",
        "bt-remove-unselected-file=false must keep padding file"
    );
}

#[tokio::test]
async fn bt_detach_seed_only_true_http_dest_match() {
    let body = payload();
    let http_body: &'static [u8] = b"detach-http-payload-bytes";
    let torrent = bt::build_single_file("det.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("det.bin"), body).unwrap();
    let http_port = spawn_http_body(http_body).await;
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().display().to_string());
    opts.set("file-allocation", "none");
    opts.set("timeout", "8");
    opts.set("connect-timeout", "4");
    opts.set("enable-dht", "false");
    opts.set("dht-listen-port", "0");
    opts.set("listen-port", "0");
    opts.set("seed-ratio", "100");
    opts.set("bt-seed-unverified", "true");
    opts.set("max-concurrent-downloads", "1");
    opts.set("bt-detach-seed-only", "true");
    opts.set("split", "1");
    let session = Session::new(opts).unwrap();
    let bt_gid = session
        .add_torrent_and_start(torrent, vec![], OptionSet::new())
        .await
        .unwrap();
    let t0 = tokio::time::Instant::now();
    loop {
        let st = session.tell_status(bt_gid.as_str()).await.unwrap();
        let done: u64 = st
            .get("completedLength")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        if done == body.len() as u64 {
            break;
        }
        if t0.elapsed() > Duration::from_secs(3) {
            panic!("seed-unverified never completed: {st}");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(40)).await;
    let mut extra = OptionSet::new();
    extra.set("out", "http.bin");
    let http_gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{http_port}/http.bin")],
            extra,
        )
        .await
        .unwrap();
    wait_complete(&session, http_gid.as_str()).await;
    assert_eq!(
        std::fs::read(dir.path().join("http.bin")).unwrap(),
        http_body,
        "--bt-detach-seed-only=true must dest-match HTTP while BT seeds"
    );
    let bt_st = session.tell_status(bt_gid.as_str()).await.unwrap();
    assert_eq!(
        bt_st.get("status").and_then(|v| v.as_str()),
        Some("active"),
        "BT seed-only must stay active while HTTP completes"
    );
    assert_eq!(std::fs::read(dir.path().join("det.bin")).unwrap(), body);
}

#[tokio::test]
async fn bt_detach_seed_only_false_http_waits_bt_dest_match() {
    let body = payload();
    let http_body: &'static [u8] = b"attach-http-payload-bytes";
    let torrent = bt::build_single_file("detf.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("detf.bin"), body).unwrap();
    let http_port = spawn_http_body(http_body).await;
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().display().to_string());
    opts.set("file-allocation", "none");
    opts.set("timeout", "8");
    opts.set("connect-timeout", "4");
    opts.set("enable-dht", "false");
    opts.set("dht-listen-port", "0");
    opts.set("listen-port", "0");
    opts.set("seed-ratio", "100");
    opts.set("bt-seed-unverified", "true");
    opts.set("max-concurrent-downloads", "1");
    opts.set("bt-detach-seed-only", "false");
    opts.set("split", "1");
    let session = Session::new(opts).unwrap();
    let bt_gid = session
        .add_torrent_and_start(torrent, vec![], OptionSet::new())
        .await
        .unwrap();
    let t0 = tokio::time::Instant::now();
    loop {
        if dir.path().join("detf.bin").exists()
            && std::fs::read(dir.path().join("detf.bin")).ok().as_deref() == Some(body)
        {
            let st = session.tell_status(bt_gid.as_str()).await.unwrap();
            if st.get("status").and_then(|v| v.as_str()) == Some("active") {
                break;
            }
        }
        if t0.elapsed() > Duration::from_secs(3) {
            panic!("BT seed-only never became active");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(40)).await;
    let mut extra = OptionSet::new();
    extra.set("out", "httpf.bin");
    let http_gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{http_port}/httpf.bin")],
            extra,
        )
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let http_st = session.tell_status(http_gid.as_str()).await.unwrap();
    assert_eq!(
        http_st.get("status").and_then(|v| v.as_str()),
        Some("waiting"),
        "without detach, HTTP must wait on seed-only slot"
    );
    assert!(
        !dir.path().join("httpf.bin").exists(),
        "HTTP dest must not exist while seed occupies the slot"
    );
    assert_eq!(
        std::fs::read(dir.path().join("detf.bin")).unwrap(),
        body,
        "BT dest-match while seed-only occupies concurrent slot"
    );
}

#[tokio::test]
async fn session_select_file_dest_match() {
    let a = vec![0xa5u8; 40 * 1024];
    let b = vec![0x5au8; 40 * 1024];
    let mut concat = a.clone();
    concat.extend_from_slice(&b);
    let body: &'static [u8] = Box::leak(concat.into_boxed_slice());
    let torrent = bt::build_multi_file(
        "ssel",
        PIECE_LEN,
        &[("one.bin", &body[..40 * 1024]), ("two.bin", &body[40 * 1024..])],
        "http://127.0.0.1:1/announce",
    );
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let torrent_path = dir.path().join("ssel.torrent");
    std::fs::write(&torrent_path, &torrent).unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("torrent-file", torrent_path.display().to_string());
    opts.set("select-file", "1");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!(
                "magnet:?xt=urn:btih:{}&dn=ssel&x.pe=127.0.0.1:{seed}",
                hex::encode(meta.info_hash)
            )],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(
        std::fs::read(dir.path().join("ssel").join("one.bin")).unwrap(),
        &body[..40 * 1024]
    );
    assert!(!dir.path().join("ssel").join("two.bin").exists());
}

#[tokio::test]
async fn get_files_select_file_dest_match() {
    let a = vec![0xa5u8; 40 * 1024];
    let b = vec![0x5au8; 40 * 1024];
    let mut concat = a.clone();
    concat.extend_from_slice(&b);
    let body: &'static [u8] = Box::leak(concat.into_boxed_slice());
    let torrent = bt::build_multi_file(
        "gfiles",
        PIECE_LEN,
        &[("one.bin", &body[..40 * 1024]), ("two.bin", &body[40 * 1024..])],
        "http://127.0.0.1:1/announce",
    );
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let torrent_path = dir.path().join("gfiles.torrent");
    std::fs::write(&torrent_path, &torrent).unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("torrent-file", torrent_path.display().to_string());
    opts.set("select-file", "1");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!(
                "magnet:?xt=urn:btih:{}&dn=gfiles&x.pe=127.0.0.1:{seed}",
                hex::encode(meta.info_hash)
            )],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    let files = session.get_files(gid.as_str()).await.unwrap();
    let arr = files.as_array().expect("files array");
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0].get("index").and_then(|v| v.as_str()), Some("1"));
    assert_eq!(arr[0].get("selected").and_then(|v| v.as_str()), Some("true"));
    assert_eq!(arr[1].get("index").and_then(|v| v.as_str()), Some("2"));
    assert_eq!(arr[1].get("selected").and_then(|v| v.as_str()), Some("false"));
    let p1 = arr[0].get("path").and_then(|v| v.as_str()).unwrap();
    let p2 = arr[1].get("path").and_then(|v| v.as_str()).unwrap();
    assert_eq!(std::fs::read(p1).unwrap(), &body[..40 * 1024]);
    assert!(!std::path::Path::new(p2).exists());
    let done1: u64 = arr[0]
        .get("completedLength")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap();
    let done2: u64 = arr[1]
        .get("completedLength")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap();
    assert_eq!(done1, 40 * 1024);
    assert_eq!(done2, 0);
    let uris = session.get_uris(gid.as_str()).await.unwrap();
    let uarr = uris.as_array().expect("uris");
    assert_eq!(uarr.len(), 1);
    assert_eq!(uarr[0].get("status").and_then(|v| v.as_str()), Some("used"));
}

#[tokio::test]
async fn max_upload_limit_seed_throttled_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0xCDu8; 4000].into_boxed_slice());
    let torrent = bt::build_single_file("upl.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let listen = probe.local_addr().unwrap().port();
    drop(probe);
    let dir = tempfile::tempdir().unwrap();
    let dest1 = dir.path().join("upl.bin");
    std::fs::write(&dest1, body).unwrap();
    let dest2 = dir.path().join("upl-leech.bin");
    let mut opts1 = job_opts();
    opts1.set("dir", dir.path().display().to_string());
    opts1.set("out", "upl.bin");
    opts1.set("seed-ratio", "1.0");
    opts1.set("bt-seed-unverified", "true");
    opts1.set("max-upload-limit", "2000");
    opts1.set("listen-port", listen.to_string());
    opts1.set("timeout", "15");
    let mut opts2 = job_opts();
    opts2.set("dir", dir.path().display().to_string());
    opts2.set("out", "upl-leech.bin");
    opts2.set("seed-ratio", "0");
    opts2.set("connect-timeout", "8");
    opts2.set("timeout", "15");
    let torrent2 = torrent.clone();
    let (_tx1, rx1) = watch::channel(false);
    let job1 = BtJob {
        torrent,
        dest: dest1.clone(),
        peers: vec![],
        opts: opts1,
        progress: HttpProgress::new(),
        cancel: rx1,
    };
    let seeder_task = tokio::spawn(async move { bt::download(job1).await });
    tokio::time::sleep(Duration::from_millis(80)).await;
    let t0 = std::time::Instant::now();
    let mut last = None;
    for _ in 0..8 {
        let (_tx, rx) = watch::channel(false);
        let j = BtJob {
            torrent: torrent2.clone(),
            dest: dest2.clone(),
            peers: vec![format!("127.0.0.1:{listen}").parse().unwrap()],
            opts: opts2.clone(),
            progress: HttpProgress::new(),
            cancel: rx,
        };
        match bt::download(j).await {
            Ok(()) => {
                last = None;
                break;
            }
            Err(e) => {
                last = Some(e.to_string());
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
    if let Some(e) = last {
        panic!("leecher failed: {e}");
    }
    assert_eq!(
        bt::last_upload_limit(),
        2000,
        "seed_conn must see --max-upload-limit=2000, got {}",
        bt::last_upload_limit()
    );
    assert!(
        t0.elapsed() >= std::time::Duration::from_millis(1500),
        "--max-upload-limit=2000 must throttle 4000-byte seed, elapsed {:?}",
        t0.elapsed()
    );
    assert_eq!(
        std::fs::read(&dest2).unwrap(),
        body,
        "--max-upload-limit must dest-match leecher bytes"
    );
    let _ = seeder_task.await;
}

#[tokio::test]
async fn max_overall_upload_limit_seed_throttled_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0xCEu8; 4000].into_boxed_slice());
    let torrent = bt::build_single_file("ovu.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let listen = probe.local_addr().unwrap().port();
    drop(probe);
    let dir = tempfile::tempdir().unwrap();
    let dest1 = dir.path().join("ovu.bin");
    std::fs::write(&dest1, body).unwrap();
    let dest2 = dir.path().join("ovu-leech.bin");
    let mut opts1 = job_opts();
    opts1.set("dir", dir.path().display().to_string());
    opts1.set("out", "ovu.bin");
    opts1.set("seed-ratio", "1.0");
    opts1.set("bt-seed-unverified", "true");
    opts1.set("max-overall-upload-limit", "2000");
    opts1.set("listen-port", listen.to_string());
    opts1.set("timeout", "15");
    let mut opts2 = job_opts();
    opts2.set("dir", dir.path().display().to_string());
    opts2.set("out", "ovu-leech.bin");
    opts2.set("seed-ratio", "0");
    opts2.set("connect-timeout", "8");
    opts2.set("timeout", "15");
    let torrent2 = torrent.clone();
    let (_tx1, rx1) = watch::channel(false);
    let job1 = BtJob {
        torrent,
        dest: dest1.clone(),
        peers: vec![],
        opts: opts1,
        progress: HttpProgress::new(),
        cancel: rx1,
    };
    let seeder_task = tokio::spawn(async move { bt::download(job1).await });
    tokio::time::sleep(Duration::from_millis(80)).await;
    let t0 = std::time::Instant::now();
    let mut last = None;
    for _ in 0..8 {
        let (_tx, rx) = watch::channel(false);
        let j = BtJob {
            torrent: torrent2.clone(),
            dest: dest2.clone(),
            peers: vec![format!("127.0.0.1:{listen}").parse().unwrap()],
            opts: opts2.clone(),
            progress: HttpProgress::new(),
            cancel: rx,
        };
        match bt::download(j).await {
            Ok(()) => {
                last = None;
                break;
            }
            Err(e) => {
                last = Some(e.to_string());
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
    if let Some(e) = last {
        panic!("leecher failed: {e}");
    }
    assert_eq!(
        bt::last_overall_upload_limit(),
        2000,
        "seed_conn must see --max-overall-upload-limit=2000, got {}",
        bt::last_overall_upload_limit()
    );
    assert_eq!(
        bt::last_upload_limit(),
        0,
        "per-torrent max-upload-limit must stay 0"
    );
    assert!(
        t0.elapsed() >= std::time::Duration::from_millis(1500),
        "--max-overall-upload-limit=2000 must throttle 4000-byte seed, elapsed {:?}",
        t0.elapsed()
    );
    assert_eq!(
        std::fs::read(&dest2).unwrap(),
        body,
        "--max-overall-upload-limit must dest-match leecher bytes"
    );
    let _ = seeder_task.await;
}

#[tokio::test]
async fn bt_seed_ratio_uploads_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("seed.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let listen = probe.local_addr().unwrap().port();
    drop(probe);
    let dir = tempfile::tempdir().unwrap();
    let dest1 = dir.path().join("seed.bin");
    let dest2 = dir.path().join("leech.bin");
    let mut opts1 = job_opts();
    opts1.set("dir", dir.path().display().to_string());
    opts1.set("out", "seed.bin");
    opts1.set("seed-ratio", "1.0");
    opts1.set("listen-port", listen.to_string());
    opts1.set("timeout", "15");
    let mut opts2 = job_opts();
    opts2.set("dir", dir.path().display().to_string());
    opts2.set("out", "leech.bin");
    opts2.set("seed-ratio", "0");
    opts2.set("connect-timeout", "8");
    opts2.set("timeout", "15");
    let torrent2 = torrent.clone();
    let (_tx1, rx1) = watch::channel(false);
    let job1 = BtJob {
        torrent,
        dest: dest1.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts: opts1,
        progress: HttpProgress::new(),
        cancel: rx1,
    };
    let seeder_task = tokio::spawn(async move { bt::download(job1).await });
    let t0 = tokio::time::Instant::now();
    loop {
        if dest1.exists() && std::fs::read(&dest1).ok().as_deref() == Some(body) {
            break;
        }
        if t0.elapsed() > Duration::from_secs(8) {
            panic!("seeder dest never matched");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    aria2_rust::sockopt::reset_writev();
    aria2_rust::sockopt::reset_recv();
    let mut last = None;
    for _ in 0..8 {
        let (_tx, rx) = watch::channel(false);
        let j = BtJob {
            torrent: torrent2.clone(),
            dest: dest2.clone(),
            peers: vec![format!("127.0.0.1:{listen}").parse().unwrap()],
            opts: opts2.clone(),
            progress: HttpProgress::new(),
            cancel: rx,
        };
        match bt::download(j).await {
            Ok(()) => {
                last = None;
                break;
            }
            Err(e) => {
                last = Some(e);
                tokio::time::sleep(Duration::from_millis(80)).await;
            }
        }
    }
    if let Some(e) = last {
        panic!("leecher failed: {e}");
    }
    seeder_task.await.unwrap().unwrap();
    assert_eq!(
        std::fs::read(&dest2).unwrap(),
        body,
        "leecher must dest-match bytes uploaded by --seed-ratio seeder"
    );
    assert!(
        aria2_rust::sockopt::last_writev() >= 1,
        "C++ BtPieceMessage writev must send PIECE without concat"
    );
    assert!(
        aria2_rust::sockopt::last_recv() >= 1,
        "C++ SocketCore::readData recv must read BT REQUEST"
    );
}

#[tokio::test]
async fn bt_seed_time_zero_disables() {
    let body = payload();
    let torrent = bt::build_single_file("st0.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("st0.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "st0.bin");
    opts.set("seed-ratio", "1.0");
    opts.set("seed-time", "0");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let t0 = tokio::time::Instant::now();
    bt::download(job).await.unwrap();
    assert!(
        t0.elapsed() < Duration::from_millis(800),
        "--seed-time=0 must not wait to seed"
    );
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn bt_seed_time_elapses_without_leecher() {
    let body = payload();
    let torrent = bt::build_single_file("st.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let listen = probe.local_addr().unwrap().port();
    drop(probe);
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("st.bin");
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "st.bin");
    opts.set("seed-ratio", "99");
    opts.set("seed-time", "0.02"); // ~1.2s
    opts.set("listen-port", listen.to_string());
    opts.set("timeout", "10");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let t0 = tokio::time::Instant::now();
    bt::download(job).await.unwrap();
    let elapsed = t0.elapsed();
    assert!(
        elapsed >= Duration::from_millis(900),
        "--seed-time minutes must keep seeding; got {elapsed:?}"
    );
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn on_bt_download_complete_hook_before_seed_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("btcomp.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let listen = probe.local_addr().unwrap().port();
    drop(probe);
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("btcomp.bin");
    let marker = dir.path().join("btcomp.txt");
    let hook = dir.path().join("on_bt.sh");
    std::fs::write(
        &hook,
        format!("#!/bin/sh\nprintf '%s\\n' \"$3\" > '{}'\n", marker.display()),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "btcomp.bin");
    opts.set("gid", "aabbccddeeff0011");
    opts.set("seed-ratio", "99");
    opts.set("listen-port", listen.to_string());
    opts.set("timeout", "15");
    opts.set("on-bt-download-complete", hook.to_string_lossy().into_owned());
    let (tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let seeder_task = tokio::spawn(async move { bt::download(job).await });
    let t0 = tokio::time::Instant::now();
    loop {
        if dest.exists() && std::fs::read(&dest).ok().as_deref() == Some(body) && marker.exists() {
            break;
        }
        if t0.elapsed() > Duration::from_secs(8) {
            panic!(
                "dest/hook never ready dest={} marker={}",
                dest.exists(),
                marker.exists()
            );
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    assert!(!seeder_task.is_finished(), "hook must fire before seed ends");
    let _ = tx.send(true);
    let _ = seeder_task.await;
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    let marked = std::fs::read_to_string(&marker).unwrap();
    assert!(marked.contains("btcomp.bin"), "{marked}");
}

#[tokio::test]
async fn follow_torrent_true_http_dest_match() {
    let body = payload();
    let seed_ih_torrent = bt::build_single_file(
        "followed.bin",
        PIECE_LEN,
        body,
        "http://127.0.0.1:1/announce",
    );
    let meta = MetaInfo::from_torrent(&seed_ih_torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let seen = Arc::new(TrackerSeen {
        port: AtomicU16::new(0),
    });
    let trk = spawn_tracker(seed, seen).await;
    let torrent = bt::build_single_file(
        "followed.bin",
        PIECE_LEN,
        body,
        &format!("http://127.0.0.1:{trk}/announce"),
    );
    let http_port = spawn_http_bytes(torrent.clone(), "followed.torrent").await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("follow-torrent", "true");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{http_port}/followed.torrent")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(
        std::fs::read(dir.path().join("followed.bin")).unwrap(),
        body,
        "follow-torrent=true must dest-match payload"
    );
    assert_eq!(
        std::fs::read(dir.path().join("followed.torrent")).unwrap(),
        torrent,
        "follow-torrent=true keeps the .torrent on disk"
    );
}

#[tokio::test]
async fn follow_torrent_false_keeps_torrent_only() {
    let body = payload();
    let torrent = bt::build_single_file("nofollow.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let http_port = spawn_http_bytes(torrent.clone(), "nofollow.torrent").await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("follow-torrent", "false");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{http_port}/nofollow.torrent")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(
        std::fs::read(dir.path().join("nofollow.torrent")).unwrap(),
        torrent,
        "follow-torrent=false dest-matches .torrent bytes"
    );
    assert!(
        !dir.path().join("nofollow.bin").exists(),
        "follow-torrent=false must not start BT payload"
    );
}

#[tokio::test]
async fn enable_bittorrent_false_http_torrent_keeps_torrent_only() {
    let body = payload();
    let torrent = bt::build_single_file("nobt.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let http_port = spawn_http_bytes(torrent.clone(), "nobt.torrent").await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("follow-torrent", "true");
    opts.set("enable-bittorrent", "false");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{http_port}/nobt.torrent")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(
        std::fs::read(dir.path().join("nobt.torrent")).unwrap(),
        torrent,
        "--enable-bittorrent=false dest-matches .torrent bytes"
    );
    assert!(
        !dir.path().join("nobt.bin").exists(),
        "--enable-bittorrent=false must not start BT payload"
    );
}

#[tokio::test]
async fn enable_bittorrent_false_add_torrent_rejected() {
    let body = payload();
    let torrent = bt::build_single_file("rej.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let dir = tempfile::tempdir().unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("enable-bittorrent", "false");
    let session = Session::new(opts).unwrap();
    let err = session
        .add_torrent_and_start(torrent, vec![], OptionSet::new())
        .await;
    assert!(
        err.is_err(),
        "--enable-bittorrent=false must reject addTorrent: {err:?}"
    );
    assert!(
        !dir.path().join("rej.bin").exists(),
        "--enable-bittorrent=false must not dest-match BT payload"
    );
}

#[tokio::test]
async fn enable_bittorrent_true_http_torrent_dest_match() {
    let body = payload();
    let seed_ih_torrent = bt::build_single_file(
        "enbt.bin",
        PIECE_LEN,
        body,
        "http://127.0.0.1:1/announce",
    );
    let meta = MetaInfo::from_torrent(&seed_ih_torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let seen = Arc::new(TrackerSeen {
        port: AtomicU16::new(0),
    });
    let trk = spawn_tracker(seed, seen).await;
    let torrent = bt::build_single_file(
        "enbt.bin",
        PIECE_LEN,
        body,
        &format!("http://127.0.0.1:{trk}/announce"),
    );
    let http_port = spawn_http_bytes(torrent.clone(), "enbt.torrent").await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("follow-torrent", "true");
    opts.set("enable-bittorrent", "true");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{http_port}/enbt.torrent")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(
        std::fs::read(dir.path().join("enbt.bin")).unwrap(),
        body,
        "--enable-bittorrent=true must dest-match payload"
    );
}

#[tokio::test]
async fn follow_torrent_mem_no_torrent_file() {
    let body = payload();
    let meta_tmp = bt::build_single_file("mem.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&meta_tmp).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let seen = Arc::new(TrackerSeen {
        port: AtomicU16::new(0),
    });
    let trk = spawn_tracker(seed, seen).await;
    let torrent = bt::build_single_file(
        "mem.bin",
        PIECE_LEN,
        body,
        &format!("http://127.0.0.1:{trk}/announce"),
    );
    let http_port = spawn_http_bytes(torrent, "mem.torrent").await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("follow-torrent", "mem");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("http://127.0.0.1:{http_port}/mem.torrent")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    wait_complete(&session, gid.as_str()).await;
    assert_eq!(std::fs::read(dir.path().join("mem.bin")).unwrap(), body);
    assert!(
        !dir.path().join("mem.torrent").exists(),
        "follow-torrent=mem must not leave the .torrent on disk"
    );
}

#[tokio::test]
async fn bt_check_integrity_complete_no_peers() {
    let body = payload();
    let torrent = bt::build_single_file("ok.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("ok.bin");
    std::fs::write(&dest, body).unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "ok.bin");
    opts.set("check-integrity", "true");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn bt_enable_hook_after_hash_check_true_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("hookok.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("hookok.bin");
    std::fs::write(&dest, body).unwrap();
    let marker = dir.path().join("hookok.txt");
    let hook = dir.path().join("on_hash.sh");
    std::fs::write(
        &hook,
        format!("#!/bin/sh\nprintf '%s\\n' \"$3\" > '{}'\n", marker.display()),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "hookok.bin");
    opts.set("check-integrity", "true");
    opts.set("bt-enable-hook-after-hash-check", "true");
    opts.set("on-bt-download-complete", hook.to_string_lossy().into_owned());
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    let marked = std::fs::read_to_string(&marker).unwrap();
    assert!(
        marked.contains("hookok.bin"),
        "--bt-enable-hook-after-hash-check=true must run hook, got {marked}"
    );
}

#[tokio::test]
async fn bt_enable_hook_after_hash_check_false_skips_hook_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("hookno.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("hookno.bin");
    std::fs::write(&dest, body).unwrap();
    let marker = dir.path().join("hookno.txt");
    let hook = dir.path().join("on_hash_no.sh");
    std::fs::write(
        &hook,
        format!("#!/bin/sh\nprintf '%s\\n' \"$3\" > '{}'\n", marker.display()),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "hookno.bin");
    opts.set("check-integrity", "true");
    opts.set("bt-enable-hook-after-hash-check", "false");
    opts.set("on-bt-download-complete", hook.to_string_lossy().into_owned());
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert!(
        !marker.exists(),
        "--bt-enable-hook-after-hash-check=false must not run hook after hash check"
    );
}

#[tokio::test]
async fn bt_check_integrity_corrupt_refetch_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("bad.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("bad.bin");
    let mut corrupt = body.to_vec();
    corrupt[0] ^= 0xff;
    std::fs::write(&dest, &corrupt).unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "bad.bin");
    opts.set("check-integrity", "true");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "check-integrity must refetch corrupt pieces until dest-match"
    );
}

#[tokio::test]
async fn bt_hash_check_only_incomplete_does_not_fetch() {
    let body = payload();
    let torrent = bt::build_single_file("hco.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let seed = spawn_seeder(meta.info_hash, body, false).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("hco.bin");
    let mut corrupt = body.to_vec();
    corrupt[10] ^= 0xff;
    std::fs::write(&dest, &corrupt).unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "hco.bin");
    opts.set("check-integrity", "true");
    opts.set("hash-check-only", "true");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![format!("127.0.0.1:{seed}").parse().unwrap()],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let err = bt::download(job).await;
    assert!(err.is_err(), "hash-check-only incomplete must abort");
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        corrupt,
        "hash-check-only must not refetch"
    );
}

#[tokio::test]
async fn bt_hash_check_seed_false_skips_seed() {
    let body = payload();
    let torrent = bt::build_single_file("nseed.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("nseed.bin");
    std::fs::write(&dest, body).unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "nseed.bin");
    opts.set("check-integrity", "true");
    opts.set("bt-hash-check-seed", "false");
    opts.set("seed-ratio", "1.0");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let t0 = tokio::time::Instant::now();
    bt::download(job).await.unwrap();
    assert!(
        t0.elapsed() < Duration::from_millis(800),
        "--bt-hash-check-seed=false must not seed after a clean hash check"
    );
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn bt_seed_unverified_keeps_corrupt_dest_without_peers() {
    let body = payload();
    let torrent = bt::build_single_file("uv.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("uv.bin");
    let mut corrupt = body.to_vec();
    corrupt[0] ^= 0xff;
    std::fs::write(&dest, &corrupt).unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "uv.bin");
    opts.set("bt-seed-unverified", "true");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        corrupt,
        "bt-seed-unverified must not hash-check or refetch"
    );
}

#[tokio::test]
async fn bt_seed_unverified_false_needs_peers() {
    let body = payload();
    let torrent = bt::build_single_file("uvf.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("uvf.bin");
    std::fs::write(&dest, body).unwrap();
    let mut opts = job_opts();
    opts.set("dir", dir.path().display().to_string());
    opts.set("out", "uvf.bin");
    opts.set("bt-seed-unverified", "false");
    let (_tx, rx) = watch::channel(false);
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let err = bt::download(job).await;
    assert!(
        err.is_err(),
        "--bt-seed-unverified=false must still require peers when dest already exists"
    );
}

#[tokio::test]
async fn bt_seed_unverified_uploads_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("uvs.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let listen = probe.local_addr().unwrap().port();
    drop(probe);
    let dir = tempfile::tempdir().unwrap();
    let dest1 = dir.path().join("uvs.bin");
    let dest2 = dir.path().join("uvl.bin");
    std::fs::write(&dest1, body).unwrap();
    let mut opts1 = job_opts();
    opts1.set("dir", dir.path().display().to_string());
    opts1.set("out", "uvs.bin");
    opts1.set("bt-seed-unverified", "true");
    opts1.set("seed-ratio", "1.0");
    opts1.set("listen-port", listen.to_string());
    opts1.set("timeout", "15");
    let mut opts2 = job_opts();
    opts2.set("dir", dir.path().display().to_string());
    opts2.set("out", "uvl.bin");
    opts2.set("seed-ratio", "0");
    opts2.set("connect-timeout", "8");
    let torrent2 = torrent.clone();
    let (_tx1, rx1) = watch::channel(false);
    let job1 = BtJob {
        torrent,
        dest: dest1.clone(),
        peers: vec![],
        opts: opts1,
        progress: HttpProgress::new(),
        cancel: rx1,
    };
    let seeder_task = tokio::spawn(async move { bt::download(job1).await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    let mut last = None;
    for _ in 0..8 {
        let (_tx, rx) = watch::channel(false);
        let j = BtJob {
            torrent: torrent2.clone(),
            dest: dest2.clone(),
            peers: vec![format!("127.0.0.1:{listen}").parse().unwrap()],
            opts: opts2.clone(),
            progress: HttpProgress::new(),
            cancel: rx,
        };
        match bt::download(j).await {
            Ok(()) => {
                last = None;
                break;
            }
            Err(e) => {
                last = Some(e);
                tokio::time::sleep(Duration::from_millis(80)).await;
            }
        }
    }
    if let Some(e) = last {
        panic!("leecher failed: {e}");
    }
    seeder_task.await.unwrap().unwrap();
    assert_eq!(
        std::fs::read(&dest2).unwrap(),
        body,
        "leecher must dest-match bytes uploaded by --bt-seed-unverified seeder"
    );
}

async fn spawn_reject_peer() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut hs = [0u8; 68];
                let _ = s.read_exact(&mut hs).await;
                let _ = s.write_all(&[0u8; 8]).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn bt_max_peers_one_stops_before_good_seeder() {
    let body = payload();
    let torrent = bt::build_single_file("max1.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let good = spawn_seeder(meta.info_hash, body, false).await;
    let bad = spawn_reject_peer().await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("max1.bin");
    let (_tx, rx) = watch::channel(false);
    let mut opts = job_opts();
    opts.set("bt-max-peers", "1");
    opts.set("bt-request-peer-speed-limit", "0");
    // queue.pop() tries the last addr first — reject then (skipped) seeder.
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![
            format!("127.0.0.1:{good}").parse().unwrap(),
            format!("127.0.0.1:{bad}").parse().unwrap(),
        ],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let r = bt::download(job).await;
    assert!(r.is_err(), "max=1 must not reach the seeder: {r:?}");
    assert_eq!(bt::last_peers_tried(), 1);
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "must not dest-match when capped to the reject peer"
    );
}

#[tokio::test]
async fn bt_max_peers_two_then_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("max2.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let good = spawn_seeder(meta.info_hash, body, false).await;
    let bad = spawn_reject_peer().await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("max2.bin");
    let (_tx, rx) = watch::channel(false);
    let mut opts = job_opts();
    opts.set("bt-max-peers", "2");
    opts.set("bt-request-peer-speed-limit", "0");
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![
            format!("127.0.0.1:{good}").parse().unwrap(),
            format!("127.0.0.1:{bad}").parse().unwrap(),
        ],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert_eq!(bt::last_peers_tried(), 2);
}

#[tokio::test]
async fn bt_max_peers_zero_unlimited_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("max0.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let good = spawn_seeder(meta.info_hash, body, false).await;
    let bad = spawn_reject_peer().await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("max0.bin");
    let (_tx, rx) = watch::channel(false);
    let mut opts = job_opts();
    opts.set("bt-max-peers", "0");
    opts.set("bt-request-peer-speed-limit", "0");
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![
            format!("127.0.0.1:{good}").parse().unwrap(),
            format!("127.0.0.1:{bad}").parse().unwrap(),
        ],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn bt_request_peer_speed_limit_bumps_max_peers_dest_match() {
    let body = payload();
    let torrent = bt::build_single_file("rpsl.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let good = spawn_seeder(meta.info_hash, body, false).await;
    let bad = spawn_reject_peer().await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("rpsl.bin");
    let (_tx, rx) = watch::channel(false);
    let mut opts = job_opts();
    opts.set("bt-max-peers", "1");
    opts.set("bt-request-peer-speed-limit", "50K");
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![
            format!("127.0.0.1:{good}").parse().unwrap(),
            format!("127.0.0.1:{bad}").parse().unwrap(),
        ],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    bt::download(job).await.unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        body,
        "--bt-request-peer-speed-limit=50K must bump past --bt-max-peers=1"
    );
    assert_eq!(bt::last_peers_tried(), 2);
    assert_eq!(
        bt::last_request_peer_speed_limit(),
        50 * 1024,
        "must see C++ default 50K, got {}",
        bt::last_request_peer_speed_limit()
    );
}

#[tokio::test]
async fn bt_request_peer_speed_limit_zero_keeps_max_peers_cap() {
    let body = payload();
    let torrent = bt::build_single_file("rps0.bin", PIECE_LEN, body, "http://127.0.0.1:1/announce");
    let meta = MetaInfo::from_torrent(&torrent).unwrap();
    let good = spawn_seeder(meta.info_hash, body, false).await;
    let bad = spawn_reject_peer().await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("rps0.bin");
    let (_tx, rx) = watch::channel(false);
    let mut opts = job_opts();
    opts.set("bt-max-peers", "1");
    opts.set("bt-request-peer-speed-limit", "0");
    let job = BtJob {
        torrent,
        dest: dest.clone(),
        peers: vec![
            format!("127.0.0.1:{good}").parse().unwrap(),
            format!("127.0.0.1:{bad}").parse().unwrap(),
        ],
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    };
    let r = bt::download(job).await;
    assert!(r.is_err(), "limit=0 must keep max-peers=1 cap: {r:?}");
    assert_eq!(bt::last_peers_tried(), 1);
    assert_eq!(bt::last_request_peer_speed_limit(), 0);
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() != body,
        "must not dest-match when speed-limit 0 keeps the cap"
    );
}
