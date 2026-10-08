// Origin: CTOX
// License: AGPL-3.0-only
//! Authenticated Owner submission of an unverified, local transcript candidate.
//! The desktop verifies its signed helper. Native only attests authorization,
//! exact meeting/deck scope and durable text storage, never speech execution.
use super::*;
use super::super::{store, workjet_jour_fixe_contract as wire};
use rusqlite::{params, OptionalExtension};
use wire::WireValidate;

pub(in crate::business_os) const COMMAND: &str = "ctox.workjet.jour_fixe.transcript.local_candidate";
const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS workjet_jour_fixe_local_candidates (
 operation_id TEXT PRIMARY KEY, request_key TEXT NOT NULL UNIQUE,
 owner_user_id TEXT NOT NULL, intent_hash TEXT NOT NULL, receipt_json TEXT NOT NULL
);";

fn parse(command: &BusinessCommand) -> anyhow::Result<(wire::LocalTranscriptCandidateRequest, Value)> {
    let mut payload = command.payload.clone();
    let object = payload.as_object_mut().context("local candidate must be an object")?;
    if let Some(channel) = object.remove("inbound_channel") {
        let text = channel.as_str().context("inbound_channel must be text")?;
        ensure!(!text.trim().is_empty() && text.len() <= 256, "invalid inbound_channel");
    }
    let request: wire::LocalTranscriptCandidateRequest = serde_json::from_value(payload.clone())?;
    request.validate().map_err(anyhow::Error::msg)?;
    for value in [&request.operation_id, &request.request_id, &request.instance_id,
        &request.project_id, &request.meeting_id] {
        ensure!(value.trim() == value && !value.is_empty(), "local candidate identity must be canonical");
    }
    ensure!(!request.text.trim().is_empty() && request.text.len() <= 4096,
        "local candidate exceeds the 4096-byte UTF-8 text budget");
    ensure!(command.record_id.as_deref() == Some(request.project_id.as_str()),
        "local candidate routing conflicts with project");
    Ok((request, payload))
}

fn scoped_meeting(root: &Path, conn: &Connection, actor: &str,
    request: &wire::LocalTranscriptCandidateRequest) -> anyhow::Result<wire::Meeting> {
    // This is the authenticated native `biz_` instance, not Workjet's UI alias.
    // Never create or rewrite an instance identity while accepting a candidate.
    ensure!(store::existing_instance_id(root)?.as_deref() == Some(request.instance_id.as_str()),
        "local candidate belongs to another native instance");
    let meeting = super::jour_fixe_owner::owned(conn, actor, Some(&request.project_id), &request.meeting_id)?;
    ensure!(meeting.state == wire::MeetingState::Live && meeting.deck_revision == request.deck_revision,
        "local candidate meeting is not live at the requested deck revision");
    ensure!(!meeting.slides.is_empty() && meeting.slides.iter().all(|s| s.meeting_id == meeting.id && s.audio.is_some()),
        "live meeting deck is unavailable");
    Ok(meeting)
}

/// Receipt replay rechecks current identity, project/Supervisor and live deck,
/// just as a new submission does. It cannot replay into a closed/stale scope.
pub(in crate::business_os) fn validate_recovery_scope(root: &Path, conn: &Connection,
    command: &BusinessCommand, actor: &str) -> anyhow::Result<()> {
    let (request, _) = parse(command)?;
    scoped_meeting(root, conn, actor, &request)?;
    Ok(())
}

