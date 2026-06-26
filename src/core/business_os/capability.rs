// Origin: CTOX
// License: AGPL-3.0-only
//
// Capability tokens for Business OS command authorization.
//
// Today native command authorization derives the actor from
// `client_context.actor` inside the replicated command document — a value
// asserted by the browser (see the SECURITY note on
// `store::rxdb_session_from_command`). A capability token closes that hole: the
// native side issues a short-lived, HMAC-signed token binding a user id to a
// role; the browser carries it on each command; the native verifies the
// signature and reads the role FROM THE TOKEN, never from the unsigned claim.
//
// This module is the pure cryptographic core (issue + verify). It takes the
// signing secret as a parameter and does no I/O, so it is unit-testable in
// isolation. Secret provisioning, the runtime enforcement flag, and the wiring
// into the command session live in `store.rs`.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ring::hmac;
use serde_json::Value;

/// The verified contents of a capability token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityClaims {
    pub user_id: String,
    pub role: String,
    pub issued_at_ms: i64,
    pub expires_at_ms: i64,
}

/// Issue an HMAC-SHA256 capability token of the form
/// `base64url(payload).base64url(sig)` binding `user_id` + `role` to a validity
/// window. Only a holder of `secret` (the native instance) can mint one.
pub fn issue_capability_token(
    secret: &[u8],
    user_id: &str,
    role: &str,
    issued_at_ms: i64,
    expires_at_ms: i64,
) -> String {
    let payload = serde_json::json!({
        "uid": user_id,
        "role": role,
        "iat": issued_at_ms,
        "exp": expires_at_ms,
    });
    let payload_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap_or_default());
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    let sig = hmac::sign(&key, payload_b64.as_bytes());
    let sig_b64 = URL_SAFE_NO_PAD.encode(sig.as_ref());
    format!("{payload_b64}.{sig_b64}")
}

/// Verify a capability token against `secret` at `now_ms`. Returns the claims
/// only when the signature is valid (constant-time) and the token has not
/// expired. Any malformed / tampered / expired token returns `None`.
pub fn verify_capability_token(
    secret: &[u8],
    token: &str,
    now_ms: i64,
) -> Option<CapabilityClaims> {
    let (payload_b64, sig_b64) = token.split_once('.')?;
    let sig = URL_SAFE_NO_PAD.decode(sig_b64).ok()?;
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    // Constant-time verification (ring); a forged signature fails here.
    hmac::verify(&key, payload_b64.as_bytes(), &sig).ok()?;
    let payload: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload_b64).ok()?).ok()?;
    let expires_at_ms = payload.get("exp").and_then(Value::as_i64)?;
    if now_ms >= expires_at_ms {
        return None;
    }
    Some(CapabilityClaims {
        user_id: payload.get("uid").and_then(Value::as_str)?.to_string(),
        role: payload.get("role").and_then(Value::as_str)?.to_string(),
        issued_at_ms: payload.get("iat").and_then(Value::as_i64).unwrap_or(0),
        expires_at_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"instance-capability-secret-0123456789";
    const NOW: i64 = 1_750_000_000_000;
    const HOUR: i64 = 60 * 60 * 1000;

    #[test]
    fn issue_then_verify_round_trips() {
        let token = issue_capability_token(SECRET, "chef1", "chef", NOW, NOW + HOUR);
        let claims = verify_capability_token(SECRET, &token, NOW + 1000).expect("valid");
        assert_eq!(claims.user_id, "chef1");
        assert_eq!(claims.role, "chef");
        assert_eq!(claims.expires_at_ms, NOW + HOUR);
    }

    #[test]
    fn wrong_secret_is_rejected() {
        let token = issue_capability_token(SECRET, "chef1", "chef", NOW, NOW + HOUR);
        assert!(verify_capability_token(b"other-secret", &token, NOW).is_none());
    }

    #[test]
    fn tampered_payload_is_rejected() {
        // Forge a chef role over a token minted for a plain user — the signature
        // no longer matches the swapped payload.
        let token = issue_capability_token(SECRET, "u1", "user", NOW, NOW + HOUR);
        let sig = token.split_once('.').unwrap().1;
        let forged_payload = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&serde_json::json!({
                "uid": "u1", "role": "chef", "iat": NOW, "exp": NOW + HOUR
            }))
            .unwrap(),
        );
        let forged = format!("{forged_payload}.{sig}");
        assert!(verify_capability_token(SECRET, &forged, NOW).is_none());
    }

    #[test]
    fn expired_token_is_rejected() {
        let token = issue_capability_token(SECRET, "chef1", "chef", NOW - 2 * HOUR, NOW - HOUR);
        assert!(verify_capability_token(SECRET, &token, NOW).is_none());
    }

    #[test]
    fn garbage_token_is_rejected() {
        assert!(verify_capability_token(SECRET, "not-a-token", NOW).is_none());
        assert!(verify_capability_token(SECRET, "a.b.c", NOW).is_none());
        assert!(verify_capability_token(SECRET, "", NOW).is_none());
    }
}
