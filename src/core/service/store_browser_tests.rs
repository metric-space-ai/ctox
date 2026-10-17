use super::*;
use crate::business_os::launch_context_v4::{build, tests::fixture};
use crate::service::store_startup::isolate_test_roots;
fn hello(context: &V4LaunchContext) -> Hello {
    let HelloFrame::Hello(h) = &context.v4.hello.offer else {
        panic!("hello required")
    };
    h.clone()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_ticket_replay_is_rejected() -> anyhow::Result<()> {
    let _scope = isolate_test_roots().await;
    let (_root, mut fg, user) = fixture().await?;
    let context = build(&fg.inputs, &user).await?;
    let admission = fg.inputs.browser_admission()?;
    let peer = context.v4.signaling.peer_id.clone();
    let h = hello(&context);
    let first = admission.admit(format!("{peer}@1"), h.clone()).await?;
    assert!(
        !first.scope_tokens.is_empty(),
        "no authorized scopes issued"
    );
    assert!(
        admission.admit(format!("{peer}@2"), h).await.is_err(),
        "browser ticket replay accepted"
    );
    fg.shutdown().await?;
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn expired_browser_ticket_is_rejected() -> anyhow::Result<()> {
    let _scope = isolate_test_roots().await;
    let (_root, mut fg, user) = fixture().await?;
    let context = build(&fg.inputs, &user).await?;
    let admission = fg.inputs.browser_admission()?;
    let h = hello(&context);
    admission
        .tickets
        .lock()
        .unwrap()
        .get_mut(&h.device_proof.nonce)
        .unwrap()
        .expires = Instant::now() - Duration::from_secs(1);
    assert!(
        admission
            .admit(context.v4.signaling.peer_id, h)
            .await
            .is_err(),
        "expired browser ticket accepted"
    );
    fg.shutdown().await?;
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_user_browser_ticket_is_rejected() -> anyhow::Result<()> {
    let _scope = isolate_test_roots().await;
    let (_root, mut fg, user) = fixture().await?;
    let context = build(&fg.inputs, &user).await?;
    let admission = fg.inputs.browser_admission()?;
    let h = hello(&context);
    admission
        .tickets
        .lock()
        .unwrap()
        .get_mut(&h.device_proof.nonce)
        .unwrap()
        .user = "other-user".into();
    assert!(
        admission
            .admit(context.v4.signaling.peer_id, h)
            .await
            .is_err(),
        "wrong-user browser ticket accepted"
    );
    fg.shutdown().await?;
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_ticket_checks_current_user_and_rights() -> anyhow::Result<()> {
    let _scope = isolate_test_roots().await;
    let (_root, mut fg, user) = fixture().await?;
    let context = build(&fg.inputs, &user).await?;
    let admission = fg.inputs.browser_admission()?;
    let h = hello(&context);
    fg.inputs.sync.writer.submit(WriteJob::new(Default::default(),|tx| {
        let table:String=tx.query_row("SELECT table_name FROM _collections WHERE name='business_users'",[],|r|r.get(0))?;
        tx.execute(&format!("UPDATE \"{}\" SET data=json_set(data,'$.active',json('false')) WHERE id='alice'",table),[])?;Ok(())
    }))?.await?;
    assert!(
        admission
            .admit(context.v4.signaling.peer_id, h)
            .await
            .is_err(),
        "disabled user admitted"
    );
    fg.shutdown().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_peer_still_requires_fresh_connection_challenge() -> anyhow::Result<()> {
    use rxdb::plugins::replication_webrtc::index_mod::{
        connection_lifecycle::PeerLifecycleRegistry,
        hello::{ApiRange, Codec, HelloPolicy, TransportOffer},
    };
    let _scope = isolate_test_roots().await;
    let (_root, mut fg, user) = fixture().await?;
    let context = build(&fg.inputs, &user).await?;
    let h = hello(&context);
    let policy = HelloPolicy {
        instance_id: h.instance_id.clone(),
        bootstrap_min: 1,
        bootstrap_max: 1,
        app_api_ver: ApiRange {
            min: "4.0".into(),
            max: "4.0".into(),
        },
        capabilities: h.capabilities.clone(),
        required_capabilities: vec![],
        transport: TransportOffer {
            codecs: vec![Codec::Gzip],
            max_frame_bytes: 16 * 1024,
            fragmentation: true,
            reassembly: true,
        },
    };
    let registry = PeerLifecycleRegistry::default();
    let peer = registry.start(
        context.v4.signaling.peer_id,
        "A".repeat(43),
        policy,
        fg.inputs.browser_admission()?.authenticator(),
        Duration::from_secs(5),
    )?;
    peer.transport_connected().await?;
    assert!(
        matches!(peer.receive_hello(h).await?, HelloFrame::HelloReject(_)),
        "native peer without connection challenge accepted"
    );
    registry.drain_all().await;
    fg.shutdown().await?;
    Ok(())
}
