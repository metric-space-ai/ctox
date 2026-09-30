//! Durable source authority for native file transfers on the authenticated
//! WebRTC channel. No grant is accepted from renderer state or a restored job.
use super::{
    policy::BusinessOsPermission, rxdb_peer_desktop_files::desktop_file_transfer_metadata, store,
};
use crate::{
    secrets,
    transfers_grant::{
        valid_grant_id, TransferGrantReply, TransferGrantRequest, TransferGrantScope,
    },
};
use ctox_sync::business_data_contract::{
    NativeBusinessDataDeviceIdentity, NativeBusinessDataPrincipal,
};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

const SECRET_SCOPE: &str = "ctox-native-transfer-grants";
const GRANT_TTL_MS: i64 = 60 * 60 * 1000;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IssuedGrant {
    version: u8,
    scope: TransferGrantScope,
    principal: NativeBusinessDataPrincipal,
    content_generation: String,
    expires_at_ms: i64,
    revoked: bool,
}
fn denied() -> String {
    "native transfer grant unavailable or unauthorized".into()
}
fn principal(root: &Path, token: &str) -> Result<NativeBusinessDataPrincipal, String> {
    let claims = store::verified_webrtc_capability_claims(root, token).ok_or_else(denied)?;
    // Transfer authority is always a revocable enrolled daemon/device edge.
    // Ordinary bearer-only sessions remain valid for other surfaces but cannot
    // issue, check or revoke a native background transfer grant.
    let device = claims.device_binding.ok_or_else(denied)?;
    Ok(NativeBusinessDataPrincipal {
        user_id: claims.user_id,
        authorization_epoch: u64::try_from(claims.actor_epoch).map_err(|_| denied())?,
        device: Some(NativeBusinessDataDeviceIdentity {
            pairing_id: device.device_pairing_id,
            device_id: device.device_id,
            proof_key_thumbprint: device.proof_key_thumbprint,
        }),
    })
}
fn current_content(root: &Path, token: &str, scope: &TransferGrantScope) -> Result<String, String> {
    scope.validate().map_err(|_| denied())?;
    let identity = crate::sync_host::signing_identity(root).map_err(|_| denied())?;
    let config = store::sync_connection_config(root).map_err(|_| denied())?;
    if identity.public_identity() != scope.source_public_key
        || config.instance_id != scope.source_instance_id
        || !store::webrtc_capability_allows_collection_permission(
            root,
            token,
            &scope.collection,
            BusinessOsPermission::DataRead,
        )
    {
        return Err(denied());
    }
    // Same native metadata/generation as the actual desktop file-demand source;
    // a removed/lazy/replaced file cannot authorize even a completed cache hit.
    let content = desktop_file_transfer_metadata(root, &scope.file_id)
        .map_err(|_| denied())?
        .ok_or_else(denied)?;
    if content.content_hash != scope.sha256 || content.size_bytes != scope.size {
        return Err(denied());
    }
    Ok(content.generation_id)
}
fn save(root: &Path, id: &str, grant: &IssuedGrant) -> Result<(), String> {
    let value = serde_json::to_string(grant).map_err(|_| denied())?;
    secrets::write_secret_records(
        root,
        &[secrets::SecretRecordWrite {
            scope: SECRET_SCOPE,
            name: id,
            value: &value,
            description: Some("Native exact-scope transfer grant"),
            // No principal, file, content hash or credential in plaintext metadata.
            metadata: json!({"version":1}),
        }],
    )
    .map_err(|_| denied())
}
fn load(root: &Path, id: &str) -> Result<IssuedGrant, String> {
    if !valid_grant_id(id) {
        return Err(denied());
    }
    let value = secrets::read_secret_value(root, SECRET_SCOPE, id).map_err(|_| denied())?;
    let grant: IssuedGrant = serde_json::from_str(&value).map_err(|_| denied())?;
    if grant.version != 1 {
        return Err(denied());
    }
    Ok(grant)
}

