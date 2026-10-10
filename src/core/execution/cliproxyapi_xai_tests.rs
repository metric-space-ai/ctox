use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn retired_creator_cancels_without_poll_and_never_commits() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let root = tempfile::tempdir().unwrap();
    let controller = CtoxXaiLogin::with_auth(
        root.path(),
        Arc::new(XaiAuth::new(
            Arc::new(FixtureLogin),
            Arc::new(SystemXaiClock),
            Arc::new(XaiRefreshCoordinator::default()),
        )),
    );
    let current = Arc::new(AtomicBool::new(true));
    let c = current.clone();
    let public = controller
        .start_authorized(
            Arc::new(move || c.load(Ordering::SeqCst)),
            Arc::new(|| Ok(())),
        )
        .await
        .unwrap();
    current.store(false, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(
        controller.poll(&public.login_id).unwrap(),
        XaiLoginProgress::Cancelled
    );
    assert!(!subscription_installed(root.path()));
}
#[tokio::test]
async fn commit_revalidates_authority_after_upstream_acceptance() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let root = tempfile::tempdir().unwrap();
    let controller = CtoxXaiLogin::with_auth(
        root.path(),
        Arc::new(XaiAuth::new(
            Arc::new(FixtureLogin),
            Arc::new(SystemXaiClock),
            Arc::new(XaiRefreshCoordinator::default()),
        )),
    );
    let checks = Arc::new(AtomicUsize::new(0));
    let c = checks.clone();
    let public = controller
        .start_authorized(
            Arc::new(|| true),
            Arc::new(move || {
                anyhow::ensure!(c.fetch_add(1, Ordering::SeqCst) == 0, "retired");
                Ok(())
            }),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while controller.poll(&public.login_id).unwrap() == XaiLoginProgress::Pending {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(checks.load(Ordering::SeqCst), 2);
    assert_eq!(
        controller.poll(&public.login_id).unwrap(),
        XaiLoginProgress::Failed
    );
    assert!(!subscription_installed(root.path()));
}
#[tokio::test]
async fn removal_invalidates_binding_and_cancels_pending() {
    let root = tempfile::tempdir().unwrap();
    let controller = CtoxXaiLogin::with_auth(
        root.path(),
        Arc::new(XaiAuth::new(
            Arc::new(FixtureLogin),
            Arc::new(SystemXaiClock),
            Arc::new(XaiRefreshCoordinator::default()),
        )),
    );
    let public = controller.start().await.unwrap();
    controller.remove().await.unwrap();
    assert_eq!(
        controller.poll(&public.login_id).unwrap(),
        XaiLoginProgress::Cancelled
    );
    save_bundle(root.path(), &bundle()).unwrap();
    let before = credential_binding(root.path()).unwrap();
    assert!(before.is_some());
    controller.remove().await.unwrap();
    assert!(credential_binding(root.path()).unwrap().is_none());
}
struct StalledLogin;
impl XaiHttpTransport for StalledLogin {
    fn execute<'a>(
        &'a self,
        _: &'a XaiHttpRequest,
        _: Duration,
        _: &'a LoginCancellation,
    ) -> XaiHttpFuture<'a> {
        Box::pin(futures_util::future::pending())
    }
}
#[tokio::test]
async fn abandoned_start_releases_pending_admission() {
    let root = tempfile::tempdir().unwrap();
    let controller = CtoxXaiLogin::with_auth(
        root.path(),
        Arc::new(XaiAuth::new(
            Arc::new(StalledLogin),
            Arc::new(SystemXaiClock),
            Arc::new(XaiRefreshCoordinator::default()),
        )),
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(10), controller.start())
            .await
            .is_err()
    );
    assert!(controller
        .sessions
        .lock()
        .unwrap()
        .values()
        .all(|login| login.progress == XaiLoginProgress::Cancelled && login.cancel.is_cancelled()));
    // A second request is admitted (it times out in the fixture transport),
    // rather than failing immediately with "login already pending".
    assert!(
        tokio::time::timeout(Duration::from_millis(10), controller.start())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn catalog_absence_denies_execution() {
    let root = tempfile::tempdir().unwrap();
    save_bundle(root.path(), &bundle()).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut data = [0; 8192];
        let read = socket.read(&mut data).await.unwrap();
        assert!(data[..read].starts_with(b"GET /models"));
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"data\":[]}",
            )
            .await
            .unwrap();
        drop(socket);
        assert!(
            tokio::time::timeout(Duration::from_millis(200), listener.accept())
                .await
                .is_err()
        );
    });
    assert!(execute_route_at(
        root.path(),
        br#"{"model":"grok-4.7","input":"fixture"}"#,
        &endpoint
    )
    .await
    .is_err());
    server.await.unwrap();
}

