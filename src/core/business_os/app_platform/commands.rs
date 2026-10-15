// Origin: CTOX
// License: AGPL-3.0-only

use super::super::super::policy::{self, BusinessOsActor, BusinessOsPermission, BusinessOsScope};
use ctox_store::change_log::subscription::protocol::InstanceSigner;
use ctox_store::{
    change_log,
    schema::{self, CollectionDefinition, PublishLock, SchemaChange},
    types::StoreError,
};
use rusqlite::{params, OptionalExtension, Transaction};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(crate) struct CommandDocument {
    pub id: String,
    /// The calling app comes from the command document, not payload metadata.
    pub app_id: String,
    pub module: String,
    pub command_type: String,
    pub payload: Value,
}

pub(crate) fn handles(kind: &str) -> bool {
    matches!(
        kind,
        "ctox.app.publish"
            | "ctox.app.update"
            | "ctox.app.rollback"
            | "ctox.app.release"
            | "ctox.app.remove"
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    app_id: String,
    // Explicitly required, including null on installation. Missing is invalid.
    base_app_ver: Value,
    draft_id: Option<String>,
    release_ver: Option<String>,
    target_ver: Option<String>,
    channel: Option<String>,
    #[serde(default)]
    discard_local: bool,
}

fn invalid(message: impl Into<String>) -> StoreError {
    StoreError::Invalid(message.into())
}
fn text<'a>(value: &'a Value, name: &str) -> Result<&'a str, StoreError> {
    value
        .get(name)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid(format!("missing {name}")))
}
fn required(value: &Option<String>, name: &str) -> Result<String, StoreError> {
    value
        .clone()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid(format!("missing {name}")))
}
fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
fn table(tx: &Transaction<'_>, name: &str) -> Result<(String, u32), StoreError> {
    let collection = schema::active_collection(tx, name)?
        .ok_or_else(|| invalid(format!("platform collection not initialized: {name}")))?;
    Ok((quoted(&collection.table), collection.definition.schema_ver))
}
pub(super) fn get(
    tx: &Transaction<'_>,
    collection: &str,
    id: &str,
) -> Result<Option<Value>, StoreError> {
    let (table, _) = table(tx, collection)?;
    let raw: Option<String> = tx
        .query_row(
            &format!("SELECT data FROM {table} WHERE id=?1 AND deleted=0"),
            [id],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|raw| serde_json::from_str(&raw).map_err(|e| invalid(e.to_string())))
        .transpose()
}
pub(super) fn put(
    tx: &Transaction<'_>,
    collection: &str,
    id: &str,
    body: &Value,
) -> Result<(), StoreError> {
    let (table, schema_ver) = table(tx, collection)?;
    let body = serde_json::to_string(body).map_err(|e| invalid(e.to_string()))?;
    let rev = format!("native:{:x}", Sha256::digest(body.as_bytes()));
    tx.execute(&format!("INSERT INTO {table}(id,rev,schema_ver,data,deleted) VALUES(?1,?2,?3,?4,0)
        ON CONFLICT(id) DO UPDATE SET rev=excluded.rev,schema_ver=excluded.schema_ver,data=excluded.data,deleted=0"),
        params![id, rev, schema_ver, body])?;
    Ok(())
}
fn catalog_contains(tx: &Transaction<'_>, app: &str, version: &str) -> Result<bool, StoreError> {
    let (table, _) = table(tx, "app_catalog")?;
    Ok(tx.query_row(
        &format!(
            "SELECT EXISTS(SELECT 1 FROM {table} WHERE deleted=0
        AND json_extract(data,'$.app_id')=?1 AND json_extract(data,'$.app_ver')=?2)"
        ),
        params![app, version],
        |r| r.get(0),
    )?)
}
/// Declaration gate only; the caller must also enforce native user policy.
/// Resolve both active pointers in the same writer transaction. Prepared/old
/// versions never become an IPC registry and no restart/cache refresh is needed.
pub(crate) fn ipc_declarations_allow(
    tx: &Transaction<'_>,
    sender: &str,
    receiver: &str,
    kind: &str,
) -> Result<bool, StoreError> {
    if sender.is_empty() || receiver.is_empty() || kind.is_empty() {
        return Ok(false);
    }
    if sender == receiver {
        return Ok(true);
    }
    let Some(source) = active_manifest(tx, sender)? else {
        return Ok(false);
    };
    let Some(target) = active_manifest(tx, receiver)? else {
        return Ok(false);
    };
    let outgoing = source
        .get("commands_out")
        .and_then(Value::as_array)
        .is_some_and(|edges| {
            edges
                .iter()
                .any(|edge| edge["app"] == receiver && edge["type"] == kind)
        });
    if !outgoing {
        return Ok(false);
    }
    let incoming = target
        .get("commands_in")
        .and_then(Value::as_array)
        .is_some_and(|edges| {
            edges.iter().any(|edge| {
                edge["type"] == kind
                    && (edge["from"] == "*"
                        || edge["from"]
                            .as_array()
                            .is_some_and(|apps| apps.iter().any(|app| app == sender)))
            })
        });
    Ok(incoming)
}
fn active_manifest(tx: &Transaction<'_>, app_id: &str) -> Result<Option<Value>, StoreError> {
    let Some(app) = get(tx, "business_apps", app_id)? else {
        return Ok(None);
    };
    if app["state"] != "active" {
        return Ok(None);
    }
    let version = text(&app, "active_ver")?;
    let Some(saved) = get(tx, "business_app_versions", &version_id(app_id, version))? else {
        return Err(invalid("active IPC manifest missing"));
    };
    if saved["app_id"] != app_id || saved["app_ver"] != version || saved["state"] != "published" {
        return Err(invalid("active IPC version binding differs"));
    }
    Ok(Some(saved["manifest"].clone()))
}
fn validate_ipc(manifest: &Value) -> Result<(), StoreError> {
    for key in ["commands_in", "commands_out"] {
        let Some(raw) = manifest.get(key) else {
            continue;
        };
        let edges = raw
            .as_array()
            .ok_or_else(|| invalid("IPC declarations must be arrays"))?;
        for edge in edges {
            text(edge, "type")?;
            if key == "commands_out" {
                text(edge, "app")?;
            } else if edge["from"] != "*"
                && !edge["from"].as_array().is_some_and(|apps| {
                    apps.iter()
                        .all(|app| app.as_str().is_some_and(|id| !id.is_empty()))
                })
            {
                return Err(invalid("commands_in.from must be '*' or app ids"));
            }
        }
    }
    Ok(())
}

