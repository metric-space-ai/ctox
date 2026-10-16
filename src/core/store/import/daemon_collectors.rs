//! Explicit E29 specialized projections. The generic collector retains complete
//! source rows; these adapters add only the reviewed app-facing projection.
use super::*;
use daemon::Source;
use std::{collections::BTreeSet, path::{Path, PathBuf}};

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Collector {
    JourFixe,
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
pub struct PendingFiles {
    pub tables: Vec<String>,
    pub plan: file_import::FilePlan,
}

fn covered(source: &Source, tables: &[String]) -> Result<()> {
    for table in tables {
        let decision = source.tables.iter().find(|d| &d.table.name == table)
            .ok_or_else(|| invalid("cutover_collector_source_without_decision"))?;
        // A projection may never replace the full-row E29 archive. Protected
        // mail-server rows remain in Core and use only the allowlisted reader.
        if decision.rule != "E29-R5" && decision.rule != "E29-R6" {
            return Err(invalid("cutover_collector_requires_full_row_retention"));
        }
    }
    Ok(())
}
fn append(out: &mut cutover::CutoverImport, extra: core_import::CoreImport, keys: &[String]) -> Result<()> {
    let mut hashes = extra.report.content_hashes.clone();
    if !extra.dataframe_rows.is_empty() {
        return Err(invalid("cutover_dataframe_owner_required"));
    }
    for doc in extra.documents {
        hashes.entry(doc.collection.clone()).or_default().entry(doc.id.clone()).or_insert(ContentHash {
            source_sha256: cutover::document_hash(&doc.collection, &doc.body)?, target_sha256: None,
        });
        for key in keys {
            out.audit.target_documents.entry(key.clone()).or_default()
                .insert((doc.collection.clone(), doc.id.clone()));
        }
        if let Some(old) = out.snapshot.documents.iter().find(|d| d.collection == doc.collection && d.id == doc.id) {
            if old.body != doc.body { return Err(invalid("cutover_specialized_target_conflict")); }
        } else {
            out.snapshot.report.collections.entry(doc.collection.clone()).or_default().source_rows += 1;
            out.snapshot.documents.push(doc);
        }
    }
    out.snapshot.report.diagnostics.extend(extra.report.diagnostics);
    for (collection, hashes) in hashes {
        let target = out.snapshot.report.content_hashes.entry(collection).or_default();
        for (id, hash) in hashes {
            if target.insert(id, hash).is_some() { return Err(invalid("cutover_duplicate_hash_witness")); }
        }
    }
    Ok(())
}
fn empty(documents: Vec<Document>) -> core_import::CoreImport {
    core_import::CoreImport { documents, dataframe_rows: vec![], report: ImportReport::default() }
}
pub fn collect(
    tx: &Transaction<'_>, source: &Source, release: &base_release::ReleasePlan,
    out: &mut cutover::CutoverImport, files: &mut Vec<PendingFiles>, root: &Path,
) -> Result<()> {
    let mut used = BTreeSet::new();
    for collector in &source.collectors {
        let tables: Vec<String> = match collector {
            Collector::JourFixe => vec![jour_fixe::COLLECTION.into()],
            Collector::MailTracking => ["stalwart_mail_tracking_tokens", "stalwart_mail_tracking_events"].map(str::to_owned).to_vec(),
            Collector::MailServer => ["stalwart_domains", "stalwart_users", "stalwart_runtime_config"].map(str::to_owned).to_vec(),
            Collector::Files { sources, .. } => sources.iter().map(|s| s.table.clone()).collect(),
        };
        covered(source, &tables)?;
        for table in &tables {
            if !used.insert(table.clone()) { return Err(invalid("cutover_duplicate_collector_source")); }
            let decision = source.tables.iter().find(|d| &d.table.name == table).unwrap();
            if !matches!(decision.table.mode, cutover::Mode::KeepCore)
                && release.collections.values().flatten().any(|d| d.name == decision.table.collection) {
                return Err(invalid("cutover_collector_raw_row_must_be_archived"));
            }
        }
        let keys: Vec<String> = tables.iter().map(|t| format!("{}:{t}", source.choice.database)).collect();
        match collector {
            Collector::JourFixe => append(out, jour_fixe::collect_bos(tx, 2_000_000, 256 * 1024 * 1024)?, &keys)?,
            Collector::MailTracking => append(out, core_import::collect_mail_tracking(tx)?, &keys)?,
            Collector::MailServer => append(out, empty(core_import::mail_server(tx)?), &keys)?,
            Collector::Files { original_root, copied_root, sources, snapshots } => {
                let copied = copied_root.canonicalize()?;
                if !copied.starts_with(root) || std::fs::symlink_metadata(copied_root)?.file_type().is_symlink() {
                    return Err(invalid("cutover_snapshot_path_escape"));
                }
                let sources: Vec<_> = sources.iter().map(|s| file_import::LegacyCollection {
                    table: s.table.clone(), collection: s.collection.clone(), owner_app: s.owner_app.clone(), owner_user: s.owner_user.clone(),
                }).collect();
                let snapshots: Vec<_> = snapshots.iter().map(|s| file_import::SnapshotFile {
                    relative_path: s.relative_path.clone(), owner_app: s.owner_app.clone(), owner_user: s.owner_user.clone(), created_at_ms: s.created_at_ms,
                }).collect();
                let plan = file_import::collect(tx, &sources, &snapshots,
                    &file_import::ProfilePaths { original_root: original_root.clone(), copied_root: copied },
                    &file_import::Budget { rows: 2_000_000, bytes: 256 * 1024 * 1024 })?;
                files.push(PendingFiles { tables: keys, plan });
            }
        }
    }
    Ok(())
}
