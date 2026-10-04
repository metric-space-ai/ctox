// ref: sdk/cliproxy/service_executors.go @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox — explicit Meta inference, preparation and refresh capabilities
// License: MIT (upstream); modifications AGPL-3.0-only
use super::auth::{
    AsyncAuthRefresher, Auth, AuthPreparer, AuthRefresher, ProviderExecutorRegistration,
};
use super::service_executors::{
    openai_compat_info_from_auth, ExecutorFactoryError, ServiceExecutorFactory,
};
use crate::internal::runtime::executor::{
    meta_executor::MetaExecutor, meta_executor_auth::MetaRequestAuthPreparer,
    meta_executor_scheduled::MetaScheduledRefresher,
};
use std::{fmt, sync::Arc};

/// Runtime owners supply both scheduled and async request-time refresh. The
/// factory neither creates a transport nor adopts another account's secrets.
pub struct MetaExecutorFactory {
    fallback: Arc<dyn ServiceExecutorFactory>,
    execution: Arc<MetaExecutor>,
    scheduled_refresher: Arc<dyn AuthRefresher>,
    async_refresher: Arc<dyn AsyncAuthRefresher>,
    preparer: Arc<dyn AuthPreparer>,
}
impl MetaExecutorFactory {
    /// Build all refresh/preparation roles from one native owner. Scheduled
    /// refresh runs on the existing worker's blocking boundary; request-time
    /// preparation and 401 recovery retain their asynchronous interface.
    pub fn with_native_auth(
        fallback: Arc<dyn ServiceExecutorFactory>,
        execution: Arc<MetaExecutor>,
        native: Arc<MetaRequestAuthPreparer>,
        runtime: tokio::runtime::Handle,
    ) -> Self {
        Self::new(
            fallback,
            execution,
            Arc::new(MetaScheduledRefresher::new(native.clone(), runtime)),
            native.clone(),
            native,
        )
    }

    pub fn new(
        fallback: Arc<dyn ServiceExecutorFactory>,
        execution: Arc<MetaExecutor>,
        scheduled_refresher: Arc<dyn AuthRefresher>,
        async_refresher: Arc<dyn AsyncAuthRefresher>,
        preparer: Arc<dyn AuthPreparer>,
    ) -> Self {
        Self {
            fallback,
            execution,
            scheduled_refresher,
            async_refresher,
            preparer,
        }
    }
}
impl fmt::Debug for MetaExecutorFactory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetaExecutorFactory")
            .finish_non_exhaustive()
    }
}
impl ServiceExecutorFactory for MetaExecutorFactory {
    fn registration_for(
        &self,
        provider_key: &str,
        auth: &Auth,
    ) -> Result<Arc<ProviderExecutorRegistration>, ExecutorFactoryError> {
        if !provider_key.trim().eq_ignore_ascii_case("meta") {
            return self.fallback.registration_for(provider_key, auth);
        }
        if !auth.provider.trim().eq_ignore_ascii_case("meta")
            || openai_compat_info_from_auth(auth).2
        {
            return Err(ExecutorFactoryError::InvalidRegistration);
        }
        if auth.disabled {
            return Err(ExecutorFactoryError::Unsupported);
        }
        let registration =
            ProviderExecutorRegistration::new("meta", self.scheduled_refresher.clone())
                .ok_or(ExecutorFactoryError::InvalidRegistration)?
                .with_execution(self.execution.clone())
                .map_err(|_| ExecutorFactoryError::InvalidRegistration)?
                .with_auth_preparer(self.preparer.clone())
                .with_async_auth_refresher(self.async_refresher.clone());
        Ok(Arc::new(registration))
    }
}
