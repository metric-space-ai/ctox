// Origin: CTOX
// License: AGPL-3.0-only

use super::*;
use serde_json::json;

fn fixture() -> Result<(tempfile::TempDir, QueuedPrompt, String)> {
    fixture_for_kind(Some("conversation"))
}

fn fixture_for_kind(turn_kind: Option<&str>) -> Result<(tempfile::TempDir, QueuedPrompt, String)> {
    let (temp, command_id) =
        crate::business_os::mcp_channel::workjet_dispatch_service_test_fixture()?;
    let command_id = if let Some(kind) = turn_kind {
        let original = channels::inspect_business_command(temp.path(), &command_id)?.unwrap();
        let accepted = crate::business_os::command_plane::accept_rxdb_business_command(
            temp.path(),
            json!({"id": format!("submit-reply-{kind}"), "module":"ctox",
                "command_type":"ctox.workjet.project.supervisor.turn.submit", "record_id":"project",
                "payload":{"project_id":"project", "thread_id":original["command"]["payload"]["thread_id"],
                    "goal":"Erkläre mir den nächsten Schritt.", "turn_kind":kind},
                "client_context":{"actor":{"id":"owner","role":"chef","is_admin":true}}}),
        )?;
        anyhow::ensure!(accepted["status"] == "completed", "reply submit was not admitted");
        accepted["result"]["turn"]["command_id"].as_str().context("reply command")?.to_owned()
    } else {
        command_id
    };
    let task = channels::load_queue_task_for_business_os_command(temp.path(), &command_id)?
        .context("missing native Supervisor task")?;
    // The tool fixture's manual lease has no execution attempt. Release it
    // through the queue API and acquire the real lease before starting execution.
    if task.route_status == "leased" {
        anyhow::ensure!(
            channels::ack_leased_messages(temp.path(), &[task.message_key.clone()], "pending")? == 1,
            "native Supervisor fixture lease was not released"
        );
    }
    let task = channels::lease_queue_task(temp.path(), &task.message_key, "fixture-service")?;
    for phase in ["leased", "running"] {
        anyhow::ensure!(
            channels::transition_business_command_for_task(
                temp.path(),
                &task.message_key,
                phase,
                None,
                None,
                None,
                "native Supervisor reply fixture starts execution",
            )?,
            "native Supervisor fixture transition was not applied"
        );
    }
    let task = channels::load_queue_task_for_business_os_command(temp.path(), &command_id)?
        .context("missing started native Supervisor task")?;
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

#[test]
fn supervisor_work_and_legacy_turns_retain_work_completion_review() -> Result<()> {
    for kind in [None, Some("work")] {
        let (temp, job, command_id) = fixture_for_kind(kind)?;
        let root = temp.path();
        // Like PR42: no mode, writeback or required-artifact metadata. Only the
        // explicit kind, not metadata absence or reply words, grants the policy.
        for reply in ["Der PR-Head ist nicht zugänglich; die Prüfung ist offen.", "Die Prüfung ist erledigt."] {
            persist_typed_business_command_result(root, &job, reply)?;
            assert!(!supervisor_conversation_reply_ready(root, &job)?);
        }
        let context = channels::inspect_business_command(root, &command_id)?.unwrap();
        assert_eq!(context["command"]["payload"]["supervisor_turn"]["kind"], "work");
    }
    Ok(())
}

#[test]
fn supervisor_conversation_requires_the_original_owner_submit_kind() -> Result<()> {
    let (temp, job, command_id) = fixture()?;
    let root = temp.path();
    persist_typed_business_command_result(root, &job, "Antwort.")?;
    assert!(supervisor_conversation_reply_ready(root, &job)?);
    let context = channels::inspect_business_command(root, &command_id)?.unwrap();
    let submitted = context["command"]["payload"]["supervisor_turn"]["submit_command_id"]
        .as_str().context("original Owner submit")?;
    // A copied marker cannot upgrade a different admitted work request.
    let policy = crate::business_os::store::open_store(root)?;
    assert_eq!(policy.execute(
        "UPDATE business_commands SET payload_json=json_set(payload_json,'$.turn_kind','work') WHERE command_id=?1",
        [submitted],
    )?, 1);
    assert!(supervisor_conversation_reply_ready(root, &job).is_err());
    Ok(())
}

#[test]
fn supervisor_submit_rejects_unknown_or_null_kind() -> Result<()> {
    let (temp, _, command_id) = fixture_for_kind(None)?;
    let original = channels::inspect_business_command(temp.path(), &command_id)?.unwrap();
    for (index, kind) in [Value::Null, json!("completed"), json!({"kind":"conversation"})].into_iter().enumerate() {
        let admitted = crate::business_os::command_plane::accept_rxdb_business_command(
            temp.path(),
            json!({"id":format!("invalid-kind-{index}"), "module":"ctox",
                "command_type":"ctox.workjet.project.supervisor.turn.submit", "record_id":"project",
                "payload":{"project_id":"project", "thread_id":original["command"]["payload"]["thread_id"],
                    "goal":"Frage.","turn_kind":kind},
                "client_context":{"actor":{"id":"owner","role":"chef","is_admin":true}}}),
        );
        if let Ok(admitted) = admitted {
            assert_ne!(admitted["status"], "completed");
            assert!(admitted["result"]["turn"]["command_id"].as_str().is_none());
        }
    }
    Ok(())
}

#[test]
fn generic_threads_request_cannot_copy_supervisor_conversation_provenance() -> Result<()> {
    let (temp, _, command_id) = fixture()?;
    let root = temp.path();
    let original = channels::inspect_business_command(root, &command_id)?.unwrap();
    let accepted = crate::business_os::command_plane::accept_rxdb_business_command(
        root,
        json!({"id":"generic-ai-forged-kind", "module":"threads", "command_type":"threads.ai.request",
            "record_id": original["command"]["payload"]["thread_id"],
            "payload":{"thread_id":original["command"]["payload"]["thread_id"],"goal":"Frage.",
                "supervisor_turn":original["command"]["payload"]["supervisor_turn"]},
            "client_context":{"actor":{"id":"owner","role":"chef","is_admin":true}}}),
    )?;
    assert_eq!(accepted["status"], "completed");
    let id = accepted["result"]["ai_command"]["command_id"].as_str()
        .or_else(|| accepted["result"]["ai_command"]["id"].as_str()).context("generic AI command")?;
    let generic = channels::inspect_business_command(root, id)?.unwrap();
    assert!(generic["command"]["payload"].get("supervisor_turn").is_none());
    assert!(!crate::business_os::mcp_channel::workjet_supervisor_reply_completion_allowed(root, &generic["command"])?);
    Ok(())
}
