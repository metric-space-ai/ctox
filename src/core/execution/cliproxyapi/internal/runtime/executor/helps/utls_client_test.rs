// ref: internal/runtime/executor/helps/utls_client_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::utls_client::*;
use crate::sdk::pluginapi::{
    HostHttpClient, HttpRequest, HttpResponse, HttpStreamResponse, PluginFuture,
};

#[derive(Default)]
struct Client {
    urls: Mutex<Vec<String>>,
}

impl HostHttpClient for Client {
    fn execute<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        self.urls.lock().unwrap().push(request.url);
        Box::pin(async {
            Ok(HttpResponse {
                status_code: 200,
                ..HttpResponse::default()
            })
        })
    }

    fn execute_stream<'a>(&'a self, _request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async { panic!("stream not used") })
    }
}

struct Factory {
    anthropic: Arc<Client>,
    chrome: Arc<Client>,
    standard: Arc<Client>,
    builds: Mutex<Vec<(Option<String>, UtlsTransportProfile)>>,
}

impl UtlsTransportFactory for Factory {
    fn build(
        &self,
        proxy_url: Option<&str>,
        profile: UtlsTransportProfile,
    ) -> Result<Arc<dyn HostHttpClient>, UtlsClientError> {
        self.builds
            .lock()
            .unwrap()
            .push((proxy_url.map(str::to_owned), profile));
        Ok(match profile {
            UtlsTransportProfile::ClaudeCodeNodeOpenSsl => self.anthropic.clone(),
            UtlsTransportProfile::Chrome => self.chrome.clone(),
            UtlsTransportProfile::Standard => self.standard.clone(),
        })
    }
}

#[tokio::test]
async fn context_client_handles_protected_host_without_proxy() {
    let context = Arc::new(Client::default());
    let factory = Factory {
        anthropic: Arc::new(Client::default()),
        chrome: Arc::new(Client::default()),
        standard: Arc::new(Client::default()),
        builds: Mutex::new(Vec::new()),
    };
    let client =
        new_utls_http_client(Some(context.clone()), &factory, None, Duration::ZERO).unwrap();
    client
        .execute(HttpRequest {
            method: "GET".into(),
            url: "https://chatgpt.com/backend-api/codex/responses".into(),
            ..HttpRequest::default()
        })
        .await
        .unwrap();
    assert_eq!(
        *context.urls.lock().unwrap(),
        ["https://chatgpt.com/backend-api/codex/responses"]
    );
    assert!(factory.builds.lock().unwrap().is_empty());
}

#[tokio::test]
async fn anthropic_uses_node_profile_chatgpt_uses_chrome_and_others_standard() {
    let anthropic = Arc::new(Client::default());
    let chrome = Arc::new(Client::default());
    let standard = Arc::new(Client::default());
    let factory = Factory {
        anthropic: anthropic.clone(),
        chrome: chrome.clone(),
        standard: standard.clone(),
        builds: Mutex::new(Vec::new()),
    };
    let client = new_utls_http_client(None, &factory, None, Duration::from_secs(5)).unwrap();
    for url in [
        "https://api.anthropic.com/v1/messages",
        "https://chatgpt.com/backend-api/codex/responses",
        "http://api.anthropic.com/insecure",
        "https://api.anthropic.com.evil.example/v1/messages",
        "https://example.com/v1",
    ] {
        client
            .execute(HttpRequest {
                url: url.into(),
                ..HttpRequest::default()
            })
            .await
            .unwrap();
    }
    assert_eq!(anthropic.urls.lock().unwrap().len(), 1);
    assert_eq!(chrome.urls.lock().unwrap().len(), 1);
    assert_eq!(standard.urls.lock().unwrap().len(), 3);
}

#[test]
fn explicit_proxy_bypasses_context_client_and_is_not_exposed_by_error() {
    let context = Arc::new(Client::default());
    let factory = Factory {
        anthropic: Arc::new(Client::default()),
        chrome: Arc::new(Client::default()),
        standard: Arc::new(Client::default()),
        builds: Mutex::new(Vec::new()),
    };
    let proxy = "http://operator:do-not-log@proxy.example";
    new_utls_http_client(Some(context), &factory, Some(proxy), Duration::ZERO).unwrap();
    assert_eq!(
        factory.builds.lock().unwrap().as_slice(),
        [
            (
                Some(proxy.into()),
                UtlsTransportProfile::ClaudeCodeNodeOpenSsl
            ),
            (Some(proxy.into()), UtlsTransportProfile::Chrome),
            (Some(proxy.into()), UtlsTransportProfile::Standard),
        ]
    );
    let rendered = UtlsClientError::Build.to_string();
    assert!(!rendered.contains("operator"));
    assert!(!rendered.contains("do-not-log"));
}
