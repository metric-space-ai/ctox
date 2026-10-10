// Generated from src/core/rxdb/tests/fixtures/workjet-supervisor-source-v1.json. Do not edit.
#![allow(dead_code)]
use serde::{Deserialize, Serialize};

pub(crate) const CONTRACT_VERSION: u64 = 1;
pub(crate) const CONTRACT_SCHEMA: &str = "ctox.workjet.supervisor.source.v1";

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
impl WireValidate for f64 {
    fn validate(&self) -> Result<(), String> {
        if !self.is_finite() {
            return Err("non-finite number".into());
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum SourceAction {
    #[serde(rename = "poll")]
    Poll,
    #[serde(rename = "claim")]
    Claim,
    #[serde(rename = "status")]
    Status,
    #[serde(rename = "cancel")]
    Cancel,
    #[serde(rename = "model_invoke")]
    ModelInvoke,
    #[serde(rename = "model_read")]
    ModelRead,
    #[serde(rename = "tool_call")]
    ToolCall,
    #[serde(rename = "sdk_observe")]
    SdkObserve,
}
impl WireValidate for SourceAction {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum SourceOfferState {
    #[serde(rename = "offered")]
    Offered,
    #[serde(rename = "claimed")]
    Claimed,
    #[serde(rename = "closed")]
    Closed,
}
impl WireValidate for SourceOfferState {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceOperation {
    pub(crate) version: u64,
    pub(crate) action: SourceAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) offer_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) controller_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) operation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) model_operation: Option<SourceModelOperation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) body_json: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) sdk_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) sequence: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) native_tool: Option<SourceNativeTool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tool_arguments_json: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) sdk_observation: Option<SourceSdkObservation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) include_confirmed_goal_read: Option<bool>,
}
impl WireValidate for SourceOperation {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.version;
            value.validate()?;
            if *value < 1 {
                return Err("SourceOperation.version violates minimum".into());
            }
            if *value > 1 {
                return Err("SourceOperation.version violates maximum".into());
            }
        }
        {
            let value = &self.action;
            value.validate()?;
        }
        if let Some(value) = &self.offer_id {
            value.validate()?;
            if value.chars().count() < 36 {
                return Err("SourceOperation.offer_id violates min_chars".into());
            }
            if value.chars().count() > 36 {
                return Err("SourceOperation.offer_id violates max_chars".into());
            }
        }
        if let Some(value) = &self.controller_id {
            value.validate()?;
            if value.chars().count() < 36 {
                return Err("SourceOperation.controller_id violates min_chars".into());
            }
            if value.chars().count() > 36 {
                return Err("SourceOperation.controller_id violates max_chars".into());
            }
        }
        if let Some(value) = &self.operation_id {
            value.validate()?;
            if value.chars().count() < 36 {
                return Err("SourceOperation.operation_id violates min_chars".into());
            }
            if value.chars().count() > 36 {
                return Err("SourceOperation.operation_id violates max_chars".into());
            }
        }
        if let Some(value) = &self.model_operation {
            value.validate()?;
        }
        if let Some(value) = &self.body_json {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceOperation.body_json violates min_chars".into());
            }
            if value.chars().count() > 98304 {
                return Err("SourceOperation.body_json violates max_chars".into());
            }
        }
        if let Some(value) = &self.sdk_session_id {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceOperation.sdk_session_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("SourceOperation.sdk_session_id violates max_chars".into());
            }
        }
        if let Some(value) = &self.sequence {
            value.validate()?;
            if *value > 65535 {
                return Err("SourceOperation.sequence violates maximum".into());
            }
        }
        if let Some(value) = &self.native_tool {
            value.validate()?;
        }
        if let Some(value) = &self.tool_arguments_json {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceOperation.tool_arguments_json violates min_chars".into());
            }
            if value.chars().count() > 65536 {
                return Err("SourceOperation.tool_arguments_json violates max_chars".into());
            }
        }
        if let Some(value) = &self.sdk_observation {
            value.validate()?;
        }
        if let Some(value) = &self.include_confirmed_goal_read {
            value.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceRequestedRoute {
    pub(crate) project_id: String,
    pub(crate) supervisor_thread_id: String,
    pub(crate) luma_id: String,
    pub(crate) configuration_revision: u64,
    pub(crate) computer_id: String,
    pub(crate) harness: String,
    pub(crate) model: String,
}
impl WireValidate for SourceRequestedRoute {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.project_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceRequestedRoute.project_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("SourceRequestedRoute.project_id violates max_chars".into());
            }
        }
        {
            let value = &self.supervisor_thread_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceRequestedRoute.supervisor_thread_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("SourceRequestedRoute.supervisor_thread_id violates max_chars".into());
            }
        }
        {
            let value = &self.luma_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceRequestedRoute.luma_id violates min_chars".into());
            }
            if value.chars().count() > 160 {
                return Err("SourceRequestedRoute.luma_id violates max_chars".into());
            }
        }
        {
            let value = &self.configuration_revision;
            value.validate()?;
            if *value < 1 {
                return Err("SourceRequestedRoute.configuration_revision violates minimum".into());
            }
        }
        {
            let value = &self.computer_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceRequestedRoute.computer_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("SourceRequestedRoute.computer_id violates max_chars".into());
            }
        }
        {
            let value = &self.harness;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceRequestedRoute.harness violates min_chars".into());
            }
            if value.chars().count() > 32 {
                return Err("SourceRequestedRoute.harness violates max_chars".into());
            }
        }
        {
            let value = &self.model;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceRequestedRoute.model violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("SourceRequestedRoute.model violates max_chars".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceOffer {
    pub(crate) offer_id: String,
    pub(crate) execution_key: String,
    pub(crate) route: SourceRequestedRoute,
    pub(crate) deadline_ms: i64,
    pub(crate) state: SourceOfferState,
}
impl WireValidate for SourceOffer {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.offer_id;
            value.validate()?;
            if value.chars().count() < 36 {
                return Err("SourceOffer.offer_id violates min_chars".into());
            }
            if value.chars().count() > 36 {
                return Err("SourceOffer.offer_id violates max_chars".into());
            }
        }
        {
            let value = &self.execution_key;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceOffer.execution_key violates min_chars".into());
            }
            if value.chars().count() > 512 {
                return Err("SourceOffer.execution_key violates max_chars".into());
            }
        }
        {
            let value = &self.route;
            value.validate()?;
        }
        {
            let value = &self.deadline_ms;
            value.validate()?;
            if *value < 1 {
                return Err("SourceOffer.deadline_ms violates minimum".into());
            }
        }
        {
            let value = &self.state;
            value.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum SourceModelOperation {
    #[serde(rename = "messages")]
    Messages,
    #[serde(rename = "count_tokens")]
    CountTokens,
}
impl WireValidate for SourceModelOperation {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum SourceNativeTool {
    #[serde(rename = "worker_dispatch")]
    WorkerDispatch,
    #[serde(rename = "confirmed_goal_read")]
    ConfirmedGoalRead,
}
impl WireValidate for SourceNativeTool {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum SourceSdkObservationKind {
    #[serde(rename = "child-spawned")]
    ChildSpawned,
    #[serde(rename = "child-closed")]
    ChildClosed,
    #[serde(rename = "sdk-init")]
    SdkInit,
    #[serde(rename = "turn-submitted")]
    TurnSubmitted,
    #[serde(rename = "parent-assistant")]
    ParentAssistant,
    #[serde(rename = "sdk-result")]
    SdkResult,
    #[serde(rename = "sdk-stream-joined")]
    SdkStreamJoined,
    #[serde(rename = "sdk-query-close-returned")]
    SdkQueryCloseReturned,
}
impl WireValidate for SourceSdkObservationKind {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceSdkObservation {
    pub(crate) version: u64,
    pub(crate) sequence: u64,
    pub(crate) kind: SourceSdkObservationKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) init_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) message_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) assistant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) result_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) subtype: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) signal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pid: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) exit_code: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) is_error: Option<bool>,
}
impl WireValidate for SourceSdkObservation {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.version;
            value.validate()?;
            if *value < 1 {
                return Err("SourceSdkObservation.version violates minimum".into());
            }
            if *value > 1 {
                return Err("SourceSdkObservation.version violates maximum".into());
            }
        }
        {
            let value = &self.sequence;
            value.validate()?;
            if *value > 511 {
                return Err("SourceSdkObservation.sequence violates maximum".into());
            }
        }
        {
            let value = &self.kind;
            value.validate()?;
        }
        if let Some(value) = &self.session_id {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceSdkObservation.session_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("SourceSdkObservation.session_id violates max_chars".into());
            }
        }
        if let Some(value) = &self.init_id {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceSdkObservation.init_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("SourceSdkObservation.init_id violates max_chars".into());
            }
        }
        if let Some(value) = &self.turn_id {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceSdkObservation.turn_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("SourceSdkObservation.turn_id violates max_chars".into());
            }
        }
        if let Some(value) = &self.message_id {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceSdkObservation.message_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("SourceSdkObservation.message_id violates max_chars".into());
            }
        }
        if let Some(value) = &self.message_model {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceSdkObservation.message_model violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("SourceSdkObservation.message_model violates max_chars".into());
            }
        }
        if let Some(value) = &self.assistant_id {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceSdkObservation.assistant_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("SourceSdkObservation.assistant_id violates max_chars".into());
            }
        }
        if let Some(value) = &self.result_id {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceSdkObservation.result_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("SourceSdkObservation.result_id violates max_chars".into());
            }
        }
        if let Some(value) = &self.subtype {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceSdkObservation.subtype violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("SourceSdkObservation.subtype violates max_chars".into());
            }
        }
        if let Some(value) = &self.signal {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceSdkObservation.signal violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("SourceSdkObservation.signal violates max_chars".into());
            }
        }
        if let Some(value) = &self.pid {
            value.validate()?;
            if *value < 1 {
                return Err("SourceSdkObservation.pid violates minimum".into());
            }
            if *value > 4294967295 {
                return Err("SourceSdkObservation.pid violates maximum".into());
            }
        }
        if let Some(value) = &self.exit_code {
            value.validate()?;
            if *value < -2147483648 {
                return Err("SourceSdkObservation.exit_code violates minimum".into());
            }
            if *value > 2147483647 {
                return Err("SourceSdkObservation.exit_code violates maximum".into());
            }
        }
        if let Some(value) = &self.is_error {
            value.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn validate_fixture(kind: &str, value: serde_json::Value) -> Result<(), String> {
    match kind {
        "SourceAction" => serde_json::from_value::<SourceAction>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "SourceOfferState" => serde_json::from_value::<SourceOfferState>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "SourceOperation" => serde_json::from_value::<SourceOperation>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "SourceRequestedRoute" => serde_json::from_value::<SourceRequestedRoute>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "SourceOffer" => serde_json::from_value::<SourceOffer>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "SourceModelOperation" => serde_json::from_value::<SourceModelOperation>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "SourceNativeTool" => serde_json::from_value::<SourceNativeTool>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "SourceSdkObservationKind" => serde_json::from_value::<SourceSdkObservationKind>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "SourceSdkObservation" => serde_json::from_value::<SourceSdkObservation>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        _ => Err("unknown contract type".into()),
    }
}
