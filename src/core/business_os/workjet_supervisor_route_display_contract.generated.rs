// Generated from src/core/rxdb/tests/fixtures/workjet-supervisor-route-display-v1.json. Do not edit.
#![allow(dead_code)]
use serde::{Deserialize, Serialize};

pub(crate) const CONTRACT_VERSION: u64 = 1;
pub(crate) const CONTRACT_SCHEMA: &str = "ctox.workjet.supervisor.route-display.v1";

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
    #[serde(rename = "ctox.workjet.supervisor.route-capabilities.v1")]
    CtoxWorkjetSupervisorRouteCapabilitiesV1,
}
impl WireValidate for RouteCapabilitiesSchema {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum RouteReadCommand {
    #[serde(rename = "ctox.workjet.project.supervisor.route.read.v1")]
    CtoxWorkjetProjectSupervisorRouteReadV1,
}
impl WireValidate for RouteReadCommand {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum RouteDisplaySchema {
    #[serde(rename = "ctox.workjet.supervisor.route-display.v1")]
    CtoxWorkjetSupervisorRouteDisplayV1,
}
impl WireValidate for RouteDisplaySchema {
    fn validate(&self) -> Result<(), String> {
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
pub(crate) struct RequestedRouteSource {
    pub(crate) execution_key: String,
    pub(crate) request_revision: String,
    pub(crate) error_code: String,
    pub(crate) created_at_ms: i64,
}
impl WireValidate for RequestedRouteSource {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.execution_key;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("RequestedRouteSource.execution_key violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("RequestedRouteSource.execution_key violates max_chars".into());
            }
        }
        {
            let value = &self.request_revision;
            value.validate()?;
            if value.chars().count() < 64 {
                return Err("RequestedRouteSource.request_revision violates min_chars".into());
            }
            if value.chars().count() > 64 {
                return Err("RequestedRouteSource.request_revision violates max_chars".into());
            }
        }
        {
            let value = &self.error_code;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("RequestedRouteSource.error_code violates min_chars".into());
            }
            if value.chars().count() > 96 {
                return Err("RequestedRouteSource.error_code violates max_chars".into());
            }
        }
        {
            let value = &self.created_at_ms;
            value.validate()?;
            if *value < 0 {
                return Err("RequestedRouteSource.created_at_ms violates minimum".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ActualSupervisorProducer {
    pub(crate) run_id: String,
    pub(crate) turn_id: String,
    pub(crate) receipt_id: String,
    pub(crate) luma_id: String,
    pub(crate) configuration_revision: u64,
    pub(crate) computer_id: String,
    pub(crate) harness: String,
    pub(crate) route_id: String,
    pub(crate) model: String,
    pub(crate) catalog_checked_at_ms: i64,
}
impl WireValidate for ActualSupervisorProducer {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.run_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ActualSupervisorProducer.run_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("ActualSupervisorProducer.run_id violates max_chars".into());
            }
        }
        {
            let value = &self.turn_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ActualSupervisorProducer.turn_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("ActualSupervisorProducer.turn_id violates max_chars".into());
            }
        }
        {
            let value = &self.receipt_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ActualSupervisorProducer.receipt_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("ActualSupervisorProducer.receipt_id violates max_chars".into());
            }
        }
        {
            let value = &self.luma_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ActualSupervisorProducer.luma_id violates min_chars".into());
            }
            if value.chars().count() > 160 {
                return Err("ActualSupervisorProducer.luma_id violates max_chars".into());
            }
        }
        {
            let value = &self.configuration_revision;
            value.validate()?;
            if *value < 1 {
                return Err(
                    "ActualSupervisorProducer.configuration_revision violates minimum".into(),
                );
            }
        }
        {
            let value = &self.computer_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ActualSupervisorProducer.computer_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("ActualSupervisorProducer.computer_id violates max_chars".into());
            }
        }
        {
            let value = &self.harness;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ActualSupervisorProducer.harness violates min_chars".into());
            }
            if value.chars().count() > 32 {
                return Err("ActualSupervisorProducer.harness violates max_chars".into());
            }
        }
        {
            let value = &self.route_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ActualSupervisorProducer.route_id violates min_chars".into());
            }
            if value.chars().count() > 160 {
                return Err("ActualSupervisorProducer.route_id violates max_chars".into());
            }
        }
        {
            let value = &self.model;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ActualSupervisorProducer.model violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("ActualSupervisorProducer.model violates max_chars".into());
            }
        }
        {
            let value = &self.catalog_checked_at_ms;
            value.validate()?;
            if *value < 0 {
                return Err(
                    "ActualSupervisorProducer.catalog_checked_at_ms violates minimum".into(),
                );
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
    pub(crate) source: Option<RequestedRouteSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) actual: Option<ActualSupervisorProducer>,
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
        if let Some(value) = &self.source {
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
        "SupervisorRouteCapabilities" => {
            serde_json::from_value::<SupervisorRouteCapabilities>(value)
                .map_err(|e| e.to_string())?
                .validate()
        }
        "RouteDisplaySchema" => serde_json::from_value::<RouteDisplaySchema>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "ConfiguredSupervisorRoute" => serde_json::from_value::<ConfiguredSupervisorRoute>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "RequestedRouteSource" => serde_json::from_value::<RequestedRouteSource>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "ActualSupervisorProducer" => serde_json::from_value::<ActualSupervisorProducer>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "SupervisorRouteDisplay" => serde_json::from_value::<SupervisorRouteDisplay>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        _ => Err("unknown contract type".into()),
    }
}
