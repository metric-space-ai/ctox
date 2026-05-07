//! Report-run state machine.
//!
//! The state field on `report_runs.status` is checked here, not by string
//! comparison sprinkled across stage modules. Stages call
//! [`require_at_least`] to assert the run has reached a precondition stage,
//! and [`advance_to`] to move the run forward atomically with their own
//! mutations.

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use rusqlite::params;
use rusqlite::Connection;

/// Iteration cap for the critique/revise loop. A run that needs more than
/// this many revisions is structurally broken; the operator must abort or
/// re-scope rather than spin.
pub const MAX_REVISE_ITERATIONS: i64 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Created,
    Scoped,
    Exploring,
    Framing,
    Enumerating,
    Gathering,
    Scoring,
    Scenarios,
    Drafting,
    Critiquing,
    Revising,
    Checked,
    Rendered,
    Finalized,
    Abandoned,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Scoped => "scoped",
            Self::Exploring => "exploring",
            Self::Framing => "framing",
            Self::Enumerating => "enumerating",
            Self::Gathering => "gathering",
            Self::Scoring => "scoring",
            Self::Scenarios => "scenarios",
            Self::Drafting => "drafting",
            Self::Critiquing => "critiquing",
            Self::Revising => "revising",
            Self::Checked => "checked",
            Self::Rendered => "rendered",
            Self::Finalized => "finalized",
            Self::Abandoned => "abandoned",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        Ok(match value {
            "created" => Self::Created,
            "scoped" => Self::Scoped,
            "exploring" => Self::Exploring,
            "framing" => Self::Framing,
            "enumerating" => Self::Enumerating,
            "gathering" => Self::Gathering,
            "scoring" => Self::Scoring,
            "scenarios" => Self::Scenarios,
            "drafting" => Self::Drafting,
            "critiquing" => Self::Critiquing,
            "revising" => Self::Revising,
            "checked" => Self::Checked,
            "rendered" => Self::Rendered,
            "finalized" => Self::Finalized,
            "abandoned" => Self::Abandoned,
            other => bail!("unknown report run status: {other}"),
        })
    }

    /// Numeric ordering used by [`require_at_least`]. The terminal states
    /// `Finalized` and `Abandoned` are not in this ordering: they reject all
    /// further mutations.
    pub fn ordinal(self) -> Option<u32> {
        Some(match self {
            Self::Created => 0,
            Self::Scoped => 1,
            Self::Exploring => 2,
            Self::Framing => 3,
            Self::Enumerating => 4,
            Self::Gathering => 5,
            Self::Scoring => 6,
            Self::Scenarios => 7,
            Self::Drafting => 8,
            Self::Critiquing => 9,
            Self::Revising => 10,
            Self::Checked => 11,
            Self::Rendered => 12,
            Self::Finalized | Self::Abandoned => return None,
        })
    }
}

/// Read the current status of a run.
pub fn current_status(conn: &Connection, run_id: &str) -> Result<Status> {
    let value: String = conn
        .query_row(
            "SELECT status FROM report_runs WHERE run_id = ?1",
            params![run_id],
            |row| row.get(0),
        )
        .with_context(|| format!("run not found: {run_id}"))?;
    Status::parse(&value)
}

/// Reject the operation if the run has not reached `min` (or is terminal).
pub fn require_at_least(conn: &Connection, run_id: &str, min: Status) -> Result<()> {
    let current = current_status(conn, run_id)?;
    if matches!(current, Status::Finalized | Status::Abandoned) {
        bail!(
            "run {run_id} is in terminal state '{}' and cannot accept further stage mutations",
            current.as_str()
        );
    }
    let cur_ord = current
        .ordinal()
        .with_context(|| format!("run {run_id} is in unexpected state {}", current.as_str()))?;
    let min_ord = min
        .ordinal()
        .expect("static minimum status is never terminal");
    if cur_ord < min_ord {
        bail!(
            "run {run_id} is in state '{}'; this stage requires at least '{}'",
            current.as_str(),
            min.as_str()
        );
    }
    Ok(())
}

