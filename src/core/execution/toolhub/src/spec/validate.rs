//! Strict spec validation. A spec that passes is compiled into the internal
//! form `SpecAdapter` executes; there is no other way to build one.
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{Map, Value};
use std::collections::BTreeSet;

use super::egress::{is_public_dns_name, EgressPolicy, HostRule};
use super::SCHEMA_ID;
use crate::model::{AuthScheme, ConnectionField, LocalizedText, ToolSpec, ToolkitSpec};

/// Risk level of an action; anything but `Read` is side-effecting and goes
/// through the CTOX approval policy (`ToolSpec::mutating`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Risk {
    Read,
    Write,
    Destructive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    String,
    Integer,
    Boolean,
    StringList,
    Object,
    Raw,
    /// `true`/`false` as a boolean or the strings "true"/"false"; sent as a boolean.
    Tristate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Date,
    DateTime,
    /// Starts with `YYYY-MM-DD` (a date or any date-time).
    DatePrefix,
    /// `#RGB`, `#RRGGBB` or a CSS colour name.
    Color,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Const(Value),
    Arg(ArgRef),
    /// String built from a template: `{name}` = string arg, `{{`/`}}` = literal braces.
    Template { parts: Vec<TmplPart> },
    /// Pick one value by the string value of `arg` (`default` when absent).
    Switch { arg: String, cases: Vec<(String, Val)>, default: Option<String>, otherwise: Option<Box<Val>> },
}

/// Object-mode field post-processing.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FieldShape {
    /// `"each": {"out": "/in", …}`: project every array item (missing -> null).
    pub each: Option<Vec<EachField>>,
    /// `"count": true`: the length of the array at `pointer` (0 when absent).
    pub count: bool,
}

/// One output key of an `each` projection: `"/ptr"` or `{"pointer": "/ptr", "each": {…}}`
/// (nested list projection; a missing nested list becomes `[]`).
#[derive(Debug, Clone, PartialEq)]
pub struct EachField {
    pub key: String,
    pub pointer: String,
    pub each: Option<Vec<EachField>>,
}

