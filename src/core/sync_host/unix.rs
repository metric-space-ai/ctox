use anyhow::{Context, Result};
use base64::Engine;
use ctox_sync::{
    authority::auth::SigningIdentity,
    host_config::{self, HostConfiguration},
    host_runtime::HostStarted,
    host_transport::HostTransport,
    local_host::HostDirectoryLock,
};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Read, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{mpsc, Arc},
    thread,
    time::Duration,
};

#[path = "guests.rs"]
pub(crate) mod guests;
#[path = "runtime.rs"]
mod runtime;
const SECRET_SCOPE: &str = super::SIGNING_IDENTITY_SECRET_KEY.0;
const IDENTITY_SECRET: &str = super::SIGNING_IDENTITY_SECRET_KEY.1;
const INPUT_LIMIT: u64 = 1024 * 1024;

fn directory(root: &Path) -> PathBuf {
    root.join("runtime").join("ctox-sync")
}
fn load_config(root: &Path) -> Result<Option<HostConfiguration>> {
    let path = crate::inference::runtime_env::runtime_config_path(root);
    if !path.exists() {
        return Ok(None);
    }
    let connection =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    connection.busy_timeout(Duration::from_secs(2))?;
    Ok(host_config::load(&connection)?)
}
pub(super) fn configuration(root: &Path) -> Result<HostConfiguration> {
    load_config(root)?.context("native Sync host is not configured")
}
pub(super) fn decode_key(encoded: &[u8]) -> Result<SigningIdentity> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct StoredKey {
        identity: String,
        pkcs8: String,
    }
    let record: StoredKey = serde_json::from_slice(encoded)
        .map_err(|_| anyhow::anyhow!("invalid native Sync key record"))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(record.pkcs8)
        .context("invalid native Sync key encoding")?;
    SigningIdentity::from_existing_pkcs8(&bytes, &record.identity)
        .context("invalid native Sync signing key")
}

pub(super) fn key(root: &Path) -> Result<Arc<SigningIdentity>> {
    let encoded = crate::secrets::read_secret_value(root, SECRET_SCOPE, IDENTITY_SECRET)
        .map_err(|_| anyhow::anyhow!("native Sync identity is unavailable in the secret store"))?;
    Ok(Arc::new(decode_key(encoded.as_bytes())?))
}

/// Borrow the freshly verified provisioned key under the existing encrypted
/// secret mutation fence. Enter before policy/worker locks; never await or
/// reenter secret APIs in the callback. This path cannot provision a key/store.
pub(super) fn with_current_key<T>(
    root: &Path,
    apply: impl FnOnce(&SigningIdentity) -> Result<T>,
) -> Result<T> {
    crate::secrets::with_current_secret_value(root, SECRET_SCOPE, IDENTITY_SECRET, |encoded| {
        let identity = decode_key(encoded)?;
        apply(&identity)
    })
}
fn transport_name(config: &HostConfiguration) -> String {
    format!("transport:{}", config.scope_id)
}
fn transport(root: &Path, config: &HostConfiguration) -> Result<HostTransport> {
    let value = crate::secrets::read_secret_value(root, SECRET_SCOPE, &transport_name(config))
        .map_err(|_| anyhow::anyhow!("native Sync transport credentials are unavailable"))?;
    Ok(HostTransport::parse(&value, config)?)
}
fn input() -> Result<String> {
    let mut value = String::new();
    io::stdin()
        .take(INPUT_LIMIT + 1)
        .read_to_string(&mut value)
        .context("cannot read native Sync input")?;
    anyhow::ensure!(
        value.len() as u64 <= INPUT_LIMIT,
        "native Sync input exceeds its size limit"
    );
    Ok(value)
}
fn print(value: impl Serialize) -> Result<()> {
    println!("{}", serde_json::to_string(&value)?);
    Ok(())
}

fn initialize(root: &Path, imported: Option<(Vec<u8>, SigningIdentity)>) -> Result<()> {
    let _lease = HostDirectoryLock::acquire(&directory(root))?;
    if crate::secrets::secret_exists(root, SECRET_SCOPE, IDENTITY_SECRET)? {
        let existing = key(root)?;
        if let Some((_, candidate)) = imported {
            anyhow::ensure!(
                existing.public_identity() == candidate.public_identity(),
                "native Sync identity is already pinned to a different key"
            );
        }
        if let Some(config) = load_config(root)? {
            config.validate_key(&existing)?;
        }
        return print(serde_json::json!({"identity": existing.public_identity()}));
    }
    let (bytes, identity) = match imported {
        Some(imported) => imported,
        None => {
            let bytes = SigningIdentity::generate_pkcs8()?;
            let key = SigningIdentity::from_pkcs8(&bytes)?;
            (bytes, key)
        }
    };
    if let Some(config) = load_config(root)? {
        config.validate_key(&identity)?;
    }
    crate::secrets::write_secret_record(
        root,
        SECRET_SCOPE,
        IDENTITY_SECRET,
        &serde_json::to_string(
            &serde_json::json!({"identity": identity.public_identity(), "pkcs8": base64::engine::general_purpose::STANDARD.encode(bytes)}),
        )?,
        Some("Native CTOX Sync identity".into()),
        serde_json::json!({"source":"native-sync-host"}),
    )?;
    print(serde_json::json!({"identity": identity.public_identity()}))
}

