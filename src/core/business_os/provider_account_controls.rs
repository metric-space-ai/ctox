// Origin: CTOX
// License: AGPL-3.0-only
//! Explicit local Claude holder mutation with cross-store stages. A Policy
//! reservation is not an applied receipt. Runtime holds exact mutation proof;
//! only proof-bearing recovery can finish cleanup and commit the final receipt.
use super::*;
use crate::execution::cliproxyapi_host::account_controls as holder;

#[cfg(test)]
#[path = "provider_account_controls_tests.rs"]
mod tests;
use serde::{Deserialize, Serialize};

pub(in crate::business_os) fn supports(kind: &str) -> bool {
    matches!(
        kind,
        "ctox.workjet.providers.account.enable" | "ctox.workjet.providers.account.remove"
    )
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Request {
    #[serde(default, rename = "inbound_channel", skip_serializing)]
    _inbound_channel: Option<String>,
    account_id: String,
    expected_account_revision: i64,
    expected_revision: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
}
#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct Target {
    owner: String,
    holder: String,
    local: String,
    binding: String,
}
#[derive(Serialize, Deserialize)]
struct Pending {
    request: Request,
    target: Target,
}
fn target(conn: &Connection, actor: &str, local_holder: &str, request: &Request) -> Result<Target> {
    let owner = management_owner(conn, actor)?;
    ensure!(
        policy_revision(conn, &owner)? == request.expected_revision,
        "provider policy revision conflict"
    );
    conn.query_row(
        "SELECT a.private_local_account_id,b.fingerprint FROM business_provider_federation_accounts a
         JOIN business_provider_federation_native_bindings b ON b.account_id=a.account_id
         WHERE a.account_id=?1 AND a.owner_user_id=?2 AND a.holder_instance_id=?3
         AND a.provider='claude' AND a.private_local_account_id<>?4 AND a.revision=?5",
        params![request.account_id,owner,local_holder,INHERITED_NATIVE_ACCOUNT_ID,request.expected_account_revision],
        |row| Ok(Target { owner:owner.clone(),holder:local_holder.into(),local:row.get(0)?,binding:row.get(1)? }),
    ).context("native account is unavailable, unsupported or changed")
}
pub(super) fn handle(
    root: &Path,
    command: &BusinessCommand,
    actor: &str,
    admission: Option<&DomainEffectAdmission>,
) -> Result<Value> {
    let request: Request = serde_json::from_value(command.payload.clone())?;
    bounded_id(&request.account_id)?;
    ensure!(
        request.expected_account_revision > 0
            && request.expected_account_revision < i64::MAX
            && request.expected_revision >= 0
            && request.expected_revision < i64::MAX,
        "provider revisions are missing"
    );
    ensure!(
        if command.command_type.ends_with(".enable") {
            request.enabled.is_some()
        } else {
            !command
                .payload
                .as_object()
                .context("invalid account request")?
                .contains_key("enabled")
        },
        "invalid account control intent"
    );
    let admitted = admission.context("account control requires new domain admission")?;
    let (id, hash, admitted_actor) = admitted.staged_identity();
    uuid::Uuid::parse_str(id).context("account operation id must be a UUID")?;
    ensure!(
        command.id.as_deref() == Some(id) && actor == admitted_actor,
        "account control claim identity mismatch"
    );
    let core = crate::channels::business_command_projection(root, id)?;
    ensure!(
        core["payload"] == command.payload,
        "account control intent changed"
    );
    ensure!(
        core["payload_hash"] == hash
            && core["terminal_status"].as_str().unwrap_or("none") == "none",
        "account control Core claim changed"
    );
    let local_holder = store::existing_instance_id(root)?;
    let pending = {
        let mut conn = store::open_store(root)?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let selected = target(&tx, actor, &local_holder, &request)?;
        let pending = Pending {
            request,
            target: selected,
        };
        tx.execute("INSERT INTO business_provider_account_controls(command_id,payload_hash,actor_user_id,owner_user_id,pending_json) VALUES (?1,?2,?3,?4,?5)",params![id,hash,actor,pending.target.owner,serde_json::to_string(&pending)?])?;
        tx.commit()?;
        pending
    };
    // No Policy transaction or store connection is held across config/secrets IO.
    // Runtime CAS will refuse a changed account, including a re-login.
    if let Err(error) = execute_reserved(root, command, hash, actor, &pending) {
        if !holder::has_effect(root, id)? {
            // A known pre-COMMIT rejection has no external effect to recover.
            store::open_store(root)?.execute("DELETE FROM business_provider_account_controls WHERE command_id=?1 AND completed=0",[id])?;
        }
        return Err(error);
    }
    recover(root, command, hash, actor)?.context("account control did not commit its result")
}

fn execute_reserved(
    root: &Path,
    command: &BusinessCommand,
    hash: &str,
    actor: &str,
    pending: &Pending,
) -> Result<()> {
    validate_reserved(root, command, hash, actor, pending)?;
    holder::apply(
        root,
        command
            .id
            .as_deref()
            .context("account operation id is missing")?,
        hash,
        actor,
        &pending.target.local,
        &pending.target.binding,
        pending.request.enabled,
        || validate_reserved(root, command, hash, actor, pending),
    )
}

fn validate_reserved(
    root: &Path,
    command: &BusinessCommand,
    hash: &str,
    actor: &str,
    pending: &Pending,
) -> Result<()> {
    let id = command
        .id
        .as_deref()
        .context("account operation id is missing")?;
    {
        let conn = store::open_store(root)?;
        ensure!(
            target(&conn, actor, &pending.target.holder, &pending.request)? == pending.target,
            "account authority or target changed before the holder effect"
        );
    }
    let core = crate::channels::business_command_projection(root, id)?;
    ensure!(
        core["payload_hash"] == hash
            && core["execution_mode"] == "control"
            && core["terminal_status"].as_str().unwrap_or("none") == "none"
            && core["execution_phase"] != "terminal",
        "account Core claim changed before the holder effect"
    );
    Ok(())
}

