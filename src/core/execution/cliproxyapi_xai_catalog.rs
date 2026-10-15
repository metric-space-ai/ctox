// Origin: CTOX
// License: AGPL-3.0-only
//! Exact native Grok Build catalog. Read-only credentials, no account refresh.
use super::*;
use crate::coding_agents::pi_sidecar::NativeModelCatalogObservation;
use std::{collections::BTreeSet, io::Read, time::Instant};
struct Captured {
    generation: String,
    record: Stored,
}
fn capture(root: &Path, id: &str) -> anyhow::Result<Option<Captured>> {
    if id != ACCOUNT_ID {
        return Ok(None);
    }
    let cfg = crate::execution::cliproxyapi_integration::load_provider_integration_config(root)?;
    if cfg.config.api_key_accounts.iter().any(|a| a.id == id) {
        return Ok(None);
    }
    let Some(generation) = credential_binding(root)? else {
        return Ok(None);
    };
    let encoded = Zeroizing::new(crate::secrets::read_secret_value(root, SCOPE, NAME)?);
    let record: Stored = serde_json::from_str(&encoded)
        .map_err(|_| anyhow::anyhow!("native Grok credential unavailable"))?;
    if record.access.trim().is_empty()
        || record.access.len() > 65_536
        || record.access.chars().any(char::is_control)
    {
        return Ok(None);
    }
    if credential_binding(root)?.as_deref() != Some(generation.as_str()) {
        return Ok(None);
    }
    Ok(Some(Captured { generation, record }))
}
fn binding(c: &Captured) -> String {
    format!("xai-oauth:v1:{}", c.generation)
}
pub(crate) fn model_binding(root: &Path, id: &str) -> anyhow::Result<Option<String>> {
    let Some(c) = capture(root, id)? else {
        return Ok(None);
    };
    if capture(root, id)?.as_ref().map(|a| a.generation.as_str()) != Some(c.generation.as_str()) {
        return Ok(None);
    }
    Ok(Some(binding(&c)))
}
fn failed(reason: &str) -> NativeModelCatalogObservation {
    NativeModelCatalogObservation {
        provider: "xai".into(),
        checked_at_ms: chrono::Utc::now().timestamp_millis(),
        models: None,
        http_status: None,
        elapsed_ms: 0,
        retry_after_seconds: None,
        failure: Some(reason.into()),
        private_binding: None,
        inherited_selected_model: None,
    }
}
fn decode(body: &[u8], record: &Stored) -> Option<Vec<String>> {
    let data: serde_json::Value = serde_json::from_slice(body).ok()?;
    let rows = data.get("data")?.as_array()?;
    if rows.is_empty() || rows.len() > 1024 {
        return None;
    }
    let mut ids = BTreeSet::new();
    for row in rows {
        let id = row.get("id")?.as_str()?;
        if id.is_empty()
            || id.len() > 160
            || id.trim() != id
            || id.chars().any(char::is_control)
            || [
                Some(record.access.as_str()),
                record.refresh.as_deref(),
                record.identity.as_deref(),
            ]
            .into_iter()
            .flatten()
            .any(|s| !s.is_empty() && id.contains(s))
        {
            return None;
        }
        ids.insert(id.to_owned());
    }
    Some(ids.into_iter().collect())
}
fn fetch(root: &Path, c: &Captured) -> NativeModelCatalogObservation {
    use ctox_cliproxyapi::{
        internal::runtime::executor::xai_executor_request::apply_xai_chat_headers,
        sdk::cliproxy::auth::Auth,
    };
    let started = Instant::now();
    if c.record.expires_at.is_some_and(|expires| {
        expires
            <= std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .saturating_add(30)
    }) {
        return failed("credential_refresh_required");
    }
    let mut auth = Auth::default();
    auth.attributes.insert("auth_kind".into(), "oauth".into());
    auth.attributes
        .insert("base_url".into(), CLI_CHAT_PROXY_BASE_URL.into());
    let mut headers = std::collections::BTreeMap::new();
    apply_xai_chat_headers(&mut headers, Some(&auth), &c.record.access, false, "");
    let timeout = Duration::from_secs(8);
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .try_proxy_from_env(false)
        .timeout_connect(timeout)
        .timeout(timeout)
        .build();
    let url = format!("{}/models", endpoint(root));
    let mut request = agent.get(&url);
    for (k, values) in &headers {
        for value in values {
            request = request.set(k, value);
        }
    }
    let response = match request.call() {
        Ok(v) | Err(ureq::Error::Status(_, v)) => v,
        Err(ureq::Error::Transport(_)) => {
            let mut r = failed("transport_failed");
            r.elapsed_ms = started.elapsed().as_millis() as u64;
            return r;
        }
    };
    let status = response.status();
    let mut r = failed(match status {
        200 => "invalid_model_list",
        401 | 403 => "upstream_rejected",
        402 => "quota_unavailable",
        429 => "rate_limited",
        _ => "model_list_unavailable",
    });
    r.http_status = Some(status);
    r.retry_after_seconds = response
        .header("Retry-After")
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|n| *n <= 604800);
    if status == 200 {
        let mut body = Vec::new();
        match response.into_reader().take(131073).read_to_end(&mut body) {
            Ok(_) if body.len() <= 131072 => {
                r.models = decode(&body, &c.record);
                if r.models.is_some() {
                    r.failure = None;
                }
            }
            Err(_) => r.failure = Some("transport_failed".into()),
            _ => {}
        }
    }
    r.elapsed_ms = started.elapsed().as_millis() as u64;
    r
}
fn observe_with(
    mut read: impl FnMut() -> anyhow::Result<Option<Captured>>,
    fetch: impl FnOnce(&Captured) -> NativeModelCatalogObservation,
) -> anyhow::Result<NativeModelCatalogObservation> {
    let Some(first) = read()? else {
        return Ok(failed("account_unavailable"));
    };
    if read()?.as_ref().map(|c| c.generation.as_str()) != Some(first.generation.as_str()) {
        return Ok(failed("account_changed"));
    }
    let mut r = fetch(&first);
    if read()?.as_ref().map(|c| c.generation.as_str()) != Some(first.generation.as_str()) {
        return Ok(failed("account_changed"));
    }
    r.private_binding = Some(binding(&first));
    Ok(r)
}
pub(crate) fn observe_model_catalog(
    root: &Path,
    id: &str,
) -> anyhow::Result<NativeModelCatalogObservation> {
    observe_with(|| capture(root, id), |c| fetch(root, c))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> anyhow::Result<tempfile::TempDir> {
        let root = tempfile::tempdir()?;
        let encoded=serde_json::json!({"access":"fixture-access","refresh":"fixture-refresh","identity":null,"expires_at":null,"token_endpoint":"https://auth.x.ai/token"}).to_string();
        crate::secrets::write_secret_record(
            root.path(),
            SCOPE,
            NAME,
            &encoded,
            None,
            serde_json::json!({}),
        )?;
        Ok(root)
    }
    #[test]
    fn xai_catalog_expiry_is_a_local_refresh_requirement_not_an_upstream_denial(
    ) -> anyhow::Result<()> {
        let root = fixture()?;
        let mut raw: serde_json::Value = serde_json::from_str(&Zeroizing::new(
            crate::secrets::read_secret_value(root.path(), SCOPE, NAME)?,
        ))?;
        raw["expires_at"] = serde_json::json!(1);
        crate::secrets::write_secret_record(
            root.path(),
            SCOPE,
            NAME,
            &raw.to_string(),
            None,
            serde_json::json!({}),
        )?;
        let r = observe_model_catalog(root.path(), ACCOUNT_ID)?;
        assert_eq!(r.failure.as_deref(), Some("credential_refresh_required"));
        assert!(r.http_status.is_none() && r.models.is_none());
        Ok(())
    }
    #[test]
    fn xai_catalog_binding_tracks_encrypted_generations_and_rejects_foreign_ids(
    ) -> anyhow::Result<()> {
        let root = fixture()?;
        let before = model_binding(root.path(), ACCOUNT_ID)?.unwrap();
        assert!(before.starts_with("xai-oauth:v1:"));
        assert!(model_binding(root.path(), "other-account")?.is_none());
        let raw = Zeroizing::new(crate::secrets::read_secret_value(root.path(), SCOPE, NAME)?);
        crate::secrets::write_secret_record(
            root.path(),
            SCOPE,
            NAME,
            &raw,
            None,
            serde_json::json!({}),
        )?;
        assert_ne!(model_binding(root.path(), ACCOUNT_ID)?.unwrap(), before);
        Ok(())
    }
    #[test]
    fn xai_catalog_generation_changes_before_or_after_io_retire_observations() -> anyhow::Result<()>
    {
        for change_at in [2, 3] {
            let root = fixture()?;
            let mut reads = 0;
            let mut calls = 0;
            let r = observe_with(
                || {
                    reads += 1;
                    let mut c = capture(root.path(), ACCOUNT_ID)?;
                    if reads == change_at {
                        c.as_mut().unwrap().generation.push('x');
                    }
                    Ok(c)
                },
                |_| {
                    calls += 1;
                    let mut r = failed("unused");
                    r.http_status = Some(200);
                    r.models = Some(vec![]);
                    r.failure = None;
                    r
                },
            )?;
            assert_eq!(calls, if change_at == 2 { 0 } else { 1 });
            assert_eq!(r.failure.as_deref(), Some("account_changed"));
            assert!(r.models.is_none() && r.private_binding.is_none());
        }
        Ok(())
    }
    #[test]
    fn xai_catalog_decoder_reuses_existing_fixture_without_accepting_secret_echoes(
    ) -> anyhow::Result<()> {
        let root = fixture()?;
        let c = capture(root.path(), ACCOUNT_ID)?.unwrap();
        let registry: serde_json::Value = serde_json::from_slice(include_bytes!(
            "cliproxyapi/internal/registry/models/models.json"
        ))?;
        let id = registry["xai"][0]["id"].as_str().unwrap();
        let bytes = serde_json::to_vec(&serde_json::json!({"data":[{"id":id},{"id":id}]}))?;
        assert_eq!(decode(&bytes, &c.record), Some(vec![id.to_owned()]));
        for body in [
            serde_json::json!({"data":[]}),
            serde_json::json!({"data":[{"id":"fixture-access"}]}),
            serde_json::json!({"data":[{"id":" invalid "}]}),
        ] {
            assert!(decode(&serde_json::to_vec(&body)?, &c.record).is_none());
        }
        Ok(())
    }
}
