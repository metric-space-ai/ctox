// Origin: CTOX
// License: AGPL-3.0-only

//! Native session registry for persistent remote-browser runtimes.
//!
//! Each Business OS `browser_sessions` row that is `active` is backed here by a
//! long-lived Chromium/Patchright process (see
//! [`crate::web_stack::spawn_persistent_browser`]). The native RxDB peer drives
//! these sessions: lifecycle commands (`browser.session.start`,
//! `browser.navigate`, ...) and the periodic input/frame maintenance loop both
//! route through this manager. The manager owns no RxDB state; it only owns the
//! live processes and serializes access to each one.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;

use anyhow::Context;
use anyhow::Result;
use serde_json::Value;

use crate::web_stack::spawn_persistent_browser;
use crate::web_stack::PersistentBrowserHandle;
use crate::web_stack::PersistentBrowserSpawn;

/// One live browser process plus its viewport, guarded so only one request runs
/// against the process at a time. The handle does blocking stdin/stdout IO, so
/// every access happens inside `spawn_blocking`.
pub struct LiveBrowserSession {
    handle: Mutex<PersistentBrowserHandle>,
    pub viewport_w: u64,
    pub viewport_h: u64,
}

/// Process registry keyed by `session_id`.
pub struct BrowserRuntimeManager {
    sessions: Mutex<HashMap<String, Arc<LiveBrowserSession>>>,
}

static MANAGER: OnceLock<BrowserRuntimeManager> = OnceLock::new();

/// Global, lazily created manager shared by the command consumer and the
/// maintenance loop.
pub fn browser_runtime_manager() -> &'static BrowserRuntimeManager {
    MANAGER.get_or_init(BrowserRuntimeManager::new)
}

impl BrowserRuntimeManager {
    fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// Return an existing live session, if any.
    pub fn get(&self, session_id: &str) -> Option<Arc<LiveBrowserSession>> {
        self.sessions.lock().ok()?.get(session_id).cloned()
    }

    /// True when a live process currently backs `session_id`.
    pub fn has_session(&self, session_id: &str) -> bool {
        self.sessions
            .lock()
            .map(|map| map.contains_key(session_id))
            .unwrap_or(false)
    }

    /// All currently live session ids.
    pub fn active_session_ids(&self) -> Vec<String> {
        self.sessions
            .lock()
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Return the existing session or spawn a fresh persistent runtime for it.
    pub async fn ensure_session(
        &self,
        root: PathBuf,
        dir: Option<PathBuf>,
        session_id: &str,
        viewport_w: u64,
        viewport_h: u64,
    ) -> Result<Arc<LiveBrowserSession>> {
        if let Some(session) = self.get(session_id) {
            return Ok(session);
        }
        let spawn = PersistentBrowserSpawn {
            dir,
            viewport_w,
            viewport_h,
        };
        let handle = tokio::task::spawn_blocking(move || spawn_persistent_browser(&root, &spawn))
            .await
            .context("browser runtime spawn worker panicked")??;
        let session = Arc::new(LiveBrowserSession {
            handle: Mutex::new(handle),
            viewport_w,
            viewport_h,
        });
        if let Ok(mut map) = self.sessions.lock() {
            map.insert(session_id.to_string(), Arc::clone(&session));
        }
        Ok(session)
    }

    /// Send one operation to a live session and await its JSON response.
    pub async fn request(
        &self,
        session: &Arc<LiveBrowserSession>,
        op: &str,
        params: Value,
    ) -> Result<Value> {
        let session = Arc::clone(session);
        let op = op.to_string();
        tokio::task::spawn_blocking(move || {
            let mut handle = session
                .handle
                .lock()
                .map_err(|_| anyhow::anyhow!("browser runtime handle poisoned"))?;
            handle.request(&op, params)
        })
        .await
        .context("browser runtime request worker panicked")?
    }

    /// Drop a session from the registry and shut its process down gracefully.
    pub async fn stop(&self, session_id: &str) {
        let removed = self
            .sessions
            .lock()
            .ok()
            .and_then(|mut map| map.remove(session_id));
        if let Some(session) = removed {
            let _ = tokio::task::spawn_blocking(move || {
                if let Ok(mut handle) = session.handle.lock() {
                    handle.shutdown();
                }
            })
            .await;
        }
    }

    /// Forget a session without a graceful close (used when the process is
    /// already dead). The `Drop` impl on the handle still kills any remnant.
    pub fn drop_session(&self, session_id: &str) {
        if let Ok(mut map) = self.sessions.lock() {
            map.remove(session_id);
        }
    }
}
