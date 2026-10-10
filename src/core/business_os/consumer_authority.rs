// Origin: CTOX
// License: AGPL-3.0-only

//! Native origin authority for account federation. This is computer enrollment,
//! not an account grant: all enrolled consumers remain eligible by default.
//! Forwarders cannot reconstruct this context from JSON or their own identity.

use super::{capability::CapabilityClaims, store};
use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};
use rxdb::plugins::replication_webrtc::{
    connection_handler_rs::{WebRTCRsConnection, WebRTCRsConnectionHandler},
    webrtc_types::WebRTCConnectionHandler,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[cfg(test)]
#[path = "consumer_authority_tests.rs"]
mod tests;

/// Public identity facts, not an authority constructor or a wire credential.
/// Registry/routing owners define their signed forwarding contract separately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConsumerFacts {
    pub owner_user_id: String,
    pub owner_epoch: i64,
    pub actor_user_id: String,
    pub actor_epoch: i64,
    pub computer_id: String,
    pub computer_revision: String,
    pub pairing_id: String,
    pub device_id: String,
    pub proof_key_thumbprint: String,
    pub pairing_revision: String,
}

/// Constructible only from the actual native transport's captured credential
/// and exact connection generation. No caller-supplied computer/owner fields.
/// Never serialize/deserialize this object, export its token or use a debug log.
pub(crate) struct AdmittedConsumerAuthority {
    root: PathBuf,
    transport: Arc<WebRTCRsConnectionHandler>,
    peer: WebRTCRsConnection,
    token: String,
    facts: ConsumerFacts,
}

impl AdmittedConsumerAuthority {
    /// Native-only store location captured from the admitted host, never a
    /// path supplied by an account/configuration request.
    pub(crate) fn native_host_root(&self) -> &Path {
        &self.root
    }

    /// Call from a guarded auxiliary handler, which supplies the accepted
    /// connection by value after the native nonce/possession admission round.
    /// An anonymous/unbound capability is never a computer identity.
    pub(crate) fn capture(
        root: &Path,
        transport: Arc<WebRTCRsConnectionHandler>,
        peer: WebRTCRsConnection,
        admitted_token: &str,
    ) -> Result<Self> {
        let token = transport
            .peer_capability_token(&peer)
            .context("consumer connection has no admitted credential")?;
        anyhow::ensure!(
            token == admitted_token,
            "consumer credential changed since admission"
        );
        let facts = current_policy(root, &token, |conn, claims| resolve(conn, claims))?;
        anyhow::ensure!(
            transport
                .with_current_peer_capability(&peer, &token, || ())
                .is_some(),
            "consumer connection changed during resolution"
        );
        Ok(Self {
            root: root.to_owned(),
            transport,
            peer,
            token,
            facts,
        })
    }

    /// Re-enter after every await and immediately around a bounded dispatch or
    /// publication operation. Order: consumer transport -> issuer -> policy.
    /// No await, network wait, secret/transport API reentry or retained borrowed
    /// connection is permitted inside apply. Membership/actor/device mutations
    /// cannot commit during this callback; a busy store fails closed.
    /// This is the origin fence, not a distributed holder/account fence.
    pub(crate) fn with_current<T>(
        &self,
        apply: impl FnOnce(&ConsumerFacts, &Connection) -> Result<T>,
    ) -> Result<T> {
        // Prepare schema/store outside the transport fence.
        let mut conn = store::open_store(&self.root)?;
        conn.busy_timeout(std::time::Duration::ZERO)?;
        self.transport
            .with_current_peer_capability(&self.peer, &self.token, || {
                with_current_policy(&self.root, &mut conn, &self.token, &self.facts, apply)
            })
            .context("consumer connection or credential retired")?
    }
}

fn with_current_policy<T>(
    root: &Path,
    conn: &mut Connection,
    token: &str,
    expected: &ConsumerFacts,
    apply: impl FnOnce(&ConsumerFacts, &Connection) -> Result<T>,
) -> Result<T> {
    store::with_current_webrtc_capability_signer(root, |secret| {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let claims = store::verified_webrtc_capability_claims_from_connection(
            &tx,
            token,
            secret,
            store::now_ms() as i64,
        )
        .context("consumer actor or device was revoked")?;
        let current = resolve(&tx, &claims)?;
        anyhow::ensure!(&current == expected, "consumer enrollment changed");
        apply(&current, &tx)
    })
}

