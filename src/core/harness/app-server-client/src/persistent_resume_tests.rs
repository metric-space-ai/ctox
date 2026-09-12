//! Real in-process app-server / on-disk ThreadManager restart coverage.
//!
//! These tests speak the app-server JSON-RPC path through
//! [`super::InProcessAppServerClient`], persist a named non-ephemeral thread
//! under an isolated `codex_home`, then recreate the manager and prove lookup,
//! resume, and history replay. They are not scripted
//! `DirectSessionControlClient` adapters.

use super::*;
use app_test_support::create_mock_responses_server_repeating_assistant;
use app_test_support::write_mock_responses_config_toml;
use ctox_app_server_protocol::AskForApproval;
use ctox_app_server_protocol::ClientRequest;
use ctox_app_server_protocol::RequestId;
use ctox_app_server_protocol::SandboxMode;
use ctox_app_server_protocol::ServerNotification;
use ctox_app_server_protocol::ThreadItem;
use ctox_app_server_protocol::ThreadListParams;
use ctox_app_server_protocol::ThreadListResponse;
use ctox_app_server_protocol::ThreadReadParams;
use ctox_app_server_protocol::ThreadReadResponse;
use ctox_app_server_protocol::ThreadResumeParams;
use ctox_app_server_protocol::ThreadResumeResponse;
use ctox_app_server_protocol::ThreadSetNameParams;
use ctox_app_server_protocol::ThreadSetNameResponse;
use ctox_app_server_protocol::ThreadSortKey;
use ctox_app_server_protocol::ThreadSourceKind;
use ctox_app_server_protocol::ThreadStartParams;
use ctox_app_server_protocol::ThreadStartResponse;
use ctox_app_server_protocol::TurnStartParams;
use ctox_app_server_protocol::TurnStartResponse;
use ctox_app_server_protocol::TurnStatus;
use ctox_app_server_protocol::UserInput;
use ctox_arg0::Arg0DispatchPaths;
use ctox_core::config::Config;
use ctox_core::config::ConfigBuilder;
use ctox_core::config_loader::CloudRequirementsLoader;
use ctox_core::config_loader::LoaderOverrides;
use ctox_feedback::CodexFeedback;
use ctox_protocol::protocol::SessionSource;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use tokio::time::Duration;
use tokio::time::timeout;

const THREAD_NAME: &str = "ctox-issue97-persistent-worker";
const USER_MARKER: &str = "remember this marker: persist-resume-97";
const ASSISTANT_MARKER: &str = "persistent-resume-ack";
const MISSING_THREAD_ID: &str = "00000000-0000-4000-8000-000000000001";

async fn isolated_mock_config(codex_home: &Path, server_uri: &str) -> Arc<Config> {
    write_mock_responses_config_toml(
        codex_home,
        server_uri,
        &BTreeMap::new(),
        8_192,
        Some(false),
        "mock_provider",
        "compact",
    )
    .expect("mock config should write");
    Arc::new(
        ConfigBuilder::default()
            .codex_home(codex_home.to_path_buf())
            .build()
            .await
            .expect("isolated config should build"),
    )
}

async fn start_isolated_client(
    session_source: SessionSource,
    config: Arc<Config>,
) -> InProcessAppServerClient {
    InProcessAppServerClient::start(InProcessClientStartArgs {
        arg0_paths: Arg0DispatchPaths::default(),
        config,
        cli_overrides: Vec::new(),
        loader_overrides: LoaderOverrides::default(),
        cloud_requirements: CloudRequirementsLoader::default(),
        auth_manager: None,
        thread_manager: None,
        feedback: CodexFeedback::new(),
        config_warnings: Vec::new(),
        session_source,
        enable_ctox_api_key_env: false,
        client_name: "ctox-app-server-client-persistent-resume-test".to_string(),
        client_version: "0.0.0-test".to_string(),
        experimental_api: true,
        opt_out_notification_methods: Vec::new(),
        channel_capacity: DEFAULT_IN_PROCESS_CHANNEL_CAPACITY,
    })
    .await
    .expect("in-process app-server client should start")
}

