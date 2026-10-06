// Origin: CTOX — explicit scheduled Meta refresh bridge.
// ref: internal/runtime/executor/meta_executor.go:249-333 @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::meta_executor_auth::MetaRequestAuthPreparer;
use crate::sdk::cliproxy::auth::{
    Auth, AuthError, AuthRefresher, RefreshCancellation, RefreshExecutorError,
};
use std::{fmt, sync::Arc};

/// Adapts the shared native Meta capability to AutoRefreshWorker's blocking
/// executor boundary. The host supplies its existing Tokio runtime handle;
/// this bridge never creates a runtime, transport, credential or detached task.
///
/// Call refresh only from a blocking worker, as AutoRefreshWorker does. The
/// runtime owner must remain alive until its refresh worker has stopped.
pub struct MetaScheduledRefresher {
    native: Arc<MetaRequestAuthPreparer>,
    runtime: tokio::runtime::Handle,
}
impl MetaScheduledRefresher {
    pub fn new(native: Arc<MetaRequestAuthPreparer>, runtime: tokio::runtime::Handle) -> Self {
        Self { native, runtime }
    }
}
impl fmt::Debug for MetaScheduledRefresher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetaScheduledRefresher")
            .finish_non_exhaustive()
    }
}
impl AuthRefresher for MetaScheduledRefresher {
    fn refresh(&self, auth: &mut Auth) -> Result<Option<Auth>, RefreshExecutorError> {
        self.refresh_with_cancellation(auth, &RefreshCancellation::default())
    }

    fn refresh_with_cancellation(
        &self,
        auth: &mut Auth,
        cancellation: &RefreshCancellation,
    ) -> Result<Option<Auth>, RefreshExecutorError> {
        if cancellation.is_cancelled() || auth.disabled {
            return Err(RefreshExecutorError::Cancelled);
        }
        // The selected Meta service already bounds minting and body reads.
        // Awaiting cancellation owns no polling loop or separate operation.
        self.runtime.block_on(async {
            tokio::select! {
                biased;
                () = cancellation.cancelled() => Err(RefreshExecutorError::Cancelled),
                candidate = self.native.refresh_candidate(auth) => {
                    if cancellation.is_cancelled() {
                        return Err(RefreshExecutorError::Cancelled);
                    }
                    candidate.map(Some).map_err(|error| {
                        let failure = error.as_ref().downcast_ref::<AuthError>().cloned()
                            .unwrap_or_else(|| AuthError {
                                code: "meta_scheduled_refresh".into(),
                                message: "Meta scheduled credential refresh failed".into(),
                                retryable: true,
                                http_status: 502,
                            });
                        RefreshExecutorError::Failed(failure)
                    })
                }
            }
        })
    }
}
