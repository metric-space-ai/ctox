// In-tree behavioral tests for the ticket store (extracted from
// mod.rs; `use super::*` keeps access to crate-private internals).
// ctox-allow-direct-state-write: migration tests below construct historical
// table layouts directly and verify lossless schema conversion/rollback.
use super::*;
use crate::mission::ticket_local_native;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_root(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "ctox-ticket-test-{}-{}",
        label,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ))
}

#[test]
fn control_approval_mode_canonical_values_round_trip_and_unknown_fails() -> Result<()> {
    for mode in approval_mode::ControlApprovalMode::ALL {
        assert_eq!(
            approval_mode::ControlApprovalMode::parse(mode.as_str())?,
            mode
        );
    }

    let error = approval_mode::ControlApprovalMode::parse("future_mode")
        .expect_err("unknown control approval modes must fail closed");
    assert!(error.to_string().contains("future_mode"));
    Ok(())
}

#[test]
fn dry_run_only_approval_contract_stays_paired() {
    let mode = approval_mode::ControlApprovalMode::DryRunOnly;
    assert_eq!(mode.rank(), 0);
    assert_eq!(
        mode.missing_approvals(),
        vec!["execution is disabled for this bundle".to_string()]
    );
    assert_eq!(
        mode.initial_case_state(),
        case_state::TicketCaseState::Blocked
    );
}

#[test]
fn human_approval_required_contract_stays_paired() {
    let mode = approval_mode::ControlApprovalMode::HumanApprovalRequired;
    assert_eq!(mode.rank(), 1);
    assert_eq!(
        mode.missing_approvals(),
        vec!["owner or designated approver".to_string()]
    );
    assert_eq!(
        mode.initial_case_state(),
        case_state::TicketCaseState::ApprovalPending
    );
}

#[test]
fn bounded_auto_execute_approval_contract_stays_paired() {
    let mode = approval_mode::ControlApprovalMode::BoundedAutoExecute;
    assert_eq!(mode.rank(), 2);
    assert!(mode.missing_approvals().is_empty());
    assert_eq!(
        mode.initial_case_state(),
        case_state::TicketCaseState::Executable
    );
}

#[test]
fn direct_execute_allowed_approval_contract_stays_paired() {
    let mode = approval_mode::ControlApprovalMode::DirectExecuteAllowed;
    assert_eq!(mode.rank(), 3);
    assert!(mode.missing_approvals().is_empty());
    assert_eq!(
        mode.initial_case_state(),
        case_state::TicketCaseState::Executable
    );
}

#[test]
fn unknown_control_approval_mode_does_not_become_approval_pending() {
    let state = approval_mode::ControlApprovalMode::parse("future_mode")
        .map(approval_mode::ControlApprovalMode::initial_case_state);
    assert!(
        state.is_err(),
        "unknown approval mode must not silently become approval_pending"
    );
}

#[test]
fn ticket_case_state_canonical_values_round_trip() -> Result<()> {
    for state in case_state::TicketCaseState::ALL {
        assert_eq!(case_state::TicketCaseState::parse(state.as_str())?, state);
    }
    Ok(())
}

#[test]
fn ticket_case_state_reads_every_documented_alias() -> Result<()> {
    use case_state::TicketCaseState;

    for (alias, expected) in [
        ("in_progress", TicketCaseState::Executing),
        ("running", TicketCaseState::Executing),
        ("review", TicketCaseState::AwaitingReview),
        ("reviewing", TicketCaseState::AwaitingReview),
        ("rework", TicketCaseState::ReworkRequired),
        ("verification", TicketCaseState::AwaitingVerification),
        ("done", TicketCaseState::Closed),
        ("completed", TicketCaseState::Closed),
    ] {
        assert_eq!(TicketCaseState::parse(alias)?, expected, "alias {alias}");
    }
    Ok(())
}

#[test]
fn ticket_case_state_rejects_unknown_values() {
    let error = case_state::TicketCaseState::parse("future_state")
        .expect_err("unknown ticket case states must fail closed");
    assert!(error.to_string().contains("future_state"));
}

#[test]
fn ticket_case_state_core_state_and_event_mappings_stay_paired() {
    use case_state::TicketCaseState;

    for (state, expected_state, expected_event) in [
        (
            TicketCaseState::Created,
            CoreState::Created,
            CoreEvent::CreateTicket,
        ),
        (
            TicketCaseState::Open,
            CoreState::Created,
            CoreEvent::CreateTicket,
        ),
        (
            TicketCaseState::Queued,
            CoreState::Created,
            CoreEvent::CreateTicket,
        ),
        (
            TicketCaseState::Classified,
            CoreState::Classified,
            CoreEvent::Classify,
        ),
        (
            TicketCaseState::Planned,
            CoreState::Planned,
            CoreEvent::Plan,
        ),
        (TicketCaseState::Ready, CoreState::Planned, CoreEvent::Plan),
        (
            TicketCaseState::Executable,
            CoreState::Planned,
            CoreEvent::Plan,
        ),
        (
            TicketCaseState::Executing,
            CoreState::Executing,
            CoreEvent::Execute,
        ),
        (
            TicketCaseState::ApprovalPending,
            CoreState::AwaitingReview,
            CoreEvent::RequestReview,
        ),
        (
            TicketCaseState::AwaitingReview,
            CoreState::AwaitingReview,
            CoreEvent::RequestReview,
        ),
        (
            TicketCaseState::ReworkRequired,
            CoreState::ReworkRequired,
            CoreEvent::RequireRework,
        ),
        (
            TicketCaseState::AwaitingVerification,
            CoreState::AwaitingVerification,
            CoreEvent::Verify,
        ),
        (
            TicketCaseState::Verified,
            CoreState::Verified,
            CoreEvent::Verify,
        ),
        (
            TicketCaseState::WritebackPending,
            CoreState::Verified,
            CoreEvent::Verify,
        ),
        (TicketCaseState::Closed, CoreState::Closed, CoreEvent::Close),
        (
            TicketCaseState::Blocked,
            CoreState::Blocked,
            CoreEvent::Block,
        ),
        (
            TicketCaseState::BlockedNeedsClarification,
            CoreState::Blocked,
            CoreEvent::Block,
        ),
    ] {
        assert_eq!(
            (state.core_state(), state.core_event()),
            (expected_state, expected_event),
            "case state {} must keep its core state/event pair",
            state.as_str()
        );
    }
}

#[test]
fn ticket_case_event_path_rejects_unknown_state_instead_of_creating_ticket() {
    let error = cases::ticket_case_core_event("future_state")
        .expect_err("unknown event-path state must not default to CreateTicket");
    assert!(error.to_string().contains("future_state"));
}

#[test]
fn ticket_event_route_status_canonical_values_round_trip() -> Result<()> {
    for status in event_route_status::TicketEventRouteStatus::ALL {
        assert_eq!(
            event_route_status::TicketEventRouteStatus::parse(status.as_str())?,
            status
        );
    }
    Ok(())
}

#[test]
fn ticket_event_route_status_rejects_unknown_values() {
    let error = event_route_status::TicketEventRouteStatus::parse("future_status")
        .expect_err("unknown ticket event route statuses must fail closed");
    assert!(error.to_string().contains("future_status"));
}

fn assert_ticket_event_route_core_pair(
    status: event_route_status::TicketEventRouteStatus,
    expected_state: CoreState,
    expected_event: CoreEvent,
) {
    assert_eq!(
        (status.core_state(), status.core_event()),
        (expected_state, expected_event),
        "event route status {} must keep its core state/event pair",
        status.as_str()
    );
}

#[test]
fn ticket_event_route_pending_core_pair_is_pinned() {
    assert_ticket_event_route_core_pair(
        event_route_status::TicketEventRouteStatus::Pending,
        CoreState::Pending,
        CoreEvent::Release,
    );
}

#[test]
fn ticket_event_route_leased_core_pair_is_pinned() {
    assert_ticket_event_route_core_pair(
        event_route_status::TicketEventRouteStatus::Leased,
        CoreState::Leased,
        CoreEvent::Lease,
    );
}

#[test]
fn ticket_event_route_observed_core_pair_is_pinned() {
    assert_ticket_event_route_core_pair(
        event_route_status::TicketEventRouteStatus::Observed,
        CoreState::Completed,
        CoreEvent::Complete,
    );
}

#[test]
fn ticket_event_route_handled_core_pair_is_pinned() {
    assert_ticket_event_route_core_pair(
        event_route_status::TicketEventRouteStatus::Handled,
        CoreState::Completed,
        CoreEvent::Complete,
    );
}

#[test]
fn ticket_event_route_failed_core_pair_is_pinned() {
    assert_ticket_event_route_core_pair(
        event_route_status::TicketEventRouteStatus::Failed,
        CoreState::Failed,
        CoreEvent::Fail,
    );
}

#[test]
fn ticket_event_route_duplicate_core_pair_is_pinned() {
    assert_ticket_event_route_core_pair(
        event_route_status::TicketEventRouteStatus::Duplicate,
        CoreState::Superseded,
        CoreEvent::Supersede,
    );
}

#[test]
fn ticket_event_route_blocked_core_pair_is_pinned() {
    assert_ticket_event_route_core_pair(
        event_route_status::TicketEventRouteStatus::Blocked,
        CoreState::Blocked,
        CoreEvent::Block,
    );
}

#[test]
fn ticket_event_route_core_maps_reject_unknown_status_instead_of_pending() {
    let state_error = ticket_event_route_core_state("future_status")
        .expect_err("unknown route status must not default to Pending");
    let event_error = ticket_event_route_core_event("future_status")
        .expect_err("unknown route status must not default to Release");
    assert!(state_error.to_string().contains("future_status"));
    assert!(event_error.to_string().contains("future_status"));
}

#[test]
fn ticket_terminal_magic_actor_and_reason_do_not_create_a_grant() -> Result<()> {
    let conn = Connection::open_in_memory()?;
    enforce_ticket_event_route_status_transition(
        &conn,
        "no-grant-event",
        "leased",
        "handled",
        "ctox-ticket-routing",
        "force_ticket_event_routed_state",
        None,
    )
    .expect_err("ticket routing actor/reason strings must not authorize completion");
    let (accepted, proof_type): (i64, Option<String>) = conn.query_row(
        "SELECT accepted, json_type(request_json, '$.metadata.terminal_policy_proof') \
         FROM ctox_core_transition_proofs WHERE entity_id='ticket-event:no-grant-event'",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(accepted, 0);
    assert_eq!(proof_type, None);
    Ok(())
}

#[test]
fn every_ticket_terminal_policy_grant_passes_its_terminal_guard() -> Result<()> {
    for (event_key, grant) in [
        (
            "outbound-event-grant",
            TerminalPolicyGrant::outbound_ticket_event(),
        ),
        (
            "baseline-event-grant",
            TerminalPolicyGrant::baseline_observed_inbound_ticket_event(),
        ),
    ] {
        let conn = Connection::open_in_memory()?;
        enforce_ticket_event_route_status_transition_with_grant(
            &conn,
            event_key,
            "leased",
            "handled",
            "audit-actor",
            "audit reason only",
            None,
            Some(&grant),
        )?;
        let entity_id = format!("ticket-event:{event_key}");
        assert!(ticket_event_has_terminal_success_proof(&conn, &entity_id)?);
    }

    let conn = Connection::open_in_memory()?;
    let grant = TerminalPolicyGrant::authorized_approval_reply(
        "approval-message-1",
        "0123456789abcdef0123456789abcdef",
    );
    work_items::enforce_ticket_self_work_state_transition(
        &conn,
        "approval-work-1",
        "verified",
        "closed",
        "audit-actor",
        "authorized approval audit",
        None,
        Some(&grant),
    )?;
    assert!(work_items::work_item_has_terminal_success_proof(
        &conn,
        "approval-work-1"
    )?);
    Ok(())
}

fn insert_legacy_ticket_transition_proof(
    conn: &Connection,
    proof_id: &str,
    entity_id: &str,
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
        params![proof_id, entity_id, request_json],
    )?;
    Ok(())
}

#[test]
fn ticket_proof_lookup_uses_structured_json_and_keeps_legacy_rows() -> Result<()> {
    let conn = Connection::open_in_memory()?;
    insert_legacy_ticket_transition_proof(
        &conn,
        "ticket-random-substring",
        "ticket-event:random-substring",
        r#"{"metadata":{"unrelated":"terminal_policy_proof is only prose here"}}"#,
    )?;
    assert!(!ticket_event_has_terminal_success_proof(
        &conn,
        "ticket-event:random-substring"
    )?);

    insert_legacy_ticket_transition_proof(
        &conn,
        "ticket-legacy-policy",
        "ticket-event:legacy-policy",
        r#"{
            "metadata": {
                "terminal_policy_proof": "policy:legacy-ticket-routing"
            }
        }"#,
    )?;
    assert!(ticket_event_has_terminal_success_proof(
        &conn,
        "ticket-event:legacy-policy"
    )?);
    Ok(())
}

#[test]
fn ticket_self_work_migration_preserves_rows_and_drops_unique() -> Result<()> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(
        r#"
        CREATE TABLE ticket_self_work_items (
            work_id TEXT PRIMARY KEY,
            source_system TEXT NOT NULL,
            kind TEXT NOT NULL,
            title TEXT NOT NULL,
            body_text TEXT NOT NULL,
            state TEXT NOT NULL,
            metadata_json TEXT NOT NULL,
            remote_ticket_id TEXT,
            remote_locator TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE(source_system, kind)
        );
        INSERT INTO ticket_self_work_items
            (work_id, source_system, kind, title, body_text, state, metadata_json, created_at, updated_at)
        VALUES
            ('w1', 'local', 'triage', 't', 'b', 'open', '{}', 'now', 'now'),
            ('w2', 'local', 'review', 't', 'b', 'open', '{}', 'now', 'now');
        "#,
    )?;

    migrate_ticket_self_work_items_schema(&conn)?;

    let count: i64 = conn.query_row("SELECT COUNT(*) FROM ticket_self_work_items", [], |row| {
        row.get(0)
    })?;
    assert_eq!(count, 2, "all rows must survive the migration");
    let sql: String = conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'ticket_self_work_items'",
        [],
        |row| row.get(0),
    )?;
    assert!(
        !sql.contains("UNIQUE(source_system, kind)"),
        "the UNIQUE(source_system, kind) constraint must be gone after migration"
    );
    let legacy: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'ticket_self_work_items_legacy_unique'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(
        legacy, 0,
        "the legacy table must be dropped after a successful migration"
    );
    Ok(())
}

#[test]
fn ticket_self_work_migration_rolls_back_without_data_loss() -> Result<()> {
    let conn = Connection::open_in_memory()?;
    // Fixture old schema with a NULLABLE title so the copy into the NOT NULL new column
    // fails after the rename, exercising the rollback path.
    conn.execute_batch(
        r#"
        CREATE TABLE ticket_self_work_items (
            work_id TEXT PRIMARY KEY,
            source_system TEXT NOT NULL,
            kind TEXT NOT NULL,
            title TEXT,
            body_text TEXT NOT NULL,
            state TEXT NOT NULL,
            metadata_json TEXT NOT NULL,
            remote_ticket_id TEXT,
            remote_locator TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE(source_system, kind)
        );
        INSERT INTO ticket_self_work_items
            (work_id, source_system, kind, body_text, state, metadata_json, created_at, updated_at)
        VALUES
            ('w1', 'local', 'triage', 'b', 'open', '{}', 'now', 'now');
        "#,
    )?;

    let result = migrate_ticket_self_work_items_schema(&conn);
    assert!(
        result.is_err(),
        "migration must fail when a row cannot be copied into the new NOT NULL schema"
    );

    // The original table and its row must be intact: the failed migration rolled back.
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM ticket_self_work_items", [], |row| {
        row.get(0)
    })?;
    assert_eq!(
        count, 1,
        "the original row must survive a rolled-back migration"
    );
    let sql: String = conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'ticket_self_work_items'",
        [],
        |row| row.get(0),
    )?;
    assert!(
        sql.contains("UNIQUE(source_system, kind)"),
        "the original schema must be restored after rollback"
    );
    Ok(())
}

