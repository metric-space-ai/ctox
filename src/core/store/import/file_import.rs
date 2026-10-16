//! Offline legacy files -> existing Transfer-CAS, then canonical owner metadata.
//! No source mutation, DDL, alternate blob store or independent sequence.
use super::*;
use crate::cas::Cas;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

#[derive(Clone)]
/// Owner scope is supplied from the frozen profile authority, never guessed.
/// None is explicit app-shared source state, not an inferred default grant.
pub struct LegacyCollection {
    pub collection: String,
    pub table: String,
    pub owner_app: String,
    pub owner_user: Option<String>,
}
pub struct SnapshotFile {
    pub relative_path: PathBuf,
    pub owner_app: String,
    pub owner_user: Option<String>,
    pub created_at_ms: i64,
}
pub struct ProfilePaths {
    pub original_root: PathBuf,
    pub copied_root: PathBuf,
}
pub struct Budget {
    pub rows: usize,
    pub bytes: usize,
}
enum Content {
    Bytes(Vec<u8>),
    File(PathBuf),
    Directory,
}
struct OwnedStaging(PathBuf);
impl Drop for OwnedStaging {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
struct Pending {
    id: String,
    name: String,
    mime: String,
    owner_app: String,
    owner_user: Option<String>,
    created_at_ms: i64,
    expected: Option<String>,
    content: Content,
}
pub struct FilePlan {
    pending: Vec<Pending>,
    /// Physical chunk rows are reported separately from logical source files.
    pub source_rows: BTreeMap<String, usize>,
}
pub struct PreparedFiles {
    pub documents: Vec<Document>,
    pub source_rows: BTreeMap<String, usize>,
    pub references: BTreeMap<String, String>,
}
pub(crate) struct ByteFile {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub owner_app: String,
    pub owner_user: Option<String>,
    pub bytes: Vec<u8>,
}
/// Staging adapter for E28 fields. The existing file/CAS importer owns bytes.
pub(crate) async fn prepare_bytes(
    files: Vec<ByteFile>,
    cas: &Cas,
    scratch: &Path,
) -> Result<PreparedFiles> {
    let count = files.len();
    let pending = files
        .into_iter()
        .map(|f| Pending {
            id: f.id,
            name: f.name,
            mime: f.mime,
            owner_app: f.owner_app,
            owner_user: f.owner_user,
            created_at_ms: 0,
            expected: Some(format!("{:x}", Sha256::digest(&f.bytes))),
            content: Content::Bytes(f.bytes),
        })
        .collect();
    prepare(
        FilePlan {
            pending,
            source_rows: BTreeMap::from([("jour_fixe_fields".into(), count)]),
        },
        cas,
        scratch,
    )
    .await
}
/// Attach metadata to the ordinary owner import after bytes are in CAS.
pub fn attach(
    mut snapshot: super::core_import::CoreImport,
    prepared: PreparedFiles,
) -> Result<super::core_import::CoreImport> {
    for (name, count) in prepared.source_rows {
        *snapshot
            .report
            .physical_source_rows
            .entry(name)
            .or_default() += count;
    }
    snapshot
        .report
        .collections
        .entry("files".into())
        .or_default()
        .source_rows += prepared.documents.len();
    let mut ids: std::collections::BTreeSet<_> = snapshot
        .documents
        .iter()
        .filter(|d| d.collection == "files")
        .map(|d| d.id.clone())
        .collect();
    for doc in prepared.documents {
        if !ids.insert(doc.id.clone()) {
            return Err(invalid("file_import_duplicate_identity"));
        }
        snapshot.documents.push(doc);
    }
    Ok(snapshot)
}
/// Stable source mapping for consumers replacing legacy blob_id references.
pub fn blob_file_id(collection: &str, blob: &str) -> String {
    format!(
        "blob:{:x}",
        Sha256::digest(format!("{collection}:{blob}").as_bytes())
    )
}
fn required<'a>(v: &'a Value, k: &str) -> Result<&'a str> {
    v[k].as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid(format!("file_import_required:{k}")))
}
fn source_owner(row: &Value, source: &LegacyCollection) -> Option<String> {
    row["owner_user"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| row["owner_id"].as_str().filter(|s| !s.trim().is_empty()))
        .map(str::to_owned)
        .or_else(|| source.owner_user.clone())
}
fn copied_path(paths: &ProfilePaths, path: &Path) -> Result<PathBuf> {
    let rel = if path.is_absolute() {
        path.strip_prefix(&paths.original_root)
            .map_err(|_| invalid("file_outside_profile"))?
    } else {
        path
    };
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(invalid("file_unsafe_path"));
    }
    let root = paths.copied_root.canonicalize()?;
    let mut target = root;
    for part in rel.components() {
        target.push(part);
        if fs::symlink_metadata(&target)?.file_type().is_symlink() {
            return Err(invalid("file_source_symlink"));
        }
    }
    if !fs::metadata(&target)?.is_file() {
        return Err(invalid("file_source_not_regular"));
    }
    Ok(target)
}
/// Strict RFC4648 decoder: legacy chunks are standard padded base64, never JSON.
fn decode(encoded: &str) -> Result<Vec<u8>> {
    if encoded.len() % 4 != 0 {
        return Err(invalid("file_chunk_base64"));
    }
    let mut out = Vec::new();
    let groups = encoded.as_bytes().chunks_exact(4);
    let len = groups.len();
    for (i, g) in groups.enumerate() {
        let mut n = [0u8; 4];
        let mut padding = 0;
        for j in 0..4 {
            n[j] = match g[j] {
                b'A'..=b'Z' => g[j] - b'A',
                b'a'..=b'z' => g[j] - b'a' + 26,
                b'0'..=b'9' => g[j] - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                b'=' if j >= 2 && i + 1 == len => {
                    padding += 1;
                    0
                }
                _ => return Err(invalid("file_chunk_base64")),
            };
        }
        if g[2] == b'=' && g[3] != b'='
            || padding == 2 && n[1] & 15 != 0
            || padding == 1 && n[2] & 3 != 0
        {
            return Err(invalid("file_chunk_base64"));
        }
        out.push(n[0] << 2 | n[1] >> 4);
        if padding < 2 {
            out.push(n[1] << 4 | n[2] >> 2);
        }
        if padding == 0 {
            out.push(n[2] << 6 | n[3]);
        }
    }
    Ok(out)
}
/// Reads only explicitly selected frozen collections; budgets cover raw JSON and
/// decoded content. Desktop folders retain their ID as metadata without a blob.
pub fn collect(
    tx: &Transaction<'_>,
    sources: &[LegacyCollection],
    snapshots: &[SnapshotFile],
    paths: &ProfilePaths,
    budget: &Budget,
) -> Result<FilePlan> {
    let mut plan = FilePlan {
        pending: vec![],
        source_rows: BTreeMap::new(),
    };
    let mut seen_rows = 0usize;
    let mut seen_bytes = 0usize;
    for source in sources {
        if source.owner_app.is_empty()
            || !(source.collection == "desktop_files"
                || source.collection.ends_with("_blob_chunks"))
        {
            return Err(invalid("file_collection_not_allowed"));
        }
        let table = sql(crate::change_log::identifier(&source.table))?;
        let mut stmt = sql(tx.prepare(&format!(
            "SELECT data FROM {table} WHERE deleted=0 ORDER BY id LIMIT ?1"
        )))?;
        let mut cursor = sql(stmt.query([budget.rows.saturating_add(1) as i64]))?;
        let mut groups: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        while let Some(row) = sql(cursor.next())? {
            let raw: String = sql(row.get(0))?;
            seen_rows += 1;
            seen_bytes = seen_bytes.saturating_add(raw.len());
            if seen_rows > budget.rows || seen_bytes > budget.bytes {
                return Err(invalid("file_import_budget"));
            }
            let value: Value =
                serde_json::from_str(&raw).map_err(|_| invalid("file_source_json"))?;
            *plan
                .source_rows
                .entry(source.collection.clone())
                .or_default() += 1;
            if source.collection == "desktop_files" {
                if value["kind"].as_str() == Some("folder") {
                    plan.pending.push(Pending {
                        id: required(&value, "id")?.into(),
                        name: required(&value, "name")?.into(),
                        mime: "inode/directory".into(),
                        owner_app: source.owner_app.clone(),
                        owner_user: source_owner(&value, source),
                        created_at_ms: value["created_at_ms"].as_i64().unwrap_or(0),
                        expected: None,
                        content: Content::Directory,
                    });
                    continue;
                }
                let path = value["local_path"]
                    .as_str()
                    .or_else(|| value["path"].as_str())
                    .ok_or_else(|| invalid("desktop_file_path_missing"))?;
                plan.pending.push(Pending {
                    id: required(&value, "id")?.into(),
                    name: required(&value, "name")?.into(),
                    mime: value["mime_type"]
                        .as_str()
                        .unwrap_or("application/octet-stream")
                        .into(),
                    owner_app: source.owner_app.clone(),
                    owner_user: source_owner(&value, source),
                    created_at_ms: value["created_at_ms"].as_i64().unwrap_or(0),
                    expected: value["content_hash"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned),
                    content: Content::File(copied_path(paths, Path::new(path))?),
                });
            } else {
                groups
                    .entry(required(&value, "blob_id")?.into())
                    .or_default()
                    .push(value);
            }
        }
        for (blob, mut chunks) in groups {
            chunks.sort_by_key(|c| c["idx"].as_u64().unwrap_or(u64::MAX));
            let first = &chunks[0];
            let total = first["total"]
                .as_u64()
                .ok_or_else(|| invalid("file_chunk_total"))?;
            if total == 0 || total != chunks.len() as u64 {
                return Err(invalid("file_chunk_incomplete"));
            }
            let mut bytes = Vec::new();
            for (idx, chunk) in chunks.iter().enumerate() {
                if chunk["idx"].as_u64() != Some(idx as u64)
                    || chunk["total"].as_u64() != Some(total)
                    || required(chunk, "encoding")? != "base64"
                {
                    return Err(invalid("file_chunk_sequence"));
                }
                let data = decode(
                    chunk["data"]
                        .as_str()
                        .ok_or_else(|| invalid("file_chunk_data"))?,
                )?;
                if bytes.len().saturating_add(data.len()) > budget.bytes {
                    return Err(invalid("file_import_budget"));
                }
                bytes.extend(data);
            }
            plan.pending.push(Pending {
                id: blob_file_id(&source.collection, &blob),
                name: blob,
                mime: first["mime_type"]
                    .as_str()
                    .unwrap_or("application/octet-stream")
                    .into(),
                owner_app: source.owner_app.clone(),
                owner_user: source_owner(first, source),
                created_at_ms: first["created_at_ms"].as_i64().unwrap_or(0),
                expected: None,
                content: Content::Bytes(bytes),
            });
        }
    }
    for snapshot in snapshots {
        seen_rows += 1;
        if seen_rows > budget.rows {
            return Err(invalid("file_import_budget"));
        }
        let rel = snapshot
            .relative_path
            .to_str()
            .ok_or_else(|| invalid("snapshot_path_utf8"))?;
        let path = copied_path(paths, &snapshot.relative_path)?;
        *plan.source_rows.entry("snapshot_files".into()).or_default() += 1;
        plan.pending.push(Pending {
            id: format!(
                "snapshot:{:x}",
                Sha256::digest(format!("{}:{rel}", snapshot.owner_app).as_bytes())
            ),
            name: path.file_name().unwrap().to_string_lossy().into_owned(),
            mime: "application/octet-stream".into(),
            owner_app: snapshot.owner_app.clone(),
            owner_user: snapshot.owner_user.clone(),
            created_at_ms: snapshot.created_at_ms,
            expected: None,
            content: Content::File(path),
        });
    }
    let mut identities = std::collections::BTreeSet::new();
    let mut bytes = 0u64;
    for p in &plan.pending {
        if p.id.len() > 180 || p.owner_app.is_empty() || !identities.insert(&p.id) {
            return Err(invalid("file_import_identity"));
        }
        bytes = bytes
            .checked_add(match &p.content {
                Content::Bytes(b) => b.len() as u64,
                Content::File(f) => fs::metadata(f)?.len(),
                Content::Directory => 0,
            })
            .ok_or_else(|| invalid("file_import_budget"))?;
        if bytes > budget.bytes as u64 {
            return Err(invalid("file_import_budget"));
        }
    }
    Ok(plan)
}
/// Run outside the source read transaction and the destination writer. Import
/// bytes with the EXISTING CAS; metadata is committed later by write_page.
pub async fn prepare(plan: FilePlan, cas: &Cas, scratch: &Path) -> Result<PreparedFiles> {
    let mut output = PreparedFiles {
        documents: vec![],
        source_rows: plan.source_rows,
        references: BTreeMap::new(),
    };
    for p in plan.pending {
        if matches!(&p.content, Content::Directory) {
            let mut body = json!({"id":p.id,"name":p.name,"mime":p.mime,"size":0,"owner_app":p.owner_app,"created_at_ms":p.created_at_ms});
            if let Some(owner) = p.owner_user {
                body["owner_user"] = owner.into();
            }
            output.documents.push(Document {
                collection: "files".into(),
                id: p.id,
                body,
                revision: None,
            });
            continue;
        }
        let temporary = match &p.content {
            Content::Bytes(bytes) => {
                let path = scratch.join(format!("import-{}", uuid::Uuid::new_v4()));
                let mut options = fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut file = options.open(&path)?;
                let owned = OwnedStaging(path);
                file.write_all(bytes)?;
                Some(owned)
            }
            Content::File(_) => None,
            Content::Directory => unreachable!("handled metadata-only folder"),
        };
        let path = match &p.content {
            Content::File(path) => path.clone(),
            Content::Bytes(_) => temporary.as_ref().unwrap().0.clone(),
            Content::Directory => unreachable!("handled metadata-only folder"),
        };
        let before = fs::metadata(&path)?;
        let imported = cas
            .import(
                path.clone(),
                uuid::Uuid::new_v4().simple().to_string().repeat(2),
            )
            .await;
        let unchanged = fs::metadata(&path)
            .map(|m| m.len() == before.len() && m.modified().ok() == before.modified().ok())
            .unwrap_or(false);
        drop(temporary);
        let blob = imported?;
        if !unchanged
            || p.expected
                .as_ref()
                .is_some_and(|expected| expected != blob.reference())
        {
            return Err(invalid("file_import_content_changed"));
        }
        output
            .references
            .insert(p.id.clone(), blob.reference().into());
        let mut body = json!({"id":p.id,"name":p.name,"mime":p.mime,"size":blob.size(),"ref":blob.reference(),"owner_app":p.owner_app,"created_at_ms":p.created_at_ms});
        if let Some(owner) = p.owner_user {
            body["owner_user"] = owner.into();
        }
        output.documents.push(Document {
            collection: "files".into(),
            id: p.id,
            body,
            revision: None,
        });
    }
    Ok(output)
}
