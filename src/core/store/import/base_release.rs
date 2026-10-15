//! Immutable A9 input -> reserved registry publication -> activated generations.
//! No direct DDL and no separate sequence allocator.
use super::*;
use crate::{
    change_log,
    schema::{self, CollectionDefinition, MigrationOutput, PublishLock, SchemaChange},
};
use std::{collections::BTreeMap, path::Path};

pub const FROM_VERSION: &str = "main@4794af03d";
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReleasePlan {
    pub version: String,
    pub manifests: Vec<Value>,
    pub collections: BTreeMap<String, Vec<CollectionDefinition>>,
    pub migrations: BTreeMap<String, Value>,
    pub removed_sync_settings: Vec<String>,
    pub field_merge_core_rule: Vec<String>,
}

fn strip(value: &mut Value, path: &str, removed: &mut Vec<String>, merges: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for key in ["syncProfile", "conflictStrategy"] {
                if let Some(old) = map.remove(key) {
                    removed.push(format!("{path}.{key}"));
                    if old == "field-merge" {
                        merges.push(path.to_owned());
                    }
                }
            }
            for (key, value) in map {
                strip(value, &format!("{path}.{key}"), removed, merges);
            }
        }
        Value::Array(values) => {
            for (i, value) in values.iter_mut().enumerate() {
                strip(value, &format!("{path}[{i}]"), removed, merges);
            }
        }
        _ => (),
    }
}

/// Budgets are explicit operator policy until manifests contain measured values.
/// Uses the registry's canonical loader, including platform ownership overrides.
pub fn load(
    v4_root: &Path,
    platform_manifest: &Path,
    defaults: &schema::BaseReleaseDefaults,
) -> Result<ReleasePlan> {
    let mut input = sql(schema::load_base_release(v4_root, platform_manifest))?;
    let mut removed = Vec::new();
    let mut merges = Vec::new();
    for manifest in &mut input.manifests {
        let app = manifest["app_id"]
            .as_str()
            .ok_or_else(|| invalid("manifest_app_id"))?
            .to_owned();
        strip(manifest, &app, &mut removed, &mut merges);
    }
    for (app, schema) in &mut input.schemas {
        strip(schema, app, &mut removed, &mut merges);
    }
    let ownership = input.ownership.get("ownership").unwrap_or(&input.ownership);
    let mut candidates: BTreeMap<String, Vec<(String, Value)>> = BTreeMap::new();
    let mut migrations = BTreeMap::new();
    for manifest in &input.manifests {
        let app = manifest["app_id"]
            .as_str()
            .ok_or_else(|| invalid("manifest_app_id"))?;
        let definitions = manifest["collections"]
            .as_object()
            .ok_or_else(|| invalid("manifest_collections"))?;
        for (name, metadata) in definitions {
            candidates
                .entry(name.clone())
                .or_default()
                .push((app.into(), metadata.clone()));
        }
        if let Some(schema) = input.schemas.get(app) {
            if schema["_base_release"] != FROM_VERSION {
                return Err(invalid(format!("wrong_base_release:{app}")));
            }
            if let Some(map) = schema["migrations"].as_object() {
                for (name, migration) in map {
                    if migration["from_ver"] != FROM_VERSION {
                        return Err(invalid(format!("wrong_migration_base:{name}")));
                    }
                    migrations.insert(name.clone(), migration.clone());
                }
            }
        }
    }
    let mut collections: BTreeMap<String, Vec<CollectionDefinition>> = BTreeMap::new();
    for manifest in &input.manifests {
        collections
            .entry(
                manifest["app_id"]
                    .as_str()
                    .ok_or_else(|| invalid("manifest_app_id"))?
                    .to_owned(),
            )
            .or_default();
    }
    for (name, choices) in candidates {
        if name == "business_module_catalog" {
            continue;
        }
        let owner = if choices.iter().any(|(a, _)| a == "ctox-platform") {
            "ctox-platform".to_owned()
        } else if [
            "outbound_lead_generation_adapters",
            "sellify_companies",
            "sellify_people",
            "sellify_campaigns",
            "outbound_research_policies",
        ]
        .contains(&name.as_str())
        {
            "outbound-lead-generation".to_owned()
        } else if let Some(owner) = ownership[&name]["owner"].as_str() {
            owner.to_owned()
        } else if choices.len() == 1 {
            choices[0].0.clone()
        } else {
            return Err(invalid(format!("ambiguous_collection_owner:{name}")));
        };
        let metadata = &choices
            .iter()
            .find(|(a, _)| a == &owner)
            .ok_or_else(|| invalid(format!("owner_definition_missing:{name}")))?
            .1;
        let entry = input
            .schemas
            .get(&owner)
            .and_then(|s| s["collections"].get(&name))
            .unwrap_or(&metadata["schema"]);
        let schema = entry.get("schema").unwrap_or(entry).clone();
        if !schema.is_object() {
            return Err(invalid(format!("schema_missing:{name}")));
        }
        let version = schema["version"].as_u64().unwrap_or(0);
        collections
            .entry(owner.clone())
            .or_default()
            .push(CollectionDefinition {
                name,
                app_id: owner,
                schema_ver: u32::try_from(version)
                    .map_err(|_| invalid("schema_version_overflow"))?,
                schema_json: schema,
                class: metadata["class"]
                    .as_str()
                    .ok_or_else(|| invalid("collection_class_missing"))?
                    .into(),
                client_limit: metadata["client_limit"]
                    .as_u64()
                    .unwrap_or(defaults.client_limit),
                byte_budget: metadata["byte_budget"]
                    .as_u64()
                    .unwrap_or(defaults.byte_budget),
                rights_ver: defaults.rights_ver,
                app_ver: defaults.app_ver.clone(),
            });
    }
    merges = merges
        .into_iter()
        .map(|path| {
            path.split_once(".collections.")
                .map(|(_, rest)| rest.split('.').next().unwrap_or(rest).to_owned())
                .unwrap_or(path)
        })
        .collect();
    merges.sort();
    merges.dedup();
    removed.sort();
    removed.dedup();
    Ok(ReleasePlan {
        version: defaults.app_ver.clone(),
        manifests: input.manifests,
        collections,
        migrations,
        removed_sync_settings: removed,
        field_merge_core_rule: merges,
    })
}

