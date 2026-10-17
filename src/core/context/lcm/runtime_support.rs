// Origin: CTOX
// License: Apache-2.0

use super::*;

pub(super) fn is_shared_memory_io_error(err: &anyhow::Error) -> bool {
    let text = err.to_string();
    text.contains("xShmMap")
        || text.contains("shared-memory")
        || (text.contains("disk I/O error") && text.contains("resize"))
}

/// Tracks LCM database paths whose schema has already been initialized in
/// this process. Used by `LcmEngine::open` so that the writer-locked
/// `init_schema()` batch only runs once per path per process — otherwise the
/// TUI's repeated refreshes serialize behind the daemon's writer.
pub(super) fn initialized_lcm_paths() -> &'static Mutex<HashSet<PathBuf>> {
    static INITIALIZED: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    INITIALIZED.get_or_init(|| Mutex::new(HashSet::new()))
}

pub fn run_init(db_path: &Path) -> Result<()> {
    owned_lcm_read(db_path, |conn| {
        conn.prepare("SELECT attempt_id FROM worker_attempt_finalizations LIMIT 0")?;
        conn.prepare("SELECT message_id FROM messages LIMIT 0")?;
        Ok(())
    })
}

pub fn run_add_message(
    db_path: &Path,
    conversation_id: i64,
    role: &str,
    content: &str,
) -> Result<MessageRecord> {
    let role = role.to_owned();
    let content = content.to_owned();
    owned_lcm_write(db_path, move |engine| {
        let role = role.as_str();
        let content = content.as_str();
        engine.add_message(conversation_id, role, content)
    })
}

/// F3: convenience wrapper for the agent harness — record an assistant
/// turn with its structured outcome in a single call.
pub fn run_add_assistant_turn(
    db_path: &Path,
    conversation_id: i64,
    content: &str,
    outcome: AgentOutcome,
) -> Result<MessageRecord> {
    let content = content.to_owned();
    owned_lcm_write(db_path, move |engine| {
        let content = content.as_str();
        engine.add_message_with_outcome(conversation_id, "assistant", content, Some(outcome))
    })
}

pub fn run_begin_worker_attempt_finalization(
    db_path: &Path,
    input: WorkerAttemptFinalizationInput<'_>,
) -> Result<WorkerAttemptRecord> {
    let (attempt_id, work_key, source_label, reply_text, error_text) = (
        input.attempt_id.to_owned(),
        input.work_key.to_owned(),
        input.source_label.to_owned(),
        input.reply_text.to_owned(),
        input.error_text.map(str::to_owned),
    );
    let (conversation_id, agent_outcome) = (input.conversation_id, input.agent_outcome);
    owned_lcm_write(db_path, move |engine| {
        engine.begin_worker_attempt_finalization(WorkerAttemptFinalizationInput {
            attempt_id: &attempt_id,
            work_key: &work_key,
            conversation_id,
            source_label: &source_label,
            agent_outcome,
            reply_text: &reply_text,
            error_text: error_text.as_deref(),
        })
    })
}

pub fn run_worker_attempt(db_path: &Path, attempt_id: &str) -> Result<Option<WorkerAttemptRecord>> {
    let attempt_id = attempt_id.to_owned();
    owned_lcm_read(db_path, move |conn| {
        snapshot_worker_attempt(conn, &attempt_id)
    })
}

pub fn run_recoverable_worker_attempt(
    db_path: &Path,
    work_key: &str,
) -> Result<Option<WorkerAttemptRecord>> {
    let work_key = work_key.to_owned();
    owned_lcm_read(db_path, move |conn| {
        snapshot_recoverable_worker_attempt(conn, &work_key)
    })
}

pub fn run_record_task_execution_plan(
    db_path: &Path,
    input: TaskExecutionPlanUpdate<'_>,
) -> Result<serde_json::Value> {
    let (work_key, task_id, command_id, attempt_id) = (
        input.work_key.to_owned(),
        input.task_id.to_owned(),
        input.command_id.to_owned(),
        input.attempt_id.to_owned(),
    );
    let explanation = input.explanation.map(str::to_owned);
    let steps = input.steps.to_vec();
    owned_lcm_write(db_path, move |engine| {
        let input = TaskExecutionPlanUpdate {
            work_key: &work_key,
            task_id: &task_id,
            command_id: &command_id,
            attempt_id: &attempt_id,
            explanation: explanation.as_deref(),
            steps: &steps,
        };
        engine.record_task_execution_plan(input)
    })
}

pub fn run_record_task_execution_activity(
    db_path: &Path,
    input: TaskExecutionActivityInput<'_>,
) -> Result<bool> {
    let (activity_id, work_key, task_id, command_id, attempt_id, kind) = (
        input.activity_id.to_owned(),
        input.work_key.to_owned(),
        input.task_id.to_owned(),
        input.command_id.to_owned(),
        input.attempt_id.to_owned(),
        input.kind.to_owned(),
    );
    let attribute_to_current_step = input.attribute_to_current_step;
    owned_lcm_write(db_path, move |engine| {
        let input = TaskExecutionActivityInput {
            activity_id: &activity_id,
            work_key: &work_key,
            task_id: &task_id,
            command_id: &command_id,
            attempt_id: &attempt_id,
            kind: &kind,
            attribute_to_current_step,
        };
        engine.record_task_execution_activity(input)
    })
}

pub fn run_prepare_task_execution_review(
    db_path: &Path,
    work_key: &str,
) -> Result<serde_json::Value> {
    let work_key = work_key.to_owned();
    owned_lcm_write(db_path, move |engine| {
        let work_key = work_key.as_str();
        engine.prepare_task_execution_review(work_key)
    })
}

pub fn run_set_task_execution_review_status(
    db_path: &Path,
    work_key: &str,
    review_status: &str,
) -> Result<serde_json::Value> {
    let work_key = work_key.to_owned();
    let review_status = review_status.to_owned();
    owned_lcm_write(db_path, move |engine| {
        let work_key = work_key.as_str();
        let review_status = review_status.as_str();
        engine.set_task_execution_review_status(work_key, review_status)
    })
}

pub fn run_task_execution_progress(
    db_path: &Path,
    work_key: &str,
) -> Result<Option<serde_json::Value>> {
    let work_key = work_key.to_owned();
    owned_lcm_read(db_path, move |conn| {
        snapshot_task_execution_progress(conn, &work_key)
    })
}

pub fn run_task_execution_progress_for_task(
    db_path: &Path,
    task_id: &str,
) -> Result<Option<serde_json::Value>> {
    let task_id = task_id.to_owned();
    owned_lcm_read(db_path, move |conn| {
        snapshot_task_execution_progress_for_task(conn, &task_id)
    })
}

pub fn run_ensure_worker_attempt_assistant_message(
    db_path: &Path,
    attempt_id: &str,
) -> Result<MessageRecord> {
    let attempt_id = attempt_id.to_owned();
    owned_lcm_write(db_path, move |engine| {
        let attempt_id = attempt_id.as_str();
        engine.ensure_worker_attempt_assistant_message(attempt_id)
    })
}

