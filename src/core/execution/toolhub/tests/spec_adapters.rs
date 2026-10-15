//! Hot-plug adapter specs: validator, differential check against the compiled
//! adapters, registry (versions, activation, rollback) and hub reload.
use ctox_toolhub::http::{HttpRequest, HttpResponse, HttpTransport};
use ctox_toolhub::spec::{SpecAdapter, SpecStore};
use ctox_toolhub::toolkits::ToolkitAdapter;
use ctox_toolhub::*;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

const TODOIST: &str = include_str!("../specs/todoist.json");
const ASANA: &str = include_str!("../specs/asana.json");

fn builtin(slug: &str) -> Box<dyn ToolkitAdapter> {
    toolkits::builtin().into_iter().find(|a| a.spec().slug == slug).unwrap()
}

fn sample(name: &str, prop: &Value) -> Value {
    if let Some(e) = prop["enum"].as_array() {
        return e[0].clone();
    }
    match prop["type"].as_str() {
        Some("integer") | Some("number") => json!(5),
        Some("boolean") => json!(true),
        Some("array") => json!(["a1", "b2"]),
        Some("object") => json!({"resource_type": "file"}),
        _ if name.ends_with("_on") || name.ends_with("_on_after") || name.ends_with("_on_before") => json!("2026-10-20"),
        _ if name.ends_with("_at") || name.ends_with("_at_after") => json!("2026-10-20T10:00:00Z"),
        _ if name == "since" || name == "until" => json!("2026-10-01T00:00:00Z"),
        _ => json!("abc123"),
    }
}

