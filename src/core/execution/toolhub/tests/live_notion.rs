//! Live Notion verification through the hub exec path. Reads the shared test page;
//! writes only a throwaway child page under it, which is archived at the end.
//! `CTOX_TOOLHUB_LIVE_NOTION_TOKEN=… cargo test --test live_notion -- --ignored --nocapture`
mod support;
use ctox_toolhub::AuthSchemeKind;
use serde_json::{json, Value};

const PAGE: &str = "3f94d550-afa5-80ff-950e-f02a84267dd9";

#[test]
#[ignore]
fn live_all_read_actions() {
    scenario();
}

/// Same scenario through the hot-plug hybrid spec `specs/notion.json`.
#[test]
#[ignore]
fn live_all_read_actions_via_spec() {
    support::use_spec(include_str!("../specs/notion.json"));
    scenario();
}

fn scenario() {
    if std::env::var("CTOX_TOOLHUB_LIVE_NOTION_TOKEN").is_err() { return; }
    let mut fails = vec![];
    let mut run = |tool: &str, args: Value| -> Value {
        match support::live_exec("notion", AuthSchemeKind::Bearer, tool, args).unwrap() {
            Ok(v) => { let s = v.to_string(); println!("LIVE PASS {tool}: {}", &s[..s.len().min(200)]); v }
            Err(e) => { println!("LIVE FAIL {tool}: {}", e.chars().take(400).collect::<String>()); fails.push(tool.to_string()); Value::Null }
        }
    };
    let search = run("NOTION_SEARCH", json!({"query": "Toolhub live test"}));
    assert!(search.to_string().contains("3f94d550"), "shared page not found via search");
    let page = run("NOTION_GET_PAGE", json!({"page_id": PAGE}));
    assert!(!page.is_null());
    run("NOTION_GET_BLOCK_CHILDREN", json!({"block_id": PAGE}));
    run("NOTION_GET_PAGE_COMMENTS", json!({"page_id": PAGE}));
    let users = run("NOTION_LIST_USERS", json!({}));
    match support::find_str(&users, "id") { Some(u) => { run("NOTION_GET_USER", json!({"user_id": u})); } None => println!("LIVE SKIP NOTION_GET_USER") }
    let dbs = run("NOTION_LIST_DATABASES", json!({}));
    if let Some(db) = support::find_str(&dbs, "id") {
        run("NOTION_GET_DATABASE", json!({"database_id": db}));
        run("NOTION_QUERY_DATABASE", json!({"database_id": db}));
        run("NOTION_FIND_DATABASE_ITEM", json!({"database_id": db}));
    } else { println!("LIVE SKIP NOTION_GET_DATABASE/QUERY_DATABASE/FIND_DATABASE_ITEM (no shared database)"); }
    // Writes: a throwaway child page under the test page only; archived (= Notion's delete) at the end.
    // The current test integration has read content only: a 403 restricted_resource skips the block.
    let child = match support::live_exec("notion", AuthSchemeKind::Bearer, "NOTION_CREATE_PAGE", json!({"parent_page_id": PAGE, "title": "toolhub live tmp", "content": "temporary"})).unwrap() {
        Ok(v) => { println!("LIVE PASS NOTION_CREATE_PAGE"); v }
        Err(e) if e.contains("restricted_resource") => { println!("LIVE SKIP writes (integration lacks insert content: {})", e.chars().take(80).collect::<String>()); Value::Null }
        Err(e) => panic!("LIVE FAIL NOTION_CREATE_PAGE: {}", e.chars().take(300).collect::<String>()),
    };
    if let Some(cid) = child["id"].as_str().map(String::from) {
        let app = run("NOTION_APPEND_TO_PAGE", json!({"block_id": cid, "content": "block to delete"}));
        match app.pointer("/results/0/id").and_then(Value::as_str) {
            Some(b) => { run("NOTION_DELETE_BLOCK", json!({"block_id": b})); }
            None => println!("LIVE SKIP NOTION_DELETE_BLOCK (no appended block id)"),
        }
        let arch = run("NOTION_ARCHIVE_PAGE", json!({"page_id": cid, "archived": true}));
        assert_eq!(arch["archived"], json!(true), "child page not archived");
    }
    // Error path: unknown page must surface as a provider error, not success.
    let bad = support::live_exec("notion", AuthSchemeKind::Bearer, "NOTION_GET_PAGE", json!({"page_id": "00000000-0000-0000-0000-000000000000"})).unwrap();
    println!("LIVE ERRPATH NOTION_GET_PAGE: {}", bad.as_ref().err().map(|e| e.chars().take(160).collect::<String>()).unwrap_or_else(|| "UNEXPECTED SUCCESS".into()));
    assert!(bad.is_err());
    assert!(fails.is_empty(), "live failures: {fails:?}");
}