/// Registered by the supervised native peer; blocking store work is off the
/// transport executor. The peer pool has already validated its device proof.
pub(super) fn handle_transfer_grant_request(
    root: &Path,
    token: &str,
    params: Vec<Value>,
) -> Result<Value, String> {
    if params.len() != 1 || serde_json::to_vec(&params).map_or(true, |v| v.len() > 4096) {
        return Err(denied());
    }
    let request: TransferGrantRequest =
        serde_json::from_value(params.into_iter().next().unwrap()).map_err(|_| denied())?;
    // Serialize compound issuance/revocation. No transport await while held;
    // secret tuples themselves are encrypted and transactionally durable.
    let _guard = secrets::credential_lifecycle_guard();
    let actor = principal(root, token)?;
    match request {
        TransferGrantRequest::Issue { scope } => {
            let generation = current_content(root, token, &scope)?;
            let mut random = [0u8; 32];
            SystemRandom::new()
                .fill(&mut random)
                .map_err(|_| denied())?;
            let id = random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            let expires_at_ms = chrono::Utc::now()
                .timestamp_millis()
                .checked_add(GRANT_TTL_MS)
                .ok_or_else(denied)?;
            let grant = IssuedGrant {
                version: 1,
                scope: scope.clone(),
                principal: actor.clone(),
                content_generation: generation,
                expires_at_ms,
                revoked: false,
            };
            // Current actor/file policy rechecked before committing; an epoch
            // change is never silently replaced by the new actor on resume.
            if principal(root, token)? != actor
                || current_content(root, token, &scope)? != grant.content_generation
            {
                return Err(denied());
            }
            save(root, &id, &grant)?;
            serde_json::to_value(TransferGrantReply {
                grant_id: id,
                scope,
                expires_at_ms,
            })
            .map_err(|_| denied())
        }
        TransferGrantRequest::Check { grant_id, scope } => {
            let grant = load(root, &grant_id)?;
            if grant.revoked
                || grant.expires_at_ms <= chrono::Utc::now().timestamp_millis()
                || grant.scope != scope
                || grant.principal != actor
                || current_content(root, token, &scope)? != grant.content_generation
                || principal(root, token)? != actor
            {
                return Err(denied());
            }
            Ok(json!({"authorized":true}))
        }
        TransferGrantRequest::Revoke { grant_id } => {
            let mut grant = load(root, &grant_id)?;
            if grant.principal != actor {
                return Err(denied());
            }
            grant.revoked = true;
            save(root, &grant_id, &grant)?;
            Ok(json!({"revoked":true}))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use ctox_sync::authority::auth::SigningIdentity;
    use ring::signature::KeyPair as _;
    struct BoundRecipient {
        token: String,
        protocol: Value,
        nonce: String,
        pairing_id: String,
    }
    fn bound_recipient(root: &Path, label: &str) -> BoundRecipient {
        let rng = SystemRandom::new();
        let pkcs8 = ring::signature::EcdsaKeyPair::generate_pkcs8(
            &ring::signature::ECDSA_P256_SHA256_FIXED_SIGNING,
            &rng,
        )
        .unwrap();
        let key = ring::signature::EcdsaKeyPair::from_pkcs8(
            &ring::signature::ECDSA_P256_SHA256_FIXED_SIGNING,
            pkcs8.as_ref(),
            &rng,
        )
        .unwrap();
        let public = key.public_key().as_ref();
        let encoder = &base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let jwk = json!({"kty":"EC","crv":"P-256","x":encoder.encode(&public[1..33]),"y":encoder.encode(&public[33..65])});
        let (_, thumbprint) =
            super::super::rxdb_peer::p256_public_key_and_thumbprint(&jwk).unwrap();
        let pairing_id = format!("pairing-{label}");
        let binding = super::super::mobile_invites::device_binding(
            Some(&pairing_id),
            Some(&format!("device-{label}")),
            Some(&thumbprint),
        )
        .unwrap()
        .unwrap();
        let created =
            super::super::mobile_invites::create(root, 300, Some(label), Some(&binding)).unwrap();
        let user = created["invite"]["session"]["user"]["id"].as_str().unwrap();
        // Give this isolated enrolled test device an existing native role; no
        // caller-injected role or authority fields enter the transfer request.
        let (token, _) = store::issue_business_os_capability_token_for_managed_user_with_binding(
            root,
            user,
            label,
            "chef",
            chrono::Utc::now().timestamp_millis(),
            Some(&binding),
        )
        .unwrap();
        let nonce = "n".repeat(43);
        let signature = key.sign(&rng, nonce.as_bytes()).unwrap();
        let protocol = json!({"peerSession":{"sessionId":format!("native-test-{label}"),"capabilityToken":token,"deviceProof":{"version":"ctox-device-proof-v1","nonce":nonce,"publicJwk":jwk,"signature":encoder.encode(signature.as_ref())}}});
        assert_eq!(
            super::super::rxdb_peer::validate_device_bound_peer_session(
                root,
                &protocol,
                Some(&nonce)
            ),
            rxdb::plugins::replication_webrtc::WebRTCPeerSessionValidation::Accept
        );
        BoundRecipient {
            token,
            protocol,
            nonce,
            pairing_id,
        }
    }
    #[test]
    fn missing_recipient_binding_cannot_issue_check_or_revoke() {
        let (root, bound, scope) = fixture();
        let grant = issue(root.path(), &bound, &scope);
        let (unbound, _) = store::issue_business_os_capability_token_for_managed_user(
            root.path(),
            "unbound",
            "Unbound",
            "chef",
            chrono::Utc::now().timestamp_millis(),
        )
        .unwrap();
        assert!(call(
            root.path(),
            &unbound,
            TransferGrantRequest::Issue {
                scope: scope.clone()
            }
        )
        .is_err());
        assert!(check(root.path(), &unbound, &scope, &grant.grant_id).is_err());
        assert!(call(
            root.path(),
            &unbound,
            TransferGrantRequest::Revoke {
                grant_id: grant.grant_id
            }
        )
        .is_err());
    }
    #[test]
    fn recipient_uses_real_nonce_proof_and_changed_or_revoked_proof_is_denied() {
        use super::super::rxdb_peer::validate_device_bound_peer_session as validate;
        use rxdb::plugins::replication_webrtc::WebRTCPeerSessionValidation::{Accept, Reject};
        let (root, _, scope) = fixture();
        let recipient = bound_recipient(root.path(), "proof-recipient");
        assert_eq!(
            validate(root.path(), &recipient.protocol, Some(&recipient.nonce)),
            Accept
        );
        let grant = issue(root.path(), &recipient.token, &scope);
        let mut missing = recipient.protocol.clone();
        missing["peerSession"]
            .as_object_mut()
            .unwrap()
            .remove("deviceProof");
        assert_eq!(
            validate(root.path(), &missing, Some(&recipient.nonce)),
            Reject
        );
        assert_eq!(
            validate(root.path(), &recipient.protocol, Some(&"x".repeat(43))),
            Reject
        );
        let mut changed = recipient.protocol.clone();
        changed["peerSession"]["deviceProof"]["signature"] =
            json!(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([0u8; 64]));
        assert_eq!(
            validate(root.path(), &changed, Some(&recipient.nonce)),
            Reject
        );
        let other = bound_recipient(root.path(), "different-key");
        changed["peerSession"]["deviceProof"] =
            other.protocol["peerSession"]["deviceProof"].clone();
        assert_eq!(
            validate(root.path(), &changed, Some(&recipient.nonce)),
            Reject
        );
        super::super::mobile_invites::revoke_by_device_pairing_id(
            root.path(),
            &recipient.pairing_id,
        )
        .unwrap();
        assert_eq!(
            validate(root.path(), &recipient.protocol, Some(&recipient.nonce)),
            Reject
        );
        assert!(check(root.path(), &recipient.token, &scope, &grant.grant_id).is_err());
        assert!(call(
            root.path(),
            &recipient.token,
            TransferGrantRequest::Issue { scope }
        )
        .is_err());
        assert!(call(
            root.path(),
            &recipient.token,
            TransferGrantRequest::Revoke {
                grant_id: grant.grant_id
            }
        )
        .is_err());
    }
    fn fixture() -> (tempfile::TempDir, String, TransferGrantScope) {
        let root = tempfile::tempdir().unwrap();
        let key_bytes = SigningIdentity::generate_pkcs8().unwrap();
        let key = SigningIdentity::from_pkcs8(&key_bytes).unwrap();
        secrets::write_secret_record(root.path(), "ctox-sync-host", "identity-pkcs8",
            &json!({"identity":key.public_identity(),"pkcs8":base64::engine::general_purpose::STANDARD.encode(key_bytes)}).to_string(),None,json!({})).unwrap();
        let token = bound_recipient(root.path(), "operator").token;
        let config = store::sync_connection_config(root.path()).unwrap();
        let scope = TransferGrantScope {
            transfer_id: "download_1".into(),
            source_instance_id: config.instance_id,
            source_public_key: key.public_identity(),
            collection: "desktop_files".into(),
            file_id: "file_1".into(),
            sha256: "a".repeat(64),
            size: 3,
        };
        let path = store::rxdb_store_path(root.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute_batch("CREATE TABLE ctox_business_os__desktop_files__v0 (id TEXT PRIMARY KEY,data TEXT NOT NULL,deleted INTEGER NOT NULL DEFAULT 0)").unwrap();
        conn.execute("INSERT INTO ctox_business_os__desktop_files__v0(id,data) VALUES('file_1',?1)", [json!({"id":"file_1","content_state":"available","content_generation_id":"generation_1","content_hash_scheme":"sha256-bytes-v1","content_hash":scope.sha256,"size_bytes":3}).to_string()]).unwrap();
        (root, token, scope)
    }
    fn call(root: &Path, token: &str, request: TransferGrantRequest) -> Result<Value, String> {
        handle_transfer_grant_request(root, token, vec![serde_json::to_value(request).unwrap()])
    }
    fn issue(root: &Path, token: &str, scope: &TransferGrantScope) -> TransferGrantReply {
        serde_json::from_value(
            call(
                root,
                token,
                TransferGrantRequest::Issue {
                    scope: scope.clone(),
                },
            )
            .unwrap(),
        )
        .unwrap()
    }
    fn check(
        root: &Path,
        token: &str,
        scope: &TransferGrantScope,
        id: &str,
    ) -> Result<Value, String> {
        call(
            root,
            token,
            TransferGrantRequest::Check {
                grant_id: id.into(),
                scope: scope.clone(),
            },
        )
    }
    #[test]
    fn grant_is_durable_and_revocation_survives_reopen() {
        let (root, token, scope) = fixture();
        let reply = issue(root.path(), &token, &scope);
        assert_eq!(
            check(root.path(), &token, &scope, &reply.grant_id).unwrap(),
            json!({"authorized":true})
        );
        // Every call opens the real encrypted store anew; there is no UI or
        // process-local grant table. Scope/principal are not plaintext metadata.
        assert_eq!(
            secrets::list_secret_records(root.path(), Some(SECRET_SCOPE)).unwrap()[0].metadata,
            json!({"version":1})
        );
        call(
            root.path(),
            &token,
            TransferGrantRequest::Revoke {
                grant_id: reply.grant_id.clone(),
            },
        )
        .unwrap();
        assert!(check(root.path(), &token, &scope, &reply.grant_id).is_err());
        assert!(load(root.path(), &reply.grant_id).unwrap().revoked);
    }
    #[test]
    fn changed_scope_content_generation_or_deleted_file_cannot_resume() {
        let (root, token, scope) = fixture();
        let reply = issue(root.path(), &token, &scope);
        for changed in [
            TransferGrantScope {
                transfer_id: "other".into(),
                ..scope.clone()
            },
            TransferGrantScope {
                file_id: "other".into(),
                ..scope.clone()
            },
            TransferGrantScope {
                sha256: "b".repeat(64),
                ..scope.clone()
            },
            TransferGrantScope {
                size: 4,
                ..scope.clone()
            },
            TransferGrantScope {
                source_public_key: "wrong".into(),
                ..scope.clone()
            },
        ] {
            assert!(check(root.path(), &token, &changed, &reply.grant_id).is_err());
        }
        let conn = rusqlite::Connection::open(store::rxdb_store_path(root.path())).unwrap();
        conn.execute("UPDATE ctox_business_os__desktop_files__v0 SET data=json_set(data,'$.content_generation_id','generation_2') WHERE id='file_1'",[]).unwrap();
        assert!(check(root.path(), &token, &scope, &reply.grant_id).is_err());
        conn.execute(
            "UPDATE ctox_business_os__desktop_files__v0 SET deleted=1 WHERE id='file_1'",
            [],
        )
        .unwrap();
        assert!(call(root.path(), &token, TransferGrantRequest::Issue { scope }).is_err());
    }
    #[test]
    fn expired_grant_and_changed_authorization_epoch_are_terminal() {
        let (root, token, scope) = fixture();
        let reply = issue(root.path(), &token, &scope);
        let mut grant = load(root.path(), &reply.grant_id).unwrap();
        let original_user = grant.principal.user_id.clone();
        store::open_store(root.path())
            .unwrap()
            .execute(
                "UPDATE business_users SET capability_epoch=capability_epoch+1 WHERE user_id=?1",
                [&original_user],
            )
            .unwrap();
        save(root.path(), &reply.grant_id, &grant).unwrap();
        assert!(check(root.path(), &token, &scope, &reply.grant_id).is_err());
        store::open_store(root.path())
            .unwrap()
            .execute(
                "UPDATE business_users SET capability_epoch=capability_epoch-1 WHERE user_id=?1",
                [&original_user],
            )
            .unwrap();
        grant.principal = principal(root.path(), &token).unwrap();
        grant.expires_at_ms = chrono::Utc::now().timestamp_millis() - 1;
        save(root.path(), &reply.grant_id, &grant).unwrap();
        assert!(check(root.path(), &token, &scope, &reply.grant_id).is_err());
    }
    #[test]
    fn current_actor_and_policy_are_required_for_issue_check_and_revoke() {
        let (root, token, scope) = fixture();
        let reply = issue(root.path(), &token, &scope);
        let other = bound_recipient(root.path(), "other").token;
        assert!(check(root.path(), &other, &scope, &reply.grant_id).is_err());
        assert!(call(
            root.path(),
            &other,
            TransferGrantRequest::Revoke {
                grant_id: reply.grant_id.clone()
            }
        )
        .is_err());
        store::open_store(root.path())
            .unwrap()
            .execute(
                "UPDATE business_users SET active=0 WHERE user_id=?1",
                [principal(root.path(), &token).unwrap().user_id],
            )
            .unwrap();
        assert!(check(root.path(), &token, &scope, &reply.grant_id).is_err());
        assert!(call(root.path(), &token, TransferGrantRequest::Issue { scope }).is_err());
        assert!(call(
            root.path(),
            "forged",
            TransferGrantRequest::Revoke {
                grant_id: reply.grant_id
            }
        )
        .is_err());
    }
    #[test]
    fn revoked_file_read_policy_denies_a_grant_with_unchanged_account_and_device() {
        let (root, enrolled, scope) = fixture();
        let enrolled_actor = principal(root.path(), &enrolled).unwrap();
        let device = enrolled_actor.device.as_ref().unwrap();
        let binding = super::super::mobile_invites::device_binding(
            Some(&device.pairing_id),
            Some(&device.device_id),
            Some(&device.proof_key_thumbprint),
        )
        .unwrap()
        .unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        let (token, _) = store::issue_business_os_capability_token_for_managed_user_with_binding(
            root.path(),
            &enrolled_actor.user_id,
            "Restricted enrolled recipient",
            "user",
            now,
            Some(&binding),
        )
        .unwrap();
        let actor = principal(root.path(), &token).unwrap();
        let conn = store::open_store(root.path()).unwrap();
        // Remove migration grants only in this isolated fixture. The recipient
        // now has precisely one explicit file-read grant and no elevated role.
        conn.execute(
            "UPDATE business_permission_grants SET active=0
             WHERE permission=?1 AND scope_type='collection' AND scope_id='desktop_files'",
            [BusinessOsPermission::DataRead.as_str()],
        )
        .unwrap();
        assert!(call(
            root.path(),
            &token,
            TransferGrantRequest::Issue {
                scope: scope.clone()
            }
        )
        .is_err());
        conn.execute(
            "INSERT INTO business_permission_grants
             (grant_id,subject_type,subject_id,permission,scope_type,scope_id,active,created_at_ms,updated_at_ms)
             VALUES('fixture.file-read','user',?1,?2,'collection','desktop_files',1,?3,?3)",
            rusqlite::params![actor.user_id, BusinessOsPermission::DataRead.as_str(), now],
        )
        .unwrap();
        let grant = issue(root.path(), &token, &scope);
        assert_eq!(
            check(root.path(), &token, &scope, &grant.grant_id).unwrap(),
            json!({"authorized":true})
        );
        conn.execute(
            "UPDATE business_permission_grants SET active=0 WHERE grant_id='fixture.file-read'",
            [],
        )
        .unwrap();
        // This is a policy-only invalidation, not an expired/replaced token,
        // account epoch change or revoked device masking the source check.
        assert_eq!(principal(root.path(), &token).unwrap(), actor);
        assert!(check(root.path(), &token, &scope, &grant.grant_id).is_err());
        assert!(call(root.path(), &token, TransferGrantRequest::Issue { scope }).is_err());
        // The original recipient can still explicitly revoke its denied grant.
        call(
            root.path(),
            &token,
            TransferGrantRequest::Revoke {
                grant_id: grant.grant_id.clone(),
            },
        )
        .unwrap();
        assert!(load(root.path(), &grant.grant_id).unwrap().revoked);
    }
    #[test]
    fn caller_identity_injection_unsupported_source_and_bad_content_are_denied() {
        let (root, token, scope) = fixture();
        let mut forged = serde_json::to_value(TransferGrantRequest::Issue {
            scope: scope.clone(),
        })
        .unwrap();
        forged["principal"] = json!({"userId":"other"});
        assert!(handle_transfer_grant_request(root.path(), &token, vec![forged]).is_err());
        for bad in [
            TransferGrantScope {
                collection: "business_users".into(),
                ..scope.clone()
            },
            TransferGrantScope {
                source_instance_id: "wrong".into(),
                ..scope.clone()
            },
            TransferGrantScope {
                sha256: "b".repeat(64),
                ..scope.clone()
            },
        ] {
            assert!(call(
                root.path(),
                &token,
                TransferGrantRequest::Issue { scope: bad }
            )
            .is_err());
        }
        assert!(handle_transfer_grant_request(root.path(), &token, vec![]).is_err());
        assert!(handle_transfer_grant_request(
            root.path(),
            &token,
            vec![json!({"action":"revoke","grantId":"x".repeat(4096)})]
        )
        .is_err());
    }
}
