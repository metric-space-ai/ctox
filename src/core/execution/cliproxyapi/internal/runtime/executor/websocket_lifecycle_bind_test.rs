// ref: internal/runtime/executor/websocket_lifecycle_bind_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::sdk::cliproxy::executor::{
    BoundResourceCloser, ExecutionLifecycle, LifecycleResult, ResourceCloseFn,
};

use super::codex_websockets_connection::{
    CodexWebsocketActionFuture, CodexWebsocketConnection, CodexWebsocketFrame,
    CodexWebsocketFrameFuture,
};
use super::codex_websockets_executor::bind_connection_lifecycle;
use super::codex_websockets_session::CodexWebsocketSessionState;

struct CountingConnection {
    lifecycle_closes: Arc<AtomicUsize>,
    async_closes: Arc<AtomicUsize>,
}

impl CodexWebsocketConnection for CountingConnection {
    fn send<'a>(&'a mut self, _frame: CodexWebsocketFrame) -> CodexWebsocketActionFuture<'a> {
        Box::pin(async { Ok(()) })
    }

    fn receive<'a>(&'a mut self) -> CodexWebsocketFrameFuture<'a> {
        Box::pin(async { Ok(CodexWebsocketFrame::Close { code: 1000 }) })
    }

    fn close<'a>(&'a mut self) -> CodexWebsocketActionFuture<'a> {
        let closes = Arc::clone(&self.async_closes);
        Box::pin(async move {
            closes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }

    fn lifecycle_closer(&self) -> Option<ResourceCloseFn> {
        let closes = Arc::clone(&self.lifecycle_closes);
        Some(Box::new(move || {
            closes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }))
    }
}

#[derive(Default)]
struct CountingLifecycle {
    binds: AtomicUsize,
    ends: AtomicUsize,
    closer: Mutex<Option<BoundResourceCloser>>,
}

impl ExecutionLifecycle for CountingLifecycle {
    fn bind(&self, closer: BoundResourceCloser) -> LifecycleResult {
        self.binds.fetch_add(1, Ordering::SeqCst);
        *self.closer.lock().unwrap() = Some(closer);
        Ok(())
    }

    fn end(&self, _reason: &str) {
        self.ends.fetch_add(1, Ordering::SeqCst);
        if let Some(closer) = self.closer.lock().unwrap().take() {
            closer.close().unwrap();
        }
    }
}
