// ref: internal/pluginhost/rpc_client_stream_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: upstream stream ordering and terminal-error evidence over process IPC
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use serde_json::{json, value::to_raw_value, Value};
use tokio::sync::mpsc;

use crate::sdk::pluginabi::Envelope;

use super::abi::{PluginCall, PluginClient, PluginClientError, PluginFuture, PluginStream};
use super::rpc_client::RpcPluginClient;

struct StreamClient {
    chunks: Vec<Result<Box<serde_json::value::RawValue>, PluginClientError>>,
}

impl PluginClient for StreamClient {
    fn call<'a>(&'a self, _call: PluginCall) -> PluginFuture<'a, Envelope> {
        Box::pin(async { Err(PluginClientError::UnsupportedCapability) })
    }

    fn call_stream<'a>(&'a self, _call: PluginCall) -> PluginFuture<'a, PluginStream> {
        Box::pin(async move {
            let (sender, receiver) = mpsc::channel(4);
            for chunk in &self.chunks {
                let copy = match chunk {
                    Ok(raw) => Ok(
                        to_raw_value(&serde_json::from_str::<Value>(raw.get()).unwrap()).unwrap(),
                    ),
                    Err(error) => Err(error.clone()),
                };
                sender.send(copy).await.unwrap();
            }
            drop(sender);
            Ok(PluginStream { chunks: receiver })
        })
    }

    fn shutdown<'a>(&'a self) -> PluginFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn ordered_chunks_decode_and_transport_error_is_terminal() {
    let client = RpcPluginClient::new(Arc::new(StreamClient {
        chunks: vec![
            Ok(to_raw_value(&json!({"index": 0})).unwrap()),
            Ok(to_raw_value(&json!({"index": 1})).unwrap()),
            Err(PluginClientError::Transport("closed".to_owned())),
            Ok(to_raw_value(&json!({"index": 2})).unwrap()),
        ],
    }));
    let mut chunks = client
        .call_stream::<_, Value>("executor.execute_stream", &json!({}), None, 4)
        .await
        .unwrap();
    assert_eq!(chunks.recv().await.unwrap().unwrap(), json!({"index": 0}));
    assert_eq!(chunks.recv().await.unwrap().unwrap(), json!({"index": 1}));
    assert_eq!(
        chunks.recv().await.unwrap(),
        Err(PluginClientError::Transport("closed".to_owned()))
    );
    assert!(chunks.recv().await.is_none());
}

#[tokio::test]
async fn invalid_capacity_is_rejected_before_transport() {
    let client = RpcPluginClient::new(Arc::new(StreamClient { chunks: vec![] }));
    assert!(matches!(
        client
            .call_stream::<_, Value>("executor.execute_stream", &json!({}), None, 0)
            .await,
        Err(PluginClientError::InvalidRequest)
    ));
}

#[tokio::test]
async fn dropping_typed_stream_cancels_a_stalled_producer_before_any_chunk() {
    type ChunkReceiver =
        mpsc::Receiver<Result<Box<serde_json::value::RawValue>, PluginClientError>>;
    struct PendingClient(std::sync::Mutex<Option<ChunkReceiver>>);
    impl PluginClient for PendingClient {
        fn call<'a>(&'a self, _: PluginCall) -> PluginFuture<'a, Envelope> {
            Box::pin(async { Err(PluginClientError::UnsupportedCapability) })
        }
        fn call_stream<'a>(&'a self, _: PluginCall) -> PluginFuture<'a, PluginStream> {
            Box::pin(async move {
                let chunks = self
                    .0
                    .lock()
                    .unwrap()
                    .take()
                    .ok_or(PluginClientError::Closed)?;
                Ok(PluginStream { chunks })
            })
        }
        fn shutdown<'a>(&'a self) -> PluginFuture<'a, ()> {
            Box::pin(async { Ok(()) })
        }
    }
    let (producer, receiver) = mpsc::channel(1);
    let client = RpcPluginClient::new(Arc::new(PendingClient(std::sync::Mutex::new(Some(
        receiver,
    )))));
    let stream = client
        .call_stream::<_, Value>("executor.execute_stream", &json!({}), None, 1)
        .await
        .unwrap();
    drop(stream);
    tokio::time::timeout(std::time::Duration::from_secs(1), producer.closed())
        .await
        .expect("stalled producer was not canceled when its typed consumer closed");
}
