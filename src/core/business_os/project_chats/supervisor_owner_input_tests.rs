// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use crate::mission::channels;
use channels::supervisor_owner_input;
const THREAD: &str = "cc6cfe73-2824-4360-9daf-3b3efb079931";

fn fixture() -> anyhow::Result<(TempDir, Value)> {
    let root = super::supervisor_turns::fixture()?;
    let result = super::supervisor_turns::control(
        root.path(),
        "submit",
        "owner",
        "submit",
        json!({"project_id":"project","thread_id":THREAD,"goal":"Review the existing PR sources"}),
    )?;
    Ok((root, result["result"]["turn"].clone()))
}
fn input(root: &Path, turn: &Value, id: &str, actor: &str, body: &str) -> anyhow::Result<Value> {
    super::supervisor_turns::control(
        root,
        id,
        actor,
        "input",
        json!({
            "project_id":"project", "thread_id":THREAD,
            "target_command_id":turn["command_id"], "body":body,
        }),
    )
}
fn task(turn: &Value) -> &str {
    turn["task_id"].as_str().unwrap()
}
fn begin(root: &Path, turn: &Value, worker: &str, attempt: &str) -> anyhow::Result<()> {
    channels::lease_queue_task(root, task(turn), worker)?;
    ensure!(channels::transition_business_command_for_task(
        root,
        task(turn),
        "running",
        None,
        None,
        None,
        "actual worker fixture started"
    )?);
    let engine = crate::lcm::LcmEngine::open(
        &crate::paths::core_db(root),
        crate::lcm::LcmConfig::default(),
    )?;
    engine.begin_worker_attempt_finalization(crate::lcm::WorkerAttemptFinalizationInput {
        attempt_id: attempt,
        work_key: task(turn),
        conversation_id: 42,
        source_label: "queue",
        agent_outcome: crate::lcm::AgentOutcome::Success,
        reply_text: "A bounded test slice",
        error_text: None,
    })?;
    Ok(())
}
fn passed(root: &Path, turn: &Value) -> anyhow::Result<()> {
    channels::persist_business_command_worker_result(root, task(turn), "A bounded test slice")?;
    channels::record_business_command_review(
        root,
        task(turn),
        "passed",
        "passed",
        &json!({"fixture":true}),
    )?;
    Ok(())
}
fn failed(value: anyhow::Result<Value>) -> bool {
    value.is_err()
        || value
            .as_ref()
            .is_ok_and(|v| v["ok"] == false || v["status"] == "failed")
}

#[test]
fn supervisor_owner_input_batch_waits_for_every_task_effect() -> anyhow::Result<()> {
    let (root, first) = fixture()?;
    let second = super::supervisor_turns::control(root.path(), "second-submit", "owner", "submit",
        json!({"project_id":"project","thread_id":THREAD,"goal":"Review a second bounded task"}))?
        ["result"]["turn"].clone();
    begin(root.path(), &first, "batch-worker", "batch-attempt")?;
    channels::lease_queue_task(root.path(), task(&second), "batch-worker")?;
    assert!(channels::transition_business_command_for_task(root.path(), task(&second),
        "running", None, None, None, "bounded task fixture started")?);
    for turn in [&first, &second] {
        supervisor_owner_input::capture(root.path(), task(turn), "batch-attempt", "batch-worker")?;
    }
    input(root.path(), &first, "first-late", "owner", "Use the actual first source.")?;
    input(root.path(), &second, "second-late", "owner", "Use the actual second source.")?;
    let tasks = [task(&first).to_owned(), task(&second).to_owned()];
    assert!(supervisor_owner_input::continue_pending(root.path(), task(&first), "batch-attempt")?);
    assert!(!supervisor_owner_input::mark_finished_slice(root.path(), "batch-attempt", &tasks)?);
    let db = Connection::open(crate::paths::core_db(root.path()))?;
    let marker: Option<String> = db.query_row(
        "SELECT queue_effects_applied_at FROM worker_attempt_finalizations WHERE attempt_id='batch-attempt'",
        [], |row| row.get(0))?;
    assert!(marker.is_none());
    assert!(supervisor_owner_input::continue_pending(root.path(), task(&second), "batch-attempt")?);
    assert!(supervisor_owner_input::mark_finished_slice(root.path(), "batch-attempt", &tasks)?);
    assert!(supervisor_owner_input::mark_finished_slice(root.path(), "batch-attempt", &tasks)?);
    Ok(())
}

