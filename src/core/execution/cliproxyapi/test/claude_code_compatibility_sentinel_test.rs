// ref: test/claude_code_compatibility_sentinel_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::{json, Map, Value};

fn tool_progress_fixture() -> Value {
    json!({
        "type": "tool_progress",
        "tool_use_id": "toolu_123",
        "tool_name": "Bash",
        "parent_tool_use_id": null,
        "elapsed_time_seconds": 2.5,
        "task_id": "task_123",
        "uuid": "11111111-1111-4111-8111-111111111111",
        "session_id": "sess_123"
    })
}

fn session_state_changed_fixture() -> Value {
    json!({
        "type": "system",
        "subtype": "session_state_changed",
        "state": "requires_action",
        "uuid": "22222222-2222-4222-8222-222222222222",
        "session_id": "sess_123"
    })
}

fn tool_use_summary_fixture() -> Value {
    json!({
        "type": "tool_use_summary",
        "summary": "Searched in auth/",
        "preceding_tool_use_ids": ["toolu_1", "toolu_2"],
        "uuid": "33333333-3333-4333-8333-333333333333",
        "session_id": "sess_123"
    })
}

fn control_request_can_use_tool_fixture() -> Value {
    json!({
        "type": "control_request",
        "request_id": "req_123",
        "request": {
            "subtype": "can_use_tool",
            "tool_name": "Bash",
            "input": {"command": "npm test"},
            "tool_use_id": "toolu_123",
            "description": "Running npm test"
        }
    })
}

fn object(value: &Value) -> &Map<String, Value> {
    value
        .as_object()
        .expect("sentinel fixture must be an object")
}

fn required_string<'a>(object: &'a Map<String, Value>, key: &str) -> &'a str {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| panic!("field {key:?} missing or empty"))
}