fn permission(
    actor: &BusinessOsActor,
    permission: BusinessOsPermission,
    app: Option<&Value>,
    app_id: &str,
) -> bool {
    let assigned = actor.id.as_deref().is_some_and(|id| {
        app.and_then(|a| a.pointer("/provenance/owner_id"))
            .and_then(Value::as_str)
            == Some(id)
    });
    policy::evaluate(
        actor,
        permission,
        &BusinessOsScope::module(app_id, assigned),
    )
    .allowed
}

pub(crate) fn can_read_draft(actor: &BusinessOsActor, draft: &Value, app: Option<&Value>) -> bool {
    if actor.id.as_deref().is_none_or(str::is_empty) {
        return false;
    }
    actor
        .id
        .as_deref()
        .is_some_and(|id| draft.get("user_id").and_then(Value::as_str) == Some(id))
        || permission(
            actor,
            BusinessOsPermission::AppsModify,
            app,
            draft.get("app_id").and_then(Value::as_str).unwrap_or(""),
        )
}
pub(crate) fn can_write_draft(actor: &BusinessOsActor, draft: &Value) -> bool {
    actor.id.as_deref().is_some_and(|id| {
        !id.is_empty() && draft.get("user_id").and_then(Value::as_str) == Some(id)
    })
}

fn result(code: &str) -> Value {
    json!({"status":"rejected", "code":code})
}
fn record(
    tx: &Transaction<'_>,
    actor: &BusinessOsActor,
    command: &CommandDocument,
    outcome: Value,
    now: u64,
) -> Result<Value, StoreError> {
    put(
        tx,
        "business_commands",
        &command.id,
        &json!({
            "id":command.id,"command_id":command.id,"app_id":command.app_id,"module":command.module,"command_type":command.command_type,
            "payload":command.payload,"client_context":{"actor":{"id":actor.id}},
            "status":if outcome["status"] == "preparing" {"running"} else if outcome["status"] == "rejected" {"failed"} else {"completed"},
            "result":outcome,"updated_at_ms":now
        }),
    )?;
    Ok(outcome)
}

