// Origin: CTOX
// License: AGPL-3.0-only
//! Registered recipes read actual project task receipts. No caller numbers,
//! SQL, URLs, or unverified external metrics enter the calculation.
use super::*;
use crate::business_os::workjet_project_kpis_contract::{
    BindKpiRequest, Calculation, Computation, Freshness, KpiSnapshot, NativeMetricRecipe,
    SourceEvidence, SourceKind,
};
use rusqlite::{OpenFlags, TransactionBehavior};

const HOUR: i64 = 60 * 60 * 1000;
const DEFINITIONS: &str = "CREATE TABLE IF NOT EXISTS workjet_project_kpi_definitions (
 project_id TEXT NOT NULL, kpi_id TEXT NOT NULL, owner_user_id TEXT NOT NULL,
 supervisor_thread_id TEXT NOT NULL, prompt_revision INTEGER NOT NULL,
 request_json TEXT NOT NULL, next_refresh_ms INTEGER NOT NULL,
 PRIMARY KEY(project_id,kpi_id));";

pub(super) fn snapshot_binding_is_current(
    conn: &Connection,
    project: &str,
    owner: &str,
    prompt: &KpiPrompt,
) -> anyhow::Result<bool> {
    if !has(conn, "workjet_project_kpi_definitions")? {
        return Ok(true);
    }
    let definition: Option<(String, String, u64)> = conn.query_row(
      "SELECT owner_user_id,supervisor_thread_id,prompt_revision FROM workjet_project_kpi_definitions WHERE project_id=?1 AND kpi_id=?2",
      params![project,prompt.kpi_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let Some((bound_owner, thread, revision)) = definition else {
        return Ok(false);
    };
    if bound_owner != owner || revision != prompt.revision {
        return Ok(false);
    }
    match super::super::project_chats::supervisor_turns::binding_from_connection(
        conn, owner, project, &thread, true,
    ) {
        Ok(_) => Ok(true),
        Err(error)
            if error
                .chain()
                .any(|v| v.is::<rusqlite::Error>() || v.is::<std::io::Error>()) =>
        {
            Err(error)
        }
        Err(_) => Ok(false),
    }
}

pub(in crate::business_os) fn catalogue() -> Value {
    json!([
      {"recipe":"project_tasks_total","label":"Tasks","meaning":"Native queued Supervisor work requests admitted to this project within the rolling window; explicit conversation replies are excluded."},
      {"recipe":"project_tasks_completed","label":"Erledigt","meaning":"Those commands with a terminal completed receipt, not model claims or lease completion."},
      {"recipe":"project_tasks_failed","label":"Fehlversuche","meaning":"Those commands with a terminal failed receipt."},
      {"recipe":"project_tasks_open","label":"Offene Tasks","meaning":"Those commands whose native execution phase is not terminal."},
      {"recipe":"project_tasks_success_rate","label":"Erfolgsquote","meaning":"Completed / (completed + failed), percent. No finished tasks means missing_source, not zero."},
      {"recipe":"github_merged_prs","label":"Gemergte PRs","meaning":"Requires a verified GitHub metric adapter; currently missing_source. Native task completions cannot substitute for merged PRs."}
    ])
}
fn has(conn: &Connection, table: &str) -> anyhow::Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |r| r.get(0),
    )?)
}
fn save(conn: &Connection, owner: &str, state: &ProjectKpis) -> anyhow::Result<()> {
    state.validate().map_err(anyhow::Error::msg)?;
    conn.execute(
        "INSERT INTO workjet_project_kpi_state(project_id,owner_user_id,state_json)
      VALUES(?1,?2,?3) ON CONFLICT(project_id) DO UPDATE SET state_json=excluded.state_json",
        params![state.project_id, owner, serde_json::to_string(state)?],
    )?;
    Ok(())
}
fn missing(code: &str, message: &str) -> KpiResult {
    KpiResult {
        status: KpiState::MissingSource,
        snapshot: None,
        reason_code: Some(code.into()),
        message: Some(message.into()),
    }
}
fn calculate(
    core: &Connection,
    owner: &str,
    thread: &str,
    request: &BindKpiRequest,
    now: i64,
) -> anyhow::Result<KpiResult> {
    if request.recipe == NativeMetricRecipe::GithubMergedPrs {
        return Ok(missing("github_metric_not_connected", "No verified GitHub metric adapter is connected. Task receipts cannot prove merged PRs."));
    }
    if !has(core, "business_command_aggregates")? {
        return Ok(missing(
            "native_task_store_unavailable",
            "The native project command ledger is unavailable.",
        ));
    }
    let start = now
        .saturating_sub(request.window_days as i64 * 24 * HOUR)
        .max(0);
    // Canonical native Supervisor turns retain the current project, owner and
    // registered thread in their admitted envelope. Neither a same-name thread
    // nor a foreign actor's command can add to this source.
    let (total,completed,failed,open,watermark):(u64,u64,u64,u64,i64) = core.query_row(
      "SELECT count(*), coalesce(sum(execution_phase='terminal' AND terminal_status='completed'),0),
       coalesce(sum(execution_phase='terminal' AND terminal_status='failed'),0),
       coalesce(sum(execution_phase!='terminal'),0), coalesce(max(updated_at_ms),0)
       FROM business_command_aggregates WHERE module='ctox' AND command_type='business_os.chat.task'
       AND execution_mode='queue' AND record_id=?1 AND created_at_ms>=?2 AND created_at_ms<=?3
       AND json_extract(intent_json,'$.payload.thread_id')=?4
       AND json_extract(intent_json,'$.payload.risk_class')='internal'
       AND json_extract(intent_json,'$.client_context.actor.id')=?5
       AND coalesce(json_extract(intent_json,'$.payload.supervisor_turn.kind'),'work')!='conversation'",
       params![request.project_id,start,now,thread,owner], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
    let (label, unit, metric, value, values, operation) = match request.recipe {
        NativeMetricRecipe::ProjectTasksTotal => (
            "Tasks",
            "tasks",
            "project_tasks.total",
            total as f64,
            vec![total],
            Calculation::Identity,
        ),
        NativeMetricRecipe::ProjectTasksCompleted => (
            "Erledigt",
            "tasks",
            "project_tasks.completed",
            completed as f64,
            vec![completed],
            Calculation::Identity,
        ),
        NativeMetricRecipe::ProjectTasksFailed => (
            "Fehlversuche",
            "tasks",
            "project_tasks.failed",
            failed as f64,
            vec![failed],
            Calculation::Identity,
        ),
        NativeMetricRecipe::ProjectTasksOpen => (
            "Offene Tasks",
            "tasks",
            "project_tasks.open",
            open as f64,
            vec![open],
            Calculation::Identity,
        ),
        NativeMetricRecipe::ProjectTasksSuccessRate => {
            let finished = completed
                .checked_add(failed)
                .context("native task count overflow")?;
            if finished == 0 {
                return Ok(missing(
                    "no_terminal_task_receipts",
                    "No completed or failed project task receipts exist in this window.",
                ));
            }
            (
                "Erfolgsquote",
                "%",
                "project_tasks.success_rate",
                completed as f64 * 100.0 / finished as f64,
                vec![completed, finished],
                Calculation::Percentage,
            )
        }
        NativeMetricRecipe::GithubMergedPrs => unreachable!(),
    };
    ensure!(
        total <= 9_007_199_254_740_991,
        "task count exceeds exact wire precision"
    );
    let revision = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(
            &json!({"project":request.project_id,"owner":owner,"thread":thread,
      "window":[start,now],"counts":[total,completed,failed,open],"watermark":watermark})
        )?)
    );
    let keys: Vec<String> = (0..values.len()).map(|n| format!("native-{n}")).collect();
    let sources = values
        .iter()
        .enumerate()
        .map(|(i, v)| SourceEvidence {
            source_key: keys[i].clone(),
            kind: SourceKind::Native,
            connection_id: "native-core-command-ledger".into(),
            metric_key: if operation == Calculation::Percentage {
                if i == 0 {
                    "project_tasks.completed"
                } else {
                    "project_tasks.finished"
                }
            } else {
                metric
            }
            .into(),
            project_id: request.project_id.clone(),
            snapshot_revision: revision.clone(),
            evidence_ref: format!("native-project-task-snapshot:{revision}"),
            observed_at_ms: now,
            value: *v as f64,
        })
        .collect();
    let snapshot = KpiSnapshot {
        project_id: request.project_id.clone(),
        kpi_id: request.kpi_id.clone(),
        prompt_revision: request.prompt_revision,
        label: label.into(),
        value,
        unit: unit.into(),
        display_value: if unit == "%" {
            format!("{value:.1} %")
        } else {
            format!("{value:.0}")
        },
        sources,
        computation: Computation {
            recipe_id: metric.into(),
            revision: 1,
            operation,
            input_keys: keys,
            window_start_ms: start,
            window_end_ms: now,
        },
        freshness: Freshness {
            calculated_at_ms: now,
            refresh_at_ms: now.checked_add(HOUR).context("refresh overflow")?,
            fresh_until_ms: now.checked_add(2 * HOUR).context("expiry overflow")?,
        },
    };
    snapshot.validate().map_err(anyhow::Error::msg)?;
    Ok(KpiResult {
        status: KpiState::Ready,
        snapshot: Some(snapshot),
        reason_code: None,
        message: None,
    })
}

