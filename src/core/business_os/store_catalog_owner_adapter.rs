//! Owner-only catalog projection. The daemon supplies its existing Sync writer.
//! No file opens, runtime, polling, private counter or fallback owner lives here.
use ctox_store::{
    change_log,
    rxdb_schema_adapter::{self, CanonicalChangeLog, RxdbTable},
    schema,
    writer::WriterHandle,
    DbId, JobHandle, JobOptions, StoreError, WriteJob,
};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    sync::{Arc, Mutex, OnceLock, Weak},
};

static OWNERS: OnceLock<Mutex<HashMap<DbId, Weak<CatalogOwner>>>> = OnceLock::new();
pub const LEGACY_TABLE: &str = "ctox_business_os__business_module_catalog__v0";

pub struct CatalogOwner {
    writer: WriterHandle,
    base_app_version: String,
}
impl CatalogOwner {
    /// P3-15 retains this registration for the daemon lifetime. Different writer
    /// incarnations cannot replace a live registration, including on restart.
    pub fn register(
        writer: WriterHandle,
        base_app_version: String,
    ) -> Result<Arc<Self>, StoreError> {
        if base_app_version.is_empty() {
            return Err(StoreError::Invalid(
                "base-release app version is required".into(),
            ));
        }
        let mut owners = OWNERS
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let id = writer.db_id().clone();
        if let Some(current) = owners.get(&id).and_then(Weak::upgrade) {
            if current.writer.generation() != writer.generation()
                || current.base_app_version != base_app_version
            {
                return Err(StoreError::AlreadyOwned);
            }
            return Ok(current);
        }
        let owner = Arc::new(Self {
            writer,
            base_app_version,
        });
        owners.insert(id, Arc::downgrade(&owner));
        Ok(owner)
    }
    pub fn for_path(path: &Path) -> Result<Arc<Self>, StoreError> {
        let id = DbId::new(path)?;
        OWNERS
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&id)
            .and_then(Weak::upgrade)
            .ok_or_else(|| {
                StoreError::Invalid(
                    "Sync writer is not registered; P3-15 must retain CatalogOwner::register"
                        .into(),
                )
            })
    }
    pub fn submit<T: Send + 'static>(
        &self,
        write: impl FnOnce(&Transaction<'_>) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<JobHandle<T>, StoreError> {
        self.writer
            .submit(WriteJob::new(JobOptions::default(), write))
    }
    pub fn base_app_version(&self) -> &str {
        &self.base_app_version
    }
    pub fn project(&self, catalog: Value, now: u64) -> Result<JobHandle<()>, StoreError> {
        let app_version = self.base_app_version.clone();
        self.submit(move |tx| project(tx, &catalog, now, &app_version))
    }
}

pub fn install_legacy(
    tx: &Transaction<'_>,
    collection: &str,
    table: &str,
    app_version: &str,
) -> Result<(), StoreError> {
    rxdb_schema_adapter::install(
        tx,
        &RxdbTable {
            collection: collection.into(),
            generation: 1,
            table: table.into(),
            schema_version: 0,
            app_version: app_version.into(),
        },
        &CanonicalChangeLog,
    )
}

fn ensure_tables(
    tx: &Transaction<'_>,
    app_version: &str,
) -> Result<schema::Collection, StoreError> {
    // P3-06 imports the platform manifest and owns schema/generation/rights.
    // Never create a table behind the registry or invent a generation here.
    schema::initialize(tx)?;
    let apps = schema::active_collection(tx, "business_apps")?.ok_or_else(|| {
        StoreError::Invalid("platform manifest business_apps is not imported".into())
    })?;
    if apps.definition.app_id != "ctox-platform" {
        return Err(StoreError::Invalid(
            "business_apps is not platform owned".into(),
        ));
    }
    tx.execute_batch(&format!("CREATE TABLE IF NOT EXISTS {LEGACY_TABLE} (id TEXT PRIMARY KEY,revision TEXT,deleted INTEGER NOT NULL,lastWriteTime REAL NOT NULL,data TEXT NOT NULL);"))?;
    install_legacy(tx, "business_module_catalog", LEGACY_TABLE, app_version)?;
    Ok(apps)
}