pub fn run_record_worker_attempt_artifact_check(
    db_path: &Path,
    attempt_id: &str,
    accepted: bool,
    details: &str,
) -> Result<WorkerAttemptRecord> {
    let attempt_id = attempt_id.to_owned();
    let details = details.to_owned();
    owned_lcm_write(db_path, move |engine| {
        let attempt_id = attempt_id.as_str();
        let details = details.as_str();
        engine.record_worker_attempt_artifact_check(attempt_id, accepted, details)
    })
}

pub fn run_terminalize_worker_attempt(
    db_path: &Path,
    attempt_id: &str,
    status: WorkerAttemptTerminalStatus,
    resumable: bool,
    effects_completed: bool,
    finalization_error: Option<&str>,
) -> Result<WorkerAttemptRecord> {
    let attempt_id = attempt_id.to_owned();
    let finalization_error = finalization_error.map(str::to_owned);
    let result = owned_lcm_write(db_path, move |engine| {
        let attempt_id = attempt_id.as_str();
        let finalization_error = finalization_error.as_deref();
        let result = engine.terminalize_worker_attempt(
            attempt_id,
            status,
            resumable,
            effects_completed,
            finalization_error,
        )?;
        Ok(result)
    })?;
    crate::business_os::harness_cockpit::refresh_after_finalization(db_path);
    Ok(result)
}

pub fn run_mark_worker_attempt_effects_completed(
    db_path: &Path,
    attempt_id: &str,
) -> Result<WorkerAttemptRecord> {
    let attempt_id = attempt_id.to_owned();
    let result = owned_lcm_write(db_path, move |engine| {
        let attempt_id = attempt_id.as_str();
        let result = engine.mark_worker_attempt_effects_completed(attempt_id)?;
        Ok(result)
    })?;
    crate::business_os::harness_cockpit::refresh_after_finalization(db_path);
    Ok(result)
}

pub fn run_mark_worker_attempt_recovery_effects_applied(
    db_path: &Path,
    attempt_id: &str,
) -> Result<bool> {
    let attempt_id = attempt_id.to_owned();
    owned_lcm_write(db_path, move |engine| {
        let attempt_id = attempt_id.as_str();
        engine.mark_worker_attempt_recovery_effects_applied(attempt_id)
    })
}

pub fn run_compact(
    db_path: &Path,
    conversation_id: i64,
    token_budget: i64,
    force: bool,
) -> Result<CompactionResult> {
    owned_lcm_compact(
        db_path,
        conversation_id,
        token_budget,
        LcmConfig::default(),
        &HeuristicSummarizer,
        force,
    )
}

pub fn run_grep(
    db_path: &Path,
    conversation_id: Option<i64>,
    scope: &str,
    mode: &str,
    query: &str,
    limit: usize,
) -> Result<GrepResult> {
    let scope = scope.to_owned();
    let mode = mode.to_owned();
    let query = query.to_owned();
    owned_lcm_session_read(db_path, move |engine| {
        let scope = scope.as_str();
        let mode = mode.as_str();
        let query = query.as_str();
        engine.grep(
            conversation_id,
            GrepScope::parse(scope)?,
            GrepMode::parse(mode)?,
            query,
            limit,
        )
    })
}

pub fn run_describe(db_path: &Path, id: &str) -> Result<Option<DescribeResult>> {
    let id = id.to_owned();
    owned_lcm_session_read(db_path, move |engine| {
        let id = id.as_str();
        engine.describe(id)
    })
}

pub fn run_expand(
    db_path: &Path,
    summary_id: &str,
    depth: usize,
    include_messages: bool,
    token_cap: i64,
) -> Result<ExpandResult> {
    let summary_id = summary_id.to_owned();
    owned_lcm_session_read(db_path, move |engine| {
        let summary_id = summary_id.as_str();
        engine.expand(summary_id, depth, include_messages, token_cap)
    })
}

pub fn run_dump(db_path: &Path, conversation_id: i64) -> Result<LcmSnapshot> {
    owned_lcm_snapshot(db_path, conversation_id)
}

pub fn run_secret_rewrite(
    db_path: &Path,
    conversation_id: i64,
    secret_scope: &str,
    secret_name: &str,
    match_text: &str,
    replacement_text: &str,
) -> Result<SecretRewriteResult> {
    let secret_scope = secret_scope.to_owned();
    let secret_name = secret_name.to_owned();
    let match_text = match_text.to_owned();
    let replacement_text = replacement_text.to_owned();
    owned_lcm_write(db_path, move |engine| {
        let secret_scope = secret_scope.as_str();
        let secret_name = secret_name.as_str();
        let match_text = match_text.as_str();
        let replacement_text = replacement_text.as_str();
        engine.rewrite_secret_literal(
            conversation_id,
            secret_scope,
            secret_name,
            match_text,
            replacement_text,
        )
    })
}

pub fn run_refresh_continuity(db_path: &Path, conversation_id: i64) -> Result<ContinuityRevision> {
    run_continuity_init(db_path, conversation_id)?;
    owned_lcm_session_read(db_path, move |engine| {
        engine
            .stored_latest_continuity(conversation_id)?
            .context("continuity documents missing after init")
    })
}

pub fn run_show_continuity(
    db_path: &Path,
    conversation_id: i64,
) -> Result<Option<ContinuityRevision>> {
    run_continuity_init(db_path, conversation_id)?;
    owned_lcm_session_read(db_path, move |engine| {
        engine.stored_latest_continuity(conversation_id)
    })
}

pub fn run_continuity_init(db_path: &Path, conversation_id: i64) -> Result<ContinuityShowAll> {
    owned_lcm_write(db_path, move |engine| {
        engine.continuity_init_documents(conversation_id)
    })
}

pub fn run_continuity_show(
    db_path: &Path,
    conversation_id: i64,
    kind: Option<&str>,
) -> Result<serde_json::Value> {
    let kind = kind.map(str::to_owned);
    owned_lcm_write(db_path, move |engine| {
        let kind = kind.as_deref();
        if let Some(kind) = kind {
            Ok(serde_json::to_value(engine.continuity_show(
                conversation_id,
                ContinuityKind::parse(kind)?,
            )?)?)
        } else {
            Ok(serde_json::to_value(
                engine.continuity_show_all(conversation_id)?,
            )?)
        }
    })
}

pub fn run_continuity_apply(
    db_path: &Path,
    conversation_id: i64,
    kind: &str,
    diff_path: &Path,
) -> Result<ContinuityDocumentState> {
    crate::persistence::require_sync_owner_context()?;
    let kind = kind.to_owned();
    let diff_text = std::fs::read_to_string(diff_path)
        .with_context(|| format!("failed to read continuity diff {}", diff_path.display()))?;
    owned_lcm_write(db_path, move |engine| {
        let kind = kind.as_str();
        engine.continuity_apply_diff(conversation_id, ContinuityKind::parse(kind)?, &diff_text)
    })
}

