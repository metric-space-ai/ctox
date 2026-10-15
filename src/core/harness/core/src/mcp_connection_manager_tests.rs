use super::*;
use ctox_protocol::protocol::GranularApprovalConfig;
use rmcp::model::JsonObject;
use std::collections::HashSet;
use std::sync::Arc;

fn create_test_tool(server_name: &str, tool_name: &str) -> ToolInfo {
    ToolInfo {
        server_name: server_name.to_string(),
        tool_name: tool_name.to_string(),
        tool_namespace: if server_name == CODEX_APPS_MCP_SERVER_NAME {
            format!("mcp__{server_name}__")
        } else {
            server_name.to_string()
        },
        tool: Tool::new(
            tool_name.to_string(),
            format!("Test tool: {tool_name}"),
            Arc::new(JsonObject::default()),
        ),
        connector_id: None,
        connector_name: None,
        plugin_display_names: Vec::new(),
        connector_description: None,
    }
}



#[test]
fn elicitation_granular_policy_defaults_to_prompting() {
    assert!(!elicitation_is_rejected_by_policy(
        AskForApproval::OnFailure
    ));
    assert!(!elicitation_is_rejected_by_policy(
        AskForApproval::OnRequest
    ));
    assert!(!elicitation_is_rejected_by_policy(
        AskForApproval::UnlessTrusted
    ));
    assert!(elicitation_is_rejected_by_policy(AskForApproval::Granular(
        GranularApprovalConfig {
            sandbox_approval: true,
            rules: true,
            skill_approval: true,
            request_permissions: true,
            mcp_elicitations: false,
        }
    )));
}

#[test]
fn elicitation_granular_policy_respects_never_and_config() {
    assert!(elicitation_is_rejected_by_policy(AskForApproval::Never));
    assert!(elicitation_is_rejected_by_policy(AskForApproval::Granular(
        GranularApprovalConfig {
            sandbox_approval: true,
            rules: true,
            skill_approval: true,
            request_permissions: true,
            mcp_elicitations: false,
        }
    )));
}

#[test]
fn test_qualify_tools_short_non_duplicated_names() {
    let tools = vec![
        create_test_tool("server1", "tool1"),
        create_test_tool("server1", "tool2"),
    ];

    let qualified_tools = qualify_tools(tools);

    assert_eq!(qualified_tools.len(), 2);
    assert!(qualified_tools.contains_key("mcp__server1__tool1"));
    assert!(qualified_tools.contains_key("mcp__server1__tool2"));
}

#[test]
fn test_qualify_tools_duplicated_names_skipped() {
    let tools = vec![
        create_test_tool("server1", "duplicate_tool"),
        create_test_tool("server1", "duplicate_tool"),
    ];

    let qualified_tools = qualify_tools(tools);

    // Only the first tool should remain, the second is skipped
    assert_eq!(qualified_tools.len(), 1);
    assert!(qualified_tools.contains_key("mcp__server1__duplicate_tool"));
}

#[test]
fn test_qualify_tools_long_names_same_server() {
    let server_name = "my_server";

    let tools = vec![
        create_test_tool(
            server_name,
            "extremely_lengthy_function_name_that_absolutely_surpasses_all_reasonable_limits",
        ),
        create_test_tool(
            server_name,
            "yet_another_extremely_lengthy_function_name_that_absolutely_surpasses_all_reasonable_limits",
        ),
    ];

    let qualified_tools = qualify_tools(tools);

    assert_eq!(qualified_tools.len(), 2);

    let mut keys: Vec<_> = qualified_tools.keys().cloned().collect();
    keys.sort();

    assert_eq!(keys[0].len(), 64);
    assert_eq!(
        keys[0],
        "mcp__my_server__extremel119a2b97664e41363932dc84de21e2ff1b93b3e9"
    );

    assert_eq!(keys[1].len(), 64);
    assert_eq!(
        keys[1],
        "mcp__my_server__yet_anot419a82a89325c1b477274a41f8c65ea5f3a7f341"
    );
}

#[test]
fn test_qualify_tools_sanitizes_invalid_characters() {
    let tools = vec![create_test_tool("server.one", "tool.two-three")];

    let qualified_tools = qualify_tools(tools);

    assert_eq!(qualified_tools.len(), 1);
    let (qualified_name, tool) = qualified_tools.into_iter().next().expect("one tool");
    assert_eq!(qualified_name, "mcp__server_one__tool_two_three");

    // The key is sanitized for OpenAI, but we keep original parts for the actual MCP call.
    assert_eq!(tool.server_name, "server.one");
    assert_eq!(tool.tool_name, "tool.two-three");

    assert!(
        qualified_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
        "qualified name must be Responses API compatible: {qualified_name:?}"
    );
}

#[test]
fn tool_filter_allows_by_default() {
    let filter = ToolFilter::default();

    assert!(filter.allows("any"));
}

#[test]
fn tool_filter_applies_enabled_list() {
    let filter = ToolFilter {
        enabled: Some(HashSet::from(["allowed".to_string()])),
        disabled: HashSet::new(),
    };

    assert!(filter.allows("allowed"));
    assert!(!filter.allows("denied"));
}

#[test]
fn tool_filter_applies_disabled_list() {
    let filter = ToolFilter {
        enabled: None,
        disabled: HashSet::from(["blocked".to_string()]),
    };

    assert!(!filter.allows("blocked"));
    assert!(filter.allows("open"));
}

#[test]
fn tool_filter_applies_enabled_then_disabled() {
    let filter = ToolFilter {
        enabled: Some(HashSet::from(["keep".to_string(), "remove".to_string()])),
        disabled: HashSet::from(["remove".to_string()]),
    };

    assert!(filter.allows("keep"));
    assert!(!filter.allows("remove"));
    assert!(!filter.allows("unknown"));
}

#[test]
fn filter_tools_applies_per_server_filters() {
    let server1_tools = vec![
        create_test_tool("server1", "tool_a"),
        create_test_tool("server1", "tool_b"),
    ];
    let server2_tools = vec![create_test_tool("server2", "tool_a")];
    let server1_filter = ToolFilter {
        enabled: Some(HashSet::from(["tool_a".to_string(), "tool_b".to_string()])),
        disabled: HashSet::from(["tool_b".to_string()]),
    };
    let server2_filter = ToolFilter {
        enabled: None,
        disabled: HashSet::from(["tool_a".to_string()]),
    };

    let filtered: Vec<_> = filter_tools(server1_tools, &server1_filter)
        .into_iter()
        .chain(filter_tools(server2_tools, &server2_filter))
        .collect();

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].server_name, "server1");
    assert_eq!(filtered[0].tool_name, "tool_a");
}
