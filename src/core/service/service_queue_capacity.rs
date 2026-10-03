/// Durable runtime configuration, never read from the process environment.
/// A single conversation keeps one worker because its context is ordered.
#[derive(Debug, Clone, Copy)]
struct QueueWorkerCapacity {
    max_workers: usize,
}

impl QueueWorkerCapacity {
    fn load(root: &Path) -> Result<Self> {
        let max_workers = runtime_env::get_runtime_env_value(root, "queue.worker_capacity")
            .map(|value| value.parse::<usize>())
            .transpose()
            .context("queue.worker_capacity must be an integer between 1 and 8")?
            .unwrap_or(4);
        anyhow::ensure!(
            (1..=8).contains(&max_workers),
            "queue.worker_capacity must be between 1 and 8"
        );
        Ok(Self { max_workers })
    }
}

pub fn configure_queue_worker_capacity(
    root: &Path,
    workers: Option<usize>,
) -> Result<serde_json::Value> {
    if let Some(workers) = workers {
        anyhow::ensure!(
            (1..=8).contains(&workers),
            "queue capacity must be between 1 and 8"
        );
        runtime_env::set_runtime_env_value(root, "queue.worker_capacity", &workers.to_string())?;
    }
    let capacity = QueueWorkerCapacity::load(root)?;
    Ok(serde_json::json!({
        "max_workers": capacity.max_workers,
        "workers_per_thread": 1,
        "scope": "independent business_os.chat.task sessions",
        "storage": "SQLite runtime store"
    }))
}

fn queue_job_has_independent_business_session(job: &QueuedPrompt) -> bool {
    job.source_label == "queue"
        && job.leased_message_keys.len() == 1
        && job.ticket_self_work_id.is_none()
        && job.leased_ticket_event_keys.is_empty()
        && job
            .thread_key
            .as_deref()
            .is_some_and(|key| !key.trim().is_empty())
        && metadata_string(&job.queue_task_metadata, "business_os_command_type").as_deref()
            == Some("business_os.chat.task")
        && business_os_app_module_target_from_metadata(&job.queue_task_metadata).is_none()
}

/// Reserve every admitted slot before spawning, so asynchronous worker startup
/// cannot overbook the pool. The normal serial router retains ownership of
/// external communication, app authoring and jobs without isolated sessions.
fn lease_business_queue_capacity(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
) -> Result<Vec<QueuedPrompt>> {
    let capacity = QueueWorkerCapacity::load(root)?;
    if capacity.max_workers == 1
        || !crate::service::working_hours::accepts_work(root)
        || crate::business_os::harness_cockpit::queue_is_paused(root)
    {
        return Ok(Vec::new());
    }
    let tasks = channels::list_queue_tasks(root, &["pending".to_string()], 128)?;
    let mut admitted = Vec::new();
    let mut shared = lock_shared_state(state);
    if shared.app_recovery_active
        || shared.durable_queue_lease_in_progress
        || runtime_blocker_backoff_remaining_secs(&shared).is_some()
    {
        return Ok(admitted);
    }
    // Isolated chat sessions do not use the serial worker's context. Count its
    // slot instead of vetoing the entire pool. Reserved chats count before
    // PromptWorkerActivity::start, so startup cannot overbook capacity.
    let active_chats = shared
        .parallel_queue_jobs
        .keys()
        .filter(|key| shared.active_worker_lease_keys.contains(*key))
        .count();
    let serial_slots = shared
        .worker_active_count
        .saturating_sub(active_chats)
        .saturating_add(usize::from(shared.serial_prompt_starting))
        .max(usize::from(!shared.pending_prompts.is_empty()));
    let serial_slots = serial_slots.max(usize::from(
        shared.busy && shared.parallel_queue_jobs.is_empty(),
    ));
    for task in tasks {
        if serial_slots + shared.parallel_queue_jobs.len() >= capacity.max_workers {
            break;
        }
        let candidate = queued_prompt_from_queue_task(task);
        if !queue_job_has_independent_business_session(&candidate)
            || queued_prompt_is_synthetic_e2e_bench_leftover(&candidate)
            || candidate
                .thread_key
                .as_ref()
                .is_some_and(|key| shared.active_worker_threads.contains_key(key))
            || shared
                .parallel_queue_jobs
                .values()
                .any(|job| job.thread_key == candidate.thread_key)
            || shared
                .pending_prompts
                .iter()
                .any(|job| job.thread_key == candidate.thread_key)
        {
            continue;
        }
        let key = &candidate.leased_message_keys[0];
        match channels::queue_task_deferred_until(root, key) {
            Ok(None) => {}
            Ok(Some(_)) => continue,
            Err(error) => {
                push_event_locked(
                    &mut shared,
                    format!("Queue cooldown lookup skipped {key}: {error}"),
                );
                continue;
            }
        }
        let leased = match channels::lease_queue_task(root, key, CHANNEL_ROUTER_LEASE_OWNER) {
            Ok(task) => task,
            Err(error) => {
                // Preserve all already reserved work even if another CLI or
                // native dispatcher won this candidate's CAS.
                push_event_locked(
                    &mut shared,
                    format!("Queue capacity lease skipped {key}: {error}"),
                );
                continue;
            }
        };
        let job = queued_prompt_from_queue_task(leased);
        shared
            .parallel_queue_jobs
            .insert(job.leased_message_keys[0].clone(), job.clone());
        track_leased_keys_locked(&mut shared, &job.leased_message_keys, &[]);
        shared.busy = true;
        admitted.push(job);
    }
    Ok(admitted)
}

