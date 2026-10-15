//! Live GitLab verification through the hub exec path against the private test
//! project `CTOX_TOOLHUB_LIVE_GITLAB_PROJECT` (read + write, with cleanup).
//! The adapter has no branch/file/delete actions, so fixtures (commit on main,
//! feature branch) and cleanup (delete issue, label, branch) use the REST API
//! directly via curl.
//! `source /workspace/toolhub-live-env.sh && cargo test --test live_gitlab -- --ignored --nocapture`
mod support;
use ctox_toolhub::AuthSchemeKind;
use serde_json::{json, Value};
use std::process::Command;

const API: &str = "https://gitlab.com/api/v4";

fn enc(s: &str) -> String { s.replace('/', "%2F") }

/// Raw REST call for fixtures/cleanup. Token goes via a header file, never argv.
fn raw(method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
    let token = std::env::var("CTOX_TOOLHUB_LIVE_GITLAB_TOKEN").unwrap();
    let hf = std::env::temp_dir().join(format!("gl-h-{}", std::process::id()));
    std::fs::write(&hf, format!("PRIVATE-TOKEN: {}\nContent-Type: application/json\n", token.trim())).unwrap();
    let mut c = Command::new("curl");
    c.args(["-s", "-X", method, "-H", &format!("@{}", hf.display()), "-w", "\n%{http_code}", &format!("{API}{path}")]);
    if let Some(b) = body { c.args(["--data", &b.to_string()]); }
    let out = c.output().unwrap();
    let _ = std::fs::remove_file(&hf);
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    let (b, code) = s.rsplit_once('\n').unwrap_or(("", "0"));
    (code.trim().parse().unwrap_or(0), serde_json::from_str(b).unwrap_or(Value::Null))
}

