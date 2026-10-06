//! Authenticated source enrollment/renewal descriptor on the existing native
//! auxiliary channel. Dispatcher admission includes nonce-bound P-256 proof.
use super::{rxdb_peer_transfer_grants::principal, store};
use crate::native_transfer_routing::{
    NativeTransferIceServer, NativeTransferProvisionReply, NativeTransferProvisionRequest,
    NativeTransferRouting,
};
use serde_json::Value;
use std::path::Path;

fn denied() -> String {
    "native transfer account unavailable or unauthorized".into()
}

#[cfg(test)]
pub(super) fn handle(root: &Path, token: &str, params: Vec<Value>) -> Result<Value, String> {
    prepare(root, token, params).map(|reply| reply.result)
}

pub(super) fn prepare(
    root: &Path,
    token: &str,
    params: Vec<Value>,
) -> Result<rxdb::plugins::replication_webrtc::index_mod::GuardedAuxiliaryResponse, String> {
    if params.len() != 1 || serde_json::to_vec(&params).map_or(true, |v| v.len() > 4096) {
        return Err(denied());
    }
    let request: NativeTransferProvisionRequest =
        serde_json::from_value(params.into_iter().next().unwrap()).map_err(|_| denied())?;
    let actor = principal(root, token)?;
    let identity = crate::sync_host::signing_identity(root)
        .map_err(|_| denied())?
        .public_identity();
    let config = store::mobile_invite_sync_config(root).map_err(|_| denied())?;
    if request.source_public_identity != identity
        || request.source_instance_id != config.instance_id
    {
        return Err(denied());
    }
    let device = actor.device.as_ref().ok_or_else(denied)?;
    let (ice, actual_ice_expiry) = store::native_transfer_ice_config(root, &device.device_id);
    let ice_servers = ice
        .iter()
        .map(|server| {
            let urls = server.get("urls").ok_or_else(denied)?;
            let urls = if let Some(url) = urls.as_str() {
                vec![url.to_owned()]
            } else {
                serde_json::from_value::<Vec<String>>(urls.clone()).map_err(|_| denied())?
            };
            Ok(NativeTransferIceServer {
                urls,
                username: server
                    .get("username")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
                credential: server
                    .get("credential")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let now = chrono::Utc::now().timestamp_millis();
    let expires = actual_ice_expiry
        .unwrap_or(i64::MAX)
        .min(now.saturating_add(30 * 60 * 1000));
    if expires <= now.saturating_add(60_000) {
        return Err(denied());
    }
    let routing = NativeTransferRouting {
        room: config.sync_room,
        signaling_urls: config.signaling_urls,
        browser_token: config.signaling_browser_token,
        browser_token_hash: config.signaling_browser_token_hash,
        native_token_hash: config.signaling_native_token_hash,
        auth_version: config.signaling_auth_version.into(),
        ice_servers,
        refreshed_at_ms: now,
        refresh_after_ms: now + (expires - now) / 2,
        expires_at_ms: expires,
    };
    routing
        .validate(&config.instance_id, now)
        .map_err(|_| denied())?;
    let (capability_token, capability_expires_at_ms) =
        store::renew_native_transfer_capability(root, token).map_err(|_| denied())?;
    // Actor/device revocation during config/key work rejects the whole reply.
    if principal(root, token)? != actor || principal(root, &capability_token)? != actor {
        return Err(denied());
    }
    let authority = super::rxdb_peer_transfer_publication::TransferPublication {
        root: root.to_path_buf(),
        original_token: token.to_owned(),
        principal: actor.clone(),
        source_public_identity: identity.clone(),
        source_instance_id: config.instance_id.clone(),
    };
    let reply = NativeTransferProvisionReply {
        version: 1,
        source_public_identity: identity,
        source_instance_id: config.instance_id,
        principal: actor,
        capability_token,
        capability_expires_at_ms,
        routing,
    };
    Ok(
        rxdb::plugins::replication_webrtc::index_mod::GuardedAuxiliaryResponse {
            result: serde_json::to_value(&reply).map_err(|_| denied())?,
            publication: std::sync::Arc::new(AccountPublication { authority, reply }),
        },
    )
}

struct AccountPublication {
    authority: super::rxdb_peer_transfer_publication::TransferPublication,
    reply: NativeTransferProvisionReply,
}

impl rxdb::plugins::replication_webrtc::WebRTCPublicationGuard for AccountPublication {
    fn with_current(
        &self,
        publish: &mut dyn FnMut() -> rxdb::rx_error::RxResult<()>,
    ) -> rxdb::rx_error::RxResult<()> {
        use anyhow::{ensure, Context};
        self.authority
            .with_current(
                &store::native_transfer_routing_secret_keys(),
                |policy, signer, values, at_ms| {
                    let issued = super::capability::verify_capability_token(
                        signer,
                        &self.reply.capability_token,
                        at_ms,
                    )
                    .context("renewed capability issuer or expiry changed")?;
                    ensure!(
                        issued.expires_at_ms == self.reply.capability_expires_at_ms,
                        "prepared capability expiry changed"
                    );
                    let current = store::verified_webrtc_capability_claims_from_connection(
                        policy,
                        &self.reply.capability_token,
                        signer,
                        at_ms,
                    )
                    .context("renewed native capability revoked")?;
                    ensure!(
                        super::rxdb_peer_transfer_grants::principal_from_claims(current)
                            .map_err(|_| anyhow::anyhow!("renewed device unavailable"))?
                            == self.authority.principal,
                        "renewed principal changed"
                    );
                    self.reply
                        .routing
                        .validate(&self.authority.source_instance_id, at_ms)?;
                    store::validate_native_transfer_routing_publication(
                        &self.reply.routing,
                        &self.authority.source_instance_id,
                        values,
                    )?;
                    Ok(publish())
                },
            )
            .map_err(|_| super::rxdb_peer_transfer_publication::denied())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_data_device::{NativeDeviceKeyScope, NativeDeviceProofKey};
    use serde_json::json;
    fn fixture() -> (
        tempfile::TempDir,
        String,
        NativeTransferProvisionRequest,
        String,
    ) {
        let root = tempfile::tempdir().unwrap();
        crate::sync_host::handle_command(root.path(), &["init".into()]).unwrap();
        let request = NativeTransferProvisionRequest {
            source_public_identity: crate::sync_host::signing_identity(root.path())
                .unwrap()
                .public_identity(),
            source_instance_id: store::sync_connection_config(root.path())
                .unwrap()
                .instance_id,
        };
        let key = NativeDeviceProofKey::prepare(
            root.path(),
            &NativeDeviceKeyScope {
                target_id: "source".into(),
                source_instance_id: request.source_instance_id.clone(),
                source_public_identity: request.source_public_identity.clone(),
                account_epoch: 1,
            },
        )
        .unwrap();
        let device = key.device_identity();
        let binding = super::super::mobile_invites::device_binding(
            Some(&device.pairing_id),
            Some(&device.device_id),
            Some(&device.proof_key_thumbprint),
        )
        .unwrap()
        .unwrap();
        let created =
            super::super::mobile_invites::create(root.path(), 300, None, Some(&binding)).unwrap();
        let token = created["invite"]["session"]["capability_token"]
            .as_str()
            .unwrap()
            .to_owned();
        (root, token, request, device.pairing_id)
    }
    #[tokio::test]
    async fn prepared_account_service_response_rechecks_authority_after_zero_byte_pending() {
        use super::super::rxdb_peer_transfer_publication_tests::PendingReply;
        use crate::native_transfer_routing::NATIVE_TRANSFER_PROVISION_METHOD;
        for change in [
            "current",
            "device-revoked",
            "user-disabled",
            "epoch-changed",
            "issuer-rotated",
            "source-rotated",
            "room-rotated",
            "native-token-rotated",
            "connection-replaced",
        ] {
            let (root, token, request, pairing) = fixture();
            let actor = principal(root.path(), &token).unwrap();
            let pending = PendingReply::start(
                root.path(),
                &token,
                "source",
                NATIVE_TRANSFER_PROVISION_METHOD,
                serde_json::to_value(request).unwrap(),
            )
            .await;
            let reply: NativeTransferProvisionReply =
                serde_json::from_value(pending.result.clone()).unwrap();
            assert_eq!(reply.principal, actor);
            assert!(!reply.capability_token.is_empty());
            match change {
                "current" => {}
                "device-revoked" => {
                    super::super::mobile_invites::revoke_by_device_pairing_id(
                        root.path(),
                        &pairing,
                    )
                    .unwrap();
                }
                "user-disabled" | "epoch-changed" => {
                    let sql = if change == "user-disabled" {
                        "UPDATE business_users SET active=0 WHERE user_id=?1"
                    } else {
                        "UPDATE business_users SET capability_epoch=capability_epoch+1 WHERE user_id=?1"
                    };
                    assert_eq!(
                        store::open_store(root.path())
                            .unwrap()
                            .execute(sql, [&actor.user_id])
                            .unwrap(),
                        1
                    );
                }
                "issuer-rotated" | "room-rotated" | "native-token-rotated" => {
                    let (scope, name) = match change {
                        "issuer-rotated" => ("credentials", "BUSINESS_OS_CAPABILITY_SECRET"),
                        "room-rotated" => ("business-os", "webrtc_room_password"),
                        _ => ("business-os", "webrtc_native_signaling_token"),
                    };
                    crate::secrets::write_secret_record(
                        root.path(),
                        scope,
                        name,
                        &"b".repeat(64),
                        None,
                        json!({}),
                    )
                    .unwrap();
                }
                "source-rotated" => {
                    use base64::Engine;
                    use ctox_sync::authority::auth::SigningIdentity;
                    let pkcs8 = SigningIdentity::generate_pkcs8().unwrap();
                    let key = SigningIdentity::from_pkcs8(&pkcs8).unwrap();
                    crate::secrets::write_secret_record(
                        root.path(),
                        "ctox-sync-host",
                        "identity-pkcs8",
                        &json!({"identity":key.public_identity(),
                            "pkcs8":base64::engine::general_purpose::STANDARD.encode(pkcs8)})
                        .to_string(),
                        None,
                        json!({}),
                    )
                    .unwrap();
                }
                "connection-replaced" => pending.replace_connection(),
                _ => unreachable!(),
            }
            pending.finish(change == "current").await;
        }
    }

    #[test]
    fn confirmed_source_renews_only_the_original_enrolled_principal() {
        let (root, token, request, _) = fixture();
        let before = principal(root.path(), &token).unwrap();
        let reply: NativeTransferProvisionReply = serde_json::from_value(
            handle(
                root.path(),
                &token,
                vec![serde_json::to_value(&request).unwrap()],
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(reply.principal, before);
        assert_eq!(
            principal(root.path(), &reply.capability_token).unwrap(),
            before
        );
        assert_eq!(reply.source_public_identity, request.source_public_identity);
        reply
            .routing
            .validate(
                &request.source_instance_id,
                chrono::Utc::now().timestamp_millis(),
            )
            .unwrap();
        assert!(reply.routing.browser_token_hash != reply.routing.native_token_hash);
    }
    #[test]
    fn revoked_device_foreign_pin_and_caller_principal_are_rejected() {
        let (root, token, mut request, pairing) = fixture();
        let original = serde_json::to_value(&request).unwrap();
        let mut extra = serde_json::to_value(&request).unwrap();
        extra["principal"] = json!({"userId":"caller"});
        assert!(handle(root.path(), &token, vec![extra]).is_err());
        request.source_public_identity = format!("ed25519:{}", "a".repeat(64));
        assert!(handle(
            root.path(),
            &token,
            vec![serde_json::to_value(&request).unwrap()]
        )
        .is_err());
        super::super::mobile_invites::revoke_by_device_pairing_id(root.path(), &pairing).unwrap();
        assert!(handle(root.path(), &token, vec![original]).is_err());
    }
}
