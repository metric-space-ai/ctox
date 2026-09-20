//! BusinessData on the existing authenticated local IPC lifecycle.
//! The host supplies the real per-connection dispatcher; this layer grants no data access.
use crate::{
    business_data_contract::{
        NativeBusinessDataEvent as Event, NativeBusinessDataHostFrame as Frame,
        NativeBusinessDataRequest as Request, NativeBusinessDataResponse as Response,
    },
    credential_ipc::{credential_channel, read_host_frame, write_host_frame, CredentialRequester},
    ipc::{IpcService, IpcServiceFuture, LocalIpcStream},
};
use futures_util::{stream::FuturesUnordered, StreamExt};
use std::{collections::HashSet, future::Future, io, pin::Pin, sync::Arc};
use tokio::sync::mpsc;

const MAX_IN_FLIGHT: usize = 4;
/// Serializes watch invalidation with the first accepted frame byte. A pending
/// write has not published anything and must remain cancellable. Once any byte
/// is accepted, finish that frame to preserve the connection's framing.
pub(crate) struct WatchLifetime {
    state: std::sync::Mutex<WatchLifetimeState>,
}

struct WatchLifetimeState {
    alive: bool,
    pending_writer: Option<std::task::Waker>,
}

impl WatchLifetime {
    pub(crate) fn new() -> Self {
        Self {
            state: std::sync::Mutex::new(WatchLifetimeState {
                alive: true,
                pending_writer: None,
            }),
        }
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.state.lock().is_ok_and(|state| state.alive)
    }

    pub(crate) fn invalidate(&self) {
        let wake = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.alive = false;
            state.pending_writer.take()
        };
        if let Some(wake) = wake {
            wake.wake();
        }
    }
}

struct WatchFrameWriter<'a, W> {
    writer: &'a mut W,
    lifetime: &'a WatchLifetime,
    started: bool,
    cancelled: bool,
}

impl<W> Drop for WatchFrameWriter<'_, W> {
    fn drop(&mut self) {
        // A timeout/connection error must not leave a task waker retained by
        // the watch. Only one frame writer can own this connection at a time.
        self.lifetime
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .pending_writer = None;
    }
}

impl<W: tokio::io::AsyncWrite + Unpin> tokio::io::AsyncWrite for WatchFrameWriter<'_, W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bytes: &[u8],
    ) -> std::task::Poll<io::Result<usize>> {
        let this = self.get_mut();
        if this.started {
            return Pin::new(&mut *this.writer).poll_write(cx, bytes);
        }
        let mut state = match this.lifetime.state.lock() {
            Ok(state) => state,
            Err(_) => {
                this.cancelled = true;
                return std::task::Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "watch publication state unavailable",
                )));
            }
        };
        if !state.alive {
            this.cancelled = true;
            return std::task::Poll::Ready(Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "watch invalidated before frame publication",
            )));
        }
        // Hold the same lock used by invalidate through the nonblocking write
        // poll. Merely checking an AtomicBool before write_all leaves a race.
        let result = Pin::new(&mut *this.writer).poll_write(cx, bytes);
        if result.is_pending() {
            state.pending_writer = Some(cx.waker().clone());
        } else {
            state.pending_writer = None;
            if matches!(&result, std::task::Poll::Ready(Ok(count)) if *count > 0) {
                this.started = true;
            }
        }
        result
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        Pin::new(&mut *self.get_mut().writer).poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        Pin::new(&mut *self.get_mut().writer).poll_shutdown(cx)
    }
}

/// Private delivery metadata; never serialized onto the wire.
pub struct QueuedBusinessDataEvent {
    pub(crate) event: Event,
    pub(crate) alive: Arc<WatchLifetime>,
    pub(crate) failed: Arc<std::sync::atomic::AtomicBool>,
    pub(crate) authority: crate::business_data_remote::EventAuthorityCheck,
}

impl QueuedBusinessDataEvent {
    fn is_alive(&self) -> bool {
        self.alive.is_alive()
    }

    async fn revalidate(mut self) -> Option<Self> {
        use std::sync::atomic::Ordering::SeqCst;
        if !self.is_alive() || self.failed.load(SeqCst) {
            return None;
        }
        let authorized = (self.authority)().await;
        if !self.is_alive() || self.failed.load(SeqCst) {
            return None;
        }
        if !authorized {
            if self.failed.swap(true, SeqCst) {
                return None;
            }
            // Never hide a missing snapshot page behind later completion.
            self.event.payload =
                crate::business_data_contract::NativeBusinessDataEventPayload::Reset {
                    code: crate::business_data_contract::NativeBusinessDataErrorCode::ResetRequired,
                };
        }
        Some(self)
    }
}
pub type BusinessDataDispatchFuture = Pin<Box<dyn Future<Output = io::Result<Response>> + Send>>;
pub trait BusinessDataDispatcher: Send + Sync {
    /// Must own only this private connection's handles and authorization state.
    /// Dropping its pending future cancels the operation; no detached work.
    fn dispatch(&self, request: Request) -> BusinessDataDispatchFuture;
    /// Release response-dependent events only after the response was written.
    fn response_sent<'a>(&'a self, _response: &'a Response) -> BusinessDataShutdownFuture<'a> {
        Box::pin(async { Ok(()) })
    }
    /// Await connection-owned cleanup after request cancellation and before the
    /// IPC service future completes. The default has no owned resources.
    fn shutdown(&self) -> BusinessDataShutdownFuture<'_> {
        Box::pin(async { Ok(()) })
    }
}
pub type BusinessDataShutdownFuture<'a> = Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'a>>;
pub type BusinessDataConnectionFactory = Arc<
    dyn Fn(
            CredentialRequester,
            mpsc::Sender<QueuedBusinessDataEvent>,
        ) -> io::Result<Arc<dyn BusinessDataDispatcher>>
        + Send
        + Sync,
