//! Port test harness (shared by tests/port_*.rs).
//! DE: Jede portierte Aktion bekommt mindestens einen Fall: Eingabe -> erwartete
//! Provider-Anfrage (aus dem Activepieces-run()-Code abgeleitet) -> aufgezeichnete
//! Antwort -> erwartetes Ergebnis. EN: one recorded case per action.
#![allow(dead_code)]
use ctox_toolhub::http::{HttpRequest, HttpResponse, HttpTransport};
use ctox_toolhub::toolkits::ToolkitAdapter;
use serde_json::Value;

pub struct Case<'a> {
    pub tool: &'a str,
    pub args: Value,
    pub method: &'a str,
    /// Substrings the request URL must contain (path + query params).
    pub url_contains: &'a [&'a str],
    /// JSON pointer -> expected value in the request body (JSON bodies).
    pub body_eq: &'a [(&'a str, Value)],
    pub response_status: u16,
    pub response: Value,
    /// JSON pointer -> expected value in the parsed result.
    pub result_eq: &'a [(&'a str, Value)],
}

/// Minimal JSON-Schema check: required keys, additionalProperties=false, primitive types.
pub fn validate(schema: &Value, args: &Value) -> Result<(), String> {
    let obj = args.as_object().ok_or("args must be an object")?;
    for r in schema["required"].as_array().into_iter().flatten() {
        let k = r.as_str().unwrap();
        if !obj.contains_key(k) {
            return Err(format!("missing required `{k}`"));
        }
    }
    let props = schema["properties"].as_object().cloned().unwrap_or_default();
    for (k, v) in obj {
        let Some(p) = props.get(k) else {
            if schema["additionalProperties"] == Value::Bool(false) {
                return Err(format!("unknown property `{k}`"));
            }
            continue;
        };
        let ok = match p["type"].as_str() {
            Some("string") => v.is_string(),
            Some("integer") => v.is_i64() || v.is_u64(),
            Some("number") => v.is_number(),
            Some("boolean") => v.is_boolean(),
            Some("array") => v.is_array(),
            Some("object") => v.is_object(),
            _ => true,
        };
        if !ok {
            return Err(format!("`{k}` has wrong type"));
        }
    }
    Ok(())
}

pub fn run(adapter: &dyn ToolkitAdapter, c: &Case) {
    let spec = adapter
        .spec()
        .tools
        .iter()
        .find(|t| t.slug == c.tool)
        .unwrap_or_else(|| panic!("tool {} not in catalog", c.tool));
    validate(&spec.input_schema, &c.args).unwrap_or_else(|e| panic!("{}: {e}", c.tool));
    let req = adapter.build_request(c.tool, &c.args).expect("build_request");
    assert_eq!(req.method, c.method, "{} method", c.tool);
    for s in c.url_contains {
        assert!(req.url.contains(s), "{}: url {} lacks {s}", c.tool, req.url);
    }
    if !c.body_eq.is_empty() {
        let body: Value = serde_json::from_slice(req.body.as_deref().expect("body")).expect("json body");
        for (ptr, want) in c.body_eq {
            assert_eq!(body.pointer(ptr), Some(want), "{} body {ptr}", c.tool);
        }
    }
    let resp = HttpResponse { status: c.response_status, body: serde_json::to_vec(&c.response).unwrap(), session_id: None };
    let out = adapter.parse_response(c.tool, &resp).expect("parse_response");
    for (ptr, want) in c.result_eq {
        assert_eq!(out.pointer(ptr), Some(want), "{} result {ptr}", c.tool);
    }
}

/// Every catalog tool must have at least one recorded case.
pub fn assert_full_coverage(adapter: &dyn ToolkitAdapter, cases: &[Case]) {
    for t in &adapter.spec().tools {
        assert!(cases.iter().any(|c| c.tool == t.slug), "no test case for {}", t.slug);
    }
}

/// Live call (read-only) against the real provider. Token from env
/// `CTOX_TOOLHUB_LIVE_<SLUG>_TOKEN`; never printed. Returns None if unset.
pub fn live(adapter: &dyn ToolkitAdapter, tool: &str, args: Value) -> Option<Result<Value, String>> {
    let var = format!("CTOX_TOOLHUB_LIVE_{}_TOKEN", adapter.spec().slug.to_ascii_uppercase().replace('-', "_"));
    let token = std::env::var(&var).ok()?;
    let spec = adapter.spec().tools.iter().find(|t| t.slug == tool).expect("tool");
    assert!(!spec.mutating, "live harness is read-only; {tool} is mutating");
    let req: HttpRequest = adapter.build_request(tool, &args).expect("build").header("authorization", format!("Bearer {token}"));
    let resp = ctox_toolhub::http::UreqTransport::default().send(req);
    Some(match resp {
        Ok(r) if (200..300).contains(&r.status) => adapter.parse_response(tool, &r).map_err(|e| e.to_string()),
        Ok(r) => Err(format!("HTTP {}", r.status)),
        Err(e) => Err(e.to_string()),
    })
}
