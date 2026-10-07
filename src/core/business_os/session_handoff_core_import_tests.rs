// Origin: CTOX
// License: AGPL-3.0-only
//! Actual Core import/next-turn regression with only the model endpoint mocked.
//! This does not issue native policy, clean effects or quorum ownership.
use serde_json::{json, Value};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

struct ModelFixture {
    url: String,
    stop: Arc<AtomicBool>,
    task: Option<std::thread::JoinHandle<Vec<Value>>>,
}
impl ModelFixture {
    fn new() -> Self {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}", server.server_addr());
        let stop = Arc::new(AtomicBool::new(false));
        let ending = stop.clone();
        let task = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(100);
            let mut requests = Vec::new();
            while !ending.load(Ordering::Acquire) && Instant::now() < deadline {
                let Some(mut request) = server.recv_timeout(Duration::from_millis(100)).unwrap()
                else {
                    continue;
                };
                if request.method() != &tiny_http::Method::Post
                    || !request.url().ends_with("/responses")
                {
                    request
                        .respond(tiny_http::Response::from_string("{\"models\":[]}"))
                        .unwrap();
                    continue;
                }
                use std::io::Read;
                let mut bytes = Vec::new();
                request
                    .as_reader()
                    .take(2 * 1024 * 1024)
                    .read_to_end(&mut bytes)
                    .unwrap();
                requests.push(serde_json::from_slice(&bytes).unwrap());
                let (id, answer) = if requests.len() == 1 {
                    ("fixture-source-response", "source fixture reply")
                } else {
                    ("fixture-target-response", "target fixture reply")
                };
                let events = [
                    json!({"type":"response.created","response":{"id":id}}),
                    json!({"type":"response.output_item.done","item":{"type":"message","role":"assistant",
                        "id":format!("message-{id}"),"content":[{"type":"output_text","text":answer}]}}),
                    json!({"type":"response.completed","response":{"id":id,
                        "usage":{"input_tokens":0,"input_tokens_details":null,"output_tokens":0,"output_tokens_details":null,"total_tokens":0}}}),
                ];
                let body = events
                    .iter()
                    .map(|event| {
                        format!(
                            "event: {}\ndata: {}\n\n",
                            event["type"].as_str().unwrap(),
                            event
                        )
                    })
                    .collect::<String>();
                request
                    .respond(tiny_http::Response::from_string(body).with_header(
                        tiny_http::Header::from_bytes("Content-Type", "text/event-stream").unwrap(),
                    ))
                    .unwrap();
                if requests.len() == 2 {
                    break;
                }
            }
            requests
        });
        Self {
            url,
            stop,
            task: Some(task),
        }
    }
    fn finish(mut self) -> Vec<Value> {
        self.task.take().unwrap().join().unwrap()
    }
}
impl Drop for ModelFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}

