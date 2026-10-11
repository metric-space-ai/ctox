// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use crate::execution::cliproxyapi_integration::{
    save_provider_integration_config, ApiKeyEndpoint, ProviderIntegrationConfig, ProviderSecretRef,
};
use ctox_cliproxyapi::sdk::pluginapi::{
    HttpRequest, HttpResponse, HttpStreamResponse, PluginFuture,
};
use std::sync::Mutex;
#[derive(Default)]
struct Wire {
    seen: Mutex<Vec<HttpRequest>>,
    root: Option<PathBuf>,
    rotate: bool,
}
impl HostHttpClient for Wire {
    fn execute_stream<'a>(&'a self, _request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async {
            Err(Arc::new(std::io::Error::other("buffered fixture"))
                as ctox_cliproxyapi::sdk::pluginapi::PluginExecutionError)
        })
    }
    fn execute<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        self.seen.lock().unwrap().push(request);
        if self.rotate {
            crate::secrets::write_secret_record(
                self.root.as_ref().unwrap(),
                "provider-api-keys",
                "api-first",
                "fixture-credential-first",
                None,
                "test",
                "private fixture",
            )
            .unwrap();
        }
        Box::pin(async {
            Ok(HttpResponse { status_code: 200, headers: Default::default(), body: br#"{"id":"chatcmpl","object":"chat.completion","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}]}"#.to_vec() })
        })
    }
}
fn model_fixture() -> String {
    // Reuse an existing port wire fixture; introduce no model identifiers,
    // defaults or catalog suggestions into this implementation.
    let catalog = ctox_cliproxyapi::internal::registry::embedded_models_catalog().unwrap();
    ctox_cliproxyapi::internal::registry::models_for_channel(&catalog, "xai")
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
        .id
}
fn account(id: &str, priority: i32) -> ApiKeyAccountConfig {
    ApiKeyAccountConfig {
        id: id.into(),
        endpoint: ApiKeyEndpoint::Xai,
        api_key_secret: ProviderSecretRef::new("provider-api-keys", format!("api-{id}")),
        disabled: false,
        priority,
        models: vec![model_fixture()],
    }
}
fn setup() -> (tempfile::TempDir, StoredProviderIntegrationConfig) {
    let root = tempfile::tempdir().unwrap();
    let config = ProviderIntegrationConfig {
        api_key_accounts: vec![account("first", 50), account("second", 0)],
        ..Default::default()
    };
    for a in &config.api_key_accounts {
        crate::secrets::write_secret_record(
            root.path(),
            &a.api_key_secret.scope,
            &a.api_key_secret.name,
            &format!("fixture-credential-{}", a.id),
            None,
            "test",
            "private fixture",
        )
        .unwrap();
    }
    save_provider_integration_config(root.path(), 0, &config).unwrap();
    let stored =
        crate::execution::cliproxyapi_integration::load_provider_integration_config(root.path())
            .unwrap();
    (root, stored)
}
#[tokio::test]
async fn native_api_selects_stably_and_substitutes_only_its_own_credential() {
    let (root, stored) = setup();
    let wire = Arc::new(Wire::default());
    let handlers = build_with_http(root.path(), &stored, wire.clone()).unwrap();
    let handler = &handlers["xai"];
    let body=serde_json::to_vec(&serde_json::json!({"model":model_fixture(),"input":"Hi","Authorization":"caller-credential-must-not-forward"})).unwrap();
    for _ in 0..3 {
        let OpenAiResponsesRouteResponse::Buffered(response) = handler.handle_route(&body).await
        else {
            panic!("buffered expected")
        };
        assert_eq!(response.status(), 200);
    }
    let seen = wire.seen.lock().unwrap();
    assert_eq!(seen.len(), 3);
    for r in &*seen {
        assert_eq!(r.url, "https://api.x.ai/v1/chat/completions");
        assert!(r
            .headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("authorization")
                && v == &vec!["Bearer fixture-credential-first".to_string()]));
        assert!(!r
            .headers
            .values()
            .flatten()
            .any(|v| v.contains("caller-credential")));
    }
}
#[tokio::test]
async fn same_bytes_new_generation_rejects_stale_route_before_io() {
    let (root, stored) = setup();
    let wire = Arc::new(Wire::default());
    let handlers = build_with_http(root.path(), &stored, wire.clone()).unwrap();
    crate::secrets::write_secret_record(
        root.path(),
        "provider-api-keys",
        "api-first",
        "fixture-credential-first",
        None,
        "test",
        "private fixture",
    )
    .unwrap();
    let body =
        serde_json::to_vec(&serde_json::json!({"model":model_fixture(),"input":"Hi"})).unwrap();
    let OpenAiResponsesRouteResponse::Buffered(response) =
        handlers["xai"].handle_route(&body).await
    else {
        panic!("buffered expected")
    };
    assert_eq!(response.status(), 409);
    assert!(wire.seen.lock().unwrap().is_empty());
}
#[tokio::test]
async fn retired_binding_during_io_cannot_publish_a_success() {
    let (root, stored) = setup();
    let wire = Arc::new(Wire {
        root: Some(root.path().to_path_buf()),
        rotate: true,
        ..Default::default()
    });
    let handlers = build_with_http(root.path(), &stored, wire.clone()).unwrap();
    let body =
        serde_json::to_vec(&serde_json::json!({"model":model_fixture(),"input":"Hi"})).unwrap();
    let OpenAiResponsesRouteResponse::Buffered(response) =
        handlers["xai"].handle_route(&body).await
    else {
        panic!("buffered expected")
    };
    assert_eq!(response.status(), 409);
    assert_eq!(wire.seen.lock().unwrap().len(), 1);
}
#[test]
fn unknown_empty_catalog_has_no_wildcard_api_route() {
    let (root, mut stored) = setup();
    for a in &mut stored.config.api_key_accounts {
        a.models.clear();
    }
    save_provider_integration_config(root.path(), stored.revision, &stored.config).unwrap();
    assert!(
        build_with_http(root.path(), &stored, Arc::new(Wire::default()))
            .unwrap()
            .is_empty()
    );
}
#[test]
fn private_topology_rejects_aliases_and_borrowed_secret_scopes() {
    let mut config = ProviderIntegrationConfig {
        api_key_accounts: vec![account("first", 0), account("second", 0)],
        ..Default::default()
    };
    config.validate().unwrap();
    config.api_key_accounts[1].api_key_secret = config.api_key_accounts[0].api_key_secret.clone();
    assert!(config.validate().is_err());
    config.api_key_accounts.truncate(1);
    config.api_key_accounts[0].api_key_secret.scope = "credentials".into();
    assert!(config.validate().is_err());
}

