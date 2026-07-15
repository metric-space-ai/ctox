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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use anyhow::Context;
use anyhow::Result;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::web_stack::spawn_persistent_browser;
use crate::web_stack::PersistentBrowserHandle;
use crate::web_stack::PersistentBrowserSpawn;

/// One live browser process plus its viewport, guarded so only one request runs
/// against the process at a time. The handle does blocking stdin/stdout IO, so
/// every access happens inside `spawn_blocking`.
pub struct LiveBrowserSession {
    handle: Mutex<PersistentBrowserHandle>,
    profile_key: String,
    pub viewport_w: u64,
    pub viewport_h: u64,
    pub downloads_dir: PathBuf,
    pub owner_user_id: String,
    pub root: PathBuf,
    last_input_seq: AtomicU64,
    clipboard: Mutex<Option<(String, Instant)>>,
}

impl LiveBrowserSession {
    pub fn record_input_seq(&self, seq: u64) {
        self.last_input_seq.fetch_max(seq, Ordering::Relaxed);
    }

    pub fn last_input_seq(&self) -> u64 {
        self.last_input_seq.load(Ordering::Relaxed)
    }

    pub fn set_clipboard(&self, value: String) {
        if let Ok(mut clipboard) = self.clipboard.lock() {
            *clipboard = Some((value, Instant::now()));
        }
    }

    pub fn clipboard(&self) -> Option<String> {
        let mut clipboard = self.clipboard.lock().ok()?;
        let Some((value, created_at)) = clipboard.as_ref() else {
            return None;
        };
        if created_at.elapsed() > Duration::from_secs(60) {
            *clipboard = None;
            return None;
        }
        Some(value.clone())
    }

    pub fn clear_clipboard(&self) {
        if let Ok(mut clipboard) = self.clipboard.lock() {
            *clipboard = None;
        }
    }
}

/// Process registry keyed by `session_id`.
pub struct BrowserRuntimeManager {
    sessions: Mutex<HashMap<String, Arc<LiveBrowserSession>>>,
    crash_history: Mutex<HashMap<String, Vec<Instant>>>,
    spawn_lock: tokio::sync::Mutex<()>,
}

static MANAGER: OnceLock<BrowserRuntimeManager> = OnceLock::new();

/// Global, lazily created manager shared by the command consumer and the
/// maintenance loop.
pub fn browser_runtime_manager() -> &'static BrowserRuntimeManager {
    MANAGER.get_or_init(BrowserRuntimeManager::new)
}

fn browser_profile_key(profile_owner: &str, session_id: &str, private_profile: bool) -> String {
    if private_profile || session_id.starts_with("browser_session_web_stack_auth_") {
        format!("{profile_owner}:{session_id}")
    } else {
        profile_owner.to_string()
    }
}

#[derive(Debug, Clone)]
pub struct BrowserSessionAutomationRequest {
    pub session_id: String,
    pub dir: Option<PathBuf>,
    pub timeout_ms: Option<u64>,
    pub source: String,
}

