// ref: internal/helps/xai_client_version_test.go @ d318bcc3afb9ea8782862f5e8cdb541afdc9dfa5
// Port-Status: adapted_to_ctox — isolated transport, virtual time, owner replacement
// License: MIT (upstream); modifications AGPL-3.0-only

use super::*;
use crate::internal::runtime::executor::xai_executor_request::apply_xai_chat_headers_with_version;
use crate::sdk::cliproxy::auth::Auth;
use crate::sdk::cliproxy::executor::Headers;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Scripted {
    replies: Mutex<VecDeque<Result<Vec<u8>, VersionFetchError>>>,
    calls: AtomicUsize,
}
impl XaiVersionFetcher for Scripted {
    fn fetch(&self) -> VersionFetchFuture<'_> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.replies
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Err(VersionFetchError::Transport))
        })
    }
}
fn source(replies: Vec<Result<Vec<u8>, VersionFetchError>>) -> Arc<Scripted> {
    Arc::new(Scripted {
        replies: Mutex::new(replies.into()),
        calls: AtomicUsize::new(0),
    })
}
fn metadata(version: &str) -> Result<Vec<u8>, VersionFetchError> {
    Ok(serde_json::to_vec(&serde_json::json!({"version":version})).unwrap())
}
async fn flush() {
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
}

#[test]
fn only_strict_numeric_versions_at_or_above_server_floor_are_accepted() {
    for version in ["1.0.13", "1.0.46", " 1.0.50 ", "1.01.0013", "2.0.0"] {
        assert_eq!(
            parse_version(&metadata(version).unwrap()).unwrap(),
            version.trim()
        );
    }
    for version in [
        "",
        "1.0.12",
        "0.9.999",
        "1.0",
        "1.0.46.1",
        "v1.0.46",
        "1.0.46-alpha",
        "1.0.46+build",
        "+1.0.46",
        "1.0.４６",
        "1.0.9223372036854775808",
        "1.0.18446744073709551616",
    ] {
        assert_eq!(
            parse_version(&metadata(version).unwrap()),
            Err(VersionFetchError::Malformed),
            "{version}"
        );
    }
    for body in [
        b"{}".as_slice(),
        b"{\"version\":46}",
        b"{\"version\":null}",
        b"not-json",
    ] {
        assert_eq!(parse_version(body), Err(VersionFetchError::Malformed));
    }
    assert_eq!(
        parse_version(&vec![b'x'; XAI_VERSION_MAX_BODY_BYTES + 1]),
        Err(VersionFetchError::BodyTooLarge)
    );
}

#[tokio::test(start_paused = true)]
async fn refresh_starts_once_and_preserves_cached_version_on_failure_without_retry() {
    let cache = Arc::new(XaiClientVersion::default());
    let source = source(vec![
        metadata("1.0.47"),
        Err(VersionFetchError::HttpStatus),
        metadata("1.0.48"),
    ]);
    assert_eq!(cache.current(), DEFAULT_XAI_FALLBACK_CLIENT_VERSION);
    let owner = cache.start(source.clone());
    flush().await;
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
    assert_eq!(cache.current(), "1.0.47");
    tokio::time::advance(XAI_VERSION_REFRESH_INTERVAL - Duration::from_secs(1)).await;
    flush().await;
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
    tokio::time::advance(Duration::from_secs(1)).await;
    flush().await;
    assert_eq!(source.calls.load(Ordering::SeqCst), 2);
    assert_eq!(cache.current(), "1.0.47");
    tokio::time::advance(Duration::from_secs(60)).await;
    flush().await;
    assert_eq!(source.calls.load(Ordering::SeqCst), 2);
    tokio::time::advance(XAI_VERSION_REFRESH_INTERVAL - Duration::from_secs(60)).await;
    flush().await;
    assert_eq!(source.calls.load(Ordering::SeqCst), 3);
    assert_eq!(cache.current(), "1.0.48");
    owner.stop().await;
    tokio::time::advance(XAI_VERSION_REFRESH_INTERVAL).await;
    flush().await;
    assert_eq!(source.calls.load(Ordering::SeqCst), 3);
}