pub fn run_continuity_full_replace(
    db_path: &Path,
    conversation_id: i64,
    kind: &str,
    content: &str,
) -> Result<ContinuityDocumentState> {
    let kind = kind.to_owned();
    let content = content.to_owned();
    owned_lcm_write(db_path, move |engine| {
        let kind = kind.as_str();
        let content = content.as_str();
        engine.continuity_full_replace_document(
            conversation_id,
            ContinuityKind::parse(kind)?,
            content,
        )
    })
}

pub fn run_continuity_string_replace(
    db_path: &Path,
    conversation_id: i64,
    kind: &str,
    find: &str,
    replace: &str,
) -> Result<ContinuityDocumentState> {
    let kind = kind.to_owned();
    let find = find.to_owned();
    let replace = replace.to_owned();
    owned_lcm_write(db_path, move |engine| {
        let kind = kind.as_str();
        let find = find.as_str();
        let replace = replace.as_str();
        engine.continuity_string_replace_document(
            conversation_id,
            ContinuityKind::parse(kind)?,
            find,
            replace,
        )
    })
}

pub fn run_continuity_log(
    db_path: &Path,
    conversation_id: i64,
    kind: Option<&str>,
) -> Result<Vec<ContinuityCommitRecord>> {
    let parsed_kind = kind.map(ContinuityKind::parse).transpose()?;
    owned_lcm_write(db_path, move |engine| {
        if let Some(kind) = parsed_kind {
            engine.ensure_continuity_document(conversation_id, kind)?;
        } else {
            load_or_init_continuity_show_all(&engine.conn, conversation_id)?;
        }
        Ok(())
    })?;
    let kind = kind.map(str::to_owned);
    owned_lcm_session_read(db_path, move |engine| {
        let kind = kind.as_deref();
        engine.continuity_log(
            conversation_id,
            kind.map(ContinuityKind::parse).transpose()?,
        )
    })
}

pub fn run_continuity_rebuild(
    db_path: &Path,
    conversation_id: i64,
    kind: &str,
) -> Result<ContinuityDocumentState> {
    let kind = kind.to_owned();
    owned_lcm_session_read(db_path, move |engine| {
        let kind = kind.as_str();
        engine.continuity_rebuild(conversation_id, ContinuityKind::parse(kind)?)
    })
}

pub fn run_continuity_forgotten(
    db_path: &Path,
    conversation_id: i64,
    kind: Option<&str>,
    query: Option<&str>,
) -> Result<Vec<ContinuityForgottenEntry>> {
    let parsed_kind = kind.map(ContinuityKind::parse).transpose()?;
    owned_lcm_write(db_path, move |engine| {
        if let Some(kind) = parsed_kind {
            engine.ensure_continuity_document(conversation_id, kind)?;
        } else {
            load_or_init_continuity_show_all(&engine.conn, conversation_id)?;
        }
        Ok(())
    })?;
    let kind = kind.map(str::to_owned);
    let query = query.map(str::to_owned);
    owned_lcm_session_read(db_path, move |engine| {
        let kind = kind.as_deref();
        let query = query.as_deref();
        engine.continuity_forgotten(
            conversation_id,
            kind.map(ContinuityKind::parse).transpose()?,
            query,
        )
    })
}

pub fn run_continuity_build_prompt(
    db_path: &Path,
    conversation_id: i64,
    kind: &str,
) -> Result<ContinuityPromptPayload> {
    let kind = ContinuityKind::parse(kind)?;
    owned_lcm_write(db_path, move |engine| {
        engine.ensure_continuity_document(conversation_id, kind)
    })?;
    let source = owned_lcm_session_read(db_path, move |engine| {
        engine.continuity_prompt_source(conversation_id, kind)
    })
    .context("read continuity prompt sources")?;
    build_continuity_prompt_from_source(conversation_id, kind, source)
        .context("assemble continuity prompt after snapshot release")
}

pub fn run_context_retrieve(
    db_path: &Path,
    conversation_id: i64,
    mode: &str,
    query: Option<&str>,
    continuity_kind: Option<&str>,
    summary_id: Option<&str>,
    limit: usize,
    depth: usize,
    include_messages: bool,
    token_cap: i64,
) -> Result<serde_json::Value> {
    match mode {
        "current" => {
            run_continuity_init(db_path, conversation_id)?;
            owned_lcm_session_read(db_path, move |engine| {
                let snapshot = engine.snapshot(conversation_id)?;
                let continuity = engine.stored_continuity_show_all(conversation_id)?;
                Ok(serde_json::json!({"mode":"current","conversation_id":conversation_id,
                    "continuity":continuity,"context_items":snapshot.context_items,
                    "messages":snapshot.messages,"summaries":snapshot.summaries}))
            })
        }
        "continuity" => run_continuity_show(db_path, conversation_id, continuity_kind),
        "forgotten" => Ok(serde_json::to_value(run_continuity_forgotten(db_path,conversation_id,continuity_kind,query)?)?),
        "search" => Ok(serde_json::to_value(run_grep(db_path,Some(conversation_id),"both","full_text",query.context("context_retrieve mode=search requires query")?,limit)?)?),
        "describe" => Ok(serde_json::to_value(run_describe(db_path,summary_id.context("context_retrieve mode=describe requires summary_id")?)?)?),
        "expand" => Ok(serde_json::to_value(run_expand(db_path,summary_id.context("context_retrieve mode=expand requires summary_id")?,depth,include_messages,token_cap)?)?),
        other => anyhow::bail!("unsupported context_retrieve mode: {other}; expected one of current, continuity, forgotten, search, describe, expand"),
    }
}

