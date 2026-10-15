// ref: internal/pluginhost/rpc_client_stream.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: upstream streaming semantics over bounded process channels
// License: MIT (upstream); modifications AGPL-3.0-only

use serde::{de::DeserializeOwned, Serialize};
use serde_json::{value::to_raw_value, Value};
use tokio::sync::mpsc;

use super::abi::{PluginCall, PluginClientError};
use super::rpc_client::{sanitize_plugin_value, RpcPluginClient};

impl RpcPluginClient {
    pub async fn call_stream<Request, Chunk>(
        &self,
        method: &str,
        request: &Request,
        deadline_unix_ms: Option<u64>,
        output_capacity: usize,
    ) -> Result<mpsc::Receiver<Result<Chunk, PluginClientError>>, PluginClientError>
    where
        Request: Serialize + ?Sized,
        Chunk: DeserializeOwned + Send + 'static,
    {
        if output_capacity == 0 || output_capacity > 256 {
            return Err(PluginClientError::InvalidRequest);
        }
        let request =
            serde_json::to_value(request).map_err(|_| PluginClientError::InvalidRequest)?;
        let payload = to_raw_value(&sanitize_plugin_value(request))
            .map_err(|_| PluginClientError::InvalidRequest)?;
        let mut stream = self
            .transport()
            .call_stream(PluginCall {
                method: method.to_owned(),
                payload,
                deadline_unix_ms,
            })
            .await?;
        let (sender, receiver) = mpsc::channel(output_capacity);
        tokio::spawn(async move {
            loop {
                let chunk = tokio::select! {
                    biased;
                    _ = sender.closed() => break,
                    chunk = stream.chunks.recv() => chunk,
                };
                let Some(chunk) = chunk else {
                    break;
                };

                let decoded = chunk.and_then(|raw| {
                    serde_json::from_str::<Chunk>(raw.get())
                        .map_err(|_| PluginClientError::InvalidResponse)
                });
                let terminal = decoded.is_err();
                if sender.send(decoded).await.is_err() || terminal {
                    break;
                }
            }
        });
        Ok(receiver)
    }
}

pub fn sanitize_stream_metadata(mut value: Value) -> Value {
    value = sanitize_plugin_value(value);
    value
}
