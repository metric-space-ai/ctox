// Origin: CTOX
// License: AGPL-3.0-only
//! Owner meeting edits, committed with their native domain application receipt.
//! These controls do not manufacture narration, STT provenance or confirmed goals.
use super::super::{workjet_identity, workjet_jour_fixe_contract as wire};
use super::*;
use crate::business_os::store;
use rusqlite::{params, OptionalExtension};
use wire::WireValidate;
const OPERATIONS: &str = "CREATE TABLE IF NOT EXISTS workjet_jour_fixe_owner_operations (
 operation_id TEXT PRIMARY KEY, owner_user_id TEXT NOT NULL,
 intent_hash TEXT NOT NULL, receipt_json TEXT NOT NULL
);";
const MAX_METADATA_BYTES: usize = 1024 * 1024;

pub(in crate::business_os) fn is_command(kind: &str) -> bool {
    matches!(
        kind,
        "ctox.workjet.jour_fixe.meeting.start"
            | "ctox.workjet.jour_fixe.meeting.end"
            | "ctox.workjet.jour_fixe.transcript.append"
            | "ctox.workjet.jour_fixe.todos.revise"
            | "ctox.workjet.jour_fixe.comment.add"
    )
}
// A declared meeting tool without a handler must fail terminally, never fall
// through into an ordinary model task or recursively queue another preparation.
pub(in crate::business_os) fn is_reserved_command(kind: &str) -> bool {
    matches!(
        kind,
        "ctox.workjet.jour_fixe.prepare"
            | "ctox.workjet.jour_fixe.deck.publish"
            | "ctox.workjet.jour_fixe.todos.propose"
            | "ctox.workjet.jour_fixe.todos.confirm"
    )
}
enum Edit {
    Start(wire::MeetingTransitionRequest),
    End(wire::MeetingTransitionRequest),
    Text(wire::AppendTranscriptRequest),
    Comment(wire::AddCommentRequest),
    Revise(wire::ProposeTodosRequest),
}
impl Edit {
    fn identity(&self) -> (&str, &str, u64) {
        match self {
            Self::Start(v) | Self::End(v) => (
                v.operation_id.as_str(),
                v.meeting_id.as_str(),
                v.expected_revision,
            ),
            Self::Text(v) => (
                v.operation_id.as_str(),
                v.meeting_id.as_str(),
                v.expected_revision,
            ),
            Self::Comment(v) => (
                v.operation_id.as_str(),
                v.meeting_id.as_str(),
                v.expected_revision,
            ),
            Self::Revise(v) => (
                v.operation_id.as_str(),
                v.meeting_id.as_str(),
                v.expected_revision,
            ),
        }
    }
}
fn parse(command: &BusinessCommand) -> anyhow::Result<(Edit, Value)> {
    let mut payload = command.payload.clone();
    let object = payload
        .as_object_mut()
        .context("meeting edit must be an object")?;
    if let Some(channel) = object.remove("inbound_channel") {
        let value = channel.as_str().context("inbound_channel must be text")?;
        ensure!(
            !value.trim().is_empty() && value.chars().count() <= 256,
            "invalid inbound_channel"
        );
    }
    let edit = match command.command_type.as_str() {
        "ctox.workjet.jour_fixe.meeting.start" => {
            let v: wire::MeetingTransitionRequest = serde_json::from_value(payload.clone())?;
            v.validate().map_err(anyhow::Error::msg)?;
            Edit::Start(v)
        }
        "ctox.workjet.jour_fixe.meeting.end" => {
            let v: wire::MeetingTransitionRequest = serde_json::from_value(payload.clone())?;
            v.validate().map_err(anyhow::Error::msg)?;
            Edit::End(v)
        }
        "ctox.workjet.jour_fixe.transcript.append" => {
            let v: wire::AppendTranscriptRequest = serde_json::from_value(payload.clone())?;
            v.validate().map_err(anyhow::Error::msg)?;
            Edit::Text(v)
        }
        "ctox.workjet.jour_fixe.comment.add" => {
            let v: wire::AddCommentRequest = serde_json::from_value(payload.clone())?;
            v.validate().map_err(anyhow::Error::msg)?;
            Edit::Comment(v)
        }
        "ctox.workjet.jour_fixe.todos.revise" => {
            let v: wire::ProposeTodosRequest = serde_json::from_value(payload.clone())?;
            v.validate().map_err(anyhow::Error::msg)?;
            Edit::Revise(v)
        }
        _ => anyhow::bail!("unsupported owner meeting edit"),
    };
    let (operation, meeting, _) = edit.identity();
    ensure!(
        operation.trim() == operation && meeting.trim() == meeting,
        "meeting operation identity must be canonical"
    );
    Ok((edit, payload))
}
pub(super) fn owned(
    conn: &Connection,
    actor: &str,
    project_route: Option<&str>,
    id: &str,
) -> anyhow::Result<wire::Meeting> {
    let exists:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workjet_jour_fixe_meetings')",[],|r|r.get(0))?;
    ensure!(exists, "meeting unavailable to this project owner");
    let owner = workjet_identity::owner_from_connection(conn, actor)?;
    let row:Option<(String,String,String)>=conn.query_row(
        "SELECT project_id,owner_user_id,metadata_json FROM workjet_jour_fixe_meetings WHERE meeting_id=?1",
        [id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let (project, stored_owner, raw) = row.context("meeting unavailable to this project owner")?;
    ensure!(
        stored_owner == owner,
        "meeting unavailable to this project owner"
    );
    ensure!(
        raw.len() <= MAX_METADATA_BYTES,
        "meeting metadata exceeds native read budget"
    );
    let meeting: wire::Meeting = serde_json::from_str(&raw)?;
    meeting.validate().map_err(anyhow::Error::msg)?;
    ensure!(
        meeting.id == id && meeting.project_id == project && meeting.owner_user_id == owner,
        "meeting ownership binding conflicts"
    );
    ensure!(
        project_route.is_none_or(|id| id == project),
        "meeting routing conflicts with project"
    );
    let binding = supervisor_turns::binding_from_connection(
        conn,
        &owner,
        &project,
        &meeting.supervisor.workjet_thread_id,
        true,
    )?;
    ensure!(
        binding.thread_key == meeting.supervisor.ctox_thread_key,
        "meeting supervisor binding conflicts"
    );
    Ok(meeting)
}

/// Native-only metadata binding. This is not a bearer credential and cannot be
/// reconstructed from browser JSON. Callers must first perform the existing
/// fresh native peer/session/collection authorization for every operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::business_os) struct LiveMeetingBinding {
    owner_user_id: String,
    project_id: String,
    meeting_id: String,
    supervisor_thread_id: String,
    supervisor_thread_key: String,
    deck_revision: u64,
    meeting_revision: u64,
}
impl LiveMeetingBinding {
    pub(super) fn identity(&self) -> Value {
        json!({"owner":self.owner_user_id,"project":self.project_id,"meeting":self.meeting_id,
            "supervisor_thread":self.supervisor_thread_id,"supervisor_key":self.supervisor_thread_key,
            "deck":self.deck_revision})
    }
    pub(super) fn revalidate_from_connection(
        &self,
        conn: &Connection,
        actor: &str,
    ) -> anyhow::Result<Self> {
        let meeting = owned(conn, actor, Some(&self.project_id), &self.meeting_id)?;
        ensure!(
            meeting.state == wire::MeetingState::Live
                && meeting.deck_revision == self.deck_revision
                && meeting.owner_user_id == self.owner_user_id
                && meeting.supervisor.workjet_thread_id == self.supervisor_thread_id
                && meeting.supervisor.ctox_thread_key == self.supervisor_thread_key,
            "live meeting execution binding changed"
        );
        ensure!(
            !meeting.slides.is_empty()
                && meeting
                    .slides
                    .iter()
                    .all(|slide| slide.meeting_id == meeting.id && slide.audio.is_some()),
            "live meeting deck is unavailable"
        );
        let mut current = self.clone();
        current.meeting_revision = meeting.revision;
        Ok(current)
    }
    pub(in crate::business_os) fn owner_user_id(&self) -> &str {
        &self.owner_user_id
    }
    pub(in crate::business_os) fn project_id(&self) -> &str {
        &self.project_id
    }
    pub(in crate::business_os) fn meeting_id(&self) -> &str {
        &self.meeting_id
    }
    pub(in crate::business_os) fn deck_revision(&self) -> u64 {
        self.deck_revision
    }
    pub(in crate::business_os) fn meeting_revision(&self) -> u64 {
        self.meeting_revision
    }
    /// Re-read after each awaited provider operation, without retaining a
    /// connection, read transaction or issuer fence across that operation.
    /// Conversation writes may advance meeting_revision; owner, deck, live
    /// state and the registered Supervisor binding must remain unchanged.
    pub(in crate::business_os) fn revalidate(
        &self,
        root: &Path,
        authenticated_actor: &str,
    ) -> anyhow::Result<Self> {
        let current = check_live_meeting_for_authenticated_actor(
            root,
            authenticated_actor,
            &self.project_id,
            &self.meeting_id,
            self.deck_revision,
        )?;
        ensure!(
            current.owner_user_id == self.owner_user_id
                && current.supervisor_thread_id == self.supervisor_thread_id
                && current.supervisor_thread_key == self.supervisor_thread_key,
            "live meeting execution binding changed"
        );
        Ok(current)
    }
}

/// Only current native metadata is read. The returned binding contains no
/// connection, policy decision cache, peer token, source audio or credentials.
/// This supplements normal authenticated native ingress; it never authorizes
/// a client, grants collection access, or accepts a caller-supplied speaker.
pub(in crate::business_os) fn check_live_meeting_for_authenticated_actor(
    root: &Path,
    authenticated_actor: &str,
    project_id: &str,
    meeting_id: &str,
    deck_revision: u64,
) -> anyhow::Result<LiveMeetingBinding> {
    ensure!(
        !authenticated_actor.trim().is_empty()
            && !project_id.trim().is_empty()
            && !meeting_id.trim().is_empty()
            && deck_revision > 0,
        "live meeting binding requires authenticated identity and an existing deck"
    );
    let mut conn = Connection::open_with_flags(
        store::business_os_store_path(root),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    let tx = conn.transaction()?;
    let meeting = owned(&tx, authenticated_actor, Some(project_id), meeting_id)?;
    ensure!(
        meeting.state == wire::MeetingState::Live && meeting.deck_revision == deck_revision,
        "meeting is not live at the requested deck revision"
    );
    ensure!(
        !meeting.slides.is_empty()
            && meeting
                .slides
                .iter()
                .all(|slide| slide.meeting_id == meeting.id && slide.audio.is_some()),
        "live meeting deck is unavailable"
    );
    Ok(LiveMeetingBinding {
        owner_user_id: meeting.owner_user_id,
        project_id: meeting.project_id,
        meeting_id: meeting.id,
        supervisor_thread_id: meeting.supervisor.workjet_thread_id,
        supervisor_thread_key: meeting.supervisor.ctox_thread_key,
        deck_revision: meeting.deck_revision,
        meeting_revision: meeting.revision,
    })
}

pub(in crate::business_os) fn handle(
    root: &Path,
    command: &BusinessCommand,
    actor: &str,
    admission: &DomainEffectAdmission,
) -> anyhow::Result<Value> {
    let (edit, payload) = parse(command)?;
    let (operation, id, expected) = edit.identity();
    let intent = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(
            &json!({"kind":command.command_type,"payload":payload})
        )?)
    );
    let mut conn = open_store(root)?;
    owned(&conn, actor, command.record_id.as_deref(), id)?;
    conn.execute_batch(OPERATIONS)?;
    let applied=admission.apply(&mut conn,|tx| {
        // Current project ownership and registered supervisor are checked inside
        // the same writer transaction as both metadata and the domain receipt.
        let mut meeting=owned(tx,actor,command.record_id.as_deref(),id)?;
        let old:Option<(String,String,String)>=tx.query_row(
            "SELECT owner_user_id,intent_hash,receipt_json FROM workjet_jour_fixe_owner_operations WHERE operation_id=?1",
            [operation],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if let Some((owner,hash,receipt))=old {
            ensure!(owner==meeting.owner_user_id && hash==intent,"meeting operation intent conflicts");
            return Ok(AppliedDomainEffect{result:serde_json::from_str(&receipt)?,projections:vec![]});
        }
        ensure!(meeting.revision==expected,"meeting revision changed; read the current meeting");
        let mut changed_id=None;
        let mut todos_revision=None;
        match &edit {
            Edit::Start(_)=>{
                ensure!(meeting.state==wire::MeetingState::Ready,"only a ready meeting can start");
                ensure!(meeting.deck_revision>0 && !meeting.slides.is_empty(),"meeting has no published deck");
                ensure!(meeting.slides.iter().all(|s|s.meeting_id==meeting.id && s.audio.is_some()),
                    "meeting narration is not ready");
                meeting.state=wire::MeetingState::Live;
            },
            Edit::End(_)=>{
                ensure!(meeting.state==wire::MeetingState::Live,"only a live meeting can enter review");
                meeting.state=wire::MeetingState::Review;
            },
            Edit::Text(request)=>{
                ensure!(matches!(meeting.state,wire::MeetingState::Live|wire::MeetingState::Review),"meeting does not accept conversation");
                let turn=&request.turn;
                ensure!(turn.meeting_id==meeting.id && turn.speaker==wire::Speaker::Owner,
                    "owner transcript cannot claim another speaker or meeting");
                if turn.modality==wire::Modality::Speech {
                    super::jour_fixe_speech::consume_in_transaction(tx,&meeting,command,&payload)?;
                } else {
                    ensure!(turn.audio.is_none() && turn.source_run_id.is_none() && turn.stream_id.is_none()
                        && turn.sentence_end_latency_ms.is_none(),"owner text cannot claim speech provenance");
                }
                ensure!(turn.ended_at_ms>=turn.started_at_ms,"transcript time range is reversed");
                let next=meeting.transcript.last().map_or(Ok(1),|v|v.sequence.checked_add(1).context("transcript sequence overflow"))?;
                ensure!(turn.sequence==next && meeting.transcript.iter().all(|v|v.id!=turn.id),"transcript sequence or identity conflicts");
                changed_id=Some(turn.id.clone());
                meeting.transcript.push(turn.clone());
            },
            Edit::Comment(request)=>{
                ensure!(matches!(meeting.state,wire::MeetingState::Live|wire::MeetingState::Review),
                    "meeting does not accept slide comments");
                ensure!(request.deck_revision>0 && request.deck_revision==meeting.deck_revision
                    && meeting.slides.iter().any(|slide|slide.id==request.slide_id
                        && slide.meeting_id==meeting.id),"comment slide or deck revision conflicts");
                ensure!(!request.text.trim().is_empty()
                    && meeting.comments.iter().all(|comment|comment.id!=request.comment_id)
                    && meeting.slides.iter().all(|slide|slide.id!=request.comment_id)
                    && meeting.transcript.iter().all(|turn|turn.id!=request.comment_id),
                    "comment text or meeting evidence identity conflicts");
                let comment=wire::Comment {
                    id:request.comment_id.clone(),slide_id:request.slide_id.clone(),
                    deck_revision:meeting.deck_revision,x:request.x,y:request.y,text:request.text.clone(),
                    author_user_id:meeting.owner_user_id.clone(),created_at_ms:chrono::Utc::now().timestamp_millis(),
                    supervisor_event_id:None,meeting_id:meeting.id.clone(),
                };
                comment.validate().map_err(anyhow::Error::msg)?;
                changed_id=Some(comment.id.clone());meeting.comments.push(comment);
            },
            Edit::Revise(request)=>{
                ensure!(meeting.state==wire::MeetingState::Review,"meeting is not in review");
                let current=meeting.todos.as_ref().context("supervisor has not proposed todos")?;
                ensure!(current.status==wire::TodoState::Proposed && request.proposal_revision==current.revision.checked_add(1).context("todo revision overflow")?,"todo proposal revision changed");
                let mut ids=std::collections::BTreeSet::new();
                let evidence:std::collections::BTreeSet<&str>=meeting.slides.iter().map(|v|v.id.as_str())
                    .chain(meeting.comments.iter().map(|v|v.id.as_str()))
                    .chain(meeting.transcript.iter().map(|v|v.id.as_str())).collect();
                for todo in &request.items {
                    ensure!(ids.insert(&todo.id) && todo.evidence_ids.iter().all(|id|evidence.contains(id.as_str())),"todo identity or meeting evidence conflicts");
                }
                todos_revision=Some(request.proposal_revision);
                meeting.todos=Some(wire::TodoList {
                    revision:request.proposal_revision,status:wire::TodoState::Proposed,
                    items:request.items.clone(),confirmed_by_user_id:None,confirmed_at_ms:None,goal:None,
                    meeting_id:meeting.id.clone(),
                });
            },
        }
        meeting.revision=meeting.revision.checked_add(1).context("meeting revision overflow")?;
        meeting.validate().map_err(anyhow::Error::msg)?;
        let metadata=serde_json::to_string(&meeting)?;
        ensure!(metadata.len()<=MAX_METADATA_BYTES,"meeting metadata exceeds native write budget");
        let receipt=wire::MeetingMutationReceipt {
            operation_id:operation.to_owned(),meeting_id:meeting.id.clone(),project_id:meeting.project_id.clone(),
            revision:meeting.revision,state:meeting.state.clone(),changed_id,todos_revision,
        };
        receipt.validate().map_err(anyhow::Error::msg)?;
        let result=json!({"ok":true,"contract":wire::CONTRACT_SCHEMA,"mutation":receipt});
        tx.execute("UPDATE workjet_jour_fixe_meetings SET metadata_json=?2 WHERE meeting_id=?1",params![id,metadata])?;
        tx.execute("INSERT INTO workjet_jour_fixe_owner_operations VALUES (?1,?2,?3,?4)",params![operation,meeting.owner_user_id,intent,serde_json::to_string(&result)?])?;
        Ok(AppliedDomainEffect{result,projections:vec![]})
    })?;
    Ok(applied.result)
}