#[tokio::test]
async fn native_router_starts_for_subscription_without_creating_a_default() {
    use ctox_cliproxyapi::sdk::api::handlers::openai::openai_responses_handlers::{
        OpenAiResponsesRouteHandler, OpenAiResponsesRouteResponse,
    };
    let root = tempfile::tempdir().unwrap();
    save_bundle(root.path(), &bundle()).unwrap();
    let router =
        crate::execution::cliproxyapi_host::build_instance_codex_responses_router(root.path())
            .unwrap()
            .unwrap();
    let response = router
        .handle_provider_route(None, br#"{"model":"grok-4.7","input":"fixture"}"#)
        .await;
    assert!(matches!(
        response,
        OpenAiResponsesRouteResponse::Buffered(_)
    ));
    assert!(
        crate::execution::cliproxyapi_host::load_instance_proxy_config(root.path())
            .unwrap()
            .is_none()
    );
}

struct FixtureLogin;
impl XaiHttpTransport for FixtureLogin {
    fn execute<'a>(
        &'a self,
        request: &'a XaiHttpRequest,
        _: Duration,
        _: &'a LoginCancellation,
    ) -> XaiHttpFuture<'a> {
        Box::pin(async move {
            let body = if request.url.ends_with("openid-configuration") {
                r#"{"device_authorization_endpoint":"https://auth.x.ai/device","token_endpoint":"https://auth.x.ai/token"}"#
            } else if request.url.ends_with("/device") {
                r#"{"device_code":"private-device-fixture","user_code":"PUBLIC-CODE","verification_uri":"https://auth.x.ai/activate","expires_in":600,"interval":5}"#
            } else {
                r#"{"access_token":"fixture-access","refresh_token":"fixture-refresh","token_type":"Bearer","expires_in":3600}"#
            };
            Ok(XaiHttpResponse::new(200, body.as_bytes().to_vec()))
        })
    }
}
#[tokio::test]
async fn device_start_projects_only_public_code_and_accepts_into_secret_store() {
    let root = tempfile::tempdir().unwrap();
    let controller = CtoxXaiLogin::with_auth(
        root.path(),
        Arc::new(XaiAuth::new(
            Arc::new(FixtureLogin),
            Arc::new(SystemXaiClock),
            Arc::new(XaiRefreshCoordinator::default()),
        )),
    );
    let public = controller.start().await.unwrap();
    let json = serde_json::to_string(&public).unwrap();
    assert!(json.contains("PUBLIC-CODE"));
    assert!(!json.contains("private-device-fixture"));
    assert!(!json.contains("fixture-access"));
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if controller.poll(&public.login_id).unwrap() == XaiLoginProgress::Accepted {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(crate::secrets::secret_exists(root.path(), SCOPE, NAME).unwrap());
}

fn bundle() -> AuthBundle {
    AuthBundle {
        token_data: TokenData::new(
            SecretString::new("fixture-access").unwrap(),
            Some(SecretString::new("fixture-refresh").unwrap()),
            None,
            "Bearer",
            3600,
            Some(std::time::SystemTime::now() + Duration::from_secs(3600)),
            "",
            "",
        ),
        last_refresh: std::time::SystemTime::now(),
        base_url: CLI_CHAT_PROXY_BASE_URL.into(),
        redirect_uri: String::new(),
        token_endpoint: "https://auth.x.ai/token".into(),
    }
}
#[tokio::test]
async fn accepted_subscription_routes_only_live_catalog_models() {
    let root = tempfile::tempdir().unwrap();
    save_bundle(root.path(), &bundle()).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        for (path, body) in [
            ("GET /models", r#"{"data":[{"id":"grok-4.7"}]}"#),
            (
                "POST /responses",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"response-fixture\",\"object\":\"response\",\"output\":[]}}\n\n",
            ),
        ] {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut data = vec![0; 16384];
            let read = socket.read(&mut data).await.unwrap();
            let request = String::from_utf8_lossy(&data[..read]).to_ascii_lowercase();
            assert!(request.starts_with(&path.to_ascii_lowercase()));
            assert!(request.contains("authorization: bearer fixture-access"));
            assert!(request.contains("x-xai-token-auth: xai-grok-cli"));
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        }
    });
    let (stream, response) = execute_route_at(
        root.path(),
        br#"{"model":"grok-4.7","input":"fixture"}"#,
        &endpoint,
    )
    .await
    .unwrap();
    assert!(!stream);
    assert!(String::from_utf8(response)
        .unwrap()
        .contains("response-fixture"));
    server.await.unwrap();
}
#[tokio::test]
async fn cancellation_and_drop_revoke_pending_login() {
    let root = tempfile::tempdir().unwrap();
    let controller = CtoxXaiLogin::new(root.path()).unwrap();
    let cancel = LoginCancellation::default();
    controller.sessions.lock().unwrap().insert(
        "fixture".into(),
        Login {
            cancel: cancel.clone(),
            progress: XaiLoginProgress::Pending,
        },
    );
    assert_eq!(
        controller.cancel("fixture").unwrap(),
        XaiLoginProgress::Cancelled
    );
    assert!(cancel.is_cancelled());
    assert_eq!(
        controller.poll("fixture").unwrap(),
        XaiLoginProgress::Cancelled
    );
    assert!(controller.poll("foreign").is_err());
    let second = LoginCancellation::default();
    controller.sessions.lock().unwrap().insert(
        "second".into(),
        Login {
            cancel: second.clone(),
            progress: XaiLoginProgress::Pending,
        },
    );
    drop(controller);
    assert!(second.is_cancelled());
}
#[tokio::test]
async fn accepted_credentials_are_encrypted_and_not_replaced_by_login() {
    let root = tempfile::tempdir().unwrap();
    save_bundle(root.path(), &bundle()).unwrap();
    let bytes = std::fs::read(crate::secrets::secret_store_path(root.path())).unwrap();
    assert!(!bytes
        .windows(b"fixture-access".len())
        .any(|v| v == b"fixture-access"));
    let controller = CtoxXaiLogin::new(root.path()).unwrap();
    assert!(controller.start().await.is_err());
    let projection = serde_json::to_string(&XaiLoginProgress::Accepted).unwrap();
    assert!(!projection.contains("fixture-access"));
}