pub fn parse_each(e: &Value, ctx: &str) -> Result<Vec<EachField>> {
    e.as_object()
        .ok_or_else(|| anyhow!("{ctx}: `each` maps output keys to item pointers"))?
        .iter()
        .map(|(k, p)| {
            let (pointer, each) = match p {
                Value::String(s) => (s.clone(), None),
                Value::Object(o) => (
                    o.get("pointer").and_then(Value::as_str).unwrap_or("").to_string(),
                    Some(parse_each(o.get("each").ok_or_else(|| anyhow!("{ctx}: `each.{k}` object needs `each`"))?, ctx)?),
                ),
                _ => bail!("{ctx}: `each.{k}` must be a pointer or {{pointer, each}}"),
            };
            if !pointer.starts_with('/') {
                bail!("{ctx}: `each.{k}` pointer must start with `/`");
            }
            Ok(EachField { key: k.clone(), pointer, each })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub enum TmplPart {
    Lit(String),
    Arg { name: String, escape: Escape, forbid: String },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Escape {
    None,
    /// `\` -> `\\`, `"` -> `\"` (formula / string-literal contexts).
    BackslashQuote,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArgRef {
    pub name: String,
    pub kind: Kind,
    pub trim: bool,
    pub required: bool,
    pub default: Option<Value>,
    pub clamp: Option<(i64, i64)>,
    pub format: Option<Format>,
    /// Allowed values; `strict` = error, otherwise the value is dropped.
    pub one_of: Option<(Vec<String>, bool)>,
    pub join: Option<String>,
    pub wrap_array: bool,
    /// Resource-URI APIs (Calendly): a bare id becomes `<prefix><id>`; a full
    /// URI must already start with the prefix (an https URL on an allowed host).
    pub uri_prefix: Option<String>,
    /// List args: at most this many items.
    pub max_items: Option<usize>,
    /// List args: every string item must be an id-like path segment.
    pub items_segment: bool,
    /// A single object is accepted and wrapped into a one-element list.
    pub wrap_object: bool,
    /// Object args: `""` values become `null` (clear a field).
    pub blank_to_null: bool,
    /// Parse every string item as a positive integer and send numbers.
    pub items_integer: bool,
    /// Remove this prefix from a string value (e.g. `@` before a username).
    pub strip_prefix: Option<String>,
    /// Validate the value (or each list item) with the path-segment charset.
    pub segment: bool,
    /// List args: keep only object items that have a string value at this key.
    pub require_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Tmpl {
    Val(Val),
    Obj(Vec<Entry>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Field(String, Tmpl),
    /// Route one string arg to a field by its shape (date / date-time / other).
    SwitchFormat { arg: String, date: String, datetime: String, other: String },
    /// `"$omit_empty": true`: drop this object when none of its fields resolved.
    OmitEmpty,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PathEnc {
    /// A namespaced id such as `group/project`: `[A-Za-z0-9._/-]{1,200}`, no `.`/`..`
    /// part, sent as one segment with `/` as `%2F` (GitLab project and group ids).
    /// `{"namespaced": ": "}` additionally allows the listed characters (percent-encoded).
    Namespaced(String),
    /// A display name such as an Airtable table name: like `Component`, but
    /// `/ \\ ? # %` are refused instead of encoded.
    Name,
    /// Leading prefix (e.g. `@`) removed, then checked like `Segment`.
    StripPrefix(String),
    /// `[A-Za-z0-9._-]{1,200}`, the compiled adapters' `path_segment`.
    Segment,
    /// Trimmed and percent-encoded like `encodeURIComponent`.
    Component,
    /// Accepts a bare id or a full resource URI; uses its last path segment
    /// (then checked like `Segment`).
    UriTail,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    None,
    Json(Tmpl),
    Form(Vec<(String, Val)>),
    GraphQl { query: String, variables: Tmpl },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Constraint {
    pub args: Vec<String>,
    /// Per-arg required value (`{"arg": .., "equals": ..}` entries); `None` = any non-empty value.
    pub equals: Vec<Option<Value>>,
    /// The constraint applies only while this arg loosely equals this value.
    pub when: Option<(String, Value)>,
    /// true = at most one may be set, false = at least one must be set.
    pub exclusive: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Response {
    Body,
    Pick { pointer: String, required: bool, empty_error: String },
    List { pointer: String, key: String },
    Object(Vec<(String, String, Option<Value>, FieldShape)>),
    MergeSuccess { pointer: String },
    Const(Value),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub slug: String,
    pub risk: Risk,
    pub method: String,
    pub path: Vec<PathPart>,
    /// `paths`: alternatives `(when-arg, path)`; the first whose `when` arg is set
    /// (or that has no `when`) is used. Empty = use `path`.
    pub alt_paths: Vec<(Option<String>, Vec<PathPart>)>,
    pub query: Vec<(String, Val)>,
    pub headers: Vec<(String, String)>,
    pub body: Body,
    pub constraints: Vec<Constraint>,
    pub nonempty_body: Option<String>,
    pub response: Response,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PathPart {
    Lit(String),
    Param(String, PathEnc),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ErrorCheck {
    /// Non-empty array/string/object at `pointer` = error; messages from `message_key`.
    Present { pointer: String, message_key: String, prefix: String },
    /// `pointer` must equal `expect`, error text at `error_pointer`.
    Flag { pointer: String, expect: Value, error_pointer: String },
}

#[derive(Debug, Clone)]
pub struct Compiled {
    pub toolkit: ToolkitSpec,
    pub version: String,
    pub api_base: String,
    pub api_base_template: Option<String>,
    pub connection_fields: Vec<ConnectionField>,
    pub egress: EgressPolicy,
    pub actions: Vec<Action>,
    pub error_check: Vec<ErrorCheck>,
    pub replaces_builtin: bool,
}

const FORBIDDEN_HEADERS: &[&str] = &[
    "authorization", "proxy-authorization", "cookie", "set-cookie", "host", "content-length",
    "transfer-encoding", "connection", "x-api-key", "x-auth-token", "api-key", "apikey",
];

fn s<'a>(v: &'a Value, key: &str, ctx: &str) -> Result<&'a str> {
    v.get(key).and_then(Value::as_str).filter(|x| !x.trim().is_empty()).ok_or_else(|| anyhow!("{ctx}: `{key}` must be a non-empty string"))
}

fn loc(v: &Value, key: &str, ctx: &str) -> Result<LocalizedText> {
    let o = v.get(key).ok_or_else(|| anyhow!("{ctx}: `{key}` is required (DE+EN)"))?;
    Ok(LocalizedText::new(s(o, "de", &format!("{ctx}.{key}"))?, s(o, "en", &format!("{ctx}.{key}"))?))
}

fn known_keys(v: &Value, allowed: &[&str], ctx: &str) -> Result<()> {
    if let Some(m) = v.as_object() {
        for k in m.keys() {
            if !allowed.contains(&k.as_str()) {
                bail!("{ctx}: unknown field `{k}`");
            }
        }
    }
    Ok(())
}

fn valid_slug(v: &str, upper: bool) -> bool {
    !v.is_empty()
        && v.len() <= 80
        && v.bytes().all(|b| b == b'_' || b.is_ascii_digit() || if upper { b.is_ascii_uppercase() } else { b.is_ascii_lowercase() || b == b'-' })
}

fn valid_version(v: &str) -> bool {
    let p: Vec<&str> = v.split('.').collect();
    p.len() == 3 && p.iter().all(|x| !x.is_empty() && x.len() <= 6 && x.bytes().all(|b| b.is_ascii_digit()))
}

/// Heuristic secret detection over every string in the spec.
pub fn find_secret_like(v: &Value, path: &str, out: &mut Vec<String>) {
    const PREFIXES: &[&str] = &[
        "ghp_", "gho_", "ghs_", "github_pat_", "glpat-", "xoxb-", "xoxp-", "xoxa-", "xapp-", "sk_live_", "sk_test_",
        "rk_live_", "pk_live_", "AKIA", "ASIA", "AIza", "ya29.", "Bearer ", "Basic ", "secret_", "ntn_", "lin_api_",
        "pat-na1-", "pat-eu1-", "-----BEGIN",
    ];
    const KEYS: &[&str] = &["token", "access_token", "refresh_token", "password", "secret", "client_secret", "api_key", "apikey", "private_key"];
    match v {
        Value::String(x) => {
            if PREFIXES.iter().any(|p| x.contains(p)) {
                out.push(path.to_string());
            }
            let long_opaque = x.len() >= 32 && !x.contains(' ') && !x.contains('/') && !x.contains('{')
                && x.bytes().filter(u8::is_ascii_digit).count() >= 6
                && x.bytes().filter(u8::is_ascii_alphabetic).count() >= 6
                && !x.contains(',') && !x.contains('.');
            if long_opaque {
                out.push(path.to_string());
            }
        }
        Value::Object(m) => {
            for (k, x) in m {
                let p = format!("{path}/{k}");
                if KEYS.contains(&k.to_ascii_lowercase().as_str()) && x.as_str().is_some_and(|y| !y.is_empty()) {
                    out.push(p.clone());
                }
                find_secret_like(x, &p, out);
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                find_secret_like(x, &format!("{path}/{i}"), out);
            }
        }
        _ => {}
    }
}

fn parse_val(v: &Value, props: &Map<String, Value>, ctx: &str, used: &mut BTreeSet<String>) -> Result<Val> {
    if let Some(c) = v.get("const") {
        known_keys(v, &["const"], ctx)?;
        return Ok(Val::Const(c.clone()));
    }
    if let Some(t) = v.get("template") {
        known_keys(v, &["template", "escape", "forbid"], ctx)?;
        let t = t.as_str().ok_or_else(|| anyhow!("{ctx}: template must be a string"))?;
        let mut parts = Vec::new();
        let mut lit = String::new();
        let mut it = t.chars().peekable();
        while let Some(c) = it.next() {
            match (c, it.peek()) {
                ('{', Some('{')) | ('}', Some('}')) => {
                    lit.push(c);
                    it.next();
                }
                ('{', _) => {
                    let name: String = it.by_ref().take_while(|&x| x != '}').collect();
                    if !props.contains_key(&name) {
                        bail!("{ctx}: template arg `{name}` is not declared in input_schema.properties");
                    }
                    used.insert(name.clone());
                    if !lit.is_empty() {
                        parts.push(TmplPart::Lit(std::mem::take(&mut lit)));
                    }
                    let escape = match v.get("escape").and_then(|e| e.get(&name)).and_then(Value::as_str) {
                        None | Some("none") => Escape::None,
                        Some("backslash_quote") => Escape::BackslashQuote,
                        Some(o) => bail!("{ctx}: unknown escape `{o}`"),
                    };
                    let forbid = v.get("forbid").and_then(|e| e.get(&name)).and_then(Value::as_str).unwrap_or("").to_string();
                    parts.push(TmplPart::Arg { name, escape, forbid });
                }
                ('}', _) => bail!("{ctx}: unmatched `}}` in template (use `}}}}` for a literal brace)"),
                _ => lit.push(c),
            }
        }
        if !lit.is_empty() {
            parts.push(TmplPart::Lit(lit));
        }
        return Ok(Val::Template { parts });
    }
    if let Some(arg) = v.get("switch") {
        known_keys(v, &["switch", "cases", "default", "else"], ctx)?;
        let arg = arg.as_str().ok_or_else(|| anyhow!("{ctx}: switch names a string arg"))?.to_string();
        if !props.contains_key(&arg) {
            bail!("{ctx}: switch arg `{arg}` is not declared in input_schema.properties");
        }
        used.insert(arg.clone());
        let mut cases = Vec::new();
        for (k, x) in v.get("cases").and_then(Value::as_object).ok_or_else(|| anyhow!("{ctx}: switch needs `cases`"))? {
            cases.push((k.clone(), parse_val(x, props, &format!("{ctx} case {k}"), used)?));
        }
        let default = v.get("default").and_then(Value::as_str).map(String::from);
        if default.as_ref().is_some_and(|d| !cases.iter().any(|(k, _)| k == d)) {
            bail!("{ctx}: switch default is not one of the cases");
        }
        let otherwise = match v.get("else") {
            Some(x) => Some(Box::new(parse_val(x, props, &format!("{ctx} else"), used)?)),
            None => None,
        };
        if default.is_some() && otherwise.is_some() {
            bail!("{ctx}: switch takes `default` or `else`, not both");
        }
        return Ok(Val::Switch { arg, cases, default, otherwise });
    }
    known_keys(v, &["arg", "type", "trim", "required", "default", "clamp", "format", "one_of", "strict", "join", "wrap_array", "uri_prefix", "max_items", "items_segment", "wrap_object", "blank_to_null", "require_key", "items_integer", "strip_prefix", "segment"], ctx)?;
    let name = s(v, "arg", ctx)?.to_string();
    let prop = props.get(&name).ok_or_else(|| anyhow!("{ctx}: arg `{name}` is not declared in input_schema.properties"))?;
    used.insert(name.clone());
    let kind = match v.get("type").and_then(Value::as_str).or_else(|| prop.get("type").and_then(Value::as_str)).unwrap_or("string") {
        "string" => Kind::String,
        "integer" | "number" => Kind::Integer,
        "boolean" => Kind::Boolean,
        "string_list" | "array" => Kind::StringList,
        "object" => Kind::Object,
        "raw" => Kind::Raw,
        "tristate" => Kind::Tristate,
        other => bail!("{ctx}: unknown type `{other}`"),
    };
    let clamp = match v.get("clamp") {
        None => None,
        Some(c) => {
            let a = c.as_array().filter(|a| a.len() == 2).ok_or_else(|| anyhow!("{ctx}: clamp must be [min, max]"))?;
            Some((a[0].as_i64().context("clamp min")?, a[1].as_i64().context("clamp max")?))
        }
    };
    let format = match v.get("format").and_then(Value::as_str) {
        None => None,
        Some("date") => Some(Format::Date),
        Some("date-time") => Some(Format::DateTime),
        Some("date-prefix") => Some(Format::DatePrefix),
        Some("color") => Some(Format::Color),
        Some(o) => bail!("{ctx}: unknown format `{o}`"),
    };
    let one_of = match v.get("one_of") {
        None => None,
        Some(a) => Some((
            a.as_array().context("one_of must be a list")?.iter().map(|x| x.as_str().map(String::from).context("one_of entries are strings")).collect::<Result<Vec<_>>>()?,
            v.get("strict").and_then(Value::as_bool).unwrap_or(true),
        )),
    };
    Ok(Val::Arg(ArgRef {
        name,
        kind,
        trim: v.get("trim").and_then(Value::as_bool).unwrap_or(false),
        required: v.get("required").and_then(Value::as_bool).unwrap_or(false),
        default: v.get("default").cloned(),
        clamp,
        format,
        one_of,
        join: v.get("join").and_then(Value::as_str).map(String::from),
        wrap_array: v.get("wrap_array").and_then(Value::as_bool).unwrap_or(false),
        uri_prefix: match v.get("uri_prefix") {
            None => None,
            Some(p) => {
                let p = p.as_str().filter(|p| p.starts_with("https://") && p.ends_with('/')).ok_or_else(|| anyhow!("{ctx}: uri_prefix must be an https URL ending in `/`"))?;
                Some(p.to_string())
            }
        },
        max_items: v.get("max_items").and_then(Value::as_u64).map(|n| n as usize),
        items_segment: v.get("items_segment").and_then(Value::as_bool).unwrap_or(false),
        wrap_object: v.get("wrap_object").and_then(Value::as_bool).unwrap_or(false),
        segment: v.get("segment").and_then(Value::as_bool).unwrap_or(false),
        strip_prefix: v.get("strip_prefix").and_then(Value::as_str).map(String::from),
        items_integer: v.get("items_integer").and_then(Value::as_bool).unwrap_or(false),
        blank_to_null: v.get("blank_to_null").and_then(Value::as_bool).unwrap_or(false),
        require_key: v.get("require_key").and_then(Value::as_str).map(String::from),
    }))
}

fn is_val(v: &Value) -> bool {
    v.get("arg").is_some() || v.get("const").is_some() || v.get("template").is_some() || v.get("switch").is_some()
}

fn parse_tmpl(v: &Value, props: &Map<String, Value>, ctx: &str, used: &mut BTreeSet<String>) -> Result<Tmpl> {
    if is_val(v) {
        return Ok(Tmpl::Val(parse_val(v, props, ctx, used)?));
    }
    let m = v.as_object().ok_or_else(|| anyhow!("{ctx}: body template entries must be objects ({{\"arg\":…}}, {{\"const\":…}} or nested)"))?;
    let mut entries = Vec::new();
    for (k, x) in m {
        if k == "$omit_empty" {
            if x.as_bool() == Some(true) {
                entries.push(Entry::OmitEmpty);
            }
        } else if k == "$switch_format" {
            let arg = s(x, "arg", ctx)?.to_string();
            if !props.contains_key(&arg) {
                bail!("{ctx}: arg `{arg}` is not declared in input_schema.properties");
            }
            used.insert(arg.clone());
            entries.push(Entry::SwitchFormat {
                arg,
                date: s(x, "date", ctx)?.into(),
                datetime: s(x, "date-time", ctx)?.into(),
                other: s(x, "other", ctx)?.into(),
            });
        } else {
            entries.push(Entry::Field(k.clone(), parse_tmpl(x, props, &format!("{ctx}.{k}"), used)?));
        }
    }
    Ok(Tmpl::Obj(entries))
}

fn parse_path(raw: &str, props: &Map<String, Value>, enc: &Map<String, Value>, ctx: &str, used: &mut BTreeSet<String>) -> Result<Vec<PathPart>> {
    if !raw.starts_with('/') || raw.contains("..") || raw.contains('?') || raw.contains('#') || raw.contains("//") {
        bail!("{ctx}: path must start with `/` and contain no `..`, `?`, `#` or `//`");
    }
    let mut out = Vec::new();
    let mut rest = raw;
    while let Some(i) = rest.find('{') {
        if i > 0 {
            out.push(PathPart::Lit(rest[..i].to_string()));
        }
        let j = rest[i..].find('}').ok_or_else(|| anyhow!("{ctx}: unclosed `{{` in path"))? + i;
        let name = &rest[i + 1..j];
        if !props.contains_key(name) {
            bail!("{ctx}: path param `{name}` is not declared in input_schema.properties");
        }
        used.insert(name.to_string());
        let e = match enc.get(name).and_then(|x| x.get("strip_prefix")).and_then(Value::as_str) {
            Some(p) => PathEnc::StripPrefix(p.to_string()),
            None if enc.get(name).is_some_and(|x| x.get("namespaced").is_some()) => {
                PathEnc::Namespaced(enc[name]["namespaced"].as_str().ok_or_else(|| anyhow!("{ctx}: `namespaced` lists extra characters as a string"))?.to_string())
            }
            None => match enc.get(name).and_then(Value::as_str) {
            None | Some("segment") => PathEnc::Segment,
            Some("component") => PathEnc::Component,
            Some("uri_tail") => PathEnc::UriTail,
            Some("namespaced") => PathEnc::Namespaced(String::new()),
            Some("name") => PathEnc::Name,
            Some(o) => bail!("{ctx}: unknown path encoding `{o}`"),
            },
        };
        out.push(PathPart::Param(name.to_string(), e));
        rest = &rest[j + 1..];
    }
    if !rest.is_empty() {
        out.push(PathPart::Lit(rest.to_string()));
    }
    for p in &out {
        if let PathPart::Lit(l) = p {
            if !l.bytes().all(|b| b.is_ascii_alphanumeric() || b"/-_.~:@".contains(&b)) {
                bail!("{ctx}: path literal `{l}` contains characters that need encoding");
            }
        }
    }
    Ok(out)
}

fn parse_response(v: Option<&Value>, ctx: &str) -> Result<Response> {
    let Some(v) = v else { return Ok(Response::Body) };
    known_keys(v, &["mode", "pointer", "required", "empty_error", "as", "fields", "value"], ctx)?;
    let ptr = |v: &Value| -> Result<String> {
        let p = v.get("pointer").and_then(Value::as_str).unwrap_or("").to_string();
        if !(p.is_empty() || p.starts_with('/')) {
            bail!("{ctx}: pointer must be empty or start with `/`");
        }
        Ok(p)
    };
    Ok(match s(v, "mode", ctx)? {
        "body" => Response::Body,
        "pick" => Response::Pick {
            pointer: ptr(v)?,
            required: v.get("required").and_then(Value::as_bool).unwrap_or(false),
            empty_error: v.get("empty_error").and_then(Value::as_str).unwrap_or("the provider returned an empty response").into(),
        },
        "list" => Response::List { pointer: ptr(v)?, key: s(v, "as", ctx)?.into() },
        "object" => {
            let f = v.get("fields").and_then(Value::as_object).ok_or_else(|| anyhow!("{ctx}: object mode needs `fields`"))?;
            let mut out = Vec::new();
            for (k, x) in f {
                // `""` is the whole response document (e.g. a top-level array).
                let p = x.get("pointer").and_then(Value::as_str).ok_or_else(|| anyhow!("{ctx}: `pointer` must be a string"))?.to_string();
                if !p.is_empty() && !p.starts_with('/') {
                    bail!("{ctx}: pointer must be empty or start with `/`");
                }
                known_keys(x, &["pointer", "default", "each", "count"], ctx)?;
                let each = match x.get("each") {
                    None => None,
                    Some(e) => Some(parse_each(e, ctx)?),
                };
                let shape = FieldShape { each, count: x.get("count").and_then(Value::as_bool).unwrap_or(false) };
                out.push((k.clone(), p, x.get("default").cloned(), shape));
            }
            Response::Object(out)
        }
        "merge_success" => Response::MergeSuccess { pointer: ptr(v)? },
        "const" => Response::Const(v.get("value").cloned().ok_or_else(|| anyhow!("{ctx}: const mode needs `value`"))?),
        o => bail!("{ctx}: unknown response mode `{o}`"),
    })
}

fn parse_action(a: &Value, toolkit: &str, prefix: &str, auth_headers: &[String]) -> Result<(Action, ToolSpec)> {
    let ctx0 = "action";
    known_keys(a, &["slug", "title", "description", "risk", "scopes", "input_schema", "request", "response", "docs"], ctx0)?;
    let slug = s(a, "slug", ctx0)?.to_string();
    let ctx = format!("action {slug}");
    if !valid_slug(&slug, true) || !slug.starts_with(prefix) {
        bail!("{ctx}: slug must be UPPER_SNAKE and start with `{prefix}`");
    }
    let schema = a.get("input_schema").cloned().ok_or_else(|| anyhow!("{ctx}: input_schema is required"))?;
    if schema.get("type").and_then(Value::as_str) != Some("object") {
        bail!("{ctx}: input_schema.type must be `object`");
    }
    let empty = Map::new();
    let props = schema.get("properties").and_then(Value::as_object).unwrap_or(&empty);
    for r in schema.get("required").and_then(Value::as_array).into_iter().flatten() {
        let r = r.as_str().ok_or_else(|| anyhow!("{ctx}: required entries are strings"))?;
        if !props.contains_key(r) {
            bail!("{ctx}: required `{r}` is not a declared property");
        }
    }
    let risk = match s(a, "risk", &ctx)? {
        "read" => Risk::Read,
        "write" => Risk::Write,
        "destructive" => Risk::Destructive,
        o => bail!("{ctx}: risk must be read|write|destructive, got `{o}`"),
    };
    let req = a.get("request").ok_or_else(|| anyhow!("{ctx}: request is required"))?;
    known_keys(req, &["method", "path", "paths", "path_encoding", "query", "headers", "body", "constraints", "nonempty_body", "read_only_post"], &ctx)?;
    let method = s(req, "method", &ctx)?.to_ascii_uppercase();
    if !["GET", "POST", "PUT", "PATCH", "DELETE"].contains(&method.as_str()) {
        bail!("{ctx}: unsupported method `{method}`");
    }
    let read_only_post = req.get("read_only_post").and_then(Value::as_bool).unwrap_or(false);
    match (method.as_str(), risk) {
        ("DELETE", r) if r != Risk::Destructive => bail!("{ctx}: DELETE actions must be marked `destructive`"),
        ("GET", Risk::Read) => {}
        ("GET", _) => {}
        (_, Risk::Read) if !read_only_post => bail!("{ctx}: {method} actions must be `write` or `destructive` (or set `read_only_post` for query-style POST endpoints)"),
        _ => {}
    }
    let mut used = BTreeSet::new();
    let enc = req.get("path_encoding").and_then(Value::as_object).cloned().unwrap_or_default();
    let mut alt_paths = Vec::new();
    let path = match req.get("paths") {
        None => parse_path(s(req, "path", &ctx)?, props, &enc, &ctx, &mut used)?,
        Some(list) => {
            if req.get("path").is_some() {
                bail!("{ctx}: use either `path` or `paths`");
            }
            let list = list.as_array().filter(|l| !l.is_empty()).ok_or_else(|| anyhow!("{ctx}: `paths` must be a non-empty list"))?;
            for (i, p) in list.iter().enumerate() {
                let when = p.get("when").and_then(Value::as_str).map(String::from);
                if let Some(w) = &when {
                    if !props.contains_key(w) {
                        bail!("{ctx}: paths[{i}].when `{w}` is not declared");
                    }
                    used.insert(w.clone());
                } else if i + 1 != list.len() {
                    bail!("{ctx}: only the last `paths` entry may omit `when`");
                }
                alt_paths.push((when, parse_path(s(p, "path", &ctx)?, props, &enc, &ctx, &mut used)?));
            }
            alt_paths[0].1.clone()
        }
    };
    let mut query = Vec::new();
    for (k, v) in req.get("query").and_then(Value::as_object).into_iter().flatten() {
        query.push((k.clone(), parse_val(v, props, &format!("{ctx} query {k}"), &mut used)?));
    }
    let mut headers = Vec::new();
    for (k, v) in req.get("headers").and_then(Value::as_object).into_iter().flatten() {
        let k = k.to_ascii_lowercase();
        if FORBIDDEN_HEADERS.contains(&k.as_str()) || auth_headers.contains(&k) || !k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            bail!("{ctx}: header `{k}` may not be set by a spec (auth is injected by the hub)");
        }
        let v = v.as_str().ok_or_else(|| anyhow!("{ctx}: header values are constant strings"))?;
        if v.contains('\r') || v.contains('\n') {
            bail!("{ctx}: header `{k}` contains a line break");
        }
        headers.push((k, v.to_string()));
    }
    let body = match req.get("body") {
        None | Some(Value::Null) => Body::None,
        Some(b) => {
            known_keys(b, &["json", "form", "graphql"], &ctx)?;
            if let Some(j) = b.get("json") {
                Body::Json(parse_tmpl(j, props, &format!("{ctx} body"), &mut used)?)
            } else if let Some(f) = b.get("form") {
                let mut out = Vec::new();
                for (k, v) in f.as_object().ok_or_else(|| anyhow!("{ctx}: form must be an object"))? {
                    out.push((k.clone(), parse_val(v, props, &format!("{ctx} form {k}"), &mut used)?));
                }
                Body::Form(out)
            } else if let Some(g) = b.get("graphql") {
                let q = s(g, "query", &ctx)?.to_string();
                let is_mutation = q.trim_start().starts_with("mutation");
                if is_mutation && risk == Risk::Read {
                    bail!("{ctx}: GraphQL mutations must be `write` or `destructive`");
                }
                let vars = match g.get("variables") {
                    Some(v) => parse_tmpl(v, props, &format!("{ctx} variables"), &mut used)?,
                    None => Tmpl::Obj(vec![]),
                };
                Body::GraphQl { query: q, variables: vars }
            } else {
                bail!("{ctx}: body needs one of json, form, graphql")
            }
        }
    };
    if method == "GET" && body != Body::None {
        bail!("{ctx}: GET requests carry no body");
    }
    if method == "GET" && risk != Risk::Read {
        bail!("{ctx}: GET actions must be `read`");
    }
    let mut constraints = Vec::new();
    for c in req.get("constraints").and_then(Value::as_array).into_iter().flatten() {
        let kind = s(c, "kind", &ctx)?;
        let mut args = Vec::new();
        let mut equals = Vec::new();
        for x in c.get("args").and_then(Value::as_array).context("constraint args")? {
            match x {
                Value::String(n) => {
                    args.push(n.clone());
                    equals.push(None);
                }
                _ => {
                    args.push(s(x, "arg", &ctx)?.to_string());
                    equals.push(Some(x.get("equals").cloned().context("constraint arg object needs `equals`")?));
                }
            }
        }
        let when = match c.get("when") {
            None => None,
            Some(w) => {
                let n = s(w, "arg", &ctx)?.to_string();
                if !props.contains_key(&n) {
                    bail!("{ctx}: constraint `when` arg `{n}` is not declared");
                }
                Some((n, w.get("equals").cloned().context("constraint `when` needs `equals`")?))
            }
        };
        for a in &args {
            if !props.contains_key(a) {
                bail!("{ctx}: constraint arg `{a}` is not declared");
            }
        }
        constraints.push(Constraint {
            args,
            equals,
            when,
            exclusive: match kind {
                "at_most_one" => true,
                "at_least_one" => false,
                o => bail!("{ctx}: unknown constraint `{o}`"),
            },
            message: s(c, "message", &ctx)?.into(),
        });
    }
    for p in props.keys() {
        if !used.contains(p) {
            bail!("{ctx}: input property `{p}` is declared but never sent");
        }
    }
    let response = parse_response(a.get("response"), &ctx)?;
    let tool = ToolSpec {
        slug: slug.clone(),
        toolkit: toolkit.into(),
        title: loc(a, "title", &ctx)?,
        description: loc(a, "description", &ctx)?,
        input_schema: schema,
        scopes: a.get("scopes").and_then(Value::as_array).map(|x| x.iter().filter_map(|y| y.as_str().map(String::from)).collect()).unwrap_or_default(),
        mutating: risk != Risk::Read,
    };
    Ok((
        Action {
            slug,
            risk,
            method,
            path,
            alt_paths,
            query,
            headers,
            body,
            constraints,
            nonempty_body: req.get("nonempty_body").and_then(Value::as_str).map(String::from),
            response,
        },
        tool,
    ))
}

/// Validate and compile a spec document.
pub fn compile(doc: &Value) -> Result<Compiled> {
    known_keys(
        doc,
        &["$schema", "schema", "slug", "version", "name", "description", "docs", "api_base", "api_base_template", "allowed_hosts", "connection_fields", "auth_schemes", "error_check", "replaces_builtin", "actions", "notes"],
        "spec",
    )?;
    if doc.get("schema").and_then(Value::as_str) != Some(SCHEMA_ID) {
        bail!("spec: `schema` must be `{SCHEMA_ID}`");
    }
    let mut leaks = Vec::new();
    find_secret_like(doc, "", &mut leaks);
    if !leaks.is_empty() {
        bail!("spec: values that look like credentials at {} (specs never contain secrets)", leaks.join(", "));
    }
    let slug = s(doc, "slug", "spec")?.to_string();
    if !valid_slug(&slug, false) || slug.len() > 40 {
        bail!("spec: slug must be lowercase [a-z0-9_-]");
    }
    let version = s(doc, "version", "spec")?.to_string();
    if !valid_version(&version) {
        bail!("spec: version must be MAJOR.MINOR.PATCH");
    }
    let mut rules = Vec::new();
    for h in doc.get("allowed_hosts").and_then(Value::as_array).filter(|a| !a.is_empty()).ok_or_else(|| anyhow!("spec: allowed_hosts must list at least one host"))? {
        rules.push(HostRule::parse(h.as_str().ok_or_else(|| anyhow!("spec: allowed_hosts entries are strings"))?)?);
    }
    let egress = EgressPolicy { rules };
    let api_base = s(doc, "api_base", "spec")?.trim_end_matches('/').to_string();
    egress.check_url(&api_base).context("spec: api_base")?;
    let auth_schemes: Vec<AuthScheme> = serde_json::from_value(doc.get("auth_schemes").cloned().unwrap_or(Value::Null)).context("spec: auth_schemes")?;
    if auth_schemes.is_empty() {
        bail!("spec: auth_schemes must not be empty");
    }
    if auth_schemes.contains(&AuthScheme::None) && auth_schemes.len() > 1 {
        bail!("spec: auth scheme `none` must be the only scheme");
    }
    let mut auth_headers = Vec::new();
    for a in &auth_schemes {
        match a {
            AuthScheme::ApiKey { header_name, .. } => {
                let h = header_name.to_ascii_lowercase();
                if !h.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') || ["host", "cookie", "content-length"].contains(&h.as_str()) {
                    bail!("spec: invalid API key header `{header_name}`");
                }
                auth_headers.push(h);
            }
            AuthScheme::OAuth2(o) => {
                for u in [&o.authorize_url, &o.token_url] {
                    let parsed = url::Url::parse(u).context("spec: oauth2 URL")?;
                    if parsed.scheme() != "https" || !parsed.host_str().is_some_and(is_public_dns_name) {
                        bail!("spec: oauth2 endpoints must be public https URLs");
                    }
                }
            }
            _ => {}
        }
    }
    let connection_fields: Vec<ConnectionField> = match doc.get("connection_fields") {
        None => vec![],
        Some(v) => serde_json::from_value(v.clone()).context("spec: connection_fields")?,
    };
    let api_base_template = doc.get("api_base_template").and_then(Value::as_str).map(|t| t.trim_end_matches('/').to_string());
    if let Some(t) = &api_base_template {
        let mut probe = t.clone();
        for f in &connection_fields {
            probe = probe.replace(&format!("{{{}}}", f.key), "probe-1");
        }
        if probe.contains('{') {
            bail!("spec: api_base_template uses a placeholder that is not a connection field");
        }
        egress.check_url(&probe).context("spec: api_base_template must stay inside allowed_hosts")?;
    }
    let mut error_check = Vec::new();
    for e in doc.get("error_check").and_then(Value::as_array).into_iter().flatten() {
        error_check.push(match s(e, "kind", "error_check")? {
            "present" => ErrorCheck::Present {
                pointer: s(e, "pointer", "error_check")?.into(),
                message_key: e.get("message_key").and_then(Value::as_str).unwrap_or("message").into(),
                prefix: e.get("prefix").and_then(Value::as_str).unwrap_or("provider error").into(),
            },
            "flag" => ErrorCheck::Flag {
                pointer: s(e, "pointer", "error_check")?.into(),
                expect: e.get("expect").cloned().unwrap_or(Value::Bool(true)),
                error_pointer: e.get("error_pointer").and_then(Value::as_str).unwrap_or("/error").into(),
            },
            o => bail!("error_check: unknown kind `{o}`"),
        });
    }
    let prefix = format!("{}_", slug.replace('-', "_").to_ascii_uppercase());
    let mut actions = Vec::new();
    let mut tools = Vec::new();
    let mut seen = BTreeSet::new();
    for a in doc.get("actions").and_then(Value::as_array).filter(|a| !a.is_empty()).ok_or_else(|| anyhow!("spec: actions must not be empty"))? {
        let (act, tool) = parse_action(a, &slug, &prefix, &auth_headers)?;
        if !seen.insert(act.slug.clone()) {
            bail!("spec: duplicate action `{}`", act.slug);
        }
        actions.push(act);
        tools.push(tool);
    }
    Ok(Compiled {
        toolkit: ToolkitSpec {
            slug: slug.clone(),
            name: s(doc, "name", "spec")?.into(),
            description: loc(doc, "description", "spec")?,
            auth_schemes,
            tools,
        },
        version,
        api_base,
        api_base_template,
        connection_fields,
        egress,
        actions,
        error_check,
        replaces_builtin: doc.get("replaces_builtin").and_then(Value::as_bool).unwrap_or(false),
    })
}