fn persistent_start_params(cwd: &Path) -> ThreadStartParams {
    ThreadStartParams {
        model: Some("compact".to_string()),
        model_provider: Some("mock_provider".to_string()),
        cwd: Some(cwd.to_string_lossy().into_owned()),
        approval_policy: Some(AskForApproval::Never),
        sandbox: Some(SandboxMode::WorkspaceWrite),
        disable_mcp_servers: Some(true),
        dynamic_tools: Some(Vec::new()),
        ephemeral: Some(false),
        persist_extended_history: true,
        ..ThreadStartParams::default()
    }
}

fn adapter_list_params() -> ThreadListParams {
    ThreadListParams {
        cursor: None,
        limit: Some(20),
        sort_key: Some(ThreadSortKey::UpdatedAt),
        model_providers: Some(Vec::new()),
        source_kinds: Some(vec![ThreadSourceKind::Exec]),
        archived: Some(false),
        cwd: None,
        search_term: None,
    }
}

fn history_contains_markers(thread: &ctox_app_server_protocol::Thread) -> bool {
    user_texts(thread)
        .iter()
        .any(|text| text.contains(USER_MARKER))
        && agent_texts(thread)
            .iter()
            .any(|text| text.contains(ASSISTANT_MARKER))
}

async fn wait_for_turn_completed(client: &mut InProcessAppServerClient, thread_id: &str) {
    let deadline = Duration::from_secs(45);
    timeout(deadline, async {
        let mut poll_id = 1000i64;
        loop {
            match timeout(Duration::from_millis(250), client.next_event()).await {
                Ok(Some(InProcessServerEvent::ServerNotification(
                    ServerNotification::TurnCompleted(notification),
                ))) if notification.thread_id == thread_id => {
                    assert_eq!(
                        notification.turn.status,
                        TurnStatus::Completed,
                        "mock turn should complete: {:?}",
                        notification.turn.error
                    );
                    return;
                }
                Ok(Some(InProcessServerEvent::ServerNotification(ServerNotification::Error(
                    error,
                )))) => {
                    panic!("app-server error while waiting for turn completion: {error:?}");
                }
                Ok(Some(InProcessServerEvent::ServerRequest(request))) => {
                    panic!(
                        "unexpected server request while waiting for turn completion: {request:?}"
                    );
                }
                Ok(Some(InProcessServerEvent::Lagged { skipped })) => {
                    panic!("event stream lagged by {skipped} while waiting for turn completion");
                }
                Ok(None) => panic!("in-process event stream closed before turn completed"),
                Ok(Some(_)) | Err(_) => {}
            }
            let request_id = RequestId::Integer(poll_id);
            poll_id += 1;
            if let Ok(read) = client
                .request_typed::<ThreadReadResponse>(ClientRequest::ThreadRead {
                    request_id,
                    params: ThreadReadParams {
                        thread_id: thread_id.to_string(),
                        include_turns: true,
                    },
                })
                .await
                && history_contains_markers(&read.thread)
            {
                return;
            }
        }
    })
    .await
    .expect("timed out waiting for persisted turn history");
}

