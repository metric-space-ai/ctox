// Origin: CTOX
// License: AGPL-3.0-only

//! Instance-wide provider model selection and per-account exclusions.
//! Live discovery remains distinct from selection, inference and consumer access.

use super::*;
use std::collections::{BTreeMap, BTreeSet};

const MAX_SELECTED_MODELS: usize = 256;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectRequest {
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
    provider: String,
    models: Vec<String>,
    expected_revision: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExcludeRequest {
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
    account_id: String,
    models: Vec<String>,
    expected_account_revision: i64,
    expected_revision: i64,
}

fn model_set(models: &[String]) -> Result<BTreeSet<String>> {
    ensure!(
        models.len() <= MAX_SELECTED_MODELS,
        "too many selected models"
    );
    for model in models {
        bounded_id(model)?;
    }
    Ok(models.iter().cloned().collect())
}

fn selection(conn: &Connection, owner: &str, provider: &str) -> Result<Option<BTreeSet<String>>> {
    let value: Option<String> = conn.query_row(
        "SELECT models_json FROM business_provider_federation_models WHERE owner_user_id=?1 AND provider=?2",
        params![owner, provider], |row| row.get(0),
    ).optional()?;
    value
        .map(|value| {
            let models: Vec<String> = serde_json::from_str(&value)?;
            model_set(&models)
        })
        .transpose()
}

fn exclusions(conn: &Connection, account: &str) -> Result<BTreeSet<String>> {
    let value: Option<String> = conn.query_row(
        "SELECT models_json FROM business_provider_federation_model_exclusions WHERE account_id=?1",
        [account], |row| row.get(0),
    ).optional()?;
    model_set(
        &value
            .map(|value| serde_json::from_str::<Vec<String>>(&value))
            .transpose()?
            .unwrap_or_default(),
    )
}

fn observed_models(catalog: &Value) -> Result<BTreeSet<String>> {
    let models: Vec<String> = serde_json::from_value(catalog["models"].clone())?;
    // Discovery may contain more models than a curated selection. It is
    // already bounded and validated by the live catalog adapter.
    for model in &models {
        bounded_id(model)?;
    }
    Ok(models.into_iter().collect())
}

fn provider_catalog(
    conn: &Connection,
    owner: &str,
    provider: &str,
    now: i64,
) -> Result<(BTreeSet<String>, BTreeSet<String>)> {
    let mut statement = conn.prepare(
        "SELECT account_id,revision,enabled,credential_ready FROM business_provider_federation_accounts
         WHERE owner_user_id=?1 AND provider=?2",
    )?;
    let accounts = statement
        .query_map(params![owner, provider], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, bool>(2)?,
                row.get::<_, bool>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(!accounts.is_empty(), "provider has no owned accounts");
    let mut observed = BTreeSet::new();
    let mut fresh = BTreeSet::new();
    for (id, revision, enabled, configured) in accounts {
        let catalog = catalog_projection(conn, &id, revision, now)?;
        let models = observed_models(&catalog)?;
        observed.extend(models.iter().cloned());
        if enabled && configured && catalog["fresh"] == true {
            fresh.extend(models);
        }
    }
    Ok((observed, fresh))
}

fn select(conn: &Connection, owner: &str, request: &SelectRequest, now: i64) -> Result<()> {
    ensure!(
        policy_revision(conn, owner)? == request.expected_revision,
        "provider policy revision conflict"
    );
    let desired = model_set(&request.models)?;
    let (_observed, fresh) = provider_catalog(conn, owner, &request.provider, now)?;
    let prior = selection(conn, owner, &request.provider)?;
    // Removing existing choices is possible during an outage. Every added
    // choice requires a current real GET/models result on an eligible account.
    let previous = prior.clone().unwrap_or_default();
    ensure!(
        desired
            .difference(&previous)
            .all(|model| fresh.contains(model)),
        "new model is not in a current owned-account live catalog"
    );
    if prior.as_ref() == Some(&desired) {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO business_provider_federation_models(owner_user_id,provider,models_json) VALUES (?1,?2,?3)
         ON CONFLICT(owner_user_id,provider) DO UPDATE SET models_json=excluded.models_json",
        params![owner,request.provider,serde_json::to_string(&desired)?],
    )?;
    bump_policy(conn, owner)?;
    Ok(())
}

fn exclude(conn: &Connection, owner: &str, request: &ExcludeRequest, now: i64) -> Result<()> {
    ensure!(
        policy_revision(conn, owner)? == request.expected_revision,
        "provider policy revision conflict"
    );
    let revision: i64 = conn.query_row(
        "SELECT revision FROM business_provider_federation_accounts WHERE account_id=?1 AND owner_user_id=?2",
        params![request.account_id,owner], |row| row.get(0),
    ).context("provider account is not owned")?;
    ensure!(
        revision == request.expected_account_revision,
        "provider account revision changed"
    );
    let desired = model_set(&request.models)?;
    let prior = exclusions(conn, &request.account_id)?;
    let catalog = catalog_projection(conn, &request.account_id, revision, now)?;
    let observed = observed_models(&catalog)?;
    ensure!(
        desired
            .difference(&prior)
            .all(|model| observed.contains(model)),
        "excluded model has not been observed on this account"
    );
    if desired == prior {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO business_provider_federation_model_exclusions(account_id,models_json) VALUES (?1,?2)
         ON CONFLICT(account_id) DO UPDATE SET models_json=excluded.models_json",
        params![request.account_id,serde_json::to_string(&desired)?],
    )?;
    bump_policy(conn, owner)?;
    Ok(())
}

/// Preserve the existing native main-route choice on first adoption, only
/// after authenticated discovery confirms it. An explicit selection (even
/// empty) always wins; this never fills a provider with every catalog model.
pub(super) fn initialize_inherited_selection(
    conn: &Connection,
    owner: &str,
    provider: &str,
    inherited: Option<&str>,
    now: i64,
) -> Result<()> {
    let Some(model) = inherited else {
        return Ok(());
    };
    if selection(conn, owner, provider)?.is_some() {
        return Ok(());
    }
    let (_, fresh) = provider_catalog(conn, owner, provider, now)?;
    if !fresh.contains(model) {
        return Ok(());
    }
    select(
        conn,
        owner,
        &SelectRequest {
            _inbound_channel: None,
            provider: provider.to_owned(),
            models: vec![model.to_owned()],
            expected_revision: policy_revision(conn, owner)?,
        },
        now,
    )
}

pub(super) fn handle_command(
    root: &Path,
    command: &BusinessCommand,
    actor: &str,
    admission: Option<&DomainEffectAdmission>,
) -> Result<Value> {
    let admitted = admission.context("model selection requires domain admission")?;
    let mut conn = store::open_store(root)?;
    let applied = admitted.apply(&mut conn, |tx| {
        let owner = management_owner(tx, actor)?;
        let now = store::now_ms() as i64;
        match command.command_type.as_str() {
            "ctox.workjet.providers.models.select" => {
                let request: SelectRequest = serde_json::from_value(command.payload.clone())?;
                bounded_id(&request.provider)?;
                select(tx, &owner, &request, now)?;
            }
            "ctox.workjet.providers.models.exclude" => {
                let request: ExcludeRequest = serde_json::from_value(command.payload.clone())?;
                bounded_id(&request.account_id)?;
                exclude(tx, &owner, &request, now)?;
            }
            _ => anyhow::bail!("unsupported provider model command"),
        }
        projection::applied(tx, &owner)
    })?;
    Ok(applied.result)
}

pub(super) fn project(conn: &Connection, owner: &str, accounts: &mut [Value]) -> Result<Value> {
    let mut providers: BTreeMap<String, Option<BTreeSet<String>>> = BTreeMap::new();
    for account in accounts {
        let provider = account["provider"]
            .as_str()
            .context("account provider missing")?
            .to_owned();
        if !providers.contains_key(&provider) {
            providers.insert(provider.clone(), selection(conn, owner, &provider)?);
        }
        let selected = providers.get(&provider).unwrap();
        let id = account["id"].as_str().context("account identity missing")?;
        let excluded = exclusions(conn, id)?;
        let observed = observed_models(&account["modelCatalog"])?;
        let effective: Vec<_> = observed
            .iter()
            .filter(|model| {
                selected
                    .as_ref()
                    .is_some_and(|selection| selection.contains(*model))
                    && !excluded.contains(*model)
            })
            .cloned()
            .collect();
        account["excludedModels"] = json!(excluded);
        account["effectiveModels"] = json!(effective);
    }
    let rows: Vec<_> = providers
        .into_iter()
        .map(|(provider, models)| json!({"provider":provider,"selection":models}))
        .collect();
    Ok(json!(rows))
}

/// Sealed native context for one exact model; it contains the holder-private
/// selector and must never become a renderer/forwarding DTO.
pub(crate) struct ConsumableModel {
    account: ConsumableAccount,
    model: String,
    catalog_checked_at_ms: i64,
    consumer: ConsumerFacts,
    private_configuration_binding: Option<String>,
    catalog_fingerprint: Vec<u8>,
}

fn assert_same_binding(expected: &ConsumableModel, current: &ConsumableModel) -> Result<()> {
    ensure!(
        expected.consumer == current.consumer
            && expected.account.account_id == current.account.account_id
            && expected.account.holder_instance_id == current.account.holder_instance_id
            && expected.account.provider == current.account.provider
            && expected.account.private_local_account_id
                == current.account.private_local_account_id
            && expected.account.account_revision == current.account.account_revision
            && expected.account.policy_revision == current.account.policy_revision
            && expected.model == current.model
            && expected.catalog_checked_at_ms == current.catalog_checked_at_ms
            && expected.private_configuration_binding == current.private_configuration_binding
            && expected.catalog_fingerprint == current.catalog_fingerprint,
        "captured model binding changed"
    );
    Ok(())
}

impl ConsumableModel {
    pub(crate) fn account(&self) -> &ConsumableAccount {
        &self.account
    }
    pub(crate) fn model(&self) -> &str {
        &self.model
    }
    pub(crate) fn catalog_checked_at_ms(&self) -> i64 {
        self.catalog_checked_at_ms
    }

    pub(crate) fn private_configuration_binding(&self) -> Option<&str> {
        self.private_configuration_binding.as_deref()
    }

    /// Revalidate a captured selection around a later bounded dispatch or
    /// publication. An identity, policy, catalog or private binding change
    /// retires it even if the model was removed and subsequently reselected.
    /// No await or network/secret API reentry is permitted inside apply.
    pub(crate) fn with_current<T>(
        &self,
        authority: &AdmittedConsumerAuthority,
        apply: impl FnOnce(&ConsumerFacts, &ConsumableModel) -> Result<T>,
    ) -> Result<T> {
        authority.with_current(|facts, conn| {
            let current = consumable_model(
                conn,
                facts,
                &self.account.account_id,
                self.account.account_revision,
                &self.model,
                store::now_ms() as i64,
            )?;
            assert_same_binding(self, &current)?;
            apply(facts, &current)
        })
    }
}

fn consumable_model(
    conn: &Connection,
    facts: &ConsumerFacts,
    id: &str,
    revision: i64,
    model: &str,
    now: i64,
) -> Result<ConsumableModel> {
    bounded_id(model)?;
    let account = consumable(conn, facts, id, revision)?;
    let catalog = catalog_projection(conn, id, revision, now)?;
    ensure!(
        catalog["fresh"] == true,
        "account live model discovery is stale or unavailable"
    );
    ensure!(
        observed_models(&catalog)?.contains(model),
        "model is not in this account's live catalog"
    );
    ensure!(
        selection(conn, &facts.owner_user_id, &account.provider)?
            .is_some_and(|models| models.contains(model)),
        "model is not selected for this provider"
    );
    ensure!(
        !exclusions(conn, id)?.contains(model),
        "model is excluded on this account"
    );
    Ok(ConsumableModel {
        account,
        model: model.to_owned(),
        catalog_checked_at_ms: catalog["lastSuccessAtMs"]
            .as_i64()
            .context("catalog time missing")?,
        consumer: facts.clone(),
        private_configuration_binding: native_binding(conn, id)?,
        catalog_fingerprint: {
            use sha2::Digest;
            sha2::Sha256::digest(serde_json::to_vec(&catalog)?).to_vec()
        },
    })
}

/// Re-enters the actual original consumer's transport, identity and policy
/// fence. Call around dispatch and publication, never network IO inside apply.
/// This is eligibility, not execution authority or a signed holder grant.
pub(crate) fn with_consumable_model<T>(
    authority: &AdmittedConsumerAuthority,
    account_id: &str,
    expected_account_revision: i64,
    model: &str,
    apply: impl FnOnce(&ConsumerFacts, &ConsumableModel) -> Result<T>,
) -> Result<T> {
    bounded_id(account_id)?;
    authority.with_current(|facts, conn| {
        let selected = consumable_model(
            conn,
            facts,
            account_id,
            expected_account_revision,
            model,
            store::now_ms() as i64,
        )?;
        apply(facts, &selected)
    })
}

/// Capture only from the real admitted transport. This object is not a
/// durable worker execution lease and cannot be reconstructed from wire JSON.
pub(crate) fn capture_consumable_model(
    authority: &AdmittedConsumerAuthority,
    account_id: &str,
    expected_account_revision: i64,
    model: &str,
) -> Result<ConsumableModel> {
    bounded_id(account_id)?;
    authority.with_current(|facts, conn| {
        consumable_model(
            conn,
            facts,
            account_id,
            expected_account_revision,
            model,
            store::now_ms() as i64,
        )
    })
}

#[path = "provider_supervisor_models.rs"]
mod supervisor;
pub(crate) use supervisor::{resolve_supervisor_model, SupervisorModelEligibility};

#[cfg(test)]
#[path = "provider_models_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "provider_model_default_tests.rs"]
mod default_tests;
