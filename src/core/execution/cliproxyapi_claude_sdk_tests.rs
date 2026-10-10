// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use crate::execution::cliproxyapi_host::{save_instance_proxy_config, CtoxClaudeSecretStore};
use ctox_cliproxyapi::internal::{
    auth::claude::{ClaudeSecretStore, ClaudeStoredCredentials},
    config::CliproxyRuntimeConfig,
};

// Verified account GET/models: g3-claude-live-models-20261009.json.
const MODEL: &str = "claude-opus-5-5";

pub(super) fn account(access: &str, refresh: &str) -> Captured {
    Captured {
        account: serde_json::from_value(serde_json::json!({
            "id":"native-claude",
            "models":[MODEL],
            "access_token_secret":{"scope":"provider-subscriptions","name":"sdk-access"},
            "refresh_token_secret":{"scope":"provider-subscriptions","name":"sdk-refresh"}
        }))
        .unwrap(),
        credentials: ClaudeStoredCredentials::new(
            SecretString::new(access).unwrap(),
            SecretString::new(refresh).unwrap(),
        ),
    }
}

pub(super) fn persist(root: &std::path::Path, value: &Captured) -> Result<()> {
    let runtime: CliproxyRuntimeConfig = serde_json::from_value(serde_json::json!({
        "claude_accounts":[value.account],
    }))?;
    CtoxClaudeSecretStore::new(root).store_credentials(
        &value.account.credential_handles().unwrap(),
        &value.credentials,
    )?;
    save_instance_proxy_config(root, 0, "claude", runtime)?;
    Ok(())
}

#[test]
fn holder_reads_exact_encrypted_account_and_exposes_no_refresh_in_sdk_configuration() -> Result<()>
{
    let root = tempfile::tempdir()?;
    let expected = account("access-private", "refresh-private");
    persist(root.path(), &expected)?;
    let selected = stable_capture(root.path(), "native-claude")?;
    assert!(selected == expected);
    assert!(stable_capture(root.path(), "other-account").is_err());
    let binding = fingerprint(&selected)?;
    let state = Mutex::new(Some(selected));
    let current = stable_capture(root.path(), "native-claude")?;
    with_captured_current(&state, &current, &binding, |value| {
        let configuration = NativeClaudeSdkConfiguration {
            model: MODEL,
            access_token: value.credentials.access_token(),
            private_binding: &binding,
        };
        assert_eq!(configuration.model(), MODEL);
        assert_eq!(configuration.upstream(), "https://api.anthropic.com");
        assert_eq!(
            configuration.access_token().expose_secret(),
            "access-private"
        );
        assert_eq!(configuration.private_binding(), binding);
        assert_eq!(
            format!("{:?}", configuration.access_token()),
            "SecretString([REDACTED])"
        );
        Ok(())
    })?;
    Ok(())
}

#[test]
fn account_access_refresh_and_configuration_changes_permanently_retire_a_reservation() -> Result<()>
{
    for change in ["access", "refresh", "configuration"] {
        let previous = account("access-private", "refresh-private");
        let binding = fingerprint(&previous)?;
        let state = Mutex::new(Some(previous));
        let mut current = account("access-private", "refresh-private");
        match change {
            "access" => {
                current.credentials = account("replacement-access", "refresh-private").credentials
            }
            "refresh" => {
                current.credentials = account("access-private", "replacement-refresh").credentials
            }
            "configuration" => current.account.priority += 1,
            _ => unreachable!(),
        }
        let mut invoked = false;
        assert!(
            with_captured_current::<()>(&state, &current, &binding, |_| {
                invoked = true;
                Ok(())
            })
            .is_err()
        );
        assert!(!invoked);
        assert!(state.lock().unwrap().is_none());
        // Even restoring the old credential cannot resurrect this reservation.
        assert!(with_captured_current::<()>(
            &state,
            &account("access-private", "refresh-private"),
            &binding,
            |_| { panic!("retired snapshot cannot dispatch") }
        )
        .is_err());
    }
    Ok(())
}

#[test]
fn relogin_in_real_secret_store_never_reuses_the_old_sdk_account() -> Result<()> {
    let root = tempfile::tempdir()?;
    let old = account("old-private-access", "old-private-refresh");
    persist(root.path(), &old)?;
    let binding = fingerprint(&old)?;
    let state = Mutex::new(Some(stable_capture(root.path(), "native-claude")?));
    let new = account("new-private-access", "new-private-refresh");
    CtoxClaudeSecretStore::new(root.path())
        .store_credentials(&new.account.credential_handles().unwrap(), &new.credentials)?;
    let current = stable_capture(root.path(), "native-claude")?;
    assert!(
        with_captured_current::<()>(&state, &current, &binding, |_| {
            panic!("old reservation cannot export a re-login credential")
        })
        .is_err()
    );
    assert!(state.lock().unwrap().is_none());
    assert!(validate(&current, &fingerprint(&current)?).is_ok());
    Ok(())
}

#[test]
fn disabled_redirected_proxied_or_malformed_credentials_cannot_prepare_sdk_auth() -> Result<()> {
    for mode in [
        "disabled",
        "http",
        "authority",
        "proxy",
        "whitespace",
        "control",
    ] {
        let mut selected = account("access-private", "refresh-private");
        match mode {
            "disabled" => selected.account.disabled = true,
            "http" => selected.account.upstream_scheme = "http".into(),
            "authority" => selected.account.upstream_authority = "elsewhere.invalid".into(),
            "proxy" => {
                selected.account.proxy_url_secret =
                    Some(selected.account.access_token_secret.clone())
            }
            "whitespace" => selected.credentials = account(" ", "refresh-private").credentials,
            "control" => {
                selected.credentials = account("bad\ncredential", "refresh-private").credentials
            }
            _ => unreachable!(),
        }
        let error = validate(&selected, &fingerprint(&selected)?)
            .unwrap_err()
            .to_string();
        assert!(!error.contains("access-private"));
        assert!(!error.contains("refresh-private"));
        assert!(!error.contains("bad\ncredential"));
    }
    let selected = account("access-private", "refresh-private");
    assert!(validate(&selected, "stale-private-binding").is_err());
    Ok(())
}

#[test]
fn release_is_idempotent_and_cannot_be_recovered_by_a_retained_callback() -> Result<()> {
    let selected = account("access-private", "refresh-private");
    let binding = fingerprint(&selected)?;
    let state = Mutex::new(Some(selected));
    release_captured(&state);
    release_captured(&state);
    assert!(state.lock().unwrap().is_none());
    assert!(with_captured_current::<()>(
        &state,
        &account("access-private", "refresh-private"),
        &binding,
        |_| { panic!("released SDK account cannot dispatch") }
    )
    .is_err());
    Ok(())
}

#[test]
fn failed_callback_still_allows_credential_retirement() -> Result<()> {
    let selected = account("access-private", "refresh-private");
    let binding = fingerprint(&selected)?;
    let state = Mutex::new(Some(selected));
    let panic = std::panic::catch_unwind(|| {
        let _: Result<()> = with_captured_current(
            &state,
            &account("access-private", "refresh-private"),
            &binding,
            |_| panic!("controlled SDK callback failure"),
        );
    });
    assert!(panic.is_err());
    release_captured(&state);
    let retired = match state.lock() {
        Err(poisoned) => poisoned.into_inner(),
        Ok(_) => panic!("controlled callback must poison the reservation"),
    };
    assert!(retired.is_none());
    Ok(())
}
