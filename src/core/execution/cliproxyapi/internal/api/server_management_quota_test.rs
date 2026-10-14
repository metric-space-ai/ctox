// ref: internal/api/handlers/management/plugin_quota_test.go @ 5fc580b1d72d87a7bcfe5cfb6f479c1aea5625b3
// License: MIT (upstream); modifications AGPL-3.0-only
use super::*;
use crate::internal::{
    api::handlers::management::*,
    pluginhost::{
        abi::{
            PluginArtifact, PluginCall, PluginClient, PluginClientError, PluginFuture,
            PluginLoader, PluginStream,
        },
        auth_provider::HostConfigSummarySource,
        config::{RuntimeConfig, RuntimeItemConfig},
        host::PluginHost,
        platform::PluginFileInfo,
    },
};
use crate::sdk::{
    pluginabi::*,
    pluginapi::{HostConfigSummary, QuotaFetchRequest},
};
use serde_json::{json, value::to_raw_value, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    },
};
struct Summary;
impl HostConfigSummarySource for Summary {
    fn snapshot(&self) -> HostConfigSummary {
        Default::default()
    }
}
struct Credentials {
    calls: AtomicUsize,
    events: Arc<Mutex<Vec<&'static str>>>,
    mismatch: AtomicBool,
    unavailable: AtomicBool,
    probe: AtomicBool,
}
impl ManagementQuotaCredentialSource for Credentials {
    fn resolve(
        &self,
        index: &str,
    ) -> Result<Option<ManagementQuotaCredential>, ManagementQuotaCredentialError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.events.lock().unwrap().push("resolve");
        if self.unavailable.load(Ordering::SeqCst) {
            return Err(ManagementQuotaCredentialError::Unavailable);
        }
        if index != "account-index" {
            return Ok(None);
        }
        Ok(Some(ManagementQuotaCredential {
            request: QuotaFetchRequest {
                auth_index: if self.mismatch.load(Ordering::SeqCst) {
                    "other".into()
                } else {
                    index.into()
                },
                provider: if self.probe.load(Ordering::SeqCst) {
                    "no-plugin".into()
                } else {
                    "quota-fixture".into()
                },
                metadata: BTreeMap::from([(
                    "test_credential".into(),
                    json!("fixture-secret-value"),
                )]),
                ..Default::default()
            },
            probe: self.probe.load(Ordering::SeqCst).then(|| {
                json!({"url":"https://quota.invalid/probe"})
                    .as_object()
                    .unwrap()
                    .clone()
            }),
        }))
    }
}
struct Client {
    events: Arc<Mutex<Vec<&'static str>>>,
    resets: AtomicUsize,
    reset_ok: AtomicBool,
    reset_error: AtomicBool,
    reset_timeout: AtomicBool,
}
impl PluginClient for Client {
    fn call<'a>(&'a self, call: PluginCall) -> PluginFuture<'a, Envelope> {
        Box::pin(async move {
            let result = match call.method.as_str() {
                METHOD_PLUGIN_REGISTER | METHOD_PLUGIN_RECONFIGURE => {
                    json!({"schema_version":2,"metadata":{"Name":"Quota fixture"},"capabilities":{"quota_provider":true}})
                }
                METHOD_QUOTA_IDENTIFIER => json!({"identifier":"quota-fixture"}),
                METHOD_QUOTA_DESCRIBE => {
                    json!({"supported_providers":["quota-fixture"],"supports_reset":true})
                }
                METHOD_QUOTA_FETCH => {
                    let request: Value = serde_json::from_str(call.payload.get()).unwrap();
                    assert_eq!(request["auth_index"], "account-index");
                    json!({"groups":[{"buckets":[{"remainingFraction":0}]}]})
                }
                METHOD_QUOTA_RESET => {
                    self.resets.fetch_add(1, Ordering::SeqCst);
                    self.events.lock().unwrap().push("provider-reset");
                    if self.reset_timeout.load(Ordering::SeqCst) {
                        std::future::pending::<()>().await;
                    }
                    if self.reset_error.load(Ordering::SeqCst) {
                        return Err(PluginClientError::Transport("fixture-secret-value".into()));
                    }
                    json!({"success":self.reset_ok.load(Ordering::SeqCst),"message":"fixture-secret-value"})
                }
                _ => return Err(PluginClientError::UnsupportedCapability),
            };
            Ok(Envelope::success(Some(to_raw_value(&result).unwrap())))
        })
    }
    fn call_stream<'a>(&'a self, _: PluginCall) -> PluginFuture<'a, PluginStream> {
        Box::pin(async { Err(PluginClientError::UnsupportedCapability) })
    }
    fn shutdown<'a>(&'a self) -> PluginFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}
