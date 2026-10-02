// ref: internal/runtime/executor/codex_executor_request.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::{Map, Value};
use uuid::Uuid;

#[cfg(feature = "codex-http-transport")]
use super::codex_executor::CodexResponsesRequest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexIdentityPolicy {
    pub enabled: bool,
    pub session_affinity: bool,
    pub routing_strategy: String,
}

impl CodexIdentityPolicy {
    pub fn active(&self) -> bool {
        self.enabled
            && (self.session_affinity
                || matches!(
                    self.routing_strategy.trim().to_ascii_lowercase().as_str(),
                    "fill-first" | "fillfirst" | "ff"
                ))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodexIdentityConfuseState {
    enabled: bool,
    original_prompt_cache_key: String,
    prompt_cache_key: String,
    turn_ids: Vec<(String, String)>,
}

impl CodexIdentityConfuseState {
    pub fn upstream_prompt_cache_key(&self) -> Option<&str> {
        (!self.prompt_cache_key.is_empty()).then_some(self.prompt_cache_key.as_str())
    }

    pub fn expose_response(&self, payload: &[u8]) -> Vec<u8> {
        if !self.enabled {
            return payload.to_vec();
        }
        let mut exposed = replace_bytes(
            payload,
            self.prompt_cache_key.as_bytes(),
            self.original_prompt_cache_key.as_bytes(),
        );
        for (original, confused) in &self.turn_ids {
            exposed = replace_bytes(&exposed, confused.as_bytes(), original.as_bytes());
        }
        exposed
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodexHeaderPolicy {
    pub disable_cloaking: bool,
    pub configured_user_agent: Option<String>,
    pub configured_beta_features: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodexRequestPolicy<'a> {
    pub model: &'a str,
    pub plan_type: &'a str,
    pub responses_lite: bool,
    pub disable_image_generation: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexRequestError {
    InvalidJson,
    InvalidObject,
    MissingModel,
}

/// Applies the accepted upstream request subset before the subscription POST.
/// ref: internal/runtime/executor/codex_executor_execute.go:57-78
pub fn prepare_codex_responses_body(
    body: &[u8],
    policy: CodexRequestPolicy<'_>,
) -> Result<Vec<u8>, CodexRequestError> {
    if policy.model.trim().is_empty() {
        return Err(CodexRequestError::MissingModel);
    }
    let mut value: Value =
        serde_json::from_slice(body).map_err(|_| CodexRequestError::InvalidJson)?;
    let object = value
        .as_object_mut()
        .ok_or(CodexRequestError::InvalidObject)?;
    object.insert("model".to_owned(), Value::String(policy.model.to_owned()));
    object.insert("stream".to_owned(), Value::Bool(true));
    for field in [
        "previous_response_id",
        "generate",
        "prompt_cache_retention",
        "safety_identifier",
        "stream_options",
    ] {
        object.remove(field);
    }
    if object.get("instructions").is_none_or(Value::is_null) {
        object.insert("instructions".to_owned(), Value::String(String::new()));
    }
    normalize_parallel_tool_calls(object, policy.responses_lite);
    if !policy.disable_image_generation {
        ensure_image_generation_tool(object, policy);
    }
    let body = serde_json::to_vec(&value).map_err(|_| CodexRequestError::InvalidJson)?;
    let body = super::helps::normalize_codex_tool_schemas(&body);
    Ok(super::helps::sanitize_codex_input_item_ids(&body))
}

pub fn prepare_codex_compact_body(body: &[u8], model: &str) -> Result<Vec<u8>, CodexRequestError> {
    if model.trim().is_empty() {
        return Err(CodexRequestError::MissingModel);
    }
    let mut value: Value =
        serde_json::from_slice(body).map_err(|_| CodexRequestError::InvalidJson)?;
    let object = value
        .as_object_mut()
        .ok_or(CodexRequestError::InvalidObject)?;
    object.insert("model".to_owned(), Value::String(model.to_owned()));
    object.remove("stream");
    if object.get("instructions").is_none_or(Value::is_null) {
        object.insert("instructions".to_owned(), Value::String(String::new()));
    }
    let body = serde_json::to_vec(&value).map_err(|_| CodexRequestError::InvalidJson)?;
    let body = super::helps::normalize_codex_tool_schemas(&body);
    Ok(super::helps::sanitize_codex_input_item_ids(&body))
}

pub fn apply_codex_identity_confuse_body(
    policy: &CodexIdentityPolicy,
    auth_id: &str,
    user_payload: &[u8],
    upstream_body: &[u8],
) -> (Vec<u8>, CodexIdentityConfuseState) {
    let auth_id = auth_id.trim();
    if !policy.active() || auth_id.is_empty() || upstream_body.is_empty() {
        return (upstream_body.to_vec(), CodexIdentityConfuseState::default());
    }
    let Ok(user) = serde_json::from_slice::<Value>(user_payload) else {
        return (upstream_body.to_vec(), CodexIdentityConfuseState::default());
    };
    let Ok(mut upstream) = serde_json::from_slice::<Value>(upstream_body) else {
        return (upstream_body.to_vec(), CodexIdentityConfuseState::default());
    };
    let Some(object) = upstream.as_object_mut() else {
        return (upstream_body.to_vec(), CodexIdentityConfuseState::default());
    };
    let mut state = CodexIdentityConfuseState {
        enabled: true,
        ..CodexIdentityConfuseState::default()
    };
    if let Some(prompt_cache_key) = user
        .get("prompt_cache_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        state.original_prompt_cache_key = prompt_cache_key.to_owned();
        state.prompt_cache_key =
            codex_identity_confuse_uuid(auth_id, "prompt-cache", prompt_cache_key);
        object.insert(
            "prompt_cache_key".to_owned(),
            Value::String(state.prompt_cache_key.clone()),
        );
    }
    if let Some(metadata) = object
        .get_mut("client_metadata")
        .and_then(Value::as_object_mut)
    {
        if let Some(installation_id) = user
            .pointer("/client_metadata/x-codex-installation-id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            metadata.insert(
                "x-codex-installation-id".to_owned(),
                Value::String(codex_identity_confuse_uuid(
                    auth_id,
                    "installation",
                    installation_id,
                )),
            );
        }
        if let Some(turn_metadata) = metadata
            .get("x-codex-turn-metadata")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
        {
            metadata.insert(
                "x-codex-turn-metadata".to_owned(),
                Value::String(confuse_turn_metadata(auth_id, &turn_metadata, &mut state)),
            );
        }
        if !state.prompt_cache_key.is_empty() && metadata.get("x-codex-window-id").is_some() {
            metadata.insert(
                "x-codex-window-id".to_owned(),
                Value::String(format!("{}:0", state.prompt_cache_key)),
            );
        }
    }
    (
        serde_json::to_vec(&upstream).unwrap_or_else(|_| upstream_body.to_vec()),
        state,
    )
}

pub fn apply_codex_identity_confuse_headers(
    headers: &mut std::collections::BTreeMap<String, String>,
    auth_id: &str,
    state: &mut CodexIdentityConfuseState,
) {
    if !state.enabled {
        return;
    }
    if let Some((key, raw)) = headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("x-codex-turn-metadata"))
        .map(|(key, value)| (key.clone(), value.clone()))
    {
        headers.insert(key, confuse_turn_metadata(auth_id, &raw, state));
    }
    if state.prompt_cache_key.is_empty() {
        return;
    }
    for key in [
        "Session_id",
        "X-Client-Request-Id",
        "Thread-Id",
        "Conversation_id",
    ] {
        if key != "Conversation_id"
            || headers
                .keys()
                .any(|candidate| candidate.eq_ignore_ascii_case(key))
        {
            set_case_insensitive(headers, key, &state.prompt_cache_key);
        }
    }
    set_case_insensitive(
        headers,
        "X-Codex-Window-Id",
        &format!("{}:0", state.prompt_cache_key),
    );
}

pub fn apply_codex_cloaking_headers(
    headers: &mut std::collections::BTreeMap<String, String>,
    policy: &CodexHeaderPolicy,
) {
    if policy.disable_cloaking {
        if let Some(user_agent) = policy
            .configured_user_agent
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            set_case_insensitive(headers, "User-Agent", user_agent);
        }
    } else {
        set_case_insensitive(
            headers,
            "User-Agent",
            super::codex_executor::CODEX_USER_AGENT,
        );
        set_case_insensitive(
            headers,
            "Originator",
            super::codex_executor::CODEX_ORIGINATOR,
        );
    }
    if let Some(beta) = policy
        .configured_beta_features
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        set_case_insensitive(headers, "OpenAI-Beta", beta);
    }
}

fn confuse_turn_metadata(
    auth_id: &str,
    raw: &str,
    state: &mut CodexIdentityConfuseState,
) -> String {
    let Ok(mut metadata) = serde_json::from_str::<Value>(raw) else {
        return if !state.prompt_cache_key.is_empty() && !state.original_prompt_cache_key.is_empty()
        {
            raw.replace(&state.original_prompt_cache_key, &state.prompt_cache_key)
        } else {
            raw.to_owned()
        };
    };
    let Some(object) = metadata.as_object_mut() else {
        return raw.to_owned();
    };
    if !state.prompt_cache_key.is_empty() {
        if object.contains_key("prompt_cache_key") {
            object.insert(
                "prompt_cache_key".to_owned(),
                Value::String(state.prompt_cache_key.clone()),
            );
        }
        if object.contains_key("window_id") {
            object.insert(
                "window_id".to_owned(),
                Value::String(format!("{}:0", state.prompt_cache_key)),
            );
        }
    }
    if let Some(turn_id) = object
        .get("turn_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
    {
        let confused = state
            .turn_ids
            .iter()
            .find(|(original, _)| original == &turn_id)
            .map(|(_, confused)| confused.clone())
            .unwrap_or_else(|| codex_identity_confuse_uuid(auth_id, "turn", &turn_id));
        if !state
            .turn_ids
            .iter()
            .any(|(original, _)| original == &turn_id)
        {
            state.turn_ids.push((turn_id, confused.clone()));
        }
        object.insert("turn_id".to_owned(), Value::String(confused));
    }
    serde_json::to_string(&metadata).unwrap_or_else(|_| raw.to_owned())
}

fn codex_identity_confuse_uuid(auth_id: &str, kind: &str, value: &str) -> String {
    let name = format!(
        "cli-proxy-api:codex:identity-confuse:{kind}:{}:{}",
        auth_id.trim(),
        value.trim()
    );
    Uuid::new_v5(&Uuid::NAMESPACE_OID, name.as_bytes()).to_string()
}

fn set_case_insensitive(
    headers: &mut std::collections::BTreeMap<String, String>,
    key: &str,
    value: &str,
) {
    headers.retain(|candidate, _| !candidate.eq_ignore_ascii_case(key));
    headers.insert(key.to_owned(), value.to_owned());
}

fn replace_bytes(payload: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    if from.is_empty() || to.is_empty() || from == to {
        return payload.to_vec();
    }
    let mut output = Vec::with_capacity(payload.len());
    let mut cursor = 0;
    while let Some(offset) = payload[cursor..]
        .windows(from.len())
        .position(|window| window == from)
    {
        let index = cursor + offset;
        output.extend_from_slice(&payload[cursor..index]);
        output.extend_from_slice(to);
        cursor = index + from.len();
    }
    output.extend_from_slice(&payload[cursor..]);
    output
}

fn normalize_parallel_tool_calls(object: &mut Map<String, Value>, responses_lite: bool) {
    if responses_lite {
        object.insert("parallel_tool_calls".to_owned(), Value::Bool(false));
        return;
    }
    let has_tools = object
        .get("tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| !tools.is_empty());
    if !has_tools {
        object.remove("parallel_tool_calls");
    }
}

fn ensure_image_generation_tool(object: &mut Map<String, Value>, policy: CodexRequestPolicy<'_>) {
    if policy.responses_lite
        || policy.model.ends_with("spark")
        || policy.plan_type.eq_ignore_ascii_case("free")
    {
        return;
    }
    let tools = object
        .entry("tools".to_owned())
        .or_insert_with(|| Value::Array(Vec::new()));
    if !tools.is_array() {
        *tools = Value::Array(Vec::new());
    }
    let tools = tools.as_array_mut().expect("array assigned above");
    let already_present = tools.iter().any(|tool| {
        tool.get("type").and_then(Value::as_str) == Some("image_generation")
            || tool.get("type").and_then(Value::as_str) == Some("function")
                && tool.get("name").and_then(Value::as_str) == Some("image_gen.imagegen")
            || tool.get("type").and_then(Value::as_str) == Some("namespace")
                && tool.get("name").and_then(Value::as_str) == Some("image_gen")
                && tool
                    .get("tools")
                    .and_then(Value::as_array)
                    .is_some_and(|nested| {
                        nested.iter().any(|tool| {
                            tool.get("type").and_then(Value::as_str) == Some("function")
                                && tool.get("name").and_then(Value::as_str) == Some("imagegen")
                        })
                    })
    });
    if !already_present {
        tools.push(serde_json::json!({
            "type": "image_generation",
            "output_format": "png"
        }));
    }
}

#[cfg(feature = "codex-http-transport")]
mod native {
    use std::future::Future;
    use std::pin::Pin;
    use std::time::Duration;

    use futures_util::StreamExt;
    use tokio::sync::mpsc;
    use wreq::header::{ACCEPT, AUTHORIZATION, CONNECTION, CONTENT_TYPE, RETRY_AFTER, USER_AGENT};
    use wreq::{Client, IntoEmulation, Proxy};
    use wreq_util::Emulation;

    use super::*;
    use crate::internal::runtime::executor::codex_executor::{
        CodexResponsesResponse, CodexResponsesStreamResponse, CodexResponsesStreamingTransport,
        CodexResponsesTransport, CodexResponsesTransportFailure, CODEX_ORIGINATOR,
        CODEX_USER_AGENT,
    };

    const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

    #[derive(Clone)]
    pub struct CodexResponsesHttpTransport {
        client: Client,
    }

    impl CodexResponsesHttpTransport {
        pub fn new(proxy_url: Option<&str>) -> Result<Self, CodexResponsesTransportBuildError> {
            let mut emulation = Emulation::Chrome133.into_emulation();
            emulation.headers.clear();
            emulation.orig_headers = wreq::header::OrigHeaderMap::new();
            let mut builder = Client::builder()
                .emulation(emulation)
                .connect_timeout(CONNECT_TIMEOUT)
                .retry(wreq::retry::Policy::never())
                .redirect(wreq::redirect::Policy::none());
            match proxy_url.map(str::trim).filter(|value| !value.is_empty()) {
                Some(proxy_url) => {
                    let proxy = Proxy::all(proxy_url)
                        .map_err(|_| CodexResponsesTransportBuildError::InvalidProxy)?;
                    builder = builder.proxy(proxy);
                }
                None => builder = builder.no_proxy(),
            }
            Ok(Self {
                client: builder
                    .build()
                    .map_err(|_| CodexResponsesTransportBuildError::Client)?,
            })
        }

        fn prepare_outgoing(
            &self,
            request: &CodexResponsesRequest,
            timeout: Duration,
        ) -> wreq::RequestBuilder {
            let mut outgoing = self
                .client
                .post(request.url())
                .header(CONTENT_TYPE, request.content_type())
                .header(
                    ACCEPT,
                    if request.stream() {
                        "text/event-stream"
                    } else {
                        "application/json"
                    },
                )
                .header(CONNECTION, "Keep-Alive")
                .header(USER_AGENT, CODEX_USER_AGENT)
                .header("Originator", CODEX_ORIGINATOR)
                .header(
                    AUTHORIZATION,
                    format!("Bearer {}", request.access_token().expose_secret()),
                )
                .timeout(timeout)
                .body(request.body().to_vec());
            if !request.account_id().is_empty() {
                outgoing = outgoing.header("Chatgpt-Account-Id", request.account_id());
            }
            if let Some(session_id) = request.session_id() {
                outgoing = outgoing.header("Session_id", session_id);
            }
            for (key, value) in request.headers() {
                outgoing = outgoing.header(key, value);
            }
            outgoing
        }
    }

    impl CodexResponsesTransport for CodexResponsesHttpTransport {
        fn execute<'a>(
            &'a self,
            request: &'a CodexResponsesRequest,
            timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<CodexResponsesResponse, CodexResponsesTransportFailure>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                let response = self
                    .prepare_outgoing(request, timeout)
                    .send()
                    .await
                    .map_err(classify_transport_error)?;
                let status = response.status().as_u16();
                let retry_after = response
                    .headers()
                    .get(RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .map(ToOwned::to_owned);
                let body = response
                    .bytes()
                    .await
                    .map_err(classify_transport_error)?
                    .to_vec();
                Ok(CodexResponsesResponse::new(status, retry_after, body))
            })
        }
    }

    impl CodexResponsesStreamingTransport for CodexResponsesHttpTransport {
        fn execute_stream<'a>(
            &'a self,
            request: &'a CodexResponsesRequest,
            timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            CodexResponsesStreamResponse,
                            CodexResponsesTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                let response = self
                    .prepare_outgoing(request, timeout)
                    .send()
                    .await
                    .map_err(classify_transport_error)?;
                let status = response.status().as_u16();
                let retry_after = response
                    .headers()
                    .get(RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .map(ToOwned::to_owned);
                let (sender, receiver) = mpsc::channel(8);
                if (200..300).contains(&status) {
                    let mut bytes = response.bytes_stream();
                    tokio::spawn(async move {
                        while let Some(chunk) = bytes.next().await {
                            let chunk = chunk
                                .map(|value| value.to_vec())
                                .map_err(classify_transport_error);
                            let terminal = chunk.is_err();
                            if sender.send(chunk).await.is_err() || terminal {
                                break;
                            }
                        }
                    });
                }
                let mut stream = CodexResponsesStreamResponse::new(status, retry_after, receiver);
                if request.passthrough_stream() {
                    stream.set_passthrough();
                }
                Ok(stream)
            })
        }
    }

    fn classify_transport_error(error: wreq::Error) -> CodexResponsesTransportFailure {
        if error.is_timeout() {
            CodexResponsesTransportFailure::Timeout
        } else if error.is_connect() {
            CodexResponsesTransportFailure::Connect
        } else {
            CodexResponsesTransportFailure::Protocol
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum CodexResponsesTransportBuildError {
        InvalidProxy,
        Client,
    }

    impl std::fmt::Display for CodexResponsesTransportBuildError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("Codex Responses HTTP transport could not be built")
        }
    }

    impl std::error::Error for CodexResponsesTransportBuildError {}
}