/// Only declared platform-manifest fields. Do not invent an active app version:
/// the registry publisher owns active_ver/migrated_at_seq and we preserve them.
fn app_documents(catalog: &Value) -> Result<BTreeMap<String, Value>, StoreError> {
    let mut result = BTreeMap::new();
    let modules = catalog
        .get("modules")
        .and_then(Value::as_array)
        .ok_or_else(|| StoreError::Invalid("catalog.modules must be an array".into()))?;
    for module in modules {
        let id = module
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty() && s.len() <= 180)
            .ok_or_else(|| StoreError::Invalid("invalid app identity".into()))?;
        let mut doc = json!({"id":id,"app_id":id,"title":module.get("title").and_then(Value::as_str).unwrap_or(id)});
        for key in [
            "visible_roles",
            "layout",
            "provenance",
            "locally_modified",
            "state",
        ] {
            if let Some(value) = module.get(key).filter(|value| match key {
                "visible_roles" => value
                    .as_array()
                    .is_some_and(|roles| roles.iter().all(Value::is_string)),
                "layout" | "provenance" => value.is_object(),
                "locally_modified" => value.is_boolean(),
                "state" => value.is_string(),
                _ => false,
            }) {
                doc[key] = value.clone();
            }
        }
        // No visibility fallback grants: rights remain resolved by the registry.
        if result.insert(id.into(), doc).is_some() {
            return Err(StoreError::Invalid("duplicate app identity".into()));
        }
    }
    Ok(result)
}

