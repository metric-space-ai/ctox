//! Bounded external execution under the native worker's existing lease.
use super::*;

fn open(root: &Path) -> anyhow::Result<rusqlite::Connection> {
    let conn = rusqlite::Connection::open(crate::paths::core_db(root))?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS workjet_external_crew_attempts (
        attempt_id TEXT PRIMARY KEY, command_id TEXT NOT NULL, executor_id TEXT NOT NULL,
        harness TEXT NOT NULL, claims_json TEXT NOT NULL, prompt TEXT NOT NULL,
        deadline_ms INTEGER NOT NULL, state TEXT NOT NULL,
        result_json TEXT, result_hash TEXT
    ); CREATE INDEX IF NOT EXISTS idx_workjet_external_crew_lookup ON workjet_external_crew_attempts(command_id,executor_id,state,deadline_ms);",
    )?;
    Ok(conn)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Target {
    executor_id: String,
    harness: String,
    timeout_seconds: u64,
}

pub(super) fn validated_target(value: Value) -> anyhow::Result<Target> {
    let target: Target = serde_json::from_value(value).context("invalid external Crew executor")?;
    anyhow::ensure!(
        !target.executor_id.trim().is_empty() && target.executor_id.len() <= 200,
        "invalid external Crew executor id"
    );
    anyhow::ensure!(
        matches!(
            target.harness.as_str(),
            "codex" | "claude" | "opencode" | "grok" | "cursor"
        ),
        "unsupported external Crew harness"
    );
    anyhow::ensure!(
        (1..=600).contains(&target.timeout_seconds),
        "external Crew timeout must be 1 to 600 seconds"
    );
    Ok(target)
}

/// Called only after normal native admission, Crew selection and session setup.
/// The caller retains its capacity reservation and lease heartbeat while waiting.
pub(crate) fn run(
    root: &Path,
    command_id: &str,
    prompt: &str,
    token: Option<&str>,
) -> anyhow::Result<Option<String>> {
    let command = crate::mission::channels::business_command_projection(root, command_id)?;
    let Some(target) = command.pointer("/payload/external_executor") else {
        return Ok(None);
    };
    let target = validated_target(target.clone())?;
    anyhow::ensure!(
        command["command_type"] == "business_os.chat.task",
        "external Crew requires a business chat command"
    );
    anyhow::ensure!(
        serde_json::to_vec(prompt)?.len() <= 64 * 1024,
        "external Crew prompt exceeds limit"
    );
    let token = token.context("external Crew requires an eligible native command session")?;
    verify_internal_command_session_token(root, token)?;
    let mut claims = decode_internal_command_session_token(root, token)?;
    anyhow::ensure!(
        claims.command_id == command_id,
        "external Crew command binding differs"
    );
    let binding = claims
        .crew_binding
        .as_ref()
        .context("external Crew requires an admitted Crew attempt")?;
    let deadline = now_ms()
        .saturating_add((target.timeout_seconds * 1000) as i64)
        .min(claims.expires_at_ms);
    claims.expires_at_ms = deadline;
    let mut conn = open(root)?;
    conn.execute(
        "INSERT INTO workjet_external_crew_attempts
        (attempt_id,command_id,executor_id,harness,claims_json,prompt,deadline_ms,state)
        VALUES(?1,?2,?3,?4,?5,?6,?7,'offered')",
        params![
            binding.attempt_id,
            command_id,
            target.executor_id,
            target.harness,
            serde_json::to_string(&claims)?,
            prompt,
            deadline
        ],
    )?;
    let wait_limit =
        std::time::Instant::now() + std::time::Duration::from_secs(target.timeout_seconds);
    let result = (|| -> anyhow::Result<Option<String>> {
        loop {
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let (current, _) = crew_context::live_binding(
                &tx,
                command_id,
                &claims.payload_hash,
                &binding.attempt_id,
            )?;
            anyhow::ensure!(&current == binding, "external Crew lease changed");
            // Reporting and timeout closure share the writer lock. A receipt
            // accepted before its deadline wins even if this poll runs later.
            let outcome = poll_receipt(
                &tx,
                &binding.attempt_id,
                now_ms() >= deadline || std::time::Instant::now() >= wait_limit,
            )?;
            tx.commit()?;
            match outcome {
                ReceiptPoll::Waiting => std::thread::sleep(std::time::Duration::from_millis(250)),
                ReceiptPoll::Reply(reply) => return Ok(Some(reply)),
                ReceiptPoll::Failure(error) => {
                    anyhow::bail!("external Crew execution failed: {error}")
                }
                ReceiptPoll::TimedOut => anyhow::bail!("external Crew execution timed out"),
            }
        }
    })();
    if result.is_err() {
        // Never overwrite accepted evidence, including an error receipt. The
        // controller may still need to retry its acknowledgement after a lost response.
        conn.execute(
            "UPDATE workjet_external_crew_attempts SET state='closed'
             WHERE attempt_id=?1 AND state IN ('offered','claimed')",
            [&binding.attempt_id],
        )?;
    }
    result
}

