use super::*;
use rusqlite::OptionalExtension;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Default)]
struct SelectionDiagnostics {
    seen: BTreeMap<String, Instant>,
    last_error: Option<String>,
}
fn diagnostics() -> &'static Mutex<BTreeMap<PathBuf, SelectionDiagnostics>> {
    static DIAGNOSTICS: OnceLock<Mutex<BTreeMap<PathBuf, SelectionDiagnostics>>> = OnceLock::new();
    DIAGNOSTICS.get_or_init(Mutex::default)
}
pub(crate) fn selection_last_error(root: &Path) -> Option<String> {
    diagnostics()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(root)
        .and_then(|d| d.last_error.clone())
}
fn selection_warning(root: &Path, task: Option<&str>, attempt: &str, cause: &str) {
    let first = {
        let mut all = diagnostics().lock().unwrap_or_else(|e| e.into_inner());
        let entry = all.entry(root.to_path_buf()).or_default();
        entry.last_error = Some(format!("Crew selection unavailable: {cause}"));
        let now = Instant::now();
        let due = entry
            .seen
            .get(cause)
            .is_none_or(|last| now.duration_since(*last) >= Duration::from_secs(3600));
        if due {
            entry.seen.insert(cause.to_string(), now);
        }
        due
    };
    if first {
        eprintln!(
            "[ctox crew] selection unavailable: {cause}; execution continues without requiring crew identity"
        );
        crate::service::harness_flow::record_harness_flow_event_lossy(
            root,
            crate::service::harness_flow::RecordHarnessFlowEventRequest {
                event_kind: "crew_selection_unavailable",
                title: cause,
                body_text: cause,
                message_key: task,
                work_id: None,
                ticket_key: None,
                attempt_index: None,
                metadata: json!({"attempt_id":attempt,"cause":cause,"cockpit_eligible":true}),
            },
        );
    }
}

fn selection_kind(reason: &str) -> &'static str {
    if reason.starts_with("assigned:") {
        "assigned"
    } else if reason.starts_with("continuity:") {
        "continuity"
    } else if reason.starts_with("routed:") {
        "routed"
    } else {
        "selected"
    }
}

/// Pump-only restart recovery. The latest successful selection supersedes an
/// older failure; malformed-member warnings on a usable selection remain visible.
pub(crate) fn durable_selection_last_error(conn: &Connection) -> Result<Option<String>> {
    let has_events: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='ctox_harness_flow_events')",
        [],
        |r| r.get(0),
    )?;
    if !has_events {
        return Ok(None);
    }
    let cause: Option<Option<String>> = conn
        .query_row(
            "SELECT CASE WHEN event_kind='crew_selection_unavailable' THEN title
                ELSE json_extract(metadata_json,'$.selection_error') END
         FROM ctox_harness_flow_events
         WHERE event_kind IN ('crew_selected','crew_selection_unavailable')
           AND COALESCE(json_extract(metadata_json,'$.repaired'),0)=0
         ORDER BY created_at DESC,rowid DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(cause
        .flatten()
        .map(|cause| format!("Crew selection unavailable: {cause}")))
}

/// Crew is optional context. A broken profile or unavailable crew store must not
/// become a worker failure, consume retry budget, or repeatedly release its lease.
pub(crate) fn prepare_attempt_or_continue(
    root: &Path,
    task_ids: &[String],
    lease_owner: &str,
    attempt: &str,
    thread_key: Option<&str>,
    metadata: &Value,
    skill: Option<&str>,
    prompt: &str,
    judge: Option<&dyn RouterJudge>,
) -> Option<CrewTurnContext> {
    diagnostics()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(root.to_path_buf())
        .or_default()
        .last_error = None;
    match prepare_attempt(
        root,
        task_ids,
        lease_owner,
        attempt,
        thread_key,
        metadata,
        skill,
        prompt,
        judge,
    ) {
        Ok(block) => {
            let mut all = diagnostics().lock().unwrap_or_else(|e| e.into_inner());
            let entry = all.entry(root.to_path_buf()).or_default();
            // A fresh failure after recovery is a new state transition, even
            // inside the previous warning's rate-limit window.
            if entry.last_error.is_none() {
                entry.seen.clear();
            }
            block
        }
        Err(error) => {
            // Do not audit raw SQL/JSON errors or prompt material.
            let cause = if error.to_string() == "no active crew member available" {
                "no active crew member available"
            } else {
                "crew preparation failed"
            };
            selection_warning(root, task_ids.first().map(String::as_str), attempt, cause);
            if let Ok(conn) = Connection::open(crate::paths::core_db(root)) {
                for id in task_ids {
                    let _ = conn.execute(
                        "UPDATE communication_routing_state SET crew_member_id=NULL
                        WHERE message_key=?1 AND route_status='leased' AND lease_owner=?2",
                        params![id, lease_owner],
                    );
                }
            }
            None
        }
    }
}