pub fn run_fixture(db_path: &Path, fixture_path: &Path) -> Result<FixtureRunOutput> {
    crate::persistence::require_sync_owner_context()?;
    let fixture_bytes = std::fs::read(fixture_path)
        .with_context(|| format!("failed to read fixture {}", fixture_path.display()))?;
    let fixture: LcmFixture = serde_json::from_slice(&fixture_bytes)
        .with_context(|| format!("failed to parse fixture {}", fixture_path.display()))?;
    let config = merge_fixture_config(fixture.config);
    let conversation_id = fixture.conversation_id;
    owned_lcm_write(db_path, move |engine| {
        engine.continuity_init_documents(conversation_id)
    })?;
    for message in fixture.messages {
        owned_lcm_write(db_path, move |engine| {
            engine.add_message(conversation_id, &message.role, &message.content)
        })?;
    }
    let compaction = owned_lcm_compact(
        db_path,
        conversation_id,
        fixture.token_budget,
        config,
        &HeuristicSummarizer,
        fixture.force_compact.unwrap_or(false),
    )?;
    let snapshot = owned_lcm_snapshot(db_path, conversation_id)?;
    let grep_results = fixture
        .grep_queries
        .unwrap_or_default()
        .into_iter()
        .map(|query| {
            owned_lcm_session_read(db_path, move |engine| {
                engine.grep(
                    Some(conversation_id),
                    GrepScope::parse(&query.scope)?,
                    GrepMode::parse(&query.mode)?,
                    &query.query,
                    query.limit.unwrap_or(20),
                )
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let fallback_summary_id = compaction.created_summary_ids.first().cloned();
    let mut expand_results = Vec::new();
    for query in fixture.expand_queries.unwrap_or_default() {
        if let Some(summary_id) = query.summary_id.or_else(|| fallback_summary_id.clone()) {
            expand_results.push(owned_lcm_session_read(db_path, move |engine| {
                engine.expand(
                    &summary_id,
                    query.depth.unwrap_or(1),
                    query.include_messages.unwrap_or(false),
                    query.token_cap.unwrap_or(8_000),
                )
            })?);
        }
    }
    Ok(FixtureRunOutput {
        compaction,
        snapshot,
        grep_results,
        expand_results,
    })
}

fn merge_fixture_config(config: Option<LcmFixtureConfig>) -> LcmConfig {
    let mut merged = LcmConfig::default();
    if let Some(config) = config {
        if let Some(value) = config.context_threshold {
            merged.context_threshold = value;
        }
        if let Some(value) = config.min_compaction_tokens {
            merged.min_compaction_tokens = value;
        }
        if let Some(value) = config.fresh_tail_count {
            merged.fresh_tail_count = value;
        }
        if let Some(value) = config.leaf_chunk_tokens {
            merged.leaf_chunk_tokens = value;
        }
        if let Some(value) = config.leaf_target_tokens {
            merged.leaf_target_tokens = value;
        }
        if let Some(value) = config.condensed_target_tokens {
            merged.condensed_target_tokens = value;
        }
        if let Some(value) = config.leaf_min_fanout {
            merged.leaf_min_fanout = value;
        }
        if let Some(value) = config.condensed_min_fanout {
            merged.condensed_min_fanout = value;
        }
        if let Some(value) = config.max_rounds {
            merged.max_rounds = value;
        }
    }
    merged
}

pub(super) fn ensure_mission_state_storage_with(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mission_states (
            conversation_id INTEGER PRIMARY KEY,
            mission TEXT NOT NULL,
            mission_status TEXT NOT NULL,
            continuation_mode TEXT NOT NULL,
            trigger_intensity TEXT NOT NULL,
            blocker TEXT NOT NULL,
            next_slice TEXT NOT NULL,
            done_gate TEXT NOT NULL,
            closure_confidence TEXT NOT NULL,
            is_open INTEGER NOT NULL,
            allow_idle INTEGER NOT NULL,
            focus_head_commit_id TEXT NOT NULL,
            last_synced_at TEXT NOT NULL,
            watcher_last_triggered_at TEXT,
            watcher_trigger_count INTEGER NOT NULL DEFAULT 0,
            agent_failure_count INTEGER NOT NULL DEFAULT 0,
            deferred_reason TEXT,
            rewrite_failure_count INTEGER NOT NULL DEFAULT 0,
            structured_state_version INTEGER NOT NULL DEFAULT 1
        );",
    )?;
    Ok(())
}

/// Seed durable queue work without replacing model-authored mission text. The
/// caller owns the transaction; queue creation uses this on the same SQLite
/// transaction as the communication message and routing rows.
pub(crate) fn seed_mission_state_for_queue_with(
    conn: &Connection,
    conversation_id: i64,
    title: &str,
) -> Result<bool> {
    let title = title.trim();
    if title.is_empty() {
        anyhow::bail!("queue mission seed title must not be empty");
    }
    ensure_mission_state_storage_with(conn)?;
    let focus_head_commit_id = if table_exists_with(conn, "continuity_documents")? {
        conn.query_row(
            "SELECT head_commit_id FROM continuity_documents
             WHERE conversation_id = ?1 AND kind = 'focus'",
            params![conversation_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .unwrap_or_default()
    } else {
        String::new()
    };
    let now = iso_now();
    let changed = conn.execute(
        "INSERT INTO mission_states (
            conversation_id, mission, mission_status, continuation_mode,
            trigger_intensity, blocker, next_slice, done_gate,
            closure_confidence, is_open, allow_idle, focus_head_commit_id,
            last_synced_at, watcher_last_triggered_at, watcher_trigger_count,
            agent_failure_count, deferred_reason, rewrite_failure_count,
            structured_state_version
         ) VALUES (
            ?1, ?2, 'active', 'continuous', 'hot', '', ?2, '', 'low', 1, 0,
            ?3, ?4, NULL, 0, 0, NULL, 0, 1
         )
         ON CONFLICT(conversation_id) DO UPDATE SET
            mission = excluded.mission,
            mission_status = 'active',
            continuation_mode = 'continuous',
            trigger_intensity = 'hot',
            next_slice = excluded.next_slice,
            closure_confidence = 'low',
            is_open = 1,
            allow_idle = 0,
            last_synced_at = excluded.last_synced_at,
            deferred_reason = NULL
         WHERE trim(mission_states.mission) = ''
           AND trim(mission_states.next_slice) = ''
           AND trim(mission_states.done_gate) = ''",
        params![conversation_id, title, focus_head_commit_id, now],
    )?;
    Ok(changed > 0)
}

fn table_exists_with(conn: &Connection, table: &str) -> Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        params![table],
        |row| row.get::<_, bool>(0),
    )
    .map_err(Into::into)
}

pub(super) fn migrate_empty_mission_split_brain_with(conn: &Connection) -> Result<()> {
    ensure_mission_state_storage_with(conn)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS lcm_data_migrations (
            migration_id TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL,
            details_json TEXT NOT NULL DEFAULT '{}'
        );",
    )?;
    let tx =
        LcmTransaction::begin(conn).context("failed to begin empty mission-state migration")?;
    let already_applied = tx.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM lcm_data_migrations WHERE migration_id = ?1
         )",
        params![EMPTY_MISSION_SPLIT_BRAIN_MIGRATION],
        |row| row.get::<_, bool>(0),
    )?;
    if already_applied {
        tx.commit()?;
        return Ok(());
    }

    let candidate_ids = {
        let mut statement = tx.prepare(
            "SELECT conversation_id
             FROM mission_states
             WHERE trim(mission) = ''
               AND trim(next_slice) = ''
               AND trim(done_gate) = ''
               AND is_open = 0
               AND lower(trim(mission_status)) = 'active'
               AND lower(trim(continuation_mode)) = 'continuous'",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, i64>(0))?;
        rows.collect::<rusqlite::Result<HashSet<_>>>()?
    };

    let mut queue_titles = HashMap::new();
    if !candidate_ids.is_empty()
        && table_exists_with(&tx, "communication_messages")?
        && table_exists_with(&tx, "communication_routing_state")?
    {
        let mut statement = tx.prepare(
            "SELECT message.thread_key, message.subject
             FROM communication_messages message
             LEFT JOIN communication_routing_state route
               ON route.message_key = message.message_key
             WHERE message.channel = 'queue'
               AND message.direction = 'inbound'
               AND lower(COALESCE(route.route_status, 'pending'))
                   IN ('pending', 'leased', 'blocked')
             ORDER BY message.external_created_at ASC, message.observed_at ASC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (thread_key, title) = row?;
            let conversation_id =
                crate::execution::agent::turn_loop::conversation_id_for_thread_key(Some(
                    thread_key.as_str(),
                ));
            if candidate_ids.contains(&conversation_id) && !title.trim().is_empty() {
                queue_titles.entry(conversation_id).or_insert(title);
            }
        }
    }

    let now = iso_now();
    let migrated_to_empty = tx.execute(
        "UPDATE mission_states
         SET mission_status = 'dormant',
             continuation_mode = 'dormant',
             trigger_intensity = 'archive',
             closure_confidence = 'low',
             is_open = 0,
             allow_idle = 1,
             last_synced_at = ?1
         WHERE trim(mission) = ''
           AND trim(next_slice) = ''
           AND trim(done_gate) = ''
           AND is_open = 0
           AND lower(trim(mission_status)) = 'active'
           AND lower(trim(continuation_mode)) = 'continuous'",
        params![now],
    )?;
    let mut queue_seeded = 0usize;
    for (conversation_id, title) in queue_titles {
        if seed_mission_state_for_queue_with(&tx, conversation_id, &title)? {
            queue_seeded = queue_seeded.saturating_add(1);
        }
    }
    tx.execute(
        "INSERT INTO lcm_data_migrations (migration_id, applied_at, details_json)
         VALUES (?1, ?2, ?3)",
        params![
            EMPTY_MISSION_SPLIT_BRAIN_MIGRATION,
            iso_now(),
            serde_json::to_string(&serde_json::json!({
                "candidate_count": candidate_ids.len(),
                "queue_seeded_count": queue_seeded,
                "empty_state_count": migrated_to_empty.saturating_sub(queue_seeded),
            }))?,
        ],
    )?;
    tx.commit()
        .context("failed to commit empty mission-state migration")?;
    Ok(())
}

