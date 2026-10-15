// ref: internal/runtime/executor/claude_executor_request_remap_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::remap_claude_oauth_tool_names_with_secret;

#[test]
fn declared_tool_and_history_receive_same_mcp_alias() {
    let body = br#"{"tools":[{"name":"fetch_url"}],"messages":[{"role":"assistant","content":[{"type":"tool_use","name":"fetch_url"}]}]}"#;
    let (mapped, reverse) = remap_claude_oauth_tool_names_with_secret(body, "caller-a");
    let root: serde_json::Value = serde_json::from_slice(&mapped).unwrap();
    let alias = root
        .pointer("/tools/0/name")
        .and_then(serde_json::Value::as_str)
        .unwrap();
    assert!(alias.starts_with("mcp__"));
    assert_eq!(
        root.pointer("/messages/0/content/0/name")
            .and_then(serde_json::Value::as_str),
        Some(alias)
    );
    assert_eq!(reverse.get(alias).map(String::as_str), Some("fetch_url"));
}

#[test]
fn typed_custom_tools_are_aliased_but_server_tools_remain_native() {
    let body = br#"{"tools":[{"name":"custom","type":"custom"},{"name":"native","type":"web_search_20250305"}],"messages":[{"role":"assistant","content":[{"type":"tool_use","name":"custom"},{"type":"tool_use","name":"native"}]}]}"#;
    let (mapped, reverse) = remap_claude_oauth_tool_names_with_secret(body, "caller-a");
    let root: serde_json::Value = serde_json::from_slice(&mapped).unwrap();
    let alias = root
        .pointer("/tools/0/name")
        .and_then(serde_json::Value::as_str)
        .unwrap();
    assert!(alias.starts_with("mcp__"));
    assert_eq!(root["tools"][1]["name"], "native");
    assert_eq!(root["messages"][0]["content"][0]["name"], alias);
    assert_eq!(root["messages"][0]["content"][1]["name"], "native");
    assert_eq!(reverse.get(alias).map(String::as_str), Some("custom"));
    assert!(!reverse.values().any(|name| name == "native"));
}

#[test]
fn server_tool_name_protects_ambiguous_history_reference() {
    let body = br#"{"tools":[{"name":"shared","type":"custom"},{"name":"shared","type":"web_search_20250305"}],"messages":[{"role":"assistant","content":[{"type":"tool_use","name":"shared"}]}]}"#;
    let (mapped, reverse) = remap_claude_oauth_tool_names_with_secret(body, "caller-a");
    let root: serde_json::Value = serde_json::from_slice(&mapped).unwrap();
    assert_ne!(root["tools"][0]["name"], "shared");
    assert_eq!(root["tools"][1]["name"], "shared");
    assert_eq!(root["messages"][0]["content"][0]["name"], "shared");
    assert_eq!(reverse.len(), 1);
}

fn upstream_mcp_request(
    stream: bool,
) -> (
    crate::internal::runtime::executor::claude_executor::ClaudeMessagesRequest,
    String,
) {
    use crate::internal::runtime::executor::claude_executor::{
        ClaudeCredentialMode, ClaudeMessagesRequest, ClaudeUpstreamTarget,
    };
    let body=br#"{"tools":[{"name":"Bash"},{"name":"mcp__docs-search__search-pages"},{"name":"mcp__docs-search__search-issues"}]}"#;
    let (body, reverse) = remap_claude_oauth_tool_names_with_secret(body, "fixture-oauth");
    let server = reverse
        .keys()
        .next()
        .unwrap()
        .strip_prefix("mcp__")
        .unwrap()
        .split_once("__")
        .unwrap()
        .0
        .to_owned();
    let request = ClaudeMessagesRequest::new(
        ClaudeUpstreamTarget::new("https", "api.anthropic.com").unwrap(),
        ClaudeCredentialMode::OAuth,
        &crate::internal::auth::claude::SecretString::new("fixture-oauth".to_owned()).unwrap(),
        body,
        stream,
    )
    .unwrap()
    .with_upstream_metadata(Vec::new(), reverse);
    (request, server)
}
#[test]
fn upstream_mcp_wrapped_passthrough_unary_returns_declared_name() {
    use crate::internal::runtime::executor::{
        claude_executor::ClaudeMessagesResponse,
        claude_executor_buffered::prepare_claude_buffered_response,
    };
    let (request, server) = upstream_mcp_request(false);
    for suffix in [
        "mcp__docs-search__search-pages",
        "docs-search__search-pages",
        "search-pages",
    ] {
        let body=serde_json::to_vec(&serde_json::json!({"content":[{"type":"tool_use","name":format!("mcp__{server}__{suffix}")}]})).unwrap();
        let response =
            prepare_claude_buffered_response(ClaudeMessagesResponse::new(200, body), &request)
                .unwrap();
        assert_eq!(response.status(), 200);
        let body: serde_json::Value = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body["content"][0]["name"], "mcp__docs-search__search-pages");
    }
}
#[test]
fn upstream_mcp_wrapped_passthrough_buffered_sse_returns_declared_name() {
    use crate::internal::runtime::executor::{
        claude_executor::ClaudeMessagesResponse,
        claude_executor_buffered::prepare_claude_buffered_response,
    };
    let (request, server) = upstream_mcp_request(true);
    let catalog = crate::internal::registry::embedded_models_catalog().unwrap();
    let model = crate::internal::registry::models_for_channel(&catalog, "claude")
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
        .id;
    for suffix in [
        "mcp__docs-search__search-pages",
        "docs-search__search-pages",
        "search-pages",
    ] {
        let frames = [
            serde_json::json!({"type":"message_start","message":{"id":"fixture-message","model":model}}),
            serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","name":format!("mcp__{server}__{suffix}")}}),
            serde_json::json!({"type":"message_delta","delta":{"stop_reason":"tool_use"}}),
        ];
        let body = frames
            .iter()
            .map(|v| format!("data: {v}\n\n"))
            .collect::<String>()
            .into_bytes();
        let response =
            prepare_claude_buffered_response(ClaudeMessagesResponse::new(200, body), &request)
                .unwrap();
        assert_eq!(response.status(), 200);
        let body = String::from_utf8(response.body().to_vec()).unwrap();
        assert!(body.contains("\"name\":\"mcp__docs-search__search-pages\""));
        assert!(!body.contains(&format!("mcp__{server}__")));
    }
}
