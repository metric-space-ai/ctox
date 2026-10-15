//! Native adapter for the service-worker artifact/blob path. Authentication,
//! manifest hash lookup and files-read authorization precede construction. This
//! adapter never resolves an app URL into a caller-selected filesystem path.
use super::super::super::webrtc_types::WebRTCPublicationGuard;
use super::super::hello::WireSeq;
use super::super::lanes::Lane;
use super::{failure, DeliveryToken, NativeSendScheduler};
use crate::rx_error::RxResult;
use ctox_store::cas::{Cas, Receiver, VerifiedBlob, CHUNK_BYTES};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Seek, SeekFrom},
    sync::Arc,
};

const HEADER_LIMIT: usize = 1024;
// This is a logical-message bound, NOT a second physical credit pool. The
// scheduler alone reserves aggregate DataChannel credit for every frame.
const PACKET_LIMIT: usize = CHUNK_BYTES + HEADER_LIMIT + 4;

/// Immutable service-worker artifact address. Resolving `path` to a hash is
/// exclusively the authenticated manifest owner's job; it is never a disk path.
#[derive(Debug, PartialEq, Eq)]
pub struct ArtifactAddress<'a> {
    pub app: &'a str,
    pub app_version: &'a str,
    pub shell_version: &'a str,
    pub path: std::borrow::Cow<'a, str>,
}
impl<'a> ArtifactAddress<'a> {
    pub fn parse(route: &'a str) -> RxResult<Self> {
        let rest = route
            .strip_prefix("/~a/")
            .ok_or_else(|| failure("artifact route prefix"))?;
        let (versions, path) = rest
            .split_once('/')
            .ok_or_else(|| failure("artifact path missing"))?;
        let (app, versions) = versions
            .split_once('@')
            .ok_or_else(|| failure("artifact app version missing"))?;
        let (app_version, shell_version) = versions
            .split_once("~shell@")
            .ok_or_else(|| failure("artifact shell version missing"))?;
        let identifier = |value: &str| {
            !value.is_empty()
                && value != "."
                && value != ".."
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        };
        if !identifier(app) || !identifier(app_version) || !identifier(shell_version) {
            return Err(failure("invalid immutable artifact address"));
        }
        let path = if path.contains('%') {
            let mut decoded = Vec::with_capacity(path.len());
            let mut raw = path.as_bytes().iter().copied();
            while let Some(byte) = raw.next() {
                if byte == b'%' {
                    let hi = raw
                        .next()
                        .and_then(|b| (b as char).to_digit(16))
                        .ok_or_else(|| failure("artifact percent escape"))?;
                    let lo = raw
                        .next()
                        .and_then(|b| (b as char).to_digit(16))
                        .ok_or_else(|| failure("artifact percent escape"))?;
                    decoded.push((hi * 16 + lo) as u8);
                } else {
                    decoded.push(byte);
                }
            }
            std::borrow::Cow::Owned(
                String::from_utf8(decoded).map_err(|_| failure("artifact UTF-8 path"))?,
            )
        } else {
            std::borrow::Cow::Borrowed(path)
        };
        if path.len() > 4096
            || path.chars().any(|c| c.is_control() || c == '\\')
            || path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(failure("invalid artifact path"));
        }
        Ok(Self {
            app,
            app_version,
            shell_version,
            path,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "ref")]
    reference: String,
    offset: WireSeq,
    size: WireSeq,
    done: bool,
}
fn packet(reference: &str, size: u64, offset: u64, data: &[u8]) -> RxResult<Vec<u8>> {
    let header = serde_json::to_vec(&Header {
        kind: "artifact_chunk".into(),
        reference: reference.into(),
        offset: WireSeq(offset),
        size: WireSeq(size),
        done: offset + data.len() as u64 == size,
    })
    .map_err(|e| failure(&e.to_string()))?;
    if header.len() > HEADER_LIMIT || data.len() > CHUNK_BYTES {
        return Err(failure("artifact chunk bounds"));
    }
    let mut bytes = Vec::with_capacity(4 + header.len() + data.len());
    bytes.extend_from_slice(&(header.len() as u32).to_be_bytes());
    bytes.extend(header);
    bytes.extend(data);
    if bytes.len() > PACKET_LIMIT {
        return Err(failure("artifact logical packet limit"));
    }
    Ok(bytes)
}
fn decode(bytes: &[u8]) -> RxResult<(Header, &[u8])> {
    if bytes.len() < 4 || bytes.len() > PACKET_LIMIT {
        return Err(failure("artifact packet bounds"));
    }
    let n = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize;
    if n > HEADER_LIMIT || n > bytes.len() - 4 {
        return Err(failure("artifact header bounds"));
    }
    let header: Header =
        serde_json::from_slice(&bytes[4..4 + n]).map_err(|e| failure(&e.to_string()))?;
    let data = &bytes[4 + n..];
    if header.kind != "artifact_chunk"
        || (!header.done && data.is_empty())
        || data.len() > CHUNK_BYTES
        || header
            .offset
            .0
            .checked_add(data.len() as u64)
            .is_none_or(|end| end > header.size.0 || header.done != (end == header.size.0))
    {
        return Err(failure("invalid artifact chunk"));
    }
    Ok((header, data))
}

