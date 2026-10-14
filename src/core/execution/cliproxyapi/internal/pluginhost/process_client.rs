// Origin: CTOX
// License: AGPL-3.0-only

//! Explicitly injected, process-isolated unary plugin transport. Streaming
//! capability calls remain unsupported; no runtime discovers or starts plugins
//! merely by constructing this loader.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{value::to_raw_value, Value};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, oneshot};

use super::abi::{
    PluginArtifact, PluginCall, PluginClient, PluginClientError, PluginFuture, PluginLoader,
    PluginStream,
};
use super::host_callbacks::HostCallbackRouter;
use super::platform::validate_plugin_id;
use super::process_transport::{read_process_message, write_process_message};
use super::rpc_schema::{ProcessMessage, PROCESS_PROTOCOL_VERSION};
use super::supervisor::{PluginProcessConfig, PluginSupervisor, RestartPolicy};
use crate::sdk::pluginabi::{Envelope, Error};

const CALL_TIMEOUT: Duration = Duration::from_secs(30);
const QUEUED_CALLS: usize = 16;

pub struct ProcessPluginLoader {
    runtime_root: PathBuf,
    callbacks: Arc<HostCallbackRouter>,
}

impl ProcessPluginLoader {
    pub fn new(
        runtime_root: PathBuf,
        callbacks: Arc<HostCallbackRouter>,
    ) -> Result<Self, PluginClientError> {
        if !runtime_root.is_absolute() || !runtime_root.is_dir() {
            return Err(PluginClientError::InvalidRequest);
        }
        Ok(Self {
            runtime_root,
            callbacks,
        })
    }
}

impl PluginLoader for ProcessPluginLoader {
    fn open<'a>(&'a self, artifact: &'a PluginArtifact) -> PluginFuture<'a, Arc<dyn PluginClient>> {
        Box::pin(async move {
            if !validate_plugin_id(&artifact.plugin_id) {
                return Err(PluginClientError::InvalidRequest);
            }
            let mut supervisor = PluginSupervisor::new(PluginProcessConfig {
                executable: artifact.executable.clone(),
                runtime_root: self.runtime_root.clone(),
                // Each load/reload has an independent socket, even for the same plugin.
                instance_id: format!("p-{}", uuid::Uuid::new_v4().simple()),
                plugin_id: artifact.plugin_id.clone(),
                restart_policy: RestartPolicy::default(),
            })
            .map_err(|_| PluginClientError::InvalidRequest)?;
            supervisor
                .start()
                .await
                .map_err(|_| PluginClientError::Transport("plugin startup failed".into()))?;
            let (sender, receiver) = mpsc::channel(QUEUED_CALLS);
            tokio::spawn(run(
                supervisor,
                receiver,
                artifact.plugin_id.clone(),
                self.callbacks.clone(),
            ));
            Ok(Arc::new(ProcessPluginClient { sender }) as Arc<dyn PluginClient>)
        })
    }
}

struct ProcessPluginClient {
    sender: mpsc::Sender<Command>,
}
enum Command {
    Call(
        PluginCall,
        oneshot::Sender<Result<Envelope, PluginClientError>>,
    ),
    Shutdown(oneshot::Sender<Result<(), PluginClientError>>),
}

impl PluginClient for ProcessPluginClient {
    fn call<'a>(&'a self, call: PluginCall) -> PluginFuture<'a, Envelope> {
        Box::pin(async move {
            let (sender, receiver) = oneshot::channel();
            self.sender
                .try_send(Command::Call(call, sender))
                .map_err(|_| PluginClientError::Closed)?;
            receiver.await.map_err(|_| PluginClientError::Closed)?
        })
    }
    fn call_stream<'a>(&'a self, _call: PluginCall) -> PluginFuture<'a, PluginStream> {
        Box::pin(async { Err(PluginClientError::UnsupportedCapability) })
    }
    fn shutdown<'a>(&'a self) -> PluginFuture<'a, ()> {
        Box::pin(async move {
            let (sender, receiver) = oneshot::channel();
            self.sender
                .send(Command::Shutdown(sender))
                .await
                .map_err(|_| PluginClientError::Closed)?;
            receiver.await.map_err(|_| PluginClientError::Closed)?
        })
    }
}

