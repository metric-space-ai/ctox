fn business_os_app_module_execution_prompt(job: &QueuedPrompt) -> String {
    let Some(target) = business_os_app_module_target_from_metadata(&job.queue_task_metadata) else {
        return job.prompt.clone();
    };
    format!(
        "{}\n\nBusiness OS app resource context:\n- module_id: {}\n- install_target: {}\n- app_directory: {}\n- skill: business-os-app-module-development\n- resource.module_contract: src/skills/system/product_engineering/business-os-app-module-development/references/module-contract.md\n- resource.dos_and_donts: src/skills/system/product_engineering/business-os-app-module-development/references/dos-and-donts.md\n- resource.green_checklist: src/skills/system/product_engineering/business-os-app-module-development/references/green-checklist.md\n- resource.architecture_translation: src/skills/system/product_engineering/business-os-app-module-development/references/architecture-translation.md\n- reference_catalog: ctox business-os app references --query \"<workflow data keywords>\" --json --limit 8\n- validation: ctox business-os app validate {} {}\n- tool_boundary: do not run ctox stop/start/upgrade, launchctl, systemctl, or service lifecycle commands during app creation; the running CTOX service is the required app runtime.",
        job.prompt,
        target.module_id,
        target.install_target,
        target.artifact_directory,
        target.module_id,
        target.mode_flag,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BusinessOsAppModuleTarget {
    module_id: String,
    install_target: String,
    mode_flag: &'static str,
    artifact_directory: String,
}

fn business_os_app_module_target_from_metadata(
    metadata: &Value,
) -> Option<BusinessOsAppModuleTarget> {
    let command_type = metadata_string(metadata, "business_os_command_type")?;
    if !matches!(
        command_type.as_str(),
        "ctox.business_os.app.create" | "ctox.business_os.app.modify"
    ) {
        return None;
    }
    let module_id = metadata_string(metadata, "business_os_record_id")?;
    let install_target = metadata_string(metadata, "business_os_install_target")
        .unwrap_or_else(|| "runtime-installed-module".to_string());
    let mode_flag = if install_target == "runtime-installed-module" {
        "--installed"
    } else {
        "--source"
    };
    let artifact_directory =
        metadata_string(metadata, "business_os_app_directory").unwrap_or_else(|| {
            if mode_flag == "--installed" {
                format!("runtime/business-os/installed-modules/{module_id}")
            } else {
                format!("src/apps/business-os/modules/{module_id}")
            }
        });
    Some(BusinessOsAppModuleTarget {
        module_id,
        install_target,
        mode_flag,
        artifact_directory,
    })
}

fn prompt_line_value(prompt: &str, prefix: &str) -> Option<String> {
    prompt.lines().find_map(|line| {
        let value = line.trim().strip_prefix(prefix)?.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

fn business_os_app_workspace_root(root: &Path, job: &QueuedPrompt) -> PathBuf {
    job.workspace_root
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| business_os_app_workspace_root_looks_valid(path))
        .unwrap_or_else(|| root.to_path_buf())
}

fn business_os_app_task_workspace_root(root: &Path, task: &channels::QueueTaskView) -> PathBuf {
    task.workspace_root
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| business_os_app_workspace_root_looks_valid(path))
        .unwrap_or_else(|| root.to_path_buf())
}

fn business_os_app_workspace_root_looks_valid(path: &Path) -> bool {
    path.join("src/apps/business-os/scripts/validate-app-module.mjs")
        .is_file()
        || path.join("runtime/business-os/installed-modules").is_dir()
}

fn business_os_app_artifact_tree_stamp(path: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    let mut visited = 0usize;
    hash_business_os_app_artifact_path(path, path, 0, &mut visited, &mut hasher);
    hasher.finish()
}

fn hash_business_os_app_artifact_path(
    root: &Path,
    path: &Path,
    depth: usize,
    visited: &mut usize,
    hasher: &mut DefaultHasher,
) {
    if *visited >= BUSINESS_OS_APP_RECOVERY_ARTIFACT_STAMP_MAX_ENTRIES {
        "truncated".hash(hasher);
        return;
    }
    *visited = (*visited).saturating_add(1);
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .hash(hasher);

    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) => {
            "metadata-error".hash(hasher);
            err.kind().hash(hasher);
            return;
        }
    };
    let file_type = metadata.file_type();
    file_type.is_dir().hash(hasher);
    file_type.is_file().hash(hasher);
    file_type.is_symlink().hash(hasher);
    metadata.len().hash(hasher);
    metadata
        .modified()
        .ok()
        .map(system_time_to_unix_nanos)
        .unwrap_or(0)
        .hash(hasher);

    if !file_type.is_dir() {
        return;
    }
    if depth >= BUSINESS_OS_APP_RECOVERY_ARTIFACT_STAMP_MAX_DEPTH {
        "depth-limit".hash(hasher);
        return;
    }
    let mut entries = match std::fs::read_dir(path) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect::<Vec<_>>(),
        Err(err) => {
            "read-dir-error".hash(hasher);
            err.kind().hash(hasher);
            return;
        }
    };
    entries.sort();
    for entry in entries {
        hash_business_os_app_artifact_path(root, &entry, depth + 1, visited, hasher);
        if *visited >= BUSINESS_OS_APP_RECOVERY_ARTIFACT_STAMP_MAX_ENTRIES {
            "truncated".hash(hasher);
            break;
        }
    }
}

