// ref: internal/runtime/executor/websocket_session_target_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::codex_websockets_connection::{
    CodexWebsocketActionFuture, CodexWebsocketConnection, CodexWebsocketFrame,
    CodexWebsocketFrameFuture,
};
use super::codex_websockets_errors::CodexWebsocketError;
use super::codex_websockets_executor::bind_connection_lifecycle;
use super::codex_websockets_session::CodexWebsocketSessionStore;
use crate::sdk::cliproxy::executor::{
    BoundResourceCloser, ExecutionLifecycle, LifecycleResult, ResourceCloseFn,
};

#[derive(Default)]
struct PhysicalClose {
    closed: AtomicBool,
    calls: AtomicUsize,
}

impl PhysicalClose {
    fn close(&self) {
        if !self.closed.swap(true, Ordering::AcqRel) {
            self.calls.fetch_add(1, Ordering::SeqCst);
        }
    }
}

struct Connection(Arc<PhysicalClose>);

impl CodexWebsocketConnection for Connection {
    fn send<'a>(&'a mut self, _: CodexWebsocketFrame) -> CodexWebsocketActionFuture<'a> {
        Box::pin(async { Ok(()) })
    }

    fn receive<'a>(&'a mut self) -> CodexWebsocketFrameFuture<'a> {
        Box::pin(async { Err(CodexWebsocketError::protocol("closed", true)) })
    }

    fn close<'a>(&'a mut self) -> CodexWebsocketActionFuture<'a> {
        let physical = Arc::clone(&self.0);
        Box::pin(async move {
            physical.close();
            Ok(())
        })
    }

    fn lifecycle_closer(&self) -> Option<ResourceCloseFn> {
        let physical = Arc::clone(&self.0);
        Some(Box::new(move || {
            physical.close();
            Ok(())
        }))
    }
}

#[derive(Default)]
struct Lifecycle {
    closer: Mutex<Option<BoundResourceCloser>>,
    binds: AtomicUsize,
    ends: AtomicUsize,
}

impl ExecutionLifecycle for Lifecycle {
    fn bind(&self, closer: BoundResourceCloser) -> LifecycleResult {
        self.binds.fetch_add(1, Ordering::SeqCst);
        *self.closer.lock().unwrap() = Some(closer);
        Ok(())
    }

    fn end(&self, _: &str) {
        self.ends.fetch_add(1, Ordering::SeqCst);
        if let Some(closer) = self.closer.lock().unwrap().take() {
            closer.close().unwrap();
        }
    }
}