fn is_terminology_firewall_text_file(path: &std::path::Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("md" | "py" | "rs" | "sh" | "js" | "mjs" | "ts" | "tsx")
    )
}

fn strict_internal_work_legacy_term(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "self-work",
        "self_work",
        "self work",
        "ticketselfwork",
        "requeueselfwork",
        "requeue-self-work",
    ]
    .iter()
    .any(|term| lower.contains(term))
}

fn user_visible_internal_work_legacy_phrase(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "self-work item",
        "self-work items",
        "self-work backlog",
        "self-work closure",
        "ticket/self-work",
        "ctox self work",
        "self work",
        "self_work_open",
        "requeueselfwork",
        "requeue-self-work",
    ]
    .iter()
    .any(|term| lower.contains(term))
}

fn internal_work_firewall_legacy_line_allowed(relative: &str, line: &str) -> bool {
    matches!(relative, "HARNESS.md" | "docs/harness-operating-model.md")
        && (line.contains("`ticket_self_work_items`")
            || line.contains("`self-work:*`")
            || line.contains("`self-work-queue-task`"))
}

fn scan_internal_work_firewall_file(
    path: &std::path::Path,
    relative: &str,
    strict: bool,
    violations: &mut Vec<String>,
) -> Result<()> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read terminology firewall file {relative}"))?;
    for (index, line) in content.lines().enumerate() {
        let has_violation = if strict {
            strict_internal_work_legacy_term(line)
        } else {
            user_visible_internal_work_legacy_phrase(line)
        };
        if has_violation && !internal_work_firewall_legacy_line_allowed(relative, line) {
            violations.push(format!("{relative}:{}: {}", index + 1, line.trim()));
        }
    }
    Ok(())
}

fn scan_internal_work_firewall_dir(
    root: &std::path::Path,
    relative_dir: &str,
    strict: bool,
    violations: &mut Vec<String>,
) -> Result<()> {
    let dir = root.join(relative_dir);
    for entry in std::fs::read_dir(&dir)
        .with_context(|| format!("failed to read terminology firewall dir {relative_dir}"))?
    {
        let entry = entry?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if path.is_dir() {
            scan_internal_work_firewall_dir(root, &relative, strict, violations)?;
        } else if is_terminology_firewall_text_file(&path) {
            scan_internal_work_firewall_file(&path, &relative, strict, violations)?;
        }
    }
    Ok(())
}

#[test]
fn internal_work_terminology_firewall_keeps_self_work_legacy_only() -> Result<()> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut violations = Vec::new();

    scan_internal_work_firewall_dir(&root, "src/skills/system", true, &mut violations)?;
    for relative in [
        "HARNESS.md",
        "docs/harness-operating-model.md",
        "src/core/context/live_context.rs",
        "src/core/harness/core/templates/collab/experimental_prompt.md",
        "src/core/mission/review.rs",
        "src/core/mission/plan.rs",
        "src/core/autonomy.rs",
        "src/core/business_os/mcp_channel.rs",
    ] {
        scan_internal_work_firewall_file(&root.join(relative), relative, true, &mut violations)?;
    }
    for relative in [
        "src/core/service/service.rs",
        "src/core/core_state/mod.rs",
        "src/core/service/harness_flow.rs",
        "src/core/service/process_mining.rs",
        "src/core/mission/channels/mod.rs",
        "src/core/mission/channels/business_os_projection.rs",
        "src/core/mission/channels/command_saga.rs",
        "src/core/mission/channels/outbound_review.rs",
        "src/core/mission/tickets/cli.rs",
        "src/core/mission/tickets/source_skills.rs",
        "src/core/mission/tickets/cases.rs",
        "src/core/mission/channels/tests.rs",
        "src/core/mission/ticket_zammad_native.rs",
    ] {
        scan_internal_work_firewall_file(&root.join(relative), relative, false, &mut violations)?;
    }

    if !violations.is_empty() {
        anyhow::bail!(
            "legacy self-work terminology leaked across the internal-work firewall:\n{}",
            violations.join("\n")
        );
    }
    Ok(())
}

#[test]
fn ticket_preflight_reports_missing_zammad_runtime() {
    let root = temp_root("preflight-zammad-missing");
    let mut settings = BTreeMap::new();
    settings.insert(
        "CTOX_TICKET_SYSTEMS".to_string(),
        "local,zammad".to_string(),
    );

    let issues = preflight_configured_ticket_systems(&root, &settings);

    assert!(issues
        .iter()
        .any(|issue| issue.system == "zammad" && issue.code == "missing_zammad_base_url"));
    assert!(issues
        .iter()
        .any(|issue| issue.system == "zammad" && issue.code == "missing_zammad_auth"));
    assert!(!issues.iter().any(|issue| issue.system == "local"));
}

#[test]
fn stale_ticket_event_lease_releases_to_pending() -> Result<()> {
    let root = temp_root("stale-ticket-lease");
    let remote = ticket_local_native::create_local_ticket(
        &root,
        "Lease me",
        "Initial baseline",
        Some("open"),
        Some("normal"),
    )?;
    sync_ticket_system(&root, "local")?;
    ticket_local_native::add_local_comment(&root, &remote.ticket_id, "Fresh update")?;
    sync_ticket_system(&root, "local")?;

    let leased = lease_pending_ticket_events(&root, 1, "ctox-service")?;
    assert_eq!(leased.len(), 1);
    let conn = open_ticket_db(&root)?;
    conn.execute(
        "UPDATE ticket_event_routing_state SET lease_expires_at='2000-01-01T00:00:00Z' WHERE event_key=?1",
        params![leased[0].event_key],
    )?;
    drop(conn);
    let released = release_stale_ticket_event_leases(&root, "ctox-service", &HashSet::new())?;

    assert_eq!(released, vec![leased[0].event_key.clone()]);
    let leased_again = lease_pending_ticket_events(&root, 1, "ctox-service")?;
    assert_eq!(leased_again[0].event_key, leased[0].event_key);
    Ok(())
}