/// Advance the run state forward. Going backwards is rejected unless the
/// caller is the critique→revise loop, which uses [`reenter_revise`].
pub fn advance_to(conn: &Connection, run_id: &str, target: Status) -> Result<()> {
    let current = current_status(conn, run_id)?;
    if matches!(current, Status::Finalized | Status::Abandoned) {
        bail!(
            "run {run_id} is terminal ('{}'); cannot advance",
            current.as_str()
        );
    }
    if let (Some(cur), Some(tgt)) = (current.ordinal(), target.ordinal()) {
        if tgt < cur {
            bail!(
                "refusing to move run {run_id} from '{}' back to '{}'",
                current.as_str(),
                target.as_str()
            );
        }
    }
    write_status(conn, run_id, target)?;
    Ok(())
}

/// Re-enter the revise loop after a critique. Permitted even though the
/// status order would otherwise reject going from `Checked` back to
/// `Revising`.
pub fn reenter_revise(conn: &Connection, run_id: &str) -> Result<()> {
    let current = current_status(conn, run_id)?;
    match current {
        Status::Drafting
        | Status::Critiquing
        | Status::Revising
        | Status::Checked
        | Status::Rendered => {
            write_status(conn, run_id, Status::Revising)?;
            Ok(())
        }
        _ => bail!(
            "run {run_id} is in state '{}'; revise can only re-enter from drafting/critiquing/revising/checked/rendered",
            current.as_str()
        ),
    }
}

/// Force-mark a run as terminal. Used by `report finalize` and `report abort`.
pub fn terminate(conn: &Connection, run_id: &str, target: Status) -> Result<()> {
    if !matches!(target, Status::Finalized | Status::Abandoned) {
        bail!("terminate target must be finalized or abandoned");
    }
    write_status(conn, run_id, target)?;
    Ok(())
}

fn write_status(conn: &Connection, run_id: &str, target: Status) -> Result<()> {
    let updated = conn
        .execute(
            "UPDATE report_runs SET status = ?2, last_stage = ?2, updated_at = ?3 WHERE run_id = ?1",
            params![run_id, target.as_str(), super::store::now_iso()],
        )
        .context("failed to update report_runs.status")?;
    if updated == 0 {
        bail!("run {run_id} not found while updating status");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::store;
    use tempfile::tempdir;

    fn seed_run(conn: &Connection, status: Status) {
        let now = store::now_iso();
        conn.execute(
            "INSERT INTO report_runs(run_id, preset, blueprint_version, topic, language,
              status, created_at, updated_at)
             VALUES('run_test','feasibility','1','t','en',?1,?2,?2)",
            params![status.as_str(), now],
        )
        .unwrap();
    }

    #[test]
    fn require_at_least_blocks_under_min() {
        let dir = tempdir().unwrap();
        let conn = store::open(dir.path()).unwrap();
        seed_run(&conn, Status::Created);
        assert!(require_at_least(&conn, "run_test", Status::Scoring).is_err());
    }

    #[test]
    fn advance_forward_works_backward_rejects() {
        let dir = tempdir().unwrap();
        let conn = store::open(dir.path()).unwrap();
        seed_run(&conn, Status::Created);
        advance_to(&conn, "run_test", Status::Scoped).unwrap();
        assert_eq!(current_status(&conn, "run_test").unwrap(), Status::Scoped);
        assert!(advance_to(&conn, "run_test", Status::Created).is_err());
    }

    #[test]
    fn reenter_revise_allowed_from_checked() {
        let dir = tempdir().unwrap();
        let conn = store::open(dir.path()).unwrap();
        seed_run(&conn, Status::Checked);
        reenter_revise(&conn, "run_test").unwrap();
        assert_eq!(current_status(&conn, "run_test").unwrap(), Status::Revising);
    }

    #[test]
    fn terminal_state_rejects_advance() {
        let dir = tempdir().unwrap();
        let conn = store::open(dir.path()).unwrap();
        seed_run(&conn, Status::Finalized);
        assert!(advance_to(&conn, "run_test", Status::Rendered).is_err());
    }
}
