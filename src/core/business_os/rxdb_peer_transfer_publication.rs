//! Transfer responses retain native authority until each actual transport poll.
//! Enter issuer -> policy -> optional projection -> exact peer lifecycle.
use super::{rxdb_peer_transfer_grants::principal_from_claims, store};
use anyhow::{ensure, Context};
use ctox_sync::business_data_contract::NativeBusinessDataPrincipal;
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use std::{path::PathBuf, time::Duration};

pub(super) struct TransferPublication {
    pub root: PathBuf,
    pub original_token: String,
    pub principal: NativeBusinessDataPrincipal,
    pub source_instance_id: String,
    pub source_public_identity: String,
}

impl TransferPublication {
    pub fn with_current<T>(
        &self,
        extra_secrets: &[(&str, &str)],
        apply: impl FnOnce(&Connection, &[u8], &[&[u8]], i64) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let mut keys = vec![crate::sync_host::SIGNING_IDENTITY_SECRET_KEY];
        keys.extend_from_slice(extra_secrets);
        store::with_current_webrtc_capability_secrets(&self.root, &keys, |signer, values| {
            let identity = crate::sync_host::signing_identity_from_record(values[0])?;
            ensure!(
                identity.public_identity() == self.source_public_identity,
                "native source issuer changed"
            );
            ensure!(
                store::existing_instance_id(&self.root)? == self.source_instance_id,
                "native source instance changed"
            );
            let mut policy = open_current_store(store::business_os_store_path(&self.root))?;
            let policy = policy.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let at_ms = chrono::Utc::now().timestamp_millis();
            let claims = store::verified_webrtc_capability_claims_from_connection(
                &policy,
                &self.original_token,
                signer,
                at_ms,
            )
            .context("original native transfer principal revoked")?;
            let current = principal_from_claims(claims)
                .map_err(|_| anyhow::anyhow!("native device unavailable"))?;
            ensure!(
                current == self.principal,
                "native transfer principal changed"
            );
            apply(&policy, signer, &values[1..], at_ms)
        })
    }
}

pub(super) fn open_current_store(path: PathBuf) -> anyhow::Result<Connection> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    conn.busy_timeout(Duration::ZERO)?;
    Ok(conn)
}

pub(super) fn denied() -> rxdb::rx_error::RxError {
    rxdb::rx_error::new_rx_error("NATIVE_TRANSFER_PUBLICATION_DENIED", None)
}

/// Both services use the real guarded auxiliary dispatcher. It retains the
/// accepted peer value and original token; no signaling-ID lookup is allowed.
pub(super) fn register<H: rxdb::plugins::replication_webrtc::WebRTCConnectionHandler + 'static>(
    pool: &rxdb::plugins::replication_webrtc::RxWebRTCReplicationPool<H>,
    root: &std::path::Path,
) -> rxdb::rx_error::RxResult<()> {
    let grant_root = root.to_path_buf();
    pool.register_guarded_auxiliary_request_handler(
        crate::transfers_grant::TRANSFER_GRANT_METHOD,
        std::sync::Arc::new(move |_accepted_peer, original_token, params| {
            let root = grant_root.clone();
            Box::pin(async move {
                tokio::task::spawn_blocking(move || {
                    super::rxdb_peer_transfer_grants::prepare(&root, &original_token, params)
                })
                .await
                .map_err(|_| "native transfer grant task failed".to_owned())?
            })
        }),
    )?;
    let account_root = root.to_path_buf();
    pool.register_guarded_auxiliary_request_handler(
        crate::native_transfer_routing::NATIVE_TRANSFER_PROVISION_METHOD,
        std::sync::Arc::new(move |_accepted_peer, original_token, params| {
            let root = account_root.clone();
            Box::pin(async move {
                tokio::task::spawn_blocking(move || {
                    super::rxdb_peer_transfer_accounts::prepare(&root, &original_token, params)
                })
                .await
                .map_err(|_| "native transfer account task failed".to_owned())?
            })
        }),
    )
}
