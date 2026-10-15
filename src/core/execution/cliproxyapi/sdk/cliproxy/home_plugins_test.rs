// ref: sdk/cliproxy/home_plugins_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};

use super::home_plugins::*;
use super::service_home::*;
use crate::internal::homeplugins::{
    current_platform, OperationContext, PluginInstallStatus, SyncError, SyncOutcome, SyncReport,
};
use crate::sdk::pluginstore::{PluginSyncRequest, PluginSyncResponse};

#[derive(Default)]
struct Context(AtomicBool);

impl OperationContext for Context {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Default)]
pub(super) struct Control {
    fetch: Mutex<Option<Result<PluginSyncResponse, HomePluginControlError>>>,
    tasks: Mutex<Vec<HomePluginTask>>,
    pushes: AtomicUsize,
    fail_pushes: AtomicUsize,
    fetches: AtomicUsize,
}

impl HomePluginControl for Control {
    fn fetch_sync(
        &self,
        _request: &mut PluginSyncRequest,
    ) -> Result<PluginSyncResponse, HomePluginControlError> {
        self.fetches.fetch_add(1, Ordering::SeqCst);
        self.fetch
            .lock()
            .unwrap()
            .take()
            .unwrap_or_else(|| Ok(PluginSyncResponse::default()))
    }

    fn push_status(&self, _report: &SyncReport) -> Result<(), HomePluginControlError> {
        let attempt = self.pushes.fetch_add(1, Ordering::SeqCst);
        if attempt < self.fail_pushes.load(Ordering::SeqCst) {
            Err(HomePluginControlError::new(
                HomePluginControlErrorKind::Rejected,
                "blocked",
            ))
        } else {
            Ok(())
        }
    }

    fn plugin_tasks(&self) -> Result<Vec<HomePluginTask>, HomePluginControlError> {
        Ok(self.tasks.lock().unwrap().clone())
    }
}

#[derive(Default)]
pub(super) struct Runtime {
    resolved: AtomicUsize,
    fallback: AtomicUsize,
    deletes: AtomicUsize,
    fail_resolved: AtomicBool,
}

impl Runtime {
    fn report(ok: bool) -> SyncReport {
        let now = DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap();
        SyncReport {
            schema_version: 1,
            task_id: 0,
            task: "plugin-sync".to_owned(),
            node_id: String::new(),
            status: if ok { "success" } else { "failed" }.to_owned(),
            phase: "install".to_owned(),
            ok,
            started_at: now,
            finished_at: Some(now),
            updated_at: now,
            platform: current_platform(),
            plugins: Vec::<PluginInstallStatus>::new(),
            error: if ok {
                String::new()
            } else {
                "failed".to_owned()
            },
        }
    }
}

impl HomePluginRuntime for Runtime {
    fn empty_report(&self) -> SyncReport {
        let mut report = Self::report(true);
        report.task.clear();
        report
    }

    fn installed_versions(
        &self,
        _config: &HomePluginConfigSnapshot,
    ) -> Result<HashMap<String, String>, SyncError> {
        Ok(HashMap::from([("sample".to_owned(), "1.0.0".to_owned())]))
    }

    fn sync_resolved(
        &self,
        _context: &dyn OperationContext,
        _config: &HomePluginConfigSnapshot,
        _response: &mut PluginSyncResponse,
        _installed_versions: &HashMap<String, String>,
    ) -> SyncOutcome {
        self.resolved.fetch_add(1, Ordering::SeqCst);
        if self.fail_resolved.load(Ordering::SeqCst) {
            let error = SyncError::new("plugin runtime sync failed");
            return SyncOutcome {
                report: Self::report(false),
                error: Some(error),
            };
        }
        SyncOutcome {
            report: Self::report(true),
            error: None,
        }
    }

    fn sync_fallback(
        &self,
        _context: &dyn OperationContext,
        _config: &HomePluginConfigSnapshot,
    ) -> SyncOutcome {
        self.fallback.fetch_add(1, Ordering::SeqCst);
        SyncOutcome {
            report: Self::report(true),
            error: None,
        }
    }

    fn completed_report(&self, error: Option<SyncError>) -> SyncReport {
        Self::report(error.is_none())
    }

    fn delete_with_report(
        &self,
        _context: &dyn OperationContext,
        _config: &HomePluginConfigSnapshot,
        task: &HomePluginTask,
    ) -> SyncReport {
        self.deletes.fetch_add(1, Ordering::SeqCst);
        let mut report = Self::report(true);
        report.task_id = task.id;
        report
    }
}

pub(super) fn config() -> HomePluginConfigSnapshot {
    HomePluginConfigSnapshot {
        home_enabled: true,
        node_id: "node-1".to_owned(),
        plugins_enabled: true,
        plugins_dir: " plugins ".to_owned(),
        ..HomePluginConfigSnapshot::default()
    }
}

#[derive(Default)]
pub(super) struct ConfigAuthority {
    pub stages: AtomicUsize,
    pub commits: AtomicUsize,
    pub runtime_applies: AtomicUsize,
    pub fail_stage: AtomicBool,
    pub commit_revisions: Mutex<Vec<u64>>,
}

