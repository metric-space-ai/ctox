// Origin: CTOX
// License: AGPL-3.0-only
//! A bounded native producer call, outside writer transactions, followed by
//! fresh Supervisor/deck policy and atomic immutable WAV custody.
use super::super::{
    project_chats::jour_fixe_local_narration as custody, workjet_jour_fixe_contract as wire,
};
use super::*;
use crate::execution::speech::{
    SpeechAudioFormat, SpeechError, SpeechGateway, SpeechRequest, VerifiedSpeechOutput,
};
use base64::Engine;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde_json::json;
use sha2::{Digest, Sha256};
use wire::WireValidate;
const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS workjet_jour_fixe_native_narration (
 operation_id TEXT PRIMARY KEY, owner_user_id TEXT NOT NULL, meeting_id TEXT NOT NULL,
 deck_revision INTEGER NOT NULL, slide_id TEXT NOT NULL, intent_hash TEXT NOT NULL,
 state TEXT NOT NULL, attempts INTEGER NOT NULL, error_class TEXT,
 receipt_json TEXT, projections_json TEXT,
 UNIQUE(meeting_id,deck_revision,slide_id));";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    action: String,
    request: wire::NarrateRequest,
}
struct NarrationAttempt {
    operation_id: String,
    owner_user_id: String,
    intent_hash: String,
    state: String,
    attempts: u64,
    receipt_json: Option<String>,
    projections_json: Option<String>,
}
fn narration_attempt(row: &rusqlite::Row<'_>) -> rusqlite::Result<NarrationAttempt> {
    Ok(NarrationAttempt {
        operation_id: row.get(0)?,
        owner_user_id: row.get(1)?,
        intent_hash: row.get(2)?,
        state: row.get(3)?,
        attempts: row.get(4)?,
        receipt_json: row.get(5)?,
        projections_json: row.get(6)?,
    })
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn current(
    core: &Connection,
    policy: &Connection,
    context: &McpChannelRequestContext,
    trusted: &Value,
    r: &wire::NarrateRequest,
) -> anyhow::Result<(wire::Meeting, String)> {
    let meeting =
        workjet_jour_fixe::current_meeting(core, policy, context, trusted, &r.meeting_id, true)?;
    anyhow::ensure!(
        meeting.deck_revision == r.deck_revision
            && matches!(
                meeting.state,
                wire::MeetingState::Preparing
                    | wire::MeetingState::Ready
                    | wire::MeetingState::Live
            ),
        "narration deck changed or is closed"
    );
    let slide = meeting
        .slides
        .iter()
        .find(|s| s.id == r.slide_id)
        .context("narration slide unavailable")?;
    anyhow::ensure!(
        slide.meeting_id == meeting.id
            && !slide.body_markdown.trim().is_empty()
            && slide.body_markdown.len() <= 4096
            && hash(slide.body_markdown.as_bytes()) == r.narration_text_sha256,
        "narration text hash differs from the bounded stored slide"
    );
    let role = context
        .trusted_role
        .as_deref()
        .context("native role unavailable")?;
    for collection in ["desktop_files", "desktop_file_chunks"] {
        for permission in [
            BusinessOsPermission::DataRead,
            BusinessOsPermission::DataWrite,
        ] {
            anyhow::ensure!(
                super::super::store_policy::trusted_actor_policy_decision_with_conn(
                    policy,
                    &context.actor,
                    role,
                    permission,
                    BusinessOsScopeType::Collection,
                    Some(collection)
                )?
                .allowed,
                "native narration file policy denied"
            );
        }
    }
    let text = slide.body_markdown.clone();
    Ok((meeting, text))
}
fn writing(root: &Path) -> anyhow::Result<(Connection, Connection)> {
    let core = Connection::open_with_flags(
        crate::paths::core_db(root),
        OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?;
    let policy = Connection::open_with_flags(
        store::business_os_store_path(root),
        OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?;
    core.busy_timeout(std::time::Duration::from_secs(5))?;
    policy.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok((core, policy))
}
fn project_custody(root: &Path, projections: &Value) -> anyhow::Result<()> {
    let conn = store::open_store(root)?;
    let refs: Vec<super::super::domain_effect::DomainRecordRef> =
        serde_json::from_value(projections.clone())?;
    anyhow::ensure!(refs.len() <= 686, "native audio projection budget exceeded");
    for record in refs {
        anyhow::ensure!(
            matches!(
                record.collection.as_str(),
                "desktop_files" | "desktop_file_chunks"
            ),
            "native audio projection scope differs"
        );
        let (raw,at):(String,i64)=conn.query_row("SELECT payload_json,updated_at_ms FROM business_records WHERE collection=?1 AND record_id=?2 AND deleted=0",params![record.collection,record.id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        store::upsert_rxdb_collection_record(
            root,
            &record.collection,
            &record.id,
            at,
            serde_json::from_str(&raw)?,
        )?;
    }
    Ok(())
}
pub(super) fn inputs(meeting: &wire::Meeting) -> Vec<wire::NarrationInput> {
    if meeting.state != wire::MeetingState::Preparing || meeting.deck_revision == 0 {
        return vec![];
    }
    meeting
        .slides
        .iter()
        .filter(|s| {
            s.audio.is_none() && s.body_markdown.len() <= 4096 && !s.body_markdown.trim().is_empty()
        })
        .map(|s| wire::NarrationInput {
            slide_id: s.id.clone(),
            deck_revision: meeting.deck_revision,
            expected_revision: meeting.revision,
            narration_text_sha256: hash(s.body_markdown.as_bytes()),
        })
        .collect()
}

pub(super) fn execute(
    root: &Path,
    context: &McpChannelRequestContext,
    args: &Value,
    trusted: &Value,
) -> anyhow::Result<Value> {
    // MCP gateway dispatches tools on spawn_blocking within this existing
    // runtime. No nested runtime, separate daemon, browser credential or fallback.
    execute_with(root, context, args, trusted, |request| {
        let handle = tokio::runtime::Handle::try_current()
            .map_err(|_| SpeechError::ConfigurationUnavailable)?;
        let gateway = SpeechGateway::from_root(root)?;
        handle.block_on(async {
            tokio::time::timeout(
                std::time::Duration::from_secs(90),
                gateway.synthesize_verified_async(request),
            )
            .await
            .map_err(|_| SpeechError::TimedOut)?
        })
    })
}
fn execute_with<F>(
    root: &Path,
    context: &McpChannelRequestContext,
    args: &Value,
    trusted: &Value,
    producer: F,
) -> anyhow::Result<Value>
where
    F: FnOnce(&SpeechRequest) -> Result<VerifiedSpeechOutput, SpeechError>,
{
    anyhow::ensure!(
        serde_json::to_vec(args)?.len() <= 4096,
        "narration request exceeds native budget"
    );
    let parsed: Request = serde_json::from_value(args.clone())?;
    anyhow::ensure!(parsed.action == "narrate", "unsupported narration action");
    let r = parsed.request;
    r.validate().map_err(anyhow::Error::msg)?;
    for id in [&r.operation_id, &r.meeting_id, &r.slide_id] {
        anyhow::ensure!(
            id.trim() == id && !id.is_empty(),
            "narration identity must be canonical"
        );
    }
    anyhow::ensure!(
        r.narration_text_sha256
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v)),
        "narration hash must be canonical SHA256"
    );
    let intent = hash(&serde_json::to_vec(&r)?);
    let text = {
        let (mut core, mut policy) = writing(root)?;
        let core_tx = core.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let tx = policy.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (meeting, text) = current(&core_tx, &tx, context, trusted, &r)?;
        tx.execute_batch(SCHEMA)?;
        // The slide slot, not a caller's fresh operation ID, owns the retry
        // budget. An operation ID already used elsewhere still wins this lookup
        // so its intent cannot be rebound to another slide.
        let old = tx.query_row(
            "SELECT operation_id,owner_user_id,intent_hash,state,attempts,receipt_json,projections_json
             FROM workjet_jour_fixe_native_narration
             WHERE operation_id=?1 OR (meeting_id=?2 AND deck_revision=?3 AND slide_id=?4)
             ORDER BY CASE WHEN operation_id=?1 THEN 0 ELSE 1 END LIMIT 1",
            params![r.operation_id, meeting.id, r.deck_revision, r.slide_id],
            narration_attempt,
        ).optional()?;
        if let Some(old) = old {
            let replay = old.operation_id == r.operation_id;
            anyhow::ensure!(
                old.owner_user_id == meeting.owner_user_id
                    && (!replay || old.intent_hash == intent),
                "narration operation intent conflicts"
            );
            if let (true, Some(raw)) = (replay, old.receipt_json.as_ref()) {
                let result = serde_json::from_str(raw)?;
                let refs = serde_json::from_str(
                    &old.projections_json
                        .context("committed native audio projection receipt missing")?,
                )?;
                tx.commit()?;
                core_tx.commit()?;
                project_custody(root, &refs)?;
                return Ok(result);
            }
            anyhow::ensure!(
                matches!(old.state.as_str(), "failed" | "failed_prerequisite")
                    && old.receipt_json.is_none()
                    && old.attempts < 3,
                "narration uniqueness conflict: existing operation_id '{}', status '{}', attempts {}; only a failed attempt below the retry limit can be retried; running, uncertain or completed audio cannot be resynthesized",
                old.operation_id, old.state, old.attempts
            );
            // Reuse the unique slot atomically under the existing writer fence.
            // Keep its attempt count even when the Supervisor changes the ID.
            tx.execute(
                "UPDATE workjet_jour_fixe_native_narration
                 SET operation_id=?2,intent_hash=?3,state='reserved',attempts=attempts+1,error_class=NULL
                 WHERE operation_id=?1",
                params![old.operation_id, r.operation_id, intent],
            )?;
        } else {
            tx.execute("INSERT INTO workjet_jour_fixe_native_narration (operation_id,owner_user_id,meeting_id,deck_revision,slide_id,intent_hash,state,attempts) VALUES(?1,?2,?3,?4,?5,?6,'reserved',1)",params![r.operation_id,meeting.owner_user_id,meeting.id,r.deck_revision,r.slide_id,intent])?;
        }
        anyhow::ensure!(
            meeting.state == wire::MeetingState::Preparing
                && meeting.revision == r.expected_revision,
            "narration requires the current preparing revision"
        );
        anyhow::ensure!(
            meeting
                .slides
                .iter()
                .find(|s| s.id == r.slide_id)
                .unwrap()
                .audio
                .is_none(),
            "slide is already narrated"
        );
        tx.commit()?;
        core_tx.commit()?;
        text
    };
    // The only producer input is the previously authorized stored slide body.
    let output = match producer(&SpeechRequest {
        text: text.clone(),
        format: SpeechAudioFormat::Wav,
        voice_id: None,
    }) {
        Ok(output) => output,
        Err(error) => {
            let known = matches!(
                error,
                SpeechError::ConfigurationUnavailable
                    | SpeechError::MissingCredential
                    | SpeechError::MissingVoice
                    | SpeechError::InvalidRequest
                    | SpeechError::UnsupportedBackend
            );
            store::open_store(root)?.execute("UPDATE workjet_jour_fixe_native_narration SET state=?2,error_class=?3 WHERE operation_id=?1 AND state='reserved'",params![r.operation_id,if known {"failed_prerequisite"} else {"uncertain"},serde_json::to_string(&error)?])?;
            anyhow::bail!("native narration readiness failure: {error}; no audio was published");
        }
    };
    anyhow::ensure!(
        matches!(output.format(), SpeechAudioFormat::Wav)
            && (44..=8 * 1024 * 1024).contains(&output.audio().len())
            && output.text_sha256() == r.narration_text_sha256
            && output.audio_sha256() == hash(output.audio())
            && output.input_characters() == text.chars().count()
            && !output.run_id().is_empty()
            && !output.model().trim().is_empty(),
        "native speech producer output differs from requested slide"
    );
    let duration = custody::wav_duration(output.audio())?;
    anyhow::ensure!(
        (1..=300000).contains(&duration),
        "native narration duration exceeds meeting budget"
    );
    let (result, refs) = {
        let (mut core, mut policy) = writing(root)?;
        let core_tx = core.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let tx = policy.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (mut meeting, current_text) = current(&core_tx, &tx, context, trusted, &r)?;
        anyhow::ensure!(
            current_text == text
                && meeting.revision == r.expected_revision
                && meeting.state == wire::MeetingState::Preparing,
            "narration authority/deck changed during synthesis"
        );
        let owner:String=tx.query_row("SELECT owner_user_id FROM workjet_jour_fixe_native_narration WHERE operation_id=?1 AND intent_hash=?2 AND state='reserved'",params![r.operation_id,intent],|row|row.get(0))?;
        anyhow::ensure!(
            owner == meeting.owner_user_id,
            "native narration owner changed"
        );
        let instance = store::existing_instance_id(root)?;
        let file_id = format!(
            "workjet_audio_{:.32}",
            hash(
                format!(
                    "{instance}:{}:{}:{}:{}:{}",
                    meeting.owner_user_id,
                    meeting.id,
                    r.deck_revision,
                    r.slide_id,
                    output.audio_sha256()
                )
                .as_bytes()
            )
        );
        let generation = format!(
            "native_audio_{:.32}",
            hash(format!("{file_id}:{}", output.audio_sha256()).as_bytes())
        );
        let now = store::now_ms() as i64;
        let encoded = base64::engine::general_purpose::STANDARD.encode(output.audio());
        let total = encoded.len().div_ceil(16384);
        let mut refs = Vec::with_capacity(total + 1);
        refs.push(custody::persist(&tx,"desktop_files",&file_id,now,json!({"id":file_id,"name":format!("{}.wav",r.slide_id),"kind":"file","mime_type":"audio/wav","extension":"wav","size_bytes":output.audio().len(),"owner_id":meeting.owner_user_id,"source":"ctox-jour-fixe-native-audio","linked_collection":"workjet_jour_fixe_meetings","linked_record_id":meeting.id,"content_state":"available","content_hash":output.audio_sha256(),"content_hash_scheme":"sha256-bytes-v1","content_generation_id":generation,"is_deleted":false,"created_at_ms":now,"updated_at_ms":now}))?);
        for (idx, chunk) in encoded.as_bytes().chunks(16384).enumerate() {
            let data = std::str::from_utf8(chunk)?;
            let id = format!("{file_id}_{generation}_{idx:06}");
            refs.push(custody::persist(&tx,"desktop_file_chunks",&id,now,json!({"id":id,"file_id":file_id,"generation_id":generation,"content_hash":output.audio_sha256(),"content_hash_scheme":"sha256-bytes-v1","idx":idx,"total":total,"encoding":"base64","data":data,"chunk_hash":hash(data.as_bytes()),"chunk_hash_scheme":"sha256-base64-chunk-v1","size_bytes":data.len(),"created_at_ms":now}))?);
        }
        let audio = wire::AudioRef {
            file_id,
            sha256: output.audio_sha256().into(),
            mime_type: "audio/wav".into(),
            duration_ms: duration,
            narration_text_sha256: output.text_sha256().into(),
            source_run_id: output.run_id().into(),
            model: output.model().into(),
            format: "wav".into(),
            synthesis_duration_ms: output.elapsed_ms(),
            provenance: Some(wire::AudioProvenance::NativeGateway),
            generation_id: Some(generation),
        };
        audio.validate().map_err(anyhow::Error::msg)?;
        let slide = meeting
            .slides
            .iter_mut()
            .find(|s| s.id == r.slide_id)
            .context("slide missing")?;
        anyhow::ensure!(slide.audio.is_none(), "native slide narration was replaced");
        slide.audio = Some(audio.clone());
        if meeting.slides.iter().all(|s| s.audio.is_some()) {
            meeting.state = wire::MeetingState::Ready;
        }
        meeting.revision = meeting
            .revision
            .checked_add(1)
            .context("meeting revision overflow")?;
        meeting.validate().map_err(anyhow::Error::msg)?;
        let raw = serde_json::to_string(&meeting)?;
        anyhow::ensure!(
            raw.len() <= 1024 * 1024,
            "meeting metadata exceeds native budget"
        );
        let narration = wire::NativeNarrationReceipt {
            operation_id: r.operation_id.clone(),
            instance_id: instance,
            project_id: meeting.project_id.clone(),
            meeting_id: meeting.id.clone(),
            slide_id: r.slide_id.clone(),
            deck_revision: meeting.deck_revision,
            owner_user_id: meeting.owner_user_id.clone(),
            revision: meeting.revision,
            audio,
            persisted_at_ms: now,
            provenance: wire::AudioProvenance::NativeGateway,
            provider_verified: true,
        };
        narration.validate().map_err(anyhow::Error::msg)?;
        let mutation = wire::MeetingMutationReceipt {
            operation_id: r.operation_id.clone(),
            meeting_id: meeting.id.clone(),
            project_id: meeting.project_id.clone(),
            revision: meeting.revision,
            state: meeting.state,
            changed_id: Some(r.slide_id.clone()),
            todos_revision: None,
        };
        let result = json!({"ok":true,"contract":wire::CONTRACT_SCHEMA,"mutation":mutation,"native_narration":narration});
        tx.execute(
            "UPDATE workjet_jour_fixe_meetings SET metadata_json=?2 WHERE meeting_id=?1",
            params![meeting.id, raw],
        )?;
        tx.execute("UPDATE workjet_jour_fixe_native_narration SET state='complete',receipt_json=?2,projections_json=?3 WHERE operation_id=?1",params![r.operation_id,result.to_string(),serde_json::to_string(&refs)?])?;
        tx.commit()?;
        core_tx.commit()?;
        (result, serde_json::to_value(refs)?)
    };
    // The authoritative copy and operation receipt are already atomic. A failed
    // projection can replay them without invoking the speech producer again.
    project_custody(root, &refs)?;
    Ok(result)
}

#[cfg(test)]
#[path = "mcp_workjet_narration_tests.rs"]
mod tests;
