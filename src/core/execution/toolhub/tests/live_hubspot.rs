//! Live HubSpot verification through the hub exec path (read-only private app / service key).
//! `CTOX_TOOLHUB_LIVE_HUBSPOT_TOKEN=… cargo test --test live_hubspot -- --ignored --nocapture`
//! Tools whose scope the test key lacks (403 MISSING_SCOPES) are reported, not failed.
mod support;
use ctox_toolhub::AuthSchemeKind;
use serde_json::{json, Value};

const CONTACT: &str = "890251478207";

#[test]
#[ignore]
fn live_all_read_actions() {
    scenario();
}

/// Same scenario through the hot-plug spec `specs/hubspot.json` (hybrid: 3 actions stay Rust).
#[test]
#[ignore]
fn live_all_read_actions_via_spec() {
    support::use_spec(include_str!("../specs/hubspot.json"));
    scenario();
}

fn scenario() {
    if std::env::var("CTOX_TOOLHUB_LIVE_HUBSPOT_TOKEN").is_err() { return; }
    let mut fails = vec![];
    let mut run = |tool: &str, args: Value| -> Value {
        match support::live_exec("hubspot", AuthSchemeKind::Bearer, tool, args).unwrap() {
            Ok(v) => { let s = v.to_string(); println!("LIVE PASS {tool}: {}", &s[..s.len().min(180)]); v }
            Err(e) if e.contains("MISSING_SCOPES") => { println!("LIVE SCOPE {tool}: test key lacks scope"); Value::Null }
            Err(e) => { println!("LIVE FAIL {tool}: {}", e.chars().take(400).collect::<String>()); fails.push(tool.to_string()); Value::Null }
        }
    };
    let c = run("HUBSPOT_GET_CONTACT", json!({"contact_id": CONTACT}));
    assert!(c.to_string().contains(CONTACT), "test contact not returned");
    let f = run("HUBSPOT_FIND_CONTACT", json!({"property_name": "lastname", "value": "Livetest"}));
    assert!(f.to_string().contains(CONTACT), "FIND_CONTACT did not find test contact");
    let s = run("HUBSPOT_SEARCH_OBJECTS", json!({"object_type": "contacts", "query": "Toolhub"}));
    assert!(s.to_string().contains(CONTACT), "SEARCH_OBJECTS did not find test contact");
    run("HUBSPOT_LIST_OBJECT_PROPERTIES", json!({"object_type": "contacts"}));
    let comps = run("HUBSPOT_SEARCH_OBJECTS", json!({"object_type": "companies", "limit": 1}));
    run("HUBSPOT_FIND_COMPANY", json!({"property_name": "name", "value": "Toolhub"}));
    if let Some(id) = support::find_str(&comps, "id") { run("HUBSPOT_GET_COMPANY", json!({"company_id": id})); } else { println!("LIVE SKIP HUBSPOT_GET_COMPANY (no company)"); }
    let deals = run("HUBSPOT_SEARCH_OBJECTS", json!({"object_type": "deals", "limit": 1}));
    run("HUBSPOT_FIND_DEAL", json!({"property_name": "dealname", "value": "Toolhub"}));
    if let Some(id) = support::find_str(&deals, "id") { run("HUBSPOT_GET_DEAL", json!({"deal_id": id})); } else { println!("LIVE SKIP HUBSPOT_GET_DEAL (no deal)"); }
    run("HUBSPOT_FIND_TICKET", json!({"property_name": "subject", "value": "Toolhub"}));
    println!("LIVE SKIP HUBSPOT_GET_TICKET (needs tickets scope + ticket id)");
    let owners = run("HUBSPOT_LIST_OWNERS", json!({}));
    if let Some(id) = support::find_str(&owners, "id") { run("HUBSPOT_GET_OWNER_BY_ID", json!({"owner_id": id})); }
    if let Some(e) = support::find_str(&owners, "email") { run("HUBSPOT_GET_OWNER_BY_EMAIL", json!({"email": e})); }
    let pipes = run("HUBSPOT_LIST_PIPELINES", json!({"object_type": "deals"}));
    if let Some(p) = support::find_str(&pipes, "id") {
        let stages = run("HUBSPOT_LIST_PIPELINE_STAGES", json!({"object_type": "deals", "pipeline_id": p}));
        if let Some(st) = stages.pointer("/results/0/id").or_else(|| stages.pointer("/stages/0/id")).or_else(|| stages.pointer("/0/id")).and_then(Value::as_str) {
            run("HUBSPOT_GET_PIPELINE_STAGE", json!({"object_type": "deals", "pipeline_id": p, "stage_id": st}));
        } else { println!("LIVE SKIP HUBSPOT_GET_PIPELINE_STAGE (stage id not found in {})", stages.to_string().chars().take(120).collect::<String>()); }
    }
    run("HUBSPOT_LIST_ASSOCIATION_TYPES", json!({"from_object_type": "contacts", "to_object_type": "companies"}));
    run("HUBSPOT_FIND_ASSOCIATIONS", json!({"from_object_id": CONTACT, "from_object_type": "contacts", "to_object_type": "companies"}));
    run("HUBSPOT_LIST_STATIC_LISTS", json!({}));
    let bad = support::live_exec("hubspot", AuthSchemeKind::Bearer, "HUBSPOT_GET_CONTACT", json!({"contact_id": "1"})).unwrap();
    println!("LIVE ERRPATH HUBSPOT_GET_CONTACT: {}", bad.as_ref().err().map(|e| e.chars().take(140).collect::<String>()).unwrap_or_else(|| "UNEXPECTED SUCCESS".into()));
    assert!(bad.is_err());
    assert!(fails.is_empty(), "live failures: {fails:?}");
}
