//! Live GitHub verification through the hub exec path (read-only actions only)
//! against the public repo metric-space-ai/ctox.
//! `CTOX_TOOLHUB_LIVE_GITHUB_TOKEN=… CTOX_LIVE_GITHUB_RUN_ID=… cargo test --test live_github -- --ignored --nocapture`
mod support;
use ctox_toolhub::AuthSchemeKind;
use serde_json::{json, Value};

const O: &str = "metric-space-ai";
const R: &str = "ctox";

#[test]
#[ignore]
fn live_all_read_actions() {
    scenario();
}

/// Same scenario through the hot-plug hybrid spec `specs/github.json`.
#[test]
#[ignore]
fn live_all_read_actions_via_spec() {
    support::use_spec(include_str!("../specs/github.json"));
    scenario();
}

fn scenario() {
    if std::env::var("CTOX_TOOLHUB_LIVE_GITHUB_TOKEN").is_err() { return; }
    let mut fails = vec![];
    let mut run = |tool: &str, args: Value| -> Value {
        match support::live_exec("github", AuthSchemeKind::Bearer, tool, args).unwrap() {
            Ok(v) => { let s = v.to_string(); println!("LIVE PASS {tool}: {}", s.chars().take(150).collect::<String>()); v }
            Err(e) => { println!("LIVE FAIL {tool}: {}", e.chars().take(300).collect::<String>()); fails.push(tool.to_string()); Value::Null }
        }
    };
    let repo = run("GITHUB_GET_REPO", json!({"owner": O, "repo": R}));
    assert!(repo.to_string().contains("metric-space-ai/ctox"), "repo full_name missing");
    let me = run("GITHUB_GET_AUTHENTICATED_USER", json!({}));
    run("GITHUB_LIST_MY_REPOSITORIES", json!({"per_page": 5}));
    if let Some(login) = support::find_str(&me, "login") { run("GITHUB_FIND_USER", json!({"username": login})); }
    run("GITHUB_FIND_BRANCH", json!({"owner": O, "repo": R, "branch": "main"}));
    run("GITHUB_LIST_BRANCHES", json!({"owner": O, "repo": R, "per_page": 5}));
    let issues = run("GITHUB_LIST_ISSUES", json!({"owner": O, "repo": R, "state": "all", "per_page": 5}));
    let num = issues.pointer("/issues/0/number").or_else(|| issues.pointer("/0/number")).or_else(|| issues.pointer("/items/0/number")).and_then(Value::as_u64);
    match num {
        Some(n) => { let i = run("GITHUB_GET_ISSUE", json!({"owner": O, "repo": R, "issue_number": n})); assert!(i.to_string().contains(&n.to_string())); }
        None => { println!("LIVE FAIL GITHUB_LIST_ISSUES: no issue number in {}", issues.to_string().chars().take(200).collect::<String>()); panic!("LIST_ISSUES shape"); }
    }
    run("GITHUB_FIND_ISSUE", json!({"owner": O, "repo": R, "title": "toolhub", "state": "all"}));
    run("GITHUB_LIST_ISSUE_ASSIGNEES", json!({"owner": O, "repo": R}));
    run("GITHUB_LIST_REPOSITORY_LABELS", json!({"owner": O, "repo": R}));
    run("GITHUB_LIST_REPOSITORY_MILESTONES", json!({"owner": O, "repo": R}));
    run("GITHUB_LIST_WORKFLOWS", json!({"owner": O, "repo": R}));
    run("GITHUB_LIST_ENVIRONMENTS", json!({"owner": O, "repo": R}));
    let f = run("GITHUB_GET_FILE_CONTENT", json!({"owner": O, "repo": R, "path": "README.md"}));
    assert!(!f.is_null());
    if let Ok(id) = std::env::var("CTOX_LIVE_GITHUB_RUN_ID") {
        run("GITHUB_GET_WORKFLOW_RUN", json!({"owner": O, "repo": R, "run_id": id.parse::<u64>().unwrap()}));
    }
    println!("LIVE SKIP GITHUB_FIND_DISCUSSION (discussions disabled on metric-space-ai/ctox)");
    let bad = support::live_exec("github", AuthSchemeKind::Bearer, "GITHUB_GET_ISSUE", json!({"owner": O, "repo": R, "issue_number": 99999999})).unwrap();
    println!("LIVE ERRPATH GITHUB_GET_ISSUE: {}", bad.as_ref().err().map(|e| e.chars().take(120).collect::<String>()).unwrap_or_else(|| "UNEXPECTED SUCCESS".into()));
    assert!(bad.is_err());
    assert!(fails.is_empty(), "live failures: {fails:?}");
}
