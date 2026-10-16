// Origin: CTOX
// License: AGPL-3.0-only
use super::super::*;
use super::*;
use base64::Engine as _;
use ctox_cliproxyapi::internal::auth::codex::CodexSecretStore as _;

#[tokio::test]
async fn native_model_check_obeys_native_effect_fence_before_account_io() {
    let root = tempfile::tempdir().unwrap();
    let entered = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observed = entered.clone();
    let fence: crate::business_os::app_platform::ModelEffectFence =
        std::sync::Arc::new(move |_effect| {
            observed.store(true, std::sync::atomic::Ordering::SeqCst);
            anyhow::bail!("revoked native invocation")
        });
    let result = check(
        root.path().to_path_buf(),
        "owner".into(),
        "account".into(),
        1,
        "model".into(),
        Some(fence),
    )
    .await;
    assert!(
        entered.load(std::sync::atomic::Ordering::SeqCst),
        "native effect fence must be consulted"
    );
    assert_eq!(result.status, "unavailable");
    assert_eq!(result.failure.as_deref(), Some("authority_unavailable"));
    assert!(
        std::fs::read_dir(root.path()).unwrap().next().is_none(),
        "rejected effect must not open account stores"
    );
}

#[test]
fn native_model_probe_requires_executor_upstream_evidence() {
    let success = || {
        OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::json(
            200,
            br#"{"object":"response","status":"completed","output":[]}"#.to_vec(),
        ))
    };
    let local = classify(
        "model",
        "account",
        success(),
        PinnedObservation {
            selected: Some("account".into()),
            ..Default::default()
        },
    );
    assert_eq!(
        local.status, "unavailable",
        "a local 200 is no upstream proof"
    );
    assert_eq!(local.source, "gateway");
    let wrong = classify(
        "model",
        "account",
        success(),
        PinnedObservation {
            selected: Some("other".into()),
            upstream_status: Some(200),
            error_class: None,
        },
    );
    assert_eq!(wrong.status, "unavailable", "another account is no proof");
}

#[test]
fn native_model_probe_real_transport_pins_the_account_without_fallback() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_path_buf();
    crate::app_capabilities::cap3::tests::run_with_core_owners(path.clone(), move || {
        drive_native(async move {
            use tokio::io::{AsyncReadExt,AsyncWriteExt};
            let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr=listener.local_addr().unwrap();
            let secret=|name:&str| RuntimeSecretRef {scope:"model-probe-fixture".into(),name:name.into()};
            let config=CliproxyRuntimeConfig {request_timeout_ms:5000,
                routing_strategy:SchedulerStrategy::RoundRobin,claude_accounts:vec![],antigravity_accounts:vec![],
                codex_accounts:vec![CodexSubscriptionAccountConfig {
                    id:"exact-account".into(),disabled:false,priority:100,weight:1,websockets:false,
                    models:vec!["fixture-model".into()],id_token_secret:secret("id"),access_token_secret:secret("access"),
                    refresh_token_secret:secret("refresh"),upstream_base_url:format!("http://{addr}/backend-api/codex"),
                    plan_type:"pro".into(),proxy_url_secret:None }]};
            let claims=json!({"email":"fixture@example.invalid","https://api.openai.com/auth":{
                "chatgpt_plan_type":"pro","chatgpt_account_id":"workspace-fixture","chatgpt_user_id":"user-fixture"}});
            let token=format!("e30.{}.signature",base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap()));
            CtoxCodexSecretStore::new(&path).store_credentials(&config.codex_accounts[0].credential_handles().unwrap(),
                &codex_auth::CodexStoredCredentials::new(codex_auth::SecretString::new(token).unwrap(),
                codex_auth::SecretString::new("fixture-access").unwrap(),codex_auth::SecretString::new("fixture-refresh").unwrap())).unwrap();
            let router=build_codex_responses_router(&path,&config.validate().unwrap()).unwrap();
            let (stop,mut stopping)=tokio::sync::oneshot::channel::<()>();
            let server=tokio::spawn(async move {
                let mut requests=Vec::new();
                loop {
                    let (mut stream,_)=tokio::select! { _=&mut stopping=>break, accepted=listener.accept()=>accepted.unwrap() };
                    let mut bytes=Vec::new();
                    let end=loop {let mut chunk=[0;4096];let n=stream.read(&mut chunk).await.unwrap();assert!(n>0);bytes.extend_from_slice(&chunk[..n]);if let Some(p)=bytes.windows(4).position(|w|w==b"\r\n\r\n"){break p+4;}};
                    let headers=String::from_utf8_lossy(&bytes[..end]);
                    let n=headers.lines().find_map(|l| {let (k,v)=l.split_once(':')?;k.eq_ignore_ascii_case("content-length").then(||v.trim().parse::<usize>().unwrap())}).unwrap();
                    while bytes.len()<end+n {let mut chunk=[0;4096];let read=stream.read(&mut chunk).await.unwrap();assert!(read>0);bytes.extend_from_slice(&chunk[..read]);}
                    let request:Value=serde_json::from_slice(&bytes[end..end+n]).unwrap();
                    assert_eq!(request["model"],"fixture-model");
                    requests.push(request);
                    let body="data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_probe\",\"object\":\"response\",\"status\":\"completed\",\"output\":[]}}\n\n";
                    let response=format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
                requests
            });
            let actual=probe_route(router.as_ref(),"codex","exact-account","fixture-model").await;
            assert_eq!(actual.status,"ok","{}",serde_json::to_string(&actual).unwrap());
            assert_eq!(actual.source,"upstream");assert_eq!(actual.http_status,Some(200));
            let missing=probe_route(router.as_ref(),"codex","absent-account","fixture-model").await;
            assert_eq!(missing.status,"unavailable");
            stop.send(()).unwrap();
            let requests=server.await.unwrap();
            assert_eq!(requests.len(),1,"a missing pin must never use another account");
            assert!(account_selection::requested_account().is_none(),"pin leaked after call");
        }).expect("bounded native provider IO completed");
    });
}
