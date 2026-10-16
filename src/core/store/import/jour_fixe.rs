//! E23.3/E28: frozen BOS meeting rows -> registered CTOX meetings and files.
//! Large fields are staged in the existing CAS outside the source transaction.
use super::*;
use rusqlite::OptionalExtension;
use serde_json::{json, Map};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const COLLECTION: &str = "workjet_jour_fixe_meetings";
/// Caller opens the frozen business-os.sqlite3 snapshot, never a live file.
/// Missing table is an error: a mistaken Core snapshot cannot look empty.
pub fn collect_bos(
    tx: &Transaction<'_>,
    max_documents: usize,
    max_bytes: usize,
) -> Result<core_import::CoreImport> {
    let mut out = core_import::CoreImport {
        documents: vec![],
        dataframe_rows: vec![],
        report: ImportReport::default(),
    };
    collect_into(tx, &mut out)?;
    if out
        .report
        .diagnostics
        .iter()
        .any(|d| d == &format!("source_table_absent:{COLLECTION}"))
    {
        return Err(invalid("import_jour_fixe_bos_source_table_missing"));
    }
    if out.documents.len() > max_documents
        || out
            .documents
            .iter()
            .map(|d| d.body.to_string().len())
            .sum::<usize>()
            > max_bytes
    {
        return Err(invalid("import_snapshot_budget_exceeded"));
    }
    Ok(out)
}

/// Call AFTER ending the BOS read transaction. Existing Transfer-CAS and files
/// metadata are used; bytes are verified before entering a writer job.
pub async fn prepare_files(
    mut snapshot: core_import::CoreImport,
    cas: &crate::cas::Cas,
    scratch: &std::path::Path,
) -> Result<core_import::CoreImport> {
    let mut files = Vec::new();
    for doc in snapshot
        .documents
        .iter_mut()
        .filter(|d| d.collection == COLLECTION)
    {
        let before = document_hash(&doc.body)?;
        for key in CHILDREN {
            let bytes =
                serde_json::to_vec(&doc.body[*key]).map_err(|_| invalid("meeting_field_json"))?;
            if bytes.len() <= INLINE_LIMIT {
                continue;
            }
            let sha = format!("{:x}", Sha256::digest(&bytes));
            let size = bytes.len();
            let id = format!(
                "jf:{:x}",
                Sha256::digest(format!("{}:{key}", doc.id).as_bytes())
            );
            files.push(file_import::ByteFile {
                id,
                name: format!("{}-{key}.json", doc.id),
                mime: "application/json".into(),
                owner_app: "ctox".into(),
                owner_user: doc.body["owner_user_id"].as_str().map(str::to_owned),
                bytes,
            });
            doc.body[*key] = json!({"blob_ref":sha,"size":size,"sha256":sha});
        }
        if before != document_hash(&doc.body)? {
            return Err(invalid("meeting_blob_hash_mismatch"));
        }
    }
    let prepared = file_import::prepare_bytes(files, cas, scratch).await?;
    file_import::attach(snapshot, prepared)
}

/// Source SQL columns are deliberately listed independently of the JSON head.
const BINDINGS: &[(&str, &str)] = &[
    ("meeting_id", "id"),
    ("project_id", "project_id"),
    ("owner_user_id", "owner_user_id"),
    ("scheduled_at_ms", "scheduled_at_ms"),
];
const HEAD: &[&str] = &[
    "id",
    "project_id",
    "owner_user_id",
    "scheduled_at_ms",
    "supervisor",
    "prepare_at_ms",
    "timezone",
    "state",
    "revision",
    "deck_revision",
    "previous_goal",
    "todos",
    "error",
];
const CHILDREN: &[&str] = &["slides", "comments", "transcript"];
const OPTIONAL: &[&str] = &["previous_goal", "todos", "error"];
pub const INLINE_LIMIT: usize = 256 * 1024;