/// Argument sets per tool: everything set, only required, and variants of
/// fields with shape-dependent routing.
fn arg_sets(schema: &Value) -> Vec<Value> {
    let props = schema["properties"].as_object().cloned().unwrap_or_default();
    let req: Vec<&str> = schema["required"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let mut all = serde_json::Map::new();
    let mut only = serde_json::Map::new();
    for (k, p) in &props {
        all.insert(k.clone(), sample(k, p));
        if req.contains(&k.as_str()) {
            only.insert(k.clone(), sample(k, p));
        }
    }
    let mut out = vec![Value::Object(all.clone()), Value::Object(only.clone())];
    // one optional at a time on top of the required ones
    for (k, p) in &props {
        if !req.contains(&k.as_str()) {
            let mut m = only.clone();
            m.insert(k.clone(), sample(k, p));
            out.push(Value::Object(m));
        }
    }
    if props.contains_key("due_date") {
        for v in ["2026-10-20", "2026-10-20T09:00:00Z", "tomorrow 5pm"] {
            let mut m = only.clone();
            m.insert("due_date".into(), json!(v));
            out.push(Value::Object(m));
        }
    }
    out.push(json!({}));
    out
}

fn norm(r: &HttpRequest) -> (String, String, Vec<(String, String)>, Option<Value>) {
    let u = url::Url::parse(&r.url).unwrap();
    let mut q: Vec<(String, String)> = u.query_pairs().map(|(a, b)| (a.into_owned(), b.into_owned())).collect();
    q.sort();
    let body = r.body.as_ref().map(|b| serde_json::from_slice(b).unwrap_or(Value::String(String::from_utf8_lossy(b).into())));
    (r.method.clone(), format!("{}{}", u.host_str().unwrap(), u.path()), q, body)
}

fn differential(slug: &str, text: &str) -> (usize, Vec<String>) {
    let rust = builtin(slug);
    let spec = SpecAdapter::from_json(text).unwrap();
    assert_eq!(rust.spec().tools.len(), spec.spec().tools.len(), "{slug} tool count");
    let mut compared = 0;
    let mut diffs = vec![];
    for t in &rust.spec().tools {
        let st = spec.spec().tools.iter().find(|x| x.slug == t.slug).expect("tool in spec");
        assert_eq!(st.input_schema, t.input_schema, "{} schema", t.slug);
        assert_eq!(st.mutating, t.mutating, "{} mutating", t.slug);
        for args in arg_sets(&t.input_schema) {
            let a = rust.build_request(&t.slug, &args);
            let b = spec.build_request(&t.slug, &args);
            compared += 1;
            match (a, b) {
                (Ok(a), Ok(b)) if norm(&a) == norm(&b) => {}
                (Err(_), Err(_)) => {}
                (a, b) => diffs.push(format!(
                    "{} {args}\n  rust: {:?}\n  spec: {:?}",
                    t.slug,
                    a.map(|r| norm(&r)).map_err(|e| e.to_string()),
                    b.map(|r| norm(&r)).map_err(|e| e.to_string())
                )),
            }
        }
        for body in [
            json!({"results": [{"id": "1"}], "next_cursor": null}),
            json!({"data": {"gid": "1", "name": "x"}}),
            json!({"data": [{"gid": "1"}], "next_page": {"offset": "off1"}}),
            json!({"data": [{"gid": "1"}], "next_page": null}),
            json!({"id": "1", "content": "x"}),
            json!({"errors": [{"message": "bad"}]}),
            json!({}),
        ] {
            let resp = HttpResponse { status: 200, body: serde_json::to_vec(&body).unwrap(), session_id: None };
            let a = rust.parse_response(&t.slug, &resp);
            let b = spec.parse_response(&t.slug, &resp);
            compared += 1;
            match (a, b) {
                (Ok(a), Ok(b)) if a == b => {}
                (Err(_), Err(_)) => {}
                (a, b) => diffs.push(format!("{} parse {body}\n  rust: {:?}\n  spec: {:?}", t.slug, a.map_err(|e| e.to_string()), b.map_err(|e| e.to_string()))),
            }
        }
    }
    (compared, diffs)
}

#[test]
fn todoist_spec_matches_compiled_adapter() {
    let (n, d) = differential("todoist", TODOIST);
    assert!(d.is_empty(), "{} of {n} differ:\n{}", d.len(), d.join("\n"));
    println!("todoist: {n} request/response cases identical");
}

#[test]
fn asana_spec_matches_compiled_adapter() {
    let (n, d) = differential("asana", ASANA);
    assert!(d.is_empty(), "{} of {n} differ:\n{}", d.len(), d.join("\n"));
    println!("asana: {n} request/response cases identical");
}

// ------------------------------------------------------------------ validator

fn minimal() -> Value {
    json!({
        "schema": "ctox.adapter-spec/v1", "slug": "acme", "version": "1.0.0", "name": "Acme",
        "description": {"de": "Acme-Dienst", "en": "Acme service"},
        "api_base": "https://api.acme.io/v2", "allowed_hosts": ["api.acme.io"],
        "auth_schemes": [{"type": "bearer"}],
        "actions": [{
            "slug": "ACME_GET_ITEM", "risk": "read",
            "title": {"de": "Eintrag lesen", "en": "Get item"}, "description": {"de": "Liest", "en": "Reads"},
            "input_schema": {"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]},
            "request": {"method": "GET", "path": "/items/{id}"}
        }]
    })
}

fn rejects(mutate: impl Fn(&mut Value), needle: &str) {
    let mut v = minimal();
    mutate(&mut v);
    let err = SpecAdapter::from_json(&v.to_string()).err().unwrap_or_else(|| panic!("accepted, expected `{needle}`"));
    assert!(format!("{err:#}").contains(needle), "`{err:#}` lacks `{needle}`");
}

#[test]
fn validator_accepts_minimal_and_rejects_unsafe_specs() {
    SpecAdapter::from_json(&minimal().to_string()).unwrap();
    rejects(|v| v["api_base"] = json!("http://api.acme.io"), "https");
    rejects(|v| v["api_base"] = json!("https://evil.io/x"), "allowed hosts");
    rejects(|v| v["allowed_hosts"] = json!(["10.0.0.5"]), "public DNS");
    rejects(|v| v["allowed_hosts"] = json!(["*.io"]), "public DNS");
    rejects(|v| v["allowed_hosts"] = json!(["metadata.internal"]), "public DNS");
    rejects(|v| v["actions"][0]["request"]["headers"] = json!({"Authorization": "x"}), "may not be set");
    rejects(|v| v["actions"][0]["request"]["headers"] = json!({"x-trace": "Bearer abcdef"}), "look like credentials");
    rejects(|v| v["notes"] = json!("ghp_0123456789abcdefghijklmnopqrstuvwxyz"), "look like credentials");
    rejects(|v| v["actions"][0]["request"]["method"] = json!("DELETE"), "destructive");
    rejects(|v| v["actions"][0]["request"]["method"] = json!("POST"), "must be `write`");
    rejects(|v| v["actions"][0]["risk"] = json!("write"), "GET actions must be `read`");
    rejects(|v| v["actions"][0]["risk"] = json!("maybe"), "risk must be");
    rejects(|v| v["actions"][0]["request"]["path"] = json!("/items/{nope}"), "not declared");
    rejects(|v| v["actions"][0]["request"]["path"] = json!("/items/../admin"), "path must");
    rejects(|v| v["actions"][0]["input_schema"]["properties"]["extra"] = json!({"type": "string"}), "never sent");
    rejects(|v| v["actions"][0]["slug"] = json!("OTHER_GET"), "start with `ACME_`");
    rejects(|v| v["schema"] = json!("v0"), "schema");
    rejects(|v| v["version"] = json!("1"), "MAJOR.MINOR.PATCH");
    rejects(|v| v["description"] = json!({"de": "nur deutsch"}), "`en` must");
    rejects(
        |v| {
            v["api_base_template"] = json!("https://{sub}.evil.io");
            v["connection_fields"] = json!([{"key": "sub", "label": {"de": "a", "en": "a"}, "help": {"de": "a", "en": "a"}, "placeholder": "", "required": true}]);
        },
        "allowed_hosts",
    );
    rejects(
        |v| {
            v["actions"][0]["risk"] = json!("read");
            v["actions"][0]["request"]["method"] = json!("POST");
            v["actions"][0]["request"]["read_only_post"] = json!(true);
            v["actions"][0]["request"]["body"] = json!({"graphql": {"query": "mutation { x }"}});
        },
        "mutations",
    );
    rejects(|v| v["actions"][0]["request"]["surprise"] = json!(1), "unknown field");
}

#[test]
fn per_connection_base_url_stays_inside_allowed_hosts() {
    let mut v = minimal();
    v["allowed_hosts"] = json!(["*.acme.io"]);
    v["api_base"] = json!("https://api.acme.io/v2");
    v["api_base_template"] = json!("https://{tenant}.acme.io/v2");
    v["connection_fields"] = json!([{"key": "tenant", "label": {"de": "Mandant", "en": "Tenant"}, "help": {"de": "", "en": ""}, "placeholder": "firma", "required": true}]);
    let a = SpecAdapter::from_json(&v.to_string()).unwrap();
    let p = |t: &str| std::collections::BTreeMap::from([("tenant".to_string(), t.to_string())]);
    assert_eq!(a.connection_api_base(&p("Firma")).unwrap().unwrap(), "https://firma.acme.io/v2");
    for bad in ["evil.io/x", "a.b", "x@y", "", "-bad"] {
        assert!(a.connection_api_base(&p(bad)).is_err(), "{bad}");
    }
}

#[test]
fn graphql_and_form_bodies() {
    let mut v = minimal();
    v["actions"] = json!([
        {"slug": "ACME_VIEWER", "risk": "read", "title": {"de": "a", "en": "a"}, "description": {"de": "a", "en": "a"},
         "input_schema": {"type": "object", "properties": {"first": {"type": "integer"}}},
         "request": {"method": "POST", "path": "/graphql", "read_only_post": true,
                     "body": {"graphql": {"query": "query($first:Int){ viewer { id } }", "variables": {"first": {"arg": "first", "default": 10, "clamp": [1, 50]}}}}},
         "response": {"mode": "pick", "pointer": "/data/viewer", "required": true}},
        {"slug": "ACME_SEND", "risk": "write", "title": {"de": "a", "en": "a"}, "description": {"de": "a", "en": "a"},
         "input_schema": {"type": "object", "properties": {"to": {"type": "string"}, "body": {"type": "string"}}, "required": ["to"]},
         "request": {"method": "POST", "path": "/messages", "body": {"form": {"To": {"arg": "to", "required": true}, "Body": {"arg": "body"}, "Kind": {"const": "sms"}}}}}
    ]);
    let a = SpecAdapter::from_json(&v.to_string()).unwrap();
    let r = a.build_request("ACME_VIEWER", &json!({"first": 99})).unwrap();
    let b: Value = serde_json::from_slice(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(b["variables"]["first"], json!(50));
    assert!(b["query"].as_str().unwrap().contains("viewer"));
    let resp = HttpResponse { status: 200, body: br#"{"data":{"viewer":{"id":"u1"}}}"#.to_vec(), session_id: None };
    assert_eq!(a.parse_response("ACME_VIEWER", &resp).unwrap(), json!({"id": "u1"}));
    let r = a.build_request("ACME_SEND", &json!({"to": "+49 1"})).unwrap();
    assert_eq!(r.headers["content-type"], "application/x-www-form-urlencoded");
    assert_eq!(String::from_utf8(r.body.unwrap()).unwrap(), "Kind=sms&To=%2B49+1");
    assert!(a.build_request("ACME_SEND", &json!({})).is_err());
    assert!(!a.spec().tools[0].mutating && a.spec().tools[1].mutating);
}

// ------------------------------------------------------------------ registry + hub

struct Mock {
    calls: Mutex<Vec<(bool, String)>>,
}
impl HttpTransport for Mock {
    fn send(&self, r: HttpRequest) -> anyhow::Result<HttpResponse> {
        self.calls.lock().unwrap().push((false, r.url));
        Ok(HttpResponse { status: 200, body: br#"{"id":"u1"}"#.to_vec(), session_id: None })
    }
    fn send_restricted(&self, r: HttpRequest) -> anyhow::Result<HttpResponse> {
        self.calls.lock().unwrap().push((true, r.url));
        Ok(HttpResponse { status: 200, body: br#"{"id":"u1"}"#.to_vec(), session_id: None })
    }
}

fn tmpdir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("ctox-spec-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

#[test]
fn registry_versions_activation_rollback_and_hot_reload() {
    let root = tmpdir("reg");
    let store = SpecStore::for_instance_root(&root);
    assert!(store.dir().ends_with("runtime/toolhub/adapter-specs"));
    let v1 = minimal();
    let mut v2 = minimal();
    v2["version"] = json!("1.1.0");
    v2["actions"][0]["request"]["path"] = json!("/v2items/{id}");
    assert!(store.install(&v1.to_string()).unwrap().new);
    assert!(!store.install(&v1.to_string()).unwrap().new);
    let mut changed = v1.clone();
    changed["name"] = json!("Acme 2");
    assert!(store.install(&changed.to_string()).is_err(), "versions are immutable");
    store.install(&v2.to_string()).unwrap();

    let mock = Arc::new(Mock { calls: Mutex::new(vec![]) });
    let mut hub = ToolHub::new(MemorySecretBackend::default(), mock.clone());
    let rep = hub.reload_specs(&store).unwrap();
    assert!(rep.loaded.is_empty(), "installed but inactive specs are not served");
    assert!(hub.tool("ACME_GET_ITEM").is_err());

    store.activate("acme", "1.0.0", "test").unwrap();
    assert!(hub.reload_specs_if_changed(&store).unwrap().is_some());
    assert!(hub.reload_specs_if_changed(&store).unwrap().is_none(), "no change, no reload");
    let cfg = AuthConfig { id: "c".into(), toolkit: "acme".into(), scheme: AuthSchemeKind::Bearer, source: ClientCredentialSource::None, client_id: None, scopes: vec![], enabled: true };
    hub.upsert_auth_config(&cfg, None).unwrap();
    hub.connect_with_credential("c", "p", "a", "tok").unwrap();
    let out = hub.execute("p", "acme.get_item", &json!({"id": "x1"}), None).unwrap();
    assert!(matches!(out, ToolOutcome::Success { .. }));

    store.activate("acme", "1.1.0", "test").unwrap();
    hub.reload_specs_if_changed(&store).unwrap().unwrap();
    hub.execute("p", "ACME_GET_ITEM", &json!({"id": "x1"}), None).unwrap();
    let ptr = store.rollback("acme", "test").unwrap();
    assert_eq!(ptr.version, "1.0.0");
    assert_eq!(ptr.previous.as_deref(), Some("1.1.0"));
    hub.reload_specs_if_changed(&store).unwrap().unwrap();
    hub.execute("p", "ACME_GET_ITEM", &json!({"id": "x1"}), None).unwrap();
    let calls = mock.calls.lock().unwrap().clone();
    assert_eq!(
        calls,
        vec![
            (true, "https://api.acme.io/v2/items/x1".to_string()),
            (true, "https://api.acme.io/v2/v2items/x1".to_string()),
            (true, "https://api.acme.io/v2/items/x1".to_string()),
        ],
        "spec adapters always go through the restricted transport"
    );

    // tampering with an active version is detected and the spec is not served
    std::fs::write(store.dir().join("acme/versions/1.0.0.json"), changed.to_string()).unwrap();
    let rep = hub.reload_specs(&store).unwrap();
    assert!(rep.loaded.is_empty() && rep.failed[0].1.contains("hash mismatch"), "{rep:?}");
    assert!(hub.tool("ACME_GET_ITEM").is_err());
    store.deactivate("acme", "test").unwrap();
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn spec_replaces_builtin_only_when_declared_and_restores_it() {
    let root = tmpdir("shadow");
    let store = SpecStore::for_instance_root(&root);
    let mut no_flag: Value = serde_json::from_str(TODOIST).unwrap();
    no_flag["replaces_builtin"] = json!(false);
    no_flag["version"] = json!("0.9.0");
    store.install(&no_flag.to_string()).unwrap();
    store.install(TODOIST).unwrap();
    let mut hub = ToolHub::new(MemorySecretBackend::default(), Arc::new(Mock { calls: Mutex::new(vec![]) }));
    store.activate("todoist", "0.9.0", "t").unwrap();
    let rep = hub.reload_specs(&store).unwrap();
    assert!(rep.loaded.is_empty() && rep.failed[0].1.contains("built-in"));
    assert!(!hub.is_spec_toolkit("todoist"));
    store.activate("todoist", "1.0.0", "t").unwrap();
    let rep = hub.reload_specs(&store).unwrap();
    assert_eq!(rep.replaced_builtins, vec!["todoist".to_string()]);
    assert!(hub.is_spec_toolkit("todoist"));
    store.deactivate("todoist", "t").unwrap();
    hub.reload_specs(&store).unwrap();
    assert!(!hub.is_spec_toolkit("todoist") && hub.tool("TODOIST_GET_CURRENT_USER").is_ok(), "builtin is back");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn hub_blocks_requests_leaving_allowed_hosts() {
    // A per-connection param can never steer the token to a foreign host.
    let mut v = minimal();
    v["allowed_hosts"] = json!(["*.acme.io"]);
    v["api_base_template"] = json!("https://{tenant}.acme.io/v2");
    v["connection_fields"] = json!([{"key": "tenant", "label": {"de": "M", "en": "T"}, "help": {"de": "", "en": ""}, "placeholder": "", "required": true}]);
    let root = tmpdir("egress");
    let store = SpecStore::for_instance_root(&root);
    store.install(&v.to_string()).unwrap();
    store.activate("acme", "1.0.0", "t").unwrap();
    let mock = Arc::new(Mock { calls: Mutex::new(vec![]) });
    let mut hub = ToolHub::new(MemorySecretBackend::default(), mock.clone());
    hub.reload_specs(&store).unwrap();
    let cfg = AuthConfig { id: "c".into(), toolkit: "acme".into(), scheme: AuthSchemeKind::Bearer, source: ClientCredentialSource::None, client_id: None, scopes: vec![], enabled: true };
    hub.upsert_auth_config(&cfg, None).unwrap();
    let bad = std::collections::BTreeMap::from([("tenant".to_string(), "evil.io#".to_string())]);
    assert!(hub.connect_with_credential_params("c", "p", "a", "tok", &bad).is_err());
    let good = std::collections::BTreeMap::from([("tenant".to_string(), "firma".to_string())]);
    hub.connect_with_credential_params("c", "p", "a", "tok", &good).unwrap();
    hub.execute("p", "ACME_GET_ITEM", &json!({"id": "x"}), None).unwrap();
    assert_eq!(mock.calls.lock().unwrap()[0], (true, "https://firma.acme.io/v2/items/x".to_string()));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn differential_check_detects_a_wrong_mapping() {
    let broken = TODOIST.replace("/tasks/{task_id}/close", "/tasks/{task_id}/done").replace("\"as\": \"labels\"", "\"as\": \"items\"");
    let (_, d) = differential("todoist", &broken);
    assert!(d.iter().any(|x| x.starts_with("TODOIST_COMPLETE_TASK")), "path change not detected");
    assert!(d.iter().any(|x| x.starts_with("TODOIST_LIST_LABELS parse")), "response change not detected");
}

// ------------------------------------------------------------------ resource-URI references (Calendly)

const CALENDLY: &str = include_str!("../specs/calendly.json");

#[test]
fn uri_prefix_and_uri_tail_handle_resource_uri_references() {
    let a = SpecAdapter::from_json(CALENDLY).expect("calendly spec compiles");
    let uuid = "356d8cef-6813-4815-8544-7ac5d11d74e1";
    let full = format!("https://api.calendly.com/users/{uuid}");
    // Query: a bare UUID is expanded, a full URI passes through unchanged.
    for v in [uuid.to_string(), full.clone()] {
        let r = a.build_request("CALENDLY_LIST_EVENT_TYPES", &json!({"user": v})).unwrap();
        assert!(r.url.contains(&format!("user=https%3A%2F%2Fapi.calendly.com%2Fusers%2F{uuid}")), "{}", r.url);
    }
    // A URI for another resource type or host is refused.
    for bad in ["https://api.calendly.com/event_types/x", "https://evil.example/users/x", "a/b"] {
        assert!(a.build_request("CALENDLY_LIST_EVENT_TYPES", &json!({"user": bad})).is_err(), "{bad}");
    }
    // Path: URI or UUID both end up as the last segment.
    for v in [uuid.to_string(), full.clone(), format!("{full}/")] {
        let r = a.build_request("CALENDLY_GET_USER", &json!({"user": v})).unwrap();
        assert_eq!(r.url, format!("https://api.calendly.com/users/{uuid}"));
    }
    assert!(a.build_request("CALENDLY_GET_USER", &json!({"user": "https://api.calendly.com/users/a%2F.."})).is_err());
    // Body: owner URI built from the event type id; owner_type constant.
    let r = a.build_request("CALENDLY_CREATE_SCHEDULING_LINK", &json!({"event_type": "et1"})).unwrap();
    let b: Value = serde_json::from_slice(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(b, json!({"max_event_count": 1, "owner": "https://api.calendly.com/event_types/et1", "owner_type": "EventType"}));
    // Owner constraint and risk levels.
    assert!(a.build_request("CALENDLY_LIST_SCHEDULED_EVENTS", &json!({})).is_err());
    assert_eq!(a.risk("CALENDLY_CANCEL_EVENT"), Some(spec::validate::Risk::Destructive));
    rejects(|v| v["actions"][0]["request"]["path_encoding"] = json!({"id": "weird"}), "unknown path encoding");
    rejects(|v| {
        v["actions"][0]["request"]["query"] = json!({"x": {"arg": "id", "uri_prefix": "http://api.acme.io/x/"}});
    }, "uri_prefix must be an https URL");
}

// ------------------------------------------------------------------ brand-new service, auth `none`

const ZIPPOPOTAM: &str = include_str!("../specs/zippopotam.json");

#[test]
fn brand_new_spec_dropped_into_store_is_served_after_hot_reload_without_credential() {
    assert!(!toolkits::builtin().iter().any(|a| a.spec().slug == "zippopotam"), "must be a brand-new service");
    let root = tmpdir("newsvc");
    let store = SpecStore::for_instance_root(&root);
    let mock = Arc::new(Mock { calls: Mutex::new(vec![]) });
    let mut hub = ToolHub::new(MemorySecretBackend::default(), mock.clone());
    hub.reload_specs_if_changed(&store).unwrap();
    assert!(hub.tool("ZIPPOPOTAM_LOOKUP_POSTAL_CODE").is_err());
    // Runtime drop-in: install + activate while the hub keeps running.
    let v = store.install(ZIPPOPOTAM).unwrap();
    store.activate(&v.slug, &v.version, "test").unwrap();
    let rep = hub.reload_specs_if_changed(&store).unwrap().expect("store changed");
    assert_eq!(rep.loaded, vec![("zippopotam".to_string(), "1.0.0".to_string())]);
    assert!(rep.replaced_builtins.is_empty() && hub.is_spec_toolkit("zippopotam"));
    // No auth config, no account: auth `none` executes directly, restricted transport.
    let out = hub.execute("p", "zippopotam.lookup_postal_code", &json!({"country": "de", "postal_code": "10115"}), None).unwrap();
    assert!(matches!(out, ToolOutcome::Success { .. }), "{out:?}");
    let out = hub.execute("p", "ZIPPOPOTAM_FIND_POSTAL_CODES", &json!({"country": "us", "state": "ma", "city": "New York"}), None).unwrap();
    assert!(matches!(out, ToolOutcome::Success { .. }));
    assert_eq!(*mock.calls.lock().unwrap(), vec![
        (true, "https://api.zippopotam.us/de/10115".to_string()),
        (true, "https://api.zippopotam.us/us/ma/New%20York".to_string()),
    ]);
    assert!(hub.execute("p", "ZIPPOPOTAM_LOOKUP_POSTAL_CODE", &json!({"country": "../x", "postal_code": "1"}), None).is_err());
    // Deactivate: gone again without restart.
    store.deactivate("zippopotam", "test").unwrap();
    hub.reload_specs_if_changed(&store).unwrap().unwrap();
    assert!(hub.tool("ZIPPOPOTAM_LOOKUP_POSTAL_CODE").is_err());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn auth_none_must_be_the_only_scheme_and_builtins_still_need_credentials() {
    rejects(|v| v["auth_schemes"] = json!([{"type": "none"}, {"type": "bearer"}]), "must be the only scheme");
    let mut v = minimal();
    v["auth_schemes"] = json!([{"type": "none"}]);
    assert!(SpecAdapter::from_json(&v.to_string()).is_ok());
    // A credential-based toolkit without account still asks for a credential.
    let hub = ToolHub::new(MemorySecretBackend::default(), Arc::new(Mock { calls: Mutex::new(vec![]) }));
    let out = hub.execute("p", "TODOIST_GET_CURRENT_USER", &json!({}), None).unwrap();
    assert!(!matches!(out, ToolOutcome::Success { .. }));
}
