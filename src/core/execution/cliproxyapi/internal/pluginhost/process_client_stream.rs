// Origin: CTOX; License: AGPL-3.0-only
//! Ordered, backpressured process streaming with the same scoped callbacks as unary calls.
use super::{
    abi::{PluginCall, PluginClientError},
    host_callbacks::HostCallbackRouter,
    process_client::{dispatch_callback, unix_time_ms},
    process_transport::{
        read_process_message, write_process_message, InflightRequests, ProcessEvent, RequestMode,
    },
    rpc_schema::{ProcessMessage, PROCESS_PROTOCOL_VERSION},
};
use crate::sdk::pluginabi::Error;
use serde_json::{value::RawValue, Value};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::mpsc,
};

pub(super) async fn exchange_stream<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    call: PluginCall,
    plugin_id: &str,
    callbacks: &HostCallbackRouter,
    chunks: &mpsc::Sender<Result<Box<RawValue>, PluginClientError>>,
) -> Result<Option<Error>, PluginClientError> {
    let callback_id = serde_json::from_str::<Value>(call.payload.get())
        .ok()
        .and_then(|v| {
            v.get("host_callback_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
    let request_id = format!("call-{}", uuid::Uuid::new_v4().simple());
    let mut inflight = InflightRequests::new();
    inflight
        .begin(
            request_id.clone(),
            RequestMode::Stream,
            call.deadline_unix_ms,
            unix_time_ms(),
        )
        .map_err(|_| PluginClientError::InvalidRequest)?;
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
    .map_err(|_| PluginClientError::Closed)?;
    loop {
        let message = read_process_message(stream)
            .await
            .map_err(|_| PluginClientError::InvalidResponse)?
            .ok_or(PluginClientError::Closed)?;
        if matches!(&message, ProcessMessage::Request { .. }) {
            dispatch_callback(
                stream,
                message,
                &request_id,
                callback_id.as_deref(),
                plugin_id,
                callbacks,
            )
            .await?;
            continue;
        }
        match inflight
            .observe(message, unix_time_ms())
            .map_err(|_| PluginClientError::InvalidResponse)?
        {
            ProcessEvent::StreamChunk {
                request_id: id,
                payload,
                ..
            } if id == request_id => chunks
                .send(Ok(payload))
                .await
                .map_err(|_| PluginClientError::Cancelled)?,
            ProcessEvent::StreamEnd {
                request_id: id,
                error,
                ..
            } if id == request_id => return Ok(error),
            ProcessEvent::Cancelled {
                request_id: id,
                was_active: true,
            } if id == request_id => return Err(PluginClientError::Cancelled),
            _ => return Err(PluginClientError::InvalidResponse),
        }
    }
}