impl<C: std::ops::Deref<Target = Connection>> LcmEngine<C> {
    pub(super) fn persist_mission_state(&self, record: &MissionStateRecord) -> Result<()> {
        let tx = LcmTransaction::begin(&self.conn)
            .context("failed to begin structured mission-state write")?;
        let mut continuity = load_or_init_continuity_show_all(&tx, record.conversation_id)?;
        persist_mission_state_with(&tx, record)?;
        let effective = load_mission_state_with(&tx, record.conversation_id)?
            .context("mission state missing after structured write")?;
        let _ = render_focus_continuity_with(
            &tx,
            &mut continuity,
            &effective,
            "Rendered focus continuity after a structured mission-state write.",
        )?;
        tx.commit()
            .context("failed to commit structured mission-state write")?;
        Ok(())
    }
}

pub(super) fn load_or_import_mission_state_with(
    conn: &Connection,
    continuity: &ContinuityShowAll,
) -> Result<(MissionStateRecord, bool)> {
    if let Some(record) = load_mission_state_with(conn, continuity.conversation_id)? {
        return Ok((record, false));
    }
    let record = import_legacy_mission_state(continuity);
    persist_mission_state_with(conn, &record)?;
    Ok((record, true))
}

/// Render the focus document from authoritative structured state and persist
/// the resulting focus head back onto that same state row.
pub(super) fn render_focus_continuity_with(
    conn: &Connection,
    continuity: &mut ContinuityShowAll,
    record: &MissionStateRecord,
    reason: &str,
) -> Result<(MissionStateRecord, bool)> {
    let rendered_content = render_focus_continuity_from_record(continuity, record);
    let changed = rendered_content.trim() != continuity.focus.content.trim();
    let synced_at = iso_now();
    if changed {
        let diff_text = format!("## Status\n+ {reason}\n");
        let commit_identity = format!("{diff_text}\n<parent:{}>", continuity.focus.head_commit_id);
        let commit_id = continuity_commit_id(
            continuity.conversation_id,
            ContinuityKind::Focus,
            &commit_identity,
            &rendered_content,
            &synced_at,
        );
        let document_id = continuity_document_id(continuity.conversation_id, ContinuityKind::Focus);
        conn.execute(
            "INSERT INTO continuity_commits (commit_id, document_id, parent_commit_id, diff_text, rendered_text, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                commit_id,
                document_id,
                continuity.focus.head_commit_id,
                diff_text,
                rendered_content,
                synced_at,
            ],
        )?;
        conn.execute(
            "UPDATE continuity_documents SET head_commit_id = ?1, updated_at = ?2 WHERE document_id = ?3",
            params![commit_id, synced_at, document_id],
        )?;
        continuity.focus = fetch_continuity_document_with(
            conn,
            continuity.conversation_id,
            ContinuityKind::Focus,
        )?
        .context("focus continuity missing after structured-state render")?;
    }

    let mut synced_record = record.clone();
    synced_record.focus_head_commit_id = continuity.focus.head_commit_id.clone();
    synced_record.last_synced_at = synced_at;
    persist_mission_state_with(conn, &synced_record)?;
    let stored = load_mission_state_with(conn, continuity.conversation_id)?
        .context("mission state missing after structured-state render")?;
    Ok((stored, changed))
}

pub(super) fn load_or_init_continuity_show_all(
    conn: &Connection,
    conversation_id: i64,
) -> Result<ContinuityShowAll> {
    let narrative =
        ensure_continuity_document_with(conn, conversation_id, ContinuityKind::Narrative)?;
    let anchors = ensure_continuity_document_with(conn, conversation_id, ContinuityKind::Anchors)?;
    let focus = ensure_continuity_document_with(conn, conversation_id, ContinuityKind::Focus)?;
    Ok(ContinuityShowAll {
        conversation_id,
        narrative,
        anchors,
        focus,
    })
}

pub(super) fn load_continuity_show_all_with(
    conn: &Connection,
    conversation_id: i64,
) -> Result<ContinuityShowAll> {
    let narrative =
        fetch_continuity_document_with(conn, conversation_id, ContinuityKind::Narrative)?
            .context("missing stored narrative continuity document")?;
    let anchors = fetch_continuity_document_with(conn, conversation_id, ContinuityKind::Anchors)?
        .context("missing stored anchors continuity document")?;
    let focus = fetch_continuity_document_with(conn, conversation_id, ContinuityKind::Focus)?
        .context("missing stored focus continuity document")?;
    Ok(ContinuityShowAll {
        conversation_id,
        narrative,
        anchors,
        focus,
    })
}

pub(super) fn ensure_continuity_document_with(
    conn: &Connection,
    conversation_id: i64,
    kind: ContinuityKind,
) -> Result<ContinuityDocumentState> {
    if let Some(state) = fetch_continuity_document_with(conn, conversation_id, kind)? {
        return Ok(state);
    }

    let document_id = continuity_document_id(conversation_id, kind);
    let created_at = iso_now();
    let template = continuity_template(kind).to_string();
    let base_commit_id = continuity_base_commit_id(conversation_id, kind);
    conn.execute(
        "INSERT INTO continuity_commits (commit_id, document_id, parent_commit_id, diff_text, rendered_text, created_at)
         VALUES (?1, ?2, NULL, ?3, ?4, ?5)",
        params![base_commit_id, document_id, "", template, created_at],
    )?;
    conn.execute(
        "INSERT INTO continuity_documents (document_id, conversation_id, kind, head_commit_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![document_id, conversation_id, kind.as_str(), base_commit_id, created_at],
    )?;

    fetch_continuity_document_with(conn, conversation_id, kind)?
        .context("continuity document missing after init")
}

