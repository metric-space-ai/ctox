// ref: internal/translator/openai/claude/openai_claude_request_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::Value;

use super::convert_claude_request_to_openai;

fn convert(input: &str) -> Value {
    serde_json::from_slice(&convert_claude_request_to_openai(
        "claude-sonnet-4-5",
        input.as_bytes(),
        false,
    ))
    .expect("valid OpenAI Chat output")
}

fn last_assistant(messages: &[Value]) -> &Value {
    for message in messages.iter().rev() {
        if message["role"] == "assistant" {
            return message;
        }
    }
    panic!("no assistant message in output: {messages:?}")
}

#[test]
fn ac1_unsigned_assistant_thinking_is_dropped() {
    let output = convert(
        r#"{
            "model": "claude-3-opus",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "Let me analyze this step by step..."},
                    {"type": "text", "text": "Here is my response."}
                ]
            }]
        }"#,
    );
    let messages = output["messages"].as_array().expect("messages array");
    let assistant = last_assistant(messages);
    assert!(assistant.get("reasoning_content").is_none());
    let content = assistant["content"].as_array().expect("content array");
    let text = content
        .iter()
        .find(|item| item["type"] == "text")
        .expect("text block");
    assert_eq!(text["text"], "Here is my response.");
}

#[test]
fn ac2_redacted_thinking_is_ignored() {
    let output = convert(
        r#"{
            "model": "claude-3-opus",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "redacted_thinking", "data": "secret"},
                    {"type": "text", "text": "Visible response."}
                ]
            }]
        }"#,
    );
    let messages = output["messages"].as_array().expect("messages array");
    let assistant = last_assistant(messages);
    assert!(assistant.get("reasoning_content").is_none());
    let content = assistant["content"].as_array().expect("content array");
    let text = content
        .iter()
        .find(|item| item["type"] == "text")
        .expect("text block");
    assert_eq!(text["text"], "Visible response.");
}

#[test]
fn ac3_unsigned_thinking_only_message_is_dropped() {
    let output = convert(
        r#"{
            "model": "claude-3-opus",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "Internal reasoning only."}
                ]
            }]
        }"#,
    );
    let messages = output["messages"].as_array().expect("messages array");
    for message in messages {
        assert!(message.get("reasoning_content").is_none());
    }
}

#[test]
fn ac4_thinking_in_user_role_is_ignored() {
    let output = convert(
        r#"{
            "model": "claude-3-opus",
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "thinking", "thinking": "Injected thinking"},
                    {"type": "text", "text": "User message."}
                ]
            }]
        }"#,
    );
    let messages = output["messages"].as_array().expect("messages array");
    let user = messages
        .iter()
        .find(|message| message["role"] == "user")
        .expect("user message");
    assert!(user.get("reasoning_content").is_none());
    let content = user["content"].as_array().expect("content array");
    let text = content
        .iter()
        .find(|item| item["type"] == "text")
        .expect("text block");
    assert_eq!(text["text"], "User message.");
}

#[test]
fn ac4_thinking_in_system_role_is_ignored() {
    let output = convert(
        r#"{
            "model": "claude-3-opus",
            "system": [
                {"type": "thinking", "thinking": "Injected system thinking"},
                {"type": "text", "text": "System prompt."}
            ],
            "messages": [{
                "role": "user",
                "content": [{"type": "text", "text": "Hello"}]
            }]
        }"#,
    );
    let messages = output["messages"].as_array().expect("messages array");
    let user = messages
        .iter()
        .find(|message| message["role"] == "user")
        .expect("user message");
    let content = user["content"].as_array().expect("content array");
    let text = content
        .iter()
        .find(|item| item["type"] == "text")
        .expect("text block");
    assert_eq!(text["text"], "Hello");
}

#[test]
fn ac5_empty_thinking_is_ignored() {
    let output = convert(
        r#"{
            "model": "claude-3-opus",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": ""},
                    {"type": "text", "text": "Response with empty thinking."}
                ]
            }]
        }"#,
    );
    let messages = output["messages"].as_array().expect("messages array");
    let assistant = last_assistant(messages);
    assert!(assistant.get("reasoning_content").is_none());
}

