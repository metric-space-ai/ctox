// ref: internal/translator/claude/openai/responses/claude_openai-responses_tool_search.go @ 97ba1e896181b29b97d4ff1324a3af3f2a639c09
// Port-Status: candidate; request-owned client tool-search context
// License: MIT (upstream); modifications AGPL-3.0-only
use crate::internal::util::sanitize_claude_function_name;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub(super) const SEARCH: &str = "tool_search";
pub(super) const ANCHOR: &str = "tool_search_deferred_anchor";
#[derive(Clone, Debug)]
pub(super) struct Descriptor {
    pub name: String,
    pub child: String,
    pub namespace: String,
    pub kind: String,
    pub tool: Value,
    pub priority: u8,
    pub direct: bool,
    pub order: usize,
    pub search: bool,
}
#[derive(Clone, Debug, Default)]
pub(super) struct ToolSearchPlan {
    pub enabled: bool,
    pub native: bool,
    pub descriptors: Vec<Descriptor>,
    names: BTreeMap<String, String>,
    reverse: BTreeMap<String, String>,
}
impl ToolSearchPlan {
    pub fn from_request(request: &[u8]) -> Self {
        serde_json::from_slice::<Value>(request)
            .map(|root| Self::new(&root))
            .unwrap_or_default()
    }
    pub fn new(root: &Value) -> Self {
        let mut all = Vec::new();
        append(&mut all, root.get("tools"), 0);
        if let Some(input) = root.get("input").and_then(Value::as_array) {
            for item in input {
                if item["type"] == "additional_tools" {
                    append(&mut all, item.get("tools"), 1);
                }
            }
        }
        let enabled = all.iter().any(|d| d.search)
            || root
                .get("input")
                .and_then(Value::as_array)
                .is_some_and(|input| {
                    input.iter().any(|item| {
                        matches!(
                            item["type"].as_str(),
                            Some("tool_search_call" | "tool_search_output")
                        ) && item["execution"] == "client"
                    })
                });
        if !enabled {
            return Self::default();
        }
        if let Some(input) = root.get("input").and_then(Value::as_array) {
            for item in input {
                if item["type"] == "tool_search_output" {
                    append(&mut all, item.get("tools"), 2);
                }
            }
        }
        let native = winner(&all, SEARCH).is_some_and(|d| d.search);
        if native {
            all.retain(|d| d.priority == 2 || d.tool["defer_loading"] != true);
        }
        let mut selected = Vec::new();
        for d in &all {
            if winner(&all, &d.name).is_some_and(|w| w.order == d.order) {
                selected.push(d.clone());
            }
        }
        let mut plan = Self {
            enabled,
            native,
            descriptors: selected,
            ..Self::default()
        };
        let mut taken = BTreeSet::new();
        for d in plan.descriptors.clone() {
            if !matches!(d.kind.as_str(), "function" | "custom") {
                plan.assign(&d.name, &d.name, &mut taken);
            }
        }
        let declared = plan
            .descriptors
            .iter()
            .filter(|d| matches!(d.kind.as_str(), "function" | "custom"))
            .map(|d| d.name.clone())
            .collect::<Vec<_>>();
        plan.allocate(declared, &mut taken);
        let mut history = Vec::new();
        if let Some(input) = root.get("input").and_then(Value::as_array) {
            for item in input {
                if matches!(
                    item["type"].as_str(),
                    Some("function_call" | "custom_tool_call")
                ) {
                    if let Some(name) = item["name"].as_str() {
                        history.push(qualify(item["namespace"].as_str().unwrap_or(""), name));
                    }
                }
            }
        }
        plan.allocate(history, &mut taken);
        plan
    }
    fn assign(&mut self, id: &str, name: &str, taken: &mut BTreeSet<String>) {
        self.names.insert(id.into(), name.into());
        self.reverse.insert(name.into(), id.into());
        taken.insert(name.into());
    }
    fn allocate(&mut self, ids: Vec<String>, taken: &mut BTreeSet<String>) {
        let mut seen = BTreeSet::new();
        let mut changed = Vec::new();
        for id in ids {
            if id.is_empty() || self.names.contains_key(&id) || !seen.insert(id.clone()) {
                continue;
            }
            if sanitize_claude_function_name(&id) == id && !taken.contains(&id) {
                self.assign(&id, &id, taken);
            } else {
                changed.push(id);
            }
        }
        let mut counts = BTreeMap::<String, usize>::new();
        for id in &changed {
            *counts.entry(sanitize_claude_function_name(id)).or_default() += 1;
        }
        let mut hashed = Vec::new();
        for id in changed {
            let base = sanitize_claude_function_name(&id);
            if counts[&base] == 1 && !taken.contains(&base) {
                self.assign(&id, &base, taken);
            } else {
                hashed.push(id);
            }
        }
        hashed.sort();
        for id in hashed {
            let mut base = sanitize_claude_function_name(&id);
            base.truncate(base.len().min(53));
            for n in 0.. {
                let seed = if n == 0 {
                    id.clone()
                } else {
                    format!("{id}\0{n}")
                };
                let digest = Sha256::digest(seed.as_bytes());
                let suffix = digest[..5]
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>();
                let name = format!("{base}_{suffix}");
                if !taken.contains(&name) {
                    self.assign(&id, &name, taken);
                    break;
                }
            }
        }
    }
    pub fn name(&self, id: &str) -> String {
        self.names
            .get(id)
            .cloned()
            .unwrap_or_else(|| sanitize_claude_function_name(id))
    }
    pub fn history_name(&self, item: &Value) -> String {
        let raw = item["name"].as_str().unwrap_or("");
        let ns = item["namespace"].as_str().unwrap_or("");
        if !ns.is_empty() {
            return self.name(&qualify(ns, raw));
        }
        if self.descriptors.iter().any(|d| d.direct && d.name == raw) {
            return self.name(raw);
        }
        if let Some(d) = self
            .descriptors
            .iter()
            .find(|d| !d.direct && d.child == raw)
        {
            return self.name(&d.name);
        }
        self.name(raw)
    }
    pub fn identity(&self, name: &str) -> (String, Option<String>, Option<&str>) {
        let id = self.reverse.get(name).map(String::as_str).unwrap_or(name);
        if let Some(d) = self.descriptors.iter().find(|d| d.name == id) {
            if !d.direct && !d.namespace.is_empty() && !id.starts_with("mcp__") {
                return (d.child.clone(), Some(d.namespace.clone()), Some(&d.kind));
            }
            return (d.name.clone(), None, Some(&d.kind));
        }
        (id.into(), None, None)
    }
    pub fn is_search(&self, name: &str) -> bool {
        self.native && name == self.name(SEARCH)
    }
    pub fn output_content(&self, item: &Value) -> Value {
        let mut discovered = Vec::new();
        append(&mut discovered, item.get("tools"), 2);
        let mut seen = BTreeSet::new();
        let mut refs = Vec::new();
        for d in &discovered {
            if self.native
                && self.descriptors.iter().any(|w| {
                    w.name == d.name
                        && w.priority == 2
                        && matches!(w.kind.as_str(), "function" | "custom")
                })
            {
                let name = self.name(&d.name);
                if seen.insert(name.clone()) {
                    refs.push(json!({"type":"tool_reference","tool_name":name}));
                }
            }
        }
        if !refs.is_empty() {
            return Value::Array(refs);
        }
        let names = discovered
            .iter()
            .map(|d| self.name(&d.name))
            .collect::<Vec<_>>();
        Value::String(if names.is_empty() {
            "No matching tools were found.".into()
        } else {
            format!("Tools now available to call: {}.", names.join(", "))
        })
    }
}
fn winner<'a>(all: &'a [Descriptor], name: &str) -> Option<&'a Descriptor> {
    all.iter()
        .filter(|d| d.name == name)
        .min_by_key(|d| (d.priority, !d.direct, d.order))
}
fn qualify(ns: &str, name: &str) -> String {
    if ns.is_empty()
        || name.starts_with("mcp__")
        || name == ns
        || name.starts_with(&format!("{ns}__"))
    {
        name.into()
    } else if ns.ends_with("__") {
        format!("{ns}{name}")
    } else {
        format!("{ns}__{name}")
    }
}
fn append(out: &mut Vec<Descriptor>, tools: Option<&Value>, priority: u8) {
    let Some(tools) = tools.and_then(Value::as_array) else {
        return;
    };
    for tool in tools {
        let kind = tool["type"].as_str().unwrap_or("function").trim();
        if kind == "namespace" {
            let ns = tool["name"].as_str().unwrap_or("").trim();
            if let Some(children) = tool["tools"].as_array() {
                for child in children {
                    let kind = child["type"].as_str().unwrap_or("function");
                    if matches!(kind, "function" | "custom") {
                        let raw = child
                            .get("name")
                            .or_else(|| child.pointer("/function/name"))
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .trim();
                        if !raw.is_empty() {
                            out.push(Descriptor {
                                name: qualify(ns, raw),
                                child: raw.into(),
                                namespace: ns.into(),
                                kind: kind.into(),
                                tool: child.clone(),
                                priority,
                                direct: false,
                                order: out.len(),
                                search: false,
                            });
                        }
                    }
                }
            }
            continue;
        }
        let search = kind == SEARCH && tool["execution"] == "client";
        if kind == SEARCH && !search {
            continue;
        }
        if matches!(
            kind,
            "image_generation" | "file_search" | "code_interpreter" | "computer_use_preview"
        ) {
            continue;
        }
        let name = if search {
            SEARCH
        } else {
            tool.get("name")
                .or_else(|| tool.pointer("/function/name"))
                .and_then(Value::as_str)
                .unwrap_or(if kind == "web_search" {
                    "web_search"
                } else {
                    ""
                })
                .trim()
        };
        if name.is_empty() || (kind == "web_search" && tool["external_web_access"] == false) {
            continue;
        }
        out.push(Descriptor {
            name: name.into(),
            child: String::new(),
            namespace: String::new(),
            kind: kind.into(),
            tool: tool.clone(),
            priority,
            direct: true,
            order: out.len(),
            search,
        });
    }
}
pub(super) fn search_schema(tool: &Value) -> Value {
    let mut schema = super::request::normalize_schema(tool.get("parameters"));
    if schema
        .pointer("/properties/query/type")
        .and_then(Value::as_str)
        != Some("string")
    {
        schema["properties"]["query"] =
            json!({"type":"string","description":"Search query for deferred tools."});
    }
    let mut required = schema["required"].as_array().cloned().unwrap_or_default();
    if !required.iter().any(|v| v == "query") {
        required.push(json!("query"));
    }
    schema["required"] = Value::Array(required);
    if !tool.get("parameters").is_some_and(Value::is_object) {
        schema["properties"]["limit"] =
            json!({"type":"number","description":"Maximum number of tools to return."});
        schema["additionalProperties"] = json!(false);
    }
    schema
}
pub(super) fn anchor() -> Value {
    json!({"name":ANCHOR,"description":"Placeholder that is never loaded; tool search returns the real tools.","input_schema":{"type":"object","properties":{}},"defer_loading":true})
}
pub(super) fn search_call_item(id: &str, status: &str, args: &str) -> Value {
    let arguments = serde_json::from_str::<Value>(args)
        .ok()
        .filter(Value::is_object);
    let status = if status == "completed" && arguments.is_none() {
        "incomplete"
    } else {
        status
    };
    json!({"id":format!("tsc_{id}"),"type":"tool_search_call","status":status,"call_id":id,"execution":"client","arguments":arguments.unwrap_or_else(||json!({}))})
}
