//! Live monday.com verification through the hub exec path (personal API token).
//! Uses ONLY the test board `CTOX_TOOLHUB_LIVE_MONDAY_BOARD` (must be 5106123210, "Toolhub Test");
//! every other board is read at most (account-wide list/search reads), never written.
//! Writes create a throwaway group, text column and item on the test board and delete them again.
//! Not run: CREATE_SUBITEM (creates a permanent hidden subitems board) and SEND_NOTIFICATION
//! (notifies a person).
//! `CTOX_TOOLHUB_LIVE_MONDAY_TOKEN=… CTOX_TOOLHUB_LIVE_MONDAY_BOARD=5106123210 cargo test --test live_monday -- --ignored --nocapture`
mod support;
use ctox_toolhub::AuthSchemeKind;
use serde_json::{json, Value};
use std::process::Command;

const TEST_BOARD: &str = "5106123210";

/// Raw GraphQL for cleanup only. Token goes via a header file, never argv.
fn raw_gql(query: &str) -> Value {
    let token = std::env::var("CTOX_TOOLHUB_LIVE_MONDAY_TOKEN").unwrap();
    let hf = std::env::temp_dir().join(format!("mon-h-{}", std::process::id()));
    std::fs::write(&hf, format!("Authorization: {}\nContent-Type: application/json\nAPI-Version: 2024-10\n", token.trim())).unwrap();
    let out = Command::new("curl")
        .args(["-s", "-X", "POST", "-H", &format!("@{}", hf.display()), "https://api.monday.com/v2", "--data", &json!({"query": query}).to_string()])
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&hf);
    serde_json::from_slice(&out.stdout).unwrap_or(Value::Null)
}

#[test]
#[ignore]
fn live_monday_board_scenario() {
    scenario();
}

/// Same scenario through the hybrid spec `specs/monday.json` (16 spec + 14 Rust actions).
#[test]
#[ignore]
fn live_monday_board_scenario_via_spec() {
    support::use_spec(include_str!("../specs/monday.json"));
    scenario();
}

