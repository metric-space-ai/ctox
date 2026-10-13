// Origin: CTOX; License: AGPL-3.0-only
//! Reported quota data in the canonical holder-private SQLite payload store.
//! Private generations never leave this module through the public snapshot DTO.
use ctox_cliproxyapi::sdk::cliproxy::auth::{
    AccountQuotaGeneration, AccountQuotaStore, AccountQuotaStoreError, ReportedQuotaSnapshot,
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::MutexGuard,
};

const PREFIX: &str = "cliproxyapi.auth.reported-quota.v1:";
const MAX_ACCOUNTS: i64 = 100_000;
pub(super) struct CtoxAccountQuotaStore {
    root: PathBuf,
}
impl CtoxAccountQuotaStore {
    pub(super) fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredObservation {
    generation: [u8; 32],
    snapshot: ReportedQuotaSnapshot,
}
fn key(provider: &str, auth_id: &str) -> Result<String, AccountQuotaStoreError> {
    let provider = provider.trim().to_ascii_lowercase();
    if provider.is_empty()
        || provider.len() > 64
        || !ctox_cliproxyapi::internal::api::account_selection::valid_account_id(auth_id)
    {
        return Err(AccountQuotaStoreError::InvalidRecord);
    }
    let mut hash = Sha256::new();
    hash.update(b"ctox.reported-quota.v1");
    for value in [provider.as_str(), auth_id] {
        hash.update((value.len() as u64).to_be_bytes());
        hash.update(value.as_bytes());
    }
    Ok(format!("{PREFIX}{:x}", hash.finalize()))
}
fn frame(hash: &mut Sha256, value: &str) {
    hash.update((value.len() as u64).to_be_bytes());
    hash.update(value.as_bytes());
}
// Caller holds the credential lifecycle guard. Only native config and protected
// record versions are inspected; no credential value is read or decrypted here.
fn generation(
    root: &Path,
    provider: &str,
    auth_id: &str,
) -> Result<Option<AccountQuotaGeneration>, AccountQuotaStoreError> {
    key(provider, auth_id)?;
    if !provider.trim().eq_ignore_ascii_case("claude") {
        return Ok(None);
    }
    let Some(stored) =
        super::load_instance_proxy_config(root).map_err(|_| AccountQuotaStoreError::Read)?
    else {
        return Ok(None);
    };
    let Some(account) = stored
        .runtime
        .claude_accounts
        .iter()
        .find(|a| a.id == auth_id)
    else {
        return Ok(None);
    };
    let mut hash = Sha256::new();
    hash.update(b"ctox.reported-quota.credential-generation.v1");
    for value in [
        auth_id,
        account.upstream_scheme.as_str(),
        account.upstream_authority.as_str(),
    ] {
        frame(&mut hash, value);
    }
    let mut refs = vec![&account.access_token_secret, &account.refresh_token_secret];
    if let Some(proxy) = &account.proxy_url_secret {
        refs.push(proxy);
    }
    for secret in refs {
        let Some(version) =
            crate::secrets::secret_record_content_version(root, &secret.scope, &secret.name)
                .map_err(|_| AccountQuotaStoreError::Read)?
        else {
            return Ok(None);
        };
        for value in [&secret.scope, &secret.name, &version] {
            frame(&mut hash, value);
        }
    }
    // Enablement, model selection, weights and priority do not change a credential.
    Ok(Some(AccountQuotaGeneration::from_host_digest(
        hash.finalize().into(),
    )))
}
fn read(
    conn: &Connection,
    key: &str,
    generation: &AccountQuotaGeneration,
) -> Result<Option<ReportedQuotaSnapshot>, AccountQuotaStoreError> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT payload_json FROM ctox_payload_store WHERE payload_key=?1",
            [key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| AccountQuotaStoreError::Read)?;
    let Some(raw) = raw else { return Ok(None) };
    let state: StoredObservation = match serde_json::from_str(&raw) {
        Ok(state) => state,
        Err(_) => {
            // An older unbound record cannot prove which credential produced it.
            // Treat valid legacy quota as unknown until a new actual response.
            let legacy: ReportedQuotaSnapshot =
                serde_json::from_str(&raw).map_err(|_| AccountQuotaStoreError::InvalidRecord)?;
            legacy.validate()?;
            return Ok(None);
        }
    };
    state.snapshot.validate()?;
    Ok((state.generation == *generation.host_digest()).then_some(state.snapshot))
}
/// Called by the existing proof-bearing account-removal recovery while it owns
/// both the lifecycle guard and the Runtime topology write transaction.
/// This is idempotent and scoped to the exact removed provider/local account.
pub(super) fn remove_under_lifecycle(
    root: &Path,
    provider: &str,
    auth_id: &str,
    _guard: &MutexGuard<'_, ()>,
) -> Result<(), AccountQuotaStoreError> {
    let key = key(provider, auth_id)?;
    let conn = crate::persistence::open_sqlite(root).map_err(|_| AccountQuotaStoreError::Write)?;
    let tx = crate::persistence::SqliteWriteTransaction::begin(&conn, "cliproxyapi.quota.remove")
        .map_err(|_| AccountQuotaStoreError::Write)?;
    tx.execute("DELETE FROM ctox_payload_store WHERE payload_key=?1", [key])
        .map_err(|_| AccountQuotaStoreError::Write)?;
    tx.commit().map_err(|_| AccountQuotaStoreError::Write)
}
/// All IO completes before the Policy callback opens its read snapshot. The
/// lifecycle guard and Runtime fence prevent a replacement credential being
/// associated with the old adopted account while this public result is built.
pub(super) fn with_public_snapshots<T>(
    root: &Path,
    accounts: &[(&str, &str)],
    publish: impl FnOnce(&BTreeMap<String, serde_json::Value>) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    anyhow::ensure!(accounts.len() <= 256, "quota account limit exceeded");
    if accounts.is_empty() {
        return publish(&BTreeMap::new());
    }
    let _guard = crate::secrets::credential_lifecycle_guard();
    let mut runtime = super::open_instance_proxy_config_db(root)?;
    let _runtime_fence = runtime.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let core = crate::persistence::open_sqlite(root)?;
    let mut snapshots = BTreeMap::new();
    for (id, binding) in accounts {
        let observed = (|| -> Result<Option<ReportedQuotaSnapshot>, AccountQuotaStoreError> {
            let current = crate::execution::cliproxyapi_claude_catalog::account_binding(root, id)
                .map_err(|_| AccountQuotaStoreError::Read)?;
            if current.as_deref() != Some(*binding) {
                return Ok(None);
            }
            let Some(current) = generation(root, "claude", id)? else {
                return Ok(None);
            };
            read(&core, &key("claude", id)?, &current)
        })();
        let value = match observed {
            Ok(Some(snapshot))
                if snapshot.observed_at_ms <= chrono::Utc::now().timestamp_millis() =>
            {
                let windows: Vec<_> = snapshot.windows.iter().map(|window| serde_json::json!({
                    "kind":window.kind,"status":window.status,"resetAtMs":window.reset_at_ms,
                    "utilization":window.utilization
                })).collect();
                serde_json::json!({"state":"reported","observedAtMs":snapshot.observed_at_ms,
                    "windows":windows})
            }
            Ok(_) => serde_json::json!({"state":"unknown"}),
            Err(_) => serde_json::json!({"state":"unavailable","errorClass":"quota_read_failed"}),
        };
        snapshots.insert((*id).to_owned(), value);
    }
    // Never serialize the map itself: its keys are private local selectors.
    publish(&snapshots)
}