async fn config(home: &Path, cwd: &Path, url: &str) -> ctox_core::config::Config {
    std::fs::create_dir_all(home).unwrap();
    std::fs::create_dir_all(cwd).unwrap();
    let mut config = ctox_core::config::ConfigBuilder::default()
        .codex_home(home.to_owned())
        .build()
        .await
        .unwrap();
    config.cwd = cwd.to_owned();
    config.model = Some("gpt-5.1".into());
    config.model_provider = ctox_core::ModelProviderInfo::create_openai_provider(Some(url.into()));
    config.model_provider.requires_openai_auth = false;
    config.model_provider.supports_websockets = false;
    config.model_provider.env_key = None;
    config.model_provider.http_headers = None;
    config.model_provider.env_http_headers = None;
    config.model_provider.request_max_retries = Some(0);
    config.model_provider.stream_max_retries = Some(0);
    config
        .model_providers
        .insert("openai".into(), config.model_provider.clone());
    config
}
fn manager(
    config: &ctox_core::config::Config,
) -> (ctox_core::ThreadManager, Arc<ctox_core::AuthManager>) {
    let auth = Arc::new(ctox_core::AuthManager::new(
        config.codex_home.clone(),
        false,
        config.cli_auth_credentials_store_mode,
    ));
    (ctox_core::ThreadManager::new(config, auth.clone(), ctox_protocol::protocol::SessionSource::Exec,
        ctox_core::models_manager::collaboration_mode_presets::CollaborationModesConfig::default()), auth)
}
async fn turn(thread: &ctox_core::CodexThread, text: &str) {
    let id = thread
        .submit(ctox_protocol::protocol::Op::UserInput {
            items: vec![ctox_protocol::user_input::UserInput::Text {
                text: text.into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            required_initial_tool: None,
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let event = thread.next_event().await.unwrap();
            assert!(
                !matches!(event.msg, ctox_protocol::protocol::EventMsg::Error(_)),
                "fixture model turn failed"
            );
            if event.id == id
                && matches!(
                    event.msg,
                    ctox_protocol::protocol::EventMsg::TurnComplete(_)
                )
            {
                break;
            }
        }
    })
    .await
    .expect("bounded native model turn");
}

#[test]
fn native_core_import_preserves_original_session_context_and_next_response_chain() {
    let root = tempfile::tempdir().unwrap();
    let model = ModelFixture::new();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let source_config = config(
            &root.path().join("source-home"),
            &root.path().join("source-workspace"),
            &model.url,
        )
        .await;
        let (source_manager, _) = manager(&source_config);
        let source = source_manager.start_thread(source_config).await.unwrap();
        turn(&source.thread, "source original fixture message").await;
        let journal = source.thread.retain_native_journal().await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), source.thread.shutdown_and_wait())
            .await
            .unwrap()
            .unwrap();
        let (_, state) = source.thread.capture_native_state().await.unwrap();
        let original = state.as_bytes().to_vec();
        let journal_bytes = journal.read_bytes(64 * 1024 * 1024).unwrap();
        let imported = || {
            ctox_core::NativeSessionState::from_checkpoint(
                &original,
                source.thread_id,
                state.model(),
                state.provider_id(),
            )
            .unwrap()
        };
        for key in [
            "sessionId",
            "harnessVersion",
            "format",
            "externalEffects",
            "credential",
        ] {
            let mut value: Value = serde_json::from_slice(&original).unwrap();
            value[key] = json!("fixture-invalid");
            assert!(ctox_core::NativeSessionState::from_checkpoint(
                &serde_json::to_vec(&value).unwrap(),
                source.thread_id,
                state.model(),
                state.provider_id()
            )
            .is_err());
        }
        let target_config = config(
            &root.path().join("target-home"),
            &root.path().join("target-workspace"),
            &model.url,
        )
        .await;
        let (target_manager, auth) = manager(&target_config);
        let target_journal = root.path().join("target-home/imported.jsonl");
        std::fs::write(&target_journal, &journal_bytes).unwrap();
        for invalid in ["model", "ephemeral", "provider"] {
            let mut denied_config = target_config.clone();
            match invalid {
                "model" => denied_config.model = Some("foreign-fixture-model".into()),
                "ephemeral" => denied_config.ephemeral = true,
                _ => denied_config.model_provider_id = "foreign-fixture-provider".into(),
            }
            let (denied_manager, denied_auth) = manager(&denied_config);
            assert!(denied_manager
                .resume_thread_from_native_checkpoint(
                    denied_config,
                    target_journal.clone(),
                    denied_auth,
                    imported(),
                )
                .await
                .is_err());
            assert_eq!(std::fs::read(&target_journal).unwrap(), journal_bytes);
        }
        // Both real import calls race for the same original session. Exactly
        // one may expose a thread; the loser must never create another recorder.
        let (first, second) = tokio::time::timeout(Duration::from_secs(20), async {
            tokio::join!(
                target_manager.resume_thread_from_native_checkpoint(
                    target_config.clone(),
                    target_journal.clone(),
                    auth.clone(),
                    imported()
                ),
                target_manager.resume_thread_from_native_checkpoint(
                    target_config,
                    target_journal.clone(),
                    auth,
                    imported()
                ),
            )
        })
        .await
        .unwrap();
        assert_ne!(
            first.is_ok(),
            second.is_ok(),
            "exactly one native import wins"
        );
        let target = first.or(second).unwrap();
        assert_eq!(target.thread_id, source.thread_id);
        assert_eq!(
            target.session_configured.cwd,
            root.path().join("target-workspace")
        );
        assert!(
            target_manager
                .start_thread(
                    config(
                        &root.path().join("target-home"),
                        &root.path().join("target-workspace"),
                        &model.url
                    )
                    .await
                )
                .await
                .is_err(),
            "native manager cannot be reused by ordinary startup"
        );
        turn(&target.thread, "continue original fixture session").await;
        tokio::time::timeout(Duration::from_secs(10), target.thread.shutdown_and_wait())
            .await
            .unwrap()
            .unwrap();
        let (_, after) = target.thread.capture_native_state().await.unwrap();
        let value: Value = serde_json::from_slice(after.as_bytes()).unwrap();
        assert_eq!(value["sessionId"], source.thread_id.to_string());
        assert!(value["history"]
            .to_string()
            .contains("source original fixture message"));
        assert!(value["history"]
            .to_string()
            .contains("source fixture reply"));
        assert!(value["history"]
            .to_string()
            .contains("continue original fixture session"));
        assert_eq!(
            value["provider"]["lastResponse"]["responseId"],
            "fixture-target-response"
        );
        assert_eq!(
            journal.read_bytes(64 * 1024 * 1024).unwrap(),
            journal_bytes,
            "source checkpoint journal is immutable"
        );
        assert_eq!(state.as_bytes(), original);
    });
    let requests = model.finish();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[1]["previous_response_id"],
        "fixture-source-response"
    );
    assert!(requests[1]["input"]
        .to_string()
        .contains("continue original fixture session"));
}
