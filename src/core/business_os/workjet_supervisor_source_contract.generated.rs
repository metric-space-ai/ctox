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
        _ => Err("unknown contract type".into()),
    }
}
