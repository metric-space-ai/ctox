// ref: internal/runtime/executor/openai_responses_signature_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::borrow::Cow;

use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};

use super::openai_responses_signature::{
    sanitize_openai_responses_reasoning_encrypted_content,
    sanitize_openai_responses_reasoning_encrypted_content_with_compat,
};
use crate::internal::signature::{
    detect_signature_provider, SignatureProvider, GEMINI_SKIP_THOUGHT_SIGNATURE_VALIDATOR,
};

fn valid_encrypted_content() -> String {
    let mut payload = vec![0_u8; 1 + 8 + 16 + 16 + 32];
    payload[0] = 0x80;
    for (index, byte) in payload.iter_mut().enumerate().skip(9) {
        *byte = index as u8;
    }
    general_purpose::URL_SAFE_NO_PAD.encode(payload)
}

#[test]
fn strips_orphan_ids_when_store_is_disabled() {
    let valid = valid_encrypted_content();
    let body = format!(
        r#"{{"store":false,"input":[{{"id":"rs_bad","type":"reasoning","encrypted_content":"bad","summary":[]}},{{"id":"rs_orphan","type":"reasoning","summary":[]}},{{"id":"rs_good","type":"reasoning","encrypted_content":"{valid}","summary":[]}},{{"id":"msg_1","type":"message","role":"user","content":"hi"}}]}}"#
    );
    let output = sanitize_openai_responses_reasoning_encrypted_content("test", body.as_bytes());
    let value: Value = serde_json::from_slice(&output).unwrap();
    assert!(value.pointer("/input/0/encrypted_content").is_none());
    assert!(value.pointer("/input/0/id").is_none());
    assert!(value.pointer("/input/1/id").is_none());
    assert_eq!(value["input"][2]["id"], "rs_good");
    assert_eq!(value["input"][2]["encrypted_content"], valid);
    assert_eq!(value["input"][3]["id"], "msg_1");
}

#[test]
fn keeps_ids_when_store_is_enabled() {
    let body = br#"{"store":true,"input":[{"id":"rs_bad","type":"reasoning","encrypted_content":"bad","summary":[]},{"id":"rs_orphan","type":"reasoning","summary":[]}]}"#;
    let output = sanitize_openai_responses_reasoning_encrypted_content("test", body);
    let value: Value = serde_json::from_slice(&output).unwrap();
    assert!(value.pointer("/input/0/encrypted_content").is_none());
    assert_eq!(value["input"][0]["id"], "rs_bad");
    assert_eq!(value["input"][1]["id"], "rs_orphan");
}

#[test]
fn noop_borrows_the_original_body() {
    let valid = valid_encrypted_content();
    let body = format!(
        r#"{{"store":false,"input":[{{"id":"rs_good","type":"reasoning","encrypted_content":"{valid}","summary":[]}},{{"role":"user","content":"hi"}}]}}"#
    );
    let output = sanitize_openai_responses_reasoning_encrypted_content("test", body.as_bytes());
    assert!(matches!(output, Cow::Borrowed(_)));
    assert_eq!(output.as_ref(), body.as_bytes());
}

#[test]
fn invalid_json_and_non_array_input_are_byte_identical_noops() {
    for body in [b"not-json".as_slice(), br#"{"input":"hello"}"#] {
        let output = sanitize_openai_responses_reasoning_encrypted_content("", body);
        assert!(matches!(output, Cow::Borrowed(_)));
        assert_eq!(output.as_ref(), body);
    }
}

#[test]
fn candidate_v16_openai_reasoning_unknown_encrypted_compatibility_is_byte_identical() {
    let body = br#" { "store":false, "input":[{"type":"reasoning","id":"rs_opaque","summary":[],"content":null,"encrypted_content":"opaque-encrypted-reasoning-token-xyz"}] } "#;
    let compatible =
        sanitize_openai_responses_reasoning_encrypted_content_with_compat("test", body, true);
    assert!(matches!(compatible, Cow::Borrowed(_)));
    assert_eq!(compatible.as_ref(), body);
    let strict: Value = serde_json::from_slice(
        &sanitize_openai_responses_reasoning_encrypted_content("test", body),
    )
    .unwrap();
    assert!(strict["input"][0].get("encrypted_content").is_none());
    assert!(strict["input"][0].get("id").is_none());
}

#[test]
fn candidate_v16_openai_reasoning_compatibility_still_strips_invalid_and_foreign_signatures() {
    assert_eq!(
        detect_signature_provider(GEMINI_SKIP_THOUGHT_SIGNATURE_VALIDATOR),
        SignatureProvider::GeminiBypass
    );
    for signature in [
        json!(""),
        json!(" opaque "),
        Value::Null,
        json!(42),
        json!(GEMINI_SKIP_THOUGHT_SIGNATURE_VALIDATOR),
    ] {
        let body = serde_json::to_vec(&json!({"store":false,"input":[{
            "type":"reasoning","id":"rs_compat","encrypted_content":signature
        }]}))
        .unwrap();
        let output: Value = serde_json::from_slice(
            &sanitize_openai_responses_reasoning_encrypted_content_with_compat("test", &body, true),
        )
        .unwrap();
        assert!(
            output["input"][0].get("encrypted_content").is_none(),
            "{signature}"
        );
        assert_eq!(output["input"][0]["id"], "rs_compat", "{signature}");
    }
}

#[test]
fn candidate_v16_openai_reasoning_cleartext_promotes_only_in_strict_mode() {
    let body = serde_json::to_vec(&json!({"store":false,"input":[{
        "type":"reasoning","id":"rs_text","summary":null,
        "content":[{"type":" reasoning_text ","text":"visible thought"},
            {"type":"reasoning_text","text":""},{"type":"output_text","text":"ignored"}]
    }]}))
    .unwrap();
    let strict: Value = serde_json::from_slice(
        &sanitize_openai_responses_reasoning_encrypted_content("test", &body),
    )
    .unwrap();
    assert_eq!(
        strict["input"][0]["summary"],
        json!([{"type":"summary_text","text":"visible thought"}])
    );
    assert_eq!(strict["input"][0]["content"], json!([]));
    assert!(strict["input"][0].get("id").is_none());
    let compatible =
        sanitize_openai_responses_reasoning_encrypted_content_with_compat("test", &body, true);
    assert!(matches!(compatible, Cow::Borrowed(_)));
    assert_eq!(compatible.as_ref(), body);
}

#[test]
fn candidate_v16_openai_reasoning_strict_retains_existing_summary_and_store_ids() {
    for summary in [
        json!([{"type":"summary_text","text":"existing"}]),
        json!("author summary"),
    ] {
        let body = serde_json::to_vec(&json!({"store":true,"input":[{
            "type":"reasoning","id":"rs_stored","summary":summary,
            "content":[{"type":"reasoning_text","text":"replacement"}],
            "encrypted_content":"opaque-encrypted-reasoning-token-xyz"
        }]}))
        .unwrap();
        let output: Value = serde_json::from_slice(
            &sanitize_openai_responses_reasoning_encrypted_content("test", &body),
        )
        .unwrap();
        assert_eq!(output["input"][0]["summary"], summary);
        assert_eq!(output["input"][0]["content"], json!([]));
        assert_eq!(output["input"][0]["id"], "rs_stored");
        assert!(output["input"][0].get("encrypted_content").is_none());
    }
}
