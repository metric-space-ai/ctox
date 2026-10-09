// Origin: CTOX
// License: AGPL-3.0-only

use super::{mobile_invites, store};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

pub(super) const WORKJET_DEVICE_WEBRTC_METHOD: &str = "ctox.workjet.device.v1";
const WORKJET_DEVICE_WEBRTC_REQUEST_MAX_BYTES: usize = 4 * 1024;

#[derive(Debug, Deserialize)]
#[serde(tag = "action", deny_unknown_fields)]
pub(crate) enum WorkjetDeviceWebRtcRequestV1 {
    #[serde(rename = "invite.create", rename_all = "camelCase")]
    InviteCreate {
        ttl_seconds: Option<i64>,
        display_name: Option<String>,
    },
    #[serde(rename = "invite.revoke", rename_all = "camelCase")]
    InviteRevoke { invite_id: String },
    #[serde(rename = "binding.list")]
    BindingList,
    #[serde(rename = "binding.revoke", rename_all = "camelCase")]
    BindingRevoke { binding_id: String },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub(crate) enum WorkjetDeviceWebRtcResponseV1 {
    InviteCreate(WorkjetDeviceInviteCreateResponseV1),
    BindingList(WorkjetDeviceBindingListResponseV1),
    Mutation(WorkjetDeviceMutationResponseV1),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkjetDeviceInviteCreateResponseV1 {
    business_os_instance_id: String,
    device_id: Option<String>,
    proof_key_thumbprint: Option<String>,
    grant_id: String,
    invite_id: String,
    invite: Value,
    expires_at: String,
    pairing_uri: String,
    qr_svg: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkjetDeviceMutationResponseV1 {
    revoked: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkjetDeviceBindingV1 {
    invite_id_hash: String,
    id: String,
    device_id: String,
    display_name: String,
    created_at_ms: i64,
    paired_at_ms: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkjetDeviceBindingListResponseV1 {
    schema: String,
    bindings: Vec<WorkjetDeviceBindingV1>,
}

fn typed_response<T>(value: Value) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(value)
        .map_err(|_| "workjet device response failed native validation".to_string())
}

fn can_manage_devices(root: &Path, user_id: &str, role: &str) -> bool {
    matches!(role, "chef" | "admin" | "founder")
        || mobile_invites::is_active_paired_device_user(root, user_id)
}

fn validate_request_object(request: &Value) -> Result<(), String> {
    let object = request
        .as_object()
        .ok_or_else(|| "workjet device request is invalid".to_string())?;
    let action = object
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| "workjet device request is invalid".to_string())?;
    let allowed_fields: &[&str] = match action {
        "invite.create" => &["action", "ttlSeconds", "displayName"],
        "invite.revoke" => &["action", "inviteId"],
        "binding.list" => &["action"],
        "binding.revoke" => &["action", "bindingId"],
        _ => return Err("workjet device request is invalid".to_string()),
    };
    if object
        .keys()
        .any(|field| !allowed_fields.contains(&field.as_str()))
    {
        return Err("workjet device request is invalid".to_string());
    }
    Ok(())
}

/// Small transient device-control surface on the already authenticated
/// RxDB/WebRTC peer. Invite secrets are returned only to this DataChannel
/// request and are never written to RxDB, HTTP, logs, or reports.
pub(super) async fn handle_workjet_device_webrtc_request(
    root: &Path,
    capability_token: &str,
    params: Vec<Value>,
) -> Result<Value, String> {
    let claims = store::verified_webrtc_capability_claims(root, capability_token)
        .ok_or_else(|| "workjet device capability is invalid".to_string())?;
    if !can_manage_devices(root, &claims.user_id, &claims.role) {
        return Err("workjet device management is not allowed".to_string());
    }
    if serde_json::to_vec(&params)
        .map(|encoded| encoded.len() > WORKJET_DEVICE_WEBRTC_REQUEST_MAX_BYTES)
        .unwrap_or(true)
    {
        return Err("workjet device request is too large".to_string());
    }
    if params.len() != 1 {
        return Err("workjet device request must contain exactly one parameter".to_string());
    }
    let request = params
        .first()
        .cloned()
        .ok_or_else(|| "workjet device request is missing".to_string())?;
    validate_request_object(&request)?;
    let request: WorkjetDeviceWebRtcRequestV1 = serde_json::from_value(request)
        .map_err(|_| "workjet device request is invalid".to_string())?;
    let response = match request {
        WorkjetDeviceWebRtcRequestV1::InviteCreate {
            ttl_seconds,
            display_name,
        } => WorkjetDeviceWebRtcResponseV1::InviteCreate(typed_response(
            mobile_invites::create_for_owner(
                root,
                ttl_seconds.unwrap_or(mobile_invites::DEFAULT_TTL_SECONDS),
                display_name.as_deref(),
                None,
                Some(&claims.user_id),
            )
            .map_err(|error| format!("workjet device invite create failed: {error}"))?,
        )?),
        WorkjetDeviceWebRtcRequestV1::InviteRevoke { invite_id } => {
            WorkjetDeviceWebRtcResponseV1::Mutation(typed_response(
                mobile_invites::revoke(root, &invite_id)
                    .map_err(|error| format!("workjet device invite revoke failed: {error}"))?,
            )?)
        }
        WorkjetDeviceWebRtcRequestV1::BindingList => {
            WorkjetDeviceWebRtcResponseV1::BindingList(typed_response(
                mobile_invites::list_device_bindings(root)
                    .map_err(|error| format!("workjet device binding list failed: {error}"))?,
            )?)
        }
        WorkjetDeviceWebRtcRequestV1::BindingRevoke { binding_id } => {
            WorkjetDeviceWebRtcResponseV1::Mutation(typed_response(
                mobile_invites::revoke_by_device_pairing_id(root, &binding_id)
                    .map_err(|error| format!("workjet device binding revoke failed: {error}"))?,
            )?)
        }
    };
    serde_json::to_value(response)
        .map_err(|_| "workjet device response serialization failed".to_string())
}

#[cfg(test)]
mod tests {
    use super::handle_workjet_device_webrtc_request;
    use tempfile::tempdir;

    #[tokio::test]
    async fn authority_can_create_list_and_revoke_without_http() -> anyhow::Result<()> {
        let root = tempdir()?;
        let now = chrono::Utc::now().timestamp_millis();
        let (token, _) = super::store::issue_business_os_capability_token_for_managed_user(
            root.path(),
            "operator",
            "Operator",
            "chef",
            now,
        )?;
        let created = handle_workjet_device_webrtc_request(
            root.path(),
            &token,
            vec![serde_json::json!({
                "action": "invite.create",
                "ttlSeconds": 300,
                "displayName": "Fold 8",
            })],
        )
        .await
        .map_err(anyhow::Error::msg)?;
        assert_eq!(created["invite"]["data_plane"], "rxdb-webrtc");
        assert_eq!(created["invite"]["http_bridge_available"], false);
        assert!(created["pairingUri"]
            .as_str()
            .is_some_and(|value| value.starts_with("workjet://pair?payload=")));
        assert!(created["qrSvg"]
            .as_str()
            .is_some_and(|value| value.contains("<svg")));

        let listed = handle_workjet_device_webrtc_request(
            root.path(),
            &token,
            vec![serde_json::json!({ "action": "binding.list" })],
        )
        .await
        .map_err(anyhow::Error::msg)?;
        assert_eq!(listed["schema"], "ctox.workjet-device-bindings.v1");
        assert_eq!(listed["bindings"].as_array().map(Vec::len), Some(0));

        let revoked = handle_workjet_device_webrtc_request(
            root.path(),
            &token,
            vec![serde_json::json!({
                "action": "invite.revoke",
                "inviteId": created["inviteId"],
            })],
        )
        .await
        .map_err(anyhow::Error::msg)?;
        assert_eq!(revoked, serde_json::json!({ "revoked": true }));
        Ok(())
    }

    #[tokio::test]
    async fn ordinary_unpaired_user_cannot_manage_devices() -> anyhow::Result<()> {
        let root = tempdir()?;
        let now = chrono::Utc::now().timestamp_millis();
        let (token, _) = super::store::issue_business_os_capability_token_for_managed_user(
            root.path(),
            "ordinary-user",
            "Ordinary User",
            "user",
            now,
        )?;
        let error = handle_workjet_device_webrtc_request(
            root.path(),
            &token,
            vec![serde_json::json!({ "action": "binding.list" })],
        )
        .await
        .expect_err("ordinary user must be denied");
        assert!(error.contains("not allowed"));
        Ok(())
    }

    #[tokio::test]
    async fn request_contract_rejects_unknown_fields() -> anyhow::Result<()> {
        let root = tempdir()?;
        let now = chrono::Utc::now().timestamp_millis();
        let (token, _) = super::store::issue_business_os_capability_token_for_managed_user(
            root.path(),
            "operator",
            "Operator",
            "chef",
            now,
        )?;
        for request in [
            serde_json::json!({
                "action": "binding.list",
                "environmentId": "forbidden-http-stack-scope",
            }),
            serde_json::json!({
                "action": "invite.create",
                "ttlSeconds": 300,
                "connectionUrl": "https://forbidden.example",
            }),
            serde_json::json!({
                "action": "invite.revoke",
                "inviteId": "invite_opaque",
                "relayToken": "forbidden",
            }),
            serde_json::json!({
                "action": "binding.revoke",
                "bindingId": "binding_opaque",
                "environmentId": "forbidden-http-stack-scope",
            }),
        ] {
            let error = handle_workjet_device_webrtc_request(root.path(), &token, vec![request])
                .await
                .expect_err("unknown fields must fail closed");
            assert_eq!(error, "workjet device request is invalid");
        }

        let error = handle_workjet_device_webrtc_request(
            root.path(),
            &token,
            vec![
                serde_json::json!({ "action": "binding.list" }),
                serde_json::json!({ "action": "binding.list" }),
            ],
        )
        .await
        .expect_err("more than one parameter must fail closed");
        assert_eq!(
            error,
            "workjet device request must contain exactly one parameter"
        );
        Ok(())
    }
}
