//! Live Calendly verification through the hot-plug spec `specs/calendly.json`
//! (there is no compiled-adapter path for this build; the spec replaces it).
//! Reads everything; writes only a single-use scheduling link and a throwaway
//! webhook to https://example.com/ctox-test that is deleted right away.
//! `CTOX_TOOLHUB_LIVE_CALENDLY_TOKEN=… cargo test --test live_calendly -- --ignored --nocapture`
mod support;
use ctox_toolhub::AuthSchemeKind;
use serde_json::{json, Value};

fn scenario() {
    if std::env::var("CTOX_TOOLHUB_LIVE_CALENDLY_TOKEN").is_err() {
        return;
    }
    let mut fails = vec![];
    let mut run = |tool: &str, args: Value| -> Value {
        match support::live_exec("calendly", AuthSchemeKind::Bearer, tool, args) {
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
    let me = run("CALENDLY_GET_CURRENT_USER", json!({}));
    let user = me["uri"].as_str().expect("user uri").to_string();
    let org = me["current_organization"].as_str().expect("org uri").to_string();
    // Bare UUID form on purpose (uri_tail).
    run("CALENDLY_GET_USER", json!({"user": user.rsplit('/').next().unwrap()}));
    let types = run("CALENDLY_LIST_EVENT_TYPES", json!({"user": user, "count": 10}));
    let et = types["collection"][0]["uri"].as_str().expect("an event type").to_string();
    run("CALENDLY_LIST_EVENT_TYPES", json!({"organization": org}));
    run("CALENDLY_GET_EVENT_TYPE", json!({"event_type": et}));
    let events = run("CALENDLY_LIST_SCHEDULED_EVENTS", json!({"user": user, "min_start_time": "2026-01-01T00:00:00Z", "sort": "start_time:desc"}));
    match events["collection"][0]["uri"].as_str() {
        Some(ev) => {
            run("CALENDLY_GET_SCHEDULED_EVENT", json!({"event": ev}));
            let inv = run("CALENDLY_LIST_EVENT_INVITEES", json!({"event": ev}));
            if let Some(i) = inv["collection"][0]["uri"].as_str() {
                run("CALENDLY_GET_EVENT_INVITEE", json!({"event": ev, "invitee": i}));
            }
        }
        None => println!("LIVE LIST-ONLY CALENDLY_GET_SCHEDULED_EVENT / LIST_EVENT_INVITEES / GET_EVENT_INVITEE / CANCEL_EVENT (no booked events)"),
    }
    let sch = run("CALENDLY_LIST_AVAILABILITY_SCHEDULES", json!({"user": user}));
    if let Some(s) = sch["schedules"][0]["uri"].as_str() {
        run("CALENDLY_GET_AVAILABILITY_SCHEDULE", json!({"schedule": s}));
    }
    run("CALENDLY_LIST_USER_BUSY_TIMES", json!({"user": user, "start_time": "2026-12-01T00:00:00Z", "end_time": "2026-12-07T00:00:00Z"}));
    run("CALENDLY_CREATE_SCHEDULING_LINK", json!({"event_type": et}));
    run("CALENDLY_LIST_WEBHOOK_SUBSCRIPTIONS", json!({"organization": org, "scope": "user", "user": user}));
    let wh = run("CALENDLY_CREATE_WEBHOOK_SUBSCRIPTION", json!({
        "url": "https://example.com/ctox-test", "events": ["invitee.created", "invitee.canceled"],
        "organization": org, "scope": "user", "user": user}));
    if let Some(w) = wh["uri"].as_str() {
        run("CALENDLY_GET_WEBHOOK_SUBSCRIPTION", json!({"webhook": w}));
        run("CALENDLY_DELETE_WEBHOOK_SUBSCRIPTION", json!({"webhook": w}));
    }
    assert!(fails.is_empty(), "live failures: {fails:?}");
}

#[test]
#[ignore]
fn live_read_and_scoped_write_actions_via_spec() {
    support::use_spec(include_str!("../specs/calendly.json"));
    scenario();
}