fn business_os_app_artifact_dir_has_user_content(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    if !path.is_dir() {
        anyhow::bail!(
            "Business OS app target {} exists but is not a directory",
            path.display()
        );
    }
    let mut entries = path
        .read_dir()
        .with_context(|| format!("failed to read Business OS app target {}", path.display()))?;
    Ok(entries.next().transpose()?.is_some())
}

fn business_os_app_validation_repair_attempt_count(prompt: &str) -> usize {
    if let Some(attempt) = business_os_app_validation_explicit_repair_attempt(prompt) {
        return attempt.min(BUSINESS_OS_APP_VALIDATION_MAX_REPAIR_ATTEMPTS);
    }
    prompt
        .match_indices(BUSINESS_OS_APP_VALIDATION_FAILURE_MARKER)
        .count()
        .min(BUSINESS_OS_APP_VALIDATION_MAX_REPAIR_ATTEMPTS)
}

fn business_os_app_validation_repair_exhausted(prompt: &str) -> bool {
    business_os_app_validation_repair_attempt_count(prompt)
        >= BUSINESS_OS_APP_VALIDATION_MAX_REPAIR_ATTEMPTS
}

fn business_os_app_validation_explicit_repair_attempt(prompt: &str) -> Option<usize> {
    prompt.lines().find_map(|line| {
        let value = line
            .trim()
            .strip_prefix("Validation repair attempt:")?
            .trim();
        let first = value
            .split(|ch: char| !ch.is_ascii_digit())
            .find(|part| !part.is_empty())?;
        first.parse::<usize>().ok()
    })
}

fn business_os_app_validation_audit_key(
    message_key: &str,
    target: Option<&BusinessOsAppModuleTarget>,
    attempt: usize,
    feedback: &str,
) -> String {
    use sha2::Digest;

    let mut hasher = sha2::Sha256::new();
    hasher.update(b"ctox-business-os-app-validation-rework-v1");
    hasher.update(message_key.as_bytes());
    if let Some(target) = target {
        hasher.update(target.module_id.as_bytes());
        hasher.update(target.mode_flag.as_bytes());
    }
    hasher.update(attempt.to_string().as_bytes());
    hasher.update(feedback.as_bytes());
    format!("business-os-app-validation-{:x}", hasher.finalize())
}

fn enforce_business_os_app_validation_feedback_transition(
    root: &Path,
    message_key: &str,
    job: &QueuedPrompt,
    feedback: &str,
    attempt: usize,
) -> Result<String> {
    let db_path = crate::paths::core_db(root);
    let conn = channels::open_channel_db(&db_path)?;
    let route_status = channels::current_queue_route_status(&conn, message_key)
        .unwrap_or_else(|_| "leased".to_string());
    let from_state = queue_core_state_for_service(&route_status);
    let target = business_os_app_module_target_from_metadata(&job.queue_task_metadata);
    let mut metadata = BTreeMap::new();
    metadata.insert("validator_rework".to_string(), "true".to_string());
    metadata.insert(
        "validator_id".to_string(),
        "business_os_app_module_validator".to_string(),
    );
    metadata.insert("feedback_owner".to_string(), "main_agent".to_string());
    metadata.insert(
        "feedback_target_entity_id".to_string(),
        message_key.to_string(),
    );
    metadata.insert("spawns_review_owned_work".to_string(), "false".to_string());
    metadata.insert("validation_attempt".to_string(), attempt.to_string());
    if let Some(target) = target.as_ref() {
        metadata.insert("module_id".to_string(), target.module_id.clone());
        metadata.insert("module_mode".to_string(), target.mode_flag.to_string());
        metadata.insert(
            "artifact_directory".to_string(),
            target.artifact_directory.clone(),
        );
    }

    let proof = enforce_core_transition(
        &conn,
        &CoreTransitionRequest {
            entity_type: CoreEntityType::QueueItem,
            entity_id: message_key.to_string(),
            lane: RuntimeLane::P2MissionDelivery,
            from_state,
            to_state: CoreState::ReworkRequired,
            event: CoreEvent::RequireRework,
            actor: "ctox-business-os-app-validator".to_string(),
            evidence: CoreEvidenceRefs {
                review_audit_key: Some(business_os_app_validation_audit_key(
                    message_key,
                    target.as_ref(),
                    attempt,
                    feedback,
                )),
                ..CoreEvidenceRefs::default()
            },
            metadata,
        },
    )?;
    Ok(proof.proof_id)
}