fn scenario() {
    let (Ok(_), Ok(board)) = (std::env::var("CTOX_TOOLHUB_LIVE_MONDAY_TOKEN"), std::env::var("CTOX_TOOLHUB_LIVE_MONDAY_BOARD")) else { return };
    assert_eq!(board, TEST_BOARD, "refusing to run against any board but the Toolhub Test board");
    let b = board.as_str();
    let tag = format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs());
    let fails: std::cell::RefCell<Vec<String>> = Default::default();
    let mut passed: std::collections::BTreeSet<String> = Default::default();
    let mut run = |tool: &str, args: Value| -> Value {
        match support::live_exec("monday", AuthSchemeKind::ApiKey, tool, args).unwrap() {
            Ok(v) => { let s = v.to_string(); println!("LIVE PASS {tool}: {}", s.chars().take(160).collect::<String>()); passed.insert(tool.into()); v }
            Err(e) => { println!("LIVE FAIL {tool}: {}", e.chars().take(300).collect::<String>()); fails.borrow_mut().push(tool.into()); Value::Null }
        }
    };
    // reads
    let me = run("MONDAY_GET_ME", json!({}));
    assert!(me["id"].is_string() || me["id"].is_number(), "GET_ME returned no id");
    run("MONDAY_GET_ACCOUNT", json!({}));
    run("MONDAY_LIST_USERS", json!({"limit": 5}));
    run("MONDAY_LIST_TEAMS", json!({}));
    run("MONDAY_LIST_WORKSPACES", json!({"limit": 5}));
    let boards = run("MONDAY_LIST_BOARDS", json!({"board_ids": [b]}));
    assert!(boards.to_string().contains("Toolhub Test"), "test board not found");
    run("MONDAY_LIST_FOLDERS", json!({"limit": 5}));
    run("MONDAY_LIST_BOARD_VIEWS", json!({"board_id": b}));
    run("MONDAY_LIST_COLUMNS", json!({"board_id": b}));
    run("MONDAY_LIST_GROUPS", json!({"board_id": b}));
    run("MONDAY_GET_COLUMN_TYPE_SCHEMA", json!({"column_type": "text"}));
    run("MONDAY_LIST_BOARD_ITEMS", json!({"board_id": b, "limit": 5}));
    run("MONDAY_GET_BOARD_VALUES", json!({"board_id": b}));
    run("MONDAY_LIST_BOARD_UPDATES", json!({"board_id": b, "limit": 5}));
    run("MONDAY_SEARCH", json!({"entity_type": "boards", "query": "Toolhub", "board_ids": [b], "limit": 5}));
    // writes on the test board only, cleaned up below
    let group = run("MONDAY_CREATE_GROUP", json!({"board_id": b, "group_name": format!("toolhub-live-{tag}")}));
    let group_id = group["id"].as_str().map(String::from);
    let col = run("MONDAY_CREATE_COLUMN", json!({"board_id": b, "column_title": format!("toolhub-{tag}"), "column_type": "text"}));
    let col_id = col["id"].as_str().map(String::from);
    let mut item_id: Option<String> = None;
    if let Some(g) = &group_id {
        let item = run("MONDAY_CREATE_BOARD_ITEM", json!({"board_id": b, "group_id": g, "item_name": format!("toolhub item {tag}")}));
        item_id = item["id"].as_str().map(String::from);
    }
    if let Some(i) = &item_id {
        run("MONDAY_GET_ITEMS", json!({"item_ids": [i]}));
        run("MONDAY_RENAME_ITEM", json!({"board_id": b, "item_id": i, "name": format!("toolhub renamed \"{tag}\"")}));
        if let Some(c) = &col_id {
            let val = format!("hello-{tag}");
            run("MONDAY_SET_ITEM_COLUMN_VALUES", json!({"board_id": b, "item_id": i, "column_values": {c: val}}));
            let cv = run("MONDAY_GET_ITEM_COLUMN_VALUES", json!({"board_id": b, "item_id": i, "column_ids": [c]}));
            if !cv.to_string().contains(&val) { println!("LIVE FAIL MONDAY_GET_ITEM_COLUMN_VALUES: value not echoed"); fails.borrow_mut().push("value echo".into()); }
            run("MONDAY_SEARCH_ITEMS_BY_COLUMN_VALUES", json!({"board_id": b, "filters": [{"column_id": c, "value": val}]}));
        }
        run("MONDAY_LIST_SUBITEMS", json!({"item_id": i}));
        let upd = run("MONDAY_CREATE_UPDATE", json!({"item_id": i, "body": format!("toolhub update {tag}")}));
        if let Some(u) = upd["id"].as_str() {
            run("MONDAY_POST_ITEM_UPDATE", json!({"parent_id": u, "body": "toolhub reply"}));
        }
        run("MONDAY_POST_ITEM_UPDATE", json!({"item_id": i, "body": "toolhub post"}));
        run("MONDAY_LIST_ITEM_UPDATES", json!({"item_id": i}));
        run("MONDAY_ARCHIVE_ITEM", json!({"item_id": i}));
    }
    // cleanup (always, even after failures)
    if let Some(i) = &item_id { println!("CLEANUP delete_item: {}", raw_gql(&format!("mutation {{ delete_item(item_id: {i}) {{ id }} }}"))); }
    if let Some(g) = &group_id { println!("CLEANUP delete_group: {}", raw_gql(&format!("mutation {{ delete_group(board_id: {b}, group_id: \"{g}\") {{ id }} }}"))); }
    if let Some(c) = &col_id { println!("CLEANUP delete_column: {}", raw_gql(&format!("mutation {{ delete_column(board_id: {b}, column_id: \"{c}\") {{ id }} }}"))); }
    drop(run);
    let fails = fails.into_inner();
    println!("LIVE SUMMARY monday: {} distinct actions passed, fails: {fails:?}", passed.len());
    assert!(fails.is_empty(), "live failures: {fails:?}");
}