fn fetch_continuity_document_with(
    conn: &Connection,
    conversation_id: i64,
    kind: ContinuityKind,
) -> Result<Option<ContinuityDocumentState>> {
    conn.query_row(
        "SELECT d.head_commit_id, c.rendered_text, d.created_at, d.updated_at
         FROM continuity_documents d
         JOIN continuity_commits c ON c.commit_id = d.head_commit_id
         WHERE d.conversation_id = ?1 AND d.kind = ?2",
        params![conversation_id, kind.as_str()],
        |row| {
            Ok(ContinuityDocumentState {
                conversation_id,
                kind,
                head_commit_id: row.get(0)?,
                content: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

pub(super) fn rewrite_message_rows_with(
    conn: &Connection,
    conversation_id: i64,
    match_text: &str,
    replacement_text: &str,
) -> Result<usize> {
    let rows: Vec<(i64, String)> = {
        let mut stmt = conn.prepare(
            "SELECT message_id, content FROM messages
             WHERE conversation_id = ?1 AND instr(content, ?2) > 0",
        )?;
        let mapped = stmt.query_map(params![conversation_id, match_text], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (message_id, content) in &rows {
        let replaced = content.replace(match_text, replacement_text);
        conn.execute(
            "UPDATE messages SET content = ?1, token_count = ?2 WHERE message_id = ?3",
            params![replaced, estimate_tokens(&replaced) as i64, message_id],
        )?;
        conn.execute(
            "INSERT INTO messages_fts(messages_fts, rowid, content) VALUES('delete', ?1, ?2)",
            params![message_id, normalize_for_fts(content)],
        )?;
        conn.execute(
            "INSERT INTO messages_fts (rowid, content) VALUES (?1, ?2)",
            params![message_id, normalize_for_fts(&replaced)],
        )?;
    }
    Ok(rows.len())
}

pub(super) fn rewrite_summary_rows_with(
    conn: &Connection,
    conversation_id: i64,
    match_text: &str,
    replacement_text: &str,
) -> Result<usize> {
    let rows: Vec<(String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT summary_id, content FROM summaries
             WHERE conversation_id = ?1 AND instr(content, ?2) > 0",
        )?;
        let mapped = stmt.query_map(params![conversation_id, match_text], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (summary_id, content) in &rows {
        let replaced = content.replace(match_text, replacement_text);
        conn.execute(
            "UPDATE summaries SET content = ?1, token_count = ?2 WHERE summary_id = ?3",
            params![replaced, estimate_tokens(&replaced) as i64, summary_id],
        )?;
        conn.execute(
            "INSERT INTO summaries_fts(summaries_fts, rowid, summary_id, content)
             VALUES('delete', (SELECT rowid FROM summaries WHERE summary_id = ?1), ?1, ?2)",
            params![summary_id, normalize_for_fts(content)],
        )?;
        conn.execute(
            "INSERT INTO summaries_fts (rowid, summary_id, content)
             VALUES ((SELECT rowid FROM summaries WHERE summary_id = ?1), ?1, ?2)",
            params![summary_id, normalize_for_fts(&replaced)],
        )?;
    }
    Ok(rows.len())
}

pub(super) fn rewrite_continuity_commit_rows_with(
    conn: &Connection,
    conversation_id: i64,
    match_text: &str,
    replacement_text: &str,
) -> Result<usize> {
    let rows: Vec<(String, String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT c.commit_id, c.diff_text, c.rendered_text
             FROM continuity_commits c
             JOIN continuity_documents d ON d.document_id = c.document_id
             WHERE d.conversation_id = ?1
               AND (instr(c.diff_text, ?2) > 0 OR instr(c.rendered_text, ?2) > 0)",
        )?;
        let mapped = stmt.query_map(params![conversation_id, match_text], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (commit_id, diff_text, rendered_text) in &rows {
        conn.execute(
            "UPDATE continuity_commits SET diff_text = ?1, rendered_text = ?2 WHERE commit_id = ?3",
            params![
                diff_text.replace(match_text, replacement_text),
                rendered_text.replace(match_text, replacement_text),
                commit_id
            ],
        )?;
    }
    if !rows.is_empty() {
        conn.execute(
            "UPDATE continuity_documents SET updated_at = ?1 WHERE conversation_id = ?2",
            params![iso_now(), conversation_id],
        )?;
    }
    Ok(rows.len())
}

pub(super) fn rewrite_continuity_revision_rows_with(
    conn: &Connection,
    conversation_id: i64,
    match_text: &str,
    replacement_text: &str,
) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE continuity_revisions
         SET narrative = replace(narrative, ?2, ?3),
             anchors = replace(anchors, ?2, ?3),
             focus = replace(focus, ?2, ?3)
         WHERE conversation_id = ?1
           AND (instr(narrative, ?2) > 0 OR instr(anchors, ?2) > 0 OR instr(focus, ?2) > 0)",
        params![conversation_id, match_text, replacement_text],
    )?)
}

pub(super) fn rewrite_mission_state_rows_with(
    conn: &Connection,
    conversation_id: i64,
    match_text: &str,
    replacement_text: &str,
) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE mission_states
         SET mission = replace(mission, ?2, ?3),
             blocker = replace(blocker, ?2, ?3),
             next_slice = replace(next_slice, ?2, ?3),
             done_gate = replace(done_gate, ?2, ?3)
         WHERE conversation_id = ?1
           AND (instr(mission, ?2) > 0 OR instr(blocker, ?2) > 0 OR instr(next_slice, ?2) > 0 OR instr(done_gate, ?2) > 0)",
        params![conversation_id, match_text, replacement_text],
    )?)
}

pub(super) fn rewrite_verification_rows_with(
    conn: &Connection,
    conversation_id: i64,
    match_text: &str,
    replacement_text: &str,
) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE verification_runs
         SET goal = replace(goal, ?2, ?3),
             preview = replace(preview, ?2, ?3),
             result_excerpt = replace(result_excerpt, ?2, ?3),
             blocker = replace(COALESCE(blocker, ''), ?2, ?3),
             review_summary = replace(review_summary, ?2, ?3),
             report_excerpt = replace(report_excerpt, ?2, ?3)
         WHERE conversation_id = ?1
           AND (
             instr(goal, ?2) > 0 OR instr(preview, ?2) > 0 OR instr(result_excerpt, ?2) > 0 OR
             instr(COALESCE(blocker, ''), ?2) > 0 OR instr(review_summary, ?2) > 0 OR instr(report_excerpt, ?2) > 0
           )",
        params![conversation_id, match_text, replacement_text],
    )?)
}

