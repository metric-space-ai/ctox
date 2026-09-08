// In-tree behavioral tests for the mission channels store (extracted
// from mod.rs; `use super::*` keeps access to crate-private internals).
// ctox-allow-direct-state-write: test fixture module
use super::*;
use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;

fn unique_test_db_path(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{prefix}-{}.db",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ))
}

#[test]
fn cockpit_release_guard_checks_all_links_in_either_order() -> Result<()> {
    // Production currently enforces task_id UNIQUE. Model a legacy/multi-link
    // store here to test the shared guard without weakening that invariant.
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("CREATE TABLE business_command_aggregates(command_id TEXT PRIMARY KEY, execution_phase TEXT NOT NULL, terminal_status TEXT NOT NULL);
        CREATE TABLE business_command_task_links(command_id TEXT PRIMARY KEY, task_id TEXT NOT NULL);")?;
    assert!(ensure_linked_commands_allow_queue_release(&conn, "task").is_ok());
    for phase in ["terminal", "validating"] {
        for restricted_first in [false, true] {
            conn.execute_batch(
                "DELETE FROM business_command_task_links; DELETE FROM business_command_aggregates;",
            )?;
            let phases = if restricted_first {
                [phase, "accepted"]
            } else {
                ["accepted", phase]
            };
            for (index, phase) in phases.into_iter().enumerate() {
                let id = format!("command-{index}");
                conn.execute(
                    "INSERT INTO business_command_aggregates VALUES(?1, ?2, ?3)",
                    params![
                        id,
                        phase,
                        if phase == "terminal" {
                            "failed"
                        } else {
                            "none"
                        }
                    ],
                )?;
                conn.execute(
                    "INSERT INTO business_command_task_links VALUES(?1, 'task')",
                    [&id],
                )?;
            }
            let error = ensure_linked_commands_allow_queue_release(&conn, "task")
                .expect_err("one restricted link must veto release");
            assert!(error.to_string().contains(phase), "{error:#}");
            conn.execute("UPDATE business_command_aggregates SET execution_phase='accepted', terminal_status='none'", [])?;
            assert!(ensure_linked_commands_allow_queue_release(&conn, "task").is_ok());
        }
    }
    Ok(())
}

#[test]
fn communication_sync_run_recorder_skips_successful_noop_heartbeats() -> Result<()> {
    let db_path = unique_test_db_path("ctox-comm-sync-noop");
    let mut conn = open_channel_db(&db_path)?;

    record_communication_sync_run(
        &mut conn,
        CommunicationSyncRun {
            run_key: "noop-1",
            channel: "email",
            account_key: "email:owner@example.test",
            folder_hint: "INBOX",
            started_at: "2026-06-27T00:00:00Z",
            finished_at: "2026-06-27T00:00:01Z",
            ok: true,
            fetched_count: 42,
            stored_count: 0,
            error_text: "",
            metadata_json: "{}",
        },
    )?;
    let count_after_noop: i64 =
        conn.query_row("SELECT COUNT(*) FROM communication_sync_runs", [], |row| {
            row.get(0)
        })?;
    assert_eq!(count_after_noop, 0);

    record_communication_sync_run(
        &mut conn,
        CommunicationSyncRun {
            run_key: "stored-1",
            channel: "email",
            account_key: "email:owner@example.test",
            folder_hint: "INBOX",
            started_at: "2026-06-27T00:00:02Z",
            finished_at: "2026-06-27T00:00:03Z",
            ok: true,
            fetched_count: 42,
            stored_count: 1,
            error_text: "",
            metadata_json: "{}",
        },
    )?;
    let count_after_store: i64 =
        conn.query_row("SELECT COUNT(*) FROM communication_sync_runs", [], |row| {
            row.get(0)
        })?;
    assert_eq!(count_after_store, 1);

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn communication_intake_source_stamp_uses_projection_clock() -> Result<()> {
    let root = tempfile::tempdir()?;
    let db_path = resolve_db_path(root.path(), None);
    let mut conn = open_channel_db(&db_path)?;
    upsert_communication_account(
        &mut conn,
        "email:agent@example.test",
        "email",
        "agent@example.test",
        "imap",
        json!({ "display": "Agent" }),
    )?;
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: "email:agent@example.test::INBOX::42",
            channel: "email",
            account_key: "email:agent@example.test",
            thread_key: "thread:projection-clock",
            remote_id: "42",
            direction: "inbound",
            folder_hint: "INBOX",
            sender_display: "Sender",
            sender_address: "sender@example.test",
            recipient_addresses_json: "[]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: "Projection clock",
            preview: "Projection clock preview",
            body_text: "Projection clock body",
            body_html: "",
            raw_payload_ref: "",
            trust_level: "normal",
            status: "received",
            seen: false,
            has_attachments: false,
            external_created_at: "2026-06-25T00:00:00Z",
            observed_at: "2026-06-25T00:00:00Z",
            metadata_json: "{}",
        },
    )?;

    let first = communication_intake_source_stamp(root.path())?;
    assert_eq!(first.account_count, 1);
    assert_eq!(first.message_count, 1);
    assert!(
        first.projection_version >= 2,
        "account/message inserts must advance projection clock, got {:?}",
        first
    );

    let explain_sql = format!("EXPLAIN QUERY PLAN {COMMUNICATION_INTAKE_SOURCE_STAMP_SQL}");
    let plan = conn
        .prepare(&explain_sql)?
        .query_map([], |row| row.get::<_, String>(3))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    assert!(
        plan.iter()
            .any(|detail| detail.contains("communication_projection_clock")),
        "communication intake source stamp must read the projection clock, got {plan:?}"
    );
    assert!(
        !plan
            .iter()
            .any(|detail| detail.contains("communication_messages")),
        "communication intake source stamp must not scan communication_messages, got {plan:?}"
    );

    conn.execute(
        r#"
        UPDATE communication_messages
        SET metadata_json = '{"changed":true}'
        WHERE message_key = 'email:agent@example.test::INBOX::42'
        "#,
        [],
    )?;
    let second = communication_intake_source_stamp(root.path())?;
    assert_eq!(second.message_count, 1);
    assert!(
        second.projection_version > first.projection_version,
        "message metadata update must advance projection clock"
    );
    assert_ne!(second, first);
    Ok(())
}

#[test]
fn communication_accounts_projection_respects_since_ms() -> Result<()> {
    let root = std::env::temp_dir().join(format!(
        "ctox-communication-accounts-projection-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root)?;
    let mut conn = open_channel_db(&resolve_db_path(&root, None))?;
    upsert_communication_account(
        &mut conn,
        "email:founder@example.test",
        "email",
        "founder@example.test",
        "imap",
        json!({ "display": "Founder" }),
    )?;
    drop(conn);

    let first = pull_communication_accounts_for_business_os(&root, Some(0), Some(10))?;
    assert_eq!(first.get("count").and_then(Value::as_i64), Some(1));
    assert_eq!(first.get("since_ms").and_then(Value::as_i64), Some(0));
    let updated_at_ms = first
        .pointer("/documents/0/updated_at_ms")
        .and_then(Value::as_i64)
        .context("expected projected account updated_at_ms")?;
    assert!(updated_at_ms > 0);

    let after_checkpoint = pull_communication_accounts_for_business_os(
        &root,
        Some(updated_at_ms.saturating_add(1)),
        Some(10),
    )?;
    assert_eq!(
        after_checkpoint.get("count").and_then(Value::as_i64),
        Some(0)
    );
    assert_eq!(
        after_checkpoint.get("since_ms").and_then(Value::as_i64),
        Some(updated_at_ms.saturating_add(1))
    );

    let _ = fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn communication_record_projection_uses_keyed_lookup() -> Result<()> {
    let root = std::env::temp_dir().join(format!(
        "ctox-communication-record-projection-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root)?;
    let mut conn = open_channel_db(&resolve_db_path(&root, None))?;
    upsert_communication_account(
        &mut conn,
        "email:founder@example.test",
        "email",
        "founder@example.test",
        "imap",
        json!({ "display": "Founder" }),
    )?;
    conn.execute(
        r#"
        INSERT INTO communication_threads (
            thread_key, channel, account_key, subject, participant_keys_json,
            last_message_key, last_message_at, message_count, unread_count,
            metadata_json, updated_at
        ) VALUES (
            'thread-keyed', 'email', 'email:founder@example.test', 'Keyed thread',
            '[]', 'message-keyed', '2026-06-26T08:00:00Z', 1, 0,
            '{}', '2026-06-26T08:00:00Z'
        )
        "#,
        [],
    )?;
    conn.execute(
        r#"
        INSERT INTO communication_messages (
            message_key, channel, account_key, thread_key, remote_id, direction,
            folder_hint, sender_display, sender_address, recipient_addresses_json,
            cc_addresses_json, bcc_addresses_json, subject, preview, body_text,
            body_html, raw_payload_ref, trust_level, status, seen,
            has_attachments, external_created_at, observed_at, metadata_json
        ) VALUES (
            'message-keyed', 'email', 'email:founder@example.test', 'thread-keyed',
            'remote-keyed', 'inbound', 'INBOX', 'Founder',
            'founder@example.test', '[]', '[]', '[]', 'Subject', 'Preview',
            'Body', '', '', 'normal', 'received', 0, 0,
            '2026-06-26T08:00:00Z', '2026-06-26T08:00:00Z',
            '{"ticket_self_work_id":"work-1"}'
        )
        "#,
        [],
    )?;
    conn.execute(
        r#"
        INSERT INTO communication_routing_state (
            message_key, route_status, lease_owner, leased_at, acked_at,
            last_error, updated_at
        ) VALUES (
            'message-keyed', 'queued', '', NULL, NULL, NULL,
            '2026-06-26T08:00:00Z'
        )
        "#,
        [],
    )?;
    drop(conn);

    let account = pull_communication_record_for_business_os(
        &root,
        "communication_accounts",
        "email:founder@example.test",
    )?
    .context("expected keyed communication account")?;
    assert_eq!(
        account.get("id").and_then(Value::as_str),
        Some("email:founder@example.test")
    );

    let thread =
        pull_communication_record_for_business_os(&root, "communication_threads", "thread-keyed")?
            .context("expected keyed communication thread")?;
    assert_eq!(
        thread.get("subject").and_then(Value::as_str),
        Some("Keyed thread")
    );

    let message = pull_communication_record_for_business_os(
        &root,
        "communication_messages",
        "message-keyed",
    )?
    .context("expected keyed communication message")?;
    assert_eq!(
        message.get("route_status").and_then(Value::as_str),
        Some("queued")
    );
    assert_eq!(
        message.get("ticket_self_work_id").and_then(Value::as_str),
        Some("work-1")
    );
    assert!(
        pull_communication_record_for_business_os(&root, "communication_messages", "missing")?
            .is_none()
    );

    let _ = fs::remove_dir_all(&root);
    Ok(())
}

/// EGRESS-3: the reviewed-outbound evidence the kernel gate compares must be
/// genuinely independent — APPROVED hashes from the durable review record,
/// OUTGOING hashes from the live request. This proves the three properties
/// that make the gate load-bearing without false-rejecting legitimate sends:
/// a normalization-equivalent send matches (no false reject), a mutated
/// body/recipient diverges (caught), and a send whose approval is not
/// record-backed falls back to the request (never NEWLY rejected).
#[test]
fn reviewed_outbound_evidence_compares_record_against_request() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE communication_founder_reply_reviews (\
             approval_key TEXT PRIMARY KEY, \
             action_json TEXT NOT NULL, \
             body_sha256 TEXT NOT NULL);",
    )
    .unwrap();

    let action = FounderOutboundAction {
        account_key: "email:cto1@example.com".to_string(),
        thread_key: "<egress-3-thread@example.com>".to_string(),
        subject: "Re: budget".to_string(),
        to: vec!["founder@example.com".to_string()],
        cc: vec!["board@example.com".to_string()],
        attachments: vec!["/tmp/q3.xlsx".to_string()],
    };
    let approved_body = "The approved reply body.";
    let (_digest, action_json, body_sha256) =
        founder_outbound_review_digest(&action, approved_body);
    conn.execute(
        "INSERT INTO communication_founder_reply_reviews \
             (approval_key, action_json, body_sha256) VALUES (?1, ?2, ?3)",
        params!["approval-egress-3", action_json, body_sha256],
    )
    .unwrap();

    let mk_request = |to: Vec<String>, body: &str| ChannelSendRequest {
        channel: "email".to_string(),
        account_key: action.account_key.clone(),
        thread_key: action.thread_key.clone(),
        body: body.to_string(),
        subject: action.subject.clone(),
        to,
        cc: action.cc.clone(),
        attachments: action.attachments.clone(),
        sender_display: None,
        sender_address: None,
        send_voice: false,
        reviewed_founder_send: true,
    };

    // (a) A legitimate send whose recipients differ only by case/whitespace
    // from the approval must NOT false-reject: both sides normalize, so the
    // approved and outgoing hashes are equal -> require_reviewed_outbound
    // passes. This is the P0 guard the old tautology silently satisfied.
    let legit = mk_request(vec!["  Founder@Example.com ".to_string()], approved_body);
    let ev = reviewed_outbound_evidence(&conn, "approval-egress-3", &legit);
    assert_eq!(
        ev.approved_body_sha256, ev.outgoing_body_sha256,
        "legitimate body must not false-reject"
    );
    assert_eq!(
        ev.approved_recipient_set_sha256, ev.outgoing_recipient_set_sha256,
        "normalization-equivalent recipients must not false-reject"
    );

    // (b) A mutated body must diverge -> the kernel gate now catches it.
    let tampered_body = mk_request(action.to.clone(), "Wire the funds to a new account.");
    let ev_body = reviewed_outbound_evidence(&conn, "approval-egress-3", &tampered_body);
    assert_ne!(
        ev_body.approved_body_sha256, ev_body.outgoing_body_sha256,
        "a body mutated after approval must be caught"
    );
    // ...and the recipients still match, isolating the body as the mismatch.
    assert_eq!(
        ev_body.approved_recipient_set_sha256, ev_body.outgoing_recipient_set_sha256,
        "unchanged recipients must stay equal when only the body is mutated"
    );

    // (c) A mutated recipient must diverge -> exfiltration to a new address
    // is caught even when the body is untouched.
    let tampered_to = mk_request(vec!["attacker@evil.example".to_string()], approved_body);
    let ev_to = reviewed_outbound_evidence(&conn, "approval-egress-3", &tampered_to);
    assert_ne!(
        ev_to.approved_recipient_set_sha256, ev_to.outgoing_recipient_set_sha256,
        "a recipient added/swapped after approval must be caught"
    );

    // (d) No record-backed approval -> fall back to the request so a
    // previously-valid send is never NEWLY rejected by this change.
    let unbacked = reviewed_outbound_evidence(&conn, "no-such-approval", &legit);
    assert_eq!(
        unbacked.approved_body_sha256, unbacked.outgoing_body_sha256,
        "missing review row falls back to the request body"
    );
    assert_eq!(
        unbacked.approved_recipient_set_sha256, unbacked.outgoing_recipient_set_sha256,
        "missing review row falls back to the request recipients"
    );
    assert_eq!(
        unbacked.review_audit_key.as_deref(),
        Some("no-such-approval"),
        "the audit key is still recorded even on fallback"
    );
}

#[test]
fn duplicate_column_alter_error_is_recognized() {
    // X2-03: the probe-then-ALTER migration tolerates a concurrent writer
    // having added the column; is_duplicate_column_error is the predicate
    // that makes that tolerance safe without swallowing unrelated errors.
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE t (a INTEGER, terminal_no_send INTEGER);")
        .unwrap();
    // Adding an already-present column yields SQLite's duplicate-column error.
    let dup = conn
        .execute("ALTER TABLE t ADD COLUMN terminal_no_send INTEGER", [])
        .unwrap_err();
    assert!(
        is_duplicate_column_error(&dup),
        "expected duplicate-column error, got: {dup}"
    );
    // An unrelated error (missing table) must NOT be mistaken for it.
    let other = conn
        .execute("ALTER TABLE no_such_table ADD COLUMN x INTEGER", [])
        .unwrap_err();
    assert!(!is_duplicate_column_error(&other));
}

#[test]
fn queue_route_status_parse_roundtrips_all_canonical_values() {
    for status in QueueRouteStatus::ALL {
        assert_eq!(QueueRouteStatus::parse(status.as_str()), Some(status));
        assert_eq!(
            QueueRouteStatus::parse(&status.as_str().to_ascii_uppercase()),
            Some(status)
        );
    }
}

#[test]
fn queue_route_status_parse_maps_every_legacy_alias() {
    let aliases = [
        ("approval-nag-handled", QueueRouteStatus::Blocked),
        ("completed", QueueRouteStatus::Handled),
        ("superseded", QueueRouteStatus::Cancelled),
        ("duplicate", QueueRouteStatus::Blocked),
        ("blocked_sender", QueueRouteStatus::Blocked),
        ("meeting_scheduled", QueueRouteStatus::Blocked),
    ];
    for (alias, expected) in aliases {
        assert_eq!(
            QueueRouteStatus::parse(alias),
            Some(expected),
            "legacy alias {alias:?} must map to {}",
            expected.as_str()
        );
    }
    assert_eq!(
        QueueRouteStatus::parse(""),
        Some(QueueRouteStatus::Pending),
        "historical blank rows remain readable as pending"
    );
}

#[test]
fn queue_route_status_parse_rejects_unknown_values() {
    assert_eq!(QueueRouteStatus::parse("nonsense-status"), None);
    assert!(canonical_queue_route_status("nonsense-status").is_err());
}

#[test]
fn current_queue_route_status_errors_on_unknown_persisted_value() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE communication_routing_state (message_key TEXT PRIMARY KEY, route_status TEXT NOT NULL);",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO communication_routing_state (message_key, route_status) VALUES (?1, ?2)",
        params!["queue:unknown", "nonsense-status"],
    )
    .unwrap();

    let error = current_queue_route_status(&conn, "queue:unknown").unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unknown queue route status for message `queue:unknown`"),
        "unexpected error: {error:#}"
    );
}

#[test]
fn queue_route_status_core_state_maps_all_canonical_variants() {
    let cases = [
        (QueueRouteStatus::Pending, CoreState::Pending),
        (QueueRouteStatus::Leased, CoreState::Leased),
        (QueueRouteStatus::Running, CoreState::Running),
        (QueueRouteStatus::Blocked, CoreState::Blocked),
        (QueueRouteStatus::ReviewRework, CoreState::ReworkRequired),
        (QueueRouteStatus::Failed, CoreState::Failed),
        (QueueRouteStatus::Handled, CoreState::Completed),
        (QueueRouteStatus::Cancelled, CoreState::Superseded),
    ];
    for (status, expected) in cases {
        assert_eq!(
            queue_route_status_core_state(status),
            expected,
            "route status {:?} must map to the pinned core state",
            status
        );
    }
}

#[test]
fn queue_sort_at_folds_priority_so_higher_priority_leases_first() {
    // router-5: durable-queue lease order is `ORDER BY queue_sort_at ASC`,
    // and queue_sort_at shifts the timestamp by priority. Pin that a higher
    // priority yields an earlier sort key (urgent < high < normal < low) so
    // priority can never silently invert, and that an unknown priority bails.
    let now = "2026-06-14T12:00:00+00:00";
    let urgent = queue_sort_at("urgent", now).unwrap();
    let high = queue_sort_at("high", now).unwrap();
    let normal = queue_sort_at("normal", now).unwrap();
    let low = queue_sort_at("low", now).unwrap();
    assert!(
        urgent < high,
        "urgent {urgent} must sort before high {high}"
    );
    assert!(
        high < normal,
        "high {high} must sort before normal {normal}"
    );
    assert!(normal < low, "normal {normal} must sort before low {low}");
    assert!(
        queue_sort_at("bogus", now).is_err(),
        "an unsupported priority must bail"
    );
}

