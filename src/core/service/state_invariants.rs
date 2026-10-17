use anyhow::Context;
use anyhow::Result;
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;

use crate::channels;
use crate::inference::turn_loop;
use crate::lcm;
use crate::plan;

const OPEN_QUEUE_STATUSES: &[&str] = &["pending", "leased", "blocked"];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RuntimeStateInvariantViolation {
    pub code: String,
    pub summary: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeStateInvariantReport {
    pub conversation_id: i64,
    pub mission_state: lcm::MissionStateRecord,
    pub continuity_focus_head_commit_id: String,
    pub open_queue_count: usize,
    pub open_plan_count: usize,
    pub open_queue_titles: Vec<String>,
    pub open_plan_titles: Vec<String>,
    pub open_work_titles: Vec<String>,
    pub violations: Vec<RuntimeStateInvariantViolation>,
}

impl RuntimeStateInvariantReport {
    pub fn is_clean(&self) -> bool {
        self.violations.is_empty()
    }
}

pub fn handle_state_invariants_command(root: &Path, args: &[String]) -> Result<()> {
    let conversation_id = find_flag_value(args, "--conversation-id")
        .map(|value| value.parse::<i64>())
        .transpose()
        .context("failed to parse --conversation-id")?
        .unwrap_or(turn_loop::CHAT_CONVERSATION_ID);
    let report = evaluate_runtime_state_invariants(root, conversation_id)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "ok": report.is_clean(),
            "report": report,
        }))?
    );
    Ok(())
}