pub(super) fn rewrite_claim_rows_with(
    conn: &Connection,
    conversation_id: i64,
    match_text: &str,
    replacement_text: &str,
) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE mission_claims
         SET subject = replace(subject, ?2, ?3),
             summary = replace(summary, ?2, ?3),
             evidence_summary = replace(evidence_summary, ?2, ?3)
         WHERE conversation_id = ?1
           AND (instr(subject, ?2) > 0 OR instr(summary, ?2) > 0 OR instr(evidence_summary, ?2) > 0)",
        params![conversation_id, match_text, replacement_text],
    )?)
}

pub(super) fn continuity_template(kind: ContinuityKind) -> &'static str {
    match kind {
        ContinuityKind::Narrative => {
            "# CONTINUITY NARRATIVE\n\n## Situation\nsummary:\nstate:\n\n## Entries\nentry_id:\nevent_type:\nsummary:\nconsequence:\nsource_class:\nsource_ref:\nobserved_at:\n"
        }
        ContinuityKind::Anchors => {
            "# CONTINUITY ANCHORS\n\n## Entries\nanchor_id:\nanchor_type:\nstatement:\nsource_class:\nsource_ref:\nobserved_at:\nconfidence:\nsupersedes:\nexpires_at:\n"
        }
        ContinuityKind::Focus => {
            "# ACTIVE FOCUS\n\n## Status\nMission:\nMission state:\nContinuation mode:\nTrigger intensity:\n\n## Blocker\nCurrent blocker:\n\n## Next\nNext slice:\n\n## Done / Gate\nDone gate:\nRetry condition:\nClosure confidence:\n\n## Contract\nmission:\nmission_state:\ncontinuation_mode:\ntrigger_intensity:\nslice:\nslice_state:\n\n## State\ngoal:\nblocker:\nmissing_dependency:\nnext_slice:\ndone_gate:\nretry_condition:\nclosure_confidence:\n\n## Sources\nsource_refs:\nnone\nupdated_at:\n"
        }
    }
}

pub(super) fn continuity_document_id(conversation_id: i64, kind: ContinuityKind) -> String {
    format!("contdoc_{}_{}", conversation_id, kind.as_str())
}

pub(super) fn strategic_directive_id(
    conversation_id: i64,
    thread_key: Option<&str>,
    directive_kind: &str,
    revision: i64,
    created_at: &str,
) -> String {
    let mut hash = Sha256::new();
    hash.update(conversation_id.to_string().as_bytes());
    hash.update(thread_key.unwrap_or_default().as_bytes());
    hash.update(directive_kind.as_bytes());
    hash.update(revision.to_string().as_bytes());
    hash.update(created_at.as_bytes());
    let digest = hash.finalize();
    let prefix = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sdir_{prefix}")
}

