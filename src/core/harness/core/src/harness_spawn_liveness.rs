use crate::config::{DEFAULT_AGENT_MAX_DEPTH, DEFAULT_AGENT_MAX_THREADS};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HarnessSubagentSpawnContract {
    pub pattern: &'static str,
    pub parent_entity_type: &'static str,
    pub child_entity_type: &'static str,
    pub ranking_function: &'static str,
    pub finite_bound: &'static str,
    pub recursion_guard: &'static str,
    pub worker_tool_surface: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HarnessSubagentSpawnModelReport {
    pub ok: bool,
    pub contracts: Vec<HarnessSubagentSpawnContract>,
    pub violations: Vec<String>,
    pub proof: String,
    pub recommended_gate: String,
}

pub fn analyze_harness_subagent_spawn_model() -> HarnessSubagentSpawnModelReport {
    let contracts = harness_subagent_spawn_contracts().to_vec();
    let mut violations = Vec::new();

    if DEFAULT_AGENT_MAX_DEPTH < 1 {
        violations.push("default_agent_max_depth_must_be_positive".to_string());
    }
    match DEFAULT_AGENT_MAX_THREADS {
        Some(max_threads) if max_threads > 0 => {}
        Some(_) => violations.push("default_agent_max_threads_must_be_positive".to_string()),
        None => violations.push("default_agent_max_threads_must_be_finite".to_string()),
    }

    let tools_spec = include_str!("tools/spec.rs");
    let spawn_handler = include_str!("tools/handlers/multi_agents/spawn.rs");
    let agent_jobs_handler = include_str!("tools/handlers/agent_jobs.rs");
    let fork_record = include_str!("../../FORK.md");

    if !tools_spec
        .contains("let subagent_session = matches!(session_source, SessionSource::SubAgent(_));")
        || !tools_spec.contains("features.enabled(Feature::Collab) && !subagent_session")
        || !tools_spec.contains("features.enabled(Feature::SpawnCsv) && !subagent_session")
    {
        violations.push("subagent_tool_surface_must_disable_recursive_spawn_tools".to_string());
    }
    if !tools_spec.contains("label.starts_with(\"agent_job:\")")
        || !tools_spec.contains("let agent_jobs_worker_tools = include_agent_job_worker_tools;")
    {
        violations.push("agent_job_workers_must_only_keep_report_tool".to_string());
    }
    if !spawn_handler.contains("exceeds_thread_spawn_depth_limit(child_depth, max_depth)")
        || !spawn_handler.contains("fork_context is disabled for CTOX sub-agents")
    {
        violations
            .push("thread_spawn_handler_must_enforce_depth_and_disable_fork_context".to_string());
    }
    if !agent_jobs_handler
        .contains("normalize_concurrency(requested_concurrency, turn.config.agent_max_threads)")
        || !agent_jobs_handler.contains("exceeds_thread_spawn_depth_limit(child_depth, max_depth)")
        || !agent_jobs_handler.contains("build_worker_prompt(&job, &item)")
    {
        violations.push("agent_job_spawn_loop_must_bound_concurrency_depth_and_items".to_string());
    }
    if !fork_record
        .contains("Thread-spawn subagents cannot recursively use collaboration-mode escalation")
        || !fork_record.contains("The review state machine must see one parent result")
    {
        violations.push("fork_record_must_state_leaf_subagent_invariants".to_string());
    }

    HarnessSubagentSpawnModelReport {
        ok: violations.is_empty(),
        contracts,
        violations,
        proof: "Harness subagents are modeled as leaf thread-control children. Thread-spawn children consume the depth rank depth_remaining = max_depth - depth, so recursive spawning is rejected once the rank reaches zero. All subagent sessions have collaboration and agent-job spawning tools removed, so a child cannot create another child through the public tool surface. Agent-job workers are finite because they are spawned from a finite item table, concurrency is capped by agents.max_threads, depth is checked before dispatch, and workers retain only report_agent_job_result. Therefore every accepted harness subagent path is either a bounded leaf execution, a finite agent-job row execution, or a rejected spawn with no recursive child-producing intervention."
            .to_string(),
        recommended_gate: "Run as a release/CI gate together with cargo test, cargo check, and process-mining spawn-liveness. Keep the static analyzer as a unit test so normal test builds fail on invariant drift; do not put this into every rustc compile via build.rs because it is a repository-conformance proof, not type checking."
            .to_string(),
    }
}

fn harness_subagent_spawn_contracts() -> &'static [HarnessSubagentSpawnContract] {
    &[
        HarnessSubagentSpawnContract {
            pattern: "thread-spawn-subagent",
            parent_entity_type: "Thread",
            child_entity_type: "Thread",
            ranking_function: "max_depth - child_depth",
            finite_bound: "agents.max_depth and agents.max_threads",
            recursion_guard: "all SubAgent sessions omit collab and spawn_agents_on_csv tools",
            worker_tool_surface: "leaf worker tools only",
        },
        HarnessSubagentSpawnContract {
            pattern: "agent-job-worker",
            parent_entity_type: "AgentJob",
            child_entity_type: "Thread",
            ranking_function: "pending_agent_job_items",
            finite_bound: "finite persisted job item table and agents.max_threads concurrency",
            recursion_guard: "agent_job workers omit collab and spawn_agents_on_csv tools",
            worker_tool_surface: "report_agent_job_result only",
        },
        HarnessSubagentSpawnContract {
            pattern: "internal-subagent",
            parent_entity_type: "ControlPlane",
            child_entity_type: "Thread",
            ranking_function: "single internal task invocation",
            finite_bound: "no public child-spawn tool surface",
            recursion_guard: "all SubAgent sessions omit collab and spawn_agents_on_csv tools",
            worker_tool_surface: "task-specific tools without delegation",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harness_subagent_spawn_model_is_bounded_and_leaf_only() {
        let report = analyze_harness_subagent_spawn_model();
        assert!(
            report.ok,
            "harness subagent spawn model violations: {:?}",
            report.violations
        );
    }
}
