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
    // adversarial string values: catch missing format checks, `@` stripping, namespaced ids
    for (k, p) in &props {
        if p["type"] == "string" && p.get("enum").is_none() {
            for v in ["not-a-date", "@handle", "grp/sub", "a b"] {
                let mut m = all.clone();
                m.insert(k.clone(), json!(v));
                out.push(Value::Object(m));
            }
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
    differential_opts(slug, text, false)
}

/// `partial`: compare only the tools present in the spec (builder drafts).
fn differential_opts(slug: &str, text: &str, partial: bool) -> (usize, Vec<String>) {
    let rust = builtin(slug);
    let spec = SpecAdapter::from_json(text).unwrap();
    if !partial {
        assert_eq!(rust.spec().tools.len(), spec.spec().tools.len(), "{slug} tool count");
    }
    let mut compared = 0;
    let mut diffs = vec![];
    for t in &rust.spec().tools {
        let Some(st) = spec.spec().tools.iter().find(|x| x.slug == t.slug) else {
            assert!(partial, "{} missing in spec", t.slug);
            continue;
        };
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

// ------------------------------------------------------------------ Stripe form encoding + cursor pagination

const STRIPE: &str = include_str!("../specs/stripe.json");

#[test]
fn form_bodies_flatten_nested_objects_and_arrays_with_brackets() {
    let a = SpecAdapter::from_json(STRIPE).expect("stripe spec");
    let r = a.build_request("STRIPE_CREATE_CUSTOMER", &json!({
        "email": "a@b.de", "name": "Ada",
        "address": {"city": "Berlin", "country": "DE", "postal_code": "10115"},
        "metadata": {"order": "42", "unused": null},
    })).unwrap();
    assert_eq!(r.headers["content-type"], "application/x-www-form-urlencoded");
    let body = String::from_utf8(r.body.unwrap()).unwrap();
    for p in ["email=a%40b.de", "name=Ada", "address%5Bcity%5D=Berlin", "address%5Bcountry%5D=DE",
              "address%5Bpostal_code%5D=10115", "metadata%5Border%5D=42"] {
        assert!(body.contains(p), "missing `{p}` in `{body}`");
    }
    assert!(!body.contains("metadata%5Bunused%5D") && !body.contains("null"), "nulls are dropped: {body}");
    let r = a.build_request("STRIPE_CREATE_PAYMENT_LINK", &json!({
        "line_items": [{"price": "price_x", "quantity": 2}, {"price": "price_y", "quantity": 1}]
    })).unwrap();
    let body = String::from_utf8(r.body.unwrap()).unwrap();
    for p in ["line_items%5B0%5D%5Bprice%5D=price_x", "line_items%5B0%5D%5Bquantity%5D=2",
              "line_items%5B1%5D%5Bprice%5D=price_y"] {
        assert!(body.contains(p), "missing `{p}` in `{body}`");
    }
    // Nested too deep / bad key.
    let bad = SpecAdapter::from_json(&{
        let mut v = minimal();
        v["actions"][0]["request"] = json!({"method":"POST","path":"/x","body":{"form":{"m":{"arg":"id","type":"raw"}}}});
        v["actions"][0]["slug"] = json!("ACME_SEND");
        v["actions"][0]["risk"] = json!("write");
        v.to_string()
    }).unwrap();
    assert!(bad.build_request("ACME_SEND", &json!({"id": {"a":{"b":{"c":{"d":{"e":{"f":1}}}}}}})).is_err());
    assert!(bad.build_request("ACME_SEND", &json!({"id": {"bad[key]": "x"}})).is_err());
}

#[test]
fn response_pointer_dash_selects_last_array_element_for_cursor_pagination() {
    let a = SpecAdapter::from_json(STRIPE).unwrap();
    let resp = HttpResponse {
        status: 200,
        body: br#"{"object":"list","has_more":true,"data":[{"id":"cus_a"},{"id":"cus_b"},{"id":"cus_c"}]}"#.to_vec(),
        session_id: None,
    };
    let out = a.parse_response("STRIPE_LIST_CUSTOMERS", &resp).unwrap();
    assert_eq!(out, json!({"data": [{"id":"cus_a"},{"id":"cus_b"},{"id":"cus_c"}], "has_more": true, "next_starting_after": "cus_c"}));
    let empty = HttpResponse { status: 200, body: br#"{"object":"list","has_more":false,"data":[]}"#.to_vec(), session_id: None };
    let out = a.parse_response("STRIPE_LIST_CUSTOMERS", &empty).unwrap();
    assert_eq!(out["has_more"], json!(false));
    assert!(out["next_starting_after"].is_null());
    // Refund requires exactly one of payment_intent / charge.
    assert!(a.build_request("STRIPE_CREATE_REFUND", &json!({})).is_err());
    assert!(a.build_request("STRIPE_CREATE_REFUND", &json!({"payment_intent":"pi_1","charge":"ch_1"})).is_err());
    let r = a.build_request("STRIPE_CREATE_REFUND", &json!({"payment_intent":"pi_1","reason":"requested_by_customer"})).unwrap();
    assert!(String::from_utf8(r.body.unwrap()).unwrap().contains("payment_intent=pi_1"));
}

// ------------------------------------------------------------------ oauth_header (Trello)

const TRELLO: &str = include_str!("../specs/trello.json");

#[test]
fn oauth_header_puts_key_and_token_in_authorization_not_in_url() {
    let a = SpecAdapter::from_json(TRELLO).expect("trello spec");
    assert_eq!(a.spec().auth_schemes, vec![AuthScheme::OAuthHeader]);
    let root = tmpdir("oauthh");
    let store = SpecStore::for_instance_root(&root);
    store.install(TRELLO).unwrap();
    store.activate("trello", "1.0.0", "t").unwrap();
    struct Cap(Mutex<Vec<(String, String)>>);
    impl HttpTransport for Cap {
        fn send(&self, _: HttpRequest) -> anyhow::Result<HttpResponse> { unreachable!() }
        fn send_restricted(&self, r: HttpRequest) -> anyhow::Result<HttpResponse> {
            self.0.lock().unwrap().push((r.url.clone(), r.headers.get("authorization").cloned().unwrap_or_default()));
            Ok(HttpResponse { status: 200, body: br#"{"id":"m1","username":"u"}"#.to_vec(), session_id: None })
        }
    }
    let cap = Arc::new(Cap(Mutex::new(vec![])));
    let mut hub = ToolHub::new(MemorySecretBackend::default(), cap.clone());
    hub.reload_specs(&store).unwrap();
    let cfg = AuthConfig { id: "c".into(), toolkit: "trello".into(), scheme: AuthSchemeKind::OAuthHeader, source: ClientCredentialSource::None, client_id: None, scopes: vec![], enabled: true };
    hub.upsert_auth_config(&cfg, None).unwrap();
    hub.connect_with_credential("c", "p", "a", "KEY123:TOKEN456").unwrap();
    assert!(matches!(hub.execute("p", "TRELLO_GET_CURRENT_MEMBER", &json!({}), None).unwrap(), ToolOutcome::Success { .. }));
    let calls = cap.0.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    assert!(!calls[0].0.contains("KEY123") && !calls[0].0.contains("TOKEN456"), "secrets must not be in the URL: {}", calls[0].0);
    assert_eq!(calls[0].1, r#"OAuth oauth_consumer_key="KEY123", oauth_token="TOKEN456""#);
    assert!(hub.connect_with_credential("c", "p", "a", "KEY123").is_err(), "needs key:token");
    assert!(hub.connect_with_credential("c", "p", "a", "KEY\"bad:TOK").is_err());
    let _ = std::fs::remove_dir_all(&root);
}

/// Builder-agent loop: `CTOX_SPEC_DRAFT=<slug>:<path> cargo test --test spec_adapters differential_report -- --ignored --nocapture`
/// prints compile errors or the per-tool differential against the compiled adapter.
#[test]
#[ignore]
fn differential_report() {
    let Ok(v) = std::env::var("CTOX_SPEC_DRAFT") else { return };
    let (slug, path) = v.split_once(':').expect("slug:path");
    let text = std::fs::read_to_string(path).unwrap();
    if let Err(e) = SpecAdapter::from_json(&text) {
        println!("COMPILE ERROR: {e:#}");
        return;
    }
    let (n, d) = differential_opts(slug, &text, true);
    let mut per: std::collections::BTreeMap<String, usize> = Default::default();
    for x in &d {
        *per.entry(x.split_whitespace().next().unwrap().to_string()).or_default() += 1;
    }
    println!("DIFF {slug}: {} of {n} differ; tools: {per:?}", d.len());
    for x in d.iter().take(std::env::var("CTOX_SPEC_SHOW").ok().and_then(|s| s.parse().ok()).unwrap_or(6)) {
        println!("{x}\n");
    }
}

// ------------------------------------------------------------------ Airtable (converted from Rust)

const AIRTABLE: &str = include_str!("../specs/airtable.json");

#[test]
fn airtable_spec_matches_compiled_adapter() {
    let (n, d) = differential("airtable", AIRTABLE);
    assert!(d.is_empty(), "{} of {n} differ:\n{}", d.len(), d.join("\n"));
}

#[test]
fn template_switch_and_list_options() {
    let a = SpecAdapter::from_json(AIRTABLE).unwrap();
    let base = json!({"base_id": "app1", "table_id_or_name": "tbl1", "search_field": "Name", "search_value": "a \"b\" \\ c"});
    let q = |r: HttpRequest| url::Url::parse(&r.url).unwrap().query_pairs().find(|(k, _)| k == "filterByFormula").map(|(_, v)| v.into_owned()).unwrap();
    assert_eq!(q(a.build_request("AIRTABLE_SEARCH_RECORDS", &base).unwrap()), r#"FIND("a \"b\" \\ c",{Name})"#);
    let mut exact = base.clone();
    exact["match_mode"] = json!("exact");
    assert_eq!(q(a.build_request("AIRTABLE_SEARCH_RECORDS", &exact).unwrap()), r#"{Name}&""="a \"b\" \\ c""#);
    exact["match_mode"] = json!("fuzzy");
    assert!(a.build_request("AIRTABLE_SEARCH_RECORDS", &exact).is_err(), "unknown switch case");
    let mut bad = base.clone();
    bad["search_field"] = json!("Na{me}");
    assert!(a.build_request("AIRTABLE_SEARCH_RECORDS", &bad).is_err(), "forbidden brace");
    // max_items / items_segment
    let ids: Vec<String> = (0..11).map(|i| format!("rec{i}")).collect();
    assert!(a.build_request("AIRTABLE_DELETE_RECORDS_BATCH", &json!({"base_id": "app1", "table_id_or_name": "tbl1", "record_ids": ids})).is_err());
    assert!(a.build_request("AIRTABLE_DELETE_RECORDS_BATCH", &json!({"base_id": "app1", "table_id_or_name": "tbl1", "record_ids": ["rec/../x"]})).is_err());
    // wrap_object + require_key + bracketed query arrays
    let r = a.build_request("AIRTABLE_LIST_RECORDS", &json!({"base_id": "app1", "table_id_or_name": "tbl1",
        "sort": {"field": "Due", "direction": "asc"}, "fields": ["Name", "Status"]})).unwrap();
    let u = url::Url::parse(&r.url).unwrap();
    let mut pairs: Vec<(String, String)> = u.query_pairs().map(|(k, v)| (k.into(), v.into())).collect();
    pairs.sort();
    assert_eq!(pairs, vec![("fields[0]".into(), "Name".into()), ("fields[1]".into(), "Status".into()),
        ("sort[0][direction]".into(), "asc".into()), ("sort[0][field]".into(), "Due".into())]);
    // blank_to_null
    let r = a.build_request("AIRTABLE_CLEAR_RECORD", &json!({"base_id": "app1", "table_id_or_name": "tbl1", "record_id": "rec1", "fields": {"Due": "", "Name": "x"}})).unwrap();
    let b: Value = serde_json::from_slice(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(b["fields"], json!({"Due": null, "Name": "x"}));
    // nested each + count
    let resp = HttpResponse { status: 200, session_id: None, body: serde_json::to_vec(&json!({"tables": [{"id": "t1", "name": "T", "primaryFieldId": "f1",
        "fields": [{"id": "f1", "name": "Name", "type": "singleLineText", "options": {}}]}]})).unwrap() };
    let out = a.parse_response("AIRTABLE_GET_BASE_SCHEMA", &resp).unwrap();
    assert_eq!(out["count"], json!(1));
    assert_eq!(out["tables"][0]["fields"], json!([{"id": "f1", "name": "Name", "type": "singleLineText", "description": null}]));
    assert_eq!(out["tables"][0]["views"], json!([]), "missing nested list -> []");
    assert_eq!(out["tables"][0]["primary_field_id"], json!("f1"));
}

const GITLAB: &str = include_str!("../specs/gitlab.json");

#[test]
fn gitlab_spec_matches_compiled_adapter() {
    let (n, d) = differential("gitlab", GITLAB);
    assert!(d.is_empty(), "{} of {n} differ:\n{}", d.len(), d.join("\n"));
}

/// `paths`, numeric path params, `namespaced` / `strip_prefix` path encodings,
/// `tristate`, `items_integer`, switch `else`, conditional constraints, `date-prefix` / `color`,
/// and the root (`""`) response pointer.
#[test]
fn gitlab_format_features() {
    let a = SpecAdapter::from_json(GITLAB).unwrap();
    let path = |r: HttpRequest| { let u = url::Url::parse(&r.url).unwrap(); (u.path().to_string(), u.query().unwrap_or("").to_string()) };
    let body = |r: HttpRequest| serde_json::from_slice::<Value>(r.body.as_ref().unwrap()).unwrap();
    // namespaced id + integer path param
    assert_eq!(path(a.build_request("GITLAB_GET_ISSUE", &json!({"project_id": "grp/sub", "issue_iid": 7})).unwrap()).0, "/api/v4/projects/grp%2Fsub/issues/7");
    assert!(a.build_request("GITLAB_GET_ISSUE", &json!({"project_id": "grp/../x", "issue_iid": 7})).is_err());
    // paths alternatives + strip_prefix
    assert_eq!(path(a.build_request("GITLAB_GET_USER_STATUS", &json!({"user": "@ada"})).unwrap()).0, "/api/v4/users/ada/status");
    assert_eq!(path(a.build_request("GITLAB_GET_USER_STATUS", &json!({})).unwrap()).0, "/api/v4/user/status");
    assert!(path(a.build_request("GITLAB_LIST_USERS", &json!({"username": "@ada"})).unwrap()).1.contains("username=ada"));
    // tristate + items_integer
    let b = body(a.build_request("GITLAB_CREATE_ISSUE", &json!({"project_id": "1", "title": "t", "confidential": "true", "assignee_ids": ["3", "4"]})).unwrap());
    assert_eq!((b["confidential"].clone(), b["assignee_ids"].clone()), (json!(true), json!([3, 4])));
    assert!(a.build_request("GITLAB_CREATE_ISSUE", &json!({"project_id": "1", "title": "t", "confidential": "yes"})).is_err());
    assert!(a.build_request("GITLAB_CREATE_ISSUE", &json!({"project_id": "1", "title": "t", "assignee_ids": ["x"]})).is_err());
    // switch with `else`: clear flag wins over the value
    let b = body(a.build_request("GITLAB_UPDATE_ISSUE", &json!({"project_id": "1", "issue_iid": 2, "clear_assignees": true, "assignee_ids": ["3"]})).unwrap());
    assert_eq!(b, json!({"assignee_ids": [0]}));
    let b = body(a.build_request("GITLAB_UPDATE_ISSUE", &json!({"project_id": "1", "issue_iid": 2, "clear_assignees": false, "assignee_ids": ["3"]})).unwrap());
    assert_eq!(b, json!({"assignee_ids": [3]}));
    assert!(a.build_request("GITLAB_UPDATE_ISSUE", &json!({"project_id": "1", "issue_iid": 2})).is_err(), "nonempty_body on an all-omitted body");
    // conditional constraint
    assert!(a.build_request("GITLAB_LIST_PROJECTS", &json!({"membership": "false"})).is_err());
    assert!(a.build_request("GITLAB_LIST_PROJECTS", &json!({"membership": "false", "owned": "false"})).is_err());
    assert!(!path(a.build_request("GITLAB_LIST_PROJECTS", &json!({"membership": "false", "owned": "true"})).unwrap()).1.contains("membership"));
    assert!(path(a.build_request("GITLAB_LIST_PROJECTS", &json!({})).unwrap()).1.contains("membership=true"));
    // formats
    assert!(a.build_request("GITLAB_LIST_COMMITS", &json!({"project_id": "1", "since": "2026-10-11T09:00:00Z"})).is_ok());
    assert!(a.build_request("GITLAB_LIST_COMMITS", &json!({"project_id": "1", "since": "yesterday"})).is_err());
    assert!(a.build_request("GITLAB_CREATE_LABEL", &json!({"project_id": "1", "name": "n", "color": "#abc"})).is_ok());
    assert!(a.build_request("GITLAB_CREATE_LABEL", &json!({"project_id": "1", "name": "n", "color": "#ggg"})).is_err());
    // root pointer list response
    let resp = HttpResponse { status: 200, session_id: None, body: br#"[{"id":1},{"id":2}]"#.to_vec() };
    assert_eq!(a.parse_response("GITLAB_LIST_LABELS", &resp).unwrap(), json!({"items": [{"id": 1}, {"id": 2}], "count": 2}));
}

const HUBSPOT: &str = include_str!("../specs/hubspot.json");

#[test]
fn hubspot_spec_matches_compiled_adapter() {
    let (n, d) = differential("hubspot", HUBSPOT);
    assert!(d.is_empty(), "{} of {n} differ:\n{}", d.len(), d.join("\n"));
}

/// Hybrid delegation, array templates, `$if`, `$merge`(+`$merge_wins`),
/// `stringify_values`, `prepend`, path encodings `digits` / `form`.
#[test]
fn hubspot_format_features() {
    let a = SpecAdapter::from_json(HUBSPOT).unwrap();
    let rust = builtin("hubspot");
    let body = |r: HttpRequest| serde_json::from_slice::<Value>(r.body.as_ref().unwrap()).unwrap();
    // hybrid: listed tools exist in the spec's tool list and are served by the compiled adapter
    assert_eq!(a.rust_actions().len(), 3);
    assert_eq!(a.spec().tools.len(), rust.spec().tools.len());
    let note = json!({"note_body": "x", "object_type": "contact", "object_id": "5", "timestamp": "2026-10-15T10:00:00Z"});
    assert_eq!(body(a.build_request("HUBSPOT_CREATE_NOTE", &note).unwrap()), body(rust.build_request("HUBSPOT_CREATE_NOTE", &note).unwrap()));
    assert!(a.risk("HUBSPOT_CREATE_NOTE").is_some_and(|r| r != ctox_toolhub::spec::validate::Risk::Read));
    // array template + `$if`
    let b = body(a.build_request("HUBSPOT_FIND_CONTACT", &json!({"value": "a@b.c"})).unwrap());
    assert_eq!(b["filterGroups"][0]["filters"].as_array().unwrap().len(), 1);
    assert_eq!(b["properties"][0], json!("firstname"), "prepend keeps the defaults first");
    let b = body(a.build_request("HUBSPOT_FIND_CONTACT", &json!({"value": "a@b.c", "property_name_2": "lastname", "value_2": "X", "properties": ["email", "custom"]})).unwrap());
    assert_eq!(b["filterGroups"][0]["filters"][1], json!({"propertyName": "lastname", "operator": "EQ", "value": "X"}));
    let props = b["properties"].as_array().unwrap();
    assert_eq!((props.iter().filter(|p| *p == "email").count(), props.last().unwrap().clone()), (1, json!("custom")));
    let b = body(a.build_request("HUBSPOT_SEARCH_OBJECTS", &json!({"object_type": "contacts"})).unwrap());
    assert!(b.get("sorts").is_none(), "array of only dropped items is omitted");
    // `$merge` + stringify: explicit field wins (upsert) vs. properties win (deal)
    let b = body(a.build_request("HUBSPOT_UPSERT_CONTACT", &json!({"email": "a@b.c", "properties": {"email": "other", "tags": ["x", "y"], "n": 3, "z": null}})).unwrap());
    assert_eq!(b["inputs"][0]["properties"], json!({"email": "a@b.c", "tags": "x;y", "n": "3"}));
    let b = body(a.build_request("HUBSPOT_CREATE_DEAL", &json!({"dealname": "A", "properties": {"dealname": "B"}})).unwrap());
    assert_eq!(b["properties"]["dealname"], json!("B"));
    // digits / form path encodings
    assert!(a.build_request("HUBSPOT_GET_OWNER_BY_ID", &json!({"owner_id": "12a"})).is_err());
    assert!(a.build_request("HUBSPOT_UNSUBSCRIBE_EMAIL", &json!({"email": "a+b@c.d"})).unwrap().url.ends_with("/a%2Bb%40c.d"));
    assert!(a.build_request("HUBSPOT_UNSUBSCRIBE_EMAIL", &json!({"email": "a/b"})).is_err());
}

#[test]
fn hybrid_spec_is_validated() {
    let mut doc: Value = serde_json::from_str(HUBSPOT).unwrap();
    doc["hybrid"]["rust_actions"] = json!(["HUBSPOT_NOPE"]);
    assert!(SpecAdapter::from_json(&doc.to_string()).unwrap_err().to_string().contains("no tool"));
    doc["hybrid"]["rust_actions"] = json!(["HUBSPOT_GET_CONTACT"]);
    assert!(SpecAdapter::from_json(&doc.to_string()).unwrap_err().to_string().contains("both"));
    doc["hybrid"] = json!({"rust_actions": ["HUBSPOT_CREATE_NOTE"]});
    assert!(SpecAdapter::from_json(&doc.to_string()).is_err(), "reason is required");
}

const MONDAY: &str = include_str!("../specs/monday.json");

#[test]
fn monday_spec_matches_compiled_adapter() {
    let (n, d) = differential("monday", MONDAY);
    assert!(d.is_empty(), "{} of {n} differ:\n{}", d.len(), d.join("\n"));
}

/// GraphQL error check, object `require`, `json_string`, template `json` escape, `each` defaults.
#[test]
fn monday_format_features() {
    let a = SpecAdapter::from_json(MONDAY).unwrap();
    let resp = |v: Value| HttpResponse { status: 200, session_id: None, body: v.to_string().into_bytes() };
    let gv = |r: HttpRequest| serde_json::from_slice::<Value>(r.body.as_ref().unwrap()).unwrap()["variables"].clone();
    // graphql: errors joined, then error_message, then missing data
    let e = a.parse_response("MONDAY_GET_ME", &resp(json!({"errors": [{"message": "A"}, {"message": "B"}]}))).unwrap_err().to_string();
    assert_eq!(e, "monday.com API error: A; B");
    let e = a.parse_response("MONDAY_GET_ME", &resp(json!({"errors": [{"code": 1}]}))).unwrap_err().to_string();
    assert_eq!(e, "monday.com API error: unknown");
    assert!(a.parse_response("MONDAY_GET_ME", &resp(json!({"error_message": "quota"}))).unwrap_err().to_string().contains("quota"));
    assert_eq!(a.parse_response("MONDAY_GET_ME", &resp(json!({"data": null}))).unwrap_err().to_string(), "monday.com API returned no data.");
    // object `require`
    assert_eq!(a.parse_response("MONDAY_GET_ME", &resp(json!({"data": {}}))).unwrap_err().to_string(), "monday.com returned no `me`.");
    assert!(a.parse_response("MONDAY_LIST_GROUPS", &resp(json!({"data": {"boards": []}}))).is_err());
    // each default: non-bool / missing -> false
    let out = a.parse_response("MONDAY_LIST_GROUPS", &resp(json!({"data": {"boards": [{"id": "1", "groups": [{"id": "g", "archived": true}, {"id": "h", "archived": "x"}]}]}}))).unwrap();
    assert_eq!((out["groups"][0]["archived"].clone(), out["groups"][1]["archived"].clone(), out["groups"][1]["deleted"].clone()), (json!(true), json!(false), json!(false)));
    // json_string + json escape
    let v = gv(a.build_request("MONDAY_SET_ITEM_COLUMN_VALUES", &json!({"board_id": "1", "item_id": "2", "column_values": {"text": "a\"b"}})).unwrap());
    assert_eq!(v["columnValues"], json!(r#"{"text":"a\"b"}"#));
    let v = gv(a.build_request("MONDAY_RENAME_ITEM", &json!({"board_id": "1", "item_id": "2", "name": "x \"y\"\nz"})).unwrap());
    assert_eq!(serde_json::from_str::<Value>(v["columnValues"].as_str().unwrap()).unwrap(), json!({"name": "x \"y\"\nz"}));
    assert_eq!(a.rust_actions().len(), 14);
}

const GITHUB: &str = include_str!("../specs/github.json");

#[test]
fn github_spec_matches_compiled_adapter() {
    let (n, d) = differential("github", GITHUB);
    assert!(d.is_empty(), "{} of {n} differ:\n{}", d.len(), d.join("\n"));
}

/// `segments` path encoding + template escape, `validate_only`, response mode `array`.
#[test]
fn github_format_features() {
    let a = SpecAdapter::from_json(GITHUB).unwrap();
    let resp = |v: Value| HttpResponse { status: 200, session_id: None, body: v.to_string().into_bytes() };
    // segments: slashes kept, each part encoded and checked
    let r = a.build_request("GITHUB_DELETE_BRANCH", &json!({"owner": "o", "repo": "r", "branch": "grp/sub"})).unwrap();
    assert!(r.url.ends_with("/git/refs/heads/grp/sub"), "{}", r.url);
    assert!(a.build_request("GITHUB_DELETE_BRANCH", &json!({"owner": "o", "repo": "r", "branch": "grp/../x"})).is_err());
    let r = a.build_request("GITHUB_CREATE_BRANCH", &json!({"owner": "o", "repo": "r", "new_branch_name": "f/x", "source_sha": "abc"})).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(r.body.as_ref().unwrap()).unwrap()["ref"], json!("refs/heads/f/x"));
    // validate_only: checked, never sent
    let ok = json!({"owner": "o", "repo": "r", "discussion_id": "D_1", "body": "hi"});
    let r = a.build_request("GITHUB_CREATE_DISCUSSION_COMMENT", &ok).unwrap();
    assert!(!String::from_utf8_lossy(r.body.as_ref().unwrap()).contains("\"o\""));
    let mut bad = ok.clone(); bad["owner"] = json!("a/b");
    assert!(a.build_request("GITHUB_CREATE_DISCUSSION_COMMENT", &bad).is_err());
    let mut miss = ok.clone(); miss.as_object_mut().unwrap().remove("repo");
    assert!(a.build_request("GITHUB_CREATE_DISCUSSION_COMMENT", &miss).is_err());
    // array mode: top-level list projected per item
    let out = a.parse_response("GITHUB_LIST_MY_REPOSITORIES", &resp(json!([{"id": 1, "name": "x", "extra": 9}]))).unwrap();
    assert_eq!(out[0]["name"], json!("x"));
    assert!(out[0].get("extra").is_none());
    assert_eq!(a.rust_actions().len(), 10);
}

const NOTION: &str = include_str!("../specs/notion.json");

#[test]
fn notion_spec_matches_compiled_adapter() {
    let (n, d) = differential("notion", NOTION);
    assert!(d.is_empty(), "{} of {n} differ:\n{}", d.len(), d.join("\n"));
    let a = SpecAdapter::from_json(NOTION).unwrap();
    assert_eq!(a.rust_actions().len(), 16);
    // Rust part still converts markdown to blocks through the hybrid.
    let r = a.build_request("NOTION_APPEND_TO_PAGE", &json!({"block_id": "b1", "content": "# T"})).unwrap();
    assert!(String::from_utf8_lossy(r.body.as_ref().unwrap()).contains("heading_1"));
}

// ------------------------------------------------------------------ dogfood format fixes (2026-10-17)

const OPEN_METEO: &str = include_str!("../specs/open_meteo.json");
const FRANKFURTER: &str = include_str!("../specs/frankfurter.json");
const NAGER_DATE: &str = include_str!("../specs/nager_date.json");

#[test]
fn number_args_keep_decimals_and_integers_stay_whole() {
    let a = SpecAdapter::from_json(OPEN_METEO).unwrap();
    let r = a.build_request("OPEN_METEO_GET_FORECAST", &json!({"latitude": 52.52, "longitude": "13.41", "current": ["temperature_2m", "wind_speed_10m"], "forecast_days": 40})).unwrap();
    assert!(r.url.contains("latitude=52.52") && r.url.contains("longitude=13.41"), "{}", r.url);
    assert!(r.url.contains("current=temperature_2m%2Cwind_speed_10m") || r.url.contains("current=temperature_2m,wind_speed_10m"), "{}", r.url);
    assert!(r.url.contains("forecast_days=16") && r.url.contains("timezone=auto"), "{}", r.url);
    assert!(a.build_request("OPEN_METEO_GET_FORECAST", &json!({"latitude": "north", "longitude": 1})).is_err());
    let f = SpecAdapter::from_json(FRANKFURTER).unwrap();
    let r = f.build_request("FRANKFURTER_GET_LATEST_RATES", &json!({"amount": 10})).unwrap();
    assert!(r.url.ends_with("amount=10"), "{}", r.url);
}

#[test]
fn dot_dot_is_text_inside_a_segment_but_never_a_segment() {
    let f = SpecAdapter::from_json(FRANKFURTER).unwrap();
    let r = f.build_request("FRANKFURTER_GET_TIME_SERIES", &json!({"start_date": "2026-10-01", "end_date": "2026-10-09", "symbols": ["USD"]})).unwrap();
    assert!(r.url.starts_with("https://api.frankfurter.dev/v1/2026-10-01..2026-10-09?"), "{}", r.url);
    assert!(f.build_request("FRANKFURTER_GET_TIME_SERIES", &json!({"start_date": "a/b", "end_date": "x"})).is_err());
    rejects(|v| v["actions"][0]["request"]["path"] = json!("/items/.."), "path must");
    rejects(|v| v["actions"][0]["request"]["path"] = json!("/./items/{id}"), "path must");
}

#[test]
fn status_mode_maps_empty_2xx_answers() {
    let a = SpecAdapter::from_json(NAGER_DATE).unwrap();
    let r = a.build_request("NAGER_DATE_IS_TODAY_PUBLIC_HOLIDAY", &json!({"country_code": "DE", "county_code": "DE-BY"})).unwrap();
    assert_eq!(r.url, "https://date.nager.at/api/v3/IsTodayPublicHoliday/DE?countyCode=DE-BY");
    let resp = |status| HttpResponse { status, body: vec![], session_id: None };
    assert_eq!(a.parse_response("NAGER_DATE_IS_TODAY_PUBLIC_HOLIDAY", &resp(200)).unwrap(), json!({"public_holiday": true}));
    assert_eq!(a.parse_response("NAGER_DATE_IS_TODAY_PUBLIC_HOLIDAY", &resp(204)).unwrap(), json!({"public_holiday": false}));
    assert!(a.parse_response("NAGER_DATE_IS_TODAY_PUBLIC_HOLIDAY", &resp(202)).is_err());
    rejects(|v| v["actions"][0]["response"] = json!({"mode": "status", "values": {"404": false}}), "2xx");
    rejects(|v| v["actions"][0]["response"] = json!({"mode": "status"}), "values");
}
