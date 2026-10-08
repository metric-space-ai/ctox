// Origin: CTOX
// License: AGPL-3.0-only
//! Read-only account calendar access, pinned to authenticated mailbox ownership.
use super::*;
use crate::communication::{email_accounts, email_native};
use rusqlite::{Connection, OpenFlags};
use sha2::{Digest, Sha256};
#[path = "workjet_calendar_contract.generated.rs"]
mod wire;
use wire::WireValidate;

pub(super) const ACCOUNTS_TOOL: &str = "business_os.calendar_accounts";
pub(super) const EVENTS_TOOL: &str = "business_os.calendar_events";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyRequest {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EventsRequest {
    account_id: String,
    start_ms: i64,
    end_ms: i64,
}
pub(super) fn descriptors() -> Vec<BusinessOsMcpToolDescriptor> {
    vec![read_tool(ACCOUNTS_TOOL,
        "List registered calendars owned by or explicitly shared with the authenticated user. No credentials, instance mailbox fallback or caller identity. Unsupported providers are marked unavailable.",
        json!({"type":"object","additionalProperties":false,"properties":{}})),
        read_tool(EVENTS_TOOL,
        "Read up to 100 events/recurring occurrences from one owned/shared registered EWS or Graph calendar over an ordered range of at most 400 days. Read-only; truncated is explicit. No arbitrary URL, credentials or caller identity. Uses ctox.workjet.calendar.v1 events.",
        json!({"type":"object","additionalProperties":false,"required":["account_id","start_ms","end_ms"],"properties":{
            "account_id":{"type":"string","minLength":1,"maxLength":256},"start_ms":{"type":"integer"},"end_ms":{"type":"integer"}}}))]
}
fn authorized_accounts(root: &Path, context: &McpChannelRequestContext) -> anyhow::Result<Vec<email_accounts::EmailAccountConfig>> {
    enforce_managed_collection_read_scope(context, "communication_accounts")?;
    let conn = Connection::open_with_flags(store::business_os_store_path(root), OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    let actor = super::super::policy::BusinessOsActor::new(Some(context.actor.clone()),
        context.trusted_role.as_deref().context("calendar trusted role missing")?);
    let owner = super::super::workjet_identity::owner_from_connection(&conn, &context.actor)?;
    let accounts = email_accounts::load_accounts(root)?;
    accounts.into_iter().filter_map(|account| {
        let identities = std::iter::once(&account.owner_user_id)
            .chain(account.shared_user_ids.iter().flatten());
        let allowed = identities.filter(|id| !id.is_empty()).try_fold(false, |allowed, id| {
            Ok::<_, anyhow::Error>(allowed || super::super::workjet_identity::owner_from_connection(&conn, id)? == owner)
        });
        match allowed {
            Ok(true) => {
                let scope = super::super::policy::BusinessOsScope {
                    scope_type: BusinessOsScopeType::Record, scope_id: Some(account.address.clone()),
                    assigned_to_actor: true, owned_by_actor: true,
                };
                match super::super::store_policy::evaluate_policy_with_explicit_grants(&conn, &actor, BusinessOsPermission::DataRead, &scope) {
                    Ok(decision) if decision.allowed => Some(Ok(account)),
                    Ok(_) => None, Err(error) => Some(Err(error)),
                }
            }
            Ok(false) => None, Err(error) => Some(Err(error)),
        }
    }).collect()
}
fn supported(provider: &str) -> bool { matches!(provider, "ews" | "owa" | "exchange" | "graph") }
fn calendar_id(address: &str) -> String {
    format!("account:{:x}", Sha256::digest(address.as_bytes()))
}
fn wire_events(account_id: &str, page: &Value) -> anyhow::Result<Value> {
    let events = page["events"].as_array().context("calendar provider returned no events")?;
    anyhow::ensure!(events.len() <= 100, "calendar provider exceeded its event budget");
    let result = events.iter().map(|event| {
        let external = event["external_id"].as_str().context("calendar event has no identity")?;
        let digest = Sha256::digest(serde_json::to_vec(event)?);
        let revision = digest[..6].iter().fold(0u64, |value, byte| (value << 8) | *byte as u64);
        Ok::<_, anyhow::Error>(json!({
            "id": format!("event:{:x}", Sha256::digest(format!("{account_id}:{external}:{}", event["start_ms"]).as_bytes())),
            "calendar_id": calendar_id(account_id), "kind":"synced", "account_id":account_id,
            "external_id":external, "title":event["title"], "start_ms":event["start_ms"], "end_ms":event["end_ms"],
            "all_day":event["all_day"], "timezone":"UTC", "location":event["location"], "revision":revision
        }))
    }).collect::<anyhow::Result<Vec<_>>>()?;
    for value in &result {
        serde_json::from_value::<wire::CalendarEvent>(value.clone())?.validate().map_err(anyhow::Error::msg)?;
    }
    bounded_receipt(json!({"ok":true,"events":result,"truncated":page["truncated"],"synced_at_ms":store::now_ms()}), "events")
}
fn bounded_receipt(mut value: Value, list: &str) -> anyhow::Result<Value> {
    loop {
        if let Ok(receipt) = mcp_tool_result(value.clone()) {
            if serde_json::to_vec(&receipt)?.len() + 1024 <= MAX_MCP_RESPONSE_BYTES { return Ok(value); }
        }
        anyhow::ensure!(value[list].as_array_mut().and_then(|items| items.pop()).is_some(), "calendar receipt exceeds its budget");
        value["truncated"] = json!(true);
    }
}
pub(super) fn execute(root: &Path, context: &McpChannelRequestContext, tool: &str, args: &Value, trusted_gateway_context: Option<&Value>) -> anyhow::Result<Value> {
    if context.trusted_role_source.as_deref() == Some("ctox_dev_managed_mcp_token") {
        if let Some(tools) = trusted_gateway_context.and_then(|gateway| gateway["managed_policy"].get("allowedTools")) {
            let tools = tools.as_array().context("invalid managed calendar tool scope")?;
            anyhow::ensure!(tools.is_empty() || tools.iter().any(|item| item.as_str() == Some(tool)),
                "calendar tool is outside this managed client scope");
        }
    }
    anyhow::ensure!(serde_json::to_vec(args)?.len() <= 1024, "calendar request exceeds its budget");
    match tool {
        ACCOUNTS_TOOL => {
            let _: EmptyRequest = serde_json::from_value(args.clone())?;
            let accounts = authorized_accounts(root, context)?;
            bounded_receipt(json!({"ok":true,"truncated":accounts.len() > 100,"accounts":accounts.iter().take(100).map(|account| json!({
                "id":account.address,"calendar_id":calendar_id(&account.address),
                "label":if account.display_name.is_empty() { account.address.clone() } else { account.display_name.chars().take(256).collect::<String>() },
                "supported":supported(&account.provider)
            })).collect::<Vec<_>>()}), "accounts")
        }
        EVENTS_TOOL => {
            let request: EventsRequest = serde_json::from_value(args.clone())?;
            anyhow::ensure!(!request.account_id.is_empty() && request.account_id.chars().count() <= 256, "invalid calendar account");
            anyhow::ensure!(request.end_ms.checked_sub(request.start_ms).is_some_and(|span| span > 0 && span <= 400 * 86_400_000), "invalid calendar range");
            let account = authorized_accounts(root, context)?.into_iter().find(|a| a.address == request.account_id)
                .context("calendar account is not owned or shared")?;
            anyhow::ensure!(supported(&account.provider), "calendar provider unsupported");
            // All policy connections are dropped before making a provider request.
            // Provider errors may contain sensitive bodies: never echo them to the caller.
            let page = email_native::read_registered_calendar(root, &account, request.start_ms, request.end_ms, 100)
                .map_err(|_| anyhow::anyhow!("calendar account sync unavailable"))?;
            anyhow::ensure!(authorized_accounts(root, context)?.iter().any(|fresh| serde_json::to_value(fresh).ok() == serde_json::to_value(&account).ok()), "calendar account authorization changed during sync");
            wire_events(&account.address, &page)
        }
        _ => anyhow::bail!("unsupported calendar tool"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calendar_shared_user_and_verified_alias_are_record_scoped_and_revocable() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let canonical = "196a89ba-ee86-4413-885c-04ca60e6f291";
        let alias = "owner@example.test";
        for actor in [canonical, alias, "shared", "foreign"] {
            store::tests::seed_business_user(root.path(), actor, "user")?;
        }
        save_mcp_policy(root.path(), &default_mcp_policy())?;
        super::super::super::workjet_identity::remember_managed_identity(
            &store::open_store(root.path())?, canonical, Some(alias), store::now_ms(),
        )?;
        let save = |shared: Vec<String>| -> anyhow::Result<()> {
            crate::inference::runtime_env::save_runtime_env_map(root.path(), &std::collections::BTreeMap::from([
                (email_accounts::REGISTRY_ENV_KEY.into(), serde_json::to_string(&vec![
                    email_accounts::EmailAccountConfig {
                        address: "calendar@example.test".into(), owner_user_id: canonical.into(),
                        shared_user_ids: Some(shared), provider: "ews".into(), ..Default::default()
                    },
                ])?)
            ]))
        };
        save(vec!["shared".into()])?;
        let read = |actor: &str| -> anyhow::Result<Value> {
            let gateway = json!({"auth_source":"ctox_dev_managed_mcp_token","channel":"ctox_dev_managed_mcp","surface":"workjet","actor":actor,"role":"user","workspace":"tenant:instance","instance_id":"source-instance","managed_policy":{"allowReads":true,"allowedCollections":["communication_accounts"]}});
            call_tool_inner(root.path(), ACCOUNTS_TOOL, json!({}), Some(&gateway))
        };
        for actor in [canonical, alias, "shared"] {
            assert_eq!(read(actor)?["accounts"][0]["id"], "calendar@example.test");
        }
        assert_eq!(read("foreign")?["accounts"].as_array().unwrap().len(), 0);
        save(vec![])?;
        assert_eq!(read("shared")?["accounts"].as_array().unwrap().len(), 0);
        store::open_store(root.path())?.execute("UPDATE business_users SET active=0 WHERE user_id=?1", [canonical])?;
        assert!(read(alias).is_err());
        Ok(())
    }
    #[test]
    fn calendar_accounts_never_expose_foreign_or_ownerless_mailboxes() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        for actor in ["owner", "other"] { store::tests::seed_business_user(root.path(), actor, "admin")?; }
        save_mcp_policy(root.path(), &default_mcp_policy())?;
        crate::inference::runtime_env::save_runtime_env_map(root.path(), &std::collections::BTreeMap::from([
            (email_accounts::REGISTRY_ENV_KEY.into(), serde_json::to_string(&vec![
                email_accounts::EmailAccountConfig { address:"mine@example.test".into(), owner_user_id:"owner".into(), provider:"ews".into(), ..Default::default() },
                email_accounts::EmailAccountConfig { address:"foreign@example.test".into(), owner_user_id:"other".into(), ..Default::default() },
                email_accounts::EmailAccountConfig { address:"ownerless@example.test".into(), ..Default::default() },
            ])?)
        ]))?;
        let gateway = json!({"auth_source":"ctox_dev_managed_mcp_token","channel":"ctox_dev_managed_mcp","surface":"workjet","actor":"owner","role":"admin","workspace":"tenant:instance","instance_id":"source-instance","managed_policy":{"allowReads":true,"allowedCollections":["communication_accounts"]}});
        let read = call_tool_inner(root.path(), ACCOUNTS_TOOL, json!({}), Some(&gateway))?;
        assert_eq!(read["accounts"].as_array().unwrap().len(), 1);
        assert_eq!(read["accounts"][0]["id"], "mine@example.test");
        assert!(call_tool_inner(root.path(), EVENTS_TOOL, json!({"account_id":"foreign@example.test","start_ms":0,"end_ms":1}), Some(&gateway)).is_err());
        assert!(call_tool_inner(root.path(), ACCOUNTS_TOOL, json!({"actor":"other"}), Some(&gateway)).is_err());
        let mut bounded = gateway.clone();
        bounded["managed_policy"]["allowedTools"] = json!(["business_os.list_modules"]);
        assert!(call_tool_inner(root.path(), ACCOUNTS_TOOL, json!({}), Some(&bounded)).is_err());
        bounded["managed_policy"]["allowedTools"] = json!([ACCOUNTS_TOOL]);
        bounded["managed_policy"]["allowReads"] = json!(false);
        assert!(call_tool_inner(root.path(), ACCOUNTS_TOOL, json!({}), Some(&bounded)).is_err());
        let mut policy = default_mcp_policy(); policy.allowed_collections = vec!["workjet_projects".into()]; save_mcp_policy(root.path(), &policy)?;
        assert!(call_tool_inner(root.path(), ACCOUNTS_TOOL, json!({}), Some(&gateway)).is_err());
        Ok(())
    }
}