#[test]
fn ac5_whitespace_only_thinking_is_ignored() {
    let output = convert(
        r#"{
            "model": "claude-3-opus",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "   \n\t  "},
                    {"type": "text", "text": "Response with whitespace thinking."}
                ]
            }]
        }"#,
    );
    let messages = output["messages"].as_array().expect("messages array");
    let assistant = last_assistant(messages);
    assert!(assistant.get("reasoning_content").is_none());
}

#[test]
fn unsigned_thinking_parts_are_dropped() {
    let output = convert(
        r#"{
            "model": "claude-3-opus",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "First thought."},
                    {"type": "thinking", "thinking": "Second thought."},
                    {"type": "text", "text": "Final answer."}
                ]
            }]
        }"#,
    );
    let messages = output["messages"].as_array().expect("messages array");
    let assistant = last_assistant(messages);
    assert!(assistant.get("reasoning_content").is_none());
    let content = assistant["content"].as_array().expect("content array");
    let text = content
        .iter()
        .find(|item| item["type"] == "text")
        .expect("text block");
    assert_eq!(text["text"], "Final answer.");
}

#[test]
fn mixed_unsigned_thinking_and_redacted_thinking() {
    let output = convert(
        r#"{
            "model": "claude-3-opus",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "Visible thought."},
                    {"type": "redacted_thinking", "data": "hidden"},
                    {"type": "text", "text": "Answer."}
                ]
            }]
        }"#,
    );
    let messages = output["messages"].as_array().expect("messages array");
    let assistant = last_assistant(messages);
    assert!(assistant.get("reasoning_content").is_none());
    let content = assistant["content"].as_array().expect("content array");
    let text = content
        .iter()
        .find(|item| item["type"] == "text")
        .expect("text block");
    assert_eq!(text["text"], "Answer.");
}

#[test]
fn signed_thinking_compatibility_keeps_reasoning_content_for_gpt() {
    // Build a synthetic GPT signature: 0x80 version byte, 8-byte timestamp,
    // 16-byte IV, 16-byte auth tag, 32-byte HMAC.
    let mut raw = vec![0_u8; 1 + 8 + 16 + 16 + 32];
    raw[0] = 0x80;
    raw[8] = 1;
    for (index, byte) in raw.iter_mut().enumerate().skip(9) {
        *byte = index as u8;
    }
    let signature = base64_url_encode(&raw);
    let input = format!(
        r#"{{
            "model": "claude-3-opus",
            "messages": [{{
                "role": "assistant",
                "content": [
                    {{"type": "thinking", "thinking": "provider state", "signature": "{signature}"}},
                    {{"type": "text", "text": "visible answer"}}
                ]
            }}]
        }}"#
    );
    let output = convert(&input);
    let messages = output["messages"].as_array().expect("messages array");
    let assistant = &messages[0];
    assert_eq!(assistant["reasoning_content"], "provider state");
    let content = assistant["content"].as_array().expect("content array");
    let text = content
        .iter()
        .find(|item| item["type"] == "text")
        .expect("text block");
    assert_eq!(text["text"], "visible answer");
}

#[test]
fn signed_thinking_compatibility_drops_reasoning_content_for_claude_prefix() {
    // The Claude "claude#..." signature is a valid Claude thinking signature
    // but is not a GPT-compatible provider signature, so reasoning_content
    // must be dropped to avoid reasoning-state injection.
    let input = r#"{
        "model": "claude-3-opus",
        "messages": [{
            "role": "assistant",
            "content": [
                {"type": "thinking", "thinking": "provider state", "signature": "claude#EjQ="},
                {"type": "text", "text": "visible answer"}
            ]
        }]
    }"#;
    let output = convert(input);
    let messages = output["messages"].as_array().expect("messages array");
    let assistant = &messages[0];
    assert!(assistant.get("reasoning_content").is_none());
    let content = assistant["content"].as_array().expect("content array");
    let text = content
        .iter()
        .find(|item| item["type"] == "text")
        .expect("text block");
    assert_eq!(text["text"], "visible answer");
}