pub(super) fn sign_manifest(
    signer: &dyn InstanceSigner,
    bytes: &[u8],
) -> Result<String, StoreError> {
    let signature = signer
        .sign(bytes)
        .map_err(|_| invalid("instance signing authority unavailable"))?;
    if signature.len() != 64 {
        return Err(invalid(
            "instance signer did not return an Ed25519 signature",
        ));
    }
    Ok(signature.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub(super) fn execute(
    tx: &Transaction<'_>,
    actor: &BusinessOsActor,
    command: &CommandDocument,
    now: u64,
    signer: &dyn InstanceSigner,
    migrations: &(dyn Fn(&CollectionDefinition, &CollectionDefinition, &Value) -> bool
          + Send
          + Sync),
) -> Result<Value, StoreError> {
    if !handles(&command.command_type)
        || command.id.is_empty()
        || command.app_id.is_empty()
        || command.module.is_empty()
    {
        return Err(invalid("invalid platform command document"));
    }
    if let Some(saved) = get(tx, "business_commands", &command.id)? {
        if saved["payload"] != command.payload
            || saved["app_id"] != command.app_id
            || saved["module"] != command.module
            || saved["command_type"] != command.command_type
            || saved.pointer("/client_context/actor/id") != Some(&json!(actor.id))
        {
            return Err(invalid("command id is bound to another request or actor"));
        }
        // Intake may already have stored the admitted command. A pending
        // document is not a completed native receipt and must still execute.
        if matches!(
            saved["status"].as_str(),
            Some("running" | "completed" | "failed")
        ) && saved.get("result").is_some_and(|v| !v.is_null())
        {
            return Ok(saved["result"].clone());
        }
    }
    let request: Request =
        serde_json::from_value(command.payload.clone()).map_err(|e| invalid(e.to_string()))?;
    if request.app_id.is_empty()
        || !(request.base_app_ver.is_null()
            || request.base_app_ver.as_str().is_some_and(|s| !s.is_empty()))
    {
        return Err(invalid("app_id and explicit base_app_ver are required"));
    }
    // serde treats a missing Value field as null; require the member separately.
    if command.payload.get("base_app_ver").is_none() {
        return Err(invalid("missing base_app_ver"));
    }
    let current = get(tx, "business_apps", &request.app_id)?;
    let required_permission = match command.command_type.as_str() {
        "ctox.app.publish" if current.is_none() => BusinessOsPermission::AppsInstall,
        "ctox.app.publish" => BusinessOsPermission::AppsModify,
        "ctox.app.update" => BusinessOsPermission::AppsInstall,
        "ctox.app.rollback" => BusinessOsPermission::AppsRollback,
        "ctox.app.release" => BusinessOsPermission::AppsRelease,
        "ctox.app.remove" => BusinessOsPermission::AppsUninstall,
        _ => unreachable!(),
    };
    let reject = |code| record(tx, actor, command, result(code), now);
    if actor.id.as_deref().is_none_or(str::is_empty)
        || !permission(
            actor,
            required_permission,
            current.as_ref(),
            &request.app_id,
        )
    {
        // guard: forbidden
        return reject("forbidden");
    }
    if request.discard_local
        && !permission(
            actor,
            BusinessOsPermission::AppsModify,
            current.as_ref(),
            &request.app_id,
        )
    {
        return reject("forbidden");
    }
    let active = current
        .as_ref()
        .map(|a| a["active_ver"].clone())
        .unwrap_or(Value::Null);
    if active != request.base_app_ver {
        return reject("stale_base");
    } // guard: stale_base
    if current.is_none()
        && !matches!(
            command.command_type.as_str(),
            "ctox.app.publish" | "ctox.app.update"
        )
    {
        return reject("not_found");
    }
    if command.command_type == "ctox.app.release" {
        let channel = required(&request.channel, "channel")?;
        let version = text(current.as_ref().unwrap(), "active_ver")?;
        let published = get(
            tx,
            "business_app_versions",
            &version_id(&request.app_id, version),
        )?
        .ok_or_else(|| invalid("active app version missing"))?;
        if published["state"] != "published" {
            return reject("not_found");
        }
        let id = format!("{}:{}:{}", request.app_id, version, channel);
        put(
            tx,
            "app_catalog",
            &id,
            &json!({"id":id,"app_id":request.app_id,"app_ver":version,
            "title":current.as_ref().unwrap()["title"],"layout":current.as_ref().unwrap()["layout"],"source":"instance","published_at":now}),
        )?; // effect: release
        let mut app = current.clone().unwrap();
        app["updated_at_ms"] = json!(now);
        app["provenance"]["release"] = json!({"channel":channel,"app_ver":version,"at_seq":change_log::head(tx)?.head_seq.to_string()});
        put(tx, "business_apps", &request.app_id, &app)?;
        return record(
            tx,
            actor,
            command,
            json!({"status":"accepted","app_ver":version,"channel":channel}),
            now,
        );
    }
    if command.command_type == "ctox.app.update"
        && current
            .as_ref()
            .is_some_and(|a| a["locally_modified"] == true)
    {
        if !request.discard_local {
            return reject("locally_modified");
        } // guard: locally_modified
    }
    let mut local_target = command.command_type == "ctox.app.publish";
    let mut base_release = current
        .as_ref()
        .and_then(|a| a.pointer("/provenance/base_release"))
        .cloned()
        .unwrap_or(Value::Null);
    let mut base_hash = current
        .as_ref()
        .and_then(|a| a.pointer("/provenance/base_hash"))
        .cloned()
        .unwrap_or(Value::Null);
    let mut manifest = match command.command_type.as_str() {
        "ctox.app.publish" => {
            let draft = get(tx, "app_drafts", &required(&request.draft_id, "draft_id")?)?;
            let Some(draft) = draft else {
                return reject("not_found");
            };
            if !can_read_draft(actor, &draft, current.as_ref()) {
                return reject("forbidden");
            }
            if draft["app_id"] != request.app_id {
                return reject("not_found");
            }
            if draft["base_app_ver"] != request.base_app_ver {
                return reject("stale_base");
            }
            let mut manifest = draft["manifest_fields"].clone();
            if !manifest.is_object() {
                return Err(invalid("draft manifest_fields must be an object"));
            }
            manifest["files"] = draft["files"].clone();
            manifest["collections"] = draft["collections_schema"].clone();
            manifest
        }
        "ctox.app.update" | "ctox.app.rollback" => {
            let target = if command.command_type == "ctox.app.update" {
                required(&request.release_ver, "release_ver")?
            } else {
                required(&request.target_ver, "target_ver")?
            };
            if command.command_type == "ctox.app.update"
                && !catalog_contains(tx, &request.app_id, &target)?
            {
                // guard: not_found
                return reject("not_found");
            }
            let Some(version) = get(
                tx,
                "business_app_versions",
                &version_id(&request.app_id, &target),
            )?
            else {
                return reject("not_found");
            };
            if version["app_id"] != request.app_id
                || version["app_ver"] != target
                || !matches!(version["state"].as_str(), Some("published" | "archived"))
            {
                return reject("not_found");
            }
            local_target =
                command.command_type == "ctox.app.rollback" && version["origin"] == "local";
            let manifest = version["manifest"].clone();
            let bytes = serde_json::to_vec(&manifest).map_err(|e| invalid(e.to_string()))?;
            if version["manifest_sha256"] != format!("{:x}", Sha256::digest(&bytes)) {
                return Err(invalid("stored manifest hash differs"));
            }
            base_release = json!(target);
            base_hash = version["manifest_sha256"].clone();
            manifest
        }
        "ctox.app.remove" => json!({"app_id":request.app_id,"collections":{}}),
        _ => unreachable!(),
    };
    let head = change_log::head(tx)?;
    let version = format!(
        "{}:{}",
        head.epoch,
        head.head_seq
            .checked_add(1)
            .ok_or_else(|| invalid("sequence overflow"))?
    );
    manifest["app_id"] = json!(request.app_id);
    manifest["app_ver"] = json!(version);
    manifest["author"] = json!(actor.id);
    manifest["created_at"] = json!(now);
    manifest["provenance"] =
        json!({"base_release":base_release,"base_hash":base_hash,"locally_modified":local_target});
    if serde_json::to_vec(&manifest)
        .map_err(|e| invalid(e.to_string()))?
        .len()
        > 1_048_576
    {
        return Err(invalid("manifest exceeds draft byte budget"));
    }
    validate_ipc(&manifest)?;
    let mut definitions = definitions(&manifest, &request.app_id, &version)?;
    let prior_manifest = if let Some(active) = active.as_str() {
        get(
            tx,
            "business_app_versions",
            &version_id(&request.app_id, active),
        )?
        .map(|v| v["manifest"].clone())
    } else {
        None
    };
    // Only a native runner resolution permits a generation migration.
    let mut generation_changes = Vec::new();
    for definition in definitions.values_mut() {
        let foreign: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM _collections WHERE name=?1 AND app_id<>?2)",
            params![definition.name, request.app_id],
            |r| r.get(0),
        )?;
        if foreign {
            return reject("forbidden");
        }
        if let Some(old) = schema::active_collection(tx, &definition.name)? {
            if old.definition.app_id != request.app_id {
                return reject("forbidden");
            }
            definition.rights_ver = old.definition.rights_ver;
            let before = prior_manifest
                .as_ref()
                .ok_or_else(|| invalid("active manifest missing"))?
                .get("collections")
                .and_then(|c| c.get(&definition.name))
                .and_then(|c| c.get("rights"));
            let after = manifest["collections"][&definition.name].get("rights");
            if before != after {
                definition.rights_ver = definition
                    .rights_ver
                    .checked_add(1)
                    .ok_or_else(|| invalid("rights version overflow"))?;
            }
            let older = definition.schema_ver < old.definition.schema_ver;
            let incompatible = older
                || schema::classify_change(&old.definition, definition)?
                    == SchemaChange::Incompatible;
            if incompatible {
                let declared = manifest
                    .get("migrations")
                    .and_then(|m| m.get(&definition.name))
                    .is_some_and(|m| {
                        m.is_object()
                            && m.get("from_ver").is_some_and(|v| !v.is_null())
                            && (m
                                .get("steps")
                                .and_then(Value::as_array)
                                .is_some_and(|s| !s.is_empty())
                                || m.get("migrate_fn")
                                    .and_then(Value::as_str)
                                    .is_some_and(|p| manifest["files"].get(p).is_some()))
                    });
                let migration_supported =
                    declared && migrations(&old.definition, definition, &manifest);
                if !migration_supported {
                    return reject("migration_required");
                } // guard: migration_required
                  // A rollback is forward publication; physical schema versions
                  // never decrease even when restoring older schema semantics.
                definition.schema_ver = old
                    .definition
                    .schema_ver
                    .checked_add(1)
                    .ok_or_else(|| invalid("schema version overflow"))?;
                definition.schema_json["version"] = json!(definition.schema_ver);
                if schema::classify_change(&old.definition, definition)?
                    == SchemaChange::Incompatible
                {
                    generation_changes.push(definition.name.clone());
                }
            }
        }
    }
    for definition in definitions.values() {
        manifest["collections"][&definition.name]["schema"] = definition.schema_json.clone();
    }
    // No business rejection after this point: operational errors roll the whole
    // writer job back, including schema preparation and local-version archive.
    let lock = PublishLock {
        app_id: request.app_id.clone(),
        base_app_ver: active.as_str().unwrap_or("").into(),
        target_app_ver: version.clone(),
        operation_id: command.id.clone(),
    };
    schema::reserve_publish(tx, &lock)?;
    let old_collections = {
        let mut statement = tx.prepare(
            "SELECT name FROM _collections WHERE app_id=?1 AND state='active' ORDER BY name",
        )?;
        let rows = statement
            .query_map([&request.app_id], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    for name in old_collections {
        if !definitions.contains_key(&name) {
            schema::archive_collection(tx, &lock, &name, (now / 1000) as i64)?;
        }
    }
    for definition in definitions.values() {
        if schema::active_collection(tx, &definition.name)?.is_some()
            && !generation_changes.contains(&definition.name)
        {
            schema::update_compatible(tx, &lock, definition)?;
        } else {
            schema::prepare_migration(tx, &lock, definition)?;
        }
    }
    let bytes = serde_json::to_vec(&manifest).map_err(|e| invalid(e.to_string()))?;
    if bytes.len() > 1_048_576 {
        return Err(invalid("manifest exceeds draft byte budget"));
    }
    let signature = sign_manifest(signer, &bytes)?;
    let id = version_id(&request.app_id, &version);
    put(
        tx,
        "business_app_versions",
        &id,
        &json!({"id":id,"app_id":request.app_id,"app_ver":version,
        "manifest":manifest,"manifest_sha256":format!("{:x}",Sha256::digest(&bytes)),"signature":signature,
        "base_app_ver":request.base_app_ver,"state":"preparing","origin":if local_target {"local"} else {"release"},"status":"preparing","created_at_ms":now}),
    )?;
    let pending = json!({"status":"preparing","app_id":request.app_id,"app_ver":version,
        "publication":lock,"remove":command.command_type == "ctox.app.remove",
        "locally_modified":local_target,
        "discard_local":command.command_type == "ctox.app.update" && request.discard_local
            && current.as_ref().is_some_and(|a| a["locally_modified"] == true),
        "owner_id":current.as_ref().and_then(|a| a.pointer("/provenance/owner_id")).cloned().unwrap_or(json!(actor.id)),
        "calling_app":command.app_id});
    record(tx, actor, command, pending, now)?;
    // A populated compatible collection may require the bounded online-index
    // runner. Its completion resumes through `Platform::complete`.
    let preparation_pending: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM _index_builds WHERE publication_id=?1 AND state='copying') OR EXISTS(SELECT 1 FROM _migrations WHERE publication_id=?1 AND state<>'ready')", [&command.id], |r| r.get(0))?;
    if preparation_pending {
        return Ok(get(tx, "business_commands", &command.id)?.unwrap()["result"].clone());
    }
    complete(tx, actor, &command.id, now)
}

pub(super) fn version_id(app: &str, version: &str) -> String {
    format!("{app}@{version}")
}

fn definitions(
    manifest: &Value,
    app: &str,
    version: &str,
) -> Result<BTreeMap<String, CollectionDefinition>, StoreError> {
    let collections = manifest
        .get("collections")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("manifest collections must be an object"))?;
    collections
        .iter()
        .map(|(name, def)| {
            let schema = def
                .get("schema")
                .filter(|s| s.is_object())
                .ok_or_else(|| invalid("collection schema missing"))?;
            let number = schema.get("version").and_then(Value::as_u64).unwrap_or(0);
            Ok((
                name.clone(),
                CollectionDefinition {
                    name: name.clone(),
                    app_id: app.into(),
                    schema_ver: u32::try_from(number)
                        .map_err(|_| invalid("schema version overflow"))?,
                    schema_json: schema.clone(),
                    class: text(def, "class")?.into(),
                    client_limit: def["client_limit"].as_u64().unwrap_or(0),
                    byte_budget: def["byte_budget"]
                        .as_u64()
                        .ok_or_else(|| invalid("byte_budget missing"))?,
                    rights_ver: 1,
                    app_ver: version.into(),
                },
            ))
        })
        .collect()
}

