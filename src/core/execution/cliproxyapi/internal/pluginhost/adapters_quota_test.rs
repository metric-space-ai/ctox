// ref: internal/pluginhost/quota_provider_test.go @ 5fc580b1d72d87a7bcfe5cfb6f479c1aea5625b3
// License: MIT (upstream); modifications AGPL-3.0-only
use super::{
    abi::{
        PluginArtifact, PluginCall, PluginClient, PluginClientError, PluginFuture, PluginLoader,
        PluginStream,
    },
    adapters::RpcCapabilityClient,
    adapters_quota::RpcQuotaProvider,
    auth_provider::HostConfigSummarySource,
    callback_contexts::{CallbackAuthority, CallbackContextRegistry},
    host_callbacks::{
        install_standard_host_callbacks, HostCallbackRouteError, HostCallbackRouter,
        HostLogAuthority,
    },
    http_bridge::HostHttpBridge,
    http_stream_bridge::{HttpStreamBridge, HttpStreamBridgeError},
    rpc_client::RpcPluginClient,
    stream_bridge::StreamBridge,
};
use crate::sdk::{
    pluginabi::*,
    pluginapi::{
        self, HostConfigSummary, HostHttpClient, HttpRequest, HttpResponse, HttpStreamResponse,
        QuotaDescribeRequest, QuotaFetchRequest, QuotaProvider, QuotaResetRequest,
    },
};
use serde_json::{json, value::to_raw_value, Value};

#[tokio::test(start_paused = true)]
async fn quota_rpc_timeout_revokes_callback_and_returns_a_real_deadline_error() {
    let (provider, client, _, contexts) = provider(true);
    let http = Arc::new(Http::new("account-a"));
    let task = tokio::spawn(async move {
        provider
            .fetch_quota(QuotaFetchRequest {
                http_client: Some(http),
                ..Default::default()
            })
            .await
    });
    client.started.notified().await;
    assert_eq!(contexts.len(), 1);
    tokio::time::advance(std::time::Duration::from_secs(31)).await;
    let error = task.await.unwrap().unwrap_err();
    assert_eq!(
        error.downcast_ref::<PluginClientError>(),
        Some(&PluginClientError::DeadlineExceeded)
    );
    assert!(contexts.is_empty());
}

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};
use tokio::sync::{mpsc, Notify};

struct ForbiddenHostMutation(Arc<AtomicUsize>);
impl super::host_callbacks::HostCallbackHandler for ForbiddenHostMutation {
    fn call<'a>(
        &'a self,
        _: &'a CallbackAuthority,
        _: &'a serde_json::value::RawValue,
    ) -> super::host_callbacks::HostCallbackFuture<'a> {
        Box::pin(async move {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(Envelope::success(Some(to_raw_value(&json!({})).unwrap())))
        })
    }
}
#[tokio::test]
async fn quota_callback_capability_cannot_invoke_registered_auth_store_mutations() {
    let contexts = CallbackContextRegistry::new();
    let count = Arc::new(AtomicUsize::new(0));
    let mut router = HostCallbackRouter::new(contexts.clone());
    router
        .register(
            "host.auth.save",
            Arc::new(ForbiddenHostMutation(count.clone())),
        )
        .unwrap();
    let lease = contexts.open(CallbackAuthority::new("quota-plugin", None).bind_http(None));
    let response = router
        .dispatch(
            "quota-plugin",
            lease.id(),
            "host.auth.save",
            &to_raw_value(&json!({})).unwrap(),
            0,
        )
        .await;
    assert_eq!(
        response.err().unwrap(),
        HostCallbackRouteError::UnknownMethod
    );
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

struct Host;
impl HostConfigSummarySource for Host {
    fn snapshot(&self) -> HostConfigSummary {
        HostConfigSummary {
            auth_dir: "/server-summary".into(),
            ..Default::default()
        }
    }
}
struct Log;
impl HostLogAuthority for Log {
    fn log<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: BTreeMap<String, Value>,
    ) -> pluginapi::PluginFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}
