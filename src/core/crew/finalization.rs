use super::*;
use rusqlite::OptionalExtension;
use serde_json::Value;

/// Only the bounded typed field of an existing reply is accepted; no model call.
pub(crate) fn parse_retrospective(reply: &str) -> Option<Retrospective> {
    if reply.len() > 1_048_576 {
        return None;
    }
    let parse = |raw: &str| {
        serde_json::from_str::<Value>(raw)
            .ok()
            .and_then(|v| v.get("crew_retrospective").cloned())
            .and_then(|v| serde_json::from_value(v).ok())
    };
    parse(reply).or_else(|| {
        reply.split("```").skip(1).step_by(2).find_map(|raw| {
            parse(
                raw.trim()
                    .strip_prefix("ctox-crew")
                    .or_else(|| raw.trim().strip_prefix("json"))
                    .unwrap_or(raw)
                    .trim(),
            )
        })
    })
}

/// Metadata stays in the durable attempt, but never enters a user's chat bubble
/// or an outbound answer. Ordinary fenced code remains byte-for-byte intact.
pub(crate) fn public_reply_text(reply: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(reply) {
        if value.get("crew_retrospective").is_some() {
            return ["user_message", "outbound_text", "reply", "text"]
                .iter()
                .find_map(|key| value.get(key).and_then(Value::as_str))
                .unwrap_or("")
                .to_string();
        }
    }
    let mut result = String::new();
    let mut cursor = 0;
    let mut removed_metadata = false;
    while let Some(offset) = reply[cursor..].find("```") {
        let start = cursor + offset;
        result.push_str(&reply[cursor..start]);
        let Some(end_offset) = reply[start + 3..].find("```") else {
            if reply[start + 3..].trim_start().starts_with("ctox-crew") {
                removed_metadata = true;
                cursor = reply.len();
            } else {
                cursor = start;
            }
            break;
        };
        let end = start + 3 + end_offset;
        let body = reply[start + 3..end].trim();
        let json_body = body.strip_prefix("json").unwrap_or(body).trim();
        let metadata = body.starts_with("ctox-crew")
            || serde_json::from_str::<Value>(json_body)
                .ok()
                .is_some_and(|value| value.get("crew_retrospective").is_some());
        if !metadata {
            result.push_str(&reply[start..end + 3]);
        } else {
            removed_metadata = true;
        }
        cursor = end + 3;
    }
    result.push_str(&reply[cursor..]);
    if removed_metadata {
        result.trim().to_string()
    } else {
        result
    }
}