pub fn handle_command(root: &Path, args: &[String]) -> Result<()> {
    let root = std::fs::canonicalize(root)?;
    // --root is resolved by the canonical CLI dispatcher before this adapter.
    let mut words = Vec::new();
    let mut arguments = args.iter();
    while let Some(argument) = arguments.next() {
        if argument == "--root" {
            arguments.next().context("missing --root value")?;
        } else {
            words.push(argument.as_str());
        }
    }
    match words.as_slice() {
        ["init"] => initialize(&root, None),
        ["import-key", expected] => {
            let bytes = base64::engine::general_purpose::STANDARD.decode(input()?.trim()).context("invalid native Sync key encoding")?;
            let identity = SigningIdentity::from_existing_pkcs8(&bytes, expected)?;
            initialize(&root, Some((bytes, identity)))
        },
        ["identity"] => print(serde_json::json!({"identity": key(&root)?.public_identity()})),
        ["configure"] => {
            let config: HostConfiguration = serde_json::from_str(&input()?).context("invalid native Sync public configuration")?;
            let _lease = HostDirectoryLock::acquire(&directory(&root))?;
            config.validate_key(key(&root)?.as_ref())?;
            let mut connection = rusqlite::Connection::open(crate::inference::runtime_env::runtime_config_path(&root))?;
            connection.busy_timeout(Duration::from_secs(2))?;
            host_config::save(&mut connection, &config)?;
            print(serde_json::json!({"configured": true, "nodeId": config.node_id(), "scopeId": config.scope_id, "activation": "next-host-start"}))
        },
        ["transport"] => {
            let config = configuration(&root)?;
            config.validate_key(key(&root)?.as_ref())?;
            let value = HostTransport::parse(&input()?, &config)?;
            crate::secrets::write_secret_record(&root, SECRET_SCOPE, &transport_name(&config), &serde_json::to_string(&value)?, Some("Native CTOX Sync transport".into()), serde_json::json!({"source":"native-sync-host"}))?;
            print(serde_json::json!({"stored": true, "activation": "signaling-on-next-reconnect-ice-on-next-start"}))
        },
        ["handoff-enroll-source"] => {
            let _lease = HostDirectoryLock::acquire(&directory(&root))?;
            let config = configuration(&root)?;
            let enrollment = input()?;
            with_current_key(&root, |identity| {
                print(crate::business_os::session_handoff_enrollment::enroll_source(
                    &root, &config, identity, &enrollment,
                )?)
            })
        },
        ["handoff-reauthorize-source", binding] => {
            let _lease = HostDirectoryLock::acquire(&directory(&root))?;
            let config = configuration(&root)?;
            super::with_current_signing_identity(&root, |identity| {
                print(serde_json::to_value(crate::business_os::session_handoff_enrollment::reauthorize_source(
                    &root, &config, identity, binding,
                )?)?)
            })
        },
        ["handoff-target-challenge"] => {
            let _lease = HostDirectoryLock::acquire(&directory(&root))?;
            let config = configuration(&root)?;
            with_current_key(&root, |identity| {
                print(serde_json::json!({"challenge":crate::business_os::session_handoff_enrollment::target::challenge(
                    &root,&config,identity)?}))
            })
        },
        ["handoff-source-offer", binding, challenge] => {
            let _lease = HostDirectoryLock::acquire(&directory(&root))?;
            print(crate::business_os::native_source_offer(&root,binding,challenge)?)
        },
        ["handoff-configure-target-repository"] => {
            let _lease = HostDirectoryLock::acquire(&directory(&root))?;
            configuration(&root)?.validate_key(key(&root)?.as_ref())?;
            crate::business_os::configure_handoff_target_repository(&root,&input()?)?;
            print(serde_json::json!({"configured":true}))
        },
        ["handoff-enroll-target"] => {
            let _lease = HostDirectoryLock::acquire(&directory(&root))?;
            let config = configuration(&root)?;
            let enrollment = input()?;
            with_current_key(&root, |identity| {
                print(crate::business_os::session_handoff_enrollment::target::enroll(&root,&config,identity,&enrollment)?)
            })
        },
        ["handoff-copy", binding, route] => checkpoint_copy(&root, binding, route, false, ""),
        ["handoff-reconstruct", binding] => checkpoint_copy(&root, binding, "", true, ""),
        ["handoff-import-guest", binding, guest] => checkpoint_copy(&root, binding, "", false, guest),
        ["handoff-revoke", binding] => {
            let revoked = crate::business_os::session_handoff_enrollment::revoke_binding(&root, binding)?;
            print(serde_json::json!({"revoked": revoked}))
        },
        ["status"] => runtime::status(&root),
        ["configure-guests"] => {
            guests::configure(&root, &input()?)?;
            print(serde_json::json!({"configured": true, "activation": "next-host-start"}))
        },
        ["revoke-guest-provider", owner, profile] => {
            crate::business_os::revoke_provider_assignment(&root, owner, profile)?;
            print(serde_json::json!({"revoked": true}))
        },
        ["guest-enroll", project, thread, profile] => guests::enroll(&root, &[project, thread, profile], &input()?),
        ["revoke-guest-workspace", owner, profile, project] => {
            crate::business_os::revoke_workspace_assignment(&root, owner, profile, project)?;
            print(serde_json::json!({"revoked": true}))
        },
        ["run"] => runtime::run(&root, async {
            let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
            tokio::select! { result = tokio::signal::ctrl_c() => result, _ = terminate.recv() => Ok(()) }
        }, |started, _authority, _guests, _control| print(serde_json::json!({"listener":"active", "nodeId":started.node_id, "scopeId":started.scope_id, "ipcEndpoint":started.ipc_endpoint}))),
        _ => anyhow::bail!("usage: ctox sync init | identity | import-key <public-identity> (key on stdin) | configure (public JSON on stdin) | transport (secret JSON on stdin) | handoff-enroll-source (public JSON on stdin) | handoff-target-challenge | handoff-source-offer <binding> <challenge> | handoff-configure-target-repository (public JSON on stdin) | handoff-enroll-target (public JSON on stdin) | handoff-copy <binding-digest> <source-route> | handoff-reconstruct <binding-digest> | handoff-import-guest <binding-digest> <guest-id> | handoff-revoke <binding> | handoff-reauthorize-source <binding> | configure-guests (public JSON on stdin) | revoke-guest-provider <owner> <profile> | revoke-guest-workspace <owner> <profile> <project> | guest-enroll <project> <thread> <profile> (opaque session on stdin) | status | run"),
    }
}

