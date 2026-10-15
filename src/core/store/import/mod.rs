//! One-time V3 takeover. Call only with an isolated source snapshot and the
//! registered destination writer; this module never opens a database or runs DDL.
pub mod base_release;
pub mod core_import;

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
                let inserted = insert_document(tx, &doc)?;
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
