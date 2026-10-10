//! The executor combinators: act's build-pipeline algebra.
//!
//! `pkg/runner` is written almost entirely as a composition of small
//! closures — run this, then that, but only if a condition holds, and either
//! way run a cleanup. Those combinators live here, and getting their error
//! handling exactly right is what decides whether a failing step stops a job.
//!
//! # `Warning` is the load-bearing idea
//!
//! A step can fail *and* let the job continue. `Warning` is an error that the
//! chain logs and steps over: `then` runs, `on_error` does **not**, and the
//! job's result is decided by whatever comes next. That is how act reports
//! "this step had a problem, look at the log" without failing the build.
//!
//! # What Go's `context` becomes
//!
//! Upstream threads a `context.Context` through every executor, and reads the
//! job error, the dry-run flag, the logger and a cancel context out of it by
//! key. Rust has no ambient context, and the equivalent — passing a struct of
//! shared state everywhere — is what [`RunContext`] is. The *policy* those keys
//! encode, especially cancellation, is in [`context`] and is tested there.
//!
//! # Porting notes
//!
//! * `Warning` is detected with `downcast_ref` on a boxed error, the Rust
//!   equivalent of Go's `switch err.(type)`. A combinator that receives a
//!   plain `Err` treats it as fatal, exactly as the `default:` arm does.
//! * `Finally` formats the original error *into* its message when the cleanup
//!   also fails, so the original error is no longer inspectable with
//!   `errors.Is`. Preserved: `errors.Is` on the result would stop matching,
//!   and something upstream may rely on that.
//! * `NewParallelExecutor` drains **all** results before returning, even after
//!   a failure. That is deliberate upstream — the comment says the executor
//!   waits so the parallel steps can clean up their containers — and a port
//!   that returned early would leak them.

use std::fmt;
use std::sync::mpsc;
use std::thread;

use anyhow::{anyhow, Result};

use crate::common::context::RunContext;

/// An error that is reported but does not stop the pipeline.
///
/// Upstream's `Warning` is a value that satisfies the `error` interface, so
/// the same "an error happened" plumbing carries it; the type switch in
/// `Then`, `ThenError` and `OnError` is the only thing that treats it
/// differently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning(pub String);

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for Warning {}

/// A warning with a message, as `Warningf`.
pub fn warning(message: impl Into<String>) -> Warning {
    Warning(message.into())
}

/// Whether an error is a [`Warning`] rather than a real failure.
pub fn is_warning(error: &anyhow::Error) -> bool {
    error.downcast_ref::<Warning>().is_some()
}

/// One step of a pipeline.
///
/// An `Arc`, not a `Box`, because a pipeline is a value that can be *run more
/// than once*: `parallel_executor` and the matrix fan-out build the step list
/// once and execute it per job, and a parallel step list has to hand its steps
/// to a channel on every run.
pub type Executor = std::sync::Arc<dyn Fn(&RunContext) -> Result<()> + Send + Sync>;



/// A predicate deciding whether a step runs.
///
/// A `Box`: a condition is evaluated inline, never handed to another thread,
/// so it does not need the shared ownership an [`Executor`] does.
pub type Conditional = Box<dyn Fn(&RunContext) -> bool + Send + Sync>;

/// A step that logs a message and succeeds.
pub fn info_executor(message: impl Into<String>) -> Executor {
    let message = message.into();
    std::sync::Arc::new(move |ctx| {
        ctx.log_info(&message);
        Ok(())
    })
}

/// A step that logs a debug message and succeeds.
pub fn debug_executor(message: impl Into<String>) -> Executor {
    let message = message.into();
    std::sync::Arc::new(move |ctx| {
        ctx.log_debug(&message);
        Ok(())
    })
}

/// A step that always fails.
pub fn error_executor(error: impl Into<String>) -> Executor {
    let error = error.into();
    std::sync::Arc::new(move |_| Err(anyhow!("{error}")))
}