async fn wait_for_session_index_name(codex_home: &Path) {
    timeout(Duration::from_secs(5), async {
        let path = codex_home.join("session_index.jsonl");
        loop {
            if let Ok(index) = std::fs::read_to_string(&path)
                && index.contains(THREAD_NAME)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("thread name should persist to session_index.jsonl");
}

fn user_texts(thread: &ctox_app_server_protocol::Thread) -> Vec<String> {
    thread
        .turns
        .iter()
        .flat_map(|turn| turn.items.iter())
        .filter_map(|item| match item {
            ThreadItem::UserMessage { content, .. } => {
                let texts = content
                    .iter()
                    .filter_map(|input| match input {
                        UserInput::Text { text, .. } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                (!texts.is_empty()).then(|| texts.join("\n"))
            }
            _ => None,
        })
        .collect()
}

fn agent_texts(thread: &ctox_app_server_protocol::Thread) -> Vec<String> {
    thread
        .turns
        .iter()
        .flat_map(|turn| turn.items.iter())
        .filter_map(|item| match item {
            ThreadItem::AgentMessage { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn named_persistent_thread_survives_manager_restart_and_missing_resume_fails_closed() {
    let server = create_mock_responses_server_repeating_assistant(ASSISTANT_MARKER).await;
    let codex_home = tempfile::TempDir::new().expect("tempdir");
    let config = isolated_mock_config(codex_home.path(), &server.uri()).await;

    let created = {
        let mut client = start_isolated_client(SessionSource::Exec, Arc::clone(&config)).await;
        let started: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
                params: persistent_start_params(codex_home.path()),
            })
            .await
            .expect("thread/start should succeed");
        assert!(
            !started.thread.ephemeral,
            "persistent worker threads must be materialized on disk"
        );
        let thread_id = started.thread.id.clone();
        let _: ThreadSetNameResponse = client
            .request_typed(ClientRequest::ThreadSetName {
                request_id: RequestId::Integer(2),
                params: ThreadSetNameParams {
                    thread_id: thread_id.clone(),
                    name: THREAD_NAME.to_string(),
                },
            })
            .await
            .expect("thread/name/set should succeed");
        let _: TurnStartResponse = client
            .request_typed(ClientRequest::TurnStart {
                request_id: RequestId::Integer(3),
                params: TurnStartParams {
                    thread_id: thread_id.clone(),
                    input: vec![UserInput::Text {
                        text: USER_MARKER.to_string(),
                        text_elements: Vec::new(),
                    }],
                    ..TurnStartParams::default()
                },
            })
            .await
            .expect("turn/start should succeed");
        wait_for_turn_completed(&mut client, &thread_id).await;
        wait_for_session_index_name(codex_home.path()).await;

        let read: ThreadReadResponse = client
            .request_typed(ClientRequest::ThreadRead {
                request_id: RequestId::Integer(4),
                params: ThreadReadParams {
                    thread_id: thread_id.clone(),
                    include_turns: true,
                },
            })
            .await
            .expect("thread/read should succeed before restart");
        assert_eq!(read.thread.id, thread_id);
        assert_eq!(read.thread.name.as_deref(), Some(THREAD_NAME));
        assert!(
            user_texts(&read.thread)
                .iter()
                .any(|text| text.contains(USER_MARKER)),
            "pre-restart history should include the user marker: {:?}",
            user_texts(&read.thread)
        );
        assert!(
            agent_texts(&read.thread)
                .iter()
                .any(|text| text.contains(ASSISTANT_MARKER)),
            "pre-restart history should include the mock assistant marker: {:?}",
            agent_texts(&read.thread)
        );

        let rollout_path = read
            .thread
            .path
            .clone()
            .expect("persistent thread should expose a rollout path");
        assert!(
            rollout_path.starts_with(codex_home.path()),
            "rollout must stay inside the isolated home: {}",
            rollout_path.display()
        );
        let rollout = std::fs::read_to_string(&rollout_path).expect("rollout should be on disk");
        assert!(
            rollout.contains(USER_MARKER),
            "on-disk rollout should contain the user marker"
        );
        assert!(
            rollout.contains(ASSISTANT_MARKER),
            "on-disk rollout should contain the assistant marker"
        );
        let session_index = std::fs::read_to_string(codex_home.path().join("session_index.jsonl"))
            .expect("session index should persist the thread name");
        assert!(
            session_index.contains(THREAD_NAME),
            "session index should contain the assigned thread name"
        );
        assert!(
            client
                .thread_manager()
                .list_thread_ids()
                .await
                .iter()
                .any(|id| id.to_string() == thread_id),
            "live manager should retain the started thread"
        );

        client.shutdown().await.expect("first manager shutdown");
        (
            thread_id,
            rollout_path,
            user_texts(&read.thread),
            agent_texts(&read.thread),
        )
    };

    let (thread_id, rollout_path, original_user_texts, original_agent_texts) = created;
    assert!(
        rollout_path.exists(),
        "rollout must survive manager shutdown"
    );

    let restarted_config = isolated_mock_config(codex_home.path(), &server.uri()).await;
    let client = start_isolated_client(SessionSource::Exec, restarted_config).await;
    assert!(
        client.thread_manager().list_thread_ids().await.is_empty(),
        "a restarted ThreadManager must not inherit in-memory threads"
    );

    let listed: ThreadListResponse = client
        .request_typed(ClientRequest::ThreadList {
            request_id: RequestId::Integer(10),
            params: adapter_list_params(),
        })
        .await
        .expect("thread/list should succeed after restart");
    let listed_thread = listed
        .data
        .iter()
        .find(|thread| !thread.ephemeral && thread.name.as_deref() == Some(THREAD_NAME))
        .unwrap_or_else(|| {
            panic!("named persistent thread should be listed after restart: {listed:?}")
        });
    assert_eq!(listed_thread.id, thread_id);
    assert_eq!(listed_thread.path.as_ref(), Some(&rollout_path));

    let resumed: ThreadResumeResponse = client
        .request_typed(ClientRequest::ThreadResume {
            request_id: RequestId::Integer(11),
            params: ThreadResumeParams {
                thread_id: thread_id.clone(),
                model: Some("compact".to_string()),
                model_provider: Some("mock_provider".to_string()),
                persist_extended_history: true,
                ..ThreadResumeParams::default()
            },
        })
        .await
        .expect("thread/resume should restore the identified thread");
    assert_eq!(resumed.thread.id, thread_id);
    assert_eq!(resumed.thread.name.as_deref(), Some(THREAD_NAME));
    assert!(!resumed.thread.ephemeral);
    assert!(
        client
            .thread_manager()
            .list_thread_ids()
            .await
            .iter()
            .any(|id| id.to_string() == thread_id),
        "resume should load the same thread into the new manager"
    );

    let reread: ThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(12),
            params: ThreadReadParams {
                thread_id: thread_id.clone(),
                include_turns: true,
            },
        })
        .await
        .expect("thread/read should inspect resumed history");
    assert_eq!(reread.thread.id, thread_id);
    assert_eq!(reread.thread.name.as_deref(), Some(THREAD_NAME));
    assert_eq!(user_texts(&reread.thread), original_user_texts);
    assert_eq!(agent_texts(&reread.thread), original_agent_texts);
    assert!(
        user_texts(&reread.thread)
            .iter()
            .any(|text| text.contains(USER_MARKER)),
        "resumed history must keep the same user marker"
    );
    assert!(
        agent_texts(&reread.thread)
            .iter()
            .any(|text| text.contains(ASSISTANT_MARKER)),
        "resumed history must keep the same assistant marker"
    );

    let err = client
        .request_typed::<ThreadResumeResponse>(ClientRequest::ThreadResume {
            request_id: RequestId::Integer(13),
            params: ThreadResumeParams {
                thread_id: MISSING_THREAD_ID.to_string(),
                persist_extended_history: true,
                ..ThreadResumeParams::default()
            },
        })
        .await
        .expect_err("missing identified thread must fail closed");
    match err {
        TypedRequestError::Server { method, source } => {
            assert_eq!(method, "thread/resume");
            assert!(
                source.message.contains("no rollout found"),
                "closest real resume-rejection seam is a JSON-RPC server error, got: {}",
                source.message
            );
        }
        other => panic!("expected JSON-RPC server error for missing resume, got {other}"),
    }

    client
        .shutdown()
        .await
        .expect("restarted manager shutdown should complete");
}
