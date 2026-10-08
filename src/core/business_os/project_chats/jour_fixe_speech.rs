// Origin: CTOX
// License: AGPL-3.0-only
//! Native speech provenance. Wire TranscriptTurn values cannot create these receipts.
//! The transport adapter still owns its fresh peer/session gate and bounded stream.
use super::*;
use super::jour_fixe_owner::{self, LiveMeetingBinding};
use super::super::{policy::BusinessOsPermission, workjet_jour_fixe_contract as wire};
use crate::execution::speech::{TranscriptionStream, VerifiedTranscriptFinal};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use wire::WireValidate;

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS workjet_jour_fixe_speech_receipts (
 stream_id TEXT PRIMARY KEY, owner_user_id TEXT NOT NULL, project_id TEXT NOT NULL,
 meeting_id TEXT NOT NULL, deck_revision INTEGER NOT NULL, binding_hash TEXT NOT NULL,
 opened_at_ms INTEGER NOT NULL, final_json TEXT, staged_json TEXT, consumed_command TEXT
);
CREATE INDEX IF NOT EXISTS workjet_jour_fixe_speech_by_meeting
 ON workjet_jour_fixe_speech_receipts(meeting_id);";

/// Owns the real non-cloneable gateway stream. Construct this immediately after
/// open_transcription and before accepting PCM; a browser stream ID is insufficient.
pub(in crate::business_os) struct BoundTranscription {
    binding: LiveMeetingBinding,
    stream: TranscriptionStream,
}

fn authenticated_actor(root:&Path, token:&str)->anyhow::Result<String> {
    // A preparatory snapshot, with no issuer fence or write transaction. The
    // normal replicated command admission rechecks authority before mutation.
    store::with_webrtc_capability_signer_snapshot(root, |secret| {
        let mut conn=Connection::open_with_flags(store::business_os_store_path(root),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
        conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
        let tx=conn.transaction()?;
        let at=chrono::Utc::now().timestamp_millis();
        let claims=store::verified_webrtc_capability_claims_from_connection(&tx,token,secret,at)
            .context("speech actor is expired or revoked")?;
        ensure!(store::check_webrtc_collection_permission_from_connection(&tx,token,secret,
            "business_commands",BusinessOsPermission::DataWrite,at)?,"speech actor may not write commands");
        Ok(claims.user_id)
    })
}
fn digest_binding(binding:&LiveMeetingBinding)->anyhow::Result<String> {
    Ok(format!("{:x}",Sha256::digest(serde_json::to_vec(&binding.identity())?)))
}
fn checked(root:&Path,token:&str,binding:&LiveMeetingBinding)->anyhow::Result<LiveMeetingBinding> {
    let actor=authenticated_actor(root,token)?;
    binding.revalidate(root,&actor)
}

impl BoundTranscription {
    pub(in crate::business_os) fn bind(
        root:&Path,token:&str,binding:LiveMeetingBinding,stream:TranscriptionStream,
    )->anyhow::Result<Self> {
        let binding=checked(root,token,&binding)?;
        let mut conn=open_store(root)?;
        conn.execute_batch(SCHEMA)?;
        let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        binding.revalidate_from_connection(&tx,binding.owner_user_id())?;
        let count:i64=tx.query_row("SELECT count(*) FROM workjet_jour_fixe_speech_receipts WHERE meeting_id=?1",
            [binding.meeting_id()],|r|r.get(0))?;
        ensure!(count<10000,"meeting speech receipt budget exhausted");
        tx.execute("INSERT INTO workjet_jour_fixe_speech_receipts
          (stream_id,owner_user_id,project_id,meeting_id,deck_revision,binding_hash,opened_at_ms)
          VALUES (?1,?2,?3,?4,?5,?6,?7)",params![stream.stream_id(),binding.owner_user_id(),
            binding.project_id(),binding.meeting_id(),binding.deck_revision(),digest_binding(&binding)?,
            chrono::Utc::now().timestamp_millis()])?;
        tx.commit()?;
        Ok(Self{binding,stream})
    }
    pub(in crate::business_os) fn stream_id(&self)->&str { self.stream.stream_id() }
    pub(in crate::business_os) fn binding(&self)->&LiveMeetingBinding { &self.binding }
    /// The adapter revalidates its current transport capability around awaited
    /// provider operations; this check also binds it to the unchanged live deck.
    pub(in crate::business_os) fn revalidate(&self,root:&Path,token:&str)->anyhow::Result<()> {
        checked(root,token,&self.binding).map(|_|())
    }
    pub(in crate::business_os) fn stream_mut(&mut self)->&mut TranscriptionStream { &mut self.stream }
    /// Consume an actual producer Final after awaiting that same owned stream.
    /// Only native provenance metadata is staged; no PCM or token is stored.
    pub(in crate::business_os) fn stage_final(
        &self,root:&Path,token:&str,receipt:VerifiedTranscriptFinal,
    )->anyhow::Result<()> {
        let binding=checked(root,token,&self.binding)?;
        ensure!(receipt.stream_id()==self.stream.stream_id(),"final belongs to another native stream");
        ensure!(!receipt.text().trim().is_empty() && receipt.text().chars().count()<=16384
            && !receipt.model().trim().is_empty() && receipt.model().chars().count()<=128
            && receipt.sequence()>0 && (1..=15000).contains(&receipt.audio_duration_ms()),
            "native final exceeds meeting transcript bounds");
        let now=chrono::Utc::now().timestamp_millis();
        let final_value=json!({"sequence":receipt.sequence(),"text":receipt.text(),"model":receipt.model(),
            "audio_duration_ms":receipt.audio_duration_ms(),"finish_to_final_ms":receipt.finish_to_final_ms(),
            "received_at_ms":now});
        let mut conn=open_store(root)?;
        let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        binding.revalidate_from_connection(&tx,binding.owner_user_id())?;
        let changed=tx.execute("UPDATE workjet_jour_fixe_speech_receipts SET final_json=?2
            WHERE stream_id=?1 AND binding_hash=?3 AND final_json IS NULL AND consumed_command IS NULL",
            params![receipt.stream_id(),final_value.to_string(),digest_binding(&binding)?])?;
        ensure!(changed==1,"native final already staged or stream binding changed");
        tx.commit()?;
        Ok(())
    }
    pub(in crate::business_os) async fn cancel(self) { self.stream.cancel().await; }
}