/// Caller supplies the artifact migration evaluator, executed outside owner jobs.
/// Tombstones stay tombstones. It must not mint a different storage identity.
pub trait MigrationEvaluator {
    fn migrate(&self, collection: &str, specification: &Value, body: Value) -> Result<Value>;
}

/// Artifact publication is owned by the app publisher. Never fabricate a
/// signature or declare a source tree installed from the mere presence of JSON.
/// The returned manifest must be the actual sanitized, signed version receipt.
pub struct PublishedManifest {
    pub app_id: String,
    pub app_ver: String,
    pub manifest: Value,
    pub manifest_sha256: String,
    pub signature: String,
    pub visible_roles: Vec<String>,
}

pub fn catalog_documents(
    plan: &ReleasePlan,
    receipts: &[PublishedManifest],
    published_at_ms: i64,
) -> Result<Vec<Document>> {
    let mut docs = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for receipt in receipts {
        if receipt.app_ver != plan.version
            || receipt.manifest_sha256.is_empty()
            || receipt.signature.is_empty()
            || !seen.insert(receipt.app_id.clone())
        {
            return Err(invalid("base_artifact_receipt_invalid"));
        }
        let expected = plan
            .manifests
            .iter()
            .find(|m| m["app_id"] == receipt.app_id)
            .ok_or_else(|| invalid("base_artifact_app_not_in_release"))?;
        let mut actual = receipt.manifest.clone();
        if actual["app_id"] != receipt.app_id || actual["app_ver"] != receipt.app_ver {
            return Err(invalid("base_artifact_identity_mismatch"));
        }
        // These fields are set by the trusted publisher; every app-supplied byte
        // (including rights, source files and handlers) must otherwise agree.
        for key in ["app_ver", "author", "created_at", "signature"] {
            actual[key] = expected.get(key).cloned().unwrap_or(Value::Null);
        }
        if actual != *expected {
            return Err(invalid("base_artifact_manifest_changed"));
        }
        let app = &receipt.app_id;
        docs.push(Document { revision: None, collection:"business_app_versions".into(),id:format!("{app}:{}",receipt.app_ver),body:serde_json::json!({"id":format!("{app}:{}",receipt.app_ver),"app_id":app,"app_ver":receipt.app_ver,"manifest":receipt.manifest,"manifest_sha256":receipt.manifest_sha256,"signature":receipt.signature,"base_app_ver":FROM_VERSION,"state":"published","created_at_ms":published_at_ms}) });
        docs.push(Document { revision: None, collection:"app_catalog".into(),id:app.clone(),body:serde_json::json!({"id":app,"app_id":app,"app_ver":receipt.app_ver,"title":expected["title"].as_str().unwrap_or(app),"layout":expected["layout"].as_object().cloned().unwrap_or_default(),"source":"main","published_at":published_at_ms}) });
    }
    if seen.len() != plan.manifests.len() {
        return Err(invalid("base_artifact_receipts_incomplete"));
    }
    Ok(docs)
}

