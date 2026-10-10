// Generated from src/core/rxdb/tests/fixtures/workjet-supervisor-execution-v1.json. Do not edit.
#![allow(dead_code)]
use serde::{Deserialize, Serialize};
pub(crate) const CONTRACT_SCHEMA: &str = "ctox.workjet.supervisor_execution.v1";
pub(crate) const CONTRACT_VERSION: u64 = 1;
pub(crate) trait WireValidate {
    fn validate(&self) -> Result<(), String>;
}
impl WireValidate for String {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}
impl WireValidate for bool {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}
impl WireValidate for u64 {
    fn validate(&self) -> Result<(), String> {
        if *self > 9_007_199_254_740_991 {
            return Err("unsafe JSON integer".into());
        }
        Ok(())
    }
}
impl WireValidate for i64 {
    fn validate(&self) -> Result<(), String> {
        if self.unsigned_abs() > 9_007_199_254_740_991 {
            return Err("unsafe JSON integer".into());
        }
        Ok(())
    }
}
impl<T: WireValidate> WireValidate for Vec<T> {
    fn validate(&self) -> Result<(), String> {
        for item in self {
            item.validate()?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EventCursor {
    pub(crate) after_sequence: u64,
    pub(crate) after_event_id: String,
}
impl WireValidate for EventCursor {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.after_sequence;
            value.validate()?;
            if *value < 1 {
                return Err("EventCursor.after_sequence violates minimum".into());
            }
        }
        {
            let value = &self.after_event_id;
            value.validate()?;
            if value.trim().is_empty() {
                return Err("EventCursor.after_event_id is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("EventCursor.after_event_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("EventCursor.after_event_id violates max_chars".into());
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExecutionPageRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) attempt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cursor: Option<EventCursor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) limit: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) include_public_text: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) include_native_message_text: Option<bool>,
}
impl WireValidate for ExecutionPageRequest {
    fn validate(&self) -> Result<(), String> {
        if let Some(value) = &self.attempt_id {
            value.validate()?;
            if value.trim().is_empty() {
                return Err("ExecutionPageRequest.attempt_id is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("ExecutionPageRequest.attempt_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("ExecutionPageRequest.attempt_id violates max_chars".into());
            }
        }
        if let Some(value) = &self.cursor {
            value.validate()?;
        }
        if let Some(value) = &self.limit {
            value.validate()?;
            if *value < 1 {
                return Err("ExecutionPageRequest.limit violates minimum".into());
            }
            if *value > 50 {
                return Err("ExecutionPageRequest.limit violates maximum".into());
            }
        }
        if let Some(value) = &self.include_public_text {
            value.validate()?;
        }
        if let Some(value) = &self.include_native_message_text {
            value.validate()?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AttemptRef {
    pub(crate) attempt_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) attempt_index: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) started_at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) finished_at_ms: Option<i64>,
}
impl WireValidate for AttemptRef {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.attempt_id;
            value.validate()?;
            if value.trim().is_empty() {
                return Err("AttemptRef.attempt_id is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("AttemptRef.attempt_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("AttemptRef.attempt_id violates max_chars".into());
            }
        }
        if let Some(value) = &self.run_id {
            value.validate()?;
            if value.trim().is_empty() {
                return Err("AttemptRef.run_id is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("AttemptRef.run_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("AttemptRef.run_id violates max_chars".into());
            }
        }
        if let Some(value) = &self.attempt_index {
            value.validate()?;
        }
        if let Some(value) = &self.status {
            value.validate()?;
            if value.trim().is_empty() {
                return Err("AttemptRef.status is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("AttemptRef.status violates min_chars".into());
            }
            if value.chars().count() > 64 {
                return Err("AttemptRef.status violates max_chars".into());
            }
        }
        if let Some(value) = &self.started_at_ms {
            value.validate()?;
            if *value < 0 {
                return Err("AttemptRef.started_at_ms violates minimum".into());
            }
        }
        if let Some(value) = &self.finished_at_ms {
            value.validate()?;
            if *value < 0 {
                return Err("AttemptRef.finished_at_ms violates minimum".into());
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PublicAssistantText {
    pub(crate) turn_id: String,
    pub(crate) item_id: String,
    pub(crate) phase: String,
    pub(crate) offset: u64,
    pub(crate) text: String,
    pub(crate) completed: bool,
    pub(crate) truncated: bool,
}
impl WireValidate for PublicAssistantText {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.turn_id;
            value.validate()?;
            if value.trim().is_empty() {
                return Err("PublicAssistantText.turn_id is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("PublicAssistantText.turn_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("PublicAssistantText.turn_id violates max_chars".into());
            }
        }
        {
            let value = &self.item_id;
            value.validate()?;
            if value.trim().is_empty() {
                return Err("PublicAssistantText.item_id is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("PublicAssistantText.item_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("PublicAssistantText.item_id violates max_chars".into());
            }
        }
        {
            let value = &self.phase;
            value.validate()?;
            if value.trim().is_empty() {
                return Err("PublicAssistantText.phase is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("PublicAssistantText.phase violates min_chars".into());
            }
            if value.chars().count() > 32 {
                return Err("PublicAssistantText.phase violates max_chars".into());
            }
        }
        {
            let value = &self.offset;
            value.validate()?;
            if *value > 65536 {
                return Err("PublicAssistantText.offset violates maximum".into());
            }
        }
        {
            let value = &self.text;
            value.validate()?;
            if value.chars().count() > 4096 {
                return Err("PublicAssistantText.text violates max_chars".into());
            }
        }
        {
            let value = &self.completed;
            value.validate()?;
        }
        {
            let value = &self.truncated;
            value.validate()?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExecutionEvent {
    pub(crate) id: String,
    pub(crate) sequence: u64,
    pub(crate) kind: String,
    pub(crate) title: String,
    pub(crate) created_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) success: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) public_text: Option<PublicAssistantText>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) native_message_text: Option<NativeMessageText>,
}
impl WireValidate for ExecutionEvent {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.id;
            value.validate()?;
            if value.trim().is_empty() {
                return Err("ExecutionEvent.id is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("ExecutionEvent.id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("ExecutionEvent.id violates max_chars".into());
            }
        }
        {
            let value = &self.sequence;
            value.validate()?;
            if *value < 1 {
                return Err("ExecutionEvent.sequence violates minimum".into());
            }
        }
        {
            let value = &self.kind;
            value.validate()?;
            if value.trim().is_empty() {
                return Err("ExecutionEvent.kind is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("ExecutionEvent.kind violates min_chars".into());
            }
            if value.chars().count() > 64 {
                return Err("ExecutionEvent.kind violates max_chars".into());
            }
        }
        {
            let value = &self.title;
            value.validate()?;
            if value.chars().count() > 256 {
                return Err("ExecutionEvent.title violates max_chars".into());
            }
        }
        {
            let value = &self.created_at_ms;
            value.validate()?;
            if *value < 0 {
                return Err("ExecutionEvent.created_at_ms violates minimum".into());
            }
        }
        if let Some(value) = &self.tool_name {
            value.validate()?;
            if value.trim().is_empty() {
                return Err("ExecutionEvent.tool_name is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("ExecutionEvent.tool_name violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("ExecutionEvent.tool_name violates max_chars".into());
            }
        }
        if let Some(value) = &self.call_id {
            value.validate()?;
            if value.trim().is_empty() {
                return Err("ExecutionEvent.call_id is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("ExecutionEvent.call_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("ExecutionEvent.call_id violates max_chars".into());
            }
        }
        if let Some(value) = &self.success {
            value.validate()?;
        }
        if let Some(value) = &self.public_text {
            value.validate()?;
        }
        if let Some(value) = &self.native_message_text {
            value.validate()?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExecutionPage {
    pub(crate) command_id: String,
    pub(crate) task_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) attempt: Option<AttemptRef>,
    pub(crate) events: Vec<ExecutionEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) next_cursor: Option<EventCursor>,
    pub(crate) has_more: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) public_text_supported: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) native_message_text_supported: Option<bool>,
}
impl WireValidate for ExecutionPage {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.command_id;
            value.validate()?;
            if value.trim().is_empty() {
                return Err("ExecutionPage.command_id is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("ExecutionPage.command_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("ExecutionPage.command_id violates max_chars".into());
            }
        }
        {
            let value = &self.task_id;
            value.validate()?;
            if value.trim().is_empty() {
                return Err("ExecutionPage.task_id is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("ExecutionPage.task_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("ExecutionPage.task_id violates max_chars".into());
            }
        }
        if let Some(value) = &self.attempt {
            value.validate()?;
        }
        {
            let value = &self.events;
            value.validate()?;
            if value.len() > 50 {
                return Err("ExecutionPage.events violates max_items".into());
            }
        }
        if let Some(value) = &self.next_cursor {
            value.validate()?;
        }
        {
            let value = &self.has_more;
            value.validate()?;
        }
        if let Some(value) = &self.public_text_supported {
            value.validate()?;
        }
        if let Some(value) = &self.native_message_text_supported {
            value.validate()?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TurnHistoryCursor {
    pub(crate) before_created_at_ms: i64,
    pub(crate) before_command_id: String,
}
impl WireValidate for TurnHistoryCursor {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.before_created_at_ms;
            value.validate()?;
            if *value < 0 {
                return Err("TurnHistoryCursor.before_created_at_ms violates minimum".into());
            }
        }
        {
            let value = &self.before_command_id;
            value.validate()?;
            if value.trim().is_empty() {
                return Err("TurnHistoryCursor.before_command_id is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("TurnHistoryCursor.before_command_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("TurnHistoryCursor.before_command_id violates max_chars".into());
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TurnHistoryRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cursor: Option<TurnHistoryCursor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) limit: Option<u64>,
}
impl WireValidate for TurnHistoryRequest {
    fn validate(&self) -> Result<(), String> {
        if let Some(value) = &self.cursor {
            value.validate()?;
        }
        if let Some(value) = &self.limit {
            value.validate()?;
            if *value < 1 {
                return Err("TurnHistoryRequest.limit violates minimum".into());
            }
            if *value > 20 {
                return Err("TurnHistoryRequest.limit violates maximum".into());
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TurnHistoryEntry {
    pub(crate) command_id: String,
    pub(crate) task_id: String,
    pub(crate) created_at_ms: i64,
    pub(crate) user_text: String,
    pub(crate) user_text_truncated: bool,
}
impl WireValidate for TurnHistoryEntry {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.command_id;
            value.validate()?;
            if value.trim().is_empty() {
                return Err("TurnHistoryEntry.command_id is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("TurnHistoryEntry.command_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("TurnHistoryEntry.command_id violates max_chars".into());
            }
        }
        {
            let value = &self.task_id;
            value.validate()?;
            if value.trim().is_empty() {
                return Err("TurnHistoryEntry.task_id is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("TurnHistoryEntry.task_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("TurnHistoryEntry.task_id violates max_chars".into());
            }
        }
        {
            let value = &self.created_at_ms;
            value.validate()?;
            if *value < 0 {
                return Err("TurnHistoryEntry.created_at_ms violates minimum".into());
            }
        }
        {
            let value = &self.user_text;
            value.validate()?;
            if value.chars().count() > 4096 {
                return Err("TurnHistoryEntry.user_text violates max_chars".into());
            }
        }
        {
            let value = &self.user_text_truncated;
            value.validate()?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TurnHistoryPage {
    pub(crate) project_id: String,
    pub(crate) thread_id: String,
    pub(crate) thread_key: String,
    pub(crate) turns: Vec<TurnHistoryEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) next_cursor: Option<TurnHistoryCursor>,
    pub(crate) has_more: bool,
}
impl WireValidate for TurnHistoryPage {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.project_id;
            value.validate()?;
            if value.trim().is_empty() {
                return Err("TurnHistoryPage.project_id is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("TurnHistoryPage.project_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("TurnHistoryPage.project_id violates max_chars".into());
            }
        }
        {
            let value = &self.thread_id;
            value.validate()?;
            if value.trim().is_empty() {
                return Err("TurnHistoryPage.thread_id is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("TurnHistoryPage.thread_id violates min_chars".into());
            }
            if value.chars().count() > 36 {
                return Err("TurnHistoryPage.thread_id violates max_chars".into());
            }
        }
        {
            let value = &self.thread_key;
            value.validate()?;
            if value.trim().is_empty() {
                return Err("TurnHistoryPage.thread_key is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("TurnHistoryPage.thread_key violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("TurnHistoryPage.thread_key violates max_chars".into());
            }
        }
        {
            let value = &self.turns;
            value.validate()?;
            if value.len() > 20 {
                return Err("TurnHistoryPage.turns violates max_items".into());
            }
        }
        if let Some(value) = &self.next_cursor {
            value.validate()?;
        }
        {
            let value = &self.has_more;
            value.validate()?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NativeMessageText {
    pub(crate) execution_key: String,
    pub(crate) model_operation_id: String,
    pub(crate) native_message_id: String,
    pub(crate) model: String,
    pub(crate) upstream_request_id: String,
    pub(crate) offset: u64,
    pub(crate) text: String,
    pub(crate) completed: bool,
}
impl WireValidate for NativeMessageText {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.execution_key;
            value.validate()?;
            if value.trim().is_empty() {
                return Err("NativeMessageText.execution_key is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("NativeMessageText.execution_key violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("NativeMessageText.execution_key violates max_chars".into());
            }
        }
        {
            let value = &self.model_operation_id;
            value.validate()?;
            if value.trim().is_empty() {
                return Err("NativeMessageText.model_operation_id is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("NativeMessageText.model_operation_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("NativeMessageText.model_operation_id violates max_chars".into());
            }
        }
        {
            let value = &self.native_message_id;
            value.validate()?;
            if value.trim().is_empty() {
                return Err("NativeMessageText.native_message_id is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("NativeMessageText.native_message_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("NativeMessageText.native_message_id violates max_chars".into());
            }
        }
        {
            let value = &self.model;
            value.validate()?;
            if value.trim().is_empty() {
                return Err("NativeMessageText.model is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("NativeMessageText.model violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("NativeMessageText.model violates max_chars".into());
            }
        }
        {
            let value = &self.upstream_request_id;
            value.validate()?;
            if value.trim().is_empty() {
                return Err("NativeMessageText.upstream_request_id is blank".into());
            }
            if value.chars().count() < 1 {
                return Err("NativeMessageText.upstream_request_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("NativeMessageText.upstream_request_id violates max_chars".into());
            }
        }
        {
            let value = &self.offset;
            value.validate()?;
            if *value > 65536 {
                return Err("NativeMessageText.offset violates maximum".into());
            }
        }
        {
            let value = &self.text;
            value.validate()?;
            if value.chars().count() > 4096 {
                return Err("NativeMessageText.text violates max_chars".into());
            }
        }
        {
            let value = &self.completed;
            value.validate()?;
        }
        Ok(())
    }
}
#[cfg(test)]
pub(crate) fn validate_fixture(kind: &str, value: serde_json::Value) -> Result<(), String> {
    match kind {
        "EventCursor" => serde_json::from_value::<EventCursor>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "ExecutionPageRequest" => serde_json::from_value::<ExecutionPageRequest>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "AttemptRef" => serde_json::from_value::<AttemptRef>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "PublicAssistantText" => serde_json::from_value::<PublicAssistantText>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "ExecutionEvent" => serde_json::from_value::<ExecutionEvent>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "ExecutionPage" => serde_json::from_value::<ExecutionPage>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "TurnHistoryCursor" => serde_json::from_value::<TurnHistoryCursor>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "TurnHistoryRequest" => serde_json::from_value::<TurnHistoryRequest>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "TurnHistoryEntry" => serde_json::from_value::<TurnHistoryEntry>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "TurnHistoryPage" => serde_json::from_value::<TurnHistoryPage>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "NativeMessageText" => serde_json::from_value::<NativeMessageText>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        _ => Err("unknown observer contract type".into()),
    }
}
