// ref: sdk/cliproxy/service_executors.go:293-296 @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox — explicit host-owned execution, refresh and preparation
// License: MIT (upstream); modifications AGPL-3.0-only
use super::auth::{
    Auth, AuthPreparer, AuthRefresher, ExecutionSessionCloser, ProviderExecutorRegistration,
};
use super::service_executors::{
    openai_compat_info_from_auth, ExecutorFactoryError, ServiceExecutorFactory,
};
use crate::internal::runtime::executor::devin_executor::DevinExecutor;
use std::{fmt, sync::Arc};

/// Adds the real Devin execution capability to the existing service factory.
/// The native owner supplies session/usage context through DevinExecutor and
/// supplies its credential preparation and refresh capabilities explicitly.
/// Other providers retain the existing factory; no default client/store/runtime
/// or fabricated context is created by this binding.
pub struct DevinExecutorFactory {
    fallback: Arc<dyn ServiceExecutorFactory>,
    execution: Arc<DevinExecutor>,
    refresher: Arc<dyn AuthRefresher>,
    preparer: Arc<dyn AuthPreparer>,
    session_closer: Option<Arc<dyn ExecutionSessionCloser>>,
}
impl DevinExecutorFactory {
    pub fn new(
        fallback: Arc<dyn ServiceExecutorFactory>,
        execution: Arc<DevinExecutor>,
        refresher: Arc<dyn AuthRefresher>,
        preparer: Arc<dyn AuthPreparer>,
    ) -> Self {
        Self {
            fallback,
            execution,
            refresher,
            preparer,
            session_closer: None,
        }
    }
    pub fn with_session_closer(mut self, closer: Arc<dyn ExecutionSessionCloser>) -> Self {
        self.session_closer = Some(closer);
        self
    }
}
impl fmt::Debug for DevinExecutorFactory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevinExecutorFactory")
            .field("has_session_closer", &self.session_closer.is_some())
            .finish_non_exhaustive()
    }
}
impl ServiceExecutorFactory for DevinExecutorFactory {
    fn registration_for(
        &self,
        provider_key: &str,
        auth: &Auth,
    ) -> Result<Arc<ProviderExecutorRegistration>, ExecutorFactoryError> {
        if !provider_key.trim().eq_ignore_ascii_case("devin") {
            return self.fallback.registration_for(provider_key, auth);
        }
        if !auth.provider.trim().eq_ignore_ascii_case("devin")
            || openai_compat_info_from_auth(auth).2
        {
            return Err(ExecutorFactoryError::InvalidRegistration);
        }
        if auth.disabled {
            return Err(ExecutorFactoryError::Unsupported);
        }
        let mut registration = ProviderExecutorRegistration::new("devin", self.refresher.clone())
            .ok_or(ExecutorFactoryError::InvalidRegistration)?
            .with_execution(self.execution.clone())
            .map_err(|_| ExecutorFactoryError::InvalidRegistration)?
            .with_auth_preparer(self.preparer.clone());
        if let Some(closer) = &self.session_closer {
            registration = registration.with_session_closer(closer.clone());
        }
        Ok(Arc::new(registration))
    }
}

#[cfg(test)]
#[path = "service_devin_test.rs"]
mod tests;
