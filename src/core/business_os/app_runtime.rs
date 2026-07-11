// Origin: CTOX
// License: AGPL-3.0-only

//! Declarative, runtime-loaded Business OS app actions.
//!
//! This module deliberately exposes a small data vocabulary. App packages can
//! compose RxDB record effects into a durable native saga, but cannot smuggle
//! SQL, shell, filesystem, browser code, or network effects into the daemon.

use crate::mission::channels;
use anyhow::Context;
use rxdb::rx_collection::RxCollection;
use rxdb::rx_database::RxDatabase;
use rxdb::types::MangoQuery;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const APP_ACTION_COMMAND_TYPE: &str = "ctox.app.action.run";
const APP_RUNTIME_CAPABILITY: &str = "ctox-app-runtime-v1";
const ALLOWED_OPS: &[&str] = &[
    "read",
    "assert",
    "insert",
    "upsert",
    "patch",
    "delete",
    "tombstone",
];

pub(crate) fn idempotent_command_id(module_id: &str, action_name: &str, key: &str) -> String {
    let material = format!("{module_id}\0{action_name}\0{key}");
    format!("cmd_app_{:x}", Sha256::digest(material.as_bytes()))
}

#[derive(Debug, Clone)]
pub(crate) struct AppActionAdmission {
    pub snapshot: Value,
    pub definition_hash: String,
    pub step_names: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct AppActionExecution {
    pub status: &'static str,
    pub result: Value,
    pub error_code: Option<&'static str>,
    pub error_message: Option<String>,
}

pub(crate) fn admit(
    root: &Path,
    command_id: &str,
    module_id: &str,
    action_name: &str,
    requested_version: Option<u64>,
    input: Value,
    actor_id: &str,
) -> anyhow::Result<AppActionAdmission> {
    validate_identifier("module_id", module_id)?;
    validate_identifier("action", action_name)?;
    anyhow::ensure!(
        !actor_id.trim().is_empty(),
        "app_action_permission_denied: actor is required"
    );

    let (manifest, manifest_path) = load_runtime_manifest(root, module_id)?;
    let runtime = manifest
        .get("data_runtime")
        .and_then(Value::as_object)
        .context("app_action_not_registered: module has no data_runtime")?;
    anyhow::ensure!(
        runtime.get("version").and_then(Value::as_u64) == Some(1),
        "app_runtime_reconfiguring: module does not provide data_runtime.version=1"
    );
    let sync = runtime
        .get("sync")
        .and_then(Value::as_str)
        .unwrap_or("realtime");
    anyhow::ensure!(
        sync == "realtime",
        "app_runtime_reconfiguring: only realtime sync is supported"
    );
    let scope = runtime
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("actor");
    anyhow::ensure!(
        matches!(scope, "actor" | "workspace"),
        "app_action_input_invalid: invalid data runtime scope"
    );
    let action = runtime
        .get("actions")
        .and_then(Value::as_object)
        .and_then(|actions| actions.get(action_name))
        .cloned()
        .with_context(|| format!("app_action_not_registered: `{module_id}.{action_name}`"))?;
    let action_object = action
        .as_object()
        .context("app_action_not_registered: action definition must be an object")?;
    let version = action_object
        .get("version")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    if let Some(requested) = requested_version {
        anyhow::ensure!(
            requested == version,
            "app_action_definition_changed: requested version {requested}, active version {version}"
        );
    }
    if let Some(input_schema) = action_object.get("input_schema") {
        validate_input_schema_definition(input_schema, "$input")?;
        validate_input_schema(input_schema, &input, "$input")?;
    }
    let declared_collections: HashSet<String> = manifest
        .get("collections")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    let step_names = validate_action_structure(module_id, &declared_collections, action_object)?;
    let canonical = canonical_json(&action);
    let definition_hash = format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(&canonical)?)
    );
    let snapshot = json!({
        "capability": APP_RUNTIME_CAPABILITY,
        "command_id": command_id,
        "module_id": module_id,
        "action_name": action_name,
        "action_version": version,
        "definition_hash": definition_hash,
        "definition": canonical,
        "input": input,
        "scope": scope,
        "actor_id": actor_id,
        "admitted_at_ms": now_ms(),
        "manifest_path": manifest_path.file_name().and_then(|name| name.to_str()).unwrap_or("module.json"),
    });
    Ok(AppActionAdmission {
        snapshot,
        definition_hash,
        step_names,
    })
}

