use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

const DEFAULT_MESSAGE_ID: &str = "msg_ctox_gateway";
const RESPONSE_OBJECT: &str = "response";
const COMPLETED_STATUS: &str = "completed";
const ASSISTANT_ROLE: &str = "assistant";
const TOOL_ACTIVITY_ITEM_TYPES: &[&str] = &[
    "command_execution",
    "file_change",
    "mcp_tool_call",
    "custom_tool_call",
    "web_search",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TurnUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

impl TurnUsage {
    pub fn from_usage_payload(usage: Option<&Value>) -> Self {
        let empty = Value::Null;
        let usage = usage.unwrap_or(&empty);
        Self {
            input_tokens: usage
                .get("prompt_tokens")
                .and_then(Value::as_u64)
                .unwrap_or_else(|| {
                    usage
                        .get("input_tokens")
                        .and_then(Value::as_u64)
                        .unwrap_or(0)
                }),
            output_tokens: usage
                .get("completion_tokens")
                .and_then(Value::as_u64)
                .unwrap_or_else(|| {
                    usage
                        .get("output_tokens")
                        .and_then(Value::as_u64)
                        .unwrap_or(0)
                }),
            total_tokens: usage
                .get("total_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TurnOutputAnnotation {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TurnOutputTextPart {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: String,
    pub annotations: Vec<TurnOutputAnnotation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TurnMessage {
    pub id: String,
    pub status: String,
    pub role: String,
    pub content: Vec<TurnOutputTextPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TurnFunctionCall {
    pub call_id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TurnOutputItem {
    Message(TurnMessage),
    FunctionCall(TurnFunctionCall),
}

impl TurnOutputItem {
    pub fn assistant_message(text: impl Into<String>) -> Self {
        Self::Message(TurnMessage {
            id: DEFAULT_MESSAGE_ID.to_string(),
            status: COMPLETED_STATUS.to_string(),
            role: ASSISTANT_ROLE.to_string(),
            content: vec![TurnOutputTextPart {
                kind: "output_text".to_string(),
                text: text.into(),
                annotations: Vec::new(),
            }],
        })
    }

    pub fn function_call(
        call_id: impl Into<String>,
        name: impl Into<String>,
        arguments: impl Into<String>,
    ) -> Self {
        Self::FunctionCall(TurnFunctionCall {
            call_id: call_id.into(),
            name: name.into(),
            arguments: arguments.into(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TurnResponse {
    pub id: String,
    pub object: String,
    pub created_at: u64,
    pub completed_at: u64,
    pub model: String,
    pub status: String,
    pub output: Vec<TurnOutputItem>,
    pub output_text: Option<String>,
    pub reasoning: Option<String>,
    pub usage: TurnUsage,
}

impl TurnResponse {
    pub fn completed(
        id: impl Into<String>,
        model: impl Into<String>,
        created_at: u64,
        completed_at: u64,
        output: Vec<TurnOutputItem>,
        output_text: Option<String>,
        reasoning: Option<String>,
        usage: TurnUsage,
    ) -> Self {
        Self {
            id: id.into(),
            object: RESPONSE_OBJECT.to_string(),
            created_at,
            completed_at,
            model: model.into(),
            status: COMPLETED_STATUS.to_string(),
            output,
            output_text,
            reasoning,
            usage,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TurnResponseBuilder {
    id: String,
    model: String,
    created_at: u64,
    completed_at: u64,
    output: Vec<TurnOutputItem>,
    output_text_parts: Vec<String>,
    reasoning_parts: Vec<String>,
    usage: TurnUsage,
}

impl TurnResponseBuilder {
    pub fn new(
        id: impl Into<String>,
        model: impl Into<String>,
        created_at: u64,
        completed_at: u64,
    ) -> Self {
        Self {
            id: id.into(),
            model: model.into(),
            created_at,
            completed_at,
            output: Vec::new(),
            output_text_parts: Vec::new(),
            reasoning_parts: Vec::new(),
            usage: TurnUsage::default(),
        }
    }

    pub fn with_usage(mut self, usage: TurnUsage) -> Self {
        self.usage = usage;
        self
    }

    pub fn push_message_text(&mut self, text: impl AsRef<str>) {
        let trimmed = text.as_ref().trim();
        if trimmed.is_empty() {
            return;
        }
        let message = trimmed.to_string();
        self.output
            .push(TurnOutputItem::assistant_message(message.clone()));
        self.output_text_parts.push(message);
    }

    pub fn replace_output_with_message(&mut self, text: impl AsRef<str>) {
        self.output.clear();
        self.output_text_parts.clear();
        self.push_message_text(text);
    }

    pub fn push_function_call(
        &mut self,
        call_id: impl Into<String>,
        name: impl Into<String>,
        arguments: impl Into<String>,
    ) {
        self.output
            .push(TurnOutputItem::function_call(call_id, name, arguments));
    }

    pub fn push_reasoning(&mut self, reasoning: impl AsRef<str>) {
        let trimmed = reasoning.as_ref().trim();
        if trimmed.is_empty() {
            return;
        }
        self.reasoning_parts.push(trimmed.to_string());
    }

    pub fn build(self) -> TurnResponse {
        let output_text = if self.output_text_parts.is_empty() {
            None
        } else {
            Some(self.output_text_parts.join(""))
        };
        let reasoning = if self.reasoning_parts.is_empty() {
            None
        } else {
            Some(self.reasoning_parts.join("\n"))
        };
        TurnResponse::completed(
            self.id,
            self.model,
            self.created_at,
            self.completed_at,
            self.output,
            output_text,
            reasoning,
            self.usage,
        )
    }
}

#[derive(Debug, Deserialize)]
struct CodexExecEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    error: Option<CodexExecEventError>,
    #[serde(default)]
    item: Option<CodexExecEventItem>,
}

#[derive(Debug, Deserialize)]
struct CodexExecEventError {
    #[serde(default)]
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CodexExecEventItem {
    #[serde(rename = "type")]
    item_type: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    item: Option<Box<CodexExecEventItem>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexExecEventStreamSummary {
    pub final_text: Option<String>,
    pub final_error: Option<String>,
    pub saw_tool_activity: bool,
}

impl CodexExecEventItem {
    fn agent_message_text(&self) -> Option<&str> {
        if self.item_type == "agent_message" {
            return self.text.as_deref();
        }
        self.item
            .as_deref()
            .and_then(CodexExecEventItem::agent_message_text)
    }
}

fn value_contains_tool_activity(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            if object
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| TOOL_ACTIVITY_ITEM_TYPES.contains(&kind))
            {
                return true;
            }
            object.values().any(value_contains_tool_activity)
        }
        Value::Array(items) => items.iter().any(value_contains_tool_activity),
        _ => false,
    }
}

pub fn summarize_event_stream(stdout: &str) -> Option<CodexExecEventStreamSummary> {
    let mut saw_nonempty_line = false;
    let mut last_agent_message = None;
    let mut last_error = None;
    let mut saw_tool_activity = false;
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        saw_nonempty_line = true;
        let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
            return None;
        };
        if value_contains_tool_activity(&value) {
            saw_tool_activity = true;
        }
        let Ok(event) = serde_json::from_value::<CodexExecEvent>(value) else {
            return None;
        };
        if let Some(item) = event.item {
            if let Some(text) = item
                .agent_message_text()
                .map(str::trim)
                .filter(|text| !text.is_empty())
            {
                last_agent_message = Some(text.to_string());
            }
        }
        if event.event_type == "error" {
            if let Some(message) = event
                .message
                .as_deref()
                .map(str::trim)
                .filter(|message| !message.is_empty())
            {
                last_error = Some(message.to_string());
            }
            continue;
        }
        if event.event_type == "turn.failed" {
            if let Some(message) = event
                .error
                .as_ref()
                .and_then(|error| error.message.as_deref())
                .map(str::trim)
                .filter(|message| !message.is_empty())
            {
                last_error = Some(message.to_string());
            }
        }
    }
    saw_nonempty_line.then_some(CodexExecEventStreamSummary {
        final_text: last_agent_message,
        final_error: last_error,
        saw_tool_activity,
    })
}

pub fn extract_final_text_from_event_stream(stdout: &str) -> Option<String> {
    summarize_event_stream(stdout).and_then(|summary| summary.final_text)
}

pub fn extract_final_error_from_event_stream(stdout: &str) -> Option<String> {
    summarize_event_stream(stdout).and_then(|summary| summary.final_error)
}

#[cfg(test)]
mod tests {
    use super::extract_final_error_from_event_stream;
    use super::extract_final_text_from_event_stream;
    use super::summarize_event_stream;
    use super::TurnOutputItem;
    use super::TurnResponseBuilder;
    use super::TurnUsage;

    #[test]
    fn response_builder_collects_output_and_reasoning() {
        let response = TurnResponseBuilder::new("resp_1", "openai/gpt-oss-120b", 42, 84)
            .with_usage(TurnUsage {
                input_tokens: 11,
                output_tokens: 7,
                total_tokens: 18,
            });
        let mut response = response;
        response.push_reasoning("step one");
        response.push_message_text("CTOX_OK");
        response.push_function_call("call_weather", "weather.lookup", "{\"city\":\"Berlin\"}");
        let built = response.build();
        assert_eq!(built.output_text.as_deref(), Some("CTOX_OK"));
        assert_eq!(built.reasoning.as_deref(), Some("step one"));
        assert_eq!(built.usage.total_tokens, 18);
        assert!(matches!(built.output[0], TurnOutputItem::Message(_)));
        assert!(matches!(built.output[1], TurnOutputItem::FunctionCall(_)));
    }

    #[test]
    fn event_stream_text_extraction_prefers_last_agent_message() {
        let raw = concat!(
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"Ich prüfe Redis.\"}}\n",
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"command_execution\",\"command\":\"echo hi\",\"aggregated_output\":\"hi\",\"exit_code\":0,\"status\":\"completed\"}}\n",
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"Redis ist bereit.\"}}\n"
        );
        assert_eq!(
            extract_final_text_from_event_stream(raw).as_deref(),
            Some("Redis ist bereit.")
        );
    }

    #[test]
    fn event_stream_summary_detects_tool_activity() {
        let raw = concat!(
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"command_execution\",\"command\":\"cmake -S . -B build\",\"aggregated_output\":\"ok\",\"exit_code\":0,\"status\":\"completed\"}}\n",
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"CTOX_CPP_SMOKE_OK\"}}\n"
        );
        let summary = summarize_event_stream(raw).expect("event stream summary");
        assert!(summary.saw_tool_activity);
        assert_eq!(summary.final_text.as_deref(), Some("CTOX_CPP_SMOKE_OK"));
    }

    #[test]
    fn event_stream_summary_stays_false_without_tool_activity() {
        let raw = "{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"CTOX_CPP_SMOKE_OK\"}}\n";
        let summary = summarize_event_stream(raw).expect("event stream summary");
        assert!(!summary.saw_tool_activity);
        assert_eq!(summary.final_text.as_deref(), Some("CTOX_CPP_SMOKE_OK"));
    }

    #[test]
    fn event_stream_error_extraction_prefers_turn_failed_message() {
        let raw = concat!(
            "{\"type\":\"thread.started\",\"thread_id\":\"abc\"}\n",
            "{\"type\":\"error\",\"message\":\"Quota exceeded.\"}\n",
            "{\"type\":\"turn.failed\",\"error\":{\"message\":\"Quota exceeded.\"}}\n"
        );
        assert_eq!(
            extract_final_error_from_event_stream(raw).as_deref(),
            Some("Quota exceeded.")
        );
    }
}