/// Call only with receipts from the trusted artifact publisher, after core and
/// legacy data import. It is NOT a signing/verification endpoint. App visibility
/// comes from that publisher's current policy decision, never a guessed role.
/// The activation watermark covers imported data as well as schema migration.
pub async fn activate_artifacts(
    writer: &WriterHandle,
    plan: &ReleasePlan,
    receipts: &[PublishedManifest],
    published_at_ms: i64,
) -> Result<ImportReport> {
    let mut documents = catalog_documents(plan, receipts, published_at_ms)?;
    let apps: Vec<_> = receipts
        .iter()
        .map(|r| (r.app_id.clone(), r.visible_roles.clone()))
        .collect();
    let manifests = plan.manifests.clone();
    let version = plan.version.clone();
    writer.submit(WriteJob::new(JobOptions::default(),move |tx| {
        let watermark = sql(change_log::head(tx))?.head_seq;
        for (app,roles) in apps {
            let active: Option<String> = sql(tx.query_row("SELECT app_ver FROM _schema_apps WHERE app_id=?1",[&app],|r|r.get(0)).optional())?;
            if active.as_deref() != Some(&version) { return Err(invalid("base_schema_not_activated")); }
            let manifest = manifests.iter().find(|m|m["app_id"] == app).ok_or_else(||invalid("base_artifact_app_not_in_release"))?;
            let collection = sql(schema::active_collection(tx,"business_apps"))?.ok_or_else(||invalid("business_apps_not_active"))?;
            let table = sql(change_log::identifier(&collection.table))?;
            let existing: Option<String> = sql(tx.query_row(&format!("SELECT data FROM {table} WHERE id=?1 AND deleted=0"),[&app],|r|r.get(0)).optional())?;
            let previous: Option<Value> = existing.map(|s|serde_json::from_str(&s).map_err(|_|invalid("existing_app_invalid"))).transpose()?;
            let activation_seq = if let Some(old) = previous {
                if old["locally_modified"] == true || old["active_ver"] != version { return Err(invalid("base_app_diverged")); }
                old["migrated_at_seq"].as_str().ok_or_else(||invalid("base_app_watermark_missing"))?.to_owned()
            } else { watermark.to_string() };
            documents.push(Document { revision: None,collection:"business_apps".into(),id:app.clone(),body:serde_json::json!({"id":app,"app_id":app,"title":manifest["title"].as_str().unwrap_or(&app),"active_ver":version,"migrated_at_seq":activation_seq,"visible_roles":roles,"layout":manifest["layout"].as_object().cloned().unwrap_or_default(),"provenance":{"source":"main","from_ver":FROM_VERSION},"locally_modified":false,"state":"active","updated_at_ms":published_at_ms})});
        }
        let mut report = ImportReport::default();
        for doc in documents {
            let inserted = super::insert_document(tx,&doc)?;
            let counts = report.collections.entry(doc.collection).or_default();
            counts.source_rows += 1; counts.target_documents += 1; counts.inserted += usize::from(inserted); counts.already_present += usize::from(!inserted);
        }
        Ok(report)
    }))?.await
}

