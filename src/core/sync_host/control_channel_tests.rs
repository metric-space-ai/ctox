#[tokio::test]
async fn native_control_adapter_receiver_guard_holds_host_fence_without_reentering_pool(
) -> io::Result<()> {
    struct Allow;
    impl WebRTCPublicationGuard for Allow {
        fn with_current(&self, publish: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
            publish()
        }
    }
    let root = tempfile::tempdir()?;
    let pool = pool();
    let owner = Owner::start(root.path(), &pool)?;
    let publication = Publication {
        state: Arc::downgrade(&owner.state),
        peer: None,
        workload: Arc::new(Allow),
    };
    publication
        .with_current(&mut || {
            assert!(
                owner.state.alive.try_lock().is_err(),
                "host retirement is fenced during publication"
            );
            Ok(())
        })
        .map_err(|_| unavailable())?;
    owner.channel().retire();
    assert!(publication
        .with_current(&mut || panic!("retired host cannot publish"))
        .is_err());
    Ok(())
}
#[test]
fn native_control_adapter_guard_requires_exactly_one_callback() {
    struct Broken(usize);
    impl WebRTCPublicationGuard for Broken {
        fn with_current(&self, publish: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
            for _ in 0..self.0 {
                let _ = publish();
            }
            Ok(())
        }
    }
    for count in [0, 1, 2] {
        let mut calls = 0;
        let result = with_once(&Broken(count), &mut || {
            calls += 1;
            Ok(())
        });
        assert_eq!(result.is_ok(), count == 1);
        assert_eq!(calls, usize::from(count != 0));
    }
    assert!(with_once(&Deny, &mut || panic!("denied guard must not publish")).is_err());
}
// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use rxdb::plugins::replication_webrtc::{RxWebRTCReplicationPool, WebRTCRsConnectionHandler};
struct Deny;
impl WebRTCPublicationGuard for Deny {
    fn with_current(&self, _: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
        Err(denied())
    }
}
impl NativeControlReplyVerifier for Deny {
    fn verify(&self, _: &str, _: &Value) -> io::Result<Value> {
        Err(unavailable())
    }
}
fn pool() -> NativePool {
    RxWebRTCReplicationPool::new_multi(Vec::new(), WebRTCRsConnectionHandler::new())
}
#[tokio::test]
async fn native_control_adapter_host_lifetime_is_exact_and_handles_do_not_retain_pool(
) -> io::Result<()> {
    let root = tempfile::tempdir()?;
    let pool = pool();
    assert!(native_control_channel(root.path()).is_err());
    let owner = Owner::start(root.path(), &pool)?;
    let channel = owner.channel();
    assert!(native_control_channel(root.path()).is_ok());
    assert!(Owner::start(root.path(), &pool).is_err());
    let old_state = owner.state.clone();
    let mut retired = old_state.retired.subscribe();
    drop(owner);
    retired.changed().await.map_err(io::Error::other)?;
    assert!(*retired.borrow());
    assert!(channel.current().is_err());
    assert!(native_control_channel(root.path()).is_err());
    let replacement = Owner::start(root.path(), &pool)?;
    assert!(replacement.channel().current().is_ok());
    assert!(
        channel.current().is_err(),
        "retired handle cannot select the replacement host"
    );
    drop(replacement);
    drop(pool);
    assert!(
        old_state.pool.upgrade().is_none(),
        "retained consumers must not retain a pool"
    );
    Ok(())
}
#[tokio::test]
async fn native_control_adapter_request_requires_existing_peer_and_bounded_workload_namespace(
) -> io::Result<()> {
    let root = tempfile::tempdir()?;
    let pool = pool();
    let owner = Owner::start(root.path(), &pool)?;
    let channel = owner.channel();
    let pin = format!("ed25519:{}", "a".repeat(64));
    for (route, identity, method, size, timeout) in [
        (
            "peer",
            pin.as_str(),
            "ctox.sync.authority.v1",
            1,
            Duration::from_secs(1),
        ),
        (
            "peer",
            "untrusted",
            "ctox.sync.workload.speech.v1",
            1,
            Duration::from_secs(1),
        ),
        (
            "peer\n",
            pin.as_str(),
            "ctox.sync.workload.speech.v1",
            1,
            Duration::from_secs(1),
        ),
        (
            "peer",
            pin.as_str(),
            "ctox.sync.workload.speech.v1",
            MAX_REQUEST + 1,
            Duration::from_secs(1),
        ),
        (
            "peer",
            pin.as_str(),
            "ctox.sync.workload.speech.v1",
            1,
            Duration::ZERO,
        ),
        (
            "peer",
            pin.as_str(),
            "ctox.sync.workload.speech.v1",
            1,
            Duration::from_secs(31),
        ),
    ] {
        let error = channel
            .request(
                route,
                identity,
                method,
                Value::String("x".repeat(size)),
                timeout,
                Arc::new(Deny),
                Arc::new(Deny),
            )
            .await
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
    let error = channel
        .request(
            "not-connected",
            &pin,
            "ctox.sync.workload.speech.v1",
            Value::Null,
            Duration::from_secs(1),
            Arc::new(Deny),
            Arc::new(Deny),
        )
        .await
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotConnected);
    assert_eq!(owner.state.pending.available_permits(), MAX_PENDING);
    let handler: GuardedAuxiliaryRequestHandler<WebRTCRsConnection> =
        Arc::new(|_, _, _| Box::pin(async { Err("unconfigured workload".into()) }));
    assert!(channel
        .register_handler("ctox.sync.authority.v1", handler.clone())
        .is_err());
    channel.register_handler("ctox.native.speech.v1", handler.clone())?;
    channel.register_handler("ctox.sync.workload.speech.v1", handler.clone())?;
    assert!(
        channel
            .register_handler("ctox.sync.workload.speech.v1", handler)
            .is_err(),
        "registration cannot replace another workload owner"
    );
    Ok(())
}
#[tokio::test]
async fn native_control_adapter_retirement_closes_capacity_and_wakes_pending_consumers(
) -> io::Result<()> {
    let root = tempfile::tempdir()?;
    let pool = pool();
    let owner = Owner::start(root.path(), &pool)?;
    let channel = owner.channel();
    let mut retired = owner.state.retired.subscribe();
    let permits: Vec<_> = (0..MAX_PENDING)
        .map(|_| owner.state.pending.clone().try_acquire_owned().unwrap())
        .collect();
    assert!(owner.state.pending.clone().try_acquire_owned().is_err());
    channel.retire();
    retired.changed().await.map_err(io::Error::other)?;
    assert!(owner.state.pending.is_closed());
    drop(permits);
    assert!(channel.current().is_err());
    Ok(())
}
