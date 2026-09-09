//! Connection-owned credential correlation for the existing private host IPC.
//! No listener or detached task; the owning IpcService drives outgoing/accept_reply.
use crate::business_data_contract::{
    NativeBusinessDataCredentialChallenge as Challenge, NativeBusinessDataCredentialReply as Reply,
};
use std::{
    collections::BTreeMap,
    io,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};

const MAX_PENDING: usize = 2;
const DEADLINE: Duration = Duration::from_secs(20);
struct Pending {
    connection_id: String,
    epoch: u64,
    challenged: bool,
    reply: oneshot::Sender<Reply>,
}
struct Shared {
    closed: AtomicBool,
    sequence: AtomicU64,
    pending: Mutex<BTreeMap<String, Pending>>,
}
impl Shared {
    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.pending
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
    }
}
/// Kept by the platform connection future. Dropping it fences every provider.
pub struct CredentialChannel {
    shared: Arc<Shared>,
    outgoing: mpsc::Receiver<Challenge>,
}
#[derive(Clone)]
pub struct CredentialRequester {
    shared: Arc<Shared>,
    outgoing: mpsc::Sender<Challenge>,
}
struct PendingGuard {
    shared: Arc<Shared>,
    id: String,
}
impl Drop for PendingGuard {
    fn drop(&mut self) {
        self.shared
            .pending
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.id);
    }
}
fn unavailable() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "native credentials unavailable",
    )
}
fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
fn base64url(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}
pub fn credential_channel() -> (CredentialChannel, CredentialRequester) {
    let shared = Arc::new(Shared {
        closed: AtomicBool::new(false),
        sequence: AtomicU64::new(0),
        pending: Mutex::new(BTreeMap::new()),
    });
    let (outgoing, receiver) = mpsc::channel(MAX_PENDING);
    (
        CredentialChannel {
            shared: shared.clone(),
            outgoing: receiver,
        },
        CredentialRequester { shared, outgoing },
    )
}
impl CredentialChannel {
    /// The transport wraps this in the generated credentialChallenge HostFrame.
    pub async fn next_challenge(&mut self) -> Option<Challenge> {
        while let Some(challenge) = self.outgoing.recv().await {
            if self.shared.closed.load(Ordering::SeqCst) {
                return None;
            }
            if self
                .shared
                .pending
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .contains_key(&challenge.request_id)
            {
                return Some(challenge);
            }
        }
        None
    }
    /// Accept only a matching current response; malformed or unsolicited frames
    /// terminate this credential channel rather than becoming an anonymous login.
    pub fn accept_reply(&self, reply: Reply) -> io::Result<()> {
        let mut pending = self
            .shared
            .pending
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let valid = !self.shared.closed.load(Ordering::SeqCst)
            && reply.version == 1
            && pending.get(&reply.request_id).is_some_and(|request| {
                request.connection_id == reply.connection_id
                    && request.epoch == reply.session_epoch
                    && reply
                        .capability_token
                        .as_ref()
                        .is_some_and(|token| !token.trim().is_empty() && token.len() <= 32768)
                    && match (&reply.device_proof, request.challenged) {
                        (None, false) => true,
                        (Some(proof), true) => {
                            base64url(&proof.public_x, 43)
                                && base64url(&proof.public_y, 43)
                                && base64url(&proof.signature, 86)
                        }
                        _ => false,
                    }
            });
        if !valid {
            drop(pending);
            self.shared.close();
            return Err(unavailable());
        }
        let request = pending.remove(&reply.request_id).ok_or_else(unavailable)?;
        request.reply.send(reply).map_err(|_| unavailable())
    }
}
impl Drop for CredentialChannel {
    fn drop(&mut self) {
        self.shared.close();
    }
}
impl CredentialRequester {
    /// Called by NativeSessionTarget.credentials after native channel proof.
    /// Account/target validity must also be rechecked by the Main callback.
    pub async fn request(
        &self,
        target_id: &str,
        connection_id: &str,
        epoch: u64,
        nonce: Option<String>,
    ) -> io::Result<Reply> {
        if !id(target_id)
            || !id(connection_id)
            || epoch > 9_007_199_254_740_991
            || nonce.as_ref().is_some_and(|value| !base64url(value, 43))
        {
            return Err(unavailable());
        }
        let sequence = self
            .shared
            .sequence
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                value.checked_add(1)
            })
            .map_err(|_| unavailable())?;
        let request_id = format!("credentials-{sequence}");
        let (send, receive) = oneshot::channel();
        {
            let mut pending = self
                .shared
                .pending
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if self.shared.closed.load(Ordering::SeqCst) || pending.len() >= MAX_PENDING {
                return Err(unavailable());
            }
            pending.insert(
                request_id.clone(),
                Pending {
                    connection_id: connection_id.into(),
                    epoch,
                    challenged: nonce.is_some(),
                    reply: send,
                },
            );
        }
        let _guard = PendingGuard {
            shared: self.shared.clone(),
            id: request_id.clone(),
        };
        self.outgoing
            .try_send(Challenge {
                version: 1,
                request_id,
                connection_id: connection_id.into(),
                target_id: target_id.into(),
                session_epoch: epoch,
                nonce,
            })
            .map_err(|_| unavailable())?;
        let reply = tokio::time::timeout(DEADLINE, receive)
            .await
            .map_err(|_| unavailable())?
            .map_err(|_| unavailable())?;
        if self.shared.closed.load(Ordering::SeqCst) {
            return Err(unavailable());
        }
        Ok(reply)
    }
}

fn frame_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid private BusinessData frame",
    )
}
struct FrameBuffer(Vec<u8>);
impl io::Write for FrameBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > crate::ipc::IPC_MAX_FRAME_BYTES.saturating_sub(self.0.len()) {
            return Err(frame_error());
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
/// Same bounded length prefix as authority IPC. Larger data pages require
/// explicit chunking; this helper never increases authority's frame limit.
pub async fn write_host_frame<W: tokio::io::AsyncWrite + Unpin>(
    stream: &mut W,
    frame: &crate::business_data_contract::NativeBusinessDataHostFrame,
) -> io::Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut bytes = FrameBuffer(Vec::new());
    serde_json::to_writer(&mut bytes, frame).map_err(|_| frame_error())?;
    tokio::time::timeout(DEADLINE, async {
        stream
            .write_all(&(bytes.0.len() as u32).to_be_bytes())
            .await?;
        stream.write_all(&bytes.0).await?;
        stream.flush().await
    })
    .await
    .map_err(|_| frame_error())?
}
/// Parse errors are deliberately fixed text: serde errors may quote credentials.
pub async fn read_host_frame<R: tokio::io::AsyncRead + Unpin>(
    stream: &mut R,
) -> io::Result<crate::business_data_contract::NativeBusinessDataHostFrame> {
    use tokio::io::AsyncReadExt;
    tokio::time::timeout(DEADLINE, async {
        let size = stream.read_u32().await? as usize;
        if size == 0 || size > crate::ipc::IPC_MAX_FRAME_BYTES {
            return Err(frame_error());
        }
        let mut bytes = vec![0; size];
        stream.read_exact(&mut bytes).await?;
        serde_json::from_slice(&bytes).map_err(|_| frame_error())
    })
    .await
    .map_err(|_| frame_error())?
}

#[cfg(test)]
#[path = "credential_ipc_tests.rs"]
mod tests;
