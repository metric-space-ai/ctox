//! Harbor Agent Trajectory Interchange Format (ATIF) export.
//!
//! Harbor expects the custom agent to write a `trajectory.json` after every
//! task run. The schema mirrors `harbor.models.trajectories` in the Harbor
//! Python package. We emit ATIF-v1.2 which is what the Harbor release ships
//! in code today.
//!
//! This exporter is lo-fi: every CTOX message becomes one Step, with the raw
//! message content as the step message. Tool-call reconstruction from the
//! embedded content blob is left to a future hi-fi pass — the harness scores
//! runs by container verification, not by trajectory introspection, so lo-fi
//! is sufficient for benchmarking.
//!
//! See https://www.harborframework.com/docs/agents/trajectory-format

use anyhow::Result;
use serde::Serialize;
use std::path::Path;

use crate::lcm::{LcmConfig, LcmEngine};

/// The schema version the Harbor Python package ships today. Update when
/// Harbor bumps it — the docs site may run ahead of the release.
pub const ATIF_SCHEMA_VERSION: &str = "ATIF-v1.2";

#[derive(Debug, Serialize)]
pub struct Trajectory {
    pub schema_version: String,
    pub session_id: String,
    pub agent: Agent,
    pub steps: Vec<Step>,
    pub final_metrics: FinalMetrics,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Agent {
    pub name: String,
    pub version: String,
    pub model_name: String,
}

#[derive(Debug, Serialize)]
pub struct Step {
    pub step_id: u64,
    pub timestamp: String,
    pub source: StepSource,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<Metrics>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StepSource {
    User,
    Agent,
    System,
}

#[derive(Debug, Serialize)]
pub struct Metrics {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

#[derive(Debug, Serialize)]
pub struct FinalMetrics {
    pub total_prompt_tokens: u64,
    pub total_completion_tokens: u64,
}

/// Build a trajectory from a CTOX conversation stored in `ctox_lcm.db`.
pub fn build_trajectory(
    db_path: &Path,
    conversation_id: i64,
    session_id: &str,
    agent_name: &str,
    agent_version: &str,
    model_name: &str,
    notes: Option<String>,
) -> Result<Trajectory> {
    let engine = LcmEngine::open(db_path, LcmConfig::default())?;
    let messages = engine.messages_for_conversation(conversation_id)?;
    let mut steps = Vec::with_capacity(messages.len());
    let mut total_prompt: u64 = 0;
    let mut total_completion: u64 = 0;
    for (idx, msg) in messages.iter().enumerate() {
        let source = match msg.role.as_str() {
            "user" => StepSource::User,
            "assistant" => StepSource::Agent,
            _ => StepSource::System,
        };
        let tokens = msg.token_count.max(0) as u64;
        let metrics = match source {
            StepSource::User => {
                total_prompt = total_prompt.saturating_add(tokens);
                Some(Metrics {
                    prompt_tokens: tokens,
                    completion_tokens: 0,
                })
            }
            StepSource::Agent => {
                total_completion = total_completion.saturating_add(tokens);
                Some(Metrics {
                    prompt_tokens: 0,
                    completion_tokens: tokens,
                })
            }
            StepSource::System => None,
        };
        steps.push(Step {
            step_id: (idx + 1) as u64,
            timestamp: msg.created_at.clone(),
            source,
            message: msg.content.clone(),
            metrics,
        });
    }
    Ok(Trajectory {
        schema_version: ATIF_SCHEMA_VERSION.to_string(),
        session_id: session_id.to_string(),
        agent: Agent {
            name: agent_name.to_string(),
            version: agent_version.to_string(),
            model_name: model_name.to_string(),
        },
        steps,
        final_metrics: FinalMetrics {
            total_prompt_tokens: total_prompt,
            total_completion_tokens: total_completion,
        },
        notes,
    })
}

/// Write a trajectory as pretty JSON, creating parent directories on demand.
pub fn write_trajectory(trajectory: &Trajectory, out_path: &Path) -> Result<()> {
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let json = serde_json::to_string_pretty(trajectory)?;
    std::fs::write(out_path, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_trajectory_serializes() {
        let t = Trajectory {
            schema_version: ATIF_SCHEMA_VERSION.to_string(),
            session_id: "s1".into(),
            agent: Agent {
                name: "ctox".into(),
                version: "0.0.0".into(),
                model_name: "openai/gpt-5.4".into(),
            },
            steps: vec![],
            final_metrics: FinalMetrics {
                total_prompt_tokens: 0,
                total_completion_tokens: 0,
            },
            notes: None,
        };
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("ATIF-v1.2"));
        assert!(json.contains("\"steps\":[]"));
    }

    #[test]
    fn step_source_lowercases() {
        let s = Step {
            step_id: 1,
            timestamp: "2026-04-14T00:00:00Z".into(),
            source: StepSource::Agent,
            message: "hi".into(),
            metrics: None,
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"source\":\"agent\""));
    }
}
