// Origin: CTOX
// License: AGPL-3.0-only
//! Live native policy RPC, using the existing native Sync peer, not HTTP.
//! Phase decisions and bounded checkpoint reads use current native authority.
//! Local copy commands ingest on the target; Core takeover stays separately gated.
use super::*;
#[path = "session_handoff_checkpoint.rs"]
mod checkpoint;
#[cfg(test)]
pub(crate) use checkpoint::assert_native_checkpoint_path;
pub(crate) use checkpoint::{CopyRequest, CopyResponse};
impl NativeHandoffHost<rxdb::plugins::replication_webrtc::WebRTCRsConnection> {
    pub(crate) fn serve_checkpoint(
        &self,
        ipc: &Path,
        pool: Arc<
            RxWebRTCReplicationPool<rxdb::plugins::replication_webrtc::WebRTCRsConnectionHandler>,
        >,
        guests: Option<Arc<super::super::NativeGuestRegistry>>,
    ) -> anyhow::Result<checkpoint::CheckpointListener> {
        checkpoint::listen(self.server.clone(), ipc, pool, guests)
    }
}
use ctox_sync::authority::auth::handoff_wire::{
    fresh_nonce, verify_request, VerifiedHandoffRequest,
};
use ctox_sync::contracts::{
    SessionHandoffWireReply, SessionHandoffWireRequest, CTOX_SYNC_SESSION_HANDOFF_METHOD,
};
use rxdb::plugins::replication_webrtc::{
    index_mod::{GuardedAuxiliaryRequestHandler, GuardedAuxiliaryResponse},
    RxWebRTCReplicationPool, WebRTCConnectionHandler, WebRTCPublicationGuard,
};
use rxdb::rx_error::{new_rx_error, RxResult};
use serde_json::Value;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Mutex, MutexGuard};

