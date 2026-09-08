use super::*;
use crate::mission::channels;
use serde_json::{json, Value};

fn task(root: &Path) -> anyhow::Result<channels::QueueTaskView> {
    static NEXT_FIXTURE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let fixture = NEXT_FIXTURE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    channels::create_queue_task(
        root,
        channels::QueueTaskCreateRequest {
            title: "Cockpit control".into(),
            prompt: format!("Read-only fixture work {fixture}"),
            thread_key: "cockpit-fixture".into(),
            workspace_root: None,
            priority: "normal".into(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )
}

#[test]
fn all_cockpit_controls_deny_user_before_effects_and_audit_admin() -> anyhow::Result<()> {
    for name in [
        "release",
        "block",
        "retry",
        "capacity",
        "pause",
        "abort_turn",
    ] {
        let root = tempfile::tempdir()?;
        let task = task(root.path())?;
        if matches!(name, "release" | "retry") {
            channels::update_queue_task(
                root.path(),
                channels::QueueTaskUpdateRequest {
                    message_key: task.message_key.clone(),
                    route_status: Some("blocked".into()),
                    status_note: Some("fixture hold".into()),
                    ..Default::default()
                },
            )?;
        }
        let core = rusqlite::Connection::open(crate::paths::core_db(root.path()))?;
        if name == "retry" {
            core.execute("UPDATE communication_routing_state SET failure_attempt_count=3,failure_class='transient',retry_not_before='2099-01-01T00:00:00Z' WHERE message_key=?1",[&task.message_key])?;
        }
        let original =
            serde_json::to_value(channels::load_queue_task(root.path(), &task.message_key)?)?;
        let mut payload = json!({"task_id":task.message_key});
        match name {
            "block" => payload["reason"] = json!("Operator hold"),
            "capacity" => payload["workers"] = json!(2),
            "pause" => {
                payload["paused"] = json!(true);
                payload["reason"] = json!("Operator pause");
            }
            _ => {}
        }
        for role in ["user", "admin"] {
            let actor = format!("cockpit-{role}");
            let (token, _) =
                super::super::store::issue_business_os_capability_token_for_managed_user(
                    root.path(),
                    &actor,
                    &actor,
                    role,
                    now_ms() as i64,
                )?;
            let id = format!("cockpit-{name}-{role}");
            let request = json!({
                "id":id,"command_id":id,"module":"ctox","command_type":format!("ctox.queue.{name}"),
                "payload":payload,"client_context":{"capability_token":token,"actor":{"id":actor,"role":role,"is_admin":role=="admin"}},
            });
            let result = accept_rxdb_business_command_with_origin(
                root.path(),
                request.clone(),
                CommandOrigin::ReplicatedPeer,
            )?;
            if role == "user" {
                assert!(result.to_string().contains("denied"), "{name}: {result}");
                assert_eq!(
                    serde_json::to_value(channels::load_queue_task(
                        root.path(),
                        &task.message_key
                    )?)?,
                    original,
                    "{name} mutated before authorization"
                );
                assert!(!super::super::harness_cockpit::queue_is_paused(root.path()));
                assert_eq!(
                    crate::service::configure_queue_worker_capacity(root.path(), None)?
                        ["max_workers"],
                    4
                );
            } else if name == "abort_turn" {
                assert_eq!(result["result"]["status"], "unsupported", "{result}");
                assert!(result["result"]["reason"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty()));
            } else {
                assert_eq!(result["ok"], true, "{name}: {result}");
                if name == "release" {
                    assert_eq!(
                        channels::load_queue_task(root.path(), &task.message_key)?
                            .unwrap()
                            .status_note
                            .as_deref(),
                        Some("fixture hold"),
                        "release without note must preserve the existing status note"
                    );
                }
            }
            if role == "admin" {
                let replay = accept_rxdb_business_command_with_origin(
                    root.path(),
                    request,
                    CommandOrigin::ReplicatedPeer,
                )?;
                assert_eq!(replay["already_accepted"], true, "{name}: {replay}");
                assert_eq!(
                    replay["status"],
                    if name == "abort_turn" {
                        "failed"
                    } else {
                        "completed"
                    },
                    "control claim must have a durable terminal outcome"
                );
                assert_eq!(
                    replay["execution_task_id"], "",
                    "a control must not claim the target task as its own execution"
                );
            }
        }
        let audited:i64=core.query_row("SELECT COUNT(*) FROM ctox_harness_flow_events WHERE event_kind='cockpit.control' AND json_extract(metadata_json,'$.actor')='cockpit-admin'",[],|r|r.get(0))?;
        assert_eq!(audited, 2, "{name} intent and result audit");
        let task = channels::load_queue_task(root.path(), &task.message_key)?.unwrap();
        match name {
            "release" => assert_eq!(task.route_status, "pending"),
            "block" => {
                assert_eq!(task.route_status, "blocked");
                assert_eq!(task.hold_reason.as_deref(), Some("Operator hold"));
            }
            "retry" => {
                assert_eq!(task.route_status, "pending");
                assert_eq!(task.failure_attempt_count, 0);
                assert!(task.retry_not_before.is_none());
                assert!(task.failure_class.is_none());
            }
            "capacity" => assert_eq!(
                crate::service::configure_queue_worker_capacity(root.path(), None)?["max_workers"],
                2
            ),
            "pause" => assert!(super::super::harness_cockpit::queue_is_paused(root.path())),
            _ => {}
        }
    }
    Ok(())
}

#[test]
fn cockpit_requested_audit_excludes_large_unknown_fields_and_caps_text() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (token, _) = super::super::store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        "audit-admin",
        "Audit Admin",
        "admin",
        now_ms() as i64,
    )?;
    for (id, reason, succeeds) in [
        ("bounded-ok", "pause".to_string(), true),
        ("bounded-invalid", "ä".repeat(1500), false),
    ] {
        let request = json!({
            "id":id,"command_id":id,"module":"ctox","command_type":"ctox.queue.pause",
            "payload":{
                "paused":true,"reason":reason,"note":"界".repeat(1500),
                "priority":{"sensitive":"not text"},"workers":{"sensitive":"not an integer"},
                "foreign_secret":"x".repeat(1024 * 1024)
            },
            "client_context":{"capability_token":token,"actor":{"id":"audit-admin","role":"admin"}}
        });
        let result = accept_rxdb_business_command_with_origin(
            root.path(),
            request,
            CommandOrigin::ReplicatedPeer,
        );
        if succeeds {
            assert_eq!(result?["ok"], true);
        } else {
            assert!(result
                .expect_err("invalid reason must fail validation")
                .to_string()
                .contains("reason exceeds 1000 characters"));
        }
    }
    let core = rusqlite::Connection::open(crate::paths::core_db(root.path()))?;
    let mut statement = core.prepare(
        "SELECT metadata_json FROM ctox_harness_flow_events
         WHERE event_kind = 'cockpit.control' AND json_extract(metadata_json, '$.stage') = 'requested'",
    )?;
    let audits = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    assert_eq!(
        audits.len(),
        2,
        "both valid and invalid intents are audited"
    );
    for raw in audits {
        assert!(raw.len() < 8192, "audit grew to {} bytes", raw.len());
        let audit: Value = serde_json::from_str(&raw)?;
        assert_eq!(audit["actor"], "audit-admin");
        let payload = audit["payload"].as_object().unwrap();
        assert_eq!(payload.len(), 3);
        assert!(!payload.contains_key("foreign_secret"));
        assert!(!payload.contains_key("priority"));
        assert!(!payload.contains_key("workers"));
        assert_eq!(payload["paused"], true);
        assert!(payload["reason"].as_str().unwrap().chars().count() <= 1000);
        assert_eq!(payload["note"].as_str().unwrap().chars().count(), 1000);
    }
    Ok(())
}

