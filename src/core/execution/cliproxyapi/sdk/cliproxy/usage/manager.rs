// ref: sdk/cliproxy/usage/manager.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use crate::internal::logging::RequestContext;

use super::Detail;

pub const DEFAULT_SERVICE_TIER: &str = "default";
pub const AUTO_SERVICE_TIER: &str = "auto";

/// Typed replacement for Go's free-form `context.WithValue` usage.
#[derive(Clone, Default)]
pub struct UsageContext {
    pub request: RequestContext,
    requested_model_alias: Option<String>,
    reasoning_effort: Option<String>,
    service_tier: Option<String>,
    generate: Option<bool>,
}

impl UsageContext {
    #[must_use]
    pub fn from_request(request: RequestContext) -> Self {
        Self {
            request,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn with_requested_model_alias(mut self, alias: impl Into<String>) -> Self {
        self.requested_model_alias = non_empty(alias.into());
        self
    }

    #[must_use]
    pub fn requested_model_alias(&self) -> &str {
        self.requested_model_alias.as_deref().unwrap_or_default()
    }

    #[must_use]
    pub fn with_reasoning_effort(mut self, effort: impl Into<String>) -> Self {
        self.reasoning_effort = non_empty(effort.into());
        self
    }

    #[must_use]
    pub fn reasoning_effort(&self) -> &str {
        self.reasoning_effort.as_deref().unwrap_or_default()
    }

    #[must_use]
    pub fn with_service_tier(mut self, tier: impl Into<String>) -> Self {
        let tier = tier.into();
        self.service_tier =
            Some(non_empty(tier).unwrap_or_else(|| DEFAULT_SERVICE_TIER.to_owned()));
        self
    }

    #[must_use]
    pub fn service_tier(&self) -> &str {
        self.service_tier.as_deref().unwrap_or(DEFAULT_SERVICE_TIER)
    }

    #[must_use]
    pub fn with_generate(mut self, generate: bool) -> Self {
        self.generate = Some(generate);
        self
    }

    #[must_use]
    pub fn generate(&self) -> bool {
        generate_enabled(self.generate)
    }
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[derive(Clone, Default, Eq, PartialEq)]
pub struct Failure {
    pub status_code: i32,
    pub body: String,
}

/// One provider attempt. This deliberately has no `Debug` implementation:
/// `api_key` may contain provider credentials and must never enter diagnostics.
#[derive(Clone, Default)]
pub struct Record {
    pub provider: String,
    pub executor_type: String,
    pub model: String,
    pub alias: String,
    pub api_key: String,
    pub auth_id: String,
    pub auth_index: String,
    /// Identifies an OAuth token version without retaining the credential.
    pub access_token_sha256: String,
    pub auth_type: String,
    pub source: String,
    pub reasoning_effort: String,
    pub service_tier: String,
    pub request_service_tier: String,
    pub response_service_tier: String,
    pub generate: Option<bool>,
    pub requested_at: Option<SystemTime>,
    pub latency: Duration,
    pub ttft: Duration,
    pub failed: bool,
    pub fail: Failure,
    pub detail: Detail,
    pub response_headers: BTreeMap<String, Vec<String>>,
}

#[must_use]
pub const fn generate_flag(generate: bool) -> Option<bool> {
    Some(generate)
}

#[must_use]
pub const fn generate_enabled(generate: Option<bool>) -> bool {
    match generate {
        Some(value) => value,
        None => true,
    }
}

pub trait Plugin: Send + Sync + 'static {
    fn handle_usage(&self, context: &UsageContext, record: &Record);
}

struct QueueItem {
    context: UsageContext,
    record: Record,
}

struct QueueState {
    queue: VecDeque<QueueItem>,
    closed: bool,
}

struct Shared {
    queue: Mutex<QueueState>,
    ready: Condvar,
    plugins: RwLock<Vec<Arc<dyn Plugin>>>,
    named: Mutex<HashMap<String, usize>>,
    plugin_panics: AtomicU64,
}

/// Instance-owned usage dispatcher. CTOX deliberately does not reproduce the
/// upstream package-global manager: lifecycle and plugin authority belong to
/// the embedding runtime instance.
pub struct Manager {
    shared: Arc<Shared>,
    worker: Mutex<Option<JoinHandle<()>>>,
    stopped: AtomicBool,
}

impl Manager {
    #[must_use]
    pub fn new(buffer: usize) -> Self {
        Self {
            shared: Arc::new(Shared {
                queue: Mutex::new(QueueState {
                    queue: VecDeque::with_capacity(buffer),
                    closed: false,
                }),
                ready: Condvar::new(),
                plugins: RwLock::new(Vec::new()),
                named: Mutex::new(HashMap::new()),
                plugin_panics: AtomicU64::new(0),
            }),
            worker: Mutex::new(None),
            stopped: AtomicBool::new(false),
        }
    }

    /// Starts the dispatcher once. `publish` also starts it lazily.
    pub fn start(&self) {
        if self.stopped.load(Ordering::Acquire) {
            return;
        }
        self.start_worker();
    }

    fn start_worker(&self) {
        let mut worker = lock_unpoisoned(&self.worker);
        // stop() may close publication while start() waits for this lock.
        // Recheck under the lock so every spawned worker is joined by stop().
        if self.stopped.load(Ordering::Acquire) || worker.is_some() {
            return;
        }
        let shared = Arc::clone(&self.shared);
        *worker = Some(
            thread::Builder::new()
                .name("ctox-cliproxy-usage".to_owned())
                .spawn(move || run(shared))
                .expect("failed to spawn usage dispatcher"),
        );
    }

    pub fn register(&self, plugin: Arc<dyn Plugin>) {
        write_unpoisoned(&self.shared.plugins).push(plugin);
    }

    pub fn register_named(&self, name: &str, plugin: Arc<dyn Plugin>) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let mut named = lock_unpoisoned(&self.shared.named);
        let mut plugins = write_unpoisoned(&self.shared.plugins);
        if let Some(index) = named.get(name).copied() {
            if let Some(slot) = plugins.get_mut(index) {
                *slot = plugin;
                return;
            }
        }
        named.insert(name.to_owned(), plugins.len());
        plugins.push(plugin);
    }

    pub fn publish(&self, context: UsageContext, record: Record) -> bool {
        self.start();
        let mut state = lock_unpoisoned(&self.shared.queue);
        if state.closed {
            return false;
        }
        state.queue.push_back(QueueItem { context, record });
        drop(state);
        self.shared.ready.notify_one();
        true
    }

    /// Closes publication, drains queued records, and joins the worker.
    pub fn stop(&self) {
        if self.stopped.swap(true, Ordering::AcqRel) {
            return;
        }
        {
            let mut state = lock_unpoisoned(&self.shared.queue);
            state.closed = true;
        }
        self.shared.ready.notify_all();
        if let Some(worker) = lock_unpoisoned(&self.worker).take() {
            let _ = worker.join();
        }
    }

    #[must_use]
    pub fn plugin_panic_count(&self) -> u64 {
        self.shared.plugin_panics.load(Ordering::Acquire)
    }
}

impl Drop for Manager {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run(shared: Arc<Shared>) {
    loop {
        let item = {
            let mut state = lock_unpoisoned(&shared.queue);
            while state.queue.is_empty() && !state.closed {
                state = shared
                    .ready
                    .wait(state)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
            match state.queue.pop_front() {
                Some(item) => item,
                None if state.closed => return,
                None => continue,
            }
        };
        let plugins = read_unpoisoned(&shared.plugins).clone();
        for plugin in plugins {
            if catch_unwind(AssertUnwindSafe(|| {
                plugin.handle_usage(&item.context, &item.record);
            }))
            .is_err()
            {
                shared.plugin_panics.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

fn lock_unpoisoned<T>(lock: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    lock.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn read_unpoisoned<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write_unpoisoned<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod candidate_lifecycle_tests {
    use super::{lock_unpoisoned, Manager, Record, UsageContext};
    use std::sync::atomic::Ordering;

    #[test]
    fn candidate_usage_dispatcher_stale_start_cannot_reopen_after_stop() {
        let manager = Manager::new(1);
        // Replay a start that observed open publication before stop(),
        // but only acquired the worker lock after stop() returned.
        assert!(!manager.stopped.load(Ordering::Acquire));
        manager.stop();
        manager.start_worker();
        assert!(!manager.publish(UsageContext::default(), Record::default()));
        assert!(lock_unpoisoned(&manager.worker).is_none());
    }
}