#[test]
fn signed_thinking_compatibility_drops_reasoning_content_for_gemini_prefix() {
    let input = r#"{
        "model": "claude-3-opus",
        "messages": [{
            "role": "assistant",
            "content": [
                {"type": "thinking", "thinking": "provider state", "signature": "gemini#EjQKMgEMOdbHO0Gd+c9Mxk4ELwPGbpCEcp2mFfYYLix2UVtBH3fL8GECc4+JITVnHF4qZDsA"},
                {"type": "text", "text": "visible answer"}
            ]
        }]
    }"#;
    let output = convert(input);
    let messages = output["messages"].as_array().expect("messages array");
    let assistant = &messages[0];
    assert!(assistant.get("reasoning_content").is_none());
}

#[test]
fn signed_thinking_compatibility_drops_reasoning_content_for_unknown_signature() {
    let input = r#"{
        "model": "claude-3-opus",
        "messages": [{
            "role": "assistant",
            "content": [
                {"type": "thinking", "thinking": "provider state", "signature": "not-a-provider-signature"},
                {"type": "text", "text": "visible answer"}
            ]
        }]
    }"#;
    let output = convert(input);
    let messages = output["messages"].as_array().expect("messages array");
    let assistant = &messages[0];
    assert!(assistant.get("reasoning_content").is_none());
}

#[test]
fn unsigned_thinking_only_message_dropped_from_message_list() {
    let output = convert(
        r#"{
            "model": "claude-3-opus",
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "What is 2+2?"}]},
                {"role": "assistant", "content": [{"type": "thinking", "thinking": "Let me calculate: 2+2=4"}]},
                {"role": "user", "content": [{"type": "text", "text": "Thanks"}]}
            ]
        }"#,
    );
    let messages = output["messages"].as_array().expect("messages array");
    assert_eq!(messages.len(), 2, "messages: {messages:?}");
    for message in messages {
        assert!(message.get("reasoning_content").is_none());
    }
}

