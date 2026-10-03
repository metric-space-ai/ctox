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

#[path = "runtime.rs"]
mod runtime;
const SECRET_SCOPE: &str = "ctox-sync-host";
const IDENTITY_SECRET: &str = "identity-pkcs8";
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
fn configuration(root: &Path) -> Result<HostConfiguration> {
    load_config(root)?.context("native Sync host is not configured")
}
pub(super) fn key(root: &Path) -> Result<Arc<SigningIdentity>> {
    let encoded = crate::secrets::read_secret_value(root, SECRET_SCOPE, IDENTITY_SECRET)
        .map_err(|_| anyhow::anyhow!("native Sync identity is unavailable in the secret store"))?;
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct StoredKey {
        identity: String,
        pkcs8: String,
    }
    let record: StoredKey = serde_json::from_str(&encoded)
        .map_err(|_| anyhow::anyhow!("invalid native Sync key record"))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(record.pkcs8)
        .context("invalid native Sync key encoding")?;
    Ok(Arc::new(
        SigningIdentity::from_existing_pkcs8(&bytes, &record.identity)
            .context("invalid native Sync signing key")?,
    ))
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
        ["status"] => runtime::status(&root),
        ["run"] => runtime::run(&root, async {
            let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
            tokio::select! { result = tokio::signal::ctrl_c() => result, _ = terminate.recv() => Ok(()) }
        }, |started| print(serde_json::json!({"listener":"active", "nodeId":started.node_id, "scopeId":started.scope_id, "ipcEndpoint":started.ipc_endpoint}))),
        _ => anyhow::bail!("usage: ctox sync init | identity | import-key <public-identity> (key on stdin) | configure (public JSON on stdin) | transport (secret JSON on stdin) | status | run"),
    }
}

pub struct ServiceHost {
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<thread::JoinHandle<()>>,
}
impl Drop for ServiceHost {
    fn drop(&mut self) {
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
                move |_| {
                    ready
                        .send(Ok(()))
                        .map_err(|_| anyhow::anyhow!("native Sync service startup receiver closed"))
                },
            );
            if result.is_err() {
                // Do not log credential-bearing lower-level signaling diagnostics.
                let _ = failed.send(Err("native Sync host failed to start".to_string()));
                eprintln!("ctox service: native Sync host stopped; local listener is unavailable");
            }
        })?;
    let host = ServiceHost {
        stop: Some(stop),
        task: Some(task),
    };
    match started
        .recv()
        .context("native Sync host startup thread ended")?
    {
        Ok(()) => Ok(Some(host)),
        Err(error) => {
            drop(host);
            anyhow::bail!(error)
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
}
struct DescriptorGuard {
    path: PathBuf,
    inode: (u64, u64),
}
impl DescriptorGuard {
    fn publish(root: &Path, started: &HostStarted) -> Result<Self> {
        let path = directory(root).join("listener.json");
        let mut file = tempfile::NamedTempFile::new_in(directory(root))?;
        serde_json::to_writer(
            file.as_file_mut(),
            &Descriptor {
                version: 1,
                node_id: started.node_id,
                scope_id: started.scope_id.clone(),
                ipc_endpoint: started.ipc_endpoint.clone(),
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