fn apply_business_os_app_validation_rework_to_leased_queue(
    root: &Path,
    job: &QueuedPrompt,
    feedback: &str,
    summary: &str,
    attempt: usize,
    attempt_id: Option<&str>,
) -> Result<usize> {
    if let Some(attempt_id) = attempt_id {
        if channels::mark_worker_attempt_queue_effects_applied_if_status(
            root,
            attempt_id,
            &job.leased_message_keys,
            "review_rework",
        )? {
            return Ok(0);
        }
    }
    let updated = apply_review_feedback_to_leased_queue(root, job, feedback, summary)?;
    anyhow::ensure!(
        updated > 0,
        "Business OS app validation feedback had no leased queue task to update"
    );
    for message_key in &job.leased_message_keys {
        enforce_business_os_app_validation_feedback_transition(
            root,
            message_key,
            job,
            feedback,
            attempt,
        )
        .with_context(|| {
            format!("failed to record Business OS app validation rework proof for {message_key}")
        })?;
    }
    match attempt_id {
        Some(attempt_id) => channels::ack_leased_messages_for_attempt(
            root,
            attempt_id,
            &job.leased_message_keys,
            "review_rework",
            None,
        ),
        None => channels::ack_leased_messages(root, &job.leased_message_keys, "review_rework"),
    }
    .context("failed to mark Business OS app validation rework queue task(s)")?;
    Ok(updated)
}

