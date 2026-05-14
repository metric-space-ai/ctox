use anyhow::Context;
use anyhow::Result;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::id as process_id;
use std::process::Command;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;

use crate::channels;
use crate::inference::engine;
use crate::inference::local_transport::LocalTransport;
use crate::inference::model_registry;
use crate::inference::runtime_kernel;
use crate::inference::runtime_state;
use crate::inference::supervisor;

const DEFAULT_DB_RELATIVE_PATH: &str = "runtime/ctox.sqlite3";
const DEFAULT_RUNTIME_ROOT: &str = "runtime/scraping";
const DEFAULT_QUEUE_PRIORITY: &str = "high";
const DEFAULT_REPAIR_SKILL: &str = "universal-scraping";
const DEFAULT_ENRICHMENT_MAX_RECORDS: usize = 50;
const MIN_TEMPLATE_TARGETS: i64 = 2;
const MIN_TEMPLATE_RESULTS: i64 = 20;
const MIN_TEMPLATE_CODE_LEN: usize = 160;

fn default_embedding_model() -> &'static str {
    model_registry::default_auxiliary_model(engine::AuxiliaryRole::Embedding)
        .expect("default embedding model must exist in the model registry")
}

const SCHEMA: &str = r#"
PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;

CREATE TABLE IF NOT EXISTS scrape_target (
    target_id TEXT PRIMARY KEY,
    target_key TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    start_url TEXT NOT NULL,
    target_kind TEXT NOT NULL DEFAULT 'generic',
    status TEXT NOT NULL DEFAULT 'active',
    schedule_hint TEXT,
    config_json TEXT NOT NULL DEFAULT '{}',
    output_schema_json TEXT NOT NULL DEFAULT '{}',
    workspace_dir TEXT NOT NULL,
    latest_script_revision_no INTEGER,
    latest_script_sha256 TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS scrape_script_revision (
    revision_id INTEGER PRIMARY KEY AUTOINCREMENT,
    target_id TEXT NOT NULL REFERENCES scrape_target(target_id) ON DELETE CASCADE,
    revision_no INTEGER NOT NULL,
    script_path TEXT NOT NULL,
    language TEXT NOT NULL DEFAULT 'javascript',
    entry_command_json TEXT NOT NULL DEFAULT '[]',
    script_sha256 TEXT NOT NULL,
    script_body TEXT NOT NULL,
    change_reason TEXT,
    notes TEXT,
    created_at TEXT NOT NULL,
    UNIQUE(target_id, revision_no),
    UNIQUE(target_id, script_sha256)
);

CREATE INDEX IF NOT EXISTS idx_scrape_script_revision_target_created
ON scrape_script_revision(target_id, created_at DESC);

CREATE TABLE IF NOT EXISTS scrape_source_revision (
    revision_id INTEGER PRIMARY KEY AUTOINCREMENT,
    target_id TEXT NOT NULL REFERENCES scrape_target(target_id) ON DELETE CASCADE,
    source_key TEXT NOT NULL,
    revision_no INTEGER NOT NULL,
    module_path TEXT NOT NULL,
    language TEXT NOT NULL DEFAULT 'javascript',
    module_sha256 TEXT NOT NULL,
    module_body TEXT NOT NULL,
    change_reason TEXT,
    notes TEXT,
    created_at TEXT NOT NULL,
    UNIQUE(target_id, source_key, revision_no),
    UNIQUE(target_id, source_key, module_sha256)
);

CREATE INDEX IF NOT EXISTS idx_scrape_source_revision_target_source_created
ON scrape_source_revision(target_id, source_key, created_at DESC);

CREATE TABLE IF NOT EXISTS scrape_template_example (
    example_id INTEGER PRIMARY KEY AUTOINCREMENT,
    template_key TEXT NOT NULL,
    target_id TEXT NOT NULL REFERENCES scrape_target(target_id) ON DELETE CASCADE,
    script_sha256 TEXT NOT NULL,
    script_body TEXT NOT NULL,
    language TEXT NOT NULL DEFAULT 'javascript',
    result_count INTEGER,
    challenge_score INTEGER NOT NULL DEFAULT 0,
    nomination_reason TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(template_key, target_id, script_sha256)
);

CREATE INDEX IF NOT EXISTS idx_scrape_template_example_key_sha
ON scrape_template_example(template_key, script_sha256, updated_at DESC);

CREATE TABLE IF NOT EXISTS scrape_template_promoted (
    template_key TEXT PRIMARY KEY,
    script_sha256 TEXT NOT NULL,
    script_body TEXT NOT NULL,
    language TEXT NOT NULL DEFAULT 'javascript',
    source_example_count INTEGER NOT NULL DEFAULT 1,
    source_target_count INTEGER NOT NULL DEFAULT 1,
    best_result_count INTEGER,
    promotion_reason TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS scrape_run (
    run_id TEXT PRIMARY KEY,
    target_id TEXT NOT NULL REFERENCES scrape_target(target_id) ON DELETE CASCADE,
    trigger_kind TEXT NOT NULL,
    scheduled_for TEXT,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    status TEXT NOT NULL,
    script_revision_no INTEGER,
    script_sha256 TEXT,
    run_context_json TEXT NOT NULL DEFAULT '{}',
    result_json TEXT NOT NULL DEFAULT '{}',
    output_dir TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_scrape_run_target_started
ON scrape_run(target_id, started_at DESC);

CREATE TABLE IF NOT EXISTS scrape_artifact (
    artifact_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES scrape_run(run_id) ON DELETE CASCADE,
    artifact_kind TEXT NOT NULL,
    path TEXT NOT NULL,
    schema_key TEXT,
    content_sha256 TEXT,
    record_count INTEGER,
    created_at TEXT NOT NULL,
    UNIQUE(run_id, artifact_kind, path)
);

CREATE TABLE IF NOT EXISTS scrape_record_latest (
    target_id TEXT NOT NULL REFERENCES scrape_target(target_id) ON DELETE CASCADE,
    record_key TEXT NOT NULL,
    record_hash TEXT NOT NULL,
    record_json TEXT NOT NULL,
    schema_key TEXT,
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    last_run_id TEXT NOT NULL,
    deleted_at TEXT,
    PRIMARY KEY(target_id, record_key)
);

CREATE INDEX IF NOT EXISTS idx_scrape_record_latest_target_active
ON scrape_record_latest(target_id, deleted_at, last_seen_at DESC);

CREATE TABLE IF NOT EXISTS scrape_semantic_record (
    target_id TEXT NOT NULL REFERENCES scrape_target(target_id) ON DELETE CASCADE,
    record_key TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    source_text TEXT NOT NULL,
    embedding_json TEXT NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    updated_at TEXT NOT NULL,
    PRIMARY KEY(target_id, record_key)
);

CREATE INDEX IF NOT EXISTS idx_scrape_semantic_record_target
ON scrape_semantic_record(target_id, updated_at DESC);
"#;

#[derive(Debug, Clone, Serialize)]
struct ScrapeTargetView {
    target_id: String,
    target_key: String,
    display_name: String,
    start_url: String,
    target_kind: String,
    status: String,
    schedule_hint: Option<String>,
    config: Value,
    output_schema: Value,
    workspace_dir: String,
    latest_script_revision_no: Option<i64>,
    latest_script_sha256: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
struct ScrapeScriptRevisionView {
    revision_no: i64,
    script_path: String,
    language: String,
    script_sha256: String,
    change_reason: Option<String>,
    notes: Option<String>,
    created_at: String,
}

#[derive(Debug, Clone)]
struct ScrapeScriptRevisionRecord {
    revision_no: i64,
    script_path: String,
    language: String,
    entry_command: Vec<String>,
    script_sha256: String,
}

#[derive(Debug, Clone)]
struct RegisteredTarget {
    view: ScrapeTargetView,
    script: ScrapeScriptRevisionRecord,
    workspace_root: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
struct ScrapeSourceRevisionView {
    source_key: String,
    revision_no: i64,
    module_path: String,
    language: String,
    module_sha256: String,
    change_reason: Option<String>,
    notes: Option<String>,
    created_at: String,
}

#[derive(Debug, Clone, Serialize)]
struct RecentRunView {
    run_id: String,
    target_key: String,
    status: String,
    trigger_kind: String,
    finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ScrapeArtifactRecord {
    artifact_id: String,
    artifact_kind: String,
    path: String,
    schema_key: Option<String>,
    content_sha256: Option<String>,
    record_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct LatestRecordView {
    record_key: String,
    last_seen_at: String,
    record: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScrapeSourceDefinition {
    source_key: String,
    display_name: String,
    start_url: String,
    source_kind: String,
    enabled: bool,
    extraction_module: String,
    merge_strategy: String,
    tags: Vec<String>,
    notes: Option<String>,
    config: Value,
}

#[derive(Debug, Clone)]
struct SemanticConfig {
    enabled: bool,
    source_fields: Vec<String>,
    embedding_model: String,
    default_limit: usize,
}

#[derive(Debug, Clone, Serialize)]
struct SemanticMatch {
    record_key: String,
    score: f64,
    source_text: String,
    record: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EnrichmentTaskConfig {
    kind: String,
    output_field: String,
    instruction: String,
    #[serde(default)]
    field_hints: Vec<String>,
    #[serde(default)]
    filter_field_hints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EnrichmentConfig {
    enabled: bool,
    model: String,
    timeout_seconds: u64,
    max_records: usize,
    source_fields: Vec<String>,
    tasks: Vec<EnrichmentTaskConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EnrichmentUpdate {
    path: String,
    value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EnrichmentResponse {
    #[serde(default)]
    updates: Vec<EnrichmentUpdate>,
    #[serde(default)]
    notes: Option<String>,
}

#[derive(Debug, Clone)]
struct EnrichmentOutcome {
    records: Vec<Value>,
    summary: Value,
    artifacts: Vec<ScrapeArtifactRecord>,
}

#[derive(Debug, Clone)]
struct ProbeResult {
    reachable: bool,
    status_code: Option<u16>,
    final_url: String,
    human_verification: bool,
    error: Option<String>,
}

#[derive(Debug)]
struct CommandExecution {
    exit_code: Option<i32>,
    timed_out: bool,
    stdout_text: String,
    stderr_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailureStatus {
    Succeeded,
    TemporaryUnreachable,
    PortalDrift,
    Blocked,
    PartialOutput,
}

impl FailureStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::TemporaryUnreachable => "temporary_unreachable",
            Self::PortalDrift => "portal_drift",
            Self::Blocked => "blocked",
            Self::PartialOutput => "partial_output",
        }
    }
}

#[derive(Debug)]
struct Classification {
    status: FailureStatus,
    should_queue_repair: bool,
    reason: String,
}

#[derive(Debug, Clone)]
struct MaterializationOutcome {
    summary: Value,
    delta_artifact: ScrapeArtifactRecord,
}

struct TargetRunLock {
    path: PathBuf,
}

impl Drop for TargetRunLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn handle_scrape_command(root: &Path, args: &[String]) -> Result<()> {
    let command = args.first().map(String::as_str).unwrap_or("");
    match command {
        "init" => {
            let conn = open_db(root)?;
            print_json(&json!({
                "ok": true,
                "db_path": resolve_db_path(root),
                "initialized": {
                    "targets_total": count_rows(&conn, "scrape_target")?,
                    "script_revisions_total": count_rows(&conn, "scrape_script_revision")?,
                    "runs_total": count_rows(&conn, "scrape_run")?,
                }
            }))
        }
        "summary" => print_json(&summary_payload(root)?),
        "list-targets" => print_json(&json!({ "ok": true, "targets": list_targets(root)? })),
        "show-target" => {
            let target_key = required_flag_value(args, "--target-key")
                .or_else(|| args.get(1).map(String::as_str))
                .context("usage: ctox scrape show-target --target-key <key>")?;
            let target = show_target(root, target_key)?.context("target_key not found")?;
            print_json(&json!({ "ok": true, "target": target }))
        }
        "show-latest" => {
            let target_key = required_flag_value(args, "--target-key")
                .or_else(|| args.get(1).map(String::as_str))
                .context("usage: ctox scrape show-latest --target-key <key> [--limit <n>]")?;
            let limit = find_flag_value(args, "--limit")
                .map(|value| value.parse::<usize>())
                .transpose()
                .context("failed to parse --limit")?
                .unwrap_or(20);
            let latest = show_latest(root, target_key, limit)?.context("target_key not found")?;
            print_json(&json!({ "ok": true, "latest": latest }))
        }
        "show-api" => {
            let target_key = required_flag_value(args, "--target-key")
                .or_else(|| args.get(1).map(String::as_str))
                .context("usage: ctox scrape show-api --target-key <key>")?;
            let api = show_api(root, target_key)?.context("target_key not found")?;
            print_json(&json!({ "ok": true, "api": api }))
        }
        "query-records" => {
            let target_key = required_flag_value(args, "--target-key")
                .or_else(|| args.get(1).map(String::as_str))
                .context("usage: ctox scrape query-records --target-key <key> [--where field=value]... [--limit <n>]")?;
            let limit = find_flag_value(args, "--limit")
                .map(|value| value.parse::<usize>())
                .transpose()
                .context("failed to parse --limit")?
                .unwrap_or(50);
            let filters = parse_where_filters(args)?;
            let response = query_records(root, target_key, &filters, limit)?
                .context("target_key not found")?;
            print_json(&json!({ "ok": true, "query": response }))
        }
        "semantic-search" => {
            let target_key = required_flag_value(args, "--target-key")
                .or_else(|| args.get(1).map(String::as_str))
                .context("usage: ctox scrape semantic-search --target-key <key> --query <text> [--limit <n>]")?;
            let query = required_flag_value(args, "--query")
                .or_else(|| find_flag_value(args, "-q"))
                .context("usage: ctox scrape semantic-search --target-key <key> --query <text> [--limit <n>]")?;
            let limit = find_flag_value(args, "--limit")
                .map(|value| value.parse::<usize>())
                .transpose()
                .context("failed to parse --limit")?
                .unwrap_or(12);
            let response =
                semantic_search(root, target_key, query, limit)?.context("target_key not found")?;
            print_json(&json!({ "ok": true, "semantic": response }))
        }
        "rebuild-semantic" => {
            let target_key = required_flag_value(args, "--target-key")
                .or_else(|| args.get(1).map(String::as_str))
                .context("usage: ctox scrape rebuild-semantic --target-key <key> [--limit <n>]")?;
            let response =
                rebuild_semantic_index(root, target_key)?.context("target_key not found")?;
            print_json(&json!({ "ok": true, "semantic_rebuild": response }))
        }
        "upsert-target" => {
            let input = required_flag_value(args, "--input").context(
                "usage: ctox scrape upsert-target --input <json-path> [--runtime-root <path>]",
            )?;
            let runtime_root =
                find_flag_value(args, "--runtime-root").unwrap_or(DEFAULT_RUNTIME_ROOT);
            let payload = load_json_file(root, input)?;
            let target = upsert_target(root, runtime_root, payload)?;
            print_json(&json!({ "ok": true, "target": target }))
        }
        "register-script" => {
            let target_key = required_flag_value(args, "--target-key")
                .context("usage: ctox scrape register-script --target-key <key> --script-file <path> [--language <lang>] [--change-reason <text>] [--notes <text>] [--runtime-root <path>]")?;
            let script_file = required_flag_value(args, "--script-file")
                .context("usage: ctox scrape register-script --target-key <key> --script-file <path> [--language <lang>] [--change-reason <text>] [--notes <text>] [--runtime-root <path>]")?;
            let runtime_root =
                find_flag_value(args, "--runtime-root").unwrap_or(DEFAULT_RUNTIME_ROOT);
            let language = find_flag_value(args, "--language").unwrap_or("javascript");
            let registered = register_script(
                root,
                runtime_root,
                target_key,
                script_file,
                language,
                find_flag_value(args, "--change-reason"),
                find_flag_value(args, "--notes"),
            )?;
            print_json(&json!({ "ok": true, "script": registered }))
        }
        "register-source-module" => {
            let target_key = required_flag_value(args, "--target-key")
                .context("usage: ctox scrape register-source-module --target-key <key> --source-key <key> --module-file <path> [--language <lang>] [--change-reason <text>] [--notes <text>] [--runtime-root <path>]")?;
            let source_key = required_flag_value(args, "--source-key")
                .context("usage: ctox scrape register-source-module --target-key <key> --source-key <key> --module-file <path> [--language <lang>] [--change-reason <text>] [--notes <text>] [--runtime-root <path>]")?;
            let module_file = required_flag_value(args, "--module-file")
                .context("usage: ctox scrape register-source-module --target-key <key> --source-key <key> --module-file <path> [--language <lang>] [--change-reason <text>] [--notes <text>] [--runtime-root <path>]")?;
            let runtime_root =
                find_flag_value(args, "--runtime-root").unwrap_or(DEFAULT_RUNTIME_ROOT);
            let language = find_flag_value(args, "--language").unwrap_or("javascript");
            let registered = register_source_module(
                root,
                runtime_root,
                target_key,
                source_key,
                module_file,
                language,
                find_flag_value(args, "--change-reason"),
                find_flag_value(args, "--notes"),
            )?;
            print_json(&json!({ "ok": true, "source_module": registered }))
        }
        "record-template-example" => {
            let target_key = required_flag_value(args, "--target-key")
                .context("usage: ctox scrape record-template-example --target-key <key> --template-key <template> --script-file <path> [--language <lang>] [--result-count <n>] [--challenge-score <n>] [--reason <text>]")?;
            let template_key = required_flag_value(args, "--template-key")
                .context("usage: ctox scrape record-template-example --target-key <key> --template-key <template> --script-file <path> [--language <lang>] [--result-count <n>] [--challenge-score <n>] [--reason <text>]")?;
            let script_file = required_flag_value(args, "--script-file")
                .context("usage: ctox scrape record-template-example --target-key <key> --template-key <template> --script-file <path> [--language <lang>] [--result-count <n>] [--challenge-score <n>] [--reason <text>]")?;
            let language = find_flag_value(args, "--language").unwrap_or("javascript");
            let result_count = find_flag_value(args, "--result-count")
                .map(|value| value.parse::<i64>())
                .transpose()
                .context("failed to parse --result-count")?;
            let challenge_score = find_flag_value(args, "--challenge-score")
                .map(|value| value.parse::<i64>())
                .transpose()
                .context("failed to parse --challenge-score")?
                .unwrap_or(0);
            let result = record_template_example(
                root,
                target_key,
                template_key,
                script_file,
                language,
                result_count,
                challenge_score,
                find_flag_value(args, "--reason"),
            )?;
            print_json(&json!({ "ok": true, "template_event": result }))
        }
        "promote-template" => {
            let template_key = required_flag_value(args, "--template-key")
                .context("usage: ctox scrape promote-template --template-key <template> --script-file <path> [--language <lang>] --reason <text>")?;
            let script_file = required_flag_value(args, "--script-file")
                .context("usage: ctox scrape promote-template --template-key <template> --script-file <path> [--language <lang>] --reason <text>")?;
            let language = find_flag_value(args, "--language").unwrap_or("javascript");
            let reason = required_flag_value(args, "--reason")
                .context("usage: ctox scrape promote-template --template-key <template> --script-file <path> [--language <lang>] --reason <text>")?;
            let promoted = promote_template(root, template_key, script_file, language, reason)?;
            print_json(&json!({ "ok": true, "promoted_template": promoted }))
        }
        "execute" => execute_scrape(root, args),
        _ => anyhow::bail!(
            "usage:\n  ctox scrape init\n  ctox scrape summary\n  ctox scrape list-targets\n  ctox scrape show-target --target-key <key>\n  ctox scrape show-latest --target-key <key> [--limit <n>]\n  ctox scrape show-api --target-key <key>\n  ctox scrape query-records --target-key <key> [--where field=value]... [--limit <n>]\n  ctox scrape semantic-search --target-key <key> --query <text> [--limit <n>]\n  ctox scrape rebuild-semantic --target-key <key>\n  ctox scrape upsert-target --input <json-path> [--runtime-root <path>]\n  ctox scrape register-script --target-key <key> --script-file <path> [--language <lang>] [--change-reason <text>] [--notes <text>] [--runtime-root <path>]\n  ctox scrape register-source-module --target-key <key> --source-key <key> --module-file <path> [--language <lang>] [--change-reason <text>] [--notes <text>] [--runtime-root <path>]\n  ctox scrape record-template-example --target-key <key> --template-key <template> --script-file <path> [--language <lang>] [--result-count <n>] [--challenge-score <n>] [--reason <text>]\n  ctox scrape promote-template --template-key <template> --script-file <path> [--language <lang>] --reason <text>\n  ctox scrape execute --target-key <key> [--trigger-kind <manual|scheduled|repair>] [--scheduled-for <iso>] [--timeout-seconds <n>] [--runtime-root <path>] [--allow-heal] [--thread-key <key>] [--queue-priority <urgent|high|normal|low>]"
        ),
    }
}

fn execute_scrape(root: &Path, args: &[String]) -> Result<()> {
    let target_key = required_flag_value(args, "--target-key")
        .context("usage: ctox scrape execute --target-key <key> [--trigger-kind <manual|scheduled|repair>] [--scheduled-for <iso>] [--timeout-seconds <n>] [--runtime-root <path>] [--allow-heal] [--input-json <text>] [--input-file <path>] [--thread-key <key>] [--queue-priority <urgent|high|normal|low>]")?;
    let trigger_kind = find_flag_value(args, "--trigger-kind").unwrap_or("manual");
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
    )?;
    let payload = match parse_execution_payload(&execution.stdout_text) {
        Ok(value) => value,
        Err(error) => json!({
            "failure_mode": "portal_drift",
            "parse_error": true,
            "detail": error.to_string(),
        }),
    };
    let records = normalize_records(&payload);
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
    let classification = classify_outcome(
        &payload,
        &probe,
        &execution,
        records_found,
        expected_min_records,
    );
    let run_finished_at = now_iso_string();
    let default_schema_key = target
        .view
        .output_schema
        .get("schema_key")
        .and_then(Value::as_str);
    let enrichment = match records.as_deref() {
        Some(items) if classification.status == FailureStatus::Succeeded => {
            Some(maybe_run_llm_enrichment(root, &target, items, &output_dir)?)
        }
        _ => None,
    };
    let materialized_records = enrichment
        .as_ref()
        .map(|outcome| outcome.records.as_slice())
        .or(records.as_deref());
    let materialization = if classification.status == FailureStatus::Succeeded {
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
        )?)
    } else {
        None
    };
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
                "last_successful_run": last_successful_run,
            }),
            result: json!({
                "records_found": records_found,
                "enriched_records_found": materialized_records.map(|items| items.len() as i64),
                "source_count": target_sources(&target.view).len(),
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

    let template_event = if classification.status == FailureStatus::Succeeded {
        maybe_record_template_from_target(root, &target, records_found)?
    } else {
        None
    };

    let repair_queue_task = if allow_heal && classification.should_queue_repair {
        let thread_key = find_flag_value(args, "--thread-key")
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("scrape/{}", target.view.target_key));
        let priority = find_flag_value(args, "--queue-priority").unwrap_or(DEFAULT_QUEUE_PRIORITY);
        let repair_prompt = build_repair_prompt(
            root,
            &target,
            &run_id,
            classification.status,
            records_found,
            repair_request_path.as_ref(),
        );
        Some(channels::create_queue_task(
            root,
            channels::QueueTaskCreateRequest {
                title: format!("repair scrape target {}", target.view.target_key),
                prompt: repair_prompt,
                thread_key,
                workspace_root: None,
                priority: priority.to_string(),
                suggested_skill: Some(DEFAULT_REPAIR_SKILL.to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )?)
    } else {
        None
    };

    print_json(&json!({
        "ok": true,
        "target_key": target.view.target_key,
        "run_id": run_id,
        "status": classification.status.as_str(),
        "records_found": records_found,
        "reason": classification.reason,
        "probe": probe_to_json(&probe),
        "should_queue_repair": classification.should_queue_repair,
        "repair_request_path": repair_request_path.as_ref().map(|path| path.to_string_lossy().to_string()),
        "repair_queue_task": repair_queue_task,
        "template_event": template_event,
        "materialization": materialization.as_ref().map(|item| item.summary.clone()),
        "run_manifest_path": run_dir.join("run.json"),
    }))
}

fn summary_payload(root: &Path) -> Result<Value> {
    let conn = open_db(root)?;
    let recent_runs = {
        let mut statement = conn.prepare(
            r#"
            SELECT r.run_id, t.target_key, r.status, r.trigger_kind, r.finished_at
            FROM scrape_run r
            JOIN scrape_target t ON t.target_id = r.target_id
            ORDER BY COALESCE(r.finished_at, r.started_at) DESC
            LIMIT 10
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            Ok(RecentRunView {
                run_id: row.get(0)?,
                target_key: row.get(1)?,
                status: row.get(2)?,
                trigger_kind: row.get(3)?,
                finished_at: row.get(4)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    Ok(json!({
        "ok": true,
        "targets_total": count_rows(&conn, "scrape_target")?,
        "targets_active": count_filtered_rows(&conn, "scrape_target", "status = 'active'")?,
        "script_revisions_total": count_rows(&conn, "scrape_script_revision")?,
        "source_revisions_total": count_rows(&conn, "scrape_source_revision")?,
        "template_examples_total": count_rows(&conn, "scrape_template_example")?,
        "templates_promoted_total": count_filtered_rows(&conn, "scrape_template_promoted", "is_active = 1")?,
        "runs_total": count_rows(&conn, "scrape_run")?,
        "materialized_active_records_total": count_filtered_rows(&conn, "scrape_record_latest", "deleted_at IS NULL")?,
        "recent_runs": recent_runs,
    }))
}

fn list_targets(root: &Path) -> Result<Vec<ScrapeTargetView>> {
    let conn = open_db(root)?;
    let mut statement = conn.prepare(
        r#"
        SELECT target_id, target_key, display_name, start_url, target_kind, status, schedule_hint,
               config_json, output_schema_json, workspace_dir, latest_script_revision_no,
               latest_script_sha256, created_at, updated_at
        FROM scrape_target
        ORDER BY updated_at DESC, target_key ASC
        "#,
    )?;
    let rows = statement.query_map([], map_target_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn show_target(root: &Path, target_key: &str) -> Result<Option<Value>> {
    let conn = open_db(root)?;
    let target = load_target_view(&conn, target_key)?;
    let Some(target) = target else {
        return Ok(None);
    };
    let revisions = load_script_revisions(&conn, &target.target_id)?;
    let sources = target_sources(&target);
    let source_revisions = load_source_revisions(&conn, &target.target_id)?;
    Ok(Some(json!({
        "target_id": target.target_id,
        "target_key": target.target_key,
        "display_name": target.display_name,
        "start_url": target.start_url,
        "target_kind": target.target_kind,
        "status": target.status,
        "schedule_hint": target.schedule_hint,
        "config": target.config,
        "sources": sources,
        "output_schema": target.output_schema,
        "workspace_dir": target.workspace_dir,
        "latest_script_revision_no": target.latest_script_revision_no,
        "latest_script_sha256": target.latest_script_sha256,
        "created_at": target.created_at,
        "updated_at": target.updated_at,
        "revisions": revisions,
        "source_revisions": source_revisions,
    })))
}

pub(crate) fn show_latest(root: &Path, target_key: &str, limit: usize) -> Result<Option<Value>> {
    let conn = open_db(root)?;
    let Some(target) = load_target_view(&conn, target_key)? else {
        return Ok(None);
    };
    let limit = limit.max(1) as i64;
    let active_count = conn.query_row(
        "SELECT COUNT(*) FROM scrape_record_latest WHERE target_id = ?1 AND deleted_at IS NULL",
        params![target.target_id],
        |row| row.get::<_, i64>(0),
    )?;
    let deleted_count = conn.query_row(
        "SELECT COUNT(*) FROM scrape_record_latest WHERE target_id = ?1 AND deleted_at IS NOT NULL",
        params![target.target_id],
        |row| row.get::<_, i64>(0),
    )?;
    let latest_records = {
        let mut statement = conn.prepare(
            r#"
            SELECT record_key, last_seen_at, record_json
            FROM scrape_record_latest
            WHERE target_id = ?1 AND deleted_at IS NULL
            ORDER BY last_seen_at DESC, record_key ASC
            LIMIT ?2
            "#,
        )?;
        let rows = statement.query_map(params![target.target_id, limit], |row| {
            let record_json: String = row.get(2)?;
            Ok(LatestRecordView {
                record_key: row.get(0)?,
                last_seen_at: row.get(1)?,
                record: serde_json::from_str(&record_json).unwrap_or_else(|_| json!({})),
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let last_successful_run = load_last_successful_run(&conn, &target.target_id)?;
    let state_dir = resolve_workspace_dir(root, &target.workspace_dir).join("state");
    Ok(Some(json!({
        "target_key": target.target_key,
        "workspace_dir": target.workspace_dir,
        "active_record_count": active_count,
        "deleted_record_count": deleted_count,
        "state_paths": {
            "latest_records": state_dir.join("latest_records.json"),
            "latest_summary": state_dir.join("latest_summary.json"),
        },
        "last_successful_run": last_successful_run,
        "records": latest_records,
    })))
}

fn show_api(root: &Path, target_key: &str) -> Result<Option<Value>> {
    let conn = open_db(root)?;
    let Some(target) = load_target_view(&conn, target_key)? else {
        return Ok(None);
    };
    Ok(Some(build_target_api_contract(root, &target)))
}

fn query_records(
    root: &Path,
    target_key: &str,
    filters: &[(String, String)],
    limit: usize,
) -> Result<Option<Value>> {
    let conn = open_db(root)?;
    let Some(target) = load_target_view(&conn, target_key)? else {
        return Ok(None);
    };
    let items = load_all_latest_active_records(&conn, &target.target_id)?;
    let filtered = items
        .into_iter()
        .filter(|item| record_matches_filters(&item.record, filters))
        .take(limit.max(1))
        .map(|item| {
            json!({
                "record_key": item.record_key,
                "last_seen_at": item.last_seen_at,
                "record": item.record,
            })
        })
        .collect::<Vec<_>>();
    Ok(Some(json!({
        "target_key": target.target_key,
        "filters": filters.iter().map(|(field, value)| json!({"field": field, "value": value})).collect::<Vec<_>>(),
        "limit": limit.max(1),
        "count": filtered.len(),
        "items": filtered,
        "api": build_target_api_contract(root, &target),
    })))
}

fn rebuild_semantic_index(root: &Path, target_key: &str) -> Result<Option<Value>> {
    let conn = open_db(root)?;
    let Some(target) = load_target_view(&conn, target_key)? else {
        return Ok(None);
    };
    let config = load_semantic_config(root, &target);
    let records = load_all_latest_active_records(&conn, &target.target_id)?;
    let indexed = ensure_semantic_records(root, &conn, &target, &records, &config)?;
    Ok(Some(json!({
        "target_key": target.target_key,
        "semantic_enabled": config.enabled,
        "source_fields": config.source_fields,
        "embedding_model": config.embedding_model,
        "indexed_records": indexed,
    })))
}

fn semantic_search(
    root: &Path,
    target_key: &str,
    query: &str,
    limit: usize,
) -> Result<Option<Value>> {
    let conn = open_db(root)?;
    let Some(target) = load_target_view(&conn, target_key)? else {
        return Ok(None);
    };
    let config = load_semantic_config(root, &target);
    if !config.enabled {
        return Ok(Some(json!({
            "target_key": target.target_key,
            "semantic_enabled": false,
            "message": "semantic search disabled for target",
            "api": build_target_api_contract(root, &target),
        })));
    }
    let records = load_all_latest_active_records(&conn, &target.target_id)?;
    let indexed = ensure_semantic_records(root, &conn, &target, &records, &config)?;
    let query_embedding = embed_texts(root, &[query.to_string()], &config.embedding_model)?
        .into_iter()
        .next()
        .context("embedding service returned no query vector")?;
    let mut matches = load_semantic_matches(&conn, &target.target_id)?
        .into_iter()
        .filter_map(|(record_key, source_text, embedding)| {
            let latest = records.iter().find(|item| item.record_key == record_key)?;
            Some(SemanticMatch {
                record_key,
                score: cosine_similarity(&query_embedding, &embedding),
                source_text,
                record: latest.record.clone(),
            })
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
    });
    matches.truncate(limit.max(1).min(config.default_limit.max(1)));
    Ok(Some(json!({
        "target_key": target.target_key,
        "query": query,
        "semantic_enabled": true,
        "embedding_model": config.embedding_model,
        "source_fields": config.source_fields,
        "indexed_records": indexed,
        "count": matches.len(),
        "matches": matches,
        "api": build_target_api_contract(root, &target),
    })))
}

pub(crate) fn service_show_api(root: &Path, target_key: &str) -> Result<Option<Value>> {
    show_api(root, target_key)
}

pub(crate) fn service_query_records(
    root: &Path,
    target_key: &str,
    filters: &[(String, String)],
    limit: usize,
) -> Result<Option<Value>> {
    query_records(root, target_key, filters, limit)
}

pub(crate) fn service_semantic_search(
    root: &Path,
    target_key: &str,
    query: &str,
    limit: usize,
) -> Result<Option<Value>> {
    semantic_search(root, target_key, query, limit)
}

fn build_target_api_contract(root: &Path, target: &ScrapeTargetView) -> Value {
    let semantic = load_semantic_config(root, target);
    let workspace_dir = resolve_workspace_dir(root, &target.workspace_dir);
    let enrichment = load_llm_enrichment_config(root, target);
    let filter_paths = configured_filter_paths(root, target);
    let sources = target_sources(target);
    let source_modules = open_db(root)
        .ok()
        .and_then(|conn| latest_source_revision_map(&conn, &target.target_id).ok())
        .map(|items| items.into_values().collect::<Vec<_>>())
        .unwrap_or_default();
    json!({
        "target_key": target.target_key,
        "display_name": target.display_name,
        "workspace_dir": target.workspace_dir,
        "source_count": sources.len(),
        "sources": sources,
        "source_modules": source_modules,
        "paths": {
            "api_contract": workspace_dir.join("api/api_contract.json"),
            "api_readme": workspace_dir.join("api/README.md"),
            "llm_enrichment_template": workspace_dir.join("api/llm_enrichment_template.json"),
            "semantic_template": workspace_dir.join("api/semantic_template.json"),
            "sources_dir": workspace_dir.join("sources"),
        },
        "endpoints": {
            "api": format!("/ctox/scrape/targets/{}/api", target.target_key),
            "records": format!("/ctox/scrape/targets/{}/records", target.target_key),
            "semantic": format!("/ctox/scrape/targets/{}/semantic", target.target_key),
            "latest": format!("/ctox/scrape/targets/{}/latest", target.target_key),
        },
        "records_query": {
            "mode": "exact-match scalar filters on dot-path fields",
            "filter_fields": filter_paths,
            "examples": [
                format!("/ctox/scrape/targets/{}/records?limit=20", target.target_key),
                format!("/ctox/scrape/targets/{}/records?title=Rust%20Engineer", target.target_key),
                format!("/ctox/scrape/targets/{}/records?classification.category=job", target.target_key),
            ],
        },
        "semantic_query": {
            "enabled": semantic.enabled,
            "source_fields": semantic.source_fields,
            "embedding_model": semantic.embedding_model,
            "example": format!("/ctox/scrape/targets/{}/semantic?q=remote%20rust%20jobs&limit=10", target.target_key),
        },
        "llm_enrichment": {
            "template_mode": "prebuilt default templates, editable per target",
            "template_path": workspace_dir.join("api/llm_enrichment_template.json"),
            "enabled": enrichment.enabled,
            "model": enrichment.model,
            "tasks": [
                "classification",
                "structured extraction",
                "summary",
                "semantic synopsis"
            ],
        },
    })
}

fn configured_filter_paths(root: &Path, target: &ScrapeTargetView) -> Vec<String> {
    if let Some(values) = target
        .config
        .get("api")
        .and_then(|value| value.get("filter_fields"))
        .and_then(Value::as_array)
    {
        let paths = values
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if !paths.is_empty() {
            return paths;
        }
    }
    let mut defaults = vec![
        "id".to_string(),
        "url".to_string(),
        "title".to_string(),
        "name".to_string(),
        "source_key".to_string(),
        "source.source_key".to_string(),
        "source.display_name".to_string(),
        "classification.category".to_string(),
        "classification.label".to_string(),
        "source".to_string(),
    ];
    if let Some(values) = target
        .output_schema
        .get("record_key_fields")
        .and_then(Value::as_array)
    {
        for field in values.iter().filter_map(Value::as_str) {
            if !defaults.iter().any(|item| item == field) {
                defaults.push(field.to_string());
            }
        }
    }
    let enrichment = load_llm_enrichment_config(root, target);
    for path in enrichment_filter_paths(&enrichment) {
        if !defaults.iter().any(|item| item == &path) {
            defaults.push(path);
        }
    }
    defaults
}

fn default_semantic_config_for_target(target: &ScrapeTargetView) -> SemanticConfig {
    let api_config = target.config.get("api");
    let semantic_config = api_config.and_then(|value| value.get("semantic"));
    let enabled = semantic_config
        .and_then(|value| value.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let source_fields = semantic_config
        .and_then(|value| value.get("source_fields"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|items| !items.is_empty())
        .unwrap_or_else(|| {
            vec![
                "title".to_string(),
                "name".to_string(),
                "summary".to_string(),
                "description".to_string(),
                "content".to_string(),
                "text".to_string(),
                "semantic_summary".to_string(),
                "classification.label".to_string(),
            ]
        });
    let embedding_model = semantic_config
        .and_then(|value| value.get("embedding_model"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_embedding_model().to_string());
    let default_limit = semantic_config
        .and_then(|value| value.get("default_limit"))
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(12);
    SemanticConfig {
        enabled,
        source_fields,
        embedding_model,
        default_limit,
    }
}

fn load_semantic_config(root: &Path, target: &ScrapeTargetView) -> SemanticConfig {
    let default = default_semantic_config_for_target(target);
    let path = target_api_dir(root, target).join("semantic_template.json");
    let Ok(raw) = fs::read_to_string(path) else {
        return default;
    };
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return default;
    };
    let enabled = value
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(default.enabled);
    let source_fields = value
        .get("source_fields")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|items| !items.is_empty())
        .unwrap_or(default.source_fields);
    let embedding_model = value
        .get("embedding_model")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or(default.embedding_model);
    let default_limit = value
        .get("default_limit")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(default.default_limit);
    SemanticConfig {
        enabled,
        source_fields,
        embedding_model,
        default_limit,
    }
}

fn default_llm_enrichment_config(root: &Path, _target: &ScrapeTargetView) -> EnrichmentConfig {
    EnrichmentConfig {
        enabled: false,
        model: runtime_state::load_or_resolve_runtime_state(root)
            .ok()
            .and_then(|state| state.active_or_selected_model().map(ToOwned::to_owned))
            .unwrap_or_else(runtime_state::default_primary_model),
        timeout_seconds: 45,
        max_records: DEFAULT_ENRICHMENT_MAX_RECORDS,
        source_fields: vec![
            "title".to_string(),
            "name".to_string(),
            "summary".to_string(),
            "description".to_string(),
            "content".to_string(),
            "text".to_string(),
            "url".to_string(),
        ],
        tasks: vec![
            EnrichmentTaskConfig {
                kind: "classify".to_string(),
                output_field: "classification".to_string(),
                instruction: "Classify the record into stable API-facing categories and operator labels.".to_string(),
                field_hints: vec!["category".to_string(), "label".to_string()],
                filter_field_hints: vec![
                    "classification.category".to_string(),
                    "classification.label".to_string(),
                ],
            },
            EnrichmentTaskConfig {
                kind: "extract".to_string(),
                output_field: "structured".to_string(),
                instruction: "Extract stable structured fields that should be filterable in the default API.".to_string(),
                field_hints: vec![
                    "company".to_string(),
                    "location".to_string(),
                    "employment_type".to_string(),
                    "remote".to_string(),
                    "seniority".to_string(),
                ],
                filter_field_hints: vec![
                    "structured.company".to_string(),
                    "structured.location".to_string(),
                    "structured.employment_type".to_string(),
                    "structured.remote".to_string(),
                    "structured.seniority".to_string(),
                ],
            },
            EnrichmentTaskConfig {
                kind: "summarize".to_string(),
                output_field: "semantic_summary".to_string(),
                instruction: "Write a compact semantic synopsis optimized for retrieval and operator overview.".to_string(),
                field_hints: Vec::new(),
                filter_field_hints: Vec::new(),
            },
        ],
    }
}

fn load_llm_enrichment_config(root: &Path, target: &ScrapeTargetView) -> EnrichmentConfig {
    let default = default_llm_enrichment_config(root, target);
    let path = target_api_dir(root, target).join("llm_enrichment_template.json");
    let Ok(raw) = fs::read_to_string(path) else {
        return default;
    };
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return default;
    };
    let enabled = value
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(default.enabled);
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or(default.model);
    let timeout_seconds = value
        .get("timeout_seconds")
        .and_then(Value::as_u64)
        .unwrap_or(default.timeout_seconds);
    let max_records = value
        .get("max_records")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(default.max_records);
    let source_fields = value
        .get("source_fields")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|items| !items.is_empty())
        .unwrap_or(default.source_fields);
    let tasks = value
        .get("tasks")
        .cloned()
        .and_then(|raw_tasks| serde_json::from_value::<Vec<EnrichmentTaskConfig>>(raw_tasks).ok())
        .filter(|items| !items.is_empty())
        .unwrap_or(default.tasks);
    EnrichmentConfig {
        enabled,
        model,
        timeout_seconds,
        max_records,
        source_fields,
        tasks,
    }
}

fn enrichment_filter_paths(config: &EnrichmentConfig) -> Vec<String> {
    let mut out = Vec::new();
    for task in &config.tasks {
        for path in &task.filter_field_hints {
            if !out.iter().any(|item| item == path) {
                out.push(path.clone());
            }
        }
        if !task.output_field.trim().is_empty()
            && !out.iter().any(|item| item == &task.output_field)
            && task.kind.eq_ignore_ascii_case("classify")
        {
            out.push(task.output_field.clone());
        }
    }
    out
}

fn normalize_target_config(start_url: &str, target_key: &str, raw: &Value) -> Value {
    let mut object = raw.as_object().cloned().unwrap_or_default();
    let sources = normalize_sources_from_config(object.get("sources"), start_url, target_key);
    object.insert(
        "sources".to_string(),
        serde_json::to_value(&sources).unwrap_or_else(|_| json!([])),
    );
    Value::Object(object)
}

fn normalize_sources_from_config(
    raw_sources: Option<&Value>,
    start_url: &str,
    target_key: &str,
) -> Vec<ScrapeSourceDefinition> {
    let mut out = Vec::new();
    let mut seen = BTreeMap::new();
    if let Some(items) = raw_sources.and_then(Value::as_array) {
        for (index, item) in items.iter().enumerate() {
            let Some(object) = item.as_object() else {
                continue;
            };
            let source_start_url = object
                .get("start_url")
                .or_else(|| object.get("url"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(start_url)
                .to_string();
            let mut source_key = slugify(
                object
                    .get("source_key")
                    .and_then(Value::as_str)
                    .or_else(|| object.get("display_name").and_then(Value::as_str))
                    .unwrap_or(&source_start_url),
            );
            if source_key.is_empty() {
                source_key = format!("{target_key}-source-{}", index + 1);
            }
            if seen.contains_key(&source_key) {
                continue;
            }
            let display_name = object
                .get("display_name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(&source_key)
                .to_string();
            let source_kind = object
                .get("source_kind")
                .or_else(|| object.get("kind"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("generic")
                .to_string();
            let extraction_module = object
                .get("extraction_module")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| format!("sources/{source_key}/extractor.js"));
            let merge_strategy = object
                .get("merge_strategy")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("upsert_by_record_key")
                .to_string();
            let tags = object
                .get("tags")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let notes = object
                .get("notes")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            let config = object.get("config").cloned().unwrap_or_else(|| json!({}));
            let source = ScrapeSourceDefinition {
                source_key: source_key.clone(),
                display_name,
                start_url: source_start_url,
                source_kind,
                enabled: object
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                extraction_module,
                merge_strategy,
                tags,
                notes,
                config,
            };
            seen.insert(source_key, true);
            out.push(source);
        }
    }
    if out.is_empty() {
        out.push(ScrapeSourceDefinition {
            source_key: "primary".to_string(),
            display_name: "Primary Source".to_string(),
            start_url: start_url.to_string(),
            source_kind: "generic".to_string(),
            enabled: true,
            extraction_module: "sources/primary/extractor.js".to_string(),
            merge_strategy: "upsert_by_record_key".to_string(),
            tags: vec!["primary".to_string()],
            notes: Some("Default synthesized source for single-entry scrape targets.".to_string()),
            config: json!({}),
        });
    }
    out
}

fn target_sources(target: &ScrapeTargetView) -> Vec<ScrapeSourceDefinition> {
    normalize_sources_from_config(
        target.config.get("sources"),
        &target.start_url,
        &target.target_key,
    )
}

fn upsert_target(root: &Path, runtime_root_arg: &str, payload: Value) -> Result<ScrapeTargetView> {
    let object = payload
        .as_object()
        .context("target payload must be a json object")?;
    let start_url = object
        .get("start_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("target payload requires non-empty start_url")?;
    let target_key = slugify(
        object
            .get("target_key")
            .and_then(Value::as_str)
            .or_else(|| object.get("display_name").and_then(Value::as_str))
            .unwrap_or(start_url),
    );
    let display_name = object
        .get("display_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&target_key)
        .to_string();
    let target_kind = object
        .get("target_kind")
        .and_then(Value::as_str)
        .unwrap_or("generic")
        .to_string();
    let status = object
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("active")
        .to_string();
    let schedule_hint = object
        .get("schedule_hint")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let raw_config = object.get("config").cloned().unwrap_or_else(|| json!({}));
    let config = normalize_target_config(start_url, &target_key, &raw_config);
    let output_schema = object
        .get("output_schema")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let target_id = format!("scrape_target-{}", stable_digest(&target_key));
    let runtime_root = resolve_runtime_root(root, runtime_root_arg);
    let workspace_dir = ensure_target_workspace(&runtime_root, &target_key)?;
    let conn = open_db(root)?;
    let existing = conn
        .query_row(
            r#"
            SELECT created_at, latest_script_revision_no, latest_script_sha256
            FROM scrape_target
            WHERE target_key = ?1
            "#,
            params![target_key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()?;
    let created_at = existing
        .as_ref()
        .map(|item| item.0.clone())
        .unwrap_or_else(now_iso_string);
    let latest_script_revision_no = existing.as_ref().and_then(|item| item.1);
    let latest_script_sha256 = existing.as_ref().and_then(|item| item.2.clone());
    let updated_at = now_iso_string();
    conn.execute(
        r#"
        INSERT INTO scrape_target (
            target_id, target_key, display_name, start_url, target_kind, status, schedule_hint,
            config_json, output_schema_json, workspace_dir, latest_script_revision_no,
            latest_script_sha256, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
        ON CONFLICT(target_key) DO UPDATE SET
            display_name = excluded.display_name,
            start_url = excluded.start_url,
            target_kind = excluded.target_kind,
            status = excluded.status,
            schedule_hint = excluded.schedule_hint,
            config_json = excluded.config_json,
            output_schema_json = excluded.output_schema_json,
            workspace_dir = excluded.workspace_dir,
            updated_at = excluded.updated_at
        "#,
        params![
            target_id,
            target_key,
            display_name,
            start_url,
            target_kind,
            status,
            schedule_hint,
            serde_json::to_string(&config)?,
            serde_json::to_string(&output_schema)?,
            workspace_dir.to_string_lossy(),
            latest_script_revision_no,
            latest_script_sha256,
            created_at,
            updated_at,
        ],
    )?;
    let target =
        load_target_view(&conn, &target_key)?.context("failed to reload target after upsert")?;
    write_target_manifest(root, &target)?;
    Ok(target)
}

fn register_script(
    root: &Path,
    runtime_root_arg: &str,
    target_key: &str,
    script_file_arg: &str,
    language: &str,
    change_reason: Option<&str>,
    notes: Option<&str>,
) -> Result<Value> {
    let conn = open_db(root)?;
    let target = load_target_view(&conn, target_key)?.context("target_key not found")?;
    let source_path = resolve_input_path(root, script_file_arg);
    let script_body = fs::read_to_string(&source_path)
        .with_context(|| format!("failed to read script file {}", source_path.display()))?;
    let script_sha256 = compute_sha256(script_body.trim());
    if let Some((revision_no, script_path, created_at)) = conn
        .query_row(
            r#"
            SELECT revision_no, script_path, created_at
            FROM scrape_script_revision
            WHERE target_id = ?1 AND script_sha256 = ?2
            "#,
            params![target.target_id, script_sha256],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?
    {
        return Ok(json!({
            "target_key": target.target_key,
            "target_id": target.target_id,
            "revision_no": revision_no,
            "script_path": script_path,
            "script_sha256": script_sha256,
            "deduplicated": true,
            "created_at": created_at,
        }));
    }
    let next_revision = conn.query_row(
        "SELECT COALESCE(MAX(revision_no), 0) + 1 FROM scrape_script_revision WHERE target_id = ?1",
        params![target.target_id],
        |row| row.get::<_, i64>(0),
    )?;
    let workspace_dir = ensure_target_workspace(
        &resolve_runtime_root(root, runtime_root_arg),
        &target.target_key,
    )?;
    let extension = script_extension(language, &source_path);
    let revision_path = workspace_dir
        .join("scripts")
        .join("revisions")
        .join(format!(
            "rev{next_revision:04}_{}.{}",
            &script_sha256[..8],
            extension.trim_start_matches('.')
        ));
    let current_path = workspace_dir
        .join("scripts")
        .join(format!("current{}", extension));
    fs::copy(&source_path, &revision_path).with_context(|| {
        format!(
            "failed to copy script revision {} -> {}",
            source_path.display(),
            revision_path.display()
        )
    })?;
    fs::copy(&source_path, &current_path).with_context(|| {
        format!(
            "failed to copy current script {} -> {}",
            source_path.display(),
            current_path.display()
        )
    })?;
    let created_at = now_iso_string();
    let entry_command = default_entry_command(language);
    conn.execute(
        r#"
        INSERT INTO scrape_script_revision (
            target_id, revision_no, script_path, language, entry_command_json, script_sha256,
            script_body, change_reason, notes, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            target.target_id,
            next_revision,
            revision_path.to_string_lossy(),
            language,
            serde_json::to_string(&entry_command)?,
            script_sha256,
            script_body,
            change_reason,
            notes,
            created_at,
        ],
    )?;
    conn.execute(
        r#"
        UPDATE scrape_target
        SET latest_script_revision_no = ?2,
            latest_script_sha256 = ?3,
            updated_at = ?4
        WHERE target_id = ?1
        "#,
        params![target.target_id, next_revision, script_sha256, created_at],
    )?;
    let updated_target = load_target_view(&conn, target_key)?
        .context("failed to reload target after script registration")?;
    write_target_manifest(root, &updated_target)?;
    Ok(json!({
        "target_key": updated_target.target_key,
        "target_id": updated_target.target_id,
        "revision_no": next_revision,
        "script_path": revision_path,
        "current_path": current_path,
        "script_sha256": script_sha256,
        "deduplicated": false,
        "created_at": created_at,
    }))
}

fn register_source_module(
    root: &Path,
    runtime_root_arg: &str,
    target_key: &str,
    source_key_raw: &str,
    module_file_arg: &str,
    language: &str,
    change_reason: Option<&str>,
    notes: Option<&str>,
) -> Result<Value> {
    let conn = open_db(root)?;
    let target = load_target_view(&conn, target_key)?.context("target_key not found")?;
    let source_key = slugify(source_key_raw);
    let source = target_sources(&target)
        .into_iter()
        .find(|item| item.source_key == source_key)
        .with_context(|| format!("source_key `{source_key}` not found on target `{target_key}`"))?;
    let source_path = resolve_input_path(root, module_file_arg);
    let module_body = fs::read_to_string(&source_path)
        .with_context(|| format!("failed to read source module {}", source_path.display()))?;
    let module_sha256 = compute_sha256(module_body.trim());
    if let Some((revision_no, module_path, created_at)) = conn
        .query_row(
            r#"
            SELECT revision_no, module_path, created_at
            FROM scrape_source_revision
            WHERE target_id = ?1 AND source_key = ?2 AND module_sha256 = ?3
            "#,
            params![target.target_id, source_key, module_sha256],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?
    {
        return Ok(json!({
            "target_key": target.target_key,
            "target_id": target.target_id,
            "source_key": source.source_key,
            "revision_no": revision_no,
            "module_path": module_path,
            "module_sha256": module_sha256,
            "deduplicated": true,
            "created_at": created_at,
        }));
    }
    let next_revision = conn.query_row(
        "SELECT COALESCE(MAX(revision_no), 0) + 1 FROM scrape_source_revision WHERE target_id = ?1 AND source_key = ?2",
        params![target.target_id, source_key],
        |row| row.get::<_, i64>(0),
    )?;
    let workspace_dir = ensure_target_workspace(
        &resolve_runtime_root(root, runtime_root_arg),
        &target.target_key,
    )?;
    let extension = script_extension(language, &source_path);
    let source_dir = workspace_dir.join("sources").join(&source.source_key);
    fs::create_dir_all(source_dir.join("revisions"))?;
    let revision_path = source_dir.join("revisions").join(format!(
        "rev{next_revision:04}_{}.{}",
        &module_sha256[..8],
        extension.trim_start_matches('.')
    ));
    let current_path = source_dir.join(format!("current{}", extension));
    let configured_path = workspace_dir.join(&source.extraction_module);
    if let Some(parent) = configured_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&source_path, &revision_path).with_context(|| {
        format!(
            "failed to copy source module revision {} -> {}",
            source_path.display(),
            revision_path.display()
        )
    })?;
    fs::copy(&source_path, &current_path).with_context(|| {
        format!(
            "failed to copy source module {} -> {}",
            source_path.display(),
            current_path.display()
        )
    })?;
    if configured_path != current_path {
        fs::copy(&source_path, &configured_path).with_context(|| {
            format!(
                "failed to copy source module {} -> {}",
                source_path.display(),
                configured_path.display()
            )
        })?;
    }
    let created_at = now_iso_string();
    conn.execute(
        r#"
        INSERT INTO scrape_source_revision (
            target_id, source_key, revision_no, module_path, language, module_sha256,
            module_body, change_reason, notes, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            target.target_id,
            source.source_key,
            next_revision,
            revision_path.to_string_lossy(),
            language,
            module_sha256,
            module_body,
            change_reason,
            notes,
            created_at,
        ],
    )?;
    write_target_manifest(root, &target)?;
    Ok(json!({
        "target_key": target.target_key,
        "target_id": target.target_id,
        "source_key": source.source_key,
        "revision_no": next_revision,
        "module_path": revision_path,
        "current_path": current_path,
        "configured_path": configured_path,
        "module_sha256": module_sha256,
        "deduplicated": false,
        "created_at": created_at,
    }))
}

fn record_template_example(
    root: &Path,
    target_key: &str,
    template_key_raw: &str,
    script_file_arg: &str,
    language: &str,
    result_count: Option<i64>,
    challenge_score: i64,
    nomination_reason: Option<&str>,
) -> Result<Value> {
    let conn = open_db(root)?;
    let target = load_target_view(&conn, target_key)?.context("target_key not found")?;
    let script_path = resolve_input_path(root, script_file_arg);
    let script_body = fs::read_to_string(&script_path)
        .with_context(|| format!("failed to read script file {}", script_path.display()))?;
    let script_sha256 = compute_sha256(script_body.trim());
    let template_key = slugify(template_key_raw);
    let now = now_iso_string();
    if let Some((existing_result_count, existing_challenge)) = conn
        .query_row(
            r#"
            SELECT result_count, challenge_score
            FROM scrape_template_example
            WHERE template_key = ?1 AND target_id = ?2 AND script_sha256 = ?3
            "#,
            params![template_key, target.target_id, script_sha256],
            |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?
    {
        let merged_result_count = match (existing_result_count, result_count) {
            (Some(left), Some(right)) => Some(left.max(right)),
            (Some(left), None) => Some(left),
            (None, Some(right)) => Some(right),
            (None, None) => None,
        };
        conn.execute(
            r#"
            UPDATE scrape_template_example
            SET script_body = ?4,
                language = ?5,
                result_count = ?6,
                challenge_score = ?7,
                nomination_reason = ?8,
                updated_at = ?9
            WHERE template_key = ?1 AND target_id = ?2 AND script_sha256 = ?3
            "#,
            params![
                template_key,
                target.target_id,
                script_sha256,
                script_body,
                language,
                merged_result_count,
                existing_challenge.max(challenge_score.clamp(0, 3)),
                nomination_reason,
                now,
            ],
        )?;
    } else {
        conn.execute(
            r#"
            INSERT INTO scrape_template_example (
                template_key, target_id, script_sha256, script_body, language, result_count,
                challenge_score, nomination_reason, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
            "#,
            params![
                template_key,
                target.target_id,
                script_sha256,
                script_body,
                language,
                result_count,
                challenge_score.clamp(0, 3),
                nomination_reason,
                now,
            ],
        )?;
    }
    let aggregate = conn.query_row(
        r#"
        SELECT
            COUNT(*) AS example_count,
            COUNT(DISTINCT target_id) AS target_count,
            MAX(COALESCE(result_count, 0)) AS best_result_count,
            MAX(challenge_score) AS best_challenge_score
        FROM scrape_template_example
        WHERE template_key = ?1 AND script_sha256 = ?2
        "#,
        params![template_key, script_sha256],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        },
    )?;
    let (promoted, promotion_reason) =
        should_auto_promote_template(&script_body, aggregate.2, aggregate.1, aggregate.3);
    if promoted {
        upsert_promoted_template(
            &conn,
            &template_key,
            &script_sha256,
            &script_body,
            language,
            aggregate.0,
            aggregate.1,
            aggregate.2,
            &promotion_reason,
        )?;
    }
    Ok(json!({
        "template_key": template_key,
        "target_key": target.target_key,
        "script_sha256": script_sha256,
        "example_count": aggregate.0,
        "target_count": aggregate.1,
        "best_result_count": aggregate.2,
        "best_challenge_score": aggregate.3,
        "promoted": promoted,
        "promotion_reason": promotion_reason,
    }))
}

fn promote_template(
    root: &Path,
    template_key_raw: &str,
    script_file_arg: &str,
    language: &str,
    reason: &str,
) -> Result<Value> {
    let conn = open_db(root)?;
    let template_key = slugify(template_key_raw);
    let script_path = resolve_input_path(root, script_file_arg);
    let script_body = fs::read_to_string(&script_path)
        .with_context(|| format!("failed to read script file {}", script_path.display()))?;
    let script_sha256 = compute_sha256(script_body.trim());
    let aggregate = conn
        .query_row(
            r#"
            SELECT
                COUNT(*) AS example_count,
                COUNT(DISTINCT target_id) AS target_count,
                MAX(COALESCE(result_count, 0)) AS best_result_count
            FROM scrape_template_example
            WHERE template_key = ?1 AND script_sha256 = ?2
            "#,
            params![template_key, script_sha256],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .unwrap_or((0, 0, 0));
    upsert_promoted_template(
        &conn,
        &template_key,
        &script_sha256,
        &script_body,
        language,
        aggregate.0.max(1),
        aggregate.1.max(1),
        aggregate.2,
        reason,
    )?;
    Ok(json!({
        "template_key": template_key,
        "script_sha256": script_sha256,
        "source_example_count": aggregate.0.max(1),
        "source_target_count": aggregate.1.max(1),
        "best_result_count": aggregate.2,
        "promotion_reason": reason,
    }))
}

fn maybe_record_template_from_target(
    root: &Path,
    target: &RegisteredTarget,
    records_found: i64,
) -> Result<Option<Value>> {
    let template_key = target
        .view
        .config
        .get("template_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(template_key) = template_key else {
        return Ok(None);
    };
    let challenge_score = target
        .view
        .config
        .get("template_challenge_score")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    Ok(Some(record_template_example(
        root,
        &target.view.target_key,
        template_key,
        &target.script.script_path,
        &target.script.language,
        Some(records_found),
        challenge_score,
        Some("successful_ctox_execute"),
    )?))
}

fn open_db(root: &Path) -> Result<Connection> {
    let path = resolve_db_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create scrape db parent {}", parent.display()))?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open scrape db {}", path.display()))?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

fn resolve_db_path(root: &Path) -> PathBuf {
    root.join(DEFAULT_DB_RELATIVE_PATH)
}

fn resolve_runtime_root(root: &Path, runtime_root_arg: &str) -> PathBuf {
    let path = PathBuf::from(runtime_root_arg);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn resolve_input_path(root: &Path, raw: &str) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn resolve_workspace_dir(root: &Path, raw: &str) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn ensure_target_workspace(runtime_root: &Path, target_key: &str) -> Result<PathBuf> {
    let workspace = runtime_root.join("targets").join(target_key);
    fs::create_dir_all(workspace.join("scripts").join("revisions"))?;
    fs::create_dir_all(workspace.join("sources"))?;
    fs::create_dir_all(workspace.join("runs"))?;
    Ok(workspace)
}

fn load_json_file(root: &Path, raw: &str) -> Result<Value> {
    let path = resolve_input_path(root, raw);
    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed to read json file {}", path.display()))?;
    serde_json::from_str(&text)
        .with_context(|| format!("failed to parse json from {}", path.display()))
}

fn load_target_view(conn: &Connection, target_key: &str) -> Result<Option<ScrapeTargetView>> {
    conn.query_row(
        r#"
        SELECT target_id, target_key, display_name, start_url, target_kind, status, schedule_hint,
               config_json, output_schema_json, workspace_dir, latest_script_revision_no,
               latest_script_sha256, created_at, updated_at
        FROM scrape_target
        WHERE target_key = ?1
        "#,
        params![target_key],
        map_target_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn load_registered_target(
    root: &Path,
    conn: &Connection,
    target_key: &str,
) -> Result<Option<RegisteredTarget>> {
    let Some(view) = load_target_view(conn, target_key)? else {
        return Ok(None);
    };
    let script = conn
        .query_row(
            r#"
            SELECT revision_no, script_path, language, entry_command_json, script_sha256
            FROM scrape_script_revision
            WHERE target_id = ?1
            ORDER BY revision_no DESC
            LIMIT 1
            "#,
            params![view.target_id],
            |row| {
                let language: String = row.get(2)?;
                let entry_command_text: String = row.get(3)?;
                let entry_command = serde_json::from_str::<Vec<String>>(&entry_command_text)
                    .unwrap_or_else(|_| default_entry_command(&language));
                Ok(ScrapeScriptRevisionRecord {
                    revision_no: row.get(0)?,
                    script_path: row.get(1)?,
                    language,
                    entry_command,
                    script_sha256: row.get(4)?,
                })
            },
        )
        .optional()?;
    let workspace_root = resolve_workspace_dir(root, &view.workspace_dir);
    Ok(script.map(|script| RegisteredTarget {
        view,
        script,
        workspace_root,
    }))
}

fn load_script_revisions(
    conn: &Connection,
    target_id: &str,
) -> Result<Vec<ScrapeScriptRevisionView>> {
    let mut statement = conn.prepare(
        r#"
        SELECT revision_no, script_path, language, script_sha256, change_reason, notes, created_at
        FROM scrape_script_revision
        WHERE target_id = ?1
        ORDER BY revision_no DESC
        LIMIT 20
        "#,
    )?;
    let rows = statement.query_map(params![target_id], |row| {
        Ok(ScrapeScriptRevisionView {
            revision_no: row.get(0)?,
            script_path: row.get(1)?,
            language: row.get(2)?,
            script_sha256: row.get(3)?,
            change_reason: row.get(4)?,
            notes: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn load_source_revisions(
    conn: &Connection,
    target_id: &str,
) -> Result<Vec<ScrapeSourceRevisionView>> {
    let mut statement = conn.prepare(
        r#"
        SELECT source_key, revision_no, module_path, language, module_sha256, change_reason, notes, created_at
        FROM scrape_source_revision
        WHERE target_id = ?1
        ORDER BY source_key ASC, revision_no DESC
        LIMIT 100
        "#,
    )?;
    let rows = statement.query_map(params![target_id], |row| {
        Ok(ScrapeSourceRevisionView {
            source_key: row.get(0)?,
            revision_no: row.get(1)?,
            module_path: row.get(2)?,
            language: row.get(3)?,
            module_sha256: row.get(4)?,
            change_reason: row.get(5)?,
            notes: row.get(6)?,
            created_at: row.get(7)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn latest_source_revision_map(
    conn: &Connection,
    target_id: &str,
) -> Result<BTreeMap<String, ScrapeSourceRevisionView>> {
    let mut out = BTreeMap::new();
    for revision in load_source_revisions(conn, target_id)? {
        out.entry(revision.source_key.clone()).or_insert(revision);
    }
    Ok(out)
}

fn map_target_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScrapeTargetView> {
    let config_text: String = row.get(7)?;
    let output_schema_text: String = row.get(8)?;
    Ok(ScrapeTargetView {
        target_id: row.get(0)?,
        target_key: row.get(1)?,
        display_name: row.get(2)?,
        start_url: row.get(3)?,
        target_kind: row.get(4)?,
        status: row.get(5)?,
        schedule_hint: row.get(6)?,
        config: serde_json::from_str(&config_text).unwrap_or_else(|_| json!({})),
        output_schema: serde_json::from_str(&output_schema_text).unwrap_or_else(|_| json!({})),
        workspace_dir: row.get(9)?,
        latest_script_revision_no: row.get(10)?,
        latest_script_sha256: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn count_rows(conn: &Connection, table: &str) -> Result<i64> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    conn.query_row(&sql, [], |row| row.get::<_, i64>(0))
        .map_err(anyhow::Error::from)
}

fn count_filtered_rows(conn: &Connection, table: &str, condition: &str) -> Result<i64> {
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE {condition}");
    conn.query_row(&sql, [], |row| row.get::<_, i64>(0))
        .map_err(anyhow::Error::from)
}

fn write_target_manifest(root: &Path, target: &ScrapeTargetView) -> Result<()> {
    let workspace_dir = resolve_workspace_dir(root, &target.workspace_dir);
    fs::create_dir_all(&workspace_dir)?;
    fs::write(
        workspace_dir.join("manifest.json"),
        serde_json::to_string_pretty(target)?,
    )?;
    write_target_api_files(root, target)?;
    write_target_source_files(root, target)?;
    Ok(())
}

fn target_api_dir(root: &Path, target: &ScrapeTargetView) -> PathBuf {
    resolve_workspace_dir(root, &target.workspace_dir).join("api")
}

fn write_target_api_files(root: &Path, target: &ScrapeTargetView) -> Result<()> {
    let api_dir = target_api_dir(root, target);
    fs::create_dir_all(&api_dir)?;
    let contract = build_target_api_contract(root, target);
    fs::write(
        api_dir.join("api_contract.json"),
        serde_json::to_string_pretty(&contract)?,
    )?;
    let semantic_default = default_semantic_config_for_target(target);
    write_json_file_if_missing(
        &api_dir.join("semantic_template.json"),
        &json!({
            "enabled": semantic_default.enabled,
            "embedding_model": semantic_default.embedding_model,
            "source_fields": semantic_default.source_fields,
            "default_limit": semantic_default.default_limit,
            "notes": "Adjust source_fields if semantic retrieval should focus only on specific record fragments."
        }),
    )?;
    let enrichment_default = default_llm_enrichment_config(root, target);
    write_json_file_if_missing(
        &api_dir.join("llm_enrichment_template.json"),
        &json!({
            "pipeline_name": "default_scrape_enrichment",
            "enabled": enrichment_default.enabled,
            "model": enrichment_default.model,
            "timeout_seconds": enrichment_default.timeout_seconds,
            "max_records": enrichment_default.max_records,
            "source_fields": enrichment_default.source_fields,
            "description": "Template for optional post-scrape LLM enrichment. Agent may edit this target-local file instead of reinventing the pipeline.",
            "tasks": enrichment_default.tasks,
            "response_contract": {
                "type": "json_object",
                "shape": {
                    "updates": [
                        {
                            "path": "classification.category",
                            "value": "job"
                        }
                    ]
                }
            }
        }),
    )?;
    let readme = format!(
        "# Scrape API for {target_key}\n\n\
This target exposes a default CTOX scrape API surface.\n\n\
Sources:\n\
- first-class source definitions live in `manifest.json` under `config.sources`\n\
- per-source modules and notes live under `sources/<source_key>/`\n\
\n\
Endpoints:\n\
- `/ctox/scrape/targets/{target_key}/api`\n\
- `/ctox/scrape/targets/{target_key}/records`\n\
- `/ctox/scrape/targets/{target_key}/semantic`\n\
\n\
Hard filters:\n\
- pass scalar query params as exact-match filters\n\
- nested fields use dot paths, e.g. `classification.category=job`\n\
\n\
Semantic search:\n\
- query with `q=<text>`\n\
- semantic source fields and embedding model are configured in `semantic_template.json`\n\
\n\
LLM enrichment:\n\
- `llm_enrichment_template.json` is the default per-target postprocessing template\n\
- the agent may copy and specialize it instead of inventing a pipeline from scratch each time\n",
        target_key = target.target_key
    );
    fs::write(api_dir.join("README.md"), readme)?;
    Ok(())
}

fn write_target_source_files(root: &Path, target: &ScrapeTargetView) -> Result<()> {
    let sources_dir = resolve_workspace_dir(root, &target.workspace_dir).join("sources");
    fs::create_dir_all(&sources_dir)?;
    let sources = target_sources(target);
    fs::write(
        sources_dir.join("sources_manifest.json"),
        serde_json::to_string_pretty(&sources)?,
    )?;
    for source in sources {
        let source_dir = sources_dir.join(&source.source_key);
        fs::create_dir_all(source_dir.join("revisions"))?;
        fs::write(
            source_dir.join("source.json"),
            serde_json::to_string_pretty(&source)?,
        )?;
        let readme = format!(
            "# Source {source_key}\n\n\
Display name: {display_name}\n\
Start URL: {start_url}\n\
Kind: {source_kind}\n\
Enabled: {enabled}\n\
Extraction module: `{extraction_module}`\n\
Merge strategy: `{merge_strategy}`\n\
\n\
Use this folder for source-specific extraction helpers, prompts, notes, and repair evidence.\n\
The main registered script may import or call this module instead of carrying all source logic inline.\n\
Register concrete module revisions with `ctox scrape register-source-module --target-key {target_key} --source-key {source_key} --module-file <path>` so source-local changes stay inspectable and reversible.\n",
            target_key = target.target_key,
            source_key = source.source_key,
            display_name = source.display_name,
            start_url = source.start_url,
            source_kind = source.source_kind,
            enabled = source.enabled,
            extraction_module = source.extraction_module,
            merge_strategy = source.merge_strategy,
        );
        fs::write(source_dir.join("README.md"), readme)?;
        let extractor_path =
            resolve_workspace_dir(root, &target.workspace_dir).join(&source.extraction_module);
        if let Some(parent) = extractor_path.parent() {
            fs::create_dir_all(parent)?;
        }
        if !extractor_path.exists() {
            let scaffold = format!(
                "module.exports = async function extractSource(context) {{\n  return {{\n    source_key: \"{source_key}\",\n    fetched_from: context.source.start_url,\n    records: []\n  }};\n}};\n",
                source_key = source.source_key
            );
            fs::write(&extractor_path, scaffold)?;
        }
    }
    Ok(())
}

fn write_json_file_if_missing(path: &Path, value: &Value) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    fs::write(path, serde_json::to_string_pretty(value)?)
        .with_context(|| format!("failed to write {}", path.display()))
}

fn build_repair_prompt(
    root: &Path,
    target: &RegisteredTarget,
    run_id: &str,
    status: FailureStatus,
    records_found: i64,
    repair_request_path: Option<&PathBuf>,
) -> String {
    let repair_request = repair_request_path
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default();
    let source_keys = target_sources(&target.view)
        .into_iter()
        .map(|source| source.source_key)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Repair CTOX scrape target `{}` in workspace `{}`. Read the repair bundle at `{}` first. This was run `{}` with status `{}` and records_found `{}`. The configured sources are: [{}]. Check `runtime/scraping/targets/{}/sources/` for source-specific modules and notes before changing the root script. If the failure is real portal drift or partial extraction, revise the target script under `runtime/scraping/targets/{}/scripts/` and/or the affected source module under `runtime/scraping/targets/{}/sources/<source_key>/`. Register root changes with `ctox scrape register-script --target-key {} --script-file <path> --change-reason script_relearned` and source-local changes with `ctox scrape register-source-module --target-key {} --source-key <source_key> --module-file <path> --change-reason source_relearned`, then rerun `ctox scrape execute --target-key {} --trigger-kind repair --allow-heal`. Do not rewrite the script if the evidence shows only temporary upstream downtime or blocking.",
        target.view.target_key,
        root.display(),
        repair_request,
        run_id,
        status.as_str(),
        records_found,
        source_keys,
        target.view.target_key,
        target.view.target_key,
        target.view.target_key,
        target.view.target_key,
        target.view.target_key,
        target.view.target_key,
    )
}

fn execute_registered_script(
    target: &RegisteredTarget,
    run_dir: &Path,
    output_dir: &Path,
    timeout_seconds: u64,
    input_json: Option<&str>,
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
    if let Some(text) = input_json {
        child.env("CTOX_SCRAPE_INPUT_JSON", text);
    }
    child
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

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
            let _ = child.kill();
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

fn parse_execution_payload(stdout_text: &str) -> Result<Value> {
    let trimmed = stdout_text.trim();
    if trimmed.is_empty() {
        return Ok(json!({}));
    }
    let value: Value =
        serde_json::from_str(trimmed).context("scrape script stdout must be valid json")?;
    if value.is_array() {
        Ok(json!({ "records": value }))
    } else {
        Ok(value)
    }
}

fn normalize_records(payload: &Value) -> Option<Vec<Value>> {
    if let Some(items) = payload.as_array() {
        return Some(items.clone());
    }
    let object = payload.as_object()?;
    for key in ["records", "jobs", "items"] {
        if let Some(items) = object.get(key).and_then(Value::as_array) {
            return Some(items.clone());
        }
    }
    if let Some(result) = object.get("result") {
        return normalize_records(result);
    }
    None
}

fn classify_outcome(
    payload: &Value,
    probe: &ProbeResult,
    execution: &CommandExecution,
    records_found: i64,
    expected_min_records: i64,
) -> Classification {
    let explicit_failure = payload
        .get("failure_mode")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if explicit_failure == "temporary_unreachable" {
        return Classification {
            status: FailureStatus::TemporaryUnreachable,
            should_queue_repair: false,
            reason: "explicit_failure_mode_temporary_unreachable".to_string(),
        };
    }
    if explicit_failure == "portal_drift" {
        return Classification {
            status: FailureStatus::PortalDrift,
            should_queue_repair: true,
            reason: "explicit_failure_mode_portal_drift".to_string(),
        };
    }
    if explicit_failure == "blocked" {
        return Classification {
            status: FailureStatus::Blocked,
            should_queue_repair: false,
            reason: "explicit_failure_mode_blocked".to_string(),
        };
    }
    if explicit_failure == "partial_output"
        || payload.get("partial_output") == Some(&Value::Bool(true))
    {
        return Classification {
            status: FailureStatus::PartialOutput,
            should_queue_repair: true,
            reason: "payload_marked_partial_output".to_string(),
        };
    }

    let lower = format!(
        "{}\n{}",
        execution.stderr_text,
        probe.error.as_deref().unwrap_or("")
    )
    .to_lowercase();
    if probe.human_verification || matches!(probe.status_code, Some(401 | 403)) {
        return Classification {
            status: FailureStatus::Blocked,
            should_queue_repair: false,
            reason: probe
                .error
                .clone()
                .unwrap_or_else(|| format!("http_{}", probe.status_code.unwrap_or_default())),
        };
    }
    if probe.status_code == Some(404) {
        return Classification {
            status: FailureStatus::PortalDrift,
            should_queue_repair: true,
            reason: "http_404".to_string(),
        };
    }
    if matches!(probe.status_code, Some(429))
        || probe.status_code.map(|code| code >= 500).unwrap_or(false)
    {
        return Classification {
            status: FailureStatus::TemporaryUnreachable,
            should_queue_repair: false,
            reason: format!("http_{}", probe.status_code.unwrap_or_default()),
        };
    }
    if !probe.reachable {
        return Classification {
            status: FailureStatus::TemporaryUnreachable,
            should_queue_repair: false,
            reason: probe
                .error
                .clone()
                .unwrap_or_else(|| "portal_unreachable".to_string()),
        };
    }
    if execution.timed_out || contains_transient_hint(&lower) {
        return Classification {
            status: FailureStatus::TemporaryUnreachable,
            should_queue_repair: false,
            reason: if execution.timed_out {
                "command_timed_out".to_string()
            } else {
                "transient_error_hint".to_string()
            },
        };
    }
    if expected_min_records > 0 && records_found > 0 && records_found < expected_min_records {
        return Classification {
            status: FailureStatus::PartialOutput,
            should_queue_repair: true,
            reason: format!(
                "records_found_below_expected_min:{}<{}",
                records_found, expected_min_records
            ),
        };
    }
    if execution.exit_code.unwrap_or(0) != 0 {
        return Classification {
            status: FailureStatus::PortalDrift,
            should_queue_repair: true,
            reason: format!("command_failed_exit_{:?}", execution.exit_code),
        };
    }
    if records_found == 0 {
        return Classification {
            status: FailureStatus::PortalDrift,
            should_queue_repair: true,
            reason: "empty_record_set_on_reachable_portal".to_string(),
        };
    }
    Classification {
        status: FailureStatus::Succeeded,
        should_queue_repair: false,
        reason: "ok".to_string(),
    }
}

fn build_run_artifacts(
    run_dir: &Path,
    output_dir: &Path,
    payload: &Value,
    records: Option<&[Value]>,
    execution: &CommandExecution,
    default_schema_key: Option<&str>,
) -> Result<Vec<ScrapeArtifactRecord>> {
    fs::create_dir_all(output_dir)?;
    let result_path = output_dir.join("result.json");
    fs::write(&result_path, serde_json::to_string_pretty(payload)?)?;

    let stdout_path = output_dir.join("stdout.txt");
    if !execution.stdout_text.is_empty() {
        fs::write(&stdout_path, &execution.stdout_text)?;
    }
    let stderr_path = output_dir.join("stderr.txt");
    if !execution.stderr_text.is_empty() {
        fs::write(&stderr_path, &execution.stderr_text)?;
    }

    let mut artifacts = vec![artifact_record("result_json", &result_path, None, None)?];
    if result_path != run_dir.join("result.json") {
        // no-op, keeps run_dir referenced for future extension
    }
    if let Some(items) = records {
        let records_path = output_dir.join("records.json");
        fs::write(&records_path, serde_json::to_string_pretty(items)?)?;
        let schema_key = payload
            .get("schema_key")
            .and_then(Value::as_str)
            .or(default_schema_key)
            .map(ToOwned::to_owned);
        artifacts.push(artifact_record(
            "records_json",
            &records_path,
            schema_key.as_deref(),
            Some(items.len() as i64),
        )?);
    }
    if stdout_path.is_file() {
        artifacts.push(artifact_record("stdout_text", &stdout_path, None, None)?);
    }
    if stderr_path.is_file() {
        artifacts.push(artifact_record("stderr_text", &stderr_path, None, None)?);
    }
    Ok(artifacts)
}

fn maybe_run_llm_enrichment(
    root: &Path,
    target: &RegisteredTarget,
    records: &[Value],
    output_dir: &Path,
) -> Result<EnrichmentOutcome> {
    let config = load_llm_enrichment_config(root, &target.view);
    if records.is_empty() {
        return Ok(EnrichmentOutcome {
            records: Vec::new(),
            summary: json!({
                "enabled": config.enabled,
                "status": "skipped_empty_input",
                "model": config.model,
                "total_records": 0,
                "applied_count": 0,
                "failed_count": 0,
                "skipped_count": 0,
            }),
            artifacts: Vec::new(),
        });
    }
    if !config.enabled || config.tasks.is_empty() {
        return Ok(EnrichmentOutcome {
            records: records.to_vec(),
            summary: json!({
                "enabled": config.enabled,
                "status": "disabled",
                "model": config.model,
                "total_records": records.len(),
                "applied_count": 0,
                "failed_count": 0,
                "skipped_count": records.len(),
            }),
            artifacts: Vec::new(),
        });
    }

    let mut enriched_records = Vec::with_capacity(records.len());
    let mut report_items = Vec::new();
    let mut applied_count = 0usize;
    let mut failed_count = 0usize;
    let mut skipped_count = 0usize;
    let process_limit = if config.max_records == 0 {
        records.len()
    } else {
        config.max_records.min(records.len())
    };

    for (index, record) in records.iter().enumerate() {
        if index >= process_limit {
            skipped_count += 1;
            enriched_records.push(record.clone());
            report_items.push(json!({
                "index": index,
                "status": "skipped_limit",
            }));
            continue;
        }
        match enrich_single_record(root, &config, record) {
            Ok((updated_record, response, raw_text)) => {
                applied_count += 1;
                let update_paths = response
                    .updates
                    .iter()
                    .map(|item| item.path.clone())
                    .collect::<Vec<_>>();
                enriched_records.push(updated_record);
                report_items.push(json!({
                    "index": index,
                    "status": "applied",
                    "update_count": response.updates.len(),
                    "update_paths": update_paths,
                    "notes": response.notes,
                    "response_excerpt": tail_excerpt(&raw_text, 1200),
                }));
            }
            Err(error) => {
                failed_count += 1;
                enriched_records.push(record.clone());
                report_items.push(json!({
                    "index": index,
                    "status": "failed",
                    "error": error.to_string(),
                }));
            }
        }
    }

    let enriched_records_path = output_dir.join("enriched_records.json");
    fs::write(
        &enriched_records_path,
        serde_json::to_string_pretty(&enriched_records)?,
    )?;
    let report = json!({
        "enabled": true,
        "status": if failed_count == 0 { "applied" } else if applied_count == 0 { "failed" } else { "partial" },
        "model": config.model,
        "timeout_seconds": config.timeout_seconds,
        "max_records": config.max_records,
        "source_fields": config.source_fields,
        "tasks": config.tasks,
        "total_records": records.len(),
        "processed_records": process_limit,
        "applied_count": applied_count,
        "failed_count": failed_count,
        "skipped_count": skipped_count,
        "report_items": report_items,
        "enriched_records_path": enriched_records_path,
    });
    let report_path = output_dir.join("enrichment_report.json");
    fs::write(&report_path, serde_json::to_string_pretty(&report)?)?;

    Ok(EnrichmentOutcome {
        records: enriched_records,
        summary: report.clone(),
        artifacts: vec![
            artifact_record(
                "enriched_records_json",
                &enriched_records_path,
                target
                    .view
                    .output_schema
                    .get("schema_key")
                    .and_then(Value::as_str),
                Some(records.len() as i64),
            )?,
            artifact_record("enrichment_report_json", &report_path, None, None)?,
        ],
    })
}

fn enrich_single_record(
    root: &Path,
    config: &EnrichmentConfig,
    record: &Value,
) -> Result<(Value, EnrichmentResponse, String)> {
    let prompt = build_enrichment_prompt(config, record);
    let raw_text = invoke_responses_text(root, &config.model, &prompt, config.timeout_seconds)?;
    let response = parse_enrichment_response(&raw_text)?;
    let updated = apply_enrichment_updates(record, config, &response.updates)?;
    Ok((updated, response, raw_text))
}

fn build_enrichment_prompt(config: &EnrichmentConfig, record: &Value) -> String {
    let source_excerpt = enrichment_source_text(record, config).unwrap_or_else(|| {
        truncate_chars(
            &serde_json::to_string_pretty(record).unwrap_or_else(|_| canonical_json(record)),
            8_000,
        )
    });
    let task_lines = config
        .tasks
        .iter()
        .map(|task| {
            let hints = if task.field_hints.is_empty() {
                String::new()
            } else {
                format!(" fields={}", task.field_hints.join(","))
            };
            format!(
                "- kind={} path={} instruction={}{}",
                task.kind, task.output_field, task.instruction, hints
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "You are post-processing one CTOX scraped record.\n\
Return ONLY one JSON object with this shape:\n\
{{\"updates\":[{{\"path\":\"classification.category\",\"value\":\"job\"}}],\"notes\":\"optional\"}}\n\
\n\
Rules:\n\
- only return valid JSON, no markdown\n\
- only use paths declared in the tasks below\n\
- omit updates when the evidence is missing\n\
- use proper JSON scalars, arrays, and objects\n\
- for object-valued paths, return the whole object in `value`\n\
\n\
Tasks:\n\
{task_lines}\n\
\n\
Source excerpt:\n\
{source_excerpt}\n\
\n\
Full record JSON:\n\
{full_record}",
        full_record = truncate_chars(
            &serde_json::to_string_pretty(record).unwrap_or_else(|_| canonical_json(record)),
            12_000
        )
    )
}

fn enrichment_source_text(record: &Value, config: &EnrichmentConfig) -> Option<String> {
    let mut parts = Vec::new();
    for field in &config.source_fields {
        if let Some(value) = json_lookup_path(record, field).and_then(scalarish_string) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                parts.push(format!("{field}: {trimmed}"));
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(truncate_chars(&parts.join("\n"), 8_000))
    }
}

fn invoke_responses_text(
    root: &Path,
    model: &str,
    prompt: &str,
    timeout_seconds: u64,
) -> Result<String> {
    let resolved_runtime = runtime_kernel::InferenceRuntimeKernel::resolve(root)
        .context("failed to resolve runtime kernel for scrape enrichment")?;
    if resolved_runtime.state.source.is_local() {
        if let Some(binding) = resolved_runtime.primary_generation.as_ref() {
            if !binding.transport.is_private_ipc() {
                anyhow::bail!(
                    "ctox_core_local requires private IPC for local responses inference; loopback HTTP transport is not allowed"
                );
            }
            let label = binding.transport.display_label();
            return invoke_responses_text_via_local_socket(
                &binding.transport,
                model,
                prompt,
                timeout_seconds,
            )
            .with_context(|| format!("failed to reach responses transport {label}"));
        }
    }
    let base_url = resolved_runtime.internal_responses_base_url();
    let response = ureq::post(&format!("{}/v1/responses", base_url.trim_end_matches('/')))
        .set("content-type", "application/json")
        .timeout(Duration::from_secs(timeout_seconds.max(5)))
        .send_string(&serde_json::to_string(&json!({
            "model": model,
            "input": prompt,
        }))?)
        .with_context(|| format!("failed to reach CTOX responses service at {}", base_url))?;
    let body = response
        .into_string()
        .context("failed to read enrichment response")?;
    let payload: Value =
        serde_json::from_str(&body).context("failed to parse enrichment responses payload")?;
    extract_response_output_text(&payload).context("enrichment response missing output_text")
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LocalResponsesSocketRequest<'a> {
    ResponsesCreate {
        model: &'a str,
        input: &'a str,
        stream: bool,
    },
}

fn invoke_responses_text_via_local_socket(
    transport: &LocalTransport,
    model: &str,
    prompt: &str,
    timeout_seconds: u64,
) -> Result<String> {
    let timeout = Duration::from_secs(timeout_seconds.max(5));
    let label = transport.display_label();
    let mut stream = transport
        .connect_blocking(timeout)
        .with_context(|| format!("failed to connect via {label}"))?;

    let request = LocalResponsesSocketRequest::ResponsesCreate {
        model,
        input: prompt,
        stream: true,
    };
    let mut payload =
        serde_json::to_vec(&request).context("failed to encode local responses socket request")?;
    payload.push(b'\n');
    stream
        .write_all(&payload)
        .with_context(|| format!("failed to write request via {label}"))?;
    stream
        .flush()
        .with_context(|| format!("failed to flush request via {label}"))?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let mut output_text = String::new();
    let mut saw_completed = false;
    loop {
        line.clear();
        let bytes_read = reader
            .read_line(&mut line)
            .with_context(|| format!("failed to read response via {label}"))?;
        if bytes_read == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let event: Value =
            serde_json::from_str(trimmed).context("failed to parse responses socket event")?;
        match event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "response.output_text.delta" => {
                if let Some(delta) = event.get("delta").and_then(Value::as_str) {
                    output_text.push_str(delta);
                }
            }
            "response.completed" => {
                saw_completed = true;
                if output_text.trim().is_empty() {
                    if let Some(response) = event.get("response") {
                        if let Some(text) = extract_response_output_text(response) {
                            output_text = text;
                        }
                    }
                }
                break;
            }
            "response.failed" => {
                let message = event
                    .get("response")
                    .and_then(|response| response.get("error"))
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("local responses socket returned a failed response");
                anyhow::bail!("{message}");
            }
            _ => {}
        }
    }
    if !saw_completed {
        anyhow::bail!("responses socket closed before response.completed");
    }
    let trimmed = output_text.trim();
    if trimmed.is_empty() {
        anyhow::bail!("responses socket completed without output_text");
    }
    Ok(trimmed.to_string())
}

fn extract_response_output_text(payload: &Value) -> Option<String> {
    if let Some(text) = payload
        .get("output_text")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        return Some(text.to_string());
    }
    payload
        .get("output")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find_map(|item| {
                item.get("content")
                    .and_then(Value::as_array)
                    .and_then(|content| {
                        content.iter().find_map(|part| {
                            if part.get("type").and_then(Value::as_str) == Some("output_text") {
                                part.get("text")
                                    .and_then(Value::as_str)
                                    .map(ToOwned::to_owned)
                            } else {
                                None
                            }
                        })
                    })
            })
        })
}

fn parse_enrichment_response(raw_text: &str) -> Result<EnrichmentResponse> {
    let trimmed = raw_text.trim();
    let candidate = extract_first_json_object(trimmed).unwrap_or(trimmed);
    serde_json::from_str::<EnrichmentResponse>(candidate)
        .context("failed to parse enrichment json object")
}

fn extract_first_json_object(raw: &str) -> Option<&str> {
    let mut depth = 0usize;
    let mut start = None;
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in raw.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => {
                if depth == 0 {
                    start = Some(index);
                }
                depth += 1;
            }
            '}' => {
                if depth == 0 {
                    continue;
                }
                depth -= 1;
                if depth == 0 {
                    let start = start?;
                    return Some(&raw[start..=index]);
                }
            }
            _ => {}
        }
    }
    None
}

fn apply_enrichment_updates(
    record: &Value,
    config: &EnrichmentConfig,
    updates: &[EnrichmentUpdate],
) -> Result<Value> {
    let mut out = record.clone();
    for update in updates {
        let path = update.path.trim();
        if path.is_empty() || !is_allowed_enrichment_path(config, path) {
            continue;
        }
        set_json_path(&mut out, path, update.value.clone())?;
    }
    Ok(out)
}

fn is_allowed_enrichment_path(config: &EnrichmentConfig, path: &str) -> bool {
    config.tasks.iter().any(|task| {
        let root = task.output_field.trim();
        !root.is_empty() && (path == root || path.starts_with(&format!("{root}.")))
    })
}

fn set_json_path(root: &mut Value, path: &str, value: Value) -> Result<()> {
    let segments = path
        .split('.')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.is_empty() {
        anyhow::bail!("json path must not be empty");
    }
    let mut current = root;
    for segment in &segments[..segments.len() - 1] {
        if !current.is_object() {
            *current = json!({});
        }
        let object = current
            .as_object_mut()
            .context("enrichment path expected object container")?;
        current = object
            .entry((*segment).to_string())
            .or_insert_with(|| json!({}));
    }
    if !current.is_object() {
        *current = json!({});
    }
    let object = current
        .as_object_mut()
        .context("enrichment path expected writable object")?;
    object.insert(segments[segments.len() - 1].to_string(), value);
    Ok(())
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    input.chars().take(max_chars).collect::<String>()
}

fn artifact_record(
    artifact_kind: &str,
    path: &Path,
    schema_key: Option<&str>,
    record_count: Option<i64>,
) -> Result<ScrapeArtifactRecord> {
    let content_sha256 = if path.is_file() {
        Some(compute_sha256_bytes(&fs::read(path)?))
    } else {
        None
    };
    Ok(ScrapeArtifactRecord {
        artifact_id: format!(
            "scrape_artifact-{}",
            stable_digest(&format!("{artifact_kind}:{}", path.display()))
        ),
        artifact_kind: artifact_kind.to_string(),
        path: path.to_string_lossy().to_string(),
        schema_key: schema_key.map(ToOwned::to_owned),
        content_sha256,
        record_count,
    })
}

struct RecordRunRequest<'a> {
    run_id: String,
    target: &'a ScrapeTargetView,
    trigger_kind: String,
    scheduled_for: Option<String>,
    started_at: String,
    finished_at: String,
    status: String,
    script_revision_no: Option<i64>,
    script_sha256: Option<String>,
    run_context: Value,
    result: Value,
    output_dir: PathBuf,
    artifacts: Vec<ScrapeArtifactRecord>,
}

fn record_run(root: &Path, conn: &Connection, request: RecordRunRequest<'_>) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO scrape_run (
            run_id, target_id, trigger_kind, scheduled_for, started_at, finished_at, status,
            script_revision_no, script_sha256, run_context_json, result_json, output_dir, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        ON CONFLICT(run_id) DO UPDATE SET
            trigger_kind = excluded.trigger_kind,
            scheduled_for = excluded.scheduled_for,
            started_at = excluded.started_at,
            finished_at = excluded.finished_at,
            status = excluded.status,
            script_revision_no = excluded.script_revision_no,
            script_sha256 = excluded.script_sha256,
            run_context_json = excluded.run_context_json,
            result_json = excluded.result_json,
            output_dir = excluded.output_dir
        "#,
        params![
            request.run_id,
            request.target.target_id,
            request.trigger_kind,
            request.scheduled_for,
            request.started_at,
            request.finished_at,
            request.status,
            request.script_revision_no,
            request.script_sha256,
            serde_json::to_string(&request.run_context)?,
            serde_json::to_string(&request.result)?,
            request.output_dir.to_string_lossy(),
            now_iso_string(),
        ],
    )?;
    for artifact in &request.artifacts {
        conn.execute(
            r#"
            INSERT INTO scrape_artifact (
                artifact_id, run_id, artifact_kind, path, schema_key, content_sha256, record_count, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(run_id, artifact_kind, path) DO UPDATE SET
                schema_key = excluded.schema_key,
                content_sha256 = excluded.content_sha256,
                record_count = excluded.record_count
            "#,
            params![
                artifact.artifact_id,
                request.run_id,
                artifact.artifact_kind,
                artifact.path,
                artifact.schema_key,
                artifact.content_sha256,
                artifact.record_count,
                now_iso_string(),
            ],
        )?;
    }
    let manifest = json!({
        "run_id": request.run_id,
        "target_key": request.target.target_key,
        "sources": target_sources(request.target),
        "source_modules": request.run_context.get("source_modules").cloned(),
        "trigger_kind": request.trigger_kind,
        "scheduled_for": request.scheduled_for,
        "status": request.status,
        "script_revision_no": request.script_revision_no,
        "script_sha256": request.script_sha256,
        "run_context": request.run_context,
        "result": request.result,
        "output_dir": request.output_dir,
        "artifacts": request.artifacts,
    });
    fs::write(
        request.output_dir.join("run.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    let refreshed_target = load_target_view(conn, &request.target.target_key)?
        .context("failed to reload target while writing run manifest")?;
    write_target_manifest(root, &refreshed_target)?;
    Ok(())
}

fn write_repair_request(
    conn: &Connection,
    run_dir: &Path,
    target: &RegisteredTarget,
    status: FailureStatus,
    reason: &str,
    probe: &ProbeResult,
    execution: &CommandExecution,
    records_found: i64,
    last_successful_run: Option<&Value>,
    materialization: Option<&MaterializationOutcome>,
) -> Result<PathBuf> {
    let path = run_dir.join("repair_request.json");
    let latest_state_paths = latest_state_paths_for_target(&target.view);
    let latest_sample = load_latest_active_records_sample(conn, &target.view.target_id, 10)?;
    let source_modules = latest_source_revision_map(conn, &target.view.target_id)?
        .into_values()
        .collect::<Vec<_>>();
    fs::write(
        &path,
        serde_json::to_string_pretty(&json!({
            "target_key": target.view.target_key,
            "display_name": target.view.display_name,
            "start_url": target.view.start_url,
            "status": status.as_str(),
            "reason": reason,
            "probe": probe_to_json(probe),
            "records_found": records_found,
            "workspace_dir": target.view.workspace_dir,
            "manifest_path": resolve_workspace_dir(Path::new(""), &target.view.workspace_dir).join("manifest.json"),
            "current_script_path": target.script.script_path,
            "current_revision_no": target.script.revision_no,
            "current_script_sha256": target.script.script_sha256,
            "sources": target_sources(&target.view),
            "source_modules": source_modules,
            "last_successful_run": last_successful_run,
            "latest_state_paths": {
                "latest_records": latest_state_paths.0,
                "latest_summary": latest_state_paths.1,
            },
            "latest_materialized_sample": latest_sample,
            "current_run_materialization": materialization.as_ref().map(|item| item.summary.clone()),
            "stdout_excerpt": tail_excerpt(&execution.stdout_text, 4000),
            "stderr_excerpt": tail_excerpt(&execution.stderr_text, 4000),
        }))?,
    )?;
    Ok(path)
}

fn materialize_latest_records(
    conn: &Connection,
    target: &RegisteredTarget,
    run_id: &str,
    finished_at: &str,
    records: &[Value],
    output_dir: &Path,
    default_schema_key: Option<&str>,
) -> Result<MaterializationOutcome> {
    let identity_fields = record_identity_fields(target);
    let schema_key = default_schema_key.map(ToOwned::to_owned).or_else(|| {
        target
            .view
            .output_schema
            .get("schema_key")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
    });
    let existing = load_active_record_index(conn, &target.view.target_id)?;
    let mut next_records: BTreeMap<String, (String, Value)> = BTreeMap::new();
    let mut duplicate_keys = Vec::new();

    for record in records {
        let key = record_identity_key(record, &identity_fields)
            .unwrap_or_else(|| format!("hash:{}", stable_digest(&canonical_json(record))));
        let hash = compute_sha256(&canonical_json(record));
        if next_records
            .insert(key.clone(), (hash, record.clone()))
            .is_some()
        {
            duplicate_keys.push(key);
        }
    }

    let mut inserted_count = 0_i64;
    let mut updated_count = 0_i64;
    let mut unchanged_count = 0_i64;
    for (record_key, (record_hash, record)) in &next_records {
        match existing.get(record_key) {
            Some(existing_hash) if existing_hash == record_hash => unchanged_count += 1,
            Some(_) => updated_count += 1,
            None => inserted_count += 1,
        }
        conn.execute(
            r#"
            INSERT INTO scrape_record_latest (
                target_id, record_key, record_hash, record_json, schema_key,
                first_seen_at, last_seen_at, last_run_id, deleted_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, NULL)
            ON CONFLICT(target_id, record_key) DO UPDATE SET
                record_hash = excluded.record_hash,
                record_json = excluded.record_json,
                schema_key = excluded.schema_key,
                last_seen_at = excluded.last_seen_at,
                last_run_id = excluded.last_run_id,
                deleted_at = NULL
            "#,
            params![
                target.view.target_id,
                record_key,
                record_hash,
                canonical_json(record),
                schema_key,
                finished_at,
                run_id,
            ],
        )?;
    }

    let mut deleted_count = 0_i64;
    for record_key in existing.keys() {
        if !next_records.contains_key(record_key) {
            deleted_count += 1;
            conn.execute(
                r#"
                UPDATE scrape_record_latest
                SET deleted_at = ?3,
                    last_seen_at = ?3,
                    last_run_id = ?4
                WHERE target_id = ?1 AND record_key = ?2 AND deleted_at IS NULL
                "#,
                params![target.view.target_id, record_key, finished_at, run_id],
            )?;
        }
    }

    let active_record_count = conn.query_row(
        "SELECT COUNT(*) FROM scrape_record_latest WHERE target_id = ?1 AND deleted_at IS NULL",
        params![target.view.target_id],
        |row| row.get::<_, i64>(0),
    )?;
    let state_dir = resolve_workspace_dir(Path::new(""), &target.view.workspace_dir).join("state");
    fs::create_dir_all(&state_dir)?;
    let latest_records_path = state_dir.join("latest_records.json");
    let latest_summary_path = state_dir.join("latest_summary.json");
    let delta_path = output_dir.join("delta.json");
    let latest_records = next_records
        .values()
        .map(|(_, record)| record.clone())
        .collect::<Vec<_>>();
    let summary = json!({
        "run_id": run_id,
        "target_key": target.view.target_key,
        "schema_key": schema_key,
        "identity_fields": identity_fields,
        "inserted_count": inserted_count,
        "updated_count": updated_count,
        "unchanged_count": unchanged_count,
        "deleted_count": deleted_count,
        "active_record_count": active_record_count,
        "duplicate_key_count": duplicate_keys.len(),
        "duplicate_keys": duplicate_keys,
        "latest_records_path": latest_records_path,
        "latest_summary_path": latest_summary_path,
    });
    fs::write(
        &latest_records_path,
        serde_json::to_string_pretty(&latest_records)?,
    )?;
    fs::write(
        &latest_summary_path,
        serde_json::to_string_pretty(&summary)?,
    )?;
    fs::write(&delta_path, serde_json::to_string_pretty(&summary)?)?;
    Ok(MaterializationOutcome {
        summary,
        delta_artifact: artifact_record("delta_json", &delta_path, schema_key.as_deref(), None)?,
    })
}

fn load_active_record_index(conn: &Connection, target_id: &str) -> Result<HashMap<String, String>> {
    let mut statement = conn.prepare(
        r#"
        SELECT record_key, record_hash
        FROM scrape_record_latest
        WHERE target_id = ?1 AND deleted_at IS NULL
        "#,
    )?;
    let rows = statement.query_map(params![target_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    Ok(rows.collect::<rusqlite::Result<HashMap<_, _>>>()?)
}

fn load_latest_active_records_sample(
    conn: &Connection,
    target_id: &str,
    limit: usize,
) -> Result<Vec<LatestRecordView>> {
    let mut statement = conn.prepare(
        r#"
        SELECT record_key, last_seen_at, record_json
        FROM scrape_record_latest
        WHERE target_id = ?1 AND deleted_at IS NULL
        ORDER BY last_seen_at DESC, record_key ASC
        LIMIT ?2
        "#,
    )?;
    let rows = statement.query_map(params![target_id, limit as i64], |row| {
        let record_json: String = row.get(2)?;
        Ok(LatestRecordView {
            record_key: row.get(0)?,
            last_seen_at: row.get(1)?,
            record: serde_json::from_str(&record_json).unwrap_or_else(|_| json!({})),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn load_all_latest_active_records(
    conn: &Connection,
    target_id: &str,
) -> Result<Vec<LatestRecordView>> {
    let mut statement = conn.prepare(
        r#"
        SELECT record_key, last_seen_at, record_json
        FROM scrape_record_latest
        WHERE target_id = ?1 AND deleted_at IS NULL
        ORDER BY last_seen_at DESC, record_key ASC
        "#,
    )?;
    let rows = statement.query_map(params![target_id], |row| {
        let record_json: String = row.get(2)?;
        Ok(LatestRecordView {
            record_key: row.get(0)?,
            last_seen_at: row.get(1)?,
            record: serde_json::from_str(&record_json).unwrap_or_else(|_| json!({})),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn record_matches_filters(record: &Value, filters: &[(String, String)]) -> bool {
    filters.iter().all(|(path, expected)| {
        json_lookup_path(record, path)
            .and_then(scalarish_string)
            .map(|actual| actual == *expected)
            .unwrap_or(false)
    })
}

fn json_lookup_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.').filter(|item| !item.is_empty()) {
        let object = current.as_object()?;
        current = object.get(segment)?;
    }
    Some(current)
}

fn semantic_text_for_record(record: &Value, config: &SemanticConfig) -> Option<String> {
    let mut parts = Vec::new();
    for field in &config.source_fields {
        if let Some(value) = json_lookup_path(record, field).and_then(scalarish_string) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                parts.push(format!("{field}: {trimmed}"));
            }
        }
    }
    if parts.is_empty() {
        let object = record.as_object()?;
        for key in [
            "title",
            "name",
            "summary",
            "description",
            "content",
            "text",
            "semantic_summary",
        ] {
            if let Some(value) = object.get(key).and_then(scalarish_string) {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    parts.push(format!("{key}: {trimmed}"));
                }
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

fn ensure_semantic_records(
    root: &Path,
    conn: &Connection,
    target: &ScrapeTargetView,
    records: &[LatestRecordView],
    config: &SemanticConfig,
) -> Result<usize> {
    if !config.enabled {
        return Ok(0);
    }
    let cached = load_semantic_cache(conn, &target.target_id)?;
    let mut to_embed = Vec::new();
    let mut active_keys = Vec::new();
    for item in records {
        let Some(source_text) = semantic_text_for_record(&item.record, config) else {
            continue;
        };
        let content_hash = compute_sha256(&source_text);
        active_keys.push(item.record_key.clone());
        let needs_refresh = cached
            .get(&item.record_key)
            .map(|(existing_hash, _, _)| existing_hash != &content_hash)
            .unwrap_or(true);
        if needs_refresh {
            to_embed.push((item.record_key.clone(), source_text, content_hash));
        }
    }
    if !to_embed.is_empty() {
        let vectors = embed_texts(
            root,
            &to_embed
                .iter()
                .map(|(_, text, _)| text.clone())
                .collect::<Vec<_>>(),
            &config.embedding_model,
        )?;
        for ((record_key, source_text, content_hash), vector) in to_embed.into_iter().zip(vectors) {
            conn.execute(
                r#"
                INSERT INTO scrape_semantic_record (
                    target_id, record_key, content_hash, source_text, embedding_json, metadata_json, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ON CONFLICT(target_id, record_key) DO UPDATE SET
                    content_hash = excluded.content_hash,
                    source_text = excluded.source_text,
                    embedding_json = excluded.embedding_json,
                    metadata_json = excluded.metadata_json,
                    updated_at = excluded.updated_at
                "#,
                params![
                    target.target_id,
                    record_key,
                    content_hash,
                    source_text,
                    serde_json::to_string(&vector)?,
                    serde_json::to_string(&json!({
                        "embedding_model": config.embedding_model,
                        "source_fields": config.source_fields,
                    }))?,
                    now_iso_string(),
                ],
            )?;
        }
    }
    let active_set = active_keys.into_iter().collect::<Vec<_>>();
    if active_set.is_empty() {
        conn.execute(
            "DELETE FROM scrape_semantic_record WHERE target_id = ?1",
            params![target.target_id],
        )?;
    } else {
        let placeholders = std::iter::repeat("?")
            .take(active_set.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "DELETE FROM scrape_semantic_record WHERE target_id = ?1 AND record_key NOT IN ({placeholders})"
        );
        let values = rusqlite::params_from_iter(
            std::iter::once(target.target_id.as_str()).chain(active_set.iter().map(String::as_str)),
        );
        conn.execute(&sql, values)?;
    }
    Ok(records.len())
}

fn load_semantic_cache(
    conn: &Connection,
    target_id: &str,
) -> Result<HashMap<String, (String, String, Vec<f64>)>> {
    let mut statement = conn.prepare(
        r#"
        SELECT record_key, content_hash, source_text, embedding_json
        FROM scrape_semantic_record
        WHERE target_id = ?1
        "#,
    )?;
    let rows = statement.query_map(params![target_id], |row| {
        let embedding_json: String = row.get(3)?;
        Ok((
            row.get::<_, String>(0)?,
            (
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                serde_json::from_str::<Vec<f64>>(&embedding_json).unwrap_or_default(),
            ),
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<HashMap<_, _>>>()?)
}

fn load_semantic_matches(
    conn: &Connection,
    target_id: &str,
) -> Result<Vec<(String, String, Vec<f64>)>> {
    let mut statement = conn.prepare(
        r#"
        SELECT record_key, source_text, embedding_json
        FROM scrape_semantic_record
        WHERE target_id = ?1
        "#,
    )?;
    let rows = statement.query_map(params![target_id], |row| {
        let embedding_json: String = row.get(2)?;
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            serde_json::from_str::<Vec<f64>>(&embedding_json).unwrap_or_default(),
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn embed_texts(root: &Path, inputs: &[String], model: &str) -> Result<Vec<Vec<f64>>> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    supervisor::ensure_auxiliary_backend_launchable(
        root,
        crate::inference::engine::AuxiliaryRole::Embedding,
    )
    .context("embedding backend is not launchable for scrape embeddings")?;
    supervisor::ensure_auxiliary_backend_ready(
        root,
        crate::inference::engine::AuxiliaryRole::Embedding,
        false,
    )
    .context("failed to ensure managed embedding backend for scrape embeddings")?;
    let resolved_runtime = runtime_kernel::InferenceRuntimeKernel::resolve(root)
        .context("failed to resolve runtime kernel for scrape embeddings")?;
    if let Some(binding) = resolved_runtime
        .binding_for_auxiliary_role(crate::inference::engine::AuxiliaryRole::Embedding)
    {
        if !binding.transport.is_private_ipc() {
            anyhow::bail!(
                "ctox_core_local requires private IPC for local embedding inference; loopback HTTP transport is not allowed"
            );
        }
        let label = binding.transport.display_label();
        return embed_texts_via_local_socket(&binding.transport, inputs, model)
            .with_context(|| format!("failed to reach embedding transport {label}"));
    }
    let base_url = resolved_runtime
        .auxiliary_base_url(crate::inference::engine::AuxiliaryRole::Embedding)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("embedding runtime is not resolved"))?;
    let response = ureq::post(&format!("{}/v1/embeddings", base_url.trim_end_matches('/')))
        .set("content-type", "application/json")
        .timeout(Duration::from_secs(12))
        .send_string(&serde_json::to_string(&json!({
            "model": model,
            "input": inputs,
        }))?)
        .with_context(|| format!("failed to reach embedding service at {}", base_url))?;
    let body = response
        .into_string()
        .context("failed to read embedding response")?;
    let payload: Value =
        serde_json::from_str(&body).context("failed to parse embedding response")?;
    let mut indexed = payload
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    indexed.sort_by_key(|item| item.get("index").and_then(Value::as_u64).unwrap_or(0));
    let vectors = indexed
        .into_iter()
        .map(|item| {
            item.get("embedding")
                .and_then(Value::as_array)
                .map(|items| items.iter().filter_map(Value::as_f64).collect::<Vec<_>>())
                .filter(|items| !items.is_empty())
                .context("embedding response missing vectors")
        })
        .collect::<Result<Vec<_>>>()?;
    if vectors.len() != inputs.len() {
        anyhow::bail!(
            "embedding response count mismatch: expected {}, got {}",
            inputs.len(),
            vectors.len()
        );
    }
    Ok(vectors)
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LocalEmbeddingSocketRequest<'a> {
    EmbeddingsCreate {
        model: &'a str,
        inputs: &'a [String],
        truncate_sequence: bool,
    },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LocalEmbeddingSocketResponse {
    Embeddings {
        model: String,
        data: Vec<Vec<f32>>,
        #[serde(rename = "prompt_tokens")]
        _prompt_tokens: u32,
        #[serde(rename = "total_tokens")]
        _total_tokens: u32,
    },
    Error {
        code: String,
        message: String,
    },
}

fn embed_texts_via_local_socket(
    transport: &LocalTransport,
    inputs: &[String],
    model: &str,
) -> Result<Vec<Vec<f64>>> {
    let timeout = Duration::from_secs(12);
    let label = transport.display_label();
    let mut stream = transport
        .connect_blocking(timeout)
        .with_context(|| format!("failed to connect via {label}"))?;

    let request = LocalEmbeddingSocketRequest::EmbeddingsCreate {
        model,
        inputs,
        truncate_sequence: false,
    };
    let mut payload =
        serde_json::to_vec(&request).context("failed to encode local embedding socket request")?;
    payload.push(b'\n');
    stream
        .write_all(&payload)
        .with_context(|| format!("failed to write request via {label}"))?;
    stream
        .flush()
        .with_context(|| format!("failed to flush request via {label}"))?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .with_context(|| format!("failed to read response via {label}"))?;
    if line.trim().is_empty() {
        anyhow::bail!("embedding socket returned an empty response");
    }
    match serde_json::from_str::<LocalEmbeddingSocketResponse>(line.trim())
        .context("failed to parse embedding socket response")?
    {
        LocalEmbeddingSocketResponse::Embeddings {
            model: response_model,
            data,
            _prompt_tokens: _,
            _total_tokens: _,
        } => {
            let _ = response_model;
            Ok(data
                .into_iter()
                .map(|values| values.into_iter().map(|value| value as f64).collect())
                .collect())
        }
        LocalEmbeddingSocketResponse::Error { code, message } => {
            anyhow::bail!("{code}: {message}");
        }
    }
}

fn cosine_similarity(left: &[f64], right: &[f64]) -> f64 {
    if left.is_empty() || right.is_empty() || left.len() != right.len() {
        return 0.0;
    }
    let mut dot = 0.0;
    let mut left_norm = 0.0;
    let mut right_norm = 0.0;
    for (l, r) in left.iter().zip(right.iter()) {
        dot += l * r;
        left_norm += l * l;
        right_norm += r * r;
    }
    if left_norm <= f64::EPSILON || right_norm <= f64::EPSILON {
        0.0
    } else {
        dot / (left_norm.sqrt() * right_norm.sqrt())
    }
}

fn load_last_successful_run(conn: &Connection, target_id: &str) -> Result<Option<Value>> {
    conn.query_row(
        r#"
        SELECT run_id, finished_at, script_revision_no, script_sha256, result_json
        FROM scrape_run
        WHERE target_id = ?1 AND status = 'succeeded'
        ORDER BY finished_at DESC, created_at DESC
        LIMIT 1
        "#,
        params![target_id],
        |row| {
            let result_json: String = row.get(4)?;
            Ok(json!({
                "run_id": row.get::<_, String>(0)?,
                "finished_at": row.get::<_, Option<String>>(1)?,
                "script_revision_no": row.get::<_, Option<i64>>(2)?,
                "script_sha256": row.get::<_, Option<String>>(3)?,
                "result": serde_json::from_str::<Value>(&result_json).unwrap_or_else(|_| json!({})),
            }))
        },
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn latest_state_paths_for_target(target: &ScrapeTargetView) -> (PathBuf, PathBuf) {
    let state_dir = resolve_workspace_dir(Path::new(""), &target.workspace_dir).join("state");
    (
        state_dir.join("latest_records.json"),
        state_dir.join("latest_summary.json"),
    )
}

fn record_identity_fields(target: &RegisteredTarget) -> Vec<String> {
    if let Some(fields) = target
        .view
        .config
        .get("record_key_fields")
        .and_then(Value::as_array)
    {
        let keys = fields
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if !keys.is_empty() {
            return keys;
        }
    }
    if let Some(fields) = target
        .view
        .output_schema
        .get("record_key_fields")
        .and_then(Value::as_array)
    {
        let keys = fields
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if !keys.is_empty() {
            return keys;
        }
    }
    Vec::new()
}

fn record_identity_key(record: &Value, configured_fields: &[String]) -> Option<String> {
    let object = record.as_object()?;
    if !configured_fields.is_empty() {
        let mut parts = Vec::with_capacity(configured_fields.len());
        for field in configured_fields {
            let value = object.get(field)?;
            parts.push(format!("{field}={}", scalarish_string(value)?));
        }
        return Some(parts.join("|"));
    }
    for field in ["id", "external_id", "job_id", "slug", "url", "link", "uuid"] {
        if let Some(value) = object.get(field).and_then(scalarish_string) {
            return Some(format!("{field}={value}"));
        }
    }
    None
}

fn scalarish_string(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Number(number) => Some(number.to_string()),
        Value::String(text) => Some(text.trim().to_string()).filter(|value| !value.is_empty()),
        _ => Some(canonical_json(value)),
    }
}

fn canonical_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

fn acquire_target_run_lock(workspace_dir: &Path, target_key: &str) -> Result<TargetRunLock> {
    fs::create_dir_all(workspace_dir)?;
    let path = workspace_dir.join(".run.lock");
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .with_context(|| format!("scrape target `{target_key}` already has an active run"))?;
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
    Ok(TargetRunLock { path })
}

fn probe_portal_health(url: &str, skip_probe: bool) -> ProbeResult {
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

fn read_response_excerpt(response: ureq::Response) -> String {
    let mut reader = response.into_reader().take(4096);
    let mut body = String::new();
    let _ = reader.read_to_string(&mut body);
    body
}

fn contains_human_verification(text: &str) -> bool {
    let lowered = text.to_lowercase();
    [
        "human verification",
        "verify you are human",
        "captcha",
        "access denied",
        "security check",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

fn contains_transient_hint(text: &str) -> bool {
    [
        "timeout",
        "timed out",
        "temporary",
        "temporarily",
        "connection refused",
        "connection reset",
        "network is unreachable",
        "name or service not known",
        "429",
        "502",
        "503",
        "504",
        "ssl",
        "proxyerror",
        "net::err_",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

fn probe_to_json(probe: &ProbeResult) -> Value {
    json!({
    "reachable": probe.reachable,
    "status_code": probe.status_code,
    "final_url": probe.final_url,
    "human_verification": probe.human_verification,
    "error": probe.error,
    })
}

fn default_entry_command(language: &str) -> Vec<String> {
    match language.trim().to_lowercase().as_str() {
        "javascript" | "js" => vec!["node".to_string(), "{script_path}".to_string()],
        "typescript" | "ts" => vec!["tsx".to_string(), "{script_path}".to_string()],
        "bash" | "shell" | "sh" => vec!["bash".to_string(), "{script_path}".to_string()],
        _ => vec!["sh".to_string(), "{script_path}".to_string()],
    }
}

fn script_extension(language: &str, source_path: &Path) -> String {
    if let Some(extension) = source_path.extension().and_then(|part| part.to_str()) {
        return format!(".{extension}");
    }
    match language.trim().to_lowercase().as_str() {
        "javascript" | "js" => ".js".to_string(),
        "typescript" | "ts" => ".ts".to_string(),
        "bash" | "shell" | "sh" => ".sh".to_string(),
        _ => ".txt".to_string(),
    }
}

fn should_auto_promote_template(
    script_body: &str,
    best_result_count: i64,
    target_count: i64,
    challenge_score: i64,
) -> (bool, String) {
    if script_body.trim().len() < MIN_TEMPLATE_CODE_LEN {
        return (false, "script_too_short".to_string());
    }
    if best_result_count < MIN_TEMPLATE_RESULTS {
        return (false, "result_count_below_threshold".to_string());
    }
    if target_count >= MIN_TEMPLATE_TARGETS {
        return (true, "multi_target_template".to_string());
    }
    if target_count >= 1 && challenge_score >= 3 {
        return (true, "manual_or_high_challenge_override".to_string());
    }
    (false, "insufficient_cross_target_evidence".to_string())
}

fn upsert_promoted_template(
    conn: &Connection,
    template_key: &str,
    script_sha256: &str,
    script_body: &str,
    language: &str,
    source_example_count: i64,
    source_target_count: i64,
    best_result_count: i64,
    promotion_reason: &str,
) -> Result<()> {
    let now = now_iso_string();
    conn.execute(
        r#"
        INSERT INTO scrape_template_promoted (
            template_key, script_sha256, script_body, language, source_example_count,
            source_target_count, best_result_count, promotion_reason, is_active, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9, ?9)
        ON CONFLICT(template_key) DO UPDATE SET
            script_sha256 = excluded.script_sha256,
            script_body = excluded.script_body,
            language = excluded.language,
            source_example_count = excluded.source_example_count,
            source_target_count = excluded.source_target_count,
            best_result_count = excluded.best_result_count,
            promotion_reason = excluded.promotion_reason,
            is_active = 1,
            updated_at = excluded.updated_at
        "#,
        params![
            template_key,
            script_sha256,
            script_body,
            language,
            source_example_count,
            source_target_count,
            best_result_count,
            promotion_reason,
            now,
        ],
    )?;
    Ok(())
}

fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in input.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "scrape-target".to_string()
    } else {
        trimmed
    }
}

fn compute_sha256(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    format!("{digest:x}")
}

fn compute_sha256_bytes(input: &[u8]) -> String {
    let digest = Sha256::digest(input);
    format!("{digest:x}")
}

fn stable_digest(input: &str) -> String {
    compute_sha256(input)[..16].to_string()
}

fn tail_excerpt(input: &str, max_chars: usize) -> String {
    if input.len() <= max_chars {
        input.to_string()
    } else {
        input[input.len() - max_chars..].to_string()
    }
}

fn now_iso_string() -> String {
    chrono::DateTime::<chrono::Utc>::from(SystemTime::now()).to_rfc3339()
}

fn required_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    find_flag_value(args, flag)
}

fn find_flag_values<'a>(args: &'a [String], flag: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if args[index] == flag {
            if let Some(value) = args.get(index + 1) {
                out.push(value.as_str());
            }
            index += 2;
        } else {
            index += 1;
        }
    }
    out
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

fn parse_where_filters(args: &[String]) -> Result<Vec<(String, String)>> {
    let mut filters = Vec::new();
    for raw in find_flag_values(args, "--where") {
        let Some((field, value)) = raw.split_once('=') else {
            anyhow::bail!("--where expects field=value");
        };
        let field = field.trim();
        let value = value.trim();
        if field.is_empty() || value.is_empty() {
            anyhow::bail!("--where expects non-empty field=value");
        }
        filters.push((field.to_string(), value.to_string()));
    }
    Ok(filters)
}

fn print_json(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;
    use std::net::TcpListener;
    use std::net::TcpStream;
    #[cfg(unix)]
    use std::os::unix::net::UnixListener;
    use std::path::Path;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::sync::Mutex;

    static SCRAPE_EXEC_TEST_LOCK: Mutex<()> = Mutex::new(());

    struct TestFeedServer {
        addr: String,
        stop: Arc<AtomicBool>,
        handle: Option<std::thread::JoinHandle<()>>,
    }

    impl Drop for TestFeedServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            let _ = TcpStream::connect(&self.addr);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    fn start_test_feed_server() -> TestFeedServer {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let addr_text = format!("127.0.0.1:{}", addr.port());
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = stop.clone();
        let handle = std::thread::spawn(move || {
            while !stop_flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut request_line = String::new();
                        let _ = std::io::BufRead::read_line(
                            &mut BufReader::new(stream.try_clone().unwrap()),
                            &mut request_line,
                        );
                        let path = request_line
                            .split_terminator(['\r', '\n'])
                            .next()
                            .and_then(|line| line.split_whitespace().nth(1))
                            .unwrap_or("/");
                        let (status, body) = match path {
                            "/rss.xml" => (
                                "200 OK",
                                r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Fixture RSS</title>
    <item>
      <title>RSS Alpha</title>
      <link>https://example.test/rss-alpha</link>
      <description>Alpha summary</description>
      <pubDate>Mon, 01 Jan 2026 10:00:00 +0000</pubDate>
    </item>
    <item>
      <title>RSS Beta</title>
      <link>https://example.test/rss-beta</link>
      <description>Beta summary</description>
      <pubDate>Tue, 02 Jan 2026 10:00:00 +0000</pubDate>
    </item>
  </channel>
</rss>"#,
                            ),
                            "/atom.xml" => (
                                "200 OK",
                                r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Fixture Atom</title>
  <entry>
    <title>Atom Alpha</title>
    <link href="https://example.test/atom-alpha" />
    <summary>Atom alpha summary</summary>
    <updated>2026-01-03T10:00:00+00:00</updated>
  </entry>
  <entry>
    <title>Atom Beta</title>
    <link href="https://example.test/atom-beta" />
    <summary>Atom beta summary</summary>
    <updated>2026-01-04T10:00:00+00:00</updated>
  </entry>
</feed>"#,
                            ),
                            _ => ("404 Not Found", "not found"),
                        };
                        let response = format!(
                            "HTTP/1.1 {status}\r\nContent-Type: application/xml; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(response.as_bytes());
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    Err(_) => break,
                }
            }
        });
        TestFeedServer {
            addr: addr_text,
            stop,
            handle: Some(handle),
        }
    }

    fn temp_root(prefix: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "ctox-scrape-{prefix}-{}",
            stable_digest(&now_iso_string())
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[cfg(unix)]
    #[test]
    fn embed_texts_via_local_socket_uses_internal_embedding_contract() {
        let root =
            std::env::temp_dir().join(format!("ce-{}", &stable_digest(&now_iso_string())[..8]));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let socket_path = root.join("e.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();
        let server = std::thread::spawn(move || -> Result<()> {
            let (stream, _) = listener.accept()?;
            let mut reader = BufReader::new(stream);
            let mut request_line = String::new();
            std::io::BufRead::read_line(&mut reader, &mut request_line)?;
            assert!(request_line.contains("\"kind\":\"embeddings_create\""));
            assert!(request_line.contains("\"model\":\"Qwen/Qwen3-Embedding-0.6B\""));
            assert!(request_line.contains("\"truncate_sequence\":false"));
            let response = concat!(
                "{\"kind\":\"embeddings\",\"model\":\"Qwen/Qwen3-Embedding-0.6B\",",
                "\"data\":[[1.0,2.5],[3.0]],\"prompt_tokens\":4,\"total_tokens\":4}\n"
            );
            std::io::Write::write_all(reader.get_mut(), response.as_bytes())?;
            std::io::Write::flush(reader.get_mut())?;
            Ok(())
        });
        let inputs = vec!["alpha".to_string(), "beta".to_string()];
        let transport = LocalTransport::UnixSocket {
            path: socket_path.clone(),
        };
        let vectors =
            embed_texts_via_local_socket(&transport, &inputs, "Qwen/Qwen3-Embedding-0.6B").unwrap();
        assert_eq!(vectors, vec![vec![1.0_f64, 2.5_f64], vec![3.0_f64]]);
        server.join().unwrap().unwrap();
        cleanup_test_root(&root);
    }

    #[cfg(unix)]
    #[test]
    fn invoke_responses_text_via_local_socket_streams_internal_response_contract() {
        let root =
            std::env::temp_dir().join(format!("cr-{}", &stable_digest(&now_iso_string())[..8]));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let socket_path = root.join("r.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();
        let server = std::thread::spawn(move || -> Result<()> {
            let (stream, _) = listener.accept()?;
            let mut reader = BufReader::new(stream);
            let mut request_line = String::new();
            std::io::BufRead::read_line(&mut reader, &mut request_line)?;
            assert!(request_line.contains("\"kind\":\"responses_create\""));
            assert!(request_line.contains("\"stream\":true"));
            assert!(request_line.contains("\"model\":\"gpt-oss-120b\""));
            assert!(request_line.contains("\"input\":\"Say hello\""));
            for line in [
                "{\"type\":\"response.output_text.delta\",\"delta\":\"Hello\"}\n",
                "{\"type\":\"response.output_text.delta\",\"delta\":\" world\"}\n",
                "{\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"output_text\":\"Hello world\"}}\n",
            ] {
                std::io::Write::write_all(reader.get_mut(), line.as_bytes())?;
            }
            std::io::Write::flush(reader.get_mut())?;
            Ok(())
        });
        let transport = LocalTransport::UnixSocket {
            path: socket_path.clone(),
        };
        let text =
            invoke_responses_text_via_local_socket(&transport, "gpt-oss-120b", "Say hello", 5)
                .unwrap();
        assert_eq!(text, "Hello world");
        server.join().unwrap().unwrap();
        cleanup_test_root(&root);
    }

    fn cleanup_test_root(root: &Path) {
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn upsert_target_creates_workspace_and_manifest() {
        let root = temp_root("upsert");
        let payload = json!({
            "target_key": "Acme Jobs",
            "display_name": "Acme Jobs",
            "start_url": "https://example.com/jobs",
            "target_kind": "jobs",
            "output_schema": {"schema_key": "jobs.v1"},
            "config": {"skip_probe": true}
        });
        let target = upsert_target(&root, DEFAULT_RUNTIME_ROOT, payload).unwrap();
        assert_eq!(target.target_key, "acme-jobs");
        assert!(resolve_workspace_dir(&root, &target.workspace_dir)
            .join("manifest.json")
            .is_file());
        assert!(resolve_workspace_dir(&root, &target.workspace_dir)
            .join("api/api_contract.json")
            .is_file());
        assert!(resolve_workspace_dir(&root, &target.workspace_dir)
            .join("api/semantic_template.json")
            .is_file());
        assert!(resolve_workspace_dir(&root, &target.workspace_dir)
            .join("api/llm_enrichment_template.json")
            .is_file());
        assert!(resolve_workspace_dir(&root, &target.workspace_dir)
            .join("sources/sources_manifest.json")
            .is_file());
        assert!(resolve_workspace_dir(&root, &target.workspace_dir)
            .join("sources/primary/source.json")
            .is_file());
        assert!(resolve_workspace_dir(&root, &target.workspace_dir)
            .join("sources/primary/extractor.js")
            .is_file());
        cleanup_test_root(&root);
    }

    #[test]
    fn upsert_target_normalizes_multi_source_config() {
        let root = temp_root("multi-source");
        let payload = json!({
            "target_key": "aggregated-jobs",
            "display_name": "Aggregated Jobs",
            "start_url": "https://example.com/jobs",
            "target_kind": "jobs",
            "config": {
                "sources": [
                    {
                        "source_key": "board-a",
                        "display_name": "Board A",
                        "start_url": "https://a.example/jobs",
                        "source_kind": "rss",
                        "extraction_module": "sources/board-a/extractor.js",
                        "tags": ["jobs", "rss"]
                    },
                    {
                        "display_name": "Board B",
                        "url": "https://b.example/jobs",
                        "kind": "html"
                    }
                ]
            }
        });
        let target = upsert_target(&root, DEFAULT_RUNTIME_ROOT, payload).unwrap();
        let sources = target_sources(&target);
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].source_key, "board-a");
        assert_eq!(sources[0].source_kind, "rss");
        assert_eq!(sources[1].source_key, "board-b");
        assert_eq!(sources[1].source_kind, "html");
        assert!(resolve_workspace_dir(&root, &target.workspace_dir)
            .join("sources/board-a/source.json")
            .is_file());
        assert!(resolve_workspace_dir(&root, &target.workspace_dir)
            .join("sources/board-b/source.json")
            .is_file());
        assert!(resolve_workspace_dir(&root, &target.workspace_dir)
            .join("sources/board-b/extractor.js")
            .is_file());
        cleanup_test_root(&root);
    }

    #[test]
    fn register_source_module_creates_revision_and_surfaces_in_target_view() {
        let root = temp_root("source-module");
        let payload = json!({
            "target_key": "aggregated-jobs",
            "display_name": "Aggregated Jobs",
            "start_url": "https://example.com/jobs",
            "target_kind": "jobs",
            "config": {
                "sources": [
                    {
                        "source_key": "board-a",
                        "display_name": "Board A",
                        "start_url": "https://a.example/jobs",
                        "source_kind": "rss"
                    }
                ]
            }
        });
        let target = upsert_target(&root, DEFAULT_RUNTIME_ROOT, payload).unwrap();
        let module_path = root.join("board-a-source.js");
        fs::write(
            &module_path,
            "module.exports = async function extractSource() { return { records: [{ id: 'a-1' }] }; };\n",
        )
        .unwrap();
        let registered = register_source_module(
            &root,
            DEFAULT_RUNTIME_ROOT,
            &target.target_key,
            "board-a",
            module_path.to_str().unwrap(),
            "javascript",
            Some("initial_source_import"),
            Some("test module"),
        )
        .unwrap();
        assert_eq!(
            registered.get("revision_no").and_then(Value::as_i64),
            Some(1)
        );
        let show = show_target(&root, &target.target_key).unwrap().unwrap();
        let source_revisions = show
            .get("source_revisions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert_eq!(source_revisions.len(), 1);
        assert_eq!(
            source_revisions[0]
                .get("source_key")
                .and_then(Value::as_str),
            Some("board-a")
        );
        assert!(resolve_workspace_dir(&root, &target.workspace_dir)
            .join("sources/board-a/current.js")
            .is_file());
        assert!(resolve_workspace_dir(&root, &target.workspace_dir)
            .join("sources/board-a/revisions")
            .read_dir()
            .unwrap()
            .next()
            .is_some());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn end_to_end_multi_source_execute_materializes_latest_and_filters_by_source() {
        let _guard = SCRAPE_EXEC_TEST_LOCK.lock().unwrap();
        let root = temp_root("e2e-multi-source");
        let server = start_test_feed_server();
        let payload = json!({
            "target_key": "fixture-multi-feed",
            "display_name": "Fixture Multi Feed",
            "start_url": format!("http://{}/rss.xml", server.addr),
            "target_kind": "articles",
            "config": {
                "record_key_fields": ["source_key", "url"],
                "sources": [
                    {
                        "source_key": "rss-source",
                        "display_name": "RSS Source",
                        "start_url": format!("http://{}/rss.xml", server.addr),
                        "source_kind": "rss",
                        "extraction_module": "sources/rss-source/extractor.js"
                    },
                    {
                        "source_key": "atom-source",
                        "display_name": "Atom Source",
                        "start_url": format!("http://{}/atom.xml", server.addr),
                        "source_kind": "atom",
                        "extraction_module": "sources/atom-source/extractor.js"
                    }
                ]
            },
            "output_schema": {
                "schema_key": "articles.v1",
                "record_key_fields": ["source_key", "url"]
            }
        });
        let target = upsert_target(&root, DEFAULT_RUNTIME_ROOT, payload).unwrap();

        let root_script = root.join("fixture-root.js");
        fs::write(
            &root_script,
            r#"const path = require("path");

function sources() {
  return JSON.parse(process.env.CTOX_SCRAPE_SOURCES_JSON || "[]");
}

async function main() {
  const targetDir = process.env.CTOX_SCRAPE_TARGET_DIR;
  const records = [];
  for (const source of sources()) {
    if (source.enabled === false) continue;
    const modulePath = path.join(targetDir, source.extraction_module);
    const extractSource = require(modulePath);
    const result = await extractSource({ source });
    for (const record of result.records || []) {
      records.push({
        source_key: source.source_key,
        source: {
          source_key: source.source_key,
          display_name: source.display_name
        },
        ...record
      });
    }
  }
  process.stdout.write(JSON.stringify({ records }, null, 2));
}

main().catch((error) => {
  process.stderr.write(String(error.stack || error.message || error));
  process.exit(1);
});
"#,
        )
        .unwrap();
        register_script(
            &root,
            DEFAULT_RUNTIME_ROOT,
            &target.target_key,
            root_script.to_str().unwrap(),
            "javascript",
            Some("fixture_root"),
            None,
        )
        .unwrap();

        let rss_module = root.join("fixture-rss.js");
        fs::write(
            &rss_module,
            r#"const http = require("http");

function fetchText(url) {
  return new Promise((resolve, reject) => {
    http.get(url, (response) => {
      let body = "";
      response.setEncoding("utf8");
      response.on("data", (chunk) => body += chunk);
      response.on("end", () => resolve(body));
    }).on("error", reject);
  });
}

module.exports = async function extractSource(context) {
  const xml = await fetchText(context.source.start_url);
  const blocks = [...xml.matchAll(/<item\b[\s\S]*?<\/item>/gi)].map((match) => match[0]);
  return {
    records: blocks.map((block, index) => ({
      id: `${context.source.source_key}-${index + 1}`,
      title: (block.match(/<title>([\s\S]*?)<\/title>/i) || [])[1] || "",
      url: (block.match(/<link>([\s\S]*?)<\/link>/i) || [])[1] || "",
      summary: (block.match(/<description>([\s\S]*?)<\/description>/i) || [])[1] || ""
    }))
  };
};
"#,
        )
        .unwrap();
        register_source_module(
            &root,
            DEFAULT_RUNTIME_ROOT,
            &target.target_key,
            "rss-source",
            rss_module.to_str().unwrap(),
            "javascript",
            Some("fixture_rss"),
            None,
        )
        .unwrap();

        let atom_module = root.join("fixture-atom.js");
        fs::write(
            &atom_module,
            r#"const http = require("http");

function fetchText(url) {
  return new Promise((resolve, reject) => {
    http.get(url, (response) => {
      let body = "";
      response.setEncoding("utf8");
      response.on("data", (chunk) => body += chunk);
      response.on("end", () => resolve(body));
    }).on("error", reject);
  });
}

module.exports = async function extractSource(context) {
  const xml = await fetchText(context.source.start_url);
  const blocks = [...xml.matchAll(/<entry\b[\s\S]*?<\/entry>/gi)].map((match) => match[0]);
  return {
    records: blocks.map((block, index) => ({
      id: `${context.source.source_key}-${index + 1}`,
      title: (block.match(/<title>([\s\S]*?)<\/title>/i) || [])[1] || "",
      url: (block.match(/<link\b[^>]*href="([^"]+)"/i) || [])[1] || "",
      summary: (block.match(/<summary>([\s\S]*?)<\/summary>/i) || [])[1] || ""
    }))
  };
};
"#,
        )
        .unwrap();
        register_source_module(
            &root,
            DEFAULT_RUNTIME_ROOT,
            &target.target_key,
            "atom-source",
            atom_module.to_str().unwrap(),
            "javascript",
            Some("fixture_atom"),
            None,
        )
        .unwrap();

        let args = vec![
            "execute".to_string(),
            "--target-key".to_string(),
            target.target_key.clone(),
            "--allow-heal".to_string(),
            "--timeout-seconds".to_string(),
            "30".to_string(),
        ];
        execute_scrape(&root, &args).unwrap();

        let latest = show_latest(&root, &target.target_key, 10).unwrap().unwrap();
        assert_eq!(
            latest.get("active_record_count").and_then(Value::as_i64),
            Some(4)
        );

        let filtered = query_records(
            &root,
            &target.target_key,
            &[("source_key".to_string(), "rss-source".to_string())],
            10,
        )
        .unwrap()
        .unwrap();
        assert_eq!(filtered.get("count").and_then(Value::as_u64), Some(2));

        let api = show_api(&root, &target.target_key).unwrap().unwrap();
        assert_eq!(api.get("source_count").and_then(Value::as_u64), Some(2));
        assert_eq!(
            api.get("source_modules")
                .and_then(Value::as_array)
                .map(|items| items.len()),
            Some(2)
        );

        let latest_records = fs::read_to_string(
            resolve_workspace_dir(&root, &target.workspace_dir).join("state/latest_records.json"),
        )
        .unwrap();
        assert!(latest_records.contains("rss-source"));
        assert!(latest_records.contains("atom-source"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn execute_with_reachable_failure_creates_repair_bundle_and_queue_task() {
        let _guard = SCRAPE_EXEC_TEST_LOCK.lock().unwrap();
        let root = temp_root("repair-flow");
        let server = start_test_feed_server();
        let payload = json!({
            "target_key": "repair-fixture",
            "display_name": "Repair Fixture",
            "start_url": format!("http://{}/rss.xml", server.addr),
            "target_kind": "articles",
            "config": {
                "record_key_fields": ["source_key", "url"],
                "sources": [
                    {
                        "source_key": "rss-source",
                        "display_name": "RSS Source",
                        "start_url": format!("http://{}/rss.xml", server.addr),
                        "source_kind": "rss",
                        "extraction_module": "sources/rss-source/extractor.js"
                    }
                ]
            },
            "output_schema": {
                "schema_key": "articles.v1",
                "record_key_fields": ["source_key", "url"]
            }
        });
        let target = upsert_target(&root, DEFAULT_RUNTIME_ROOT, payload).unwrap();

        let root_script = root.join("broken-root.js");
        fs::write(
            &root_script,
            "process.stderr.write('selector drift detected'); process.exit(1);\n",
        )
        .unwrap();
        register_script(
            &root,
            DEFAULT_RUNTIME_ROOT,
            &target.target_key,
            root_script.to_str().unwrap(),
            "javascript",
            Some("broken_fixture"),
            None,
        )
        .unwrap();

        let source_module = root.join("repair-source.js");
        fs::write(
            &source_module,
            "module.exports = async function extractSource() { return { records: [{ id: 'x' }] }; };\n",
        )
        .unwrap();
        register_source_module(
            &root,
            DEFAULT_RUNTIME_ROOT,
            &target.target_key,
            "rss-source",
            source_module.to_str().unwrap(),
            "javascript",
            Some("fixture_source"),
            None,
        )
        .unwrap();

        let args = vec![
            "execute".to_string(),
            "--target-key".to_string(),
            target.target_key.clone(),
            "--allow-heal".to_string(),
            "--timeout-seconds".to_string(),
            "30".to_string(),
        ];
        execute_scrape(&root, &args).unwrap();

        let tasks = crate::channels::list_queue_tasks(&root, &["pending".to_string()], 10).unwrap();
        assert_eq!(tasks.len(), 1);
        assert!(tasks[0]
            .prompt
            .contains("ctox scrape register-source-module"));
        assert!(tasks[0].thread_key.contains("repair-fixture"));

        let workspace = resolve_workspace_dir(&root, &target.workspace_dir).join("runs");
        let mut repair_request_found = false;
        for entry in fs::read_dir(&workspace).unwrap() {
            let entry = entry.unwrap();
            let repair_path = entry.path().join("repair_request.json");
            if repair_path.is_file() {
                let text = fs::read_to_string(&repair_path).unwrap();
                assert!(text.contains("\"source_modules\""));
                assert!(text.contains("\"source_key\": \"rss-source\""));
                repair_request_found = true;
            }
        }
        assert!(repair_request_found);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn classify_reachable_empty_output_as_portal_drift() {
        let payload = json!({});
        let probe = ProbeResult {
            reachable: true,
            status_code: Some(200),
            final_url: "https://example.com/jobs".to_string(),
            human_verification: false,
            error: None,
        };
        let execution = CommandExecution {
            exit_code: Some(0),
            timed_out: false,
            stdout_text: String::new(),
            stderr_text: String::new(),
        };
        let classification = classify_outcome(&payload, &probe, &execution, 0, 0);
        assert_eq!(classification.status, FailureStatus::PortalDrift);
        assert!(classification.should_queue_repair);
    }

    #[test]
    fn classify_unreachable_probe_as_temporary_unreachable() {
        let payload = json!({});
        let probe = ProbeResult {
            reachable: false,
            status_code: None,
            final_url: "https://example.com/jobs".to_string(),
            human_verification: false,
            error: Some("ConnectionRefusedError".to_string()),
        };
        let execution = CommandExecution {
            exit_code: Some(1),
            timed_out: false,
            stdout_text: String::new(),
            stderr_text: "Connection refused".to_string(),
        };
        let classification = classify_outcome(&payload, &probe, &execution, 0, 0);
        assert_eq!(classification.status, FailureStatus::TemporaryUnreachable);
        assert!(!classification.should_queue_repair);
    }

    #[test]
    fn classify_reachable_content_with_transient_words_as_succeeded() {
        let payload = json!({
            "records": [
                {
                    "id": "entry-1",
                    "title": "Temporary network guidance",
                    "summary": "This article mentions a timeout, but the scrape itself succeeded."
                }
            ]
        });
        let probe = ProbeResult {
            reachable: true,
            status_code: Some(200),
            final_url: "https://example.com/feed.xml".to_string(),
            human_verification: false,
            error: None,
        };
        let execution = CommandExecution {
            exit_code: Some(0),
            timed_out: false,
            stdout_text: serde_json::to_string(&payload).unwrap(),
            stderr_text: String::new(),
        };
        let classification = classify_outcome(&payload, &probe, &execution, 1, 0);
        assert_eq!(classification.status, FailureStatus::Succeeded);
        assert!(!classification.should_queue_repair);
    }

    #[test]
    fn materialize_latest_records_tracks_insert_update_and_delete() {
        let root = temp_root("materialize");
        let target = upsert_target(
            &root,
            DEFAULT_RUNTIME_ROOT,
            json!({
                "target_key": "delta-target",
                "display_name": "Delta Target",
                "start_url": "https://example.com/jobs",
                "target_kind": "jobs",
                "output_schema": {"schema_key": "jobs.v1", "record_key_fields": ["id"]},
                "config": {"skip_probe": true, "record_key_fields": ["id"]}
            }),
        )
        .unwrap();
        let registered = RegisteredTarget {
            view: target,
            script: ScrapeScriptRevisionRecord {
                revision_no: 1,
                script_path: root
                    .join("runtime/scraping/targets/delta-target/scripts/current.js")
                    .to_string_lossy()
                    .to_string(),
                language: "javascript".to_string(),
                entry_command: vec!["node".to_string(), "{script_path}".to_string()],
                script_sha256: "sha".to_string(),
            },
            workspace_root: resolve_workspace_dir(&root, "runtime/scraping/targets/delta-target"),
        };
        let conn = open_db(&root).unwrap();
        let first_output_dir =
            resolve_workspace_dir(&root, &registered.view.workspace_dir).join("runs/run-1/outputs");
        fs::create_dir_all(&first_output_dir).unwrap();
        let first = materialize_latest_records(
            &conn,
            &registered,
            "run-1",
            "2026-03-27T10:00:00Z",
            &[
                json!({"id": "1", "title": "A"}),
                json!({"id": "2", "title": "B"}),
            ],
            &first_output_dir,
            Some("jobs.v1"),
        )
        .unwrap();
        assert_eq!(
            first.summary.get("inserted_count").and_then(Value::as_i64),
            Some(2)
        );
        assert_eq!(
            first.summary.get("deleted_count").and_then(Value::as_i64),
            Some(0)
        );

        let second_output_dir =
            resolve_workspace_dir(&root, &registered.view.workspace_dir).join("runs/run-2/outputs");
        fs::create_dir_all(&second_output_dir).unwrap();
        let second = materialize_latest_records(
            &conn,
            &registered,
            "run-2",
            "2026-03-27T11:00:00Z",
            &[
                json!({"id": "1", "title": "A updated"}),
                json!({"id": "3", "title": "C"}),
            ],
            &second_output_dir,
            Some("jobs.v1"),
        )
        .unwrap();
        assert_eq!(
            second.summary.get("inserted_count").and_then(Value::as_i64),
            Some(1)
        );
        assert_eq!(
            second.summary.get("updated_count").and_then(Value::as_i64),
            Some(1)
        );
        assert_eq!(
            second.summary.get("deleted_count").and_then(Value::as_i64),
            Some(1)
        );

        let latest = show_latest(&root, "delta-target", 10).unwrap().unwrap();
        assert_eq!(
            latest.get("active_record_count").and_then(Value::as_i64),
            Some(2)
        );
        let records = latest.get("records").and_then(Value::as_array).unwrap();
        assert_eq!(records.len(), 2);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ensure_semantic_records_clears_cache_when_latest_records_are_empty() {
        let root = temp_root("semantic-prune");
        let target = upsert_target(
            &root,
            DEFAULT_RUNTIME_ROOT,
            json!({
                "target_key": "semantic-target",
                "display_name": "Semantic Target",
                "start_url": "https://example.com/jobs",
                "target_kind": "jobs",
                "output_schema": {"schema_key": "jobs.v1", "record_key_fields": ["id"]},
                "config": {
                    "skip_probe": true,
                    "api": {
                        "semantic": {
                            "enabled": true,
                            "source_fields": ["title", "description"]
                        }
                    }
                }
            }),
        )
        .unwrap();
        let conn = open_db(&root).unwrap();
        let config = load_semantic_config(&root, &target);
        let record = LatestRecordView {
            record_key: "job-1".to_string(),
            last_seen_at: "2026-03-28T10:00:00Z".to_string(),
            record: json!({
                "id": "job-1",
                "title": "Rust Engineer",
                "description": "Build scraping APIs"
            }),
        };
        conn.execute(
            r#"
            INSERT INTO scrape_semantic_record (
                target_id, record_key, content_hash, source_text, embedding_json, metadata_json, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                target.target_id,
                record.record_key,
                compute_sha256("title: Rust Engineer\ndescription: Build scraping APIs"),
                "title: Rust Engineer\ndescription: Build scraping APIs",
                serde_json::to_string(&vec![0.1, 0.2, 0.3]).unwrap(),
                serde_json::to_string(&json!({
                    "embedding_model": config.embedding_model,
                    "source_fields": config.source_fields,
                }))
                .unwrap(),
                now_iso_string(),
            ],
        )
        .unwrap();
        ensure_semantic_records(&root, &conn, &target, &[], &config).unwrap();
        let remaining = load_semantic_matches(&conn, &target.target_id).unwrap();
        assert!(remaining.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn semantic_embeddings_fail_fast_when_engine_binary_is_missing() {
        let root = temp_root("semantic-missing-engine");
        let err = supervisor::ensure_auxiliary_backend_launchable(
            &root,
            crate::inference::engine::AuxiliaryRole::Embedding,
        )
        .expect_err("missing ctox-engine should fail fast");
        assert!(
            err.to_string()
                .contains("embedding backend requires ctox-engine"),
            "unexpected error: {err}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn apply_enrichment_updates_builds_nested_fields() {
        let config = EnrichmentConfig {
            enabled: true,
            model: runtime_state::default_primary_model(),
            timeout_seconds: 30,
            max_records: 10,
            source_fields: vec!["title".to_string()],
            tasks: vec![
                EnrichmentTaskConfig {
                    kind: "classify".to_string(),
                    output_field: "classification".to_string(),
                    instruction: "classify".to_string(),
                    field_hints: vec!["category".to_string()],
                    filter_field_hints: vec!["classification.category".to_string()],
                },
                EnrichmentTaskConfig {
                    kind: "extract".to_string(),
                    output_field: "structured".to_string(),
                    instruction: "extract".to_string(),
                    field_hints: vec!["remote".to_string()],
                    filter_field_hints: vec!["structured.remote".to_string()],
                },
            ],
        };
        let updated = apply_enrichment_updates(
            &json!({"title": "Rust Engineer"}),
            &config,
            &[
                EnrichmentUpdate {
                    path: "classification.category".to_string(),
                    value: json!("job"),
                },
                EnrichmentUpdate {
                    path: "structured.remote".to_string(),
                    value: json!(true),
                },
            ],
        )
        .unwrap();
        assert_eq!(
            json_lookup_path(&updated, "classification.category").and_then(Value::as_str),
            Some("job")
        );
        assert_eq!(
            json_lookup_path(&updated, "structured.remote").and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn write_target_api_files_preserves_existing_enrichment_template() {
        let root = temp_root("template-preserve");
        let target = upsert_target(
            &root,
            DEFAULT_RUNTIME_ROOT,
            json!({
                "target_key": "preserve-target",
                "display_name": "Preserve Target",
                "start_url": "https://example.com/jobs",
                "target_kind": "jobs",
                "config": {"skip_probe": true}
            }),
        )
        .unwrap();
        let template_path = resolve_workspace_dir(&root, &target.workspace_dir)
            .join("api/llm_enrichment_template.json");
        fs::write(
            &template_path,
            serde_json::to_string_pretty(&json!({
                "enabled": true,
                "model": "custom/model",
                "timeout_seconds": 5,
                "max_records": 2,
                "source_fields": ["title"],
                "tasks": [{
                    "kind": "summarize",
                    "output_field": "semantic_summary",
                    "instruction": "custom"
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        write_target_manifest(&root, &target).unwrap();
        let preserved: Value =
            serde_json::from_str(&fs::read_to_string(&template_path).unwrap()).unwrap();
        assert_eq!(
            preserved.get("model").and_then(Value::as_str),
            Some("custom/model")
        );
        assert_eq!(
            preserved.get("enabled").and_then(Value::as_bool),
            Some(true)
        );
        let _ = fs::remove_dir_all(root);
    }
}
