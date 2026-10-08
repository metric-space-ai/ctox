// Origin: CTOX
// License: AGPL-3.0-only
//! Owner confirmation is a Core effect: exact plans, confirmed meeting snapshot
//! and application receipt share ONE transaction. Policy is only revalidated
//! under its writer reservation; no two-WAL-database atomicity is assumed.
use super::*;
use super::super::{domain_effect, workjet_jour_fixe_contract as wire};
use crate::mission::plan::confirmed_goal;
use rusqlite::{params,OptionalExtension,OpenFlags,TransactionBehavior};
use wire::WireValidate;
const MAX_METADATA_BYTES:usize=1024*1024;
const SCHEMA:&str="
CREATE TABLE IF NOT EXISTS workjet_jour_fixe_confirmations (
 meeting_id TEXT PRIMARY KEY, project_id TEXT NOT NULL, owner_user_id TEXT NOT NULL,
 operation_id TEXT NOT NULL UNIQUE, intent_hash TEXT NOT NULL, goal_id TEXT NOT NULL UNIQUE,
 goal_revision INTEGER NOT NULL, metadata_json TEXT NOT NULL, receipt_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS workjet_project_goal_definitions (
 project_id TEXT PRIMARY KEY, owner_user_id TEXT NOT NULL, supervisor_thread_key TEXT NOT NULL,
 goal_id TEXT NOT NULL, revision INTEGER NOT NULL, scheduled_at_ms INTEGER NOT NULL
);";

fn has_schema(conn:&Connection)->anyhow::Result<bool> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workjet_jour_fixe_confirmations')",[],|r|r.get(0))?)
}
fn reader(root:&Path)->anyhow::Result<Option<Connection>> {
    let path=crate::paths::core_db(root);
    if !path.exists(){return Ok(None)}
    let conn=Connection::open_with_flags(path,OpenFlags::SQLITE_OPEN_READ_ONLY|OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    Ok(Some(conn))
}

pub(in crate::business_os) fn overlay_from_core(core:&Connection, draft:wire::Meeting)->anyhow::Result<wire::Meeting> {
    if !has_schema(core)? {return Ok(draft)}
    let row:Option<(String,String)>=core.query_row(
        "SELECT metadata_json,goal_id FROM workjet_jour_fixe_confirmations WHERE meeting_id=?1",
        [&draft.id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let Some((raw,goal_id))=row else{return Ok(draft)};
    ensure!(raw.len()<=MAX_METADATA_BYTES,"confirmed meeting exceeds native read budget");
    let confirmed:wire::Meeting=serde_json::from_str(&raw)?;
    confirmed.validate().map_err(anyhow::Error::msg)?;
    ensure!(confirmed.id==draft.id && confirmed.project_id==draft.project_id
        && confirmed.owner_user_id==draft.owner_user_id
        && confirmed.supervisor.workjet_thread_id==draft.supervisor.workjet_thread_id
        && confirmed.supervisor.ctox_thread_key==draft.supervisor.ctox_thread_key
        && confirmed.revision>draft.revision && confirmed.state==wire::MeetingState::Confirmed,
        "confirmed Core meeting binding conflicts");
    let todos=confirmed.todos.as_ref().context("confirmed Core todos missing")?;
    ensure!(todos.status==wire::TodoState::Confirmed
        && todos.goal.as_ref().is_some_and(|goal|goal.goal_id==goal_id),"confirmed Core goal reference conflicts");
    let thread:Option<String>=core.query_row("SELECT thread_key FROM planned_goals WHERE goal_id=?1",[goal_id],|r|r.get(0)).optional()?;
    ensure!(thread.as_deref()==Some(confirmed.supervisor.ctox_thread_key.as_str()),"confirmed real Core goal is unavailable");
    Ok(confirmed)
}
pub(in crate::business_os) fn overlay(root:&Path,draft:wire::Meeting)->anyhow::Result<wire::Meeting> {
    match reader(root)? {Some(core)=>overlay_from_core(&core,draft),None=>Ok(draft)}
}

pub(in crate::business_os) fn previous_goal(root:&Path,owner:&str,project:&str,thread_key:&str)
    ->anyhow::Result<Option<wire::GoalRef>> {
    let Some(core)=reader(root)? else{return Ok(None)};
    current_goal(&core,owner,project,thread_key)
}
fn current_goal(core:&Connection,owner:&str,project:&str,_thread_key:&str)->anyhow::Result<Option<wire::GoalRef>> {
    if !has_schema(core)? {return Ok(None)}
    let row:Option<(String,String,String,u64)>=core.query_row(
        "SELECT d.owner_user_id,d.supervisor_thread_key,d.goal_id,d.revision
         FROM workjet_project_goal_definitions d WHERE d.project_id=?1",[project],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let Some((stored_owner,stored_thread,goal_id,revision))=row else{return Ok(None)};
    ensure!(stored_owner==owner,"project goal owner changed");
    let actual:Option<String>=core.query_row("SELECT thread_key FROM planned_goals WHERE goal_id=?1",
        [&goal_id],|r|r.get(0)).optional()?;
    ensure!(actual.as_deref()==Some(stored_thread.as_str()),"real project goal is unavailable");
    // A current, revalidated Supervisor binding may read its project's previous
    // definition after rebinding; this never reads another project's history.
    Ok(Some(wire::GoalRef{goal_id,revision}))
}

/// The next deck reads the accepted definition and real plan progress, never a
/// caller-supplied goal label or a guessed completion count.
pub(in crate::business_os) fn goal_for_deck(core:&Connection,meeting:&wire::Meeting)->anyhow::Result<Value> {
    let Some(reference)=meeting.previous_goal.as_ref() else{return Ok(Value::Null)};
    ensure!(has_schema(core)?,"previous Core goal confirmation is unavailable");
    let (owner,project,revision,raw):(String,String,u64,String)=core.query_row(
        "SELECT owner_user_id,project_id,goal_revision,metadata_json FROM workjet_jour_fixe_confirmations WHERE goal_id=?1",
        [&reference.goal_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
    ensure!(owner==meeting.owner_user_id && project==meeting.project_id && revision==reference.revision
        && raw.len()<=MAX_METADATA_BYTES,"previous goal belongs to another project or revision");
    let previous:wire::Meeting=serde_json::from_str(&raw)?;
    previous.validate().map_err(anyhow::Error::msg)?;
    let todos=previous.todos.context("previous confirmed todos unavailable")?;
    ensure!(todos.status==wire::TodoState::Confirmed && todos.goal.as_ref().is_some_and(|goal|
        goal.goal_id==reference.goal_id && goal.revision==reference.revision),"previous goal proof differs");
    let status:String=core.query_row("SELECT status FROM planned_goals WHERE goal_id=?1",[&reference.goal_id],|r|r.get(0))?;
    let mut statement=core.prepare("SELECT step_id,title,status,substr(last_result_excerpt,1,420)
        FROM planned_steps WHERE goal_id=?1 ORDER BY step_order LIMIT 101")?;
    let steps=statement.query_map([&reference.goal_id],|r|Ok(json!({"id":r.get::<_,String>(0)?,
        "title":r.get::<_,String>(1)?,"status":r.get::<_,String>(2)?,"result_excerpt":r.get::<_,Option<String>>(3)?})))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(steps.len()==todos.items.len(),"previous goal steps differ from the confirmed definition");
    let result=json!({"goal":reference,"status":status,"items":todos.items,"steps":steps});
    ensure!(serde_json::to_vec(&result)?.len()<=MAX_METADATA_BYTES,"previous goal exceeds deck read budget");
    Ok(result)
}
pub(in crate::business_os) fn previous_goal_for_deck(root:&Path,meeting:&wire::Meeting)->anyhow::Result<Value> {
    match reader(root)? {
        Some(core)=>goal_for_deck(&core,meeting),
        None if meeting.previous_goal.is_none()=>Ok(Value::Null),
        None=>anyhow::bail!("previous Core goal is unavailable"),
    }
}

pub(in crate::business_os) fn handle(root:&Path,command:&BusinessCommand,actor:&str,
    admission:&DomainEffectAdmission)->anyhow::Result<Value> {
    let mut payload=command.payload.clone();
    let object=payload.as_object_mut().context("todo confirmation must be an object")?;
    if let Some(channel)=object.remove("inbound_channel") {
        let text=channel.as_str().context("inbound_channel must be text")?;
        ensure!(!text.trim().is_empty()&&text.chars().count()<=256,"invalid inbound_channel");
    }
    let request:wire::ConfirmTodosRequest=serde_json::from_value(payload.clone())?;
    request.validate().map_err(anyhow::Error::msg)?;
    ensure!(request.operation_id.trim()==request.operation_id && request.meeting_id.trim()==request.meeting_id,
        "confirmation identity must be canonical");
    let intent=format!("{:x}",Sha256::digest(serde_json::to_vec(&json!({"kind":command.command_type,"payload":payload}))?));
    // Initialize the existing Core plan schema before opening either writer
    // transaction. Lock order is Core -> Policy, including ordinary owner edits.
    let mut core=confirmed_goal::open(root)?;
    core.execute_batch(SCHEMA)?;core.execute_batch(domain_effect::SCHEMA)?;
    let mut policy=open_store(root)?;
    let core_tx=core.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let policy_tx=policy.transaction_with_behavior(TransactionBehavior::Immediate)?;
    admission.validate_core_claim(&core_tx)?;
    let draft=super::jour_fixe_owner::owned(&policy_tx,actor,command.record_id.as_deref(),&request.meeting_id)?;
    let current=overlay_from_core(&core_tx,draft)?;
    let applied=admission.apply_in_transaction(&core_tx,|tx| {
        // Operation replay is authorized against the CURRENT project binding
        // above, then returns the original proof without reactivating its plan.
        let old:Option<(String,String,String,String)>=tx.query_row(
            "SELECT meeting_id,owner_user_id,intent_hash,receipt_json FROM workjet_jour_fixe_confirmations WHERE operation_id=?1",
            [&request.operation_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        if let Some((meeting,owner,hash,receipt))=old {
            ensure!(meeting==current.id && owner==current.owner_user_id && hash==intent,"confirmation operation intent conflicts");
            return Ok(AppliedDomainEffect{result:serde_json::from_str(&receipt)?,projections:vec![]});
        }
        let mut meeting=current.clone();
        ensure!(meeting.state==wire::MeetingState::Review && meeting.revision==request.expected_revision,
            "meeting revision changed or is not in review");
        let todos=meeting.todos.as_ref().context("supervisor has not proposed todos")?;
        ensure!(todos.status==wire::TodoState::Proposed && todos.revision==request.proposal_revision
            && !todos.items.is_empty(),"todo proposal changed or has no executable steps");
        ensure!(meeting.slides.iter().all(|v|v.meeting_id==meeting.id)
            && meeting.comments.iter().all(|v|v.meeting_id==meeting.id)
            && meeting.transcript.iter().all(|v|v.meeting_id==meeting.id),"meeting evidence provenance differs");
        let evidence:std::collections::BTreeSet<&str>=meeting.slides.iter().map(|v|v.id.as_str())
            .chain(meeting.comments.iter().map(|v|v.id.as_str()))
            .chain(meeting.transcript.iter().map(|v|v.id.as_str())).collect();
        let mut ids=std::collections::BTreeSet::new();
        for todo in &todos.items {
            ensure!(ids.insert(&todo.id) && !todo.title.trim().is_empty() && !todo.acceptance.trim().is_empty()
                && todo.owner.as_ref().is_some_and(|owner|!owner.trim().is_empty())
                && todo.evidence_ids.iter().all(|id|evidence.contains(id.as_str())),
                "todo owner, acceptance, identity or meeting evidence is unavailable");
        }
        let head:Option<(String,String,u64,i64)>=tx.query_row(
            "SELECT owner_user_id,goal_id,revision,scheduled_at_ms FROM workjet_project_goal_definitions WHERE project_id=?1",
            [&meeting.project_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        let previous_revision=head.as_ref().map_or(0,|v|v.2);
        ensure!(request.expected_goal_revision==previous_revision,"project goal revision changed");
        if let Some((owner,_,_,scheduled))=&head {
            ensure!(*owner==meeting.owner_user_id && meeting.scheduled_at_ms>*scheduled,
                "project goal owner changed or a later meeting is already confirmed");
        }
        let revision=previous_revision.checked_add(1).context("project goal revision overflow")?;
        let goal_id=stable_id("workjet_goal",&[&meeting.owner_user_id,&meeting.project_id,&meeting.id]);
        let source=json!({"schema":"ctox.workjet.confirmed_goal.v1","project_id":meeting.project_id,
            "meeting_id":meeting.id,"proposal_revision":todos.revision,"items":todos.items});
        let steps=todos.items.iter().map(|todo|Ok(confirmed_goal::ConfirmedStep {
            id:todo.id.clone(),title:todo.title.clone(),
            instruction:format!("Complete this explicitly confirmed JourFix task. Preserve its owner, due date and acceptance criteria. Report evidence through the existing Core completion path. {}",serde_json::to_string(todo)?),
        })).collect::<anyhow::Result<Vec<_>>>()?;
        confirmed_goal::insert(tx,&goal_id,&format!("Confirmed JourFix tasks · {}",meeting.project_id),
            &serde_json::to_string(&source)?,&meeting.supervisor.ctox_thread_key,&steps,head.as_ref().map(|v|v.1.as_str()))?;
        let goal=wire::GoalRef{goal_id:goal_id.clone(),revision};
        let list=meeting.todos.as_mut().context("todo list disappeared")?;
        list.status=wire::TodoState::Confirmed;list.confirmed_by_user_id=Some(meeting.owner_user_id.clone());
        list.confirmed_at_ms=Some(chrono::Utc::now().timestamp_millis());list.goal=Some(goal.clone());
        meeting.state=wire::MeetingState::Confirmed;
        meeting.revision=meeting.revision.checked_add(1).context("meeting revision overflow")?;
        meeting.validate().map_err(anyhow::Error::msg)?;
        let metadata=serde_json::to_string(&meeting)?;
        ensure!(metadata.len()<=MAX_METADATA_BYTES,"confirmed meeting exceeds native write budget");
        let receipt=wire::MeetingMutationReceipt{operation_id:request.operation_id.clone(),meeting_id:meeting.id.clone(),
            project_id:meeting.project_id.clone(),revision:meeting.revision,state:meeting.state.clone(),
            changed_id:Some(goal_id.clone()),todos_revision:Some(request.proposal_revision)};
        receipt.validate().map_err(anyhow::Error::msg)?;
        let result=json!({"ok":true,"contract":wire::CONTRACT_SCHEMA,"mutation":receipt,"goal":goal});
        tx.execute("INSERT INTO workjet_jour_fixe_confirmations VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![meeting.id,meeting.project_id,meeting.owner_user_id,request.operation_id,intent,goal_id,revision,metadata,serde_json::to_string(&result)?])?;
        tx.execute("INSERT INTO workjet_project_goal_definitions VALUES (?1,?2,?3,?4,?5,?6)
            ON CONFLICT(project_id) DO UPDATE SET owner_user_id=excluded.owner_user_id,
            supervisor_thread_key=excluded.supervisor_thread_key,goal_id=excluded.goal_id,
            revision=excluded.revision,scheduled_at_ms=excluded.scheduled_at_ms",
            params![meeting.project_id,meeting.owner_user_id,meeting.supervisor.ctox_thread_key,goal_id,revision,meeting.scheduled_at_ms])?;
        Ok(AppliedDomainEffect{result,projections:vec![]})
    })?;
    // Policy has no mutation to commit. Hold its reservation until Core's
    // single authoritative commit completes, then release both before I/O.
    core_tx.commit()?;drop(policy_tx);drop(policy);drop(core);
    confirmed_goal::committed(root)?;
    Ok(applied.result)
}

