// Real loopback SSH sessions; no NAS credentials, containers or remote retry.
use ctox_transfers::ssh_exec::{exec, SshExecOptions};
use russh::{
    keys::{decode_secret_key, HashAlg, PublicKey},
    server, Channel, ChannelId,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{net::TcpListener, task::JoinHandle};

const COMMAND: &str = "printf '%s' 'literal ; $(nothing) \\ quoted'";

#[derive(Clone, Copy)]
enum Case {
    Echo,
    Overflow,
    MissingStatus,
    Hang,
}

struct Handler {
    case: Case,
    public_key: PublicKey,
    authenticated: Arc<AtomicBool>,
    input: Vec<u8>,
}
impl server::Handler for Handler {
    type Error = anyhow::Error;
    async fn auth_publickey(
        &mut self,
        user: &str,
        key: &PublicKey,
    ) -> anyhow::Result<server::Auth> {
        if user == "fixture" && key.key_data() == self.public_key.key_data() {
            self.authenticated.store(true, Ordering::Release);
            Ok(server::Auth::Accept)
        } else {
            Ok(server::Auth::reject())
        }
    }
    async fn channel_open_session(
        &mut self,
        _: Channel<server::Msg>,
        reply: server::ChannelOpenHandle,
        _session: &mut server::Session,
    ) -> anyhow::Result<()> {
        reply.accept().await;
        Ok(())
    }
    async fn exec_request(
        &mut self,
        channel: ChannelId,
        bytes: &[u8],
        session: &mut server::Session,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            bytes == COMMAND.as_bytes(),
            "command bytes changed in transport"
        );
        session.channel_success(channel)?;
        Ok(())
    }
    async fn data(
        &mut self,
        _: ChannelId,
        bytes: &[u8],
        _: &mut server::Session,
    ) -> anyhow::Result<()> {
        self.input.extend_from_slice(bytes);
        Ok(())
    }
    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        session: &mut server::Session,
    ) -> anyhow::Result<()> {
        match self.case {
            Case::Echo => {
                session.data(channel, self.input.clone())?;
                session.extended_data(channel, 1, b"stderr\0exact".to_vec())?;
                session.exit_status_request(channel, 17)?;
                session.eof(channel)?;
                session.close(channel)?;
            }
            Case::Overflow => {
                for _ in 0..33 {
                    session.data(channel, vec![b'x'; 32 * 1024])?;
                }
                session.exit_status_request(channel, 0)?;
                session.close(channel)?;
            }
            Case::MissingStatus => {
                session.close(channel)?;
            }
            Case::Hang => (),
        }
        Ok(())
    }
}
struct Fixture {
    port: u16,
    private_key: String,
    pin: String,
    authenticated: Arc<AtomicBool>,
    task: Option<JoinHandle<()>>,
}
impl Fixture {
    async fn start(case: Case) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture-key");
        let status = std::process::Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        let private_key = std::fs::read_to_string(&path).unwrap();
        let key = decode_secret_key(&private_key, None).unwrap();
        let public_key = key.public_key().clone();
        let pin = public_key.fingerprint(HashAlg::Sha256).to_string();
        let authenticated = Arc::new(AtomicBool::new(false));
        let handler = Handler {
            case,
            public_key,
            authenticated: authenticated.clone(),
            input: vec![],
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut config = server::Config::default();
        config.keys.push(key);
        config.auth_rejection_time = Duration::from_millis(5);
        config.auth_rejection_time_initial = Some(Duration::ZERO);
        config.inactivity_timeout = Some(Duration::from_secs(12));
        let task = tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                if let Ok(session) = server::run_stream(Arc::new(config), stream, handler).await {
                    let _ = session.await;
                }
            }
        });
        Self {
            port,
            private_key,
            pin,
            authenticated,
            task: Some(task),
        }
    }
    fn options(&self) -> SshExecOptions<'_> {
        SshExecOptions {
            host: "127.0.0.1".into(),
            port: self.port,
            username: "fixture".into(),
            host_key_sha256: self.pin.clone(),
            host_key_algorithm: Some("ssh-ed25519".into()),
            private_key: &self.private_key,
            passphrase: None,
        }
    }
    async fn stop(&mut self) {
        if let Some(mut task) = self.task.take() {
            if tokio::time::timeout(Duration::from_secs(2), &mut task)
                .await
                .is_err()
            {
                task.abort();
                let _ = task.await;
            }
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ssh_exec_preserves_binary_input_output_and_nonzero_status() {
    let mut f = Fixture::start(Case::Echo).await;
    let body = b"source bytes\0quotes '\"\n";
    let result = exec(f.options(), COMMAND, body).unwrap();
    assert!(f.authenticated.load(Ordering::Acquire));
    assert_eq!(result.exit_code, 17);
    assert_eq!(result.stdout, body);
    assert_eq!(result.stderr, b"stderr\0exact");
    f.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ssh_exec_wrong_pin_stops_before_authentication() {
    let mut f = Fixture::start(Case::Echo).await;
    let mut options = f.options();
    options.host_key_sha256 = "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into();
    assert!(exec(options, COMMAND, b"").is_err());
    assert!(!f.authenticated.load(Ordering::Acquire));
    f.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ssh_exec_rejects_oversized_output_and_missing_status() {
    for case in [Case::Overflow, Case::MissingStatus] {
        let mut f = Fixture::start(case).await;
        let error = exec(f.options(), COMMAND, b"").unwrap_err().to_string();
        match case {
            Case::Overflow => assert!(error.contains("output exceeds one MiB"), "{error}"),
            Case::MissingStatus => assert!(error.contains("without an exit status"), "{error}"),
            _ => unreachable!(),
        }
        assert!(f.authenticated.load(Ordering::Acquire));
        f.stop().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ssh_exec_unresponsive_command_has_a_total_deadline() {
    let mut f = Fixture::start(Case::Hang).await;
    let start = Instant::now();
    let error = exec(f.options(), COMMAND, b"").unwrap_err().to_string();
    assert!(error.contains("timed out"), "{error}");
    assert!(start.elapsed() < Duration::from_secs(12));
    assert!(f.authenticated.load(Ordering::Acquire));
    f.stop().await;
}
