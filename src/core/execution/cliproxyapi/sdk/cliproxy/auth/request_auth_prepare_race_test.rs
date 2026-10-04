// ref: sdk/cliproxy/auth/conductor_execution.go:1534-1594 @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox — asynchronous mint/preparation owner-race guards
// License: MIT (upstream); modifications AGPL-3.0-only
use super::*;
use tokio::sync::Semaphore;

#[tokio::test]
async fn candidate_held_preparation_observes_disabled_before_mint() {
    let (runtime, executor, manager, _) = runtime(Mode::Success, &["auth-a"]);
    let held = HeldPreparer::new();
    let registration = registration(executor, held.clone());
    let mut snapshot = manager.lifecycle().get_cached("auth-a").unwrap();
    let mut current = snapshot.clone();
    current.disabled = true;
    manager
        .update(
            current,
            AuthMutationOptions::default(),
            DateTime::parse_from_rfc3339("2026-08-04T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        )
        .unwrap();
    let error = runtime
        .prepare(&registration, &mut snapshot)
        .await
        .unwrap_err();
    assert_eq!(error.downcast_ref::<AuthError>().unwrap().http_status, 403);
    assert_eq!(held.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn candidate_held_preparation_concurrent_disable_prevents_inference() {
    let (runtime, executor, manager, _) = runtime(Mode::Success, &["auth-a"]);
    let held = HeldPreparer::new();
    manager.register_executor(registration(executor.clone(), held.clone()));
    let operation =
        tokio::spawn(async move { runtime.execute(&["claude".into()], request()).await });
    tokio::time::timeout(Duration::from_secs(5), held.entered.acquire())
        .await
        .unwrap()
        .unwrap()
        .forget();
    let mut current = manager.lifecycle().get_cached("auth-a").unwrap();
    current.disabled = true;
    manager
        .update(
            current,
            AuthMutationOptions::default(),
            DateTime::parse_from_rfc3339("2026-08-04T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        )
        .unwrap();
    held.release.add_permits(1);
    assert!(operation.await.unwrap().is_err());
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
    assert!(manager.lifecycle().get_cached("auth-a").unwrap().disabled);
}

struct HeldPreparer {
    entered: Semaphore,
    release: Semaphore,
    calls: AtomicUsize,
}
impl HeldPreparer {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            entered: Semaphore::new(0),
            release: Semaphore::new(0),
            calls: AtomicUsize::new(0),
        })
    }
}
impl AuthPreparer for HeldPreparer {
    fn prepare<'a>(
        &'a self,
        auth: &'a mut Auth,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), AuthPreparationError>> + Send + 'a>,
    > {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.entered.add_permits(1);
            self.release.acquire().await.unwrap().forget();
            auth.metadata
                .insert("access_token".into(), serde_json::json!("minted-obsolete"));
            auth.attributes.insert("prepared".into(), "true".into());
            Ok(())
        })
    }
}
fn registration(
    executor: Arc<TestExecutor>,
    preparer: Arc<HeldPreparer>,
) -> Arc<ProviderExecutorRegistration> {
    Arc::new(
        ProviderExecutorRegistration::new("claude", executor.clone())
            .unwrap()
            .with_execution(executor)
            .unwrap()
            .with_auth_preparer(preparer),
    )
}

#[tokio::test]
async fn candidate_held_preparation_rejects_removed_or_reregistered_account() {
    for removed in [true, false] {
        let (runtime, executor, manager, _) = runtime(Mode::Success, &["auth-a"]);
        let held = HeldPreparer::new();
        let registration = registration(executor.clone(), held.clone());
        let mut snapshot = manager.lifecycle().get_cached("auth-a").unwrap();
        let operation = tokio::spawn(async move {
            runtime
                .prepare(&registration, &mut snapshot)
                .await
                .map(|()| snapshot)
        });
        held.entered.acquire().await.unwrap().forget();
        if removed {
            manager.delete("auth-a").unwrap();
        } else {
            let mut replacement = manager.lifecycle().get_cached("auth-a").unwrap();
            replacement
                .metadata
                .insert("access_token".into(), serde_json::json!("replacement"));
            manager
                .register(
                    replacement,
                    AuthMutationOptions::default(),
                    FixedClock(
                        DateTime::parse_from_rfc3339("2026-08-03T12:00:00Z")
                            .unwrap()
                            .with_timezone(&Utc),
                    )
                    .now(),
                )
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
async fn candidate_held_preparation_merges_concurrent_user_settings_before_returning() {
    let (runtime, executor, manager, _) = runtime(Mode::Success, &["auth-a"]);
    let held = HeldPreparer::new();
    let registration = registration(executor, held.clone());
    let mut snapshot = manager.lifecycle().get_cached("auth-a").unwrap();
    let operation = tokio::spawn(async move {
        runtime
            .prepare(&registration, &mut snapshot)
            .await
            .map(|()| snapshot)
    });
    held.entered.acquire().await.unwrap().forget();
    let mut current = manager.lifecycle().get_cached("auth-a").unwrap();
    current.label = "User name".into();
    current.proxy_url = "https://user-proxy.example".into();
    current
        .metadata
        .insert("notes".into(), serde_json::json!("edited while minting"));
    manager
        .update(
            current,
            AuthMutationOptions::default(),
            DateTime::parse_from_rfc3339("2026-08-03T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        )
        .unwrap();
    held.release.add_permits(1);
    let prepared = operation.await.unwrap().unwrap();
    assert_eq!(prepared.label, "User name");
    assert_eq!(prepared.proxy_url, "https://user-proxy.example");
    assert_eq!(prepared.metadata["notes"], "edited while minting");
    assert_eq!(prepared.metadata["access_token"], "minted-obsolete");
    assert_eq!(
        manager.lifecycle().get_cached("auth-a").unwrap().label,
        "User name"
    );
}

#[tokio::test]
async fn candidate_cancelled_held_preparation_never_publishes_an_unaccepted_key() {
    let (runtime, executor, manager, _) = runtime(Mode::Success, &["auth-a"]);
    let held = HeldPreparer::new();
    let registration = registration(executor, held.clone());
    let mut snapshot = manager.lifecycle().get_cached("auth-a").unwrap();
    let operation =
        tokio::spawn(async move { runtime.prepare(&registration, &mut snapshot).await });
    held.entered.acquire().await.unwrap().forget();
    operation.abort();
    assert!(operation.await.unwrap_err().is_cancelled());
    held.release.add_permits(1);
    tokio::task::yield_now().await;
    let current = manager.lifecycle().get_cached("auth-a").unwrap();
    assert_eq!(current.metadata["access_token"], "stale");
    assert!(!current.attributes.contains_key("prepared"));
    assert_eq!(held.calls.load(Ordering::SeqCst), 1);
}