struct Http {
    account: &'static str,
    calls: AtomicUsize,
    stream_sender: Mutex<Option<mpsc::Sender<pluginapi::HttpStreamChunk>>>,
}
impl Http {
    fn new(account: &'static str) -> Self {
        Self {
            account,
            calls: AtomicUsize::new(0),
            stream_sender: Mutex::new(None),
        }
    }
}
impl HostHttpClient for Http {
    fn execute<'a>(&'a self, _: HttpRequest) -> pluginapi::PluginFuture<'a, HttpResponse> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(HttpResponse {
                status_code: 200,
                body: self.account.as_bytes().to_vec(),
                ..Default::default()
            })
        })
    }
    fn execute_stream<'a>(
        &'a self,
        _: HttpRequest,
    ) -> pluginapi::PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let (sender, chunks) = mpsc::channel(1);
            *self.stream_sender.lock().unwrap() = Some(sender);
            Ok(HttpStreamResponse {
                status_code: 200,
                headers: Default::default(),
                chunks,
            })
        })
    }
}
fn router(
    contexts: CallbackContextRegistry,
    fallback: Arc<dyn HostHttpClient>,
) -> (Arc<HostCallbackRouter>, Arc<HttpStreamBridge>) {
    let streams = Arc::new(HttpStreamBridge::default());
    let mut router = HostCallbackRouter::new(contexts);
    install_standard_host_callbacks(
        &mut router,
        HostHttpBridge::new(fallback),
        streams.clone(),
        StreamBridge::new(),
        Arc::new(Log),
    )
    .unwrap();
    (Arc::new(router), streams)
}
struct Client {
    router: Arc<HostCallbackRouter>,
    calls: Mutex<Vec<Value>>,
    started: Notify,
    hold: bool,
}
impl PluginClient for Client {
    fn call<'a>(&'a self, call: PluginCall) -> PluginFuture<'a, Envelope> {
        Box::pin(async move {
            let payload: Value = serde_json::from_str(call.payload.get()).unwrap();
            self.calls
                .lock()
                .unwrap()
                .push(json!({"method":call.method,"payload":payload}));
            let response = match call.method.as_str() {
                METHOD_PLUGIN_REGISTER | METHOD_PLUGIN_RECONFIGURE => {
                    json!({"schema_version":2,"metadata":{"Name":"Quota fixture"},"capabilities":{"quota_provider":true}})
                }
                METHOD_QUOTA_IDENTIFIER => json!({"identifier":"quota-fixture"}),
                METHOD_QUOTA_DESCRIBE => {
                    assert!(payload.get("host_callback_id").is_none());
                    json!({"supported_providers":["quota-fixture"],"supports_reset":true})
                }
                METHOD_QUOTA_FETCH | METHOD_QUOTA_RESET => {
                    let id = payload["host_callback_id"].as_str().unwrap();
                    let authority = self.router.contexts().resolve(id).unwrap();
                    assert!(authority.http_is_scoped());
                    assert_eq!(payload["host"]["AuthDir"], "/server-summary");
                    assert!(payload.get("http_client").is_none());
                    self.started.notify_one();
                    if self.hold {
                        std::future::pending::<()>().await;
                    }
                    let request = to_raw_value(
                        &json!({"method":"GET","url":"https://quota.invalid/limits","body":""}),
                    )
                    .unwrap();
                    let result = self
                        .router
                        .dispatch("quota-plugin", id, METHOD_HOST_HTTP_DO, &request, 0)
                        .await
                        .map_err(|_| PluginClientError::UnsupportedCapability)?;
                    let body: Value = serde_json::from_str(result.result.unwrap().get()).unwrap();
                    if call.method == METHOD_QUOTA_FETCH {
                        json!({"subscription":{"plan":body["body"]}})
                    } else {
                        json!({"success":false,"message":"provider declined"})
                    }
                }
                _ => return Err(PluginClientError::UnsupportedCapability),
            };
            Ok(Envelope::success(Some(to_raw_value(&response).unwrap())))
        })
    }
    fn call_stream<'a>(&'a self, _: PluginCall) -> PluginFuture<'a, PluginStream> {
        Box::pin(async { Err(PluginClientError::UnsupportedCapability) })
    }
    fn shutdown<'a>(&'a self) -> PluginFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}
