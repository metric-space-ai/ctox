// ref: internal/translator/gemini/claude/gemini_claude_compat_test.go @ e2bff0107bb307337aaa19018ccddd55f64253d5
// Port-Status: candidate_v8
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::{json, Value};

use super::{convert_claude_request_to_gemini, convert_claude_request_to_gemini_with_compat};

const GEMINI_SIGNATURE: &str =
    "EjQKMgEMOdbHO0Gd+c9Mxk4ELwPGbpCEcp2mFfYYLix2UVtBH3fL8GECc4+JITVnHF4qZDsA";
const BYPASS: &str = "skip_thought_signature_validator";

fn converted(input: &Value, compat: bool) -> Value {
    let bytes = serde_json::to_vec(input).unwrap();
    serde_json::from_slice(&if compat {
        convert_claude_request_to_gemini_with_compat("gemini-3-flash", &bytes, false)
    } else {
        convert_claude_request_to_gemini("gemini-3-flash", &bytes, false)
    })
    .unwrap()
}

#[test]
fn standard_drops_thinking_while_compat_preserves_unsigned_thought() {
    let input = json!({"messages":[{"role":"assistant","content":[
        {"type":"thinking","thinking":"reason","signature":""}
    ]}]});
    assert_eq!(converted(&input, false)["contents"][0]["parts"], json!([]));
    assert_eq!(
        converted(&input, true)["contents"][0]["parts"][0],
        json!({"text":"reason","thought":true,"thoughtSignature":BYPASS})
    );
}

#[test]
fn compat_normalizes_gemini_and_bypasses_foreign_or_missing_signatures() {
    for (raw, expected) in [
        (GEMINI_SIGNATURE.to_owned(), GEMINI_SIGNATURE),
        (format!("gemini#{GEMINI_SIGNATURE}"), GEMINI_SIGNATURE),
        ("claude#opaque-signature-12345".to_owned(), BYPASS),
        (String::new(), BYPASS),
    ] {
        let input = json!({"messages":[{"role":"assistant","content":[
            {"type":"thinking","thinking":"reason","signature":raw}
        ]}]});
        assert_eq!(
            converted(&input, true)["contents"][0]["parts"][0]["thoughtSignature"],
            expected
        );
    }
}

#[test]
fn compat_retains_empty_null_missing_and_user_thought_blocks() {
    for role in ["assistant", "user"] {
        for thinking in [
            json!({"type":"thinking"}),
            json!({"type":"thinking","thinking":null}),
            json!({"type":"thinking","thinking":""}),
        ] {
            let input = json!({"messages":[{"role":role,"content":[thinking]}]});
            assert_eq!(
                converted(&input, true)["contents"][0]["parts"][0],
                json!({"text":"","thought":true,"thoughtSignature":BYPASS})
            );
            assert_eq!(converted(&input, false)["contents"][0]["parts"], json!([]));
        }
    }
}

#[test]
fn direct_gemini_keeps_thought_position_with_media_and_tools() {
    let input = json!({"messages":[
        {"role":"assistant","content":[
            {"type":"text","text":"before"},
            {"type":"thinking","thinking":"reason","signature":""},
            {"type":"text","text":"after"},
            {"type":"tool_use","id":"call-1","name":"inspect","input":{"ok":true}}
        ]},
        {"role":"user","content":[
            {"type":"tool_result","tool_use_id":"call-1","content":"done"},
            {"type":"image","source":{"type":"base64","media_type":"image/png","data":"aGVsbG8="}}
        ]}
    ]});
    let output = converted(&input, true);
    let parts = output["contents"][0]["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 4);
    assert_eq!(parts[0]["text"], "before");
    assert_eq!(parts[1]["thought"], true);
    assert_eq!(parts[1]["text"], "reason");
    assert_eq!(parts[2]["text"], "after");
    assert_eq!(parts[3]["functionCall"]["name"], "inspect");
    assert_eq!(
        output["contents"][1]["parts"][0]["functionResponse"]["name"],
        "inspect"
    );
    assert_eq!(
        output["contents"][1]["parts"][1]["inline_data"]["mime_type"],
        "image/png"
    );
    assert_eq!(
        output["contents"][1]["parts"][1]["inline_data"]["data"],
        "aGVsbG8="
    );
}

#[test]
fn both_direct_facades_preserve_invalid_and_non_object_bytes() {
    for input in [b"not-json".as_slice(), br#"["not-an-object"]"#, b"null"] {
        for stream in [false, true] {
            assert_eq!(
                convert_claude_request_to_gemini("gemini-3-flash", input, stream),
                input
            );
            assert_eq!(
                convert_claude_request_to_gemini_with_compat("gemini-3-flash", input, stream),
                input
            );
        }
    }
}