pub(super) const CONSUMER_AUTHORITY_METHOD: &str = "ctox.workjet.consumer.v1";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConsumerRequest {
    version: u32,
}

/// Transient, read-only WebRTC control response. JSON facts are not a forwarded
/// authorization token. Models uses the sealed context, not this JSON, to admit
/// native dispatch. No account selection or permission is granted here.
pub(super) fn resolve_response(
    root: &Path,
    transport: Arc<WebRTCRsConnectionHandler>,
    peer: WebRTCRsConnection,
    admitted_token: &str,
    params: Vec<Value>,
) -> Result<rxdb::plugins::replication_webrtc::index_mod::GuardedAuxiliaryResponse> {
    anyhow::ensure!(
        params.len() == 1 && serde_json::to_vec(&params)?.len() <= 1024,
        "invalid consumer request"
    );
    let request: ConsumerRequest = serde_json::from_value(params.into_iter().next().unwrap())?;
    anyhow::ensure!(request.version == 1, "unsupported consumer request version");
    let authority = AdmittedConsumerAuthority::capture(root, transport, peer, admitted_token)?;
    let result = authority.with_current(|facts, _| Ok(json!({"version":1,"consumer":facts})))?;
    let conn = store::open_store(root)?;
    conn.busy_timeout(std::time::Duration::ZERO)?;
    let publication = ConsumerPublication {
        root: authority.root,
        token: authority.token,
        facts: authority.facts,
        conn: std::sync::Mutex::new(conn),
    };
    Ok(
        rxdb::plugins::replication_webrtc::index_mod::GuardedAuxiliaryResponse {
            result,
            publication: Arc::new(publication),
        },
    )
}

/// Private guard is installed only by the guarded auxiliary responder, whose
/// outer guard already holds the exact connection/token and pool-life fence.
/// Do not reenter that transport from a physical publication poll.
struct ConsumerPublication {
    root: PathBuf,
    token: String,
    facts: ConsumerFacts,
    conn: std::sync::Mutex<Connection>,
}

impl rxdb::plugins::replication_webrtc::WebRTCPublicationGuard for ConsumerPublication {
    fn with_current(
        &self,
        publish: &mut dyn FnMut() -> rxdb::rx_error::RxResult<()>,
    ) -> rxdb::rx_error::RxResult<()> {
        let mut conn = self
            .conn
            .try_lock()
            .map_err(|_| rxdb::rx_error::new_rx_error("consumer_authority_unavailable", None))?;
        with_current_policy(&self.root, &mut conn, &self.token, &self.facts, |_, _| {
            publish().map_err(|_| anyhow::anyhow!("consumer publication failed"))
        })
        .map_err(|_| rxdb::rx_error::new_rx_error("consumer_authority_retired", None))
    }
}

fn current_policy<T>(
    root: &Path,
    token: &str,
    apply: impl FnOnce(&Connection, &CapabilityClaims) -> Result<T>,
) -> Result<T> {
    let mut conn = store::open_store(root)?;
    conn.busy_timeout(std::time::Duration::ZERO)?;
    store::with_current_webrtc_capability_signer(root, |secret| {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Deferred)?;
        let claims = store::verified_webrtc_capability_claims_from_connection(
            &tx,
            token,
            secret,
            super::store::now_ms() as i64,
        )
        .context("consumer capability is invalid")?;
        apply(&tx, &claims)
    })
}

