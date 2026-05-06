use ctox_protocol::models::ResponseItem;
use serde_json::Value;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Compression {
    #[default]
    None,
    Zstd,
}

pub(crate) fn attach_item_ids(payload_json: &mut Value, original_items: &[ResponseItem]) {
    let Some(input_value) = payload_json.get_mut("input") else {
        return;
    };
    let Value::Array(items) = input_value else {
        return;
    };

    for (index, (value, item)) in items.iter_mut().zip(original_items.iter()).enumerate() {
        let Some(id) = response_item_history_id(item, index) else {
            continue;
        };
        if let Some(obj) = value.as_object_mut() {
            obj.insert("id".to_string(), Value::String(id));
        }
    }
}

fn response_item_history_id(item: &ResponseItem, index: usize) -> Option<String> {
    let explicit = match item {
        ResponseItem::Reasoning { id, .. } => Some(id),
        ResponseItem::Message { id: Some(id), .. }
        | ResponseItem::WebSearchCall { id: Some(id), .. }
        | ResponseItem::FunctionCall { id: Some(id), .. }
        | ResponseItem::ToolSearchCall { id: Some(id), .. }
        | ResponseItem::LocalShellCall { id: Some(id), .. }
        | ResponseItem::CustomToolCall { id: Some(id), .. } => Some(id),
        _ => None,
    };
    if let Some(id) = explicit
        && !id.is_empty()
    {
        return Some(id.clone());
    }

    match item {
        ResponseItem::FunctionCallOutput { call_id, .. }
        | ResponseItem::CustomToolCallOutput { call_id, .. } => {
            Some(format!("fc_output_{}", response_id_component(call_id)))
        }
        ResponseItem::ToolSearchOutput {
            call_id: Some(call_id),
            ..
        } => Some(format!("fc_output_{}", response_id_component(call_id))),
        ResponseItem::ToolSearchOutput { call_id: None, .. } => Some(format!("fc_output_{index}")),
        _ => None,
    }
}

fn response_id_component(value: &str) -> String {
    let component = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if component.is_empty() {
        "item".to_string()
    } else {
        component
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctox_protocol::models::ContentItem;
    use ctox_protocol::models::FunctionCallOutputPayload;
    use serde_json::json;

    #[test]
    fn attaches_ids_to_function_call_history_for_responses_providers() {
        let items = vec![
            ResponseItem::FunctionCall {
                id: Some("fc_123".to_string()),
                name: "shell".to_string(),
                namespace: None,
                arguments: "{}".to_string(),
                call_id: "call_123".to_string(),
            },
            ResponseItem::FunctionCallOutput {
                call_id: "call_123".to_string(),
                output: FunctionCallOutputPayload::from_text("ok".to_string()),
            },
            ResponseItem::Message {
                id: Some("msg_123".to_string()),
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: "done".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
        ];
        let mut body = json!({
            "input": [
                {"type": "function_call", "call_id": "call_123", "name": "shell", "arguments": "{}"},
                {"type": "function_call_output", "call_id": "call_123", "output": "ok"},
                {"type": "message", "role": "assistant", "content": "done"}
            ]
        });

        attach_item_ids(&mut body, &items);

        assert_eq!(body["input"][0]["id"], "fc_123");
        assert_eq!(body["input"][1]["id"], "fc_output_call_123");
        assert_eq!(body["input"][2]["id"], "msg_123");
    }
}
