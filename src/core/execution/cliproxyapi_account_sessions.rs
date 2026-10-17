// Origin: CTOX; License: AGPL-3.0-only
//! Private, non-secret bindings in the existing CTOX SQLite payload store.
use ctox_cliproxyapi::sdk::cliproxy::auth::conductor_execution::AccountRoutingError;
use ctox_cliproxyapi::sdk::cliproxy::auth::{AccountSessionStore, AccountSessionStoreError};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::PathBuf;

const PREFIX: &str = "cliproxyapi.auth.session.v1:";
const MAX_BINDINGS: i64 = 100_000;
/// Compare-and-set phases before a continuously changing binding is reported.
const MAX_SELECTION_PHASES: usize = 8;

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
/// Domain errors raised inside an owner phase travel through `anyhow` so the
/// owner rolls the phase back; owner/admission failures map to `fallback`.
fn owner_error(
    fallback: AccountSessionStoreError,
) -> impl FnOnce(anyhow::Error) -> AccountRoutingError {
    move |failure| {
        failure
            .downcast_ref::<AccountRoutingError>()
            .copied()
            .unwrap_or(error(fallback))
    }
}
/// Result of one compare-and-set phase under the Core owner writer.
enum Select {
    Committed,
    Changed(Option<String>),
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
        let path = crate::paths::core_db(&self.root);
        let read_key = key.clone();
        let mut previous = crate::lcm::with_lcm_read(&path, move |conn| Ok(read(conn, &read_key)?))
            .map_err(owner_error(AccountSessionStoreError::Read))?;
        // The chooser performs no database I/O and cannot enter an owner job,
        // so it runs outside the writer. The owner phase commits its choice
        // only if the binding it was shown is still current (compare-and-set);
        // otherwise the chooser sees the newer binding, as the former in-tx
        // second call did. Concurrent first selections therefore converge.
        for _ in 0..MAX_SELECTION_PHASES {
            let selected = choose(previous.as_deref())?;
            // Healthy continuations need no writer transaction and no TTL-based
            // reassignment. The chooser has still checked current eligibility.
            if previous.as_deref() == Some(selected.as_str()) {
                return Ok(selected);
            }
            if !ctox_cliproxyapi::internal::api::account_selection::valid_account_id(&selected) {
                return Err(error(AccountSessionStoreError::InvalidRecord));
            }
            let value = serde_json::to_string(&selected)
                .map_err(|_| error(AccountSessionStoreError::InvalidRecord))?;
            let (write_key, expected) = (key.clone(), previous.clone());
            // Core owner phase `cliproxyapi.session.select`.
            let outcome = crate::lcm::with_lcm_write(&path, move |conn| {
                let current = read(conn, &write_key)?;
                if current != expected {
                    return Ok(Select::Changed(current));
                }
                if current.is_none() {
                    let count: i64 = conn.query_row(
                        "SELECT COUNT(*) FROM ctox_payload_store WHERE payload_key LIKE 'cliproxyapi.auth.session.v1:%'",
                        [], |row| row.get(0),
                    ).map_err(|_| error(AccountSessionStoreError::Read))?;
                    // Fail visibly rather than evicting a healthy live session.
                    if count >= MAX_BINDINGS {
                        return Err(error(AccountSessionStoreError::Capacity).into());
                    }
                }
                conn.execute(
                    "INSERT INTO ctox_payload_store(payload_key,payload_json,updated_at) VALUES (?1,?2,unixepoch()) ON CONFLICT(payload_key) DO UPDATE SET payload_json=excluded.payload_json,updated_at=excluded.updated_at",
                    params![write_key, value],
                ).map_err(|_| error(AccountSessionStoreError::Write))?;
                Ok(Select::Committed)
            })
            .map_err(owner_error(AccountSessionStoreError::Write))?;
            match outcome {
                Select::Committed => return Ok(selected),
                Select::Changed(current) => previous = current,
            }
        }
        // Bounded: a binding that keeps changing under every phase is reported,
        // never overwritten blindly.
        Err(error(AccountSessionStoreError::Write))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctox_cliproxyapi::sdk::cliproxy::auth::{account_session_key, AccountSelectionError};

    /// Same fixture as the host tests: the CLI composition registers Core,
    /// Runtime and Secret owners for an isolated root and drains them after.
    fn with_owned_test_root(operation: impl FnOnce(&std::path::Path) + Send + 'static) {
        let root = std::sync::Arc::new(tempfile::tempdir().unwrap());
        let work = std::sync::Arc::clone(&root);
        crate::persistence::run_cli_owned(root.path(), move || {
            operation(work.path());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn provider_session_binding_survives_router_restart_and_contains_no_raw_session() {
        with_owned_test_root(|root| {
            let key = account_session_key("claude", None, "private-session-label").unwrap();
            let first = CtoxAccountSessionStore::new(root);
            assert_eq!(
                first
                    .choose(&key, &mut |prior| Ok(prior.unwrap_or("account-a").into()))
                    .unwrap(),
                "account-a"
            );
            drop(first);
            let restored = CtoxAccountSessionStore::new(root);
            assert_eq!(
                restored
                    .choose(&key, &mut |prior| Ok(prior.unwrap_or("account-b").into()))
                    .unwrap(),
                "account-a"
            );
            let (stored_key, raw): (String, String) =
                crate::lcm::with_lcm_read(&crate::paths::core_db(root), |conn| {
                    Ok(conn.query_row(
                        "SELECT payload_key,payload_json FROM ctox_payload_store WHERE payload_key LIKE 'cliproxyapi.auth.session.v1:%'", [], |r| Ok((r.get(0)?,r.get(1)?)),
                    )?)
                })
                .unwrap();
            assert_eq!(stored_key, format!("{PREFIX}{key}"));
            assert_eq!(raw, "\"account-a\"");
            assert!(!stored_key.contains("private-session-label"));
        });
    }

    #[test]
    fn provider_session_first_selection_is_serialized_between_independent_hosts() {
        with_owned_test_root(|root| {
            let key = account_session_key("codex", None, "concurrent-session").unwrap();
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
            let threads =
                ["account-a", "account-b"].map(|fallback| {
                    let root = root.to_path_buf();
                    let key = key.clone();
                    let barrier = barrier.clone();
                    std::thread::spawn(move || {
                        let store = CtoxAccountSessionStore::new(root.clone());
                        barrier.wait();
                        // Each independent host thread is admitted by the same
                        // registered Core owner, never by its own opener.
                        crate::persistence::run_core_blocking_at(&root, move || {
                            Ok(store
                                .choose(&key, &mut |prior| Ok(prior.unwrap_or(fallback).into()))?)
                        })
                        .unwrap()
                    })
                });
            let [a, b] = threads.map(|thread| thread.join().unwrap());
            assert_eq!(a, b);
        });
    }

    #[test]
    fn provider_session_failed_selection_does_not_create_or_replace_a_binding() {
        with_owned_test_root(|root| {
            let store = CtoxAccountSessionStore::new(root);
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
        });
    }
}
