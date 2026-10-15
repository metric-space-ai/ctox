// Origin: CTOX
// License: AGPL-3.0-only

//! Explicitly injected, process-isolated unary and streaming plugin transport.
//! No runtime discovers or starts plugins
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
const STREAM_TIMEOUT: Duration = Duration::from_secs(180);
const QUEUED_CALLS: usize = 16;
const STREAM_BUFFER_CHUNKS: usize = 2;
type ShutdownReply = oneshot::Sender<Result<(), PluginClientError>>;

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
            let (shutdown, shutdowns) = mpsc::channel(1);
            tokio::spawn(run(
                supervisor,
                receiver,
                shutdowns,
                artifact.plugin_id.clone(),
                self.callbacks.clone(),
            ));
            Ok(Arc::new(ProcessPluginClient { sender, shutdown }) as Arc<dyn PluginClient>)
        })
    }
}

struct ProcessPluginClient {
    sender: mpsc::Sender<Command>,
    shutdown: mpsc::Sender<ShutdownReply>,
}
enum Command {
    Call(
        PluginCall,
        oneshot::Sender<Result<Envelope, PluginClientError>>,
    ),
    Stream(
        PluginCall,
        oneshot::Sender<Result<PluginStream, PluginClientError>>,
    ),
}

impl PluginClient for ProcessPluginClient {
    fn call<'a>(&'a self, call: PluginCall) -> PluginFuture<'a, Envelope> {
        Box::pin(async move {
            let call = bounded_call(call, CALL_TIMEOUT)?;
            let deadline = call.deadline_unix_ms.unwrap_or_default();
            let (sender, receiver) = oneshot::channel();
            self.sender
                .try_send(Command::Call(call, sender))
                .map_err(|_| PluginClientError::Closed)?;
            tokio::time::timeout(
                Duration::from_millis(deadline.saturating_sub(unix_time_ms())),
                receiver,
            )
            .await
            .map_err(|_| PluginClientError::DeadlineExceeded)?
            .map_err(|_| PluginClientError::Closed)?
        })
    }
    fn call_stream<'a>(&'a self, call: PluginCall) -> PluginFuture<'a, PluginStream> {
        Box::pin(async move {
            let call = bounded_call(call, STREAM_TIMEOUT)?;
            let deadline = call.deadline_unix_ms.unwrap_or_default();
            let (sender, receiver) = oneshot::channel();
            self.sender
                .try_send(Command::Stream(call, sender))
                .map_err(|_| PluginClientError::Closed)?;
            tokio::time::timeout(
                Duration::from_millis(deadline.saturating_sub(unix_time_ms())),
                receiver,
            )
            .await
            .map_err(|_| PluginClientError::DeadlineExceeded)?
            .map_err(|_| PluginClientError::Closed)?
        })
    }
    fn shutdown<'a>(&'a self) -> PluginFuture<'a, ()> {
        Box::pin(async move {
            let (sender, receiver) = oneshot::channel();
            self.shutdown
                .send(sender)
                .await
                .map_err(|_| PluginClientError::Closed)?;
            receiver.await.map_err(|_| PluginClientError::Closed)?
        })
    }
}

