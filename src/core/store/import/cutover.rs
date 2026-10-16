//! E29 offline cutover policy. Explicit frozen sources, no discovery or DDL.
use super::*;
use base_release::{MigrationEvaluator, ReleasePlan};
use rusqlite::{types::ValueRef, DatabaseName};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub const ARCHIVE: &str = "legacy_records";
const INLINE: usize = 256 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Rxdb,
    Core,
    Mirror,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Entity {
    App,
    Core,
}
impl Entity {
    fn authority(self) -> SourceKind {
        match self {
            Self::App => SourceKind::Rxdb,
            Self::Core => SourceKind::Core,
        }
    }
}
/// Supplied by DevOps from its freeze receipt, not inferred from a basename.
pub struct FileChoice {
    pub database: String,
    pub live_file: PathBuf,
    pub frozen_file: PathBuf,
    pub lsof_receipt_sha256: String,
}
/// An explicit projection retains the complete SQL row in the archive as well.
pub enum Mode {
    Rows,
    MirrorRows { collection_column: String },
    Projection { columns: Vec<String> },
    KeepCore,
}
pub struct Table {
    pub name: String,
    pub collection: String,
    pub version: u32,
    pub id_column: String,
    pub body_column: Option<String>,
    pub entity: Entity,
    pub mode: Mode,
}
pub struct FrozenInput<'a, 'c> {
    pub tx: &'a Transaction<'c>,
    pub file: &'a FileChoice,
    /// The source selected for this snapshot, as supplied by the freeze caller.
    pub copied_from: &'a Path,
    pub kind: SourceKind,
    /// Exact internal table from the frozen RxDB inventory.
    pub internal_table: Option<&'a str>,
    pub tables: &'a [Table],
}
#[derive(Serialize, Deserialize, Debug)]
pub struct Finding {
    pub code: String,
    pub source_db: String,
    pub source_table: String,
    pub collection: String,
    pub record_id: String,
    pub authority_sha256: Option<String>,
    pub mirror_sha256: String,
    pub archive_id: String,
}
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Audit {
    pub retained_core_tables: Vec<String>,
    pub findings: Vec<Finding>,
    pub compared_equal: Vec<String>,
    pub source_rows: BTreeMap<String, usize>,
    pub source_row_hashes: BTreeMap<String, BTreeMap<String, String>>,
}
pub struct CutoverImport {
    pub snapshot: core_import::CoreImport,
    pub audit: Audit,
}
pub struct Budget {
    pub rows: usize,
    pub bytes: usize,
}
fn canonical(value: Value) -> Value {
    match value {
        Value::Object(v) => Value::Object(
            v.into_iter()
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .map(|(k, v)| (k, canonical(v)))
                .collect(),
        ),
        Value::Array(v) => Value::Array(v.into_iter().map(canonical).collect()),
        v => v,
    }
}
fn encoded(v: &Value) -> Vec<u8> {
    canonical(v.clone()).to_string().into_bytes()
}
fn sha(v: &Value) -> String {
    format!("{:x}", Sha256::digest(encoded(v)))
}
fn digest_valid(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn bound(value: &str, max: usize) -> Result<()> {
    if value.is_empty() || value.len() > max {
        Err(invalid("cutover_identity"))
    } else {
        Ok(())
    }
}
fn validate_file(input: &FrozenInput<'_, '_>) -> Result<()> {
    // E29-R7: no fallback to root/runtime or the most recently modified file.
    if input.copied_from != input.file.live_file
        || input.file.live_file == input.file.frozen_file
        || !input.file.live_file.is_absolute()
        || !input.file.frozen_file.is_absolute()
        || !digest_valid(&input.file.lsof_receipt_sha256)
    {
        return Err(invalid("cutover_source_choice_required"));
    }
    bound(&input.file.database, 256)?;
    let actual = input
        .tx
        .path()
        .ok_or_else(|| invalid("cutover_frozen_path_required"))?;
    if Path::new(actual).canonicalize()? != input.file.frozen_file.canonicalize()?
        || !sql(input.tx.is_readonly(DatabaseName::Main))?
    {
        return Err(invalid("cutover_not_readonly_frozen_source"));
    }
    Ok(())
}
/// The native V3 writer stores context=collection, data={name,version,schema}.
/// Missing/ambiguous metadata is a finding, never MAX(version).
pub fn active_versions(
    tx: &Transaction<'_>,
    internal_table: &str,
) -> Result<BTreeMap<String, u32>> {
    let table = sql(crate::change_log::identifier(internal_table))?;
    let mut stmt = sql(tx.prepare(&format!("SELECT data FROM {table} WHERE deleted=0")))?;
    let mut cursor = sql(stmt.query([]))?;
    let mut versions = BTreeMap::new();
    while let Some(row) = sql(cursor.next())? {
        let text: String = sql(row.get(0))?;
        let body: Value =
            serde_json::from_str(&text).map_err(|_| invalid("cutover_internal_json"))?;
        if body["context"] != "collection" || body["_deleted"] == true {
            continue;
        }
        let name = body["data"]["name"]
            .as_str()
            .ok_or_else(|| invalid("cutover_internal_name"))?;
        let version = body["data"]["version"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| invalid("cutover_internal_version"))?;
        if body["data"]["schema"]["version"] != version {
            return Err(invalid("cutover_internal_schema_version"));
        }
        if versions
            .insert(name.into(), version)
            .is_some_and(|old| old != version)
        {
            return Err(invalid(format!("cutover_ambiguous_active_version:{name}")));
        }
    }
    Ok(versions)
}
struct Row {
    db: String,
    table: String,
    collection: String,
    version: u32,
    id: String,
    raw: Value,
    body: Value,
    revision: Option<String>,
    kind: SourceKind,
    authority: SourceKind,
    projection: bool,
}
/// Actual physical generations; core collection ownership is an explicit input.
/// The list is not a decision about which version is active.
pub fn rxdb_tables(tx: &Transaction<'_>, core_entities: &BTreeSet<String>) -> Result<Vec<Table>> {
    let mut stmt =
        sql(tx.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"))?;
    let names = sql(stmt.query_map([], |r| r.get::<_, String>(0)))?;
    let mut out = Vec::new();
    for name in names {
        let name = sql(name)?;
        let Some((collection, version)) = name
            .strip_prefix("ctox_business_os__")
            .and_then(|v| v.rsplit_once("__v"))
        else {
            continue;
        };
        if collection.trim_matches('_') == "rxdb_internal" {
            continue;
        }
        let version = version
            .parse()
            .map_err(|_| invalid("cutover_physical_version"))?;
        out.push(Table {
            name: name.clone(),
            collection: collection.into(),
            version,
            id_column: "id".into(),
            body_column: Some("data".into()),
            entity: if core_entities.contains(collection) {
                Entity::Core
            } else {
                Entity::App
            },
            mode: Mode::Rows,
        });
    }
    Ok(out)
}
/// Mixed-collection native mirrors can never become an authority by row order.
pub fn business_records_table() -> Table {
    Table {
        name: "business_records".into(),
        collection: String::new(),
        version: 0,
        id_column: "record_id".into(),
        body_column: Some("payload_json".into()),
        entity: Entity::App,
        mode: Mode::MirrorRows {
            collection_column: "collection".into(),
        },
    }
}

fn read_rows(
    input: &FrozenInput<'_, '_>,
    table: &Table,
    budget: &Budget,
    total: &mut (usize, usize),
) -> Result<Vec<Row>> {
    let name = sql(crate::change_log::identifier(&table.name))?;
    let mut stmt = sql(input.tx.prepare(&format!("SELECT * FROM {name}")))?;
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let mut cursor = sql(stmt.query([]))?;
    let mut rows = Vec::new();
    while let Some(row) = sql(cursor.next())? {
        let mut raw = serde_json::Map::new();
        for (idx, key) in names.iter().enumerate() {
            let value = match sql(row.get_ref(idx))? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(n) => json!(n),
                ValueRef::Real(n) => {
                    if n.is_finite() {
                        json!(n)
                    } else {
                        json!({"sqlite_nonfinite_real":n.to_string()})
                    }
                }
                ValueRef::Text(s) => match std::str::from_utf8(s) {
                    Ok(s) => Value::String(s.into()),
                    Err(_) => {
                        json!({"sqlite_text_hex":s.iter().map(|b|format!("{b:02x}")).collect::<String>()})
                    }
                },
                ValueRef::Blob(bytes) => {
                    json!({"sqlite_blob_hex":bytes.iter().map(|b|format!("{b:02x}")).collect::<String>()})
                }
            };
            raw.insert(key.clone(), value);
        }
        let raw = Value::Object(raw);
        total.0 += 1;
        total.1 = total.1.saturating_add(encoded(&raw).len());
        if total.0 > budget.rows || total.1 > budget.bytes {
            return Err(invalid("cutover_budget"));
        }
        let id = match &raw[&table.id_column] {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => return Err(invalid("cutover_source_id")),
        };
        bound(&id, 512)?;
        let collection = if let Mode::MirrorRows { collection_column } = &table.mode {
            raw[collection_column]
                .as_str()
                .ok_or_else(|| invalid("cutover_mirror_collection"))?
                .to_owned()
        } else {
            table.collection.clone()
        };
        let revision = raw["revision"]
            .as_str()
            .or_else(|| raw["rev"].as_str())
            .map(str::to_owned);
        let mut body = if raw["deleted"] == true || raw["deleted"] == 1 {
            json!({"id":id,"_deleted":true})
        } else if let Some(col) = &table.body_column {
            raw[col]
                .as_str()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(Value::Null)
        } else {
            raw.clone()
        };
        if let Mode::Projection { columns } = &table.mode {
            body = Value::Object(
                columns
                    .iter()
                    .map(|c| {
                        raw.get(c)
                            .cloned()
                            .map(|v| (c.clone(), v))
                            .ok_or_else(|| invalid(format!("cutover_projection_column:{c}")))
                    })
                    .collect::<Result<_>>()?,
            );
        }
        if body.is_object() && body.get("id").is_none() {
            body["id"] = id.clone().into();
        }
        let deleted = body["_deleted"] == true;
        rows.push(Row {
            db: input.file.database.clone(),
            table: table.name.clone(),
            collection,
            version: table.version,
            id,
            raw,
            body,
            revision,
            kind: if matches!(table.mode, Mode::MirrorRows { .. }) {
                SourceKind::Mirror
            } else {
                input.kind
            },
            authority: table.entity.authority(),
            projection: deleted || matches!(table.mode, Mode::Projection { .. }),
        });
    }
    Ok(rows)
}
fn archive(row: &Row, imported_at: u64) -> Result<Document> {
    bound(&row.table, 256)?;
    bound(&row.collection, 256)?;
    let id = sha(&json!([
        row.db,
        row.table,
        row.collection,
        row.version,
        row.id
    ]));
    Ok(Document {
        collection: ARCHIVE.into(),
        id: id.clone(),
        revision: None,
        body: json!({"id":id,"source_db":row.db,"source_table":row.table,"collection":row.collection,"collection_version":row.version,"record_id":row.id,"doc":row.raw,"doc_storage":"inline","sha256":sha(&row.raw),"imported_at":imported_at}),
    })
}
fn add(docs: &mut BTreeMap<(String, String), Document>, doc: Document) -> Result<()> {
    let key = (doc.collection.clone(), doc.id.clone());
    if let Some(old) = docs.get(&key) {
        if old.body != doc.body {
            return Err(invalid("cutover_duplicate_target"));
        }
    } else {
        docs.insert(key, doc);
    }
    Ok(())
}
/// Row equality is audited before target writes. This is an offline import, not
/// a running projection or permission shortcut. CoreKeep never reads contents.
pub fn collect(
    inputs: &[FrozenInput<'_, '_>],
    plan: &ReleasePlan,
    evaluator: &impl MigrationEvaluator,
    budget: &Budget,
    imported_at: u64,
) -> Result<CutoverImport> {
    if imported_at > 9_007_199_254_740_991 {
        return Err(invalid("cutover_import_time"));
    }
    let known: BTreeSet<&str> = plan
        .collections
        .values()
        .flatten()
        .map(|d| d.name.as_str())
        .collect();
    let mut audit = Audit::default();
    let mut rows = Vec::new();
    let mut total = (0, 0);
    let mut sources = BTreeSet::new();
    let mut active = BTreeMap::new();
    for input in inputs {
        validate_file(input)?;
        if !sources.insert(&input.file.database) {
            return Err(invalid("cutover_duplicate_source_choice"));
        }
        if input.kind == SourceKind::Rxdb {
            let metadata = input
                .internal_table
                .ok_or_else(|| invalid("cutover_internal_table_required"))?;
            for (name, version) in active_versions(input.tx, metadata)? {
                if active
                    .insert(name, version)
                    .is_some_and(|old| old != version)
                {
                    return Err(invalid("cutover_ambiguous_active_source"));
                }
            }
        }
    }
    for input in inputs {
        validate_file(input)?;
        if (input.file.database.contains("secret")
            || input
                .file
                .live_file
                .file_name()
                .is_some_and(|n| n.to_string_lossy().contains("secret")))
            && input
                .tables
                .iter()
                .any(|t| !matches!(t.mode, Mode::KeepCore))
        {
            return Err(invalid("cutover_secrets_require_store_api"));
        }
        for table in input.tables {
            if matches!(table.mode, Mode::KeepCore) {
                if input.kind != SourceKind::Core
                    || table.entity != Entity::Core
                    || known.contains(table.collection.as_str())
                {
                    return Err(invalid("cutover_retention_requires_unmapped_core"));
                }
                audit
                    .retained_core_tables
                    .push(format!("{}:{}", input.file.database, table.name));
                continue;
            } // E29-R6
            let part = read_rows(input, table, budget, &mut total)?;
            let key = format!("{}:{}", input.file.database, table.name);
            if audit.source_rows.insert(key.clone(), part.len()).is_some() {
                return Err(invalid("cutover_duplicate_source_table"));
            }
            let mut hashes = BTreeMap::new();
            for row in &part {
                if hashes
                    .insert(
                        json!([row.collection, row.version, row.id]).to_string(),
                        sha(&row.raw),
                    )
                    .is_some()
                {
                    return Err(invalid("cutover_duplicate_source_id"));
                }
            }
            audit.source_row_hashes.insert(key, hashes);
            rows.extend(part);
        }
    }
    // Only current authoritative rows may be the comparison oracle.
    let mut authorities = BTreeMap::new();
    for row in &rows {
        if row.body.is_object()
            && row.kind == row.authority
            && (row.authority == SourceKind::Core
                || active.get(&row.collection) == Some(&row.version))
        {
            let key = (row.collection.clone(), row.id.clone());
            if authorities.insert(key, &row.body).is_some() {
                return Err(invalid("cutover_multiple_authorities"));
            }
        }
    }
    let mut docs = BTreeMap::new();
    for row in &rows {
        if !row.body.is_object() {
            let doc = archive(row, imported_at)?;
            audit.findings.push(Finding {
                code: "invalid_source_body".into(),
                source_db: row.db.clone(),
                source_table: row.table.clone(),
                collection: row.collection.clone(),
                record_id: row.id.clone(),
                authority_sha256: None,
                mirror_sha256: sha(&row.raw),
                archive_id: doc.id.clone(),
            });
            add(&mut docs, doc)?;
            continue;
        }
        if row.kind != row.authority {
            // E29-R3
            let authority = authorities.get(&(row.collection.clone(), row.id.clone()));
            if authority.is_some_and(|body| *body == &row.body) {
                audit
                    .compared_equal
                    .push(format!("{}:{}:{}", row.db, row.table, row.id));
            } else {
                let doc = archive(row, imported_at)?;
                audit.findings.push(Finding {
                    code: if authority.is_some() {
                        "mirror_mismatch"
                    } else {
                        "authoritative_row_missing"
                    }
                    .into(),
                    source_db: row.db.clone(),
                    source_table: row.table.clone(),
                    collection: row.collection.clone(),
                    record_id: row.id.clone(),
                    authority_sha256: authority.map(|v| sha(v)),
                    mirror_sha256: sha(&row.body),
                    archive_id: doc.id.clone(),
                });
                add(&mut docs, doc)?;
            }
            continue;
        }
        let declared = known.contains(row.collection.as_str()); // E29-R1
        if !declared {
            add(&mut docs, archive(row, imported_at)?)?;
            continue;
        } // E29-R2
        if row.authority == SourceKind::Rxdb {
            let current = active.get(&row.collection).ok_or_else(|| {
                invalid(format!(
                    "cutover_active_metadata_missing:{}",
                    row.collection
                ))
            })?;
            if row.version != *current {
                add(&mut docs, archive(row, imported_at)?)?;
                continue;
            } // E29-R4
        }
        let mut body = row.body.clone();
        if let Some(spec) = plan.migrations.get(&row.collection) {
            body = evaluator.migrate(&row.collection, spec, body)?;
        }
        let id = body["id"]
            .as_str()
            .ok_or_else(|| invalid("cutover_target_id"))?
            .into();
        add(
            &mut docs,
            Document {
                collection: row.collection.clone(),
                id,
                body,
                revision: row.revision.clone(),
            },
        )?;
        if row.projection {
            add(&mut docs, archive(row, imported_at)?)?;
        } // E29-R5
    }
    let mut report = ImportReport::default();
    report.physical_source_rows = audit.source_rows.clone();
    for doc in docs.values() {
        report
            .collections
            .entry(doc.collection.clone())
            .or_default()
            .source_rows += 1;
        report
            .content_hashes
            .entry(doc.collection.clone())
            .or_default()
            .insert(
                doc.id.clone(),
                ContentHash {
                    source_sha256: document_hash(&doc.collection, &doc.body)?,
                    target_sha256: None,
                },
            );
    }
    Ok(CutoverImport {
        snapshot: core_import::CoreImport {
            documents: docs.into_values().collect(),
            dataframe_rows: vec![],
            report,
        },
        audit,
    })
}
/// Archive payloads use the exact E29 schema, not a new blob transport. Marker
/// doc_storage disambiguates a real old document shaped like a BlobRef.
pub async fn prepare_files(
    mut snapshot: core_import::CoreImport,
    cas: &crate::cas::Cas,
    scratch: &Path,
    archive_custodian_user: &str,
) -> Result<core_import::CoreImport> {
    bound(archive_custodian_user, 128)?;
    let mut files = Vec::new();
    for doc in snapshot
        .documents
        .iter_mut()
        .filter(|d| d.collection == ARCHIVE)
    {
        if doc.body["doc_storage"] == "blob" {
            continue;
        }
        let bytes = encoded(&doc.body["doc"]);
        if bytes.len() <= INLINE {
            continue;
        }
        let hash = format!("{:x}", Sha256::digest(&bytes));
        let size = bytes.len();
        files.push(file_import::ByteFile {
            id: format!("legacy:{}", doc.id),
            name: format!("{}.json", doc.id),
            mime: "application/json".into(),
            owner_app: "ctox".into(),
            owner_user: Some(archive_custodian_user.into()),
            bytes,
        });
        doc.body["doc"] = json!({"blob_ref":hash,"size":size,"sha256":hash});
        doc.body["doc_storage"] = "blob".into();
    }
    let prepared = file_import::prepare_bytes(files, cas, scratch).await?;
    let mut prepared = prepared;
    let count = prepared.documents.len();
    prepared.source_rows = BTreeMap::from([("legacy_record_blobs".into(), count)]);
    file_import::attach(snapshot, prepared)
}
pub(crate) fn document_hash(collection: &str, body: &Value) -> Result<String> {
    if collection != ARCHIVE {
        return Ok(sha(body));
    }
    let mut canonical = body.clone();
    let (hash, size) = if body["doc_storage"] == "blob" {
        let blob = &body["doc"];
        let hash = blob["sha256"]
            .as_str()
            .ok_or_else(|| invalid("legacy_blob_hash"))?;
        let size = blob["size"]
            .as_u64()
            .ok_or_else(|| invalid("legacy_blob_size"))?;
        if !digest_valid(hash)
            || blob["blob_ref"] != hash
            || size <= INLINE as u64
            || blob.as_object().is_none_or(|v| v.len() != 3)
        {
            return Err(invalid("legacy_blob_reference"));
        }
        (hash.into(), size)
    } else {
        let bytes = encoded(&body["doc"]);
        (format!("{:x}", Sha256::digest(&bytes)), bytes.len() as u64)
    };
    if body["sha256"] != hash {
        return Err(invalid("legacy_content_hash"));
    }
    canonical["doc"] = json!({"sha256":hash,"size":size});
    canonical.as_object_mut().unwrap().remove("doc_storage");
    Ok(sha(&canonical))
}
pub(crate) fn persisted_hash(tx: &Transaction<'_>, doc: &Document) -> Result<String> {
    if doc.collection == ARCHIVE
        && doc.body["doc_storage"] != "blob"
        && encoded(&doc.body["doc"]).len() > INLINE
    {
        return Err(invalid("legacy_blob_not_prepared"));
    }
    let active = sql(crate::schema::active_collection(tx, &doc.collection))?
        .ok_or_else(|| invalid("cutover_target_schema_missing"))?;
    if doc.collection == ARCHIVE && active.definition.app_id != "ctox" {
        return Err(invalid("legacy_owner_mismatch"));
    }
    let table = sql(crate::change_log::identifier(&active.table))?;
    let stored: Option<(Option<String>, bool)> = sql(tx
        .query_row(
            &format!("SELECT data,deleted FROM {table} WHERE id=?1"),
            [&doc.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional())?;
    let (data, deleted) = stored.ok_or_else(|| invalid("cutover_target_missing"))?;
    let body = if deleted {
        json!({"id":doc.id,"_deleted":true})
    } else {
        serde_json::from_str(&data.ok_or_else(|| invalid("cutover_target_missing"))?)
            .map_err(|_| invalid("cutover_target_json"))?
    };
    document_hash(&doc.collection, &body)
}