#[test]
fn ordinary_queue_block_preserves_the_harness_hold_reason() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let task = task(root.path())?;
    let core = rusqlite::Connection::open(crate::paths::core_db(root.path()))?;
    core.execute(
        "UPDATE communication_routing_state SET hold_reason='awaiting_review' WHERE message_key=?1",
        [&task.message_key],
    )?;
    channels::update_queue_task(
        root.path(),
        channels::QueueTaskUpdateRequest {
            message_key: task.message_key.clone(),
            route_status: Some("blocked".into()),
            ..Default::default()
        },
    )?;
    assert_eq!(
        channels::load_queue_task(root.path(), &task.message_key)?
            .unwrap()
            .hold_reason
            .as_deref(),
        Some("awaiting_review")
    );
    Ok(())
}

#[test]
fn pause_prevents_new_leases_and_retry_clears_projected_nulls() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let task = task(root.path())?;
    crate::inference::runtime_env::set_runtime_env_value(
        root.path(),
        "queue.pause",
        r#"{"paused":true,"reason":"test"}"#,
    )?;
    assert!(channels::lease_queue_task(root.path(), &task.message_key, "fixture-worker").is_err());
    assert!(channels::lease_pending_inbound_messages(root.path(), 1, "fixture-worker")?.is_empty());
    crate::inference::runtime_env::set_runtime_env_value(
        root.path(),
        "queue.pause",
        r#"{"paused":false,"reason":null}"#,
    )?;
    let leased = channels::lease_queue_task(root.path(), &task.message_key, "fixture-worker")?;
    assert_eq!(leased.attempt, 1);
    let core = rusqlite::Connection::open(crate::paths::core_db(root.path()))?;
    core.execute("UPDATE communication_routing_state SET route_status='blocked',failure_attempt_count=3,failure_class='transient',hold_reason='wait',wait_entity_type='approval',wait_entity_id='approval-fixture',retry_not_before='2099-01-01T00:00:00Z' WHERE message_key=?1",[&task.message_key])?;
    let old = channels::load_queue_task(root.path(), &task.message_key)?.unwrap();
    let mut payload = json!({});
    super::super::store_projections::enrich_queue_projection_payload(
        &mut payload,
        &old,
        &old.route_status,
    );
    assert_eq!(payload["failure_attempt_count"], 3);
    assert_eq!(payload["wait_entity_id"], "approval-fixture");
    let released = channels::control_queue_task(
        root.path(),
        channels::QueueTaskUpdateRequest {
            message_key: task.message_key,
            route_status: Some("pending".into()),
            clear_note: true,
            ..Default::default()
        },
        true,
    )?;
    super::super::store_projections::enrich_queue_projection_payload(
        &mut payload,
        &released,
        &released.route_status,
    );
    for key in [
        "lease_expires_at",
        "lease_worker_id",
        "failure_class",
        "retry_not_before",
        "hold_reason",
        "wait_entity_type",
        "wait_entity_id",
        "crew_member_id",
    ] {
        assert_eq!(
            payload.get(key),
            Some(&Value::Null),
            "explicit null required for {key}"
        );
    }
    assert_eq!(payload["failure_attempt_count"], 0);
    Ok(())
}

