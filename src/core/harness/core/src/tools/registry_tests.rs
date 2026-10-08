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

fn native_plan_invocation(
    session: &Arc<crate::codex::Session>,
    turn: Arc<crate::codex::TurnContext>,
    call_id: &str,
    arguments: &str,
) -> ToolInvocation {
    ToolInvocation {
        session: session.clone(),
        turn,
        tracker: Arc::new(tokio::sync::Mutex::new(
            crate::turn_diff_tracker::TurnDiffTracker::new(),
        )),
        call_id: call_id.into(),
        tool_name: "update_plan".into(),
        tool_namespace: None,
        payload: ToolPayload::Function {
            arguments: arguments.into(),
        },
    }
}
fn observe_native_plan(session: &crate::codex::Session, call_id: &str, name: &str) {
    let item = ctox_protocol::models::ResponseItem::FunctionCall {
        id: None,
        name: name.into(),
        namespace: None,
        arguments: "{}".into(),
        call_id: call_id.into(),
    };
    session.native_effects.observe_provider_item(&item, false);
    session.native_effects.observe_provider_item(&item, true);
}
fn pending_native_effects(session: &crate::codex::Session) -> u64 {
    serde_json::to_value(
        session
            .native_effects
            .capture(session.conversation_id)
            .unwrap()
            .report(),
    )
    .unwrap()["unreconciledObservations"]
        .as_u64()
        .unwrap()
}

#[tokio::test]
async fn actual_plan_handler_owns_only_its_matching_native_effect() {
    let (session, turn, _events) = crate::codex::make_session_and_context_with_rx().await;
    session.native_effects.register_source_factory().unwrap();
    let registry = ToolRegistry::new(HashMap::from([(
        "update_plan".into(),
        Arc::new(crate::tools::handlers::PlanHandler) as Arc<dyn AnyToolHandler>,
    )]));
    session.native_effects.observe_unreconciled(); // unrelated host/tool effect
    observe_native_plan(&session, "owned-plan", "update_plan");
    let arguments = r#"{"plan":[{"step":"retain original state","status":"completed"}]}"#;
    registry
        .dispatch_any(native_plan_invocation(
            &session,
            turn.clone(),
            "owned-plan",
            arguments,
        ))
        .await
        .unwrap();
    assert_eq!(
        pending_native_effects(&session),
        1,
        "unrelated effect was erased"
    );
    // A completed call ID cannot be adopted by a second dispatch or next turn.
    observe_native_plan(&session, "owned-plan", "update_plan");
    registry
        .dispatch_any(native_plan_invocation(
            &session,
            turn,
            "owned-plan",
            arguments,
        ))
        .await
        .unwrap();
    assert_eq!(pending_native_effects(&session), 4);
}

struct PretendPlan;
#[async_trait]
impl ToolHandler for PretendPlan {
    type Output = crate::tools::context::FunctionToolOutput;
    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }
    async fn handle(&self, _: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        Ok(crate::tools::context::FunctionToolOutput::from_text(
            "Plan updated".into(),
            Some(true),
        ))
    }
}
#[tokio::test]
async fn plan_name_success_and_foreign_provider_item_do_not_mint_native_receipts() {
    let (session, turn, _events) = crate::codex::make_session_and_context_with_rx().await;
    session.native_effects.register_source_factory().unwrap();
    observe_native_plan(&session, "pretend-plan", "update_plan");
    let registry = ToolRegistry::new(HashMap::from([(
        "update_plan".into(),
        Arc::new(PretendPlan) as Arc<dyn AnyToolHandler>,
    )]));
    registry
        .dispatch_any(native_plan_invocation(
            &session,
            turn.clone(),
            "pretend-plan",
            "{}",
        ))
        .await
        .unwrap();
    assert_eq!(pending_native_effects(&session), 3);
    let actual = ToolRegistry::new(HashMap::from([(
        "update_plan".into(),
        Arc::new(crate::tools::handlers::PlanHandler) as Arc<dyn AnyToolHandler>,
    )]));
    observe_native_plan(&session, "foreign-item", "unknown");
    actual
        .dispatch_any(native_plan_invocation(
            &session,
            turn,
            "foreign-item",
            r#"{"plan":[{"step":"retain original state","status":"completed"}]}"#,
        ))
        .await
        .unwrap();
    assert_eq!(pending_native_effects(&session), 6);
}
#[tokio::test]
async fn rejected_or_cancelled_plan_stays_unknown_on_the_real_dispatch_path() {
    use futures::FutureExt;
    let (session, turn, _events) = crate::codex::make_session_and_context_with_rx().await;
    session.native_effects.register_source_factory().unwrap();
    let registry = ToolRegistry::new(HashMap::from([(
        "update_plan".into(),
        Arc::new(crate::tools::handlers::PlanHandler) as Arc<dyn AnyToolHandler>,
    )]));
    observe_native_plan(&session, "invalid-plan", "update_plan");
    assert!(
        registry
            .dispatch_any(native_plan_invocation(
                &session,
                turn.clone(),
                "invalid-plan",
                "not-json"
            ))
            .await
            .is_err()
    );
    assert_eq!(pending_native_effects(&session), 3);
    observe_native_plan(&session, "cancelled-plan", "update_plan");
    let active = session.active_turn.lock().await;
    assert!(
        registry
            .dispatch_any(native_plan_invocation(
                &session,
                turn,
                "cancelled-plan",
                "{}"
            ))
            .now_or_never()
            .is_none()
    );
    drop(active);
    assert_eq!(pending_native_effects(&session), 6);
}