>;

pub struct BusinessDataIpc {
    factory: BusinessDataConnectionFactory,
}
fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid private BusinessData exchange",
    )
}
impl BusinessDataIpc {
    pub fn new(factory: BusinessDataConnectionFactory) -> Self {
        Self { factory }
    }

    pub async fn serve(&self, stream: Box<dyn LocalIpcStream>) -> io::Result<()> {
        let (mut reader, mut writer) = tokio::io::split(stream);
        let (mut credentials, requester) = credential_channel();
        let (events, mut event_receiver) = mpsc::channel(8);
        let dispatcher = (self.factory)(requester, events)?;
        let connection_dispatcher = dispatcher.clone();
        let (incoming, mut received) = mpsc::channel(8);
        // Separate futures, not spawned tasks. A partial read is never cancelled
        // merely because a response/event wins a select branch.
        let read = async {
            loop {
                let frame = read_host_frame(&mut reader).await?;
                if incoming.send(frame).await.is_err() {
                    return Ok::<(), io::Error>(());
                }
            }
        };
        let dispatch = async {
            let mut work = FuturesUnordered::new();
            let mut releases = FuturesUnordered::new();
            let mut event_checks = FuturesUnordered::new();
            let mut ids = HashSet::new();
            let mut has_events = true;
            let mut has_credentials = true;
            loop {
                tokio::select! {
                    frame = received.recv() => {
                      let Some(frame) = frame else { return Ok::<(), io::Error>(()); };
                      match frame {
                        Frame::CredentialReply { reply } => credentials.accept_reply(reply)?,
                        Frame::Request { request } => {
                            // Reuse canonical semantic and resource validation.
                            let bytes = serde_json::to_vec(&request).map_err(|_| invalid())?;
                            let request = crate::business_data::decode_request(&bytes).map_err(|_| invalid())?;
                            if work.len() + releases.len() >= MAX_IN_FLIGHT || !ids.insert(request.request_id.clone()) {
                                return Err(invalid());
                            }
                            let expected = request.request_id.clone();
                            let operation = dispatcher.dispatch(request);
                            work.push(async move {
                                let response = operation.await?;
                                if response.version != 1 || response.request_id != expected { return Err(invalid()); }
                                Ok::<_, io::Error>(response)
                            });
                        },
                        _ => return Err(invalid()),
                      }
                    },
                    challenge = credentials.next_challenge(), if has_credentials => {
                        if let Some(challenge) = challenge {
                            write_host_frame(&mut writer, &Frame::CredentialChallenge { challenge }).await?;
                        } else { has_credentials = false; }
                    },
                    event = event_receiver.recv(), if has_events && event_checks.is_empty() => {
                        if let Some(event) = event {
                            // Poll authority alongside requests/credential replies,
                            // retaining FIFO with at most one pending event check.
                            event_checks.push(event.revalidate());
                        } else { has_events = false; }
                    },
                    checked = event_checks.next(), if !event_checks.is_empty() => {
                        if let Some(Some(queued)) = checked {
                            let mut guarded = WatchFrameWriter {
                                writer: &mut writer,
                                lifetime: &queued.alive,
                                started: false,
                                cancelled: false,
                            };
                            let result = write_host_frame(&mut guarded, &Frame::Event { event: queued.event }).await;
                            if !guarded.cancelled { result?; }
                        }
                    },
                    result = work.next(), if !work.is_empty() => {
                        let response = result.ok_or_else(invalid)??;
                        ids.remove(&response.request_id);
                        let frame = Frame::Response { response };
                        write_host_frame(&mut writer, &frame).await?;
                        if let Frame::Response { response } = frame {
                            let owner = dispatcher.clone();
                            // Keep host/authority awaits out of the writer path.
                            // Credential replies and other requests must progress
                            // while a release waits on a session/watch lock.
                            releases.push(async move { owner.response_sent(&response).await });
                        }
                    },
                    released = releases.next(), if !releases.is_empty() => {
                        released.ok_or_else(invalid)??;
                    },
                }
            }
        };
        let dispatcher = connection_dispatcher;
        // Error/EOF/cancellation drops both futures and the credential owner.
        // Connection-owned cleanup is awaited even when a future failed.
        let result = tokio::try_join!(read, dispatch);
        let cleanup = dispatcher.shutdown().await;
        result.and(cleanup)
    }
}
impl IpcService for BusinessDataIpc {
    fn serve_connection(&self, stream: Box<dyn LocalIpcStream>) -> IpcServiceFuture<'_> {
        Box::pin(self.serve(stream))
    }
}

#[cfg(test)]
#[path = "business_data_ipc_tests.rs"]
mod tests;
