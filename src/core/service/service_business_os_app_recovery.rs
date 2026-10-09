fn business_os_queue_task_is_app_module(task: &channels::QueueTaskView) -> bool {
    business_os_app_module_target_from_metadata(&task.metadata).is_some()
}

fn leased_business_os_app_queue_task_exists(root: &Path) -> Result<bool> {
    let tasks = channels::list_queue_tasks(root, &["leased".to_string()], 32)?;
    Ok(tasks.iter().any(business_os_queue_task_is_app_module))
}

fn leased_business_os_app_queue_task_exists_for_stop_guard(root: &Path) -> Result<bool> {
    if leased_business_os_app_queue_task_exists(root)? {
        return Ok(true);
    }
    leased_business_os_rxdb_app_queue_task_exists(root)
}

fn leased_business_os_rxdb_app_queue_task_exists(root: &Path) -> Result<bool> {
    let db_path = crate::paths::runtime_dir(root).join("business-os-rxdb.sqlite3");
    if !db_path.is_file() {
        return Ok(false);
    }
    let conn = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| {
        format!(
            "failed to open Business OS RxDB queue store {}",
            db_path.display()
        )
    })?;
    let mut stmt = match conn.prepare(
        r#"
        SELECT json_extract(data, '$.metadata')
        FROM ctox_business_os__ctox_queue_tasks__v0
        WHERE deleted = 0
          AND json_valid(data)
          AND COALESCE(json_extract(data, '$.route_status'), '') = 'leased'
          AND json_extract(data, '$.metadata.business_os_command_type') IN (
              'ctox.business_os.app.create',
              'ctox.business_os.app.modify'
          )
        LIMIT 32
        "#,
    ) {
        Ok(stmt) => stmt,
        Err(err) if err.to_string().contains("no such table") => return Ok(false),
        Err(err) => {
            return Err(err).with_context(|| {
                format!(
                    "failed to inspect Business OS RxDB queue table {}",
                    db_path.display()
                )
            });
        }
    };
    let rows = stmt.query_map([], |row| row.get::<_, Option<String>>(0))?;
    for row in rows {
        let metadata = row?
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .unwrap_or(Value::Null);
        if business_os_app_module_target_from_metadata(&metadata).is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn maybe_lease_next_durable_queue_prompt(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
    guard: DurableQueueDispatchGuard,
) -> Result<Option<QueuedPrompt>> {
    let Some(lease_attempt) = begin_durable_queue_lease_attempt(root, state, guard) else {
        return Ok(None);
    };
    if let Some(prompt) = lease_priority_system_queue_prompt(root, state, &lease_attempt)? {
        return Ok(Some(prompt));
    }
    // Every path that hands the serial slot to durable queue work (router,
    // working-hours tick, worker-idle kick) passes here, so communication
    // that waits for the slot is checked once for all of them.
    if communication_inbound_waiting(root) {
        return Ok(None);
    }
    let mut app_queue_lease_active = leased_business_os_app_queue_task_exists(root)?;
    if !lease_attempt.is_current() {
        return Ok(None);
    }
    if mark_business_os_app_recovery_preflight_due(root) {
        let recovery = if app_queue_lease_active {
            recover_stale_business_os_app_queue_tasks(
                root,
                state,
                BUSINESS_OS_APP_RECOVERY_SCAN_LIMIT,
            )
        } else {
            recover_abandoned_business_os_app_queue_tasks(
                root,
                state,
                BUSINESS_OS_APP_RECOVERY_SCAN_LIMIT,
            )
        };
        match recovery {
            Ok(updated) if updated > 0 => {
                mark_business_os_app_recovery_ran(root);
                push_event(
                    state,
                    format!(
                        "Recovered {updated} stale or abandoned Business OS app queue task(s) before leasing new work"
                    ),
                );
            }
            Ok(_) => {
                mark_business_os_app_recovery_ran(root);
            }
            Err(err) => push_event(
                state,
                format!(
                    "Business OS app validation recovery skipped: {}",
                    clip_text(&err.to_string(), 180)
                ),
            ),
        }
        app_queue_lease_active = leased_business_os_app_queue_task_exists(root)?;
        if !lease_attempt.is_current() {
            return Ok(None);
        }
    }
    match quarantine_synthetic_e2e_queue_tasks_before_dispatch(root) {
        Ok(blocked) if blocked > 0 => push_event(
            state,
            format!(
                "Quarantined {blocked} synthetic E2E/bench queue task(s) before durable dispatch"
            ),
        ),
        Ok(_) => {}
        Err(err) => push_event(
            state,
            format!(
                "Synthetic E2E/bench queue quarantine skipped: {}",
                clip_text(&err.to_string(), 180)
            ),
        ),
    }
    if !app_queue_lease_active {
        if !lease_attempt.is_current() {
            return Ok(None);
        }
        if let Some(prompt) = maybe_lease_business_os_app_validation_rework(root, state)? {
            clear_idle_durable_queue_empty_gate(root);
            return Ok(Some(prompt));
        }
    }
    let tasks = channels::list_queue_tasks(root, &["pending".to_string()], 16)?;
    for task in tasks {
        if channels::queue_task_deferred_until(root, &task.message_key)?.is_some() {
            continue;
        }
        if block_repeated_unstarted_business_os_app_queue_task_before_dispatch(root, &task)? {
            push_event(
                state,
                format!(
                    "Blocked repeated unstarted Business OS app queue task {} before durable dispatch",
                    task.message_key
                ),
            );
            continue;
        }
        if app_queue_lease_active && business_os_queue_task_is_app_module(&task) {
            continue;
        }
        if appsec_pipeline_queue_task_state_dir(root, &task)?.is_some() {
            continue;
        }
        if durable_queue_task_already_enqueued_in_memory_or_clear_stale(state, &task.message_key) {
            continue;
        }
        if !lease_attempt.is_current() {
            return Ok(None);
        }
        let leased =
            channels::lease_queue_task(root, &task.message_key, CHANNEL_ROUTER_LEASE_OWNER)?;
        clear_idle_durable_queue_empty_gate(root);
        return Ok(Some(queued_prompt_from_queue_task(leased)));
    }
    Ok(None)
}

fn mark_business_os_app_recovery_preflight_due(root: &Path) -> bool {
    let root = root.to_path_buf();
    let gate = BUSINESS_OS_APP_RECOVERY_PREFLIGHT_GATE.get_or_init(|| Mutex::new(None));
    let mut gate = gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(previous) = gate.as_ref() {
        if previous.root == root
            && previous.last_run.elapsed()
                < Duration::from_secs(BUSINESS_OS_APP_RECOVERY_PREFLIGHT_IDLE_SAFETY_SECS)
        {
            return false;
        }
    }
    *gate = Some(BusinessOsAppRecoveryPreflightGateState {
        root,
        last_run: Instant::now(),
    });
    true
}

fn should_skip_idle_business_os_app_recovery(root: &Path) -> bool {
    let root_path = root.to_path_buf();
    let source_stamp = business_os_app_recovery_source_stamp(root);
    let now = Instant::now();
    let gate = BUSINESS_OS_APP_RECOVERY_IDLE_GATE.get_or_init(|| Mutex::new(None));
    let guard = gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(previous) = guard.as_ref() else {
        return false;
    };
    if business_os_app_recovery_source_has_due_time(&source_stamp) {
        return false;
    }
    previous.root == root_path
        && previous.source_stamp == source_stamp
        && now.duration_since(previous.last_run)
            < Duration::from_secs(BUSINESS_OS_APP_RECOVERY_IDLE_SAFETY_SECS)
}

fn mark_business_os_app_recovery_ran(root: &Path) {
    let root_path = root.to_path_buf();
    let source_stamp = business_os_app_recovery_source_stamp(root);
    let gate = BUSINESS_OS_APP_RECOVERY_IDLE_GATE.get_or_init(|| Mutex::new(None));
    let mut guard = gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some(BusinessOsAppRecoveryIdleGateState {
        root: root_path,
        source_stamp,
        last_run: Instant::now(),
    });
}

#[cfg(test)]
fn clear_business_os_app_recovery_idle_gate_for_tests() {
    if let Some(gate) = BUSINESS_OS_APP_RECOVERY_IDLE_GATE.get() {
        let mut guard = gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = None;
    }
}

const SYNTHETIC_E2E_QUEUE_BLOCK_NOTE: &str =
    "Synthetic Business OS E2E/bench task quarantined before service dispatch.";

fn quarantine_synthetic_e2e_queue_tasks_before_dispatch(root: &Path) -> Result<usize> {
    let statuses = ["pending", "leased", "review_rework"]
        .iter()
        .map(|status| status.to_string())
        .collect::<Vec<_>>();
    let tasks = channels::list_queue_tasks(root, &statuses, 64)?;
    let mut blocked = 0usize;
    for task in tasks {
        if !queue_task_is_synthetic_e2e_bench_leftover(&task) {
            continue;
        }
        channels::update_queue_task(
            root,
            channels::QueueTaskUpdateRequest {
                message_key: task.message_key.clone(),
                route_status: Some("blocked".to_string()),
                status_note: Some(SYNTHETIC_E2E_QUEUE_BLOCK_NOTE.to_string()),
                ..Default::default()
            },
        )?;
        blocked = blocked.saturating_add(1);
    }
    Ok(blocked)
}

fn queue_task_is_synthetic_e2e_bench_leftover(task: &channels::QueueTaskView) -> bool {
    queue_identity_is_synthetic_e2e_bench_leftover(&task.thread_key, &task.title, &task.prompt)
}

fn queued_prompt_is_synthetic_e2e_bench_leftover(prompt: &QueuedPrompt) -> bool {
    queue_identity_is_synthetic_e2e_bench_leftover(
        prompt.thread_key.as_deref().unwrap_or_default(),
        &prompt.goal,
        &prompt.prompt,
    )
}

fn queue_identity_is_synthetic_e2e_bench_leftover(
    thread_key: &str,
    title: &str,
    prompt: &str,
) -> bool {
    let haystack = format!("{thread_key}\n{title}\n{prompt}");
    if !contains_any(&haystack, &["CTOX_E2E_", "ctox_e2e_", "_thread_isolation"]) {
        return false;
    }
    thread_key.starts_with("business-os/bench_")
        || thread_key.contains("evidence-live-cli-admin")
        || contains_any(title, &["CTOX_E2E_", "ctox_e2e_"])
        || contains_any(prompt, &["CTOX_E2E_", "ctox_e2e_"])
}

fn block_synthetic_e2e_queued_prompt_before_dispatch(
    root: &Path,
    prompt: &QueuedPrompt,
) -> Result<usize> {
    if !queued_prompt_is_synthetic_e2e_bench_leftover(prompt) {
        return Ok(0);
    }
    let mut blocked = 0usize;
    for message_key in &prompt.leased_message_keys {
        let Some(task) = channels::load_queue_task(root, message_key)? else {
            continue;
        };
        if !queue_task_is_synthetic_e2e_bench_leftover(&task) {
            continue;
        }
        channels::update_queue_task(
            root,
            channels::QueueTaskUpdateRequest {
                message_key: message_key.clone(),
                route_status: Some("blocked".to_string()),
                status_note: Some(SYNTHETIC_E2E_QUEUE_BLOCK_NOTE.to_string()),
                ..Default::default()
            },
        )?;
        let _ = crate::business_os::store::refresh_business_command_queue_task_projection(
            root,
            message_key,
        );
        blocked = blocked.saturating_add(1);
    }
    Ok(blocked)
}

fn priority_system_queue_task_is_eligible(task: &channels::QueueTaskView) -> bool {
    matches!(task.priority.as_str(), "urgent" | "high")
        && !business_os_queue_task_is_app_module(task)
        && !queue_task_is_synthetic_e2e_bench_leftover(task)
}

fn maybe_lease_priority_system_queue_prompt_for_idle_dispatch(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
) -> Result<Option<QueuedPrompt>> {
    if crate::service::working_hours::hold_reason(root).is_some()
        || runtime_blocker_backoff_remaining_secs(&lock_shared_state(state)).is_some()
    {
        return Ok(None);
    }
    let Some(attempt) =
        begin_durable_queue_lease_attempt(root, state, DurableQueueDispatchGuard::StrictIdle)
    else {
        return Ok(None);
    };
    lease_priority_system_queue_prompt(root, state, &attempt)
}

fn lease_priority_system_queue_prompt(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
    attempt: &DurableQueueLeaseAttemptGuard,
) -> Result<Option<QueuedPrompt>> {
    if crate::service::working_hours::hold_reason(root).is_some()
        || runtime_blocker_backoff_remaining_secs(&lock_shared_state(state)).is_some()
    {
        return Ok(None);
    }
    // A leased app is not ownership of the free serial slot. Never lease a
    // second app here; validation/recovery remains with its existing owner.
    for task in channels::list_pending_priority_system_queue_tasks(root, 64)? {
        if !priority_system_queue_task_is_eligible(&task)
            || channels::queue_task_deferred_until(root, &task.message_key)?.is_some()
            || appsec_pipeline_queue_task_state_dir(root, &task)?.is_some()
            || durable_queue_task_already_enqueued_in_memory_or_clear_stale(
                state,
                &task.message_key,
            )
        {
            continue;
        }
        if !attempt.is_current() {
            return Ok(None);
        }
        let leased = channels::lease_queue_task_if(
            root,
            &task.message_key,
            CHANNEL_ROUTER_LEASE_OWNER,
            priority_system_queue_task_is_eligible,
        )?;
        clear_idle_durable_queue_empty_gate(root);
        return Ok(Some(queued_prompt_from_queue_task(leased)));
    }
    Ok(None)
}

fn maybe_lease_next_durable_queue_prompt_for_idle_dispatch(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
) -> Result<Option<QueuedPrompt>> {
    maybe_lease_next_durable_queue_prompt(root, state, DurableQueueDispatchGuard::StrictIdle)
}

fn maybe_lease_next_durable_queue_prompt_for_worker_finalization(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
) -> Result<Option<QueuedPrompt>> {
    maybe_lease_next_durable_queue_prompt(
        root,
        state,
        DurableQueueDispatchGuard::CurrentWorkerFinalizing,
    )
}

fn maybe_lease_next_durable_queue_after_worker_idle(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
) -> Result<Option<QueuedPrompt>> {
    // A worker can move its own durable task from `review_rework` back to
    // `pending` while the idle dispatcher still holds an "empty queue" gate.
    // The source-stamp cache may not observe that transition immediately when
    // SQLite writes land in the WAL. A worker-idle kick is an explicit signal
    // that finalization may have made durable work runnable, so it must bypass
    // the stale empty result instead of waiting for the hourly safety poll.
    clear_idle_durable_queue_empty_gate(root);
    maybe_lease_next_durable_queue_prompt_for_idle_dispatch(root, state)
}

fn maybe_enqueue_next_durable_queue_after_worker_idle(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
    event: &str,
) {
    match maybe_lease_next_durable_queue_after_worker_idle(root, state) {
        Ok(Some(queued)) => enqueue_prompt(root, state, queued, event.to_string()),
        Ok(None) => {}
        Err(err) => push_event(
            state,
            format!(
                "Failed to lease next durable queue task after worker activity dropped: {}",
                clip_text(&err.to_string(), 180)
            ),
        ),
    }
}

fn spawn_delayed_worker_idle_queue_kick(
    root: PathBuf,
    state: Arc<Mutex<SharedState>>,
    event: String,
) {
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(WORKER_IDLE_QUEUE_KICK_DELAY_MS));
        let kicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            maybe_enqueue_next_durable_queue_after_worker_idle(&root, &state, &event);
        }));
        if kicked.is_err() {
            push_event(
                &state,
                "Delayed worker-idle queue kick panicked; continuing".to_string(),
            );
        }
    });
}

