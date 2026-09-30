//! Native account-store consumer for the transfer daemon. Enrollment and token
//! refresh belong to authenticated native provisioning, never renderer input.
use anyhow::{ensure, Result};

use ctox_sync::{
    business_data_contract::NativeBusinessDataPrincipal,
    business_data_session::{BusinessDataSessionHost, SavedBusinessDataTarget},
    native::{NativeSessionTarget, NativeSessionTargetProvider, NativeSyncOptions},
};
use futures_util::future::BoxFuture;
use rxdb::plugins::replication_webrtc::{
    LocalSessionCredentials, LocalSessionProvider, WebRTCRsConnection,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io, path::PathBuf, sync::Arc};

use crate::native_data_device::{NativeDeviceKeyScope, NativeDeviceProofKey};

const AUTHORITY_SCOPE: &str = "ctox-native-business-data-accounts";
const CREDENTIAL_SCOPE: &str = "ctox-native-business-data-account-credentials";
const MAX_RECORD_BYTES: usize = 64 * 1024;

/// Non-secret state written by authenticated native enrollment. A renderer may
/// select an existing ID; it cannot supply this state or its source pins.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeTransferAccount {
    pub version: u8,
    pub target_id: String,
    pub public_identity: String,
    pub instance_id: String,
    pub account_epoch: u64,
    pub principal: NativeBusinessDataPrincipal,
    pub active: bool,
}

impl NativeTransferAccount {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1 && self.account_epoch > 0,
            "invalid native account"
        );
        for value in [&self.target_id, &self.instance_id, &self.principal.user_id] {
            ensure!(
                !value.is_empty()
                    && value.len() <= 256
                    && value.trim() == value
                    && !value.chars().any(char::is_control),
                "invalid native account"
            );
        }
        ensure!(
            self.public_identity.len() == 72
                && self.public_identity.starts_with("ed25519:")
                && self.public_identity[8..]
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "invalid native source pin"
        );
        ensure!(
            self.principal.authorization_epoch > 0,
            "invalid native principal"
        );
        let device = self.principal.device.as_ref().ok_or_else(unavailable)?;
        ensure!(
            !device.pairing_id.is_empty()
                && !device.device_id.is_empty()
                && device.proof_key_thumbprint.len() == 43
                && device
                    .proof_key_thumbprint
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'),
            "invalid enrolled device"
        );
        Ok(())
    }

    fn key_scope(&self) -> NativeDeviceKeyScope {
        NativeDeviceKeyScope {
            target_id: self.target_id.clone(),
            source_instance_id: self.instance_id.clone(),
            source_public_identity: self.public_identity.clone(),
            account_epoch: self.account_epoch,
        }
    }

    fn credential_name(&self) -> Result<String> {
        self.validate()?;
        Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(self)?)))
    }
}

/// Stored separately so resolving public pins/options never loads a bearer.
/// Deliberately no Debug implementation and never included in diagnostics.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredCredentials {
    version: u8,
    account: NativeTransferAccount,
    capability_token: String,
}

/// The service owner derives current query-only options from native config.
/// A cached renderer token or an already-installed credential provider is invalid.
pub(crate) type NativeTransferOptionsProvider = Arc<
    dyn Fn(NativeTransferAccount) -> BoxFuture<'static, io::Result<NativeSyncOptions>>
        + Send
        + Sync,
>;

#[derive(Clone)]
pub(crate) struct NativeTransferAccountHost {
    root: PathBuf,
    options: NativeTransferOptionsProvider,
}

fn unavailable() -> anyhow::Error {
    anyhow::anyhow!("native account authority unavailable")
}
fn host_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "native account authority unavailable",
    )
}
fn credential_error() -> rxdb::rx_error::RxError {
    rxdb::rx_error::new_rx_error(
        "RC_WEBRTC_PEER",
        Some(serde_json::json!({
            "code":"native_account_credentials_unavailable",
            "message":"native account credentials unavailable"
        })),
    )
}
fn authority_name(target_id: &str) -> String {
    format!("{:x}", Sha256::digest(target_id.as_bytes()))
}

