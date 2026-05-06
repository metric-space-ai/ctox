use super::openrouter_chat;
use super::ResponsesTransportKind;

const DEFAULT_MODEL: &str = "deepseek/deepseek-v4-flash";

pub fn adapter_id() -> &'static str {
    "deepseek"
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
    openrouter_chat::rewrite_request(raw, DEFAULT_MODEL, "DeepSeek")
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
        "DeepSeek",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use serde_json::Value;

    #[test]
    fn openrouter_deepseek_keeps_reasoning_parameter_without_thinking_flag() {
        let raw = json!({
            "model": "deepseek/deepseek-v4-flash",
            "instructions": "Use tools.",
            "input": [{"type": "message", "role": "user", "content": "hi"}],
            "reasoning": {"effort": "high"}
        });

        let rewritten = rewrite_request(raw.to_string().as_bytes()).expect("rewrite request");
        let payload: Value =
            serde_json::from_slice(&rewritten).expect("rewritten request should be JSON");

        assert_eq!(payload.get("reasoning"), Some(&json!({"effort": "high"})));
        assert!(payload.get("enable_thinking").is_none());
    }
}
