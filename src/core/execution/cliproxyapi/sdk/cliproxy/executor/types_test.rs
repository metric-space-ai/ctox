// ref: sdk/cliproxy/executor/types_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::sdk::translator;

use super::{
    downstream_websocket, response_format_or_source, with_downstream_websocket,
    with_required_upstream_websocket, ExecutionMetadata, Options, RequestTerminatedError,
    StatusError, StreamChunk, StreamResult,
};

#[test]
fn response_format_or_source_uses_explicit_response_format() {
    let options = Options {
        source_format: translator::openai(),
        response_format: translator::claude(),
        ..Options::default()
    };

    assert_eq!(response_format_or_source(&options), translator::claude());
}

#[test]
fn response_format_or_source_falls_back_to_source_format() {
    let options = Options {
        source_format: translator::gemini(),
        ..Options::default()
    };

    assert_eq!(response_format_or_source(&options), translator::gemini());
}

#[test]
fn terminated_error_returns_owned_response_copies_and_typed_status() {
    let mut headers = BTreeMap::new();
    headers.insert(
        "content-type".to_owned(),
        vec!["application/json".to_owned()],
    );
    let error = RequestTerminatedError {
        http_status: 409,
        headers,
        body: br#"{"error":"blocked"}"#.to_vec(),
    };

    let mut copied_headers = error.response_headers();
    copied_headers
        .get_mut("content-type")
        .expect("copied header")
        .push("changed".to_owned());
    let mut copied_body = error.response_body();
    copied_body.clear();

    assert_eq!(error.status_code(), 409);
    assert_eq!(StatusError::status_code(&error), 409);
    assert_eq!(error.headers["content-type"].len(), 1);
    assert_eq!(error.body, br#"{"error":"blocked"}"#);
    assert_eq!(error.to_string(), "request terminated by plugin");
}

#[test]
fn metadata_keeps_callbacks_typed_and_generation_defaults_enabled() {
    let selected = Arc::new(Mutex::new(Vec::new()));
    let selected_for_callback = Arc::clone(&selected);
    let mut metadata = ExecutionMetadata {
        selected_auth_callback: Some(Arc::new(move |auth_id| {
            selected_for_callback
                .lock()
                .expect("selected callback lock")
                .push(format!("auth:{auth_id}"));
        })),
        ..ExecutionMetadata::default()
    };
    let selected_for_callback = Arc::clone(&selected);
    metadata.selected_auth_index_callback = Some(Arc::new(move |auth_index| {
        selected_for_callback
            .lock()
            .expect("selected callback lock")
            .push(format!("index:{auth_index}"));
    }));

    assert!(metadata.generate_enabled());
    metadata.generate = Some(true);
    assert!(metadata.generate_enabled());
    metadata.generate = Some(false);
    assert!(!metadata.generate_enabled());

    metadata.notify_selected_auth("account-a", "stable-7");
    assert_eq!(
        *selected.lock().expect("selected callback lock"),
        ["auth:account-a", "index:stable-7"]
    );
    assert!(!metadata
        .extensions
        .contains_key(super::SELECTED_AUTH_CALLBACK_METADATA_KEY));
}

#[tokio::test]
async fn stream_result_owns_receive_only_chunk_lane() {
    let (sender, receiver) = mpsc::channel(1);
    let mut result = StreamResult {
        headers: BTreeMap::new(),
        chunks: receiver,
    };
    sender
        .send(StreamChunk {
            payload: b"data: one\n\n".to_vec(),
            error: None,
        })
        .await
        .expect("chunk receiver remains alive");
    drop(sender);

    let chunk = result.chunks.recv().await.expect("one chunk");
    assert_eq!(chunk.payload, b"data: one\n\n");
    assert!(chunk.error.is_none());
    assert!(result.chunks.recv().await.is_none());
}