fn complete_business_os_app_validation_success_to_leased_queue(
    root: &Path,
    job: &QueuedPrompt,
    reason: &str,
    attempt_id: Option<&str>,
) -> Result<usize> {
    anyhow::ensure!(
        !job.leased_message_keys.is_empty(),
        "Business OS app validation success had no leased queue task to update"
    );
    if let Some(attempt_id) = attempt_id {
        if channels::mark_worker_attempt_queue_effects_applied_if_status(
            root,
            attempt_id,
            &job.leased_message_keys,
            "handled",
        )? {
            return Ok(job.leased_message_keys.len());
        }
    }
    let expected_module_id = business_os_app_module_target_from_metadata(&job.queue_task_metadata)
        .map(|target| target.module_id);
    let mut fallback_message_keys = Vec::new();
    let mut updated = 0usize;
    for message_key in &job.leased_message_keys {
        let task = channels::load_queue_task(root, message_key)?.with_context(|| {
            format!("Business OS app validation queue task {message_key} disappeared")
        })?;
        if task.route_status == "review_rework" {
            release_business_os_app_validation_rework_to_pending(root, message_key)
                .with_context(|| {
                    format!(
                        "failed to requeue green Business OS app validation rework task {message_key} before completion"
                    )
                })?;
        }
        match crate::business_os::store::complete_business_command_from_app_validation_success(
            root,
            message_key,
            expected_module_id.as_deref(),
            reason,
        )
            .with_context(|| {
                format!(
                    "failed to complete Business OS app command after green validation for {message_key}"
                )
            })? {
            Some(_) => {
                let task = channels::load_queue_task(root, message_key)?.with_context(|| {
                    format!(
                        "Business OS app validation completed command but queue task {message_key} disappeared"
                    )
                })?;
                if task.route_status != "handled" {
                    channels::update_queue_task_with_terminal_policy_grant(
                        root,
                        channels::QueueTaskUpdateRequest {
                            message_key: message_key.clone(),
                            route_status: Some("handled".to_string()),
                            status_note: Some(
                                "business-os:terminal-success: app validation passed".to_string(),
                            ),
                            ..Default::default()
                        },
                        channels::TerminalPolicyGrant::business_os_app_validation_passed(),
                    )
                    .with_context(|| {
                        format!(
                            "failed to mark app-validation-verified queue task {message_key} handled after command completion"
                        )
                    })?;
                    crate::business_os::store::refresh_business_command_queue_task_projection(
                        root,
                        message_key,
                    )
                    .with_context(|| {
                        format!(
                            "failed to refresh Business OS app validation handled projection for {message_key}"
                        )
                    })?;
                }
                updated = updated.saturating_add(1);
            }
            None => fallback_message_keys.push(message_key.clone()),
        }
    }
    if !fallback_message_keys.is_empty() {
        for message_key in &fallback_message_keys {
            channels::update_queue_task_with_terminal_policy_grant(
                root,
                channels::QueueTaskUpdateRequest {
                    message_key: message_key.clone(),
                    route_status: Some("handled".to_string()),
                    status_note: Some(
                        "business-os:terminal-success: app validation passed".to_string(),
                    ),
                    ..Default::default()
                },
                channels::TerminalPolicyGrant::business_os_app_validation_passed(),
            )
            .with_context(|| {
                format!("failed to mark app-validation-verified queue task {message_key} handled")
            })?;
            updated = updated.saturating_add(1);
            crate::business_os::store::refresh_business_command_queue_task_projection(
                root,
                message_key,
            )
            .with_context(|| {
                format!(
                    "failed to refresh Business OS app validation handled projection for {message_key}"
                )
            })?;
        }
    }
    if let Some(attempt_id) = attempt_id {
        anyhow::ensure!(
            channels::mark_worker_attempt_queue_effects_applied_if_status(
                root,
                attempt_id,
                &job.leased_message_keys,
                "handled",
            )?,
            "Business OS app validation success did not bind handled queue effects to attempt {attempt_id}"
        );
    }
    Ok(updated)
}

fn enforce_business_os_app_validation_requeue_transition(
    root: &Path,
    message_key: &str,
) -> Result<String> {
    let db_path = crate::paths::core_db(root);
    let conn = channels::open_channel_db(&db_path)?;
    let route_status = channels::current_queue_route_status(&conn, message_key)
        .unwrap_or_else(|_| "review_rework".to_string());
    let from_state = queue_core_state_for_service(&route_status);
    if !matches!(from_state, CoreState::ReworkRequired) {
        anyhow::bail!(
            "queue item {message_key} is in route status {route_status}, not review_rework"
        );
    }

    let mut metadata = BTreeMap::new();
    metadata.insert("validator_rework".to_string(), "true".to_string());
    metadata.insert("validator_requeue".to_string(), "true".to_string());
    metadata.insert(
        "validator_id".to_string(),
        "business_os_app_module_validator".to_string(),
    );
    metadata.insert("feedback_owner".to_string(), "main_agent".to_string());
    metadata.insert(
        "feedback_target_entity_id".to_string(),
        message_key.to_string(),
    );
    metadata.insert("spawns_review_owned_work".to_string(), "false".to_string());

    let proof = enforce_core_transition(
        &conn,
        &CoreTransitionRequest {
            entity_type: CoreEntityType::QueueItem,
            entity_id: message_key.to_string(),
            lane: RuntimeLane::P2MissionDelivery,
            from_state,
            to_state: CoreState::Pending,
            event: CoreEvent::Retry,
            actor: "ctox-business-os-app-validator".to_string(),
            evidence: CoreEvidenceRefs {
                review_audit_key: Some(format!("business-os-app-validation-requeue:{message_key}")),
                ..CoreEvidenceRefs::default()
            },
            metadata,
        },
    )?;
    Ok(proof.proof_id)
}

