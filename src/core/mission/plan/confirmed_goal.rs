// Origin: CTOX
// License: AGPL-3.0-only
//! Exact, already-confirmed steps in the existing durable Core plan store.
//! Policy/admission belong to the caller. This API never parses a model plan,
//! dispatches a worker, commits its caller's transaction, or writes another DB.
use super::*;

pub(crate) struct ConfirmedStep {
    pub id: String,
    pub title: String,
    pub instruction: String,
}

pub(crate) fn open(root: &Path) -> Result<Connection> { open_plan_db(root) }
pub(crate) fn committed(root: &Path) -> Result<()> { touch_plan_state_stamp(root) }

pub(crate) fn insert(
    tx: &Transaction<'_>, goal_id: &str, title: &str, prompt: &str,
    thread_key: &str, steps: &[ConfirmedStep], previous_goal: Option<&str>,
) -> Result<()> {
    anyhow::ensure!(!goal_id.trim().is_empty() && goal_id.len() <= 128
        && !title.trim().is_empty() && !thread_key.trim().is_empty()
        && !steps.is_empty() && steps.len() <= 100,
        "confirmed goal requires identity, registered thread and bounded exact steps");
    let mut ids = HashSet::new();
    for step in steps {
        anyhow::ensure!(!step.id.trim().is_empty() && ids.insert(step.id.as_str())
            && !step.title.trim().is_empty() && !step.instruction.trim().is_empty(),
            "confirmed goal step is empty or duplicated");
    }
    let now = now_iso_string();
    supersede_active_goals_for_thread_key_tx(tx,thread_key,goal_id,&now)?;
    if let Some(previous) = previous_goal {
        // A registered Supervisor may have been rebound. Only the previous
        // project definition identified by the native caller is superseded.
        tx.execute("UPDATE planned_goals SET status='superseded',updated_at=?2
            WHERE goal_id=?1 AND status='active'",params![previous,now])?;
    }
    tx.execute("INSERT INTO planned_goals
        (goal_id,title,source_prompt,thread_key,skill,auto_advance,status,created_at,updated_at)
        VALUES (?1,?2,?3,?4,NULL,1,'active',?5,?5)",params![goal_id,title,prompt,thread_key,now])?;
    for (index,step) in steps.iter().enumerate() {
        tx.execute("INSERT INTO planned_steps
            (step_id,goal_id,step_order,title,instruction,status,defer_until,blocked_reason,
             attempt_count,last_message_key,last_result_excerpt,created_at,updated_at,completed_at)
            VALUES (?1,?2,?3,?4,?5,'pending',NULL,NULL,0,NULL,NULL,?6,?6,NULL)",
            params![format!("{goal_id}::{}",step.id),goal_id,index as i64+1,step.title,step.instruction,now])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn confirmed_steps_are_real_plans_and_rollback_with_the_callers_transaction() -> Result<()> {
        let root=tempfile::tempdir()?;
        let mut conn=open(root.path())?;
        let steps=[ConfirmedStep{id:"todo-1".into(),title:"Prove persistence".into(),
            instruction:"Owner: Michael; acceptance: reopening preserves the answer; due: 2026-10-12".into()}];
        {
            let tx=conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            insert(&tx,"rolled-back","Draft","Never accepted","supervisor/one",&steps,None)?;
        }
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM planned_goals",[],|r|r.get::<_,i64>(0))?,0);
        let tx=conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        insert(&tx,"accepted","Confirmed","Exact todos","supervisor/one",&steps,None)?;
        tx.commit()?;committed(root.path())?;
        let goal=load_goal_with_steps(root.path(),"accepted")?.context("real goal missing")?;
        assert_eq!(goal.goal.thread_key,"supervisor/one");
        assert!(goal.goal.auto_advance);
        assert_eq!(goal.steps.len(),1);
        assert_eq!(goal.steps[0].instruction,steps[0].instruction);
        assert!(goal.steps[0].defer_until.is_none(),"a due date is not a wait-until date");
        Ok(())
    }
    #[test]
    fn confirmed_goal_replaces_only_its_registered_thread_and_native_previous_definition() -> Result<()> {
        let root=tempfile::tempdir()?;let mut conn=open(root.path())?;
        let step=[ConfirmedStep{id:"one".into(),title:"One".into(),instruction:"Explicit acceptance".into()}];
        let tx=conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        for (id,key) in [("foreign","supervisor/foreign"),("previous","supervisor/old"),("same-thread","supervisor/new")] {
            insert(&tx,id,id,id,key,&step,None)?;
        }
        insert(&tx,"new","New","New","supervisor/new",&step,Some("previous"))?;
        tx.commit()?;
        for (id,status) in [("foreign","active"),("previous","superseded"),("same-thread","superseded"),("new","active")] {
            assert_eq!(conn.query_row("SELECT status FROM planned_goals WHERE goal_id=?1",[id],|r|r.get::<_,String>(0))?,status);
        }
        Ok(())
    }
}
