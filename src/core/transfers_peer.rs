//! Native-owned binding between durable transfer jobs and an authenticated Sync
//! connection. A route or persisted request cannot instantiate this binding.
#[path = "transfers_peer_resolver.rs"]
mod resolver;
pub(crate) use resolver::NativeTransferPeerResolver;
#[cfg(test)]
#[path = "transfers_peer_tests.rs"]
mod tests;
use anyhow::{ensure, Context, Result};
use ctox_sync::{
    business_data_contract::NativeBusinessDataPrincipal,
    business_data_session::{BusinessDataSessionHost, SavedBusinessDataTarget},
    native::NativeSyncSession,
};
use ctox_transfers::{
    DownloadRequest, PeerAccountBinding, PeerRangeSource, PeerReadFailure, Store, Transfer,
};
use rxdb::plugins::replication_webrtc::{
    file_fetch_client::{fetch_file_range, FileRangeBytes},
    file_fetch_handler::{FileFetchRequest, FileRange},
    WebRTCRsConnection,
};
use rxdb::rx_error::RxError;
use sha2::{Digest, Sha256};
use std::time::Duration;
use std::{future::Future, pin::Pin, sync::Arc};

// The RPC already returns bounded reasons. Never expose its raw message/parameters.
fn range_failure(error: &RxError) -> PeerReadFailure {
    if error.code() != "RC_WEBRTC_FILE" {
        return PeerReadFailure::Unavailable;
    }
    match error
        .parameters()
        .get("reason")
        .and_then(serde_json::Value::as_str)
    {
        Some("file_timeout") => PeerReadFailure::Timeout,
        Some("local_fetch_limit") => PeerReadFailure::Busy,
        Some("chunk_sequence_gap") => PeerReadFailure::SequenceGap,
        Some("peer_not_ready" | "peer_disconnected") => PeerReadFailure::Disconnected,
        Some(
            "file_pool_closed"
            | "response_stream_closed"
            | "message_stream_closed"
            | "disconnect_stream_closed",
        ) => PeerReadFailure::Closed,
        Some("file_not_accepted" | "remote_file_error") => PeerReadFailure::Rejected,
        Some(
            "chunk_after_completion"
            | "invalid_chunk"
            | "chunk_sequence_overflow"
            | "file_cancelled"
            | "chunk_too_large"
            | "invalid_base64"
            | "range_too_large"
            | "chunk_hash_mismatch"
            | "invalid_terminal_chunk"
            | "empty_data_chunk"
            | "range_incomplete",
        ) => PeerReadFailure::InvalidChunk,
        _ => PeerReadFailure::Unavailable,
    }
}

async fn fetch_authorized_range<A, R, AF, RF>(
    mut authorize: A,
    mut fetch: R,
    retry_delay: Duration,
) -> Result<FileRangeBytes>
where
    A: FnMut() -> AF,
    R: FnMut() -> RF,
    AF: Future<Output = Result<()>>,
    RF: Future<Output = std::result::Result<FileRangeBytes, RxError>>,
{
    for attempt in 0..3 {
        // Retain the original job/grant and fail closed if authority changes.
        authorize()
            .await
            .map_err(|_| anyhow::Error::from(PeerReadFailure::Authorization))?;
        match fetch().await {
            Ok(bytes) => return Ok(bytes),
            Err(error) => {
                let failure = range_failure(&error);
                if !failure.retryable() || attempt == 2 {
                    return Err(failure.into());
                }
                // fetch_file_range discards partial responses and cancels its RPC
                // on drop. The worker's durable byte checkpoint does not advance.
                tokio::time::sleep(retry_delay).await;
            }
        }
    }
    unreachable!("native range attempts are bounded")
}

/// Implemented by the daemon's existing account/session host. This must reject
/// revoked jobs, changed accounts/epochs and stale connection generations, and
/// apply current file policy. A previous receipt or peer identity is not a grant.
pub(crate) trait NativePeerJobAdmission: Send + Sync {
    fn authorize<'a>(
        &'a self,
        request: &'a DownloadRequest,
        connection: &'a WebRTCRsConnection,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
}

pub(crate) fn principal_digest(principal: &NativeBusinessDataPrincipal) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(principal)?)
    ))
}

/// Capture the existing host's authority only when creating a new job. Resume
/// must retain this immutable snapshot; neither this digest nor enrollment is
/// a substitute for the source's current file policy.
pub(crate) async fn enqueue_enrolled_peer(
    store: &Store,
    host: &dyn BusinessDataSessionHost,
    target_id: &str,
    grant_id: &str,
    mut request: DownloadRequest,
) -> Result<Transfer> {
    let source = request
        .peer_source
        .as_mut()
        .context("peer source required")?;
    ensure!(
        source.account_binding.is_none(),
        "job already has an account binding"
    );
    let saved = host
        .saved_target(target_id)
        .await?
        .context("target is not enrolled")?;
    ensure!(
        saved.instance_id == source.instance_id && saved.public_identity == source.public_key,
        "source differs from enrolled target"
    );
    let principal = host
        .current_principal(target_id)
        .await?
        .context("target has no current account")?;
    source.account_binding = Some(PeerAccountBinding {
        target_id: target_id.into(),
        grant_id: grant_id.into(),
        account_epoch: saved.account_epoch,
        principal_sha256: principal_digest(&principal)?,
    });
    // A changed account during snapshot capture cannot create an admitted job.
    ensure!(
        current_account(host, &request).await? == principal,
        "account changed during transfer admission"
    );
    store.enqueue(request)
}

struct EnrolledPeerJobAdmission {
    host: Arc<dyn BusinessDataSessionHost>,
    session: Arc<NativeSyncSession>,
    grant_admission: Arc<dyn NativePeerJobAdmission>,
}

