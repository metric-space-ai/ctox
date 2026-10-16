// ref: sdk/cliproxy/service_executionregistry_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

//! Semantic disposition of the 2,984-line upstream service integration suite.
//!
//! Registry-owned dispatch, close/release concurrency, observations and
//! barriers are also tested under `executionregistry/`. This mirror covers the
//! registry-facing contract through the real instance-owned `Registry`,
//! `service_home` coordinator and `ServiceRuntimeGraph`: generation fencing,
//! FIFO config work, explicit drain, safe in-flight failover, reusable log
//! forwarding, selector identity and serialized Home/watcher runtime apply.

use crate::internal::config::ValidatedRuntimeConfig;
use chrono::{TimeZone, Utc};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use super::auth::{CooldownStateRecord, CooldownStateStore, CooldownStoreError, SchedulerStrategy};
use super::executionregistry::{Registry, ScopeSpec, WaitBudget};
use super::home_plugins_test::{input, ConfigAuthority, Control, Publisher, Retry, Runtime};
use super::service_home::{
    HomeCancellation, HomeConfigAuthority, HomeConfigCommit, HomeConfigWorkQueue,
    HomeLifecycleCoordinator, HomeLifecycleError, HomeLifecycleErrorKind, HomeOverlayInput,
    HomeOverlaySnapshot, HomeReplacementMode,
};
use super::service_runtime::{HomeLogBinding, HomeLogForwarder, ServiceRuntimeGraph};
use super::service_test_support::{runtime_fixture, validated_config};

#[test]
fn service_registry_instances_do_not_share_barriers_or_executions() {
    let first = Registry::new();
    let second = Registry::new();
    let pending = first.begin_dispatch().unwrap();
    first.observe_barrier(14);
    let scope = first
        .install(
            &pending,
            ScopeSpec {
                request_id: "request".into(),
                credential_id: "credential".into(),
                model: "model".into(),
                kind: "http".into(),
                started_at: Utc.timestamp_opt(1, 0).unwrap(),
                accounted: true,
            },
        )
        .unwrap();

    let first_freeze = first.freeze_in_flight(Utc.timestamp_opt(2, 0).unwrap());
    let second_freeze = second.freeze_in_flight(Utc.timestamp_opt(2, 0).unwrap());
    assert_eq!(first_freeze.barrier_revision, 14);
    assert_eq!(first_freeze.executions.len(), 1);
    assert_eq!(second_freeze.barrier_revision, 0);
    assert!(second_freeze.executions.is_empty());
    scope.end("complete");
}

#[test]
fn replacement_fences_staged_pre_ack_work_before_commit() {
    let config = Arc::new(ConfigAuthority::default());
    let control = Arc::new(Control::default());
    let runtime = Arc::new(Runtime::default());
    let publisher = Arc::new(Publisher::default());
    let retry = Arc::new(Retry::default());
    let coordinator = HomeLifecycleCoordinator::new(
        config.clone(),
        control.clone(),
        runtime.clone(),
        publisher.clone(),
        retry.clone(),
    );
    let first = coordinator
        .start_lifetime(HomeReplacementMode::Drain, WaitBudget::unbounded())
        .unwrap();
    let mut staged = coordinator.stage_until_ready(&first, 1, &input(1)).unwrap();
    let second = coordinator
        .start_lifetime(
            HomeReplacementMode::PreserveInFlight,
            WaitBudget::unbounded(),
        )
        .unwrap();
    assert!(coordinator
        .commit_finalize_until_done(&first, &mut staged)
        .is_err());
    assert_eq!(config.commits.load(Ordering::SeqCst), 0);
    assert_eq!(coordinator.active_generation(), Some(second.generation()));
}

