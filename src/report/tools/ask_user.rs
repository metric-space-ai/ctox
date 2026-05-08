//! `ask_user` tool. Persists a question card into `report_questions`
//! and signals to the manager that it must end the run with
//! `decision: needs_user_input`. Manager treats every successful
//! `ask_user` as a soft-error.

use anyhow::{Context, Result};
use rusqlite::params;
use serde::Deserialize;
use serde_json::json;

use crate::report::schema::{ensure_schema, new_id, now_iso, open};
use crate::report::tools::{err, user_input, ToolContext, ToolEnvelope};

const TOOL: &str = "ask_user";
const MAX_QUESTIONS: usize = 5;

#[derive(Debug, Clone, Deserialize)]
pub struct Args {
    pub section: String,
    pub reason: String,
    pub questions: Vec<String>,
}

pub fn execute(ctx: &ToolContext, args: &Args) -> Result<ToolEnvelope> {
    if args.questions.is_empty() {
        return Ok(err(TOOL, "ask_user requires at least one question".into()));
    }
    if args.questions.len() > MAX_QUESTIONS {
        return Ok(err(
            TOOL,
            format!(
                "ask_user accepts at most {MAX_QUESTIONS} questions; got {}",
                args.questions.len()
            ),
        ));
    }
    for (idx, q) in args.questions.iter().enumerate() {
        if q.trim().is_empty() {
            return Ok(err(TOOL, format!("questions[{idx}] is empty")));
        }
    }

    let conn = open(ctx.root)?;
    ensure_schema(&conn)?;
    let question_id = new_id("q");
    let questions_json =
        serde_json::to_string(&args.questions).context("failed to encode questions list")?;
    let raised_at = now_iso();
    conn.execute(
        "INSERT INTO report_questions (
             question_id, run_id, section, reason, questions_json,
             allow_fallback, raised_at, answered_at, answer_text
         ) VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, NULL, NULL)",
        params![
            question_id,
            ctx.run_id,
            args.section,
            args.reason,
            questions_json,
            raised_at,
        ],
    )
    .context("failed to persist ask_user question card")?;

    let payload = json!({
        "question_id": question_id,
        "section": args.section,
        "reason": args.reason,
        "questions": args.questions,
        "raised_at": raised_at,
    });
    Ok(user_input(TOOL, payload))
}
