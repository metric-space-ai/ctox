//! Files cross the app boundary only as transfer-CAS references.
use super::*;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

pub const MAX_BLOB_BYTES: u64 = 64 * 1024 * 1024;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlobRef {
    pub sha256: String,
    pub size: u64,
    pub transfer_id: String,
}
impl BlobRef {
    pub fn validate(&self) -> Result<(), CtoxError> {
        if self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self.size > MAX_BLOB_BYTES
        {
            return Err(invalid("$.blob"));
        }
        crate::app_capabilities::protocol::identifier(&self.transfer_id, "$.blob.transfer_id")
    }
}
/// Core-owned authority. Every read/publication must check the current caller,
/// transfer grant, executor fence and app scope at the actual effect point.
/// No constructor from wire claims and no permissive production implementation.
pub trait BlobAccess {
    fn read(&self, ctx: &CapCallContext, reference: &BlobRef) -> Result<Vec<u8>, CtoxError>;
    fn publish(&self, ctx: &CapCallContext, bytes: &[u8], mime: &str)
        -> Result<BlobRef, CtoxError>;
}
pub fn read(
    blobs: &dyn BlobAccess,
    ctx: &CapCallContext,
    reference: &BlobRef,
) -> Result<Vec<u8>, CtoxError> {
    reference.validate()?;
    let bytes = blobs.read(ctx, reference)?;
    if bytes.len() as u64 != reference.size
        || format!("{:x}", Sha256::digest(&bytes)) != reference.sha256
    {
        return Err(CtoxError::new(
            ErrorCode::Conflict,
            "blob content differs from CAS reference",
        ));
    }
    Ok(bytes)
}
pub fn publish(
    blobs: &dyn BlobAccess,
    ctx: &CapCallContext,
    bytes: &[u8],
    mime: &str,
) -> Result<BlobRef, CtoxError> {
    if bytes.len() as u64 > MAX_BLOB_BYTES {
        return Err(CtoxError::new(
            ErrorCode::Quota,
            "blob exceeds capability limit",
        ));
    }
    let reference = blobs.publish(ctx, bytes, mime)?;
    reference.validate()?;
    if reference.size != bytes.len() as u64
        || reference.sha256 != format!("{:x}", Sha256::digest(bytes))
    {
        return Err(CtoxError::uncertain());
    }
    Ok(reference)
}
/// Repoint to the existing transfer CAS reader. The supplied Core authorization
/// closure must bind ctx to the stored job's original account/grant, before AND
/// after the read. A hash by itself is not a read grant.
pub fn read_completed_transfer(
    store: &ctox_transfers::Store,
    ctx: &CapCallContext,
    reference: &BlobRef,
    authorize: &dyn Fn(&CapCallContext, &ctox_transfers::DownloadRequest) -> Result<(), CtoxError>,
) -> Result<Vec<u8>, CtoxError> {
    reference.validate()?;
    let saved = store.get(&reference.transfer_id).map_err(native_error)?;
    if saved.request.sha256 != reference.sha256 || saved.request.size != reference.size {
        return Err(CtoxError::new(
            ErrorCode::Conflict,
            "transfer reference mismatch",
        ));
    }
    authorize(ctx, &saved.request)?;
    let result = store
        .read_completed_peer_artifact(&saved.request, |file| {
            let mut bytes = Vec::new();
            file.take(MAX_BLOB_BYTES + 1).read_to_end(&mut bytes)?;
            Ok(bytes)
        })
        .map_err(native_error)?;
    authorize(ctx, &saved.request)?;
    Ok(result)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BlobRequest {
    #[serde(alias = "blob_id")]
    blob: BlobRef,
}
pub(super) fn execute(
    blobs: &dyn BlobAccess,
    ctx: &CapCallContext,
    args: &Value,
) -> Result<Value, CtoxError> {
    let request: BlobRequest = decode(args)?;
    read(blobs, ctx, &request.blob)?;
    Ok(json!({"blob_id":request.blob,"sha256":request.blob.sha256}))
}
/// Native libraries needing a Path receive an owned, scoped scratch copy, never
/// an app-supplied filename. The file is removed on every return/unwind.
pub(super) fn with_file<T>(
    root: &Path,
    blobs: &dyn BlobAccess,
    ctx: &CapCallContext,
    reference: &BlobRef,
    consume: impl FnOnce(&Path) -> Result<T, CtoxError>,
) -> Result<T, CtoxError> {
    let bytes = read(blobs, ctx, reference)?;
    let directory = root.join("runtime/capability-scratch");
    std::fs::create_dir_all(&directory).map_err(native_error)?;
    let mut file = tempfile::Builder::new()
        .prefix("cas-")
        .tempfile_in(directory)
        .map_err(native_error)?;
    file.write_all(&bytes).map_err(native_error)?;
    file.flush().map_err(native_error)?;
    consume(file.path())
}