pub(crate) fn persist_admission(
    root: &Path,
    command_id: &str,
    admission: &AppActionAdmission,
) -> anyhow::Result<()> {
    let module_id = admission
        .snapshot
        .get("module_id")
        .and_then(Value::as_str)
        .context("app action admission lacks module_id")?;
    let action_name = admission
        .snapshot
        .get("action_name")
        .and_then(Value::as_str)
        .context("app action admission lacks action_name")?;
    channels::start_runtime_business_command_saga(
        root,
        command_id,
        module_id,
        action_name,
        &admission.definition_hash,
        &admission.snapshot,
        &admission.step_names,
    )
}

pub(crate) fn admitted_snapshot(root: &Path, command_id: &str) -> anyhow::Result<Option<Value>> {
    channels::runtime_business_command_action_snapshot(root, command_id)
}

pub(crate) fn inspect_module(root: &Path, module_id: &str) -> anyhow::Result<Value> {
    validate_identifier("module_id", module_id)?;
    let (manifest, manifest_path) = load_runtime_manifest(root, module_id)?;
    let runtime = manifest.get("data_runtime").cloned().unwrap_or_else(
        || json!({ "version": 1, "sync": "realtime", "scope": "actor", "actions": {} }),
    );
    let canonical = canonical_json(&runtime);
    let runtime_object = canonical
        .as_object()
        .context("app_runtime_reconfiguring: data_runtime must be an object")?;
    anyhow::ensure!(
        runtime_object.get("version").and_then(Value::as_u64) == Some(1),
        "app_runtime_reconfiguring: data_runtime.version must be 1"
    );
    anyhow::ensure!(
        runtime_object
            .get("sync")
            .and_then(Value::as_str)
            .unwrap_or("realtime")
            == "realtime",
        "app_runtime_reconfiguring: data_runtime.sync must be realtime"
    );
    anyhow::ensure!(
        matches!(
            runtime_object
                .get("scope")
                .and_then(Value::as_str)
                .unwrap_or("actor"),
            "actor" | "workspace"
        ),
        "app_runtime_reconfiguring: data_runtime.scope must be actor or workspace"
    );
    let declared_collections = manifest
        .get("collections")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<HashSet<_>>();
    if let Some(definitions) = runtime_object.get("actions").and_then(Value::as_object) {
        for (name, definition) in definitions {
            validate_identifier("action", name)?;
            let definition = definition
                .as_object()
                .context("app_runtime_reconfiguring: action definition must be an object")?;
            anyhow::ensure!(
                definition
                    .get("version")
                    .and_then(Value::as_u64)
                    .unwrap_or(1)
                    > 0,
                "app_runtime_reconfiguring: action version must be positive"
            );
            if let Some(schema) = definition.get("input_schema") {
                validate_input_schema_definition(schema, "$input")?;
            }
            validate_action_structure(module_id, &declared_collections, definition)?;
        }
    }
    let runtime_hash = format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(&canonical)?)
    );
    let actions = runtime
        .get("actions")
        .and_then(Value::as_object)
        .map(|actions| actions.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    Ok(json!({
        "ok": true,
        "capability": APP_RUNTIME_CAPABILITY,
        "module_id": module_id,
        "manifest_path": manifest_path,
        "runtime_hash": runtime_hash,
        "data_runtime": canonical,
        "collections": manifest.get("collections").cloned().unwrap_or_else(|| json!([])),
        "actions": actions,
        "validated": true,
        "requires_backend_recompile": false,
        "data_plane": "rxdb-webrtc",
    }))
}

