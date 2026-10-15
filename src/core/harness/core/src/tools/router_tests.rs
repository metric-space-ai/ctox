use std::sync::Arc;

use crate::codex::make_session_and_context;
use crate::tools::context::ToolPayload;
use ctox_protocol::models::ResponseItem;

use super::ToolRouter;
use super::is_removed_free_subagent_tool;

#[test]
fn all_free_subagent_tool_names_are_reserved_and_removed() {
    for name in [
        "spawn_agent",
        "spawn_agents_on_csv",
        "send_input",
        "list_agents",
        "resume_agent",
        "wait_agent",
        "close_agent",
        "report_agent_job_result",
    ] {
        assert!(is_removed_free_subagent_tool(name), "{name}");
    }
    assert!(!is_removed_free_subagent_tool("ctox_deep_research"));
}



#[tokio::test]
async fn build_tool_call_uses_namespace_for_registry_name() -> anyhow::Result<()> {
    let (session, _) = make_session_and_context().await;
    let session = Arc::new(session);
    let tool_name = "create_event".to_string();

    let call = ToolRouter::build_tool_call(
        &session,
        ResponseItem::FunctionCall {
            id: None,
            name: tool_name.clone(),
            namespace: Some("mcp__codex_apps__calendar".to_string()),
            arguments: "{}".to_string(),
            call_id: "call-namespace".to_string(),
        },
    )
    .await?
    .expect("function_call should produce a tool call");

    assert_eq!(call.tool_name, tool_name);
    assert_eq!(
        call.tool_namespace,
        Some("mcp__codex_apps__calendar".to_string())
    );
    assert_eq!(call.call_id, "call-namespace");
    match call.payload {
        ToolPayload::Function { arguments } => {
            assert_eq!(arguments, "{}");
        }
        other => panic!("expected function payload, got {other:?}"),
    }

    Ok(())
}
