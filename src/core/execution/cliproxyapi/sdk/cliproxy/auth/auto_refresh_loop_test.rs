// ref: sdk/cliproxy/auth/auto_refresh_loop_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::json;

use super::{
    next_refresh_check_at, parse_duration, register_refresh_lead_provider, should_refresh, Auth,
    AuthError, AuthLifecycle, AuthMutationOptions, AuthRefresher, AuthRefresherResolver,
    AuthStatus, AuthStore, AuthStoreError, AutoRefreshClock, AutoRefreshConfig, AutoRefreshWorker,
    ModelResumeSink, RefreshCancellation, RefreshExecutorError, RefreshLeadRuntime,
    RefreshSchedule,
};

fn at(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .expect("test timestamp")
        .with_timezone(&Utc)
}

fn add(timestamp: DateTime<Utc>, seconds: i64) -> DateTime<Utc> {
    timestamp + chrono::Duration::seconds(seconds)
}

struct EvaluatingRuntime(bool);

impl RefreshLeadRuntime for EvaluatingRuntime {
    fn evaluates_refresh(&self) -> bool {
        true
    }

    fn should_refresh(&self, _now: DateTime<Utc>, _auth: &Auth) -> bool {
        self.0
    }
}

#[test]
fn disabled_oauth_stays_scheduled_at_provider_expiry_lead() {
    let now = at("2026-04-12T00:00:00Z");
    let expiry = add(now, 3_600);
    register_refresh_lead_provider("worker-18bi-disabled", || Some(Duration::from_secs(600)));
    let mut auth = Auth::default();
    auth.id = "a1".into();
    auth.provider = "worker-18bi-disabled".into();
    auth.disabled = true;
    auth.status = AuthStatus::Disabled;
    auth.metadata.insert("email".into(), json!("x@example.com"));
    auth.metadata
        .insert("expires_at".into(), json!(expiry.to_rfc3339()));

    assert_eq!(
        next_refresh_check_at(now, &auth, Duration::from_secs(900)),
        Some(add(expiry, -600))
    );
}

#[test]
fn api_key_is_unscheduled_and_next_refresh_after_gates_oauth() {
    let now = at("2026-04-12T00:00:00Z");
    let mut api_key = Auth::default();
    api_key.attributes.insert("api_key".into(), "k".into());
    assert_eq!(
        next_refresh_check_at(now, &api_key, Duration::from_secs(900)),
        None
    );

    let next = add(now, 1_800);
    let mut oauth = Auth::default();
    oauth
        .metadata
        .insert("email".into(), json!("x@example.com"));
    oauth.next_refresh_after = next;
    assert_eq!(
        next_refresh_check_at(now, &oauth, Duration::from_secs(900)),
        Some(next)
    );
    assert!(!should_refresh(&oauth, now));
}

#[test]
fn preferred_interval_picks_earliest_expiry_or_last_refresh_candidate() {
    let now = at("2026-04-12T00:00:00Z");
    let expiry = add(now, 1_200);
    let mut auth = Auth::default();
    auth.last_refreshed_at = now;
    auth.metadata.insert("email".into(), json!("x@example.com"));
    auth.metadata
        .insert("expires_at".into(), json!(expiry.to_rfc3339()));
    auth.metadata
        .insert("refresh_interval_seconds".into(), json!(900));

    assert_eq!(
        next_refresh_check_at(now, &auth, Duration::from_secs(900)),
        Some(add(expiry, -900))
    );
    assert!(!should_refresh(&auth, now));
    assert!(should_refresh(&auth, add(now, 301)));
}

