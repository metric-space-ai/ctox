// ref: internal/util/responses_tools.go:16-258,331-343
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox — owned descriptors and identity mapping
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{sanitize_function_name, valid_json_bytes};
use crate::internal::client::codex::apply_patch::is_custom_tool;
use gjson::{Kind, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    fmt,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponsesToolIdentity {
    pub name: String,
    pub namespace: String,
    pub custom: bool,
    pub apply_patch: bool,
}
#[derive(Clone, Eq, PartialEq)]
pub struct ResponsesToolDescriptor {
    pub name: String,
    pub local_name: String,
    pub namespace: String,
    pub tool_type: String,
    pub tool_json: Vec<u8>,
    pub source_priority: u8,
    pub direct: bool,
    pub order: usize,
    pub apply_patch: bool,
}
impl fmt::Debug for ResponsesToolDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResponsesToolDescriptor")
            .field("tool_type", &self.tool_type)
            .field("source_priority", &self.source_priority)
            .field("direct", &self.direct)
            .field("order", &self.order)
            .field("raw_bytes", &self.tool_json.len())
            .field("apply_patch", &self.apply_patch)
            .finish()
    }
}

pub fn qualify_responses_namespace_tool_name(namespace: &str, child: &str) -> String {
    let child = child.trim();
    let namespace = namespace.trim();
    if child.is_empty()
        || namespace.is_empty()
        || child.starts_with("mcp__")
        || child == namespace
        || child.starts_with(&format!("{namespace}__"))
    {
        return child.to_owned();
    }
    if namespace.ends_with("__") {
        format!("{namespace}{child}")
    } else {
        format!("{namespace}__{child}")
    }
}
fn string(value: &Value<'_>, path: &str) -> String {
    let field = value.get(path);
    field.str().to_owned()
}
fn tool_name(tool: &Value<'_>) -> String {
    let direct = string(tool, "name");
    if !direct.trim().is_empty() {
        direct.trim().to_owned()
    } else {
        string(tool, "function.name").trim().to_owned()
    }
}
fn append_descriptor(
    out: &mut Vec<ResponsesToolDescriptor>,
    tool: &Value<'_>,
    namespace: &str,
    priority: u8,
    direct: bool,
) {
    let local_name = tool_name(tool);
    if local_name.is_empty() {
        return;
    }
    let kind = string(tool, "type");
    let tool_type = match kind.trim() {
        "" | "function" => "function",
        "custom" => "custom",
        _ => return,
    };
    out.push(ResponsesToolDescriptor {
        name: qualify_responses_namespace_tool_name(namespace, &local_name),
        local_name,
        namespace: namespace.to_owned(),
        tool_type: tool_type.to_owned(),
        tool_json: tool.json().as_bytes().to_vec(),
        source_priority: priority,
        direct,
        order: out.len(),
        // Only the winning raw declaration can opt into the patch contract.
        // A trimmed type or function.name fallback alone is insufficient.
        apply_patch: is_custom_tool(tool),
    });
}
fn append_source(out: &mut Vec<ResponsesToolDescriptor>, tools: &Value<'_>, priority: u8) {
    if tools.kind() != Kind::Array {
        return;
    }
    for tool in tools.array() {
        if string(&tool, "type").trim() == "namespace" {
            let namespace = string(&tool, "name").trim().to_owned();
            let mut children = tool.get("tools");
            if children.kind() != Kind::Array {
                children = tool.get("children");
            }
            if children.kind() != Kind::Array {
                continue;
            }
            for child in children.array() {
                append_descriptor(out, &child, &namespace, priority, false);
            }
        } else {
            append_descriptor(out, &tool, "", priority, true);
        }
    }
}