/// A handle to the injected Core owner. It holds no SQLite connection and
/// never keeps a transaction across a model/tool call. Each method is one
/// admitted owner job (or the existing snapshot/compute/write compaction).
#[derive(Clone)]
pub(crate) struct LcmClient {
    path: PathBuf,
}
impl LcmClient {
    pub(crate) fn attach(path: &Path) -> Result<Self> {
        crate::persistence::require_sync_owner_context()?;
        owner_adapter::OwnerAdapter::for_path(path)?;
        run_init(path)?;
        Ok(Self {
            path: path.to_owned(),
        })
    }
    pub(crate) fn continuity_init_documents(&self, id: i64) -> Result<ContinuityShowAll> {
        run_continuity_init(&self.path, id)
    }
    pub(crate) fn continuity_show_all(&self, id: i64) -> Result<ContinuityShowAll> {
        self.continuity_init_documents(id)?;
        owned_lcm_session_read(&self.path, move |engine| {
            engine.stored_continuity_show_all(id)
        })
    }
    pub(crate) fn stored_continuity_show_all(&self, id: i64) -> Result<ContinuityShowAll> {
        owned_lcm_session_read(&self.path, move |engine| {
            engine.stored_continuity_show_all(id)
        })
    }
    pub(crate) fn continuity_show(
        &self,
        id: i64,
        kind: ContinuityKind,
    ) -> Result<ContinuityDocumentState> {
        owned_lcm_write(&self.path, move |engine| {
            engine.ensure_continuity_document(id, kind)
        })?;
        owned_lcm_session_read(&self.path, move |engine| engine.continuity_show(id, kind))
    }
    pub(crate) fn working_set_snapshot(&self, id: i64, limit: usize) -> Result<LcmSnapshot> {
        owned_lcm_session_read(&self.path, move |engine| {
            engine.working_set_snapshot(id, limit)
        })
    }
    pub(crate) fn snapshot(&self, id: i64) -> Result<LcmSnapshot> {
        owned_lcm_session_read(&self.path, move |engine| engine.snapshot(id))
    }
    pub(crate) fn peek_mission_state(&self, id: i64) -> Result<MissionStateRecord> {
        owned_lcm_session_read(&self.path, move |engine| engine.peek_mission_state(id))
    }
    pub(crate) fn continuity_forgotten(
        &self,
        id: i64,
        kind: Option<ContinuityKind>,
        query: Option<&str>,
    ) -> Result<Vec<ContinuityForgottenEntry>> {
        let query = query.map(str::to_owned);
        owned_lcm_session_read(&self.path, move |engine| {
            engine.continuity_forgotten(id, kind, query.as_deref())
        })
    }
    pub(crate) fn evaluate_compaction(&self, id: i64, budget: i64) -> Result<CompactionDecision> {
        owned_lcm_session_read(&self.path, move |engine| {
            engine.evaluate_compaction(id, budget)
        })
    }
    pub(crate) fn continuity_full_replace_document(
        &self,
        id: i64,
        kind: ContinuityKind,
        content: &str,
    ) -> Result<ContinuityDocumentState> {
        run_continuity_full_replace(&self.path, id, kind.as_str(), content)
    }
    pub(crate) fn continuity_string_replace_document(
        &self,
        id: i64,
        kind: ContinuityKind,
        find: &str,
        replace: &str,
    ) -> Result<ContinuityDocumentState> {
        run_continuity_string_replace(&self.path, id, kind.as_str(), find, replace)
    }
    pub(crate) fn record_task_execution_plan(
        &self,
        input: TaskExecutionPlanUpdate<'_>,
    ) -> Result<serde_json::Value> {
        run_record_task_execution_plan(&self.path, input)
    }
    pub(crate) fn record_task_execution_plan_guarded(
        &self,
        input: TaskExecutionPlanUpdate<'_>,
        authorize: impl FnOnce(&Connection) -> Result<()> + Send + 'static,
    ) -> Result<serde_json::Value> {
        let (work_key, task_id, command_id, attempt_id) = (
            input.work_key.to_owned(),
            input.task_id.to_owned(),
            input.command_id.to_owned(),
            input.attempt_id.to_owned(),
        );
        let explanation = input.explanation.map(str::to_owned);
        let steps = input.steps.to_vec();
        owned_lcm_write(&self.path, move |engine| {
            engine.record_task_execution_plan_guarded(
                TaskExecutionPlanUpdate {
                    work_key: &work_key,
                    task_id: &task_id,
                    command_id: &command_id,
                    attempt_id: &attempt_id,
                    explanation: explanation.as_deref(),
                    steps: &steps,
                },
                authorize,
            )
        })
    }
    pub(crate) fn mission_state(&self, id: i64) -> Result<MissionStateRecord> {
        owned_lcm_write(&self.path, move |engine| engine.mission_state(id))
    }
    pub(crate) fn stored_mission_state(&self, id: i64) -> Result<Option<MissionStateRecord>> {
        owned_lcm_session_read(&self.path, move |engine| engine.stored_mission_state(id))
    }
    pub(crate) fn mission_assurance_snapshot(&self, id: i64) -> Result<MissionAssuranceSnapshot> {
        owned_lcm_session_read(&self.path, move |engine| {
            engine.mission_assurance_snapshot(id)
        })
    }
    pub(crate) fn active_strategy_snapshot(
        &self,
        id: i64,
        thread: Option<&str>,
    ) -> Result<StrategySnapshot> {
        let thread = thread.map(str::to_owned);
        owned_lcm_session_read(&self.path, move |engine| {
            engine.active_strategy_snapshot(id, thread.as_deref())
        })
    }
    pub(crate) fn continuity_forgotten_recent(
        &self,
        id: i64,
        kind: Option<ContinuityKind>,
        limit: usize,
    ) -> Result<Vec<ContinuityForgottenEntry>> {
        owned_lcm_session_read(&self.path, move |engine| {
            engine.continuity_forgotten_recent(id, kind, limit)
        })
    }
    pub(crate) fn continuity_build_prompt(
        &self,
        id: i64,
        kind: ContinuityKind,
    ) -> Result<ContinuityPromptPayload> {
        run_continuity_build_prompt(&self.path, id, kind.as_str())
    }
    pub(crate) fn continuity_apply_diff(
        &self,
        id: i64,
        kind: ContinuityKind,
        diff: &str,
    ) -> Result<ContinuityDocumentState> {
        let diff = diff.to_owned();
        owned_lcm_write(&self.path, move |engine| {
            engine.continuity_apply_diff(id, kind, &diff)
        })
    }
    pub(crate) fn continuity_preserve_recent_anchor_literals(
        &self,
        id: i64,
    ) -> Result<Option<ContinuityDocumentState>> {
        // Prepare the mechanical diff outside both SQLite executors. Recheck
        // the source under the writer before applying so deleted literals or
        // a newer anchor head are never overwritten by stale preparation.
        owned_lcm_write(&self.path, move |engine| {
            engine.ensure_continuity_document(id, ContinuityKind::Anchors)
        })?;
        let source = owned_lcm_session_read(&self.path, move |engine| {
            Ok((
                engine.continuity_show(id, ContinuityKind::Anchors)?,
                engine.working_set_snapshot(id, 512)?,
                engine.continuity_forgotten_recent(id, Some(ContinuityKind::Anchors), 256)?,
            ))
        })?;
        let (document, snapshot, forgotten) = source;
        let mut literals = collect_explicit_anchor_literals(&snapshot.messages);
        literals.retain(|literal| {
            !forgotten
                .iter()
                .any(|entry| entry.line.contains(&literal.literal))
        });
        let Some(diff) = build_anchor_literal_preservation_diff(&document.content, &literals)
        else {
            return Ok(None);
        };
        let before = serde_json::to_string(&(document, snapshot, forgotten))?;
        owned_lcm_write(&self.path, move |engine| {
            let current = (
                engine.continuity_show(id, ContinuityKind::Anchors)?,
                engine.working_set_snapshot(id, 512)?,
                engine.continuity_forgotten_recent(id, Some(ContinuityKind::Anchors), 256)?,
            );
            if serde_json::to_string(&current)? != before {
                return Err(ctox_store::StoreError::Busy.into());
            }
            engine
                .continuity_apply_diff(id, ContinuityKind::Anchors, &diff)
                .map(Some)
        })
    }
    pub(crate) fn compact<S: Summarizer>(
        &self,
        id: i64,
        budget: i64,
        summarizer: &S,
        force: bool,
    ) -> Result<CompactionResult> {
        owned_lcm_compact(
            &self.path,
            id,
            budget,
            LcmConfig::default(),
            summarizer,
            force,
        )
    }
    pub(crate) fn sync_mission_state_from_continuity_with_repair(
        &self,
        id: i64,
    ) -> Result<MissionStateRepairOutcome> {
        owned_lcm_write(&self.path, move |engine| {
            engine.sync_mission_state_from_continuity_with_repair(id)
        })
    }
    pub(crate) fn sync_mission_state_from_continuity(&self, id: i64) -> Result<MissionStateRecord> {
        self.mission_state(id)
    }
    pub(crate) fn increment_mission_agent_failure_count(
        &self,
        id: i64,
    ) -> Result<MissionStateRecord> {
        owned_lcm_write(&self.path, move |engine| {
            engine.increment_mission_agent_failure_count(id)
        })
    }
    pub(crate) fn reset_mission_agent_failure_count(&self, id: i64) -> Result<MissionFailureReset> {
        owned_lcm_write(&self.path, move |engine| {
            engine.reset_mission_agent_failure_count(id)
        })
    }
    pub(crate) fn defer_mission_for_reason(
        &self,
        id: i64,
        reason: &str,
    ) -> Result<MissionStateRecord> {
        let reason = reason.to_owned();
        owned_lcm_write(&self.path, move |engine| {
            engine.defer_mission_for_reason(id, &reason)
        })
    }
    pub(crate) fn overwrite_mission_state(&self, record: &MissionStateRecord) -> Result<()> {
        let record = record.clone();
        owned_lcm_write(&self.path, move |engine| {
            engine.overwrite_mission_state(&record)
        })
    }
    pub(crate) fn add_message(&self, id: i64, role: &str, content: &str) -> Result<MessageRecord> {
        run_add_message(&self.path, id, role, content)
    }
}

/// A domain phase may combine existing LCM methods with its own marker SQL.
/// The borrowed session cannot escape or recursively submit another owner job.
pub(crate) fn with_lcm_write_session<T: Send + 'static>(
    path: &Path,
    operation: impl FnOnce(&Connection, &LcmEngine<&Connection>) -> Result<T> + Send + 'static,
) -> Result<T> {
    owned_lcm_write(path, move |engine| operation(&engine.conn, engine))
}

/// SQL-only phases for compatibility callers whose durable tables share Core.
/// Closures cannot retain a connection, submit recursive jobs, or run model I/O.
pub(crate) fn with_lcm_write<T: Send + 'static>(
    path: &Path,
    operation: impl FnOnce(&Connection) -> Result<T> + Send + 'static,
) -> Result<T> {
    owned_lcm_write(path, move |engine| operation(&engine.conn))
}
pub(crate) fn with_lcm_read<T: Send + 'static>(
    path: &Path,
    operation: impl FnOnce(&Connection) -> Result<T> + Send + 'static,
) -> Result<T> {
    owned_lcm_read(path, operation)
}
