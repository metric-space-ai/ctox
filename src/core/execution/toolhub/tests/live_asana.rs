//! Live Asana verification through the hub exec path (personal access token, Bearer).
//! Reads the workspace; writes only inside the test project and deletes what it created.
//! `CTOX_TOOLHUB_LIVE_ASANA_TOKEN=… CTOX_TOOLHUB_LIVE_ASANA_WORKSPACE=… CTOX_TOOLHUB_LIVE_ASANA_PROJECT=…
//!  cargo test --test live_asana -- --ignored --nocapture`
mod support;
use ctox_toolhub::AuthSchemeKind;
use serde_json::{json, Value};

#[test]
#[ignore]
fn live_read_and_scoped_write_actions() {
    let (Ok(_), Ok(ws), Ok(project)) = (
        std::env::var("CTOX_TOOLHUB_LIVE_ASANA_TOKEN"),
        std::env::var("CTOX_TOOLHUB_LIVE_ASANA_WORKSPACE"),
        std::env::var("CTOX_TOOLHUB_LIVE_ASANA_PROJECT"),
    ) else {
        return;
    };
    let mut fails = vec![];
    let mut run = |tool: &str, args: Value| -> Value {
        match support::live_exec("asana", AuthSchemeKind::Bearer, tool, args) {
            None => Value::Null,
            Some(Ok(v)) => { println!("LIVE PASS {tool}"); v }
            Some(Err(e)) => {
                println!("LIVE FAIL {tool}: {}", e.chars().take(300).collect::<String>());
                fails.push(tool.to_string());
                Value::Null
            }
        }
    };
    let me = run("ASANA_GET_CURRENT_USER", json!({}));
    let user = support::find_str(&me, "gid").unwrap_or_default();
    run("ASANA_GET_USER", json!({"user": user}));
    run("ASANA_LIST_WORKSPACES", json!({}));
    run("ASANA_GET_WORKSPACE", json!({"workspace": ws}));
    run("ASANA_LIST_USERS", json!({"workspace": ws}));
    let teams = run("ASANA_LIST_TEAMS", json!({"workspace": ws}));
    match support::find_str(&teams, "gid") {
        Some(t) => { run("ASANA_GET_TEAM", json!({"team": t})); }
        None => println!("LIVE SKIP ASANA_GET_TEAM (no team)"),
    }
    run("ASANA_LIST_PROJECTS", json!({"workspace": ws}));
    run("ASANA_GET_PROJECT", json!({"project": project}));
    let sections = run("ASANA_LIST_SECTIONS", json!({"project": project}));
    let section = support::find_str(&sections, "gid");
    if let Some(s) = &section { run("ASANA_GET_SECTION", json!({"section": s})); }
    let tags = run("ASANA_LIST_TAGS", json!({"workspace": ws}));
    let tag = support::find_str(&tags, "gid");
    match &tag { Some(t) => { run("ASANA_GET_TAG", json!({"tag": t})); } None => println!("LIVE SKIP ASANA_GET_TAG (no tag)") }
    let tasks = run("ASANA_LIST_PROJECT_TASKS", json!({"project": project}));
    let task = support::find_str(&tasks, "gid").expect("task in test project");
    run("ASANA_GET_TASK", json!({"task": task}));
    run("ASANA_LIST_SUBTASKS", json!({"task": task}));
    run("ASANA_LIST_TASK_TAGS", json!({"task": task}));
    let stories = run("ASANA_LIST_TASK_STORIES", json!({"task": task}));
    match support::find_str(&stories, "gid") { Some(s) => { run("ASANA_GET_STORY", json!({"story": s})); } None => println!("LIVE SKIP ASANA_GET_STORY") }
    run("ASANA_SEARCH_WORKSPACE_OBJECTS", json!({"workspace": ws, "resource_type": "task", "query": "Toolhub"}));
    run("ASANA_SEARCH_TASKS", json!({"workspace": ws, "text": "Toolhub live test"}));

    let created = run("ASANA_CREATE_TASK", json!({"project": project, "name": "Toolhub Schreibtest (wird gelöscht)", "notes": "CTOX Toolhub Livetest"}));
    let new = support::find_str(&created, "gid").expect("created task gid");
    run("ASANA_UPDATE_TASK", json!({"task": new, "name": "Toolhub Schreibtest (aktualisiert)", "completed": true}));
    let sub = run("ASANA_CREATE_SUBTASK", json!({"task": new, "name": "Teilaufgabe Livetest"}));
    run("ASANA_ADD_TASK_COMMENT", json!({"task": new, "text": "Kommentar vom Toolhub-Livetest"}));
    run("ASANA_ADD_TASK_TO_PROJECT", json!({"task": new, "project": project}));
    if let Some(s) = &section { run("ASANA_MOVE_TASK_TO_SECTION", json!({"section": s, "task": new})); }
    match &tag { Some(t) => { run("ASANA_ADD_TAG_TO_TASK", json!({"task": new, "tag": t})); } None => println!("LIVE SKIP ASANA_ADD_TAG_TO_TASK (no tag)") }
    if let Some(s) = support::find_str(&sub, "gid") { run("ASANA_DELETE_TASK", json!({"task": s})); }
    run("ASANA_DELETE_TASK", json!({"task": new}));
    println!("LIVE SKIP ASANA_CREATE_SECTION (Abschnitte lassen sich über die Tools nicht wieder löschen)");
    assert!(fails.is_empty(), "live failures: {fails:?}");
}
