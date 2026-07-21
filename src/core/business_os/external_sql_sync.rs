// Origin: CTOX
// License: AGPL-3.0-only

//! Declarative external SQL synchronization for local Business OS apps.
//!
//! CTOX owns the connector, scheduling, SQLite/RxDB projection, writeback
//! receipts, conflict checks, and policy-gated command execution. Local apps
//! own only their server-side mapping declaration. No customer schema or
//! business workflow is compiled into this module.

use super::policy::BusinessOsPermission;
use super::store::{self, BusinessCommand};
use anyhow::{bail, Context};
use ctox_sqlserver_adapter::{
    validate_read_statement, validate_write_statement, SqlParameter, SqlServerAdapter,
    SqlServerConfig,
};
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const SQLSERVER_KIND: &str = "sqlserver";
const SYNC_COMMAND: &str = "external_sql.sync.refresh";
const WRITE_COMMAND: &str = "external_sql.write";
const BACKGROUND_POLL_SECONDS: u64 = 30;
const MIN_SYNC_INTERVAL_SECONDS: u64 = 30;
const MAX_SYNC_INTERVAL_SECONDS: u64 = 86_400;
const DEFAULT_SYNC_INTERVAL_SECONDS: u64 = 300;
const DEFAULT_PAGE_SIZE: usize = 1_000;
const MAX_PAGE_SIZE: usize = 5_000;