#[test]
fn retryable_ticket_event_terminalizes_after_three_failures() -> Result<()> {
    let root = temp_root("ticket-event-retry-budget");
    let remote = ticket_local_native::create_local_ticket(
        &root,
        "Retry me",
        "Initial baseline",
        Some("open"),
        Some("normal"),
    )?;
    sync_ticket_system(&root, "local")?;
    ticket_local_native::add_local_comment(&root, &remote.ticket_id, "Fresh update")?;
    sync_ticket_system(&root, "local")?;

    let event_key = lease_pending_ticket_events(&root, 1, "ctox-service")?[0]
        .event_key
        .clone();
    for attempt in 1..=3 {
        fail_ticket_events(
            &root,
            &[event_key.clone()],
            TicketEventFailureClass::Retryable,
            "transient runtime failure",
        )?;
        let conn = open_ticket_db(&root)?;
        let row: (String, i64, Option<String>, Option<String>) = conn.query_row(
            "SELECT route_status, failure_attempt_count, retry_not_before, failure_proof FROM ticket_event_routing_state WHERE event_key=?1",
            params![event_key],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        assert_eq!(row.1, attempt);
        if attempt < 3 {
            assert_eq!(row.0, "pending");
            assert!(row.2.is_some());
            conn.execute(
                "UPDATE ticket_event_routing_state SET retry_not_before='2000-01-01T00:00:00Z' WHERE event_key=?1",
                params![event_key],
            )?;
            drop(conn);
            assert_eq!(
                lease_pending_ticket_events(&root, 1, "ctox-service")?[0].event_key,
                event_key
            );
        } else {
            assert_eq!(row.0, "failed");
            assert!(row.2.is_none());
            assert!(row.3.is_some());
        }
    }
    Ok(())
}

#[test]
fn waiting_external_ticket_event_only_wakes_for_matching_reference() -> Result<()> {
    let root = temp_root("ticket-event-wait-ref");
    let remote = ticket_local_native::create_local_ticket(
        &root,
        "Wait for approval",
        "Initial baseline",
        Some("open"),
        Some("normal"),
    )?;
    sync_ticket_system(&root, "local")?;
    ticket_local_native::add_local_comment(&root, &remote.ticket_id, "Approval needed")?;
    sync_ticket_system(&root, "local")?;

    let event_key = lease_pending_ticket_events(&root, 1, "ctox-service")?[0]
        .event_key
        .clone();
    block_ticket_events_for_wait(
        &root,
        std::slice::from_ref(&event_key),
        &crate::mission::plan::WaitRef {
            entity_type: "approval-gate".to_string(),
            entity_id: "work-release".to_string(),
        },
        "release approval is open",
    )?;

    assert!(lease_pending_ticket_events(&root, 1, "ctox-service")?.is_empty());
    assert_eq!(
        wake_ticket_events_waiting_for(&root, "approval-gate", "other-work")?,
        0
    );
    assert_eq!(
        wake_ticket_events_waiting_for(&root, "approval-gate", "work-release")?,
        1
    );
    assert_eq!(
        lease_pending_ticket_events(&root, 1, "ctox-service")?[0].event_key,
        event_key
    );
    Ok(())
}

#[test]
fn blocked_ticket_event_releases_after_knowledge_and_control_are_ready() -> Result<()> {
    let root = temp_root("blocked-ticket-release");
    let remote = ticket_local_native::create_local_ticket(
        &root,
        "Blocked until controls exist",
        "Initial baseline",
        Some("open"),
        Some("normal"),
    )?;
    sync_ticket_system(&root, "local")?;
    ticket_local_native::add_local_comment(&root, &remote.ticket_id, "Fresh update")?;
    sync_ticket_system(&root, "local")?;
    let ticket_key = format!("local:{}", remote.ticket_id);
    let leased = lease_pending_ticket_events(&root, 1, "ctox-service")?;
    assert_eq!(leased.len(), 1);
    ack_leased_ticket_events(&root, &[leased[0].event_key.clone()], "blocked")?;

    let still_blocked = release_ready_blocked_ticket_events(&root, 10)?;
    assert!(still_blocked.is_empty());

    refresh_observed_ticket_knowledge(&root, "local")?;
    set_ticket_label(
        &root,
        &ticket_key,
        "support/general",
        "test",
        None,
        json!({}),
    )?;
    put_control_bundle(
        &root,
        ControlBundleInput {
            label: "support/general".to_string(),
            runbook_id: "rb-general".to_string(),
            runbook_version: "v1".to_string(),
            policy_id: "pol-general".to_string(),
            policy_version: "v1".to_string(),
            approval_mode: "human_approval_required".to_string(),
            autonomy_level: "A0".to_string(),
            verification_profile_id: "verify-general".to_string(),
            writeback_profile_id: "writeback-general".to_string(),
            support_mode: "support_case".to_string(),
            default_risk_level: "low".to_string(),
            execution_actions: default_execution_actions(),
            notes: None,
        },
    )?;

    let released = release_ready_blocked_ticket_events(&root, 10)?;
    assert_eq!(released, vec![leased[0].event_key.clone()]);
    Ok(())
}

fn write_reply_bundle(bundle_dir: &std::path::Path, items: &[Value]) -> Result<()> {
    std::fs::create_dir_all(bundle_dir)?;
    std::fs::write(
        bundle_dir.join("main_skill.json"),
        serde_json::to_string_pretty(&json!({
            "main_skill_id": "eventus.email.support.main.v1",
            "title": "Eventus Email Support Main",
            "primary_channel": "email",
            "entry_action": "resolve_runbook_item",
            "resolver_contract": {"mode": "runbook-item"},
            "execution_contract": {"mode": "reply-only"},
            "resolve_flow": [
                "resolve the best matching runbook item",
                "load the linked skillbook",
                "compose a reply suggestion"
            ],
            "writeback_flow": [
                "verify reply",
                "write public comment back to the ticket"
            ],
            "linked_skillbooks": ["eventus.email.support.v1"],
            "linked_runbooks": ["eventus.runbook.registration.v1"]
        }))?,
    )?;
    std::fs::write(
        bundle_dir.join("skillbook.json"),
        serde_json::to_string_pretty(&json!({
            "skillbook_id": "eventus.email.support.v1",
            "title": "Eventus Email Support",
            "version": "v1",
            "mission": "Handle incoming support emails safely and clearly.",
            "non_negotiable_rules": [
                "Never invent product behavior.",
                "Keep the answer aligned with the manual."
            ],
            "runtime_policy": "Resolve a runbook item first, then draft the reply.",
            "answer_contract": "Give a concise, actionable email answer.",
            "workflow_backbone": [
                "identify the request",
                "load the runbook item",
                "reply only from the runbook facts"
            ],
            "routing_taxonomy": ["registration", "login"],
            "linked_runbooks": ["eventus.runbook.registration.v1"]
        }))?,
    )?;
    let item_labels = items
        .iter()
        .filter_map(|item| item.get("label").and_then(Value::as_str))
        .collect::<Vec<_>>();
    std::fs::write(
        bundle_dir.join("runbook.json"),
        serde_json::to_string_pretty(&json!({
            "runbook_id": "eventus.runbook.registration.v1",
            "skillbook_id": "eventus.email.support.v1",
            "title": "Registration issues",
            "version": "v1",
            "status": "active",
            "problem_domain": "registration",
            "item_labels": item_labels
        }))?,
    )?;
    let mut jsonl = String::new();
    for item in items {
        jsonl.push_str(&serde_json::to_string(item)?);
        jsonl.push('\n');
    }
    std::fs::write(bundle_dir.join("runbook_items.jsonl"), jsonl)?;
    Ok(())
}

#[test]
fn source_skill_bundle_imports_every_catalog_runbook_without_orphans() -> Result<()> {
    let root = temp_root("source-skill-runbook-catalog");
    let bundle_dir = root.join("runtime/generated-skills/catalog");
    std::fs::create_dir_all(&bundle_dir)?;
    let runbook_ids = ["catalog.runbook.one.v1", "catalog.runbook.two.v1"];
    std::fs::write(
        bundle_dir.join("main_skill.json"),
        serde_json::to_string_pretty(&json!({
            "main_skill_id": "catalog.main.v1",
            "title": "Catalog Main",
            "primary_channel": "research",
            "entry_action": "resolve_runbook_item",
            "linked_skillbooks": ["catalog.skillbook.v1"],
            "linked_runbooks": runbook_ids,
        }))?,
    )?;
    std::fs::write(
        bundle_dir.join("skillbook.json"),
        serde_json::to_string_pretty(&json!({
            "skillbook_id": "catalog.skillbook.v1",
            "title": "Catalog Skillbook",
            "version": "v1",
            "mission": "Test complete catalog imports.",
            "runtime_policy": "Resolve a runbook item.",
            "answer_contract": "Use cited facts only.",
            "linked_runbooks": runbook_ids,
        }))?,
    )?;
    std::fs::write(
        bundle_dir.join("runbook.json"),
        serde_json::to_string_pretty(&json!({
            "schema": "ctox.knowledge.runbook_catalog.v2",
            "primary_runbook": runbook_ids[0],
            "runbooks": [
                {
                    "runbook_id": runbook_ids[0],
                    "skillbook_id": "catalog.skillbook.v1",
                    "title": "First",
                    "version": "v1",
                    "status": "active",
                    "problem_domain": "first",
                    "item_labels": ["ONE"],
                },
                {
                    "runbook_id": runbook_ids[1],
                    "skillbook_id": "catalog.skillbook.v1",
                    "title": "Second",
                    "version": "v1",
                    "status": "active",
                    "problem_domain": "second",
                    "item_labels": ["TWO"],
                }
            ],
        }))?,
    )?;
    let items = [
        json!({
            "item_id": "catalog.item.one.v1",
            "runbook_id": runbook_ids[0],
            "skillbook_id": "catalog.skillbook.v1",
            "label": "ONE",
            "title": "First item",
            "problem_class": "first",
            "chunk_text": "first cited fact",
        }),
        json!({
            "item_id": "catalog.item.two.v1",
            "runbook_id": runbook_ids[1],
            "skillbook_id": "catalog.skillbook.v1",
            "label": "TWO",
            "title": "Second item",
            "problem_class": "second",
            "chunk_text": "second cited fact",
        }),
    ];
    let jsonl = items
        .iter()
        .map(serde_json::to_string)
        .collect::<std::result::Result<Vec<_>, _>>()?
        .join("\n");
    std::fs::write(bundle_dir.join("runbook_items.jsonl"), format!("{jsonl}\n"))?;
    std::fs::write(
        bundle_dir.join("resources.jsonl"),
        format!(
            "{}\n",
            serde_json::to_string(&json!({
                "resource_id": "catalog.resource.one",
                "title": "Source receipt",
                "kind": "evidence_receipt",
                "source_id": "SRC-001",
                "role": "evidence",
                "canonical_url": "https://example.com/source",
                "snapshot_hash": "abc123",
                "evidence_eligible": true,
                "linked_runbook_items": ["catalog.item.one.v1"],
            }))?
        ),
    )?;

    let imported = import_ticket_source_skill_bundle(
        &root,
        "catalog-test",
        bundle_dir.to_str().context("bundle path utf-8")?,
        None,
        true,
    )?;
    assert_eq!(
        imported.get("runbook_count").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        imported.get("resource_count").and_then(Value::as_u64),
        Some(1)
    );

    let conn = open_ticket_db(&root)?;
    let runbook_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM knowledge_runbooks WHERE skillbook_id = 'catalog.skillbook.v1'",
        [],
        |row| row.get(0),
    )?;
    let orphan_count: i64 = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM knowledge_runbook_items AS item
        LEFT JOIN knowledge_runbooks AS runbook ON runbook.runbook_id = item.runbook_id
        WHERE item.skillbook_id = 'catalog.skillbook.v1' AND runbook.runbook_id IS NULL
        "#,
        [],
        |row| row.get(0),
    )?;
    let resource_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM knowledge_resources WHERE skillbook_id = 'catalog.skillbook.v1'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(runbook_count, 2);
    assert_eq!(orphan_count, 0);
    assert_eq!(resource_count, 1);
    conn.execute(
        "INSERT INTO knowledge_embeddings (item_id, embedding_model, embedding_json, updated_at)
         VALUES ('catalog.item.one.v1', 'test-model', '[0.5]', '2026-01-01T00:00:00Z')",
        [],
    )?;
    drop(conn);

    std::fs::remove_file(bundle_dir.join("resources.jsonl"))?;
    let legacy_reimport = import_ticket_source_skill_bundle(
        &root,
        "catalog-test",
        bundle_dir.to_str().context("bundle path utf-8")?,
        None,
        true,
    )?;
    assert!(legacy_reimport
        .get("resource_count")
        .is_some_and(Value::is_null));
    let conn = open_ticket_db(&root)?;
    let preserved_embedding_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM knowledge_embeddings
          WHERE item_id = 'catalog.item.one.v1' AND embedding_model = 'test-model'",
        [],
        |row| row.get(0),
    )?;
    let preserved_resource_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM knowledge_resources
          WHERE resource_id = 'catalog.resource.one'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(preserved_embedding_count, 1);
    assert_eq!(preserved_resource_count, 1);

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn source_skill_resources_import_without_replacing_runbooks() -> Result<()> {
    let root = temp_root("source-skill-resource-import");
    create_or_update_skillbook(
        &root,
        "resource.skillbook.v1",
        "Resource Skillbook",
        "v1",
        "Use verified resources.",
        "",
        "",
        Vec::new(),
        Vec::new(),
        Vec::new(),
        vec!["resource.runbook.v1".to_owned()],
    )?;
    create_or_update_runbook(
        &root,
        "resource.runbook.v1",
        "resource.skillbook.v1",
        "Resource Runbook",
        "v1",
        "active",
        "evidence",
        vec!["RESOURCE-01".to_owned()],
    )?;
    add_or_update_runbook_item(
        &root,
        "resource.item.v1",
        "resource.runbook.v1",
        "resource.skillbook.v1",
        "RESOURCE-01",
        "Use source receipt",
        "evidence",
        "Use the verified source receipt.",
        "v1",
        "active",
        None,
        true,
    )?;

    let resources_file = root.join("runtime/resources.jsonl");
    std::fs::create_dir_all(resources_file.parent().context("resources parent")?)?;
    std::fs::write(
        &resources_file,
        format!(
            "{}\n",
            serde_json::to_string(&json!({
                "resource_id": "resource.one",
                "title": "Verified source",
                "kind": "primary_source",
                "source_id": "SRC-001",
                "role": "evidence",
                "canonical_url": "https://example.com/source",
                "snapshot_hash": "abc123",
                "evidence_eligible": true,
                "linked_runbook_items": ["resource.item.v1"],
            }))?
        ),
    )?;

    let imported = import_ticket_source_skill_resources(
        &root,
        "resource.skillbook.v1",
        resources_file.to_str().context("resources path utf-8")?,
        true,
    )?;
    assert_eq!(
        imported.get("resource_count").and_then(Value::as_u64),
        Some(1)
    );
    let conn = open_ticket_db(&root)?;
    let runbook_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM knowledge_runbooks WHERE skillbook_id = 'resource.skillbook.v1'",
        [],
        |row| row.get(0),
    )?;
    let item_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM knowledge_runbook_items WHERE skillbook_id = 'resource.skillbook.v1'",
        [],
        |row| row.get(0),
    )?;
    let resource_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM knowledge_resources WHERE skillbook_id = 'resource.skillbook.v1'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(runbook_count, 1);
    assert_eq!(item_count, 1);
    assert_eq!(resource_count, 1);

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn source_skill_bundle_rejects_missing_runbook_parent_before_writes() -> Result<()> {
    let root = temp_root("source-skill-runbook-missing-parent");
    let bundle_dir = root.join("runtime/generated-skills/missing-parent");
    write_reply_bundle(
        &bundle_dir,
        &[json!({
            "item_id": "orphan.item.v1",
            "runbook_id": "missing.runbook.v1",
            "skillbook_id": "eventus.email.support.v1",
            "label": "ORPHAN",
            "title": "Orphan item",
            "problem_class": "invalid",
            "chunk_text": "must not be imported",
        })],
    )?;

    let error = import_ticket_source_skill_bundle(
        &root,
        "invalid-test",
        bundle_dir.to_str().context("bundle path utf-8")?,
        None,
        true,
    )
    .expect_err("missing runbook parent must fail");
    assert!(error.to_string().contains("references missing runbook"));

    let conn = open_ticket_db(&root)?;
    let main_skill_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM knowledge_main_skills", [], |row| {
            row.get(0)
        })?;
    assert_eq!(main_skill_count, 0);

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn source_skill_bundle_rejects_ids_owned_by_another_skillbook() -> Result<()> {
    let root = temp_root("source-skill-foreign-owner");
    create_or_update_runbook(
        &root,
        "eventus.runbook.registration.v1",
        "foreign.skillbook.v1",
        "Foreign runbook",
        "v1",
        "active",
        "foreign",
        Vec::new(),
    )?;
    let bundle_dir = root.join("runtime/generated-skills/foreign-owner");
    write_reply_bundle(
        &bundle_dir,
        &[json!({
            "item_id": "eventus.registration.v1",
            "runbook_id": "eventus.runbook.registration.v1",
            "skillbook_id": "eventus.email.support.v1",
            "label": "REG-01",
            "title": "Registration",
            "problem_class": "registration",
            "chunk_text": "source-backed guidance",
        })],
    )?;

    let error = import_ticket_source_skill_bundle(
        &root,
        "eventus",
        bundle_dir.to_str().context("bundle path utf-8")?,
        None,
        true,
    )
    .expect_err("foreign runbook ownership must fail");
    assert!(error.to_string().contains("already owned by skillbook"));

    let conn = open_ticket_db(&root)?;
    let owner: String = conn.query_row(
        "SELECT skillbook_id FROM knowledge_runbooks
          WHERE runbook_id = 'eventus.runbook.registration.v1'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(owner, "foreign.skillbook.v1");

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn ticket_local_sync_dry_run_and_audit_flow_round_trips() -> Result<()> {
    let root = temp_root("lifecycle");
    std::fs::create_dir_all(&root)?;

    let remote = ticket_local_native::create_local_ticket(
        &root,
        "VPN outage",
        "Users cannot reach the VPN gateway.",
        Some("open"),
        Some("high"),
    )?;
    ticket_local_native::add_local_comment(&root, &remote.ticket_id, "Customer impact confirmed")?;
    let sync = sync_ticket_system(&root, "local")?;
    assert_eq!(sync.get("ok").and_then(Value::as_bool), Some(true));

    let ticket_key = format!("local:{}", remote.ticket_id);
    let ticket = load_ticket(&root, &ticket_key)?.context("ticket missing after sync")?;
    assert_eq!(ticket.title, "VPN outage");

    let bundle = put_control_bundle(
        &root,
        ControlBundleInput {
            label: "support/vpn".to_string(),
            runbook_id: "rb-vpn".to_string(),
            runbook_version: "v1".to_string(),
            policy_id: "pol-vpn".to_string(),
            policy_version: "v1".to_string(),
            approval_mode: "human_approval_required".to_string(),
            autonomy_level: "A1".to_string(),
            verification_profile_id: "verify-vpn".to_string(),
            writeback_profile_id: "writeback-comment".to_string(),
            support_mode: "incident".to_string(),
            default_risk_level: "high".to_string(),
            execution_actions: vec![
                "observe".to_string(),
                "analyze".to_string(),
                "draft_communication".to_string(),
            ],
            notes: Some("VPN incident starter bundle".to_string()),
        },
    )?;
    assert_eq!(bundle.bundle_version, 1);

    let assignment = set_ticket_label(
        &root,
        &ticket_key,
        "support/vpn",
        "manual",
        Some("support queue routing"),
        json!({"signal": "vpn"}),
    )?;
    assert_eq!(assignment.label, "support/vpn");

    let dry_run = create_dry_run(
        &root,
        &ticket_key,
        Some("VPN outage appears reproducible"),
        None,
    )?;
    assert_eq!(dry_run.label, "support/vpn");
    let case = load_case(&root, &dry_run.case_id)?.context("case missing after dry run")?;
    assert_eq!(case.state, "approval_pending");

    let case = decide_case_approval(
        &root,
        &case.case_id,
        "approved",
        "owner",
        Some("Proceed with bounded investigation"),
    )?;
    assert_eq!(case.state, "executable");

    let case =
        record_execution_action(&root, &case.case_id, "Reviewed VPN endpoint configuration")?;
    assert_eq!(case.state, "executing");

    let case = record_verification(
        &root,
        &case.case_id,
        "passed",
        Some("Dry verification complete"),
    )?;
    assert_eq!(case.state, "writeback_pending");

    let case = writeback_comment(
        &root,
        &case.case_id,
        "CTOX dry run complete; ready for controlled execution.",
        false,
    )?;
    assert_eq!(case.state, "writeback_pending");
    let leased_after_writeback = lease_pending_ticket_events(&root, 20, "ticket-test")?;
    assert!(
        leased_after_writeback.iter().all(|event| {
            event.metadata.get("origin").and_then(Value::as_str) != Some("ctox-writeback")
        }),
        "writeback-generated outbound events must not re-enter the inbound lease queue"
    );

    let audit = list_audit_records(&root, Some(&ticket_key), 20)?;
    assert!(audit
        .iter()
        .any(|item| item.action_type == "ticket_label_assignment"));
    assert!(audit
        .iter()
        .any(|item| item.action_type == "dry_run_record"));
    assert!(audit
        .iter()
        .any(|item| item.action_type == "approval_decision"));
    assert!(audit
        .iter()
        .any(|item| item.action_type == "writeback_record"));
    assert!(!audit.iter().any(|item| item.action_type == "case_closed"));

    let history = list_ticket_history(&root, &ticket_key, 20)?;
    assert!(history.iter().any(|event| event.event_type == "comment"));

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn business_os_ticket_commands_drive_full_case_lifecycle() -> Result<()> {
    let root = temp_root("business-os-ticket-lifecycle");
    std::fs::create_dir_all(&root)?;

    let remote = ticket_local_native::create_local_ticket(
        &root,
        "Business OS lifecycle",
        "Exercise the command adapter for every visible Ticket action.",
        Some("open"),
        Some("normal"),
    )?;
    sync_ticket_system(&root, "local")?;
    let ticket_key = format!("local:{}", remote.ticket_id);

    put_control_bundle(
        &root,
        ControlBundleInput {
            label: "support/business-os".to_string(),
            runbook_id: "rb-business-os".to_string(),
            runbook_version: "v1".to_string(),
            policy_id: "pol-business-os".to_string(),
            policy_version: "v1".to_string(),
            approval_mode: "human_approval_required".to_string(),
            autonomy_level: "A1".to_string(),
            verification_profile_id: "verify-business-os".to_string(),
            writeback_profile_id: "writeback-comment".to_string(),
            support_mode: "support_case".to_string(),
            default_risk_level: "low".to_string(),
            execution_actions: default_execution_actions(),
            notes: Some("Business OS command adapter lifecycle coverage".to_string()),
        },
    )?;
    set_ticket_label(
        &root,
        &ticket_key,
        "support/business-os",
        "test",
        Some("route to Business OS support controls"),
        json!({"source": "business-os-command-test"}),
    )?;

    let dry_run = create_dry_run(
        &root,
        &ticket_key,
        Some("Business OS command lifecycle dry run"),
        None,
    )?;
    let case_id = dry_run.case_id;

    let approved = run_business_os_ticket_command(
        &root,
        "ctox.ticket.approve",
        &json!({
            "case_id": case_id,
            "status": "approved",
            "decided_by": "business-os-test",
            "rationale": "approve command adapter path",
        }),
    )?;
    assert_eq!(
        approved.pointer("/case/state").and_then(Value::as_str),
        Some("executable")
    );

    let executed = run_business_os_ticket_command(
        &root,
        "ctox.ticket.execute",
        &json!({
            "case_id": case_id,
            "summary": "Executed through Business OS command adapter",
        }),
    )?;
    assert_eq!(
        executed.pointer("/case/state").and_then(Value::as_str),
        Some("executing")
    );

    let verified = run_business_os_ticket_command(
        &root,
        "ctox.ticket.verify",
        &json!({
            "case_id": case_id,
            "status": "passed",
            "summary": "Verified through Business OS command adapter",
        }),
    )?;
    assert_eq!(
        verified.pointer("/case/state").and_then(Value::as_str),
        Some("writeback_pending")
    );

    let written_back = run_business_os_ticket_command(
        &root,
        "ctox.ticket.writeback_comment",
        &json!({
            "case_id": case_id,
            "body": "Business OS command adapter writeback smoke.",
            "internal": false,
        }),
    )?;
    assert_eq!(
        written_back.pointer("/case/state").and_then(Value::as_str),
        Some("writeback_pending")
    );

    let closed = run_business_os_ticket_command(
        &root,
        "ctox.ticket.close",
        &json!({
            "case_id": case_id,
            "summary": "Closed through Business OS command adapter",
        }),
    )?;
    assert_eq!(
        closed.pointer("/case/state").and_then(Value::as_str),
        Some("closed")
    );

    let audit = list_audit_records(&root, Some(&ticket_key), 30)?;
    assert!(audit
        .iter()
        .any(|item| item.action_type == "approval_decision"));
    assert!(audit
        .iter()
        .any(|item| item.action_type == "execution_case"));
    assert!(audit
        .iter()
        .any(|item| item.action_type == "verification_record"));
    assert!(audit
        .iter()
        .any(|item| item.action_type == "writeback_record"));
    assert!(audit.iter().any(|item| item.action_type == "case_closed"));

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn clarification_request_round_trips_through_business_os_projection() -> Result<()> {
    let root = temp_root("ticket-clarification-business-os");
    std::fs::create_dir_all(&root)?;

    let remote = ticket_local_native::create_local_ticket(
        &root,
        "Missing VPN target",
        "Please connect me, but the VPN endpoint is not included.",
        Some("open"),
        Some("normal"),
    )?;
    sync_ticket_system(&root, "local")?;
    let ticket_key = format!("local:{}", remote.ticket_id);

    put_control_bundle(
        &root,
        ControlBundleInput {
            label: "support/vpn".to_string(),
            runbook_id: "rb-vpn".to_string(),
            runbook_version: "v1".to_string(),
            policy_id: "pol-vpn".to_string(),
            policy_version: "v1".to_string(),
            approval_mode: "direct_execute_allowed".to_string(),
            autonomy_level: "A1".to_string(),
            verification_profile_id: "verify-vpn".to_string(),
            writeback_profile_id: "writeback-comment".to_string(),
            support_mode: "support_case".to_string(),
            default_risk_level: "low".to_string(),
            execution_actions: default_execution_actions(),
            notes: Some("Clarification workflow test bundle".to_string()),
        },
    )?;
    set_ticket_label(
        &root,
        &ticket_key,
        "support/vpn",
        "test",
        Some("route to VPN support"),
        json!({}),
    )?;
    let dry_run = create_dry_run(&root, &ticket_key, Some("VPN request needs endpoint"), None)?;

    let requested = run_business_os_ticket_command(
        &root,
        "ctox.ticket.request_clarification",
        &json!({
            "case_id": dry_run.case_id,
            "question": "Which VPN endpoint should CTOX use?",
            "missing_inputs": ["vpn_endpoint"],
            "unblock_criteria": "Requester supplies the exact VPN endpoint.",
        }),
    )?;
    let clarification_id = requested
        .get("clarification_id")
        .and_then(Value::as_str)
        .context("clarification id missing")?
        .to_string();
    assert_eq!(
        requested
            .pointer("/clarification/status")
            .and_then(Value::as_str),
        Some("draft")
    );
    let blocked_case = load_case(&root, &dry_run.case_id)?.context("case missing")?;
    assert_eq!(blocked_case.state, "blocked_needs_clarification");

    let projection = business_os_ticket_projection_documents(&root, 50)?;
    let clarification_docs = projection
        .get("ctox_ticket_clarification_requests")
        .context("clarification projection missing")?;
    assert!(clarification_docs.iter().any(|doc| {
        doc.get("clarification_id").and_then(Value::as_str) == Some(clarification_id.as_str())
            && doc.get("status").and_then(Value::as_str) == Some("draft")
    }));

    let resolved = run_business_os_ticket_command(
        &root,
        "ctox.ticket.resolve_clarification",
        &json!({
            "clarification_id": clarification_id,
            "response_key": "manual:test-response",
            "body": "Use vpn.example.test.",
        }),
    )?;
    assert_eq!(
        resolved
            .pointer("/clarification/status")
            .and_then(Value::as_str),
        Some("resolved")
    );
    let resumed_case = load_case(&root, &dry_run.case_id)?.context("case missing")?;
    assert_eq!(resumed_case.state, "executable");

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn business_os_ticket_projection_reuses_one_ticket_db_connection() -> Result<()> {
    let root = temp_root("ticket-projection-single-db-open");
    std::fs::create_dir_all(&root)?;
    let db_path = resolve_db_path(&root);

    ticket_local_native::create_local_ticket(
        &root,
        "Projection connection reuse",
        "Project ticket state without reopening the ticket database per bucket.",
        Some("open"),
        Some("normal"),
    )?;
    sync_ticket_system(&root, "local")?;
    put_control_bundle(
        &root,
        ControlBundleInput {
            label: "support/projection".to_string(),
            runbook_id: "rb-projection".to_string(),
            runbook_version: "v1".to_string(),
            policy_id: "pol-projection".to_string(),
            policy_version: "v1".to_string(),
            approval_mode: "direct_execute_allowed".to_string(),
            autonomy_level: "A1".to_string(),
            verification_profile_id: "verify-projection".to_string(),
            writeback_profile_id: "writeback-comment".to_string(),
            support_mode: "support_case".to_string(),
            default_risk_level: "low".to_string(),
            execution_actions: default_execution_actions(),
            notes: Some("Projection connection reuse guard".to_string()),
        },
    )?;

    let before = ticket_db_open_call_count_for_tests(&db_path);
    let projection = business_os_ticket_projection_documents(&root, 50)?;
    assert!(
        projection
            .get("ctox_ticket_control_bundles")
            .is_some_and(|docs| !docs.is_empty()),
        "control bundles must still be projected"
    );
    assert_eq!(
        ticket_db_open_call_count_for_tests(&db_path) - before,
        1,
        "Business OS ticket projection must reuse its single ticket DB connection across buckets"
    );

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn published_clarification_resolves_from_later_inbound_ticket_comment() -> Result<()> {
    let root = temp_root("ticket-clarification-inbound");
    std::fs::create_dir_all(&root)?;

    let remote = ticket_local_native::create_local_ticket(
        &root,
        "Printer setup missing model",
        "Please configure the printer.",
        Some("open"),
        Some("normal"),
    )?;
    sync_ticket_system(&root, "local")?;
    let ticket_key = format!("local:{}", remote.ticket_id);
    put_control_bundle(
        &root,
        ControlBundleInput {
            label: "support/printer".to_string(),
            runbook_id: "rb-printer".to_string(),
            runbook_version: "v1".to_string(),
            policy_id: "pol-printer".to_string(),
            policy_version: "v1".to_string(),
            approval_mode: "direct_execute_allowed".to_string(),
            autonomy_level: "A1".to_string(),
            verification_profile_id: "verify-printer".to_string(),
            writeback_profile_id: "writeback-comment".to_string(),
            support_mode: "support_case".to_string(),
            default_risk_level: "low".to_string(),
            execution_actions: default_execution_actions(),
            notes: Some("Clarification auto-resolver test bundle".to_string()),
        },
    )?;
    set_ticket_label(
        &root,
        &ticket_key,
        "support/printer",
        "test",
        Some("route to printer support"),
        json!({}),
    )?;
    let dry_run = create_dry_run(
        &root,
        &ticket_key,
        Some("Printer request needs model"),
        None,
    )?;
    let clarification = create_ticket_clarification_request(
        &root,
        TicketClarificationRequestInput {
            case_id: Some(dry_run.case_id.clone()),
            ticket_key: None,
            work_id: None,
            target_type: "requester".to_string(),
            target_channel: "ticket".to_string(),
            question: "Which printer model should CTOX configure?".to_string(),
            missing_inputs: vec!["printer_model".to_string()],
            unblock_criteria: Some("Requester supplies the printer model.".to_string()),
            resume_state: "executable".to_string(),
            created_by: "ctox-test".to_string(),
            metadata: json!({}),
        },
    )?;
    let waiting = publish_ticket_clarification_request(
        &root,
        &clarification.clarification_id,
        "review-test",
        "Question is bounded and safe for requester.",
    )?;
    assert_eq!(waiting.status, "waiting_for_response");

    ticket_local_native::add_local_comment(
        &root,
        &remote.ticket_id,
        "The model is LaserJet 4100.",
    )?;
    let sync = sync_ticket_system(&root, "local")?;
    assert_eq!(
        sync.get("resolved_clarification_count")
            .and_then(Value::as_u64),
        Some(1)
    );
    let resolved = load_ticket_clarification_request(&root, &clarification.clarification_id)?
        .context("clarification missing")?;
    assert_eq!(resolved.status, "resolved");
    assert!(resolved
        .inbound_response_body
        .as_deref()
        .unwrap_or_default()
        .contains("LaserJet 4100"));
    let resumed_case = load_case(&root, &dry_run.case_id)?.context("case missing")?;
    assert_eq!(resumed_case.state, "executable");

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn autonomy_grant_controls_effective_ticket_execution_mode() -> Result<()> {
    let root = temp_root("autonomy");
    std::fs::create_dir_all(&root)?;

    let remote = ticket_local_native::create_local_ticket(
        &root,
        "Password reset request",
        "User requests a bounded password reset workflow.",
        Some("open"),
        Some("medium"),
    )?;
    sync_ticket_system(&root, "local")?;
    let ticket_key = format!("local:{}", remote.ticket_id);

    put_control_bundle(
        &root,
        ControlBundleInput {
            label: "support/password-reset".to_string(),
            runbook_id: "rb-password-reset".to_string(),
            runbook_version: "v2".to_string(),
            policy_id: "pol-password-reset".to_string(),
            policy_version: "v2".to_string(),
            approval_mode: "direct_execute_allowed".to_string(),
            autonomy_level: "A4".to_string(),
            verification_profile_id: "verify-password-reset".to_string(),
            writeback_profile_id: "writeback-comment".to_string(),
            support_mode: "service_request".to_string(),
            default_risk_level: "medium".to_string(),
            execution_actions: vec![
                "observe".to_string(),
                "analyze".to_string(),
                "draft_communication".to_string(),
                "remote_write".to_string(),
            ],
            notes: Some("Password reset bundle wants broad autonomy".to_string()),
        },
    )?;
    set_ticket_label(
        &root,
        &ticket_key,
        "support/password-reset",
        "manual",
        Some("service desk triage"),
        json!({"queue": "identity"}),
    )?;

    let first_dry_run = create_dry_run(&root, &ticket_key, Some("Bounded reset request"), None)?;
    let first_case =
        load_case(&root, &first_dry_run.case_id)?.context("first case missing after dry run")?;
    assert_eq!(first_case.state, "approval_pending");
    assert_eq!(first_case.approval_mode, "human_approval_required");
    assert_eq!(first_case.autonomy_level, "A0");
    assert_eq!(
        first_dry_run
            .artifact
            .get("autonomy_grant")
            .cloned()
            .unwrap_or(Value::Null),
        Value::Null
    );

    let first_case = decide_case_approval(
        &root,
        &first_case.case_id,
        "approved",
        "owner",
        Some("Initial supervised execution"),
    )?;
    let first_case = record_execution_action(
        &root,
        &first_case.case_id,
        "Prepared reset checklist and bounded operator plan",
    )?;
    let first_case = record_verification(
        &root,
        &first_case.case_id,
        "passed",
        Some("Checklist and verification evidence captured"),
    )?;

    let candidate = create_learning_candidate(
        &root,
        &first_case.case_id,
        "Observed password reset flow is stable and bounded",
        None,
        None,
    )?;
    assert_eq!(candidate.status, "proposed");
    let candidate = decide_learning_candidate(
        &root,
        &candidate.candidate_id,
        "approved",
        "owner",
        Some("Promote this runbook pattern"),
        Some("A3"),
    )?;
    assert_eq!(candidate.status, "approved");
    assert_eq!(candidate.promoted_autonomy_level.as_deref(), Some("A3"));

    let grant = put_autonomy_grant(
        &root,
        AutonomyGrantInput {
            label: "support/password-reset".to_string(),
            bundle_version: None,
            approval_mode: "bounded_auto_execute".to_string(),
            autonomy_level: "A3".to_string(),
            approved_by: "owner".to_string(),
            source_candidate_id: Some(candidate.candidate_id.clone()),
            rationale: Some("Approved bounded automation for this runbook".to_string()),
        },
    )?;
    assert_eq!(grant.approval_mode, "bounded_auto_execute");
    assert_eq!(grant.autonomy_level, "A3");

    let second_dry_run = create_dry_run(
        &root,
        &ticket_key,
        Some("Second identical request after grant"),
        None,
    )?;
    let second_case =
        load_case(&root, &second_dry_run.case_id)?.context("second case missing after dry run")?;
    assert_eq!(second_case.state, "executable");
    assert_eq!(second_case.approval_mode, "bounded_auto_execute");
    assert_eq!(second_case.autonomy_level, "A3");
    assert_eq!(
        second_dry_run
            .artifact
            .get("autonomy_grant")
            .and_then(|item| item.get("approved_by"))
            .and_then(Value::as_str),
        Some("owner")
    );

    let grants = list_autonomy_grants(&root)?;
    assert_eq!(grants.len(), 1);
    let candidates = list_learning_candidates(&root, Some("support/password-reset"), None, 8)?;
    assert_eq!(candidates.len(), 1);

    let audit = list_audit_records(&root, Some(&ticket_key), 40)?;
    assert!(audit
        .iter()
        .any(|item| item.action_type == "learning_candidate"));
    assert!(audit
        .iter()
        .any(|item| item.action_type == "learning_candidate_decision"));
    let control_audit = list_audit_records(&root, Some("*autonomy-grant*"), 20)?;
    assert!(control_audit
        .iter()
        .any(|item| item.action_type == "autonomy_grant_change"));

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn first_attach_baselines_existing_ticket_events_but_routes_new_ones() -> Result<()> {
    let root = temp_root("attach-baseline");
    std::fs::create_dir_all(&root)?;

    let remote = ticket_local_native::create_local_ticket(
        &root,
        "Existing helpdesk backlog item",
        "This ticket existed before CTOX was attached.",
        Some("open"),
        Some("medium"),
    )?;
    ticket_local_native::add_local_comment(
        &root,
        &remote.ticket_id,
        "Historic conversation before CTOX attach",
    )?;

    let first_sync = sync_ticket_system(&root, "local")?;
    assert_eq!(first_sync.get("ok").and_then(Value::as_bool), Some(true));
    assert_eq!(
        first_sync
            .get("source_control")
            .and_then(|item| item.get("adoption_mode"))
            .and_then(Value::as_str),
        Some("baseline_observe_only")
    );

    let source_controls = list_ticket_source_controls(&root)?;
    assert_eq!(source_controls.len(), 1);
    assert_eq!(source_controls[0].source_system, "local");

    let initially_leased = lease_pending_ticket_events(&root, 20, "attach-test")?;
    assert!(
        initially_leased.is_empty(),
        "existing backlog must be baselined on first attach instead of entering active routing"
    );

    ticket_local_native::add_local_comment(
        &root,
        &remote.ticket_id,
        "Fresh update after CTOX attach",
    )?;
    sync_ticket_system(&root, "local")?;

    let leased_after_new_comment = lease_pending_ticket_events(&root, 20, "attach-test")?;
    assert_eq!(leased_after_new_comment.len(), 1);
    assert_eq!(leased_after_new_comment[0].event_type, "comment");
    assert_eq!(
        leased_after_new_comment[0].body_text,
        "Fresh update after CTOX attach"
    );

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn sync_bootstraps_knowledge_but_not_self_work_for_ticket_sources() -> Result<()> {
    let root = temp_root("knowledge");
    std::fs::create_dir_all(&root)?;

    let remote = ticket_local_native::create_local_ticket(
        &root,
        "[VPN] host vpn-gateway-01 unreachable",
        "Users cannot reach vpn-gateway-01 after the overnight maintenance window.",
        Some("open"),
        Some("high"),
    )?;
    let sync = sync_ticket_system(&root, "local")?;
    assert_eq!(sync.get("self_work_count").and_then(Value::as_u64), Some(0));

    let knowledge = list_ticket_knowledge_entries(&root, Some("local"), None, None, 20)?;
    assert!(knowledge
        .iter()
        .any(|entry| entry.domain == "source_profile"));
    assert!(knowledge.iter().any(|entry| entry.domain == "glossary"));
    assert!(knowledge.iter().any(|entry| entry.domain == "access_model"));
    assert!(knowledge
        .iter()
        .any(|entry| entry.domain == "monitoring_landscape"));

    let load = create_ticket_knowledge_load(&root, &format!("local:{}", remote.ticket_id), None)?;
    assert_eq!(load.status, "ready");
    assert!(load.gap_domains.is_empty());

    let item = put_ticket_self_work_item(
        &root,
        TicketSelfWorkUpsertInput {
            source_system: "local".to_string(),
            kind: "system-onboarding".to_string(),
            title: "Review current helpdesk working model".to_string(),
            body_text: "Review the observed operating model and propose the next adoption steps."
                .to_string(),
            state: "open".to_string(),
            metadata: json!({
                "skill": "system-onboarding",
                "phase": "observe",
            }),
        },
        true,
    )?;
    assert_eq!(item.kind, "system-onboarding");
    assert_eq!(item.state, "published");
    assert!(item.remote_ticket_id.is_some());

    let listed = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
    assert_eq!(listed.len(), 1);

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn monitoring_ingest_persists_generic_monitoring_knowledge() -> Result<()> {
    let root = temp_root("monitoring");
    std::fs::create_dir_all(&root)?;

    let entry = put_ticket_knowledge_entry(
        &root,
        TicketKnowledgeUpsertInput {
            source_system: "local".to_string(),
            domain: "monitoring_landscape".to_string(),
            knowledge_key: "prometheus".to_string(),
            title: "Prometheus overview".to_string(),
            summary: summarize_monitoring_snapshot(&json!({
                "sources": [{"name": "prometheus"}],
                "services": [{"name": "vpn"}],
                "alerts": [{"name": "vpn-down"}],
            })),
            status: "observed".to_string(),
            content: json!({
                "sources": [{"name": "prometheus"}],
                "services": [{"name": "vpn"}],
                "alerts": [{"name": "vpn-down"}],
            }),
        },
    )?;
    assert_eq!(entry.domain, "monitoring_landscape");
    assert_eq!(entry.knowledge_key, "prometheus");
    assert!(entry.summary.contains("1 sources"));

    let loaded = load_ticket_knowledge_entry(&root, "local", "monitoring_landscape", "prometheus")?
        .context("monitoring entry missing")?;
    assert_eq!(loaded.status, "observed");

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn attached_source_without_active_binding_defaults_to_onboarding_skill() -> Result<()> {
    let root = temp_root("ticket-onboarding-default-skill");
    std::fs::create_dir_all(&root)?;

    let _remote = crate::mission::ticket_local_native::create_local_ticket(
        &root,
        "Erste Desk-Anbindung",
        "Der lokale Desk ist frisch verbunden und noch ohne aktive Desk-Skill-Bindung.",
        Some("open"),
        Some("normal"),
    )?;
    sync_ticket_system(&root, "local")?;

    assert_eq!(
        preferred_skill_for_ticket_source(&root, "local")?,
        Some("system-onboarding".to_string())
    );

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn access_request_self_work_keeps_secret_refs_outside_ticket_truth() -> Result<()> {
    let root = temp_root("access-request");
    std::fs::create_dir_all(&root)?;

    let item = put_ticket_self_work_item(
        &root,
        TicketSelfWorkUpsertInput {
            source_system: "local".to_string(),
            kind: "access-request".to_string(),
            title: "Need monitoring access for onboarding".to_string(),
            body_text: "Please grant read access to monitoring and provide references to the required tokens."
                .to_string(),
            state: "open".to_string(),
            metadata: json!({
                "skill": "ticket-access-and-secrets",
                "required_scopes": ["monitoring.read", "ticket.transition"],
                "secret_refs": ["secret:monitoring/prometheus-api-token"],
                "channels": ["mail", "jami"],
            }),
        },
        false,
    )?;
    assert_eq!(item.kind, "access-request");
    assert_eq!(
        item.suggested_skill.as_deref(),
        Some("ticket-access-and-secrets")
    );
    assert_eq!(
        item.metadata
            .get("secret_refs")
            .and_then(Value::as_array)
            .map(|items| items.len()),
        Some(1)
    );
    assert!(item.remote_ticket_id.is_none());

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn ticket_self_work_list_cache_reuses_idle_reads_until_store_changes() -> Result<()> {
    let root = temp_root("self-work-list-cache");
    std::fs::create_dir_all(&root)?;
    let db_path = resolve_db_path(&root);
    let _ = open_ticket_db(&root)?;

    let first = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
    assert!(first.is_empty());
    assert_eq!(
        ticket_self_work_list_cache_miss_count_for_tests(&db_path, Some("local"), None, 10),
        1,
        "first self-work list must hit SQLite"
    );

    let second = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
    assert!(second.is_empty());
    assert_eq!(
        ticket_self_work_list_cache_miss_count_for_tests(&db_path, Some("local"), None, 10),
        1,
        "unchanged idle self-work list must reuse the cached snapshot"
    );

    let created = put_ticket_self_work_item(
        &root,
        TicketSelfWorkUpsertInput {
            source_system: "local".to_string(),
            kind: "cache-test".to_string(),
            title: "Refresh self-work snapshot after write".to_string(),
            body_text: "The idle list cache must notice self-work writes.".to_string(),
            state: "open".to_string(),
            metadata: json!({
                "skill": "cache-test",
                "dedupe_key": "self-work-list-cache",
            }),
        },
        false,
    )?;

    let after_write = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
    assert_eq!(after_write.len(), 1);
    assert_eq!(after_write[0].work_id, created.work_id);
    assert_eq!(
        ticket_self_work_list_cache_miss_count_for_tests(&db_path, Some("local"), None, 10),
        2,
        "self-work writes must invalidate the cached snapshot"
    );

    let after_idle = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
    assert_eq!(after_idle.len(), 1);
    assert_eq!(after_idle[0].work_id, created.work_id);
    assert_eq!(
        ticket_self_work_list_cache_miss_count_for_tests(&db_path, Some("local"), None, 10),
        2,
        "unchanged post-write self-work list must reuse the refreshed snapshot"
    );

    assign_ticket_self_work_item(
        &root,
        &created.work_id,
        "ctox-core",
        "cache-test",
        Some("exercise assignment hydration invalidation"),
    )?;

    let after_assignment = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
    assert_eq!(after_assignment.len(), 1);
    assert_eq!(
        after_assignment[0].assigned_to.as_deref(),
        Some("ctox-core")
    );
    assert_eq!(
        ticket_self_work_list_cache_miss_count_for_tests(&db_path, Some("local"), None, 10),
        3,
        "assignment writes must invalidate hydrated self-work snapshots"
    );

    let after_assignment_idle = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
    assert_eq!(after_assignment_idle.len(), 1);
    assert_eq!(
        after_assignment_idle[0].assigned_to.as_deref(),
        Some("ctox-core")
    );
    assert_eq!(
        ticket_self_work_list_cache_miss_count_for_tests(&db_path, Some("local"), None, 10),
        3,
        "unchanged assigned self-work list must stay cached"
    );

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn ticket_self_work_list_batches_latest_assignment_hydration() -> Result<()> {
    let root = temp_root("self-work-assignment-batch-hydration");
    std::fs::create_dir_all(&root)?;
    let before = ticket_self_work_assignment_batch_hydration_call_count_for_tests();

    let mut expected = BTreeMap::new();
    for index in 0..5 {
        let item = put_ticket_self_work_item(
            &root,
            TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: "batch-hydration".to_string(),
                title: format!("Batch hydrate {index}"),
                body_text: "Hydrate assignment in one set-based pass.".to_string(),
                state: "open".to_string(),
                metadata: json!({
                    "skill": "batch-hydration",
                    "dedupe_key": format!("self-work-assignment-batch-{index}"),
                }),
            },
            false,
        )?;
        let assignee = format!("ctox-agent-{index}");
        assign_ticket_self_work_item(
            &root,
            &item.work_id,
            &assignee,
            "batch-test",
            Some("exercise batch assignment hydration"),
        )?;
        expected.insert(item.work_id, assignee);
    }

    let items = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
    assert_eq!(items.len(), 5);
    for item in &items {
        assert_eq!(
            item.assigned_to.as_deref(),
            expected.get(&item.work_id).map(String::as_str)
        );
    }
    assert_eq!(
        ticket_self_work_assignment_batch_hydration_call_count_for_tests() - before,
        1,
        "self-work list assignment hydration must be one batch query, not one query per item"
    );

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn ticket_workflow_materialize_cache_reuses_idle_noop_until_store_changes() -> Result<()> {
    let root = temp_root("workflow-materialize-cache");
    std::fs::create_dir_all(&root)?;
    let db_path = resolve_db_path(&root);
    let _ = open_ticket_db(&root)?;

    let first = materialize_ready_workflow_steps(&root, 8)?;
    assert_eq!(first.materialized_count, 0);
    assert_eq!(
        ticket_workflow_materialize_cache_miss_count_for_tests(&db_path, None, 8),
        1,
        "first workflow materialize pass must inspect SQLite"
    );

    let second = materialize_ready_workflow_steps(&root, 8)?;
    assert_eq!(second.materialized_count, 0);
    assert_eq!(
        ticket_workflow_materialize_cache_miss_count_for_tests(&db_path, None, 8),
        1,
        "unchanged idle workflow materialize pass must reuse the no-op cache"
    );

    let workflow = start_ticket_workflow(
        &root,
        TicketWorkflowStartInput {
            source_system: "internal".to_string(),
            title: "Exercise workflow materialize cache".to_string(),
            goal: "Create a ready step so the cache must refresh after writes.".to_string(),
            thread_key: Some("workflow/cache".to_string()),
            workspace_root: None,
            skill: None,
            priority: Some("normal".to_string()),
            first_phase: "plan".to_string(),
            first_phase_goal: None,
            first_exit_gate: None,
            first_step_title: None,
            first_step_prompt: None,
            queue_now: false,
        },
    )?;
    assert_eq!(workflow.ready_steps, vec!["phase-0-reducer".to_string()]);

    let materialized = materialize_ready_workflow_steps(&root, 8)?;
    assert_eq!(materialized.materialized_count, 1);
    assert_eq!(
        ticket_workflow_materialize_cache_miss_count_for_tests(&db_path, None, 8),
        2,
        "workflow writes must invalidate the previous no-op cache"
    );

    let after_materialized = materialize_ready_workflow_steps(&root, 8)?;
    assert_eq!(after_materialized.materialized_count, 0);
    assert_eq!(
        ticket_workflow_materialize_cache_miss_count_for_tests(&db_path, None, 8),
        3,
        "first post-materialization no-op pass must refresh the cache"
    );

    let after_idle = materialize_ready_workflow_steps(&root, 8)?;
    assert_eq!(after_idle.materialized_count, 0);
    assert_eq!(
        ticket_workflow_materialize_cache_miss_count_for_tests(&db_path, None, 8),
        3,
        "unchanged post-materialization no-op pass must stay cached"
    );

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn access_request_command_defaults_to_access_and_secrets_skill() -> Result<()> {
    let root = temp_root("access-request-command");
    std::fs::create_dir_all(&root)?;

    handle_ticket_command(
        &root,
        &[
            "access-request-put".to_string(),
            "--system".to_string(),
            "local".to_string(),
            "--title".to_string(),
            "Need admin approval for access request".to_string(),
            "--body".to_string(),
            "Please confirm whether CTOX may handle password reset tickets autonomously."
                .to_string(),
        ],
    )?;

    let items = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].kind, "access-request");
    assert_eq!(
        items[0].suggested_skill.as_deref(),
        Some("ticket-access-and-secrets")
    );

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn self_work_put_accepts_explicit_skill_hint() -> Result<()> {
    let root = temp_root("self-work-skill");
    std::fs::create_dir_all(&root)?;

    handle_ticket_command(
        &root,
        &[
            "self-work-put".to_string(),
            "--system".to_string(),
            "local".to_string(),
            "--kind".to_string(),
            "secret-hygiene".to_string(),
            "--title".to_string(),
            "Protect leaked API token".to_string(),
            "--body".to_string(),
            "Move the pasted API token into the encrypted store and rewrite memory.".to_string(),
            "--skill".to_string(),
            "secret-hygiene".to_string(),
        ],
    )?;

    let items = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].kind, "secret-hygiene");
    assert_eq!(items[0].suggested_skill.as_deref(), Some("secret-hygiene"));

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn internal_work_put_alias_accepts_explicit_skill_hint() -> Result<()> {
    let root = temp_root("internal-work-skill");
    std::fs::create_dir_all(&root)?;

    handle_ticket_command(
        &root,
        &[
            "internal-work-put".to_string(),
            "--system".to_string(),
            "local".to_string(),
            "--kind".to_string(),
            "secret-hygiene".to_string(),
            "--title".to_string(),
            "Protect leaked API token".to_string(),
            "--body".to_string(),
            "Move the pasted API token into the encrypted store and rewrite memory.".to_string(),
            "--skill".to_string(),
            "secret-hygiene".to_string(),
        ],
    )?;

    let items = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].kind, "secret-hygiene");
    assert_eq!(items[0].suggested_skill.as_deref(), Some("secret-hygiene"));

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn ticket_workflow_delta_materializes_successor_after_verified_predecessor() -> Result<()> {
    let root = temp_root("ticket-workflow-delta");
    std::fs::create_dir_all(&root)?;

    let workflow = start_ticket_workflow(
        &root,
        TicketWorkflowStartInput {
            source_system: "internal".to_string(),
            title: "Stabilize long task handling".to_string(),
            goal: "Break a hard implementation into verifiable CTOX ticket phases.".to_string(),
            thread_key: Some("workflow/test".to_string()),
            workspace_root: None,
            skill: None,
            priority: Some("normal".to_string()),
            first_phase: "plan".to_string(),
            first_phase_goal: Some("Create the first executable phase".to_string()),
            first_exit_gate: Some("Reducer produces a bounded implementation step".to_string()),
            first_step_title: None,
            first_step_prompt: None,
            queue_now: false,
        },
    )?;
    assert_eq!(workflow.steps.len(), 1);
    assert_eq!(workflow.ready_steps, vec!["phase-0-reducer".to_string()]);

    let delta = json!({
        "phase_decision": "advance",
        "update_steps": [{"step_id": "phase-0-reducer", "workflow_step_status": "verified", "evidence": {"summary": "planning complete"}}],
        "create_steps": [{"step_id": "implement-one-slice", "phase": "implementation", "role": "leaf", "title": "Implement one bounded slice", "prompt": "Make one scoped code change and report evidence.", "predecessor_steps": ["phase-0-reducer"], "exit_gate": "Focused check passes or blocker is recorded", "priority": "normal"}]
    });
    let _ = apply_ticket_workflow_delta(&root, &workflow.workflow_id, delta, false)?;
    let view = load_ticket_workflow(&root, &workflow.workflow_id)?.context("workflow missing")?;
    assert!(view
        .ready_steps
        .contains(&"implement-one-slice".to_string()));

    let materialized =
        materialize_ready_workflow_steps_for_workflow(&root, Some(&workflow.workflow_id), 8)?;
    assert_eq!(materialized.materialized_count, 1);
    assert_eq!(
        materialized.materialized[0].assigned_to.as_deref(),
        Some("self")
    );
    assert_eq!(materialized.materialized[0].state, "queued");

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn self_work_items_allow_multiple_entries_for_same_kind_when_not_deduped() -> Result<()> {
    let root = temp_root("self-work-multi-kind");
    std::fs::create_dir_all(&root)?;

    let first = put_ticket_self_work_item(
        &root,
        TicketSelfWorkUpsertInput {
            source_system: "internal".to_string(),
            kind: "queue-overflow".to_string(),
            title: "Queue spill: monitoring drift".to_string(),
            body_text: "First queue spill body".to_string(),
            state: "spilled".to_string(),
            metadata: json!({
                "queue_message_key": "queue:one",
            }),
        },
        false,
    )?;
    let second = put_ticket_self_work_item(
        &root,
        TicketSelfWorkUpsertInput {
            source_system: "internal".to_string(),
            kind: "queue-overflow".to_string(),
            title: "Queue spill: alert storm".to_string(),
            body_text: "Second queue spill body".to_string(),
            state: "spilled".to_string(),
            metadata: json!({
                "queue_message_key": "queue:two",
            }),
        },
        false,
    )?;

    assert_ne!(first.work_id, second.work_id);
    let conn = open_ticket_db(&root)?;
    let first_spawn_count: i64 = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM ctox_core_spawn_edges
        WHERE child_entity_type = 'WorkItem'
          AND child_entity_id = ?1
          AND spawn_kind = 'self-work:queue-overflow'
          AND parent_entity_type = 'QueueTask'
          AND accepted = 1
        "#,
        params![&first.work_id],
        |row| row.get(0),
    )?;
    let second_spawn_count: i64 = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM ctox_core_spawn_edges
        WHERE child_entity_type = 'WorkItem'
          AND child_entity_id = ?1
          AND spawn_kind = 'self-work:queue-overflow'
          AND parent_entity_type = 'QueueTask'
          AND accepted = 1
        "#,
        params![&second.work_id],
        |row| row.get(0),
    )?;
    assert_eq!(first_spawn_count, 1);
    assert_eq!(second_spawn_count, 1);
    let listed = list_ticket_self_work_items(&root, Some("internal"), None, 10)?;
    let overflow_count = listed
        .iter()
        .filter(|item| item.kind == "queue-overflow")
        .count();
    assert_eq!(overflow_count, 2);

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn self_work_status_reads_aliases_and_rejects_unknown_persisted_values() -> Result<()> {
    let root = temp_root("self-work-status-persistence");
    std::fs::create_dir_all(&root)?;

    let item = put_ticket_self_work_item(
        &root,
        TicketSelfWorkUpsertInput {
            source_system: "local".to_string(),
            kind: "status-persistence".to_string(),
            title: "Validate persisted work-item status".to_string(),
            body_text: "Read aliases canonically and reject unknown values.".to_string(),
            state: "open".to_string(),
            metadata: json!({"dedupe_key": "self-work-status-persistence"}),
        },
        false,
    )?;
    let conn = open_ticket_db(&root)?;
    conn.execute(
        "UPDATE ticket_self_work_items SET state = 'completed' WHERE work_id = ?1",
        params![&item.work_id],
    )?;
    drop(conn);

    let loaded = load_ticket_self_work_item(&root, &item.work_id)?
        .context("work item missing after alias update")?;
    assert_eq!(loaded.state, "closed");

    let conn = open_ticket_db(&root)?;
    conn.execute(
        "UPDATE ticket_self_work_items SET state = 'mystery' WHERE work_id = ?1",
        params![&item.work_id],
    )?;
    drop(conn);
    let err = load_ticket_self_work_item(&root, &item.work_id)
        .expect_err("unknown persisted state must fail closed");
    assert!(err
        .to_string()
        .contains("unknown persisted ticket work-item status `mystery`"));

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn review_spawn_budget_is_scoped_to_parent_work_episode() {
    let first = json!({
        "thread_key": "email/shared-thread",
        "parent_work_id": "work-episode-1"
    });
    let second = json!({
        "thread_key": "email/shared-thread",
        "parent_work_id": "work-episode-2"
    });
    let (first_key, first_budget) =
        ticket_self_work_spawn_budget("completion-review-rework", "email/shared-thread", &first);
    let (second_key, second_budget) =
        ticket_self_work_spawn_budget("completion-review-rework", "email/shared-thread", &second);

    assert_ne!(first_key, second_key);
    assert!(first_key.ends_with("episode:work-episode-1"));
    assert!(second_key.ends_with("episode:work-episode-2"));
    assert_eq!(first_budget, 5);
    assert_eq!(second_budget, 5);
}

#[test]
fn queue_parent_cannot_spawn_strategy_direction_self_work() -> Result<()> {
    let root = temp_root("queue-strategy-self-work-core-rejected");
    std::fs::create_dir_all(&root)?;

    let err = put_ticket_self_work_item(
        &root,
        TicketSelfWorkUpsertInput {
            source_system: "local".to_string(),
            kind: "strategic-direction-pass".to_string(),
            title: "Strategic direction setup".to_string(),
            body_text: "Establish strategy before benchmark work.".to_string(),
            state: "open".to_string(),
            metadata: json!({
                "thread_key": "queue/normal-work",
                "source_label": "queue",
                "workspace_root": "/tmp/ctox-workspace",
                "dedupe_key": "strategy-direction:queue/normal-work",
            }),
        },
        false,
    )
    .expect_err("queue execution must not spawn strategic-direction self-work");
    assert!(err.to_string().contains("core spawn gate rejected"));

    let conn = open_ticket_db(&root)?;
    let rejected_edges: i64 = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM ctox_core_spawn_edges
        WHERE spawn_kind = 'self-work:strategic-direction-pass'
          AND accepted = 0
        "#,
        [],
        |row| row.get(0),
    )?;
    assert_eq!(rejected_edges, 1);
    let violation_codes_json: String = conn.query_row(
        r#"
        SELECT violation_codes_json
        FROM ctox_core_spawn_edges
        WHERE spawn_kind = 'self-work:strategic-direction-pass'
          AND accepted = 0
        LIMIT 1
        "#,
        [],
        |row| row.get(0),
    )?;
    assert!(violation_codes_json.contains("strategy_direction_spawn_for_queue_execution"));

    let items = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].state, "blocked");

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn source_skill_binding_can_be_listed_and_guides_live_ticket_skill_selection() -> Result<()> {
    let root = temp_root("source-skill-binding");
    std::fs::create_dir_all(&root)?;

    let binding = put_ticket_source_skill_binding(
        &root,
        "local",
        "roller-ticket-desk-operator-v4",
        "operating-model",
        "active",
        "ticket-onboarding",
        Some("runtime/generated-skills/roller-ticket-desk-operator-v4"),
        Some("Use the generated desk skill for live local ticket routing."),
    )?;
    assert_eq!(binding.source_system, "local");
    assert_eq!(binding.skill_name, "roller-ticket-desk-operator-v4");

    let listed = list_ticket_source_skill_bindings(&root, Some("local"))?;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].skill_name, "roller-ticket-desk-operator-v4");

    let suggested = suggested_skill_for_live_ticket_source(
        &root,
        &RoutedTicketEvent {
            event_key: "evt-1".to_string(),
            ticket_key: "local:123".to_string(),
            source_system: "local".to_string(),
            remote_event_id: "comment-1".to_string(),
            event_type: "comment".to_string(),
            summary: "Please continue with the MHS lock investigation.".to_string(),
            body_text: "The user is still locked after the password reset.".to_string(),
            title: "Sperrung MHS Benutzer".to_string(),
            remote_status: "open".to_string(),
            label: "support/access".to_string(),
            bundle_label: "support/access".to_string(),
            bundle_version: 1,
            case_id: "case-1".to_string(),
            dry_run_id: "dry-1".to_string(),
            dry_run_artifact: json!({}),
            support_mode: "support_case".to_string(),
            approval_mode: "human_approval_required".to_string(),
            autonomy_level: "A0".to_string(),
            risk_level: "unknown".to_string(),
            thread_key: "ticket:local:123".to_string(),
        },
    )?;
    assert_eq!(suggested.as_deref(), Some("roller-ticket-desk-operator-v4"));

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn history_export_writes_canonical_jsonl_from_mirrored_tickets() -> Result<()> {
    let root = temp_root("history-export");
    std::fs::create_dir_all(&root)?;

    let remote = ticket_local_native::create_local_ticket(
        &root,
        "[VPN] host vpn-gateway-01 unreachable",
        "Users cannot reach vpn-gateway-01 after the overnight maintenance window.",
        Some("open"),
        Some("high"),
    )?;
    ticket_local_native::add_local_comment(
        &root,
        &remote.ticket_id,
        "Please verify whether the tunnel service restarted cleanly.",
    )?;
    sync_ticket_system(&root, "local")?;

    let output = root.join("runtime/history/local-history.jsonl");
    let result = export_ticket_history_dataset(&root, "local", &output)?;
    assert_eq!(result.get("record_count").and_then(Value::as_u64), Some(1));
    let content = std::fs::read_to_string(&output)?;
    let first_line = content.lines().next().context("missing exported row")?;
    let row: Value = serde_json::from_str(first_line)?;
    assert_eq!(
        row.get("ticket_id").and_then(Value::as_str),
        Some(remote.ticket_id.as_str())
    );
    assert_eq!(
        row.get("title").and_then(Value::as_str),
        Some("[VPN] host vpn-gateway-01 unreachable")
    );
    assert_eq!(
        row.get("request_type").and_then(Value::as_str),
        Some("ticket")
    );
    assert_eq!(row.get("category").and_then(Value::as_str), Some("general"));
    assert!(row
        .get("request_text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .contains("vpn-gateway-01"));
    assert!(row
        .get("action_text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .contains("Please verify"));
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn history_export_skips_ctox_self_work_and_legacy_internal_tickets() -> Result<()> {
    let root = temp_root("history-export-filters-self-work");
    std::fs::create_dir_all(&root)?;

    let remote = ticket_local_native::create_local_ticket(
        &root,
        "VPN Benutzer kann sich nicht anmelden",
        "Benutzer kann sich nach Passwortwechsel nicht am VPN anmelden.",
        Some("open"),
        Some("high"),
    )?;

    let _work = put_ticket_self_work_item(
        &root,
        TicketSelfWorkUpsertInput {
            source_system: "local".to_string(),
            kind: "system-onboarding".to_string(),
            title: "CTOX: Ticket system onboarding".to_string(),
            body_text: "Visible onboarding work item for routing validation.".to_string(),
            state: "open".to_string(),
            metadata: json!({"skill": "system-onboarding"}),
        },
        true,
    )?;

    ticket_local_native::create_local_ticket(
        &root,
        "CTOX: legacy onboarding note",
        "Review the attached ticket system and generate onboarding work.",
        Some("closed"),
        Some("normal"),
    )?;

    sync_ticket_system(&root, "local")?;

    let output = root.join("runtime/history/local-history-filtered.jsonl");
    let result = export_ticket_history_dataset(&root, "local", &output)?;
    assert_eq!(result.get("record_count").and_then(Value::as_u64), Some(1));
    let content = std::fs::read_to_string(&output)?;
    let exported_rows: Vec<Value> = content
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    assert_eq!(exported_rows.len(), 1);
    assert_eq!(
        exported_rows[0].get("ticket_id").and_then(Value::as_str),
        Some(remote.ticket_id.as_str())
    );

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn source_skill_show_and_query_use_bound_operating_model_artifact() -> Result<()> {
    let root = temp_root("source-skill-query");
    std::fs::create_dir_all(&root)?;
    let skill_dir = root.join("runtime/generated-skills/demo-skill");
    let generated_dir = skill_dir.join("references/generated");
    std::fs::create_dir_all(&generated_dir)?;
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "# Demo Desk Skill\n\nUse this for desk work.\n\n## How To Handle A New Ticket\n\nQuery historical families first.\n",
    )?;
    std::fs::write(
        generated_dir.join("family_playbooks.json"),
        serde_json::to_string_pretty(&vec![json!({
            "family_key": "access :: identity :: mhs",
            "signals": {
                "token_signals": ["MHS", "Sperrung"],
                "common_phrases": ["mhs benutzer", "benutzer gesperrt"]
            },
            "usual_handling": {
                "dominant_channels": [["email", 4]],
                "dominant_states": [["open", 4]],
                "actions_seen": ["entsperrt"],
                "closure_tendency": 0.75
            },
            "decision_support": {
                "mode": "access_change",
                "operator_summary": "This desk handles MHS user locks as access work.",
                "triage_focus": ["identify the locked user"],
                "handling_steps": ["confirm the affected MHS identity", "unlock only after identity is clear"],
                "close_when": "Close when the user can sign in again.",
                "caution_signals": ["do not unlock the wrong account"],
                "note_guidance": "Record the affected identity and whether retry worked."
            },
            "historical_examples": {
                "canonical": [{"ticket_id": "100", "title": "Sperrung MHS Benutzer", "why": "Representative historical case."}]
            }
        })])?
            + "\n",
    )?;
    std::fs::write(
        generated_dir.join("retrieval_index.jsonl"),
        serde_json::to_string(&json!({
            "card_id": "family:1",
            "card_type": "family_playbook",
            "family_key": "access :: identity :: mhs",
            "request_type": "access",
            "category": "identity",
            "subcategory": "mhs",
            "text": "access identity mhs benutzer sperrung entsperrt"
        }))? + "\n",
    )?;
    put_ticket_source_skill_binding(
        &root,
        "local",
        "demo-skill",
        "operating-model",
        "active",
        "test",
        Some("runtime/generated-skills/demo-skill"),
        Some("test binding"),
    )?;

    let shown = show_ticket_source_skill(&root, "local")?;
    assert_eq!(shown.binding.skill_name, "demo-skill");
    assert!(shown
        .skill_preview
        .unwrap_or_default()
        .contains("Demo Desk Skill"));

    let queried = query_ticket_source_skill(
        &root,
        "local",
        "Benutzer ist im MHS gesperrt und braucht Entsperrung.",
        1,
    )?;
    let top_family = queried
        .get("result")
        .and_then(|value| value.get("families"))
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("family_key"))
        .and_then(Value::as_str);
    assert_eq!(top_family, Some("access :: identity :: mhs"));

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn skillbook_runbook_bundle_can_drive_reply_flow_for_ticket_case() -> Result<()> {
    let root = temp_root("source-skill-runbook-reply");
    std::fs::create_dir_all(&root)?;

    let bundle_dir = root.join("runtime/generated-skills/eventus-email-main");
    std::fs::create_dir_all(&bundle_dir)?;
    std::fs::write(
        bundle_dir.join("main_skill.json"),
        serde_json::to_string_pretty(&json!({
            "main_skill_id": "eventus.email.support.main.v1",
            "title": "Eventus Email Support Main",
            "primary_channel": "email",
            "entry_action": "resolve_runbook_item",
            "resolver_contract": {"mode": "runbook-item"},
            "execution_contract": {"mode": "reply-only"},
            "resolve_flow": [
                "resolve the best matching runbook item",
                "load the linked skillbook",
                "compose a reply suggestion"
            ],
            "writeback_flow": [
                "verify reply",
                "write public comment back to the ticket"
            ],
            "linked_skillbooks": ["eventus.email.support.v1"],
            "linked_runbooks": ["eventus.runbook.registration.v1"]
        }))?,
    )?;
    std::fs::write(
        bundle_dir.join("skillbook.json"),
        serde_json::to_string_pretty(&json!({
            "skillbook_id": "eventus.email.support.v1",
            "title": "Eventus Email Support",
            "version": "v1",
            "mission": "Handle incoming support emails safely and clearly.",
            "non_negotiable_rules": [
                "Never invent product behavior.",
                "Keep the answer aligned with the manual."
            ],
            "runtime_policy": "Resolve a runbook item first, then draft the reply.",
            "answer_contract": "Give a concise, actionable email answer.",
            "workflow_backbone": [
                "identify the request",
                "load the runbook item",
                "reply only from the runbook facts"
            ],
            "routing_taxonomy": ["registration", "login"],
            "linked_runbooks": ["eventus.runbook.registration.v1"]
        }))?,
    )?;
    std::fs::write(
        bundle_dir.join("runbook.json"),
        serde_json::to_string_pretty(&json!({
            "runbook_id": "eventus.runbook.registration.v1",
            "skillbook_id": "eventus.email.support.v1",
            "title": "Registration issues",
            "version": "v1",
            "status": "active",
            "problem_domain": "registration",
            "item_labels": ["REG-03"]
        }))?,
    )?;
    std::fs::write(
        bundle_dir.join("runbook_items.jsonl"),
        serde_json::to_string(&json!({
            "item_id": "eventus.runbook.reg.03.v1",
            "runbook_id": "eventus.runbook.registration.v1",
            "skillbook_id": "eventus.email.support.v1",
            "label": "REG-03",
            "title": "Password is rejected during registration",
            "problem_class": "registration.password_policy",
            "trigger_phrases": [
                "password is not accepted",
                "registration password",
                "what password rules apply"
            ],
            "entry_conditions": [
                "user is in the registration flow"
            ],
            "earliest_blocker": "Password does not satisfy the registration password policy.",
            "expected_guidance": "Please check whether your password has at least 6 characters and contains one uppercase letter, one lowercase letter and one digit. Avoid easily guessable personal data. If the password still gets rejected although it matches these rules, reply to this email and we will investigate further.",
            "tool_actions": {
                "kind": "reply_only",
                "tools": []
            },
            "verification": [
                "reply references the documented password rules"
            ],
            "writeback_policy": {
                "channel": "public_reply"
            },
            "escalate_when": [
                "a formally valid password is still rejected"
            ],
            "sources": {
                "manual": "Supplier manual - E.VENT.US_en (demo manual)"
            },
            "pages": ["8"],
            "chunk_text": "REG-03 registration password rejected password policy one uppercase one lowercase one digit minimum 6 characters"
        }))? + "\n",
    )?;

    let imported = import_ticket_source_skill_bundle(
        &root,
        "local",
        bundle_dir.to_str().context("bundle path utf-8")?,
        None,
        true,
    )?;
    assert_eq!(
        imported.get("embeddings_indexed").and_then(Value::as_bool),
        Some(false)
    );

    let remote = ticket_local_native::create_local_ticket(
        &root,
        "Registration password rejected",
        "Hello, during registration my password is not accepted. Which password rules apply?",
        Some("open"),
        Some("normal"),
    )?;
    sync_ticket_system(&root, "local")?;
    let ticket_key = format!("local:{}", remote.ticket_id);

    let queried = query_ticket_source_skill(
        &root,
        "local",
        "During registration my password is not accepted. Which password rules apply?",
        1,
    )?;
    assert_eq!(
        queried
            .get("result")
            .and_then(|value| value.get("retrieval_mode"))
            .and_then(Value::as_str),
        Some("lexical_fallback")
    );
    assert_eq!(
        queried
            .get("result")
            .and_then(|value| value.get("matches"))
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("label"))
            .and_then(Value::as_str),
        Some("REG-03")
    );

    set_ticket_label(
        &root,
        &ticket_key,
        "support/registration",
        "test",
        Some("Bind this ticket to the registration reply flow."),
        json!({}),
    )?;
    put_control_bundle(
        &root,
        ControlBundleInput {
            label: "support/registration".to_string(),
            runbook_id: "eventus.runbook.registration.v1".to_string(),
            runbook_version: "v1".to_string(),
            policy_id: "eventus.reply.policy".to_string(),
            policy_version: "v1".to_string(),
            approval_mode: "direct_execute_allowed".to_string(),
            autonomy_level: "A1".to_string(),
            verification_profile_id: "reply-verification".to_string(),
            writeback_profile_id: "writeback-comment".to_string(),
            support_mode: "support_case".to_string(),
            default_risk_level: "low".to_string(),
            execution_actions: default_execution_actions(),
            notes: Some("Public reply flow for registration FAQ-style tickets.".to_string()),
        },
    )?;

    let dry_run = create_dry_run(
        &root,
        &ticket_key,
        Some("Prepare a registration reply"),
        None,
    )?;
    let case = load_case(&root, &dry_run.case_id)?.context("case missing after dry run")?;
    assert_eq!(case.state, "approval_pending");
    decide_case_approval(
        &root,
        &dry_run.case_id,
        "approved",
        "owner",
        Some("Approved public reply for FAQ-style registration request."),
    )?;

    let reply = compose_ticket_source_skill_reply(
        &root,
        None,
        Some(&dry_run.case_id),
        "suggestion",
        None,
        false,
    )?;
    assert_eq!(
        reply.get("matched_label").and_then(Value::as_str),
        Some("REG-03")
    );
    let reply_body = reply
        .get("reply_body")
        .and_then(Value::as_str)
        .context("reply body missing")?
        .to_string();
    assert!(reply_body.contains("at least 6 characters"));
    assert!(reply_body.contains("one uppercase letter"));

    record_execution_action(&root, &dry_run.case_id, "Prepared public reply from REG-03")?;
    record_verification(
        &root,
        &dry_run.case_id,
        "passed",
        Some("Reply follows REG-03 and references the documented password rules."),
    )?;
    writeback_comment(&root, &dry_run.case_id, &reply_body, false)?;

    let history = list_ticket_history(&root, &ticket_key, 12)?;
    assert!(history.iter().any(|event| {
        event.direction == "outbound"
            && event.body_text.contains("at least 6 characters")
            && event.body_text.contains("one uppercase letter")
    }));

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn skillbook_runbook_reply_requires_review_for_ambiguous_lexical_match() -> Result<()> {
    let root = temp_root("source-skill-runbook-ambiguous");
    std::fs::create_dir_all(&root)?;

    let bundle_dir = root.join("runtime/generated-skills/eventus-email-main");
    write_reply_bundle(
        &bundle_dir,
        &[
            json!({
                "item_id": "eventus.runbook.reg.03.v1",
                "runbook_id": "eventus.runbook.registration.v1",
                "skillbook_id": "eventus.email.support.v1",
                "label": "REG-03",
                "title": "Password is rejected during registration",
                "problem_class": "registration.password_policy",
                "trigger_phrases": [
                    "password is not accepted",
                    "registration password",
                    "what password rules apply"
                ],
                "entry_conditions": ["user is in the registration flow"],
                "earliest_blocker": "Password does not satisfy the registration password policy.",
                "expected_guidance": "Reply with the documented password policy.",
                "tool_actions": { "kind": "reply_only", "tools": [] },
                "verification": ["reply references the documented password rules"],
                "writeback_policy": { "channel": "public_reply" },
                "escalate_when": ["a formally valid password is still rejected"],
                "sources": { "manual": "Supplier manual - E.VENT.US_en (demo manual)" },
                "pages": ["8"],
                "chunk_text": "registration password rejected password rules one uppercase one lowercase one digit minimum 6 characters"
            }),
            json!({
                "item_id": "eventus.runbook.reg.08.v1",
                "runbook_id": "eventus.runbook.registration.v1",
                "skillbook_id": "eventus.email.support.v1",
                "label": "REG-08",
                "title": "Registration password policy reminder",
                "problem_class": "registration.password_policy_repeat",
                "trigger_phrases": [
                    "password rules",
                    "registration password",
                    "password policy"
                ],
                "entry_conditions": ["user asks for password rules during registration"],
                "earliest_blocker": "Password policy reminder is still too generic for direct send.",
                "expected_guidance": "Reply with a manual-backed password policy reminder.",
                "tool_actions": { "kind": "reply_only", "tools": [] },
                "verification": ["reply references the documented password rules"],
                "writeback_policy": { "channel": "public_reply" },
                "escalate_when": ["the right rule set is still unclear"],
                "sources": { "manual": "Supplier manual - E.VENT.US_en (demo manual)" },
                "pages": ["8"],
                "chunk_text": "registration password rejected password rules one uppercase one lowercase one digit minimum 6 characters"
            }),
        ],
    )?;

    import_ticket_source_skill_bundle(
        &root,
        "local",
        bundle_dir.to_str().context("bundle path utf-8")?,
        None,
        true,
    )?;

    let remote = ticket_local_native::create_local_ticket(
        &root,
        "Registration password rules",
        "During registration my password is not accepted. Which password rules apply?",
        Some("open"),
        Some("normal"),
    )?;
    sync_ticket_system(&root, "local")?;

    let reply = compose_ticket_source_skill_reply(
        &root,
        Some(&format!("local:{}", remote.ticket_id)),
        None,
        "suggestion",
        None,
        false,
    )?;
    assert_eq!(
        reply.get("decision").and_then(Value::as_str),
        Some("needs_review")
    );
    assert_eq!(
        reply.get("retrieval_mode").and_then(Value::as_str),
        Some("lexical_fallback")
    );
    assert_eq!(
        reply
            .get("matches")
            .and_then(Value::as_array)
            .map(|items| items.len()),
        Some(2)
    );
    assert!(reply.get("reply_body").is_none());

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn skillbook_runbook_flow_stays_generic_until_adapter_writeback_boundary() -> Result<()> {
    let root = temp_root("source-skill-runbook-generic-adapter");
    std::fs::create_dir_all(&root)?;

    let bundle_dir = root.join("runtime/generated-skills/eventus-email-main");
    write_reply_bundle(
        &bundle_dir,
        &[json!({
            "item_id": "eventus.runbook.reg.03.v1",
            "runbook_id": "eventus.runbook.registration.v1",
            "skillbook_id": "eventus.email.support.v1",
            "label": "REG-03",
            "title": "Password is rejected during registration",
            "problem_class": "registration.password_policy",
            "trigger_phrases": [
                "password is not accepted",
                "registration password",
                "what password rules apply"
            ],
            "entry_conditions": ["user is in the registration flow"],
            "earliest_blocker": "Password does not satisfy the registration password policy.",
            "expected_guidance": "Please check whether your password has at least 6 characters and contains one uppercase letter, one lowercase letter and one digit.",
            "tool_actions": { "kind": "reply_only", "tools": [] },
            "verification": ["reply references the documented password rules"],
            "writeback_policy": { "channel": "public_reply" },
            "escalate_when": ["a formally valid password is still rejected"],
            "sources": { "manual": "Supplier manual - E.VENT.US_en (demo manual)" },
            "pages": ["8"],
            "chunk_text": "registration password rejected password rules one uppercase one lowercase one digit minimum 6 characters"
        })],
    )?;

    import_ticket_source_skill_bundle(
        &root,
        "mockdesk",
        bundle_dir.to_str().context("bundle path utf-8")?,
        None,
        true,
    )?;

    let now = now_iso_string();
    let ticket_key = upsert_ticket_from_adapter(
        &root,
        AdapterTicketMirrorRequest {
            system: "mockdesk",
            remote_ticket_id: "T-42",
            title: "Registration password rejected",
            body_text: "Hello, during registration my password is not accepted. Which password rules apply?",
            remote_status: "open",
            priority: Some("normal"),
            requester: Some("test@example.com"),
            metadata: json!({"channel": "email"}),
            external_created_at: &now,
            external_updated_at: &now,
        },
    )?
    .key;
    upsert_ticket_event_from_adapter(
        &root,
        AdapterTicketEventRequest {
            system: "mockdesk",
            remote_ticket_id: "T-42",
            remote_event_id: "E-1",
            direction: "inbound",
            event_type: "email",
            summary: "Customer asks for password rules",
            body_text:
                "During registration my password is not accepted. Which password rules apply?",
            metadata: json!({}),
            external_created_at: &now,
        },
    )?;

    let resolved = resolve_ticket_source_skill_for_target(&root, Some(&ticket_key), None, 1)?;
    assert_eq!(
        resolved
            .get("resolution")
            .and_then(|value| value.get("matches"))
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("label"))
            .and_then(Value::as_str),
        Some("REG-03")
    );

    set_ticket_label(
        &root,
        &ticket_key,
        "support/registration",
        "test",
        Some("Bind this ticket to the registration reply flow."),
        json!({}),
    )?;
    for domain in REQUIRED_KNOWLEDGE_DOMAINS {
        put_ticket_knowledge_entry(
            &root,
            TicketKnowledgeUpsertInput {
                source_system: "mockdesk".to_string(),
                domain: (*domain).to_string(),
                knowledge_key: format!("baseline::{domain}"),
                title: format!("Mockdesk {domain}"),
                summary: format!("Baseline knowledge for required domain {domain}."),
                status: "active".to_string(),
                content: json!({
                    "source": "test",
                    "domain": domain,
                }),
            },
        )?;
    }
    put_control_bundle(
        &root,
        ControlBundleInput {
            label: "support/registration".to_string(),
            runbook_id: "eventus.runbook.registration.v1".to_string(),
            runbook_version: "v1".to_string(),
            policy_id: "eventus.reply.policy".to_string(),
            policy_version: "v1".to_string(),
            approval_mode: "direct_execute_allowed".to_string(),
            autonomy_level: "A1".to_string(),
            verification_profile_id: "reply-verification".to_string(),
            writeback_profile_id: "writeback-comment".to_string(),
            support_mode: "support_case".to_string(),
            default_risk_level: "low".to_string(),
            execution_actions: default_execution_actions(),
            notes: Some("Public reply flow for registration FAQ-style tickets.".to_string()),
        },
    )?;

    let dry_run = create_dry_run(
        &root,
        &ticket_key,
        Some("Prepare a registration reply"),
        None,
    )?;
    decide_case_approval(
        &root,
        &dry_run.case_id,
        "approved",
        "owner",
        Some("Approved public reply for FAQ-style registration request."),
    )?;
    let reply = compose_ticket_source_skill_reply(
        &root,
        None,
        Some(&dry_run.case_id),
        "suggestion",
        None,
        false,
    )?;
    let reply_body = reply
        .get("reply_body")
        .and_then(Value::as_str)
        .context("reply body missing")?
        .to_string();
    assert!(reply_body.contains("at least 6 characters"));
    record_execution_action(&root, &dry_run.case_id, "Prepared public reply from REG-03")?;
    let case = record_verification(
        &root,
        &dry_run.case_id,
        "passed",
        Some("Reply follows REG-03 and references the documented password rules."),
    )?;
    assert_eq!(case.state, "writeback_pending");

    let err = writeback_comment(&root, &dry_run.case_id, &reply_body, false)
        .expect_err("mockdesk should only fail at the adapter writeback boundary");
    assert!(err
        .to_string()
        .contains("unsupported ticket system for writeback: mockdesk"));

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn public_writeback_requires_verified_case_state() -> Result<()> {
    let root = temp_root("ticket-writeback-gate");
    std::fs::create_dir_all(&root)?;

    let remote = ticket_local_native::create_local_ticket(
        &root,
        "Registration password rejected",
        "Hello, during registration my password is not accepted. Which password rules apply?",
        Some("open"),
        Some("normal"),
    )?;
    sync_ticket_system(&root, "local")?;
    let ticket_key = format!("local:{}", remote.ticket_id);

    set_ticket_label(
        &root,
        &ticket_key,
        "support/registration",
        "test",
        Some("Bind this ticket to the registration reply flow."),
        json!({}),
    )?;
    put_control_bundle(
        &root,
        ControlBundleInput {
            label: "support/registration".to_string(),
            runbook_id: "eventus.runbook.registration.v1".to_string(),
            runbook_version: "v1".to_string(),
            policy_id: "eventus.reply.policy".to_string(),
            policy_version: "v1".to_string(),
            approval_mode: "direct_execute_allowed".to_string(),
            autonomy_level: "A1".to_string(),
            verification_profile_id: "reply-verification".to_string(),
            writeback_profile_id: "writeback-comment".to_string(),
            support_mode: "support_case".to_string(),
            default_risk_level: "low".to_string(),
            execution_actions: default_execution_actions(),
            notes: Some("Public reply flow for registration FAQ-style tickets.".to_string()),
        },
    )?;

    let dry_run = create_dry_run(
        &root,
        &ticket_key,
        Some("Prepare a registration reply"),
        None,
    )?;
    decide_case_approval(
        &root,
        &dry_run.case_id,
        "approved",
        "owner",
        Some("Approved public reply for FAQ-style registration request."),
    )?;
    record_execution_action(&root, &dry_run.case_id, "Prepared public reply draft")?;

    let err = writeback_comment(
        &root,
        &dry_run.case_id,
        "Hello, please check the documented password rules.",
        false,
    )
    .expect_err("public writeback before verification should fail");
    assert!(err
        .to_string()
        .contains("is not ready for writeback; current state is executing"));

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn ticket_close_is_blocked_without_verified_guard_proof() -> Result<()> {
    let root = temp_root("ticket-close-guard");
    std::fs::create_dir_all(&root)?;

    let remote = ticket_local_native::create_local_ticket(
        &root,
        "Registration password rejected",
        "Hello, during registration my password is not accepted.",
        Some("open"),
        Some("normal"),
    )?;
    sync_ticket_system(&root, "local")?;
    let ticket_key = format!("local:{}", remote.ticket_id);

    set_ticket_label(
        &root,
        &ticket_key,
        "support/registration",
        "test",
        Some("Bind this ticket to the registration reply flow."),
        json!({}),
    )?;
    put_control_bundle(
        &root,
        ControlBundleInput {
            label: "support/registration".to_string(),
            runbook_id: "eventus.runbook.registration.v1".to_string(),
            runbook_version: "v1".to_string(),
            policy_id: "eventus.reply.policy".to_string(),
            policy_version: "v1".to_string(),
            approval_mode: "direct_execute_allowed".to_string(),
            autonomy_level: "A1".to_string(),
            verification_profile_id: "reply-verification".to_string(),
            writeback_profile_id: "writeback-comment".to_string(),
            support_mode: "support_case".to_string(),
            default_risk_level: "low".to_string(),
            execution_actions: default_execution_actions(),
            notes: Some("Public reply flow for registration FAQ-style tickets.".to_string()),
        },
    )?;

    let dry_run = create_dry_run(
        &root,
        &ticket_key,
        Some("Prepare a registration reply"),
        None,
    )?;
    decide_case_approval(
        &root,
        &dry_run.case_id,
        "approved",
        "owner",
        Some("Approved bounded reply work."),
    )?;
    record_execution_action(&root, &dry_run.case_id, "Prepared reply draft")?;

    let err = close_case(&root, &dry_run.case_id, Some("premature close"))
        .expect_err("close without verification must be rejected by the core guard");
    assert!(err.to_string().contains("closure_requires_verification"));

    let case = record_verification(
        &root,
        &dry_run.case_id,
        "passed",
        Some("Reply was verified against source-skill evidence."),
    )?;
    assert_eq!(case.state, "writeback_pending");
    let case = close_case(&root, &dry_run.case_id, Some("verified close"))?;
    assert_eq!(case.state, "closed");

    let conn = open_ticket_db(&root)?;
    let accepted_proofs: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ctox_core_transition_proofs WHERE entity_id = ?1 AND accepted = 1",
        params![dry_run.case_id],
        |row| row.get(0),
    )?;
    let rejected_proofs: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ctox_core_transition_proofs WHERE entity_id = ?1 AND accepted = 0",
        params![dry_run.case_id],
        |row| row.get(0),
    )?;
    assert_eq!(accepted_proofs, 1);
    assert_eq!(rejected_proofs, 1);

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn source_skill_review_note_accepts_plain_grounded_internal_note() -> Result<()> {
    let root = temp_root("source-skill-note-review-good");
    std::fs::create_dir_all(&root)?;
    let skill_dir = root.join("runtime/generated-skills/demo-skill");
    let generated_dir = skill_dir.join("references/generated");
    std::fs::create_dir_all(&generated_dir)?;
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "# Demo Desk Skill\n\n## How To Handle A New Ticket\n\nUse desk language.\n",
    )?;
    std::fs::write(
        generated_dir.join("family_playbooks.json"),
        serde_json::to_string_pretty(&vec![json!({
            "family_key": "access :: identity :: mhs",
            "signals": {
                "token_signals": ["MHS", "Sperrung", "Benutzer"],
                "common_phrases": ["mhs benutzer", "benutzer gesperrt"]
            },
            "usual_handling": {
                "dominant_channels": [["email", 4]],
                "dominant_states": [["open", 4]],
                "actions_seen": ["entsperrt"],
                "closure_tendency": 0.75
            },
            "decision_support": {
                "mode": "access_change",
                "operator_summary": "This desk handles MHS user locks as access work.",
                "triage_focus": ["identify the locked user"],
                "handling_steps": ["confirm the affected MHS identity", "unlock only after identity is clear"],
                "close_when": "Close when the user can sign in again.",
                "caution_signals": ["do not unlock the wrong account"],
                "note_guidance": "Record the affected identity and whether retry worked."
            },
            "historical_examples": {
                "canonical": [{"ticket_id": "100", "title": "Sperrung MHS Benutzer GAJ", "why": "Representative historical case."}]
            }
        })])?
            + "\n",
    )?;
    std::fs::write(
        generated_dir.join("retrieval_index.jsonl"),
        serde_json::to_string(&json!({
            "card_id": "family:1",
            "card_type": "family_playbook",
            "family_key": "access :: identity :: mhs",
            "request_type": "access",
            "category": "identity",
            "subcategory": "mhs",
            "text": "access identity mhs benutzer sperrung kurzzeichen login entsperrt"
        }))? + "\n",
    )?;
    put_ticket_source_skill_binding(
        &root,
        "local",
        "demo-skill",
        "operating-model",
        "active",
        "test",
        Some("runtime/generated-skills/demo-skill"),
        Some("test binding"),
    )?;
    let remote = ticket_local_native::create_local_ticket(
        &root,
        "Sperrung MHS Benutzer GAJ",
        "Benutzer GAJ ist in MHS gesperrt und kann sich nicht mehr anmelden.",
        Some("open"),
        Some("high"),
    )?;
    sync_ticket_system(&root, "local")?;
    let review = review_ticket_note_with_source_skill(
        &root,
        &format!("local:{}", remote.ticket_id),
        "Benutzer GAJ ist in MHS gesperrt. Ich prüfe zuerst das betroffene Kurzzeichen und teste danach den erneuten Login nach der Entsperrung.",
        1,
    )?;
    assert!(review.desk_ready);
    assert!(review.language_clean);
    assert!(review.copy_safe);
    assert!(review.grounded_in_ticket);
    assert_eq!(
        review.matched_family.as_deref(),
        Some("access :: identity :: mhs")
    );

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn source_skill_review_note_flags_leaky_or_copied_notes() -> Result<()> {
    let root = temp_root("source-skill-note-review-bad");
    std::fs::create_dir_all(&root)?;
    let skill_dir = root.join("runtime/generated-skills/demo-skill");
    let generated_dir = skill_dir.join("references/generated");
    std::fs::create_dir_all(&generated_dir)?;
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "# Demo Desk Skill\n\n## How To Handle A New Ticket\n\nUse desk language.\n",
    )?;
    std::fs::write(
        generated_dir.join("family_playbooks.json"),
        serde_json::to_string_pretty(&vec![json!({
            "family_key": "access :: identity :: mhs",
            "signals": {
                "token_signals": ["MHS", "Sperrung"],
                "common_phrases": ["mhs benutzer", "benutzer gesperrt"]
            },
            "usual_handling": {
                "dominant_channels": [["email", 4]],
                "dominant_states": [["open", 4]],
                "actions_seen": ["entsperrt"],
                "closure_tendency": 0.75
            },
            "decision_support": {
                "mode": "access_change",
                "operator_summary": "This desk handles MHS user locks as access work.",
                "triage_focus": ["identify the locked user"],
                "handling_steps": ["confirm the affected MHS identity", "unlock only after identity is clear"],
                "close_when": "Close when the user can sign in again.",
                "caution_signals": ["do not unlock the wrong account"],
                "note_guidance": "Record the affected identity and whether retry worked."
            },
            "historical_examples": {
                "canonical": [{"ticket_id": "100", "title": "Sperrung MHS Benutzer", "why": "Representative historical case."}]
            }
        })])?
            + "\n",
    )?;
    std::fs::write(
        generated_dir.join("retrieval_index.jsonl"),
        serde_json::to_string(&json!({
            "card_id": "family:1",
            "card_type": "family_playbook",
            "family_key": "access :: identity :: mhs",
            "request_type": "access",
            "category": "identity",
            "subcategory": "mhs",
            "text": "access identity mhs benutzer sperrung entsperrt"
        }))? + "\n",
    )?;
    put_ticket_source_skill_binding(
        &root,
        "local",
        "demo-skill",
        "operating-model",
        "active",
        "test",
        Some("runtime/generated-skills/demo-skill"),
        Some("test binding"),
    )?;
    let remote = ticket_local_native::create_local_ticket(
        &root,
        "Sperrung MHS Benutzer",
        "MHS account is locked.",
        Some("open"),
        Some("high"),
    )?;
    sync_ticket_system(&root, "local")?;
    let review = review_ticket_note_with_source_skill(
        &root,
        &format!("local:{}", remote.ticket_id),
        "This desk handles MHS user locks as access work. Use `note_guidance` from sqlite before writeback.",
        1,
    )?;
    assert!(!review.desk_ready);
    assert!(!review.language_clean);
    assert!(!review.copy_safe);
    assert!(review
        .findings
        .iter()
        .any(|item| item.kind == "internal_field_names" || item.kind == "tooling_terms"));
    assert!(review
        .findings
        .iter()
        .any(|item| item.kind == "copied_skill_language"));

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn self_work_lifecycle_supports_assign_notes_and_transition() -> Result<()> {
    let root = temp_root("self-work-lifecycle");
    std::fs::create_dir_all(&root)?;

    let item = put_ticket_self_work_item(
        &root,
        TicketSelfWorkUpsertInput {
            source_system: "local".to_string(),
            kind: "onboarding-gap".to_string(),
            title: "Review access gaps for monitoring".to_string(),
            body_text: "Investigate which monitoring systems still need access.".to_string(),
            state: "open".to_string(),
            metadata: json!({"skill": "system-onboarding"}),
        },
        true,
    )?;
    assert_eq!(item.state, "published");
    assert!(item.remote_ticket_id.is_some());

    let item = assign_ticket_self_work_item(
        &root,
        &item.work_id,
        "ctox-agent",
        "ctox",
        Some("CTOX should own onboarding work by default"),
    )?;
    assert_eq!(item.assigned_to.as_deref(), Some("ctox-agent"));
    assert_eq!(item.assigned_by.as_deref(), Some("ctox"));

    let note = append_ticket_self_work_note(
        &root,
        &item.work_id,
        "Observed that monitoring access is still missing for two systems.",
        "ctox",
        "internal",
    )?;
    assert_eq!(note.authored_by, "ctox");
    assert_eq!(note.visibility, "internal");
    assert!(note.remote_event_id.is_some());

    let item = transition_ticket_self_work_item(
        &root,
        &item.work_id,
        "blocked",
        "ctox",
        Some("Blocked until monitoring credentials are provided."),
        "internal",
    )?;
    assert_eq!(item.state, "blocked");

    let shown = load_ticket_self_work_item(&root, &item.work_id)?
        .context("internal work item missing after lifecycle")?;
    assert_eq!(shown.assigned_to.as_deref(), Some("ctox-agent"));

    let assignments = list_ticket_self_work_assignments(&root, &item.work_id, 10)?;
    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].assigned_to, "ctox-agent");

    let notes = list_ticket_self_work_notes(&root, &item.work_id, 10)?;
    assert_eq!(notes.len(), 2);
    assert!(notes
        .iter()
        .any(|entry| entry.body_text.contains("two systems")));
    assert!(notes
        .iter()
        .any(|entry| entry.body_text.contains("credentials are provided")));

    let local_ticket = ticket_local_native::load_local_ticket(
        &root,
        item.remote_ticket_id
            .as_deref()
            .context("missing remote ticket id")?,
    )?
    .context("published local ticket missing")?;
    assert_eq!(
        local_ticket
            .metadata
            .get("assigned_to")
            .and_then(Value::as_str),
        Some("ctox-agent")
    );
    let local_events = ticket_local_native::list_local_ticket_events(
        &root,
        item.remote_ticket_id.as_deref().unwrap(),
        20,
    )?;
    assert!(local_events
        .iter()
        .any(|event| event.event_type == "assignment_changed"));
    assert!(local_events
        .iter()
        .any(|event| event.body_text.contains("two systems")));
    assert!(local_events
        .iter()
        .any(|event| event.event_type == "status_changed"));

    let audit = list_audit_records(
        &root,
        Some(&format!("*self-work:{}*", shown.source_system)),
        20,
    )?;
    assert!(audit
        .iter()
        .any(|entry| entry.action_type == "self_work_assigned"));
    assert!(audit
        .iter()
        .any(|entry| entry.action_type == "self_work_note_appended"));
    assert!(audit
        .iter()
        .any(|entry| entry.action_type == "self_work_transitioned"));

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn rejected_self_work_transition_writes_no_remote_events_or_notes() -> Result<()> {
    let root = temp_root("self-work-rejected-transition");
    std::fs::create_dir_all(&root)?;

    let item = put_ticket_self_work_item(
        &root,
        TicketSelfWorkUpsertInput {
            source_system: "local".to_string(),
            kind: "founder-communication-rework".to_string(),
            title: "Founder communication rework: stale".to_string(),
            body_text: "Stale rework without a success proof.".to_string(),
            state: "open".to_string(),
            metadata: json!({}),
        },
        true,
    )?;
    let item = transition_ticket_self_work_item(
        &root,
        &item.work_id,
        "blocked",
        "ctox",
        Some("Blocked for the regression."),
        "internal",
    )?;
    let remote_ticket_id = item
        .remote_ticket_id
        .clone()
        .context("missing remote ticket id")?;
    let notes_before = list_ticket_self_work_notes(&root, &item.work_id, 100)?.len();
    let events_before =
        ticket_local_native::list_local_ticket_events(&root, &remote_ticket_id, 100)?.len();

    // Closing without a terminal success proof is rejected by the core state
    // guard. The service retries this every router tick, so a rejection must
    // not leave a remote status change, comment or note behind.
    for _ in 0..3 {
        let result = transition_ticket_self_work_item(
            &root,
            &item.work_id,
            "closed",
            "ctox-service",
            Some("Founder communication already has a reviewed sent reply; closing stale internal work."),
            "internal",
        );
        assert!(result.is_err(), "closing without proof must be rejected");
    }

    assert_eq!(
        list_ticket_self_work_notes(&root, &item.work_id, 100)?.len(),
        notes_before
    );
    assert_eq!(
        ticket_local_native::list_local_ticket_events(&root, &remote_ticket_id, 100)?.len(),
        events_before
    );
    let shown =
        load_ticket_self_work_item(&root, &item.work_id)?.context("internal work item missing")?;
    assert_eq!(shown.state, "blocked");
    Ok(())
}