pub async fn publish(
    writer: &WriterHandle,
    plan: &ReleasePlan,
    evaluator: &impl MigrationEvaluator,
    now: i64,
) -> Result<Vec<String>> {
    let mut activated = Vec::new();
    for (app, definitions) in &plan.collections {
        let app = app.clone();
        let definitions = definitions.clone();
        let target = plan.version.clone();
        let prepared = writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                sql(schema::initialize(tx))?;
                let current: Option<String> = sql(tx
                    .query_row(
                        "SELECT app_ver FROM _schema_apps WHERE app_id=?1",
                        [&app],
                        |r| r.get(0),
                    )
                    .optional())?;
                if current.as_deref() == Some(&target) {
                    for def in &definitions {
                        if sql(schema::active_collection(tx, &def.name))?
                            .map(|c| c.definition)
                            .as_ref()
                            != Some(def)
                        {
                            return Err(invalid("import_version_reused_with_different_schema"));
                        }
                    }
                    return Ok(None);
                }
                let operation_id = format!("base-import:{app}:{target}");
                let lock = PublishLock {
                    app_id: app.clone(),
                    base_app_ver: current.unwrap_or_default(),
                    target_app_ver: target,
                    operation_id,
                };
                sql(schema::reserve_publish(tx, &lock))?;
                let mut names = Vec::new();
                for def in &definitions {
                    let old = sql(schema::active_collection(tx, &def.name))?;
                    if let Some(old) = old {
                        if sql(schema::classify_change(&old.definition, def))?
                            == SchemaChange::Compatible
                        {
                            sql(schema::update_compatible(tx, &lock, def))?;
                            continue;
                        }
                    }
                    let migration = sql(schema::prepare_migration(tx, &lock, def))?;
                    if migration.source_gen != 0 {
                        names.push(def.name.clone());
                    }
                }
                Ok(Some((lock, names)))
            }))?
            .await?;
        let Some((lock, names)) = prepared else {
            continue;
        };
        for name in names {
            loop {
                let read_lock = lock.clone();
                let read_name = name.clone();
                let (migration, page) = writer
                    .submit(WriteJob::new(JobOptions::default(), move |tx| {
                        let migration =
                            sql(schema::migration(tx, &read_lock.operation_id, &read_name))?;
                        let page = sql(change_log::read_page(
                            tx,
                            &read_name,
                            migration.source_gen,
                            migration.cursor,
                            schema::MIGRATION_BATCH,
                        ))?;
                        Ok((migration, page))
                    }))?
                    .await?;
                let mut outputs = Vec::new();
                for doc in &page.docs {
                    outputs.push(if doc.op == change_log::Operation::Delete {
                        MigrationOutput::Delete
                    } else {
                        let mut body = doc
                            .body
                            .clone()
                            .ok_or_else(|| invalid("migration_body_missing"))?;
                        if let Some(spec) = plan.migrations.get(&name) {
                            body = evaluator.migrate(&name, spec, body)?;
                        }
                        if body["id"].as_str() != Some(&doc.id) {
                            return Err(invalid(format!("migration_identity_mismatch:{name}")));
                        }
                        MigrationOutput::Upsert {
                            rev: doc.rev.clone(),
                            body,
                        }
                    });
                }
                let apply_lock = lock.clone();
                let apply_name = name.clone();
                let state = writer
                    .submit(WriteJob::new(JobOptions::default(), move |tx| {
                        sql(schema::apply_migration_page(
                            tx,
                            &apply_lock,
                            &apply_name,
                            migration.cursor,
                            &page,
                            &outputs,
                            now,
                        ))
                    }))?
                    .await?;
                if state.state == "ready" {
                    break;
                }
            }
        }
        let activate = lock.clone();
        writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                sql(schema::activate_publication(tx, &activate, now))
            }))?
            .await?;
        activated.push(lock.app_id);
    }
    Ok(activated)
}

pub struct LegacyPage {
    pub documents: Vec<Document>,
    pub cursor: String,
    pub complete: bool,
}

/// Read a bounded page from the explicit legacy table mapping of the frozen
/// RxDB profile. Never guesses which historical schema/table is authoritative.
/// Caller imports pages before exposing the newly published app artifacts.
pub fn legacy_page(
    tx: &rusqlite::Transaction<'_>,
    plan: &ReleasePlan,
    evaluator: &impl MigrationEvaluator,
    collection: &str,
    table: &str,
    after_id: &str,
    limit: usize,
) -> Result<LegacyPage> {
    if limit == 0 || limit > 128 {
        return Err(invalid("legacy_import_page_limit"));
    }
    if !plan
        .collections
        .values()
        .flatten()
        .any(|d| d.name == collection)
    {
        return Err(invalid("legacy_collection_not_in_release"));
    }
    let table = sql(change_log::identifier(table))?;
    let mut statement = sql(tx.prepare(&format!(
        "SELECT id,data,deleted,revision FROM {table} WHERE id>?1 ORDER BY id LIMIT ?2"
    )))?;
    let values = sql(statement.query_map(params![after_id, limit as i64], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, bool>(2)?,
            r.get::<_, Option<String>>(3)?,
        ))
    }))?;
    let mut docs = Vec::new();
    let mut cursor = after_id.to_owned();
    for value in values {
        let (source_id, data, deleted, revision) = sql(value)?;
        cursor = source_id.clone();
        if deleted {
            docs.push(Document {
                revision,
                collection: collection.into(),
                id: source_id.clone(),
                body: serde_json::json!({"id":source_id,"_deleted":true}),
            });
            continue;
        }
        let mut body: Value = serde_json::from_str(&data)
            .map_err(|_| invalid(format!("legacy_invalid_json:{collection}:{source_id}")))?;
        if let Some(specification) = plan.migrations.get(collection) {
            body = evaluator.migrate(collection, specification, body)?;
        }
        let key = match collection {
            "communication_accounts" => "account_key",
            "communication_threads" => "thread_key",
            "communication_messages" => "message_key",
            _ => "id",
        };
        let id = body[key]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(&source_id)
            .to_owned();
        body["id"] = id.clone().into();
        docs.push(Document {
            revision,
            collection: collection.into(),
            id,
            body,
        });
    }
    let complete = docs.len() < limit;
    Ok(LegacyPage {
        documents: docs,
        cursor,
        complete,
    })
}
