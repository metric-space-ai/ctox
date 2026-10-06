// Origin: CTOX actual asynchronous 401 replay/publication/owner-race guards.
// ref: sdk/cliproxy/auth/conductor_execution.go:1534-1594 @ d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only
use super::*;
use std::sync::atomic::AtomicBool;
use tokio::sync::Semaphore;

struct HeldRefresher {
    entered: Semaphore,
    release: Semaphore,
    calls: AtomicUsize,
    failure: AtomicBool,
}
impl HeldRefresher {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            entered: Semaphore::new(0),
            release: Semaphore::new(0),
            calls: AtomicUsize::new(0),
            failure: AtomicBool::new(false),
        })
    }
    async fn wait_until_entered(&self) {
        tokio::time::timeout(Duration::from_secs(5), self.entered.acquire())
            .await
            .unwrap()
            .unwrap()
            .forget();
    }
}
impl AsyncAuthRefresher for HeldRefresher {
    fn refresh<'a>(
        &'a self,
        auth: &'a Auth,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Auth, AuthPreparationError>> + Send + 'a>,
    > {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.entered.add_permits(1);
            self.release.acquire().await.unwrap().forget();
            if self.failure.load(Ordering::SeqCst) {
                return Err(
                    Arc::new(std::io::Error::other("fixture mint failed")) as AuthPreparationError
                );
            }
            let mut candidate = auth.clone();
            candidate
                .metadata
                .insert("access_token".into(), serde_json::json!("fresh"));
            Ok(candidate)
        })
    }
}
impl AuthPreparer for HeldRefresher {
    fn should_prepare(&self, auth: &Auth) -> bool {
        auth.metadata
            .get("access_token")
            .and_then(serde_json::Value::as_str)
            != Some("fresh")
    }
    fn prepare<'a>(
        &'a self,
        auth: &'a mut Auth,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), AuthPreparationError>> + Send + 'a>,
    > {
        Box::pin(async move {
            *auth = AsyncAuthRefresher::refresh(self, auth).await?;
            Ok(())
        })
    }
}
fn registration(
    executor: Arc<TestExecutor>,
    held: Arc<HeldRefresher>,
) -> Arc<ProviderExecutorRegistration> {
    Arc::new(
        ProviderExecutorRegistration::new("claude", executor.clone())
            .unwrap()
            .with_execution(executor.clone())
            .unwrap()
            .with_auth_preparer(executor)
            .with_async_auth_refresher(held),
    )
}
fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-08-04T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}
fn spawn_refresh(
    runtime: Arc<GenericAuthRuntime>,
    registration: Arc<ProviderExecutorRegistration>,
    auth: Auth,
) -> tokio::task::JoinHandle<Result<Option<Auth>, GenericExecutionError>> {
    tokio::spawn(async move {
        runtime
            .refresh_after_unauthorized(&auth, &registration)
            .await
    })
}

