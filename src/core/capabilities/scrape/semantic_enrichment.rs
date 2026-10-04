// LLM enrichment and semantic search: configs, prompt build, response
// parsing/apply, embedding transport (local socket first), index cache.

use super::registry::open_db;
use super::{
    artifact_record, build_target_api_contract, canonical_json, compute_sha256,
    extract_first_json_object, invoke_responses_text, json_lookup_path,
    load_all_latest_active_records, load_target_view, now_iso_string, scalarish_string,
    set_json_path, tail_excerpt, target_api_dir, truncate_chars, EnrichmentOutcome,
    EnrichmentTaskConfig, EnrichmentUpdate, LatestRecordView, LocalEmbeddingSocketRequest,
    LocalEmbeddingSocketResponse, RegisteredTarget, ScrapeTargetView, SemanticMatch,
    DEFAULT_ENRICHMENT_MAX_RECORDS,
};
use crate::inference::engine;
use crate::inference::local_transport::LocalTransport;
use crate::inference::model_registry;
use crate::inference::runtime_kernel;
use crate::inference::runtime_state;
use crate::inference::supervisor;
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead as _, BufReader, Write as _};
use std::path::Path;
use std::time::Duration;

pub(super) fn default_embedding_model() -> &'static str {
    model_registry::default_auxiliary_model(engine::AuxiliaryRole::Embedding)
        .expect("default embedding model must exist in the model registry")
}

#[derive(Debug, Clone)]
pub(super) struct SemanticConfig {
    pub(super) enabled: bool,
    pub(super) source_fields: Vec<String>,
    pub(super) embedding_model: String,
    pub(super) default_limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct EnrichmentConfig {
    pub(super) enabled: bool,
    pub(super) model: String,
    pub(super) timeout_seconds: u64,
    pub(super) max_records: usize,
    pub(super) source_fields: Vec<String>,
    pub(super) tasks: Vec<EnrichmentTaskConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct EnrichmentResponse {
    #[serde(default)]
    updates: Vec<EnrichmentUpdate>,
    #[serde(default)]
    notes: Option<String>,
}

pub(super) fn rebuild_semantic_index(root: &Path, target_key: &str) -> Result<Option<Value>> {
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

pub(super) fn semantic_search(
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

pub(crate) fn service_semantic_search(
    root: &Path,
    target_key: &str,
    query: &str,
    limit: usize,
) -> Result<Option<Value>> {
    semantic_search(root, target_key, query, limit)
}

pub(super) fn default_semantic_config_for_target(target: &ScrapeTargetView) -> SemanticConfig {
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

pub(super) fn load_semantic_config(root: &Path, target: &ScrapeTargetView) -> SemanticConfig {
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

pub(super) fn default_llm_enrichment_config(
    root: &Path,
    _target: &ScrapeTargetView,
) -> EnrichmentConfig {
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

pub(super) fn load_llm_enrichment_config(
    root: &Path,
    target: &ScrapeTargetView,
) -> EnrichmentConfig {
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

pub(super) fn enrichment_filter_paths(config: &EnrichmentConfig) -> Vec<String> {
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

pub(super) fn maybe_run_llm_enrichment(
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

pub(super) fn enrich_single_record(
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

pub(super) fn build_enrichment_prompt(config: &EnrichmentConfig, record: &Value) -> String {
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

pub(super) fn enrichment_source_text(record: &Value, config: &EnrichmentConfig) -> Option<String> {
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

pub(super) fn parse_enrichment_response(raw_text: &str) -> Result<EnrichmentResponse> {
    let trimmed = raw_text.trim();
    let candidate = extract_first_json_object(trimmed).unwrap_or(trimmed);
    serde_json::from_str::<EnrichmentResponse>(candidate)
        .context("failed to parse enrichment json object")
}

pub(super) fn apply_enrichment_updates(
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

pub(super) fn is_allowed_enrichment_path(config: &EnrichmentConfig, path: &str) -> bool {
    config.tasks.iter().any(|task| {
        let root = task.output_field.trim();
        !root.is_empty() && (path == root || path.starts_with(&format!("{root}.")))
    })
}

pub(super) fn semantic_text_for_record(record: &Value, config: &SemanticConfig) -> Option<String> {
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

pub(super) fn ensure_semantic_records(
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

pub(super) fn load_semantic_cache(
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

pub(super) fn load_semantic_matches(
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

pub(super) fn embed_texts(root: &Path, inputs: &[String], model: &str) -> Result<Vec<Vec<f64>>> {
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

pub(super) fn embed_texts_via_local_socket(
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
    // Decode the tag before the typed payload: Serde's tagged-enum buffer
    // cannot replay fractional numbers with serde_json/arbitrary_precision.
    // https://github.com/serde-rs/json/issues/721
    #[derive(serde::Deserialize)]
    struct EmbeddingsReply {
        #[serde(rename = "model")]
        _model: String,
        data: Vec<Vec<f32>>,
        #[serde(rename = "prompt_tokens")]
        _prompt_tokens: u32,
        #[serde(rename = "total_tokens")]
        _total_tokens: u32,
    }
    #[derive(serde::Deserialize)]
    struct ErrorReply {
        code: String,
        message: String,
    }
    let response: LocalEmbeddingSocketResponse =
        serde_json::from_str(line.trim()).context("failed to parse embedding socket response")?;
    match response.kind.as_str() {
        "embeddings" => {
            let reply: EmbeddingsReply = serde_json::from_str(line.trim())
                .context("failed to parse embedding vector payload")?;
            Ok(reply
                .data
                .into_iter()
                .map(|values| values.into_iter().map(|value| value as f64).collect())
                .collect())
        }
        "error" => {
            let reply: ErrorReply = serde_json::from_str(line.trim())
                .context("failed to parse embedding socket error")?;
            anyhow::bail!("{}: {}", reply.code, reply.message);
        }
        kind => anyhow::bail!("unknown embedding socket response kind: {kind}"),
    }
}

pub(super) fn cosine_similarity(left: &[f64], right: &[f64]) -> f64 {
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
