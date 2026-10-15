//! Live Todoist verification through the hub exec path (personal API token, Bearer).
//! Reads everything; writes only inside the test project "Toolhub Test"
//! (creates a throwaway task, updates, comments, completes, reopens, deletes it).
//! `CTOX_TOOLHUB_LIVE_TODOIST_TOKEN=… cargo test --test live_todoist -- --ignored --nocapture`
mod support;
use ctox_toolhub::AuthSchemeKind;
use serde_json::{json, Value};

fn project_id(v: &Value, name: &str) -> Option<String> {
    match v {
        Value::Object(m) if m.get("name").and_then(Value::as_str) == Some(name) => {
            m.get("id").and_then(Value::as_str).map(String::from)
        }
        Value::Object(m) => m.values().find_map(|x| project_id(x, name)),
        Value::Array(a) => a.iter().find_map(|x| project_id(x, name)),
        _ => None,
    }
}

fn scenario() {
    if std::env::var("CTOX_TOOLHUB_LIVE_TODOIST_TOKEN").is_err() {
        return;
    }
    let mut fails = vec![];
    let mut run = |tool: &str, args: Value| -> Value {
        match support::live_exec("todoist", AuthSchemeKind::Bearer, tool, args) {
            None => Value::Null,
            Some(Ok(v)) => {
                println!("LIVE PASS {tool}");
                v
            }
            Some(Err(e)) => {
                println!("LIVE FAIL {tool}: {}", e.chars().take(400).collect::<String>());
                fails.push(tool.to_string());
                Value::Null
            }
        }
    };
    run("TODOIST_GET_CURRENT_USER", json!({}));
    let projects = run("TODOIST_LIST_PROJECTS", json!({}));
    let project = project_id(&projects, "Toolhub Test").expect("project 'Toolhub Test' not found");
    run("TODOIST_GET_PROJECT", json!({"project_id": project}));
    run("TODOIST_LIST_ARCHIVED_PROJECTS", json!({}));
    let sections = run("TODOIST_LIST_SECTIONS", json!({"project_id": project}));
    match support::find_str(&sections, "id") {
        Some(s) => { run("TODOIST_GET_SECTION", json!({"section_id": s})); }
        None => println!("LIVE SKIP TODOIST_GET_SECTION (no section)"),
    }
    run("TODOIST_LIST_ARCHIVED_SECTIONS", json!({"project_id": project}));
    let labels = run("TODOIST_LIST_LABELS", json!({}));
    match support::find_str(&labels, "id") {
        Some(l) => { run("TODOIST_GET_LABEL", json!({"label_id": l})); }
        None => println!("LIVE SKIP TODOIST_GET_LABEL (no label)"),
    }
    run("TODOIST_LIST_SHARED_LABELS", json!({}));
    let found = run("TODOIST_FILTER_TASKS", json!({"query": "search: Toolhub live test"}));
    if let Some(t) = support::find_str(&found, "id") {
        run("TODOIST_GET_TASK", json!({"task_id": t}));
    }
    run("TODOIST_LIST_COMPLETED_TASKS", json!({"since": "2026-10-01T00:00:00Z", "until": "2026-10-31T00:00:00Z"}));
    run("TODOIST_LIST_COMPLETED_TASKS_BY_DUE_DATE", json!({"since": "2026-10-01T00:00:00Z", "until": "2026-10-31T00:00:00Z"}));
    run("TODOIST_LIST_ACTIVITY_LOG", json!({}));
    run("TODOIST_LIST_COMMENTS", json!({"project_id": project}));

    // Scoped writes inside "Toolhub Test" only.
    let created = run("TODOIST_CREATE_TASK", json!({"content": "Toolhub Schreibtest (wird gelöscht)", "project_id": project}));
    let task = support::find_str(&created, "id").expect("created task id");
    run("TODOIST_UPDATE_TASK", json!({"task_id": task, "content": "Toolhub Schreibtest (aktualisiert)"}));
    let c = run("TODOIST_CREATE_TASK_COMMENT", json!({"task_id": task, "content": "Kommentar vom Toolhub-Livetest"}));
    match support::find_str(&c, "id") {
        Some(cid) => { run("TODOIST_GET_COMMENT", json!({"comment_id": cid})); }
        None => println!("LIVE SKIP TODOIST_GET_COMMENT"),
    }
    run("TODOIST_LIST_COMMENTS", json!({"task_id": task}));
    run("TODOIST_COMPLETE_TASK", json!({"task_id": task}));
    run("TODOIST_REOPEN_TASK", json!({"task_id": task}));
    run("TODOIST_DELETE_TASK", json!({"task_id": task}));
    for t in ["TODOIST_MOVE_TASK", "TODOIST_QUICK_ADD_TASK", "TODOIST_CREATE_PROJECT", "TODOIST_UPDATE_PROJECT",
              "TODOIST_UNARCHIVE_PROJECT", "TODOIST_CREATE_SECTION", "TODOIST_UPDATE_SECTION"] {
        println!("LIVE SKIP {t} (würde dauerhafte Objekte außerhalb der Testaufgabe anlegen/ändern)");
    }
    assert!(fails.is_empty(), "live failures: {fails:?}");
}

#[test]
#[ignore]
fn live_read_and_scoped_write_actions() {
    scenario();
}

/// Same scenario through the hot-plug spec `specs/todoist.json` (SpecAdapter).
#[test]
#[ignore]
fn live_read_and_scoped_write_actions_via_spec() {
    support::use_spec(include_str!("../specs/todoist.json"));
    scenario();
}
