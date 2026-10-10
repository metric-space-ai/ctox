// Origin: CTOX
// License: AGPL-3.0-only
//! The registered Supervisor binds a prompted metric; native readers supply all values.
use super::super::workjet_project_kpis::{self as kpis, resolver};
use super::super::workjet_project_kpis_contract as wire;
use super::*;
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use serde_json::json;
use wire::WireValidate;
pub(super) const TOOL: &str = "business_os.project_kpi";
#[derive(Deserialize)]
#[serde(
    tag = "action",
    content = "request",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Request {
    Read(wire::ReadKpisRequest),
    Resolve(wire::BindKpiRequest),
}
pub(super) fn allows(tool: &str, args: &Value) -> bool {
    tool == TOOL && matches!(args["action"].as_str(), Some("read" | "resolve"))
}
pub(super) fn descriptor() -> BusinessOsMcpToolDescriptor {
    let fixture: Value = serde_json::from_str(include_str!(
        "../rxdb/tests/fixtures/workjet-project-kpis-v1.json"
    ))
    .expect("shared KPI fixture");
    write_tool(TOOL,"Read prompted KPIs and the registered recipe catalogue, or resolve one prompt by binding a matching recipe and rolling window_days (1..365). Restricted to the current native registered project Supervisor. Native receipts calculate values; no caller values, SQL, URLs or cross-project source. expected_revision and prompt_revision fence stale results. Stable operation_id replays once; definitions refresh hourly and before JourFix. Unsupported GitHub/connected sources remain missing_source, never estimated.",
      json!({"type":"object","additionalProperties":false,"required":["action","request"],
      "properties":{"action":{"type":"string","enum":["read","resolve"]},"request":{"type":"object"}},
      "oneOf":([("read","ReadKpisRequest"),("resolve","BindKpiRequest")].iter().map(|(action,kind)| json!({"type":"object","additionalProperties":false,"required":["action","request"],
        "properties":{"action":{"const":action,"type":"string"},"request":workjet_jour_fixe::schema(&fixture,kind)}} )).collect::<Vec<_>>())}))
}
pub(super) fn execute(
    root: &Path,
    context: &McpChannelRequestContext,
    args: &Value,
    trusted: Option<&Value>,
) -> anyhow::Result<Value> {
    anyhow::ensure!(allows(TOOL, args), "unsupported KPI action");
    anyhow::ensure!(
        serde_json::to_vec(args)?.len() <= 8192,
        "KPI request exceeds native budget"
    );
    let request: Request = serde_json::from_value(args.clone())?;
    let writing = matches!(request, Request::Resolve(_));
    let flags = if writing {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let behavior = if writing {
        TransactionBehavior::Immediate
    } else {
        TransactionBehavior::Deferred
    };
    let mut core = Connection::open_with_flags(crate::paths::core_db(root), flags)?;
    core.busy_timeout(std::time::Duration::from_secs(5))?;
    let mut policy = Connection::open_with_flags(store::business_os_store_path(root), flags)?;
    policy.busy_timeout(std::time::Duration::from_secs(5))?;
    let core_tx = core.transaction_with_behavior(behavior)?;
    let tx = policy.transaction_with_behavior(behavior)?;
    let (project, thread, _) = workjet_jour_fixe::bound_project(
        &core_tx,
        &tx,
        context,
        trusted.context("restricted Supervisor session missing")?,
    )?;
    let owner = kpis::require_project(&tx, &context.actor, &project)?;
    let role = context
        .trusted_role
        .as_deref()
        .context("native role missing")?;
    for permission in [
        BusinessOsPermission::DataRead,
        BusinessOsPermission::DataWrite,
    ] {
        if !writing && permission == BusinessOsPermission::DataWrite {
            continue;
        }
        anyhow::ensure!(
            super::super::store_policy::trusted_actor_policy_decision_with_conn(
                &tx,
                &context.actor,
                role,
                permission,
                BusinessOsScopeType::Record,
                Some(&project)
            )?
            .allowed,
            "KPI policy denied"
        );
    }
    let result = match request {
        Request::Read(query) => {
            query.validate().map_err(anyhow::Error::msg)?;
            anyhow::ensure!(query.project_id == project, "KPI read project differs");
            json!({"ok":true,"contract":wire::CONTRACT_SCHEMA,"kpis":kpis::read_state_with_schedule(&tx,&project,&owner,query.include_refresh_schedule.unwrap_or(false))?,"recipes":resolver::catalogue()})
        }
        Request::Resolve(query) => resolver::resolve(
            &core_tx,
            &tx,
            &context.actor,
            &project,
            &thread,
            &query,
            store::now_ms() as i64,
        )?,
    };
    tx.commit()?;
    core_tx.commit()?;
    Ok(result)
}
#[cfg(test)]
#[path = "mcp_workjet_kpis_tests.rs"]
mod tests;
