//! Host-owned credentials for the existing ctoxProtocol device-proof exchange.
//! No token/key cache, remote authorization decision or new wire protocol.
use super::webrtc_types::WebRTCConnectionHandler;
use crate::rx_error::{new_rx_error, RxError};
use futures::future::BoxFuture;
use serde_json::{json, Value};
use std::sync::Arc;

/// Public proof material only. The private key stays in the host's key store.
pub struct LocalDeviceProof {
    pub public_x: String,
    pub public_y: String,
    pub signature: String,
}

/// Obtain the token and matching signer from one current host identity.
/// Deliberately not Debug/Serialize: tokens must not enter diagnostics or disk.
pub struct LocalSessionCredentials {
    pub capability_token: String,
    pub device_proof: Option<LocalDeviceProof>,
}

/// Called per handshake, not per collection. The connection is a local lifetime
/// handle, not a verified remote identity. The provider must authorize it before
/// disclosing a token or signing its nonce, and fail after credential revocation.
pub type LocalSessionProvider<P> = Arc<
    dyn Fn(P, Option<String>) -> BoxFuture<'static, Result<LocalSessionCredentials, RxError>>
        + Send
        + Sync,
>;

fn credential_error() -> RxError {
    new_rx_error(
        "RC_WEBRTC_PEER",
        Some(json!({"code":"local_session_credentials_unavailable",
            "message":"local session credentials unavailable or connection retired"})),
    )
}

fn base64url(value: &str, size: usize) -> bool {
    value.len() == size
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Attach only the existing capabilityToken/deviceProof fields. Provider errors
/// are deliberately redacted; an installed provider must never fall back to an
/// anonymous handshake on failure. The transport rechecks the captured lifetime
/// after asynchronous key-store work without holding its lifecycle lock.
pub(crate) async fn attach_local_session<H: WebRTCConnectionHandler>(
    handler: &H,
    peer: &H::Peer,
    payload: &mut Value,
    challenge: Option<&Value>,
) -> Result<(), RxError> {
    let nonce = match challenge {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) if base64url(value, 43) => Some(value.clone()),
        _ => return Err(credential_error()),
    };
    if !handler.is_peer_current(peer) {
        return Err(credential_error());
    }
    let credentials = tokio::time::timeout(
        super::webrtc_helper::REQUEST_ANSWER_TIMEOUT,
        handler.local_session_credentials(peer, nonce.clone()),
    )
    .await
    .map_err(|_| credential_error())?
    .map_err(|_| credential_error())?;
    if !handler.is_peer_current(peer) {
        return Err(credential_error());
    }
    let Some(credentials) = credentials else {
        return Ok(());
    };
    if credentials.capability_token.trim().is_empty() {
        return Err(credential_error());
    }
    let proof = match (nonce, credentials.device_proof) {
        (None, None) => None,
        (Some(nonce), Some(proof))
            if base64url(&proof.public_x, 43)
                && base64url(&proof.public_y, 43)
                && base64url(&proof.signature, 86) =>
        {
            Some(json!({"version":"ctox-device-proof-v1", "nonce":nonce,
                "publicJwk":{"kty":"EC","crv":"P-256","x":proof.public_x,"y":proof.public_y},
                "signature":proof.signature}))
        }
        _ => return Err(credential_error()),
    };
    payload["peerSession"]["capabilityToken"] = Value::String(credentials.capability_token);
    if let Some(proof) = proof {
        payload["peerSession"]["deviceProof"] = proof;
    }
    Ok(())
}