pub fn project(
    tx: &Transaction<'_>,
    catalog: &Value,
    now: u64,
    app_version: &str,
) -> Result<(), StoreError> {
    let docs = app_documents(catalog)?;
    let apps = ensure_tables(tx, app_version)?;
    let table = format!("\"{}\"", apps.table.replace('"', "\"\""));
    for (id, mut doc) in docs {
        let old: Option<(String, Option<String>, i64)> = tx
            .query_row(
                &format!("SELECT rev,data,deleted FROM {table} WHERE id=?1"),
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some((_, Some(body), 0)) = &old {
            let mut previous: Value =
                serde_json::from_str(body).map_err(|e| StoreError::Invalid(e.to_string()))?;
            for key in [
                "active_ver",
                "migrated_at_seq",
                "visible_roles",
                "layout",
                "provenance",
                "locally_modified",
                "state",
            ] {
                if doc.get(key).is_none() {
                    if let Some(value) = previous.get(key) {
                        doc[key] = value.clone();
                    }
                }
            }
            previous
                .as_object_mut()
                .ok_or_else(|| StoreError::Invalid("app is not an object".into()))?
                .remove("updated_at_ms");
            if previous == doc {
                continue;
            } // unchanged apps keep body, revision and seq
        }
        let rev = format!(
            "{}-catalog",
            old.as_ref()
                .and_then(|(rev, _, _)| rev.split('-').next()?.parse::<u64>().ok())
                .unwrap_or(0)
                + 1
        );
        doc["updated_at_ms"] = now.into();
        tx.execute(&format!("INSERT INTO {table}(id,rev,schema_ver,data,deleted) VALUES(?1,?2,?4,?3,0) ON CONFLICT(id) DO UPDATE SET rev=excluded.rev,schema_ver=excluded.schema_ver,data=excluded.data,deleted=0"), params![id,rev,doc.to_string(),apps.definition.schema_ver])?;
    }
    // Transitional V3 reader projection, never the V4 catalog source.
    let mut legacy = catalog.clone();
    let revision = format!("{now}-ctox-module-catalog");
    legacy["_rev"] = revision.clone().into();
    legacy["_meta"] = json!({"lwt": now as f64});
    legacy["_attachments"] = json!({});
    legacy["_deleted"] = false.into();
    legacy["is_deleted"] = false.into();
    tx.execute(&format!("INSERT INTO {LEGACY_TABLE}(id,revision,deleted,lastWriteTime,data) VALUES('module-catalog',?1,0,?2,?3) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,deleted=0,lastWriteTime=excluded.lastWriteTime,data=excluded.data"), params![revision,now as f64,legacy.to_string()])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn real_sink_preserves_schema_zero_and_emits_hard_delete() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let writer = Registry.open(
            DbId::new(dir.path().join("sink.sqlite3"))?,
            WriterOptions::default(),
        )?;
        writer.ready().await?;
        writer
            .submit(WriteJob::new(JobOptions::default(), |tx| {
                tx.execute_batch(
                "CREATE TABLE rx_docs(id TEXT PRIMARY KEY,revision TEXT,deleted INTEGER,data TEXT)",
            )?;
                let binding = RxdbTable {
                    collection: "docs".into(),
                    table: "rx_docs".into(),
                    generation: 1,
                    schema_version: 0,
                    app_version: "base".into(),
                };
                tx.execute(
                    "INSERT INTO rx_docs VALUES('doc','1-r',0,'{\"id\":\"doc\"}')",
                    [],
                )?;
                rxdb_schema_adapter::install(tx, &binding, &CanonicalChangeLog)?;
                assert!(rxdb_schema_adapter::backfill(tx, &binding.table, None)?.is_none());
                let inserted = change_log::read_page(tx, "docs", 1, 0, 10)?;
                assert_eq!(inserted.docs[0].schema_ver, 0);
                assert_eq!(inserted.docs[0].rev, "1-r");
                tx.execute("DELETE FROM rx_docs WHERE id='doc'", [])?;
                let removed = change_log::read_page(tx, "docs", 1, inserted.cursor_to, 10)?;
                assert_eq!(removed.docs[0].op, change_log::Operation::Delete);
                assert!(removed.docs[0].seq > inserted.cursor_to);
                assert!(removed.docs[0].body.is_none());
                Ok(())
            }))?
            .await?;
        writer.shutdown().await?;
        Ok(())
    }
    use ctox_store::{registry::Registry, writer::WriterOptions};
    #[tokio::test]
    async fn changing_one_app_preserves_other_apps_revision_body_and_sequence() -> anyhow::Result<()>
    {
        let dir = tempfile::tempdir()?;
        let writer = Registry.open(
            DbId::new(dir.path().join("sync.sqlite3"))?,
            WriterOptions::default(),
        )?;
        writer.ready().await?;
        writer.submit(WriteJob::new(JobOptions::default(), |tx| {
            schema::create_collection(tx, &schema::CollectionDefinition {
                name: "business_apps".into(), app_id: "ctox-platform".into(), schema_ver: 0,
                schema_json: json!({"primaryKey":"id","type":"object","properties":{"id":{"type":"string"},"app_id":{"type":"string"},"state":{"type":"string"}},"indexes":["app_id","state"]}),
                class: "full".into(), client_limit: 0, byte_budget: 16384, rights_ver: 1, app_ver: "platform-base-release".into()
            })?;
            Ok(())
        }))?.await?;
        let owner = CatalogOwner::register(writer.clone(), "platform-base-release".into())?;
        owner
            .project(
                json!({"modules":[{"id":"a","title":"A"},{"id":"b","title":"B"}]}),
                10,
            )?
            .await?;
        let before = writer
            .submit(WriteJob::new(JobOptions::default(), |tx| {
                Ok((
                    tx.query_row(
                        "SELECT rev,data FROM c_business_apps WHERE id='b'",
                        [],
                        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                    )?,
                    change_log::head(tx)?.head_seq,
                ))
            }))?
            .await?;
        owner
            .project(
                json!({"modules":[{"id":"a","title":"New A"},{"id":"b","title":"B"}]}),
                20,
            )?
            .await?;
        writer.submit(WriteJob::new(JobOptions::default(), move |tx| {
            let after: (String,String) = tx.query_row("SELECT rev,data FROM c_business_apps WHERE id='b'", [], |r| Ok((r.get(0)?,r.get(1)?)))?;
            assert_eq!(before.0, after, "unrelated app must remain byte-identical");
            let page = change_log::read_page(tx,"business_apps",1,before.1,100)?;
            assert_eq!(page.docs.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), vec!["a"]);
            assert_eq!(page.docs[0].body.as_ref().unwrap()["title"], "New A");
            let legacy: String = tx.query_row("SELECT data FROM ctox_business_os__business_module_catalog__v0 WHERE id='module-catalog'",[],|r|r.get(0))?;
            assert_eq!(serde_json::from_str::<Value>(&legacy).unwrap()["modules"][0]["title"], "New A");
            Ok(())
        }))?.await?;
        drop(owner);
        writer.shutdown().await?;
        Ok(())
    }
}