fn recover_stale_business_os_app_queue_tasks(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
    limit: usize,
) -> Result<usize> {
    recover_stale_business_os_app_queue_task_summary(
        root,
        state,
        limit,
        BUSINESS_OS_APP_RECOVERY_STALE_SECS,
    )
    .map(BusinessOsAppQueueRecoverySummary::total)
}

fn recover_abandoned_business_os_app_queue_tasks(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
    limit: usize,
) -> Result<usize> {
    recover_stale_business_os_app_queue_task_summary(root, state, limit, 0)
        .map(BusinessOsAppQueueRecoverySummary::total)
}

fn recover_stale_business_os_app_queue_task_summary(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
    limit: usize,
    minimum_lease_age_secs: u64,
) -> Result<BusinessOsAppQueueRecoverySummary> {
    let tasks = channels::list_queue_tasks(root, &["leased".to_string()], limit.max(1))?;
    let mut summary = BusinessOsAppQueueRecoverySummary::default();
    for task in tasks {
        if active_or_pending_leased_message_key(state, &task.message_key) {
            continue;
        }
        if !queue_task_lease_is_stale_enough(&task, minimum_lease_age_secs) {
            continue;
        }
        clear_idle_pending_prompt_shadow_for_recovery(state, &task.message_key);
        let Some(target) = business_os_app_module_target_from_metadata(&task.metadata) else {
            continue;
        };
        if requeue_unstarted_business_os_app_queue_task(root, &task, &target)? {
            summary.rework = summary.rework.saturating_add(1);
            continue;
        }
        match recover_business_os_app_queue_task_from_validation(
            root,
            &task.message_key,
            "Business OS app artifacts validated during idle recovery",
            "Business OS app artifact validation failed during idle recovery for queue",
        )? {
            BusinessOsAppValidationQueueRecovery::Handled => {
                summary.handled = summary.handled.saturating_add(1);
            }
            BusinessOsAppValidationQueueRecovery::Rework => {
                summary.rework = summary.rework.saturating_add(1);
            }
            BusinessOsAppValidationQueueRecovery::Failed => {
                summary.failed = summary.failed.saturating_add(1);
            }
            BusinessOsAppValidationQueueRecovery::Unchanged => {}
        }
    }
    Ok(summary)
}