#[test]
fn pause_preserves_current_lease_until_normal_acknowledgement() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let active = task(root.path())?;
    let waiting = task(root.path())?;
    assert_ne!(active.message_key, waiting.message_key);
    let leased = channels::lease_queue_task(root.path(), &active.message_key, "fixture-worker")?;
    crate::inference::runtime_env::set_runtime_env_value(
        root.path(),
        "queue.pause",
        r#"{"paused":true,"reason":"Drain current slice"}"#,
    )?;
    let still_leased = channels::load_queue_task(root.path(), &active.message_key)?.unwrap();
    assert_eq!(still_leased.route_status, "leased");
    assert_eq!(still_leased.lease_owner, leased.lease_owner);
    assert_eq!(still_leased.lease_expires_at, leased.lease_expires_at);
    assert!(
        channels::lease_queue_task(root.path(), &waiting.message_key, "another-worker").is_err()
    );
    assert_eq!(
        channels::ack_leased_messages_with_failure_reason(
            root.path(),
            &[active.message_key.clone()],
            "failed",
            "Fixture slice ended normally with an execution failure"
        )?,
        1
    );
    assert_eq!(
        channels::load_queue_task(root.path(), &active.message_key)?
            .unwrap()
            .route_status,
        "failed"
    );
    assert!(channels::lease_pending_inbound_messages(root.path(), 1, "another-worker")?.is_empty());
    assert_eq!(
        channels::load_queue_task(root.path(), &waiting.message_key)?
            .unwrap()
            .route_status,
        "pending"
    );
    Ok(())
}