impl NativeTransferAccountHost {
    pub(crate) fn new(root: PathBuf, options: NativeTransferOptionsProvider) -> Arc<Self> {
        Arc::new(Self { root, options })
    }

    fn read_account(&self, target_id: &str) -> Result<Option<NativeTransferAccount>> {
        let name = authority_name(target_id);
        if !crate::secrets::secret_exists(&self.root, AUTHORITY_SCOPE, &name)? {
            return Ok(None);
        }
        let value = crate::secrets::read_secret_value(&self.root, AUTHORITY_SCOPE, &name)
            .map_err(|_| unavailable())?;
        ensure!(
            value.len() <= MAX_RECORD_BYTES,
            "native account authority unavailable"
        );
        let account: NativeTransferAccount =
            serde_json::from_str(&value).map_err(|_| unavailable())?;
        account.validate().map_err(|_| unavailable())?;
        ensure!(
            account.target_id == target_id,
            "native account authority unavailable"
        );
        Ok(account.active.then_some(account))
    }

    async fn account(&self, target_id: &str) -> io::Result<Option<NativeTransferAccount>> {
        let host = self.clone();
        let target_id = target_id.to_owned();
        tokio::task::spawn_blocking(move || host.read_account(&target_id))
            .await
            .map_err(|_| host_error())?
            .map_err(|_| host_error())
    }

    /// Enumeration restores ID callbacks after daemon restart, without loading
    /// credentials. Each callback resolves live state again when actually used.
    pub(crate) async fn providers(
        self: &Arc<Self>,
    ) -> io::Result<BTreeMap<String, NativeSessionTargetProvider>> {
        let host = self.clone();
        let accounts = tokio::task::spawn_blocking(move || {
            let records = crate::secrets::list_secret_records(&host.root, Some(AUTHORITY_SCOPE))?;
            let mut accounts = Vec::new();
            for record in records {
                let value = crate::secrets::read_secret_value(
                    &host.root,
                    AUTHORITY_SCOPE,
                    &record.secret_name,
                )?;
                ensure!(
                    value.len() <= MAX_RECORD_BYTES,
                    "native account authority unavailable"
                );
                let account: NativeTransferAccount = serde_json::from_str(&value)?;
                account.validate()?;
                ensure!(
                    record.secret_name == authority_name(&account.target_id),
                    "native account authority unavailable"
                );
                if account.active {
                    accounts.push(account);
                }
            }
            Ok::<_, anyhow::Error>(accounts)
        })
        .await
        .map_err(|_| host_error())?
        .map_err(|_| host_error())?;
        Ok(accounts
            .into_iter()
            .map(|account| {
                let id = account.target_id;
                let provider = self.provider(id.clone());
                (id, provider)
            })
            .collect())
    }

    /// Construct a live callback for one target without enumerating other
    /// accounts or loading credentials. The callback denies missing authority.
    pub(crate) fn provider(self: &Arc<Self>, target_id: String) -> NativeSessionTargetProvider {
        let host = self.clone();
        Arc::new(move |connection| {
            let host = host.clone();
            let id = target_id.clone();
            Box::pin(async move {
                let account = host
                    .account(&id)
                    .await
                    .map_err(|_| credential_error())?
                    .ok_or_else(credential_error)?;
                let public_identity = account.public_identity.clone();
                let instance_id = account.instance_id.clone();
                // NativeSyncSession proves these pins before invoking credentials.
                let credentials: LocalSessionProvider<WebRTCRsConnection> =
                    Arc::new(move |current, nonce| {
                        let host = host.clone();
                        let account = account.clone();
                        let same_connection = current == connection;
                        Box::pin(async move {
                            if !same_connection {
                                return Err(credential_error());
                            }
                            let nonce = nonce.ok_or_else(credential_error)?;
                            tokio::task::spawn_blocking(move || host.credentials(&account, &nonce))
                                .await
                                .map_err(|_| credential_error())?
                                .map_err(|_| credential_error())
                        })
                    });
                Ok(NativeSessionTarget {
                    public_identity,
                    instance_id,
                    credentials,
                })
            })
        })
    }

