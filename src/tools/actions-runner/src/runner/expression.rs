//! Interpolation: folding `${{ … }}` that is embedded in a larger string into a
//! single `format()` call.
//!
//! Port of act's `pkg/runner/expression.go`.
//!
//! Three halves are here.
//!
//! **The rewriter** — [`rewrite_sub_expression`] folds `${{ … }}` embedded in a
//! larger string into one `format()` call; it is pure and needs nothing else.
//!
//! **The context tree** — `github`, `env`, `job`, `steps`, `runner`, `secrets`,
//! `strategy`, `matrix`, `needs`, `inputs` — arrives with
//! [`super::run_context`]. What does not need a live run is here already:
//! [`strategy_context`], [`needs_context`], [`steps_context`], [`env_context`],
//! [`secrets_context`], [`secrets_for_call`], assembled by [`EvaluationInputs`].
//!
//! **The two entry points** — [`interpolate`] for a string and
//! [`evaluate_yaml_node`] for a YAML subtree, plus [`eval_bool`] for a
//! condition. All three differ in exactly one flag and one place, which is the
//! most load-bearing detail in the file:
//!
//! | | rewrite | consequence |
//! |---|---|---|
//! | [`interpolate`] | `force_format = true` | even a lone expression goes through `format()`, so `false` arrives as the **text** `"false"` |
//! | [`eval_bool`], [`evaluate_yaml_node`] | `force_format = false` | the expression reaches the evaluator **bare**, so `false` stays a `bool` |
//!
//! Swapping the two flags breaks a workflow in a way no test on the other half
//! would notice: an `if:` that is always true, or a `run:` body carrying the
//! literal text `false`. `a_condition_and_an_interpolation_read_the_same_expression_differently`
//! exists to make that swap fail loudly.
//!
//! Still missing from upstream's file: the `getEvaluatorInputs` assembly of the
//! `github`, `runner` and `inputs` halves, which needs `RunContext.caller` and a
//! live `ExprEval` for the reusable-workflow path.
//!
//! It is not a second implementation of the language: [`crate::expr`] already
//! provides `success()`, `always()`, `contains`, the `hashFiles` hook and
//! `default_status_check`, and this module's output is handed straight to it.
//! The test `the_rewritten_form_evaluates_through_the_real_interpreter` exists
//! to keep the two halves honest about each other.
//!
//! # Why a rewriter is needed at all
//!
//! `${{ x }}` is an *expression*. `echo ${{ x }} ${{ y }}` is not one expression
//! but three pieces — an expression, a literal, an expression — and the
//! expression evaluator only takes one. The rewriter splices them into a single
//! call: `format('{0} {1}', x, y)`. GitHub does the same thing internally, which
//! is why `format('echo Hello {0} ${{Test}}', 'World')` evaluates to
//! `echo Hello World ${Test}` and not to a parse error.
//!
//! # The scanner is a three-state machine, and quotes are the whole difficulty
//!
//! `}}` closes an expression — unless it is inside a string literal. So the
//! scanner has to know whether it is inside one, and `'}}'` has to *not* close
//! anything while `'''}}` opens a string that swallows the `}}`. The state
//! machine is byte-oriented, and it scans forward over string literals with the
//! regex `(?:''|[^'])*'` — `''` being an escaped quote, so a run of quotes is
//! consumed in pairs and only an odd one opens a string.
//!
//! That regex is the one place in this module where the *engine* could differ,
//! not just the code: Go's RE2 and Rust's `regex` crate both promise leftmost-first
//! submatch semantics, but a `''`-alternation is exactly the sort of pattern
//! where a greedy repetition and a backtracker can land on different closing
//! quotes. So every case below was run through the **real** upstream function
//! first, and the expectations are its output, not a reading of the pattern.
//!
//! Measured, `force_format = false`:
//!
//! | input | output |
//! |---|---|
//! | `${{ true }}` | `${{ true }}` (unchanged) |
//! | `${{ true }} ${{ true }}` | `format('{0} {1}', true, true)` |
//! | `${{ '}}' }}` | unchanged — the `}}` is inside the string |
//! | `${{ '''}}''' }}` | unchanged — `''` escapes, the third `'` opens, the `}}` is inside |
//! | `${{ x }} }}` | `format('{0} }}}}', x)` — a literal `}` has to be doubled |
//!
//! The last row is the other half of the job: the literal text between
//! expressions is escaped, because a `}` in it would otherwise be read as a
//! format placeholder.
//!
//! # One deliberate deviation: a bad expression is an error, not a panic
//!
//! Upstream `panic`s on two inputs — an unterminated string literal and an
//! unterminated expression — even though the function already returns an
//! `error`. Both are reachable from a malformed workflow, and act really does
//! crash on them. This port returns [`RewriteError`] instead. Nothing that act
//! survives can reach the difference, because the only inputs that differ are
//! the two that would have taken the process down, and CTOX embeds this engine
//! in a long-lived service where a panic is a far worse outcome than a
//! reported error. The three measured cases are pinned as errors below.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::model::{Run, StepResult};

/// The two ways the rewriter can fail, where upstream panics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewriteError {
    /// A `'` opened a string literal that is never closed.
    UnclosedString,
    /// A `${{` opened an expression that is never closed.
    UnclosedExpression,
}

impl std::fmt::Display for RewriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // The wording is upstream's panic text, kept so a log line names
            // the same condition act would have.
            RewriteError::UnclosedString => f.write_str("unclosed string."),
            RewriteError::UnclosedExpression => f.write_str("unclosed expression."),
        }
    }
}

impl std::error::Error for RewriteError {}

/// A rewrite failure *is* an evaluation failure from every caller's point of
/// view: upstream reaches it through a `panic` inside the same expression
/// pipeline, so a caller that catches evaluation errors should catch these too
/// rather than having to name a second type.
impl From<RewriteError> for crate::expr::EvalError {
    fn from(error: RewriteError) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

/// `escapeFormatString`: double every brace in the literal text between
/// expressions, so the `format()` call does not read them as placeholders.
///
/// The two passes are sequential and that matters: `{` is doubled first, then
/// `}`. The first pass introduces no `}`, so each `}` is doubled exactly once.
pub fn escape_format_string(input: &str) -> String {
    input.replace('{', "{{").replace('}', "}}")
}

/// A string literal, with `''` as the escaped quote.
fn string_pattern() -> &'static regex::Regex {
    static PATTERN: OnceLock<regex::Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        regex::Regex::new(r"(?:''|[^'])*'").expect("the string-literal pattern is a constant")
    })
}

/// `strings.Index`: the byte offset of `needle`, or `-1`.
fn index_of(haystack: &str, needle: &str) -> isize {
    match haystack.find(needle) {
        Some(at) => at as isize,
        None => -1,
    }
}

/// Appends the `{n}` placeholder for the next extracted expression.
fn push_placeholder(out: &mut String, index: usize) {
    out.push('{');
    out.push_str(&index.to_string());
    out.push('}');
}

/// `rewriteSubExpression`: fold the `${{ … }}` in `input` into one `format()`
/// call.
///
/// With `force_format` false, a string that is *exactly* one expression and
/// nothing else is returned untouched — so `${{ x }}` stays `${{ x }}` and is
/// parsed as a bare expression, with the value's own type, rather than being
/// flattened to a string through `format()`. That is the difference between
/// `if: ${{ steps.a.outputs.n }}` seeing a boolean and seeing `"false"`.
/// `Interpolate` passes `true` because its result is a string either way.
///
/// Literal text between the expressions is escaped, and single quotes in it
/// are doubled, because it ends up inside a `'…'` literal.
pub fn rewrite_sub_expression(input: &str, force_format: bool) -> Result<String, RewriteError> {
    if !input.contains("${{") || !input.contains("}}") {
        return Ok(input.to_string());
    }

    let string_pattern = string_pattern();
    let mut pos = 0usize;
    // -1 is upstream's "not in a state" marker for both, kept rather than
    // replaced with an `Option` so the branch order can be read against the Go.
    let mut expr_start: isize = -1;
    let mut str_start: isize = -1;
    let mut results: Vec<String> = Vec::new();
    let mut format_out = String::new();

    while pos < input.len() {
        let tail = &input[pos..];
        if str_start > -1 {
            // Inside a string literal: skip past its closing quote, consuming
            // `''` pairs on the way so an odd quote is the one that opens.
            let matched = string_pattern
                .find(tail)
                .ok_or(RewriteError::UnclosedString)?;
            str_start = -1;
            pos += matched.end();
        } else if expr_start > -1 {
            let mut expr_end = index_of(tail, "}}");
            str_start = index_of(tail, "'");
            // Whichever comes first decides which construct we are in: a quote
            // before the `}}` means the `}}` is inside a string, and vice
            // versa. The other one is discarded so the branch below takes it.
            if expr_end > -1 && str_start > -1 {
                if expr_end < str_start {
                    str_start = -1;
                } else {
                    expr_end = -1;
                }
            }
            if expr_end > -1 {
                push_placeholder(&mut format_out, results.len());
                let start = expr_start as usize;
                let end = pos + expr_end as usize;
                results.push(input[start..end].trim().to_string());
                pos += expr_end as usize + 2;
                expr_start = -1;
            } else if str_start > -1 {
                // A quote opens a string that the `}}` is inside; step over the
                // quote and look again.
                pos += str_start as usize + 1;
            } else {
                return Err(RewriteError::UnclosedExpression);
            }
        } else if let Some(at) = tail.find("${{") {
            format_out.push_str(&escape_format_string(&tail[..at]));
            expr_start = (pos + at + 3) as isize;
            pos = expr_start as usize;
        } else {
            format_out.push_str(&escape_format_string(tail));
            pos = input.len();
        }
    }

    if results.len() == 1 && format_out == "{0}" && !force_format {
        return Ok(input.to_string());
    }

    Ok(format!(
        "format('{}', {})",
        format_out.replace('\'', "''"),
        results.join(", ")
    ))
}

/// Everything the evaluator reads, assembled from a
/// [`RunContext`](super::run_context::RunContext).
///
/// The eleven contexts a workflow can name, and where each comes from:
///
/// | context | from |
/// |---|---|
/// | `github` | [`get_github_context`](super::run_context::RunContext::get_github_context) |
/// | `env` | the run's environment, or a step's own for a step evaluator |
/// | `job` | [`get_job_context`](super::run_context::RunContext::get_job_context) — only `status` |
/// | `steps` | [`get_steps_context`](super::run_context::RunContext::get_steps_context) |
/// | `runner` | the job container's `runner_context` |
/// | `secrets` | the config, or the caller's for a reusable workflow |
/// | `vars` | the config, unchanged |
/// | `strategy` | the job's resolved `fail-fast` and `max-parallel` |
/// | `matrix` | the run's matrix cell |
/// | `needs` | each needed job's outputs and result |
/// | `inputs` | `INPUT_*`, and the dispatch or call inputs |
///
/// # `strategy` holds the *resolved* values, and resolution is a side effect
///
/// The model keeps `fail-fast:` and `max-parallel:` as strings because YAML
/// allows either a boolean or a number. Upstream turns them into a `bool` and
/// an `int` with act's defaults already applied — `true` and `4` — so a
/// workflow writing `strategy.max-parallel == 4` on a job that never mentions
/// it sees `4`, not an empty string.
///
/// That normalisation is **not** a decode step, which is the part worth
/// knowing. `Job.GetMatrixes()` (`pkg/model/workflow.go:401`) assigns
/// `Strategy.FailFast` and `Strategy.MaxParallel` as its first act, and it is
/// the only caller. `runner.go:172` calls it before any job executes, so every
/// expression a *step* evaluates sees resolved values. Measured on upstream
/// v0.2.89, one job with `strategy: {}`:
///
/// | when | `FailFast` | `MaxParallel` |
/// |---|---|---|
/// | straight after decoding | `false` | `0` |
/// | after `GetMatrixes()` | `true` | `4` |
///
/// The zero row is reachable: `runner.go:166` builds an evaluator to resolve
/// `strategy.matrix` *before* line 172 expands it, and that evaluator's
/// `strategy` context reads `false` and `0`. Here
/// [`Job::fail_fast`](crate::model::Job::fail_fast) and
/// [`Job::max_parallel`](crate::model::Job::max_parallel) compute the same
/// numbers on read instead of storing
/// them, so every post-expansion read agrees and the pre-expansion read does
/// not. Whoever ports `runner.go` must therefore expand the matrix before
/// executing a job, exactly as upstream does.
#[derive(Debug, Clone, Default)]
pub struct EvaluationInputs {
    /// The eleven contexts, ready for [`crate::expr::Interpreter`].
    pub environment: crate::expr::EvaluationEnvironment,
}

