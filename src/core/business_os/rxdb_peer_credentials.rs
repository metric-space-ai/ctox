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
    capability_token: &str,
    mut params: Vec<Value>,
) -> Result<Value, String> {
    if !authorized(root, capability_token) {
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
    let value = Zeroizing::new(
        crate::secrets::read_secret_value(root, "credentials", name)
            .map_err(|_| UNAVAILABLE.to_string())?,
    );
    if value.len() > 64 * 1024 {
        return Err(UNAVAILABLE.into());
    }
    // Recheck revocation/role/epoch after the potentially blocking store read.
    if !authorized(root, capability_token) {
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
                &token,
                vec![json!({"name":"TEST_LOGIN"})],
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
                token,
                vec![json!({"name":"TEST_LOGIN"})],
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
            let result = handle_credential_reveal_webrtc_request(root.path(), &token, params);
            assert!(matches!(result, Err(ref error) if error == INVALID));
        }
        let result = handle_credential_reveal_webrtc_request(
            root.path(),
            &token,
            vec![json!({"name":"MISSING"})],
        );
        assert!(matches!(result, Err(ref error) if error == UNAVAILABLE));
        assert_no_plaintext_files(root.path())?;
        Ok(())
    }
}