async fn run(
    mut supervisor: PluginSupervisor,
    mut commands: mpsc::Receiver<Command>,
    plugin_id: String,
    callbacks: Arc<HostCallbackRouter>,
) {
    while let Some(command) = commands.recv().await {
        match command {
            Command::Shutdown(reply) => {
                let result = supervisor
                    .shutdown()
                    .await
                    .map(|_| ())
                    .map_err(|_| PluginClientError::Closed);
                let _ = reply.send(result);
                return;
            }
            Command::Call(call, mut reply) => {
                if reply.is_closed() {
                    continue;
                }
                let now = unix_time_ms();
                let duration = call
                    .deadline_unix_ms
                    .map(|deadline| Duration::from_millis(deadline.saturating_sub(now)))
                    .unwrap_or(CALL_TIMEOUT)
                    .min(CALL_TIMEOUT);
                if duration.is_zero() {
                    let _ = reply.send(Err(PluginClientError::DeadlineExceeded));
                    continue;
                }
                let result = tokio::select! {
                    _ = reply.closed() => Err(PluginClientError::Cancelled),
                    result = tokio::time::timeout(duration, supervisor.call_process(call, &plugin_id, &callbacks)) => result.unwrap_or(Err(PluginClientError::DeadlineExceeded)),
                };
                let failed = result.is_err();
                let _ = reply.send(result);
                if failed {
                    // A cancelled read_exact may have consumed half a frame.
                    // Never reuse that connection or deliver a stale response.
                    supervisor.abort_process().await;
                    return;
                }
            }
        }
    }
    supervisor.abort_process().await;
}

pub(super) async fn exchange_unary<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    call: PluginCall,
    plugin_id: &str,
    callbacks: &HostCallbackRouter,
) -> Result<Envelope, PluginClientError> {
    let callback_id = serde_json::from_str::<Value>(call.payload.get())
        .ok()
        .and_then(|v| {
            v.get("host_callback_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
    let request_id = format!("call-{}", uuid::Uuid::new_v4().simple());
    write_process_message(
        stream,
        &ProcessMessage::Request {
            protocol_version: PROCESS_PROTOCOL_VERSION,
            request_id: request_id.clone(),
            method: call.method,
            deadline_unix_ms: call.deadline_unix_ms,
            payload: call.payload,
        },
    )
    .await
    .map_err(|_| PluginClientError::InvalidRequest)?;
    loop {
        let message = read_process_message(stream)
            .await
            .map_err(|_| PluginClientError::InvalidResponse)?
            .ok_or(PluginClientError::Closed)?;
        match message {
            ProcessMessage::Response {
                request_id: id,
                envelope,
                ..
            } if id == request_id => return Ok(envelope),
            ProcessMessage::Request {
                request_id: id,
                method,
                payload,
                ..
            } if id != request_id && method.starts_with("host.") => {
                let mut value: Value = serde_json::from_str(payload.get())
                    .map_err(|_| PluginClientError::InvalidResponse)?;
                let supplied = value
                    .as_object_mut()
                    .and_then(|object| object.remove("host_callback_id"))
                    .and_then(|v| v.as_str().map(str::to_owned));
                let response = if supplied.as_deref().filter(|id| !id.is_empty())
                    == callback_id.as_deref().filter(|id| !id.is_empty())
                    && supplied.is_some()
                {
                    let payload =
                        to_raw_value(&value).map_err(|_| PluginClientError::InvalidResponse)?;
                    callbacks
                        .dispatch(
                            plugin_id,
                            supplied.as_deref().unwrap_or_default(),
                            &method,
                            &payload,
                            unix_time_ms(),
                        )
                        .await
                        .ok()
                } else {
                    None
                };
                let envelope = response.unwrap_or_else(|| {
                    Envelope::failure(Error {
                        code: "callback_denied".into(),
                        message: "Host callback authority rejected".into(),
                        ..Error::default()
                    })
                });
                write_process_message(
                    stream,
                    &ProcessMessage::Response {
                        protocol_version: PROCESS_PROTOCOL_VERSION,
                        request_id: id,
                        envelope,
                    },
                )
                .await
                .map_err(|_| PluginClientError::Closed)?;
            }
            _ => return Err(PluginClientError::InvalidResponse),
        }
    }
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
