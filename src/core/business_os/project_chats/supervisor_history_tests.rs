// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
const THREAD: &str = "cc6cfe73-2824-4360-9daf-3b3efb079931";

fn read(root: &Path, actor: &str, page: Value) -> anyhow::Result<Value> {
    crate::business_os::project_chats::supervisor_history::history(
        root, actor, json!({"project_id":"project","thread_id":THREAD,"history_page":page}),
    )
}
fn submit(root: &Path, id: &str, goal: &str) -> anyhow::Result<Value> {
    let accepted = super::supervisor_turns::control(
        root, id, "owner", "submit",
        json!({"project_id":"project","thread_id":THREAD,"goal":goal}),
    )?;
    ensure!(accepted["status"] == "completed", "{accepted}");
    Ok(accepted["result"]["turn"].clone())
}

#[test]
fn supervisor_history_paginates_real_native_turns_after_another_send_and_reader_reopen(
) -> anyhow::Result<()> {
    let root = super::supervisor_turns::fixture()?;
    let first = submit(root.path(),"question-one","First fixture question")?;
    let second = submit(root.path(),"question-two","Second fixture question")?;
    let third = submit(root.path(),"question-three","Third fixture question")?;
    let conn = Connection::open(crate::paths::core_db(root.path()))?;
    let count = || conn.query_row(
        "SELECT count(*) FROM business_command_aggregates
            WHERE command_type='business_os.chat.task'",
        [], |r| r.get::<_,i64>(0),
    );
    assert_eq!(count()?,3);
    let page = read(root.path(),"owner",json!({"limit":2}))?;
    assert_eq!(page["contract"],"ctox.workjet.supervisor_history.v1");
    assert_eq!(page["history_page"]["turns"].as_array().unwrap().len(),2);
    assert_eq!(page["history_page"]["has_more"],true);
    let reopened = read(root.path(),"owner",json!({"limit":2}))?;
    assert_eq!(reopened,page);
    let remaining = read(root.path(),"owner",json!({"limit":2,
        "cursor":page["history_page"]["next_cursor"]}))?;
    assert_eq!(remaining["history_page"]["turns"].as_array().unwrap().len(),1);
    assert_eq!(remaining["history_page"]["has_more"],false);
    let mut observed = page["history_page"]["turns"].as_array().unwrap().clone();
    observed.extend(remaining["history_page"]["turns"].as_array().unwrap().clone());
    for turn in [&first,&second,&third] {
        let entry = observed.iter().find(|entry| entry["command_id"]==turn["command_id"])
            .context("earlier admitted turn was lost")?;
        assert_eq!(entry["task_id"],turn["task_id"]);
        assert_eq!(entry["user_text_truncated"],false);
    }
    let mut texts = observed.iter().map(|e| e["user_text"].as_str().unwrap()).collect::<Vec<_>>();
    texts.sort();
    assert_eq!(texts,vec!["First fixture question","Second fixture question","Third fixture question"]);
    assert_eq!(count()?,3,"history reads must not submit another task");
    let operation = super::supervisor_turns::control(
        root.path(),"history-operation","owner","history",
        json!({"project_id":"project","thread_id":THREAD,"history_page":{"limit":2}}),
    )?;
    assert_eq!(operation["status"],"completed");
    assert_eq!(operation["result"]["history_page"],page["history_page"]);
    assert_eq!(count()?,3);
    Ok(())
}

#[test]
fn supervisor_history_denies_foreign_owner_forged_cursor_and_excess_request_fields(
) -> anyhow::Result<()> {
    let root = super::supervisor_turns::fixture()?;
    let turn = submit(root.path(),"question","Owner's actual fixture question")?;
    assert!(read(root.path(),"other",json!({})).is_err());
    for page in [
        json!({"limit":0}),json!({"limit":21}),json!({"owner_user_id":"other"}),
        json!({"cursor":{"before_created_at_ms":0,"before_command_id":"missing"}}),
        json!({"cursor":{"before_created_at_ms":-1,"before_command_id":turn["command_id"]}}),
    ] { assert!(read(root.path(),"owner",page.clone()).is_err(),"{page}"); }
    let all = read(root.path(),"owner",json!({}))?;
    let entry = &all["history_page"]["turns"][0];
    assert!(read(root.path(),"owner",json!({"cursor":{
        "before_created_at_ms":entry["created_at_ms"].as_i64().unwrap()+1,
        "before_command_id":entry["command_id"],
    }})).is_err());
    let conn = Connection::open(crate::paths::core_db(root.path()))?;
    conn.execute(
        "UPDATE business_command_aggregates SET intent_json=
            json_set(intent_json,'$.payload.thread_key','foreign-thread')
            WHERE command_id=?1", [turn["command_id"].as_str().unwrap()],
    )?;
    assert_eq!(read(root.path(),"owner",json!({}))?["history_page"]["turns"],json!([]));
    Ok(())
}
