// Run execution: the outcome orchestrator, the sandboxed runner spawn
// (env-clear + allowlist, process-group kill), the per-target run lock
// with holder-liveness reclaim, and the portal health probe.
use super::classify::{classify_outcome, Classification, ScrapeRunStatus};
use super::registry::{load_registered_target, open_db};
use super::{
    artifact_record, bind_scrape_record_provenance, build_repair_prompt, build_run_artifacts,
    contains_human_verification, default_entry_command, emit_reauthorization_handoff,
    extracted_record_fields, find_flag_value, latest_source_revision_map, load_last_successful_run,
    materialize_latest_records, maybe_record_template_from_target, maybe_run_llm_enrichment,
    normalize_records, now_iso_string, parse_execution_payload, print_json, probe_to_json,
    read_response_excerpt, record_run, repair_skill_for_status, required_flag_value,
    resolve_workspace_dir, scrape_error_diagnostic, session_expiry_reauthorization, stable_digest,
    tail_excerpt, target_sources, url_host_lower, write_repair_request, RecordRunRequest,
    RegisteredTarget, ScrapeExecutionOutcome, DEFAULT_QUEUE_PRIORITY, SCRAPE_RUNNER_ENV_ALLOWLIST,
};
use crate::channels;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::fs::OpenOptions;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::id as process_id;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
#[derive(Debug, Clone)]
pub(super) struct ProbeResult {
    pub(super) reachable: bool,
    pub(super) status_code: Option<u16>,
    pub(super) final_url: String,
    pub(super) human_verification: bool,
    pub(super) error: Option<String>,
}

#[derive(Debug)]
pub(super) struct CommandExecution {
    pub(super) exit_code: Option<i32>,
    pub(super) timed_out: bool,
    pub(super) stdout_text: String,
    pub(super) stderr_text: String,
}

pub(super) struct TargetRunLock {
    path: PathBuf,
}

impl Drop for TargetRunLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub(super) fn execute_scrape(root: &Path, args: &[String]) -> Result<()> {
    let outcome = execute_scrape_with_outcome(root, args)?;
    print_json(&serde_json::to_value(outcome)?)
}

