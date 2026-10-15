// Origin: CTOX; License: AGPL-3.0-only
//! Explicit, instance-owned quota plugins. No discovery, downloads or env switches.
use super::*;
use ctox_cliproxyapi::{
    internal::{
        api::handlers::management::ManagementPluginQuotaService,
        pluginhost::{
            auth_provider::HostConfigSummarySource,
            callback_contexts::CallbackContextRegistry,
            config::{RuntimeConfig, RuntimeItemConfig},
            host::PluginHost,
            host_callbacks::{
                install_standard_host_callbacks, HostCallbackRouter, HostLogAuthority,
            },
            http_bridge::HostHttpBridge,
            http_stream_bridge::HttpStreamBridge,
            platform::PluginFileInfo,
            process_client::ProcessPluginLoader,
            stream_bridge::StreamBridge,
        },
    },
    sdk::pluginapi::{
        self, HostConfigSummary, HostHttpClient, HttpRequest, HttpResponse, HttpStreamResponse,
    },
};
use std::io::Read;
const TABLE: &str = "cliproxyapi_quota_plugins";
const SCHEMA: &str = "ctox.cliproxyapi.quota-plugins.v1";
const MAX_CONFIG: u64 = 65536;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plugin {
    pub id: String,
    pub executable: PathBuf,
    pub sha256: String,
    #[serde(default)]
    pub priority: i32,
    pub config_secret: Option<RuntimeSecretRef>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: String,
    pub expected_revision: u64,
    pub enabled: bool,
    pub plugins: Vec<Plugin>,
}
fn validate(config: &Config) -> anyhow::Result<()> {
    anyhow::ensure!(
        config.schema == SCHEMA
            && config.plugins.len() <= 4
            && (!config.enabled || !config.plugins.is_empty()),
        "invalid quota plugin configuration"
    );
    let mut ids = BTreeSet::new();
    for plugin in &config.plugins {
        anyhow::ensure!(
            ctox_cliproxyapi::internal::pluginhost::platform::validate_plugin_id(&plugin.id)
                && ids.insert(&plugin.id)
                && plugin.executable.is_absolute()
                && plugin.sha256.len() == 64
                && plugin
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "invalid quota plugin artifact"
        );
        if let Some(secret) = &plugin.config_secret {
            anyhow::ensure!(
                secret.scope == "cliproxyapi-plugins"
                    && secret.name == format!("{}-config", plugin.id),
                "quota plugin configuration must use its own secret-store record"
            );
        }
    }
    Ok(())
}
fn load(root: &Path) -> anyhow::Result<Option<(u64, Config)>> {
    let path = crate::inference::runtime_env::runtime_config_path(root);
    if !path.try_exists()? {
        return Ok(None);
    }
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(Duration::ZERO)?;
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [TABLE],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(None);
    }
    let row: Option<(u64, String)> = conn
        .query_row(
            &format!("SELECT revision, config_json FROM {TABLE} WHERE config_id=1"),
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    row.map(|(revision, json)| {
        anyhow::ensure!(
            json.len() as u64 <= MAX_CONFIG,
            "oversized quota plugin configuration"
        );
        let config: Config = serde_json::from_str(&json)
            .map_err(|_| anyhow::anyhow!("invalid stored quota plugin configuration"))?;
        validate(&config)?;
        Ok((revision, config))
    })
    .transpose()
}
pub fn configure_from_file(root: &Path, path: &Path) -> anyhow::Result<serde_json::Value> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_CONFIG + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() as u64 <= MAX_CONFIG,
        "quota plugin configuration exceeds limit"
    );
    let config: Config = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("invalid quota plugin configuration"))?;
    validate(&config)?;
    if config.enabled {
        for plugin in &config.plugins {
            verify_artifact(plugin)?;
        }
    }
    let path = crate::inference::runtime_env::runtime_config_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_secs(5))?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(&format!("CREATE TABLE IF NOT EXISTS {TABLE}(config_id INTEGER PRIMARY KEY CHECK(config_id=1),revision INTEGER NOT NULL,config_json TEXT NOT NULL)"))?;
    let old: u64 = tx
        .query_row(
            &format!("SELECT revision FROM {TABLE} WHERE config_id=1"),
            [],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0);
    anyhow::ensure!(
        old == config.expected_revision,
        "quota plugin configuration revision conflict"
    );
    let revision = old
        .checked_add(1)
        .context("quota plugin revision overflow")?;
    tx.execute(&format!("INSERT INTO {TABLE}(config_id,revision,config_json) VALUES(1,?1,?2) ON CONFLICT(config_id) DO UPDATE SET revision=excluded.revision,config_json=excluded.config_json"),params![revision,serde_json::to_string(&config)?])?;
    tx.commit()?;
    Ok(
        serde_json::json!({"schema":SCHEMA,"revision":revision,"enabled":config.enabled,"plugins":config.plugins.iter().map(|p|p.id.as_str()).collect::<Vec<_>>()}),
    )
}
fn verify_artifact(plugin: &Plugin) -> anyhow::Result<()> {
    let metadata = std::fs::symlink_metadata(&plugin.executable)?;
    anyhow::ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() <= 128 * 1024 * 1024,
        "invalid quota plugin artifact file"
    );
    let mut file = std::fs::File::open(&plugin.executable)?;
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 65536];
    let mut size = 0usize;
    loop {
        let count = file.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        size += count;
        anyhow::ensure!(size <= 128 * 1024 * 1024, "oversized quota plugin artifact");
        hash.update(&bytes[..count]);
    }
    anyhow::ensure!(
        format!("{:x}", hash.finalize()) == plugin.sha256,
        "quota plugin artifact digest mismatch"
    );
    Ok(())
}
pub(super) fn identity(root: &Path) -> anyhow::Result<String> {
    let Some((revision, config)) = load(root)? else {
        return Ok("unconfigured".into());
    };
    let mut hash = Sha256::new();
    hash.update(revision.to_be_bytes());
    hash.update(serde_json::to_vec(&config)?);
    if config.enabled {
        for plugin in &config.plugins {
            if let Some(key) = &plugin.config_secret {
                let version =
                    crate::secrets::secret_record_content_version(root, &key.scope, &key.name)?
                        .context("quota plugin configuration secret unavailable")?;
                hash.update(version.as_bytes());
            }
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}
struct Summary;
impl HostConfigSummarySource for Summary {
    fn snapshot(&self) -> HostConfigSummary {
        // No plaintext auth directory, global proxy or global model aliases exist.
        HostConfigSummary::default()
    }
}
struct UnscopedHttp;
impl HostHttpClient for UnscopedHttp {
    fn execute<'a>(&'a self, _: HttpRequest) -> pluginapi::PluginFuture<'a, HttpResponse> {
        Box::pin(async {
            Err(Arc::new(std::io::Error::other(
                "quota HTTP requires a bound native account",
            )) as pluginapi::PluginExecutionError)
        })
    }
    fn execute_stream<'a>(
        &'a self,
        _: HttpRequest,
    ) -> pluginapi::PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async {
            Err(Arc::new(std::io::Error::other(
                "quota HTTP requires a bound native account",
            )) as pluginapi::PluginExecutionError)
        })
    }
}
struct Logs;
impl HostLogAuthority for Logs {
    fn log<'a>(
        &'a self,
        plugin: &'a str,
        level: &'a str,
        _: &'a str,
        _: BTreeMap<String, serde_json::Value>,
    ) -> pluginapi::PluginFuture<'a, ()> {
        // Plugin-controlled messages/configuration can contain secrets; do not echo them.
        Box::pin(async move {
            let level = match level {
                "debug" => "debug",
                "info" => "info",
                "warn" => "warn",
                "error" => "error",
                _ => "unknown",
            };
            eprintln!("ctox quota plugin event: plugin={plugin} level={level}");
            Ok(())
        })
    }
}
pub(super) struct Owner {
    pub(super) identity: String,
    pub(super) service: Option<Arc<ManagementPluginQuotaService>>,
    host: Option<Arc<PluginHost>>,
}
impl Owner {
    pub(super) async fn start(root: &Path) -> anyhow::Result<Self> {
        let initial = identity(root)?;
        let Some((_, config)) = load(root)? else {
            return Ok(Self {
                identity: initial,
                service: None,
                host: None,
            });
        };
        if !config.enabled {
            return Ok(Self {
                identity: initial,
                service: None,
                host: None,
            });
        }
        let runtime_root = crate::inference::runtime_env::runtime_config_path(root)
            .parent()
            .context("runtime root missing")?
            .join(".qp");
        std::fs::create_dir_all(&runtime_root)?;
        let contexts = CallbackContextRegistry::new();
        let mut callbacks = HostCallbackRouter::new(contexts.clone());
        install_standard_host_callbacks(
            &mut callbacks,
            HostHttpBridge::new(Arc::new(UnscopedHttp)),
            Arc::new(HttpStreamBridge::default()),
            StreamBridge::new(),
            Arc::new(Logs),
        )
        .map_err(|_| anyhow::anyhow!("quota host callbacks unavailable"))?;
        let loader = Arc::new(
            ProcessPluginLoader::new(runtime_root, Arc::new(callbacks))
                .map_err(|_| anyhow::anyhow!("quota process runtime unavailable"))?,
        );
        let host = Arc::new(PluginHost::with_callback_contexts(loader, contexts));
        let mut runtime = RuntimeConfig {
            enabled: true,
            ..Default::default()
        };
        let mut files = Vec::new();
        for plugin in &config.plugins {
            verify_artifact(plugin)?;
            let yaml = match &plugin.config_secret {
                Some(key) => Zeroizing::new(crate::secrets::read_secret_value(
                    root, &key.scope, &key.name,
                )?),
                None => Zeroizing::new(format!("enabled: true\npriority: {}\n", plugin.priority)),
            };
            anyhow::ensure!(
                yaml.len() <= 65536,
                "quota plugin private configuration exceeds limit"
            );
            runtime.items.insert(
                plugin.id.clone(),
                RuntimeItemConfig {
                    id: plugin.id.clone(),
                    enabled: true,
                    priority: plugin.priority,
                    version: None,
                    config_yaml: yaml.as_bytes().to_vec(),
                },
            );
            files.push(PluginFileInfo {
                id: plugin.id.clone(),
                path: plugin.executable.clone(),
                version: None,
            });
        }
        let report = host.apply_config(&runtime, &files).await;
        if !report.failures.is_empty()
            || host.quota_provider_identifiers().len() != config.plugins.len()
            || !identity(root).is_ok_and(|current| current == initial)
        {
            host.shutdown().await;
            anyhow::bail!("quota plugin initialization failed or configuration changed")
        }
        let service = Arc::new(ManagementPluginQuotaService::new(
            host.clone(),
            Arc::new(super::plugin_quota::CtoxQuotaCredentials::new(root)),
            Arc::new(Summary),
        ));
        Ok(Self {
            identity: initial,
            service: Some(service),
            host: Some(host),
        })
    }
    pub(super) async fn shutdown(&self) {
        if let Some(host) = &self.host {
            host.shutdown().await;
        }
    }
}