fn hash(value: Value) -> String {
    fn ordered(value: Value) -> Value {
        match value {
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .collect::<BTreeMap<_, _>>()
                    .into_iter()
                    .map(|(k, v)| (k, ordered(v)))
                    .collect(),
            ),
            Value::Array(items) => Value::Array(items.into_iter().map(ordered).collect()),
            other => other,
        }
    }
    format!(
        "{:x}",
        Sha256::digest(ordered(value).to_string().as_bytes())
    )
}

fn normalize_metadata(mut metadata: Map<String, Value>) -> Map<String, Value> {
    // Wire optionals serialize as either absent or null; both mean None.
    for key in OPTIONAL {
        if metadata.get(*key) == Some(&Value::Null) {
            metadata.remove(*key);
        }
    }
    metadata
}

fn row_value(
    id: Value,
    project: Value,
    owner: Value,
    scheduled: Value,
    metadata: Value,
    task: Value,
) -> Value {
    json!({"meeting_id":id,"project_id":project,"owner_user_id":owner,
        "scheduled_at_ms":scheduled,"metadata_json":metadata,"preparation_task_id":task})
}

fn required_head(body: &Value) -> Result<()> {
    for key in HEAD.iter().filter(|k| !OPTIONAL.contains(k)) {
        if body.get(*key).is_none_or(Value::is_null) {
            return Err(invalid(format!("import_jour_fixe_required:{key}")));
        }
    }
    for key in ["id", "project_id", "owner_user_id", "timezone"] {
        if body[key].as_str().is_none_or(str::is_empty) {
            return Err(invalid(format!("import_jour_fixe_identity:{key}")));
        }
    }
    for key in [
        "scheduled_at_ms",
        "prepare_at_ms",
        "revision",
        "deck_revision",
    ] {
        if body[key].as_u64().is_none_or(|n| n > 9_007_199_254_740_991) {
            return Err(invalid(format!("import_jour_fixe_integer:{key}")));
        }
    }
    if ![
        "planned",
        "preparing",
        "ready",
        "live",
        "review",
        "confirmed",
        "cancelled",
        "failed",
    ]
    .contains(&body["state"].as_str().unwrap_or(""))
    {
        return Err(invalid("import_jour_fixe_state"));
    }
    for key in ["workjet_thread_id", "ctox_thread_key"] {
        if body["supervisor"][key].as_str().is_none_or(str::is_empty) {
            return Err(invalid(format!("import_jour_fixe_supervisor:{key}")));
        }
    }
    Ok(())
}

/// Hash the whole semantic row, including each child's content digest. Inline
/// arrays and verified CAS references have the same representation, never an
/// omitted field. The source JSON's irrelevant object-key order is normalized.
fn field_fingerprint(value: &Value) -> Result<Value> {
    if value.is_array() {
        let bytes = serde_json::to_vec(value).map_err(|_| invalid("meeting_field_json"))?;
        return Ok(json!({"size":bytes.len(),"sha256":format!("{:x}", Sha256::digest(&bytes))}));
    }
    let object = value
        .as_object()
        .ok_or_else(|| invalid("meeting_field_shape"))?;
    let sha = value["sha256"]
        .as_str()
        .ok_or_else(|| invalid("meeting_blob_sha256"))?;
    if object.len() != 3
        || sha.len() != 64
        || !sha
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || value["blob_ref"] != sha
        || value["size"]
            .as_u64()
            .is_none_or(|n| n <= INLINE_LIMIT as u64 || n > 9_007_199_254_740_991)
    {
        return Err(invalid("meeting_blob_ref"));
    }
    Ok(json!({"size":value["size"],"sha256":sha}))
}
fn row_hash(mut row: Value) -> Result<String> {
    for key in CHILDREN {
        row["metadata_json"][*key] = field_fingerprint(&row["metadata_json"][*key])?;
    }
    Ok(hash(row))
}

