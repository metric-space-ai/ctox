fn command_targets_outbound_lead_writeback(command: &Value) -> bool {
    command.get("command_type").and_then(Value::as_str) == Some("business_os.chat.task")
        && command.get("module").and_then(Value::as_str) == Some("outbound-lead-generation")
        && command.pointer("/payload/mode").and_then(Value::as_str) == Some("data")
        && (command
            .pointer("/payload/writeback_contract/collection")
            .and_then(Value::as_str)
            == Some("outbound_lead_generation_leads")
            || command
                .pointer("/payload/writeback_contract/allowed_collections")
                .and_then(Value::as_array)
                .is_some_and(|collections| {
                    collections.iter().any(|collection| {
                        collection.as_str() == Some("outbound_lead_generation_leads")
                    })
                }))
}

fn validate_command_writeback_contract(command: &Value, contract: &Value) -> Result<()> {
    if command_targets_outbound_lead_writeback(command)
        || contract.get("command_type").is_some()
        || contract.get("mechanism").and_then(Value::as_str) == Some("business_command")
    {
        anyhow::ensure!(
            crate::business_os::mcp_channel::supports_command_writeback(contract),
            "writeback contract incomplete: expected mechanism=business_command, command_type=outbound.lead.research_writeback, collection=outbound_lead_generation_leads and nonempty record_ids; allowed_actions cannot substitute for this command contract"
        );
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CommandWritebackReceiptState {
    Completed,
    InFlight,
    Failed,
    Unknown(String),
    Uncorrelated(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandWritebackReceipt {
    command_id: String,
    state: CommandWritebackReceiptState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandWritebackTarget {
    parent_id: String,
    command_type: String,
    record_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandWritebackRecord {
    target: CommandWritebackTarget,
    receipts: Vec<CommandWritebackReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CommandWritebackProbe {
    NotRequired,
    InvalidContract(String),
    Records(Vec<CommandWritebackRecord>),
}

/// Capture every contracted record from one native read snapshot. Absence,
/// unfinished commands, terminal failures and ambiguous correlation remain
/// distinct; none of these states grants permission to repeat a write.
fn read_command_writeback_receipts(
    conn: &rusqlite::Connection,
    targets: Vec<CommandWritebackTarget>,
) -> Result<Vec<CommandWritebackRecord>> {
    let transaction = conn.unchecked_transaction()?;
    let mut statement = transaction.prepare(
        "SELECT command_id, status,
                CASE WHEN json_valid(payload_json)
                     THEN CASE WHEN json_type(payload_json, '$.research_command_id')='text'
                               THEN json_extract(payload_json, '$.research_command_id')
                          END
                END
         FROM business_commands WHERE command_type=?1 AND record_id=?2
         ORDER BY command_id",
    )?;
    let mut records = Vec::with_capacity(targets.len());
    for target in targets {
        let rows = statement.query_map(
            rusqlite::params![target.command_type, target.record_id],
            |row| {
                let command_id = row.get::<_, String>(0)?;
                let status = row.get::<_, String>(1)?;
                let originating_research = row.get::<_, Option<String>>(2)?;
                let origin = originating_research
                    .as_deref()
                    .filter(|id| !id.trim().is_empty());
                let state = if origin.is_none() {
                    CommandWritebackReceiptState::Uncorrelated(status)
                } else if origin != Some(target.parent_id.as_str()) {
                    // Blank detection is whitespace-aware; valid IDs are compared
                    // exactly, never trimmed into the current parent's authority.
                    return Ok(None);
                } else {
                    match status.as_str() {
                        "completed" => CommandWritebackReceiptState::Completed,
                        "pending" | "running" | "accepted" | "queued" => {
                            CommandWritebackReceiptState::InFlight
                        }
                        "failed" | "cancelled" | "rejected" => CommandWritebackReceiptState::Failed,
                        _ => CommandWritebackReceiptState::Unknown(status),
                    }
                };
                Ok(Some(CommandWritebackReceipt { command_id, state }))
            },
        )?;
        let receipts = rows
            .filter_map(|receipt| receipt.transpose())
            .collect::<rusqlite::Result<Vec<_>>>()?;
        records.push(CommandWritebackRecord { target, receipts });
    }
    Ok(records)
}

fn command_writeback_probe(root: &Path, job: &QueuedPrompt) -> Result<CommandWritebackProbe> {
    if metadata_string(&job.queue_task_metadata, "business_os_command_type").as_deref()
        != Some("business_os.chat.task")
    {
        return Ok(CommandWritebackProbe::NotRequired);
    }
    let mut targets = Vec::new();
    for key in &job.leased_message_keys {
        let Some(context) = channels::inspect_business_command_for_task(root, key)? else {
            continue;
        };
        let Some(contract) = context.pointer("/command/payload/writeback_contract") else {
            continue;
        };
        if let Err(error) = validate_command_writeback_contract(&context["command"], contract) {
            return Ok(CommandWritebackProbe::InvalidContract(error.to_string()));
        }
        if !crate::business_os::mcp_channel::supports_command_writeback(contract) {
            continue;
        }
        let parent_id = context
            .pointer("/command/command_id")
            .and_then(Value::as_str)
            .context("writeback parent command id missing")?;
        let command_type = contract["command_type"]
            .as_str()
            .context("writeback command type missing")?;
        let records = contract["record_ids"]
            .as_array()
            .context("writeback record IDs missing")?;
        for record in records {
            targets.push(CommandWritebackTarget {
                parent_id: parent_id.to_string(),
                command_type: command_type.to_string(),
                record_id: record
                    .as_str()
                    .context("writeback record id must be a string")?
                    .to_string(),
            });
        }
    }
    if targets.is_empty() {
        return Ok(CommandWritebackProbe::NotRequired);
    }
    let conn = crate::business_os::store::open_store(root)?;
    Ok(CommandWritebackProbe::Records(
        read_command_writeback_receipts(&conn, targets)?,
    ))
}

/// Bound only presentation; the probe retains complete typed evidence.
fn bounded_command_writeback_receipt_diagnostic(receipts: &[CommandWritebackReceipt]) -> String {
    const MAX_ENTRIES: usize = 8;
    const MAX_BODY_BYTES: usize = 944;
    const MAX_ID_BYTES: usize = 96;
    let mut body = String::new();
    let mut shown = 0;
    for receipt in receipts.iter().take(MAX_ENTRIES) {
        let mut end = receipt.command_id.len().min(MAX_ID_BYTES);
        while !receipt.command_id.is_char_boundary(end) {
            end -= 1;
        }
        let suffix = if end < receipt.command_id.len() {
            "..."
        } else {
            ""
        };
        let state = match &receipt.state {
            CommandWritebackReceiptState::Completed => "Completed",
            CommandWritebackReceiptState::InFlight => "InFlight",
            CommandWritebackReceiptState::Failed => "Failed",
            CommandWritebackReceiptState::Unknown(_) => "Unknown",
            CommandWritebackReceiptState::Uncorrelated(_) => "Uncorrelated",
        };
        let entry = format!("{}{}:{state}", &receipt.command_id[..end], suffix);
        let separator = if shown == 0 { "" } else { ", " };
        if body.len() + separator.len() + entry.len() > MAX_BODY_BYTES {
            break;
        }
        body.push_str(separator);
        body.push_str(&entry);
        shown += 1;
    }
    format!(
        "total={} omitted={} [{body}]",
        receipts.len(),
        receipts.len() - shown
    )
}

fn command_writeback_failure(root: &Path, job: &QueuedPrompt) -> Result<Option<String>> {
    let records = match command_writeback_probe(root, job)? {
        CommandWritebackProbe::NotRequired => return Ok(None),
        CommandWritebackProbe::InvalidContract(error) => return Ok(Some(error)),
        CommandWritebackProbe::Records(records) => records,
    };
    for record in records {
        if record
            .receipts
            .iter()
            .any(|receipt| receipt.state == CommandWritebackReceiptState::Completed)
        {
            continue;
        }
        let CommandWritebackTarget {
            parent_id,
            command_type,
            record_id,
        } = record.target;
        let observed = bounded_command_writeback_receipt_diagnostic(&record.receipts);
        return Ok(Some(format!(
            "Business command writeback failed: no successful {command_type} receipt for record {record_id} and originating research {parent_id}. CLI/shell/terminal/SQLite/direct_sql are forbidden writeback mechanisms and cannot complete this task. Research output must be retained for recovery. Observed receipts: {observed}."
        )));
    }
    Ok(None)
}

#[cfg(test)]
mod command_writeback_tests {
    use super::*;

    #[test]
    fn writeback_receipt_diagnostic_is_bounded_without_dropping_typed_evidence() {
        let receipts = (0..50)
            .map(|index| CommandWritebackReceipt {
                command_id: format!("{index}-{}", "🙂".repeat(200)),
                state: CommandWritebackReceiptState::Unknown("future-state".repeat(500)),
            })
            .collect::<Vec<_>>();
        let summary = bounded_command_writeback_receipt_diagnostic(&receipts);
        assert!(summary.len() <= 1024, "diagnostic must have a byte bound");
        assert!(summary.starts_with("total=50 omitted=42 ["));
        assert_eq!(summary.matches(":Unknown").count(), 8);
        assert_eq!(receipts.len(), 50);
        assert!(receipts[49].command_id.len() > 700);
        assert_eq!(
            bounded_command_writeback_receipt_diagnostic(&[]),
            "total=0 omitted=0 []",
        );
    }

    #[test]
    fn writeback_receipt_probe_keeps_all_records_and_uncertain_receipts() -> Result<()> {
        let conn = rusqlite::Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE business_commands (
                command_id TEXT PRIMARY KEY, command_type TEXT NOT NULL,
                record_id TEXT NOT NULL, status TEXT NOT NULL, payload_json TEXT NOT NULL
            );",
        )?;
        let parent = "research-a";
        let command_type = "outbound.lead.research_writeback";
        let payload = serde_json::json!({"research_command_id": parent}).to_string();
        for (id, record, status, body) in [
            ("done-1", "done", "completed", payload.as_str()),
            ("mixed-1", "mixed", "completed", payload.as_str()),
            ("mixed-2", "mixed", "pending", payload.as_str()),
            ("mixed-3", "mixed", "failed", payload.as_str()),
            ("waiting-1", "waiting", "running", payload.as_str()),
            ("failed-1", "failed", "cancelled", payload.as_str()),
            ("ambiguous-1", "ambiguous", "completed", "{invalid"),
            ("ambiguous-2", "ambiguous", "completed", "{}"),
            (
                "ambiguous-3",
                "ambiguous",
                "completed",
                r#"{"research_command_id":123}"#,
            ),
            (
                "ambiguous-4",
                "ambiguous",
                "completed",
                r#"{"research_command_id":""}"#,
            ),
            (
                "ambiguous-5",
                "ambiguous",
                "completed",
                r#"{"research_command_id":" \t\n\u2003 "}"#,
            ),
            (
                "ambiguous-6",
                "ambiguous",
                "future-status",
                payload.as_str(),
            ),
            (
                "padded-parent",
                "wrong-parent",
                "completed",
                r#"{"research_command_id":" research-a "}"#,
            ),
            (
                "unrelated",
                "wrong-parent",
                "completed",
                r#"{"research_command_id":"research-b"}"#,
            ),
        ] {
            conn.execute(
                "INSERT INTO business_commands VALUES (?1,?2,?3,?4,?5)",
                rusqlite::params![id, command_type, record, status, body],
            )?;
        }
        conn.execute(
            "INSERT INTO business_commands VALUES ('different-type','other.command','missing','completed',?1)",
            [&payload],
        )?;
        let ids = [
            "missing",
            "done",
            "mixed",
            "waiting",
            "failed",
            "ambiguous",
            "wrong-parent",
        ];
        let records = read_command_writeback_receipts(
            &conn,
            ids.iter()
                .map(|id| CommandWritebackTarget {
                    parent_id: parent.to_string(),
                    command_type: command_type.to_string(),
                    record_id: id.to_string(),
                })
                .collect(),
        )?;
        assert_eq!(records.len(), ids.len());
        for (record, id) in records.iter().zip(ids) {
            assert_eq!(record.target.parent_id, parent);
            assert_eq!(record.target.command_type, command_type);
            assert_eq!(record.target.record_id, id);
        }
        assert!(records[0].receipts.is_empty());
        assert_eq!(records[1].receipts[0].command_id, "done-1");
        assert_eq!(
            records[1].receipts[0].state,
            CommandWritebackReceiptState::Completed
        );
        assert_eq!(
            records[2]
                .receipts
                .iter()
                .map(|receipt| receipt.state.clone())
                .collect::<Vec<_>>(),
            vec![
                CommandWritebackReceiptState::Completed,
                CommandWritebackReceiptState::InFlight,
                CommandWritebackReceiptState::Failed,
            ],
        );
        assert_eq!(
            records[3].receipts[0].state,
            CommandWritebackReceiptState::InFlight
        );
        assert_eq!(
            records[4].receipts[0].state,
            CommandWritebackReceiptState::Failed
        );
        assert_eq!(records[5].receipts.len(), 6);
        assert!(records[5].receipts[..5].iter().all(|receipt| receipt.state
            == CommandWritebackReceiptState::Uncorrelated("completed".to_string())));
        assert_eq!(
            records[5].receipts[5].state,
            CommandWritebackReceiptState::Unknown("future-status".to_string()),
        );
        assert!(records[6].receipts.is_empty());
        Ok(())
    }

    #[test]
    fn incident_legacy_lead_contract_validation_preserves_other_data_chat_scopes() {
        let legacy_contract = serde_json::json!({
            "collection": "outbound_lead_generation_leads",
            "allowed_collections": ["outbound_lead_generation_leads"],
            "record_ids": ["lead-a"], "min_independent_sources": 2
        });
        let command = serde_json::json!({
            "command_type": "business_os.chat.task", "module": "outbound-lead-generation",
            "payload": {"mode": "data", "writeback_contract": legacy_contract}
        });
        assert!(validate_command_writeback_contract(&command, &legacy_contract).is_err());
        for (pointer, value) in [
            ("/module", "other-module"),
            ("/payload/mode", "conversation"),
            ("/command_type", "other.command"),
        ] {
            let mut other = command.clone();
            *other.pointer_mut(pointer).unwrap() = Value::String(value.to_string());
            assert!(validate_command_writeback_contract(&other, &legacy_contract).is_ok());
        }
        let mut other_collection = command.clone();
        other_collection["payload"]["writeback_contract"] = serde_json::json!({
            "collection": "other_records", "allowed_collections": ["other_records"],
            "record_ids": ["record-a"]
        });
        assert!(validate_command_writeback_contract(
            &other_collection,
            &other_collection["payload"]["writeback_contract"]
        )
        .is_ok());
        let mut missing_collection = command.clone();
        missing_collection["payload"]["writeback_contract"]
            .as_object_mut()
            .unwrap()
            .remove("collection");
        assert!(validate_command_writeback_contract(
            &missing_collection,
            &missing_collection["payload"]["writeback_contract"]
        )
        .is_err());
        let plain_chat = serde_json::json!({"command": {
            "command_type": "business_os.chat.task", "module": "outbound-lead-generation",
            "payload": {"mode": "data", "instruction": "Explain this table"}
        }});
        assert!(completion_review_scope_from_command_context(&plain_chat).is_some());
    }

    #[test]
    fn incident_incomplete_command_writeback_fails_before_turn_and_cannot_complete() -> Result<()> {
        let contracts = [
            // Persisted App 1.0.99 shape: the native fields existed only on
            // the app task envelope, not inside command.payload.
            serde_json::json!({"collection": "outbound_lead_generation_leads",
                "allowed_collections": ["outbound_lead_generation_leads"],
                "record_ids": ["lead-a"], "min_independent_sources": 2}),
            serde_json::json!({"command_type": "outbound.lead.research_writeback"}),
            serde_json::json!({"mechanism": "business_command"}),
            serde_json::json!({"mechanism": "business_command",
                "command_type": "outbound.lead.research_writeback",
                "collection": "outbound_lead_generation_leads", "record_ids": []}),
            serde_json::json!({"command_type": "outbound.lead.research_writeback",
                "allowed_actions": [{"module_id": "outbound-lead-generation",
                    "action_id": "web_stack.person_research"}]}),
        ];
        for contract in contracts {
            let temp = tempfile::tempdir()?;
            let root = temp.path();
            let (capability, _) =
                crate::business_os::store::issue_business_os_capability_token_for_managed_user(
                    root,
                    "operator",
                    "Operator",
                    "admin",
                    chrono::Utc::now().timestamp_millis(),
                )?;
            let accepted = crate::business_os::store::accept_rxdb_business_command_with_origin(
                root,
                serde_json::json!({
                    "id": "incomplete-research", "module": "outbound-lead-generation",
                    "command_type": "business_os.chat.task", "record_id": "lead-a",
                    "payload": {"instruction": "Research and write back", "mode": "data",
                        "writeback_contract": contract},
                    "client_context": {"capability_token": capability}
                }),
                crate::business_os::store::CommandOrigin::ReplicatedPeer,
            )?;
            let key = accepted["task_id"].as_str().context("queue task missing")?;
            let job = queued_prompt_from_queue_task(
                channels::load_queue_task(root, key)?.context("queue task missing")?,
            );
            let context = channels::inspect_business_command_for_task(root, key)?.unwrap();
            assert_eq!(
                context.pointer("/command/payload/writeback_contract"),
                Some(&contract)
            );
            let mut options = chat_turn_session_options_for_queue_job(&job);
            let error = configure_business_os_mcp_session_for_queue_job(root, &job, &mut options)
                .expect_err("an incomplete writeback contract must stop the turn")
                .to_string();
            assert!(error.contains("writeback contract incomplete"), "{error}");
            assert!(options.business_os_mcp_command_session.is_none());
            assert!(!runtime_error_is_transient_api_failure(&error));
            assert!(!founder_email_worker_error_is_retryable(&job, &error));
            assert!(!matches!(
                classify_agent_failure(&error),
                lcm::AgentOutcome::TurnTimeout
            ));
            assert_eq!(failed_worker_route_status(false, false, false), "failed");
            let context = channels::inspect_business_command_for_task(root, key)?.unwrap();
            assert!(completion_review_scope_from_command_context(&context).is_none());
            let state = Arc::new(Mutex::new(SharedState::default()));
            match run_completion_review(root, &state, &job, "Research completed", 1, None) {
                CompletionReviewDisposition::TerminalQueueFailure { summary } => {
                    assert!(
                        summary.contains("writeback contract incomplete"),
                        "{summary}"
                    );
                }
                _ => panic!("resumed work must not complete with an incomplete writeback contract"),
            }
        }
        Ok(())
    }

    #[test]
    fn incident_command_only_contract_enables_mcp_and_requires_successful_correlated_writeback(
    ) -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let command_id = "incident_research_command";
        let (capability, _) =
            crate::business_os::store::issue_business_os_capability_token_for_managed_user(
                root,
                "operator",
                "Operator",
                "admin",
                chrono::Utc::now().timestamp_millis(),
            )?;
        let accepted = crate::business_os::store::accept_rxdb_business_command_with_origin(
            root,
            serde_json::json!({
                "id": command_id, "command_id": command_id, "module": "outbound-lead-generation",
                "command_type": "business_os.chat.task", "record_id": "lead-a",
                "payload": {"title": "Research lead", "instruction": "Research and write back",
                    "mode": "data", "writeback_contract": {
                        "mechanism": "business_command", "command_type": "outbound.lead.research_writeback",
                        "collection": "outbound_lead_generation_leads", "record_ids": ["lead-a"],
                        "forbidden_mechanisms": ["cli", "shell", "sqlite"]
                    }},
                "client_context": {"capability_token": capability}
            }),
            crate::business_os::store::CommandOrigin::ReplicatedPeer,
        )?;
        let key = accepted["task_id"]
            .as_str()
            .context("test research queue task missing")?;
        let task = channels::load_queue_task(root, key)?.context("test queue task missing")?;
        let job = queued_prompt_from_queue_task(task);
        let mut options = chat_turn_session_options_for_queue_job(&job);
        assert!(configure_business_os_mcp_session_for_queue_job(
            root,
            &job,
            &mut options
        )?);
        assert!(options.enable_business_os_mcp);
        assert!(options.business_os_mcp_command_session.is_some());
        assert!(options.force_isolated_session);
        let context = channels::inspect_business_command_for_task(root, key)?.unwrap();
        assert!(completion_review_scope_from_command_context(&context).is_none());
        let state = Arc::new(Mutex::new(SharedState::default()));
        let disposition = run_completion_review(
            root,
            &state,
            &job,
            "Research complete. CLI writeback failed because the sandbox blocked SQLite.",
            1,
            None,
        );
        match disposition {
            CompletionReviewDisposition::TerminalQueueFailure { summary } => {
                assert!(summary.contains("writeback failed"));
                assert!(summary.contains("forbidden"));
            }
            _ => panic!("forbidden writeback must fail the queue task terminally"),
        }
        let conn = crate::business_os::store::open_store(root)?;
        conn.execute("INSERT INTO business_commands
            (command_id,module,command_type,record_id,status,payload_json,client_context_json,observed_at_ms)
            VALUES ('receipt','outbound-lead-generation','outbound.lead.research_writeback','lead-a',?1,?2,'{}',1)",
            rusqlite::params!["failed", serde_json::json!({"research_command_id": command_id}).to_string()])?;
        assert!(command_writeback_failure(root, &job)?.is_some());
        conn.execute("UPDATE business_commands SET status='completed',payload_json=?1 WHERE command_id='receipt'",
            [serde_json::json!({"research_command_id": "other-research"}).to_string()])?;
        assert!(command_writeback_failure(root, &job)?.is_some());
        conn.execute(
            "UPDATE business_commands SET payload_json=?1 WHERE command_id='receipt'",
            [serde_json::json!({"research_command_id": command_id}).to_string()],
        )?;
        assert!(command_writeback_failure(root, &job)?.is_none());
        Ok(())
    }
}