pub fn evaluate_runtime_state_invariants(
    root: &Path,
    conversation_id: i64,
) -> Result<RuntimeStateInvariantReport> {
    let lcm_db_path = crate::paths::core_db(root);
    let engine = lcm::LcmClient::attach(&lcm_db_path)?;
    let continuity = engine.stored_continuity_show_all(conversation_id)?;
    // Inspect structured state without rendering or persisting anything. Focus
    // text is now a rendered view, not a second source of truth, so there is no
    // honest field-level "continuity resync" comparison to perform here. The
    // durable focus-head pointer comparison below remains the drift check.
    let mission_state = engine.peek_mission_state(conversation_id)?;

    let queue_tasks = channels::list_queue_tasks(
        root,
        &OPEN_QUEUE_STATUSES
            .iter()
            .map(|status| (*status).to_string())
            .collect::<Vec<_>>(),
        10_000,
    )?
    .into_iter()
    .filter(|task| {
        turn_loop::conversation_id_for_thread_key(Some(task.thread_key.as_str())) == conversation_id
    })
    .collect::<Vec<_>>();
    let open_queue_titles = queue_tasks
        .iter()
        .map(|task| task.title.trim())
        .filter(|title| !title.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    let plan_goals = plan::list_goals(root)?
        .into_iter()
        .filter(|goal| plan_goal_belongs_to_conversation(goal.thread_key.as_str(), conversation_id))
        .filter(|goal| goal.status != "completed")
        .collect::<Vec<_>>();
    let open_plan_titles = plan_goals
        .iter()
        .map(|goal| goal.title.trim())
        .filter(|title| !title.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    let open_queue_count = open_queue_titles.len();
    let open_plan_count = open_plan_titles.len();
    let open_runtime_work_count = open_queue_count + open_plan_count;
    let open_work_titles = open_queue_titles
        .iter()
        .cloned()
        .chain(open_plan_titles.iter().cloned())
        .collect::<Vec<_>>();

    let mut violations = Vec::new();
    let mission_status = normalize_token(&mission_state.mission_status);
    let continuation_mode = normalize_token(&mission_state.continuation_mode);

    if open_runtime_work_count > 0
        && (!mission_state.is_open
            || mission_status == "done"
            || continuation_mode == "closed"
            || continuation_mode == "dormant")
    {
        violations.push(RuntimeStateInvariantViolation {
            code: "closed_mission_with_open_runtime_work".to_string(),
            summary: "Mission state says closed while durable runtime work is still open."
                .to_string(),
            detail: format!(
                "mission_status={} continuation_mode={} is_open={} open_work_count={} titles={:?}",
                mission_state.mission_status,
                mission_state.continuation_mode,
                mission_state.is_open,
                open_runtime_work_count,
                open_work_titles
            ),
        });
    }

    if open_runtime_work_count > 0 && mission_state.allow_idle {
        violations.push(RuntimeStateInvariantViolation {
            code: "idle_allowed_with_open_runtime_work".to_string(),
            summary: "Mission allows idle while durable runtime work is still open.".to_string(),
            detail: format!(
                "allow_idle=true open_work_count={} titles={:?}",
                open_runtime_work_count, open_work_titles
            ),
        });
    }

    if mission_state.focus_head_commit_id != continuity.focus.head_commit_id {
        violations.push(RuntimeStateInvariantViolation {
            code: "mission_focus_head_mismatch".to_string(),
            summary: "Mission state is not synced to the latest focus continuity head.".to_string(),
            detail: format!(
                "mission_focus_head_commit_id={} continuity_focus_head_commit_id={}",
                mission_state.focus_head_commit_id, continuity.focus.head_commit_id
            ),
        });
    }

    let focus_conflicts = focus_semantic_conflicts(&continuity.focus.content);
    if !focus_conflicts.is_empty() {
        violations.push(RuntimeStateInvariantViolation {
            code: "focus_semantic_conflict".to_string(),
            summary: "Focus continuity contains conflicting values for the same semantic field."
                .to_string(),
            detail: focus_conflicts.join("; "),
        });
    }

    Ok(RuntimeStateInvariantReport {
        conversation_id,
        mission_state,
        continuity_focus_head_commit_id: continuity.focus.head_commit_id,
        open_queue_count,
        open_plan_count,
        open_queue_titles,
        open_plan_titles,
        open_work_titles,
        violations,
    })
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

fn plan_goal_belongs_to_conversation(thread_key: &str, conversation_id: i64) -> bool {
    if turn_loop::conversation_id_for_thread_key(Some(thread_key)) == conversation_id {
        return true;
    }
    conversation_id == turn_loop::CHAT_CONVERSATION_ID && thread_key.trim().starts_with("plan/")
}

fn normalize_token(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn focus_semantic_conflicts(content: &str) -> Vec<String> {
    let tracked_fields = [
        "Mission",
        "Mission state",
        "Continuation mode",
        "Trigger intensity",
        "Current blocker",
        "Next slice",
        "Done gate",
        "Closure confidence",
    ];
    let mut seen: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();

    for raw_line in content.lines() {
        let line = raw_line.trim_start_matches(['-', '+', '*', ' ']).trim();
        if line.is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        for field in tracked_fields {
            if normalize_token(name) == normalize_token(field) {
                let value = value.trim();
                if !value.is_empty() {
                    seen.entry(field).or_default().push(value.to_string());
                }
            }
        }
    }

    let mut conflicts = Vec::new();
    for (field, values) in seen {
        let mut distinct = Vec::new();
        for value in values {
            if !distinct
                .iter()
                .any(|existing: &String| normalize_token(existing) == normalize_token(&value))
            {
                distinct.push(value);
            }
        }
        if distinct.len() > 1 {
            conflicts.push(format!("{field} has conflicting values {:?}", distinct));
        }
    }
    conflicts
}

#[cfg(test)]
mod tests {
    use super::evaluate_runtime_state_invariants;
    use crate::channels;
    use crate::lcm::{ContinuityKind, LcmConfig, LcmEngine};
    use crate::plan;
    use rusqlite::params;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    // The timestamp alone is not unique: the test binary runs these in
    // parallel and two of them can start inside the same clock tick, which
    // made them share a root and stomp on each other's SQLite file — a
    // different test failed on almost every run. The counter makes the root
    // unique per test regardless of clock resolution.
    static TEMP_ROOT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn temp_root() -> PathBuf {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        let sequence = TEMP_ROOT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        path.push(format!(
            "ctox-state-invariants-{}-{nanos}-{sequence}",
            std::process::id()
        ));
        path
    }

    fn seed_focus(root: &PathBuf, focus_diff: &str) -> anyhow::Result<LcmEngine> {
        fs::create_dir_all(root.join("runtime"))?;
        let db_path = root.join("runtime/ctox.sqlite3");
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
        engine.continuity_apply_diff(1, ContinuityKind::Focus, focus_diff)?;
        Ok(engine)
    }

    #[test]
    fn detects_closed_mission_with_open_runtime_plan() -> anyhow::Result<()> {
        let root = temp_root();
        let _engine = seed_focus(
            &root,
            "## Status\n+ Mission: Legacy split-brain closure state.\n+ Mission state: done.\n+ Continuation mode: closed.\n+ Trigger intensity: cold.\n## Next\n+ Next slice: none.\n## Done / Gate\n+ Done gate: stale closure.\n+ Closure confidence: complete.\n",
        )?;
        plan::handle_plan_command(
            &root,
            &[
                "ingest".to_string(),
                "--title".to_string(),
                "canonical split brain continuation".to_string(),
                "--prompt".to_string(),
                "Reopen the canonical mission from split-brain state and leave exactly one open continuation.".to_string(),
            ],
        )?;

        let report = evaluate_runtime_state_invariants(&root, 1)?;
        assert_eq!(report.open_queue_count, 0);
        assert_eq!(report.open_plan_count, 1);
        assert!(report
            .violations
            .iter()
            .any(|issue| issue.code == "closed_mission_with_open_runtime_work"));
        assert!(report
            .violations
            .iter()
            .any(|issue| issue.code == "idle_allowed_with_open_runtime_work"));

        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn ignores_open_queue_work_from_other_conversations() -> anyhow::Result<()> {
        let root = temp_root();
        let _engine = seed_focus(
            &root,
            "## Status\n+ Mission: Keep the root chat mission clean.\n+ Mission state: done.\n+ Continuation mode: closed.\n+ Trigger intensity: cold.\n## Next\n+ Next slice: none.\n## Done / Gate\n+ Done gate: the root chat mission stays closed unless its own work reopens.\n+ Closure confidence: high.\n",
        )?;
        channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Unrelated queue mission".to_string(),
                prompt: "This belongs to queue/mission-1, not the root conversation.".to_string(),
                thread_key: "queue/mission-1".to_string(),
                workspace_root: None,
                priority: "high".to_string(),
                suggested_skill: None,
                parent_message_key: None,
                extra_metadata: None,
            },
        )?;

        let report = evaluate_runtime_state_invariants(&root, 1)?;
        assert_eq!(report.open_queue_count, 0);
        assert!(
            !report
                .violations
                .iter()
                .any(|issue| issue.code == "closed_mission_with_open_runtime_work"),
            "unexpected violations: {:?}",
            report.violations
        );

        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn detects_focus_head_mismatch_between_mission_state_and_continuity() -> anyhow::Result<()> {
        let root = temp_root();
        let engine = seed_focus(
            &root,
            "## Status\n+ Mission: Keep continuity primary.\n+ Mission state: active.\n+ Continuation mode: continuous.\n+ Trigger intensity: hot.\n## Next\n+ Next slice: verify the latest focus head.\n## Done / Gate\n+ Done gate: focus head stays aligned.\n+ Closure confidence: low.\n",
        )?;
        let db_path = root.join("runtime/ctox.sqlite3");
        let conn = rusqlite::Connection::open(&db_path)?;
        conn.execute(
            "UPDATE mission_states SET focus_head_commit_id = ?1 WHERE conversation_id = 1",
            params!["stale_focus_head"],
        )?;
        drop(conn);
        drop(engine);

        let report = evaluate_runtime_state_invariants(&root, 1)?;
        assert!(report
            .violations
            .iter()
            .any(|issue| issue.code == "mission_focus_head_mismatch"));

        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
        let stored = engine
            .stored_mission_state(1)?
            .expect("missing stored mission state after invariant inspection");
        assert_eq!(stored.focus_head_commit_id, "stale_focus_head");
        assert_eq!(
            engine.stored_continuity_show_all(1)?.focus.head_commit_id,
            report.continuity_focus_head_commit_id
        );
        drop(engine);

        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn stays_clean_when_active_mission_and_runtime_state_agree() -> anyhow::Result<()> {
        let root = temp_root();
        let _engine = seed_focus(
            &root,
            "## Status\n+ Mission: Reopen the canonical mission from durable evidence.\n+ Mission state: active.\n+ Continuation mode: continuous.\n+ Trigger intensity: hot.\n## Next\n+ Next slice: continue the canonical split brain continuation.\n## Done / Gate\n+ Done gate: one canonical continuation remains open.\n+ Closure confidence: low.\n",
        )?;
        plan::handle_plan_command(
            &root,
            &[
                "ingest".to_string(),
                "--title".to_string(),
                "canonical split brain continuation".to_string(),
                "--prompt".to_string(),
                "Continue the single canonical split-brain continuation.".to_string(),
            ],
        )?;

        let report = evaluate_runtime_state_invariants(&root, 1)?;
        assert!(
            report.is_clean(),
            "unexpected violations: {:?}",
            report.violations
        );

        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn detects_focus_semantic_conflict_from_duplicate_values() -> anyhow::Result<()> {
        let root = temp_root();
        let engine = seed_focus(
            &root,
            "## Status\n+ Mission: Old continuity head before partial-commit recovery.\n+ Mission state: active.\n+ Continuation mode: continuous.\n+ Trigger intensity: warm.\n## Blocker\n+ Current blocker: the recovery path still points at the old continuity head.\n## Next\n+ Next slice: advance to the new continuity head.\n## Done / Gate\n+ Done gate: resync the live mission state to the newest continuity head.\n+ Closure confidence: low.\n",
        )?;
        let focus_head = engine.stored_continuity_show_all(1)?.focus.head_commit_id;
        let db_path = root.join("runtime/ctox.sqlite3");
        let conn = rusqlite::Connection::open(&db_path)?;
        conn.execute(
            "UPDATE continuity_commits
             SET rendered_text = rendered_text || ?1
             WHERE commit_id = ?2",
            params![
                "\n- Mission: Keep the newest continuity head primary after partial-commit recovery.\n- Trigger intensity: hot.\n",
                focus_head,
            ],
        )?;
        drop(conn);
        drop(engine);

        let report = evaluate_runtime_state_invariants(&root, 1)?;
        assert!(report
            .violations
            .iter()
            .any(|issue| issue.code == "focus_semantic_conflict"));

        let _ = fs::remove_dir_all(root);
        Ok(())
    }
}