fn business_os_app_module_validation_feedback(
    root: &Path,
    job: &QueuedPrompt,
) -> Result<Option<String>> {
    let Some(target) = business_os_app_module_target_from_metadata(&job.queue_task_metadata) else {
        return Ok(None);
    };
    let app_workspace_root = business_os_app_workspace_root(root, job);
    let script = app_workspace_root.join("src/apps/business-os/scripts/validate-app-module.mjs");
    if !script.exists() {
        return Ok(Some(render_business_os_app_module_validation_feedback(
            job,
            &target,
            &format!(
                "Business OS app artifact validator is missing at {}. The app command cannot be marked complete until the release image includes this validator.",
                script.display()
            ),
        )));
    }
    let mut command =
        Command::new(crate::service::business_os::resolve_business_os_validator_node(root));
    command
        .arg(&script)
        .arg(&target.module_id)
        .arg(target.mode_flag)
        .arg("--workspace")
        .arg(&app_workspace_root);
    let output = match command_output_with_timeout(
        &mut command,
        Duration::from_secs(90),
        "Business OS app artifact validator",
    ) {
        Ok(output) => output,
        Err(err) => {
            return Ok(Some(render_business_os_app_module_validation_feedback(
                job,
                &target,
                &format!("Business OS app artifact validator could not run: {err}"),
            )));
        }
    };
    if output.status.success() {
        let import_source_kind = job
            .queue_task_metadata
            .get("business_os_import_source_kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if import_source_kind.is_empty() {
            return Ok(None);
        }
        if let Err(err) = crate::business_os::store::write_module_catalog_projection_to_rxdb(root) {
            return Ok(Some(render_business_os_app_module_validation_feedback(
                job,
                &target,
                &format!("App catalog projection before browser smoke failed: {err:#}"),
            )));
        }
        let smoke_args = vec!["--installed".to_string(), "--json".to_string()];
        let smoke = match super::business_os_app_testing::run_business_os_app_smoke(
            root,
            &target.module_id,
            &smoke_args,
        ) {
            Ok(output) => output,
            Err(err) => {
                return Ok(Some(render_business_os_app_module_validation_feedback(
                    job,
                    &target,
                    &format!("Business OS app browser smoke could not run: {err:#}"),
                )));
            }
        };
        if smoke.status.success() {
            let command_id = job
                .queue_task_metadata
                .get("business_os_command_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if let Err(err) = crate::business_os::store::record_business_os_app_import_smoke_success(
                root,
                command_id,
                &target.module_id,
                &smoke.stdout,
            ) {
                return Ok(Some(render_business_os_app_module_validation_feedback(
                    job,
                    &target,
                    &format!(
                        "Business OS app browser smoke evidence could not be persisted: {err:#}"
                    ),
                )));
            }
            return Ok(None);
        }
        let stderr = String::from_utf8_lossy(&smoke.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&smoke.stdout).trim().to_string();
        let report = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!(
                "Business OS app browser smoke exited with status {} and no output.",
                smoke.status
            )
        };
        return Ok(Some(render_business_os_app_module_validation_feedback(
            job, &target, &report,
        )));
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let report = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!(
            "Business OS app artifact validator exited with status {} and no output.",
            output.status
        )
    };
    Ok(Some(render_business_os_app_module_validation_feedback(
        job, &target, &report,
    )))
}

fn render_business_os_app_module_validation_feedback(
    job: &QueuedPrompt,
    target: &BusinessOsAppModuleTarget,
    report: &str,
) -> String {
    let next_attempt = business_os_app_validation_repair_attempt_count(&job.prompt)
        .saturating_add(1)
        .min(BUSINESS_OS_APP_VALIDATION_MAX_REPAIR_ATTEMPTS);
    let report = clip_multiline_text(
        &redact_business_os_app_prompt_secrets(report.trim()),
        BUSINESS_OS_APP_VALIDATION_REPORT_MAX_CHARS,
    );
    let request_summary = business_os_app_original_request_summary(&job.prompt);
    let feedback = format!(
        "{BUSINESS_OS_APP_VALIDATION_FAILURE_MARKER}\n\nValidation repair attempt: {} of {}\n\nTask source: {}\n\nBusiness OS app task metadata:\n- module_id: {}\n- install_target: {}\n- app_directory: {}\n- resource.module_contract: src/skills/system/product_engineering/business-os-app-module-development/references/module-contract.md\n- resource.dos_and_donts: src/skills/system/product_engineering/business-os-app-module-development/references/dos-and-donts.md\n- resource.green_checklist: src/skills/system/product_engineering/business-os-app-module-development/references/green-checklist.md\n- resource.architecture_translation: src/skills/system/product_engineering/business-os-app-module-development/references/architecture-translation.md\n- validation: ctox business-os app validate {} {}\n\nWhat to do now:\nAfter the required plan update, make app_directory exist in the first authoring tool call and create or repair the validator-required vanilla HTML/CSS/JS files immediately. This is an implementation repair, not another discovery pass: do not reread the skill, source tree, or reference catalog before the target exists and the validation command has run once. Keep the same Business OS app request active; do not edit skill/resource files or service lifecycle state.\n\nValidator report:\n{}\n\nOriginal app request summary:\n{}",
        next_attempt,
        BUSINESS_OS_APP_VALIDATION_MAX_REPAIR_ATTEMPTS,
        job.source_label,
        target.module_id,
        target.install_target,
        target.artifact_directory,
        target.module_id,
        target.mode_flag,
        report,
        request_summary,
    );
    clip_multiline_text(&feedback, BUSINESS_OS_APP_VALIDATION_FEEDBACK_MAX_CHARS)
}

