use super::openrouter_chat;
use super::ResponsesTransportKind;

const DEFAULT_MODEL: &str = "tencent/hy3-preview:free";

pub fn adapter_id() -> &'static str {
    "hy3"
}

pub fn transport_kind() -> ResponsesTransportKind {
    ResponsesTransportKind::ChatCompletions
}

pub fn upstream_path() -> &'static str {
    "/v1/chat/completions"
}

pub fn compact_instructions() -> &'static str {
    openrouter_chat::COMPACT_INSTRUCTIONS
}

pub fn reasoning_effort_override() -> Option<&'static str> {
    None
}

pub fn unified_exec_enabled() -> bool {
    false
}

pub fn uses_ctox_web_stack() -> bool {
    false
}

pub fn compact_limit(_model: &str, realized_context: usize) -> usize {
    ((realized_context as f64) * 3.0 / 4.0).round() as usize
}

pub fn runtime_tuning(
    _preset: crate::inference::runtime_plan::ChatPreset,
    _max_output_tokens: u32,
) -> crate::inference::runtime_state::AdapterRuntimeTuning {
    crate::inference::runtime_state::AdapterRuntimeTuning::default()
}

pub fn rewrite_request(raw: &[u8]) -> anyhow::Result<Vec<u8>> {
    openrouter_chat::rewrite_request(raw, DEFAULT_MODEL, "HY3")
}

pub fn rewrite_success_response(
    raw: &[u8],
    fallback_model: Option<&str>,
    exact_text_override: Option<&str>,
) -> anyhow::Result<Vec<u8>> {
    openrouter_chat::rewrite_success_response(
        raw,
        fallback_model,
        exact_text_override,
        DEFAULT_MODEL,
        "HY3",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use serde_json::Value;

    #[test]
    fn openrouter_hy3_uses_normalized_chat_completions_request() {
        let raw = json!({
            "model": "tencent/hy3-preview:free",
            "instructions": "Use tools.",
            "input": [{"type": "message", "role": "user", "content": "hi"}],
            "max_output_tokens": 512,
            "reasoning": {"effort": "low", "exclude": true},
            "tools": [{
                "type": "function",
                "function": {
                    "name": "exec_command",
                    "parameters": {"type": "object"}
                }
            }]
        });

        let rewritten = rewrite_request(raw.to_string().as_bytes()).expect("rewrite request");
        let payload: Value =
            serde_json::from_slice(&rewritten).expect("rewritten request should be JSON");

        assert_eq!(
            payload.get("model"),
            Some(&json!("tencent/hy3-preview:free"))
        );
        assert_eq!(payload.get("max_tokens"), Some(&json!(512)));
        assert_eq!(
            payload.get("reasoning"),
            Some(&json!({"effort": "low", "exclude": true}))
        );
        assert!(payload.get("enable_thinking").is_none());
        assert!(payload
            .get("tools")
            .and_then(Value::as_array)
            .is_some_and(|tools| !tools.is_empty()));
    }

    #[test]
    fn openrouter_hy3_rewrites_tool_calls_from_chat_response() {
        let raw = json!({
            "id": "gen-hy3",
            "model": "tencent/hy3-preview:free",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "exec_command",
                            "arguments": "{\"cmd\":\"pwd\"}"
                        }
                    }]
                }
            }]
        });

        let rewritten = rewrite_success_response(raw.to_string().as_bytes(), None, None)
            .expect("rewrite response");
        let payload: Value =
            serde_json::from_slice(&rewritten).expect("rewritten response should be JSON");

        let output = payload.to_string();
        assert!(output.contains("call_1"));
        assert!(output.contains("exec_command"));
    }
}
