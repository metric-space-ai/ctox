// Origin: CTOX
// License: AGPL-3.0-only
//! Holder-private proof of a topology mutation. Runtime COMMIT is the point of
//! no return; replay may finish exact credential cleanup but never repeat it
//! against a newly configured account. No field here is a public projection.
use super::*;
use serde::{Deserialize, Serialize};

#[cfg(test)]
#[path = "cliproxyapi_account_controls_tests.rs"]
mod tests;

#[path = "cliproxyapi_account_control_binding.rs"]
mod binding;

pub(crate) fn account_binding(
    root: &Path,
    provider: &str,
    id: &str,
) -> anyhow::Result<Option<String>> {
    let Some(first) = binding::capture(root, provider, id)? else {
        return Ok(None);
    };
    if binding::capture(root, provider, id)?.as_ref() != Some(&first) {
        return Ok(None);
    }
    first.fingerprint().map(Some)
}

fn legacy_provider() -> String {
    "claude".into()
}

const TABLE: &str = "cliproxyapi_account_control_effects";
#[derive(Serialize, Deserialize)]
pub(crate) struct Effect {
    pub(crate) command_id: String,
    pub(crate) payload_hash: String,
    pub(crate) actor: String,
    pub(crate) local_id: String,
    #[serde(default = "legacy_provider")]
    pub(crate) provider: String,
    pub(crate) enabled: Option<bool>,
    pub(crate) before_binding: String,
    pub(crate) after_binding: Option<String>,
    pub(crate) config_revision: u64,
    keys: Vec<(String, String, String)>,
}
fn schema(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(&format!("CREATE TABLE IF NOT EXISTS {TABLE} (command_id TEXT PRIMARY KEY, effect_json TEXT NOT NULL)"))?;
    Ok(())
}
fn read(conn: &Connection, id: &str) -> anyhow::Result<Option<Effect>> {
    let raw: Option<String> = conn
        .query_row(
            &format!("SELECT effect_json FROM {TABLE} WHERE command_id=?1"),
            [id],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|raw| serde_json::from_str(&raw).context("invalid holder account effect"))
        .transpose()
}
pub(crate) fn has_effect(root: &Path, id: &str) -> anyhow::Result<bool> {
    let Some(conn) = open_instance_proxy_config_db_read_only(root)? else {
        return Ok(false);
    };
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type=\"table\" AND name=?1)",
        [TABLE],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(false);
    }
    Ok(read(&conn, id)?.is_some())
}

/// Compatibility helper for existing Claude control fixtures.
#[cfg(test)]
pub(crate) fn apply(
    root: &Path,
    id: &str,
    hash: &str,
    actor: &str,
    local: &str,
    expected_binding: &str,
    enabled: Option<bool>,
    validate_current_authority: impl FnOnce() -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    apply_account(
        root,
        id,
        hash,
        actor,
        "claude",
        local,
        expected_binding,
        enabled,
        validate_current_authority,
    )
}

