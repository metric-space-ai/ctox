// ref: internal/pluginhost/rpc_schema_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: complete upstream schema parity inside CTOX process frames
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::json;

use super::rpc_schema::*;
use crate::sdk::{
    pluginabi::{Envelope, SCHEMA_VERSION},
    pluginapi::{ConfigField, ConfigFieldType, ExecutorModelScope, ExecutorRequest, Metadata},
};

fn registration() -> RpcRegistration {
    RpcRegistration {
        schema_version: SCHEMA_VERSION,
        metadata: Metadata {
            name: "safe-plugin".into(),
            version: "1.0.0".into(),
            author: "ctox".into(),
            github_repository: "https://example.invalid/safe-plugin".into(),
            logo: String::new(),
            config_fields: vec![ConfigField {
                name: "mode".into(),
                field_type: ConfigFieldType(ConfigFieldType::ENUM.into()),
                enum_values: vec!["safe".into()],
                description: "execution mode".into(),
                default_value: None,
            }],
        },
        capabilities: RpcCapabilities {
            model_registrar: false,
            model_provider: false,
            auth_provider: false,
            frontend_auth_provider: true,
            frontend_auth_provider_exclusive: true,
            scheduler: true,
            model_router: true,
            executor: true,
            executor_model_scope: ExecutorModelScope(ExecutorModelScope::BOTH.into()),
            executor_input_formats: vec!["openai-responses".into()],
            executor_output_formats: vec!["anthropic-messages".into()],
            request_translator: true,
            request_normalizer: false,
            request_interceptor: false,
            request_lifecycle_plugin: true,
            response_translator: true,
            response_before_translator: false,
            response_after_translator: false,
            response_interceptor: false,
            stream_chunk_interceptor: true,
            thinking_applier: false,
            usage_plugin: false,
            command_line_plugin: false,
            management_api: false,
        },
    }
}

#[test]
fn upstream_lifecycle_uses_go_byte_slice_encoding() {
    let request = RpcLifecycleRequest {
        config_yaml: b"mode: test".to_vec(),
        schema_version: SCHEMA_VERSION,
    };
    assert_eq!(
        serde_json::to_value(request).unwrap(),
        json!({"config_yaml":"bW9kZTogdGVzdA==","schema_version":2})
    );
}

#[test]
fn upstream_registration_preserves_capabilities_and_go_metadata_names() {
    let value = serde_json::to_value(registration()).unwrap();
    assert_eq!(value["metadata"]["Name"], "safe-plugin");
    assert_eq!(value["metadata"]["ConfigFields"][0]["Type"], "enum");
    assert_eq!(value["capabilities"]["scheduler"], true);
    assert_eq!(value["capabilities"]["model_router"], true);
    assert_eq!(
        value["capabilities"]["frontend_auth_provider_exclusive"],
        true
    );
    assert_eq!(value["capabilities"]["response_stream_interceptor"], true);
}

#[test]
fn upstream_schema_keeps_go_missing_and_unknown_field_compatibility() {
    let lifecycle: RpcLifecycleRequest = serde_json::from_value(json!({
        "schema_version": 2,
        "future_addition": true
    }))
    .unwrap();
    assert!(lifecycle.config_yaml.is_empty());

    let error: crate::sdk::pluginabi::Error = serde_json::from_value(json!({
        "code": "future",
        "message": "failure",
        "http_status": -1,
        "future_addition": {"ignored": true}
    }))
    .unwrap();
    assert_eq!(error.http_status, -1);
}

#[test]
fn process_frame_round_trips_upstream_payload() {
    let payload = encode_upstream_json(&RpcLifecycleRequest {
        config_yaml: b"mode: test".to_vec(),
        schema_version: SCHEMA_VERSION,
    })
    .unwrap();
    let message = ProcessMessage::Request {
        protocol_version: PROCESS_PROTOCOL_VERSION,
        request_id: "request-7".into(),
        method: crate::sdk::pluginabi::METHOD_PLUGIN_REGISTER.into(),
        deadline_unix_ms: Some(1_800_000_000_000),
        payload,
    };
    let frame = encode_process_frame(&message).unwrap();
    assert_eq!(
        u32::from_be_bytes(frame[..4].try_into().unwrap()) as usize,
        frame.len() - 4
    );
    let decoded = decode_process_frame(&frame).unwrap();
    assert_eq!(encode_process_frame(&decoded).unwrap(), frame);
}

#[test]
fn process_frame_carries_response_cancel_and_ordered_stream_signatures() {
    let response = ProcessMessage::Response {
        protocol_version: PROCESS_PROTOCOL_VERSION,
        request_id: "request-8".into(),
        envelope: Envelope::success(Some(encode_upstream_json(&registration()).unwrap())),
    };
    let cancel = ProcessMessage::Cancel {
        protocol_version: PROCESS_PROTOCOL_VERSION,
        request_id: "request-8".into(),
    };
    let chunk = ProcessMessage::StreamChunk {
        protocol_version: PROCESS_PROTOCOL_VERSION,
        request_id: "request-8".into(),
        sequence: 0,
        payload: serde_json::value::to_raw_value(&json!({"data":"one"})).unwrap(),
    };
    let end = ProcessMessage::StreamEnd {
        protocol_version: PROCESS_PROTOCOL_VERSION,
        request_id: "request-8".into(),
        next_sequence: 1,
        error: None,
    };
    for message in [response, cancel, chunk, end] {
        let frame = encode_process_frame(&message).unwrap();
        let decoded = decode_process_frame(&frame).unwrap();
        assert_eq!(encode_process_frame(&decoded).unwrap(), frame);
    }
}

