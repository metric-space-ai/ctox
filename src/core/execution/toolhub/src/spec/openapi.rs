//! OpenAPI 3.x / Swagger 2.0 -> adapter-spec draft (`ctox toolhub draft-spec --from-openapi`).
//!
//! DE: Bildet Operationen auf Aktionen ab (Risiko nach Methode), schlägt die
//! Anmeldung vor, setzt die Host-Allowlist auf den Server-Host und entwirft
//! Lese-Proben aus den Beispielen der Doku. Das Ergebnis ist ein Entwurf: er
//! muss validieren und live bestehen wie jeder andere Spec.
//!
//! EN: Maps operations to actions (risk by method), suggests auth, sets the
//! host allowlist to the server host and drafts read probes from the doc's
//! examples. The output is a draft: it must validate and pass a live test like
//! any other spec. What it cannot map is listed in `skipped` / `notes`.
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Map, Value};

#[derive(Debug, Clone)]
pub struct Options {
    /// Toolkit slug; default from `info.title`.
    pub slug: Option<String>,
    pub version: String,
    /// Keep at most this many actions (reads first).
    pub max_actions: usize,
    /// Only these operations (operationId or generated action slug), if set.
    pub operations: Vec<String>,
    /// Where the document came from (URL or file), for `docs` and relative servers.
    pub source: String,
}

impl Default for Options {
    fn default() -> Self {
        Self { slug: None, version: "1.0.0".into(), max_actions: 40, operations: vec![], source: String::new() }
    }
}

#[derive(Debug, Clone)]
pub struct Draft {
    pub spec: Value,
    /// `[{tool, args, expect}]` read probes built from examples.
    pub probes: Vec<Value>,
    /// Read actions without a probe: which required args had no example.
    pub probes_missing: Vec<Value>,
    /// Operations not mapped, with the reason.
    pub skipped: Vec<Value>,
    pub notes: Vec<String>,
    /// Auth suggestion and why.
    pub auth: Value,
}

/// JSON or YAML text -> JSON value.
pub fn parse_document(text: &str) -> Result<Value> {
    let t = text.trim_start_matches('\u{feff}').trim_start();
    let v: Value = if t.starts_with('{') {
        serde_json::from_str(t).context("OpenAPI document is not valid JSON")?
    } else {
        serde_yaml::from_str(t).context("OpenAPI document is neither JSON nor YAML")?
    };
    if v.get("openapi").is_none() && v.get("swagger").is_none() {
        bail!("not an OpenAPI/Swagger document (no `openapi` or `swagger` field)");
    }
    Ok(v)
}

const METHODS: &[&str] = &["get", "post", "put", "patch", "delete"];

/// Follow local `$ref`s (`#/...`), at most 12 hops.
fn deref<'a>(doc: &'a Value, mut v: &'a Value) -> Option<&'a Value> {
    for _ in 0..12 {
        match v.get("$ref").and_then(Value::as_str) {
            None => return Some(v),
            Some(r) => {
                let p = r.strip_prefix('#')?;
                v = doc.pointer(p)?;
            }
        }
    }
    None
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

/// First sentence-ish, markdown-light, at most `max` chars.
fn clean(s: &str, max: usize) -> String {
    let one: String = s.replace(['\r', '\n', '\t'], " ").split_whitespace().collect::<Vec<_>>().join(" ");
    let one = one.replace("**", "").replace('`', "");
    if one.chars().count() <= max {
        return one;
    }
    let cut: String = one.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

fn snake(s: &str, upper: bool) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            if c.is_ascii_uppercase() && prev_lower {
                out.push('_');
            }
            prev_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
            out.push(if upper { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() });
        } else {
            if !out.ends_with('_') && !out.is_empty() {
                out.push('_');
            }
            prev_lower = false;
        }
    }
    out.trim_matches('_').to_string()
}

