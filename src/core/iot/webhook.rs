// Origin: CTOX
// License: AGPL-3.0-only
//
// RFC 0011 / spec §5 — webhook connector, in & out. The spec is explicit that
// webhooks are first-class I/O:
//   * INBOUND  an external HTTP POST → a signal datapoint. Webhooks are PUSH, so
//     they do NOT fit the pull-loop `IotAgent` model; instead the payload is
//     mapped to an attribute write and flows through the SAME inbound path as a
//     device read (commands::attribute_write → process_attribute_event +
//     conditions + the RFC-0011 widget watchers via tick_widgets_for_signal).
//     A thin `tiny_http` route on the existing service listener calls `ingest`;
//     the CLI `ctox iot webhook ingest` is the same entry, so an external HTTP
//     front (or a test) drives it without a new server.
//   * OUTBOUND an agent action POSTs to a URL as part of fulfilling a "Dann"
//     ("…und meld's per Webhook ans ERP"). `send` resolves an optional auth
//     secret from the secret store (never env vars, never the agent payload) and
//     posts via the in-tree ureq wrapper.
//
// HARD RULES: native Rust; the clock is injected (ts_ms passed in); no std::env
// (secrets via runtime_env::env_or_config → secret store); no browser HTTP
// bridge (this is external-device I/O, not business-os sync).

use crate::iot::commands::{self, AttributeWriteReq};
use crate::iot::Result;
use anyhow::{anyhow, Context};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Canonical signal binding form: `"<asset_id>::<attribute_name>"`.
fn parse_signal_ref(signal_ref: &str) -> Result<(&str, &str)> {
    signal_ref
        .split_once("::")
        .filter(|(a, b)| !a.is_empty() && !b.is_empty())
        .ok_or_else(|| anyhow!("signal_ref must be '<asset_id>::<attribute_name>', got '{signal_ref}'"))
}

/// Extract the value at a dot-path (`"data.temp.value"`) from a JSON payload.
/// An empty/`None` path yields the whole payload (a scalar body).
fn value_at_path<'a>(payload: &'a Value, path: Option<&str>) -> Option<&'a Value> {
    let path = match path {
        Some(p) if !p.trim().is_empty() => p,
        _ => return Some(payload),
    };
    let mut cur = payload;
    for seg in path.split('.').filter(|s| !s.is_empty()) {
        cur = cur.get(seg)?;
    }
    Some(cur)
}

/// Inbound webhook → signal datapoint. Maps the payload to one attribute write
/// and reuses the full inbound path (so conditions AND the widget watchers run).
/// `realm = None` is the trusted ingest (the HTTP front authenticates the caller
/// via the webhook secret before calling this).
pub(crate) fn ingest(
    root: &std::path::Path,
    signal_ref: &str,
    payload: &Value,
    path: Option<&str>,
    ts_ms: i64,
    realm: Option<&str>,
) -> Result<Value> {
    let (asset_id, attribute_name) = parse_signal_ref(signal_ref)?;
    let value = value_at_path(payload, path)
        .cloned()
        .ok_or_else(|| anyhow!("webhook payload has no value at path {:?}", path))?;
    anyhow::ensure!(!value.is_null(), "webhook payload value is null");

    let req = AttributeWriteReq {
        asset_id: asset_id.to_string(),
        name: attribute_name.to_string(),
        value,
        timestamp_ms: ts_ms,
    };
    let outcome = commands::attribute_write(root, req, realm)?;
    Ok(outcome.into_value())
}

