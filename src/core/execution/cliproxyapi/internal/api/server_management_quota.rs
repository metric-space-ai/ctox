// ref: internal/api/handlers/management/plugin_quota.go @ 5fc580b1d72d87a7bcfe5cfb6f479c1aea5625b3
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only
use super::*;
use crate::internal::api::handlers::management::{
    ManagementPluginQuotaError, ManagementPluginQuotaService, ManagementQuotaProbeError,
};
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct QuotaRequest {
    auth_index: Option<String>,
    #[serde(rename = "authIndex")]
    camel: Option<String>,
    #[serde(rename = "AuthIndex")]
    pascal: Option<String>,
    plugin_id: Option<String>,
    provider: Option<String>,
}
impl QuotaRequest {
    fn index(&self) -> Option<&str> {
        [&self.auth_index, &self.camel, &self.pascal]
            .into_iter()
            .flatten()
            .map(|s| s.trim())
            .find(|s| !s.is_empty())
    }
}
enum Route {
    Providers,
    Fetch,
    Reset,
    Plugin(String, bool),
}
fn route(path: &str) -> Option<Route> {
    match path {
        "/v0/management/quota/providers" => Some(Route::Providers),
        "/v0/management/quota/fetch" => Some(Route::Fetch),
        "/v0/management/quota/reset" => Some(Route::Reset),
        _ => {
            let rest = path.strip_prefix("/v0/management/plugins/")?;
            let (id, suffix) = rest.split_once("/quota")?;
            if id.is_empty() || id.contains('/') || !matches!(suffix, "" | "/reset") {
                return None;
            }
            let id = percent_encoding::percent_decode_str(id)
                .decode_utf8()
                .ok()?
                .into_owned();
            Some(Route::Plugin(id, suffix == "/reset"))
        }
    }
}
fn error(error: ManagementPluginQuotaError) -> ManagementHttpResponse {
    use ManagementPluginQuotaError::*;
    let (status, kind, message) = match error {
        AccountNotFound => (404, "account_not_found", "auth not found"),
        CredentialSourceUnavailable => (
            500,
            "credential_source_unavailable",
            "account source unavailable",
        ),
        AccountContextMismatch => (
            500,
            "account_context_mismatch",
            "account context unavailable",
        ),
        Unsupported => (
            501,
            "quota_unsupported",
            "no quota provider or probe available",
        ),
        PluginNotFound => (
            404,
            "quota_plugin_not_found",
            "quota provider not found for plugin",
        ),
        PluginCallFailed => (502, "quota_plugin_failed", "quota plugin call failed"),
        Timeout => (504, "quota_timeout", "quota check timed out"),
        ResetDeclined => (
            502,
            "quota_reset_declined",
            "quota reset rejected by provider",
        ),
        RoutingResetFailed => (500, "routing_reset_failed", "failed to reset routing quota"),
        Probe(ManagementQuotaProbeError::UpstreamStatus(_)) => (
            502,
            "quota_upstream_status",
            "quota probe returned an upstream error",
        ),
        Probe(ManagementQuotaProbeError::Transport(_)) => (
            502,
            "quota_probe_transport",
            "quota probe transport unavailable",
        ),
        Probe(_) => (502, "quota_response_invalid", "invalid quota response"),
    };
    let mut body = serde_json::json!({"error":message,"kind":kind});
    if let Probe(ManagementQuotaProbeError::UpstreamStatus(status)) = error {
        body["upstream_status"] = status.into();
    }
    ManagementHttpResponse::json(status, serde_json::to_vec(&body).unwrap())
}
impl ManagementHandler {
    pub async fn handle_async(
        &self,
        method: &str,
        target: &str,
        headers: &BTreeMap<String, Vec<String>>,
        body: &[u8],
        client_ip: IpAddr,
    ) -> ManagementHttpResponse {
        if route(target.split('?').next().unwrap_or(target)).is_none() {
            return self.handle(method, target, headers, body, client_ip);
        }
        match tokio::time::timeout(
            crate::internal::pluginhost::adapters_quota::QUOTA_CALL_TIMEOUT,
            self.handle_quota_route(method, target, headers, body, client_ip),
        )
        .await
        {
            Ok(response) => response,
            Err(_) => error(ManagementPluginQuotaError::Timeout),
        }
    }