pub(in crate::business_os) fn handle(root: &Path, command: &BusinessCommand,
    actor: &str, admission: &DomainEffectAdmission) -> anyhow::Result<Value> {
    let (request, payload) = parse(command)?;
    let intent_hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&payload)?));
    let mut conn = open_store(root)?;
    scoped_meeting(root, &conn, actor, &request)?;
    conn.execute_batch(SCHEMA)?;
    let applied = admission.apply(&mut conn, |tx| {
        let mut meeting = scoped_meeting(root, tx, actor, &request)?;
        let old: Option<(String, String, String)> = tx.query_row(
            "SELECT owner_user_id,intent_hash,receipt_json FROM workjet_jour_fixe_local_candidates WHERE operation_id=?1",
            [&request.operation_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?;
        if let Some((owner, hash, receipt)) = old {
            ensure!(owner == meeting.owner_user_id && hash == intent_hash, "local candidate operation intent conflicts");
            return Ok(AppliedDomainEffect { result: serde_json::from_str(&receipt)?, projections: vec![] });
        }
        ensure!(meeting.revision == request.expected_revision, "meeting revision changed; read the current meeting");
        let request_key = stable_id("workjet_local_candidate", &[&request.instance_id, &meeting.owner_user_id,
            &request.project_id, &request.meeting_id, &request.deck_revision.to_string(), &request.request_id]);
        let duplicate: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM workjet_jour_fixe_local_candidates WHERE request_key=?1)",
            [&request_key], |r| r.get(0))?;
        ensure!(!duplicate, "local helper final was already submitted; reuse its original operation_id");
        let sequence = meeting.transcript.last().map_or(Ok(1), |v| v.sequence.checked_add(1).context("transcript sequence overflow"))?;
        let now = chrono::Utc::now().timestamp_millis();
        ensure!(meeting.transcript.iter().all(|v| v.id != request_key), "local transcript identity conflicts");
        let turn = wire::TranscriptTurn {
            id: request_key.clone(), sequence, speaker: wire::Speaker::Owner, modality: wire::Modality::Text,
            text: request.text.clone(), started_at_ms: now, ended_at_ms: now, meeting_id: meeting.id.clone(),
            source_run_id: None, audio: None, stream_id: None, sentence_end_latency_ms: None,
        };
        turn.validate().map_err(anyhow::Error::msg)?;
        meeting.transcript.push(turn);
        meeting.revision = meeting.revision.checked_add(1).context("meeting revision overflow")?;
        meeting.validate().map_err(anyhow::Error::msg)?;
        let metadata = serde_json::to_string(&meeting)?;
        ensure!(metadata.len() <= 1024 * 1024, "meeting metadata exceeds native write budget");
        let receipt = wire::LocalTranscriptCandidateReceipt {
            operation_id: request.operation_id.clone(), request_id: request.request_id.clone(),
            instance_id: request.instance_id.clone(), project_id: meeting.project_id.clone(),
            meeting_id: meeting.id.clone(), deck_revision: meeting.deck_revision, owner_user_id: meeting.owner_user_id.clone(),
            turn_id: request_key.clone(), sequence, revision: meeting.revision,
            text_sha256: format!("{:x}", Sha256::digest(request.text.as_bytes())), persisted_at_ms: now,
            provenance: wire::LocalTranscriptProvenance::AuthenticatedOwnerLocalCandidate, provider_verified: false,
        };
        receipt.validate().map_err(anyhow::Error::msg)?;
        let mutation = wire::MeetingMutationReceipt {
            operation_id: request.operation_id.clone(), meeting_id: meeting.id.clone(), project_id: meeting.project_id.clone(),
            revision: meeting.revision, state: meeting.state, changed_id: Some(request_key.clone()), todos_revision: None,
        };
        let result = json!({"ok":true,"contract":wire::CONTRACT_SCHEMA,"mutation":mutation,"local_candidate":receipt});
        tx.execute("UPDATE workjet_jour_fixe_meetings SET metadata_json=?2 WHERE meeting_id=?1",
            params![meeting.id, metadata])?;
        tx.execute("INSERT INTO workjet_jour_fixe_local_candidates VALUES (?1,?2,?3,?4,?5)",
            params![request.operation_id, request_key, meeting.owner_user_id, intent_hash, serde_json::to_string(&result)?])?;
        Ok(AppliedDomainEffect { result, projections: vec![] })
    })?;
    Ok(applied.result)
}
