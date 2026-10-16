//! Core-owned scheduling metadata in the SAME Store and writer transaction.
//! No app rows, record bodies, credentials or task results are copied here.
use ctox_store::StoreError;
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};
type Result<T> = std::result::Result<T, StoreError>;
fn invalid(code: &str) -> StoreError {
    StoreError::Invalid(format!("{code}: runtime state rejected"))
}
pub fn initialize(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS _app_runtime_triggers(
        app_id TEXT NOT NULL,trigger_id TEXT NOT NULL,cursor TEXT NOT NULL DEFAULT '{}',
        last_seq INTEGER NOT NULL DEFAULT 0,next_due INTEGER NOT NULL DEFAULT 0,
        running INTEGER NOT NULL DEFAULT 0,run_id TEXT,fence INTEGER,outcome TEXT,
        PRIMARY KEY(app_id,trigger_id)) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS _app_runtime_completions(
        task_id TEXT PRIMARY KEY,app_id TEXT NOT NULL,app_ver TEXT NOT NULL,actor_id TEXT NOT NULL,
        on_complete TEXT,on_failed TEXT,state TEXT NOT NULL,run_id TEXT,fence INTEGER,outcome TEXT);")?;
    Ok(())
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .filter(|v| !v.is_empty() && v.len() <= 512 && !v.chars().any(char::is_control))
        .ok_or_else(|| invalid("invalid"))
}
fn outcome(value: &Value) -> String {
    // The task/command store owns results, this receipt only tracks disposition.
    json!({"status":value["status"],"code":value["code"],"uncertain":value["uncertain"]==true})
        .to_string()
}
pub fn apply(tx: &Transaction<'_>, binding: &Value, method: &str, args: &Value) -> Result<Value> {
    let command = &binding["command"];
    let app = crate::execution_app(command).map_err(|_| invalid("invalid"))?;
    let run = text(&binding["fence"], "run_id")?;
    let fence = binding["fence"]["fence"]
        .as_i64()
        .filter(|v| *v > 0)
        .ok_or_else(|| invalid("revoked"))?;
    match method {
        "trigger.claim" => {
            let trigger = text(args, "trigger_id")?;
            let event = &args["event"];
            tx.execute(
                "INSERT OR IGNORE INTO _app_runtime_triggers(app_id,trigger_id) VALUES(?,?)",
                params![app, trigger],
            )?;
            let(running,last,next):(bool,i64,i64)=tx.query_row("SELECT running,last_seq,next_due FROM _app_runtime_triggers WHERE app_id=? AND trigger_id=?",params![app,trigger],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
            if running {
                return Ok(json!({"claimed":false}));
            }
            let (seq, due) = if event["kind"] == "change" {
                let seq = event["seq"]
                    .as_i64()
                    .filter(|v| *v > 0)
                    .ok_or_else(|| invalid("invalid"))?;
                if seq <= last {
                    return Ok(json!({"claimed":false}));
                }
                (seq, next)
            } else if event["kind"] == "timer" {
                let now = event["now_ms"].as_i64().ok_or_else(|| invalid("invalid"))?;
                let interval = event["interval_ms"]
                    .as_i64()
                    .filter(|v| *v >= 60000)
                    .ok_or_else(|| invalid("invalid"))?;
                if now < next {
                    return Ok(json!({"claimed":false}));
                }
                (
                    last,
                    now.checked_add(interval)
                        .ok_or_else(|| invalid("invalid"))?,
                )
            } else {
                return Err(invalid("invalid"));
            };
            tx.execute("UPDATE _app_runtime_triggers SET running=1,last_seq=?,next_due=?,run_id=?,fence=?,outcome=NULL WHERE app_id=? AND trigger_id=?",params![seq,due,run,fence,app,trigger])?;
            Ok(json!({"claimed":true}))
        }
        "trigger.finish" => {
            let trigger = text(args, "trigger_id")?;
            let changed=tx.execute("UPDATE _app_runtime_triggers SET running=0,outcome=? WHERE app_id=? AND trigger_id=? AND running=1 AND run_id=? AND fence=?",params![outcome(&args["outcome"]),app,trigger,run,fence])?;
            if changed != 1 {
                return Err(invalid("revoked"));
            }
            Ok(json!({"status":"accepted"}))
        }
        "trigger.cursor" => {
            // client_context is untrusted metadata, never an authority grant.
            if binding["kind"] != "trigger" {
                return Err(invalid("forbidden"));
            }
            let trigger = text(binding, "trigger_id")?;
            let key = args[1]
                .as_str()
                .filter(|v| !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control))
                .ok_or_else(|| invalid("invalid"))?;
            let cursor:Option<String>=tx.query_row("SELECT cursor FROM _app_runtime_triggers WHERE app_id=? AND trigger_id=? AND running=1 AND run_id=? AND fence=?",params![app,trigger,run,fence],|r|r.get(0)).optional()?;
            let mut cursor: Value =
                serde_json::from_str(&cursor.ok_or_else(|| invalid("revoked"))?)
                    .map_err(|_| invalid("invalid"))?;
            if args[0] == "get" {
                return Ok(cursor[key].clone());
            }
            if args[0] != "set" {
                return Err(invalid("invalid"));
            }
            cursor[key] = args[2].clone();
            let encoded = cursor.to_string();
            if encoded.len() > 1024 * 1024 {
                return Err(invalid("quota"));
            }
            tx.execute("UPDATE _app_runtime_triggers SET cursor=? WHERE app_id=? AND trigger_id=? AND run_id=? AND fence=?",params![encoded,app,trigger,run,fence])?;
            Ok(json!({"status":"accepted"}))
        }
        "completion.register" => {
            let task = text(args, "task_id")?;
            let version = text(command, "app_ver")?;
            let actor = text(&command["actor"], "id")?;
            let callbacks = &args["callbacks"];
            let complete = callbacks["on_complete"].as_str();
            let failed = callbacks["on_failed"].as_str();
            if complete.is_none() && failed.is_none() {
                return Err(invalid("invalid"));
            }
            let prior:Option<(String,String,String,Option<String>,Option<String>)>=tx.query_row("SELECT app_id,app_ver,actor_id,on_complete,on_failed FROM _app_runtime_completions WHERE task_id=?",[task],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
            if let Some(prior) = prior {
                if prior
                    != (
                        app.into(),
                        version.into(),
                        actor.into(),
                        complete.map(str::to_owned),
                        failed.map(str::to_owned),
                    )
                {
                    return Err(invalid("conflict"));
                }
            } else {
                tx.execute("INSERT INTO _app_runtime_completions(task_id,app_id,app_ver,actor_id,on_complete,on_failed,state) VALUES(?,?,?,?,?,?,'pending')",params![task,app,version,actor,complete,failed])?;
            }
            Ok(json!({"status":"accepted"}))
        }
        "completion.claim" => {
            let task = text(args, "task_id")?;
            let prior:Option<(String,String,String,String,Option<String>,Option<String>)>=tx.query_row("SELECT app_id,app_ver,actor_id,state,on_complete,on_failed FROM _app_runtime_completions WHERE task_id=?",[task],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
            let Some((owner, version, actor, state, complete, failed)) = prior else {
                return Ok(json!({"claimed":false}));
            };
            if owner != app || command["app_ver"] != version || command["actor"]["id"] != actor {
                return Err(invalid("forbidden"));
            }
            if state != "pending" {
                return Ok(json!({"claimed":false}));
            }
            let entry = if args["succeeded"] == true {
                complete
            } else {
                failed
            };
            tx.execute("UPDATE _app_runtime_completions SET state=?,run_id=?,fence=? WHERE task_id=? AND state='pending'",params![if entry.is_some(){"running"}else{"finished"},run,fence,task])?;
            Ok(json!({"claimed":entry.is_some(),"entry":entry}))
        }
        "completion.finish" => {
            let task = text(args, "task_id")?;
            let changed=tx.execute("UPDATE _app_runtime_completions SET state='finished',outcome=? WHERE task_id=? AND app_id=? AND app_ver=? AND actor_id=? AND state='running' AND run_id=? AND fence=?",params![outcome(&args["outcome"]),task,app,text(command,"app_ver")?,text(&command["actor"],"id")?,run,fence])?;
            if changed != 1 {
                return Err(invalid("revoked"));
            }
            Ok(json!({"status":"accepted"}))
        }
        _ => Err(invalid("invalid")),
    }
}

#[cfg(test)]
mod e39_tests {
    use super::*;
    #[test]
    fn callback_metadata_uses_execution_target() {
        let mut db=rusqlite::Connection::open_in_memory().unwrap();
        let tx=db.transaction().unwrap();initialize(&tx).unwrap();
        let binding=json!({"command":{"app_id":"sender","target_app":"receiver","app_ver":"v1","actor":{"id":"owner"}},"manifest":{"handlers":{"done":"handlers/done.js"}},"fence":{"run_id":"app-runtime:parent","fence":1}});
        apply(&tx,&binding,"completion.register",&json!({"task_id":"command:child","callbacks":{"on_complete":"done"}})).unwrap();
        let app:String=tx.query_row("SELECT app_id FROM _app_runtime_completions WHERE task_id='command:child'",[],|r|r.get(0)).unwrap();
        assert_eq!(app,"receiver","E39 callback scope must belong to executing target");
    }
}
