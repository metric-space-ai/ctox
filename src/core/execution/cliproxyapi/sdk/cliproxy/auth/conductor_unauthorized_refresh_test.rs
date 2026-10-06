// ref: sdk/cliproxy/auth/conductor_unauthorized_refresh_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};

use chrono::{DateTime, Utc};
use serde_json::json;

use super::{
    register_refresh_lead_provider, Auth, AuthError, AuthRefresher, AuthStatus, AuthStore,
    AuthStoreError, ModelState, QuotaState, RefreshCoordinator, RefreshExecutorError,
    RefreshTransactionError,
};

fn at(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .expect("test timestamp")
        .with_timezone(&Utc)
}

#[derive(Default)]
struct MemoryAuthStore {
    records: Mutex<BTreeMap<String, Auth>>,
    saves: AtomicUsize,
    fail_save: AtomicBool,
}

impl MemoryAuthStore {
    fn with(auth: Auth) -> Self {
        Self {
            records: Mutex::new([(auth.id.clone(), auth)].into()),
            ..Self::default()
        }
    }

    fn get(&self, id: &str) -> Auth {
        self.records
            .lock()
            .expect("memory store")
            .get(id)
            .expect("stored auth")
            .clone()
    }
}

impl AuthStore for MemoryAuthStore {
    fn list(&self) -> Result<Vec<Auth>, AuthStoreError> {
        Ok(self
            .records
            .lock()
            .map_err(|_| AuthStoreError::Read)?
            .values()
            .cloned()
            .collect())
    }

    fn save(&self, auth: &Auth) -> Result<String, AuthStoreError> {
        self.saves.fetch_add(1, Ordering::SeqCst);
        if self.fail_save.load(Ordering::SeqCst) {
            return Err(AuthStoreError::Write);
        }
        self.records
            .lock()
            .map_err(|_| AuthStoreError::Write)?
            .insert(auth.id.clone(), auth.clone());
        Ok(format!("ctox-auth://{}", auth.id))
    }

    fn delete(&self, id: &str) -> Result<(), AuthStoreError> {
        self.records
            .lock()
            .map_err(|_| AuthStoreError::Delete)?
            .remove(id);
        Ok(())
    }
}

struct RotatingRefresher {
    calls: AtomicUsize,
    next_token: &'static str,
}

impl AuthRefresher for RotatingRefresher {
    fn refresh(&self, auth: &mut Auth) -> Result<Option<Auth>, RefreshExecutorError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        auth.metadata
            .insert("access_token".into(), json!(self.next_token));
        Ok(None)
    }
}

struct FailedRefresher(RefreshExecutorError);

impl AuthRefresher for FailedRefresher {
    fn refresh(&self, _auth: &mut Auth) -> Result<Option<Auth>, RefreshExecutorError> {
        Err(self.0.clone())
    }
}

fn oauth(id: &str) -> Auth {
    let mut auth = Auth::default();
    auth.id = id.into();
    auth.provider = "codex".into();
    auth.metadata
        .insert("access_token".into(), json!("stale-access-token"));
    auth.metadata
        .insert("refresh_token".into(), json!("refresh-token"));
    auth
}

#[test]
fn concurrent_stale_unauthorized_calls_refresh_once_and_coalesce() {
    let store = Arc::new(MemoryAuthStore::with(oauth("account")));
    let coordinator = Arc::new(RefreshCoordinator::new(store.clone()));
    let refresher = Arc::new(RotatingRefresher {
        calls: AtomicUsize::new(0),
        next_token: "fresh-access-token",
    });
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let coordinator = coordinator.clone();
        let refresher = refresher.clone();
        let barrier = barrier.clone();
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            coordinator
                .refresh(
                    "account",
                    Some("stale-access-token"),
                    at("2026-08-03T12:00:00Z"),
                    refresher.as_ref(),
                )
                .expect("refresh transaction")
        }));
    }
    barrier.wait();
    let outcomes = workers
        .into_iter()
        .map(|worker| worker.join().expect("worker"))
        .collect::<Vec<_>>();

    assert_eq!(refresher.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        outcomes.iter().filter(|outcome| outcome.coalesced).count(),
        1
    );
    assert_eq!(store.saves.load(Ordering::SeqCst), 1);
    assert_eq!(
        super::access_token(&store.get("account")),
        Some("fresh-access-token")
    );
}