fn clear_idle_pending_prompt_shadow_for_recovery(
    state: &Arc<Mutex<SharedState>>,
    message_key: &str,
) {
    let mut shared = lock_shared_state(state);
    if shared.busy || shared.worker_active_count > 0 || shared.durable_queue_lease_in_progress {
        return;
    }
    let before = shared.pending_prompts.len();
    shared.pending_prompts.retain(|prompt| {
        !prompt
            .leased_message_keys
            .iter()
            .any(|key| key == message_key)
    });
    if shared.pending_prompts.len() != before {
        push_event_locked(
            &mut shared,
            format!("Cleared idle pending prompt shadow for abandoned queue task {message_key}"),
        );
    }
}

fn queue_task_lease_is_stale_enough(
    task: &channels::QueueTaskView,
    minimum_lease_age_secs: u64,
) -> bool {
    let Some(leased_at) = task.leased_at.as_deref() else {
        return false;
    };
    let Some(leased_at) = parse_rfc3339_system_time(leased_at) else {
        return false;
    };
    SystemTime::now()
        .duration_since(leased_at)
        .map(|age| age >= Duration::from_secs(minimum_lease_age_secs))
        .unwrap_or(false)
}

fn requeue_unstarted_business_os_app_queue_task(
    root: &Path,
    task: &channels::QueueTaskView,
    target: &BusinessOsAppModuleTarget,
) -> Result<bool> {
    if task.route_status != "leased" {
        return Ok(false);
    }
    let app_workspace_root = business_os_app_task_workspace_root(root, task);
    let artifact_dir = app_workspace_root.join(&target.artifact_directory);
    if business_os_app_artifact_dir_has_user_content(&artifact_dir)? {
        return Ok(false);
    }
    let previous_requeues = business_os_app_unstarted_requeue_count(root, task)?;
    let next_requeues = previous_requeues.saturating_add(1);
    channels::set_queue_task_metadata_value(
        root,
        &task.message_key,
        BUSINESS_OS_APP_UNSTARTED_REQUEUE_COUNT_KEY,
        Value::from(next_requeues as u64),
    )?;
    channels::set_queue_task_metadata_value(
        root,
        &task.message_key,
        BUSINESS_OS_APP_UNSTARTED_REQUEUED_AT_KEY,
        Value::String(now_iso_string()),
    )?;
    if previous_requeues >= BUSINESS_OS_APP_UNSTARTED_REQUEUE_BLOCK_THRESHOLD {
        block_unstarted_business_os_app_queue_task(root, task, previous_requeues)?;
        return Ok(true);
    }
    let _updated = channels::update_queue_task(
        root,
        channels::QueueTaskUpdateRequest {
            message_key: task.message_key.clone(),
            route_status: Some("pending".to_string()),
            status_note: Some(
                "business-os:requeued-unstarted-app: app target missing or empty".to_string(),
            ),
            ..Default::default()
        },
    )?;
    let _ = crate::business_os::store::refresh_business_command_queue_task_projection(
        root,
        &task.message_key,
    );
    Ok(true)
}

