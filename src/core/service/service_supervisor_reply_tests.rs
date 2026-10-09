// Origin: CTOX
// License: AGPL-3.0-only

use super::*;
use serde_json::json;

fn fixture() -> Result<(tempfile::TempDir, QueuedPrompt, String)> {
    let (temp, command_id) =
        crate::business_os::mcp_channel::workjet_dispatch_service_test_fixture()?;
    let task = channels::load_queue_task_for_business_os_command(temp.path(), &command_id)?
        .context("missing native Supervisor task")?;
    let job = queued_prompt_from_queue_task(task);
    Ok((temp, job, command_id))
}

#[test]
fn supervisor_reply_completes_one_durable_turn_without_an_external_reviewer() -> Result<()> {
    let (temp, job, command_id) = fixture()?;
    let root = temp.path();
    let key = &job.leased_message_keys[0];
    let reply = "Der Arbeitsauftrag ist noch offen. Ich kann dir den nächsten Schritt erklären.";
    let before = channels::inspect_business_command(root, &command_id)?.unwrap();
    let attempt = before["command"]["attempt"].clone();

    assert_eq!(
        persist_typed_business_command_result(root, &job, reply)?,
        Some(key.clone())
    );
    let state = Arc::new(Mutex::new(SharedState::default()));
    // No model/account is configured by this fixture. This path must not start
    // a second 900-second model turn merely to accept a conversation response.
    assert!(supervisor_conversation_reply_ready(root, &job)?);
    let disposition = run_completion_review(root, &state, &job, reply, 1, None);
    assert!(matches!(
        disposition,
        CompletionReviewDisposition::ReplyValidated
    ));
    record_typed_business_command_review(root, key, &disposition, None, "reply-turn", None)?;
    // Re-entering finalization after its durable policy record is safe too.
    assert!(supervisor_conversation_reply_ready(root, &job)?);
    let resumed = run_completion_review(root, &state, &job, reply, 1, None);
    assert!(matches!(
        resumed,
        CompletionReviewDisposition::ReplyValidated
    ));
    record_typed_business_command_review(root, key, &resumed, None, "reply-turn", None)?;

    let core = Connection::open(crate::paths::core_db(root))?;
    let evidence: String = core.query_row(
        "SELECT review_evidence_json FROM business_command_results WHERE command_id=?1",
        [&command_id],
        |row| row.get(0),
    )?;
    let evidence: Value = serde_json::from_str(&evidence)?;
    assert_eq!(evidence["disposition"], "reply_validated");
    assert_eq!(
        evidence["policy_id"],
        "workjet.supervisor.conversation-reply.v1"
    );
    drop(core);

    assert!(channels::transition_business_command_for_task(
        root,
        key,
        "handled",
        None,
        None,
        None,
        "native reply policy validated",
    )?);
    let after = channels::inspect_business_command(root, &command_id)?.unwrap();
    assert_eq!(after["command"]["execution_phase"], "terminal");
    assert_eq!(after["command"]["status"], "completed");
    assert_eq!(after["command"]["attempt"], attempt);
    assert_eq!(after["command"]["result"]["user_reply"], reply);
    assert_eq!(
        channels::load_queue_task_for_business_os_command(root, &command_id)?
            .unwrap()
            .route_status,
        "handled"
    );

    // A duplicate service finish may not submit/retry another conversation turn.
    assert!(channels::transition_business_command_for_task(
        root,
        key,
        "handled",
        None,
        None,
        None,
        "native reply policy validated",
    )?);
    let core = Connection::open(crate::paths::core_db(root))?;
    let links: i64 = core.query_row(
        "SELECT COUNT(*) FROM business_command_task_links WHERE command_id=?1",
        [&command_id],
        |row| row.get(0),
    )?;
    assert_eq!(links, 1);
    assert_eq!(
        channels::inspect_business_command(root, &command_id)?.unwrap()["command"]["attempt"],
        attempt
    );
    Ok(())
}

#[test]
fn supervisor_reply_requires_persisted_response_and_current_owner_binding() -> Result<()> {
    let (temp, job, command_id) = fixture()?;
    let root = temp.path();
    // A model reply or its claimed success alone does not authorize completion.
    assert!(supervisor_conversation_reply_ready(root, &job).is_err());
    persist_typed_business_command_result(root, &job, "Eine gespeicherte Antwort.")?;
    assert!(supervisor_conversation_reply_ready(root, &job)?);

    let canonical = channels::inspect_business_command(root, &command_id)?.unwrap();
    let mut conflicting = canonical["command"].clone();
    conflicting["record_id"] = json!("foreign-project");
    assert!(
        crate::business_os::mcp_channel::workjet_supervisor_reply_completion_allowed(
            root,
            &conflicting
        )
        .is_err()
    );

    let policy = crate::business_os::store::open_store(root)?;
    policy.execute(
        "UPDATE workjet_supervisor_bindings SET owner_user_id='foreign' WHERE project_id='project'",
        [],
    )?;
    assert!(!supervisor_conversation_reply_ready(root, &job)?);
    Ok(())
}

#[test]
fn supervisor_reply_policy_does_not_exempt_writebacks_or_external_work() -> Result<()> {
    let (temp, job, command_id) = fixture()?;
    let root = temp.path();
    persist_typed_business_command_result(root, &job, "Antwort.")?;
    let context = channels::inspect_business_command(root, &command_id)?.unwrap();
    for (field, value) in [
        ("mode", json!("action")),
        (
            "writeback_contract",
            json!({"mechanism":"business_command"}),
        ),
        ("external_executor", json!({"kind":"worker"})),
        ("attachments", json!([{"file_id":"required-file"}])),
        (
            "dependencies",
            json!([{"collection":"customers","record_id":"required-record"}]),
        ),
        ("attachments", Value::Null),
    ] {
        let mut command = context["command"].clone();
        command["payload"][field] = value;
        assert!(
            !crate::business_os::mcp_channel::workjet_supervisor_reply_completion_allowed(
                root, &command
            )?,
            "{field}"
        );
    }
    for kind in 0..4 {
        let mut work = job.clone();
        match kind {
            0 => work.source_label = "ticket".into(),
            1 => work.ticket_self_work_id = Some("open-work".into()),
            2 => work.suggested_skill = Some("business-os-app-module-development".into()),
            _ => work.leased_ticket_event_keys.push("ticket:event".into()),
        }
        assert!(!supervisor_conversation_reply_ready(root, &work)?);
    }
    let mut worker = context["command"].clone();
    worker["command_type"] = json!("ctox.coding.turn");
    assert!(
        !crate::business_os::mcp_channel::workjet_supervisor_reply_completion_allowed(
            root, &worker
        )?
    );
    Ok(())
}
