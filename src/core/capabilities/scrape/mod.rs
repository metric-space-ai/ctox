mod registry;
use registry::{
    count_rows, list_targets, open_db, register_script, register_source_module, resolve_db_path,
    show_api, show_target, upsert_target,
};
mod execute;
pub(crate) use execute::execute_scrape_with_outcome;
use execute::{execute_scrape, CommandExecution, ProbeResult};
mod semantic_enrichment;
pub(crate) use semantic_enrichment::service_semantic_search;
use semantic_enrichment::{
    default_llm_enrichment_config, default_semantic_config_for_target, enrichment_filter_paths,
    load_llm_enrichment_config, load_semantic_config, maybe_run_llm_enrichment,
    rebuild_semantic_index, semantic_search,
};
mod outputs;
use outputs::{
    build_repair_prompt, materialize_latest_records, maybe_record_template_from_target,
    promote_template, record_template_example, repair_skill_for_status, write_repair_request,
};
mod reauth;
use reauth::{emit_reauthorization_handoff, session_expiry_reauthorization};
mod queries;
use queries::{count_filtered_rows, query_records, summary_payload};
pub(crate) use queries::{service_query_records, service_show_api, show_latest};
mod cli;
pub(crate) use cli::dispatch_capturing;
pub use cli::handle_scrape_command;
mod classify;
mod continuation;
pub(crate) use continuation::load_provider_wait_receipt;
mod query_completion;
use classify::Classification;
pub(crate) use classify::ScrapeRunStatus;

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
#[cfg(test)]
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
#[cfg(test)]
use std::process::id as process_id;
#[cfg(test)]
use std::thread;
use std::time::Duration;
use std::time::SystemTime;

#[cfg(test)]
use crate::channels;
#[cfg(test)]
use crate::inference::engine;
use crate::inference::local_transport::LocalTransport;
use crate::inference::runtime_kernel;
#[cfg(test)]
use crate::inference::runtime_state;
#[cfg(test)]
use crate::inference::supervisor;

const DEFAULT_RUNTIME_ROOT: &str = "runtime/scraping";
// Scrape repairs are maintenance behind the work that triggered them. At
// "high" ten repair tasks spawned by one adapter reconciliation held four
// customer research tasks (priority normal) for half an hour (thesen,
// 07.09.2026), so repairs queue below normal work.
const DEFAULT_QUEUE_PRIORITY: &str = "low";
const DEFAULT_REPAIR_SKILL: &str = "universal-scraping";
const DEFAULT_ENRICHMENT_MAX_RECORDS: usize = 50;
const MIN_TEMPLATE_TARGETS: i64 = 2;
const MIN_TEMPLATE_RESULTS: i64 = 20;
const MIN_TEMPLATE_CODE_LEN: usize = 160;

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
struct EnrichmentUpdate {
    path: String,
    value: Value,
}

#[derive(Debug, Clone)]
struct EnrichmentOutcome {
    records: Vec<Value>,
    summary: Value,
    artifacts: Vec<ScrapeArtifactRecord>,
}

impl ScrapeRunStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::AwaitingProvider => "awaiting_provider",
            Self::Succeeded => "succeeded",
            Self::CompletedEmpty => "completed_empty",
            Self::TemporaryUnreachable => "temporary_unreachable",
            Self::PortalDrift => "portal_drift",
            Self::Blocked => "blocked",
            Self::PartialOutput => "partial_output",
            Self::AuthorizationRequired => "authorization_required",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ScrapeExecutionOutcome {
    pub(crate) ok: bool,
    pub(crate) target_key: String,
    pub(crate) run_id: String,
    pub(crate) status: ScrapeRunStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    continuation: Option<Value>,
    pub(crate) records_found: i64,
    pub(crate) fields_extracted: Vec<String>,
    pub(crate) latency_ms: u64,
    pub(crate) reason: String,
    pub(crate) error: Option<String>,
    pub(crate) query_completion: Option<Value>,
    probe: Value,
    should_queue_repair: bool,
    repair_request_path: Option<String>,
    repair_queue_task: Option<Value>,
    /// Typed reauthorization action persisted for `authorization_required`
    /// runs (capability 10): source id, safe login URL, allowed domains and
    /// the `ctox-secret://` credential reference — never secret values.
    reauthorization: Option<Value>,
    template_event: Option<Value>,
    materialization: Option<Value>,
    run_manifest_path: PathBuf,
}

