// Origin: CTOX
// License: AGPL-3.0-only
//! Native Grok Build model checks, pinned to the existing encrypted OAuth record.
use super::{
    cliproxyapi_integration::{ProviderIntegrationConfig, PROVIDER_INTEGRATION_CONFIG_TABLE},
    cliproxyapi_xai as xai,
};
use crate::business_os::{
    consumer_authority::{AdmittedConsumerAuthority, ConsumerFacts},
    provider_federation::{capture_consumable_model, ConsumableModel},
};
use anyhow::{ensure, Context, Result};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};
pub(crate) struct NativeXaiModelReservation {
    root: PathBuf,
    pub(super) selected: ConsumableModel,
    encrypted: String,
    active: Mutex<bool>,
}
pub(crate) struct NativeXaiPublicationCheck {
    encrypted: String,
    runtime: Mutex<Connection>,
    secrets: Mutex<Connection>,
}
fn no_override(conn: &Connection) -> Result<()> {
    let table: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [PROVIDER_INTEGRATION_CONFIG_TABLE],
        |r| r.get(0),
    )?;
    let raw: Option<String> = if table {
        conn.query_row(
            &format!(
                "SELECT config_json FROM {PROVIDER_INTEGRATION_CONFIG_TABLE} WHERE config_id=1"
            ),
            [],
            |r| r.get(0),
        )
        .optional()?
    } else {
        None
    };
    let cfg: ProviderIntegrationConfig = match raw {
        Some(raw) => serde_json::from_str(&raw)
            .map_err(|_| anyhow::anyhow!("native Grok configuration unavailable"))?,
        None => ProviderIntegrationConfig::default(),
    };
    ensure!(
        !cfg.api_key_accounts.iter().any(|a| a.id == xai::ACCOUNT_ID),
        "native Grok OAuth account overridden"
    );
    Ok(())
}
fn encrypted(conn: &Connection) -> Result<String> {
    let row:Option<(String,String)>=conn.query_row("SELECT nonce_b64,ciphertext_b64 FROM ctox_secret_records WHERE scope=?1 AND secret_name=?2",rusqlite::params![xai::SCOPE,xai::NAME],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let (nonce, ciphertext) = row.context("native Grok credential unavailable")?;
    let mut h = Sha256::new();
    h.update(b"ctox/native-xai-model-record/v1");
    for value in [nonce, ciphertext] {
        h.update((value.len() as u64).to_be_bytes());
        h.update(value.as_bytes());
    }
    Ok(format!("{:x}", h.finalize()))
}
impl NativeXaiPublicationCheck {
    fn open(root: &Path) -> Result<Self> {
        let path = crate::inference::runtime_env::runtime_config_path(root);
        if !path.exists() {
            drop(super::cliproxyapi_integration::open_integration_config_db(
                root,
            )?);
        }
        let runtime = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        runtime.busy_timeout(std::time::Duration::ZERO)?;
        no_override(&runtime)?;
        let secrets = Connection::open_with_flags(
            crate::secrets::resolve_db_path(root),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        secrets.busy_timeout(std::time::Duration::ZERO)?;
        let encrypted = encrypted(&secrets)?;
        Ok(Self {
            encrypted,
            runtime: Mutex::new(runtime),
            secrets: Mutex::new(secrets),
        })
    }
    pub(crate) fn prepare(reservation: &NativeXaiModelReservation) -> Result<Self> {
        let check = Self::open(&reservation.root)?;
        ensure!(
            check.encrypted == reservation.encrypted,
            "native Grok credential changed"
        );
        Ok(check)
    }
    fn records<T>(&self, publish: impl FnOnce() -> Result<T>) -> Result<T> {
        let runtime = self
            .runtime
            .try_lock()
            .map_err(|_| anyhow::anyhow!("native Grok configuration busy"))?;
        let runtime = Transaction::new_unchecked(&runtime, TransactionBehavior::Immediate)?;
        no_override(&runtime)?;
        let secrets = self
            .secrets
            .try_lock()
            .map_err(|_| anyhow::anyhow!("native Grok credential metadata busy"))?;
        // The admitted issuer already reserves this encrypted store. Reuse
        // that fence; another IMMEDIATE transaction here would reject every
        // real consumer publication as SQLITE_BUSY.
        ensure!(
            encrypted(&secrets)? == self.encrypted,
            "native Grok credential changed"
        );
        publish()
    }
    pub(crate) fn with_current_in_held_policy<T>(
        &self,
        reservation: &NativeXaiModelReservation,
        facts: &ConsumerFacts,
        policy: &Connection,
        publish: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let mut active = reservation
            .active
            .try_lock()
            .map_err(|_| anyhow::anyhow!("native Grok reservation busy"))?;
        let result = (|| {
            ensure!(*active, "native Grok reservation released");
            ensure!(
                self.encrypted == reservation.encrypted,
                "native Grok prepared account changed"
            );
            reservation
                .selected
                .assert_current_in_policy(facts, policy)?;
            self.records(publish)
        })();
        if result.is_err() {
            *active = false;
        }
        result
    }
}
impl NativeXaiModelReservation {
    pub(crate) fn prepare_model_check(
        authority: &AdmittedConsumerAuthority,
        id: &str,
        revision: i64,
        model: &str,
    ) -> Result<Self> {
        let selected = capture_consumable_model(authority, id, revision, model)?;
        ensure!(
            selected.account().provider == "xai"
                && selected.account().private_local_account_id == xai::ACCOUNT_ID
                && selected.account().holder_instance_id == authority.native_host_instance_id()?,
            "model check requires Grok OAuth held on this native instance"
        );
        let binding = selected
            .private_configuration_binding()
            .context("native Grok account requires current holder discovery")?;
        ensure!(
            binding.starts_with("xai-oauth:v1:"),
            "native Grok OAuth binding unavailable"
        );
        let root = authority.native_host_root().to_owned();
        let current = || xai::model_binding(&root, xai::ACCOUNT_ID);
        ensure!(
            current()?.as_deref() == Some(binding),
            "native Grok binding changed"
        );
        let check = NativeXaiPublicationCheck::open(&root)?;
        ensure!(
            current()?.as_deref() == Some(binding),
            "native Grok binding changed during preparation"
        );
        let reservation = Self {
            root,
            selected,
            encrypted: check.encrypted,
            active: Mutex::new(true),
        };
        reservation.with_current(authority, || Ok(()))?;
        Ok(reservation)
    }
    pub(crate) fn with_current<T>(
        &self,
        authority: &AdmittedConsumerAuthority,
        publish: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let result = (|| {
            ensure!(
                self.root == authority.native_host_root(),
                "native Grok source host changed"
            );
            let check = NativeXaiPublicationCheck::prepare(self)?;
            authority.with_current(|facts, policy| {
                check.with_current_in_held_policy(self, facts, policy, publish)
            })
        })();
        if result.is_err() {
            self.release();
        }
        result
    }
    pub(crate) fn release(&self) {
        *self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = false;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Result<(tempfile::TempDir, String)> {
        let root = tempfile::tempdir()?;
        let raw=serde_json::json!({"access":"fixture-access","refresh":"fixture-refresh","identity":null,"expires_at":null,"token_endpoint":"https://auth.x.ai/token"}).to_string();
        crate::secrets::write_secret_record(
            root.path(),
            xai::SCOPE,
            xai::NAME,
            &raw,
            None,
            serde_json::json!({}),
        )?;
        Ok((root, raw))
    }
    #[test]
    fn xai_model_credential_writes_are_excluded_during_publication() -> Result<()> {
        let (root, _) = fixture()?;
        let check = NativeXaiPublicationCheck::open(root.path())?;
        let writer = Connection::open(crate::secrets::resolve_db_path(root.path()))?;
        writer.busy_timeout(std::time::Duration::ZERO)?;
        let sql="UPDATE ctox_secret_records SET nonce_b64=nonce_b64||'x' WHERE scope=?1 AND secret_name=?2";
        // Same concrete encrypted-store fence as the actual capability signer.
        // Connections were prepared outside it; metadata reads must not reenter.
        crate::secrets::with_current_secret_value(root.path(), xai::SCOPE, xai::NAME, |_| {
            check.records(|| {
                assert!(writer
                    .execute(sql, rusqlite::params![xai::SCOPE, xai::NAME])
                    .is_err());
                Ok(())
            })
        })?;
        assert_eq!(
            writer.execute(sql, rusqlite::params![xai::SCOPE, xai::NAME])?,
            1
        );
        assert!(check.records(|| Ok(())).is_err());
        Ok(())
    }
    #[test]
    fn xai_model_api_shadow_configuration_retires_oauth_publication() -> Result<()> {
        let (root, _) = fixture()?;
        let check = NativeXaiPublicationCheck::open(root.path())?;
        let mut cfg = ProviderIntegrationConfig::default();
        cfg.api_key_accounts
            .push(super::super::cliproxyapi_integration::ApiKeyAccountConfig {
                id: xai::ACCOUNT_ID.into(),
                endpoint: super::super::cliproxyapi_integration::ApiKeyEndpoint::Xai,
                api_key_secret: super::super::cliproxyapi_integration::ProviderSecretRef {
                    scope: "provider-api-keys".into(),
                    name: "fixture-shadow".into(),
                },
                owner_user_id: None,
                creation_operation: None,
                disabled: false,
                priority: 0,
                models: vec![],
            });
        // Corrupt/legacy topology must fail closed even though the typed writer
        // already prohibits this reserved ID for newly created API accounts.
        let writer = Connection::open(crate::inference::runtime_env::runtime_config_path(
            root.path(),
        ))?;
        writer.execute(&format!("INSERT INTO {PROVIDER_INTEGRATION_CONFIG_TABLE}(config_id,revision,config_json,updated_at_ms) VALUES(1,1,?1,0)"),[serde_json::to_string(&cfg)?])?;
        let mut called = false;
        assert!(check
            .records(|| {
                called = true;
                Ok(())
            })
            .is_err());
        assert!(!called);
        assert!(xai::model_binding(root.path(), xai::ACCOUNT_ID).is_err());
        Ok(())
    }
    #[test]
    fn xai_model_snapshot_replacement_and_removal_retire_publication() -> Result<()> {
        for remove in [false, true] {
            let (root, raw) = fixture()?;
            let check = NativeXaiPublicationCheck::open(root.path())?;
            check.records(|| Ok(()))?;
            if remove {
                crate::secrets::delete_secret_record(root.path(), xai::SCOPE, xai::NAME)?;
            } else {
                crate::secrets::write_secret_record(
                    root.path(),
                    xai::SCOPE,
                    xai::NAME,
                    &raw,
                    None,
                    serde_json::json!({}),
                )?;
            }
            let mut called = false;
            assert!(check
                .records(|| {
                    called = true;
                    Ok(())
                })
                .is_err());
            assert!(!called);
        }
        Ok(())
    }
    #[test]
    fn xai_model_runtime_topology_is_excluded_during_publication() -> Result<()> {
        let (root, _) = fixture()?;
        let check = NativeXaiPublicationCheck::open(root.path())?;
        let writer = Connection::open(crate::inference::runtime_env::runtime_config_path(
            root.path(),
        ))?;
        writer.busy_timeout(std::time::Duration::ZERO)?;
        let sql=format!("INSERT INTO {PROVIDER_INTEGRATION_CONFIG_TABLE}(config_id,revision,config_json,updated_at_ms) VALUES(1,1,'{{}}',0)");
        check.records(|| {
            assert!(writer.execute(&sql, []).is_err());
            Ok(())
        })?;
        assert_eq!(writer.execute(&sql, [])?, 1);
        Ok(())
    }
}
