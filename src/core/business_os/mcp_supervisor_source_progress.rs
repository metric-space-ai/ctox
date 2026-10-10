// Origin: CTOX
// License: AGPL-3.0-only
//! Native Messages text, not SDK output or a completed Supervisor result.
//! The model callback only enqueues/extracts. This drain runs after its guard
//! has returned, inside the retained ORIGINAL native controller reservation.
use super::*;
use crate::business_os::workjet_supervisor_execution_contract::{NativeMessageText, WireValidate};

pub(super) struct Draft {
    pub(super) model: String,
    pub(super) message: String,
    pub(super) request: String,
    pub(super) text: String,
    pub(super) offset: usize,
    pub(super) completed: bool,
}
pub(super) fn publish_in_current(
    core: &Connection,
    controller: &NativeSupervisorHoldingController,
    operation: &str,
    draft: &Draft,
) -> anyhow::Result<()> {
    let lease = &controller.lease;
    // Existing signed service command/attempt only. Confirmed-plan steps without
    // an ordinary command are not relabelled as conversation turns.
    let Some(command) = lease.trusted["command_id"]
        .as_str()
        .filter(|s| !s.is_empty())
    else {
        return Ok(());
    };
    let task = lease.trusted["workjet_supervisor_lease"]["task_id"]
        .as_str()
        .context("native progress task missing")?;
    let attempt = lease.trusted["workjet_supervisor_lease"]["lease_worker_id"]
        .as_str()
        .context("native progress attempt missing")?;
    let linked: bool = core.query_row(
        "SELECT EXISTS(SELECT 1 FROM business_command_task_links
        WHERE command_id=?1 AND task_id=?2)",
        params![command, task],
        |row| row.get(0),
    )?;
    anyhow::ensure!(linked, "native progress command/task differs");
    let request: bool = core.query_row(
        "SELECT EXISTS(SELECT 1 FROM workjet_supervisor_native_model_requests
        WHERE operation_id=?1 AND controller_id=?2 AND execution_key=?3 AND lease_hash=?4
          AND operation_kind='messages' AND state='accepted')",
        params![
            operation,
            controller.controller_id(),
            lease.execution_key,
            lease.lease_hash
        ],
        |row| row.get(0),
    )?;
    anyhow::ensure!(request, "native progress request retired");
    publish_chunks(
        core,
        command,
        task,
        attempt,
        &lease.execution_key,
        operation,
        draft,
    )
}
fn publish_chunks(
    core: &Connection,
    command: &str,
    task: &str,
    attempt: &str,
    execution: &str,
    operation: &str,
    draft: &Draft,
) -> anyhow::Result<()> {
    use sha2::Digest;
    crate::service::harness_flow::ensure_event_schema(core)?;
    let chars: Vec<char> = draft.text.chars().collect();
    anyhow::ensure!(
        draft.offset.saturating_add(chars.len()) <= 65536,
        "native public text exceeds budget"
    );
    let parts: Vec<String> = if chars.is_empty() {
        vec![String::new()]
    } else {
        chars
            .chunks(4096)
            .map(|part| part.iter().collect())
            .collect()
    };
    let mut offset = draft.offset;
    for (index, text) in parts.iter().enumerate() {
        let chunk = NativeMessageText {
            execution_key: execution.to_owned(),
            model_operation_id: operation.to_owned(),
            native_message_id: draft.message.clone(),
            model: draft.model.clone(),
            upstream_request_id: draft.request.clone(),
            offset: offset as u64,
            text: text.clone(),
            completed: draft.completed && index + 1 == parts.len(),
        };
        chunk.validate().map_err(anyhow::Error::msg)?;
        let metadata = json!({"attempt_id":attempt,"command_id":command,
            "native_message_text":chunk,"cockpit_eligible":false});
        let metadata = serde_json::to_string(&metadata)?;
        let id = format!(
            "native-message-text:{:x}",
            sha2::Sha256::digest(serde_json::to_vec(&(
                execution,
                operation,
                task,
                attempt,
                &draft.message,
                offset,
                chunk.completed,
                text
            ))?)
        );
        let existing: Option<String> = core
            .query_row(
                "SELECT metadata_json FROM ctox_harness_flow_events WHERE event_id=?1",
                [&id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            anyhow::ensure!(existing == metadata, "native message chunk replay differs");
        } else {
            core.execute(
                "INSERT INTO ctox_harness_flow_events
              (event_id,chain_key,event_kind,title,body_text,message_key,metadata_json,created_at)
              VALUES (?1,?2,'worker.native_message_text','Assistant response','',?3,?4,?5)",
                params![
                    id,
                    format!("message:{task}"),
                    task,
                    metadata,
                    chrono::Utc::now().to_rfc3339()
                ],
            )?;
        }
        offset += text.chars().count();
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_chunks_are_unicode_bounded_replayable_and_rolled_back_with_reservation(
    ) -> anyhow::Result<()> {
        let mut core = Connection::open_in_memory()?;
        let (root, token) = super::super::super::super::super::tests::fixture(true)?;
        let lease = NativeSupervisorExecutionLease::capture(root.path(), &token)?;
        let draft = Draft {
            model: lease.requested.model.clone(),
            message: "actual-message".into(),
            request: "actual-request".into(),
            text: "🦊".repeat(4100),
            offset: 0,
            completed: true,
        };
        let tx = core.transaction_with_behavior(TransactionBehavior::Immediate)?;
        publish_chunks(
            &tx,
            "command",
            "task",
            "attempt",
            "execution",
            "model-op",
            &draft,
        )?;
        tx.rollback()?;
        assert!(!core.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='ctox_harness_flow_events')",
            [],
            |r| r.get::<_, bool>(0)
        )?);
        publish_chunks(
            &core,
            "command",
            "task",
            "attempt",
            "execution",
            "model-op",
            &draft,
        )?;
        publish_chunks(
            &core,
            "command",
            "task",
            "attempt",
            "execution",
            "model-op",
            &draft,
        )?;
        assert_eq!(
            core.query_row("SELECT count(*) FROM ctox_harness_flow_events", [], |r| r
                .get::<_, i64>(
                0
            ))?,
            2
        );
        let values: Vec<String> = core
            .prepare("SELECT metadata_json FROM ctox_harness_flow_events ORDER BY rowid")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let values: Vec<Value> = values
            .iter()
            .map(|v| serde_json::from_str(v))
            .collect::<Result<_, _>>()?;
        assert_eq!(values[0]["native_message_text"]["offset"], 0);
        assert_eq!(values[1]["native_message_text"]["offset"], 4096);
        assert_eq!(values[0]["native_message_text"]["completed"], false);
        assert_eq!(values[1]["native_message_text"]["completed"], true);
        assert_eq!(values[0]["attempt_id"], "attempt");
        assert!(values[0]["native_message_text"].get("turn_id").is_none());
        Ok(())
    }
}