#[test]
fn message_system_role_wraps_as_user_reminder() {
    let output = convert(
        r#"{
            "model": "claude-sonnet-4-5",
            "system": [{"type": "text", "text": "Top-level rules"}],
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "Hello"}]},
                {"role": "system", "content": "String mid-conversation rule"},
                {"role": "assistant", "content": [{"type": "text", "text": "Hi there"}]},
                {"role": "system", "content": [{"type": "text", "text": "Array mid-conversation rule"}]},
                {"role": "user", "content": [{"type": "text", "text": "Follow up"}]}
            ]
        }"#,
    );
    let messages = output["messages"].as_array().expect("messages array");
    assert_eq!(messages.len(), 6, "messages: {messages:?}");
    let roles: Vec<&str> = messages
        .iter()
        .map(|message| message["role"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(
        roles,
        vec!["system", "user", "user", "assistant", "user", "user"],
        "roles were {roles:?}"
    );
    let system_content = messages[0]["content"].as_array().expect("system content");
    assert_eq!(system_content.len(), 1);
    assert_eq!(system_content[0]["text"], "Top-level rules");
    assert_eq!(
        messages[2]["content"][0]["text"],
        "<system-reminder>\nString mid-conversation rule\n</system-reminder>"
    );
    assert_eq!(
        messages[4]["content"][0]["text"],
        "<system-reminder>\nArray mid-conversation rule\n</system-reminder>"
    );
}

#[test]
fn system_message_scenarios() {
    let cases: Vec<(&str, bool, Option<&str>)> = vec![
        (
            r#"{"model":"claude-3-opus","messages":[{"role":"user","content":"hello"}]}"#,
            false,
            None,
        ),
        (
            r#"{"model":"claude-3-opus","system":"","messages":[{"role":"user","content":"hello"}]}"#,
            false,
            None,
        ),
        (
            r#"{"model":"claude-3-opus","system":"Be helpful","messages":[{"role":"user","content":"hello"}]}"#,
            true,
            Some("Be helpful"),
        ),
        (
            r#"{"model":"claude-3-opus","system":[{"type":"text","text":"Array system"}],"messages":[{"role":"user","content":"hello"}]}"#,
            true,
            Some("Array system"),
        ),
        (
            r#"{"model":"claude-3-opus","system":[{"type":"text","text":"Block 1"},{"type":"text","text":"Block 2"}],"messages":[{"role":"user","content":"hello"}]}"#,
            true,
            Some("Block 2"),
        ),
    ];
    for (input, want_has_sys, want_text) in cases {
        let output = convert(input);
        let messages = output["messages"].as_array().expect("messages array");
        let (has_sys, sys_msg) = if !messages.is_empty() && messages[0]["role"] == "system" {
            (true, &messages[0])
        } else {
            (false, &Value::Null)
        };
        assert_eq!(has_sys, want_has_sys, "input: {input}");
        if let Some(want_text) = want_text {
            let content = &sys_msg["content"];
            let actual = if let Some(array) = content.as_array() {
                array
                    .last()
                    .and_then(|item| item["text"].as_str())
                    .unwrap_or("")
                    .to_owned()
            } else {
                content.as_str().unwrap_or("").to_owned()
            };
            assert_eq!(actual, want_text, "input: {input}");
        }
    }
}

#[test]
fn tool_schema_adds_missing_object_properties_recursively() {
    let output = convert(
        r#"{
            "model": "claude-3-opus",
            "tools": [
                {
                    "name": "empty_params",
                    "description": "No args",
                    "input_schema": {"type": "object"}
                },
                {
                    "name": "nested_params",
                    "description": "Nested args",
                    "input_schema": {
                        "type": "object",
                        "properties": {
                            "nested": {"type": "object"},
                            "items": {
                                "type": "array",
                                "items": {"type": "object"}
                            }
                        }
                    }
                }
            ],
            "messages": [{"role": "user", "content": "hello"}]
        }"#,
    );
    let tools = output["tools"].as_array().expect("tools array");
    assert_eq!(tools[0]["function"]["parameters"]["type"], "object");
    assert!(tools[0]["function"]["parameters"]["properties"].is_object());
    let nested = &tools[1]["function"]["parameters"]["properties"]["nested"];
    assert_eq!(nested["type"], "object");
    assert!(nested["properties"].is_object());
    let items = &tools[1]["function"]["parameters"]["properties"]["items"];
    assert!(items["items"]["properties"].is_object());
}

#[test]
fn tool_result_order_and_content() {
    let output = convert(
        r#"{
            "model": "claude-3-opus",
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {"type": "tool_use", "id": "call_1", "name": "do_work", "input": {"a": 1}}
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        {"type": "text", "text": "before"},
                        {"type": "tool_result", "tool_use_id": "call_1", "content": [{"type":"text","text":"tool ok"}]},
                        {"type": "text", "text": "after"}
                    ]
                }
            ]
        }"#,
    );
    let messages = output["messages"].as_array().expect("messages array");
    assert_eq!(messages.len(), 3, "messages: {messages:?}");
    assert_eq!(messages[0]["role"], "assistant");
    assert!(messages[0]["tool_calls"].is_array());
    assert_eq!(messages[1]["role"], "tool");
    assert_eq!(messages[1]["tool_call_id"], "call_1");
    assert_eq!(messages[1]["content"], "tool ok");
    assert_eq!(messages[2]["role"], "user");
    let user_content = messages[2]["content"].as_array().expect("user content");
    assert_eq!(user_content[0]["text"], "before");
    assert_eq!(user_content[1]["text"], "after");
}