#[test]
fn provider_lead_and_evaluator_match_pinned_schedule_precedence() {
    let now = at("2026-04-12T00:00:00Z");
    let expiry = add(now, 3_600);
    register_refresh_lead_provider("worker-18bi-provider", || Some(Duration::from_secs(600)));
    let mut auth = Auth::default();
    auth.provider = "worker-18bi-provider".into();
    auth.metadata.insert("email".into(), json!("x@example.com"));
    auth.metadata
        .insert("expires_at".into(), json!(expiry.to_rfc3339()));
    assert_eq!(
        next_refresh_check_at(now, &auth, Duration::from_secs(900)),
        Some(add(expiry, -600))
    );
    assert!(!should_refresh(&auth, add(expiry, -601)));
    assert!(should_refresh(&auth, add(expiry, -600)));

    auth.runtime = Some(Arc::new(EvaluatingRuntime(false)));
    assert_eq!(
        next_refresh_check_at(now, &auth, Duration::from_secs(900)),
        Some(add(now, 900))
    );
    assert!(!should_refresh(&auth, add(expiry, 1)));
    auth.runtime = Some(Arc::new(EvaluatingRuntime(true)));
    assert!(should_refresh(&auth, now));
}

#[test]
fn unauthorized_failure_unschedules_and_refresh_tokens_accept_both_aliases() {
    let now = at("2026-04-12T00:00:00Z");
    let mut auth = Auth::default();
    auth.metadata
        .insert("refreshToken".into(), json!(" refresh "));
    auth.metadata
        .insert("accessToken".into(), json!(" access "));
    auth.last_error = Some(AuthError {
        code: "UNAUTHORIZED".into(),
        message: "safe".into(),
        retryable: false,
        http_status: 0,
    });
    assert!(super::has_refresh_credential(&auth));
    assert_eq!(super::access_token(&auth), Some("access"));
    assert_eq!(
        next_refresh_check_at(now, &auth, Duration::from_secs(900)),
        None
    );
    assert!(!should_refresh(&auth, now));
}

#[test]
fn duration_parser_covers_go_units_sequences_fractions_and_numeric_seconds() {
    for (input, nanos) in [
        ("1ns", 1),
        ("1us", 1_000),
        ("1µs", 1_000),
        ("1μs", 1_000),
        ("1.5ms", 1_500_000),
        ("0.000000001999999999999999999s", 2),
        ("1m30.25s", 90_250_000_000),
        ("+2h", 7_200_000_000_000),
        ("0.25", 250_000_000),
        ("1e100", i64::MAX as u128),
    ] {
        assert_eq!(
            parse_duration(input).map(|duration| duration.as_nanos()),
            Some(nanos),
            "{input}"
        );
    }
    for input in ["", "0", "-1s", "1d", "nan"] {
        assert_eq!(parse_duration(input), None, "{input}");
    }
}

#[test]
fn refresh_schedule_updates_removes_and_pops_equal_deadlines_deterministically() {
    let now = at("2026-04-12T00:00:00Z");
    let schedule = RefreshSchedule::default();
    assert!(schedule.upsert("b", add(now, 20)));
    assert!(schedule.upsert("a", add(now, 20)));
    assert!(schedule.upsert("later", add(now, 30)));
    assert!(schedule.upsert("later", add(now, 10)));
    assert_eq!(schedule.peek(), Some(add(now, 10)));
    assert_eq!(schedule.pop_due(add(now, 20)), vec!["later", "a", "b"]);
    assert_eq!(schedule.len(), 0);
    assert!(!schedule.remove("missing"));
}

#[derive(Default)]
struct WorkerStore(Mutex<BTreeMap<String, Auth>>);

impl AuthStore for WorkerStore {
    fn list(&self) -> Result<Vec<Auth>, AuthStoreError> {
        Ok(self
            .0
            .lock()
            .map_err(|_| AuthStoreError::Read)?
            .values()
            .cloned()
            .collect())
    }

    fn save(&self, auth: &Auth) -> Result<String, AuthStoreError> {
        self.0
            .lock()
            .map_err(|_| AuthStoreError::Write)?
            .insert(auth.id.clone(), auth.clone());
        Ok(auth.id.clone())
    }

    fn delete(&self, id: &str) -> Result<(), AuthStoreError> {
        self.0
            .lock()
            .map_err(|_| AuthStoreError::Delete)?
            .remove(id);
        Ok(())
    }
}

struct FixedWorkerClock(DateTime<Utc>);

impl AutoRefreshClock for FixedWorkerClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

#[derive(Default)]
struct WorkerResumeSink;

impl ModelResumeSink for WorkerResumeSink {
    fn resume_model(&self, _auth_id: &str, _model: &str) {}
}

