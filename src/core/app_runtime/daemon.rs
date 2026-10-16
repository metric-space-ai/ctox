//! Async daemon seam for the synchronous, process-supervised app runtime.
//!
//! Core supplies its existing writer/read pool, live authority and CAP router
//! through CoreRuntimeHost. This module never opens a database or creates a
//! Tokio runtime. Publish/change/completion calls must follow committed Core
//! events; none is inferred by polling tables or watching a directory.
use crate::{
    core_host::CoreRuntimeHost,
    service::{AppRuntimeService, ChangeEvent},
};
use anyhow::{Context, Result};
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::Semaphore;

pub type BlockingOperation = Box<dyn FnOnce() -> Result<()> + Send + 'static>;
pub type BlockingFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'static>>;
/// The native host supplies its bounded Core compatibility admission. The
/// portable runtime never grants itself database or credential authority.
pub trait BlockingExecutor: Send + Sync + 'static {
    fn run(&self, operation: BlockingOperation) -> BlockingFuture;
}

#[derive(Clone)]
pub struct AppRuntimeDaemon {
    service: Arc<AppRuntimeService>,
    blocking_calls: Arc<Semaphore>,
    executor: Option<Arc<dyn BlockingExecutor>>,
}
impl AppRuntimeDaemon {
    pub(crate) fn bootstrap_published(&self, app: &str, version: &str) -> Result<()> {
        self.service.published(app, version)
    }
    pub fn production(root: PathBuf, host: Arc<CoreRuntimeHost>) -> Result<Self> {
        Ok(Self::from_service(AppRuntimeService::production(
            root, host,
        )?))
    }
    pub(crate) fn from_service(service: AppRuntimeService) -> Self {
        Self {
            service: Arc::new(service),
            blocking_calls: Arc::new(Semaphore::new(2)),
            executor: None,
        }
    }
    pub fn with_executor(mut self, executor: Arc<dyn BlockingExecutor>) -> Self {
        self.executor = Some(executor);
        self
    }
    async fn call<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&AppRuntimeService) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        // Fail at admission instead of accumulating an unbounded blocking queue.
        let permit = self
            .blocking_calls
            .clone()
            .try_acquire_owned()
            .map_err(|_| anyhow::anyhow!("unavailable: app runtime busy"))?;
        let service = self.service.clone();
        if let Some(executor) = &self.executor {
            let (send, receive) = tokio::sync::oneshot::channel();
            executor
                .run(Box::new(move || {
                    let _permit = permit;
                    let _ = send.send(operation(&service));
                    Ok(())
                }))
                .await?;
            return receive
                .await
                .context("unavailable: admitted app runtime call stopped")?;
        }
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            operation(&service)
        })
        .await
        .context("unavailable: app runtime call stopped")?
    }
    /// Core calls this only after its install/publish transaction committed.
    pub async fn published(&self, app: String, version: String) -> Result<()> {
        self.call(move |service| service.published(&app, &version))
            .await
    }
    pub async fn removed(&self, app: String) -> Result<()> {
        self.call(move |service| service.removed(&app)).await
    }
    /// Preserve the authoritative command document, including app_id and type.
    pub async fn command(&self, command: Value, fence: Value) -> Result<Value> {
        self.call(move |service| service.command(command, fence))
            .await
    }
    /// Feed the ordered committed change stream; trigger claims are durable.
    pub async fn change(&self, event: ChangeEvent) -> Result<Vec<Value>> {
        self.call(move |service| service.change(&event)).await
    }
    /// Core's shared scheduler calls this at an interval of at least one minute.
    pub async fn tick(&self) -> Result<Vec<Value>> {
        self.call(AppRuntimeService::tick).await
    }
    /// Legacy export callbacks only. Generic task completion is published by
    /// Core as an own-app business command and enters through command().
    pub async fn completed(
        &self,
        binding: Value,
        task_id: String,
        succeeded: bool,
        outcome: Value,
    ) -> Result<Value> {
        self.call(move |service| service.completed(binding, &task_id, succeeded, outcome))
            .await
    }
    pub async fn skill(
        &self,
        scoped_name: String,
        actor: Value,
    ) -> Result<crate::artifact_text::AppSkill> {
        self.call(move |service| service.skill(&scoped_name, &actor))
            .await
    }
    pub async fn prompt(&self, app: String, path: String, actor: Value) -> Result<Value> {
        self.call(move |service| service.prompt(&app, &path, &actor))
            .await
    }
    pub async fn command_completed(
        &self,
        binding: Value,
        command_id: String,
        outcome: Value,
    ) -> Result<Value> {
        self.call(move |service| service.command_completed(binding, &command_id, outcome))
            .await
    }
}
