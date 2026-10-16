//! Reviewed native ports of v4/schemas/migrations; source hashes fail closed on drift.
use super::*;

pub(super) fn validate(operation: &Value) -> anyhow::Result<()> {
    let name = operation["collection"].as_str().unwrap_or_default();
    let expected = match name {
        "communication_accounts" => {
            "8937ce91c473b38b3e45b44aef2289195f15162a60b0f714b9f5b190b7cdfd96"
        }
        "communication_messages" => {
            "b96d2bcd2317ef7006c7d5c16d0b63f2a79cd2d723d06c46097aa552c266eb54"
        }
        "communication_threads" => {
            "bbf8e3af294c8e2e4040a948a3266d95621d58f462e0123525399d06fc47c2c0"
        }
        "ctox_crew_learnings" => "325d1acc7666a20e49693944414ae60f1c02973d308c9d4bfb69f5f9ee06de55",
        "accounting_dunning_runs" => {
            "4ff0d08a5b93536c03d7a6f284d36bb35070fb0eee843fd51f1a660b012f7eee"
        }
        _ => anyhow::bail!("unreviewed native v4 migration: {name}"),
    };
    anyhow::ensure!(
        operation["source_sha256"].as_str() == Some(expected),
        "native v4 migration source changed: {name}; review port before cleanup"
    );
    Ok(())
}

pub(super) fn apply(body: &Value, operation: &Value) -> anyhow::Result<Value> {
    validate(operation)?;
    let name = operation["collection"].as_str().unwrap();
    let mut next = body
        .as_object()
        .context("native v4 migration requires an object")?
        .clone();
    match name {
        // ref: v4/schemas/migrations/conversations/communication_{accounts,messages,threads}.js
        "communication_accounts" | "communication_messages" | "communication_threads" => {
            if !body["id"].as_str().is_some_and(|id| !id.is_empty()) {
                let key = match name {
                    "communication_accounts" => "account_key",
                    "communication_messages" => "message_key",
                    _ => "thread_key",
                };
                let id = body[key]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .context("missing communication natural key; source retained")?;
                next.insert("id".into(), id.into());
            }
        }
        // ref: v4/schemas/migrations/ctox/ctox_crew_learnings.js:7-10
        "ctox_crew_learnings" => {
            if !body["normalized_text"].is_string() {
                let text = body["text"].as_str().unwrap_or_default();
                // ECMAScript \s includes BOM, excludes NEL (unlike Rust White_Space).
                let normalized = text
                    .split(|c: char| (c.is_whitespace() && c != '\u{0085}') || c == '\u{feff}')
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase();
                next.insert("normalized_text".into(), normalized.into());
            }
        }
        // ref: v4/schemas/migrations/invoices/accounting_dunning_runs.js:5-12
        "accounting_dunning_runs" => {
            if !body["invoices_total"].is_array() {
                if body["invoices_total"].is_number() {
                    next.insert(
                        "invoices_total_count".into(),
                        body["invoices_total"].clone(),
                    );
                }
                next.insert("invoices_total".into(), json!([]));
            }
        }
        _ => unreachable!("validated migration port"),
    }
    Ok(next.into())
}