#[tokio::test]
async fn topology_and_credential_changes_refresh_without_a_service_restart() {
    let (root, stored) = setup();
    let initial = ProviderIntegrationConfig::default();
    save_provider_integration_config(root.path(), stored.revision, &initial).unwrap();
    let snapshot =
        crate::execution::cliproxyapi_integration::load_provider_integration_config(root.path())
            .unwrap();
    let routes = build(root.path(), &snapshot, Duration::from_secs(5)).unwrap();
    let wire = Arc::new(Wire::default());
    *routes.http.lock().unwrap() = Some(wire.clone());
    assert!(routes.route_for("xai").unwrap().is_none());
    save_provider_integration_config(root.path(), snapshot.revision, &stored.config).unwrap();
    let first = routes.route_for("xai").unwrap().unwrap();
    assert!(Arc::ptr_eq(
        &first,
        &routes.route_for("xai").unwrap().unwrap()
    ));
    crate::secrets::write_secret_record(
        root.path(),
        "provider-api-keys",
        "api-first",
        "rotated-fixture-credential",
        None,
        "test",
        "private fixture",
    )
    .unwrap();
    let replacement = routes.route_for("xai").unwrap().unwrap();
    assert!(!Arc::ptr_eq(&first, &replacement));
    let body =
        serde_json::to_vec(&serde_json::json!({"model":model_fixture(),"input":"Hi"})).unwrap();
    let OpenAiResponsesRouteResponse::Buffered(stale) = first.handle_route(&body).await else {
        panic!("buffered expected")
    };
    assert_eq!(stale.status(), 409);
    let OpenAiResponsesRouteResponse::Buffered(fresh) = replacement.handle_route(&body).await
    else {
        panic!("buffered expected")
    };
    assert_eq!(fresh.status(), 200);
    let seen = wire.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert!(seen[0]
        .headers
        .values()
        .flatten()
        .any(|value| value == "Bearer rotated-fixture-credential"));
    drop(seen);
    let mut disabled =
        crate::execution::cliproxyapi_integration::load_provider_integration_config(root.path())
            .unwrap();
    disabled.config.api_key_accounts[0].disabled = true;
    save_provider_integration_config(root.path(), disabled.revision, &disabled.config).unwrap();
    let enabled = routes.route_for("xai").unwrap().unwrap();
    let OpenAiResponsesRouteResponse::Buffered(response) = enabled.handle_route(&body).await else {
        panic!("buffered expected")
    };
    assert_eq!(response.status(), 200);
    assert!(wire.seen.lock().unwrap()[1]
        .headers
        .values()
        .flatten()
        .any(|value| value == "Bearer fixture-credential-second"));
    let mut removed =
        crate::execution::cliproxyapi_integration::load_provider_integration_config(root.path())
            .unwrap();
    removed.config.api_key_accounts.clear();
    save_provider_integration_config(root.path(), removed.revision, &removed.config).unwrap();
    assert!(routes.route_for("xai").unwrap().is_none());
}
