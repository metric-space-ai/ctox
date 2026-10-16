//! E50: bounded, generation-local, single-use browser admission. No key lookup
//! or durable ticket table: the held E22 identity is the sole signing authority.
use super::store_inputs::StoreInputs;
use crate::business_os::{
    launch_context_v4::{self, V4LaunchContext},
    policy::sync_rights,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ctox_store::{StoreError, WriteJob};
use rxdb::plugins::replication_webrtc::index_mod::{
    connection_lifecycle::{AuthenticatePeer, AuthorizedSession},
    hello::{Hello, HelloFrame, HelloReject, RejectReason, WireSeq},
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashMap},
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct Ticket {
    peer: String,
    user: String,
    role: String,
    digest: [u8; 32],
    expires: Instant,
    local: bool,
}
pub(crate) struct BrowserAdmission {
    inputs: Weak<StoreInputs>,
    authority: Arc<sync_rights::StoreRightsAuthority>,
    key: [u8; 32],
    tickets: Mutex<HashMap<String, Ticket>>,
    peers: Mutex<HashMap<String, Arc<BrowserPeer>>>,
}
impl BrowserAdmission {
    pub(super) fn new(
        inputs: &Arc<StoreInputs>,
        authority: Arc<sync_rights::StoreRightsAuthority>,
    ) -> Result<Arc<Self>, StoreError> {
        let key = inputs
            .instance_signer()?
            .key_id()
            .strip_prefix("ed25519:")
            .ok_or(StoreError::Closed)?
            .as_bytes()
            .chunks_exact(2)
            .map(|p| {
                std::str::from_utf8(p)
                    .ok()
                    .and_then(|s| u8::from_str_radix(s, 16).ok())
                    .ok_or(StoreError::Closed)
            })
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .map_err(|_| StoreError::Closed)?;
        Ok(Arc::new(Self {
            inputs: Arc::downgrade(inputs),
            authority,
            key,
            tickets: Mutex::new(HashMap::new()),
            peers: Mutex::new(HashMap::new()),
        }))
    }
    pub(crate) async fn register(
        self: &Arc<Self>,
        context: &V4LaunchContext,
        local: bool,
    ) -> Result<(), StoreError> {
        let HelloFrame::Hello(hello) = &context.v4.hello.offer else {
            return Err(StoreError::Invalid("hello required".into()));
        };
        let actor =
            launch_context_v4::verify_offer(hello, &context.v4.signaling.peer_id, &self.key)
                .map_err(|_| StoreError::Invalid("invalid launch proof".into()))?;
        if actor.id != context.v4.user_id || actor != context.v4.actor {
            return Err(StoreError::Invalid("launch principal mismatch".into()));
        }
        let inputs = self.inputs.upgrade().ok_or(StoreError::Closed)?;
        inputs.instance_signer()?;
        let ticket = Ticket {
            peer: context.v4.signaling.peer_id.clone(),
            user: actor.id,
            role: actor.role,
            digest: digest(hello)?,
            expires: Instant::now() + Duration::from_secs(60),
            local,
        };
        let owner = self.clone();
        let nonce = hello.device_proof.nonce.clone();
        let hello = hello.clone();
        self.authority
            .writer()
            .submit(WriteJob::new(Default::default(), move |tx| {
                validate_user(tx, &ticket.user, &ticket.role, ticket.local)?;
                let rights = sync_rights::read(tx)?;
                validate_scopes(&rights, &ticket.user, &ticket.role, &hello)?;
                let head = ctox_store::change_log::head(tx)?;
                owner
                    .authority
                    .router
                    .with_snapshot(rights.rights_ver, &head.epoch, |instance, _| {
                        if instance != hello.instance_id {
                            return Err("foreign instance".into());
                        }
                        let mut tickets =
                            owner.tickets.lock().map_err(|_| "ticket owner poisoned")?;
                        tickets.retain(|_, t| t.expires > Instant::now());
                        if tickets.len() >= 4096 || tickets.contains_key(&nonce) {
                            return Err("ticket capacity or duplicate".into());
                        }
                        tickets.insert(nonce, ticket);
                        Ok(())
                    })
                    .map_err(StoreError::Invalid)
            }))?
            .await?;
        inputs.instance_signer()?;
        Ok(())
    }
    pub(crate) fn authenticator(self: &Arc<Self>) -> AuthenticatePeer {
        let owner = self.clone();
        Arc::new(move |peer, hello| {
            let owner = owner.clone();
            Box::pin(async move {
                owner.admit(peer, hello).await.map_err(|_| {
                    HelloReject::new(
                        RejectReason::AuthenticationFailed,
                        "current single-use browser ticket",
                    )
                })
            })
        })
    }
    async fn admit(
        self: &Arc<Self>,
        peer: String,
        hello: Hello,
    ) -> Result<AuthorizedSession, StoreError> {
        let inputs = self.inputs.upgrade().ok_or(StoreError::Closed)?;
        let signer = inputs.instance_signer()?;
        let transport_peer = peer.clone();
        let peer = peer
            .rsplit_once('@')
            .filter(|(_, g)| g.parse::<u64>().is_ok_and(|g| g > 0))
            .map_or(peer.as_str(), |(id, _)| id)
            .to_owned();
        let actor = launch_context_v4::verify_offer(&hello, &peer, &self.key)
            .map_err(|_| StoreError::Invalid("invalid launch proof".into()))?;
        let hash = digest(&hello)?;
        let owner = self.clone();
        let device = hello.device_proof.device_id.clone();
        let binding_actor = actor.clone();
        let wanted = hello.scopes_wanted.clone();
        let (authorized, local) = self
            .authority
            .writer()
            .submit(WriteJob::new(Default::default(), move |tx| {
                let head = ctox_store::change_log::head(tx)?;
                let rights = sync_rights::read(tx)?;
                owner
                    .authority
                    .router
                    .with_snapshot(rights.rights_ver, &head.epoch, |instance, _| {
                        if instance != hello.instance_id {
                            return Err("foreign instance".into());
                        }
                        let mut tickets =
                            owner.tickets.lock().map_err(|_| "ticket owner poisoned")?;
                        let ticket = tickets
                            .get(&hello.device_proof.nonce)
                            .ok_or("unknown or consumed browser ticket")?;
                        if ticket.peer != peer
                            || ticket.user != actor.id
                            || ticket.role != actor.role
                            || ticket.digest != hash
                        {
                            return Err("browser ticket binding mismatch".into());
                        }
                        validate_user(tx, &actor.id, &actor.role, ticket.local)
                            .map_err(|_| "inactive browser principal")?;
                        let scopes = validate_scopes(&rights, &actor.id, &actor.role, &hello)
                            .map_err(|_| "browser scope revoked")?;
                        let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map_err(|_| "clock unavailable")?
                            .as_secs();
                        let mut tokens = Vec::new();
                        for scope in scopes {
                            let claims = sync_rights::scope::ScopeClaims {
                                instance_id: instance.into(),
                                device_id: device.clone(),
                                scope_hash: scope.hash()?,
                                rights_ver: rights.rights_ver,
                                exp: now
                                    .checked_add(sync_rights::scope::PEER_LEASE_SECONDS)
                                    .ok_or("expiry overflow")?,
                            };
                            let mut bytes = b"ctox:v4:scope\0".to_vec();
                            bytes.extend(sync_rights::scope::canonical_json(
                                &serde_json::to_value(&claims).map_err(|_| "scope encoding")?,
                            )?);
                            let signature = URL_SAFE_NO_PAD.encode(signer.sign(&bytes)?);
                            let token = sync_rights::scope::SignedScope { claims, signature };
                            token.verify(&owner.key, instance, &device, &scope, now)?;
                            tokens.push(token.to_wire()?);
                        }
                        if ticket.expires <= Instant::now() {
                            return Err("expired browser ticket".into());
                        }
                        let local = ticket.local;
                        // Linearization: no await between current-user/rights validation,
                        // signature checks and removal. Concurrent replay cannot win twice.
                        tickets
                            .remove(&hello.device_proof.nonce)
                            .ok_or("browser ticket already consumed")?;
                        Ok((
                            AuthorizedSession {
                                instance_id: instance.into(),
                                device_id: device,
                                epoch: head.epoch.clone(),
                                head_seq: WireSeq(head.head_seq),
                                scope_tokens: tokens,
                                rights_ver: WireSeq(rights.rights_ver),
                            },
                            local,
                        ))
                    })
                    .map_err(StoreError::Invalid)
            }))?
            .await?;
        inputs.instance_signer()?;
        self.authority
            .router
            .with_snapshot(authorized.rights_ver.0, &authorized.epoch, |_, _| Ok(()))
            .map_err(StoreError::Invalid)?;
        let scopes = wanted
            .into_iter()
            .zip(authorized.scope_tokens.iter())
            .map(|(name, token)| {
                sync_rights::scope::SignedScope::from_wire(token)
                    .map(|t| (name, t.claims.scope_hash))
            })
            .collect::<Result<HashMap<_, _>, _>>()
            .map_err(StoreError::Invalid)?;
        let binding = Arc::new(BrowserPeer {
            local,
            user: binding_actor.id,
            role: binding_actor.role,
            device: authorized.device_id.clone(),
            epoch: authorized.epoch.clone(),
            rights_ver: authorized.rights_ver.0,
            expires: Instant::now() + Duration::from_secs(299),
            scopes,
            tokens: authorized
                .scope_tokens
                .iter()
                .map(|wire| {
                    sync_rights::scope::SignedScope::from_wire(wire)
                        .map(|token| (token.claims.scope_hash.clone(), token))
                })
                .collect::<Result<_, _>>()
                .map_err(StoreError::Invalid)?,
            results: Mutex::new(None),
            gap: Mutex::new(None),
        });
        let mut peers = self.peers.lock().map_err(|_| StoreError::Closed)?;
        peers.retain(|_, p| p.expires > Instant::now());
        if peers.len() >= 4096 || peers.contains_key(&transport_peer) {
            return Err(StoreError::Busy);
        }
        peers.insert(transport_peer, binding);
        Ok(authorized)
    }
    pub(super) fn peer(&self, peer: &str) -> Result<Arc<BrowserPeer>, StoreError> {
        self.inputs
            .upgrade()
            .ok_or(StoreError::Closed)?
            .instance_signer()?;
        self.peers
            .lock()
            .map_err(|_| StoreError::Closed)?
            .get(peer)
            .filter(|p| p.expires > Instant::now())
            .cloned()
            .ok_or(StoreError::Closed)
    }
    pub(super) fn authority(&self) -> Arc<sync_rights::StoreRightsAuthority> {
        self.authority.clone()
    }
    pub(super) fn with_write_fence<T>(
        &self,
        peer: &BrowserPeer,
        collection: &str,
        generation: u64,
        op_id: &str,
        before: Option<&serde_json::Value>,
        after: Option<&serde_json::Value>,
        effect: impl FnOnce() -> sync_rights::scope::Result<T>,
    ) -> Result<T, sync_rights::OutboxError> {
        let unavailable = || sync_rights::OutboxError::AuthorityUnavailable;
        let hash = peer.scopes.get(collection).ok_or_else(unavailable)?;
        let token = peer.tokens.get(hash).ok_or_else(unavailable)?;
        let id = format!("browser-write:{}", uuid::Uuid::new_v4());
        self.authority
            .router
            .subscribe(
                &id,
                sync_rights::Subscription {
                    device_id: peer.device.clone(),
                    actor_id: peer.user.clone(),
                    role: peer.role.clone(),
                    collection: collection.into(),
                    generation,
                },
                token,
                &self.key,
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| unavailable())?
                    .as_secs(),
            )
            .map_err(|_| unavailable())?;
        struct Registration<'a>(&'a sync_rights::RightsRouter, String);
        impl Drop for Registration<'_> {
            fn drop(&mut self) {
                let _ = self.0.unsubscribe(&self.1);
            }
        }
        let _registration = Registration(&self.authority.router, id.clone());
        self.authority
            .router
            .with_outbox_commit(&id, op_id, before, after, effect)
    }
}
pub(super) struct BrowserPeer {
    pub local: bool,
    pub user: String,
    pub role: String,
    pub device: String,
    pub epoch: String,
    pub rights_ver: u64,
    pub expires: Instant,
    pub scopes: HashMap<String, String>,
    pub tokens: HashMap<String, sync_rights::scope::SignedScope>,
    pub results:
        Mutex<Option<tokio::sync::mpsc::Sender<rxdb::rx_error::RxResult<serde_json::Value>>>>,
    pub gap: Mutex<Option<(u64, tokio::task::JoinHandle<()>)>>,
}
impl Drop for BrowserPeer {
    fn drop(&mut self) {
        if let Ok(slot) = self.gap.get_mut() {
            if let Some((_, task)) = slot.take() {
                task.abort();
            }
        }
    }
}
fn digest(hello: &Hello) -> Result<[u8; 32], StoreError> {
    let bytes = HelloFrame::Hello(hello.clone())
        .encode()
        .map_err(|_| StoreError::Invalid("invalid hello".into()))?;
    Ok(Sha256::digest(bytes).into())
}
fn validate_scopes(
    rights: &sync_rights::RightsSnapshot,
    user: &str,
    role: &str,
    hello: &Hello,
) -> Result<Vec<sync_rights::scope::ResolvedScope>, StoreError> {
    let mut seen = BTreeSet::new();
    if hello.scopes_wanted.len() > 128 {
        return Err(StoreError::Invalid("scope budget".into()));
    }
    hello
        .scopes_wanted
        .iter()
        .map(|name| {
            if !seen.insert(name) {
                return Err(StoreError::Invalid("duplicate scope".into()));
            }
            let scope = rights
                .resolve_collection(role, user, name)
                .map_err(StoreError::Invalid)?;
            if scope.projections.is_empty() {
                return Err(StoreError::Invalid("scope denied".into()));
            }
            Ok(scope)
        })
        .collect()
}
pub(super) fn validate_user(
    tx: &rusqlite::Transaction<'_>,
    user: &str,
    role: &str,
    local: bool,
) -> Result<(), StoreError> {
    if local && user == "local-dev" && role == "admin" {
        return Ok(());
    }
    let table: String = tx.query_row(
        "SELECT table_name FROM _collections WHERE name='business_users' AND state='active'",
        [],
        |r| r.get(0),
    )?;
    let raw: String = tx.query_row(
        &format!(
            "SELECT data FROM \"{}\" WHERE id=?1 AND deleted=0",
            table.replace('"', "\"\"")
        ),
        [user],
        |r| r.get(0),
    )?;
    let doc: serde_json::Value =
        serde_json::from_str(&raw).map_err(|_| StoreError::Invalid("invalid principal".into()))?;
    if doc["id"] != user || doc["active"] != true || doc["role"] != role {
        return Err(StoreError::Invalid("inactive or changed principal".into()));
    }
    Ok(())
}

#[cfg(all(test, unix))]
#[path = "store_browser_tests.rs"]
mod tests;