impl HomeConfigAuthority for ConfigAuthority {
    fn stage(
        &self,
        context: &HomeCancellation,
        input: &HomeOverlayInput,
    ) -> Result<HomeOverlaySnapshot, HomeLifecycleError> {
        self.stages.fetch_add(1, Ordering::SeqCst);
        if context.is_cancelled() {
            return Err(HomeLifecycleError::new(
                HomeLifecycleErrorKind::Cancelled,
                "cancelled",
            ));
        }
        if self.fail_stage.load(Ordering::SeqCst) {
            return Err(HomeLifecycleError::new(
                HomeLifecycleErrorKind::Stage,
                "stage failed",
            ));
        }
        Ok(HomeOverlaySnapshot {
            revision: input.revision,
            payload: input.payload.clone(),
            plugin_config: input.plugin_config.clone(),
            observation_barrier_revision: input.observation_barrier_revision,
            runtime_config: None,
        })
    }

    fn commit(
        &self,
        context: &HomeCancellation,
        snapshot: &HomeOverlaySnapshot,
    ) -> Result<HomeConfigCommit, HomeLifecycleError> {
        if context.is_cancelled() {
            return Err(HomeLifecycleError::new(
                HomeLifecycleErrorKind::Cancelled,
                "cancelled",
            ));
        }
        self.commits.fetch_add(1, Ordering::SeqCst);
        self.commit_revisions
            .lock()
            .unwrap()
            .push(snapshot.revision);
        Ok(HomeConfigCommit {
            revision: snapshot.revision,
            runtime_revision: snapshot.revision,
            runtime_config: snapshot.runtime_config.clone(),
        })
    }

    fn apply_runtime(
        &self,
        context: &HomeCancellation,
        _commit: &HomeConfigCommit,
    ) -> Result<(), HomeLifecycleError> {
        if context.is_cancelled() {
            return Err(HomeLifecycleError::new(
                HomeLifecycleErrorKind::Cancelled,
                "cancelled",
            ));
        }
        self.runtime_applies.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

pub(super) struct Retry {
    pub allow: AtomicBool,
    pub waits: AtomicUsize,
}

impl Default for Retry {
    fn default() -> Self {
        Self {
            allow: AtomicBool::new(false),
            waits: AtomicUsize::new(0),
        }
    }
}

impl HomeRetryPolicy for Retry {
    fn wait(&self, context: &HomeCancellation, _attempt: u32) -> bool {
        self.waits.fetch_add(1, Ordering::SeqCst);
        self.allow.load(Ordering::SeqCst) && !context.is_cancelled()
    }
}

#[derive(Default)]
pub(super) struct Publisher {
    pub publishes: AtomicUsize,
    pub stops: Arc<AtomicUsize>,
}

struct Published(Arc<AtomicUsize>);

impl HomePublishedLifetime for Published {
    fn stop_and_wait(
        &self,
        _budget: super::executionregistry::WaitBudget,
    ) -> Result<(), HomeLifecycleError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

impl HomePublisherAuthority for Publisher {
    fn publish(
        &self,
        _lease: HomePublisherLease,
    ) -> Result<Arc<dyn HomePublishedLifetime>, HomeLifecycleError> {
        self.publishes.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(Published(self.stops.clone())))
    }
}

pub(super) fn input(revision: u64) -> HomeOverlayInput {
    HomeOverlayInput {
        revision,
        payload: format!("revision={revision}").into_bytes(),
        plugin_config: config(),
        observation_barrier_revision: revision as i64,
    }
}

#[derive(Default)]
struct BlockingControl {
    entered: AtomicBool,
    gate: Mutex<bool>,
    changed: Condvar,
}

impl BlockingControl {
    fn wait_until_entered(&self) {
        let started = std::time::Instant::now();
        while !self.entered.load(Ordering::Acquire) {
            assert!(started.elapsed() < Duration::from_secs(2));
            std::thread::yield_now();
        }
    }

    fn release(&self) {
        *self.gate.lock().unwrap() = true;
        self.changed.notify_all();
    }
}

impl HomePluginControl for BlockingControl {
    fn fetch_sync(
        &self,
        _request: &mut PluginSyncRequest,
    ) -> Result<PluginSyncResponse, HomePluginControlError> {
        Ok(PluginSyncResponse::default())
    }

    fn push_status(&self, _report: &SyncReport) -> Result<(), HomePluginControlError> {
        self.entered.store(true, Ordering::Release);
        let mut released = self.gate.lock().unwrap();
        while !*released {
            released = self.changed.wait(released).unwrap();
        }
        Ok(())
    }

    fn plugin_tasks(&self) -> Result<Vec<HomePluginTask>, HomePluginControlError> {
        Ok(Vec::new())
    }
}

// Upstream's `forceHomeRuntimeConfig` clears a raw `Plugins.StoreAuth` field.
// CTOX's `HomeOverlayInput` cannot carry credential material at all; secret
// authority remains outside this coordinator, so that literal mutation test is
// a reasoned exclusion rather than a synthetic field added for parity.