#[test]
fn cockpit_collections_are_founder_readable_but_user_denied() {
    use crate::business_os::policy::{role_may_read_collection, BusinessOsRole};
    // Harness status (pause, capacity, who is on duty) is user-readable since
    // 08.09.2026; runs and event streams stay admin/founder.
    let status_scope =
        crate::business_os::policy::BusinessOsScope::collection("ctox_harness_status");
    let user = crate::business_os::policy::BusinessOsActor::new(None, "user");
    assert!(
        crate::business_os::policy::evaluate(
            &user,
            crate::business_os::policy::BusinessOsPermission::DataRead,
            &status_scope
        )
        .allowed
    );
    assert!(
        !crate::business_os::policy::evaluate(
            &user,
            crate::business_os::policy::BusinessOsPermission::DataWrite,
            &status_scope
        )
        .allowed
    );
    assert!(role_may_read_collection(
        BusinessOsRole::User,
        "ctox_harness_status"
    ));
    for collection in ["ctox_harness_events", "ctox_runs", "ctox_crew_learnings"] {
        let scope = crate::business_os::policy::BusinessOsScope::collection(collection);
        for role in ["admin", "founder", "user"] {
            let actor = crate::business_os::policy::BusinessOsActor::new(None, role);
            assert_eq!(
                crate::business_os::policy::evaluate(
                    &actor,
                    crate::business_os::policy::BusinessOsPermission::DataRead,
                    &scope
                )
                .allowed,
                role != "user"
            );
            assert!(
                !crate::business_os::policy::evaluate(
                    &actor,
                    crate::business_os::policy::BusinessOsPermission::DataWrite,
                    &scope
                )
                .allowed
            );
        }
        assert!(role_may_read_collection(BusinessOsRole::Admin, collection));
        assert!(role_may_read_collection(
            BusinessOsRole::Founder,
            collection
        ));
        assert!(!role_may_read_collection(BusinessOsRole::User, collection));
    }
    assert!(!role_may_read_collection(
        BusinessOsRole::Founder,
        "business_credentials"
    ));
}

#[test]
fn existing_native_queue_handlers_keep_their_queue_classification() {
    for command_type in [
        "kundenpipeline.triage.write",
        "kundenpipeline.decision.request",
        "kundenpipeline.decision.resolve",
        "kundenpipeline.decision.answer",
        "kundenpipeline.mail.send",
        "kundenpipeline.delegate",
    ] {
        assert!(
            !crate::business_os::store::is_rxdb_control_command_type(command_type),
            "{command_type} must remain queued work"
        );
    }
}

#[test]
fn chat_retention_drops_status_before_user_and_reply_messages() {
    let mut messages = (0..38)
        .map(|i| json!({"id":i,"kind":"reply","role":"ctox"}))
        .collect::<Vec<_>>();
    messages.insert(0, json!({"id":"question","kind":"question","role":"user"}));
    for i in 0..4 {
        messages.push(json!({"id":format!("status-{i}"),"kind":"status"}));
    }
    super::super::harness_cockpit::trim_messages(&mut messages);
    assert_eq!(messages.len(), 40);
    assert_eq!(messages[0]["id"], "question");
    assert_eq!(messages.iter().filter(|m| m["kind"] == "reply").count(), 38);
    assert_eq!(messages.last().unwrap()["id"], "status-3");
}
