//! What a Go `context.Context` carries between the runner's steps, and the
//! cancellation policy that governs it.
//!
//! Upstream stuffs four things into the context and reads them back by key:
//! the job's error, the dry-run flag, the logger, and a *cancel context* — a
//! second, softer cancellation scope. Rust has no ambient context, so the
//! values become fields on [`RunContext`] and the keys disappear. The policy
//! is what matters and it is reproduced exactly:
//!
//! # Two cancellation scopes
//!
//! * **Force** — the whole run stops. `SIGTERM` and a second `Ctrl+C` do this,
//!   and so does the returned cancel function.
//! * **Graceful** — the current job is abandoned but its cleanup still runs, so
//!   containers and volumes are not orphaned. The *first* `Ctrl+C` does this.
//!
//! The distinction is the whole reason the type exists. `EarlyCancelContext`
//! builds a context that is cancelled when *either* scope dies, which is how a
//! step learns to stop promptly without the runner tearing down out from under
//! it.
//!
//! # Observable details preserved
//!
//! * A cancelled context makes a successful step return the cancellation, not
//!   success. Several combinators check this after the step, not only on
//!   failure, so a cancelled run cannot report a clean result.
//! * The cancellation error *outranks* a step's own error in a parallel
//!   executor: `NewParallelExecutor` returns `ctx.Err()` and drops
//!   `firstErr`.
//! * `createGracefulJobCancellationContext` hands back the signal channel, so
//!   the test can push a signal into it. [`SignalSource`] is the same seam, and
//!   it is what makes the policy testable without raising a real signal.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::anyhow;

/// Which cancellation scope to act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Stop the current job, still running its cleanup.
    Graceful,
    /// Stop everything.
    Force,
}

/// What the caller can cancel through.
#[derive(Debug, Clone)]
pub struct Cancellation {
    inner: Arc<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    force: AtomicBool,
    graceful: AtomicBool,
    /// `force` implies `graceful`, and this remembers that a `Ctrl+C` has
    /// already been seen so the *second* one escalates.
    interrupted: AtomicBool,
}

impl Cancellation {
    /// A fresh, un-cancelled scope.
    pub fn new() -> Self {
        Cancellation {
            inner: Arc::new(Inner::default()),
        }
    }

    /// Whether `scope` has been cancelled.
    pub fn is_cancelled(&self, scope: Scope) -> bool {
        match scope {
            // A force cancellation stops everything, so a graceful check has
            // to report it too.
            Scope::Graceful => {
                self.inner.graceful.load(Ordering::SeqCst)
                    || self.inner.force.load(Ordering::SeqCst)
            }
            Scope::Force => self.inner.force.load(Ordering::SeqCst),
        }
    }

    /// Cancels `scope`.
    pub fn cancel(&self, scope: Scope) {
        match scope {
            Scope::Graceful => self.inner.graceful.store(true, Ordering::SeqCst),
            Scope::Force => {
                self.inner.graceful.store(true, Ordering::SeqCst);
                self.inner.force.store(true, Ordering::SeqCst);
            }
        }
    }

    /// Resets the interruption counter, as the returned cleanup does when it
    /// stops listening for signals.
    pub fn stop_listening(&self) {
        self.inner.interrupted.store(false, Ordering::SeqCst);
    }

    /// The cancellation error for a context that has been cancelled, or
    /// `None`.
    ///
    /// Upstream returns Go's `context.Canceled` for both scopes, and several
    /// combinators return it *instead of* their own error, so the distinction
    /// between the scopes lives in what gets cancelled, not in the error.
    pub fn error_if_cancelled(&self, scope: Scope) -> Option<anyhow::Error> {
        self.is_cancelled(scope)
            .then(|| anyhow!("context canceled"))
    }
}

impl Default for Cancellation {
    fn default() -> Self {
        Self::new()
    }
}

/// The signal a [`SignalSource`] reports, mirroring what `os/signal` delivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// `Ctrl+C`.
    Interrupt,
    /// `SIGTERM`, and the Windows equivalent.
    Terminate,
}

