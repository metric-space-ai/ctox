// Generated from src/core/rxdb/tests/fixtures/workjet-supervisor-route-computation-v2.json. Do not edit.
#![allow(dead_code)]
use serde::{Deserialize, Serialize};

pub(crate) const CONTRACT_VERSION: u64 = 2;
pub(crate) const CONTRACT_SCHEMA: &str = "ctox.workjet.supervisor.route-display.v2";

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
pub(crate) enum RouteCapabilitiesSchema {
    #[serde(rename = "ctox.workjet.supervisor.route-capabilities.v2")]
    CtoxWorkjetSupervisorRouteCapabilitiesV2,
}
impl WireValidate for RouteCapabilitiesSchema {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum RouteReadCommand {
    #[serde(rename = "ctox.workjet.project.supervisor.route.read.v2")]
    CtoxWorkjetProjectSupervisorRouteReadV2,
}
impl WireValidate for RouteReadCommand {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum RouteDisplaySchema {
    #[serde(rename = "ctox.workjet.supervisor.route-display.v2")]
    CtoxWorkjetSupervisorRouteDisplayV2,
}
impl WireValidate for RouteDisplaySchema {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SupervisorRouteCapabilities {
    pub(crate) schema: RouteCapabilitiesSchema,
    pub(crate) project_id: String,
    pub(crate) supervisor_thread_id: String,
    pub(crate) read_schema: RouteDisplaySchema,
    pub(crate) read_command: RouteReadCommand,
}
impl WireValidate for SupervisorRouteCapabilities {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.schema;
            value.validate()?;
        }
        {
            let value = &self.project_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SupervisorRouteCapabilities.project_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("SupervisorRouteCapabilities.project_id violates max_chars".into());
            }
        }
        {
            let value = &self.supervisor_thread_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err(
                    "SupervisorRouteCapabilities.supervisor_thread_id violates min_chars".into(),
                );
            }
            if value.chars().count() > 256 {
                return Err(
                    "SupervisorRouteCapabilities.supervisor_thread_id violates max_chars".into(),
                );
            }
        }
        {
            let value = &self.read_schema;
            value.validate()?;
        }
        {
            let value = &self.read_command;
            value.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConfiguredSupervisorRoute {
    pub(crate) luma_id: String,
    pub(crate) configuration_revision: u64,
    pub(crate) computer_id: String,
    pub(crate) harness: String,
    pub(crate) route_id: String,
    pub(crate) model: String,
    pub(crate) catalog_checked_at_ms: i64,
}
impl WireValidate for ConfiguredSupervisorRoute {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.luma_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ConfiguredSupervisorRoute.luma_id violates min_chars".into());
            }
            if value.chars().count() > 160 {
                return Err("ConfiguredSupervisorRoute.luma_id violates max_chars".into());
            }
        }
        {
            let value = &self.configuration_revision;
            value.validate()?;
            if *value < 1 {
                return Err(
                    "ConfiguredSupervisorRoute.configuration_revision violates minimum".into(),
                );
            }
        }
        {
            let value = &self.computer_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ConfiguredSupervisorRoute.computer_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("ConfiguredSupervisorRoute.computer_id violates max_chars".into());
            }
        }
        {
            let value = &self.harness;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ConfiguredSupervisorRoute.harness violates min_chars".into());
            }
            if value.chars().count() > 32 {
                return Err("ConfiguredSupervisorRoute.harness violates max_chars".into());
            }
        }
        {
            let value = &self.route_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ConfiguredSupervisorRoute.route_id violates min_chars".into());
            }
            if value.chars().count() > 160 {
                return Err("ConfiguredSupervisorRoute.route_id violates max_chars".into());
            }
        }
        {
            let value = &self.model;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ConfiguredSupervisorRoute.model violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("ConfiguredSupervisorRoute.model violates max_chars".into());
            }
        }
        {
            let value = &self.catalog_checked_at_ms;
            value.validate()?;
            if *value < 0 {
                return Err(
                    "ConfiguredSupervisorRoute.catalog_checked_at_ms violates minimum".into(),
                );
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SelectedSupervisorRoute {
    pub(crate) luma_id: String,
    pub(crate) configuration_revision: u64,
    pub(crate) route_id: String,
}
impl WireValidate for SelectedSupervisorRoute {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.luma_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SelectedSupervisorRoute.luma_id violates min_chars".into());
            }
            if value.chars().count() > 160 {
                return Err("SelectedSupervisorRoute.luma_id violates max_chars".into());
            }
        }
        {
            let value = &self.configuration_revision;
            value.validate()?;
            if *value < 1 {
                return Err(
                    "SelectedSupervisorRoute.configuration_revision violates minimum".into(),
                );
            }
        }
        {
            let value = &self.route_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SelectedSupervisorRoute.route_id violates min_chars".into());
            }
            if value.chars().count() > 160 {
                return Err("SelectedSupervisorRoute.route_id violates max_chars".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ActualSupervisorComputation {
    pub(crate) receipt_id: String,
    pub(crate) execution_key: String,
    pub(crate) selected_route: SelectedSupervisorRoute,
    pub(crate) harness: String,
    pub(crate) computer_id: String,
    pub(crate) account_id: String,
    pub(crate) model: String,
    pub(crate) model_operation_id: String,
    pub(crate) native_message_id: String,
    pub(crate) upstream_request_id: String,
    pub(crate) sdk_session_id: String,
    pub(crate) sdk_turn_id: String,
    pub(crate) sdk_assistant_id: String,
    pub(crate) sdk_result_id: String,
    pub(crate) model_finished_at_ms: i64,
    pub(crate) published_at_ms: i64,
}
impl WireValidate for ActualSupervisorComputation {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.receipt_id;
            value.validate()?;
            if value.chars().count() < 64 {
                return Err("ActualSupervisorComputation.receipt_id violates min_chars".into());
            }
            if value.chars().count() > 64 {
                return Err("ActualSupervisorComputation.receipt_id violates max_chars".into());
            }
        }
        {
            let value = &self.execution_key;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ActualSupervisorComputation.execution_key violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("ActualSupervisorComputation.execution_key violates max_chars".into());
            }
        }
        {
            let value = &self.selected_route;
            value.validate()?;
        }
        {
            let value = &self.harness;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ActualSupervisorComputation.harness violates min_chars".into());
            }
            if value.chars().count() > 32 {
                return Err("ActualSupervisorComputation.harness violates max_chars".into());
            }
        }
        {
            let value = &self.computer_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ActualSupervisorComputation.computer_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("ActualSupervisorComputation.computer_id violates max_chars".into());
            }
        }
        {
            let value = &self.account_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ActualSupervisorComputation.account_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("ActualSupervisorComputation.account_id violates max_chars".into());
            }
        }
        {
            let value = &self.model;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ActualSupervisorComputation.model violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("ActualSupervisorComputation.model violates max_chars".into());
            }
        }
        {
            let value = &self.model_operation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err(
                    "ActualSupervisorComputation.model_operation_id violates min_chars".into(),
                );
            }
            if value.chars().count() > 256 {
                return Err(
                    "ActualSupervisorComputation.model_operation_id violates max_chars".into(),
                );
            }
        }
        {
            let value = &self.native_message_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err(
                    "ActualSupervisorComputation.native_message_id violates min_chars".into(),
                );
            }
            if value.chars().count() > 256 {
                return Err(
                    "ActualSupervisorComputation.native_message_id violates max_chars".into(),
                );
            }
        }
        {
            let value = &self.upstream_request_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err(
                    "ActualSupervisorComputation.upstream_request_id violates min_chars".into(),
                );
            }
            if value.chars().count() > 256 {
                return Err(
                    "ActualSupervisorComputation.upstream_request_id violates max_chars".into(),
                );
            }
        }
        {
            let value = &self.sdk_session_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ActualSupervisorComputation.sdk_session_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("ActualSupervisorComputation.sdk_session_id violates max_chars".into());
            }
        }
        {
            let value = &self.sdk_turn_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ActualSupervisorComputation.sdk_turn_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("ActualSupervisorComputation.sdk_turn_id violates max_chars".into());
            }
        }
        {
            let value = &self.sdk_assistant_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err(
                    "ActualSupervisorComputation.sdk_assistant_id violates min_chars".into(),
                );
            }
            if value.chars().count() > 256 {
                return Err(
                    "ActualSupervisorComputation.sdk_assistant_id violates max_chars".into(),
                );
            }
        }
        {
            let value = &self.sdk_result_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ActualSupervisorComputation.sdk_result_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("ActualSupervisorComputation.sdk_result_id violates max_chars".into());
            }
        }
        {
            let value = &self.model_finished_at_ms;
            value.validate()?;
            if *value < 0 {
                return Err(
                    "ActualSupervisorComputation.model_finished_at_ms violates minimum".into(),
                );
            }
        }
        {
            let value = &self.published_at_ms;
            value.validate()?;
            if *value < 0 {
                return Err("ActualSupervisorComputation.published_at_ms violates minimum".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SupervisorRouteDisplay {
    pub(crate) schema: RouteDisplaySchema,
    pub(crate) project_id: String,
    pub(crate) supervisor_thread_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) configured: Option<ConfiguredSupervisorRoute>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) actual: Option<ActualSupervisorComputation>,
}
impl WireValidate for SupervisorRouteDisplay {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.schema;
            value.validate()?;
        }
        {
            let value = &self.project_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SupervisorRouteDisplay.project_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("SupervisorRouteDisplay.project_id violates max_chars".into());
            }
        }
        {
            let value = &self.supervisor_thread_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err(
                    "SupervisorRouteDisplay.supervisor_thread_id violates min_chars".into(),
                );
            }
            if value.chars().count() > 36 {
                return Err(
                    "SupervisorRouteDisplay.supervisor_thread_id violates max_chars".into(),
                );
            }
        }
        if let Some(value) = &self.configured {
            value.validate()?;
        }
        if let Some(value) = &self.actual {
            value.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn validate_fixture(kind: &str, value: serde_json::Value) -> Result<(), String> {
    match kind {
        "RouteCapabilitiesSchema" => serde_json::from_value::<RouteCapabilitiesSchema>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "RouteReadCommand" => serde_json::from_value::<RouteReadCommand>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "RouteDisplaySchema" => serde_json::from_value::<RouteDisplaySchema>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "SupervisorRouteCapabilities" => {
            serde_json::from_value::<SupervisorRouteCapabilities>(value)
                .map_err(|e| e.to_string())?
                .validate()
        }
        "ConfiguredSupervisorRoute" => serde_json::from_value::<ConfiguredSupervisorRoute>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "SelectedSupervisorRoute" => serde_json::from_value::<SelectedSupervisorRoute>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "ActualSupervisorComputation" => {
            serde_json::from_value::<ActualSupervisorComputation>(value)
                .map_err(|e| e.to_string())?
                .validate()
        }
        "SupervisorRouteDisplay" => serde_json::from_value::<SupervisorRouteDisplay>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        _ => Err("unknown contract type".into()),
    }
}
