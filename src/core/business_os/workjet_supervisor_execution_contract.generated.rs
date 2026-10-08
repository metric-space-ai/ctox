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
        "ExecutionEvent" => serde_json::from_value::<ExecutionEvent>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "ExecutionPage" => serde_json::from_value::<ExecutionPage>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        _ => Err("unknown observer contract type".into()),
    }
}
