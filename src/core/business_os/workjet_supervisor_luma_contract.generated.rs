// Generated from src/core/rxdb/tests/fixtures/workjet-supervisor-luma-v1.json. Do not edit.
#![allow(dead_code)]
use serde::{Deserialize, Serialize};

pub(crate) const CONTRACT_VERSION: u64 = 1;
pub(crate) const CONTRACT_SCHEMA: &str = "ctox.workjet.supervisor_luma.v1";

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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectSupervisorLuma {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) supervisor_luma_id: Option<String>,
}
impl WireValidate for ProjectSupervisorLuma {
    fn validate(&self) -> Result<(), String> {
        if let Some(value) = &self.supervisor_luma_id {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ProjectSupervisorLuma.supervisor_luma_id violates min_chars".into());
            }
            if value.chars().count() > 160 {
                return Err("ProjectSupervisorLuma.supervisor_luma_id violates max_chars".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn validate_fixture(kind: &str, value: serde_json::Value) -> Result<(), String> {
    match kind {
        "ProjectSupervisorLuma" => serde_json::from_value::<ProjectSupervisorLuma>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        _ => Err("unknown contract type".into()),
    }
}
