//! Native K-WJ-2 files.get responder. The already admitted V4 session owns
//! request routing and response IDs; this adapter owns no peer, socket or store.
//! Wire chunks are 64 KiB (independent of CAS's larger storage/import chunks).
use super::super::super::webrtc_types::WebRTCPublicationGuard;
use super::super::hello::WireSeq;
use super::super::lanes::Lane;
use super::super::scheduler::NativeSendScheduler;
use crate::rx_error::{new_rx_error, RxResult};
use base64::{engine::general_purpose::STANDARD, Engine};
use ctox_store::cas::{Cas, VerifiedBlob};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Seek, SeekFrom},
    sync::Arc,
};

pub const TRANSFER_CHUNK_BYTES: u64 = 64 * 1024;
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferGet {
    #[serde(rename = "type")]
    pub kind: String,
    pub request_id: String,
    #[serde(rename = "ref")]
    pub reference: String,
    #[serde(rename = "kind")]
    pub object_kind: String,
    pub offset: u64,
    pub limit: u64,
}
impl TransferGet {
    fn validate(&self) -> RxResult<()> {
        if self.kind != "transfer_get"
            || !["blob", "artifact"].contains(&self.object_kind.as_str())
            || self.request_id.is_empty()
            || self.request_id.len() > 180
            || self.request_id.chars().any(char::is_control)
            || self.reference.len() != 64
            || !self
                .reference
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self.offset > 2_000_000_000
            || self.offset % TRANSFER_CHUNK_BYTES != 0
            || self.limit != TRANSFER_CHUNK_BYTES
        {
            return Err(error("invalid", "transfer_range"));
        }
        Ok(())
    }
}
pub struct AuthorizedTransferBlob {
    pub blob: VerifiedBlob,
    pub mime: String,
    /// Current files/manifest permission AND original session/device authority.
    /// Re-evaluated before CAS read and for each actual scheduled SCTP frame.
    pub publication: Arc<dyn WebRTCPublicationGuard>,
}
#[async_trait::async_trait]
pub trait TransferBlobAccess: Send + Sync {
    /// Resolve only through the schema owner's real files/manifest read path.
    /// A hash supplied by a peer is NOT permission; no default grant is provided.
    async fn resolve(&self, request: &TransferGet) -> RxResult<AuthorizedTransferBlob>;
}
#[derive(Serialize)]
struct TransferData {
    #[serde(rename = "type")]
    kind: &'static str,
    request_id: String,
    #[serde(rename = "ref")]
    reference: String,
    offset: u64,
    total: u64,
    mime: String,
    data: String,
    sha256: String,
}
pub struct NativeTransferChunk {
    payload: Vec<u8>,
    publication: Arc<dyn WebRTCPublicationGuard>,
}
impl NativeTransferChunk {
    /// The caller supplies the next response application ID from its existing
    /// session framer, never an offset masquerading as a unique message ID.
    pub async fn send(self, scheduler: &NativeSendScheduler, response_id: WireSeq) -> RxResult<()> {
        self.publication.with_current(&mut || Ok(()))?;
        scheduler
            .send(
                Lane::Transfer,
                response_id,
                &self.payload,
                Some(self.publication),
            )
            .await
    }
}
pub struct NativeTransferRouter {
    cas: Cas,
    access: Arc<dyn TransferBlobAccess>,
}
impl NativeTransferRouter {
    pub fn new(cas: Cas, access: Arc<dyn TransferBlobAccess>) -> Self {
        Self { cas, access }
    }
    pub async fn read(&self, request: TransferGet) -> RxResult<NativeTransferChunk> {
        request.validate()?;
        let admitted = self.access.resolve(&request).await?;
        let total = admitted.blob.size();
        if admitted.blob.reference() != request.reference
            || total > 2_000_000_000
            || request.offset > total
            || admitted.mime.len() > 160
            || admitted.mime.chars().any(char::is_control)
        {
            return Err(error("invalid", "transfer_metadata"));
        }
        let cas = self.cas.clone();
        let blob = admitted.blob;
        let offset = request.offset;
        let guard = admitted.publication.clone();
        let data = tokio::task::spawn_blocking(move || {
            let mut bytes = None;
            guard.with_current(&mut || {
                if bytes.is_some() {
                    return Err(error("conflict", "transfer_duplicate_read"));
                }
                // CAS::reader verifies the complete object against the retained
                // receipt before a range is trusted. All disk/hash work is off
                // the async runtime and the canonical writer.
                let mut file = cas
                    .reader(&blob)
                    .map_err(|_| error("invalid", "transfer_object_changed"))?;
                file.seek(SeekFrom::Start(offset))
                    .map_err(|_| error("unavailable", "transfer_read"))?;
                let count = TRANSFER_CHUNK_BYTES.min(total - offset) as usize;
                let mut chunk = vec![0; count];
                file.read_exact(&mut chunk)
                    .map_err(|_| error("unavailable", "transfer_read"))?;
                bytes = Some(chunk);
                Ok(())
            })?;
            bytes.ok_or_else(|| error("forbidden", "transfer_read_not_admitted"))
        })
        .await
        .map_err(|_| error("unavailable", "transfer_read_worker"))??;
        let message = TransferData {
            kind: "transfer_data",
            request_id: request.request_id,
            reference: request.reference,
            offset,
            total,
            mime: admitted.mime,
            sha256: format!("{:x}", Sha256::digest(&data)),
            data: STANDARD.encode(data),
        };
        let payload =
            serde_json::to_vec(&message).map_err(|_| error("invalid", "transfer_encoding"))?;
        Ok(NativeTransferChunk {
            payload,
            publication: admitted.publication,
        })
    }
}
fn error(code: &str, reason: &str) -> crate::rx_error::RxError {
    new_rx_error(
        "V4_TRANSFER",
        Some(serde_json::json!({"code":code,"reason":reason})),
    )
}
