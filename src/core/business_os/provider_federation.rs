// Origin: CTOX
// License: AGPL-3.0-only

//! Authoritative account existence and consumer withdrawals. Credentials and
//! holder-local selectors never leave the native policy store. This increment
//! adopts native account metadata; remote adoption, live catalog observations,
//! replicated UI projections and holder execution remain separate adapters.

use super::{
    consumer_authority::{AdmittedConsumerAuthority, ConsumerFacts},
    domain_effect::{AppliedDomainEffect, DomainEffectAdmission},
    store::{self, BusinessCommand},
    workjet_identity,
};
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;

pub(super) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS business_provider_federation_policy (
    owner_user_id TEXT PRIMARY KEY,
    revision INTEGER NOT NULL CHECK(revision > 0)
);
CREATE TABLE IF NOT EXISTS business_provider_federation_accounts (
    account_id TEXT PRIMARY KEY,
    owner_user_id TEXT NOT NULL,
    holder_instance_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    private_local_account_id TEXT NOT NULL,
    enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
    credential_ready INTEGER NOT NULL CHECK(credential_ready IN (0,1)),
    revision INTEGER NOT NULL CHECK(revision > 0),
    observed_at_ms INTEGER NOT NULL,
    UNIQUE(holder_instance_id,provider,private_local_account_id)
);
CREATE INDEX IF NOT EXISTS business_provider_federation_owner
    ON business_provider_federation_accounts(owner_user_id);
CREATE TABLE IF NOT EXISTS business_provider_federation_withdrawals (
    account_id TEXT NOT NULL,
    computer_id TEXT NOT NULL,
    PRIMARY KEY(account_id,computer_id),
    FOREIGN KEY(account_id) REFERENCES business_provider_federation_accounts(account_id)
);
CREATE TABLE IF NOT EXISTS business_provider_federation_model_observations (
    account_id TEXT PRIMARY KEY,
    account_revision INTEGER NOT NULL,
    last_success_at_ms INTEGER,
    models_json TEXT,
    last_attempt_json TEXT NOT NULL,
    FOREIGN KEY(account_id) REFERENCES business_provider_federation_accounts(account_id)
);
CREATE TABLE IF NOT EXISTS business_provider_federation_native_bindings (
    account_id TEXT PRIMARY KEY,
    fingerprint TEXT NOT NULL,
    FOREIGN KEY(account_id) REFERENCES business_provider_federation_accounts(account_id)
);
CREATE TABLE IF NOT EXISTS business_provider_federation_models (
    owner_user_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    models_json TEXT NOT NULL,
    PRIMARY KEY(owner_user_id,provider)
);
CREATE TABLE IF NOT EXISTS business_provider_federation_model_exclusions (
    account_id TEXT PRIMARY KEY,
    models_json TEXT NOT NULL,
    FOREIGN KEY(account_id) REFERENCES business_provider_federation_accounts(account_id)
);";

const MAX_ACCOUNTS: usize = 256;
const MAX_ID_BYTES: usize = 256;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyRequest {
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WithdrawRequest {
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
    account_id: String,
    computer_id: String,
    withdrawn: bool,
    expected_revision: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ObserveNativeRequest {
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
    account_id: String,
    expected_account_revision: i64,
}

/// Only native account metadata constructs this type; this is not live health.
struct NativeAccountObservation {
    provider: String,
    local_account_id: String,
    enabled: bool,
    credential_ready: bool,
    private_binding: Option<String>,
}

fn bounded_id(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= MAX_ID_BYTES
            && value.trim() == value
            && !value.chars().any(char::is_control),
        "invalid provider federation identity"
    );
    Ok(())
}

fn observations(projection: &Value) -> Result<Vec<NativeAccountObservation>> {
    let accounts = projection["accounts"]
        .as_array()
        .context("native account metadata is unavailable")?;
    ensure!(
        accounts.len() <= MAX_ACCOUNTS,
        "native account limit exceeded"
    );
    let mut seen = std::collections::BTreeSet::new();
    accounts
        .iter()
        .map(|account| {
            let provider = account["provider"]
                .as_str()
                .context("native provider is missing")?;
            let local_account_id = account["id"]
                .as_str()
                .context("native account identity is missing")?;
            bounded_id(provider)?;
            bounded_id(local_account_id)?;
            ensure!(
                seen.insert((provider, local_account_id)),
                "duplicate native account identity"
            );
            let phase = account["status"].as_str().unwrap_or("unknown");
            Ok(NativeAccountObservation {
                provider: provider.into(),
                local_account_id: local_account_id.into(),
                enabled: account["enabled"].as_bool().unwrap_or(false),
                // The source's ready status means credential configured, never
                // provider health, available quota or successful inference.
                credential_ready: matches!(phase, "ready" | "connected"),
                private_binding: None,
            })
        })
        .collect()
}

/// The command plane supplies a currently authorized session actor. Validate
/// the current actor again in the domain transaction, then resolve only a
/// verified managed alias. No owner field can be supplied in the payload.
fn management_owner(conn: &Connection, actor: &str) -> Result<String> {
    let allowed: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM business_users
         WHERE user_id=?1 AND active=1 AND role IN ('chef','admin'))",
        [actor],
        |row| row.get(0),
    )?;
    ensure!(
        allowed,
        "provider federation management requires current Owner/Admin authority"
    );
    workjet_identity::owner_from_connection(conn, actor)
}