#[test]
fn tool_result_object_content() {
    let output = convert(
        r#"{
            "model": "claude-3-opus",
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {"type": "tool_use", "id": "call_1", "name": "do_work", "input": {"a": 1}}
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        {"type": "tool_result", "tool_use_id": "call_1", "content": {"foo": "bar"}}
                    ]
                }
            ]
        }"#,
    );
    let messages = output["messages"].as_array().expect("messages array");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1]["role"], "tool");
    let content = messages[1]["content"]
        .as_str()
        .expect("tool content string");
    let parsed: Value = serde_json::from_str(content).expect("tool content JSON");
    assert_eq!(parsed["foo"], "bar");
}

#[test]
fn tool_result_text_and_image_content() {
    let output = convert(
        r#"{
            "model": "claude-3-opus",
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {"type": "tool_use", "id": "call_1", "name": "do_work", "input": {"a": 1}}
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "tool_result",
                            "tool_use_id": "call_1",
                            "content": [
                                {"type": "text", "text": "tool ok"},
                                {
                                    "type": "image",
                                    "source": {
                                        "type": "base64",
                                        "media_type": "image/png",
                                        "data": "iVBORw0KGgoAAAANSUhEUg=="
                                    }
                                }
                            ]
                        }
                    ]
                }
            ]
        }"#,
    );
    let messages = output["messages"].as_array().expect("messages array");
    let content = messages[1]["content"]
        .as_array()
        .expect("tool content array");
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[0]["text"], "tool ok");
    assert_eq!(content[1]["type"], "image_url");
    assert_eq!(
        content[1]["image_url"]["url"],
        "data:image/png;base64,iVBORw0KGgoAAAANSUhEUg=="
    );
}

#[test]
fn tool_result_url_image_only() {
    let output = convert(
        r#"{
            "model": "claude-3-opus",
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {"type": "tool_use", "id": "call_1", "name": "do_work", "input": {"a": 1}}
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "tool_result",
                            "tool_use_id": "call_1",
                            "content": {
                                "type": "image",
                                "source": {
                                    "type": "url",
                                    "url": "https://example.com/tool.png"
                                }
                            }
                        }
                    ]
                }
            ]
        }"#,
    );
    let messages = output["messages"].as_array().expect("messages array");
    let content = messages[1]["content"]
        .as_array()
        .expect("tool content array");
    assert_eq!(content[0]["type"], "image_url");
    assert_eq!(
        content[0]["image_url"]["url"],
        "https://example.com/tool.png"
    );
}

#[test]
fn assistant_text_tool_use_text_unified() {
    let output = convert(
        r#"{
            "model": "claude-3-opus",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "text", "text": "pre"},
                    {"type": "tool_use", "id": "call_1", "name": "do_work", "input": {"a": 1}},
                    {"type": "text", "text": "post"}
                ]
            }]
        }"#,
    );
    let messages = output["messages"].as_array().expect("messages array");
    assert_eq!(messages.len(), 1);
    let assistant = &messages[0];
    assert_eq!(assistant["role"], "assistant");
    let tool_calls = assistant["tool_calls"].as_array().expect("tool_calls");
    assert_eq!(tool_calls[0]["id"], "call_1");
    assert_eq!(tool_calls[0]["function"]["name"], "do_work");
    let content = assistant["content"].as_array().expect("content array");
    assert_eq!(content[0]["text"], "pre");
    assert_eq!(content[1]["text"], "post");
}

#[test]
fn assistant_unsigned_thinking_tool_use_text_unified_without_reasoning() {
    let output = convert(
        r#"{
            "model": "claude-3-opus",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "t1"},
                    {"type": "text", "text": "pre"},
                    {"type": "tool_use", "id": "call_1", "name": "do_work", "input": {"a": 1}},
                    {"type": "thinking", "thinking": "t2"},
                    {"type": "text", "text": "post"}
                ]
            }]
        }"#,
    );
    let messages = output["messages"].as_array().expect("messages array");
    assert_eq!(messages.len(), 1);
    let assistant = &messages[0];
    let content = assistant["content"].as_array().expect("content array");
    assert_eq!(content[0]["text"], "pre");
    assert_eq!(content[1]["text"], "post");
    assert!(assistant["tool_calls"].is_array());
    assert!(assistant.get("reasoning_content").is_none());
}

