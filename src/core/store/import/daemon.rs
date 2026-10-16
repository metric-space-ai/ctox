//! Explicit frozen-input attachment to the foreground owner's existing writer.
//! The operator CLI only submits IPC; no offline writer fallback exists.
use super::*;
use crate::{
    ipc::{self, Dispatcher, JobKind, Request},
    JobHandle,
};
use cutover::{FileChoice, FrozenInput, Mode, SourceKind, Table};
use rusqlite::{Connection, OpenFlags};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

pub const METHOD: &str = "v4.cutover.import";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportRequest {
    pub snapshot: PathBuf,
    pub dry_run: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub schema: String,
    pub frozen: bool,
    pub method: String,
    pub imported_at_ms: u64,
    pub archive_custodian_user: String,
    pub release: ReleaseFiles,
    pub databases: Vec<Source>,
    pub inventory: BTreeMap<String, BTreeSet<String>>,
    #[serde(default)]
    pub retained: Vec<Retained>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Retained {
    pub database: String,
    pub table: String,
    pub decision: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseFiles {
    pub v4_root: PathBuf,
    pub platform_manifest: PathBuf,
    pub app_ver: String,
    pub client_limit: u64,
    pub byte_budget: u64,
    pub rights_ver: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub choice: FileChoice,
    /// Digest of the frozen table measurements, not raw protected DB bytes.
    pub measurement_sha256: String,
    pub kind: SourceKind,
    pub internal_table: Option<String>,
    pub tables: Vec<Decision>,
    #[serde(default)]
    pub collectors: Vec<super::daemon_collectors::Collector>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub rule: String,
    pub table: Table,
    pub measurement: Option<freeze::Measurement>,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct TableReport {
    pub database: String,
    pub table: String,
    pub decision: String,
    pub source_rows: Option<usize>,
    pub source_sha256: Option<String>,
    pub target_rows: Option<usize>,
    pub expected_target_rows: usize,
    pub retained_source: bool,
    pub retained_current_verified: bool,
}
#[derive(Serialize, Deserialize)]
pub struct Report {
    pub snapshot_sha256: String,
    pub release_sha256: String,
    pub inventory_complete: bool,
    pub content_complete: bool,
    pub dry_run: bool,
    pub tables: Vec<TableReport>,
    pub audit: cutover::Audit,
    pub import: Option<ImportReport>,
}
struct Prepared {
    snapshot: Snapshot,
    plan: base_release::ReleasePlan,
    hash: String,
    import: cutover::CutoverImport,
    tables: Vec<TableReport>,
    files: Vec<super::daemon_collectors::PendingFiles>,
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn within(root: &Path, path: &Path) -> Result<PathBuf> {
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(invalid("cutover_snapshot_symlink"));
    }
    let actual = path.canonicalize()?;
    if !actual.starts_with(root) {
        return Err(invalid("cutover_snapshot_path_escape"));
    }
    Ok(actual)
}
/// The host lends its reviewed native artifact migration implementation.
/// The independent store crate never invents identity migrations or executes JS.
pub type Migration = fn(&str, &Value, Value) -> Result<Value>;
struct NativeMigration(Migration);
impl base_release::MigrationEvaluator for NativeMigration {
    fn migrate(&self, collection: &str, spec: &Value, body: Value) -> Result<Value> {
        (self.0)(collection, spec, body)
    }
}
fn prepare(path: PathBuf, live: &[PathBuf], migration: Migration) -> Result<Prepared> {
    let path = if path.is_dir() {
        path.join("cutover.json")
    } else {
        path
    };
    let path = path.canonicalize()?;
    let root = path
        .parent()
        .ok_or_else(|| invalid("cutover_snapshot_root"))?;
    if fs::metadata(&path)?.len() > 16 * 1024 * 1024 {
        return Err(invalid("cutover_manifest_budget"));
    }
    let bytes = fs::read(&path)?;
    let snapshot: Snapshot =
        serde_json::from_slice(&bytes).map_err(|_| invalid("cutover_snapshot_contract"))?;
    if snapshot.schema != "ctox.v4.cutover_snapshot.v1"
        || !snapshot.frozen
        || snapshot.method != "sqlite_backup_api"
        || snapshot.databases.is_empty()
    {
        return Err(invalid("cutover_sqlite_backup_attestation_required"));
    }
    let mut declared: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for source in &snapshot.databases {
        if declared
            .insert(
                source.choice.database.clone(),
                source.tables.iter().map(|d| d.table.name.clone()).collect(),
            )
            .is_some()
        {
            return Err(invalid("cutover_duplicate_source_choice"));
        }
    }
    for kept in &snapshot.retained {
        if kept.decision != "secret_store_api_only"
            || !kept.database.to_lowercase().contains("secret")
            || snapshot
                .databases
                .iter()
                .any(|s| s.choice.database == kept.database)
            || !declared
                .entry(kept.database.clone())
                .or_default()
                .insert(kept.table.clone())
        {
            return Err(invalid("cutover_retention_decision_required"));
        }
    }
    if declared != snapshot.inventory {
        return Err(invalid("cutover_inventory_without_decision"));
    }
    let release_root = within(root, &snapshot.release.v4_root)?;
    let platform = within(root, &snapshot.release.platform_manifest)?;
    let plan = base_release::load(
        &release_root,
        &platform,
        &crate::schema::BaseReleaseDefaults {
            app_ver: snapshot.release.app_ver.clone(),
            client_limit: snapshot.release.client_limit,
            byte_budget: snapshot.release.byte_budget,
            rights_ver: snapshot.release.rights_ver,
        },
    )?;
    let mut databases = Vec::new();
    let mut identities = Vec::new();
    for source in &snapshot.databases {
        if source.choice.database.to_lowercase().contains("secret") {
            return Err(invalid("cutover_secrets_require_store_api"));
        }
        let file = within(root, &source.choice.frozen_file)?;
        // DbId also refuses hard links, including an alias of a held live file.
        let id = crate::DbId::new(&file)?;
        if live.contains(&file)
            || file
                == source
                    .choice
                    .live_file
                    .canonicalize()
                    .unwrap_or_else(|_| source.choice.live_file.clone())
            || identities.contains(&id)
        {
            return Err(invalid("cutover_live_or_duplicate_source_forbidden"));
        }
        identities.push(id);
        for suffix in ["-wal", "-journal"] {
            if fs::metadata(format!("{}{suffix}", file.display())).is_ok_and(|m| m.len() > 0) {
                return Err(invalid("cutover_snapshot_has_journal"));
            }
        }
        let encoded = file
            .to_str()
            .ok_or_else(|| invalid("cutover_snapshot_path_encoding"))?
            .bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.') {
                    (b as char).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect::<String>();
        let db = Connection::open_with_flags(
            format!("file:{encoded}?mode=ro&immutable=1"),
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_URI,
        )?;
        db.pragma_update(None, "query_only", true)?;
        databases.push(db);
    }
    let transactions: Vec<_> = databases
        .iter_mut()
        .map(Connection::transaction)
        .collect::<rusqlite::Result<_>>()?;
    let mut tables = Vec::new();
    let mut specs = Vec::new();
    for (source, tx) in snapshot.databases.iter().zip(&transactions) {
        let actual: BTreeSet<String> = tx
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND substr(name,1,7)<>'sqlite_'",
            )?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let expected: BTreeSet<String> =
            source.tables.iter().map(|d| d.table.name.clone()).collect();
        if expected.len() != source.tables.len() || actual != expected {
            return Err(invalid("cutover_table_without_decision"));
        }
        let measurements: BTreeMap<_, _> = source
            .tables
            .iter()
            .map(|d| (&d.table.name, &d.measurement))
            .collect();
        if hash(
            &serde_json::to_vec(&measurements)
                .map_err(|_| invalid("cutover_measurement_encoding"))?,
        ) != source.measurement_sha256
        {
            return Err(invalid("cutover_snapshot_hash_mismatch"));
        }
        let mut selected = Vec::new();
        for decision in &source.tables {
            if !matches!(
                decision.rule.as_str(),
                "E29-R1" | "E29-R2" | "E29-R3" | "E29-R4" | "E29-R5" | "E29-R6" | "E29-R7"
            ) {
                return Err(invalid("cutover_table_without_decision"));
            }
            let metadata = source.internal_table.as_ref() == Some(&decision.table.name);
            let retained = matches!(decision.table.mode, Mode::KeepCore);
            if retained && !(decision.rule == "E29-R6" || (metadata && decision.rule == "E29-R4")) {
                return Err(invalid("cutover_retention_decision_mismatch"));
            }
            if !retained && decision.rule == "E29-R6" {
                return Err(invalid("cutover_retention_decision_mismatch"));
            }
            let quoted = sql(crate::change_log::identifier(&decision.table.name))?;
            let columns: Vec<String> = tx
                .prepare(&format!("PRAGMA table_info({quoted})"))?
                .query_map([], |r| r.get(1))?
                .collect::<rusqlite::Result<_>>()?;
            let sensitive = columns.iter().any(|c| {
                [
                    "password",
                    "password_hash",
                    "dkim_private_key",
                    "access_token",
                    "refresh_token",
                    "secret_value",
                    "api_key",
                    "env_value",
                ]
                .contains(&c.as_str())
            });
            let (count, sha) = if sensitive {
                if !retained || decision.measurement.is_some() {
                    return Err(invalid("cutover_protected_content_keep_only"));
                }
                (
                    tx.query_row(&format!("SELECT count(*) FROM {quoted}"), [], |r| r.get(0))?,
                    None,
                )
            } else {
                let measured = freeze::measure(tx, &decision.table.name)?;
                let expected = decision
                    .measurement
                    .as_ref()
                    .ok_or_else(|| invalid("cutover_measurement_required"))?;
                if measured.row_count != expected.row_count
                    || measured.source_table_hash != expected.source_table_hash
                    || measured.schema_columns != expected.schema_columns
                {
                    return Err(invalid("cutover_measurement_mismatch"));
                }
                (measured.row_count, Some(measured.source_table_hash))
            };
            tables.push(TableReport {
                database: source.choice.database.clone(),
                table: decision.table.name.clone(),
                decision: decision.rule.clone(),
                source_rows: Some(count),
                source_sha256: sha,
                target_rows: None,
                expected_target_rows: 0,
                retained_source: retained,
                retained_current_verified: false,
            });
            if !metadata {
                selected.push(
                    serde_json::from_value(
                        serde_json::to_value(&decision.table)
                            .map_err(|_| invalid("cutover_table_contract"))?,
                    )
                    .map_err(|_| invalid("cutover_table_contract"))?,
                );
            }
        }
        specs.push(selected);
    }
    let inputs: Vec<_> = snapshot
        .databases
        .iter()
        .enumerate()
        .map(|(i, s)| FrozenInput {
            tx: &transactions[i],
            file: &s.choice,
            copied_from: &s.choice.live_file,
            kind: s.kind,
            internal_table: s.internal_table.as_deref(),
            tables: &specs[i],
        })
        .collect();
    let mut import = cutover::collect(
        &inputs,
        &plan,
        &NativeMigration(migration),
        &cutover::Budget {
            rows: 2_000_000,
            bytes: 256 * 1024 * 1024,
        },
        snapshot.imported_at_ms,
    )?;
    let mut files = Vec::new();
    for (index, source) in snapshot.databases.iter().enumerate() {
        super::daemon_collectors::collect(
            &transactions[index],
            source,
            &plan,
            &mut import,
            &mut files,
            root,
        )?;
    }
    for table in &mut tables {
        table.expected_target_rows = if table.retained_source {
            table.source_rows.unwrap_or(0)
        } else {
            import
                .audit
                .target_documents
                .get(&format!("{}:{}", table.database, table.table))
                .map_or(0, BTreeSet::len)
        };
    }
    for kept in &snapshot.retained {
        tables.push(TableReport {
            database: kept.database.clone(),
            table: kept.table.clone(),
            decision: kept.decision.clone(),
            source_rows: None,
            source_sha256: None,
            target_rows: None,
            expected_target_rows: 0,
            retained_source: true,
            retained_current_verified: false,
        });
    }
    drop(inputs);
    drop(transactions);
    drop(databases);
    Ok(Prepared {
        snapshot,
        plan,
        hash: hash(&bytes),
        import,
        tables,
        files,
    })
}
struct NoDataframe;
impl core_import::DataframeImporter for NoDataframe {
    fn import_timeseries(&mut self, _: &str, _: &[Value]) -> Result<usize> {
        Err(invalid("cutover_dataframe_owner_required"))
    }
}
pub async fn execute(
    writer: WriterHandle,
    readers: crate::read_pool::ReadPool,
    retention: BTreeMap<PathBuf, crate::read_pool::ReadPool>,
    live: Vec<PathBuf>,
    request: ImportRequest,
    migration: Migration,
    cas_root: PathBuf,
    scratch: PathBuf,
) -> Result<Report> {
    let prepared = tokio::task::spawn_blocking(move || prepare(request.snapshot, &live, migration))
        .await
        .map_err(|_| StoreError::Panicked)??;
    let mut report = Report {
        inventory_complete: true,
        content_complete: false,
        snapshot_sha256: prepared.hash,
        release_sha256: hash(
            &serde_json::to_vec(&prepared.plan).map_err(|_| invalid("cutover_release_encoding"))?,
        ),
        dry_run: request.dry_run,
        tables: prepared.tables,
        audit: prepared.import.audit,
        import: None,
    };
    if request.dry_run {
        return Ok(report);
    }
    let scratch_dir = scratch.clone();
    let cas = tokio::task::spawn_blocking(move || {
        fs::create_dir_all(scratch_dir)?;
        crate::cas::Cas::open(cas_root)
    })
    .await
    .map_err(|_| StoreError::Panicked)??;
    let imported = cutover::prepare_files(
        prepared.import.snapshot,
        &cas,
        &scratch,
        &prepared.snapshot.archive_custodian_user,
    )
    .await?;
    let mut imported = jour_fixe::prepare_files(imported, &cas, &scratch).await?;
    for files in prepared.files {
        let prepared_files = file_import::prepare(files.plan, &cas, &scratch).await?;
        for key in files.tables {
            let targets = report
                .audit
                .target_documents
                .entry(key.clone())
                .or_default();
            targets.extend(
                prepared_files
                    .documents
                    .iter()
                    .map(|d| (d.collection.clone(), d.id.clone())),
            );
            if let Some(table) = report
                .tables
                .iter_mut()
                .find(|t| format!("{}:{}", t.database, t.table) == key)
            {
                table.expected_target_rows = targets.len();
            }
        }
        imported = file_import::attach(imported, prepared_files)?;
    }
    base_release::publish(
        &writer,
        &prepared.plan,
        &NativeMigration(migration),
        prepared.snapshot.imported_at_ms as i64,
    )
    .await?;
    report.import = Some(
        core_import::apply(&writer, imported, &mut NoDataframe, &report.snapshot_sha256).await?,
    );
    for (key, targets) in &report.audit.target_documents {
        let mut actual = 0;
        for page in targets.iter().cloned().collect::<Vec<_>>().chunks(128) {
            let page = page.to_vec();
            actual += readers
                .read_job(crate::ReadJob::new(JobOptions::default(), move |tx| {
                    for (collection, id) in &page {
                        let table: String = tx.query_row(
                            "SELECT table_name FROM _collections WHERE name=?1 AND state='active'",
                            [collection],
                            |r| r.get(0),
                        )?;
                        let table = crate::change_log::identifier(&table)?;
                        if !tx.query_row(
                            &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id=?1)"),
                            [id],
                            |r| r.get::<_, bool>(0),
                        )? {
                            return Err(invalid("cutover_target_row_missing"));
                        }
                    }
                    Ok(page.len())
                }))
                .await?;
        }
        for table in &mut report.tables {
            if format!("{}:{}", table.database, table.table) == *key {
                table.target_rows = Some(actual);
            }
        }
    }
    for table in report
        .tables
        .iter_mut()
        .filter(|t| !t.retained_source && t.source_rows == Some(0) && t.expected_target_rows == 0)
    {
        table.target_rows = Some(0);
    }
    for table in report.tables.iter_mut().filter(|t| t.retained_source) {
        let Some(file) = report
            .audit
            .selected_files
            .iter()
            .find(|f| f.database == table.database)
        else {
            continue;
        };
        let live = file
            .live_file
            .canonicalize()
            .unwrap_or_else(|_| file.live_file.clone());
        let Some(pool) = retention.get(&live) else {
            continue;
        };
        let name = table.table.clone();
        let protected = table.source_sha256.is_none();
        let measured = pool
            .read_job(crate::ReadJob::new(JobOptions::default(), move |db| {
                if protected {
                    let name = crate::change_log::identifier(&name)?;
                    let count =
                        db.query_row(&format!("SELECT count(*) FROM {name}"), [], |r| r.get(0))?;
                    Ok((count, None))
                } else {
                    let m = freeze::measure(db, &name)?;
                    Ok((m.row_count, Some(m.source_table_hash)))
                }
            }))
            .await?;
        if Some(measured.0) != table.source_rows || measured.1 != table.source_sha256 {
            return Err(invalid("cutover_retained_core_changed"));
        }
        table.target_rows = Some(measured.0);
        table.expected_target_rows = measured.0;
        table.retained_current_verified = !protected;
    }
    report.content_complete = report
        .tables
        .iter()
        .all(|t| t.source_sha256.is_some() && t.target_rows.is_some());
    Ok(report)
}

/// Attached only to the retained foreground handles. Requests cannot provide a
/// target DB/root, and the CLI never creates an Attachment or a Registry owner.
pub struct Attachment {
    writer: WriterHandle,
    readers: crate::read_pool::ReadPool,
    live: Vec<PathBuf>,
    root: PathBuf,
    runtime: tokio::runtime::Handle,
    active: Arc<Mutex<Option<PathBuf>>>,
    migration: Migration,
    retention: BTreeMap<PathBuf, crate::read_pool::ReadPool>,
}
impl Attachment {
    pub fn new(
        writer: WriterHandle,
        readers: crate::read_pool::ReadPool,
        live: Vec<PathBuf>,
        root: PathBuf,
        runtime: tokio::runtime::Handle,
        migration: Migration,
        retention: BTreeMap<PathBuf, crate::read_pool::ReadPool>,
    ) -> Self {
        Self {
            writer,
            readers,
            live,
            root,
            runtime,
            active: Arc::new(Mutex::new(None)),
            migration,
            retention,
        }
    }
}
impl Dispatcher for Attachment {
    fn dispatch(&self, request: Request, options: JobOptions) -> Result<JobHandle<Vec<u8>>> {
        if request.method != METHOD {
            return Err(invalid("unknown cutover operation"));
        }
        let run: ImportRequest = serde_json::from_slice(&request.payload)
            .map_err(|_| invalid("invalid cutover request"))?;
        if request.kind
            != if run.dry_run {
                JobKind::Read
            } else {
                JobKind::Write
            }
        {
            return Err(invalid("cutover_job_kind"));
        }
        options.check()?;
        let (send, receive) = tokio::sync::oneshot::channel();
        let mut active = self.active.lock().unwrap_or_else(|p| p.into_inner());
        if active.is_some() {
            return Err(StoreError::Busy);
        }
        *active = Some(run.snapshot.clone());
        let migration = self.migration;
        let writer = self.writer.clone();
        let readers = self.readers.clone();
        let retention = self.retention.clone();
        let live = self.live.clone();
        let root = self.root.clone();
        let active = self.active.clone();
        let job_id = uuid::Uuid::new_v4().to_string();
        let report_path = root.join("cutover-import").join(format!("{job_id}.json"));
        let receipt =
            serde_json::json!({"state":"admitted","job_id":job_id,"report_path":report_path});
        self.runtime.spawn(async move {
            // A bounded IPC response admits the daemon-owned job. Its terminal
            // report survives connection loss and can exceed the IPC frame cap.
            let path=report_path.clone();
            let admitted=tokio::task::spawn_blocking(move ||persist(&path,&receipt)).await.map_err(|_|StoreError::Panicked).and_then(|r|r);
            if let Err(error)=admitted { *active.lock().unwrap_or_else(|p|p.into_inner())=None; let _=send.send(Err(error)); return; }
            let _=send.send(serde_json::to_vec(&serde_json::json!({"state":"admitted","job_id":job_id,"report_path":report_path})).map_err(|_|invalid("cutover_receipt_encoding")));
            let result=execute(writer,readers,retention,live,run,migration,root.join("transfers"),root.join("cutover-staging")).await;
            *active.lock().unwrap_or_else(|p|p.into_inner())=None;
            let terminal=match result { Ok(report)=>serde_json::json!({"state":"completed","job_id":job_id,"report":report}),Err(error)=>serde_json::json!({"state":"failed","job_id":job_id,"error":error.to_string()}) };
            let saved=tokio::task::spawn_blocking(move ||persist(&report_path,&terminal)).await;
            if !matches!(saved,Ok(Ok(()))) { eprintln!("cutover terminal report could not be persisted job={job_id}"); }
        });
        Ok(JobHandle::from_receiver(receive))
    }
}
fn persist(path: &Path, value: &Value) -> Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| invalid("cutover_report_path"))?;
    fs::create_dir_all(parent)?;
    let staging = path.with_extension("json.pending");
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&staging)?;
    file.write_all(&serde_json::to_vec(value).map_err(|_| invalid("cutover_report_encoding"))?)?;
    file.sync_all()?;
    drop(file);
    fs::rename(staging, path)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
