// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use ctox_cliproxyapi::sdk::pluginapi::{HttpResponse, HttpStreamResponse, PluginFuture};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Mutex,
};
const KEY: &str = "synthetic-account-key-private";
struct Wire {
    accepted: Vec<ApiKeyEndpoint>,
    seen: Mutex<Vec<HttpRequest>>,
    malformed: bool,
    retire: Option<Arc<AtomicBool>>,
}
fn model_fixture() -> String {
    let catalog = ctox_cliproxyapi::internal::registry::embedded_models_catalog().unwrap();
    ctox_cliproxyapi::internal::registry::models_for_channel(&catalog, "xai")
        .unwrap()
        .remove(0)
        .id
}
impl HostHttpClient for Wire {
    fn execute<'a>(&'a self, r: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        let accepted = self
            .accepted
            .iter()
            .any(|e| r.url == format!("{}/models", e.base_url()));
        self.seen.lock().unwrap().push(r);
        if let Some(alive) = &self.retire {
            alive.store(false, Ordering::SeqCst);
        }
        let body = if self.malformed {
            serde_json::json!({"data":[{"id":KEY}]})
        } else {
            serde_json::json!({"data":[{"id":model_fixture()},{"id":model_fixture()}]})
        };
        Box::pin(async move {
            Ok(HttpResponse {
                status_code: if accepted { 200 } else { 401 },
                headers: Default::default(),
                body: serde_json::to_vec(&body).unwrap(),
            })
        })
    }
    fn execute_stream<'a>(&'a self, _: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async {
            Err(Arc::new(std::io::Error::other("unused synthetic stream"))
                as ctox_cliproxyapi::sdk::pluginapi::PluginExecutionError)
        })
    }
}
fn wire(accepted: Vec<ApiKeyEndpoint>) -> Arc<Wire> {
    Arc::new(Wire {
        accepted,
        seen: Mutex::new(Vec::new()),
        malformed: false,
        retire: None,
    })
}
#[tokio::test]
async fn kimi_tests_all_fixed_endpoints_and_chooses_the_accepting_platform() {
    let http = wire(vec![ApiKeyEndpoint::KimiPlatformGlobal]);
    let found = discover_api_key_account(ApiKeyProvider::Kimi, KEY, http.clone(), || Ok(()))
        .await
        .unwrap();
    assert_eq!(found.endpoint, ApiKeyEndpoint::KimiPlatformGlobal);
    assert_eq!(found.models, vec![model_fixture()]);
    assert_eq!(found.attempts.len(), 4);
    let seen = http.seen.lock().unwrap();
    assert_eq!(seen.len(), 4);
    for r in &*seen {
        assert_eq!(r.method, "GET");
        assert!(r.body.is_empty());
        assert_eq!(r.headers["Authorization"], vec![format!("Bearer {KEY}")]);
        assert_eq!(
            r.headers["User-Agent"],
            vec!["CTOX/provider-account-discovery"]
        );
    }
    assert!(!serde_json::to_string(&found).unwrap().contains(KEY));
}
#[tokio::test]
async fn coding_takes_precedence_without_skipping_platform_validation() {
    let http = wire(vec![
        ApiKeyEndpoint::KimiPlatformGlobal,
        ApiKeyEndpoint::KimiCoding,
    ]);
    let found = discover_api_key_account(ApiKeyProvider::Kimi, KEY, http.clone(), || Ok(()))
        .await
        .unwrap();
    assert_eq!(found.endpoint, ApiKeyEndpoint::KimiCoding);
    assert_eq!(http.seen.lock().unwrap().len(), 4);
}
#[tokio::test]
async fn rejected_key_reports_checked_endpoints_and_only_real_statuses() {
    let error = discover_api_key_account(ApiKeyProvider::Kimi, KEY, wire(vec![]), || Ok(()))
        .await
        .unwrap_err();
    assert_eq!(error.attempts.len(), 4);
    assert!(error.attempts.iter().all(|a| a.http_status == Some(401)
        && matches!(a.failure, Some(CatalogFailure::CredentialsRejected))));
    let text = error.to_string();
    assert!(text.contains("https://api.kimi.com/coding/v1"));
    assert!(text.contains("https://api.moonshot.ai/v1"));
    assert!(!text.contains(KEY));
}
#[tokio::test]
async fn credentials_echoed_in_catalog_are_not_returned_or_persisted() {
    let http = Arc::new(Wire {
        accepted: vec![ApiKeyEndpoint::Xai],
        seen: Mutex::new(Vec::new()),
        malformed: true,
        retire: None,
    });
    let error = discover_api_key_account(ApiKeyProvider::Xai, KEY, http, || Ok(()))
        .await
        .unwrap_err();
    assert!(matches!(
        error.attempts[0].failure,
        Some(CatalogFailure::InvalidCatalog)
    ));
    assert!(!serde_json::to_string(&error).unwrap().contains(KEY));
}
#[tokio::test]
async fn revoked_authority_denies_network_and_retires_received_results() {
    let http = wire(vec![ApiKeyEndpoint::Xai]);
    let error = discover_api_key_account(ApiKeyProvider::Xai, KEY, http.clone(), || {
        anyhow::bail!("synthetic denied authority")
    })
    .await
    .unwrap_err();
    assert!(matches!(error.reason, CatalogFailure::AuthorityRetired));
    assert!(http.seen.lock().unwrap().is_empty());
    let alive = Arc::new(AtomicBool::new(true));
    let http = Arc::new(Wire {
        accepted: vec![ApiKeyEndpoint::Xai],
        seen: Mutex::new(Vec::new()),
        malformed: false,
        retire: Some(alive.clone()),
    });
    let error = discover_api_key_account(ApiKeyProvider::Xai, KEY, http, || {
        anyhow::ensure!(alive.load(Ordering::SeqCst), "retired");
        Ok(())
    })
    .await
    .unwrap_err();
    assert!(matches!(error.reason, CatalogFailure::AuthorityRetired));
}
#[test]
fn commit_keeps_credentials_private_and_records_actual_endpoint_and_models() {
    let root = tempfile::tempdir().unwrap();
    let found = DiscoveredAccount {
        endpoint: ApiKeyEndpoint::KimiPlatformGlobal,
        models: vec![model_fixture()],
        attempts: Vec::new(),
    };
    let receipt = commit_discovered(root.path(), KEY, found, || Ok(())).unwrap();
    let stored = super::super::load_provider_integration_config(root.path()).unwrap();
    assert_eq!(stored.config.api_key_accounts.len(), 1);
    let account = &stored.config.api_key_accounts[0];
    assert_eq!(account.id, receipt.id);
    assert_eq!(account.endpoint, ApiKeyEndpoint::KimiPlatformGlobal);
    assert_eq!(account.priority, 0);
    assert_eq!(
        crate::secrets::read_secret_value(
            root.path(),
            &account.api_key_secret.scope,
            &account.api_key_secret.name
        )
        .unwrap(),
        KEY
    );
    assert!(!serde_json::to_string(&stored.config).unwrap().contains(KEY));
    assert!(!serde_json::to_string(&receipt).unwrap().contains(KEY));
}
#[test]
fn revocation_before_config_commit_removes_only_the_new_secret() {
    let root = tempfile::tempdir().unwrap();
    crate::secrets::write_secret_record(
        root.path(),
        "provider-api-keys",
        "retained-existing",
        "synthetic-existing-credential",
        None,
        serde_json::json!({}),
    )
    .unwrap();
    let checks = AtomicUsize::new(0);
    let found = DiscoveredAccount {
        endpoint: ApiKeyEndpoint::Xai,
        models: vec![model_fixture()],
        attempts: Vec::new(),
    };
    assert!(commit_discovered(root.path(), KEY, found, || {
        anyhow::ensure!(checks.fetch_add(1, Ordering::SeqCst) < 2, "retired");
        Ok(())
    })
    .is_err());
    assert!(super::super::load_provider_integration_config(root.path())
        .unwrap()
        .config
        .api_key_accounts
        .is_empty());
    assert_eq!(
        crate::secrets::list_secrets(root.path(), Some("provider-api-keys"))
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        crate::secrets::read_secret_value(root.path(), "provider-api-keys", "retained-existing")
            .unwrap(),
        "synthetic-existing-credential"
    );
}