pub(crate) async fn current_account(
    host: &dyn BusinessDataSessionHost,
    request: &DownloadRequest,
) -> Result<NativeBusinessDataPrincipal> {
    let source = request
        .peer_source
        .as_ref()
        .context("peer source required")?;
    let binding = source
        .account_binding
        .as_ref()
        .context("original account binding required")?;
    binding.validate()?;
    let expected = SavedBusinessDataTarget {
        public_identity: source.public_key.clone(),
        instance_id: source.instance_id.clone(),
        account_epoch: binding.account_epoch,
    };
    ensure!(
        host.saved_target(&binding.target_id).await?.as_ref() == Some(&expected),
        "transfer enrollment or account epoch changed"
    );
    let principal = host
        .current_principal(&binding.target_id)
        .await?
        .context("transfer account is unavailable")?;
    ensure!(
        principal_digest(&principal)? == binding.principal_sha256,
        "transfer principal changed"
    );
    ensure!(
        host.saved_target(&binding.target_id).await?.as_ref() == Some(&expected),
        "transfer account changed while resolving principal"
    );
    Ok(principal)
}

impl NativePeerJobAdmission for EnrolledPeerJobAdmission {
    fn authorize<'a>(
        &'a self,
        request: &'a DownloadRequest,
        connection: &'a WebRTCRsConnection,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let principal = current_account(self.host.as_ref(), request).await?;
            self.grant_admission.authorize(request, connection).await?;
            let source = request
                .peer_source
                .as_ref()
                .context("peer source required")?;
            let proof = self
                .session
                .peer_identity_proof(connection.clone(), &source.public_key, &source.instance_id)
                .await?;
            ensure!(
                proof.principal.as_ref() == Some(&principal),
                "connection principal differs from transfer account"
            );
            // Reuse the source's actual file-fetch policy even for a cache hit
            // or a completely checkpointed file; a signed principal is not a
            // collection/file grant. This reads no payload bytes.
            let probe = self
                .session
                .file_range(
                    connection.clone(),
                    FileFetchRequest {
                        request_id: String::new(),
                        collection_name: source.collection.clone(),
                        file_id: source.file_id.clone(),
                        range: Some(FileRange {
                            offset: 0,
                            length: 0,
                        }),
                        known_sequences: vec![],
                    },
                )
                .await
                .map_err(|_| anyhow::anyhow!("current peer file permission unavailable"))?;
            ensure!(
                probe.offset == 0 && probe.bytes.is_empty(),
                "invalid file permission probe"
            );
            ensure!(
                current_account(self.host.as_ref(), request).await? == principal,
                "account changed during peer authorization"
            );
            self.grant_admission.authorize(request, connection).await?;
            Ok(())
        })
    }
}

pub(crate) struct NativePeerRangeSource {
    pool: ctox_sync::native::NativePool,
    connection: WebRTCRsConnection,
    request: DownloadRequest,
    admission: Arc<dyn NativePeerJobAdmission>,
}

impl NativePeerRangeSource {
    /// The host resolves a live session for the original saved account. This
    /// adapter independently checks that account and current source file policy.
    /// `grant_admission` must validate the issued grant ID, exact job scope and
    /// current revocation/expiry. Account identity cannot replace that check.
    pub(crate) async fn bind_enrolled(
        session: Arc<NativeSyncSession>,
        connection: WebRTCRsConnection,
        request: DownloadRequest,
        host: Arc<dyn BusinessDataSessionHost>,
        grant_admission: Arc<dyn NativePeerJobAdmission>,
    ) -> Result<Self> {
        let admission = Arc::new(EnrolledPeerJobAdmission {
            host,
            session: session.clone(),
            grant_admission,
        });
        Self::bind(&session, connection, request, admission).await
    }

    /// `request` must come from the host's trusted enrollment and authorized job
    /// admission, never from an unverified remote manifest or signaling response.
    /// Credentials and current file policy remain owned by the existing session.
    pub(crate) async fn bind(
        session: &ctox_sync::native::NativeSyncSession,
        connection: WebRTCRsConnection,
        request: DownloadRequest,
        admission: Arc<dyn NativePeerJobAdmission>,
    ) -> Result<Self> {
        ensure!(
            request.sources.is_empty(),
            "native binding requires a peer-only job"
        );
        let source = request
            .peer_source
            .as_ref()
            .context("peer source required")?;
        session
            .peer_identity_proof(connection.clone(), &source.public_key, &source.instance_id)
            .await?;
        admission.authorize(&request, &connection).await?;
        Ok(Self {
            pool: session.pool().clone(),
            connection,
            request,
            admission,
        })
    }
}

impl PeerRangeSource for NativePeerRangeSource {
    fn authorize<'a>(
        &'a self,
        request: &'a DownloadRequest,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            ensure!(
                request == &self.request,
                "peer job differs from admitted native binding"
            );
            self.admission.authorize(request, &self.connection).await
        })
    }

    fn read_range<'a>(
        &'a self,
        request: &'a DownloadRequest,
        offset: u64,
        length: u64,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + 'a>> {
        Box::pin(async move {
            let source = request
                .peer_source
                .as_ref()
                .context("peer source required")?;
            let result = fetch_authorized_range(
                || self.authorize(request),
                || {
                    fetch_file_range(
                        self.pool.clone(),
                        self.connection.clone(),
                        FileFetchRequest {
                            request_id: String::new(),
                            collection_name: source.collection.clone(),
                            file_id: source.file_id.clone(),
                            range: Some(FileRange { offset, length }),
                            known_sequences: vec![],
                        },
                    )
                },
                Duration::from_millis(250),
            )
            .await?;
            ensure!(result.offset == offset, "peer range offset mismatch");
            Ok(result.bytes)
        })
    }
}
