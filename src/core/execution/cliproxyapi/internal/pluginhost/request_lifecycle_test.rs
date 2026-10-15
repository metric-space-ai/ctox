// ref: internal/pluginhost/request_lifecycle_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: ordered interceptor termination and detached lifecycle completion
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Notify;

use crate::sdk::pluginapi::{
    PluginFuture, RequestCompletion, RequestCompletionOutcome, RequestInterceptRequest,
    RequestInterceptResponse, RequestInterceptor, RequestLifecyclePlugin,
};

use super::adapters_interceptors::{
    RequestCompletionDispatcher, RequestInterceptorChain, RequestInterceptorRecord,
    RequestLifecycleRecord,
};

struct Interceptor {
    calls: Arc<AtomicUsize>,
    terminate: bool,
}

impl RequestInterceptor for Interceptor {
    fn intercept_request_before_auth<'a>(
        &'a self,
        _request: RequestInterceptRequest,
    ) -> PluginFuture<'a, RequestInterceptResponse> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(RequestInterceptResponse {
                terminate: self.terminate,
                status_code: if self.terminate { 403 } else { 0 },
                response_body: if self.terminate {
                    br#"{"error":"blocked"}"#.to_vec()
                } else {
                    Vec::new()
                },
                ..RequestInterceptResponse::default()
            })
        })
    }

    fn intercept_request_after_auth<'a>(
        &'a self,
        request: RequestInterceptRequest,
    ) -> PluginFuture<'a, RequestInterceptResponse> {
        self.intercept_request_before_auth(request)
    }
}

struct Lifecycle {
    seen: Arc<Mutex<Vec<RequestCompletion>>>,
    started: Arc<Notify>,
    release: Arc<Notify>,
}

impl RequestLifecyclePlugin for Lifecycle {
    fn handle_request_complete<'a>(
        &'a self,
        completion: RequestCompletion,
    ) -> PluginFuture<'a, ()> {
        Box::pin(async move {
            self.seen
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(completion);
            self.started.notify_one();
            self.release.notified().await;
            Ok(())
        })
    }
}