pub(crate) fn execute_scrape_with_outcome(
    root: &Path,
    args: &[String],
) -> Result<ScrapeExecutionOutcome> {
    let execution_started = Instant::now();
    let target_key = required_flag_value(args, "--target-key")
        .context("usage: ctox scrape execute --target-key <key> [--trigger-kind <manual|scheduled|repair>] [--scheduled-for <iso>] [--timeout-seconds <n>] [--runtime-root <path>] [--allow-heal] [--input-json <text>] [--input-file <path>] [--thread-key <key>] [--owner-user-id <id>] [--queue-priority <urgent|high|normal|low>]")?;
    let trigger_kind = find_flag_value(args, "--trigger-kind").unwrap_or("manual");
    let owner_user_id = find_flag_value(args, "--owner-user-id")
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let timeout_seconds = find_flag_value(args, "--timeout-seconds")
        .map(|value| value.parse::<u64>())
        .transpose()
        .context("failed to parse --timeout-seconds")?
        .unwrap_or(120);
    let allow_heal = args.iter().any(|arg| arg == "--allow-heal");
    let scheduled_for = find_flag_value(args, "--scheduled-for").map(ToOwned::to_owned);
    // Caller-supplied dynamic input forwarded to the script as
    // CTOX_SCRAPE_INPUT_JSON. Lets one registered target serve per-call
    // queries (e.g. person-research handing the company name to a Northdata
    // extractor) without registering a new target per query.
    let input_json: Option<String> = if let Some(text) = find_flag_value(args, "--input-json") {
        Some(text.to_string())
    } else if let Some(path) = find_flag_value(args, "--input-file") {
        Some(
            fs::read_to_string(path)
                .with_context(|| format!("failed to read --input-file {path}"))?,
        )
    } else {
        None
    };
    if let Some(text) = &input_json {
        serde_json::from_str::<Value>(text)
            .context("--input-json / --input-file must be valid JSON")?;
    }
    let conn = open_db(root)?;
    let target =
        load_registered_target(root, &conn, target_key)?.context("target_key not found")?;
    let workspace_dir = resolve_workspace_dir(root, &target.view.workspace_dir);
    let _run_lock = acquire_target_run_lock(&workspace_dir, target_key)?;
    let run_started_at = now_iso_string();
    let run_id = format!(
        "scrape_run-{}",
        stable_digest(&format!(
            "{}:{}:{}:{}",
            target.view.target_key,
            trigger_kind,
            scheduled_for.as_deref().unwrap_or(""),
            run_started_at
        ))
    );
    let run_dir = workspace_dir.join("runs").join(&run_id);
    let output_dir = run_dir.join("outputs");
    fs::create_dir_all(&output_dir).with_context(|| {
        format!(
            "failed to create scrape output dir {}",
            output_dir.display()
        )
    })?;

    let probe = probe_portal_health(
        target
            .view
            .config
            .get("probe_url")
            .and_then(Value::as_str)
            .unwrap_or(&target.view.start_url),
        target
            .view
            .config
            .get("skip_probe")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    );

    let execution = execute_registered_script(
        &target,
        &run_dir,
        &output_dir,
        timeout_seconds,
        input_json.as_deref(),
        owner_user_id,
    )?;
    let payload = match parse_execution_payload(&execution.stdout_text) {
        Ok(value) => value,
        Err(error) => json!({
            "failure_mode": "portal_drift",
            "parse_error": true,
            "detail": error.to_string(),
        }),
    };
    let mut records = normalize_records(&payload);
    if let Some(items) = records.as_mut() {
        bind_scrape_record_provenance(items, &run_id, input_json.as_deref(), &now_iso_string());
    }
    let expected_min_records = target
        .view
        .config
        .get("expected_min_records")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let records_found = records
        .as_ref()
        .map(|items| items.len() as i64)
        .unwrap_or(0);
    let mut classification = classify_outcome(
        &payload,
        &probe,
        &execution,
        records_found,
        expected_min_records,
    );
    let mut continuation = None;
    if payload.get("failure_mode").and_then(Value::as_str) == Some("awaiting_provider")
        || payload.get("continuation").is_some()
    {
        match super::continuation::validate_provider_continuation(
            &payload,
            &run_id,
            &target.view.target_key,
            &target.view.config,
            input_json.as_deref(),
            &execution,
        ) {
            Ok(receipt) => {
                continuation = Some(serde_json::to_value(receipt)?);
                classification = Classification {
                    status: ScrapeRunStatus::AwaitingProvider,
                    should_queue_repair: false,
                    reason: "current_provider_collection_pending".to_string(),
                };
            }
            Err(_) => {
                classification = Classification {
                    status: ScrapeRunStatus::PortalDrift,
                    should_queue_repair: false,
                    reason: "invalid_provider_continuation".to_string(),
                };
            }
        }
    }
    // Capability 10: an expired/invalid session on a credential-protected
    // source lands on the source's own login page. That is not portal drift —
    // upgrade the classification and persist the precise reauthorization
    // action so the typed auth-assist handoff can fire below.
    let reauthorization =
        session_expiry_reauthorization(&target, &probe, &payload, &classification);
    if reauthorization.is_some() && classification.status != ScrapeRunStatus::AuthorizationRequired
    {
        let host = url_host_lower(&probe.final_url).unwrap_or_default();
        classification = Classification {
            status: ScrapeRunStatus::AuthorizationRequired,
            should_queue_repair: true,
            reason: format!("session_expired_login_landing:{host}"),
        };
    }
    let mut query_completion = None;
    let mut query_evidence_bytes = None;
    if payload.get("query_completion").is_some() {
        let validation = super::query_completion::validate_query_completion(
            &payload,
            &run_dir,
            &run_id,
            &target.view.target_key,
            &target.view.start_url,
            input_json.as_deref(),
            &probe,
            &execution,
        );
        match validation {
            Ok((receipt, bytes))
                if classification.reason == "empty_record_set_on_reachable_portal"
                    && reauthorization.is_none() =>
            {
                query_completion = Some(serde_json::to_value(receipt)?);
                query_evidence_bytes = Some(bytes);
                classification = Classification {
                    status: ScrapeRunStatus::CompletedEmpty,
                    should_queue_repair: false,
                    reason: "current_query_completed_without_matches".to_string(),
                };
            }
            Err(error)
                if classification.reason == "empty_record_set_on_reachable_portal"
                    || classification.status == ScrapeRunStatus::Succeeded =>
            {
                classification = Classification {
                    status: ScrapeRunStatus::PortalDrift,
                    should_queue_repair: true,
                    reason: format!("invalid_query_completion:{error}"),
                };
            }
            // Existing authentication, probe, timeout and partial-output failures
            // outrank a receipt and retain their existing recovery disposition.
            _ => {}
        }
    }
    let run_finished_at = now_iso_string();
    let default_schema_key = target
        .view
        .output_schema
        .get("schema_key")
        .and_then(Value::as_str);
    let enrichment = match records.as_deref() {
        Some(items) if classification.status == ScrapeRunStatus::Succeeded => {
            Some(maybe_run_llm_enrichment(root, &target, items, &output_dir)?)
        }
        _ => None,
    };
    let materialized_records = enrichment
        .as_ref()
        .map(|outcome| outcome.records.as_slice())
        .or(records.as_deref());
    let materialization = if classification.status == ScrapeRunStatus::Succeeded {
        materialized_records
            .map(|items| {
                materialize_latest_records(
                    &conn,
                    &target,
                    &run_id,
                    &run_finished_at,
                    items,
                    &output_dir,
                    default_schema_key,
                )
            })
            .transpose()?
    } else {
        None
    };
    let last_successful_run = load_last_successful_run(&conn, &target.view.target_id)?;
    let source_revision_map = latest_source_revision_map(&conn, &target.view.target_id)?
        .into_values()
        .collect::<Vec<_>>();
    let repair_request_path = if classification.should_queue_repair {
        Some(write_repair_request(
            &conn,
            &run_dir,
            &target,
            classification.status,
            &classification.reason,
            &probe,
            &execution,
            records_found,
            last_successful_run.as_ref(),
            materialization.as_ref(),
            reauthorization.as_ref(),
        )?)
    } else {
        None
    };
    // Typed auth-assist handoff for expired/invalid protected sessions. This
    // fires whenever the classification is `authorization_required` — it is a
    // request for human authorization, not an automated heal, so it is not
    // gated on --allow-heal (the adapter scripts emit the same request
    // ungated when they detect the auth wall themselves).
    let auth_assist_handoff = if classification.status == ScrapeRunStatus::AuthorizationRequired {
        reauthorization.as_ref().and_then(|action| {
            emit_reauthorization_handoff(
                root,
                &run_id,
                find_flag_value(args, "--thread-key"),
                owner_user_id,
                action,
            )
        })
    } else {
        None
    };
    let failure_mode = if classification.status == ScrapeRunStatus::AuthorizationRequired {
        Some("authorization_required")
    } else {
        payload.get("failure_mode").and_then(Value::as_str)
    };
    let browser_assist_requested = payload
        .get("browser_assist_requested")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || auth_assist_handoff.is_some();
    let mut artifacts = build_run_artifacts(
        &run_dir,
        &output_dir,
        &payload,
        records.as_deref(),
        &execution,
        default_schema_key,
    )?;
    if let Some(materialization) = &materialization {
        artifacts.push(materialization.delta_artifact.clone());
    }
    if let Some(enrichment) = &enrichment {
        artifacts.extend(enrichment.artifacts.clone());
    }
    if let Some(bytes) = query_evidence_bytes {
        let path = output_dir.join("verified-query-evidence.json");
        fs::write(&path, bytes)?;
        artifacts.push(artifact_record(
            "query_completion_evidence",
            &path,
            Some("ctox.scrape.query_completion.v1"),
            Some(0),
        )?);
    }
    record_run(
        root,
        &conn,
        RecordRunRequest {
            run_id: run_id.clone(),
            target: &target.view,
            trigger_kind: trigger_kind.to_string(),
            scheduled_for: scheduled_for.clone(),
            started_at: run_started_at.clone(),
            finished_at: run_finished_at.clone(),
            status: classification.status.as_str().to_string(),
            script_revision_no: Some(target.script.revision_no),
            script_sha256: Some(target.script.script_sha256.clone()),
            run_context: json!({
                "probe": probe_to_json(&probe),
                "sources": target_sources(&target.view),
                "source_modules": source_revision_map,
                "reason": classification.reason,
                "enrichment": enrichment.as_ref().map(|item| item.summary.clone()),
                "repair_request_path": repair_request_path.as_ref().map(|path| path.to_string_lossy().to_string()),
                "reauthorization": reauthorization.clone().unwrap_or(Value::Null),
                "last_successful_run": last_successful_run,
            }),
            result: json!({
                "continuation": continuation,
                "records_found": records_found,
                "query_completion": query_completion,
                "enriched_records_found": materialized_records.map(|items| items.len() as i64),
                "source_count": target_sources(&target.view).len(),
                "failure_mode": failure_mode,
                "detail": payload.get("detail").cloned().unwrap_or(Value::Null),
                "browser_assist_requested": browser_assist_requested,
                "reauthorization": reauthorization.clone().unwrap_or(Value::Null),
                "stdout_excerpt": tail_excerpt(&execution.stdout_text, 4000),
                "stderr_excerpt": tail_excerpt(&execution.stderr_text, 4000),
                "timed_out": execution.timed_out,
                "exit_code": execution.exit_code,
                "enrichment": enrichment.as_ref().map(|item| item.summary.clone()),
                "materialization": materialization.as_ref().map(|item| item.summary.clone()),
            }),
            output_dir: run_dir.clone(),
            artifacts: artifacts.clone(),
        },
    )?;

    let template_event = if classification.status == ScrapeRunStatus::Succeeded {
        maybe_record_template_from_target(root, &target, records_found)?
    } else {
        None
    };

    let repair_queue_task = if classification.status == ScrapeRunStatus::AuthorizationRequired {
        // The reauthorization handoff IS the queued action for an expired
        // session; a generic script-repair task would be the wrong action.
        auth_assist_handoff.clone()
    } else if allow_heal && classification.should_queue_repair {
        let thread_key = find_flag_value(args, "--thread-key")
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("scrape/{}", target.view.target_key));
        let priority = find_flag_value(args, "--queue-priority").unwrap_or(DEFAULT_QUEUE_PRIORITY);
        let repair_workspace = resolve_workspace_dir(root, &target.view.workspace_dir);
        let repair_prompt = build_repair_prompt(
            &repair_workspace,
            &target,
            &run_id,
            classification.status,
            records_found,
            repair_request_path.as_ref(),
        );
        let suggested_skill = repair_skill_for_status(classification.status);
        Some(serde_json::to_value(
            channels::create_scrape_repair_queue_task(
                root,
                "scrape",
                &target.view.target_key,
                channels::QueueTaskCreateRequest {
                    title: format!("repair scrape target {}", target.view.target_key),
                    prompt: repair_prompt,
                    thread_key,
                    workspace_root: Some(repair_workspace.to_string_lossy().into_owned()),
                    priority: priority.to_string(),
                    suggested_skill: Some(suggested_skill.to_string()),
                    parent_message_key: None,
                    extra_metadata: Some(json!({
                        "scrape_repair": {
                            "target_key": target.view.target_key,
                            "run_id": run_id,
                            "workspace_root": repair_workspace,
                        }
                    })),
                },
            )?,
        )?)
    } else {
        None
    };

    let fields_extracted = extracted_record_fields(records.as_deref());
    let error = scrape_error_diagnostic(&classification, &payload, &probe, &execution);
    // `ok` reflects the classification: a blocked/drifted/unreachable run
    // must not report success next to a populated `error` field. Partial
    // output still delivered records, so it counts as ok-with-reason.
    let run_ok = matches!(
        classification.status,
        ScrapeRunStatus::Succeeded
            | ScrapeRunStatus::CompletedEmpty
            | ScrapeRunStatus::PartialOutput
    );
    Ok(ScrapeExecutionOutcome {
        ok: run_ok,
        target_key: target.view.target_key,
        run_id,
        status: classification.status,
        continuation,
        records_found,
        fields_extracted,
        latency_ms: execution_started
            .elapsed()
            .as_millis()
            .min(u64::MAX as u128) as u64,
        reason: classification.reason,
        error,
        query_completion,
        probe: probe_to_json(&probe),
        should_queue_repair: classification.should_queue_repair,
        repair_request_path: repair_request_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
        repair_queue_task,
        reauthorization,
        template_event,
        materialization: materialization.as_ref().map(|item| item.summary.clone()),
        run_manifest_path: run_dir.join("run.json"),
    })
}

