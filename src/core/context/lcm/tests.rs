// In-tree behavioral tests for the lifecycle context manager (extracted
// from mod.rs; `use super::*` keeps access to crate-private internals).
use super::*;

fn temp_db() -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    let counter = TEMP_DB_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    path.push(format!("ctox-lcm-{nanos}-{counter}.sqlite"));
    path
}

fn remove_structured_mission_state(engine: &LcmEngine, conversation_id: i64) -> Result<()> {
    engine.conn.execute(
        "DELETE FROM mission_states WHERE conversation_id = ?1",
        [conversation_id],
    )?;
    Ok(())
}

#[test]
fn compacts_messages_and_supports_retrieval() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(
        &db_path,
        LcmConfig {
            context_threshold: 0.4,
            min_compaction_tokens: 0,
            fresh_tail_count: 2,
            // Batch the compactable messages into one leaf chunk and force
            // the heuristic summary to truncate well below the source, so
            // the leaf pass genuinely reduces tokens. Single-message chunks
            // (the old leaf_chunk_tokens: 20) produce a summary larger than
            // their source, which the lcm-x5 token-gate now correctly rolls
            // back — that regime is exercised by the RegressingSummarizer
            // test below, not here.
            leaf_chunk_tokens: 200,
            leaf_target_tokens: 30,
            condensed_target_tokens: 120,
            leaf_min_fanout: 3,
            condensed_min_fanout: 2,
            max_rounds: 4,
        },
    )?;

    for idx in 0..8 {
        engine.add_message(
            1,
            if idx % 2 == 0 { "user" } else { "assistant" },
            &format!("message {idx} about postgres migration planning and rollout details"),
        )?;
    }

    let result = engine.compact(1, 40, &HeuristicSummarizer, false)?;
    assert!(result.action_taken);
    assert!(!result.created_summary_ids.is_empty());

    let grep = engine.grep(Some(1), GrepScope::Both, GrepMode::FullText, "postgres", 10)?;
    assert!(grep.total_matches > 0);

    let described = engine.describe(&result.created_summary_ids[0])?;
    assert!(described.is_some());

    let expanded = engine.expand(&result.created_summary_ids[0], 1, true, 10_000)?;
    assert!(!expanded.messages.is_empty());

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn live_working_set_is_bounded_with_large_message_and_continuity_history() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let conversation_id = 50_000;
    let _ = engine.continuity_init_documents(conversation_id)?;
    let tx = engine.conn.unchecked_transaction()?;
    {
        let mut message = tx.prepare(
            "INSERT INTO messages (message_id, conversation_id, seq, role, content, token_count, created_at) VALUES (?1, ?2, ?3, 'user', ?4, 4, ?5)",
        )?;
        let mut context = tx.prepare(
            "INSERT INTO context_items (conversation_id, ordinal, item_type, message_id, summary_id, created_at) VALUES (?1, ?2, 'message', ?3, NULL, ?4)",
        )?;
        for seq in 1_i64..=50_000 {
            let created_at = format!(
                "2026-01-01T00:{:02}:{:02}.{:05}Z",
                (seq / 60) % 60,
                seq % 60,
                seq
            );
            message.execute(rusqlite::params![
                seq,
                conversation_id,
                seq,
                format!("historical message {seq}"),
                created_at,
            ])?;
            context.execute(rusqlite::params![conversation_id, seq, seq, created_at])?;
        }
    }
    let narrative_document: String = tx.query_row(
        "SELECT document_id FROM continuity_documents WHERE conversation_id=?1 AND kind='narrative'",
        rusqlite::params![conversation_id],
        |row| row.get(0),
    )?;
    {
        let mut commit = tx.prepare(
            "INSERT INTO continuity_commits (commit_id, document_id, parent_commit_id, diff_text, rendered_text, created_at) VALUES (?1, ?2, NULL, ?3, ?4, ?5)",
        )?;
        for idx in 0_i64..10_000 {
            commit.execute(rusqlite::params![
                format!("stress-commit-{idx:05}"),
                narrative_document,
                format!("- forgotten line {idx}\n+ replacement {idx}"),
                format!("historical rendered document {idx}"),
                format!("2026-02-01T00:00:{:02}.{:05}Z", idx % 60, idx),
            ])?;
        }
    }
    tx.commit()?;

    let working = engine.working_set_snapshot(conversation_id, 512)?;
    assert_eq!(working.context_items.len(), 512);
    assert_eq!(working.messages.len(), 512);
    assert_eq!(
        working.messages.first().map(|message| message.seq),
        Some(49_489)
    );
    assert_eq!(
        working.messages.last().map(|message| message.seq),
        Some(50_000)
    );
    let forgotten = engine.continuity_forgotten_recent(
        conversation_id,
        Some(ContinuityKind::Narrative),
        128,
    )?;
    assert!(forgotten.len() <= 128);
    assert!(forgotten
        .iter()
        .all(|entry| entry.line.contains("forgotten line")));
    let message_count: i64 = engine.conn.query_row(
        "SELECT COUNT(*) FROM messages WHERE conversation_id=?1",
        rusqlite::params![conversation_id],
        |row| row.get(0),
    )?;
    let commit_count: i64 = engine.conn.query_row(
        "SELECT COUNT(*) FROM continuity_commits c JOIN continuity_documents d ON d.document_id=c.document_id WHERE d.conversation_id=?1",
        rusqlite::params![conversation_id],
        |row| row.get(0),
    )?;
    assert_eq!(message_count, 50_000);
    assert!(commit_count >= 10_000);

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

/// A summarizer whose output is far larger than the source it replaces, so
/// a compaction pass that committed it unconditionally would durably
/// enlarge the context. The token-gate must roll the insert+delete back.
struct RegressingSummarizer;

impl Summarizer for RegressingSummarizer {
    fn summarize(
        &self,
        _kind: SummaryKind,
        _depth: i64,
        lines: &[String],
        _target_tokens: usize,
    ) -> Result<String> {
        Ok(lines.join(" ").repeat(8))
    }
}

#[test]
fn compact_never_enlarges_context_under_regressing_summarizer() -> Result<()> {
    let config = LcmConfig {
        context_threshold: 0.4,
        min_compaction_tokens: 0,
        fresh_tail_count: 2,
        leaf_chunk_tokens: 20,
        leaf_target_tokens: 120,
        condensed_target_tokens: 120,
        leaf_min_fanout: 3,
        condensed_min_fanout: 2,
        max_rounds: 4,
    };

    for force in [false, true] {
        let db_path = temp_db();
        let engine = LcmEngine::open(&db_path, config.clone())?;
        for idx in 0..8 {
            engine.add_message(
                1,
                if idx % 2 == 0 { "user" } else { "assistant" },
                &format!("message {idx} about postgres migration planning"),
            )?;
        }
        let tokens_before = engine.context_token_count(1)?;
        let result = engine.compact(1, 40, &RegressingSummarizer, force)?;
        assert_eq!(result.tokens_before, tokens_before);
        assert!(
            result.tokens_after <= result.tokens_before,
            "force={force}: tokens_after ({}) must not exceed tokens_before ({})",
            result.tokens_after,
            result.tokens_before,
        );
        assert!(
            engine.context_token_count(1)? <= tokens_before,
            "force={force}: persisted context must not be enlarged",
        );
        let _ = std::fs::remove_file(db_path);
    }
    Ok(())
}

