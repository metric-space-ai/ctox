// ref: sdk/cliproxy/auth/conductor_scheduler_refresh_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::json;

use super::{
    register_refresh_lead_provider, AccountSelectionError, Auth, AuthError, AuthLifecycle,
    AuthLifecycleRefreshError, AuthManager, AuthMutationOptions, AuthRefresher, AuthScheduler,
    AuthSchedulerView, AuthStatus, AuthStore, AuthStoreError, ModelResumeSink, ModelState,
    ProviderExecutorRegistry, QuotaState, RefreshExecutorError, RefreshSchedule,
    RefreshTransactionError, SchedulerCapabilities, SchedulerCapabilitySource,
    SchedulerPickOptions, SchedulerStrategy, SchedulerViewError,
};

fn at(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .expect("test timestamp")
        .with_timezone(&Utc)
}

#[derive(Default)]
struct SchedulerRefreshStore(Mutex<BTreeMap<String, Auth>>);

impl AuthStore for SchedulerRefreshStore {
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

#[derive(Default)]
struct ResumeRecorder(Mutex<Vec<(String, String)>>);

impl ModelResumeSink for ResumeRecorder {
    fn resume_model(&self, auth_id: &str, model: &str) {
        self.0
            .lock()
            .expect("resume recorder")
            .push((auth_id.to_owned(), model.to_owned()));
    }
}

#[derive(Default)]
struct PublicationRecorder(Mutex<Vec<String>>);

impl ModelResumeSink for PublicationRecorder {
    fn auth_published(&self, auth_id: &str) {
        self.0.lock().expect("published auths").push(auth_id.into());
    }

    fn resume_model(&self, _auth_id: &str, _model: &str) {}
}

#[derive(Default)]
struct MutableSchedulerCapabilities(Mutex<BTreeMap<String, SchedulerCapabilities>>);

impl MutableSchedulerCapabilities {
    fn set(&self, auth_id: &str, capabilities: SchedulerCapabilities) {
        self.0
            .lock()
            .expect("scheduler capabilities")
            .insert(auth_id.to_owned(), capabilities);
    }
}

impl SchedulerCapabilitySource for MutableSchedulerCapabilities {
    fn capabilities_for(&self, auth_id: &str, _provider: &str) -> Option<SchedulerCapabilities> {
        self.0
            .lock()
            .expect("scheduler capabilities")
            .get(auth_id)
            .cloned()
    }
}

struct SuccessfulRefresher;

impl AuthRefresher for SuccessfulRefresher {
    fn refresh(&self, auth: &mut Auth) -> Result<Option<Auth>, RefreshExecutorError> {
        auth.metadata.insert("access_token".into(), json!("fresh"));
        Ok(None)
    }
}

struct FailedRefresher(AuthError);

impl AuthRefresher for FailedRefresher {
    fn refresh(&self, _auth: &mut Auth) -> Result<Option<Auth>, RefreshExecutorError> {
        Err(RefreshExecutorError::Failed(self.0.clone()))
    }
}

fn oauth(id: &str, provider: &str) -> Auth {
    let mut auth = Auth::default();
    auth.id = id.into();
    auth.provider = provider.into();
    auth.status = AuthStatus::Active;
    auth.metadata.insert("access_token".into(), json!("stale"));
    auth.metadata
        .insert("refresh_token".into(), json!("refresh"));
    auth
}

#[derive(Clone, Copy)]
enum CancelledRefreshOutcome {
    Before,
    Success,
    Unauthorized,
}

struct CancellingRefresher {
    cancellation: super::RefreshCancellation,
    outcome: CancelledRefreshOutcome,
    calls: Arc<std::sync::atomic::AtomicUsize>,
}
impl AuthRefresher for CancellingRefresher {
    fn refresh(&self, auth: &mut Auth) -> Result<Option<Auth>, RefreshExecutorError> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        auth.metadata
            .insert("access_token".into(), json!("must-not-publish"));
        self.cancellation.cancel();
        match self.outcome {
            CancelledRefreshOutcome::Unauthorized => Err(RefreshExecutorError::Failed(AuthError {
                code: "unauthorized".into(),
                message: "test-only rejection".into(),
                retryable: false,
                http_status: 401,
            })),
            _ => Ok(None),
        }
    }
}
fn assert_cancelled_refresh_is_unpublished(outcome: CancelledRefreshOutcome) {
    let provider = match outcome {
        CancelledRefreshOutcome::Before => "worker-candidate-cancel-before",
        CancelledRefreshOutcome::Success => "worker-candidate-cancel-success",
        CancelledRefreshOutcome::Unauthorized => "worker-candidate-cancel-error",
    };
    register_refresh_lead_provider(provider, || Some(Duration::from_secs(600)));
    let now = at("2026-10-04T12:00:00Z");
    let store = Arc::new(SchedulerRefreshStore::default());
    let schedule = Arc::new(RefreshSchedule::default());
    let lifecycle = AuthLifecycle::new(store.clone(), schedule.clone(), Duration::from_secs(1));
    lifecycle
        .register(
            oauth(provider, provider),
            AuthMutationOptions::default(),
            now,
        )
        .unwrap();
    let before = lifecycle.get_cached(provider).unwrap();
    let before_schedule = schedule.peek();
    let cancellation = super::RefreshCancellation::default();
    if matches!(outcome, CancelledRefreshOutcome::Before) {
        cancellation.cancel();
    }
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let publication = PublicationRecorder::default();
    let result = lifecycle.refresh_with_cancellation(
        provider,
        None,
        now,
        &CancellingRefresher {
            cancellation: cancellation.clone(),
            outcome,
            calls: calls.clone(),
        },
        &publication,
        &cancellation,
    );
    assert!(matches!(
        result,
        Err(AuthLifecycleRefreshError::Refresh(
            RefreshTransactionError::Cancelled
        ))
    ));
    let cached = lifecycle.get_cached(provider).unwrap();
    let persisted = store.0.lock().unwrap().get(provider).unwrap().clone();
    for record in [cached, persisted] {
        assert_eq!(record.metadata, before.metadata);
        assert_eq!(record.status, before.status);
        assert_eq!(record.last_error, before.last_error);
        assert_eq!(record.last_refreshed_at, before.last_refreshed_at);
    }
    assert_eq!(schedule.peek(), before_schedule);
    assert!(publication.0.lock().unwrap().is_empty());
    assert_eq!(
        calls.load(std::sync::atomic::Ordering::SeqCst),
        if matches!(outcome, CancelledRefreshOutcome::Before) {
            0
        } else {
            1
        }
    );
}
#[test]
fn candidate_refresh_cancellation_before_attempt_skips_provider_and_publication() {
    assert_cancelled_refresh_is_unpublished(CancelledRefreshOutcome::Before);
}
#[test]
fn candidate_refresh_cancellation_after_success_does_not_publish_new_credentials() {
    assert_cancelled_refresh_is_unpublished(CancelledRefreshOutcome::Success);
}
#[test]
fn candidate_refresh_cancellation_after_failure_does_not_publish_auth_rejection() {
    assert_cancelled_refresh_is_unpublished(CancelledRefreshOutcome::Unauthorized);
}