#[test]
fn supervisor_owner_input_replays_once_without_another_task_or_attempt() -> anyhow::Result<()> {
    let (root, turn) = fixture()?;
    let before = channels::load_queue_task(root.path(), task(&turn))?.unwrap();
    let first = input(
        root.path(),
        &turn,
        "input-1",
        "owner",
        "Use the PR's actual source revision.",
    )?;
    assert_eq!(first["status"], "completed");
    assert_eq!(
        first["result"]["contract"],
        "ctox.workjet.supervisor_input.v1"
    );
    assert_eq!(first["result"]["turn"]["command_id"], turn["command_id"]);
    assert_eq!(first["result"]["turn"]["task_id"], turn["task_id"]);
    assert_eq!(first["result"]["delivery"], "next_slice");
    assert_eq!(first["result"]["worker_interrupted"], false);
    assert_eq!(first["result"]["input"]["sequence"], 1);
    let replay = input(
        root.path(),
        &turn,
        "input-1",
        "owner",
        "Use the PR's actual source revision.",
    )?;
    assert_eq!(replay["result"], first["result"]);
    assert!(failed(input(
        root.path(),
        &turn,
        "input-1",
        "owner",
        "A changed intent"
    )));
    let after = channels::load_queue_task(root.path(), task(&turn))?.unwrap();
    assert_eq!(after.attempt, before.attempt);
    assert_eq!(after.route_status, before.route_status);
    assert_eq!(after.prompt, before.prompt);
    let conn = Connection::open(crate::paths::core_db(root.path()))?;
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM supervisor_owner_inputs", [], |row| {
            row.get::<_, i64>(0)
        })?,
        1
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM business_command_task_links",
            [],
            |row| row.get::<_, i64>(0)
        )?,
        1
    );
    Ok(())
}

#[test]
fn supervisor_owner_input_preserves_live_lease_and_late_input_requires_same_task_continuation(
) -> anyhow::Result<()> {
    let (root, turn) = fixture()?;
    begin(root.path(), &turn, "native-worker-1", "attempt-1")?;
    let before = channels::load_queue_task(root.path(), task(&turn))?.unwrap();
    input(
        root.path(),
        &turn,
        "input-before-context",
        "owner",
        "Inspect the exact PR diff.",
    )?;
    let captured =
        supervisor_owner_input::capture(root.path(), task(&turn), "attempt-1", "native-worker-1")?;
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].body, "Inspect the exact PR diff.");
    input(
        root.path(),
        &turn,
        "input-after-context",
        "owner",
        "The source is now available; preserve the review.",
    )?;
    let recovered_snapshot = supervisor_owner_input::capture(
        root.path(), task(&turn), "attempt-1", "native-worker-1",
    )?;
    assert_eq!(recovered_snapshot.len(), 1);
    let after = channels::load_queue_task(root.path(), task(&turn))?.unwrap();
    assert_eq!(after.lease_worker_id, before.lease_worker_id);
    assert_eq!(after.lease_expires_at, before.lease_expires_at);
    assert_eq!(after.attempt, before.attempt);
    passed(root.path(), &turn)?;
    assert!(channels::transition_business_command_for_task(
        root.path(),
        task(&turn),
        "handled",
        None,
        None,
        None,
        "old-context closure must fail"
    )
    .is_err());
    assert!(
        supervisor_owner_input::continue_pending(root.path(), task(&turn), "foreign-attempt")
            .is_err()
    );
    assert!(supervisor_owner_input::continue_pending(
        root.path(),
        task(&turn),
        "attempt-1"
    )?);
    let continuing = channels::load_queue_task(root.path(), task(&turn))?.unwrap();
    assert_eq!(continuing.route_status, "pending");
    assert_eq!(continuing.message_key, task(&turn));
    assert_eq!(
        channels::business_command_projection(root.path(), turn["command_id"].as_str().unwrap())?
            ["execution_phase"],
        "retry_wait"
    );
    assert!(supervisor_owner_input::mark_finished_slice(
        root.path(), "attempt-1", &[task(&turn).to_owned()])?);
    let engine = crate::lcm::LcmEngine::open(
        &crate::paths::core_db(root.path()), crate::lcm::LcmConfig::default())?;
    engine.record_worker_attempt_artifact_check("attempt-1", true, "bounded slice fixture")?;
    engine.terminalize_worker_attempt("attempt-1",
        crate::lcm::WorkerAttemptTerminalStatus::Succeeded, false, true, None)?;
    begin(root.path(), &turn, "native-worker-2", "attempt-2")?;
    let new_lease = channels::load_queue_task(root.path(), task(&turn))?.unwrap();
    assert!(supervisor_owner_input::continue_pending(root.path(), task(&turn), "attempt-1")?);
    let after_recovery = channels::load_queue_task(root.path(), task(&turn))?.unwrap();
    assert_eq!(after_recovery.lease_worker_id, new_lease.lease_worker_id);
    assert_eq!(after_recovery.lease_expires_at, new_lease.lease_expires_at);
    let next =
        supervisor_owner_input::capture(root.path(), task(&turn), "attempt-2", "native-worker-2")?;
    assert_eq!(next.len(), 2);
    assert_eq!(next[1].sequence, 2);
    assert!(
        supervisor_owner_input::capture(root.path(), task(&turn), "stale", "native-worker-1")
            .is_err()
    );
    passed(root.path(), &turn)?;
    assert!(!supervisor_owner_input::continue_pending(
        root.path(),
        task(&turn),
        "attempt-2"
    )?);
    // A fresh context still needs the existing work/plan/artifact guards.
    // Use the actual Owner cancel control to test terminal input rejection.
    super::supervisor_turns::control(
        root.path(),
        "cancel",
        "owner",
        "cancel",
        json!({"project_id":"project","thread_id":THREAD,"target_command_id":turn["command_id"],
            "reason":"End the bounded test task"}),
    )?;
    assert!(failed(input(
        root.path(),
        &turn,
        "terminal-input",
        "owner",
        "Cannot reopen terminal work."
    )));
    let conn = Connection::open(crate::paths::core_db(root.path()))?;
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM business_command_task_links",
            [],
            |row| row.get::<_, i64>(0)
        )?,
        1
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM supervisor_owner_inputs", [], |row| {
            row.get::<_, i64>(0)
        })?,
        2
    );
    Ok(())
}

