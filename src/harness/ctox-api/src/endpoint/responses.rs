use crate::auth::AuthProvider;
use crate::common::ResponseStream;
use crate::common::ResponsesApiRequest;
use crate::endpoint::session::EndpointSession;
use crate::error::ApiError;
use crate::provider::Provider;
#[cfg(test)]
use crate::provider::RetryConfig;
use crate::requests::headers::build_conversation_headers;
use crate::requests::headers::insert_header;
use crate::requests::headers::subagent_header;
use crate::requests::responses::Compression;
use crate::requests::responses::attach_item_ids;
use crate::sse::spawn_response_stream;
use crate::telemetry::SseTelemetry;
use ctox_client::HttpTransport;
use ctox_client::RequestCompression;
use ctox_client::RequestTelemetry;
use ctox_protocol::protocol::SessionSource;
use http::HeaderMap;
use http::HeaderValue;
use http::Method;
use ctox_protocol::models::ContentItem;
use ctox_protocol::models::ResponseItem;
use ctox_protocol::protocol::TokenUsage;
use serde_json::Value;
use serde_json::json;
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::sync::mpsc;
use tracing::instrument;

const OPENROUTER_CHAT_COMPLETION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

pub struct ResponsesClient<T: HttpTransport, A: AuthProvider> {
    session: EndpointSession<T, A>,
    sse_telemetry: Option<Arc<dyn SseTelemetry>>,
}

#[derive(Default)]
pub struct ResponsesOptions {
    pub conversation_id: Option<String>,
    pub session_source: Option<SessionSource>,
    pub extra_headers: HeaderMap,
    pub compression: Compression,
    pub turn_state: Option<Arc<OnceLock<String>>>,
}

impl<T: HttpTransport, A: AuthProvider> ResponsesClient<T, A> {
    pub fn new(transport: T, provider: Provider, auth: A) -> Self {
        Self {
            session: EndpointSession::new(transport, provider, auth),
            sse_telemetry: None,
        }
    }

    pub fn with_telemetry(
        self,
        request: Option<Arc<dyn RequestTelemetry>>,
        sse: Option<Arc<dyn SseTelemetry>>,
    ) -> Self {
        Self {
            session: self.session.with_request_telemetry(request),
            sse_telemetry: sse,
        }
    }

    #[instrument(
        name = "responses.stream_request",
        level = "info",
        skip_all,
        fields(
            transport = "responses_http",
            http.method = "POST",
            api.path = "responses"
        )
    )]
    pub async fn stream_request(
        &self,
        request: ResponsesApiRequest,
        options: ResponsesOptions,
    ) -> Result<ResponseStream, ApiError> {
        let ResponsesOptions {
            conversation_id,
            session_source,
            extra_headers,
            compression,
            turn_state,
        } = options;

        let mut body = serde_json::to_value(&request)
            .map_err(|e| ApiError::Stream(format!("failed to encode responses request: {e}")))?;
        attach_item_ids(&mut body, &request.input);

        let mut headers = extra_headers;
        if let Some(ref conv_id) = conversation_id {
            insert_header(&mut headers, "x-client-request-id", conv_id);
        }
        headers.extend(build_conversation_headers(conversation_id));
        if let Some(subagent) = subagent_header(&session_source) {
            insert_header(&mut headers, "x-openai-subagent", &subagent);
        }

        if should_use_openrouter_chat_adapter(self.session.provider(), &request.model) {
            return self
                .stream_openrouter_chat_completion(request, headers, turn_state)
                .await;
        }

        self.stream(body, headers, compression, turn_state).await
    }

    fn path() -> &'static str {
        "responses"
    }

    #[instrument(
        name = "responses.stream",
        level = "info",
        skip_all,
        fields(
            transport = "responses_http",
            http.method = "POST",
            api.path = "responses",
            turn.has_state = turn_state.is_some()
        )
    )]
    pub async fn stream(
        &self,
        body: Value,
        extra_headers: HeaderMap,
        compression: Compression,
        turn_state: Option<Arc<OnceLock<String>>>,
    ) -> Result<ResponseStream, ApiError> {
        let request_compression = match compression {
            Compression::None => RequestCompression::None,
            Compression::Zstd => RequestCompression::Zstd,
        };

        let stream_response = self
            .session
            .stream_with(
                Method::POST,
                Self::path(),
                extra_headers,
                Some(body),
                |req| {
                    req.headers.insert(
                        http::header::ACCEPT,
                        HeaderValue::from_static("text/event-stream"),
                    );
                    req.compression = request_compression;
                },
            )
            .await?;

        Ok(spawn_response_stream(
            stream_response,
            self.session.provider().stream_idle_timeout,
            self.sse_telemetry.clone(),
            turn_state,
        ))
    }

    async fn stream_openrouter_chat_completion(
        &self,
        request: ResponsesApiRequest,
        extra_headers: HeaderMap,
        turn_state: Option<Arc<OnceLock<String>>>,
    ) -> Result<ResponseStream, ApiError> {
        let body = openrouter_chat_completion_request(&request)?;
        let response = self
            .session
            .execute_with(Method::POST, "chat/completions", extra_headers, Some(body), |req| {
                req.timeout = Some(OPENROUTER_CHAT_COMPLETION_TIMEOUT);
            })
            .await?;
        if !response.status.is_success() {
            let message = String::from_utf8_lossy(&response.body).trim().to_string();
            return Err(ApiError::Api {
                status: response.status,
                message,
            });
        }
        let payload: Value = serde_json::from_slice(&response.body)
            .map_err(|err| ApiError::Stream(format!("failed to parse chat response: {err}")))?;
        chat_completion_response_stream(payload, request.model, turn_state)
    }
}

