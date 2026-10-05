// ref: internal/translator/common/claude_native_response.go:1-81 @ 16d98881d4bb37adaa827599e4be8f5154e81646
// Port-Status: candidate
// License: MIT (upstream); modifications AGPL-3.0-only

use super::set_raw_path;
use serde_json::{json, value::RawValue, Value};
use std::borrow::Cow;

/// Adapt a complete native Messages response for the existing SSE aggregators.
/// Other payloads retain their borrowed bytes, including CRLF and invalid JSON.
pub fn claude_messages_json_to_sse(raw: &[u8]) -> (Cow<'_, [u8]>, String) {
    let Ok(document) = std::str::from_utf8(raw) else {
        return (Cow::Borrowed(raw), String::new());
    };
    if serde_json::from_slice::<&RawValue>(raw).is_err()
        || gjson::get(document, "type").str() != "message"
    {
        return (Cow::Borrowed(raw), String::new());
    }
    let content = gjson::get(document, "content");
    let Ok(blocks) = serde_json::from_str::<Vec<&RawValue>>(content.json()) else {
        return (Cow::Borrowed(raw), String::new());
    };
    let model = gjson::get(document, "model").str().to_owned();
    let mut output = Vec::new();
    let message = set_raw_path(raw, "content", b"[]");
    let message = set_raw_path(&message, "stop_reason", b"null");
    let message = set_raw_path(&message, "stop_sequence", b"null");
    emit_raw(
        &mut output,
        b"{\"type\":\"message_start\",\"message\":",
        &message,
        b"}",
    );
    for (index, block) in blocks.into_iter().enumerate() {
        let raw_block = block.get();
        let kind = gjson::get(raw_block, "type").str().to_owned();
        let mut start = raw_block.as_bytes().to_vec();
        let delta = match kind.as_str() {
            "text" => {
                start = set_raw_path(&start, "text", b"\"\"");
                Some(json!({"type":"text_delta", "text":gjson::get(raw_block, "text").str()}))
            }
            "tool_use" => {
                start = set_raw_path(&start, "input", b"{}");
                let input = gjson::get(raw_block, "input");
                let input = if input.exists() { input.json() } else { "{}" };
                Some(json!({"type":"input_json_delta", "partial_json":input}))
            }
            "thinking" => {
                start = set_raw_path(&start, "thinking", b"\"\"");
                start = set_raw_path(&start, "signature", b"\"\"");
                Some(
                    json!({"type":"thinking_delta", "thinking":gjson::get(raw_block, "thinking").str()}),
                )
            }
            _ => None,
        };
        emit_raw(
            &mut output,
            format!("{{\"type\":\"content_block_start\",\"index\":{index},\"content_block\":")
                .as_bytes(),
            &start,
            b"}",
        );
        if let Some(delta) = delta {
            emit_value(
                &mut output,
                &json!({"type":"content_block_delta","index":index,"delta":delta}),
            );
        }
        if kind == "text" {
            let citations = gjson::get(raw_block, "citations");
            if let Ok(citations) = serde_json::from_str::<Vec<&RawValue>>(citations.json()) {
                for citation in citations {
                    emit_raw(&mut output, format!("{{\"type\":\"content_block_delta\",\"index\":{index},\"delta\":{{\"type\":\"citations_delta\",\"citation\":").as_bytes(), citation.get().as_bytes(), b"}}");
                }
            }
        }
        let signature = gjson::get(raw_block, "signature");
        if kind == "thinking" && signature.exists() {
            emit_value(
                &mut output,
                &json!({"type":"content_block_delta","index":index,
                "delta":{"type":"signature_delta","signature":signature.str()}}),
            );
        }
        emit_value(
            &mut output,
            &json!({"type":"content_block_stop","index":index}),
        );
    }
    let stop_reason = gjson::get(document, "stop_reason");
    let stop_sequence = gjson::get(document, "stop_sequence");
    let usage = gjson::get(document, "usage");
    let terminal = format!("{{\"type\":\"message_delta\",\"delta\":{{\"stop_reason\":{},\"stop_sequence\":{}}},\"usage\":{}}}",
        if stop_reason.exists() { stop_reason.json() } else { "null" },
        if stop_sequence.exists() { stop_sequence.json() } else { "null" },
        if usage.exists() { usage.json() } else { "{}" });
    emit_raw(&mut output, b"", terminal.as_bytes(), b"");
    emit_value(&mut output, &json!({"type":"message_stop"}));
    (Cow::Owned(output), model)
}