fn arg_name(raw: &str, taken: &BTreeSet<String>) -> String {
    let ok = !raw.is_empty() && raw.len() <= 60 && raw.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') && !raw.as_bytes()[0].is_ascii_digit();
    let mut n = if ok { raw.to_string() } else { snake(raw, false) };
    if n.is_empty() || n.as_bytes()[0].is_ascii_digit() {
        n = format!("p_{n}");
    }
    let base = n.clone();
    let mut i = 2;
    while taken.contains(&n) {
        n = format!("{base}_{i}");
        i += 1;
    }
    n
}

/// `(api_base, host)` from `servers` (OAS 3) or `host`/`basePath` (Swagger 2).
fn server(doc: &Value, source: &str, notes: &mut Vec<String>) -> Result<(String, String)> {
    let raw = if let Some(s) = doc.pointer("/servers/0") {
        let mut u = s.get("url").and_then(Value::as_str).unwrap_or("/").to_string();
        if let Some(vars) = s.get("variables").and_then(Value::as_object) {
            for (k, v) in vars {
                let d = v.get("default").map(|d| d.as_str().map(String::from).unwrap_or_else(|| d.to_string())).unwrap_or_default();
                u = u.replace(&format!("{{{k}}}"), &d);
            }
        }
        if doc.get("servers").and_then(Value::as_array).is_some_and(|a| a.len() > 1) {
            notes.push(format!("several servers listed; using the first ({u})"));
        }
        u
    } else if let Some(h) = text(doc, "host") {
        let schemes: Vec<&str> = doc.get("schemes").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
        let scheme = if schemes.is_empty() || schemes.contains(&"https") { "https" } else { schemes[0] };
        format!("{scheme}://{h}{}", text(doc, "basePath").unwrap_or_default())
    } else {
        "/".to_string()
    };
    let mut url = match url::Url::parse(&raw) {
        Ok(u) => u,
        Err(_) => {
            let base = url::Url::parse(source).map_err(|_| anyhow!("server URL `{raw}` is relative and the document has no URL source; pass --base-url"))?;
            notes.push(format!("server URL `{raw}` is relative; resolved against the document URL"));
            base.join(&raw)?
        }
    };
    if url.scheme() == "http" {
        notes.push(format!("server `{url}` is http; the spec uses https (only https is allowed)"));
        url.set_scheme("https").map_err(|_| anyhow!("cannot switch `{url}` to https"))?;
    }
    if url.scheme() != "https" {
        bail!("server scheme `{}` is not supported", url.scheme());
    }
    if url.port().is_some() {
        notes.push(format!("server `{url}` names a port; ports are not supported in allowed_hosts"));
    }
    let host = url.host_str().ok_or_else(|| anyhow!("server URL `{url}` has no host"))?.to_ascii_lowercase();
    let base = format!("https://{host}{}", url.path().trim_end_matches('/'));
    Ok((base, host))
}