#[test]
fn successful_refresh_republishes_cache_schedule_and_model_resumption() {
    register_refresh_lead_provider("worker-18bl-success", || Some(Duration::from_secs(10 * 60)));
    let now = at("2026-08-03T12:00:00Z");
    let store = Arc::new(SchedulerRefreshStore::default());
    let schedule = Arc::new(RefreshSchedule::default());
    let lifecycle = AuthLifecycle::new(store, schedule.clone(), Duration::from_secs(1));
    let mut auth = oauth("success", "worker-18bl-success");
    auth.model_states.insert(
        "model-a".into(),
        ModelState {
            status: AuthStatus::Error,
            unavailable: true,
            last_error: Some(AuthError {
                code: "unauthorized".into(),
                message: "expired".into(),
                retryable: false,
                http_status: 401,
            }),
            quota: QuotaState {
                exceeded: true,
                ..QuotaState::default()
            },
            ..ModelState::default()
        },
    );
    lifecycle
        .register(auth, AuthMutationOptions::default(), now)
        .expect("register");
    let recorder = ResumeRecorder::default();
    let outcome = lifecycle
        .refresh(
            "success",
            Some("stale"),
            now,
            &SuccessfulRefresher,
            &recorder,
        )
        .expect("refresh");

    assert_eq!(outcome.resumed_models, vec!["model-a"]);
    assert_eq!(
        super::access_token(&lifecycle.get_cached("success").expect("cache")),
        Some("fresh")
    );
    assert_eq!(schedule.len(), 1);
    assert_eq!(schedule.peek(), Some(now + chrono::Duration::minutes(10)));
    assert_eq!(
        *recorder.0.lock().expect("recorder"),
        vec![("success".into(), "model-a".into())]
    );
}

