// Origin: CTOX
// License: AGPL-3.0-only
//! Standalone, admitted native Settings probes. No project or meeting lease.
use super::consumer_authority::{AdmittedConsumerAuthority, NativeConsumerCorePublication};
use crate::execution::{
    cliproxyapi_claude_sdk::{NativeClaudeSdkAccountReservation, NativeClaudeSdkPublicationCheck},
    cliproxyapi_model_probe::{check_claude_model, NativeModelProbe},
};
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection};
use rxdb::plugins::replication_webrtc::{
    connection_handler_rs::WebRTCRsConnectionHandler,
    index_mod::{GuardedAuxiliaryResponse, RxWebRTCReplicationPool},
    WebRTCPublicationGuard,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};

pub(super) const METHOD: &str = "ctox.workjet.models.check.v1";
const MAX_MODELS: usize = 256;
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Request {
    version: u32,
    op: String,
    command_id: String,
    account_id: String,
    account_revision: i64,
    model_id: String,
}
fn parse(params: Vec<Value>) -> Result<Request> {
    ensure!(params.len() == 1 && serde_json::to_vec(&params)?.len() <= 4096, "invalid model check request");
    let request: Request = serde_json::from_value(params.into_iter().next().unwrap())?;
    ensure!(request.version == 1 && request.op == "check" && request.account_revision > 0,
        "unsupported model check request");
    for (value, limit) in [(&request.command_id, 128), (&request.account_id, 256), (&request.model_id, 256)] {
        ensure!(!value.is_empty() && value.len() <= limit && value.trim() == value
            && !value.chars().any(char::is_control), "invalid model check identity");
    }
    Ok(request)
}

pub(super) fn register(
    pool: &Arc<RxWebRTCReplicationPool<WebRTCRsConnectionHandler>>,
    root: &Path,
) -> rxdb::rx_error::RxResult<()> {
    let root = root.to_owned();
    let transport = Arc::clone(&pool.connection_handler);
    pool.register_guarded_auxiliary_request_handler(METHOD, Arc::new(move |peer, admitted_token, params| {
        let root = root.clone();
        let transport = Arc::clone(&transport);
        Box::pin(async move {
            let request = parse(params).map_err(|_| "invalid native model check request".to_owned())?;
            // Capture ONLY the actual admitted peer/token. No JSON identity can
            // construct authority. Native store preparation is off the executor.
            let prepared = tokio::task::spawn_blocking(move || {
                let authority = AdmittedConsumerAuthority::capture(&root, transport, peer, &admitted_token)?;
                let reservation = NativeClaudeSdkAccountReservation::prepare_model_check(
                    &authority, &request.account_id, request.account_revision, &request.model_id)?;
                Ok::<_, anyhow::Error>((authority, reservation, request))
            }).await.map_err(|_| "native model check admission unavailable".to_owned())?
                .map_err(|_| "native model check account unavailable".to_owned())?;
            let (authority, reservation, request) = prepared;
            // This retained reservation pins the original private generation
            // through persistence and physical publication. The IO adapter also
            // checks its current authority during the bounded network request.
            let probe = check_claude_model(&authority, &request.account_id,
                request.account_revision, &request.model_id).await;
            tokio::task::spawn_blocking(move || {
                let private = NativeClaudeSdkPublicationCheck::prepare(&reservation)?;
                let consumer = authority.prepare_core_publication(&authority)?;
                let publication = Arc::new(Publication { consumer, private, reservation, request, probe });
                publication.retain()?;
                let result = json!({"version":1,"op":"check",
                    "commandId":publication.request.command_id,
                    "accountId":publication.request.account_id,
                    "accountRevision":publication.request.account_revision,
                    "probe":publication.probe});
                Ok::<_, anyhow::Error>(GuardedAuxiliaryResponse { result, publication })
            }).await.map_err(|_| "native model check result unavailable".to_owned())?
                .map_err(|_| "native model check result retired".to_owned())
        })
    }))
}

/// Never serializable. The pool adds its exact peer/token/lifecycle fence.
/// These preopened native checks do not reenter transport or secret APIs.
struct Publication {
    consumer: NativeConsumerCorePublication,
    private: NativeClaudeSdkPublicationCheck,
    reservation: NativeClaudeSdkAccountReservation,
    request: Request,
    probe: NativeModelProbe,
}
impl Publication {
    fn retain(&self) -> Result<()> {
        ensure!(self.probe.model_id == self.request.model_id, "model check result changed");
        self.consumer.with_current(|facts, _, policy| {
            self.private.with_current_in_held_policy(&self.reservation, facts, policy, || {
                retain(policy, &self.request.account_id, self.request.account_revision, &self.probe)
            })
        })
    }
}
impl WebRTCPublicationGuard for Publication {
    fn with_current(&self, publish: &mut dyn FnMut() -> rxdb::rx_error::RxResult<()>) -> rxdb::rx_error::RxResult<()> {
        self.consumer.with_current(|facts, _, policy| {
            self.private.with_current_in_held_policy(&self.reservation, facts, policy, || {
                publish().map_err(|_| anyhow::anyhow!("model check publication failed"))
            })
        }).map_err(|_| rxdb::rx_error::new_rx_error("native_model_check_retired", None))
    }
}
impl Drop for Publication {
    fn drop(&mut self) { self.reservation.release(); }
}

// Called only under the current consumer/account/private-record fence. This
// policy store is the sole writer; no RxDB shadow/projection writer is added.
fn retain(conn: &Connection, account: &str, revision: i64, probe: &NativeModelProbe) -> Result<()> {
    ensure!(revision > 0 && probe.checked_at_ms > 0, "invalid model observation");
    conn.execute(
        "INSERT INTO business_provider_federation_model_checks(account_id,account_revision,model_id,checked_at_ms,result_json)
         VALUES(?1,?2,?3,?4,?5)
         ON CONFLICT(account_id,model_id) DO UPDATE SET account_revision=excluded.account_revision,
         checked_at_ms=excluded.checked_at_ms,result_json=excluded.result_json
         WHERE excluded.checked_at_ms>=business_provider_federation_model_checks.checked_at_ms",
        params![account, revision, probe.model_id, probe.checked_at_ms, serde_json::to_string(probe)?],
    )?;
    Ok(())
}

/// Existing account-list control response, scoped to the current revision.
/// Only decode/serialize the closed metadata DTO; never return raw DB JSON.
pub(super) fn project(conn: &Connection, account: &str, revision: i64) -> Result<Value> {
    let mut stmt = conn.prepare("SELECT model_id,checked_at_ms,result_json FROM business_provider_federation_model_checks
        WHERE account_id=?1 AND account_revision=?2 ORDER BY model_id LIMIT ?3")?;
    let rows = stmt.query_map(params![account, revision, MAX_MODELS as i64 + 1], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?))
    })?.collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(rows.len() <= MAX_MODELS, "native model observation limit exceeded");
    let checks = rows.into_iter().map(|(model, checked, raw)| {
        let probe: NativeModelProbe = serde_json::from_str(&raw).context("invalid retained model observation")?;
        ensure!(probe.model_id == model && probe.checked_at_ms == checked && checked > 0,
            "retained model observation identity changed");
        Ok(serde_json::to_value(probe)?)
    }).collect::<Result<Vec<_>>>()?;
    Ok(Value::Array(checks))
}

#[cfg(test)]
#[path = "provider_model_checks_tests.rs"]
mod tests;
