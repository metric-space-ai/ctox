//! Live Linear verification through the hub exec path (read-only key).
//! `CTOX_TOOLHUB_LIVE_LINEAR_TOKEN=… cargo test --test live_linear -- --ignored --nocapture`
mod support;
use ctox_toolhub::AuthSchemeKind;
use serde_json::{json, Value};

fn call(tool: &str, args: Value) -> Option<Result<Value, String>> {
    support::live_exec("linear", AuthSchemeKind::ApiKey, tool, args)
}

#[test]
#[ignore]
fn live_all_read_actions() {
    let mut fails = vec![];
    let mut run = |tool: &str, args: Value| -> Value {
        match call(tool, args) {
            None => Value::Null,
            Some(Ok(v)) => { println!("LIVE PASS {tool}"); v }
            Some(Err(e)) => { println!("LIVE FAIL {tool}: {}", e.chars().take(400).collect::<String>()); fails.push(tool.to_string()); Value::Null }
        }
    };
    if std::env::var("CTOX_TOOLHUB_LIVE_LINEAR_TOKEN").is_err() { return; }
    run("LINEAR_GET_VIEWER", json!({}));
    let teams = run("LINEAR_LIST_TEAMS", json!({}));
    let team = support::find_str(&teams, "id").unwrap_or_default();
    run("LINEAR_GET_TEAM", json!({"team_id": team}));
    run("LINEAR_LIST_USERS", json!({}));
    run("LINEAR_LIST_WORKFLOW_STATES", json!({"team_id": team}));
    run("LINEAR_LIST_CYCLES", json!({"team_id": team}));
    run("LINEAR_LIST_ISSUE_LABELS", json!({"team_id": team}));
    let issue = run("LINEAR_GET_ISSUE", json!({"issue_id": "CTO-5"}));
    assert!(issue.to_string().contains("CTO-5") || issue.is_null(), "CTO-5 not returned");
    run("LINEAR_LIST_ISSUES", json!({"team_id": team}));
    run("LINEAR_SEARCH_ISSUES", json!({"term": "Toolhub live test"}));
    let comments = run("LINEAR_LIST_COMMENTS", json!({"issue_id": "CTO-5"}));
    if let Some(c) = support::find_str(&comments, "id") { run("LINEAR_GET_COMMENT", json!({"comment_id": c})); } else { println!("LIVE SKIP LINEAR_GET_COMMENT (no comment)"); }
    let projects = run("LINEAR_LIST_PROJECTS", json!({}));
    if let Some(p) = support::find_str(&projects, "id") {
        run("LINEAR_GET_PROJECT", json!({"project_id": p}));
        let ups = run("LINEAR_LIST_PROJECT_STATUS_UPDATES", json!({"project_id": p}));
        if let Some(u) = support::find_str(&ups, "id") { run("LINEAR_GET_PROJECT_STATUS_UPDATE", json!({"status_update_id": u})); } else { println!("LIVE SKIP LINEAR_GET_PROJECT_STATUS_UPDATE (none)"); }
    } else { println!("LIVE SKIP LINEAR_GET_PROJECT (no project)"); }
    println!("LIVE SKIP LINEAR_GET_ATTACHMENT (needs attachment id)");
    assert!(fails.is_empty(), "live failures: {fails:?}");
}
