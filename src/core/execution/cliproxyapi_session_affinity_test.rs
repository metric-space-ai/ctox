// Origin: CTOX; License: AGPL-3.0-only
// Included in the existing host tests; all credentials belong to a synthetic isolated root.
#[derive(Default)]
struct SessionMessagesTransport { headers: ctox_cliproxyapi::sdk::cliproxy::executor::Headers }
impl ClaudeMessagesTransport for SessionMessagesTransport {
    fn execute<'a>(
        &'a self,
        request: &'a ClaudeMessagesRequest,
        _: Duration,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<ClaudeMessagesResponse, ClaudeMessagesTransportFailure>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let body: serde_json::Value = serde_json::from_slice(request.body()).unwrap();
            assert_eq!(body["stream"], true);
            let events = [
                serde_json::json!({"type":"message_start","message":{
                    "id":"message-test","type":"message","role":"assistant",
                    "model":body["model"],"content":[],
                    "usage":{"input_tokens":2,"output_tokens":0}
                }}),
                serde_json::json!({"type":"content_block_start","index":0,
                    "content_block":{"type":"text","text":""}}),
                serde_json::json!({"type":"content_block_delta","index":0,
                    "delta":{"type":"text_delta","text":"Hi"}}),
                serde_json::json!({"type":"content_block_stop","index":0}),
                serde_json::json!({"type":"message_delta",
                    "delta":{"stop_reason":"end_turn","stop_sequence":null},
                    "usage":{"output_tokens":1}}),
                serde_json::json!({"type":"message_stop"}),
            ];
            let mut bytes = Vec::new();
            for event in events {
                bytes.extend_from_slice(format!("data: {event}\n\n").as_bytes());
            }
            // Buffered Responses clients still request SSE upstream. The native
            // transport buffers that event stream before Responses aggregation.
            Ok(ClaudeMessagesResponse::new(200, bytes).with_headers(self.headers.clone()))
        })
    }
}

async fn session_http_turn(
    root: &Path,
    config: CliproxyRuntimeConfig,
    model: &str,
    account: Option<&str>,
) -> String {
    session_http_turn_reported(root, config, model, account, "private-native-session", &HashMap::new()).await
}
async fn session_http_turn_reported(
    root: &Path,
    config: CliproxyRuntimeConfig,
    model: &str,
    account: Option<&str>,
    session: &str,
    reported: &HashMap<String, ctox_cliproxyapi::sdk::cliproxy::executor::Headers>,
) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let config = config.validate().unwrap();
    let transports = config
        .claude_accounts()
        .iter()
        .map(|entry| {
            (
                entry.id.clone(),
                ClaudeAccountTransports {
                    refresh: Arc::new(UnusedRefreshTransport),
                    messages: Arc::new(SessionMessagesTransport { headers: reported.get(&entry.id).cloned().unwrap_or_default() }),
                    messages_stream: None,
                },
            )
        })
        .collect();
    let pool = CtoxCliproxyRuntimeFactory::new(root)
        .build_claude_pool(
            &config,
            &transports,
            Arc::new(FixedRuntimeClock),
            Arc::new(FixedRuntimeClock),
        )
        .unwrap();
    let handler = OpenAiResponsesClaudeHandler::new(Arc::new(pool));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        ctox_cliproxyapi::internal::api::server::serve_one_responses_connection(
            &listener, &handler,
        )
        .await
        .unwrap();
    });
    let body = serde_json::json!({"model":model,"input":"Hi","stream":false}).to_string();
    let pin = account
        .map(|id| format!("X-CTOX-Account: {id}\r\n"))
        .unwrap_or_default();
    let request=format!("POST /v1/responses HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nX-Session-ID: {session}\r\n{pin}Content-Length: {}\r\n\r\n{body}",body.len());
    let mut client = tokio::net::TcpStream::connect(address).await.unwrap();
    client.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), client.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    server.await.unwrap();
    let response = String::from_utf8(response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("Hi"), "{response}");
    response
        .lines()
        .find_map(|line| line.strip_prefix("X-CTOX-Account-Selected: "))
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn provider_session_native_http_continuation_survives_rebuild_failover_and_exact_model_check()
{
    let root = tempfile::tempdir().unwrap();
    let mut config = stored_claude_runtime();
    let model = config.claude_accounts[0].models[0].clone();
    config.routing_strategy = SchedulerStrategy::RoundRobin;
    let mut a = config.claude_accounts.remove(0);
    a.id = "account-a".into();
    a.priority = 0;
    a.weight = 1;
    let mut b = a.clone();
    b.id = "account-b".into();
    b.access_token_secret.name = "second-access".into();
    b.refresh_token_secret.name = "second-refresh".into();
    config.claude_accounts = vec![a, b];
    for account in &config.claude_accounts {
        CtoxClaudeSecretStore::new(root.path())
            .store_credentials(
                &account.credential_handles().unwrap(),
                &credentials("synthetic-access", "synthetic-refresh"),
            )
            .unwrap();
    }
    assert_eq!(
        session_http_turn(root.path(), config.clone(), &model, None).await,
        "account-a"
    );
    // Each turn rebuilds the complete native pool. Raising another account's
    // priority must not discard the continuation's already usable cache lane.
    config.claude_accounts[1].priority = 100;
    assert_eq!(
        session_http_turn(root.path(), config.clone(), &model, None).await,
        "account-a"
    );
    config.claude_accounts[0].disabled = true;
    assert_eq!(
        session_http_turn(root.path(), config.clone(), &model, None).await,
        "account-b"
    );
    config.claude_accounts[0].disabled = false;
    config.claude_accounts[0].priority = 200;
    assert_eq!(
        session_http_turn(root.path(), config.clone(), &model, None).await,
        "account-b"
    );
    assert_eq!(
        session_http_turn(root.path(), config.clone(), &model, Some("account-a")).await,
        "account-a"
    );
    assert_eq!(
        session_http_turn(root.path(), config, &model, None).await,
        "account-b"
    );
}

