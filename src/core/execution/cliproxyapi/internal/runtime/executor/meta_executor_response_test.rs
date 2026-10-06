use super::*;
use std::time::SystemTime;
const NOW: SystemTime = SystemTime::UNIX_EPOCH;

#[test]
fn candidate_meta_response_reconstructs_sorted_overwritten_and_fallback_raw_items() {
    let mut items = MetaOutputItems::default();
    for event in [
        br#"{"item":{"id":"old"},"output_index":1}"#.as_slice(),
        br#"{"item":{"id":"second","number":900719925474099312345},"output_index":1}"#,
        br#"{"item":{"id":"first"},"output_index":0}"#,
        br#"{"item":{"id":"fallback"}}"#,
        br#"{"item":"invalid","output_index":2}"#,
    ] {
        items.collect(event)
    }
    let out = items.patch_completed(
        br#"{"type":"response.incomplete","response":{"output":[],"usage":{"input_tokens":1}}}"#,
    );
    let doc = std::str::from_utf8(&out).unwrap();
    assert_eq!(gjson::get(doc, "response.output.0.id").str(), "first");
    assert_eq!(gjson::get(doc, "response.output.1.id").str(), "second");
    assert_eq!(gjson::get(doc, "response.output.2.id").str(), "fallback");
    assert_eq!(gjson::get(doc, "type").str(), "response.incomplete");
    assert!(doc.contains("900719925474099312345"));
    assert_eq!(gjson::get(doc, "response.usage.input_tokens").i64(), 1);
}
#[test]
fn candidate_meta_response_hydrates_only_missing_ids_without_replacing_terminal_output() {
    let mut items = MetaOutputItems::default();
    for index in 0..5 {
        items.collect(
            format!(r#"{{"output_index":{index},"item":{{"id":"server-{index}"}}}}"#).as_bytes(),
        );
    }
    let body=br#"{"type":"response.completed","response":{"output":[{"text":"preserved"},{"id":null},{"id":"  "},{"id":" client "},{"id":42}]}}"#;
    let out = items.patch_completed(body);
    let doc = std::str::from_utf8(&out).unwrap();
    for index in 0..3 {
        assert_eq!(
            gjson::get(doc, &format!("response.output.{index}.id")).str(),
            format!("server-{index}")
        );
    }
    assert_eq!(gjson::get(doc, "response.output.0.text").str(), "preserved");
    assert_eq!(gjson::get(doc, "response.output.3.id").str(), " client ");
    assert_eq!(gjson::get(doc, "response.output.4.id").i64(), 42);
}
#[test]
fn candidate_meta_response_completion_fallback_preserves_incomplete_and_rejects_arbitrary_json() {
    for body in [
        br#"{"type":"response.incomplete","response":{"output":[]}}"#.as_slice(),
        br#"{"type":"response.completed","response":{"output":[]}}"#,
    ] {
        assert_eq!(meta_as_completed_event(body).unwrap(), body);
    }
    let out = meta_as_completed_event(
        br#" { "object":"response","output":[],"number":900719925474099312345 } "#,
    )
    .unwrap();
    let doc = std::str::from_utf8(&out).unwrap();
    assert_eq!(gjson::get(doc, "type").str(), "response.completed");
    assert!(doc.contains("900719925474099312345"));
    for body in [
        b"not-json".as_slice(),
        br#"{"error":{"message":"failure"}}"#,
        b"null",
        b"[]",
    ] {
        assert!(meta_as_completed_event(body).is_none());
    }
}
#[test]
fn candidate_meta_response_error_numeric_code_and_exact_root_scope() {
    let error = meta_stream_event_error(br#"{"type":"error","error":{"code":401}}"#, NOW).unwrap();
    assert_eq!(error.status_code(), 401);
    assert_eq!(
        error
            .source()
            .unwrap()
            .downcast_ref::<AuthError>()
            .unwrap()
            .http_status,
        401
    );
    for code in ["399", "600", "\"rate_limit_exceeded\""] {
        let error = meta_stream_event_error(
            format!(r#"{{"type":"response.failed","error":{{"code":{code}}}}}"#).as_bytes(),
            NOW,
        )
        .unwrap();
        assert_eq!(error.status_code(), 502);
    }
    // Upstream checks error.code only, not response.error.code.
    assert_eq!(
        meta_stream_event_error(
            br#"{"type":"response.failed","response":{"error":{"code":401}}}"#,
            NOW
        )
        .unwrap()
        .status_code(),
        502
    );
    assert!(meta_stream_event_error(
        br#"{"type":"response.completed","error":{"code":401}}"#,
        NOW
    )
    .is_none());
}
#[test]
fn candidate_meta_response_reset_retry_and_subscription_scope_match_provider_contract() {
    let body=br#"{"error":{"code":"rate_limit_exceeded","message":"Subscription quota exhausted","resets_at":180}}"#;
    let error = meta_upstream_error(429, body, NOW);
    assert_eq!(error.retry_after, Some(Duration::from_secs(180)));
    assert!(error.credential_scoped);
    assert_eq!(
        meta_upstream_error(404, b"{}", NOW).retry_after,
        Some(META_NOT_FOUND_COOLDOWN)
    );
    assert_eq!(
        meta_upstream_error(404, body, NOW).retry_after,
        Some(Duration::from_secs(180))
    );
    assert_eq!(meta_upstream_error(200, body, NOW).retry_after, None);
    assert_eq!(
        meta_upstream_error(429, body, NOW + Duration::from_secs(200)).retry_after,
        None
    );
    assert!(
        !meta_upstream_error(429, br#"{"error":{"code":"rate_limit_exceeded"}}"#, NOW)
            .credential_scoped
    );
    assert!(!meta_upstream_error(403, body, NOW).credential_scoped);
    let debug = format!("{error:?}");
    assert!(!debug.contains("Subscription quota"));
}
#[test]
fn candidate_meta_response_fragmented_lines_crlf_and_final_unterminated_line() {
    let mut lines = MetaResponseLines::default();
    assert!(lines.push(b"da").unwrap().is_empty());
    assert_eq!(
        lines.push(b"ta: {}\r\n\r\nlast").unwrap(),
        vec![b"data: {}".to_vec(), Vec::new()]
    );
    assert_eq!(lines.finish().unwrap(), b"last");
    assert!(lines.finish().is_none());
}
