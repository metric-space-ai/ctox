use super::*;
use crate::codex::make_session_and_context;
use crate::config::ApprovalsReviewer;
use crate::config::ConfigToml;
use crate::config::types::AppConfig;
use crate::config::types::AppToolConfig;
use crate::config::types::AppToolsConfig;
use crate::config::types::AppsConfigToml;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_once;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use ctox_config::CONFIG_TOML_FILE;
use pretty_assertions::assert_eq;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::tempdir;

fn annotations(
    read_only: Option<bool>,
    destructive: Option<bool>,
    open_world: Option<bool>,
) -> ToolAnnotations {
    ToolAnnotations::from_raw(None, read_only, destructive, None, open_world)
}

fn approval_metadata(
    connector_id: Option<&str>,
    connector_name: Option<&str>,
    connector_description: Option<&str>,
    tool_title: Option<&str>,
    tool_description: Option<&str>,
) -> McpToolApprovalMetadata {
    McpToolApprovalMetadata {
        annotations: None,
        connector_id: connector_id.map(str::to_string),
        connector_name: connector_name.map(str::to_string),
        connector_description: connector_description.map(str::to_string),
        tool_title: tool_title.map(str::to_string),
        tool_description: tool_description.map(str::to_string),
        codex_apps_meta: None,
    }
}

fn prompt_options(
    allow_session_remember: bool,
    allow_persistent_approval: bool,
) -> McpToolApprovalPromptOptions {
    McpToolApprovalPromptOptions {
        allow_session_remember,
        allow_persistent_approval,
    }
}

struct NativeEmissionProbe {
    thread: String,
    turn: String,
    calls: std::sync::atomic::AtomicUsize,
}

impl crate::native_mcp_dispatch::NativeMcpDispatch for NativeEmissionProbe {
    fn dispatch(
        &self,
        invocation: crate::native_mcp_dispatch::NativeMcpInvocation<'_>,
    ) -> Option<Result<CallToolResult, String>> {
        assert_eq!(invocation.thread_id(), self.thread);
        assert_eq!(invocation.turn_id(), self.turn);
        assert_eq!(invocation.call_id(), "actual-call");
        assert_eq!(invocation.server(), "native-test");
        assert_eq!(invocation.tool(), "actual-tool");
        // Model-provided labels remain arguments, never dispatch identity.
        assert_eq!(
            invocation.arguments(),
            Some(&serde_json::json!({"thread_id": "forged", "turn_id": "forged"})),
        );
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Some(Ok(CallToolResult {
            content: vec![],
            structured_content: Some(serde_json::json!({"native": true})),
            is_error: None,
            meta: None,
        }))
    }
}