/// Runs `true_executor` or `false_executor` depending on `conditional`. A
/// missing executor for the chosen branch is a success.
pub fn conditional_executor(
    conditional: Conditional,
    true_executor: Option<Executor>,
    false_executor: Option<Executor>,
) -> Executor {
    std::sync::Arc::new(move |ctx| {
        let chosen = if conditional(ctx) {
            &true_executor
        } else {
            &false_executor
        };
        match chosen {
            Some(executor) => executor(ctx),
            None => Ok(()),
        }
    })
}

/// Runs the steps in order, stopping at the first real failure.
///
/// An empty pipeline is a step that does nothing, so `pipeline()` and
/// `pipeline(step)` behave the same way.
pub fn pipeline(steps: Vec<Executor>) -> Executor {
    let mut result: Option<Executor> = None;
    for step in steps {
        result = Some(match result {
            None => step,
            Some(previous) => then(previous, step),
        });
    }
    result.unwrap_or_else(|| std::sync::Arc::new(|_: &RunContext| Ok(())))
}

/// Runs the steps with at most `parallel` at a time.
///
/// A `parallel` below 1 is raised to 1. Every step is started and every result
/// is collected, so a failure does not abandon the running ones — the steps own
/// containers that have to be torn down. The first error is returned, unless
/// the context was cancelled, in which case the cancellation wins.
pub fn parallel_executor(parallel: usize, steps: Vec<Executor>) -> Executor {
    std::sync::Arc::new(move |ctx| {
        if steps.is_empty() {
            return Ok(());
        }
        let parallel = parallel.max(1);

        let (work_tx, work_rx) = mpsc::channel::<Executor>();
        let (result_tx, result_rx) = mpsc::channel::<Result<()>>();
        // `mpsc::Receiver` is not `Sync` and the workers share it, so the
        // queue lives behind a mutex.
        let work_rx = std::sync::Arc::new(std::sync::Mutex::new(work_rx));

        let workers: Vec<_> = (0..parallel)
            .map(|_| {
                let work_rx = std::sync::Arc::clone(&work_rx);
                let result_tx = result_tx.clone();
                let ctx = ctx.clone();
                thread::spawn(move || {
                    loop {
                        let queued = {
                            let queue = work_rx.lock().expect("work queue poisoned");
                            queue.recv().ok()
                        };
                        let Some(queued) = queued else { break };
                        // A panic in a step must not swallow the results the
                        // other steps are still going to send, so it is
                        // reported as a failure of its own.
                        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            queued(&ctx)
                        }))
                        .unwrap_or_else(|_| Err(anyhow!("step panicked")));
                        if result_tx.send(outcome).is_err() {
                            break;
                        }
                    }
                })
            })
            .collect();
        drop(result_tx);

        for executor in &steps {
            if work_tx.send(std::sync::Arc::clone(executor)).is_err() {
                break;
            }
        }
        drop(work_tx);

        let mut first_error: Option<anyhow::Error> = None;
        for _ in 0..steps.len() {
            // Every result is drained even once one has failed: the steps are
            // holding resources.
            if let Ok(result) = result_rx.recv() {
                if first_error.is_none() {
                    first_error = result.err();
                }
            }
        }
        for worker in workers {
            let _ = worker.join();
        }

        if let Some(error) = ctx.cancellation_error() {
            return Err(error);
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    })
}

/// Runs `then` if this step did not fail for real.
///
/// A [`Warning`] is logged and treated as success, and a cancelled context
/// stops the chain even when the step succeeded.
pub fn then(step: Executor, next: Executor) -> Executor {
    std::sync::Arc::new(move |ctx| {
        match step(ctx) {
            Ok(()) => {}
            Err(error) => {
                // The one case that does not stop the chain.
                if is_warning(&error) {
                    ctx.log_warning(&error.to_string());
                } else {
                    return Err(error);
                }
            }
        }
        if let Some(error) = ctx.cancellation_error() {
            return Err(error);
        }
        next(ctx)
    })
}