pub(super) fn complete(
    tx: &Transaction<'_>,
    actor: &BusinessOsActor,
    command_id: &str,
    now: u64,
) -> Result<Value, StoreError> {
    let mut command = get(tx, "business_commands", command_id)?
        .ok_or_else(|| invalid("publication command missing"))?;
    let pending = command["result"].clone();
    if pending["status"] != "preparing" {
        return Ok(pending);
    }
    let lock: PublishLock = serde_json::from_value(pending["publication"].clone())
        .map_err(|e| invalid(e.to_string()))?;
    let app = get(tx, "business_apps", &lock.app_id)?;
    let right = match command["command_type"].as_str() {
        Some("ctox.app.publish") if app.is_none() => BusinessOsPermission::AppsInstall,
        Some("ctox.app.publish") => BusinessOsPermission::AppsModify,
        Some("ctox.app.update") => BusinessOsPermission::AppsInstall,
        Some("ctox.app.rollback") => BusinessOsPermission::AppsRollback,
        Some("ctox.app.remove") => BusinessOsPermission::AppsUninstall,
        _ => return Err(invalid("invalid prepared publication command")),
    };
    if actor.id.as_deref().is_none_or(str::is_empty)
        || !permission(actor, right, app.as_ref(), &lock.app_id)
        || (pending["discard_local"] == true
            && !permission(
                actor,
                BusinessOsPermission::AppsModify,
                app.as_ref(),
                &lock.app_id,
            ))
    {
        schema::abort_publication(tx, &lock, (now / 1000) as i64)?;
        let id = version_id(&lock.app_id, &lock.target_app_ver);
        if let Some(mut version) = get(tx, "business_app_versions", &id)? {
            version["state"] = json!("failed");
            version["status"] = json!("failed");
            put(tx, "business_app_versions", &id, &version)?;
        }
        let outcome = result("forbidden");
        command["result"] = outcome.clone();
        command["status"] = json!("failed");
        command["updated_at_ms"] = json!(now);
        put(tx, "business_commands", command_id, &command)?;
        return Ok(outcome);
    }
    if app
        .as_ref()
        .and_then(|a| a["active_ver"].as_str())
        .unwrap_or("")
        != lock.base_app_ver
    {
        return Err(invalid(
            "publication base version changed before activation",
        ));
    }
    let watermark = schema::activate_publication(tx, &lock, (now / 1000) as i64)?;
    if pending["discard_local"] == true {
        // Archive the customized version in the pointer-switch transaction,
        // not while the replacement is still preparing.
        let id = version_id(&lock.app_id, &lock.base_app_ver);
        let mut local = get(tx, "business_app_versions", &id)?
            .ok_or_else(|| invalid("customized version missing"))?;
        local["origin"] = json!("local");
        local["status"] = json!("archived");
        local["state"] = json!("archived");
        put(tx, "business_app_versions", &id, &local)?;
    }
    if pending["remove"] == true {
        // Explicit app removal retains its data indefinitely. Ordinary old
        // migration generations may still have P3's bounded retention.
        tx.execute(
            "UPDATE _collections SET retire_after=NULL WHERE app_id=?1 AND state='archived'",
            [&lock.app_id],
        )?;
        tx.execute("UPDATE _archive SET expires_at=9223372036854775807 WHERE app_id=?1 AND reason='collection removed'", [&lock.app_id])?;
    }
    let id = version_id(&lock.app_id, &lock.target_app_ver);
    let mut version = get(tx, "business_app_versions", &id)?
        .ok_or_else(|| invalid("prepared version missing"))?;
    version["state"] = json!(if pending["remove"] == true {
        "archived"
    } else {
        "published"
    });
    version["status"] = version["state"].clone();
    version["migrated_at_seq"] = json!(watermark.to_string());
    put(tx, "business_app_versions", &id, &version)?; // effect: publish/update/rollback
    let manifest = &version["manifest"];
    let mut provenance = manifest["provenance"].clone();
    provenance["owner_id"] = pending["owner_id"].clone();
    provenance["calling_app"] = pending["calling_app"].clone();
    // Role assignment is separate native policy, not a manifest field an app
    // editor can grant itself. New apps are initially visible to installers.
    let visible_roles = app
        .as_ref()
        .and_then(|a| a.get("visible_roles"))
        .cloned()
        .unwrap_or(json!(["chef", "admin"]));
    let layout = manifest
        .get("layout")
        .cloned()
        .or_else(|| app.as_ref().and_then(|a| a.get("layout")).cloned())
        .unwrap_or(json!({}));
    put(
        tx,
        "business_apps",
        &lock.app_id,
        &json!({"id":lock.app_id,"app_id":lock.app_id,
        "title":manifest.get("title").cloned().or_else(||app.as_ref().map(|a|a["title"].clone())).unwrap_or(json!(lock.app_id)),
        "active_ver":lock.target_app_ver,"migrated_at_seq":watermark.to_string(),
        "visible_roles":visible_roles,"layout":layout,
        "provenance":provenance,"locally_modified":pending["locally_modified"],
        "state":if pending["remove"] == true {"archived"} else {"active"},"updated_at_ms":now}),
    )?; // effect: remove
        // No app_activations row: publishing the active pointer is not proof that
        // any connected client has fetched code or reached this watermark.
    let outcome = json!({"status":"accepted","app_id":lock.app_id,"app_ver":lock.target_app_ver,"migrated_at_seq":watermark.to_string()});
    command["result"] = outcome.clone();
    command["status"] = json!("completed");
    command["updated_at_ms"] = json!(now);
    put(tx, "business_commands", command_id, &command)?;
    Ok(outcome)
}