pub(super) fn is_preserved_runner_env_key(key: &str) -> bool {
    SCRAPE_RUNNER_ENV_ALLOWLIST
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(key))
}

/// Kill a timed-out runner and (on unix) its whole process group, so descendant
/// processes such as Chromium are not orphaned. The child was spawned with
/// `process_group(0)`, making it the group leader (pgid == child pid).
pub(super) fn kill_runner_process_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let pid = child.id() as libc::pid_t;
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}

pub(super) fn execute_registered_script(
    target: &RegisteredTarget,
    run_dir: &Path,
    output_dir: &Path,
    timeout_seconds: u64,
    input_json: Option<&str>,
    owner_user_id: Option<&str>,
) -> Result<CommandExecution> {
    let sources = target_sources(&target.view);
    let mut command_parts = target.script.entry_command.clone();
    if command_parts.is_empty() {
        command_parts = default_entry_command(&target.script.language);
    }
    let materialized = command_parts
        .into_iter()
        .map(|part| part.replace("{script_path}", &target.script.script_path))
        .collect::<Vec<_>>();
    let executable = materialized
        .first()
        .cloned()
        .context("empty scrape script command")?;
    let args = materialized.into_iter().skip(1).collect::<Vec<_>>();

    let mut child = Command::new(&executable);
    // Trust boundary: the runner body is untrusted (hot-revisable / auto-heal
    // LLM-rewritten). Start from an empty environment and re-add only the
    // allow-listed host vars, so the script cannot read daemon secrets via the
    // environment. CTOX-specific inputs are passed explicitly below.
    child.env_clear();
    for (key, value) in std::env::vars_os() {
        if key.to_str().is_some_and(is_preserved_runner_env_key) {
            child.env(&key, &value);
        }
    }
    child
        .args(&args)
        .current_dir(&target.workspace_root)
        .env("CTOX_SCRAPE_TARGET_KEY", &target.view.target_key)
        .env(
            "CTOX_SCRAPE_TARGET_DIR",
            target.workspace_root.to_string_lossy().to_string(),
        )
        .env(
            "CTOX_SCRAPE_MANIFEST_PATH",
            target
                .workspace_root
                .join("manifest.json")
                .to_string_lossy()
                .to_string(),
        )
        .env("CTOX_SCRAPE_RUN_DIR", run_dir.to_string_lossy().to_string())
        .env(
            "CTOX_SCRAPE_OUTPUT_DIR",
            output_dir.to_string_lossy().to_string(),
        )
        .env("CTOX_SCRAPE_START_URL", &target.view.start_url)
        .env(
            "CTOX_SCRAPE_SOURCES_JSON",
            serde_json::to_string(&sources).unwrap_or_else(|_| "[]".to_string()),
        )
        .env(
            "CTOX_SCRAPE_SOURCES_MANIFEST_PATH",
            target
                .workspace_root
                .join("sources")
                .join("sources_manifest.json")
                .to_string_lossy()
                .to_string(),
        )
        .env(
            "CTOX_SCRAPE_SOURCES_DIR",
            target
                .workspace_root
                .join("sources")
                .to_string_lossy()
                .to_string(),
        );
    // Hand the script the exact ctox binary that's running this scrape
    // execute, so nested shell-outs (`ctox web search`, `ctox web read`,
    // `ctox secret get`) hit the same code-base instead of a different
    // PATH-resolved binary.
    if let Ok(self_exe) = std::env::current_exe() {
        child.env("CTOX_BIN", self_exe.to_string_lossy().to_string());
    }
    if let Some(text) = input_json {
        child.env("CTOX_SCRAPE_INPUT_JSON", text);
    }
    if let Some(owner_user_id) = owner_user_id {
        child.env("CTOX_OWNER_USER_ID", owner_user_id);
    }
    child.stdout(Stdio::piped()).stderr(Stdio::piped());
    // Own process group so a timeout kills the whole runner tree (node/bash plus
    // any Playwright/Chromium children), not just the direct parent.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        child.process_group(0);
    }

    let mut child = child
        .spawn()
        .with_context(|| format!("failed to spawn scrape command {executable}"))?;
    let stdout = child
        .stdout
        .take()
        .context("failed to capture scrape stdout")?;
    let stderr = child
        .stderr
        .take()
        .context("failed to capture scrape stderr")?;

    let stdout_handle = thread::spawn(move || -> std::io::Result<Vec<u8>> {
        let mut reader = std::io::BufReader::new(stdout);
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        Ok(buf)
    });
    let stderr_handle = thread::spawn(move || -> std::io::Result<Vec<u8>> {
        let mut reader = std::io::BufReader::new(stderr);
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        Ok(buf)
    });

    let started = Instant::now();
    let mut timed_out = false;
    let exit_code = loop {
        if let Some(status) = child.try_wait()? {
            break status.code();
        }
        if started.elapsed() >= Duration::from_secs(timeout_seconds) {
            timed_out = true;
            kill_runner_process_tree(&mut child);
            let status = child.wait()?;
            break status.code();
        }
        thread::sleep(Duration::from_millis(50));
    };

    let stdout_bytes = stdout_handle
        .join()
        .map_err(|_| anyhow::anyhow!("stdout capture thread panicked"))??;
    let stderr_bytes = stderr_handle
        .join()
        .map_err(|_| anyhow::anyhow!("stderr capture thread panicked"))??;

    Ok(CommandExecution {
        exit_code,
        timed_out,
        stdout_text: String::from_utf8_lossy(&stdout_bytes).to_string(),
        stderr_text: String::from_utf8_lossy(&stderr_bytes).to_string(),
    })
}