fn auth(doc: &Value, notes: &mut Vec<String>) -> Value {
    let schemes = doc.pointer("/components/securitySchemes").or_else(|| doc.get("securityDefinitions")).and_then(Value::as_object);
    let global: Vec<&Value> = doc.get("security").and_then(Value::as_array).map(|a| a.iter().collect()).unwrap_or_default();
    let ops_use_security = doc.get("paths").and_then(Value::as_object).is_some_and(|p| {
        p.values().any(|item| METHODS.iter().any(|m| item.get(*m).and_then(|o| o.get("security")).and_then(Value::as_array).is_some_and(|a| a.iter().any(|r| r.as_object().is_some_and(|o| !o.is_empty())))))
    });
    let optional = global.iter().any(|r| r.as_object().is_some_and(Map::is_empty));
    let Some(schemes) = schemes.filter(|s| !s.is_empty()) else {
        return json!({"scheme": {"type": "none"}, "why": "the document declares no security schemes"});
    };
    if (global.is_empty() && !ops_use_security) || optional {
        return json!({"scheme": {"type": "none"}, "why": if optional { "security is optional (an empty requirement is listed)" } else { "security schemes are declared but no operation requires one" }, "declared": schemes.keys().collect::<Vec<_>>()});
    }
    let (name, s) = schemes.iter().next().unwrap();
    let s = deref(doc, s).unwrap_or(s);
    let kind = s.get("type").and_then(Value::as_str).unwrap_or("");
    let scheme = match (kind, s.get("scheme").and_then(Value::as_str).map(str::to_ascii_lowercase).as_deref(), s.get("in").and_then(Value::as_str)) {
        ("http", Some("bearer"), _) => json!({"type": "bearer"}),
        ("http", Some("basic"), _) | ("basic", _, _) => json!({"type": "basic"}),
        ("apiKey", _, Some("header")) => {
            let h = text(s, "name").unwrap_or_else(|| "X-API-Key".into());
            if h.eq_ignore_ascii_case("authorization") {
                json!({"type": "api_key", "header_name": "Authorization", "prefix": ""})
            } else {
                json!({"type": "api_key", "header_name": h})
            }
        }
        ("apiKey", _, Some(place)) => {
            notes.push(format!("API key in {place} (`{name}`) is not supported by the spec format (keys never go into URLs); use a compiled adapter or ask the provider for a header key"));
            return json!({"scheme": null, "why": format!("apiKey in {place}"), "unsupported": true});
        }
        ("oauth2", _, _) | ("openIdConnect", _, _) => {
            notes.push(format!("`{name}` is OAuth2/OIDC; drafted as bearer (personal or app token). Managed OAuth needs a provider entry in subscription_auth"));
            json!({"type": "bearer"})
        }
        _ => {
            notes.push(format!("security scheme `{name}` ({kind}) not recognised; drafted as bearer"));
            json!({"type": "bearer"})
        }
    };
    json!({"scheme": scheme, "why": format!("security scheme `{name}` ({kind})")})
}

/// Example value of a parameter or schema, if the doc gives one.
fn example(doc: &Value, p: &Value, schema: Option<&Value>) -> Option<Value> {
    if let Some(e) = p.get("example") {
        return Some(e.clone());
    }
    if let Some(m) = p.get("examples").and_then(Value::as_object) {
        // Prefer a non-empty example (docs often list "no filter": "" first).
        let vals: Vec<Value> = m.values().filter_map(|e| deref(doc, e)).filter_map(|e| e.get("value").cloned()).collect();
        if let Some(v) = vals.iter().find(|v| !crate::builder::probe::is_empty_result(v)).or(vals.first()) {
            return Some(v.clone());
        }
    }
    let s = schema?;
    s.get("example").cloned().or_else(|| s.get("default").cloned()).or_else(|| s.pointer("/enum/0").cloned()).or_else(|| s.pointer("/examples/0").cloned())
}

/// Input-schema type and request `type` for an OpenAPI schema.
fn kinds(doc: &Value, schema: &Value) -> Option<(Value, &'static str)> {
    let s = deref(doc, schema)?;
    let t = s.get("type").and_then(Value::as_str).or_else(|| s.get("type").and_then(Value::as_array).and_then(|a| a.iter().filter_map(Value::as_str).find(|t| *t != "null")));
    let t = match t {
        Some(t) => t,
        None if s.get("enum").is_some() => "string",
        None if s.get("properties").is_some() => "object",
        None => "string",
    };
    let mut prop = Map::new();
    let req = match t {
        "integer" => "integer",
        "number" if s.get("format").and_then(Value::as_str) == Some("integer") => "integer",
        "number" => "number",
        "boolean" => "boolean",
        "array" => {
            let item = s.get("items").and_then(|i| deref(doc, i)).and_then(|i| i.get("type")).and_then(Value::as_str).unwrap_or("string");
            if item != "string" && item != "integer" && item != "number" {
                return None;
            }
            prop.insert("items".into(), json!({"type": "string"}));
            "string_list"
        }
        "object" => return None,
        _ => "string",
    };
    prop.insert("type".into(), json!(match req { "string_list" => "array", other => other }));
    if let Some(e) = s.get("enum").and_then(Value::as_array).filter(|e| !e.is_empty() && e.len() <= 60) {
        let vals: Vec<Value> = e.iter().filter(|x| !x.is_null()).map(|x| if req == "string" { json!(x.as_str().map(String::from).unwrap_or_else(|| x.to_string())) } else { x.clone() }).collect();
        prop.insert("enum".into(), json!(vals));
    }
    for k in ["minimum", "maximum"] {
        if let Some(n) = s.get(k).filter(|n| n.is_number()) {
            prop.insert(k.into(), n.clone());
        }
    }
    Some((Value::Object(prop), req))
}

