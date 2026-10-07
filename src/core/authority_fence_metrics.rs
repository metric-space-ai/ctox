//! Fixed-cardinality measurements; never an authority decision or a permit.
//! Collection/key/actor/root labels deliberately cannot enter this API.

use serde::Serialize;
use std::cell::Cell;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const REPORT_INTERVAL: Duration = Duration::from_secs(15);
const BUCKET_UPPER_US: [u64; 8] = [
    100,
    500,
    2_000,
    10_000,
    50_000,
    250_000,
    1_000_000,
    u64::MAX,
];
const CATEGORY_NAMES: [&str; 3] = [
    "issuer_publication",
    "native_read_publication",
    "native_write_publication",
];
const STAGE_NAMES: [&str; 5] = ["issuer", "encrypted_store", "core", "policy", "projection"];

#[derive(Clone, Copy)]
pub(crate) enum Category {
    IssuerPublication = 0,
    NativeReadPublication = 1,
    NativeWritePublication = 2,
}

#[derive(Clone, Copy)]
pub(crate) enum Stage {
    Issuer = 0,
    EncryptedStore = 1,
    Core = 2,
    Policy = 3,
    Projection = 4,
}

/// Declare before every measured authority guard. Its report then runs only
/// after those guards release. Stage guards borrow this timer but hold no locks.
pub(crate) struct FenceTiming {
    category: Category,
    began: Instant,
    holds: [Cell<Option<Duration>>; 5],
    succeeded: Cell<bool>,
}

impl FenceTiming {
    pub(crate) fn new(category: Category) -> Self {
        PROCESS_STARTED.get_or_init(Instant::now);
        Self {
            category,
            began: Instant::now(),
            holds: std::array::from_fn(|_| Cell::new(None)),
            succeeded: Cell::new(false),
        }
    }

    /// Declare immediately before acquiring the actual guard/transaction;
    /// call acquired only after success. Drop records its actual release.
    pub(crate) fn stage(&self, stage: Stage) -> StageTiming<'_> {
        StageTiming {
            timing: self,
            stage,
            acquired: Cell::new(None),
        }
    }

    pub(crate) fn finish(&self, succeeded: bool) {
        self.succeeded.set(succeeded);
    }
}

pub(crate) struct StageTiming<'a> {
    timing: &'a FenceTiming,
    stage: Stage,
    acquired: Cell<Option<Instant>>,
}

impl StageTiming<'_> {
    pub(crate) fn acquired(&self) {
        self.acquired.set(Some(Instant::now()));
    }
}

impl Drop for StageTiming<'_> {
    fn drop(&mut self) {
        if let Some(at) = self.acquired.get() {
            self.timing.holds[self.stage as usize].set(Some(at.elapsed()));
        }
    }
}

#[derive(Clone, Default, Serialize)]
struct Histogram {
    count: u64,
    total_us: u64,
    max_us: u64,
    buckets: [u64; 8],
}

impl Histogram {
    fn record(&mut self, duration: Duration) {
        let us = u64::try_from(duration.as_micros()).unwrap_or(u64::MAX);
        self.count = self.count.saturating_add(1);
        self.total_us = self.total_us.saturating_add(us);
        self.max_us = self.max_us.max(us);
        let bucket = BUCKET_UPPER_US
            .iter()
            .position(|upper| us <= *upper)
            .unwrap_or(7);
        self.buckets[bucket] = self.buckets[bucket].saturating_add(1);
    }
}

#[derive(Clone, Default, Serialize)]
struct Measurements {
    completed_attempts: u64,
    successful_callbacks: u64,
    elapsed: Histogram,
    stages: std::collections::BTreeMap<&'static str, Histogram>,
}

impl Measurements {
    fn record(&mut self, succeeded: bool, elapsed: Duration, holds: [Option<Duration>; 5]) {
        self.completed_attempts = self.completed_attempts.saturating_add(1);
        if succeeded {
            self.successful_callbacks = self.successful_callbacks.saturating_add(1);
        }
        self.elapsed.record(elapsed);
        for (name, hold) in STAGE_NAMES.iter().zip(holds) {
            if let Some(hold) = hold {
                self.stages.entry(name).or_default().record(hold);
            }
        }
    }
}

#[derive(Default)]
struct Statistics {
    measurements: [Measurements; 3],
    last_report: Option<Duration>,
}

impl Statistics {
    fn record(
        &mut self,
        category: Category,
        succeeded: bool,
        elapsed: Duration,
        holds: [Option<Duration>; 5],
    ) {
        self.measurements[category as usize].record(succeeded, elapsed, holds);
    }

