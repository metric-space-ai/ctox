//! BusinessData source/principal identity on the existing RxDB/WebRTC channel.
//! A source proof can be requested without credentials; a principal is returned
//! only for a currently valid native capability. Neither result grants data access.
use super::store;
use ctox_sync::business_data_contract::{
    NativeBusinessDataDeviceIdentity, NativeBusinessDataIdentityRequest,
    NativeBusinessDataPrincipal, CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
};
use serde_json::Value;
use std::path::Path;

pub(super) fn identity_response(
    root: &Path,
    capability_token: &str,
    params: Vec<Value>,
) -> Result<Value, String> {
    if params.len() != 1 || serde_json::to_vec(&params).map_or(true, |bytes| bytes.len() > 4096) {
        return Err("invalid BusinessData identity request".into());
    }
    let request: NativeBusinessDataIdentityRequest =
        serde_json::from_value(params.into_iter().next().unwrap())
            .map_err(|_| "invalid BusinessData identity request".to_string())?;
    if request.version != CTOX_BUSINESS_DATA_PROTOCOL_VERSION
        || request.challenge.len() != 64
        || !request
            .challenge
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("invalid BusinessData identity challenge".into());
    }
    let principal = if capability_token.is_empty() {
        None
    } else {
        let claims = store::verified_webrtc_capability_claims(root, capability_token)
            .ok_or_else(|| "BusinessData capability is invalid or revoked".to_string())?;
        Some(NativeBusinessDataPrincipal {
            user_id: claims.user_id,
            authorization_epoch: u64::try_from(claims.actor_epoch)
                .map_err(|_| "BusinessData authorization epoch is invalid".to_string())?,
            device: claims
                .device_binding
                .map(|binding| NativeBusinessDataDeviceIdentity {
                    pairing_id: binding.device_pairing_id,
                    device_id: binding.device_id,
                    proof_key_thumbprint: binding.proof_key_thumbprint,
                }),
        })
    };
    // No key creation, fallback key or self-signed TOFU enrollment in a read.
    let key = crate::sync_host::signing_identity(root)
        .map_err(|_| "native BusinessData source identity is not provisioned".to_string())?;
    let instance_id = store::sync_connection_config(root)
        .map_err(|_| "native BusinessData instance identity is unavailable".to_string())?
        .instance_id;
    let reply = key
        .attest_business_data_identity(&instance_id, &request.challenge, principal)
        .map_err(|_| "native BusinessData identity could not be attested".to_string())?;
    serde_json::to_value(reply)
        .map_err(|_| "native BusinessData identity encoding failed".to_string())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use base64::Engine;
    use ctox_sync::{
        authority::auth::SigningIdentity,
        business_data_identity::{fresh_challenge, verify_peer_identity},
    };
    use serde_json::json;

    fn provision(root: &Path) -> String {
        let pkcs8 = SigningIdentity::generate_pkcs8().unwrap();
        let key = SigningIdentity::from_pkcs8(&pkcs8).unwrap();
        crate::secrets::write_secret_record(root, "ctox-sync-host", "identity-pkcs8",
            &json!({"identity": key.public_identity(), "pkcs8": base64::engine::general_purpose::STANDARD.encode(pkcs8)}).to_string(),
            None, json!({"source":"test"})).unwrap();
        key.public_identity()
    }

    #[test]
    fn source_preflight_has_no_principal_and_uses_existing_instance_and_key() {
        let root = tempfile::tempdir().unwrap();
        let challenge = fresh_challenge().unwrap();
        let request = json!({"version":1,"challenge":challenge});
        assert!(identity_response(root.path(), "", vec![request.clone()]).is_err());
        assert!(
            !crate::secrets::secret_exists(root.path(), "ctox-sync-host", "identity-pkcs8")
                .unwrap()
        );
        let pin = provision(root.path());
        let instance = store::sync_connection_config(root.path())
            .unwrap()
            .instance_id;
        let response = identity_response(root.path(), "", vec![request]).unwrap();
        let proof = serde_json::from_value(response.clone()).unwrap();
        verify_peer_identity(&proof, &pin, &instance, &challenge).unwrap();
        assert!(response["principal"].is_null());
        assert_eq!(
            crate::sync_host::signing_identity(root.path())
                .unwrap()
                .public_identity(),
            pin
        );
    }

    #[test]
    fn principal_comes_from_current_store_and_revoked_or_forged_tokens_fail() {
        let root = tempfile::tempdir().unwrap();
        let pin = provision(root.path());
        let (token, _) = store::issue_business_os_capability_token_for_managed_user(
            root.path(),
            "operator",
            "Operator",
            "chef",
            chrono::Utc::now().timestamp_millis(),
        )
        .unwrap();
        let challenge = fresh_challenge().unwrap();
        let request = json!({"version":1,"challenge":challenge});
        let response = identity_response(root.path(), &token, vec![request.clone()]).unwrap();
        let proof = serde_json::from_value(response.clone()).unwrap();
        let instance = store::sync_connection_config(root.path())
            .unwrap()
            .instance_id;
        verify_peer_identity(&proof, &pin, &instance, &challenge).unwrap();
        assert_eq!(response["principal"]["userId"], "operator");
        assert!(identity_response(root.path(), "forged", vec![request.clone()]).is_err());
        store::open_store(root.path())
            .unwrap()
            .execute(
                "UPDATE business_users SET active = 0 WHERE user_id = 'operator'",
                [],
            )
            .unwrap();
        assert!(identity_response(root.path(), &token, vec![request]).is_err());
    }

    #[test]
    fn caller_cannot_inject_identity_and_oversized_requests_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        for request in [
            json!({"version":1,"challenge":fresh_challenge().unwrap(),"userId":"other"}),
            json!({"version":1,"challenge":fresh_challenge().unwrap(),"instanceId":"other"}),
            json!({"version":1,"challenge":"invalid"}),
            json!({"version":2,"challenge":fresh_challenge().unwrap()}),
            json!({"version":1,"challenge":"x".repeat(4096)}),
        ] {
            assert!(identity_response(root.path(), "", vec![request]).is_err());
        }
        assert!(identity_response(root.path(), "", vec![]).is_err());
    }
}