impl BrowserRuntimeManager {
    fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            crash_history: Mutex::new(HashMap::new()),
            spawn_lock: tokio::sync::Mutex::new(()),
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
        profile_owner: &str,
        private_profile: bool,
    ) -> Result<Arc<LiveBrowserSession>> {
        if let Some(session) = self.get(session_id) {
            return Ok(session);
        }
        // Chromium permits only one live process per persistent profile. Session
        // starts can race (or replace a disconnected logical session), so make
        // profile handoff atomic and stop the previous process before launching
        // the same tenant/user profile again.
        let _spawn_guard = self.spawn_lock.lock().await;
        if let Some(session) = self.get(session_id) {
            return Ok(session);
        }
        let profile_key = browser_profile_key(profile_owner, session_id, private_profile);
        let conflicting_session_ids = self
            .sessions
            .lock()
            .map(|sessions| {
                sessions
                    .iter()
                    .filter_map(|(id, session)| {
                        (session.profile_key == profile_key).then_some(id.clone())
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for conflicting_session_id in conflicting_session_ids {
            self.stop(&conflicting_session_id).await;
        }
        if let Ok(mut history) = self.crash_history.lock() {
            let crashes = history.entry(session_id.to_string()).or_default();
            crashes.retain(|at| at.elapsed() < Duration::from_secs(5 * 60));
            anyhow::ensure!(crashes.len() < 3, "browser crash-loop protection is active");
        }
        let max_sessions = crate::inference::runtime_env::get_runtime_env_value(
            &root,
            "CTOX_BROWSER_MAX_SESSIONS",
        )
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(16)
        .clamp(1, 128);
        let max_sessions_per_user = crate::inference::runtime_env::get_runtime_env_value(
            &root,
            "CTOX_BROWSER_MAX_SESSIONS_PER_USER",
        )
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(3)
        .clamp(1, 16);
        if let Ok(sessions) = self.sessions.lock() {
            anyhow::ensure!(
                sessions.len() < max_sessions,
                "browser session budget is exhausted"
            );
            anyhow::ensure!(
                sessions
                    .values()
                    .filter(|session| session.owner_user_id == profile_owner)
                    .count()
                    < max_sessions_per_user,
                "browser session budget for this user is exhausted"
            );
        }
        let digest = Sha256::digest(profile_key.as_bytes());
        let profile_kind = if private_profile {
            "private"
        } else {
            "profiles"
        };
        let profile_dir = Some(
            crate::paths::runtime_dir(&root)
                .join("browser")
                .join(profile_kind)
                .join(format!("{:x}", digest)),
        );
        let downloads_dir = crate::paths::runtime_dir(&root)
            .join("browser/downloads")
            .join(format!("{:x}", digest));
        let spawn = PersistentBrowserSpawn {
            dir,
            viewport_w,
            viewport_h,
            profile_dir,
            private_profile,
            egress_allow_hosts: ctox_web_stack::browser_egress_allow_hosts_from_config(&root),
            downloads_dir: Some(downloads_dir.clone()),
        };
        let session_root = root.clone();
        let handle = tokio::task::spawn_blocking(move || spawn_persistent_browser(&root, &spawn))
            .await
            .context("browser runtime spawn worker panicked")??;
        let session = Arc::new(LiveBrowserSession {
            handle: Mutex::new(handle),
            profile_key,
            viewport_w,
            viewport_h,
            downloads_dir,
            owner_user_id: profile_owner.to_string(),
            root: session_root,
            last_input_seq: AtomicU64::new(0),
            clipboard: Mutex::new(None),
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

    /// Send one operation with a process-level deadline. This is used for
    /// operator-provided automation where page code or browser cleanup can
    /// otherwise leave the daemon blocked indefinitely.
    pub async fn request_with_timeout(
        &self,
        session: &Arc<LiveBrowserSession>,
        op: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value> {
        let session = Arc::clone(session);
        let op = op.to_string();
        tokio::task::spawn_blocking(move || {
            let mut handle = session
                .handle
                .lock()
                .map_err(|_| anyhow::anyhow!("browser runtime handle poisoned"))?;
            handle.request_with_timeout(&op, params, timeout)
        })
        .await
        .context("browser runtime timed request worker panicked")?
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

    pub fn drop_session_after_crash(&self, session_id: &str) {
        self.drop_session(session_id);
        if let Ok(mut history) = self.crash_history.lock() {
            history
                .entry(session_id.to_string())
                .or_default()
                .push(Instant::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_stack_auth_sessions_use_source_scoped_profiles() {
        let owner = "ctox";
        let dnb = "browser_session_web_stack_auth_dnbhoovers-com";
        let xing = "browser_session_web_stack_auth_xing-com";

        assert_ne!(
            Sha256::digest(browser_profile_key(owner, dnb, false)),
            Sha256::digest(browser_profile_key(owner, xing, false))
        );
        assert_eq!(
            browser_profile_key(owner, "browser_session_regular", false),
            owner
        );
    }
}