pub(super) fn acquire_target_run_lock(
    workspace_dir: &Path,
    target_key: &str,
) -> Result<TargetRunLock> {
    fs::create_dir_all(workspace_dir)?;
    let path = workspace_dir.join(".run.lock");
    for attempt in 0..2 {
        match OpenOptions::new().create_new(true).write(true).open(&path) {
            Ok(mut file) => {
                use std::io::Write;
                writeln!(
                    file,
                    "{}",
                    serde_json::to_string(&json!({
                        "target_key": target_key,
                        "pid": process_id(),
                        "created_at": now_iso_string(),
                    }))?
                )?;
                return Ok(TargetRunLock { path });
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists && attempt == 0 => {
                // A lock left behind by a crashed/killed executor blocked the
                // target forever ("already has an active run" until manual
                // deletion). Probe the recorded pid; only a live holder keeps
                // the lock. (Pid reuse can false-keep a stale lock — the
                // benign direction.)
                if run_lock_holder_is_alive(&path) {
                    anyhow::bail!("scrape target `{target_key}` already has an active run");
                }
                let _ = fs::remove_file(&path);
            }
            Err(err) => {
                return Err(err).with_context(|| {
                    format!("scrape target `{target_key}` already has an active run")
                });
            }
        }
    }
    anyhow::bail!("scrape target `{target_key}` already has an active run");
}

pub(super) fn run_lock_holder_is_alive(path: &Path) -> bool {
    let Some(pid) = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(text.trim()).ok())
        .and_then(|value| value.get("pid").and_then(Value::as_i64))
    else {
        // Unreadable or garbled lock file: nobody provably holds it.
        return false;
    };
    if pid <= 0 {
        return false;
    }
    process_is_alive(pid)
}

