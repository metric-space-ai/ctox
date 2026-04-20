use crate::gpt_oss_harmony;
use crate::openai::{FunctionCalled, Message, MessageContent, ToolCall};
use engine_core::{Tool, ToolChoice, ToolType};
use regex::Regex;
use serde_json::Value;

use super::AdaptedChatRequest;
use super::AdaptedFunctionCall;
use super::AdaptedResponseItem;
use super::ResponsesTransportKind;

pub fn matches(model_id: &str) -> bool {
    model_id
        .trim()
        .to_ascii_lowercase()
        .contains("nemotron-cascade-2")
}

pub fn transport_kind() -> ResponsesTransportKind {
    ResponsesTransportKind::ChatCompletions
}

pub fn requires_buffered_chat_response() -> bool {
    true
}

pub fn prepare_chat_request(
    messages: &[Message],
    tools: Option<&[Tool]>,
    tool_choice: Option<&ToolChoice>,
) -> AdaptedChatRequest {
    let mut adapted_messages = messages.to_vec();
    if let Some(prompt) =
        build_exact_text_prompt(gpt_oss_harmony::extract_exact_text_override(messages).as_deref())
    {
        prepend_system_message(&mut adapted_messages, prompt);
    }
    if let Some(prompt) = build_tool_prompt(tools, tool_choice) {
        prepend_system_message(&mut adapted_messages, prompt);
    }

    let disable_native_tools = tools.is_some_and(|items| !items.is_empty());
    AdaptedChatRequest {
        messages: adapted_messages,
        tools: if disable_native_tools {
            None
        } else {
            tools.map(|items| items.to_vec())
        },
        tool_choice: if disable_native_tools {
            None
        } else {
            tool_choice.cloned()
        },
    }
}

pub fn parse_response_items(raw_text: &str) -> Vec<AdaptedResponseItem> {
    let (reasoning_text, visible_text) = split_nemotron_reasoning_and_content(raw_text);
    let tool_call_re = Regex::new(
        r"(?s)<tool_call>\s*<function=([A-Za-z0-9_.-]+)>\s*(.*?)\s*</function>\s*</tool_call>",
    )
    .expect("valid Nemotron tool call regex");
    let param_re = Regex::new(r"(?s)<parameter=([A-Za-z0-9_.-]+)>\s*(.*?)\s*</parameter>")
        .expect("valid Nemotron parameter regex");

    let mut items = Vec::new();
    if let Some(reasoning) = reasoning_text.filter(|text| !text.trim().is_empty()) {
        items.push(AdaptedResponseItem::Reasoning(reasoning));
    }
    let mut plain_text = String::new();
    let mut last_end = 0usize;

    for (index, captures) in tool_call_re.captures_iter(&visible_text).enumerate() {
        let Some(matched) = captures.get(0) else {
            continue;
        };
        plain_text.push_str(&visible_text[last_end..matched.start()]);
        last_end = matched.end();

        let name = captures
            .get(1)
            .map(|capture| capture.as_str().trim().to_string())
            .unwrap_or_default();
        let body = captures
            .get(2)
            .map(|capture| capture.as_str())
            .unwrap_or_default();
        let mut arguments = serde_json::Map::new();
        for param in param_re.captures_iter(body) {
            let Some(param_name) = param.get(1).map(|capture| capture.as_str().trim()) else {
                continue;
            };
            let value = param
                .get(2)
                .map(|capture| capture.as_str().trim().to_string())
                .unwrap_or_default();
            arguments.insert(param_name.to_string(), Value::String(value));
        }
        items.push(AdaptedResponseItem::FunctionCall(AdaptedFunctionCall {
            call_id: format!("call_ctox_local_{index}"),
            name,
            arguments: Value::Object(arguments).to_string(),
        }));
    }

    plain_text.push_str(&visible_text[last_end..]);
    let plain_text = plain_text.trim();
    if !plain_text.is_empty() {
        items.insert(0, AdaptedResponseItem::Message(plain_text.to_string()));
    }
    if items.is_empty() && !raw_text.trim().is_empty() {
        items.push(AdaptedResponseItem::Message(raw_text.trim().to_string()));
    }
    items
}

