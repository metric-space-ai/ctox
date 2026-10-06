// ref: sdk/auth/devin_test.go:16-439
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only
use super::*;
use crate::sdk::{
    auth::{Manager, ManagerErrorKind},
    cliproxy::auth::{Auth, AuthStatus, AuthStore, AuthStoreError},
    pluginapi::{
        Headers, HostHttpClient, HttpRequest, HttpResponse, HttpStreamChunk, HttpStreamResponse,
        PluginFuture,
    },
};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{mpsc, Notify},
};

struct Client {
    requests: Mutex<Vec<HttpRequest>>,
    replies: Mutex<VecDeque<(u16, Vec<u8>)>>,
}
impl HostHttpClient for Client {
    fn execute<'a>(&'a self, _: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async { panic!("login must use the selected bounded stream transport") })
    }
    fn execute_stream<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request);
            let (status_code, payload) = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("no extra HTTP calls");
            let (sender, chunks) = mpsc::channel(1);
            sender
                .try_send(HttpStreamChunk {
                    payload,
                    error: None,
                })
                .unwrap();
            drop(sender);
            Ok(HttpStreamResponse {
                status_code,
                headers: Headers::new(),
                chunks,
            })
        })
    }
}
struct State;
impl DevinStateGenerator for State {
    fn generate(&self) -> Result<Zeroizing<String>, AuthenticatorError> {
        Ok(Zeroizing::new("expected-state".into()))
    }
}
struct Clock;
impl DevinClock for Clock {
    fn now(&self) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(100)
    }
}
#[derive(Default)]
struct Presenter {
    presentations: Mutex<Vec<(String, Option<u16>)>>,
    presented: Notify,
    prompted: Notify,
    prompts: AtomicUsize,
    prompt_drops: Arc<AtomicUsize>,
    pending_prompt: bool,
}
struct DropCounter(Arc<AtomicUsize>);
impl Drop for DropCounter {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl DevinLoginPresenter for Presenter {
    fn present(&self, challenge: &DevinLoginPresentation) -> Result<(), PromptError> {
        assert_eq!(
            challenge.automatic_browser_allowed(),
            challenge.callback_port().is_some()
        );
        assert!(!format!("{challenge:?}").contains("expected-state"));
        self.presentations
            .lock()
            .unwrap()
            .push((challenge.auth_url().into(), challenge.callback_port()));
        self.presented.notify_one();
        Ok(())
    }
    fn prompt<'a>(
        &'a self,
        callback: PromptCallback,
        message: &'static str,
        _: &'a LoginCancellation,
    ) -> DevinPromptFuture<'a> {
        Box::pin(async move {
            let _owned = DropCounter(self.prompt_drops.clone());
            self.prompts.fetch_add(1, Ordering::SeqCst);
            self.prompted.notify_one();
            if self.pending_prompt {
                std::future::pending().await
            } else {
                callback(message)
            }
        })
    }
}
#[derive(Default)]
struct Store(Mutex<Vec<Auth>>);
impl AuthStore for Store {
    fn list(&self) -> Result<Vec<Auth>, AuthStoreError> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn save(&self, record: &Auth) -> Result<String, AuthStoreError> {
        self.0.lock().unwrap().push(record.clone());
        Ok("ctox-secret://auth/devin".into())
    }
    fn delete(&self, _: &str) -> Result<(), AuthStoreError> {
        Ok(())
    }
}
fn setup(code: bool, presenter: Arc<Presenter>) -> (Arc<Client>, DevinAuthenticator) {
    let mut replies = VecDeque::new();
    if code {
        replies.push_back((200, br#"{"token":"eyJsynthetic"}"#.to_vec()));
    }
    replies.push_back((
        200,
        br#"{"user_name":"test-user","user_id":"uid","org_id":"org"}"#.to_vec(),
    ));
    replies.push_back((503, Vec::new()));
    let client = Arc::new(Client {
        requests: Mutex::new(Vec::new()),
        replies: Mutex::new(replies),
    });
    let service = DevinAuthService::new(client.clone())
        .with_api_base_url("https://selected.invalid")
        .with_server_base_url("https://status.selected.invalid");
    let auth = DevinAuthenticator::new(Arc::new(service), presenter)
        .with_state_generator(Arc::new(State))
        .with_clock(Arc::new(Clock));
    (client, auth)
}
fn headless(input: &'static str) -> LoginOptions {
    LoginOptions {
        no_browser: true,
        prompt: Some(Arc::new(move |_| Ok(input.into()))),
        ..Default::default()
    }
}
async fn request(port: u16, target: &str) -> String {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        stream
            .write_all(
                format!("GET {target} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        String::from_utf8(bytes).unwrap()
    })
    .await
    .expect("bounded synthetic callback")
}
async fn listener_closed(port: u16) {
    assert!(TcpStream::connect(("127.0.0.1", port)).await.is_err());
    drop(TcpListener::bind(("127.0.0.1", port)).await.unwrap());
}

#[test]
fn candidate_devin_sdk_provider_and_permanent_token_refresh_contract() {
    let (_, auth) = setup(false, Arc::new(Presenter::default()));
    assert_eq!(auth.provider(), "devin");
    assert_eq!(auth.refresh_lead(), None);
    assert!(!format!("{auth:?}").contains("selected.invalid"));
}
#[test]
fn candidate_devin_sdk_manual_paste_matches_primary_forms_and_redacts() {
    for (input, code, token) in [
        ("  ", "", ""),
        ("manual-code", "manual-code", ""),
        (" 'manual-code' ", "manual-code", ""),
        ("\"eyJraw\"", "", "eyJraw"),
        (
            "devin-session-token$eyJraw",
            "",
            "devin-session-token$eyJraw",
        ),
        (
            "http://localhost/callback?code=abc&state=expected-state",
            "abc",
            "",
        ),
        ("?code=abc&state=expected-state", "abc", ""),
        ("code=abc", "abc", ""),
        (
            "https://localhost/#code=abc&state=expected-state",
            "abc",
            "",
        ),
    ] {
        let paste = parse_manual_paste(input, "expected-state").unwrap();
        assert!(!format!("{paste:?}").contains("eyJraw"));
        match paste {
            DevinManualPaste::Empty => assert_eq!((code, token), ("", "")),
            DevinManualPaste::Code(value) => assert_eq!(value.as_str(), code),
            DevinManualPaste::Token(value) => assert_eq!(value.as_str(), token),
        }
    }
    for (input, error) in [
        (
            "https://localhost/?code=abc&state=wrong",
            DevinLoginError::StateMismatch,
        ),
        (
            "https://localhost/?error=denied&error_description=secret",
            DevinLoginError::ProviderRejected,
        ),
        ("invalid callback input", DevinLoginError::UnrecognizedInput),
        (
            "https://localhost/?state=expected-state",
            DevinLoginError::UnrecognizedInput,
        ),
    ] {
        assert_eq!(
            parse_manual_paste(input, "expected-state").unwrap_err(),
            error
        );
    }
}
#[tokio::test]
async fn candidate_devin_sdk_headless_token_uses_manager_selected_transport_and_store() {
    let presenter = Arc::new(Presenter::default());
    let (client, auth) = setup(false, presenter.clone());
    let store = Arc::new(Store::default());
    let manager = Manager::new(
        Some(store.clone()),
        [Arc::new(auth) as Arc<dyn Authenticator>],
    );
    let (record, path) = manager
        .login(
            &LoginCancellation::default(),
            "devin",
            &LoginConfig::default(),
            &headless("eyJsynthetic"),
        )
        .await
        .unwrap();
    assert_eq!(path, "ctox-secret://auth/devin");
    assert_eq!(record.id, "devin-test-user.json");
    assert_eq!(record.status, AuthStatus::Active);
    assert_eq!(record.quota.observed_at.timestamp(), 100);
    assert_eq!(
        record.attributes["session_token"],
        "devin-session-token$eyJsynthetic"
    );
    assert_eq!(store.0.lock().unwrap().len(), 1);
    let requests = client.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].url, "https://selected.invalid/v3/self");
    assert!(requests[1]
        .url
        .starts_with("https://status.selected.invalid/"));
    assert!(requests
        .iter()
        .all(|request| !request.url.ends_with("/token")));
    assert_eq!(presenter.prompts.load(Ordering::SeqCst), 1);
    let presentations = presenter.presentations.lock().unwrap();
    assert_eq!(presentations[0].1, None);
    assert!(presentations[0].0.contains("cli_pkce_marker=1"));
    assert!(!presentations[0].0.contains("redirect_uri="));
}
#[tokio::test]
async fn candidate_devin_sdk_headless_codes_share_this_flows_pkce_verifier() {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    for input in [
        "manual-code",
        "'manual-code'",
        "http://localhost/callback?code=manual-code&state=expected-state",
    ] {
        let presenter = Arc::new(Presenter::default());
        let (client, auth) = setup(true, presenter.clone());
        auth.login(
            &LoginCancellation::default(),
            &LoginConfig::default(),
            &headless(input),
        )
        .await
        .unwrap();
        let requests = client.requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(body["code"], "manual-code");
        let verifier = body["code_verifier"].as_str().unwrap();
        assert_eq!(verifier.len(), 86);
        let url = url::Url::parse(&presenter.presentations.lock().unwrap()[0].0).unwrap();
        let challenge = url
            .query_pairs()
            .find(|(k, _)| k == "code_challenge")
            .unwrap()
            .1;
        assert_eq!(
            challenge,
            URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
        );
        assert!(!requests[0].headers.contains_key("Authorization"));
    }
}
#[tokio::test]
async fn candidate_devin_sdk_invalid_manual_input_never_exchanges_or_persists() {
    for options in [
        headless("https://localhost/?code=secret-code&state=wrong"),
        headless("?error=denied&error_description=secret"),
        headless("invalid callback input"),
        LoginOptions {
            no_browser: true,
            ..Default::default()
        },
    ] {
        let presenter = Arc::new(Presenter::default());
        let (client, auth) = setup(false, presenter);
        let store = Arc::new(Store::default());
        let manager = Manager::new(
            Some(store.clone()),
            [Arc::new(auth) as Arc<dyn Authenticator>],
        );
        let error = manager
            .login(
                &LoginCancellation::default(),
                "devin",
                &LoginConfig::default(),
                &options,
            )
            .await
            .unwrap_err();
        assert_eq!(error.kind, ManagerErrorKind::Authentication);
        assert!(!format!("{error:?}").contains("secret"));
        assert!(client.requests.lock().unwrap().is_empty());
        assert!(store.0.lock().unwrap().is_empty());
    }
}
#[tokio::test]
async fn candidate_devin_sdk_empty_headless_input_cancels_and_prompt_error_fails() {
    let (client, auth) = setup(false, Arc::new(Presenter::default()));
    let empty = auth
        .login(
            &LoginCancellation::default(),
            &LoginConfig::default(),
            &headless(" "),
        )
        .await
        .unwrap_err();
    assert_eq!(empty.kind, AuthenticatorErrorKind::Cancelled);
    let options = LoginOptions {
        no_browser: true,
        prompt: Some(Arc::new(|_| Err(PromptError))),
        ..Default::default()
    };
    let error = auth
        .login(
            &LoginCancellation::default(),
            &LoginConfig::default(),
            &options,
        )
        .await
        .unwrap_err();
    assert_eq!(error.kind, AuthenticatorErrorKind::LoginFailed);
    assert!(client.requests.lock().unwrap().is_empty());
}
#[tokio::test]
async fn candidate_devin_sdk_cancelled_pending_prompt_releases_owned_future() {
    let presenter = Arc::new(Presenter {
        pending_prompt: true,
        ..Default::default()
    });
    let (client, auth) = setup(false, presenter.clone());
    let cancellation = LoginCancellation::default();
    let config = LoginConfig::default();
    let options = headless("unused");
    let (result, ()) = tokio::join!(auth.login(&cancellation, &config, &options), async {
        presenter.prompted.notified().await;
        cancellation.cancel();
    });
    assert_eq!(result.unwrap_err().kind, AuthenticatorErrorKind::Cancelled);
    assert_eq!(presenter.prompt_drops.load(Ordering::SeqCst), 1);
    assert!(client.requests.lock().unwrap().is_empty());
}
#[tokio::test(start_paused = true)]
async fn candidate_devin_sdk_pending_headless_prompt_has_fixed_deadline() {
    let presenter = Arc::new(Presenter {
        pending_prompt: true,
        ..Default::default()
    });
    let (client, auth) = setup(false, presenter.clone());
    let result = auth
        .login(
            &LoginCancellation::default(),
            &LoginConfig::default(),
            &headless("unused"),
        )
        .await;
    assert_eq!(
        result.unwrap_err().kind,
        AuthenticatorErrorKind::LoginFailed
    );
    assert_eq!(presenter.prompt_drops.load(Ordering::SeqCst), 1);
    assert!(client.requests.lock().unwrap().is_empty());
}
#[tokio::test]
async fn candidate_devin_sdk_browser_callback_rejects_unrelated_state_and_closes_listener() {
    let presenter = Arc::new(Presenter::default());
    let (client, auth) = setup(true, presenter.clone());
    let cancellation = LoginCancellation::default();
    let config = LoginConfig::default();
    let options = LoginOptions::default();
    let (result, port) = tokio::join!(auth.login(&cancellation, &config, &options), async {
        presenter.presented.notified().await;
        let port = presenter.presentations.lock().unwrap()[0].1.unwrap();
        assert!(request(port, "/callback?code=wrong&state=unrelated")
            .await
            .starts_with("HTTP/1.1 400"));
        assert!(
            request(port, "/callback?code=manual-code&state=expected-state")
                .await
                .starts_with("HTTP/1.1 200")
        );
        port
    });
    assert_eq!(result.unwrap().unwrap().provider, "devin");
    listener_closed(port).await;
    assert_eq!(client.requests.lock().unwrap().len(), 3);
    assert_eq!(presenter.prompts.load(Ordering::SeqCst), 0);
}
#[tokio::test(start_paused = true)]
async fn candidate_devin_sdk_browser_manual_token_wins_and_cancels_listener() {
    let presenter = Arc::new(Presenter::default());
    let (client, auth) = setup(false, presenter.clone());
    let options = LoginOptions {
        prompt: Some(Arc::new(|_| Ok("eyJsynthetic".into()))),
        ..Default::default()
    };
    let record = auth
        .login(
            &LoginCancellation::default(),
            &LoginConfig::default(),
            &options,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.provider, "devin");
    assert_eq!(presenter.prompts.load(Ordering::SeqCst), 1);
    let port = presenter.presentations.lock().unwrap()[0].1.unwrap();
    listener_closed(port).await;
    assert_eq!(client.requests.lock().unwrap().len(), 2);
}
#[tokio::test]
async fn candidate_devin_sdk_browser_cancellation_closes_listener_without_requests() {
    let presenter = Arc::new(Presenter::default());
    let (client, auth) = setup(false, presenter.clone());
    let cancellation = LoginCancellation::default();
    let config = LoginConfig::default();
    let options = LoginOptions::default();
    let (result, port) = tokio::join!(auth.login(&cancellation, &config, &options), async {
        presenter.presented.notified().await;
        let port = presenter.presentations.lock().unwrap()[0].1.unwrap();
        cancellation.cancel();
        port
    });
    assert_eq!(result.unwrap_err().kind, AuthenticatorErrorKind::Cancelled);
    listener_closed(port).await;
    assert!(client.requests.lock().unwrap().is_empty());
}
#[tokio::test]
async fn candidate_devin_sdk_browser_provider_error_is_terminal_without_exchange() {
    let presenter = Arc::new(Presenter::default());
    let (client, auth) = setup(false, presenter.clone());
    let cancellation = LoginCancellation::default();
    let config = LoginConfig::default();
    let options = LoginOptions::default();
    let (result, port) = tokio::join!(auth.login(&cancellation, &config, &options), async {
        presenter.presented.notified().await;
        let port = presenter.presentations.lock().unwrap()[0].1.unwrap();
        assert!(request(port, "/callback?error=denied&state=expected-state")
            .await
            .starts_with("HTTP/1.1 400"));
        port
    });
    assert_eq!(
        result.unwrap_err().kind,
        AuthenticatorErrorKind::LoginFailed
    );
    listener_closed(port).await;
    assert!(client.requests.lock().unwrap().is_empty());
}
#[tokio::test]
async fn candidate_devin_sdk_single_use_exchange_failure_never_retries_or_saves() {
    let presenter = Arc::new(Presenter::default());
    let (client, auth) = setup(true, presenter);
    *client.replies.lock().unwrap() = VecDeque::from([(401, b"synthetic rejected code".to_vec())]);
    let store = Arc::new(Store::default());
    let manager = Manager::new(
        Some(store.clone()),
        [Arc::new(auth) as Arc<dyn Authenticator>],
    );
    assert!(manager
        .login(
            &LoginCancellation::default(),
            "devin",
            &LoginConfig::default(),
            &headless("manual-code")
        )
        .await
        .is_err());
    assert_eq!(client.requests.lock().unwrap().len(), 1);
    assert!(store.0.lock().unwrap().is_empty());
}
#[tokio::test]
async fn candidate_devin_sdk_precancelled_login_has_no_ui_or_http_effects() {
    let presenter = Arc::new(Presenter::default());
    let (client, auth) = setup(false, presenter.clone());
    let cancellation = LoginCancellation::default();
    cancellation.cancel();
    assert_eq!(
        auth.login(
            &cancellation,
            &LoginConfig::default(),
            &headless("eyJsynthetic")
        )
        .await
        .unwrap_err()
        .kind,
        AuthenticatorErrorKind::Cancelled
    );
    assert!(presenter.presentations.lock().unwrap().is_empty());
    assert!(client.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn candidate_devin_sdk_optional_prompt_failure_or_empty_input_keeps_browser_active() {
    for reply in [Ok(""), Ok("invalid callback input"), Err(PromptError)] {
        let presenter = Arc::new(Presenter::default());
        let (client, auth) = setup(true, presenter.clone());
        let options = LoginOptions {
            prompt: Some(Arc::new(move |_| reply.map(str::to_owned))),
            ..Default::default()
        };
        let config = LoginConfig::default();
        let cancellation = LoginCancellation::default();
        let (result, port) = tokio::join!(auth.login(&cancellation, &config, &options), async {
            presenter.prompted.notified().await;
            let port = presenter.presentations.lock().unwrap()[0].1.unwrap();
            assert!(
                request(port, "/callback?code=manual-code&state=expected-state")
                    .await
                    .starts_with("HTTP/1.1 200")
            );
            port
        });
        assert_eq!(result.unwrap().unwrap().provider, "devin");
        listener_closed(port).await;
        assert_eq!(client.requests.lock().unwrap().len(), 3);
        assert_eq!(presenter.prompts.load(Ordering::SeqCst), 1);
        assert_eq!(presenter.prompt_drops.load(Ordering::SeqCst), 1);
    }
}