#[test]
#[ignore]
fn live_gitlab_read_write() {
    if std::env::var("CTOX_TOOLHUB_LIVE_GITLAB_TOKEN").is_err() { return; }
    let p = std::env::var("CTOX_TOOLHUB_LIVE_GITLAB_PROJECT").unwrap_or("metric-space-toolhub/toolhub-test".into());
    let grp = std::env::var("CTOX_TOOLHUB_LIVE_GITLAB_GROUP").unwrap_or("metric-space-toolhub".into());
    let pe = enc(&p);
    let tag = format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs());
    let mut fails: Vec<String> = vec![];
    let mut passed: Vec<String> = vec![];
    let mut run = |tool: &str, args: Value| -> Value {
        match support::live_exec("gitlab", AuthSchemeKind::Bearer, tool, args).unwrap() {
            Ok(v) => { println!("LIVE PASS {tool}: {}", v.to_string().chars().take(150).collect::<String>()); passed.push(tool.into()); v }
            Err(e) => { println!("LIVE FAIL {tool}: {}", e.chars().take(300).collect::<String>()); fails.push(tool.into()); Value::Null }
        }
    };

    // Fixtures: make sure main exists, then a feature branch with one commit.
    let (c, _) = raw("GET", &format!("/projects/{pe}/repository/branches/main"), None);
    if c == 404 {
        let (c, b) = raw("POST", &format!("/projects/{pe}/repository/commits"), Some(json!({"branch":"main","commit_message":"init","actions":[{"action":"create","file_path":"README.md","content":"toolhub live test\n"}]})));
        assert!(c == 201, "init main: {c} {b}");
    }
    let br = format!("toolhub-live-{tag}");
    let (c, b) = raw("POST", &format!("/projects/{pe}/repository/commits"), Some(json!({"branch":br,"start_branch":"main","commit_message":"live test","actions":[{"action":"create","file_path":format!("live/{tag}.txt"),"content":"x\n"}]})));
    assert!(c == 201, "feature branch: {c} {b}");

    let me = run("GITLAB_GET_CURRENT_USER", json!({}));
    let uid = me.get("id").and_then(Value::as_u64);
    let uname = me.get("username").and_then(Value::as_str).unwrap_or("ctox-toolhub-test").to_string();
    run("GITLAB_LIST_USERS", json!({"username": uname}));
    run("GITLAB_GET_USER_STATUS", json!({}));
    run("GITLAB_GET_USER_STATUS", json!({"user": uname}));
    run("GITLAB_LIST_PROJECTS", json!({"per_page": 5}));
    let proj = run("GITLAB_GET_PROJECT", json!({"project_id": p}));
    assert_eq!(proj.get("path_with_namespace").and_then(Value::as_str), Some(p.as_str()));
    run("GITLAB_LIST_PROJECT_MEMBERS", json!({"project_id": p}));
    let gl = run("GITLAB_LIST_GROUPS", json!({"owned": "true"})); println!("groups owned contain test group: {}", gl.to_string().contains(&grp));
    run("GITLAB_GET_GROUP", json!({"group_id": grp}));
    let br_list = run("GITLAB_LIST_BRANCHES", json!({"project_id": p, "search": "toolhub-live"}));
    assert!(br_list.to_string().contains(&br), "new branch not listed");
    run("GITLAB_LIST_COMMITS", json!({"project_id": p, "ref_name": br}));
    run("GITLAB_LIST_PIPELINES", json!({"project_id": p}));
    run("GITLAB_LIST_MILESTONES", json!({"project_id": p}));

    // Labels
    let lname = format!("toolhub-{tag}");
    run("GITLAB_CREATE_LABEL", json!({"project_id": p, "name": lname, "color": "#1f75cb", "description": "live"}));
    run("GITLAB_GET_LABEL", json!({"project_id": p, "label_id": lname}));
    let ll = run("GITLAB_LIST_LABELS", json!({"project_id": p, "search": "toolhub-"}));
    assert!(ll.to_string().contains(&lname), "label not listed");

    // Issues + notes
    let mut ia = json!({"project_id": p, "title": format!("toolhub live {tag}"), "description": "d", "labels": [lname]});
    if let Some(u) = uid { ia["assignee_ids"] = json!([u]); }
    let iss = run("GITLAB_CREATE_ISSUE", ia);
    let iid = iss.get("iid").and_then(Value::as_u64);
    if let Some(iid) = iid {
        let g = run("GITLAB_GET_ISSUE", json!({"project_id": p, "issue_iid": iid}));
        assert!(g.to_string().contains(&lname));
        run("GITLAB_UPDATE_ISSUE", json!({"project_id": p, "issue_iid": iid, "title": format!("toolhub live {tag} upd"), "remove_labels": [lname], "due_date": "2030-01-01"}));
        run("GITLAB_LIST_ISSUES", json!({"project_id": p, "search": tag, "state": "all"}));
        run("GITLAB_GET_ISSUE_TIME_STATS", json!({"project_id": p, "issue_iid": iid}));
        let n = run("GITLAB_CREATE_ISSUE_NOTE", json!({"project_id": p, "issue_iid": iid, "body": "live note"}));
        if let Some(nid) = n.get("id").and_then(Value::as_u64) {
            run("GITLAB_GET_ISSUE_NOTE", json!({"project_id": p, "issue_iid": iid, "note_id": nid}));
            run("GITLAB_LIST_ISSUE_NOTES", json!({"project_id": p, "issue_iid": iid}));
            run("GITLAB_DELETE_ISSUE_NOTE", json!({"project_id": p, "issue_iid": iid, "note_id": nid}));
        }
        run("GITLAB_UPDATE_ISSUE", json!({"project_id": p, "issue_iid": iid, "state_event": "close"}));
    }

    // Merge requests
    let mr = run("GITLAB_CREATE_MERGE_REQUEST", json!({"project_id": p, "source_branch": br, "target_branch": "main", "title": format!("toolhub live {tag}"), "description": "d"}));
    let mid = mr.get("iid").and_then(Value::as_u64);
    if let Some(m) = mid {
        run("GITLAB_GET_MERGE_REQUEST", json!({"project_id": p, "merge_request_iid": m}));
        run("GITLAB_GET_MERGE_REQUEST_APPROVALS", json!({"project_id": p, "merge_request_iid": m}));
        std::thread::sleep(std::time::Duration::from_secs(3));
        run("GITLAB_LIST_MERGE_REQUEST_COMMITS", json!({"project_id": p, "merge_request_iid": m}));
        run("GITLAB_UPDATE_MERGE_REQUEST", json!({"project_id": p, "merge_request_iid": m, "title": format!("toolhub live {tag} upd"), "add_labels": [lname]}));
        run("GITLAB_LIST_MERGE_REQUESTS", json!({"project_id": p, "state": "opened"}));
        run("GITLAB_LIST_MERGE_REQUESTS", json!({"scope": "created_by_me"}));
        run("GITLAB_UPDATE_MERGE_REQUEST", json!({"project_id": p, "merge_request_iid": m, "state_event": "close"}));
    }

    // Error path
    let bad = support::live_exec("gitlab", AuthSchemeKind::Bearer, "GITLAB_GET_ISSUE", json!({"project_id": p, "issue_iid": 99999999})).unwrap();
    assert!(bad.is_err(), "missing issue should fail");

    // Cleanup
    if let Some(iid) = iid { println!("cleanup issue {}", raw("DELETE", &format!("/projects/{pe}/issues/{iid}"), None).0); }
    if let Some(m) = mid { println!("cleanup mr {}", raw("DELETE", &format!("/projects/{pe}/merge_requests/{m}"), None).0); }
    println!("cleanup label {}", raw("DELETE", &format!("/projects/{pe}/labels/{}", enc(&lname)), None).0);
    println!("cleanup branch {}", raw("DELETE", &format!("/projects/{pe}/repository/branches/{}", enc(&br)), None).0);

    passed.sort(); passed.dedup();
    println!("LIVE SUMMARY gitlab: {} distinct actions passed, fails: {fails:?}", passed.len());
    assert!(fails.is_empty(), "live failures: {fails:?}");
}