#[test]
fn unauthorized_refresh_failure_republishes_terminal_state_and_unschedules() {
    register_refresh_lead_provider("worker-18bl-unauthorized", || {
        Some(Duration::from_secs(10 * 60))
    });
    let now = at("2026-08-03T12:00:00Z");
    let store = Arc::new(SchedulerRefreshStore::default());
    let schedule = Arc::new(RefreshSchedule::default());
    let lifecycle = AuthLifecycle::new(store, schedule.clone(), Duration::from_secs(1));
    lifecycle
        .register(
            oauth("unauthorized", "worker-18bl-unauthorized"),
            AuthMutationOptions::default(),
            now,
        )
        .expect("register");
    assert_eq!(schedule.len(), 1);

    assert!(matches!(
        lifecycle.refresh(
            "unauthorized",
            None,
            now,
            &FailedRefresher(AuthError {
                message: "invalid grant".into(),
                http_status: 401,
                ..AuthError::default()
            }),
            &ResumeRecorder::default(),
        ),
        Err(AuthLifecycleRefreshError::Refresh(
            RefreshTransactionError::Refresh(AuthError {
                http_status: 401,
                ..
            })
        ))
    ));
    let cached = lifecycle.get_cached("unauthorized").expect("cache");
    assert_eq!(cached.status, AuthStatus::Error);
    assert!(cached.unavailable);
    assert!(schedule.is_empty());
}

#[test]
fn ordinary_refresh_failure_republishes_five_minute_retry_schedule() {
    register_refresh_lead_provider("worker-18bl-retry", || Some(Duration::from_secs(10 * 60)));
    let now = at("2026-08-03T12:00:00Z");
    let store = Arc::new(SchedulerRefreshStore::default());
    let schedule = Arc::new(RefreshSchedule::default());
    let lifecycle = AuthLifecycle::new(store, schedule.clone(), Duration::from_secs(1));
    lifecycle
        .register(
            oauth("retry", "worker-18bl-retry"),
            AuthMutationOptions::default(),
            now,
        )
        .expect("register");

    assert!(lifecycle
        .refresh(
            "retry",
            None,
            now,
            &FailedRefresher(AuthError {
                message: "temporary".into(),
                http_status: 503,
                ..AuthError::default()
            }),
            &ResumeRecorder::default(),
        )
        .is_err());
    assert_eq!(schedule.peek(), Some(now + chrono::Duration::minutes(5)));
    assert_eq!(
        lifecycle
            .get_cached("retry")
            .expect("cache")
            .next_refresh_after,
        now + chrono::Duration::minutes(5)
    );
}

#[test]
fn scheduler_entry_becomes_pickable_only_after_model_capability_refresh() {
    let now = at("2026-08-03T12:00:00Z");
    let store = Arc::new(SchedulerRefreshStore::default());
    let lifecycle = Arc::new(AuthLifecycle::new(
        store,
        Arc::new(RefreshSchedule::default()),
        Duration::from_secs(1),
    ));
    lifecycle
        .register(
            oauth("refresh-entry", "GeMiNi"),
            AuthMutationOptions::default(),
            now,
        )
        .expect("register");
    let capabilities = Arc::new(MutableSchedulerCapabilities::default());
    let view = AuthSchedulerView::new(lifecycle, capabilities.clone());
    let scheduler = AuthScheduler::new(SchedulerStrategy::RoundRobin);

    assert!(!view.refresh_entry("refresh-entry").expect("empty refresh"));
    assert_eq!(
        scheduler.pick_single(
            "gemini",
            Some("scheduler-refresh-model"),
            0,
            &view.snapshot(),
            &[],
            &SchedulerPickOptions::default(),
        ),
        Err(AccountSelectionError::NotFound)
    );

    capabilities.set(
        "refresh-entry",
        SchedulerCapabilities {
            priority: 4,
            weight: 3,
            websocket_enabled: true,
            supported_models: vec![
                "scheduler-refresh-model(high)".into(),
                "scheduler-refresh-model".into(),
            ],
        },
    );
    assert!(view.refresh_entry("refresh-entry").expect("model refresh"));
    let candidates = view.snapshot();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].provider, "gemini");
    assert_eq!(candidates[0].supported_models, ["scheduler-refresh-model"]);
    assert_eq!(
        scheduler
            .pick_single(
                "gemini",
                Some("scheduler-refresh-model(low)"),
                0,
                &candidates,
                &[],
                &SchedulerPickOptions::default(),
            )
            .expect("pick")
            .auth_id,
        "refresh-entry"
    );
}

