//! Generic executor for compiled adapter specs.
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

use super::egress::EgressPolicy;
use super::validate::*;
use crate::http::{HttpRequest, HttpResponse};
use crate::model::{ConnectionField, ToolkitSpec};
use crate::toolkits::ToolkitAdapter;

pub struct SpecAdapter {
    c: Compiled,
    sha256: String,
    /// Compiled adapter serving `hybrid.rust_actions`.
    rust: Option<Box<dyn ToolkitAdapter>>,
}

impl std::fmt::Debug for SpecAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SpecAdapter({}@{})", self.c.toolkit.slug, self.c.version)
    }
}

impl SpecAdapter {
    /// Validate and compile a spec document (JSON text).
    pub fn from_json(text: &str) -> Result<Self> {
        let doc: Value = serde_json::from_str(text).map_err(|e| anyhow!("spec is not valid JSON: {e}"))?;
        let mut c = compile(&doc)?;
        let mut rust = None;
        if !c.rust_actions.is_empty() {
            let r = crate::toolkits::builtin().into_iter().find(|a| a.spec().slug == c.toolkit.slug).ok_or_else(|| anyhow!("spec.hybrid: no compiled `{}` adapter in this build", c.toolkit.slug))?;
            for t in &c.rust_actions {
                if c.actions.iter().any(|a| &a.slug == t) {
                    bail!("spec.hybrid: `{t}` is both a spec action and a Rust action");
                }
                let tool = r.spec().tools.iter().find(|x| &x.slug == t).ok_or_else(|| anyhow!("spec.hybrid: compiled adapter has no tool `{t}`"))?;
                c.toolkit.tools.push(tool.clone());
            }
            rust = Some(r);
        }
        Ok(Self { c, sha256: sha256_hex(text.as_bytes()), rust })
    }

