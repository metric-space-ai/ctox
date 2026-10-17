//! Explicit E29 specialized projections. The generic collector retains complete
//! source rows; these adapters add only the reviewed app-facing projection.
use super::*;
use daemon::Source;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Collector {
    JourFixe,
    Core {
        tables: Vec<String>,
        original_root: PathBuf,
        copied_root: PathBuf,
        owner_user: Option<String>,
    },
    MailTracking,
    MailServer,
    Files {
        original_root: PathBuf,
        copied_root: PathBuf,
        sources: Vec<FileSource>,
        snapshots: Vec<SnapshotSource>,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileSource {
    pub table: String,
    pub collection: String,
    pub owner_app: String,
    pub owner_user: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotSource {
    pub relative_path: PathBuf,
    pub owner_app: String,
    pub owner_user: Option<String>,
    pub created_at_ms: i64,
}
pub(crate) struct PendingFiles {
    pub tables: Vec<String>,
    pub plan: Option<file_import::FilePlan>,
    pub bytes: Vec<file_import::ByteFile>,
}

fn covered(source: &Source, tables: &[String]) -> Result<()> {
    for table in tables {
        let decision = source
            .tables
            .iter()
            .find(|d| &d.table.name == table)
            .ok_or_else(|| invalid("cutover_collector_source_without_decision"))?;
        // A projection may never replace the full-row E29 archive. Protected
        // mail-server rows remain in Core and use only the allowlisted reader.
        if decision.rule != "E29-R5" && decision.rule != "E29-R6" && decision.rule != "E29.1" {
            return Err(invalid("cutover_collector_requires_full_row_retention"));
        }
    }
    Ok(())
}
fn append(
    out: &mut cutover::CutoverImport,
    extra: core_import::CoreImport,
    keys: &[String],
) -> Result<()> {
    let mut hashes = extra.report.content_hashes.clone();
    if extra.report.collections.values().any(|c| c.rejected != 0) {
        return Err(invalid("cutover_specialized_rejected_rows"));
    }
    if !extra.dataframe_rows.is_empty() {
        return Err(invalid("cutover_dataframe_owner_required"));
    }
    for doc in extra.documents {
        hashes
            .entry(doc.collection.clone())
            .or_default()
            .entry(doc.id.clone())
            .or_insert(ContentHash {
                source_sha256: cutover::document_hash(&doc.collection, &doc.body)?,
                target_sha256: None,
            });
        for key in keys {
            out.audit
                .target_documents
                .entry(key.clone())
                .or_default()
                .insert((doc.collection.clone(), doc.id.clone()));
        }
        if let Some(old) = out
            .snapshot
            .documents
            .iter()
            .find(|d| d.collection == doc.collection && d.id == doc.id)
        {
            if old.body != doc.body {
                return Err(invalid("cutover_specialized_target_conflict"));
            }
        } else {
            out.snapshot
                .report
                .collections
                .entry(doc.collection.clone())
                .or_default()
                .source_rows += 1;
            out.snapshot.documents.push(doc);
        }
    }
    out.snapshot
        .report
        .diagnostics
        .extend(extra.report.diagnostics);
    for (collection, hashes) in hashes {
        let target = out
            .snapshot
            .report
            .content_hashes
            .entry(collection)
            .or_default();
        for (id, hash) in hashes {
            if target.insert(id, hash).is_some() {
                return Err(invalid("cutover_duplicate_hash_witness"));
            }
        }
    }
    Ok(())
}
fn empty(documents: Vec<Document>) -> core_import::CoreImport {
    core_import::CoreImport {
        documents,
        dataframe_rows: vec![],
        report: ImportReport::default(),
    }
}
pub(crate) fn collect(
    tx: &Transaction<'_>,
    source: &Source,
    release: &base_release::ReleasePlan,
    out: &mut cutover::CutoverImport,
    files: &mut Vec<PendingFiles>,
    root: &Path,
    conversation: Option<daemon_core::Conversation>,
) -> Result<()> {
    let mut used = BTreeSet::new();
    for collector in &source.collectors {
        let tables: Vec<String> = match collector {
            Collector::JourFixe => vec![jour_fixe::COLLECTION.into()],
            Collector::Core { tables, .. } => {
                let actual: BTreeSet<_> = source
                    .tables
                    .iter()
                    .map(|d| d.table.name.as_str())
                    .filter(|n| daemon_core::TABLES.contains(n))
                    .collect();
                if actual != tables.iter().map(String::as_str).collect()
                    || source.kind != cutover::SourceKind::Core
                {
                    return Err(invalid("cutover_core_collector_coverage"));
                }
                tables.clone()
            }
            Collector::MailTracking => [
                "stalwart_mail_tracking_tokens",
                "stalwart_mail_tracking_events",
            ]
            .map(str::to_owned)
            .to_vec(),
            Collector::MailServer => [
                "stalwart_domains",
                "stalwart_users",
                "stalwart_runtime_config",
            ]
            .map(str::to_owned)
            .to_vec(),
            Collector::Files { sources, .. } => sources.iter().map(|s| s.table.clone()).collect(),
        };
        covered(source, &tables)?;
        for table in &tables {
            if !used.insert(table.clone()) {
                return Err(invalid("cutover_duplicate_collector_source"));
            }
            let decision = source
                .tables
                .iter()
                .find(|d| &d.table.name == table)
                .unwrap();
            if !matches!(
                decision.table.mode,
                cutover::Mode::KeepCore | cutover::Mode::ArchiveOnly
            ) && release
                .collections
                .values()
                .flatten()
                .any(|d| d.name == decision.table.collection)
            {
                return Err(invalid("cutover_collector_raw_row_must_be_archived"));
            }
        }
        let keys: Vec<String> = tables
            .iter()
            .map(|t| format!("{}:{t}", source.choice.database))
            .collect();
        match collector {
            Collector::Core {
                original_root,
                copied_root,
                owner_user,
                ..
            } => {
                let copied = copied_root.canonicalize()?;
                if !copied.starts_with(root)
                    || std::fs::symlink_metadata(copied_root)?
                        .file_type()
                        .is_symlink()
                {
                    return Err(invalid("cutover_snapshot_path_escape"));
                }
                let mut services = daemon_core::Services::new(
                    conversation
                        .ok_or_else(|| invalid("cutover_core_conversation_rule_required"))?,
                    original_root,
                    &copied,
                    owner_user.clone(),
                )?;
                let archive_timeseries = source
                    .tables
                    .iter()
                    .any(|d| d.table.name == "iot_datapoints" && d.rule == "E29.1");
                let mut extra = if archive_timeseries {
                    let key = format!("{}:iot_datapoints", source.choice.database);
                    let rows = freeze::measure(tx, "iot_datapoints")?.row_count;
                    if out.audit.source_rows.get(&key) != Some(&rows)
                        || out
                            .audit
                            .target_documents
                            .get(&key)
                            .map_or(0, BTreeSet::len)
                            != rows
                    {
                        return Err(invalid("cutover_timeseries_archive_incomplete"));
                    }
                    core_import::collect_archiving_timeseries(
                        tx,
                        &mut services,
                        2_000_000,
                        256 * 1024 * 1024,
                    )?
                } else {
                    core_import::collect(tx, &mut services, 2_000_000, 256 * 1024 * 1024)?
                };
                // E29-R2: an unregistered computed view cannot become a target.
                // Every original row is already archived or retained by E29-R5/6.
                let mut declared_documents = Vec::new();
                for doc in extra.documents {
                    if release.collections.values().flatten().any(|d| d.name == doc.collection) {
                        declared_documents.push(doc);
                    } else {
                        extra.report.diagnostics.push(format!("core_projection_unregistered:{}",doc.collection));
                    }
                }
                extra.documents = declared_documents;
                let keys: Vec<_> = keys
                    .into_iter()
                    .filter(|key| {
                        !source.tables.iter().any(|d| {
                            (d.rule == "E29.1" || !release.collections.values().flatten().any(|schema|schema.name==d.table.collection))
                                && *key == format!("{}:{}", source.choice.database, d.table.name)
                        })
                    })
                    .collect();
                append(out, extra, &keys)?;
                files.push(PendingFiles {
                    tables: keys,
                    plan: None,
                    bytes: services.files,
                });
            }
            Collector::JourFixe => append(
                out,
                jour_fixe::collect_bos(tx, 2_000_000, 256 * 1024 * 1024)?,
                &keys,
            )?,
            Collector::MailTracking => append(out, core_import::collect_mail_tracking(tx)?, &keys)?,
            Collector::MailServer => append(out, empty(core_import::mail_server(tx)?), &keys)?,
            Collector::Files {
                original_root,
                copied_root,
                sources,
                snapshots,
            } => {
                let copied = copied_root.canonicalize()?;
                if !copied.starts_with(root)
                    || std::fs::symlink_metadata(copied_root)?
                        .file_type()
                        .is_symlink()
                {
                    return Err(invalid("cutover_snapshot_path_escape"));
                }
                let sources: Vec<_> = sources
                    .iter()
                    .map(|s| file_import::LegacyCollection {
                        table: s.table.clone(),
                        collection: s.collection.clone(),
                        owner_app: s.owner_app.clone(),
                        owner_user: s.owner_user.clone(),
                    })
                    .collect();
                let snapshots: Vec<_> = snapshots
                    .iter()
                    .map(|s| file_import::SnapshotFile {
                        relative_path: s.relative_path.clone(),
                        owner_app: s.owner_app.clone(),
                        owner_user: s.owner_user.clone(),
                        created_at_ms: s.created_at_ms,
                    })
                    .collect();
                let plan = file_import::collect(
                    tx,
                    &sources,
                    &snapshots,
                    &file_import::ProfilePaths {
                        original_root: original_root.clone(),
                        copied_root: copied,
                    },
                    &file_import::Budget {
                        rows: 2_000_000,
                        bytes: 256 * 1024 * 1024,
                    },
                )?;
                files.push(PendingFiles {
                    tables: keys,
                    plan: Some(plan),
                    bytes: vec![],
                });
            }
        }
    }
    Ok(())
}