    fn report(&mut self, process_elapsed: Duration) -> Option<serde_json::Value> {
        if self
            .last_report
            .is_some_and(|last| process_elapsed.saturating_sub(last) < REPORT_INTERVAL)
        {
            return None;
        }
        self.last_report = Some(process_elapsed);
        Some(self.snapshot(process_elapsed))
    }

    fn snapshot(&self, process_elapsed: Duration) -> serde_json::Value {
        let categories = CATEGORY_NAMES
            .into_iter()
            .zip(self.measurements.iter())
            .map(|(name, measurements)| (name, measurements))
            .collect::<std::collections::BTreeMap<_, _>>();
        serde_json::json!({
            "schema": "ctox.authority_fence_metrics.v1",
            "pid": std::process::id(),
            "observed": self.measurements.iter().any(|sample| sample.completed_attempts > 0),
            "process_elapsed_us": u64::try_from(process_elapsed.as_micros()).unwrap_or(u64::MAX),
            "bucket_upper_us": BUCKET_UPPER_US,
            "categories": categories,
        })
    }
}

static PROCESS_STARTED: OnceLock<Instant> = OnceLock::new();
static STATISTICS: OnceLock<Mutex<Statistics>> = OnceLock::new();

/// Read the current process aggregate without acquiring authority, opening
/// SQLite, initializing measurements or consuming the journal report interval.
/// The daemon heartbeat carries this value to external status readers; a CLI
/// process must never substitute its own empty aggregate for the daemon's.
pub(crate) fn snapshot() -> serde_json::Value {
    match STATISTICS.get() {
        Some(statistics) => statistics
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .snapshot(PROCESS_STARTED.get().map(Instant::elapsed).unwrap_or_default()),
        None => Statistics::default().snapshot(Duration::ZERO),
    }
}

