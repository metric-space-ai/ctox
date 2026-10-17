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
    retire_models: bool,
    status: Option<u16>,
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
                serde_json::json!({"purpose":"private fixture"}),
            )
            .unwrap();
        }
        if self.retire_models {
            let conn = crate::business_os::store::open_store(self.root.as_ref().unwrap()).unwrap();
            conn.execute(
                "INSERT INTO business_provider_federation_models VALUES (?1,?2,?3)",
                ["owner", "xai", "[]"],
            )
            .unwrap();
        }
        Box::pin(async move {
            if self.status == Some(0) {
                return Err(
                    Arc::new(std::io::Error::other("synthetic transport failure"))
                        as ctox_cliproxyapi::sdk::pluginapi::PluginExecutionError,
                );
            }
            Ok(HttpResponse { status_code: self.status.unwrap_or(200), headers: Default::default(), body: br#"{"id":"chatcmpl","object":"chat.completion","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}]}"#.to_vec() })
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
        owner_user_id: None,
        creation_operation: None,
        disabled: false,
        priority,
        models: vec![model_fixture()],
    }
}
fn setup(
    root: std::sync::Arc<tempfile::TempDir>,
) -> (
    std::sync::Arc<tempfile::TempDir>,
    StoredProviderIntegrationConfig,
) {
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
            serde_json::json!({"purpose":"private fixture"}),
        )
        .unwrap();
    }
    save_provider_integration_config(root.path(), 0, &config).unwrap();
    let stored =
        crate::execution::cliproxyapi_integration::load_provider_integration_config(root.path())
            .unwrap();
    (root, stored)
}
#[test]
fn native_api_selects_stably_and_substitutes_only_its_own_credential() {
    crate::persistence::test_support::run_async(move |owner_root| async move {
    let (root, stored) = setup(owner_root);
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
    Ok(())
})
.unwrap()
}
#[test]
fn same_bytes_new_generation_rejects_stale_route_before_io() {
    crate::persistence::test_support::run_async(move |owner_root| async move {
        let (root, stored) = setup(owner_root);
        let wire = Arc::new(Wire::default());
        let handlers = build_with_http(root.path(), &stored, wire.clone()).unwrap();
        crate::secrets::write_secret_record(
            root.path(),
            "provider-api-keys",
            "api-first",
            "fixture-credential-first",
            None,
            serde_json::json!({"purpose":"private fixture"}),
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
        Ok(())
    })
    .unwrap()
}
#[test]
fn retired_binding_during_io_cannot_publish_a_success() {
    crate::persistence::test_support::run_async(move |owner_root| async move {
        let (root, stored) = setup(owner_root);
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
        Ok(())
    })
    .unwrap()
}
#[test]
fn unknown_empty_catalog_has_no_wildcard_api_route() {
    crate::persistence::test_support::run(move |owner_root| {
        let (root, mut stored) = setup(owner_root);
        for a in &mut stored.config.api_key_accounts {
            a.models.clear();
        }
        save_provider_integration_config(root.path(), stored.revision, &stored.config).unwrap();
        assert!(
            build_with_http(root.path(), &stored, Arc::new(Wire::default()))
                .unwrap()
                .is_empty()
        );
        Ok(())
    })
    .unwrap()
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
    config.api_key_accounts[0].api_key_secret.scope = "provider-api-keys".into();
    config.api_key_accounts[0].id = crate::execution::cliproxyapi_xai::ACCOUNT_ID.into();
    assert!(config.validate().is_err());
}

#[test]
fn topology_and_credential_changes_refresh_without_a_service_restart() {
    crate::persistence::test_support::run_async(move |owner_root| async move {
        let (root, stored) = setup(owner_root);
        let initial = ProviderIntegrationConfig::default();
        save_provider_integration_config(root.path(), stored.revision, &initial).unwrap();
        let snapshot = crate::execution::cliproxyapi_integration::load_provider_integration_config(
            root.path(),
        )
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
            serde_json::json!({"purpose":"private fixture"}),
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
            crate::execution::cliproxyapi_integration::load_provider_integration_config(
                root.path(),
            )
            .unwrap();
        disabled.config.api_key_accounts[0].disabled = true;
        save_provider_integration_config(root.path(), disabled.revision, &disabled.config).unwrap();
        let enabled = routes.route_for("xai").unwrap().unwrap();
        let OpenAiResponsesRouteResponse::Buffered(response) = enabled.handle_route(&body).await
        else {
            panic!("buffered expected")
        };
        assert_eq!(response.status(), 200);
        assert!(wire.seen.lock().unwrap()[1]
            .headers
            .values()
            .flatten()
            .any(|value| value == "Bearer fixture-credential-second"));
        let mut removed =
            crate::execution::cliproxyapi_integration::load_provider_integration_config(
                root.path(),
            )
            .unwrap();
        removed.config.api_key_accounts.clear();
        save_provider_integration_config(root.path(), removed.revision, &removed.config).unwrap();
        assert!(routes.route_for("xai").unwrap().is_none());
        Ok(())
    })
    .unwrap()
}

fn owned_setup(
    root: std::sync::Arc<tempfile::TempDir>,
) -> (
    std::sync::Arc<tempfile::TempDir>,
    StoredProviderIntegrationConfig,
) {
    let (root, mut stored) = setup(root);
    stored.config.api_key_accounts.truncate(1);
    let account = &mut stored.config.api_key_accounts[0];
    account.owner_user_id = Some("owner".into());
    account.creation_operation = Some(uuid::Uuid::new_v4().to_string());
    account.models.clear();
    save_provider_integration_config(root.path(), stored.revision, &stored.config).unwrap();
    crate::business_os::provider_federation::seed_gateway_api_catalog(
        root.path(),
        &stored.config.api_key_accounts[0],
        &[model_fixture()],
    )
    .unwrap();
    stored =
        crate::execution::cliproxyapi_integration::load_provider_integration_config(root.path())
            .unwrap();
    (root, stored)
}
#[test]
fn owned_api_uses_live_catalog_and_retires_remove_readd_route() {
    crate::persistence::test_support::run_async(move |owner_root| async move {
        let (root, stored) = owned_setup(owner_root);
        assert!(stored.config.api_key_accounts[0].models.is_empty());
        let routes = build(root.path(), &stored, Duration::from_secs(5)).unwrap();
        let wire = Arc::new(Wire::default());
        *routes.http.lock().unwrap() = Some(wire.clone());
        let first = routes.route_for("xai").unwrap().unwrap();
        let body =
            serde_json::to_vec(&serde_json::json!({"model":model_fixture(),"input":"Hi"})).unwrap();
        let OpenAiResponsesRouteResponse::Buffered(ok) = first.handle_route(&body).await else {
            panic!("buffered expected")
        };
        assert_eq!(ok.status(), 200);
        let conn = crate::business_os::store::open_store(root.path()).unwrap();
        conn.execute(
            "INSERT INTO business_provider_federation_models VALUES (?1,?2,?3)",
            ["owner", "xai", "[]"],
        )
        .unwrap();
        let OpenAiResponsesRouteResponse::Buffered(retired) = first.handle_route(&body).await
        else {
            panic!("buffered expected")
        };
        assert_eq!(retired.status(), 409);
        assert!(routes.route_for("xai").unwrap().is_none());
        assert!(routes.cached.lock().unwrap().is_empty());
        conn.execute(
            "UPDATE business_provider_federation_models SET models_json=?1 WHERE owner_user_id=?2",
            rusqlite::params![
                serde_json::to_string(&vec![model_fixture()]).unwrap(),
                "owner"
            ],
        )
        .unwrap();
        let replacement = routes.route_for("xai").unwrap().unwrap();
        assert!(!Arc::ptr_eq(&first, &replacement));
        let OpenAiResponsesRouteResponse::Buffered(fresh) = replacement.handle_route(&body).await
        else {
            panic!("buffered expected")
        };
        assert_eq!(fresh.status(), 200);
        assert_eq!(wire.seen.lock().unwrap().len(), 2);
        Ok(())
    })
    .unwrap()
}
#[test]
fn owned_api_model_withdrawal_during_io_cannot_publish_success() {
    crate::persistence::test_support::run_async(move |owner_root| async move {
        let (root, stored) = owned_setup(owner_root);
        let wire = Arc::new(Wire {
            root: Some(root.path().to_path_buf()),
            retire_models: true,
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
        Ok(())
    })
    .unwrap()
}

struct HttpFixture(Arc<Handler>);
impl ctox_cliproxyapi::sdk::api::handlers::openai::openai_responses_handlers::OpenAiResponsesRouteHandler for HttpFixture {
    fn handle_provider_route<'a>(&'a self,_provider:Option<&'a str>,body:&'a [u8])->std::pin::Pin<Box<dyn std::future::Future<Output=OpenAiResponsesRouteResponse>+Send+'a>> {
        Box::pin(self.0.handle_route(body))
    }
}
async fn scoped_http_request(handler: Arc<Handler>, pin: Option<&str>) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let body =
        serde_json::to_vec(&serde_json::json!({"model":model_fixture(),"input":"Hi"})).unwrap();
    let mut request=format!("POST /v1/responses HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nX-CTOX-Session-ID: ordinary-session\r\n",body.len());
    if let Some(pin) = pin {
        request.push_str(&format!("X-CTOX-Account: {pin}\r\n"));
    }
    request.push_str("\r\n");
    let mut bytes = request.into_bytes();
    bytes.extend(body);
    let (mut server, mut client) = tokio::io::duplex(64 * 1024);
    let exchange = async move {
        let serve = async move {
            ctox_cliproxyapi::internal::api::server::serve_responses_connection(
                &mut server,
                &HttpFixture(handler),
            )
            .await
            .unwrap();
        };
        let read = async move {
            client.write_all(&bytes).await.unwrap();
            client.shutdown().await.unwrap();
            let mut response = Vec::new();
            client.read_to_end(&mut response).await.unwrap();
            String::from_utf8(response).unwrap()
        };
        let (_, response) = tokio::join!(serve, read);
        response
    };
    tokio::time::timeout(Duration::from_secs(5), exchange)
        .await
        .unwrap()
}
#[test]
fn native_api_http_pin_uses_exact_account_without_rebinding_session() {
    crate::persistence::test_support::run_async(move |owner_root| async move {
        let (root, stored) = setup(owner_root);
        let wire = Arc::new(Wire::default());
        let handlers = build_with_http(root.path(), &stored, wire.clone()).unwrap();
        let handler = handlers["xai"].clone();
        for (pin, expected) in [(None, "first"), (Some("second"), "second"), (None, "first")] {
            let response = scoped_http_request(handler.clone(), pin).await;
            assert!(response.starts_with("HTTP/1.1 200"));
            assert_eq!(
                response.contains("X-CTOX-Upstream-Status: 200\r\n"),
                pin.is_some()
            );
            assert!(
                response.contains(&format!("X-CTOX-Account-Selected: {expected}\r\n")),
                "selected-account acknowledgement differs from the requested account"
            );
            let seen = wire.seen.lock().unwrap();
            let last = seen.last().unwrap();
            assert!(
                last.headers
                    .values()
                    .flatten()
                    .any(|value| value == &format!("Bearer fixture-credential-{expected}")),
                "the selected account must use only its own fixture credential"
            );
        }
        assert_eq!(wire.seen.lock().unwrap().len(), 3);
        Ok(())
    })
    .unwrap()
}
#[test]
fn native_api_http_unknown_or_disabled_pin_never_borrows_sibling_credentials() {
    crate::persistence::test_support::run_async(move |owner_root| async move {
        let (root, mut stored) = setup(owner_root);
        stored.config.api_key_accounts[1].disabled = true;
        save_provider_integration_config(root.path(), stored.revision, &stored.config).unwrap();
        let wire = Arc::new(Wire::default());
        let handlers = build_with_http(root.path(), &stored, wire.clone()).unwrap();
        for pin in ["missing", "second"] {
            let response = scoped_http_request(handlers["xai"].clone(), Some(pin)).await;
            assert!(response.starts_with("HTTP/1.1 503"));
            assert!(!response.contains("X-CTOX-Account-Selected:"));
            assert!(!response.contains("X-CTOX-Upstream-Status:"));
            assert!(!response.contains("X-CTOX-Error-Class:"));
        }
        assert!(wire.seen.lock().unwrap().is_empty());
        Ok(())
    })
    .unwrap()
}