    pub fn version(&self) -> &str {
        &self.c.version
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    pub fn replaces_builtin(&self) -> bool {
        self.c.replaces_builtin
    }
    pub fn risk(&self, tool: &str) -> Option<Risk> {
        if let Some(t) = self.rust_tool(tool) {
            return Some(if t { Risk::Write } else { Risk::Read });
        }
        self.action(tool).ok().map(|a| a.risk)
    }
    /// Actions this spec delegates to the compiled adapter.
    pub fn rust_actions(&self) -> &[String] {
        &self.c.rust_actions
    }
    /// `Some(mutating)` when `tool` is a hybrid Rust action.
    fn rust_tool(&self, tool: &str) -> Option<bool> {
        self.c.rust_actions.iter().any(|t| t == tool).then(|| self.c.toolkit.tools.iter().find(|t| t.slug == tool).is_some_and(|t| t.mutating))
    }

    fn action(&self, tool: &str) -> Result<&Action> {
        self.c.actions.iter().find(|a| a.slug == tool).ok_or_else(|| anyhow!("unknown {} tool `{tool}`", self.c.toolkit.name))
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn has_value(args: &Value, key: &str) -> bool {
    match args.get(key) {
        None | Some(Value::Null) => false,
        Some(Value::String(s)) => !s.trim().is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(_) => true,
    }
}

/// The id charset of path segments (`[A-Za-z0-9._-]{1,200}`), for ids sent in a body or query.
fn check_segment(v: &Value, name: &str) -> Result<()> {
    let s = v.as_str().unwrap_or_default();
    crate::toolkits::path_segment(&json!({ "v": s }), "v").map(|_| ()).map_err(|_| anyhow!("invalid argument `{name}`"))
}

/// `true` equals `"true"`, `3` equals `"3"`; otherwise plain JSON equality.
fn loose_eq(x: &Value, v: &Value) -> bool {
    let norm = |y: &Value| match y {
        Value::String(s) => s.trim().to_string(),
        o => o.to_string(),
    };
    x == v || (!x.is_null() && norm(x) == norm(v))
}

fn is_date(t: &str) -> bool {
    t.len() == 10
        && t.bytes().enumerate().all(|(i, b)| if i == 4 || i == 7 { b == b'-' } else { b.is_ascii_digit() })
        && (1..=12).contains(&t[5..7].parse::<u32>().unwrap_or(0))
        && (1..=31).contains(&t[8..10].parse::<u32>().unwrap_or(0))
}

/// ISO 8601 date-time with an explicit zone (`Z` or `±hh:mm`).
fn is_datetime_tz(t: &str) -> bool {
    let n = t.len();
    let b = t.as_bytes();
    let zulu = n > 1 && b[n - 1] == b'Z';
    let offset = n > 6 && matches!(b[n - 6], b'+' | b'-') && b[n - 3] == b':';
    is_datetime(t) && (zulu || offset)
}

fn is_datetime(t: &str) -> bool {
    let Some((d, rest)) = t.split_once('T') else { return false };
    is_date(d) && rest.len() >= 5 && rest.as_bytes()[2] == b':'
}

fn encode_component(v: &str) -> String {
    let mut out = String::new();
    for b in v.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Resolve one value; `Ok(None)` = absent (omitted from query/body).
fn eval(v: &Val, args: &Value) -> Result<Option<Value>> {
    let a = match v {
        Val::Const(c) => return Ok(Some(c.clone())),
        Val::Arg(a) => a,
        Val::Template { parts } => {
            let mut out = String::new();
            for p in parts {
                match p {
                    TmplPart::Lit(l) => out.push_str(l),
                    TmplPart::Arg { name, escape, forbid } => {
                        let raw = match args.get(name) {
                            Some(Value::String(s)) => s.trim().to_string(),
                            Some(Value::Number(n)) => n.to_string(),
                            _ => String::new(),
                        };
                        if raw.is_empty() {
                            bail!("missing argument `{name}`");
                        }
                        if let Some(c) = raw.chars().find(|c| forbid.contains(*c)) {
                            bail!("`{name}` must not contain \"{c}\"");
                        }
                        match escape {
                            Escape::None => out.push_str(&raw),
                            Escape::BackslashQuote => out.push_str(&raw.replace('\\', "\\\\").replace('"', "\\\"")),
                            Escape::Segments => {
                                if raw.split('/').any(|seg| crate::toolkits::path_segment(&json!({ "v": seg }), "v").is_err()) {
                                    bail!("invalid argument `{name}`");
                                }
                                out.push_str(&raw);
                            }
                            Escape::Json => {
                                let q = serde_json::to_string(&raw).unwrap_or_default();
                                out.push_str(&q[1..q.len() - 1]);
                            }
                        }
                    }
                }
            }
            return Ok(Some(Value::String(out)));
        }
        Val::Switch { arg, cases, default, otherwise } => {
            let given = match args.get(arg) {
                Some(Value::Bool(b)) => Some(b.to_string()),
                x => x.and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(String::from),
            };
            if let Some(o) = otherwise {
                return match given.and_then(|k| cases.iter().find(|(c, _)| *c == k)) {
                    Some((_, v)) => eval(v, args),
                    None => eval(o, args),
                };
            }
            let key = match given {
                Some(k) => k,
                None => match default {
                    Some(d) => d.clone(),
                    None => return Ok(None),
                },
            };
            let (_, v) = cases.iter().find(|(k, _)| *k == key).ok_or_else(|| {
                anyhow!("`{arg}` must be one of {}, got `{key}`", cases.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join(", "))
            })?;
            return eval(v, args);
        }
    };
    let raw = args.get(&a.name).filter(|x| !x.is_null());
    let mut out: Option<Value> = match (a.kind, raw) {
        (_, None) => None,
        (Kind::String, Some(Value::String(s))) => {
            let s = if a.trim { s.trim() } else { s.as_str() };
            let s = a.strip_prefix.as_deref().and_then(|p| s.strip_prefix(p)).unwrap_or(s);
            (!s.is_empty()).then(|| Value::String(s.to_string()))
        }
        (Kind::String, Some(Value::Number(n))) => Some(Value::String(n.to_string())),
        (Kind::String, Some(_)) => bail!("`{}` must be a string", a.name),
        (Kind::Integer, Some(x)) => {
            let n = x.as_i64().or_else(|| x.as_str().and_then(|s| s.trim().parse().ok())).ok_or_else(|| anyhow!("`{}` must be a whole number", a.name))?;
            Some(json!(a.clamp.map_or(n, |(lo, hi)| n.clamp(lo, hi))))
        }
        (Kind::Number, Some(x)) => {
            if let Some(n) = x.as_i64().or_else(|| x.as_str().and_then(|s| s.trim().parse::<i64>().ok())) {
                Some(json!(a.clamp.map_or(n, |(lo, hi)| n.clamp(lo, hi))))
            } else {
                let f = x.as_f64().or_else(|| x.as_str().and_then(|s| s.trim().parse::<f64>().ok())).filter(|f| f.is_finite()).ok_or_else(|| anyhow!("`{}` must be a number", a.name))?;
                let f = a.clamp.map_or(f, |(lo, hi)| f.clamp(lo as f64, hi as f64));
                Some(json!(f))
            }
        }
        (Kind::Boolean, Some(Value::Bool(b))) => Some(Value::Bool(*b)),
        (Kind::Boolean, Some(_)) => bail!("`{}` must be true or false", a.name),
        (Kind::Tristate, Some(Value::Bool(b))) => Some(Value::Bool(*b)),
        (Kind::Tristate, Some(Value::String(s))) if s == "true" || s == "false" => Some(Value::Bool(s == "true")),
        (Kind::Tristate, Some(_)) => bail!("`{}` must be \"true\" or \"false\"", a.name),
        (Kind::StringList, Some(Value::String(s))) => (!s.is_empty()).then(|| json!([s])),
        (Kind::StringList, Some(Value::Array(x))) => {
            let v: Vec<Value> = x.iter().filter_map(|y| y.as_str().map(|s| json!(s))).collect();
            (!v.is_empty()).then_some(Value::Array(v))
        }
        (Kind::StringList, Some(_)) => bail!("`{}` must be a list of strings", a.name),
        (Kind::Object, Some(Value::Object(m))) if a.blank_to_null => Some(Value::Object(
            m.iter().map(|(k, v)| (k.clone(), if v.as_str() == Some("") { Value::Null } else { v.clone() })).collect(),
        )),
        (Kind::Object, Some(Value::Object(m))) if a.stringify_values.is_some() => {
            let sep = a.stringify_values.as_deref().unwrap_or_default();
            let str_of = |x: &Value| match x {
                Value::String(s) => s.clone(),
                o => o.to_string(),
            };
            Some(Value::Object(m.iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| {
                let s = match v {
                    Value::Array(items) => items.iter().map(str_of).collect::<Vec<_>>().join(sep),
                    o => str_of(o),
                };
                (k.clone(), Value::String(s))
            }).collect()))
        }
        (Kind::Object, Some(x)) => x.is_object().then(|| x.clone()),
        (Kind::Raw, Some(Value::Object(m))) if a.wrap_object => Some(Value::Array(vec![Value::Object(m.clone())])),
        (Kind::Raw, Some(x)) => Some(x.clone()),
    };
    if !a.prepend.is_empty() && a.kind == Kind::StringList {
        let mut all: Vec<Value> = a.prepend.iter().map(|p| json!(p)).collect();
        for x in out.as_ref().and_then(Value::as_array).into_iter().flatten() {
            if !all.contains(x) {
                all.push(x.clone());
            }
        }
        out = Some(Value::Array(all));
    }
    if let (Some(key), Some(Value::Array(items))) = (&a.require_key, &out) {
        let kept: Vec<Value> = items.iter().filter(|it| it.get(key).is_some_and(Value::is_string)).cloned().collect();
        out = (!kept.is_empty()).then_some(Value::Array(kept));
    }
    if let Some(Value::Array(items)) = out.clone() {
        let items = &items;
        if let Some(max) = a.max_items {
            if items.len() > max {
                bail!("`{}` takes at most {max} items; received {}. Split into batches of {max}.", a.name, items.len());
            }
        }
        if a.segment {
            for it in items {
                check_segment(it, &a.name)?;
            }
        }
        if a.items_integer {
            let mut nums = Vec::new();
            for it in items {
                let n = it.as_u64().or_else(|| it.as_str().and_then(|s| s.trim().parse().ok())).filter(|n| *n > 0);
                nums.push(json!(n.ok_or_else(|| anyhow!("`{}` must be a list of positive user IDs", a.name))?));
            }
            out = Some(Value::Array(nums));
        }
        if a.items_segment {
            for it in items {
                let s = it.as_str().unwrap_or_default();
                if crate::toolkits::path_segment(&json!({"v": s}), "v").is_err() {
                    bail!("invalid item `{s}` in `{}`", a.name);
                }
            }
        }
    }
    if let (true, Some(v @ Value::String(_))) = (a.segment, &out) {
        check_segment(v, &a.name)?;
    }
    if let (true, Some(v)) = (a.json_string, &out) {
        if !v.is_string() {
            out = Some(Value::String(v.to_string()));
        }
    }
    if out.is_none() {
        if let Some(d) = &a.default {
            out = Some(d.clone());
        } else if a.required {
            bail!("missing argument `{}`", a.name);
        }
    }
    if let (Some(f), Some(Value::String(s))) = (a.format, &out) {
        let ok = match f {
            Format::Date => is_date(s),
            Format::DateTime => is_datetime_tz(s),
            Format::DatePrefix => s.len() >= 10 && s.is_char_boundary(10) && s[..10].bytes().enumerate().all(|(i, b)| if i == 4 || i == 7 { b == b'-' } else { b.is_ascii_digit() }),
            Format::Color => {
                let hex = (s.len() == 7 || s.len() == 4) && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit());
                hex || ((3..=30).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphabetic()))
            }
        };
        if !ok {
            bail!("`{}` must be {}, got \"{s}\"", a.name, match f { Format::Date => "a date in YYYY-MM-DD format", Format::DatePrefix => "an ISO 8601 date, e.g. 2026-10-11 or 2026-10-11T09:00:00Z", Format::Color => "a hex color such as #FF0000 or a CSS color name", Format::DateTime => "an ISO 8601 date-time such as 2026-10-01T15:00:00Z (with Z or a +hh:mm offset)" });
        }
    }
    if let (Some((allowed, strict)), Some(Value::String(s))) = (&a.one_of, &out) {
        if !allowed.contains(s) {
            if *strict {
                bail!("`{}` must be one of {}", a.name, allowed.join(", "));
            }
            out = None;
        }
    }
    if let (Some(prefix), Some(Value::String(v))) = (&a.uri_prefix, &out) {
        let full = if v.starts_with("https://") {
            let tail = v.strip_prefix(prefix.as_str()).ok_or_else(|| anyhow!("`{}` must be an id or a URI starting with {prefix}", a.name))?;
            if tail.is_empty() || tail.contains(['/', '?', '#']) {
                bail!("`{}` must be an id or a URI starting with {prefix}", a.name);
            }
            v.clone()
        } else {
            crate::toolkits::path_segment(&json!({"v": v}), "v").map_err(|_| anyhow!("invalid argument `{}`", a.name))?;
            format!("{prefix}{v}")
        };
        out = Some(Value::String(full));
    }
    if let (Some(sep), Some(Value::Array(x))) = (&a.join, &out) {
        out = Some(Value::String(x.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(sep)));
    }
    if a.wrap_array {
        out = out.map(|x| Value::Array(vec![x]));
    }
    Ok(out)
}

fn eval_tmpl(t: &Tmpl, args: &Value) -> Result<Option<Value>> {
    match t {
        Tmpl::Val(v) => eval(v, args),
        Tmpl::Obj(entries) => {
            let mut m = Map::new();
            for e in entries {
                match e {
                    Entry::Field(k, t) => {
                        if let Some(v) = eval_tmpl(t, args)? {
                            m.insert(k.clone(), v);
                        }
                    }
                    Entry::SwitchFormat { arg, date, datetime, other } => {
                        if let Some(s) = args.get(arg).and_then(Value::as_str).filter(|s| !s.is_empty()) {
                            let field = if is_date(s) { date } else if is_datetime(s) { datetime } else { other };
                            m.insert(field.clone(), json!(s));
                        }
                    }
                    Entry::OmitEmpty => {}
                    Entry::If(names) => {
                        if !names.iter().all(|n| has_value(args, n)) {
                            return Ok(None);
                        }
                    }
                    Entry::Merge(..) => {}
                }
            }
            for e in entries {
                if let Entry::Merge(v, wins) = e {
                    if let Some(Value::Object(extra)) = eval(v, args)? {
                        for (k, x) in extra {
                            if *wins || !m.contains_key(&k) {
                                m.insert(k, x);
                            }
                        }
                    }
                }
            }
            if m.is_empty() && entries.contains(&Entry::OmitEmpty) {
                return Ok(None);
            }
            Ok(Some(Value::Object(m)))
        }
        Tmpl::Arr(items) => {
            let mut out = Vec::new();
            for t in items {
                if let Some(v) = eval_tmpl(t, args)? {
                    out.push(v);
                }
            }
            Ok((!out.is_empty()).then_some(Value::Array(out)))
        }
    }
}

fn query_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Bracketed form encoding (Stripe, Rails, PHP): objects become `k[sub]=v`,
/// arrays `k[0]=v` / `k[0][sub]=v`; nulls are dropped. Depth is capped.
fn flatten_form(key: &str, v: &Value, out: &mut Vec<(String, String)>, depth: usize) -> Result<()> {
    if depth > 5 {
        bail!("form value `{key}` is nested too deeply");
    }
    match v {
        Value::Null => {}
        Value::Object(m) => {
            for (k, x) in m {
                if k.is_empty() || k.contains(['[', ']']) || k.chars().any(char::is_control) {
                    bail!("invalid key `{k}` in `{key}`");
                }
                flatten_form(&format!("{key}[{k}]"), x, out, depth + 1)?;
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                flatten_form(&format!("{key}[{i}]"), x, out, depth + 1)?;
            }
        }
        other => out.push((key.to_string(), query_string(other))),
    }
    Ok(())
}

/// JSON pointer where a `-` segment selects the LAST array element (RFC 6901
/// reserves `-` for "past the end", which never resolves on reads). Used for
/// cursor pagination that continues after the last item (Stripe `starting_after`).
fn pointer_last<'a>(v: &'a Value, p: &str) -> Option<&'a Value> {
    if !p.split('/').any(|s| s == "-") {
        return v.pointer(p);
    }
    let mut cur = v;
    for seg in p.split('/').skip(1) {
        cur = match (cur, seg) {
            (Value::Array(a), "-") => a.last()?,
            (c, s) => c.pointer(&format!("/{s}"))?,
        };
    }
    Some(cur)
}

