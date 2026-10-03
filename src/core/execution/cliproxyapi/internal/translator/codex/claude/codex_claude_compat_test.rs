// ref: internal/translator/codex/claude/codex_claude_compat_test.go @ e2bff0107bb307337aaa19018ccddd55f64253d5
// Port-Status: candidate; execution pending
// License: MIT (upstream); modifications AGPL-3.0-only

use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};

use super::{convert_claude_request_to_codex, convert_claude_request_to_codex_with_compat};

fn translated(signature: Option<Value>, compat: bool, stream: bool) -> Value {
    let mut part = json!({"type":"thinking", "thinking":"private-reasoning"});
    if let Some(signature) = signature {
        part["signature"] = signature;
    }
    let request = serde_json::to_vec(&json!({
        "messages":[{"role":"assistant", "content":[part]}]
    })).unwrap();
    let bytes = if compat {
        convert_claude_request_to_codex_with_compat("deepseek-v4", &request, stream)
    } else {
        convert_claude_request_to_codex("deepseek-v4", &request, stream)
    };
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn compatibility_preserves_empty_missing_null_and_whitespace_signatures() {
    for stream in [false, true] {
        for (signature, expected) in [
            (None, ""), (Some(Value::Null), ""),
            (Some(json!("")), ""), (Some(json!("   ")), "   "),
        ] {
            let strict = translated(signature.clone(), false, stream);
            assert_eq!(strict["input"], json!([]));
            let compatible = translated(signature, true, stream);
            assert_eq!(compatible["input"][0], json!({
                "type":"reasoning", "summary":[], "content":null,
                "encrypted_content":expected,
            }));
            assert!(!compatible.to_string().contains("private-reasoning"));
        }
    }
}

#[test]
fn unknown_escaped_signature_preserves_message_and_tool_order() {
    let signature = "enc:\"token\"\\with\\unicode-一";
    let request = serde_json::to_vec(&json!({
        "messages":[{"role":"assistant", "content":[
            {"type":"text", "text":"before"},
            {"type":"thinking", "thinking":"private-reasoning", "signature":signature},
            {"type":"text", "text":"after"},
            {"type":"tool_use", "id":"call-1", "name":"lookup", "input":{"q":"x"}},
        ]}]
    })).unwrap();
    for stream in [false, true] {
        let compatible: Value = serde_json::from_slice(
            &convert_claude_request_to_codex_with_compat("deepseek-v4", &request, stream)
        ).unwrap();
        assert_eq!(compatible["input"].as_array().unwrap().len(), 4);
        assert_eq!(compatible["input"][0]["content"][0]["text"], "before");
        assert_eq!(compatible["input"][1]["type"], "reasoning");
        assert_eq!(compatible["input"][1]["encrypted_content"], signature);
        assert_eq!(compatible["input"][2]["content"][0]["text"], "after");
        assert_eq!(compatible["input"][3]["type"], "function_call");
        let strict: Value = serde_json::from_slice(
            &convert_claude_request_to_codex("deepseek-v4", &request, stream)
        ).unwrap();
        assert_eq!(strict["input"].as_array().unwrap().len(), 2);
        assert_eq!(strict["input"][0]["content"].as_array().unwrap().len(), 2);
    }
}

#[test]
fn compatibility_rejects_non_string_nonempty_signatures_and_known_foreign_provider() {
    for stream in [false, true] {
        for signature in [json!(12345), json!(true), json!(["arr"]), json!({"opaque":"data"}),
            json!("skip_thought_signature_validator")] {
            assert_eq!(translated(Some(signature), true, stream)["input"], json!([]));
        }
    }
}

#[test]
fn compatibility_still_normalizes_valid_gpt_provider_prefix() {
    let mut payload = vec![0_u8; 1 + 8 + 16 + 16 + 32];
    payload[0] = 0x80;
    payload[8] = 1;
    for (index, byte) in payload.iter_mut().enumerate().skip(9) {
        *byte = index as u8;
    }
    let raw = general_purpose::URL_SAFE.encode(payload);
    for stream in [false, true] {
        assert_eq!(translated(Some(json!(format!("gpt#{raw}"))), true, stream)
            ["input"][0]["encrypted_content"], raw);
    }
}

#[test]
fn compatibility_no_op_keeps_invalid_or_non_object_bytes_identical() {
    for payload in [b"  {bad-json}\n".as_slice(), b" [ 1, 2 ]\n".as_slice()] {
        for stream in [false, true] {
            assert_eq!(convert_claude_request_to_codex_with_compat("deepseek-v4", payload, stream), payload);
            assert_eq!(convert_claude_request_to_codex("deepseek-v4", payload, stream), payload);
        }
    }
}
