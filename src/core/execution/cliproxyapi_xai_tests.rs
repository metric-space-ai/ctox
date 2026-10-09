use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
                r#"{"id":"response-fixture","object":"response","output":[]}"#,
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
