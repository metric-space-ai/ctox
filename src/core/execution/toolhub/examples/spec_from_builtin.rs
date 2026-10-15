//! Builder-agent helper: draft a hot-plug spec (`ctox.adapter-spec/v1`) from a
//! compiled adapter by *probing* it, not by reading its source.
//!
//! For every tool it copies title/description/scopes/input_schema/risk and infers
//! method, path params, query args, JSON/GraphQL body and a response mode by calling
//! `build_request` / `parse_response` with marked sample values. The draft is a
//! starting point: run the differential test (`tests/spec_adapters.rs`) and fix the
//! reported cases by hand (clamps, defaults, conditionals, response shapes).
//!
//! `cargo run --example spec_from_builtin -- <slug> > specs/<slug>.json`
use ctox_toolhub::http::HttpResponse;
use ctox_toolhub::toolkits::{self, ToolkitAdapter};
use serde_json::{json, Map, Value};

pub fn sample(name: &str, prop: &Value, i: usize) -> Value {
    if let Some(e) = prop["enum"].as_array() {
        return e[0].clone();
    }
    let n = name.to_ascii_lowercase();
    match prop["type"].as_str() {
        Some("integer") | Some("number") => {
            let lo = prop["minimum"].as_i64().unwrap_or(0);
            let hi = prop["maximum"].as_i64().unwrap_or(1000);
            json!((lo + 3 + i as i64).min(hi).max(lo))
        }
        Some("boolean") => json!(true),
        Some("array") => {
            let item = sample(name, &prop["items"], i);
            match item {
                Value::String(s) => json!([format!("{s}a"), format!("{s}b")]),
                o => json!([o]),
            }
        }
        Some("object") => json!({format!("k{i}"): format!("v{i}")}),
        _ if n.contains("email") => json!(format!("u{i}@example.com")),
        _ if n.contains("url") || n.contains("uri") => json!(format!("https://example.com/p{i}")),
        _ if n.ends_with("_on") || n.ends_with("date") || n.contains("_on_") => json!(format!("2026-10-{:02}", 10 + i % 18)),
        _ if n.ends_with("_at") || n.contains("time") || n == "since" || n == "until" || n.contains("_at_") => {
            json!(format!("2026-10-{:02}T10:00:00Z", 10 + i % 18))
        }
        _ => json!(format!("s{i}x{}", n.len())),
    }
}

fn args_for(schema: &Value, required_only: bool) -> (Map<String, Value>, Vec<(String, Value)>) {
    let props = schema["properties"].as_object().cloned().unwrap_or_default();
    let req: Vec<String> = schema["required"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(String::from)).collect();
    let mut m = Map::new();
    let mut samples = vec![];
    for (i, (k, p)) in props.iter().enumerate() {
        let v = sample(k, p, i);
        samples.push((k.clone(), v.clone()));
        if !required_only || req.contains(k) {
            m.insert(k.clone(), v);
        }
    }
    (m, samples)
}

fn arg_of<'a>(v: &Value, samples: &'a [(String, Value)]) -> Option<&'a str> {
    samples.iter().find(|(_, s)| s == v || (s.is_number() && v.as_str() == Some(&s.to_string())) || (s.is_boolean() && v.as_str() == Some(&s.to_string()))).map(|(k, _)| k.as_str())
}

/// Map a concrete JSON value back to a template (`{"arg": …}` / `{"const": …}` / nested).
fn template(v: &Value, samples: &[(String, Value)], schema_props: &Value) -> Value {
    if let Some(a) = arg_of(v, samples) {
        let mut t = json!({"arg": a});
        if schema_props[a]["type"] == "string" {
            t["trim"] = json!(true);
        }
        if schema_props[a]["type"] == "array" && schema_props[a]["items"]["type"] != "string" {
            t["type"] = json!("raw");
        }
        return t;
    }
    if let Value::Array(items) = v {
        // joined list arg sent as a string?
        for (k, s) in samples {
            if let Value::Array(sv) = s {
                if sv == items {
                    let ty = if schema_props[k.as_str()]["items"]["type"] == "string" { "string_list" } else { "raw" };
                    return json!({"arg": k, "type": ty});
                }
            }
        }
    }
    if let Value::String(s) = v {
        for (k, sv) in samples {
            if let Value::Array(items) = sv {
                let parts: Vec<&str> = items.iter().filter_map(Value::as_str).collect();
                for sep in [",", " ", ";"] {
                    if !parts.is_empty() && parts.join(sep) == *s {
                        return json!({"arg": k, "type": "string_list", "join": sep});
                    }
                }
            }
        }
    }
    match v {
        Value::Object(m) if !m.is_empty() => {
            let mut out = Map::new();
            for (k, x) in m {
                out.insert(k.clone(), template(x, samples, schema_props));
            }
            Value::Object(out)
        }
        other => json!({"const": other}),
    }
}