/// A built outbound request — separated from the actual send so header/secret
/// assembly is testable without hitting the network.
struct OutboundRequest {
    method: String,
    url: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

fn build_outbound(
    root: &std::path::Path,
    url: &str,
    payload: &Value,
    secret_ref: Option<&str>,
    extra_headers: &[(String, String)],
) -> Result<OutboundRequest> {
    anyhow::ensure!(
        url.starts_with("http://") || url.starts_with("https://"),
        "webhook url must be http(s): {url}"
    );
    let mut headers = BTreeMap::new();
    headers.insert("Content-Type".to_string(), "application/json".to_string());
    // Auth secret resolved from the encrypted secret store — never from the
    // agent payload, never from env vars.
    if let Some(name) = secret_ref.filter(|s| !s.trim().is_empty()) {
        let token = crate::execution::models::runtime_env::env_or_config(root, name)
            .ok_or_else(|| anyhow!("webhook secret '{name}' not found in secret store"))?;
        headers.insert("Authorization".to_string(), format!("Bearer {token}"));
    }
    for (k, v) in extra_headers {
        headers.insert(k.clone(), v.clone());
    }
    Ok(OutboundRequest {
        method: "POST".to_string(),
        url: url.to_string(),
        headers,
        body: serde_json::to_vec(payload).context("failed to serialize webhook body")?,
    })
}

/// Outbound webhook: POST `payload` to `url`, attaching `Authorization: Bearer
/// <secret>` when `secret_ref` resolves in the secret store. The actual POST is a
/// thin call to the in-tree ureq wrapper.
pub(crate) fn send(
    root: &std::path::Path,
    url: &str,
    payload: &Value,
    secret_ref: Option<&str>,
    extra_headers: &[(String, String)],
) -> Result<Value> {
    let req = build_outbound(root, url, payload, secret_ref, extra_headers)?;
    let resp = crate::communication::email_native::http_request(
        &req.method,
        &req.url,
        &req.headers,
        Some(&req.body),
    )
    .with_context(|| format!("webhook POST to {url} failed"))?;
    Ok(json!({
        "url": url,
        "status": resp.status,
        "ok": (200..300).contains(&resp.status),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iot::commands::{AssetUpsertReq, WidgetUpsertReq};
    use crate::iot::{datapoints, store};

    fn make_asset(root: &std::path::Path) {
        commands::asset_upsert(
            root,
            AssetUpsertReq {
                id: Some("asset-1".to_string()),
                realm: "master".to_string(),
                asset_type: "Sensor".to_string(),
                name: "Serverraum".to_string(),
                parent_id: None,
                asset_type_info: None,
            },
            None,
        )
        .unwrap();
    }

    #[test]
    fn value_at_path_navigates_json() {
        let p = json!({ "data": { "temp": 23.5 }, "flat": 7 });
        assert_eq!(value_at_path(&p, Some("data.temp")).unwrap(), &json!(23.5));
        assert_eq!(value_at_path(&p, Some("flat")).unwrap(), &json!(7));
        assert_eq!(value_at_path(&p, None).unwrap(), &p);
        assert!(value_at_path(&p, Some("data.missing")).is_none());
    }

    #[test]
    fn parse_signal_ref_requires_both_halves() {
        assert_eq!(parse_signal_ref("a::b").unwrap(), ("a", "b"));
        assert!(parse_signal_ref("nope").is_err());
    }

    // Inbound webhook records a datapoint on the bound signal (full inbound path).
    #[test]
    fn ingest_records_datapoint_from_payload() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        make_asset(root);

        let payload = json!({ "device": "srv-1", "readings": { "temperature": 31.5 } });
        ingest(root, "asset-1::temperature", &payload, Some("readings.temperature"), 1_000, None).unwrap();

        let conn = store::open_iot_store(root).unwrap();
        let pts = datapoints::all(&conn, "asset-1", "temperature", 0, 10_000).unwrap();
        assert_eq!(pts.len(), 1);
        assert_eq!(pts[0].value.as_numeric(), Some(31.5));
    }

    // End-to-end: an inbound webhook drives a bound widget's watcher → fire.
    #[test]
    fn ingest_fires_bound_widget_watcher() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        make_asset(root);

        // A dashboard + an automation widget on this signal with a watcher.
        commands::dashboard_upsert(
            root,
            commands::DashboardUpsertReq {
                id: Some("d1".to_string()),
                realm: "master".to_string(),
                name: "D".to_string(),
                scope: None,
                scope_ref: None,
                view_mode: None,
                sort_index: None,
            },
            None,
        )
        .unwrap();
        commands::widget_upsert(
            root,
            WidgetUpsertReq {
                id: Some("w1".to_string()),
                dashboard_id: "d1".to_string(),
                realm: "master".to_string(),
                signal_ref: "asset-1::temperature".to_string(),
                cond_text: Some("zu heiß".to_string()),
                action_prompt: Some("Kühlung hoch".to_string()),
                trigger_code: Some(r#"if signal.last() > 30.0 { fire("heiß"); }"#.to_string()),
                render_code: None,
                x: None,
                y: None,
                w: None,
                h: None,
                sort_index: None,
            },
            None,
        )
        .unwrap();

        // Webhook pushes a hot reading → datapoint → watcher fires → widget fired.
        // ts=0 is normalized to now (§2A.2) so it lands inside the watcher's
        // wall-clock lookback window (the tick is anchored at now_ms()).
        let payload = json!({ "temperature": 35.0 });
        ingest(root, "asset-1::temperature", &payload, Some("temperature"), 0, None).unwrap();

        let conn = store::open_iot_store(root).unwrap();
        let status: String = conn
            .query_row("SELECT trigger_status FROM iot_widgets WHERE id = 'w1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "fired", "inbound webhook should drive the watcher to fire");
    }

    #[test]
    fn build_outbound_sets_json_and_rejects_bad_url() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let req = build_outbound(root, "https://erp.example/hook", &json!({ "a": 1 }), None, &[]).unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.headers.get("Content-Type").unwrap(), "application/json");
        assert!(req.headers.get("Authorization").is_none());
        assert_eq!(req.body, b"{\"a\":1}");
        // No auth header without a secret; non-http URL rejected.
        assert!(build_outbound(root, "ftp://nope", &json!({}), None, &[]).is_err());
    }
}
