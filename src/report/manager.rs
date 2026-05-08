//! Manager loop for a deep-research run.
//!
//! Drives the eleven-tool inventory in `crate::report::tools` against an
//! [`InferenceCallable`] until the LLM emits a terminal decision, then
//! applies the host loop-end gate (four checks must all be ready or
//! `check_applicable=false`) and persists the final run status.
//!
//! Wave-5 owns the orchestration only; tool dispatch is fully delegated
//! to the per-tool modules and the `SubSkillRunner` shipped alongside.

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use rusqlite::params;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::report::asset_pack::AssetPack;
use crate::report::checks::{record_check_outcome, CheckOutcome};
use crate::report::manager_prompt::{build_manager_run_input, build_manager_system_prompt};
use crate::report::schema::{ensure_schema, new_id, now_iso, open};
use crate::report::sources::ResolverStack;
use crate::report::state::{finalise as state_finalise, load_run_with};
use crate::report::sub_skill::InferenceCallable;
use crate::report::tools::{self, SubSkillRunner, ToolContext, ToolEnvelope, TOOL_NAMES};
use crate::report::workspace::Workspace;

/// Tunables for the manager loop. Defaults match the values described
/// in the deep-research skill design notes.
#[derive(Debug, Clone)]
pub struct ManagerConfig {
    pub max_turns: usize,
    pub max_run_duration: Duration,
    pub allow_revision: bool,
    pub allow_research: bool,
    pub allow_research_retry: bool,
    pub research_timeout: Duration,
    pub skill_timeout: Duration,
}