#[derive(Default)]
struct WorkerResolver(Mutex<BTreeMap<String, Arc<dyn AuthRefresher>>>);

impl WorkerResolver {
    fn insert(&self, provider: &str, refresher: Arc<dyn AuthRefresher>) {
        self.0
            .lock()
            .expect("resolver")
            .insert(provider.into(), refresher);
    }
}

impl AuthRefresherResolver for WorkerResolver {
    fn resolve(&self, provider: &str) -> Option<Arc<dyn AuthRefresher>> {
        self.0.lock().ok()?.get(provider).cloned()
    }
}

struct ConcurrencyRefresher {
    calls: AtomicUsize,
    active: AtomicUsize,
    maximum: AtomicUsize,
}

struct CancelledRefresher;

impl AuthRefresher for CancelledRefresher {
    fn refresh(&self, _auth: &mut Auth) -> Result<Option<Auth>, RefreshExecutorError> {
        Err(RefreshExecutorError::Cancelled)
    }
}

#[derive(Default)]
struct CancellationAwareRefresher {
    started: AtomicBool,
    observed: AtomicBool,
    calls: AtomicUsize,
}

impl AuthRefresher for CancellationAwareRefresher {
    fn refresh(&self, _auth: &mut Auth) -> Result<Option<Auth>, RefreshExecutorError> {
        panic!("worker must use the cancellation-aware refresh entry point")
    }

    fn refresh_with_cancellation(
        &self,
        _auth: &mut Auth,
        cancellation: &RefreshCancellation,
    ) -> Result<Option<Auth>, RefreshExecutorError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.started.store(true, Ordering::SeqCst);
        while !cancellation.is_cancelled() {
            std::thread::sleep(Duration::from_millis(2));
        }
        self.observed.store(true, Ordering::SeqCst);
        Err(RefreshExecutorError::Cancelled)
    }
}

impl ConcurrencyRefresher {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            maximum: AtomicUsize::new(0),
        }
    }
}

impl AuthRefresher for ConcurrencyRefresher {
    fn refresh(&self, auth: &mut Auth) -> Result<Option<Auth>, RefreshExecutorError> {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.maximum.fetch_max(active, Ordering::SeqCst);
        self.calls.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(35));
        auth.metadata
            .insert("access_token".into(), json!("rotated"));
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(None)
    }
}

fn worker_auth(id: &str, provider: &str) -> Auth {
    let mut auth = Auth::default();
    auth.id = id.into();
    auth.provider = provider.into();
    auth.status = AuthStatus::Active;
    auth.metadata.insert("access_token".into(), json!("old"));
    auth.metadata
        .insert("refresh_token".into(), json!("refresh"));
    auth
}

async fn wait_for(predicate: impl Fn() -> bool) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while !predicate() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("worker condition timeout");
}

#[tokio::test]
async fn worker_wakes_for_late_schedule_and_requeues_missing_provider() {
    let now = at("2026-08-03T12:00:00Z");
    register_refresh_lead_provider("worker-18bm-missing", || Some(Duration::from_secs(600)));
    let store = Arc::new(WorkerStore::default());
    let schedule = Arc::new(RefreshSchedule::default());
    let lifecycle = Arc::new(AuthLifecycle::new(
        store,
        schedule.clone(),
        Duration::from_secs(42),
    ));
    let worker = AutoRefreshWorker::spawn(
        lifecycle.clone(),
        schedule.clone(),
        Arc::new(WorkerResolver::default()),
        Arc::new(WorkerResumeSink),
        Arc::new(FixedWorkerClock(now)),
        AutoRefreshConfig {
            interval: Duration::from_secs(42),
            concurrency: 1,
            job_buffer: 1,
        },
    );
    assert!(schedule.is_empty());
    lifecycle
        .register(
            worker_auth("late", "worker-18bm-missing"),
            AuthMutationOptions::default(),
            now,
        )
        .expect("register");
    wait_for(|| schedule.peek() == Some(add(now, 42))).await;
    worker.stop().await;
}

