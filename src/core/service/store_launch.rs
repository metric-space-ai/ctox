//! E41 route boundary. Harness owns business_os::launch_context_v4::build.
//! The exact #900 envelope is retained here until that module is published.
//! No synthesized user, scope, key, offer or signaling credentials are returned.
use super::store_inputs::StoreInputs;
use crate::business_os::store::BusinessOsSessionUser;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ctox_store::StoreError;
use rusqlite::OptionalExtension;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Serialize)]
pub(crate) struct V4LaunchContext {
    pub v4: V4Context,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct V4Context {
    pub instance_id: String,
    pub user_id: String,
    pub actor: LaunchActor,
    pub signaling: LaunchSignaling,
    pub hello: LaunchHello,
    pub package_keys: Vec<PackageKey>,
}
#[derive(Serialize)]
pub(crate) struct LaunchActor {
    pub id: String,
    pub role: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LaunchSignaling {
    pub urls: Vec<String>,
    pub room: String,
    pub peer_id: String,
    pub native_peer_id: String,
    pub token: String,
    pub ice_servers: Vec<rxdb::plugins::replication_webrtc::RTCIceServer>,
}
#[derive(Serialize)]
pub(crate) struct LaunchHello {
    pub offer: HelloOffer,
}
#[derive(Serialize)]
#[serde(tag = "type")]
pub(crate) enum HelloOffer {
    #[serde(rename = "hello")]
    Hello(rxdb::plugins::replication_webrtc::index_mod::hello::Hello),
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackageKey {
    pub key_id: String,
    pub alg: KeyAlgorithm,
    pub public_key: String,
}
#[derive(Serialize)]
pub(crate) enum KeyAlgorithm {
    Ed25519,
}

async fn build(
    owners: &Arc<StoreInputs>,
    _user: BusinessOsSessionUser,
) -> Result<V4LaunchContext, StoreError> {
    owners.instance_signer()?;
    // E41-SPLIT: replace ONLY this call boundary with Harness's build(owners,
    // user) after v4/launch-context-v4 is published. Do not duplicate minting.
    Err(StoreError::Invalid("V4 launch builder unavailable".into()))
}

/// Authentication is a read on the retained BOS owner. No raw opener, write,
/// implicit loopback admin, stale cookie role or profile seeding is allowed.
async fn authenticated_user(
    inputs: &StoreInputs,
    hash: String,
) -> Result<Option<BusinessOsSessionUser>, StoreError> {
    inputs.instance_signer()?;
    let bos = inputs.databases.get("bos").ok_or(StoreError::Closed)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StoreError::Invalid("clock unavailable".into()))?
        .as_millis();
    let now = i64::try_from(now).map_err(|_| StoreError::Invalid("clock unavailable".into()))?;
    let user = bos.readers.read(move |tx| {
        tx.query_row("SELECT u.user_id,u.display_name,u.role FROM business_sessions s JOIN business_users u ON u.user_id=s.user_id WHERE s.token_hash=?1 AND s.revoked=0 AND s.expires_at_ms>?2 AND u.active=1 AND u.role IN ('chef','admin','founder','user')", rusqlite::params![hash,now], |r| {
            let role: String=r.get(2)?;
            Ok(BusinessOsSessionUser {id:r.get(0)?,display_name:r.get(1)?,is_admin:matches!(role.as_str(),"chef"|"admin"),role})
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
    let Some(token) = token else {
        return Ok((
            401,
            "application/json",
            b"{\"error\":\"authentication_required\"}".to_vec(),
        ));
    };
    let hash = URL_SAFE_NO_PAD.encode(Sha256::digest(token.as_bytes()));
    let owners = inputs.clone();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let task = inputs.runtime.spawn(async move {
        let result = async {
            let Some(user) = authenticated_user(&owners, hash).await? else {
                return Ok((
                    401,
                    "application/json",
                    b"{\"error\":\"authentication_required\"}".to_vec(),
                ));
            };
            let context = build(&owners, user).await?;
            owners.instance_signer()?;
            let body = serde_json::to_vec(&context)
                .map_err(|_| StoreError::Invalid("launch encoding unavailable".into()))?;
            Ok((200, "application/json", body))
        }
        .await;
        let _ = tx.send(result);
    });
    match rx.recv_timeout(Duration::from_secs(5)) {
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
