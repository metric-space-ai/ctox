//! Core-resolved projections and short-lived instance-signed scope authority (v4 §16.2).
//! No filesystem, network, environment or credential lookup occurs in this module.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ring::signature::{Ed25519KeyPair, UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[path = "../rxdb/src/plugins/replication_webrtc/scope_filter.rs"]
pub mod scope_filter;

pub type Result<T> = std::result::Result<T, String>;
pub const INSTANCE_SIGNING_KEY: &str = "instance_signing_key";
pub const PEER_LEASE_SECONDS: u64 = 300;
pub const PEER_RENEW_SECONDS: u64 = 120;

/// Implemented by the Core's official SecretStore adapter. Key bytes are borrowed
/// only inside this callback; there is no fallback to files, env or a generated key.
pub trait InstanceSigningKeyStore {
    fn with_instance_signing_key<T>(&self, use_key: impl FnOnce(&[u8]) -> Result<T>) -> Result<T>;
}

pub fn canonical_json(value: &Value) -> Result<Vec<u8>> {
    fn ordered(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut keys: Vec<_> = map.keys().collect();
                keys.sort();
                let mut out = Map::new();
                for key in keys {
                    out.insert(key.clone(), ordered(&map[key]));
                }
                Value::Object(out)
            }
            Value::Array(values) => Value::Array(values.iter().map(ordered).collect()),
            value => value.clone(),
        }
    }
    serde_json::to_vec(&ordered(value)).map_err(|_| "invalid canonical JSON".into())
}