const MAX_CHALLENGES: usize = 64;
#[derive(Clone)]
struct Challenge {
    sender: String,
    request: SessionHandoffGateRequest,
    nonce: String,
    permit: SessionHandoffPermit,
    used: bool,
}
struct Ledger<P> {
    alive: bool,
    pending: HashMap<P, Challenge>,
}
struct Server<P> {
    gate: NativeSessionHandoffGate,
    scope: String,
    ledger: Mutex<Ledger<P>>,
}
/// Handler futures may retain Server, but cannot retain a live host authority.
pub(crate) struct NativeHandoffHost<P> {
    server: Arc<Server<P>>,
}
impl<P> Drop for NativeHandoffHost<P> {
    fn drop(&mut self) {
        let mut ledger = self
            .server
            .ledger
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        ledger.alive = false;
        ledger.pending.clear();
    }
}
impl<P: Clone + Eq + Hash + Send + Sync + 'static> NativeHandoffHost<P> {
    pub(crate) fn start<H: WebRTCConnectionHandler<Peer = P> + 'static>(
        root: &Path,
        pool: Arc<RxWebRTCReplicationPool<H>>,
    ) -> anyhow::Result<Self> {
        let config = crate::sync_host::handoff_configuration(root)?;
        let server = Arc::new(Server {
            gate: NativeSessionHandoffGate {
                root: root.into(),
                issuer_identity: config.identity()?.to_owned(),
                permit_ttl_ms: PERMIT_TTL_MS,
            },
            scope: config.scope_id,
            ledger: Mutex::new(Ledger {
                alive: true,
                pending: HashMap::new(),
            }),
        });
        let weak_server = Arc::downgrade(&server);
        let weak_pool = Arc::downgrade(&pool);
        let handler: GuardedAuxiliaryRequestHandler<P> = Arc::new(move |peer, _, params| {
            let server = weak_server.upgrade();
            let pool = weak_pool.upgrade();
            Box::pin(async move {
                let server = server.ok_or_else(|| "host_retired".to_string())?;
                let pool = pool.ok_or_else(|| "host_retired".to_string())?;
                if !pool.is_peer_ready_for_control(&peer) {
                    return Err("peer_not_admitted".into());
                }
                // Signature checks, secrets and SQLite stay off the control
                // executor; no authority fence survives this await.
                tokio::task::spawn_blocking(move || server.answer(peer, params))
                    .await
                    .map_err(|_| "handoff_setup_failed".to_string())?
                    .map_err(|_| "handoff_setup_failed".to_string())
            })
        });
        pool.register_guarded_native_control_handler(CTOX_SYNC_SESSION_HANDOFF_METHOD, handler)?;
        Ok(Self { server })
    }
}
fn currency_matches(original: &SessionHandoffPermit, current: &SessionHandoffPermit) -> bool {
    // Full request is held separately. Authority epochs cannot be replaced
    // by a fresh permit after an older response was prepared and queued.
    original.principal_epoch == current.principal_epoch
        && original.binding_revision == current.binding_revision
        && original.issued_at_ms <= now_ms() as u64
        && original.expires_at_ms > now_ms() as u64
}
impl<P: Clone + Eq + Hash + Send + Sync + 'static> Server<P> {
    fn lock_ledger(&self) -> Result<MutexGuard<'_, Ledger<P>>, SessionHandoffDenial> {
        self.ledger.lock().map_err(|_| deny("host_unavailable"))
    }

    fn authorize_peer(
        &self,
        conn: &Connection,
        identity: &SigningIdentity,
        verified: &VerifiedHandoffRequest,
        audited: bool,
    ) -> Result<SessionHandoffPermit, SessionHandoffDenial> {
        let request = verified.request();
        let binding = load_binding(conn, &request.binding_digest)
            .map_err(|_| deny("store_unavailable"))?
            .ok_or_else(|| deny("binding_unknown"))?;
        let counterpart = match request.phase {
            SessionHandoffPhase::Disclose => &binding.target_identity,
            SessionHandoffPhase::Receive | SessionHandoffPhase::Resume => &binding.source_identity,
        };
        if counterpart != verified.sender() {
            let decision = Err(deny("peer_not_bound"));
            if audited {
                audit_decision(conn, request, identity, &decision)?;
            }
            return decision;
        }
        if audited {
            self.gate.authorize_fenced(conn, identity, request)
        } else {
            self.gate.resolve_fenced(conn, identity, request)
        }
    }
    fn answer(
        self: Arc<Self>,
        peer: P,
        params: Vec<Value>,
    ) -> Result<GuardedAuxiliaryResponse, SessionHandoffDenial> {
        if params.len() != 1 {
            return Err(deny("invalid_request"));
        }
        let verified = Arc::new(
            verify_request(
                params.into_iter().next().unwrap(),
                &self.gate.issuer_identity,
                &self.scope,
            )
            .map_err(|_| deny("invalid_request"))?,
        );
        match verified.message() {
            SessionHandoffWireRequest::Probe { request } => {
                let nonce = fresh_nonce().map_err(|_| deny("nonce_generation_failed"))?;
                // Insert only after the audited policy transaction commits.
                let (permit, result) = self.gate.with_current_authority(|conn, identity| {
                    let permit = self.authorize_peer(conn, identity, &verified, true)?;
                    let result = verified
                        .reply(
                            identity,
                            SessionHandoffWireReply::Challenge {
                                challenge: nonce.clone(),
                            },
                        )
                        .map_err(|_| deny("reply_signing_failed"))?;
                    Ok((permit, result))
                })?;
                let challenge = Challenge {
                    sender: verified.sender().into(),
                    request: request.clone(),
                    nonce,
                    permit,
                    used: false,
                };
                {
                    let mut ledger = self.lock_ledger()?;
                    if !ledger.alive {
                        return Err(deny("host_retired"));
                    }
                    ledger
                        .pending
                        .retain(|_, v| v.permit.expires_at_ms > now_ms() as u64);
                    if !ledger.pending.contains_key(&peer) && ledger.pending.len() >= MAX_CHALLENGES
                    {
                        return Err(deny("challenge_capacity"));
                    }
                    ledger.pending.insert(peer.clone(), challenge.clone());
                }
                Ok(GuardedAuxiliaryResponse {
                    result,
                    publication: Arc::new(Publication {
                        server: self,
                        peer,
                        verified,
                        challenge,
                        authorized: false,
                    }),
                })
            }
            SessionHandoffWireRequest::Fetch { .. } => {
                checkpoint::fetch(self.clone(), peer, verified.clone())
            }
            SessionHandoffWireRequest::Authorize { request, challenge } => {
                let (bound, result) = self.gate.with_current_authority(|conn, identity| {
                    let permit = self.authorize_peer(conn, identity, &verified, true)?;
                    let mut ledger = self.lock_ledger()?;
                    if !ledger.alive {
                        return Err(deny("host_retired"));
                    }
                    let bound = ledger
                        .pending
                        .get_mut(&peer)
                        .ok_or_else(|| deny("challenge_unknown"))?;
                    if bound.used
                        || bound.nonce != *challenge
                        || bound.request != *request
                        || bound.sender != verified.sender()
                        || !currency_matches(&bound.permit, &permit)
                    {
                        return Err(deny("challenge_changed"));
                    }
                    bound.used = true; // Commit/signing failure stays consumed.
                    let result = verified
                        .reply(identity, SessionHandoffWireReply::Authorized { permit })
                        .map_err(|_| deny("reply_signing_failed"))?;
                    Ok((bound.clone(), result))
                })?;
                Ok(GuardedAuxiliaryResponse {
                    result,
                    publication: Arc::new(Publication {
                        server: self,
                        peer,
                        verified,
                        challenge: bound,
                        authorized: true,
                    }),
                })
            }
        }
    }
}
struct Publication<P> {
    server: Arc<Server<P>>,
    peer: P,
    verified: Arc<VerifiedHandoffRequest>,
    challenge: Challenge,
    authorized: bool,
}
impl<P: Clone + Eq + Hash + Send + Sync + 'static> WebRTCPublicationGuard for Publication<P> {
    fn with_current(&self, publish: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
        let mut outcome = None;
        self.server
            .gate
            .with_current_authority(|conn, identity| {
                let permit = self
                    .server
                    .authorize_peer(conn, identity, &self.verified, false)?;
                if !currency_matches(&self.challenge.permit, &permit) {
                    return Err(deny("authority_changed"));
                }
                let ledger = self.server.lock_ledger()?;
                let current = ledger
                    .pending
                    .get(&self.peer)
                    .ok_or_else(|| deny("challenge_unknown"))?;
                if !ledger.alive
                    || !currency_matches(&current.permit, &permit)
                    || current.nonce != self.challenge.nonce
                    || current.used != self.authorized
                    || current.sender != self.challenge.sender
                    || current.request != self.challenge.request
                {
                    return Err(deny("challenge_changed"));
                }
                // Hold current encrypted issuer, SQLite policy mutation and host
                // retirement fences for precisely this bounded physical poll.
                // RxDB adds exact connection and room-admission fences around it.
                outcome = Some(publish());
                Ok(())
            })
            .map_err(|_| new_rx_error("RC_WEBRTC_CONTROL", None))?;
        outcome.unwrap_or_else(|| Err(new_rx_error("RC_WEBRTC_CONTROL", None)))
    }
}

#[cfg(test)]
#[path = "session_handoff_transport_tests.rs"]
mod tests;
