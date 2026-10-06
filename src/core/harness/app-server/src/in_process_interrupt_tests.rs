use super::*;
use app_test_support::create_mock_responses_server_repeating_assistant;
use app_test_support::mount_mock_responses_assistant;
use app_test_support::write_mock_responses_config_toml;
use ctox_app_server_protocol::ClientInfo;
use ctox_app_server_protocol::ThreadStartParams;
use ctox_app_server_protocol::ThreadStartResponse;
use ctox_app_server_protocol::TurnInterruptParams;
use ctox_app_server_protocol::TurnInterruptResponse;
use ctox_app_server_protocol::TurnStartParams;
use ctox_app_server_protocol::TurnStartResponse;
use ctox_app_server_protocol::TurnStatus;
use ctox_app_server_protocol::TurnSteerParams;
use ctox_app_server_protocol::TurnSteerResponse;
use ctox_app_server_protocol::UserInput;
use ctox_core::config::ConfigBuilder;
use std::collections::BTreeMap;

async fn rpc(
    client: &InProcessClientHandle,
    request: ClientRequest,
) -> PendingClientRequestResponse {
    // Exercise the wire contract, including turnId, as well as real request dispatch.
    let wire = serde_json::to_value(request).expect("serialize request");
    let request = serde_json::from_value(wire).expect("deserialize request");
    timeout(Duration::from_secs(10), client.request(request))
        .await
        .expect("RPC must finish")
        .expect("request transport")
}

fn input(text: &str) -> Vec<UserInput> {
    vec![UserInput::Text {
        text: text.into(),
        text_elements: Vec::new(),
    }]
}

async fn start_turn(client: &InProcessClientHandle, thread: &str, id: i64) -> String {
    let response = rpc(
        client,
        ClientRequest::TurnStart {
            request_id: RequestId::Integer(id),
            params: TurnStartParams {
                thread_id: thread.into(),
                input: input("interrupt regression"),
                ..Default::default()
            },
        },
    )
    .await
    .expect("turn/start success");
    let response: TurnStartResponse = serde_json::from_value(response).unwrap();
    assert_eq!(response.turn.status, TurnStatus::InProgress);
    response.turn.id
}

async fn completed(client: &mut InProcessClientHandle, thread: &str, turn: &str) -> TurnStatus {
    timeout(Duration::from_secs(10), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream remains open");
            if let InProcessServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                event,
            )) = event
            {
                assert_eq!(event.thread_id, thread);
                assert_eq!(event.turn.id, turn, "no other turn may complete");
                return event.turn.status;
            }
        }
    })
    .await
    .expect("matching terminal notification")
}

#[tokio::test]
async fn exact_turn_interrupt_rpc_preserves_identity_and_successor() {
    timeout(Duration::from_secs(45), async {
        let server = create_mock_responses_server_repeating_assistant("done").await;
        let home = tempfile::TempDir::new().expect("isolated home");
        write_mock_responses_config_toml(
            home.path(),
            &server.uri(),
            &BTreeMap::new(),
            8_192,
            Some(false),
            "mock_provider",
            "compact",
        )
        .unwrap();
        let config = ConfigBuilder::default()
            .codex_home(home.path().to_path_buf())
            .build()
            .await
            .expect("mock provider config");
        let mut client = start(InProcessStartArgs {
            arg0_paths: Arg0DispatchPaths::default(),
            config: Arc::new(config),
            cli_overrides: Vec::new(),
            loader_overrides: LoaderOverrides::default(),
            cloud_requirements: CloudRequirementsLoader::default(),
            auth_manager: None,
            thread_manager: None,
            feedback: CodexFeedback::new(),
            config_warnings: Vec::new(),
            session_source: SessionSource::Exec,
            enable_ctox_api_key_env: false,
            initialize: InitializeParams {
                client_info: ClientInfo {
                    name: "exact-turn-interrupt-test".into(),
                    title: None,
                    version: "0.0.0".into(),
                },
                capabilities: None,
            },
            channel_capacity: DEFAULT_IN_PROCESS_CHANNEL_CAPACITY,
        })
        .await
        .expect("start real app-server");
        let thread = rpc(
            &client,
            ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
                params: ThreadStartParams {
                    ephemeral: Some(true),
                    ..Default::default()
                },
            },
        )
        .await
        .expect("thread/start success");
        let thread: ThreadStartResponse = serde_json::from_value(thread).unwrap();
        let thread = thread.thread.id;
        let old_turn = start_turn(&client, &thread, 2).await;
        assert_eq!(
            completed(&mut client, &thread, &old_turn).await,
            TurnStatus::Completed
        );

        server.reset().await;
        // Longer than the whole test: the successor cannot finish by elapsed time.
        mount_mock_responses_assistant(&server, "later", Duration::from_secs(120)).await;
        let successor = start_turn(&client, &thread, 3).await;
        assert_ne!(old_turn, successor);
        timeout(Duration::from_secs(10), async {
            loop {
                if server
                    .received_requests()
                    .await
                    .unwrap()
                    .iter()
                    .any(|request| request.url.path().ends_with("/responses"))
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("successor reached the model and is active");

        for (id, rejected_turn) in [(10, old_turn), (20, "unknown-turn".into())] {
            let error = rpc(
                &client,
                ClientRequest::TurnInterrupt {
                    request_id: RequestId::Integer(id),
                    params: TurnInterruptParams {
                        thread_id: thread.clone(),
                        turn_id: rejected_turn,
                    },
                },
            )
            .await
            .expect_err("old and unknown IDs must be rejected");
            assert_eq!(error.code, crate::error_code::INVALID_REQUEST_ERROR_CODE);
            // This calls core steer_input with the exact active-turn precondition.
            let response = rpc(
                &client,
                ClientRequest::TurnSteer {
                    request_id: RequestId::Integer(id + 1),
                    params: TurnSteerParams {
                        thread_id: thread.clone(),
                        expected_turn_id: successor.clone(),
                        input: input("still active"),
                    },
                },
            )
            .await
            .expect("rejected interrupt must leave successor running");
            let response: TurnSteerResponse = serde_json::from_value(response).unwrap();
            assert_eq!(response.turn_id, successor);
        }

        // Let subsequent interrupt compaction respond promptly. The already received
        // successor request remains delayed until the real core abort cancels it.
        server.reset().await;
        mount_mock_responses_assistant(&server, "compacted", Duration::ZERO).await;
        let response = rpc(
            &client,
            ClientRequest::TurnInterrupt {
                request_id: RequestId::Integer(30),
                params: TurnInterruptParams {
                    thread_id: thread.clone(),
                    turn_id: successor.clone(),
                },
            },
        )
        .await
        .expect("matching interrupt succeeds");
        let _: TurnInterruptResponse = serde_json::from_value(response).unwrap();
        assert_eq!(
            completed(&mut client, &thread, &successor).await,
            TurnStatus::Interrupted
        );
        let error = rpc(
            &client,
            ClientRequest::TurnSteer {
                request_id: RequestId::Integer(31),
                params: TurnSteerParams {
                    thread_id: thread,
                    expected_turn_id: successor,
                    input: input("must not be accepted"),
                },
            },
        )
        .await
        .expect_err("matching interrupt leaves no running successor");
        assert_eq!(error.code, crate::error_code::INVALID_REQUEST_ERROR_CODE);
        client.shutdown().await.expect("shutdown test app-server");
    })
    .await
    .expect("bounded interrupt RPC regression");
}
