// ref: internal/logging/cpa_trace_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::{Arc, Barrier};
use std::thread;

use chrono::NaiveDate;

use super::*;

#[test]
fn request_ids_preserve_context_derivation_and_handler_precedence() {
    for _ in 0..32 {
        let request_id = generate_request_id();
        assert_eq!(request_id.len(), 8);
        assert!(request_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
    }

    assert_eq!(get_request_id(None), "");
    assert_eq!(get_handler_request_id(None), "");
    let base = RequestContext::default();
    let derived = with_request_id(Some(&base), "context-id");
    assert_eq!(get_request_id(Some(&base)), "");
    assert_eq!(get_request_id(Some(&derived)), "context-id");

    let mut handler = derived.clone();
    set_handler_request_id(Some(&mut handler), "handler-id");
    assert_eq!(get_request_id(Some(&handler)), "context-id");
    assert_eq!(get_handler_request_id(Some(&handler)), "handler-id");
    let callback = handler_cpa_trace_id_callback(Some(&mut handler)).unwrap();
    callback("auth");
    assert!(get_handler_cpa_trace_id(Some(&handler)).ends_with("-auth-handler-id"));
}

#[test]
fn request_metadata_snapshots_values_and_shares_only_explicit_holders() {
    assert_eq!(get_endpoint(None), "");
    assert_eq!(
        get_client_request_metadata(None),
        ClientRequestMetadata::default()
    );
    assert_eq!(get_response_status(None), 0);
    assert_eq!(get_response_headers(None), None);

    let base = RequestContext::default();
    let endpoint = with_endpoint(Some(&base), "/v1/responses");
    assert_eq!(get_endpoint(Some(&base)), "");
    assert_eq!(get_endpoint(Some(&endpoint)), "/v1/responses");

    let mut metadata = ClientRequestMetadata {
        client_ip: "192.0.2.10".into(),
        x_forwarded_for: "198.51.100.1".into(),
        user_agent: "agent/1".into(),
    };
    let context = with_client_request_metadata(Some(&endpoint), metadata.clone());
    metadata.client_ip = "changed".into();
    assert_eq!(
        get_client_request_metadata(Some(&context)).client_ip,
        "192.0.2.10"
    );

    set_response_status(Some(&context), 204);
    assert_eq!(get_response_status(Some(&context)), 0);
    let status_context = with_response_status_holder(Some(&context));
    let status_derived = with_response_status_holder(Some(&status_context));
    set_response_status(Some(&status_derived), 204);
    assert_eq!(get_response_status(Some(&status_context)), 204);
    set_response_status(Some(&status_context), 0);
    assert_eq!(get_response_status(Some(&status_derived)), 204);

    let header_context = with_response_headers_holder(Some(&status_context));
    let mut source = ResponseHeaders::from([("X-Test".into(), vec!["one".into(), "two".into()])]);
    set_response_headers(Some(&header_context), &source);
    source.get_mut("X-Test").unwrap().push("changed".into());
    let mut snapshot = get_response_headers(Some(&header_context)).unwrap();
    assert_eq!(snapshot["X-Test"], ["one", "two"]);
    snapshot.get_mut("X-Test").unwrap().clear();
    assert_eq!(
        get_response_headers(Some(&header_context)).unwrap()["X-Test"],
        ["one", "two"]
    );
    set_response_headers(Some(&header_context), &ResponseHeaders::new());
    assert_eq!(get_response_headers(Some(&header_context)), None);
}

#[test]
fn cpa_trace_writer_requires_selection_before_response_commit() {
    let mut selected = RequestContext::default();
    set_handler_request_id(Some(&mut selected), "1234abcd");
    let mut writer = CpaTraceResponseWriter::new(&mut selected);
    set_handler_cpa_trace_id(Some(&mut selected), "auth-index");
    writer.write_header(200);
    let trace_id = &writer.headers()[CPA_TRACE_ID_HEADER][0];
    assert_eq!(trace_id.len(), "20060102150405-auth-index-1234abcd".len());
    assert_eq!(&trace_id[15..], "auth-index-1234abcd");
    assert_eq!(writer.status_code(), Some(200));

    let mut unselected = RequestContext::default();
    set_handler_request_id(Some(&mut unselected), "1234abcd");
    let mut writer = CpaTraceResponseWriter::new(&mut unselected);
    set_handler_cpa_trace_id(Some(&mut unselected), "");
    writer.write_header(200);
    assert!(!writer.headers().contains_key(CPA_TRACE_ID_HEADER));

    let mut committed = RequestContext::default();
    set_handler_request_id(Some(&mut committed), "1234abcd");
    let mut writer = CpaTraceResponseWriter::new(&mut committed);
    writer.write_header_now();
    set_handler_cpa_trace_id(Some(&mut committed), "auth-index");
    assert!(!writer.headers().contains_key(CPA_TRACE_ID_HEADER));
}

#[test]
fn cpa_callback_survives_context_release_and_writer_operations_commit_once() {
    let (callback, mut writer) = {
        let mut context = with_request_id(None, "context-request");
        let writer = CpaTraceResponseWriter::new(&mut context);
        let callback = handler_cpa_trace_id_callback(Some(&mut context)).unwrap();
        (callback, writer)
    };
    callback(" auth-index ");
    assert_eq!(writer.write_string("hello"), 5);
    assert_eq!(writer.write(b" world"), 6);
    writer.flush();
    writer.write_header(503);
    assert_eq!(writer.status_code(), Some(200));
    assert_eq!(writer.body(), b"hello world");
    assert!(writer.written());
    assert!(writer.flushed());
    assert!(writer.headers()[CPA_TRACE_ID_HEADER][0].ends_with("-auth-index-context-request"));
}

#[test]
fn concurrent_selection_and_response_commit_remain_request_local() {
    for _ in 0..100 {
        let mut context = RequestContext::default();
        set_handler_request_id(Some(&mut context), "1234abcd");
        let callback = handler_cpa_trace_id_callback(Some(&mut context)).unwrap();
        let mut writer = CpaTraceResponseWriter::new(&mut context);
        let barrier = Arc::new(Barrier::new(2));
        let worker_barrier = Arc::clone(&barrier);
        let worker = thread::spawn(move || {
            worker_barrier.wait();
            callback("auth-index");
        });
        barrier.wait();
        assert_eq!(writer.write(b"\n"), 1);
        worker.join().unwrap();
        assert_eq!(writer.status_code(), Some(200));
    }
}