struct Pending {
    calls: AtomicUsize,
    dropped: Arc<AtomicUsize>,
}
struct Dropped(Arc<AtomicUsize>);
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl XaiVersionFetcher for Pending {
    fn fetch(&self) -> VersionFetchFuture<'_> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let _guard = Dropped(self.dropped.clone());
            std::future::pending().await
        })
    }
}
fn pending() -> Arc<Pending> {
    Arc::new(Pending {
        calls: AtomicUsize::new(0),
        dropped: Arc::new(AtomicUsize::new(0)),
    })
}

#[tokio::test(start_paused = true)]
async fn fetch_timeout_cancels_the_inflight_body_and_keeps_fallback() {
    let cache = Arc::new(XaiClientVersion::default());
    let source = pending();
    let owner = cache.start(source.clone());
    flush().await;
    tokio::time::advance(XAI_VERSION_FETCH_TIMEOUT).await;
    flush().await;
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
    assert_eq!(source.dropped.load(Ordering::SeqCst), 1);
    assert_eq!(cache.current(), DEFAULT_XAI_FALLBACK_CLIENT_VERSION);
    owner.stop().await;
}

#[tokio::test(start_paused = true)]
async fn replacement_cancels_old_fetch_and_old_guard_cannot_stop_new_owner() {
    let cache = Arc::new(XaiClientVersion::default());
    let old_source = pending();
    let old = cache.start(old_source.clone());
    flush().await;
    let new_source = source(vec![metadata("1.0.47"), metadata("1.0.48")]);
    let new = cache.start(new_source.clone());
    flush().await;
    assert_eq!(old_source.dropped.load(Ordering::SeqCst), 1);
    assert_eq!(cache.current(), "1.0.47");
    drop(old);
    tokio::time::advance(XAI_VERSION_REFRESH_INTERVAL).await;
    flush().await;
    assert_eq!(new_source.calls.load(Ordering::SeqCst), 2);
    assert_eq!(cache.current(), "1.0.48");
    new.stop().await;
}

#[tokio::test(start_paused = true)]
async fn dropping_owner_cancels_an_inflight_fetch() {
    let cache = Arc::new(XaiClientVersion::default());
    let source = pending();
    let owner = cache.start(source.clone());
    flush().await;
    drop(owner);
    flush().await;
    assert_eq!(source.dropped.load(Ordering::SeqCst), 1);
    assert!(cache.state.lock().unwrap().active.is_none());
}

#[tokio::test(start_paused = true)]
async fn malformed_and_oversized_metadata_never_replaces_an_accepted_version() {
    let cache = Arc::new(XaiClientVersion::default());
    let source = source(vec![
        metadata("1.0.47"),
        metadata("1.0.12"),
        Ok(vec![b'x'; XAI_VERSION_MAX_BODY_BYTES + 1]),
        metadata("1.0.46-alpha"),
    ]);
    let owner = cache.start(source);
    flush().await;
    for _ in 0..3 {
        tokio::time::advance(XAI_VERSION_REFRESH_INTERVAL).await;
        flush().await;
        assert_eq!(cache.current(), "1.0.47");
    }
    owner.stop().await;
}