fn canonical<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    canonical_json(&serde_json::to_value(value).map_err(|_| "invalid scope encoding")?)
}

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FilterValue {
    Literal { value: Value },
    ActorId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum RowFilter {
    All,
    None,
    Eq { path: String, value: FilterValue },
    And { filters: Vec<RowFilter> },
    Or { filters: Vec<RowFilter> },
}

fn path_parts(path: &str) -> Result<Vec<String>> {
    if !path.starts_with('/') || path.len() > 1024 {
        return Err("field paths must be bounded JSON pointers".into());
    }
    let mut parts = Vec::new();
    for part in path[1..].split('/') {
        let mut decoded = String::new();
        let mut chars = part.chars();
        while let Some(c) = chars.next() {
            if c != '~' {
                decoded.push(c);
            } else {
                decoded.push(match chars.next() {
                    Some('0') => '~',
                    Some('1') => '/',
                    _ => return Err("invalid JSON pointer escape".into()),
                });
            }
        }
        if decoded.is_empty() || parts.len() >= 16 {
            return Err("empty field or excessive field nesting".into());
        }
        parts.push(decoded);
    }
    Ok(parts)
}

impl RowFilter {
    pub fn resolve(&self, actor_id: &str) -> Result<Self> {
        fn visit(
            filter: &RowFilter,
            actor: &str,
            depth: usize,
            count: &mut usize,
        ) -> Result<RowFilter> {
            *count += 1;
            if depth > 16 || *count > 256 {
                return Err("row filter exceeds complexity limit".into());
            }
            match filter {
                RowFilter::All | RowFilter::None => Ok(filter.clone()),
                RowFilter::Eq { path, value } => {
                    path_parts(path)?;
                    let value = match value {
                        FilterValue::Literal { value } => value.clone(),
                        FilterValue::ActorId if !actor.is_empty() => Value::String(actor.into()),
                        FilterValue::ActorId => return Err("actor identity required".into()),
                    };
                    if canonical_json(&value)?.len() > 16_384 {
                        return Err("row filter literal too large".into());
                    }
                    Ok(RowFilter::Eq {
                        path: path.clone(),
                        value: FilterValue::Literal { value },
                    })
                }
                RowFilter::And { filters } | RowFilter::Or { filters } => {
                    let and = matches!(filter, RowFilter::And { .. });
                    let mut normalized = Vec::new();
                    for child in filters {
                        let child = visit(child, actor, depth + 1, count)?;
                        match (and, &child) {
                            (true, RowFilter::None) => return Ok(RowFilter::None),
                            (false, RowFilter::All) => return Ok(RowFilter::All),
                            (true, RowFilter::All) | (false, RowFilter::None) => {}
                            (true, RowFilter::And { filters })
                            | (false, RowFilter::Or { filters }) => {
                                normalized.extend(filters.clone())
                            }
                            _ => normalized.push(child),
                        }
                    }
                    let mut keyed = normalized
                        .into_iter()
                        .map(|f| Ok((canonical(&f)?, f)))
                        .collect::<Result<Vec<_>>>()?;
                    keyed.sort_by(|a, b| a.0.cmp(&b.0));
                    keyed.dedup_by(|a, b| a.0 == b.0);
                    let mut filters: Vec<_> = keyed.into_iter().map(|(_, f)| f).collect();
                    Ok(match filters.len() {
                        0 if and => RowFilter::All,
                        0 => RowFilter::None,
                        1 => filters.remove(0),
                        _ if and => RowFilter::And { filters },
                        _ => RowFilter::Or { filters },
                    })
                }
            }
        }
        visit(self, actor_id, 0, &mut 0)
    }

    pub fn matches(&self, doc: &Value) -> bool {
        match self {
            Self::All => true,
            Self::None => false,
            Self::Eq {
                path,
                value: FilterValue::Literal { value },
            } => doc.pointer(path).is_some_and(|actual| actual == value),
            Self::Eq {
                value: FilterValue::ActorId,
                ..
            } => false,
            Self::And { filters } => filters.iter().all(|f| f.matches(doc)),
            Self::Or { filters } => filters.iter().any(|f| f.matches(doc)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum FieldMask {
    All,
    Fields { paths: BTreeSet<String> },
}

impl FieldMask {
    pub fn normalize(&self) -> Result<Self> {
        match self {
            Self::All => Ok(Self::All),
            Self::Fields { paths } => {
                if paths.len() > 256 {
                    return Err("too many field paths".into());
                }
                for path in paths {
                    path_parts(path)?;
                }
                let paths = paths
                    .iter()
                    .filter(|path| {
                        !paths.iter().any(|parent| {
                            *path != parent && path.starts_with(&format!("{parent}/"))
                        })
                    })
                    .cloned()
                    .collect();
                Ok(Self::Fields { paths })
            }
        }
    }

    pub fn allows(&self, path: &str) -> bool {
        match self {
            Self::All => true,
            Self::Fields { paths } => paths
                .iter()
                .any(|parent| path == parent || path.starts_with(&format!("{parent}/"))),
        }
    }

    /// Masking walks object fields only. Arrays may be selected as a whole field;
    /// element masks are rejected instead of accidentally copying an entire array.
    pub fn apply(&self, doc: &Value) -> Result<Value> {
        if !doc.is_object() {
            return Err("replicated documents must be objects".into());
        }
        if matches!(self, Self::All) {
            return Ok(doc.clone());
        }
        let Self::Fields { paths } = self else {
            unreachable!()
        };
        fn copy_path(source: &Value, target: &mut Value, parts: &[String]) -> Result<()> {
            let Some(map) = source.as_object() else {
                return Err("field masks only traverse objects".into());
            };
            let Some(value) = map.get(&parts[0]) else {
                return Ok(());
            };
            let out = target.as_object_mut().ok_or("invalid masked document")?;
            if parts.len() == 1 {
                out.insert(parts[0].clone(), value.clone());
            } else {
                copy_path(
                    value,
                    out.entry(parts[0].clone())
                        .or_insert_with(|| Value::Object(Map::new())),
                    &parts[1..],
                )?;
            }
            Ok(())
        }
        let mut target = Value::Object(Map::new());
        for path in paths {
            copy_path(doc, &mut target, &path_parts(path)?)?;
        }
        Ok(target)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Projection {
    pub collection: String,
    pub row_filter: RowFilter,
    pub field_mask: FieldMask,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedScope {
    pub rights_ver: u64,
    pub projections: Vec<Projection>,
}

impl ResolvedScope {
    pub fn new(rights_ver: u64, projections: Vec<Projection>, actor_id: &str) -> Result<Self> {
        let mut projections = projections
            .into_iter()
            .map(|p| {
                if p.collection.is_empty() || p.collection.len() > 256 {
                    return Err("invalid scope collection".into());
                }
                Ok(Projection {
                    collection: p.collection,
                    row_filter: p.row_filter.resolve(actor_id)?,
                    field_mask: p.field_mask.normalize()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        projections.sort_by(|a, b| a.collection.cmp(&b.collection));
        if projections
            .windows(2)
            .any(|p| p[0].collection == p[1].collection)
        {
            return Err("duplicate collection projection".into());
        }
        if rights_ver == 0 || projections.len() > 4096 {
            return Err("invalid scope version/size".into());
        }
        Ok(Self {
            rights_ver,
            projections,
        })
    }

    pub fn hash(&self) -> Result<String> {
        Ok(digest(&canonical(self)?))
    }
    pub fn projection(&self, collection: &str) -> Option<&Projection> {
        self.projections.iter().find(|p| p.collection == collection)
    }
    /// Called only by the Core. Equal version numbers alone never prove completeness.
    pub fn carry_checkpoint(&self, next: &Self, collection: &str, checkpoint: u64) -> u64 {
        match (self.projection(collection), next.projection(collection)) {
            (Some(old), Some(new)) if old == new => checkpoint,
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeClaims {
    pub instance_id: String,
    pub scope_hash: String,
    pub rights_ver: u64,
    pub device_id: String,
    pub exp: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedScope {
    pub claims: ScopeClaims,
    pub signature: String,
}

fn domain_message(domain: &[u8], claims: &impl Serialize) -> Result<Vec<u8>> {
    let mut message = domain.to_vec();
    message.extend(canonical(claims)?);
    Ok(message)
}
fn sign(store: &impl InstanceSigningKeyStore, message: &[u8]) -> Result<String> {
    store.with_instance_signing_key(|bytes| {
        let key = Ed25519KeyPair::from_pkcs8(bytes).map_err(|_| "invalid instance signing key")?;
        Ok(URL_SAFE_NO_PAD.encode(key.sign(message).as_ref()))
    })
}
fn verify(public_key: &[u8], signature: &str, message: &[u8]) -> Result<()> {
    let signature = URL_SAFE_NO_PAD
        .decode(signature)
        .map_err(|_| "invalid signature encoding")?;
    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(message, &signature)
        .map_err(|_| "invalid instance signature".into())
}
fn check_claims(
    claims: &ScopeClaims,
    instance: &str,
    device: &str,
    scope: &ResolvedScope,
    now: u64,
) -> Result<()> {
    if claims.instance_id != instance
        || claims.device_id != device
        || claims.rights_ver != scope.rights_ver
        || claims.scope_hash != scope.hash()?
        || now >= claims.exp
        || claims.exp.saturating_sub(now) > PEER_LEASE_SECONDS
    {
        return Err("scope authority expired, revoked or mismatched".into());
    }
    Ok(())
}

impl SignedScope {
    pub fn issue(
        store: &impl InstanceSigningKeyStore,
        instance: &str,
        device: &str,
        scope: &ResolvedScope,
        now: u64,
    ) -> Result<Self> {
        if instance.is_empty() || device.is_empty() {
            return Err("identity required".into());
        }
        let claims = ScopeClaims {
            instance_id: instance.into(),
            device_id: device.into(),
            scope_hash: scope.hash()?,
            rights_ver: scope.rights_ver,
            exp: now
                .checked_add(PEER_LEASE_SECONDS)
                .ok_or("invalid expiry")?,
        };
        let signature = sign(store, &domain_message(b"ctox:v4:scope\0", &claims)?)?;
        Ok(Self { claims, signature })
    }
    pub fn verify(
        &self,
        public_key: &[u8],
        instance: &str,
        device: &str,
        scope: &ResolvedScope,
        now: u64,
    ) -> Result<()> {
        verify(
            public_key,
            &self.signature,
            &domain_message(b"ctox:v4:scope\0", &self.claims)?,
        )?;
        check_claims(&self.claims, instance, device, scope, now)
    }
}

/// A lease is separately domain-signed; a scope token is not a peer lease.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerLease {
    pub claims: ScopeClaims,
    pub signature: String,
}
impl PeerLease {
    pub fn issue(
        store: &impl InstanceSigningKeyStore,
        instance: &str,
        device: &str,
        scope: &ResolvedScope,
        now: u64,
    ) -> Result<Self> {
        let token = SignedScope::issue(store, instance, device, scope, now)?;
        let claims = token.claims;
        let signature = sign(store, &domain_message(b"ctox:v4:peer-lease\0", &claims)?)?;
        Ok(Self { claims, signature })
    }
    pub fn renewal_due(&self, now: u64) -> bool {
        now >= self
            .claims
            .exp
            .saturating_sub(PEER_LEASE_SECONDS - PEER_RENEW_SECONDS)
    }
    pub fn verify(
        &self,
        public_key: &[u8],
        instance: &str,
        device: &str,
        scope: &ResolvedScope,
        now: u64,
    ) -> Result<()> {
        verify(
            public_key,
            &self.signature,
            &domain_message(b"ctox:v4:peer-lease\0", &self.claims)?,
        )?;
        check_claims(&self.claims, instance, device, scope, now)
    }
}

pub mod decimal {
    use serde::{de::Error, Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &u64, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&v.to_string())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<u64, D::Error> {
        let s = String::deserialize(d)?;
        if s.is_empty()
            || !s.bytes().all(|b| b.is_ascii_digit())
            || (s.len() > 1 && s.starts_with('0'))
        {
            return Err(D::Error::custom("canonical decimal seq required"));
        }
        s.parse().map_err(D::Error::custom)
    }
}

/// Signed envelope binds the payload to its exact collection, generation and range.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerPacket {
    pub instance_id: String,
    pub scope_hash: String,
    pub rights_ver: u64,
    pub epoch: String,
    pub collection: String,
    pub gen: u64,
    pub package_id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(with = "decimal")]
    pub seq_from: u64,
    #[serde(with = "decimal")]
    pub seq_to: u64,
    pub payload_hash: String,
}
impl PeerPacket {
    pub fn sign(&self, store: &impl InstanceSigningKeyStore) -> Result<String> {
        sign(store, &domain_message(b"ctox:v4:peer-packet\0", self)?)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn authorize(
        &self,
        signature: &str,
        payload: &[u8],
        public_key: &[u8],
        sender: (&str, &PeerLease),
        receiver: (&str, &PeerLease),
        instance: &str,
        epoch: &str,
        gen: u64,
        scope: &ResolvedScope,
        now: u64,
    ) -> Result<()> {
        sender
            .1
            .verify(public_key, instance, sender.0, scope, now)?;
        receiver
            .1
            .verify(public_key, instance, receiver.0, scope, now)?;
        if sender.0 == receiver.0
            || self.instance_id != instance
            || self.epoch != epoch
            || self.gen != gen
            || gen == 0
            || self.seq_from > self.seq_to
            || self.package_id.is_empty()
            || !matches!(self.kind.as_str(), "page" | "live")
            || self.rights_ver != scope.rights_ver
            || self.scope_hash != scope.hash()?
            || scope.projection(&self.collection).is_none()
            || self.payload_hash != digest(payload)
        {
            return Err("peer packet outside current projection".into());
        }
        verify(
            public_key,
            signature,
            &domain_message(b"ctox:v4:peer-packet\0", self)?,
        )
    }
}

#[cfg(test)]
#[path = "scope_tests.rs"]
mod tests;

/// Persisted checkpoint domain. A projection-equivalence declaration never
/// carries a cursor across an instance, epoch, collection or generation change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedCheckpoint {
    pub instance_id: String,
    pub epoch: String,
    pub collection: String,
    pub generation: u64,
    pub scope_hash: String,
    #[serde(with = "decimal")]
    pub seq: u64,
}
impl ScopedCheckpoint {
    #[allow(clippy::too_many_arguments)]
    pub fn reuse_for(
        &self,
        old: &ResolvedScope,
        next: &ResolvedScope,
        instance: &str,
        epoch: &str,
        collection: &str,
        generation: u64,
    ) -> Result<u64> {
        if self.instance_id != instance
            || self.epoch != epoch
            || self.collection != collection
            || self.generation != generation
            || generation == 0
            || self.scope_hash != old.hash()?
        {
            return Ok(0);
        }
        Ok(old.carry_checkpoint(next, collection, self.seq))
    }
}