impl Drop for FenceTiming {
    fn drop(&mut self) {
        // Only completed attempts enter the aggregate, including failures before
        // acquisition. The tiny metrics mutex is never held during authority,
        // SQLite, user callbacks or journal I/O. Poison cannot alter admission.
        let report = {
            let mut statistics = STATISTICS
                .get_or_init(|| Mutex::new(Statistics::default()))
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            statistics.record(
                self.category,
                self.succeeded.get(),
                self.began.elapsed(),
                std::array::from_fn(|index| self.holds[index].get()),
            );
            statistics.report(PROCESS_STARTED.get_or_init(Instant::now).elapsed())
        };
        if let Some(report) = report {
            // A closed journal pipe must never change an authorization result.
            let _ = writeln!(
                std::io::stderr(),
                "[business-os] authority fence metrics: {report}"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_snapshot_distinguishes_unobserved_from_failed_acquisition() {
        let mut statistics = Statistics::default();
        let before = statistics.snapshot(Duration::ZERO);
        assert_eq!(before["observed"], false);
        assert_eq!(before["pid"], std::process::id());
        assert_eq!(before["categories"].as_object().unwrap().len(), 3);
        assert!(before["categories"]["issuer_publication"]["stages"].as_object().unwrap().is_empty());
        statistics.record(Category::IssuerPublication, false, Duration::from_micros(7), [None; 5]);
        let after = statistics.snapshot(Duration::from_secs(1));
        assert_eq!(after["observed"], true);
        assert_eq!(after["categories"]["issuer_publication"]["completed_attempts"], 1);
        assert_eq!(after["categories"]["issuer_publication"]["successful_callbacks"], 0);
        assert!(after["categories"]["issuer_publication"]["stages"].as_object().unwrap().is_empty());
        assert_eq!(after.as_object().unwrap().len(), 6);
    }

    #[test]
    fn status_snapshot_preserves_counters_and_does_not_consume_journal_throttle() {
        let mut statistics = Statistics::default();
        assert!(statistics.report(Duration::from_secs(1)).is_some());
        let mut holds = [None; 5];
        holds[2] = Some(Duration::from_micros(123));
        statistics.record(Category::NativeReadPublication, true, Duration::from_micros(150), holds);
        let current = statistics.snapshot(Duration::from_secs(2));
        assert_eq!(current["categories"]["native_read_publication"]["stages"]["core"]["total_us"], 123);
        assert_eq!(statistics.last_report, Some(Duration::from_secs(1)));
        assert!(statistics.report(Duration::from_secs(2)).is_none());
        let next = statistics.report(Duration::from_secs(16)).unwrap();
        assert_eq!(next["categories"], current["categories"]);
    }

    #[test]
    fn failed_acquisitions_count_attempts_without_inventing_zero_length_holds() {
        let mut statistics = Statistics::default();
        statistics.record(
            Category::IssuerPublication,
            false,
            Duration::from_micros(20),
            [None; 5],
        );
        let sample = &statistics.measurements[0];
        assert_eq!(sample.completed_attempts, 1);
        assert_eq!(sample.successful_callbacks, 0);
        assert_eq!(sample.elapsed.count, 1);
        assert!(sample.stages.is_empty());
    }

    #[test]
    fn fast_and_failed_callbacks_retain_every_actually_acquired_fence() {
        let mut statistics = Statistics::default();
        let mut holds = [None; 5];
        holds[0] = Some(Duration::ZERO);
        statistics.record(
            Category::IssuerPublication,
            true,
            Duration::from_micros(10),
            holds,
        );
        holds[0] = Some(Duration::from_micros(1_999));
        statistics.record(
            Category::IssuerPublication,
            false,
            Duration::from_micros(2_010),
            holds,
        );
        let sample = &statistics.measurements[0];
        assert_eq!(sample.completed_attempts, 2);
        assert_eq!(sample.successful_callbacks, 1);
        assert_eq!(sample.stages["issuer"].count, 2);
        assert_eq!(sample.stages["issuer"].total_us, 1_999);
        assert_eq!(sample.stages["issuer"].max_us, 1_999);
        assert_eq!(sample.stages["issuer"].buckets, [1, 0, 1, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn completed_native_read_and_write_attempts_are_distinct() {
        let mut statistics = Statistics::default();
        statistics.record(
            Category::NativeReadPublication,
            true,
            Duration::from_millis(3),
            [None; 5],
        );
        statistics.record(
            Category::NativeWritePublication,
            false,
            Duration::from_millis(4),
            [None; 5],
        );
        assert_eq!(statistics.measurements[0].completed_attempts, 0);
        assert_eq!(statistics.measurements[1].successful_callbacks, 1);
        assert_eq!(statistics.measurements[2].completed_attempts, 1);
        assert_eq!(statistics.measurements[2].successful_callbacks, 0);
    }

    #[test]
    fn report_throttle_retains_samples_and_reports_exact_counter_deltas() {
        let mut statistics = Statistics::default();
        let mut holds = [None; 5];
        holds[2] = Some(Duration::from_micros(500));
        statistics.record(
            Category::NativeReadPublication,
            true,
            Duration::from_micros(800),
            holds,
        );
        let first = statistics.report(Duration::from_secs(1)).unwrap();
        for _ in 0..100 {
            statistics.record(
                Category::NativeReadPublication,
                true,
                Duration::from_micros(800),
                holds,
            );
        }
        assert!(statistics.report(Duration::from_secs(15)).is_none());
        let last = statistics.report(Duration::from_secs(16)).unwrap();
        let path = "/categories/native_read_publication/stages/core/count";
        assert_eq!(
            last.pointer(path).unwrap().as_u64().unwrap()
                - first.pointer(path).unwrap().as_u64().unwrap(),
            100
        );
        assert_eq!(
            last["process_elapsed_us"].as_u64().unwrap()
                - first["process_elapsed_us"].as_u64().unwrap(),
            15_000_000
        );
        assert_eq!(last["categories"].as_object().unwrap().len(), 3);
        assert_eq!(last["schema"], "ctox.authority_fence_metrics.v1");
    }

    #[test]
    fn histograms_use_inclusive_bounds_and_saturate_without_panicking() {
        let mut histogram = Histogram::default();
        for us in [100, 101, 500, 501, 1_000_000, 1_000_001] {
            histogram.record(Duration::from_micros(us));
        }
        assert_eq!(histogram.buckets, [1, 2, 1, 0, 0, 0, 1, 1]);
        histogram.total_us = u64::MAX;
        histogram.record(Duration::from_micros(1));
        assert_eq!(histogram.total_us, u64::MAX);
    }

    #[test]
    fn stage_guard_distinguishes_unacquired_from_released_authority() {
        let timing = FenceTiming::new(Category::NativeReadPublication);
        {
            let _failed = timing.stage(Stage::Core);
        }
        assert_eq!(timing.holds[Stage::Core as usize].get(), None);
        {
            let acquired = timing.stage(Stage::Policy);
            acquired.acquired();
        }
        assert!(timing.holds[Stage::Policy as usize].get().is_some());
        timing.finish(true);
    }
}
