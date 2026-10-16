use super::*;
use crate::business_os::launch_context_v4::{build, tests::fixture};
use crate::service::store_startup::isolate_test_roots;
use rxdb::plugins::replication_webrtc::index_mod::hello::HelloFrame;
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_ingress_reads_the_real_owner_and_fences_revoked_pages() -> anyhow::Result<()> {
    let _scope = isolate_test_roots().await;
    let (_root, mut foreground, user) = fixture().await?;
    foreground.inputs.sync.writer.submit(ctox_store::WriteJob::new(Default::default(),|tx|{
        let table:String=tx.query_row("SELECT table_name FROM _collections WHERE name='notes'",[],|r|r.get(0))?;
        tx.execute(&format!("INSERT INTO \"{}\"(id,rev,schema_ver,data,deleted) VALUES('real-owner-note','1',1,?1,0)",table),[r#"{"id":"real-owner-note","title":"held writer"}"#])?;Ok(())
    }))?.await?;
    let context = build(&foreground.inputs, &user).await?;
    let HelloFrame::Hello(hello) = context.v4.hello.offer.clone() else {
        panic!("hello required")
    };
    let peer = format!("{}@1", context.v4.signaling.peer_id);
    let admission = foreground.inputs.browser_admission()?;
    (admission.authenticator())(peer.clone(), hello)
        .await
        .map_err(|_| anyhow::anyhow!("ticket rejected"))?;
    let source = Source {
        inputs: Arc::downgrade(&foreground.inputs),
        admission: admission.clone(),
    };
    let binding = admission.peer(&peer)?;
    let request = serde_json::json!({"type":"subscribe","collection":"notes","scope_hash":binding.scopes["notes"],"gen":1,"checkpoint":null,"priority":"P1"});
    let mut stream = source
        .subscribe(&peer, request)
        .await
        .map_err(|_| anyhow::anyhow!("subscription rejected"))?;
    let page = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await?
        .ok_or_else(|| anyhow::anyhow!("subscription ended"))?
        .map_err(|_| anyhow::anyhow!("owner read failed"))?;
    let packet: ctox_store::change_log::subscription::protocol::StreamPacket =
        serde_json::from_value(page.clone())?;
    packet
        .verify(&*foreground.inputs.instance_verifier()?)
        .map_err(anyhow::Error::msg)?;
    assert_eq!(packet.participant, "alice");
    assert!(
        packet.docs.iter().any(|d| d.id == "real-owner-note"
            && d.body.as_ref().is_some_and(|d| d["title"] == "held writer")),
        "real writer document missing from signed cursor"
    );
    let guard = source
        .publication(&peer, &page)
        .map_err(|_| anyhow::anyhow!("publication rejected"))?;
    guard
        .with_current(&mut || Ok(()))
        .map_err(|_| anyhow::anyhow!("current guard rejected"))?;
    admission
        .authority()
        .replace(vec![], vec![])
        .await
        .map_err(anyhow::Error::msg)?;
    assert!(
        guard.with_current(&mut || Ok(())).is_err(),
        "prepared page crossed rights revocation"
    );
    drop(stream);
    foreground.shutdown().await?;
    Ok(())
}