fn block_repeated_unstarted_business_os_app_queue_task_before_dispatch(
    root: &Path,
    task: &channels::QueueTaskView,
) -> Result<bool> {
    let Some(target) = business_os_app_module_target_from_metadata(&task.metadata) else {
        return Ok(false);
    };
    let previous_requeues = business_os_app_unstarted_requeue_count(root, task)?;
    if previous_requeues < BUSINESS_OS_APP_UNSTARTED_REQUEUE_BLOCK_THRESHOLD {
        return Ok(false);
    }
    let app_workspace_root = business_os_app_task_workspace_root(root, task);
    let artifact_dir = app_workspace_root.join(&target.artifact_directory);
    if business_os_app_artifact_dir_has_user_content(&artifact_dir)? {
        return Ok(false);
    }
    channels::set_queue_task_metadata_value(
        root,
        &task.message_key,
        BUSINESS_OS_APP_UNSTARTED_REQUEUE_COUNT_KEY,
        Value::from(previous_requeues as u64),
    )?;
    block_unstarted_business_os_app_queue_task(root, task, previous_requeues)?;
    Ok(true)
}

fn block_unstarted_business_os_app_queue_task(
    root: &Path,
    task: &channels::QueueTaskView,
    previous_requeues: usize,
) -> Result<()> {
    let _updated = channels::update_queue_task(
        root,
        channels::QueueTaskUpdateRequest {
            message_key: task.message_key.clone(),
            route_status: Some("blocked".to_string()),
            status_note: Some(format!(
                "business-os:blocked-unstarted-app: app target still missing or empty after {previous_requeues} automatic requeue(s); owner review required before retry"
            )),
            ..Default::default()
        },
    )?;
    let _ = crate::business_os::store::refresh_business_command_queue_task_projection(
        root,
        &task.message_key,
    );
    Ok(())
}

