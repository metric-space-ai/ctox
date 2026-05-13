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
    let lcm_db_path = root.join("runtime/ctox.sqlite3");
    let engine = lcm::LcmEngine::open(&lcm_db_path, lcm::LcmConfig::default())?;
    let continuity = engine.stored_continuity_show_all(conversation_id)?;
    let preview_synced_mission_state =
        engine.preview_mission_state_from_continuity(conversation_id)?;
    let mission_state = engine
        .stored_mission_state(conversation_id)?
        .unwrap_or_else(|| preview_synced_mission_state.clone());

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

    let resync_diffs = mission_state_differences(&mission_state, &preview_synced_mission_state);
    if !resync_diffs.is_empty() {
        violations.push(RuntimeStateInvariantViolation {
            code: "mission_state_requires_continuity_resync".to_string(),
            summary: "Mission state would change after a continuity resync.".to_string(),
            detail: format!(
                "stored mission_state diverges from continuity-derived state: {}",
                resync_diffs.join("; ")
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

fn mission_state_differences(
    stored: &lcm::MissionStateRecord,
    synced: &lcm::MissionStateRecord,
) -> Vec<String> {
    let mut diffs = Vec::new();
    push_mission_state_diff(&mut diffs, "mission", &stored.mission, &synced.mission);
    push_mission_state_diff(
        &mut diffs,
        "mission_status",
        &stored.mission_status,
        &synced.mission_status,
    );
    push_mission_state_diff(
        &mut diffs,
        "continuation_mode",
        &stored.continuation_mode,
        &synced.continuation_mode,
    );
    push_mission_state_diff(
        &mut diffs,
        "trigger_intensity",
        &stored.trigger_intensity,
        &synced.trigger_intensity,
    );
    push_mission_state_diff(&mut diffs, "blocker", &stored.blocker, &synced.blocker);
    push_mission_state_diff(
        &mut diffs,
        "next_slice",
        &stored.next_slice,
        &synced.next_slice,
    );
    push_mission_state_diff(
        &mut diffs,
        "done_gate",
        &stored.done_gate,
        &synced.done_gate,
    );
    push_mission_state_diff(
        &mut diffs,
        "closure_confidence",
        &stored.closure_confidence,
        &synced.closure_confidence,
    );
    if stored.is_open != synced.is_open {
        diffs.push(format!("is_open={} -> {}", stored.is_open, synced.is_open));
    }
    if stored.allow_idle != synced.allow_idle {
        diffs.push(format!(
            "allow_idle={} -> {}",
            stored.allow_idle, synced.allow_idle
        ));
    }
    if stored.focus_head_commit_id != synced.focus_head_commit_id {
        diffs.push(format!(
            "focus_head_commit_id={} -> {}",
            stored.focus_head_commit_id, synced.focus_head_commit_id
        ));
    }
    diffs
}

fn push_mission_state_diff(diffs: &mut Vec<String>, field: &str, stored: &str, synced: &str) {
    if normalize_token(stored) != normalize_token(synced) {
        diffs.push(format!("{field}={stored:?} -> {synced:?}"));
    }
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
    use anyhow::Context;
    use rusqlite::params;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root() -> PathBuf {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        path.push(format!("ctox-state-invariants-{nanos}"));
        path
    }

    fn seed_focus(root: &PathBuf, focus_diff: &str) -> anyhow::Result<LcmEngine> {
        fs::create_dir_all(root.join("runtime"))?;
        let db_path = root.join("runtime/ctox.sqlite3");
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
        let _ = engine.continuity_init_documents(1)?;
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
    fn detects_mission_state_that_requires_continuity_resync() -> anyhow::Result<()> {
        let root = temp_root();
        let engine = seed_focus(
            &root,
            "## Status\n+ Mission: Keep continuity truth primary after stale mission cache repair.\n+ Mission state: active.\n+ Continuation mode: continuous.\n+ Trigger intensity: hot.\n## Blocker\n+ Current blocker: stale mission cache rows may disagree with durable continuity.\n## Next\n+ Next slice: verify that durable continuity stays primary and record the recovery.\n## Done / Gate\n+ Done gate: keep the continuity-backed mission primary and leave exactly one bounded continuation open.\n+ Closure confidence: low.\n",
        )?;
        let current = engine
            .stored_mission_state(1)?
            .context("missing stored mission state")?;
        let db_path = root.join("runtime/ctox.sqlite3");
        let conn = rusqlite::Connection::open(&db_path)?;
        conn.execute(
            "UPDATE mission_states
             SET mission = ?1, mission_status = ?2, continuation_mode = ?3, trigger_intensity = ?4,
                 blocker = ?5, next_slice = ?6, done_gate = ?7, closure_confidence = ?8,
                 is_open = ?9, allow_idle = ?10, focus_head_commit_id = ?11
             WHERE conversation_id = 1",
            params![
                "Archive the stale mission cache row. This seeded value is not live work.",
                "done",
                "closed",
                "cold",
                "stale mission cache row only",
                "do not trust the stale mission cache row",
                "stale cache row should not decide the live mission",
                "high",
                0,
                1,
                current.focus_head_commit_id,
            ],
        )?;
        drop(conn);
        drop(engine);

        let report = evaluate_runtime_state_invariants(&root, 1)?;
        assert!(report
            .violations
            .iter()
            .any(|issue| issue.code == "mission_state_requires_continuity_resync"));

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
        engine.continuity_apply_diff(
            1,
            ContinuityKind::Focus,
            "## Status\n+ Mission: Keep the newest continuity head primary after partial-commit recovery.\n+ Trigger intensity: hot.\n## Blocker\n+ Current blocker: the live mission cache may still point at the old focus head.\n## Next\n+ Next slice: verify the newest focus head is the active runtime truth.\n## Done / Gate\n+ Done gate: keep the newest focus head primary and leave exactly one bounded continuation open.\n",
        )?;
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