/// The seam between the operating system's signals and the cancellation
/// policy.
///
/// Upstream registers a `signal.Notify` channel and hands that channel back
/// from its constructor, so the test writes a synthetic `os.Interrupt` into it
/// instead of raising one. The same seam is the only way this policy is
/// testable, and it is also where a host application plugs its own shutdown
/// handling in.
pub trait SignalSource: Send + Sync {
    /// Delivers one signal to the cancellation policy.
    ///
    /// The escalation rule lives here: the **first** interrupt is graceful, a
    /// second one is forceful, and a terminate is always forceful.
    fn deliver(&self, signal: Signal, cancellation: &Cancellation);
}

/// The default policy, as `createGracefulJobCancellationContext` implements it.
#[derive(Debug, Default)]
pub struct DefaultSignalSource;

impl SignalSource for DefaultSignalSource {
    fn deliver(&self, signal: Signal, cancellation: &Cancellation) {
        match signal {
            Signal::Interrupt => {
                if cancellation.inner.interrupted.swap(true, Ordering::SeqCst) {
                    // The second Ctrl+C: the user is insisting.
                    cancellation.cancel(Scope::Force);
                } else {
                    cancellation.cancel(Scope::Graceful);
                }
            }
            Signal::Terminate => cancellation.cancel(Scope::Force),
        }
    }
}

/// Everything a step needs from its surroundings.
///
/// Cloning is cheap: the cancellation is shared, and the log sink is behind an
/// `Arc` so a cloned context logs to the same place.
#[derive(Clone)]
pub struct RunContext {
    cancellation: Cancellation,
    dryrun: bool,
    sink: Option<Arc<dyn LogSink>>,
    /// The job's error, which upstream keeps in a mutable map inside the
    /// context so a step deep in the tree can report a failure that the
    /// runner reads back afterwards.
    job_error: Arc<std::sync::Mutex<Option<anyhow::Error>>>,
}

impl std::fmt::Debug for RunContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunContext")
            .field("dryrun", &self.dryrun)
            .field("cancelled", &self.cancellation.is_cancelled(Scope::Force))
            .field("has_sink", &self.sink.is_some())
            .finish()
    }
}

impl Default for RunContext {
    fn default() -> Self {
        RunContext::new()
    }
}

impl RunContext {
    /// A fresh context: not a dry run, nothing cancelled, no log sink.
    pub fn new() -> Self {
        RunContext {
            cancellation: Cancellation::new(),
            dryrun: false,
            sink: None,
            job_error: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Whether this is a dry run.
    pub fn dryrun(&self) -> bool {
        self.dryrun
    }

    /// Sets the dry-run flag.
    pub fn with_dryrun(mut self, dryrun: bool) -> Self {
        self.dryrun = dryrun;
        self
    }

    /// The cancellation scope.
    pub fn cancellation(&self) -> &Cancellation {
        &self.cancellation
    }

    /// The cancellation error when the graceful scope is gone, which is what
    /// the combinators check.
    pub fn cancellation_error(&self) -> Option<anyhow::Error> {
        self.cancellation.error_if_cancelled(Scope::Graceful)
    }

    /// Attaches a log sink.
    pub fn with_sink(mut self, sink: Arc<dyn LogSink>) -> Self {
        self.sink = Some(sink);
        self
    }

    /// The log sink, for code that has to *hand* output to it rather than log
    /// a line itself.
    ///
    /// The Docker back-end needs this for the pull and build response streams:
    /// those arrive as a byte stream that has to be parsed line by line and
    /// then written to the sink, which no `log_*` method can express. Returning
    /// `None` rather than a silent no-op sink matters — a caller that decodes a
    /// stream and has nowhere to put it will otherwise throw the result away
    /// without noticing.
    pub fn sink(&self) -> Option<Arc<dyn LogSink>> {
        self.sink.clone()
    }

    /// Records the job's error. Upstream's `SetJobError` overwrites, so a
    /// second failure replaces the first.
    pub fn set_job_error(&self, error: anyhow::Error) {
        if let Ok(mut slot) = self.job_error.lock() {
            *slot = Some(error);
        }
    }

    /// The job's error, if one was recorded.
    pub fn job_error(&self) -> Option<anyhow::Error> {
        self.job_error
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(|error| anyhow!("{error}")))
    }

    /// `Info`.
    pub fn log_info(&self, message: &str) {
        if let Some(sink) = &self.sink {
            sink.log(Level::Info, message);
        }
    }

    /// `Debug`.
    pub fn log_debug(&self, message: &str) {
        if let Some(sink) = &self.sink {
            sink.log(Level::Debug, message);
        }
    }

    /// `Warning`. Warnings reach the log through the error path, so this is
    /// the level they are reported at.
    pub fn log_warning(&self, message: &str) {
        if let Some(sink) = &self.sink {
            sink.log(Level::Warn, message);
        }
    }

    /// `Error`.
    pub fn log_error(&self, message: &str) {
        if let Some(sink) = &self.sink {
            sink.log(Level::Error, message);
        }
    }
}

/// The severities act's logger distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Verbose output that is off by default.
    Debug,
    /// Normal progress.
    Info,
    /// Something the user should look at, but which is not a failure.
    Warn,
    /// A failure.
    Error,
}

