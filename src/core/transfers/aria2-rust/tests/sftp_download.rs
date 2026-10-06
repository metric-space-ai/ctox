//! SFTP dest-match + `--ssh-host-key-md` (sha-1 of host-key blob).
#![forbid(unsafe_code)]

use aria2_rust::http::HttpProgress;
use aria2_rust::options::OptionSet;
use aria2_rust::session::Session;
use aria2_rust::sftp::{self, SftpJob};
use russh::keys::PrivateKey;
use russh::server::{Auth, Msg, Session as SshServerSession};
use russh::{Channel, ChannelId};
use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, Handle, Name, Status, StatusCode,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{watch, Mutex};

struct Seen {
    reads: AtomicU64,
}

struct SshSession {
    user: String,
    pass: String,
    body: &'static [u8],
    clients: Arc<Mutex<HashMap<ChannelId, Channel<Msg>>>>,
    seen: Arc<Seen>,
}

impl russh::server::Handler for SshSession {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        if user == self.user && password == self.pass {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::reject())
        }
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: russh::server::ChannelOpenHandle,
        _session: &mut SshServerSession,
    ) -> Result<(), Self::Error> {
        self.clients.lock().await.insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        session: &mut SshServerSession,
    ) -> Result<(), Self::Error> {
        session.close(channel)?;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel_id: ChannelId,
        name: &str,
        session: &mut SshServerSession,
    ) -> Result<(), Self::Error> {
        if name == "sftp" {
            let channel = self.clients.lock().await.remove(&channel_id).unwrap();
            let sftp = MemSftp {
                body: self.body,
                seen: Arc::clone(&self.seen),
            };
            session.channel_success(channel_id)?;
            russh_sftp::server::run(channel.into_stream(), sftp).await;
        } else {
            session.channel_failure(channel_id)?;
        }
        Ok(())
    }
}

struct MemSftp {
    body: &'static [u8],
    seen: Arc<Seen>,
}

impl russh_sftp::server::Handler for MemSftp {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn open(
        &mut self,
        id: u32,
        _filename: String,
        _pflags: russh_sftp::protocol::OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        Ok(Handle {
            id,
            handle: "f1".into(),
        })
    }

    async fn close(&mut self, id: u32, _handle: String) -> Result<Status, Self::Error> {
        Ok(Status {
            id,
            status_code: StatusCode::Ok,
            error_message: "Ok".into(),
            language_tag: "en-US".into(),
        })
    }

    async fn read(
        &mut self,
        id: u32,
        _handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, Self::Error> {
        self.seen.reads.fetch_add(1, Ordering::SeqCst);
        if offset >= self.body.len() as u64 {
            return Err(StatusCode::Eof);
        }
        let start = offset as usize;
        let end = (start + len as usize).min(self.body.len());
        Ok(Data {
            id,
            data: self.body[start..end].to_vec(),
        })
    }

    async fn stat(&mut self, id: u32, _path: String) -> Result<Attrs, Self::Error> {
        Ok(Attrs {
            id,
            attrs: FileAttributes {
                size: Some(self.body.len() as u64),
                ..Default::default()
            },
        })
    }

    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        self.stat(id, path).await
    }

    async fn fstat(&mut self, id: u32, _handle: String) -> Result<Attrs, Self::Error> {
        self.stat(id, String::new()).await
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        Ok(Name {
            id,
            files: vec![File::dummy(path)],
        })
    }
}

const HOST_KEY: &str = "\
-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACCwvKlAyksA+jE64YxD6yiU3vu2Jp6UJAPch1AeUad+9QAAAJgNBeqJDQXq
iQAAAAtzc2gtZWQyNTUxOQAAACCwvKlAyksA+jE64YxD6yiU3vu2Jp6UJAPch1AeUad+9Q
AAAEDjR2lZbwufgVpYP/g5B3WLrqpWKPCuhDg8TOVdhlNj3LC8qUDKSwD6MTrhjEPrKJTe
+7YmnpQkA9yHUB5Rp371AAAAFXJvb3RAaGRzLXRwbjhlenQ2bGg1cQ==
-----END OPENSSH PRIVATE KEY-----
";

async fn spawn_sftp(
    body: &'static [u8],
    user: &str,
    pass: &str,
) -> (u16, Vec<u8>, Arc<Seen>) {
    let key = PrivateKey::from_openssh(HOST_KEY).unwrap();
    let blob = sftp::host_key_blob(key.public_key()).unwrap();
    let seen = Arc::new(Seen {
        reads: AtomicU64::new(0),
    });
    let config = russh::server::Config {
        auth_rejection_time: Duration::from_millis(1),
        auth_rejection_time_initial: Some(Duration::ZERO),
        keys: vec![key],
        ..Default::default()
    };
    let config = Arc::new(config);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let user = user.to_string();
    let pass = pass.to_string();
    let seen_c = Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let Ok((sock, _)) = listener.accept().await else { break };
            let handler = SshSession {
                user: user.clone(),
                pass: pass.clone(),
                body,
                clients: Arc::new(Mutex::new(HashMap::new())),
                seen: Arc::clone(&seen_c),
            };
            let cfg = Arc::clone(&config);
            tokio::spawn(async move {
                let _ = russh::server::run_stream(cfg, sock, handler).await;
            });
        }
    });
    (port, blob, seen)
}