#[test]
fn explicit_replacement_waits_for_pending_and_active_execution_drain() {
    let config = Arc::new(ConfigAuthority::default());
    let control = Arc::new(Control::default());
    let runtime = Arc::new(Runtime::default());
    let publisher = Arc::new(Publisher::default());
    let retry = Arc::new(Retry::default());
    let coordinator = HomeLifecycleCoordinator::new(
        config.clone(),
        control.clone(),
        runtime.clone(),
        publisher.clone(),
        retry.clone(),
    );
    let first = coordinator
        .start_lifetime(HomeReplacementMode::Drain, WaitBudget::unbounded())
        .unwrap();
    let pending = first.registry().begin_dispatch().unwrap();
    let scope = first
        .registry()
        .install(
            &pending,
            ScopeSpec {
                request_id: "request".into(),
                credential_id: "credential".into(),
                model: "model".into(),
                kind: "stream".into(),
                started_at: Utc.timestamp_opt(1, 0).unwrap(),
                accounted: true,
            },
        )
        .unwrap();
    std::thread::scope(|thread| {
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let coordinator = &coordinator;
        thread.spawn(move || {
            let result = coordinator.start_lifetime(
                HomeReplacementMode::Drain,
                WaitBudget::for_duration(Duration::from_secs(2)),
            );
            done_tx.send(result).unwrap();
        });
        while !first.cancellation().is_cancelled() {
            std::thread::yield_now();
        }
        assert!(done_rx.try_recv().is_err());
        scope.end("complete");
        assert!(done_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .is_ok());
    });
}

#[test]
fn safe_failover_preserves_registry_scope_and_stops_acked_publisher() {
    let config = Arc::new(ConfigAuthority::default());
    let control = Arc::new(Control::default());
    let runtime = Arc::new(Runtime::default());
    let publisher = Arc::new(Publisher::default());
    let retry = Arc::new(Retry::default());
    let coordinator = HomeLifecycleCoordinator::new(
        config.clone(),
        control.clone(),
        runtime.clone(),
        publisher.clone(),
        retry.clone(),
    );
    let first = coordinator
        .start_lifetime(HomeReplacementMode::Drain, WaitBudget::unbounded())
        .unwrap();
    let pending = first.registry().begin_dispatch().unwrap();
    let scope = first
        .registry()
        .install(&pending, ScopeSpec::default())
        .unwrap();
    let mut work = coordinator.stage_until_ready(&first, 1, &input(1)).unwrap();
    coordinator
        .commit_finalize_until_done(&first, &mut work)
        .unwrap();
    let second = coordinator
        .start_lifetime(
            HomeReplacementMode::PreserveInFlight,
            WaitBudget::unbounded(),
        )
        .unwrap();
    assert!(Arc::ptr_eq(&first.registry(), &second.registry()));
    assert_eq!(publisher.stops.load(Ordering::SeqCst), 1);
    assert_eq!(
        second
            .registry()
            .freeze_in_flight(Utc.timestamp_opt(2, 0).unwrap())
            .executions
            .len(),
        1
    );
    scope.end("complete");
}

#[test]
fn observation_barrier_waits_for_pre_barrier_pending_dispatch() {
    let config = Arc::new(ConfigAuthority::default());
    let control = Arc::new(Control::default());
    let runtime = Arc::new(Runtime::default());
    let publisher = Arc::new(Publisher::default());
    let retry = Arc::new(Retry::default());
    let coordinator = HomeLifecycleCoordinator::new(
        config.clone(),
        control.clone(),
        runtime.clone(),
        publisher.clone(),
        retry.clone(),
    );
    let lifetime = coordinator
        .start_lifetime(HomeReplacementMode::Drain, WaitBudget::unbounded())
        .unwrap();
    let pending = lifetime.registry().begin_dispatch().unwrap();
    coordinator.observe_barrier(&lifetime, 14).unwrap();
    assert_eq!(
        lifetime
            .registry()
            .freeze_in_flight(Utc.timestamp_opt(1, 0).unwrap())
            .barrier_revision,
        0
    );
    pending.end();
    assert_eq!(
        lifetime
            .registry()
            .freeze_in_flight(Utc.timestamp_opt(2, 0).unwrap())
            .barrier_revision,
        14
    );
}