async fn run(
    mut supervisor: PluginSupervisor,
    mut commands: mpsc::Receiver<Command>,
    mut shutdowns: mpsc::Receiver<ShutdownReply>,
    plugin_id: String,
    callbacks: Arc<HostCallbackRouter>,
) {
    loop {
        let command = tokio::select! {
            biased;
            reply = shutdowns.recv() => {
                if let Some(reply) = reply {
                    let result = supervisor.shutdown().await.map(|_| ()).map_err(|_| PluginClientError::Closed);
                    let _ = reply.send(result);
                } else {
                    supervisor.abort_process().await;
                }
                return;
            },
            command = commands.recv() => match command {
                Some(command) => command,
                None => { supervisor.abort_process().await; return; }
            },
        };
        match command {
            Command::Stream(call, reply) => {
                if !run_stream(
                    &mut supervisor,
                    call,
                    reply,
                    &mut shutdowns,
                    &plugin_id,
                    &callbacks,
                )
                .await
                {
                    return;
                }
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
}

fn bounded_call(mut call: PluginCall, ceiling: Duration) -> Result<PluginCall, PluginClientError> {
    let now = unix_time_ms();
    let deadline = call
        .deadline_unix_ms
        .unwrap_or(u64::MAX)
        .min(now.saturating_add(ceiling.as_millis() as u64));
    if deadline <= now {
        return Err(PluginClientError::DeadlineExceeded);
    }
    call.deadline_unix_ms = Some(deadline);
    // Reject malformed input before it can poison a healthy child connection.
    super::rpc_schema::encode_process_frame(&ProcessMessage::Request {
        protocol_version: PROCESS_PROTOCOL_VERSION,
        request_id: "validate".into(),
        method: call.method.clone(),
        deadline_unix_ms: call.deadline_unix_ms,
        payload: call.payload.clone(),
    })
    .map_err(|_| PluginClientError::InvalidRequest)?;
    Ok(call)
}

async fn run_stream(
    supervisor: &mut PluginSupervisor,
    call: PluginCall,
    reply: oneshot::Sender<Result<PluginStream, PluginClientError>>,
    shutdowns: &mut mpsc::Receiver<ShutdownReply>,
    plugin_id: &str,
    callbacks: &HostCallbackRouter,
) -> bool {
    if reply.is_closed() {
        return true;
    }
    let duration = Duration::from_millis(
        call.deadline_unix_ms
            .unwrap_or_default()
            .saturating_sub(unix_time_ms()),
    )
    .min(STREAM_TIMEOUT);
    if duration.is_zero() {
        let _ = reply.send(Err(PluginClientError::DeadlineExceeded));
        return true;
    }
    let (chunks, receiver) = mpsc::channel(STREAM_BUFFER_CHUNKS + 1);
    // Reserve a terminal slot: timeout/protocol failures must remain observable
    // even when a slow consumer has filled the bounded data buffer.
    let terminal = chunks
        .clone()
        .reserve_owned()
        .await
        .expect("new stream channel");
    if reply.send(Ok(PluginStream { chunks: receiver })).is_err() {
        return true;
    }
    enum Completion {
        Stop(Option<ShutdownReply>),
        Finished(Result<Option<Error>, PluginClientError>),
    }
    let completion = tokio::select! {
        biased;
        stop = shutdowns.recv() => Completion::Stop(stop),
        _ = chunks.closed() => Completion::Finished(Err(PluginClientError::Cancelled)),
        result = tokio::time::timeout(duration, supervisor.call_process_stream(call, plugin_id, callbacks, &chunks)) =>
            Completion::Finished(result.unwrap_or(Err(PluginClientError::DeadlineExceeded))),
    };
    let result = match completion {
        Completion::Stop(stop) => {
            terminal.send(Err(PluginClientError::Cancelled));
            supervisor.abort_process().await;
            if let Some(stop) = stop {
                let _ = stop.send(Ok(()));
            }
            return false;
        }
        Completion::Finished(result) => result,
    };
    match result {
        Ok(Some(error)) => {
            terminal.send(Err(PluginClientError::Plugin {
                code: error.code,
                message: error.message,
                retryable: error.retryable,
                http_status: error.http_status,
            }));
            true
        }
        Ok(None) => true,
        Err(error) => {
            terminal.send(Err(error));
            supervisor.abort_process().await;
            false
        }
    }
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
            message @ ProcessMessage::Request { .. } => {
                dispatch_callback(
                    stream,
                    message,
                    &request_id,
                    callback_id.as_deref(),
                    plugin_id,
                    callbacks,
                )
                .await?;
            }
            _ => return Err(PluginClientError::InvalidResponse),
        }
    }
}

pub(super) async fn dispatch_callback<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    message: ProcessMessage,
    request_id: &str,
    callback_id: Option<&str>,
    plugin_id: &str,
    callbacks: &HostCallbackRouter,
) -> Result<(), PluginClientError> {
    let ProcessMessage::Request {
        request_id: id,
        method,
        payload,
        ..
    } = message
    else {
        return Err(PluginClientError::InvalidResponse);
    };
    if id == request_id || !method.starts_with("host.") {
        return Err(PluginClientError::InvalidResponse);
    }
    let mut value: Value =
        serde_json::from_str(payload.get()).map_err(|_| PluginClientError::InvalidResponse)?;
    let supplied = value
        .as_object_mut()
        .and_then(|object| object.remove("host_callback_id"))
        .and_then(|v| v.as_str().map(str::to_owned));
    let response = if supplied.as_deref().filter(|id| !id.is_empty())
        == callback_id.filter(|id| !id.is_empty())
        && supplied.is_some()
    {
        let payload = to_raw_value(&value).map_err(|_| PluginClientError::InvalidResponse)?;
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
    .map_err(|_| PluginClientError::Closed)
}

pub(super) fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
