// Source guard for the actual CTOX embedding router. The fixture has no
// external provider or live credential authority.
struct CandidateInstanceHeaderStream;

impl AntigravityGenerateTransport for CandidateInstanceHeaderStream {
    fn execute<'a>(
        &'a self,
        _request: &'a AntigravityGenerateRequest,
        _timeout: Duration,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        AntigravityGenerateResponse,
                        AntigravityGenerateTransportFailure,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async { Err(AntigravityGenerateTransportFailure::Protocol) })
    }
}

impl ctox_cliproxyapi::internal::runtime::executor::AntigravityGenerateStreamingTransport
    for CandidateInstanceHeaderStream
{
    fn execute_stream<'a>(
        &'a self, _request: &'a AntigravityGenerateRequest, _timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<
        ctox_cliproxyapi::internal::runtime::executor::AntigravityGenerateStreamResponse,
        AntigravityGenerateTransportFailure,
    >> + Send + 'a>>{
        Box::pin(async {
            let (sender, receiver) = tokio::sync::mpsc::channel(2);
            sender.send(Ok(br#"data: {"response":{"responseId":"header-probe","candidates":[{"content":{"parts":[{"text":"partial"}]}}]}}

"#.to_vec())).await.unwrap();
            sender
                .send(Err(AntigravityGenerateTransportFailure::Protocol))
                .await
                .unwrap();
            drop(sender);
            Ok(ctox_cliproxyapi::internal::runtime::executor::AntigravityGenerateStreamResponse::new(
                200, None, receiver,
            ))
        })
    }
}

fn candidate_instance_header_router(root: &Path) -> InstanceResponsesRouter {
    let secret = |name: &str| ctox_cliproxyapi::internal::config::RuntimeSecretRef {
        scope: "provider-subscriptions".to_owned(),
        name: name.to_owned(),
    };
    let config = ctox_cliproxyapi::internal::config::CliproxyRuntimeConfig {
        request_timeout_ms: 5_000,
        routing_strategy: SchedulerStrategy::RoundRobin,
        claude_accounts: Vec::new(),
        codex_accounts: Vec::new(),
        antigravity_accounts: vec![
            ctox_cliproxyapi::internal::config::AntigravitySubscriptionAccountConfig {
                id: "header-account".to_owned(),
                disabled: false,
                priority: 0,
                weight: 1,
                websockets: false,
                models: Vec::new(),
                access_token_secret: secret("header-access"),
                refresh_token_secret: secret("header-refresh"),
                state_secret: secret("header-state"),
                upstream_base_url: "https://daily-cloudcode-pa.googleapis.com".to_owned(),
                proxy_url_secret: None,
            },
        ],
    }
    .validate()
    .unwrap();
    CtoxAntigravitySecretStore::new(root)
        .store_credentials(
            &config.antigravity_accounts()[0]
                .credential_handles()
                .unwrap(),
            &antigravity_credentials(
                "header-access-do-not-leak",
                "header-refresh-do-not-leak",
                SystemTime::UNIX_EPOCH + Duration::from_secs(3_600),
                "header-project",
            ),
        )
        .unwrap();
    let transport = Arc::new(CandidateInstanceHeaderStream);
    let transports = HashMap::from([(
        "header-account".to_owned(),
        AntigravityAccountTransports {
            refresh: Arc::new(UnusedAntigravityRefreshTransport),
            generate: transport.clone(),
            generate_stream: Some(transport),
        },
    )]);
    let pool = Arc::new(
        CtoxCliproxyRuntimeFactory::new(root)
            .build_antigravity_pool(
                &config,
                &transports,
                Arc::new(FixedAntigravityRuntimeClock),
                Arc::new(FixedRuntimeClock),
            )
            .unwrap(),
    );
    InstanceResponsesRouter {
        xai_root: None,
        default_provider: "antigravity".to_owned(),
        portable: Some(Arc::new(
            OpenAiResponsesProviderRouter::new(
                "antigravity",
                None,
                None,
                Some(Arc::new(OpenAiResponsesAntigravityHandler::new(pool))),
            )
            .unwrap(),
        )),
        kimi: None,
    }
}

#[tokio::test]
async fn candidate_cliproxy_host_headers_survive_instance_router() {
    let body = br#"{"model":"gemini-3-flash-agent","stream":true,"input":"hello"}"#;
    for (codex, provider) in [(true, None), (true, Some(" Antigravity ")), (false, None)] {
        let root = tempfile::tempdir().unwrap();
        let router = candidate_instance_header_router(root.path());
        let headers = std::collections::BTreeMap::from([(
            "oRiGiNaToR".to_owned(),
            vec![if codex { "codex_cli_rs" } else { "workjet" }.to_owned()],
        )]);
        let response = router
            .handle_provider_route_with_headers(provider, &headers, body)
            .await;
        let OpenAiResponsesRouteResponse::AntigravityStream(mut stream) = response else {
            panic!("expected the actual Antigravity tracked stream");
        };
        let mut output = Vec::new();
        tokio::time::timeout(Duration::from_secs(3), async {
            while let Some(chunk) = stream.next_chunk().await {
                output.extend(chunk);
            }
        })
        .await
        .expect("header fixture must terminate");
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("partial"), "{text}");
        assert!(
            text.contains(if codex {
                "event: response.failed"
            } else {
                "event: error"
            }),
            "{text}"
        );
        assert!(
            !text.contains(if codex {
                "event: error"
            } else {
                "event: response.failed"
            }),
            "{text}"
        );
        assert!(
            !text.contains("header-access-do-not-leak")
                && !text.contains("header-refresh-do-not-leak")
        );
    }
}
