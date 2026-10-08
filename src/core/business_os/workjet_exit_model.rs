// Origin: CTOX
// License: AGPL-3.0-only

//! Owner-gated immutable assessments. Refresh uses one existing durable
//! supervisor turn per project/calendar month; queue admission is never E5.
use super::domain_effect::{AppliedDomainEffect, DomainEffectAdmission, DomainRecordRef};
use super::store::{
    self, open_store, outbound_load_record, upsert_business_record, BusinessCommand, CommandOrigin,
};
use super::workjet_exit_model_engine::{self as engine, Inputs, Resources};
use anyhow::{ensure, Context};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

#[cfg(test)]
#[path = "workjet_exit_model_tests.rs"]
mod tests;

const TABLE: &str = "workjet_exit_model_runs";
const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS workjet_exit_model_runs (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT, run_id TEXT UNIQUE NOT NULL,
 project_id TEXT NOT NULL, owner_user_id TEXT NOT NULL, assessment_json TEXT NOT NULL,
 inputs_json TEXT, research_command_id TEXT UNIQUE
);
CREATE INDEX IF NOT EXISTS workjet_exit_model_project_runs ON workjet_exit_model_runs(project_id,sequence DESC);";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    project_id: String,
    #[serde(default)]
    as_of: Option<String>,
    #[serde(default)]
    resources: Option<Resources>,
    #[serde(default)]
    inputs: Option<Inputs>,
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
}