#[cfg(unix)]
fn process_is_alive(pid: i64) -> bool {
    if unsafe { libc::kill(pid as libc::pid_t, 0) } == 0 {
        return true;
    }
    // EPERM: the process exists but belongs to someone else — still alive.
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(windows)]
fn process_is_alive(pid: i64) -> bool {
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_ACCESS_DENIED, STILL_ACTIVE,
    };
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let Ok(pid) = u32::try_from(pid) else {
        return false;
    };
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        // Match the Unix EPERM behavior: an inaccessible process still exists
        // and must retain its lock.
        return unsafe { GetLastError() } == ERROR_ACCESS_DENIED;
    }
    let mut exit_code = 0u32;
    let queried = unsafe { GetExitCodeProcess(process, &mut exit_code) } != 0;
    unsafe {
        CloseHandle(process);
    }
    queried && i32::try_from(exit_code).ok() == Some(STILL_ACTIVE)
}

#[cfg(not(any(unix, windows)))]
fn process_is_alive(_pid: i64) -> bool {
    false
}

pub(super) fn probe_portal_health(url: &str, skip_probe: bool) -> ProbeResult {
    if skip_probe {
        return ProbeResult {
            reachable: true,
            status_code: Some(200),
            final_url: url.to_string(),
            human_verification: false,
            error: None,
        };
    }
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(20))
        .build();
    match agent
        .get(url)
        .set("User-Agent", "Mozilla/5.0 CTOX scraping probe")
        .call()
    {
        Ok(response) => {
            let status_code = response.status();
            let final_url = response.get_url().to_string();
            let content_type = response.header("content-type").unwrap_or("").to_lowercase();
            let body_text = if content_type.contains("html") {
                read_response_excerpt(response)
            } else {
                String::new()
            };
            ProbeResult {
                reachable: status_code < 500,
                status_code: Some(status_code),
                final_url,
                human_verification: contains_human_verification(&body_text),
                error: None,
            }
        }
        Err(ureq::Error::Status(code, response)) => {
            let final_url = response.get_url().to_string();
            let content_type = response.header("content-type").unwrap_or("").to_lowercase();
            let body_text = if content_type.contains("html") {
                read_response_excerpt(response)
            } else {
                String::new()
            };
            ProbeResult {
                reachable: code < 500,
                status_code: Some(code),
                final_url,
                human_verification: contains_human_verification(&body_text),
                error: Some(format!("HTTPError: status_{code}")),
            }
        }
        Err(ureq::Error::Transport(error)) => ProbeResult {
            reachable: false,
            status_code: None,
            final_url: url.to_string(),
            human_verification: false,
            error: Some(format!("TransportError: {error}")),
        },
    }
}
