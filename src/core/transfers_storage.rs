//! Local operator admission and native registry fencing for storage transfers.
use crate::business_os::computer_capabilities::{ComputerCapability, StoragePurpose};
use crate::business_os::computer_endpoints::{
    self as endpoints, ComputerEndpoint, ComputerEndpointRequest, EndpointUse,
};
use anyhow::{ensure, Context, Result};
use ctox_transfers::{
    storage_smb, storage_ssh, DownloadRequest, StorageConnection, StorageDirection,
    StorageResolver, StorageTransfer, Store,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

fn endpoint_request(storage: &StorageTransfer) -> Result<ComputerEndpointRequest> {
    storage.validate()?;
    let purpose = match storage.purpose.as_str() {
        "artifacts" => StoragePurpose::Artifacts,
        "backups" => StoragePurpose::Backups,
        "exchange" => StoragePurpose::Exchange,
        _ => anyhow::bail!("invalid storage purpose"),
    };
    Ok(ComputerEndpointRequest {
        owner_user_id: storage.owner_user_id.clone(),
        computer_id: storage.computer_id.clone(),
        endpoint_ref: storage.endpoint_ref.clone(),
        usage: EndpointUse::Storage { purpose },
    })
}
fn fingerprint(storage: &StorageTransfer) -> String {
    format!("sha256:{}", storage.endpoint_fingerprint)
}

pub(crate) fn resolver(root: &Path) -> Arc<dyn StorageResolver> {
    Arc::new(NativeStorage { root: root.into() })
}
struct NativeStorage {
    root: PathBuf,
}
impl StorageResolver for NativeStorage {
    fn authorize(&self, request: &DownloadRequest) -> Result<()> {
        let storage = request
            .storage
            .as_ref()
            .context("storage request required")?;
        endpoints::with_current_computer_endpoint(
            &self.root,
            &endpoint_request(storage)?,
            &fingerprint(storage),
            |resolved, _| {
                let ComputerCapability::Storage(grant) = &resolved.grant else {
                    anyhow::bail!("storage grant required")
                };
                if let Some(quota) = grant.quota_gib {
                    ensure!(
                        request.size <= quota.saturating_mul(1024 * 1024 * 1024),
                        "artifact exceeds configured storage quota"
                    );
                }
                Ok(())
            },
        )
    }
    fn connect(&self, request: &DownloadRequest) -> Result<Box<dyn StorageConnection>> {
        let storage = request
            .storage
            .as_ref()
            .context("storage request required")?;
        let authority = endpoint_request(storage)?;
        let frozen = fingerprint(storage);
        let inner = endpoints::with_current_computer_endpoint(
            &self.root,
            &authority,
            &frozen,
            |resolved, secrets| {
                let ComputerCapability::Storage(grant) = &resolved.grant else {
                    anyhow::bail!("storage grant required")
                };
                // Borrow credentials only for synchronous library authentication.
                // Every later operation reenters the same registry/secret fence.
                let credential = |index: usize| -> Result<&str> {
                    Ok(std::str::from_utf8(
                        secrets.get(index).context("credential missing")?,
                    )?)
                };
                match &resolved.connection {
                    ComputerEndpoint::Ssh {
                        host,
                        port,
                        username,
                        host_key_sha256,
                        passphrase,
                        ..
                    } => storage_ssh::connect(storage_ssh::SshStorageOptions {
                        host: host.clone(),
                        port: *port,
                        username: username.clone(),
                        root: grant.root.clone(),
                        host_key_sha256: host_key_sha256.clone(),
                        private_key: credential(0)?,
                        passphrase: if passphrase.is_some() {
                            Some(credential(1)?)
                        } else {
                            None
                        },
                    }),
                    ComputerEndpoint::Smb {
                        host,
                        port,
                        username,
                        share,
                        ..
                    } => storage_smb::connect(storage_smb::SmbStorageOptions {
                        host: host.clone(),
                        port: *port,
                        username: username.clone(),
                        root: grant.root.clone(),
                        share: share.clone(),
                        password: credential(0)?,
                    }),
                }
            },
        )?;
        Ok(Box::new(FencedStorage {
            inner,
            root: self.root.clone(),
            authority,
            fingerprint: frozen,
        }))
    }
}
struct FencedStorage {
    inner: Box<dyn StorageConnection>,
    root: PathBuf,
    authority: ComputerEndpointRequest,
    fingerprint: String,
}
impl FencedStorage {
    fn operation<T>(
        &mut self,
        apply: impl FnOnce(&mut dyn StorageConnection) -> Result<T>,
    ) -> Result<T> {
        endpoints::with_current_computer_endpoint(
            &self.root,
            &self.authority,
            &self.fingerprint,
            |_, _| apply(self.inner.as_mut()),
        )
    }
}
impl StorageConnection for FencedStorage {
    fn length(&mut self, path: &str) -> Result<Option<u64>> {
        self.operation(|c| c.length(path))
    }
    fn read(&mut self, path: &str, offset: u64, length: usize) -> Result<Vec<u8>> {
        self.operation(|c| c.read(path, offset, length))
    }
    fn create(&mut self, path: &str) -> Result<()> {
        self.operation(|c| c.create(path))
    }
    fn truncate(&mut self, path: &str, length: u64) -> Result<()> {
        self.operation(|c| c.truncate(path, length))
    }
    fn write(&mut self, path: &str, offset: u64, bytes: &[u8]) -> Result<()> {
        self.operation(|c| c.write(path, offset, bytes))
    }
    fn publish(&mut self, staging: &str, destination: &str) -> Result<()> {
        self.operation(|c| c.publish(staging, destination))
    }
    // Revocation must never prevent retiring an already owned connection.
    fn close(&mut self) -> Result<()> {
        self.inner.close()
    }
}

/// Trusted local operator interface, not a browser or remote command handler.
/// Remote callers must obtain owner identity from their verified native session.
pub(crate) fn enqueue(
    root: &Path,
    store: &Store,
    args: &[String],
    direction: StorageDirection,
) -> Result<ctox_transfers::Transfer> {
    let mut storage = StorageTransfer {
        owner_user_id: args[2].clone(),
        computer_id: args[3].clone(),
        endpoint_ref: args[4].clone(),
        purpose: args[5].clone(),
        relative_path: args[8].clone(),
        direction,
        endpoint_fingerprint: "0".repeat(64),
    };
    let resolved = endpoints::resolve_computer_endpoint(root, &endpoint_request(&storage)?)?;
    storage.endpoint_fingerprint = resolved
        .fingerprint
        .strip_prefix("sha256:")
        .context("invalid endpoint fingerprint")?
        .into();
    let request = DownloadRequest {
        id: args[1].clone(),
        sha256: args[6].clone(),
        size: args[7].parse()?,
        sources: vec![],
        peer_source: None,
        storage: Some(storage),
    };
    let authority = NativeStorage { root: root.into() };
    authority.authorize(&request)?;
    if direction == StorageDirection::Upload {
        store.stage_storage_upload(Path::new(&args[9]), &request.sha256, request.size)?;
    }
    authority.authorize(&request)?;
    store.enqueue(request)
}
