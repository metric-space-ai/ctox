use super::*;
use core_test_support::responses::{
    ev_assistant_message, ev_completed, ev_function_call, mount_sse_sequence, sse,
};
use serde_json::json;
use wiremock::MockServer;

async fn fixture(
    server: &MockServer,
) -> (
    Arc<Session>,
    Arc<TurnContext>,
    async_channel::Receiver<Event>,
) {
    let (mut session, mut context, events) = make_session_and_context_with_rx().await;
    let provider = crate::model_provider_info::create_oss_provider_with_base_url(
        &server.uri(),
        crate::model_provider_info::WireApi::Responses,
    );
    let session_mut = Arc::get_mut(&mut session).expect("unshared fixture session");
    session_mut.services.model_client = ModelClient::new(
        None,
        session_mut.conversation_id,
        provider.clone(),
        SessionSource::Exec,
        None,
        false,
        false,
        false,
        None,
    );
    let context_mut = Arc::get_mut(&mut context).expect("unshared fixture context");
    context_mut.provider = provider;
    context_mut.required_initial_tool = Some("update_plan".to_string());
    // The default unit fixture has a read-only tool surface, which omits
    // update_plan. Exercise the planning-capable service turn surface here.
    context_mut.tools_config.read_only_surface = false;
    (session, context, events)
}

fn answer(id: &str, text: &str) -> String {
    sse(vec![ev_assistant_message(id, text), ev_completed(id)])
}

fn plan() -> String {
    sse(vec![
        ev_function_call(
            "current-plan",
            "update_plan",
            r#"{"plan":[{"step":"Calculate the sum","status":"completed"}]}"#,
        ),
        ev_completed("planned"),
    ])
}

async fn execute(
    session: Arc<Session>,
    context: Arc<TurnContext>,
    cancellation: CancellationToken,
) -> Option<String> {
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        run_turn(
            session,
            context,
            vec![UserInput::Text {
                text: "What is 19 plus 8?".to_string(),
                text_elements: Vec::new(),
            }],
            None,
            cancellation,
        ),
    )
    .await
    .expect("bounded mock turn")
}

#[tokio::test]
async fn required_initial_tool_recovers_text_only_response_in_same_turn() {
    let server = MockServer::start().await;
    let mock = mount_sse_sequence(
        &server,
        vec![
            answer("premature", "27"),
            plan(),
            answer("final", "The sum is 27."),
        ],
    )
    .await;
    let (session, context, events) = fixture(&server).await;
    assert_eq!(
        execute(session, context, CancellationToken::new())
            .await
            .as_deref(),
        Some("The sum is 27.")
    );
    let requests = mock.requests();
    assert_eq!(requests.len(), 3);
    for request in &requests[..2] {
        let body = request.body_json();
        assert_eq!(body["tool_choice"], "auto");
        assert_eq!(body["tools"].as_array().unwrap().len(), 1);
        assert_eq!(body["tools"][0]["name"], "update_plan");
    }
    assert!(
        requests[1].body_json()["input"]
            .to_string()
            .contains("correction 1 of 2")
    );
    assert!(requests[2].body_json()["tools"].as_array().unwrap().len() > 1);
    let mut plans = 0;
    while let Ok(event) = events.try_recv() {
        if matches!(event.msg, EventMsg::PlanUpdate(_)) {
            plans += 1;
        }
    }
    assert_eq!(plans, 1, "only the real model plan produces a plan event");
}

#[tokio::test]
async fn required_initial_tool_repeated_refusal_fails_after_two_corrections() {
    let server = MockServer::start().await;
    let mock = mount_sse_sequence(
        &server,
        vec![
            answer("one", "27"),
            answer("two", "27"),
            answer("three", "27"),
        ],
    )
    .await;
    let (session, context, events) = fixture(&server).await;
    assert!(
        execute(session, context, CancellationToken::new())
            .await
            .is_none()
    );
    assert_eq!(mock.requests().len(), 3);
    let mut error = false;
    while let Ok(event) = events.try_recv() {
        assert!(!matches!(event.msg, EventMsg::PlanUpdate(_)));
        if let EventMsg::Error(event) = event.msg {
            error |= event.message.contains("after two corrections");
        }
    }
    assert!(error);
}