fn checkpoint_copy(
    root: &Path,
    binding: &str,
    route: &str,
    reconstruct: bool,
    guest_id: &str,
) -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let config = configuration(root)?;
    let descriptor: Descriptor = serde_json::from_reader(
        std::fs::File::open(directory(root).join("listener.json"))?.take(16384),
    )?;
    anyhow::ensure!(
        descriptor.version == 1
            && descriptor.node_id == config.node_id()
            && descriptor.scope_id == config.scope_id,
        "native checkpoint host mismatch"
    );
    let endpoint = descriptor
        .checkpoint_endpoint
        .context("native checkpoint receiver unavailable")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let response: crate::business_os::NativeCheckpointCopyResponse = runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(65), async {
            let mut stream = tokio::net::UnixStream::connect(endpoint).await?;
            anyhow::ensure!(
                stream.peer_cred()?.uid() == unsafe { libc::geteuid() },
                "foreign native checkpoint host"
            );
            let bytes = serde_json::to_vec(&crate::business_os::NativeCheckpointCopyRequest {
                binding_digest: binding.into(),
                source_route: route.into(),
                reconstruct,
                guest_id: guest_id.into(),
            })?;
            anyhow::ensure!(bytes.len() <= 2048, "checkpoint control request too large");
            stream.write_u32(bytes.len() as u32).await?;
            stream.write_all(&bytes).await?;
            let n = stream.read_u32().await? as usize;
            anyhow::ensure!(n > 0 && n <= 2048, "invalid checkpoint response");
            let mut bytes = vec![0; n];
            stream.read_exact(&mut bytes).await?;
            Ok::<_, anyhow::Error>(serde_json::from_slice(&bytes)?)
        })
        .await?
    })?;
    match response {
        crate::business_os::NativeCheckpointCopyResponse::Reconstructed {
            checkpoint_digest,
            preparation_id,
        } if reconstruct && guest_id.is_empty() => print(
            serde_json::json!({"reconstructed":true,"checkpointDigest":checkpoint_digest,"preparationId":preparation_id,"resumed":false}),
        ),
        crate::business_os::NativeCheckpointCopyResponse::Copied { checkpoint_digest }
            if !reconstruct && guest_id.is_empty() =>
        {
            print(
                serde_json::json!({"copied":true,"checkpointDigest":checkpoint_digest,"resumed":false}),
            )
        }
        crate::business_os::NativeCheckpointCopyResponse::GuestImported {
            checkpoint_digest,
            guest_id: imported_guest,
            controller_id,
            controller_generation,
            effect_id,
        } if imported_guest == guest_id && !guest_id.is_empty() => print(
            serde_json::json!({"imported":true,"checkpointDigest":checkpoint_digest,
                "guestId":imported_guest,"controllerId":controller_id,
                "controllerGeneration":controller_generation,"effectId":effect_id,"resumed":false}),
        ),
        crate::business_os::NativeCheckpointCopyResponse::Denied => {
            anyhow::bail!("native checkpoint copy denied or interrupted")
        }
        _ => anyhow::bail!("native checkpoint response operation differs"),
    }
}