/// The `strategy` context, or nothing when the job declares no strategy.
///
/// Upstream builds an empty map when there is no `strategy:`, and
/// `strategy.fail-fast` then reads as absent rather than as `false`. That
/// difference is kept: a workflow may test for the context's existence.
pub fn strategy_context(run: Option<&Run>) -> BTreeMap<String, crate::expr::Value> {
    use crate::expr::Value;
    let Some(job) = run.and_then(|run| run.job()) else {
        return BTreeMap::new();
    };
    if job.strategy.is_none() {
        return BTreeMap::new();
    }
    BTreeMap::from([
        ("fail-fast".to_string(), Value::Bool(job.fail_fast())),
        ("max-parallel".to_string(), Value::Int(job.max_parallel())),
    ])
}

/// The `needs` context: each needed job's outputs and result.
///
/// # Deliberate deviation: a dangling `needs:` does not panic
///
/// Upstream indexes the map directly — `jobs[needs].Outputs` and
/// `jobs[needs].Result` (`pkg/runner/expression.go:50-52`) — and `Needs()`
/// does **not** filter the list against the workflow, so a need naming a job
/// that does not exist yields a nil `*model.Job` and the field read panics.
/// Measured on v0.2.89 with `needs: [nope]`:
///
/// ```text
/// runtime error: invalid memory address or nil pointer dereference
/// ```
///
/// It is reachable, not merely theoretical: `createStages` skips an unknown
/// job id (`pkg/model/planner.go:350`, guarded by `w.GetJob(jID) != nil`) but
/// leaves the *declaring* job in the graph, so the job runs and then panics
/// while its first evaluator is built.
///
/// Here the key is kept with empty members, so a typo in `needs:` surfaces as a
/// missing value at the use site instead of a crash. That is a real behavioural
/// difference and not a fidelity win — it is recorded here so nobody reads the
/// code as matching upstream.
pub fn needs_context(run: Option<&Run>) -> BTreeMap<String, crate::expr::Value> {
    use crate::expr::Value;
    let mut out = BTreeMap::new();
    let Some(run) = run else { return out };
    let Some(job) = run.job() else { return out };
    let doc = run.document();
    for need in job.needs(doc) {
        let needed = run.workflow.jobs.get(&need);
        let outputs = needed
            .map(|job| {
                job.outputs
                    .iter()
                    .map(|(key, value)| (key.clone(), Value::String(value.clone())))
                    .collect()
            })
            .unwrap_or_default();
        let result = needed
            .map(|job| Value::String(job.result.clone()))
            .unwrap_or(Value::Null);
        out.insert(
            need,
            Value::object([("outputs", Value::Object(outputs)), ("result", result)]),
        );
    }
    out
}

/// The `steps` context: what every finished step produced.
///
/// `conclusion` and `outcome` are **both** exposed because they differ exactly
/// where it matters: a `continue-on-error` step concluded `success` and ended as
/// `failure`, and a workflow branching on the wrong one takes the wrong branch.
pub fn steps_context(
    results: &BTreeMap<String, StepResult>,
) -> BTreeMap<String, crate::expr::Value> {
    use crate::expr::Value;
    results
        .iter()
        .map(|(id, result)| {
            (
                id.clone(),
                Value::object([
                    (
                        "conclusion",
                        Value::String(result.conclusion.as_str().to_string()),
                    ),
                    (
                        "outcome",
                        Value::String(result.outcome.as_str().to_string()),
                    ),
                    (
                        "outputs",
                        Value::Object(
                            result
                                .outputs
                                .iter()
                                .map(|(key, value)| (key.clone(), Value::String(value.clone())))
                                .collect(),
                        ),
                    ),
                ]),
            )
        })
        .collect()
}

/// The `env` context.
pub fn env_context(env: &BTreeMap<String, String>) -> BTreeMap<String, crate::expr::Value> {
    use crate::expr::Value;
    env.iter()
        .map(|(key, value)| (key.clone(), Value::String(value.clone())))
        .collect()
}

/// The `secrets` and `vars` contexts, both straight from the config.
///
/// For a reusable workflow the secrets come from the **caller**, and each value
/// is interpolated in the caller's context first — a secret may itself be an
/// expression. The caller is not available here; [`secrets_for_call`] takes the
/// already-resolved map instead.
pub fn secrets_context(secrets: &BTreeMap<String, String>) -> BTreeMap<String, crate::expr::Value> {
    env_context(secrets)
}

/// The already-resolved secrets of a called workflow.
pub fn secrets_for_call(
    secrets: &BTreeMap<String, String>,
) -> BTreeMap<String, crate::expr::Value> {
    env_context(secrets)
}

// ---------------------------------------------------------------------------
// getEvaluatorInputs and the evaluator it feeds
// ---------------------------------------------------------------------------

/// `getNeedsTransitive`: a job's own `needs:` plus, for each of them, their own.
///
/// The recursion is what makes it transitive — each call resolves its job's
/// parents completely before the parent list is appended, so one pass suffices
/// and the `range` in upstream never sees the appended entries.
///
/// **Duplicates are kept.** Two jobs that both need a third put it in the list
/// twice, because the list is built by concatenation and nothing deduplicates
/// it. It cannot change the answer — `jobSuccess` and `jobFailure` both ask a
/// yes/no question over the list — so the port does not deduplicate either.
///
/// **Deliberate deviation: a dangling `needs:` does not panic.** Upstream
/// recurses into `GetJob(need)`, which returns nil for a job the workflow does
/// not define, and then calls `Needs()` on that nil. Same deviation, and the
/// same reason, as [`needs_context`]: the key stays with empty members instead
/// of taking the process down.
pub fn needs_transitive(run: &Run) -> Vec<String> {
    if run.job().is_none() {
        return Vec::new();
    }
    needs_transitive_of(run, &run.job_id)
}

/// [`needs_transitive`] with the actual recursion, by job id.
fn needs_transitive_of(run: &Run, job_id: &str) -> Vec<String> {
    let Some(job) = run.workflow.jobs.get(job_id) else {
        return Vec::new();
    };
    let doc = run.document();
    let mut needs = job.needs(doc);
    for need in job.needs(doc) {
        needs.extend(needs_transitive_of(run, &need));
    }
    needs
}

/// The status hook, answered from values taken when the evaluator was built.
///
/// # Why this is a snapshot and not a borrow
///
/// Upstream reaches back into `model.Run` on every call to `success()`. The
/// Rust inverts that behind [`crate::expr::StatusProvider`], and a provider
/// that borrowed the [`RunContext`](super::run_context::RunContext) it was
/// built from could not be stored next
/// to it. Snapshotting the two things the four methods actually read — the
/// transitive `needs` results and this job's status — removes the lifetime
/// without changing a single answer, and the two cannot drift because there is
/// only one of each.
///
/// # What each function reads
///
/// | function | reads | for a job with no `needs:` |
/// |---|---|---|
/// | `success()` | every transitive result `== "success"` | `true` |
/// | `failure()` | any transitive result `== "failure"` | `false` |
/// | `cancelled()` | *this* job's status, **not** the needs chain | job status |
/// | `always()` | nothing | `true` |
///
/// The asymmetry is upstream's and is worth stating: a job that was cancelled
/// does not make its dependants `cancelled()`, and a *step* context reads the
/// job status where a *job* context reads the needs chain. `cancelled()` is the
/// one function that never looks at `needs`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunStatus {
    /// Every transitive `needs` result, in the order `needs_transitive` gives.
    needs_results: Vec<String>,
    /// `getJobContext().status`, which is also what the step-scoped
    /// `success()`/`failure()` read.
    job_status: String,
}

impl RunStatus {
    /// Builds the snapshot for a run context.
    pub fn new(rc: &super::run_context::RunContext) -> Self {
        let mut needs_results = Vec::new();
        if let Some(run) = &rc.run {
            for need in needs_transitive_of(run, &run.job_id) {
                let result = run
                    .workflow
                    .jobs
                    .get(&need)
                    .map(|job| job.result.clone())
                    .unwrap_or_default();
                needs_results.push(result);
            }
        }
        Self {
            needs_results,
            job_status: rc.get_job_context().status,
        }
    }
}

impl crate::expr::StatusProvider for RunStatus {
    fn job_success(&self) -> bool {
        self.needs_results.iter().all(|result| result == "success")
    }
    fn step_success(&self) -> bool {
        self.job_status == "success"
    }
    fn job_failure(&self) -> bool {
        self.needs_results.iter().any(|result| result == "failure")
    }
    fn step_failure(&self) -> bool {
        self.job_status == "failure"
    }
    fn cancelled(&self) -> bool {
        self.job_status == "cancelled"
    }
}

/// `getEvaluatorInputs`: the `inputs` context, from four sources in this order.
///
/// 1. **The caller's values**, when this run *is* a reusable workflow call
///    (the private `setup_workflow_inputs`). Only then does the context exist
///    at all.
/// 2. **`INPUT_*` environment variables**, lower-cased and stripped of the
///    prefix. This is what an `action`-style `with:` block produces, and it
///    applies to every run, not only to a call.
/// 3. **`workflow_dispatch` declarations**, but only when this run is *not* a
///    call — a called workflow gets its inputs from the caller, and reading the
///    dispatch defaults as well would let a dispatched value override a passed
///    one.
/// 4. **`workflow_call` declarations**, the counterpart for a call.
///
/// Sources 3 and 4 therefore never both fire, and the guard is not an
/// optimisation.
pub fn get_evaluator_inputs(
    rc: &super::run_context::RunContext,
    env: &BTreeMap<String, String>,
    github: &crate::model::GithubContext,
) -> BTreeMap<String, crate::expr::Value> {
    use crate::expr::Value;
    let mut inputs = setup_workflow_inputs(rc);

    for (key, value) in env {
        if let Some(name) = key.strip_prefix("INPUT_") {
            inputs.insert(name.to_lowercase(), Value::String(value.clone()));
        }
    }

    let is_call = rc.caller.is_some();

    if !is_call && github.event_name == "workflow_dispatch" {
        if let Some(run) = &rc.run {
            if let Some(declared) = run.workflow.workflow_dispatch_inputs(run.document()) {
                for (name, input) in declared {
                    let value = event_input(&github.event, &name)
                        .map(Value::String)
                        .unwrap_or_else(|| Value::String(input.default.clone()));
                    inputs.insert(name, coerce_declared(&input.input_type, value));
                }
            }
        }
    }

    if github.event_name == "workflow_call" {
        if let Some(run) = &rc.run {
            if let Some(declared) = run.workflow.workflow_call_inputs(run.document()) {
                for (name, input) in declared {
                    let value = event_input(&github.event, &name)
                        .map(Value::String)
                        .unwrap_or_else(|| Value::String(input.default.clone()));
                    inputs.insert(name, coerce_declared(&input.input_type, value));
                }
            }
        }
    }

    inputs
}

/// `ghc.event.inputs.<name>`, as text.
fn event_input(event: &serde_json::Map<String, serde_json::Value>, name: &str) -> Option<String> {
    crate::model::github_context::nested_map_lookup(event, &["inputs", name])
        .and_then(|value| value.as_str().map(str::to_string))
}

