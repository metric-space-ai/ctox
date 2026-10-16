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
            .snapshot(
                PROCESS_STARTED
                    .get()
                    .map(Instant::elapsed)
                    .unwrap_or_default(),
            ),
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
}