#[test]
fn process_frame_fails_closed_on_limits_version_and_ambiguity() {
    let too_long_method = ProcessMessage::Request {
        protocol_version: PROCESS_PROTOCOL_VERSION,
        request_id: "request-9".into(),
        method: "x".repeat(MAX_METHOD_BYTES + 1),
        deadline_unix_ms: None,
        payload: serde_json::value::to_raw_value(&json!({})).unwrap(),
    };
    assert_eq!(
        encode_process_frame(&too_long_method),
        Err(ProcessCodecError::InvalidMethod)
    );

    let future = ProcessMessage::Cancel {
        protocol_version: PROCESS_PROTOCOL_VERSION + 1,
        request_id: "request-9".into(),
    };
    assert_eq!(
        encode_process_frame(&future),
        Err(ProcessCodecError::UnsupportedVersion)
    );

    let valid = ProcessMessage::Cancel {
        protocol_version: PROCESS_PROTOCOL_VERSION,
        request_id: "request-9".into(),
    };
    let mut trailing = encode_process_frame(&valid).unwrap();
    trailing.push(0);
    assert_eq!(
        decode_process_frame(&trailing).unwrap_err(),
        ProcessCodecError::TrailingBytes
    );

    let oversized = ((MAX_FRAME_BYTES as u32) + 1).to_be_bytes();
    assert_eq!(
        decode_process_frame(&oversized).unwrap_err(),
        ProcessCodecError::FrameTooLarge
    );
}

#[test]
fn codec_errors_never_echo_untrusted_payloads() {
    let secret = "do-not-echo-secret";
    let mut frame = (secret.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(secret.as_bytes());
    let error = decode_process_frame(&frame).unwrap_err().to_string();
    assert!(!error.contains(secret));
}

#[test]
fn lifecycle_and_process_debug_redact_payloads() {
    let secret = "debug-secret-never-render";
    let lifecycle = RpcLifecycleRequest {
        config_yaml: secret.as_bytes().to_vec(),
        schema_version: SCHEMA_VERSION,
    };
    assert!(!format!("{lifecycle:?}").contains(secret));

    let message = ProcessMessage::Response {
        protocol_version: PROCESS_PROTOCOL_VERSION,
        request_id: "request-redacted".into(),
        envelope: Envelope::success(Some(
            encode_upstream_json(&json!({"secret": secret})).unwrap(),
        )),
    };
    assert!(!format!("{message:?}").contains(secret));
}

#[test]
fn callback_wrapper_preserves_embedded_go_fields_and_callback_id() {
    let request = RpcCallbackRequest {
        request: crate::sdk::pluginapi::RequestCompletion {
            request_id: "request-1".to_owned(),
            ..crate::sdk::pluginapi::RequestCompletion::default()
        },
        host_callback_id: "callback-7".to_owned(),
    };
    let value = serde_json::to_value(request).unwrap();
    assert_eq!(value["RequestID"], "request-1");
    assert_eq!(value["host_callback_id"], "callback-7");
    assert!(value.get("request").is_none());
}

#[test]
fn executor_stream_schema_preserves_bytes_errors_and_owner_handles() {
    let request = RpcExecutorStreamRequest {
        request: ExecutorRequest {
            model: "model-a".to_owned(),
            payload: b"request".to_vec(),
            ..ExecutorRequest::default()
        },
        stream_id: "stream-1".to_owned(),
        host_callback_id: "callback-1".to_owned(),
    };
    let value = serde_json::to_value(request).unwrap();
    assert_eq!(value["Model"], "model-a");
    assert_eq!(value["Payload"], "cmVxdWVzdA==");
    assert_eq!(value["stream_id"], "stream-1");

    let response: RpcExecutorStreamResponse = serde_json::from_value(json!({
        "headers": {"x-test": ["one"]},
        "chunks": [{"Payload": "Y2h1bms=", "Err": "terminal"}]
    }))
    .unwrap();
    assert_eq!(response.chunks[0].payload, b"chunk");
    assert_eq!(response.chunks[0].error, "terminal");
}

#[test]
fn management_schema_carries_descriptors_without_function_pointers() {
    let response = RpcManagementRegistrationResponse {
        routes: vec![RpcManagementRoute {
            method: "POST".to_owned(),
            path: "/rotate".to_owned(),
            menu: "Runtime".to_owned(),
            description: "Rotate bounded state".to_owned(),
        }],
        resources: Vec::new(),
    };
    let value = serde_json::to_value(response).unwrap();
    assert_eq!(value["routes"][0]["path"], "/rotate");
    assert!(value["routes"][0].get("handler").is_none());
}
