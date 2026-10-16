//! E23.3: frozen native meeting rows -> the registered CTOX meeting head.
//! No schema invention: populated fields without a V4 destination stop import.
use super::*;
use rusqlite::OptionalExtension;
use serde_json::{json, Map};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const COLLECTION: &str = "workjet_jour_fixe_meetings";
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

fn row_value(id: Value, project: Value, owner: Value, scheduled: Value, metadata: Value) -> Value {
    json!({"meeting_id":id,"project_id":project,"owner_user_id":owner,
        "scheduled_at_ms":scheduled,"metadata_json":metadata,"preparation_task_id":null})
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

/// Reverse only the published head mapping; empty wire children and absent
/// nullable preparation_task_id are reconstructed without discarding content.
pub fn document_hash(body: &Value) -> Result<String> {
    required_head(body)?;
    let mut metadata: Map<_, _> = HEAD
        .iter()
        .filter_map(|k| body.get(*k).cloned().map(|v| ((*k).into(), v)))
        .collect();
    for key in CHILDREN {
        metadata.insert((*key).into(), json!([]));
    }
    Ok(hash(row_value(
        body["id"].clone(),
        body["project_id"].clone(),
        body["owner_user_id"].clone(),
        body["scheduled_at_ms"].clone(),
        Value::Object(normalize_metadata(metadata)),
    )))
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
    let columns = sql(tx.prepare("PRAGMA table_info(workjet_jour_fixe_meetings)"))?
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
        let mut gaps = Vec::new();
        if task.is_some() {
            gaps.push("preparation_task_id".to_owned());
        }
        for key in CHILDREN {
            let items = map
                .get(*key)
                .and_then(Value::as_array)
                .ok_or_else(|| invalid(format!("import_jour_fixe_required:{key}")))?;
            if !items.is_empty() {
                gaps.push(format!("metadata_json.{key}"));
            }
        }
        if !gaps.is_empty() {
            return Err(invalid(format!(
                "import_jour_fixe_schema_gap:{}:{}",
                id,
                gaps.join(",")
            )));
        }
        let source = row_value(
            json!(id),
            json!(project),
            json!(owner),
            json!(scheduled),
            Value::Object(normalize_metadata(map.clone())),
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
        let mut body = normalize_metadata(body);
        body.insert("updated_at_ms".into(), json!(0)); // unknown; never fabricate an update time
        body.insert("is_deleted".into(), json!(false));
        let body = Value::Object(body);
        let source_hash = hash(source);
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