fn business_os_app_unstarted_requeue_count(
    root: &Path,
    task: &channels::QueueTaskView,
) -> Result<usize> {
    let stored = channels::queue_task_metadata_value(
        root,
        &task.message_key,
        BUSINESS_OS_APP_UNSTARTED_REQUEUE_COUNT_KEY,
    )?;
    let mut count = match stored {
        Some(Value::Number(number)) => number.as_u64().unwrap_or(0) as usize,
        Some(Value::String(value)) => value.trim().parse::<usize>().unwrap_or(0),
        _ => 0,
    };
    if task
        .status_note
        .as_deref()
        .is_some_and(|note| note.starts_with("business-os:requeued-unstarted-app:"))
    {
        count = count.max(1);
    }
    Ok(count)
}

fn recover_business_os_app_queue_task_after_worker_finalization(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
    job: &QueuedPrompt,
    phase: &str,
) -> usize {
    if job.leased_message_keys.is_empty()
        || business_os_app_module_target_from_metadata(&job.queue_task_metadata).is_none()
    {
        return 0;
    }
    let mut updated = 0usize;
    for message_key in &job.leased_message_keys {
        match recover_business_os_app_queue_task_from_validation(
            root,
            message_key,
            "Business OS app artifacts validated after worker finalization",
            "Business OS app artifact validation failed after worker finalization for queue",
        ) {
            Ok(BusinessOsAppValidationQueueRecovery::Handled) => {
                updated = updated.saturating_add(1);
                push_event(
                    state,
                    format!("Recovered green Business OS app queue task {message_key} {phase}"),
                );
            }
            Ok(BusinessOsAppValidationQueueRecovery::Rework) => {
                updated = updated.saturating_add(1);
                push_event(
                    state,
                    format!(
                        "Moved red Business OS app queue task {message_key} to validation rework {phase}"
                    ),
                );
            }
            Ok(BusinessOsAppValidationQueueRecovery::Failed) => {
                updated = updated.saturating_add(1);
                push_event(
                    state,
                    format!(
                        "Failed exhausted Business OS app validation queue task {message_key} {phase}"
                    ),
                );
            }
            Ok(BusinessOsAppValidationQueueRecovery::Unchanged) => {}
            Err(err) => push_event(
                state,
                format!(
                    "Business OS app validation recovery skipped for {message_key} {phase}: {}",
                    clip_text(&err.to_string(), 180)
                ),
            ),
        }
    }
    updated
}

