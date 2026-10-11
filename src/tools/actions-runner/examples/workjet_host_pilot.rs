//! Native pilot, invoked only inside the admitted GPU lane.
//! No daemon, credential access or action downloads. Not the distributed service.
use anyhow::{anyhow, bail, Context, Result};
use ctox_actions_runner::{
    common::context::{Level, LogSink},
    container::{env_file::parse_env_text, HostEnvironment},
    expr::{StatusProvider, Value},
    host::{prepare_run_step, HostGapKind, HostWorkflow},
    model::{GitLookups, StepResult, StepStatus},
    runner::{
        run_context::RunContext,
        step::{
            is_continue_on_error, is_step_enabled, new_step_expression_evaluator_with_env,
            setup_env, StepStage,
        },
        step_executor::file_command_env,
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};
#[derive(Deserialize)]
struct Config {
    source: PathBuf,
    build_root: PathBuf,
    cache_root: PathBuf,
    core_source: PathBuf,
    source_sha: String,
    core_sha: String,
    engine_sha: String,
    repository: String,
    pr: u64,
    node_bin: PathBuf,
    step_timeout_seconds: u64,
    #[serde(default)]
    trusted_snapshot: bool,
    #[serde(default)]
    allowed_workflows: Vec<String>,
}
#[derive(Serialize)]
struct Report {
    repository: String,
    source_sha: String,
    core_sha: String,
    engine_sha: String,
    pr: u64,
    node: String,
    host: String,
    workflows: Vec<WorkflowReport>,
}
#[derive(Serialize)]
struct WorkflowReport {
    workflow: String,
    workflow_sha256: String,
    status: String,
    reason: String,
    jobs: Vec<JobReport>,
}
#[derive(Serialize)]
struct JobReport {
    job: String,
    status: String,
    reason: String,
    steps: Vec<StepReport>,
    duration_ms: u128,
}
#[derive(Serialize)]
struct StepReport {
    step: String,
    status: String,
    reason: String,
    exit_code: Option<i32>,
    duration_ms: u128,
    log: Option<String>,
    excerpt: String,
}
struct Status(bool);
impl StatusProvider for Status {
    fn job_success(&self) -> bool {
        !self.0
    }
    fn step_success(&self) -> bool {
        !self.0
    }
    fn job_failure(&self) -> bool {
        self.0
    }
    fn step_failure(&self) -> bool {
        self.0
    }
    fn cancelled(&self) -> bool {
        false
    }
}
struct LogState {
    file: fs::File,
    bytes: usize,
    tail: VecDeque<String>,
}
type JobMasks = Arc<Mutex<Vec<String>>>;
struct Sink {
    state: Mutex<LogState>,
    masks: JobMasks,
}
fn redact(message: &str, masks: &JobMasks) -> String {
    let mut text = message.to_string();
    for mask in masks.lock().unwrap().iter().filter(|m| !m.is_empty()) {
        text = text.replace(mask, "***");
    }
    text
}
impl Sink {
    fn new(path: &Path, masks: JobMasks) -> Result<Self> {
        Ok(Self {
            state: Mutex::new(LogState {
                file: fs::File::create(path)?,
                bytes: 0,
                tail: VecDeque::new(),
            }),
            masks,
        })
    }
    fn excerpt(&self) -> String {
        self.state
            .lock()
            .unwrap()
            .tail
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }
}
impl LogSink for Sink {
    fn log(&self, _: Level, message: &str) {
        if let Some(mask) = message.strip_prefix("::add-mask::") {
            self.masks.lock().unwrap().push(
                mask.replace("%0D", "\r")
                    .replace("%0A", "\n")
                    .replace("%25", "%"),
            );
            return;
        }
        let mut text = redact(message, &self.masks);
        let mut s = self.state.lock().unwrap();
        if text.to_ascii_lowercase().contains("authorization:") || text.contains("Bearer ") {
            text = "[redacted authentication output]".into();
        }
        let text = text.chars().take(4096).collect::<String>();
        if s.bytes < 512 * 1024 {
            let _ = writeln!(s.file, "{text}");
            s.bytes += text.len() + 1;
        }
        if s.tail.len() == 12 {
            s.tail.pop_front();
        }
        s.tail.push_back(text);
    }
}
fn relative(path: &str) -> Result<PathBuf> {
    let p = PathBuf::from(path);
    if p.is_absolute()
        || p.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        bail!("path must stay inside the job worktree: {path}");
    }
    Ok(p)
}
fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if entry.path().ends_with("docs/brand/capture/.profile")
            || name == ".git"
            || name == "node_modules"
            || name == ".next"
            || name == "target"
            || name == "runtime"
        {
            continue;
        }
        let kind = entry.file_type()?;
        let target = dst.join(&name);
        if kind.is_symlink() {
            bail!(
                "source symlink needs managed transfer validation: {}",
                entry.path().display()
            );
        }
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
fn condition(parsed: &HostWorkflow, id: Option<usize>) -> String {
    id.and_then(|id| parsed.document.scalar(id))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "success()".into())
}
fn unavailable(step: &ctox_actions_runner::model::Step, parsed: &HostWorkflow) -> Option<String> {
    let text = format!(
        "{} {:?} {} {:?}",
        step.run,
        step.with,
        step.uses,
        step.environment(&parsed.document)
    );
    let re = regex::Regex::new(r"secrets\.([A-Za-z_][A-Za-z0-9_]*)").unwrap();
    let names = re
        .captures_iter(&text)
        .map(|c| c[1].to_string())
        .collect::<std::collections::BTreeSet<_>>();
    if !names.is_empty() {
        return Some(format!(
            "secret nicht verfügbar: {}",
            names.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    let lower = text.to_ascii_lowercase();
    if lower.contains("wrangler deploy")
        || lower.contains("npm run deploy")
        || lower.contains("vercel deploy")
        || lower.contains("rotate:")
        || lower.contains("db:migrate")
    {
        return Some("deployment or mutation denied by the pilot assignment".into());
    }
    None
}
fn remap_tmp(value: &str, tmp: &Path) -> String {
    let destination = tmp.to_string_lossy();
    let mut result = String::new();
    let mut cursor = 0;
    for (index, _) in value.match_indices("/tmp") {
        let before = value[..index].chars().next_back();
        let after = value[index + 4..].chars().next();
        let boundary_before = before.is_none_or(|c| c.is_whitespace() || "'\"=:<>({[".contains(c));
        let boundary_after =
            after.is_none_or(|c| c == '/' || c.is_whitespace() || "'\");,]}".contains(c));
        let already_mapped = value[index..].starts_with(destination.as_ref())
            && value[index + destination.len()..]
                .chars()
                .next()
                .is_none_or(|c| c == '/');
        if boundary_before && boundary_after && !already_mapped {
            result.push_str(&value[cursor..index]);
            result.push_str(&destination);
            cursor = index + 4;
        }
    }
    result.push_str(&value[cursor..]);
    result
}
fn builtin(
    step: &ctox_actions_runner::model::Step,
    config: &Config,
    work: &Path,
    tmp: &Path,
    artifacts: &Path,
) -> Result<String> {
    let (name, pin) = step
        .uses
        .split_once('@')
        .ok_or_else(|| anyhow!("action must have a pinned reference"))?;
    if pin.is_empty() {
        bail!("action reference is empty");
    }
    match name {
        "actions/checkout" => {
            for key in step.with.keys() {
                if !["repository", "path", "ref", "fetch-depth"].contains(&key.as_str()) {
                    bail!("unsupported checkout input: {key}");
                }
            }
            let repo = step
                .with
                .get("repository")
                .map(String::as_str)
                .unwrap_or(&config.repository);
            let requested = step.with.get("ref").map(String::as_str).unwrap_or("");
            let expected = if repo == config.repository {
                &config.source_sha
            } else {
                &config.core_sha
            };
            if !requested.is_empty() && requested != expected {
                bail!("checkout ref differs from supplied exact snapshot");
            }
            if repo == config.repository && !step.with.contains_key("path") {
                return Ok(format!("Workjet snapshot checkout: {}", config.source_sha));
            }
            let path = relative(
                step.with
                    .get("path")
                    .ok_or_else(|| anyhow!("secondary checkout requires path"))?,
            )?;
            if repo == "metric-space-ai/ctox" {
                copy_tree(&config.core_source, &work.join(path))?;
                Ok(format!("Workjet local core snapshot: {}", config.core_sha))
            } else {
                bail!("external checkout snapshot not supplied: {repo}");
            }
        }
        "actions/setup-node" => {
            for key in step.with.keys() {
                if !["node-version", "cache", "cache-dependency-path"].contains(&key.as_str()) {
                    bail!("unsupported setup-node input: {key}");
                }
            }
            let requested = step
                .with
                .get("node-version")
                .ok_or_else(|| anyhow!("node-version required"))?;
            let out = std::process::Command::new(&config.node_bin)
                .arg("--version")
                .env_clear()
                .output()?;
            let actual = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !out.status.success()
                || !actual
                    .trim_start_matches('v')
                    .starts_with(&format!("{requested}."))
            {
                bail!("native Node mismatch: requested {requested}, observed {actual}");
            }
            Ok(format!(
                "Native {actual}; npm cache under pilot root; no action download"
            ))
        }
        "actions/upload-artifact" => {
            for key in step.with.keys() {
                if !["name", "path", "retention-days", "if-no-files-found"].contains(&key.as_str())
                {
                    bail!("unsupported artifact input: {key}");
                }
            }
            let name = relative(
                step.with
                    .get("name")
                    .map(String::as_str)
                    .unwrap_or("artifact"),
            )?;
            let dest = artifacts.join(name);
            fs::create_dir_all(&dest)?;
            let paths = step
                .with
                .get("path")
                .ok_or_else(|| anyhow!("artifact path required"))?;
            let mut count = 0;
            for raw in paths.lines().filter(|p| !p.trim().is_empty()) {
                let path = remap_tmp(raw.trim(), tmp);
                if path.contains(['*', '?', '[']) {
                    bail!("pilot artifact globs unsupported");
                }
                let src = if Path::new(&path).is_absolute() {
                    PathBuf::from(&path)
                } else {
                    work.join(relative(&path)?)
                };
                if !src.exists() {
                    continue;
                }
                let src = src.canonicalize()?;
                if !src.starts_with(work) && !src.starts_with(tmp) {
                    bail!("artifact escaped job root");
                }
                if !src.is_file() || fs::metadata(&src)?.len() > 50 * 1024 * 1024 {
                    bail!("artifact must be a regular file no larger than 50 MB");
                }
                fs::copy(&src, dest.join(src.file_name().unwrap()))?;
                count += 1;
            }
            if count == 0 && step.with.get("if-no-files-found").map(String::as_str) == Some("error")
            {
                bail!("artifact source files not produced");
            }
            Ok(format!(
                "{count} artifacts retained locally under {}; pilot expiry 7 days",
                dest.display()
            ))
        }
        _ => bail!("native action loader unavailable: {name}"),
    }
}
fn execute_job(
    config: &Config,
    parsed: &HostWorkflow,
    job_id: &str,
    workflow_root: &Path,
) -> Result<JobReport> {
    relative(job_id)?;
    let begin = Instant::now();
    let run = parsed.run(job_id)?;
    let job = run.job().unwrap().clone();
    let work = workflow_root.join(job_id).join("worktree");
    let tmp = workflow_root.join(job_id).join("tmp");
    let logs = workflow_root.join("logs").join(job_id);
    let artifacts = workflow_root.join("artifacts").join(job_id);
    let mut report = JobReport {
        job: job_id.into(),
        status: "success".into(),
        reason: String::new(),
        steps: Vec::new(),
        duration_ms: 0,
    };
    let gap = parsed.gaps.iter().find(|g| {
        g.job.as_deref().is_none_or(|id| id == job_id) && g.kind != HostGapKind::ActionLoader
    });
    if let Some(gap) = gap {
        report.status = "blocked".into();
        report.reason = gap.reason.clone();
        for (index, step) in job.steps.iter().enumerate() {
            report.steps.push(StepReport {
                step: if step.name.is_empty() {
                    format!("step-{index} {}", step.uses)
                } else {
                    step.name.clone()
                },
                status: "blocked".into(),
                reason: gap.reason.clone(),
                exit_code: None,
                duration_ms: 0,
                log: None,
                excerpt: String::new(),
            });
        }
        return Ok(report);
    }
    if job.raw_if.is_some()
        || job.strategy.is_some()
        || !job.needs(&parsed.document).is_empty()
        || !job.outputs.is_empty()
    {
        report.status = "blocked".into();
        report.reason = "matrix/needs/job outputs require managed orchestration".into();
        return Ok(report);
    }
    if !job
        .runs_on(&parsed.document)
        .iter()
        .any(|p| p.starts_with("ubuntu") || p == "linux" || p == "self-hosted")
    {
        report.status = "blocked".into();
        report.reason = "no matching Linux runner label".into();
        return Ok(report);
    }
    for p in [
        &tmp,
        &logs,
        &artifacts,
        &tmp.join("home"),
        &tmp.join("runner/workflow"),
    ] {
        fs::create_dir_all(p)?;
    }
    copy_tree(&config.source, &work)?;
    let mut host = HostEnvironment::new(
        work.clone(),
        tmp.clone(),
        tmp.join("tools"),
        work.to_str().unwrap(),
    );
    host.act_path = tmp.join("runner");
    let branch = format!("refs/pull/{}/head", config.pr);
    let sha = config.source_sha.clone();
    let repo = config.repository.clone();
    let git = GitLookups::new(
        move |_| Ok(branch.clone()),
        move |_| Ok(sha.clone()),
        move |_, _, _| Ok(repo.clone()),
    );
    let mut rc = RunContext {
        run: Some(run),
        ..Default::default()
    };
    rc.config.workdir = work.display().to_string();
    rc.config.event_name = "pull_request".into();
    rc.config.actor = "workjet-actions".into();
    let mut inputs = BTreeMap::new();
    if !parsed
        .workflow
        .on(&parsed.document)
        .iter()
        .any(|e| e == "pull_request")
    {
        rc.config.event_name = "workflow_dispatch".into();
        if let Some(node) = parsed
            .document
            .root()
            .and_then(|r| parsed.document.map_get(r, "on"))
            .and_then(|on| parsed.document.map_get(on, "workflow_dispatch"))
            .and_then(|dispatch| parsed.document.map_get(dispatch, "inputs"))
            .and_then(|id| parsed.document.node(id))
        {
            for pair in node.content.chunks_exact(2) {
                if let (Some(key), Some(value)) = (
                    parsed.document.scalar(pair[0]),
                    parsed
                        .document
                        .map_get(pair[1], "default")
                        .and_then(|id| parsed.document.scalar(id)),
                ) {
                    inputs.insert(key, value);
                }
            }
        }
    }
    rc.event_json=serde_json::json!({"pull_request":{"number":config.pr,"draft":false,"head":{"sha":config.source_sha}},"inputs":inputs}).to_string();
    rc.config.env = BTreeMap::from([
        (
            "PATH".into(),
            format!(
                "{}:/usr/local/bin:/usr/bin:/bin",
                config.node_bin.parent().unwrap().display()
            ),
        ),
        ("HOME".into(), tmp.join("home").display().to_string()),
        ("TMPDIR".into(), tmp.display().to_string()),
        ("TMP".into(), tmp.display().to_string()),
        ("TEMP".into(), tmp.display().to_string()),
        (
            "NPM_CONFIG_CACHE".into(),
            config.cache_root.display().to_string(),
        ),
        (
            "NPM_CONFIG_USERCONFIG".into(),
            tmp.join("empty-npmrc").display().to_string(),
        ),
        ("NPM_CONFIG_JOBS".into(), "2".into()),
        ("UV_THREADPOOL_SIZE".into(), "2".into()),
        ("NODE_OPTIONS".into(), "--max-old-space-size=4096".into()),
        ("NEXT_TELEMETRY_DISABLED".into(), "1".into()),
        (
            "XDG_CACHE_HOME".into(),
            tmp.join("cache").display().to_string(),
        ),
        ("GITHUB_REPOSITORY".into(), config.repository.clone()),
        ("GITHUB_SHA".into(), config.source_sha.clone()),
        ("GITHUB_REF".into(), format!("refs/pull/{}/head", config.pr)),
    ]);
    fs::write(tmp.join("empty-npmrc"), "")?;
    let mut failed = false;
    let masks: JobMasks = Arc::new(Mutex::new(Vec::new()));
    for (index, mut step) in job.steps.iter().cloned().enumerate() {
        if step.id.is_empty() {
            step.id = format!("step-{index}");
        }
        rc.current_step = step.id.clone();
        let name = if step.name.is_empty() {
            if step.uses.is_empty() {
                format!("run-{index}")
            } else {
                step.uses.clone()
            }
        } else {
            step.name.clone()
        };
        let mut entry = StepReport {
            step: name.clone(),
            status: "skipped".into(),
            reason: String::new(),
            exit_code: None,
            duration_ms: 0,
            log: None,
            excerpt: String::new(),
        };
        rc.env.extend(rc.global_env.clone());
        let github = rc.get_github_context(&git)?;
        let status = Status(failed);
        let mut env = BTreeMap::new();
        setup_env(&mut rc, &git, &status, &mut env, &step, &github).map_err(|e| anyhow!(e))?;
        let mut evaluator =
            new_step_expression_evaluator_with_env(&rc, &env, &rc.env, &github, false);
        evaluator
            .runner
            .insert("os".into(), Value::String("Linux".into()));
        evaluator
            .runner
            .insert("arch".into(), Value::String(std::env::consts::ARCH.into()));
        let enabled = is_step_enabled(
            &evaluator,
            &status,
            &condition(parsed, step.raw_if),
            StepStage::Main,
        )
        .map_err(|e| anyhow!(e))?;
        let continued = is_continue_on_error(&evaluator, &status, &step.raw_continue_on_error)
            .map_err(|e| anyhow!(e))?;
        if !enabled {
            entry.reason = "if/default success() did not select this step".into();
            report.steps.push(entry);
            continue;
        }
        let started = Instant::now();
        println!(
            "PR {} / {} / {job_id} / {name}: starting",
            config.pr, parsed.workflow.file
        );
        let log_path = logs.join(format!("{index}.log"));
        let sink = Arc::new(Sink::new(&log_path, masks.clone())?);
        host.replace_log_writer(sink.clone());
        entry.log = Some(log_path.display().to_string());
        let result = (|| -> Result<String> {
            if let Some(reason) = unavailable(&step, parsed) {
                bail!("{reason}");
            }
            for value in env.values_mut() {
                *value = remap_tmp(value, &tmp);
            }
            rc.apply_extra_path(&host, &mut env);
            for (key, path) in file_command_env(host.act_path.to_str().unwrap()) {
                fs::write(&path, "")?;
                env.insert(key, path);
            }
            if !step.uses.is_empty() {
                return builtin(&step, config, &work, &tmp, &artifacts);
            }
            step.run = remap_tmp(&step.run, &tmp);
            let prepared = prepare_run_step(&mut rc, &git, &status, &mut step, &env, &host)?;
            let selected = Path::new(&prepared.working_directory);
            let cwd = if selected.is_absolute() {
                selected.to_path_buf()
            } else {
                work.join(selected)
            }
            .canonicalize()?;
            if !cwd.starts_with(&work) {
                bail!("working directory escaped worktree");
            }
            fs::write(
                host.act_path.join(&prepared.script.name),
                &prepared.script.script,
            )?;
            let mut cmd = vec![
                "timeout".into(),
                "--signal=TERM".into(),
                "--kill-after=10s".into(),
                format!("{}s", config.step_timeout_seconds.min(900)),
            ];
            cmd.extend(prepared.script.cmd);
            host.exec(&cmd, &env, &prepared.working_directory)?;
            parse_env_text(&fs::read_to_string(&env["GITHUB_ENV"])?, &mut rc.global_env)
                .map_err(|e| anyhow!("{e}"))?;
            let mut outputs = BTreeMap::new();
            parse_env_text(&fs::read_to_string(&env["GITHUB_OUTPUT"])?, &mut outputs)
                .map_err(|e| anyhow!("{e}"))?;
            for path in fs::read_to_string(&env["GITHUB_PATH"])?.lines() {
                rc.extra_path.insert(0, path.into());
            }
            rc.step_results.insert(
                step.id.clone(),
                StepResult {
                    outcome: StepStatus::Success,
                    conclusion: StepStatus::Success,
                    outputs,
                },
            );
            Ok("native process exited 0".into())
        })();
        entry.duration_ms = started.elapsed().as_millis();
        entry.excerpt = sink.excerpt();
        match result {
            Ok(reason) => {
                entry.status = "success".into();
                entry.reason = reason;
                entry.exit_code = Some(0);
            }
            Err(error) => {
                entry.reason = redact(&error.to_string(), &masks);
                entry.status = if entry.reason.starts_with("secret nicht verfügbar") {
                    "blocked".into()
                } else {
                    "failure".into()
                };
                let re =
                    regex::Regex::new(r"Process completed with exit code (-?[0-9]+)\.").unwrap();
                entry.exit_code = re.captures(&entry.reason).and_then(|c| c[1].parse().ok());
                rc.step_results.insert(
                    step.id.clone(),
                    StepResult {
                        outcome: StepStatus::Failure,
                        conclusion: if continued {
                            StepStatus::Success
                        } else {
                            StepStatus::Failure
                        },
                        outputs: BTreeMap::new(),
                    },
                );
                if !continued {
                    failed = true;
                }
            }
        }
        println!(
            "PR {} / {job_id} / {name}: {} {}",
            config.pr, entry.status, entry.reason
        );
        report.steps.push(entry);
    }
    report.status = if failed {
        "failure".into()
    } else {
        "success".into()
    };
    report.duration_ms = begin.elapsed().as_millis();
    fs::remove_dir_all(&work)?;
    fs::remove_dir_all(&tmp)?;
    Ok(report)
}
fn execute(config: &Config) -> Result<Report> {
    if !config.trusted_snapshot {
        bail!("native pilot requires an explicitly trusted snapshot; no security sandbox");
    }
    for name in &config.allowed_workflows {
        if name.is_empty()
            || Path::new(name).file_name().and_then(|n| n.to_str()) != Some(name)
            || !matches!(
                Path::new(name).extension().and_then(|s| s.to_str()),
                Some("yml" | "yaml")
            )
            || !config.source.join(".github/workflows").join(name).is_file()
        {
            bail!("allowlist must name an existing workflow filename: {name}");
        }
    }
    if config.source_sha.len() != 40 || config.core_sha.len() != 40 {
        bail!("exact 40-character commit identities required");
    }
    if config.step_timeout_seconds == 0 {
        bail!("nonzero timeout required");
    }
    if config.build_root.exists() {
        bail!("pilot root must be new and uniquely owned");
    }
    fs::create_dir_all(&config.build_root)?;
    fs::create_dir_all(&config.cache_root)?;
    let mut report = Report {
        repository: config.repository.clone(),
        source_sha: config.source_sha.clone(),
        core_sha: config.core_sha.clone(),
        engine_sha: config.engine_sha.clone(),
        pr: config.pr,
        node: config.node_bin.display().to_string(),
        host: std::env::var("HOSTNAME").unwrap_or_else(|_| "gpu3".into()),
        workflows: Vec::new(),
    };
    let mut files = fs::read_dir(config.source.join(".github/workflows"))?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    files.sort();
    for path in files {
        if !matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("yml" | "yaml")
        ) {
            continue;
        }
        let source = fs::read_to_string(&path)?;
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let parsed = HostWorkflow::parse(&name, &source)?;
        let mut workflow = WorkflowReport {
            workflow: name.clone(),
            workflow_sha256: format!("{:x}", Sha256::digest(source.as_bytes())),
            status: "not_triggered".into(),
            reason: String::new(),
            jobs: Vec::new(),
        };
        if !parsed
            .workflow
            .on(&parsed.document)
            .iter()
            .any(|e| e == "pull_request")
            && !config.allowed_workflows.contains(&name)
        {
            workflow.reason = "no pull_request trigger and not explicitly allowlisted".into();
            report.workflows.push(workflow);
            fs::write(
                config.build_root.join("receipt.json"),
                serde_json::to_vec_pretty(&report)?,
            )?;
            continue;
        }
        let secrets = regex::Regex::new(r"secrets\.([A-Za-z_][A-Za-z0-9_]*)")?
            .captures_iter(&source)
            .map(|c| c[1].to_string())
            .collect::<std::collections::BTreeSet<_>>();
        if !secrets.is_empty() || name.starts_with("live-") {
            workflow.status = "blocked".into();
            workflow.reason = if secrets.is_empty() {
                "isolated live target not supplied; welsch live runs remain prohibited".into()
            } else {
                format!(
                    "secret nicht verfügbar: {}",
                    secrets.into_iter().collect::<Vec<_>>().join(", ")
                )
            };
            for (id, job) in &parsed.workflow.jobs {
                workflow.jobs.push(JobReport {
                    job: id.clone(),
                    status: "blocked".into(),
                    reason: workflow.reason.clone(),
                    steps: job
                        .steps
                        .iter()
                        .enumerate()
                        .map(|(index, step)| StepReport {
                            step: if step.name.is_empty() {
                                format!("step-{index} {}", step.uses)
                            } else {
                                step.name.clone()
                            },
                            status: "blocked".into(),
                            reason: workflow.reason.clone(),
                            exit_code: None,
                            duration_ms: 0,
                            log: None,
                            excerpt: String::new(),
                        })
                        .collect(),
                    duration_ms: 0,
                });
            }
        } else {
            let root = config.build_root.join(&name);
            fs::create_dir_all(&root)?;
            for job in parsed.workflow.jobs.keys() {
                let result =
                    execute_job(config, &parsed, job, &root).unwrap_or_else(|e| JobReport {
                        job: job.clone(),
                        status: "failure".into(),
                        reason: format!("pilot preparation/execution: {e}"),
                        steps: Vec::new(),
                        duration_ms: 0,
                    });
                if let Ok(id) = relative(job) {
                    for path in [root.join(&id).join("worktree"), root.join(&id).join("tmp")] {
                        if path.exists() {
                            fs::remove_dir_all(path)?;
                        }
                    }
                }
                workflow.jobs.push(result);
            }
            workflow.status = if workflow.jobs.iter().all(|j| j.status == "success") {
                "success".into()
            } else {
                "failure".into()
            };
        }
        report.workflows.push(workflow);
        fs::write(
            config.build_root.join("receipt.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;
    }
    Ok(report)
}
fn main() -> Result<()> {
    let path=std::env::args().nth(1).context("usage: cargo run --example workjet_host_pilot -- CONFIG.json (inside admitted GPU lane only)")?;
    let config: Config = serde_json::from_slice(&fs::read(path)?)?;
    let report = execute(&config)?;
    if report
        .workflows
        .iter()
        .any(|w| w.status != "success" && w.status != "not_triggered")
    {
        std::process::exit(1);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_remapping_preserves_owned_paths_and_is_idempotent() {
        let root = Path::new("/mnt/sata8t/build/job/tmp");
        let owned = root.join("cache/node-gyp").display().to_string();
        assert_eq!(remap_tmp(&owned, root), owned);
        let script = "cache=/tmp/cache; cat '/tmp/report' /var/tmp/keep https://host/tmp/keep";
        let mapped = remap_tmp(script, root);
        assert_eq!(mapped, "cache=/mnt/sata8t/build/job/tmp/cache; cat '/mnt/sata8t/build/job/tmp/report' /var/tmp/keep https://host/tmp/keep");
        assert_eq!(remap_tmp(&mapped, root), mapped);
        assert_eq!(remap_tmp("/tmp", root), root.display().to_string());
        assert_eq!(
            remap_tmp("/tmp/build/job/cache", Path::new("/tmp/build/job")),
            "/tmp/build/job/cache"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn native_failure_preserves_always_and_skips_success_steps() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir_all(source.join(".github/workflows")).unwrap();
        fs::write(source.join(".github/workflows/test.yml"),"on: pull_request\njobs:\n  check:\n    runs-on: ubuntu-latest\n    steps:\n      - name: failing\n        run: mkdir -p \"$XDG_CACHE_HOME/node-gyp\"; echo cached > \"$XDG_CACHE_HOME/node-gyp/test\"; echo observed; exit 7\n      - name: skipped\n        run: echo forbidden\n      - name: cleanup\n        if: always()\n        run: echo cleaned\n").unwrap();
        let config = Config {
            source,
            build_root: root.path().join("build"),
            cache_root: root.path().join("cache"),
            core_source: root.path().join("unused-core"),
            source_sha: "1".repeat(40),
            core_sha: "2".repeat(40),
            engine_sha: "3".repeat(40),
            repository: "fixture/project".into(),
            pr: 1,
            node_bin: PathBuf::from("/usr/bin/node"),
            step_timeout_seconds: 10,
            trusted_snapshot: true,
            allowed_workflows: Vec::new(),
        };
        let report = execute(&config).unwrap();
        let job = &report.workflows[0].jobs[0];
        assert_eq!(job.status, "failure");
        assert_eq!(job.steps[0].exit_code, Some(7));
        assert_eq!(job.steps[1].status, "skipped");
        assert_eq!(job.steps[2].status, "success");
        assert!(job.steps[0].excerpt.contains("observed"));
        assert!(job.steps[2].excerpt.contains("cleaned"));
        assert!(!config.build_root.join("test.yml/check/worktree").exists());
        assert!(!config.build_root.join("test.yml/check/tmp").exists());
        assert!(fs::read_dir(config.build_root.join("test.yml/check"))
            .unwrap()
            .next()
            .is_none());
    }
    #[test]
    fn container_job_is_blocked_without_copy_or_execution() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir_all(source.join(".github/workflows")).unwrap();
        fs::write(source.join(".github/workflows/test.yml"),"on: pull_request\njobs:\n  check:\n    runs-on: ubuntu-latest\n    services:\n      database:\n        image: postgres\n    steps:\n      - run: echo forbidden\n").unwrap();
        let config = Config {
            source,
            build_root: root.path().join("build"),
            cache_root: root.path().join("cache"),
            core_source: root.path().join("unused-core"),
            source_sha: "1".repeat(40),
            core_sha: "2".repeat(40),
            engine_sha: "3".repeat(40),
            repository: "fixture/project".into(),
            pr: 1,
            node_bin: PathBuf::from("/usr/bin/node"),
            step_timeout_seconds: 10,
            trusted_snapshot: true,
            allowed_workflows: Vec::new(),
        };
        let report = execute(&config).unwrap();
        assert_eq!(report.workflows[0].status, "failure");
        assert_eq!(report.workflows[0].jobs[0].status, "blocked");
        assert!(!config.build_root.join("test.yml/check/worktree").exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn native_selection_is_pr_or_explicit_allowlist_only() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir_all(source.join(".github/workflows")).unwrap();
        fs::write(source.join(".github/workflows/manual.yml"), "on: workflow_dispatch\njobs:\n  check:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo ::add-mask::job-fixture\n      - run: echo job-fixture\n").unwrap();
        let mut config = Config {
            source,
            build_root: root.path().join("not-selected"),
            cache_root: root.path().join("cache"),
            core_source: root.path().join("unused-core"),
            source_sha: "1".repeat(40),
            core_sha: "2".repeat(40),
            engine_sha: "3".repeat(40),
            repository: "fixture/project".into(),
            pr: 1,
            node_bin: PathBuf::from("/usr/bin/node"),
            step_timeout_seconds: 10,
            trusted_snapshot: true,
            allowed_workflows: Vec::new(),
        };
        let report = execute(&config).unwrap();
        assert_eq!(report.workflows[0].status, "not_triggered");
        assert!(report.workflows[0].jobs.is_empty());
        assert!(!config.build_root.join("manual.yml").exists());
        config.build_root = root.path().join("allowlisted");
        config.allowed_workflows.push("manual.yml".into());
        let report = execute(&config).unwrap();
        assert_eq!(report.workflows[0].status, "success");
        let job = &report.workflows[0].jobs[0];
        assert_eq!(job.steps[1].excerpt, "***");
        assert_eq!(
            fs::read_to_string(job.steps[1].log.as_ref().unwrap()).unwrap(),
            "***\n"
        );
        config.build_root = root.path().join("untrusted");
        config.trusted_snapshot = false;
        assert!(execute(&config)
            .err()
            .unwrap()
            .to_string()
            .contains("trusted snapshot"));
        assert!(!config.build_root.exists());
        config.trusted_snapshot = true;
        config.allowed_workflows = vec!["../manual.yml".into()];
        assert!(execute(&config)
            .err()
            .unwrap()
            .to_string()
            .contains("allowlist"));
    }

    #[test]
    fn artifact_paths_cannot_escape() {
        assert!(relative("../foreign").is_err());
        assert!(relative("/etc/passwd").is_err());
        assert!(relative("owned/file").is_ok());
    }
    #[test]
    fn secrets_and_deploys_are_denied() {
        let parsed = HostWorkflow::parse(
            "fixture.yml",
            "jobs:\n  check:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo ok\n",
        )
        .unwrap();
        let mut step = parsed.workflow.jobs["check"].steps[0].clone();
        step.run = "echo ${{ secrets.REQUIRED }}".into();
        assert_eq!(
            unavailable(&step, &parsed).unwrap(),
            "secret nicht verfügbar: REQUIRED"
        );
        step.run = "npm run deploy".into();
        assert!(unavailable(&step, &parsed).unwrap().contains("denied"));
    }
    #[test]
    fn masks_are_applied_before_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log");
        let masks = Arc::new(Mutex::new(Vec::new()));
        let sink = Sink::new(&path, masks.clone()).unwrap();
        sink.log(Level::Info, "::add-mask::fixture-value");
        sink.log(Level::Info, "echo fixture-value");
        assert_eq!(fs::read_to_string(path).unwrap(), "echo ***\n");
        assert!(!sink.excerpt().contains("fixture-value"));
        let next = dir.path().join("next-step.log");
        let next_sink = Sink::new(&next, masks).unwrap();
        next_sink.log(Level::Info, "next fixture-value");
        assert_eq!(fs::read_to_string(next).unwrap(), "next ***\n");
        assert!(!next_sink.excerpt().contains("fixture-value"));
        let other_job = Sink::new(
            &dir.path().join("other.log"),
            Arc::new(Mutex::new(Vec::new())),
        )
        .unwrap();
        other_job.log(Level::Info, "fixture-value");
        assert_eq!(other_job.excerpt(), "fixture-value");
    }
}