/// Resume offset is the receiver's durable prefix, obtained on the authorized
/// same-epoch route. Across session reset query the prefix again; never guess it
/// from SCTP handoffs. E14 replay handles unacknowledged chunks within a session.
/// Success is bounded retained admission; final delivery is the receiver's
/// verified CAS completion. Association close interrupts admission, retaining
/// already accepted messages for rebind rather than leaving this producer stuck.
pub async fn send_blob(
    scheduler: &NativeSendScheduler,
    cas: Cas,
    blob: VerifiedBlob,
    offset: u64,
    publication: Arc<dyn WebRTCPublicationGuard>,
) -> RxResult<()> {
    let mut draining = scheduler.drain.subscribe();
    let mut status = scheduler.status.clone();
    if *draining.borrow() || status.borrow().is_some() {
        return Err(failure("artifact association closed"));
    }
    tokio::select! {
        biased;
        _ = draining.changed() => Err(failure("artifact association draining; resume required")),
        _ = status.changed() => Err(failure("artifact association ended; resume required")),
        result = async {
    if scheduler.session.is_none() { return Err(failure("artifact delivery requires E14 session")); }
    if offset > blob.size() { return Err(failure("artifact resume beyond size")); }
    let reference = blob.reference().to_owned(); let size = blob.size();
    let mut file = tokio::task::spawn_blocking(move || cas.reader(&blob)).await
        .map_err(|e| failure(&e.to_string()))?.map_err(|e| failure(&e.to_string()))?;
    // Each file operation is off the async executor and writer thread. Admission
    // awaits only Transfer's retained window, never a control/command permit.
    let mut cursor = offset;
    loop {
        let (returned, data) = tokio::task::spawn_blocking(move || -> std::io::Result<_> {
            file.seek(SeekFrom::Start(cursor))?;
            let mut bytes = vec![0; CHUNK_BYTES.min((size - cursor) as usize)];
            file.read_exact(&mut bytes)?; Ok((file, bytes))
        }).await.map_err(|e| failure(&e.to_string()))?.map_err(|e| failure(&e.to_string()))?;
        file = returned;
        let payload = packet(&reference, size, cursor, &data)?;
        scheduler.send(Lane::Transfer, WireSeq(cursor), &payload, Some(publication.clone())).await?;
        cursor += data.len() as u64;
        if cursor == size { return Ok(()); }
    }
        } => result,
    }
}

/// One instance per authorized transfer. The peer/session owner serializes it
/// across rebind. Offset ACK follows sync_data; completion ACK follows full hash
/// verification and atomic CAS install. files metadata is published separately
/// with VerifiedBlob::commit_with_file in the primitive's transaction.
pub struct ArtifactReceiver {
    receiver: Option<Receiver>,
    reference: String,
    size: u64,
    completed: Option<VerifiedBlob>,
}
impl ArtifactReceiver {
    pub fn open(cas: &Cas, scope: &str, reference: &str, size: u64) -> RxResult<Self> {
        let receiver = cas
            .receive(scope, reference, size)
            .map_err(|e| failure(&e.to_string()))?;
        Ok(Self {
            receiver: Some(receiver),
            reference: reference.into(),
            size,
            completed: None,
        })
    }
    pub fn offset(&self) -> RxResult<u64> {
        if self.completed.is_some() {
            return Ok(self.size);
        }
        self.receiver
            .as_ref()
            .ok_or_else(|| failure("artifact receiver closed"))?
            .offset()
            .map_err(|e| failure(&e.to_string()))
    }
    fn apply(&mut self, bytes: &[u8]) -> RxResult<Option<VerifiedBlob>> {
        let (header, data) = decode(bytes)?;
        if header.reference != self.reference || header.size.0 != self.size {
            return Err(failure("artifact identity mismatch"));
        }
        if let Some(blob) = &self.completed {
            return Ok(Some(blob.clone()));
        }
        let receiver = self
            .receiver
            .as_mut()
            .ok_or_else(|| failure("artifact receiver closed"))?;
        if !data.is_empty() {
            receiver
                .append(header.offset.0, data)
                .map_err(|e| failure(&e.to_string()))?;
        }
        if header.done {
            let blob = self
                .receiver
                .take()
                .unwrap()
                .finish()
                .map_err(|e| failure(&e.to_string()))?;
            self.completed = Some(blob.clone());
            return Ok(Some(blob));
        }
        Ok(None)
    }
    /// Caller owns this receiver between calls. Synchronous file IO is performed
    /// on a blocking worker, never inside the scheduler's stream-poll loop.
    pub async fn deliver(
        mut self,
        bytes: Vec<u8>,
        delivery: DeliveryToken,
    ) -> RxResult<(Self, Option<VerifiedBlob>)> {
        let (this, blob) = tokio::task::spawn_blocking(move || {
            let blob = self.apply(&bytes)?;
            Ok::<_, crate::rx_error::RxError>((self, blob))
        })
        .await
        .map_err(|e| failure(&e.to_string()))??;
        delivery.acknowledge()?;
        Ok((this, blob))
    }
}

#[cfg(test)]
#[path = "lane_artifacts_tests.rs"]
mod tests;
