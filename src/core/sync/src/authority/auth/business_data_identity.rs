//! Source identity attestation over the existing native Sync signing key.
//! A valid proof authenticates the PINNED source, not collection permissions.
//! Trust must come from enrollment/SSH/authenticated provisioning, never the reply.
use super::{hex, invalid, public_key, unhex, SigningIdentity};
use crate::business_data_contract::{
    NativeBusinessDataPeerIdentity as PeerIdentity, NativeBusinessDataPrincipal as Principal,
    CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
};
use ring::{
    rand::{SecureRandom, SystemRandom},
    signature::{UnparsedPublicKey, ED25519},
};
use std::io;

const DOMAIN: &[u8] = b"ctox.sync.business-data.identity.v1\0";

pub fn fresh_challenge() -> io::Result<String> {
    let mut nonce = [0; 32];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| io::Error::other("identity challenge generation failed"))?;
    Ok(hex(&nonce))
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn signing_bytes(identity: &PeerIdentity) -> io::Result<Vec<u8>> {
    if identity.version != CTOX_BUSINESS_DATA_PROTOCOL_VERSION || !valid_id(&identity.instance_id) {
        return Err(invalid("invalid BusinessData identity"));
    }
    unhex::<32>(&identity.challenge)?;
    unhex::<32>(&identity.channel_binding)?;
    public_key(&identity.public_identity)?;
    if let Some(principal) = &identity.principal {
        if !valid_id(&principal.user_id) || principal.authorization_epoch > 9_007_199_254_740_991 {
            return Err(invalid("invalid BusinessData principal"));
        }
        if principal.device.as_ref().is_some_and(|device| {
            !valid_id(&device.pairing_id)
                || !valid_id(&device.device_id)
                || !valid_id(&device.proof_key_thumbprint)
        }) {
            return Err(invalid("invalid BusinessData device identity"));
        }
    }
    let mut value = serde_json::to_value(identity).map_err(io::Error::other)?;
    value
        .as_object_mut()
        .expect("identity is an object")
        .remove("signature");
    value.sort_all_objects();
    let mut bytes = DOMAIN.to_vec();
    bytes.extend(serde_json::to_vec(&value).map_err(io::Error::other)?);
    Ok(bytes)
}

impl SigningIdentity {
    /// The native host derives `instance_id` and principal from its own identity
    /// and current verified capability. An anonymous preflight has no principal.
    /// Never sign a caller-supplied user/epoch or treat a signature as a data grant.
    pub fn attest_business_data_identity(
        &self,
        instance_id: &str,
        challenge: &str,
        channel_binding: &str,
        principal: Option<Principal>,
    ) -> io::Result<PeerIdentity> {
        let mut identity = PeerIdentity {
            version: CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
            challenge: challenge.into(),
            channel_binding: channel_binding.into(),
            instance_id: instance_id.into(),
            public_identity: self.public_identity(),
            principal,
            signature: String::new(),
        };
        identity.signature = hex(self.key.sign(&signing_bytes(&identity)?).as_ref());
        Ok(identity)
    }
}

/// `expected_key` and `expected_instance` MUST come from trusted saved-target
/// provisioning. The response's key, room, URL and signaling ID cannot supply
/// that trust. The caller owns and consumes its fresh per-exchange challenge.
/// An anonymous proof returns no principal and cannot make a data session ready.
/// Revocation and collection/document policy must still be rechecked on use.
pub fn verify_peer_identity(
    identity: &PeerIdentity,
    expected_key: &str,
    expected_instance: &str,
    challenge: &str,
    expected_channel_binding: &str,
) -> io::Result<()> {
    if identity.public_identity != expected_key
        || identity.instance_id != expected_instance
        || identity.challenge != challenge
        || identity.channel_binding != expected_channel_binding
    {
        return Err(invalid(
            "BusinessData identity does not match the pinned target and challenge",
        ));
    }
    UnparsedPublicKey::new(&ED25519, public_key(expected_key)?)
        .verify(
            &signing_bytes(identity)?,
            &unhex::<64>(&identity.signature)?,
        )
        .map_err(|_| invalid("BusinessData identity signature rejected"))
}
