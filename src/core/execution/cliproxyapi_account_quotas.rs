// Origin: CTOX; License: AGPL-3.0-only
//! Non-secret provider observations in the canonical local SQLite payload store.
use ctox_cliproxyapi::sdk::cliproxy::auth::{
    AccountQuotaStore, AccountQuotaStoreError, ReportedQuotaSnapshot,
};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf};

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
fn read(
    conn: &Connection,
    key: &str,
) -> Result<Option<ReportedQuotaSnapshot>, AccountQuotaStoreError> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT payload_json FROM ctox_payload_store WHERE payload_key=?1",
            [key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| AccountQuotaStoreError::Read)?;
    raw.map(|raw| {
        let state: ReportedQuotaSnapshot =
            serde_json::from_str(&raw).map_err(|_| AccountQuotaStoreError::InvalidRecord)?;
        state.validate()?;
        Ok(state)
    })
    .transpose()
}
impl AccountQuotaStore for CtoxAccountQuotaStore {
    fn load(
        &self,
        provider: &str,
        auth_ids: &[String],
    ) -> Result<BTreeMap<String, ReportedQuotaSnapshot>, AccountQuotaStoreError> {
        let conn = crate::persistence::open_sqlite(&self.root)
            .map_err(|_| AccountQuotaStoreError::Read)?;
        let mut values = BTreeMap::new();
        for id in auth_ids {
            if let Some(state) = read(&conn, &key(provider, id)?)? {
                values.insert(id.clone(), state);
            }
        }
        Ok(values)
    }
    fn save(
        &self,
        provider: &str,
        auth_id: &str,
        state: &ReportedQuotaSnapshot,
    ) -> Result<(), AccountQuotaStoreError> {
        state.validate()?;
        let key = key(provider, auth_id)?;
        let conn = crate::persistence::open_sqlite(&self.root)
            .map_err(|_| AccountQuotaStoreError::Write)?;
        let tx =
            crate::persistence::SqliteWriteTransaction::begin(&conn, "cliproxyapi.quota.observe")
                .map_err(|_| AccountQuotaStoreError::Write)?;
        let previous = read(&tx, &key)?;
        if previous
            .as_ref()
            .is_some_and(|prior| prior.observed_at_ms > state.observed_at_ms)
        {
            return Ok(());
        }
        if previous.is_none() {
            let count: i64 = tx.query_row(
                "SELECT COUNT(*) FROM ctox_payload_store WHERE payload_key LIKE 'cliproxyapi.auth.reported-quota.v1:%'",
                [], |row| row.get(0),
            ).map_err(|_| AccountQuotaStoreError::Read)?;
            if count >= MAX_ACCOUNTS {
                return Err(AccountQuotaStoreError::Capacity);
            }
        }
        tx.execute(
            "INSERT INTO ctox_payload_store(payload_key,payload_json,updated_at) VALUES (?1,?2,unixepoch()) ON CONFLICT(payload_key) DO UPDATE SET payload_json=excluded.payload_json,updated_at=excluded.updated_at",
            params![key,serde_json::to_string(state).map_err(|_| AccountQuotaStoreError::InvalidRecord)?],
        ).map_err(|_| AccountQuotaStoreError::Write)?;
        tx.commit().map_err(|_| AccountQuotaStoreError::Write)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctox_cliproxyapi::sdk::cliproxy::auth::{
        QuotaWindowKind, ReportedQuotaStatus, ReportedQuotaWindow,
    };
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
}