#[test]
fn native_api_http_only_received_upstream_errors_report_provider_auth() {
    for status in [401, 0] {
        crate::persistence::test_support::run_async(move |owner_root| async move {
            let (root, stored) = setup(owner_root);
            let wire = Arc::new(Wire {
                status: Some(status),
                ..Default::default()
            });
            let handlers = build_with_http(root.path(), &stored, wire.clone()).unwrap();
            let response = scoped_http_request(handlers["xai"].clone(), Some("second")).await;
            if status == 401 {
                assert!(response.starts_with("HTTP/1.1 401"));
                assert!(response.contains("X-CTOX-Error-Class: auth\r\n"));
                assert!(response.contains("X-CTOX-Upstream-Status: 401\r\n"));
            } else {
                assert!(response.starts_with("HTTP/1.1 502"));
                assert!(!response.contains("X-CTOX-Error-Class:"));
                assert!(!response.contains("X-CTOX-Upstream-Status:"));
            }
            assert!(response.contains("X-CTOX-Account-Selected: second\r\n"));
            assert_eq!(wire.seen.lock().unwrap().len(), 1);
            Ok(())
        })
        .unwrap();
    }
}

#[test]
fn api_pool_missing_credential_keeps_siblings_available_without_pin_fallback() {
    crate::persistence::test_support::run_async(move |owner_root| async move {
        let (root, stored) = setup(owner_root);
        let wire = Arc::new(Wire::default());
        let routes = build(root.path(), &stored, Duration::from_secs(5)).unwrap();
        *routes.http.lock().unwrap() = Some(wire.clone());
        let old = routes.route_for("xai").unwrap().unwrap();
        crate::secrets::delete_secret_record(root.path(), "provider-api-keys", "api-first")
            .unwrap();
        let fresh = routes
            .route_for("xai")
            .expect("a missing account must not disable its provider pool")
            .unwrap();
        let response = scoped_http_request(fresh.clone(), None).await;
        assert!(response.starts_with("HTTP/1.1 200"));
        assert!(response.contains("X-CTOX-Account-Selected: second\r\n"));
        let pinned = scoped_http_request(fresh, Some("first")).await;
        assert!(pinned.starts_with("HTTP/1.1 503"));
        assert!(!pinned.contains("X-CTOX-Upstream-Status:"));
        let stale = scoped_http_request(old, Some("first")).await;
        assert!(stale.starts_with("HTTP/1.1 409"));
        {
            let seen = wire.seen.lock().unwrap();
            assert_eq!(seen.len(), 1);
            assert!(seen[0]
                .headers
                .values()
                .flatten()
                .any(|v| v == "Bearer fixture-credential-second"));
        }
        crate::secrets::write_secret_record(
            root.path(),
            "provider-api-keys",
            "api-first",
            "fixture-credential-first",
            None,
            serde_json::json!({"purpose":"private fixture"}),
        )
        .unwrap();
        let restored = routes.route_for("xai").unwrap().unwrap();
        for (pin, expected) in [(Some("first"), "first"), (None, "second")] {
            let response = scoped_http_request(restored.clone(), pin).await;
            assert!(response.starts_with("HTTP/1.1 200"));
            assert!(response.contains(&format!("X-CTOX-Account-Selected: {expected}\r\n")));
        }
        assert_eq!(wire.seen.lock().unwrap().len(), 3);
        Ok(())
    })
    .unwrap()
}
#[test]
fn api_pool_invalid_local_key_is_not_a_provider_auth_verdict_or_wildcard() {
    crate::persistence::test_support::run_async(move |owner_root| async move {
        let (root, stored) = setup(owner_root);
        let wire = Arc::new(Wire::default());
        crate::secrets::write_secret_record(
            root.path(),
            "provider-api-keys",
            "api-first",
            "",
            None,
            serde_json::json!({}),
        )
        .unwrap();
        let handlers = build_with_http(root.path(), &stored, wire.clone())
            .expect("invalid local key must not prevent healthy account use");
        let response = scoped_http_request(handlers["xai"].clone(), None).await;
        assert!(response.starts_with("HTTP/1.1 200"));
        assert!(response.contains("X-CTOX-Account-Selected: second\r\n"));
        let pinned = scoped_http_request(handlers["xai"].clone(), Some("first")).await;
        assert!(pinned.starts_with("HTTP/1.1 503"));
        assert!(!pinned.contains("X-CTOX-Error-Class:"));
        assert_eq!(wire.seen.lock().unwrap().len(), 1);
        crate::secrets::delete_secret_record(root.path(), "provider-api-keys", "api-second")
            .unwrap();
        assert!(build_with_http(root.path(), &stored, wire)
            .unwrap()
            .is_empty());
        Ok(())
    })
    .unwrap()
}
