// Origin: CTOX
// License: AGPL-3.0-only
//! Operator-requested live model discovery for the inherited Pi route.
//! No inference, queue mutation, credential export or fallback selection.
use super::{resolve_inherited_catalog_route, InheritedCodingRoute};
use crate::execution::models::runtime_env;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    io::Read,
    path::Path,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

pub(crate) struct NativeInheritedAccountMetadata {
    pub provider: String,
    // Holder-private configuration/credential fingerprint. Never serialize.
    pub private_binding: String,
}

fn private_binding(route: &InheritedCodingRoute, credential: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    digest.update(b"ctox/native-account-binding/v1");
    for value in [
        route.provider.as_str(),
        route.base_url.as_str(),
        route.credential_key,
        credential,
    ] {
        digest.update((value.len() as u64).to_be_bytes());
        digest.update(value.as_bytes());
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

// This is configured account existence only: neither a model-list observation
// nor a credential/limit/Hi validation. No model or credential leaves the host.
fn native_metadata_from_current(
    route: &InheritedCodingRoute,
    credential: Option<&str>,
    current_route: Option<&InheritedCodingRoute>,
    current_credential: Option<&str>,
) -> anyhow::Result<Option<NativeInheritedAccountMetadata>> {
    anyhow::ensure!(
        current_route == Some(route) && current_credential == credential,
        "native main account configuration changed"
    );
    let Some(credential) = credential else {
        return Ok(None);
    };
    if credential.trim().is_empty()
        || credential.len() > 8192
        || credential.chars().any(char::is_control)
    {
        return Ok(None);
    }
    Ok(Some(NativeInheritedAccountMetadata {
        provider: route.provider.clone(),
        private_binding: private_binding(route, credential),
    }))
}

pub(super) fn account_metadata(
    root: &Path,
) -> anyhow::Result<Option<NativeInheritedAccountMetadata>> {
    let route = resolve_inherited_catalog_route(root)
        .map_err(|_| anyhow::anyhow!("native main account route is unavailable"))?;
    let credential = runtime_env::load_runtime_env_map(root)
        .map_err(|_| anyhow::anyhow!("native main credential metadata is unavailable"))?
        .remove(route.credential_key)
        .map(Zeroizing::new);
    let current_route = resolve_inherited_catalog_route(root)
        .map_err(|_| anyhow::anyhow!("native main account route is unavailable"))?;
    let current_credential = runtime_env::load_runtime_env_map(root)
        .map_err(|_| anyhow::anyhow!("native main credential metadata is unavailable"))?
        .remove(current_route.credential_key)
        .map(Zeroizing::new);
    native_metadata_from_current(
        &route,
        credential.as_ref().map(|value| value.as_str()),
        Some(&current_route),
        current_credential.as_ref().map(|value| value.as_str()),
    )
}

const MAX_BODY: u64 = 65_536;
const MAX_MODELS: usize = 1024;
const DEADLINE: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Failure {
    UnsupportedModelList,
    CredentialUnavailable,
    InvalidEndpoint,
    TransportFailed,
    UpstreamRejected,
    RateLimited,
    QuotaUnavailable,
    ModelListUnavailable,
    InvalidModelList,
    RouteChanged,
}

#[derive(Debug, Serialize)]
struct Probe {
    http_status: Option<u16>,
    retry_after_seconds: Option<u64>,
    elapsed_ms: u64,
    models: Option<Vec<String>>,
    failure: Option<Failure>,
}

impl Probe {
    fn failed(failure: Failure) -> Self {
        Self {
            http_status: None,
            retry_after_seconds: None,
            elapsed_ms: 0,
            models: None,
            failure: Some(failure),
        }
    }
}

#[derive(Deserialize)]
struct ModelList {
    data: Vec<ModelEntry>,
    #[serde(default)]
    has_more: bool,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
}

fn models_endpoint(route: &InheritedCodingRoute) -> Result<url::Url, Failure> {
    // Azure deployment discovery is a different API. Do not guess it or send
    // an Azure credential to the OpenAI-compatible discovery path.
    if route.provider == "azure_foundry" {
        return Err(Failure::UnsupportedModelList);
    }
    let mut endpoint = url::Url::parse(&route.base_url).map_err(|_| Failure::InvalidEndpoint)?;
    let loopback = endpoint.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    });
    if !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
        || !(endpoint.scheme() == "https" || (endpoint.scheme() == "http" && loopback))
    {
        return Err(Failure::InvalidEndpoint);
    }
    endpoint.set_path(&format!("{}/models", endpoint.path().trim_end_matches('/')));
    Ok(endpoint)
}

