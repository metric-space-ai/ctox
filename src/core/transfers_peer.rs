//! Native-owned binding between durable transfer jobs and an authenticated Sync
//! connection. A route or persisted request cannot instantiate this binding.
use anyhow::{ensure, Result};
use ctox_transfers::{PeerRangeSource, PeerSource};
use rxdb::plugins::replication_webrtc::{
    file_fetch_client::fetch_file_range,
    file_fetch_handler::{FileFetchRequest, FileRange},
    WebRTCRsConnection,
};
use std::{future::Future, pin::Pin};

pub(crate) struct NativePeerRangeSource {
    pool: ctox_sync::native::NativePool,
    connection: WebRTCRsConnection,
    source: PeerSource,
}

impl NativePeerRangeSource {
    /// `source` must come from the host's trusted enrollment and authorized job
    /// admission, never from an unverified remote manifest or signaling response.
    /// Credentials and current file policy remain owned by the existing session.
    pub(crate) async fn bind(
        session: &ctox_sync::native::NativeSyncSession,
        connection: WebRTCRsConnection,
        source: PeerSource,
    ) -> Result<Self> {
        session
            .peer_identity_proof(connection.clone(), &source.public_key, &source.instance_id)
            .await?;
        Ok(Self {
            pool: session.pool().clone(),
            connection,
            source,
        })
    }
}

impl PeerRangeSource for NativePeerRangeSource {
    fn read_range<'a>(
        &'a self,
        source: &'a PeerSource,
        offset: u64,
        length: u64,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + 'a>> {
        Box::pin(async move {
            ensure!(
                source == &self.source,
                "peer source differs from admitted native binding"
            );
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