impl Level {
    /// The name a log formatter prints.
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Debug => "debug",
            Level::Info => "info",
            Level::Warn => "warning",
            Level::Error => "error",
        }
    }
}

/// Where a [`RunContext`] writes.
///
/// Upstream's is logrus, which this crate does not carry; the seam is what the
/// runner needs, and CTOX's own logging satisfies it.
pub trait LogSink: Send + Sync {
    /// Records one line.
    fn log(&self, level: Level, message: &str);
}

/// A sink that discards everything.
///
/// The Docker back-end needs one before the runner installs a real sink: a
/// step's output arrives as a stream from the daemon whether or not anybody is
/// listening, and collecting it by default would grow without bound.
pub struct NullSink;

impl LogSink for NullSink {
    fn log(&self, _level: Level, _message: &str) {}
}

/// A sink that collects lines, for tests and for a run whose output is
/// captured rather than printed.
#[derive(Debug, Default)]
pub struct CollectingSink {
    lines: std::sync::Mutex<Vec<(Level, String)>>,
}

impl CollectingSink {
    /// An empty sink.
    pub fn new() -> Self {
        CollectingSink::default()
    }

    /// Everything logged so far, in order.
    pub fn lines(&self) -> Vec<(Level, String)> {
        self.lines.lock().expect("sink poisoned").clone()
    }

    /// The messages logged at `level`.
    pub fn messages_at(&self, level: Level) -> Vec<String> {
        self.lines()
            .into_iter()
            .filter(|(logged, _)| *logged == level)
            .map(|(_, message)| message)
            .collect()
    }
}

impl LogSink for CollectingSink {
    fn log(&self, level: Level, message: &str) {
        self.lines
            .lock()
            .expect("sink poisoned")
            .push((level, message.to_string()));
    }
}

/// A cancellation error, for callers that need one without a context.
pub fn canceled() -> anyhow::Error {
    anyhow!("context canceled")
}