#[tokio::test]
async fn required_initial_tool_uses_current_turn_not_retained_plan() {
    let server = MockServer::start().await;
    let mock = mount_sse_sequence(&server, vec![plan(), answer("final", "27")]).await;
    let (session, context, _events) = fixture(&server).await;
    session
        .record_conversation_items(
            &context,
            &[
                ResponseItem::FunctionCall {
                    id: None,
                    name: "update_plan".to_string(),
                    namespace: None,
                    arguments: r#"{"plan":[{"step":"Previous task","status":"completed"}]}"#
                        .to_string(),
                    call_id: "old-plan".to_string(),
                },
                ResponseItem::FunctionCallOutput {
                    call_id: "old-plan".to_string(),
                    output: ctox_protocol::models::FunctionCallOutputPayload::from_text(
                        "Plan updated".to_string(),
                    ),
                },
            ],
        )
        .await;
    assert_eq!(
        execute(session, context, CancellationToken::new())
            .await
            .as_deref(),
        Some("27")
    );
    let requests = mock.requests();
    assert_eq!(requests.len(), 2, "planned turns require no correction");
    assert_eq!(
        requests[0].body_json()["tools"].as_array().unwrap().len(),
        1
    );
    assert!(
        !requests[1].body_json()["input"]
            .to_string()
            .contains("correction 1 of 2")
    );
}

#[tokio::test]
async fn required_initial_tool_unrelated_call_does_not_release_tools() {
    let server = MockServer::start().await;
    let unrelated = sse(vec![
        ev_function_call("unknown", "not_a_registered_tool", "{}"),
        ev_completed("unknown"),
    ]);
    let mock = mount_sse_sequence(&server, vec![unrelated, plan(), answer("final", "27")]).await;
    let (session, context, _events) = fixture(&server).await;
    assert_eq!(
        execute(session, context, CancellationToken::new())
            .await
            .as_deref(),
        Some("27")
    );
    let requests = mock.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[1].body_json()["tools"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        requests[1].body_json()["tools"][0]["name"],
        json!("update_plan")
    );
}

#[tokio::test]
async fn required_initial_tool_failed_plan_call_keeps_tools_restricted() {
    let server = MockServer::start().await;
    let invalid = sse(vec![
        ev_function_call("invalid-plan", "update_plan", "{}"),
        ev_completed("invalid"),
    ]);
    let mock = mount_sse_sequence(&server, vec![invalid, plan(), answer("final", "27")]).await;
    let (session, context, _events) = fixture(&server).await;
    assert_eq!(
        execute(session, context, CancellationToken::new())
            .await
            .as_deref(),
        Some("27")
    );
    let requests = mock.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[1].body_json()["tools"].as_array().unwrap().len(),
        1
    );
    assert_eq!(requests[1].body_json()["tools"][0]["name"], "update_plan");
}

#[tokio::test]
async fn required_initial_tool_survives_stream_disconnect_after_successful_plan() {
    let server = MockServer::start().await;
    // End the stream after the call, without response.completed.
    let interrupted = sse(vec![ev_function_call(
        "plan-before-disconnect",
        "update_plan",
        r#"{"plan":[{"step":"Calculate the sum","status":"completed"}]}"#,
    )]);
    let mock = mount_sse_sequence(&server, vec![interrupted, answer("resumed", "27")]).await;
    let (session, context, events) = fixture(&server).await;
    assert_eq!(
        execute(session, context, CancellationToken::new())
            .await
            .as_deref(),
        Some("27")
    );
    let requests = mock.requests();
    assert_eq!(requests.len(), 2);
    let resumed = requests[1].body_json();
    assert!(resumed["tools"].as_array().unwrap().len() > 1);
    assert!(resumed["input"].as_array().unwrap().iter().any(|item| {
        item["type"] == "function_call_output" && item["call_id"] == "plan-before-disconnect"
    }));
    assert!(!resumed["input"].to_string().contains("correction 1 of 2"));
    let mut plans = 0;
    while let Ok(event) = events.try_recv() {
        if matches!(event.msg, EventMsg::PlanUpdate(_)) {
            plans += 1;
        }
    }
    assert_eq!(plans, 1);
}

#[tokio::test]
async fn required_initial_tool_cancelled_turn_does_not_request_correction() {
    let server = MockServer::start().await;
    let (session, context, _events) = fixture(&server).await;
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    assert!(execute(session, context, cancellation).await.is_none());
    assert!(server.received_requests().await.unwrap().is_empty());
}
