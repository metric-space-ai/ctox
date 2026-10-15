// Isolated executable composition of the real #828 import library. This is not
// a product CLI, signed app publication or proof that the daemon consumes it.
use ctox_store::{
    change_log,
    import::{self, base_release, core_import},
    registry::Registry,
    schema,
    writer::WriterOptions,
    DbId, JobOptions, StoreError, WriteJob,
};
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{path::Path, time::Instant};
type Result<T> = std::result::Result<T, StoreError>;
fn invalid(s: &str) -> StoreError {
    StoreError::Invalid(s.into())
}
struct Services;
impl core_import::ImportServices for Services {
    fn member_conversation_id(&self, id: &str) -> i64 {
        // Exact algorithm: crew/memory.rs:20 -> turn_loop.rs:2060, fb2f6ac15.
        let hash = Sha256::digest(format!("crew:{id}").as_bytes());
        let value =
            (u64::from_be_bytes(hash[..8].try_into().unwrap()) & 0x3fff_ffff_ffff_ffff) as i64;
        value.max(2)
    }
    fn script(&mut self, _: &str, _: &str, _: &str) -> Result<Option<(String, u64)>> {
        Err(invalid("fixture_contains_unhandled_script"))
    }
    fn artifact(&mut self, _: &str, _: Option<&str>) -> Result<Option<String>> {
        Err(invalid("fixture_contains_unhandled_artifact"))
    }
}
impl core_import::DataframeImporter for Services {
    fn import_timeseries(&mut self, _: &str, _: &[Value]) -> Result<usize> {
        Err(invalid("fixture_contains_unhandled_timeseries"))
    }
}
struct NoUnverifiedMigration;
impl base_release::MigrationEvaluator for NoUnverifiedMigration {
    fn migrate(&self, _: &str, _: &Value, _: Value) -> Result<Value> {
        Err(invalid("product_migration_evaluator_not_connected"))
    }
}
async fn head(writer: &ctox_store::writer::WriterHandle) -> Result<String> {
    writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            Ok(change_log::head(tx)?.head_seq.to_string())
        }))?
        .await
}
async fn main_run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err(invalid("usage: SNAPSHOT_DIR INPUT_ROOT DESTINATION REPORT"));
    }
    let started = Instant::now();
    let snapshot = Path::new(&args[0]);
    let inputs = Path::new(&args[1]);
    let output = Path::new(&args[2]);
    if output.exists() {
        return Err(invalid("new_destination_required"));
    }
    let defaults = schema::BaseReleaseDefaults {
        app_ver: "main@4794af03d+v4".into(),
        client_limit: 100000,
        byte_budget: 33554432,
        rights_ver: 1,
    };
    let plan = base_release::load(
        &inputs.join("v4"),
        &inputs.join("ctox-platform.manifest.json"),
        &defaults,
    )?;
    let writer = Registry.open(DbId::new(output)?, WriterOptions::default())?;
    writer.ready().await?;
    let activated = base_release::publish(&writer, &plan, &NoUnverifiedMigration, 1).await?;
    let core_path = snapshot.join("ctox.sqlite3");
    let mut core = Connection::open_with_flags(core_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let tx = core.transaction()?;
    let first = core_import::apply(
        &writer,
        core_import::collect(&tx, &mut Services, 100000, 128 * 1024 * 1024)?,
        &mut Services,
        "m7-810-core",
    )
    .await?;
    let mut legacy = Connection::open_with_flags(
        snapshot.join("business-os-rxdb.sqlite3"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let legacy_tx = legacy.transaction()?;
    let tables:Vec<(String,String)>=legacy_tx.prepare("SELECT name,substr(name,19,length(name)-22) FROM sqlite_master WHERE type='table' AND name LIKE 'ctox_business_os__e2e_%__v0' ORDER BY name")?.query_map([],|r|r.get::<_,String>(0))?.map(|r|{
        let table=r?;let collection=table.strip_prefix("ctox_business_os__").and_then(|s|s.strip_suffix("__v0")).ok_or(rusqlite::Error::InvalidQuery)?.to_string();Ok((table,collection))
    }).collect::<rusqlite::Result<_>>()?;
    if tables.is_empty() {
        return Err(invalid("zero_legacy_fixture_tables"));
    }
    let mut legacy_reports = vec![];
    for replay in [false, true] {
        let before = head(&writer).await?;
        let mut counts = vec![];
        for (table, collection) in &tables {
            let mut cursor = String::new();
            let mut total = import::Counts::default();
            let mut compared = 0usize;
            let mut tombstones = 0usize;
            loop {
                let page = base_release::legacy_page(
                    &legacy_tx,
                    &plan,
                    &NoUnverifiedMigration,
                    collection,
                    table,
                    &cursor,
                    128,
                )?;
                let documents = page.documents.clone();
                let report = import::write_page(&writer, page.documents).await?;
                let name = collection.clone();
                let verified = writer
                    .submit(WriteJob::new(JobOptions::default(), move |tx| {
                        let active = schema::active_collection(tx, &name)?
                            .ok_or_else(|| invalid("collection_inactive"))?;
                        let table = format!("\"{}\"", active.table.replace('"', "\"\""));
                        let mut deleted_count = 0usize;
                        for doc in &documents {
                            let (rev, data, deleted): (String, Option<String>, bool) = tx
                                .query_row(
                                    &format!("SELECT rev,data,deleted FROM {table} WHERE id=?1"),
                                    [&doc.id],
                                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                                )?;
                            let expected_deleted = doc.body["_deleted"].as_bool().unwrap_or(false);
                            if deleted != expected_deleted
                                || doc.revision.as_ref().is_some_and(|r| r != &rev)
                            {
                                return Err(invalid("import_row_metadata_diverged"));
                            }
                            if deleted {
                                if data.is_some() {
                                    return Err(invalid("tombstone_retained_payload"));
                                }
                                deleted_count += 1;
                            } else if data
                                .as_deref()
                                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                                .as_ref()
                                != Some(&doc.body)
                            {
                                return Err(invalid("import_row_body_diverged"));
                            }
                        }
                        Ok((documents.len(), deleted_count))
                    }))?
                    .await?;
                compared += verified.0;
                tombstones += verified.1;
                if let Some(n) = report.collections.get(collection) {
                    total.source_rows += n.source_rows;
                    total.target_documents += n.target_documents;
                    total.inserted += n.inserted;
                    total.already_present += n.already_present;
                    total.rejected += n.rejected;
                }
                cursor = page.cursor;
                if page.complete {
                    break;
                }
            }
            let active_name = collection.clone();
            let expected = total.source_rows;
            let actual = writer
                .submit(WriteJob::new(JobOptions::default(), move |tx| {
                    let active = schema::active_collection(tx, &active_name)?
                        .ok_or_else(|| invalid("collection_inactive"))?;
                    let table = format!("\"{}\"", active.table.replace('"', "\"\""));
                    Ok(
                        tx.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| {
                            r.get::<_, usize>(0)
                        })?,
                    )
                }))?
                .await?;
            if actual != expected || total.rejected != 0 {
                return Err(invalid("legacy_count_mismatch"));
            }
            counts.push(json!({"collection":collection,"counts":total,"actual_target_rows":actual,"exact_documents_compared":compared,"payload_free_tombstones":tombstones}));
        }
        let after = head(&writer).await?;
        if replay && before != after {
            return Err(invalid("replay_allocated_seq"));
        }
        legacy_reports.push(
            json!({"replay":replay,"head_before":before,"head_after":after,"collections":counts}),
        );
    }
    let before = head(&writer).await?;
    let second = core_import::apply(
        &writer,
        core_import::collect(&tx, &mut Services, 100000, 128 * 1024 * 1024)?,
        &mut Services,
        "m7-810-core",
    )
    .await?;
    let after = head(&writer).await?;
    if before != after {
        return Err(invalid("core_replay_allocated_seq"));
    }
    first.validate_complete()?;
    second.validate_complete()?;
    let result = json!({"schema":"ctox.m7.base-import.v1","source_writes":0,"base":"main@4794af03d","activated_schema_apps":activated,"core":first,"core_replay":second,"legacy":legacy_reports,"core_head_before":before,"core_head_after":after,"durationMs":started.elapsed().as_millis(),"pass":true,"daemonImportConnected":false,"signedAppsPublished":false,"generationMigrationAccepted":false,"limit":"Real library and registry into a separate destination; no product CLI/export in fb2f6ac15. File/script/dataframe inputs fail closed. Empty destination does not prove populated-generation migration."});
    writer.shutdown().await?;
    std::fs::write(&args[3], serde_json::to_vec_pretty(&result).unwrap())?;
    Ok(())
}
fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    if let Err(e) = runtime.block_on(main_run()) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