/// Post-process a template: mark required args, defaults seen in the required-only
/// request, and `$omit_empty` on objects that vanish when optional args are absent.
fn refine(t: &mut Value, min: Option<&Value>, required: &[String]) {
    if let Some(name) = t.get("arg").and_then(Value::as_str).map(String::from) {
        if required.contains(&name) {
            t["required"] = json!(true);
        } else if let Some(m) = min {
            if t.get("default").is_none() && !m.is_null() {
                t["default"] = m.clone();
            }
        }
        return;
    }
    if t.get("const").is_some() {
        return;
    }
    if let Some(obj) = t.as_object_mut() {
        let keys: Vec<String> = obj.keys().cloned().collect();
        let absent = min.is_none() || min.is_some_and(Value::is_null);
        for k in keys {
            let mv = min.and_then(|m| m.get(&k));
            refine(obj.get_mut(&k).unwrap(), mv, required);
        }
        if absent && min.is_none() {
            obj.insert("$omit_empty".into(), json!(true));
        }
    }
}

fn infer_request(a: &dyn ToolkitAdapter, tool: &str, schema: &Value, api_base: &str) -> Value {
    let (all, samples) = args_for(schema, false);
    let Ok(r) = a.build_request(tool, &Value::Object(all)) else {
        eprintln!("TODO {tool}: build_request failed with sample args");
        return json!({"method": "GET", "path": "/TODO"});
    };
    let props = &schema["properties"];
    let url = url::Url::parse(&r.url).unwrap();
    let base_path = url::Url::parse(api_base).map(|u| u.path().trim_end_matches('/').to_string()).unwrap_or_default();
    let raw_path = url.path().strip_prefix(&base_path).unwrap_or(url.path()).to_string();
    let mut path = String::new();
    let mut enc = Map::new();
    for seg in raw_path.split('/').skip(1) {
        let dec = urlencoding_decode(seg);
        path.push('/');
        match arg_of(&json!(dec), &samples) {
            Some(k) => {
                path.push_str(&format!("{{{k}}}"));
                let mut spaced = args_for(schema, false).0;
                spaced.insert(k.into(), json!(format!("{} x", dec)));
                if dec != seg || dec.contains(['@', ' ', ':']) || a.build_request(tool, &Value::Object(spaced)).is_ok() {
                    enc.insert(k.into(), json!("component"));
                }
            }
            None => path.push_str(seg),
        }
    }
    let (req_only, _) = args_for(schema, true);
    let r_min = a.build_request(tool, &Value::Object(req_only)).ok();
    let min_q: Vec<(String, String)> = r_min.as_ref().and_then(|r| url::Url::parse(&r.url).ok()).map(|u| u.query_pairs().map(|(a, b)| (a.into(), b.into())).collect()).unwrap_or_default();
    let mut query = Map::new();
    for (k, v) in url.query_pairs() {
        let t = template(&json!(v), &samples, props);
        let t = match (t.get("arg").and_then(Value::as_str), min_q.iter().find(|(mk, _)| mk == &k)) {
            (Some(arg), Some((_, mv))) if mv != &v && arg_of(&json!(mv), &samples).is_none() => {
                let d = mv.parse::<i64>().map(Value::from).unwrap_or(json!(mv));
                let mut t = t.clone();
                t["default"] = d;
                t
            }
            _ => t,
        };
        query.insert(k.into_owned(), t);
    }
    let mut req = json!({"method": r.method, "path": path});
    if r_min.is_none() {
        // Fails with required args only: usually "supply at least one of …".
        let (only, _) = args_for(schema, true);
        if let Err(e) = a.build_request(tool, &Value::Object(only)) {
            req["nonempty_body"] = json!(e.to_string());
        }
    }
    if !enc.is_empty() {
        req["path_encoding"] = Value::Object(enc);
    }
    if !query.is_empty() {
        req["query"] = Value::Object(query);
    }
    if let Some(b) = &r.body {
        let ct = r.headers.get("content-type").cloned().unwrap_or_default();
        if ct.contains("x-www-form-urlencoded") {
            let mut f = Map::new();
            for (k, v) in url::form_urlencoded::parse(b) {
                f.insert(k.into_owned(), template(&json!(v), &samples, props));
            }
            req["body"] = json!({"form": f});
        } else if let Ok(j) = serde_json::from_slice::<Value>(b) {
            if let (Some(q), true) = (j["query"].as_str(), j.get("variables").is_some() || j.as_object().is_some_and(|m| m.len() == 1)) {
                req["body"] = json!({"graphql": {"query": q, "variables": template(&j["variables"], &samples, props)}});
                if !q.trim_start().starts_with("mutation") && r.method == "POST" {
                    req["read_only_post"] = json!(true);
                }
            } else {
                let mut t = template(&j, &samples, props);
                let min_body: Option<Value> = r_min.as_ref().and_then(|r| r.body.as_ref()).and_then(|b| serde_json::from_slice(b).ok());
                let required: Vec<String> = schema["required"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(String::from)).collect();
                if let Some(m) = t.as_object_mut() {
                    for (k, v) in m.iter_mut() {
                        refine(v, min_body.as_ref().and_then(|b| b.get(k)), &required);
                    }
                }
                req["body"] = json!({"json": t});
            }
        }
    }
    for (k, v) in &r.headers {
        if !["content-type", "accept", "user-agent"].contains(&k.as_str()) {
            req["headers"].as_object_mut().map(|_| ()).unwrap_or_else(|| req["headers"] = json!({}));
            req["headers"][k] = json!(v);
        }
    }
    req
}

