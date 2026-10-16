//! Live Airtable verification through the hub exec path (personal access token, Bearer).
//! Reads the test base; writes only throwaway records in the test table and deletes them again.
//! `CTOX_TOOLHUB_LIVE_AIRTABLE_TOKEN=… CTOX_TOOLHUB_LIVE_AIRTABLE_BASE=app… CTOX_TOOLHUB_LIVE_AIRTABLE_TABLE=tbl…
//!  cargo test --test live_airtable -- --ignored --nocapture`
mod support;
use ctox_toolhub::AuthSchemeKind;
use serde_json::{json, Value};

#[test]
#[ignore]
fn live_read_and_scoped_write_actions() {
    let (Ok(_), Ok(base), Ok(table)) = (
        std::env::var("CTOX_TOOLHUB_LIVE_AIRTABLE_TOKEN"),
        std::env::var("CTOX_TOOLHUB_LIVE_AIRTABLE_BASE"),
        std::env::var("CTOX_TOOLHUB_LIVE_AIRTABLE_TABLE"),
    ) else {
        return;
    };
    let mut fails = vec![];
    let mut run = |tool: &str, args: Value| -> Value {
        match support::live_exec("airtable", AuthSchemeKind::Bearer, tool, args) {
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
    let bt = |extra: Value| {
        let mut v = json!({"base_id": base, "table_id_or_name": table});
        v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        v
    };
    run("AIRTABLE_GET_CURRENT_USER", json!({}));
    run("AIRTABLE_LIST_BASES", json!({}));
    run("AIRTABLE_GET_BASE", json!({"base_id": base}));
    let schema = run("AIRTABLE_GET_BASE_SCHEMA", json!({"base_id": base}));
    let primary = schema
        .pointer("/tables")
        .or_else(|| schema.pointer("/data/tables"))
        .and_then(Value::as_array)
        .and_then(|ts| ts.iter().find(|t| t["id"] == table.as_str()))
        .and_then(|t| {
            let pid = t["primaryFieldId"].as_str()?;
            t["fields"].as_array()?.iter().find(|f| f["id"] == pid)?["name"].as_str().map(String::from)
        })
        .unwrap_or_else(|| "Name".into());
    println!("primary field: {primary}");
    run("AIRTABLE_LIST_RECORDS", bt(json!({"max_records": 5})));
    run("AIRTABLE_SEARCH_RECORDS", bt(json!({"search_field": primary, "search_value": "Toolhub live test"})));
    let found = run("AIRTABLE_FIND_RECORD", bt(json!({"search_field": primary, "search_value": "Toolhub live test"})));
    let rec = support::find_str(&found, "id").expect("record 'Toolhub live test'");
    run("AIRTABLE_GET_RECORD", bt(json!({"record_id": rec})));
    run("AIRTABLE_GET_RECORD_BY_ID", bt(json!({"record_id": rec})));
    run("AIRTABLE_LIST_RECORD_COMMENTS", bt(json!({"record_id": rec})));

    let created = run("AIRTABLE_CREATE_RECORD", bt(json!({"fields": {primary.clone(): "Toolhub Schreibtest (wird gelöscht)"}})));
    let new = support::find_str(&created, "id").expect("created record id");
    run("AIRTABLE_UPDATE_RECORD", bt(json!({"record_id": new, "fields": {primary.clone(): "Toolhub Schreibtest (aktualisiert)"}})));
    run("AIRTABLE_ADD_COMMENT_TO_RECORD", bt(json!({"record_id": new, "text": "Kommentar vom Toolhub-Livetest"})));
    run("AIRTABLE_LIST_RECORD_COMMENTS", bt(json!({"record_id": new})));
    run("AIRTABLE_CLEAR_RECORD", bt(json!({"record_id": new, "fields": {primary.clone(): ""}})));
    run("AIRTABLE_DELETE_RECORD", bt(json!({"record_id": new})));
    let up = run("AIRTABLE_UPSERT_RECORDS", bt(json!({
        "records": [{"fields": {primary.clone(): "Toolhub Upsert-Test (wird gelöscht)"}}],
        "fields_to_merge_on": [primary.clone()]
    })));
    let ids0: Vec<String> = up.to_string().split('"').filter(|s| s.starts_with("rec") && s.len() == 17).map(String::from).collect();
    let mut ids = ids0; ids.sort(); ids.dedup();
    if ids.is_empty() { println!("LIVE FAIL AIRTABLE_UPSERT_RECORDS: no record id in result"); fails.push("UPSERT ids".into()); }
    else { run("AIRTABLE_DELETE_RECORDS_BATCH", bt(json!({"record_ids": ids}))); }
    for t in ["AIRTABLE_CREATE_BASE", "AIRTABLE_CREATE_TABLE", "AIRTABLE_UPDATE_TABLE", "AIRTABLE_CREATE_FIELD", "AIRTABLE_UPDATE_FIELD"] {
        println!("LIVE SKIP {t} (Schemaänderung, nicht über die Tools rückgängig zu machen)");
    }
    assert!(fails.is_empty(), "live failures: {fails:?}");
}
