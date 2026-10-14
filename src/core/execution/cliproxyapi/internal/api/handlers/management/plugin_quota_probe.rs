// ref: internal/api/handlers/management/plugin_quota.go:394-582 @ 0d18d3b365be000eeaee7de4615f9da596b2059d
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only
use super::{
    normalize_quota_probe, ManagementApiToolError, ManagementApiToolRequest, ManagementApiTools,
    QuotaProbeError,
};
use crate::sdk::pluginapi::QuotaFetchResponse;
use serde_json::{Map, Value};
use std::{collections::BTreeMap, sync::Arc, time::SystemTime};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManagementQuotaProbeError {
    InvalidAccount,
    Transport(ManagementApiToolError),
    UpstreamStatus(u16),
    Shape(QuotaProbeError),
}
/// Executes a server-stored declarative probe for an already-authorized account.
/// The caller resolves the account first. Only the existing host executor may
/// resolve `$TOKEN$`, choose transport/proxy policy, or access credential values.
pub struct ManagementQuotaProbe {
    tools: Arc<ManagementApiTools>,
}
impl ManagementQuotaProbe {
    pub fn new(tools: Arc<ManagementApiTools>) -> Self {
        Self { tools }
    }
    pub fn fetch(
        &self,
        auth_index: &str,
        probe: &Map<String, Value>,
        now: SystemTime,
    ) -> Result<Option<QuotaFetchResponse>, ManagementQuotaProbeError> {
        let url = probe
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        if url.is_empty() {
            return Ok(None);
        }
        let auth_index = auth_index.trim();
        if auth_index.is_empty() {
            return Err(ManagementQuotaProbeError::InvalidAccount);
        }
        let method = probe
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let headers = probe
            .get("header")
            .and_then(Value::as_object)
            .or_else(|| probe.get("headers").and_then(Value::as_object))
            .into_iter()
            .flat_map(|headers| headers.iter())
            .filter_map(|(key, value)| value.as_str().map(|v| (key.clone(), v.to_owned())))
            .collect::<BTreeMap<_, _>>();
        let response = self
            .tools
            .execute(ManagementApiToolRequest {
                auth_index: auth_index.into(),
                method: if method.is_empty() {
                    "GET".into()
                } else {
                    method.into()
                },
                url: url.into(),
                headers,
                body: probe
                    .get("data")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
            })
            .map_err(ManagementQuotaProbeError::Transport)?;
        if !(200..300).contains(&response.status_code) {
            return Err(ManagementQuotaProbeError::UpstreamStatus(
                response.status_code,
            ));
        }
        let offset = response
            .headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("date"))
            .and_then(|(_, values)| values.first())
            .and_then(|value| httpdate::parse_http_date(value).ok())
            .and_then(|date| {
                let signed = match date.duration_since(now) {
                    Ok(delta) => i128::try_from(delta.as_millis()).ok()?,
                    Err(_) => -i128::try_from(now.duration_since(date).ok()?.as_millis()).ok()?,
                };
                i64::try_from(signed).ok()
            })
            .unwrap_or_default();
        normalize_quota_probe(
            response.body.as_bytes(),
            probe.get("mapping").and_then(Value::as_object),
            offset,
        )
        .map(Some)
        .map_err(ManagementQuotaProbeError::Shape)
    }
}