fn urlencoding_decode(s: &str) -> String {
    url::form_urlencoded::parse(format!("x={s}").as_bytes()).next().map(|(_, v)| v.into_owned()).unwrap_or_else(|| s.into())
}

/// Response keys the compiled adapter reads (`v["key"]`, `.get("key")`), from its source.
fn source_keys(slug: &str) -> Vec<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("src/toolkits/{slug}.rs"));
    let src = std::fs::read_to_string(path).unwrap_or_default();
    let mut keys = std::collections::BTreeSet::new();
    for pat in ["[\"", ".get(\""] {
        for (i, _) in src.match_indices(pat) {
            let rest = &src[i + pat.len()..];
            if let Some(end) = rest.find('"') {
                let k = &rest[..end];
                if !k.is_empty() && k.len() < 40 && k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
                    keys.insert(k.to_string());
                }
            }
        }
    }
    keys.into_iter().collect()
}

fn parse_with(a: &dyn ToolkitAdapter, tool: &str, body: &Value) -> Option<Value> {
    a.parse_response(tool, &HttpResponse { status: 200, body: serde_json::to_vec(body).unwrap(), session_id: None }).ok()
}

/// Infer the response mapping by marker probing: every key the adapter's source
/// mentions gets a unique marker value (scalar, nested object, list of items); the
/// compiled adapter's output then shows where each output field comes from.
fn infer_response(a: &dyn ToolkitAdapter, tool: &str, keys: &[String]) -> Value {
    let m = |p: &str| json!(format!("M:{p}"));
    // Keys that make the adapter report a provider error (error/errors/ok…) are left out.
    let keys: Vec<String> = keys.iter().filter(|k| parse_with(a, tool, &json!({k.as_str(): format!("M:{k}")})).is_some() && parse_with(a, tool, &json!({k.as_str(): [{"x": 1}]})).is_some()).cloned().collect();
    let keys = &keys;
    let scalar: Map<String, Value> = keys.iter().map(|k| (k.clone(), m(k))).collect();
    let nested: Map<String, Value> = keys.iter().map(|k| (k.clone(), Value::Object(keys.iter().map(|x| (x.clone(), m(&format!("{k}/{x}")))).collect()))).collect();
    let lists: Map<String, Value> = keys.iter().map(|k| (k.clone(), json!([Value::Object(keys.iter().map(|x| (x.clone(), m(&format!("{k}[]/{x}")))).collect())]))).collect();
    let (b_scalar, b_nested, b_lists) = (Value::Object(scalar), Value::Object(nested), Value::Object(lists));
    let (o_s, o_n, o_l, o_e) = (parse_with(a, tool, &b_scalar), parse_with(a, tool, &b_nested), parse_with(a, tool, &b_lists), parse_with(a, tool, &json!({})));
    if o_s.as_ref() == Some(&b_scalar) && o_l.as_ref() == Some(&b_lists) {
        return Value::Null; // raw body passthrough
    }
    let marker = |v: &Value| v.as_str().and_then(|s| s.strip_prefix("M:")).map(String::from);
    // pick of a list? (whole output is one container's items)
    if let Some(Value::Array(items)) = &o_l {
        if let Some(first) = items.first() {
            if let Some(k) = first.as_object().and_then(|o| o.values().find_map(|v| marker(v))).and_then(|p| p.split_once("[]/").map(|(c, _)| c.to_string())) {
                if Some(first) == b_lists.get(&k).and_then(|l| l.get(0)) {
                    return json!({"mode": "pick", "pointer": format!("/{k}")});
                }
            }
        }
    }
    let Some(Value::Object(out_s)) = &o_s else {
        eprintln!("TODO {tool}: response mapping not inferred (output is not an object)");
        return Value::Null;
    };
    let mut fields = Map::new();
    let mut containers = vec![];
    for (ok, vs) in out_s {
        let vn = o_n.as_ref().and_then(|o| o.get(ok));
        let vl = o_l.as_ref().and_then(|o| o.get(ok));
        let ve = o_e.as_ref().and_then(|o| o.get(ok)).filter(|v| !v.is_null());
        let mut f = if let Some(p) = marker(vs) {
            json!({"pointer": format!("/{p}")})
        } else if let Some(p) = vn.and_then(|v| marker(v)) {
            json!({"pointer": format!("/{p}")})
        } else if let Some(Value::Array(items)) = vl {
            match items.first().and_then(Value::as_object) {
                Some(item) if item.values().all(|v| v.is_null() || marker(v).is_some_and(|p| p.contains("[]/"))) && !item.is_empty() => {
                    let mut each = Map::new();
                    let mut container = None;
                    for (ik, iv) in item {
                        match marker(iv) {
                            Some(p) => {
                                let (c, x) = p.split_once("[]/").unwrap();
                                container = Some(c.to_string());
                                each.insert(ik.clone(), json!(format!("/{x}")));
                            }
                            None => { each.insert(ik.clone(), json!("/__ctox_missing__")); }
                        }
                    }
                    let c = container.unwrap_or_default();
                    containers.push(c.clone());
                    json!({"pointer": format!("/{c}"), "each": each})
                }
                Some(_) => {
                    // whole items copied: find the container whose list equals this one
                    match lists_key(&b_lists, items) {
                        Some(c) => { containers.push(c.clone()); json!({"pointer": format!("/{c}")}) }
                        None => { eprintln!("TODO {tool}: field `{ok}` list shape not inferred"); json!({"pointer": "/__ctox_missing__"}) }
                    }
                }
                None => json!({"pointer": "/__ctox_missing__"}),
            }
        } else if vl.is_some_and(|v| v == &json!(1)) && ve.is_none_or(|v| v == &json!(0)) {
            json!({"pointer": "/__COUNT__", "count": true})
        } else {
            eprintln!("TODO {tool}: field `{ok}` source not found");
            json!({"pointer": "/__ctox_missing__"})
        };
        if let Some(d) = ve {
            if f.get("count").is_none() && f.get("each").is_none() {
                f["default"] = d.clone();
            }
        }
        fields.insert(ok.clone(), f);
    }
    for f in fields.values_mut() {
        if f["pointer"] == "/__COUNT__" {
            match containers.first() {
                Some(c) => f["pointer"] = json!(format!("/{c}")),
                None => eprintln!("TODO {tool}: count field without a list container"),
            }
        }
    }
    json!({"mode": "object", "fields": fields})
}

