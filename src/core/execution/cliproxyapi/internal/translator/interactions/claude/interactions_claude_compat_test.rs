// ref: internal/translator/interactions/claude/interactions_claude_compat_test.go @ e2bff0107bb307337aaa19018ccddd55f64253d5
// Port-Status: candidate; execution pending
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{
    convert_claude_request_to_interactions, convert_claude_request_to_interactions_with_compat,
};
use serde_json::{json, Value};

#[test]
fn compatibility_retains_empty_null_and_missing_thought_steps() {
    for thinking in [None, Some(Value::Null), Some(json!(""))] {
        let mut part = json!({"type":"thinking", "signature":""});
        if let Some(thinking) = thinking {
            part["thinking"] = thinking;
        }
        let request =
            serde_json::to_vec(&json!({"messages":[{"role":"assistant", "content":[part]}]}))
                .unwrap();
        for stream in [false, true] {
            let strict: Value = serde_json::from_slice(&convert_claude_request_to_interactions(
                "deepseek-v4",
                &request,
                stream,
            ))
            .unwrap();
            assert_eq!(strict["input"], json!([]));
            let compatible: Value =
                serde_json::from_slice(&convert_claude_request_to_interactions_with_compat(
                    "deepseek-v4",
                    &request,
                    stream,
                ))
                .unwrap();
            assert_eq!(
                compatible["input"],
                json!([{"type":"thought", "content":[{"type":"text", "text":""}]}])
            );
        }
    }
}

#[test]
fn compatibility_preserves_text_thought_and_function_call_order() {
    let request = br#"{"messages":[{"role":"assistant","content":[{"type":"text","text":"before"},{"type":"thinking","thinking":"","signature":""},{"type":"text","text":"after"},{"type":"tool_use","id":"call_1","name":"Read","input":{}}]}]}"#;
    let output: Value = serde_json::from_slice(
        &convert_claude_request_to_interactions_with_compat("deepseek-v4", request, false),
    )
    .unwrap();
    assert_eq!(output["input"].as_array().unwrap().len(), 4);
    assert_eq!(output["input"][0]["content"][0]["text"], "before");
    assert_eq!(output["input"][1]["type"], "thought");
    assert_eq!(output["input"][2]["content"][0]["text"], "after");
    assert_eq!(output["input"][3]["type"], "function_call");
}

#[test]
fn compatibility_preserves_explicit_stream_false_and_matches_normal_nonempty_thinking() {
    let request = br#"{"stream":false,"messages":[{"role":"assistant","content":[{"type":"thinking","thinking":"reason","signature":""}]}]}"#;
    let strict = convert_claude_request_to_interactions("deepseek-v4", request, true);
    let compatible =
        convert_claude_request_to_interactions_with_compat("deepseek-v4", request, true);
    assert_eq!(compatible, strict);
    let output: Value = serde_json::from_slice(&compatible).unwrap();
    assert_eq!(output["stream"], false);
    assert_eq!(output["input"][0]["content"][0]["text"], "reason");
}
