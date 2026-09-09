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
pub type BusinessDataDispatchFuture = Pin<Box<dyn Future<Output = io::Result<Response>> + Send>>;
pub trait BusinessDataDispatcher: Send + Sync {
    /// Must own only this private connection's handles and authorization state.
    /// Dropping its pending future cancels the operation; no detached work.
    fn dispatch(&self, request: Request) -> BusinessDataDispatchFuture;
}
pub type BusinessDataConnectionFactory = Arc<
    dyn Fn(CredentialRequester, mpsc::Sender<Event>) -> io::Result<Arc<dyn BusinessDataDispatcher>>
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
                            if work.len() >= MAX_IN_FLIGHT || !ids.insert(request.request_id.clone()) {
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
                    event = event_receiver.recv(), if has_events => {
                        if let Some(event) = event {
                            write_host_frame(&mut writer, &Frame::Event { event }).await?;
                        } else { has_events = false; }
                    },
                    result = work.next(), if !work.is_empty() => {
                        let response = result.ok_or_else(invalid)??;
                        ids.remove(&response.request_id);
                        write_host_frame(&mut writer, &Frame::Response { response }).await?;
                    },
                }
            }
        };
        // Error/EOF/cancellation drops both futures, the dispatcher and the
        // credential owner. No connection-owned task outlives this service.
        tokio::try_join!(read, dispatch)?;
        Ok(())
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
