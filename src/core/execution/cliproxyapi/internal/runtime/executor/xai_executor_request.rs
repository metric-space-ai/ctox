// ref: internal/runtime/executor/xai_executor_request.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use crate::sdk::cliproxy::auth::Auth;
use crate::sdk::cliproxy::executor::Headers;

use super::xai_executor::{
    header_set, XAI_AUTHENTICATE_RESPONSE_HEADER, XAI_AUTHENTICATE_RESPONSE_VALUE,
    XAI_CLIENT_IDENTIFIER_HEADER, XAI_CLIENT_IDENTIFIER_VALUE, XAI_CLIENT_VERSION_HEADER,
    XAI_CLIENT_VERSION_VALUE, XAI_TOKEN_AUTH_HEADER, XAI_TOKEN_AUTH_VALUE,
};

pub const XAI_IMAGES_GENERATIONS_PATH: &str = "/images/generations";
pub const XAI_IMAGES_EDITS_PATH: &str = "/images/edits";
pub const XAI_VIDEOS_GENERATIONS_PATH: &str = "/videos/generations";
pub const XAI_VIDEOS_EDITS_PATH: &str = "/videos/edits";
pub const XAI_VIDEOS_EXTENSIONS_PATH: &str = "/videos/extensions";
pub const XAI_VIDEOS_PATH: &str = "/videos";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XaiCredentials {
    pub token: String,
    pub base_url: String,
    pub using_api: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct XaiPreparedRequest {
    pub body: Vec<u8>,
    pub base_model: String,
    pub session_id: String,
    pub namespace_tools: BTreeMap<String, NamespaceToolRef>,
    pub client_declared_tools: BTreeSet<ClientToolKey>,
    pub filter_internal_x_search: bool,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ClientToolKey {
    pub tool_type: String,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamespaceToolRef {
    pub namespace: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct XaiRequestPolicy<'a> {
    pub model: &'a str,
    pub stream: bool,
    pub inject_x_search: bool,
    pub session_id: Option<&'a str>,
    pub reasoning_effort: Option<&'a str>,
}

#[must_use]
pub fn xai_credentials(auth: Option<&Auth>) -> XaiCredentials {
    let token = auth
        .and_then(|a| {
            a.attributes
                .get("api_key")
                .map(String::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .or_else(|| metadata_string(&a.metadata, "access_token"))
        })
        .unwrap_or_default()
        .trim()
        .to_owned();
    let base_url = auth
        .and_then(|a| {
            a.attributes
                .get("base_url")
                .map(String::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .or_else(|| metadata_string(&a.metadata, "base_url"))
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(super::xai_executor::DEFAULT_XAI_API_BASE_URL)
        .trim_end_matches('/')
        .to_owned();
    let using_api = xai_using_api(auth);
    XaiCredentials {
        token,
        base_url,
        using_api,
    }
}

// ref: internal/runtime/executor/xai_executor_request.go:216-253 @ e2bff010
fn xai_using_api(auth: Option<&Auth>) -> bool {
    let Some(auth) = auth else {
        return true;
    };
    if let Some(value) = auth
        .attributes
        .get("using_api")
        .and_then(|value| xai_parse_bool(value))
    {
        return value;
    }
    if let Some(value) = auth.metadata.get("using_api").and_then(|value| {
        value
            .as_bool()
            .or_else(|| value.as_str().and_then(xai_parse_bool))
    }) {
        return value;
    }
    let kind = auth
        .attributes
        .get("auth_kind")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| metadata_string(&auth.metadata, "auth_kind"))
        .map(str::trim);
    !kind.is_some_and(|kind| kind.eq_ignore_ascii_case("oauth"))
}

fn xai_parse_bool(value: &str) -> Option<bool> {
    match value.trim() {
        "1" | "t" | "T" | "TRUE" | "true" | "True" => Some(true),
        "0" | "f" | "F" | "FALSE" | "false" | "False" => Some(false),
        _ => None,
    }
}

// ref: internal/runtime/executor/xai_executor_request.go:263-296 @ e2bff010
#[must_use]
pub fn xai_chat_base_url(auth: Option<&Auth>) -> String {
    let credentials = xai_credentials(auth);
    if credentials.using_api
        || credentials.base_url != super::xai_executor::DEFAULT_XAI_API_BASE_URL
    {
        credentials.base_url
    } else {
        super::xai_executor::DEFAULT_XAI_CHAT_BASE_URL.to_owned()
    }
}

#[must_use]
pub fn xai_compact_base_url(auth: Option<&Auth>) -> String {
    let base_url = xai_credentials(auth).base_url;
    if base_url == super::xai_executor::DEFAULT_XAI_CHAT_BASE_URL {
        super::xai_executor::DEFAULT_XAI_API_BASE_URL.to_owned()
    } else {
        base_url
    }
}

#[must_use]
pub fn xai_base_url_source(base_url: &str) -> &'static str {
    if base_url.trim_end_matches('/') == super::xai_executor::DEFAULT_XAI_API_BASE_URL {
        "default_api"
    } else if base_url.contains("chat") || base_url.contains("proxy") {
        "chat_proxy"
    } else {
        "custom_api"
    }
}

pub fn apply_xai_headers(
    headers: &mut Headers,
    auth: Option<&Auth>,
    token: &str,
    stream: bool,
    session_id: &str,
) {
    apply_xai_default_headers(headers, token, stream, session_id);
    apply_xai_custom_headers(headers, auth);
}

// ref: internal/runtime/executor/xai_executor_request.go:321-351 @ e2bff010
fn apply_xai_default_headers(headers: &mut Headers, token: &str, stream: bool, session_id: &str) {
    if token.trim().is_empty() {
        headers.retain(|name, _| !name.eq_ignore_ascii_case("Authorization"));
    } else {
        header_set(headers, "Authorization", format!("Bearer {}", token.trim()));
    }
    header_set(headers, "Content-Type", "application/json");
    header_set(
        headers,
        "Accept",
        if stream {
            "text/event-stream"
        } else {
            "application/json"
        },
    );
    header_set(headers, "Connection", "Keep-Alive");
    if !session_id.trim().is_empty() {
        header_set(headers, "x-grok-conv-id", session_id.trim());
    }
}

fn apply_xai_custom_headers(headers: &mut Headers, auth: Option<&Auth>) {
    if let Some(auth) = auth {
        for (key, value) in &auth.attributes {
            if let Some(name) = key
                .strip_prefix("header:")
                .filter(|name| !name.trim().is_empty())
            {
                header_set(headers, name.trim(), value.clone());
            }
        }
    }
}

pub fn apply_xai_chat_headers(
    headers: &mut Headers,
    auth: Option<&Auth>,
    token: &str,
    stream: bool,
    session_id: &str,
) {
    apply_xai_default_headers(headers, token, stream, session_id);
    // ref: internal/runtime/executor/xai_executor_request.go:356-365 @ e2bff010
    if !xai_credentials(auth).using_api
        && xai_chat_base_url(auth) == super::xai_executor::DEFAULT_XAI_CHAT_BASE_URL
    {
        header_set(headers, XAI_TOKEN_AUTH_HEADER, XAI_TOKEN_AUTH_VALUE);
        header_set(headers, XAI_CLIENT_VERSION_HEADER, XAI_CLIENT_VERSION_VALUE);
        header_set(
            headers,
            "User-Agent",
            format!("xai-grok-workspace/{XAI_CLIENT_VERSION_VALUE}"),
        );
        header_set(
            headers,
            XAI_CLIENT_IDENTIFIER_HEADER,
            XAI_CLIENT_IDENTIFIER_VALUE,
        );
        header_set(
            headers,
            XAI_AUTHENTICATE_RESPONSE_HEADER,
            XAI_AUTHENTICATE_RESPONSE_VALUE,
        );
    }
    apply_xai_custom_headers(headers, auth);
}

pub fn prepare_xai_responses_body(
    body: &[u8],
    policy: XaiRequestPolicy<'_>,
) -> Result<XaiPreparedRequest, serde_json::Error> {
    let mut root: Value = serde_json::from_slice(body)?;
    let object = root
        .as_object_mut()
        .ok_or_else(|| custom_json_error("xAI request must be an object"))?;
    object.insert(
        "model".into(),
        Value::String(strip_thinking_suffix(policy.model)),
    );
    object.insert("stream".into(), Value::Bool(policy.stream));
    object.remove("stop");
    if let Some(effort) = policy
        .reasoning_effort
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        object
            .entry("reasoning")
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(reasoning) = object.get_mut("reasoning").and_then(Value::as_object_mut) {
            reasoning.insert("effort".into(), Value::String(effort.to_owned()));
        }
    }
    promote_additional_tools(&mut root);
    normalize_tools(&mut root);
    normalize_input_custom_tool_calls(&mut root);
    if policy.inject_x_search {
        ensure_native_x_search(&mut root);
    }
    prune_orphaned_tool_choice(&mut root);
    let namespace_tools = collect_namespace_tool_refs(&root);
    let client_declared_tools = collect_client_declared_tools(&root);
    let filter_internal_x_search = request_has_native_x_search(&root);
    let session_id = policy.session_id.unwrap_or_default().trim().to_owned();
    Ok(XaiPreparedRequest {
        body: serde_json::to_vec(&root)?,
        base_model: strip_thinking_suffix(policy.model),
        session_id,
        namespace_tools,
        client_declared_tools,
        filter_internal_x_search,
    })
}

#[must_use]
pub fn normalize_image_refs(body: &[u8]) -> Vec<u8> {
    let Ok(mut value) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    rewrite_image_refs(&mut value);
    serde_json::to_vec(&value).unwrap_or_else(|_| body.to_vec())
}

fn rewrite_image_refs(value: &mut Value) {
    match value {
        Value::Array(values) => values.iter_mut().for_each(rewrite_image_refs),
        Value::Object(object) => {
            if let Some(url) = object.remove("image_url") {
                object.entry("url").or_insert(url);
            }
            for child in object.values_mut() {
                rewrite_image_refs(child);
            }
        }
        _ => {}
    }
}

fn promote_additional_tools(root: &mut Value) {
    let mut promoted = Vec::new();
    if let Some(input) = root.get_mut("input").and_then(Value::as_array_mut) {
        input.retain_mut(|item| {
            if item.get("type").and_then(Value::as_str) == Some("additional_tools") {
                if let Some(tools) = item.get_mut("tools").and_then(Value::as_array_mut) {
                    promoted.append(tools);
                }
                false
            } else {
                true
            }
        });
    }
    if !promoted.is_empty() {
        root.as_object_mut()
            .unwrap()
            .entry("tools")
            .or_insert_with(|| Value::Array(Vec::new()));
        root.get_mut("tools")
            .and_then(Value::as_array_mut)
            .unwrap()
            .append(&mut promoted);
    }
}

fn normalize_tools(root: &mut Value) {
    let Some(tools) = root.get_mut("tools").and_then(Value::as_array_mut) else {
        return;
    };
    let mut flattened = Vec::new();
    for mut tool in std::mem::take(tools) {
        let is_namespace = tool.get("type").and_then(Value::as_str) == Some("namespace");
        if is_namespace {
            let namespace = tool
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            if let Some(children) = tool.get_mut("tools").and_then(Value::as_array_mut) {
                for mut child in std::mem::take(children) {
                    normalize_function_tool(&mut child, Some(&namespace));
                    flattened.push(child);
                }
            }
        } else {
            normalize_function_tool(&mut tool, None);
            flattened.push(tool);
        }
    }
    *tools = flattened;
}

fn normalize_function_tool(tool: &mut Value, namespace: Option<&str>) {
    let Some(object) = tool.as_object_mut() else {
        return;
    };
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if kind == "custom" {
        object.insert("type".into(), Value::String("function".into()));
    }
    if object.get("type").and_then(Value::as_str) != Some("function") {
        return;
    }
    if let Some(namespace) = namespace {
        if let Some(name) = object.get("name").and_then(Value::as_str) {
            object.insert("name".into(), Value::String(format!("{namespace}__{name}")));
        }
    }
    if let Some(parameters) = object.get_mut("parameters").and_then(Value::as_object_mut) {
        ensure_object_union_types(parameters);
    }
}

fn ensure_object_union_types(schema: &mut Map<String, Value>) {
    for key in ["oneOf", "anyOf", "allOf"] {
        if let Some(branches) = schema.get_mut(key).and_then(Value::as_array_mut) {
            for branch in branches {
                if let Some(branch) = branch.as_object_mut() {
                    branch
                        .entry("type")
                        .or_insert(Value::String("object".into()));
                }
            }
        }
    }
}

fn ensure_native_x_search(root: &mut Value) {
    let object = root.as_object_mut().unwrap();
    let tools = object
        .entry("tools")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .unwrap();
    if !tools
        .iter()
        .any(|tool| tool.get("type").and_then(Value::as_str) == Some("x_search"))
    {
        tools.push(serde_json::json!({"type":"x_search"}));
    }
}

fn prune_orphaned_tool_choice(root: &mut Value) {
    let Some(choice) = root.get("tool_choice") else {
        return;
    };
    let name = choice
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if name.is_empty() {
        return;
    }
    let found = root
        .get("tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| {
            tools
                .iter()
                .any(|tool| tool.get("name").and_then(Value::as_str) == Some(name))
        });
    if !found {
        root.as_object_mut().unwrap().remove("tool_choice");
    }
}

fn normalize_input_custom_tool_calls(root: &mut Value) {
    let Some(input) = root.get_mut("input").and_then(Value::as_array_mut) else {
        return;
    };
    for item in input {
        let Some(object) = item.as_object_mut() else {
            continue;
        };
        match object.get("type").and_then(Value::as_str) {
            Some("custom_tool_call") => {
                object.insert("type".into(), Value::String("function_call".into()));
                if let Some(input) = object.remove("input") {
                    object.insert("arguments".into(), input);
                }
            }
            Some("custom_tool_call_output") => {
                object.insert("type".into(), Value::String("function_call_output".into()));
            }
            _ => {}
        }
    }
}

fn collect_namespace_tool_refs(root: &Value) -> BTreeMap<String, NamespaceToolRef> {
    root.get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|tool| {
            let name = tool.get("name")?.as_str()?;
            let (namespace, original) = name.split_once("__")?;
            Some((
                name.to_owned(),
                NamespaceToolRef {
                    namespace: namespace.to_owned(),
                    name: original.to_owned(),
                },
            ))
        })
        .collect()
}

fn collect_client_declared_tools(root: &Value) -> BTreeSet<ClientToolKey> {
    root.get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|tool| {
            let tool_type = tool.get("type")?.as_str()?.to_owned();
            let name = tool
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            Some(ClientToolKey { tool_type, name })
        })
        .collect()
}

fn request_has_native_x_search(root: &Value) -> bool {
    root.get("tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| {
            tools
                .iter()
                .any(|tool| tool.get("type").and_then(Value::as_str) == Some("x_search"))
        })
}
fn strip_thinking_suffix(model: &str) -> String {
    model
        .trim()
        .split_once('(')
        .map_or(model.trim(), |(base, _)| base.trim())
        .to_owned()
}
fn metadata_string<'a>(metadata: &'a BTreeMap<String, Value>, key: &str) -> Option<&'a str> {
    metadata.get(key).and_then(Value::as_str)
}
fn custom_json_error(message: &str) -> serde_json::Error {
    serde_json::Error::io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message,
    ))
}