#[test]
fn scheduler_full_rebuild_is_atomic_on_invalid_capability() {
    let now = at("2026-08-03T12:00:00Z");
    let lifecycle = Arc::new(AuthLifecycle::new(
        Arc::new(SchedulerRefreshStore::default()),
        Arc::new(RefreshSchedule::default()),
        Duration::from_secs(1),
    ));
    for id in ["a", "b"] {
        lifecycle
            .register(oauth(id, "gemini"), AuthMutationOptions::default(), now)
            .expect("register");
    }
    let capabilities = Arc::new(MutableSchedulerCapabilities::default());
    for id in ["a", "b"] {
        capabilities.set(
            id,
            SchedulerCapabilities {
                weight: 1,
                supported_models: vec!["model".into()],
                ..SchedulerCapabilities::default()
            },
        );
    }
    let view = AuthSchedulerView::new(lifecycle, capabilities.clone());
    assert_eq!(view.refresh_all(), Ok(2));
    let before = view.snapshot();

    capabilities.set(
        "b",
        SchedulerCapabilities {
            weight: 1_000_001,
            supported_models: vec!["model".into()],
            ..SchedulerCapabilities::default()
        },
    );
    assert_eq!(view.refresh_all(), Err(SchedulerViewError::InvalidWeight));
    assert_eq!(view.snapshot(), before);
}

#[test]
fn disabled_or_removed_auth_is_pruned_by_entry_refresh() {
    let now = at("2026-08-03T12:00:00Z");
    let lifecycle = Arc::new(AuthLifecycle::new(
        Arc::new(SchedulerRefreshStore::default()),
        Arc::new(RefreshSchedule::default()),
        Duration::from_secs(1),
    ));
    lifecycle
        .register(
            oauth("pruned", "gemini"),
            AuthMutationOptions::default(),
            now,
        )
        .expect("register");
    let capabilities = Arc::new(MutableSchedulerCapabilities::default());
    capabilities.set(
        "pruned",
        SchedulerCapabilities {
            weight: 1,
            supported_models: vec!["model".into()],
            ..SchedulerCapabilities::default()
        },
    );
    let view = AuthSchedulerView::new(lifecycle.clone(), capabilities);
    assert!(view.refresh_entry("pruned").expect("initial refresh"));

    let mut disabled = lifecycle.get_cached("pruned").expect("cached auth");
    disabled.disabled = true;
    disabled.status = AuthStatus::Disabled;
    lifecycle
        .update(disabled, AuthMutationOptions::default(), now)
        .expect("disable");
    assert!(!view.refresh_entry("pruned").expect("disabled refresh"));
    assert!(view.is_empty());

    assert!(lifecycle.remove_runtime("pruned"));
    assert!(!view.refresh_entry("pruned").expect("removed refresh"));
}

#[test]
fn durable_refresh_failure_republishes_manager_routing_view_after_unlock() {
    let now = at("2026-08-03T12:00:00Z");
    let lifecycle = Arc::new(AuthLifecycle::new(
        Arc::new(SchedulerRefreshStore::default()),
        Arc::new(RefreshSchedule::default()),
        Duration::from_secs(1),
    ));
    let capabilities = Arc::new(MutableSchedulerCapabilities::default());
    capabilities.set(
        "manager-refresh-failure",
        SchedulerCapabilities {
            weight: 1,
            supported_models: vec!["model".into()],
            ..SchedulerCapabilities::default()
        },
    );
    let view = Arc::new(AuthSchedulerView::new(lifecycle.clone(), capabilities));
    let manager = Arc::new(AuthManager::new(
        lifecycle,
        Arc::new(ProviderExecutorRegistry::default()),
        view,
    ));
    manager
        .register(
            oauth("manager-refresh-failure", "gemini"),
            AuthMutationOptions::default(),
            now,
        )
        .expect("register");
    assert_eq!(manager.candidates().len(), 1);
    let recorder = Arc::new(PublicationRecorder::default());
    let downstream: Arc<dyn ModelResumeSink> = recorder.clone();
    let sink = manager.refresh_publication_sink(downstream);

    assert!(manager
        .lifecycle()
        .refresh(
            "manager-refresh-failure",
            None,
            now,
            &FailedRefresher(AuthError {
                message: "invalid grant".into(),
                http_status: 401,
                ..AuthError::default()
            }),
            sink.as_ref(),
        )
        .is_err());

    assert!(manager.candidates().is_empty());
    assert_eq!(
        *recorder.0.lock().expect("published auths"),
        vec!["manager-refresh-failure".to_owned()]
    );
    assert_eq!(sink.publication_failures(), 0);
}
