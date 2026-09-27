// Provider account state of a scrape target.
//
// When a provider refuses the paying account behind a stored credential
// (Bright Data "HTTP 400: Customer is not active": 77 of 80 LinkedIn runs in
// one production incident), every further call returns the same refusal. The state
// is kept per target so that
// - calls are suppressed without contacting the provider and without
//   inventing a run; the answer names the run that caused the state,
// - exactly one probe runs when the credential changed, when an authorized
//   operator asks for it, or when the backoff (6 h, then 24 h) is due,
// - concurrent triggers get one probe through an atomic lease,
// - a success clears the state only for the generation it probed, so a late
//   result cannot erase a newer state.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};

const FIRST_BACKOFF_MS: i64 = 6 * 60 * 60 * 1000;
const LATER_BACKOFF_MS: i64 = 24 * 60 * 60 * 1000;
const PROBE_LEASE_MS: i64 = 30 * 60 * 1000;
/// A probe that failed for another reason (network, timeout) keeps the state
/// and tries again after this pause instead of hammering the provider.
const RETRY_AFTER_OTHER_FAILURE_MS: i64 = 60 * 60 * 1000;

/// Provider texts that mean "the account is not active", in any failure mode
/// an adapter used to report them.
pub(super) fn is_provider_account_inactive(detail: &str) -> bool {
    let text = detail.to_ascii_lowercase();
    [
        "customer is not active",
        "account is not active",
        "account is inactive",
        "account inactive",
        "account is suspended",
        "account suspended",
        "account has been suspended",
        "account is disabled",
        "subscription expired",
        "subscription is inactive",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

/// The stored credential a target depends on, identified by name and the
/// version (updated_at) of its secret record, `absent` without a record.
/// `version` is `None` when the secret store could not be read: that is not
/// a change and must not replace the version the state recorded. Never the
/// value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Credential {
    pub(super) reference: String,
    pub(super) version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AccountState {
    pub(super) target_id: String,
    pub(super) generation: i64,
    pub(super) reason: String,
    pub(super) causal_run_id: String,
    pub(super) last_probe_run_id: String,
    pub(super) credential_ref: Option<String>,
    pub(super) credential_version: Option<String>,
    pub(super) failed_probes: i64,
    pub(super) next_probe_at_ms: i64,
    pub(super) probe_lease_owner: Option<String>,
    pub(super) probe_lease_until_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Admission {
    /// Run the adapter. `probe_generation` is set when the run probes an
    /// existing state under the lease for that generation.
    Run { probe_generation: Option<i64> },
    /// Do not contact the provider; answer with the stored state.
    Suppress(AccountState),
}

pub(super) fn load(conn: &Connection, target_id: &str) -> Result<Option<AccountState>> {
    Ok(conn
        .query_row(
            "SELECT target_id, generation, reason, causal_run_id, last_probe_run_id,
                    credential_ref, credential_version, failed_probes, next_probe_at_ms,
                    probe_lease_owner, probe_lease_until_ms
             FROM scrape_account_state WHERE target_id = ?1",
            params![target_id],
            |row| {
                Ok(AccountState {
                    target_id: row.get(0)?,
                    generation: row.get(1)?,
                    reason: row.get(2)?,
                    causal_run_id: row.get(3)?,
                    last_probe_run_id: row.get(4)?,
                    credential_ref: row.get(5)?,
                    credential_version: row.get(6)?,
                    failed_probes: row.get(7)?,
                    next_probe_at_ms: row.get(8)?,
                    probe_lease_owner: row.get(9)?,
                    probe_lease_until_ms: row.get(10)?,
                })
            },
        )
        .optional()?)
}

fn credential_changed(state: &AccountState, credential: Option<&Credential>) -> bool {
    match credential {
        Some(credential) => {
            state.credential_ref.as_deref() != Some(credential.reference.as_str())
                || credential
                    .version
                    .as_deref()
                    .is_some_and(|version| state.credential_version.as_deref() != Some(version))
        }
        None => false,
    }
}

/// Decides whether a run may contact the provider. With a stored state, a
/// probe needs a reason (credential changed since the LAST probe, authorized
/// operator request, due backoff) AND the atomic lease for the current
/// generation. Taking the lease records the credential version the probe
/// uses, so a failed probe consumes that version.
pub(super) fn admit(
    conn: &Connection,
    target_id: &str,
    credential: Option<&Credential>,
    authorized_probe: bool,
    now_ms: i64,
    run_id: &str,
) -> Result<Admission> {
    // Read and lease in one IMMEDIATE transaction, so the decision and the
    // lease are taken on the same state and a concurrent writer waits on
    // busy_timeout. (Hardening; no failure of the former separate read and
    // write was observed.)
    conn.execute_batch("BEGIN IMMEDIATE")?;
    match admit_in_transaction(
        conn,
        target_id,
        credential,
        authorized_probe,
        now_ms,
        run_id,
    ) {
        Ok(admission) => {
            conn.execute_batch("COMMIT")?;
            Ok(admission)
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn admit_in_transaction(
    conn: &Connection,
    target_id: &str,
    credential: Option<&Credential>,
    authorized_probe: bool,
    now_ms: i64,
    run_id: &str,
) -> Result<Admission> {
    let Some(state) = load(conn, target_id)? else {
        return Ok(Admission::Run {
            probe_generation: None,
        });
    };
    let reason_to_probe = credential_changed(&state, credential)
        || authorized_probe
        || now_ms >= state.next_probe_at_ms;
    if !reason_to_probe {
        return Ok(Admission::Suppress(state));
    }
    let taken = conn.execute(
        "UPDATE scrape_account_state
         SET probe_lease_owner = ?1,
             probe_lease_until_ms = ?2,
             credential_ref = COALESCE(?3, credential_ref),
             credential_version = COALESCE(?4, credential_version),
             updated_at_ms = ?5
         WHERE target_id = ?6
           AND generation = ?7
           AND (probe_lease_owner IS NULL OR probe_lease_until_ms < ?5)",
        params![
            run_id,
            now_ms + PROBE_LEASE_MS,
            credential.map(|c| c.reference.as_str()),
            credential.and_then(|c| c.version.as_deref()),
            now_ms,
            target_id,
            state.generation,
        ],
    )?;
    if taken == 1 {
        Ok(Admission::Run {
            probe_generation: Some(state.generation),
        })
    } else {
        Ok(Admission::Suppress(load(conn, target_id)?.unwrap_or(state)))
    }
}

/// How a run ended, as far as the account state is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RunResult<'a> {
    AccountInactive(&'a str),
    Succeeded,
    OtherFailure,
}

fn backoff_after(failed_probes: i64) -> i64 {
    if failed_probes <= 1 {
        FIRST_BACKOFF_MS
    } else {
        LATER_BACKOFF_MS
    }
}

/// Books a finished run. Only the run that holds the lease of the generation
/// it was admitted with may change an existing state; anything else is a late
/// or foreign result and leaves the newer state alone.
pub(super) fn record(
    conn: &Connection,
    target_id: &str,
    run_id: &str,
    probe_generation: Option<i64>,
    result: RunResult<'_>,
    credential: Option<&Credential>,
    now_ms: i64,
) -> Result<()> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    match record_in_transaction(
        conn,
        target_id,
        run_id,
        probe_generation,
        result,
        credential,
        now_ms,
    ) {
        Ok(()) => {
            conn.execute_batch("COMMIT")?;
            Ok(())
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn record_in_transaction(
    conn: &Connection,
    target_id: &str,
    run_id: &str,
    probe_generation: Option<i64>,
    result: RunResult<'_>,
    credential: Option<&Credential>,
    now_ms: i64,
) -> Result<()> {
    match (result, probe_generation) {
        (RunResult::AccountInactive(reason), None) => {
            // First detection. A concurrent run may have inserted meanwhile;
            // then its state stands.
            conn.execute(
                "INSERT OR IGNORE INTO scrape_account_state
                   (target_id, generation, reason, causal_run_id, last_probe_run_id,
                    credential_ref, credential_version, failed_probes, next_probe_at_ms,
                    probe_lease_owner, probe_lease_until_ms, updated_at_ms)
                 VALUES (?1, 1, ?2, ?3, ?3, ?4, ?5, 1, ?6, NULL, 0, ?7)",
                params![
                    target_id,
                    reason,
                    run_id,
                    credential.map(|c| c.reference.as_str()),
                    credential.and_then(|c| c.version.as_deref()),
                    now_ms + FIRST_BACKOFF_MS,
                    now_ms,
                ],
            )?;
        }
        (RunResult::AccountInactive(reason), Some(generation)) => {
            let failed = load(conn, target_id)?
                .map(|state| state.failed_probes + 1)
                .unwrap_or(1);
            conn.execute(
                "UPDATE scrape_account_state
                 SET generation = generation + 1,
                     reason = ?1,
                     last_probe_run_id = ?2,
                     failed_probes = ?3,
                     next_probe_at_ms = ?4,
                     probe_lease_owner = NULL,
                     probe_lease_until_ms = 0,
                     updated_at_ms = ?5
                 WHERE target_id = ?6 AND generation = ?7 AND probe_lease_owner = ?2",
                params![
                    reason,
                    run_id,
                    failed,
                    now_ms + backoff_after(failed),
                    now_ms,
                    target_id,
                    generation,
                ],
            )?;
        }
        (RunResult::Succeeded, Some(generation)) => {
            conn.execute(
                "DELETE FROM scrape_account_state
                 WHERE target_id = ?1 AND generation = ?2 AND probe_lease_owner = ?3",
                params![target_id, generation, run_id],
            )?;
        }
        (RunResult::OtherFailure, Some(generation)) => {
            conn.execute(
                "UPDATE scrape_account_state
                 SET probe_lease_owner = NULL,
                     probe_lease_until_ms = 0,
                     next_probe_at_ms = ?1,
                     updated_at_ms = ?2
                 WHERE target_id = ?3 AND generation = ?4 AND probe_lease_owner = ?5",
                params![
                    now_ms + RETRY_AFTER_OTHER_FAILURE_MS,
                    now_ms,
                    target_id,
                    generation,
                    run_id,
                ],
            )?;
        }
        // No state at admission: a success or other failure leaves any state
        // a concurrent run may have created in the meantime untouched.
        (RunResult::Succeeded | RunResult::OtherFailure, None) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn);
        conn
    }

    fn create_schema(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE scrape_account_state (
                target_id TEXT PRIMARY KEY, generation INTEGER NOT NULL, reason TEXT NOT NULL,
                causal_run_id TEXT NOT NULL, last_probe_run_id TEXT NOT NULL,
                credential_ref TEXT, credential_version TEXT, failed_probes INTEGER NOT NULL,
                next_probe_at_ms INTEGER NOT NULL, probe_lease_owner TEXT,
                probe_lease_until_ms INTEGER NOT NULL DEFAULT 0, updated_at_ms INTEGER NOT NULL);",
        )
        .unwrap();
    }

    fn cred(version: &str) -> Credential {
        Credential {
            reference: "BRIGHTDATA_API_KEY".to_string(),
            version: Some(version.to_string()),
        }
    }

    const T: &str = "t-li";
    const H: i64 = 60 * 60 * 1000;

    fn inactive(conn: &Connection, run: &str, generation: Option<i64>, c: &Credential, now: i64) {
        record(
            conn,
            T,
            run,
            generation,
            RunResult::AccountInactive("Customer is not active"),
            Some(c),
            now,
        )
        .unwrap();
    }

    #[test]
    fn provider_texts_are_recognised_and_network_text_is_not() {
        assert!(is_provider_account_inactive(
            "Bright Data antwortete mit HTTP 400: Customer is not active"
        ));
        assert!(is_provider_account_inactive("Account has been suspended"));
        assert!(!is_provider_account_inactive("connection reset by peer"));
        assert!(!is_provider_account_inactive(
            "weder LinkedIn-Profil-URL noch Vor- und Nachname im Auftrag"
        ));
    }

    #[test]
    fn repeated_calls_are_suppressed_without_a_new_state_or_run() {
        let conn = db();
        let c = cred("v1");
        assert_eq!(
            admit(&conn, T, Some(&c), false, 0, "r1").unwrap(),
            Admission::Run {
                probe_generation: None
            }
        );
        inactive(&conn, "r1", None, &c, 0);
        for (index, now) in [1, H, 5 * H].into_iter().enumerate() {
            match admit(&conn, T, Some(&c), false, now, &format!("x{index}")).unwrap() {
                Admission::Suppress(state) => {
                    assert_eq!(state.causal_run_id, "r1");
                    assert_eq!(state.generation, 1);
                }
                other => panic!("expected suppression, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_credential_edit_allows_exactly_one_probe_and_a_failed_probe_consumes_the_version() {
        let conn = db();
        inactive(&conn, "r1", None, &cred("v1"), 0);
        let edited = cred("v2");
        assert_eq!(
            admit(&conn, T, Some(&edited), false, H, "p1").unwrap(),
            Admission::Run {
                probe_generation: Some(1)
            }
        );
        // A second trigger while the probe runs does not get through.
        assert!(matches!(
            admit(&conn, T, Some(&edited), false, H, "p2").unwrap(),
            Admission::Suppress(_)
        ));
        inactive(&conn, "p1", Some(1), &edited, H + 1);
        // Same (consumed) version: no new probe before the backoff.
        assert!(matches!(
            admit(&conn, T, Some(&edited), false, 2 * H, "p3").unwrap(),
            Admission::Suppress(_)
        ));
        let state = load(&conn, T).unwrap().unwrap();
        assert_eq!(state.generation, 2);
        assert_eq!(state.credential_version.as_deref(), Some("v2"));
        assert_eq!(state.last_probe_run_id, "p1");
        assert_eq!(state.causal_run_id, "r1");
    }

    #[test]
    fn concurrent_triggers_get_one_probe() {
        let conn = db();
        inactive(&conn, "r1", None, &cred("v1"), 0);
        let due = 7 * H;
        let first = admit(&conn, T, Some(&cred("v1")), false, due, "a").unwrap();
        let second = admit(&conn, T, Some(&cred("v1")), false, due, "b").unwrap();
        assert_eq!(
            first,
            Admission::Run {
                probe_generation: Some(1)
            }
        );
        assert!(matches!(second, Admission::Suppress(_)));
    }

    #[test]
    fn backoff_is_six_then_twenty_four_hours_and_an_authorized_probe_may_run_earlier() {
        let conn = db();
        let c = cred("v1");
        inactive(&conn, "r1", None, &c, 0);
        assert!(matches!(
            admit(&conn, T, Some(&c), false, 6 * H - 1, "e").unwrap(),
            Admission::Suppress(_)
        ));
        assert_eq!(
            admit(&conn, T, Some(&c), false, 6 * H, "p1").unwrap(),
            Admission::Run {
                probe_generation: Some(1)
            }
        );
        inactive(&conn, "p1", Some(1), &c, 6 * H);
        assert_eq!(
            load(&conn, T).unwrap().unwrap().next_probe_at_ms,
            6 * H + 24 * H
        );
        assert!(matches!(
            admit(&conn, T, Some(&c), false, 20 * H, "e2").unwrap(),
            Admission::Suppress(_)
        ));
        assert_eq!(
            admit(&conn, T, Some(&c), true, 20 * H, "op").unwrap(),
            Admission::Run {
                probe_generation: Some(2)
            }
        );
    }

    #[test]
    fn success_clears_only_its_own_generation_and_a_stale_result_keeps_the_newer_state() {
        let conn = db();
        let c = cred("v1");
        inactive(&conn, "r1", None, &c, 0);
        assert_eq!(
            admit(&conn, T, Some(&c), true, H, "p1").unwrap(),
            Admission::Run {
                probe_generation: Some(1)
            }
        );
        inactive(&conn, "p1", Some(1), &c, H + 1);
        // A late success reported for generation 1 by the same run id: stale.
        record(
            &conn,
            T,
            "p1",
            Some(1),
            RunResult::Succeeded,
            Some(&c),
            H + 2,
        )
        .unwrap();
        assert_eq!(load(&conn, T).unwrap().unwrap().generation, 2);
        // The current probe of generation 2 succeeds: state cleared.
        assert_eq!(
            admit(&conn, T, Some(&c), true, H + 3, "p2").unwrap(),
            Admission::Run {
                probe_generation: Some(2)
            }
        );
        record(
            &conn,
            T,
            "p2",
            Some(2),
            RunResult::Succeeded,
            Some(&c),
            H + 4,
        )
        .unwrap();
        assert!(load(&conn, T).unwrap().is_none());
    }

    #[test]
    fn a_probe_failing_for_another_reason_releases_the_lease_and_waits() {
        let conn = db();
        let c = cred("v1");
        inactive(&conn, "r1", None, &c, 0);
        assert_eq!(
            admit(&conn, T, Some(&c), true, H, "p1").unwrap(),
            Admission::Run {
                probe_generation: Some(1)
            }
        );
        record(
            &conn,
            T,
            "p1",
            Some(1),
            RunResult::OtherFailure,
            Some(&c),
            H,
        )
        .unwrap();
        let state = load(&conn, T).unwrap().unwrap();
        assert_eq!(state.generation, 1);
        assert!(state.probe_lease_owner.is_none());
        assert_eq!(state.next_probe_at_ms, H + RETRY_AFTER_OTHER_FAILURE_MS);
    }

    #[test]
    fn an_unreadable_secret_store_is_neither_a_change_nor_overwrites_the_version() {
        let conn = db();
        inactive(&conn, "r1", None, &cred("v1"), 0);
        let unreadable = Credential {
            reference: "BRIGHTDATA_API_KEY".to_string(),
            version: None,
        };
        assert!(matches!(
            admit(&conn, T, Some(&unreadable), false, H, "u1").unwrap(),
            Admission::Suppress(_)
        ));
        // An authorized probe with an unreadable store keeps the recorded version.
        assert_eq!(
            admit(&conn, T, Some(&unreadable), true, H, "op").unwrap(),
            Admission::Run {
                probe_generation: Some(1)
            }
        );
        inactive(&conn, "op", Some(1), &unreadable, H + 1);
        assert_eq!(
            load(&conn, T)
                .unwrap()
                .unwrap()
                .credential_version
                .as_deref(),
            Some("v1")
        );
        // "absent" is a real version: deleting the secret is a change.
        let absent = cred("absent");
        assert!(matches!(
            admit(&conn, T, Some(&absent), false, 2 * H, "d1").unwrap(),
            Admission::Run { .. }
        ));
    }

    struct FileDb(std::path::PathBuf);

    impl FileDb {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "ctox-account-state-{name}-{}-{}.sqlite3",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let conn = Connection::open(&path).unwrap();
            conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))
                .unwrap();
            create_schema(&conn);
            Self(path)
        }

        fn open(&self) -> Connection {
            let conn = Connection::open(&self.0).unwrap();
            conn.busy_timeout(std::time::Duration::from_secs(10))
                .unwrap();
            conn
        }
    }

    impl Drop for FileDb {
        fn drop(&mut self) {
            for suffix in ["", "-wal", "-shm"] {
                let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
            }
        }
    }

    #[test]
    fn concurrent_connections_racing_for_a_due_probe_get_exactly_one_lease() {
        let file = FileDb::new("race");
        inactive(&file.open(), "r1", None, &cred("v1"), 0);
        let due = 7 * H;
        for round in 0..20 {
            // Each round starts from a free lease on the same generation.
            file.open()
                .execute(
                    "UPDATE scrape_account_state SET probe_lease_owner = NULL, probe_lease_until_ms = 0",
                    [],
                )
                .unwrap();
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
            // Two connections: the host allows at most two test workers.
            let handles: Vec<_> = (0..2)
                .map(|worker| {
                    let barrier = barrier.clone();
                    let conn = file.open();
                    std::thread::spawn(move || {
                        barrier.wait();
                        admit(
                            &conn,
                            T,
                            Some(&cred("v1")),
                            false,
                            due,
                            &format!("w{round}-{worker}"),
                        )
                        .unwrap()
                    })
                })
                .collect();
            let results: Vec<Admission> = handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect();
            let runs = results
                .iter()
                .filter(|result| {
                    matches!(
                        result,
                        Admission::Run {
                            probe_generation: Some(1)
                        }
                    )
                })
                .count();
            assert_eq!(runs, 1, "round {round}: {results:?}");
        }
    }

    #[test]
    fn a_crashed_probe_holds_the_lease_until_it_expires_and_its_late_result_is_ignored() {
        let file = FileDb::new("crash");
        let conn = file.open();
        let c = cred("v1");
        inactive(&conn, "r1", None, &c, 0);
        let due = 7 * H;
        assert_eq!(
            admit(&conn, T, Some(&c), false, due, "crashed").unwrap(),
            Admission::Run {
                probe_generation: Some(1)
            }
        );
        // The holder dies without recording anything. Until the lease
        // expires nobody else probes.
        drop(conn);
        let other = file.open();
        assert!(matches!(
            admit(&other, T, Some(&c), false, due + PROBE_LEASE_MS - 1, "b1").unwrap(),
            Admission::Suppress(_)
        ));
        assert_eq!(
            admit(&other, T, Some(&c), false, due + PROBE_LEASE_MS + 1, "b2").unwrap(),
            Admission::Run {
                probe_generation: Some(1)
            }
        );
        // A late success from the crashed run must not clear the state that
        // the new lease holder is probing.
        record(
            &other,
            T,
            "crashed",
            Some(1),
            RunResult::Succeeded,
            Some(&c),
            due + PROBE_LEASE_MS + 2,
        )
        .unwrap();
        let state = load(&other, T).unwrap().expect("state kept");
        assert_eq!(state.probe_lease_owner.as_deref(), Some("b2"));
    }
}