fn validate_action_structure(
    module_id: &str,
    declared_collections: &HashSet<String>,
    action: &Map<String, Value>,
) -> anyhow::Result<Vec<String>> {
    let steps = action
        .get("steps")
        .and_then(Value::as_array)
        .context("app_action_not_registered: action requires steps")?;
    anyhow::ensure!(
        !steps.is_empty() && steps.len() <= 64,
        "app_action_input_invalid: action must contain 1..64 steps"
    );
    let mut step_names = Vec::with_capacity(steps.len());
    let mut seen_names = HashSet::new();
    for (index, step) in steps.iter().enumerate() {
        let object = step
            .as_object()
            .context("app_action_input_invalid: every step must be an object")?;
        let op = object.get("op").and_then(Value::as_str).unwrap_or_default();
        anyhow::ensure!(
            ALLOWED_OPS.contains(&op),
            "app_action_input_invalid: step {index} uses unsupported op `{op}`"
        );
        let collection = object
            .get("collection")
            .and_then(Value::as_str)
            .unwrap_or_default();
        anyhow::ensure!(declared_collections.contains(collection), "app_action_permission_denied: collection `{collection}` is not declared by `{module_id}`");
        let collection_prefix = format!("{}_", module_id.replace('-', "_"));
        anyhow::ensure!(
            collection.starts_with(&collection_prefix),
            "app_action_permission_denied: runtime action collection `{collection}` must be owned by `{module_id}`"
        );
        let name = object
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("step_{index}_{op}"));
        validate_identifier("step name", &name)?;
        anyhow::ensure!(
            seen_names.insert(name.clone()),
            "app_action_input_invalid: duplicate step name `{name}`"
        );
        match op {
            "insert" | "upsert" => anyhow::ensure!(
                object.get("record").is_some(),
                "app_action_input_invalid: `{op}` requires record"
            ),
            "patch" => anyhow::ensure!(
                object.get("id").is_some() && object.get("patch").is_some(),
                "app_action_input_invalid: patch requires id and patch"
            ),
            _ => anyhow::ensure!(
                object.get("id").is_some(),
                "app_action_input_invalid: `{op}` requires id"
            ),
        }
        step_names.push(name);
    }
    Ok(step_names)
}

pub(crate) async fn execute(
    root: &Path,
    database: &Arc<RxDatabase>,
    command_id: &str,
    snapshot: &Value,
) -> anyhow::Result<AppActionExecution> {
    let durable_status = channels::business_command_saga_status(root, command_id)?;
    if let Some(status) = durable_status.as_ref() {
        let phase = status
            .get("phase")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let error = status
            .get("error_message")
            .and_then(Value::as_str)
            .unwrap_or("runtime action recovered after process interruption")
            .to_owned();
        if phase == "compensated" {
            return Ok(AppActionExecution {
                status: "failed",
                result: json!({ "ok": false, "compensated": true }),
                error_code: Some("app_action_input_invalid"),
                error_message: Some(error),
            });
        }
        if phase == "manual_intervention" {
            return Ok(AppActionExecution {
                status: "failed",
                result: json!({ "ok": false, "manual_intervention": true }),
                error_code: Some("app_action_compensation_failed"),
                error_message: Some(error),
            });
        }
        if phase == "completed" {
            return Ok(AppActionExecution {
                status: "completed",
                result: json!({
                    "ok": true,
                    "module_id": snapshot.get("module_id"),
                    "action": snapshot.get("action_name"),
                    "definition_hash": snapshot.get("definition_hash"),
                    "steps_completed": status.get("total_steps"),
                    "resumed_after_effects": true,
                }),
                error_code: None,
                error_message: None,
            });
        }
    }
    let definition = snapshot
        .get("definition")
        .context("app action snapshot has no definition")?;
    let steps = definition
        .get("steps")
        .and_then(Value::as_array)
        .context("app action snapshot has no steps")?;
    let input = snapshot.get("input").cloned().unwrap_or(Value::Null);
    let actor_id = snapshot
        .get("actor_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let admitted_at_ms = snapshot
        .get("admitted_at_ms")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let scope = snapshot
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("actor");

    if durable_status
        .as_ref()
        .and_then(|status| status.get("phase"))
        .and_then(Value::as_str)
        == Some("compensating")
    {
        let pending = channels::business_command_saga_pending_compensation_steps(root, command_id)?;
        let pending = pending
            .into_iter()
            .map(|name| (name, Value::Null))
            .collect::<Vec<_>>();
        if let Err(error) = compensate(root, database, command_id, &pending).await {
            return Ok(AppActionExecution {
                status: "failed",
                result: json!({ "ok": false, "manual_intervention": true }),
                error_code: Some("app_action_compensation_failed"),
                error_message: Some(error.to_string()),
            });
        }
        return Ok(AppActionExecution {
            status: "failed",
            result: json!({ "ok": false, "compensated": true, "resumed_after_effects": true }),
            error_code: Some("app_action_input_invalid"),
            error_message: durable_status
                .as_ref()
                .and_then(|status| status.get("error_message"))
                .and_then(Value::as_str)
                .map(str::to_owned),
        });
    }
    let mut completed = Vec::new();

    for (index, step) in steps.iter().enumerate() {
        let object = step
            .as_object()
            .context("snapshotted app action step is invalid")?;
        let op = object.get("op").and_then(Value::as_str).unwrap_or_default();
        let name = object
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("step_{index}_{op}"));
        if !channels::claim_business_command_saga_step(root, command_id, &name, false)? {
            completed.push((name, step.clone()));
            continue;
        }
        match apply_step(
            root,
            database,
            command_id,
            &name,
            step,
            &input,
            actor_id,
            scope,
            admitted_at_ms,
        )
        .await
        {
            Ok(evidence) => {
                channels::complete_business_command_saga_step(
                    root, command_id, &name, false, &evidence,
                )?;
                completed.push((name, step.clone()));
            }
            Err(error) => {
                channels::fail_business_command_saga_step(
                    root,
                    command_id,
                    &name,
                    &error.to_string(),
                    false,
                )?;
                let compensation = compensate(root, database, command_id, &completed).await;
                if let Err(compensation_error) = compensation {
                    return Ok(AppActionExecution {
                        status: "failed",
                        result: json!({ "ok": false, "manual_intervention": true }),
                        error_code: Some("app_action_compensation_failed"),
                        error_message: Some(compensation_error.to_string()),
                    });
                }
                return Ok(AppActionExecution {
                    status: "failed",
                    result: json!({ "ok": false, "compensated": true }),
                    error_code: Some(classify_action_error(&error)),
                    error_message: Some(error.to_string()),
                });
            }
        }
    }
    Ok(AppActionExecution {
        status: "completed",
        result: json!({
            "ok": true,
            "module_id": snapshot.get("module_id"),
            "action": snapshot.get("action_name"),
            "definition_hash": snapshot.get("definition_hash"),
            "steps_completed": steps.len(),
        }),
        error_code: None,
        error_message: None,
    })
}