fn business_os_app_original_request_summary(prompt: &str) -> String {
    for marker in [
        "Original app request summary:",
        "Business OS app request summary:",
    ] {
        if let Some(summary) = prompt_section_after_marker(prompt, marker) {
            let summary = redact_business_os_app_prompt_secrets(summary.trim());
            if !summary.trim().is_empty() {
                return clip_multiline_text(
                    summary.trim(),
                    BUSINESS_OS_APP_VALIDATION_REQUEST_MAX_CHARS,
                );
            }
        }
    }

    let mut lines = Vec::new();
    for line in prompt.lines() {
        let trimmed = line.trim_end();
        if trimmed.starts_with("Business OS task resources:")
            || trimmed.starts_with("Business OS app task metadata:")
            || trimmed.starts_with("Business OS command:")
            || trimmed.starts_with("Payload JSON:")
            || trimmed.starts_with("Client context JSON:")
        {
            break;
        }
        if lines.is_empty() && trimmed.trim().is_empty() {
            continue;
        }
        lines.push(trimmed);
    }
    let summary = lines.join("\n");
    let summary = redact_business_os_app_prompt_secrets(summary.trim());
    if summary.trim().is_empty() {
        return "Use the app metadata above and the persisted Business OS command record if more request detail is needed.".to_string();
    }
    clip_multiline_text(summary.trim(), BUSINESS_OS_APP_VALIDATION_REQUEST_MAX_CHARS)
}

fn prompt_section_after_marker<'a>(prompt: &'a str, marker: &str) -> Option<&'a str> {
    let rest = prompt.split_once(marker)?.1;
    let section = rest
        .split("\nBusiness OS command:")
        .next()
        .unwrap_or(rest)
        .split("\nBusiness OS app task metadata:")
        .next()
        .unwrap_or(rest)
        .split("\nPayload JSON:")
        .next()
        .unwrap_or(rest)
        .split("\nClient context JSON:")
        .next()
        .unwrap_or(rest)
        .split("\nValidator report:")
        .next()
        .unwrap_or(rest)
        .split("\nWhat to do now:")
        .next()
        .unwrap_or(rest)
        .split(BUSINESS_OS_APP_VALIDATION_FAILURE_MARKER)
        .next()
        .unwrap_or(rest);
    Some(section)
}

fn redact_business_os_app_prompt_secrets(value: &str) -> String {
    let mut redacted = Vec::new();
    for line in value.lines() {
        let lowered = line.to_ascii_lowercase();
        if lowered.contains("capability_token")
            || lowered.contains("authorization")
            || lowered.contains("cookie")
            || lowered.contains("set-cookie")
            || lowered.contains("bearer ")
        {
            redacted.push("[redacted sensitive Business OS context line]".to_string());
        } else {
            redacted.push(line.to_string());
        }
    }
    redacted.join("\n")
}

fn clip_multiline_text(value: &str, max_chars: usize) -> String {
    let value = value.trim();
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut clipped = value
        .chars()
        .take(max_chars.saturating_sub(56))
        .collect::<String>();
    clipped.push_str("\n... truncated; full record is persisted in CTOX ...");
    clipped
}

fn is_business_os_chat_queue_job(root: &Path, job: &QueuedPrompt) -> bool {
    if job.prompt.contains("business_os.chat.task") {
        return true;
    }
    let db_path = crate::paths::core_db(root);
    let Ok(conn) = Connection::open(db_path) else {
        return false;
    };
    job.leased_message_keys.iter().any(|message_key| {
        conn.query_row(
            "SELECT metadata_json FROM communication_messages WHERE message_key = ?1",
            params![message_key],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|metadata| {
            metadata
                .get("business_os_command_type")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .as_deref()
            == Some("business_os.chat.task")
    })
}
