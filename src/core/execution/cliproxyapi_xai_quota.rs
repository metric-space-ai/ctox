// Origin: CTOX
// License: AGPL-3.0-only
//! Public XAI billing observation; credentials and upstream bodies never leave
//! the gateway. Wire fields follow xai-org/grok-build billing.rs (blob
//! dbdc67562f36969c6548e369435b8d2c42981ce5), not inferred cooldown usage.
use super::*;
use anyhow::Context as _;
use serde_json::{json, Value};
use std::io::Read;

pub(crate) struct Observation {
    pub(crate) binding: Option<String>,
    pub(crate) quota: Value,
}
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}
fn unavailable() -> Value {
    json!({"state":"unavailable","errorClass":"quota_read_failed"})
}
fn decode(bytes: &[u8], now: i64) -> Option<Value> {
    if !(1..=9_007_199_254_740_991).contains(&now) {
        return None;
    }
    let value: Value = serde_json::from_slice(bytes).ok()?;
    let config = value.get("config")?;
    let percent = config.get("creditUsagePercent")?.as_f64()?;
    if !percent.is_finite() || !(0.0..=100.0).contains(&percent) {
        return None;
    }
    let current = config.get("currentPeriod")?;
    let period = match current.get("type")?.as_str()? {
        "USAGE_PERIOD_TYPE_WEEKLY" => "weekly",
        "USAGE_PERIOD_TYPE_MONTHLY" => "monthly",
        _ => return None,
    };
    let reset = chrono::DateTime::parse_from_rfc3339(current.get("end")?.as_str()?)
        .ok()?
        .timestamp_millis();
    if reset <= now || reset > 9_007_199_254_740_991 || reset.saturating_sub(now) > 40 * 86_400_000
    {
        return None;
    }
    Some(
        json!({"state":"reported","period":period,"used_percent":percent,
        "observedAtMs":now,"resetAtMs":reset}),
    )
}

/// Called only by the admitted native account observer, outside its writer tx.
/// No auth refresh, proxy-env routing, redirects, upstream error bodies or logs.
pub(crate) fn observe(root: &Path) -> anyhow::Result<Observation> {
    let binding = credential_binding(root)?;
    let Some(_) = binding else {
        return Ok(Observation {
            binding,
            quota: json!({"state":"unknown"}),
        });
    };
    let fetch = || -> anyhow::Result<Value> {
        let encoded = Zeroizing::new(crate::secrets::read_secret_value(root, SCOPE, NAME)?);
        let record: Stored = serde_json::from_str(&encoded)?;
        let now = now_ms();
        anyhow::ensure!(
            !record.access.is_empty()
                && record.access.len() <= 8192
                && !record.access.chars().any(char::is_control),
            "credential unavailable"
        );
        anyhow::ensure!(
            record.expires_at.is_none_or(|t| t > (now / 1000) as u64),
            "credential expired"
        );
        use ctox_cliproxyapi::internal::runtime::executor::xai_executor_request::apply_xai_chat_headers;
        use ctox_cliproxyapi::sdk::cliproxy::auth::Auth;
        let mut auth = Auth::default();
        auth.attributes.insert("auth_kind".into(), "oauth".into());
        auth.attributes
            .insert("base_url".into(), CLI_CHAT_PROXY_BASE_URL.into());
        let mut headers = std::collections::BTreeMap::new();
        apply_xai_chat_headers(&mut headers, Some(&auth), &record.access, false, "");
        let agent = ureq::AgentBuilder::new()
            .redirects(0)
            .try_proxy_from_env(false)
            .timeout_connect(Duration::from_secs(3))
            .timeout(Duration::from_secs(8))
            .build();
        let url = format!(
            "{}/billing?format=credits",
            endpoint(root).trim_end_matches('/')
        );
        let mut request = agent.get(&url).set("Accept", "application/json");
        for (key, values) in &headers {
            for value in values {
                request = request.set(key, value);
            }
        }
        let (_, user) = parse_jwt_identity(record.identity.as_deref().unwrap_or(&record.access));
        if !user.is_empty() {
            request = request.set("x-userid", &user);
        }
        let response = request
            .call()
            .map_err(|_| anyhow::anyhow!("quota read failed"))?;
        let mut bytes = Vec::new();
        response.into_reader().take(65537).read_to_end(&mut bytes)?;
        anyhow::ensure!(bytes.len() <= 65536, "quota response too large");
        decode(&bytes, now_ms()).context("quota unavailable")
    };
    let quota = fetch().unwrap_or_else(|_| unavailable());
    if credential_binding(root)? != binding {
        return Ok(Observation {
            binding: None,
            quota: unavailable(),
        });
    }
    Ok(Observation { binding, quota })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    #[test]
    fn xai_billing_reports_real_period_and_percent_and_rejects_unknown_usage() -> anyhow::Result<()>
    {
        crate::persistence::test_support::run(move |root| {
            crate::secrets::write_secret_record(
                root.path(),
                SCOPE,
                NAME,
                &serde_json::to_string(&Stored {
                    access: "fixture-access".into(),
                    refresh: None,
                    identity: None,
                    expires_at: None,
                    token_endpoint: "https://auth.x.ai/token".into(),
                })?,
                None,
                json!({"provider":"xai"}),
            )?;
            let listener = TcpListener::bind("127.0.0.1:0")?;
            test_endpoint(
                root.path(),
                Some(format!("http://{}", listener.local_addr()?)),
            );
            let end =
                chrono::DateTime::<chrono::Utc>::from_timestamp_millis(now_ms() + 8 * 86_400_000)
                    .unwrap()
                    .to_rfc3339();
            let server = std::thread::spawn(move || -> anyhow::Result<()> {
                for (period, percent) in [("WEEKLY", 37.25), ("MONTHLY", 0.0), ("WEEKLY", 101.0)] {
                    let (mut stream, _) = listener.accept()?;
                    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
                    let mut bytes = Vec::new();
                    while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                        let mut chunk = [0; 1024];
                        let n = stream.read(&mut chunk)?;
                        anyhow::ensure!(
                            n > 0 && bytes.len() + n <= 16384,
                            "invalid fixture request"
                        );
                        bytes.extend_from_slice(&chunk[..n]);
                    }
                    let text = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
                    assert!(text.starts_with("get /billing?format=credits "));
                    assert!(text.contains("authorization: bearer fixture-access"));
                    assert!(text.contains("x-xai-token-auth: xai-grok-cli"));
                    let body = json!({"config":{"creditUsagePercent":percent,
                    "currentPeriod":{"type":format!("USAGE_PERIOD_TYPE_{period}"),"end":end}},
                    "private_upstream_field":"must-not-be-published"})
                    .to_string();
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )?;
                }
                Ok(())
            });
            let weekly = observe(root.path())?;
            assert_eq!(
                weekly.quota["period"], "weekly",
                "gateway period must be preserved"
            );
            assert_eq!(weekly.quota["used_percent"], 37.25);
            assert!(!weekly.quota.to_string().contains("private_upstream_field"));
            let monthly = observe(root.path())?;
            assert_eq!(monthly.quota["period"], "monthly");
            assert_eq!(monthly.quota["used_percent"], 0.0);
            assert_eq!(observe(root.path())?.quota["state"], "unavailable");
            server.join().unwrap()?;
            test_endpoint(root.path(), None);
            Ok(())
        })
    }
}
