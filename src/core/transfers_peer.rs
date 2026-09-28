//! Native-owned binding between durable transfer jobs and an authenticated Sync
//! connection. A route or persisted request cannot instantiate this binding.
use anyhow::{ensure, Context, Result};
use ctox_sync::{
    business_data_contract::NativeBusinessDataPrincipal,
    business_data_session::{BusinessDataSessionHost, SavedBusinessDataTarget},
    native::NativeSyncSession,
};
use ctox_transfers::{DownloadRequest, PeerAccountBinding, PeerRangeSource, Store, Transfer};
use rxdb::plugins::replication_webrtc::{
    file_fetch_client::fetch_file_range,
    file_fetch_handler::{FileFetchRequest, FileRange},
    WebRTCRsConnection,
};
use sha2::{Digest, Sha256};
use std::{future::Future, pin::Pin, sync::Arc};

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

fn principal_digest(principal: &NativeBusinessDataPrincipal) -> Result<String> {
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
        account_epoch: saved.account_epoch,
        principal_sha256: principal_digest(&principal)?,
    });
    // A changed account during snapshot capture cannot create an admitted job.
    ensure!(
        host.saved_target(target_id).await?.as_ref() == Some(&saved)
            && host.current_principal(target_id).await?.as_ref() == Some(&principal),
        "account changed during transfer admission"
    );
    store.enqueue(request)
}

struct EnrolledPeerJobAdmission {
    host: Arc<dyn BusinessDataSessionHost>,
    session: Arc<NativeSyncSession>,
}

impl EnrolledPeerJobAdmission {
    async fn current(&self, request: &DownloadRequest) -> Result<NativeBusinessDataPrincipal> {
        let source = request
            .peer_source
            .as_ref()
            .context("peer source required")?;
        let binding = source
            .account_binding
            .as_ref()
            .context("original account binding required")?;
        let expected = SavedBusinessDataTarget {
            public_identity: source.public_key.clone(),
            instance_id: source.instance_id.clone(),
            account_epoch: binding.account_epoch,
        };
        ensure!(
            self.host.saved_target(&binding.target_id).await?.as_ref() == Some(&expected),
            "transfer enrollment or account epoch changed"
        );
        let principal = self
            .host
            .current_principal(&binding.target_id)
            .await?
            .context("transfer account is unavailable")?;
        ensure!(
            principal_digest(&principal)? == binding.principal_sha256,
            "transfer principal changed"
        );
        ensure!(
            self.host.saved_target(&binding.target_id).await?.as_ref() == Some(&expected),
            "transfer account changed while resolving principal"
        );
        Ok(principal)
    }
}

impl NativePeerJobAdmission for EnrolledPeerJobAdmission {
    fn authorize<'a>(
        &'a self,
        request: &'a DownloadRequest,
        connection: &'a WebRTCRsConnection,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let principal = self.current(request).await?;
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
                self.current(request).await? == principal,
                "account changed during peer authorization"
            );
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
    pub(crate) async fn bind_enrolled(
        session: Arc<NativeSyncSession>,
        connection: WebRTCRsConnection,
        request: DownloadRequest,
        host: Arc<dyn BusinessDataSessionHost>,
    ) -> Result<Self> {
        let admission = Arc::new(EnrolledPeerJobAdmission {
            host,
            session: session.clone(),
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
            self.authorize(request).await?;
            let source = request
                .peer_source
                .as_ref()
                .context("peer source required")?;
            let result = fetch_file_range(
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
            .await
            .map_err(|_| anyhow::anyhow!("authorized peer range unavailable"))?;
            ensure!(result.offset == offset, "peer range offset mismatch");
            Ok(result.bytes)
        })
    }
}
