//! Per-subscription projection at the final frame boundary, independent of transport.
use super::{Projection, ResolvedScope, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ProjectedChange {
    Put {
        collection: String,
        id: String,
        #[serde(with = "super::decimal")]
        seq: u64,
        doc: Value,
    },
    Unproject {
        collection: String,
        id: String,
        #[serde(with = "super::decimal")]
        seq: u64,
    },
}
pub fn project(
    projection: Option<&Projection>,
    collection: &str,
    id: &str,
    seq: u64,
    document: Option<&Value>,
) -> Result<ProjectedChange> {
    if let (Some(projection), Some(doc)) = (projection, document) {
        if projection.collection == collection && projection.row_filter.matches(doc) {
            return Ok(ProjectedChange::Put {
                collection: collection.into(),
                id: id.into(),
                seq,
                doc: projection.field_mask.apply(doc)?,
            });
        }
    }
    // Emit even when this subscriber may never have held the row. No history
    // cache is needed; the client ignores unproject for an absent document.
    Ok(ProjectedChange::Unproject {
        collection: collection.into(),
        id: id.into(),
        seq,
    })
}

/// Scope updates and final SCTP handoff share this fence. A rights transaction
/// commits inside update_committed; an already-running handoff is at most one
/// frame, and no later frame can start with old rights. Do not queue raw frames
/// outside this fence or await network I/O while holding it.
pub struct FrameAuthority(Mutex<Option<ResolvedScope>>);
impl FrameAuthority {
    pub fn new(scope: ResolvedScope) -> Self {
        Self(Mutex::new(Some(scope)))
    }
    pub fn update_committed(
        &self,
        commit: impl FnOnce(&ResolvedScope) -> Result<ResolvedScope>,
    ) -> Result<()> {
        let mut current = self.0.lock().map_err(|_| "rights authority poisoned")?;
        let previous = current
            .take()
            .ok_or("rights authority unavailable; resubscribe")?;
        // If publication fails or returns an invalid version, fail closed. A
        // possibly committed transaction can never leave old frame authority live.
        let next = commit(&previous)?;
        if next.rights_ver <= previous.rights_ver {
            return Err("rights version must increase".into());
        }
        *current = Some(next);
        Ok(())
    }
    pub fn with_frame<T>(
        &self,
        expected_version: u64,
        expected_hash: &str,
        send_one_frame: impl FnOnce(&ResolvedScope) -> Result<T>,
    ) -> Result<T> {
        let scope = self.0.lock().map_err(|_| "rights authority poisoned")?;
        let scope = scope
            .as_ref()
            .ok_or("rights authority unavailable; resubscribe")?;
        if scope.rights_ver != expected_version || scope.hash()? != expected_hash {
            return Err("revoked".into());
        }
        send_one_frame(&scope)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactExposure {
    pub hash: String,
    pub collection: String,
    pub row_id: String,
    pub field_path: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeRevoke {
    pub rights_ver: u64,
    /// Replacement applies only to these collections, even for an empty scope.
    /// Other apps/collections and their checkpoints must remain intact.
    pub collections: std::collections::BTreeSet<String>,
    pub remaining: Vec<Projection>,
    /// Candidate hashes after deleting the revoked references. The client only
    /// removes bytes when no authorized reference outside this scope remains.
    pub artifacts: Vec<String>,
    pub outbox_reason: String,
}
impl ScopeRevoke {
    pub fn new(
        scope: &ResolvedScope,
        held_rows: &std::collections::BTreeMap<(String, String), Value>,
        artifacts: &[ArtifactExposure],
    ) -> Self {
        let collections = scope
            .projections
            .iter()
            .map(|p| p.collection.clone())
            .chain(held_rows.keys().map(|(c, _)| c.clone()))
            .chain(artifacts.iter().map(|a| a.collection.clone()))
            .collect();
        Self::for_collections(scope, collections, held_rows, artifacts)
    }
    pub fn for_collection(
        scope: &ResolvedScope,
        collection: &str,
        held_rows: &std::collections::BTreeMap<(String, String), Value>,
        artifacts: &[ArtifactExposure],
    ) -> Self {
        Self::for_collections(scope, [collection.to_string()].into(), held_rows, artifacts)
    }
    fn for_collections(
        scope: &ResolvedScope,
        collections: std::collections::BTreeSet<String>,
        held_rows: &std::collections::BTreeMap<(String, String), Value>,
        artifacts: &[ArtifactExposure],
    ) -> Self {
        let mut revoked: Vec<_> = artifacts
            .iter()
            .filter(|artifact| {
                if !collections.contains(&artifact.collection) {
                    return false;
                }
                let Some(projection) = scope.projection(&artifact.collection) else {
                    return true;
                };
                let Some(doc) =
                    held_rows.get(&(artifact.collection.clone(), artifact.row_id.clone()))
                else {
                    return true;
                };
                !projection.row_filter.matches(doc)
                    || !projection.field_mask.allows(&artifact.field_path)
            })
            .map(|a| a.hash.clone())
            .collect();
        revoked.sort();
        revoked.dedup();
        let remaining = scope
            .projections
            .iter()
            .filter(|p| collections.contains(&p.collection))
            .cloned()
            .collect();
        Self {
            rights_ver: scope.rights_ver,
            collections,
            remaining,
            artifacts: revoked,
            outbox_reason: "revoked".into(),
        }
    }
    pub fn clean_document(&self, collection: &str, doc: &Value) -> Result<Option<Value>> {
        if !self.collections.contains(collection) {
            return Ok(Some(doc.clone()));
        }
        match self.remaining.iter().find(|p| p.collection == collection) {
            Some(p) if p.row_filter.matches(doc) => Ok(Some(p.field_mask.apply(doc)?)),
            _ => Ok(None),
        }
    }
}

/// Core rejection, not a silent deletion of an optimistic write. The client
/// moves its original payload to archive_local when applying this receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevokedWrite {
    pub op_id: String,
    pub status: String,
    pub reason: String,
}
impl RevokedWrite {
    pub fn new(op_id: impl Into<String>) -> Self {
        Self {
            op_id: op_id.into(),
            status: "rejected".into(),
            reason: "revoked".into(),
        }
    }
}

use rxdb::plugins::replication_webrtc::{
    WebRTCConnectionHandler, WebRTCPublicationGuard, WebRTCResponse, WebRTCWireFrame,
};
use rxdb::rx_error::{new_rx_error, RxResult};
use std::sync::Arc;

pub type CoreClock = Arc<dyn Fn() -> u64 + Send + Sync>;
fn denied() -> rxdb::rx_error::RxError {
    new_rx_error("ctox_v4_scope_revoked", None)
}

/// Retained by the native queue. The real native send path reacquires this
/// fence on EVERY physical send poll, and releases it on Pending/backpressure.
pub struct NativePublication {
    pub authority: Arc<super::super::StoreRightsAuthority>,
    pub subscription_id: String,
    pub token: super::SignedScope,
    pub epoch: String,
    pub public_key: Vec<u8>,
    pub collection: String,
    pub generation: u64,
    pub clock: CoreClock,
}
impl WebRTCPublicationGuard for NativePublication {
    fn with_current(&self, publish: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
        // Router epoch is immutable; rights are rechecked under the actual IO
        // fence below as well, so releasing this preflight cannot widen access.
        self.authority
            .router
            .with_snapshot(self.token.claims.rights_ver, &self.epoch, |_, _| Ok(()))
            .map_err(|_| denied())?;
        let mut io_error = None;
        let result = self.authority.router.with_change_frame(
            &self.subscription_id,
            &self.token,
            &self.public_key,
            (self.clock)(),
            &self.collection,
            self.generation,
            "",
            0,
            None,
            |_| match publish() {
                Ok(()) => Ok(()),
                Err(error) => {
                    io_error = Some(error);
                    Err("send failed".into())
                }
            },
        );
        match io_error {
            Some(error) => Err(error),
            None => result.map_err(|_| denied()),
        }
    }
}
pub struct NativeChange<'a> {
    pub collection: &'a str,
    pub generation: u64,
    pub epoch: &'a str,
    pub id: &'a str,
    pub seq: u64,
    pub document: Option<&'a Value>,
    pub core_sent_at_ms: u64,
}
/// The caller is the Core change-log drain for one active subscription. Only
/// the masked projection enters the queue; its native guard survives queueing.
/// There is deliberately no unguarded-send fallback.
pub async fn send_native_change<H: WebRTCConnectionHandler>(
    transport: &H,
    peer: &H::Peer,
    authority: Arc<NativePublication>,
    change: NativeChange<'_>,
) -> RxResult<()> {
    if change.collection != authority.collection
        || change.generation != authority.generation
        || change.epoch != authority.epoch
    {
        return Err(denied());
    }
    let frame=authority.authority.router.with_change_frame(&authority.subscription_id,&authority.token,
        &authority.public_key,(authority.clock)(),change.collection,change.generation,change.id,
        change.seq,change.document,|projected|{
            Ok(WebRTCWireFrame::Response(WebRTCResponse{
                id:"v4:change".into(),collection:Some(change.collection.into()),error:None,
                result:serde_json::json!({"type":"live","epoch":change.epoch,"gen":change.generation,
                    "scope_hash":authority.token.claims.scope_hash,"rights_ver":authority.token.claims.rights_ver.to_string(),
                    "core_sent_at_ms":change.core_sent_at_ms,"cursor_to":change.seq.to_string(),
                    "changes":[projected]}),
            }))
        }).map_err(|_|denied())?;
    transport.send_guarded(peer, frame, authority).await
}
/// A peer forwarding queue revalidates BOTH leases and the original signed
/// packet on every physical poll, just like Core-originated publication.
pub struct NativePeerPublication {
    pub authority: Arc<super::super::StoreRightsAuthority>,
    pub subscription_id: String,
    pub public_key: Vec<u8>,
    pub sender_device: String,
    pub sender_lease: super::PeerLease,
    pub receiver_lease: super::PeerLease,
    pub packet: super::PeerPacket,
    pub signature: String,
    pub payload: Vec<u8>,
    pub clock: CoreClock,
}
impl WebRTCPublicationGuard for NativePeerPublication {
    fn with_current(&self, publish: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
        let mut io_error = None;
        let result = self.authority.router.with_peer_frame(
            &self.subscription_id,
            &self.public_key,
            (self.clock)(),
            (&self.sender_device, &self.sender_lease),
            &self.receiver_lease,
            &self.packet,
            &self.signature,
            &self.payload,
            || match publish() {
                Ok(()) => Ok(()),
                Err(error) => {
                    io_error = Some(error);
                    Err("send failed".into())
                }
            },
        );
        match io_error {
            Some(error) => Err(error),
            None => result.map_err(|_| denied()),
        }
    }
}

/// Revoke uses the newly issued current token, including an empty projection;
/// no obsolete token is promoted just to send the cleanup notification.
pub async fn send_native_revoke<H: WebRTCConnectionHandler>(
    transport: &H,
    peer: &H::Peer,
    authority: Arc<NativePublication>,
    revoke: &ScopeRevoke,
) -> RxResult<()> {
    if revoke.collections.len() != 1 || !revoke.collections.contains(&authority.collection) {
        return Err(denied());
    }
    let expected = ResolvedScope {
        rights_ver: revoke.rights_ver,
        projections: revoke.remaining.clone(),
    };
    if revoke.rights_ver != authority.token.claims.rights_ver
        || expected.hash().map_err(|_| denied())? != authority.token.claims.scope_hash
    {
        return Err(denied());
    }
    let frame = WebRTCWireFrame::Response(WebRTCResponse {
        id: "v4:scope_revoke".into(),
        collection: None,
        error: None,
        result: serde_json::json!({"type":"scope_revoke","scope_hash":authority.token.claims.scope_hash,
            "rights_ver":revoke.rights_ver.to_string(),"remaining":revoke.remaining,"artifacts":revoke.artifacts,
            "outbox_reason":revoke.outbox_reason,"collections":revoke.collections,
            "artifact_policy":"unreferenced_after_scope_cleanup"}),
    });
    transport.send_guarded(peer, frame, authority).await
}
