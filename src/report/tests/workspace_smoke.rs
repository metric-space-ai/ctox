//! Smoke tests for `state::create_run` and `Workspace`.

use serde_json::Value;

use crate::report::state::{create_run, load_run, CreateRunParams};
use crate::report::tests::fixtures::{rascon_create_params, TestRoot};
use crate::report::workspace::{SkillMode, Workspace};

#[test]
fn create_run_persists_and_loads() {
    let root = TestRoot::new().expect("temp root");
    let params = rascon_create_params();
    let run_id = create_run(root.path(), params.clone()).expect("create_run");
    assert!(run_id.starts_with("run_"), "run_id format: {run_id}");
    let loaded = load_run(root.path(), &run_id).expect("load_run");
    assert_eq!(loaded.run_id, run_id);
    assert_eq!(loaded.report_type_id, params.report_type_id);
    assert_eq!(loaded.domain_profile_id, params.domain_profile_id);
    assert_eq!(loaded.depth_profile_id, params.depth_profile_id);
    assert_eq!(loaded.style_profile_id, params.style_profile_id);
    assert_eq!(loaded.language, params.language);
    assert_eq!(loaded.raw_topic, params.raw_topic);
    assert_eq!(loaded.status, "created");
}

#[test]
fn create_run_rejects_unknown_report_type() {
    let root = TestRoot::new().expect("temp root");
    let mut params = rascon_create_params();
    params.report_type_id = "definitely_not_a_real_report_type".into();
    let err = create_run(root.path(), params).expect_err("create_run should reject");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("definitely_not_a_real_report_type") || msg.contains("report_type_id"),
        "expected rejection mentioning the bad report_type_id; got: {msg}"
    );
}

#[test]
fn workspace_snapshot_has_required_keys() {
    let root = TestRoot::new().expect("temp root");
    let run_id = create_run(root.path(), rascon_create_params()).expect("create_run");
    let workspace = Workspace::load(root.path(), &run_id).expect("workspace load");
    let snap = workspace.workspace_snapshot().expect("snapshot");
    let obj = snap.as_object().expect("snapshot is a JSON object");
    for key in &[
        "run_metadata",
        "report_type",
        "expected_blocks",
        "existing_blocks",
        "pending_blocks",
        "completeness",
        "character_budget",
        "evidence_register_size",
    ] {
        assert!(
            obj.contains_key(*key),
            "workspace_snapshot missing required key {key:?}; keys present: {:?}",
            obj.keys().collect::<Vec<_>>()
        );
    }
    // run_metadata is itself an object with run_id.
    let metadata = obj.get("run_metadata").unwrap();
    assert_eq!(
        metadata.get("run_id").and_then(Value::as_str),
        Some(run_id.as_str()),
        "run_metadata.run_id should be the created run id"
    );
}

#[test]
fn workspace_skill_input_write_mode_has_brief() {
    let root = TestRoot::new().expect("temp root");
    let run_id = create_run(root.path(), rascon_create_params()).expect("create_run");
    let workspace = Workspace::load(root.path(), &run_id).expect("workspace load");
    let bundle = workspace
        .skill_input(
            SkillMode::Write,
            &["doc_study__study_title_block".to_string()],
            Some("test brief"),
            &[],
        )
        .expect("skill_input write mode");
    assert_eq!(
        bundle.get("mode").and_then(Value::as_str),
        Some("write"),
        "mode should be 'write'"
    );
    assert_eq!(
        bundle.get("brief").and_then(Value::as_str),
        Some("test brief"),
        "brief should round-trip"
    );
    let goals = bundle.get("goals").expect("goals key present");
    assert!(
        goals.as_array().map(|a| a.is_empty()).unwrap_or(false),
        "goals[] should be empty in write mode; got {goals:?}"
    );
}

#[test]
fn workspace_skill_input_revision_mode_has_goals() {
    let root = TestRoot::new().expect("temp root");
    let run_id = create_run(root.path(), rascon_create_params()).expect("create_run");
    let workspace = Workspace::load(root.path(), &run_id).expect("workspace load");
    let goals_in = vec!["tighten".to_string()];
    let bundle = workspace
        .skill_input(
            SkillMode::Revision,
            &["doc_study__management_summary".to_string()],
            None,
            &goals_in,
        )
        .expect("skill_input revision mode");
    assert_eq!(
        bundle.get("mode").and_then(Value::as_str),
        Some("revision"),
        "mode should be 'revision'"
    );
    let goals = bundle
        .get("goals")
        .and_then(Value::as_array)
        .expect("goals[] is array");
    let collected: Vec<&str> = goals.iter().filter_map(Value::as_str).collect();
    assert_eq!(
        collected,
        vec!["tighten"],
        "revision mode goals[] should contain the supplied goal"
    );
}

#[test]
fn workspace_committed_and_pending_blocks_default_empty() {
    let root = TestRoot::new().expect("temp root");
    let run_id = create_run(root.path(), rascon_create_params()).expect("create_run");
    let workspace = Workspace::load(root.path(), &run_id).expect("workspace load");
    let committed = workspace.committed_blocks().expect("committed_blocks");
    let pending = workspace.pending_blocks().expect("pending_blocks");
    assert!(committed.is_empty(), "fresh run has no committed blocks");
    assert!(pending.is_empty(), "fresh run has no pending blocks");
    let evidence = workspace.evidence_register().expect("evidence_register");
    assert!(evidence.is_empty(), "fresh run has no evidence");
    let budget = workspace.character_budget().expect("character_budget");
    assert_eq!(budget.actual_chars, 0);
    assert_eq!(budget.status, "not_started");
}

#[test]
fn create_run_rejects_empty_topic() {
    let root = TestRoot::new().expect("temp root");
    let mut params: CreateRunParams = rascon_create_params();
    params.raw_topic = "   ".into();
    let err = create_run(root.path(), params).expect_err("empty topic rejected");
    let msg = format!("{err:#}");
    assert!(
        msg.to_lowercase().contains("topic"),
        "expected rejection mentioning topic; got {msg}"
    );
}