#[test]
fn supervisor_owner_input_wakes_missing_source_review_without_approving_it() -> anyhow::Result<()> {
    let (root, turn) = fixture()?;
    begin(root.path(), &turn, "native-review-worker", "review-attempt")?;
    channels::persist_business_command_worker_result(
        root.path(),
        task(&turn),
        "Sources still required",
    )?;
    channels::record_business_command_review(
        root.path(),
        task(&turn),
        "held",
        "pending",
        &json!({"retryable_hold":true,"reason":"missing sources"}),
    )?;
    channels::hold_leased_messages_for_attempt(
        root.path(),
        "review-attempt",
        &[task(&turn).to_owned()],
        &crate::review::HoldReason::MissingArtifact,
        "Missing PR sources",
    )?;
    let before = channels::load_queue_task(root.path(), task(&turn))?.unwrap();
    assert!(before.retry_not_before.is_some());
    input(
        root.path(),
        &turn,
        "source-input",
        "owner",
        "Here is the actual source revision.",
    )?;
    let after = channels::load_queue_task(root.path(), task(&turn))?.unwrap();
    assert!(after.retry_not_before.is_none());
    assert_eq!(after.hold_reason, before.hold_reason);
    assert_eq!(after.failure_attempt_count, before.failure_attempt_count);
    assert_eq!(after.attempt, before.attempt);
    assert!(channels::transition_business_command_for_task(
        root.path(),
        task(&turn),
        "handled",
        None,
        None,
        None,
        "input is not approval"
    )
    .is_err());
    let context =
        channels::inspect_business_command(root.path(), turn["command_id"].as_str().unwrap())?
            .unwrap();
    assert_ne!(context["command"]["status"], "completed");
    Ok(())
}

#[test]
fn supervisor_owner_input_rejects_foreign_routes_forged_approval_and_oversize_bodies(
) -> anyhow::Result<()> {
    let (root, turn) = fixture()?;
    assert!(failed(input(
        root.path(),
        &turn,
        "foreign-input",
        "foreign",
        "Foreign intent"
    )));
    for extra in [
        json!({"thread_id":"b8958e94-4c45-4b47-b44b-03b9d8c47123"}),
        json!({"project_id":"other-project"}),
        json!({"approved":true}),
        json!({"turn_kind":"conversation"}),
        json!({"target_command_id":"missing"}),
        json!({"body":"x".repeat(4097)}),
        json!({"body":" "}),
    ] {
        let mut payload = json!({"project_id":"project","thread_id":THREAD,
            "target_command_id":turn["command_id"],"body":"Real new facts"});
        for (key, value) in extra.as_object().unwrap() {
            payload[key] = value.clone();
        }
        assert!(failed(super::supervisor_turns::control(
            root.path(),
            &uuid::Uuid::new_v4().to_string(),
            "owner",
            "input",
            payload
        )));
    }
    let conn = Connection::open(crate::paths::core_db(root.path()))?;
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM supervisor_owner_inputs", [], |row| {
            row.get::<_, i64>(0)
        })?,
        0
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM business_command_task_links",
            [],
            |row| row.get::<_, i64>(0)
        )?,
        1
    );
    Ok(())
}

