// ref: internal/runtime/executor/helps/claude_builtin_tools.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::HashMap;

use serde_json::Value;

const DEFAULT_CLAUDE_BUILTIN_TOOL_NAMES: [&str; 4] =
    ["web_search", "code_execution", "text_editor", "computer"];

fn new_claude_builtin_tool_registry() -> HashMap<String, bool> {
    DEFAULT_CLAUDE_BUILTIN_TOOL_NAMES
        .iter()
        .map(|name| ((*name).to_owned(), true))
        .collect()
}

fn gjson_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        value => serde_json::to_string(value).unwrap_or_default(),
    }
}

pub fn is_claude_server_tool_type(tool_type: &str) -> bool {
    let tool_type = tool_type.trim().to_ascii_lowercase();
    [
        "advisor_",
        "agent_toolset_",
        "bash_",
        "code_execution_",
        "computer_",
        "memory_",
        "text_editor_",
        "tool_search_tool_",
        "web_fetch_",
        "web_search_",
    ]
    .iter()
    .any(|prefix| tool_type.starts_with(prefix))
}

pub fn augment_claude_builtin_tool_registry(
    body: &[u8],
    registry: Option<HashMap<String, bool>>,
) -> HashMap<String, bool> {
    let mut registry = registry.unwrap_or_else(new_claude_builtin_tool_registry);
    let Ok(body) = serde_json::from_slice::<Value>(body) else {
        return registry;
    };
    let Some(tools) = body.get("tools").and_then(Value::as_array) else {
        return registry;
    };
    for tool in tools {
        let Some(tool) = tool.as_object() else {
            continue;
        };
        let Some(tool_type) = tool.get("type") else {
            continue;
        };
        if !is_claude_server_tool_type(&gjson_string(tool_type)) {
            continue;
        }
        let Some(name) = tool.get("name") else {
            continue;
        };
        let name = gjson_string(name);
        if !name.is_empty() {
            registry.insert(name, true);
        }
    }
    registry
}
