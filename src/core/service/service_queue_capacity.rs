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
    if capacity.max_workers == 1 || !crate::service::working_hours::accepts_work(root) {
        return Ok(Vec::new());
    }
    let tasks = channels::list_queue_tasks(root, &["pending".to_string()], 128)?;
    let mut admitted = Vec::new();
    let mut shared = lock_shared_state(state);
    if shared.app_recovery_active
        || shared.durable_queue_lease_in_progress
        || !shared.pending_prompts.is_empty()
        || runtime_blocker_backoff_remaining_secs(&shared).is_some()
        || shared.worker_active_count > shared.parallel_queue_jobs.len()
        || (shared.busy && shared.parallel_queue_jobs.is_empty())
    {
        return Ok(admitted);
    }
    for task in tasks {
        if shared.parallel_queue_jobs.len() >= capacity.max_workers {
            break;
        }
        let candidate = queued_prompt_from_queue_task(task);
        if !queue_job_has_independent_business_session(&candidate)
            || queued_prompt_is_synthetic_e2e_bench_leftover(&candidate)
            || shared
                .parallel_queue_jobs
                .values()
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
