// Origin: CTOX
// License: AGPL-3.0-only

//! One bounded SSH command using the same pure-Rust stack as storage.
//! This low-level transport grants no authority. Native callers must enter the
//! current endpoint/SecretStore fence and supply a generated, quoted command.
//! Long builds run detached on the host; this API only launches/probes them.

use anyhow::{bail, ensure, Context, Result};
use russh::{
    client,
    keys::{decode_secret_key, Algorithm, HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate},
    ChannelMsg,
};
use std::{sync::Arc, time::Duration};

pub const MAX_IO_BYTES: usize = 1024 * 1024;
pub const MAX_COMMAND_BYTES: usize = 64 * 1024;
const DEADLINE: Duration = Duration::from_secs(10);

pub struct SshExecOptions<'a> {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub host_key_sha256: String,
    pub host_key_algorithm: Option<String>,
    pub private_key: &'a str,
    pub passphrase: Option<&'a str>,
}

/// Exit status and exact bytes. No implicit success on missing exit status.
#[derive(Debug)]
pub struct SshExecOutput {
    pub exit_code: u32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

struct PinnedKey(String);
impl client::Handler for PinnedKey {
    type Error = anyhow::Error;
    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool> {
        Ok(match key {
            PublicKeyOrCertificate::PublicKey { key, .. } => {
                key.fingerprint(HashAlg::Sha256).to_string() == self.0
            }
            PublicKeyOrCertificate::Certificate(_) => false,
        })
    }
}

/// Connect, authenticate and execute exactly once; no retry, agent forwarding,
/// PTY, environment forwarding, persistent signer or vendor CLI subprocess.
/// Connection, stdin, output and teardown share one ten-second deadline.
pub fn exec(options: SshExecOptions<'_>, command: &str, input: &[u8]) -> Result<SshExecOutput> {
    ensure!(
        options.port != 0 && !options.host.is_empty() && !options.username.is_empty(),
        "invalid SSH endpoint"
    );
    ensure!(
        options.host_key_sha256.starts_with("SHA256:"),
        "SSH requires a SHA256 host-key pin"
    );
    ensure!(
        !command.is_empty() && command.len() <= MAX_COMMAND_BYTES && !command.contains('\0'),
        "invalid SSH command"
    );
    ensure!(input.len() <= MAX_IO_BYTES, "SSH input exceeds one MiB");
    // The scoped thread joins while the credentials are still borrowed. It also
    // keeps creation/destruction of a Tokio runtime outside the caller's runtime.
    std::thread::scope(|scope| {
        scope
            .spawn(move || -> Result<SshExecOutput> {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                let result = runtime.block_on(async {
                    tokio::time::timeout(DEADLINE, execute(options, command, input))
                        .await
                        .context("SSH execution timed out")?
                });
                runtime.shutdown_timeout(Duration::from_secs(1));
                result
            })
            .join()
    })
    .map_err(|_| anyhow::anyhow!("SSH execution thread failed"))?
}

async fn execute(
    options: SshExecOptions<'_>,
    command: &str,
    input: &[u8],
) -> Result<SshExecOutput> {
    let key = Arc::new(decode_secret_key(options.private_key, options.passphrase)?);
    let key_lifetime = Arc::downgrade(&key);
    let mut config = client::Config::default();
    if let Some(algorithm) = options.host_key_algorithm.as_deref() {
        config.preferred.key = std::borrow::Cow::Owned(vec![Algorithm::new(algorithm)?]);
    }
    let mut session = client::connect(
        Arc::new(config),
        (options.host.as_str(), options.port),
        PinnedKey(options.host_key_sha256),
    )
    .await?;
    let algorithm = session.best_supported_rsa_hash().await?.flatten();
    ensure!(
        session
            .authenticate_publickey(
                &options.username,
                PrivateKeyWithHashAlg::new(key, algorithm)
            )
            .await?
            .success(),
        "SSH execution authentication failed"
    );
    ensure!(
        key_lifetime.upgrade().is_none(),
        "SSH authentication retained signing credentials"
    );

    let mut channel = session.channel_open_session().await?;
    channel.exec(true, command.as_bytes()).await?;
    // Do not send input if the server rejects the exec request.
    loop {
        match channel.wait().await {
            Some(ChannelMsg::Success) => break,
            Some(ChannelMsg::WindowAdjusted { .. }) => (),
            Some(ChannelMsg::Failure) => bail!("SSH command rejected"),
            _ => bail!("SSH command did not acknowledge execution"),
        }
    }
    channel.data(input).await?;
    channel.eof().await?;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut status = None;
    while let Some(message) = channel.wait().await {
        match message {
            ChannelMsg::Data { data } => {
                ensure!(
                    stdout.len() + stderr.len() + data.len() <= MAX_IO_BYTES,
                    "SSH output exceeds one MiB"
                );
                stdout.extend_from_slice(&data);
            }
            ChannelMsg::ExtendedData { ext: 1, data } => {
                ensure!(
                    stdout.len() + stderr.len() + data.len() <= MAX_IO_BYTES,
                    "SSH output exceeds one MiB"
                );
                stderr.extend_from_slice(&data);
            }
            ChannelMsg::ExitStatus { exit_status } => {
                ensure!(
                    status.replace(exit_status).is_none(),
                    "duplicate SSH exit status"
                );
            }
            ChannelMsg::ExitSignal { .. } => bail!("SSH command terminated by a signal"),
            ChannelMsg::Close => break,
            ChannelMsg::Eof | ChannelMsg::WindowAdjusted { .. } => (),
            ChannelMsg::Failure => bail!("SSH command failed"),
            _ => bail!("unexpected SSH execution message"),
        }
    }
    let exit_code = status.context("SSH command closed without an exit status")?;
    // A verified remote exit is retained even when the peer already closed its
    // session. The runtime shutdown disposes of all connection tasks.
    let _ = session
        .disconnect(
            russh::Disconnect::ByApplication,
            "bounded command finished",
            "en",
        )
        .await;
    Ok(SshExecOutput {
        exit_code,
        stdout,
        stderr,
    })
}
