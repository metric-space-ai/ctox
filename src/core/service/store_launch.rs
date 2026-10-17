//! E41 route boundary. Harness owns business_os::launch_context_v4::build.
//! Harness supplies the exact #900 envelope; this route never duplicates it.
//! No synthesized user, scope, key, offer or signaling credentials are returned.
use super::store_inputs::StoreInputs;
use crate::business_os::store::BusinessOsUser;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ctox_store::StoreError;
use rusqlite::OptionalExtension;
use sha2::{Digest, Sha256};
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

async fn build(
    owners: &Arc<StoreInputs>,
    user: &BusinessOsUser,
) -> Result<crate::business_os::launch_context_v4::V4LaunchContext, StoreError> {
    crate::business_os::launch_context_v4::build(owners, user)
        .await
        .map_err(|_| StoreError::Invalid("V4 launch authority unavailable".into()))
}

/// Authentication is a read on the retained BOS owner. No raw opener, write,
/// stale cookie role or profile seeding is allowed. E50 loopback admission is
/// separately authenticated from the actual TCP peer AND the Host header.
async fn authenticated_user(
    inputs: &StoreInputs,
    hash: String,
) -> Result<Option<BusinessOsUser>, StoreError> {
    inputs.instance_signer()?;
    let bos = inputs.databases.get("bos").ok_or(StoreError::Closed)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StoreError::Invalid("clock unavailable".into()))?
        .as_millis();
    let now = i64::try_from(now).map_err(|_| StoreError::Invalid("clock unavailable".into()))?;
    let user = bos.readers.read(move |tx| {
        tx.query_row("SELECT u.user_id,u.display_name,u.role,u.active,u.profile_json,u.created_at_ms,u.updated_at_ms FROM business_sessions s JOIN business_users u ON u.user_id=s.user_id WHERE s.token_hash=?1 AND s.revoked=0 AND s.expires_at_ms>?2 AND u.active=1 AND u.role IN ('chef','admin','founder','user')", rusqlite::params![hash,now], |r| {
            let role: String=r.get(2)?;
            Ok(BusinessOsUser {id:r.get(0)?,display_name:r.get(1)?,role,active:r.get(3)?,profile:serde_json::from_str(&r.get::<_,String>(4)?).map_err(|e|rusqlite::Error::FromSqlConversionFailure(4,rusqlite::types::Type::Text,Box::new(e)))?,created_at_ms:r.get(5)?,updated_at_ms:r.get(6)?})
        }).optional()
    }).await?;
    inputs.instance_signer()?;
    Ok(user)
}

/// Called by a bounded HTTP OS worker, never a runtime worker. The task is
/// cancelled on deadline; an abandoned HTTP request must not keep minting offers.
pub(super) fn response(
    inputs: &Arc<StoreInputs>,
    request: &tiny_http::Request,
) -> Result<(u16, &'static str, Vec<u8>), StoreError> {
    if request.method() != &tiny_http::Method::Get {
        return Ok((405, "text/plain", Vec::new()));
    }
    let token = request
        .headers()
        .iter()
        .filter(|h| h.field.equiv("Cookie"))
        .flat_map(|h| h.value.as_str().split(';'))
        .find_map(|part| {
            let (name, value) = part.trim().split_once('=')?;
            (name == "ctox_business_os_auth" && !value.is_empty() && value.len() <= 512)
                .then_some(value)
        });
    let local = token.is_none() && allows_local_admin(request);
    if token.is_none() && !local {
        return Ok((
            401,
            "application/json",
            b"{\"error\":\"authentication_required\"}".to_vec(),
        ));
    }
    let hash = token.map(|token| URL_SAFE_NO_PAD.encode(Sha256::digest(token.as_bytes())));

    let owners = inputs.clone();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let task = inputs.runtime.spawn(async move {
        let result = async {
            if local {
                owners.browser_ingress_ready()?;
                let context = crate::business_os::launch_context_v4::build_local(&owners)
                    .await
                    .map_err(|_| StoreError::Invalid("local launch unavailable".into()))?;
                owners.browser_ingress_ready()?;
                owners.instance_signer()?;
                return Ok((
                    200,
                    "application/json",
                    serde_json::to_vec(&context)
                        .map_err(|_| StoreError::Invalid("launch encoding".into()))?,
                ));
            }
            let hash = hash.ok_or(StoreError::Closed)?;
            let Some(user) = authenticated_user(&owners, hash.clone()).await? else {
                return Ok((
                    401,
                    "application/json",
                    b"{\"error\":\"authentication_required\"}".to_vec(),
                ));
            };
            owners.browser_ingress_ready()?;
            let context = build(&owners, &user).await?;
            // Recheck after builder awaits; no session/role snapshot is a grant.
            let current = authenticated_user(&owners, hash).await?;
            if current
                .as_ref()
                .is_none_or(|u| u.id != user.id || u.role != user.role)
                || context.v4.user_id != user.id
                || context.v4.actor.id != user.id
                || context.v4.actor.role != user.role
                || context.v4.instance_id != owners.instance_signer()?.instance_id()
            {
                return Ok((
                    401,
                    "application/json",
                    b"{\"error\":\"authentication_required\"}".to_vec(),
                ));
            }
            owners.browser_ingress_ready()?;
            owners.instance_signer()?;
            let body = serde_json::to_vec(&context)
                .map_err(|_| StoreError::Invalid("launch encoding unavailable".into()))?;
            Ok((200, "application/json", body))
        }
        .await;
        let _ = tx.send(result);
    });
    match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(result) => result,
        Err(_) => {
            task.abort();
            Err(StoreError::DeadlineExceeded)
        }
    }
}

#[cfg(all(test, unix))]
#[path = "store_launch_tests.rs"]
mod tests;

fn allows_local_admin(request: &tiny_http::Request) -> bool {
    if !request.remote_addr().is_some_and(|a| a.ip().is_loopback()) {
        return false;
    }
    let host = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Host"))
        .map(|h| h.value.as_str());
    host.is_none_or(|h| {
        if h.contains(['@', '/', '\\', '#', '?', ',']) {
            return false;
        }

        url::Url::parse(&format!("http://{h}"))
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
            .is_some_and(|h| {
                h.eq_ignore_ascii_case("localhost")
                    || h.to_ascii_lowercase().ends_with(".localhost")
                    || h.trim_matches(['[', ']'])
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback())
            })
    })
}