fn localized(t: &str) -> Value {
    json!({"de": t, "en": t})
}

/// The success response: (json schema if any, empty body, non-JSON only).
fn success(doc: &Value, op: &Value) -> (Option<Value>, bool, bool) {
    let Some(rs) = op.get("responses").and_then(Value::as_object) else { return (None, false, false) };
    let ok = rs.iter().filter(|(c, _)| c.starts_with('2')).min_by_key(|(c, _)| c.to_string());
    let Some((code, r)) = ok else { return (None, false, false) };
    let r = deref(doc, r).unwrap_or(r);
    if let Some(content) = r.get("content").and_then(Value::as_object) {
        let json_ct = content.iter().find(|(ct, _)| ct.contains("json") || ct.as_str() == "*/*");
        return match json_ct {
            Some((_, c)) => (c.get("schema").and_then(|s| deref(doc, s)).cloned(), false, false),
            None => (None, false, content.keys().all(|ct| !ct.starts_with("text/"))),
        };
    }
    // Swagger 2: schema directly on the response; `produces` decides the type.
    if let Some(s) = r.get("schema") {
        let produces: Vec<&str> = op.get("produces").or_else(|| doc.get("produces")).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
        let binary = !produces.is_empty() && !produces.iter().any(|p| p.contains("json") || p.starts_with("text/"));
        return (deref(doc, s).cloned(), false, binary);
    }
    (None, code == "204" || code == "205", false)
}

