// Origin: CTOX
// License: AGPL-3.0-only
use super::tests::Fixture;
use super::*;
use crate::{
    business_os::policy::BusinessOsActor, execution::cliproxyapi_host::model_probe::Probe,
};
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
fn request() -> CommandDocument {
    CommandDocument {
        id: "14445260-37b2-4bef-997f-893704637647".into(),
        app_id: "settings".into(),
        module: "ctox-platform".into(),
        command_type: "instance.providers.models.check".into(),
        payload: json!({"account_id":"opaque-account","expected_account_revision":3,"model_id":"requested-model"}),
    }
}
#[tokio::test]
async fn native_model_check_is_permissioned_bound_and_not_reexecuted() {
    let mut f = Fixture::new().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let entered = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    let (started, finish) = (entered.clone(), resume.clone());
    f.platform.model_checker = Some(Arc::new(move |actor, r, _fence| {
        assert_eq!(actor, "owner");
        assert_eq!(r.account_id, "opaque-account");
        assert_eq!(r.expected_account_revision, 3);
        count.fetch_add(1, Ordering::SeqCst);
        let (started, finish) = (started.clone(), finish.clone());
        Box::pin(async move {
            started.notify_one();
            finish.notified().await;
            Probe::unavailable(&r.model_id, "gateway_state_unavailable")
        })
    }));
    let user = BusinessOsActor::new(Some("ordinary".into()), "user");
    let denied = f.platform.dispatch(user, request(), 1000).await;
    assert_eq!(
        denied["code"], "forbidden",
        "ordinary users must not spend model calls"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let mut command = request();
    command.id = uuid::Uuid::new_v4().to_string();
    let first = f.platform.dispatch(
        BusinessOsActor::new(Some("owner".into()), "admin"),
        command.clone(),
        1001,
    );
    tokio::pin!(first);
    tokio::select! {
        _ = entered.notified() => (),
        value = &mut first => panic!("probe returned before provider completed: {value}"),
        _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => panic!("probe not entered"),
    }
    // Writer is free while the provider is pending; the same command is not
    // another paid model request. No timing/sleep is used to race the claim.
    let replay = f
        .platform
        .dispatch(
            BusinessOsActor::new(Some("owner".into()), "admin"),
            command.clone(),
            1002,
        )
        .await;
    assert_eq!(replay["status"], "running");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    resume.notify_one();
    let result = first.await;
    assert_eq!(result["accountId"], "opaque-account");
    assert_eq!(result["accountRevision"], 3);
    assert_eq!(result["probe"]["modelId"], "requested-model");
    assert_eq!(result["probe"]["status"], "unavailable");
    assert_eq!(result["probe"]["source"], "gateway");
    assert_eq!(result["probe"]["httpStatus"], Value::Null);
    assert_eq!(
        f.platform
            .dispatch(
                BusinessOsActor::new(Some("owner".into()), "admin"),
                command.clone(),
                1003
            )
            .await,
        result
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let saved = f.read("business_commands", &command.id).await.unwrap();
    assert_eq!(saved["status"], "completed");
    assert_eq!(saved["result"], result);
    let mut changed = command;
    changed.payload["model_id"] = json!("different-model");
    assert_eq!(
        f.platform
            .dispatch(
                BusinessOsActor::new(Some("owner".into()), "admin"),
                changed,
                1004
            )
            .await["status"],
        "failed"
    );
    // A native executor may be revoked while the provider is pending. The
    // real writer must obey its commit fence and retain the pending receipt,
    // rather than turn a cancelled/expired invocation into completed.
    f.platform.model_checker = Some(Arc::new(|_actor, r, _fence| {
        Box::pin(async move { Probe::unavailable(&r.model_id, "gateway_state_unavailable") })
    }));
    let mut revoked = request();
    revoked.id = uuid::Uuid::new_v4().to_string();
    let outcome = f
        .platform
        .dispatch_fenced(
            BusinessOsActor::new(Some("owner".into()), "admin"),
            revoked.clone(),
            1005,
            Some(ModelCheckFence {
                effect: Arc::new(|effect| effect()),
                commit: Arc::new(|_| Err(StoreError::Cancelled)),
            }),
        )
        .await;
    assert_eq!(
        outcome["reason"], "store_cancelled",
        "revoked native fence must forbid publication"
    );
    assert_eq!(
        f.read("business_commands", &revoked.id).await.unwrap()["status"],
        "running"
    );
    f.stores.shutdown().await.unwrap();
}

#[tokio::test]
async fn native_model_fence_borrows_the_existing_store_read_transaction() {
    let mut f = Fixture::new().await;
    super::runtime_host::check_model_fence(f.readers.clone(),Arc::new(|tx| {
        assert!(!tx.is_autocommit(), "native reader must already hold its snapshot");
        tx.query_row("SELECT v FROM _seq WHERE singleton=1",[],|r|r.get::<_,u64>(0))?;
        Ok(())
    })).await.expect("one real Store snapshot, no nested BEGIN");
    f.stores.shutdown().await.unwrap();
}
