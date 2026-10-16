//! Public-key verification bound to the foreground's actual Store generation.
//! No key creation, secret lookup, cached role or client-supplied grant lives here.
use super::store_inputs::StoreInputs;
use crate::business_os::policy::{
    sync_rights::{scope::SignedScope, StoreRightsAuthority},
    BusinessOsActor,
};
use ctox_store::change_log::subscription::protocol::InstanceVerifier;
use ctox_store::StoreError;
use std::sync::{Arc, Weak};

pub(crate) struct CoreVerifier {
    inputs: Weak<StoreInputs>,
    authority: Arc<StoreRightsAuthority>,
    instance: String,
    key_id: String,
    public: Vec<u8>,
}
impl CoreVerifier {
    pub(super) fn new(
        inputs: &Arc<StoreInputs>,
        authority: Arc<StoreRightsAuthority>,
    ) -> Result<Arc<Self>, StoreError> {
        let signer = inputs.instance_signer()?;
        let public = signer
            .key_id()
            .strip_prefix("ed25519:")
            .filter(|hex| hex.len() == 64)
            .ok_or_else(|| StoreError::Invalid("invalid instance verification identity".into()))?;
        let public = public
            .as_bytes()
            .chunks_exact(2)
            .map(|part| {
                std::str::from_utf8(part)
                    .ok()
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                    .ok_or_else(|| {
                        StoreError::Invalid("invalid instance verification identity".into())
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Arc::new(Self {
            inputs: Arc::downgrade(inputs),
            authority,
            instance: signer.instance_id().into(),
            key_id: signer.key_id().into(),
            public,
        }))
    }
    fn current(&self) -> Result<Arc<StoreInputs>, String> {
        let inputs = self.inputs.upgrade().ok_or("verification owner retired")?;
        let signer = inputs
            .instance_signer()
            .map_err(|_| "verification owner retired")?;
        if signer.instance_id() != self.instance || signer.key_id() != self.key_id {
            return Err("verification generation changed".into());
        }
        Ok(inputs)
    }
    /// The caller supplies an authenticated Core actor, never a browser role.
    /// Resolve the scope under the live policy fence, including expiry, exact
    /// device, collection and rights version. A valid old signature is not a grant.
    pub(crate) fn verify_scope(
        &self,
        actor: &BusinessOsActor,
        device: &str,
        collection: &str,
        token: &SignedScope,
        now: u64,
    ) -> Result<(), String> {
        let _inputs = self.current()?;
        let id = actor
            .id
            .as_deref()
            .filter(|id| !id.is_empty())
            .ok_or("actor required")?;
        self.authority.router.with_identity(|instance, rights| {
            if instance != self.instance {
                return Err("instance changed".into());
            }
            let scope = rights.resolve_collection(actor.role.as_str(), id, collection)?;
            if scope.projections.is_empty() {
                return Err("collection denied".into());
            }
            token.verify(&self.public, instance, device, &scope, now)
        })?;
        self.current()?;
        Ok(())
    }
}
impl InstanceVerifier for CoreVerifier {
    fn verify(
        &self,
        instance: &str,
        key_id: &str,
        canonical: &[u8],
        signature: &[u8],
    ) -> Result<(), String> {
        let _inputs = self.current()?;
        if instance != self.instance || key_id != self.key_id {
            return Err("foreign instance signature".into());
        }
        ctox_sync::authority::auth::CanonicalInstancePacket::from_stream_bytes(canonical)
            .map_err(|_| "noncanonical instance packet")?;
        ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, &self.public)
            .verify(canonical, signature)
            .map_err(|_| "invalid instance signature")?;
        self.current()?;
        Ok(())
    }
}

// RxDB and Store include the same wire source in separate crates. Delegate,
// rather than inventing another verifier/key or accepting a transport identity.
impl rxdb::plugins::replication_webrtc::v4_stream::InstanceVerifier for CoreVerifier {
    fn verify(
        &self,
        instance: &str,
        key_id: &str,
        canonical: &[u8],
        signature: &[u8],
    ) -> Result<(), String> {
        InstanceVerifier::verify(self, instance, key_id, canonical, signature)
    }
}
