// Origin: CTOX
// License: AGPL-3.0-only

//! Receipt-backed candidates for the existing native command intake. This
//! supplies selection evidence only; command authorization and intent/actor
//! verification still happen in the shared command plane.

use super::domain_effect;
use rusqlite::Connection;
use std::path::Path;

// Previous combined predicate remains a test oracle for selection semantics.
#[cfg(test)]
pub(super) const BUSINESS_COMMAND_RETRY_CANDIDATE_SQL: &str = r#"(
  json_extract(data, '$.status') IN ('pending_sync', 'waiting_dependencies')
  OR (
    (
      json_extract(data, '$.status') = 'accepted'
      OR (
        json_extract(data, '$.status') = 'failed'
        AND COALESCE(json_extract(data, '$.terminal_status'), 'none') = 'none'
      )
    )
    AND json_extract(data, '$.command_type') IN (
      'external_sql.sync.refresh',
      'external_sql.write',
      'outbound.research_source.generate_adapter',
      'outbound.research_source.test',
      'outbound.research_source.auth_assist',
      'web_stack.person_research'
    )
  )
)"#;

pub(super) fn pending_query(
    table: &str,
    deleted: &str,
    lwt: &str,
    direction: &str,
    receipt: Option<&str>,
) -> String {
    // Separate disjoint candidate classes so SQLite can use the status index
    // for pending commands and the type index for background/applied commands.
    // Cap each ordered branch before the merge: at most three small pages sort.
    let pending = "json_extract(data, '$.status') IN ('pending_sync', 'waiting_dependencies')";
    let background = "(
        +json_extract(data, '$.status') = 'accepted'
        OR (+json_extract(data, '$.status') = 'failed'
            AND COALESCE(json_extract(data, '$.terminal_status'), 'none') = 'none')
      ) AND json_extract(data, '$.command_type') IN (
        'external_sql.sync.refresh', 'external_sql.write',
        'outbound.research_source.generate_adapter', 'outbound.research_source.test',
        'outbound.research_source.auth_assist', 'web_stack.person_research')";
    let mut branches = vec![("pending", pending), ("background", background)];
    if let Some(receipt) = receipt {
        branches.push(("applied", receipt));
    }
    let ctes = branches
        .iter()
        .map(|(name, predicate)| {
            format!(
                "{name} AS (SELECT data, {lwt} AS intake_lwt FROM {table}
         WHERE {deleted} = 0 AND ({predicate})
         ORDER BY intake_lwt {direction} LIMIT ?1)"
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let union = branches
        .iter()
        .map(|(name, _)| format!("SELECT data, intake_lwt FROM {name}"))
        .collect::<Vec<_>>()
        .join(" UNION ALL ");
    format!("WITH {ctes} SELECT data FROM ({union}) ORDER BY intake_lwt {direction} LIMIT ?1")
}

/// Return an additional, disjoint candidate predicate. The normal pending
/// states and old background commands remain owned by their existing query.
pub(super) fn retry_predicate(
    receipt_store: &Path,
    conn: &Connection,
    quoted_table: &str,
    deleted_expr: &str,
) -> anyhow::Result<Option<String>> {
    let types = domain_effect::COMMAND_TYPES
        .iter()
        .map(|name| format!("'{name}'"))
        .collect::<Vec<_>>()
        .join(",");
    // Disqualify the broad status expression index for this small set of
    // command types. Unary + preserves json_extract values but makes SQLite
    // prefer (deleted, command_type), avoiding unrelated accepted history.
    let candidate = format!(
        "{deleted_expr} = 0
         AND +json_extract(data, '$.status') IN ('accepted', 'completed', 'failed')
         AND COALESCE(json_extract(data, '$.terminal_status'), 'none') = 'none'
         AND COALESCE(json_extract(data, '$.execution_phase'), '') != 'terminal'
         AND json_extract(data, '$.command_type') IN ({types})"
    );
    // Avoid opening another database for the ordinary command/idle path.
    let has_candidates: bool = conn.query_row(
        &format!("SELECT EXISTS(SELECT 1 FROM {quoted_table} WHERE {candidate})"),
        [],
        |row| row.get(0),
    )?;
    if !has_candidates {
        return Ok(None);
    }
    if !receipt_store.is_file() {
        return Ok(None);
    }
    let mut uri = url::Url::from_file_path(receipt_store)
        .map_err(|_| anyhow::anyhow!("invalid domain receipt store path"))?;
    uri.query_pairs_mut().append_pair("mode", "ro");
    conn.execute(
        "ATTACH DATABASE ?1 AS domain_receipt_source",
        [uri.as_str()],
    )?;
    let has_schema: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM domain_receipt_source.sqlite_master
         WHERE type = 'table' AND name = 'business_command_domain_effects')",
        [],
        |row| row.get(0),
    )?;
    if !has_schema {
        return Ok(None);
    }
    Ok(Some(format!(
        "({candidate} AND EXISTS(
            SELECT 1 FROM domain_receipt_source.business_command_domain_effects AS receipt
            WHERE receipt.command_id = COALESCE(json_extract(data, '$.command_id'), json_extract(data, '$.id'))
        ))"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;
    use rusqlite::{params, OpenFlags};
    use serde_json::json;
    use serde_json::Value;

    fn commands() -> anyhow::Result<Connection> {
        let conn = Connection::open_with_flags(
            ":memory:",
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_URI,
        )?;
        conn.execute_batch(
            "CREATE TABLE commands (id TEXT PRIMARY KEY, deleted INTEGER, lastWriteTime REAL, data TEXT);
             CREATE INDEX commands_type ON commands(deleted, json_extract(data, '$.command_type'));
             CREATE INDEX commands_status ON commands(deleted, json_extract(data, '$.status'));")?;
        Ok(conn)
    }

    fn insert(
        conn: &Connection,
        id: &str,
        status: &str,
        terminal: &str,
        command_type: &str,
    ) -> anyhow::Result<Value> {
        let value = json!({"id":id,"command_id":id,"status":status,"terminal_status":terminal,
            "command_type":command_type,"created_at_ms":1});
        conn.execute(
            "INSERT INTO commands VALUES (?1, 0, 1, ?2)",
            params![id, value.to_string()],
        )?;
        Ok(value)
    }

    fn proof(path: &Path, id: &str) -> anyhow::Result<()> {
        let conn = Connection::open(path)?;
        conn.execute_batch(domain_effect::SCHEMA)?;
        conn.execute(
            "INSERT INTO business_command_domain_effects VALUES (?1, 'hash', 'actor', '{}')",
            [id],
        )?;
        Ok(())
    }

    #[test]
    fn receipt_candidates_are_read_only_and_exclude_unproved_and_terminal_commands(
    ) -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("domain #1 ? evidence.sqlite3");
        proof(&path, "applied")?;
        proof(&path, "terminal")?;
        let conn = commands()?;
        insert(
            &conn,
            "applied",
            "accepted",
            "none",
            domain_effect::COMMAND_TYPES[0],
        )?;
        insert(
            &conn,
            "unproved",
            "accepted",
            "none",
            domain_effect::COMMAND_TYPES[0],
        )?;
        insert(
            &conn,
            "terminal",
            "completed",
            "completed",
            domain_effect::COMMAND_TYPES[0],
        )?;
        insert(&conn, "unrelated", "accepted", "none", "ctox.unrelated")?;
        let predicate = retry_predicate(&path, &conn, "commands", "deleted")?
            .context("missing candidate predicate")?;
        let ids = conn
            .prepare(&format!(
                "SELECT id FROM commands WHERE deleted=0 AND {predicate}"
            ))?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        assert_eq!(ids, vec!["applied"]);
        assert!(
            conn.execute(
                "DELETE FROM domain_receipt_source.business_command_domain_effects",
                []
            )
            .is_err(),
            "receipt attachment must not be writable"
        );
        for status in ["completed", "failed"] {
            conn.execute(
                "UPDATE commands SET data=json_set(data,'$.status',?1) WHERE id='applied'",
                [status],
            )?;
            let count: i64 = conn.query_row(
                &format!("SELECT COUNT(*) FROM commands WHERE {predicate}"),
                [],
                |r| r.get(0),
            )?;
            assert_eq!(
                count, 1,
                "nonterminal projection status must remain recoverable"
            );
        }
        Ok(())
    }

    #[test]
    fn missing_or_pre_receipt_schema_never_creates_or_migrates_a_store() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("absent.sqlite3");
        let conn = commands()?;
        insert(
            &conn,
            "pending-proof",
            "accepted",
            "none",
            domain_effect::COMMAND_TYPES[0],
        )?;
        assert!(retry_predicate(&path, &conn, "commands", "deleted")?.is_none());
        assert!(!path.exists());
        Connection::open(&path)?.execute_batch("CREATE TABLE legacy (id TEXT)")?;
        assert!(retry_predicate(&path, &conn, "commands", "deleted")?.is_none());
        let legacy = Connection::open(&path)?;
        let count: i64 = legacy.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='business_command_domain_effects'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(count, 0);
        Ok(())
    }

    #[test]
    fn ordered_intake_matches_original_predicate_across_lifecycle_states() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("states.sqlite3");
        proof(&path, "schema-seed")?;
        let receipts = Connection::open(&path)?;
        let conn = commands()?;
        let types = [
            "ctox.normal",
            "external_sql.sync.refresh",
            "external_sql.write",
            "outbound.research_source.generate_adapter",
            "outbound.research_source.test",
            "outbound.research_source.auth_assist",
            "web_stack.person_research",
        ]
        .into_iter()
        .chain(domain_effect::COMMAND_TYPES);
        let mut sequence = 0;
        for kind in types {
            for status in [
                json!("pending_sync"),
                json!("waiting_dependencies"),
                json!("accepted"),
                json!("failed"),
                json!("completed"),
                json!("cancelled"),
                Value::Null,
            ] {
                for terminal in [
                    json!("none"),
                    json!("completed"),
                    json!("failed"),
                    Value::Null,
                ] {
                    for phase in [json!("accepted"), json!("terminal"), Value::Null] {
                        for deleted in [0, 1] {
                            sequence += 1;
                            let id = format!("candidate-{sequence}");
                            let data = json!({"id":id,"command_type":kind,"status":status,
                                "terminal_status":terminal,"execution_phase":phase});
                            conn.execute(
                                "INSERT INTO commands VALUES (?1,?2,?3,?4)",
                                params![id, deleted, sequence, data.to_string()],
                            )?;
                            if sequence % 3 == 0 {
                                receipts.execute("INSERT INTO business_command_domain_effects VALUES (?1,'hash','actor','{}')", [&id])?;
                            }
                        }
                    }
                }
            }
        }
        drop(receipts);
        let receipt =
            retry_predicate(&path, &conn, "commands", "deleted")?.context("receipt predicate")?;
        for proof in [None, Some(receipt.as_str())] {
            let predicate = match proof {
                Some(applied) => format!("({BUSINESS_COMMAND_RETRY_CANDIDATE_SQL} OR {applied})"),
                None => BUSINESS_COMMAND_RETRY_CANDIDATE_SQL.to_owned(),
            };
            for direction in ["ASC", "DESC"] {
                let old = format!("SELECT data FROM commands WHERE deleted=0 AND {predicate} ORDER BY lastWriteTime {direction} LIMIT ?1");
                let new = pending_query("commands", "deleted", "lastWriteTime", direction, proof);
                for limit in [1, 2, 25, 5000] {
                    let collect = |sql: &str| -> anyhow::Result<Vec<String>> {
                        Ok(conn
                            .prepare(sql)?
                            .query_map([limit], |row| row.get::<_, String>(0))?
                            .collect::<rusqlite::Result<Vec<_>>>()?)
                    };
                    assert_eq!(
                        collect(&new)?,
                        collect(&old)?,
                        "receipt={} {direction} limit={limit}",
                        proof.is_some()
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn ordered_intake_is_bounded_and_preserves_oldest_newest_selection() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("ordered-domain.sqlite3");
        let conn = commands()?;
        conn.execute_batch("WITH RECURSIVE numbers(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM numbers WHERE n<20000)
            INSERT INTO commands SELECT 'history-'||n,0,n,
            json_object('id','history-'||n,'status','accepted','terminal_status','none','command_type','ctox.unrelated')
            FROM numbers;")?;
        for (id, status, kind, time) in [
            ("pending-old", "pending_sync", "ctox.normal", 1),
            (
                "receipt-old",
                "accepted",
                domain_effect::COMMAND_TYPES[0],
                2,
            ),
            ("background", "accepted", "external_sql.write", 3),
            ("receipt-new", "failed", domain_effect::COMMAND_TYPES[0], 4),
            ("pending-new", "waiting_dependencies", "ctox.normal", 5),
        ] {
            insert(&conn, id, status, "none", kind)?;
            conn.execute(
                "UPDATE commands SET lastWriteTime=?1 WHERE id=?2",
                params![time, id],
            )?;
            if id.starts_with("receipt-") {
                proof(&path, id)?;
            }
        }
        let receipt =
            retry_predicate(&path, &conn, "commands", "deleted")?.context("receipt predicate")?;
        let mut maximum_steps = 0;
        for (direction, expected) in [
            ("ASC", ["pending-old", "receipt-old"]),
            ("DESC", ["pending-new", "receipt-new"]),
        ] {
            let sql = pending_query(
                "commands",
                "deleted",
                "COALESCE(lastWriteTime,0)",
                direction,
                Some(&receipt),
            );
            let mut elapsed = Vec::new();
            for _ in 0..30 {
                let started = std::time::Instant::now();
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt
                    .query_map([2], |row| row.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                let ids = rows
                    .iter()
                    .map(|row| {
                        serde_json::from_str::<Value>(row)
                            .map(|value| value["id"].as_str().unwrap().to_owned())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                assert_eq!(ids, expected);
                maximum_steps =
                    maximum_steps.max(stmt.get_status(rusqlite::StatementStatus::VmStep));
                elapsed.push(started.elapsed().as_micros());
            }
            elapsed.sort_unstable();
            eprintln!("ordered_native_intake direction={direction} rows=20005 samples=30 p50_us={} p95_us={} max_vm_steps={maximum_steps}", elapsed[14], elapsed[28]);
        }
        // Structural bound, not a machine-speed timing assertion: five actual
        // candidates must not visit twenty thousand unrelated accepted records.
        assert!(
            maximum_steps < 10_000,
            "unrelated history scanned: {maximum_steps} VM steps"
        );
        Ok(())
    }

    #[test]
    fn receipt_selection_uses_indexes_with_large_unrelated_history() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("domain.sqlite3");
        proof(&path, "applied")?;
        let conn = commands()?;
        conn.execute_batch("WITH RECURSIVE numbers(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM numbers WHERE n<20000)
            INSERT INTO commands SELECT 'old-'||n,0,n,
            json_object('id','old-'||n,'status','accepted','terminal_status','none','command_type','ctox.unrelated')
            FROM numbers;")?;
        insert(
            &conn,
            "applied",
            "accepted",
            "none",
            domain_effect::COMMAND_TYPES[0],
        )?;
        let predicate =
            retry_predicate(&path, &conn, "commands", "deleted")?.context("predicate missing")?;
        let sql = format!("SELECT COUNT(*) FROM commands WHERE deleted=0 AND {predicate}");
        let plan = conn
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?
            .query_map([], |r| r.get::<_, String>(3))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        assert!(
            plan.iter()
                .any(|s| s.contains("SEARCH commands USING INDEX commands_type")),
            "{plan:?}"
        );
        assert!(
            plan.iter().any(|s| s.contains("SEARCH receipt")),
            "{plan:?}"
        );
        let mut timings = Vec::new();
        for _ in 0..30 {
            let start = std::time::Instant::now();
            let count: i64 = conn.query_row(&sql, [], |r| r.get(0))?;
            assert_eq!(count, 1);
            timings.push(start.elapsed().as_micros());
        }
        timings.sort_unstable();
        // Same connection, rows and production-shaped indexes: measure the
        // original status-index choice separately from the corrected query.
        let broad_sql = sql.replace("+json_extract", "json_extract");
        let mut broad_timings = Vec::new();
        for _ in 0..30 {
            let start = std::time::Instant::now();
            let count: i64 = conn.query_row(&broad_sql, [], |r| r.get(0))?;
            assert_eq!(count, 1);
            broad_timings.push(start.elapsed().as_micros());
        }
        broad_timings.sort_unstable();
        eprintln!(
            "domain_receipt_selection_original same_fixture=true samples=30 p50_us={} p95_us={}",
            broad_timings[14], broad_timings[28]
        );
        eprintln!(
            "domain_receipt_selection rows=20001 samples=30 p50_us={} p95_us={} plan={plan:?}",
            timings[14], timings[28]
        );
        Ok(())
    }
}