fn job(uri: String, dest: std::path::PathBuf, opts: OptionSet) -> SftpJob {
    let (_tx, rx) = watch::channel(false);
    SftpJob {
        uris: vec![uri],
        dest,
        opts,
        progress: HttpProgress::new(),
        cancel: rx,
    }
}

#[tokio::test]
async fn sftp_dest_match_and_host_key() {
    let body: &'static [u8] = Box::leak(vec![0xABu8; 32 * 1024].into_boxed_slice());
    let (port, blob, seen) = spawn_sftp(body, "alice", "secret").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("blob.bin");
    let digest = sftp::hash_ssh_host_key("sha-1", &blob).unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("ftp-user", "alice");
    opts.set("ftp-passwd", "secret");
    opts.set("ssh-host-key-md", format!("sha-1={}", hex::encode(&digest)));
    opts.set("file-allocation", "none");
    opts.set("disk-cache", "0");
    aria2_rust::storage::reset_try_pwrite();
    sftp::reset_sftp_seek();
    sftp::download(job(
        format!("sftp://127.0.0.1:{port}/blob.bin"),
        dest.clone(),
        opts,
    ))
    .await
    .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert!(
        aria2_rust::storage::last_try_pwrite() > 0,
        "C++ DefaultDiskWriter: SFTP body must pwrite without per-chunk await"
    );
    assert!(seen.reads.load(Ordering::SeqCst) > 1, "must stream multiple SSH_FXP_READ");
    assert_eq!(
        sftp::last_sftp_seek(),
        0,
        "C++ SftpDownloadCommand: full GET must sequential READ, no per-chunk SEEK, got {}",
        sftp::last_sftp_seek()
    );
}

#[tokio::test]
async fn sftp_wrong_host_key_rejected() {
    let body: &'static [u8] = b"nope";
    let (port, _, _) = spawn_sftp(body, "alice", "secret").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("x.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("ftp-user", "alice");
    opts.set("ftp-passwd", "secret");
    opts.set("ssh-host-key-md", "sha-1=0000000000000000000000000000000000000000");
    opts.set("file-allocation", "none");
    let err = sftp::download(job(
        format!("sftp://127.0.0.1:{port}/x.bin"),
        dest,
        opts,
    ))
    .await;
    assert!(err.is_err(), "wrong ssh-host-key-md must fail");
}

#[tokio::test]
async fn sftp_wrong_password_fails() {
    let body: &'static [u8] = b"nope";
    let (port, _, _) = spawn_sftp(body, "alice", "secret").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("x.bin");
    let mut opts = OptionSet::with_defaults();
    opts.set("ftp-user", "alice");
    opts.set("ftp-passwd", "wrong");
    opts.set("file-allocation", "none");
    let err = sftp::download(job(
        format!("sftp://127.0.0.1:{port}/x.bin"),
        dest,
        opts,
    ))
    .await;
    assert!(err.is_err(), "wrong ftp-passwd must fail sftp auth");
}

#[tokio::test]
async fn sftp_resume_dest_match() {
    let body: &'static [u8] = Box::leak((0u8..=255).cycle().take(8192).collect::<Vec<_>>().into_boxed_slice());
    let (port, _, _) = spawn_sftp(body, "anonymous", "ARIA2USER@").await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("c.bin");
    std::fs::write(&dest, &body[..4096]).unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("continue", "true");
    opts.set("file-allocation", "none");
    sftp::reset_sftp_seek();
    sftp::download(job(
        format!("sftp://127.0.0.1:{port}/c.bin"),
        dest.clone(),
        opts,
    ))
    .await
    .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert_eq!(
        sftp::last_sftp_seek(),
        1,
        "C++ SftpDownloadCommand: resume SEEK once then sequential READ, got {}",
        sftp::last_sftp_seek()
    );
}

#[tokio::test]
async fn sftp_session_add_uri_dest_match() {
    let body: &'static [u8] = Box::leak(vec![0x44u8; 4096].into_boxed_slice());
    let (port, _, _) = spawn_sftp(body, "sess", "ion").await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = OptionSet::with_defaults();
    opts.set("dir", dir.path().display().to_string());
    opts.set("ftp-user", "sess");
    opts.set("ftp-passwd", "ion");
    opts.set("file-allocation", "none");
    let session = Session::new(opts).unwrap();
    let gid = session
        .add_uri_and_start(
            vec![format!("sftp://127.0.0.1:{port}/sess.bin")],
            OptionSet::new(),
        )
        .await
        .unwrap();
    let mut ok = false;
    for _ in 0..400 {
        tokio::time::sleep(Duration::from_millis(15)).await;
        let st = session.tell_status(gid.as_str()).await.unwrap();
        match st.get("status").and_then(|v| v.as_str()) {
            Some("complete") => {
                ok = true;
                break;
            }
            Some("error") => panic!("session sftp error: {st}"),
            _ => {}
        }
    }
    assert!(ok, "session sftp download did not complete");
    assert_eq!(std::fs::read(dir.path().join("sess.bin")).unwrap(), body);
}
