// Origin: CTOX; License: AGPL-3.0-only
use super::*;
use ctox_cliproxyapi::internal::api::handlers::management::{
    ManagementPluginQuotaError, ManagementPluginQuotaService, ManagementQuotaCredential,
    ManagementQuotaCredentialError, ManagementQuotaCredentialSource,
};
use std::sync::{atomic::AtomicBool, Mutex};

struct AccountHttp {
    generation: Arc<Mutex<u64>>,
    retire: AtomicBool,
    calls: AtomicUsize,
}
impl HostHttpClient for AccountHttp {
    fn execute<'a>(&'a self, request: HttpRequest) -> pluginapi::PluginFuture<'a, HttpResponse> {
        Box::pin(async move {
            assert_eq!(request.url, "https://fixture.invalid/quota");
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.retire.swap(false, Ordering::SeqCst) {
                *self.generation.lock().unwrap() += 1;
            }
            Ok(HttpResponse {
                status_code: 200,
                body: b"fixture-only".to_vec(),
                ..Default::default()
            })
        })
    }
    fn execute_stream<'a>(
        &'a self,
        request: HttpRequest,
    ) -> pluginapi::PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async move {
            Err(Arc::new(std::io::Error::other(format!(
                "unexpected fixture stream: {}",
                request.method
            ))) as Arc<dyn std::error::Error + Send + Sync>)
        })
    }
}
struct Credentials {
    generation: Arc<Mutex<u64>>,
    http: Arc<AccountHttp>,
}
impl ManagementQuotaCredentialSource for Credentials {
    fn resolve(
        &self,
        index: &str,
    ) -> Result<Option<ManagementQuotaCredential>, ManagementQuotaCredentialError> {
        if index != "fixture-account" {
            return Ok(None);
        }
        Ok(Some(ManagementQuotaCredential {
            binding: self.generation.lock().unwrap().to_string(),
            request: QuotaFetchRequest {
                auth_index: index.into(),
                provider: "fixture-quota".into(),
                http_client: Some(self.http.clone()),
                ..Default::default()
            },
            probe: None,
        }))
    }
    fn with_current_binding(
        &self,
        index: &str,
        binding: &str,
        publish: &mut dyn FnMut(),
    ) -> Result<bool, ManagementQuotaCredentialError> {
        let generation = self.generation.lock().unwrap();
        if index != "fixture-account" || binding != generation.to_string() {
            return Ok(false);
        }
        publish();
        Ok(true)
    }
}

#[tokio::test]
async fn management_quota_rejects_retired_binding_after_real_process_http_callback() {
    let base = if cfg!(target_os = "macos") {
        "/Volumes/tmp"
    } else {
        "/tmp"
    };
    let root = tempfile::Builder::new()
        .prefix("cpa-qb-")
        .tempdir_in(base)
        .unwrap();
    let contexts = CallbackContextRegistry::new();
    let fallback = Arc::new(Http::default());
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
    let host = Arc::new(PluginHost::with_callback_contexts(loader, contexts.clone()));
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
    let generation = Arc::new(Mutex::new(1));
    let http = Arc::new(AccountHttp {
        generation: generation.clone(),
        retire: AtomicBool::new(false),
        calls: AtomicUsize::new(0),
    });
    let credentials = Arc::new(Credentials {
        generation,
        http: http.clone(),
    });
    let service = ManagementPluginQuotaService::new(host.clone(), credentials, Arc::new(Summary));
    let response = service
        .fetch("fixture-account", id, Some(id), true)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(response).unwrap()["subscription"]["plan"],
        "fixture-plan"
    );
    http.retire.store(true, Ordering::SeqCst);
    assert!(matches!(
        service.fetch("fixture-account", id, Some(id), true).await,
        Err(ManagementPluginQuotaError::AccountContextMismatch)
    ));
    // A fresh resolution remains usable; the stale response does not poison the account.
    assert!(service
        .fetch("fixture-account", id, Some(id), true)
        .await
        .is_ok());
    assert_eq!(http.calls.load(Ordering::SeqCst), 3);
    assert_eq!(fallback.0.load(Ordering::SeqCst), 0);
    assert!(contexts.is_empty());
    host.shutdown().await;
    assert_eq!(
        std::fs::read_dir(root.path().join(".cpa")).unwrap().count(),
        0
    );
}