/// Reverse every imported SQL column, preserving the complete wire metadata.
pub fn document_hash(body: &Value) -> Result<String> {
    required_head(body)?;
    let mut metadata: Map<_, _> = HEAD
        .iter()
        .filter_map(|k| body.get(*k).cloned().map(|v| ((*k).into(), v)))
        .collect();
    for key in CHILDREN {
        metadata.insert(
            (*key).into(),
            body.get(*key)
                .cloned()
                .ok_or_else(|| invalid(format!("import_jour_fixe_required:{key}")))?,
        );
    }
    row_hash(row_value(
        body["id"].clone(),
        body["project_id"].clone(),
        body["owner_user_id"].clone(),
        body["scheduled_at_ms"].clone(),
        Value::Object(normalize_metadata(metadata)),
        body.get("preparation_task_id")
            .cloned()
            .ok_or_else(|| invalid("import_jour_fixe_required:preparation_task_id"))?,
    ))
}

pub(super) fn collect_into(tx: &Transaction<'_>, out: &mut core_import::CoreImport) -> Result<()> {
    out.report.collections.entry(COLLECTION.into()).or_default();
    let exists: bool = sql(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [COLLECTION],
        |r| r.get(0),
    ))?;
    if !exists {
        out.report
            .diagnostics
            .push(format!("source_table_absent:{COLLECTION}"));
        return Ok(());
    }
    let columns = sql(tx.prepare("PRAGMA table_xinfo(workjet_jour_fixe_meetings)"))?
        .query_map([], |r| r.get::<_, String>(1))
        .map_err(StoreError::from)?
        .collect::<rusqlite::Result<BTreeSet<_>>>()
        .map_err(StoreError::from)?;
    let expected: BTreeSet<String> = BINDINGS
        .iter()
        .map(|(k, _)| (*k).to_owned())
        .chain(["metadata_json".into(), "preparation_task_id".into()])
        .collect();
    let extra: Vec<_> = columns.difference(&expected).cloned().collect();
    let missing: Vec<_> = expected.difference(&columns).cloned().collect();
    if !extra.is_empty() || !missing.is_empty() {
        return Err(invalid(format!(
            "import_jour_fixe_columns:extra={};missing={}",
            extra.join(","),
            missing.join(",")
        )));
    }
    let source_rows: usize = sql(tx.query_row(
        "SELECT count(*) FROM workjet_jour_fixe_meetings",
        [],
        |r| r.get(0),
    ))?;
    let mut statement = sql(tx.prepare("SELECT meeting_id,project_id,owner_user_id,scheduled_at_ms,metadata_json,preparation_task_id FROM workjet_jour_fixe_meetings ORDER BY meeting_id"))?;
    let mut cursor = sql(statement.query([]))?;
    let mut seen = BTreeSet::new();
    let mut documents = Vec::new();
    let mut hashes = BTreeMap::new();
    while let Some(row) = sql(cursor.next())? {
        let id: String = sql(row.get(0))?;
        let project: String = sql(row.get(1))?;
        let owner: String = sql(row.get(2))?;
        let scheduled: i64 = sql(row.get(3))?;
        let raw: String = sql(row.get(4))?;
        let task: Option<String> = sql(row.get(5))?;
        let metadata: Value =
            serde_json::from_str(&raw).map_err(|_| invalid("import_jour_fixe_metadata_json"))?;
        let map = metadata
            .as_object()
            .ok_or_else(|| invalid("import_jour_fixe_metadata_object"))?;
        let unknown: Vec<_> = map
            .keys()
            .filter(|k| !HEAD.contains(&k.as_str()) && !CHILDREN.contains(&k.as_str()))
            .cloned()
            .collect();
        if !unknown.is_empty() {
            return Err(invalid(format!(
                "import_jour_fixe_metadata_fields:{}",
                unknown.join(",")
            )));
        }
        if task
            .as_ref()
            .is_some_and(|t| t.is_empty() || t.chars().count() > 128)
        {
            return Err(invalid("import_jour_fixe_preparation_task_id"));
        }
        for (key, limit) in [("slides", 100), ("comments", 1000), ("transcript", 10000)] {
            let items = map
                .get(key)
                .and_then(Value::as_array)
                .ok_or_else(|| invalid(format!("import_jour_fixe_required:{key}")))?;
            if items.len() > limit {
                return Err(invalid(format!("import_jour_fixe_max_items:{key}")));
            }
        }
        let source = row_value(
            json!(id),
            json!(project),
            json!(owner),
            json!(scheduled),
            Value::Object(normalize_metadata(map.clone())),
            json!(task),
        );
        let mut body: Map<String, Value> = HEAD
            .iter()
            .filter(|k| !BINDINGS.iter().any(|(_, v)| v == *k))
            .filter_map(|k| map.get(*k).cloned().map(|v| ((*k).into(), v)))
            .collect();
        for (column, field) in BINDINGS {
            if source[*column] != metadata[*field] {
                return Err(invalid(format!("import_jour_fixe_binding:{column}")));
            }
            body.insert((*field).into(), source[*column].clone());
        }
        for key in CHILDREN {
            body.insert((*key).into(), metadata[*key].clone());
        }
        body.insert("preparation_task_id".into(), json!(task));
        let mut body = normalize_metadata(body);
        body.insert("updated_at_ms".into(), json!(0)); // unknown; never fabricate an update time
        body.insert("is_deleted".into(), json!(false));
        let body = Value::Object(body);
        let source_hash = row_hash(source)?;
        if document_hash(&body)? != source_hash {
            return Err(invalid(format!(
                "import_content_mismatch:{COLLECTION}:{id}"
            )));
        }
        if !seen.insert(id.clone()) {
            return Err(invalid("import_jour_fixe_duplicate_identity"));
        }
        hashes.insert(
            id.clone(),
            ContentHash {
                source_sha256: source_hash,
                target_sha256: None,
            },
        );
        documents.push(Document {
            collection: COLLECTION.into(),
            id,
            body,
            revision: None,
        });
    }
    if documents.len() != source_rows {
        return Err(invalid(format!("import_count_mismatch:{COLLECTION}")));
    }
    out.report
        .physical_source_rows
        .insert(COLLECTION.into(), source_rows);
    out.report
        .collections
        .entry(COLLECTION.into())
        .or_default()
        .source_rows = source_rows;
    out.report.content_hashes.insert(COLLECTION.into(), hashes);
    out.documents.extend(documents);
    Ok(())
}