impl Default for ManagerConfig {
    fn default() -> Self {
        Self {
            max_turns: 90,
            max_run_duration: Duration::from_secs(18 * 60),
            allow_revision: true,
            allow_research: true,
            allow_research_retry: true,
            research_timeout: Duration::from_secs(6 * 60),
            skill_timeout: Duration::from_secs(8 * 60),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagerDecision {
    Finished,
    NeedsUserInput,
    Blocked,
}

impl ManagerDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            ManagerDecision::Finished => "finished",
            ManagerDecision::NeedsUserInput => "needs_user_input",
            ManagerDecision::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ManagerRunOutcome {
    pub run_id: String,
    pub decision: ManagerDecision,
    pub summary: String,
    pub changed_blocks: Vec<String>,
    pub open_questions: Vec<String>,
    pub reason: String,
    pub tool_calls: usize,
    pub turns: usize,
    pub last_completeness: Option<CheckOutcome>,
    pub last_character_budget: Option<CheckOutcome>,
    pub last_release_guard: Option<CheckOutcome>,
    pub last_narrative_flow: Option<CheckOutcome>,
}

/// Run the manager loop end-to-end. The function is synchronous; the
/// inference callable is responsible for any async I/O it does
/// internally.
pub fn run_manager(
    root: &Path,
    run_id: &str,
    config: ManagerConfig,
    sub_skill_runner: &dyn SubSkillRunner,
    manager_inference: &dyn InferenceCallable,
) -> Result<ManagerRunOutcome> {
    let mut workspace = Workspace::load(root, run_id)?;
    let asset_pack = AssetPack::load()?;
    let resolver = ResolverStack::new(root, run_id, None)
        .context("failed to construct ResolverStack for manager run")?;

    let system_prompt = build_manager_system_prompt();
    let initial_user = build_manager_run_input(&workspace)?;

    let deadline = Instant::now() + config.max_run_duration;

    let mut tool_calls_count: usize = 0;
    let mut turns: usize = 0;
    let mut last_completeness: Option<CheckOutcome> = None;
    let mut last_character_budget: Option<CheckOutcome> = None;
    let mut last_release_guard: Option<CheckOutcome> = None;
    let mut last_narrative_flow: Option<CheckOutcome> = None;

    let mut next_user_message = initial_user.clone();
    let mut malformed_retry_used = false;

    let outcome: ManagerLoopOutcome = loop {
        if turns >= config.max_turns {
            break ManagerLoopOutcome::ended(
                ManagerDecision::Blocked,
                "manager exhausted max_turns".to_string(),
                "max_turns_reached".to_string(),
                Vec::new(),
                Vec::new(),
            );
        }
        if Instant::now() > deadline {
            break ManagerLoopOutcome::ended(
                ManagerDecision::Blocked,
                "manager exceeded max_run_duration".to_string(),
                "deadline_exceeded".to_string(),
                Vec::new(),
                Vec::new(),
            );
        }
        turns += 1;

        let user_value = json!({ "user_message": next_user_message });
        let raw = manager_inference
            .run_one_shot(&system_prompt, &user_value, config.skill_timeout)
            .context("manager inference call failed")?;

        let parsed = parse_manager_output(&raw);
        let parsed = match parsed {
            Ok(p) => {
                malformed_retry_used = false;
                p
            }
            Err(_) if !malformed_retry_used => {
                malformed_retry_used = true;
                let names = TOOL_NAMES.join(", ");
                next_user_message = format!(
                    "Please return only the JSON object, no markdown. \
                     Tool-call shape: {{\"tool\":\"<name>\",\"args\":{{...}}}}. \
                     End-decision shape: {{\"decision\":\"finished|needs_user_input|blocked\",\
                     \"summary\":\"...\",\"changed_blocks\":[...],\"open_questions\":[...],\"reason\":\"...\"}}. \
                     Tools: {names}."
                );
                continue;
            }
            Err(e) => {
                break ManagerLoopOutcome::ended(
                    ManagerDecision::Blocked,
                    "manager output malformed".to_string(),
                    format!("malformed_output: {e}"),
                    Vec::new(),
                    Vec::new(),
                );
            }
        };

        match parsed {
            ManagerOutput::Decision {
                decision,
                summary,
                changed_blocks,
                open_questions,
                reason,
            } => {
                break ManagerLoopOutcome::ended(
                    decision,
                    summary,
                    reason,
                    changed_blocks,
                    open_questions,
                );
            }
            ManagerOutput::ToolCalls(calls) => {
                let mut envelopes: Vec<Value> = Vec::with_capacity(calls.len());
                let mut user_input_break: Option<(Vec<String>, String)> = None;
                let mut workspace_dirty = false;
                for call in calls {
                    let tool_name = call.name.as_str();
                    if !TOOL_NAMES.iter().any(|n| *n == tool_name) {
                        envelopes.push(serde_json::to_value(tools::err(
                            "unknown",
                            format!("unknown tool {tool_name:?}"),
                        ))?);
                        continue;
                    }
                    let ctx = ToolContext {
                        root,
                        run_id,
                        workspace: &workspace,
                        asset_pack,
                        resolver: &resolver,
                        sub_skill_runner,
                    };
                    let envelope =
                        dispatch_tool(&ctx, tool_name, &call.args).unwrap_or_else(|err| {
                            tools::err(
                                static_tool_name(tool_name),
                                format!("tool dispatch failed: {err}"),
                            )
                        });
                    tool_calls_count += 1;

                    // Forensic breadcrumb: persist a provenance row for
                    // every tool call so an operator can reconstruct
                    // exactly what the manager attempted, regardless of
                    // whether the tool itself wrote anything. Best-effort
                    // — never let a logging failure abort the loop.
                    if let Ok(conn) = open(root) {
                        let payload = json!({
                            "tool": tool_name,
                            "args": call.args.clone(),
                            "ok": envelope.ok,
                            "user_input_required": envelope.user_input_required,
                            "error": envelope.error.clone(),
                            "data_keys": match &envelope.data {
                                Value::Object(map) => map.keys().take(8).cloned().collect::<Vec<_>>(),
                                _ => Vec::new(),
                            },
                        });
                        let _ = record_provenance_note(&conn, run_id, "manager_tool_call", payload);
                    }

                    // Track the latest check outcome for the host gate.
                    match tool_name {
                        "completeness_check" => {
                            last_completeness = decode_check_outcome(&envelope);
                        }
                        "character_budget_check" => {
                            last_character_budget = decode_check_outcome(&envelope);
                        }
                        "release_guard_check" => {
                            last_release_guard = decode_check_outcome(&envelope);
                        }
                        "narrative_flow_check" => {
                            last_narrative_flow = decode_check_outcome(&envelope);
                        }
                        "apply_block_patch" => {
                            workspace_dirty = true;
                        }
                        _ => {}
                    }

                    if envelope.user_input_required {
                        let questions = extract_questions(&envelope);
                        let summary = envelope
                            .data
                            .get("blocking_reason")
                            .and_then(Value::as_str)
                            .or_else(|| envelope.data.get("reason").and_then(Value::as_str))
                            .unwrap_or("user input required")
                            .to_string();
                        user_input_break = Some((questions, summary));
                        envelopes.push(serde_json::to_value(envelope)?);
                        break;
                    }

                    envelopes.push(serde_json::to_value(envelope)?);
                }

                if workspace_dirty {
                    // Refresh the workspace view so the next turn sees
                    // the just-committed blocks.
                    workspace = Workspace::load(root, run_id)?;
                }

                if let Some((questions, summary)) = user_input_break {
                    break ManagerLoopOutcome::ended(
                        ManagerDecision::NeedsUserInput,
                        summary,
                        "tool returned user_input_required".to_string(),
                        Vec::new(),
                        questions,
                    );
                }

                next_user_message = serde_json::to_string_pretty(&json!({
                    "tool_results": envelopes,
                }))?;
            }
        }
    };

    // Apply the host loop-end gate.
    let gated = apply_loop_end_gate(
        outcome,
        last_completeness.as_ref(),
        last_character_budget.as_ref(),
        last_release_guard.as_ref(),
        last_narrative_flow.as_ref(),
    );

    // Persist final state.
    let conn = open(root)?;
    ensure_schema(&conn)?;
    match gated.decision {
        ManagerDecision::Finished => {
            // Best-effort persist any check outcomes whose payloads were
            // stored only in our in-memory `last_*` slots; in practice
            // the tool layer already wrote them, so this is a no-op.
            let _ = (
                &last_completeness,
                &last_character_budget,
                &last_release_guard,
                &last_narrative_flow,
            );
            state_finalise(&conn, run_id)?;
        }
        ManagerDecision::NeedsUserInput => {
            // Keep current run status. Persist a provenance note so the
            // operator surface can show why the run paused.
            record_provenance_note(
                &conn,
                run_id,
                "manager_pause",
                json!({
                    "decision": gated.decision.as_str(),
                    "summary": gated.summary,
                    "open_questions": gated.open_questions,
                    "reason": gated.reason,
                }),
            )?;
        }
        ManagerDecision::Blocked => {
            // Keep current run status; record the reason as a provenance
            // note so the operator surface can pick it up.
            record_provenance_note(
                &conn,
                run_id,
                "manager_block",
                json!({
                    "decision": gated.decision.as_str(),
                    "summary": gated.summary,
                    "reason": gated.reason,
                }),
            )?;

            // Also rewrite the latest check outcomes (if any) so the
            // gate failure is auditable; safe to skip if the slots are
            // empty.
            if let Some(check) = &last_completeness {
                record_check_outcome(&conn, run_id, check)?;
            }
            if let Some(check) = &last_character_budget {
                record_check_outcome(&conn, run_id, check)?;
            }
            if let Some(check) = &last_release_guard {
                record_check_outcome(&conn, run_id, check)?;
            }
            if let Some(check) = &last_narrative_flow {
                record_check_outcome(&conn, run_id, check)?;
            }
        }
    }
    let _ = load_run_with(&conn, run_id)?;

    Ok(ManagerRunOutcome {
        run_id: run_id.to_string(),
        decision: gated.decision,
        summary: gated.summary,
        changed_blocks: gated.changed_blocks,
        open_questions: gated.open_questions,
        reason: gated.reason,
        tool_calls: tool_calls_count,
        turns,
        last_completeness,
        last_character_budget,
        last_release_guard,
        last_narrative_flow,
    })
}

// --------------------------------------------------------------------
// Tool dispatch
// --------------------------------------------------------------------

fn dispatch_tool(ctx: &ToolContext, name: &str, args: &Value) -> Result<ToolEnvelope> {
    let args_for = |default: Value| -> Value {
        if args.is_null() {
            default
        } else {
            args.clone()
        }
    };
    match name {
        "workspace_snapshot" => {
            let parsed: tools::workspace_snapshot::Args =
                serde_json::from_value(args_for(json!({})))
                    .context("decoding workspace_snapshot args")?;
            tools::workspace_snapshot_execute(ctx, &parsed)
        }
        "asset_lookup" => {
            let parsed: tools::asset_lookup::Args = serde_json::from_value(args_for(json!({})))
                .context("decoding asset_lookup args")?;
            tools::asset_lookup_execute(ctx, &parsed)
        }
        "ask_user" => {
            let parsed: tools::ask_user::Args =
                serde_json::from_value(args_for(json!({}))).context("decoding ask_user args")?;
            tools::ask_user_execute(ctx, &parsed)
        }
        "public_research" => {
            let parsed: tools::public_research::Args = serde_json::from_value(args_for(json!({})))
                .context("decoding public_research args")?;
            tools::public_research_execute(ctx, &parsed)
        }
        "write_with_skill" => {
            let parsed: tools::write_with_skill::Args = serde_json::from_value(args_for(json!({})))
                .context("decoding write_with_skill args")?;
            tools::write_with_skill_execute(ctx, &parsed)
        }
        "revise_with_skill" => {
            let parsed: tools::revise_with_skill::Args =
                serde_json::from_value(args_for(json!({})))
                    .context("decoding revise_with_skill args")?;
            tools::revise_with_skill_execute(ctx, &parsed)
        }
        "apply_block_patch" => {
            let parsed: tools::apply_block_patch::Args =
                serde_json::from_value(args_for(json!({})))
                    .context("decoding apply_block_patch args")?;
            tools::apply_block_patch_execute(ctx, &parsed)
        }
        "completeness_check" => {
            let parsed: tools::completeness_check::Args =
                serde_json::from_value(args_for(json!({})))
                    .context("decoding completeness_check args")?;
            tools::completeness_check_execute(ctx, &parsed)
        }
        "character_budget_check" => {
            let parsed: tools::character_budget_check::Args =
                serde_json::from_value(args_for(json!({})))
                    .context("decoding character_budget_check args")?;
            tools::character_budget_check_execute(ctx, &parsed)
        }
        "release_guard_check" => {
            let parsed: tools::release_guard_check::Args =
                serde_json::from_value(args_for(json!({})))
                    .context("decoding release_guard_check args")?;
            tools::release_guard_check_execute(ctx, &parsed)
        }
        "narrative_flow_check" => {
            let parsed: tools::narrative_flow_check::Args =
                serde_json::from_value(args_for(json!({})))
                    .context("decoding narrative_flow_check args")?;
            tools::narrative_flow_check_execute(ctx, &parsed)
        }
        other => bail!("unknown tool {other:?}"),
    }
}

fn static_tool_name(name: &str) -> &'static str {
    for known in TOOL_NAMES {
        if *known == name {
            return *known;
        }
    }
    "unknown"
}

// --------------------------------------------------------------------
// Manager output parsing
// --------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ToolCall {
    name: String,
    args: Value,
}

#[derive(Debug, Clone)]
enum ManagerOutput {
    ToolCalls(Vec<ToolCall>),
    Decision {
        decision: ManagerDecision,
        summary: String,
        changed_blocks: Vec<String>,
        open_questions: Vec<String>,
        reason: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
struct DecisionWire {
    decision: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    changed_blocks: Vec<String>,
    #[serde(default)]
    open_questions: Vec<String>,
    #[serde(default)]
    reason: String,
}

fn parse_manager_output(raw: &str) -> Result<ManagerOutput> {
    let cleaned = strip_code_fences(raw.trim());
    let value: Value = serde_json::from_str(cleaned)
        .with_context(|| format!("manager output is not valid JSON: {cleaned:?}"))?;

    if let Some(decision) = value.get("decision").and_then(Value::as_str) {
        let wire: DecisionWire = serde_json::from_value(value.clone())
            .context("decoding decision-shape manager output")?;
        let parsed = match decision {
            "finished" => ManagerDecision::Finished,
            "needs_user_input" => ManagerDecision::NeedsUserInput,
            "blocked" => ManagerDecision::Blocked,
            other => bail!("unknown manager decision {other:?}"),
        };
        return Ok(ManagerOutput::Decision {
            decision: parsed,
            summary: wire.summary,
            changed_blocks: wire.changed_blocks,
            open_questions: wire.open_questions,
            reason: wire.reason,
        });
    }

    if let Some(calls) = value.get("tool_calls").and_then(Value::as_array) {
        let mut out: Vec<ToolCall> = Vec::with_capacity(calls.len());
        for entry in calls {
            let name = entry
                .get("name")
                .or_else(|| entry.get("tool"))
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("tool_calls[] entry missing name"))?
                .to_string();
            let args = entry.get("args").cloned().unwrap_or(Value::Null);
            out.push(ToolCall { name, args });
        }
        return Ok(ManagerOutput::ToolCalls(out));
    }

    if let Some(name) = value.get("tool").and_then(Value::as_str) {
        let args = value.get("args").cloned().unwrap_or(Value::Null);
        return Ok(ManagerOutput::ToolCalls(vec![ToolCall {
            name: name.to_string(),
            args,
        }]));
    }

    bail!("manager output had neither `tool`/`tool_calls` nor `decision` keys")
}

fn strip_code_fences(input: &str) -> &str {
    let trimmed = input.trim();
    if let Some(rest) = trimmed.strip_prefix("```json") {
        return rest
            .trim_start_matches('\n')
            .trim_end()
            .trim_end_matches("```")
            .trim();
    }
    if let Some(rest) = trimmed.strip_prefix("```") {
        return rest
            .trim_start_matches('\n')
            .trim_end()
            .trim_end_matches("```")
            .trim();
    }
    trimmed
}

// --------------------------------------------------------------------
// Loop-end gate
// --------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ManagerLoopOutcome {
    decision: ManagerDecision,
    summary: String,
    reason: String,
    changed_blocks: Vec<String>,
    open_questions: Vec<String>,
}

impl ManagerLoopOutcome {
    fn ended(
        decision: ManagerDecision,
        summary: String,
        reason: String,
        changed_blocks: Vec<String>,
        open_questions: Vec<String>,
    ) -> Self {
        Self {
            decision,
            summary,
            reason,
            changed_blocks,
            open_questions,
        }
    }
}

fn apply_loop_end_gate(
    mut outcome: ManagerLoopOutcome,
    completeness: Option<&CheckOutcome>,
    character_budget: Option<&CheckOutcome>,
    release_guard: Option<&CheckOutcome>,
    narrative_flow: Option<&CheckOutcome>,
) -> ManagerLoopOutcome {
    if outcome.decision != ManagerDecision::Finished {
        return outcome;
    }

    let downgrade = |outcome: &mut ManagerLoopOutcome, message: String| {
        outcome.decision = ManagerDecision::Blocked;
        outcome.reason = format!("host loop-end gate: {message}");
        if outcome.summary.is_empty() {
            outcome.summary = outcome.reason.clone();
        }
    };

    let check_or_downgrade =
        |outcome: &mut ManagerLoopOutcome, maybe: Option<&CheckOutcome>, name: &str| match maybe {
            None => {
                downgrade(outcome, format!("{name} not run"));
                false
            }
            Some(check) => {
                if !(check.ready_to_finish || !check.check_applicable) {
                    downgrade(outcome, format!("{name} failed"));
                    return false;
                }
                true
            }
        };

    if !check_or_downgrade(&mut outcome, completeness, "completeness_check") {
        return outcome;
    }
    if !check_or_downgrade(&mut outcome, character_budget, "character_budget_check") {
        return outcome;
    }
    if !check_or_downgrade(&mut outcome, release_guard, "release_guard_check") {
        return outcome;
    }
    if !check_or_downgrade(&mut outcome, narrative_flow, "narrative_flow_check") {
        return outcome;
    }
    outcome
}

// --------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------

fn decode_check_outcome(envelope: &ToolEnvelope) -> Option<CheckOutcome> {
    if !envelope.ok {
        return None;
    }
    serde_json::from_value(envelope.data.clone()).ok()
}

fn extract_questions(envelope: &ToolEnvelope) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(arr) = envelope.data.get("questions").and_then(Value::as_array) {
        for q in arr {
            if let Some(s) = q.as_str() {
                out.push(s.to_string());
            }
        }
    }
    if out.is_empty() {
        if let Some(arr) = envelope
            .data
            .get("blocking_questions")
            .and_then(Value::as_array)
        {
            for q in arr {
                if let Some(s) = q.as_str() {
                    out.push(s.to_string());
                }
            }
        }
    }
    out
}

fn record_provenance_note(
    conn: &rusqlite::Connection,
    run_id: &str,
    kind: &str,
    payload: Value,
) -> Result<()> {
    let prov_id = new_id("prov");
    let payload_text =
        serde_json::to_string(&payload).context("encode manager provenance note payload")?;
    conn.execute(
        "INSERT INTO report_provenance (
             prov_id, run_id, kind, occurred_at, instance_id, skill_run_id,
             research_id, payload_json
         ) VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL, ?5)",
        params![prov_id, run_id, kind, now_iso(), payload_text],
    )
    .context("failed to insert manager provenance note")?;
    Ok(())
}