fn should_use_openrouter_chat_adapter(provider: &Provider, model: &str) -> bool {
    let base_url = provider.base_url.trim().to_ascii_lowercase();
    if !base_url.contains("openrouter.ai") {
        return false;
    }
    matches!(
        model.trim().to_ascii_lowercase().as_str(),
        "moonshotai/kimi-k2.5"
            | "moonshotai/kimi-k2.6"
            | "deepseek/deepseek-v4-flash"
            | "tencent/hy3-preview:free"
    )
}

fn openrouter_chat_completion_request(request: &ResponsesApiRequest) -> Result<Value, ApiError> {
    let mut messages = Vec::new();
    if !request.instructions.trim().is_empty() {
        messages.push(json!({
            "role": "system",
            "content": request.instructions,
        }));
    }
    for item in &request.input {
        match item {
            ResponseItem::Message { role, content, .. } => {
                let mapped_role = if role == "developer" { "system" } else { role };
                messages.push(json!({
                    "role": mapped_role,
                    "content": content_text(content),
                }));
            }
            ResponseItem::FunctionCall {
                name,
                arguments,
                call_id,
                ..
            } => {
                messages.push(json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": call_id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": arguments,
                        },
                    }],
                }));
            }
            ResponseItem::FunctionCallOutput { call_id, output } => {
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": output.to_string(),
                }));
            }
            ResponseItem::CustomToolCall {
                call_id,
                name,
                input,
                ..
            } => {
                messages.push(json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": call_id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": input,
                        },
                    }],
                }));
            }
            ResponseItem::CustomToolCallOutput { call_id, output } => {
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": output.to_string(),
                }));
            }
            _ => {}
        }
    }

    let mut body = serde_json::Map::new();
    body.insert("model".to_string(), Value::String(request.model.clone()));
    body.insert("messages".to_string(), Value::Array(messages));
    if !request.tools.is_empty() {
        body.insert(
            "tools".to_string(),
            Value::Array(
                request
                    .tools
                    .iter()
                    .filter_map(openrouter_chat_tool)
                    .collect(),
            ),
        );
        body.insert("tool_choice".to_string(), Value::String(request.tool_choice.clone()));
    }
    if let Some(max_output_tokens) = request.max_output_tokens {
        body.insert("max_tokens".to_string(), json!(max_output_tokens));
    }
    if let Some(reasoning) = &request.reasoning {
        let reasoning_value = serde_json::to_value(reasoning).map_err(|err| {
            ApiError::Stream(format!("failed to encode chat reasoning controls: {err}"))
        })?;
        body.insert("reasoning".to_string(), reasoning_value);
    }
    body.insert(
        "parallel_tool_calls".to_string(),
        Value::Bool(request.parallel_tool_calls),
    );
    body.insert("stream".to_string(), Value::Bool(false));
    Ok(Value::Object(body))
}

