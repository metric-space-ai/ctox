// Origin: CTOX
// License: AGPL-3.0-only

use super::{policy::BusinessOsPermission, store};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;
use zeroize::Zeroizing;

pub(super) const CREDENTIAL_REVEAL_WEBRTC_METHOD: &str = "ctox.credentials.reveal.v1";
const DENIED: &str = "credential_reveal_denied";
const INVALID: &str = "credential_reveal_invalid_request";
const UNAVAILABLE: &str = "credential_reveal_unavailable";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RevealRequest {
    name: String,
}

fn authorized(root: &Path, token: &str) -> bool {
    store::webrtc_capability_allows_workspace_permission(
        root,
        token,
        BusinessOsPermission::SecretsManage,
    ) && store::webrtc_capability_allows_collection_permission(
        root,
        token,
        "business_commands",
        BusinessOsPermission::DataRead,
    )
}

/// Explicit end-user reveal, NOT a persisted business command or MCP action.
/// The authenticated peer supplies the token, never the request body. Scope is
/// fixed to credentials. Only this transient DataChannel response carries the
/// value; no command, audit payload, projection, or log is created here.
pub(super) fn handle_credential_reveal_webrtc_request(
    root: &Path,
    peer_id: &str,
    session_id: &str,
    capability_token: &str,
    params: Vec<Value>,
    is_current: impl Fn() -> bool,
) -> Result<Value, String> {
    reveal_with_reader(
        root,
        peer_id,
        session_id,
        capability_token,
        params,
        is_current,
        |name| {
            crate::secrets::read_secret_value(root, "credentials", name)
                .map_err(|_| UNAVAILABLE.to_string())
        },
    )
}

fn reveal_with_reader(
    root: &Path,
    peer_id: &str,
    session_id: &str,
    capability_token: &str,
    params: Vec<Value>,
    is_current: impl Fn() -> bool,
    read: impl FnOnce(&str) -> Result<String, String>,
) -> Result<Value, String> {
    reveal_with_validation(
        params,
        is_current,
        || {
            matches!(store::is_business_peer_revoked(root, peer_id), Ok(false))
                && matches!(store::is_business_peer_revoked(root, session_id), Ok(false))
                && authorized(root, capability_token)
        },
        read,
    )
}

