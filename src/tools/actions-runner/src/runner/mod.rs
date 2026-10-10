//! Orchestrating a job: the parts of act's `pkg/runner` that turn a workflow
//! into running steps.
//!
//! Port of act's `pkg/runner` (5,419 lines). That package is the seam between
//! the *model* (what a workflow says) and the *container* (where a step runs),
//! and it is the largest single package in act.
//!
//! It splits along three lines, and the modules here follow the same split:
//!
//! 1. **The workflow command grammar** — [`command`] — `::set-output::`,
//!    `::add-path::`, `::save-state::`, `::stop-commands::`, `::add-mask::`, and
//!    Azure DevOps' `##[group]`. A step's *only* way to talk back to the runner
//!    is by printing one of these on stdout; the line is matched here and turned
//!    into state. This is ported first because it is a pure grammar: no I/O, no
//!    model, and it is the one part of the runner a user can hit from their
//!    first `run:` step.
//!
//! 2. **The expression evaluator** — [`expression`] — `expression.go`, in
//!    three halves. The rewriter folds `${{ … }}` embedded in a larger string
//!    into one `format()` call. It is not a second implementation of the
//!    language: [`crate::expr::interpreter`] already provides `success()`,
//!    `always()`, `contains`, the `hashFiles` hook and `default_status_check`,
//!    and this module's output is handed straight to it. The context tree is
//!    here for the builders that do not need a live run — `strategy`, `needs`,
//!    `steps`, `env`, `secrets`. And both entry points are here:
//!    [`expression::interpolate`] for a string, [`expression::evaluate_yaml_node`]
//!    for a YAML subtree, [`expression::eval_bool`] for a condition.
//!    What is still missing is the `getEvaluatorInputs` assembly of the
//!    `github`, `runner` and `inputs` halves, which needs `RunContext.caller`
//!    and a live `ExprEval` for the reusable-workflow path.
//!
//! 3. **The step algebra** — `step.go`, `step_run.go`, `step_action_local.go`,
//!    `step_action_remote.go`, `step_docker.go`, `action_composite.go` and
//!    `run_context.go`. The `step` trait and `stepStage` (`Pre`/`Main`/`Post`)
//!    are the spine; `run_context.go` (1,177 lines, the largest file in the
//!    package) is the state every step reads and writes.
//!
//! # Upstream has no defined answer here
//!
//! `unescapeCommandData("%250A")` is **nondeterministic in Go**: 400 runs of
//! the upstream function gave `"\n"` 294 times and `"%0A"` 106 times, because it
//! iterates a Go map literal whose order the spec leaves undefined. There is no
//! single upstream output to match, so this port picks the literal's source
//! order (`%25`, `%0D`, `%0A`) and pins it with a test. See [`command`].

pub mod command;
pub mod expression;
pub mod job_container;
pub mod node_tool;
pub mod run_context;
pub mod step;
pub mod step_executor;
pub mod step_run;