#[tokio::test]
async fn provider_quota_native_http_observations_rank_new_sessions_without_moving_continuations() {
    use ctox_cliproxyapi::sdk::cliproxy::auth::AccountQuotaStore;
    let root = tempfile::tempdir().unwrap();
    let mut config = stored_claude_runtime();
    let model = config.claude_accounts[0].models[0].clone();
    let mut a = config.claude_accounts.remove(0); a.id = "account-a".into();
    let mut b = a.clone(); b.id = "account-b".into();
    b.access_token_secret.name = "second-access".into(); b.refresh_token_secret.name = "second-refresh".into();
    config.claude_accounts = vec![a,b];
    for account in &config.claude_accounts {
        CtoxClaudeSecretStore::new(root.path()).store_credentials(
            &account.credential_handles().unwrap(), &credentials("synthetic-access", "synthetic-refresh"),
        ).unwrap();
    }
    let headers = |reset: &str| BTreeMap::from([
        ("Anthropic-Ratelimit-Unified-5h-Status".into(),vec!["allowed".into()]),
        ("Anthropic-Ratelimit-Unified-5h-Reset".into(),vec![reset.into()]),
        ("Anthropic-Ratelimit-Unified-5h-Utilization".into(),vec!["0.25".into()]),
    ]);
    let mut reported = HashMap::from([("account-a".into(),headers("40")),("account-b".into(),headers("20"))]);
    assert_eq!(session_http_turn_reported(root.path(),config.clone(),&model,None,"ongoing-a",&reported).await,"account-a");
    assert_eq!(session_http_turn_reported(root.path(),config.clone(),&model,Some("account-b"),"ongoing-a",&reported).await,"account-b");
    let saved = CtoxAccountQuotaStore::new(root.path()).load("claude", &["account-b".into()]).unwrap();
    assert_eq!(saved["account-b"].windows[0].reset_at_ms,20_000);
    assert_eq!(saved["account-b"].windows[0].utilization,Some(0.25));
    assert_eq!(session_http_turn_reported(root.path(),config.clone(),&model,None,"new-b",&reported).await,"account-b");
    assert_eq!(session_http_turn_reported(root.path(),config.clone(),&model,None,"ongoing-a",&reported).await,"account-a");
    reported.insert("account-a".into(),headers("15"));
    assert_eq!(session_http_turn_reported(root.path(),config.clone(),&model,Some("account-a"),"probe",&reported).await,"account-a");
    assert_eq!(session_http_turn_reported(root.path(),config.clone(),&model,None,"new-a",&reported).await,"account-a");
    assert_eq!(session_http_turn_reported(root.path(),config,&model,None,"new-b",&reported).await,"account-b");
}

