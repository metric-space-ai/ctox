// Origin: CTOX
// License: AGPL-3.0-only
//! The bound project supervisor narrates the project: one feed post per merged
//! PR, finished session, disruption, Regeltermin or goal change.
use super::super::workjet_project_feed::{self as feed, FeedPost, ReadFeedRequest};
use super::super::workjet_project_kpis as kpis;
use super::*;
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use serde_json::json;
pub(super) const TOOL: &str = "business_os.project_feed";

/// Supervisor skill text; also part of the tool description so every bound
/// supervisor sees it.
pub(super) const SKILL: &str = "After a PR merges to the default branch, after a worker session ends, when a worker becomes disrupted or recovers, after each Regeltermin and when the confirmed goal changes, write exactly one feed post. Write for the project owner, in the project's language, plain text, no markdown. Headline at most 140 characters. Body 1-3 sentences: what changed and why it matters. List open items separately. Cite sources (PR, commit, session, file). Never include secrets, tokens, local absolute paths or raw CLI errors; translate errors into what hangs, why and who has to act. Reuse thread_ref to connect related posts.";

#[derive(Deserialize)]
#[serde(tag = "action", content = "request", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Read(ReadFeedRequest),
    Post(FeedPost),
}
pub(super) fn allows(tool: &str, args: &Value) -> bool {
    tool == TOOL && matches!(args["action"].as_str(), Some("read" | "post"))
}
pub(super) fn descriptor() -> BusinessOsMcpToolDescriptor {
    write_tool(
        TOOL,
        &format!("Read or append project feed posts (ctox.workjet.project_feed.v1). Restricted to the current native registered project Supervisor; the project comes from the binding, never from the request. post_id is an idempotency key. {SKILL}"),
        json!({"type":"object","additionalProperties":false,"required":["action","request"],
          "properties":{"action":{"type":"string","enum":["read","post"]},"request":{"type":"object"}}}),
    )
}
pub(super) fn execute(
    root: &Path,
    context: &McpChannelRequestContext,
    args: &Value,
    trusted: Option<&Value>,
) -> anyhow::Result<Value> {
    anyhow::ensure!(allows(TOOL, args), "unsupported feed action");
    anyhow::ensure!(serde_json::to_vec(args)?.len() <= 16384, "feed request exceeds native budget");
    let request: Request = serde_json::from_value(args.clone())?;
    let writing = matches!(request, Request::Post(_));
    let flags = if writing { OpenFlags::SQLITE_OPEN_READ_WRITE } else { OpenFlags::SQLITE_OPEN_READ_ONLY };
    let behavior = if writing { TransactionBehavior::Immediate } else { TransactionBehavior::Deferred };
    let mut core = Connection::open_with_flags(crate::paths::core_db(root), flags)?;
    core.busy_timeout(std::time::Duration::from_secs(5))?;
    let mut policy = Connection::open_with_flags(store::business_os_store_path(root), flags)?;
    policy.busy_timeout(std::time::Duration::from_secs(5))?;
    let core_tx = core.transaction_with_behavior(behavior)?;
    let tx = policy.transaction_with_behavior(behavior)?;
    let (project, _thread, _) = workjet_jour_fixe::bound_project(
        &core_tx,
        &tx,
        context,
        trusted.context("restricted Supervisor session missing")?,
    )?;
    let owner = kpis::require_project(&tx, &context.actor, &project)?;
    let role = context.trusted_role.as_deref().context("native role missing")?;
    for permission in [BusinessOsPermission::DataRead, BusinessOsPermission::DataWrite] {
        if !writing && permission == BusinessOsPermission::DataWrite {
            continue;
        }
        anyhow::ensure!(
            super::super::store_policy::trusted_actor_policy_decision_with_conn(
                &tx, &context.actor, role, permission, BusinessOsScopeType::Record, Some(&project)
            )?
            .allowed,
            "feed policy denied"
        );
    }
    let result = match request {
        Request::Read(query) => {
            query.validate()?;
            anyhow::ensure!(query.project_id == project, "feed read project differs");
            let (posts, next) = feed::read_posts(&tx, &project, &owner, query.before_ms, query.limit.unwrap_or(30))?;
            json!({"ok":true,"contract":feed::CONTRACT,"posts":posts,"next_before_ms":next})
        }
        Request::Post(post) => {
            anyhow::ensure!(post.project_id == project, "feed post project differs from the binding");
            let created = feed::append_post(&tx, &owner, &post, store::now_ms() as i64)?;
            json!({"ok":true,"contract":feed::CONTRACT,"post_id":post.post_id,"created":created})
        }
    };
    tx.commit()?;
    core_tx.commit()?;
    Ok(result)
}