/// Runs `then` with this step's outcome, whether it failed or not.
///
/// Unlike [`then`], a real failure is *passed on* rather than stopping the
/// chain, which is how a step's failure gets reported without aborting the
/// job.
pub fn then_error(step: Executor, next: Executor2) -> Executor {
    std::sync::Arc::new(move |ctx| {
        let outcome = step(ctx);
        report_warning(ctx, &outcome);
        if let Some(error) = ctx.cancellation_error() {
            return Err(error);
        }
        next(ctx, outcome)
    })
}

/// A step that sees the previous step's outcome.
pub type Executor2 = Box<dyn Fn(&RunContext, Result<()>) -> Result<()> + Send + Sync>;

/// Runs `then` only when this step failed for real.
///
/// The step's error and whatever `then` returns are joined, so neither is
/// lost. A [`Warning`] means the step did *not* fail as far as this is
/// concerned: it is logged and `then` is skipped.
pub fn on_error(step: Executor, then: Executor) -> Executor {
    std::sync::Arc::new(move |ctx| {
        match step(ctx) {
            Ok(()) => {}
            Err(error) => {
                if is_warning(&error) {
                    // A warning is not a failure, so the handler does not run.
                    ctx.log_warning(&error.to_string());
                } else {
                    // Neither error is lost: the handler's becomes the cause
                    // and the step's the context. Go's `errors.Join` puts them
                    // side by side; anyhow nests them, which is the closest
                    // equivalent.
                    return match then(ctx) {
                        Ok(()) => Err(error),
                        Err(secondary) => Err(secondary.context(format!("{error}"))),
                    };
                }
            }
        }
        if let Some(cancelled) = ctx.cancellation_error() {
            return Err(cancelled);
        }
        Ok(())
    })
}

/// Runs `step` only if `conditional` holds.
pub fn if_then(step: Executor, conditional: Conditional) -> Executor {
    std::sync::Arc::new(move |ctx| {
        if conditional(ctx) {
            return step(ctx);
        }
        Ok(())
    })
}

/// Runs `step` only if `conditional` does **not** hold.
pub fn if_not(step: Executor, conditional: Conditional) -> Executor {
    std::sync::Arc::new(move |ctx| {
        if !conditional(ctx) {
            return step(ctx);
        }
        Ok(())
    })
}

/// Runs `step` when `conditional` is true, decided before the pipeline runs.
pub fn if_bool(step: Executor, conditional: bool) -> Executor {
    if_then(step, Box::new(move |_: &RunContext| conditional))
}

/// Runs `finally` after `step`, whatever happened.
///
/// If the cleanup also fails, both errors are reported in one message and the
/// original is no longer separately inspectable — which is upstream's
/// behaviour, and the reason a caller cannot `errors.Is` its way to the step's
/// own failure in that case.
pub fn finally(step: Executor, cleanup: Executor) -> Executor {
    std::sync::Arc::new(move |ctx| {
        let outcome = step(ctx);
        let cleanup_outcome = cleanup(ctx);
        if let Err(cleanup_error) = cleanup_outcome {
            let original = match &outcome {
                Ok(()) => "<nil>".to_string(),
                Err(error) => error.to_string(),
            };
            return Err(anyhow!(
                "Error occurred running finally: {cleanup_error} (original error: {original})"
            ));
        }
        outcome
    })
}