pub(super) fn check_target_schema(tx: &Transaction<'_>, doc: &Document) -> Result<()> {
    let active = sql(crate::schema::active_collection(tx, COLLECTION))?
        .ok_or_else(|| invalid("import_jour_fixe_collection_not_active"))?;
    if active.definition.app_id != "ctox" {
        return Err(invalid("import_jour_fixe_app_owner"));
    }
    let properties = active.definition.schema_json["properties"]
        .as_object()
        .ok_or_else(|| invalid("import_jour_fixe_target_schema"))?;
    let missing: Vec<_> = doc
        .body
        .as_object()
        .ok_or_else(|| invalid("import_jour_fixe_head"))?
        .keys()
        .filter(|k| !properties.contains_key(*k))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Err(invalid(format!(
            "import_jour_fixe_target_fields:{}",
            missing.join(",")
        )));
    }
    for key in CHILDREN {
        if doc.body[*key].is_array()
            && serde_json::to_vec(&doc.body[*key]).unwrap().len() > INLINE_LIMIT
        {
            return Err(invalid(format!("import_jour_fixe_blob_not_prepared:{key}")));
        }
    }
    required_head(&doc.body)
}

pub(super) fn persisted_hash(tx: &Transaction<'_>, doc: &Document) -> Result<String> {
    let active = sql(crate::schema::active_collection(tx, COLLECTION))?
        .ok_or_else(|| invalid("import_jour_fixe_collection_not_active"))?;
    let table = sql(crate::change_log::identifier(&active.table))?;
    let raw: Option<String> = sql(tx
        .query_row(
            &format!("SELECT data FROM {table} WHERE id=?1 AND deleted=0"),
            [&doc.id],
            |r| r.get(0),
        )
        .optional())?;
    let body: Value =
        serde_json::from_str(&raw.ok_or_else(|| invalid("import_jour_fixe_target_missing"))?)
            .map_err(|_| invalid("import_jour_fixe_target_json"))?;
    document_hash(&body)
}