#[tokio::test]
async fn worker_never_exceeds_configured_refresh_concurrency() {
    let now = at("2026-08-03T12:00:00Z");
    register_refresh_lead_provider("worker-18bm-concurrency", || Some(Duration::from_secs(600)));
    let store = Arc::new(WorkerStore::default());
    let schedule = Arc::new(RefreshSchedule::default());
    let lifecycle = Arc::new(AuthLifecycle::new(
        store,
        schedule.clone(),
        Duration::from_secs(5),
    ));
    for id in ["a", "b", "c", "d"] {
        lifecycle
            .register(
                worker_auth(id, "worker-18bm-concurrency"),
                AuthMutationOptions::default(),
                now,
            )
            .expect("register");
    }
    let refresher = Arc::new(ConcurrencyRefresher::new());
    let resolver = Arc::new(WorkerResolver::default());
    resolver.insert("worker-18bm-concurrency", refresher.clone());
    let worker = AutoRefreshWorker::spawn(
        lifecycle,
        schedule,
        resolver,
        Arc::new(WorkerResumeSink),
        Arc::new(FixedWorkerClock(now)),
        AutoRefreshConfig {
            interval: Duration::from_secs(5),
            concurrency: 2,
            job_buffer: 2,
        },
    );
    wait_for(|| refresher.calls.load(Ordering::SeqCst) == 4).await;
    worker.stop().await;
    assert_eq!(refresher.maximum.load(Ordering::SeqCst), 2);
    assert_eq!(refresher.active.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn worker_coalesces_same_auth_wakeup_while_refresh_is_pending() {
    let now = at("2026-08-03T12:00:00Z");
    register_refresh_lead_provider("worker-18bm-pending-coalesce", || {
        Some(Duration::from_secs(600))
    });
    let store = Arc::new(WorkerStore::default());
    let schedule = Arc::new(RefreshSchedule::default());
    let lifecycle = Arc::new(AuthLifecycle::new(
        store,
        schedule.clone(),
        Duration::from_secs(5),
    ));
    lifecycle
        .register(
            worker_auth("single", "worker-18bm-pending-coalesce"),
            AuthMutationOptions::default(),
            now,
        )
        .expect("register");
    let refresher = Arc::new(ConcurrencyRefresher::new());
    let resolver = Arc::new(WorkerResolver::default());
    resolver.insert("worker-18bm-pending-coalesce", refresher.clone());
    let worker = AutoRefreshWorker::spawn(
        lifecycle,
        schedule.clone(),
        resolver,
        Arc::new(WorkerResumeSink),
        Arc::new(FixedWorkerClock(now)),
        AutoRefreshConfig {
            interval: Duration::from_secs(5),
            concurrency: 1,
            job_buffer: 1,
        },
    );
    wait_for(|| refresher.active.load(Ordering::SeqCst) == 1).await;
    schedule.upsert("single", now);
    wait_for(|| {
        refresher.active.load(Ordering::SeqCst) == 0 && schedule.peek() == Some(add(now, 600))
    })
    .await;
    tokio::time::sleep(Duration::from_millis(25)).await;
    worker.stop().await;
    assert_eq!(refresher.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn worker_shutdown_cancels_future_timer_without_refresh() {
    let now = at("2026-08-03T12:00:00Z");
    register_refresh_lead_provider("worker-18bm-stop", || Some(Duration::from_secs(600)));
    let store = Arc::new(WorkerStore::default());
    let schedule = Arc::new(RefreshSchedule::default());
    let lifecycle = Arc::new(AuthLifecycle::new(
        store,
        schedule.clone(),
        Duration::from_secs(5),
    ));
    let mut auth = worker_auth("future", "worker-18bm-stop");
    auth.last_refreshed_at = now;
    lifecycle
        .register(auth, AuthMutationOptions::default(), now)
        .expect("register");
    let refresher = Arc::new(ConcurrencyRefresher::new());
    let resolver = Arc::new(WorkerResolver::default());
    resolver.insert("worker-18bm-stop", refresher.clone());
    let worker = AutoRefreshWorker::spawn(
        lifecycle,
        schedule,
        resolver,
        Arc::new(WorkerResumeSink),
        Arc::new(FixedWorkerClock(now)),
        AutoRefreshConfig::default(),
    );
    tokio::task::yield_now().await;
    worker.stop().await;
    assert_eq!(refresher.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn worker_shutdown_requeues_every_popped_but_unfinished_auth() {
    let now = at("2026-08-03T12:00:00Z");
    register_refresh_lead_provider("worker-18bm-stop-requeue", || {
        Some(Duration::from_secs(600))
    });
    let store = Arc::new(WorkerStore::default());
    let schedule = Arc::new(RefreshSchedule::default());
    let lifecycle = Arc::new(AuthLifecycle::new(
        store.clone(),
        schedule.clone(),
        Duration::from_secs(5),
    ));
    for id in ["a", "b", "c", "d"] {
        lifecycle
            .register(
                worker_auth(id, "worker-18bm-stop-requeue"),
                AuthMutationOptions::default(),
                now,
            )
            .expect("register");
    }
    let initial_auth = store.list().expect("initial accounts");
    let refresher = Arc::new(CancellationAwareRefresher::default());
    let resolver = Arc::new(WorkerResolver::default());
    resolver.insert("worker-18bm-stop-requeue", refresher.clone());
    let worker = AutoRefreshWorker::spawn(
        lifecycle,
        schedule.clone(),
        resolver,
        Arc::new(WorkerResumeSink),
        Arc::new(FixedWorkerClock(now)),
        AutoRefreshConfig {
            interval: Duration::from_secs(5),
            concurrency: 1,
            job_buffer: 1,
        },
    );
    wait_for(|| refresher.started.load(Ordering::SeqCst)).await;
    worker.stop().await;
    assert_eq!(refresher.calls.load(Ordering::SeqCst), 1);
    assert!(refresher.observed.load(Ordering::SeqCst));
    assert_eq!(schedule.pop_due(add(now, 60)), vec!["a", "b", "c", "d"]);
    assert_eq!(schedule.peek(), None);
    let stored = store.list().expect("stored accounts");
    assert_eq!(stored.len(), 4);
    for auth in stored {
        assert_eq!(auth.metadata.get("access_token"), Some(&json!("old")));
        let initial = initial_auth
            .iter()
            .find(|initial| initial.id == auth.id)
            .expect("registered account");
        assert_eq!(auth.last_refreshed_at, initial.last_refreshed_at);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn worker_shutdown_preserves_completed_refresh_and_unstarted_auth() {
    let now = at("2026-08-03T12:00:00Z");
    register_refresh_lead_provider("worker-18bm-stop-completed", || {
        Some(Duration::from_secs(600))
    });
    let store = Arc::new(WorkerStore::default());
    let schedule = Arc::new(RefreshSchedule::default());
    let lifecycle = Arc::new(AuthLifecycle::new(
        store.clone(),
        schedule.clone(),
        Duration::from_secs(5),
    ));
    lifecycle
        .register(
            worker_auth("a", "worker-18bm-stop-completed"),
            AuthMutationOptions::default(),
            now,
        )
        .expect("register");
    let refresher = Arc::new(ConcurrencyRefresher::new());
    let resolver = Arc::new(WorkerResolver::default());
    resolver.insert("worker-18bm-stop-completed", refresher.clone());
    let worker = AutoRefreshWorker::spawn(
        lifecycle.clone(),
        schedule.clone(),
        resolver,
        Arc::new(WorkerResumeSink),
        Arc::new(FixedWorkerClock(now)),
        AutoRefreshConfig {
            interval: Duration::from_secs(5),
            concurrency: 1,
            job_buffer: 1,
        },
    );
    wait_for(|| {
        store.list().expect("stored accounts").iter().any(|auth| {
            auth.id == "a"
                && auth.last_refreshed_at == now
                && auth.metadata.get("access_token") == Some(&json!("rotated"))
        }) && schedule.peek() == Some(add(now, 600))
    })
    .await;
    // No async yield between registering the remaining work and stop's
    // cancellation signal: only the already committed refresh may survive.
    for id in ["b", "c", "d"] {
        lifecycle
            .register(
                worker_auth(id, "worker-18bm-stop-completed"),
                AuthMutationOptions::default(),
                now,
            )
            .expect("register");
    }
    let initial_auth = store.list().expect("initial accounts");
    worker.stop().await;
    assert_eq!(refresher.calls.load(Ordering::SeqCst), 1);
    assert_eq!(schedule.pop_due(add(now, 60)), vec!["b", "c", "d"]);
    assert_eq!(schedule.peek(), Some(add(now, 600)));
    let stored = store.list().expect("stored accounts");
    assert_eq!(stored.len(), 4);
    for auth in stored {
        if auth.id == "a" {
            assert_eq!(auth.metadata.get("access_token"), Some(&json!("rotated")));
            assert_eq!(auth.last_refreshed_at, now);
        } else {
            assert_eq!(auth.metadata.get("access_token"), Some(&json!("old")));
            let initial = initial_auth
                .iter()
                .find(|initial| initial.id == auth.id)
                .expect("registered account");
            assert_eq!(auth.last_refreshed_at, initial.last_refreshed_at);
        }
    }
}

#[tokio::test]
async fn worker_shutdown_propagates_cancellation_into_inflight_refresher() {
    let now = at("2026-08-03T12:00:00Z");
    register_refresh_lead_provider("worker-18bm-cancel-signal", || {
        Some(Duration::from_secs(600))
    });
    let store = Arc::new(WorkerStore::default());
    let schedule = Arc::new(RefreshSchedule::default());
    let lifecycle = Arc::new(AuthLifecycle::new(
        store,
        schedule.clone(),
        Duration::from_secs(5),
    ));
    lifecycle
        .register(
            worker_auth("inflight", "worker-18bm-cancel-signal"),
            AuthMutationOptions::default(),
            now,
        )
        .expect("register");
    let refresher = Arc::new(CancellationAwareRefresher::default());
    let resolver = Arc::new(WorkerResolver::default());
    resolver.insert("worker-18bm-cancel-signal", refresher.clone());
    let worker = AutoRefreshWorker::spawn(
        lifecycle,
        schedule,
        resolver,
        Arc::new(WorkerResumeSink),
        Arc::new(FixedWorkerClock(now)),
        AutoRefreshConfig {
            interval: Duration::from_secs(5),
            concurrency: 1,
            job_buffer: 1,
        },
    );
    wait_for(|| refresher.started.load(Ordering::SeqCst)).await;
    worker.stop().await;
    assert!(refresher.observed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn worker_applies_pending_backoff_after_cancelled_attempt() {
    let now = at("2026-08-03T12:00:00Z");
    register_refresh_lead_provider("worker-18bm-pending", || Some(Duration::from_secs(600)));
    let store = Arc::new(WorkerStore::default());
    let schedule = Arc::new(RefreshSchedule::default());
    let lifecycle = Arc::new(AuthLifecycle::new(
        store,
        schedule.clone(),
        Duration::from_secs(5),
    ));
    lifecycle
        .register(
            worker_auth("cancelled", "worker-18bm-pending"),
            AuthMutationOptions::default(),
            now,
        )
        .expect("register");
    let resolver = Arc::new(WorkerResolver::default());
    resolver.insert("worker-18bm-pending", Arc::new(CancelledRefresher));
    let worker = AutoRefreshWorker::spawn(
        lifecycle,
        schedule.clone(),
        resolver,
        Arc::new(WorkerResumeSink),
        Arc::new(FixedWorkerClock(now)),
        AutoRefreshConfig {
            interval: Duration::from_secs(5),
            concurrency: 1,
            job_buffer: 1,
        },
    );
    wait_for(|| schedule.peek() == Some(add(now, 60))).await;
    worker.stop().await;
}

#[test]
fn worker_config_normalizes_upstream_defaults_and_buffer_bound() {
    assert_eq!(
        AutoRefreshConfig {
            interval: Duration::ZERO,
            concurrency: 0,
            job_buffer: 0,
        }
        .normalized(),
        AutoRefreshConfig::default()
    );
    assert_eq!(
        AutoRefreshConfig {
            interval: Duration::from_secs(9),
            concurrency: 3,
            job_buffer: 1,
        }
        .normalized()
        .job_buffer,
        3
    );
}