struct Loader(Arc<Client>);
impl PluginLoader for Loader {
    fn open<'a>(&'a self, _: &'a PluginArtifact) -> PluginFuture<'a, Arc<dyn PluginClient>> {
        Box::pin(async move { Ok(self.0.clone() as Arc<dyn PluginClient>) })
    }
}
struct Routing {
    events: Arc<Mutex<Vec<&'static str>>>,
    calls: AtomicUsize,
    fail: AtomicBool,
}
impl ManagementQuotaResetSource for Routing {
    fn reset_by_index(
        &self,
        index: &str,
    ) -> Result<Option<ManagementQuotaResetResult>, ManagementQuotaResetError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.events.lock().unwrap().push("routing-reset");
        if self.fail.load(Ordering::SeqCst) {
            return Err(ManagementQuotaResetError::StoreUnavailable);
        }
        Ok(Some(ManagementQuotaResetResult {
            auth_index: index.into(),
            models: vec![],
        }))
    }
}
struct Probe {
    status: AtomicUsize,
    calls: AtomicUsize,
}
impl ManagementApiCallExecutor for Probe {
    fn execute(
        &self,
        request: &ManagementApiToolRequest,
    ) -> Result<ManagementApiToolResponse, ManagementApiToolError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(request.auth_index, "account-index");
        Ok(ManagementApiToolResponse {
            status_code: self.status.load(Ordering::SeqCst) as u16,
            headers: Default::default(),
            body: if self.status.load(Ordering::SeqCst) == 200 {
                r#"{"summary":[{"key":"balance","label":"Balance","value":0}]}"#.into()
            } else {
                "fixture-secret-value".into()
            },
        })
    }
}
struct Fixture {
    handler: Arc<ManagementHandler>,
    credentials: Arc<Credentials>,
    client: Arc<Client>,
    routing: Arc<Routing>,
    events: Arc<Mutex<Vec<&'static str>>>,
    probe: Arc<Probe>,
}
async fn fixture() -> Fixture {
    let events = Arc::new(Mutex::new(vec![]));
    let credentials = Arc::new(Credentials {
        calls: AtomicUsize::new(0),
        events: events.clone(),
        mismatch: AtomicBool::new(false),
        unavailable: AtomicBool::new(false),
        probe: AtomicBool::new(false),
    });
    let client = Arc::new(Client {
        events: events.clone(),
        resets: AtomicUsize::new(0),
        reset_ok: AtomicBool::new(true),
        reset_error: AtomicBool::new(false),
        reset_timeout: AtomicBool::new(false),
    });
    let host = Arc::new(PluginHost::new(Arc::new(Loader(client.clone()))));
    let config = RuntimeConfig {
        enabled: true,
        items: BTreeMap::from([(
            "quota-plugin".into(),
            RuntimeItemConfig {
                enabled: true,
                ..Default::default()
            },
        )]),
        ..Default::default()
    };
    assert!(host
        .apply_config(
            &config,
            &[PluginFileInfo {
                id: "quota-plugin".into(),
                path: PathBuf::from("/fixture/quota-plugin"),
                version: None
            }]
        )
        .await
        .failures
        .is_empty());
    let routing = Arc::new(Routing {
        events: events.clone(),
        calls: AtomicUsize::new(0),
        fail: AtomicBool::new(false),
    });
    let probe = Arc::new(Probe {
        status: AtomicUsize::new(200),
        calls: AtomicUsize::new(0),
    });
    let service = ManagementPluginQuotaService::new(host, credentials.clone(), Arc::new(Summary))
        .with_routing_reset(routing.clone())
        .with_probe(Arc::new(ManagementQuotaProbe::new(Arc::new(
            ManagementApiTools::new(probe.clone()),
        ))));
    let handler = Arc::new(
        ManagementHandler::new(Arc::new(
            ManagementAuthenticator::new(
                "fixture-management-key",
                false,
                Arc::new(SystemManagementAuthClock),
            )
            .unwrap(),
        ))
        .attach_plugin_quota_service(Arc::new(service)),
    );
    Fixture {
        handler,
        credentials,
        client,
        routing,
        events,
        probe,
    }
}
fn headers() -> BTreeMap<String, Vec<String>> {
    BTreeMap::from([
        (
            "Authorization".into(),
            vec!["Bearer fixture-management-key".into()],
        ),
        ("Content-Type".into(), vec!["application/json".into()]),
    ])
}