async fn compensate(
    root: &Path,
    database: &Arc<RxDatabase>,
    command_id: &str,
    completed: &[(String, Value)],
) -> anyhow::Result<()> {
    for (name, _step) in completed.iter().rev() {
        if !channels::claim_business_command_saga_step(root, command_id, name, true)? {
            continue;
        }
        let evidence = channels::business_command_saga_step_evidence(root, command_id, name)?;
        if let Err(error) = restore_evidence(database, &evidence).await {
            channels::fail_business_command_saga_step(
                root,
                command_id,
                name,
                &error.to_string(),
                true,
            )?;
            return Err(error);
        }
        channels::complete_business_command_saga_step(root, command_id, name, true, &evidence)?;
    }
    Ok(())
}

async fn apply_step(
    root: &Path,
    database: &Arc<RxDatabase>,
    command_id: &str,
    step_name: &str,
    step: &Value,
    input: &Value,
    actor_id: &str,
    scope: &str,
    admitted_at_ms: u64,
) -> anyhow::Result<Value> {
    let object = step.as_object().context("invalid action step")?;
    let op = object.get("op").and_then(Value::as_str).unwrap_or_default();
    let collection_name = object
        .get("collection")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let collection = database.collection(collection_name).with_context(|| {
        format!("app_runtime_reconfiguring: collection `{collection_name}` is not registered")
    })?;
    let context = TemplateContext {
        input,
        actor_id,
        command_id,
        admitted_at_ms,
    };

    let (id, mut record) = match op {
        "insert" | "upsert" => {
            let record = render_template(object.get("record").unwrap_or(&Value::Null), &context)?;
            let id = record_id(&collection, &record)?;
            (id, Some(record))
        }
        _ => {
            let id = render_template(object.get("id").unwrap_or(&Value::Null), &context)?
                .as_str()
                .context("app_action_input_invalid: rendered id must be a string")?
                .to_owned();
            (id, None)
        }
    };
    let current = find_record(&collection, &id).await?;
    let existing_evidence =
        channels::business_command_saga_step_evidence(root, command_id, step_name)?;
    let prepared = existing_evidence.get("prepared").and_then(Value::as_bool) == Some(true);
    let before = if prepared {
        existing_evidence
            .get("before")
            .cloned()
            .filter(|value| !value.is_null())
    } else {
        current.clone()
    };
    enforce_actor_scope(scope, actor_id, current.as_ref().or(before.as_ref()))?;
    let evidence = json!({
        "prepared": true,
        "collection": collection_name,
        "record_id": id,
        "op": op,
        "before": before.clone().map(strip_rxdb_meta).unwrap_or(Value::Null),
    });
    if !prepared {
        channels::record_business_command_saga_step_evidence(
            root, command_id, step_name, &evidence,
        )?;
    }

    match op {
        "read" => anyhow::ensure!(
            current.is_some(),
            "app_action_input_invalid: record `{id}` does not exist"
        ),
        "assert" => {
            let expected = render_template(object.get("equals").unwrap_or(&Value::Null), &context)?;
            let path = object
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let actual = current
                .as_ref()
                .and_then(|value| json_path(value, path))
                .cloned()
                .unwrap_or(Value::Null);
            anyhow::ensure!(
                actual == expected,
                "app_action_input_invalid: assertion failed at `{path}`"
            );
        }
        "insert" => {
            enforce_actor_write(scope, actor_id, record.as_mut().unwrap())?;
            let desired = record.clone().unwrap();
            if let Some(current) = current {
                anyhow::ensure!(
                    prepared && strip_rxdb_meta(current) == desired,
                    "app_action_input_invalid: record `{id}` already exists with different data"
                );
            } else {
                collection
                    .insert(desired)
                    .await
                    .map_err(anyhow::Error::from)?;
            }
        }
        "upsert" => {
            enforce_actor_write(scope, actor_id, record.as_mut().unwrap())?;
            collection
                .incremental_upsert(record.clone().unwrap())
                .await
                .map_err(anyhow::Error::from)?;
        }
        "patch" => {
            let mut next = current
                .clone()
                .context("app_action_input_invalid: patch target does not exist")?;
            let patch = render_template(object.get("patch").unwrap_or(&Value::Null), &context)?;
            merge_patch(&mut next, &patch)?;
            enforce_actor_write(scope, actor_id, &mut next)?;
            collection
                .incremental_upsert(next)
                .await
                .map_err(anyhow::Error::from)?;
        }
        "delete" | "tombstone" => {
            if current.is_some() {
                let removed = collection
                    .bulk_remove_by_ids(vec![id.clone()])
                    .await
                    .map_err(anyhow::Error::from)?;
                anyhow::ensure!(
                    removed.error.is_empty(),
                    "failed to tombstone record `{id}`"
                );
            }
        }
        _ => anyhow::bail!("app_action_input_invalid: unsupported action op `{op}`"),
    }
    Ok(evidence)
}

