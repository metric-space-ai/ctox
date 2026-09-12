//! Real in-process app-server / on-disk ThreadManager restart coverage.
//!
//! These tests speak the app-server JSON-RPC path through
//! [`super::InProcessAppServerClient`], persist a named non-ephemeral thread
//! under an isolated `codex_home`, then recreate the manager and prove lookup,
//! resume, and history replay. They are not scripted
//! `DirectSessionControlClient` adapters.
//!
//! Run from the nested harness workspace, not the repository root:
//! `cargo test --manifest-path src/core/harness/Cargo.toml -p ctox-app-server-client -- --test-threads=2 named_persistent_thread_survives_manager_restart`

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
use futures::FutureExt;
use pretty_assertions::assert_eq;
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::Arc;
use tokio::time::Duration;
use tokio::time::timeout;

const THREAD_NAME: &str = "ctox-issue97-persistent-worker";
const USER_MARKER: &str = "remember this marker: persist-resume-97";
const ASSISTANT_MARKER: &str = "persistent-resume-ack";
const MISSING_THREAD_ID: &str = "00000000-0000-4000-8000-000000000001";
const FIXTURE_TIMEOUT: Duration = Duration::from_secs(90);
const START_TIMEOUT: Duration = Duration::from_secs(15);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const TURN_WAIT_TIMEOUT: Duration = Duration::from_secs(45);
const SESSION_INDEX_TIMEOUT: Duration = Duration::from_secs(5);

struct IsolatedClient {
    client: Option<InProcessAppServerClient>,
}

impl IsolatedClient {
    async fn start(session_source: SessionSource, config: Arc<Config>) -> Self {
        let client = timeout(
            START_TIMEOUT,
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
            }),
        )
        .await
        .expect("in-process app-server client start timed out")
        .expect("in-process app-server client should start");
        Self {
            client: Some(client),
        }
    }

    fn get(&self) -> &InProcessAppServerClient {
        self.client.as_ref().expect("isolated client still live")
    }

    fn get_mut(&mut self) -> &mut InProcessAppServerClient {
        self.client.as_mut().expect("isolated client still live")
    }

    async fn request<T>(&self, request: ClientRequest, what: &str) -> T
    where
        T: DeserializeOwned + Send,
    {
        timeout(REQUEST_TIMEOUT, self.get().request_typed(request))
            .await
            .unwrap_or_else(|_| panic!("{what} timed out"))
            .unwrap_or_else(|err| panic!("{what} failed: {err}"))
    }

    async fn request_err<T>(&self, request: ClientRequest, what: &str) -> TypedRequestError
    where
        T: DeserializeOwned + Send,
    {
        match timeout(REQUEST_TIMEOUT, self.get().request_typed::<T>(request)).await {
            Err(_) => panic!("{what} timed out"),
            Ok(Err(err)) => err,
            Ok(Ok(_)) => panic!("{what} succeeded, expected a JSON-RPC error"),
        }
    }

    async fn shutdown(&mut self) {
        if let Some(client) = self.client.take() {
            // Native shutdown already bounds send + worker wait internally.
            client.shutdown().await.expect("client shutdown failed");
        }
    }

    async fn shutdown_best_effort(&mut self) {
        if let Some(client) = self.client.take() {
            let _ = client.shutdown().await;
        }
    }
}

#[derive(Default)]
struct ClientScope {
    clients: Vec<IsolatedClient>,
}

impl ClientScope {
    async fn start(&mut self, session_source: SessionSource, config: Arc<Config>) -> usize {
        self.clients
            .push(IsolatedClient::start(session_source, config).await);
        self.clients.len() - 1
    }

    fn get_mut(&mut self, id: usize) -> &mut IsolatedClient {
        &mut self.clients[id]
    }

    async fn shutdown_all(&mut self) {
        for client in &mut self.clients {
            client.shutdown_best_effort().await;
        }
    }
}

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
    timeout(
        START_TIMEOUT,
        ConfigBuilder::default()
            .codex_home(codex_home.to_path_buf())
            .build(),
    )
    .await
    .expect("isolated config build timed out")
    .map(Arc::new)
    .expect("isolated config should build")
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