fn provider(
    hold: bool,
) -> (
    Arc<RpcQuotaProvider>,
    Arc<Client>,
    Arc<Http>,
    CallbackContextRegistry,
) {
    let contexts = CallbackContextRegistry::new();
    let fallback = Arc::new(Http::new("fallback"));
    let (router, _) = router(contexts.clone(), fallback.clone());
    let client = Arc::new(Client {
        router,
        calls: Mutex::new(vec![]),
        started: Notify::new(),
        hold,
    });
    let provider = RpcQuotaProvider::new(
        " Quota-Fixture ",
        RpcCapabilityClient::new(
            "quota-plugin",
            RpcPluginClient::new(client.clone()),
            contexts.clone(),
        )
        .unwrap(),
        Arc::new(Host),
    )
    .unwrap();
    (Arc::new(provider), client, fallback, contexts)
}
#[tokio::test]
async fn quota_rpc_keeps_accounts_separate_and_retains_only_their_http_authority() {
    let (provider, client, fallback, contexts) = provider(false);
    assert_eq!(provider.identifier(), "quota-fixture");
    let desc = provider
        .describe_quota(QuotaDescribeRequest::default())
        .await
        .unwrap();
    assert!(desc.supports_reset);
    assert!(contexts.is_empty());
    let first = Arc::new(Http::new("account-a"));
    let second = Arc::new(Http::new("account-b"));
    let (a, b) = tokio::join!(
        provider.fetch_quota(QuotaFetchRequest {
            auth_index: "a".into(),
            http_client: Some(first.clone()),
            ..Default::default()
        }),
        provider.fetch_quota(QuotaFetchRequest {
            auth_index: "b".into(),
            http_client: Some(second.clone()),
            ..Default::default()
        })
    );
    use base64::Engine;
    assert_eq!(
        a.unwrap().subscription.unwrap().plan,
        base64::engine::general_purpose::STANDARD.encode("account-a")
    );
    assert_eq!(
        b.unwrap().subscription.unwrap().plan,
        base64::engine::general_purpose::STANDARD.encode("account-b")
    );
    assert_eq!(first.calls.load(Ordering::SeqCst), 1);
    assert_eq!(second.calls.load(Ordering::SeqCst), 1);
    assert_eq!(fallback.calls.load(Ordering::SeqCst), 0);
    assert!(contexts.is_empty());
    assert_eq!(Arc::strong_count(&first), 1);
    assert_eq!(Arc::strong_count(&second), 1);
    let calls = client.calls.lock().unwrap();
    let ids = calls
        .iter()
        .filter_map(|c| c["payload"]["host_callback_id"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
}
#[tokio::test]
async fn quota_rpc_without_account_http_never_uses_the_broader_host_fallback() {
    let (provider, _, fallback, contexts) = provider(false);
    assert!(provider
        .fetch_quota(QuotaFetchRequest::default())
        .await
        .is_err());
    assert_eq!(fallback.calls.load(Ordering::SeqCst), 0);
    assert!(contexts.is_empty());
}
#[tokio::test]
async fn quota_rpc_reset_keeps_the_provider_decline_and_does_not_clear_routing_quota() {
    let (provider, _, fallback, contexts) = provider(false);
    let http = Arc::new(Http::new("account-a"));
    let result = provider
        .reset_quota(QuotaResetRequest {
            http_client: Some(http.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(!result.success);
    assert_eq!(result.message, "provider declined");
    assert_eq!(http.calls.load(Ordering::SeqCst), 1);
    assert_eq!(fallback.calls.load(Ordering::SeqCst), 0);
    assert!(contexts.is_empty());
}
#[tokio::test]
async fn dropping_a_quota_rpc_revokes_its_callback_identity_and_http_handle() {
    let (provider, client, _, contexts) = provider(true);
    let http = Arc::new(Http::new("account-a"));
    let request = QuotaFetchRequest {
        http_client: Some(http.clone()),
        ..Default::default()
    };
    let task = tokio::spawn(async move { provider.fetch_quota(request).await });
    client.started.notified().await;
    assert_eq!(contexts.len(), 1);
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(contexts.is_empty());
    assert_eq!(Arc::strong_count(&http), 1);
}
struct PendingHttp {
    started: Notify,
    dropped: Arc<AtomicUsize>,
}
struct Dropped(Arc<AtomicUsize>);
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl HostHttpClient for PendingHttp {
    fn execute<'a>(&'a self, _: HttpRequest) -> pluginapi::PluginFuture<'a, HttpResponse> {
        Box::pin(async move {
            let _guard = Dropped(self.dropped.clone());
            self.started.notify_one();
            std::future::pending().await
        })
    }
    fn execute_stream<'a>(
        &'a self,
        _: HttpRequest,
    ) -> pluginapi::PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async { panic!("unused stream") })
    }
}
#[tokio::test]
async fn revoked_quota_context_cancels_an_already_running_http_callback() {
    let contexts = CallbackContextRegistry::new();
    let fallback = Arc::new(Http::new("fallback"));
    let (http_router, _) = router(contexts.clone(), fallback.clone());
    let dropped = Arc::new(AtomicUsize::new(0));
    let http = Arc::new(PendingHttp {
        started: Notify::new(),
        dropped: dropped.clone(),
    });
    let mut lease =
        contexts.open(CallbackAuthority::new("quota-plugin", None).bind_http(Some(http.clone())));
    let id = lease.id().to_owned();
    let task = tokio::spawn(async move {
        let request =
            to_raw_value(&json!({"method":"GET","url":"https://quota.invalid/limits","body":""}))
                .unwrap();
        http_router
            .dispatch("quota-plugin", &id, METHOD_HOST_HTTP_DO, &request, 0)
            .await
    });
    http.started.notified().await;
    lease.close();
    assert_eq!(
        task.await.unwrap().err().unwrap(),
        HostCallbackRouteError::Handler(PluginClientError::Cancelled)
    );
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    assert_eq!(fallback.calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn quota_http_streams_close_with_their_call_and_reject_other_plugins() {
    let contexts = CallbackContextRegistry::new();
    let fallback = Arc::new(Http::new("fallback"));
    let (http_router, streams) = router(contexts.clone(), fallback.clone());
    let http = Arc::new(Http::new("account-a"));
    let mut lease =
        contexts.open(CallbackAuthority::new("quota-plugin", None).bind_http(Some(http.clone())));
    let request =
        to_raw_value(&json!({"method":"GET","url":"https://quota.invalid/limits","body":""}))
            .unwrap();
    assert_eq!(
        http_router
            .dispatch(
                "foreign-plugin",
                lease.id(),
                METHOD_HOST_HTTP_DO_STREAM,
                &request,
                0
            )
            .await
            .err()
            .unwrap(),
        HostCallbackRouteError::WrongPlugin
    );
    let response = http_router
        .dispatch(
            "quota-plugin",
            lease.id(),
            METHOD_HOST_HTTP_DO_STREAM,
            &request,
            0,
        )
        .await
        .unwrap();
    let response: Value = serde_json::from_str(response.result.unwrap().get()).unwrap();
    let stream_id = response["stream_id"].as_str().unwrap();
    let other = contexts.open(CallbackAuthority::new("quota-plugin", None).bind_http(None));
    let read = to_raw_value(&json!({"stream_id":stream_id})).unwrap();
    assert!(http_router
        .dispatch(
            "quota-plugin",
            other.id(),
            METHOD_HOST_HTTP_STREAM_READ,
            &read,
            0
        )
        .await
        .is_err());
    drop(other);
    lease.close();
    assert_eq!(
        streams.read("quota-plugin", stream_id).await,
        Err(HttpStreamBridgeError::NotOpen)
    );
    assert!(http
        .stream_sender
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .is_closed());
    assert_eq!(fallback.calls.load(Ordering::SeqCst), 0);
}
struct Loader(Arc<Client>);
impl PluginLoader for Loader {
    fn open<'a>(&'a self, _: &'a PluginArtifact) -> PluginFuture<'a, Arc<dyn PluginClient>> {
        Box::pin(async move { Ok(self.0.clone() as Arc<dyn PluginClient>) })
    }
}
#[tokio::test]
async fn quota_only_process_plugins_register_and_produce_an_actual_sdk_adapter() {
    let (_, client, _, _) = provider(false);
    let host = super::host::PluginHost::new(Arc::new(Loader(client)));
    let mut config = super::config::RuntimeConfig {
        enabled: true,
        ..Default::default()
    };
    config.items.insert(
        "quota-plugin".into(),
        super::config::RuntimeItemConfig {
            enabled: true,
            ..Default::default()
        },
    );
    let report = host
        .apply_config(
            &config,
            &[super::platform::PluginFileInfo {
                id: "quota-plugin".into(),
                path: PathBuf::from("/fixture/quota-plugin"),
                version: None,
            }],
        )
        .await;
    assert!(report.failures.is_empty());
    let snapshot = host.snapshot();
    let listed = snapshot.registered_plugins();
    assert!(listed[0]
        .capabilities
        .contains(&"quota_provider".to_owned()));
    let record = snapshot.record("quota-plugin").unwrap();
    assert_eq!(record.identifiers["quota_provider"], "quota-fixture");
    let plugin = super::adapters::plugin_from_record(
        record,
        host.callback_contexts().clone(),
        host.streams().clone(),
        Arc::new(Host),
    )
    .unwrap();
    assert!(plugin.capabilities.quota_provider.is_some());
    host.shutdown().await;
}