fn complete_validated_business_os_app_queue_task(
    root: &Path,
    message_key: &str,
    reason: &str,
) -> Result<bool> {
    let Some(task) = channels::load_queue_task(root, message_key)? else {
        return Ok(false);
    };
    if !matches!(
        task.route_status.as_str(),
        "leased" | "pending" | "review_rework"
    ) {
        return Ok(false);
    }
    if business_os_app_module_target_from_metadata(&task.metadata).is_none() {
        return Ok(false);
    }
    let job = queued_prompt_from_queue_task(task);
    match business_os_app_module_validation_feedback(root, &job)? {
        Some(_) => Ok(false),
        None => {
            complete_business_os_app_validation_success_to_leased_queue(root, &job, reason, None)
                .map(|updated| updated > 0)
        }
    }
}

fn recover_business_os_app_queue_task_from_validation(
    root: &Path,
    message_key: &str,
    success_reason: &str,
    rework_summary: &str,
) -> Result<BusinessOsAppValidationQueueRecovery> {
    let Some(task) = channels::load_queue_task(root, message_key)? else {
        return Ok(BusinessOsAppValidationQueueRecovery::Unchanged);
    };
    if !matches!(
        task.route_status.as_str(),
        "leased" | "pending" | "review_rework"
    ) {
        return Ok(BusinessOsAppValidationQueueRecovery::Unchanged);
    }
    if business_os_app_module_target_from_metadata(&task.metadata).is_none() {
        return Ok(BusinessOsAppValidationQueueRecovery::Unchanged);
    }
    let route_status = task.route_status.clone();
    let job = queued_prompt_from_queue_task(task);
    match business_os_app_module_validation_feedback(root, &job)? {
        None => {
            let handled = complete_business_os_app_validation_success_to_leased_queue(
                root,
                &job,
                success_reason,
                None,
            )?;
            if handled > 0 {
                Ok(BusinessOsAppValidationQueueRecovery::Handled)
            } else {
                Ok(BusinessOsAppValidationQueueRecovery::Unchanged)
            }
        }
        Some(feedback) => {
            if route_status != "leased" {
                return Ok(BusinessOsAppValidationQueueRecovery::Unchanged);
            }
            if business_os_app_validation_repair_exhausted(&job.prompt) {
                let failure_reason = format!(
                    "Business OS app validation repair attempts exhausted during recovery: {}",
                    clip_text(&feedback, 1200)
                );
                channels::ack_leased_messages_with_failure_reason(
                    root,
                    &job.leased_message_keys,
                    "failed",
                    &failure_reason,
                )?;
                for message_key in &job.leased_message_keys {
                    crate::business_os::store::fail_business_command_from_queue_error(
                        root,
                        message_key,
                        &failure_reason,
                    )?;
                }
                return Ok(BusinessOsAppValidationQueueRecovery::Failed);
            }
            let repair_attempts = business_os_app_validation_repair_attempt_count(&job.prompt);
            apply_business_os_app_validation_rework_to_leased_queue(
                root,
                &job,
                &feedback,
                rework_summary,
                repair_attempts.saturating_add(1),
                None,
            )?;
            for message_key in &job.leased_message_keys {
                let _ = crate::business_os::store::refresh_business_command_queue_task_projection(
                    root,
                    message_key,
                );
            }
            Ok(BusinessOsAppValidationQueueRecovery::Rework)
        }
    }
}