#[test]
fn queue_tasks_round_trip_through_channel_store() {
    let root = std::env::temp_dir().join(format!(
        "ctox-queue-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("failed to create temp test root");

    let created = create_queue_task(
        &root,
        QueueTaskCreateRequest {
            title: "queue smoke".to_string(),
            prompt: "Inspect the queue task round-trip.".to_string(),
            thread_key: "queue/test".to_string(),
            workspace_root: None,
            priority: "high".to_string(),
            suggested_skill: Some("queue-orchestrator".to_string()),
            parent_message_key: None,
            extra_metadata: None,
        },
    )
    .expect("failed to create queue task");
    assert_eq!(created.route_status, "pending");
    assert_eq!(created.priority, "high");
    assert_eq!(
        created.suggested_skill.as_deref(),
        Some("queue-orchestrator")
    );
    let conn = open_channel_db(&crate::paths::core_db(&root)).expect("failed to open channel db");
    let spawn_edge_count: i64 = conn
        .query_row(
            r#"
            SELECT COUNT(*)
            FROM ctox_core_spawn_edges
            WHERE child_entity_type = 'QueueTask'
              AND child_entity_id = ?1
              AND spawn_kind = 'queue-task'
              AND parent_entity_type = 'Thread'
              AND accepted = 1
            "#,
            params![&created.message_key],
            |row| row.get(0),
        )
        .expect("failed to count queue spawn edge");
    assert_eq!(spawn_edge_count, 1);

    let updated = update_queue_task(
        &root,
        QueueTaskUpdateRequest {
            message_key: created.message_key.clone(),
            priority: Some("urgent".to_string()),
            route_status: Some("blocked".to_string()),
            status_note: Some("waiting for owner".to_string()),
            ..Default::default()
        },
    )
    .expect("failed to update queue task");
    assert_eq!(updated.route_status, "blocked");
    assert_eq!(updated.priority, "urgent");
    assert_eq!(updated.status_note.as_deref(), Some("waiting for owner"));

    let loaded = load_queue_task(&root, &created.message_key)
        .expect("failed to load queue task")
        .expect("queue task missing after update");
    assert_eq!(loaded.message_key, created.message_key);
    assert_eq!(loaded.route_status, "blocked");

    let listed = list_queue_tasks(&root, &["blocked".to_string()], 10)
        .expect("failed to list blocked queue tasks");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].message_key, created.message_key);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn queue_peek_projects_the_complete_routing_row() {
    let root = std::env::temp_dir().join(format!(
        "ctox-queue-peek-routing-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("failed to create temp test root");

    let created = create_queue_task(
        &root,
        QueueTaskCreateRequest {
            title: "peek queue task".to_string(),
            prompt: "Verify the read-only queue peek projection.".to_string(),
            thread_key: "queue/peek-routing".to_string(),
            workspace_root: None,
            priority: "high".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )
    .expect("failed to create queue task");

    let peeked = peek_leasable_inbound_messages(&root, 10, "peek-worker")
        .expect("queue peek must map every routing column");
    assert_eq!(peeked.len(), 1);
    assert_eq!(peeked[0].message_key, created.message_key);

    let loaded = load_queue_task(&root, &created.message_key)
        .expect("failed to load queue task after peek")
        .expect("queue task missing after peek");
    assert_eq!(loaded.route_status, "pending");
    assert!(loaded.lease_owner.is_none());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn releasing_queue_task_clears_retry_and_defer_state() {
    let root = std::env::temp_dir().join(format!(
        "ctox-queue-release-defer-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("failed to create temp test root");

    let created = create_queue_task(
        &root,
        QueueTaskCreateRequest {
            title: "release deferred queue task".to_string(),
            prompt: "Resume this task immediately when explicitly released.".to_string(),
            thread_key: "queue/release-deferred".to_string(),
            workspace_root: None,
            priority: "urgent".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: Some(json!({
                "not_before": "2099-01-01T00:00:00Z",
                "defer_reason": "review cooldown"
            })),
        },
    )
    .expect("failed to create deferred queue task");
    let db_path = resolve_db_path(&root, None);
    let conn = open_channel_db(&db_path).expect("failed to open channel db");
    conn.execute(
        "UPDATE communication_routing_state
         SET route_status='blocked', retry_not_before='2099-01-01T00:00:00Z',
             hold_reason='missing_review_evidence', wait_entity_type='review',
             wait_entity_id='review-1', lease_expires_at='2099-01-01T00:15:00Z'
         WHERE message_key=?1",
        params![&created.message_key],
    )
    .expect("failed to seed deferred routing state");
    drop(conn);

    let released = update_queue_task(
        &root,
        QueueTaskUpdateRequest {
            message_key: created.message_key.clone(),
            route_status: Some("pending".to_string()),
            status_note: Some("operator release".to_string()),
            ..Default::default()
        },
    )
    .expect("failed to release deferred queue task");
    assert_eq!(released.route_status, "pending");
    assert_eq!(
        queue_task_deferred_until(&root, &created.message_key)
            .expect("failed to inspect deferred state"),
        None
    );

    let conn = open_channel_db(&db_path).expect("failed to reopen channel db");
    let cleared: (i64, i64, i64, i64, i64, i64, i64) = conn
        .query_row(
            "SELECT retry_not_before IS NULL, hold_reason IS NULL,
                    wait_entity_type IS NULL, wait_entity_id IS NULL,
                    lease_expires_at IS NULL,
                    json_extract(m.metadata_json, '$.not_before') IS NULL,
                    json_extract(m.metadata_json, '$.defer_reason') IS NULL
             FROM communication_routing_state r
             JOIN communication_messages m ON m.message_key=r.message_key
             WHERE r.message_key=?1",
            params![&created.message_key],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .expect("failed to verify cleared defer state");
    assert_eq!(cleared, (1, 1, 1, 1, 1, 1, 1));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn queue_task_list_cache_reuses_idle_reads_until_store_changes() {
    let root = std::env::temp_dir().join(format!(
        "ctox-queue-list-cache-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("failed to create temp test root");
    let db_path = resolve_db_path(&root, None);
    let pending = vec!["pending".to_string()];

    let first = list_queue_tasks(&root, &pending, 10).expect("failed to list empty queue");
    assert!(first.is_empty());
    assert_eq!(
        queue_task_list_cache_miss_count_for_tests(&db_path, &pending, 10),
        1,
        "first queue list must hit SQLite"
    );

    let second = list_queue_tasks(&root, &pending, 10).expect("failed to relist empty queue");
    assert!(second.is_empty());
    assert_eq!(
        queue_task_list_cache_miss_count_for_tests(&db_path, &pending, 10),
        1,
        "unchanged idle queue list must reuse the cached snapshot"
    );

    let created = create_queue_task(
        &root,
        QueueTaskCreateRequest {
            title: "cache invalidation".to_string(),
            prompt: "Make the queue list cache refresh after a store write.".to_string(),
            thread_key: "queue/list-cache".to_string(),
            workspace_root: None,
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )
    .expect("failed to create queue task");

    let after_write =
        list_queue_tasks(&root, &pending, 10).expect("failed to list queue after write");
    assert_eq!(after_write.len(), 1);
    assert_eq!(after_write[0].message_key, created.message_key);
    assert_eq!(
        queue_task_list_cache_miss_count_for_tests(&db_path, &pending, 10),
        2,
        "store writes must invalidate the cached queue snapshot"
    );

    let after_idle =
        list_queue_tasks(&root, &pending, 10).expect("failed to list queue after idle");
    assert_eq!(after_idle.len(), 1);
    assert_eq!(after_idle[0].message_key, created.message_key);
    assert_eq!(
        queue_task_list_cache_miss_count_for_tests(&db_path, &pending, 10),
        2,
        "unchanged post-write queue list must reuse the refreshed snapshot"
    );

    update_queue_task(
        &root,
        QueueTaskUpdateRequest {
            message_key: created.message_key.clone(),
            route_status: Some("blocked".to_string()),
            status_note: Some("cache invalidation after status update".to_string()),
            ..Default::default()
        },
    )
    .expect("failed to update queue task status");

    let pending_after_status_update =
        list_queue_tasks(&root, &pending, 10).expect("failed to list queue after status update");
    assert!(
        pending_after_status_update.is_empty(),
        "cached pending queue snapshot must not survive a status update"
    );
    assert_eq!(
        queue_task_list_cache_miss_count_for_tests(&db_path, &pending, 10),
        3,
        "status updates must invalidate the cached queue snapshot"
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn queue_task_caches_ignore_sync_run_metadata_churn() {
    let root = std::env::temp_dir().join(format!(
        "ctox-queue-cache-sync-run-churn-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("failed to create temp test root");
    let db_path = resolve_db_path(&root, None);
    let pending = vec!["pending".to_string()];

    let listed = list_queue_tasks(&root, &pending, 10).expect("failed to list empty queue");
    assert!(listed.is_empty());
    assert_eq!(
        queue_task_list_cache_miss_count_for_tests(&db_path, &pending, 10),
        1
    );
    let count = count_queue_tasks(&root, &pending).expect("failed to count empty queue");
    assert_eq!(count, 0);
    assert_eq!(
        queue_task_count_cache_miss_count_for_tests(&db_path, &pending),
        1
    );

    let conn = Connection::open(&db_path).expect("failed to open channel db directly");
    conn.execute(
        r#"
        INSERT INTO communication_sync_runs (
            run_key, channel, account_key, folder_hint, started_at, finished_at, ok,
            fetched_count, stored_count, error_text, metadata_json
        ) VALUES (
            'queue-cache-sync-run-churn', 'email', 'email:owner@example.test', 'INBOX',
            '2026-06-27T00:00:00Z', '2026-06-27T00:00:01Z', 1, 10, 10, '', '{}'
        )
        "#,
        [],
    )
    .expect("failed to insert communication sync-run metadata");

    let listed_after_churn =
        list_queue_tasks(&root, &pending, 10).expect("failed to relist after sync churn");
    assert!(listed_after_churn.is_empty());
    assert_eq!(
        queue_task_list_cache_miss_count_for_tests(&db_path, &pending, 10),
        1,
        "sync-run metadata writes must not invalidate cached queue lists"
    );
    let count_after_churn =
        count_queue_tasks(&root, &pending).expect("failed to recount after sync churn");
    assert_eq!(count_after_churn, 0);
    assert_eq!(
        queue_task_count_cache_miss_count_for_tests(&db_path, &pending),
        1,
        "sync-run metadata writes must not invalidate cached queue counts"
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn queue_task_count_cache_reuses_idle_reads_until_store_changes() {
    let root = std::env::temp_dir().join(format!(
        "ctox-queue-count-cache-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("failed to create temp test root");
    let db_path = resolve_db_path(&root, None);
    let pending = vec!["pending".to_string()];
    let blocked = vec!["blocked".to_string()];

    let first = count_queue_tasks(&root, &pending).expect("failed to count empty queue");
    assert_eq!(first, 0);
    assert_eq!(
        queue_task_count_cache_miss_count_for_tests(&db_path, &pending),
        1,
        "first queue count must hit SQLite"
    );

    let second = count_queue_tasks(&root, &pending).expect("failed to recount empty queue");
    assert_eq!(second, 0);
    assert_eq!(
        queue_task_count_cache_miss_count_for_tests(&db_path, &pending),
        1,
        "unchanged idle queue count must reuse the cached count"
    );

    let created = create_queue_task(
        &root,
        QueueTaskCreateRequest {
            title: "count cache invalidation".to_string(),
            prompt: "Make the queue count cache refresh after a store write.".to_string(),
            thread_key: "queue/count-cache".to_string(),
            workspace_root: None,
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )
    .expect("failed to create queue task");

    let after_write =
        count_queue_tasks(&root, &pending).expect("failed to count queue after write");
    assert_eq!(after_write, 1);
    assert_eq!(
        queue_task_count_cache_miss_count_for_tests(&db_path, &pending),
        2,
        "store writes must invalidate the cached queue count"
    );

    let after_idle = count_queue_tasks(&root, &pending).expect("failed to count queue after idle");
    assert_eq!(after_idle, 1);
    assert_eq!(
        queue_task_count_cache_miss_count_for_tests(&db_path, &pending),
        2,
        "unchanged post-write queue count must reuse the refreshed count"
    );

    update_queue_task(
        &root,
        QueueTaskUpdateRequest {
            message_key: created.message_key,
            route_status: Some("blocked".to_string()),
            status_note: Some("count cache invalidation after status update".to_string()),
            ..Default::default()
        },
    )
    .expect("failed to update queue task status");

    let pending_after_status_update = count_queue_tasks(&root, &pending)
        .expect("failed to count pending queue after status update");
    assert_eq!(pending_after_status_update, 0);
    assert_eq!(
        queue_task_count_cache_miss_count_for_tests(&db_path, &pending),
        3,
        "status updates must invalidate the cached pending count"
    );

    let blocked_after_status_update = count_queue_tasks(&root, &blocked)
        .expect("failed to count blocked queue after status update");
    assert_eq!(blocked_after_status_update, 1);
    assert_eq!(
        queue_task_count_cache_miss_count_for_tests(&db_path, &blocked),
        1,
        "a different status set has an independent cache entry"
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn routing_backfill_is_idempotent_under_parallel_schema_open() {
    let db_path = unique_test_db_path("ctox-routing-backfill-parallel");
    {
        let mut conn = open_channel_db(&db_path).expect("failed to open db");
        ensure_account(
            &mut conn,
            QUEUE_ACCOUNT_KEY,
            QUEUE_CHANNEL_NAME,
            QUEUE_ACCOUNT_ADDRESS,
            QUEUE_PROVIDER,
            json!({"source": "ctox-queue"}),
        )
        .expect("failed to create queue account");
        for index in 0..32 {
            let message_key = format!("queue:parallel::{index}");
            let remote_id = format!("remote-parallel-{index}");
            upsert_communication_message(
                &mut conn,
                UpsertMessage {
                    message_key: &message_key,
                    channel: QUEUE_CHANNEL_NAME,
                    account_key: QUEUE_ACCOUNT_KEY,
                    thread_key: "parallel-routing",
                    remote_id: &remote_id,
                    direction: "inbound",
                    folder_hint: "queue",
                    sender_display: QUEUE_SENDER_DISPLAY,
                    sender_address: QUEUE_SENDER_ADDRESS,
                    recipient_addresses_json: "[]",
                    cc_addresses_json: "[]",
                    bcc_addresses_json: "[]",
                    subject: "parallel queue",
                    preview: "parallel queue",
                    body_text: "parallel queue",
                    body_html: "",
                    raw_payload_ref: "",
                    trust_level: "high",
                    status: "received",
                    seen: false,
                    has_attachments: false,
                    external_created_at: "2026-05-13T10:00:00Z",
                    observed_at: "2026-05-13T10:00:00Z",
                    metadata_json: "{}",
                },
            )
            .expect("failed to insert queue message");
        }
    }

    let workers = 12;
    let barrier = Arc::new(Barrier::new(workers));
    let handles = (0..workers)
        .map(|_| {
            let db_path = db_path.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                open_channel_db(&db_path).map(|_| ())
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle
            .join()
            .expect("routing backfill thread panicked")
            .expect("parallel schema open should not race on routing state");
    }

    let conn = open_channel_db(&db_path).expect("failed to reopen db");
    let routing_rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM communication_routing_state WHERE message_key LIKE 'queue:parallel::%'",
            [],
            |row| row.get(0),
        )
        .expect("failed to count routing rows");
    assert_eq!(routing_rows, 32);

    let _ = fs::remove_file(&db_path);
}

#[test]
fn channel_schema_is_ensured_once_per_open_database_file() {
    let db_path = unique_test_db_path("ctox-channel-schema-once");
    let conn = open_channel_db(&db_path).expect("failed to open db first time");
    drop(conn);
    assert_eq!(
        channel_schema_ensure_count_for_tests(&db_path),
        1,
        "first open must install the channel schema"
    );
    assert_eq!(
        channel_open_routing_ensure_count_for_tests(&db_path),
        1,
        "first open must run the routing backfill"
    );

    let conn = open_channel_db(&db_path).expect("failed to open db second time");
    drop(conn);
    assert_eq!(
        channel_schema_ensure_count_for_tests(&db_path),
        1,
        "second open of the same database file must not re-run schema DDL"
    );
    let settled_routing_ensure_count = channel_open_routing_ensure_count_for_tests(&db_path);
    assert!(
        (1..=2).contains(&settled_routing_ensure_count),
        "routing backfill should run at most once more while SQLite settles a newly created WAL database"
    );

    let conn = open_channel_db(&db_path).expect("failed to open db third time");
    drop(conn);
    assert_eq!(
        channel_open_routing_ensure_count_for_tests(&db_path),
        settled_routing_ensure_count,
        "subsequent opens of a settled database file must not re-run routing backfill"
    );

    let _ = fs::remove_file(&db_path);
}

#[test]
fn queue_thread_refresh_failure_does_not_leave_a_committed_lease() {
    let root = std::env::temp_dir().join(format!(
        "ctox-queue-refresh-failure-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("failed to create temp test root");

    let created = create_queue_task(
        &root,
        QueueTaskCreateRequest {
            title: "refresh failure before lease".to_string(),
            prompt: "Keep this task pending when thread refresh fails.".to_string(),
            thread_key: "queue/lease-refresh-failure".to_string(),
            workspace_root: None,
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )
    .expect("failed to create queue task");
    let conn = open_channel_db(&resolve_db_path(&root, None)).expect("open channel db");
    conn.execute_batch(
        r#"
        CREATE TRIGGER fail_queue_thread_refresh_before_lease
        BEFORE UPDATE ON communication_threads
        WHEN NEW.thread_key = 'queue/lease-refresh-failure'
        BEGIN
            SELECT RAISE(ABORT, 'injected thread refresh failure');
        END;
        "#,
    )
    .expect("install refresh failure trigger");
    drop(conn);

    let error = lease_queue_task(&root, &created.message_key, "ctox-service")
        .expect_err("thread refresh failure must abort before queue lease commit");
    assert!(error
        .to_string()
        .contains("injected thread refresh failure"));
    let reloaded = load_queue_task(&root, &created.message_key)
        .expect("failed to load queue task")
        .expect("missing queue task");
    assert_eq!(reloaded.route_status, "pending");
    assert!(reloaded.lease_owner.is_none());
    assert!(reloaded.leased_at.is_none());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn incident_lease_heartbeat_preserves_live_work_then_expiry_requeues_it() {
    let root = std::env::temp_dir().join(format!(
        "ctox-incident-lease-heartbeat-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).unwrap();
    let task = create_queue_task(
        &root,
        QueueTaskCreateRequest {
            title: "incident heartbeat acceptance".to_string(),
            prompt: "Research with a bounded queue lease".to_string(),
            thread_key: "incident/heartbeat".to_string(),
            workspace_root: None,
            priority: "urgent".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )
    .unwrap();
    lease_queue_task(&root, &task.message_key, "worker-before-restart").unwrap();
    record_queue_lease_worker(
        &root,
        &[task.message_key.clone()],
        "worker-before-restart",
        "attempt-1",
    )
    .unwrap();
    assert_eq!(
        renew_message_leases(&root, "worker-before-restart", &[task.message_key.clone()]).unwrap(),
        1
    );
    let sweep = release_stale_queue_task_leases(&root, "new-daemon", &HashSet::new()).unwrap();
    assert!(sweep.released.is_empty());
    assert!(sweep.failures.is_empty());

    // Advance the persisted clock beyond TTL without another heartbeat;
    // opening the store again models a worker/process disappearing.
    let conn = open_channel_db(&resolve_db_path(&root, None)).unwrap();
    conn.execute("UPDATE communication_routing_state SET lease_expires_at='2000-01-01T00:00:00Z' WHERE message_key=?1", params![task.message_key]).unwrap();
    drop(conn);
    let sweep = release_stale_queue_task_leases(&root, "new-daemon", &HashSet::new()).unwrap();
    assert_eq!(sweep.released, vec![task.message_key.clone()]);
    assert!(sweep.failures.is_empty());
    assert_eq!(
        load_queue_task(&root, &task.message_key)
            .unwrap()
            .unwrap()
            .route_status,
        "pending"
    );
    let conn = open_channel_db(&resolve_db_path(&root, None)).unwrap();
    let cleared: bool = conn.query_row("SELECT lease_owner IS NULL AND leased_at IS NULL AND lease_expires_at IS NULL AND lease_worker_id IS NULL FROM communication_routing_state WHERE message_key=?1", params![task.message_key], |row| row.get(0)).unwrap();
    assert!(cleared);
    assert_eq!(
        renew_message_leases(&root, "worker-before-restart", &[task.message_key.clone()]).unwrap(),
        0
    );
    assert!(
        release_stale_queue_task_leases(&root, "new-daemon", &HashSet::new())
            .unwrap()
            .released
            .is_empty()
    );
    drop(conn);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn incident_sweep_reclaims_old_leases_without_expiry_or_worker_id() {
    for missing_expiry in [None, Some("")] {
        let root = std::env::temp_dir().join(format!(
            "ctox-incident-legacy-lease-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let task = create_queue_task(
            &root,
            QueueTaskCreateRequest {
                title: "legacy lease without expiry".to_string(),
                prompt: "Recover work whose worker disappeared".to_string(),
                thread_key: "incident/legacy-lease".to_string(),
                workspace_root: None,
                priority: "normal".to_string(),
                suggested_skill: None,
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .unwrap();
        lease_queue_task(&root, &task.message_key, "previous-daemon").unwrap();
        // A raw connection preserves the legacy shape until the production
        // sweep opens the store. Do not backfill between injection and sweep.
        let conn = Connection::open(resolve_db_path(&root, None)).unwrap();
        let bounded_without_worker: bool = conn
            .query_row(
                "SELECT datetime(lease_expires_at) > datetime(leased_at)
                        AND lease_worker_id IS NULL
                 FROM communication_routing_state WHERE message_key=?1",
                params![task.message_key],
                |row| row.get(0),
            )
            .unwrap();
        assert!(bounded_without_worker);
        conn.execute(
            "UPDATE communication_routing_state
             SET leased_at='2000-01-01T00:00:00Z', lease_expires_at=?2,
                 lease_worker_id=NULL WHERE message_key=?1",
            params![task.message_key, missing_expiry],
        )
        .unwrap();
        let legacy_shape: bool = conn
            .query_row(
                "SELECT route_status='leased' AND leased_at='2000-01-01T00:00:00Z'
                        AND lease_expires_at IS ?2 AND lease_worker_id IS NULL
                 FROM communication_routing_state WHERE message_key=?1",
                params![task.message_key, missing_expiry],
                |row| row.get(0),
            )
            .unwrap();
        assert!(legacy_shape);
        drop(conn);

        let sweep = release_stale_queue_task_leases(&root, "new-daemon", &HashSet::new()).unwrap();
        assert!(sweep.failures.is_empty());
        assert_eq!(sweep.released, vec![task.message_key.clone()]);
        let conn = Connection::open(resolve_db_path(&root, None)).unwrap();
        let recovered: bool = conn
            .query_row(
                "SELECT route_status='pending' AND lease_owner IS NULL
                        AND leased_at IS NULL AND lease_expires_at IS NULL
                        AND lease_worker_id IS NULL
                 FROM communication_routing_state WHERE message_key=?1",
                params![task.message_key],
                |row| row.get(0),
            )
            .unwrap();
        assert!(recovered);
        drop(conn);
        assert!(
            release_stale_queue_task_leases(&root, "new-daemon", &HashSet::new())
                .unwrap()
                .released
                .is_empty()
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn stale_queue_task_lease_releases_to_pending() {
    let root = std::env::temp_dir().join(format!(
        "ctox-queue-stale-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("failed to create temp test root");

    let created = create_queue_task(
        &root,
        QueueTaskCreateRequest {
            title: "stale lease".to_string(),
            prompt: "Release this stale queue lease.".to_string(),
            thread_key: "queue/stale".to_string(),
            workspace_root: None,
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )
    .expect("failed to create queue task");
    lease_queue_task(&root, &created.message_key, "ctox-service")
        .expect("failed to lease queue task");
    let conn = open_channel_db(&resolve_db_path(&root, None)).expect("open channel db");
    conn.execute(
        "UPDATE communication_routing_state SET lease_expires_at='2000-01-01T00:00:00Z' WHERE message_key=?1",
        params![created.message_key],
    )
    .expect("expire queue lease");
    drop(conn);

    let sweep = release_stale_queue_task_leases(&root, "ctox-service", &HashSet::new())
        .expect("failed to release stale queue lease");
    assert_eq!(sweep.released, vec![created.message_key.clone()]);
    assert!(sweep.failures.is_empty());
    let reloaded = load_queue_task(&root, &created.message_key)
        .expect("failed to load queue task")
        .expect("missing queue task");
    assert_eq!(reloaded.route_status, "pending");
    assert!(reloaded.lease_owner.is_none());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn stale_queue_task_lease_recovery_does_not_clobber_concurrent_renewal() {
    let root = std::env::temp_dir().join(format!(
        "ctox-queue-stale-renew-race-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("failed to create temp test root");

    let created = create_queue_task(
        &root,
        QueueTaskCreateRequest {
            title: "stale lease renewal race".to_string(),
            prompt: "Preserve the renewed lease.".to_string(),
            thread_key: "queue/stale-renew-race".to_string(),
            workspace_root: None,
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )
    .expect("failed to create queue task");
    lease_queue_task(&root, &created.message_key, "ctox-service")
        .expect("failed to lease queue task");

    let db_path = resolve_db_path(&root, None);
    let blocker = open_channel_db(&db_path).expect("open renewal connection");
    blocker
        .execute(
            "UPDATE communication_routing_state
             SET lease_expires_at='2000-01-01T00:00:00Z',
                 lease_worker_id='worker-old'
             WHERE message_key=?1",
            params![created.message_key],
        )
        .expect("expire queue lease");
    blocker
        .execute_batch("BEGIN IMMEDIATE")
        .expect("begin concurrent renewal");
    blocker
        .execute(
            "UPDATE communication_routing_state
             SET leased_at='2099-01-01T00:00:00Z',
                 lease_expires_at='2099-01-01T00:15:00Z',
                 lease_worker_id='worker-renewed'
             WHERE message_key=?1",
            params![created.message_key],
        )
        .expect("stage renewed lease");

    let sweep_root = root.clone();
    let sweep_thread = std::thread::spawn(move || {
        release_stale_queue_task_leases(&sweep_root, "ctox-service", &HashSet::new())
    });
    // The sweep can read the previously committed stale identity while the
    // renewal transaction holds SQLite's writer lock, then must re-check
    // that identity atomically once the renewal commits.
    std::thread::sleep(std::time::Duration::from_millis(100));
    blocker
        .execute_batch("COMMIT")
        .expect("commit concurrent renewal");

    let sweep = sweep_thread
        .join()
        .expect("sweep thread panicked")
        .expect("sweep failed");
    assert!(sweep.released.is_empty());
    assert!(sweep.failures.is_empty());

    let reloaded = load_queue_task(&root, &created.message_key)
        .expect("failed to load queue task")
        .expect("missing queue task");
    assert_eq!(reloaded.route_status, "leased");
    assert_eq!(reloaded.lease_owner.as_deref(), Some("ctox-service"));
    let conn = open_channel_db(&db_path).expect("open lease verification connection");
    let (worker_id, lease_expires_at): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT lease_worker_id, lease_expires_at
             FROM communication_routing_state
             WHERE message_key=?1",
            params![created.message_key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("read renewed lease identity");
    assert_eq!(
        worker_id.as_deref(),
        Some("worker-renewed"),
        "the recovery CAS must preserve the concurrent worker identity"
    );
    assert_eq!(
        lease_expires_at.as_deref(),
        Some("2099-01-01T00:15:00Z"),
        "the recovery CAS must preserve the renewed expiry"
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ownerless_queue_task_lease_releases_immediately() {
    let root = std::env::temp_dir().join(format!(
        "ctox-queue-ownerless-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("failed to create temp test root");

    let created = create_queue_task(
        &root,
        QueueTaskCreateRequest {
            title: "ownerless lease".to_string(),
            prompt: "Recover this incomplete queue lease.".to_string(),
            thread_key: "queue/ownerless".to_string(),
            workspace_root: None,
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )
    .expect("failed to create queue task");
    lease_queue_task(&root, &created.message_key, "ctox-service")
        .expect("failed to lease queue task");
    let conn = open_channel_db(&resolve_db_path(&root, None)).expect("open channel db");
    conn.execute(
        "UPDATE communication_routing_state
         SET lease_owner=NULL, leased_at=NULL
         WHERE message_key=?1",
        params![created.message_key],
    )
    .expect("make queue lease ownerless");
    drop(conn);

    let sweep = release_stale_queue_task_leases(&root, "ctox-service", &HashSet::new())
        .expect("failed to release ownerless queue lease");
    assert_eq!(sweep.released, vec![created.message_key.clone()]);
    assert!(sweep.failures.is_empty());
    let reloaded = load_queue_task(&root, &created.message_key)
        .expect("failed to load queue task")
        .expect("missing queue task");
    assert_eq!(reloaded.route_status, "pending");
    assert!(reloaded.lease_owner.is_none());
    assert!(reloaded.leased_at.is_none());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn terminal_command_orphaned_queue_lease_settles_to_terminal_route() {
    let root = std::env::temp_dir().join(format!(
        "ctox-queue-orphaned-terminal-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("failed to create temp test root");

    let created = create_queue_task(
        &root,
        QueueTaskCreateRequest {
            title: "orphaned terminal lease".to_string(),
            prompt: "Settle this orphaned queue lease.".to_string(),
            thread_key: "queue/orphaned-terminal".to_string(),
            workspace_root: None,
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )
    .expect("failed to create queue task");
    lease_queue_task(&root, &created.message_key, "ctox-service")
        .expect("failed to lease queue task");
    let conn = open_channel_db(&resolve_db_path(&root, None)).expect("open channel db");
    // F-002 regression fixture: the linked Business OS command reached a
    // durable terminal state while its queue route stayed `leased` — the
    // exact orphaned-lease shape production showed (acked_at NULL, no
    // active worker, no heartbeat).
    conn.execute(
        "INSERT INTO business_command_aggregates (
            command_id, idempotency_key, payload_hash, module, command_type,
            execution_mode, execution_phase, terminal_status, intent_json,
            created_at_ms, updated_at_ms
         ) VALUES (?1, ?2, 'hash', 'ctox', 'ctox.research.run', 'queue', 'terminal', 'failed', '{}', 1, 1)",
        params!["cmd-orphaned-1", "idem-orphaned-1"],
    )
    .expect("seed terminal command aggregate");
    conn.execute(
        "INSERT INTO business_command_task_links (command_id, task_id, created_at_ms) VALUES (?1, ?2, 1)",
        params!["cmd-orphaned-1", created.message_key],
    )
    .expect("link terminal command to queue task");
    conn.execute(
        "UPDATE communication_routing_state SET lease_expires_at='2000-01-01T00:00:00Z' WHERE message_key=?1",
        params![created.message_key],
    )
    .expect("expire queue lease");
    drop(conn);

    let sweep = release_stale_queue_task_leases(&root, "ctox-service", &HashSet::new())
        .expect("failed to settle orphaned queue lease");
    assert_eq!(sweep.released, vec![created.message_key.clone()]);
    assert!(sweep.failures.is_empty());
    let reloaded = load_queue_task(&root, &created.message_key)
        .expect("failed to load queue task")
        .expect("missing queue task");
    // The orphaned lease must settle to the terminal route matching the
    // durable command — never back to pending (which would duplicate the
    // terminal command) and never left `leased` as phantom progress.
    assert_eq!(reloaded.route_status, "failed");
    assert!(reloaded.lease_owner.is_none());
    assert!(reloaded.leased_at.is_none());
    // Idempotent: the settled terminal row is no longer a stale-lease
    // candidate, so a second sweep pass is a no-op.
    let second = release_stale_queue_task_leases(&root, "ctox-service", &HashSet::new())
        .expect("second sweep pass failed");
    assert!(second.released.is_empty());
    assert!(second.failures.is_empty());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn failed_app_create_command_can_retry_with_same_command_and_queue_ids() {
    let root = std::env::temp_dir().join(format!(
        "ctox-app-create-terminal-retry-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("failed to create temp test root");
    let created = create_queue_task(
        &root,
        QueueTaskCreateRequest {
            title: "retry app import".to_string(),
            prompt: "Retry the immutable app import.".to_string(),
            thread_key: "queue/app-create-terminal-retry".to_string(),
            workspace_root: None,
            priority: "normal".to_string(),
            suggested_skill: Some("business-os-app-module-development".to_string()),
            parent_message_key: None,
            extra_metadata: None,
        },
    )
    .expect("failed to create queue task");
    update_queue_task(
        &root,
        QueueTaskUpdateRequest {
            message_key: created.message_key.clone(),
            route_status: Some("failed".to_string()),
            status_note: Some("model API unavailable".to_string()),
            ..Default::default()
        },
    )
    .expect("failed to terminalize app queue task");
    let conn = open_channel_db(&resolve_db_path(&root, None)).expect("open channel db");
    conn.execute(
        "INSERT INTO business_command_aggregates (
            command_id, idempotency_key, payload_hash, module, command_type,
            execution_mode, execution_phase, terminal_status, projection_version,
            intent_json, error_code, error_message, created_at_ms, updated_at_ms
         ) VALUES (?1, ?2, 'hash', 'importer', 'ctox.business_os.app.create',
                   'queue', 'terminal', 'failed', 4, '{}', 'runtime_api',
                   'model API unavailable', 1, 1)",
        params!["cmd-app-retry-1", "idem-app-retry-1"],
    )
    .expect("seed terminal app command aggregate");
    conn.execute(
        "INSERT INTO business_command_task_links (command_id, task_id, created_at_ms)
         VALUES (?1, ?2, 1)",
        params!["cmd-app-retry-1", created.message_key],
    )
    .expect("link app command to queue task");
    drop(conn);

    let retried = retry_failed_app_create_business_command(&root, "cmd-app-retry-1")
        .expect("retry failed app-create command");
    assert_eq!(retried["command_id"], "cmd-app-retry-1");
    assert_eq!(retried["task_id"], created.message_key);
    assert_eq!(retried["status"], "queued");
    let task = load_queue_task(&root, &created.message_key)
        .expect("load retried queue task")
        .expect("retried queue task exists");
    assert_eq!(task.route_status, "pending");
    assert!(task.lease_owner.is_none());
    let inspected = inspect_business_command(&root, "cmd-app-retry-1")
        .expect("inspect retried app command")
        .expect("retried app command exists");
    assert_eq!(inspected["command"]["execution_phase"], "queued");
    assert_eq!(inspected["command"]["terminal_status"], "none");
    assert_eq!(inspected["command"]["projection_version"], 5);

    let second = retry_failed_app_create_business_command(&root, "cmd-app-retry-1")
        .expect_err("queued app command must not be retried twice");
    assert!(second.to_string().contains("not a terminal failed"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn queue_lease_worker_identity_persists_and_clears_on_recovery() {
    let root = std::env::temp_dir().join(format!(
        "ctox-queue-lease-worker-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("failed to create temp test root");

    let created = create_queue_task(
        &root,
        QueueTaskCreateRequest {
            title: "worker identity lease".to_string(),
            prompt: "Track worker identity on this lease.".to_string(),
            thread_key: "queue/lease-worker".to_string(),
            workspace_root: None,
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )
    .expect("failed to create queue task");
    lease_queue_task(&root, &created.message_key, "ctox-service")
        .expect("failed to lease queue task");
    let attached = record_queue_lease_worker(
        &root,
        std::slice::from_ref(&created.message_key),
        "ctox-service",
        "boot-test:worker:1",
    )
    .expect("failed to persist lease worker identity");
    assert_eq!(attached, 1);
    let conn = open_channel_db(&resolve_db_path(&root, None)).expect("open channel db");
    let worker_id: Option<String> = conn
        .query_row(
            "SELECT lease_worker_id FROM communication_routing_state WHERE message_key=?1",
            params![created.message_key],
            |row| row.get(0),
        )
        .expect("read lease worker id");
    assert_eq!(worker_id.as_deref(), Some("boot-test:worker:1"));
    conn.execute(
        "UPDATE communication_routing_state SET lease_expires_at='2000-01-01T00:00:00Z' WHERE message_key=?1",
        params![created.message_key],
    )
    .expect("expire queue lease");
    drop(conn);

    // The worker vanished without ack; the recovery sweep releases the
    // orphaned lease and clears the durable worker identity with it.
    let sweep = release_stale_queue_task_leases(&root, "ctox-service", &HashSet::new())
        .expect("failed to release orphaned queue lease");
    assert_eq!(sweep.released, vec![created.message_key.clone()]);
    assert!(sweep.failures.is_empty());
    let conn = open_channel_db(&resolve_db_path(&root, None)).expect("open channel db");
    let cleared: Option<String> = conn
        .query_row(
            "SELECT lease_worker_id FROM communication_routing_state WHERE message_key=?1",
            params![created.message_key],
            |row| row.get(0),
        )
        .expect("read lease worker id after sweep");
    assert!(cleared.is_none());
    drop(conn);
    // A stale worker must not restamp a lease that was already reclaimed:
    // the attach is constrained to rows still leased by the same owner.
    let restamped = record_queue_lease_worker(
        &root,
        std::slice::from_ref(&created.message_key),
        "ctox-service",
        "boot-test:worker:1",
    )
    .expect("stale worker attach failed");
    assert_eq!(restamped, 0);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn queue_task_lease_stalled_probe_tracks_heartbeat_expiry() {
    let root = std::env::temp_dir().join(format!(
        "ctox-queue-lease-stalled-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("failed to create temp test root");

    let created = create_queue_task(
        &root,
        QueueTaskCreateRequest {
            title: "stalled probe".to_string(),
            prompt: "Probe lease health transitions.".to_string(),
            thread_key: "queue/lease-stalled".to_string(),
            workspace_root: None,
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )
    .expect("failed to create queue task");
    // A pending task has no lease at all: not stalled.
    assert!(!queue_task_lease_stalled(&root, &created.message_key)
        .expect("stalled probe failed for pending task"));
    lease_queue_task(&root, &created.message_key, "ctox-service")
        .expect("failed to lease queue task");
    // A fresh lease with a future expiry has a live heartbeat window.
    assert!(!queue_task_lease_stalled(&root, &created.message_key)
        .expect("stalled probe failed for fresh lease"));
    let conn = open_channel_db(&resolve_db_path(&root, None)).expect("open channel db");
    conn.execute(
        "UPDATE communication_routing_state SET lease_expires_at='2000-01-01T00:00:00Z' WHERE message_key=?1",
        params![created.message_key],
    )
    .expect("expire queue lease");
    drop(conn);
    // Heartbeat stopped and the expiry passed: the lease is stalled and
    // must surface as such, never as healthy progress.
    assert!(queue_task_lease_stalled(&root, &created.message_key)
        .expect("stalled probe failed for expired lease"));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn typed_holds_block_on_wait_ref_and_budget_technical_retries() {
    let root = std::env::temp_dir().join(format!(
        "ctox-queue-typed-hold-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("runtime")).expect("create runtime dir");

    let waiting = create_queue_task(
        &root,
        QueueTaskCreateRequest {
            title: "wait for approval".to_string(),
            prompt: "Wait without running the model.".to_string(),
            thread_key: "queue/wait-ref".to_string(),
            workspace_root: None,
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )
    .expect("create waiting task");
    lease_queue_task(&root, &waiting.message_key, "ctox-test").expect("lease waiting task");
    hold_leased_messages(
        &root,
        std::slice::from_ref(&waiting.message_key),
        &HoldReason::WaitingExternal(crate::mission::plan::WaitRef {
            entity_type: "approval-gate".to_string(),
            entity_id: "gate-42".to_string(),
        }),
        "approval is still open",
    )
    .expect("block waiting task");
    assert_eq!(
        load_queue_task(&root, &waiting.message_key)
            .expect("load waiting task")
            .expect("waiting task exists")
            .route_status,
        "blocked"
    );
    assert!(lease_queue_task(&root, &waiting.message_key, "ctox-test").is_err());
    assert_eq!(
        wake_messages_waiting_for(&root, "approval-gate", "gate-42").expect("wake waiting task"),
        1
    );
    assert_eq!(
        load_queue_task(&root, &waiting.message_key)
            .expect("load woken task")
            .expect("woken task exists")
            .route_status,
        "pending"
    );

    let technical = create_queue_task(
        &root,
        QueueTaskCreateRequest {
            title: "bounded technical hold".to_string(),
            prompt: "Retry with a finite budget.".to_string(),
            thread_key: "queue/technical-hold".to_string(),
            workspace_root: None,
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )
    .expect("create technical task");
    for attempt in 1..=5 {
        lease_queue_task(&root, &technical.message_key, "ctox-test").expect("lease technical task");
        hold_leased_messages(
            &root,
            std::slice::from_ref(&technical.message_key),
            &HoldReason::MissingReviewEvidence,
            "review evidence unavailable",
        )
        .expect("persist technical hold");
        let conn = open_channel_db(&resolve_db_path(&root, None)).expect("open channel db");
        let (status, attempts): (String, i64) = conn
            .query_row(
                "SELECT route_status, failure_attempt_count FROM communication_routing_state WHERE message_key=?1",
                params![technical.message_key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("load hold state");
        assert_eq!(attempts, attempt);
        assert_eq!(status, if attempt == 5 { "failed" } else { "pending" });
        if attempt < 5 {
            conn.execute(
                "UPDATE communication_routing_state SET retry_not_before='2000-01-01T00:00:00Z' WHERE message_key=?1",
                params![technical.message_key],
            )
            .expect("expire hold backoff");
        }
    }

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn attempt_bound_hold_and_failed_ack_do_not_rewrite_on_resume() -> Result<()> {
    let root = tempfile::tempdir()?;
    fs::create_dir_all(root.path().join("runtime"))?;
    let db_path = resolve_db_path(root.path(), None);
    let engine =
        crate::context::lcm::LcmEngine::open(&db_path, crate::context::lcm::LcmConfig::default())?;

    let held = create_queue_task(
        root.path(),
        QueueTaskCreateRequest {
            title: "attempt-bound hold".to_string(),
            prompt: "Hold once across finalization resume.".to_string(),
            thread_key: "queue/attempt-hold".to_string(),
            workspace_root: None,
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )?;
    lease_queue_task(root.path(), &held.message_key, "ctox-test")?;
    engine.begin_worker_attempt_finalization(
        crate::context::lcm::WorkerAttemptFinalizationInput {
            attempt_id: "attempt-hold-once",
            work_key: "queue:attempt-hold-once",
            conversation_id: 7201,
            source_label: "queue-test",
            agent_outcome: crate::context::lcm::AgentOutcome::Success,
            reply_text: "held",
            error_text: None,
        },
    )?;
    assert_eq!(
        hold_leased_messages_for_attempt(
            root.path(),
            "attempt-hold-once",
            std::slice::from_ref(&held.message_key),
            &HoldReason::MissingArtifact,
            "artifact is still missing",
        )?,
        1
    );
    let held_once: (i64, String, Option<String>) = open_channel_db(&db_path)?.query_row(
        "SELECT failure_attempt_count, updated_at, retry_not_before
         FROM communication_routing_state WHERE message_key = ?1",
        [held.message_key.as_str()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    thread::sleep(std::time::Duration::from_millis(2));
    assert_eq!(
        hold_leased_messages_for_attempt(
            root.path(),
            "attempt-hold-once",
            std::slice::from_ref(&held.message_key),
            &HoldReason::MissingArtifact,
            "artifact is still missing",
        )?,
        0
    );
    let held_after_resume: (i64, String, Option<String>) = open_channel_db(&db_path)?.query_row(
        "SELECT failure_attempt_count, updated_at, retry_not_before
         FROM communication_routing_state WHERE message_key = ?1",
        [held.message_key.as_str()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    assert_eq!(held_after_resume, held_once);

    let failed = create_queue_task(
        root.path(),
        QueueTaskCreateRequest {
            title: "attempt-bound failed ack".to_string(),
            prompt: "Fail once across finalization resume.".to_string(),
            thread_key: "queue/attempt-failed".to_string(),
            workspace_root: None,
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )?;
    lease_queue_task(root.path(), &failed.message_key, "ctox-test")?;
    engine.begin_worker_attempt_finalization(
        crate::context::lcm::WorkerAttemptFinalizationInput {
            attempt_id: "attempt-failed-once",
            work_key: "queue:attempt-failed-once",
            conversation_id: 7202,
            source_label: "queue-test",
            agent_outcome: crate::context::lcm::AgentOutcome::ExecutionError,
            reply_text: "failed",
            error_text: Some("terminal worker failure"),
        },
    )?;
    assert_eq!(
        ack_leased_messages_for_attempt(
            root.path(),
            "attempt-failed-once",
            std::slice::from_ref(&failed.message_key),
            "failed",
            Some("terminal worker failure"),
        )?,
        1
    );
    let failed_once: (String, Option<String>, String) = open_channel_db(&db_path)?.query_row(
        "SELECT updated_at, acked_at, last_error
         FROM communication_routing_state WHERE message_key = ?1",
        [failed.message_key.as_str()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    thread::sleep(std::time::Duration::from_millis(2));
    assert_eq!(
        ack_leased_messages_for_attempt(
            root.path(),
            "attempt-failed-once",
            std::slice::from_ref(&failed.message_key),
            "failed",
            Some("terminal worker failure"),
        )?,
        0
    );
    let failed_after_resume: (String, Option<String>, String) = open_channel_db(&db_path)?
        .query_row(
            "SELECT updated_at, acked_at, last_error
             FROM communication_routing_state WHERE message_key = ?1",
            [failed.message_key.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
    assert_eq!(failed_after_resume, failed_once);
    Ok(())
}

#[test]
fn tui_ingest_sanitizes_minimax_secret_before_persisting_message() -> Result<()> {
    let root = std::env::temp_dir().join(format!(
        "ctox-tui-secret-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("runtime"))?;
    let db_path = resolve_db_path(&root, None);
    let mut conn = open_channel_db(&db_path)?;
    let fake_key = "sk-api-test_minimax_abcdefghijklmnopqrstuvwxyz0123456789";

    let stored = ingest_tui_message(
        &root,
        &mut conn,
        TuiIngestRequest {
            account_key: "local".to_string(),
            thread_key: "example-supervisor".to_string(),
            body: format!(
                "MiniMax API key fuer MiniMax M2.7: {fake_key}. Bitte mit Secret-Skill ablegen."
            ),
            subject: "MiniMax key".to_string(),
            sender_display: "Codex".to_string(),
            sender_address: "tui:codex".to_string(),
            metadata: json!({"source": "test"}),
        },
    )?;
    let message_key = stored
        .get("message_key")
        .and_then(Value::as_str)
        .context("missing stored message key")?;
    let (body_text, preview, metadata_json): (String, String, String) = conn.query_row(
        "SELECT body_text, preview, metadata_json FROM communication_messages WHERE message_key = ?1",
        [message_key],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let metadata: Value = serde_json::from_str(&metadata_json)?;

    assert!(!body_text.contains(fake_key));
    assert!(!preview.contains(fake_key));
    assert!(body_text.contains("[secret-ref:credentials/MINIMAX_API_KEY"));
    assert_eq!(
        secrets::read_secret_value(&root, "credentials", "MINIMAX_API_KEY")?,
        fake_key
    );
    assert_eq!(
        metadata.get("secret_sanitized").and_then(Value::as_bool),
        Some(true)
    );

    let _ = fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn sync_prompt_identity_persists_founder_role_titles() {
    let root = std::env::temp_dir().join(format!(
        "ctox-founder-role-sync-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("runtime")).expect("failed to create temp test root");

    let mut settings = BTreeMap::new();
    settings.insert(
        "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
        "founder@example.com".to_string(),
    );
    settings.insert(
        "CTOX_FOUNDER_EMAIL_ADDRESSES".to_string(),
        "founder@example.com,s.mueller@example.com".to_string(),
    );
    settings.insert(
        "CTOX_FOUNDER_EMAIL_ROLES".to_string(),
        "founder@example.com=CEO / Founder,s.mueller@example.com=Sales Officer".to_string(),
    );

    sync_prompt_identity(&root, &settings).expect("failed to sync prompt identity");

    let db_path = resolve_db_path(&root, None);
    let conn = open_channel_db(&db_path).expect("failed to open channel db");
    let metadata_json: String = conn
        .query_row(
            "SELECT metadata_json FROM owner_profiles WHERE owner_key = ?1",
            ["s.mueller@example.com"],
            |row| row.get(0),
        )
        .expect("failed to load founder profile");
    let metadata: Value =
        serde_json::from_str(&metadata_json).expect("failed to parse founder metadata");

    assert_eq!(
        metadata.get("role").and_then(Value::as_str),
        Some("founder")
    );
    assert_eq!(
        metadata.get("role_title").and_then(Value::as_str),
        Some("Sales Officer")
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn owner_profile_settings_merge_adds_founder_mailboxes_for_routing() {
    let root = std::env::temp_dir().join(format!(
        "ctox-founder-profile-settings-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("runtime")).expect("failed to create temp test root");

    let db_path = resolve_db_path(&root, None);
    let mut conn = open_channel_db(&db_path).expect("failed to open channel db");
    upsert_identity_profile(
        &mut conn,
        "mp@example.com",
        "Marco Pucciarelli",
        json!({
            "email": "mp@example.com",
            "role": "founder",
            "role_title": "CFO / Founder",
            "allow_admin_actions": true,
            "allow_sudo_actions": false,
            "mail_instruction_scope": "founder_strategic",
        }),
    )
    .expect("failed to insert founder profile");

    let mut settings = BTreeMap::new();
    settings.insert(
        "CTOX_FOUNDER_EMAIL_ADDRESSES".to_string(),
        "s.mueller@example.com".to_string(),
    );

    merge_owner_profile_settings(&root, &mut settings).expect("failed to merge owner profiles");

    let policy = classify_email_sender(&settings, "mp@example.com");
    assert!(policy.allowed);
    assert_eq!(policy.role, "founder");
    assert!(settings
        .get("CTOX_FOUNDER_EMAIL_ROLES")
        .is_some_and(|roles| roles.contains("mp@example.com=CFO / Founder")));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn founder_ack_guard_uses_sqlite_owner_profiles() {
    let root = std::env::temp_dir().join(format!(
        "ctox-founder-ack-owner-profiles-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("runtime")).expect("failed to create temp test root");

    let db_path = resolve_db_path(&root, None);
    let mut conn = open_channel_db(&db_path).expect("failed to open channel db");
    upsert_identity_profile(
        &mut conn,
        "mp@example.com",
        "Marco Pucciarelli",
        json!({
            "email": "mp@example.com",
            "role": "founder",
            "role_title": "CFO / Founder",
        }),
    )
    .expect("failed to insert founder profile");
    let message_key = "email:cto1@example.com::INBOX::101";
    conn.execute(
        r#"INSERT INTO communication_messages (
            message_key, channel, account_key, thread_key, remote_id, direction, folder_hint,
            sender_display, sender_address, recipient_addresses_json, cc_addresses_json,
            bcc_addresses_json, subject, preview, body_text, body_html, raw_payload_ref,
            trust_level, status, seen, has_attachments, external_created_at, observed_at,
            metadata_json
        ) VALUES (
            ?1, 'email', 'email:cto1@example.com', 'crm-thread',
            '101', 'inbound', 'INBOX', 'Marco Pucciarelli',
            'mp@example.com', '[]', '[]', '[]',
            'AW: Example CRM', 'CRM reply', 'Bitte beantworten.',
            '', '', 'normal', 'received', 0, 0,
            '2026-04-29T06:51:46Z', '2026-04-29T08:13:36Z', '{}'
        )"#,
        params![message_key],
    )
    .expect("failed to insert founder inbound");
    conn.execute(
        r#"INSERT INTO communication_routing_state (
            message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
        ) VALUES (?1, 'leased', 'ctox-service', '2026-04-29T08:20:00Z', NULL, NULL, '2026-04-29T08:20:00Z')"#,
        params![message_key],
    )
    .expect("failed to insert route");

    let err = ack_leased_messages(&root, &[message_key.to_string()], "handled")
        .expect_err("founder mail must not be handled without reviewed send proof");
    assert!(err
        .to_string()
        .contains("cannot mark founder/owner/admin inbound mail as handled"));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn queue_task_workspace_root_round_trips_and_legacy_prompt_falls_back() {
    let root = std::env::temp_dir().join(format!(
        "ctox-queue-workspace-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("failed to create temp test root");

    let explicit = create_queue_task(
        &root,
        QueueTaskCreateRequest {
            title: "workspace explicit".to_string(),
            prompt: "Build only in the assigned workspace.".to_string(),
            thread_key: "queue/workspace-explicit".to_string(),
            workspace_root: Some("/tmp/ctox-explicit-workspace".to_string()),
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )
    .expect("failed to create explicit workspace task");
    assert_eq!(
        explicit.workspace_root.as_deref(),
        Some("/tmp/ctox-explicit-workspace")
    );

    let legacy = create_queue_task(
        &root,
        QueueTaskCreateRequest {
            title: "workspace legacy".to_string(),
            prompt: "Arbeite ausschließlich im Verzeichnis /tmp/ctox-legacy-workspace.\n\nImplementiere die Aufgabe dort.".to_string(),
            thread_key: "queue/workspace-legacy".to_string(),
            workspace_root: None,
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )
    .expect("failed to create legacy workspace task");
    assert_eq!(
        legacy.workspace_root.as_deref(),
        Some("/tmp/ctox-legacy-workspace")
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn failed_queue_ack_requires_and_persists_failure_reason() {
    let root = std::env::temp_dir().join(format!(
        "ctox-queue-failed-ack-reason-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("failed to create temp test root");

    let task = create_queue_task(
        &root,
        QueueTaskCreateRequest {
            title: "queue failure reason".to_string(),
            prompt: "Exercise failed ack reason handling.".to_string(),
            thread_key: "queue/failure-reason".to_string(),
            workspace_root: None,
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )
    .expect("failed to create queue task");
    lease_queue_task(&root, &task.message_key, "ctox-test").expect("failed to lease queue task");

    let err = ack_leased_messages(&root, std::slice::from_ref(&task.message_key), "failed")
        .expect_err("failed ack without reason must be rejected");
    assert!(err
        .to_string()
        .contains("failed queue ack requires a non-empty failure reason"));

    ack_leased_messages_with_failure_reason(
        &root,
        std::slice::from_ref(&task.message_key),
        "failed",
        "worker execution failed before completion review",
    )
    .expect("failed ack with reason should succeed");

    let conn = open_channel_db(&crate::paths::core_db(&root)).expect("failed to open channel db");
    let (route_status, last_error): (String, Option<String>) = conn
        .query_row(
            "SELECT route_status, last_error FROM communication_routing_state WHERE message_key = ?1",
            params![task.message_key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("failed to read routing state");
    assert_eq!(route_status, "failed");
    assert_eq!(
        last_error.as_deref(),
        Some("worker execution failed before completion review")
    );
    let reloaded = load_queue_task(&root, &task.message_key)
        .expect("failed to reload failed queue task")
        .expect("failed queue task should remain inspectable");
    assert_eq!(
        reloaded.status_note.as_deref(),
        Some("worker execution failed before completion review")
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn queue_task_prompt_update_preserves_existing_workspace_root() {
    let root = std::env::temp_dir().join(format!(
        "ctox-queue-workspace-update-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("failed to create temp test root");

    let task = create_queue_task(
        &root,
        QueueTaskCreateRequest {
            title: "workspace update".to_string(),
            prompt: "Create and verify artifacts.".to_string(),
            thread_key: "queue/workspace-update".to_string(),
            workspace_root: Some("/tmp/ctox-original-workspace".to_string()),
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )
    .expect("failed to create workspace task");

    let updated = update_queue_task(
        &root,
        QueueTaskUpdateRequest {
            message_key: task.message_key,
            prompt: Some(
                "HARNESS FEEDBACK\n\nCURRENT TASK\nWork only inside this workspace: /tmp/wrong-inline-text Execution contract: keep working.\n\nRUNTIME FAILURE\nx".to_string(),
            ),
            ..Default::default()
        },
    )
    .expect("failed to update workspace task");

    assert_eq!(
        updated.workspace_root.as_deref(),
        Some("/tmp/ctox-original-workspace")
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn communication_support_paths_use_native_gateway_runtime_merge() {
    let root = PathBuf::from("/tmp/ctox-root");
    let mut email_settings = BTreeMap::new();
    email_settings.insert(
        "CTO_EMAIL_RAW_DIR".to_string(),
        root.join("runtime/communication/email/raw")
            .display()
            .to_string(),
    );
    assert_eq!(
        crate::communication::gateway::runtime_settings_from_settings(
            &root,
            crate::communication::gateway::CommunicationAdapterKind::Email,
            &email_settings,
        )
        .get("CTO_EMAIL_RAW_DIR"),
        Some(
            &root
                .join("runtime/communication/email/raw")
                .display()
                .to_string()
        )
    );
    let mut jami_settings = BTreeMap::new();
    jami_settings.insert(
        "CTO_JAMI_INBOX_DIR".to_string(),
        root.join("runtime/communication/jami/inbox")
            .display()
            .to_string(),
    );
    assert_eq!(
        crate::communication::gateway::runtime_settings_from_settings(
            &root,
            crate::communication::gateway::CommunicationAdapterKind::Jami,
            &jami_settings,
        )
        .get("CTO_JAMI_INBOX_DIR"),
        Some(
            &root
                .join("runtime/communication/jami/inbox")
                .display()
                .to_string()
        )
    );
}

#[test]
fn resolve_outbound_subject_reuses_existing_thread_subject() {
    let db_path = unique_test_db_path("ctox-channel-subject-reuse");
    let mut conn = open_channel_db(&db_path).expect("failed to open db");
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: "msg-1",
            channel: "email",
            account_key: "email:test@example.com",
            thread_key: "email/thread-1",
            remote_id: "remote-1",
            direction: "inbound",
            folder_hint: "INBOX",
            sender_display: "Owner",
            sender_address: "owner@example.com",
            recipient_addresses_json: "[]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: "Existing subject",
            preview: "preview",
            body_text: "body",
            body_html: "",
            raw_payload_ref: "",
            trust_level: "owner_verified",
            status: "received",
            seen: false,
            has_attachments: false,
            external_created_at: "2026-03-26T10:00:00Z",
            observed_at: "2026-03-26T10:00:00Z",
            metadata_json: "{}",
        },
    )
    .expect("failed to upsert message");
    refresh_thread(&mut conn, "email/thread-1").expect("failed to refresh thread");

    let resolved = resolve_outbound_subject(
        &conn,
        ChannelSendRequest {
            channel: "email".to_string(),
            account_key: "email:test@example.com".to_string(),
            thread_key: "email/thread-1".to_string(),
            body: "reply".to_string(),
            subject: String::new(),
            to: vec!["owner@example.com".to_string()],
            cc: Vec::new(),
            attachments: Vec::new(),
            sender_display: None,
            sender_address: None,
            send_voice: false,
            reviewed_founder_send: false,
        },
    )
    .expect("failed to resolve subject");
    assert_eq!(resolved.subject, "Existing subject");

    let _ = fs::remove_file(&db_path);
}

#[test]
fn resolve_outbound_subject_rejects_missing_email_subject() {
    let db_path = unique_test_db_path("ctox-channel-subject-missing");
    let conn = open_channel_db(&db_path).expect("failed to open db");
    let error = resolve_outbound_subject(
        &conn,
        ChannelSendRequest {
            channel: "email".to_string(),
            account_key: "email:test@example.com".to_string(),
            thread_key: "email/thread-2".to_string(),
            body: "reply".to_string(),
            subject: "(no subject)".to_string(),
            to: vec!["owner@example.com".to_string()],
            cc: Vec::new(),
            attachments: Vec::new(),
            sender_display: None,
            sender_address: None,
            send_voice: false,
            reviewed_founder_send: false,
        },
    )
    .expect_err("missing email subject should fail");
    assert!(
        error
            .to_string()
            .contains("email send requires a real subject"),
        "unexpected error: {error}"
    );

    let _ = fs::remove_file(&db_path);
}

#[test]
fn founder_outbound_email_requires_review_override() {
    let mut settings = BTreeMap::new();
    settings.insert(
        "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
        "founder@example.com".to_string(),
    );

    let error = validate_founder_outbound_email(
        &settings,
        &ChannelSendRequest {
            channel: "email".to_string(),
            account_key: "email:cto1@example.com".to_string(),
            thread_key: "mail-thread".to_string(),
            body: "Short founder update.".to_string(),
            subject: "Re: Test".to_string(),
            to: vec!["founder@example.com".to_string()],
            cc: Vec::new(),
            attachments: Vec::new(),
            sender_display: None,
            sender_address: None,
            send_voice: false,
            reviewed_founder_send: false,
        },
    )
    .expect_err("founder outbound should require review override");

    assert!(
        error.to_string().contains("blocked without review"),
        "unexpected error: {error}"
    );
}

#[test]
fn generic_outbound_email_requires_communication_review() {
    let settings = BTreeMap::new();
    let error = validate_founder_outbound_email(
        &settings,
        &ChannelSendRequest {
            channel: "email".to_string(),
            account_key: "email:cto1@example.com".to_string(),
            thread_key: "mail-thread".to_string(),
            body: "Short external update.".to_string(),
            subject: "Re: Test".to_string(),
            to: vec!["customer@example.com".to_string()],
            cc: Vec::new(),
            attachments: Vec::new(),
            sender_display: None,
            sender_address: None,
            send_voice: false,
            reviewed_founder_send: false,
        },
    )
    .expect_err("all outbound email must require communication review");

    assert!(
        error
            .to_string()
            .contains("blocked without communication review"),
        "unexpected error: {error}"
    );
}

#[test]
fn parse_send_request_allows_teams_without_to_recipient() {
    let args = vec![
        "send".to_string(),
        "--channel".to_string(),
        "teams".to_string(),
        "--account-key".to_string(),
        "teams:bot".to_string(),
        "--thread-key".to_string(),
        "teams:bot::chat::chat-123".to_string(),
        "--body".to_string(),
        "kurze antwort".to_string(),
    ];

    let request = parse_send_request(&args).expect("teams send should not require --to");
    assert_eq!(request.channel, "teams");
    assert!(request.to.is_empty());
}

#[test]
fn parse_send_request_allows_bot_chat_adapters_without_to_recipient() {
    for channel in [
        "slack",
        "discord",
        "telegram",
        "matrix",
        "mattermost",
        "zulip",
        "google_chat",
    ] {
        let args = vec![
            "send".to_string(),
            "--channel".to_string(),
            channel.to_string(),
            "--account-key".to_string(),
            format!("{channel}:bot"),
            "--thread-key".to_string(),
            format!("{channel}:bot::channel::configured-default"),
            "--body".to_string(),
            "kurze antwort".to_string(),
        ];

        let request = parse_send_request(&args)
            .unwrap_or_else(|error| panic!("{channel} should not require --to: {error}"));
        assert_eq!(request.channel, channel);
        assert!(request.to.is_empty());
    }
}

#[test]
fn teams_tenant_comes_from_profile_not_account_key() {
    let account = AccountConfig {
        provider: "graph".to_string(),
        profile_json: json!({
            "tenantId": "tenant-123",
        }),
    };

    assert_eq!(
        teams_tenant_from_account_config(Some(&account)).as_deref(),
        Some("tenant-123")
    );
    assert_eq!(teams_tenant_from_account_config(None), None);
}

#[test]
fn parse_send_request_still_requires_email_to_recipient() {
    let args = vec![
        "send".to_string(),
        "--channel".to_string(),
        "email".to_string(),
        "--account-key".to_string(),
        "email:cto@example.com".to_string(),
        "--thread-key".to_string(),
        "email-thread".to_string(),
        "--body".to_string(),
        "kurze antwort".to_string(),
    ];

    let error = parse_send_request(&args).expect_err("email send should require --to");
    assert!(
        error
            .to_string()
            .contains("channel send for email requires at least one --to value"),
        "unexpected error: {error}"
    );
}

#[test]
fn founder_outbound_email_does_not_string_scrape_body_content() {
    // Core no longer scrapes outbound bodies for "internal vocabulary"
    // substrings. That guidance lives in `owner-communication/SKILL.md`.
    // Whatever the body contains, the generic `channel send` path is
    // still blocked for founder/owner/admin recipients — the operator
    // must use the reviewed founder-outbound pipeline.
    let mut settings = BTreeMap::new();
    settings.insert(
        "CTOX_FOUNDER_EMAIL_ADDRESSES".to_string(),
        "founder@example.com".to_string(),
    );

    let error = validate_founder_outbound_email(
        &settings,
        &ChannelSendRequest {
            channel: "email".to_string(),
            account_key: "email:cto1@example.com".to_string(),
            thread_key: "mail-thread".to_string(),
            body: "Die Dateien liegen unter /home/ubuntu/workspace/example/public/mockups/."
                .to_string(),
            subject: "Re: Test".to_string(),
            to: vec!["founder@example.com".to_string()],
            cc: Vec::new(),
            attachments: Vec::new(),
            sender_display: None,
            sender_address: None,
            send_voice: false,
            reviewed_founder_send: true,
        },
    )
    .expect_err("generic founder send should still be blocked irrespective of body content");

    let message = error.to_string();
    assert!(
        message.contains("generic channel send is disabled"),
        "unexpected error: {message}"
    );
    assert!(
        !message.contains("internal-language leakage"),
        "core must not string-scrape outbound bodies anymore: {message}"
    );
}

#[test]
fn founder_outbound_body_rejects_address_headers_in_body() {
    let error = ensure_founder_outbound_body_clean(&ChannelSendRequest {
        channel: "email".to_string(),
        account_key: "email:cto1@example.com".to_string(),
        thread_key: "mail-thread".to_string(),
        body: "An: founder@example.com\nCc: owner@example.com\nBetreff: Re: Test\n\nHallo zusammen,\n\nsauberer Text.".to_string(),
        subject: "Re: Test".to_string(),
        to: vec!["founder@example.com".to_string()],
        cc: vec!["owner@example.com".to_string()],
        attachments: Vec::new(),
        sender_display: None,
        sender_address: None,
        send_voice: false,
        reviewed_founder_send: true,
    })
    .expect_err("body header preamble should be blocked");

    assert!(
        error
            .to_string()
            .contains("headers were placed in the message body"),
        "unexpected error: {error}"
    );
}

#[test]
fn founder_outbound_body_rejects_internal_send_status_report() {
    let error = ensure_founder_outbound_body_clean(&ChannelSendRequest {
        channel: "email".to_string(),
        account_key: "email:cto1@example.com".to_string(),
        thread_key: "mail-thread".to_string(),
        body: "Die Founder-Mail ist als Reply raus. Review-Approval, Send-Proof, Outbound-Message-Row und Routing-State sind persistiert; Michaels Inbound `email:cto1@example.com::INBOX::105` steht jetzt auf `handled`."
            .to_string(),
        subject: "Example CRM: ehrlicher Zwischenstand".to_string(),
        to: vec!["founder@example.com".to_string()],
        cc: Vec::new(),
        attachments: Vec::new(),
        sender_display: None,
        sender_address: None,
        send_voice: false,
        reviewed_founder_send: true,
    })
    .expect_err("internal send status reports must never reach founders");

    let message = error.to_string();
    assert!(
        message.contains("internal-language leakage"),
        "unexpected error: {message}"
    );
    assert!(
        message.contains("review-approval") || message.contains("routing-state"),
        "unexpected markers: {message}"
    );
}

#[test]
fn founder_outbound_email_still_blocks_generic_send_after_review_override() {
    let mut settings = BTreeMap::new();
    settings.insert(
        "CTOX_FOUNDER_EMAIL_ADDRESSES".to_string(),
        "founder@example.com".to_string(),
    );

    let error = validate_founder_outbound_email(
        &settings,
        &ChannelSendRequest {
            channel: "email".to_string(),
            account_key: "email:cto1@example.com".to_string(),
            thread_key: "mail-thread".to_string(),
            body: "Kurzes sauberes Update ohne internen Systemmuell.".to_string(),
            subject: "Re: Test".to_string(),
            to: vec!["founder@example.com".to_string()],
            cc: Vec::new(),
            attachments: Vec::new(),
            sender_display: None,
            sender_address: None,
            send_voice: false,
            reviewed_founder_send: true,
        },
    )
    .expect_err("generic founder send should still be blocked");

    assert!(
        error
            .to_string()
            .contains("generic channel send is disabled"),
        "unexpected error: {error}"
    );
}

#[test]
fn reviewed_founder_reply_for_forward_targets_original_recipient_and_ccs_sender() {
    let db_path = unique_test_db_path("ctox-founder-forward-reply");
    let mut conn = open_channel_db(&db_path).expect("failed to open db");
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: "email:cto1@example.com::INBOX::forward-1",
            channel: "email",
            account_key: "email:cto1@example.com",
            thread_key: "<forward-thread@example.com>",
            remote_id: "remote-forward-1",
            direction: "inbound",
            folder_hint: "INBOX",
            sender_display: "Max Mustermann",
            sender_address: "founder@example.com",
            recipient_addresses_json: "[\"s.mueller@example.com\"]",
            cc_addresses_json: "[\"cto1@example.com\"]",
            bcc_addresses_json: "[]",
            subject: "Fwd: Visuelle Homepage",
            preview: "Hi Olaf",
            body_text: "Hi Olaf,\n\nAnfang der weitergeleiteten Nachricht:\n...",
            body_html: "",
            raw_payload_ref: "",
            trust_level: "trusted",
            status: "received",
            seen: false,
            has_attachments: false,
            external_created_at: "2026-04-24T12:04:04Z",
            observed_at: "2026-04-24T12:04:05Z",
            metadata_json: "{}",
        },
    )
    .expect("message upsert");

    let inbound = load_message_from_conn(&conn, "email:cto1@example.com::INBOX::forward-1")
        .expect("load inbound")
        .expect("inbound missing");
    let addressing =
        load_message_addressing_from_conn(&conn, "email:cto1@example.com::INBOX::forward-1")
            .expect("load addressing")
            .expect("addressing missing");

    let (to, cc) = derive_founder_reply_recipients(&inbound, &addressing);
    assert_eq!(to, vec!["s.mueller@example.com".to_string()]);
    assert_eq!(cc, vec!["founder@example.com".to_string()]);

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn founder_reply_detects_qr_code_as_required_deliverable() {
    let required = detect_required_founder_deliverables(
        "Jami zugang schicken.",
        "Schick mir bitte den Jami QR code Zugang für den Chat mir dir.",
    );
    assert_eq!(required, vec!["qr_code".to_string()]);
}

#[test]
fn founder_reply_blocks_send_when_qr_code_is_missing() {
    let root = std::env::temp_dir().join(format!(
        "ctox-founder-qr-deliverable-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
    let db_path = crate::paths::core_db(&root);
    let mut conn = open_channel_db(&db_path).expect("failed to open db");
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: "email:cto1@example.com::INBOX::qr-1",
            channel: "email",
            account_key: "email:cto1@example.com",
            thread_key: "<qr-thread@example.com>",
            remote_id: "remote-qr-1",
            direction: "inbound",
            folder_hint: "INBOX",
            sender_display: "Max Mustermann",
            sender_address: "founder@example.com",
            recipient_addresses_json: "[\"cto1@example.com\"]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: "Jami zugang schicken.",
            preview: "QR code needed",
            body_text: "Schick mir bitte den Jami QR code Zugang für den Chat mir dir.",
            body_html: "",
            raw_payload_ref: "",
            trust_level: "trusted",
            status: "received",
            seen: false,
            has_attachments: false,
            external_created_at: "2026-04-24T14:28:56Z",
            observed_at: "2026-04-24T14:28:56Z",
            metadata_json: "{}",
        },
    )
    .expect("message upsert");

    let error = ensure_founder_reply_deliverables_present(
        &root,
        "email:cto1@example.com::INBOX::qr-1",
        "Hi Michael,\n\nhier ist der direkte Jami-Zugang:\n\njami:abc123",
        &[],
    )
    .expect_err("missing qr code should block founder reply");
    assert!(error
        .to_string()
        .contains("missing required deliverable(s): qr_code"));

    let _ = std::fs::remove_file(&db_path);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn teams_work_ack_is_blocked_without_pipeline_backing() {
    let root = std::env::temp_dir().join(format!(
        "ctox-teams-ack-guard-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
    let db_path = crate::paths::core_db(&root);
    let conn = open_channel_db(&db_path).expect("failed to open channel db");
    let request = ChannelSendRequest {
        channel: "teams".to_string(),
        account_key: "teams:inf.yoda@example.test".to_string(),
        thread_key: "teams:inf.yoda@example.test::chat::jill".to_string(),
        body: "Danke für den Hinweis — verstanden. Ich scrolle die Seite vollständig durch und übertrage die Aussteller aus Deutschland in eine Excel.".to_string(),
        subject: "(Teams)".to_string(),
        to: Vec::new(),
        cc: Vec::new(),
        attachments: Vec::new(),
        sender_display: None,
        sender_address: None,
        send_voice: false,
        reviewed_founder_send: false,
    };

    let err = enforce_external_work_ack_has_pipeline_backing(&conn, &request)
        .expect_err("work acknowledgement must require durable backing");
    assert!(err.to_string().contains("promises follow-up work"));

    let _ = std::fs::remove_file(&db_path);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn create_queue_task_is_idempotent_under_same_idempotency_key() {
    let root = std::env::temp_dir().join(format!(
        "ctox-queue-idem-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("failed to create temp root");
    let thread_key = "queue:idem::thread";
    let make = |extra: Option<Value>| QueueTaskCreateRequest {
        title: "Structure the latest export".to_string(),
        prompt: "Structure the uploaded spreadsheet.".to_string(),
        thread_key: thread_key.to_string(),
        workspace_root: None,
        priority: "normal".to_string(),
        suggested_skill: None,
        parent_message_key: None,
        extra_metadata: extra,
    };

    let first = create_queue_task(&root, make(Some(json!({"idempotency_key": "retry-abc"}))))
        .expect("first create");
    let second = create_queue_task(&root, make(Some(json!({"idempotency_key": "retry-abc"}))))
        .expect("second create (retry)");
    assert_eq!(
        first.message_key, second.message_key,
        "same idempotency key must yield the same message_key"
    );

    let conn = open_channel_db(&crate::paths::core_db(&root)).expect("failed to reopen db");
    let rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM communication_messages WHERE message_key = ?1",
            [first.message_key.as_str()],
            |row| row.get(0),
        )
        .expect("failed to count messages");
    assert_eq!(
        rows, 1,
        "an idempotent retry must not duplicate the queue row"
    );

    let budget_key = format!("queue-task:thread:{thread_key}");
    let edges: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ctox_core_spawn_edges WHERE budget_key = ?1 AND accepted = 1",
            [budget_key.as_str()],
            |row| row.get(0),
        )
        .expect("failed to count spawn edges");
    assert_eq!(
        edges, 1,
        "an idempotent retry must spend the spawn budget only once"
    );

    let conversation_id =
        crate::execution::agent::turn_loop::conversation_id_for_thread_key(Some(thread_key));
    let seeded = conn
        .query_row(
            "SELECT mission, next_slice, is_open FROM mission_states WHERE conversation_id = ?1",
            [conversation_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, bool>(2)?,
                ))
            },
        )
        .expect("failed to load queue mission seed");
    assert_eq!(seeded.0, "Structure the latest export");
    assert_eq!(seeded.1, "Structure the latest export");
    assert!(seeded.2);
    conn.execute(
        "UPDATE mission_states SET mission = ?2, next_slice = ?3 WHERE conversation_id = ?1",
        rusqlite::params![
            conversation_id,
            "Model-authored mission",
            "Model-authored next slice"
        ],
    )
    .expect("failed to install model-authored mission fixture");
    drop(conn);

    let third = create_queue_task(&root, make(None)).expect("third create without key");
    assert_ne!(
        third.message_key, first.message_key,
        "a keyless create must still get a distinct now-salted message_key"
    );
    let conn = open_channel_db(&crate::paths::core_db(&root)).expect("failed to reopen db");
    let preserved = conn
        .query_row(
            "SELECT mission, next_slice FROM mission_states WHERE conversation_id = ?1",
            [conversation_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .expect("failed to load preserved model mission");
    assert_eq!(preserved.0, "Model-authored mission");
    assert_eq!(preserved.1, "Model-authored next slice");

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn teams_work_ack_requires_review_even_with_queue_backing() {
    let root = std::env::temp_dir().join(format!(
        "ctox-teams-ack-backed-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("failed to create temp root");
    let thread_key = "teams:inf.yoda@example.test::chat::jill";
    create_queue_task(
        &root,
        QueueTaskCreateRequest {
            title: "Intersolar-Aussteller Deutschland in Excel".to_string(),
            prompt: "Scrape Intersolar and create the verified Excel artifact.".to_string(),
            thread_key: thread_key.to_string(),
            workspace_root: None,
            priority: "high".to_string(),
            suggested_skill: Some("universal-scraping".to_string()),
            parent_message_key: None,
            extra_metadata: None,
        },
    )
    .expect("failed to create queue backing");
    let conn = open_channel_db(&crate::paths::core_db(&root)).expect("failed to reopen db");
    let request = ChannelSendRequest {
        channel: "teams".to_string(),
        account_key: "teams:inf.yoda@example.test".to_string(),
        thread_key: thread_key.to_string(),
        body: "Danke, ich prüfe das und erstelle die Excel.".to_string(),
        subject: "(Teams)".to_string(),
        to: Vec::new(),
        cc: Vec::new(),
        attachments: Vec::new(),
        sender_display: None,
        sender_address: None,
        send_voice: false,
        reviewed_founder_send: false,
    };

    let err = enforce_external_work_ack_has_pipeline_backing(&conn, &request)
        .expect_err("queue-backed acknowledgement must still require review");
    assert!(err
        .to_string()
        .contains("has not passed communication review"));

    let reviewed_request = ChannelSendRequest {
        reviewed_founder_send: true,
        ..request
    };
    enforce_external_work_ack_has_pipeline_backing(&conn, &reviewed_request)
        .expect("reviewed queue-backed acknowledgement should be allowed");

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn teams_send_requires_external_chat_review_even_without_work_promise() {
    let request = ChannelSendRequest {
        channel: "teams".to_string(),
        account_key: "teams:inf.yoda@example.test".to_string(),
        thread_key: "teams:inf.yoda@example.test::chat::jill".to_string(),
        body: "Verstanden, ich habe die Rueckfrage gesehen.".to_string(),
        subject: "(Teams)".to_string(),
        to: Vec::new(),
        cc: Vec::new(),
        attachments: Vec::new(),
        sender_display: None,
        sender_address: None,
        send_voice: false,
        reviewed_founder_send: false,
    };

    let err = enforce_external_chat_send_is_reviewed(&request)
        .expect_err("external chat sends must pass review even without work promise");
    assert!(err.to_string().contains("must pass communication review"));

    let reviewed_request = ChannelSendRequest {
        reviewed_founder_send: true,
        ..request
    };
    enforce_external_chat_send_is_reviewed(&reviewed_request)
        .expect("reviewed external chat sends should pass this guard");
}

#[test]
fn email_communication_review_approval_is_exact_and_typed() {
    let root = std::env::temp_dir().join(format!(
        "ctox-email-communication-review-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
    let db_path = crate::paths::core_db(&root);
    let conn = open_channel_db(&db_path).expect("failed to open db");
    let action = ExternalChatAction {
        channel: "email".to_string(),
        account_key: "email:cto1@example.com".to_string(),
        thread_key: "email-thread".to_string(),
        subject: "Re: Customer".to_string(),
        to: vec!["customer@example.com".to_string()],
        cc: vec!["ops@example.com".to_string()],
        attachments: vec!["/tmp/result.pdf".to_string()],
    };
    let body = "Hello,\n\nThe requested work is complete; attached is the result.";

    record_external_chat_review_approval(
        &root,
        "email:cto1@example.com::INBOX::customer-1",
        &action,
        body,
        "PASS",
    )
    .expect("failed to record communication review");
    let action_json: String = conn
        .query_row(
            "SELECT action_json FROM communication_founder_reply_reviews LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("failed to load action json");
    assert!(action_json.contains("reviewed_outbound_email"));
    assert!(!action_json.contains("external_chat_quick_response"));

    require_any_unconsumed_external_chat_review(&conn, &action, body)
        .expect("exact approved email body should match");
    let changed_body = require_any_unconsumed_external_chat_review(&conn, &action, "Changed body")
        .expect_err("changed email body must not inherit approval");
    assert!(changed_body
        .to_string()
        .contains("no matching unconsumed review approval"));

    let _ = std::fs::remove_file(&db_path);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn reviewed_external_chat_send_writes_core_transition_proof() {
    let root = std::env::temp_dir().join(format!(
        "ctox-reviewed-chat-core-proof-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
    let db_path = crate::paths::core_db(&root);
    let conn = open_channel_db(&db_path).expect("failed to open db");
    let request = ChannelSendRequest {
        channel: "teams".to_string(),
        account_key: "teams:bot".to_string(),
        thread_key: "teams:chat-1".to_string(),
        body: "Ich habe die Aufgabe angelegt und bearbeite sie als naechstes.".to_string(),
        subject: "(Teams)".to_string(),
        to: Vec::new(),
        cc: Vec::new(),
        attachments: Vec::new(),
        sender_display: None,
        sender_address: None,
        send_voice: false,
        reviewed_founder_send: true,
    };
    let action = external_chat_action_from_send_request(&request);
    record_external_chat_review_approval(
        &root,
        "teams:bot::chat::msg-1",
        &action,
        &request.body,
        "PASS",
    )
    .expect("failed to record chat review");
    let approval = require_any_unconsumed_external_chat_review(&conn, &action, &request.body)
        .expect("exact approved chat body should match");
    let entity_id = enforce_reviewed_communication_send_core_transition_if_approved(
        &conn,
        &request,
        Some(&approval),
    )
    .expect("reviewed chat send should write a core proof")
    .expect("reviewed chat approval should produce an entity id");

    let accepted: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ctox_core_transition_proofs
             WHERE entity_type = 'FounderCommunication'
               AND entity_id = ?1
               AND from_state = 'Approved'
               AND to_state = 'Sending'
               AND accepted = 1",
            params![entity_id],
            |row| row.get(0),
        )
        .expect("failed to count proofs");
    assert_eq!(accepted, 1);

    let _ = std::fs::remove_file(&db_path);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn fake_bot_chat_send_requires_exact_review_and_marks_audit_sent() {
    let root = unique_root("ctox-fake-chat-reviewed-send");
    fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
    let mut runtime_settings = BTreeMap::new();
    runtime_settings.insert(
        "CTO_SLACK_API_BASE_URL".to_string(),
        "ctox-fake://slack".to_string(),
    );
    runtime_settings.insert("CTO_SLACK_BOT_TOKEN".to_string(), "ctox-fake".to_string());
    runtime_settings.insert("CTO_SLACK_WORKSPACE_ID".to_string(), "TFAKE".to_string());
    runtime_settings.insert("CTO_SLACK_BOT_USER_ID".to_string(), "UFAKE".to_string());
    runtime_settings.insert("CTO_SLACK_CHANNEL_ID".to_string(), "CFAKE".to_string());
    crate::inference::runtime_env::save_runtime_env_map(&root, &runtime_settings)
        .expect("failed to persist fake Slack runtime settings");
    let db_path = resolve_db_path(&root, None);
    let body = "OK.";
    let action = ExternalChatAction {
        channel: "slack".to_string(),
        account_key: "slack:UFAKE".to_string(),
        thread_key: "slack:UFAKE::channel::CFAKE::thread::1719360000.000001".to_string(),
        subject: "(Slack)".to_string(),
        to: Vec::new(),
        cc: Vec::new(),
        attachments: Vec::new(),
    };

    let direct = send_message(
        &root,
        &db_path,
        ChannelSendRequest {
            channel: action.channel.clone(),
            account_key: action.account_key.clone(),
            thread_key: action.thread_key.clone(),
            body: body.to_string(),
            subject: action.subject.clone(),
            to: action.to.clone(),
            cc: action.cc.clone(),
            attachments: action.attachments.clone(),
            sender_display: None,
            sender_address: None,
            send_voice: false,
            reviewed_founder_send: false,
        },
    )
    .expect_err("direct fake Slack send must be blocked before review");
    assert!(
        direct
            .to_string()
            .contains("must pass communication review"),
        "unexpected error: {direct}"
    );

    let missing_review = send_reviewed_external_chat_action(&root, &action, body)
        .expect_err("reviewed fake Slack send must still require an approval row");
    assert!(
        missing_review
            .to_string()
            .contains("no matching unconsumed review approval"),
        "unexpected error: {missing_review}"
    );

    record_external_chat_review_approval(
        &root,
        "slack:UFAKE::channel::CFAKE::inbound-review-anchor",
        &action,
        body,
        "PASS: fake Slack send approved",
    )
    .expect("failed to record chat review approval");
    let send_result = send_reviewed_external_chat_action(&root, &action, body)
        .expect("approved fake Slack send should succeed");
    assert_eq!(send_result.get("ok").and_then(Value::as_bool), Some(true));
    assert_eq!(
        send_result.get("status").and_then(Value::as_str),
        Some("sent")
    );

    let conn = open_channel_db(&db_path).expect("failed to reopen db");
    let (sent_reviews, sent_result_status): (i64, String) = conn
        .query_row(
            r#"
            SELECT COUNT(*), COALESCE(MAX(json_extract(send_result_json, '$.status')), '')
            FROM communication_founder_reply_reviews
            WHERE sent_at IS NOT NULL
              AND json_extract(action_json, '$.channel') = 'slack'
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("failed to query sent review");
    assert_eq!(sent_reviews, 1);
    assert_eq!(sent_result_status, "sent");
    let outbound_rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM communication_messages WHERE channel = 'slack' AND direction = 'outbound' AND status = 'sent'",
            [],
            |row| row.get(0),
        )
        .expect("failed to count outbound fake Slack messages");
    assert_eq!(outbound_rows, 1);
    let (remote_id, provider_ts): (String, String) = conn
        .query_row(
            r#"
            SELECT remote_id,
                   COALESCE(json_extract(metadata_json, '$.providerResponse.ts'), '')
            FROM communication_messages
            WHERE channel = 'slack'
              AND direction = 'outbound'
              AND status = 'sent'
            LIMIT 1
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("failed to inspect outbound fake Slack evidence");
    // The audit row must carry the id the provider actually returned. The fake
    // provider mints a fresh outbound id per send (identical messages must not
    // collapse onto one row in the outbox), so pin the relationship rather than
    // the literal — the value is the fake adapter's business, the agreement is
    // the evidence.
    assert!(
        !remote_id.trim().is_empty(),
        "outbound evidence must record a provider id"
    );
    assert_eq!(
        remote_id, provider_ts,
        "recorded remote id must be the id the provider reported"
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn teams_reviewed_send_allows_attachments_for_adapter_delivery() {
    let request = ChannelSendRequest {
        channel: "teams".to_string(),
        account_key: "teams:inf.yoda@example.test".to_string(),
        thread_key: "teams:inf.yoda@example.test::chat::jill".to_string(),
        body: "Hier ist die Excel.".to_string(),
        subject: "(Teams)".to_string(),
        to: Vec::new(),
        cc: Vec::new(),
        attachments: vec!["/tmp/result.xlsx".to_string()],
        sender_display: None,
        sender_address: None,
        send_voice: false,
        reviewed_founder_send: true,
    };

    enforce_external_chat_send_is_reviewed(&request)
        .expect("reviewed Teams attachment send should pass review guard");
    enforce_channel_attachment_support(&request)
        .expect("Teams attachments are handed to the adapter for Graph delivery");
}

#[test]
fn bot_chat_adapter_rejects_attachments_until_provider_review_exists() {
    let request = ChannelSendRequest {
        channel: "slack".to_string(),
        account_key: "slack:bot".to_string(),
        thread_key: "slack:bot::channel::C123::thread::1719360000.000001".to_string(),
        body: "Hier ist die Datei.".to_string(),
        subject: "(Slack)".to_string(),
        to: Vec::new(),
        cc: Vec::new(),
        attachments: vec!["/tmp/result.pdf".to_string()],
        sender_display: None,
        sender_address: None,
        send_voice: false,
        reviewed_founder_send: true,
    };

    let error = enforce_channel_attachment_support(&request)
        .expect_err("Slack attachment send must stay blocked in text-only v1");
    assert!(
        error
            .to_string()
            .contains("attachments are not supported by the native chat adapter v1"),
        "unexpected error: {error}"
    );
}

#[test]
fn reviewed_founder_reply_requires_exact_approval_before_send() {
    let root = std::env::temp_dir().join(format!(
        "ctox-founder-exact-review-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
    let db_path = crate::paths::core_db(&root);
    let mut conn = open_channel_db(&db_path).expect("failed to open db");
    let inbound_key = "email:cto1@example.com::INBOX::exact-review-1";
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: inbound_key,
            channel: "email",
            account_key: "email:cto1@example.com",
            thread_key: "<exact-review-thread@example.com>",
            remote_id: "remote-exact-review-1",
            direction: "inbound",
            folder_hint: "INBOX",
            sender_display: "Max Mustermann",
            sender_address: "founder@example.com",
            recipient_addresses_json: "[\"cto1@example.com\"]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: "Status",
            preview: "Status bitte",
            body_text: "Bitte antworte mit dem aktuellen Stand.",
            body_html: "",
            raw_payload_ref: "",
            trust_level: "trusted",
            status: "received",
            seen: false,
            has_attachments: false,
            external_created_at: "2026-04-24T18:00:00Z",
            observed_at: "2026-04-24T18:00:00Z",
            metadata_json: "{}",
        },
    )
    .expect("message upsert");

    let action = prepare_reviewed_founder_reply(&root, inbound_key).expect("prepare reply");
    let approved_body = "Hi Michael,\n\nDer Status ist jetzt konkret.";
    let before_approval =
        require_unconsumed_founder_reply_review(&conn, inbound_key, &action, approved_body)
            .expect_err("send must be blocked before review approval");
    assert!(before_approval
        .to_string()
        .contains("no matching unconsumed review approval"));

    record_founder_reply_review_approval(&root, inbound_key, approved_body, "PASS")
        .expect("record approval");
    let conn = open_channel_db(&db_path).expect("reopen db");
    require_unconsumed_founder_reply_review(&conn, inbound_key, &action, approved_body)
        .expect("exact reviewed body should be approved");
    let changed_body =
        require_unconsumed_founder_reply_review(&conn, inbound_key, &action, "Changed body")
            .expect_err("changed body must not inherit the approval");
    assert!(changed_body
        .to_string()
        .contains("no matching unconsumed review approval"));

    let _ = std::fs::remove_file(&db_path);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn reviewed_founder_send_writes_core_transition_proof() {
    let db_path = unique_test_db_path("ctox-founder-core-proof");
    let conn = open_channel_db(&db_path).expect("failed to open db");
    let request = ChannelSendRequest {
        channel: "email".to_string(),
        account_key: "email:cto1@example.com".to_string(),
        thread_key: "mail-thread".to_string(),
        body: "Hi Michael,\n\nDer Status ist belegt.".to_string(),
        subject: "Re: Status".to_string(),
        to: vec!["founder@example.com".to_string()],
        cc: vec!["s.mueller@example.com".to_string()],
        attachments: Vec::new(),
        sender_display: None,
        sender_address: None,
        send_voice: false,
        reviewed_founder_send: true,
    };

    enforce_reviewed_founder_send_core_transition(
        &conn,
        "founder-reply:email:cto1@example.com::INBOX::proof",
        "founder-review:proof",
        &request,
    )
    .expect("reviewed founder send should write an accepted proof");

    let accepted: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ctox_core_transition_proofs
             WHERE entity_type = 'FounderCommunication'
               AND lane = 'P0FounderCommunication'
               AND from_state = 'Approved'
               AND to_state = 'Sending'
               AND accepted = 1",
            [],
            |row| row.get(0),
        )
        .expect("failed to count proofs");
    assert_eq!(accepted, 1);

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn reviewed_founder_send_success_reaches_terminal_sent() {
    let db_path = unique_test_db_path("ctox-founder-core-sent");
    let conn = open_channel_db(&db_path).expect("failed to open db");
    let request = ChannelSendRequest {
        channel: "email".to_string(),
        account_key: "email:cto1@example.com".to_string(),
        thread_key: "mail-thread".to_string(),
        body: "Hi Michael,\n\nDer Status ist belegt.".to_string(),
        subject: "Re: Status".to_string(),
        to: vec!["founder@example.com".to_string()],
        cc: vec!["s.mueller@example.com".to_string()],
        attachments: Vec::new(),
        sender_display: None,
        sender_address: None,
        send_voice: false,
        reviewed_founder_send: true,
    };
    let entity_id = "founder-reply:email:cto1@example.com::INBOX::sent";
    let approval_key = "founder-review:sent";

    // Drive Approved -> Sending exactly as the production review path does.
    enforce_reviewed_founder_send_core_transition(&conn, entity_id, approval_key, &request)
        .expect("Approved -> Sending should be accepted");

    // The success twin must now witness Sending -> Sent so the entity is no
    // longer stranded in non-terminal Sending.
    emit_reviewed_founder_send_succeeded_transition(
        &conn,
        entity_id,
        approval_key,
        &request,
        "pending-sent-1",
    )
    .expect("Sending -> Sent should be accepted");

    let accepted: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ctox_core_transition_proofs
             WHERE entity_type = 'FounderCommunication'
               AND lane = 'P0FounderCommunication'
               AND from_state = 'Sending'
               AND to_state = 'Sent'
               AND accepted = 1",
            [],
            |row| row.get(0),
        )
        .expect("failed to count proofs");
    assert_eq!(
        accepted, 1,
        "a successful reviewed founder send must reach terminal Sent"
    );

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn founder_inbound_cannot_be_handled_without_reviewed_send() {
    let root = std::env::temp_dir().join(format!(
        "ctox-founder-handled-guard-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
    let mut runtime_settings = BTreeMap::new();
    runtime_settings.insert(
        "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
        "founder@example.com".to_string(),
    );
    crate::inference::runtime_env::save_runtime_env_map(&root, &runtime_settings)
        .expect("failed to persist owner setting");
    let db_path = crate::paths::core_db(&root);
    let mut conn = open_channel_db(&db_path).expect("failed to open db");
    let inbound_key = "email:cto1@example.com::INBOX::handled-guard-1";
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: inbound_key,
            channel: "email",
            account_key: "email:cto1@example.com",
            thread_key: "<handled-guard-thread@example.com>",
            remote_id: "remote-handled-guard-1",
            direction: "inbound",
            folder_hint: "INBOX",
            sender_display: "Max Mustermann",
            sender_address: "founder@example.com",
            recipient_addresses_json: "[\"cto1@example.com\"]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: "Bitte antworten",
            preview: "Bitte antworten",
            body_text: "Bitte antworte.",
            body_html: "",
            raw_payload_ref: "",
            trust_level: "trusted",
            status: "received",
            seen: false,
            has_attachments: false,
            external_created_at: "2026-04-24T18:05:00Z",
            observed_at: "2026-04-24T18:05:00Z",
            metadata_json: "{}",
        },
    )
    .expect("message upsert");

    let err = ack_leased_messages(&root, &[inbound_key.to_string()], "handled")
        .expect_err("founder inbound should not be handleable before reviewed send");
    assert!(err
        .to_string()
        .contains("cannot mark founder/owner/admin inbound mail as handled"));

    let _ = std::fs::remove_file(&db_path);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn take_messages_allows_pending_rows_with_stale_lease_owner() {
    let db_path = unique_test_db_path("ctox-channel-take-pending-stale-owner");
    let mut conn = open_channel_db(&db_path).expect("failed to open db");
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: "pending-stale-owner-1",
            channel: "email",
            account_key: "email:cto1@example.com",
            thread_key: "<pending-stale-owner@example.com>",
            remote_id: "remote-pending-stale-owner-1",
            direction: "inbound",
            folder_hint: "INBOX",
            sender_display: "Max Mustermann",
            sender_address: "founder@example.com",
            recipient_addresses_json: "[\"cto1@example.com\"]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: "Re: Visuelle Homepage",
            preview: "latest founder reply",
            body_text: "Please answer the latest founder feedback.",
            body_html: "",
            raw_payload_ref: "",
            trust_level: "trusted",
            status: "received",
            seen: false,
            has_attachments: false,
            external_created_at: "2026-04-24T18:41:06Z",
            observed_at: "2026-04-24T18:41:06Z",
            metadata_json: "{}",
        },
    )
    .expect("message upsert");
    ensure_routing_rows_for_inbound(&conn).expect("routing rows");
    conn.execute(
        "UPDATE communication_routing_state SET route_status='pending', lease_owner='ctox', leased_at='2026-04-24T18:41:07Z' WHERE message_key=?1",
        params!["pending-stale-owner-1"],
    )
    .expect("failed to seed stale lease owner");

    let taken = take_messages(&mut conn, Some("email"), 10, "ctox-service")
        .expect("take messages should succeed");
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].message_key, "pending-stale-owner-1");

    let _ = fs::remove_file(&db_path);
}

#[test]
fn take_messages_cas_rejects_concurrent_lease_steal() {
    let db_path = unique_test_db_path("ctox-channel-lease-cas-steal");
    let mut conn_a = open_channel_db(&db_path).expect("failed to open db a");
    upsert_communication_message(
        &mut conn_a,
        UpsertMessage {
            message_key: "lease-cas-1",
            channel: "email",
            account_key: "email:cto1@example.com",
            thread_key: "<lease-cas@example.com>",
            remote_id: "remote-lease-cas-1",
            direction: "inbound",
            folder_hint: "INBOX",
            sender_display: "Max Mustermann",
            sender_address: "founder@example.com",
            recipient_addresses_json: "[\"cto1@example.com\"]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: "Re: Lease race",
            preview: "race",
            body_text: "race body",
            body_html: "",
            raw_payload_ref: "",
            trust_level: "trusted",
            status: "received",
            seen: false,
            has_attachments: false,
            external_created_at: "2026-04-24T18:41:06Z",
            observed_at: "2026-04-24T18:41:06Z",
            metadata_json: "{}",
        },
    )
    .expect("message upsert");
    ensure_routing_rows_for_inbound(&conn_a).expect("routing rows");
    conn_a
        .execute(
            "UPDATE communication_routing_state SET route_status='pending', lease_owner=NULL, leased_at=NULL WHERE message_key=?1",
            params!["lease-cas-1"],
        )
        .expect("failed to seed pending row");

    // Owner A leases the pending row through the public path.
    let a = take_messages(&mut conn_a, Some("email"), 10, "owner-A").expect("A lease");
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].message_key, "lease-cas-1");

    // Owner B observed the row while it was still pending and now issues the
    // same guarded CAS UPDATE on a second connection. The CAS WHERE must
    // reject it (0 rows) because A already owns the lease.
    let mut conn_b = open_channel_db(&db_path).expect("failed to open db b");
    let now = now_iso_string();
    let updated = conn_b
        .execute(
            r#"
            INSERT INTO communication_routing_state (
                message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
            )
            VALUES (?1, 'leased', ?2, ?3, NULL, NULL, ?3)
            ON CONFLICT(message_key) DO UPDATE SET
                route_status='leased',
                lease_owner=excluded.lease_owner,
                leased_at=excluded.leased_at,
                acked_at=NULL,
                updated_at=excluded.updated_at
            WHERE communication_routing_state.lease_owner IS NULL
               OR communication_routing_state.lease_owner = ''
               OR communication_routing_state.lease_owner = ?2
               OR communication_routing_state.route_status = 'pending'
            "#,
            params!["lease-cas-1", "owner-B", now],
        )
        .expect("B cas update");
    assert_eq!(
        updated, 0,
        "the CAS guard must reject a steal of an already-leased row"
    );

    // The lease still belongs to A.
    let owner: Option<String> = conn_b
        .query_row(
            "SELECT lease_owner FROM communication_routing_state WHERE message_key=?1",
            params!["lease-cas-1"],
            |row| row.get(0),
        )
        .expect("read owner");
    assert_eq!(owner.as_deref(), Some("owner-A"));

    // And B cannot lease it through the public path either.
    let b = take_messages(&mut conn_b, Some("email"), 10, "owner-B").expect("B lease attempt");
    assert_eq!(b.len(), 0, "B must not be able to lease A's row");

    let _ = fs::remove_file(&db_path);
}

#[test]
fn releasing_leased_message_to_pending_does_not_ack_it() {
    let root = std::env::temp_dir().join(format!(
        "ctox-channel-pending-release-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
    let db_path = crate::paths::core_db(&root);
    let mut conn = open_channel_db(&db_path).expect("failed to open db");
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: "pending-release-1",
            channel: "email",
            account_key: "email:cto1@example.com",
            thread_key: "<pending-release@example.com>",
            remote_id: "remote-pending-release-1",
            direction: "inbound",
            folder_hint: "INBOX",
            sender_display: "Customer",
            sender_address: "customer@example.com",
            recipient_addresses_json: "[\"cto1@example.com\"]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: "Needs rework",
            preview: "needs rework",
            body_text: "Please rework before replying.",
            body_html: "",
            raw_payload_ref: "",
            trust_level: "trusted",
            status: "received",
            seen: false,
            has_attachments: false,
            external_created_at: "2026-04-24T18:42:06Z",
            observed_at: "2026-04-24T18:42:06Z",
            metadata_json: "{}",
        },
    )
    .expect("message upsert");
    ensure_routing_rows_for_inbound(&conn).expect("routing rows");
    let taken = take_messages(&mut conn, Some("email"), 10, "ctox-service")
        .expect("take messages should succeed");
    assert_eq!(taken.len(), 1);

    ack_leased_messages(&root, &["pending-release-1".to_string()], "pending")
        .expect("pending release should succeed");
    let (route_status, acked_at): (String, Option<String>) = conn
        .query_row(
            "SELECT route_status, acked_at FROM communication_routing_state WHERE message_key = ?1",
            params!["pending-release-1"],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("missing routing row");
    assert_eq!(route_status, "pending");
    assert!(acked_at.is_none());

    let taken_again = take_messages(&mut conn, Some("email"), 10, "ctox-service")
        .expect("released pending message should be leaseable again");
    assert_eq!(taken_again.len(), 1);
    let acked_after_retake: Option<String> = conn
        .query_row(
            "SELECT acked_at FROM communication_routing_state WHERE message_key = ?1",
            params!["pending-release-1"],
            |row| row.get(0),
        )
        .expect("missing routing row after retake");
    assert!(acked_after_retake.is_none());

    let _ = fs::remove_file(&db_path);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn stalled_inbound_includes_acked_failed_messages_for_repair() {
    let root = std::env::temp_dir().join(format!(
        "ctox-channel-acked-failed-stalled-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
    let db_path = crate::paths::core_db(&root);
    let mut conn = open_channel_db(&db_path).expect("failed to open db");
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: "acked-failed-founder-1",
            channel: "email",
            account_key: "email:cto1@example.com",
            thread_key: "<acked-failed-founder@example.com>",
            remote_id: "remote-acked-failed-founder-1",
            direction: "inbound",
            folder_hint: "INBOX",
            sender_display: "Founder",
            sender_address: "founder@example.com",
            recipient_addresses_json: "[\"cto1@example.com\"]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: "Affiliate follow-up",
            preview: "Please answer this founder follow-up.",
            body_text: "Please answer this founder follow-up.",
            body_html: "",
            raw_payload_ref: "",
            trust_level: "trusted",
            status: "received",
            seen: false,
            has_attachments: false,
            external_created_at: "2026-04-27T09:01:02Z",
            observed_at: "2026-04-27T09:01:02Z",
            metadata_json: "{}",
        },
    )
    .expect("message upsert");
    conn.execute(
        r#"
        INSERT INTO communication_routing_state (
            message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
        ) VALUES (
            'acked-failed-founder-1', 'failed', NULL, NULL,
            '2026-04-27T11:55:27Z', NULL, '2026-04-27T11:55:27Z'
        )
        "#,
        [],
    )
    .expect("failed to seed failed acked route");

    let stalled =
        list_stalled_inbound_messages(&root, 10).expect("failed to list stalled messages");
    assert_eq!(stalled.len(), 1);
    assert_eq!(stalled[0].message_key, "acked-failed-founder-1");

    let _ = fs::remove_file(&db_path);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn stalled_inbound_includes_failed_chat_channel_messages() {
    let root = std::env::temp_dir().join(format!(
        "ctox-channel-chat-stalled-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
    let db_path = crate::paths::core_db(&root);
    let mut conn = open_channel_db(&db_path).expect("failed to open db");
    for (message_key, channel, account_key, thread_key) in [
        (
            "stalled-whatsapp-1",
            "whatsapp",
            "whatsapp:test-account",
            "whatsapp/thread-1",
        ),
        (
            "stalled-telegram-1",
            "telegram",
            "telegram:test-account",
            "telegram/thread-1",
        ),
    ] {
        upsert_communication_message(
            &mut conn,
            UpsertMessage {
                message_key,
                channel,
                account_key,
                thread_key,
                remote_id: message_key,
                direction: "inbound",
                folder_hint: "INBOX",
                sender_display: "Owner",
                sender_address: "chat:owner",
                recipient_addresses_json: "[]",
                cc_addresses_json: "[]",
                bcc_addresses_json: "[]",
                subject: "Chat question",
                preview: "Bitte kurz beantworten.",
                body_text: "Bitte kurz beantworten.",
                body_html: "",
                raw_payload_ref: "",
                trust_level: "trusted",
                status: "received",
                seen: false,
                has_attachments: false,
                external_created_at: "2026-04-27T09:01:02Z",
                observed_at: "2026-04-27T09:01:02Z",
                metadata_json: "{}",
            },
        )
        .expect("message upsert");
        conn.execute(
            r#"
            INSERT INTO communication_routing_state (
                message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
            ) VALUES (?1, 'failed', NULL, NULL, '2026-04-27T11:55:27Z', NULL, '2026-04-27T11:55:27Z')
            "#,
            params![message_key],
        )
        .expect("failed to seed failed chat route");
    }

    let stalled =
        list_stalled_inbound_messages(&root, 10).expect("failed to list stalled messages");
    let mut keys = stalled
        .iter()
        .map(|message| message.message_key.as_str())
        .collect::<Vec<_>>();
    keys.sort_unstable();
    assert_eq!(keys, vec!["stalled-telegram-1", "stalled-whatsapp-1"]);

    let _ = fs::remove_file(&db_path);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn take_messages_prefers_latest_pending_message_in_thread() {
    let db_path = unique_test_db_path("ctox-channel-take-latest-per-thread");
    let mut conn = open_channel_db(&db_path).expect("failed to open db");
    for (message_key, external_created_at, preview) in [
        (
            "thread-msg-old",
            "2026-04-24T18:10:00Z",
            "old founder reply",
        ),
        (
            "thread-msg-new",
            "2026-04-24T18:41:06Z",
            "latest founder reply",
        ),
    ] {
        upsert_communication_message(
            &mut conn,
            UpsertMessage {
                message_key,
                channel: "email",
                account_key: "email:cto1@example.com",
                thread_key: "<latest-thread@example.com>",
                remote_id: message_key,
                direction: "inbound",
                folder_hint: "INBOX",
                sender_display: "Max Mustermann",
                sender_address: "founder@example.com",
                recipient_addresses_json: "[\"cto1@example.com\"]",
                cc_addresses_json: "[]",
                bcc_addresses_json: "[]",
                subject: "Re: Visuelle Homepage",
                preview,
                body_text: preview,
                body_html: "",
                raw_payload_ref: "",
                trust_level: "trusted",
                status: "received",
                seen: false,
                has_attachments: false,
                external_created_at,
                observed_at: external_created_at,
                metadata_json: "{}",
            },
        )
        .expect("message upsert");
    }
    ensure_routing_rows_for_inbound(&conn).expect("routing rows");

    let taken = take_messages(&mut conn, Some("email"), 10, "ctox-service")
        .expect("take messages should succeed");
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].message_key, "thread-msg-new");

    let _ = fs::remove_file(&db_path);
}

#[test]
fn take_messages_ages_threads_while_using_latest_message_within_thread() {
    let db_path = unique_test_db_path("ctox-channel-take-thread-aging");
    let mut conn = open_channel_db(&db_path).expect("failed to open db");
    for (message_key, thread_key, external_created_at) in [
        ("old-thread-first", "thread-old", "2026-04-24T08:00:00Z"),
        ("old-thread-latest", "thread-old", "2026-04-24T10:00:00Z"),
        ("new-thread-latest", "thread-new", "2026-04-24T11:00:00Z"),
    ] {
        upsert_communication_message(
            &mut conn,
            UpsertMessage {
                message_key,
                channel: "email",
                account_key: "email:aging@example.com",
                thread_key,
                remote_id: message_key,
                direction: "inbound",
                folder_hint: "INBOX",
                sender_display: "Customer",
                sender_address: "customer@example.com",
                recipient_addresses_json: "[]",
                cc_addresses_json: "[]",
                bcc_addresses_json: "[]",
                subject: "Aging",
                preview: message_key,
                body_text: message_key,
                body_html: "",
                raw_payload_ref: "",
                trust_level: "trusted",
                status: "received",
                seen: false,
                has_attachments: false,
                external_created_at,
                observed_at: external_created_at,
                metadata_json: "{}",
            },
        )
        .expect("message upsert");
    }
    ensure_routing_rows_for_inbound(&conn).expect("routing rows");

    let taken = take_messages(&mut conn, Some("email"), 1, "ctox-service")
        .expect("take messages should succeed");
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].message_key, "old-thread-latest");

    let _ = fs::remove_file(&db_path);
}

#[test]
fn take_messages_does_not_retake_same_owner_lease() {
    let db_path = unique_test_db_path("ctox-channel-no-same-owner-retake");
    let mut conn = open_channel_db(&db_path).expect("failed to open db");
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: "single-message",
            channel: "email",
            account_key: "email:single@example.com",
            thread_key: "single-thread",
            remote_id: "single-message",
            direction: "inbound",
            folder_hint: "INBOX",
            sender_display: "Customer",
            sender_address: "customer@example.com",
            recipient_addresses_json: "[]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: "Single",
            preview: "single",
            body_text: "single",
            body_html: "",
            raw_payload_ref: "",
            trust_level: "trusted",
            status: "received",
            seen: false,
            has_attachments: false,
            external_created_at: "2026-04-24T08:00:00Z",
            observed_at: "2026-04-24T08:00:00Z",
            metadata_json: "{}",
        },
    )
    .expect("message upsert");
    ensure_routing_rows_for_inbound(&conn).expect("routing rows");

    assert_eq!(
        take_messages(&mut conn, Some("email"), 1, "ctox-service")
            .expect("first take")
            .len(),
        1
    );
    assert!(take_messages(&mut conn, Some("email"), 1, "ctox-service")
        .expect("second take")
        .is_empty());

    let _ = fs::remove_file(&db_path);
}

#[test]
fn thread_prefers_voice_reply_for_voice_jami_inbound() {
    let db_path = unique_test_db_path("ctox-channel-jami-voice");
    let mut conn = open_channel_db(&db_path).expect("failed to open db");
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: "msg-jami-voice-1",
            channel: "jami",
            account_key: "jami:test-account",
            thread_key: "jami/thread-voice-1",
            remote_id: "remote-jami-voice-1",
            direction: "inbound",
            folder_hint: "INBOX",
            sender_display: "Owner",
            sender_address: "jami:owner",
            recipient_addresses_json: "[]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: "Voice subject",
            preview: "voice preview",
            body_text: "voice body",
            body_html: "",
            raw_payload_ref: "",
            trust_level: "owner_verified",
            status: "received",
            seen: false,
            has_attachments: true,
            external_created_at: "2026-03-29T08:00:00Z",
            observed_at: "2026-03-29T08:00:00Z",
            metadata_json: r#"{"preferredReplyModality":"voice"}"#,
        },
    )
    .expect("failed to upsert jami voice message");

    assert!(thread_prefers_voice_reply(&conn, "jami/thread-voice-1")
        .expect("failed to resolve jami voice preference"));

    let _ = fs::remove_file(&db_path);
}

#[test]
fn system_probe_inbound_messages_are_marked_handled() {
    let db_path = unique_test_db_path("ctox-channel-system-probe");
    let mut conn = open_channel_db(&db_path).expect("failed to open db");
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: "probe-1",
            channel: "email",
            account_key: "email:test@example.com",
            thread_key: "email/thread-self-test",
            remote_id: "remote-probe-1",
            direction: "inbound",
            folder_hint: "INBOX",
            sender_display: "CTOX",
            sender_address: "test@example.com",
            recipient_addresses_json: "[]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: "[CTOX mail self-test] 2026-03-26T10:00:00Z",
            preview: "CTOX self-test",
            body_text: "CTOX self-test <abc@example.com>",
            body_html: "",
            raw_payload_ref: "",
            trust_level: "system_probe",
            status: "self_test_received",
            seen: false,
            has_attachments: false,
            external_created_at: "2026-03-26T10:00:00Z",
            observed_at: "2026-03-26T10:00:00Z",
            metadata_json: "{\"technicalSelfTest\":true}",
        },
    )
    .expect("failed to insert system probe");
    ensure_routing_rows_for_inbound(&conn).expect("failed to backfill routing rows");
    let route_status: String = conn
        .query_row(
            "SELECT route_status FROM communication_routing_state WHERE message_key = ?1",
            params!["probe-1"],
            |row| row.get(0),
        )
        .expect("missing routing row");
    assert_eq!(route_status, "handled");

    let _ = fs::remove_file(&db_path);
}

#[test]
fn system_probe_routing_heals_existing_non_handled_rows() {
    let db_path = unique_test_db_path("ctox-channel-system-probe-heal");
    let mut conn = open_channel_db(&db_path).expect("failed to open db");
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: "probe-2",
            channel: "email",
            account_key: "email:test@example.com",
            thread_key: "email/thread-self-test-2",
            remote_id: "remote-probe-2",
            direction: "inbound",
            folder_hint: "INBOX",
            sender_display: "CTOX",
            sender_address: "test@example.com",
            recipient_addresses_json: "[]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: "[CTOX mail self-test] 2026-03-26T10:10:00Z",
            preview: "CTOX self-test",
            body_text: "CTOX self-test <def@example.com>",
            body_html: "",
            raw_payload_ref: "",
            trust_level: "system_probe",
            status: "self_test_received",
            seen: false,
            has_attachments: false,
            external_created_at: "2026-03-26T10:10:00Z",
            observed_at: "2026-03-26T10:10:00Z",
            metadata_json: "{\"technicalSelfTest\":true}",
        },
    )
    .expect("failed to insert system probe");
    conn.execute(
        "INSERT INTO communication_routing_state (message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at) VALUES (?1, 'blocked_sender', NULL, NULL, NULL, 'legacy', ?2)",
        params!["probe-2", "2026-03-26T10:10:01Z"],
    )
    .expect("failed to seed legacy routing row");
    ensure_routing_rows_for_inbound(&conn).expect("failed to normalize routing rows");
    let route_status: String = conn
        .query_row(
            "SELECT route_status FROM communication_routing_state WHERE message_key = ?1",
            params!["probe-2"],
            |row| row.get(0),
        )
        .expect("missing routing row");
    assert_eq!(route_status, "handled");

    let _ = fs::remove_file(&db_path);
}

#[test]
fn channel_history_and_search_can_reconstruct_related_messages() {
    let db_path = unique_test_db_path("ctox-channel-search");
    let mut conn = open_channel_db(&db_path).expect("failed to open db");
    for (message_key, thread_key, sender_address, subject, preview, body_text, created_at) in [
        (
            "msg-1",
            "email/thread-a",
            "owner@example.com",
            "Nextcloud blocked",
            "Need endpoint",
            "Nextcloud is blocked on missing endpoint and credentials.",
            "2026-03-26T10:00:00Z",
        ),
        (
            "msg-2",
            "email/thread-a",
            "ctox@example.com",
            "Nextcloud blocked",
            "Asked for endpoint",
            "I asked for NEXTCLOUD_URL and credentials.",
            "2026-03-26T10:05:00Z",
        ),
        (
            "msg-3",
            "email/thread-b",
            "owner@example.com",
            "Redis recovered",
            "Rotated secret",
            "The Redis password was rotated and the service is healthy now.",
            "2026-03-26T11:00:00Z",
        ),
    ] {
        upsert_communication_message(
            &mut conn,
            UpsertMessage {
                message_key,
                channel: "email",
                account_key: "email:cto1@example.com",
                thread_key,
                remote_id: message_key,
                direction: "inbound",
                folder_hint: "INBOX",
                sender_display: "Owner",
                sender_address,
                recipient_addresses_json: "[]",
                cc_addresses_json: "[]",
                bcc_addresses_json: "[]",
                subject,
                preview,
                body_text,
                body_html: "",
                raw_payload_ref: "",
                trust_level: "owner_verified",
                status: "received",
                seen: false,
                has_attachments: false,
                external_created_at: created_at,
                observed_at: created_at,
                metadata_json: "{}",
            },
        )
        .expect("failed to insert communication message");
    }

    let thread_history =
        list_thread_messages(&conn, "email/thread-a", 10).expect("failed to load history");
    assert_eq!(thread_history.len(), 2);
    assert_eq!(thread_history[0].message_key, "msg-2");

    let search = search_messages(&conn, "nextcloud endpoint", Some("email"), None, 10)
        .expect("failed to search");
    assert_eq!(search.len(), 2);
    assert!(search
        .iter()
        .all(|item| item.thread_key == "email/thread-a"));

    let sender_search =
        search_messages(&conn, "redis", Some("email"), Some("owner@example.com"), 10)
            .expect("failed to search sender-scoped messages");
    assert_eq!(sender_search.len(), 1);
    assert_eq!(sender_search[0].message_key, "msg-3");

    let _ = fs::remove_file(&db_path);
}

#[test]
fn channel_context_groups_thread_state_blockers_and_open_questions() {
    let db_path = unique_test_db_path("ctox-channel-context");
    let mut conn = open_channel_db(&db_path).expect("failed to open db");
    for (
        message_key,
        thread_key,
        direction,
        sender_display,
        sender_address,
        subject,
        preview,
        body_text,
        created_at,
    ) in [
        (
            "ctx-1",
            "email/thread-zammad",
            "inbound",
            "Michael",
            "michael@example.com",
            "Zammad status",
            "Please finish it",
            "Can you finish Zammad and report the blocker?",
            "2026-03-26T10:00:00Z",
        ),
        (
            "ctx-2",
            "email/thread-zammad",
            "outbound",
            "CTOX",
            "cto1@example.com",
            "Zammad status",
            "Still blocked",
            "Blocked: the admin API still returns 401 and I need to repair auth.",
            "2026-03-26T10:05:00Z",
        ),
        (
            "ctx-3",
            "email/thread-zammad",
            "inbound",
            "Michael",
            "michael@example.com",
            "Zammad status",
            "Please continue",
            "Please continue and tell me if you need anything else?",
            "2026-03-26T10:10:00Z",
        ),
        (
            "ctx-4",
            "tui/main",
            "inbound",
            "Michael",
            "tui:local",
            "TUI",
            "Freigabe",
            "Die Freigabe fuer die Zammad-Reparatur ist erteilt.",
            "2026-03-26T10:12:00Z",
        ),
        (
            "ctx-5",
            "email/thread-redis",
            "outbound",
            "CTOX",
            "cto1@example.com",
            "Redis repaired",
            "Follow-up queued",
            "I queued a follow-up review for Redis and will continue after verification.",
            "2026-03-26T09:00:00Z",
        ),
    ] {
        upsert_communication_message(
            &mut conn,
            UpsertMessage {
                message_key,
                channel: if thread_key.starts_with("tui/") {
                    "tui"
                } else {
                    "email"
                },
                account_key: "email:cto1@example.com",
                thread_key,
                remote_id: message_key,
                direction,
                folder_hint: if direction == "inbound" {
                    "INBOX"
                } else {
                    "Sent"
                },
                sender_display,
                sender_address,
                recipient_addresses_json: "[]",
                cc_addresses_json: "[]",
                bcc_addresses_json: "[]",
                subject,
                preview,
                body_text,
                body_html: "",
                raw_payload_ref: "",
                trust_level: "owner_verified",
                status: "received",
                seen: false,
                has_attachments: false,
                external_created_at: created_at,
                observed_at: created_at,
                metadata_json: "{}",
            },
        )
        .expect("failed to insert context message");
    }

    let context = build_communication_context(
        &conn,
        "email/thread-zammad",
        Some("zammad blocker repair"),
        Some("michael@example.com"),
        10,
    )
    .expect("failed to build communication context");

    assert_eq!(context.thread_messages.len(), 3);
    assert_eq!(context.latest_subject.as_deref(), Some("Zammad status"));
    assert_eq!(
        context
            .latest_inbound
            .as_ref()
            .map(|item| item.message_key.as_str()),
        Some("ctx-3")
    );
    assert_eq!(
        context
            .latest_outbound
            .as_ref()
            .map(|item| item.message_key.as_str()),
        Some("ctx-2")
    );
    assert!(!context.candidate_blockers.is_empty());
    assert!(context
        .candidate_blockers
        .iter()
        .any(|item| item.message_key == "ctx-2"));
    assert!(!context.open_owner_questions.is_empty());
    assert!(context
        .related_messages
        .iter()
        .any(|item| item.message_key == "ctx-4"));

    let _ = fs::remove_file(&db_path);
}

// F4: pipeline_status returns a structured snapshot joining mission
// state, agent attempts, review/approval rows, and outbound sends for
// a given thread_key. The test seeds rows into the runtime db that
// `pipeline_status` will resolve via `resolve_db_path(root, None)`.
#[test]
fn pipeline_status_reports_thread_state() {
    let root = std::env::temp_dir().join(format!(
        "ctox-pipeline-status-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");

    let thread_key = "pipeline-status-thread";

    // Seed the channel db with one outbound message and one routing row.
    let db_path = crate::paths::core_db(&root);
    let mut conn = open_channel_db(&db_path).expect("open channel db");
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: "outbound-1",
            channel: "email",
            account_key: "email:cto1@example.com",
            thread_key,
            remote_id: "remote-1",
            direction: "outbound",
            folder_hint: "Sent",
            sender_display: "CTOX",
            sender_address: "cto1@example.com",
            recipient_addresses_json: "[\"founder@example.com\"]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: "Founder update",
            preview: "Update for founder.",
            body_text: "Update for founder.",
            body_html: "",
            raw_payload_ref: "",
            trust_level: "trusted",
            status: "sent",
            seen: true,
            has_attachments: false,
            external_created_at: "2026-04-27T10:00:00Z",
            observed_at: "2026-04-27T10:00:00Z",
            metadata_json: "{}",
        },
    )
    .expect("failed to upsert outbound");

    // Seed an agent assistant row with structured outcome on the matching conversation_id.
    let conversation_id =
        crate::execution::agent::turn_loop::conversation_id_for_thread_key(Some(thread_key));
    let engine = crate::lcm::LcmEngine::open(&db_path, crate::lcm::LcmConfig::default())
        .expect("open lcm engine");
    let _ = engine
        .add_message_with_outcome(
            conversation_id,
            "assistant",
            "(agent turn did not complete)",
            Some(crate::lcm::AgentOutcome::TurnTimeout),
        )
        .expect("seed assistant row");
    // Bump the failure counter to simulate one failed turn.
    let _ = engine
        .increment_mission_agent_failure_count(conversation_id)
        .expect("bump failure count");

    let report = pipeline_status(&root, Some(thread_key), 10).expect("pipeline status");
    assert_eq!(report.thread_key.as_deref(), Some(thread_key));
    assert_eq!(report.send_attempts.len(), 1);
    assert_eq!(report.send_attempts[0].message_key, "outbound-1");
    assert_eq!(report.agent_attempts.len(), 1);
    assert_eq!(
        report.agent_attempts[0].outcome.as_deref(),
        Some("TurnTimeout")
    );
    assert_eq!(report.agent_failure_count, 1);
    // No review row was seeded → no founder_outbound_intent.
    assert!(!report.founder_outbound_intent);
    assert_eq!(report.rewrite_iteration_count, 0);
    assert_eq!(report.rework_iteration_count, 1);
    assert_eq!(report.current_disposition, "RequeueInternalWork");
    assert!(
        report.strategic_directive_authority_events.is_empty(),
        "no strategic-directive authority events seeded → field must be empty"
    );

    let _ = fs::remove_dir_all(&root);
}

// E (PR): pipeline_status surfaces strategic-directive owner-authority
// governance events that match the thread, but skips events whose
// details reference a different thread.
#[test]
fn pipeline_status_surfaces_strategic_directive_authority_events() {
    let root = std::env::temp_dir().join(format!(
        "ctox-pipeline-strategy-auth-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");

    let thread_key = "pipeline-strategy-auth-thread";
    let conversation_id =
        crate::execution::agent::turn_loop::conversation_id_for_thread_key(Some(thread_key));

    // Seed two strategic-directive authority events: one matching this
    // thread, one matching an unrelated thread. Only the first should
    // surface in the report.
    let _ = crate::governance::record_event(
        &root,
        crate::governance::GovernanceEventRequest {
            mechanism_id: "strategic_directive_mutation_owner_authorised",
            conversation_id: Some(conversation_id),
            severity: "info",
            reason: "test-permitted",
            action_taken: "permitted_strategic_directive_mutation",
            details: serde_json::json!({
                "triggered_by_message_key": "owner-msg-A",
                "sender_address": "owner@example.com",
                "sender_role": "owner",
                "directive_kind": "mission",
                "attempted_status": "active",
                "action": "set",
                "thread_key": thread_key,
                "conversation_id": conversation_id,
            }),
            idempotence_key: Some("test-permitted-A"),
        },
    )
    .expect("record permitted event");
    let _ = crate::governance::record_event(
        &root,
        crate::governance::GovernanceEventRequest {
            mechanism_id: "strategic_directive_mutation_blocked_non_owner_sender",
            conversation_id: None,
            severity: "critical",
            reason: "test-blocked-other-thread",
            action_taken: "blocked_strategic_directive_mutation",
            details: serde_json::json!({
                "triggered_by_message_key": "founder-msg-B",
                "sender_address": "founder@example.com",
                "sender_role": "founder",
                "directive_kind": "vision",
                "attempted_status": "active",
                "action": "set",
                "thread_key": "some-other-thread",
            }),
            idempotence_key: Some("test-blocked-B"),
        },
    )
    .expect("record blocked event");

    let report = pipeline_status(&root, Some(thread_key), 10).expect("pipeline status");
    let events = &report.strategic_directive_authority_events;
    assert_eq!(
        events.len(),
        1,
        "expected exactly the matching authority event in the per-thread report, got {events:#?}"
    );
    assert_eq!(
        events[0].mechanism_id,
        "strategic_directive_mutation_owner_authorised"
    );
    assert_eq!(events[0].sender_role.as_deref(), Some("owner"));
    assert_eq!(events[0].directive_kind.as_deref(), Some("mission"));
    assert_eq!(events[0].action.as_deref(), Some("set"));
    assert_eq!(
        events[0].triggered_by_message_key.as_deref(),
        Some("owner-msg-A")
    );

    let _ = fs::remove_dir_all(&root);
}

fn unique_root(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{prefix}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ))
}

fn upsert_test_inbound(conn: &mut Connection, message_key: &str, metadata: Value) -> Result<()> {
    upsert_communication_message(
        conn,
        UpsertMessage {
            message_key,
            channel: "email",
            account_key: "email:cto1@example.com",
            thread_key: "email/test-thread",
            remote_id: message_key,
            direction: "inbound",
            folder_hint: "INBOX",
            sender_display: "Jill",
            sender_address: "a.lindner@example.com",
            recipient_addresses_json: "[]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: "Re: any subject (irrelevant for structural test)",
            preview: "preview",
            body_text: "body",
            body_html: "",
            raw_payload_ref: "",
            trust_level: "high",
            status: "received",
            seen: false,
            has_attachments: false,
            external_created_at: "2026-04-27T09:00:00Z",
            observed_at: "2026-04-27T09:00:00Z",
            metadata_json: &metadata.to_string(),
        },
    )?;
    Ok(())
}

#[test]
fn metadata_marks_auto_submitted_consults_only_structured_fields() {
    // Subject and body content must NOT influence the decision —
    // only the structured fields written by the inbound parser.
    let positive = json!({"autoSubmitted": true});
    assert!(metadata_marks_auto_submitted(&positive));
    let suppress = json!({"autoResponseSuppress": true});
    assert!(metadata_marks_auto_submitted(&suppress));
    let raw_value = json!({"autoSubmittedValue": "auto-replied; foo=bar"});
    assert!(metadata_marks_auto_submitted(&raw_value));
    let neg = json!({
        "subject": "Automatische Antwort: ich bin im Urlaub",
        "body_text": "Out of office until 2026-05-12.",
    });
    assert!(
        !metadata_marks_auto_submitted(&neg),
        "subject/body strings must never trigger the marker"
    );
    let explicit_no = json!({"autoSubmitted": false, "autoSubmittedValue": "no"});
    assert!(!metadata_marks_auto_submitted(&explicit_no));
}

#[test]
fn route_status_is_terminal_covers_documented_terminal_states() {
    for sticky in ["handled", "cancelled", "failed", "completed", "superseded"] {
        assert!(
            route_status_is_terminal(sticky),
            "{sticky} must be terminal"
        );
    }
    for non_sticky in ["pending", "leased", "review_rework", "blocked", ""] {
        assert!(
            !route_status_is_terminal(non_sticky),
            "{non_sticky} must NOT be terminal"
        );
    }
}

#[test]
fn record_terminal_no_send_verdict_is_persistent_and_idempotent() -> Result<()> {
    let root = unique_root("ctox-no-send-verdict");
    fs::create_dir_all(root.join("runtime"))?;
    let db_path = resolve_db_path(&root, None);
    let mut conn = open_channel_db(&db_path)?;
    let key = "email:cto1@example.com::ooo-1";
    upsert_test_inbound(
        &mut conn,
        key,
        json!({"autoSubmitted": true, "autoSubmittedValue": "auto-replied"}),
    )?;
    drop(conn);

    record_terminal_no_send_verdict(&root, key, "test", "first NO-SEND: auto-reply")?;
    assert!(inbound_message_has_terminal_no_send(&root, key)?);

    // Re-recording must be idempotent and must not flip the flag.
    record_terminal_no_send_verdict(&root, key, "test", "second NO-SEND record (idempotent)")?;
    assert!(inbound_message_has_terminal_no_send(&root, key)?);

    // A different inbound key has no verdict.
    assert!(!inbound_message_has_terminal_no_send(
        &root,
        "email:cto1@example.com::other"
    )?);

    let _ = fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn inbound_message_is_auto_submitted_reads_persisted_metadata() -> Result<()> {
    let root = unique_root("ctox-auto-submitted-metadata");
    fs::create_dir_all(root.join("runtime"))?;
    let db_path = resolve_db_path(&root, None);
    let mut conn = open_channel_db(&db_path)?;
    let auto_key = "email:cto1@example.com::ooo-2";
    let human_key = "email:cto1@example.com::human-1";
    upsert_test_inbound(
        &mut conn,
        auto_key,
        json!({"autoSubmitted": true, "autoSubmittedValue": "auto-replied"}),
    )?;
    upsert_test_inbound(&mut conn, human_key, json!({"autoSubmitted": false}))?;
    drop(conn);
    assert!(inbound_message_is_auto_submitted(&root, auto_key)?);
    assert!(!inbound_message_is_auto_submitted(&root, human_key)?);
    assert!(!inbound_message_is_auto_submitted(
        &root,
        "email:does-not-exist"
    )?);
    let _ = fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn message_metadata_marks_auto_submitted_low_level_helper_works_off_db() -> Result<()> {
    // Tests the low-level conn-bound helper used by the
    // founder-handled-ack guard. It must consult only the
    // structured metadata field, never subject/body strings.
    let root = unique_root("ctox-meta-marks-auto-submitted");
    fs::create_dir_all(root.join("runtime"))?;
    let db_path = resolve_db_path(&root, None);
    let mut conn = open_channel_db(&db_path)?;
    let auto_key = "email:cto1@example.com::ooo-low";
    let human_key = "email:cto1@example.com::human-low";
    upsert_test_inbound(&mut conn, auto_key, json!({"autoSubmitted": true}))?;
    upsert_test_inbound(&mut conn, human_key, json!({}))?;
    assert!(message_metadata_marks_auto_submitted(&conn, auto_key)?);
    assert!(!message_metadata_marks_auto_submitted(&conn, human_key)?);

    // And the no-send-flag helper reads only the
    // terminal_no_send column, not the review_summary string.
    record_terminal_no_send_verdict(&root, auto_key, "test", "auto-replied / NO-SEND")?;
    let conn = open_channel_db(&db_path)?;
    assert!(message_has_terminal_no_send_in_conn(&conn, auto_key)?);
    assert!(!message_has_terminal_no_send_in_conn(&conn, human_key)?);

    let _ = fs::remove_dir_all(&root);
    Ok(())
}

// -----------------------------------------------------------------------
// RFC 0001 Phase 1 — body durability before Sending + SendFailed transition.
//
// These tests cover the helper functions added to harden the
// `send_reviewed_founder_*` paths against provider failure. Helper-level
// tests are used because the full `send_reviewed_founder_*` paths require
// account configs, identity profiles, settings, and a live email adapter
// — wiring all of that for an injected mock would be a larger refactor
// than Phase 1 is scoped for. The helpers are the load-bearing surface:
// if they round-trip correctly, the wiring in `send_reviewed_founder_*`
// (a `match` over `send_email_message` with the same helper calls) is a
// small, locally-auditable change.
// -----------------------------------------------------------------------

fn phase1_test_request(body: &str) -> ChannelSendRequest {
    ChannelSendRequest {
        channel: "email".to_string(),
        account_key: "email:cto1@example.com".to_string(),
        thread_key: "<phase1-thread@example.com>".to_string(),
        body: body.to_string(),
        subject: "Vorschlag Tag-System fuer Lead-Funnel".to_string(),
        to: vec!["j.kowalski@example.com".to_string()],
        cc: vec![
            "a.lindner@example.com".to_string(),
            "d.berger@example.com".to_string(),
        ],
        attachments: Vec::new(),
        sender_display: None,
        sender_address: None,
        send_voice: false,
        reviewed_founder_send: true,
    }
}

#[test]
fn phase1_record_outbound_pending_send_persists_body_with_draft_pending_send_status() {
    let db_path = unique_test_db_path("ctox-phase1-pending-send");
    let conn = open_channel_db(&db_path).expect("failed to open db");
    let request =
        phase1_test_request("Hallo Jill,\n\nVorschlag fuer Tag-System: ...\n\nGruesse, Yoda");
    let body_sha256 = sha256_hex(request.body.trim().as_bytes());

    let message_key =
        record_outbound_pending_send(&conn, &request, "founder-review:phase1", &body_sha256)
            .expect("pending send must persist")
            .message_key;

    let (status, body_text, direction, subject, recipients_json, metadata_json): (
        String,
        String,
        String,
        String,
        String,
        String,
    ) = conn
        .query_row(
            "SELECT status, body_text, direction, subject, recipient_addresses_json, metadata_json
             FROM communication_messages WHERE message_key = ?1",
            params![message_key],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .expect("row must exist");
    assert_eq!(status, "draft_pending_send");
    assert_eq!(direction, "outbound");
    assert_eq!(body_text, request.body);
    assert_eq!(subject, request.subject);
    assert!(recipients_json.contains("j.kowalski@example.com"));
    let metadata: Value = serde_json::from_str(&metadata_json).expect("valid json");
    assert_eq!(
        metadata.get("approval_key").and_then(Value::as_str),
        Some("founder-review:phase1")
    );
    assert_eq!(
        metadata.get("body_sha256").and_then(Value::as_str),
        Some(body_sha256.as_str())
    );
    assert_eq!(
        metadata.get("pending_send").and_then(Value::as_bool),
        Some(true)
    );

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn reviewed_founder_send_cli_path_requires_exact_unconsumed_review() {
    let root = unique_root("ctox-reviewed-send-cli-approval");
    fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
    let body = "Hallo Julia,\n\nhier ist der freigegebene Vorschlag.\n\nViele Gruesse";
    let action = FounderOutboundAction {
        account_key: "email:cto1@example.com".to_string(),
        thread_key: "salesforce-tags".to_string(),
        subject: "Vorschlag Tag-System fuer Lead-Funnel".to_string(),
        to: vec!["j.kowalski@example.com".to_string()],
        cc: vec!["a.lindner@example.com".to_string()],
        attachments: Vec::new(),
    };

    record_founder_outbound_review_approval(
        &root,
        "tui-outbound:test",
        &action,
        body,
        "PASS: send-ready",
    )
    .expect("approval should persist");
    let conn = open_channel_db(&resolve_db_path(&root, None)).expect("failed to open db");

    let (approval_key, anchor) =
        require_any_unconsumed_founder_outbound_review(&conn, &action, body)
            .expect("exact reviewed send should find approval");
    assert!(approval_key.starts_with("founder-outbound-review:tui-outbound:test:"));
    assert_eq!(anchor, "tui-outbound:test");

    let err = require_any_unconsumed_founder_outbound_review(
        &conn,
        &action,
        "Hallo Julia,\n\nleicht geaenderter Text.",
    )
    .expect_err("changed body must not match review approval");
    assert!(
        err.to_string()
            .contains("no matching unconsumed review approval"),
        "unexpected error: {err}"
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn outbound_send_marks_do_not_clobber_a_resolved_row() {
    // EGRESS-5: the live mark_outbound_send_* now CAS-guard on a pending
    // status, so a late/duplicate failure can never clobber an accepted
    // send (and a duplicate accept can never resurrect a terminal row), and
    // a 0-row case is an idempotent no-op rather than an error.
    let db_path = unique_test_db_path("ctox-egress5-cas");
    let conn = open_channel_db(&db_path).expect("failed to open db");
    let request = phase1_test_request("Body fuer EGRESS-5");
    let body_sha256 = sha256_hex(request.body.trim().as_bytes());
    let message_key =
        record_outbound_pending_send(&conn, &request, "founder-review:egress5", &body_sha256)
            .expect("pending send must persist")
            .message_key;

    // Accept the pending send.
    mark_outbound_send_accepted(&conn, &message_key, "accepted", &json!({"ok": true}))
        .expect("accept must succeed for a pending row");
    let status: String = conn
        .query_row(
            "SELECT status FROM communication_messages WHERE message_key = ?1",
            params![message_key],
            |row| row.get(0),
        )
        .expect("row must exist");
    assert_eq!(status, "accepted");

    // A late/duplicate failure mark must NOT clobber the accepted send and
    // must NOT error (erroring on the send path could trigger a re-send).
    mark_outbound_send_failed(&conn, &message_key, "late smtp timeout")
        .expect("a stale failure mark must be an idempotent no-op, not an error");
    let status_after: String = conn
        .query_row(
            "SELECT status FROM communication_messages WHERE message_key = ?1",
            params![message_key],
            |row| row.get(0),
        )
        .expect("row must exist");
    assert_eq!(
        status_after, "accepted",
        "a late failure must not clobber an accepted send"
    );

    // A duplicate accept on the now-terminal row is also an idempotent no-op.
    mark_outbound_send_accepted(&conn, &message_key, "accepted", &json!({"ok": true}))
        .expect("duplicate accept must be an idempotent no-op");

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn phase1_pending_send_message_key_is_stable_for_same_request() {
    let request = phase1_test_request("Konsistente Anfrage");
    let body_sha256 = sha256_hex(request.body.trim().as_bytes());
    let key_a = pending_send_message_key(&request, &body_sha256);
    let key_b = pending_send_message_key(&request, &body_sha256);
    assert_eq!(
        key_a, key_b,
        "same request inputs must yield the same message_key — retry binding"
    );

    let mut request_changed = phase1_test_request("Konsistente Anfrage");
    request_changed.body = "Andere Nachricht".to_string();
    let other_sha = sha256_hex(request_changed.body.trim().as_bytes());
    let key_c = pending_send_message_key(&request_changed, &other_sha);
    assert_ne!(
        key_a, key_c,
        "different body must yield a different durable message_key"
    );
}

#[test]
fn phase1_update_pending_send_to_accepted_flips_status_and_records_adapter_result() {
    let db_path = unique_test_db_path("ctox-phase1-accepted");
    let conn = open_channel_db(&db_path).expect("failed to open db");
    let request = phase1_test_request("Body fuer Erfolg");
    let body_sha256 = sha256_hex(request.body.trim().as_bytes());
    let message_key =
        record_outbound_pending_send(&conn, &request, "founder-review:phase1", &body_sha256)
            .expect("pending send must persist")
            .message_key;

    update_pending_send_to_accepted(
        &conn,
        &message_key,
        &json!({
            "ok": true,
            "channel": "email",
            "status": "accepted",
            "remote_id": "smtp-msg-123",
        }),
    )
    .expect("accepted update must succeed");

    let (status, metadata_json): (String, String) = conn
        .query_row(
            "SELECT status, metadata_json FROM communication_messages WHERE message_key = ?1",
            params![message_key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("row must exist");
    assert_eq!(status, "accepted");
    let metadata: Value = serde_json::from_str(&metadata_json).expect("valid json");
    assert_eq!(
        metadata.get("pending_send").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        metadata.get("transitioned_to").and_then(Value::as_str),
        Some("accepted")
    );
    assert_eq!(
        metadata
            .get("adapter_result")
            .and_then(|value| value.get("remote_id"))
            .and_then(Value::as_str),
        Some("smtp-msg-123")
    );

    // Idempotence guard: a second accepted-update on a non-pending row
    // must error rather than silently overwrite.
    let err = update_pending_send_to_accepted(&conn, &message_key, &json!({}))
        .expect_err("second accepted-update must fail because row is no longer pending");
    assert!(err.to_string().contains("not in draft_pending_send"));

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn terminal_founder_outbound_artifact_count_accepts_terminal_and_queued_send_row() {
    let root = unique_root("ctox-terminal-outbound-artifact-count");
    fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
    let db_path = resolve_db_path(&root, None);
    let conn = open_channel_db(&db_path).expect("failed to open db");
    let request = phase1_test_request("Body fuer Outcome-Gate");
    let action = FounderOutboundAction {
        account_key: request.account_key.to_ascii_uppercase(),
        thread_key: request.thread_key.clone(),
        subject: request.subject.clone(),
        to: request.to.clone(),
        cc: request.cc.clone(),
        attachments: request.attachments.clone(),
    };
    let body_sha256 = sha256_hex(request.body.trim().as_bytes());
    let message_key =
        record_outbound_pending_send(&conn, &request, "founder-review:phase1", &body_sha256)
            .expect("pending send must persist")
            .message_key;

    assert_eq!(
        terminal_founder_outbound_artifact_count(&root, &action).expect("count pending artifact"),
        0,
        "draft_pending_send is not a delivered outcome"
    );

    update_pending_send_to_accepted(
        &conn,
        &message_key,
        &json!({
            "ok": true,
            "channel": "email",
            "status": "accepted",
        }),
    )
    .expect("accepted update must succeed");

    assert_eq!(
        terminal_founder_outbound_artifact_count(&root, &action).expect("count accepted artifact"),
        1
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn reviewed_send_result_witness_accepts_queued_sent_artifact_by_message_key() {
    let root = unique_root("ctox-reviewed-send-result-witness");
    fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
    let db_path = resolve_db_path(&root, None);
    let conn = open_channel_db(&db_path).expect("failed to open db");
    let request = phase1_test_request("Body fuer Queue-Witness");
    let body_sha256 = sha256_hex(request.body.trim().as_bytes());
    let message_key =
        record_outbound_pending_send(&conn, &request, "founder-review:queued", &body_sha256)
            .expect("pending send must persist")
            .message_key;

    assert!(
        !reviewed_send_result_has_durable_outbound_artifact(
            &root,
            &json!({ "message_key": message_key })
        )
        .expect("pending witness should be queryable"),
        "draft_pending_send must not satisfy the reviewed-send witness"
    );

    mark_outbound_send_accepted(
        &conn,
        &message_key,
        "queued",
        &json!({
            "ok": true,
            "channel": "email",
            "status": "queued",
            "provider_dispatch_status": "queued_in_mailserver",
        }),
    )
    .expect("queued update must persist");

    assert!(
        reviewed_send_result_has_durable_outbound_artifact(
            &root,
            &json!({ "message_key": message_key })
        )
        .expect("queued witness should be queryable"),
        "a concrete queued send row with pendingSend=false is a durable outbound artifact"
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn phase1_update_pending_send_to_failed_preserves_body_and_records_provider_error() {
    let db_path = unique_test_db_path("ctox-phase1-failed");
    let conn = open_channel_db(&db_path).expect("failed to open db");
    let request =
        phase1_test_request("Body fuer Provider-Fehler-Pfad: muss nach dem Failure noch da sein.");
    let body_sha256 = sha256_hex(request.body.trim().as_bytes());
    let message_key =
        record_outbound_pending_send(&conn, &request, "founder-review:phase1", &body_sha256)
            .expect("pending send must persist")
            .message_key;

    update_pending_send_to_failed(
        &conn,
        &message_key,
        "smtp authentication failed: 535 5.7.0 outdated endpoint",
    )
    .expect("send-failed update must succeed");

    let (status, body_text, metadata_json): (String, String, String) = conn
        .query_row(
            "SELECT status, body_text, metadata_json FROM communication_messages WHERE message_key = ?1",
            params![message_key],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("row must exist");
    assert_eq!(status, "send_failed");
    assert_eq!(
        body_text, request.body,
        "body must survive provider failure for retry"
    );
    let metadata: Value = serde_json::from_str(&metadata_json).expect("valid json");
    assert_eq!(
        metadata.get("transitioned_to").and_then(Value::as_str),
        Some("send_failed")
    );
    assert!(metadata
        .get("provider_error")
        .and_then(Value::as_str)
        .unwrap_or("")
        .contains("smtp authentication failed"));

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn phase1_emit_send_failed_transition_records_kernel_proof() {
    let db_path = unique_test_db_path("ctox-phase1-sendfailed-kernel");
    let conn = open_channel_db(&db_path).expect("failed to open db");
    let request = phase1_test_request("Body fuer Kernel-Transition");
    let body_sha256 = sha256_hex(request.body.trim().as_bytes());
    let pending_message_key =
        record_outbound_pending_send(&conn, &request, "founder-review:phase1", &body_sha256)
            .expect("pending send must persist")
            .message_key;

    // First the Approved → Sending proof (precondition for SendFailed):
    enforce_reviewed_founder_send_core_transition(
        &conn,
        "founder-outbound:phase1-anchor",
        "founder-review:phase1",
        &request,
    )
    .expect("Approved->Sending must be accepted");

    // Now the failure path:
    emit_reviewed_founder_send_failed_transition(
        &conn,
        "founder-outbound:phase1-anchor",
        "founder-review:phase1",
        &request,
        &pending_message_key,
        "smtp 535 outdated endpoint",
    )
    .expect("Sending->SendFailed must be accepted by kernel");

    let send_failed: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ctox_core_transition_proofs
             WHERE entity_type = 'FounderCommunication'
               AND lane = 'P0FounderCommunication'
               AND from_state = 'Sending'
               AND to_state = 'SendFailed'
               AND core_event = 'Fail'
               AND accepted = 1",
            [],
            |row| row.get(0),
        )
        .expect("kernel proof query must run");
    assert_eq!(
        send_failed, 1,
        "the Sending->SendFailed transition must be witnessed by the kernel"
    );

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn phase1_stranded_send_attempt_blocks_blind_resend() {
    // EGRESS-2: distinguish "never sent" from "maybe sent". A fresh pending
    // row (provider call not yet initiated) is safe to retry; once the
    // provider send is initiated (send_attempt_started_at marker) the row is
    // stranded "maybe sent" and send_email_message must refuse to
    // blind-resend it; once accepted (durable) it is no longer stranded
    // because the durable-result dedupe owns it.
    let db_path = unique_test_db_path("ctox-phase1-stranded-send-attempt");
    let conn = open_channel_db(&db_path).expect("failed to open db");
    let request = phase1_test_request("Hallo Jill,\n\nEGRESS-2 crash-window body.\n\nGruesse");
    let body_sha256 = sha256_hex(request.body.trim().as_bytes());
    let message_key =
        record_outbound_pending_send(&conn, &request, "founder-review:egress2", &body_sha256)
            .expect("pending send must persist")
            .message_key;

    // Fresh pending row: a crash before send_cli is safe to retry.
    assert!(
        stranded_outbound_send_attempt(&conn, &message_key)
            .expect("stranded probe")
            .is_none(),
        "a pending row whose provider call has not started is not stranded"
    );

    // Provider send initiated, then crash before the accepted-mark.
    mark_outbound_send_attempt_started(&conn, &message_key).expect("mark attempt");
    assert!(
        stranded_outbound_send_attempt(&conn, &message_key)
            .expect("stranded probe")
            .is_some(),
        "a row whose provider call was initiated but not accepted is stranded"
    );

    // Acceptance makes the row durable; the durable-result dedupe owns it.
    mark_outbound_send_accepted(&conn, &message_key, "accepted", &json!({ "ok": true }))
        .expect("mark accepted");
    assert!(
        stranded_outbound_send_attempt(&conn, &message_key)
            .expect("stranded probe")
            .is_none(),
        "an accepted (durable) row is not stranded"
    );

    // A genuinely-failed send (adapter.send_cli returned Err = the provider
    // rejected it, NOT delivered) must stay retryable: mark_outbound_send_failed
    // clears the marker and a send_failed row is never treated as stranded.
    let failed_request =
        phase1_test_request("Hallo Jill,\n\nEGRESS-2 provider-rejected body.\n\nGruesse");
    let failed_body_sha256 = sha256_hex(failed_request.body.trim().as_bytes());
    let failed_key = record_outbound_pending_send(
        &conn,
        &failed_request,
        "founder-review:egress2-failed",
        &failed_body_sha256,
    )
    .expect("pending send must persist")
    .message_key;
    mark_outbound_send_attempt_started(&conn, &failed_key).expect("mark attempt");
    mark_outbound_send_failed(&conn, &failed_key, "smtp 550 rejected").expect("mark failed");
    assert!(
        stranded_outbound_send_attempt(&conn, &failed_key)
            .expect("stranded probe")
            .is_none(),
        "a send_failed row (provider rejected, never delivered) must stay retryable"
    );

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn phase1_record_outbound_pending_send_is_idempotent_for_retry() {
    let db_path = unique_test_db_path("ctox-phase1-retry-idempotent");
    let conn = open_channel_db(&db_path).expect("failed to open db");
    let request = phase1_test_request("Wiederholung");
    let body_sha256 = sha256_hex(request.body.trim().as_bytes());

    let key_first =
        record_outbound_pending_send(&conn, &request, "founder-review:phase1", &body_sha256)
            .expect("first persist")
            .message_key;
    let key_second =
        record_outbound_pending_send(&conn, &request, "founder-review:phase1", &body_sha256)
            .expect("second persist (retry-style) must not crash")
            .message_key;
    assert_eq!(
        key_first, key_second,
        "retrying record_outbound_pending_send must yield the same key (idempotent upsert)"
    );

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM communication_messages WHERE message_key = ?1",
            params![key_first],
            |row| row.get(0),
        )
        .expect("count query");
    assert_eq!(
        count, 1,
        "exactly one durable row must exist for the retry-bound key"
    );

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn phase1_record_outbound_pending_send_does_not_reset_existing_sent_row() {
    let db_path = unique_test_db_path("ctox-phase1-retry-existing-sent");
    let conn = open_channel_db(&db_path).expect("failed to open db");
    let request = phase1_test_request("Bereits versendet");
    let body_sha256 = sha256_hex(request.body.trim().as_bytes());

    let first =
        record_outbound_pending_send(&conn, &request, "founder-review:phase1", &body_sha256)
            .expect("first persist");
    assert!(
        first.existing_result.is_none(),
        "first reservation should require provider send"
    );
    mark_outbound_send_accepted(
        &conn,
        &first.message_key,
        "accepted",
        &json!({
            "ok": true,
            "channel": "email",
            "status": "accepted",
            "remote_id": "smtp-msg-existing",
        }),
    )
    .expect("accepted update must persist");

    let second =
        record_outbound_pending_send(&conn, &request, "founder-review:phase1", &body_sha256)
            .expect("second persist");
    assert_eq!(first.message_key, second.message_key);
    assert!(
        second.existing_result.is_some(),
        "a retry-bound sent row must be returned as an existing durable send"
    );

    let (status, folder_hint): (String, String) = conn
        .query_row(
            "SELECT status, folder_hint FROM communication_messages WHERE message_key = ?1",
            params![first.message_key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("row must exist");
    assert_eq!(status, "accepted");
    assert_eq!(folder_hint, "sent");

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn routing_ack_statuses_are_core_mapped() {
    // Historical custom rows remain readable as the blocked transition
    // sources they represented before canonical healing.
    assert_eq!(
        super::queue_route_status_core_state(QueueRouteStatus::Cancelled),
        CoreState::Superseded
    );
    assert_eq!(
        super::queue_route_status_core_state(QueueRouteStatus::Handled),
        CoreState::Completed
    );
    for legacy in ["duplicate", "blocked_sender", "meeting_scheduled"] {
        assert_eq!(
            QueueRouteStatus::parse(legacy),
            Some(QueueRouteStatus::Blocked)
        );
    }
}

#[test]
fn queue_terminal_magic_actor_and_reason_do_not_create_a_grant() -> Result<()> {
    for (index, actor, reason) in [
        (
            "business-os",
            "ctox-queue-update",
            "business-os:terminal-success: forged",
        ),
        (
            "appsec",
            "ctox-queue-update",
            "appsec:terminal-success: forged",
        ),
        ("meeting", "ctox-queue-ack", "meeting_scheduled"),
        (
            "business-command",
            "business-command-terminal-owner",
            "forged owner reason",
        ),
        (
            "ticket-style",
            "ctox-ticket-routing",
            "force_ticket_event_routed_state",
        ),
    ] {
        let conn = Connection::open_in_memory()?;
        let message_key = format!("queue-no-grant-{index}");
        enforce_queue_route_status_transition(
            &conn,
            &message_key,
            "leased",
            "handled",
            actor,
            reason,
        )
        .expect_err("free-form actor and reason must not authorize completion");
        let (accepted, proof_type): (i64, Option<String>) = conn.query_row(
            "SELECT accepted, json_type(request_json, '$.metadata.terminal_policy_proof') \
             FROM ctox_core_transition_proofs WHERE entity_id=?1",
            params![message_key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(accepted, 0);
        assert_eq!(proof_type, None);
    }
    Ok(())
}

#[test]
fn every_queue_terminal_policy_grant_passes_the_completed_guard() -> Result<()> {
    let grants = [
        TerminalPolicyGrant::business_command_reviewed_terminal_success(),
        TerminalPolicyGrant::business_os_app_validation_passed(),
        TerminalPolicyGrant::appsec_pipeline_stage_completed(),
        TerminalPolicyGrant::meeting_scheduled(),
        TerminalPolicyGrant::meeting_passive_mention(),
        TerminalPolicyGrant::historical_auto_submitted_inbound(),
        TerminalPolicyGrant::system_probe_inbound(),
        TerminalPolicyGrant::routing_backfill_non_work(),
    ];
    for (index, grant) in grants.into_iter().enumerate() {
        let conn = Connection::open_in_memory()?;
        let message_key = format!("queue-grant-{index}");
        enforce_queue_route_status_transition_with_grant(
            &conn,
            &message_key,
            "leased",
            "handled",
            "audit-actor",
            "audit reason only",
            Some(grant),
        )?;
        assert!(queue_completed_has_terminal_success_proof(
            &conn,
            &message_key
        )?);
        let persisted_proof: String = conn.query_row(
            "SELECT json_extract(request_json, '$.metadata.terminal_policy_proof') \
             FROM ctox_core_transition_proofs \
             WHERE entity_type='QueueItem' AND entity_id=?1 AND accepted=1",
            params![message_key],
            |row| row.get(0),
        )?;
        assert_eq!(persisted_proof, grant.proof());
    }
    Ok(())
}

fn insert_legacy_queue_transition_proof(
    conn: &Connection,
    proof_id: &str,
    message_key: &str,
    request_json: &str,
) -> Result<()> {
    ensure_core_transition_guard_schema(conn)?;
    conn.execute(
        r#"
        INSERT INTO ctox_core_transition_proofs (
            proof_id, entity_type, entity_id, lane, from_state, to_state,
            core_event, actor, accepted, violation_codes_json, request_json,
            report_json, created_at, updated_at
        ) VALUES (
            ?1, 'QueueItem', ?2, 'P2MissionDelivery', 'Leased', 'Completed',
            'Complete', 'legacy-test', 1, '[]', ?3, '{}',
            '2026-07-01T00:00:00Z', '2026-07-01T00:00:00Z'
        )
        "#,
        params![proof_id, message_key, request_json],
    )?;
    Ok(())
}

#[test]
fn queue_proof_lookup_uses_structured_json_and_keeps_legacy_rows() -> Result<()> {
    let conn = Connection::open_in_memory()?;
    insert_legacy_queue_transition_proof(
        &conn,
        "legacy-random-substring",
        "queue-random-substring",
        r#"{"metadata":{"unrelated":"contains \"terminal_policy_proof\" but is not proof metadata"}}"#,
    )?;
    assert!(!queue_completed_has_terminal_success_proof(
        &conn,
        "queue-random-substring"
    )?);

    insert_legacy_queue_transition_proof(
        &conn,
        "legacy-terminal-policy",
        "queue-legacy-policy",
        r#"{
            "metadata": {
                "terminal_policy_proof": "policy:legacy-before-structured-reader"
            }
        }"#,
    )?;
    assert!(queue_completed_has_terminal_success_proof(
        &conn,
        "queue-legacy-policy"
    )?);

    insert_legacy_queue_transition_proof(
        &conn,
        "legacy-reviewed-success",
        "queue-legacy-reviewed",
        r#"{"metadata":{"reviewed_work_terminal_success":"true"}}"#,
    )?;
    assert!(queue_completed_has_terminal_success_proof(
        &conn,
        "queue-legacy-reviewed"
    )?);
    Ok(())
}

fn business_command_test_root(prefix: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "{prefix}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create business command test root");
    root
}

fn business_command_claim(command_id: &str, payload_hash: &str) -> BusinessCommandClaimRequest {
    BusinessCommandClaimRequest {
        command_id: command_id.to_string(),
        idempotency_key: command_id.to_string(),
        payload_hash: payload_hash.to_string(),
        module: "tests".to_string(),
        command_type: "tests.command".to_string(),
        record_id: "record-1".to_string(),
        intent: json!({"command_id": command_id, "payload": {"value": 1}}),
        created_at_ms: 1_700_000_000_000,
    }
}

#[test]
fn business_command_queue_claim_is_atomic_and_idempotent() {
    let root = business_command_test_root("ctox-business-command-queue-claim");
    let request = || QueueTaskCreateRequest {
        title: "Atomic business command".to_string(),
        prompt: "Execute the command once.".to_string(),
        thread_key: "business-os/tests/atomic".to_string(),
        workspace_root: Some(root.display().to_string()),
        priority: "normal".to_string(),
        suggested_skill: None,
        parent_message_key: None,
        extra_metadata: Some(json!({"idempotency_key": "command-atomic-1"})),
    };

    let first = claim_business_command_with_queue(
        &root,
        business_command_claim("command-atomic-1", "sha256:first"),
        request(),
    )
    .expect("first atomic queue claim");
    assert!(!first.already_claimed);
    let retry = claim_business_command_with_queue(
        &root,
        business_command_claim("command-atomic-1", "sha256:first"),
        request(),
    )
    .expect("idempotent queue claim retry");
    assert!(retry.already_claimed);
    assert_eq!(first.task.message_key, retry.task.message_key);

    let conflict = claim_business_command_with_queue(
        &root,
        business_command_claim("command-atomic-1", "sha256:changed"),
        request(),
    )
    .expect_err("changed immutable intent must conflict");
    assert!(conflict.to_string().contains("idempotency_conflict"));

    let conn = open_channel_db(&resolve_db_path(&root, None)).expect("reopen core db");
    let counts: (i64, i64, i64, i64) = conn
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM business_command_aggregates WHERE command_id = 'command-atomic-1'),
                (SELECT COUNT(*) FROM business_command_task_links WHERE command_id = 'command-atomic-1'),
                (SELECT COUNT(*) FROM business_command_transitions WHERE command_id = 'command-atomic-1'),
                (SELECT COUNT(*) FROM business_command_outbox WHERE command_id = 'command-atomic-1')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("count atomic command rows");
    assert_eq!(counts, (1, 1, 2, 4));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn adapter_reconciliation_keeps_one_open_task_per_configuration() -> Result<()> {
    let root = tempfile::tempdir()?;
    let claim = |id: &str, digest: &str| {
        let mut claim = business_command_claim(id, id);
        claim.command_type = "outbound.research.adapters.reconcile".into();
        claim.intent["payload"] = json!({"configuration_digest":digest});
        claim
    };
    let request = |id: &str| QueueTaskCreateRequest {
        title: "Adapterabgleich".into(),
        prompt: "Reconcile the adapters".into(),
        thread_key: format!("adapter/{id}"),
        workspace_root: None,
        priority: "low".into(),
        suggested_skill: None,
        parent_message_key: None,
        extra_metadata: Some(json!({"idempotency_key":id})),
    };
    let first = claim_business_command_with_queue(
        root.path(),
        claim("adapter-first", "v1"),
        request("adapter-first"),
    )?;
    lease_queue_task(root.path(), &first.task.message_key, "worker")?;
    for n in 0..10 {
        let id = format!("adapter-duplicate-{n}");
        let duplicate =
            claim_business_command_with_queue(root.path(), claim(&id, "v1"), request(&id))?;
        assert_eq!(duplicate.task.route_status, "cancelled");
        let projection = business_command_projection(root.path(), &id)?;
        assert_eq!(projection["terminal_status"], "cancelled");
        assert_eq!(
            projection["result"]["superseded_by_task_id"],
            first.task.message_key
        );
    }
    let changed = claim_business_command_with_queue(
        root.path(),
        claim("adapter-new", "v2"),
        request("adapter-new"),
    )?;
    assert_eq!(changed.task.route_status, "pending");
    let conn = open_channel_db(&resolve_db_path(root.path(), None))?;
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM communication_routing_state WHERE route_status IN ('pending','leased')", [], |r|r.get(0))?;
    assert_eq!(count, 2);
    let plan: Vec<String> = conn.prepare("EXPLAIN QUERY PLAN SELECT command_id FROM business_command_aggregates
        WHERE command_type='outbound.research.adapters.reconcile' AND execution_phase!='terminal'
          AND module=?1 AND record_id=?2 AND json_extract(intent_json,'$.payload.configuration_digest')=?3
        ORDER BY created_at_ms,command_id LIMIT 1")?
        .query_map(params!["tests","record-1","v1"], |r|r.get(3))?.collect::<rusqlite::Result<_>>()?;
    assert!(
        plan.iter()
            .any(|line| line.contains("idx_active_adapter_reconciliation")),
        "{plan:?}"
    );
    let replay = claim_business_command_with_queue(
        root.path(),
        claim("adapter-duplicate-0", "v1"),
        request("adapter-duplicate-0"),
    )?;
    assert_eq!(
        replay.task.route_status, "cancelled",
        "idempotent replay must not revive duplicate work"
    );
    let mut other_actor = claim("adapter-other-actor", "v1");
    other_actor.intent["native_authorization"] = json!({"actor":{"user_id":"other"}});
    let other = claim_business_command_with_queue(
        root.path(),
        other_actor,
        request("adapter-other-actor"),
    )?;
    assert_eq!(
        other.task.route_status, "pending",
        "different authority must not share work"
    );
    let mut other_context = claim("adapter-other-context", "v1");
    other_context.intent["client_context"] = json!({"source":"other-surface"});
    let other = claim_business_command_with_queue(
        root.path(),
        other_context,
        request("adapter-other-context"),
    )?;
    assert_eq!(
        other.task.route_status, "pending",
        "different audited context must not share work"
    );
    transition_business_command_for_task(
        root.path(),
        &first.task.message_key,
        "cancelled",
        None,
        None,
        None,
        "fixture cancels original",
    )?;
    let fresh = claim_business_command_with_queue(
        root.path(),
        claim("adapter-after-terminal", "v1"),
        request("adapter-after-terminal"),
    )?;
    assert_eq!(
        fresh.task.route_status, "pending",
        "a completed reconciliation must not suppress future work"
    );
    Ok(())
}

#[test]
fn business_control_claim_suppresses_uncertain_replay_and_returns_terminal_result() {
    let root = business_command_test_root("ctox-business-command-control-claim");
    let first = claim_business_control_command(
        &root,
        business_command_claim("command-control-1", "sha256:control"),
    )
    .expect("first control claim");
    assert_eq!(first.disposition, "new");

    let uncertain = claim_business_control_command(
        &root,
        business_command_claim("command-control-1", "sha256:control"),
    )
    .expect("retry before outcome");
    assert_eq!(uncertain.disposition, "uncertain");

    complete_business_control_command(
        &root,
        "command-control-1",
        "completed",
        &json!({"ok": true, "value": 42}),
        None,
    )
    .expect("persist control outcome");
    let terminal = claim_business_control_command(
        &root,
        business_command_claim("command-control-1", "sha256:control"),
    )
    .expect("terminal retry");
    assert_eq!(terminal.disposition, "terminal");
    assert_eq!(terminal.terminal_status.as_deref(), Some("completed"));
    assert_eq!(terminal.result, Some(json!({"ok": true, "value": 42})));

    let conn = open_channel_db(&resolve_db_path(&root, None)).expect("reopen core db");
    let counts: (i64, i64, i64) = conn
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM business_command_effects WHERE command_id = 'command-control-1'),
                (SELECT COUNT(*) FROM business_command_transitions WHERE command_id = 'command-control-1'),
                (SELECT COUNT(*) FROM business_command_outbox WHERE command_id = 'command-control-1')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("count control command rows");
    assert_eq!(counts, (1, 2, 4));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn business_control_progress_is_durable_and_idempotent() {
    let root = business_command_test_root("ctox-business-command-control-progress");
    claim_business_control_command(
        &root,
        business_command_claim("command-control-progress-1", "sha256:control-progress"),
    )
    .expect("claim control command");

    progress_business_control_command(
        &root,
        "command-control-progress-1",
        "running",
        &json!({"ok": true, "status": "running"}),
    )
    .expect("persist control progress");
    progress_business_control_command(
        &root,
        "command-control-progress-1",
        "running",
        &json!({"ok": true, "status": "running"}),
    )
    .expect("repeat control progress");

    let projection = business_command_projection(&root, "command-control-progress-1")
        .expect("project running control command");
    assert_eq!(projection["execution_phase"], "running");
    assert_eq!(projection["task_status"], "running");
    assert_eq!(projection["attempt"], 1);
    let conn = open_channel_db(&resolve_db_path(&root, None)).expect("reopen core db");
    let transitions: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM business_command_transitions
             WHERE command_id = 'command-control-progress-1'",
            [],
            |row| row.get(0),
        )
        .expect("count progress transitions");
    assert_eq!(transitions, 2);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn registered_saga_blocks_premature_success_and_persists_compensation() {
    let root = business_command_test_root("ctox-business-command-saga");
    let mut claim = business_command_claim("command-saga-1", "sha256:saga");
    claim.command_type = "ctox.module.set_visible".to_string();
    claim_business_control_command(&root, claim).expect("claim control command");
    start_business_command_saga(&root, "command-saga-1", "ctox.module.set_visible")
        .expect("start registered saga");

    let premature = complete_business_control_command(
        &root,
        "command-saga-1",
        "completed",
        &json!({"ok": true}),
        None,
    )
    .expect_err("terminal success before saga completion must fail");
    assert!(premature.to_string().contains("terminal success rejected"));

    assert!(
        claim_business_command_saga_step(&root, "command-saga-1", "persist_visibility", false,)
            .expect("claim first step")
    );
    complete_business_command_saga_step(
        &root,
        "command-saga-1",
        "persist_visibility",
        false,
        &json!({"previous_visible": false}),
    )
    .expect("complete first step");
    claim_business_command_saga_step(&root, "command-saga-1", "project_catalog", false)
        .expect("claim second step");
    fail_business_command_saga_step(
        &root,
        "command-saga-1",
        "project_catalog",
        "projection failed",
        false,
    )
    .expect("fail second step");
    assert!(
        claim_business_command_saga_step(&root, "command-saga-1", "persist_visibility", true,)
            .expect("claim compensation")
    );
    complete_business_command_saga_step(
        &root,
        "command-saga-1",
        "persist_visibility",
        true,
        &json!({"restored_visible": false}),
    )
    .expect("complete compensation");
    complete_business_control_command(
        &root,
        "command-saga-1",
        "failed",
        &json!({"ok": false}),
        Some("projection failed"),
    )
    .expect("terminal failure after compensation");
    let projection =
        business_command_projection(&root, "command-saga-1").expect("project saga fields");
    assert_eq!(projection["saga_phase"], "compensated");
    assert_eq!(projection["compensation_status"], "completed");
    assert_eq!(projection["saga_total_steps"], 2);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_action_saga_snapshots_definition_and_derives_effect_keys() {
    let root = business_command_test_root("ctox-runtime-action-saga");
    let mut claim = business_command_claim("command-runtime-action-1", "sha256:runtime-action");
    claim.command_type = "ctox.app.action.run".to_string();
    claim_business_control_command(&root, claim).expect("claim runtime action");
    let snapshot = json!({
        "module_id": "workbench",
        "action_name": "approve",
        "definition_hash": "sha256:def",
        "definition": { "steps": [{ "op": "patch" }, { "op": "insert" }] },
    });
    start_runtime_business_command_saga(
        &root,
        "command-runtime-action-1",
        "workbench",
        "approve",
        "sha256:def",
        &snapshot,
        &["patch_record".to_string(), "write_audit".to_string()],
    )
    .expect("register dynamic saga");
    assert_eq!(
        runtime_business_command_action_snapshot(&root, "command-runtime-action-1")
            .expect("load snapshot"),
        Some(snapshot.clone())
    );
    // Re-registration with identical immutable input is an idempotent crash replay.
    start_runtime_business_command_saga(
        &root,
        "command-runtime-action-1",
        "workbench",
        "approve",
        "sha256:def",
        &snapshot,
        &["patch_record".to_string(), "write_audit".to_string()],
    )
    .expect("replay registration");
    let conn = open_channel_db(&resolve_db_path(&root, None)).expect("open core db");
    let keys: (String, String, i64) = conn
        .query_row(
            "SELECT forward_effect_key, compensation_effect_key,
                    (SELECT COUNT(*) FROM business_command_saga_steps WHERE saga_id = 'saga:command-runtime-action-1')
             FROM business_command_saga_steps
             WHERE saga_id = 'saga:command-runtime-action-1' AND step_index = 0",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("runtime effect keys");
    assert_eq!(keys.0, "command-runtime-action-1:sha256:def:0:forward");
    assert_eq!(keys.1, "command-runtime-action-1:sha256:def:0:compensation");
    assert_eq!(keys.2, 2);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn registered_saga_replays_claimed_effect_and_keeps_failed_compensation_visible() {
    let root = business_command_test_root("ctox-business-command-saga-crash-replay");
    let mut claim = business_command_claim("command-saga-crash", "sha256:saga-crash");
    claim.command_type = "ctox.module.set_visible".to_string();
    claim_business_control_command(&root, claim).expect("claim control command");
    start_business_command_saga(&root, "command-saga-crash", "ctox.module.set_visible")
        .expect("start saga");

    let out_of_order =
        claim_business_command_saga_step(&root, "command-saga-crash", "project_catalog", false)
            .expect_err("second forward effect must wait for the first");
    assert!(out_of_order.to_string().contains("registered order"));

    assert!(claim_business_command_saga_step(
        &root,
        "command-saga-crash",
        "persist_visibility",
        false,
    )
    .expect("claim first forward effect"));
    let evidence = json!({"module_id": "notes", "previous_visible": false, "visible": true});
    record_business_command_saga_step_evidence(
        &root,
        "command-saga-crash",
        "persist_visibility",
        &evidence,
    )
    .expect("persist pre-effect evidence");

    // Simulate a process restart after the effect evidence commit but
    // before the step completion commit. Registration is idempotent and
    // the same step is re-claimable with its evidence intact.
    start_business_command_saga(&root, "command-saga-crash", "ctox.module.set_visible")
        .expect("restart saga registration");
    assert_eq!(
        business_command_saga_step_evidence(&root, "command-saga-crash", "persist_visibility",)
            .expect("recover effect evidence"),
        evidence,
    );
    assert!(claim_business_command_saga_step(
        &root,
        "command-saga-crash",
        "persist_visibility",
        false,
    )
    .expect("reclaim interrupted effect"));
    complete_business_command_saga_step(
        &root,
        "command-saga-crash",
        "persist_visibility",
        false,
        &evidence,
    )
    .expect("complete replayed effect");
    assert!(!claim_business_command_saga_step(
        &root,
        "command-saga-crash",
        "persist_visibility",
        false,
    )
    .expect("completed effect is idempotent"));

    claim_business_command_saga_step(&root, "command-saga-crash", "project_catalog", false)
        .expect("claim projection effect");
    fail_business_command_saga_step(
        &root,
        "command-saga-crash",
        "project_catalog",
        "projection failed",
        false,
    )
    .expect("start compensation");
    claim_business_command_saga_step(&root, "command-saga-crash", "persist_visibility", true)
        .expect("claim compensation");
    fail_business_command_saga_step(
        &root,
        "command-saga-crash",
        "persist_visibility",
        "visibility restore failed",
        true,
    )
    .expect("persist failed compensation");

    let premature_success = complete_business_control_command(
        &root,
        "command-saga-crash",
        "completed",
        &json!({"ok": true}),
        None,
    )
    .expect_err("manual intervention saga must never report success");
    assert!(premature_success
        .to_string()
        .contains("terminal success rejected"));
    complete_business_control_command(
        &root,
        "command-saga-crash",
        "failed",
        &json!({"ok": false, "code": "saga_compensation_failed"}),
        Some("saga_compensation_failed"),
    )
    .expect("terminal failure retains manual intervention evidence");
    let projection = business_command_projection(&root, "command-saga-crash")
        .expect("project failed compensation");
    assert_eq!(projection["saga_phase"], "manual_intervention");
    assert_eq!(projection["compensation_status"], "failed");

    let conn = open_channel_db(&resolve_db_path(&root, None)).expect("open saga database");
    let attempts: i64 = conn
        .query_row(
            "SELECT forward_attempts FROM business_command_saga_steps WHERE saga_id = 'saga:command-saga-crash' AND step_name = 'persist_visibility'",
            [],
            |row| row.get(0),
        )
        .expect("read durable retry count");
    assert_eq!(
        attempts, 2,
        "crash replay must be visible as a second attempt"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn business_command_cannot_complete_before_typed_result_review_and_validation() {
    let root = business_command_test_root("ctox-business-command-terminal-gate");
    let claimed = claim_business_command_with_queue(
        &root,
        business_command_claim("command-terminal-gate", "sha256:terminal-gate"),
        QueueTaskCreateRequest {
            title: "Terminal gate".to_string(),
            prompt: "Prove terminal ordering.".to_string(),
            thread_key: "business-os/tests/terminal-gate".to_string(),
            workspace_root: Some(root.display().to_string()),
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: Some(json!({"idempotency_key": "command-terminal-gate"})),
        },
    )
    .expect("claim command");
    let task_id = claimed.task.message_key;
    lease_queue_task(&root, &task_id, "ctox-test").expect("lease queue task");
    transition_business_command_for_task(
        &root,
        &task_id,
        "leased",
        None,
        None,
        None,
        "worker leased",
    )
    .expect("lease command");
    transition_business_command_for_task(
        &root,
        &task_id,
        "running",
        None,
        None,
        None,
        "worker started",
    )
    .expect("start command");
    persist_business_command_worker_result(&root, &task_id, "done").expect("persist typed result");
    let persisted = business_command_projection(&root, "command-terminal-gate")
        .expect("load persisted result envelope");
    assert_eq!(persisted["result"]["command_id"], "command-terminal-gate");
    assert_eq!(persisted["result"]["execution_task_id"], task_id);
    assert_eq!(persisted["result"]["user_message"], "done");
    assert!(persisted["result"]["structured_output"].is_null());
    assert!(persisted["result"]["verification_claims"].is_array());

    let premature = transition_business_command_for_task(
        &root,
        &task_id,
        "handled",
        Some(&json!({"user_reply": "done"})),
        None,
        None,
        "assistant prose claimed completion",
    )
    .expect_err("completion before review must fail");
    assert!(premature
        .to_string()
        .contains("passed review and validation"));

    record_business_command_review(
        &root,
        &task_id,
        "passed",
        "passed",
        &json!({"review": "PASS", "validation": "PASS"}),
    )
    .expect("persist review");
    transition_business_command_for_task(
        &root,
        &task_id,
        "handled",
        Some(&json!({"user_reply": "done"})),
        None,
        None,
        "reviewed completion",
    )
    .expect("terminalize after evidence");
    let projection =
        business_command_projection(&root, "command-terminal-gate").expect("load projection");
    assert_eq!(projection["execution_phase"], "terminal");
    assert_eq!(projection["terminal_status"], "completed");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn failed_queue_ack_terminalizes_linked_business_command() {
    let root = business_command_test_root("ctox-business-command-failed-queue-ack");
    let claimed = claim_business_command_with_queue(
        &root,
        business_command_claim("command-failed-queue-ack", "sha256:failed-queue-ack"),
        QueueTaskCreateRequest {
            title: "Terminal failure projection".to_string(),
            prompt: "Fail the linked command with the queue task.".to_string(),
            thread_key: "business-os/tests/failed-queue-ack".to_string(),
            workspace_root: Some(root.display().to_string()),
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: Some(json!({"idempotency_key": "command-failed-queue-ack"})),
        },
    )
    .expect("claim command");
    let task_id = claimed.task.message_key;
    lease_queue_task(&root, &task_id, "ctox-test").expect("lease queue task");
    transition_business_command_for_task(
        &root,
        &task_id,
        "leased",
        None,
        None,
        None,
        "worker leased",
    )
    .expect("lease command");
    transition_business_command_for_task(
        &root,
        &task_id,
        "running",
        None,
        None,
        None,
        "worker started",
    )
    .expect("start command");

    ack_leased_messages_with_failure_reason(
        &root,
        std::slice::from_ref(&task_id),
        "failed",
        "finite review budget exhausted",
    )
    .expect("fail linked queue task and command atomically");

    let projection = business_command_projection(&root, "command-failed-queue-ack")
        .expect("load terminal command projection");
    assert_eq!(projection["execution_phase"], "terminal");
    assert_eq!(projection["terminal_status"], "failed");
    assert_eq!(projection["error_code"], "queue_terminal_failure");
    assert_eq!(
        projection["error_message"],
        "finite review budget exhausted"
    );
    let task = load_queue_task(&root, &task_id)
        .expect("load failed queue task")
        .expect("failed queue task remains inspectable");
    assert_eq!(task.route_status, "failed");
    assert_eq!(
        task.status_note.as_deref(),
        Some("finite review budget exhausted")
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn waiting_dependency_command_promotes_once_to_one_queue_task() {
    let root = business_command_test_root("ctox-business-command-dependency-promotion");
    let claim = business_command_claim("command-dependency", "sha256:dependency");
    claim_business_command_waiting_dependencies(
        &root,
        claim.clone(),
        &json!([{"collection": "desktop_files", "record_id": "file-1"}]),
    )
    .expect("record dependency wait");
    let request = || QueueTaskCreateRequest {
        title: "Dependency ready".to_string(),
        prompt: "Run after dependency materializes.".to_string(),
        thread_key: "business-os/tests/dependency".to_string(),
        workspace_root: Some(root.display().to_string()),
        priority: "normal".to_string(),
        suggested_skill: None,
        parent_message_key: None,
        extra_metadata: Some(json!({"idempotency_key": "command-dependency"})),
    };
    let promoted = claim_business_command_with_queue(&root, claim.clone(), request())
        .expect("promote dependency command");
    let replay = claim_business_command_with_queue(&root, claim, request())
        .expect("replay promoted command");
    assert_eq!(promoted.task.message_key, replay.task.message_key);
    let conn = open_channel_db(&resolve_db_path(&root, None)).expect("open core db");
    let counts: (i64, i64) = conn
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM business_command_task_links WHERE command_id = 'command-dependency'),
                (SELECT COUNT(*) FROM communication_messages WHERE message_key = ?1)",
            params![promoted.task.message_key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("count promoted rows");
    assert_eq!(counts, (1, 1));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn business_command_audit_does_not_recreate_retained_outbox_rows() {
    let root = business_command_test_root("ctox-business-command-retained-outbox");
    claim_business_control_command(
        &root,
        business_command_claim("command-retained-outbox", "sha256:retained-outbox"),
    )
    .expect("claim control command");
    let db_path = resolve_db_path(&root, None);
    let conn = open_channel_db(&db_path).expect("open core db");
    conn.execute(
        "UPDATE business_command_outbox
         SET status = 'delivered', delivered_at_ms = 1
         WHERE command_id = 'command-retained-outbox'",
        [],
    )
    .expect("age delivered outbox rows");
    drop(conn);

    let retention = business_command_retention_maintenance(&root, true).expect("prune outbox");
    assert_eq!(retention["pruned_delivered_outbox"], 2);
    let audited = audit_and_migrate_business_command_storage(&root, true).expect("audit storage");
    assert!(audited.get("missing_current_outbox").is_none());

    let conn = open_channel_db(&db_path).expect("reopen core db");
    let outbox_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM business_command_outbox
             WHERE command_id = 'command-retained-outbox'",
            [],
            |row| row.get(0),
        )
        .expect("count retained outbox rows");
    assert_eq!(outbox_count, 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn business_command_audit_resolves_only_proven_transient_intake_failures() {
    let root = business_command_test_root("ctox-business-command-transient-intake-reconcile");
    let original = business_command_claim("command-transient-intake", "sha256:original");
    claim_business_control_command(&root, original.clone()).expect("claim control command");
    let db_path = resolve_db_path(&root, None);
    let conn = open_channel_db(&db_path).expect("open core db");
    conn.execute(
        "UPDATE business_command_outbox
         SET status = 'delivered', delivered_at_ms = 1
         WHERE command_id = ?1",
        params![original.command_id],
    )
    .expect("mark canonical projections delivered");
    drop(conn);

    record_business_command_intake_failure(
        &root,
        original.clone(),
        "transient native store contention: database is locked",
        3,
    )
    .expect("record transient failure after canonical acceptance");
    let report =
        audit_and_migrate_business_command_storage(&root, false).expect("audit transient failure");
    assert_eq!(
        report["resolvable_transient_intake_failures"],
        json!([original.command_id.clone()])
    );
    assert_eq!(report["resolved_transient_intake_failures"], 0);

    let applied = audit_and_migrate_business_command_storage(&root, true)
        .expect("resolve proven transient failure");
    assert_eq!(applied["resolved_transient_intake_failures"], 1);
    let conn = open_channel_db(&db_path).expect("reopen core db");
    let open_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM business_command_intake_failures
             WHERE command_id = ?1 AND resolved_at_ms IS NULL",
            params![original.command_id],
            |row| row.get(0),
        )
        .expect("count open transient failures");
    assert_eq!(open_count, 0);

    let conflict_original =
        business_command_claim("command-idempotency-conflict", "sha256:original");
    claim_business_control_command(&root, conflict_original.clone())
        .expect("claim conflict baseline command");
    conn.execute(
        "UPDATE business_command_outbox
         SET status = 'delivered', delivered_at_ms = 1
         WHERE command_id = ?1",
        params![conflict_original.command_id],
    )
    .expect("mark conflict baseline projections delivered");
    let conflict = business_command_claim("command-idempotency-conflict", "sha256:conflict");
    record_business_command_intake_failure(
        &root,
        conflict,
        "business command idempotency conflict",
        3,
    )
    .expect("record conflicting replay");
    let conflict_report =
        audit_and_migrate_business_command_storage(&root, true).expect("audit conflicting replay");
    assert_eq!(
        conflict_report["resolvable_transient_intake_failures"],
        json!([])
    );
    let conflict_open_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM business_command_intake_failures
             WHERE command_id = ?1 AND resolved_at_ms IS NULL",
            params![conflict_original.command_id],
            |row| row.get(0),
        )
        .expect("count open conflict failures");
    assert_eq!(conflict_open_count, 1);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn cancelling_linked_queue_task_terminalizes_business_command() {
    let root = business_command_test_root("ctox-business-command-cancel");
    let claimed = claim_business_command_with_queue(
        &root,
        business_command_claim("command-cancel", "sha256:cancel"),
        QueueTaskCreateRequest {
            title: "Cancel linked work".to_string(),
            prompt: "This task will be cancelled.".to_string(),
            thread_key: "business-os/tests/cancel".to_string(),
            workspace_root: Some(root.display().to_string()),
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: Some(json!({"idempotency_key": "command-cancel"})),
        },
    )
    .expect("claim command");
    let task_id = claimed.task.message_key;
    lease_queue_task(&root, &task_id, "ctox-test").expect("lease queue task");
    transition_business_command_for_task(
        &root,
        &task_id,
        "leased",
        None,
        None,
        None,
        "initial lease",
    )
    .expect("lease command");
    transition_business_command_for_task(
        &root,
        &task_id,
        "running",
        None,
        None,
        None,
        "initial execution",
    )
    .expect("run command");

    let cancelled = update_queue_task(
        &root,
        QueueTaskUpdateRequest {
            message_key: task_id,
            route_status: Some("cancelled".to_string()),
            status_note: Some("operator cancelled stale work".to_string()),
            ..Default::default()
        },
    )
    .expect("cancel linked queue task");
    assert_eq!(cancelled.route_status, "cancelled");

    let projection = business_command_projection(&root, "command-cancel").expect("load projection");
    assert_eq!(projection["execution_phase"], "terminal");
    assert_eq!(projection["terminal_status"], "cancelled");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn cancelled_ack_terminalizes_linked_business_command_atomically() {
    let root = business_command_test_root("ctox-business-command-cancel-ack");
    let claimed = claim_business_command_with_queue(
        &root,
        business_command_claim("command-cancel-ack", "sha256:cancel-ack"),
        QueueTaskCreateRequest {
            title: "Cancel acknowledged work".to_string(),
            prompt: "This linked task will be cancelled through the ack path.".to_string(),
            thread_key: "business-os/tests/cancel-ack".to_string(),
            workspace_root: Some(root.display().to_string()),
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: Some(json!({"idempotency_key": "command-cancel-ack"})),
        },
    )
    .expect("claim command");
    let task_id = claimed.task.message_key;
    lease_queue_task(&root, &task_id, "ctox-test").expect("lease queue task");
    transition_business_command_for_task(
        &root,
        &task_id,
        "leased",
        None,
        None,
        None,
        "initial lease",
    )
    .expect("lease command");
    transition_business_command_for_task(
        &root,
        &task_id,
        "running",
        None,
        None,
        None,
        "initial execution",
    )
    .expect("run command");

    assert_eq!(
        ack_leased_messages(&root, std::slice::from_ref(&task_id), "cancelled")
            .expect("cancel through ack path"),
        1
    );
    let projection =
        business_command_projection(&root, "command-cancel-ack").expect("load projection");
    assert_eq!(projection["execution_phase"], "terminal");
    assert_eq!(projection["terminal_status"], "cancelled");
    assert_eq!(
        load_queue_task(&root, &task_id)
            .expect("load cancelled task")
            .expect("cancelled task exists")
            .route_status,
        "cancelled"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn legacy_cancelled_queue_command_migration_runs_once_per_database() {
    let root = business_command_test_root("ctox-business-command-reconcile-cancel");
    let claimed = claim_business_command_with_queue(
        &root,
        business_command_claim("command-reconcile-cancel", "sha256:reconcile-cancel"),
        QueueTaskCreateRequest {
            title: "Repair cancelled work".to_string(),
            prompt: "Simulate a legacy split-brain cancellation.".to_string(),
            thread_key: "business-os/tests/reconcile-cancel".to_string(),
            workspace_root: Some(root.display().to_string()),
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: Some(json!({"idempotency_key": "command-reconcile-cancel"})),
        },
    )
    .expect("claim command");
    let task_id = claimed.task.message_key;
    lease_queue_task(&root, &task_id, "ctox-test").expect("lease queue task");
    transition_business_command_for_task(
        &root,
        &task_id,
        "leased",
        None,
        None,
        None,
        "initial lease",
    )
    .expect("lease command");
    transition_business_command_for_task(
        &root,
        &task_id,
        "running",
        None,
        None,
        None,
        "initial execution",
    )
    .expect("run command");
    let conn = open_channel_db(&resolve_db_path(&root, None)).expect("open core db");
    conn.execute(
        "UPDATE communication_routing_state
         SET route_status = 'cancelled', lease_owner = NULL, leased_at = NULL,
             lease_expires_at = NULL
         WHERE message_key = ?1",
        params![task_id],
    )
    .expect("seed legacy cancelled route");
    drop(conn);

    let report = audit_and_migrate_business_command_storage(&root, false)
        .expect("report legacy cancelled rows");
    assert_eq!(
        report["cancelled_queue_command_drift"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    let applied = audit_and_migrate_business_command_storage(&root, true)
        .expect("migrate legacy cancelled rows");
    assert_eq!(applied["repaired_cancelled_queue_commands"], 1);
    assert_eq!(
        applied["legacy_cancelled_queue_command_migration"]["applied_now"],
        true
    );
    let projection = business_command_projection(&root, "command-reconcile-cancel")
        .expect("load reconciled projection");
    assert_eq!(projection["execution_phase"], "terminal");
    assert_eq!(projection["terminal_status"], "cancelled");
    let clean =
        audit_and_migrate_business_command_storage(&root, false).expect("verify migration marker");
    assert_eq!(
        clean["cancelled_queue_command_drift"]
            .as_array()
            .map(Vec::len),
        Some(0)
    );
    assert_eq!(
        clean["legacy_cancelled_queue_command_migration"]["already_applied"],
        true
    );
    let second_apply =
        audit_and_migrate_business_command_storage(&root, true).expect("skip completed migration");
    assert_eq!(second_apply["repaired_cancelled_queue_commands"], 0);
    assert_eq!(
        second_apply["legacy_cancelled_queue_command_migration"]["already_applied"],
        true
    );
    assert_eq!(
        second_apply["legacy_cancelled_queue_command_migration"]["applied_now"],
        false
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn reconciler_repairs_failed_queue_with_nonterminal_command() {
    let root = business_command_test_root("ctox-business-command-reconcile-failed");
    let claimed = claim_business_command_with_queue(
        &root,
        business_command_claim("command-reconcile-failed", "sha256:reconcile-failed"),
        QueueTaskCreateRequest {
            title: "Repair failed work".to_string(),
            prompt: "Simulate a legacy split-brain terminal failure.".to_string(),
            thread_key: "business-os/tests/reconcile-failed".to_string(),
            workspace_root: Some(root.display().to_string()),
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: Some(json!({"idempotency_key": "command-reconcile-failed"})),
        },
    )
    .expect("claim command");
    let task_id = claimed.task.message_key;
    lease_queue_task(&root, &task_id, "ctox-test").expect("lease queue task");
    transition_business_command_for_task(
        &root,
        &task_id,
        "leased",
        None,
        None,
        None,
        "initial lease",
    )
    .expect("lease command");
    transition_business_command_for_task(
        &root,
        &task_id,
        "running",
        None,
        None,
        None,
        "initial execution",
    )
    .expect("run command");
    let conn = open_channel_db(&resolve_db_path(&root, None)).expect("open core db");
    conn.execute(
        "UPDATE communication_routing_state
         SET route_status = 'failed', lease_owner = NULL, leased_at = NULL,
             lease_expires_at = NULL, last_error = 'finite review budget exhausted'
         WHERE message_key = ?1",
        params![task_id],
    )
    .expect("seed legacy failed route");
    drop(conn);

    let report = audit_and_migrate_business_command_storage(&root, false)
        .expect("report failed queue/command drift");
    assert_eq!(
        report["terminal_failure_queue_command_drift"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    let applied = audit_and_migrate_business_command_storage(&root, true)
        .expect("repair failed queue/command drift");
    assert_eq!(applied["repaired_terminal_failure_queue_commands"], 1);
    assert_eq!(
        applied["terminal_failure_queue_command_drift"]
            .as_array()
            .map(Vec::len),
        Some(0)
    );
    let projection = business_command_projection(&root, "command-reconcile-failed")
        .expect("load reconciled failed command");
    assert_eq!(projection["execution_phase"], "terminal");
    assert_eq!(projection["terminal_status"], "failed");
    assert_eq!(
        projection["error_code"],
        "queue_terminal_failure_reconciled"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn review_rework_reuses_task_and_increments_attempt() {
    let root = business_command_test_root("ctox-business-command-rework-attempt");
    let claimed = claim_business_command_with_queue(
        &root,
        business_command_claim("command-rework", "sha256:rework"),
        QueueTaskCreateRequest {
            title: "Rework continuity".to_string(),
            prompt: "Keep this command and task.".to_string(),
            thread_key: "business-os/tests/rework".to_string(),
            workspace_root: Some(root.display().to_string()),
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: Some(json!({"idempotency_key": "command-rework"})),
        },
    )
    .expect("claim command");
    let task_id = claimed.task.message_key;
    lease_queue_task(&root, &task_id, "ctox-test").expect("lease first queue attempt");
    transition_business_command_for_task(
        &root,
        &task_id,
        "leased",
        None,
        None,
        None,
        "attempt one lease",
    )
    .expect("lease first attempt");
    transition_business_command_for_task(
        &root,
        &task_id,
        "running",
        None,
        None,
        None,
        "attempt one",
    )
    .expect("start first attempt");
    persist_business_command_worker_result(&root, &task_id, "first answer")
        .expect("persist first result");
    record_business_command_review(
        &root,
        &task_id,
        "failed",
        "pending",
        &json!({"feedback": "fix it"}),
    )
    .expect("record rework");
    ack_leased_messages(&root, std::slice::from_ref(&task_id), "pending")
        .expect("release first queue attempt");
    lease_queue_task(&root, &task_id, "ctox-test").expect("lease second queue attempt");
    transition_business_command_for_task(
        &root,
        &task_id,
        "leased",
        None,
        None,
        None,
        "attempt two lease",
    )
    .expect("lease second attempt");
    transition_business_command_for_task(
        &root,
        &task_id,
        "running",
        None,
        None,
        None,
        "attempt two",
    )
    .expect("start second attempt");
    persist_business_command_worker_result(&root, &task_id, "second answer")
        .expect("persist second result");
    let projection = business_command_projection(&root, "command-rework").expect("load projection");
    assert_eq!(projection["attempt"], 2);
    let conn = open_channel_db(&resolve_db_path(&root, None)).expect("open core db");
    let result_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM business_command_results WHERE command_id = 'command-rework'",
            [],
            |row| row.get(0),
        )
        .expect("count attempt results");
    assert_eq!(result_count, 2);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn releasing_failed_queue_task_moves_running_command_to_retry_wait() {
    let root = business_command_test_root("ctox-business-command-manual-release");
    let claimed = claim_business_command_with_queue(
        &root,
        business_command_claim("command-manual-release", "sha256:manual-release"),
        QueueTaskCreateRequest {
            title: "Recover interrupted command".to_string(),
            prompt: "Continue the interrupted command.".to_string(),
            thread_key: "business-os/tests/manual-release".to_string(),
            workspace_root: Some(root.display().to_string()),
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: Some(json!({"idempotency_key": "command-manual-release"})),
        },
    )
    .expect("claim command");
    let task_id = claimed.task.message_key;
    lease_queue_task(&root, &task_id, "ctox-test").expect("lease queue task");
    transition_business_command_for_task(
        &root,
        &task_id,
        "leased",
        None,
        None,
        None,
        "initial lease",
    )
    .expect("lease command");
    transition_business_command_for_task(
        &root,
        &task_id,
        "running",
        None,
        None,
        None,
        "initial execution",
    )
    .expect("run command");

    update_queue_task(
        &root,
        QueueTaskUpdateRequest {
            message_key: task_id.clone(),
            route_status: Some("failed".to_string()),
            status_note: Some("worker interrupted before lifecycle cleanup".to_string()),
            ..Default::default()
        },
    )
    .expect("record interrupted queue route");
    let released = update_queue_task(
        &root,
        QueueTaskUpdateRequest {
            message_key: task_id.clone(),
            route_status: Some("pending".to_string()),
            status_note: Some("operator retry".to_string()),
            ..Default::default()
        },
    )
    .expect("release interrupted command");
    assert_eq!(released.route_status, "pending");
    let projection = business_command_projection(&root, "command-manual-release")
        .expect("load released command projection");
    assert_eq!(projection["execution_phase"], "retry_wait");

    lease_queue_task(&root, &task_id, "ctox-test").expect("lease retry");
    transition_business_command_for_task(
        &root,
        &task_id,
        "leased",
        None,
        None,
        None,
        "retry lease",
    )
    .expect("retry_wait must transition to leased");
    let projection = business_command_projection(&root, "command-manual-release")
        .expect("load retried command projection");
    assert_eq!(projection["execution_phase"], "leased");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn unavailable_review_waits_for_retry_without_blocking_command() {
    let root = business_command_test_root("ctox-business-command-review-unavailable");
    let claimed = claim_business_command_with_queue(
        &root,
        business_command_claim("command-review-unavailable", "sha256:review-unavailable"),
        QueueTaskCreateRequest {
            title: "Retry unavailable review".to_string(),
            prompt: "Answer from supplied facts.".to_string(),
            thread_key: "business-os/tests/review-unavailable".to_string(),
            workspace_root: Some(root.display().to_string()),
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: Some(json!({"idempotency_key": "command-review-unavailable"})),
        },
    )
    .expect("claim command");
    let task_id = claimed.task.message_key;
    lease_queue_task(&root, &task_id, "ctox-test").expect("lease first queue attempt");
    transition_business_command_for_task(
        &root,
        &task_id,
        "leased",
        None,
        None,
        None,
        "attempt one lease",
    )
    .expect("lease command");
    transition_business_command_for_task(
        &root,
        &task_id,
        "running",
        None,
        None,
        None,
        "attempt one",
    )
    .expect("run command");
    persist_business_command_worker_result(&root, &task_id, "correct answer")
        .expect("persist typed result");
    record_business_command_review(
        &root,
        &task_id,
        "held",
        "pending",
        &json!({"retryable_hold": true, "reason": "reviewer unavailable"}),
    )
    .expect("record retryable review hold");

    let projection =
        business_command_projection(&root, "command-review-unavailable").expect("load projection");
    assert_eq!(projection["execution_phase"], "retry_wait");
    assert_eq!(projection["terminal_status"], "none");
    assert_eq!(projection["retryable"], true);

    ack_leased_messages(&root, std::slice::from_ref(&task_id), "pending")
        .expect("release held queue attempt");
    lease_queue_task(&root, &task_id, "ctox-test").expect("lease review retry queue attempt");
    transition_business_command_for_task(
        &root,
        &task_id,
        "leased",
        None,
        None,
        None,
        "review retry lease",
    )
    .expect("retry_wait must be leasable");
    let projection = business_command_projection(&root, "command-review-unavailable")
        .expect("reload projection");
    assert_eq!(projection["execution_phase"], "leased");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn typed_result_persistence_hold_retries_command_and_queue_together() {
    let root = business_command_test_root("ctox-business-command-result-hold");
    let claimed = claim_business_command_with_queue(
        &root,
        business_command_claim("command-result-hold", "sha256:result-hold"),
        QueueTaskCreateRequest {
            title: "Retry typed result persistence".to_string(),
            prompt: "Return a small supplied-fact answer.".to_string(),
            thread_key: "business-os/tests/result-hold".to_string(),
            workspace_root: Some(root.display().to_string()),
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: Some(json!({"idempotency_key": "command-result-hold"})),
        },
    )
    .expect("claim command");
    let task_id = claimed.task.message_key;
    lease_queue_task(&root, &task_id, "ctox-test").expect("lease queue task");
    transition_business_command_for_task(
        &root,
        &task_id,
        "leased",
        None,
        None,
        None,
        "worker leased",
    )
    .expect("lease command");
    transition_business_command_for_task(
        &root,
        &task_id,
        "running",
        None,
        None,
        None,
        "worker started",
    )
    .expect("start command");

    hold_leased_messages(
        &root,
        std::slice::from_ref(&task_id),
        &HoldReason::Technical {
            policy_id: "typed-command-result-persist".to_string(),
        },
        "database is locked while persisting typed result",
    )
    .expect("persist atomic command and queue hold");

    let task = load_queue_task(&root, &task_id)
        .expect("load held queue task")
        .expect("queue task exists");
    assert_eq!(task.route_status, "pending");
    assert!(task.lease_owner.is_none());
    assert!(task.leased_at.is_none());
    let projection = business_command_projection(&root, "command-result-hold")
        .expect("load held command projection");
    assert_eq!(projection["execution_phase"], "retry_wait");
    assert_eq!(projection["terminal_status"], "none");
    assert_eq!(projection["retryable"], true);

    let conn = open_channel_db(&resolve_db_path(&root, None)).expect("open core db");
    let (attempts, retry_not_before, hold_reason): (i64, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT failure_attempt_count, retry_not_before, hold_reason FROM communication_routing_state WHERE message_key=?1",
            params![task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("load durable hold metadata");
    assert_eq!(attempts, 1);
    assert!(retry_not_before.is_some());
    assert_eq!(
        hold_reason.as_deref(),
        Some("technical:typed-command-result-persist")
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn retention_externalizes_large_terminal_results_only_after_delivery() {
    let root = business_command_test_root("ctox-business-command-retention");
    claim_business_control_command(
        &root,
        business_command_claim("command-retention", "sha256:retention"),
    )
    .expect("claim control command");
    complete_business_control_command(
        &root,
        "command-retention",
        "completed",
        &json!({"payload": "x".repeat(70 * 1024)}),
        None,
    )
    .expect("complete large command");
    let before_delivery = business_command_retention_maintenance(&root, false)
        .expect("retention report before delivery");
    assert_eq!(before_delivery["large_result_candidates"], 0);

    let db_path = resolve_db_path(&root, None);
    let conn = open_channel_db(&db_path).expect("open core db");
    conn.execute(
        "UPDATE business_command_outbox
         SET status = 'delivered', delivered_at_ms = 1
         WHERE command_id = 'command-retention'",
        [],
    )
    .expect("mark projections delivered");
    drop(conn);
    let applied = business_command_retention_maintenance(&root, true).expect("apply retention");
    assert_eq!(applied["externalized_results"], 1);
    assert_eq!(applied["aggregate_and_transition_evidence_deleted"], 0);
    let conn = open_channel_db(&db_path).expect("reopen core db");
    let (result_json, transitions, outbox): (String, i64, i64) = conn
        .query_row(
            "SELECT
                (SELECT result_json FROM business_command_aggregates WHERE command_id = 'command-retention'),
                (SELECT COUNT(*) FROM business_command_transitions WHERE command_id = 'command-retention'),
                (SELECT COUNT(*) FROM business_command_outbox WHERE command_id = 'command-retention')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("inspect retained evidence");
    let reference: Value = serde_json::from_str(&result_json).expect("artifact reference json");
    assert_eq!(reference["externalized"], true);
    assert_eq!(transitions, 2);
    assert_eq!(outbox, 0);
    let artifact = root.join(reference["artifact_ref"].as_str().expect("artifact path"));
    assert!(artifact.is_file());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn projection_outbox_retries_with_backoff_then_dead_letters() {
    let root = business_command_test_root("ctox-business-command-outbox-retry");
    claim_business_control_command(
        &root,
        business_command_claim("command-outbox-retry", "sha256:outbox-retry"),
    )
    .expect("claim command");
    let event = pending_business_command_outbox(&root, 1)
        .expect("list pending outbox")
        .into_iter()
        .next()
        .expect("pending event");
    mark_business_command_outbox_failed(&root, &event.event_id, "projection offline", 2)
        .expect("first delivery failure");
    let db_path = resolve_db_path(&root, None);
    let conn = open_channel_db(&db_path).expect("open core db");
    let first: (String, i64, i64) = conn
        .query_row(
            "SELECT status, attempts, next_attempt_at_ms FROM business_command_outbox WHERE event_id = ?1",
            params![event.event_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("inspect failed event");
    assert_eq!(first.0, "failed");
    assert_eq!(first.1, 1);
    assert!(first.2 > 0);
    drop(conn);
    mark_business_command_outbox_failed(&root, &event.event_id, "still offline", 2)
        .expect("second delivery failure");
    let conn = open_channel_db(&db_path).expect("reopen core db");
    let terminal: (String, i64) = conn
        .query_row(
            "SELECT status, attempts FROM business_command_outbox WHERE event_id = ?1",
            params![event.event_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("inspect dead letter");
    assert_eq!(terminal, ("dead_letter".to_string(), 2));
    let _ = fs::remove_dir_all(root);
}