#[test]
fn unauthorized_without_refresh_credential_never_calls_refresher() {
    let mut auth = oauth("static");
    auth.metadata.remove("refresh_token");
    let store = Arc::new(MemoryAuthStore::with(auth));
    let coordinator = RefreshCoordinator::new(store);
    let refresher = RotatingRefresher {
        calls: AtomicUsize::new(0),
        next_token: "must-not-run",
    };

    assert!(matches!(
        coordinator.refresh(
            "static",
            Some("stale-access-token"),
            at("2026-08-03T12:00:00Z"),
            &refresher,
        ),
        Err(RefreshTransactionError::NotRefreshable)
    ));
    assert_eq!(refresher.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn successful_refresh_normalizes_auth_and_recovers_unauthorized_model_state() {
    let now = at("2026-08-03T12:00:00Z");
    let mut auth = oauth("recover");
    auth.status = AuthStatus::Error;
    auth.status_message = "old error".into();
    auth.unavailable = true;
    auth.last_error = Some(AuthError {
        code: "unauthorized".into(),
        message: "old".into(),
        retryable: false,
        http_status: 401,
    });
    auth.model_states.insert(
        "gpt-5".into(),
        ModelState {
            status: AuthStatus::Error,
            status_message: "unauthorized".into(),
            unavailable: true,
            next_retry_after: now + chrono::Duration::minutes(30),
            last_error: Some(AuthError {
                code: "unauthorized".into(),
                message: "old model error".into(),
                retryable: false,
                http_status: 401,
            }),
            quota: QuotaState {
                exceeded: true,
                reason: "quota".into(),
                next_recover_at: now + chrono::Duration::minutes(30),
                backoff_level: 2,
                ..QuotaState::default()
            },
            updated_at: now - chrono::Duration::minutes(1),
        },
    );
    let store = Arc::new(MemoryAuthStore::with(auth));
    let coordinator = RefreshCoordinator::new(store.clone());
    let outcome = coordinator
        .refresh(
            "recover",
            Some("stale-access-token"),
            now,
            &RotatingRefresher {
                calls: AtomicUsize::new(0),
                next_token: "fresh",
            },
        )
        .expect("successful refresh");

    assert_eq!(outcome.resumed_models, vec!["gpt-5"]);
    assert!(!outcome.ineffective);
    assert_eq!(outcome.auth.status, AuthStatus::Active);
    assert!(!outcome.auth.unavailable);
    assert!(outcome.auth.last_error.is_none());
    assert_eq!(outcome.auth.last_refreshed_at, now);
    let model = &outcome.auth.model_states["gpt-5"];
    assert_eq!(model.status, AuthStatus::Active);
    assert!(!model.unavailable);
    assert!(model.last_error.is_none());
    assert!(!model.quota.exceeded);
    assert_eq!(store.get("recover").last_refreshed_at, now);
}

#[test]
fn unchanged_expired_credential_receives_ineffective_backoff() {
    let now = at("2026-08-03T12:00:00Z");
    register_refresh_lead_provider("worker-18bj-ineffective", || {
        Some(std::time::Duration::from_secs(600))
    });
    let mut auth = oauth("ineffective");
    auth.provider = "worker-18bj-ineffective".into();
    auth.metadata.insert(
        "expires_at".into(),
        json!((now - chrono::Duration::minutes(1)).to_rfc3339()),
    );
    let store = Arc::new(MemoryAuthStore::with(auth));
    let coordinator = RefreshCoordinator::new(store.clone());
    let outcome = coordinator
        .refresh(
            "ineffective",
            None,
            now,
            &RotatingRefresher {
                calls: AtomicUsize::new(0),
                next_token: "fresh",
            },
        )
        .expect("successful but ineffective refresh");
    assert!(outcome.ineffective);
    assert_eq!(
        outcome.auth.next_refresh_after,
        now + chrono::Duration::seconds(30)
    );
}

#[test]
fn refresh_failures_are_durable_and_join_store_failure_evidence() {
    let now = at("2026-08-03T12:00:00Z");
    let store = Arc::new(MemoryAuthStore::with(oauth("failure")));
    let coordinator = RefreshCoordinator::new(store.clone());
    let unauthorized = AuthError {
        code: String::new(),
        message: "refresh token invalid".into(),
        retryable: true,
        http_status: 401,
    };
    assert!(matches!(
        coordinator.refresh(
            "failure",
            None,
            now,
            &FailedRefresher(RefreshExecutorError::Failed(unauthorized)),
        ),
        Err(RefreshTransactionError::Refresh(AuthError {
            code,
            http_status: 401,
            retryable: false,
            ..
        })) if code == "unauthorized"
    ));
    let persisted = store.get("failure");
    assert_eq!(persisted.status, AuthStatus::Error);
    assert!(persisted.unavailable);
    assert_eq!(persisted.updated_at, now);
    assert_eq!(
        persisted.last_error.expect("persisted failure").http_status,
        401
    );

    store.fail_save.store(true, Ordering::SeqCst);
    assert!(matches!(
        coordinator.refresh(
            "failure",
            None,
            now + chrono::Duration::minutes(1),
            &FailedRefresher(RefreshExecutorError::Failed(AuthError {
                message: "transport".into(),
                ..AuthError::default()
            })),
        ),
        Err(RefreshTransactionError::RefreshAndStore {
            store: AuthStoreError::Write,
            ..
        })
    ));
}

#[test]
fn non_unauthorized_failure_receives_five_minute_backoff() {
    let now = at("2026-08-03T12:00:00Z");
    let store = Arc::new(MemoryAuthStore::with(oauth("backoff")));
    let coordinator = RefreshCoordinator::new(store.clone());
    assert!(matches!(
        coordinator.refresh(
            "backoff",
            None,
            now,
            &FailedRefresher(RefreshExecutorError::Failed(AuthError {
                code: "temporarily_unavailable".into(),
                message: "provider unavailable".into(),
                retryable: true,
                http_status: 503,
            })),
        ),
        Err(RefreshTransactionError::Refresh(AuthError {
            http_status: 503,
            retryable: false,
            ..
        }))
    ));
    let persisted = store.get("backoff");
    assert_eq!(
        persisted.next_refresh_after,
        now + chrono::Duration::minutes(5)
    );
    assert!(!persisted.unavailable);
    assert_ne!(persisted.status, AuthStatus::Error);
}

struct IdentityChangingRefresher;

impl AuthRefresher for IdentityChangingRefresher {
    fn refresh(&self, auth: &mut Auth) -> Result<Option<Auth>, RefreshExecutorError> {
        auth.id = "different-account".into();
        Ok(None)
    }
}

#[test]
fn refreshed_identity_cannot_replace_another_auth_record() {
    let store = Arc::new(MemoryAuthStore::with(oauth("stable-account")));
    let coordinator = RefreshCoordinator::new(store.clone());
    assert!(matches!(
        coordinator.refresh(
            "stable-account",
            None,
            at("2026-08-03T12:00:00Z"),
            &IdentityChangingRefresher,
        ),
        Err(RefreshTransactionError::InvalidRefreshedIdentity)
    ));
    assert_eq!(store.saves.load(Ordering::SeqCst), 0);
    assert_eq!(store.get("stable-account").id, "stable-account");
}

#[test]
fn cancellation_never_persists_a_transition() {
    let store = Arc::new(MemoryAuthStore::with(oauth("cancel")));
    let coordinator = RefreshCoordinator::new(store.clone());
    assert!(matches!(
        coordinator.refresh(
            "cancel",
            None,
            at("2026-08-03T12:00:00Z"),
            &FailedRefresher(RefreshExecutorError::Cancelled),
        ),
        Err(RefreshTransactionError::Cancelled)
    ));
    assert_eq!(store.saves.load(Ordering::SeqCst), 0);
}
