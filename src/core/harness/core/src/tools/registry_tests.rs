use super::*;
use crate::tools::context::ToolInvocation;
use async_trait::async_trait;
use pretty_assertions::assert_eq;

struct TestHandler;

#[async_trait]
impl ToolHandler for TestHandler {
    type Output = crate::tools::context::FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, _invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        unreachable!("test handler should not be invoked")
    }
}

#[test]
fn handler_looks_up_namespaced_aliases_explicitly() {
    let plain_handler = Arc::new(TestHandler) as Arc<dyn AnyToolHandler>;
    let namespaced_handler = Arc::new(TestHandler) as Arc<dyn AnyToolHandler>;
    let namespace = "mcp__codex_apps__gmail";
    let tool_name = "gmail_get_recent_emails";
    let namespaced_name = tool_handler_key(tool_name, Some(namespace));
    let registry = ToolRegistry::new(HashMap::from([
        (tool_name.to_string(), Arc::clone(&plain_handler)),
        (namespaced_name, Arc::clone(&namespaced_handler)),
    ]));

    let plain = registry.handler(tool_name, None);
    let namespaced = registry.handler(tool_name, Some(namespace));
    let missing_namespaced = registry.handler(tool_name, Some("mcp__codex_apps__calendar"));

    assert_eq!(plain.is_some(), true);
    assert_eq!(namespaced.is_some(), true);
    assert_eq!(missing_namespaced.is_none(), true);
    assert!(
        plain
            .as_ref()
            .is_some_and(|handler| Arc::ptr_eq(handler, &plain_handler))
    );
    assert!(
        namespaced
            .as_ref()
            .is_some_and(|handler| Arc::ptr_eq(handler, &namespaced_handler))
    );
}

#[tokio::test]
async fn native_effect_observation_precedes_mcp_metadata_await_and_survives_cancellation() {
    use futures::FutureExt;
    let (session, turn, _events) = crate::codex::make_session_and_context_with_rx().await;
    session.native_effects.register_source_factory().unwrap();
    let registry = ToolRegistry::new(HashMap::new());
    let manager = session.services.mcp_connection_manager.write().await;
    let invocation = ToolInvocation {
        session: Arc::clone(&session),
        turn,
        tracker: Arc::new(tokio::sync::Mutex::new(
            crate::turn_diff_tracker::TurnDiffTracker::new(),
        )),
        call_id: "pending-mcp-metadata".into(),
        tool_name: "unknown".into(),
        tool_namespace: None,
        payload: ToolPayload::Mcp {
            server: "unknown".into(),
            tool: "unknown".into(),
            raw_arguments: "{}".into(),
        },
    };
    // Polling reaches the held production await, then cancels the dispatch.
    assert!(
        registry
            .dispatch_any(invocation.clone())
            .now_or_never()
            .is_none()
    );
    let observed = serde_json::to_value(
        session
            .native_effects
            .capture(session.conversation_id)
            .unwrap()
            .report(),
    )
    .unwrap();
    assert_eq!(observed["unreconciledObservations"], 1);
    drop(manager);
    assert!(registry.dispatch_any(invocation).await.is_err());
    let rejected = serde_json::to_value(
        session
            .native_effects
            .capture(session.conversation_id)
            .unwrap()
            .report(),
    )
    .unwrap();
    assert_eq!(rejected["unreconciledObservations"], 2);
    assert!(session.native_effects.register_source_factory().is_err());
}