    pub fn attach_plugin_quota_service(
        mut self,
        service: Arc<ManagementPluginQuotaService>,
    ) -> Self {
        self.quota_service = Some(service);
        self
    }
    async fn handle_quota_route(
        &self,
        method: &str,
        target: &str,
        headers: &BTreeMap<String, Vec<String>>,
        body: &[u8],
        client_ip: IpAddr,
    ) -> ManagementHttpResponse {
        let path = target.split('?').next().unwrap_or(target);
        let Some(route) = route(path) else {
            return self.handle(method, target, headers, body, client_ip);
        };
        if let Err(error) = self.authenticator.authenticate(
            &client_ip.to_string(),
            client_ip.is_loopback(),
            management_key(headers),
        ) {
            return ManagementHttpResponse::error(error.status(), &error.message());
        }
        if let Route::Providers = route {
            if method != "GET" {
                return ManagementHttpResponse::error(405, "method not allowed");
            }
            let providers = match &self.quota_service {
                Some(service) => service.providers().await,
                None => vec![],
            };
            return ManagementHttpResponse::json(
                200,
                serde_json::to_vec(&serde_json::json!({"providers":providers})).unwrap(),
            );
        }
        let reset = matches!(route, Route::Reset | Route::Plugin(_, true)) || method == "DELETE";
        let allowed = match route {
            Route::Fetch | Route::Reset => method == "POST",
            Route::Plugin(_, true) => method == "POST",
            Route::Plugin(_, false) => matches!(method, "GET" | "POST" | "DELETE"),
            Route::Providers => false,
        };
        if !allowed {
            return ManagementHttpResponse::error(405, "method not allowed");
        }
        if body.len() > MAX_RESET_QUOTA_BODY_BYTES {
            return ManagementHttpResponse::error(413, "request body is too large");
        }
        let query_index = if matches!(route, Route::Plugin(..)) && (method == "GET" || reset) {
            query_parameter(target, "auth_index")
                .filter(|s| !s.trim().is_empty())
                .or_else(|| query_parameter(target, "authIndex").filter(|s| !s.trim().is_empty()))
        } else {
            None
        };
        let request: QuotaRequest = if query_index.is_some() || method == "GET" {
            Default::default()
        } else {
            if !header_value(headers, "content-type")
                .and_then(|s| s.split(';').next())
                .is_some_and(|s| s.trim().eq_ignore_ascii_case("application/json"))
            {
                return ManagementHttpResponse::error(400, "invalid request body");
            }
            match serde_json::from_slice(body) {
                Ok(request) => request,
                Err(_) => return ManagementHttpResponse::error(400, "invalid request body"),
            }
        };
        let Some(index) = query_index
            .as_deref()
            .map(str::trim)
            .or_else(|| request.index())
        else {
            return ManagementHttpResponse::error(400, "auth_index is required");
        };
        let Some(service) = &self.quota_service else {
            return error(if matches!(route, Route::Plugin(..)) {
                ManagementPluginQuotaError::PluginNotFound
            } else {
                ManagementPluginQuotaError::Unsupported
            });
        };
        let plugin = match &route {
            Route::Plugin(id, _) => Some(id.as_str()),
            _ => request
                .plugin_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty()),
        };
        let provider = request.provider.as_deref().unwrap_or_default();
        if reset {
            match service.reset(index, provider, plugin).await {
                Ok(()) => ManagementHttpResponse::json(
                    200,
                    serde_json::to_vec(&serde_json::json!({"status":"ok","auth_index":index}))
                        .unwrap(),
                ),
                Err(err) => error(err),
            }
        } else {
            match service
                .fetch(index, provider, plugin, matches!(route, Route::Plugin(..)))
                .await
            {
                Ok(result) => match serde_json::to_vec(&result) {
                    Ok(body) => ManagementHttpResponse::json(200, body),
                    Err(_) => ManagementHttpResponse::error(502, "invalid quota response"),
                },
                Err(err) => error(err),
            }
        }
    }
}