fn lists_key(b: &Value, items: &[Value]) -> Option<String> {
    b.as_object()?.iter().find(|(_, v)| v.as_array().is_some_and(|a| a.as_slice() == items)).map(|(k, _)| k.clone())
}

#[allow(dead_code)]
fn find_pointer(hay: &Value, needle: &Value) -> Option<String> {
    if hay == needle {
        return Some(String::new());
    }
    match hay {
        Value::Object(m) => m.iter().find_map(|(k, v)| find_pointer(v, needle).map(|p| format!("/{k}{p}"))),
        Value::Array(a) => a.iter().enumerate().find_map(|(i, v)| find_pointer(v, needle).map(|p| format!("/{i}{p}"))),
        _ => None,
    }
}

#[allow(dead_code)]
fn simulate(resp: &Value, body: &Value) -> Option<Value> {
    if resp.is_null() {
        return Some(body.clone());
    }
    body.pointer(resp["pointer"].as_str().unwrap_or("")).filter(|v| !v.is_null()).cloned()
}

/// Longest common `scheme://host/path-prefix` over every tool's probe request.
fn infer_api_base(a: &dyn ToolkitAdapter) -> String {
    let urls: Vec<url::Url> = a.spec().tools.iter().filter_map(|t| {
        let (all, _) = args_for(&t.input_schema, false);
        a.build_request(&t.slug, &Value::Object(all)).ok().and_then(|r| url::Url::parse(&r.url).ok())
    }).collect();
    let Some(first) = urls.first() else { return "https://TODO".into() };
    let host = first.host_str().unwrap_or("TODO").to_string();
    let mut prefix: Vec<String> = first.path().split('/').skip(1).map(String::from).collect();
    for u in &urls {
        let segs: Vec<&str> = u.path().split('/').skip(1).collect();
        let n = prefix.iter().zip(&segs).take_while(|(a, b)| a.as_str() == **b).count();
        prefix.truncate(n.min(segs.len().saturating_sub(1)));
    }
    let p = prefix.join("/");
    if p.is_empty() { format!("https://{host}") } else { format!("https://{host}/{p}") }
}