/// Logs a [`Warning`] on the context, leaving the error otherwise untouched.
fn report_warning(ctx: &RunContext, outcome: &Result<()>) {
    if let Err(error) = outcome {
        if is_warning(error) {
            ctx.log_warning(&error.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::context::{Cancellation, CollectingSink, RunContext, Scope};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    /// A step that counts how many times it ran.
    fn counting(counter: Arc<AtomicUsize>) -> Executor {
        std::sync::Arc::new(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }

    // executor_test.go: TestNewWorkflow
    #[test]
    fn a_pipeline_runs_every_step_in_order() {
        let ctx = RunContext::new();

        // Empty.
        assert!(pipeline(vec![])(&ctx).is_ok());

        // One failure.
        assert!(error_executor("test error")(&ctx).is_err());

        // Several successes, all of them run.
        let run = Arc::new(AtomicUsize::new(0));
        assert!(pipeline(vec![counting(Arc::clone(&run)), counting(Arc::clone(&run))])(&ctx).is_ok());
        assert_eq!(run.load(Ordering::SeqCst), 2);
    }

    /// A real failure stops the chain, and the steps after it do not run.
    #[test]
    fn a_real_failure_stops_the_pipeline() {
        let ctx = RunContext::new();
        let run = Arc::new(AtomicUsize::new(0));
        let steps = vec![
            counting(Arc::clone(&run)),
            error_executor("boom"),
            counting(Arc::clone(&run)),
        ];
        assert!(pipeline(steps)(&ctx).is_err());
        assert_eq!(run.load(Ordering::SeqCst), 1, "the third step never ran");
    }

    /// This is the point of `Warning`: a step reports a problem and the
    /// pipeline carries on as if it had succeeded.
    #[test]
    fn a_warning_is_logged_and_the_pipeline_continues() {
        let sink = Arc::new(CollectingSink::new());
        let ctx = RunContext::new().with_sink(sink.clone());
        let run = Arc::new(AtomicUsize::new(0));

        let steps = vec![
            std::sync::Arc::new(|_: &RunContext| Err(anyhow!(warning("look at this")))) as Executor,
            counting(Arc::clone(&run)),
        ];
        assert!(
            pipeline(steps)(&ctx).is_ok(),
            "a warning is not a failure",
        );
        assert_eq!(run.load(Ordering::SeqCst), 1, "the chain continued");
        assert_eq!(sink.messages_at(crate::common::context::Level::Warn), ["look at this"]);
    }

    /// `on_error` runs only for a real failure, and never for a warning.
    #[test]
    fn on_error_runs_only_for_a_real_failure() {
        let ctx = RunContext::new();

        let recovered = Arc::new(AtomicUsize::new(0));
        let steps = vec![
            error_executor("boom"),
            counting(Arc::clone(&recovered)),
        ];
        let outcome = on_error(pipeline(steps), counting(Arc::clone(&recovered)))(&ctx);
        assert!(outcome.is_err(), "the failure is still reported");
        assert_eq!(recovered.load(Ordering::SeqCst), 1, "only the handler ran");

        let handled = Arc::new(AtomicUsize::new(0));
        let warned: Executor =
            std::sync::Arc::new(|_: &RunContext| Err(anyhow!(warning("hmm"))));
        let outcome = on_error(warned, counting(Arc::clone(&handled)))(&ctx);
        assert!(outcome.is_ok(), "a warning is not a failure");
        assert_eq!(handled.load(Ordering::SeqCst), 0, "the handler did not run");
    }

    /// `finally` runs whatever happened, and a failing cleanup folds the
    /// original into its message.
    #[test]
    fn finally_always_runs() {
        let ctx = RunContext::new();
        let cleanup = Arc::new(AtomicUsize::new(0));

        let outcome = finally(
            counting(Arc::clone(&cleanup)),
            counting(Arc::clone(&cleanup)),
        )(&ctx);
        assert!(outcome.is_ok());
        assert_eq!(cleanup.load(Ordering::SeqCst), 2);

        let outcome = finally(
            error_executor("the step failed"),
            counting(Arc::clone(&cleanup)),
        )(&ctx);
        assert_eq!(outcome.unwrap_err().to_string(), "the step failed");
    }

    /// When the cleanup also fails, the original error is no longer
    /// separately inspectable — upstream formats it into the message.
    #[test]
    fn a_failing_cleanup_reports_both_errors() {
        let ctx = RunContext::new();
        let outcome = finally(
            error_executor("original"),
            error_executor("cleanup"),
        )(&ctx);
        let message = outcome.unwrap_err().to_string();
        assert_eq!(
            message,
            "Error occurred running finally: cleanup (original error: original)",
        );
    }

    // executor_test.go: TestNewConditionalExecutor
    #[test]
    fn a_conditional_picks_one_branch() {
        let ctx = RunContext::new();
        let yes = Arc::new(AtomicUsize::new(0));
        let no = Arc::new(AtomicUsize::new(0));

        conditional_executor(
            Box::new(|_: &RunContext| false),
            Some(counting(Arc::clone(&yes))),
            Some(counting(Arc::clone(&no))),
        )(&ctx)
        .expect("no failure");
        assert_eq!(yes.load(Ordering::SeqCst), 0);
        assert_eq!(no.load(Ordering::SeqCst), 1);

        conditional_executor(
            Box::new(|_: &RunContext| true),
            Some(counting(Arc::clone(&yes))),
            Some(counting(Arc::clone(&no))),
        )(&ctx)
        .expect("no failure");
        assert_eq!(yes.load(Ordering::SeqCst), 1);
        assert_eq!(no.load(Ordering::SeqCst), 1);
    }

    /// A missing executor for the chosen branch is a success, not a crash.
    #[test]
    fn a_conditional_without_a_branch_succeeds() {
        let ctx = RunContext::new();
        assert!(conditional_executor(Box::new(|_: &RunContext| true), None, None)(&ctx).is_ok());
    }

    #[test]
    fn the_conditional_helpers_agree() {
        let ctx = RunContext::new();
        let run = Arc::new(AtomicUsize::new(0));

        if_bool(counting(Arc::clone(&run)), true)(&ctx).expect("no failure");
        if_bool(counting(Arc::clone(&run)), false)(&ctx).expect("no failure");
        assert_eq!(run.load(Ordering::SeqCst), 1, "only the true branch");

        if_then(counting(Arc::clone(&run)), Box::new(|_: &RunContext| true))(&ctx)
            .expect("no failure");
        if_not(counting(Arc::clone(&run)), Box::new(|_: &RunContext| true))(&ctx)
            .expect("no failure");
        assert_eq!(run.load(Ordering::SeqCst), 2);
    }

    // executor_test.go: TestNewParallelExecutor
    #[test]
    fn a_parallel_executor_respects_its_limit() {
        let ctx = RunContext::new();
        let count = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));

        // Upstream sleeps two seconds per step to make the overlap
        // observable. Sampling the peak while the steps are in flight asserts
        // the same thing without the six seconds.
        //
        // An `Executor` is not `Clone`, so the step is built afresh for each
        // slot; the counters are shared, which is what is being measured.
        let make_step = || -> Executor {
            let count = Arc::clone(&count);
            let active = Arc::clone(&active);
            let peak = Arc::clone(&peak);
            std::sync::Arc::new(move |_: &RunContext| {
                count.fetch_add(1, Ordering::SeqCst);
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(40));
                active.fetch_sub(1, Ordering::SeqCst);
                Ok(())
            })
        };

        parallel_executor(2, vec![make_step(), make_step(), make_step()])(&ctx)
            .expect("no failure");

        assert_eq!(count.load(Ordering::SeqCst), 3, "all three ran");
        assert_eq!(
            peak.load(Ordering::SeqCst),
            2,
            "exactly two at a time, with three steps of the same length",
        );
    }

    /// A `parallel` below one is raised to one, so a "parallel" of 0 still
    /// runs everything — serially.
    #[test]
    fn a_parallel_executor_never_uses_zero_workers() {
        let ctx = RunContext::new();
        let count = Arc::new(AtomicUsize::new(0));
        parallel_executor(0, vec![
            counting(Arc::clone(&count)),
            counting(Arc::clone(&count)),
            counting(Arc::clone(&count)),
        ])(&ctx)
        .expect("no failure");
        assert_eq!(count.load(Ordering::SeqCst), 3, "all three still ran");
    }

    // executor_test.go: TestNewParallelExecutorCanceled
    #[test]
    fn a_cancelled_context_outranks_a_step_error() {
        let ctx = RunContext::new();
        ctx.cancellation().cancel(Scope::Force);

        let count = Arc::new(AtomicUsize::new(0));
        let outcome = parallel_executor(3, vec![
            error_executor("fake error"),
            counting(Arc::clone(&count)),
            counting(Arc::clone(&count)),
        ])(&ctx);

        // Every step still runs — the port drains them so their resources are
        // released — but the returned error is the cancellation.
        assert_eq!(count.load(Ordering::SeqCst), 2);
        assert_eq!(
            outcome.unwrap_err().to_string(),
            "context canceled",
            "the cancellation wins over the step's own error",
        );
    }

    // executor_test.go: TestNewParallelExecutorFailed
    #[test]
    fn a_parallel_executor_reports_its_first_error() {
        let ctx = RunContext::new();
        let outcome = parallel_executor(1, vec![error_executor("fake error")])(&ctx);
        assert_eq!(outcome.unwrap_err().to_string(), "fake error");
    }

    /// Even a failing step gets drained, so a parallel fan-out cannot leave a
    /// container running.
    #[test]
    fn a_parallel_executor_drains_every_step_after_a_failure() {
        let ctx = RunContext::new();
        let count = Arc::new(AtomicUsize::new(0));
        let outcome = parallel_executor(1, vec![
            error_executor("first fails"),
            counting(Arc::clone(&count)),
            counting(Arc::clone(&count)),
        ])(&ctx);
        assert!(outcome.is_err());
        assert_eq!(count.load(Ordering::SeqCst), 2, "the rest still ran");
    }

    /// A step that panics is reported as a failure rather than taking the
    /// whole process down, which is what keeps one bad step from losing the
    /// job's container.
    #[test]
    fn a_panicking_step_is_reported_not_propagated() {
        let ctx = RunContext::new();
        let outcome = parallel_executor(1, vec![
            std::sync::Arc::new(|_: &RunContext| -> Result<()> { panic!("boom") }) as Executor,
        ])(&ctx);
        assert!(outcome.is_err());
    }

    /// A cancelled context turns a *successful* step into a cancellation, so
    /// a cancelled run cannot report success.
    #[test]
    fn a_cancelled_context_fails_a_successful_step() {
        let ctx = RunContext::new();
        ctx.cancellation().cancel(Scope::Graceful);
        let count = Arc::new(AtomicUsize::new(0));
        let outcome = then(counting(Arc::clone(&count)), counting(Arc::clone(&count)))(&ctx);
        assert_eq!(outcome.unwrap_err().to_string(), "context canceled");
        assert_eq!(count.load(Ordering::SeqCst), 1, "the second step was skipped");
    }

    /// `then_error` sees the outcome either way, which is how a step's failure
    /// gets reported without ending the job.
    #[test]
    fn then_error_sees_the_previous_outcome() {
        let ctx = RunContext::new();
        let seen = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&seen);
        let handler: Executor2 = Box::new(move |_: &RunContext, outcome: Result<()>| {
            if outcome.is_err() {
                counted.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        });

        then_error(error_executor("boom"), handler)(&ctx).expect("handled");
        assert_eq!(seen.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn the_logging_steps_succeed_and_write() {
        let sink = Arc::new(CollectingSink::new());
        let ctx = RunContext::new().with_sink(sink.clone());
        info_executor("hello")(&ctx).expect("no failure");
        debug_executor("there")(&ctx).expect("no failure");
        assert_eq!(sink.messages_at(crate::common::context::Level::Info), ["hello"]);
        assert_eq!(sink.messages_at(crate::common::context::Level::Debug), ["there"]);
    }

    #[test]
    fn a_warning_is_distinguishable_from_a_failure() {
        assert!(is_warning(&anyhow!(warning("careful"))));
        assert!(!is_warning(&anyhow!("careful")));
        assert_eq!(warning("careful").to_string(), "careful");
    }

    /// The cancellation type is what the executors read, and force implies
    /// graceful.
    #[test]
    fn the_cancellation_scopes_nest() {
        let cancellation = Cancellation::new();
        assert!(!cancellation.is_cancelled(Scope::Graceful));
        cancellation.cancel(Scope::Force);
        assert!(cancellation.is_cancelled(Scope::Graceful));
        assert!(cancellation.is_cancelled(Scope::Force));
    }
}