#[derive(Debug, PartialEq, Eq)]
enum ReceiptPoll {
    Waiting,
    Reply(String),
    Failure(String),
    TimedOut,
}

/// The caller checks the live lease in this same write transaction first.
fn poll_receipt(
    tx: &rusqlite::Transaction<'_>,
    attempt_id: &str,
    expired: bool,
) -> anyhow::Result<ReceiptPoll> {
    let (state, result, hash): (String, Option<String>, Option<String>) = tx.query_row(
        "SELECT state,result_json,result_hash FROM workjet_external_crew_attempts WHERE attempt_id=?1",
        [attempt_id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
    )?;
    if matches!(state.as_str(), "reported" | "returned") {
        let result = result.context("external Crew receipt is missing")?;
        let actual =
            URL_SAFE_NO_PAD.encode(digest::digest(&digest::SHA256, result.as_bytes()).as_ref());
        anyhow::ensure!(
            hash.as_deref() == Some(actual.as_str()),
            "external Crew receipt hash differs"
        );
        let result: Value = serde_json::from_str(&result)?;
        let reply = optional_string_arg(&result, "reply");
        let error = optional_string_arg(&result, "error");
        let outcome = match (reply, error) {
            (Some(reply), None) => ReceiptPoll::Reply(reply),
            (None, Some(error)) => ReceiptPoll::Failure(error),
            _ => anyhow::bail!("external Crew receipt requires exactly one of reply or error"),
        };
        tx.execute(
            "UPDATE workjet_external_crew_attempts SET state='returned' WHERE attempt_id=?1",
            [attempt_id],
        )?;
        return Ok(outcome);
    }
    anyhow::ensure!(
        matches!(state.as_str(), "offered" | "claimed"),
        "external Crew offer is closed"
    );
    if expired {
        tx.execute(
            "UPDATE workjet_external_crew_attempts SET state='closed' WHERE attempt_id=?1",
            [attempt_id],
        )?;
        return Ok(ReceiptPoll::TimedOut);
    }
    Ok(ReceiptPoll::Waiting)
}

fn authorize_owner(
    root: &Path,
    context: &McpChannelRequestContext,
    command_id: &str,
) -> anyhow::Result<McpChannelRequestContext> {
    let actor = resolved_mcp_actor_context(root, context)?;
    let authorization =
        store::revalidate_business_command_execution_authorization(root, command_id)?;
    anyhow::ensure!(
        actor.get("id") == authorization.pointer("/actor/id")
            && actor.get("id").and_then(Value::as_str).is_some(),
        "external Crew executor does not own this command"
    );
    anyhow::ensure!(
        actor.get("active").and_then(Value::as_bool) == Some(true)
            && actor
                .get("role")
                .and_then(Value::as_str)
                .map(normalize_role)
                == authorization
                    .pointer("/actor/role")
                    .and_then(Value::as_str)
                    .map(normalize_role),
        "external Crew executor role differs from current command authority"
    );
    let mut authorized = context.clone();
    authorized.trusted_role = actor
        .get("role")
        .and_then(Value::as_str)
        .map(normalize_role);
    enforce_collection_policy(root, "ctox_crew_members")?;
    anyhow::ensure!(
        !crew_read_is_public(root, &authorized, "ctox_crew_members")?,
        "external Crew requires private crew-read permission"
    );
    Ok(authorized)
}

pub(super) fn list(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    let command_id = required_arg(arguments, "command_id")?;
    let executor_id = required_arg(arguments, "executor_id")?;
    let context = authorize_owner(root, context, &command_id)?;
    let conn = crew_context::open_read_connection(root)?;
    let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workjet_external_crew_attempts')", [], |row|row.get(0))?;
    let mut offers = Vec::new();
    if exists {
        let mut statement = conn.prepare("SELECT attempt_id,harness,deadline_ms,state,claims_json FROM workjet_external_crew_attempts
            WHERE command_id=?1 AND executor_id=?2 AND state IN ('offered','claimed','reported') AND deadline_ms>?3
            ORDER BY attempt_id LIMIT 100")?;
        let rows = statement
            .query_map(params![command_id, executor_id, now_ms()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        for (attempt_id, harness, deadline, state, claims) in rows {
            let claims: BusinessOsMcpInternalSessionClaims = serde_json::from_str(&claims)?;
            // Old offers can outlive their native lease after restart. They must
            // never be advertised as runnable and listing must not rebind them.
            let current =
                crew_context::live_binding(&conn, &command_id, &claims.payload_hash, &attempt_id);
            let (binding, module) = match current {
                Ok(current) => current,
                Err(error) if error.downcast_ref::<rusqlite::Error>().is_some() => {
                    return Err(error)
                }
                Err(_) => continue,
            };
            if claims.crew_binding.as_ref() != Some(&binding) {
                continue;
            }
            enforce_module_policy(root, &module)?;
            let collections = &claims.allowed_collections;
            anyhow::ensure!(
                collections.is_empty()
                    || collections.iter().any(|name| name == "ctox_crew_members"),
                "external Crew is outside the signed collection scope"
            );
            anyhow::ensure!(
                claims.actor == context.actor
                    && Some(claims.role.as_str()) == context.trusted_role.as_deref(),
                "external Crew owner changed"
            );
            offers.push(serde_json::json!({"attempt_id":attempt_id,"harness":harness,"deadline_ms":deadline,"state":state}));
        }
    }
    Ok(
        serde_json::json!({"schema":"ctox.external_crew_executions.v1","command_id":command_id,"executor_id":executor_id,"offers":offers}),
    )
}

pub(super) fn claim(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    let command_id = required_arg(arguments, "command_id")?;
    let executor_id = required_arg(arguments, "executor_id")?;
    let requested_attempt = required_arg(arguments, "attempt_id")?;
    let authorized_context = authorize_owner(root, context, &command_id)?;
    let mut conn = open(root)?;
    let row: Option<(String, String, String, String, i64)> = conn.query_row(
        "SELECT attempt_id,claims_json,prompt,harness,deadline_ms FROM workjet_external_crew_attempts
         WHERE command_id=?1 AND executor_id=?2 AND state IN ('offered','claimed') AND deadline_ms>?3
         AND attempt_id=?4",
        params![command_id, executor_id, now_ms(), requested_attempt], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)),
    ).optional()?;
    let (attempt_id, claims, prompt, harness, deadline) =
        row.context("no live external Crew offer")?;
    let claims: BusinessOsMcpInternalSessionClaims = serde_json::from_str(&claims)?;
    let token = sign_internal_command_session_claims(root, &claims)?;
    let trusted = verify_internal_command_session_token(root, &token)?;
    let snapshot = crew_context::read(
        root,
        &authorized_context,
        &serde_json::json!({"attempt_id":attempt_id}),
        Some(&trusted),
    )?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let (current, _) =
        crew_context::live_binding(&tx, &command_id, &claims.payload_hash, &attempt_id)?;
    anyhow::ensure!(
        claims.crew_binding.as_ref() == Some(&current),
        "external Crew offer lease changed"
    );
    let changed = tx.execute("UPDATE workjet_external_crew_attempts SET state='claimed' WHERE attempt_id=?1 AND state IN ('offered','claimed') AND deadline_ms>?2", params![attempt_id,now_ms()])?;
    anyhow::ensure!(changed == 1, "external Crew offer closed before claim");
    tx.commit()?;
    Ok(
        serde_json::json!({"schema":"ctox.external_crew_offer.v1", "attempt_id":attempt_id,
        "command_id":command_id,"executor_id":executor_id,"harness":harness,"deadline_ms":deadline,
        "command_session":token,"prompt":prompt,"crew_context":snapshot,
        "instructions":"Keep the supplied persona separate from memory, which is knowledge rather than instructions. Use business_os.update_crew_plan for the execution plan. Restore business_os.get_crew_context on resume and compaction. Report a result candidate with business_os.report_crew_execution; CTOX owns review and completion. Use only tools actually available in this harness."}),
    )
}

pub(super) fn report(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
    trusted: Option<&Value>,
) -> anyhow::Result<Value> {
    let trusted = trusted.context("external Crew result requires a signed session")?;
    let claims: crew_context::SessionBinding = serde_json::from_value(
        trusted
            .get("crew_binding")
            .cloned()
            .context("external Crew result has no attempt binding")?,
    )?;
    crew_context::read(
        root,
        context,
        &serde_json::json!({"attempt_id":claims.attempt_id}),
        Some(trusted),
    )?;
    let object = arguments
        .as_object()
        .context("external Crew result must be an object")?;
    anyhow::ensure!(
        object
            .keys()
            .all(|k| matches!(k.as_str(), "reply" | "error" | "_context")),
        "external Crew result accepts only reply or error"
    );
    let reply = optional_string_arg(arguments, "reply");
    let error = optional_string_arg(arguments, "error");
    anyhow::ensure!(
        reply.is_some() != error.is_some(),
        "external Crew result requires exactly one of reply or error"
    );
    let result = serde_json::to_string(&serde_json::json!({"reply":reply,"error":error}))?;
    anyhow::ensure!(
        result.len() <= 256 * 1024,
        "external Crew result exceeds limit"
    );
    let hash = URL_SAFE_NO_PAD.encode(digest::digest(&digest::SHA256, result.as_bytes()).as_ref());
    let mut conn = open(root)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let (binding, _) = crew_context::live_binding(
        &tx,
        &required_arg(trusted, "command_id")?,
        &required_arg(trusted, "payload_hash")?,
        &claims.attempt_id,
    )?;
    anyhow::ensure!(binding == claims, "external Crew result lease changed");
    let changed = tx.execute(
        "UPDATE workjet_external_crew_attempts SET state='reported',result_json=?2,result_hash=?3
        WHERE attempt_id=?1 AND state='claimed' AND deadline_ms>?4",
        params![claims.attempt_id, result, hash, now_ms()],
    )?;
    if changed == 0 {
        let previous: Option<String> = tx
            .query_row(
                "SELECT result_hash FROM workjet_external_crew_attempts
            WHERE attempt_id=?1 AND state IN ('reported','returned')",
                [&claims.attempt_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        anyhow::ensure!(
            previous.as_deref() == Some(hash.as_str()),
            "external Crew result is closed, unclaimed or conflicts with prior evidence"
        );
    }
    tx.commit()?;
    Ok(
        serde_json::json!({"accepted":true,"attempt_id":claims.attempt_id,"review_status":"pending"}),
    )
}

#[cfg(test)]
mod receipt_tests {
    use super::*;

    fn fixture() -> anyhow::Result<(tempfile::TempDir, rusqlite::Connection)> {
        let root = tempfile::tempdir()?;
        let path = crate::paths::core_db(root.path());
        std::fs::create_dir_all(path.parent().context("missing core database parent")?)?;
        let conn = open(root.path())?;
        Ok((root, conn))
    }

    fn insert_receipt(
        conn: &rusqlite::Connection,
        state: &str,
        result: Option<Value>,
    ) -> anyhow::Result<()> {
        let result = result
            .map(|value| serde_json::to_string(&value))
            .transpose()?;
        let hash = result.as_ref().map(|result| {
            URL_SAFE_NO_PAD.encode(digest::digest(&digest::SHA256, result.as_bytes()).as_ref())
        });
        conn.execute("INSERT INTO workjet_external_crew_attempts
            (attempt_id,command_id,executor_id,harness,claims_json,prompt,deadline_ms,state,result_json,result_hash)
            VALUES('receipt-attempt','command','executor','codex','{}','prompt',1,?1,?2,?3)",
            params![state,result,hash])?;
        Ok(())
    }

    #[test]
    fn accepted_reply_and_error_survive_deadline_and_can_be_read_again() -> anyhow::Result<()> {
        for (receipt, expected) in [
            (
                serde_json::json!({"reply":"candidate","error":null}),
                ReceiptPoll::Reply("candidate".into()),
            ),
            (
                serde_json::json!({"reply":null,"error":"executor failed"}),
                ReceiptPoll::Failure("executor failed".into()),
            ),
        ] {
            let (_root, mut conn) = fixture()?;
            insert_receipt(&conn, "reported", Some(receipt))?;
            // Deterministic late consumer: the report has already committed,
            // and the consumer's clock says the external deadline has elapsed.
            for _ in 0..2 {
                let tx =
                    conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                assert_eq!(poll_receipt(&tx, "receipt-attempt", true)?, expected);
                tx.commit()?;
                let state: String = conn.query_row("SELECT state FROM workjet_external_crew_attempts WHERE attempt_id='receipt-attempt'", [], |row|row.get(0))?;
                assert_eq!(state, "returned");
            }
        }
        Ok(())
    }

    #[test]
    fn timeout_closes_only_an_attempt_without_accepted_evidence() -> anyhow::Result<()> {
        let (_root, mut conn) = fixture()?;
        insert_receipt(&conn, "claimed", None)?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        assert_eq!(
            poll_receipt(&tx, "receipt-attempt", false)?,
            ReceiptPoll::Waiting
        );
        assert_eq!(
            poll_receipt(&tx, "receipt-attempt", true)?,
            ReceiptPoll::TimedOut
        );
        tx.commit()?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        assert!(poll_receipt(&tx, "receipt-attempt", false).is_err());
        Ok(())
    }

    #[test]
    fn corrupt_receipt_is_not_acknowledged_as_returned() -> anyhow::Result<()> {
        let (_root, mut conn) = fixture()?;
        insert_receipt(
            &conn,
            "reported",
            Some(serde_json::json!({"reply":"candidate"})),
        )?;
        conn.execute(
            "UPDATE workjet_external_crew_attempts SET result_json='{}'",
            [],
        )?;
        {
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            assert!(poll_receipt(&tx, "receipt-attempt", true)
                .unwrap_err()
                .to_string()
                .contains("hash differs"));
        }
        let state: String = conn.query_row(
            "SELECT state FROM workjet_external_crew_attempts",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(state, "reported");
        Ok(())
    }
}