/// A declared input whose `type:` is `boolean` becomes a real boolean, and
/// only when its value is **exactly** the string `true`.
///
/// This reproduces an upstream comparison rather than a reasonable one.
/// Go writes `inputs[k] = value == "true"` where `value` is an `interface{}`,
/// so the comparison is a *typed* one: the string `"true"` matches, and a
/// YAML boolean `true` does **not**. A reusable workflow declaring
/// `default: true` with `type: boolean` therefore gets `inputs.x == false`,
/// while passing `x: "true"` over the wire gets `true`.
///
/// That is upstream's answer and not a Go/Rust artifact, so it is kept. A
/// workflow that depends on the saner reading is broken on act too, and
/// fixing it here would make the port disagree with the thing it ports.
fn coerce_declared(input_type: &str, value: crate::expr::Value) -> crate::expr::Value {
    use crate::expr::Value;
    if input_type == "boolean" {
        Value::Bool(matches!(&value, Value::String(text) if text == "true"))
    } else {
        value
    }
}

/// `setupWorkflowInputs`: a called workflow's inputs, taken from the caller.
///
/// Every declared input gets an entry, whether or not the caller passed a
/// value — a declared-but-unpassed input reads as absent, which is what lets
/// `inputs.<name> == null` distinguish "not passed" from "never declared".
///
/// The caller's value is interpolated in the **caller's** context before it
/// arrives, and the default in the **called** context, so the two are not
/// evaluated the same way. Both need an evaluator, which this run does not yet
/// have while it is still being built, so both are left to the caller: what is
/// returned here is the raw pass-through, and the values are re-evaluated by
/// whoever assembles the final environment. Recorded here because it is a real
/// seam, not an omission.
fn setup_workflow_inputs(
    rc: &super::run_context::RunContext,
) -> BTreeMap<String, crate::expr::Value> {
    use crate::expr::Value;
    let mut inputs = BTreeMap::new();
    let Some(caller) = &rc.caller else {
        return inputs;
    };
    let Some(run) = &rc.run else {
        return inputs;
    };
    let Some(declared) = run.workflow.workflow_call_inputs(run.document()) else {
        return inputs;
    };
    let passed = caller
        .run_context
        .run
        .as_ref()
        .and_then(|run| run.job())
        .map(|job| job.with.clone())
        .unwrap_or_default();
    for (name, input) in declared {
        // A passed value keeps its own type; an unpassed one falls back to the
        // declared default, which the model holds as raw text.
        let value = passed
            .get(&name)
            .cloned()
            .unwrap_or_else(|| Value::String(input.default.clone()));
        inputs.insert(name, coerce_declared(&input.input_type, value));
    }
    inputs
}

/// `NewExpressionEvaluatorWithEnv`: the whole `EvaluationEnvironment`.
///
/// The order of the fields is upstream's and is not significant, but the two
/// that are *absent* upstream are significant: `runner` is only set when a job
/// container exists, and `jobs` only when this run is a call whose own
/// evaluator already exists. Both are `None`/empty otherwise, so a workflow can
/// test for the context's presence.
pub fn new_expression_evaluator_with_env(
    rc: &super::run_context::RunContext,
    env: &BTreeMap<String, String>,
    github: &crate::model::GithubContext,
) -> crate::expr::EvaluationEnvironment {
    let mut jobs = None;
    if rc.caller.is_some() {
        let mut map = BTreeMap::new();
        if let Some(run) = &rc.run {
            for (name, job) in &run.workflow.jobs {
                map.insert(
                    name.clone(),
                    crate::expr::Value::object([(
                        "outputs",
                        crate::expr::Value::Object(
                            job.outputs
                                .iter()
                                .map(|(key, value)| {
                                    (key.clone(), crate::expr::Value::String(value.clone()))
                                })
                                .collect(),
                        ),
                    )]),
                );
            }
        }
        jobs = Some(map);
    }

    let secrets = if let Some(caller) = &rc.caller {
        let call_doc = caller.run_context.run.as_ref().map(|run| run.document());
        let job = caller.run_context.run.as_ref().and_then(|run| run.job());
        // Go distinguishes "no `secrets:`" (nil) from "`secrets: {}`" (empty
        // but not nil), and only inherits in the first case. `raw_secrets` is
        // what carries that difference here, so the check is on the node's
        // presence rather than on the decoded map being empty.
        let explicit = job.and_then(|job| {
            let doc = call_doc?;
            if job.raw_secrets.is_some() {
                Some(job.secrets(doc))
            } else {
                None
            }
        });
        match explicit {
            Some(secrets) => secrets,
            None => {
                let inherits = job
                    .zip(call_doc)
                    .is_some_and(|(job, doc)| job.inherit_secrets(doc));
                if inherits {
                    caller.run_context.config.secrets.clone()
                } else {
                    BTreeMap::new()
                }
            }
        }
    } else {
        rc.config.secrets.clone()
    };

    let runner = match &rc.job_container {
        // Upstream reads this off the container. Until the lifecycle lands,
        // there is no container to ask, so the context is empty rather than
        // invented — a named gap, recorded on the field.
        Some(_) => BTreeMap::new(),
        None => BTreeMap::new(),
    };

    crate::expr::EvaluationEnvironment {
        github: Some(github.to_value()),
        env: env_context(env),
        job: Some(crate::expr::Value::object([(
            "status",
            crate::expr::Value::String(rc.get_job_context().status),
        )])),
        jobs,
        steps: steps_context(&rc.step_results),
        runner,
        secrets: secrets_context(&secrets),
        vars: env_context(&rc.config.vars),
        strategy: strategy_context(rc.run.as_ref()),
        matrix: rc
            .matrix
            .iter()
            .map(|(key, value)| (key.clone(), crate::expr::from_json_value(value)))
            .collect(),
        needs: needs_context(rc.run.as_ref()),
        inputs: get_evaluator_inputs(rc, env, github),
        hash_files: None,
    }
}

/// Why [`interpolate`] produced no string.
///
/// Upstream has no such type: it logs the first case and `panic`s on the
/// second. Both are reproduced as errors so that the caller decides, and
/// [`InterpolateError::upstream_value`] is what upstream's own code would have
/// ended up with.
#[derive(Debug, Clone, PartialEq)]
pub enum InterpolateError {
    /// The expression did not evaluate.
    ///
    /// Upstream logs `Unable to interpolate expression '%s': %s` and returns
    /// `""`.
    Evaluate {
        /// The rewritten expression that failed.
        expression: String,
        /// Why it failed.
        error: crate::expr::EvalError,
    },
    /// The expression evaluated, but not to a string.
    ///
    /// Upstream `panic`s with `Expression %s did not evaluate to a string`.
    /// **Deliberate deviation**: this port does not panic. It cannot happen
    /// through the normal path — `Interpolate` rewrites with
    /// `force_format = true`, so every expression goes through `format()` and
    /// comes back a `Value::String` — but a rewriter bug that dropped the
    /// forced format would land here, and a panic in a worker is a worse
    /// failure than an error.
    NotAString {
        /// The rewritten expression.
        expression: String,
        /// What it evaluated to.
        value: crate::expr::Value,
    },
    /// The input could not be rewritten.
    ///
    /// Upstream discards `rewriteSubExpression`'s error here, which changes
    /// nothing: that function never returns a non-nil error, it `panic`s on
    /// the two malformed inputs. **Deliberate deviation** again — see
    /// [`rewrite_sub_expression`].
    Rewrite(crate::expr::EvalError),
}

impl InterpolateError {
    /// What upstream's `Interpolate` would have returned.
    ///
    /// `""` for everything, because `""` is what it returns for an evaluation
    /// error and what a panic's caller would not have survived to see.
    pub fn upstream_value(&self) -> String {
        String::new()
    }
}