/// Retrying a staged final does not invoke STT again. Revision/sequence are
/// refreshed only while unconsumed; a committed final keeps its original intent.
/// This executes the ordinary replicated-peer command path, never TrustedLocal.
pub(in crate::business_os) fn submit_staged_final(
    root:&Path,token:&str,binding:&LiveMeetingBinding,stream_id:&str,
)->anyhow::Result<Value> {
    let binding=checked(root,token,binding)?;
    let actor=authenticated_actor(root,token)?;
    let document={
        let mut conn=open_store(root)?;
        let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current=binding.revalidate_from_connection(&tx,&actor)?;
        let (opened,raw,staged,consumed):(i64,Option<String>,Option<String>,Option<String>)=tx.query_row(
            "SELECT opened_at_ms,final_json,staged_json,consumed_command FROM workjet_jour_fixe_speech_receipts
            WHERE stream_id=?1 AND binding_hash=?2",params![stream_id,digest_binding(&binding)?],
            |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
        let document=if consumed.is_some() {
            serde_json::from_str::<Value>(&staged.context("consumed transcript has no intent")?)?
        } else {
            let value:Value=serde_json::from_str(&raw.context("native final has not arrived")?)?;
            let meeting=jour_fixe_owner::owned(&tx,&actor,Some(binding.project_id()),binding.meeting_id())?;
            let next=meeting.transcript.last().map_or(Ok(1),|v|v.sequence.checked_add(1).context("transcript overflow"))?;
            let id=stable_id("workjet_stt_turn",&[binding.meeting_id(),stream_id,
                &value["sequence"].to_string()]);
            let operation=stable_id("workjet_stt_append",&[stream_id]);
            let command=stable_id("workjet_stt_command",&[stream_id,&current.meeting_revision().to_string(),
                &uuid::Uuid::new_v4().to_string()]);
            let turn=wire::TranscriptTurn{id,meeting_id:binding.meeting_id().to_owned(),sequence:next,
                speaker:wire::Speaker::Owner,modality:wire::Modality::Speech,
                text:value["text"].as_str().context("native final text unavailable")?.to_owned(),
                started_at_ms:opened,ended_at_ms:value["received_at_ms"].as_i64().context("native final time unavailable")?,
                source_run_id:None,audio:None,stream_id:Some(stream_id.to_owned()),sentence_end_latency_ms:None};
            // Gateway finish-to-final is NOT client sentence-end latency.
            turn.validate().map_err(anyhow::Error::msg)?;
            let document=json!({"id":command,"module":"ctox","record_id":binding.project_id(),
                "command_type":"ctox.workjet.jour_fixe.transcript.append",
                "payload":{"operation_id":operation,"meeting_id":binding.meeting_id(),
                    "expected_revision":current.meeting_revision(),"turn":turn}});
            tx.execute("UPDATE workjet_jour_fixe_speech_receipts SET staged_json=?2
                WHERE stream_id=?1 AND consumed_command IS NULL",params![stream_id,document.to_string()])?;
            document
        };
        tx.commit()?;
        document
    };
    let mut document=document;
    document["client_context"]=json!({"actor":{"id":actor},"capability_token":token});
    super::super::command_plane::accept_rxdb_business_command_with_origin(root,document,
        store::CommandOrigin::ReplicatedPeer)
}

/// Called only inside the domain writer transaction. An asserted wire speech
/// turn must exactly match a private native producer receipt and staged intent.
/// Consumption commits with the transcript and immutable domain receipt.
pub(super) fn consume_in_transaction(
    tx:&Connection,meeting:&wire::Meeting,command:&BusinessCommand,payload:&Value,
)->anyhow::Result<()> {
    ensure!(meeting.state==wire::MeetingState::Live,"speech meeting is no longer live");
    ensure!(!meeting.slides.is_empty() && meeting.slides.iter().all(|slide|
        slide.meeting_id==meeting.id && slide.audio.is_some()),
        "speech meeting deck is no longer available");
    let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table'
        AND name='workjet_jour_fixe_speech_receipts')",[],|r|r.get(0))?;
    ensure!(exists,"speech transcript has no native provenance");
    let stream=payload["turn"]["stream_id"].as_str().context("speech stream required")?;
    let row:Option<(String,String,String,u64,String,Option<String>,String)>=tx.query_row(
        "SELECT owner_user_id,project_id,meeting_id,deck_revision,staged_json,consumed_command,binding_hash
         FROM workjet_jour_fixe_speech_receipts WHERE stream_id=?1 AND final_json IS NOT NULL",
         [stream],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional()?;
    let (owner,project,id,deck,raw,consumed,stored_binding)=row.context("speech transcript has no staged native final")?;
    ensure!(owner==meeting.owner_user_id && project==meeting.project_id && id==meeting.id
        && deck==meeting.deck_revision && consumed.is_none(),"native speech binding or consumption conflicts");
    let current_identity=json!({"owner":meeting.owner_user_id,"project":meeting.project_id,"meeting":meeting.id,
        "supervisor_thread":meeting.supervisor.workjet_thread_id,"supervisor_key":meeting.supervisor.ctox_thread_key,
        "deck":meeting.deck_revision});
    ensure!(stored_binding==format!("{:x}",Sha256::digest(serde_json::to_vec(&current_identity)?)),
        "native speech supervisor binding changed");
    let staged:Value=serde_json::from_str(&raw)?;
    ensure!(staged["payload"]==*payload && staged["id"].as_str()==command.id.as_deref()
        && staged["record_id"].as_str()==command.record_id.as_deref(),"wire transcript differs from native final");
    let changed=tx.execute("UPDATE workjet_jour_fixe_speech_receipts SET consumed_command=?2
        WHERE stream_id=?1 AND consumed_command IS NULL",params![stream,command.id])?;
    ensure!(changed==1,"native final was already consumed");
    Ok(())
}