#[tokio::test]
async fn native_mcp_emission_uses_actual_session_and_turn_after_argument_validation() {
    let (session, turn, _events) = crate::codex::make_session_and_context_with_rx().await;
    let probe = Arc::new(NativeEmissionProbe {
        thread: session.conversation_id.to_string(),
        turn: turn.sub_id.clone(),
        calls: std::sync::atomic::AtomicUsize::new(0),
    });
    let registration =
        crate::native_mcp_dispatch::register_native_mcp_dispatch(&session, probe.clone()).unwrap();
    crate::native_mcp_dispatch::tests::start_live_turn(&session, &turn).await;
    let invalid = handle_mcp_tool_call(
        session.clone(),
        &turn,
        "actual-call".into(),
        "native-test".into(),
        "actual-tool".into(),
        "{".into(),
    )
    .await;
    assert_eq!(invalid.is_error, Some(true));
    assert_eq!(probe.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    let result = handle_mcp_tool_call(
        session.clone(),
        &turn,
        "actual-call".into(),
        "native-test".into(),
        "actual-tool".into(),
        r#"{"thread_id":"forged","turn_id":"forged"}"#.into(),
    )
    .await;
    assert_eq!(
        result.structured_content,
        Some(serde_json::json!({"native": true}))
    );
    assert_eq!(probe.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(session.abort_turn(&turn.sub_id).await);
    drop(registration);
    let absent = dispatch_mcp_tool(
        &session,
        &turn,
        "actual-call",
        "native-test",
        "actual-tool",
        None,
        None,
    )
    .await;
    assert!(
        absent.is_err(),
        "unregistered call should use the absent normal MCP server"
    );
    assert_eq!(probe.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn approval_required_when_read_only_false_and_destructive() {
    let annotations = annotations(Some(false), Some(true), None);
    assert_eq!(requires_mcp_tool_approval(&annotations), true);
}

#[test]
fn approval_required_when_read_only_false_and_open_world() {
    let annotations = annotations(Some(false), None, Some(true));
    assert_eq!(requires_mcp_tool_approval(&annotations), true);
}

#[test]
fn approval_required_when_destructive_even_if_read_only_true() {
    let annotations = annotations(Some(true), Some(true), Some(true));
    assert_eq!(requires_mcp_tool_approval(&annotations), true);
}

#[test]
fn prompt_mode_does_not_allow_persistent_remember() {
    assert_eq!(
        normalize_approval_decision_for_mode(
            McpToolApprovalDecision::AcceptForSession,
            AppToolApproval::Prompt,
        ),
        McpToolApprovalDecision::Accept
    );
    assert_eq!(
        normalize_approval_decision_for_mode(
            McpToolApprovalDecision::AcceptAndRemember,
            AppToolApproval::Prompt,
        ),
        McpToolApprovalDecision::Accept
    );
}










#[test]
fn sanitize_mcp_tool_result_for_model_rewrites_image_content() {
    let result = Ok(CallToolResult {
        content: vec![
            serde_json::json!({
                "type": "image",
                "data": "Zm9v",
                "mimeType": "image/png",
            }),
            serde_json::json!({
                "type": "text",
                "text": "hello",
            }),
        ],
        structured_content: None,
        is_error: Some(false),
        meta: None,
    });

    let got = sanitize_mcp_tool_result_for_model(false, result).expect("sanitized result");

    assert_eq!(
        got.content,
        vec![
            serde_json::json!({
                "type": "text",
                "text": "<image content omitted because you do not support image input>",
            }),
            serde_json::json!({
                "type": "text",
                "text": "hello",
            }),
        ]
    );
}

#[test]
fn sanitize_mcp_tool_result_for_model_preserves_image_when_supported() {
    let original = CallToolResult {
        content: vec![serde_json::json!({
            "type": "image",
            "data": "Zm9v",
            "mimeType": "image/png",
        })],
        structured_content: Some(serde_json::json!({"x": 1})),
        is_error: Some(false),
        meta: Some(serde_json::json!({"k": "v"})),
    };

    let got =
        sanitize_mcp_tool_result_for_model(true, Ok(original.clone())).expect("unsanitized result");

    assert_eq!(got, original);
}











#[test]
fn declined_elicitation_response_stays_decline() {
    let response = parse_mcp_tool_approval_elicitation_response(
        Some(ElicitationResponse {
            action: ElicitationAction::Decline,
            content: Some(serde_json::json!({
                "approval": MCP_TOOL_APPROVAL_ACCEPT,
            })),
            meta: None,
        }),
        "approval",
    );

    assert_eq!(response, McpToolApprovalDecision::Decline);
}

#[test]
fn synthetic_decline_request_user_input_response_stays_decline() {
    let response = parse_mcp_tool_approval_response(
        Some(RequestUserInputResponse {
            answers: HashMap::from([(
                "approval".to_string(),
                RequestUserInputAnswer {
                    answers: vec![MCP_TOOL_APPROVAL_DECLINE_SYNTHETIC.to_string()],
                },
            )]),
        }),
        "approval",
    );

    assert_eq!(response, McpToolApprovalDecision::Decline);
}

#[test]
fn accepted_elicitation_response_uses_always_persist_meta() {
    let response = parse_mcp_tool_approval_elicitation_response(
        Some(ElicitationResponse {
            action: ElicitationAction::Accept,
            content: None,
            meta: Some(serde_json::json!({
                MCP_TOOL_APPROVAL_PERSIST_KEY: MCP_TOOL_APPROVAL_PERSIST_ALWAYS,
            })),
        }),
        "approval",
    );

    assert_eq!(response, McpToolApprovalDecision::AcceptAndRemember);
}

#[test]
fn accepted_elicitation_response_uses_session_persist_meta() {
    let response = parse_mcp_tool_approval_elicitation_response(
        Some(ElicitationResponse {
            action: ElicitationAction::Accept,
            content: None,
            meta: Some(serde_json::json!({
                MCP_TOOL_APPROVAL_PERSIST_KEY: MCP_TOOL_APPROVAL_PERSIST_SESSION,
            })),
        }),
        "approval",
    );

    assert_eq!(response, McpToolApprovalDecision::AcceptForSession);
}

#[test]
fn accepted_elicitation_without_content_defaults_to_accept() {
    let response = parse_mcp_tool_approval_elicitation_response(
        Some(ElicitationResponse {
            action: ElicitationAction::Accept,
            content: None,
            meta: None,
        }),
        "approval",
    );

    assert_eq!(response, McpToolApprovalDecision::Accept);
}



#[tokio::test]
async fn approve_mode_skips_when_annotations_do_not_require_approval() {
    let (session, turn_context) = make_session_and_context().await;
    let session = Arc::new(session);
    let turn_context = Arc::new(turn_context);
    let invocation = McpInvocation {
        server: "custom_server".to_string(),
        tool: "read_only_tool".to_string(),
        arguments: None,
    };
    let metadata = McpToolApprovalMetadata {
        annotations: Some(annotations(Some(true), None, None)),
        connector_id: None,
        connector_name: None,
        connector_description: None,
        tool_title: Some("Read Only Tool".to_string()),
        tool_description: None,
        codex_apps_meta: None,
    };

    let decision = maybe_request_mcp_tool_approval(
        &session,
        &turn_context,
        "call-1",
        &invocation,
        Some(&metadata),
        AppToolApproval::Approve,
    )
    .await;

    assert_eq!(decision, None);
}