async fn restore_evidence(database: &Arc<RxDatabase>, evidence: &Value) -> anyhow::Result<()> {
    let collection_name = evidence
        .get("collection")
        .and_then(Value::as_str)
        .context("compensation evidence lacks collection")?;
    let id = evidence
        .get("record_id")
        .and_then(Value::as_str)
        .context("compensation evidence lacks record id")?;
    let op = evidence
        .get("op")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let collection = database.collection(collection_name).with_context(|| {
        format!("collection `{collection_name}` is unavailable during compensation")
    })?;
    let before = evidence.get("before").cloned().unwrap_or(Value::Null);
    if matches!(op, "read" | "assert") {
        return Ok(());
    }
    if before.is_null() {
        if find_record(&collection, id).await?.is_some() {
            let result = collection
                .bulk_remove_by_ids(vec![id.to_owned()])
                .await
                .map_err(anyhow::Error::from)?;
            anyhow::ensure!(
                result.error.is_empty(),
                "failed to remove inserted record during compensation"
            );
        }
    } else {
        let restored = prepare_compensation_document(before, now_ms());
        collection
            .incremental_upsert(restored)
            .await
            .map_err(anyhow::Error::from)?;
    }
    Ok(())
}

fn prepare_compensation_document(mut before: Value, restored_at_ms: u64) -> Value {
    if let Some(object) = before.as_object_mut() {
        if object.contains_key("updated_at_ms") {
            object.insert("updated_at_ms".to_owned(), Value::from(restored_at_ms));
        }
        if object.contains_key("updatedAtMs") {
            object.insert("updatedAtMs".to_owned(), Value::from(restored_at_ms));
        }
    }
    before
}