#[tokio::test]
async fn refreshed_identity_is_consistent_and_scoped_to_official_oauth_origin() {
    let cache = Arc::new(XaiClientVersion::default());
    let owner = cache.start(source(vec![metadata("1.0.47")]));
    flush().await;
    let version = cache.current();
    let mut auth = Auth::default();
    auth.attributes.insert("auth_kind".into(), "oauth".into());
    let mut headers = Headers::new();
    apply_xai_chat_headers_with_version(&mut headers, Some(&auth), "token", false, "", &version);
    assert_eq!(headers["x-grok-client-version"], vec!["1.0.47"]);
    assert_eq!(headers["User-Agent"], vec!["xai-grok-workspace/1.0.47"]);
    auth.attributes.insert("using_api".into(), "true".into());
    let mut api = Headers::new();
    apply_xai_chat_headers_with_version(&mut api, Some(&auth), "token", false, "", &version);
    assert!(!api.contains_key("x-grok-client-version"));
    assert!(!api.contains_key("User-Agent"));
    auth.attributes.remove("using_api");
    auth.attributes
        .insert("base_url".into(), "https://edge.example/v1".into());
    let mut custom = Headers::new();
    apply_xai_chat_headers_with_version(&mut custom, Some(&auth), "token", false, "", &version);
    assert!(!custom.contains_key("x-grok-client-version"));
    auth.attributes.remove("base_url");
    auth.attributes.insert(
        "header:x-grok-client-version".into(),
        "custom-version".into(),
    );
    let mut overridden = Headers::new();
    apply_xai_chat_headers_with_version(&mut overridden, Some(&auth), "token", false, "", &version);
    assert_eq!(overridden["x-grok-client-version"], vec!["custom-version"]);
    owner.stop().await;
}

#[cfg(any(
    feature = "anthropic-fingerprint-transport",
    feature = "codex-http-transport"
))]
async fn http_case(status: u16, body: Vec<u8>) -> Result<Vec<u8>, VersionFetchError> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/latest", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        loop {
            let mut chunk = [0; 512];
            let count = socket.read(&mut chunk).await.unwrap();
            assert!(count > 0 && request.len() + count <= 4096);
            request.extend_from_slice(&chunk[..count]);
            if request.windows(4).any(|x| x == b"\r\n\r\n") {
                break;
            }
        }
        let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
        assert!(request.starts_with("get /latest http/1.1"));
        assert!(request.contains("accept: application/json"));
        assert!(request.contains("user-agent: cliproxyapi"));
        assert!(!request.contains("authorization:"));
        assert!(!request.contains("x-xai-token-auth:"));
        let header = format!(
            "HTTP/1.1 {status} response\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        socket.write_all(header.as_bytes()).await.unwrap();
        // Rejecting a non-200 or oversized body can close the client socket.
        let _ = socket.write_all(&body).await;
    });
    let mut fetcher = HttpXaiVersionFetcher::new(None).unwrap();
    fetcher.registry_url = url;
    let result = tokio::time::timeout(XAI_VERSION_FETCH_TIMEOUT, fetcher.fetch())
        .await
        .unwrap();
    server.await.unwrap();
    result
}

#[cfg(any(
    feature = "anthropic-fingerprint-transport",
    feature = "codex-http-transport"
))]
#[tokio::test]
async fn actual_http_fetch_is_anonymous_requires_200_and_bounds_the_body() {
    let response = http_case(200, metadata("1.0.47").unwrap()).await.unwrap();
    assert_eq!(parse_version(&response).unwrap(), "1.0.47");
    assert_eq!(
        http_case(201, metadata("1.0.47").unwrap()).await,
        Err(VersionFetchError::HttpStatus)
    );
    assert_eq!(
        http_case(401, b"private rejected body".to_vec()).await,
        Err(VersionFetchError::HttpStatus)
    );
    assert_eq!(
        http_case(200, vec![b'x'; XAI_VERSION_MAX_BODY_BYTES + 1]).await,
        Err(VersionFetchError::BodyTooLarge)
    );
}

#[cfg(any(
    feature = "anthropic-fingerprint-transport",
    feature = "codex-http-transport"
))]
#[test]
fn configured_proxy_errors_are_redacted_and_do_not_fall_back_to_direct() {
    match HttpXaiVersionFetcher::new(Some("http://secret-user:secret-password@")) {
        Ok(_) => panic!("invalid configured proxy was accepted"),
        Err(error) => {
            assert_eq!(error, VersionFetchError::Transport);
            assert!(!error.to_string().contains("secret"));
        }
    }
}