pub fn collect_responses_tool_descriptors(root: &Value<'_>) -> Vec<ResponsesToolDescriptor> {
    let mut out = Vec::new();
    let tools = root.get("tools");
    append_source(&mut out, &tools, 0);
    let input = root.get("input");
    if input.kind() == Kind::Array {
        for item in input.array() {
            if string(&item, "type") == "additional_tools" {
                let tools = item.get("tools");
                append_source(&mut out, &tools, 1);
            }
        }
    }
    out
}
fn precedes(left: &ResponsesToolDescriptor, right: &ResponsesToolDescriptor) -> bool {
    (left.source_priority, !left.direct, left.order)
        < (right.source_priority, !right.direct, right.order)
}
pub fn collect_responses_tool_winners(
    root: &Value<'_>,
) -> HashMap<String, ResponsesToolDescriptor> {
    let mut out: HashMap<String, ResponsesToolDescriptor> = HashMap::new();
    for descriptor in collect_responses_tool_descriptors(root) {
        if out
            .get(&descriptor.name)
            .is_none_or(|current| precedes(&descriptor, current))
        {
            out.insert(descriptor.name.clone(), descriptor);
        }
    }
    out
}

// ref: responses_tools.go:202-258 — lexical sorting and salted SHA-256 aliases.
fn sanitized_names(names: impl IntoIterator<Item = String>) -> HashMap<String, String> {
    let mut sorted = BTreeMap::new();
    for name in names {
        if !name.is_empty() {
            sorted.insert(name, ());
        }
    }
    let mut counts = HashMap::<String, usize>::new();
    for name in sorted.keys() {
        *counts.entry(sanitize_function_name(name)).or_default() += 1;
    }
    let mut used = HashMap::new();
    let mut out = HashMap::new();
    for (original, ()) in sorted {
        let base = sanitize_function_name(&original);
        let mapped =
            if counts.get(&base).copied().unwrap_or_default() > 1 || used.contains_key(&base) {
                disambiguate(&base, &original, &used)
            } else {
                base
            };
        used.insert(mapped.clone(), original.clone());
        out.insert(original, mapped);
    }
    out
}
fn disambiguate(base: &str, original: &str, used: &HashMap<String, String>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for attempt in 0_u64.. {
        let digest = Sha256::digest(format!("{original}\0{attempt}").as_bytes());
        let mut suffix = String::from("_");
        for byte in &digest[..6] {
            suffix.push(char::from(HEX[usize::from(byte >> 4)]));
            suffix.push(char::from(HEX[usize::from(byte & 15)]));
        }
        let prefix = &base[..base.len().min(64 - suffix.len())];
        let candidate = format!("{prefix}{suffix}");
        if !used.contains_key(&candidate) {
            return candidate;
        }
    }
    unreachable!("finite request names cannot occupy every SHA-256 alias")
}

/// Maps sanitized wire names and original qualified names to the winning original
/// declaration. Parameters/descriptions are never parsed into floating-point JSON.
// ref: responses_tools.go:261-329,331-343 — identity portion of Gemini declarations.
pub fn responses_tool_reverse_identity_map(
    raw_json: &[u8],
) -> HashMap<String, ResponsesToolIdentity> {
    if !valid_json_bytes(raw_json) {
        return HashMap::new();
    }
    let document = String::from_utf8_lossy(raw_json);
    let root = gjson::parse(&document);
    let request = root.get("request");
    let selected = if request.exists()
        && ["model", "input", "tools"]
            .iter()
            .any(|path| request.get(path).exists())
    {
        &request
    } else {
        &root
    };
    let descriptors = collect_responses_tool_descriptors(selected);
    let winners = collect_responses_tool_winners(selected);
    let winning = descriptors
        .into_iter()
        .filter(|descriptor| {
            winners
                .get(&descriptor.name)
                .is_some_and(|winner| winner.order == descriptor.order)
        })
        .collect::<Vec<_>>();
    let names = sanitized_names(winning.iter().map(|descriptor| descriptor.name.clone()));
    let mut out = HashMap::new();
    for descriptor in winning {
        let mapped = names
            .get(&descriptor.name)
            .filter(|name| !name.is_empty())
            .cloned()
            .unwrap_or_else(|| sanitize_function_name(&descriptor.name));
        let identity = ResponsesToolIdentity {
            name: descriptor.local_name,
            namespace: descriptor.namespace,
            custom: descriptor.tool_type == "custom",
            apply_patch: descriptor.apply_patch,
        };
        out.insert(mapped.clone(), identity.clone());
        if descriptor.name != mapped {
            out.insert(descriptor.name, identity);
        }
    }
    out
}