#[test]
fn rapid_config_work_commits_in_fifo_order_and_publishes_once() {
    let config = Arc::new(ConfigAuthority::default());
    let control = Arc::new(Control::default());
    let runtime = Arc::new(Runtime::default());
    let publisher = Arc::new(Publisher::default());
    let retry = Arc::new(Retry::default());
    let coordinator = HomeLifecycleCoordinator::new(
        config.clone(),
        control.clone(),
        runtime.clone(),
        publisher.clone(),
        retry.clone(),
    );
    let lifetime = coordinator
        .start_lifetime(HomeReplacementMode::Drain, WaitBudget::unbounded())
        .unwrap();
    let queue = HomeConfigWorkQueue::default();
    queue.enqueue(input(1));
    queue.enqueue(input(2));
    while let Some((sequence, input)) = queue.dequeue(lifetime.cancellation().as_ref()) {
        let mut work = coordinator
            .stage_until_ready(&lifetime, sequence, &input)
            .unwrap();
        coordinator
            .commit_finalize_until_done(&lifetime, &mut work)
            .unwrap();
    }
    assert_eq!(*config.commit_revisions.lock().unwrap(), vec![1, 2]);
    assert_eq!(publisher.publishes.load(Ordering::SeqCst), 1);

    let mut duplicate = coordinator
        .stage_until_ready(&lifetime, 2, &input(2))
        .unwrap();
    coordinator
        .commit_finalize_until_done(&lifetime, &mut duplicate)
        .unwrap();
    assert_eq!(*config.commit_revisions.lock().unwrap(), vec![1, 2]);
    assert_eq!(publisher.publishes.load(Ordering::SeqCst), 1);
}

struct FailOnceStage {
    inner: ConfigAuthority,
    failed: std::sync::atomic::AtomicBool,
}

