//! E49.2 platform metadata in the canonical writer. No caller role, credential
//! value, remote MCP grant or restart timestamp can become effect authority.
use super::{commands::{self, CommandDocument}, *};
use crate::business_os::policy::{self, BusinessOsPermission, BusinessOsRole, BusinessOsScope};
use rusqlite::{params, OptionalExtension, Transaction};
use serde::Deserialize;
use serde_json::json;

pub(crate) fn handles(kind: &str) -> bool {
    matches!(kind, "ctox.secret.request.list" | "ctox.secret.request.fulfill" |
        "ctox.secret.request.decline" | "ctox.workjet.remote_worker.admit" |
        "ctox.maintenance.restart")
}
fn invalid() -> StoreError { StoreError::Invalid("invalid: platform request".into()) }
fn initialize(tx: &Transaction<'_>) -> Result<(), StoreError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS _platform_secret_requests(
        id TEXT PRIMARY KEY,app_id TEXT NOT NULL,user_id TEXT NOT NULL,
        secret_scope TEXT NOT NULL,secret_name TEXT NOT NULL,
        state TEXT NOT NULL CHECK(state IN ('pending','fulfilled','declined')),
        operation_id TEXT UNIQUE);
        CREATE TABLE IF NOT EXISTS _platform_worker_grants(
        command_id TEXT PRIMARY KEY,actor_id TEXT NOT NULL,app_id TEXT NOT NULL,
        device_id TEXT NOT NULL,enrollment_id TEXT NOT NULL,scopes TEXT NOT NULL,
        rights_ver INTEGER NOT NULL,expires_at_ms INTEGER NOT NULL,revoked INTEGER NOT NULL DEFAULT 0);
        CREATE TABLE IF NOT EXISTS _platform_maintenance_requests(
        command_id TEXT PRIMARY KEY,actor_id TEXT NOT NULL,
        state TEXT NOT NULL CHECK(state IN ('awaiting_reply','ready','claimed')));
        CREATE TABLE IF NOT EXISTS _platform_route_audit(
        command_id TEXT PRIMARY KEY,actor_id TEXT NOT NULL,kind TEXT NOT NULL,decision TEXT NOT NULL);")?;
    Ok(())
}
/// Native producer only: no wire deserializer or value field. Metadata shares
/// the command writer; secret bytes belong exclusively to the SecretStore.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn request_secret(tx: &Transaction<'_>, id: &str, app: &str, user: &str,
    scope: &str, name: &str) -> Result<(), StoreError> {
    if [id,app,user,scope,name].iter().any(|s|s.is_empty() || s.len()>180 || s.chars().any(char::is_control)) { return Err(invalid()); }
    initialize(tx)?;
    tx.execute("INSERT INTO _platform_secret_requests VALUES(?1,?2,?3,?4,?5,'pending',NULL)",params![id,app,user,scope,name])?;
    Ok(())
}
fn allowed(tx: &Transaction<'_>, actor: &BusinessOsActor, kind: &str) -> bool {
    let right = if kind.starts_with("ctox.secret.") { BusinessOsPermission::SecretsManage }
        else if kind == "ctox.workjet.remote_worker.admit" { BusinessOsPermission::MachinesManage }
        else if kind == "ctox.maintenance.restart" { BusinessOsPermission::RuntimeManage }
        else { BusinessOsPermission::IntegrationsManage };
    let restart_admin = kind != "ctox.maintenance.restart" || actor.role == BusinessOsRole::Admin;
    restart_admin && actor.id.as_deref().is_some_and(|id|!id.is_empty()) &&
        policy::evaluate_in_store(tx, actor, right, &BusinessOsScope::workspace()).allowed
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestId { request_id: String }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Admission { device_id: String, enrollment_id: String, scopes: Vec<String>, ttl_ms: u64 }
fn deny(code: &str, reason: &str) -> Value {
    commands::failure("rejected",code,reason,"payload","Platform request denied.")
}
pub(super) fn execute(tx: &Transaction<'_>, actor: &BusinessOsActor,
    command: &CommandDocument, now: u64) -> Result<Value, StoreError> {
    initialize(tx)?;
    let admitted = allowed(tx, actor, &command.command_type);
    tx.execute("INSERT OR IGNORE INTO _platform_route_audit VALUES(?1,?2,?3,?4)",
        params![command.id,actor.id,command.command_type,if admitted {"admitted"} else {"denied"}])?;
    let result = if !admitted { deny("forbidden","permission_denied") }
    else { match command.command_type.as_str() {
        "ctox.secret.request.list" => {
            if command.payload != json!({}) { deny("invalid","payload_invalid") } else {
                let rows = tx.prepare("SELECT id,app_id,user_id,state FROM _platform_secret_requests ORDER BY id")?
                    .query_map([],|r|Ok(json!({"request_id":r.get::<_,String>(0)?,"app_id":r.get::<_,String>(1)?,"user_id":r.get::<_,String>(2)?,"status":r.get::<_,String>(3)?})))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                json!({"requests":rows})
            }
        }
        "ctox.secret.request.fulfill" | "ctox.secret.request.decline" => {
            let request: RequestId = serde_json::from_value(command.payload.clone()).map_err(|_|invalid())?;
            let state: Option<String> = tx.query_row("SELECT state FROM _platform_secret_requests WHERE id=?1",[&request.request_id],|r|r.get(0)).optional()?;
            match state.as_deref() {
                None => deny("not_found","secret_request_missing"),
                Some("pending") if command.command_type.ends_with("decline") => {
                    tx.execute("UPDATE _platform_secret_requests SET state='declined',operation_id=?2 WHERE id=?1 AND state='pending'",params![request.request_id,command.id])?;
                    json!({"request_id":request.request_id,"declined":true})
                }
                // Never mark fulfilled before the protected provider commits.
                // A native provider must consume command-bound intake and call
                // finish_fulfill in the same retained invocation. No raw-value
                // fallback and no second RxDB credential ledger exists.
                Some("pending") => deny("unavailable","protected_secret_intake_required"),
                _ => deny("conflict","secret_request_closed"),
            }
        }
        "ctox.workjet.remote_worker.admit" => {
            let a: Admission = serde_json::from_value(command.payload.clone()).map_err(|_|invalid())?;
            if a.ttl_ms == 0 || a.ttl_ms > 900_000 || a.scopes.is_empty() || a.scopes.len()>16 ||
                a.scopes.iter().any(|s|!matches!(s.as_str(),"workspace.read"|"workspace.write"|"task.execute")) {
                deny("invalid","grant_scope_invalid")
            } else {
                let device = commands::get(tx,"devices",&a.device_id)?;
                let enrollment = commands::get(tx,"machine_enrollments",&a.enrollment_id)?;
                let owner = actor.id.as_deref().unwrap();
                let current = device.as_ref().is_some_and(|d|d["kind"]=="machine" && d["user_id"]==owner && d["machine_enrollment_id"]==a.enrollment_id && d["revoked"]!=true) &&
                    enrollment.as_ref().is_some_and(|e|e["status"]=="green" && e["created_by"]==owner);
                if !current { deny("forbidden","machine_binding_invalid") } else {
                    let rights=policy::sync_rights::read(tx).map_err(|e|StoreError::Invalid(e.to_string()))?;
                    let expires = now.checked_add(a.ttl_ms).ok_or_else(invalid)?;
                    tx.execute("INSERT INTO _platform_worker_grants(command_id,actor_id,app_id,device_id,enrollment_id,scopes,rights_ver,expires_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![command.id,owner,command.app_id,a.device_id,a.enrollment_id,serde_json::to_string(&a.scopes).map_err(|_|invalid())?,rights.rights_ver,expires])?;
                    json!({"grant":{"id":command.id,"actor_id":owner,"app_id":command.app_id,"device_id":a.device_id,"enrollment_id":a.enrollment_id,"scopes":a.scopes,"rights_ver":rights.rights_ver.to_string(),"expires_at_ms":expires}})
                }
            }
        }
        "ctox.maintenance.restart" => {
            if command.payload != json!({}) { deny("invalid","payload_invalid") } else {
                tx.execute("INSERT INTO _platform_maintenance_requests VALUES(?1,?2,'awaiting_reply')",params![command.id,actor.id])?;
                json!({"restart_requested":true,"awaiting_reply":true})
            }
        }
        _ => return Err(invalid()),
    }};
    commands::record(tx,actor,command,result,now)
}
/// Only the native egress owner calls this AFTER the committed result was sent.
/// A command submission, timer or caller-supplied boolean cannot trigger restart.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn reply_published(tx: &Transaction<'_>, command_id: &str) -> Result<bool, StoreError> {
    initialize(tx)?;
    let Some(doc)=commands::get(tx,"business_commands",command_id)? else { return Ok(false) };
    if !matches!(doc["status"].as_str(),Some("done"|"completed")) || doc["result"]["restart_requested"]!=true { return Ok(false) }
    Ok(tx.execute("UPDATE _platform_maintenance_requests SET state='ready' WHERE command_id=?1 AND state='awaiting_reply'",[command_id])?==1)
}
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn claim_restart(tx: &Transaction<'_>, command_id: &str) -> Result<bool, StoreError> {
    initialize(tx)?;
    let owner: Option<String> = tx.query_row("SELECT actor_id FROM _platform_maintenance_requests WHERE command_id=?1 AND state='ready'",[command_id],|r|r.get(0)).optional()?;
    let Some(owner)=owner else { return Ok(false) };
    let actor=match super::runtime_host::actor(tx,&owner) { Ok(actor)=>actor,Err(_)=>return Ok(false) };
    if !allowed(tx,&actor,"ctox.maintenance.restart") { return Ok(false) }
    Ok(tx.execute("UPDATE _platform_maintenance_requests SET state='claimed' WHERE command_id=?1 AND state='ready'",[command_id])?==1)
}

/// Native execution-point read. The tuple comes from the admitted invocation,
/// not a deserialized grant document. Revocation, role/epoch changes, expiry and
/// device replacement all fence a previously issued grant in this snapshot.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn worker_grant(tx: &Transaction<'_>, id: &str, actor: &BusinessOsActor,
    app: &str, device_id: &str, now: u64) -> Result<Option<Value>, StoreError> {
    initialize(tx)?;
    let Some(owner_id)=actor.id.as_deref() else { return Ok(None) };
    if !super::runtime_host::actor(tx,owner_id).is_ok_and(|current|current==*actor) { return Ok(None) }
    if !allowed(tx,actor,"ctox.workjet.remote_worker.admit") { return Ok(None) }
    let row:Option<(String,String,String,String,String,u64,u64,bool)>=tx.query_row(
        "SELECT actor_id,app_id,device_id,enrollment_id,scopes,rights_ver,expires_at_ms,revoked FROM _platform_worker_grants WHERE command_id=?1",[id],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?))).optional()?;
    let Some((owner,source,target,enrollment,scopes,version,expires,revoked))=row else { return Ok(None) };
    let rights=policy::sync_rights::read(tx).map_err(|e|StoreError::Invalid(e.to_string()))?;
    if actor.id.as_deref()!=Some(owner.as_str()) || source!=app || target!=device_id || revoked || now>=expires || version!=rights.rights_ver { return Ok(None) }
    let device=commands::get(tx,"devices",device_id)?;
    let machine=commands::get(tx,"machine_enrollments",&enrollment)?;
    if !device.as_ref().is_some_and(|d|d["kind"]=="machine" && d["user_id"]==owner && d["machine_enrollment_id"]==enrollment && d["revoked"]!=true) ||
        !machine.as_ref().is_some_and(|m|m["status"]=="green" && m["created_by"]==owner) { return Ok(None) }
    let scopes:Value=serde_json::from_str(&scopes).map_err(|_|invalid())?;
    Ok(Some(json!({"id":id,"actor_id":owner,"app_id":source,"device_id":target,"enrollment_id":enrollment,"scopes":scopes,"rights_ver":version.to_string(),"expires_at_ms":expires})))
}

#[cfg(test)]
#[path="platform_routes_tests.rs"]
mod tests;