pub struct ServiceHost {
    authority: Arc<dyn ctox_sync::authority::client::ExecutionAuthority>,
    guest_registry: Option<Arc<crate::business_os::NativeGuestRegistry>>,
    control_channel: super::NativeControlChannel,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<thread::JoinHandle<()>>,
}
impl ServiceHost {
    pub(crate) fn native_control_channel(&self) -> super::NativeControlChannel {
        self.control_channel.clone()
    }
    /// Borrow only the registry attached to this live host's native peer.
    pub(crate) fn guest_registry(&self) -> Option<Arc<crate::business_os::NativeGuestRegistry>> {
        self.guest_registry.clone()
    }
    /// The host remains the lifecycle owner; cloning this handle cannot keep
    /// a stopped listener/discovery or revoked quorum owner authorized.
    pub(crate) fn execution_authority(
        &self,
    ) -> Arc<dyn ctox_sync::authority::client::ExecutionAuthority> {
        self.authority.clone()
    }
}
impl Drop for ServiceHost {
    fn drop(&mut self) {
        self.control_channel.retire();
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}
pub fn start_if_configured(root: &Path) -> Result<Option<ServiceHost>> {
    if load_config(root)?.is_none() {
        return Ok(None);
    }
    let root = std::fs::canonicalize(root)?;
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let (ready, started) = mpsc::channel();
    let failed = ready.clone();
    let task = thread::Builder::new()
        .name("ctox-sync-host".into())
        .spawn(move || {
            let result = runtime::run(
                &root,
                async {
                    let _ = stopped.await;
                    Ok(())
                },
                move |_, authority, guest_registry, control_channel| {
                    ready
                        .send(Ok((authority, guest_registry, control_channel)))
                        .map_err(|_| anyhow::anyhow!("native Sync service startup receiver closed"))
                },
            );
            if result.is_err() {
                // Do not log credential-bearing lower-level signaling diagnostics.
                let _ = failed.send(Err("native Sync host failed to start".to_string()));
                eprintln!("ctox service: native Sync host stopped; local listener is unavailable");
            }
        })?;
    match started.recv() {
        Ok(Ok((authority, guest_registry, control_channel))) => Ok(Some(ServiceHost {
            authority,
            guest_registry,
            control_channel,
            stop: Some(stop),
            task: Some(task),
        })),
        failed => {
            let _ = stop.send(());
            let _ = task.join();
            match failed {
                Ok(Err(error)) => anyhow::bail!(error),
                Err(error) => Err(error).context("native Sync host startup thread ended"),
                Ok(Ok(_)) => unreachable!(),
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Descriptor {
    version: u32,
    node_id: u64,
    scope_id: String,
    ipc_endpoint: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    guest_endpoint: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    checkpoint_endpoint: Option<PathBuf>,
}
struct DescriptorGuard {
    path: PathBuf,
    inode: (u64, u64),
}
impl DescriptorGuard {
    fn publish(
        root: &Path,
        started: &HostStarted,
        guest_endpoint: Option<PathBuf>,
        checkpoint_endpoint: Option<PathBuf>,
    ) -> Result<Self> {
        let path = directory(root).join("listener.json");
        let mut file = tempfile::NamedTempFile::new_in(directory(root))?;
        serde_json::to_writer(
            file.as_file_mut(),
            &Descriptor {
                version: 1,
                node_id: started.node_id,
                scope_id: started.scope_id.clone(),
                ipc_endpoint: started.ipc_endpoint.clone(),
                guest_endpoint,
                checkpoint_endpoint,
            },
        )?;
        file.as_file_mut().flush()?;
        let metadata = file.as_file().metadata()?;
        file.persist(&path).map_err(|error| error.error)?;
        Ok(Self {
            path,
            inode: (metadata.dev(), metadata.ino()),
        })
    }
}
impl Drop for DescriptorGuard {
    fn drop(&mut self) {
        if let Ok(metadata) = std::fs::symlink_metadata(&self.path) {
            if metadata.is_file() && (metadata.dev(), metadata.ino()) == self.inode {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }
}
