// Generated from src/core/rxdb/tests/fixtures/workjet-worker-execution-policy-v1.json. Do not edit.
#![allow(dead_code)]
use serde::{Deserialize, Serialize};

pub(crate) const CONTRACT_VERSION: u64 = 1;
pub(crate) const CONTRACT_SCHEMA: &str = "ctox.workjet.worker_execution_policy.v1";

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
pub(crate) enum WorkerExecutionPolicyMode {
    #[serde(rename = "autonomous-worktree")]
    AutonomousWorktree,
}
impl WireValidate for WorkerExecutionPolicyMode {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkerExecutionPolicyReference {
    pub(crate) mode: WorkerExecutionPolicyMode,
    #[serde(rename = "projectId")]
    pub(crate) project_id: String,
    pub(crate) revision: u64,
}
impl WireValidate for WorkerExecutionPolicyReference {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.mode;
            value.validate()?;
        }
        {
            let value = &self.project_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("WorkerExecutionPolicyReference.projectId violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("WorkerExecutionPolicyReference.projectId violates max_chars".into());
            }
        }
        {
            let value = &self.revision;
            value.validate()?;
            if *value < 1 {
                return Err("WorkerExecutionPolicyReference.revision violates minimum".into());
            }
            if *value > 9007199254740991 {
                return Err("WorkerExecutionPolicyReference.revision violates maximum".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn validate_fixture(kind: &str, value: serde_json::Value) -> Result<(), String> {
    match kind {
        "WorkerExecutionPolicyMode" => serde_json::from_value::<WorkerExecutionPolicyMode>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "WorkerExecutionPolicyReference" => {
            serde_json::from_value::<WorkerExecutionPolicyReference>(value)
                .map_err(|e| e.to_string())?
                .validate()
        }
        _ => Err("unknown contract type".into()),
    }
}