async fn find_record(collection: &Arc<RxCollection>, id: &str) -> anyhow::Result<Option<Value>> {
    let primary = collection.primary_path().unwrap_or_else(|| "id".to_owned());
    let mut selector = Map::new();
    selector.insert(primary, json!({ "$eq": id }));
    let value = collection
        .find_one(Some(MangoQuery {
            selector: Some(Value::Object(selector)),
            ..Default::default()
        }))?
        .exec(false)
        .await?;
    Ok((!value.is_null()).then_some(value))
}

fn record_id(collection: &Arc<RxCollection>, record: &Value) -> anyhow::Result<String> {
    let primary = collection.primary_path().unwrap_or_else(|| "id".to_owned());
    record
        .get(&primary)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .with_context(|| {
            format!("app_action_input_invalid: record requires string primary key `{primary}`")
        })
}

fn enforce_actor_scope(scope: &str, actor_id: &str, record: Option<&Value>) -> anyhow::Result<()> {
    if scope == "actor" {
        if let Some(record) = record {
            anyhow::ensure!(
                record.get("actor_id").and_then(Value::as_str) == Some(actor_id),
                "app_action_permission_denied: actor-scoped record belongs to another actor"
            );
        }
    }
    Ok(())
}

fn enforce_actor_write(scope: &str, actor_id: &str, record: &mut Value) -> anyhow::Result<()> {
    if scope == "actor" {
        let object = record
            .as_object_mut()
            .context("app_action_input_invalid: record must be an object")?;
        if let Some(existing) = object.get("actor_id").and_then(Value::as_str) {
            anyhow::ensure!(
                existing == actor_id,
                "app_action_permission_denied: actor_id cannot be changed"
            );
        }
        object.insert("actor_id".to_owned(), Value::String(actor_id.to_owned()));
    }
    Ok(())
}

struct TemplateContext<'a> {
    input: &'a Value,
    actor_id: &'a str,
    command_id: &'a str,
    admitted_at_ms: u64,
}

fn render_template(value: &Value, context: &TemplateContext<'_>) -> anyhow::Result<Value> {
    if let Some(object) = value.as_object() {
        if object.len() == 1 {
            if let Some(path) = object.get("$input").and_then(Value::as_str) {
                return json_path(context.input, path).cloned().with_context(|| {
                    format!("app_action_input_invalid: input path `{path}` does not exist")
                });
            }
            if object.get("$actor").and_then(Value::as_str) == Some("id") {
                return Ok(Value::String(context.actor_id.to_owned()));
            }
            if object.contains_key("$command_id") {
                return Ok(Value::String(context.command_id.to_owned()));
            }
            if object.contains_key("$now_ms") {
                return Ok(Value::from(context.admitted_at_ms));
            }
            if let Some(literal) = object.get("$literal") {
                return Ok(literal.clone());
            }
        }
        return object
            .iter()
            .map(|(key, value)| Ok((key.clone(), render_template(value, context)?)))
            .collect::<anyhow::Result<Map<String, Value>>>()
            .map(Value::Object);
    }
    if let Some(array) = value.as_array() {
        return array
            .iter()
            .map(|value| render_template(value, context))
            .collect::<anyhow::Result<Vec<_>>>()
            .map(Value::Array);
    }
    Ok(value.clone())
}

fn merge_patch(target: &mut Value, patch: &Value) -> anyhow::Result<()> {
    let target = target
        .as_object_mut()
        .context("patch target must be an object")?;
    let patch = patch
        .as_object()
        .context("rendered patch must be an object")?;
    for (key, value) in patch {
        if value.is_null() {
            target.remove(key);
        } else {
            target.insert(key.clone(), value.clone());
        }
    }
    Ok(())
}

fn strip_rxdb_meta(mut value: Value) -> Value {
    if let Some(object) = value.as_object_mut() {
        for key in ["_rev", "_meta", "_attachments", "_deleted"] {
            object.remove(key);
        }
    }
    value
}

fn load_runtime_manifest(root: &Path, module_id: &str) -> anyhow::Result<(Value, PathBuf)> {
    let app_root = runtime_app_root(root);
    let candidates = [
        app_root
            .join("installed-modules")
            .join(module_id)
            .join("module.json"),
        app_root
            .join("local-modules")
            .join(module_id)
            .join("module.json"),
        root.join("src/apps/business-os/modules")
            .join(module_id)
            .join("module.json"),
    ];
    for path in candidates {
        if !path.is_file() {
            continue;
        }
        let value: Value = serde_json::from_str(
            &fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?,
        )
        .with_context(|| format!("parse {}", path.display()))?;
        anyhow::ensure!(
            value.get("id").and_then(Value::as_str) == Some(module_id),
            "app_action_not_registered: module id mismatch"
        );
        return Ok((value, path));
    }
    anyhow::bail!("app_action_not_registered: module `{module_id}` is not installed")
}

