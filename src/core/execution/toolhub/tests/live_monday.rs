//! Live monday.com verification through the hub exec path against the test board
//! `CTOX_TOOLHUB_LIVE_MONDAY_BOARD` (read + write). Everything created here
//! (group, column, items, subitem, updates) is deleted again via raw GraphQL.
//! `source /workspace/toolhub-live-env.sh && cargo test --test live_monday -- --ignored --nocapture`
mod support;
use ctox_toolhub::AuthSchemeKind;
use serde_json::{json, Value};
use std::process::Command;

/// Raw GraphQL for cleanup only. Token goes via a header file, never argv.
fn gql(query: &str) -> Value {
    let token = std::env::var("CTOX_TOOLHUB_LIVE_MONDAY_TOKEN").unwrap();
    let hf = std::env::temp_dir().join(format!("mon-h-{}", std::process::id()));
    std::fs::write(&hf, format!("Authorization: {}\nContent-Type: application/json\nAPI-Version: 2024-10\n", token.trim())).unwrap();
    let out = Command::new("curl").args(["-s", "-X", "POST", "-H", &format!("@{}", hf.display()), "https://api.monday.com/v2", "--data", &json!({"query": query}).to_string()]).output().unwrap();
    let _ = std::fs::remove_file(&hf);
    serde_json::from_slice(&out.stdout).unwrap_or(Value::Null)
}

fn id_of(v: &Value) -> Option<String> {
    match v.get("id")? { Value::String(s) => Some(s.clone()), Value::Number(n) => Some(n.to_string()), _ => None }
}

#[test]
#[ignore]
fn live_monday_read_write() {
    if std::env::var("CTOX_TOOLHUB_LIVE_MONDAY_TOKEN").is_err() { return; }
    let b = std::env::var("CTOX_TOOLHUB_LIVE_MONDAY_BOARD").unwrap_or("5106123210".into());
    let tag = format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs());
    let mut fails: Vec<String> = vec![];
    let mut passed: Vec<String> = vec![];
    let mut run = |tool: &str, args: Value| -> Value {
        match support::live_exec("monday", AuthSchemeKind::ApiKey, tool, args).unwrap() {
            Ok(v) => { println!("LIVE PASS {tool}: {}", v.to_string().chars().take(160).collect::<String>()); passed.push(tool.into()); v }
            Err(e) => { println!("LIVE FAIL {tool}: {}", e.chars().take(400).collect::<String>()); fails.push(tool.into()); Value::Null }
        }
    };

    let me = run("MONDAY_GET_ME", json!({}));
    let uid = id_of(&me);
    run("MONDAY_GET_ACCOUNT", json!({}));
    run("MONDAY_LIST_USERS", json!({"limit": 5}));
    run("MONDAY_LIST_TEAMS", json!({}));
    run("MONDAY_LIST_WORKSPACES", json!({}));
    let boards = run("MONDAY_LIST_BOARDS", json!({"board_ids": [b]}));
    assert!(boards.to_string().contains("Toolhub Test"), "test board not found");
    run("MONDAY_LIST_BOARD_VIEWS", json!({"board_id": b}));
    run("MONDAY_LIST_FOLDERS", json!({}));
    run("MONDAY_GET_COLUMN_TYPE_SCHEMA", json!({"column_type": "text"}));

    // Writes
    let grp = run("MONDAY_CREATE_GROUP", json!({"board_id": b, "group_name": format!("toolhub {tag}")}));
    let gid = id_of(&grp);
    let col = run("MONDAY_CREATE_COLUMN", json!({"board_id": b, "column_title": format!("th {tag}"), "column_type": "text"}));
    let cid = id_of(&col);
    run("MONDAY_LIST_GROUPS", json!({"board_id": b}));
    run("MONDAY_LIST_COLUMNS", json!({"board_id": b}));
    let mut ia = json!({"board_id": b, "item_name": format!("toolhub item {tag}")});
    if let Some(g) = &gid { ia["group_id"] = json!(g); }
    let item = run("MONDAY_CREATE_BOARD_ITEM", ia);
    let iid = id_of(&item);
    let mut sid = None;
    if let Some(i) = &iid {
        run("MONDAY_RENAME_ITEM", json!({"board_id": b, "item_id": i, "name": format!("toolhub item {tag} upd")}));
        if let Some(c) = &cid {
            run("MONDAY_SET_ITEM_COLUMN_VALUES", json!({"board_id": b, "item_id": i, "column_values": {c.as_str(): format!("val{tag}")}}));
            std::thread::sleep(std::time::Duration::from_secs(2));
            let s = run("MONDAY_SEARCH_ITEMS_BY_COLUMN_VALUES", json!({"board_id": b, "filters": [{"column_id": c, "value": format!("val{tag}")}]}));
            println!("search by column found item: {}", s.to_string().contains(i.as_str()));
        }
        run("MONDAY_GET_ITEMS", json!({"item_ids": [i]}));
        run("MONDAY_GET_ITEM_COLUMN_VALUES", json!({"board_id": b, "item_id": i}));
        run("MONDAY_GET_BOARD_VALUES", json!({"board_id": b}));
        let lb = run("MONDAY_LIST_BOARD_ITEMS", json!({"board_id": b}));
        assert!(lb.to_string().contains(i.as_str()), "item not in board list");
        if let Some(g) = &gid { run("MONDAY_LIST_BOARD_ITEMS", json!({"board_id": b, "group_id": g})); }
        let sub = run("MONDAY_CREATE_SUBITEM", json!({"parent_item_id": i, "item_name": "sub"}));
        sid = id_of(&sub);
        run("MONDAY_LIST_SUBITEMS", json!({"item_id": i}));
        let up = run("MONDAY_CREATE_UPDATE", json!({"item_id": i, "body": "live update"}));
        if let Some(u) = id_of(&up) {
            run("MONDAY_POST_ITEM_UPDATE", json!({"parent_id": u, "body": "live reply"}));
        }
        run("MONDAY_POST_ITEM_UPDATE", json!({"item_id": i, "body": "live post"}));
        run("MONDAY_LIST_ITEM_UPDATES", json!({"item_id": i}));
        run("MONDAY_LIST_BOARD_UPDATES", json!({"board_id": b}));
        if let Some(u) = &uid { run("MONDAY_SEND_NOTIFICATION", json!({"user_id": u, "target_id": i, "target_type": "item", "text": "toolhub live test"})); }
        run("MONDAY_SEARCH", json!({"entity_type": "boards", "query": "Toolhub"}));
        run("MONDAY_ARCHIVE_ITEM", json!({"item_id": i}));
    }

    // Cleanup (raw GraphQL; never touches other boards)
    if let Some(s) = &sid { println!("cleanup subitem {}", gql(&format!("mutation {{ delete_item(item_id: {s}) {{ id }} }}"))); }
    if let Some(i) = &iid { println!("cleanup item {}", gql(&format!("mutation {{ delete_item(item_id: {i}) {{ id }} }}"))); }
    if let Some(c) = &cid { println!("cleanup column {}", gql(&format!("mutation {{ delete_column(board_id: {b}, column_id: \"{c}\") {{ id }} }}"))); }
    if let Some(g) = &gid { println!("cleanup group {}", gql(&format!("mutation {{ delete_group(board_id: {b}, group_id: \"{g}\") {{ id }} }}"))); }

    passed.sort(); passed.dedup();
    println!("LIVE SUMMARY monday: {} distinct actions passed, fails: {fails:?}", passed.len());
    assert!(fails.is_empty(), "live failures: {fails:?}");
}
