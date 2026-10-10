// Generated from src/core/rxdb/tests/fixtures/workjet-project-execution-policy-v1.json. Do not edit.
#![allow(dead_code)]
use serde::{Deserialize, Serialize};

pub(crate) const CONTRACT_VERSION: u64 = 1;
pub(crate) const CONTRACT_SCHEMA: &str = "ctox.workjet.project_execution_policy.v1";

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
pub(crate) enum ProjectExecutionPolicyMode {
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "autonomous_worktree")]
    AutonomousWorktree,
}
impl WireValidate for ProjectExecutionPolicyMode {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum ProjectExecutionPolicySchema {
    #[serde(rename = "ctox.workjet.project_execution_policy.v1")]
    CtoxWorkjetProjectExecutionPolicyV1,
}
impl WireValidate for ProjectExecutionPolicySchema {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectExecutionPolicyUpdate {
    pub(crate) schema: ProjectExecutionPolicySchema,
    pub(crate) mode: ProjectExecutionPolicyMode,
    pub(crate) expected_revision: u64,
}
impl WireValidate for ProjectExecutionPolicyUpdate {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.schema;
            value.validate()?;
        }
        {
            let value = &self.mode;
            value.validate()?;
        }
        {
            let value = &self.expected_revision;
            value.validate()?;
            if *value > 9007199254740991 {
                return Err(
                    "ProjectExecutionPolicyUpdate.expected_revision violates maximum".into(),
                );
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectExecutionPolicy {
    pub(crate) schema: ProjectExecutionPolicySchema,
    pub(crate) mode: ProjectExecutionPolicyMode,
    pub(crate) revision: u64,
}
impl WireValidate for ProjectExecutionPolicy {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.schema;
            value.validate()?;
        }
        {
            let value = &self.mode;
            value.validate()?;
        }
        {
            let value = &self.revision;
            value.validate()?;
            if *value > 9007199254740991 {
                return Err("ProjectExecutionPolicy.revision violates maximum".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn validate_fixture(kind: &str, value: serde_json::Value) -> Result<(), String> {
    match kind {
        "ProjectExecutionPolicyMode" => serde_json::from_value::<ProjectExecutionPolicyMode>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "ProjectExecutionPolicySchema" => {
            serde_json::from_value::<ProjectExecutionPolicySchema>(value)
                .map_err(|e| e.to_string())?
                .validate()
        }
        "ProjectExecutionPolicyUpdate" => {
            serde_json::from_value::<ProjectExecutionPolicyUpdate>(value)
                .map_err(|e| e.to_string())?
                .validate()
        }
        "ProjectExecutionPolicy" => serde_json::from_value::<ProjectExecutionPolicy>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        _ => Err("unknown contract type".into()),
    }
}
