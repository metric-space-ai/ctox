// ref: sdk/cliproxy/service.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

//! Service state and injected host boundaries.

use std::fmt;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::watch;

use crate::internal::config::ValidatedRuntimeConfig;
use crate::internal::runtime::executor::helps::xai_client_version::{
    XaiVersionFetcher, XaiVersionUpdater,
};
use crate::sdk::api::options::ServerOption;

use super::builder::{ServiceAssembly, ServiceBindingRequirement, ServiceBindings};
use super::pprof_server::{PprofConfig, PprofError, PprofServer};
use super::providers::LoadContext;
use super::service_runtime::ServiceRuntimeGraph;
use super::types::{WatcherError, WatcherWrapper};

pub type ServiceFuture = Pin<Box<dyn Future<Output = Result<(), ServiceError>> + Send>>;
pub type ClockFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceErrorKind {
    MissingBinding,
    AuthDirectory,
    TokenProvider,
    ApiKeyProvider,
    ServerBuild,
    Server,
    WatcherCreate,
    WatcherStart,
    WatcherStop,
    Reload,
    PluginHost,
    Pprof,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceError {
    pub kind: ServiceErrorKind,
    pub detail: String,
}

impl ServiceError {
    pub(crate) fn new(kind: ServiceErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cliproxy: {}", self.detail)
    }
}

impl std::error::Error for ServiceError {}

pub trait HostServer: Send + Sync {
    fn serve(self: Arc<Self>) -> ServiceFuture;
    fn stop(&self, timeout: Duration) -> ServiceFuture;
}

pub trait HostListenerAuthority: Send + Sync {
    fn create_server(
        &self,
        config: &ValidatedRuntimeConfig,
        config_path: &Path,
        options: Vec<ServerOption>,
    ) -> Result<Arc<dyn HostServer>, ServiceError>;
}

pub trait DirectoryAuthority: Send + Sync {
    fn ensure_directory(&self, path: &Path) -> Result<(), ServiceError>;
}

pub trait ServiceClock: Send + Sync {
    fn after_start_delay(&self, delay: Duration) -> ClockFuture;
}

pub trait ShutdownAuthority: Send + Sync {
    fn stop_server(&self, server: Arc<dyn HostServer>, timeout: Duration) -> ServiceFuture;
}

pub trait ReloadAuthority: Send + Sync {
    fn apply(&self, config: &ValidatedRuntimeConfig) -> Result<(), ServiceError>;
}

pub struct ServiceHost {
    pub listener: Arc<dyn HostListenerAuthority>,
    pub directories: Arc<dyn DirectoryAuthority>,
    pub clock: Arc<dyn ServiceClock>,
    pub shutdown: Arc<dyn ShutdownAuthority>,
    pub reload: Arc<dyn ReloadAuthority>,
    pub auth_dir: PathBuf,
    pub shutdown_timeout: Duration,
    pub pprof: Option<Arc<PprofServer>>,
    pub pprof_config: PprofConfig,
}

#[derive(Clone)]
pub struct RunCancellation {
    receiver: watch::Receiver<bool>,
    load: LoadContext,
}

pub struct RunCancellationHandle {
    sender: watch::Sender<bool>,
    load: LoadContext,
}

impl RunCancellation {
    #[must_use]
    pub fn channel() -> (RunCancellationHandle, Self) {
        let (sender, receiver) = watch::channel(false);
        let load = LoadContext::default();
        (
            RunCancellationHandle {
                sender,
                load: load.clone(),
            },
            Self { receiver, load },
        )
    }

    pub(crate) async fn cancelled(&mut self) {
        if *self.receiver.borrow() {
            return;
        }
        while self.receiver.changed().await.is_ok() {
            if *self.receiver.borrow() {
                return;
            }
        }
    }

    pub(crate) fn load_context(&self) -> &LoadContext {
        &self.load
    }
}

impl RunCancellationHandle {
    pub fn cancel(&self) {
        self.load.cancel();
        let _ = self.sender.send(true);
    }
}

pub struct Service {
    pub(crate) assembly: Mutex<ServiceAssembly>,
    pub(crate) host: ServiceHost,
    pub(crate) server: Mutex<Option<Arc<dyn HostServer>>>,
    pub(crate) watcher: Mutex<Option<WatcherWrapper>>,
    pub(crate) auth_queue: Mutex<Option<tokio::task::JoinHandle<()>>>,
    pub(crate) shutdown_result: tokio::sync::Mutex<Option<Result<(), ServiceError>>>,
    pub(crate) runtime_graph: Option<Arc<ServiceRuntimeGraph>>,
    pub(crate) xai_version_fetcher: Option<Arc<dyn XaiVersionFetcher>>,
    pub(crate) xai_version_updater: Mutex<Option<XaiVersionUpdater>>,
}

impl Service {
    pub fn new(
        mut assembly: ServiceAssembly,
        bindings: ServiceBindings,
        host: ServiceHost,
    ) -> Result<Arc<Self>, ServiceError> {
        let runtime_graph = bindings.runtime_graph.clone();
        let xai_version_fetcher = bindings.xai_client_version_fetcher.clone();
        assembly
            .materialize(bindings)
            .map_err(|error| ServiceError::new(ServiceErrorKind::PluginHost, error.to_string()))?;
        if !assembly.is_materializable() {
            let missing = assembly
                .requirements()
                .iter()
                .map(|requirement| match requirement {
                    ServiceBindingRequirement::ApiKeyClientProvider => "api-key-provider",
                    ServiceBindingRequirement::WatcherFactory => "watcher-factory",
                    ServiceBindingRequirement::CoreAuthManager => "core-auth-manager",
                    ServiceBindingRequirement::PluginHost => "plugin-host",
                    ServiceBindingRequirement::PersistedAuthUpdateSink => "auth-update-sink",
                })
                .collect::<Vec<_>>()
                .join(", ");
            return Err(ServiceError::new(
                ServiceErrorKind::MissingBinding,
                format!("missing service bindings: {missing}"),
            ));
        }
        Ok(Arc::new(Self {
            assembly: Mutex::new(assembly),
            host,
            server: Mutex::new(None),
            watcher: Mutex::new(None),
            auth_queue: Mutex::new(None),
            shutdown_result: tokio::sync::Mutex::new(None),
            runtime_graph,
            xai_version_fetcher,
            xai_version_updater: Mutex::new(None),
        }))
    }

    pub async fn apply_pprof_config(&self, config: &PprofConfig) -> bool {
        match &self.host.pprof {
            Some(pprof) => pprof.apply_context(config).await,
            None => !config.enable,
        }
    }

    pub(crate) async fn apply_pprof_config_with_cancellation(
        &self,
        config: &PprofConfig,
        cancellation: &LoadContext,
    ) -> bool {
        match &self.host.pprof {
            Some(pprof) => {
                pprof
                    .apply_context_with_cancellation(config, Some(cancellation))
                    .await
            }
            None => !config.enable && !cancellation.is_cancelled(),
        }
    }

    pub(crate) fn pprof_error(error: PprofError) -> ServiceError {
        ServiceError::new(ServiceErrorKind::Pprof, error.to_string())
    }

    pub(crate) fn watcher_error(kind: ServiceErrorKind, error: WatcherError) -> ServiceError {
        ServiceError::new(kind, error.to_string())
    }

    #[must_use]
    pub fn runtime_graph(&self) -> Option<Arc<ServiceRuntimeGraph>> {
        self.runtime_graph.clone()
    }

    pub(crate) fn apply_watcher_runtime_config(
        &self,
        config: ValidatedRuntimeConfig,
    ) -> Result<(), ServiceError> {
        self.host.reload.apply(&config)?;
        if let Some(graph) = &self.runtime_graph {
            graph.apply_watcher_config(config.clone());
        }
        if let Some(watcher) = self.watcher.lock().unwrap().as_ref() {
            watcher.set_config(config);
        }
        Ok(())
    }
}
