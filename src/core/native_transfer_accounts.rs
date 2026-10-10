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
use crate::native_transfer_routing::{
    NativeTransferProvisionReply, NativeTransferProvisionRequest, NativeTransferRouting,
    NATIVE_TRANSFER_PROVISION_METHOD,
};
use ctox_sync::native::NativeSyncSession;
use rxdb::plugins::replication_webrtc::{
    send_message_and_await_answer, WebRTCConnectionHandler, WebRTCMessage,
};
use std::{sync::atomic::Ordering, time::Duration};

const AUTHORITY_SCOPE: &str = "ctox-native-business-data-accounts";
const CREDENTIAL_SCOPE: &str = "ctox-native-business-data-account-credentials";
const ROUTING_SCOPE: &str = "ctox-native-business-data-account-routing";
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
        // Business OS capability epochs start at zero. Preserve the exact
        // source-verified epoch; the positive local account generation above
        // is a separate value. Current source policy still fences revocation.
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
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredRouting {
    account: NativeTransferAccount,
    routing: NativeTransferRouting,
}

/// A cached renderer token or an already-installed credential provider is invalid.
pub(crate) type NativeTransferOptionsProvider = Arc<
    dyn Fn(NativeTransferAccount) -> BoxFuture<'static, io::Result<NativeSyncOptions>>
        + Send
        + Sync,
>;

