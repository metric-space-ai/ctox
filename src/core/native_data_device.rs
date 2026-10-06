//! Native recipient proof keys in CTOX's existing encrypted secret store.
//! A key is possession evidence, never an account, enrollment or file grant.
use anyhow::{ensure, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ctox_sync::business_data_contract::NativeBusinessDataDeviceIdentity;
use ring::{
    rand::SystemRandom,
    signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_FIXED_SIGNING},
};
use rxdb::plugins::replication_webrtc::LocalDeviceProof;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

const SECRET_SCOPE: &str = "ctox-native-business-data-devices";

/// Supplied by native enrollment. Resume keeps the original target/account
/// epoch; it cannot silently create a key for a newly logged-in account.
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeDeviceKeyScope {
    pub target_id: String,
    pub source_instance_id: String,
    pub source_public_identity: String,
    pub account_epoch: u64,
}
impl NativeDeviceKeyScope {
    fn validate(&self) -> Result<()> {
        for value in [&self.target_id, &self.source_instance_id] {
            ensure!(
                !value.is_empty()
                    && value.len() <= 256
                    && value.trim() == value
                    && !value.chars().any(char::is_control),
                "invalid native device target"
            );
        }
        ensure!(
            self.account_epoch > 0,
            "invalid native device account epoch"
        );
        ensure!(
            self.source_public_identity.len() == 72
                && self.source_public_identity.starts_with("ed25519:")
                && self.source_public_identity[8..]
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "invalid native device source pin"
        );
        Ok(())
    }
    fn record_name(&self) -> Result<String> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)?;
        Ok(ring::digest::digest(&ring::digest::SHA256, &bytes)
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredKey {
    version: u8,
    scope: NativeDeviceKeyScope,
    pkcs8: String,
}

/// No Debug/Serialize implementation: the private key never enters a receipt,
/// renderer contract or caller-visible enrollment descriptor.
pub(crate) struct NativeDeviceProofKey {
    key: EcdsaKeyPair,
    x: String,
    y: String,
    thumbprint: String,
}
impl NativeDeviceProofKey {
    /// Explicit native enrollment only. Existing/corrupt records never rotate
    /// implicitly; reconnect must use `load`, which cannot generate a key.
    pub(crate) fn prepare(root: &Path, scope: &NativeDeviceKeyScope) -> Result<Self> {
        let name = scope.record_name()?;
        let _guard = crate::secrets::credential_lifecycle_guard();
        if crate::secrets::secret_exists(root, SECRET_SCOPE, &name)? {
            return Self::load(root, scope);
        }
        let bytes =
            EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &SystemRandom::new())
                .map_err(|_| anyhow::anyhow!("native device proof key unavailable"))?;
        let stored = StoredKey {
            version: 1,
            scope: scope.clone(),
            pkcs8: URL_SAFE_NO_PAD.encode(bytes.as_ref()),
        };
        Self::persist_candidate(root, scope, &stored)
    }

    fn persist_candidate(
        root: &Path,
        scope: &NativeDeviceKeyScope,
        stored: &StoredKey,
    ) -> Result<Self> {
        // The process-local preparation guard is only an optimization. This
        // atomic create arbitrates independent daemons preparing the same
        // scope; neither can replace the already persisted recipient identity.
        Self::restore(stored, scope)?;
        crate::secrets::create_secret_record_if_absent(
            root,
            SECRET_SCOPE,
            &scope.record_name()?,
            &serde_json::to_string(stored)?,
            json!({"version":1}),
        )?;
        // Always return the stored winner, including when another process won
        // after our initial absence check. Corrupt existing state stays closed.
        Self::load(root, scope)
    }

    pub(crate) fn load(root: &Path, scope: &NativeDeviceKeyScope) -> Result<Self> {
        let name = scope.record_name()?;
        let unavailable = || anyhow::anyhow!("native device proof key unavailable");
        let value = crate::secrets::read_secret_value(root, SECRET_SCOPE, &name)
            .map_err(|_| unavailable())?;
        ensure!(value.len() <= 8192, "native device proof key unavailable");
        let stored: StoredKey = serde_json::from_str(&value).map_err(|_| unavailable())?;
        Self::restore(&stored, scope)
    }

    fn restore(stored: &StoredKey, expected: &NativeDeviceKeyScope) -> Result<Self> {
        ensure!(
            stored.version == 1 && stored.scope == *expected && stored.pkcs8.len() <= 4096,
            "native device proof key unavailable"
        );
        let bytes = URL_SAFE_NO_PAD
            .decode(&stored.pkcs8)
            .map_err(|_| anyhow::anyhow!("native device proof key unavailable"))?;
        let key = EcdsaKeyPair::from_pkcs8(
            &ECDSA_P256_SHA256_FIXED_SIGNING,
            &bytes,
            &SystemRandom::new(),
        )
        .map_err(|_| anyhow::anyhow!("native device proof key unavailable"))?;
        let public = key.public_key().as_ref();
        ensure!(
            public.len() == 65 && public[0] == 4,
            "native device proof key unavailable"
        );
        let x = URL_SAFE_NO_PAD.encode(&public[1..33]);
        let y = URL_SAFE_NO_PAD.encode(&public[33..65]);
        // RFC7638, exactly the native source validator's canonical JWK order.
        let canonical = format!(r#"{{"crv":"P-256","kty":"EC","x":"{x}","y":"{y}"}}"#);
        let thumbprint = URL_SAFE_NO_PAD
            .encode(ring::digest::digest(&ring::digest::SHA256, canonical.as_bytes()).as_ref());
        Ok(Self {
            key,
            x,
            y,
            thumbprint,
        })
    }

    pub(crate) fn public_jwk(&self) -> Value {
        json!({"kty":"EC","crv":"P-256","x":self.x,"y":self.y})
    }
    /// The existing native one-time invite binding uses the key thumbprint as
    /// both stable device and pairing ID. These public values grant no access.
    pub(crate) fn device_identity(&self) -> NativeBusinessDataDeviceIdentity {
        NativeBusinessDataDeviceIdentity {
            pairing_id: self.thumbprint.clone(),
            device_id: self.thumbprint.clone(),
            proof_key_thumbprint: self.thumbprint.clone(),
        }
    }
    /// Called only by the host's connection-bound credentials provider after
    /// source proof. The key does not choose/verify a remote target itself.
    pub(crate) fn sign_nonce(&self, nonce: &str) -> Result<LocalDeviceProof> {
        ensure!(
            nonce.len() == 43
                && nonce
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
            "invalid native device proof nonce"
        );
        let signature = self
            .key
            .sign(&SystemRandom::new(), nonce.as_bytes())
            .map_err(|_| anyhow::anyhow!("native device proof key unavailable"))?;
        Ok(LocalDeviceProof {
            public_x: self.x.clone(),
            public_y: self.y.clone(),
            signature: URL_SAFE_NO_PAD.encode(signature.as_ref()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scope() -> NativeDeviceKeyScope {
        NativeDeviceKeyScope {
            target_id: "source-one".into(),
            source_instance_id: "instance-one".into(),
            source_public_identity: format!("ed25519:{}", "a".repeat(64)),
            account_epoch: 1,
        }
    }
    #[test]
    fn reopened_key_preserves_identity_and_signs_a_real_challenge() {
        let root = tempfile::tempdir().unwrap();
        let scope = scope();
        let identity = NativeDeviceProofKey::prepare(root.path(), &scope)
            .unwrap()
            .device_identity();
        let key = NativeDeviceProofKey::load(root.path(), &scope).unwrap();
        assert_eq!(key.device_identity(), identity);
        assert_eq!(
            NativeDeviceProofKey::prepare(root.path(), &scope)
                .unwrap()
                .device_identity(),
            identity
        );
        let nonce = "n".repeat(43);
        let proof = key.sign_nonce(&nonce).unwrap();
        let mut public = vec![4];
        public.extend(URL_SAFE_NO_PAD.decode(proof.public_x).unwrap());
        public.extend(URL_SAFE_NO_PAD.decode(proof.public_y).unwrap());
        let verifier = ring::signature::UnparsedPublicKey::new(
            &ring::signature::ECDSA_P256_SHA256_FIXED,
            public,
        );
        let signature = URL_SAFE_NO_PAD.decode(proof.signature).unwrap();
        verifier.verify(nonce.as_bytes(), &signature).unwrap();
        assert!(verifier
            .verify("x".repeat(43).as_bytes(), &signature)
            .is_err());
        assert!(key.sign_nonce("").is_err());
        assert!(key.sign_nonce(&" ".repeat(43)).is_err());
        assert_eq!(
            crate::secrets::list_secret_records(root.path(), Some(SECRET_SCOPE)).unwrap()[0]
                .metadata,
            json!({"version":1})
        );
    }
    #[test]
    fn restore_cannot_generate_identity_or_reuse_a_different_target_account() {
        let root = tempfile::tempdir().unwrap();
        let original = scope();
        assert!(NativeDeviceProofKey::load(root.path(), &original).is_err());
        NativeDeviceProofKey::prepare(root.path(), &original).unwrap();
        for changed in [
            NativeDeviceKeyScope {
                account_epoch: 2,
                ..original.clone()
            },
            NativeDeviceKeyScope {
                target_id: "other".into(),
                ..original.clone()
            },
            NativeDeviceKeyScope {
                source_instance_id: "other".into(),
                ..original.clone()
            },
            NativeDeviceKeyScope {
                source_public_identity: format!("ed25519:{}", "b".repeat(64)),
                ..original.clone()
            },
        ] {
            assert!(NativeDeviceProofKey::load(root.path(), &changed).is_err());
        }
    }
    #[test]
    fn competing_prepared_candidates_return_one_durable_identity() {
        let root = tempfile::tempdir().unwrap();
        let scope = scope();
        // Initialize the shared SecretStore first: the race under test is the
        // recipient record, not an unrelated master-key bootstrap migration.
        crate::secrets::write_secret_record(
            root.path(),
            "fixture",
            "ready",
            "ready",
            None,
            json!({}),
        )
        .unwrap();
        let mut candidates = Vec::new();
        for _ in 0..2 {
            assert!(!crate::secrets::secret_exists(
                root.path(),
                SECRET_SCOPE,
                &scope.record_name().unwrap()
            )
            .unwrap());
            let bytes = EcdsaKeyPair::generate_pkcs8(
                &ECDSA_P256_SHA256_FIXED_SIGNING,
                &SystemRandom::new(),
            )
            .unwrap();
            candidates.push(StoredKey {
                version: 1,
                scope: scope.clone(),
                pkcs8: URL_SAFE_NO_PAD.encode(bytes.as_ref()),
            });
        }
        assert_ne!(
            NativeDeviceProofKey::restore(&candidates[0], &scope)
                .unwrap()
                .device_identity(),
            NativeDeviceProofKey::restore(&candidates[1], &scope)
                .unwrap()
                .device_identity()
        );
        // Both contenders have already observed absence and generated different
        // identities before either INSERT. Independent SQLite connections, not
        // the process-local preparation mutex, decide and preserve the winner.
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let identities = std::thread::scope(|threads| {
            let handles = candidates
                .into_iter()
                .map(|candidate| {
                    let barrier = barrier.clone();
                    let scope = &scope;
                    let root = root.path();
                    threads.spawn(move || {
                        barrier.wait();
                        NativeDeviceProofKey::persist_candidate(root, scope, &candidate)
                            .unwrap()
                            .device_identity()
                    })
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(identities[0], identities[1]);
        assert_eq!(
            NativeDeviceProofKey::load(root.path(), &scope)
                .unwrap()
                .device_identity(),
            identities[0]
        );
        assert_eq!(
            crate::secrets::list_secret_records(root.path(), Some(SECRET_SCOPE))
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn corrupt_saved_key_is_not_replaced_by_explicit_prepare() {
        let root = tempfile::tempdir().unwrap();
        let scope = scope();
        let name = scope.record_name().unwrap();
        NativeDeviceProofKey::prepare(root.path(), &scope).unwrap();
        crate::secrets::write_secret_record(
            root.path(),
            SECRET_SCOPE,
            &name,
            "corrupt",
            None,
            json!({"version":1}),
        )
        .unwrap();
        assert!(NativeDeviceProofKey::load(root.path(), &scope).is_err());
        assert!(NativeDeviceProofKey::prepare(root.path(), &scope).is_err());
        let bytes =
            EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &SystemRandom::new())
                .unwrap();
        let stale = StoredKey {
            version: 1,
            scope: scope.clone(),
            pkcs8: URL_SAFE_NO_PAD.encode(bytes.as_ref()),
        };
        // A contender that observed absence earlier also cannot overwrite a
        // corrupt record that appeared before its atomic creation attempt.
        assert!(NativeDeviceProofKey::persist_candidate(root.path(), &scope, &stale).is_err());
        assert_eq!(
            crate::secrets::read_secret_value(root.path(), SECRET_SCOPE, &name).unwrap(),
            "corrupt"
        );
    }
}