#[tokio::test(start_paused = true)]
async fn quota_management_timeout_returns_a_terminal_error_and_preserves_cooldown() {
    let f = fixture().await;
    f.client.reset_timeout.store(true, Ordering::SeqCst);
    let response = send(
        &f,
        "POST",
        "/v0/management/quota/reset",
        json!({"auth_index":"account-index"}),
    )
    .await;
    assert_eq!(response.status(), 504);
    assert_eq!(body(&response)["kind"], "quota_timeout");
    assert_eq!(f.routing.calls.load(Ordering::SeqCst), 0);
}

fn ip() -> IpAddr {
    "127.0.0.1".parse().unwrap()
}
async fn send(f: &Fixture, method: &str, target: &str, body: Value) -> ManagementHttpResponse {
    f.handler
        .handle_async(
            method,
            target,
            &headers(),
            &serde_json::to_vec(&body).unwrap(),
            ip(),
        )
        .await
}
fn body(response: &ManagementHttpResponse) -> Value {
    serde_json::from_slice(response.body()).unwrap()
}
#[tokio::test]
async fn quota_routes_authorize_before_credential_or_plugin_access() {
    let f = fixture().await;
    let response = f
        .handler
        .handle_async(
            "POST",
            "/v0/management/quota/reset",
            &BTreeMap::new(),
            br#"{"auth_index":"account-index"}"#,
            ip(),
        )
        .await;
    assert_eq!(response.status(), 401);
    assert_eq!(f.credentials.calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.client.resets.load(Ordering::SeqCst), 0);
    assert_eq!(f.routing.calls.load(Ordering::SeqCst), 0);
    let response = send(&f, "GET", "/v0/management/quota/providers", json!({})).await;
    assert_eq!(response.status(), 200);
    assert_eq!(body(&response)["providers"][0]["plugin_id"], "quota-plugin");
    assert_eq!(f.credentials.calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn quota_routes_reject_invalid_bodies_methods_and_account_injection() {
    let f = fixture().await;
    for value in [
        json!({}),
        json!({"auth_index":"account-index","storage_json":"forged"}),
        json!({"auth_index":"account-index","url":"https://foreign.invalid"}),
    ] {
        assert_eq!(
            send(&f, "POST", "/v0/management/quota/fetch", value)
                .await
                .status(),
            400
        );
    }
    assert_eq!(
        send(&f, "PUT", "/v0/management/quota/fetch", json!({}))
            .await
            .status(),
        405
    );
    let too_large = vec![b' '; 16 * 1024 + 1];
    assert_eq!(
        f.handler
            .handle_async(
                "POST",
                "/v0/management/quota/fetch",
                &headers(),
                &too_large,
                ip()
            )
            .await
            .status(),
        413
    );
    assert_eq!(f.credentials.calls.load(Ordering::SeqCst), 0);
    let response = send(
        &f,
        "POST",
        "/v0/management/quota/fetch",
        json!({"auth_index":"missing"}),
    )
    .await;
    assert_eq!(response.status(), 404);
    assert_eq!(body(&response)["kind"], "account_not_found");
}
#[tokio::test]
async fn quota_routes_preserve_index_alias_precedence_and_actual_zero() {
    let f = fixture().await;
    let response = send(
        &f,
        "POST",
        "/v0/management/quota/fetch",
        json!({"auth_index":" ","authIndex":"account-index","AuthIndex":"missing"}),
    )
    .await;
    assert_eq!(response.status(), 200);
    assert_eq!(
        body(&response)["groups"][0]["buckets"][0]["remainingFraction"].as_f64(),
        Some(0.0)
    );
    let response = send(
        &f,
        "GET",
        "/v0/management/plugins/quota-plugin/quota?authIndex=account-index",
        json!({}),
    )
    .await;
    assert_eq!(response.status(), 200);
    let response = send(
        &f,
        "POST",
        "/v0/management/plugins/missing/quota",
        json!({"auth_index":"account-index"}),
    )
    .await;
    assert_eq!(response.status(), 404);
    assert_eq!(body(&response)["kind"], "quota_plugin_not_found");
}
#[tokio::test]
async fn quota_reset_requires_provider_success_before_routing_mutation() {
    let f = fixture().await;
    let response = send(
        &f,
        "POST",
        "/v0/management/quota/reset",
        json!({"auth_index":"account-index"}),
    )
    .await;
    assert_eq!(response.status(), 200);
    assert_eq!(
        *f.events.lock().unwrap(),
        vec!["resolve", "provider-reset", "routing-reset"]
    );
    assert!(!String::from_utf8_lossy(response.body()).contains("fixture-secret-value"));
}
#[tokio::test]
async fn quota_reset_decline_and_plugin_error_leave_routing_cooldown_untouched() {
    for transport_error in [false, true] {
        let f = fixture().await;
        f.client.reset_ok.store(false, Ordering::SeqCst);
        f.client
            .reset_error
            .store(transport_error, Ordering::SeqCst);
        let response = send(
            &f,
            "POST",
            "/v0/management/quota/reset",
            json!({"auth_index":"account-index"}),
        )
        .await;
        assert_eq!(response.status(), 502);
        assert_eq!(f.routing.calls.load(Ordering::SeqCst), 0);
        assert_eq!(*f.events.lock().unwrap(), vec!["resolve", "provider-reset"]);
        assert!(!String::from_utf8_lossy(response.body()).contains("fixture-secret-value"));
    }
}
#[tokio::test]
async fn quota_reset_reports_routing_store_failure_after_provider_acknowledgement() {
    let f = fixture().await;
    f.routing.fail.store(true, Ordering::SeqCst);
    let response = send(
        &f,
        "POST",
        "/v0/management/quota/reset",
        json!({"auth_index":"account-index"}),
    )
    .await;
    assert_eq!(response.status(), 500);
    assert_eq!(body(&response)["kind"], "routing_reset_failed");
    assert_eq!(
        *f.events.lock().unwrap(),
        vec!["resolve", "provider-reset", "routing-reset"]
    );
    let f = fixture().await;
    assert_eq!(
        send(
            &f,
            "DELETE",
            "/v0/management/plugins/quota-plugin/quota?auth_index=account-index",
            json!({})
        )
        .await
        .status(),
        200
    );
}
#[tokio::test]
async fn quota_pre_request_failures_are_not_mislabeled_as_provider_auth_denials() {
    let f = fixture().await;
    f.credentials.unavailable.store(true, Ordering::SeqCst);
    let response = send(
        &f,
        "POST",
        "/v0/management/quota/fetch",
        json!({"auth_index":"account-index"}),
    )
    .await;
    assert_eq!(response.status(), 500);
    assert_eq!(body(&response)["kind"], "credential_source_unavailable");
    let f = fixture().await;
    f.credentials.mismatch.store(true, Ordering::SeqCst);
    let response = send(
        &f,
        "POST",
        "/v0/management/quota/reset",
        json!({"auth_index":"account-index"}),
    )
    .await;
    assert_eq!(response.status(), 500);
    assert_eq!(body(&response)["kind"], "account_context_mismatch");
    assert_eq!(f.client.resets.load(Ordering::SeqCst), 0);
    assert_eq!(f.probe.calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn quota_declarative_fallback_uses_only_server_configuration_and_retains_true_upstream_status(
) {
    let f = fixture().await;
    f.credentials.probe.store(true, Ordering::SeqCst);
    let response = send(
        &f,
        "POST",
        "/v0/management/quota/fetch",
        json!({"auth_index":"account-index"}),
    )
    .await;
    assert_eq!(response.status(), 200);
    assert_eq!(body(&response)["summary"][0]["value"].as_f64(), Some(0.0));
    f.probe.status.store(401, Ordering::SeqCst);
    let response = send(
        &f,
        "POST",
        "/v0/management/quota/fetch",
        json!({"auth_index":"account-index"}),
    )
    .await;
    assert_eq!(response.status(), 502);
    assert_eq!(body(&response)["kind"], "quota_upstream_status");
    assert_eq!(body(&response)["upstream_status"], 401);
    assert!(!String::from_utf8_lossy(response.body()).contains("fixture-secret-value"));
}
#[tokio::test]
async fn quota_routes_are_reachable_over_the_real_management_transport() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let f = fixture().await;
    let handler = f.handler.clone();
    let (mut client, mut server) = tokio::io::duplex(16 * 1024);
    let task = tokio::spawn(async move {
        serve_management_connection(&mut server, &handler, ip())
            .await
            .unwrap()
    });
    let payload = r#"{"auth_index":"account-index"}"#;
    let request=format!("POST /v0/management/quota/fetch HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer fixture-management-key\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",payload.len(),payload);
    client.write_all(request.as_bytes()).await.unwrap();
    let mut response = vec![];
    client.read_to_end(&mut response).await.unwrap();
    task.await.unwrap();
    let response = String::from_utf8(response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200"));
    let value: Value = serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(
        value["groups"][0]["buckets"][0]["remainingFraction"].as_f64(),
        Some(0.0)
    );
}
