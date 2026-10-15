//! Live Trello verification through the hot-plug spec `specs/trello.json`
//! (auth scheme `oauth_header`, credential `key:token`, nothing in the URL).
//! Everything happens on a dedicated throwaway board that is deleted at the end.
//! `CTOX_TOOLHUB_LIVE_TRELLO_KEY=… CTOX_TOOLHUB_LIVE_TRELLO_TOKEN=… cargo test --test live_trello -- --ignored --nocapture`
mod support;
use ctox_toolhub::AuthSchemeKind;
use serde_json::{json, Value};

fn scenario() {
    if std::env::var("CTOX_TOOLHUB_LIVE_TRELLO_TOKEN").is_err() || std::env::var("CTOX_TOOLHUB_LIVE_TRELLO_KEY").is_err() {
        return;
    }
    let mut fails = vec![];
    let mut run = |tool: &str, args: Value| -> Value {
        match support::live_exec("trello", AuthSchemeKind::OAuthHeader, tool, args) {
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
    let me = run("TRELLO_GET_CURRENT_MEMBER", json!({}));
    let org = me["idOrganizations"][0].as_str().map(String::from);
    run("TRELLO_LIST_BOARDS", json!({}));
    let tag = format!("CTOX Toolhub Test {}", std::process::id());
    let b = run("TRELLO_CREATE_BOARD", json!({"name": tag, "idOrganization": org, "defaultLists": false, "desc": "Wird vom Livetest gelöscht"}));
    let board = b["id"].as_str().expect("board id").to_string();
    let scenario_body = |run: &mut dyn FnMut(&str, Value) -> Value| {
        run("TRELLO_GET_BOARD", json!({"board": board}));
        let l1 = run("TRELLO_CREATE_LIST", json!({"board": board, "name": "Eingang"}));
        let l2 = run("TRELLO_CREATE_LIST", json!({"board": board, "name": "Erledigt", "pos": "bottom"}));
        let (l1, l2) = (l1["id"].as_str().unwrap_or_default().to_string(), l2["id"].as_str().unwrap_or_default().to_string());
        run("TRELLO_LIST_LISTS", json!({"board": board}));
        let c = run("TRELLO_CREATE_CARD", json!({"list": l1, "name": "Testkarte", "desc": "**Markdown**", "due": "2026-11-01T12:00:00Z"}));
        let card = c["id"].as_str().unwrap_or_default().to_string();
        run("TRELLO_GET_CARD", json!({"card": card}));
        run("TRELLO_UPDATE_CARD", json!({"card": card, "name": "Testkarte (geändert)", "dueComplete": true}));
        run("TRELLO_LIST_CARDS", json!({"list": l1}));
        let lab = run("TRELLO_CREATE_LABEL", json!({"board": board, "name": "ctox", "color": "green"}));
        if let Some(lid) = lab["id"].as_str() { run("TRELLO_ADD_LABEL_TO_CARD", json!({"card": card, "label": lid})); }
        run("TRELLO_ADD_COMMENT", json!({"card": card, "text": "Kommentar vom Toolhub-Livetest"}));
        let cl = run("TRELLO_ADD_CHECKLIST", json!({"card": card, "name": "Schritte"}));
        if let Some(cid) = cl["id"].as_str() { run("TRELLO_ADD_CHECKLIST_ITEM", json!({"checklist": cid, "name": "Erster Punkt"})); }
        let mv = run("TRELLO_MOVE_CARD", json!({"card": card, "list": l2}));
        assert_eq!(mv["idList"].as_str(), Some(l2.as_str()), "card moved");
        run("TRELLO_SEARCH", json!({"query": "Testkarte", "boards": [board]}));
        let wh = run("TRELLO_CREATE_WEBHOOK", json!({"callbackURL": "https://example.com/", "idModel": board, "description": "ctox live test"}));
        if let Some(w) = wh["id"].as_str() { run("TRELLO_DELETE_WEBHOOK", json!({"webhook": w})); }
        let c2 = run("TRELLO_CREATE_CARD", json!({"list": l1, "name": "Wegwerfkarte"}));
        if let Some(id) = c2["id"].as_str() { run("TRELLO_DELETE_CARD", json!({"card": id})); }
    };
    scenario_body(&mut run);
    // Cleanup: close, then delete the test board (always attempted).
    run("TRELLO_CLOSE_BOARD", json!({"board": board}));
    run("TRELLO_DELETE_BOARD", json!({"board": board}));
    assert!(fails.is_empty(), "live failures: {fails:?}");
}

#[test]
#[ignore]
fn live_board_lifecycle_via_spec() {
    support::use_spec(include_str!("../specs/trello.json"));
    scenario();
}
