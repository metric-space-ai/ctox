//! Single-file lib requests from the native authoritative artifact manifest.
//! The host checks instance/app read rights and the pinned manifest revision
//! before calling. A manifest is metadata, not a grant; every physical send
//! still requires the current native publication guard. No eager file walk.
use super::{
    Cas, NativeSendScheduler, VerifiedBlob, WebRTCPublicationGuard, artifact_path, failure,
    send_blob,
};
use crate::rx_error::RxResult;
use serde::Deserialize;
use std::{borrow::Cow, collections::BTreeMap, sync::Arc};

/// Projection of the canonical AppManifest (path -> sha256). Other manifest
/// fields belong to the platform owner and are deliberately not reinterpreted.
#[derive(Debug, Deserialize)]
pub struct FileManifest {
    pub app_id: String,
    pub app_ver: String,
    pub files: BTreeMap<String, String>,
}

/// §16.39 pins BOTH major and minor for immutable service-worker caching.
#[derive(Debug)]
pub struct LibraryFileRequest<'a> {
    pub name: &'a str,
    pub version: &'a str,
    pub path: Cow<'a, str>,
}
impl<'a> LibraryFileRequest<'a> {
    pub fn parse(route: &'a str) -> RxResult<Self> {
        let rest = route
            .strip_prefix("/~a/lib:")
            .ok_or_else(|| failure("library artifact prefix"))?;
        let (identity, path) = rest
            .split_once('/')
            .ok_or_else(|| failure("library file missing"))?;
        let (name, version) = identity
            .split_once('@')
            .ok_or_else(|| failure("library version missing"))?;
        if name.is_empty()
            || name == "."
            || name == ".."
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        {
            return Err(failure("invalid library identity"));
        }
        let (major, minor) = version
            .split_once('.')
            .ok_or_else(|| failure("library minor must be pinned"))?;
        let canonical = |s: &str| {
            !s.is_empty()
                && (s == "0" || !s.starts_with('0'))
                && s.bytes().all(|b| b.is_ascii_digit())
                && s.parse::<u64>().is_ok()
        };
        if !canonical(major) || !canonical(minor) {
            return Err(failure("invalid pinned library version"));
        }
        Ok(Self {
            name,
            version,
            path: artifact_path(path)?,
        })
    }

    /// Resolve exactly one path. In particular, unrequested manifest entries
    /// neither open CAS files nor enqueue transfers (large libs stay lazy).
    pub fn reference<'m>(&self, manifest: &'m FileManifest) -> RxResult<&'m str> {
        if manifest.app_id.strip_prefix("lib:") != Some(self.name)
            || manifest.app_ver != self.version
        {
            return Err(failure("library manifest identity/version mismatch"));
        }
        let reference = manifest
            .files
            .get(self.path.as_ref())
            .ok_or_else(|| failure("library file is not in the pinned manifest"))?;
        if reference.len() != 64
            || !reference
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(failure("invalid manifest file sha256"));
        }
        Ok(reference)
    }

    /// The verified blob/size comes from canonical CAS/files metadata; the URL
    /// is never mapped to a disk path. Reuse E14 replay and durable offset resume.
    pub async fn send(
        &self,
        manifest: &FileManifest,
        scheduler: &NativeSendScheduler,
        cas: Cas,
        blob: VerifiedBlob,
        offset: u64,
        publication: Arc<dyn WebRTCPublicationGuard>,
    ) -> RxResult<()> {
        let expected = self.reference(manifest)?;
        if blob.reference() != expected {
            return Err(failure("blob is not the pinned manifest file"));
        }
        send_blob(scheduler, cas, blob, offset, publication).await
    }
}