#[test]
fn strips_claude_code_attribution_from_system() {
    let output = convert(
        r#"{
            "model": "claude-sonnet-4-5",
            "system": [
                {"type": "text", "text": "x-anthropic-billing-header: cc_version=2.1.63.abc; cc_entrypoint=cli; cch=12345;"},
                {"type": "text", "text": "User system prompt"}
            ],
            "messages": [{"role": "user", "content": [{"type": "text", "text": "hi"}]}]
        }"#,
    );
    let messages = output["messages"].as_array().expect("messages array");
    assert_eq!(messages[0]["role"], "system");
    let content = messages[0]["content"]
        .as_array()
        .expect("system content array");
    assert_eq!(content.len(), 1, "system content: {content:?}");
    assert_eq!(content[0]["text"], "User system prompt");
}

fn base64_url_encode(bytes: &[u8]) -> String {
    // Use the standard URL-safe base64 alphabet without padding. We avoid
    // pulling in the `base64` crate at the test boundary by using a tiny
    // local encoder.
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let chunks = bytes.chunks(3);
    for chunk in chunks {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        output.push(ALPHABET[(b0 >> 2) as usize] as char);
        output.push(ALPHABET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(ALPHABET[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        }
        if chunk.len() > 2 {
            output.push(ALPHABET[(b2 & 0x3f) as usize] as char);
        }
    }
    output
}

#[test]
fn tool_schema_strips_bad_patterns_and_normalizes_boolean_subschemas() {
    let output = convert(
        r#"{
            "model":"claude-3-opus",
            "messages":[{"role":"user","content":"hello"}],
            "tools":[{
                "name":"patch_tool",
                "description":"Applies a JSON patch",
                "input_schema":{
                    "type":"object",
                    "properties":{
                        "patch":{"type":"array","items":true},
                        "anything":true,
                        "disabled":false,
                        "enabled_flag":{"type":"boolean","default":true,"enum":[true,false]},
                        "either":{"anyOf":[true,{"type":"string"}]},
                        "field":{"type":"string","pattern":"\\p{L}+"},
                        "asset_id":{"type":"string","pattern":"^[0-9a-f]{32}$"},
                        "regex_config":{"type":"object","default":{"pattern":"\\p{L}+"},"enum":[{"pattern":"\\p{N}+"}]},
                        "nested_obj":{"type":"object","properties":{"foo":{"type":"string"}},"additionalProperties":true}
                    },
                    "additionalProperties":false,
                    "$defs":{"wildcard":true},
                    "patternProperties":{"^\\p{L}+$":{"type":"string"},"^[a-z]+$":{"type":"number"}}
                }
            }]
        }"#,
    );
    let parameters = &output["tools"][0]["function"]["parameters"];
    assert_eq!(parameters["properties"]["patch"]["items"], serde_json::json!({}));
    assert_eq!(parameters["properties"]["anything"], serde_json::json!({}));
    assert_eq!(parameters["properties"]["disabled"], false);
    assert_eq!(parameters["properties"]["enabled_flag"]["default"], true);
    assert_eq!(parameters["properties"]["enabled_flag"]["enum"][0], true);
    assert_eq!(parameters["properties"]["either"]["anyOf"][0], serde_json::json!({}));
    assert!(parameters["properties"]["field"].get("pattern").is_none());
    assert_eq!(parameters["properties"]["asset_id"]["pattern"], "^[0-9a-f]{32}$");
    assert_eq!(
        parameters["properties"]["regex_config"]["default"]["pattern"],
        r"\p{L}+"
    );
    assert_eq!(parameters["additionalProperties"], false);
    assert_eq!(
        parameters["properties"]["nested_obj"]["additionalProperties"],
        true
    );
    assert_eq!(parameters["$defs"]["wildcard"], serde_json::json!({}));
    assert!(parameters["patternProperties"].get(r"^\p{L}+$").is_none());
    assert_eq!(parameters["patternProperties"]["^[a-z]+$"]["type"], "number");
}
