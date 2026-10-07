//! Actual Core state and shutdown-handler regressions; model responses remain fixtures.
use super::*;
use ctox_protocol::models::{ContentItem, ResponseItem};
use futures::FutureExt;

async fn owner(session: Arc<Session>, successful_loop: bool) -> Codex {
    let (tx_sub, _rx_sub) = async_channel::bounded(1);
    let (_tx_event, rx_event) = async_channel::unbounded();
    let (_tx_status, agent_status) = watch::channel(AgentStatus::PendingInit);
    let completion = futures::future::ready(if successful_loop { Ok(()) } else { Err(()) })
        .boxed()
        .shared();
    let _ = completion.clone().await;
    Codex {
        tx_sub,
        rx_event,
        agent_status,
        session,
        session_loop_termination: completion,
    }
}

#[tokio::test]
async fn native_session_state_requires_checked_handler_and_successful_loop() {
    let (session, _) = make_session_and_context().await;
    let session = Arc::new(session);
    let core = owner(Arc::clone(&session), true).await;
    assert!(core.capture_native_state().await.is_err());
    assert!(handlers::shutdown(&session, "native-state-test".into()).await);
    assert!(core.capture_native_state().await.is_ok());
    let failed = owner(session, false).await;
    assert!(failed.capture_native_state().await.is_err());
}

#[tokio::test]
async fn native_session_state_preserves_actual_compacted_context_without_authority_material() {
    let (session, _) = make_session_and_context().await;
    let item = ResponseItem::Message {
        id: None,
        role: "user".into(),
        content: vec![ContentItem::InputText {
            text: "exact post-compaction context".into(),
        }],
        end_turn: None,
        phase: None,
    };
    {
        let mut state = session.state.lock().await;
        state.replace_history(vec![item.clone()], None);
        state.session_configuration.base_instructions = "actual native base".into();
        state.session_configuration.user_instructions =
            Some("actual native user instructions".into());
        state.set_previous_turn_settings(Some(PreviousTurnSettings {
            model: "previous-actual-model".into(),
            realtime_active: Some(false),
        }));
        state
            .dependency_env
            .insert("fixture-secret-key".into(), "fixture-secret-value".into());
        state
            .active_connector_selection
            .extend(["connector-b".into(), "connector-a".into()]);
    }
    let session = Arc::new(session);
    assert!(handlers::shutdown(&session, "native-state-test".into()).await);
    let core = owner(Arc::clone(&session), true).await;
    let (config, exported) = core.capture_native_state().await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(exported.as_bytes()).unwrap();
    assert_eq!(exported.session_id(), session.conversation_id);
    assert_eq!(exported.model(), config.model);
    assert_eq!(parsed["history"], serde_json::json!([item]));
    assert_eq!(parsed["baseInstructions"], "actual native base");
    assert_eq!(
        parsed["userInstructions"],
        "actual native user instructions"
    );
    assert_eq!(parsed["previousTurn"]["model"], "previous-actual-model");
    assert_eq!(
        parsed["activeConnectorSelection"],
        serde_json::json!(["connector-a", "connector-b"])
    );
    assert_eq!(
        parsed["provider"]["conversationId"],
        session.conversation_id.to_string()
    );
    assert_eq!(parsed["targetAuthority"], "reauthorization-required");
    let text = std::str::from_utf8(exported.as_bytes()).unwrap();
    for excluded in [
        "fixture-secret-value",
        "fixture-secret-key",
        "Test API Key",
        "grantedPermissions",
        "authManager",
    ] {
        assert!(!text.contains(excluded), "{excluded}");
    }
    let (_, repeated) = core.capture_native_state().await.unwrap();
    assert_eq!(repeated.as_bytes(), exported.as_bytes());
}

#[tokio::test]
async fn native_session_state_rejects_foreign_model_client_and_ephemeral_sources() {
    let (mut session, _) = make_session_and_context().await;
    session.conversation_id = ThreadId::new();
    let session = Arc::new(session);
    assert!(handlers::shutdown(&session, "native-state-test".into()).await);
    assert!(
        owner(session, true)
            .await
            .capture_native_state()
            .await
            .is_err()
    );

    let (session, _) = make_session_and_context().await;
    {
        let mut state = session.state.lock().await;
        Arc::make_mut(&mut state.session_configuration.original_config_do_not_use).ephemeral = true;
    }
    let session = Arc::new(session);
    assert!(handlers::shutdown(&session, "native-state-test".into()).await);
    assert!(
        owner(session, true)
            .await
            .capture_native_state()
            .await
            .is_err()
    );
}