pub(super) fn handle_command(
    root: &Path,
    command: &BusinessCommand,
    actor: &str,
    admission: Option<&DomainEffectAdmission>,
) -> Result<Value> {
    ensure!(
        command.record_id.is_none(),
        "provider commands do not accept record_id"
    );
    match command.command_type.as_str() {
        "ctox.workjet.providers.list" => {
            let _: EmptyRequest = serde_json::from_value(command.payload.clone())?;
            let mut conn = store::open_store(root)?;
            let snapshot = conn.transaction()?;
            let owner = management_owner(&snapshot, actor)?;
            list(&snapshot, &owner)
        }
        "ctox.workjet.providers.adopt_native" => {
            let _: EmptyRequest = serde_json::from_value(command.payload.clone())?;
            let admitted =
                admission.context("native account adoption requires domain admission")?;
            let holder = store::existing_instance_id(root)?;
            bounded_id(&holder)?;
            // Resolve existing account metadata before taking the policy writer
            // transaction. No network call, auth refresh or credential mutation.
            let snapshot = store::provider_subscription_status_for_control_plane(root);
            let mut accounts = observations(&snapshot["provider_subscriptions"])?;
            let inherited =
                crate::coding_agents::pi_sidecar::inherited_coding_account_metadata(root);
            if let Ok(Some(metadata)) = &inherited {
                accounts.push(NativeAccountObservation {
                    provider: metadata.provider.clone(),
                    local_account_id: INHERITED_NATIVE_ACCOUNT_ID.into(),
                    enabled: true,
                    credential_ready: true,
                    private_binding: Some(metadata.private_binding.clone()),
                });
            }
            ensure!(
                accounts.len() <= MAX_ACCOUNTS,
                "native account limit exceeded"
            );
            let mut conn = store::open_store(root)?;
            let applied = admitted.apply(&mut conn, |tx| {
                let owner = management_owner(tx, actor)?;
                let now = store::now_ms() as i64;
                // Unsupported/unreadable main routes do not erase the last
                // observation or prevent independent subscription adoption.
                if let Ok(current) = &inherited {
                    retire_inherited_route(
                        tx,
                        &owner,
                        &holder,
                        current.as_ref().map(|metadata| metadata.provider.as_str()),
                        now,
                    )?;
                }
                adopt(tx, &owner, &holder, &accounts, now)?;
                projection::applied(tx, &owner)
            })?;
            Ok(applied.result)
        }
        "ctox.workjet.providers.observe_native" => {
            let request: ObserveNativeRequest = serde_json::from_value(command.payload.clone())?;
            bounded_id(&request.account_id)?;
            ensure!(
                request.expected_account_revision > 0,
                "provider account revision is missing"
            );
            let admitted =
                admission.context("native model observation requires domain admission")?;
            let holder = store::existing_instance_id(root)?;
            let provider = {
                let conn = store::open_store(root)?;
                let owner = management_owner(&conn, actor)?;
                native_catalog_target(&conn, &owner, &holder, &request)?
            };
            // Network wait is outside the policy transaction. Caller-supplied
            // endpoints, credentials and model lists are never accepted.
            let observation =
                crate::coding_agents::pi_sidecar::inherited_coding_model_catalog(root)?;
            ensure!(
                observation.provider == provider,
                "native account provider changed"
            );
            let mut conn = store::open_store(root)?;
            let applied = admitted.apply(&mut conn, |tx| {
                let owner = management_owner(tx, actor)?;
                ensure!(
                    observation.private_binding.is_some()
                        && native_binding(tx, &request.account_id)? == observation.private_binding,
                    "native account configuration changed; refresh account metadata"
                );
                let current = native_catalog_target(tx, &owner, &holder, &request)?;
                ensure!(
                    current == observation.provider,
                    "native account provider changed"
                );
                retain_catalog_observation(tx, &request, &observation)?;
                models::initialize_inherited_selection(
                    tx,
                    &owner,
                    &current,
                    observation.inherited_selected_model.as_deref(),
                    observation.checked_at_ms,
                )?;
                projection::applied(tx, &owner)
            })?;
            Ok(applied.result)
        }
        "ctox.workjet.providers.models.select" | "ctox.workjet.providers.models.exclude" => {
            models::handle_command(root, command, actor, admission)
        }
        "ctox.workjet.providers.withdraw" => {
            let request: WithdrawRequest = serde_json::from_value(command.payload.clone())?;
            bounded_id(&request.account_id)?;
            bounded_id(&request.computer_id)?;
            ensure!(
                request.expected_revision > 0,
                "provider policy revision is missing"
            );
            let admitted = admission.context("account withdrawal requires domain admission")?;
            let mut conn = store::open_store(root)?;
            let applied = admitted.apply(&mut conn, |tx| {
                let owner = management_owner(tx, actor)?;
                withdraw(tx, &owner, &request)?;
                projection::applied(tx, &owner)
            })?;
            Ok(applied.result)
        }
        _ => anyhow::bail!("unsupported provider federation command"),
    }
}