pub(in crate::business_os) fn identity(
    conn: &Connection,
    id: &str,
) -> Result<Option<super::super::domain_effect::DomainEffectIdentity>> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name=?1)",
        ["business_provider_account_controls"],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(None);
    }
    Ok(conn.query_row("SELECT payload_hash,actor_user_id FROM business_provider_account_controls WHERE command_id=?1",[id],|r| Ok(super::super::domain_effect::DomainEffectIdentity{payload_hash:r.get(0)?,actor_user_id:r.get(1)?})).optional()?)
}
pub(in crate::business_os) fn recover(
    root: &Path,
    command: &BusinessCommand,
    hash: &str,
    actor: &str,
) -> Result<Option<Value>> {
    if !supports(&command.command_type) {
        return Ok(None);
    }
    let id = command
        .id
        .as_deref()
        .context("account operation id is missing")?;
    let pending = {
        let conn = store::open_store(root)?;
        if let Some(applied) = super::super::domain_effect::load(&conn, id, hash, actor)? {
            return Ok(Some(applied.result));
        }
        let row:Option<(String,String,String)>=conn.query_row("SELECT payload_hash,actor_user_id,pending_json FROM business_provider_account_controls WHERE command_id=?1",[id],|r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let Some((stored_hash, stored_actor, raw)) = row else {
            return Ok(None);
        };
        ensure!(
            hash == stored_hash && actor == stored_actor,
            "account control identity mismatch"
        );
        let pending: Pending = serde_json::from_str(&raw)?;
        let current = target(
            &conn,
            actor,
            &store::existing_instance_id(root)?,
            &pending.request,
        )?;
        ensure!(
            current == pending.target,
            "account control target changed; reconciliation required"
        );
        pending
    };
    let canonical = crate::channels::business_command_projection(root, id)?;
    ensure!(
        canonical["payload_hash"] == hash
            && canonical["command_type"] == command.command_type
            && canonical["execution_mode"] == "control"
            && canonical["terminal_status"].as_str().unwrap_or("none") == "none",
        "account effect does not match current Core intent"
    );
    // Recovery may finish already proven external effects; never execute a NEW
    // topology mutation under an uncertain claim or inferred authority.
    let effect = holder::finish(root, id, hash, actor)?;
    ensure!(
        effect.local_id == pending.target.local
            && effect.before_binding == pending.target.binding
            && effect.enabled == pending.request.enabled,
        "holder effect does not match reserved account"
    );
    let mut conn = store::open_store(root)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if let Some(applied) = super::super::domain_effect::load(&tx, id, hash, actor)? {
        return Ok(Some(applied.result));
    }
    ensure!(
        target(&tx, actor, &pending.target.holder, &pending.request)? == pending.target,
        "account control target changed; reconciliation required"
    );
    if let Some(enabled) = effect.enabled {
        tx.execute("UPDATE business_provider_federation_accounts SET enabled=?2,credential_ready=?2,revision=revision+1,observed_at_ms=?3 WHERE account_id=?1",params![pending.request.account_id,enabled,store::now_ms() as i64])?;
        set_native_binding(
            &tx,
            &pending.request.account_id,
            effect.after_binding.as_deref(),
        )?;
    } else {
        for table in [
            "business_provider_federation_withdrawals",
            "business_provider_federation_model_observations",
            "business_provider_federation_model_exclusions",
            "business_provider_federation_native_bindings",
            "business_provider_federation_accounts",
        ] {
            tx.execute(
                &format!("DELETE FROM {table} WHERE account_id=?1"),
                [&pending.request.account_id],
            )?;
        }
    }
    if effect.enabled.is_none() {
        tx.execute("DELETE FROM business_provider_federation_models WHERE owner_user_id=?1 AND provider=?2 AND NOT EXISTS(SELECT 1 FROM business_provider_federation_accounts WHERE owner_user_id=?1 AND provider=?2)", params![pending.target.owner, "claude"])?;
    }
    bump_policy(&tx, &pending.target.owner)?;
    tx.execute(
        "UPDATE business_provider_account_controls SET completed=1 WHERE command_id=?1",
        [id],
    )?;
    let applied = projection::applied(&tx, &pending.target.owner)?;
    tx.execute("INSERT INTO business_command_domain_effects(command_id,payload_hash,actor_user_id,receipt_json) VALUES (?1,?2,?3,?4)",params![id,hash,actor,serde_json::to_string(&applied)?])?;

    tx.commit()?;
    Ok(Some(applied.result))
}
pub(super) fn project(conn: &Connection, rows: &mut [Value]) -> Result<()> {
    for row in rows {
        let id = row["id"].as_str().context("account id is missing")?;
        let supported:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM business_provider_federation_accounts a JOIN business_provider_federation_native_bindings b ON b.account_id=a.account_id WHERE a.account_id=?1 AND a.provider='claude' AND a.private_local_account_id<>?2 AND NOT EXISTS(SELECT 1 FROM business_provider_account_controls p WHERE p.owner_user_id=a.owner_user_id AND p.completed=0))",params![id,INHERITED_NATIVE_ACCOUNT_ID],|r| r.get(0))?;
        row["controls"] = json!({"canEnable":supported,"canRemove":supported});
    }
    Ok(())
}