pub fn response_text_to_history_messages(raw_text: &str) -> Vec<Message> {
    parse_response_items(raw_text)
        .into_iter()
        .filter_map(|item| match item {
            AdaptedResponseItem::Reasoning(_) => None,
            AdaptedResponseItem::Message(text) => Some(Message {
                content: Some(MessageContent::from_text(text)),
                role: "assistant".to_string(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            }),
            AdaptedResponseItem::FunctionCall(call) => Some(Message {
                content: None,
                role: "assistant".to_string(),
                name: None,
                tool_calls: Some(vec![ToolCall {
                    id: Some(call.call_id),
                    tp: ToolType::Function,
                    function: FunctionCalled {
                        name: call.name,
                        arguments: call.arguments,
                    },
                }]),
                tool_call_id: None,
            }),
        })
        .collect()
}

fn split_nemotron_reasoning_and_content(text: &str) -> (Option<String>, String) {
    let trimmed = text.trim();
    if let Some(rest) = trimmed.strip_prefix("<think>") {
        if let Some((reasoning, content)) = rest.split_once("</think>") {
            let reasoning = reasoning.trim().to_string();
            let content = content.trim().to_string();
            return (
                if reasoning.is_empty() {
                    None
                } else {
                    Some(reasoning)
                },
                content,
            );
        }
    }
    (None, trimmed.to_string())
}

fn prepend_system_message(messages: &mut Vec<Message>, prompt: String) {
    if prompt.trim().is_empty() {
        return;
    }
    if let Some(first) = messages.first_mut() {
        if first.role == "system" {
            let merged = first
                .content
                .as_ref()
                .and_then(MessageContent::to_text)
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty())
                .map(|text| format!("{text}\n\n{prompt}"))
                .unwrap_or(prompt);
            first.content = Some(MessageContent::from_text(merged));
            return;
        }
    }
    messages.insert(
        0,
        Message {
            content: Some(MessageContent::from_text(prompt)),
            role: "system".to_string(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        },
    );
}

fn build_exact_text_prompt(exact_text_override: Option<&str>) -> Option<String> {
    let exact_text = exact_text_override
        .map(str::trim)
        .filter(|text| !text.is_empty())?;
    Some(format!(
        "Return exactly this final answer and nothing else: {exact_text}\nDo not emit reasoning or any additional text."
    ))
}

fn build_tool_prompt(tools: Option<&[Tool]>, tool_choice: Option<&ToolChoice>) -> Option<String> {
    let tools = tools.filter(|items| !items.is_empty())?;
    let mut lines = vec![
        "Available tools are described below.".to_string(),
        "When you need a tool, emit exactly one XML tool call and no surrounding prose using this format:".to_string(),
        "<tool_call>".to_string(),
        "<function=TOOL_NAME>".to_string(),
        "<parameter=ARG_NAME>ARG_VALUE</parameter>".to_string(),
        "</function>".to_string(),
        "</tool_call>".to_string(),
    ];
    if let Some(required_tool) = required_tool_name(tool_choice) {
        lines.push(format!(
            "Your next response must be a tool call for `{required_tool}`."
        ));
    }
    lines.push("Tools:".to_string());
    for tool in tools {
        let Some(spec) = serde_json::to_value(tool).ok() else {
            continue;
        };
        let Some(function) = spec.get("function").and_then(Value::as_object) else {
            continue;
        };
        let name = function
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if name.is_empty() || name == "apply_patch" {
            continue;
        }
        let description = function
            .get("description")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("No description provided.");
        lines.push(format!("- {name}: {description}"));
    }
    Some(lines.join("\n"))
}

fn required_tool_name(tool_choice: Option<&ToolChoice>) -> Option<String> {
    match tool_choice {
        Some(ToolChoice::Tool(tool)) => serde_json::to_value(tool).ok().and_then(|value| {
            value
                .get("function")
                .and_then(|function| function.get("name"))
                .or_else(|| value.get("name"))
                .and_then(Value::as_str)
                .map(str::to_string)
        }),
        _ => None,
    }
}