fn reveal_with_validation(
    mut params: Vec<Value>,
    is_current: impl Fn() -> bool,
    validate_store: impl Fn() -> bool,
    read: impl FnOnce(&str) -> Result<String, String>,
) -> Result<Value, String> {
    // A handshake can replace the token/session without changing the transport
    // generation while SQLite authorization reads block. Validate the captured
    // transport identity again AFTER those reads, including before release.
    let may_release = || is_current() && validate_store() && is_current();
    if !may_release() {
        return Err(DENIED.into());
    }
    if params.len() != 1 {
        return Err(INVALID.into());
    }
    let request: RevealRequest =
        serde_json::from_value(params.remove(0)).map_err(|_| INVALID.to_string())?;
    let name = &request.name;
    if name.is_empty()
        || name.len() > 64
        || !name.as_bytes()[0].is_ascii_uppercase()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(INVALID.into());
    }
    let value = Zeroizing::new(read(name).map_err(|_| UNAVAILABLE.to_string())?);
    if value.len() > 64 * 1024 {
        return Err(UNAVAILABLE.into());
    }
    // Recheck revocation/role/epoch after the potentially blocking store read.
    if !may_release() {
        return Err(DENIED.into());
    }
    Ok(json!({
        "schema": "ctox.credential-reveal.v1",
        "name": name,
        "value": value.as_str(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CANARY: &str = "synthetic-reveal-canary-9JxZ-not-a-real-password";

    #[test]
    fn credential_reveal_discards_value_when_identity_changes_during_store_validation(
    ) -> anyhow::Result<()> {
        for replace_token in [true, false] {
            let root = tempfile::tempdir()?;
            let token = token(root.path(), "chef")?;
            let session = "fixture-session";
            let identity = std::cell::RefCell::new((token.clone(), session.to_string()));
            let validations = std::cell::Cell::new(0);
            let reads = std::cell::Cell::new(0);
            crate::secrets::write_secret_record(
                root.path(),
                "credentials",
                "TEST_LOGIN",
                CANARY,
                None,
                json!({}),
            )?;
            let result = reveal_with_validation(
                vec![json!({"name":"TEST_LOGIN"})],
                || {
                    let current = identity.borrow();
                    current.0 == token && current.1 == session
                },
                || {
                    validations.set(validations.get() + 1);
                    let allowed = matches!(
                        store::is_business_peer_revoked(root.path(), "fixture-peer"),
                        Ok(false)
                    ) && matches!(
                        store::is_business_peer_revoked(root.path(), session),
                        Ok(false)
                    ) && authorized(root.path(), &token);
                    assert!(allowed, "captured actor token remains authorized");
                    if validations.get() == 2 {
                        // Deterministic same-generation handshake interleaving:
                        // the store checks still admit the captured identity,
                        // but the transport now belongs to another token/session.
                        let mut current = identity.borrow_mut();
                        if replace_token {
                            current.0 = "replacement-token".to_string();
                        } else {
                            current.1 = "replacement-session".to_string();
                        }
                    }
                    allowed
                },
                |name| {
                    reads.set(reads.get() + 1);
                    crate::secrets::read_secret_value(root.path(), "credentials", name)
                        .map_err(|_| "fixture read failed".to_string())
                },
            );
            assert_eq!(validations.get(), 2);
            assert_eq!(reads.get(), 1);
            assert!(matches!(result, Err(ref error) if error == DENIED));
            assert_no_plaintext_files(root.path())?;
        }
        Ok(())
    }

    #[test]
    fn credential_reveal_discards_value_when_peer_or_session_is_revoked_during_read(
    ) -> anyhow::Result<()> {
        for revoked_id in ["fixture-peer", "fixture-session"] {
            let root = tempfile::tempdir()?;
            let token = token(root.path(), "chef")?;
            crate::secrets::write_secret_record(
                root.path(),
                "credentials",
                "TEST_LOGIN",
                CANARY,
                None,
                json!({}),
            )?;
            let result = reveal_with_reader(
                root.path(),
                "fixture-peer",
                "fixture-session",
                &token,
                vec![json!({"name":"TEST_LOGIN"})],
                || true,
                |name| {
                    // Deterministic interleaving: the actual encrypted read
                    // succeeds, then an administrative action revokes the peer
                    // before the read operation hands its value back.
                    let value = crate::secrets::read_secret_value(root.path(), "credentials", name)
                        .map_err(|_| "fixture read failed".to_string())?;
                    store::revoke_business_peer(root.path(), revoked_id, "fixture-admin", "test")
                        .map_err(|_| "fixture revoke failed".to_string())?;
                    assert!(
                        authorized(root.path(), &token),
                        "actor epoch alone still admits this token"
                    );
                    Ok(value)
                },
            );
            assert!(matches!(result, Err(ref error) if error == DENIED));
            assert_no_plaintext_files(root.path())?;
        }
        Ok(())
    }

    #[test]
    fn credential_reveal_discards_value_when_connection_retires_during_read() -> anyhow::Result<()>
    {
        let root = tempfile::tempdir()?;
        let token = token(root.path(), "chef")?;
        let current = std::cell::Cell::new(true);
        let result = reveal_with_reader(
            root.path(),
            "fixture-peer",
            "fixture-session",
            &token,
            vec![json!({"name":"TEST_LOGIN"})],
            || current.get(),
            |_| {
                current.set(false);
                Ok(CANARY.to_string())
            },
        );
        assert!(matches!(result, Err(ref error) if error == DENIED));
        assert_no_plaintext_files(root.path())?;
        Ok(())
    }

    #[test]
    fn credential_reveal_denies_missing_or_pre_revoked_peer_identity_without_reading(
    ) -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let token = token(root.path(), "chef")?;
        store::revoke_business_peer(root.path(), "revoked", "fixture-admin", "test")?;
        for (peer, session) in [
            ("", "session"),
            ("peer", ""),
            ("revoked", "session"),
            ("peer", "revoked"),
        ] {
            let result = reveal_with_reader(
                root.path(),
                peer,
                session,
                &token,
                vec![json!({"name":"TEST_LOGIN"})],
                || true,
                |_| panic!("denied peer must not read a secret"),
            );
            assert!(matches!(result, Err(ref error) if error == DENIED));
        }
        Ok(())
    }

    fn token(root: &Path, role: &str) -> anyhow::Result<String> {
        Ok(store::issue_business_os_capability_token_for_managed_user(
            root,
            "reveal-fixture",
            "Reveal fixture",
            role,
            chrono::Utc::now().timestamp_millis(),
        )?
        .0)
    }

    fn assert_no_plaintext_files(path: &Path) -> anyhow::Result<()> {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                assert_no_plaintext_files(&entry.path())?;
            } else if entry.file_type()?.is_file() {
                let bytes = std::fs::read(entry.path())?;
                assert!(
                    !bytes
                        .windows(CANARY.len())
                        .any(|part| part == CANARY.as_bytes()),
                    "synthetic secret persisted in plaintext"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn credential_reveal_returns_existing_value_without_rotation_or_persistence(
    ) -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let token = token(root.path(), "chef")?;
        crate::secrets::write_secret_record(
            root.path(),
            "credentials",
            "TEST_LOGIN",
            CANARY,
            None,
            json!({}),
        )?;
        let before = serde_json::to_value(crate::secrets::list_secret_records(root.path(), None)?)?;
        for _ in 0..2 {
            let response = handle_credential_reveal_webrtc_request(
                root.path(),
                "fixture-peer",
                "fixture-session",
                &token,
                vec![json!({"name":"TEST_LOGIN"})],
                || true,
            )
            .map_err(anyhow::Error::msg)?;
            assert_eq!(response["schema"], "ctox.credential-reveal.v1");
            assert_eq!(response["name"], "TEST_LOGIN");
            assert!(response["value"].as_str() == Some(CANARY));
        }
        assert_eq!(
            before,
            serde_json::to_value(crate::secrets::list_secret_records(root.path(), None)?)?
        );
        assert!(
            crate::secrets::read_secret_value(root.path(), "credentials", "TEST_LOGIN")? == CANARY
        );
        assert_no_plaintext_files(root.path())?;
        Ok(())
    }

    #[test]
    fn credential_reveal_denies_unprivileged_invalid_and_stale_capabilities() -> anyhow::Result<()>
    {
        let root = tempfile::tempdir()?;
        let old = token(root.path(), "chef")?;
        let unprivileged = token(root.path(), "user")?;
        for token in ["", "invalid-token", old.as_str(), unprivileged.as_str()] {
            let result = handle_credential_reveal_webrtc_request(
                root.path(),
                "fixture-peer",
                "fixture-session",
                token,
                vec![json!({"name":"TEST_LOGIN"})],
                || true,
            );
            assert!(matches!(result, Err(ref error) if error == DENIED));
        }
        Ok(())
    }

    #[test]
    fn credential_reveal_rejects_scope_overrides_and_redacts_errors() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let token = token(root.path(), "chef")?;
        for params in [
            vec![],
            vec![json!({}), json!({})],
            vec![json!({"name":"TEST_LOGIN", "scope":"other"})],
            vec![json!({"name":"TEST_LOGIN", "value":CANARY})],
            vec![json!({"name":CANARY})],
            vec![json!({"name":"X".repeat(65)})],
        ] {
            let result = handle_credential_reveal_webrtc_request(
                root.path(),
                "fixture-peer",
                "fixture-session",
                &token,
                params,
                || true,
            );
            assert!(matches!(result, Err(ref error) if error == INVALID));
        }
        let result = handle_credential_reveal_webrtc_request(
            root.path(),
            "fixture-peer",
            "fixture-session",
            &token,
            vec![json!({"name":"MISSING"})],
            || true,
        );
        assert!(matches!(result, Err(ref error) if error == UNAVAILABLE));
        assert_no_plaintext_files(root.path())?;
        Ok(())
    }
}
