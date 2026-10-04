// ref: internal/runtime/executor/codex_executor_tokens.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use serde::Deserialize;
use serde_json::value::RawValue;
use std::fmt;
use tiktoken_rs::{cl100k_base_singleton, o200k_base_singleton, CoreBPE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexTokenCountError {
    InvalidJson,
}

impl fmt::Display for CodexTokenCountError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Codex token-count request is invalid JSON")
    }
}

impl std::error::Error for CodexTokenCountError {}

#[derive(Deserialize)]
struct CountRoot<'a> {
    #[serde(borrow)]
    instructions: Option<&'a RawValue>,
    #[serde(borrow, default)]
    input: Vec<&'a RawValue>,
    #[serde(borrow, default)]
    tools: Vec<&'a RawValue>,
    #[serde(borrow)]
    text: Option<CountText<'a>>,
}

#[derive(Deserialize)]
struct CountText<'a> {
    #[serde(borrow)]
    format: Option<&'a RawValue>,
}

/// Counts the same semantic Codex request segments as upstream while keeping
/// JSON-valued arguments and schemas in their original lexical order.
pub fn count_codex_input_tokens(model: &str, body: &[u8]) -> Result<i64, CodexTokenCountError> {
    count_codex_input_tokens_with_encoder(tokenizer_for_codex_model(model), body)
}

/// Meta always uses upstream O200kBase, independently of its model identifier.
/// ref: internal/runtime/executor/meta_executor_execute.go:247-272 @ d7914afdedca7af95ee974a42453dc49fc1388ce
pub fn count_meta_input_tokens(body: &[u8]) -> Result<i64, CodexTokenCountError> {
    count_codex_input_tokens_with_encoder(o200k_base_singleton(), body)
}

fn count_codex_input_tokens_with_encoder(
    encoder: &CoreBPE,
    body: &[u8],
) -> Result<i64, CodexTokenCountError> {
    if body.is_empty() {
        return Ok(0);
    }
    let root: CountRoot<'_> =
        serde_json::from_slice(body).map_err(|_| CodexTokenCountError::InvalidJson)?;
    let mut segments = Vec::new();
    push_raw_text(&mut segments, root.instructions);
    for raw in root.input {
        let value: serde_json::Value =
            serde_json::from_str(raw.get()).map_err(|_| CodexTokenCountError::InvalidJson)?;
        match value
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
        {
            "message" => {
                if let Some(content) = value.get("content").and_then(serde_json::Value::as_array) {
                    for part in content {
                        push_trimmed(
                            &mut segments,
                            part.get("text").and_then(serde_json::Value::as_str),
                        );
                    }
                }
            }
            "function_call" => {
                push_trimmed(
                    &mut segments,
                    value.get("name").and_then(serde_json::Value::as_str),
                );
                push_raw_field(&mut segments, raw.get(), "arguments")?;
            }
            "function_call_output" => push_raw_field(&mut segments, raw.get(), "output")?,
            _ => push_trimmed(
                &mut segments,
                value.get("text").and_then(serde_json::Value::as_str),
            ),
        }
    }
    for raw in root.tools {
        let value: serde_json::Value =
            serde_json::from_str(raw.get()).map_err(|_| CodexTokenCountError::InvalidJson)?;
        push_trimmed(
            &mut segments,
            value.get("name").and_then(serde_json::Value::as_str),
        );
        push_trimmed(
            &mut segments,
            value.get("description").and_then(serde_json::Value::as_str),
        );
        push_raw_field(&mut segments, raw.get(), "parameters")?;
    }
    if let Some(format) = root.text.and_then(|text| text.format) {
        let value: serde_json::Value =
            serde_json::from_str(format.get()).map_err(|_| CodexTokenCountError::InvalidJson)?;
        push_trimmed(
            &mut segments,
            value.get("name").and_then(serde_json::Value::as_str),
        );
        push_raw_field(&mut segments, format.get(), "schema")?;
    }
    let joined = segments.join("\n");
    if joined.is_empty() {
        return Ok(0);
    }
    Ok(encoder.count_ordinary(&joined) as i64)
}

/// Builds the internal Responses-shaped usage payload returned by upstream's
/// `CodexExecutor.CountTokens` before any caller-specific format translation.
pub fn codex_token_count_response(
    model: &str,
    body: &[u8],
) -> Result<Vec<u8>, CodexTokenCountError> {
    let count = count_codex_input_tokens(model, body)?;
    serde_json::to_vec(&serde_json::json!({
        "response": {
            "usage": {
                "input_tokens": count,
                "output_tokens": 0,
                "total_tokens": count
            }
        }
    }))
    .map_err(|_| CodexTokenCountError::InvalidJson)
}