fn decode_models(bytes: &[u8], credential: &str) -> Result<Vec<String>, Failure> {
    let list: ModelList = serde_json::from_slice(bytes).map_err(|_| Failure::InvalidModelList)?;
    if list.has_more || list.data.len() > MAX_MODELS {
        return Err(Failure::InvalidModelList);
    }
    let mut ids = BTreeSet::new();
    for entry in list.data {
        let id = entry.id;
        if id.is_empty()
            || id.len() > 160
            || id.trim() != id
            || id.chars().any(char::is_control)
            || (!credential.is_empty() && id.contains(credential))
        {
            return Err(Failure::InvalidModelList);
        }
        ids.insert(id);
    }
    Ok(ids.into_iter().collect())
}

fn fetch(route: &InheritedCodingRoute, credential: &str, deadline: Duration) -> Probe {
    let endpoint = match models_endpoint(route) {
        Ok(endpoint) => endpoint,
        Err(failure) => return Probe::failed(failure),
    };
    if credential.trim().is_empty()
        || credential.len() > 8192
        || credential.chars().any(char::is_control)
    {
        return Probe::failed(Failure::CredentialUnavailable);
    }
    let started = Instant::now();
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .try_proxy_from_env(false)
        .timeout_connect(deadline)
        .timeout(deadline)
        .build();
    let response = agent
        .get(endpoint.as_str())
        .set("Accept", "application/json")
        .set("User-Agent", "CTOX-Native-Model-Catalog")
        .set("Authorization", &format!("Bearer {credential}"))
        .call();
    let response = match response {
        Ok(response) | Err(ureq::Error::Status(_, response)) => response,
        // Transport errors can carry the private URL. Never format or return
        // them, and never classify a pre-response error as rejected credentials.
        Err(ureq::Error::Transport(_)) => {
            let mut probe = Probe::failed(Failure::TransportFailed);
            probe.elapsed_ms = started.elapsed().as_millis() as u64;
            return probe;
        }
    };
    let status = response.status();
    let retry = response
        .header("Retry-After")
        .and_then(|header| header.parse::<u64>().ok())
        .filter(|seconds| *seconds <= 604_800);
    let mut probe = Probe {
        http_status: Some(status),
        retry_after_seconds: retry,
        elapsed_ms: 0,
        models: None,
        failure: None,
    };
    if status != 200 {
        probe.failure = Some(match status {
            401 | 403 => Failure::UpstreamRejected,
            402 => Failure::QuotaUnavailable,
            429 => Failure::RateLimited,
            _ => Failure::ModelListUnavailable,
        });
    } else {
        let mut bytes = Vec::new();
        let read = response
            .into_reader()
            .take(MAX_BODY + 1)
            .read_to_end(&mut bytes);
        if read.is_err() {
            probe.failure = Some(Failure::TransportFailed);
        } else if bytes.len() as u64 > MAX_BODY {
            probe.failure = Some(Failure::InvalidModelList);
        } else {
            match decode_models(&bytes, credential) {
                Ok(models) => probe.models = Some(models),
                Err(failure) => probe.failure = Some(failure),
            }
        }
    }
    probe.elapsed_ms = started.elapsed().as_millis() as u64;
    probe
}

fn read_credential(root: &Path, route: &InheritedCodingRoute) -> Option<Zeroizing<String>> {
    runtime_env::load_runtime_env_map(root)
        .ok()?
        .remove(route.credential_key)
        .filter(|credential| !credential.trim().is_empty())
        .map(Zeroizing::new)
}