#[test]
fn insert_summary_token_gated_rolls_back_a_regressing_leaf_pass() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;

    let m1 = engine.add_message(1, "user", "alpha")?;
    let m2 = engine.add_message(1, "assistant", "beta")?;
    let tokens_before = engine.context_token_count(1)?;
    let ordinal = engine.next_context_ordinal(1)?;

    // Content whose token estimate exceeds the removed source token sum, so
    // committing it would enlarge the context.
    let bloated = "x".repeat(4000);
    let outcome = engine.insert_summary_token_gated(
        1,
        SummaryKind::Leaf,
        0,
        &bloated,
        0,
        0,
        0,
        &[],
        vec![m1.message_id, m2.message_id],
        ordinal,
        vec![1, 2],
        tokens_before,
        false,
    )?;

    assert!(outcome.is_none(), "regressing pass must roll back");
    assert_eq!(
        engine.context_token_count(1)?,
        tokens_before,
        "rolled-back pass must leave the context token count unchanged",
    );
    assert_eq!(
        engine
            .get_summary(&summary_id_for(1, &bloated, 0))?
            .is_none(),
        true,
        "the rolled-back summary row must not persist",
    );

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn creates_condensed_summary_from_leaf_summaries() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(
        &db_path,
        LcmConfig {
            context_threshold: 0.2,
            min_compaction_tokens: 0,
            fresh_tail_count: 0,
            leaf_chunk_tokens: 60,
            leaf_target_tokens: 10,
            condensed_target_tokens: 10,
            leaf_min_fanout: 2,
            condensed_min_fanout: 2,
            max_rounds: 6,
        },
    )?;

    let leaf_a = engine.insert_summary(
        7,
        SummaryKind::Leaf,
        0,
        "leaf summary A with rollout evidence and retrieval details",
        0,
        0,
        24,
        &[],
        Vec::new(),
        0,
        Vec::new(),
    )?;
    let leaf_b = engine.insert_summary(
        7,
        SummaryKind::Leaf,
        0,
        "leaf summary B with fallback notes and verification details",
        0,
        0,
        26,
        &[],
        Vec::new(),
        1,
        Vec::new(),
    )?;

    let condensed_id = engine
        .compact_condensed_pass(7, &HeuristicSummarizer, true, i64::MAX)?
        .context("expected condensed summary")?;
    let condensed = engine
        .get_summary(&condensed_id)?
        .context("missing condensed summary")?;

    assert_eq!(condensed.kind, SummaryKind::Condensed);
    assert_eq!(condensed.depth, 1);
    assert_eq!(condensed.source_message_token_count, 50);
    assert_eq!(condensed.descendant_count, 2);
    assert_eq!(
        engine.summary_parent_ids(&leaf_a)?,
        vec![condensed_id.clone()]
    );
    assert_eq!(engine.summary_parent_ids(&leaf_b)?, vec![condensed_id]);

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn secret_rewrite_replaces_literals_across_memory_without_breaking_structure() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let literal = "sk-live-very-secret";
    let replacement = "[secret-ref:ticket/zammad/api-token]";

    engine.add_message(
        91,
        "user",
        &format!("Please use {literal} for the monitoring API"),
    )?;
    let ordinal = engine.next_context_ordinal(91)?;
    let _ = engine.insert_summary(
        91,
        SummaryKind::Leaf,
        0,
        &format!("Summary still mentions {literal} before hygiene."),
        0,
        0,
        0,
        &[],
        vec![],
        ordinal,
        vec![],
    )?;
    remove_structured_mission_state(&engine, 91)?;
    engine.continuity_apply_diff(
        91,
        ContinuityKind::Focus,
        &format!(
            "## Status\n+ Mission: stabilize monitoring with {literal}\n## Blocker\n+ Current blocker: waiting for {literal}\n"
        ),
    )?;

    let rewrite =
        engine.rewrite_secret_literal(91, "ticket/zammad", "api-token", literal, replacement)?;
    assert!(rewrite.message_rows_updated >= 1);
    assert!(rewrite.summary_rows_updated >= 1);
    assert!(rewrite.continuity_commit_rows_updated >= 1);

    let snapshot = engine.snapshot(91)?;
    assert!(snapshot
        .messages
        .iter()
        .all(|item| !item.content.contains(literal)));
    assert!(snapshot
        .messages
        .iter()
        .any(|item| item.content.contains(replacement)));
    assert!(snapshot
        .summaries
        .iter()
        .all(|item| !item.content.contains(literal)));
    assert!(snapshot
        .summaries
        .iter()
        .any(|item| item.content.contains(replacement)));

    let continuity = engine.continuity_show_all(91)?;
    assert!(!continuity.focus.content.contains(literal));
    assert!(continuity.focus.content.contains(replacement));

    let mission = engine.mission_state(91)?;
    assert!(!mission.blocker.contains(literal));

    let grep_old = engine.grep(Some(91), GrepScope::Both, GrepMode::FullText, literal, 10)?;
    assert_eq!(grep_old.total_matches, 0);
    let grep_new = engine.grep(Some(91), GrepScope::Both, GrepMode::FullText, "api", 10)?;
    assert!(grep_new.total_matches > 0);

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn new_session_starts_with_raw_continuity_templates() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;

    engine.add_message(9, "user", "First session message.")?;

    let current = engine
        .latest_continuity(9)?
        .context("expected continuity state")?;
    assert!(current.narrative.contains("# CONTINUITY NARRATIVE"));
    assert!(current.narrative.contains("## Situation"));
    assert!(current.anchors.contains("# CONTINUITY ANCHORS"));
    assert!(current.focus.contains("# ACTIVE FOCUS"));
    assert!(current.focus.contains("Mission:"));
    assert!(!current.narrative.contains("- "));

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn mission_state_tracks_focus_contract_and_preserves_watcher_metadata() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let _ = engine.continuity_init_documents(13)?;
    remove_structured_mission_state(&engine, 13)?;
    engine.continuity_apply_diff(
        13,
        ContinuityKind::Focus,
        "## Status\n+ Mission: Build and operate the Airbnb clone.\n+ Mission state: active.\n+ Continuation mode: continuous.\n+ Trigger intensity: hot.\n## Blocker\n+ Current blocker: none.\n## Next\n+ Next slice: implement the host onboarding shell.\n## Done / Gate\n+ Done gate: never claim completion while the capability audit is still open.\n+ Closure confidence: low.\n",
    )?;

    let mission = engine.mission_state(13)?;
    assert_eq!(mission.mission, "Build and operate the Airbnb clone.");
    assert_eq!(mission.mission_status, "active");
    assert_eq!(mission.continuation_mode, "continuous");
    assert_eq!(mission.trigger_intensity, "hot");
    assert!(mission.is_open);
    assert!(!mission.allow_idle);

    let triggered = engine.note_mission_watcher_triggered(13, "2026-03-31T12:00:00Z")?;
    assert_eq!(triggered.watcher_trigger_count, 1);
    assert_eq!(
        triggered.watcher_last_triggered_at.as_deref(),
        Some("2026-03-31T12:00:00Z")
    );

    let synced = engine.sync_mission_state_from_continuity(13)?;
    assert_eq!(synced.watcher_trigger_count, 1);
    assert_eq!(
        synced.watcher_last_triggered_at.as_deref(),
        Some("2026-03-31T12:00:00Z")
    );

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn mission_state_normalizes_free_form_focus_controls_and_preserves_mission() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let _ = engine.continuity_init_documents(17)?;
    remove_structured_mission_state(&engine, 17)?;
    engine.continuity_apply_diff(
        17,
        ContinuityKind::Focus,
        "## Status\n+ Mission: Keep the marketplace delivery mission durable.\n+ Mission state: Still open; roadmap and progress docs were updated, but the mission has not reached a stable stopping point.\n+ Continuation mode: Keep the discovery work attached to the marketplace core and continue the same durable slice.\n+ Trigger intensity: High while the mission remains open and idle-watch pressure is present.\n## Blocker\n+ Current blocker: Keep the discovery work attached to the marketplace core.\n## Next\n+ Next slice: Carry the discovery expectations through the roadmap slice.\n## Done / Gate\n+ Done gate: Preserve mission continuity while advancing durable slices.\n+ Closure confidence: Low until the marketplace core reaches a stable stopping point.\n",
    )?;
    let initial = engine.mission_state(17)?;
    assert_eq!(
        initial.mission,
        "Keep the marketplace delivery mission durable."
    );
    assert_eq!(initial.mission_status, "active");
    assert_eq!(initial.continuation_mode, "continuous");
    assert_eq!(initial.trigger_intensity, "hot");
    assert_eq!(initial.closure_confidence, "low");
    assert!(initial.is_open);
    assert!(!initial.allow_idle);

    engine.continuity_apply_diff(
        17,
        ContinuityKind::Focus,
        "## Status\n- Mission: Keep the marketplace delivery mission durable.\n## Next\n+ Next slice: Continue the same durable slice.\n",
    )?;
    let synced = engine.sync_mission_state_from_continuity(17)?;
    assert_eq!(
        synced.mission,
        "Keep the marketplace delivery mission durable."
    );
    assert_eq!(synced.continuation_mode, "continuous");
    assert_eq!(synced.trigger_intensity, "hot");

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn continuity_init_documents_keeps_mission_state_on_focus_head() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;

    let _ = engine.continuity_init_documents(18)?;
    let mission = engine.mission_state(18)?;
    let continuity = engine.stored_continuity_show_all(18)?;

    assert_eq!(
        mission.focus_head_commit_id,
        continuity.focus.head_commit_id
    );

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn continuity_apply_diff_updates_mission_state_to_new_focus_head() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let _ = engine.continuity_init_documents(19)?;
    remove_structured_mission_state(&engine, 19)?;

    let updated = engine.continuity_apply_diff(
        19,
        ContinuityKind::Focus,
        "## Status\n+ Mission: Keep continuity crash-safe.\n+ Mission state: active.\n## Next\n+ Next slice: verify the focus head remains aligned.\n## Done / Gate\n+ Done gate: mission state must stay on the latest focus head.\n",
    )?;
    let mission = engine.mission_state(19)?;

    assert_eq!(mission.focus_head_commit_id, updated.head_commit_id);
    assert_eq!(mission.mission, "Keep continuity crash-safe.");

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn focus_diff_mission_only_updates_mission_and_preserves_other_structured_fields() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let continuity = engine.continuity_init_documents(58)?;
    let baseline = MissionStateRecord {
        conversation_id: 58,
        mission: "Keep the original mission.".to_string(),
        mission_status: "blocked".to_string(),
        continuation_mode: "maintenance".to_string(),
        trigger_intensity: "warm".to_string(),
        blocker: "Waiting for the writeback fix.".to_string(),
        next_slice: "Apply a mission-only focus diff.".to_string(),
        done_gate: "All untouched fields survive the partial update.".to_string(),
        closure_confidence: "medium".to_string(),
        is_open: true,
        allow_idle: false,
        focus_head_commit_id: continuity.focus.head_commit_id,
        last_synced_at: iso_now(),
        watcher_last_triggered_at: Some("2026-07-31T09:00:00Z".to_string()),
        watcher_trigger_count: 4,
        agent_failure_count: 2,
        deferred_reason: Some("prior retry boundary".to_string()),
        rewrite_failure_count: 1,
    };
    engine.overwrite_mission_state(&baseline)?;
    let before = engine
        .stored_mission_state(58)?
        .context("baseline mission state missing")?;

    let focus = engine.continuity_apply_diff(
        58,
        ContinuityKind::Focus,
        "## Status\n+ Mission: Keep the mission-only writeback authoritative.\n",
    )?;
    let after = engine
        .stored_mission_state(58)?
        .context("updated mission state missing")?;

    assert_eq!(
        after.mission,
        "Keep the mission-only writeback authoritative."
    );
    assert_eq!(after.mission_status, before.mission_status);
    assert_eq!(after.continuation_mode, before.continuation_mode);
    assert_eq!(after.trigger_intensity, before.trigger_intensity);
    assert_eq!(after.blocker, before.blocker);
    assert_eq!(after.next_slice, before.next_slice);
    assert_eq!(after.done_gate, before.done_gate);
    assert_eq!(after.closure_confidence, before.closure_confidence);
    assert_eq!(after.is_open, before.is_open);
    assert_eq!(after.allow_idle, before.allow_idle);
    assert_eq!(
        after.watcher_last_triggered_at,
        before.watcher_last_triggered_at
    );
    assert_eq!(after.watcher_trigger_count, before.watcher_trigger_count);
    assert_eq!(after.agent_failure_count, before.agent_failure_count);
    assert_eq!(after.deferred_reason, before.deferred_reason);
    assert_eq!(after.rewrite_failure_count, before.rewrite_failure_count);
    assert_eq!(after.focus_head_commit_id, focus.head_commit_id);

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn consecutive_focus_diffs_persist_the_second_mission_value() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let continuity = engine.continuity_init_documents(59)?;
    engine.overwrite_mission_state(&MissionStateRecord {
        conversation_id: 59,
        mission: "Mission before either diff.".to_string(),
        mission_status: "active".to_string(),
        continuation_mode: "continuous".to_string(),
        trigger_intensity: "hot".to_string(),
        blocker: String::new(),
        next_slice: "Apply both focus diffs.".to_string(),
        done_gate: "The second value is canonical.".to_string(),
        closure_confidence: "low".to_string(),
        is_open: true,
        allow_idle: false,
        focus_head_commit_id: continuity.focus.head_commit_id,
        last_synced_at: iso_now(),
        watcher_last_triggered_at: None,
        watcher_trigger_count: 0,
        agent_failure_count: 0,
        deferred_reason: None,
        rewrite_failure_count: 0,
    })?;

    engine.continuity_apply_diff(
        59,
        ContinuityKind::Focus,
        "## Status\n+ Mission: First focus-diff mission.\n",
    )?;
    let focus = engine.continuity_apply_diff(
        59,
        ContinuityKind::Focus,
        "## Status\n+ Mission: Second focus-diff mission.\n",
    )?;
    let mission = engine.mission_state(59)?;

    assert_eq!(mission.mission, "Second focus-diff mission.");
    assert!(focus
        .content
        .contains("- Mission: Second focus-diff mission."));
    assert!(!focus.content.contains("First focus-diff mission."));

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn invalid_canonical_focus_control_value_fails_without_changing_state() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let continuity = engine.continuity_init_documents(60)?;
    engine.overwrite_mission_state(&MissionStateRecord {
        conversation_id: 60,
        mission: "Preserve valid state when a diff is broken.".to_string(),
        mission_status: "active".to_string(),
        continuation_mode: "continuous".to_string(),
        trigger_intensity: "warm".to_string(),
        blocker: "No blocker.".to_string(),
        next_slice: "Reject the invalid value atomically.".to_string(),
        done_gate: "The prior row and focus head remain unchanged.".to_string(),
        closure_confidence: "low".to_string(),
        is_open: true,
        allow_idle: false,
        focus_head_commit_id: continuity.focus.head_commit_id,
        last_synced_at: iso_now(),
        watcher_last_triggered_at: Some("2026-07-31T09:30:00Z".to_string()),
        watcher_trigger_count: 2,
        agent_failure_count: 1,
        deferred_reason: None,
        rewrite_failure_count: 3,
    })?;
    let before_state = engine
        .stored_mission_state(60)?
        .context("baseline mission state missing")?;
    let before_focus = engine.stored_continuity_show_all(60)?.focus;

    let error = engine
        .continuity_apply_diff(
            60,
            ContinuityKind::Focus,
            "## Status\n+ Mission state: bewildered.\n",
        )
        .expect_err("unknown canonical mission state must fail loudly");
    assert!(
        error
            .to_string()
            .contains("invalid canonical focus value for Mission state"),
        "unexpected error: {error:#}"
    );

    let after_state = engine
        .stored_mission_state(60)?
        .context("mission state missing after rejected diff")?;
    let after_focus = engine.stored_continuity_show_all(60)?.focus;
    assert_eq!(
        MissionStateFields::try_from_record(&after_state)?,
        MissionStateFields::try_from_record(&before_state)?
    );
    assert_eq!(
        after_state.focus_head_commit_id,
        before_state.focus_head_commit_id
    );
    assert_eq!(after_state.last_synced_at, before_state.last_synced_at);
    assert_eq!(
        after_state.watcher_last_triggered_at,
        before_state.watcher_last_triggered_at
    );
    assert_eq!(
        after_state.watcher_trigger_count,
        before_state.watcher_trigger_count
    );
    assert_eq!(
        after_state.agent_failure_count,
        before_state.agent_failure_count
    );
    assert_eq!(after_state.deferred_reason, before_state.deferred_reason);
    assert_eq!(
        after_state.rewrite_failure_count,
        before_state.rewrite_failure_count
    );
    assert_eq!(after_focus.head_commit_id, before_focus.head_commit_id);
    assert_eq!(after_focus.content, before_focus.content);

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn structured_mission_state_round_trips_without_changing_runtime_fields() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let continuity = engine.continuity_init_documents(52)?;
    let record = MissionStateRecord {
        conversation_id: 52,
        mission: "Keep typed mission state authoritative.".to_string(),
        mission_status: "blocked".to_string(),
        continuation_mode: "continuous".to_string(),
        trigger_intensity: "warm".to_string(),
        blocker: "Waiting for schema verification.".to_string(),
        next_slice: "Verify the persisted typed fieldset.".to_string(),
        done_gate: "Roundtrip every runtime control unchanged.".to_string(),
        closure_confidence: "medium".to_string(),
        is_open: true,
        allow_idle: false,
        focus_head_commit_id: continuity.focus.head_commit_id,
        last_synced_at: iso_now(),
        watcher_last_triggered_at: Some("2026-07-31T08:00:00Z".to_string()),
        watcher_trigger_count: 3,
        agent_failure_count: 2,
        deferred_reason: None,
        rewrite_failure_count: 1,
    };
    let expected = MissionStateFields::try_from_record(&record)?;

    engine.overwrite_mission_state(&record)?;
    let stored = engine
        .stored_mission_state(52)?
        .context("structured mission state missing after write")?;
    let actual = MissionStateFields::try_from_record(&stored)?;

    assert_eq!(actual, expected);
    assert_eq!(
        stored.watcher_last_triggered_at,
        record.watcher_last_triggered_at
    );
    assert_eq!(stored.watcher_trigger_count, 3);
    assert_eq!(stored.agent_failure_count, 2);
    assert_eq!(stored.rewrite_failure_count, 1);

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn legacy_status_and_mission_state_blocked_import_to_same_typed_state() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let legacy_status = engine.continuity_init_documents(53)?;
    let mission_state = engine.continuity_init_documents(55)?;
    let common = "\n- Mission: Keep blocked work visible.\n- Continuation mode: continuous.\n- Trigger intensity: hot.\n## Blocker\n- Current blocker: dependency unavailable.\n## Next\n- Next slice: retry after dependency recovery.\n## Done / Gate\n- Done gate: dependency verified.\n- Closure confidence: low.\n";
    engine.conn.execute(
        "UPDATE continuity_commits SET rendered_text = ?1 WHERE commit_id = ?2",
        params![
            format!("# ACTIVE FOCUS\n\n## Status\n- Status: blocked.{common}"),
            legacy_status.focus.head_commit_id,
        ],
    )?;
    engine.conn.execute(
        "UPDATE continuity_commits SET rendered_text = ?1 WHERE commit_id = ?2",
        params![
            format!("# ACTIVE FOCUS\n\n## Status\n- Mission state: blocked.{common}"),
            mission_state.focus.head_commit_id,
        ],
    )?;
    remove_structured_mission_state(&engine, 53)?;
    remove_structured_mission_state(&engine, 55)?;

    let from_status = engine.mission_state(53)?;
    let from_mission_state = engine.mission_state(55)?;

    assert_eq!(from_status.mission_status, "blocked");
    assert_eq!(
        MissionStateFields::try_from_record(&from_status)?,
        MissionStateFields::try_from_record(&from_mission_state)?
    );

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn structured_mission_state_renders_over_conflicting_focus_text_without_reimport() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let _ = engine.continuity_init_documents(54)?;
    remove_structured_mission_state(&engine, 54)?;
    engine.continuity_apply_diff(
        54,
        ContinuityKind::Focus,
        "## Status\n+ Mission: Keep the structured focus head primary.\n+ Mission state: active.\n+ Continuation mode: continuous.\n+ Trigger intensity: warm.\n## Blocker\n+ Current blocker: none.\n## Next\n+ Next slice: render from typed state.\n## Done / Gate\n+ Done gate: conflicting text cannot replace typed state.\n+ Closure confidence: low.\n",
    )?;
    let baseline = engine.mission_state(54)?;
    let head = engine.stored_continuity_show_all(54)?.focus.head_commit_id;
    engine.conn.execute(
        "UPDATE continuity_commits SET rendered_text = rendered_text || ?1 WHERE commit_id = ?2",
        params![
            "\n## Status\n- Mission: Malicious text-only replacement.\n- Mission state: done.\n- Trigger intensity: hot.\n",
            head,
        ],
    )?;

    let before = engine.stored_continuity_show_all(54)?;
    assert!(!focus_semantic_conflicts_local(&before.focus.content).is_empty());

    let render = engine.sync_mission_state_from_continuity_with_repair(54)?;
    let after = engine.stored_continuity_show_all(54)?;

    assert!(render.focus_repaired);
    assert_eq!(render.mission_state.mission, baseline.mission);
    assert_eq!(render.mission_state.mission_status, baseline.mission_status);
    assert_eq!(
        render.mission_state.trigger_intensity,
        baseline.trigger_intensity
    );
    assert!(focus_semantic_conflicts_local(&after.focus.content).is_empty());
    assert!(!after
        .focus
        .content
        .contains("Malicious text-only replacement"));
    assert!(after
        .focus
        .content
        .contains("Mission: Keep the structured focus head primary."));

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn mission_state_does_not_close_when_done_gate_mentions_completed_slice() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let _ = engine.continuity_init_documents(23)?;
    remove_structured_mission_state(&engine, 23)?;
    engine.continuity_apply_diff(
        23,
        ContinuityKind::Focus,
        "## Status\n+ Mission: Keep the marketplace core as the main thread, with discovery features folded into the same slice.\n+ Mission state: Slice 2 remains the main marketplace thread; a trust-and-safety response slice is now tracked alongside it for the suspicious host cluster.\n+ Continuation mode: Keep the main mission thread intact, advance Slice 2, and keep the trust-and-safety response slice as a contained response path.\n+ Trigger intensity: High until Slice 2 and the trust-and-safety response slice are both advanced and recorded.\n## Blocker\n+ Current blocker: Suspicious new host cluster with repeated near-duplicate listings and mismatched identity signals needs a concrete response without derailing the marketplace core.\n## Next\n+ Next slice: Advance the marketplace core slice with mobile-first search, map-based discovery, and saved-search support, while keeping the trust-and-safety response slice contained and tracked.\n## Done / Gate\n+ Done gate: Slice completed cleanly with continuity preserved, discovery kept inside the marketplace core thread, and the trust-and-safety response slice documented without displacing the main roadmap.\n+ Closure confidence: Low until Slice 2 and the trust-and-safety response path are both stable.\n",
    )?;

    let mission = engine.mission_state(23)?;
    assert_eq!(mission.mission_status, "active");
    assert_eq!(mission.continuation_mode, "continuous");
    assert_eq!(mission.trigger_intensity, "hot");
    assert_eq!(mission.closure_confidence, "low");
    assert!(mission.is_open);
    assert!(!mission.allow_idle);

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn legacy_mission_state_import_ignores_empty_focus_template_placeholders() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let continuity = engine.continuity_init_documents(29)?;

    let mission = import_legacy_mission_state(&continuity);
    assert_eq!(mission.mission, "");
    assert_eq!(mission.blocker, "");
    assert_eq!(mission.next_slice, "");
    assert_eq!(mission.done_gate, "");
    // An untouched template is not an active-but-closed mission. Queue-backed
    // creation promotes this honest empty state atomically when work exists.
    assert_eq!(mission.mission_status, "dormant");
    assert_eq!(mission.continuation_mode, "dormant");
    assert_eq!(mission.trigger_intensity, "archive");
    assert!(!mission.is_open);
    assert!(mission.allow_idle);
    assert_eq!(mission.watcher_trigger_count, 0);
    assert!(mission.watcher_last_triggered_at.is_none());

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn empty_active_continuous_rows_migrate_once_per_database() -> Result<()> {
    let db_path = temp_db();
    let conn = Connection::open(&db_path)?;
    ensure_mission_state_storage_with(&conn)?;
    conn.execute(
        "INSERT INTO mission_states (
            conversation_id, mission, mission_status, continuation_mode,
            trigger_intensity, blocker, next_slice, done_gate,
            closure_confidence, is_open, allow_idle, focus_head_commit_id,
            last_synced_at, watcher_last_triggered_at, watcher_trigger_count,
            agent_failure_count, deferred_reason, rewrite_failure_count,
            structured_state_version
         ) VALUES (?1, '', 'active', 'continuous', 'hot', '', '', '', 'low',
                   0, 0, '', ?2, NULL, 0, 0, NULL, 0, 1)",
        params![71_i64, iso_now()],
    )?;

    migrate_empty_mission_split_brain_with(&conn)?;
    let migrated = conn.query_row(
        "SELECT mission_status, continuation_mode, trigger_intensity, is_open, allow_idle
         FROM mission_states WHERE conversation_id = ?1",
        params![71_i64],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, bool>(3)?,
                row.get::<_, bool>(4)?,
            ))
        },
    )?;
    assert_eq!(
        migrated,
        (
            "dormant".to_string(),
            "dormant".to_string(),
            "archive".to_string(),
            false,
            true,
        )
    );
    let marker_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM lcm_data_migrations WHERE migration_id = ?1",
        params![EMPTY_MISSION_SPLIT_BRAIN_MIGRATION],
        |row| row.get(0),
    )?;
    assert_eq!(marker_count, 1);

    // Prove the durable marker, rather than a process-local path cache, owns
    // once-per-database behavior: recreating the legacy values stays untouched
    // on a second explicit migration pass.
    conn.execute(
        "UPDATE mission_states
         SET mission_status = 'active', continuation_mode = 'continuous',
             trigger_intensity = 'hot', is_open = 0, allow_idle = 0
         WHERE conversation_id = ?1",
        params![71_i64],
    )?;
    migrate_empty_mission_split_brain_with(&conn)?;
    let status: String = conn.query_row(
        "SELECT mission_status FROM mission_states WHERE conversation_id = ?1",
        params![71_i64],
        |row| row.get(0),
    )?;
    assert_eq!(status, "active");

    drop(conn);
    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn mission_state_accepts_open_and_partial_focus_values_without_falling_back() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let _ = engine.continuity_init_documents(49)?;
    remove_structured_mission_state(&engine, 49)?;
    engine.continuity_apply_diff(
        49,
        ContinuityKind::Focus,
        "## Status\n+ Mission: Rehydrate Split Brain Gate (reconcile seeded contradiction; treat live runtime track as canonical).\n+ Mission state: in_progress.\n+ Continuation mode: open.\n+ Trigger intensity: cold.\n## Blocker\n+ Current blocker: Seeded focus marks closed, but live runtime has work.\n## Next\n+ Next slice: Persist exactly one open canonical continuation in runtime state.\n## Done / Gate\n+ Done gate: Keep the continuation open until runtime state is verified.\n+ Closure confidence: partial (until runtime state is verified).\n",
    )?;
    let continuity = engine.continuity_show_all(49)?;
    let mission = import_legacy_mission_state(&continuity);
    assert_eq!(
        mission.mission,
        "Rehydrate Split Brain Gate (reconcile seeded contradiction; treat live runtime track as canonical)."
    );
    assert_eq!(mission.mission_status, "active");
    assert_eq!(mission.continuation_mode, "continuous");
    assert_eq!(mission.closure_confidence, "low");
    assert!(mission.is_open);
    assert!(!mission.allow_idle);

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn mission_state_keeps_explicit_blank_focus_fields_blank() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let _ = engine.continuity_init_documents(57)?;
    remove_structured_mission_state(&engine, 57)?;
    engine.continuity_apply_diff(
        57,
        ContinuityKind::Focus,
        "## Status\n+ Mission: Keep the restore follow-up open until queue pressure drops.\n+ Mission state: active.\n+ Continuation mode: continuous.\n+ Trigger intensity: hot.\n## Blocker\n+ Current blocker:\n## Next\n+ Next slice:\n## Done / Gate\n+ Done gate:\n+ Closure confidence: low.\n",
    )?;

    let mission = engine.mission_state(57)?;
    assert_eq!(mission.blocker, "");
    assert_eq!(mission.next_slice, "");
    assert_eq!(mission.done_gate, "");
    assert_eq!(mission.closure_confidence, "low");

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn verification_runs_and_open_claims_persist_in_lcm_db() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let created_at = iso_now();
    let run = VerificationRunRecord {
        run_id: verification_run_id(
            41,
            "queue",
            "Repair deployment",
            "Repair deployment",
            "Deployment still looked broken after the patch.",
            &created_at,
        ),
        conversation_id: 41,
        source_label: "queue".to_string(),
        goal: "Repair deployment".to_string(),
        preview: "Repair deployment".to_string(),
        result_excerpt: "Deployment still looked broken after the patch.".to_string(),
        blocker: None,
        review_required: true,
        review_verdict: "fail".to_string(),
        review_summary: "HTTP health check still returns 502.".to_string(),
        review_score: 4,
        review_reasons: vec![
            "closure_claim".to_string(),
            "runtime_or_infra_change".to_string(),
        ],
        report_excerpt: "VERDICT: FAIL".to_string(),
        raw_report: "VERDICT: FAIL\nMISSION_STATE: UNHEALTHY".to_string(),
        mission_state: "UNHEALTHY".to_string(),
        failed_gates: vec!["HTTP health check still returns 502.".to_string()],
        semantic_findings: vec!["Deployment is still unhealthy.".to_string()],
        open_items: vec!["Repair upstream health failure.".to_string()],
        evidence: vec!["curl /health => 502".to_string()],
        handoff: None,
        claim_count: 2,
        open_claim_count: 2,
        closure_blocking_claim_count: 2,
        created_at: created_at.clone(),
    };
    let claims = vec![
        MissionClaimRecord {
            claim_key: mission_claim_key(41, "operational_state", "Repair deployment"),
            conversation_id: 41,
            last_run_id: run.run_id.clone(),
            claim_kind: "operational_state".to_string(),
            claim_status: "needs_recheck".to_string(),
            blocks_closure: true,
            subject: "Repair deployment".to_string(),
            summary: "Operational state still needs live revalidation.".to_string(),
            evidence_summary: "Review FAIL: HTTP health check still returns 502.".to_string(),
            recheck_policy: "revalidate_live_state_before_close".to_string(),
            expires_at: None,
            created_at: created_at.clone(),
            updated_at: created_at.clone(),
        },
        MissionClaimRecord {
            claim_key: mission_claim_key(41, "completion_gate", "Repair deployment"),
            conversation_id: 41,
            last_run_id: run.run_id.clone(),
            claim_kind: "completion_gate".to_string(),
            claim_status: "needs_recheck".to_string(),
            blocks_closure: true,
            subject: "Repair deployment".to_string(),
            summary: "Completion gate must stay open.".to_string(),
            evidence_summary: "Review FAIL: HTTP health check still returns 502.".to_string(),
            recheck_policy: "keep_open_until_supporting_claims_verified".to_string(),
            expires_at: None,
            created_at: created_at.clone(),
            updated_at: created_at.clone(),
        },
    ];
    engine.persist_verification_run(&run, &claims)?;

    let latest = engine
        .latest_verification_run(41)?
        .context("expected latest verification run")?;
    assert_eq!(latest.run_id, run.run_id);
    assert_eq!(latest.open_claim_count, 2);

    let assurance = engine.mission_assurance_snapshot(41)?;
    assert_eq!(assurance.open_claims.len(), 2);
    assert_eq!(assurance.closure_blocking_claims.len(), 2);

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn strategic_directives_are_versioned_and_activate_cleanly() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let active = engine.create_strategic_directive(
        99,
        Some("example-supervisor"),
        "mission",
        "Launch the platform",
        "Build a credible marketplace for hiring AI employees.",
        "active",
        "founder",
        Some("initial mission"),
    )?;
    let proposed = engine.create_strategic_directive(
        99,
        Some("example-supervisor"),
        "mission",
        "Tighten the launch scope",
        "Start with three hireable roles and a clear interview-to-hire path.",
        "proposed",
        "ctox",
        Some("scope refinement"),
    )?;
    let activated = engine.activate_strategic_directive(
        &proposed.directive_id,
        "founder",
        Some("approved refinement"),
    )?;
    let snapshot = engine.active_strategy_snapshot(99, Some("example-supervisor"))?;
    assert_eq!(
        snapshot
            .active_mission
            .as_ref()
            .map(|item| item.directive_id.clone()),
        Some(activated.directive_id.clone())
    );
    let history =
        engine.list_strategic_directives(99, Some("example-supervisor"), Some("mission"), 10)?;
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].status, "active");
    let superseded = history
        .iter()
        .find(|item| item.directive_id == active.directive_id)
        .context("missing superseded mission revision")?;
    assert_eq!(superseded.status, "superseded");
    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn continuity_diff_documents_apply_and_track_forgotten_lines() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;

    let docs = engine.continuity_init_documents(11)?;
    assert!(docs.narrative.content.contains("## Entries"));

    let updated = engine.continuity_apply_diff(
        11,
        ContinuityKind::Narrative,
        "## Entries\n+ entry_id: rollout-break\n+ event_type: failure\n+ summary: Service started with a fragile migration plan.\n+ consequence: Cache warmer timing caused the breakage.\n+ source_class: tool_observed\n+ source_ref: log://deploy\n+ observed_at: 2026-04-02T10:00:00Z\n",
    )?;
    assert!(updated
        .content
        .contains("Service started with a fragile migration plan."));
    assert!(updated
        .content
        .contains("Cache warmer timing caused the breakage."));

    let updated_again = engine.continuity_apply_diff(
        11,
        ContinuityKind::Narrative,
        "## Entries\n- consequence: Cache warmer timing caused the breakage.\n+ consequence: Cache warmer timing after verification caused the breakage.\n",
    )?;
    assert!(updated_again
        .content
        .contains("Cache warmer timing after verification caused the breakage."));
    assert!(!updated_again
        .content
        .contains("Cache warmer timing caused the breakage."));

    let forgotten =
        engine.continuity_forgotten(11, Some(ContinuityKind::Narrative), Some("Cache warmer"))?;
    assert_eq!(forgotten.len(), 1);
    assert!(forgotten[0]
        .line
        .contains("Cache warmer timing caused the breakage."));

    let rebuilt = engine.continuity_rebuild(11, ContinuityKind::Narrative)?;
    assert_eq!(rebuilt.content, updated_again.content);

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn continuity_apply_diff_accepts_headerless_anchor_entries() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let _ = engine.continuity_init_documents(46)?;

    let updated = engine.continuity_apply_diff(
        46,
        ContinuityKind::Anchors,
        "+ anchor_id: ANCHOR_MAIN_GATEWAY\n+ anchor_type: invariant\n+ statement: Keep the gateway mission primary.\n+ source_class: assistant_reply\n",
    )?;

    assert!(updated.content.contains("ANCHOR_MAIN_GATEWAY"));
    assert!(updated
        .content
        .contains("Keep the gateway mission primary."));

    let rebuilt = engine.continuity_rebuild(46, ContinuityKind::Anchors)?;
    assert_eq!(rebuilt.content, updated.content);

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn continuity_apply_diff_routes_headerless_focus_fields_to_known_sections() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let _ = engine.continuity_init_documents(47)?;
    remove_structured_mission_state(&engine, 47)?;

    let updated = engine.continuity_apply_diff(
        47,
        ContinuityKind::Focus,
        "+ Mission: Keep gateway intake hardening as the main mission.\n+ Mission state: active.\n+ Next slice: record the interrupt buffer without changing the main mission.\n+ Done gate: leave exactly one bounded continuation open.\n+ mission: keep gateway intake hardening primary\n+ next_slice: record interrupt buffer and return to the main thread\n",
    )?;

    assert!(updated
        .content
        .contains("Mission: keep gateway intake hardening primary"));
    assert!(updated
        .content
        .contains("Next slice: record interrupt buffer and return to the main thread"));
    assert!(updated
        .content
        .contains("mission: keep gateway intake hardening primary"));
    assert!(updated
        .content
        .contains("next_slice: record interrupt buffer and return to the main thread"));

    let mission = engine.mission_state(47)?;
    assert_eq!(mission.mission, "keep gateway intake hardening primary");
    assert_eq!(mission.mission_status, "active");
    assert!(mission.is_open);

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn continuity_apply_diff_accepts_indented_focus_diff_lines() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let _ = engine.continuity_init_documents(48)?;
    remove_structured_mission_state(&engine, 48)?;

    let updated = engine.continuity_apply_diff(
        48,
        ContinuityKind::Focus,
        "  + Mission: Keep gateway intake hardening as the main mission.\n  + Mission state: active.\n  + Next slice: record the interrupt buffer without changing the main mission.\n  + Done gate: leave exactly one bounded continuation open.\n  + mission: keep gateway intake hardening primary\n  - none\n",
    )?;

    assert!(updated
        .content
        .contains("Mission: keep gateway intake hardening primary"));
    assert!(updated.content.contains("## Next"));
    assert!(focus_semantic_conflicts_local(&updated.content).is_empty());

    let mission = engine.mission_state(48)?;
    assert_eq!(mission.mission, "keep gateway intake hardening primary");
    assert_eq!(mission.next_slice, "");
    assert_eq!(mission.mission_status, "active");
    assert!(mission.is_open);

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn continuity_prompt_contains_document_and_diff_rules() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let _ = engine.continuity_init_documents(12)?;
    engine.add_message(
        12,
        "user",
        "Keep the rollout gate active until validation passes on db-prod.internal.",
    )?;

    let payload = engine.continuity_build_prompt(12, ContinuityKind::Narrative)?;
    assert!(payload
        .prompt
        .contains("Reply with only a diff that uses the existing sections."));
    assert!(payload.prompt.contains("<CURRENT_DOCUMENT>"));
    assert!(payload.prompt.contains("<RECENT_MESSAGES>"));
    assert!(payload.prompt.contains("## Entries"));
    assert!(payload
        .prompt
        .contains("The first non-empty diff line must be a `## ...` section header"));
    assert!(payload.prompt.contains("Example valid diff:"));

    let focus_payload = engine.continuity_build_prompt(12, ContinuityKind::Focus)?;
    assert!(focus_payload.prompt.contains("mission_state:"));
    assert!(focus_payload.prompt.contains("continuation_mode:"));
    assert!(focus_payload.prompt.contains("next_slice:"));
    assert!(focus_payload
        .prompt
        .contains("update both `## Status` and `## Contract`/`## State`"));
    assert!(focus_payload
        .prompt
        .contains("Do not keep stale closed fields"));
    assert!(focus_payload
        .prompt
        .contains("+ Continuation mode: continuous"));

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn focus_continuity_prompt_keeps_open_continuation_signal_from_long_assistant_reply() -> Result<()>
{
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let _ = engine.continuity_init_documents(13)?;
    engine.add_message(
        13,
        "assistant",
        "Mission\n\nRehydrated the partial-commit state to the newest durable focus head and kept that head as the active truth.\n\nCompleted\n\n- Verified continuity focus head is current.\n- Verified the old head is no longer authoritative.\n- Created both required workspace artifacts.\n- Verified mission-state focus_head_commit_id is resynced to the newest head.\n- Left exactly 1 open CTOX runtime item: active plan `partial commit resync: verify restart stays on new head`.\n- Verified runtime open work counts: `ctox plan list` = 1, `ctox queue list` = 0.\n\nArtifacts\n\n- `docs/partial-commit-recovery.md`\n- `ops/progress/progress-latest.md`\n\nNext\n\n- Open bounded continuation: `partial commit resync: verify restart stays on new head`.",
    )?;

    let focus_payload = engine.continuity_build_prompt(13, ContinuityKind::Focus)?;
    assert!(focus_payload
        .prompt
        .contains("Left exactly 1 open CTOX runtime item"));
    assert!(focus_payload
        .prompt
        .contains("partial commit resync: verify restart stays on new head"));

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn continuity_anchor_prompt_preserves_explicit_literals() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let _ = engine.continuity_init_documents(44)?;
    engine.add_message(
        44,
        "assistant",
        "Retained the same three anchors:\n- `ANCHOR_REDWOOD`\n- `ANCHOR_GLASS_BRIDGE`\n- `ANCHOR_QUEUE_LANTERN`\nAlso kept `BENCH_CORE_CONTINUITY`.",
    )?;

    let payload = engine.continuity_build_prompt(44, ContinuityKind::Anchors)?;
    assert!(payload.prompt.contains("<EXPLICIT_ANCHOR_LITERALS>"));
    assert!(payload.prompt.contains("ANCHOR_REDWOOD"));
    assert!(payload.prompt.contains("ANCHOR_GLASS_BRIDGE"));
    assert!(payload.prompt.contains("ANCHOR_QUEUE_LANTERN"));
    assert!(payload.prompt.contains("BENCH_CORE_CONTINUITY"));

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn continuity_preserve_recent_anchor_literals_adds_missing_tokens() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let _ = engine.continuity_init_documents(45)?;
    engine.add_message(
        45,
        "assistant",
        "Retained the same three anchors without change:\n- `ANCHOR_REDWOOD`\n- `ANCHOR_GLASS_BRIDGE`\n- `ANCHOR_QUEUE_LANTERN`",
    )?;

    let updated = engine
        .continuity_preserve_recent_anchor_literals(45)?
        .context("expected literal preservation diff")?;
    assert!(updated.content.contains("ANCHOR_REDWOOD"));
    assert!(updated.content.contains("ANCHOR_GLASS_BRIDGE"));
    assert!(updated.content.contains("ANCHOR_QUEUE_LANTERN"));

    let repeated = engine.continuity_preserve_recent_anchor_literals(45)?;
    assert!(repeated.is_none());

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn sentence_fragment_handles_multibyte_unicode_boundaries() {
    let content = "### Mission Diagnose the real bug with “smart quotes” intact.";
    let fragment = sentence_fragment(content, 42);
    assert!(fragment.ends_with("..."));
    assert!(fragment.contains('“'));
    assert!(std::str::from_utf8(fragment.as_bytes()).is_ok());
}

#[test]
fn deterministic_fallback_handles_multibyte_unicode_boundaries() {
    let content = "é".repeat(FALLBACK_MAX_CHARS + 8);
    let fallback = build_deterministic_fallback(&content, 1234);
    assert!(fallback.contains("[Truncated from 1234 tokens]"));
    assert!(std::str::from_utf8(fallback.as_bytes()).is_ok());
}

// F3: structured agent_outcome round-trips on assistant rows and is
// ignored on non-assistant rows.
#[test]
fn add_message_with_outcome_persists_for_assistant_only() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let _ = engine.continuity_init_documents(7)?;

    // user row: outcome must be ignored.
    let user_record =
        engine.add_message_with_outcome(7, "user", "ping", Some(AgentOutcome::TurnTimeout))?;
    assert!(user_record.agent_outcome.is_none());

    // assistant success row.
    let success_record =
        engine.add_message_with_outcome(7, "assistant", "all done", Some(AgentOutcome::Success))?;
    assert_eq!(success_record.agent_outcome.as_deref(), Some("Success"));
    assert_eq!(engine.last_agent_outcome(7)?, Some(AgentOutcome::Success));

    // assistant timeout row supersedes the success.
    let _ = engine.add_message_with_outcome(
        7,
        "assistant",
        "(agent turn did not complete)",
        Some(AgentOutcome::TurnTimeout),
    )?;
    assert_eq!(
        engine.last_agent_outcome(7)?,
        Some(AgentOutcome::TurnTimeout)
    );

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn add_message_with_outcome_rolls_back_on_partial_failure() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let _ = engine.continuity_init_documents(7)?;
    engine.add_message_with_outcome(7, "user", "ping", None)?;

    let messages_before: i64 = engine.conn.query_row(
        "SELECT COUNT(*) FROM messages WHERE conversation_id = ?1",
        [7],
        |row| row.get(0),
    )?;
    let fts_before: i64 =
        engine
            .conn
            .query_row("SELECT COUNT(*) FROM messages_fts", [], |row| row.get(0))?;

    // Make the context_items step fail after the messages + FTS inserts have run,
    // proving the whole row set rolls back as one transaction.
    engine
        .conn
        .execute_batch("ALTER TABLE context_items RENAME TO context_items_hidden;")?;
    let result =
        engine.add_message_with_outcome(7, "assistant", "boom", Some(AgentOutcome::Success));
    assert!(
        result.is_err(),
        "add_message_with_outcome must fail when the context_items step cannot run"
    );
    engine
        .conn
        .execute_batch("ALTER TABLE context_items_hidden RENAME TO context_items;")?;

    let messages_after: i64 = engine.conn.query_row(
        "SELECT COUNT(*) FROM messages WHERE conversation_id = ?1",
        [7],
        |row| row.get(0),
    )?;
    let fts_after: i64 = engine
        .conn
        .query_row("SELECT COUNT(*) FROM messages_fts", [], |row| row.get(0))?;
    assert_eq!(
        messages_after, messages_before,
        "rolled-back transaction must leave no orphan messages row"
    );
    assert_eq!(
        fts_after, fts_before,
        "rolled-back transaction must leave no orphan messages_fts row"
    );

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn worker_attempt_success_persists_one_marker_and_typed_reply() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let _ = engine.continuity_init_documents(71)?;
    let input = WorkerAttemptFinalizationInput {
        attempt_id: "attempt-success-71",
        work_key: "queue:success-71",
        conversation_id: 71,
        source_label: "test-queue",
        agent_outcome: AgentOutcome::Success,
        reply_text: "done once",
        error_text: None,
    };
    engine.begin_worker_attempt_finalization(input.clone())?;
    let first = engine.ensure_worker_attempt_assistant_message(input.attempt_id)?;
    let second = engine.ensure_worker_attempt_assistant_message(input.attempt_id)?;
    assert_eq!(first.message_id, second.message_id);
    assert_eq!(first.agent_outcome.as_deref(), Some("Success"));

    let attempts: i64 = engine.conn.query_row(
        "SELECT COUNT(*) FROM worker_attempt_finalizations WHERE work_key = ?1",
        [input.work_key],
        |row| row.get(0),
    )?;
    let replies: i64 = engine.conn.query_row(
        "SELECT COUNT(*) FROM messages WHERE conversation_id = 71 AND role = 'assistant' AND agent_outcome = 'Success'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(attempts, 1);
    assert_eq!(replies, 1);
    Ok(())
}

#[test]
fn worker_attempt_finalizing_crash_resumes_without_duplicate_reply() -> Result<()> {
    let db_path = temp_db();
    {
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
        let _ = engine.continuity_init_documents(72)?;
        engine.begin_worker_attempt_finalization(WorkerAttemptFinalizationInput {
            attempt_id: "attempt-crash-72",
            work_key: "queue:crash-72",
            conversation_id: 72,
            source_label: "test-queue",
            agent_outcome: AgentOutcome::Success,
            reply_text: "recover me",
            error_text: None,
        })?;
        engine.ensure_worker_attempt_assistant_message("attempt-crash-72")?;
        // Simulated process loss: no artifact check and no terminal transition.
    }
    let resumed = LcmEngine::open(&db_path, LcmConfig::default())?;
    let attempt = resumed
        .recoverable_worker_attempt("queue:crash-72")?
        .expect("finalizing attempt must be resumable");
    assert_eq!(attempt.attempt_id, "attempt-crash-72");
    let original_message_id = attempt.reply_message_id.expect("durable reply id");
    let replay = resumed.ensure_worker_attempt_assistant_message(&attempt.attempt_id)?;
    assert_eq!(replay.message_id, original_message_id);
    let replies: i64 = resumed.conn.query_row(
        "SELECT COUNT(*) FROM messages WHERE conversation_id = 72 AND role = 'assistant'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(replies, 1);
    resumed.record_worker_attempt_artifact_check(
        &attempt.attempt_id,
        true,
        "no external artifacts required",
    )?;
    let terminal = resumed.terminalize_worker_attempt(
        &attempt.attempt_id,
        WorkerAttemptTerminalStatus::Succeeded,
        false,
        true,
        None,
    )?;
    assert_eq!(terminal.status, "succeeded");
    assert!(terminal.effects_completed);
    Ok(())
}

#[test]
fn context_token_count_propagates_db_errors() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let _ = engine.continuity_init_documents(7)?;

    // A real DB error in the token query must surface as Err, not collapse to
    // 0: a silent 0 would let pre-turn compaction skip a genuinely over-budget
    // context. Removing context_items makes the query fail deterministically.
    engine
        .conn
        .execute_batch("ALTER TABLE context_items RENAME TO context_items_hidden;")?;
    assert!(
        engine.context_token_count(7).is_err(),
        "a failing token-count query must return Err, not a silent 0"
    );

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn agent_outcome_token_round_trips() {
    for outcome in [
        AgentOutcome::Success,
        AgentOutcome::TurnTimeout,
        AgentOutcome::ExecutionError,
        AgentOutcome::ContextRejected,
        AgentOutcome::Aborted,
        AgentOutcome::Cancelled,
    ] {
        let token = outcome.as_str();
        assert_eq!(AgentOutcome::from_token(token), Some(outcome));
    }
    assert!(AgentOutcome::from_token("unknown").is_none());
}

#[test]
fn agent_outcome_failure_predicate_only_excludes_success() {
    assert!(!AgentOutcome::Success.is_agent_failure());
    assert!(AgentOutcome::TurnTimeout.is_agent_failure());
    assert!(AgentOutcome::ExecutionError.is_agent_failure());
    assert!(AgentOutcome::ContextRejected.is_agent_failure());
    assert!(AgentOutcome::Aborted.is_agent_failure());
    assert!(AgentOutcome::Cancelled.is_agent_failure());
}

/// P2 — clobber guard: a watchdog write that tries to clear
/// `done_gate` while the prior row carried a non-empty `done_gate`
/// must be silently downgraded to a no-op for that field, the prior
/// value preserved, and the attempt audited as a governance event.
/// (The reviewer-rework loop in production saw `next_slice` /
/// `done_gate` collapse to length 0 within ~25 minutes; this guard
/// is the structural fix.)
#[test]
fn mission_state_done_gate_clobber_is_blocked_and_audited() -> Result<()> {
    // Test root layout: runtime/ctox.sqlite3 is the shared DB used
    // by both LcmEngine and governance::record_event.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    let counter = TEMP_DB_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("ctox-clobber-guard-{nanos}-{counter}"));
    std::fs::create_dir_all(root.join("runtime"))?;
    let db_path = root.join("runtime/ctox.sqlite3");
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;

    // Drain anything other tests may have leaked onto this thread.
    let _ = drain_pending_mission_state_clobbers_for_test();

    // Seed an existing mission_states row with non-empty done_gate.
    let baseline = MissionStateRecord {
        conversation_id: 7,
        mission: "Founder mail covering vision and mission".to_string(),
        mission_status: "active".to_string(),
        continuation_mode: "continuous".to_string(),
        trigger_intensity: "hot".to_string(),
        blocker: "operator-set blocker".to_string(),
        next_slice: "wait for reviewer disposition before sending".to_string(),
        done_gate: "X".to_string(),
        closure_confidence: "low".to_string(),
        is_open: true,
        allow_idle: false,
        focus_head_commit_id: "focus-clobber".to_string(),
        last_synced_at: iso_now(),
        watcher_last_triggered_at: None,
        watcher_trigger_count: 0,
        agent_failure_count: 0,
        deferred_reason: None,
        rewrite_failure_count: 0,
    };
    engine.overwrite_mission_state(&baseline)?;
    let after_seed = engine
        .stored_mission_state(7)?
        .expect("seeded mission state should be visible");
    assert_eq!(after_seed.done_gate, "X");
    assert_eq!(
        after_seed.next_slice,
        "wait for reviewer disposition before sending"
    );
    // Drain the buffer caused by the seed write itself (none expected,
    // but keep the test deterministic).
    let _ = drain_pending_mission_state_clobbers_for_test();

    // Watchdog-shaped write: same row, but `done_gate` empty and
    // `next_slice` empty. The guard must preserve both prior
    // non-empty values.
    let watchdog_write = MissionStateRecord {
        done_gate: String::new(),
        next_slice: String::new(),
        mission: "Founder mail covering vision and mission updated".to_string(),
        ..baseline.clone()
    };
    engine.overwrite_mission_state(&watchdog_write)?;

    let after_watchdog = engine
        .stored_mission_state(7)?
        .expect("mission state still present after blocked clobber");
    assert_eq!(
        after_watchdog.done_gate, "X",
        "guard must preserve the prior non-empty done_gate"
    );
    assert_eq!(
        after_watchdog.next_slice, "wait for reviewer disposition before sending",
        "guard must preserve the prior non-empty next_slice"
    );
    // Other fields keep their existing semantics: `mission` was
    // overwritten exactly as the writer requested.
    assert_eq!(
        after_watchdog.mission, "Founder mail covering vision and mission updated",
        "guard must not interfere with non-protected fields"
    );

    // Flush the suppressed clobber attempts to governance and verify
    // the audit event landed.
    engine.drain_pending_mission_state_clobber_events_to_governance(&root);
    let events = crate::governance::list_recent_events(&root, 7, 16)
        .expect("failed to list governance events");
    let clobber_events: Vec<_> = events
        .iter()
        .filter(|event| event.mechanism_id == "mission_state_field_clobbered_blocked")
        .collect();
    assert_eq!(
        clobber_events.len(),
        2,
        "expected exactly two clobber-blocked events (next_slice, done_gate); got {clobber_events:?}",
    );
    let blocked_fields: std::collections::BTreeSet<String> = clobber_events
        .iter()
        .filter_map(|event| {
            event
                .details
                .get("field")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
        })
        .collect();
    assert!(blocked_fields.contains("done_gate"));
    assert!(blocked_fields.contains("next_slice"));
    for event in &clobber_events {
        assert_eq!(event.severity, "warning");
        assert_eq!(event.action_taken, "preserved_prior_non_empty_field");
    }

    // Replacement with NEW non-empty content must succeed (the
    // ratchet allows replace, only blocks silent clear).
    let replacement = MissionStateRecord {
        done_gate: "fresh non-empty done gate".to_string(),
        next_slice: "fresh non-empty next slice".to_string(),
        ..baseline.clone()
    };
    engine.overwrite_mission_state(&replacement)?;
    let after_replace = engine.stored_mission_state(7)?.unwrap();
    assert_eq!(after_replace.done_gate, "fresh non-empty done gate");
    assert_eq!(after_replace.next_slice, "fresh non-empty next slice");

    // Owner-intent clear bypasses the guard.
    let cleared = engine.clear_mission_state_done_fields_with_owner_intent(7, true, true)?;
    assert!(cleared.next_slice.is_empty());
    assert!(cleared.done_gate.is_empty());
    let after_owner_clear = engine.stored_mission_state(7)?.unwrap();
    assert!(after_owner_clear.next_slice.is_empty());
    assert!(after_owner_clear.done_gate.is_empty());

    let _ = std::fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn continuity_refresh_prompt_pins_the_conversation_id() {
    // Without --conversation-id the CLI defaults to conversation 1 and
    // hashed worker conversations silently lose every refresh.
    let prompt = build_continuity_prompt_text(
        42,
        ContinuityKind::Focus,
        "## Status\n- Mission state: open\n",
        &[],
        &[],
        &[],
        &[],
    );
    for mode_marker in ["--mode full", "--mode replace", "--mode diff"] {
        let line = prompt
            .lines()
            .find(|line| line.contains(mode_marker))
            .unwrap_or_else(|| panic!("prompt must show a {mode_marker} command"));
        assert!(
            line.contains("--conversation-id 42"),
            "{mode_marker} command must pin the conversation id, got: {line}"
        );
    }
}

#[test]
fn durable_execution_progress_reserves_ten_percent_for_native_review() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let work_key = "queue:progress-one-step";
    let running = [TaskExecutionPlanStepInput {
        label: "Bericht erstellen".to_string(),
        status: "in_progress".to_string(),
    }];
    let progress = engine.record_task_execution_plan(TaskExecutionPlanUpdate {
        work_key,
        task_id: "task-progress-one",
        command_id: "cmd-progress-one",
        attempt_id: "attempt-progress-one",
        explanation: Some("Ein überprüfbarer Arbeitsschritt"),
        steps: &running,
    })?;
    assert_eq!(progress["revision"], 1);
    assert_eq!(progress["percent"], 0);
    assert_eq!(progress["current_step"], 1);

    let activity = TaskExecutionActivityInput {
        activity_id: "attempt-progress-one:tool:1",
        work_key,
        task_id: "task-progress-one",
        command_id: "cmd-progress-one",
        attempt_id: "attempt-progress-one",
        kind: "tool",
        attribute_to_current_step: true,
    };
    assert!(engine.record_task_execution_activity(activity.clone())?);
    assert!(!engine.record_task_execution_activity(activity)?);
    let with_turn = engine.task_execution_progress(work_key)?.unwrap();
    assert_eq!(with_turn["percent"], 0);
    assert_eq!(with_turn["activity_turns"]["total"], 1);
    assert_eq!(with_turn["steps"][0]["activity_turns"], 1);

    let completed = [TaskExecutionPlanStepInput {
        label: "Bericht erstellen".to_string(),
        status: "completed".to_string(),
    }];
    let work_done = engine.record_task_execution_plan(TaskExecutionPlanUpdate {
        work_key,
        task_id: "task-progress-one",
        command_id: "cmd-progress-one",
        attempt_id: "attempt-progress-one",
        explanation: None,
        steps: &completed,
    })?;
    assert_eq!(work_done["revision"], 1);
    assert_eq!(work_done["percent"], 90);
    assert_eq!(work_done["phase"], "review");

    let reviewing = engine.prepare_task_execution_review(work_key)?;
    assert_eq!(reviewing["percent"], 90);
    assert_eq!(reviewing["review"]["status"], "in_progress");
    let validated = engine.set_task_execution_review_status(work_key, "completed")?;
    assert_eq!(validated["percent"], 100);
    assert_eq!(validated["phase"], "completed");

    drop(engine);
    let recovered = LcmEngine::open(&db_path, LcmConfig::default())?
        .task_execution_progress(work_key)?
        .unwrap();
    assert_eq!(recovered["percent"], 100);
    assert_eq!(recovered["activity_turns"]["total"], 1);
    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn execution_replanning_preserves_turn_total_and_recalculates_step_percent() -> Result<()> {
    let db_path = temp_db();
    let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
    let work_key = "queue:progress-replan";
    let initial = [
        TaskExecutionPlanStepInput {
            label: "Daten laden".to_string(),
            status: "completed".to_string(),
        },
        TaskExecutionPlanStepInput {
            label: "Daten prüfen".to_string(),
            status: "in_progress".to_string(),
        },
        TaskExecutionPlanStepInput {
            label: "Ergebnis schreiben".to_string(),
            status: "pending".to_string(),
        },
    ];
    let first = engine.record_task_execution_plan(TaskExecutionPlanUpdate {
        work_key,
        task_id: "task-progress-replan",
        command_id: "cmd-progress-replan",
        attempt_id: "attempt-progress-replan",
        explanation: None,
        steps: &initial,
    })?;
    assert_eq!(first["revision"], 1);
    assert_eq!(first["percent"], 30);

    engine.record_task_execution_activity(TaskExecutionActivityInput {
        activity_id: "attempt-progress-replan:thinking:1",
        work_key,
        task_id: "task-progress-replan",
        command_id: "cmd-progress-replan",
        attempt_id: "attempt-progress-replan",
        kind: "thinking",
        attribute_to_current_step: true,
    })?;
    let replanned = [
        initial[0].clone(),
        initial[1].clone(),
        TaskExecutionPlanStepInput {
            label: "Ausnahmen klären".to_string(),
            status: "pending".to_string(),
        },
        initial[2].clone(),
    ];
    let second = engine.record_task_execution_plan(TaskExecutionPlanUpdate {
        work_key,
        task_id: "task-progress-replan",
        command_id: "cmd-progress-replan",
        attempt_id: "attempt-progress-replan",
        explanation: Some("Eine Ausnahme erfordert einen zusätzlichen Schritt"),
        steps: &replanned,
    })?;
    assert_eq!(second["revision"], 2);
    assert_eq!(second["percent"], 23);
    assert_eq!(second["activity_turns"]["total"], 1);

    let revisions: i64 = engine.conn.query_row(
        "SELECT COUNT(*) FROM task_execution_plan_revisions WHERE work_key = ?1",
        [work_key],
        |row| row.get(0),
    )?;
    assert_eq!(revisions, 2);
    drop(engine);
    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[test]
fn closure_claim_count_fails_closed_when_claims_schema_is_missing() -> Result<()> {
    let bare = Connection::open_in_memory()?;
    let err = count_open_closure_blocking_claims(&bare, 1)
        .expect_err("missing claims schema must not open the closure gate");
    assert!(err.to_string().contains("mission_claims schema"));

    let incomplete = Connection::open_in_memory()?;
    incomplete.execute_batch(
        "CREATE TABLE mission_claims (conversation_id INTEGER, blocks_closure INTEGER);",
    )?;
    let err = count_open_closure_blocking_claims(&incomplete, 1)
        .expect_err("incomplete claims schema must not open the closure gate");
    assert!(err.to_string().contains("claim_status"));
    Ok(())
}
