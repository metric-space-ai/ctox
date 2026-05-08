//! Manager-loop smoke tests.
//!
//! The manager wires `&dyn InferenceCallable` and `&dyn SubSkillRunner`
//! against a temporary CTOX root. We script both of these with small
//! deterministic fixtures so the entire pipeline runs without network
//! traffic. The four listed scenarios cover the gate-relevant decision
//! shapes; full end-to-end finished/ready-to-finish coverage lives in
//! `rascon_replay.rs`.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;

use crate::report::manager::{run_manager, ManagerConfig, ManagerDecision};
use crate::report::manager_prompt::{build_manager_run_input, build_manager_system_prompt};
use crate::report::state::create_run;
use crate::report::sub_skill::{CtoxSubSkillRunner, InferenceCallable};
use crate::report::tests::fixtures::{rascon_create_params, TestRoot};
use crate::report::workspace::Workspace;

/// Inference fixture that returns scripted manager turns plus three
/// canned writer/revisor/flow_reviewer responses. Discriminates between
/// the manager prompt and the three sub-skill prompts by inspecting the
/// substring in the system prompt.
struct ScriptedInference {
    manager_responses: Mutex<VecDeque<String>>,
    writer_response: String,
    revisor_response: String,
    flow_reviewer_response: String,
}

impl ScriptedInference {
    fn new(manager_script: Vec<String>, writer: &str, revisor: &str, flow_reviewer: &str) -> Self {
        Self {
            manager_responses: Mutex::new(VecDeque::from(manager_script)),
            writer_response: writer.to_string(),
            revisor_response: revisor.to_string(),
            flow_reviewer_response: flow_reviewer.to_string(),
        }
    }
}

impl InferenceCallable for ScriptedInference {
    fn run_one_shot(
        &self,
        system_prompt: &str,
        _user_payload: &Value,
        _timeout: Duration,
    ) -> anyhow::Result<String> {
        // The manager system prompt opens with `# CTOX Deep Research Manager`.
        if system_prompt.contains("Deep Research Manager") {
            let mut q = self.manager_responses.lock().unwrap();
            return q
                .pop_front()
                .ok_or_else(|| anyhow::anyhow!("scripted manager queue exhausted"));
        }
        if system_prompt.contains("Block Writer") {
            return Ok(self.writer_response.clone());
        }
        if system_prompt.contains("Revision") {
            return Ok(self.revisor_response.clone());
        }
        if system_prompt.contains("Flow Review") {
            return Ok(self.flow_reviewer_response.clone());
        }
        // Fallback: assume manager.
        let mut q = self.manager_responses.lock().unwrap();
        q.pop_front()
            .ok_or_else(|| anyhow::anyhow!("scripted queue exhausted (no prompt match)"))
    }
}

/// Build a small `(SubSkillRunner, InferenceCallable)` pair.
fn build_runner_pair(
    root: &std::path::Path,
    script: ScriptedInference,
) -> (CtoxSubSkillRunner, std::sync::Arc<ScriptedInference>) {
    let arc = std::sync::Arc::new(script);
    let runner = CtoxSubSkillRunner::new(
        root,
        Box::new(SharedInference {
            inner: std::sync::Arc::clone(&arc),
        }),
    )
    .expect("CtoxSubSkillRunner");
    (runner, arc)
}

/// Tiny adapter so the same `ScriptedInference` instance can back both
/// the sub-skill runner and the manager inference call.
struct SharedInference {
    inner: std::sync::Arc<ScriptedInference>,
}

impl InferenceCallable for SharedInference {
    fn run_one_shot(
        &self,
        system_prompt: &str,
        user_payload: &Value,
        timeout: Duration,
    ) -> anyhow::Result<String> {
        self.inner
            .run_one_shot(system_prompt, user_payload, timeout)
    }
}

/// Quick check: `manager_system_prompt` mentions every tool exactly
/// once. Useful as a low-cost smoke that the prompt builder is wired.
#[test]
fn manager_system_prompt_lists_every_tool() {
    let prompt = build_manager_system_prompt();
    for name in crate::report::tools::TOOL_NAMES {
        assert!(
            prompt.contains(name),
            "manager system prompt should mention tool {name}"
        );
    }
}

/// `build_manager_run_input` returns a non-empty bilingual directive
/// for a fresh feasibility-study run.
#[test]
fn manager_run_input_has_directive_for_feasibility_study() {
    let root = TestRoot::new().expect("temp root");
    let run_id = create_run(root.path(), rascon_create_params()).expect("create_run");
    let ws = Workspace::load(root.path(), &run_id).expect("workspace");
    let body = build_manager_run_input(&ws).expect("build_manager_run_input");
    assert!(
        body.contains("feasibility_study"),
        "directive must mention active report_type; body:\n{body}"
    );
    assert!(
        body.contains("workspace_snapshot"),
        "directive must mention workspace_snapshot tool; body:\n{body}"
    );
}