/// What a turn receives for its member: identity for the base-instruction
/// lane, memory for the runtime-context lane, and the reason for the cockpit.
#[derive(Clone, Debug)]
pub(crate) struct CrewTurnContext {
    pub member_id: String,
    pub member_name: String,
    pub selection_reason: String,
    pub persona: String,
    pub memory_block: Option<String>,
}

/// Called once before invoking a slice, never from a progress callback. The
/// judgment (router) happens before the write transaction; the transaction
/// pins identity to the immutable attempt and its still-held lease.
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_attempt(
    root: &Path,
    task_ids: &[String],
    lease_owner: &str,
    attempt: &str,
    thread_key: Option<&str>,
    metadata: &Value,
    skill: Option<&str>,
    prompt: &str,
    judge: Option<&dyn RouterJudge>,
) -> Result<Option<CrewTurnContext>> {
    let Some(task_id) = task_ids.first() else {
        return Ok(None);
    };
    let conn = Connection::open(crate::paths::core_db(root))?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    // New ledgers remain lazy until admitted work. Existing ledgers were already
    // deduplicated/indexed by the migration; this also covers a ledger initialized
    // independently since then. No extra reads are added to progress emissions.
    crate::service::harness_flow::ensure_event_schema(&conn)?;
    ensure_selection_event_index(&conn)?;
    ensure_schema(&conn)?;
    let existing: Option<String> = conn
        .query_row(
            "SELECT member_id FROM crew_attempts WHERE attempt_id=?1",
            [attempt],
            |r| r.get(0),
        )
        .optional()?;
    let (all, warnings) = members_with_errors(&conn)?;
    let mut task = TaskTraits {
        thread_key: thread_key.map(String::from),
        module: metadata
            .get("business_os_module")
            .or_else(|| metadata.get("business_os_module_id"))
            .or_else(|| metadata.get("module_id"))
            .and_then(Value::as_str)
            .map(String::from),
        command_type: metadata
            .get("business_os_command_type")
            .or_else(|| metadata.get("command_type"))
            .and_then(Value::as_str)
            .map(String::from),
        skills: skill.into_iter().map(String::from).collect(),
        ..Default::default()
    };
    let prompt_lower = prompt.to_lowercase();
    task.tags = all
        .iter()
        .flat_map(|m| m.specialties.tags.iter())
        .filter(|tag| {
            prompt_lower
                .split(|c: char| !c.is_alphanumeric())
                .any(|word| word == tag.to_lowercase())
        })
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    task.manual_member = conn
        .query_row(
            "SELECT crew_assigned_member_id FROM communication_routing_state
         WHERE message_key=?1 AND route_status='leased' AND lease_owner=?2",
            params![task_id, lease_owner],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?
        .context("crew attachment requires the held lease")?;
    if let Some(member) = crate::business_os::project_crew_member_for_task(root, task_id)? {
        anyhow::ensure!(
            task_ids.len() == 1,
            "private project Crew cannot share a batch attempt"
        );
        anyhow::ensure!(
            task.manual_member
                .as_ref()
                .is_none_or(|assigned| assigned == &member),
            "manual Crew assignment conflicts with the project worker"
        );
        anyhow::ensure!(
            existing.as_ref().is_none_or(|assigned| assigned == &member),
            "resumed Crew identity differs from the current project binding"
        );
        task.manual_member = Some(member);
    }
    // A retry is scored again; its own previous selection is not a manual pin
    // or evidence of continuity with a different task in the conversation.
    task.continuity_member = conn
        .query_row(
            "SELECT member_id FROM crew_attempts
        WHERE thread_key=?1 AND task_id!=?2 AND (started_at IS NOT NULL OR finalized_at IS NOT NULL)
        ORDER BY selected_at DESC,attempt_id DESC LIMIT 1",
            params![thread_key, task_id],
            |r| r.get(0),
        )
        .optional()?;
    let history=conn.prepare("SELECT member_id,module,thread_key,succeeded,finalized_at FROM crew_attempts WHERE finalized_at IS NOT NULL ORDER BY finalized_at DESC,attempt_id DESC LIMIT 1000")?
        .query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,Option<String>>(2)?,r.get::<_,bool>(3)?,r.get::<_,String>(4)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?.into_iter().filter_map(|(member_id,module,thread_key,succeeded,at)| Some(History{member_id,module,thread_key,succeeded,finished_at_ms:chrono::DateTime::parse_from_rfc3339(&at).ok()?.timestamp_millis()})).collect::<Vec<_>>();
    let summary = task_summary_from_prompt(prompt, task.module.as_deref());
    let engine = open_engine(root).ok();
    let selection = if let Some(id) = existing {
        Selection {
            member_id: id,
            reason: "Identität des wiederaufgenommenen Versuchs bleibt erhalten".into(),
        }
    } else {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let memories = all
            .iter()
            .map(|member| {
                engine
                    .as_ref()
                    .map(|engine| load_member_memory(engine, &member.id))
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        let recents = all
            .iter()
            .map(|member| recent_attempts(&conn, &member.id, 6).unwrap_or_default())
            .collect::<Vec<_>>();
        let candidates = all
            .iter()
            .enumerate()
            .filter(|(_, member)| !member.archived)
            .map(|(index, member)| RouterCandidate {
                member,
                memory: &memories[index],
                recent: &recents[index],
                failures_24h: history
                    .iter()
                    .filter(|h| {
                        h.member_id == member.id
                            && !h.succeeded
                            && task.module.is_some()
                            && h.module == task.module
                            && h.finished_at_ms >= now_ms.saturating_sub(86_400_000)
                    })
                    .count(),
            })
            .collect::<Vec<_>>();
        let router_task = RouterTask {
            summary: &summary,
            module: task.module.as_deref(),
            command_type: task.command_type.as_deref(),
            skills: &task.skills,
            thread_key: task.thread_key.as_deref(),
        };
        route(
            &all,
            &task,
            &history,
            &router_task,
            &candidates,
            judge,
            now_ms,
        )
        .context("no active crew member available")?
    };
    let member = all
        .iter()
        .find(|m| m.id == selection.member_id)
        .context("attempt member no longer exists")?;
    let now = chrono::Utc::now().to_rfc3339();
    let tx = conn.unchecked_transaction()?;
    let inserted=tx.execute("INSERT OR IGNORE INTO crew_attempts(attempt_id,task_id,member_id,module,thread_key,selected_at,started_at,selection_reason,task_summary) VALUES(?1,?2,?3,?4,?5,?6,?6,?7,?8)",params![attempt,task_id,member.id,task.module,thread_key,now,selection.reason,summary])?;
    // Check the durable identity after INSERT OR IGNORE, inside the same write
    // transaction as attachment. A replay must not borrow another task's member
    // or revive an already finalized attempt. Checking only the earlier member
    // lookup would also miss a concurrent insertion or finalization.
    let (bound_task, bound_member, finalized): (String, String, Option<String>) = tx.query_row(
        "SELECT task_id,member_id,finalized_at FROM crew_attempts WHERE attempt_id=?1",
        [attempt],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    anyhow::ensure!(
        bound_task == *task_id,
        "crew attempt belongs to another task"
    );
    anyhow::ensure!(
        bound_member == member.id,
        "crew attempt member changed during admission"
    );
    anyhow::ensure!(finalized.is_none(), "crew attempt is already finalized");
    for id in task_ids {
        let changed = tx.execute(
            "UPDATE communication_routing_state SET crew_member_id=?2,crew_assigned_member_id=NULL,updated_at=?3
             WHERE message_key=?1 AND route_status='leased' AND lease_owner=?4",
            params![id, member.id, now, lease_owner],
        )?;
        anyhow::ensure!(changed == 1, "crew attachment lost its lease");
    }
    if inserted > 0 {
        tx.execute("UPDATE crew_members SET stats_json=json_set(stats_json,'$.last_active_at',?2),updated_at=?2 WHERE id=?1",params![member.id,now])?;
    }
    tx.commit()?;
    // Memory after the commit: the legacy learnings table is folded into the
    // member's anchors once, then the documents are read as they are.
    let mut member_memory = MemberMemory::default();
    if let Some(engine) = engine.as_ref() {
        if let Err(error) = migrate_learnings_into_memory(&conn, engine, &member.id) {
            eprintln!(
                "[ctox crew] legacy learnings migration deferred for {}: {error}",
                member.id
            );
        }
        member_memory = load_member_memory(engine, &member.id);
    }
    let recent = recent_attempts(&conn, &member.id, 6).unwrap_or_default();
    let persona = render_persona(member);
    let memory_block = render_memory_block(member, &member_memory, &recent);
    for warning in &warnings {
        selection_warning(root, Some(task_id), attempt, warning);
    }
    if inserted > 0 {
        crate::service::harness_flow::record_harness_flow_event_lossy(
            root,
            crate::service::harness_flow::RecordHarnessFlowEventRequest {
                event_kind: "crew_selected",
                title: &selection.reason,
                body_text: &selection.reason,
                message_key: Some(task_id),
                work_id: None,
                ticket_key: None,
                attempt_index: None,
                metadata: json!({"attempt_id":attempt,"crew_member_id":selection.member_id,"reason":selection.reason,"selection_kind":selection_kind(&selection.reason),"selection_error":warnings.first(),"cockpit_eligible":true}),
            },
        );
        if memory_block.is_some() {
            // The member is reading now: the projection shows it for a moment.
            let _ = conn.execute(
                "UPDATE crew_members SET last_memory_read_at=?2,updated_at=?2 WHERE id=?1",
                params![member.id, now],
            );
            let anchors = anchor_lines(&member_memory.anchors).len();
            let experiences = narrative_lines(&member_memory.narrative).len();
            let title = format!(
                "{} liest sein Gedächtnis: {} Wissenseinträge, {} Erfahrungen, {} letzte Einsätze",
                member.name,
                anchors,
                experiences,
                recent.len()
            );
            crate::service::harness_flow::record_harness_flow_event_lossy(
                root,
                crate::service::harness_flow::RecordHarnessFlowEventRequest {
                    event_kind: "crew.memory_read",
                    title: &title,
                    body_text: "",
                    message_key: Some(task_id),
                    work_id: None,
                    ticket_key: None,
                    attempt_index: None,
                    metadata: json!({"attempt_id":attempt,"crew_member_id":member.id,"anchors":anchors,"experiences":experiences,"recent_attempts":recent.len(),"cockpit_eligible":true}),
                },
            );
        }
    }
    Ok(Some(CrewTurnContext {
        member_id: member.id.clone(),
        member_name: member.name.clone(),
        selection_reason: selection.reason,
        persona,
        memory_block,
    }))
}

/// Repair a lost notification from durable selection evidence on the pump.
/// The unique event index also closes a race with the initial best-effort emit.
pub(crate) fn repair_selection_events(root: &Path, conn: &Connection) -> Result<()> {
    ensure_selection_event_index(conn)?;
    let mut cursor = String::new();
    loop {
        let rows=conn.prepare("SELECT a.attempt_id,a.task_id,a.member_id,a.selection_reason
            FROM crew_attempts a WHERE a.attempt_id>?1 AND a.selection_reason!=''
              AND (a.started_at IS NOT NULL OR a.finalized_at IS NOT NULL)
              AND NOT EXISTS(SELECT 1 FROM ctox_harness_flow_events e
                WHERE e.event_kind='crew_selected' AND json_extract(e.metadata_json,'$.attempt_id')=a.attempt_id)
            ORDER BY a.attempt_id LIMIT 128")?.query_map([&cursor],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        if rows.is_empty() {
            break;
        }
        for (attempt, task, member, reason) in rows {
            cursor = attempt.clone();
            // A concurrent successful initial emission may win the unique key;
            // the next sweep then observes that durable event instead.
            crate::service::harness_flow::record_harness_flow_event_lossy(
                root,
                crate::service::harness_flow::RecordHarnessFlowEventRequest {
                    event_kind: "crew_selected",
                    title: &reason,
                    body_text: &reason,
                    message_key: Some(&task),
                    work_id: None,
                    ticket_key: None,
                    attempt_index: None,
                    metadata: json!({"attempt_id":attempt,"crew_member_id":member,"reason":reason,"selection_kind":selection_kind(&reason),"repaired":true,"cockpit_eligible":true}),
                },
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn crew_warning_window_expires_and_durable_lookup_uses_index() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        std::fs::create_dir_all(root.path().join("runtime"))?;
        let conn = rusqlite::Connection::open(crate::paths::core_db(root.path()))?;
        crate::service::harness_flow::ensure_event_schema(&conn)?;
        super::ensure_selection_event_index(&conn)?;
        super::selection_warning(root.path(), None, "a", "crew preparation failed");
        super::selection_warning(root.path(), None, "b", "crew preparation failed");
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM ctox_harness_flow_events WHERE event_kind='crew_selection_unavailable'", [], |r|r.get::<_,i64>(0))?, 1);
        super::diagnostics()
            .lock()
            .unwrap()
            .get_mut(root.path())
            .unwrap()
            .seen
            .insert(
                "crew preparation failed".into(),
                std::time::Instant::now() - std::time::Duration::from_secs(3601),
            );
        super::selection_warning(root.path(), None, "c", "crew preparation failed");
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM ctox_harness_flow_events WHERE event_kind='crew_selection_unavailable'", [], |r|r.get::<_,i64>(0))?, 2);
        super::diagnostics().lock().unwrap().remove(root.path());
        assert!(super::selection_last_error(root.path()).is_none());
        assert!(super::durable_selection_last_error(&conn)?
            .unwrap()
            .contains("crew preparation failed"));
        let plan = conn.prepare("EXPLAIN QUERY PLAN SELECT title FROM ctox_harness_flow_events WHERE event_kind IN ('crew_selected','crew_selection_unavailable') AND COALESCE(json_extract(metadata_json,'$.repaired'),0)=0 ORDER BY created_at DESC,rowid DESC LIMIT 1")?.query_map([], |r|r.get::<_,String>(3))?.collect::<rusqlite::Result<Vec<_>>>()?.join("\n");
        assert!(
            plan.contains("idx_crew_selection_diagnostic_time"),
            "{plan}"
        );
        assert!(!plan.contains("TEMP B-TREE"), "{plan}");
        Ok(())
    }

    use super::*;
    #[test]
    fn conflicting_and_finalized_attempts_cannot_attach_or_consume_assignment() -> Result<()> {
        for finalized in [false, true] {
            let root = tempfile::tempdir()?;
            let create = |thread: &str| {
                crate::mission::channels::create_queue_task(
                    root.path(),
                    crate::mission::channels::QueueTaskCreateRequest {
                        title: "Crew attempt binding fixture".into(),
                        prompt: "Code prüfen".into(),
                        thread_key: thread.into(),
                        workspace_root: None,
                        priority: "normal".into(),
                        suggested_skill: None,
                        parent_message_key: None,
                        extra_metadata: None,
                    },
                )
            };
            let original =
                create("original-thread").context("create original Crew fixture task")?;
            let other = create("other-thread").context("create competing Crew fixture task")?;
            let conn = Connection::open(crate::paths::core_db(root.path()))?;
            conn.execute(
                "UPDATE communication_routing_state SET route_status='leased',
                 lease_owner='crew-worker',crew_assigned_member_id='crew-pico'
                 WHERE message_key IN (?1,?2)",
                params![original.message_key, other.message_key],
            )?;
            prepare_attempt(
                root.path(),
                &[original.message_key.clone()],
                "crew-worker",
                "immutable-attempt",
                Some("original-thread"),
                &json!({}),
                None,
                "Code prüfen",
                None,
            )
            .context("prepare initial Crew fixture attempt")?
            .context("initial crew context missing")?;
            let target = if finalized { &original } else { &other };
            if finalized {
                crate::crew::finalization::finalize_attempt(
                    &conn,
                    "immutable-attempt",
                    "failed",
                    None,
                    "2026-09-09T12:00:00Z",
                    None,
                    "Attempt failed",
                    None,
                )
                .context("finalize original Crew fixture attempt")?;
            }
            conn.execute(
                "UPDATE communication_routing_state SET crew_member_id=NULL,
                 crew_assigned_member_id='crew-nori' WHERE message_key=?1",
                [&target.message_key],
            )?;
            let stats_before: String = conn.query_row(
                "SELECT stats_json FROM crew_members WHERE id='crew-pico'",
                [],
                |r| r.get(0),
            )?;
            let events_before: i64 =
                conn.query_row("SELECT COUNT(*) FROM ctox_harness_flow_events", [], |r| {
                    r.get(0)
                })?;
            let error = prepare_attempt(
                root.path(),
                &[target.message_key.clone()],
                "crew-worker",
                "immutable-attempt",
                Some("original-thread"),
                &json!({}),
                None,
                "Code prüfen",
                None,
            )
            .expect_err("an incompatible replay must be rejected");
            assert_eq!(
                error.to_string(),
                if finalized {
                    "crew attempt is already finalized"
                } else {
                    "crew attempt belongs to another task"
                }
            );
            let binding: (Option<String>, Option<String>) = conn.query_row(
                "SELECT crew_member_id,crew_assigned_member_id FROM communication_routing_state
                 WHERE message_key=?1",
                [&target.message_key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            assert_eq!(binding, (None, Some("crew-nori".into())));
            let stored: (String, String, i64) = conn.query_row(
                "SELECT task_id,member_id,COUNT(*) FROM crew_attempts",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            assert_eq!(stored, (original.message_key, "crew-pico".into(), 1));
            let stats_after: String = conn.query_row(
                "SELECT stats_json FROM crew_members WHERE id='crew-pico'",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(stats_after, stats_before);
            let events_after: i64 =
                conn.query_row("SELECT COUNT(*) FROM ctox_harness_flow_events", [], |r| {
                    r.get(0)
                })?;
            assert_eq!(events_after, events_before);
        }
        Ok(())
    }

    #[test]
    fn lease_identity_and_literal_reason_survive_replay() -> Result<()> {
        let root = tempfile::tempdir()?;
        let task = crate::mission::channels::create_queue_task(
            root.path(),
            crate::mission::channels::QueueTaskCreateRequest {
                title: "Crew lease fixture".into(),
                prompt: "Code prüfen".into(),
                thread_key: "crew-thread".into(),
                workspace_root: None,
                priority: "normal".into(),
                suggested_skill: None,
                parent_message_key: None,
                extra_metadata: None,
            },
        )?;
        let conn = Connection::open(crate::paths::core_db(root.path()))?;
        conn.execute("UPDATE communication_routing_state SET route_status='leased',lease_owner='crew-worker',crew_assigned_member_id='crew-pico' WHERE message_key=?1",[&task.message_key])?;
        let first = prepare_attempt(
            root.path(),
            &[task.message_key.clone()],
            "crew-worker",
            "crew-attempt",
            Some("crew-thread"),
            &json!({}),
            None,
            "Code prüfen",
            None,
        )?;
        let second = prepare_attempt(
            root.path(),
            &[task.message_key.clone()],
            "crew-worker",
            "crew-attempt",
            Some("crew-thread"),
            &json!({}),
            None,
            "Code prüfen",
            None,
        )?;
        assert_eq!(first.unwrap().member_name, "Pico");
        assert_eq!(second.unwrap().member_name, "Pico");
        let (count,title,metadata):(i64,String,String)=conn.query_row(
            "SELECT COUNT(*),title,metadata_json FROM ctox_harness_flow_events WHERE event_kind='crew_selected' AND message_key=?1",[&task.message_key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        assert_eq!(count, 1);
        conn.execute("DELETE FROM ctox_harness_flow_events WHERE event_kind='crew_selected' AND message_key=?1",[&task.message_key])?;
        repair_selection_events(root.path(), &conn)?;
        let repaired:String=conn.query_row("SELECT title FROM ctox_harness_flow_events WHERE event_kind='crew_selected' AND message_key=?1",[&task.message_key],|r|r.get(0))?;
        assert_eq!(repaired, title);
        let plan=conn.prepare("EXPLAIN QUERY PLAN SELECT event_id FROM ctox_harness_flow_events WHERE event_kind='crew_selected' AND json_extract(metadata_json,'$.attempt_id')='crew-attempt'")?.query_map([],|r|r.get::<_,String>(3))?.collect::<rusqlite::Result<Vec<_>>>()?.join("\n");
        assert!(plan.contains("idx_crew_selection_event_attempt"), "{plan}");

        assert_eq!(
            json!(title),
            serde_json::from_str::<Value>(&metadata)?["reason"]
        );
        conn.execute("UPDATE communication_routing_state SET lease_owner='other-worker' WHERE message_key=?1", [&task.message_key])?;
        assert!(prepare_attempt(
            root.path(),
            &[task.message_key.clone()],
            "crew-worker",
            "lost-lease",
            Some("crew-thread"),
            &json!({}),
            None,
            "Code prüfen",
            None,
        )
        .is_err());
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM crew_attempts", [], |r| r.get(0))?;
        assert_eq!(count, 1, "losing a lease must not attach another attempt");
        assert_eq!(
            crate::mission::channels::load_queue_task(root.path(), &task.message_key)?
                .context("task missing")?
                .crew_member_id
                .as_deref(),
            Some("crew-pico")
        );
        Ok(())
    }
}