pub(in crate::business_os) fn resolve(
    core: &Connection,
    policy: &Connection,
    actor: &str,
    project: &str,
    thread: &str,
    request: &BindKpiRequest,
    now: i64,
) -> anyhow::Result<Value> {
    request.validate().map_err(anyhow::Error::msg)?;
    ensure!(
        request.project_id == project,
        "KPI belongs to another Supervisor project"
    );
    let owner = require_project(policy, actor, project)?;
    super::super::project_chats::supervisor_turns::binding_from_connection(
        policy, &owner, project, thread, true,
    )?;
    let mut state = load(policy, project, &owner)?;
    let item = state
        .items
        .iter()
        .find(|v| v.prompt.kpi_id == request.kpi_id)
        .context("KPI prompt unavailable")?;
    ensure!(
        item.prompt.revision == request.prompt_revision,
        "KPI prompt revision changed"
    );
    policy.execute_batch(SCHEMA)?;
    policy.execute_batch(DEFINITIONS)?;
    let hash = format!(
        "{:x}",
        Sha256::digest(format!("resolve:{}", serde_json::to_string(request)?))
    );
    let replay:Option<(String,String,String)>=policy.query_row("SELECT owner_user_id,intent_hash,result_json FROM workjet_project_kpi_operations WHERE operation_id=?1",[&request.operation_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    if let Some((prior_owner, prior_hash, raw)) = replay {
        ensure!(
            prior_owner == owner && prior_hash == hash,
            "KPI operation intent conflicts"
        );
        return Ok(serde_json::from_str(&raw)?);
    }
    ensure!(
        state.revision == request.expected_revision,
        "KPI revision conflict"
    );
    let result = calculate(core, &owner, thread, request, now)?;
    state
        .items
        .iter_mut()
        .find(|v| v.prompt.kpi_id == request.kpi_id)
        .unwrap()
        .result = result;
    state.revision = state
        .revision
        .checked_add(1)
        .context("KPI revision overflow")?;
    save(policy, &owner, &state)?;
    policy.execute("INSERT INTO workjet_project_kpi_definitions VALUES(?1,?2,?3,?4,?5,?6,?7)
      ON CONFLICT(project_id,kpi_id) DO UPDATE SET owner_user_id=excluded.owner_user_id,supervisor_thread_id=excluded.supervisor_thread_id,
      prompt_revision=excluded.prompt_revision,request_json=excluded.request_json,next_refresh_ms=excluded.next_refresh_ms",
      params![project,request.kpi_id,owner,thread,request.prompt_revision,serde_json::to_string(request)?,now.checked_add(HOUR).context("refresh overflow")?])?;
    let receipt = json!({"ok":true,"contract":super::super::workjet_project_kpis_contract::CONTRACT_SCHEMA,"kpis":state});
    policy.execute(
        "INSERT INTO workjet_project_kpi_operations VALUES(?1,?2,?3,?4)",
        params![
            request.operation_id,
            owner,
            hash,
            serde_json::to_string(&receipt)?
        ],
    )?;
    Ok(receipt)
}

pub(in crate::business_os) fn refresh_project(
    root: &Path,
    project: &str,
    force: bool,
) -> anyhow::Result<()> {
    refresh_at(
        root,
        Some(project),
        force,
        super::super::store::now_ms() as i64,
    )
}
pub(in crate::business_os) fn refresh_due(root: &Path) -> anyhow::Result<()> {
    refresh_at(root, None, false, super::super::store::now_ms() as i64)
}
fn refresh_at(root: &Path, project: Option<&str>, force: bool, now: i64) -> anyhow::Result<()> {
    // No writer acquired on ticks without due definitions.
    let mut reader = Connection::open_with_flags(
        super::super::store::business_os_store_path(root),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    reader.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    let read_tx = reader.transaction()?;
    if !has(&read_tx, "workjet_project_kpi_definitions")? {
        return Ok(());
    }
    let projects=read_tx.prepare("SELECT DISTINCT project_id FROM workjet_project_kpi_definitions
       WHERE (?1 IS NULL OR project_id=?1) AND (?2 OR next_refresh_ms<=?3) ORDER BY project_id LIMIT 101")?
       .query_map(params![project,force,now],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        projects.len() <= 100,
        "KPI refresh exceeds native project budget"
    );
    drop(read_tx);
    drop(reader);
    if projects.is_empty() {
        return Ok(());
    }
    let core_path = crate::paths::core_db(root);
    if !core_path.exists() {
        return Ok(());
    }
    let mut core = Connection::open_with_flags(core_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    core.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    let core_tx = core.transaction()?;
    let mut policy = open_store(root)?;
    let tx = policy.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for project in projects {
        let definitions=tx.prepare("SELECT owner_user_id,supervisor_thread_id,request_json FROM workjet_project_kpi_definitions
         WHERE project_id=?1 AND (?2 OR next_refresh_ms<=?3) ORDER BY kpi_id LIMIT 4")?
         .query_map(params![project,force,now],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        ensure!(definitions.len() <= 3, "KPI definitions exceed prompt cap");
        for (owner, thread, raw) in definitions {
            let binding = super::super::project_chats::supervisor_turns::binding_from_connection(
                &tx, &owner, &project, &thread, true,
            );
            if let Err(error) = binding {
                if error
                    .chain()
                    .any(|v| v.is::<rusqlite::Error>() || v.is::<std::io::Error>())
                {
                    return Err(error);
                }
                continue;
            }
            // A revoked Owner or changed role grants no autonomous read/write.
            let role: Option<String> = tx
                .query_row(
                    "SELECT role FROM business_users WHERE user_id=?1 AND active=1",
                    [&owner],
                    |r| r.get(0),
                )
                .optional()?;
            let Some(role) = role else { continue };
            let mut allowed = true;
            for permission in [
                super::super::policy::BusinessOsPermission::DataRead,
                super::super::policy::BusinessOsPermission::DataWrite,
            ] {
                allowed &= super::super::store_policy::trusted_actor_policy_decision_with_conn(
                    &tx,
                    &owner,
                    &role,
                    permission,
                    super::super::policy::BusinessOsScopeType::Record,
                    Some(&project),
                )?
                .allowed;
            }
            if !allowed {
                continue;
            }
            let request: BindKpiRequest = serde_json::from_str(&raw)?;
            request.validate().map_err(anyhow::Error::msg)?;
            ensure!(
                request.project_id == project,
                "KPI definition project differs"
            );
            let mut state = load(&tx, &project, &owner)?;
            let Some(item) = state.items.iter_mut().find(|v| {
                v.prompt.kpi_id == request.kpi_id && v.prompt.revision == request.prompt_revision
            }) else {
                continue;
            };
            item.result = calculate(&core_tx, &owner, &thread, &request, now)?;
            state.revision = state
                .revision
                .checked_add(1)
                .context("KPI revision overflow")?;
            save(&tx, &owner, &state)?;
            tx.execute("UPDATE workjet_project_kpi_definitions SET next_refresh_ms=?3 WHERE project_id=?1 AND kpi_id=?2",params![project,request.kpi_id,now.checked_add(HOUR).context("refresh overflow")?])?;
        }
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
pub(in crate::business_os) fn refresh_test_at(
    root: &Path,
    project: Option<&str>,
    force: bool,
    now: i64,
) -> anyhow::Result<()> {
    refresh_at(root, project, force, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_task_metrics_exclude_conversations_and_keep_legacy_work() -> anyhow::Result<()> {
        let core = Connection::open_in_memory()?;
        core.execute_batch(
            "CREATE TABLE business_command_aggregates (
                module TEXT, command_type TEXT, execution_mode TEXT, record_id TEXT,
                created_at_ms INTEGER, updated_at_ms INTEGER, intent_json TEXT,
                execution_phase TEXT, terminal_status TEXT
            )",
        )?;
        let now = 100 * HOUR;
        for (kind, phase, status, owner, thread, project) in [
            (None, "terminal", "completed", "owner", "thread", "project"),
            (Some("work"), "running", "", "owner", "thread", "project"),
            (
                Some("work"),
                "terminal",
                "failed",
                "owner",
                "thread",
                "project",
            ),
            (
                Some("conversation"),
                "terminal",
                "completed",
                "owner",
                "thread",
                "project",
            ),
            (
                Some("conversation"),
                "running",
                "",
                "owner",
                "thread",
                "project",
            ),
            (
                Some("work"),
                "terminal",
                "completed",
                "foreign",
                "thread",
                "project",
            ),
            (
                Some("work"),
                "terminal",
                "completed",
                "owner",
                "foreign",
                "project",
            ),
            (
                Some("work"),
                "terminal",
                "completed",
                "owner",
                "thread",
                "foreign",
            ),
        ] {
            let mut intent = json!({
                "payload": {"thread_id": thread, "risk_class": "internal"},
                "client_context": {"actor": {"id": owner}}
            });
            if let Some(kind) = kind {
                intent["payload"]["supervisor_turn"] =
                    json!({"kind": kind, "submit_command_id": "submit"});
            }
            core.execute(
                "INSERT INTO business_command_aggregates VALUES ('ctox','business_os.chat.task','queue',?1,?2,?2,?3,?4,?5)",
                params![project, now - HOUR, serde_json::to_string(&intent)?, phase, status],
            )?;
        }
        for (recipe, expected) in [
            (NativeMetricRecipe::ProjectTasksTotal, 3.0),
            (NativeMetricRecipe::ProjectTasksCompleted, 1.0),
            (NativeMetricRecipe::ProjectTasksFailed, 1.0),
            (NativeMetricRecipe::ProjectTasksOpen, 1.0),
        ] {
            let request = BindKpiRequest {
                operation_id: "metric".into(),
                project_id: "project".into(),
                kpi_id: "tasks".into(),
                prompt_revision: 1,
                expected_revision: 0,
                recipe,
                window_days: 7,
            };
            let result = calculate(&core, "owner", "thread", &request, now)?;
            assert_eq!(result.status, KpiState::Ready);
            assert_eq!(result.snapshot.context("snapshot")?.value, expected);
        }
        Ok(())
    }
}