#[derive(Clone, Copy)]
pub(crate) struct NativeTransferSessionDeadline {
    pub refresh_after_ms: i64,
    pub expires_at_ms: i64,
}

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
    fn read_record(&self, scope: &str, name: &str) -> Result<Option<String>> {
        if !crate::secrets::secret_exists(&self.root, scope, name)? {
            return Ok(None);
        }
        let value = crate::secrets::read_secret_value(&self.root, scope, name)
            .map_err(|_| unavailable())?;
        ensure!(
            value.len() <= MAX_RECORD_BYTES,
            "native account record unavailable"
        );
        Ok(Some(value))
    }

    fn read_retained_routing(
        &self,
        account: &NativeTransferAccount,
    ) -> Result<NativeTransferRouting> {
        let value = self
            .read_record(ROUTING_SCOPE, &account.credential_name()?)?
            .ok_or_else(unavailable)?;
        let stored: StoredRouting = serde_json::from_str(&value).map_err(|_| unavailable())?;
        ensure!(stored.account == *account, "native routing account changed");
        stored
            .routing
            .retained_rendezvous(&account.instance_id, chrono::Utc::now().timestamp_millis())?;
        Ok(stored.routing)
    }

    fn read_routing(&self, account: &NativeTransferAccount) -> Result<NativeTransferRouting> {
        let routing = self.read_retained_routing(account)?;
        routing.validate(&account.instance_id, chrono::Utc::now().timestamp_millis())?;
        Ok(routing)
    }

    /// A bounded publication fence for the exact original native enrollment and
    /// credential generation. The fingerprint is private and never a wire permit.
    /// No awaits, secret API reentry, or transport reentry inside apply.
    pub(crate) fn with_current_enrollment<T>(
        &self,
        expected: &NativeTransferAccount,
        fingerprint: Option<&str>,
        apply: impl FnOnce(&str) -> Result<T>,
    ) -> Result<T> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct CredentialBinding<'a> {
            version: u8,
            account: NativeTransferAccount,
            // Borrow the bearer without materializing an additional plaintext copy.
            #[serde(borrow)]
            capability_token: &'a str,
        }
        expected.validate()?;
        ensure!(expected.active, "native enrollment retired");
        let authority = authority_name(&expected.target_id);
        let credentials = expected.credential_name()?;
        crate::secrets::with_current_secret_values_and_fingerprint(
            &self.root,
            &[
                (AUTHORITY_SCOPE, &authority),
                (CREDENTIAL_SCOPE, &credentials),
            ],
            |values, current_fingerprint| {
                ensure!(
                    values.iter().all(|value| value.len() <= MAX_RECORD_BYTES),
                    "native enrollment record budget exceeded"
                );
                let current: NativeTransferAccount = serde_json::from_slice(values[0])?;
                let credential: CredentialBinding = serde_json::from_slice(values[1])?;
                ensure!(
                    !credential.capability_token.is_empty(),
                    "native credential missing"
                );
                ensure!(
                    current == *expected
                        && credential.version == 1
                        && credential.account == *expected,
                    "native enrollment changed"
                );
                ensure!(
                    fingerprint.is_none_or(|expected| expected == current_fingerprint),
                    "native credential generation changed"
                );
                apply(current_fingerprint)
            },
        )
    }

    /// Native service refresh deadline and current ICE snapshot. Callers must
    /// renew through the live source and recreate the session before expiry;
    /// an expired snapshot never falls back to local daemon configuration.
    pub(crate) async fn routing(&self, target_id: &str) -> io::Result<NativeTransferRouting> {
        let account = self.account(target_id).await?.ok_or_else(host_error)?;
        let host = self.clone();
        tokio::task::spawn_blocking(move || {
            let routing = host.read_routing(&account)?;
            ensure!(
                host.read_account(&account.target_id)?.as_ref() == Some(&account),
                "native account changed"
            );
            Ok::<_, anyhow::Error>(routing)
        })
        .await
        .map_err(|_| host_error())?
        .map_err(|_| host_error())
    }

    pub(crate) fn new(root: PathBuf, options: NativeTransferOptionsProvider) -> Arc<Self> {
        Arc::new(Self { root, options })
    }

    pub(crate) async fn require_new_target(&self, target_id: &str) -> Result<()> {
        let host = self.clone();
        let target_id = target_id.to_owned();
        tokio::task::spawn_blocking(move || {
            ensure!(
                host.read_record(AUTHORITY_SCOPE, &authority_name(&target_id))?
                    .is_none(),
                "native target already exists; pairing cannot replace or recover an account"
            );
            Ok(())
        })
        .await?
    }

    /// Explicit first enrollment only. Invite credentials remain native and are
    /// released only after NativeSyncSession proves the supplied source pin.
    pub(crate) async fn pairing_provider(
        self: &Arc<Self>,
        scope: NativeDeviceKeyScope,
        invite_secret: String,
    ) -> Result<NativeSessionTargetProvider> {
        ensure!(
            scope.account_epoch == 1,
            "initial native account epoch required"
        );
        ensure!(
            invite_secret.len() == 43
                && invite_secret
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'),
            "native one-time pairing invite required"
        );
        self.require_new_target(&scope.target_id).await?;
        let root = self.root.clone();
        let key_scope = scope.clone();
        tokio::task::spawn_blocking(move || {
            NativeDeviceProofKey::prepare(&root, &key_scope).map(|_| ())
        })
        .await??;
        self.require_new_target(&scope.target_id).await?;
        let host = self.clone();
        Ok(Arc::new(move |connection| {
            let host = host.clone();
            let scope = scope.clone();
            let token = invite_secret.clone();
            Box::pin(async move {
                let stale = || rxdb::rx_error::new_rx_error("RC_WEBRTC_PEER", None);
                host.require_new_target(&scope.target_id)
                    .await
                    .map_err(|_| stale())?;
                let identity = scope.source_public_identity.clone();
                let instance = scope.source_instance_id.clone();
                let credentials: LocalSessionProvider<WebRTCRsConnection> =
                    Arc::new(move |current, nonce| {
                        let host = host.clone();
                        let scope = scope.clone();
                        let token = token.clone();
                        let same_connection = current == connection;
                        Box::pin(async move {
                            let stale = || rxdb::rx_error::new_rx_error("RC_WEBRTC_PEER", None);
                            if !same_connection {
                                return Err(stale());
                            }
                            host.pairing_credentials(&scope, token, nonce)
                                .await
                                .map_err(|_| stale())
                        })
                    });
                Ok(NativeSessionTarget {
                    public_identity: identity,
                    instance_id: instance,
                    credentials,
                })
            })
        }))
    }

    /// The initial protocol probe has no remote nonce. NativeSyncSession has
    /// already verified the source pin before calling this boundary; the source
    /// still defers device admission until its actual nonce is answered.
    async fn pairing_credentials(
        &self,
        scope: &NativeDeviceKeyScope,
        capability_token: String,
        nonce: Option<String>,
    ) -> Result<LocalSessionCredentials> {
        self.require_new_target(&scope.target_id).await?;
        let root = self.root.clone();
        let key_scope = scope.clone();
        let device_proof = tokio::task::spawn_blocking(move || {
            let key = NativeDeviceProofKey::load(&root, &key_scope)?;
            nonce
                .as_deref()
                .map(|nonce| key.sign_nonce(nonce))
                .transpose()
        })
        .await??;
        self.require_new_target(&scope.target_id).await?;
        Ok(LocalSessionCredentials {
            capability_token,
            device_proof,
        })
    }

    /// Admission of a new job captures its account before connecting. Do not
    /// release a different account's credentials if enrollment changes meanwhile.
    pub(crate) fn provider_for_account(
        self: &Arc<Self>,
        expected: NativeTransferAccount,
    ) -> NativeSessionTargetProvider {
        let host = self.clone();
        let provider = self.provider(expected.target_id.clone());
        Arc::new(move |connection| {
            let host = host.clone();
            let provider = provider.clone();
            let expected = expected.clone();
            Box::pin(async move {
                let stale = || rxdb::rx_error::new_rx_error("RC_WEBRTC_PEER", None);
                if host
                    .account(&expected.target_id)
                    .await
                    .map_err(|_| stale())?
                    .as_ref()
                    != Some(&expected)
                {
                    return Err(stale());
                }
                let target = provider(connection).await?;
                // The underlying callback captures this exact account and checks
                // it again before and after reading credentials/signing a nonce.
                if host
                    .account(&expected.target_id)
                    .await
                    .map_err(|_| stale())?
                    .as_ref()
                    != Some(&expected)
                    || target.public_identity != expected.public_identity
                    || target.instance_id != expected.instance_id
                {
                    return Err(stale());
                }
                Ok(target)
            })
        })
    }

    /// An expired source descriptor may locate that same source for control-only
    /// recovery. Never reuse its ICE credentials, grant payload access, or change
    /// the saved account. The caller must provision, close this session and open
    /// a fresh routed session before authorizing a file or issuing a new grant.
    pub(crate) async fn recovery_options(
        &self,
        target_id: &str,
    ) -> io::Result<Option<NativeSyncOptions>> {
        let account = self.account(target_id).await?.ok_or_else(host_error)?;
        let host = self.clone();
        let expected = account.clone();
        let routing = tokio::task::spawn_blocking(move || {
            let routing = host.read_retained_routing(&expected)?;
            ensure!(
                host.read_account(&expected.target_id)?.as_ref() == Some(&expected),
                "native account changed"
            );
            Ok::<_, anyhow::Error>(routing)
        })
        .await
        .map_err(|_| host_error())?
        .map_err(|_| host_error())?;
        let now = chrono::Utc::now().timestamp_millis();
        if routing.expires_at_ms > now {
            return Ok(None);
        }
        let rendezvous = routing
            .retained_rendezvous(&account.instance_id, now)
            .map_err(|_| host_error())?;
        let mut options = (self.options)(account.clone()).await?;
        options.room = rendezvous.room.clone();
        // Retain only credential-free source STUN discovery. An explicit empty
        // entry suppresses native defaults if the source supplied none. Never
        // recycle expired TURN credentials; the source advertises its own
        // current candidates on the newly authenticated connection.
        options.ice_servers = rendezvous.bootstrap_ice();
        let host = self.clone();
        let enrolled = account.clone();
        let peer_id = options.peer_session_id.clone();
        options.signaling_urls = Arc::new(move || {
            let resolve = || -> Result<Vec<String>> {
                ensure!(
                    host.read_account(&enrolled.target_id)?.as_ref() == Some(&enrolled),
                    "native account changed"
                );
                let now = chrono::Utc::now().timestamp_millis();
                let current = host
                    .read_retained_routing(&enrolled)?
                    .retained_rendezvous(&enrolled.instance_id, now)?;
                ensure!(current == rendezvous, "native rendezvous changed");
                let urls = current.signaling_at(&peer_id, now)?;
                ensure!(
                    host.read_account(&enrolled.target_id)?.as_ref() == Some(&enrolled),
                    "native account changed"
                );
                Ok(urls)
            };
            resolve().unwrap_or_default()
        });
        let query_only = options.local_session_provider.is_none()
            && options.collections.is_empty()
            && options.database.collections.lock().is_empty();
        if !query_only || self.account(target_id).await?.as_ref() != Some(&account) {
            return Err(host_error());
        }
        Ok(Some(options))
    }

    /// Return the deadlines from the exact descriptor used for these options.
    /// A separate routing read could race another native refresh.
    pub(crate) async fn native_options_with_deadline(
        &self,
        target_id: &str,
    ) -> io::Result<(NativeSyncOptions, NativeTransferSessionDeadline)> {
        let account = self.account(target_id).await?.ok_or_else(host_error)?;
        let routing = self.routing(target_id).await?;
        let mut options = (self.options)(account.clone()).await?;
        let deadline = NativeTransferSessionDeadline {
            refresh_after_ms: routing.refresh_after_ms,
            expires_at_ms: routing.expires_at_ms,
        };
        options.ice_servers = routing.ice();
        options.room = routing.room;
        let host = self.clone();
        let enrolled = account.clone();
        let peer_id = options.peer_session_id.clone();
        let enrolled_room = options.room.clone();
        let ice_expires = routing.expires_at_ms;
        options.signaling_urls = Arc::new(move || {
            // Re-read the encrypted source descriptor on each reconnect.
            // Revocation/rotation/expiry yields no route, never local config.
            if host
                .read_account(&enrolled.target_id)
                .ok()
                .flatten()
                .as_ref()
                != Some(&enrolled)
            {
                return Vec::new();
            }
            let now = chrono::Utc::now().timestamp_millis();
            if now >= ice_expires {
                return Vec::new();
            }
            let urls = host
                .read_routing(&enrolled)
                .and_then(|routing| {
                    ensure!(routing.room == enrolled_room, "native routing room changed");
                    routing.signaling_at(&enrolled.instance_id, &peer_id, now)
                })
                .unwrap_or_default();
            if host
                .read_account(&enrolled.target_id)
                .ok()
                .flatten()
                .as_ref()
                != Some(&enrolled)
            {
                return Vec::new();
            }
            urls
        });
        let query_only = options.local_session_provider.is_none()
            && options.collections.is_empty()
            && options.database.collections.lock().is_empty();
        if !query_only
            || chrono::Utc::now().timestamp_millis() >= ice_expires
            || self.account(target_id).await?.as_ref() != Some(&account)
        {
            return Err(host_error());
        }
        Ok((options, deadline))
    }

    /// Initial enrollment and renewal both require an actual ready native
    /// session plus a fresh source proof. No bearer/principal/route is accepted
    /// as renderer input. Explicit pairing must already have prepared the key.
    pub(crate) async fn provision_from_session(
        &self,
        scope: NativeDeviceKeyScope,
        session: &NativeSyncSession,
        connection: &WebRTCRsConnection,
    ) -> Result<NativeTransferAccount> {
        let host = self.clone();
        let target = scope.target_id.clone();
        let (previous, previous_route) = tokio::task::spawn_blocking(move || {
            let previous = host.read_record(AUTHORITY_SCOPE, &authority_name(&target))?;
            let previous_route = previous
                .as_ref()
                .map(|value| {
                    let account: NativeTransferAccount = serde_json::from_str(value)?;
                    account.validate()?;
                    ensure!(account.target_id == target, "native account target changed");
                    host.read_record(ROUTING_SCOPE, &account.credential_name()?)
                })
                .transpose()?
                .flatten();
            Ok::<_, anyhow::Error>((previous, previous_route))
        })
        .await??;
        let pool = session.pool();
        let current = || {
            !pool.canceled.load(Ordering::SeqCst)
                && pool.connection_handler.is_peer_current(connection)
                && pool.is_peer_ready_for_control(connection)
        };
        ensure!(current(), "native enrollment connection retired");
        session
            .peer_identity_proof(
                connection.clone(),
                &scope.source_public_identity,
                &scope.source_instance_id,
            )
            .await?;
        ensure!(current(), "native enrollment connection retired");
        let response = tokio::select! {
            biased;
            _ = pool.cancelled() => anyhow::bail!("native enrollment session stopped"),
            response = tokio::time::timeout(Duration::from_secs(10), send_message_and_await_answer(
                pool.connection_handler.clone(), connection.clone(), WebRTCMessage {
                    id: format!("native-account-{}", uuid::Uuid::new_v4()),
                    method: NATIVE_TRANSFER_PROVISION_METHOD.into(), collection: None,
                    params: vec![serde_json::to_value(NativeTransferProvisionRequest {
                        source_public_identity: scope.source_public_identity.clone(),
                        source_instance_id: scope.source_instance_id.clone(),
                    })?],
                })) => response.map_err(|_| unavailable())??,
        };
        ensure!(
            current()
                && response.error.is_none()
                && serde_json::to_vec(&response.result)?.len() <= MAX_RECORD_BYTES,
            "native enrollment rejected or retired"
        );
        let reply: NativeTransferProvisionReply =
            serde_json::from_value(response.result).map_err(|_| unavailable())?;
        let host = self.clone();
        // Keep cancellation and exact-connection currency inside the blocking
        // commit too. Account and route CAS guard concurrent writers/processes.
        let pool = pool.clone();
        let connection = connection.clone();
        tokio::task::spawn_blocking(move || {
            ensure!(
                !pool.canceled.load(Ordering::SeqCst)
                    && pool.connection_handler.is_peer_current(&connection)
                    && pool.is_peer_ready_for_control(&connection),
                "native enrollment connection retired"
            );
            host.commit_authenticated(
                &scope,
                previous.as_deref(),
                previous_route.as_deref(),
                reply,
            )
        })
        .await?
    }

    fn commit_authenticated(
        &self,
        scope: &NativeDeviceKeyScope,
        previous: Option<&str>,
        previous_route: Option<&str>,
        reply: NativeTransferProvisionReply,
    ) -> Result<NativeTransferAccount> {
        let now = chrono::Utc::now().timestamp_millis();
        ensure!(
            reply.version == 1
                && reply.source_public_identity == scope.source_public_identity
                && reply.source_instance_id == scope.source_instance_id
                && reply.capability_expires_at_ms > now
                && !reply.capability_token.is_empty()
                && reply.capability_token.len() <= 16 * 1024
                && reply.capability_token.trim() == reply.capability_token
                && !reply.capability_token.chars().any(char::is_control),
            "native enrollment rejected"
        );
        reply.routing.validate(&scope.source_instance_id, now)?;
        let key = NativeDeviceProofKey::load(&self.root, scope)?;
        ensure!(
            reply.principal.device.as_ref() == Some(&key.device_identity()),
            "native enrollment signer mismatch"
        );
        let account = NativeTransferAccount {
            version: 1,
            target_id: scope.target_id.clone(),
            public_identity: scope.source_public_identity.clone(),
            instance_id: scope.source_instance_id.clone(),
            account_epoch: scope.account_epoch,
            principal: reply.principal,
            active: true,
        };
        account.validate()?;
        let old = previous
            .map(serde_json::from_str::<NativeTransferAccount>)
            .transpose()?;
        if let Some(old) = &old {
            old.validate()?;
            ensure!(
                old.target_id == account.target_id
                    && (old == &account || account.account_epoch > old.account_epoch),
                "explicit newer account enrollment required"
            );
        }
        let name = account.credential_name()?;
        let authority = serde_json::to_string(&account)?;
        let credentials = serde_json::to_string(&StoredCredentials {
            version: 1,
            account: account.clone(),
            capability_token: reply.capability_token,
        })?;
        let route = serde_json::to_string(&StoredRouting {
            account: account.clone(),
            routing: reply.routing,
        })?;
        let authority_id = authority_name(&account.target_id);
        let old_name = old
            .as_ref()
            .map(NativeTransferAccount::credential_name)
            .transpose()?;
        let route_guard = old_name.as_deref().unwrap_or(&name);
        let deletes = old_name
            .as_deref()
            .filter(|old| *old != name)
            .map(|old| vec![(CREDENTIAL_SCOPE, old), (ROUTING_SCOPE, old)])
            .unwrap_or_default();
        ensure!(
            crate::secrets::compare_and_write_secret_records(
                &self.root,
                &[
                    (AUTHORITY_SCOPE, &authority_id, previous),
                    (ROUTING_SCOPE, route_guard, previous_route)
                ],
                &[
                    crate::secrets::SecretRecordWrite {
                        scope: AUTHORITY_SCOPE,
                        name: &authority_id,
                        value: &authority,
                        description: None,
                        metadata: serde_json::json!({"version":1})
                    },
                    crate::secrets::SecretRecordWrite {
                        scope: CREDENTIAL_SCOPE,
                        name: &name,
                        value: &credentials,
                        description: None,
                        metadata: serde_json::json!({"version":1})
                    },
                    crate::secrets::SecretRecordWrite {
                        scope: ROUTING_SCOPE,
                        name: &name,
                        value: &route,
                        description: None,
                        metadata: serde_json::json!({"version":1})
                    },
                ],
                &deletes
            )?,
            "native account changed during enrollment"
        );
        Ok(account)
    }

    /// Native explicit disconnect persists a tombstone and erases that exact
    /// credential/routing tuple in one transaction. A stale disconnect cannot
    /// remove a newly enrolled account with a later epoch.
    pub(crate) async fn revoke(&self, expected: NativeTransferAccount) -> Result<()> {
        let host = self.clone();
        tokio::task::spawn_blocking(move || {
            let name = expected.credential_name()?;
            let previous = serde_json::to_string(&expected)?;
            let authority_id = authority_name(&expected.target_id);
            let mut inactive = expected;
            inactive.active = false;
            let tombstone = serde_json::to_string(&inactive)?;
            ensure!(
                crate::secrets::compare_and_write_secret_records(
                    &host.root,
                    &[(AUTHORITY_SCOPE, &authority_id, Some(&previous))],
                    &[crate::secrets::SecretRecordWrite {
                        scope: AUTHORITY_SCOPE,
                        name: &authority_id,
                        value: &tombstone,
                        description: None,
                        metadata: serde_json::json!({"version":1})
                    }],
                    &[(CREDENTIAL_SCOPE, &name), (ROUTING_SCOPE, &name)]
                )?,
                "native account changed during disconnect"
            );
            Ok(())
        })
        .await?
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

    pub(crate) async fn account(
        &self,
        target_id: &str,
    ) -> io::Result<Option<NativeTransferAccount>> {
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
                            tokio::task::spawn_blocking(move || {
                                host.credentials(&account, nonce.as_deref())
                            })
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
        nonce: Option<&str>,
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
        let device_proof = nonce.map(|nonce| key.sign_nonce(nonce)).transpose()?;
        ensure!(
            self.read_account(&expected.target_id)?.as_ref() == Some(expected),
            "native account changed"
        );
        Ok(LocalSessionCredentials {
            capability_token: stored.capability_token,
            device_proof,
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
            self.native_options_with_deadline(target_id)
                .await
                .map(|(options, _)| options)
        })
    }
}

#[cfg(test)]
#[path = "native_transfer_accounts_tests.rs"]
mod tests;