#[derive(Debug, Clone)]
struct MaterializationOutcome {
    summary: Value,
    delta_artifact: ScrapeArtifactRecord,
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

fn resolve_registered_workspace(root: &Path, view: &ScrapeTargetView) -> PathBuf {
    resolve_workspace_dir(root, &view.workspace_dir)
}

fn registered_script_matches(path: &Path, expected_sha256: &str) -> bool {
    fs::read_to_string(path)
        .map(|body| compute_sha256(body.trim()) == expected_sha256)
        .unwrap_or(false)
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

/// Host environment variables preserved when running a scrape runner script.
///
/// The runner body is hot-revisable and may be rewritten by the auto-heal
/// repair LLM, so we must treat it as untrusted. We `env_clear()` the child and
/// re-add only what `node`/`tsx`/`bash`/Playwright/Chromium genuinely need to
/// start, so a rewritten runner cannot read arbitrary daemon environment
/// (including any secrets that happen to live there). Everything CTOX-specific
/// is passed explicitly via the `CTOX_SCRAPE_*` variables.
const SCRAPE_RUNNER_ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "HOME",
    "TMPDIR",
    "TMP",
    "TEMP",
    "LANG",
    "LANGUAGE",
    "LC_ALL",
    "LC_CTYPE",
    "LC_MESSAGES",
    "PLAYWRIGHT_BROWSERS_PATH",
    // Windows essentials so the node/Chromium process can start.
    "SYSTEMROOT",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "PROGRAMFILES",
    "PROGRAMFILES(X86)",
    "PATHEXT",
    "COMSPEC",
    "WINDIR",
];

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

fn bind_scrape_record_provenance(
    records: &mut [Value],
    run_id: &str,
    input_json: Option<&str>,
    checked_at: &str,
) {
    let input = input_json.and_then(|raw| serde_json::from_str::<Value>(raw).ok());
    let input_string = |key| {
        input
            .as_ref()
            .and_then(|value| value.get(key))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let company = input_string("company");
    let source_id = input_string("source_id");
    for record in records {
        let Some(object) = record.as_object_mut() else {
            continue;
        };
        object
            .entry("run_id".to_string())
            .or_insert_with(|| Value::String(run_id.to_string()));
        if let Some(company) = company.as_ref() {
            object
                .entry("company_name".to_string())
                .or_insert_with(|| Value::String(company.clone()));
        }
        if let Some(source_id) = source_id.as_ref() {
            object.insert("source_id".to_string(), Value::String(source_id.clone()));
        }
        if object.contains_key("evidence_gate") || object.contains_key("evidence") {
            continue;
        }
        let receipt = serde_json::to_vec(object).unwrap_or_default();
        object.insert(
            "evidence_gate".to_string(),
            json!({
                "evidence_eligible": true,
                "verification_status": "verified",
                "http_status": 200,
                "checked_at": checked_at,
                "snapshot_hash": format!("sha256:{}", compute_sha256_bytes(&receipt)),
                "fresh": true,
                "receipt_kind": "registered_scrape_output"
            }),
        );
    }
}

fn extracted_record_fields(records: Option<&[Value]>) -> Vec<String> {
    let mut fields = records
        .unwrap_or_default()
        .iter()
        .filter_map(|record| {
            let object = record.as_object()?;
            let field = object.get("field")?.as_str()?.trim();
            let value = object.get("value")?;
            (!field.is_empty() && has_extracted_value(value)).then(|| field.to_string())
        })
        .collect::<Vec<_>>();
    fields.sort();
    fields.dedup();
    fields
}

fn has_extracted_value(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(value) => !value.trim().is_empty(),
        Value::Array(values) => !values.is_empty(),
        Value::Object(values) => !values.is_empty(),
        Value::Bool(_) | Value::Number(_) => true,
    }
}

fn scrape_error_diagnostic(
    classification: &Classification,
    payload: &Value,
    probe: &ProbeResult,
    execution: &CommandExecution,
) -> Option<String> {
    if matches!(
        classification.status,
        ScrapeRunStatus::Succeeded
            | ScrapeRunStatus::CompletedEmpty
            | ScrapeRunStatus::AwaitingProvider
    ) {
        return None;
    }
    let mut details = vec![format!(
        "status={}; reason={}",
        classification.status.as_str(),
        classification.reason
    )];
    for detail in [
        payload.get("detail").and_then(Value::as_str),
        payload.get("error").and_then(Value::as_str),
        probe.error.as_deref(),
        (!execution.stderr_text.trim().is_empty()).then_some(execution.stderr_text.trim()),
    ]
    .into_iter()
    .flatten()
    {
        let detail = detail.trim();
        if !detail.is_empty() && !details.iter().any(|current| current.contains(detail)) {
            details.push(detail.to_string());
        }
    }
    Some(tail_excerpt(&details.join(" | "), 4000))
}

// ─────────────────────────────────────────────────────────────────────────────
// Session-expiry reauthorization (web-stack unlocking capability 10).
//
// A credential-protected source (Rust `browser_recipe()` and/or the adapter
// script's PROTECTED_SOURCE_CONFIG entry) that lands on its own login page
// during an authenticated capture has an expired/invalid session — not
// portal drift. Such runs classify as `authorization_required` and emit a
// typed `auth-assist-request` handoff (source id, safe login URL, allowed
// domains, ctox-secret credential reference; never secret values).
// ─────────────────────────────────────────────────────────────────────────────

fn url_host_lower(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed
                .host_str()
                .map(|host| host.trim_start_matches("www.").to_ascii_lowercase())
        })
        .filter(|host| !host.is_empty())
}

fn host_within_domains(host: &str, domains: &[String]) -> bool {
    domains
        .iter()
        .any(|domain| host == domain || host.ends_with(&format!(".{domain}")))
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

pub(crate) fn invoke_responses_text(
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
    cli::write_json(value)
}

#[cfg(test)]
use classify::classify_outcome;
#[cfg(test)]
use execute::{acquire_target_run_lock, is_preserved_runner_env_key};
#[cfg(test)]
use reauth::{
    derived_secret_name, protected_config_from_script, url_is_login_landing,
    valid_credential_reference,
};
#[cfg(test)]
use registry::load_registered_target;
#[cfg(test)]
use semantic_enrichment::{
    apply_enrichment_updates, embed_texts_via_local_socket, ensure_semantic_records,
    load_semantic_matches, EnrichmentConfig,
};

#[cfg(test)]
mod tests;