fn maybe_lease_business_os_app_validation_rework(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
) -> Result<Option<QueuedPrompt>> {
    let tasks = channels::list_queue_tasks(root, &["review_rework".to_string()], 16)?;
    for task in tasks {
        if durable_queue_task_already_enqueued_in_memory_or_clear_stale(state, &task.message_key)
            || !queue_task_is_business_os_app_validation_rework(&task)
        {
            continue;
        }
        let job = queued_prompt_from_queue_task(task.clone());
        if business_os_app_module_validation_feedback(root, &job)?.is_none() {
            let updated = complete_business_os_app_validation_success_to_leased_queue(
                root,
                &job,
                "Business OS app artifacts validated during validation rework leasing",
                None,
            )?;
            if updated > 0 {
                push_event(
                    state,
                    format!(
                        "Completed green Business OS app validation rework task {} before re-lease",
                        task.message_key
                    ),
                );
            }
            continue;
        }
        release_business_os_app_validation_rework_to_pending(root, &task.message_key)?;
        let leased =
            channels::lease_queue_task(root, &task.message_key, CHANNEL_ROUTER_LEASE_OWNER)?;
        return Ok(Some(queued_prompt_from_queue_task(leased)));
    }
    Ok(None)
}

fn release_business_os_app_validation_rework_to_pending(
    root: &Path,
    message_key: &str,
) -> Result<()> {
    enforce_business_os_app_validation_requeue_transition(root, message_key)?;
    if !channels::set_queue_task_route_status(root, message_key, "pending")? {
        anyhow::bail!("Business OS app validation rework queue task {message_key} disappeared");
    }
    Ok(())
}

fn queue_task_is_business_os_app_validation_rework(task: &channels::QueueTaskView) -> bool {
    (task.prompt.contains("Business OS app validation failed.")
        || task
            .prompt
            .contains("Business OS app artifact validation failed."))
        && business_os_app_module_target_from_metadata(&task.metadata).is_some()
}

#[cfg(test)]
const BUSINESS_OS_APP_RECOVERY_MODULE_SIZE_BOUNDARY: () = ();
