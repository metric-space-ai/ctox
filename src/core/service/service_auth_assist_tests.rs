use super::*;
use serde_json::json;

#[test]
fn auth_assist_recovery_keeps_ordinary_command_lease_guard() {
    let root = tempfile::tempdir().unwrap();
    let task_id = auth_request_fixture(root.path(), "ordinary-command", true);
    let error = channels::transition_business_command_for_task(
        root.path(),
        &task_id,
        "leased",
        None,
        None,
        None,
        "no owned lease",
    )
    .expect_err("ordinary commands still need their real worker lease");
    assert_eq!(
        error.to_string(),
        "business command `ordinary-command` requires an owned, expiring queue lease before `leased`"
    );
    let task = channels::load_queue_task(root.path(), &task_id)
        .unwrap()
        .unwrap();
    assert_eq!(task.route_status, "pending");
    assert!(task.lease_owner.is_none());
    assert_eq!(
        channels::business_command_projection(root.path(), "ordinary-command").unwrap()
            ["execution_phase"],
        "queued",
    );
}

fn auth_request_fixture(root: &Path, command_id: &str, legacy: bool) -> String {
    channels::claim_business_command_with_queue(
        root,
        channels::BusinessCommandClaimRequest {
            command_id: command_id.into(),
            idempotency_key: command_id.into(),
            payload_hash: format!("sha256:{command_id}"),
            module: "ctox".into(),
            command_type: if legacy {
                "tests.legacy_auth"
            } else {
                "web_stack.auth_assist.request"
            }
            .into(),
            record_id: "handelsregister.de".into(),
            intent: json!({"payload": {
                "purpose": "web_stack_auth",
                "owner_user_id": "owner-a",
                "session_id": "browser_session_web_stack_auth_handelsregister_de_owner_a",
                "requesting_task_id": "research-task",
                "target_url": "https://www.handelsregister.de/",
                "expires_at_ms": 1
            }}),
            created_at_ms: 1_700_000_000_000,
        },
        channels::QueueTaskCreateRequest {
            title: "web stack auth assist request · handelsregister.de".into(),
            prompt: "Owner confirms login in the browser.".into(),
            thread_key: format!("business-os/ctox/{command_id}"),
            workspace_root: None,
            priority: "normal".into(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: Some(json!({"business_os_command_id":command_id})),
        },
    )
    .expect("admit auth request")
    .task
    .message_key
}

#[test]
fn auth_assist_recovery_admission_is_durable_human_wait_without_worker_attempt() {
    let root = tempfile::tempdir().unwrap();
    let task_id = auth_request_fixture(root.path(), "auth-admission", false);
    let task = channels::load_queue_task(root.path(), &task_id)
        .unwrap()
        .unwrap();
    assert_eq!(task.route_status, "blocked");
    assert!(task.lease_owner.is_none());
    assert!(task.leased_at.is_none());
    assert!(channels::lease_queue_task(root.path(), &task_id, CHANNEL_ROUTER_LEASE_OWNER).is_err());
    let projection = channels::business_command_projection(root.path(), "auth-admission").unwrap();
    assert_eq!(projection["execution_phase"], "blocked");
    assert_eq!(projection["terminal_status"], "none");
    assert_eq!(projection["attempt"], 0);
    assert_eq!(
        channels::recover_auth_assist_requests(root.path()).unwrap(),
        0
    );
}

#[test]
fn auth_assist_recovery_boot_preserves_legacy_request_and_incomplete_plan() {
    for had_worker_lease in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let command_id = "auth-legacy";
        let task_id = auth_request_fixture(root.path(), command_id, true);
        if had_worker_lease {
            channels::lease_queue_task(root.path(), &task_id, CHANNEL_ROUTER_LEASE_OWNER).unwrap();
        }
        let steps = ["Open login", "Owner completes MFA", "Confirm login"].map(|label| {
            lcm::TaskExecutionPlanStepInput {
                label: label.into(),
                status: "pending".into(),
            }
        });
        lcm::run_record_task_execution_plan(
            &crate::paths::core_db(root.path()),
            lcm::TaskExecutionPlanUpdate {
                work_key: &task_id,
                task_id: &task_id,
                command_id,
                attempt_id: "legacy-attempt",
                explanation: None,
                steps: &steps,
            },
        )
        .unwrap();
        let original_plan = lcm::run_task_execution_progress_for_task(
            &crate::paths::core_db(root.path()),
            &task_id,
        )
        .unwrap()
        .expect("durable incomplete plan");
        assert_eq!(original_plan["completed_steps"], 0);
        assert_eq!(original_plan["total_steps"], 3);
        // Reproduce the persisted pre-fix aggregate. No live worker exists in
        // the new process; the missing-lease variant models outcome recovery.
        let mut conn = channels::open_channel_db(&crate::paths::core_db(root.path())).unwrap();
        // Install a pre-upgrade snapshot under one writer reservation. No
        // synthetic worker should race projection startup while preparing
        // the database of a process that has already exited.
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        tx.execute(
            "UPDATE business_command_aggregates
             SET command_type='web_stack.auth_assist.request',
                 execution_phase=CASE WHEN ?2 THEN 'running' ELSE 'queued' END,
                 attempt=CASE WHEN ?2 THEN 1 ELSE 0 END
             WHERE command_id=?1",
            params![command_id, had_worker_lease],
        )
        .unwrap();
        tx.commit().unwrap();
        let original_intent: String = conn
            .query_row(
                "SELECT intent_json FROM business_command_aggregates WHERE command_id=?1",
                [command_id],
                |row| row.get(0),
            )
            .unwrap();
        let original_attempt: i64 = conn
            .query_row(
                "SELECT attempt FROM communication_routing_state WHERE message_key=?1",
                [&task_id],
                |row| row.get(0),
            )
            .unwrap();
        drop(conn);

        let state = Arc::new(Mutex::new(SharedState::default()));
        release_stale_service_communication_leases_on_boot(root.path(), &state);
        let conn = channels::open_channel_db(&crate::paths::core_db(root.path())).unwrap();
        let before: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM business_command_transitions WHERE command_id=?1",
                [command_id],
                |row| row.get(0),
            )
            .unwrap();
        drop(conn);
        release_stale_service_communication_leases_on_boot(root.path(), &state);

        let task = channels::load_queue_task(root.path(), &task_id)
            .unwrap()
            .unwrap();
        assert_eq!(task.route_status, "blocked");
        assert!(task.lease_owner.is_none());
        assert!(task.leased_at.is_none());
        assert!(
            channels::lease_queue_task(root.path(), &task_id, CHANNEL_ROUTER_LEASE_OWNER).is_err()
        );
        let projection = channels::business_command_projection(root.path(), command_id).unwrap();
        assert_eq!(projection["execution_phase"], "blocked");
        assert_eq!(projection["terminal_status"], "none");
        let conn = channels::open_channel_db(&crate::paths::core_db(root.path())).unwrap();
        let row = conn
            .query_row(
                "SELECT hold_reason, wait_entity_type, wait_entity_id, attempt,
                    failure_attempt_count, retry_not_before, last_error
             FROM communication_routing_state WHERE message_key=?1",
                [&task_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            row,
            (
                "waiting_external".into(),
                "web_stack_auth_assist".into(),
                command_id.into(),
                original_attempt,
                0,
                None,
                None
            )
        );
        let (intent, transitions): (String, i64) = conn
            .query_row(
                "SELECT intent_json, (SELECT COUNT(*) FROM business_command_transitions
             WHERE command_id=?1) FROM business_command_aggregates WHERE command_id=?1",
                [command_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            intent, original_intent,
            "owner/session/expiry and requesting task survive"
        );
        assert_eq!(
            transitions, before,
            "second boot must not create another transition"
        );
        drop(conn);
        assert_eq!(
            lcm::run_task_execution_progress_for_task(
                &crate::paths::core_db(root.path()),
                &task_id,
            )
            .unwrap()
            .unwrap(),
            original_plan,
            "recovery must neither complete nor fail the unfinished model plan",
        );
        // Only explicit browser confirmation/cancellation closes the helper;
        // restarting after closure must not reopen a completed human handoff.
        channels::set_queue_task_route_status(root.path(), &task_id, "cancelled").unwrap();
        release_stale_service_communication_leases_on_boot(root.path(), &state);
        assert_eq!(
            channels::load_queue_task(root.path(), &task_id)
                .unwrap()
                .unwrap()
                .route_status,
            "cancelled"
        );
    }
}
