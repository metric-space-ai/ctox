//! Bind execution-control requests and replies to configured peer keys and fresh request nonces.
#[path = "auth/business_data_identity.rs"]
pub mod business_data_identity;
#[cfg(all(feature = "webrtc", unix))]
pub(crate) mod route;
#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
use super::{
    network::{ControlTransport, Packet, Reply},
    node::AuthorityNode,
    Peer,
};
use crate::{
    checkpoint::CheckpointStore,
    contracts::{CheckpointCopyReceipt, ExecutionOwnership, ExecutionSpec},
};
use async_trait::async_trait;
use ring::{
    rand::{SecureRandom, SystemRandom},
    signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{io, sync::Arc};
const MAX_ENVELOPE_BYTES: usize = 4 * 1024 * 1024;
const DOMAIN: &[u8] = b"ctox.sync.authority.envelope.v1\0";
const COPY_DOMAIN: &[u8] = b"ctox.sync.checkpoint.durable-copy.v1\0";
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn unhex<const N: usize>(text: &str) -> io::Result<[u8; N]> {
    if text.len() != N * 2 {
        return Err(invalid("invalid authority key, signature or nonce length"));
    }
    let mut bytes = [0; N];
    for (i, pair) in text.as_bytes().chunks_exact(2).enumerate() {
        fn nibble(b: u8) -> Option<u8> {
            match b {
                b'0'..=b'9' => Some(b - b'0'),
                b'a'..=b'f' => Some(b - b'a' + 10),
                _ => None,
            }
        }
        bytes[i] = (nibble(pair[0]).ok_or_else(|| invalid("non-canonical authority encoding"))?
            << 4)
            | nibble(pair[1]).ok_or_else(|| invalid("non-canonical authority encoding"))?;
    }
    Ok(bytes)
}
pub(crate) fn public_key(identity: &str) -> io::Result<[u8; 32]> {
    unhex(
        identity
            .strip_prefix("ed25519:")
            .ok_or_else(|| invalid("unsupported authority identity"))?,
    )
}

pub struct SigningIdentity {
    key: Ed25519KeyPair,
}
impl SigningIdentity {
    /// Import an existing host key without replacing its confirmed public identity.
    /// Node/OpenSSL PKCS#8 v1 lacks the public half, so require an independent pin.
    pub fn from_existing_pkcs8(bytes: &[u8], expected_identity: &str) -> io::Result<Self> {
        let expected = public_key(expected_identity)?;
        let key = Ed25519KeyPair::from_pkcs8_maybe_unchecked(bytes)
            .map_err(|_| invalid("invalid existing authority signing key"))?;
        if key.public_key().as_ref() != expected {
            return Err(invalid(
                "existing signing key does not match the confirmed identity",
            ));
        }
        Ok(Self { key })
    }
    pub fn from_pkcs8(bytes: &[u8]) -> io::Result<Self> {
        Ok(Self {
            key: Ed25519KeyPair::from_pkcs8(bytes)
                .map_err(|_| invalid("invalid authority signing key"))?,
        })
    }
    pub fn generate_pkcs8() -> io::Result<Vec<u8>> {
        Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
            .map(|p| p.as_ref().to_vec())
            .map_err(|_| io::Error::other("authority key generation failed"))
    }
    pub fn public_identity(&self) -> String {
        format!("ed25519:{}", hex(self.key.public_key().as_ref()))
    }
    /// A peer signs its own durable, complete copy, never a sender's replica list.
    /// The caller must run filesystem verification off the async control loop.
    pub fn acknowledge_checkpoint(
        &self,
        store: &CheckpointStore,
        node_id: u64,
        spec: &ExecutionSpec,
        ownership: &ExecutionOwnership,
        digest: &str,
    ) -> io::Result<CheckpointCopyReceipt> {
        let manifest = store.verify_durable_copy(digest)?;
        let session = &manifest.session;
        if session.scope_id != spec.scope_id
            || session.session_id != spec.session_id
            || session.harness != spec.harness
            || session.harness_version != spec.harness_version
            || session.model_route_id != spec.model_route_id
            || session.gateway_account_id != spec.gateway_account_id
            || session.model_id != spec.model_id
            || session.required_capabilities != spec.required_capabilities
            || !manifest.pending_effects.is_empty()
        {
            return Err(invalid(
                "checkpoint does not match the execution or requires reconciliation",
            ));
        }
        let mut receipt = CheckpointCopyReceipt {
            version: 1,
            node_id,
            spec: spec.clone(),
            ownership: ownership.clone(),
            checkpoint_digest: digest.to_owned(),
            sequence: manifest.sequence,
            signature: String::new(),
        };
        receipt.signature = hex(self.key.sign(&copy_signing_bytes(&receipt)?).as_ref());
        Ok(receipt)
    }
    fn sign(&self, body: Body) -> io::Result<Envelope> {
        let bytes = signing_bytes(&body)?;
        Ok(Envelope {
            body,
            signature: hex(self.key.sign(&bytes).as_ref()),
        })
    }
}
fn copy_signing_bytes(receipt: &CheckpointCopyReceipt) -> io::Result<Vec<u8>> {
    let mut data = serde_json::to_value(receipt).map_err(io::Error::other)?;
    data.as_object_mut()
        .expect("receipt is an object")
        .remove("signature");
    data.sort_all_objects();
    let mut bytes = COPY_DOMAIN.to_vec();
    bytes.extend(serde_json::to_vec(&data).map_err(io::Error::other)?);
    Ok(bytes)
}
pub(super) fn verify_checkpoint_copy(
    receipt: &CheckpointCopyReceipt,
    peer: &Peer,
) -> io::Result<()> {
    if receipt.version != 1 || !peer.data_replica {
        return Err(invalid("checkpoint receipt is not from a data peer"));
    }
    UnparsedPublicKey::new(&ED25519, public_key(&peer.identity)?)
        .verify(
            &copy_signing_bytes(receipt)?,
            &unhex::<64>(&receipt.signature)?,
        )
        .map_err(|_| invalid("checkpoint copy signature rejected"))
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Body {
    version: u32,
    sender: String,
    recipient: String,
    scope_id: String,
    nonce: String,
    kind: String,
    data: Value,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    body: Body,
    signature: String,
}
fn signing_bytes(body: &Body) -> io::Result<Vec<u8>> {
    // RxDB enables preserve_order; signatures must remain independent of it.
    let mut canonical = serde_json::to_value(body).map_err(io::Error::other)?;
    canonical.sort_all_objects();
    let payload = serde_json::to_vec(&canonical).map_err(io::Error::other)?;

    if payload.len() > MAX_ENVELOPE_BYTES {
        return Err(invalid("authority envelope exceeds its budget"));
    }
    let mut bytes = Vec::with_capacity(DOMAIN.len() + payload.len());
    bytes.extend_from_slice(DOMAIN);
    bytes.extend_from_slice(&payload);
    Ok(bytes)
}
fn verify(envelope: &Envelope, recipient: &str, scope: &str, kind: &str) -> io::Result<()> {
    let b = &envelope.body;
    if b.version != 1 || b.recipient != recipient || b.scope_id != scope || b.kind != kind {
        return Err(invalid(
            "authority envelope has wrong scope, recipient or protocol",
        ));
    }
    unhex::<16>(&b.nonce)?;
    UnparsedPublicKey::new(&ED25519, public_key(&b.sender)?)
        .verify(&signing_bytes(b)?, &unhex::<64>(&envelope.signature)?)
        .map_err(|_| invalid("authority envelope signature rejected"))
}
/// A CTOX Sync control channel. It may discover a route from signaling hints;
/// the signed exchange, not that hint, establishes the trusted peer identity.
#[async_trait]
pub trait ControlChannel: Send + Sync + 'static {
    async fn request(&self, target_identity: &str, envelope: Value) -> io::Result<Value>;
}
pub struct SignedTransport {
    identity: Arc<SigningIdentity>,
    scope_id: String,
    channel: Arc<dyn ControlChannel>,
}
impl SignedTransport {
    pub fn new(
        identity: Arc<SigningIdentity>,
        scope_id: String,
        channel: Arc<dyn ControlChannel>,
    ) -> Self {
        Self {
            identity,
            scope_id,
            channel,
        }
    }
}
#[async_trait]
impl ControlTransport for SignedTransport {
    async fn exchange(&self, target: &Peer, packet: Packet) -> io::Result<Reply> {
        public_key(&target.identity)?;
        if packet.scope_id != self.scope_id {
            return Err(invalid("outbound authority scope mismatch"));
        }
        let mut random = [0u8; 16];
        SystemRandom::new()
            .fill(&mut random)
            .map_err(|_| io::Error::other("authority nonce generation failed"))?;
        let nonce = hex(&random);
        let sender = self.identity.public_identity();
        let envelope = self.identity.sign(Body {
            version: 1,
            sender: sender.clone(),
            recipient: target.identity.clone(),
            scope_id: self.scope_id.clone(),
            nonce: nonce.clone(),
            kind: "request".into(),
            data: serde_json::to_value(packet).map_err(io::Error::other)?,
        })?;
        let raw = self
            .channel
            .request(
                &target.identity,
                serde_json::to_value(envelope).map_err(io::Error::other)?,
            )
            .await?;
        let reply: Envelope = serde_json::from_value(raw).map_err(io::Error::other)?;
        verify(&reply, &sender, &self.scope_id, "reply")?;
        if reply.body.sender != target.identity || reply.body.nonce != nonce {
            return Err(invalid("authority reply identity or nonce mismatch"));
        }
        serde_json::from_value(reply.body.data).map_err(io::Error::other)
    }
}
/// Registered as the explicit authority auxiliary method on an existing RxDB/WebRTC pool.
pub async fn receive(
    identity: &SigningIdentity,
    scope: &str,
    node: &AuthorityNode,
    raw: Value,
) -> io::Result<Value> {
    let envelope: Envelope = serde_json::from_value(raw).map_err(io::Error::other)?;
    let local = identity.public_identity();
    verify(&envelope, &local, scope, "request")?;
    let sender = envelope.body.sender.clone();
    let packet: Packet = serde_json::from_value(envelope.body.data).map_err(io::Error::other)?;
    let reply = node.handle(&sender, packet).await?;
    let signed = identity.sign(Body {
        version: 1,
        sender: local,
        recipient: sender,
        scope_id: scope.into(),
        nonce: envelope.body.nonce,
        kind: "reply".into(),
        data: serde_json::to_value(reply).map_err(io::Error::other)?,
    })?;
    serde_json::to_value(signed).map_err(io::Error::other)
}