#[cfg(test)]
mod queue_capacity_tests {
    use super::*;

    #[test]
    fn finished_chat_cannot_dispatch_buffered_work_over_a_live_serial_worker() -> Result<()> {
        let root = tempfile::tempdir()?;
        let task = channels::create_queue_task(
            root.path(),
            channels::QueueTaskCreateRequest {
                title: "buffered serial successor".into(),
                prompt: "Read the fixture".into(),
                thread_key: "serial/successor".into(),
                workspace_root: None,
                priority: "normal".into(),
                suggested_skill: None,
                parent_message_key: None,
                extra_metadata: None,
            },
        )?;
        let job = queued_prompt_from_queue_task(task);
        // Chat finalization clears busy before the other worker's registration
        // drains. Neither direct intake nor buffered dispatch may trust busy alone.
        let mut shared = SharedState {
            busy: false,
            worker_active_count: 1,
            pending_prompts: VecDeque::from([job.clone()]),
            ..SharedState::default()
        };
        assert!(serial_prompt_admission_is_busy(&shared));
        assert!(maybe_start_next_queued_prompt_locked(root.path(), &mut shared).is_none());
        assert_eq!(shared.pending_prompts.len(), 1);

        shared.worker_active_count = 0;
        shared
            .parallel_queue_jobs
            .insert("unstarted-chat".into(), job);
        assert!(maybe_start_next_queued_prompt_locked(root.path(), &mut shared).is_none());
        shared.parallel_queue_jobs.clear();
        shared.serial_prompt_starting = true;
        assert!(maybe_start_next_queued_prompt_locked(root.path(), &mut shared).is_none());

        shared.serial_prompt_starting = false;
        assert!(!serial_prompt_admission_is_busy(&shared));
        let next = maybe_start_next_queued_prompt_locked(root.path(), &mut shared)
            .expect("worker cleanup makes the buffered successor dispatchable");
        assert_eq!(next.thread_key.as_deref(), Some("serial/successor"));
        assert!(shared.pending_prompts.is_empty());
        assert!(shared.serial_prompt_starting);
        assert!(serial_prompt_admission_is_busy(&shared));
        Ok(())
    }

    #[test]
    fn serial_start_reservation_survives_parallel_worker_startup() -> Result<()> {
        let root = tempfile::tempdir()?;
        runtime_env::set_runtime_env_value(root.path(), "queue.worker_capacity", "3")?;
        for index in 0..4 {
            channels::create_queue_task(
                root.path(),
                channels::QueueTaskCreateRequest {
                    title: format!("startup {index}"),
                    prompt: "Read the fixture".into(),
                    thread_key: format!("startup/{index}"),
                    workspace_root: None,
                    priority: "normal".into(),
                    suggested_skill: None,
                    parent_message_key: None,
                    extra_metadata: Some(
                        serde_json::json!({"business_os_command_type":"business_os.chat.task"}),
                    ),
                },
            )?;
        }
        let state = Arc::new(Mutex::new(SharedState {
            busy: true,
            serial_prompt_starting: true,
            ..SharedState::default()
        }));
        let jobs = lease_business_queue_capacity(root.path(), &state)?;
        assert_eq!(jobs.len(), 2);
        {
            let mut shared = lock_shared_state(&state);
            shared.worker_active_count = 1;
            // An unrelated completion may clear busy during serial startup.
            shared.busy = false;
            shared
                .active_worker_lease_keys
                .insert(jobs[0].leased_message_keys[0].clone());
        }
        assert!(
            lease_business_queue_capacity(root.path(), &state)?.is_empty(),
            "starting one chat must not erase the serial startup reservation"
        );
        {
            let mut shared = lock_shared_state(&state);
            shared.serial_prompt_starting = false;
            shared.worker_active_count = 2;
        }
        assert!(
            lease_business_queue_capacity(root.path(), &state)?.is_empty(),
            "serial registration replaces its reservation without changing occupied capacity"
        );
        lock_shared_state(&state).worker_active_count = 1;
        assert_eq!(
            lease_business_queue_capacity(root.path(), &state)?.len(),
            1,
            "serial completion frees precisely one slot"
        );
        Ok(())
    }