const INHERITED_NATIVE_ACCOUNT_ID: &str = "@ctox/native-main-route";

// A provider switch or an explicitly absent credential retires consumption
// metadata, preserving UUIDs, withdrawals, secrets and provider configuration.
fn retire_inherited_route(
    conn: &Connection,
    owner: &str,
    holder: &str,
    current_provider: Option<&str>,
    now: i64,
) -> Result<()> {
    let rows = {
        let mut stmt = conn.prepare(
            "SELECT account_id,provider,revision FROM business_provider_federation_accounts
             WHERE owner_user_id=?1 AND holder_instance_id=?2 AND private_local_account_id=?3
             AND (enabled=1 OR credential_ready=1)",
        )?;
        let collected = stmt
            .query_map(params![owner, holder, INHERITED_NATIVE_ACCOUNT_ID], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        collected
    };
    let mut changed = false;
    for (id, provider, revision) in rows {
        if current_provider == Some(provider.as_str()) {
            continue;
        }
        let revision = revision
            .checked_add(1)
            .context("account revision exhausted")?;
        conn.execute(
            "UPDATE business_provider_federation_accounts
             SET enabled=0,credential_ready=0,revision=?2,observed_at_ms=?3 WHERE account_id=?1",
            params![id, revision, now],
        )?;
        changed = true;
    }
    if changed {
        bump_policy(conn, owner)?;
    }
    Ok(())
}

fn policy_revision(conn: &Connection, owner: &str) -> Result<i64> {
    Ok(conn
        .query_row(
            "SELECT revision FROM business_provider_federation_policy WHERE owner_user_id=?1",
            [owner],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(0))
}

fn bump_policy(conn: &Connection, owner: &str) -> Result<()> {
    let revision = policy_revision(conn, owner)?
        .checked_add(1)
        .context("provider policy revision exhausted")?;
    conn.execute(
        "INSERT INTO business_provider_federation_policy(owner_user_id,revision) VALUES (?1,?2)
         ON CONFLICT(owner_user_id) DO UPDATE SET revision=excluded.revision",
        params![owner, revision],
    )?;
    Ok(())
}

fn adopt(
    conn: &Connection,
    owner: &str,
    holder: &str,
    accounts: &[NativeAccountObservation],
    now: i64,
) -> Result<()> {
    // This table belongs to this instance; an authenticated Admin must not
    // reassign an already adopted holder/account to another logical owner.
    let mut changed = false;
    for observation in accounts {
        let existing: Option<(String, String, bool, bool, i64)> = conn
            .query_row(
                "SELECT account_id,owner_user_id,enabled,credential_ready,revision
             FROM business_provider_federation_accounts
             WHERE holder_instance_id=?1 AND provider=?2 AND private_local_account_id=?3",
                params![holder, observation.provider, observation.local_account_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?;
        if let Some((id, previous_owner, enabled, configured, revision)) = existing {
            ensure!(
                previous_owner == owner,
                "native account already belongs to another owner"
            );
            let previous_binding = native_binding(conn, &id)?;
            if enabled != observation.enabled
                || configured != observation.credential_ready
                || previous_binding != observation.private_binding
            {
                let revision = revision
                    .checked_add(1)
                    .context("account revision exhausted")?;
                conn.execute(
                    "UPDATE business_provider_federation_accounts
                     SET enabled=?2,credential_ready=?3,revision=?4,observed_at_ms=?5
                     WHERE account_id=?1",
                    params![
                        id,
                        observation.enabled,
                        observation.credential_ready,
                        revision,
                        now
                    ],
                )?;
                changed = true;
            } else {
                conn.execute(
                    "UPDATE business_provider_federation_accounts SET observed_at_ms=?2 WHERE account_id=?1",
                    params![id, now],
                )?;
            }
            set_native_binding(conn, &id, observation.private_binding.as_deref())?;
        } else {
            let count: i64 = conn.query_row(
                "SELECT count(*) FROM business_provider_federation_accounts WHERE owner_user_id=?1",
                [owner],
                |row| row.get(0),
            )?;
            ensure!(count < MAX_ACCOUNTS as i64, "owner account limit exceeded");
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO business_provider_federation_accounts
                 (account_id,owner_user_id,holder_instance_id,provider,private_local_account_id,
                  enabled,credential_ready,revision,observed_at_ms)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,1,?8)",
                params![
                    id,
                    owner,
                    holder,
                    observation.provider,
                    observation.local_account_id,
                    observation.enabled,
                    observation.credential_ready,
                    now
                ],
            )?;
            set_native_binding(conn, &id, observation.private_binding.as_deref())?;
            changed = true;
        }
    }
    // Missing accounts/holders retain their identity and withdrawals. Only an
    // explicit holder deletion receipt may remove them in the execution adapter.
    if changed {
        bump_policy(conn, owner)?;
    }
    Ok(())
}

fn require_enrolled_computer(conn: &Connection, owner: &str, computer: &str) -> Result<()> {
    let raw: String = conn
        .query_row(
            "SELECT payload_json FROM business_records
         WHERE collection='workjet_computers' AND record_id=?1 AND deleted=0
           AND json_extract(payload_json,'$.owner_user_id')=?2
           AND json_extract(payload_json,'$.status')='assigned'
           AND coalesce(json_extract(payload_json,'$.is_deleted'),0)=0
           AND coalesce(json_extract(payload_json,'$._deleted'),0)=0
           AND coalesce(json_extract(payload_json,'$.agentless'),0)=0
           AND json_extract(payload_json,'$.hosting_mode') IN ('workstation','self_hosted')",
            params![computer, owner],
            |row| row.get(0),
        )
        .context("withdrawal target is not an enrolled computer")?;
    let record: Value = serde_json::from_str(&raw)?;
    let pairing = record["device_binding_id"]
        .as_str()
        .context("computer has no paired device")?;
    let current = super::consumer_authority::validate_owner_binding(conn, owner, pairing)?;
    ensure!(
        record["native_device_binding"] == current,
        "computer enrollment changed"
    );
    Ok(())
}

fn withdraw(conn: &Connection, owner: &str, request: &WithdrawRequest) -> Result<()> {
    ensure!(
        policy_revision(conn, owner)? == request.expected_revision,
        "provider policy revision conflict"
    );
    let account_owned: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM business_provider_federation_accounts
         WHERE account_id=?1 AND owner_user_id=?2)",
        params![request.account_id, owner],
        |row| row.get(0),
    )?;
    ensure!(account_owned, "provider account is not owned");
    require_enrolled_computer(conn, owner, &request.computer_id)?;
    let changed = if request.withdrawn {
        conn.execute(
            "INSERT OR IGNORE INTO business_provider_federation_withdrawals(account_id,computer_id) VALUES (?1,?2)",
            params![request.account_id, request.computer_id],
        )?
    } else {
        conn.execute(
            "DELETE FROM business_provider_federation_withdrawals WHERE account_id=?1 AND computer_id=?2",
            params![request.account_id, request.computer_id],
        )?
    };
    if changed != 0 {
        bump_policy(conn, owner)?;
    }
    Ok(())
}