fn project_list(list: &[Value], each: &[EachField]) -> Value {
    Value::Array(
        list.iter()
            .map(|it| {
                Value::Object(
                    each.iter()
                        .map(|f| {
                            let v = match &f.each {
                                None => match (it.pointer(&f.pointer), &f.default) {
                                    (Some(v), Some(d)) if std::mem::discriminant(v) != std::mem::discriminant(d) => d.clone(),
                                    (Some(v), _) => v.clone(),
                                    (None, Some(d)) => d.clone(),
                                    (None, None) => Value::Null,
                                },
                                Some(sub) => it.pointer(&f.pointer).and_then(Value::as_array).map_or(json!([]), |l| project_list(l, sub)),
                            };
                            (f.key.clone(), v)
                        })
                        .collect(),
                )
            })
            .collect(),
    )
}

fn path_value(args: &Value, name: &str, enc: &PathEnc) -> Result<String> {
    match enc {
        PathEnc::Segment => match args.get(name) {
            // Numeric ids (GitLab iids, GitHub numbers) are sent as their decimal form.
            Some(Value::Number(n)) if n.is_u64() || n.is_i64() => Ok(n.to_string()),
            _ => Ok(crate::toolkits::path_segment(args, name)?.to_string()),
        },
        PathEnc::Component => {
            let v = args.get(name).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).ok_or_else(|| anyhow!("missing argument `{name}`"))?;
            if v.len() > 200 || v == "." || v == ".." || v.chars().any(char::is_control) {
                bail!("invalid argument `{name}`");
            }
            Ok(encode_component(v))
        }
        PathEnc::StripPrefix(p) => {
            let v = args.get(name).and_then(Value::as_str).map(str::trim).unwrap_or_default();
            let v = v.strip_prefix(p.as_str()).unwrap_or(v);
            crate::toolkits::path_segment(&json!({ "v": v }), "v").map(str::to_string).map_err(|_| anyhow!("invalid argument `{name}`"))
        }
        PathEnc::Segments => {
            let v = args.get(name).and_then(Value::as_str).filter(|v| !v.trim().is_empty()).ok_or_else(|| anyhow!("missing argument `{name}`"))?;
            for seg in v.split('/') {
                crate::toolkits::path_segment(&json!({ "v": seg }), "v").map_err(|_| anyhow!("invalid argument `{name}`"))?;
            }
            Ok(v.to_string())
        }
        PathEnc::Digits => {
            let v = match args.get(name) {
                Some(Value::Number(n)) => n.to_string(),
                x => x.and_then(Value::as_str).map(str::trim).unwrap_or_default().to_string(),
            };
            if v.is_empty() || v.len() > 40 || !v.bytes().all(|b| b.is_ascii_digit()) {
                bail!("invalid argument `{name}`");
            }
            Ok(v)
        }
        PathEnc::Form => {
            let v = args.get(name).and_then(Value::as_str).filter(|v| !v.is_empty()).ok_or_else(|| anyhow!("missing argument `{name}`"))?;
            if v.len() > 200 || v.contains(['/', '?', '#']) {
                bail!("invalid argument `{name}`");
            }
            Ok(url::form_urlencoded::byte_serialize(v.as_bytes()).collect())
        }
        PathEnc::Name => {
            let v = args.get(name).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).ok_or_else(|| anyhow!("missing argument `{name}`"))?;
            if v.len() > 200 || v.chars().any(|c| c.is_control() || matches!(c, '/' | '\\' | '?' | '#' | '%')) {
                bail!("invalid argument `{name}`");
            }
            Ok(encode_component(v))
        }
        PathEnc::Namespaced(extra) => {
            let v = args.get(name).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).ok_or_else(|| anyhow!("missing argument `{name}`"))?;
            let ok = v.len() <= 200 && !v.split('/').any(|p| p == "." || p == "..") && v.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/') || extra.contains(c));
            if !ok {
                bail!("invalid argument `{name}`");
            }
            Ok(encode_component(v))
        }
        PathEnc::UriTail => {
            let v = args.get(name).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).ok_or_else(|| anyhow!("missing argument `{name}`"))?;
            let tail = v.trim_end_matches('/').rsplit('/').next().unwrap_or_default();
            Ok(crate::toolkits::path_segment(&json!({ name: tail }), name)?.to_string())
        }
    }
}

