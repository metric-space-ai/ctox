// ref: internal/runtime/executor/codex_websockets_session.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::sync::Mutex as AsyncMutex;

use super::codex_websockets_connection::CodexWebsocketConnection;
use crate::sdk::cliproxy::executor::ExecutionLifecycle;

pub struct CodexWebsocketSession {
    pub(crate) id: String,
    pub(crate) execution: AsyncMutex<CodexWebsocketSessionState>,
}

pub(crate) struct CodexWebsocketSessionState {
    pub auth_id: String,
    pub target_url: String,
    pub connection: Option<Box<dyn CodexWebsocketConnection>>,
    pub generation: u64,
    pub committed: bool,
    pub lifecycle: Option<Arc<dyn ExecutionLifecycle>>,
    pub lifecycle_generation: u64,
}

impl CodexWebsocketSessionState {
    pub async fn close(&mut self, reason: &str) {
        if let Some(lifecycle) = self.lifecycle.take() {
            lifecycle.end(reason);
        }
        self.lifecycle_generation = 0;
        if let Some(mut connection) = self.connection.take() {
            let _ = connection.close().await;
        }
        self.committed = false;
    }
}

impl CodexWebsocketSession {
    fn new(id: String) -> Self {
        Self {
            id,
            execution: AsyncMutex::new(CodexWebsocketSessionState {
                auth_id: String::new(),
                target_url: String::new(),
                connection: None,
                generation: 0,
                committed: false,
                lifecycle: None,
                lifecycle_generation: 0,
            }),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

impl fmt::Debug for CodexWebsocketSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodexWebsocketSession")
            .field("id", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

/// Executor-owned store. Unlike upstream's package global, session ownership
/// and shutdown are explicit and isolated per executor instance.
#[derive(Default)]
pub struct CodexWebsocketSessionStore {
    sessions: Mutex<HashMap<String, Arc<CodexWebsocketSession>>>,
}

impl CodexWebsocketSessionStore {
    pub fn get_or_create(&self, session_id: &str) -> Option<Arc<CodexWebsocketSession>> {
        let session_id = session_id.trim();
        if session_id.is_empty() {
            return None;
        }
        let mut sessions = lock_recover(&self.sessions);
        Some(
            sessions
                .entry(session_id.to_owned())
                .or_insert_with(|| Arc::new(CodexWebsocketSession::new(session_id.to_owned())))
                .clone(),
        )
    }

    pub fn remove(&self, session_id: &str) -> Option<Arc<CodexWebsocketSession>> {
        lock_recover(&self.sessions).remove(session_id)
    }

    pub fn len(&self) -> usize {
        lock_recover(&self.sessions).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub async fn close(&self, session_id: &str) {
        if let Some(session) = self.remove(session_id) {
            let mut state = session.execution.lock().await;
            state.close("session_closed").await;
        }
    }

    pub async fn close_all(&self) {
        let sessions = {
            let mut sessions = lock_recover(&self.sessions);
            sessions
                .drain()
                .map(|(_, session)| session)
                .collect::<Vec<_>>()
        };
        for session in sessions {
            let mut state = session.execution.lock().await;
            state.close("shutdown").await;
        }
    }
}

impl fmt::Debug for CodexWebsocketSessionStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodexWebsocketSessionStore")
            .field("session_count", &self.len())
            .finish()
    }
}

fn lock_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
}