    #[test]
    fn serial_worker_leaves_free_slots_for_isolated_chats() -> Result<()> {
        let root = tempfile::tempdir()?;
        runtime_env::set_runtime_env_value(root.path(), "queue.worker_capacity", "3")?;
        for index in 0..6 {
            channels::create_queue_task(
                root.path(),
                channels::QueueTaskCreateRequest {
                    title: format!("research {index}"),
                    prompt: "Read the supplied lead".into(),
                    thread_key: format!("research/{}", index / 2),
                    workspace_root: None,
                    priority: "normal".into(),
                    suggested_skill: None,
                    parent_message_key: None,
                    extra_metadata: Some(
                        serde_json::json!({"business_os_command_type":"business_os.chat.task"}),
                    ),
                },
            )?;
        }
        let state = Arc::new(Mutex::new(SharedState {
            busy: true,
            worker_active_count: 1,
            active_worker_threads: BTreeMap::from([("research/0".into(), 1)]),
            ..SharedState::default()
        }));
        for index in 0..4 {
            let task = channels::create_queue_task(
                root.path(),
                channels::QueueTaskCreateRequest {
                    title: format!("buffered serial {index}"),
                    prompt: "Reconcile the fixture".into(),
                    thread_key: format!("buffered/{index}"),
                    workspace_root: None,
                    priority: "normal".into(),
                    suggested_skill: None,
                    parent_message_key: None,
                    extra_metadata: None,
                },
            )?;
            lock_shared_state(&state)
                .pending_prompts
                .push_back(queued_prompt_from_queue_task(task));
        }
        let jobs = lease_business_queue_capacity(root.path(), &state)?;
        assert_eq!(
            jobs.len(),
            2,
            "one serial worker consumes one of three slots"
        );
        assert_ne!(jobs[0].thread_key, jobs[1].thread_key);
        assert!(
            jobs.iter()
                .all(|job| job.thread_key.as_deref() != Some("research/0")),
            "serial context must retain exclusive ownership of its thread"
        );
        assert!(
            lease_business_queue_capacity(root.path(), &state)?.is_empty(),
            "unstarted reservations still consume capacity"
        );
        {
            let mut shared = lock_shared_state(&state);
            shared.worker_active_count = 3;
            shared
                .active_worker_lease_keys
                .extend(jobs.iter().map(|job| job.leased_message_keys[0].clone()));
        }
        assert!(lease_business_queue_capacity(root.path(), &state)?.is_empty());
        Ok(())
    }

    #[test]
    fn incident_capacity_leases_four_of_five_waiting_independent_tasks() -> Result<()> {
        let root =
            std::env::temp_dir().join(format!("ctox-incident-capacity-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root)?;
        runtime_env::set_runtime_env_value(&root, "queue.worker_capacity", "4")?;
        for index in 0..5 {
            channels::create_queue_task(
                &root,
                channels::QueueTaskCreateRequest {
                    title: format!("incident independent research {index}"),
                    prompt: "Research the supplied lead".into(),
                    thread_key: format!("incident/research/{index}"),
                    workspace_root: None,
                    priority: "urgent".into(),
                    suggested_skill: None,
                    parent_message_key: None,
                    extra_metadata: Some(
                        serde_json::json!({"business_os_command_type": "business_os.chat.task"}),
                    ),
                },
            )?;
        }
        let state = Arc::new(Mutex::new(SharedState::default()));
        let jobs = lease_business_queue_capacity(&root, &state)?;
        assert_eq!(jobs.len(), 4);
        assert_eq!(
            channels::list_queue_tasks(&root, &["leased".into()], 100)?.len(),
            4
        );
        assert_eq!(
            channels::list_queue_tasks(&root, &["pending".into()], 100)?.len(),
            1
        );
        assert!(
            lease_business_queue_capacity(&root, &state)?.is_empty(),
            "reservations count before workers start"
        );
        for job in &jobs {
            assert!(!queue_job_reuses_persistent_session(
                &chat_turn_session_options_for_queue_job(job)
            ));
        }
        runtime_env::set_runtime_env_value(&root, "queue.worker_capacity", "9")?;
        assert!(QueueWorkerCapacity::load(&root).is_err());
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