fn error_text(v: &Value, key: &str) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(|x| x.get(key).and_then(Value::as_str).map(String::from).unwrap_or_else(|| x.to_string())).collect::<Vec<_>>().join("; "),
        Value::Object(_) => v.get(key).and_then(Value::as_str).map(String::from).unwrap_or_else(|| v.to_string()),
        other => other.to_string(),
    }
}

impl ToolkitAdapter for SpecAdapter {
    fn spec(&self) -> &ToolkitSpec {
        &self.c.toolkit
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        if let (Some(r), Some(_)) = (&self.rust, self.rust_tool(tool)) {
            return r.build_request(tool, args);
        }
        let a = self.action(tool)?;
        let empty = json!({});
        let args = if args.is_null() { &empty } else { args };
        if !args.is_object() {
            bail!("arguments must be a JSON object");
        }
        for v in &a.validate_only {
            eval(v, args)?;
        }
        for c in &a.constraints {
            if let Some((w, v)) = &c.when {
                if !args.get(w).is_some_and(|x| loose_eq(x, v)) {
                    continue;
                }
            }
            let n = c.args.iter().zip(&c.equals).filter(|(k, e)| match e {
                None => has_value(args, k),
                Some(v) => args.get(k.as_str()).is_some_and(|x| loose_eq(x, v)),
            }).count();
            if (c.exclusive && n > 1) || (!c.exclusive && n == 0) {
                bail!("{}", c.message);
            }
        }
        let mut path = String::new();
        let parts = a
            .alt_paths
            .iter()
            .find(|(w, _)| w.as_ref().is_none_or(|w| has_value(args, w)))
            .map(|(_, p)| p)
            .unwrap_or(&a.path);
        for p in parts {
            match p {
                PathPart::Lit(l) => path.push_str(l),
                PathPart::Param(n, e) => path.push_str(&path_value(args, n, e)?),
            }
        }
        let mut ser = url::form_urlencoded::Serializer::new(String::new());
        let mut any = false;
        for (k, v) in &a.query {
            if let Some(v) = eval(v, args)? {
                // Objects/arrays use bracketed keys (`fields[0]=a`, `sort[0][field]=x`).
                let mut pairs = Vec::new();
                flatten_form(k, &v, &mut pairs, 0)?;
                for (k, v) in pairs {
                    ser.append_pair(&k, &v);
                    any = true;
                }
            }
        }
        let url = format!("{}{}{}", self.c.api_base, path, if any { format!("?{}", ser.finish()) } else { String::new() });
        let mut req = HttpRequest::new(&a.method, url);
        for (k, v) in &a.headers {
            req = req.header(k, v.clone());
        }
        let nonempty = |v: &Value| -> Result<()> {
            if let (Some(msg), true) = (&a.nonempty_body, v.is_null()) {
                bail!("{msg}");
            }
            if let (Some(msg), Some(m)) = (&a.nonempty_body, v.as_object()) {
                let inner_empty = m.is_empty() || (m.len() == 1 && m.values().next().and_then(Value::as_object).is_some_and(|x| x.is_empty()));
                if inner_empty {
                    bail!("{msg}");
                }
            }
            Ok(())
        };
        Ok(match &a.body {
            Body::None => req,
            Body::Json(t) => {
                let v = eval_tmpl(t, args)?.unwrap_or(Value::Null);
                nonempty(&v)?;
                req.json(&v)
            }
            Body::Form(pairs) => {
                let mut out = Vec::new();
                for (k, v) in pairs {
                    if let Some(v) = eval(v, args)? {
                        flatten_form(k, &v, &mut out, 0)?;
                    }
                }
                let refs: Vec<(&str, &str)> = out.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                req.form(&refs)
            }
            Body::GraphQl { query, variables } => {
                let vars = eval_tmpl(variables, args)?.unwrap_or(json!({}));
                req.json(&json!({"query": query, "variables": vars}))
            }
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        if let (Some(r), Some(_)) = (&self.rust, self.rust_tool(tool)) {
            return r.parse_response(tool, response);
        }
        let a = self.action(tool)?;
        let body = response.json().or_else(|_| {
            let text: String = String::from_utf8_lossy(&response.body).chars().take(2000).collect();
            Ok::<Value, anyhow::Error>(json!({"text": text}))
        })?;
        for e in &self.c.error_check {
            match e {
                ErrorCheck::Present { pointer, message_key, prefix } => {
                    if let Some(v) = body.pointer(pointer) {
                        let present = match v {
                            Value::Null => false,
                            Value::Array(x) => !x.is_empty(),
                            Value::String(s) => !s.is_empty(),
                            Value::Object(m) => !m.is_empty(),
                            _ => true,
                        };
                        if present {
                            bail!("{prefix}: {}", error_text(v, message_key));
                        }
                    }
                }
                ErrorCheck::GraphQl { prefix, require_data, no_data } => {
                    if let Some(errors) = body.get("errors").and_then(Value::as_array).filter(|e| !e.is_empty()) {
                        let msg = errors.iter().filter_map(|e| e["message"].as_str()).collect::<Vec<_>>().join("; ");
                        bail!("{prefix}: {}", if msg.is_empty() { "unknown" } else { &msg });
                    }
                    if *require_data && body.get("data").is_none_or(Value::is_null) {
                        bail!("{no_data}");
                    }
                }
                ErrorCheck::Flag { pointer, expect, error_pointer } => {
                    if body.pointer(pointer).is_some_and(|v| v != expect) {
                        bail!("provider error: {}", body.pointer(error_pointer).map(|v| error_text(v, "message")).unwrap_or_default());
                    }
                }
            }
        }
        Ok(match &a.response {
            Response::Body => body,
            Response::Pick { pointer, required, empty_error } => match body.pointer(pointer) {
                Some(v) if !v.is_null() => v.clone(),
                _ if *required => bail!("{empty_error}"),
                _ => Value::Null,
            },
            Response::List { pointer, key } => {
                let items = body.pointer(pointer).and_then(Value::as_array).cloned().unwrap_or_default();
                json!({ key.as_str(): items.clone(), "count": items.len() })
            }
            Response::Array { pointer, each } => {
                let list = pointer_last(&body, pointer).and_then(Value::as_array).cloned().unwrap_or_default();
                match each {
                    Some(e) => project_list(&list, e),
                    None => Value::Array(list),
                }
            }
            Response::Object(fields, require) => {
                if let Some((p, msg)) = require {
                    if !body.pointer(p).is_some_and(Value::is_object) {
                        bail!("{msg}");
                    }
                }
                let mut m = Map::new();
                for (k, p, d, shape) in fields {
                    if shape.count {
                        m.insert(k.clone(), json!(pointer_last(&body, p).and_then(Value::as_array).map_or(0, Vec::len)));
                        continue;
                    }
                    if let Some(each) = &shape.each {
                        let v = match pointer_last(&body, p).and_then(Value::as_array) {
                            Some(list) => project_list(list, each),
                            // explicit `"default": null` keeps an absent list null
                            None if d == &Some(Value::Null) => Value::Null,
                            None => json!([]),
                        };
                        m.insert(k.clone(), v);
                        continue;
                    }
                    let v = pointer_last(&body, p).filter(|v| !v.is_null() && v.as_str() != Some("")).cloned();
                    let v = match (v, d) {
                        (Some(v), Some(d)) if std::mem::discriminant(&v) != std::mem::discriminant(d) => d.clone(),
                        (Some(v), _) => v,
                        (None, Some(d)) => d.clone(),
                        (None, None) => Value::Null,
                    };
                    m.insert(k.clone(), v);
                }
                Value::Object(m)
            }
            Response::MergeSuccess { pointer } => {
                let mut m = body.pointer(pointer).and_then(Value::as_object).cloned().unwrap_or_default();
                m.insert("success".into(), json!(true));
                Value::Object(m)
            }
            Response::Const(v) => v.clone(),
            Response::Status(values) => match values.iter().find(|(c, _)| *c == response.status) {
                Some((_, v)) => v.clone(),
                None => bail!("unexpected status {} (spec maps {})", response.status, values.iter().map(|(c, _)| c.to_string()).collect::<Vec<_>>().join(", ")),
            },
        })
    }

    fn connection_fields(&self) -> Vec<ConnectionField> {
        self.c.connection_fields.clone()
    }

    fn default_api_base(&self) -> Option<String> {
        Some(self.c.api_base.clone())
    }

    fn connection_api_base(&self, params: &BTreeMap<String, String>) -> Result<Option<String>> {
        let Some(t) = &self.c.api_base_template else { return Ok(None) };
        let mut out = t.clone();
        for f in &self.c.connection_fields {
            let key = format!("{{{}}}", f.key);
            if !out.contains(&key) {
                continue;
            }
            let v = params.get(&f.key).map(|v| v.trim().to_ascii_lowercase()).unwrap_or_default();
            let label_ok = !v.is_empty() && v.len() <= 63 && !v.starts_with('-') && !v.ends_with('-') && v.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
            if !label_ok {
                bail!("connection parameter `{}` must be a single DNS label (letters, digits, hyphen)", f.key);
            }
            out = out.replace(&key, &v);
        }
        self.c.egress.check_url(&out)?;
        Ok(Some(out))
    }

    fn egress_policy(&self) -> Option<&EgressPolicy> {
        Some(&self.c.egress)
    }
}