    fn credentials(
        &self,
        expected: &NativeTransferAccount,
        nonce: &str,
    ) -> Result<LocalSessionCredentials> {
        ensure!(
            self.read_account(&expected.target_id)?.as_ref() == Some(expected),
            "native account changed"
        );
        let value = crate::secrets::read_secret_value(
            &self.root,
            CREDENTIAL_SCOPE,
            &expected.credential_name()?,
        )
        .map_err(|_| unavailable())?;
        ensure!(
            value.len() <= MAX_RECORD_BYTES,
            "native account credentials unavailable"
        );
        let stored: StoredCredentials = serde_json::from_str(&value).map_err(|_| unavailable())?;
        ensure!(
            stored.version == 1
                && stored.account == *expected
                && !stored.capability_token.is_empty()
                && stored.capability_token.trim() == stored.capability_token
                && !stored.capability_token.chars().any(char::is_control),
            "native account credentials unavailable"
        );
        // Reconnect can load only the original enrolled signer, never prepare
        // another key or convert a missing key into a new account identity.
        let key = NativeDeviceProofKey::load(&self.root, &expected.key_scope())?;
        ensure!(
            expected.principal.device.as_ref() == Some(&key.device_identity()),
            "native account device changed"
        );
        let device_proof = key.sign_nonce(nonce)?;
        ensure!(
            self.read_account(&expected.target_id)?.as_ref() == Some(expected),
            "native account changed"
        );
        Ok(LocalSessionCredentials {
            capability_token: stored.capability_token,
            device_proof: Some(device_proof),
        })
    }
}

// Read-only host methods are live lookups. A missing/corrupt/inactive record
// denies access; there is no fallback to an earlier enrollment or UI account.
impl BusinessDataSessionHost for NativeTransferAccountHost {
    fn saved_target<'a, 'b, 'f>(
        &'a self,
        target_id: &'b str,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = io::Result<Option<SavedBusinessDataTarget>>>
                + Send
                + 'f,
        >,
    >
    where
        'a: 'f,
        'b: 'f,
        Self: 'f,
    {
        Box::pin(async move {
            Ok(self
                .account(target_id)
                .await?
                .map(|account| SavedBusinessDataTarget {
                    public_identity: account.public_identity,
                    instance_id: account.instance_id,
                    account_epoch: account.account_epoch,
                }))
        })
    }

    fn current_principal<'a, 'b, 'f>(
        &'a self,
        target_id: &'b str,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = io::Result<Option<NativeBusinessDataPrincipal>>>
                + Send
                + 'f,
        >,
    >
    where
        'a: 'f,
        'b: 'f,
        Self: 'f,
    {
        Box::pin(async move {
            Ok(self
                .account(target_id)
                .await?
                .map(|account| account.principal))
        })
    }

    fn native_options<'a, 'b, 'f>(
        &'a self,
        target_id: &'b str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = io::Result<NativeSyncOptions>> + Send + 'f>,
    >
    where
        'a: 'f,
        'b: 'f,
        Self: 'f,
    {
        Box::pin(async move {
            let account = self.account(target_id).await?.ok_or_else(host_error)?;
            let options = (self.options)(account.clone()).await?;
            let query_only = options.local_session_provider.is_none()
                && options.collections.is_empty()
                && options.database.collections.lock().is_empty();
            if !query_only || self.account(target_id).await?.as_ref() != Some(&account) {
                return Err(host_error());
            }
            Ok(options)
        })
    }
}

#[cfg(test)]
#[path = "native_transfer_accounts_tests.rs"]
mod tests;