#[tokio::test]
async fn candidate_async_refresh_unary_and_stream_bootstrap_use_async_capability() {
    for stream in [false, true] {
        let (runtime, executor, manager, _) = runtime(
            if stream {
                Mode::StreamBootstrap401
            } else {
                Mode::RefreshUnary
            },
            &["auth-a"],
        );
        let held = HeldRefresher::new();
        held.release.add_permits(1);
        manager.register_executor(registration(executor.clone(), held.clone()));
        if stream {
            let mut response = runtime
                .execute_stream(&["claude".into()], request())
                .await
                .unwrap();
            let first = response.chunks.recv().await.unwrap();
            assert_eq!(first.payload, b"fresh-stream");
            assert!(first.error.is_none());
            assert!(response.chunks.recv().await.is_none());
        } else {
            assert_eq!(
                runtime
                    .execute(&["claude".into()], request())
                    .await
                    .unwrap()
                    .payload,
                b"auth-a"
            );
        }
        assert_eq!(held.calls.load(Ordering::SeqCst), 1);
        assert_eq!(executor.refreshes.load(Ordering::SeqCst), 0);
        assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            manager.lifecycle().get_cached("auth-a").unwrap().metadata["access_token"],
            "fresh"
        );
    }
}
#[tokio::test]
async fn candidate_async_refresh_committed_stream_never_replays() {
    let (runtime, executor, manager, _) = runtime(Mode::StreamTail401, &["auth-a"]);
    let held = HeldRefresher::new();
    manager.register_executor(registration(executor.clone(), held.clone()));
    let mut response = runtime
        .execute_stream(&["claude".into()], request())
        .await
        .unwrap();
    assert_eq!(response.chunks.recv().await.unwrap().payload, b"committed");
    assert!(response.chunks.recv().await.unwrap().error.is_some());
    assert!(response.chunks.recv().await.is_none());
    assert_eq!(held.calls.load(Ordering::SeqCst), 0);
    assert_eq!(executor.refreshes.load(Ordering::SeqCst), 0);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn candidate_async_refresh_concurrent_401s_reuse_accepted_key() {
    let (runtime, executor, manager, _) = runtime(Mode::Success, &["auth-a"]);
    let held = HeldRefresher::new();
    let registration = registration(executor, held.clone());
    let snapshot = manager.lifecycle().get_cached("auth-a").unwrap();
    let first = spawn_refresh(runtime.clone(), registration.clone(), snapshot.clone());
    held.wait_until_entered().await;
    let second = spawn_refresh(runtime, registration, snapshot);
    held.release.add_permits(1);
    let left = first.await.unwrap().unwrap().unwrap();
    let right = second.await.unwrap().unwrap().unwrap();
    assert_eq!(left.metadata["access_token"], "fresh");
    assert_eq!(right.metadata["access_token"], "fresh");
    assert_eq!(held.calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn candidate_async_refresh_preparation_and_401_share_account_lock() {
    let (runtime, executor, manager, _) = runtime(Mode::Success, &["auth-a"]);
    let held = HeldRefresher::new();
    let registration = Arc::new(
        ProviderExecutorRegistration::new("claude", executor)
            .unwrap()
            .with_auth_preparer(held.clone())
            .with_async_auth_refresher(held.clone()),
    );
    let original = manager.lifecycle().get_cached("auth-a").unwrap();
    let preparing = {
        let runtime = runtime.clone();
        let registration = registration.clone();
        let mut snapshot = original.clone();
        tokio::spawn(async move {
            runtime
                .prepare(&registration, &mut snapshot)
                .await
                .map(|()| snapshot)
        })
    };
    held.wait_until_entered().await;
    let refreshing = spawn_refresh(runtime, registration, original);
    held.release.add_permits(1);
    assert_eq!(
        preparing.await.unwrap().unwrap().metadata["access_token"],
        "fresh"
    );
    assert_eq!(
        refreshing.await.unwrap().unwrap().unwrap().metadata["access_token"],
        "fresh"
    );
    assert_eq!(held.calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn candidate_async_refresh_held_result_rejects_delete_and_reregistration() {
    for removed in [true, false] {
        let (runtime, executor, manager, _) = runtime(Mode::Success, &["auth-a"]);
        let held = HeldRefresher::new();
        let registration = registration(executor.clone(), held.clone());
        let snapshot = manager.lifecycle().get_cached("auth-a").unwrap();
        let operation = spawn_refresh(runtime, registration, snapshot);
        held.wait_until_entered().await;
        if removed {
            manager.delete("auth-a").unwrap();
        } else {
            let mut replacement = manager.lifecycle().get_cached("auth-a").unwrap();
            replacement
                .metadata
                .insert("access_token".into(), serde_json::json!("replacement"));
            manager
                .register(replacement, AuthMutationOptions::default(), now())
                .unwrap();
        }
        held.release.add_permits(1);
        assert!(operation.await.unwrap().is_err());
        assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
        if removed {
            assert!(manager.lifecycle().get_cached("auth-a").is_none());
        } else {
            assert_eq!(
                manager.lifecycle().get_cached("auth-a").unwrap().metadata["access_token"],
                "replacement"
            );
        }
    }
}
#[tokio::test]
async fn candidate_async_refresh_concurrent_disable_prevents_inference() {
    let (runtime, executor, manager, _) = runtime(Mode::RefreshUnary, &["auth-a"]);
    let held = HeldRefresher::new();
    manager.register_executor(registration(executor.clone(), held.clone()));
    let operation =
        tokio::spawn(async move { runtime.execute(&["claude".into()], request()).await });
    held.wait_until_entered().await;
    let mut current = manager.lifecycle().get_cached("auth-a").unwrap();
    current.disabled = true;
    manager
        .update(current, AuthMutationOptions::default(), now())
        .unwrap();
    held.release.add_permits(1);
    assert!(operation.await.unwrap().is_err());
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert!(manager.lifecycle().get_cached("auth-a").unwrap().disabled);
}
#[tokio::test]
async fn candidate_async_refresh_merges_concurrent_user_edit() {
    let (runtime, executor, manager, _) = runtime(Mode::Success, &["auth-a"]);
    let held = HeldRefresher::new();
    let registration = registration(executor, held.clone());
    let snapshot = manager.lifecycle().get_cached("auth-a").unwrap();
    let operation = spawn_refresh(runtime, registration, snapshot);
    held.wait_until_entered().await;
    let mut current = manager.lifecycle().get_cached("auth-a").unwrap();
    current.label = "User label".into();
    current.proxy_url = "https://user.fixture.invalid".into();
    current
        .metadata
        .insert("notes".into(), serde_json::json!("current edit"));
    manager
        .update(current, AuthMutationOptions::default(), now())
        .unwrap();
    held.release.add_permits(1);
    let accepted = operation.await.unwrap().unwrap().unwrap();
    assert_eq!(accepted.label, "User label");
    assert_eq!(accepted.proxy_url, "https://user.fixture.invalid");
    assert_eq!(accepted.metadata["notes"], "current edit");
    assert_eq!(accepted.metadata["access_token"], "fresh");
}
#[tokio::test]
async fn candidate_async_refresh_cancellation_leaves_current_key_and_unlocks_owner() {
    let (runtime, executor, manager, _) = runtime(Mode::Success, &["auth-a"]);
    let held = HeldRefresher::new();
    let registration = registration(executor, held.clone());
    let snapshot = manager.lifecycle().get_cached("auth-a").unwrap();
    let operation = spawn_refresh(runtime.clone(), registration.clone(), snapshot.clone());
    held.wait_until_entered().await;
    operation.abort();
    assert!(operation.await.unwrap_err().is_cancelled());
    assert_eq!(
        manager.lifecycle().get_cached("auth-a").unwrap().metadata["access_token"],
        "stale"
    );
    held.release.add_permits(1);
    let next = tokio::time::timeout(
        Duration::from_secs(5),
        runtime.refresh_after_unauthorized(&snapshot, &registration),
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap();
    assert_eq!(next.metadata["access_token"], "fresh");
    assert_eq!(held.calls.load(Ordering::SeqCst), 2);
}
#[tokio::test]
async fn candidate_async_refresh_failure_keeps_current_snapshot() {
    let (runtime, executor, manager, _) = runtime(Mode::Success, &["auth-a"]);
    let held = HeldRefresher::new();
    held.failure.store(true, Ordering::SeqCst);
    held.release.add_permits(1);
    let registration = registration(executor.clone(), held);
    let snapshot = manager.lifecycle().get_cached("auth-a").unwrap();
    assert!(runtime
        .refresh_after_unauthorized(&snapshot, &registration)
        .await
        .is_err());
    assert_eq!(
        manager.lifecycle().get_cached("auth-a").unwrap().metadata["access_token"],
        "stale"
    );
    assert_eq!(executor.refreshes.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn candidate_async_refresh_rejects_obsolete_or_disabled_before_mint() {
    for disabled in [true, false] {
        let (runtime, executor, manager, _) = runtime(Mode::Success, &["auth-a"]);
        let held = HeldRefresher::new();
        let registration = registration(executor, held.clone());
        let snapshot = manager.lifecycle().get_cached("auth-a").unwrap();
        let mut current = snapshot.clone();
        if disabled {
            current.disabled = true;
            manager
                .update(current, AuthMutationOptions::default(), now())
                .unwrap();
        } else {
            manager
                .register(current, AuthMutationOptions::default(), now())
                .unwrap();
        }
        assert!(runtime
            .refresh_after_unauthorized(&snapshot, &registration)
            .await
            .is_err());
        assert_eq!(held.calls.load(Ordering::SeqCst), 0);
    }
}
#[tokio::test]
async fn candidate_async_refresh_config_api_key_does_not_invoke_oauth_refresh() {
    let (runtime, executor, manager, _) = runtime(Mode::Success, &["auth-a"]);
    let held = HeldRefresher::new();
    let registration = registration(executor, held.clone());
    let mut snapshot = manager.lifecycle().get_cached("auth-a").unwrap();
    snapshot
        .attributes
        .insert("auth_kind".into(), "apikey".into());
    snapshot
        .attributes
        .insert("source".into(), "config:api-key".into());
    assert!(runtime
        .refresh_after_unauthorized(&snapshot, &registration)
        .await
        .unwrap()
        .is_none());
    assert_eq!(held.calls.load(Ordering::SeqCst), 0);
}
