//! Source-issued native transfer grants. Grant IDs are references, never bearer
//! credentials; every use goes back to the authenticated source's current policy.
use anyhow::{ensure, Context, Result};
use ctox_sync::native::NativeSyncSession;
use ctox_transfers::DownloadRequest;
use rxdb::plugins::replication_webrtc::{
    send_message_and_await_answer, WebRTCMessage, WebRTCRsConnection,
};
use serde::{Deserialize, Serialize};
use std::{
    future::Future,
    pin::Pin,
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

pub(crate) const TRANSFER_GRANT_METHOD: &str = "ctox.transfer.grant.v1";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TransferGrantScope {
    pub transfer_id: String,
    pub source_instance_id: String,
    pub source_public_key: String,
    pub collection: String,
    pub file_id: String,
    pub sha256: String,
    pub size: u64,
}
impl TransferGrantScope {
    pub(crate) fn from_request(request: &DownloadRequest) -> Result<Self> {
        ensure!(
            request.sources.is_empty(),
            "native grant requires peer-only transfer"
        );
        let source = request
            .peer_source
            .as_ref()
            .context("peer source required")?;
        let scope = Self {
            transfer_id: request.id.clone(),
            source_instance_id: source.instance_id.clone(),
            source_public_key: source.public_key.clone(),
            collection: source.collection.clone(),
            file_id: source.file_id.clone(),
            sha256: request.sha256.clone(),
            size: request.size,
        };
        scope.validate()?;
        Ok(scope)
    }
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            !self.transfer_id.is_empty()
                && self.transfer_id.len() <= 128
                && self
                    .transfer_id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
            "invalid transfer id"
        );
        for (value, maximum) in [
            (&self.source_instance_id, 256),
            (&self.source_public_key, 256),
            (&self.file_id, 1024),
        ] {
            ensure!(
                !value.is_empty()
                    && value.len() <= maximum
                    && value.trim() == value
                    && !value.chars().any(char::is_control),
                "invalid native grant scope"
            );
        }
        // Other blob sources have no authoritative immutable generation metadata
        // on this path yet. They must not get a permissive generic grant.
        ensure!(
            self.collection == "desktop_files",
            "native transfer grant source unsupported"
        );
        ensure!(
            self.sha256.len() == 64
                && self
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "invalid native grant content digest"
        );
        ensure!(self.size <= i64::MAX as u64, "invalid native grant size");
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum TransferGrantRequest {
    Issue {
        scope: TransferGrantScope,
    },
    Check {
        #[serde(rename = "grantId")]
        grant_id: String,
        scope: TransferGrantScope,
    },
    Revoke {
        #[serde(rename = "grantId")]
        grant_id: String,
    },
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TransferGrantReply {
    pub grant_id: String,
    pub scope: TransferGrantScope,
    pub expires_at_ms: i64,
}

/// Concrete admission adapter for the daemon's live NativeSyncSession. No UI
/// token, saved request, receipt or identity pin can replace the source check.
pub(crate) struct NativeTransferGrantAdmission {
    session: Arc<NativeSyncSession>,
}
impl NativeTransferGrantAdmission {
    pub(crate) fn new(session: Arc<NativeSyncSession>) -> Self {
        Self { session }
    }
    pub(crate) async fn issue(
        &self,
        connection: &WebRTCRsConnection,
        request: &DownloadRequest,
    ) -> Result<TransferGrantReply> {
        let scope = TransferGrantScope::from_request(request)?;
        let value = self
            .exchange(
                connection,
                TransferGrantRequest::Issue {
                    scope: scope.clone(),
                },
            )
            .await?;
        let reply: TransferGrantReply = serde_json::from_value(value)?;
        ensure!(
            reply.scope == scope && valid_grant_id(&reply.grant_id),
            "source returned mismatched native transfer grant"
        );
        ensure!(
            reply.expires_at_ms > chrono::Utc::now().timestamp_millis(),
            "source returned expired native transfer grant"
        );
        Ok(reply)
    }
    pub(crate) async fn revoke(
        &self,
        connection: &WebRTCRsConnection,
        request: &DownloadRequest,
    ) -> Result<()> {
        let source = request
            .peer_source
            .as_ref()
            .context("peer source required")?;
        let binding = source
            .account_binding
            .as_ref()
            .context("original native grant binding required")?;
        ensure!(
            valid_grant_id(&binding.grant_id),
            "invalid issued native transfer grant id"
        );
        self.session
            .peer_identity_proof(connection.clone(), &source.public_key, &source.instance_id)
            .await?;
        let result = self
            .exchange(
                connection,
                TransferGrantRequest::Revoke {
                    grant_id: binding.grant_id.clone(),
                },
            )
            .await?;
        ensure!(
            result == serde_json::json!({"revoked":true}),
            "native transfer grant revocation failed"
        );
        Ok(())
    }

    async fn exchange(
        &self,
        connection: &WebRTCRsConnection,
        request: TransferGrantRequest,
    ) -> Result<serde_json::Value> {
        let pool = self.session.pool();
        let current = || {
            !pool.canceled.load(Ordering::SeqCst)
                && pool.connection_handler.is_peer_current(connection)
                && pool.is_peer_ready_for_control(connection)
        };
        ensure!(current(), "native transfer grant connection retired");
        let scope = match &request {
            TransferGrantRequest::Issue { scope } | TransferGrantRequest::Check { scope, .. } => {
                Some(scope)
            }
            _ => None,
        };
        if let Some(scope) = scope {
            self.session
                .peer_identity_proof(
                    connection.clone(),
                    &scope.source_public_key,
                    &scope.source_instance_id,
                )
                .await?;
        }
        ensure!(current(), "native transfer grant connection retired");
        let response = tokio::select! {
            biased;
            _ = pool.cancelled() => anyhow::bail!("native transfer grant session stopped"),
            response = tokio::time::timeout(Duration::from_secs(10), send_message_and_await_answer(
                pool.connection_handler.clone(), connection.clone(), WebRTCMessage {
                    id: format!("transfer-grant-{}", uuid::Uuid::new_v4()), method: TRANSFER_GRANT_METHOD.into(),
                    params: vec![serde_json::to_value(request)?], collection: None,
                })) => response.context("native transfer grant deadline")??,
        };
        ensure!(
            current()
                && response.error.is_none()
                && serde_json::to_vec(&response.result)?.len() <= 4096,
            "native transfer grant rejected or retired"
        );
        Ok(response.result)
    }
}
impl crate::transfers_peer::NativePeerJobAdmission for NativeTransferGrantAdmission {
    fn authorize<'a>(
        &'a self,
        request: &'a DownloadRequest,
        connection: &'a WebRTCRsConnection,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let binding = request
                .peer_source
                .as_ref()
                .and_then(|s| s.account_binding.as_ref())
                .context("original native grant binding required")?;
            ensure!(
                valid_grant_id(&binding.grant_id),
                "invalid issued native transfer grant id"
            );
            let scope = TransferGrantScope::from_request(request)?;
            let value = self
                .exchange(
                    connection,
                    TransferGrantRequest::Check {
                        grant_id: binding.grant_id.clone(),
                        scope,
                    },
                )
                .await?;
            ensure!(
                value == serde_json::json!({"authorized":true}),
                "native transfer grant check failed"
            );
            Ok(())
        })
    }
}
pub(crate) fn valid_grant_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
