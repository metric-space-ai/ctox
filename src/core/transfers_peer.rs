//! Native-owned binding between durable transfer jobs and an authenticated Sync
//! connection. A route or persisted request cannot instantiate this binding.
use anyhow::{ensure, Context, Result};
use ctox_transfers::{DownloadRequest, PeerRangeSource};
use rxdb::plugins::replication_webrtc::{
    file_fetch_client::fetch_file_range,
    file_fetch_handler::{FileFetchRequest, FileRange},
    WebRTCRsConnection,
};
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

pub(crate) struct NativePeerRangeSource {
    pool: ctox_sync::native::NativePool,
    connection: WebRTCRsConnection,
    request: DownloadRequest,
    admission: Arc<dyn NativePeerJobAdmission>,
}

impl NativePeerRangeSource {
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