static ACTIVE_SOURCES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExternalSqlSourceConfig {
    module_id: String,
    id: String,
    #[serde(default = "default_source_kind")]
    kind: String,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    display_name: String,
    connection: ExternalSqlConnectionConfig,
    #[serde(default = "default_sync_interval_seconds")]
    sync_interval_seconds: u64,
    status_collection: String,
    #[serde(default)]
    status_record_id: String,
    #[serde(default)]
    projections: Vec<ProjectionConfig>,
    #[serde(default)]
    write_operations: Vec<WriteOperationConfig>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExternalSqlConnectionConfig {
    server: String,
    #[serde(default = "default_sqlserver_port")]
    port: u16,
    database: String,
    user: String,
    password_secret: String,
    #[serde(default = "default_true")]
    encrypt: bool,
    #[serde(default)]
    trust_server_certificate: bool,
    #[serde(default = "default_request_timeout_ms")]
    request_timeout_ms: u64,
    #[serde(default = "default_connector_max_rows")]
    max_rows: usize,
    #[serde(default)]
    allow_writes: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectionConfig {
    id: String,
    collection: String,
    record_id_field: String,
    query: String,
    #[serde(default)]
    readback_query: String,
    #[serde(default)]
    parameters: Vec<SqlParameter>,
    #[serde(default = "default_true")]
    paged: bool,
    #[serde(default = "default_page_size")]
    page_size: usize,
    #[serde(default)]
    max_records: Option<usize>,
    #[serde(default = "default_true")]
    reconcile_deletions: bool,
    #[serde(default)]
    record_id_prefix: String,
    #[serde(default)]
    field_map: BTreeMap<String, String>,
    #[serde(default)]
    boolean_fields: Vec<String>,
    #[serde(default)]
    number_fields: Vec<String>,
    #[serde(default)]
    search_fields: Vec<String>,
    #[serde(default)]
    source_payload_path: String,
    #[serde(default)]
    incremental_cursor: Option<IncrementalCursorConfig>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IncrementalCursorConfig {
    cursor_field: String,
    #[serde(rename = "type")]
    value_type: String,
    initial_value: Value,
    #[serde(default)]
    deleted_field: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteOperationConfig {
    id: String,
    #[serde(default = "default_true")]
    transaction: bool,
    #[serde(default)]
    version_check: Option<VersionCheckConfig>,
    #[serde(default)]
    source_receipt: Option<SourceReceiptConfig>,
    statements: Vec<StatementConfig>,
    #[serde(default)]
    refresh: Vec<RefreshQueryConfig>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceReceiptConfig {
    lookup_query: String,
    #[serde(default)]
    lookup_parameters: Vec<ParameterBinding>,
    #[serde(default = "default_payload_hash_field")]
    payload_hash_field: String,
    claim: StatementConfig,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StatementConfig {
    sql: String,
    #[serde(default)]
    parameters: Vec<ParameterBinding>,
    #[serde(default)]
    when_changed_field: String,
    #[serde(default)]
    returns_rows: bool,
    #[serde(default)]
    conflict_if_zero_rows: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionCheckConfig {
    sql: String,
    #[serde(default)]
    parameters: Vec<ParameterBinding>,
    #[serde(default = "default_source_version_field")]
    result_field: String,
    #[serde(default = "default_expected_source_version_path")]
    expected_path: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RefreshQueryConfig {
    #[serde(default)]
    projection_id: String,
    collection: String,
    record_id_field: String,
    #[serde(default)]
    query: String,
    #[serde(default)]
    parameters: Vec<ParameterBinding>,
    #[serde(default)]
    record_id_prefix: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ParameterBinding {
    path: String,
    #[serde(rename = "type")]
    value_type: String,
    #[serde(default = "default_true")]
    required: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RunStatus {
    Syncing,
    Ready,
    Error,
}

impl RunStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Syncing => "syncing",
            Self::Ready => "ready",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Debug, Default)]
struct ProjectionStats {
    source_count: usize,
    inserted_count: usize,
    updated_count: usize,
    unchanged_count: usize,
    deleted_count: usize,
    complete: bool,
    truncated: bool,
    next_offset: usize,
    cursor: Option<Value>,
    resumable: bool,
}

#[derive(Clone, Debug)]
struct WritebackReceipt {
    module_id: String,
    source_id: String,
    operation_id: String,
    payload_hash: String,
    stage: String,
    result: Value,
    error_text: String,
}

#[derive(Debug)]
struct ExternalSqlWriteConflict(String);

impl fmt::Display for ExternalSqlWriteConflict {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ExternalSqlWriteConflict {}

pub(crate) fn is_external_sql_command(command_type: &str) -> bool {
    matches!(command_type, SYNC_COMMAND | WRITE_COMMAND)
}

pub(crate) fn data_write_permission() -> BusinessOsPermission {
    BusinessOsPermission::DataWrite
}

pub(crate) fn server_owned_projection_collections(
    root: &Path,
) -> anyhow::Result<std::collections::HashSet<String>> {
    let sources = load_source_configs(root)?;
    let mut collections = std::collections::HashSet::new();
    for source in sources {
        collections.insert(source.status_collection);
        collections.extend(
            source
                .projections
                .into_iter()
                .map(|projection| projection.collection),
        );
        collections.extend(
            source
                .write_operations
                .into_iter()
                .flat_map(|operation| operation.refresh)
                .map(|refresh| refresh.collection),
        );
    }
    Ok(collections)
}

pub(crate) fn projection_collection_is_server_owned(root: &Path, collection: &str) -> bool {
    match server_owned_projection_collections(root) {
        Ok(collections) => collections.contains(collection),
        // Fail closed: when projection ownership cannot be resolved (e.g. an
        // invalid local mapping), treat the collection as server-owned so no
        // peer write slips into a projection mirror.
        Err(_) => true,
    }
}

pub(crate) fn handle_business_command(
    root: &Path,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    let source_id = required_string(&command.payload, "source_id")?;
    let source = load_source_configs(root)?
        .into_iter()
        .find(|source| source.module_id == command.module && source.id == source_id)
        .with_context(|| {
            format!(
                "external SQL source `{source_id}` is not registered for local module `{}`",
                command.module
            )
        })?;
    validate_source(&source)?;
    let source_key = format!("{}:{}", source.module_id, source.id);
    let _lease = SourceLease::acquire(source_key)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build external SQL runtime")?;
    runtime.block_on(async {
        match command.command_type.as_str() {
            SYNC_COMMAND => sync_source(root, &source, command.id.as_deref()).await,
            WRITE_COMMAND => write_source(root, &source, command).await,
            other => bail!("unsupported external SQL command: {other}"),
        }
    })
}

pub(crate) fn start_background_sync(root: &Path) {
    let root = root.to_path_buf();
    std::thread::spawn(move || loop {
        if let Err(error) = run_due_sources(&root) {
            eprintln!("CTOX external SQL background sync failed: {error:#}");
        }
        std::thread::sleep(Duration::from_secs(BACKGROUND_POLL_SECONDS));
    });
}

fn run_due_sources(root: &Path) -> anyhow::Result<()> {
    let now = now_ms();
    for source in load_source_configs(root)? {
        if !source.enabled
            || validate_source(&source).is_err()
            || !source_is_due(root, &source, now)?
        {
            continue;
        }
        let key = format!("{}:{}", source.module_id, source.id);
        let Ok(_lease) = SourceLease::acquire(key) else {
            continue;
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("build external SQL background runtime")?;
        if let Err(error) = runtime.block_on(sync_source(root, &source, None)) {
            eprintln!(
                "External SQL source {}/{} failed: {error:#}",
                source.module_id, source.id
            );
        }
    }
    Ok(())
}

async fn sync_source(
    root: &Path,
    source: &ExternalSqlSourceConfig,
    requested_run_id: Option<&str>,
) -> anyhow::Result<Value> {
    let started_at_ms = now_ms();
    let run_id = requested_run_id
        .map(str::to_owned)
        .unwrap_or_else(|| format!("external-sql-{}-{started_at_ms}", source.id));
    project_status(
        root,
        source,
        &run_id,
        RunStatus::Syncing,
        started_at_ms,
        None,
        &BTreeMap::new(),
        None,
    )?;
    let mut adapter = adapter_for_source(root, source)?;
    let mut all_stats = BTreeMap::new();
    for projection in &source.projections {
        match sync_projection(root, &mut adapter, source, projection).await {
            Ok(stats) => {
                all_stats.insert(projection.id.clone(), stats);
                project_status(
                    root,
                    source,
                    &run_id,
                    RunStatus::Syncing,
                    started_at_ms,
                    None,
                    &all_stats,
                    None,
                )?;
            }
            Err(error) => {
                let error_text = format!("{error:#}");
                project_status(
                    root,
                    source,
                    &run_id,
                    RunStatus::Error,
                    started_at_ms,
                    Some(now_ms()),
                    &all_stats,
                    Some(&error_text),
                )?;
                return Err(error);
            }
        }
    }
    let finished_at_ms = now_ms();
    project_status(
        root,
        source,
        &run_id,
        RunStatus::Ready,
        started_at_ms,
        Some(finished_at_ms),
        &all_stats,
        None,
    )?;
    Ok(json!({
        "ok": true,
        "source_id": source.id,
        "module_id": source.module_id,
        "run_id": run_id,
        "started_at_ms": started_at_ms,
        "finished_at_ms": finished_at_ms,
        "projections": stats_json(&all_stats),
    }))
}

async fn sync_projection(
    root: &Path,
    adapter: &mut SqlServerAdapter,
    source: &ExternalSqlSourceConfig,
    projection: &ProjectionConfig,
) -> anyhow::Result<ProjectionStats> {
    let existing = existing_projection_fingerprints(root, source, projection)?;
    let mut seen = BTreeSet::new();
    let mut writer = store::BusinessProjectionWriter::open(root)?;
    let mut stats = ProjectionStats::default();
    let limit = projection.max_records.unwrap_or(usize::MAX);
    let mut cursor = projection
        .incremental_cursor
        .as_ref()
        .map(|config| load_projection_cursor(root, source, projection, config))
        .transpose()?;
    if let Some(cursor) = &cursor {
        stats.cursor = Some(cursor.clone());
        stats.resumable = true;
    }
    loop {
        let mut parameters = projection.parameters.clone();
        if let (Some(config), Some(cursor)) = (&projection.incremental_cursor, &cursor) {
            parameters.push(sql_parameter_from_value(cursor, &config.value_type)?);
            parameters.push(SqlParameter::I32(projection.page_size as i32));
        } else if projection.paged {
            parameters.push(SqlParameter::I64(stats.next_offset as i64));
            parameters.push(SqlParameter::I32(projection.page_size as i32));
        }
        let rows = adapter.query(&projection.query, &parameters).await?;
        if !projection.paged && rows.len() > projection.page_size {
            bail!(
                "unpaged projection `{}` returned {} rows, exceeding page_size {}",
                projection.id,
                rows.len(),
                projection.page_size
            );
        }
        let row_count = rows.len();
        for source_row in rows {
            if stats.source_count >= limit {
                stats.truncated = true;
                break;
            }
            let record_id_value = value_at_path(&source_row, &projection.record_id_field)
                .with_context(|| {
                    format!(
                        "projection `{}` row is missing record id field `{}`",
                        projection.id, projection.record_id_field
                    )
                })?;
            let record_id = format!(
                "{}{}",
                projection.record_id_prefix,
                scalar_string(record_id_value)?
            );
            let next_cursor = if let Some(config) = &projection.incremental_cursor {
                let value =
                    value_at_path(&source_row, &config.cursor_field).with_context(|| {
                        format!(
                            "incremental projection `{}` row is missing cursor field `{}`",
                            projection.id, config.cursor_field
                        )
                    })?;
                validate_cursor_value(value, &config.value_type)?;
                if cursor.as_ref().is_some_and(|current| {
                    !cursor_is_strictly_after(value, current, &config.value_type)
                }) {
                    bail!(
                        "incremental projection `{}` returned a non-increasing cursor",
                        projection.id
                    );
                }
                Some(value.clone())
            } else {
                None
            };
            let explicitly_deleted = projection
                .incremental_cursor
                .as_ref()
                .filter(|config| !config.deleted_field.is_empty())
                .and_then(|config| value_at_path(&source_row, &config.deleted_field))
                .is_some_and(value_as_bool);
            let mut row = if explicitly_deleted {
                json!({
                    "id": record_id,
                    "is_deleted": true,
                    "deleted_at_ms": now_ms(),
                })
            } else {
                transform_projection_row(source_row, projection)?
            };
            let object = row.as_object_mut().with_context(|| {
                format!(
                    "projection `{}` query must return object rows",
                    projection.id
                )
            })?;
            object.insert("id".to_string(), Value::String(record_id.clone()));
            object.insert("is_deleted".to_string(), Value::Bool(explicitly_deleted));
            object.insert(
                "_ctox_external_sql".to_string(),
                projection_provenance(source, projection),
            );
            let updated_at_ms = object
                .get("updated_at_ms")
                .and_then(Value::as_i64)
                .unwrap_or_else(now_ms);
            object.insert("updated_at_ms".to_string(), json!(updated_at_ms));
            if explicitly_deleted {
                writer.upsert(&projection.collection, &record_id, updated_at_ms, row)?;
                stats.deleted_count += 1;
            } else {
                let fingerprint = payload_fingerprint(&row);
                match existing.get(&record_id) {
                    None => stats.inserted_count += 1,
                    Some(existing) if existing != &fingerprint => stats.updated_count += 1,
                    Some(_) => stats.unchanged_count += 1,
                }
                if existing.get(&record_id) != Some(&fingerprint) {
                    writer.upsert(&projection.collection, &record_id, updated_at_ms, row)?;
                }
            }
            seen.insert(record_id);
            stats.source_count += 1;
            stats.next_offset += 1;
            if let Some(next_cursor) = next_cursor {
                cursor = Some(next_cursor.clone());
                stats.cursor = Some(next_cursor);
            }
        }
        if let Some(cursor) = &cursor {
            store_projection_cursor(root, source, projection, cursor)?;
        }
        if stats.truncated || !projection.paged || row_count < projection.page_size {
            stats.complete = !stats.truncated;
            break;
        }
        if row_count == 0 {
            stats.complete = true;
            break;
        }
    }
    if projection.incremental_cursor.is_none() && projection.reconcile_deletions && stats.complete {
        for record_id in existing
            .keys()
            .filter(|record_id| !seen.contains(*record_id))
        {
            let deleted_at_ms = now_ms();
            writer.upsert(
                &projection.collection,
                record_id,
                deleted_at_ms,
                json!({
                    "id": record_id,
                    "is_deleted": true,
                    "deleted_at_ms": deleted_at_ms,
                    "updated_at_ms": deleted_at_ms,
                    "_ctox_external_sql": projection_provenance(source, projection),
                }),
            )?;
            stats.deleted_count += 1;
        }
    }
    Ok(stats)
}

async fn write_source(
    root: &Path,
    source: &ExternalSqlSourceConfig,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    if !source.connection.allow_writes {
        bail!(
            "writes are disabled for external SQL source `{}`",
            source.id
        );
    }
    let operation_id = required_string(&command.payload, "operation_id")?;
    let operation = source
        .write_operations
        .iter()
        .find(|operation| operation.id == operation_id)
        .with_context(|| format!("write operation `{operation_id}` is not registered"))?;
    let command_id = command
        .id
        .as_deref()
        .context("external SQL write requires command id")?;
    ensure_writeback_schema(root)?;
    let payload_hash = write_command_fingerprint(source, &operation_id, &command.payload);
    let execution_payload = write_execution_payload(&command.payload, command_id, &payload_hash)?;
    let existing_receipt = load_writeback_receipt(root, command_id)?;
    if let Some(receipt) = &existing_receipt {
        validate_receipt_identity(receipt, source, &operation_id, &payload_hash)?;
        if receipt.stage == "completed" {
            return Ok(receipt.result.clone());
        }
        if receipt.stage == "pending_source" && operation.source_receipt.is_none() {
            bail!("external SQL write `{command_id}` has an unresolved pending source application");
        }
    }
    if existing_receipt
        .as_ref()
        .is_none_or(|receipt| receipt.stage != "source_applied")
    {
        record_writeback_stage(
            root,
            command_id,
            source,
            &operation_id,
            &payload_hash,
            "pending_source",
            Value::Null,
            None,
        )?;
    }
    let mut adapter = adapter_for_source(root, source)?;
    let result: anyhow::Result<Value> = async {
        let (affected_rows, returned_rows, recovered_from_source_receipt) = match &existing_receipt {
            Some(receipt) if receipt.stage == "source_applied" => (
                receipt
                    .result
                    .get("affected_rows")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                receipt
                    .result
                    .get("returned_rows")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
                receipt
                    .result
                    .get("recovered_from_source_receipt")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            ),
            _ => {
                if let Some(receipt_config) = &operation.source_receipt {
                    if let Some(rows) = lookup_source_receipt(
                        &mut adapter,
                        receipt_config,
                        &execution_payload,
                        &payload_hash,
                    )
                    .await?
                    {
                        let source_result = json!({
                            "affected_rows": 0,
                            "returned_rows": rows,
                            "recovered_from_source_receipt": true,
                        });
                        record_writeback_stage(
                            root,
                            command_id,
                            source,
                            &operation_id,
                            &payload_hash,
                            "source_applied",
                            source_result.clone(),
                            None,
                        )?;
                        (
                            0,
                            source_result["returned_rows"]
                                .as_array()
                                .cloned()
                                .unwrap_or_default(),
                            true,
                        )
                    } else {
                        apply_source_write(
                            root,
                            source,
                            operation,
                            command_id,
                            &operation_id,
                            &payload_hash,
                            &execution_payload,
                            &mut adapter,
                        )
                        .await?
                    }
                } else {
                    apply_source_write(
                        root,
                        source,
                        operation,
                        command_id,
                        &operation_id,
                        &payload_hash,
                        &execution_payload,
                        &mut adapter,
                    )
                    .await?
                }
            }
        };
        let mut refresh_payload = command.payload.clone();
        if let (Some(payload), Some(first_row)) =
            (refresh_payload.as_object_mut(), returned_rows.first())
        {
            payload.insert("result".to_string(), first_row.clone());
        }
        let mut projections = Vec::new();
        for refresh in &operation.refresh {
            let projection = if refresh.projection_id.is_empty() {
                None
            } else {
                Some(
                    source
                        .projections
                        .iter()
                        .find(|projection| projection.id == refresh.projection_id)
                        .with_context(|| {
                            format!(
                                "refresh references unknown projection `{}`",
                                refresh.projection_id
                            )
                        })?,
                )
            };
            let query = if refresh.query.is_empty() {
                projection
                    .map(|projection| projection.readback_query.as_str())
                    .filter(|query| !query.is_empty())
                    .context("refresh requires query or projection readback_query")?
            } else {
                refresh.query.as_str()
            };
            validate_read_sql(query)?;
            let parameters = bind_parameters(&refresh_payload, &refresh.parameters)?;
            let rows = adapter.query(query, &parameters).await?;
            let mut writer = store::BusinessProjectionWriter::open(root)?;
            for source_row in rows {
                let mut row = match projection {
                    Some(projection) => transform_projection_row(source_row, projection)?,
                    None => source_row,
                };
                let record_id = format!("{}{}", refresh.record_id_prefix, scalar_string(value_at_path(&row, &refresh.record_id_field).context("refresh query record id is missing")?)?);
                let object = row.as_object_mut().context("refresh query must return object rows")?;
                object.insert("id".into(), Value::String(record_id.clone()));
                object.insert("is_deleted".into(), Value::Bool(false));
                if let Some(projection) = projection {
                    object.insert(
                        "_ctox_external_sql".into(),
                        projection_provenance(source, projection),
                    );
                }
                let updated_at_ms = object
                    .get("updated_at_ms")
                    .and_then(Value::as_i64)
                    .unwrap_or_else(now_ms);
                object.insert("updated_at_ms".into(), json!(updated_at_ms));
                writer.upsert(&refresh.collection, &record_id, updated_at_ms, row)?;
                projections.push(json!({"collection":refresh.collection,"record_id":record_id}));
            }
        }
        Ok(json!({"ok":true,"source_id":source.id,"operation_id":operation_id,"affected_rows":affected_rows,"returned_rows":returned_rows,"recovered_from_source_receipt":recovered_from_source_receipt,"projections":projections}))
    }
    .await;
    match result {
        Ok(value) => {
            record_writeback_stage(
                root,
                command_id,
                source,
                &operation_id,
                &payload_hash,
                "completed",
                value.clone(),
                None,
            )?;
            Ok(value)
        }
        Err(error) => {
            let conflict = error.downcast_ref::<ExternalSqlWriteConflict>().is_some();
            let error_text = format!("{error:#}");
            let receipt = load_writeback_receipt(root, command_id)?;
            let source_was_applied = receipt
                .as_ref()
                .is_some_and(|receipt| receipt.stage == "source_applied");
            record_writeback_stage(
                root,
                command_id,
                source,
                &operation_id,
                &payload_hash,
                if source_was_applied {
                    "source_applied"
                } else if conflict {
                    "conflict"
                } else {
                    "failed"
                },
                receipt.map(|receipt| receipt.result).unwrap_or(Value::Null),
                Some(&error_text),
            )?;
            Err(error)
        }
    }
}

async fn apply_source_write(
    root: &Path,
    source: &ExternalSqlSourceConfig,
    operation: &WriteOperationConfig,
    command_id: &str,
    operation_id: &str,
    payload_hash: &str,
    payload: &Value,
    adapter: &mut SqlServerAdapter,
) -> anyhow::Result<(u64, Vec<Value>, bool)> {
    if operation.transaction {
        adapter.begin_transaction().await?;
    }
    let result: anyhow::Result<(u64, Vec<Value>, bool)> = async {
        if let Some(check) = &operation.version_check {
            verify_source_version(adapter, check, payload).await?;
        }
        if let Some(receipt) = &operation.source_receipt {
            let (claimed_rows, _) = execute_statement(adapter, &receipt.claim, payload).await?;
            if claimed_rows != 1 {
                return Err(write_conflict(format!(
                    "external SQL idempotency conflict: source receipt claim affected {claimed_rows} rows"
                )));
            }
        }
        let mut affected_rows = 0u64;
        let mut returned_rows = Vec::new();
        for statement in &operation.statements {
            if !statement.when_changed_field.is_empty()
                && !payload_changed_field(payload, &statement.when_changed_field)
            {
                continue;
            }
            let (rows_affected, rows) = execute_statement(adapter, statement, payload).await?;
            if statement.conflict_if_zero_rows && rows_affected == 0 && rows.is_empty() {
                return Err(write_conflict(
                    "external SQL write conflict: conditional write affected no rows",
                ));
            }
            affected_rows += rows_affected;
            returned_rows.extend(rows);
        }
        if operation.transaction {
            adapter.commit_transaction().await?;
        }
        let source_result = json!({
            "affected_rows": affected_rows,
            "returned_rows": returned_rows,
            "recovered_from_source_receipt": false,
        });
        record_writeback_stage(
            root,
            command_id,
            source,
            operation_id,
            payload_hash,
            "source_applied",
            source_result,
            None,
        )?;
        Ok((affected_rows, returned_rows, false))
    }
    .await;
    if result.is_err() && operation.transaction {
        let _ = adapter.rollback_transaction().await;
    }
    result
}

async fn execute_statement(
    adapter: &mut SqlServerAdapter,
    statement: &StatementConfig,
    payload: &Value,
) -> anyhow::Result<(u64, Vec<Value>)> {
    validate_write_sql(&statement.sql)?;
    let parameters = bind_parameters(payload, &statement.parameters)?;
    if statement.returns_rows {
        Ok((
            0,
            adapter
                .execute_returning(&statement.sql, &parameters)
                .await?,
        ))
    } else {
        let result = adapter.execute(&statement.sql, &parameters).await?;
        Ok((result.rows_affected().iter().sum(), Vec::new()))
    }
}

async fn lookup_source_receipt(
    adapter: &mut SqlServerAdapter,
    config: &SourceReceiptConfig,
    payload: &Value,
    expected_payload_hash: &str,
) -> anyhow::Result<Option<Vec<Value>>> {
    validate_read_sql(&config.lookup_query)?;
    let rows = adapter
        .query(
            &config.lookup_query,
            &bind_parameters(payload, &config.lookup_parameters)?,
        )
        .await?;
    validate_source_receipt_rows(rows, config, expected_payload_hash)
}

fn validate_source_receipt_rows(
    rows: Vec<Value>,
    config: &SourceReceiptConfig,
    expected_payload_hash: &str,
) -> anyhow::Result<Option<Vec<Value>>> {
    if rows.is_empty() {
        return Ok(None);
    }
    if rows.len() != 1 {
        return Err(write_conflict(
            "external SQL idempotency conflict: source receipt lookup returned multiple rows",
        ));
    }
    let actual_hash = value_at_path(&rows[0], &config.payload_hash_field)
        .and_then(Value::as_str)
        .ok_or_else(|| {
            write_conflict("external SQL idempotency conflict: source receipt has no payload hash")
        })?;
    if actual_hash != expected_payload_hash {
        return Err(write_conflict(
            "external SQL idempotency conflict: source receipt belongs to different intent",
        ));
    }
    Ok(Some(rows))
}

fn write_execution_payload(
    payload: &Value,
    command_id: &str,
    payload_hash: &str,
) -> anyhow::Result<Value> {
    let mut payload = payload.clone();
    let object = payload
        .as_object_mut()
        .context("external SQL write payload must be an object")?;
    if object.contains_key("ctox_write") {
        bail!("external SQL write payload uses reserved field `ctox_write`");
    }
    object.insert(
        "ctox_write".to_string(),
        json!({"command_id": command_id, "payload_hash": payload_hash}),
    );
    Ok(payload)
}

fn write_conflict(message: impl Into<String>) -> anyhow::Error {
    ExternalSqlWriteConflict(message.into()).into()
}

async fn verify_source_version(
    adapter: &mut SqlServerAdapter,
    check: &VersionCheckConfig,
    payload: &Value,
) -> anyhow::Result<()> {
    validate_read_sql(&check.sql)?;
    let expected = value_at_path(payload, &check.expected_path)
        .context("expected source version is required")?;
    let rows = adapter
        .query(&check.sql, &bind_parameters(payload, &check.parameters)?)
        .await?;
    let actual = rows
        .first()
        .and_then(|row| value_at_path(row, &check.result_field))
        .context("source version check returned no version")?;
    if scalar_string(actual)? != scalar_string(expected)? {
        return Err(write_conflict(
            "external SQL write conflict: source version changed",
        ));
    }
    Ok(())
}

fn adapter_for_source(
    root: &Path,
    source: &ExternalSqlSourceConfig,
) -> anyhow::Result<SqlServerAdapter> {
    if source.kind != SQLSERVER_KIND {
        bail!("unsupported external SQL source kind `{}`", source.kind);
    }
    let password = crate::secrets::get_credential(root, &source.connection.password_secret)
        .with_context(|| {
            format!(
                "secret `{}` is not configured",
                source.connection.password_secret
            )
        })?;
    SqlServerAdapter::new(SqlServerConfig {
        server: source.connection.server.clone(),
        port: source.connection.port,
        database: source.connection.database.clone(),
        user: source.connection.user.clone(),
        password: Some(password),
        password_file: None,
        encrypt: source.connection.encrypt,
        trust_server_certificate: source.connection.trust_server_certificate,
        request_timeout_ms: source.connection.request_timeout_ms,
        max_rows: source.connection.max_rows,
        allow_writes: source.connection.allow_writes,
        application_name: format!("ctox-external-sql-{}", source.id),
    })
}

fn load_source_configs(root: &Path) -> anyhow::Result<Vec<ExternalSqlSourceConfig>> {
    store::local_external_data_source_declarations(root)?
        .into_iter()
        .map(|value| {
            serde_json::from_value(value).context("invalid local external SQL declaration")
        })
        .collect()
}

fn validate_source(source: &ExternalSqlSourceConfig) -> anyhow::Result<()> {
    for (name, value) in [
        ("module_id", source.module_id.as_str()),
        ("id", source.id.as_str()),
        ("status_collection", source.status_collection.as_str()),
        ("connection.server", source.connection.server.as_str()),
        ("connection.database", source.connection.database.as_str()),
        ("connection.user", source.connection.user.as_str()),
        (
            "connection.password_secret",
            source.connection.password_secret.as_str(),
        ),
    ] {
        if value.trim().is_empty() {
            bail!("external SQL {name} must not be empty");
        }
    }
    validate_identifier(&source.module_id, "module_id")?;
    validate_identifier(&source.id, "source id")?;
    validate_collection(&source.status_collection)?;
    if !(MIN_SYNC_INTERVAL_SECONDS..=MAX_SYNC_INTERVAL_SECONDS)
        .contains(&source.sync_interval_seconds)
    {
        bail!("sync_interval_seconds must be between {MIN_SYNC_INTERVAL_SECONDS} and {MAX_SYNC_INTERVAL_SECONDS}");
    }
    if source.projections.is_empty() {
        bail!("external SQL source must declare at least one projection");
    }
    let mut ids = HashSet::new();
    for projection in &source.projections {
        validate_identifier(&projection.id, "projection id")?;
        validate_collection(&projection.collection)?;
        validate_json_path(&projection.record_id_field)?;
        validate_read_sql(&projection.query)?;
        if !projection.readback_query.is_empty() {
            validate_read_sql(&projection.readback_query)?;
        }
        if !ids.insert(projection.id.as_str()) {
            bail!("duplicate projection id `{}`", projection.id);
        }
        if projection.page_size == 0 || projection.page_size > MAX_PAGE_SIZE {
            bail!("projection page_size must be between 1 and {MAX_PAGE_SIZE}");
        }
        if projection.page_size > source.connection.max_rows {
            bail!("projection page_size exceeds connection max_rows");
        }
        if let Some(cursor) = &projection.incremental_cursor {
            if !projection.paged {
                bail!(
                    "incremental projection `{}` must enable paging",
                    projection.id
                );
            }
            validate_json_path(&cursor.cursor_field)?;
            if !matches!(cursor.value_type.as_str(), "i64" | "string") {
                bail!(
                    "incremental projection `{}` cursor type must be i64 or string",
                    projection.id
                );
            }
            validate_cursor_value(&cursor.initial_value, &cursor.value_type)?;
            if projection.reconcile_deletions && cursor.deleted_field.is_empty() {
                bail!(
                    "incremental projection `{}` must declare deleted_field when deletion reconciliation is enabled",
                    projection.id
                );
            }
            if !cursor.deleted_field.is_empty() {
                validate_json_path(&cursor.deleted_field)?;
            }
        }
    }
    let mut operations = HashSet::new();
    for operation in &source.write_operations {
        validate_identifier(&operation.id, "write operation id")?;
        if !operations.insert(operation.id.as_str()) {
            bail!("duplicate write operation id `{}`", operation.id);
        }
        if operation.statements.is_empty() {
            bail!("write operation `{}` has no statements", operation.id);
        }
        if let Some(receipt) = &operation.source_receipt {
            if !operation.transaction {
                bail!(
                    "write operation `{}` source_receipt requires a transaction",
                    operation.id
                );
            }
            validate_read_sql(&receipt.lookup_query)?;
            validate_bindings(&receipt.lookup_parameters)?;
            validate_json_path(&receipt.payload_hash_field)?;
            validate_write_sql(&receipt.claim.sql)?;
            validate_bindings(&receipt.claim.parameters)?;
            if receipt.claim.returns_rows
                || receipt.claim.conflict_if_zero_rows
                || !receipt.claim.when_changed_field.is_empty()
            {
                bail!(
                    "write operation `{}` source receipt claim must be an unconditional non-returning statement",
                    operation.id
                );
            }
            let lookup_paths = receipt
                .lookup_parameters
                .iter()
                .map(|binding| binding.path.as_str())
                .collect::<HashSet<_>>();
            let claim_paths = receipt
                .claim
                .parameters
                .iter()
                .map(|binding| binding.path.as_str())
                .collect::<HashSet<_>>();
            if !lookup_paths.contains("ctox_write.command_id") {
                bail!(
                    "write operation `{}` source receipt lookup must bind `ctox_write.command_id`",
                    operation.id
                );
            }
            for required in ["ctox_write.command_id", "ctox_write.payload_hash"] {
                if !claim_paths.contains(required) {
                    bail!(
                        "write operation `{}` source receipt claim must bind `{required}`",
                        operation.id
                    );
                }
            }
        }
        for statement in &operation.statements {
            validate_write_sql(&statement.sql)?;
            validate_bindings(&statement.parameters)?;
            validate_optional_identifier(
                &statement.when_changed_field,
                "statement when_changed_field",
            )?;
        }
        for refresh in &operation.refresh {
            validate_collection(&refresh.collection)?;
            validate_json_path(&refresh.record_id_field)?;
            validate_bindings(&refresh.parameters)?;
            let projection = if refresh.projection_id.is_empty() {
                None
            } else {
                Some(
                    source
                        .projections
                        .iter()
                        .find(|projection| projection.id == refresh.projection_id)
                        .with_context(|| {
                            format!(
                                "refresh references unknown projection `{}`",
                                refresh.projection_id
                            )
                        })?,
                )
            };
            if let Some(projection) = projection {
                if projection.collection != refresh.collection {
                    bail!(
                        "refresh projection `{}` targets collection `{}`, not `{}`",
                        refresh.projection_id,
                        projection.collection,
                        refresh.collection
                    );
                }
            }
            let query = if refresh.query.is_empty() {
                projection
                    .map(|projection| projection.readback_query.as_str())
                    .filter(|query| !query.is_empty())
                    .context("refresh requires query or projection readback_query")?
            } else {
                refresh.query.as_str()
            };
            validate_read_sql(query)?;
        }
    }
    Ok(())
}

fn validate_bindings(bindings: &[ParameterBinding]) -> anyhow::Result<()> {
    for binding in bindings {
        validate_json_path(&binding.path)?;
        if !matches!(
            binding.value_type.as_str(),
            "string" | "bytes_base64" | "boolean" | "i16" | "i32" | "i64" | "f32" | "f64"
        ) {
            bail!("unsupported SQL parameter type `{}`", binding.value_type);
        }
    }
    Ok(())
}

fn bind_parameters(
    payload: &Value,
    bindings: &[ParameterBinding],
) -> anyhow::Result<Vec<SqlParameter>> {
    bindings
        .iter()
        .map(|binding| {
            let value = value_at_path(payload, &binding.path);
            if value.is_none() || value == Some(&Value::Null) {
                if binding.required {
                    bail!("required SQL parameter `{}` is missing", binding.path);
                }
                return Ok(SqlParameter::NullString);
            }
            sql_parameter_from_value(value.expect("checked value"), &binding.value_type)
        })
        .collect()
}

fn sql_parameter_from_value(value: &Value, value_type: &str) -> anyhow::Result<SqlParameter> {
    match value_type {
        "string" => Ok(SqlParameter::String(scalar_string(value)?)),
        "bytes_base64" => Ok(SqlParameter::Bytes(
            base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                value
                    .as_str()
                    .context("base64 parameter must be a string")?,
            )
            .context("invalid base64 SQL parameter")?,
        )),
        "boolean" => Ok(SqlParameter::Boolean(
            value.as_bool().context("boolean SQL parameter expected")?,
        )),
        "i16" => Ok(SqlParameter::I16(
            i16::try_from(value.as_i64().context("i16 SQL parameter expected")?)
                .context("i16 SQL parameter out of range")?,
        )),
        "i32" => Ok(SqlParameter::I32(
            i32::try_from(value.as_i64().context("i32 SQL parameter expected")?)
                .context("i32 SQL parameter out of range")?,
        )),
        "i64" => Ok(SqlParameter::I64(
            value.as_i64().context("i64 SQL parameter expected")?,
        )),
        "f32" => Ok(SqlParameter::F32(
            value.as_f64().context("f32 SQL parameter expected")? as f32,
        )),
        "f64" => Ok(SqlParameter::F64(
            value.as_f64().context("f64 SQL parameter expected")?,
        )),
        other => bail!("unsupported SQL parameter type `{other}`"),
    }
}

fn transform_projection_row(
    source_row: Value,
    projection: &ProjectionConfig,
) -> anyhow::Result<Value> {
    if projection.field_map.is_empty()
        && projection.search_fields.is_empty()
        && projection.source_payload_path.is_empty()
    {
        return Ok(source_row);
    }
    let mut target = Map::new();
    for (target_field, source_path) in &projection.field_map {
        validate_json_path(target_field)?;
        validate_json_path(source_path)?;
        if let Some(value) = value_at_path(&source_row, source_path) {
            target.insert(target_field.clone(), value.clone());
        }
    }
    for field in &projection.boolean_fields {
        if let Some(value) = target.get_mut(field) {
            *value = Value::Bool(value_as_bool(value));
        }
    }
    for field in &projection.number_fields {
        if let Some(value) = target.get_mut(field) {
            if let Some(number) = value_as_number(value) {
                *value = number;
            }
        }
    }
    if !projection.search_fields.is_empty() {
        let search_text = projection
            .search_fields
            .iter()
            .filter_map(|field| target.get(field))
            .filter_map(|value| match value {
                Value::String(value) => Some(value.as_str()),
                _ => None,
            })
            .filter(|value| !value.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        target.insert("search_text".to_string(), Value::String(search_text));
    }
    if !projection.source_payload_path.is_empty() {
        insert_value_at_path(&mut target, &projection.source_payload_path, source_row)?;
    }
    Ok(Value::Object(target))
}

fn insert_value_at_path(
    target: &mut Map<String, Value>,
    path: &str,
    value: Value,
) -> anyhow::Result<()> {
    validate_json_path(path)?;
    let segments = path.split('.').collect::<Vec<_>>();
    let mut current = target;
    for segment in &segments[..segments.len().saturating_sub(1)] {
        let entry = current
            .entry((*segment).to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        current = entry
            .as_object_mut()
            .context("source_payload_path overlaps a non-object field")?;
    }
    current.insert(
        segments
            .last()
            .context("source_payload_path is empty")?
            .to_string(),
        value,
    );
    Ok(())
}

fn value_as_bool(value: &Value) -> bool {
    value
        .as_bool()
        .or_else(|| value.as_i64().map(|value| value != 0))
        .or_else(|| {
            value.as_str().map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "ja"
                )
            })
        })
        .unwrap_or(false)
}

fn value_as_number(value: &Value) -> Option<Value> {
    if value.is_number() {
        return Some(value.clone());
    }
    value
        .as_str()
        .and_then(|value| value.trim().parse::<f64>().ok())
        .and_then(serde_json::Number::from_f64)
        .map(Value::Number)
}

fn payload_changed_field(payload: &Value, expected: &str) -> bool {
    payload
        .get("changed_fields")
        .and_then(Value::as_array)
        .is_some_and(|fields| fields.iter().any(|field| field.as_str() == Some(expected)))
}

fn source_is_due(root: &Path, source: &ExternalSqlSourceConfig, now: i64) -> anyhow::Result<bool> {
    let status_id = status_record_id(source);
    let Some(status) = load_status(root, &source.status_collection, &status_id)? else {
        return Ok(true);
    };
    let last_attempt = status
        .get("finished_at_ms")
        .and_then(Value::as_i64)
        .or_else(|| status.get("started_at_ms").and_then(Value::as_i64))
        .unwrap_or(0);
    Ok(now.saturating_sub(last_attempt) >= (source.sync_interval_seconds as i64 * 1_000))
}

fn project_status(
    root: &Path,
    source: &ExternalSqlSourceConfig,
    run_id: &str,
    status: RunStatus,
    started_at_ms: i64,
    finished_at_ms: Option<i64>,
    stats: &BTreeMap<String, ProjectionStats>,
    error: Option<&str>,
) -> anyhow::Result<()> {
    let record_id = status_record_id(source);
    let previous = load_status(root, &source.status_collection, &record_id)?;
    let updated_at_ms = finished_at_ms.unwrap_or_else(now_ms);
    let successful = status == RunStatus::Ready;
    let sync_mode = if source
        .projections
        .iter()
        .any(|projection| projection.incremental_cursor.is_some())
    {
        "incremental_cursor_with_explicit_deletes"
    } else {
        "background_snapshot_reconciliation"
    };
    let last_success_at_ms = if successful {
        updated_at_ms
    } else {
        previous
            .as_ref()
            .and_then(|value| value.get("last_success_at_ms"))
            .and_then(Value::as_i64)
            .unwrap_or(0)
    };
    let last_error = if successful {
        ""
    } else {
        error.unwrap_or_else(|| {
            previous
                .as_ref()
                .and_then(|value| value.get("last_error"))
                .and_then(Value::as_str)
                .unwrap_or_default()
        })
    };
    let payload = json!({
        "id": record_id,
        "module_id": source.module_id,
        "source_id": source.id,
        "display_name": if source.display_name.is_empty() { &source.id } else { &source.display_name },
        "source_kind": source.kind,
        "status": status.as_str(),
        "run_id": run_id,
        "source_of_truth": true,
        "app_reads_from": "sqlite_rxdb_projection",
        "app_writes_via": "external_sql.write",
        "sync_mode": sync_mode,
        "projections": stats_json(stats),
        "complete": successful && stats.values().all(|stats| stats.complete),
        "truncated": stats.values().any(|stats| stats.truncated),
        "last_success_at_ms": last_success_at_ms,
        "last_error": last_error,
        "started_at_ms": started_at_ms,
        "finished_at_ms": finished_at_ms.unwrap_or(0),
        "updated_at_ms": updated_at_ms,
        "is_deleted": false,
    });
    store::upsert_projection_record(
        root,
        &source.status_collection,
        &record_id,
        updated_at_ms,
        payload,
    )
}

fn stats_json(stats: &BTreeMap<String, ProjectionStats>) -> Value {
    Value::Object(stats.iter().map(|(id, stats)| (id.clone(), json!({
        "source_count": stats.source_count,
        "projected_count": stats.inserted_count + stats.updated_count + stats.unchanged_count,
        "inserted_count": stats.inserted_count,
        "updated_count": stats.updated_count,
        "unchanged_count": stats.unchanged_count,
        "deleted_count": stats.deleted_count,
        "complete": stats.complete,
        "truncated": stats.truncated,
        "cursor": {
            "next_offset": stats.next_offset,
            "value": stats.cursor.as_ref(),
            "resumable": stats.resumable,
        },
    }))).collect::<Map<String, Value>>())
}

fn existing_projection_fingerprints(
    root: &Path,
    source: &ExternalSqlSourceConfig,
    projection: &ProjectionConfig,
) -> anyhow::Result<BTreeMap<String, String>> {
    let conn = store::open_store(root)?;
    let mut statement = conn.prepare(
        "SELECT record_id, payload_json FROM business_records WHERE collection=?1 AND deleted=0",
    )?;
    let rows = statement.query_map([&projection.collection], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut values = BTreeMap::new();
    for row in rows {
        let (id, raw) = row?;
        let payload: Value = serde_json::from_str(&raw)?;
        if payload.get("is_deleted").and_then(Value::as_bool) != Some(true)
            && projection_belongs_to(&payload, source, projection)
        {
            values.insert(id, payload_fingerprint(&payload));
        }
    }
    Ok(values)
}

fn projection_provenance(source: &ExternalSqlSourceConfig, projection: &ProjectionConfig) -> Value {
    json!({
        "module_id": source.module_id,
        "source_id": source.id,
        "projection_id": projection.id,
    })
}

fn projection_belongs_to(
    payload: &Value,
    source: &ExternalSqlSourceConfig,
    projection: &ProjectionConfig,
) -> bool {
    let Some(provenance) = payload.get("_ctox_external_sql") else {
        return false;
    };
    provenance.get("module_id").and_then(Value::as_str) == Some(source.module_id.as_str())
        && provenance.get("source_id").and_then(Value::as_str) == Some(source.id.as_str())
        && provenance.get("projection_id").and_then(Value::as_str) == Some(projection.id.as_str())
}

fn ensure_projection_cursor_schema(root: &Path) -> anyhow::Result<()> {
    store::open_store(root)?.execute_batch(
        "CREATE TABLE IF NOT EXISTS external_sql_projection_cursors (
            module_id TEXT NOT NULL,
            source_id TEXT NOT NULL,
            projection_id TEXT NOT NULL,
            cursor_json TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY(module_id, source_id, projection_id)
        );",
    )?;
    Ok(())
}

fn load_projection_cursor(
    root: &Path,
    source: &ExternalSqlSourceConfig,
    projection: &ProjectionConfig,
    config: &IncrementalCursorConfig,
) -> anyhow::Result<Value> {
    ensure_projection_cursor_schema(root)?;
    let conn = store::open_store(root)?;
    let raw = conn
        .query_row(
            "SELECT cursor_json FROM external_sql_projection_cursors
             WHERE module_id=?1 AND source_id=?2 AND projection_id=?3",
            params![source.module_id, source.id, projection.id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let value = raw
        .map(|raw| serde_json::from_str(&raw).context("invalid external SQL cursor"))
        .transpose()?
        .unwrap_or_else(|| config.initial_value.clone());
    validate_cursor_value(&value, &config.value_type)?;
    Ok(value)
}

fn store_projection_cursor(
    root: &Path,
    source: &ExternalSqlSourceConfig,
    projection: &ProjectionConfig,
    cursor: &Value,
) -> anyhow::Result<()> {
    ensure_projection_cursor_schema(root)?;
    let conn = store::open_store(root)?;
    conn.execute(
        "INSERT INTO external_sql_projection_cursors
            (module_id,source_id,projection_id,cursor_json,updated_at_ms)
         VALUES (?1,?2,?3,?4,?5)
         ON CONFLICT(module_id,source_id,projection_id) DO UPDATE SET
            cursor_json=excluded.cursor_json,updated_at_ms=excluded.updated_at_ms",
        params![
            source.module_id,
            source.id,
            projection.id,
            serde_json::to_string(cursor)?,
            now_ms()
        ],
    )?;
    Ok(())
}

fn validate_cursor_value(value: &Value, value_type: &str) -> anyhow::Result<()> {
    match value_type {
        "i64" if value.as_i64().is_some() => Ok(()),
        "string" if value.as_str().is_some() => Ok(()),
        "i64" | "string" => bail!("external SQL cursor value does not match type `{value_type}`"),
        _ => bail!("unsupported external SQL cursor type `{value_type}`"),
    }
}

fn cursor_is_strictly_after(next: &Value, current: &Value, value_type: &str) -> bool {
    match value_type {
        "i64" => next
            .as_i64()
            .zip(current.as_i64())
            .is_some_and(|(next, current)| next > current),
        "string" => next
            .as_str()
            .zip(current.as_str())
            .is_some_and(|(next, current)| next > current),
        _ => false,
    }
}

fn ensure_writeback_schema(root: &Path) -> anyhow::Result<()> {
    let conn = store::open_store(root)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS external_sql_writeback_receipts (
            command_id TEXT PRIMARY KEY,
            module_id TEXT NOT NULL,
            source_id TEXT NOT NULL,
            operation_id TEXT NOT NULL,
            payload_hash TEXT NOT NULL DEFAULT '',
            stage TEXT NOT NULL,
            result_json TEXT NOT NULL DEFAULT '{}',
            error_text TEXT NOT NULL DEFAULT '',
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_external_sql_writeback_stage
          ON external_sql_writeback_receipts(stage, updated_at_ms);",
    )?;
    let has_payload_hash = conn
        .prepare("PRAGMA table_info(external_sql_writeback_receipts)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|name| name == "payload_hash");
    if !has_payload_hash {
        conn.execute(
            "ALTER TABLE external_sql_writeback_receipts ADD COLUMN payload_hash TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }
    Ok(())
}

fn load_writeback_receipt(
    root: &Path,
    command_id: &str,
) -> anyhow::Result<Option<WritebackReceipt>> {
    let conn = store::open_store(root)?;
    conn.query_row(
        "SELECT module_id,source_id,operation_id,payload_hash,stage,result_json,error_text
         FROM external_sql_writeback_receipts WHERE command_id=?1",
        params![command_id],
        |row| {
            let result_json = row.get::<_, String>(5)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                result_json,
                row.get::<_, String>(6)?,
            ))
        },
    )
    .optional()?
    .map(
        |(module_id, source_id, operation_id, payload_hash, stage, result_json, error_text)| {
            Ok(WritebackReceipt {
                module_id,
                source_id,
                operation_id,
                payload_hash,
                stage,
                result: serde_json::from_str(&result_json)
                    .context("invalid writeback receipt result")?,
                error_text,
            })
        },
    )
    .transpose()
}

fn validate_receipt_identity(
    receipt: &WritebackReceipt,
    source: &ExternalSqlSourceConfig,
    operation_id: &str,
    payload_hash: &str,
) -> anyhow::Result<()> {
    if receipt.module_id != source.module_id
        || receipt.source_id != source.id
        || receipt.operation_id != operation_id
        || (!receipt.payload_hash.is_empty() && receipt.payload_hash != payload_hash)
    {
        bail!("external SQL idempotency conflict: command id was reused with different intent");
    }
    Ok(())
}

fn write_command_fingerprint(
    source: &ExternalSqlSourceConfig,
    operation_id: &str,
    payload: &Value,
) -> String {
    let intent = json!({
        "module_id": source.module_id,
        "source_id": source.id,
        "operation_id": operation_id,
        "payload": payload,
    });
    format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(&intent).unwrap_or_default())
    )
}

fn record_writeback_stage(
    root: &Path,
    command_id: &str,
    source: &ExternalSqlSourceConfig,
    operation_id: &str,
    payload_hash: &str,
    stage: &str,
    result: Value,
    error: Option<&str>,
) -> anyhow::Result<()> {
    let conn = store::open_store(root)?;
    let now = now_ms();
    conn.execute(
        "INSERT INTO external_sql_writeback_receipts
            (command_id,module_id,source_id,operation_id,payload_hash,stage,result_json,error_text,created_at_ms,updated_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)
         ON CONFLICT(command_id) DO UPDATE SET
            payload_hash=CASE
                WHEN external_sql_writeback_receipts.payload_hash='' THEN excluded.payload_hash
                ELSE external_sql_writeback_receipts.payload_hash
            END,
            stage=excluded.stage,result_json=excluded.result_json,error_text=excluded.error_text,updated_at_ms=excluded.updated_at_ms
         WHERE external_sql_writeback_receipts.module_id=excluded.module_id
           AND external_sql_writeback_receipts.source_id=excluded.source_id
           AND external_sql_writeback_receipts.operation_id=excluded.operation_id
           AND (external_sql_writeback_receipts.payload_hash='' OR external_sql_writeback_receipts.payload_hash=excluded.payload_hash)",
        params![command_id, source.module_id, source.id, operation_id, payload_hash, stage, serde_json::to_string(&result)?, error.unwrap_or_default(), now],
    )?;
    let receipt = load_writeback_receipt(root, command_id)?
        .context("external SQL writeback receipt was not persisted")?;
    validate_receipt_identity(&receipt, source, operation_id, payload_hash)?;
    Ok(())
}

fn load_status(root: &Path, collection: &str, record_id: &str) -> anyhow::Result<Option<Value>> {
    let conn = store::open_store(root)?;
    let raw = conn
        .query_row(
            "SELECT payload_json FROM business_records WHERE collection=?1 AND record_id=?2",
            params![collection, record_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    raw.map(|raw| serde_json::from_str(&raw).map_err(Into::into))
        .transpose()
}

fn status_record_id(source: &ExternalSqlSourceConfig) -> String {
    if source.status_record_id.trim().is_empty() {
        format!("external-sql-{}", source.id)
    } else {
        source.status_record_id.clone()
    }
}

fn validate_read_sql(sql: &str) -> anyhow::Result<()> {
    validate_read_statement(sql)
}

fn validate_write_sql(sql: &str) -> anyhow::Result<()> {
    validate_write_statement(sql)
}

fn validate_collection(value: &str) -> anyhow::Result<()> {
    if value.is_empty()
        || value.len() > 160
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        bail!("invalid collection `{value}`");
    }
    Ok(())
}

fn validate_identifier(value: &str, label: &str) -> anyhow::Result<()> {
    if value.is_empty()
        || value.len() > 160
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
    {
        bail!("invalid {label} `{value}`");
    }
    Ok(())
}

fn validate_optional_identifier(value: &str, label: &str) -> anyhow::Result<()> {
    if value.is_empty() {
        Ok(())
    } else {
        validate_identifier(value, label)
    }
}

fn validate_json_path(path: &str) -> anyhow::Result<()> {
    if path.is_empty()
        || !path.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
    {
        bail!("invalid JSON field path `{path}`");
    }
    Ok(())
}

fn value_at_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .try_fold(value, |current, segment| current.get(segment))
}

fn required_string(value: &Value, path: &str) -> anyhow::Result<String> {
    value_at_path(value, path)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .with_context(|| format!("{path} is required"))
}

fn scalar_string(value: &Value) -> anyhow::Result<String> {
    match value {
        Value::String(value) => Ok(value.clone()),
        Value::Number(value) => Ok(value.to_string()),
        Value::Bool(value) => Ok(value.to_string()),
        _ => bail!("expected scalar SQL value"),
    }
}

fn payload_fingerprint(value: &Value) -> String {
    let mut normalized = value.clone();
    if let Some(object) = normalized.as_object_mut() {
        object.remove("_deleted");
        object.remove("_rev");
        object.remove("updated_at_ms");
        object.remove("deleted_at_ms");
    }
    let normalized = canonical_json_value(normalized);
    format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(&normalized).unwrap_or_default())
    )
}

fn canonical_json_value(value: Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut fields = object.into_iter().collect::<Vec<_>>();
            fields.sort_by(|left, right| left.0.cmp(&right.0));
            Value::Object(
                fields
                    .into_iter()
                    .map(|(key, value)| (key, canonical_json_value(value)))
                    .collect(),
            )
        }
        Value::Array(values) => {
            Value::Array(values.into_iter().map(canonical_json_value).collect())
        }
        scalar => scalar,
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn default_true() -> bool {
    true
}
fn default_source_kind() -> String {
    SQLSERVER_KIND.to_owned()
}
fn default_sqlserver_port() -> u16 {
    1433
}
fn default_sync_interval_seconds() -> u64 {
    DEFAULT_SYNC_INTERVAL_SECONDS
}
fn default_page_size() -> usize {
    DEFAULT_PAGE_SIZE
}
fn default_request_timeout_ms() -> u64 {
    30_000
}
fn default_connector_max_rows() -> usize {
    MAX_PAGE_SIZE
}
fn default_source_version_field() -> String {
    "source_version".to_owned()
}
fn default_expected_source_version_path() -> String {
    "expected_source_version".to_owned()
}
fn default_payload_hash_field() -> String {
    "payload_hash".to_owned()
}

struct SourceLease {
    key: String,
}

impl SourceLease {
    fn acquire(key: String) -> anyhow::Result<Self> {
        let mut active = ACTIVE_SOURCES
            .get_or_init(|| Mutex::new(HashSet::new()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !active.insert(key.clone()) {
            bail!("external SQL source `{key}` is already syncing");
        }
        Ok(Self { key })
    }
}

impl Drop for SourceLease {
    fn drop(&mut self) {
        ACTIVE_SOURCES
            .get_or_init(|| Mutex::new(HashSet::new()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> ExternalSqlSourceConfig {
        serde_json::from_value(json!({
            "module_id":"inventory",
            "id":"erp-primary",
            "connection":{"server":"sql.example.test","database":"erp","user":"sync","password_secret":"ERP_SQL_PASSWORD","allow_writes":true},
            "sync_interval_seconds":300,
            "status_collection":"inventory_sync_status",
            "projections":[{"id":"items","collection":"inventory_items","record_id_field":"item_id","query":"SELECT item_id,name FROM dbo.items ORDER BY item_id OFFSET @P1 ROWS FETCH NEXT @P2 ROWS ONLY"}],
            "write_operations":[{"id":"item_update","statements":[{"sql":"UPDATE dbo.items SET name=@P1 WHERE item_id=@P2","parameters":[{"path":"values.name","type":"string"},{"path":"item_id","type":"i64"}]}]}]
        })).expect("source config")
    }

    #[test]
    fn generic_source_contract_validates() {
        let source = source();
        validate_source(&source).expect("valid source");
        assert_eq!(source.module_id, "inventory");
        assert_eq!(source.projections[0].collection, "inventory_items");
    }

    #[test]
    fn sql_interpolation_and_administrative_writes_are_rejected() {
        assert!(validate_read_sql("SELECT * FROM dbo.items WHERE id={{id}}").is_err());
        assert!(validate_write_sql("DROP TABLE dbo.items").is_err());
        assert!(validate_write_sql("UPDATE dbo.items SET name=@P1 WHERE id=@P2").is_ok());
    }

    #[test]
    fn parameter_bindings_are_typed_and_fail_closed() {
        let bindings = vec![
            ParameterBinding {
                path: "values.name".into(),
                value_type: "string".into(),
                required: true,
            },
            ParameterBinding {
                path: "item_id".into(),
                value_type: "i64".into(),
                required: true,
            },
        ];
        let values = bind_parameters(&json!({"item_id":42,"values":{"name":"New"}}), &bindings)
            .expect("bindings");
        assert_eq!(
            values,
            vec![SqlParameter::String("New".into()), SqlParameter::I64(42)]
        );
        assert!(bind_parameters(&json!({"item_id":42}), &bindings).is_err());
    }

    #[test]
    fn writeback_receipt_advances_durably() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let source = source();
        ensure_writeback_schema(root.path())?;
        record_writeback_stage(
            root.path(),
            "cmd-1",
            &source,
            "item_update",
            "sha256:test",
            "pending_source",
            Value::Null,
            None,
        )?;
        record_writeback_stage(
            root.path(),
            "cmd-1",
            &source,
            "item_update",
            "sha256:test",
            "completed",
            json!({"affected_rows":1}),
            None,
        )?;
        let conn = store::open_store(root.path())?;
        let stage: String = conn.query_row(
            "SELECT stage FROM external_sql_writeback_receipts WHERE command_id='cmd-1'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(stage, "completed");
        Ok(())
    }

    #[test]
    fn projection_readback_contract_validates_and_mismatches_fail_closed() {
        let mut source = source();
        source.projections[0].readback_query =
            "SELECT item_id,name FROM dbo.items WHERE item_id=@P1".into();
        source.write_operations[0].refresh = vec![RefreshQueryConfig {
            projection_id: "items".into(),
            collection: "inventory_items".into(),
            record_id_field: "item_id".into(),
            query: String::new(),
            parameters: vec![ParameterBinding {
                path: "item_id".into(),
                value_type: "i64".into(),
                required: true,
            }],
            record_id_prefix: "inventory-item-".into(),
        }];
        validate_source(&source).expect("projection readback source");

        source.write_operations[0].refresh[0].collection = "inventory_other".into();
        assert!(validate_source(&source).is_err());
    }

    #[test]
    fn writeback_receipt_rejects_reused_command_intent() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let source = source();
        ensure_writeback_schema(root.path())?;
        let payload_hash = write_command_fingerprint(
            &source,
            "item_update",
            &json!({"item_id":42,"values":{"name":"First"}}),
        );
        record_writeback_stage(
            root.path(),
            "cmd-idempotent",
            &source,
            "item_update",
            &payload_hash,
            "completed",
            json!({"affected_rows":1}),
            None,
        )?;
        let receipt =
            load_writeback_receipt(root.path(), "cmd-idempotent")?.context("receipt exists")?;
        validate_receipt_identity(&receipt, &source, "item_update", &payload_hash)?;
        assert!(
            validate_receipt_identity(&receipt, &source, "item_update", "sha256:different")
                .is_err()
        );
        assert_eq!(receipt.result, json!({"affected_rows":1}));
        Ok(())
    }

    #[test]
    fn source_receipt_contract_is_transactional_and_binds_stable_intent() -> anyhow::Result<()> {
        let mut source = source();
        source.write_operations[0].source_receipt = Some(
            serde_json::from_value(json!({
                "lookup_query":"SELECT payload_hash FROM dbo.ctox_write_receipts WHERE command_id=@P1",
                "lookup_parameters":[
                    {"path":"ctox_write.command_id","type":"string"}
                ],
                "claim":{
                    "sql":"INSERT INTO dbo.ctox_write_receipts(command_id,payload_hash) VALUES(@P1,@P2)",
                    "parameters":[
                        {"path":"ctox_write.command_id","type":"string"},
                        {"path":"ctox_write.payload_hash","type":"string"}
                    ]
                }
            }))
            .expect("source receipt"),
        );
        validate_source(&source).expect("transactional receipt contract");

        let execution = write_execution_payload(
            &json!({"item_id":42,"values":{"name":"New"}}),
            "cmd-42",
            "sha256:intent",
        )
        .expect("execution payload");
        let bindings = bind_parameters(
            &execution,
            &source.write_operations[0]
                .source_receipt
                .as_ref()
                .expect("receipt")
                .claim
                .parameters,
        )
        .expect("receipt bindings");
        assert_eq!(
            bindings,
            vec![
                SqlParameter::String("cmd-42".into()),
                SqlParameter::String("sha256:intent".into())
            ]
        );
        let receipt = source.write_operations[0]
            .source_receipt
            .as_ref()
            .expect("receipt");
        assert!(validate_source_receipt_rows(
            vec![json!({"payload_hash":"sha256:other"})],
            receipt,
            "sha256:intent",
        )
        .is_err());
        assert!(validate_source_receipt_rows(
            vec![json!({"payload_hash":"sha256:intent"})],
            receipt,
            "sha256:intent",
        )?
        .is_some());

        source.write_operations[0].transaction = false;
        assert!(validate_source(&source).is_err());
        Ok(())
    }

    #[test]
    fn writeback_conflict_is_visible_and_identity_cannot_be_overwritten() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let source = source();
        ensure_writeback_schema(root.path())?;
        record_writeback_stage(
            root.path(),
            "cmd-conflict",
            &source,
            "item_update",
            "sha256:original",
            "conflict",
            Value::Null,
            Some("source version changed"),
        )?;
        let receipt = load_writeback_receipt(root.path(), "cmd-conflict")?.context("receipt")?;
        assert_eq!(receipt.stage, "conflict");
        assert_eq!(receipt.error_text, "source version changed");

        assert!(record_writeback_stage(
            root.path(),
            "cmd-conflict",
            &source,
            "item_update",
            "sha256:different",
            "completed",
            json!({"affected_rows":1}),
            None,
        )
        .is_err());
        let receipt = load_writeback_receipt(root.path(), "cmd-conflict")?.context("receipt")?;
        assert_eq!(receipt.payload_hash, "sha256:original");
        assert_eq!(receipt.stage, "conflict");
        Ok(())
    }

    #[test]
    fn incremental_cursor_is_durable_and_strictly_monotonic() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let mut source = source();
        source.projections[0].incremental_cursor = Some(IncrementalCursorConfig {
            cursor_field: "change_id".into(),
            value_type: "i64".into(),
            initial_value: json!(0),
            deleted_field: "is_deleted".into(),
        });
        validate_source(&source)?;
        let projection = &source.projections[0];
        let config = projection.incremental_cursor.as_ref().context("cursor")?;
        assert_eq!(
            load_projection_cursor(root.path(), &source, projection, config)?,
            json!(0)
        );
        store_projection_cursor(root.path(), &source, projection, &json!(41))?;
        assert_eq!(
            load_projection_cursor(root.path(), &source, projection, config)?,
            json!(41)
        );
        assert!(cursor_is_strictly_after(&json!(42), &json!(41), "i64"));
        assert!(!cursor_is_strictly_after(&json!(41), &json!(41), "i64"));
        assert!(!cursor_is_strictly_after(&json!(40), &json!(41), "i64"));
        Ok(())
    }

    #[test]
    fn snapshot_reconciliation_only_owns_matching_projection_records() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let source = source();
        let projection = &source.projections[0];
        for (id, provenance) in [
            ("owned", Some(projection_provenance(&source, projection))),
            (
                "other-source",
                Some(json!({
                    "module_id": source.module_id,
                    "source_id": "another-source",
                    "projection_id": projection.id,
                })),
            ),
            ("legacy-unscoped", None),
        ] {
            let mut payload = json!({"id":id,"name":id,"is_deleted":false});
            if let Some(provenance) = provenance {
                payload
                    .as_object_mut()
                    .context("payload")?
                    .insert("_ctox_external_sql".into(), provenance);
            }
            store::upsert_projection_record(root.path(), &projection.collection, id, 1, payload)?;
        }

        let existing = existing_projection_fingerprints(root.path(), &source, projection)?;
        assert_eq!(existing.keys().cloned().collect::<Vec<_>>(), vec!["owned"]);
        Ok(())
    }

    #[test]
    fn projection_fingerprint_is_independent_of_json_field_order() {
        let mut left = Map::new();
        left.insert("name".into(), json!("Example"));
        left.insert("address".into(), json!({"city":"Berlin","zip":"10115"}));

        let mut nested = Map::new();
        nested.insert("zip".into(), json!("10115"));
        nested.insert("city".into(), json!("Berlin"));
        let mut right = Map::new();
        right.insert("address".into(), Value::Object(nested));
        right.insert("name".into(), json!("Example"));

        assert_eq!(
            payload_fingerprint(&Value::Object(left)),
            payload_fingerprint(&Value::Object(right))
        );
    }

    #[test]
    fn projection_fingerprint_ignores_business_os_runtime_metadata() {
        let source = json!({
            "id": "company-1",
            "name": "Example",
            "is_deleted": false,
            "updated_at_ms": 42,
        });
        let stored = json!({
            "_deleted": false,
            "_rev": "rev-runtime-only",
            "id": "company-1",
            "name": "Example",
            "is_deleted": false,
            "updated_at_ms": 99,
        });

        assert_eq!(payload_fingerprint(&source), payload_fingerprint(&stored));
    }

    #[test]
    fn invalid_local_mapping_fails_server_owned_collection_discovery() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let module_dir = root.path().join("business-os/local-modules/inventory");
        std::fs::create_dir_all(&module_dir)?;
        std::fs::write(
            module_dir.join("module.json"),
            serde_json::to_vec_pretty(&json!({
                "id": "inventory",
                "title": "Inventory",
                "external_data_sources_file": "external-sql.json"
            }))?,
        )?;
        std::fs::write(module_dir.join("external-sql.json"), b"{not-json")?;

        assert!(server_owned_projection_collections(root.path()).is_err());
        Ok(())
    }
}