fn retain_current_result(
    probe: &mut Probe,
    route: &InheritedCodingRoute,
    credential: &str,
    current_route: Option<&InheritedCodingRoute>,
    current_credential: Option<&str>,
) {
    if current_route != Some(route) || current_credential != Some(credential) {
        probe.failure = Some(Failure::RouteChanged);
        probe.models = None;
        probe.http_status = None;
        probe.retry_after_seconds = None;
    }
}

/// A real, bounded GET /models observation. No endpoint, selected model,
/// credential selector or secret is represented in this metadata value.
#[derive(Serialize)]
pub(crate) struct NativeModelCatalogObservation {
    pub provider: String,
    pub checked_at_ms: i64,
    pub models: Option<Vec<String>>,
    pub http_status: Option<u16>,
    pub elapsed_ms: u64,
    pub retry_after_seconds: Option<u64>,
    pub failure: Option<String>,
    #[serde(skip)]
    pub private_binding: Option<String>,
    // Holder-local adoption hint, only after current live discovery confirms
    // the configured model. Do not serialize an unvalidated runtime choice.
    #[serde(skip)]
    pub inherited_selected_model: Option<String>,
}

fn probe_current(root: &Path, route: &InheritedCodingRoute) -> (Probe, Option<String>) {
    let Some(credential) = read_credential(root, route) else {
        return (Probe::failed(Failure::CredentialUnavailable), None);
    };
    // Recheck the exact endpoint/key pair BEFORE disclosing the captured key.
    // A credential read must not combine an old endpoint with a changed route.
    let current_route = resolve_inherited_catalog_route(root).ok();
    let current_credential = current_route
        .as_ref()
        .and_then(|route| read_credential(root, route));
    if current_route.as_ref() != Some(route)
        || current_credential.as_ref().map(|secret| secret.as_str()) != Some(credential.as_str())
    {
        return (Probe::failed(Failure::RouteChanged), None);
    }
    let mut probe = fetch(route, &credential, DEADLINE);
    let current_route = resolve_inherited_catalog_route(root).ok();
    let current_credential = current_route
        .as_ref()
        .and_then(|route| read_credential(root, route));
    retain_current_result(
        &mut probe,
        route,
        &credential,
        current_route.as_ref(),
        current_credential.as_ref().map(|secret| secret.as_str()),
    );
    let binding =
        (probe.failure != Some(Failure::RouteChanged)).then(|| private_binding(route, &credential));
    (probe, binding)
}

/// Called only after native Owner/Admin command admission. The observer reads
/// the actual route and rechecks its private credential after the network wait.
/// This is metadata, never authorization for inference or a capacity check.
pub(super) fn observe(root: &Path) -> anyhow::Result<NativeModelCatalogObservation> {
    let route = resolve_inherited_catalog_route(root)
        .map_err(|_| anyhow::anyhow!("native model catalog route is unavailable"))?;
    let (probe, binding) = probe_current(root, &route);
    let failure = serde_json::to_value(probe.failure)?
        .as_str()
        .map(str::to_owned);
    let inherited_selected_model = (probe.failure.is_none()
        && probe.http_status == Some(200)
        && probe
            .models
            .as_ref()
            .is_some_and(|models| models.contains(&route.model_id)))
    .then(|| route.model_id.clone());
    Ok(NativeModelCatalogObservation {
        provider: route.provider,
        checked_at_ms: chrono::Utc::now().timestamp_millis(),
        models: probe.models,
        http_status: probe.http_status,
        elapsed_ms: probe.elapsed_ms,
        retry_after_seconds: probe.retry_after_seconds,
        failure,
        private_binding: binding,
        inherited_selected_model,
    })
}

