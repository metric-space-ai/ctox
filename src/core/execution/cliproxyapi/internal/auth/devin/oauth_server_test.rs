// ref: internal/auth/devin/devin_auth_test.go:220-260
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only
use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn request(addr: SocketAddr, method: &str, target: &str) -> String {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let head =
            format!("{method} {target} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
        stream.write_all(head.as_bytes()).await.unwrap();
        let mut body = Vec::new();
        stream.read_to_end(&mut body).await.unwrap();
        String::from_utf8(body).unwrap()
    })
    .await
    .expect("bounded local callback request")
}
async fn assert_listener_closed(addr: SocketAddr) {
    assert!(
        TcpStream::connect(addr).await.is_err(),
        "consumed listener must be closed"
    );
    let rebound = TcpListener::bind(addr)
        .await
        .expect("same port can be reused");
    drop(rebound);
}

#[tokio::test]
async fn candidate_devin_callback_success_is_loopback_state_bound_and_owned() {
    let server = OAuthServer::start(0, "unit-state").await.unwrap();
    let addr = server.local_addr().unwrap();
    assert_eq!(addr.ip(), std::net::IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert_eq!(
        server.redirect_uri().unwrap(),
        format!("http://{addr}/callback")
    );
    assert!(!format!("{server:?}").contains("unit-state"));
    let (result, reply) = tokio::join!(
        server.wait_for_callback(),
        request(
            addr,
            "GET",
            "/callback?code=%20secret-code%20&state=%20unit-state%20"
        )
    );
    let result = result.unwrap();
    assert!(result.is_success());
    assert_eq!(result.code(), Some("secret-code"));
    assert_eq!(result.state(), Some("unit-state"));
    assert!(reply.starts_with("HTTP/1.1 200 OK"));
    assert!(reply.contains("Authentication Complete"));
    assert!(reply.contains("Cache-Control: no-store"));
    assert!(reply.contains("Referrer-Policy: no-referrer"));
    assert!(!reply.contains("secret-code"));
    assert!(!format!("{result:?}").contains("secret-code"));
    assert!(!format!("{result:?}").contains("unit-state"));
    assert_listener_closed(addr).await;
}

#[tokio::test]
async fn candidate_devin_callback_unrelated_state_cannot_consume_a_login() {
    let server = OAuthServer::start(0, "correct-state").await.unwrap();
    let addr = server.local_addr().unwrap();
    let (result, replies) = tokio::join!(server.wait_for_callback(), async {
        let invalid = request(addr, "GET", "/callback?code=wrong&state=other-state").await;
        let missing = request(addr, "GET", "/callback?code=wrong").await;
        let fake_error = request(
            addr,
            "GET",
            "/callback?error=access_denied&state=other-state",
        )
        .await;
        let valid = request(
            addr,
            "GET",
            "/callback?code=correct-code&state=correct-state",
        )
        .await;
        (invalid, missing, fake_error, valid)
    });
    assert_eq!(result.unwrap().code(), Some("correct-code"));
    for reply in [&replies.0, &replies.1, &replies.2] {
        assert!(reply.starts_with("HTTP/1.1 400 Bad Request"));
    }
    assert!(replies.3.starts_with("HTTP/1.1 200 OK"));
    assert_listener_closed(addr).await;
}

#[tokio::test]
async fn candidate_devin_callback_error_description_is_escaped_and_terminal() {
    let server = OAuthServer::start(0, "unit-state").await.unwrap();
    let addr = server.local_addr().unwrap();
    let (result, reply) = tokio::join!(server.wait_for_callback(), request(addr, "GET", "/callback?error=access_denied&error_description=%3Cscript%3E%26%22%27&state=unit-state"));
    let result = result.unwrap();
    assert!(!result.is_success());
    assert!(result.code().is_none());
    assert_eq!(result.error(), Some("access_denied: <script>&\"'"));
    assert!(reply.starts_with("HTTP/1.1 400 Bad Request"));
    assert!(reply.contains("access_denied: &lt;script&gt;&amp;&#34;&#39;"));
    assert!(!reply.contains("<script>"));
    assert!(!format!("{result:?}").contains("access_denied"));
    assert_listener_closed(addr).await;
}

#[tokio::test]
async fn candidate_devin_callback_missing_code_keeps_upstream_error_text() {
    let server = OAuthServer::start(0, "unit-state").await.unwrap();
    let addr = server.local_addr().unwrap();
    let (result, reply) = tokio::join!(
        server.wait_for_callback(),
        request(addr, "GET", "/callback?state=unit-state&code=%20")
    );
    assert_eq!(result.unwrap().error(), Some("missing authorization code"));
    assert!(reply.contains("missing authorization code"));
    assert_listener_closed(addr).await;
}

#[tokio::test]
async fn candidate_devin_callback_wrong_path_and_method_do_not_consume_state() {
    let server = OAuthServer::start(0, "unit-state").await.unwrap();
    let addr = server.local_addr().unwrap();
    let (result, replies) = tokio::join!(server.wait_for_callback(), async {
        let favicon = request(addr, "GET", "/favicon.ico").await;
        let post = request(addr, "POST", "/callback?state=unit-state&code=wrong").await;
        let wrong_path = request(addr, "GET", "/auth/callback?state=unit-state&code=wrong").await;
        let valid = request(addr, "GET", "/callback?state=unit-state&code=correct").await;
        (favicon, post, wrong_path, valid)
    });
    assert_eq!(result.unwrap().code(), Some("correct"));
    assert!(replies.0.starts_with("HTTP/1.1 404 Not Found"));
    assert!(replies.1.starts_with("HTTP/1.1 405 Method Not Allowed"));
    assert!(replies.1.contains("Allow: GET"));
    assert!(replies.2.starts_with("HTTP/1.1 404 Not Found"));
    assert!(replies.3.starts_with("HTTP/1.1 200 OK"));
}

#[test]
fn candidate_devin_callback_one_valid_request_claims_the_session_once() {
    let expected: [u8; 32] = Sha256::digest(b"unit-state").into();
    let claimed = AtomicBool::new(false);
    let make = |code: &str| RequestHead {
        method: "GET".into(),
        target: format!("/callback?state=unit-state&code={code}"),
    };
    let (_, first) = route_request(&make("first"), &expected, &claimed);
    let (_, second) = route_request(&make("second"), &expected, &claimed);
    assert_eq!(first.unwrap().code(), Some("first"));
    assert!(second.is_none());
}

#[test]
fn candidate_devin_callback_absolute_authority_fragment_targets_are_rejected() {
    let expected: [u8; 32] = Sha256::digest(b"unit-state").into();
    let claimed = AtomicBool::new(false);
    for target in [
        "https://elsewhere.invalid/callback?state=unit-state&code=x",
        "//elsewhere.invalid/callback?state=unit-state&code=x",
        "/callback?state=unit-state&code=x#fragment",
    ] {
        let (_, result) = route_request(
            &RequestHead {
                method: "GET".into(),
                target: target.into(),
            },
            &expected,
            &claimed,
        );
        assert!(result.is_none());
        assert!(!claimed.load(Ordering::Acquire));
    }
}

#[tokio::test(start_paused = true)]
async fn candidate_devin_callback_timeout_releases_listener() {
    let server = OAuthServer::start_with_timeout(0, "unit-state", Duration::from_millis(10))
        .await
        .unwrap();
    let addr = server.local_addr().unwrap();
    assert_eq!(
        server.wait_for_callback().await.unwrap_err(),
        OAuthServerError::Timeout
    );
    assert_listener_closed(addr).await;
}

#[tokio::test(start_paused = true)]
async fn candidate_devin_callback_deadline_starts_when_login_is_created() {
    let server = OAuthServer::start_with_timeout(0, "unit-state", Duration::from_secs(1))
        .await
        .unwrap();
    let addr = server.local_addr().unwrap();
    tokio::time::advance(Duration::from_secs(2)).await;
    assert_eq!(
        server.wait_for_callback().await.unwrap_err(),
        OAuthServerError::Timeout
    );
    assert_listener_closed(addr).await;
}

#[tokio::test]
async fn candidate_devin_callback_drop_before_wait_releases_listener() {
    let server = OAuthServer::start(0, "unit-state").await.unwrap();
    let addr = server.local_addr().unwrap();
    drop(server);
    assert_listener_closed(addr).await;
}

#[tokio::test]
async fn candidate_devin_callback_cancel_wait_releases_listener_and_pending_io() {
    let server = OAuthServer::start(0, "unit-state").await.unwrap();
    let addr = server.local_addr().unwrap();
    let wait = tokio::spawn(server.wait_for_callback());
    let mut slow_client = TcpStream::connect(addr).await.unwrap();
    slow_client.write_all(b"GET /callback").await.unwrap();
    wait.abort();
    assert!(wait.await.unwrap_err().is_cancelled());
    assert_listener_closed(addr).await;
    let mut byte = [0u8; 1];
    let ended = tokio::time::timeout(Duration::from_secs(5), slow_client.read(&mut byte))
        .await
        .unwrap();
    assert!(matches!(ended, Ok(0) | Err(_)));
}

#[tokio::test]
async fn candidate_devin_callback_blank_state_rejected_before_binding() {
    for state in ["", " ", "\t", " leading", "trailing "] {
        assert_eq!(
            OAuthServer::start(0, state).await.unwrap_err(),
            OAuthServerError::InvalidExpectedState
        );
    }
}

#[tokio::test]
async fn candidate_devin_callback_connection_budget_terminates_and_closes_port() {
    let server = OAuthServer::start(0, "unit-state").await.unwrap();
    let addr = server.local_addr().unwrap();
    let (result, ()) = tokio::join!(server.wait_for_callback(), async {
        for _ in 0..MAX_TOTAL_CONNECTIONS {
            assert!(request(addr, "GET", "/favicon.ico")
                .await
                .starts_with("HTTP/1.1 404 Not Found"));
        }
    });
    assert_eq!(result.unwrap_err(), OAuthServerError::ConnectionLimit);
    assert_listener_closed(addr).await;
}
