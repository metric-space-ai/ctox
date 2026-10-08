// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use crate::business_os::project_chats::{jour_fixe_owner::check_live_meeting_for_authenticated_actor,
    jour_fixe_speech::{BoundTranscription,submit_staged_final}};
use crate::execution::speech::{tests::{fixture as transport_fixture,start},VerifiedTranscriptEvent,VerifiedTranscriptFinal};
use tokio::{task::JoinHandle,time::{timeout,Duration}};

fn owner_token(root:&Path)->anyhow::Result<String> {
    Ok(store::issue_business_os_capability_token_for_managed_user(root,"owner","Owner","admin",
        chrono::Utc::now().timestamp_millis())?.0)
}
async fn bound(root:&Path,token:&str)->anyhow::Result<(BoundTranscription,JoinHandle<()>)> {
    let binding=check_live_meeting_for_authenticated_actor(root,"owner","project","meeting-1",1)?;
    let (endpoint,server)=transport_fixture("normal").await;
    let stream=start(&endpoint).await;
    Ok((BoundTranscription::bind(root,token,binding,stream)?,server))
}
async fn final_receipt(bound:&mut BoundTranscription)->anyhow::Result<VerifiedTranscriptFinal> {
    let stream=bound.stream_mut();
    stream.append_pcm(&[0;640])?;
    stream.finish_audio()?;
    loop {
        let event=timeout(Duration::from_secs(10),stream.next_verified_event()).await?
            .context("fixture stream ended without final")??;
        if let VerifiedTranscriptEvent::Final(final_value)=event {return Ok(final_value)}
    }
}
async fn finish(bound:BoundTranscription,server:JoinHandle<()>)->anyhow::Result<()> {
    bound.cancel().await;
    timeout(Duration::from_secs(10),server).await??;
    Ok(())
}
fn consumed(root:&Path)->anyhow::Result<i64> {
    Ok(open_store(root)?.query_row("SELECT count(*) FROM workjet_jour_fixe_speech_receipts WHERE consumed_command IS NOT NULL",[],|r|r.get(0))?)
}
#[tokio::test]
async fn real_stream_final_is_persisted_once_through_replicated_command_authority() -> anyhow::Result<()> {
    let root=super::jour_fixe_owner::fixture("live")?;
    let token=owner_token(root.path())?;
    let (mut stream,server)=bound(root.path(),&token).await?;
    let receipt=final_receipt(&mut stream).await?;
    stream.stage_final(root.path(),&token,receipt)?;
    let first=submit_staged_final(root.path(),&token,stream.binding(),stream.stream_id())?;
    assert_eq!(first["status"],"completed","{first}");
    let replay=submit_staged_final(root.path(),&token,stream.binding(),stream.stream_id())?;
    assert_eq!(first["result"],replay["result"]);
    let meeting=super::jour_fixe_owner::saved(root.path())?;
    let turns=meeting["transcript"].as_array().unwrap();
    assert_eq!(turns.len(),1);assert_eq!(turns[0]["modality"],"speech");
    assert_eq!(turns[0]["text"],"Hallo Welt.");
    assert_eq!(turns[0]["stream_id"],stream.stream_id());
    assert!(turns[0]["sentence_end_latency_ms"].is_null(),"gateway delay is not end-to-end latency");
    assert_eq!(consumed(root.path())?,1);
    finish(stream,server).await
}
#[tokio::test]
async fn actual_final_from_another_stream_cannot_enter_the_bound_meeting() -> anyhow::Result<()> {
    let root=super::jour_fixe_owner::fixture("live")?;
    let token=owner_token(root.path())?;
    let (first,server1)=bound(root.path(),&token).await?;
    let (mut second,server2)=bound(root.path(),&token).await?;
    let receipt=final_receipt(&mut second).await?;
    assert!(first.stage_final(root.path(),&token,receipt).is_err());
    assert_eq!(consumed(root.path())?,0);
    assert!(super::jour_fixe_owner::saved(root.path())?["transcript"].as_array().unwrap().is_empty());
    finish(first,server1).await?;finish(second,server2).await
}
#[tokio::test]
async fn ending_meeting_during_provider_await_prevents_final_staging() -> anyhow::Result<()> {
    let root=super::jour_fixe_owner::fixture("live")?;
    let token=owner_token(root.path())?;
    let (mut stream,server)=bound(root.path(),&token).await?;
    let receipt=final_receipt(&mut stream).await?;
    let conn=open_store(root.path())?;
    let mut metadata=super::jour_fixe_owner::saved(root.path())?;
    metadata["state"]=json!("review");
    conn.execute("UPDATE workjet_jour_fixe_meetings SET metadata_json=?1",[metadata.to_string()])?;
    assert!(stream.stage_final(root.path(),&token,receipt).is_err());
    assert_eq!(consumed(root.path())?,0);finish(stream,server).await
}
#[tokio::test]
async fn revoked_actor_cannot_stage_an_actual_final_or_read_a_retry() -> anyhow::Result<()> {
    let root=super::jour_fixe_owner::fixture("live")?;
    let token=owner_token(root.path())?;
    let (mut stream,server)=bound(root.path(),&token).await?;
    let receipt=final_receipt(&mut stream).await?;
    assert!(store::verified_webrtc_capability_claims(root.path(),&token).is_some());
    open_store(root.path())?.execute("UPDATE business_users SET active=0 WHERE user_id='owner'",[])?;
    assert!(store::verified_webrtc_capability_claims(root.path(),&token).is_none());
    assert!(stream.stage_final(root.path(),&token,receipt).is_err());
    assert!(submit_staged_final(root.path(),&token,stream.binding(),stream.stream_id()).is_err());
    assert_eq!(consumed(root.path())?,0);finish(stream,server).await
}
#[tokio::test]
async fn domain_receipt_failure_rolls_back_speech_consumption_and_allows_bounded_retry() -> anyhow::Result<()> {
    let root=super::jour_fixe_owner::fixture("live")?;
    let token=owner_token(root.path())?;
    let (mut stream,server)=bound(root.path(),&token).await?;
    let receipt=final_receipt(&mut stream).await?;
    stream.stage_final(root.path(),&token,receipt)?;
    let conn=open_store(root.path())?;
    conn.execute_batch("CREATE TRIGGER fail_speech_receipt BEFORE INSERT ON business_command_domain_effects BEGIN SELECT RAISE(FAIL,'fixture receipt failure'); END;")?;
    let result=submit_staged_final(root.path(),&token,stream.binding(),stream.stream_id());
    assert!(result.is_err() || result.as_ref().is_ok_and(|v|v["status"]=="failed"),"{result:?}");
    assert_eq!(consumed(root.path())?,0);
    assert!(super::jour_fixe_owner::saved(root.path())?["transcript"].as_array().unwrap().is_empty());
    conn.execute_batch("DROP TRIGGER fail_speech_receipt")?;
    let retried=submit_staged_final(root.path(),&token,stream.binding(),stream.stream_id())?;
    assert_eq!(retried["status"],"completed","{retried}");
    assert_eq!(consumed(root.path())?,1);finish(stream,server).await
}
#[tokio::test]
async fn concurrent_native_final_retries_apply_exactly_one_transcript() -> anyhow::Result<()> {
    let root=super::jour_fixe_owner::fixture("live")?;
    let token=owner_token(root.path())?;
    let (mut stream,server)=bound(root.path(),&token).await?;
    let receipt=final_receipt(&mut stream).await?;
    stream.stage_final(root.path(),&token,receipt)?;
    let gate=std::sync::Arc::new(std::sync::Barrier::new(2));
    let workers=(0..2).map(|_| {
        let gate=gate.clone();let root=root.path().to_owned();let token=token.clone();
        let binding=stream.binding().clone();let stream_id=stream.stream_id().to_owned();
        std::thread::spawn(move || {gate.wait();submit_staged_final(&root,&token,&binding,&stream_id)})
    }).collect::<Vec<_>>();
    let results=workers.into_iter().map(|w|w.join().expect("retry worker panicked")).collect::<Vec<_>>();
    assert!(results.iter().any(|r|r.as_ref().is_ok_and(|v|v["status"]=="completed")),"{results:?}");
    assert_eq!(super::jour_fixe_owner::saved(root.path())?["transcript"].as_array().unwrap().len(),1);
    assert_eq!(consumed(root.path())?,1);
    let receipts:i64=open_store(root.path())?.query_row("SELECT count(*) FROM business_command_domain_effects
        WHERE command_id IN (SELECT consumed_command FROM workjet_jour_fixe_speech_receipts)",[],|r|r.get(0))?;
    assert_eq!(receipts,1);
    finish(stream,server).await
}
#[tokio::test]
async fn valid_foreign_admin_cannot_bind_an_owners_actual_stream() -> anyhow::Result<()> {
    let root=super::jour_fixe_owner::fixture("live")?;
    let _owner=owner_token(root.path())?;
    let foreign=store::issue_business_os_capability_token_for_managed_user(root.path(),
        "foreign","Foreign","admin",chrono::Utc::now().timestamp_millis())?.0;
    assert!(store::verified_webrtc_capability_claims(root.path(),&foreign).is_some());
    let binding=check_live_meeting_for_authenticated_actor(root.path(),"owner","project","meeting-1",1)?;
    let (endpoint,server)=transport_fixture("normal").await;
    let stream=start(&endpoint).await;
    assert!(BoundTranscription::bind(root.path(),&foreign,binding,stream).is_err());
    assert!(super::jour_fixe_owner::saved(root.path())?["transcript"].as_array().unwrap().is_empty());
    timeout(Duration::from_secs(10),server).await??;
    Ok(())
}
#[tokio::test]
async fn existing_open_stream_does_not_authorize_browser_fabricated_speech() -> anyhow::Result<()> {
    let root=super::jour_fixe_owner::fixture("live")?;
    let token=owner_token(root.path())?;
    let (stream,server)=bound(root.path(),&token).await?;
    let result=crate::business_os::command_plane::accept_rxdb_business_command(root.path(),json!({
        "id":"forged-final","module":"ctox","record_id":"project",
        "command_type":"ctox.workjet.jour_fixe.transcript.append",
        "payload":{"meeting_id":"meeting-1","operation_id":"forged-op","expected_revision":0,
            "turn":{"id":"forged","meeting_id":"meeting-1","sequence":1,"speaker":"owner",
                "modality":"speech","text":"Invented speech","stream_id":stream.stream_id(),
                "started_at_ms":1,"ended_at_ms":2}},
        "client_context":{"actor":{"id":"owner","role":"admin"}}
    }));
    assert!(result.is_err() || result.as_ref().is_ok_and(|v|v["status"]=="failed"),"{result:?}");
    assert_eq!(consumed(root.path())?,0);finish(stream,server).await
}
