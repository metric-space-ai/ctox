//! Productive app host. The composition root lends every native owner (E22).
//! This module never creates a signing key, database owner or Tokio runtime.
mod authority;
mod codec;
mod feed;
mod origin;
pub(crate) use origin::{enqueue_core_command, CoreCommandSource};
#[cfg(all(test, unix, feature = "app-runtime-node"))]
mod native_proof;
#[cfg(test)]
mod runtime_tests;

use crate::{
    business_os::policy::{self, sync_rights, BusinessOsActor},
    service::store_runtime::Database,
};
use anyhow::{anyhow, Context, Result};
pub(crate) use authority::RuntimeAuthority;
pub(crate) use codec::RuntimeCodec;
use ctox_app_runtime::{
    core_host::CoreRuntimeHost,
    registration::{AppRuntimeInputs, AppRuntimeRegistration},
};
use ctox_store::{cas::Cas, JobOptions, ReadJob, StoreError};
use ctox_sync::authority::{client::ExecutionAuthority, Ownership};
pub(crate) use feed::RuntimeBinding;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, future::Future, sync::Arc, time::Duration};
use tokio::runtime::Handle;

/// Native-only constructor inputs, deliberately no Deserialize or wire builder.
/// All handles retain the root's existing generations and authority lifetime.
pub(crate) struct RuntimeOwners {
    pub root: std::path::PathBuf,
    pub sync: Database,
    pub secrets: Database,
    pub runtime: Handle,
    pub signer: super::Signer,
    pub execution: Arc<dyn ExecutionAuthority>,
    pub cas: Cas,
}
fn invalid(message: impl Into<String>) -> StoreError {
    StoreError::Invalid(message.into())
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("invalid: missing {key}"))
}
fn sql_error(e: impl std::fmt::Display) -> StoreError {
    invalid(e.to_string())
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
fn wait<T: Send + 'static>(
    handle: &Handle,
    future: impl Future<Output = Result<T, StoreError>> + Send + 'static,
) -> Result<T> {
    // Only native blocking callbacks use this bridge; never block a Tokio worker.
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    handle.spawn(async move {
        let _ = send.send(future.await);
    });
    receive
        .recv_timeout(Duration::from_secs(15))
        .context("unavailable: owner operation requires reconciliation")?
        .map_err(Into::into)
}
// Ordinary reads retain the store owner's admission and execution guards.
async fn observed_read<T: Send + 'static>(
    pool: ctox_store::read_pool::ReadPool,
    stage: &'static str,
    job: ReadJob<T>,
) -> Result<T, StoreError> {
    measured_read(pool, stage, job).await.0
}
async fn measured_read<T: Send + 'static>(
    pool: ctox_store::read_pool::ReadPool,
    stage: &'static str,
    job: ReadJob<T>,
) -> (Result<T, StoreError>, Option<u64>) {
    use std::sync::atomic::{AtomicU64, Ordering};
    let started = std::time::Instant::now();
    let cpu = Arc::new(AtomicU64::new(u64::MAX));
    let callback_cpu = cpu.clone();
    let entered = Arc::new(AtomicU64::new(u64::MAX));
    let finished = Arc::new(AtomicU64::new(u64::MAX));
    let enter = entered.clone();
    let finish = finished.clone();
    let result = pool
        .read_job(ReadJob::new(job.options, move |db| {
            enter.store(started.elapsed().as_micros() as u64, Ordering::Relaxed);
            let before_cpu = thread_cpu_us();
            let result = (job.operation)(db);
            if let (Some(before), Some(after)) = (before_cpu, thread_cpu_us()) {
                callback_cpu.store(after.saturating_sub(before), Ordering::Relaxed);
            }
            finish.store(started.elapsed().as_micros() as u64, Ordering::Relaxed);
            result
        }))
        .await;
    if let Err(error) = &result {
        eprintln!("native app store read failed stage={stage} elapsed_us={} entered_us={} finished_us={} callback_cpu_us={} error={error:?}", started.elapsed().as_micros(), entered.load(Ordering::Relaxed), finished.load(Ordering::Relaxed), cpu.load(Ordering::Relaxed));
    }
    let cost = cpu.load(Ordering::Relaxed);
    (result, (cost != u64::MAX).then_some(cost))
}
// CLOCK_THREAD_CPUTIME_ID excludes time when this blocking reader is descheduled.
#[cfg(target_os = "linux")]
fn thread_cpu_us() -> Option<u64> {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: time is an initialized, writable timespec; the thread clock has no handle lifetime.
    if unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut time) } == 0 {
        Some((time.tv_sec as u64).saturating_mul(1_000_000) + time.tv_nsec as u64 / 1_000)
    } else {
        None
    }
}
#[cfg(not(target_os = "linux"))]
fn thread_cpu_us() -> Option<u64> {
    None
}
#[derive(Clone)]
struct Collection {
    table: String,
    gen: u64,
    schema: Value,
    schema_ver: u32,
    app_ver: String,
    owner: String,
}
fn collection(db: &Connection, name: &str) -> Result<Collection, StoreError> {
    db.query_row("SELECT table_name,gen,schema_json,schema_ver,app_ver,app_id FROM _collections WHERE name=?1 AND state='active'", [name], |r| {
        Ok((r.get::<_,String>(0)?,r.get::<_,u64>(1)?,r.get::<_,String>(2)?,r.get::<_,u32>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?))
    }).map_err(Into::into).and_then(|(table,gen,schema,schema_ver,app_ver,owner)| Ok(Collection{table,gen,schema:serde_json::from_str(&schema).map_err(sql_error)?,schema_ver,app_ver,owner}))
}
pub(crate) fn document(db: &Connection, name: &str, id: &str) -> Result<Option<Value>, StoreError> {
    let c = collection(db, name)?;
    let raw: Option<String> = db
        .query_row(
            &format!(
                "SELECT data FROM {} WHERE id=?1 AND deleted=0",
                quote(&c.table)
            ),
            [id],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|s| serde_json::from_str(&s).map_err(sql_error))
        .transpose()
}
pub(crate) fn rights(db: &Connection) -> Result<sync_rights::RightsSnapshot, StoreError> {
    // Decode the existing P6 tables in the caller's already-open WAL snapshot.
    // Policy evaluation itself is the canonical policy.rs implementation.
    let rights_ver = db.query_row(
        "SELECT rights_ver FROM _rights_state WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    let mut statement=db.prepare("SELECT role,collection,row_filter_json,field_mask_json,ops FROM c_rights__g1 WHERE id!='0:' ORDER BY role,collection")?;
    let tuples = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let rules = tuples
        .into_iter()
        .map(|(role, collection, filter, mask, ops)| {
            Ok(sync_rights::Right {
                role,
                collection,
                row_filter: serde_json::from_str(&filter).map_err(sql_error)?,
                field_mask: serde_json::from_str(&mask).map_err(sql_error)?,
                ops: serde_json::from_str(&ops).map_err(sql_error)?,
            })
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    let raw: Option<String> = db
        .query_row("SELECT data FROM c_rights__g1 WHERE id='0:'", [], |r| {
            r.get(0)
        })
        .optional()?;
    let metadata: Value = raw
        .map(|v| serde_json::from_str(&v))
        .transpose()
        .map_err(sql_error)?
        .unwrap_or(json!({}));
    let manifests: Vec<sync_rights::manifest_rules::ManifestRules> =
        serde_json::from_value(metadata.get("manifests").cloned().unwrap_or(json!([])))
            .map_err(sql_error)?;
    for manifest in &manifests {
        manifest.validate().map_err(sql_error)?;
        manifest.validate_registry(&manifests).map_err(sql_error)?;
    }
    Ok(sync_rights::RightsSnapshot {
        rights_ver,
        rules,
        manifests,
    })
}
pub(crate) fn actor(db: &Connection, id: &str) -> Result<BusinessOsActor, StoreError> {
    let user = document(db, "business_users", id)?
        .ok_or_else(|| invalid("revoked: native user missing"))?;
    if user["active"] != true || user.get("user_id").and_then(Value::as_str).unwrap_or(id) != id {
        return Err(invalid("revoked: native user inactive"));
    }
    let role = user["role"]
        .as_str()
        .ok_or_else(|| invalid("revoked: native role missing"))?;
    if !matches!(role, "chef" | "admin" | "founder" | "user") {
        return Err(invalid("revoked: native role invalid"));
    }
    Ok(BusinessOsActor::new(Some(id.into()), role))
}
fn actor_json(a: &BusinessOsActor) -> Value {
    json!({"id":a.id,"role":a.role.as_str()})
}
fn assigned(db: &Connection, app: &str, id: &str) -> Result<bool, StoreError> {
    let record = document(db, "business_apps", app)?.ok_or_else(|| invalid("not_found: app"))?;
    Ok(record["assigned_user_ids"]
        .as_array()
        .is_some_and(|v| v.iter().any(|v| v == id)))
}
fn permit(
    db: &Connection,
    app: &str,
    a: &BusinessOsActor,
    kind: &str,
) -> Result<sync_rights::manifest_rules::CommandPermit, StoreError> {
    permit_with_rights(db, &rights(db)?, app, a, kind)
}
fn permit_with_rights(
    db: &Connection,
    snapshot: &sync_rights::RightsSnapshot,
    app: &str,
    a: &BusinessOsActor,
    kind: &str,
) -> Result<sync_rights::manifest_rules::CommandPermit, StoreError> {
    let id = a.id.as_deref().ok_or_else(|| invalid("forbidden: actor"))?;
    snapshot
        .authorize_command(app, id, a.role.as_str(), kind, assigned(db, app, id)?)
        .map_err(sql_error)
}

/// Construct on the existing blocking executor. No production test signer or
/// default authority exists; a detached root cannot produce an executable host.
pub(crate) fn host(
    owners: &RuntimeOwners,
) -> Result<(Arc<CoreRuntimeHost>, Arc<RuntimeAuthority>)> {
    for db in [&owners.sync, &owners.secrets] {
        anyhow::ensure!(
            db.writer.generation().0 == db.readers.generation(),
            "unavailable: owner generation changed"
        );
    }
    let codec = Arc::new(RuntimeCodec {
        signer: owners.signer.clone(),
        cas: owners.cas.clone(),
    });
    let authority = RuntimeAuthority::new(
        owners.sync.clone(),
        owners.runtime.clone(),
        owners.execution.clone(),
        codec.clone(),
    );
    let blobs = Arc::new(codec::RuntimeBlobs {
        database: owners.sync.clone(),
        runtime: owners.runtime.clone(),
        codec: codec.clone(),
        authority: authority.clone(),
    });
    let capabilities = Arc::new(crate::service::app_runtime_native::NativeCapabilityRouter {
        root: owners.root.clone(),
        authority: authority.clone(),
        blobs,
    });
    let host = Arc::new(CoreRuntimeHost::new(
        owners.sync.writer.clone(),
        owners.sync.readers.clone(),
        codec,
        authority.clone(),
        capabilities,
        owners.runtime.clone(),
    )?);
    Ok((host, authority))
}
fn error_code(message: &str) -> &str {
    match message.split(':').next().unwrap_or("") {
        "invalid" => "invalid",
        "forbidden" => "forbidden",
        "not_found" => "not_found",
        "conflict" => "conflict",
        "revoked" => "revoked",
        "expired" => "expired",
        "quota" => "quota",
        _ => "unavailable",
    }
}

impl RuntimeOwners {
    pub(crate) fn from_store_inputs(
        inputs: &crate::service::store_inputs::StoreInputs,
        signer: super::Signer,
        execution: Arc<dyn ExecutionAuthority>,
        cas: Cas,
    ) -> Result<Self, StoreError> {
        let secrets = inputs
            .databases
            .get("secrets")
            .cloned()
            .ok_or(StoreError::Closed)?;
        Ok(Self {
            root: inputs.root.clone(),
            sync: inputs.sync.clone(),
            secrets,
            runtime: inputs.runtime.clone(),
            signer,
            execution,
            cas,
        })
    }
}

pub(super) fn store_command(tx: &rusqlite::Transaction<'_>, doc: &Value) -> Result<(), StoreError> {
    let id = text(doc, "id").map_err(sql_error)?;
    let provenance = origin::before_native_write(tx, doc)?;
    super::commands::put(tx, "business_commands", id, doc)?;
    origin::after_native_write(tx, doc, provenance)?;
    let c = collection(tx, "business_commands")?;
    let (rev, seq): (String, u64) = tx.query_row(
        "SELECT rev,seq FROM _changes WHERE collection='business_commands' AND gen=?1 AND id=?2",
        params![c.gen, id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    ctox_store::change_log::subscription::write_contract::project_command(
        tx, id, c.gen, &rev, seq, doc,
    )?;
    Ok(())
}
/// Native publication participates in the same P6 policy transaction. Existing
/// grants are retained; new collections use policy.rs's native baseline only.
pub(super) fn publish_rights(
    tx: &rusqlite::Transaction<'_>,
    app: &str,
    version: &str,
    manifest: &Value,
) -> Result<(), StoreError> {
    let mut current = sync_rights::read(tx)?;
    let owned = manifest["collections"]
        .as_object()
        .ok_or_else(|| invalid("invalid: manifest collections"))?
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let additions = owned
        .iter()
        .filter(|name| !current.rules.iter().any(|r| &r.collection == *name))
        .cloned()
        .collect::<Vec<_>>();
    for rule in policy::base_release_rights(&additions)
        .into_iter()
        .filter(|r| additions.contains(&r.collection))
    {
        current.rules.push(rule);
    }
    let rules =
        sync_rights::manifest_rules::ManifestRules::from_manifest(app, version, owned, manifest)
            .map_err(sql_error)?;
    current.manifests.retain(|m| m.app_id() != app);
    current.manifests.push(rules);
    sync_rights::replace_with_manifests(
        tx,
        current.rights_ver,
        &current.rules,
        &current.manifests,
    )?;
    Ok(())
}

/// Called only by the native foreground root after its E22 session is retained.
/// Unenrolled storage-only roots have no execution capability to attach.
pub(crate) async fn attach_native(
    inputs: Arc<crate::service::store_inputs::StoreInputs>,
) -> Result<Option<Box<dyn crate::service::store_startup::RunningEngine>>, StoreError> {
    let native = match inputs.native_session() {
        Ok(native) => native,
        Err(StoreError::Closed) => return Ok(None),
        Err(error) => return Err(error),
    };
    let signer = native.instance_signer.clone();
    let execution = native.execution_authority.clone();
    // Existing Transfer-CAS layout: workjet_transfer_workspace.rs::store.
    // Cas is a filesystem handle, not another database or writer.
    let transfer_root = crate::paths::runtime_dir(&inputs.root).join("transfers");
    let cas = tokio::task::spawn_blocking(move || Cas::open(transfer_root))
        .await
        .map_err(|_| StoreError::Panicked)??;
    let owners = RuntimeOwners::from_store_inputs(&inputs, signer, execution, cas)?;
    let binding = RuntimeBinding::attach(inputs.root.clone(), owners)
        .await
        .map_err(sql_error)?;
    Ok(Some(Box::new(binding)))
}
