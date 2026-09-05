fn command_writeback_failure(root: &Path, job: &QueuedPrompt) -> Result<Option<String>> {
    if metadata_string(&job.queue_task_metadata, "business_os_command_type").as_deref()
        != Some("business_os.chat.task")
    {
        return Ok(None);
    }
    for key in &job.leased_message_keys {
        let Some(context) = channels::inspect_business_command_for_task(root, key)? else {
            continue;
        };
        let Some(contract) = context.pointer("/command/payload/writeback_contract") else {
            continue;
        };
        if contract.get("mechanism").and_then(Value::as_str) != Some("business_command")
            || contract.get("command_type").and_then(Value::as_str)
                != Some("outbound.lead.research_writeback")
        {
            continue;
        }
        let parent_id = context
            .pointer("/command/command_id")
            .and_then(Value::as_str)
            .context("writeback parent command id missing")?;
        if !crate::business_os::mcp_channel::supports_command_writeback(contract) {
            return Ok(Some("Business command writeback failed: unsupported or incomplete persisted writeback contract; no fallback through CLI, shell or SQLite is permitted.".into()));
        }
        let command_type = contract["command_type"]
            .as_str()
            .context("writeback command type missing")?;
        let records = contract["record_ids"]
            .as_array()
            .context("writeback record IDs missing")?;
        let conn = crate::business_os::store::open_store(root)?;
        for record in records {
            let record_id = record
                .as_str()
                .context("writeback record id must be a string")?;
            let completed: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM business_commands
                 WHERE command_type=?1 AND record_id=?2 AND status='completed'
                   AND json_valid(payload_json)
                   AND json_extract(payload_json, '$.research_command_id')=?3)",
                rusqlite::params![command_type, record_id, parent_id],
                |row| row.get(0),
            )?;
            if !completed {
                return Ok(Some(format!(
                    "Business command writeback failed: no successful {command_type} receipt for record {record_id} and originating research {parent_id}. CLI/shell/terminal/SQLite/direct_sql are forbidden writeback mechanisms and cannot complete this task. Research output must be retained for recovery."
                )));
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod command_writeback_tests {
    use super::*;

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
