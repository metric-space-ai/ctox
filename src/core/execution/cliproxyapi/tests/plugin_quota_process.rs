// Origin: CTOX
// License: AGPL-3.0-only
#![cfg(unix)]
use ctox_cliproxyapi::{
    internal::pluginhost::{
        auth_provider::HostConfigSummarySource,
        callback_contexts::CallbackContextRegistry,
        config::{RuntimeConfig, RuntimeItemConfig},
        host::PluginHost,
        host_callbacks::{install_standard_host_callbacks, HostCallbackRouter, HostLogAuthority},
        http_bridge::HostHttpBridge,
        http_stream_bridge::HttpStreamBridge,
        platform::PluginFileInfo,
        process_client::ProcessPluginLoader,
        stream_bridge::StreamBridge,
    },
    sdk::pluginapi::{
        self, HostConfigSummary, HostHttpClient, HttpRequest, HttpResponse, HttpStreamResponse,
        QuotaFetchRequest,
    },
};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

struct Summary;
impl HostConfigSummarySource for Summary {
    fn snapshot(&self) -> HostConfigSummary {
        Default::default()
    }
}
struct Log;
impl HostLogAuthority for Log {
    fn log<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: BTreeMap<String, serde_json::Value>,
    ) -> pluginapi::PluginFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}
#[derive(Default)]
struct Http(AtomicUsize);
impl HostHttpClient for Http {
    fn execute<'a>(&'a self, request: HttpRequest) -> pluginapi::PluginFuture<'a, HttpResponse> {
        Box::pin(async move {
            assert_eq!(request.url, "https://fixture.invalid/quota");
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(HttpResponse {
                status_code: 200,
                body: b"fixture-only".to_vec(),
                ..Default::default()
            })
        })
    }
    fn execute_stream<'a>(
        &'a self,
        _: HttpRequest,
    ) -> pluginapi::PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async {
            Err(
                Arc::new(std::io::Error::other("stream not part of unary fixture"))
                    as Arc<dyn std::error::Error + Send + Sync>,
            )
        })
    }
}

#[tokio::test]
async fn quota_host_uses_actual_process_and_only_account_bound_http_authority() {
    #[cfg(unix)]
    let base = PathBuf::from("/tmp");
    #[cfg(windows)]
    let base = std::env::temp_dir();
    let root = tempfile::Builder::new()
        .prefix("cpa-q-")
        .tempdir_in(base)
        .unwrap();
    let contexts = CallbackContextRegistry::new();
    let fallback = Arc::new(Http::default());
    let account = Arc::new(Http::default());
    let mut router = HostCallbackRouter::new(contexts.clone());
    install_standard_host_callbacks(
        &mut router,
        HostHttpBridge::new(fallback.clone()),
        Arc::new(HttpStreamBridge::default()),
        StreamBridge::new(),
        Arc::new(Log),
    )
    .unwrap();
    let loader = Arc::new(ProcessPluginLoader::new(root.path().into(), Arc::new(router)).unwrap());
    let host = PluginHost::with_callback_contexts(loader, contexts.clone());
    let id = "fixture-quota";
    let config = RuntimeConfig {
        enabled: true,
        items: BTreeMap::from([(
            id.into(),
            RuntimeItemConfig {
                enabled: true,
                ..Default::default()
            },
        )]),
        ..Default::default()
    };
    let report = host
        .apply_config(
            &config,
            &[PluginFileInfo {
                id: id.into(),
                path: PathBuf::from(env!("CARGO_BIN_EXE_cliproxy-differential")),
                version: None,
            }],
        )
        .await;
    assert!(report.failures.is_empty(), "{report:?}");
    assert_eq!(host.quota_provider_identifiers(), vec![id]);
    let providers = host.quota_providers().await;
    assert_eq!(providers[0].supported_providers, vec![id]);
    let response = host
        .fetch_quota(
            QuotaFetchRequest {
                provider: id.into(),
                http_client: Some(account.clone()),
                ..Default::default()
            },
            Arc::new(Summary),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::to_value(response).unwrap()["subscription"]["plan"],
        "fixture-plan"
    );
    assert_eq!(account.0.load(Ordering::SeqCst), 1);
    assert_eq!(fallback.0.load(Ordering::SeqCst), 0);
    assert!(contexts.is_empty());
    assert!(host
        .fetch_quota(
            QuotaFetchRequest {
                provider: id.into(),
                ..Default::default()
            },
            Arc::new(Summary)
        )
        .await
        .is_err());
    assert_eq!(fallback.0.load(Ordering::SeqCst), 0);
    assert!(contexts.is_empty());
    host.shutdown().await;
    assert_eq!(
        std::fs::read_dir(root.path().join(".cpa")).unwrap().count(),
        0
    );
}
