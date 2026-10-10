// Generated from src/core/rxdb/tests/fixtures/workjet-worker-outcome-v1.json. Do not edit.
#![allow(dead_code)]
use serde::{Deserialize, Serialize};

pub(crate) const CONTRACT_VERSION: u64 = 1;
pub(crate) const CONTRACT_SCHEMA: &str = "ctox.workjet.worker-outcome.v1";

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
pub(crate) enum WorkerOutcomeSchema {
    #[serde(rename = "ctox.workjet.worker-outcome.v1")]
    CtoxWorkjetWorkerOutcomeV1,
}
impl WireValidate for WorkerOutcomeSchema {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum TerminalPullRequestState {
    #[serde(rename = "merged")]
    Merged,
    #[serde(rename = "closed")]
    Closed,
}
impl WireValidate for TerminalPullRequestState {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum PullRequestProvider {
    #[serde(rename = "github")]
    Github,
}
impl WireValidate for PullRequestProvider {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkerPullRequestOutcome {
    pub(crate) provider: PullRequestProvider,
    pub(crate) number: u64,
    pub(crate) url: String,
    pub(crate) head_oid: String,
    pub(crate) state: TerminalPullRequestState,
}
impl WireValidate for WorkerPullRequestOutcome {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.provider;
            value.validate()?;
        }
        {
            let value = &self.number;
            value.validate()?;
            if *value < 1 {
                return Err("WorkerPullRequestOutcome.number violates minimum".into());
            }
        }
        {
            let value = &self.url;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("WorkerPullRequestOutcome.url violates min_chars".into());
            }
            if value.chars().count() > 2048 {
                return Err("WorkerPullRequestOutcome.url violates max_chars".into());
            }
        }
        {
            let value = &self.head_oid;
            value.validate()?;
            if value.chars().count() < 40 {
                return Err("WorkerPullRequestOutcome.head_oid violates min_chars".into());
            }
            if value.chars().count() > 64 {
                return Err("WorkerPullRequestOutcome.head_oid violates max_chars".into());
            }
        }
        {
            let value = &self.state;
            value.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkerTerminalReceipt {
    pub(crate) schema: WorkerOutcomeSchema,
    pub(crate) worker_thread_id: String,
    pub(crate) environment_id: String,
    pub(crate) computer_id: String,
    pub(crate) branch: String,
    pub(crate) execution_stopped: bool,
    pub(crate) pull_request: WorkerPullRequestOutcome,
}
impl WireValidate for WorkerTerminalReceipt {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.schema;
            value.validate()?;
        }
        {
            let value = &self.worker_thread_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("WorkerTerminalReceipt.worker_thread_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("WorkerTerminalReceipt.worker_thread_id violates max_chars".into());
            }
        }
        {
            let value = &self.environment_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("WorkerTerminalReceipt.environment_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("WorkerTerminalReceipt.environment_id violates max_chars".into());
            }
        }
        {
            let value = &self.computer_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("WorkerTerminalReceipt.computer_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("WorkerTerminalReceipt.computer_id violates max_chars".into());
            }
        }
        {
            let value = &self.branch;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("WorkerTerminalReceipt.branch violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("WorkerTerminalReceipt.branch violates max_chars".into());
            }
        }
        {
            let value = &self.execution_stopped;
            value.validate()?;
        }
        {
            let value = &self.pull_request;
            value.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn validate_fixture(kind: &str, value: serde_json::Value) -> Result<(), String> {
    match kind {
        "WorkerOutcomeSchema" => serde_json::from_value::<WorkerOutcomeSchema>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "TerminalPullRequestState" => serde_json::from_value::<TerminalPullRequestState>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "PullRequestProvider" => serde_json::from_value::<PullRequestProvider>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "WorkerPullRequestOutcome" => serde_json::from_value::<WorkerPullRequestOutcome>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "WorkerTerminalReceipt" => serde_json::from_value::<WorkerTerminalReceipt>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        _ => Err("unknown contract type".into()),
    }
}
