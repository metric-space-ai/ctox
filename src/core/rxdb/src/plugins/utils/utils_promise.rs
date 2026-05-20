//! Promise utilities.
//!
//! T3 deviations: JS Promises map onto Rust Futures. The upstream constants
//! `PROMISE_RESOLVE_*` exist purely to avoid allocating new Promises in JS;
//! they have no Rust equivalent (await on a value is allocation-free). The
//! `requestIdleCallback`-based helpers have no Rust counterpart either; we
//! provide a `yield_now`-based stand-in.

use std::time::Duration;

// ref: rxdb/src/plugins/utils/utils-promise.ts:1-6
/// returns a future that resolves on the next runtime tick
pub async fn next_tick() {
    tokio::task::yield_now().await;
}

// ref: rxdb/src/plugins/utils/utils-promise.ts:8-10
pub async fn promise_wait(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

// ref: rxdb/src/plugins/utils/utils-promise.ts:12-19
// `toPromise(maybePromise)` — no-op in Rust: a `Future` is already a `Future`.

// ref: rxdb/src/plugins/utils/utils-promise.ts:24-32
// `isPromise(value)` — no-op in Rust: Futures are nominally typed.

// ref: rxdb/src/plugins/utils/utils-promise.ts:38-41
// `PROMISE_RESOLVE_*` constants — no Rust equivalent; just return the value.

// ref: rxdb/src/plugins/utils/utils-promise.ts:44-74
/// "Request idle" with a timeout. Rust has no `requestIdleCallback`;
/// we yield to the runtime so other tasks can progress.
pub async fn request_idle_promise_no_queue(timeout_ms: Option<u64>) {
    let _ = timeout_ms;
    tokio::task::yield_now().await;
}

// ref: rxdb/src/plugins/utils/utils-promise.ts:76-89
/// Queued variant — single-threaded ordering preserved via tokio scheduler.
pub async fn request_idle_promise(timeout_ms: Option<u64>) {
    request_idle_promise_no_queue(timeout_ms).await;
}

// ref: rxdb/src/plugins/utils/utils-promise.ts:97-112
/// Best-effort: invoke `fun` after yielding. JS has `requestIdleCallback`;
/// Rust uses a yield + spawn.
pub fn request_idle_callback_if_available(fun: impl FnOnce() + Send + 'static) {
    tokio::spawn(async move {
        tokio::task::yield_now().await;
        fun();
    });
}

// ref: rxdb/src/plugins/utils/utils-promise.ts:115-129
/// like Promise.all() but runs in series instead of parallel
pub async fn promise_series<T, F, Fut>(tasks: Vec<F>, mut acc: T) -> Vec<T>
where
    F: FnOnce(T) -> Fut,
    Fut: std::future::Future<Output = T>,
    T: Clone,
{
    let mut out = Vec::with_capacity(tasks.len());
    for task in tasks {
        acc = task(acc).await;
        out.push(acc.clone());
    }
    out
}
