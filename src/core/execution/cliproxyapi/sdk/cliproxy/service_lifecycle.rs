// ref: sdk/cliproxy/service_lifecycle.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

//! Cancellation-safe `Run` and idempotent `Shutdown` lifecycle.

use std::sync::Arc;
use std::time::Duration;

use super::service::{RunCancellation, Service, ServiceError, ServiceErrorKind};
use super::types::ReloadCallback;

impl Service {
    pub async fn run(
        self: &Arc<Self>,
        mut cancellation: RunCancellation,
    ) -> Result<(), ServiceError> {
        self.host
            .directories
            .ensure_directory(&self.host.auth_dir)?;

        let (
            config,
            config_path,
            token_provider,
            api_key_provider,
            watcher_factory,
            persisted_auth_sink,
            options,
        ) = {
            let mut assembly = self.assembly.lock().unwrap();
            assembly.run_before_start();
            (
                assembly.config().clone(),
                assembly.config_path().to_path_buf(),
                assembly.token_provider(),
                assembly
                    .api_key_provider()
                    .expect("materialization checked"),
                assembly.watcher_factory().expect("materialization checked"),
                assembly
                    .persisted_auth_update_sink()
                    .expect("materialization checked"),
                assembly.take_server_options(),
            )
        };

        token_provider
            .load(cancellation.load_context(), &config)
            .await
            .map_err(|error| {
                ServiceError::new(ServiceErrorKind::TokenProvider, error.to_string())
            })?;
        api_key_provider
            .load(cancellation.load_context(), &config)
            .await
            .map_err(|error| {
                ServiceError::new(ServiceErrorKind::ApiKeyProvider, error.to_string())
            })?;

        let server = self
            .host
            .listener
            .create_server(&config, &config_path, options)
            .map_err(|error| ServiceError::new(ServiceErrorKind::ServerBuild, error.to_string()))?;
        *self.server.lock().unwrap() = Some(server.clone());
        let mut server_task = tokio::spawn(server.serve());

        self.host
            .clock
            .after_start_delay(Duration::from_millis(100))
            .await;
        if !self
            .apply_pprof_config_with_cancellation(
                &self.host.pprof_config,
                cancellation.load_context(),
            )
            .await
        {
            let _ = self.shutdown().await;
            return Err(ServiceError::new(
                ServiceErrorKind::Pprof,
                "failed to apply pprof configuration",
            ));
        }
        self.assembly.lock().unwrap().run_after_start();

        let weak = Arc::downgrade(self);
        let reload: ReloadCallback = Arc::new(move |new_config| {
            if let Some(service) = weak.upgrade() {
                let _ = service.apply_watcher_runtime_config(new_config);
            }
        });
        let watcher = match watcher_factory.create(&config_path, &self.host.auth_dir, reload) {
            Ok(watcher) => watcher,
            Err(error) => {
                let lifecycle_error = Self::watcher_error(ServiceErrorKind::WatcherCreate, error);
                let _ = self.shutdown().await;
                return Err(lifecycle_error);
            }
        };
        watcher.set_config(config);
        let (auth_updates, mut auth_update_receiver) = tokio::sync::mpsc::channel(64);
        watcher.set_auth_update_queue(auth_updates);
        *self.auth_queue.lock().unwrap() = Some(tokio::spawn(async move {
            while let Some(update) = auth_update_receiver.recv().await {
                let _ = persisted_auth_sink.dispatch_persisted_auth_update(update);
            }
        }));
        if let Err(error) = watcher.start(cancellation.load_context()).await {
            let lifecycle_error = Self::watcher_error(ServiceErrorKind::WatcherStart, error);
            let _ = watcher.stop();
            let _ = self.shutdown().await;
            return Err(lifecycle_error);
        }
        *self.watcher.lock().unwrap() = Some(watcher);

        if let Some(fetcher) = &self.xai_version_fetcher {
            let updater = crate::internal::runtime::executor::helps::xai_client_version::start_xai_version_updater(Arc::clone(fetcher));
            *self.xai_version_updater.lock().unwrap() = Some(updater);
        }

        let result = tokio::select! {
            () = cancellation.cancelled() => Err(ServiceError::new(ServiceErrorKind::Cancelled, "service context cancelled")),
            result = &mut server_task => match result {
                Ok(result) => result,
                Err(error) => Err(ServiceError::new(ServiceErrorKind::Server, format!("server task failed: {error}"))),
            }
        };
        let shutdown = self.shutdown().await;
        match (result, shutdown) {
            (Err(error), _) => Err(error),
            (Ok(()), Err(error)) => Err(error),
            (Ok(()), Ok(())) => Ok(()),
        }
    }

    pub async fn shutdown(&self) -> Result<(), ServiceError> {
        // Holding this async mutex across teardown intentionally serializes the
        // one-shot authority release, mirroring upstream `sync.Once`.
        let mut shutdown_result = self.shutdown_result.lock().await;
        if let Some(result) = shutdown_result.clone() {
            return result;
        }

        let mut first_error = None;
        let updater = self.xai_version_updater.lock().unwrap().take();
        if let Some(updater) = updater {
            updater.stop().await;
        }
        let watcher = self.watcher.lock().unwrap().take();
        if let Some(watcher) = watcher {
            if let Err(error) = watcher.stop() {
                first_error = Some(Self::watcher_error(ServiceErrorKind::WatcherStop, error));
            }
        }
        let auth_queue = self.auth_queue.lock().unwrap().take();
        if let Some(auth_queue) = auth_queue {
            auth_queue.abort();
            let _ = auth_queue.await;
        }
        if let Some(graph) = &self.runtime_graph {
            if let Err(error) = graph.shutdown(super::executionregistry::WaitBudget::unbounded()) {
                first_error.get_or_insert_with(|| {
                    ServiceError::new(ServiceErrorKind::Reload, error.to_string())
                });
            }
        }
        if let Some(pprof) = &self.host.pprof {
            if let Err(error) = pprof.shutdown().await {
                first_error.get_or_insert_with(|| Self::pprof_error(error));
            }
        }
        let server = self.server.lock().unwrap().take();
        if let Some(server) = server {
            if let Err(error) = self
                .host
                .shutdown
                .stop_server(server, self.host.shutdown_timeout)
                .await
            {
                first_error.get_or_insert(error);
            }
        }
        let result = first_error.map_or(Ok(()), Err);
        *shutdown_result = Some(result.clone());
        result
    }
}