/// Called only by the existing owner-authorized computer.assign command.
/// No hostnames, device IDs, labels or source secrets are computer authority.
pub(super) fn validate_owner_binding(
    conn: &Connection,
    owner: &str,
    pairing: &str,
) -> Result<Value> {
    if pairing.is_empty() {
        return Ok(Value::Null);
    }
    anyhow::ensure!(
        pairing.len() <= 256 && !pairing.chars().any(char::is_control),
        "invalid pairing id"
    );
    let row: Option<(String, String, String, String, i64)> = conn
        .query_row(
            "SELECT i.user_id,i.device_id,i.proof_key_thumbprint,i.invite_id_hash,i.created_at_ms
         FROM business_mobile_invites i JOIN business_users u ON u.user_id=i.user_id
         WHERE i.device_pairing_id=?1 AND i.created_by_user_id=?2
           AND i.revoked_at_ms IS NULL AND i.proof_key_thumbprint IS NOT NULL AND u.active=1",
            rusqlite::params![pairing, owner],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?;
    let (actor, device, thumbprint, invite_hash, created_at) =
        row.context("paired device has no current verified owner association")?;
    Ok(
        json!({"actor_user_id":actor,"pairing_id":pairing,"device_id":device,
        "proof_key_thumbprint":thumbprint,"invite_id_hash":invite_hash,"created_at_ms":created_at}),
    )
}

fn resolve(conn: &Connection, claims: &CapabilityClaims) -> Result<ConsumerFacts> {
    let binding = claims
        .device_binding
        .as_ref()
        .context("consumer has no possession-bound device")?;
    let owner: String = conn
        .query_row(
            "SELECT created_by_user_id FROM business_mobile_invites
         WHERE user_id=?1 AND device_pairing_id=?2 AND device_id=?3
           AND proof_key_thumbprint=?4 AND revoked_at_ms IS NULL",
            rusqlite::params![
                claims.user_id,
                binding.device_pairing_id,
                binding.device_id,
                binding.proof_key_thumbprint
            ],
            |row| row.get(0),
        )
        .context("consumer pairing has no retained owner")?;
    let association = validate_owner_binding(conn, &owner, &binding.device_pairing_id)?;
    let owner_epoch: i64 = conn
        .query_row(
            "SELECT capability_epoch FROM business_users WHERE user_id=?1 AND active=1",
            [&owner],
            |row| row.get(0),
        )
        .context("consumer owner is inactive")?;
    // Materialize at most two candidates; never scan/deserialize the complete
    // computer registry while holding publication authority.
    let mut stmt = conn.prepare(
        "SELECT payload_json FROM business_records
        WHERE collection='workjet_computers' AND deleted=0
          AND json_extract(payload_json,'$.owner_user_id')=?1
          AND json_extract(payload_json,'$.device_binding_id')=?2
          AND json_extract(payload_json,'$.status')='assigned'
          AND coalesce(json_extract(payload_json,'$.is_deleted'),0)=0
          AND coalesce(json_extract(payload_json,'$._deleted'),0)=0
          AND coalesce(json_extract(payload_json,'$.agentless'),0)=0
          AND json_extract(payload_json,'$.hosting_mode') IN ('workstation','self_hosted')
        LIMIT 2",
    )?;
    let computers = stmt
        .query_map(rusqlite::params![owner, binding.device_pairing_id], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    // Duplicate enrollment fails even if one association witness is stale.
    anyhow::ensure!(
        computers.len() == 1,
        "device has missing or ambiguous computer enrollment"
    );
    let mut matches = computers
        .into_iter()
        .map(|row| serde_json::from_str::<Value>(&row))
        .collect::<serde_json::Result<Vec<_>>>()?
        .into_iter()
        .filter(|row| row["native_device_binding"] == association);
    let computer = matches
        .next()
        .context("device is not associated with an enrolled computer")?;
    anyhow::ensure!(
        matches.next().is_none(),
        "device has ambiguous computer enrollment"
    );
    Ok(ConsumerFacts {
        owner_user_id: owner,
        owner_epoch,
        actor_user_id: claims.user_id.clone(),
        actor_epoch: claims.actor_epoch,
        computer_id: computer["id"]
            .as_str()
            .context("computer identity missing")?
            .to_owned(),
        computer_revision: computer["_rev"]
            .as_str()
            .context("computer revision missing")?
            .to_owned(),
        pairing_id: binding.device_pairing_id.clone(),
        device_id: binding.device_id.clone(),
        proof_key_thumbprint: binding.proof_key_thumbprint.clone(),
        pairing_revision: association["invite_id_hash"]
            .as_str()
            .context("pairing revision missing")?
            .to_owned(),
    })
}