impl std::fmt::Display for InterpolateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Evaluate { expression, error } => write!(
                formatter,
                "Unable to interpolate expression '{expression}': {error}"
            ),
            Self::NotAString { expression, value } => write!(
                formatter,
                "Expression {expression} did not evaluate to a string: {value:?}"
            ),
            Self::Rewrite(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for InterpolateError {}

/// `Interpolate`: fold every `${{ … }}` in `input` into one string.
///
/// This is the gate every workflow string passes through — a `run:` body, a
/// `with:` value, a secret. Two details carry the behaviour:
///
/// 1. **The rewrite is forced.** `force_format = true` wraps even a lone
///    expression in `format('{0}', …)`, so a boolean arrives as the text
///    `"false"` and not as a `false`. Without it, `${{ !env.X }}` in a `run:`
///    would leave the string untouched and a falsy value would read as a
///    non-empty string — true for *every* value.
/// 2. **The guard is on both delimiters.** A string holding `${{` without a
///    matching `}}` — or `}}` without an opening — is returned unchanged
///    rather than rewritten into a `format()` call that would then fail to
///    parse.
pub fn interpolate(
    environment: &crate::expr::EvaluationEnvironment,
    status: &dyn crate::expr::StatusProvider,
    context: crate::expr::EvaluationContext,
    input: &str,
) -> Result<String, InterpolateError> {
    if !input.contains("${{") || !input.contains("}}") {
        return Ok(input.to_string());
    }
    let expression = rewrite_sub_expression(input, true)
        .map_err(|error| InterpolateError::Rewrite(error.into()))?;
    let interpreter = crate::expr::Interpreter::new(environment, status, context);
    let value = interpreter
        .evaluate(&expression, crate::expr::DefaultStatusCheck::None)
        .map_err(|error| InterpolateError::Evaluate {
            expression: expression.clone(),
            error,
        })?;
    match value {
        crate::expr::Value::String(text) => Ok(text),
        other => Err(InterpolateError::NotAString {
            expression,
            value: other,
        }),
    }
}

/// `EvalBool`: an expression as a condition, through `IsTruthy`.
///
/// The rewrite here is **not** forced — that is the difference from
/// [`interpolate`], and it is the whole point. A `condition:` that is a single
/// `${{ … }}` must reach the evaluator as a bare expression so that the value's
/// own type decides the answer; wrapping it in `format()` would turn `false`
/// into the non-empty string `"false"`, which is truthy, and every `if:` on a
/// false value would pass.
pub fn eval_bool(
    environment: &crate::expr::EvaluationEnvironment,
    status: &dyn crate::expr::StatusProvider,
    context: crate::expr::EvaluationContext,
    expression: &str,
    default_status_check: crate::expr::DefaultStatusCheck,
) -> Result<bool, crate::expr::EvalError> {
    let rewritten = rewrite_sub_expression(expression, false)?;
    let interpreter = crate::expr::Interpreter::new(environment, status, context);
    let value = interpreter.evaluate(&rewritten, default_status_check)?;
    Ok(crate::expr::is_truthy(&value))
}

/// The `insert` directive: a mapping key of exactly `${{ insert }}`.
///
/// GitHub's undocumented way to splice a map into its parent, and it is what
/// makes a reusable workflow able to add a caller's keys. The key is matched as
/// **raw text**, before evaluation — `${{insert}}`, `${{  insert  }}` and
/// `${{ INSERT }}` all match, and `${{ insert }} and more` does not.
fn is_insert_directive(key: &str) -> bool {
    let trimmed = key.trim();
    trimmed.starts_with("${{") && trimmed.ends_with("}}") && {
        let body = &trimmed[3..trimmed.len() - 2];
        body.trim() == "insert"
    }
}

/// The replacement for a scalar that carried an expression.
///
/// Upstream `Encode`s the evaluated value into a fresh node, so the result
/// keeps the value's **own type** rather than its text: an expression that
/// evaluated to a number comes back as a number node, and one that evaluated to
/// a map comes back as a mapping. That matters because `EvaluateYamlNode`
/// writes the node back and the caller decodes it into a typed field, so
/// stringifying here would quietly turn a bool into a non-empty string — and a
/// truthy one.
fn value_to_node(
    document: &mut crate::yaml_node::Document,
    value: &crate::expr::Value,
) -> crate::yaml_node::NodeId {
    use crate::expr::Value;
    use crate::yaml_node::{Node, NodeKind};
    match value {
        Value::String(text) => {
            let mut node = Node::detached(NodeKind::Scalar);
            node.value = text.clone();
            document.alloc(node)
        }
        Value::Bool(value) => {
            tagged_scalar(document, if *value { "true" } else { "false" }, "!!bool")
        }
        Value::Int(value) => tagged_scalar(document, &value.to_string(), "!!int"),
        Value::Float(value) => {
            tagged_scalar(document, &crate::expr::format_float_g(*value), "!!float")
        }
        Value::Null => tagged_scalar(document, "null", "!!null"),
        Value::Array(items) => {
            let children: Vec<_> = items
                .iter()
                .map(|item| value_to_node(document, item))
                .collect();
            let mut node = Node::detached(NodeKind::Sequence);
            node.content = children;
            document.alloc(node)
        }
        Value::Object(members) => {
            let mut node = Node::detached(NodeKind::Mapping);
            for (key, member) in members {
                let mut key_node = Node::detached(NodeKind::Scalar);
                key_node.value = key.clone();
                let key_id = document.alloc(key_node);
                node.content.push(key_id);
                node.content.push(value_to_node(document, member));
            }
            document.alloc(node)
        }
    }
}

/// A scalar carrying an explicit YAML tag, as `Encode` does for a typed value.
fn tagged_scalar(
    document: &mut crate::yaml_node::Document,
    text: &str,
    tag: &str,
) -> crate::yaml_node::NodeId {
    use crate::yaml_node::Node;
    let mut node = Node::detached(crate::yaml_node::NodeKind::Scalar);
    node.value = text.to_string();
    node.tag = Some(tag.to_string());
    document.alloc(node)
}

/// `EvaluateYamlNode`: resolve every expression in the subtree rooted at `id`.
///
/// The node at `id` is replaced **in place** — its kind, tag and children are
/// overwritten — so a caller holding a `NodeId` keeps holding a valid handle.
/// Returns whether anything changed at all, which is how the walk knows to
/// leave the untouched parts of a collection alone instead of rebuilding them.
///
/// The rewrite is **not** forced here, unlike [`interpolate`]. A scalar that is
/// exactly one expression reaches the evaluator bare, so `if: ${{ !env.X }}`
/// comes back as a `false` and not as the string `"false"`. That is the
/// difference between a condition that works and one that is always true.
pub fn evaluate_yaml_node(
    document: &mut crate::yaml_node::Document,
    environment: &crate::expr::EvaluationEnvironment,
    status: &dyn crate::expr::StatusProvider,
    context: crate::expr::EvaluationContext,
    id: crate::yaml_node::NodeId,
) -> Result<bool, crate::expr::EvalError> {
    let Some(replacement) =
        evaluate_yaml_node_internal(document, environment, status, context.clone(), id)?
    else {
        return Ok(false);
    };
    let Some(incoming) = document.node(replacement).cloned() else {
        return Ok(false);
    };
    document.replace(id, incoming);
    Ok(true)
}

/// The recursive walk. `Ok(None)` means "this node needs no replacement",
/// which is what lets a collection skip an entry that did not change.
fn evaluate_yaml_node_internal(
    document: &mut crate::yaml_node::Document,
    environment: &crate::expr::EvaluationEnvironment,
    status: &dyn crate::expr::StatusProvider,
    context: crate::expr::EvaluationContext,
    id: crate::yaml_node::NodeId,
) -> Result<Option<crate::yaml_node::NodeId>, crate::expr::EvalError> {
    let Some(node) = document.node(id).cloned() else {
        return Ok(None);
    };
    match node.kind {
        crate::yaml_node::NodeKind::Scalar => {
            evaluate_scalar_yaml_node(document, environment, status, context, id)
        }
        crate::yaml_node::NodeKind::Mapping => {
            evaluate_mapping_yaml_node(document, environment, status, context, id)
        }
        crate::yaml_node::NodeKind::Sequence => {
            evaluate_sequence_yaml_node(document, environment, status, context, id)
        }
        _ => Ok(None),
    }
}

fn evaluate_scalar_yaml_node(
    document: &mut crate::yaml_node::Document,
    environment: &crate::expr::EvaluationEnvironment,
    status: &dyn crate::expr::StatusProvider,
    context: crate::expr::EvaluationContext,
    id: crate::yaml_node::NodeId,
) -> Result<Option<crate::yaml_node::NodeId>, crate::expr::EvalError> {
    let Some(text) = document.scalar(id) else {
        return Ok(None);
    };
    // Both delimiters, not just the opening one: a bare `}}` in a `run:` body
    // is ordinary text and must not send the string through a `format()` call.
    if !text.contains("${{") || !text.contains("}}") {
        return Ok(None);
    }
    let expression = rewrite_sub_expression(&text, false)?;
    let interpreter = crate::expr::Interpreter::new(environment, status, context);
    let value = interpreter.evaluate(&expression, crate::expr::DefaultStatusCheck::None)?;
    Ok(Some(value_to_node(document, &value)))
}

fn evaluate_mapping_yaml_node(
    document: &mut crate::yaml_node::Document,
    environment: &crate::expr::EvaluationEnvironment,
    status: &dyn crate::expr::StatusProvider,
    context: crate::expr::EvaluationContext,
    id: crate::yaml_node::NodeId,
) -> Result<Option<crate::yaml_node::NodeId>, crate::expr::EvalError> {
    use crate::yaml_node::{Node, NodeKind};
    let entries = document.map_entries(id);
    // `None` until the first entry that actually changes. Upstream deep-copies
    // the whole node and truncates its children to the unchanged prefix at that
    // moment, so the prefix is carried here instead of appended entry by entry.
    let mut rebuilt: Option<Vec<crate::yaml_node::NodeId>> = None;

    for (index, (key, value)) in entries.iter().copied().enumerate() {
        let evaluated_value =
            evaluate_yaml_node_internal(document, environment, status, context.clone(), value)?;
        if rebuilt.is_none() && evaluated_value.is_some() {
            rebuilt = Some(flatten_prefix(&entries[..index]));
        }
        let value_id = evaluated_value.unwrap_or(value);

        let key_text = document.scalar(key).unwrap_or_default();
        if is_insert_directive(&key_text) {
            // Upstream's error text, kept because a workflow that trips it has
            // a `with:` whose value is not a map.
            if !document.node(value_id).is_some_and(Node::is_mapping) {
                return Err(crate::expr::EvalError {
                    message: format!(
                        "failed to insert node {} into mapping {} unexpected type {:?} expected MappingNode",
                        value_id,
                        document.node(id).map(Node::location).unwrap_or_default(),
                        document
                            .node(value_id)
                            .map(|node| format!("{:?}", node.kind))
                            .unwrap_or_default()
                    ),
                });
            }
            // An insert directive always materialises the node, even when the
            // value itself did not change — the key is going away either way.
            let prefix = rebuilt.get_or_insert_with(|| flatten_prefix(&entries[..index]));
            let spliced = document
                .node(value_id)
                .map(|node| node.content.clone())
                .unwrap_or_default();
            prefix.extend(spliced);
            continue;
        }

        let evaluated_key =
            evaluate_yaml_node_internal(document, environment, status, context.clone(), key)?;
        if rebuilt.is_none() && evaluated_key.is_some() {
            rebuilt = Some(flatten_prefix(&entries[..index]));
        }
        let key_id = evaluated_key.unwrap_or(key);
        if let Some(content) = rebuilt.as_mut() {
            content.push(key_id);
            content.push(value_id);
        }
    }

    let Some(content) = rebuilt else {
        return Ok(None);
    };
    let mut node = Node::detached(NodeKind::Mapping);
    node.content = content;
    Ok(Some(document.alloc(node)))
}

fn evaluate_sequence_yaml_node(
    document: &mut crate::yaml_node::Document,
    environment: &crate::expr::EvaluationEnvironment,
    status: &dyn crate::expr::StatusProvider,
    context: crate::expr::EvaluationContext,
    id: crate::yaml_node::NodeId,
) -> Result<Option<crate::yaml_node::NodeId>, crate::expr::EvalError> {
    use crate::yaml_node::{Node, NodeKind};
    let original: Vec<_> = document
        .node(id)
        .map(|node| node.content.clone())
        .unwrap_or_default();
    let mut rebuilt: Option<Vec<crate::yaml_node::NodeId>> = None;

    for (index, value) in original.iter().copied().enumerate() {
        // Whether the *source* item was a sequence, decided before evaluation.
        // An item that was already a sequence stays one entry, but an item that
        // became one through evaluation is spliced — that is how
        // `${{ fromJSON('[1,2]') }}` contributes two entries instead of one
        // list-shaped entry.
        let was_sequence = document.node(value).is_some_and(Node::is_sequence);
        let evaluated =
            evaluate_yaml_node_internal(document, environment, status, context.clone(), value)?;
        match evaluated {
            Some(evaluated) => {
                if rebuilt.is_none() {
                    rebuilt = Some(original[..index].to_vec());
                }
                let content = rebuilt.as_mut().expect("just materialised");
                if !was_sequence && document.node(evaluated).is_some_and(Node::is_sequence) {
                    let spliced = document
                        .node(evaluated)
                        .map(|node| node.content.clone())
                        .unwrap_or_default();
                    content.extend(spliced);
                } else {
                    content.push(evaluated);
                }
            }
            None => {
                // Unchanged, but only worth carrying once a sibling changed.
                if let Some(content) = rebuilt.as_mut() {
                    content.push(value);
                }
            }
        }
    }

    let Some(content) = rebuilt else {
        return Ok(None);
    };
    let mut node = Node::detached(NodeKind::Sequence);
    node.content = content;
    Ok(Some(document.alloc(node)))
}

/// The first `index` mapping entries as a flat key/value child list.
fn flatten_prefix(
    entries: &[(crate::yaml_node::NodeId, crate::yaml_node::NodeId)],
) -> Vec<crate::yaml_node::NodeId> {
    entries
        .iter()
        .flat_map(|(key, value)| [*key, *value])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{
        DefaultStatus, DefaultStatusCheck, EvaluationEnvironment, Interpreter, Value,
    };
    use crate::model::{StepStatus, Workflow};
    use crate::yaml_node::Document;
    use std::rc::Rc;

    /// The measured upstream cases: `(input, force_format = false, force_format = true)`.
    ///
    /// `None` is where upstream panics and this port returns an error. Every
    /// `Some` value is the real function's output, transcribed — not derived by
    /// reading the state machine, which is how the `${{ '''}}''' }}` fixture
    /// was first got wrong here.
    const MEASURED: &[(&str, Option<&str>, Option<&str>)] = &[
        // --- upstream's TestRewriteSubExpression table ---
        ("Hello World", Some("Hello World"), Some("Hello World")),
        (
            "${{ true }}",
            Some("${{ true }}"),
            Some("format('{0}', true)"),
        ),
        (
            "${{ true }} ${{ true }}",
            Some("format('{0} {1}', true, true)"),
            Some("format('{0} {1}', true, true)"),
        ),
        (
            "${{ true || false }} ${{ true && true }}",
            Some("format('{0} {1}', true || false, true && true)"),
            Some("format('{0} {1}', true || false, true && true)"),
        ),
        (
            "${{ '}}' }}",
            Some("${{ '}}' }}"),
            Some("format('{0}', '}}')"),
        ),
        (
            "${{ ''' ''' }}",
            Some("${{ ''' ''' }}"),
            Some("format('{0}', ''' ''')"),
        ),
        // The fixture is three quotes either side of the `}}`. `''` escapes to
        // one, so the third quote *opens* a string and the `}}` is inside it.
        // This is upstream's own line, byte-verified with `od -c`: read
        // visually it looks like `'''Clusters }}` and then it panics.
        (
            "${{ '''}}''' }}",
            Some("${{ '''}}''' }}"),
            Some("format('{0}', '''}}''')"),
        ),
        (
            "${{ '''' }}",
            Some("${{ '''' }}"),
            Some("format('{0}', '''')"),
        ),
        (
            "${{ fromJSON('\"}}\"') }}",
            Some("${{ fromJSON('\"}}\"') }}"),
            Some("format('{0}', fromJSON('\"}}\"'))"),
        ),
        (
            "${{ fromJSON('\"\\\"}}\\\"\"') }}",
            Some("${{ fromJSON('\"\\\"}}\\\"\"') }}"),
            Some("format('{0}', fromJSON('\"\\\"}}\\\"\"'))"),
        ),
        (
            "${{ fromJSON('\"''}}\"') }}",
            Some("${{ fromJSON('\"''}}\"') }}"),
            Some("format('{0}', fromJSON('\"''}}\"'))"),
        ),
        (
            "Hello ${{ 'World' }}",
            Some("format('Hello {0}', 'World')"),
            Some("format('Hello {0}', 'World')"),
        ),
        // --- upstream's TestRewriteSubExpressionForceFormat table ---
        // --- and the cases measured because they are where the two regex
        //     engines could have differed ---
        (
            "${{ ''''}}''}}",
            Some("format('{0}''''}}}}', '''')"),
            Some("format('{0}''''}}}}', '''')"),
        ),
        ("${{ '' }}", Some("${{ '' }}"), Some("format('{0}', '')")),
        (
            "${{ 'a}}b' }}",
            Some("${{ 'a}}b' }}"),
            Some("format('{0}', 'a}}b')"),
        ),
        (
            "${{ 'a' }}${{ 'b' }}",
            Some("format('{0}{1}', 'a', 'b')"),
            Some("format('{0}{1}', 'a', 'b')"),
        ),
        (
            "a${{ x }}b${{ y }}c",
            Some("format('a{0}b{1}c', x, y)"),
            Some("format('a{0}b{1}c', x, y)"),
        ),
        (
            "${{ x }}${{ y }}",
            Some("format('{0}{1}', x, y)"),
            Some("format('{0}{1}', x, y)"),
        ),
        (
            "${{ a }} ${{ '}}' }} ${{ b }}",
            Some("format('{0} {1} {2}', a, '}}', b)"),
            Some("format('{0} {1} {2}', a, '}}', b)"),
        ),
        // A nested `format` is *not* re-entered: only the outer `${{` is
        // rewritten, so the inner braces go through the literal escaping.
        (
            "${{ format('{0}', x) }}",
            Some("${{ format('{0}', x) }}"),
            Some("format('{0}', format('{0}', x))"),
        ),
        // Literal braces in the text between expressions are doubled, once for
        // the `format` placeholder grammar and once more because the literal is
        // itself inside a `'…'`.
        (
            "${{ x }} }}",
            Some("format('{0} }}}}', x)"),
            Some("format('{0} }}}}', x)"),
        ),
        (
            "${{ env.x }} }}",
            Some("format('{0} }}}}', env.x)"),
            Some("format('{0} }}}}', env.x)"),
        ),
        (
            "${{ env.x }} ${{ env.y }}",
            Some("format('{0} {1}', env.x, env.y)"),
            Some("format('{0} {1}', env.x, env.y)"),
        ),
        (
            "}} before ${{ y }}",
            Some("format('}}}} before {0}', y)"),
            Some("format('}}}} before {0}', y)"),
        ),
        (
            "${{ ''}}' }}",
            Some("format('{0}'' }}}}', '')"),
            Some("format('{0}'' }}}}', '')"),
        ),
        (
            "${{ '''  ' }}x${{ y }}",
            Some("format('{0}x{1}', '''  ', y)"),
            Some("format('{0}x{1}', '''  ', y)"),
        ),
        (
            "${{ '}}' }} tail ${{ '}}' }}",
            Some("format('{0} tail {1}', '}}', '}}')"),
            Some("format('{0} tail {1}', '}}', '}}')"),
        ),
        // --- the three cases where upstream panics ---
        ("${{ '''}}", None, None),
        ("${{ '''''}}", None, None),
        ("${{ a'b }}", None, None),
        ("}} ${{ abc", None, None),
        ("${{ 'abc }}", None, None),
    ];

    /// Every measured case, for `force_format = false`.
    ///
    /// The twelve entries that are also upstream's `TestRewriteSubExpression`
    /// and the five that are its `TestRewriteSubExpressionForceFormat` are
    /// marked in the table above; the rest exist because a `''`-alternation is
    /// where two regex engines could disagree, and the only way to know is to
    /// run the original.
    #[test]
    fn every_measured_case_matches_for_both_force_flags() {
        for (input, without, with) in MEASURED {
            assert_eq!(
                rewrite_sub_expression(input, false).ok().as_deref(),
                *without,
                "force_format = false, input {input:?}"
            );
            assert_eq!(
                rewrite_sub_expression(input, true).ok().as_deref(),
                *with,
                "force_format = true, input {input:?}"
            );
        }
    }

    /// The three failing inputs report the two conditions distinctly, so a
    /// caller can tell an unterminated string from an unterminated expression.
    #[test]
    fn an_unterminated_construct_is_an_error_not_a_panic() {
        assert_eq!(
            rewrite_sub_expression("${{ 'abc }}", false),
            Err(RewriteError::UnclosedString)
        );
        assert_eq!(
            rewrite_sub_expression("}} ${{ abc", false),
            Err(RewriteError::UnclosedExpression)
        );
        assert_eq!(
            rewrite_sub_expression("${{ a'b }}", false),
            Err(RewriteError::UnclosedString),
            "an odd quote inside the expression opens a string nothing closes"
        );
        // The wording is upstream's panic text, so a log line names the same
        // condition act would have.
        assert_eq!(RewriteError::UnclosedString.to_string(), "unclosed string.");
        assert_eq!(
            RewriteError::UnclosedExpression.to_string(),
            "unclosed expression."
        );
    }

    /// Anything without both delimiters is returned untouched, without being
    /// scanned — so a `$` or a `{` in ordinary output costs nothing.
    #[test]
    fn a_string_without_both_delimiters_is_returned_as_is() {
        for input in [
            "just some text",
            "${{ unbalanced",
            "}} unbalanced",
            "a $ b { c } d",
            "",
            "$",
        ] {
            assert_eq!(
                rewrite_sub_expression(input, false).as_deref(),
                Ok(input),
                "input {input:?}"
            );
            assert_eq!(
                rewrite_sub_expression(input, true).as_deref(),
                Ok(input),
                "force_format must not change a string with no expression, {input:?}"
            );
        }
    }

    /// Multi-byte input, which is where a byte-oriented scanner would break if
    /// it indexed characters instead of bytes.
    #[test]
    fn multi_byte_surroundings_survive() {
        assert_eq!(
            rewrite_sub_expression("${{ 'héllo' }} ${{ 'wörld' }}", false).as_deref(),
            Ok("format('{0} {1}', 'héllo', 'wörld')")
        );
        assert_eq!(
            rewrite_sub_expression("vorher ${{ x }} nachher", false).as_deref(),
            Ok("format('vorher {0} nachher', x)")
        );
        assert_eq!(
            rewrite_sub_expression("${{ '日本語' }}", true).as_deref(),
            Ok("format('{0}', '日本語')")
        );
    }

    /// Mirrors the type assertion `Interpolate` makes on the result.
    ///
    /// Upstream does `evaluated.(string)` and panics when that fails. It never
    /// fails there precisely because `Interpolate` rewrites with
    /// `force_format = true`, so every value goes through `format()` and comes
    /// back a string — a boolean `false` arrives as `"false"`, not as a bool.
    /// Asserting it here is what would catch a regression that turned the
    /// forced rewrite back into a bare expression.
    fn as_interpolated(
        result: Result<crate::expr::Value, crate::expr::EvalError>,
    ) -> Result<String, String> {
        match result.map_err(|err| err.message)? {
            crate::expr::Value::String(text) => Ok(text),
            other => Err(format!("{other:?} did not evaluate to a string")),
        }
    }

    /// The point of the whole function: what it produces must be something the
    /// expression evaluator can actually run. A string comparison against
    /// upstream would still pass if the two halves disagreed about the shape of
    /// a `format()` call, so this is the test that closes that gap.
    #[test]
    fn the_rewritten_form_evaluates_through_the_real_interpreter() {
        let env = EvaluationEnvironment {
            env: [
                (
                    "KEY-WITH-HYPHENS".to_string(),
                    crate::expr::Value::String("hyphen".into()),
                ),
                (
                    "SOMETHING_TRUE".to_string(),
                    crate::expr::Value::String("true".into()),
                ),
                (
                    "SOMETHING_FALSE".to_string(),
                    crate::expr::Value::String("false".into()),
                ),
                ("x".to_string(), crate::expr::Value::String("X".into())),
            ]
            .into_iter()
            .collect(),
            ..EvaluationEnvironment::default()
        };
        let status = DefaultStatus;
        let eval = |input: &str| {
            let rewritten = rewrite_sub_expression(input, true).expect("a well-formed input");
            as_interpolated(
                Interpreter::new(&env, &status, crate::expr::EvaluationContext::Job)
                    .evaluate(&rewritten, DefaultStatusCheck::None),
            )
        };

        // A single expression stays a single expression, and the surrounding
        // literal is reassembled by `format`.
        assert_eq!(
            eval(" ${{ env.KEY-WITH-HYPHENS }} ").as_deref(),
            Ok(" hyphen ")
        );
        assert_eq!(
            eval(" ${{  (true || false)  }} to ${{2}} ").as_deref(),
            Ok(" true to 2 ")
        );
        // The literal `}}` in a string must survive reassembly, or a workflow
        // with `${{ '}}' }}` in it would come back with the brace eaten.
        assert_eq!(eval("${{ '}}' }}").as_deref(), Ok("}}"));
        assert_eq!(eval("${{ 'a' }}${{ 'b' }}").as_deref(), Ok("ab"));
        // The escaped-brace rule: `}}` in the surrounding text is literal text
        // and must not be read as a placeholder. The rewriter measures this
        // input as `format('{0} }}}}', env.x)`, and `}}}}` in a format pattern
        // is two literal braces, so the trailing text comes back intact.
        assert_eq!(eval("${{ env.x }} }}").as_deref(), Ok("X }}"));
        // The boolean cases from upstream's `TestInterpolate`, which are the
        // ones that would expose a value being stringified on the way through
        // — and which only work *because* the rewrite was forced.
        assert_eq!(eval("${{ !env.SOMETHING_TRUE }}").as_deref(), Ok("false"));
        assert_eq!(eval("${{ !env.SOMETHING_FALSE }}").as_deref(), Ok("false"));
        assert_eq!(
            eval("${{ env.SOMETHING_TRUE && true }}").as_deref(),
            Ok("true")
        );
        assert_eq!(
            eval("${{ env.SOMETHING_FALSE && true }}").as_deref(),
            Ok("true")
        );
        assert_eq!(
            eval("${{ env.SOMETHING_TRUE || false }}").as_deref(),
            Ok("true")
        );
        assert_eq!(
            eval("${{ env.SOMETHING_FALSE || false }}").as_deref(),
            Ok("false")
        );
        // A *single* unforced expression must not go through `format`, or the
        // boolean would arrive as the string "false" and `if:` would be truthy.
        // This is the whole reason `force_format` exists.
        //
        // A bare `x` is not resolvable in Actions — every context member needs
        // its prefix — so the round trip uses `env.x`. That the rewriter leaves
        // the expression body untouched is pinned by the measured table.
        let unwritten = rewrite_sub_expression("${{ env.SOMETHING_FALSE && true }}", false)
            .expect("a well-formed input");
        assert_eq!(
            unwritten, "${{ env.SOMETHING_FALSE && true }}",
            "one expression and nothing else stays unwritten"
        );
    }

    /// `escapeFormatString` doubles braces in two sequential passes, so a `}` is
    /// doubled exactly once no matter what the `{` pass did.
    #[test]
    fn braces_are_doubled_exactly_once_each() {
        assert_eq!(escape_format_string(""), "");
        assert_eq!(escape_format_string("plain"), "plain");
        assert_eq!(escape_format_string("{"), "{{");
        assert_eq!(escape_format_string("}"), "}}");
        assert_eq!(escape_format_string("{}"), "{{}}");
        // Five braces in, ten out: three `{` become six, two `}` become four.
        assert_eq!(escape_format_string("{{{}}}"), "{{{{{{}}}}}}");
    }

    /// A run of one job, with `job_body` spliced into the `test` job.
    ///
    /// Named for what it takes rather than what it produces: an earlier version
    /// of this helper was called `run_with_strategy` and took `""` to mean "no
    /// strategy", which made it read as "a strategy, empty" and produced a job
    /// with no `strategy:` at all. Two tests then contradicted each other and
    /// neither was obviously the liar.
    fn run_with_job_body(job_body: &str) -> Run {
        let source = format!(
            "name: test-workflow\njobs:\n  test:\n    name: test\n{}",
            job_body
                .lines()
                .map(|line| format!("    {line}\n"))
                .collect::<String>()
        );
        let doc = Rc::new(Document::parse(&source).expect("the fixture parses"));
        let workflow = Workflow::from_document("test.yml", &doc).expect("the fixture decodes");
        Run::new(workflow, doc, "test")
    }

    /// `strategy` reports the **resolved** values, so a workflow that writes
    /// `strategy.max-parallel == 4` on a job that never mentions it sees 4 and
    /// not an empty string. The resolution is a side effect of upstream's
    /// `GetMatrixes()`; see the note on [`EvaluationInputs`].
    #[test]
    fn the_strategy_context_holds_resolved_values() {
        // `strategy: {}` is present but says nothing, so what comes out is
        // entirely act's defaults.
        let run = run_with_job_body("strategy: {}\n");
        let strategy = strategy_context(Some(&run));
        assert_eq!(
            strategy.get("fail-fast"),
            Some(&Value::Bool(true)),
            "act's default"
        );
        assert_eq!(
            strategy.get("max-parallel"),
            Some(&Value::Int(4)),
            "act's default"
        );

        let run = run_with_job_body("strategy:\n  fail-fast: false\n  max-parallel: 2\n");
        let strategy = strategy_context(Some(&run));
        assert_eq!(strategy.get("fail-fast"), Some(&Value::Bool(false)));
        assert_eq!(strategy.get("max-parallel"), Some(&Value::Int(2)));
    }

    /// A job with no `strategy:` reports **nothing**, not `false` and `4`. A
    /// workflow may test for the context's existence, and upstream leaves it
    /// empty in that case. This is the case the helper's old name hid.
    #[test]
    fn no_strategy_means_no_context_at_all() {
        let run = run_with_job_body("");
        assert!(strategy_context(Some(&run)).is_empty());
        assert!(strategy_context(None).is_empty());
    }

    /// `needs.<job>.outputs` and `needs.<job>.result` come from the jobs the
    /// current one waits for.
    #[test]
    fn the_needs_context_reports_outputs_and_result() {
        let source = "name: w\njobs:\n  a:\n    name: a\n    needs: [b]\n  b:\n    name: b\n    outputs:\n      thing: value\n";
        let doc = Rc::new(Document::parse(source).expect("the fixture parses"));
        let mut workflow = Workflow::from_document("test.yml", &doc).expect("decodes");
        // `result` is set by the runner, not by the YAML.
        workflow.jobs.get_mut("b").expect("job b").result = "success".to_string();
        let run = Run::new(workflow, doc, "a");

        let needs = needs_context(Some(&run));
        let b = needs.get("b").and_then(Value::as_object).expect("need b");
        assert_eq!(b.get("result").and_then(Value::as_str), Some("success"));
        assert_eq!(
            b.get("outputs")
                .and_then(Value::as_object)
                .and_then(|o| o.get("thing"))
                .and_then(Value::as_str),
            Some("value")
        );
    }

    /// A need naming a job that does not exist still gets a key, with empty
    /// members. Upstream panics on the same input — see the deviation note on
    /// [`needs_context`]. This is the one place in the context tree where this
    /// port knowingly differs, so it is pinned rather than assumed.
    #[test]
    fn a_need_naming_a_missing_job_still_has_a_key() {
        let source = "name: w\njobs:\n  a:\n    name: a\n    needs: [nope]\n";
        let doc = Rc::new(Document::parse(source).expect("the fixture parses"));
        let workflow = Workflow::from_document("test.yml", &doc).expect("decodes");
        let run = Run::new(workflow, doc, "a");

        let needs = needs_context(Some(&run));
        let nope = needs
            .get("nope")
            .and_then(Value::as_object)
            .expect("the key");
        assert!(nope
            .get("outputs")
            .and_then(Value::as_object)
            .expect("outputs")
            .is_empty());
    }

    /// `conclusion` and `outcome` are both exposed, and they differ exactly
    /// where it matters: a `continue-on-error` step concluded `success` and
    /// ended as `failure`.
    #[test]
    fn the_steps_context_exposes_conclusion_and_outcome_separately() {
        let results = BTreeMap::from([
            (
                "idwithnothing".to_string(),
                StepResult {
                    outputs: BTreeMap::from([("o".to_string(), "v".to_string())]),
                    conclusion: StepStatus::Success,
                    outcome: StepStatus::Failure,
                },
            ),
            (
                "id-with-hyphens".to_string(),
                StepResult {
                    outputs: BTreeMap::from([("o-with-hyphens".to_string(), "v".to_string())]),
                    conclusion: StepStatus::Failure,
                    outcome: StepStatus::Failure,
                },
            ),
            ("id_with_underscores".to_string(), StepResult::default()),
        ]);
        let steps = steps_context(&results);

        let first = steps
            .get("idwithnothing")
            .and_then(Value::as_object)
            .expect("step one");
        assert_eq!(
            first.get("conclusion").and_then(Value::as_str),
            Some("success")
        );
        assert_eq!(
            first.get("outcome").and_then(Value::as_str),
            Some("failure")
        );
        assert_eq!(
            first
                .get("outputs")
                .and_then(Value::as_object)
                .and_then(|o| o.get("o"))
                .and_then(Value::as_str),
            Some("v")
        );

        // A hyphenated id is reachable, and a default result concludes
        // `success` because success is the zero value.
        let second = steps
            .get("id-with-hyphens")
            .and_then(Value::as_object)
            .expect("step two");
        assert_eq!(
            second.get("conclusion").and_then(Value::as_str),
            Some("failure")
        );
        let third = steps
            .get("id_with_underscores")
            .and_then(Value::as_object)
            .expect("step three");
        assert_eq!(
            third.get("conclusion").and_then(Value::as_str),
            Some("success")
        );
    }

    /// The contexts are what the interpreter actually reads, so a workflow
    /// condition resolves against them. This is the round trip: build the
    /// context, then ask a question of it as a workflow would.
    #[test]
    fn a_workflow_condition_resolves_against_the_assembled_context() {
        let run = run_with_job_body("strategy: {}\n");
        let results = BTreeMap::from([(
            "idwithnothing".to_string(),
            StepResult {
                outputs: BTreeMap::from([(
                    "foowithnothing".to_string(),
                    "barwithnothing".to_string(),
                )]),
                conclusion: StepStatus::Success,
                outcome: StepStatus::Failure,
            },
        )]);
        let env = BTreeMap::from([("key".to_string(), "value".to_string())]);
        let secrets =
            BTreeMap::from([("CASE_INSENSITIVE_SECRET".to_string(), "value".to_string())]);
        let vars = BTreeMap::from([("CASE_INSENSITIVE_VAR".to_string(), "value".to_string())]);
        let matrix: BTreeMap<String, Value> = serde_json::json!({"os": "Linux", "foo": "bar"})
            .as_object()
            .expect("an object")
            .iter()
            .map(|(key, value)| (key.clone(), crate::expr::from_json_value(value)))
            .collect();

        let environment = EvaluationEnvironment {
            env: env_context(&env),
            job: Some(Value::object([(
                "status",
                Value::String("success".to_string()),
            )])),
            steps: steps_context(&results),
            secrets: secrets_context(&secrets),
            vars: env_context(&vars),
            strategy: strategy_context(Some(&run)),
            matrix,
            needs: needs_context(Some(&run)),
            ..EvaluationEnvironment::default()
        };
        let status = DefaultStatus;
        // Upstream's own tables compare the *value*, not a rendering of it
        // (`assert.Equal(t, table.out, out)` against a Go `interface{}`), and
        // the difference is load-bearing: `strategy.fail-fast` is a `bool` and
        // `strategy.max-parallel` an `int`, while `steps.<id>.conclusion` is a
        // `string`. A helper that only accepted strings would have silently
        // dropped the two non-string cases instead of failing on them.
        let evaluate = |source: &str| {
            Interpreter::new(&environment, &status, crate::expr::EvaluationContext::Job)
                .evaluate(source, DefaultStatusCheck::None)
        };
        let text = |value: &str| Value::String(value.to_string());

        // `TestEvaluateRunContext`, the cases that read an assembled context
        // rather than a builtin.
        assert_eq!(evaluate("matrix.os"), Ok(text("Linux")));
        assert_eq!(evaluate("matrix.foo"), Ok(text("bar")));
        assert_eq!(evaluate("env.key"), Ok(text("value")));
        assert_eq!(evaluate("job.status"), Ok(text("success")));
        assert_eq!(
            evaluate("secrets.CASE_INSENSITIVE_SECRET"),
            Ok(text("value"))
        );
        assert_eq!(
            evaluate("secrets.case_insensitive_secret"),
            Ok(text("value")),
            "context members are case-insensitive"
        );
        assert_eq!(evaluate("vars.CASE_INSENSITIVE_VAR"), Ok(text("value")));
        assert_eq!(evaluate("vars.case_insensitive_var"), Ok(text("value")));

        // `TestEvaluateStep`. Upstream reaches these through
        // `NewStepExpressionEvaluator`; this goes through the ordinary
        // evaluator, which is the same for these expressions because
        // `Config.Context` only chooses the implementation of `success()` and
        // `failure()` (`pkg/exprparser/interpreter.go:648-663`) and none of
        // them calls either. The contexts themselves come from the same
        // builders on both paths.
        assert_eq!(
            evaluate("steps.idwithnothing.conclusion"),
            Ok(text("success"))
        );
        assert_eq!(evaluate("steps.idwithnothing.outcome"), Ok(text("failure")));
        assert_eq!(
            evaluate("steps.idwithnothing.outputs.foowithnothing"),
            Ok(text("barwithnothing"))
        );

        // `strategy`: `fail-fast` is asserted as a `bool` upstream, in
        // `pkg/exprparser/interpreter_test.go`. `max-parallel` is asserted
        // nowhere, so its type is this port's reading of the context builder
        // — an `int`, the same one `runner.go:181` clamps the parallelism
        // with. Asserting it as the string "4" would have been a claim with
        // no upstream behind it.
        assert_eq!(evaluate("strategy.fail-fast"), Ok(Value::Bool(true)));
        assert_eq!(
            evaluate("strategy.max-parallel"),
            Ok(Value::Int(4)),
            "act's resolved default, and an int rather than a string"
        );
    }

    /// The `TestInterpolate` environment: five env vars whose values are the
    /// words `true` and `false`, plus one secret and one var, both spelled in
    /// upper case so the case-insensitive lookup is exercised.
    ///
    /// `SOMETHING_TRUE` / `SOMETHING_FALSE` are *strings*. The `!` and `&&`
    /// cases below therefore test the coercion, not a boolean context member —
    /// which is the shape real workflows have, since `env:` is always text.
    fn interpolate_environment() -> EvaluationEnvironment {
        let pairs = [
            ("KEYWITHNOTHING", "valuewithnothing"),
            ("KEY-WITH-HYPHENS", "value-with-hyphens"),
            ("KEY_WITH_UNDERSCORES", "value_with_underscores"),
            ("SOMETHING_TRUE", "true"),
            ("SOMETHING_FALSE", "false"),
        ];
        EvaluationEnvironment {
            env: pairs
                .into_iter()
                .map(|(key, value)| (key.to_string(), Value::String(value.to_string())))
                .collect(),
            secrets: [("CASE_INSENSITIVE_SECRET", "value")]
                .into_iter()
                .map(|(key, value)| (key.to_string(), Value::String(value.to_string())))
                .collect(),
            vars: [("CASE_INSENSITIVE_VAR", "value")]
                .into_iter()
                .map(|(key, value)| (key.to_string(), Value::String(value.to_string())))
                .collect(),
            ..EvaluationEnvironment::default()
        }
    }

    /// Upstream's `TestInterpolate`, all 31 cases, transcribed.
    ///
    /// The context is [`EvaluationContext::Other`] with an empty name because
    /// that is what upstream builds: `NewExpressionEvaluatorWithEnv` leaves
    /// `Config.Context` unset, so it is `""` and only `Job`/`Step` are accepted
    /// by the status functions. None of these cases calls one, so it does not
    /// change an answer — but using `Job` here would have been a convenient
    /// lie that the next case with `success()` in it would expose.
    #[test]
    fn interpolate_matches_every_upstream_case() {
        const CASES: &[(&str, &str)] = &[
            (" text ", " text "),
            (" $text ", " $text "),
            (" ${text} ", " ${text} "),
            (
                " ${{          1                         }} to ${{2}} ",
                " 1 to 2 ",
            ),
            (" ${{  (true || false)  }} to ${{2}} ", " true to 2 "),
            (" ${{  (false   ||  '}}'  )    }} to ${{2}} ", " }} to 2 "),
            (" ${{ env.KEYWITHNOTHING }} ", " valuewithnothing "),
            (" ${{ env.KEY-WITH-HYPHENS }} ", " value-with-hyphens "),
            (
                " ${{ env.KEY_WITH_UNDERSCORES }} ",
                " value_with_underscores ",
            ),
            ("${{ secrets.CASE_INSENSITIVE_SECRET }}", "value"),
            ("${{ secrets.case_insensitive_secret }}", "value"),
            ("${{ vars.CASE_INSENSITIVE_VAR }}", "value"),
            ("${{ vars.case_insensitive_var }}", "value"),
            ("${{ env.UNKNOWN }}", ""),
            ("${{ env.SOMETHING_TRUE }}", "true"),
            ("${{ env.SOMETHING_FALSE }}", "false"),
            ("${{ !env.SOMETHING_TRUE }}", "false"),
            ("${{ !env.SOMETHING_FALSE }}", "false"),
            ("${{ !env.SOMETHING_TRUE && true }}", "false"),
            ("${{ !env.SOMETHING_FALSE && true }}", "false"),
            ("${{ env.SOMETHING_TRUE && true }}", "true"),
            ("${{ env.SOMETHING_FALSE && true }}", "true"),
            ("${{ !env.SOMETHING_TRUE || true }}", "true"),
            ("${{ !env.SOMETHING_FALSE || true }}", "true"),
            ("${{ !env.SOMETHING_TRUE && false }}", "false"),
            ("${{ !env.SOMETHING_FALSE && false }}", "false"),
            ("${{ !env.SOMETHING_TRUE || false }}", "false"),
            ("${{ !env.SOMETHING_FALSE || false }}", "false"),
            ("${{ env.SOMETHING_TRUE || false }}", "true"),
            ("${{ env.SOMETHING_FALSE || false }}", "false"),
            // A value that is *not* a string still comes out as text, because
            // the forced `format()` stringifies it. Without the force this
            // would be a `bool` and upstream would panic on the type assertion.
            (
                "${{ env.SOMETHING_FALSE }} && ${{ env.SOMETHING_TRUE }}",
                "false && true",
            ),
            ("${{ fromJSON('{}') < 2 }}", "false"),
        ];

        let environment = interpolate_environment();
        let status = DefaultStatus;
        let context = crate::expr::EvaluationContext::Other(String::new());
        for (input, want) in CASES {
            assert_eq!(
                interpolate(&environment, &status, context.clone(), input).as_deref(),
                Ok(*want),
                "input: {input:?}"
            );
        }
    }

    /// What `force_format` is for, stated as a test rather than a comment.
    ///
    /// `interpolate` and `eval_bool` take the same input and must disagree.
    /// If they ever agree, one of them is rewriting with the wrong flag: the
    /// string path is handing a caller `"false"` for a false value, or the
    /// condition path is seeing the non-empty string `"false"` and calling it
    /// true. Both are the same bug wearing different clothes.
    ///
    /// # The variable name is a trap
    ///
    /// `SOMETHING_FALSE` holds the **string** `"false"`, and a non-empty string
    /// is truthy in Actions. So `${{ env.SOMETHING_FALSE && true }}` is `true`
    /// — upstream's own table says so, and I filed it under the false group
    /// here first because the name reads the other way. Only `!` turns it
    /// around, which is why most of the pairs below are `!`-expressions.
    #[test]
    fn a_condition_and_an_interpolation_read_the_same_expression_differently() {
        let environment = interpolate_environment();
        let status = DefaultStatus;
        let context = crate::expr::EvaluationContext::Job;
        for expression in [
            "${{ !env.SOMETHING_TRUE }}",
            "${{ !env.SOMETHING_TRUE && true }}",
            "${{ !env.SOMETHING_TRUE || false }}",
        ] {
            let as_bool = eval_bool(
                &environment,
                &status,
                context.clone(),
                expression,
                DefaultStatusCheck::None,
            )
            .expect("evaluates");
            let as_text =
                interpolate(&environment, &status, context.clone(), expression).expect("evaluates");
            assert_eq!(
                (as_bool, as_text.as_str()),
                (false, "false"),
                "expression: {expression}"
            );
        }
        // And the mirror image: a condition that *is* true stays true in both.
        //
        // Note what is *not* in either list: a `!` that comes out true. Both
        // variables are non-empty strings and therefore truthy, so `!` on
        // either is `false`. Upstream's table agrees — every `!` row there
        // evaluates to `"false"`. A `!` that looked like it flipped the value
        // would mean the coercion had started working on the word rather than
        // on the string.
        for expression in [
            "${{ env.SOMETHING_FALSE && true }}",
            "${{ env.SOMETHING_TRUE || false }}",
        ] {
            let as_bool = eval_bool(
                &environment,
                &status,
                context.clone(),
                expression,
                DefaultStatusCheck::None,
            )
            .expect("evaluates");
            let as_text =
                interpolate(&environment, &status, context.clone(), expression).expect("evaluates");
            assert_eq!(
                (as_bool, as_text.as_str()),
                (true, "true"),
                "expression: {expression}"
            );
        }

        // The one case where the two genuinely disagree, and the reason the
        // flags differ at all. `env.SOMETHING_FALSE` is the truthy string
        // `"false"`, so the disjunction short-circuits to `true` and the
        // condition passes — while the forced `format()` renders the
        // short-circuited `false` branch as the text `"false"`. A workflow
        // writing this into a `run:` body gets the string `false`; the same
        // expression in an `if:` runs the step. One expression, two readings,
        // and upstream's table asserts both.
        let expression = "${{ env.SOMETHING_FALSE || false }}";
        assert_eq!(
            eval_bool(
                &environment,
                &status,
                context.clone(),
                expression,
                DefaultStatusCheck::None,
            ),
            Ok(true)
        );
        assert_eq!(
            interpolate(&environment, &status, context, expression),
            Ok("false".to_string())
        );
    }

    /// Upstream's `NotAString` panic becomes an error, and the *upstream*
    /// outcome is still reachable through [`InterpolateError::upstream_value`].
    ///
    /// Reaching it needs a rewriter bug, so it is provoked by handing
    /// `interpolate` an expression the forced rewrite cannot turn into a
    /// string — which is exactly the condition upstream would panic under.
    #[test]
    fn a_rewrite_failure_is_reported_rather_than_panicking() {
        let environment = interpolate_environment();
        let status = DefaultStatus;
        let context = crate::expr::EvaluationContext::Job;
        // `${{ 'unclosed` has an opening delimiter and a quote that never
        // closes, so `rewrite_sub_expression` gives up.
        let error = interpolate(&environment, &status, context, "${{ 'unclosed }}")
            .expect_err("the rewriter cannot close this string");
        assert!(
            matches!(error, InterpolateError::Rewrite(_)),
            "got {error:?}"
        );
        assert_eq!(
            error.upstream_value(),
            "",
            "what upstream's caller would see"
        );
    }

    /// The Go probe's `RunContext` environment, member for member.
    ///
    /// Not [`interpolate_environment`], which is the `TestInterpolate` one and
    /// has no `KEY`. Aliasing the two was the first version of this and it
    /// failed on `scalar-interp` with a `!!null` — the walk was right and the
    /// fixture was answering about a variable that was not there.
    fn node_environment() -> EvaluationEnvironment {
        let pairs = [
            ("SOMETHING_TRUE", "true"),
            ("SOMETHING_FALSE", "false"),
            ("SOME_TEXT", "text"),
            ("KEY", "value"),
        ];
        EvaluationEnvironment {
            env: pairs
                .into_iter()
                .map(|(key, value)| (key.to_string(), Value::String(value.to_string())))
                .collect(),
            ..EvaluationEnvironment::default()
        }
    }

    /// The Go probe's `dump`, reproduced.
    ///
    /// It exists so the expectations below are *the strings the probe printed*
    /// rather than a Rust-shaped restatement of them. A rewrite of the same
    /// tree that only looks right is exactly what this test has to catch, and
    /// the cheapest way to fail that is to compare against a dump the upstream
    /// binary produced.
    ///
    /// One thing has to be reconciled to make the strings comparable: a Go
    /// `yaml.Node` always carries a **resolved** tag, so `a: 1` reports
    /// `!!int`, while this arena keeps the tag implicit (`None`) and resolves
    /// on decode. Without [`resolved_tag`] every expectation would have to be
    /// restated with `None` where the probe said `!!int`, and the restatement
    /// would no longer be the probe's output. The resolution rule for a plain
    /// scalar is small and fixed, so it lives in the test helper rather than
    /// in `yaml_node`, which has no reason to grow it.
    fn dump_node(document: &Document, id: crate::yaml_node::NodeId, depth: usize) -> String {
        use crate::yaml_node::NodeKind;
        let Some(node) = document.node(id) else {
            return "<nil>".to_string();
        };
        let pad = "  ".repeat(depth);
        match node.kind {
            NodeKind::Scalar => format!(
                "{pad}Scalar(tag={:?},value={:?})",
                resolved_tag(node),
                node.value
            ),
            NodeKind::Sequence => {
                let mut out = format!("{pad}Seq(tag={:?})\n", resolved_tag(node));
                for child in &node.content {
                    out.push_str(&dump_node(document, *child, depth + 1));
                    out.push('\n');
                }
                out
            }
            NodeKind::Mapping => {
                let mut out = format!("{pad}Map(tag={:?})\n", resolved_tag(node));
                for (key, value) in document.map_entries(id) {
                    out.push_str(&format!(
                        "{pad}  key: {}\n",
                        dump_node(document, key, depth + 2)
                    ));
                    out.push_str(&format!(
                        "{pad}  val: {}\n",
                        dump_node(document, value, depth + 2)
                    ));
                }
                out
            }
            other => format!("{pad}Kind({other:?})"),
        }
    }

    /// yaml.v3's tag for a node that carries none, so a dump can be compared
    /// against one the Go probe printed.
    fn resolved_tag(node: &crate::yaml_node::Node) -> String {
        if let Some(tag) = &node.tag {
            return tag.clone();
        }
        match node.kind {
            crate::yaml_node::NodeKind::Mapping => "!!map".to_string(),
            crate::yaml_node::NodeKind::Sequence => "!!seq".to_string(),
            _ => match node.value.as_str() {
                "" | "null" | "~" | "Null" | "NULL" => "!!null",
                "true" | "True" | "TRUE" | "false" | "False" | "FALSE" => "!!bool",
                other if other.parse::<i64>().is_ok() => "!!int",
                other if other.parse::<f64>().is_ok() => "!!float",
                _ => "!!str",
            }
            .to_string(),
        }
    }

    /// Every case from the Go probe, with upstream's own output as the
    /// expectation.
    ///
    /// Upstream has **no test** for `EvaluateYamlNode` — `expression_test.go`
    /// covers only the evaluator and the rewriter, and `insert` appears in no
    /// test file at all. So there is no upstream table to transcribe, and the
    /// fixtures below were run through the real `EvaluateYamlNode` on v0.2.89
    /// and the strings are what it wrote. That is a weaker authority than a
    /// test upstream maintains, and it is the reason this one is worth reading
    /// before changing the walk.
    #[test]
    fn evaluate_yaml_node_matches_what_upstream_wrote() {
        use crate::yaml_node::NodeId;
        let environment = node_environment();
        let status = DefaultStatus;
        let context = crate::expr::EvaluationContext::Other(String::new());

        let cases: &[(&str, &str, &str)] = &[
            (
                "scalar-literal",
                "plain\n",
                "  Scalar(tag=\"!!str\",value=\"plain\")",
            ),
            // `}}` with no opening delimiter is ordinary text, not a broken
            // expression. The guard is on *both* delimiters for this reason.
            (
                "scalar-braces-only",
                "a }} b\n",
                "  Scalar(tag=\"!!str\",value=\"a }} b\")",
            ),
            (
                "scalar-interp",
                "${{ env.KEY }}\n",
                "  Scalar(tag=\"!!str\",value=\"value\")",
            ),
            // The rewrite is unforced here, so this stays a boolean and comes
            // back **tagged**. Forcing the format would have made it the string
            // "false" — truthy, and a condition that never fails.
            (
                "scalar-bool",
                "${{ !env.SOMETHING_TRUE }}\n",
                "  Scalar(tag=\"!!bool\",value=\"false\")",
            ),
            (
                "scalar-int",
                "${{ 1 }}\n",
                "  Scalar(tag=\"!!int\",value=\"1\")",
            ),
            (
                "mapping-nothing-changes",
                "a: 1\nb: 2\n",
                "  Map(tag=\"!!map\")\n    key:       Scalar(tag=\"!!str\",value=\"a\")\n    val:       Scalar(tag=\"!!int\",value=\"1\")\n    key:       Scalar(tag=\"!!str\",value=\"b\")\n    val:       Scalar(tag=\"!!int\",value=\"2\")\n",
            ),
            (
                "mapping-first-changes",
                "a: ${{ env.KEY }}\nb: 2\nc: 3\n",
                "  Map(tag=\"!!map\")\n    key:       Scalar(tag=\"!!str\",value=\"a\")\n    val:       Scalar(tag=\"!!str\",value=\"value\")\n    key:       Scalar(tag=\"!!str\",value=\"b\")\n    val:       Scalar(tag=\"!!int\",value=\"2\")\n    key:       Scalar(tag=\"!!str\",value=\"c\")\n    val:       Scalar(tag=\"!!int\",value=\"3\")\n",
            ),
            (
                "mapping-last-changes",
                "a: 1\nb: 2\nc: ${{ env.KEY }}\n",
                "  Map(tag=\"!!map\")\n    key:       Scalar(tag=\"!!str\",value=\"a\")\n    val:       Scalar(tag=\"!!int\",value=\"1\")\n    key:       Scalar(tag=\"!!str\",value=\"b\")\n    val:       Scalar(tag=\"!!int\",value=\"2\")\n    key:       Scalar(tag=\"!!str\",value=\"c\")\n    val:       Scalar(tag=\"!!str\",value=\"value\")\n",
            ),
            // A key is evaluated too, which is how a workflow renames itself.
            (
                "mapping-key-changes",
                "a: 1\n${{ env.KEY }}: 2\n",
                "  Map(tag=\"!!map\")\n    key:       Scalar(tag=\"!!str\",value=\"a\")\n    val:       Scalar(tag=\"!!int\",value=\"1\")\n    key:       Scalar(tag=\"!!str\",value=\"value\")\n    val:       Scalar(tag=\"!!int\",value=\"2\")\n",
            ),
            // The insert directive: the key disappears and the value's entries
            // take its place, in order, between the surrounding entries.
            (
                "insert",
                "before: 1\n${{ insert }}:\n  x: 9\n  y: 10\nafter: 2\n",
                "  Map(tag=\"!!map\")\n    key:       Scalar(tag=\"!!str\",value=\"before\")\n    val:       Scalar(tag=\"!!int\",value=\"1\")\n    key:       Scalar(tag=\"!!str\",value=\"x\")\n    val:       Scalar(tag=\"!!int\",value=\"9\")\n    key:       Scalar(tag=\"!!str\",value=\"y\")\n    val:       Scalar(tag=\"!!int\",value=\"10\")\n    key:       Scalar(tag=\"!!str\",value=\"after\")\n    val:       Scalar(tag=\"!!int\",value=\"2\")\n",
            ),
            // No spaces around the directive body, still an insert.
            (
                "insert-tight",
                "before: 1\n${{insert}}:\n  x: 9\nafter: 2\n",
                "  Map(tag=\"!!map\")\n    key:       Scalar(tag=\"!!str\",value=\"before\")\n    val:       Scalar(tag=\"!!int\",value=\"1\")\n    key:       Scalar(tag=\"!!str\",value=\"x\")\n    val:       Scalar(tag=\"!!int\",value=\"9\")\n    key:       Scalar(tag=\"!!str\",value=\"after\")\n    val:       Scalar(tag=\"!!int\",value=\"2\")\n",
            ),
            (
                "sequence-nothing",
                "- 1\n- 2\n",
                "  Seq(tag=\"!!seq\")\n    Scalar(tag=\"!!int\",value=\"1\")\n    Scalar(tag=\"!!int\",value=\"2\")\n",
            ),
            (
                "sequence-one",
                "- 1\n- ${{ env.KEY }}\n- 3\n",
                "  Seq(tag=\"!!seq\")\n    Scalar(tag=\"!!int\",value=\"1\")\n    Scalar(tag=\"!!str\",value=\"value\")\n    Scalar(tag=\"!!int\",value=\"3\")\n",
            ),
            // An item that *becomes* a sequence through evaluation is spliced:
            // four entries out of three, not one list-shaped entry.
            (
                "sequence-splice",
                "- 1\n- ${{ fromJSON('[7,8]') }}\n- 3\n",
                "  Seq(tag=\"!!seq\")\n    Scalar(tag=\"!!int\",value=\"1\")\n    Scalar(tag=\"!!int\",value=\"7\")\n    Scalar(tag=\"!!int\",value=\"8\")\n    Scalar(tag=\"!!int\",value=\"3\")\n",
            ),
            // An item that was **already** a sequence stays one, even after
            // evaluation. That is the `wasseq` check, and the two cases below
            // are the only thing pinning it.
            (
                "sequence-already-seq",
                "- 1\n- - 7\n  - 8\n- 3\n",
                "  Seq(tag=\"!!seq\")\n    Scalar(tag=\"!!int\",value=\"1\")\n    Seq(tag=\"!!seq\")\n      Scalar(tag=\"!!int\",value=\"7\")\n      Scalar(tag=\"!!int\",value=\"8\")\n\n    Scalar(tag=\"!!int\",value=\"3\")\n",
            ),
            (
                "sequence-already-seq-changed",
                "- 1\n- - ${{ env.KEY }}\n  - 8\n- 3\n",
                "  Seq(tag=\"!!seq\")\n    Scalar(tag=\"!!int\",value=\"1\")\n    Seq(tag=\"!!seq\")\n      Scalar(tag=\"!!str\",value=\"value\")\n      Scalar(tag=\"!!int\",value=\"8\")\n\n    Scalar(tag=\"!!int\",value=\"3\")\n",
            ),
            (
                "nested",
                "a:\n  b: ${{ env.KEY }}\nlist:\n  - ${{ env.KEY }}\n",
                "  Map(tag=\"!!map\")\n    key:       Scalar(tag=\"!!str\",value=\"a\")\n    val:       Map(tag=\"!!map\")\n        key:           Scalar(tag=\"!!str\",value=\"b\")\n        val:           Scalar(tag=\"!!str\",value=\"value\")\n\n    key:       Scalar(tag=\"!!str\",value=\"list\")\n    val:       Seq(tag=\"!!seq\")\n        Scalar(tag=\"!!str\",value=\"value\")\n\n",
            ),
        ];

        for (label, source, want) in cases {
            let mut document = Document::parse(source).expect("the fixture parses");
            let root: NodeId = document.root().expect("a root");
            evaluate_yaml_node(&mut document, &environment, &status, context.clone(), root)
                .expect("upstream succeeded on every fixture here");
            assert_eq!(dump_node(&document, root, 1), *want, "case: {label}");
        }
    }

    /// A node that changes nothing must come back **untouched**, not
    /// rebuilt-identical.
    ///
    /// Upstream's `EvaluateYamlNode` only decodes the replacement back into the
    /// node when the walk produced one, and the walk produces nothing when no
    /// entry changed. A port that always rebuilds would produce the same
    /// `dump` for the fixtures above and still be wrong here — it would
    /// re-derive every `!!int` tag on the way through, and a document that
    /// relied on a value's original spelling would quietly lose it.
    #[test]
    fn a_tree_with_nothing_to_evaluate_is_left_alone() {
        let environment = node_environment();
        let status = DefaultStatus;
        let context = crate::expr::EvaluationContext::Other(String::new());
        let mut document = Document::parse("a: 1\nb:\n  - 2\n  - 3\n").expect("parses");
        let root = document.root().expect("a root");
        let before = dump_node(&document, root, 0);
        let changed = evaluate_yaml_node(&mut document, &environment, &status, context, root)
            .expect("no expression, no error");
        assert!(!changed, "nothing changed, so nothing should be reported");
        assert_eq!(dump_node(&document, root, 0), before);
    }

    /// `insert` onto something that is not a map is an error, and — measured —
    /// the tree is left **exactly** as it was.
    ///
    /// Upstream returns before `ret.Decode(node)`, so a failed walk writes
    /// nothing. That is the easy half to get wrong: building the replacement
    /// first and installing it at the end is correct, installing it as you go
    /// and unwinding on error is not.
    #[test]
    fn a_failed_insert_leaves_the_document_untouched() {
        let environment = node_environment();
        let status = DefaultStatus;
        let context = crate::expr::EvaluationContext::Other(String::new());
        let mut document = Document::parse("before: 1\n${{ insert }}: plain\n").expect("parses");
        let root = document.root().expect("a root");
        let before = dump_node(&document, root, 0);

        let error = evaluate_yaml_node(&mut document, &environment, &status, context, root)
            .expect_err("a scalar cannot be inserted into a mapping");
        assert!(
            error.message.starts_with("failed to insert node"),
            "got: {error}"
        );
        assert!(
            error.message.contains("expected MappingNode"),
            "the tail upstream keeps, got: {error}"
        );
        assert_eq!(dump_node(&document, root, 0), before, "nothing was written");
    }
}
