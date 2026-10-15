//! Live Slack verification through the hub exec path.
//! `CTOX_TOOLHUB_LIVE_SLACK_TOKEN=… cargo test --test live_slack -- --ignored --nocapture`
//! Tools whose scope the token lacks (`missing_scope`) are reported, not failed.
mod support;
use ctox_toolhub::AuthSchemeKind;
use serde_json::{json, Value};

#[test]
#[ignore]
fn live_all_read_actions() {
    if std::env::var("CTOX_TOOLHUB_LIVE_SLACK_TOKEN").is_err() { return; }
    let mut fails = vec![];
    let mut run = |tool: &str, args: Value| -> Value {
        match support::live_exec("slack", AuthSchemeKind::Bearer, tool, args).unwrap() {
            Ok(v) => { let s = v.to_string(); println!("LIVE PASS {tool}: {}", s.chars().take(160).collect::<String>()); v }
            Err(e) if e.contains("missing_scope") || e.contains("not_allowed_token_type") => { println!("LIVE SCOPE {tool}: {}", e.chars().take(120).collect::<String>()); Value::Null }
            Err(e) => { println!("LIVE FAIL {tool}: {}", e.chars().take(300).collect::<String>()); fails.push(tool.to_string()); Value::Null }
        }
    };
    let chans = run("SLACK_LIST_CHANNELS", json!({}));
    let chan = chans.to_string();
    let ch = serde_json::from_str::<Value>(&chan).ok().and_then(|v| {
        v["channels"].as_array().and_then(|a| a.iter().find(|c| c["name"] == "toolhub-test")).and_then(|c| c["id"].as_str().map(String::from))
    });
    let Some(ch) = ch else { panic!("#toolhub-test not found in SLACK_LIST_CHANNELS") };
    run("SLACK_GET_CHANNEL_INFO", json!({"channel": ch}));
    let hist = run("SLACK_GET_HISTORY", json!({"channel": ch}));
    let msg = hist["messages"].as_array().and_then(|a| a.iter().find(|m| m["text"].as_str().is_some_and(|t| t.contains("Toolhub live test")))).cloned();
    if let Some(m) = &msg {
        let ts = m["ts"].as_str().unwrap();
        run("SLACK_GET_MESSAGE", json!({"channel": ch, "ts": ts}));
        run("SLACK_GET_THREAD", json!({"channel": ch, "thread_ts": ts}));
        run("SLACK_GET_MESSAGE_PERMALINK", json!({"channel": ch, "message_ts": ts}));
    } else { println!("LIVE NOTE history has no 'Toolhub live test' message: {}", hist.to_string().chars().take(200).collect::<String>()); panic!("history content check failed"); }
    let members = run("SLACK_LIST_CHANNEL_MEMBERS", json!({"channel": ch}));
    run("SLACK_LIST_USER_CONVERSATIONS", json!({}));
    run("SLACK_LIST_SCHEDULED_MESSAGES", json!({}));
    let users = run("SLACK_LIST_USERS", json!({}));
    let uid = members["members"].as_array().and_then(|a| a.first()).and_then(Value::as_str).map(String::from).or_else(|| support::find_str(&users, "id"));
    if let Some(u) = uid { run("SLACK_GET_USER", json!({"user": u})); }
    match support::find_str(&users, "email") { Some(e) => { run("SLACK_FIND_USER_BY_EMAIL", json!({"email": e})); } None => println!("LIVE SKIP SLACK_FIND_USER_BY_EMAIL (no email visible; needs users:read.email)") }
    let groups = run("SLACK_LIST_USER_GROUPS", json!({}));
    match support::find_str(&groups, "id") { Some(g) => { run("SLACK_LIST_USER_GROUP_USERS", json!({"usergroup": g})); } None => println!("LIVE SKIP SLACK_LIST_USER_GROUP_USERS (no user group)") }
    let s = run("SLACK_SEARCH_MESSAGES", json!({"query": "Toolhub live test"}));
    if !s.is_null() && !s.to_string().contains("Toolhub live test") { println!("LIVE NOTE search returned no hit (Slack search index can lag)"); }
    run("SLACK_SEARCH_ALL", json!({"query": "Toolhub"}));
    run("SLACK_LIST_FILES", json!({}));
    let bad = support::live_exec("slack", AuthSchemeKind::Bearer, "SLACK_GET_CHANNEL_INFO", json!({"channel": "C00000000"})).unwrap();
    println!("LIVE ERRPATH SLACK_GET_CHANNEL_INFO: {}", bad.as_ref().err().map(|e| e.chars().take(120).collect::<String>()).unwrap_or_else(|| "UNEXPECTED SUCCESS".into()));
    assert!(bad.is_err());
    assert!(fails.is_empty(), "live failures: {fails:?}");
}
