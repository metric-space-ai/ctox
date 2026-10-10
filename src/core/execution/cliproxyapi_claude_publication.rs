// Origin: CTOX
// License: AGPL-3.0-only

//! Prepared native account metadata check. It is not a publication permit.
//! Only the genuine controller's already-held issuer/Core/Policy scope may
//! use this check. Credentials are never reread/decrypted inside that scope.

use super::*;
use crate::business_os::consumer_authority::ConsumerFacts;
use ctox_cliproxyapi::internal::config::{ClaudeSubscriptionAccountConfig, CliproxyRuntimeConfig};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior};
use sha2::{Digest, Sha256};

/// Holder-private, retained connections and exact encrypted-record identity.
/// No Serialize/Debug/Clone; preparing this object does not authorize a send.
pub(crate) struct NativeClaudeSdkPublicationCheck {
    account: ClaudeSubscriptionAccountConfig,
    encrypted_revision: String,
    runtime: Mutex<Connection>,
    secrets: Mutex<Connection>,
}

fn encrypted_revision(
    conn: &Connection,
    account: &ClaudeSubscriptionAccountConfig,
) -> Result<String> {
    let handles = account
        .credential_handles()
        .map_err(|_| anyhow::anyhow!("native Claude credential reference unavailable"))?;
    let keys = [handles.access_token(), handles.refresh_token()];
    let mut digest = Sha256::new();
    digest.update(b"ctox/native-claude-encrypted-records/v1");
    for key in keys {
        let (nonce, ciphertext): (String, String) = conn.query_row(
            "SELECT nonce_b64,ciphertext_b64 FROM ctox_secret_records WHERE scope=?1 AND secret_name=?2",
            rusqlite::params![key.scope(), key.name()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional()?.context("native Claude credential record unavailable")?;
        for value in [key.scope(), key.name(), nonce.as_str(), ciphertext.as_str()] {
            digest.update((value.len() as u64).to_be_bytes());
            digest.update(value.as_bytes());
        }
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn current_account(conn: &Connection, id: &str) -> Result<ClaudeSubscriptionAccountConfig> {
    let value: String = conn
        .query_row(
            "SELECT config_json FROM cliproxyapi_runtime_config WHERE config_id=1",
            [],
            |row| row.get(0),
        )
        .optional()?
        .context("native Claude configuration unavailable")?;
    let runtime: CliproxyRuntimeConfig = serde_json::from_str(&value)
        .map_err(|_| anyhow::anyhow!("native Claude configuration unavailable"))?;
    runtime
        .claude_accounts
        .into_iter()
        .find(|account| account.id == id)
        .context("native Claude account removed")
}

/// Preparatory revision observation only; this never permits publication.
pub(super) fn capture_revision(
    root: &std::path::Path,
    account: &ClaudeSubscriptionAccountConfig,
) -> Result<String> {
    let conn = Connection::open_with_flags(
        crate::secrets::resolve_db_path(root),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    conn.busy_timeout(std::time::Duration::ZERO)?;
    let tx = Transaction::new_unchecked(&conn, TransactionBehavior::Deferred)?;
    encrypted_revision(&tx, account)
}

impl NativeClaudeSdkPublicationCheck {
    /// Prepare outside transport/issuer/Core/Policy locks. Both connections are
    /// preopened without schema creation; no account secret is retained here.
    fn open(root: &std::path::Path, account: &ClaudeSubscriptionAccountConfig) -> Result<Self> {
        let runtime = Connection::open_with_flags(
            crate::inference::runtime_env::runtime_config_path(root),
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        runtime.busy_timeout(std::time::Duration::ZERO)?;
        ensure!(
            current_account(&runtime, &account.id)? == *account,
            "native Claude configuration changed during preparation"
        );
        let secrets = Connection::open_with_flags(
            crate::secrets::resolve_db_path(root),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        secrets.busy_timeout(std::time::Duration::ZERO)?;
        let encrypted_revision = encrypted_revision(&secrets, account)?;
        Ok(Self {
            account: account.clone(),
            encrypted_revision,
            runtime: Mutex::new(runtime),
            secrets: Mutex::new(secrets),
        })
    }

    pub(crate) fn prepare(reservation: &NativeClaudeSdkAccountReservation) -> Result<Self> {
        let current = stable_capture(
            &reservation.root,
            &reservation.selected.account().private_local_account_id,
        )?;
        let check = Self::open(&reservation.root, &current.account)?;
        ensure!(
            check.encrypted_revision == reservation.encrypted_revision,
            "native Claude credential changed"
        );
        ensure!(
            stable_capture(&reservation.root, &current.account.id)? == current,
            "native Claude account changed during preparation"
        );
        with_captured_current(
            &reservation.captured,
            &current,
            &reservation.private_binding,
            |_| Ok(()),
        )?;
        Ok(check)
    }

    pub(super) fn with_current_records<T>(&self, publish: impl FnOnce() -> Result<T>) -> Result<T> {
        let runtime = self
            .runtime
            .try_lock()
            .map_err(|_| anyhow::anyhow!("native Claude configuration busy"))?;
        let runtime = Transaction::new_unchecked(&runtime, TransactionBehavior::Immediate)?;
        ensure!(
            current_account(&runtime, &self.account.id)? == self.account,
            "native Claude configuration changed"
        );
        let secrets = self
            .secrets
            .try_lock()
            .map_err(|_| anyhow::anyhow!("native Claude credential metadata busy"))?;
        ensure!(
            encrypted_revision(&secrets, &self.account)? == self.encrypted_revision,
            "native Claude credential changed"
        );
        publish()
    }

    /// An additional metadata check, never a substitute for Crew's sealed
    /// original-controller scope. The caller already holds the genuine issuer
    /// fence, so encrypted-record rotation is excluded during this callback.
    /// A native runtime writer reservation excludes config edits while the
    /// bounded physical publication executes. No secret/transport reentry.
    pub(crate) fn with_current_in_held_policy<T>(
        &self,
        reservation: &NativeClaudeSdkAccountReservation,
        facts: &ConsumerFacts,
        policy: &Connection,
        publish: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let mut state = reservation
            .captured
            .try_lock()
            .map_err(|_| anyhow::anyhow!("native Claude account reservation unavailable"))?;
        let result = (|| {
            let prior = state
                .as_ref()
                .context("native Claude account reservation released")?;
            validate(prior, &reservation.private_binding)?;
            ensure!(
                prior.account == self.account,
                "native Claude prepared account changed"
            );
            reservation
                .selected
                .assert_current_in_policy(facts, policy)?;
            self.with_current_records(publish)
        })();
        if result.is_err() {
            state.take();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{account, persist};
    use super::*;

    #[test]
    fn metadata_check_pins_encrypted_generation_even_when_plaintext_is_restored() -> Result<()> {
        let root = tempfile::tempdir()?;
        let original = account("private-access", "private-refresh");
        persist(root.path(), &original)?;
        let check = NativeClaudeSdkPublicationCheck::open(root.path(), &original.account)?;
        check.with_current_records(|| Ok(()))?;
        assert_eq!(check.encrypted_revision.len(), 64);

        let store = crate::execution::cliproxyapi_host::CtoxClaudeSecretStore::new(root.path());
        use ctox_cliproxyapi::internal::auth::claude::ClaudeSecretStore;
        let handles = original.account.credential_handles().unwrap();
        store.store_credentials(
            &handles,
            &account("other-access", "other-refresh").credentials,
        )?;
        assert!(check
            .with_current_records::<()>(|| panic!("rotated record cannot publish"))
            .is_err());
        store.store_credentials(&handles, &original.credentials)?;
        assert!(check
            .with_current_records::<()>(|| panic!(
                "restoring plaintext cannot restore the encrypted generation"
            ))
            .is_err());
        Ok(())
    }

    #[test]
    fn physical_check_uses_existing_issuer_fence_without_secret_reentry() -> Result<()> {
        let root = tempfile::tempdir()?;
        // Normal bootstrap, never schema initialization under the issuer.
        crate::persistence::store_json_payload(root.path(), "publication-test", Some(&true))?;
        let original = account("private-access", "private-refresh");
        persist(root.path(), &original)?;
        let check = NativeClaudeSdkPublicationCheck::open(root.path(), &original.account)?;
        let handles = original.account.credential_handles().unwrap();
        let access = handles.access_token();
        let refresh = handles.refresh_token();
        let other = Connection::open(crate::secrets::resolve_db_path(root.path()))?;
        other.busy_timeout(std::time::Duration::ZERO)?;
        crate::secrets::with_current_secret_values(
            root.path(),
            &[
                (access.scope(), access.name()),
                (refresh.scope(), refresh.name()),
            ],
            |_| {
                check.with_current_records(|| {
                // Metadata reads succeed while the already-held issuer fence
                // excludes a second encrypted-record writer.
                let err = other.execute(
                    "UPDATE ctox_secret_records SET nonce_b64=nonce_b64 WHERE scope=?1 AND secret_name=?2",
                    rusqlite::params![access.scope(), access.name()],
                ).unwrap_err();
                assert!(matches!(err, rusqlite::Error::SqliteFailure(code, _)
                    if matches!(code.code, rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)));
                Ok(())
            })
            },
        )?;
        Ok(())
    }

    #[test]
    fn native_runtime_edit_or_account_removal_prevents_publication() -> Result<()> {
        let root = tempfile::tempdir()?;
        let original = account("private-access", "private-refresh");
        persist(root.path(), &original)?;
        let check = NativeClaudeSdkPublicationCheck::open(root.path(), &original.account)?;
        let runtime = Connection::open(crate::inference::runtime_env::runtime_config_path(
            root.path(),
        ))?;
        let mut changed = original.account.clone();
        changed.disabled = true;
        let value = serde_json::to_string(&serde_json::json!({"claude_accounts":[changed]}))?;
        runtime.execute(
            "UPDATE cliproxyapi_runtime_config SET config_json=?1,revision=revision+1",
            [value],
        )?;
        assert!(check
            .with_current_records::<()>(|| panic!("disabled account cannot publish"))
            .is_err());
        runtime.execute(
            "UPDATE cliproxyapi_runtime_config SET config_json=?1,revision=revision+1",
            [r#"{"claude_accounts":[]}"#],
        )?;
        assert!(check
            .with_current_records::<()>(|| panic!("removed account cannot publish"))
            .is_err());
        Ok(())
    }

    #[test]
    fn physical_account_check_fences_runtime_writer_and_ignores_unrelated_accounts() -> Result<()> {
        let root = tempfile::tempdir()?;
        let original = account("private-access", "private-refresh");
        persist(root.path(), &original)?;
        let check = NativeClaudeSdkPublicationCheck::open(root.path(), &original.account)?;
        let other = Connection::open(crate::inference::runtime_env::runtime_config_path(
            root.path(),
        ))?;
        other.busy_timeout(std::time::Duration::ZERO)?;
        check.with_current_records(|| {
            let err = other.execute("UPDATE cliproxyapi_runtime_config SET revision=revision+1", []).unwrap_err();
            assert!(matches!(err, rusqlite::Error::SqliteFailure(code, _)
                if matches!(code.code, rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)));
            Ok(())
        })?;
        other.execute(
            "UPDATE cliproxyapi_runtime_config SET revision=revision+1",
            [],
        )?;
        // A global revision edit with the exact account unchanged does not
        // unnecessarily move/cache-reset a still current account session.
        check.with_current_records(|| Ok(()))?;
        Ok(())
    }
}
