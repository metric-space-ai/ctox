//! Live-test helpers: result-shape assertions, one retry on transient network
//! errors, redirect hints. Kept apart from the flow in `mod.rs`.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::http::HttpTransport;

/// What a passing probe result must look like. Without `expect` a probe still
/// fails on an empty result (`null`, `""`, `[]`, `{}`, `{"text": ""}`), so an
/// empty body cannot pass silently; set `allow_empty` when empty is the answer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expect {
    #[serde(default)]
    pub allow_empty: bool,
    /// `array`, `object`, `string`, `number` or `boolean`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// JSON pointers that must resolve to a non-null value.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pointers: Vec<String>,
    /// Minimum length of the result array (or of the array at the first pointer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_items: Option<usize>,
}

pub fn is_empty_result(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.trim().is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(m) => m.is_empty() || (m.len() == 1 && m.get("text").and_then(Value::as_str).is_some_and(|t| t.trim().is_empty())),
        _ => false,
    }
}

fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// `Err(reason)` when `data` does not match the expectation.
pub fn check(data: &Value, expect: Option<&Expect>) -> Result<(), String> {
    let e = expect.cloned().unwrap_or_default();
    if is_empty_result(data) {
        return if e.allow_empty { Ok(()) } else { Err("empty result (set expect.allow_empty if empty is a valid answer)".into()) };
    }
    if let Some(k) = &e.kind {
        if kind_of(data) != k {
            return Err(format!("expected a{} {k}, got {}", if k.starts_with(['a', 'o']) { "n" } else { "" }, kind_of(data)));
        }
    }
    for p in &e.pointers {
        if data.pointer(p).is_none_or(Value::is_null) {
            return Err(format!("result has no value at `{p}`"));
        }
    }
    if let Some(min) = e.min_items {
        let list = e.pointers.first().and_then(|p| data.pointer(p)).unwrap_or(data);
        let n = list.as_array().map_or(0, Vec::len);
        if n < min {
            return Err(format!("expected at least {min} items, got {n}"));
        }
    }
    Ok(())
}

/// Wait before the single live-test retry after HTTP 429. Providers often send
/// no `Retry-After` (Spaceflight News: burst limit of about 4 requests/s).
pub const RATE_LIMIT_BACKOFF: std::time::Duration = std::time::Duration::from_secs(2);

/// Network/TLS hiccups worth one retry; HTTP answers and policy refusals are not.
pub fn is_transient(err: &str) -> bool {
    let e = err.to_ascii_lowercase();
    if e.contains("private or reserved") || e.contains("not allowed") || e.contains("egress") {
        return false;
    }
    ["transport error", "network error", "tls", "timed out", "timeout", "connection reset", "connection refused", "broken pipe", "unexpected eof", "peer closed"]
        .iter()
        .any(|n| e.contains(n))
}

/// A fake-IP egress network (DNS answers in 198.18.0.0/15) makes every spec
/// call fail with this; the hint names the operator command.
pub fn egress_hint(err: &str) -> Option<String> {
    err.contains("private or reserved").then(|| {
        "the host resolved to a private/reserved address. If this network answers DNS with 198.18.0.0/15 fake IPs (egress proxy), the operator enables it once: `ctox toolhub config set fake-ip-egress on`".to_string()
    })
}

/// Redirect of `url`, as a suggestion: `{status, location, suggested_host, suggested_api_base}`.
/// `suffix` is the part of `url` after the spec's `api_base` (stripped from the
/// suggestion so it names the new base).
pub fn redirect_hint(transport: &dyn HttpTransport, url: &str, suffix: &str) -> Option<Value> {
    let (status, location) = transport.redirect_of(url).ok().flatten()?;
    let parsed = url::Url::parse(&location).ok()?;
    let host = parsed.host_str()?.to_string();
    let mut base = location.split(['?', '#']).next().unwrap_or(&location).to_string();
    let suffix = suffix.split(['?', '#']).next().unwrap_or("");
    if !suffix.is_empty() && base.ends_with(suffix) {
        base.truncate(base.len() - suffix.len());
    }
    let base = base.trim_end_matches('/').to_string();
    Some(json!({"url": url, "status": status, "location": location, "suggested_host": host, "suggested_api_base": base,
        "hint": format!("{url} answers {status} -> {location}; redirects are never followed, so point api_base at {base} and allowed_hosts at {host}")}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_results_fail_unless_allowed() {
        for v in [json!(null), json!(""), json!([]), json!({}), json!({"text": ""})] {
            assert!(check(&v, None).is_err(), "{v}");
            assert!(check(&v, Some(&Expect { allow_empty: true, ..Default::default() })).is_ok());
        }
        assert!(check(&json!({"public_holiday": false}), None).is_ok());
        assert!(check(&json!(false), None).is_ok());
    }

    #[test]
    fn kind_pointers_and_min_items() {
        let d = json!({"results": [{"id": 1}], "count": 1});
        assert!(check(&d, Some(&Expect { kind: Some("array".into()), ..Default::default() })).unwrap_err().contains("array"));
        assert!(check(&d, Some(&Expect { pointers: vec!["/results/0/id".into()], ..Default::default() })).is_ok());
        assert!(check(&d, Some(&Expect { pointers: vec!["/next".into()], ..Default::default() })).is_err());
        assert!(check(&d, Some(&Expect { pointers: vec!["/results".into()], min_items: Some(2), ..Default::default() })).is_err());
        assert!(check(&json!([1, 2]), Some(&Expect { kind: Some("array".into()), min_items: Some(2), ..Default::default() })).is_ok());
    }

    #[test]
    fn transient_errors_are_network_only() {
        assert!(is_transient("http transport error: Network Error: peer closed connection without sending TLS close_notify"));
        assert!(is_transient("http transport error: timed out reading response"));
        assert!(!is_transient("http transport error: Dns Failed: host resolves only to private or reserved addresses"));
        assert!(!is_transient("provider returned HTTP 500"));
        assert!(egress_hint("resolves only to private or reserved addresses").unwrap().contains("fake-ip-egress on"));
    }
}