fn emit_raw(output: &mut Vec<u8>, prefix: &[u8], raw: &[u8], suffix: &[u8]) {
    output.extend_from_slice(b"data: ");
    output.extend_from_slice(prefix);
    output.extend_from_slice(raw);
    output.extend_from_slice(suffix);
    output.extend_from_slice(b"\n\n");
}

fn emit_value(output: &mut Vec<u8>, event: &Value) {
    let raw = serde_json::to_vec(event).expect("in-memory JSON events serialize");
    emit_raw(output, b"", &raw, b"");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::translator::claude::openai::chat_completions::convert_claude_response_to_openai_chat_non_stream as chat;
    use crate::internal::translator::claude::openai::responses::convert_claude_response_to_openai_responses_non_stream as responses;

    fn events(raw: &[u8]) -> Vec<Value> {
        raw.split(|byte| *byte == b'\n')
            .filter_map(|line| {
                line.strip_prefix(b"data: ")
                    .map(|data| serde_json::from_slice(data).unwrap())
            })
            .collect()
    }

    fn native(content: Value, reason: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({"id":"msg_native","type":"message","role":"assistant",
            "model":"claude-native","content":content,"stop_reason":reason,"stop_sequence":null,
            "usage":{"input_tokens":13,"cache_read_input_tokens":7,"cache_creation_input_tokens":3,"output_tokens":5}})).unwrap()
    }

    #[test]
    fn candidate_claude_native_json_passthrough_is_borrowed_and_identical() {
        for raw in [
            b"event: message_start\r\ndata: {\"type\":\"message_start\"}\r\n\r\n: keepalive\r\n"
                .as_slice(),
            b"data: {\"type\":\"message_stop\"}\n\n",
            br#"{"type":"error","error":{"message":"bad request"}}"#,
            br#"{"type":"message","content":"wrong shape"}"#,
            br#"{"type":"message","content":"#,
            b"",
        ] {
            let (output, model) = claude_messages_json_to_sse(raw);
            assert!(matches!(output, Cow::Borrowed(_)));
            assert_eq!(output.as_ptr(), raw.as_ptr());
            assert_eq!(output.as_ref(), raw);
            assert!(model.is_empty());
        }
    }

    #[test]
    fn candidate_claude_native_json_emits_ordered_blocks_and_raw_tool_input() {
        let raw = br#"{"id":"msg_native","type":"message","role":"assistant","model":"claude-native",
            "content":[{"type":"text","text":"Hello"},
            {"type":"tool_use","id":"nested","name":"lookup","input":{"n":2e+09,"n":7,"items":[true,null,2]}},
            {"type":"tool_use","id":"empty","name":"clock","input":{}},
            {"type":"thinking","thinking":"plan","signature":"sig"},
            {"type":"redacted_thinking","data":"opaque"}],
            "stop_reason":"stop_sequence","stop_sequence":"END",
            "unknown":9007199254740993,"usage":{"input_tokens":13,"output_tokens":5}}"#;
        let (output, model) = claude_messages_json_to_sse(raw);
        let events = events(&output);
        assert_eq!(model, "claude-native");
        assert_eq!(
            events
                .iter()
                .map(|event| event["type"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec![
                "message_start",
                "content_block_start",
                "content_block_delta",
                "content_block_stop",
                "content_block_start",
                "content_block_delta",
                "content_block_stop",
                "content_block_start",
                "content_block_delta",
                "content_block_stop",
                "content_block_start",
                "content_block_delta",
                "content_block_delta",
                "content_block_stop",
                "content_block_start",
                "content_block_stop",
                "message_delta",
                "message_stop"
            ]
        );
        assert_eq!(events[0]["message"]["content"], json!([]));
        assert_eq!(
            events[0]["message"]["unknown"].as_u64(),
            Some(9007199254740993)
        );
        assert_eq!(
            events[5]["delta"]["partial_json"],
            r#"{"n":2e+09,"n":7,"items":[true,null,2]}"#
        );
        assert_eq!(events[8]["delta"]["partial_json"], "{}");
        assert_eq!(events[12]["delta"]["signature"], "sig");
        assert_eq!(events[14]["content_block"]["data"], "opaque");
        assert_eq!(events[16]["delta"]["stop_sequence"], "END");
        assert_eq!(
            events[16]["usage"],
            json!({"input_tokens":13,"output_tokens":5})
        );
    }

    #[test]
    fn candidate_claude_native_json_keeps_citations_in_adapter_and_response() {
        let citation = json!({"type":"web_search_result_location","url":"https://example.invalid",
            "title":"Source","cited_text":"Answer.","encrypted_index":"IDX"});
        let raw = native(
            json!([{"type":"text","text":"Answer.","citations":[citation.clone()]}]),
            "end_turn",
        );
        let (adapted, _) = claude_messages_json_to_sse(&raw);
        let events = events(&adapted);
        assert_eq!(events[3]["delta"]["type"], "citations_delta");
        assert_eq!(events[3]["delta"]["citation"], citation);
        assert_eq!(events[3]["index"], 0);
        let output: Value = serde_json::from_slice(&responses(b"{}", b"{}", &raw)).unwrap();
        assert_eq!(
            output["output"][0]["content"][0]["annotations"][0]["url"],
            "https://example.invalid"
        );
    }

    #[test]
    fn candidate_claude_native_json_chat_preserves_usage_and_terminal_reason() {
        for (reason, expected) in [
            ("end_turn", "stop"),
            ("tool_use", "tool_calls"),
            ("max_tokens", "length"),
        ] {
            let mut blocks = vec![
                json!({"type":"text","text":"Hello "}),
                json!({"type":"text","text":"world!"}),
            ];
            if reason == "tool_use" {
                blocks.extend([json!({"type":"tool_use","id":"weather","name":"weather","input":{"city":"Paris","days":2}}),
                    json!({"type":"tool_use","id":"clock","name":"clock","input":{}})]);
            }
            let raw = native(Value::Array(blocks), reason);
            let output: Value = serde_json::from_slice(&chat(b"{}", b"{}", &raw)).unwrap();
            assert_eq!(output["id"], "msg_native");
            assert_eq!(output["model"], "claude-native");
            assert_eq!(output["choices"][0]["message"]["content"], "Hello world!");
            assert_eq!(output["choices"][0]["finish_reason"], expected);
            assert_eq!(output["usage"]["prompt_tokens"], 23);
            assert_eq!(output["usage"]["completion_tokens"], 5);
            assert_eq!(output["usage"]["total_tokens"], 28);
            assert_eq!(output["usage"]["prompt_tokens_details"]["cached_tokens"], 7);
            assert_eq!(
                output["usage"]["prompt_tokens_details"]["cached_creation_tokens"],
                3
            );
            assert_eq!(
                output["usage"]["prompt_tokens_details"]["cache_write_tokens"],
                3
            );
            if reason == "tool_use" {
                let calls = &output["choices"][0]["message"]["tool_calls"];
                assert_eq!(calls.as_array().unwrap().len(), 2);
                assert_eq!(calls[0]["id"], "weather");
                assert_eq!(
                    serde_json::from_str::<Value>(
                        calls[0]["function"]["arguments"].as_str().unwrap()
                    )
                    .unwrap(),
                    json!({"city":"Paris","days":2})
                );
                assert_eq!(
                    serde_json::from_str::<Value>(
                        calls[1]["function"]["arguments"].as_str().unwrap()
                    )
                    .unwrap(),
                    json!({})
                );
            }
        }
    }

    #[test]
    fn candidate_claude_native_json_responses_preserves_merged_text_tools_and_limits() {
        for (reason, expected) in [
            ("end_turn", "completed"),
            ("tool_use", "completed"),
            ("max_tokens", "incomplete"),
        ] {
            let mut blocks = vec![
                json!({"type":"text","text":"Hello "}),
                json!({"type":"text","text":"world!"}),
            ];
            if reason == "tool_use" {
                blocks.extend([json!({"type":"tool_use","id":"weather","name":"weather","input":{"city":"Paris","days":2}}),
                    json!({"type":"tool_use","id":"clock","name":"clock","input":{}})]);
            }
            let raw = native(Value::Array(blocks), reason);
            let output: Value = serde_json::from_slice(&responses(b"{}", b"{}", &raw)).unwrap();
            assert_eq!(output["id"], "msg_native");
            assert_eq!(output["model"], "claude-native");
            assert_eq!(output["status"], expected);
            assert_eq!(output["output"][0]["status"], expected);
            assert_eq!(output["output"][0]["content"][0]["text"], "Hello world!");
            assert_eq!(output["usage"]["input_tokens"], 23);
            assert_eq!(output["usage"]["output_tokens"], 5);
            assert_eq!(output["usage"]["total_tokens"], 28);
            assert_eq!(output["usage"]["input_tokens_details"]["cached_tokens"], 7);
            if reason == "max_tokens" {
                assert_eq!(output["incomplete_details"]["reason"], "max_output_tokens");
            } else {
                assert!(output["incomplete_details"].is_null());
            }
            assert_eq!(
                output["output"].as_array().unwrap().len(),
                if reason == "tool_use" { 3 } else { 1 }
            );
            if reason == "tool_use" {
                assert_eq!(output["output"][1]["call_id"], "weather");
                assert_eq!(
                    serde_json::from_str::<Value>(
                        output["output"][1]["arguments"].as_str().unwrap()
                    )
                    .unwrap(),
                    json!({"city":"Paris","days":2})
                );
                assert_eq!(
                    serde_json::from_str::<Value>(
                        output["output"][2]["arguments"].as_str().unwrap()
                    )
                    .unwrap(),
                    json!({})
                );
            }
        }
    }

    #[test]
    fn candidate_claude_native_json_authoritative_model_and_reasoning_survive_aliases() {
        let text = "line1\n\"quoted\" \\ 雪 <tag>";
        let input = json!({"nested":{"text":text,"items":[true,null,2]},"empty":{}});
        let raw = native(
            json!([{"type":"thinking","thinking":text,"signature":"sig\n\"native"},
            {"type":"redacted_thinking","data":"opaque-data"},
            {"type":"text","text":text},
            {"type":"tool_use","id":"tool_native","name":"lookup","input":input.clone()}]),
            "tool_use",
        );
        for request in [br#"{}"#.as_slice(), br#"{"model":"request-alias"}"#] {
            let chat: Value = serde_json::from_slice(&chat(request, request, &raw)).unwrap();
            let response: Value =
                serde_json::from_slice(&responses(request, request, &raw)).unwrap();
            assert_eq!(chat["model"], "claude-native");
            assert_eq!(chat["choices"][0]["message"]["content"], text);
            assert_eq!(chat["choices"][0]["message"]["reasoning_content"], text);
            assert_eq!(response["model"], "claude-native");
            assert_eq!(response["output"][0]["summary"][0]["text"], text);
            assert_eq!(response["output"][0]["encrypted_content"], "sig\n\"native");
            assert_eq!(
                response["output"][1]["encrypted_content"],
                "claude-redacted-thinking:opaque-data"
            );
            assert_eq!(response["output"][2]["content"][0]["text"], text);
            assert_eq!(
                serde_json::from_str::<Value>(response["output"][3]["arguments"].as_str().unwrap())
                    .unwrap(),
                input
            );
        }
    }

    #[test]
    fn candidate_claude_native_json_missing_usage_and_unknown_blocks_remain_valid() {
        let raw = br#"{"type":"message","content":[{"type":"custom","number":2e+09,"number":9}]}"#;
        let (adapted, model) = claude_messages_json_to_sse(raw);
        assert!(model.is_empty());
        let events = events(&adapted);
        assert_eq!(events.len(), 5);
        assert_eq!(events[1]["content_block"]["type"], "custom");
        assert!(std::str::from_utf8(&adapted)
            .unwrap()
            .contains("\"number\":2e+09,\"number\":9"));
        assert_eq!(events[3]["usage"], json!({}));
        assert!(events[3]["delta"]["stop_reason"].is_null());
        assert!(events[3]["delta"]["stop_sequence"].is_null());
    }

    #[test]
    fn candidate_claude_native_json_preserves_sse_request_echo_model() {
        let raw = native(json!([{"type":"text","text":"SSE"}]), "end_turn");
        let (sse, _) = claude_messages_json_to_sse(&raw);
        let request = br#"{"model":"request-alias"}"#;
        let response: Value = serde_json::from_slice(&responses(request, request, &sse)).unwrap();
        assert_eq!(response["model"], "request-alias");
        assert_eq!(response["output"][0]["content"][0]["text"], "SSE");
    }

    #[test]
    fn candidate_claude_native_json_responses_stop_at_protocol_terminator() {
        let raw = native(json!([{"type":"text","text":"accepted"}]), "end_turn");
        let (sse, _) = claude_messages_json_to_sse(&raw);
        let mut sse = sse.into_owned();
        sse.extend_from_slice(b"data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"late\"}}\n\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":999}}\n\n");
        let response: Value = serde_json::from_slice(&responses(b"{}", b"{}", &sse)).unwrap();
        assert_eq!(response["output"][0]["content"][0]["text"], "accepted");
        assert_eq!(response["usage"]["output_tokens"], 5);
    }

    #[test]
    fn candidate_claude_native_json_text_merges_only_between_non_text_boundaries() {
        let raw = native(
            json!([
                {"type":"text","text":"before"},
                {"type":"custom","opaque":"boundary"},
                {"type":"text","text":"adjacent "},
                {"type":"text","text":"text"},
                {"type":"thinking","thinking":"plan","signature":"signed"},
                {"type":"text","text":"after"}
            ]),
            "end_turn",
        );
        let response: Value = serde_json::from_slice(&responses(b"{}", b"{}", &raw)).unwrap();
        let output = response["output"].as_array().unwrap();
        assert_eq!(output.len(), 4);
        assert_eq!(output[0]["content"][0]["text"], "before");
        assert_eq!(output[1]["content"][0]["text"], "adjacent text");
        assert_eq!(output[2]["type"], "reasoning");
        assert_eq!(output[2]["summary"][0]["text"], "plan");
        assert_eq!(output[3]["content"][0]["text"], "after");
    }

    #[test]
    fn candidate_claude_native_json_stream_reasoning_and_terminal_usage_match_chat() {
        use crate::internal::translator::claude::openai::chat_completions::{
            convert_claude_response_to_openai_chat_stream, ClaudeToChatStreamState,
        };
        let raw = native(
            json!([
                {"type":"thinking","thinking":"visible plan","signature":"signed"},
                {"type":"redacted_thinking","data":"hidden-plan"},
                {"type":"text","text":"answer"}
            ]),
            "end_turn",
        );
        let chat: Value = serde_json::from_slice(&chat(b"{}", b"{}", &raw)).unwrap();
        assert!(chat["choices"][0]["message"].get("reasoning").is_none());
        let (sse, _) = claude_messages_json_to_sse(&raw);
        let mut state = ClaudeToChatStreamState::default();
        let mut chunks = Vec::<Value>::new();
        for line in sse
            .split(|byte| *byte == b'\n')
            .filter(|line| line.starts_with(b"data:"))
        {
            chunks.extend(
                convert_claude_response_to_openai_chat_stream(
                    "selected-alias",
                    b"{}",
                    b"{}",
                    line,
                    &mut state,
                )
                .iter()
                .map(|chunk| serde_json::from_slice::<Value>(chunk).unwrap()),
            );
        }
        let reasoning = chunks
            .iter()
            .filter_map(|chunk| {
                chunk
                    .pointer("/choices/0/delta/reasoning_content")
                    .and_then(Value::as_str)
            })
            .collect::<String>();
        assert_eq!(
            reasoning,
            chat["choices"][0]["message"]["reasoning_content"]
                .as_str()
                .unwrap()
        );
        assert_eq!(reasoning, "visible plan");
        assert!(!serde_json::to_string(&chunks)
            .unwrap()
            .contains("hidden-plan"));
        let terminal = chunks
            .iter()
            .filter(|chunk| {
                chunk
                    .get("choices")
                    .and_then(Value::as_array)
                    .is_some_and(Vec::is_empty)
            })
            .collect::<Vec<_>>();
        assert_eq!(terminal.len(), 1);
        assert_eq!(terminal[0]["model"], "selected-alias");
        assert_eq!(terminal[0]["id"], "msg_native");
        assert_eq!(terminal[0]["usage"], chat["usage"]);
        assert_eq!(
            terminal[0]["usage"]["prompt_tokens_details"]["cache_write_tokens"],
            3
        );
        assert!(convert_claude_response_to_openai_chat_stream(
            "selected-alias",
            b"{}",
            b"{}",
            br#"data: {"type":"message_stop"}"#,
            &mut state
        )
        .is_empty());
    }

    #[test]
    fn candidate_claude_native_json_stream_without_usage_omits_terminal_chunk() {
        use crate::internal::translator::claude::openai::chat_completions::{
            convert_claude_response_to_openai_chat_stream, ClaudeToChatStreamState,
        };
        let mut state = ClaudeToChatStreamState::default();
        let start = convert_claude_response_to_openai_chat_stream(
            "selected",
            b"{}",
            b"{}",
            br#"data: {"type":"message_start","message":{"id":"no_usage"}}"#,
            &mut state,
        );
        assert_eq!(start.len(), 1);
        assert!(convert_claude_response_to_openai_chat_stream(
            "selected",
            b"{}",
            b"{}",
            br#"data: {"type":"message_stop"}"#,
            &mut state
        )
        .is_empty());
    }

    #[test]
    fn candidate_claude_native_json_stream_tool_indices_ignore_other_blocks() {
        use crate::internal::translator::claude::openai::chat_completions::{
            convert_claude_response_to_openai_chat_stream, ClaudeToChatStreamState,
        };
        let raw = native(
            json!([
                {"type":"thinking","thinking":"plan"},
                {"type":"tool_use","id":"weather","name":"weather","input":{"city":"Paris"}},
                {"type":"tool_use","id":"clock","name":"clock","input":{"city":"Tokyo"}}
            ]),
            "tool_use",
        );
        let (sse, _) = claude_messages_json_to_sse(&raw);
        let mut state = ClaudeToChatStreamState::default();
        let mut calls = Vec::<Value>::new();
        for line in sse
            .split(|byte| *byte == b'\n')
            .filter(|line| line.starts_with(b"data:"))
        {
            for chunk in convert_claude_response_to_openai_chat_stream(
                "selected", b"{}", b"{}", line, &mut state,
            ) {
                let chunk: Value = serde_json::from_slice(&chunk).unwrap();
                if let Some(call) = chunk.pointer("/choices/0/delta/tool_calls/0") {
                    calls.push(call.clone());
                }
            }
        }
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0]["index"], 0);
        assert_eq!(calls[1]["index"], 1);
        assert_eq!(calls[0]["id"], "weather");
        assert_eq!(calls[1]["id"], "clock");
        assert_eq!(
            serde_json::from_str::<Value>(calls[0]["function"]["arguments"].as_str().unwrap())
                .unwrap(),
            json!({"city":"Paris"})
        );
        assert_eq!(
            serde_json::from_str::<Value>(calls[1]["function"]["arguments"].as_str().unwrap())
                .unwrap(),
            json!({"city":"Tokyo"})
        );
    }
}
