// Origin: CTOX
// License: AGPL-3.0-only
//! Exact owned Codex OAuth model-check reservation. Credentials stay in the
//! normal gateway; only account configuration and encrypted generations remain.
use crate::business_os::{
    consumer_authority::{AdmittedConsumerAuthority, ConsumerFacts},
    provider_federation::{capture_consumable_model, ConsumableModel},
};
use anyhow::{ensure, Context, Result};
use ctox_cliproxyapi::internal::config::{CliproxyRuntimeConfig, CodexSubscriptionAccountConfig};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

pub(crate) struct NativeCodexModelReservation {
    root: PathBuf,
    pub(super) selected: ConsumableModel,
    account: CodexSubscriptionAccountConfig,
    encrypted: String,
    active: Mutex<bool>,
}
pub(crate) struct NativeCodexPublicationCheck {
    account: CodexSubscriptionAccountConfig,
    encrypted: String,
    runtime: Mutex<Connection>,
    secrets: Mutex<Connection>,
}
fn current_account(conn: &Connection, id: &str) -> Result<CodexSubscriptionAccountConfig> {
    let raw: String = conn
        .query_row(
            "SELECT config_json FROM cliproxyapi_runtime_config WHERE config_id=1",
            [],
            |r| r.get(0),
        )
        .optional()?
        .context("native Codex configuration unavailable")?;
    let cfg: CliproxyRuntimeConfig = serde_json::from_str(&raw)
        .map_err(|_| anyhow::anyhow!("native Codex configuration unavailable"))?;
    cfg.codex_accounts
        .into_iter()
        .find(|a| a.id == id)
        .context("native Codex account removed")
}
fn encrypted(conn: &Connection, account: &CodexSubscriptionAccountConfig) -> Result<String> {
    let mut keys = vec![
        &account.id_token_secret,
        &account.access_token_secret,
        &account.refresh_token_secret,
    ];
    keys.extend(account.proxy_url_secret.as_ref());
    let mut hash = Sha256::new();
    hash.update(b"ctox/native-codex-model-records/v1");
    for key in keys {
        let row:Option<(String,String)>=conn.query_row(
            "SELECT nonce_b64,ciphertext_b64 FROM ctox_secret_records WHERE scope=?1 AND secret_name=?2",
            rusqlite::params![key.scope,key.name],|r|Ok((r.get(0)?,r.get(1)?))
        ).optional()?;
        let (nonce, ciphertext) = row.context("native Codex credential unavailable")?;
        for value in [
            key.scope.as_str(),
            key.name.as_str(),
            nonce.as_str(),
            ciphertext.as_str(),
        ] {
            hash.update((value.len() as u64).to_be_bytes());
            hash.update(value.as_bytes());
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}
impl NativeCodexPublicationCheck {
    fn open(root: &Path, expected: &CodexSubscriptionAccountConfig) -> Result<Self> {
        ensure!(!expected.disabled, "native Codex account disabled");
        let runtime = Connection::open_with_flags(
            crate::inference::runtime_env::runtime_config_path(root),
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        runtime.busy_timeout(std::time::Duration::ZERO)?;
        ensure!(
            current_account(&runtime, &expected.id)? == *expected,
            "native Codex configuration changed"
        );
        let secrets = Connection::open_with_flags(
            crate::secrets::resolve_db_path(root),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        secrets.busy_timeout(std::time::Duration::ZERO)?;
        let encrypted = encrypted(&secrets, expected)?;
        Ok(Self {
            account: expected.clone(),
            encrypted,
            runtime: Mutex::new(runtime),
            secrets: Mutex::new(secrets),
        })
    }
    pub(crate) fn prepare(reservation: &NativeCodexModelReservation) -> Result<Self> {
        let check = Self::open(&reservation.root, &reservation.account)?;
        ensure!(
            check.encrypted == reservation.encrypted,
            "native Codex credential changed"
        );
        Ok(check)
    }
    fn records<T>(&self, publish: impl FnOnce() -> Result<T>) -> Result<T> {
        let runtime = self
            .runtime
            .try_lock()
            .map_err(|_| anyhow::anyhow!("native Codex configuration busy"))?;
        let runtime = Transaction::new_unchecked(&runtime, TransactionBehavior::Immediate)?;
        ensure!(
            current_account(&runtime, &self.account.id)? == self.account,
            "native Codex configuration changed"
        );
        let secrets = self
            .secrets
            .try_lock()
            .map_err(|_| anyhow::anyhow!("native Codex credential metadata busy"))?;
        ensure!(
            encrypted(&secrets, &self.account)? == self.encrypted,
            "native Codex credential changed"
        );
        publish()
    }
    /// Called only inside the actual admitted issuer/Core/Policy scope. All
    /// connections were opened before that scope; no secret or network reentry.
    pub(crate) fn with_current_in_held_policy<T>(
        &self,
        reservation: &NativeCodexModelReservation,
        facts: &ConsumerFacts,
        policy: &Connection,
        publish: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let mut active = reservation
            .active
            .try_lock()
            .map_err(|_| anyhow::anyhow!("native Codex reservation busy"))?;
        let result = (|| {
            ensure!(*active, "native Codex reservation released");
            ensure!(
                self.account == reservation.account && self.encrypted == reservation.encrypted,
                "native Codex prepared account changed"
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
impl NativeCodexModelReservation {
    pub(crate) fn prepare_model_check(
        authority: &AdmittedConsumerAuthority,
        id: &str,
        revision: i64,
        model: &str,
    ) -> Result<Self> {
        let selected = capture_consumable_model(authority, id, revision, model)?;
        ensure!(
            selected.account().provider == "codex"
                && selected.account().holder_instance_id == authority.native_host_instance_id()?,
            "model check requires a Codex account held on this native instance"
        );
        let binding = selected
            .private_configuration_binding()
            .context("native Codex account requires current holder discovery")?;
        let root = authority.native_host_root().to_owned();
        // This existing holder reader excludes inherited auth snapshots and
        // shared credential references. No caller can supply a store or token.
        let current = || {
            super::cliproxyapi_host::account_controls::account_binding(
                &root,
                "codex",
                &selected.account().private_local_account_id,
            )
        };
        ensure!(
            current()?.as_deref() == Some(binding),
            "native Codex binding changed"
        );
        let cfg = super::cliproxyapi_host::load_instance_proxy_config(&root)?
            .context("native Codex configuration unavailable")?;
        let account = cfg
            .runtime
            .codex_accounts
            .into_iter()
            .find(|a| a.id == selected.account().private_local_account_id)
            .context("native Codex account unavailable")?;
        let check = NativeCodexPublicationCheck::open(&root, &account)?;
        ensure!(
            current()?.as_deref() == Some(binding),
            "native Codex binding changed during preparation"
        );
        let reservation = Self {
            root,
            selected,
            account,
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
                "native Codex source host changed"
            );
            let check = NativeCodexPublicationCheck::prepare(self)?;
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
#[path = "cliproxyapi_codex_model_tests.rs"]
pub(crate) mod tests;
