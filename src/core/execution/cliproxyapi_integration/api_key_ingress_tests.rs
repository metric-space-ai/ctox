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
    crate::persistence::test_support::run(move |owner_root| {
        let root = owner_root;
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
        Ok(())
    })
    .unwrap()
}
#[test]
fn revocation_before_config_commit_removes_only_the_new_secret() {
    crate::persistence::test_support::run(move |owner_root| {
        let root = owner_root;
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
        let id = uuid::Uuid::new_v4();
        assert!(commit_discovered_with_id(
            root.path(),
            KEY,
            found,
            || {
                anyhow::ensure!(checks.fetch_add(1, Ordering::SeqCst) < 2, "retired");
                Ok(())
            },
            id,
        )
        .is_err());
        assert!(super::super::load_provider_integration_config(root.path())
            .unwrap()
            .config
            .api_key_accounts
            .is_empty());
        assert!(!crate::secrets::secret_exists(
            root.path(),
            "provider-api-keys",
            &format!("api-key-{id}")
        )
        .unwrap());
        assert_eq!(
            crate::secrets::read_secret_value(
                root.path(),
                "provider-api-keys",
                "retained-existing"
            )
            .unwrap(),
            "synthetic-existing-credential"
        );
        Ok(())
    })
    .unwrap()
}

#[test]
fn owned_creation_replay_preserves_key_revision_and_rejects_changes() {
    crate::persistence::test_support::run_async(move |owner_root| async move {
        use sha2::{Digest, Sha256};
        let root = owner_root;
        let operation = uuid::Uuid::new_v4();
        let mut hash = Sha256::new();
        hash.update(b"verified-owner");
        hash.update([0]);
        hash.update(operation.to_string().as_bytes());
        let id = format!("api-{:x}", hash.finalize());
        let first = commit_discovered_record(
            root.path(),
            KEY,
            DiscoveredAccount {
                endpoint: ApiKeyEndpoint::Xai,
                models: vec![model_fixture()],
                attempts: Vec::new(),
            },
            || Ok(()),
            id,
            Some(("verified-owner", &operation.to_string())),
        )
        .unwrap();
        let stored = super::super::load_provider_integration_config(root.path()).unwrap();
        let account = &stored.config.api_key_accounts[0];
        let generation = crate::secrets::secret_record_content_version(
            root.path(),
            &account.api_key_secret.scope,
            &account.api_key_secret.name,
        )
        .unwrap();
        let replay = install_api_key_account_owned_authorized(
            root.path(),
            "verified-owner",
            operation,
            ApiKeyProvider::Xai,
            Zeroizing::new(KEY.into()),
            || Ok(()),
        )
        .await
        .unwrap();
        assert_eq!(first.id, replay.id);
        assert_eq!(first.revision, replay.revision);
        assert_eq!(
            generation,
            crate::secrets::secret_record_content_version(
                root.path(),
                &account.api_key_secret.scope,
                &account.api_key_secret.name
            )
            .unwrap()
        );
        for (provider, key) in [
            (ApiKeyProvider::Xai, "changed-synthetic-key"),
            (ApiKeyProvider::Kimi, KEY),
        ] {
            assert!(install_api_key_account_owned_authorized(
                root.path(),
                "verified-owner",
                operation,
                provider,
                Zeroizing::new(key.into()),
                || Ok(())
            )
            .await
            .is_err());
        }
        assert!(install_api_key_account_owned_authorized(
            root.path(),
            "verified-owner",
            operation,
            ApiKeyProvider::Xai,
            Zeroizing::new(KEY.into()),
            || anyhow::bail!("retired")
        )
        .await
        .is_err());
        assert_eq!(
            super::super::load_provider_integration_config(root.path())
                .unwrap()
                .config
                .api_key_accounts,
            stored.config.api_key_accounts
        );
        Ok(())
    })
    .unwrap()
}