fn runtime_app_root(root: &Path) -> PathBuf {
    if root.file_name().and_then(|name| name.to_str()) == Some("runtime") {
        return root.join("business-os");
    }
    if root.join("runtime").exists() {
        return root.join("runtime/business-os");
    }
    root.join("business-os")
}

fn validate_input_schema_definition(schema: &Value, path: &str) -> anyhow::Result<()> {
    let object = schema
        .as_object()
        .context("app_action_input_invalid: input_schema must be an object")?;
    if let Some(expected) = object.get("type").and_then(Value::as_str) {
        anyhow::ensure!(
            matches!(
                expected,
                "object" | "array" | "string" | "number" | "integer" | "boolean" | "null"
            ),
            "app_action_input_invalid: unsupported schema type `{expected}` at {path}"
        );
    }
    if let Some(required) = object.get("required") {
        anyhow::ensure!(
            required
                .as_array()
                .is_some_and(|items| items.iter().all(Value::is_string)),
            "app_action_input_invalid: required must be a string array at {path}"
        );
    }
    if let Some(properties) = object.get("properties") {
        let properties = properties
            .as_object()
            .context("app_action_input_invalid: properties must be an object")?;
        for (key, child) in properties {
            validate_input_schema_definition(child, &format!("{path}.{key}"))?;
        }
    }
    Ok(())
}

fn validate_input_schema(schema: &Value, input: &Value, path: &str) -> anyhow::Result<()> {
    let object = schema
        .as_object()
        .context("app_action_input_invalid: input_schema must be an object")?;
    if let Some(expected) = object.get("type").and_then(Value::as_str) {
        let matches = match expected {
            "object" => input.is_object(),
            "array" => input.is_array(),
            "string" => input.is_string(),
            "number" => input.is_number(),
            "integer" => input.as_i64().is_some() || input.as_u64().is_some(),
            "boolean" => input.is_boolean(),
            "null" => input.is_null(),
            _ => false,
        };
        anyhow::ensure!(
            matches,
            "app_action_input_invalid: {path} must be {expected}"
        );
    }
    if let Some(required) = object.get("required").and_then(Value::as_array) {
        let input_object = input
            .as_object()
            .context("app_action_input_invalid: required fields need object input")?;
        for key in required.iter().filter_map(Value::as_str) {
            anyhow::ensure!(
                input_object.contains_key(key),
                "app_action_input_invalid: missing {path}.{key}"
            );
        }
    }
    if let (Some(properties), Some(input_object)) = (
        object.get("properties").and_then(Value::as_object),
        input.as_object(),
    ) {
        for (key, property_schema) in properties {
            if let Some(value) = input_object.get(key) {
                validate_input_schema(property_schema, value, &format!("{path}.{key}"))?;
            }
        }
        if object.get("additionalProperties").and_then(Value::as_bool) == Some(false) {
            for key in input_object.keys() {
                anyhow::ensure!(
                    properties.contains_key(key),
                    "app_action_input_invalid: unexpected {path}.{key}"
                );
            }
        }
    }
    Ok(())
}

fn validate_identifier(label: &str, value: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        !value.is_empty()
            && value.len() <= 160
            && value
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.')),
        "app_action_input_invalid: invalid {label}"
    );
    Ok(())
}

fn canonical_json(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            BTreeMap::from_iter(
                object
                    .iter()
                    .map(|(key, value)| (key.clone(), canonical_json(value))),
            )
            .into_iter()
            .collect(),
        ),
        Value::Array(array) => Value::Array(array.iter().map(canonical_json).collect()),
        _ => value.clone(),
    }
}

fn json_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(value);
    }
    path.split('.')
        .try_fold(value, |current, segment| current.get(segment))
}

