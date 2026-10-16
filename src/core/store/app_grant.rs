//! E55 app grants. One retained owner per authenticated browser peer.
//! Adapters supply the foreground E22 signer, never key bytes or browser roles.
use super::{manifest_rules::AppOperation, scope, Operation, RightsSnapshot, StoreRightsAuthority};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ctox_store::change_log::subscription::protocol::InstanceSigner;
use ring::signature::{UnparsedPublicKey, ED25519};
use rusqlite::Transaction;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

pub const DOMAIN: &[u8] = b"ctox:v4:scope\0";
pub const MAX_TOKEN_BYTES: usize = 16 * 1024;
const MAX_APPS: usize = 256;
const TTL: u64 = 240;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionGrant {
    pub collection: String,
    pub gen: u64,
    pub rights: Vec<String>,
    pub field_mask_digest: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppGrantClaims {
    pub instance_id: String,
    pub user_id: String,
    pub app_id: String,
    pub scope_hash: String,
    pub rights_ver: String,
    pub device_id: String,
    pub exp: u64,
    pub collections: Vec<CollectionGrant>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedGrant {
    claims: AppGrantClaims,
    signature: String,
}
#[derive(Debug, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum GrantRequest {
    #[serde(rename = "scope.grant.request")]
    Request { app_id: String },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScopeRevoke {
    #[serde(rename = "type")]
    kind: &'static str,
    pub app_id: String,
    pub reason: &'static str,
}
struct Issued {
    fingerprint: String,
    claims: AppGrantClaims,
}
struct State {
    user: String,
    role: String,
    device: String,
    active: bool,
    issued: BTreeMap<String, Issued>,
    // Retained across identity changes: never reproduce an earlier token in
    // the same second when a user signs back in with the same app and rights.
    last_exp: BTreeMap<String, u64>,
}
/// Foreground adapter validates its live browser-peer binding and current
/// principal (e.g. store_browser::validate_user) INSIDE this writer transaction.
/// Implementations must be bounded and must not enqueue jobs or re-enter us.
pub trait GrantPeerAuthority: Send + Sync {
    fn validate(
        &self,
        tx: &Transaction<'_>,
        user: &str,
        role: &str,
        device: &str,
        app_id: &str,
    ) -> scope::Result<()>;
}
/// No Deserialize/Clone identity setter: only the authenticated native adapter
/// constructs/rebinds this owner. Old subscriptions must be retired before rebind.
pub struct AppGrantSession {
    authority: Arc<StoreRightsAuthority>,
    signer: Arc<dyn InstanceSigner>,
    peer_authority: Arc<dyn GrantPeerAuthority>,
    state: Mutex<State>,
}
fn bounded(s: &str) -> scope::Result<()> {
    if s.is_empty() || s.len() > 256 || s.chars().any(char::is_control) {
        return Err("invalid grant identity".into());
    }
    Ok(())
}
fn canonical<T: Serialize>(value: &T) -> scope::Result<Vec<u8>> {
    scope::canonical_json(&serde_json::to_value(value).map_err(|_| "invalid grant encoding")?)
}
pub fn signing_bytes(claims: &AppGrantClaims) -> scope::Result<Vec<u8>> {
    let mut bytes = DOMAIN.to_vec();
    bytes.extend(canonical(claims)?);
    if bytes.len() > MAX_TOKEN_BYTES {
        return Err("grant exceeds budget".into());
    }
    Ok(bytes)
}
fn public_key(signer: &dyn InstanceSigner) -> scope::Result<Vec<u8>> {
    let text = signer
        .key_id()
        .strip_prefix("ed25519:")
        .ok_or("invalid instance pin")?;
    if text.len() != 64 {
        return Err("invalid instance pin".into());
    }
    (0..64)
        .step_by(2)
        .map(|i| {
            let pair = text.get(i..i + 2).ok_or("invalid instance pin")?;
            if !pair
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err("invalid instance pin".into());
            }
            u8::from_str_radix(pair, 16).map_err(|_| "invalid instance pin".into())
        })
        .collect()
}
fn verify_signature(signer: &dyn InstanceSigner, grant: &SignedGrant) -> scope::Result<()> {
    let signature = URL_SAFE_NO_PAD
        .decode(&grant.signature)
        .map_err(|_| "invalid grant signature")?;
    UnparsedPublicKey::new(&ED25519, public_key(signer)?)
        .verify(&signing_bytes(&grant.claims)?, &signature)
        .map_err(|_| "invalid grant signature".into())
}
fn decode(token: &str) -> scope::Result<SignedGrant> {
    if token.len() > MAX_TOKEN_BYTES {
        return Err("grant exceeds budget".into());
    }
    let raw = URL_SAFE_NO_PAD
        .decode(token)
        .map_err(|_| "invalid grant encoding")?;
    let grant: SignedGrant = serde_json::from_slice(&raw).map_err(|_| "invalid grant encoding")?;
    if canonical(&grant)? != raw {
        return Err("noncanonical grant encoding".into());
    }
    Ok(grant)
}

/// Resolves ONLY manifest-declared, user-readable collections in this writer
/// snapshot. The field mask and resolved row filters feed the P6 scope hash.
fn resolve(
    tx: &Transaction<'_>,
    rights: &RightsSnapshot,
    state: &State,
    app: &str,
) -> scope::Result<(Vec<CollectionGrant>, String)> {
    let manifest = rights
        .manifests
        .iter()
        .find(|m| m.app_id() == app)
        .ok_or("forbidden")?;
    manifest.validate_registry(&rights.manifests)?;
    let mut collections = Vec::new();
    let mut projections = Vec::new();
    for rule in rights
        .rules
        .iter()
        .filter(|r| r.role == state.role && r.ops.contains(&Operation::Read))
    {
        let scope =
            match rights.resolve_app_collection(app, &state.role, &state.user, &rule.collection) {
                Ok(scope) => scope,
                Err(_) => continue,
            };
        let Some(mut projection) = scope.projection(&rule.collection).cloned() else {
            continue;
        };
        if projection.row_filter == scope::RowFilter::None {
            continue;
        }
        let Some(active) = ctox_store::schema::active_collection(tx, &rule.collection)
            .map_err(|_| "grant registry unavailable")?
        else {
            continue;
        };
        let wire = if active.gen == 1 {
            rule.collection.clone()
        } else {
            format!("{}@g{}", rule.collection, active.gen)
        };
        let mut operations = vec!["read".to_owned()];
        if rule.ops.contains(&Operation::Write)
            && manifest
                .authorize_collection(&rights.manifests, &rule.collection, AppOperation::Write)
                .is_ok()
        {
            operations.push("write".to_owned());
        }
        collections.push(CollectionGrant {
            collection: wire.clone(),
            gen: active.gen,
            rights: operations,
            field_mask_digest: scope::digest(&canonical(&projection.field_mask)?),
        });
        projection.collection = wire;
        projections.push(projection);
    }
    if collections.is_empty() {
        return Err("forbidden".into());
    }
    collections.sort_by(|a, b| a.collection.cmp(&b.collection));
    let hash = scope::ResolvedScope::new(rights.rights_ver, projections, &state.user)?.hash()?;
    Ok((collections, hash))
}
impl AppGrantSession {
    pub fn new(
        authority: Arc<StoreRightsAuthority>,
        signer: Arc<dyn InstanceSigner>,
        peer_authority: Arc<dyn GrantPeerAuthority>,
        user: String,
        role: String,
        device: String,
    ) -> scope::Result<Arc<Self>> {
        for value in [&user, &role, &device] {
            bounded(value)?;
        }
        authority.router.with_identity(|instance, _| {
            if instance != signer.instance_id() {
                return Err("instance mismatch".into());
            }
            public_key(&*signer).map(|_| ())
        })?;
        Ok(Arc::new(Self {
            authority,
            signer,
            peer_authority,
            state: Mutex::new(State {
                user,
                role,
                device,
                active: true,
                issued: BTreeMap::new(),
                last_exp: BTreeMap::new(),
            }),
        }))
    }
    /// Control-lane entry; the transport correlates by app_id and sends this
    /// result only to the peer owning this session. No browser identity input.
    pub async fn handle_request(self: &Arc<Self>, request: GrantRequest, now: u64) -> Value {
        let GrantRequest::Request { app_id } = request;
        match self.issue(&app_id, now).await {
            Ok(token) => serde_json::json!({"type":"scope.grant", "token":token}),
            Err(_) => {
                serde_json::json!({"type":"forbidden", "app_id":app_id, "reason":"forbidden"})
            }
        }
    }
    pub async fn issue(self: &Arc<Self>, app_id: &str, now: u64) -> scope::Result<String> {
        bounded(app_id)?;
        let owner = self.clone();
        let app = app_id.to_owned();
        self.authority
            .writer()
            .submit(ctox_store::WriteJob::new(Default::default(), move |tx| {
                owner
                    .issue_in_transaction(tx, &app, now)
                    .map_err(|e| ctox_store::StoreError::Invalid(e))
            }))
            .map_err(|_| "grant admission failed")?
            .await
            .map_err(|_| "grant issuance unavailable".into())
    }
    fn issue_in_transaction(
        &self,
        tx: &Transaction<'_>,
        app: &str,
        now: u64,
    ) -> scope::Result<String> {
        let rights = super::read(tx).map_err(|_| "grant rights unavailable")?;
        let head = ctox_store::change_log::head(tx).map_err(|_| "grant store unavailable")?;
        self.authority
            .router
            .with_snapshot(rights.rights_ver, &head.epoch, |instance, _| {
                let mut state = self.state.lock().map_err(|_| "grant session unavailable")?;
                if !state.active {
                    return Err("revoked".into());
                }
                self.peer_authority
                    .validate(tx, &state.user, &state.role, &state.device, app)?;
                if !state.last_exp.contains_key(app) && state.last_exp.len() >= MAX_APPS {
                    return Err("grant app budget".into());
                }
                let (collections, scope_hash) = resolve(tx, &rights, &state, app)?;
                let exp = now.checked_add(TTL).ok_or("invalid expiry")?.max(
                    state
                        .last_exp
                        .get(app)
                        .copied()
                        .unwrap_or(0)
                        .checked_add(1)
                        .ok_or("invalid expiry")?,
                );
                if exp.saturating_sub(now) > scope::PEER_LEASE_SECONDS {
                    return Err("grant renewal budget".into());
                }
                let claims = AppGrantClaims {
                    instance_id: instance.into(),
                    user_id: state.user.clone(),
                    app_id: app.into(),
                    scope_hash,
                    rights_ver: rights.rights_ver.to_string(),
                    device_id: state.device.clone(),
                    exp,
                    collections,
                };
                let bytes = signing_bytes(&claims)?;
                let signature = self
                    .signer
                    .sign(&bytes)
                    .map_err(|_| "instance signing unavailable")?;
                let grant = SignedGrant {
                    claims,
                    signature: URL_SAFE_NO_PAD.encode(signature),
                };
                verify_signature(&*self.signer, &grant)?;
                let token = URL_SAFE_NO_PAD.encode(canonical(&grant)?);
                if token.len() > MAX_TOKEN_BYTES {
                    return Err("grant exceeds budget".into());
                }
                state.last_exp.insert(app.into(), exp);
                state.issued.insert(
                    app.into(),
                    Issued {
                        fingerprint: scope::digest(token.as_bytes()),
                        claims: grant.claims,
                    },
                );
                Ok(token)
            })
    }
    /// Used by P8's native reader in its own SQLite snapshot. Authorization,
    /// reprojection and synchronous frame handoff share live policy/session
    /// fences. deliver must be bounded, must not await or re-enter this owner.
    pub fn with_read_in_transaction<T>(
        &self,
        tx: &Transaction<'_>,
        token: &str,
        collection: &str,
        now: u64,
        document: &Value,
        deliver: impl FnOnce(Value) -> scope::Result<T>,
    ) -> scope::Result<T> {
        self.with_verified_in_transaction(tx, token, now, |claims, rights, state| {
            let base = logical_collection(claims, collection, "read")?;
            let projected = rights.authorize_app_read(
                &claims.app_id,
                &state.role,
                &state.user,
                &base,
                document,
            )?;
            deliver(projected)
        })
    }
    /// App outbox/patch authorization and commit use the same writer snapshot.
    /// This grants no core command permit and never bypasses write_guard.
    pub fn with_write_in_transaction<T>(
        &self,
        tx: &Transaction<'_>,
        token: &str,
        collection: &str,
        now: u64,
        before: Option<&Value>,
        after: Option<&Value>,
        commit: impl FnOnce() -> scope::Result<T>,
    ) -> scope::Result<T> {
        self.with_verified_in_transaction(tx, token, now, |claims, rights, state| {
            let base = logical_collection(claims, collection, "write")?;
            rights.authorize_app_write(
                &claims.app_id,
                &state.role,
                &state.user,
                &base,
                before,
                after,
                None,
            )?;
            commit()
        })
    }
    /// Inspection for subscribe/control admission only. Claims are not a
    /// reusable authorization: actual reads/writes MUST use the fenced methods.
    pub fn verify_in_transaction(
        &self,
        tx: &Transaction<'_>,
        token: &str,
        now: u64,
    ) -> scope::Result<AppGrantClaims> {
        self.with_verified_in_transaction(tx, token, now, |claims, _, _| Ok(claims.clone()))
    }
    fn with_verified_in_transaction<T>(
        &self,
        tx: &Transaction<'_>,
        token: &str,
        now: u64,
        use_grant: impl FnOnce(&AppGrantClaims, &RightsSnapshot, &State) -> scope::Result<T>,
    ) -> scope::Result<T> {
        let grant = decode(token)?;
        verify_signature(&*self.signer, &grant)?;
        let rights = super::read(tx).map_err(|_| "grant rights unavailable")?;
        let head = ctox_store::change_log::head(tx).map_err(|_| "grant store unavailable")?;
        self.authority
            .router
            .with_snapshot(rights.rights_ver, &head.epoch, |instance, _| {
                let state = self.state.lock().map_err(|_| "grant session unavailable")?;
                let claims = &grant.claims;
                self.peer_authority.validate(
                    tx,
                    &state.user,
                    &state.role,
                    &state.device,
                    &claims.app_id,
                )?;
                if !state.active
                    || claims.instance_id != instance
                    || claims.user_id != state.user
                    || claims.device_id != state.device
                    || now >= claims.exp
                    || claims.exp.saturating_sub(now) > scope::PEER_LEASE_SECONDS
                    || claims.rights_ver != rights.rights_ver.to_string()
                    || !state
                        .issued
                        .get(&claims.app_id)
                        .is_some_and(|g| g.fingerprint == scope::digest(token.as_bytes()))
                {
                    return Err("revoked".into());
                }
                let (collections, hash) = resolve(tx, &rights, &state, &claims.app_id)?;
                if claims.collections != collections || claims.scope_hash != hash {
                    return Err("revoked".into());
                }
                // A retained signer fails after native session teardown. An old
                // public signature alone can never keep a retired authority alive.
                self.signer
                    .sign(&signing_bytes(claims)?)
                    .map_err(|_| "native session ended")?;
                use_grant(claims, &rights, &state)
            })
    }
    /// Call BEFORE authenticating a different identity/role/device; returned
    /// frames are per app, and old grants stop authorizing immediately.
    pub fn rebind(
        &self,
        user: String,
        role: String,
        device: String,
    ) -> scope::Result<Vec<ScopeRevoke>> {
        for value in [&user, &role, &device] {
            bounded(value)?;
        }
        let mut state = self.state.lock().map_err(|_| "grant session unavailable")?;
        if !state.active {
            return Err("revoked".into());
        }
        let revoked = revoke_all(&mut state);
        state.user = user;
        state.role = role;
        state.device = device;
        Ok(revoked)
    }
    pub fn revoke_app(&self, app: &str) -> scope::Result<Option<ScopeRevoke>> {
        let mut state = self.state.lock().map_err(|_| "grant session unavailable")?;
        Ok(state.issued.remove(app).map(|_| revoke_frame(app)))
    }
    pub fn retire(&self) -> scope::Result<Vec<ScopeRevoke>> {
        let mut state = self.state.lock().map_err(|_| "grant session unavailable")?;
        state.active = false;
        Ok(revoke_all(&mut state))
    }
    /// After a committed policy/schema change, adapters emit these frames on
    /// the existing control stream. Read validation already fails closed even
    /// before the notification is drained; this never polls or opens a DB.
    pub fn revocations_in_transaction(
        &self,
        tx: &Transaction<'_>,
        now: u64,
    ) -> scope::Result<Vec<ScopeRevoke>> {
        let rights = super::read(tx).map_err(|_| "grant rights unavailable")?;
        let mut state = self.state.lock().map_err(|_| "grant session unavailable")?;
        let invalid: Vec<_> = state
            .issued
            .iter()
            .filter_map(|(app, issued)| {
                let valid = state.active
                    && self
                        .peer_authority
                        .validate(tx, &state.user, &state.role, &state.device, app)
                        .is_ok()
                    && now < issued.claims.exp
                    && issued.claims.rights_ver == rights.rights_ver.to_string()
                    && resolve(tx, &rights, &state, app).is_ok_and(|(cols, hash)| {
                        cols == issued.claims.collections && hash == issued.claims.scope_hash
                    });
                (!valid).then(|| app.clone())
            })
            .collect();
        for app in &invalid {
            state.issued.remove(app);
        }
        Ok(invalid.iter().map(|app| revoke_frame(app)).collect())
    }
}
fn logical_collection(
    claims: &AppGrantClaims,
    wire: &str,
    operation: &str,
) -> scope::Result<String> {
    let entry = claims
        .collections
        .iter()
        .find(|c| c.collection == wire && c.rights.iter().any(|r| r == operation))
        .ok_or("forbidden")?;
    if entry.gen == 1 {
        return Ok(wire.to_owned());
    }
    wire.strip_suffix(&format!("@g{}", entry.gen))
        .map(str::to_owned)
        .ok_or("invalid collection".into())
}
fn revoke_frame(app: &str) -> ScopeRevoke {
    ScopeRevoke {
        kind: "scope_revoke",
        app_id: app.into(),
        reason: "revoked",
    }
}
fn revoke_all(state: &mut State) -> Vec<ScopeRevoke> {
    std::mem::take(&mut state.issued)
        .keys()
        .map(|app| revoke_frame(app))
        .collect()
}