/// Pump-owned transaction. Replays cannot count an attempt or learning twice.
pub(crate) fn finalize_attempt(
    conn: &Connection,
    attempt: &str,
    status: &str,
    review_passed: Option<bool>,
    finished: &str,
    elapsed_ms: Option<i64>,
    reply: &str,
    owner_feedback: Option<&str>,
) -> Result<()> {
    let finished = if let Ok(ms) = finished.parse::<i64>() {
        chrono::DateTime::from_timestamp_millis(ms)
            .context("invalid attempt timestamp")?
            .to_rfc3339()
    } else {
        chrono::DateTime::parse_from_rfc3339(finished)?.to_rfc3339()
    };
    let finished = finished.as_str();
    // Claim the writer before reading finalized_at. A deferred transaction can
    // otherwise lose its WAL snapshot to another writer and fail on promotion.
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;

    let member: Option<String> = tx
        .query_row(
            "SELECT member_id FROM crew_attempts WHERE attempt_id=?1 AND finalized_at IS NULL",
            [attempt],
            |r| r.get(0),
        )
        .optional()?;
    let Some(member) = member else {
        return Ok(());
    };
    let succeeded = status == "succeeded";
    let retrospective = parse_retrospective(reply).and_then(|mut r| {
        r.normalize();
        match r.validate(succeeded && review_passed == Some(true), owner_feedback) {
            Ok(()) => Some(r),
            Err(_) => {
                // The transaction's finalized_at guard makes this once per attempt.
                // Neither the rejected text nor credentials are logged.
                eprintln!("[ctox crew] rejected retrospective for attempt {attempt}: invalid prose or unsupported evidence");
                None
            }
        }
    });
    // Learnings no longer get their own store. They wait as typed JSON on the
    // attempt until the learner (maintenance loop) writes them into the
    // member's anchors document and refreshes its memory in the LCM.
    let learning_json = retrospective
        .as_ref()
        .map(|r| serde_json::to_string(&r.learnings))
        .transpose()?
        .unwrap_or_else(|| "[]".to_string());
    tx.execute(
        "UPDATE crew_attempts SET finalized_at=?2,succeeded=?3,review_passed=?4,
        elapsed_ms=?5,retrospective=?6,learning_json=?7,learning_due=1
        WHERE attempt_id=?1 AND finalized_at IS NULL",
        params![
            attempt,
            finished,
            succeeded,
            review_passed,
            elapsed_ms,
            retrospective.as_ref().map(|r| r.retrospective.as_str()),
            learning_json
        ],
    )?;
    let raw: String = tx.query_row(
        "SELECT stats_json FROM crew_members WHERE id=?1",
        [&member],
        |r| r.get(0),
    )?;
    let mut stats: Stats = serde_json::from_str(&raw)?;
    let previous = stats.tasks_total;
    stats.tasks_total += 1;
    if succeeded {
        stats.succeeded += 1;
    } else {
        stats.failed += 1;
    }
    if review_passed == Some(true) {
        stats.review_passed += 1;
    } else if review_passed == Some(false) {
        stats.review_rejected += 1;
    }
    stats.avg_elapsed_ms = ((u128::from(stats.avg_elapsed_ms) * u128::from(previous)
        + elapsed_ms.unwrap_or(0).max(0) as u128)
        / u128::from(stats.tasks_total))
    .min(u64::MAX as u128) as u64;
    tx.execute(
        "UPDATE crew_members SET stats_json=?2,updated_at=?3 WHERE id=?1",
        params![member, serde_json::to_string(&stats)?, finished],
    )?;
    retain_learnings(&tx, &member)?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn retain_learnings(conn: &Connection, member: &str) -> Result<()> {
    loop {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM crew_member_learnings WHERE member_id=?1",
            [member],
            |r| r.get(0),
        )?;
        if count <= 200 {
            break;
        }
        conn.execute(
            "DELETE FROM crew_member_learnings WHERE id IN (
            SELECT id FROM crew_member_learnings WHERE member_id=?1
            ORDER BY confirmed_by_owner,created_at,id LIMIT ?2)",
            params![member, (count - 200).min(128)],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn structured_metadata_is_retained_for_learning_but_not_delivered_to_the_user() {
        let metadata =
            serde_json::json!({"crew_retrospective":{"retrospective":"Geprüft.","learnings":[]}})
                .to_string();
        let reply = format!("Fertig.\n\n```ctox-crew\n{metadata}\n```");
        assert!(parse_retrospective(&reply).is_some());
        assert_eq!(public_reply_text(&reply), "Fertig.");
        let code = "Beispiel:\n```json\n{\"user_data\":1}\n```";
        assert_eq!(public_reply_text(code), code);
        assert_eq!(
            public_reply_text("  unveränderte Antwort\n"),
            "  unveränderte Antwort\n"
        );
        assert_eq!(
            public_reply_text("Antwort\n```ctox-crew\n{unclosed}"),
            "Antwort"
        );
        assert_eq!(
            public_reply_text("Antwort\n```ctox-crew\n{broken secret}\n```"),
            "Antwort"
        );
    }
    #[test]
    fn finalization_reserves_writer_before_reading_attempt() -> Result<()> {
        use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
        use std::sync::{Arc, Mutex};

        let directory = tempfile::tempdir()?;
        let path = directory.path().join("crew.sqlite");
        let conn = Connection::open(&path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(
            "CREATE TABLE communication_routing_state(message_key TEXT PRIMARY KEY,route_status TEXT,leased_at TEXT);
             INSERT INTO communication_routing_state(message_key) VALUES ('existing');",
        )?;
        super::super::ensure_schema(&conn)?;
        conn.execute(
            "INSERT INTO crew_attempts(attempt_id,task_id,member_id,selected_at)
             VALUES('concurrent','t','crew-milo','2026-09-09T12:00:00Z')",
            [],
        )?;
        let writer = Connection::open(&path)?;
        writer.busy_timeout(std::time::Duration::ZERO)?;
        let observed = Arc::new(Mutex::new(None));
        let observed_in_hook = Arc::clone(&observed);
        // Preparing the first UPDATE happens after the finalized_at SELECT.
        // Use the already-enabled SQLite hook to place a second writer exactly
        // in that gap, without sleeps, scheduler races or production test hooks.
        conn.authorizer(Some(move |context: AuthContext<'_>| {
            if matches!(context.action, AuthAction::Update { table_name: "crew_attempts", .. }) {
                let mut observed = observed_in_hook.lock().unwrap();
                if observed.is_none() {
                    *observed = Some(writer.execute(
                        "UPDATE communication_routing_state SET route_status='raced' WHERE message_key='existing'",
                        [],
                    ));
                }
            }
            Authorization::Allow
        }));
        let result = finalize_attempt(
            &conn,
            "concurrent",
            "failed",
            None,
            "2026-09-09T12:01:00Z",
            None,
            "Failed",
            None,
        );
        let observed = observed.lock().unwrap();
        let writer_result = observed.as_ref().context("competing writer did not run")?;
        assert_eq!(
            writer_result
                .as_ref()
                .err()
                .and_then(rusqlite::Error::sqlite_error_code),
            Some(rusqlite::ErrorCode::DatabaseBusy),
            "another writer must not commit between finalization's read and write",
        );
        result?;
        finalize_attempt(
            &conn,
            "concurrent",
            "failed",
            None,
            "2026-09-09T12:02:00Z",
            None,
            "Repeated",
            None,
        )?;
        let (total, failed): (i64, i64) = conn.query_row(
            "SELECT json_extract(stats_json,'$.tasks_total'),json_extract(stats_json,'$.failed')
             FROM crew_members WHERE id='crew-milo'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!((total, failed), (1, 1));
        Ok(())
    }

    #[test]
    fn finalization_is_idempotent_deduplicated_and_requires_passed_review() {
        let conn = super::super::tests::fixture();
        for id in ["a", "b", "c"] {
            conn.execute(
                "INSERT INTO crew_attempts(attempt_id,task_id,member_id,selected_at)
                VALUES(?1,'t','crew-milo','2026-09-05T12:00:00Z')",
                [id],
            )
            .unwrap();
        }
        let reply =
            serde_json::json!({"crew_retrospective": {"retrospective":"Das Schema wurde geprüft.",
            "learnings":[{"text":"Schema prüfen","kind":"insight","scope":{}}]}})
            .to_string();
        finalize_attempt(
            &conn,
            "a",
            "succeeded",
            Some(true),
            "2026-09-05T12:01:00Z",
            Some(60000),
            &reply,
            None,
        )
        .unwrap();
        finalize_attempt(
            &conn,
            "a",
            "succeeded",
            Some(true),
            "2026-09-05T12:01:00Z",
            Some(60000),
            &reply,
            None,
        )
        .unwrap();
        finalize_attempt(
            &conn,
            "b",
            "succeeded",
            Some(true),
            "2026-09-05T12:02:00Z",
            Some(1000),
            &reply.replace("Schema prüfen", " SCHEMA   PRÜFEN "),
            None,
        )
        .unwrap();
        finalize_attempt(
            &conn,
            "c",
            "failed",
            Some(false),
            "2026-09-05T12:03:00Z",
            Some(1000),
            &reply.replace("Schema prüfen", "Anderes Learning"),
            None,
        )
        .unwrap();
        // Learnings wait on the attempt for the learner; a failed attempt
        // keeps no retrospective and therefore no learnings.
        let learning_json = |id: &str| {
            conn.query_row(
                "SELECT COALESCE(learning_json,'[]'),learning_due FROM crew_attempts WHERE attempt_id=?1",
                [id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
            )
            .unwrap()
        };
        assert!(learning_json("a").0.contains("Schema prüfen"));
        assert_eq!(learning_json("a").1, 1);
        assert_eq!(learning_json("c").0, "[]");
        assert_eq!(learning_json("c").1, 1);
        assert_eq!(
            members(&conn)
                .unwrap()
                .into_iter()
                .find(|m| m.id == "crew-milo")
                .unwrap()
                .stats
                .tasks_total,
            3
        );
        assert_eq!(
            conn.query_row(
                "SELECT retrospective FROM crew_attempts WHERE attempt_id='c'",
                [],
                |r| r.get::<_, Option<String>>(0)
            )
            .unwrap(),
            None
        );
    }
    #[test]
    fn retention_removes_oldest_unconfirmed_first() {
        let conn = super::super::tests::fixture();
        for i in 0..205 {
            conn.execute("INSERT INTO crew_member_learnings(id,member_id,text,normalized_text,kind,scope_json,evidence_run_id,created_at,confirmed_by_owner)
                VALUES(?1,'crew-milo',?1,?1,'pitfall','{}','a',?1,?2)",params![format!("{i:04}"),i==0]).unwrap();
        }
        retain_learnings(&conn, "crew-milo").unwrap();
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM crew_member_learnings", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            200
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM crew_member_learnings WHERE id='0000'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM crew_member_learnings WHERE id='0001'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
}
