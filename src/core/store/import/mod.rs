//! One-time V3 takeover. Call only with an isolated source snapshot and the
//! registered destination writer; this module never opens a database or runs DDL.
pub mod base_release;
pub mod core_import;
pub mod cutover;
pub mod daemon;
pub mod daemon_collectors;
pub mod daemon_core;
pub mod file_import;
pub mod freeze;
pub mod jour_fixe;
pub mod owner_alias;

use crate::{
    types::{JobOptions, StoreError, WriteJob},
    writer::WriterHandle,
};
use rusqlite::{params, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub type Result<T> = std::result::Result<T, StoreError>;
pub(crate) fn invalid(message: impl Into<String>) -> StoreError {
    StoreError::Invalid(message.into())
}
pub(crate) fn sql<T>(value: rusqlite::Result<T>) -> Result<T> {
    value.map_err(Into::into)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Document {
    pub collection: String,
    pub id: String,
    pub body: Value,
    #[serde(default)]
    pub revision: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Counts {
    pub source_rows: usize,
    pub target_documents: usize,
    pub inserted: usize,
    pub already_present: usize,
    pub rejected: usize,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ImportReport {
    pub collections: BTreeMap<String, Counts>,
    /// Codes and source IDs only; never raw rows, configuration or credentials.
    pub diagnostics: Vec<String>,
    /// Raw source rows before chunk assembly/grouping; logical entities above.
    #[serde(default)]
    pub physical_source_rows: BTreeMap<String, usize>,
    /// Semantic source-row hashes, compared with persisted target rows.
    #[serde(default)]
    pub content_hashes: BTreeMap<String, BTreeMap<String, ContentHash>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContentHash {
    pub source_sha256: String,
    pub target_sha256: Option<String>,
}

impl ImportReport {
    /// Never certify a partial or rejected takeover as a completed import.
    pub fn validate_complete(&self) -> Result<()> {
        for (name, counts) in &self.collections {
            if counts.source_rows != counts.target_documents || counts.rejected != 0 {
                return Err(invalid(format!("import_count_mismatch:{name}")));
            }
        }
        for (name, hashes) in &self.content_hashes {
            if self
                .collections
                .get(name)
                .is_none_or(|c| c.source_rows != hashes.len())
                || hashes
                    .values()
                    .any(|h| h.target_sha256.as_ref() != Some(&h.source_sha256))
            {
                return Err(invalid(format!("import_content_mismatch:{name}")));
            }
        }
        Ok(())
    }
}

/// Insert only: reruns cannot roll back app edits or resurrect app tombstones.
/// A differing existing document is an explicit collision, never an overwrite.
pub(crate) fn insert_document(tx: &Transaction<'_>, doc: &Document) -> Result<bool> {
    if doc.id.is_empty() || doc.body.get("id").and_then(Value::as_str) != Some(&doc.id) {
        return Err(invalid("import_document_identity_mismatch"));
    }
    let active = sql(crate::schema::active_collection(tx, &doc.collection))?
        .ok_or_else(|| invalid(format!("import_collection_not_active:{}", doc.collection)))?;
    let table = sql(crate::change_log::identifier(&active.table))?;
    let previous: Option<(Option<String>, bool)> = sql(tx
        .query_row(
            &format!("SELECT data,deleted FROM {table} WHERE id=?1"),
            [&doc.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional())?;
    let incoming_delete = doc.body["_deleted"].as_bool().unwrap_or(false);
    // The base publisher now exposes the V4 contract. Initial legacy import
    // must satisfy the same migration boundary as generation backfill; merely
    // stamping schema_ver=3 on a legacy body would bypass the Core contract.
    if doc.collection == "business_commands"
        && active.definition.schema_ver == 3
        && !incoming_delete
    {
        sql(
            crate::change_log::subscription::write_contract::validate_migrated_command(
                &doc.body, &doc.id,
            ),
        )?;
    }
    if let Some((body, deleted)) = previous {
        if deleted != incoming_delete
            || (!deleted
                && body
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                    .as_ref()
                    != Some(&doc.body))
        {
            return Err(invalid(format!(
                "import_target_conflict:{}:{}",
                doc.collection, doc.id
            )));
        }
        return Ok(false);
    }
    sql(tx.execute(
        &format!("INSERT INTO {table}(id,rev,schema_ver,data,deleted) VALUES(?1,?2,?3,?4,?5)"),
        params![
            doc.id,
            doc.revision
                .clone()
                .unwrap_or_else(|| format!("1-import:{}", doc.id)),
            active.definition.schema_ver,
            if incoming_delete {
                None
            } else {
                Some(doc.body.to_string())
            },
            incoming_delete
        ],
    ))?;
    Ok(true)
}

/// Bound one owner job to a page. Schema triggers allocate the only global seq.
/// The report is returned only after commit; a collision rolls back the page.
pub async fn write_page(writer: &WriterHandle, docs: Vec<Document>) -> Result<ImportReport> {
    write_page_audited(writer, docs, BTreeMap::new()).await
}

pub(crate) async fn write_page_audited(
    writer: &WriterHandle,
    docs: Vec<Document>,
    expected: BTreeMap<(String, String), String>,
) -> Result<ImportReport> {
    if docs.len() > 128 {
        return Err(invalid("import_page_limit"));
    }
    if docs.len() > 1 && docs.iter().map(|d| d.body.to_string().len()).sum::<usize>() > 1024 * 1024
    {
        return Err(invalid("import_page_byte_limit"));
    }
    writer
        .submit(WriteJob::new(JobOptions::default(), move |tx| {
            let mut report = ImportReport::default();
            for doc in docs {
                if doc.collection == jour_fixe::COLLECTION {
                    jour_fixe::check_target_schema(tx, &doc)?;
                }
                let inserted = insert_document(tx, &doc)?;
                if let Some(source) = expected.get(&(doc.collection.clone(), doc.id.clone())) {
                    let target = if doc.collection == jour_fixe::COLLECTION {
                        jour_fixe::persisted_hash(tx, &doc)?
                    } else {
                        cutover::persisted_hash(tx, &doc)?
                    };
                    if source != &target {
                        return Err(invalid(format!(
                            "import_content_mismatch:{}:{}",
                            doc.collection, doc.id
                        )));
                    }
                    report
                        .content_hashes
                        .entry(doc.collection.clone())
                        .or_default()
                        .insert(
                            doc.id.clone(),
                            ContentHash {
                                source_sha256: source.clone(),
                                target_sha256: Some(target),
                            },
                        );
                }
                let count = report.collections.entry(doc.collection).or_default();
                count.source_rows += 1;
                count.target_documents += 1;
                count.inserted += usize::from(inserted);
                count.already_present += usize::from(!inserted);
            }
            Ok(report)
        }))?
        .await
}

#[cfg(test)]
mod tests;