impl Default for FailOnceStage {
    fn default() -> Self {
        Self {
            inner: ConfigAuthority::default(),
            failed: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

impl HomeConfigAuthority for FailOnceStage {
    fn stage(
        &self,
        context: &HomeCancellation,
        input: &HomeOverlayInput,
    ) -> Result<HomeOverlaySnapshot, HomeLifecycleError> {
        if !self.failed.swap(true, Ordering::SeqCst) {
            self.inner.stages.fetch_add(1, Ordering::SeqCst);
            return Err(HomeLifecycleError::new(
                HomeLifecycleErrorKind::Stage,
                "transient stage failure",
            ));
        }
        self.inner.stage(context, input)
    }

    fn commit(
        &self,
        context: &HomeCancellation,
        snapshot: &HomeOverlaySnapshot,
    ) -> Result<HomeConfigCommit, HomeLifecycleError> {
        self.inner.commit(context, snapshot)
    }

    fn apply_runtime(
        &self,
        context: &HomeCancellation,
        commit: &HomeConfigCommit,
    ) -> Result<(), HomeLifecycleError> {
        self.inner.apply_runtime(context, commit)
    }
}

#[test]
fn transient_stage_failure_retries_the_same_sequence_before_commit() {
    let config = Arc::new(FailOnceStage::default());
    let control = Arc::new(Control::default());
    let runtime = Arc::new(Runtime::default());
    let publisher = Arc::new(Publisher::default());
    let retry = Arc::new(Retry::default());
    retry.allow.store(true, Ordering::SeqCst);
    let coordinator = HomeLifecycleCoordinator::new(
        config.clone(),
        control.clone(),
        runtime.clone(),
        publisher.clone(),
        retry.clone(),
    );
    let lifetime = coordinator
        .start_lifetime(HomeReplacementMode::Drain, WaitBudget::unbounded())
        .unwrap();
    let mut work = coordinator
        .stage_until_ready(&lifetime, 41, &input(9))
        .unwrap();
    assert_eq!(work.sequence, 41);
    coordinator
        .commit_finalize_until_done(&lifetime, &mut work)
        .unwrap();
    assert_eq!(config.inner.stages.load(Ordering::SeqCst), 2);
    assert_eq!(config.inner.commits.load(Ordering::SeqCst), 1);
    assert_eq!(retry.waits.load(Ordering::SeqCst), 1);
}

#[test]
fn shutdown_cancels_lifetime_and_waits_for_execution_drain() {
    let config = Arc::new(ConfigAuthority::default());
    let control = Arc::new(Control::default());
    let runtime = Arc::new(Runtime::default());
    let publisher = Arc::new(Publisher::default());
    let retry = Arc::new(Retry::default());
    let coordinator = HomeLifecycleCoordinator::new(
        config.clone(),
        control.clone(),
        runtime.clone(),
        publisher.clone(),
        retry.clone(),
    );
    let lifetime = coordinator
        .start_lifetime(HomeReplacementMode::Drain, WaitBudget::unbounded())
        .unwrap();
    let pending = lifetime.registry().begin_dispatch().unwrap();
    let scope = lifetime
        .registry()
        .install(&pending, ScopeSpec::default())
        .unwrap();
    std::thread::scope(|thread| {
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let coordinator = &coordinator;
        thread.spawn(move || {
            done_tx
                .send(coordinator.shutdown(WaitBudget::for_duration(Duration::from_secs(2))))
                .unwrap();
        });
        while !lifetime.cancellation().is_cancelled() {
            std::thread::yield_now();
        }
        assert!(done_rx.try_recv().is_err());
        scope.end("complete");
        assert!(done_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .is_ok());
    });
    assert_eq!(coordinator.active_generation(), None);
}

#[derive(Default)]
struct BlockingRuntimeConfig {
    inner: ConfigAuthority,
    entered: std::sync::atomic::AtomicBool,
    released: Mutex<bool>,
    changed: Condvar,
}

impl BlockingRuntimeConfig {
    fn wait_until_entered(&self) {
        let started = std::time::Instant::now();
        while !self.entered.load(Ordering::Acquire) {
            assert!(started.elapsed() < Duration::from_secs(2));
            std::thread::yield_now();
        }
    }

    fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.changed.notify_all();
    }
}

impl HomeConfigAuthority for BlockingRuntimeConfig {
    fn stage(
        &self,
        context: &HomeCancellation,
        input: &HomeOverlayInput,
    ) -> Result<HomeOverlaySnapshot, HomeLifecycleError> {
        self.inner.stage(context, input)
    }

    fn commit(
        &self,
        context: &HomeCancellation,
        snapshot: &HomeOverlaySnapshot,
    ) -> Result<HomeConfigCommit, HomeLifecycleError> {
        self.inner.commit(context, snapshot)
    }

    fn apply_runtime(
        &self,
        context: &HomeCancellation,
        _commit: &HomeConfigCommit,
    ) -> Result<(), HomeLifecycleError> {
        self.entered.store(true, Ordering::Release);
        let mut released = self.released.lock().unwrap();
        while !*released {
            released = self.changed.wait(released).unwrap();
        }
        if context.is_cancelled() {
            Err(HomeLifecycleError::new(
                HomeLifecycleErrorKind::Cancelled,
                "cancelled runtime apply",
            ))
        } else {
            self.inner.runtime_applies.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }
}

#[test]
fn replacement_cancels_blocked_post_commit_runtime_apply_before_publish() {
    let config = Arc::new(BlockingRuntimeConfig::default());
    let control = Arc::new(Control::default());
    let runtime = Arc::new(Runtime::default());
    let publisher = Arc::new(Publisher::default());
    let retry = Arc::new(Retry::default());
    let coordinator = HomeLifecycleCoordinator::new(
        config.clone(),
        control.clone(),
        runtime.clone(),
        publisher.clone(),
        retry.clone(),
    );
    let lifetime = coordinator
        .start_lifetime(HomeReplacementMode::Drain, WaitBudget::unbounded())
        .unwrap();
    let mut work = coordinator
        .stage_until_ready(&lifetime, 1, &input(1))
        .unwrap();
    std::thread::scope(|thread| {
        let (worker_tx, worker_rx) = std::sync::mpsc::channel();
        let coordinator_ref = &coordinator;
        let lifetime_ref = &lifetime;
        let work_ref = &mut work;
        thread.spawn(move || {
            worker_tx
                .send(coordinator_ref.commit_finalize_until_done(lifetime_ref, work_ref))
                .unwrap();
        });
        config.wait_until_entered();
        let (replace_tx, replace_rx) = std::sync::mpsc::channel();
        let coordinator_ref = &coordinator;
        thread.spawn(move || {
            replace_tx
                .send(coordinator_ref.start_lifetime(
                    HomeReplacementMode::PreserveInFlight,
                    WaitBudget::unbounded(),
                ))
                .unwrap();
        });
        while !lifetime.cancellation().is_cancelled() {
            std::thread::yield_now();
        }
        assert!(replace_rx.try_recv().is_err());
        config.release();
        assert!(worker_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .is_err());
        assert!(replace_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .is_ok());
    });
    assert_eq!(config.inner.commits.load(Ordering::SeqCst), 1);
    assert_eq!(config.inner.runtime_applies.load(Ordering::SeqCst), 0);
    assert_eq!(publisher.publishes.load(Ordering::SeqCst), 0);
}

#[derive(Default)]
struct EmptyCooldowns;

impl CooldownStateStore for EmptyCooldowns {
    fn load(&self) -> Result<Vec<CooldownStateRecord>, CooldownStoreError> {
        Ok(Vec::new())
    }

    fn save(&self, _records: &[CooldownStateRecord]) -> Result<(), CooldownStoreError> {
        Ok(())
    }
}

#[derive(Default)]
struct RecordingLogForwarder {
    binds: Mutex<Vec<u64>>,
    deactivations: Mutex<Vec<u64>>,
    stops: std::sync::atomic::AtomicUsize,
}

impl HomeLogForwarder for RecordingLogForwarder {
    fn bind(&self, binding: HomeLogBinding) -> Result<(), HomeLifecycleError> {
        assert!(Arc::strong_count(&binding.registry) >= 2);
        self.binds.lock().unwrap().push(binding.generation);
        Ok(())
    }

    fn deactivate(&self, generation: u64) {
        self.deactivations.lock().unwrap().push(generation);
    }

    fn stop(&self) {
        self.stops.fetch_add(1, Ordering::SeqCst);
    }
}

fn config_with_strategy(strategy: SchedulerStrategy) -> ValidatedRuntimeConfig {
    let mut config = validated_config().into_config();
    config.routing_strategy = strategy;
    config.validate().unwrap()
}

fn runtime_graph(
    home_config: Arc<dyn HomeConfigAuthority>,
    logs: Arc<RecordingLogForwarder>,
) -> Arc<ServiceRuntimeGraph> {
    let fixture = runtime_fixture(None);
    Arc::new(ServiceRuntimeGraph::new(
        validated_config(),
        fixture.runtime,
        Arc::new(EmptyCooldowns),
        home_config,
        Arc::new(Control::default()),
        Arc::new(Runtime::default()),
        Arc::new(Publisher::default()),
        Arc::new(Retry::default()),
        logs,
    ))
}

#[test]
fn service_runtime_graph_reuses_log_forwarder_and_fences_stale_generation() {
    let logs = Arc::new(RecordingLogForwarder::default());
    let graph = runtime_graph(Arc::new(ConfigAuthority::default()), logs.clone());
    let first = graph
        .start_home_lifetime(HomeReplacementMode::Drain, WaitBudget::unbounded())
        .unwrap();
    graph.apply_home_overlay(&first, 1, &input(1)).unwrap();

    let second = graph
        .start_home_lifetime(
            HomeReplacementMode::PreserveInFlight,
            WaitBudget::unbounded(),
        )
        .unwrap();
    assert!(Arc::ptr_eq(&first.registry(), &second.registry()));
    graph.apply_home_overlay(&second, 2, &input(2)).unwrap();

    assert_eq!(*logs.binds.lock().unwrap(), vec![1, 2]);
    assert_eq!(*logs.deactivations.lock().unwrap(), vec![1]);
    graph.shutdown(WaitBudget::unbounded()).unwrap();
    assert_eq!(*logs.deactivations.lock().unwrap(), vec![1, 2]);
    assert_eq!(logs.stops.load(Ordering::SeqCst), 1);
}

#[test]
fn service_runtime_graph_preserves_selector_identity_for_same_routing() {
    let graph = runtime_graph(
        Arc::new(ConfigAuthority::default()),
        Arc::new(RecordingLogForwarder::default()),
    );
    let initial = graph.selector();
    graph.apply_watcher_config(config_with_strategy(SchedulerStrategy::RoundRobin));
    assert!(Arc::ptr_eq(&initial, &graph.selector()));
    assert_eq!(graph.selector_generation(), 1);

    graph.apply_watcher_config(config_with_strategy(SchedulerStrategy::FillFirst));
    let changed = graph.selector();
    assert!(!Arc::ptr_eq(&initial, &changed));
    assert_eq!(graph.selector_generation(), 2);
    graph.apply_watcher_config(config_with_strategy(SchedulerStrategy::FillFirst));
    assert!(Arc::ptr_eq(&changed, &graph.selector()));
    assert_eq!(
        graph.current_config().config.routing_strategy(),
        SchedulerStrategy::FillFirst
    );
}

struct BlockingProjectedConfig {
    entered: std::sync::atomic::AtomicBool,
    released: Mutex<bool>,
    changed: Condvar,
}

impl BlockingProjectedConfig {
    fn new() -> Self {
        Self {
            entered: std::sync::atomic::AtomicBool::new(false),
            released: Mutex::new(false),
            changed: Condvar::new(),
        }
    }

    fn wait_until_entered(&self) {
        let started = std::time::Instant::now();
        while !self.entered.load(Ordering::Acquire) {
            assert!(started.elapsed() < Duration::from_secs(2));
            std::thread::yield_now();
        }
    }

    fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.changed.notify_all();
    }
}

impl HomeConfigAuthority for BlockingProjectedConfig {
    fn stage(
        &self,
        context: &HomeCancellation,
        input: &HomeOverlayInput,
    ) -> Result<HomeOverlaySnapshot, HomeLifecycleError> {
        if context.is_cancelled() {
            return Err(HomeLifecycleError::new(
                HomeLifecycleErrorKind::Cancelled,
                "cancelled",
            ));
        }
        Ok(HomeOverlaySnapshot {
            revision: input.revision,
            payload: input.payload.clone(),
            plugin_config: input.plugin_config.clone(),
            observation_barrier_revision: input.observation_barrier_revision,
            runtime_config: Some(config_with_strategy(SchedulerStrategy::FillFirst)),
        })
    }

    fn commit(
        &self,
        _context: &HomeCancellation,
        snapshot: &HomeOverlaySnapshot,
    ) -> Result<HomeConfigCommit, HomeLifecycleError> {
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
        self.entered.store(true, Ordering::Release);
        let mut released = self.released.lock().unwrap();
        while !*released {
            released = self.changed.wait(released).unwrap();
        }
        if context.is_cancelled() {
            Err(HomeLifecycleError::new(
                HomeLifecycleErrorKind::Cancelled,
                "cancelled",
            ))
        } else {
            Ok(())
        }
    }
}