impl AccountQuotaStore for CtoxAccountQuotaStore {
    fn load(
        &self,
        provider: &str,
        auth_ids: &[String],
    ) -> Result<BTreeMap<String, ReportedQuotaSnapshot>, AccountQuotaStoreError> {
        let _guard = crate::secrets::credential_lifecycle_guard();
        let conn = crate::persistence::open_sqlite(&self.root)
            .map_err(|_| AccountQuotaStoreError::Read)?;
        let mut values = BTreeMap::new();
        for id in auth_ids {
            let Some(current) = generation(&self.root, provider, id)? else {
                continue;
            };
            if let Some(state) = read(&conn, &key(provider, id)?, &current)? {
                values.insert(id.clone(), state);
            }
        }
        Ok(values)
    }
    fn capture_generation(
        &self,
        provider: &str,
        auth_id: &str,
    ) -> Result<Option<AccountQuotaGeneration>, AccountQuotaStoreError> {
        let _guard = crate::secrets::credential_lifecycle_guard();
        generation(&self.root, provider, auth_id)
    }
    fn save(
        &self,
        provider: &str,
        auth_id: &str,
        state: &ReportedQuotaSnapshot,
    ) -> Result<(), AccountQuotaStoreError> {
        let captured = self.capture_generation(provider, auth_id)?;
        self.save_if_generation(provider, auth_id, state, captured.as_ref())
    }
    fn save_if_generation(
        &self,
        provider: &str,
        auth_id: &str,
        state: &ReportedQuotaSnapshot,
        captured: Option<&AccountQuotaGeneration>,
    ) -> Result<(), AccountQuotaStoreError> {
        state.validate()?;
        let key = key(provider, auth_id)?;
        let Some(captured) = captured else {
            return Ok(());
        };
        let _guard = crate::secrets::credential_lifecycle_guard();
        // Exclude topology writes by other processes as well as this process.
        // Runtime -> Core lock order matches proof-bearing removal recovery.
        let mut runtime = super::open_instance_proxy_config_db(&self.root)
            .map_err(|_| AccountQuotaStoreError::Write)?;
        let _runtime_tx = runtime
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| AccountQuotaStoreError::Write)?;
        if generation(&self.root, provider, auth_id)?.as_ref() != Some(captured) {
            return Ok(());
        }
        let conn = crate::persistence::open_sqlite(&self.root)
            .map_err(|_| AccountQuotaStoreError::Write)?;
        let tx =
            crate::persistence::SqliteWriteTransaction::begin(&conn, "cliproxyapi.quota.observe")
                .map_err(|_| AccountQuotaStoreError::Write)?;
        let previous = read(&tx, &key, captured)?;
        if previous
            .as_ref()
            .is_some_and(|prior| prior.observed_at_ms > state.observed_at_ms)
        {
            return Ok(());
        }
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM ctox_payload_store WHERE payload_key=?1)",
                [&key],
                |row| row.get(0),
            )
            .map_err(|_| AccountQuotaStoreError::Read)?;
        if !exists {
            let count: i64 = tx.query_row(
                "SELECT COUNT(*) FROM ctox_payload_store WHERE payload_key LIKE 'cliproxyapi.auth.reported-quota.v1:%'", [], |row| row.get(0)
            ).map_err(|_| AccountQuotaStoreError::Read)?;
            if count >= MAX_ACCOUNTS {
                return Err(AccountQuotaStoreError::Capacity);
            }
        }
        let stored = StoredObservation {
            generation: *captured.host_digest(),
            snapshot: state.clone(),
        };
        tx.execute(
            "INSERT INTO ctox_payload_store(payload_key,payload_json,updated_at) VALUES (?1,?2,unixepoch()) ON CONFLICT(payload_key) DO UPDATE SET payload_json=excluded.payload_json,updated_at=excluded.updated_at",
            params![key, serde_json::to_string(&stored).map_err(|_| AccountQuotaStoreError::InvalidRecord)?]
        ).map_err(|_| AccountQuotaStoreError::Write)?;
        tx.commit().map_err(|_| AccountQuotaStoreError::Write)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctox_cliproxyapi::internal::auth::claude::{ClaudeStoredCredentials, SecretString};
    use ctox_cliproxyapi::sdk::cliproxy::auth::{
        QuotaWindowKind, ReportedQuotaStatus, ReportedQuotaWindow,
    };
    fn install(root: &Path, id: &str, token: &str) {
        let credentials = ClaudeStoredCredentials::new(
            SecretString::new(token).unwrap(),
            SecretString::new(format!("refresh-{token}")).unwrap(),
        );
        super::super::install_claude_subscription(root, id, &credentials).unwrap();
    }
    fn snapshot(observed: i64, reset: i64) -> ReportedQuotaSnapshot {
        ReportedQuotaSnapshot {
            observed_at_ms: observed,
            windows: vec![ReportedQuotaWindow {
                kind: QuotaWindowKind::FiveHour,
                status: ReportedQuotaStatus::Allowed,
                reset_at_ms: reset,
                utilization: Some(0.25),
            }],
        }
    }
    #[test]
    fn provider_quota_store_survives_restart_and_scopes_account_and_provider() {
        let root = tempfile::tempdir().unwrap();
        install(root.path(), "account-a", "quota-access-a");
        install(root.path(), "account-b", "quota-access-b");
        let store = CtoxAccountQuotaStore::new(root.path());
        store
            .save("claude", "account-a", &snapshot(1_000, 5_000))
            .unwrap();
        drop(store);
        let store = CtoxAccountQuotaStore::new(root.path());
        let ids = ["account-a".into(), "account-b".into()];
        assert_eq!(
            store.load("CLAUDE", &ids).unwrap(),
            BTreeMap::from([("account-a".into(), snapshot(1_000, 5_000))])
        );
        assert!(store.load("codex", &ids).unwrap().is_empty());
    }
    #[test]
    fn provider_quota_store_older_response_cannot_replace_newer_observation() {
        let root = tempfile::tempdir().unwrap();
        install(root.path(), "account-a", "quota-access-a");
        let store = CtoxAccountQuotaStore::new(root.path());
        store
            .save("claude", "account-a", &snapshot(2_000, 5_000))
            .unwrap();
        store
            .save("claude", "account-a", &snapshot(1_000, 8_000))
            .unwrap();
        assert_eq!(
            store.load("claude", &["account-a".into()]).unwrap()["account-a"].observed_at_ms,
            2_000
        );
    }
    #[test]
    fn provider_quota_store_rejects_invalid_or_duplicate_windows_without_overwriting() {
        let root = tempfile::tempdir().unwrap();
        install(root.path(), "account-a", "quota-access-a");
        let store = CtoxAccountQuotaStore::new(root.path());
        let valid = snapshot(1_000, 5_000);
        store.save("claude", "account-a", &valid).unwrap();
        let mut invalid = valid.clone();
        invalid.windows.push(valid.windows[0].clone());
        assert_eq!(
            store.save("claude", "account-a", &invalid),
            Err(AccountQuotaStoreError::InvalidRecord)
        );
        assert_eq!(
            store.load("claude", &["account-a".into()]).unwrap()["account-a"],
            valid
        );
    }
    #[test]
    fn provider_quota_store_old_credential_response_cannot_overwrite_relogin() {
        let root = tempfile::tempdir().unwrap();
        install(root.path(), "account-a", "old-quota-access");
        let store = CtoxAccountQuotaStore::new(root.path());
        let old = store.capture_generation("claude", "account-a").unwrap();
        store
            .save("claude", "account-a", &snapshot(1_000, 5_000))
            .unwrap();
        install(root.path(), "account-a", "new-quota-access");
        assert!(store
            .load("claude", &["account-a".into()])
            .unwrap()
            .is_empty());
        let fresh = snapshot(2_000, 6_000);
        store.save("claude", "account-a", &fresh).unwrap();
        store
            .save_if_generation("claude", "account-a", &snapshot(3_000, 9_000), old.as_ref())
            .unwrap();
        assert_eq!(
            store.load("claude", &["account-a".into()]).unwrap()["account-a"],
            fresh
        );
    }
    #[test]
    fn provider_quota_store_unbound_legacy_data_is_unknown_until_real_observation() {
        let root = tempfile::tempdir().unwrap();
        install(root.path(), "account-a", "quota-access-a");
        let conn = crate::persistence::open_sqlite(root.path()).unwrap();
        conn.execute("INSERT INTO ctox_payload_store(payload_key,payload_json,updated_at) VALUES (?1,?2,unixepoch())", params![key("claude", "account-a").unwrap(), serde_json::to_string(&snapshot(1_000, 5_000)).unwrap()]).unwrap();
        drop(conn);
        let store = CtoxAccountQuotaStore::new(root.path());
        assert!(store
            .load("claude", &["account-a".into()])
            .unwrap()
            .is_empty());
        store
            .save("claude", "account-a", &snapshot(2_000, 6_000))
            .unwrap();
        assert_eq!(
            store.load("claude", &["account-a".into()]).unwrap()["account-a"],
            snapshot(2_000, 6_000)
        );
    }
}