fn classify_action_error(error: &anyhow::Error) -> &'static str {
    let message = error.to_string();
    if message.contains("permission") {
        "app_action_permission_denied"
    } else if message.contains("reconfiguring") || message.contains("not registered") {
        "app_runtime_reconfiguring"
    } else {
        "app_action_input_invalid"
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission::channels::BusinessCommandClaimRequest;

    #[test]
    fn templates_are_data_only_and_resolve_stable_context() {
        let input = json!({ "record": { "id": "r1", "title": "Hello" } });
        let context = TemplateContext {
            input: &input,
            actor_id: "actor-1",
            command_id: "cmd-1",
            admitted_at_ms: 42,
        };
        let rendered = render_template(
            &json!({
                "id": { "$input": "record.id" },
                "title": { "$input": "record.title" },
                "actor_id": { "$actor": "id" },
                "command_id": { "$command_id": true }
            }),
            &context,
        )
        .unwrap();
        assert_eq!(rendered["id"], "r1");
        assert_eq!(rendered["actor_id"], "actor-1");
        assert_eq!(rendered["command_id"], "cmd-1");
    }

    #[test]
    fn input_schema_rejects_unknown_and_missing_fields() {
        let schema = json!({
            "type": "object",
            "required": ["title"],
            "additionalProperties": false,
            "properties": { "title": { "type": "string" } }
        });
        assert!(validate_input_schema(&schema, &json!({ "title": "ok" }), "$input").is_ok());
        assert!(validate_input_schema(&schema, &json!({}), "$input").is_err());
        assert!(
            validate_input_schema(&schema, &json!({ "title": "ok", "sql": "DROP" }), "$input")
                .is_err()
        );
    }

    #[test]
    fn compensation_advances_technical_update_timestamp() {
        let restored = prepare_compensation_document(
            json!({ "id": "record-1", "status": "created", "updated_at_ms": 10 }),
            20,
        );
        assert_eq!(restored["status"], "created");
        assert_eq!(restored["updated_at_ms"], 20);
    }

    #[test]
    fn runtime_definition_is_snapshotted_before_execution() {
        let root = std::env::temp_dir().join(format!("ctox-app-runtime-{}", now_ms()));
        let module_dir = root.join("runtime/business-os/local-modules/record-workbench");
        fs::create_dir_all(&module_dir).unwrap();
        let manifest = json!({
            "id": "record-workbench",
            "collections": ["record_workbench_records"],
            "data_runtime": {
                "version": 1,
                "sync": "realtime",
                "scope": "actor",
                "actions": {
                    "save": {
                        "version": 1,
                        "input_schema": {
                            "type": "object",
                            "required": ["id"],
                            "properties": { "id": { "type": "string" } }
                        },
                        "steps": [{
                            "name": "save_record",
                            "op": "upsert",
                            "collection": "record_workbench_records",
                            "record": {
                                "id": { "$input": "id" },
                                "actor_id": { "$actor": "id" }
                            }
                        }]
                    }
                }
            }
        });
        fs::write(
            module_dir.join("module.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        channels::claim_business_control_command(
            &root,
            BusinessCommandClaimRequest {
                command_id: "cmd-runtime-snapshot".to_owned(),
                idempotency_key: "cmd-runtime-snapshot".to_owned(),
                payload_hash: "sha256:test".to_owned(),
                module: "record-workbench".to_owned(),
                command_type: APP_ACTION_COMMAND_TYPE.to_owned(),
                record_id: "record-1".to_owned(),
                intent: json!({ "input": { "id": "record-1" } }),
                created_at_ms: 1,
            },
        )
        .unwrap();
        let admission = admit(
            &root,
            "cmd-runtime-snapshot",
            "record-workbench",
            "save",
            Some(1),
            json!({ "id": "record-1" }),
            "actor-1",
        )
        .unwrap();
        persist_admission(&root, "cmd-runtime-snapshot", &admission).unwrap();
        let original = admitted_snapshot(&root, "cmd-runtime-snapshot")
            .unwrap()
            .unwrap();

        let mut changed = manifest;
        changed["data_runtime"]["actions"]["save"]["steps"][0]["op"] =
            Value::String("insert".to_owned());
        fs::write(
            module_dir.join("module.json"),
            serde_json::to_vec_pretty(&changed).unwrap(),
        )
        .unwrap();
        assert_eq!(
            admitted_snapshot(&root, "cmd-runtime-snapshot")
                .unwrap()
                .unwrap(),
            original
        );
        let _ = fs::remove_dir_all(root);
    }
}