fn tokenizer_for_codex_model(model: &str) -> &'static CoreBPE {
    let model = model.trim().to_ascii_lowercase();
    if model.starts_with("gpt-5") || model.starts_with("gpt-4.1") || model.starts_with("gpt-4o") {
        o200k_base_singleton()
    } else {
        cl100k_base_singleton()
    }
}

fn push_trimmed(segments: &mut Vec<String>, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        segments.push(value.to_owned());
    }
}

fn push_raw_text(segments: &mut Vec<String>, value: Option<&RawValue>) {
    let Some(value) = value else { return };
    if let Ok(text) = serde_json::from_str::<String>(value.get()) {
        push_trimmed(segments, Some(&text));
    }
}

fn push_raw_field(
    segments: &mut Vec<String>,
    object: &str,
    field: &str,
) -> Result<(), CodexTokenCountError> {
    #[derive(Deserialize)]
    struct RawFields<'a> {
        #[serde(borrow)]
        arguments: Option<&'a RawValue>,
        #[serde(borrow)]
        output: Option<&'a RawValue>,
        #[serde(borrow)]
        parameters: Option<&'a RawValue>,
        #[serde(borrow)]
        schema: Option<&'a RawValue>,
    }
    let fields: RawFields<'_> =
        serde_json::from_str(object).map_err(|_| CodexTokenCountError::InvalidJson)?;
    let value = match field {
        "arguments" => fields.arguments,
        "output" => fields.output,
        "parameters" => fields.parameters,
        "schema" => fields.schema,
        _ => None,
    };
    let Some(value) = value else {
        return Ok(());
    };
    if let Ok(text) = serde_json::from_str::<String>(value.get()) {
        push_trimmed(segments, Some(&text));
    } else {
        push_trimmed(segments, Some(value.get()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_semantic_segments_match_upstream_shape() {
        assert_eq!(count_codex_input_tokens("gpt-5", b"{}").unwrap(), 0);
        let count = count_codex_input_tokens(
            "gpt-5",
            br#"{"instructions":"be brief","input":[{"type":"message","content":[{"text":"hello"}]},{"type":"function_call","name":"lookup","arguments":"{\"q\":\"x\"}"},{"type":"function_call_output","output":{"ok":true}}],"tools":[{"name":"lookup","description":"Find data","parameters":{"type":"object","properties":{"q":{"type":"string"}}}}],"text":{"format":{"name":"answer","schema":{"type":"object"}}}}"#,
        )
        .unwrap();
        assert!(count > 20);
    }

    #[test]
    fn model_families_select_distinct_upstream_encodings() {
        let body = r#"{"instructions":"こんにちは世界","input":[]}"#.as_bytes();
        let modern = count_codex_input_tokens("gpt-5-codex", body).unwrap();
        let legacy = count_codex_input_tokens("gpt-4", body).unwrap();
        assert_ne!(modern, legacy);
        assert_eq!(count_codex_input_tokens("unknown", body).unwrap(), legacy);
    }

    #[test]
    fn candidate_meta_tokens_always_use_o200k_and_semantic_codex_segments() {
        let body = r#"{"instructions":"こんにちは世界","input":[{"type":"message","content":[{"type":"input_text","text":"hello"}]},{"type":"function_call","name":"lookup","arguments":{"q":"世界"}}],"tools":[{"name":"lookup","description":"lookup data","parameters":{"type":"object","properties":{"q":{"type":"string"}}}}]}"#.as_bytes();
        let count = count_meta_input_tokens(body).unwrap();
        assert_eq!(count, count_codex_input_tokens("gpt-5", body).unwrap());
        let short = r#"{"instructions":"こんにちは世界","input":[]}"#.as_bytes();
        assert_ne!(
            count_meta_input_tokens(short).unwrap(),
            count_codex_input_tokens("muse-spark-1.3", short).unwrap()
        );
        assert_eq!(count_meta_input_tokens(b"{}").unwrap(), 0);
        assert_eq!(
            count_meta_input_tokens(b"invalid"),
            Err(CodexTokenCountError::InvalidJson)
        );
    }

    #[test]
    fn response_uses_internal_responses_usage_contract() {
        let payload =
            codex_token_count_response("gpt-5", br#"{"instructions":"hello","input":[]}"#).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        let count = value["response"]["usage"]["input_tokens"].as_i64().unwrap();
        assert!(count > 0);
        assert_eq!(value["response"]["usage"]["output_tokens"], 0);
        assert_eq!(value["response"]["usage"]["total_tokens"], count);
    }
}