/// Only the trusted local operator CLI can request this network observation.
/// It uses Pi's stored provider/model/secret selection, with the configured
/// upstream endpoint for discovery instead of the internal inference edge.
/// An observation never authorizes a later turn or certifies capacity.
pub(super) fn inspect(root: &Path) -> anyhow::Result<Value> {
    let route = resolve_inherited_catalog_route(root)?;
    let (probe, _private_binding) = probe_current(root, &route);
    let selected_model_listed = probe
        .models
        .as_ref()
        .map(|models| models.contains(&route.model_id));
    let ok = probe.failure.is_none();
    // Serialize only the selected public fields; no provider body, paths,
    // credential selector, credentials or request headers escape this owner.
    let result = serde_json::json!({
        "ok": ok,
        "schema": "ctox.coding.main-route-models.v1",
        "provider": route.provider,
        "model": route.model_id,
        "upstream_origin": url::Url::parse(&route.base_url)?.origin().ascii_serialization(),
        "wire_api": route.api,
        "selected_model_listed": selected_model_listed,
        "capacity_verified": false,
        "probe": probe,
    });
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_http::{Header, Response, Server, StatusCode};

    fn route(base: &str) -> InheritedCodingRoute {
        InheritedCodingRoute {
            provider: "ctox_proxy".to_owned(),
            model_id: "MiniMax-M3".to_owned(),
            base_url: base.to_owned(),
            credential_key: "CTOX_LLM_PROXY_API_KEY",
            api: "openai-responses",
        }
    }

    #[test]
    fn native_catalog_observer_reads_the_stored_route_and_never_exports_private_state(
    ) -> anyhow::Result<()> {
        use std::collections::BTreeMap;
        let root = tempfile::tempdir()?;
        let server = Server::http("127.0.0.1:0").unwrap();
        let address = format!("http://{}/v1", server.server_addr());
        // Runtime state classifies custom loopback upstreams as OpenAI.
        // Exercise that actual persisted credential selector, not a guessed proxy key.
        let settings = BTreeMap::from([
            ("CTOX_CHAT_SOURCE".to_owned(), "api".to_owned()),
            ("CTOX_API_PROVIDER".to_owned(), "openai".to_owned()),
            ("CTOX_CHAT_MODEL".to_owned(), "MiniMax-M3".to_owned()),
            ("CTOX_CHAT_MODEL_BASE".to_owned(), "MiniMax-M3".to_owned()),
            ("CTOX_UPSTREAM_BASE_URL".to_owned(), address.clone()),
            (
                "OPENAI_API_KEY".to_owned(),
                "fixture-private-proxy".to_owned(),
            ),
        ]);
        runtime_env::save_runtime_env_map(root.path(), &settings)?;
        let before = runtime_env::load_runtime_env_map(root.path())?;
        // Assert the selected private endpoint before any network IO. A
        // fixture must never silently probe a live or shared gateway.
        assert_eq!(
            resolve_inherited_catalog_route(root.path())?.base_url,
            address
        );
        let worker = std::thread::spawn(move || {
            let request = server
                .recv_timeout(Duration::from_secs(60))
                .unwrap()
                .expect("native preflight must reach the isolated upstream");
            assert_eq!(request.url(), "/v1/models");
            assert!(request
                .headers()
                .iter()
                .any(|header| header.field.equiv("Authorization")
                    && header.value.as_str() == "Bearer fixture-private-proxy"));
            request
                .respond(Response::from_string(
                    r#"{"data":[{"id":"MiniMax-M3","private":"fixture-private-proxy"}]}"#,
                ))
                .unwrap();
        });
        let observation = observe(root.path())?;
        worker.join().unwrap();
        assert_eq!(observation.provider, "openai");
        assert_eq!(
            observation.models.as_deref(),
            Some(&["MiniMax-M3".to_owned()][..])
        );
        assert_eq!(observation.http_status, Some(200));
        assert!(observation.failure.is_none());
        assert_eq!(
            observation.inherited_selected_model.as_deref(),
            Some("MiniMax-M3")
        );
        let public = serde_json::to_string(&observation)?;
        assert!(!public.contains("inherited_selected_model"));
        for private in ["fixture-private-proxy", "OPENAI_API_KEY", address.as_str()] {
            assert!(!public.contains(private));
        }
        assert_eq!(runtime_env::load_runtime_env_map(root.path())?, before);
        assert!(!root.path().join("coding-agents").exists());
        Ok(())
    }

    #[test]
    fn native_catalog_refuses_a_changed_private_route_before_network_io() -> anyhow::Result<()> {
        use std::collections::BTreeMap;
        let root = tempfile::tempdir()?;
        let old_server = Server::http("127.0.0.1:0").unwrap();
        let new_server = Server::http("127.0.0.1:0").unwrap();
        let mut settings = BTreeMap::from([
            ("CTOX_CHAT_SOURCE".to_owned(), "api".to_owned()),
            ("CTOX_API_PROVIDER".to_owned(), "openai".to_owned()),
            ("CTOX_CHAT_MODEL".to_owned(), "MiniMax-M3".to_owned()),
            ("CTOX_CHAT_MODEL_BASE".to_owned(), "MiniMax-M3".to_owned()),
            (
                "CTOX_UPSTREAM_BASE_URL".to_owned(),
                format!("http://{}/v1", old_server.server_addr()),
            ),
            (
                "OPENAI_API_KEY".to_owned(),
                "fixture-old-private".to_owned(),
            ),
        ]);
        runtime_env::save_runtime_env_map(root.path(), &settings)?;
        // Warm the inference cache before changing the endpoint/key pair.
        // Discovery must not combine its retired endpoint with the new key.
        crate::execution::models::runtime_kernel::InferenceRuntimeKernel::resolve(root.path())?;
        let captured = resolve_inherited_catalog_route(root.path())?;
        settings.insert(
            "CTOX_UPSTREAM_BASE_URL".into(),
            format!("http://{}/v1", new_server.server_addr()),
        );
        settings.insert("OPENAI_API_KEY".into(), "fixture-new-private".into());
        runtime_env::save_runtime_env_map(root.path(), &settings)?;
        let before_probe = runtime_env::load_runtime_env_map(root.path())?;
        let (probe, binding) = probe_current(root.path(), &captured);
        assert_eq!(probe.failure, Some(Failure::RouteChanged));
        assert!(probe.http_status.is_none());
        assert!(probe.models.is_none());
        assert!(binding.is_none());
        assert!(old_server.try_recv()?.is_none());
        assert!(new_server.try_recv()?.is_none());
        assert_eq!(
            runtime_env::load_runtime_env_map(root.path())?,
            before_probe
        );
        Ok(())
    }

    #[test]
    fn inherited_account_metadata_reads_only_the_native_store_and_keeps_configuration(
    ) -> anyhow::Result<()> {
        use std::collections::BTreeMap;
        let root = tempfile::tempdir()?;
        let settings = BTreeMap::from([
            ("CTOX_CHAT_SOURCE".to_owned(), "api".to_owned()),
            ("CTOX_API_PROVIDER".to_owned(), "ctox_proxy".to_owned()),
            ("CTOX_CHAT_MODEL".to_owned(), "MiniMax-M3".to_owned()),
            ("CTOX_CHAT_MODEL_BASE".to_owned(), "MiniMax-M3".to_owned()),
            (
                "CTOX_UPSTREAM_BASE_URL".to_owned(),
                "https://llm.ctox.dev".to_owned(),
            ),
            (
                "CTOX_LLM_PROXY_API_KEY".to_owned(),
                "fixture-private-proxy".to_owned(),
            ),
            (
                "MINIMAX_API_KEY".to_owned(),
                "fixture-unrelated-private".to_owned(),
            ),
        ]);
        runtime_env::save_runtime_env_map(root.path(), &settings)?;
        let before = runtime_env::load_runtime_env_map(root.path())?;
        let metadata = account_metadata(root.path())?.unwrap();
        assert_eq!(metadata.provider, "ctox_proxy");
        let mut rotated = before.clone();
        rotated.insert(
            "CTOX_LLM_PROXY_API_KEY".into(),
            "fixture-rotated-proxy".into(),
        );
        runtime_env::save_runtime_env_map(root.path(), &rotated)?;
        let changed = account_metadata(root.path())?.unwrap();
        assert_ne!(metadata.private_binding, changed.private_binding);
        assert_eq!(runtime_env::load_runtime_env_map(root.path())?, rotated);
        runtime_env::save_runtime_env_map(root.path(), &before)?;
        assert_eq!(runtime_env::load_runtime_env_map(root.path())?, before);
        assert!(
            !root.path().join("coding-agents").exists(),
            "metadata must not prepare or start Pi"
        );
        Ok(())
    }

    #[test]
    fn inherited_account_metadata_requires_an_unchanged_private_snapshot() {
        let original = route("https://llm.ctox.dev/v1");
        let metadata = native_metadata_from_current(
            &original,
            Some("fixture-secret"),
            Some(&original),
            Some("fixture-secret"),
        )
        .unwrap()
        .unwrap();
        assert_eq!(metadata.provider, "ctox_proxy");
        let changed = route("https://llm.ctox.dev/v2");
        for (current, credential) in [
            (Some(&changed), Some("fixture-secret")),
            (Some(&original), Some("other-private")),
            (None, None),
        ] {
            let error = native_metadata_from_current(
                &original,
                Some("fixture-secret"),
                current,
                credential,
            )
            .err()
            .unwrap()
            .to_string();
            assert_eq!(error, "native main account configuration changed");
            assert!(!error.contains("fixture-secret"));
            assert!(!error.contains("other-private"));
        }
    }

    #[test]
    fn inherited_account_metadata_does_not_invent_an_account_without_credentials() {
        let original = route("https://llm.ctox.dev/v1");
        let oversized = "x".repeat(8193);
        for credential in [
            None,
            Some(""),
            Some(" "),
            Some("invalid\nkey"),
            Some(oversized.as_str()),
        ] {
            assert!(native_metadata_from_current(
                &original,
                credential,
                Some(&original),
                credential,
            )
            .unwrap()
            .is_none());
        }
    }

    #[test]
    fn live_models_decode_only_complete_bounded_ids() {
        let body = br#"{"data":[{"id":"MiniMax-M3","private":"ignored"},{"id":"MiniMax-M3"}]}"#;
        assert_eq!(
            decode_models(body, "fixture-secret").unwrap(),
            ["MiniMax-M3"]
        );
        assert!(decode_models(br#"{"data":[],"has_more":true}"#, "").is_err());
        assert!(decode_models(br#"{"data":[{"id":" MiniMax-M3"}]}"#, "").is_err());
        assert!(decode_models(br#"{"data":[{"id":"\u0000"}]}"#, "").is_err());
        assert!(decode_models(br#"{"data":[{"id":"fixture-secret"}]}"#, "fixture-secret").is_err());
        let oversized = serde_json::json!({"data": (0..=MAX_MODELS).map(|_| serde_json::json!({"id": "MiniMax-M3"})).collect::<Vec<_>>()});
        assert!(decode_models(&serde_json::to_vec(&oversized).unwrap(), "").is_err());
    }

    #[test]
    fn live_models_uses_existing_private_route_and_bearer() {
        let server = Server::http("127.0.0.1:0").unwrap();
        let address = format!("http://{}/v1", server.server_addr());
        let worker = std::thread::spawn(move || {
            let request = server
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .unwrap();
            assert_eq!(request.method().as_str(), "GET");
            assert_eq!(request.url(), "/v1/models");
            assert!(request
                .headers()
                .iter()
                .any(|header| header.field.equiv("Authorization")
                    && header.value.as_str() == "Bearer fixture-secret"));
            request
                .respond(Response::from_string(
                    r#"{"data":[{"id":"MiniMax-M3","secret":"fixture-secret"}]}"#,
                ))
                .unwrap();
        });
        let probe = fetch(&route(&address), "fixture-secret", Duration::from_secs(2));
        worker.join().unwrap();
        assert_eq!(probe.http_status, Some(200));
        assert_eq!(probe.models.unwrap(), ["MiniMax-M3"]);
    }

    #[test]
    fn live_models_http_failure_does_not_echo_body_or_redirect() {
        for status in [302, 401, 403, 402, 429, 404, 503] {
            let server = Server::http("127.0.0.1:0").unwrap();
            let address = format!("http://{}/v1", server.server_addr());
            let worker = std::thread::spawn(move || {
                let request = server
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap()
                    .unwrap();
                request
                    .respond(
                        Response::from_string("fixture-secret upstream-private")
                            .with_status_code(StatusCode(status))
                            .with_header(Header::from_bytes("Retry-After", "120").unwrap())
                            .with_header(
                                Header::from_bytes("Location", "https://example.invalid/private")
                                    .unwrap(),
                            ),
                    )
                    .unwrap();
            });
            let probe = fetch(&route(&address), "fixture-secret", Duration::from_secs(2));
            worker.join().unwrap();
            assert_eq!(probe.http_status, Some(status));
            assert_eq!(probe.retry_after_seconds, Some(120));
            assert!(probe.models.is_none());
            assert!(probe.failure.is_some());
            let public = serde_json::to_string(&probe).unwrap();
            assert!(!public.contains("fixture-secret"));
            assert!(!public.contains("upstream-private"));
        }
    }

    #[test]
    fn live_models_rejects_bad_or_oversized_success_body() {
        for body in ["not-json".to_owned(), " ".repeat(MAX_BODY as usize + 1)] {
            let server = Server::http("127.0.0.1:0").unwrap();
            let address = format!("http://{}/v1", server.server_addr());
            let worker = std::thread::spawn(move || {
                let request = server
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap()
                    .unwrap();
                let _ = request.respond(Response::from_string(body));
            });
            let probe = fetch(&route(&address), "fixture-secret", Duration::from_secs(2));
            worker.join().unwrap();
            assert_eq!(probe.failure, Some(Failure::InvalidModelList));
            assert!(probe.models.is_none());
        }
    }

    #[test]
    fn live_models_network_failure_is_not_credentials_rejected() {
        let server = Server::http("127.0.0.1:0").unwrap();
        let address = format!("http://{}/v1", server.server_addr());
        let worker = std::thread::spawn(move || {
            let request = server
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .unwrap();
            std::thread::sleep(Duration::from_millis(800));
            let _ = request.respond(Response::empty(200));
        });
        let probe = fetch(
            &route(&address),
            "fixture-secret",
            Duration::from_millis(200),
        );
        worker.join().unwrap();
        assert_eq!(probe.http_status, None);
        assert_eq!(probe.failure, Some(Failure::TransportFailed));
    }

    #[test]
    fn live_models_refuses_unsafe_endpoint_and_unsupported_discovery() {
        for base in [
            "http://example.com/v1",
            "https://example.com/v1?key=secret",
            "https://user:secret@example.com/v1",
            "https://example.com/v1#secret",
        ] {
            assert_eq!(models_endpoint(&route(base)), Err(Failure::InvalidEndpoint));
        }
        assert_eq!(
            models_endpoint(&route("https://llm.ctox.dev/v1"))
                .unwrap()
                .as_str(),
            "https://llm.ctox.dev/v1/models"
        );
        assert!(models_endpoint(&route("http://[::1]:12345/v1")).is_ok());
        let mut azure = route("https://example.com/v1");
        azure.provider = "azure_foundry".to_owned();
        assert_eq!(models_endpoint(&azure), Err(Failure::UnsupportedModelList));
    }

    #[test]
    fn live_models_discards_result_on_route_or_credential_change() {
        let original = route("https://llm.ctox.dev/v1");
        let mut changed = route("https://llm.ctox.dev/v1");
        changed.model_id = "MiniMax-M2.7".to_owned();
        for (current, secret) in [
            (Some(&changed), Some("same")),
            (Some(&original), Some("rotated")),
            (None, None),
        ] {
            let mut probe = Probe {
                http_status: Some(200),
                retry_after_seconds: None,
                elapsed_ms: 10,
                models: Some(vec!["MiniMax-M3".to_owned()]),
                failure: None,
            };
            retain_current_result(&mut probe, &original, "same", current, secret);
            assert_eq!(probe.failure, Some(Failure::RouteChanged));
            assert!(probe.models.is_none());
            assert!(probe.http_status.is_none());
        }
    }
}
