// Origin: CTOX; License: AGPL-3.0-only
//! Private, non-secret bindings in the existing CTOX SQLite payload store.
use ctox_cliproxyapi::sdk::cliproxy::auth::conductor_execution::AccountRoutingError;
use ctox_cliproxyapi::sdk::cliproxy::auth::{AccountSessionStore, AccountSessionStoreError};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::PathBuf;

const PREFIX: &str = "cliproxyapi.auth.session.v1:";
const MAX_BINDINGS: i64 = 100_000;

pub(super) struct CtoxAccountSessionStore {
    root: PathBuf,
}
impl CtoxAccountSessionStore {
    pub(super) fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}
fn error(kind: AccountSessionStoreError) -> AccountRoutingError {
    AccountRoutingError::Session(kind)
}
fn read(conn: &Connection, key: &str) -> Result<Option<String>, AccountRoutingError> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT payload_json FROM ctox_payload_store WHERE payload_key = ?1",
            [key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| error(AccountSessionStoreError::Read))?;
    raw.map(|raw| {
        let id: String = serde_json::from_str(&raw)
            .map_err(|_| error(AccountSessionStoreError::InvalidRecord))?;
        if !ctox_cliproxyapi::internal::api::account_selection::valid_account_id(&id) {
            return Err(error(AccountSessionStoreError::InvalidRecord));
        }
        Ok(id)
    })
    .transpose()
}
impl AccountSessionStore for CtoxAccountSessionStore {
    fn choose(
        &self,
        key: &str,
        choose: &mut dyn FnMut(Option<&str>) -> Result<String, AccountRoutingError>,
    ) -> Result<String, AccountRoutingError> {
        if key.len() != 64
            || !key
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err(error(AccountSessionStoreError::InvalidRecord));
        }
        let key = format!("{PREFIX}{key}");
        let conn = crate::persistence::open_sqlite(&self.root)
            .map_err(|_| error(AccountSessionStoreError::Read))?;
        let previous = read(&conn, &key)?;
        let selected = choose(previous.as_deref())?;
        // Healthy continuations need no writer transaction and no TTL-based
        // reassignment. The chooser has still checked current eligibility.
        if previous.as_deref() == Some(selected.as_str()) {
            return Ok(selected);
        }
        let tx =
            crate::persistence::SqliteWriteTransaction::begin(&conn, "cliproxyapi.session.select")
                .map_err(|_| error(AccountSessionStoreError::Write))?;
        let current = read(&tx, &key)?;
        let selected = choose(current.as_deref())?;
        if !ctox_cliproxyapi::internal::api::account_selection::valid_account_id(&selected) {
            return Err(error(AccountSessionStoreError::InvalidRecord));
        }
        if current.is_none() {
            let count: i64 = tx.query_row(
                "SELECT COUNT(*) FROM ctox_payload_store WHERE payload_key LIKE 'cliproxyapi.auth.session.v1:%'",
                [], |row| row.get(0),
            ).map_err(|_| error(AccountSessionStoreError::Read))?;
            // Fail visibly rather than evicting a healthy live session.
            if count >= MAX_BINDINGS {
                return Err(error(AccountSessionStoreError::Capacity));
            }
        }
        if current.as_deref() != Some(selected.as_str()) {
            tx.execute(
                "INSERT INTO ctox_payload_store(payload_key,payload_json,updated_at) VALUES (?1,?2,unixepoch()) ON CONFLICT(payload_key) DO UPDATE SET payload_json=excluded.payload_json,updated_at=excluded.updated_at",
                params![key, serde_json::to_string(&selected).map_err(|_| error(AccountSessionStoreError::InvalidRecord))?],
            ).map_err(|_| error(AccountSessionStoreError::Write))?;
        }
        tx.commit()
            .map_err(|_| error(AccountSessionStoreError::Write))?;
        Ok(selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctox_cliproxyapi::sdk::cliproxy::auth::{account_session_key, AccountSelectionError};

    #[test]
    fn provider_session_binding_survives_router_restart_and_contains_no_raw_session() {
        let root = tempfile::tempdir().unwrap();
        let key = account_session_key("claude", None, "private-session-label").unwrap();
        let first = CtoxAccountSessionStore::new(root.path());
        assert_eq!(
            first
                .choose(&key, &mut |prior| Ok(prior.unwrap_or("account-a").into()))
                .unwrap(),
            "account-a"
        );
        drop(first);
        let restored = CtoxAccountSessionStore::new(root.path());
        assert_eq!(
            restored
                .choose(&key, &mut |prior| Ok(prior.unwrap_or("account-b").into()))
                .unwrap(),
            "account-a"
        );
        let conn = crate::persistence::open_sqlite(root.path()).unwrap();
        let (stored_key, raw): (String,String) = conn.query_row(
            "SELECT payload_key,payload_json FROM ctox_payload_store WHERE payload_key LIKE 'cliproxyapi.auth.session.v1:%'", [], |r| Ok((r.get(0)?,r.get(1)?)),
        ).unwrap();
        assert_eq!(stored_key, format!("{PREFIX}{key}"));
        assert_eq!(raw, "\"account-a\"");
        assert!(!stored_key.contains("private-session-label"));
    }

    #[test]
    fn provider_session_first_selection_is_serialized_between_independent_hosts() {
        let root = tempfile::tempdir().unwrap();
        crate::persistence::open_sqlite(root.path()).unwrap();
        let key = account_session_key("codex", None, "concurrent-session").unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let threads = ["account-a", "account-b"].map(|fallback| {
            let root = root.path().to_path_buf();
            let key = key.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let store = CtoxAccountSessionStore::new(root);
                barrier.wait();
                store
                    .choose(&key, &mut |prior| Ok(prior.unwrap_or(fallback).into()))
                    .unwrap()
            })
        });
        let [a, b] = threads.map(|thread| thread.join().unwrap());
        assert_eq!(a, b);
    }

    #[test]
    fn provider_session_failed_selection_does_not_create_or_replace_a_binding() {
        let root = tempfile::tempdir().unwrap();
        let store = CtoxAccountSessionStore::new(root.path());
        let key = account_session_key("antigravity", None, "session").unwrap();
        let fail = || AccountRoutingError::Selection(AccountSelectionError::Unavailable);
        assert_eq!(store.choose(&key, &mut |_| Err(fail())), Err(fail()));
        assert_eq!(
            store
                .choose(&key, &mut |prior| {
                    assert!(prior.is_none());
                    Ok("account-a".into())
                })
                .unwrap(),
            "account-a"
        );
        assert_eq!(store.choose(&key, &mut |_| Err(fail())), Err(fail()));
        assert_eq!(
            store
                .choose(&key, &mut |prior| Ok(prior.unwrap().into()))
                .unwrap(),
            "account-a"
        );
    }
}