#[test]
fn supervisor_owner_input_does_not_clear_provider_capacity_hold() -> anyhow::Result<()> {
    let (root, turn) = fixture()?;
    begin(root.path(), &turn, "capacity-worker", "capacity-attempt")?;
    channels::persist_business_command_worker_result(root.path(), task(&turn), "Provider capacity is unavailable")?;
    channels::record_business_command_review(root.path(), task(&turn), "held", "pending",
        &json!({"retryable_hold":true,"reason":"provider capacity"}))?;
    channels::hold_leased_messages_for_attempt(root.path(), "capacity-attempt",
        &[task(&turn).to_owned()], &crate::review::HoldReason::Technical {
            policy_id: channels::PROVIDER_CAPACITY_HOLD_POLICY.to_owned(),
        }, "Provider capacity is unavailable")?;
    let before = channels::load_queue_task(root.path(), task(&turn))?.unwrap();
    input(root.path(), &turn, "capacity-input", "owner", "Preserve my new facts until capacity returns.")?;
    let after = channels::load_queue_task(root.path(), task(&turn))?.unwrap();
    assert_eq!(after.retry_not_before, before.retry_not_before);
    assert_eq!(after.hold_reason, before.hold_reason);
    assert_eq!(after.failure_attempt_count, before.failure_attempt_count);
    assert_eq!(after.attempt, before.attempt);
    Ok(())
}

#[test]
fn concurrent_supervisor_owner_inputs_keep_both_intents_in_admission_order() -> anyhow::Result<()> {
    let (root, turn) = fixture()?;
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut workers = Vec::new();
    for index in 0..2 {
        let path = root.path().to_owned();
        let task_id = task(&turn).to_owned();
        let command = turn["command_id"].as_str().unwrap().to_owned();
        let barrier = barrier.clone();
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            supervisor_owner_input::admit(&path, &task_id, &command,
                &format!("concurrent-input-{index}"), "owner", &format!("Source fact {index}"))
        }));
    }
    let mut sequences = Vec::new();
    for worker in workers { sequences.push(worker.join().unwrap()?.sequence); }
    sequences.sort();
    assert_eq!(sequences, vec![1,2]);
    begin(root.path(), &turn, "concurrent-reader", "concurrent-attempt")?;
    let captured = supervisor_owner_input::capture(root.path(), task(&turn),
        "concurrent-attempt", "concurrent-reader")?;
    assert_eq!(captured.len(), 2);
    assert_ne!(captured[0].body, captured[1].body);
    assert_eq!(captured[0].sequence, 1);
    assert_eq!(captured[1].sequence, 2);
    Ok(())
}

#[test]
fn supervisor_input_capability_is_scoped_opt_in_without_creating_work() -> anyhow::Result<()> {
    let root = super::supervisor_turns::fixture()?;
    let legacy = super::supervisor_turns::control(root.path(), "legacy-cap", "owner", "capabilities",
        json!({"project_id":"project","thread_id":THREAD}))?;
    assert!(legacy["result"].get("input_contract").is_none());
    let enabled = super::supervisor_turns::control(root.path(), "input-cap", "owner", "capabilities",
        json!({"project_id":"project","thread_id":THREAD,"include_input":true}))?;
    assert_eq!(enabled["status"], "completed");
    assert_eq!(enabled["result"]["input_contract"], "ctox.workjet.supervisor_input.v1");
    assert_eq!(enabled["result"]["input_delivery"], "next_slice");
    assert_eq!(enabled["result"]["max_input_chars"], 4096);
    assert!(failed(super::supervisor_turns::control(root.path(), "foreign-cap", "foreign", "capabilities",
        json!({"project_id":"project","thread_id":THREAD,"include_input":true}))));
    let conn = Connection::open(crate::paths::core_db(root.path()))?;
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM business_command_task_links", [], |row| row.get::<_, i64>(0))?, 0);
    Ok(())
}