fn exists(conn: &Connection) -> anyhow::Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [TABLE],
        |r| r.get(0),
    )?)
}
fn reader(root: &Path) -> anyhow::Result<Connection> {
    let conn = Connection::open_with_flags(
        store::business_os_store_path(root),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    Ok(conn)
}
fn empty(project_id: &str) -> Value {
    json!({"contract":engine::CONTRACT,"project_id":project_id,"run_id":null,"as_of":null,"exit_date":null,"refresh_due":null,"status":"not_started","missing_inputs":["resource_plan","research_inputs"],"findings":[],"sources":[],"plan_summary":null,"result":null,"scenarios":[],"history":[]})
}
fn summary(run: &Value) -> Value {
    json!({"run_id":run["run_id"],"as_of":run["as_of"],"exit_date":run["exit_date"],"status":run["status"],"result":run["result"],"missing_inputs":run["missing_inputs"]})
}
pub(super) fn read_state(conn: &Connection, project: &str, owner: &str) -> anyhow::Result<Value> {
    if !exists(conn)? {
        return Ok(empty(project));
    }
    let mut query=conn.prepare("SELECT owner_user_id,assessment_json FROM workjet_exit_model_runs WHERE project_id=?1 ORDER BY sequence DESC LIMIT 24")?;
    let rows = query.query_map([project], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    let mut latest = None;
    let mut history = Vec::new();
    for row in rows {
        let (stored_owner, raw) = row?;
        ensure!(
            stored_owner == owner,
            "exit-model owner conflicts with project ownership"
        );
        ensure!(raw.len() <= 512 * 1024, "assessment exceeds read budget");
        let value: Value = serde_json::from_str(&raw)?;
        history.push(summary(&value));
        if latest.is_none() {
            latest = Some(value);
        }
    }
    let mut state = latest.unwrap_or_else(|| empty(project));
    state["history"] = json!(history);
    Ok(state)
}
fn operation_id(command: &BusinessCommand) -> anyhow::Result<&str> {
    let id = command
        .id
        .as_deref()
        .context("exit model requires command id")?;
    ensure!(
        !id.is_empty() && id.len() <= 256,
        "invalid exit-model command id"
    );
    Ok(id)
}
fn stable_id(prefix: &str, parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!("{prefix}_{:x}", digest.finalize())
}
fn assessment(
    project: &str,
    run: &str,
    as_of: &str,
    inputs: Option<&Inputs>,
    resources: Option<&Resources>,
    research: bool,
) -> anyhow::Result<Value> {
    let mut value = empty(project);
    value["run_id"] = json!(run);
    value["as_of"] = json!(as_of);
    value["exit_date"] = json!(engine::add_months(as_of, 60)?);
    value["refresh_due"] = json!(engine::add_months(as_of, 1)?);
    value["engine_version"] = json!(engine::ENGINE);
    value["currency"] = json!("EUR");
    value["basis"] = json!("100_percent_equity_before_fees_and_personal_tax");
    if let Some(inputs) = inputs {
        let missing = engine::validate(inputs, as_of)?;
        value["sources"] = serde_json::to_value(&inputs.sources)?;
        value["plan_summary"] = if inputs.plan.mode == "committed_plan" {
            engine::plan_summary(&inputs.plan)
        } else {
            Value::Null
        };
        value["missing_inputs"] = json!(missing);
        value["compiled_parameters_hash"] =
            json!(format!("{:x}", Sha256::digest(serde_json::to_vec(inputs)?)));
        if missing.is_empty() {
            match engine::calculate(inputs) {
                Ok((result, outcomes)) => {
                    value["status"] = json!("provisional");
                    value["result"] = result;
                    value["scenarios"]=json!(outcomes.iter().map(|o|json!({"state":o.state,"probability":o.probability,"sale_probability":o.sale_probability,"equity_price_eur":o.equity_price_eur,"contribution_eur":o.probability*o.sale_probability*o.equity_price_eur})).collect::<Vec<_>>());
                    value["diagnostics"] = serde_json::to_value(outcomes)?;
                    value["findings"] = json!([{"code":"evidence_not_independently_verified","message":"Source references and researcher assumptions are retained; their economic support has not been independently reviewed. This finite model is provisional."}]);
                }
                Err(error) => {
                    value["status"] = json!("failed");
                    value["findings"] =
                        json!([{"code":"calculation_failed","message":error.to_string()}]);
                }
            }
        } else {
            value["status"] = json!("blocked");
            value["findings"] = json!([{"code":"required_inputs_missing","message":"The resource plan, rights, adapter or current source evidence is incomplete."}]);
        }
    } else {
        value["status"] = json!(if research { "researching" } else { "blocked" });
        value["missing_inputs"] = json!(if resources.is_some() {
            vec!["confirmed_60_month_resource_plan", "researched_inputs"]
        } else {
            vec![
                "resource_proposal",
                "confirmed_60_month_resource_plan",
                "researched_inputs",
            ]
        });
        value["findings"] = json!([{"code":if research{"research_admitted"}else{"resource_or_supervisor_missing"},"message":if research{"One bounded durable supervisor turn is gathering source-backed inputs. No valuation is available yet."}else{"Provide explicit resources and bind this project's supervisor before research can be admitted."}}]);
        if let Some(resources) = resources {
            value["resource_proposal"] = serde_json::to_value(resources)?;
        }
    }
    Ok(value)
}
fn persist(
    conn: &Connection,
    actor: &str,
    request: &Request,
    run_id: &str,
    research_command: Option<&str>,
) -> anyhow::Result<AppliedDomainEffect> {
    let owner = super::workjet_project_kpis::require_project(conn, actor, &request.project_id)?;
    conn.execute_batch(SCHEMA)?;
    if let Some(existing) = conn
        .query_row(
            "SELECT assessment_json FROM workjet_exit_model_runs WHERE run_id=?1",
            [run_id],
            |r| r.get::<_, String>(0),
        )
        .optional()?
    {
        let existing: Value = serde_json::from_str(&existing)?;
        ensure!(
            existing["project_id"] == request.project_id,
            "run conflicts with project"
        );
        return Ok(AppliedDomainEffect {
            result: json!({"ok":true,"assessment":read_state(conn,&request.project_id,&owner)?}),
            projections: vec![DomainRecordRef {
                collection: "workjet_projects".into(),
                id: request.project_id.clone(),
            }],
        });
    }
    let as_of = request
        .as_of
        .as_deref()
        .context("as_of must be resolved before write")?;
    let mut value = assessment(
        &request.project_id,
        run_id,
        as_of,
        request.inputs.as_ref(),
        request.resources.as_ref(),
        research_command.is_some(),
    )?;
    if request.resources.is_none() {
        let previous = read_state(conn, &request.project_id, &owner)?;
        if let Some(proposal) = previous.get("resource_proposal") {
            value["resource_proposal"] = proposal.clone();
        }
    }
    conn.execute("INSERT INTO workjet_exit_model_runs(run_id,project_id,owner_user_id,assessment_json,inputs_json,research_command_id) VALUES(?1,?2,?3,?4,?5,?6)",params![run_id,request.project_id,owner,serde_json::to_string(&value)?,request.inputs.as_ref().map(serde_json::to_string).transpose()?,research_command])?;
    let state = read_state(conn, &request.project_id, &owner)?;
    let mut project = outbound_load_record(conn, "workjet_projects", &request.project_id)?
        .context("project disappeared")?;
    let now = store::now_ms() as i64;
    project["exit_model"] = state.clone();
    project["updated_at_ms"] = json!(now);
    upsert_business_record(conn, "workjet_projects", &request.project_id, now, project)?;
    Ok(AppliedDomainEffect {
        result: json!({"ok":true,"assessment":state}),
        projections: vec![DomainRecordRef {
            collection: "workjet_projects".into(),
            id: request.project_id.clone(),
        }],
    })
}

fn admit_research(
    root: &Path,
    actor: &str,
    request: &Request,
) -> anyhow::Result<Option<(String, String)>> {
    let Some(resources) = request.resources.as_ref() else {
        return Ok(None);
    };
    let conn = reader(root)?;
    let owner = super::workjet_project_kpis::require_project(&conn, actor, &request.project_id)?;
    let bound:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workjet_supervisor_bindings')",[],|r|r.get(0))?;
    if !bound {
        return Ok(None);
    };
    let thread:Option<String>=conn.query_row("SELECT thread_id FROM workjet_supervisor_bindings WHERE project_id=?1 AND owner_user_id=?2",params![request.project_id,owner],|r|r.get(0)).optional()?;
    let Some(thread) = thread else {
        return Ok(None);
    };
    let as_of = request.as_of.as_deref().context("research date missing")?;
    // A different proposal within the same month conflicts with the immutable
    // native producer intent. It cannot start a second parallel research job.
    let operation = stable_id(
        "workjet_exit_research",
        &[&owner, &request.project_id, &as_of[..7]],
    );
    if exists(&conn)? {
        let active: Option<(String,String,String)> = conn.query_row(
            "SELECT run_id,research_command_id,assessment_json FROM workjet_exit_model_runs WHERE project_id=?1 AND research_command_id IS NOT NULL ORDER BY sequence DESC LIMIT 1",
            [&request.project_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if let Some((run, command_id, raw)) = active {
            if crate::mission::channels::business_command_projection(root, &command_id)?
                ["execution_phase"]
                != "terminal"
            {
                let previous: Value = serde_json::from_str(&raw)?;
                ensure!(
                    previous["resource_proposal"] == serde_json::to_value(resources)?,
                    "a research turn with a different proposal is active"
                );
                return Ok(Some((run, command_id)));
            }
        }
        let prior:Option<(String,String)>=conn.query_row("SELECT run_id,research_command_id FROM workjet_exit_model_runs WHERE project_id=?1 AND json_extract(assessment_json,'$.as_of') LIKE ?2 AND research_command_id IS NOT NULL ORDER BY sequence DESC LIMIT 1",params![request.project_id,format!("{}%",&as_of[..7])],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some(prior) = prior {
            return Ok(Some(prior));
        }
    }
    drop(conn);
    let session = store::active_domain_recovery_session(root, &owner)?;
    ensure!(
        store::module_policy_decision(
            root,
            &session,
            super::policy::BusinessOsPermission::CtoxTaskCreate,
            "ctox"
        )?
        .allowed,
        "exit research owner may not create CTOX tasks"
    );
    let mut goal=format!("Research project {} exit proceeds as of {} over 60 calendar months. Resource PROPOSAL only: {}. No hidden budget allocations; obtain a confirmed 60-month committed_plan, transferable rights and source-backed conditional scenarios. Do not make up numbers or sources, contact people, buy data, deploy, or schedule recurring work. Use authorized project evidence and public primary sources. Return ONLY one JSON object {{\"exit_model_inputs\":<Inputs object>}} conforming to docs/workjet-exit-model.md, or {{\"exit_model_blocked\":{{\"missing_inputs\":[<explicit gaps>]}}}}. Use adapter saas_reference_v1 only for qualifying recurring software; terminal_equity_grid_v1 requires independently sourced terminal equity prices and already includes failed/no-sale states. Unsupported game/IP economics stay blocked. Sources retain observed_at, valid_until and kind; assumptions are never observations. The native engine, not your prose, computes E5.",request.project_id,as_of,serde_json::to_string(resources)?);
    goal.push_str(" Research relevant project factors: addressable/servable market, demand and willingness to pay, competitors and defensible moat, acquisition/distribution access and conversion, retention, brand white space and trust, technical feasibility and milestones, team/development/sales/onboarding/service capacity, costs and committed financing, transferable IP/data/licence rights, founder replacement, buyer classes, comparable control-sale prices and sale probability including failed/unsold peers. Translate evidence into explicit operating assumptions, not a score-to-Euro multiplier. Distinguish observations, derivations, priors and unknowns. Keep alternative buyer routes exclusive and avoid double-counting cash, brand, technology or synergy.");
    goal.push_str(" Plan fields: version,mode=committed_plan,comparison_mode,confirmed,hours_per_week,assumptions:string[],source_ids:string[],opening_customers,opening_cash,paid_marketing,brand_budget,cash_opex,equity_funding,capex,sales_capacity,onboarding_capacity,service_capacity,reserve_months,tax_proxy. All eight monthly arrays require 60 finite nonnegative values. Sources: id,reference,observed_at,valid_until,kind=observed|derived|assumed. Inputs fields: adapter,sale_perimeter=100_percent_equity_before_fees_and_personal_tax,rights_confirmed,perimeter_source_ids,sources,plan,scenarios,outcomes,build_probability,technical_failure_sale_probability,technical_failure_equity_price,probability_source_ids. SaaS scenarios: name,source_ids,weight_given_build,launch_month,sales_lag,sam0,sam_annual_growth,cpl,conversion,organic_leads0,organic_annual_growth,logo_churn,arpa0,arpa_annual_growth,gross_margin,brand0,brand_ceiling,brand_speed,brand_reference_budget,brand_organic_lift,brand_price_lift,multiple_basis=ARR|LTM_REVENUE|EBITDA,multiple,sale_probability,debt_like_exit,wc_adjustment,failure_sale_probability,failure_equity_price. Grid outcomes: state,probability,sale_probability,equity_price_eur,source_ids; no additional build risk. Never claim owner confirmation from your own proposal.");
    let accepted = store::accept_rxdb_business_command_with_origin(
        root,
        json!({"id":operation,"module":"ctox","command_type":"ctox.workjet.project.supervisor.turn.submit","record_id":request.project_id,"payload":{"project_id":request.project_id,"thread_id":thread,"goal":goal},"client_context":{"actor":super::threads::actor_payload(&session),"source":"native-workjet-exit-research"}}),
        CommandOrigin::TrustedLocal,
    )?;
    ensure!(
        accepted["status"] == "completed"
            && accepted["result"]["binding"]["project_id"] == request.project_id,
        "research supervisor admission failed: {}",
        accepted["error_message"]
    );
    let research = accepted["result"]["turn"]["command_id"]
        .as_str()
        .context("research has no durable command receipt")?
        .to_owned();
    Ok(Some((stable_id("exit_run", &[&operation]), research)))
}

pub(super) fn handle_command(
    root: &Path,
    command: &BusinessCommand,
    actor: &str,
    admission: Option<&DomainEffectAdmission>,
) -> anyhow::Result<Value> {
    ensure!(
        serde_json::to_vec(&command.payload)?.len() <= 512 * 1024,
        "exit-model input exceeds bounded payload budget"
    );
    let mut request: Request = serde_json::from_value(command.payload.clone())?;
    ensure!(
        !request.project_id.trim().is_empty() && request.project_id.len() <= 128,
        "invalid project_id"
    );
    ensure!(
        command
            .record_id
            .as_deref()
            .is_none_or(|id| id == request.project_id),
        "record_id must match project_id"
    );
    let kind = command.command_type.as_str();
    if kind == "ctox.workjet.exit_model.read" {
        ensure!(
            request.inputs.is_none() && request.resources.is_none() && request.as_of.is_none(),
            "read accepts project_id only"
        );
        let mut conn = reader(root)?;
        let tx = conn.transaction()?;
        let owner = super::workjet_project_kpis::require_project(&tx, actor, &request.project_id)?;
        return Ok(json!({"ok":true,"assessment":read_state(&tx,&request.project_id,&owner)?}));
    }
    ensure!(
        matches!(
            kind,
            "ctox.workjet.exit_model.refresh" | "ctox.workjet.exit_model.submit"
        ),
        "unsupported exit-model command"
    );
    request.as_of = Some(request.as_of.unwrap_or_else(|| {
        chrono::DateTime::from_timestamp_millis(store::now_ms() as i64)
            .expect("current timestamp")
            .date_naive()
            .to_string()
    }));
    engine::date(request.as_of.as_deref().unwrap())?;
    if let Some(resources) = &request.resources {
        resources.validate()?;
    }
    let admitted = admission.context("exit model mutation requires domain admission")?;
    let mut conn = open_store(root)?;
    let owner = super::workjet_project_kpis::require_project(&conn, actor, &request.project_id)?;
    if kind.ends_with(".refresh") && request.resources.is_none() {
        request.resources = read_state(&conn, &request.project_id, &owner)?
            .get("resource_proposal")
            .cloned()
            .map(serde_json::from_value)
            .transpose()?;
    }
    let (run, research) = if kind.ends_with(".submit") {
        ensure!(
            request.resources.is_none(),
            "submit resources belong in the typed 60-month plan"
        );
        let inputs = request
            .inputs
            .as_ref()
            .context("submit requires researched inputs")?;
        engine::validate(inputs, request.as_of.as_deref().unwrap())?;
        (
            stable_id("exit_run", &[&owner, operation_id(command)?]),
            None,
        )
    } else {
        ensure!(
            request.inputs.is_none(),
            "refresh cannot accept numeric researched inputs"
        );
        match admit_research(root, actor, &request)? {
            Some((run, research)) => (run, Some(research)),
            None => (
                stable_id("exit_run", &[&owner, operation_id(command)?]),
                None,
            ),
        }
    };
    let applied = admitted.apply(&mut conn, |tx| {
        persist(tx, actor, &request, &run, research.as_deref())
    })?;
    if let Some(research) = research.as_deref() {
        reconcile_research(root, research)?;
        return Ok(json!({"ok":true,"assessment":read_state(&conn,&request.project_id,&owner)?}));
    }
    Ok(applied.result)
}

/// Reconcile a reviewed terminal command that won the admission race. Both
/// the terminal hook and this path run after their respective durable commits:
/// whichever commit is last sees the other, and replay uses the same run id.
pub(super) fn reconcile_research(root: &Path, research: &str) -> anyhow::Result<()> {
    let canonical = crate::mission::channels::business_command_projection(root, research)?;
    if canonical["execution_phase"] != "terminal" {
        return Ok(());
    }
    let reply = canonical
        .pointer("/result/outbound_text")
        .and_then(Value::as_str)
        .unwrap_or("{}");
    complete_research(root, research, reply)
}

/// Called only after the existing native terminal review gate has admitted a
/// queue reply. It never trusts a chat message as a valuation or bypasses the
/// current owner/policy check when submitting compiled research inputs.
pub(super) fn complete_research(
    root: &Path,
    research_command: &str,
    reply: &str,
) -> anyhow::Result<()> {
    let conn = reader(root)?;
    if !exists(&conn)? {
        return Ok(());
    }
    let record:Option<(String,String,String)>=conn.query_row("SELECT run_id,owner_user_id,assessment_json FROM workjet_exit_model_runs WHERE research_command_id=?1",[research_command],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let Some((run, owner, raw)) = record else {
        return Ok(());
    };
    let original: Value = serde_json::from_str(&raw)?;
    drop(conn);
    let parsed = serde_json::from_str::<Value>(reply).ok();
    let inputs = parsed.as_ref().and_then(|v| v.get("exit_model_inputs"));
    let valid_inputs = inputs
        .and_then(|raw| serde_json::from_value::<Inputs>(raw.clone()).ok())
        .filter(|inputs| engine::validate(inputs, original["as_of"].as_str().unwrap_or("")).is_ok())
        .filter(|inputs| {
            let Some(proposal) = original
                .get("resource_proposal")
                .cloned()
                .and_then(|v| serde_json::from_value::<Resources>(v).ok())
            else {
                return false;
            };
            // The owner controls resource bounds. A researcher may explicitly
            // propose allocations, but cannot enlarge the approved proposal.
            inputs.plan.hours_per_week <= proposal.hours_per_week
                && inputs.plan.comparison_mode == proposal.comparison_mode
                && (0..60).all(|i| {
                    inputs.plan.paid_marketing[i]
                        + inputs.plan.brand_budget[i]
                        + inputs.plan.cash_opex[i]
                        + inputs.plan.capex[i]
                        <= proposal.monthly_budget_eur + 1e-8
                })
        });
    if let Some(inputs) = valid_inputs {
        let session = store::active_domain_recovery_session(root, &owner)?;
        let operation = stable_id("exit_research_result", &[&run]);
        let accepted = store::accept_rxdb_business_command_with_origin(
            root,
            json!({"id":operation,"module":"ctox","command_type":"ctox.workjet.exit_model.submit","record_id":original["project_id"],"payload":{"project_id":original["project_id"],"as_of":original["as_of"],"inputs":inputs},"client_context":{"actor":super::threads::actor_payload(&session),"source":"native-exit-research-result"}}),
            CommandOrigin::TrustedLocal,
        )?;
        if accepted["status"] == "completed" {
            return Ok(());
        }
    }
    // Keep the immutable research admission run. Append a failed/blocked
    // snapshot with no result, preserving the prior numeric history.
    let mut conn = open_store(root)?;
    let tx = conn.transaction()?;
    let project = original["project_id"]
        .as_str()
        .context("research project missing")?;
    super::workjet_project_kpis::require_project(&tx, &owner, project)?;
    let next = stable_id("exit_research_blocked", &[&run]);
    if tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM workjet_exit_model_runs WHERE run_id=?1)",
        [&next],
        |r| r.get::<_, bool>(0),
    )? {
        return Ok(());
    }
    let mut value = original.clone();
    value["run_id"] = json!(next);
    value["status"] = json!("blocked");
    let gaps = parsed
        .as_ref()
        .and_then(|v| v.pointer("/exit_model_blocked/missing_inputs"))
        .and_then(Value::as_array)
        .filter(|v| {
            !v.is_empty()
                && v.len() <= 100
                && v.iter()
                    .all(|s| s.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 512))
        });
    value["missing_inputs"] = gaps
        .map(|v| json!(v))
        .unwrap_or_else(|| json!(["valid_source_backed_research_inputs"]));
    value["findings"] = json!([{"code":"research_did_not_supply_valid_inputs","message":"The bounded research turn did not supply admissible, source-backed inputs. No numeric valuation was created."}]);
    tx.execute("INSERT INTO workjet_exit_model_runs(run_id,project_id,owner_user_id,assessment_json) VALUES(?1,?2,?3,?4)",params![next,project,owner,serde_json::to_string(&value)?])?;
    let state = read_state(&tx, project, &owner)?;
    let mut record = outbound_load_record(&tx, "workjet_projects", project)?
        .context("research project disappeared")?;
    let now = store::now_ms() as i64;
    record["exit_model"] = state;
    record["updated_at_ms"] = json!(now);
    upsert_business_record(&tx, "workjet_projects", project, now, record)?;
    tx.commit()?;
    store::upsert_rxdb_collection_record(
        root,
        "workjet_projects",
        project,
        now,
        outbound_load_record(&conn, "workjet_projects", project)?
            .context("research projection missing")?,
    )?;
    Ok(())
}