const CATALOG_FRESHNESS_MS: i64 = 86_400_000;

fn native_binding(conn: &Connection, id: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT fingerprint FROM business_provider_federation_native_bindings WHERE account_id=?1",
        [id],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn set_native_binding(conn: &Connection, id: &str, binding: Option<&str>) -> Result<()> {
    if let Some(binding) = binding {
        conn.execute("INSERT INTO business_provider_federation_native_bindings(account_id,fingerprint) VALUES (?1,?2) ON CONFLICT(account_id) DO UPDATE SET fingerprint=excluded.fingerprint", params![id,binding])?;
    } else {
        conn.execute(
            "DELETE FROM business_provider_federation_native_bindings WHERE account_id=?1",
            [id],
        )?;
    }
    Ok(())
}

fn native_catalog_target(
    conn: &Connection,
    owner: &str,
    holder: &str,
    request: &ObserveNativeRequest,
) -> Result<String> {
    conn.query_row(
        "SELECT provider FROM business_provider_federation_accounts
         WHERE account_id=?1 AND owner_user_id=?2 AND holder_instance_id=?3
           AND private_local_account_id=?4 AND revision=?5 AND enabled=1 AND credential_ready=1",
        params![
            request.account_id,
            owner,
            holder,
            INHERITED_NATIVE_ACCOUNT_ID,
            request.expected_account_revision
        ],
        |row| row.get(0),
    )
    .context("selected native catalog account is unavailable or changed")
}

fn retain_catalog_observation(
    conn: &Connection,
    request: &ObserveNativeRequest,
    observation: &crate::coding_agents::pi_sidecar::NativeModelCatalogObservation,
) -> Result<()> {
    ensure!(
        observation.checked_at_ms > 0,
        "invalid native observation time"
    );
    let observed = observation.failure.is_none()
        && observation.http_status == Some(200)
        && observation.models.is_some();
    ensure!(
        observation.models.is_none() || observed,
        "invalid native model observation"
    );
    let previous: Option<(Option<i64>,Option<String>)> = conn.query_row(
        "SELECT last_success_at_ms,models_json FROM business_provider_federation_model_observations
         WHERE account_id=?1 AND account_revision=?2",
        params![request.account_id, request.expected_account_revision],
        |row| Ok((row.get(0)?,row.get(1)?)),
    ).optional()?;
    let (success_at, models_json) = if observed {
        (
            Some(observation.checked_at_ms),
            Some(serde_json::to_string(observation.models.as_ref().unwrap())?),
        )
    } else {
        previous.unwrap_or((None, None))
    };
    // Persist only allowlisted metadata. A failed GET never changes account
    // enablement, credentials, cooldown, session affinity or previous live IDs.
    let attempt = json!({
        "checkedAtMs":observation.checked_at_ms,"httpStatus":observation.http_status,
        "elapsedMs":observation.elapsed_ms,"retryAfterSeconds":observation.retry_after_seconds,
        "failure":observation.failure,"success":observed,
    });
    conn.execute(
        "INSERT INTO business_provider_federation_model_observations
         (account_id,account_revision,last_success_at_ms,models_json,last_attempt_json)
         VALUES (?1,?2,?3,?4,?5)
         ON CONFLICT(account_id) DO UPDATE SET account_revision=excluded.account_revision,
         last_success_at_ms=excluded.last_success_at_ms,models_json=excluded.models_json,
         last_attempt_json=excluded.last_attempt_json",
        params![
            request.account_id,
            request.expected_account_revision,
            success_at,
            models_json,
            serde_json::to_string(&attempt)?
        ],
    )?;
    Ok(())
}

fn catalog_projection(
    conn: &Connection,
    account_id: &str,
    revision: i64,
    now: i64,
) -> Result<Value> {
    let row: Option<(Option<i64>, Option<String>, String)> = conn
        .query_row(
            "SELECT last_success_at_ms,models_json,last_attempt_json
         FROM business_provider_federation_model_observations
         WHERE account_id=?1 AND account_revision=?2",
            params![account_id, revision],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((success_at, models, attempt)) = row else {
        return Ok(
            json!({"observed":false,"fresh":false,"models":[],"lastSuccessAtMs":null,"lastAttempt":null}),
        );
    };
    let attempt: Value = serde_json::from_str(&attempt)?;
    let models: Value = models
        .map(|value| serde_json::from_str(&value))
        .transpose()?
        .unwrap_or(json!([]));
    let fresh = success_at
        .is_some_and(|checked| now >= checked && now - checked <= CATALOG_FRESHNESS_MS)
        && attempt["success"] == true;
    Ok(
        json!({"observed":success_at.is_some(),"fresh":fresh,"models":models,
        "lastSuccessAtMs":success_at,"lastAttempt":attempt}),
    )
}

fn list(conn: &Connection, owner: &str) -> Result<Value> {
    let mut stmt = conn.prepare(
        "SELECT account_id,holder_instance_id,provider,enabled,credential_ready,revision,observed_at_ms
         FROM business_provider_federation_accounts WHERE owner_user_id=?1 ORDER BY provider,account_id LIMIT ?2",
    )?;
    let mut rows = stmt
        .query_map(params![owner, MAX_ACCOUNTS as i64 + 1], |row| {
            Ok(json!({
                "id":row.get::<_,String>(0)?,
                "holder":{"kind":"ctox_instance","id":row.get::<_,String>(1)?},
                "provider":row.get::<_,String>(2)?,
                "enabled":row.get::<_,bool>(3)?,
                "credentialReady":row.get::<_,bool>(4)?,
                "revision":row.get::<_,i64>(5)?,
                "observedAtMs":row.get::<_,i64>(6)?,
                "modelCatalogObserved":false,
                "inferenceVerified":false
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(rows.len() <= MAX_ACCOUNTS, "owner account limit exceeded");
    let now = store::now_ms() as i64;
    for entry in &mut rows {
        let id = entry["id"]
            .as_str()
            .context("native account id is missing")?;
        let revision = entry["revision"]
            .as_i64()
            .context("native account revision is missing")?;
        let catalog = catalog_projection(conn, id, revision, now)?;
        entry["modelCatalogObserved"] = catalog["observed"].clone();
        entry["modelCatalog"] = catalog;
    }
    let providers = models::project(conn, owner, &mut rows)?;
    Ok(
        json!({"ok":true,"schema":"ctox.provider-federation-registry.v1",
        "revision":policy_revision(conn, owner)?,"accounts":rows,"providers":providers}),
    )
}

/// Native-only selected-account context. No Debug, Serialize or Deserialize:
/// the private selector is resolved on its holding instance, never replicated.
pub(crate) struct ConsumableAccount {
    pub(crate) account_id: String,
    pub(crate) holder_instance_id: String,
    pub(crate) provider: String,
    pub(crate) private_local_account_id: String,
    pub(crate) account_revision: i64,
    pub(crate) policy_revision: i64,
}

fn consumable(
    conn: &Connection,
    consumer: &ConsumerFacts,
    id: &str,
    revision: i64,
) -> Result<ConsumableAccount> {
    let account: Option<(String,String,String,bool,bool,i64)> = conn.query_row(
        "SELECT holder_instance_id,provider,private_local_account_id,enabled,credential_ready,revision
         FROM business_provider_federation_accounts WHERE account_id=?1 AND owner_user_id=?2",
        params![id,consumer.owner_user_id],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?)),
    ).optional()?;
    let (holder, provider, local, enabled, configured, current) =
        account.context("provider account is unavailable")?;
    ensure!(
        enabled && configured,
        "provider account metadata is not ready"
    );
    ensure!(current == revision, "provider account revision changed");
    let withdrawn: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM business_provider_federation_withdrawals WHERE account_id=?1 AND computer_id=?2)",
        params![id,consumer.computer_id], |row| row.get(0),
    )?;
    ensure!(
        !withdrawn,
        "provider access withdrawn for the actual consumer"
    );
    Ok(ConsumableAccount {
        account_id: id.into(),
        holder_instance_id: holder,
        provider,
        private_local_account_id: local,
        account_revision: current,
        policy_revision: policy_revision(conn, &consumer.owner_user_id)?,
    })
}

/// Eligibility is default-on for every current enrolled consumer. There is no
/// account allowlist or DTO-based consumer constructor. This is a local policy
/// fence only; it is NOT a remotely forwardable grant, live-model authorization,
/// holder reachability or permission to perform network IO inside the callback.
pub(crate) fn with_consumable_account<T>(
    authority: &AdmittedConsumerAuthority,
    account_id: &str,
    expected_account_revision: i64,
    apply: impl FnOnce(&ConsumerFacts, &ConsumableAccount) -> Result<T>,
) -> Result<T> {
    bounded_id(account_id)?;
    authority.with_current(|facts, conn| {
        let selected = consumable(conn, facts, account_id, expected_account_revision)?;
        apply(facts, &selected)
    })
}

#[cfg(test)]
#[path = "provider_federation_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "provider_federation_catalog_tests.rs"]
mod catalog_tests;

#[path = "provider_models.rs"]
mod models;

pub(crate) use models::{capture_consumable_model, with_consumable_model, ConsumableModel};

#[path = "provider_projection.rs"]
mod projection;
pub(super) use projection::{
    repair as repair_projections, visible as projection_visible, COLLECTION as REGISTRY_COLLECTION,
};