/// Provider is resolved from the admitted private account, never caller input.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_account(
    root: &Path,
    id: &str,
    hash: &str,
    actor: &str,
    provider: &str,
    local: &str,
    expected_binding: &str,
    enabled: Option<bool>,
    validate_current_authority: impl FnOnce() -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let _guard = crate::secrets::credential_lifecycle_guard();
    let mut stored =
        load_instance_proxy_config(root)?.context("provider topology is unavailable")?;
    let mut captured = binding::capture(root, provider, local)?
        .context("owned provider credentials are unavailable")?;
    anyhow::ensure!(
        captured.fingerprint()? == expected_binding,
        "native account configuration changed; refresh account metadata"
    );
    captured.check_config(&stored.runtime)?;
    let mut keys = Vec::new();
    if enabled.is_none() {
        let refs = captured.remove(&mut stored.runtime);
        let remaining = runtime_secret_keys(&stored.runtime);
        for key in refs {
            anyhow::ensure!(
                key.scope == INSTANCE_CODEX_SECRET_SCOPE
                    && !remaining.contains(&(key.scope.clone(), key.name.clone())),
                "provider credential reference is shared or unsupported"
            );
            let version =
                crate::secrets::secret_record_content_version(root, &key.scope, &key.name)?
                    .context("provider credential generation is unavailable")?;
            keys.push((key.scope, key.name, version));
        }
    } else {
        captured.set_enabled(&mut stored.runtime, enabled.unwrap());
    }
    anyhow::ensure!(
        account_binding(root, provider, local)?.as_deref() == Some(expected_binding),
        "native credential generation changed; refresh account metadata"
    );
    // Preserve the default even when dormant. Never select another provider.
    validate_persisted_proxy_topology(&stored.runtime)?;
    let revision = stored
        .revision
        .checked_add(1)
        .context("proxy revision exhausted")?;
    let after_binding = if enabled.is_some() {
        Some(captured.fingerprint()?)
    } else {
        None
    };
    let effect = Effect {
        command_id: id.into(),
        payload_hash: hash.into(),
        actor: actor.into(),
        local_id: local.into(),
        provider: provider.into(),
        enabled,
        before_binding: expected_binding.into(),
        after_binding,
        config_revision: revision,
        keys,
    };
    let mut conn = open_instance_proxy_config_db(root)?;
    schema(&conn)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    anyhow::ensure!(
        read(&tx, id)?.is_none(),
        "holder mutation already applied; recovery required"
    );
    let current: u64 = tx.query_row(
        &format!("SELECT revision FROM {INSTANCE_PROXY_CONFIG_TABLE} WHERE config_id=1"),
        [],
        |r| r.get(0),
    )?;
    anyhow::ensure!(
        current == stored.revision,
        "native account configuration changed; refresh account metadata"
    );
    // Recheck the real command/owner at the CAS boundary, after credential IO.
    // This callback must not retain a Policy transaction or call Runtime APIs.
    validate_current_authority()?;
    tx.execute(&format!("UPDATE {INSTANCE_PROXY_CONFIG_TABLE} SET revision=?1,config_json=?2,updated_at_ms=?3 WHERE config_id=1"), params![revision,serde_json::to_string(&stored.runtime)?,chrono::Utc::now().timestamp_millis()])?;
    tx.execute(
        &format!("INSERT INTO {TABLE}(command_id,effect_json) VALUES (?1,?2)"),
        params![id, serde_json::to_string(&effect)?],
    )?;
    tx.commit()?;
    Ok(())
}
/// Read proof and finish exact cleanup only. A prepared-only operation is
/// uncertain, not authorization to perform the original topology mutation.
pub(crate) fn finish(root: &Path, id: &str, hash: &str, actor: &str) -> anyhow::Result<Effect> {
    let _guard = crate::secrets::credential_lifecycle_guard();
    let mut conn = open_instance_proxy_config_db(root)?;
    schema(&conn)?;
    let effect =
        read(&conn, id)?.context("holder account effect is uncertain; reconciliation required")?;
    anyhow::ensure!(
        effect.payload_hash == hash && effect.actor == actor,
        "holder account effect identity mismatch"
    );
    let binding = account_binding(root, &effect.provider, &effect.local_id)?;
    anyhow::ensure!(
        binding == effect.after_binding,
        "native account generation changed after mutation; reconciliation required"
    );
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let raw: (u64, String) = tx.query_row(
        &format!(
            "SELECT revision,config_json FROM {INSTANCE_PROXY_CONFIG_TABLE} WHERE config_id=1"
        ),
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    anyhow::ensure!(
        raw.0 == effect.config_revision,
        "native topology changed after mutation; reconciliation required"
    );
    let runtime: CliproxyRuntimeConfig = serde_json::from_str(&raw.1)?;
    if effect.enabled.is_none() {
        anyhow::ensure!(
            !binding::configured(&runtime, &effect.provider, &effect.local_id),
            "provider account was reconfigured; reconciliation required"
        );
        crate::secrets::delete_secret_records_if_versions(root, &effect.keys)?;
        if effect.provider == "claude" {
            super::account_quotas::remove_under_lifecycle(
                root,
                "claude",
                &effect.local_id,
                &_guard,
            )?;
        }
    }
    tx.commit()?;
    Ok(effect)
}