#[cfg(feature = "codex-http-transport")]
pub use native::{CodexResponsesHttpTransport, CodexResponsesTransportBuildError};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_subscription_request_and_injects_image_tool() {
        let body = prepare_codex_responses_body(
            br#"{"model":"alias","input":"hello","instructions":null,"previous_response_id":"old","parallel_tool_calls":true}"#,
            CodexRequestPolicy {
                model: "gpt-5.5",
                plan_type: "plus",
                responses_lite: false,
                disable_image_generation: false,
            },
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["model"], "gpt-5.5");
        assert_eq!(value["stream"], true);
        assert_eq!(value["instructions"], "");
        assert!(value.get("previous_response_id").is_none());
        assert!(value.get("parallel_tool_calls").is_none());
        assert_eq!(value["tools"][0]["type"], "image_generation");
    }

    #[test]
    fn lite_and_free_policies_do_not_add_image_generation() {
        for (plan, lite) in [("free", false), ("plus", true)] {
            let body = prepare_codex_responses_body(
                br#"{"input":[],"parallel_tool_calls":true}"#,
                CodexRequestPolicy {
                    model: "gpt-5.5",
                    plan_type: plan,
                    responses_lite: lite,
                    disable_image_generation: false,
                },
            )
            .unwrap();
            let value: Value = serde_json::from_slice(&body).unwrap();
            assert!(value.get("tools").is_none());
            if lite {
                assert_eq!(value["parallel_tool_calls"], false);
            }
        }
    }

    #[test]
    fn long_input_ids_are_shortened_and_encrypted_reasoning_is_dropped() {
        let long = "x".repeat(90);
        let body = serde_json::json!({
            "input": [
                {"type":"message","id":long,"content":[]},
                {"type":"reasoning","id":"r".repeat(90),"encrypted_content":"cipher"}
            ]
        });
        let output = prepare_codex_responses_body(
            &serde_json::to_vec(&body).unwrap(),
            CodexRequestPolicy {
                model: "gpt-5.5",
                plan_type: "free",
                responses_lite: false,
                disable_image_generation: false,
            },
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["input"].as_array().unwrap().len(), 1);
        assert_eq!(
            value["input"][0]["id"].as_str().unwrap().chars().count(),
            64
        );
    }

    #[test]
    fn message_item_ids_receive_upstream_codex_prefix_before_length_check() {
        let output = prepare_codex_responses_body(
            br#"{"input":[{"type":"message","id":"sub2api-17","content":[]},{"type":"message","id":"msg_existing","content":[]}]}"#,
            CodexRequestPolicy {
                model: "gpt-5.5",
                plan_type: "free",
                responses_lite: false,
                disable_image_generation: true,
            },
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["input"][0]["id"], "msg_sub2api-17");
        assert_eq!(value["input"][1]["id"], "msg_existing");
    }

    #[test]
    fn identity_confuse_is_deterministic_and_exposes_downstream_identity() {
        let policy = CodexIdentityPolicy {
            enabled: true,
            session_affinity: true,
            routing_strategy: String::new(),
        };
        let raw = br#"{"prompt_cache_key":"client-session","client_metadata":{"x-codex-installation-id":"install","x-codex-window-id":"window","x-codex-turn-metadata":"{\"prompt_cache_key\":\"client-session\",\"window_id\":\"window\",\"turn_id\":\"turn\"}"}}"#;
        let (first, mut state) = apply_codex_identity_confuse_body(&policy, "auth-a", raw, raw);
        let (second, _) = apply_codex_identity_confuse_body(&policy, "auth-a", raw, raw);
        assert_eq!(first, second);
        let value: Value = serde_json::from_slice(&first).unwrap();
        let upstream = value["prompt_cache_key"].as_str().unwrap();
        assert_ne!(upstream, "client-session");
        assert_eq!(upstream.len(), 36);
        let mut headers = std::collections::BTreeMap::from([
            ("Session_id".to_owned(), "client-session".to_owned()),
            (
                "X-Codex-Turn-Metadata".to_owned(),
                r#"{"turn_id":"turn"}"#.to_owned(),
            ),
        ]);
        apply_codex_identity_confuse_headers(&mut headers, "auth-a", &mut state);
        assert_eq!(headers["Session_id"], upstream);
        let exposed = state.expose_response(
            format!(
                r#"{{"session":"{upstream}","turn":"{}"}}"#,
                state.turn_ids[0].1
            )
            .as_bytes(),
        );
        let exposed = String::from_utf8(exposed).unwrap();
        assert!(exposed.contains("client-session"));
        assert!(exposed.contains("turn"));
        assert!(!exposed.contains(upstream));
    }

    #[test]
    fn cloaking_is_default_and_can_be_explicitly_disabled() {
        let mut headers = std::collections::BTreeMap::from([
            ("User-Agent".to_owned(), "client".to_owned()),
            ("Originator".to_owned(), "client".to_owned()),
        ]);
        apply_codex_cloaking_headers(&mut headers, &CodexHeaderPolicy::default());
        assert_eq!(
            headers["User-Agent"],
            super::super::codex_executor::CODEX_USER_AGENT
        );
        assert_eq!(
            headers["Originator"],
            super::super::codex_executor::CODEX_ORIGINATOR
        );

        let mut headers = std::collections::BTreeMap::new();
        apply_codex_cloaking_headers(
            &mut headers,
            &CodexHeaderPolicy {
                disable_cloaking: true,
                configured_user_agent: Some("configured".to_owned()),
                configured_beta_features: Some("responses_websockets=2025-02-01".to_owned()),
            },
        );
        assert_eq!(headers["User-Agent"], "configured");
        assert_eq!(headers["OpenAI-Beta"], "responses_websockets=2025-02-01");
        assert!(!headers.contains_key("Originator"));
    }

    #[cfg(feature = "codex-http-transport")]
    #[tokio::test]
    async fn real_loopback_applies_codex_subscription_headers_and_body() {
        use std::sync::Arc;
        use std::time::Duration;

        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        use crate::internal::auth::codex::SecretString;
        use crate::internal::runtime::executor::codex_executor::{
            CodexResponsesTransport, CodexUpstreamTarget,
        };

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = CodexUpstreamTarget::new(format!(
            "http://{}/backend-api/codex",
            listener.local_addr().unwrap()
        ))
        .unwrap();
        let captured = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let server_capture = captured.clone();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 4096];
            loop {
                let read = socket.read(&mut buffer).await.unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                    let end = end + 4;
                    let headers = String::from_utf8_lossy(&request[..end]);
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|value| value.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= end + length {
                        break;
                    }
                }
            }
            *server_capture.lock().await = request;
            let body = b"data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"output\":[]}}\n\n";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            socket.write_all(body).await.unwrap();
        });

        let body = br#"{"model":"gpt-5.5","stream":true}"#.to_vec();
        let request = CodexResponsesRequest::new(
            &target,
            SecretString::new("access-wire-secret").unwrap(),
            "acct-wire",
            Some("session-wire".to_owned()),
            body.clone(),
        );
        let response = CodexResponsesHttpTransport::new(None)
            .unwrap()
            .execute(&request, Duration::from_secs(5))
            .await
            .unwrap();
        server.await.unwrap();
        assert_eq!(response.status(), 200);

        let request = String::from_utf8(captured.lock().await.clone()).unwrap();
        let lower = request.to_ascii_lowercase();
        assert!(request.starts_with("POST /backend-api/codex/responses HTTP/1.1\r\n"));
        assert!(lower.contains("authorization: bearer access-wire-secret"));
        assert!(lower.contains("chatgpt-account-id: acct-wire"));
        assert!(lower.contains("originator: codex-tui"));
        assert!(lower.contains("accept: text/event-stream"));
        assert!(lower.contains("session_id: session-wire"));
        assert!(request.ends_with(std::str::from_utf8(&body).unwrap()));
    }

    #[cfg(feature = "codex-http-transport")]
    #[tokio::test]
    async fn native_stream_exposes_bootstrap_before_upstream_tail() {
        use std::time::Duration;

        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        use tokio::sync::oneshot;

        use crate::internal::auth::codex::SecretString;
        use crate::internal::runtime::executor::codex_executor::{
            CodexResponsesRequest, CodexResponsesStreamingTransport, CodexUpstreamTarget,
        };

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = CodexUpstreamTarget::new(format!(
            "http://{}/backend-api/codex",
            listener.local_addr().unwrap()
        ))
        .unwrap();
        let (bootstrap_sent, bootstrap_seen) = oneshot::channel();
        let (release_tail, tail_released) = oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 4096];
            loop {
                let read = socket.read(&mut buffer).await.unwrap();
                request.extend_from_slice(&buffer[..read]);
                if let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end + 4]);
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|value| value.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();
            let first = b"data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_incremental\"}}\n\n";
            socket
                .write_all(format!("{:x}\r\n", first.len()).as_bytes())
                .await
                .unwrap();
            socket.write_all(first).await.unwrap();
            socket.write_all(b"\r\n").await.unwrap();
            let _ = bootstrap_sent.send(());
            let _ = tail_released.await;
            let tail = b"data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_incremental\",\"status\":\"completed\"}}\n\n";
            socket
                .write_all(format!("{:x}\r\n", tail.len()).as_bytes())
                .await
                .unwrap();
            socket.write_all(tail).await.unwrap();
            socket.write_all(b"\r\n0\r\n\r\n").await.unwrap();
        });

        let request = CodexResponsesRequest::new(
            &target,
            SecretString::new("stream-wire-secret").unwrap(),
            "acct-stream",
            None,
            br#"{"model":"gpt-5.5","stream":true}"#.to_vec(),
        );
        let mut response = CodexResponsesHttpTransport::new(None)
            .unwrap()
            .execute_stream(&request, Duration::from_secs(5))
            .await
            .unwrap();
        bootstrap_seen.await.unwrap();
        response.bootstrap_first_response_event().await.unwrap();
        let first = response.next_chunk().await.unwrap().unwrap();
        assert!(String::from_utf8_lossy(&first).contains("response.created"));
        release_tail.send(()).unwrap();
        let tail = response.next_chunk().await.unwrap().unwrap();
        assert!(String::from_utf8_lossy(&tail).contains("response.completed"));
        server.await.unwrap();
    }
}