fn main() {
    let slug = std::env::args().nth(1).expect("usage: spec_from_builtin <slug>");
    let a = toolkits::builtin().into_iter().find(|a| a.spec().slug == slug).unwrap_or_else(|| panic!("no builtin `{slug}`"));
    println!("{}", serde_json::to_string_pretty(&draft(a.as_ref())).unwrap());
}

pub fn draft(a: &dyn ToolkitAdapter) -> Value {
    let s = a.spec();
    let api_base = a.default_api_base().unwrap_or_else(|| infer_api_base(a));
    let host = url::Url::parse(&api_base).ok().and_then(|u| u.host_str().map(String::from)).unwrap_or_default();
    let keys = source_keys(&s.slug);
    let actions: Vec<Value> = s.tools.iter().map(|t| {
        let mut req = infer_request(a, &t.slug, &t.input_schema, &api_base);
        let risk = if !t.mutating { "read" } else if req["method"] == "DELETE" { "destructive" } else { "write" };
        if risk == "read" && req["method"] != "GET" {
            req["read_only_post"] = json!(true);
        }
        let mut act = json!({
            "slug": t.slug, "title": t.title, "description": t.description, "risk": risk,
            "scopes": t.scopes, "input_schema": t.input_schema, "request": req,
        });
        let used = req.to_string();
        for k in t.input_schema["properties"].as_object().into_iter().flatten().map(|(k, _)| k) {
            if !used.contains(&format!("\"arg\":\"{k}\"")) && !used.contains(&format!("{{{k}}}")) {
                eprintln!("TODO {}: property `{k}` not mapped (conditional/derived in the compiled adapter)", t.slug);
            }
        }
        let resp = infer_response(a, &t.slug, &keys);
        if !resp.is_null() {
            act["response"] = resp;
        }
        act
    }).collect();
    json!({
        "schema": "ctox.adapter-spec/v1", "slug": s.slug, "version": "1.0.0",
        "notes": format!("Drafted by examples/spec_from_builtin.rs from the compiled adapter src/toolkits/{}.rs; verified by the differential test.", s.slug),
        "api_base": api_base, "allowed_hosts": [host], "replaces_builtin": true,
        "name": s.name, "description": s.description, "auth_schemes": s.auth_schemes,
        "connection_fields": a.connection_fields(),
        "actions": actions,
    })
}
