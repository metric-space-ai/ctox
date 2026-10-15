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
