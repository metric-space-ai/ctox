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
    use ctox_toolhub::AuthSchemeKind as K;
    let spec = adapter.spec().tools.iter().find(|t| t.slug == tool).expect("tool");
    assert!(!spec.mutating, "live harness is read-only; {tool} is mutating");
    let kinds: Vec<K> = adapter.spec().auth_schemes.iter().map(|s| s.kind()).collect();
    // Token-style schemes run through the real hub exec path.
    let scheme = [K::ApiKey, K::Bearer].into_iter().find(|k| kinds.contains(k)).unwrap_or(K::Bearer);
    live_exec(&adapter.spec().slug, scheme, tool, args)
}

/// Live call through the real hub exec path (auth config + stored credential +
/// header injection + response parsing), exactly as `ctox toolhub exec` does.
/// Credential from env `CTOX_TOOLHUB_LIVE_<SLUG>_TOKEN`; never printed.
pub fn live_exec(toolkit: &str, scheme: ctox_toolhub::AuthSchemeKind, tool: &str, args: Value) -> Option<Result<Value, String>> {
    use ctox_toolhub::*;
    let var = format!("CTOX_TOOLHUB_LIVE_{}_TOKEN", toolkit.to_ascii_uppercase().replace('-', "_"));
    let token = std::env::var(&var).ok()?;
    #[allow(unused_mut)]
    let mut hub = ToolHub::new(
        MemorySecretBackend::default(),
        std::sync::Arc::new(ctox_toolhub::http::UreqTransport::with_options(
            // The CTOX box resolves public hosts to 198.18.x.x behind its egress proxy.
            ctox_toolhub::http::TransportOptions { fake_ip_egress: true },
        ))
    );
    if let Some(spec) = SPEC_MODE.with(|m| m.borrow().clone()) {
        // Hot-plug path: the bundled spec is installed into a throwaway instance
        // store, activated and loaded; it replaces the compiled adapter.
        let root = std::env::temp_dir().join(format!("ctox-live-spec-{}-{toolkit}", std::process::id()));
        let store = ctox_toolhub::spec::SpecStore::for_instance_root(&root);
        let v = store.install(&spec).expect("install spec");
        store.activate(&v.slug, &v.version, "live-test").expect("activate spec");
        let rep = hub.reload_specs(&store).expect("reload specs");
        assert!(rep.replaced_builtins.contains(&toolkit.to_string()), "spec not loaded: {rep:?}");
        assert!(hub.is_spec_toolkit(toolkit));
    }
    let cfg = AuthConfig { id: "live".into(), toolkit: toolkit.into(), scheme, source: ClientCredentialSource::None, client_id: None, scopes: vec![], enabled: true };
    hub.upsert_auth_config(&cfg, None).expect("auth config");
    hub.connect_with_credential("live", "_instance", "live-test", token.trim()).expect("connect");
    Some(match hub.execute("_instance", tool, &args, None) {
        Ok(ToolOutcome::Success { data }) => Ok(data),
        Ok(ToolOutcome::ProviderError { status, data }) => Err(format!("provider {status}: {data}")),
        Ok(other) => Err(format!("{other:?}")),
        Err(e) => Err(format!("error: {e:#}")),
    })
}

thread_local! {
    static SPEC_MODE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Run following `live_exec` calls on this thread through the hot-plug spec
/// (`specs/<slug>.json`) instead of the compiled adapter.
pub fn use_spec(spec_json: &str) {
    SPEC_MODE.with(|m| *m.borrow_mut() = Some(spec_json.to_string()));
}

/// First string value under key `key` anywhere in `v` (depth-first).
pub fn find_str(v: &Value, key: &str) -> Option<String> {
    match v {
        Value::Object(m) => m.get(key).and_then(Value::as_str).map(String::from).or_else(|| m.values().find_map(|x| find_str(x, key))),
        Value::Array(a) => a.iter().find_map(|x| find_str(x, key)),
        _ => None,
    }
}
