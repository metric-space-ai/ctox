// Origin: CTOX
// License: AGPL-3.0-only
//! Bounded, exact-account live Codex catalog. No token refresh or inference.
use super::binding::{capture, Captured};
use crate::coding_agents::pi_sidecar::NativeModelCatalogObservation;
use std::{
    collections::BTreeSet,
    io::Read,
    path::Path,
    time::{Duration, Instant},
};

fn failed(reason: &str) -> NativeModelCatalogObservation {
    NativeModelCatalogObservation {
        provider: "codex".into(),
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
fn decode(bytes: &[u8], secrets: &[&str]) -> Option<Vec<String>> {
    let raw: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let rows = raw.get("models")?.as_array()?;
    if rows.is_empty() || rows.len() > 1024 {
        return None;
    }
    let mut ids = BTreeSet::new();
    for row in rows {
        let id = row.get("slug")?.as_str()?;
        if id.is_empty()
            || id.len() > 160
            || id.trim() != id
            || id.chars().any(char::is_control)
            || secrets.iter().any(|s| !s.is_empty() && id.contains(s))
        {
            return None;
        }
        ids.insert(id.to_owned());
    }
    Some(ids.into_iter().collect())
}
fn fetch(captured: &Captured) -> NativeModelCatalogObservation {
    let Captured::Codex {
        account,
        credentials,
        ..
    } = captured
    else {
        return failed("account_unavailable");
    };
    if account.disabled {
        return failed("account_disabled");
    }
    // No override, ambient proxy, redirects or inherited auth-file access.
    if account.upstream_base_url.trim_end_matches('/') != "https://chatgpt.com/backend-api/codex"
        || account.proxy_url_secret.is_some()
    {
        return failed("unsupported_model_list_endpoint");
    }
    let access = credentials.access_token().expose_secret();
    if access.trim().is_empty() || access.len() > 8192 || access.chars().any(char::is_control) {
        return failed("credential_unavailable");
    }
    let started = Instant::now();
    let timeout = Duration::from_secs(8);
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .try_proxy_from_env(false)
        .timeout_connect(timeout)
        .timeout(timeout)
        .build();
    // The pinned client version is shared with the existing Codex catalog CLI.
    let version =
        ctox_cliproxyapi::internal::cmd::fetch_codex_models::Options::default().client_version;
    let endpoint = format!("https://chatgpt.com/backend-api/codex/models?client_version={version}");
    let bearer = zeroize::Zeroizing::new(format!("Bearer {access}"));
    let mut request = agent
        .get(&endpoint)
        .set("Accept", "application/json")
        .set("Authorization", &bearer)
        .set("Originator", "codex_cli_rs")
        .set("User-Agent", &format!("codex_cli_rs/{version}"));
    let claims = ctox_cliproxyapi::internal::auth::codex::parse_jwt_token(
        credentials.id_token().expose_secret(),
    )
    .ok();
    if let Some(id) = claims
        .as_ref()
        .map(|v| v.account_id())
        .filter(|id| !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control))
    {
        request = request.set("Chatgpt-Account-Id", id);
    }
    let response = match request.call() {
        Ok(r) | Err(ureq::Error::Status(_, r)) => r,
        Err(ureq::Error::Transport(_)) => {
            let mut r = failed("transport_failed");
            r.elapsed_ms = started.elapsed().as_millis() as u64;
            return r;
        }
    };
    let status = response.status();
    let mut result = failed(match status {
        200 => "invalid_model_list",
        401 | 403 => "upstream_rejected",
        402 => "quota_unavailable",
        429 => "rate_limited",
        _ => "model_list_unavailable",
    });
    result.http_status = Some(status);
    result.retry_after_seconds = response
        .header("Retry-After")
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|v| *v <= 604800);
    if status == 200 {
        let mut bytes = Vec::new();
        match response.into_reader().take(131073).read_to_end(&mut bytes) {
            Ok(_) if bytes.len() <= 131072 => {
                result.models = decode(
                    &bytes,
                    &[
                        access,
                        credentials.id_token().expose_secret(),
                        credentials.refresh_token().expose_secret(),
                    ],
                );
                if result.models.is_some() {
                    result.failure = None;
                }
            }
            Ok(_) => {}
            Err(_) => result.failure = Some("transport_failed".into()),
        }
    }
    result.elapsed_ms = started.elapsed().as_millis() as u64;
    result
}
fn observe_with(
    mut read: impl FnMut() -> anyhow::Result<Option<Captured>>,
    fetch: impl FnOnce(&Captured) -> NativeModelCatalogObservation,
) -> anyhow::Result<NativeModelCatalogObservation> {
    let Some(first) = read()? else {
        return Ok(failed("account_unavailable"));
    };
    if !matches!(&first,Captured::Codex{account,..} if !account.disabled) {
        return Ok(failed("account_disabled"));
    }
    if read()?.as_ref() != Some(&first) {
        return Ok(failed("account_changed"));
    }
    let mut result = fetch(&first);
    if read()?.as_ref() != Some(&first) {
        return Ok(failed("account_changed"));
    }
    result.private_binding = Some(first.fingerprint()?);
    Ok(result)
}
pub(crate) fn observe(root: &Path, id: &str) -> anyhow::Result<NativeModelCatalogObservation> {
    observe_with(|| capture(root, "codex", id), fetch)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::cliproxyapi_codex_model::tests::fixture;
    #[test]
    fn codex_catalog_decode_preserves_existing_wire_fixture_id_and_deduplicates(
    ) -> anyhow::Result<()> {
        // Reuse the existing Codex wire fixture without registering any ID.
        // This proves decoder shape only, never a live catalog or health result.
        let fixture: serde_json::Value = serde_json::from_slice(include_bytes!(
            "cliproxyapi/tests/differential/codex_responses_fixtures.json"
        ))?;
        let id = fixture[0]["model"].as_str().unwrap();
        let bytes = serde_json::to_vec(&serde_json::json!({"models":[{"slug":id},{"slug":id}]}))?;
        assert_eq!(decode(&bytes, &[]), Some(vec![id.to_owned()]));
        Ok(())
    }
    #[test]
    fn codex_catalog_changed_before_io_and_after_io_retire_results() -> anyhow::Result<()> {
        for change_at in [2, 3] {
            let (root, a) = fixture()?;
            let mut reads = 0;
            let mut calls = 0;
            let result = observe_with(
                || {
                    reads += 1;
                    let mut c = capture(root.path(), "codex", &a.id)?;
                    if reads == change_at {
                        if let Some(Captured::Codex { account, .. }) = &mut c {
                            account.priority += 1;
                        }
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
            assert_eq!(result.failure.as_deref(), Some("account_changed"));
            assert!(result.private_binding.is_none());
            assert!(result.models.is_none());
        }
        Ok(())
    }
    #[test]
    fn codex_catalog_decode_rejects_secret_echoes_and_malformed_lists() {
        let id = "credential-echo";
        for bytes in [
            serde_json::json!({"models":[{"slug":id}]}),
            serde_json::json!({"models":[]}),
            serde_json::json!({"models":[{"slug":" invalid "}]}),
        ] {
            assert!(decode(&serde_json::to_vec(&bytes).unwrap(), &[id]).is_none());
        }
    }
    #[test]
    fn codex_catalog_rejects_override_before_any_request() -> anyhow::Result<()> {
        let (root, a) = fixture()?;
        let mut captured = capture(root.path(), "codex", &a.id)?.unwrap();
        if let Captured::Codex { account, .. } = &mut captured {
            account.upstream_base_url = "https://example.invalid".into();
        }
        let result = fetch(&captured);
        assert_eq!(
            result.failure.as_deref(),
            Some("unsupported_model_list_endpoint")
        );
        assert!(result.http_status.is_none());
        Ok(())
    }
}