fn completed_turn(thread: &ctox_app_server_protocol::Thread) -> bool {
    thread
        .turns
        .iter()
        .any(|turn| turn.status == TurnStatus::Completed)
}

fn failed_turn_error(thread: &ctox_app_server_protocol::Thread) -> Option<String> {
    thread.turns.iter().find_map(|turn| {
        matches!(turn.status, TurnStatus::Failed | TurnStatus::Interrupted)
            .then(|| format!("{:?}: {:?}", turn.status, turn.error))
    })
}

fn is_transient_read_error(err: &TypedRequestError) -> bool {
    let message = err.to_string();
    message.contains("not materialized yet") || message.contains("includeTurns is unavailable")
}

async fn wait_for_turn_completed(client: &mut IsolatedClient, thread_id: &str) {
    timeout(TURN_WAIT_TIMEOUT, async {
        let mut poll_id = 1000i64;
        loop {
            match timeout(Duration::from_millis(250), client.get_mut().next_event()).await {
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
            match timeout(
                REQUEST_TIMEOUT,
                client
                    .get()
                    .request_typed::<ThreadReadResponse>(ClientRequest::ThreadRead {
                        request_id,
                        params: ThreadReadParams {
                            thread_id: thread_id.to_string(),
                            include_turns: true,
                        },
                    }),
            )
            .await
            {
                Err(_) => panic!("thread/read timed out while waiting for turn completion"),
                Ok(Err(err)) if is_transient_read_error(&err) => {}
                Ok(Err(err)) => {
                    panic!("thread/read failed while waiting for turn completion: {err}")
                }
                Ok(Ok(read)) => {
                    if let Some(error) = failed_turn_error(&read.thread) {
                        panic!("turn did not complete successfully: {error}");
                    }
                    if completed_turn(&read.thread) {
                        return;
                    }
                }
            }
        }
    })
    .await
    .expect("timed out waiting for a completed turn");
}

async fn wait_for_session_index_name(codex_home: &Path) {
    timeout(SESSION_INDEX_TIMEOUT, async {
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

async fn manager_thread_ids(client: &IsolatedClient) -> Vec<String> {
    let mut ids = client
        .get()
        .thread_manager()
        .list_thread_ids()
        .await
        .into_iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

async fn run_named_persistent_thread_restart(
    scope: &mut ClientScope,
    codex_home: &tempfile::TempDir,
    server_uri: &str,
) {
    let config = isolated_mock_config(codex_home.path(), server_uri).await;

    let created = {
        let id = scope.start(SessionSource::Exec, Arc::clone(&config)).await;
        let client = scope.get_mut(id);
        let started: ThreadStartResponse = client
            .request(
                ClientRequest::ThreadStart {
                    request_id: RequestId::Integer(1),
                    params: persistent_start_params(codex_home.path()),
                },
                "thread/start",
            )
            .await;
        assert!(
            !started.thread.ephemeral,
            "persistent worker threads must be materialized on disk"
        );
        let thread_id = started.thread.id.clone();
        let _: ThreadSetNameResponse = client
            .request(
                ClientRequest::ThreadSetName {
                    request_id: RequestId::Integer(2),
                    params: ThreadSetNameParams {
                        thread_id: thread_id.clone(),
                        name: THREAD_NAME.to_string(),
                    },
                },
                "thread/name/set",
            )
            .await;
        let _: TurnStartResponse = client
            .request(
                ClientRequest::TurnStart {
                    request_id: RequestId::Integer(3),
                    params: TurnStartParams {
                        thread_id: thread_id.clone(),
                        input: vec![UserInput::Text {
                            text: USER_MARKER.to_string(),
                            text_elements: Vec::new(),
                        }],
                        ..TurnStartParams::default()
                    },
                },
                "turn/start",
            )
            .await;
        wait_for_turn_completed(client, &thread_id).await;
        wait_for_session_index_name(codex_home.path()).await;

        let read: ThreadReadResponse = client
            .request(
                ClientRequest::ThreadRead {
                    request_id: RequestId::Integer(4),
                    params: ThreadReadParams {
                        thread_id: thread_id.clone(),
                        include_turns: true,
                    },
                },
                "thread/read before restart",
            )
            .await;
        assert_eq!(read.thread.id, thread_id);
        assert_eq!(read.thread.name.as_deref(), Some(THREAD_NAME));
        assert!(
            completed_turn(&read.thread),
            "pre-restart history should include a completed turn: {:?}",
            read.thread.turns
        );
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
        assert_eq!(manager_thread_ids(&client).await, vec![thread_id.clone()]);

        client.shutdown().await;
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

    let restarted_config = isolated_mock_config(codex_home.path(), server_uri).await;
    let id = scope.start(SessionSource::Exec, restarted_config).await;
    let client = scope.get_mut(id);
    assert!(
        manager_thread_ids(&client).await.is_empty(),
        "a restarted ThreadManager must not inherit in-memory threads"
    );

    let listed: ThreadListResponse = client
        .request(
            ClientRequest::ThreadList {
                request_id: RequestId::Integer(10),
                params: adapter_list_params(),
            },
            "thread/list after restart",
        )
        .await;
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
        .request(
            ClientRequest::ThreadResume {
                request_id: RequestId::Integer(11),
                params: ThreadResumeParams {
                    thread_id: thread_id.clone(),
                    model: Some("compact".to_string()),
                    model_provider: Some("mock_provider".to_string()),
                    persist_extended_history: true,
                    ..ThreadResumeParams::default()
                },
            },
            "thread/resume identified thread",
        )
        .await;
    assert_eq!(resumed.thread.id, thread_id);
    assert_eq!(resumed.thread.name.as_deref(), Some(THREAD_NAME));
    assert!(!resumed.thread.ephemeral);
    assert_eq!(manager_thread_ids(&client).await, vec![thread_id.clone()]);

    let reread: ThreadReadResponse = client
        .request(
            ClientRequest::ThreadRead {
                request_id: RequestId::Integer(12),
                params: ThreadReadParams {
                    thread_id: thread_id.clone(),
                    include_turns: true,
                },
            },
            "thread/read after resume",
        )
        .await;
    assert_eq!(reread.thread.id, thread_id);
    assert_eq!(reread.thread.name.as_deref(), Some(THREAD_NAME));
    assert!(
        completed_turn(&reread.thread),
        "resumed history should include a completed turn: {:?}",
        reread.thread.turns
    );
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
        .request_err::<ThreadResumeResponse>(
            ClientRequest::ThreadResume {
                request_id: RequestId::Integer(13),
                params: ThreadResumeParams {
                    thread_id: MISSING_THREAD_ID.to_string(),
                    persist_extended_history: true,
                    ..ThreadResumeParams::default()
                },
            },
            "missing identified thread must fail closed",
        )
        .await;
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
    assert_eq!(
        manager_thread_ids(&client).await,
        vec![thread_id.clone()],
        "missing-id resume rejection must keep exactly the original loaded thread"
    );

    client.shutdown().await;
}

#[tokio::test]
async fn named_persistent_thread_survives_manager_restart_and_missing_resume_fails_closed() {
    let server = timeout(
        START_TIMEOUT,
        create_mock_responses_server_repeating_assistant(ASSISTANT_MARKER),
    )
    .await
    .expect("mock responses server start timed out");
    let codex_home = tempfile::TempDir::new().expect("tempdir");
    let mut scope = ClientScope::default();
    let outcome = AssertUnwindSafe(timeout(
        FIXTURE_TIMEOUT,
        run_named_persistent_thread_restart(&mut scope, &codex_home, &server.uri()),
    ))
    .catch_unwind()
    .await;
    scope.shutdown_all().await;
    // Keep the server and on-disk home alive until every client has stopped,
    // including when the test body times out or panics.
    drop(server);
    drop(codex_home);
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(_)) => panic!("persistent-thread restart fixture timed out"),
        Err(payload) => std::panic::resume_unwind(payload),
    }
}