/// Strict CLI grammar, reused by the executable and contract tests.
pub fn cli_request(args: &[String]) -> Result<ImportRequest> {
    if args.len() < 4 || args[..3] != ["v4", "cutover", "import"] {
        return Err(invalid(
            "usage: business-os v4 cutover import --snapshot PATH [--dry-run]",
        ));
    }
    let mut snapshot = None;
    let mut dry_run = false;
    let mut i = 3;
    while i < args.len() {
        match args[i].as_str() {
            "--snapshot" if snapshot.is_none() && i + 1 < args.len() => {
                i += 1;
                snapshot = Some(PathBuf::from(&args[i]));
            }
            "--dry-run" if !dry_run => dry_run = true,
            _ => return Err(invalid("invalid cutover argument")),
        }
        i += 1;
    }
    Ok(ImportRequest {
        snapshot: snapshot.ok_or_else(|| invalid("snapshot required"))?,
        dry_run,
    })
}
/// Same-user IPC is the only CLI path, including when the daemon is absent.
pub fn request(endpoint: ipc::StoreEndpoint, request: ImportRequest) -> Result<Vec<u8>> {
    let admitted = ipc::request(
        endpoint,
        Request {
            request_id: 1,
            kind: if request.dry_run {
                JobKind::Read
            } else {
                JobKind::Write
            },
            method: METHOD.into(),
            payload: serde_json::to_vec(&request)
                .map_err(|_| invalid("cutover_request_encoding"))?,
        },
        JobOptions {
            deadline: Some(std::time::Instant::now() + Duration::from_secs(30)),
            ..Default::default()
        },
    )?
    .blocking_wait()?;
    let receipt: Value =
        serde_json::from_slice(&admitted).map_err(|_| invalid("cutover_receipt_encoding"))?;
    eprintln!("{}", String::from_utf8_lossy(&admitted));
    let path = receipt["report_path"]
        .as_str()
        .ok_or_else(|| invalid("cutover_report_path"))?;
    let deadline = std::time::Instant::now() + Duration::from_secs(3600);
    loop {
        let bytes = fs::read(path)?;
        let state: Value =
            serde_json::from_slice(&bytes).map_err(|_| invalid("cutover_report_encoding"))?;
        match state["state"].as_str() {
            Some("completed") => return Ok(bytes),
            Some("failed") => return Err(invalid(format!("cutover_job_failed:report={path}"))),
            Some("admitted") if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(250))
            }
            _ => return Err(invalid(format!("cutover_outcome_unknown:report={path}"))),
        }
    }
}
#[cfg(test)]
#[path = "tests/daemon.rs"]
mod tests;