#[test]
fn manager_max_turns_blocks() {
    let root = TestRoot::new().expect("temp root");
    let run_id = create_run(root.path(), rascon_create_params()).expect("create_run");
    // Feed the manager a script that emits a fresh tool call every turn.
    // With max_turns=2 it should bail out as Blocked after consuming two
    // entries.
    let tool_call = serde_json::to_string(&serde_json::json!({
        "tool": "workspace_snapshot",
        "args": {}
    }))
    .unwrap();
    let script = ScriptedInference::new(
        vec![tool_call.clone(), tool_call.clone(), tool_call],
        "{}",
        "{}",
        "{}",
    );
    let (runner, arc) = build_runner_pair(root.path(), script);
    let cfg = ManagerConfig {
        max_turns: 2,
        max_run_duration: Duration::from_secs(60),
        ..ManagerConfig::default()
    };
    let outcome = run_manager(
        root.path(),
        &run_id,
        cfg,
        &runner,
        &SharedInference {
            inner: std::sync::Arc::clone(&arc),
        },
    )
    .expect("run_manager");
    assert_eq!(
        outcome.decision,
        ManagerDecision::Blocked,
        "max_turns exhaustion should block: {outcome:?}"
    );
    assert!(
        outcome.reason.contains("max_turns") || outcome.reason.contains("turns"),
        "reason should mention turns; got {:?}",
        outcome.reason
    );
}

#[test]
fn manager_finished_without_checks_is_downgraded_to_blocked() {
    // Even if the LLM emits `finished` straight away, the host loop-end
    // gate must downgrade to Blocked because no check ran.
    let root = TestRoot::new().expect("temp root");
    let run_id = create_run(root.path(), rascon_create_params()).expect("create_run");
    let finished_decision = serde_json::to_string(&serde_json::json!({
        "decision": "finished",
        "summary": "all done",
        "changed_blocks": [],
        "open_questions": [],
        "reason": "claims complete"
    }))
    .unwrap();
    let script = ScriptedInference::new(vec![finished_decision], "{}", "{}", "{}");
    let (runner, arc) = build_runner_pair(root.path(), script);
    let cfg = ManagerConfig {
        max_turns: 4,
        max_run_duration: Duration::from_secs(60),
        ..ManagerConfig::default()
    };
    let outcome = run_manager(
        root.path(),
        &run_id,
        cfg,
        &runner,
        &SharedInference {
            inner: std::sync::Arc::clone(&arc),
        },
    )
    .expect("run_manager");
    assert_eq!(
        outcome.decision,
        ManagerDecision::Blocked,
        "host gate should downgrade premature finished -> blocked: {outcome:?}"
    );
    assert!(
        outcome.reason.contains("loop-end gate"),
        "downgrade reason should mention loop-end gate; got {:?}",
        outcome.reason
    );
}

#[test]
fn manager_blocked_when_release_guard_fails() {
    // Manager calls completeness, character_budget, release_guard
    // (which fires LINT-FAB-DOI), narrative_flow, then finishes. The
    // host gate should downgrade because release_guard reported
    // ready_to_finish=false.
    let root = TestRoot::new().expect("temp root");
    let run_id = create_run(root.path(), rascon_create_params()).expect("create_run");
    // Pre-populate a committed block with a fabricated DOI so the
    // release_guard tool will fire LINT-FAB-DOI when the manager calls it.
    let body = "Eddy current per Mueller et al. (DOI: 10.9999/fake.0001) zeigt POD 87 Prozent.";
    crate::report::tests::fixtures::insert_committed_block(
        root.path(),
        &run_id,
        "doc_study",
        "detail_assessment_per_option",
        "Detailbewertung",
        120,
        body,
        &[],
    )
    .expect("seed dirty block");

    let scripted = vec![
        serde_json::to_string(&serde_json::json!({
            "tool": "completeness_check",
            "args": {}
        }))
        .unwrap(),
        serde_json::to_string(&serde_json::json!({
            "tool": "character_budget_check",
            "args": {}
        }))
        .unwrap(),
        serde_json::to_string(&serde_json::json!({
            "tool": "release_guard_check",
            "args": {}
        }))
        .unwrap(),
        serde_json::to_string(&serde_json::json!({
            "decision": "finished",
            "summary": "done",
            "changed_blocks": [],
            "open_questions": [],
            "reason": "claim done"
        }))
        .unwrap(),
    ];
    let script = ScriptedInference::new(scripted, "{}", "{}", "{}");
    let (runner, arc) = build_runner_pair(root.path(), script);
    let cfg = ManagerConfig {
        max_turns: 6,
        max_run_duration: Duration::from_secs(60),
        ..ManagerConfig::default()
    };
    let outcome = run_manager(
        root.path(),
        &run_id,
        cfg,
        &runner,
        &SharedInference {
            inner: std::sync::Arc::clone(&arc),
        },
    )
    .expect("run_manager");
    assert_eq!(
        outcome.decision,
        ManagerDecision::Blocked,
        "release_guard fail must block: {outcome:?}"
    );
    let release_guard = outcome
        .last_release_guard
        .as_ref()
        .expect("release_guard outcome captured");
    assert!(
        !release_guard.ready_to_finish,
        "release_guard must report not ready: {release_guard:?}"
    );
}
