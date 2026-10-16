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