fn openrouter_chat_tool(tool: &Value) -> Option<Value> {
    if tool.get("type").and_then(Value::as_str) == Some("function")
        && tool.get("function").is_some()
    {
        return Some(tool.clone());
    }
    if tool.get("type").and_then(Value::as_str) != Some("function") {
        return None;
    }
    let name = tool.get("name").and_then(Value::as_str)?;
    let description = tool
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let parameters = tool
        .get("parameters")
        .cloned()
        .unwrap_or_else(|| json!({"type": "object"}));
    Some(json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": parameters,
        },
    }))
}

fn content_text(content: &[ContentItem]) -> String {
    content
        .iter()
        .filter_map(|item| match item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                Some(text.as_str())
            }
            ContentItem::InputImage { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn chat_completion_response_stream(
    payload: Value,
    fallback_model: String,
    _turn_state: Option<Arc<OnceLock<String>>>,
) -> Result<ResponseStream, ApiError> {
    let (tx_event, rx_event) = mpsc::channel(32);
    tokio::spawn(async move {
        let response_id = payload
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("resp_openrouter_chat")
            .to_string();
        let model = payload
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or(&fallback_model)
            .to_string();
        let _ = tx_event
            .send(Ok(crate::common::ResponseEvent::Created))
            .await;
        let _ = tx_event
            .send(Ok(crate::common::ResponseEvent::ServerModel(model)))
            .await;

        if let Some(choice) = payload
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            && let Some(message) = choice.get("message").and_then(Value::as_object)
        {
            if let Some(content) = message
                .get("content")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
            {
                let item = ResponseItem::Message {
                    id: None,
                    role: "assistant".to_string(),
                    content: vec![ContentItem::OutputText {
                        text: content.to_string(),
                    }],
                    end_turn: None,
                    phase: None,
                };
                let _ = tx_event
                    .send(Ok(crate::common::ResponseEvent::OutputTextDelta(
                        content.to_string(),
                    )))
                    .await;
                let _ = tx_event
                    .send(Ok(crate::common::ResponseEvent::OutputItemDone(item)))
                    .await;
            }
            if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
                for tool_call in tool_calls {
                    let function = tool_call.get("function").unwrap_or(tool_call);
                    let name = function
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let arguments = function
                        .get("arguments")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                        .unwrap_or_else(|| "{}".to_string());
                    let call_id = tool_call
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("call_openrouter_chat")
                        .to_string();
                    let item = ResponseItem::FunctionCall {
                        id: None,
                        name,
                        namespace: None,
                        arguments,
                        call_id,
                    };
                    let _ = tx_event
                        .send(Ok(crate::common::ResponseEvent::OutputItemDone(item)))
                        .await;
                }
            }
        }

        let token_usage = payload.get("usage").map(chat_token_usage);
        let _ = tx_event
            .send(Ok(crate::common::ResponseEvent::Completed {
                response_id,
                token_usage,
            }))
            .await;
    });
    Ok(ResponseStream { rx_event })
}

fn chat_token_usage(usage: &Value) -> TokenUsage {
    let input_tokens = usage
        .get("prompt_tokens")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let output_tokens = usage
        .get("completion_tokens")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let total_tokens = usage
        .get("total_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(input_tokens + output_tokens);
    TokenUsage {
        input_tokens,
        cached_input_tokens: 0,
        output_tokens,
        reasoning_output_tokens: 0,
        total_tokens,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Provider;
    use async_trait::async_trait;
    use ctox_client::Request;
    use ctox_client::Response;
    use ctox_client::StreamResponse;
    use ctox_client::TransportError;
    use http::StatusCode;
    use std::sync::Mutex;

    #[derive(Clone)]
    struct CapturingTransport {
        last_request: Arc<Mutex<Option<Request>>>,
        response_body: Arc<Vec<u8>>,
    }

    impl CapturingTransport {
        fn new(response_body: Vec<u8>) -> Self {
            Self {
                last_request: Arc::new(Mutex::new(None)),
                response_body: Arc::new(response_body),
            }
        }
    }

    #[async_trait]
    impl HttpTransport for CapturingTransport {
        async fn execute(&self, req: Request) -> Result<Response, TransportError> {
            *self.last_request.lock().expect("lock request store") = Some(req);
            Ok(Response {
                status: StatusCode::OK,
                headers: HeaderMap::new(),
                body: self.response_body.as_ref().clone().into(),
            })
        }

        async fn stream(&self, _req: Request) -> Result<StreamResponse, TransportError> {
            Err(TransportError::Build("stream should not run".to_string()))
        }
    }

    #[derive(Clone, Default)]
    struct DummyAuth;

    impl AuthProvider for DummyAuth {
        fn bearer_token(&self) -> Option<String> {
            None
        }
    }

    fn provider(base_url: &str) -> Provider {
        Provider {
            name: "openrouter".to_string(),
            base_url: base_url.to_string(),
            query_params: None,
            headers: HeaderMap::new(),
            retry: RetryConfig {
                max_attempts: 1,
                base_delay: std::time::Duration::from_millis(1),
                retry_429: false,
                retry_5xx: false,
                retry_transport: false,
            },
            stream_idle_timeout: std::time::Duration::from_secs(5),
        }
    }

    #[test]
    fn openrouter_deepseek_uses_chat_adapter() {
        assert!(should_use_openrouter_chat_adapter(
            &provider("https://openrouter.ai/api/v1"),
            "deepseek/deepseek-v4-flash"
        ));
        assert!(should_use_openrouter_chat_adapter(
            &provider("https://openrouter.ai/api/v1"),
            "tencent/hy3-preview:free"
        ));
        assert!(!should_use_openrouter_chat_adapter(
            &provider("https://api.openai.com/v1"),
            "deepseek/deepseek-v4-flash"
        ));
    }

    #[test]
    fn openrouter_chat_request_maps_responses_tools() {
        let request = ResponsesApiRequest {
            model: "deepseek/deepseek-v4-flash".to_string(),
            instructions: "Use tools.".to_string(),
            previous_response_id: None,
            input: vec![ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "hi".to_string(),
                }],
                end_turn: None,
                phase: None,
            }],
            tools: vec![json!({
                "type": "function",
                "name": "exec_command",
                "description": "run shell",
                "parameters": {"type": "object"}
            })],
            tool_choice: "auto".to_string(),
            parallel_tool_calls: true,
            reasoning: None,
            max_output_tokens: Some(128),
            store: false,
            stream: true,
            include: Vec::new(),
            service_tier: None,
            prompt_cache_key: None,
            text: None,
        };

        let body = openrouter_chat_completion_request(&request).unwrap();
        assert_eq!(body.get("stream"), Some(&Value::Bool(false)));
        assert_eq!(body.get("max_tokens"), Some(&json!(128)));
        assert_eq!(
            body.pointer("/tools/0/function/name"),
            Some(&json!("exec_command"))
        );
        assert_eq!(body.pointer("/messages/0/role"), Some(&json!("system")));
        assert_eq!(body.pointer("/messages/1/role"), Some(&json!("user")));
    }

    #[tokio::test]
    async fn openrouter_chat_adapter_sets_request_timeout() {
        let response_body = serde_json::to_vec(&json!({
            "id": "chatcmpl-timeout-test",
            "model": "deepseek/deepseek-v4-flash",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "ok"
                }
            }]
        }))
        .unwrap();
        let transport = CapturingTransport::new(response_body);
        let client = ResponsesClient::new(
            transport.clone(),
            provider("https://openrouter.ai/api/v1"),
            DummyAuth,
        );
        let request = ResponsesApiRequest {
            model: "deepseek/deepseek-v4-flash".to_string(),
            instructions: "Use tools.".to_string(),
            previous_response_id: None,
            input: vec![ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "hi".to_string(),
                }],
                end_turn: None,
                phase: None,
            }],
            tools: Vec::new(),
            tool_choice: "auto".to_string(),
            parallel_tool_calls: true,
            reasoning: None,
            max_output_tokens: Some(128),
            store: false,
            stream: true,
            include: Vec::new(),
            service_tier: None,
            prompt_cache_key: None,
            text: None,
        };

        let _stream = client
            .stream_request(
                request,
                ResponsesOptions {
                    conversation_id: None,
                    session_source: Some(SessionSource::Exec),
                    extra_headers: HeaderMap::new(),
                    compression: Compression::None,
                    turn_state: None,
                },
            )
            .await
            .expect("chat adapter should return stream");

        let captured = transport
            .last_request
            .lock()
            .expect("lock request store")
            .clone()
            .expect("request captured");
        assert_eq!(captured.url, "https://openrouter.ai/api/v1/chat/completions");
        assert_eq!(captured.timeout, Some(OPENROUTER_CHAT_COMPLETION_TIMEOUT));
    }
}