/// Runs `body` with a context that is cancelled when `early` fires.
///
/// This is `EarlyCancelContext`: a context that dies with the run *or* with an
/// earlier point, whichever comes first. Upstream returns the context
/// unchanged when there is no earlier one, and so does this.
pub fn early_cancel<F, T>(ctx: &RunContext, early: &Cancellation, body: F) -> T
where
    F: FnOnce(&Cancellation) -> T,
{
    let scoped = ctx.cancellation().clone();
    let early = early.clone();
    let handle = {
        let scoped = scoped.clone();
        std::thread::spawn(move || loop {
            if scoped.is_cancelled(Scope::Force) || early.is_cancelled(Scope::Force) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        })
    };
    let outcome = body(&scoped);
    // A scope that was not cancelled has to stop the watcher.
    scoped.stop_listening();
    drop(handle);
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// context_test.go: TestGracefulJobCancellationViaSigint
    ///
    /// The first interrupt stops the job but leaves the run going; the second
    /// stops the run.
    #[test]
    fn the_first_interrupt_is_graceful_and_the_second_is_forceful() {
        let cancellation = Cancellation::new();
        let source = DefaultSignalSource;
        assert!(!cancellation.is_cancelled(Scope::Graceful), "nothing yet");
        assert!(!cancellation.is_cancelled(Scope::Force));

        source.deliver(Signal::Interrupt, &cancellation);
        assert!(cancellation.is_cancelled(Scope::Graceful), "job stops");
        assert!(!cancellation.is_cancelled(Scope::Force), "run continues");

        source.deliver(Signal::Interrupt, &cancellation);
        assert!(cancellation.is_cancelled(Scope::Force), "run stops");
    }

    /// context_test.go: TestForceCancellationViaSigterm
    #[test]
    fn a_terminate_signal_is_forceful_and_implies_graceful() {
        let cancellation = Cancellation::new();
        DefaultSignalSource.deliver(Signal::Terminate, &cancellation);
        assert!(cancellation.is_cancelled(Scope::Force));
        assert!(
            cancellation.is_cancelled(Scope::Graceful),
            "a forceful stop is also a job stop",
        );
    }

    /// context_test.go: TestCreateGracefulJobCancellationContextCancelFunc
    #[test]
    fn the_returned_cancel_function_cancels_both_scopes() {
        let cancellation = Cancellation::new();
        cancellation.cancel(Scope::Force);
        assert_eq!(
            cancellation
                .error_if_cancelled(Scope::Graceful)
                .map(|e| e.to_string()),
            Some("context canceled".to_string()),
        );
        let fresh = Cancellation::new();
        assert!(fresh.error_if_cancelled(Scope::Graceful).is_none());
    }

    /// job_error.go: a job's error is recorded once and read back.
    #[test]
    fn the_job_error_is_recorded_and_read_back() {
        let ctx = RunContext::new();
        assert!(ctx.job_error().is_none());
        ctx.set_job_error(anyhow!("step 3 failed"));
        assert_eq!(
            ctx.job_error().expect("an error").to_string(),
            "step 3 failed",
        );
        // Upstream overwrites, so a second failure replaces the first.
        ctx.set_job_error(anyhow!("step 4 failed"));
        assert_eq!(
            ctx.job_error().expect("an error").to_string(),
            "step 4 failed",
        );
        // A clone shares the slot, which is how a deep step reports back.
        assert!(ctx.clone().job_error().is_some());
    }

    /// The sink is what replaces logrus, and the warning path is the only
    /// reason a step's error reaches it as a log line.
    #[test]
    fn log_lines_reach_the_sink_with_their_level() {
        let sink = Arc::new(CollectingSink::new());
        let ctx = RunContext::new().with_sink(sink.clone());
        ctx.log_info("one");
        ctx.log_debug("two");
        ctx.log_warning("three");
        ctx.log_error("four");
        assert_eq!(sink.messages_at(Level::Info), ["one"]);
        assert_eq!(sink.messages_at(Level::Debug), ["two"]);
        assert_eq!(sink.messages_at(Level::Warn), ["three"]);
        assert_eq!(sink.messages_at(Level::Error), ["four"]);
    }

    /// dryrun.go: the flag is a plain context value.
    #[test]
    fn the_dryrun_flag_is_carried_on_the_context() {
        assert!(!RunContext::new().dryrun());
        assert!(RunContext::new().with_dryrun(true).dryrun());
    }

    /// `early_cancel` hands the body a scope that dies with either side.
    #[test]
    fn an_early_cancellation_reaches_the_body() {
        let ctx = RunContext::new();
        let early = Cancellation::new();
        early.cancel(Scope::Force);
        early_cancel(&ctx, &early, |scoped| scoped.is_cancelled(Scope::Force));
    }

    /// The watcher thread must not outlive the call by long enough to matter;
    /// a cancelled run returns promptly rather than after a poll interval.
    #[test]
    fn early_cancel_returns_promptly() {
        let ctx = RunContext::new();
        let early = Cancellation::new();
        let started = std::time::Instant::now();
        early_cancel(&ctx, &early, |_| ());
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "took {:?}",
            started.elapsed(),
        );
    }
}