/// Object properties, merged through `allOf` (and `$ref`s inside it).
fn props_of(doc: &Value, s: &Value, depth: u8) -> Option<Map<String, Value>> {
    let s = deref(doc, s).unwrap_or(s);
    let mut out = s.get("properties").and_then(Value::as_object).cloned().unwrap_or_default();
    if depth < 4 {
        for part in s.get("allOf").and_then(Value::as_array).into_iter().flatten() {
            out.extend(props_of(doc, part, depth + 1).unwrap_or_default());
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Arrays of objects in a response: the response itself (`""`) or a
/// top-level property (`"/results"`), with their item schema.
fn item_arrays(doc: &Value, schema: &Value) -> Vec<(String, Value)> {
    let s = deref(doc, schema).unwrap_or(schema);
    let item_of = |v: &Value| -> Option<Value> {
        let v = deref(doc, v).unwrap_or(v);
        let is_array = v.get("type").and_then(Value::as_str) == Some("array") || v.get("items").is_some();
        let it = v.get("items").filter(|_| is_array)?;
        let it = deref(doc, it).unwrap_or(it);
        props_of(doc, it, 0).map(|_| it.clone())
    };
    if let Some(it) = item_of(s) {
        return vec![(String::new(), it)];
    }
    s.get("properties").and_then(Value::as_object).into_iter().flatten().filter_map(|(k, v)| item_of(v).map(|it| (format!("/{}", k.replace('~', "~0").replace('/', "~1")), it))).collect()
}

fn expect_for(schema: Option<&Value>, doc: &Value) -> Value {
    let Some(s) = schema.and_then(|s| deref(doc, s)) else { return json!({}) };
    let mut e = Map::new();
    let t = s.get("type").and_then(Value::as_str).or_else(|| s.get("properties").map(|_| "object")).or_else(|| s.get("items").map(|_| "array"));
    if let Some(t @ ("array" | "object")) = t {
        e.insert("kind".into(), json!(t));
    }
    let req: Vec<String> = s.get("required").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).take(3).map(|k| format!("/{}", k.replace('~', "~0").replace('/', "~1"))).collect();
    if !req.is_empty() {
        e.insert("pointers".into(), json!(req));
    }
    Value::Object(e)
}

pub fn draft_from_openapi(doc: &Value, opts: &Options) -> Result<Draft> {
    let mut notes = Vec::new();
    let mut skipped = Vec::new();
    let swagger2 = doc.get("swagger").is_some();
    let info = doc.get("info").cloned().unwrap_or_else(|| json!({}));
    let title = text(&info, "title").unwrap_or_else(|| "API".into());
    let slug = match &opts.slug {
        // Mission slugs are kebab-case, toolkit slugs snake_case.
        Some(s) => s.trim().to_ascii_lowercase().replace('-', "_"),
        None => {
            let mut s = snake(&title.replace(" API", "").replace(" Api", ""), false);
            s.truncate(40);
            s.trim_end_matches('_').to_string()
        }
    };
    if slug.is_empty() || !slug.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') || slug.len() > 40 {
        bail!("`{slug}` is no valid toolkit slug (from `{title}`; lowercase a-z, 0-9, _; max 40); pass --slug");
    }
    let prefix = format!("{}_", slug.to_ascii_uppercase());
    let (api_base, host) = server(doc, &opts.source, &mut notes)?;
    let auth = auth(doc, &mut notes);
    let paths = doc.get("paths").and_then(Value::as_object).ok_or_else(|| anyhow!("document has no `paths`"))?;

    let mut actions: Vec<(bool, Value, Option<Value>, Value)> = Vec::new(); // (is_read, action, probe, missing)
    let mut slugs = BTreeSet::new();
    // Probe chaining: list results that can feed detail probes.
    let mut sources: Vec<(String, String, Value)> = Vec::new(); // (tool, pointer to an item array, item schema)
    let mut chain_wanted: BTreeMap<String, (String, String, Value)> = BTreeMap::new(); // tool -> (arg, param name, response schema)
    for (raw_path, item) in paths {
        let item = deref(doc, item).unwrap_or(item);
        let shared: Vec<Value> = item.get("parameters").and_then(Value::as_array).cloned().unwrap_or_default();
        for m in METHODS {
            let Some(op) = item.get(*m) else { continue };
            let method = m.to_ascii_uppercase();
            let op_id = text(op, "operationId");
            let name = op_id.clone().unwrap_or_else(|| format!("{m}{}", raw_path.replace(['{', '}'], "").replace('/', "_")));
            let mut aslug = format!("{prefix}{}", snake(&name, true));
            if aslug.len() > 80 {
                aslug.truncate(80);
                aslug = aslug.trim_end_matches('_').to_string();
            }
            let base_slug = aslug.clone();
            let mut i = 2;
            while slugs.contains(&aslug) {
                aslug = format!("{base_slug}_{i}");
                i += 1;
            }
            let label = format!("{method} {raw_path}");
            if !opts.operations.is_empty() && !opts.operations.iter().any(|o| Some(o) == op_id.as_ref() || *o == aslug) {
                continue;
            }
            if op.get("deprecated").and_then(Value::as_bool) == Some(true) {
                skipped.push(json!({"operation": label, "reason": "deprecated"}));
                continue;
            }
            let (resp_schema, no_content, binary) = success(doc, op);
            if binary {
                skipped.push(json!({"operation": label, "reason": "binary / non-JSON response (not supported by the spec format)"}));
                continue;
            }
            let risk = match method.as_str() {
                "GET" => "read",
                "DELETE" => "destructive",
                _ => "write",
            };
            // Parameters: path-level first, operation-level overrides by (in, name).
            let mut params: BTreeMap<(String, String), Value> = BTreeMap::new();
            let mut order = Vec::new();
            for p in shared.iter().chain(op.get("parameters").and_then(Value::as_array).into_iter().flatten()) {
                let Some(p) = deref(doc, p) else {
                    notes.push(format!("{label}: unresolved parameter reference skipped"));
                    continue;
                };
                let key = (text(p, "in").unwrap_or_default(), text(p, "name").unwrap_or_default());
                if !params.contains_key(&key) {
                    order.push(key.clone());
                }
                params.insert(key, p.clone());
            }
            let mut props = Map::new();
            let mut required = Vec::new();
            let mut query = Map::new();
            let mut enc = Map::new();
            let mut taken = BTreeSet::new();
            let mut path = raw_path.clone();
            let mut probe_args = Map::new();
            let mut missing = Vec::new();
            let mut path_args: Vec<(String, String)> = Vec::new();
            let mut bad = None;
            let mut body_params = Vec::new();
            for key in &order {
                let p = &params[key];
                let (loc, pname) = (key.0.as_str(), key.1.as_str());
                let req = p.get("required").and_then(Value::as_bool).unwrap_or(loc == "path");
                let schema = if swagger2 && p.get("schema").is_none() { Some(p) } else { p.get("schema") };
                match loc {
                    "header" | "cookie" => {
                        if req {
                            bad = Some(format!("required {loc} parameter `{pname}` (not supported)"));
                        }
                        continue;
                    }
                    "body" | "formData" => {
                        body_params.push(p.clone());
                        continue;
                    }
                    "path" | "query" => {}
                    _ => continue,
                }
                let Some((mut prop, kind)) = schema.and_then(|s| kinds(doc, s)) else {
                    if req {
                        bad = Some(format!("required {loc} parameter `{pname}` has an object schema"));
                    } else {
                        notes.push(format!("{label}: optional {loc} parameter `{pname}` (object) skipped"));
                    }
                    continue;
                };
                let a = arg_name(pname, &taken);
                taken.insert(a.clone());
                if let Some(d) = text(p, "description").or_else(|| schema.and_then(|s| deref(doc, s)).and_then(|s| text(s, "description"))) {
                    prop.as_object_mut().unwrap().insert("description".into(), json!(clean(&d, 300)));
                }
                if loc == "path" {
                    if kind == "string_list" {
                        bad = Some(format!("array path parameter `{pname}`"));
                        continue;
                    }
                    path = path.replace(&format!("{{{pname}}}"), &format!("{{{a}}}"));
                    path_args.push((a.clone(), pname.to_string()));
                    if kind == "string" {
                        enc.insert(a.clone(), json!("component"));
                    }
                } else {
                    let mut q = json!({"arg": a, "type": kind});
                    if kind == "string_list" {
                        q["join"] = json!(",");
                        let explode = p.get("explode").and_then(Value::as_bool).unwrap_or(p.get("collectionFormat").and_then(Value::as_str) == Some("multi"));
                        if explode {
                            notes.push(format!("{label}: `{pname}` is an exploded array (k=a&k=b); drafted comma-joined, check it live"));
                        }
                    }
                    if req {
                        q["required"] = json!(true);
                    }
                    query.insert(pname.to_string(), q);
                }
                props.insert(a.clone(), prop);
                if req {
                    required.push(json!(a));
                    match example(doc, p, schema.and_then(|s| deref(doc, s))) {
                        Some(v) => {
                            probe_args.insert(a.clone(), v);
                        }
                        None => missing.push(a.clone()),
                    }
                }
            }
            // Request body (JSON object properties become args).
            let mut body = Value::Null;
            let body_schema = if swagger2 {
                body_params.iter().find(|p| p.get("in").and_then(Value::as_str) == Some("body")).and_then(|p| p.get("schema")).and_then(|s| deref(doc, s)).cloned()
            } else {
                op.get("requestBody").and_then(|b| deref(doc, b)).and_then(|b| {
                    let c = b.get("content")?.as_object()?;
                    match c.iter().find(|(ct, _)| ct.contains("json")) {
                        Some((_, x)) => x.get("schema").and_then(|s| deref(doc, s)).cloned().or(Some(json!({}))),
                        None => {
                            bad = Some(format!("request body is {} (only JSON bodies are mapped)", c.keys().cloned().collect::<Vec<_>>().join(", ")));
                            None
                        }
                    }
                })
            };
            if swagger2 && body_params.iter().any(|p| p.get("in").and_then(Value::as_str) == Some("formData")) {
                bad = Some("formData body (not mapped)".into());
            }
            if let Some(bs) = body_schema {
                if method == "GET" {
                    notes.push(format!("{label}: GET with a request body; body ignored"));
                } else if let Some(bprops) = bs.get("properties").and_then(Value::as_object) {
                    let breq: BTreeSet<&str> = bs.get("required").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
                    let mut tmpl = Map::new();
                    for (k, v) in bprops {
                        let v = deref(doc, v).unwrap_or(v);
                        if v.get("readOnly").and_then(Value::as_bool) == Some(true) {
                            continue;
                        }
                        let raw = if taken.contains(k) { format!("body_{k}") } else { k.clone() };
                        let a = arg_name(&raw, &taken);
                        taken.insert(a.clone());
                        let (prop, kind) = kinds(doc, v).map(|(p, k)| (p, k)).unwrap_or_else(|| (json!({"type": "object"}), "raw"));
                        let mut prop = prop;
                        if kind == "raw" {
                            prop = json!({"description": "JSON value"});
                        }
                        if let Some(d) = text(v, "description") {
                            prop.as_object_mut().unwrap().insert("description".into(), json!(clean(&d, 300)));
                        }
                        props.insert(a.clone(), prop);
                        let t = match kind { "string_list" => "array", "raw" => "raw", k => k };
                        let mut v = json!({"arg": a, "type": t});
                        if breq.contains(k.as_str()) {
                            required.push(json!(a));
                            v["required"] = json!(true);
                        }
                        tmpl.insert(k.clone(), v);
                    }
                    body = json!({"json": tmpl});
                } else {
                    let a = arg_name("body", &taken);
                    props.insert(a.clone(), json!({"description": "JSON request body"}));
                    body = json!({"json": {"arg": a, "type": "raw"}});
                }
            }
            if let Some(why) = bad {
                skipped.push(json!({"operation": label, "reason": why}));
                continue;
            }
            if !path.bytes().all(|b| b.is_ascii_alphanumeric() || b"/-_.~:@{}".contains(&b)) || path.contains("//") {
                skipped.push(json!({"operation": label, "reason": "path has characters the spec format cannot carry"}));
                continue;
            }
            let summary = text(op, "summary").map(|s| clean(&s, 80));
            let desc = text(op, "description").or_else(|| summary.clone()).unwrap_or_else(|| label.clone());
            let title = summary.unwrap_or_else(|| clean(&name.replace(['_', '-'], " "), 80));
            let mut request = json!({"method": method, "path": if path.is_empty() { "/".to_string() } else { path }});
            if !query.is_empty() {
                request["query"] = Value::Object(query);
            }
            if !enc.is_empty() {
                request["path_encoding"] = Value::Object(enc);
            }
            if !body.is_null() {
                request["body"] = body;
            }
            let response = if no_content { json!({"mode": "const", "value": {"success": true}}) } else { json!({"mode": "body"}) };
            let action = json!({
                "slug": aslug, "title": localized(&title), "description": localized(&clean(&desc, 600)), "risk": risk, "scopes": [],
                "input_schema": {"type": "object", "additionalProperties": false, "properties": props, "required": required},
                "request": request, "response": response,
            });
            slugs.insert(aslug.clone());
            let is_read = risk == "read";
            if is_read {
                if let Some(rs) = resp_schema.as_ref() {
                    if missing.is_empty() {
                        for (ptr, item) in item_arrays(doc, rs) {
                            sources.push((aslug.clone(), ptr, item));
                        }
                    }
                    if let [(a, pname)] = path_args.as_slice() {
                        if missing.iter().all(|m| m == a) {
                            chain_wanted.insert(aslug.clone(), (a.clone(), pname.clone(), rs.clone()));
                        }
                    }
                }
            }
            let probe = (is_read && missing.is_empty()).then(|| {
                let mut p = json!({"tool": aslug, "args": probe_args});
                let e = expect_for(resp_schema.as_ref(), doc);
                if e.as_object().is_some_and(|m| !m.is_empty()) {
                    p["expect"] = e;
                }
                p
            });
            let miss = if is_read && !missing.is_empty() { json!({"tool": aslug, "needs": missing}) } else { Value::Null };
            actions.push((is_read, action, probe, miss));
        }
    }
    if actions.is_empty() {
        bail!("no operation could be mapped ({} skipped)", skipped.len());
    }
    // Reads first, then writes; cap.
    actions.sort_by_key(|(r, ..)| !*r);
    if actions.len() > opts.max_actions {
        for (_, a, ..) in actions.drain(opts.max_actions..) {
            skipped.push(json!({"operation": a["slug"], "reason": format!("over --max-actions {}", opts.max_actions)}));
        }
    }
    let mut probes = Vec::new();
    let mut chained = Vec::new();
    let mut probes_missing = Vec::new();
    let mut list = Vec::new();
    let kept: BTreeSet<String> = actions.iter().map(|(_, a, ..)| a["slug"].as_str().unwrap_or_default().to_string()).collect();
    for (_, a, p, m) in actions {
        let tool = a["slug"].as_str().unwrap_or_default().to_string();
        // A detail probe takes its id from a list probe whose items have the
        // same schema as the detail response (document examples go stale).
        let chain = chain_wanted.get(&tool).and_then(|(arg, pname, rs)| {
            let target = deref(doc, rs).unwrap_or(rs);
            sources.iter().filter(|(t, ..)| kept.contains(t)).find_map(|(t, ptr, item)| {
                let item = deref(doc, item).unwrap_or(item);
                if item != target {
                    return None;
                }
                let props = props_of(doc, item, 0)?;
                let field = [pname.as_str(), "identifier", "id"].into_iter().find(|f| props.contains_key(*f)).map(String::from).or_else(|| props.keys().find(|k| k.to_ascii_lowercase().ends_with("id")).cloned())?;
                Some((arg.clone(), format!("{t}#{ptr}/0/{}", field.replace('~', "~0").replace('/', "~1"))))
            })
        });
        match (chain, p) {
            (Some((arg, src)), p) => {
                let mut p = p.unwrap_or_else(|| {
                    let mut p = json!({"tool": tool, "args": {}});
                    if let Some((_, _, rs)) = chain_wanted.get(&tool) {
                        let e = expect_for(Some(rs), doc);
                        if e.as_object().is_some_and(|m| !m.is_empty()) {
                            p["expect"] = e;
                        }
                    }
                    p
                });
                if let Some(o) = p["args"].as_object_mut() {
                    o.remove(&arg);
                }
                p["from"] = json!({arg: src});
                chained.push(p);
            }
            (None, p) => {
                probes.extend(p);
                if !m.is_null() {
                    probes_missing.push(m);
                }
            }
        }
        list.push(a);
    }
    if !chained.is_empty() {
        notes.push(format!("{} detail probe(s) take their id from a list probe (`from`), not from document examples", chained.len()));
    }
    probes.extend(chained);
    let name = clean(&title, 80);
    let about = text(&info, "description").map(|d| clean(&d, 240)).unwrap_or_else(|| name.clone());
    let auth_scheme = auth.get("scheme").filter(|s| !s.is_null()).cloned().unwrap_or_else(|| json!({"type": "none"}));
    notes.push("titles and descriptions are copied from the document into both de and en; translate the missing language".into());
    let spec = json!({
        "schema": super::SCHEMA_ID,
        "slug": slug,
        "version": opts.version,
        "name": name,
        "description": localized(&about),
        "docs": if opts.source.is_empty() { json!([]) } else { json!([opts.source]) },
        "notes": format!("Drafted by `ctox toolhub draft-spec --from-openapi` from {} ({} {}).", if opts.source.is_empty() { "an OpenAPI document" } else { &opts.source }, if swagger2 { "Swagger" } else { "OpenAPI" }, doc.get("openapi").or_else(|| doc.get("swagger")).and_then(Value::as_str).unwrap_or("")),
        "api_base": api_base,
        "allowed_hosts": [host],
        "auth_schemes": [auth_scheme],
        "actions": list,
    });
    Ok(Draft { spec, probes, probes_missing, skipped, notes, auth })
}
