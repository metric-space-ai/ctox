//! Port of act's `pkg/runner/step.go`.
//!
//! The shared contract every step type implements, plus the four helpers the
//! runner itself calls: the environment merge, the two conditions, and the
//! symlink guard.
//!
//! | act | here |
//! |---|---|
//! | `step` interface | [`Step`] |
//! | `stepStage` | [`StepStage`] |
//! | `mergeEnv` | [`merge_env`] (a free function; it needs a container-free seam) and [`merge_github_env`] |
//! | `setupEnv` | [`setup_env`] |
//! | `isStepEnabled`, `isContinueOnError` | [`is_step_enabled`], [`is_continue_on_error`] |
//! | `mergeIntoMap*` | [`merge_into_map`] and its two halves |
//! | `symlinkJoin` | [`symlink_join`] |
//!
//! # What is deliberately not here
//!
//! `runStepExecutor`, `evaluateStepTimeout`, `monitorJobCancellation` and the
//! two `processRunner*Command` functions exec in the job container, read an
//! archive out of it, and cancel it. The container lifecycle has landed, so
//! these are the next ones — named here so the gap is a list rather than an
//! absence. The *decisions* [`runStepExecutor`](super::step_executor) makes are
//! already ported and tested; what is missing is the part that calls them.
//!
//! # `symlinkJoin` uses Go's `path`, not `filepath`
//!
//! The guard compares slash paths — container paths, which are slash paths on
//! every host — so it reaches for [`crate::gopath`]. Reaching for
//! `filepath`/`std::path` would agree on macOS and be wrong on Windows. That
//! trap is spelled out in `gopath`'s module docs, and the reason
//! [`symlink_join`] has its own table test.

use std::collections::BTreeMap;

/// Which phase of a step is running.
///
/// The distinction is not cosmetic: a `post:` step gets
/// [`crate::expr::DefaultStatusCheck::Always`] as its implicit condition where
/// `pre:` and `main:` get `Success`. A `post:` step therefore runs even after
/// the step it belongs to has failed, which is the only way `if: always()`
/// cleanup can happen at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StepStage {
    /// `pre:`
    Pre,
    /// `main:`
    #[default]
    Main,
    /// `post:`
    Post,
}

impl std::fmt::Display for StepStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // "Unknown" for a value outside the three is upstream's, and it is
        // reachable only through a deserialised value this port cannot build.
        f.write_str(match self {
            Self::Pre => "Pre",
            Self::Main => "Main",
            Self::Post => "Post",
        })
    }
}

/// `maxSymlinkDepth`: how many symlinks act follows while reading an action's
/// files out of the container.
///
/// There is no upstream function of this name — it is a **loop bound**, used
/// twice, in the `action.yaml` reader of `step_action_local.go` and of
/// `step_action_remote.go`: each pass reads one archive, and an entry that is
/// itself a symlink is followed by the next pass. Exceeding the bound gives
/// `max depth 10 of symlinks exceeded while reading <path>`.
///
/// So the constant has no user in this module. It is ported with its
/// documentation because the two step types that need it are the next ones, and
/// a named constant that appears with no reader is less useful than one that
/// appears with its rule attached.
pub const MAX_SYMLINK_DEPTH: usize = 10;

/// A step being run.
///
/// Upstream's `step` interface has three executors and four accessors. The
/// executors return [`crate::common::Executor`] and are declared here so the
/// shape is fixed, but they are not yet implemented — they need the container
/// lifecycle.
pub trait Step {
    /// The step model, as it was written in the workflow.
    ///
    /// Named in full rather than imported: `crate::model::Step` is the *data*
    /// and this trait is the *behaviour*, and upstream has the same collision
    /// (`model.Step` and the unexported `step` interface). Upstream dodges it by
    /// not exporting the interface; here the trait is public because the step
    /// types implement it, so the data type stays spelled out.
    fn step_model(&self) -> &crate::model::Step;
    /// This step's own environment, which `setupEnv` has already filled in.
    fn env(&self) -> &BTreeMap<String, String>;
    /// The condition for `stage`, which is the step's own `if:` for `main` and
    /// the job's for the other two.
    fn if_expression(&self, stage: StepStage) -> String;
    /// The run context this step belongs to.
    fn run_context(&self) -> &super::run_context::RunContext;
}

// ---------------------------------------------------------------------------
// mergeIntoMap
// ---------------------------------------------------------------------------

/// `mergeIntoMapCaseSensitive`: later maps win, keys compared exactly.
///
/// A plain left-to-right overwrite. Repeated for both halves because upstream
/// has one function and the case-insensitive one needs the same iteration.
pub fn merge_into_map_case_sensitive(
    target: &mut BTreeMap<String, String>,
    maps: &[BTreeMap<String, String>],
) {
    for map in maps {
        for (key, value) in map {
            target.insert(key.clone(), value.clone());
        }
    }
}

/// `mergeIntoMapCaseInsensitive`: later maps win, keys compared folded, and the
/// **first** spelling of a name is the one kept.
///
/// Two measured examples that pin the rule:
///
/// | target | incoming | result |
/// |---|---|---|
/// | `{"KEY": "1"}` | `{"key": "2"}` | `{"KEY": "2"}` — the existing spelling wins |
/// | `{}` then `{"A":"1"}, {"a":"2"}, {"A":"3"}` | | `{"A":"3"}` — the first spelling is kept |
///
/// # Deliberate deviation: upstream is non-deterministic here
///
/// `foldKeys` is seeded by ranging over a Go `map`, whose iteration order is
/// randomised. So a target that *already* holds two names differing only in
/// case resolves differently from run to run. Measured on v0.2.89 with
/// `{"A":"1","a":"9"}` and `{"a":"2"}`: `{"A":"2","a":"9"}` on one run, and
/// `{"A":"1","a":"2"}` on another. Both are "correct" upstream, because there is
/// no order to be correct about.
///
/// A `BTreeMap` has one order, so this port is deterministic: the
/// lexicographically **first** spelling wins, and that is the rule on both
/// paths — the target's own keys and the incoming ones. Upstream applies two
/// different rules (last-wins when seeding, first-wins for incoming keys) and
/// resolves neither stably, so collapsing them into one is the smallest honest
/// choice. It is a real behavioural difference and a *reduction* in variance, not
/// a fix: a workflow depending on the coin flip cannot depend on it here.
/// Upstream's own `TestMergeIntoMap` never puts two case variants in the target,
/// so no upstream test covers the case.
pub fn merge_into_map_case_insensitive(
    target: &mut BTreeMap<String, String>,
    maps: &[BTreeMap<String, String>],
) {
    // Seed with the target's own keys. `or_insert` rather than `insert`, so the
    // rule is the same on both paths: **the first spelling encountered names the
    // variable.** Upstream's seeding loop overwrites, so its own rule is
    // last-wins for the target and first-wins for incoming keys; with a Go map
    // on top, neither was stable. A `BTreeMap` iterates sorted, so
    // "first encountered" here means the lexicographically smallest.
    let mut fold_keys: BTreeMap<String, String> = BTreeMap::new();
    for key in target.keys() {
        fold_keys.entry(fold(key)).or_insert_with(|| key.clone());
    }
    for map in maps {
        for (key, value) in map {
            let folded = fold(key);
            let name = match fold_keys.get(&folded) {
                Some(existing) => existing.clone(),
                None => {
                    fold_keys.insert(folded, key.clone());
                    key.clone()
                }
            };
            target.insert(name, value.clone());
        }
    }
}

/// `strings.ToLower`, which is Unicode-aware in both languages.
///
/// Measured: `Ä` and `ä` fold together, so a variable written in either case is
/// the same variable. Rust's `to_lowercase` agrees; `to_ascii_lowercase` would
/// not, and is not used.
fn fold(value: &str) -> String {
    value.to_lowercase()
}

/// `mergeIntoMap`: the case-sensitivity is the container's, not the caller's.
///
/// Upstream reads it off `rc.JobContainer`, and a nil container takes the
/// case-sensitive branch. So does a `None` here — the tests that never build a
/// container are the case-sensitive path.
pub fn merge_into_map(
    rc: &super::run_context::RunContext,
    target: &mut BTreeMap<String, String>,
    maps: &[BTreeMap<String, String>],
) {
    let insensitive = rc
        .job_container
        .as_ref()
        .is_some_and(|container| container.environment_case_insensitive);
    if insensitive {
        merge_into_map_case_insensitive(target, maps);
    } else {
        merge_into_map_case_sensitive(target, maps);
    }
}

// ---------------------------------------------------------------------------
// symlinkJoin
// ---------------------------------------------------------------------------

/// `symlinkJoin`: the path a symlink may point at, or a refusal.
///
/// `filename` is the link, `sym` its raw target, `parent` the directory the
/// action is confined to. The result is `path.Join(path.Dir(filename), sym)`
/// — cleaned, so a target that climbs out and back in again is allowed, which
/// is the point of using the *slash* package.
///
/// # The two ways out
///
/// * `prefix == "./"`, i.e. `parent` cleans to `.`, disables the check
///   entirely. That is the `--no-container` case, and it is upstream's own
///   escape hatch rather than something this port adds.
/// * Everything else must leave the result starting with `Clean(parent) + "/"`.
///
/// # What is and is not a traversal
///
/// Measured on v0.2.89, and the second column is the one that is easy to get
/// wrong:
///
/// | link | target | parent | result |
/// |---|---|---|---|
/// | `/a/b/link` | `target` | `/a/b` | `/a/b/target` |
/// | `/a/b/link` | `../outside` | `/a/b` | refused — `/a/outside` |
/// | `/a/b/link` | `../../etc/passwd` | `/a/b` | refused — `/etc/passwd` |
/// | `/a/b/link` | `../../../a/b/target` | `/a/b` | **allowed** — it lands back inside |
/// | `/a/b/c/link` | `../../b/target` | `/a/b/c` | refused — it lands in a *sibling* subtree |
/// | `/a/b/link` | `/a/b/abs` | `/a/b` | `/a/b/a/b/abs` — see below |
/// | `link` | `target` | `.` | `target` — the `.` escape hatch |
///
/// The fourth row is why the check is on the *cleaned* result and not on the
/// number of `..`: escaping and returning is legitimate, and only the landing
/// place matters.
///
/// The sixth row is `path.Join` treating a later element as relative even when
/// it begins with `/`. A symlink whose target looks absolute is therefore
/// appended to the link's directory instead of obeyed — the opposite of what
/// `std::path::Path::join` does, and the reason this uses [`crate::gopath`].
///
/// # The error text
///
/// `'` is doubled in both paths, upstream's way of quoting for a log line. It
/// is reproduced because the message is the only thing a user sees when an
/// action's symlink is refused.
pub fn symlink_join(
    filename: &str,
    sym: &str,
    parent: &str,
) -> Result<String, String> {
    let dir = crate::gopath::dir(filename);
    let dest = crate::gopath::join(&[&dir, sym]);
    let prefix = format!("{}/", crate::gopath::clean(parent));
    if dest.starts_with(&prefix) || prefix == "./" {
        return Ok(dest);
    }
    Err(format!(
        "symlink tries to access file '{}' outside of '{}'",
        dest.replace('\'', "''"),
        parent.replace('\'', "''")
    ))
}

// ---------------------------------------------------------------------------
// imageOS
// ---------------------------------------------------------------------------

/// The `ImageOS` a step's platform label implies.
///
/// Upstream builds an image name that cannot be looked up — the comment says
/// so — and this reproduces it rather than fixing it, because a workflow may
/// already read the variable.
///
/// The recipe is two steps, and the order matters: the **first** `-` is removed,
/// then everything from the first `.` is dropped. So `ubuntu-22.04` becomes
/// `ubuntu22`, and only the first `-` goes. Measured on v0.2.89:
///
/// | label | `ImageOS` |
/// |---|---|
/// | `ubuntu-latest` | `ubuntu20` — hardcoded, see below |
/// | `ubuntu-22.04` | `ubuntu22` |
/// | `ubuntu-20.04` | `ubuntu20` |
/// | `macos-13-xlarge` | `macos13-xlarge` — only the first `-` |
/// | `windows-latest` | `windowslatest` |
/// | `node16` | `node16` — no `-` at all |
/// | `a--b` | `a-b` |
/// | `x-1.2-y` | `x1` |
/// | `foo.bar-baz` | `foo` — the `.` is cut before the `-` is |
/// | `""` | `""` |
///
/// `ubuntu-latest` is special-cased to the literal `ubuntu20` "since we have no
/// way to check that on the fly" — so a run on Ubuntu 24 still reports
/// `ubuntu20`. Kept.
pub fn image_os(platform_name: &str) -> String {
    if platform_name.is_empty() {
        return String::new();
    }
    if platform_name == "ubuntu-latest" {
        return "ubuntu20".to_string();
    }
    // `strings.Replace(s, "-", "", 1)` — the first occurrence only. Rust's
    // `replacen` takes the same count.
    let without_first_dash = platform_name.replacen('-', "", 1);
    // `strings.SplitN(s, ".", 2)[0]`
    without_first_dash
        .split_once('.')
        .map(|(head, _)| head.to_string())
        .unwrap_or(without_first_dash)
}

// ---------------------------------------------------------------------------
// The step evaluator and the two conditions
// ---------------------------------------------------------------------------

/// `NewStepExpressionEvaluatorExt`: the environment a *step* evaluates in.
///
/// It differs from the job evaluator in four ways, and all four are load-bearing:
///
/// 1. `Env` is the **step's** environment, not the job's — so `env.FOO` inside a
///    step sees what that step set, not what the job set.
/// 2. `Config.Context` is `"step"`, which is what makes `success()` and
///    `failure()` read `job.status` instead of the `needs` chain. A step
///    continuing after a failed *step* is the whole point; a job's own needs are
///    already settled by the time its steps run.
/// 3. `Jobs` is **not set**. A step has no `jobs` context at all, so
///    `jobs.<id>.result` is absent rather than empty.
/// 4. The `github` context comes from the **step's** getter and the `ghc`
///    argument is ignored — upstream names the parameter `_` and does exactly
///    this. Reproduced, because the step's context is the one with `GITHUB_ACTION`
///    and `GITHUB_ACTION_PATH` filled in, which the job's is not.
///
/// `rc_inputs` is upstream's `rcInputs` flag, and it changes **which
/// environment the `inputs` context is harvested from** — not which environment
/// the step sees:
///
/// * `true` (the main stage): the *run's* env, so a `uses:` action's own
///   `INPUT_*` are not mistaken for the workflow's inputs.
/// * `false` (`pre:`/`post:`): the *step's* env.
///
/// The `env` context itself is the step's either way, which is what makes
/// `env.FOO` inside a step see what that step set.
///
/// `run_env` is passed in rather than read from `rc`, because upstream's
/// `GetEnv` memoises into the context and this takes `&RunContext`. The caller
/// has already paid for the merge — `setup_env` calls it first — so the value
/// here is the same one upstream would have read.
pub fn new_step_expression_evaluator_with_env(
    rc: &super::run_context::RunContext,
    step_env: &BTreeMap<String, String>,
    run_env: &BTreeMap<String, String>,
    github: &crate::model::GithubContext,
    rc_inputs: bool,
) -> crate::expr::EvaluationEnvironment {
    let inputs_env = if rc_inputs { run_env } else { step_env };
    crate::expr::EvaluationEnvironment {
        github: Some(github.to_value()),
        env: super::expression::env_context(step_env),
        job: Some(crate::expr::Value::object([(
            "status",
            crate::expr::Value::String(rc.get_job_context().status),
        )])),
        // A step has no `jobs` context. `None` is not an empty map: the
        // difference is visible as `jobs` being absent.
        jobs: None,
        steps: super::expression::steps_context(rc.get_steps_context()),
        // Upstream reads this off the container. Until the lifecycle lands
        // there is nothing to ask, so the context is empty rather than
        // invented — the same named gap as in the job evaluator.
        runner: BTreeMap::new(),
        secrets: super::expression::secrets_context(&rc.config.secrets),
        vars: super::expression::env_context(&rc.config.vars),
        strategy: super::expression::strategy_context(rc.run.as_ref()),
        matrix: rc
            .matrix
            .iter()
            .map(|(key, value)| (key.clone(), crate::expr::from_json_value(value)))
            .collect(),
        needs: super::expression::needs_context(rc.run.as_ref()),
        inputs: super::expression::get_evaluator_inputs(rc, inputs_env, github),
        hash_files: None,
    }
}

/// `isStepEnabled`: a step's `if:`, evaluated for `stage`.
///
/// # The stage decides the implicit check, and that is not a detail
///
/// `post:` gets [`crate::expr::DefaultStatusCheck::Always`], everything else
/// gets `Success`. Without it a `post:` step would be skipped by the very
/// failure it exists to clean up after, so a `post:` that writes a summary or
/// uploads an artifact would silently never run.
pub fn is_step_enabled(
    environment: &crate::expr::EvaluationEnvironment,
    status: &dyn crate::expr::StatusProvider,
    expr: &str,
    stage: StepStage,
) -> Result<bool, String> {
    let default_status_check = match stage {
        StepStage::Post => crate::expr::DefaultStatusCheck::Always,
        StepStage::Pre | StepStage::Main => crate::expr::DefaultStatusCheck::Success,
    };
    super::expression::eval_bool(
        environment,
        status,
        crate::expr::EvaluationContext::Step,
        expr,
        default_status_check,
    )
    .map_err(|error| format!("  ❌  Error in if-expression: \"if: {expr}\" ({error})"))
}

/// `isContinueOnError`: a step's `continue-on-error:`, evaluated.
///
/// Two things differ from [`is_step_enabled`] and both matter:
///
/// * The implicit check is **none**, not `Success`. Upstream passes
///   `DefaultStatusCheckNone`, so `continue-on-error: ${{ !cancelled() }}` is a
///   plain expression with nothing prepended — prepending `success() &&` would
///   change what a step is asking.
/// * An absent or blank value is `false` with **no** error, checked before any
///   evaluation. `continue-on-error:` with nothing after it is not a syntax
///   error upstream; it is the default.
///
/// The error text carries the same `❌` shape as the `if:` one, with
/// `continue-on-error:` as the key.
pub fn is_continue_on_error(
    environment: &crate::expr::EvaluationEnvironment,
    status: &dyn crate::expr::StatusProvider,
    expr: &str,
) -> Result<bool, String> {
    if expr.trim().is_empty() {
        return Ok(false);
    }
    super::expression::eval_bool(
        environment,
        status,
        crate::expr::EvaluationContext::Step,
        expr,
        crate::expr::DefaultStatusCheck::None,
    )
    .map_err(|error| {
        format!("  ❌  Error in continue-on-error-expression: \"continue-on-error: {expr}\" ({error})")
    })
}

// ---------------------------------------------------------------------------
// mergeEnv and setupEnv
// ---------------------------------------------------------------------------

/// `mergeEnv`: the step's starting environment, before interpolation.
///
/// Three things happen, in this order:
///
/// 1. the job's environment — and the `container.env:` block, when the job has
///    a `container:`, so a job-level container variable reaches the step;
/// 2. the twenty-odd `GITHUB_*` variables, which therefore **overwrite** a
///    workflow `env:` of the same name. `GITHUB_SHA: whatever` in a workflow is
///    silently replaced by the real one;
/// 3. for a `uses:` step only, every key *containing* `INPUT_` is deleted.
///
/// The third is the one that reads like a bug and is not. A `uses:` step's
/// `with:` block has already been flattened into `INPUT_*` by the job
/// environment, so an action that declares fewer inputs than the caller passes
/// would otherwise see the surplus ones — and GitHub does not pass them. The
/// comment upstream carries says "due to design flaw", and the deletion is the
/// workaround. It is deliberately `Contains`, not `HasPrefix`, which is why a
/// workflow variable named `MY_INPUT_` is also dropped from a `uses:` step.
pub fn merge_env(
    rc: &super::run_context::RunContext,
    step_env: &mut BTreeMap<String, String>,
    step_model: &crate::model::Step,
) {
    let run_env = {
        let mut rc = rc.clone();
        rc.get_env()
    };

    let container_env = rc
        .run
        .as_ref()
        .and_then(|run| run.job())
        .zip(rc.run.as_ref())
        .and_then(|(job, run)| job.container(run.document()))
        .map(|spec| spec.env)
        .unwrap_or_default();

    if container_env.is_empty() {
        merge_into_map(rc, step_env, &[run_env]);
    } else {
        merge_into_map(rc, step_env, &[run_env, container_env]);
    }

    // The third step of `mergeEnv`, which needs no evaluator and therefore
    // belongs here rather than in the github half. See the note on the deletion
    // above the function.
    if !step_model.uses.is_empty() {
        step_env.retain(|key, _| !key.contains("INPUT_"));
    }
}

/// `mergeEnv`'s `github` half, split out because it needs an evaluator and
/// [`merge_env`] does not have one to hand.
pub fn merge_github_env(
    rc: &super::run_context::RunContext,
    environment: &crate::expr::EvaluationEnvironment,
    status: &dyn crate::expr::StatusProvider,
    github: &crate::model::GithubContext,
    step_env: &mut BTreeMap<String, String>,
) {
    rc.with_github_env(environment, status, github, step_env);
}

/// `setupEnv`: the environment a step actually runs with, in one map.
///
/// Five steps, in this order, and the order is the whole function:
///
/// 1. [`merge_env`] — the job's environment, the container's `env:`, and the
///    twenty-odd `GITHUB_*` variables;
/// 2. the step's own `env:` block **last**, so a workflow's step-level
///    variable cannot be overwritten by the defaults above it. The comment
///    upstream carries says exactly that;
/// 3. one interpolation pass over everything that is **not** `INPUT_*`,
///    against the **run's** environment;
/// 4. the evaluator rebuilt from the **step's** now-resolved environment;
/// 5. a second pass over the `INPUT_*` keys alone.
///
/// # Why two passes and not one
///
/// Steps 3 and 5 look like a cycle: a step can set an env var that an action's
/// `with:` value references, and an action's `with:` value is an `INPUT_*`
/// variable. Resolving both against one snapshot cannot order them. So the
/// non-`INPUT_*` half is settled first, the evaluator is rebuilt over the
/// result, and only then are the `INPUT_*` values folded in. That is what
/// upstream's comment means by "after we have an evaluated step context,
/// update the expressions evaluator with a new env context — you can use step
/// level env in the `with` property of a `uses` construct".
///
/// The two passes also read *different* environments on purpose, and this is
/// easy to get backwards: pass 1 evaluates the **step's** values against the
/// **run's** environment, because the step's own values are not yet resolved
/// and would otherwise resolve against themselves.
///
/// # The evaluator is a *job* evaluator, despite the values being the step's
///
/// Both passes use `rc.NewExpressionEvaluator` / `…WithEnv`, and that pair is
/// the one act configures `Context: "job"` — so `success()` and `failure()`
/// resolve through the **`needs` chain**, not through `job.status`. The
/// step-scoped pair is `rc.NewStepExpressionEvaluator`, and `setupEnv` never
/// touches it.
///
/// The name makes this the obvious thing to get backwards, and getting it
/// backwards does not error: a step whose `env:` holds `${{ success() }}` in a
/// job that needed a failed job silently becomes `false` instead of `true`.
/// Measured on v0.2.89 rather than read, because `expression.go` carries both
/// `Context:` literals 62 lines apart and which function closes over which one
/// is not visible from the literal:
///
/// | evaluator | `success()` with a failed `needs` and no failed step |
/// |---|---|
/// | `NewExpressionEvaluator` | **`false`** — the needs chain |
/// | `NewExpressionEvaluatorWithEnv` | **`false`** |
/// | `NewStepExpressionEvaluator` | `true` — `job.status` |
/// | `NewStepExpressionEvaluatorExt` | `true` |
///
/// # Two github contexts, deliberately
///
/// `step_github` is the *step's* `getGithubContext`, which for a remote action
/// carries the resolved `action_repository`/`action_ref`; the evaluators are
/// built from the **run's**, which does not. Upstream therefore assembles the
/// context twice here, and so does this.
///
/// # `merge_env` and `merge_github_env` are called in the opposite order to
/// upstream's single `mergeEnv`
///
/// Upstream runs the `GITHUB_*` write **before** the deletion of every key
/// containing `INPUT_`; this port deletes first and writes after. The two
/// orders are indistinguishable because no key `withGithubEnv` or
/// `setActionRuntimeVars` can write contains `INPUT_` — the full set is
/// `CI`, `GITHUB_*`, `RUNNER_*`, `ACTIONS_RUNTIME_URL`,
/// `ACTIONS_RESULTS_URL`, `ACTIONS_RUNTIME_TOKEN` and `ImageOS`. A test pins
/// the claim; it is a claim about the *whole* helper, not just the visible
/// part, so it is not the kind of thing to assert by reading.
///
/// # Errors
///
/// Upstream's signature returns `error` and the body always returns `nil`.
/// This one returns `Result` because [`super::run_context::RunContext::get_github_context`]
/// has a real error to report — and that context is only ever built, never
/// inspected, so its error is not one to propagate.
pub fn setup_env(
    rc: &mut super::run_context::RunContext,
    git: &crate::model::GitLookups,
    status: &dyn crate::expr::StatusProvider,
    step_env: &mut BTreeMap<String, String>,
    step_model: &crate::model::Step,
    step_github: &crate::model::GithubContext,
) -> Result<(), String> {
    // `GetEnv` memoises into the context, so this is the one value every later
    // reader sees. Called first, on purpose: `merge_env` re-reads it, and the
    // memoisation is what makes the two reads one value rather than two.
    let run_env = rc.get_env();

    // The evaluator the `GITHUB_*` half and pass 1 share. Upstream's
    // `rc.ExprEval` and its `rc.NewExpressionEvaluator(ctx)` are the same
    // construction from the same environment, so one environment here is not a
    // shortcut — it is the same environment.
    let run_github = match rc.get_github_context(git) {
        Ok(github) => github,
        // `getGithubContext` has no error return upstream; a failure here is one
        // of the git lookups refusing, and act carries on with whatever it
        // built. Reproduced as a skip, which is the same as carrying on.
        Err(_) => return Ok(()),
    };
    let run_environment =
        super::expression::new_expression_evaluator_with_env(rc, &run_env, &run_github);

    merge_env(rc, step_env, step_model);
    merge_github_env(rc, &run_environment, status, step_github, step_env);

    // `merge step env last, since it should not be overwritten`.
    // No run means no `env:` block to read, which is an empty map rather than
    // an error: the step is simply not from a workflow.
    let step_own_env = match rc.run.as_ref() {
        Some(run) => step_model.get_env(run.document()),
        None => BTreeMap::new(),
    };
    merge_into_map(rc, step_env, &[step_own_env]);

    // Pass 1: everything but the action's own inputs, against the run's env.
    interpolate_env(&run_environment, status, step_env, false);
    // Pass 2: the inputs, against the step's *resolved* env.
    let step_environment =
        super::expression::new_expression_evaluator_with_env(rc, step_env, step_github);
    interpolate_env(&step_environment, status, step_env, true);

    Ok(())
}

/// One of [`setup_env`]'s two passes: interpolate in place, selecting by the
/// `INPUT_` prefix.
///
/// The selection is `HasPrefix` and not the `Contains` that [`merge_env`]
/// deletes with. Both are upstream, on the same map, in the same function —
/// the deletion is deliberately broad because a `uses:` step must not inherit a
/// polluted `INPUT_`, and the substitution is deliberately narrow because a
/// variable merely *containing* the text is not an action input.
///
/// The context is [`crate::expr::EvaluationContext::Job`], because both
/// evaluators in [`setup_env`] come from `rc.NewExpressionEvaluator*`, and that
/// pair is the *job*-scoped one. Measured, not read — see
/// `success_in_an_env_value_reads_the_needs_chain_not_the_step_status`.
///
/// A value that fails to interpolate becomes `""`, which is what upstream's
/// `Interpolate` returns for an evaluation error.
fn interpolate_env(
    environment: &crate::expr::EvaluationEnvironment,
    status: &dyn crate::expr::StatusProvider,
    step_env: &mut BTreeMap<String, String>,
    inputs_only: bool,
) {
    let resolved: Vec<(String, String)> = step_env
        .iter()
        .filter(|(key, _)| key.starts_with("INPUT_") == inputs_only)
        .map(|(key, value)| {
            let value = super::expression::interpolate(
                environment,
                status,
                crate::expr::EvaluationContext::Job,
                value,
            )
            .unwrap_or_default();
            (key.clone(), value)
        })
        .collect();
    for (key, value) in resolved {
        step_env.insert(key, value);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        image_os, is_continue_on_error, is_step_enabled, merge_env,
        merge_into_map_case_insensitive, merge_into_map_case_sensitive,
        new_step_expression_evaluator_with_env, setup_env, symlink_join, StepStage,
        MAX_SYMLINK_DEPTH,
    };
    use crate::model::{Run, StepResult, StepStatus, Workflow};
    use crate::runner::run_context::{ContainerPaths, RunConfig, RunContext};
    use crate::yaml_node::Document;
    use std::collections::BTreeMap;
    use std::rc::Rc;

    /// A `github` context built without a repository, which is the host
    /// environment (`-P ubuntu-latest=`) and needs no git lookups.
    ///
    /// Local rather than shared: `run_context`'s equivalent lives in that
    /// module's test scope, and a test helper from one module's tests reaching
    /// into another's is how a test ends up depending on someone else's fixture.
    fn no_git() -> crate::model::GitLookups {
        crate::model::GitLookups::new(
            |_| Err(anyhow::anyhow!("not a repository")),
            |_| Err(anyhow::anyhow!("not a repository")),
            |_, _, _| Err(anyhow::anyhow!("not a repository")),
        )
    }

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect()
    }

    // --------------------------------------------------------- stepStage --

    /// Upstream has no test for this, but the three names appear in act's log
    /// output and in `RUNNER_STEP`-style diagnostics.
    #[test]
    fn the_three_stages_have_upstreams_names() {
        assert_eq!(StepStage::Pre.to_string(), "Pre");
        assert_eq!(StepStage::Main.to_string(), "Main");
        assert_eq!(StepStage::Post.to_string(), "Post");
    }

    /// `maxSymlinkDepth` is the loop bound in the two action step types' file
    /// readers; a deeper chain is an error rather than an infinite walk. The
    /// value is upstream's, and it is asserted so a change to it is deliberate
    /// even though nothing in this module uses it yet.
    #[test]
    fn the_symlink_depth_limit_is_ten() {
        assert_eq!(MAX_SYMLINK_DEPTH, 10);
    }

    // ------------------------------------------------- mergeIntoMap --

    /// Upstream `TestMergeIntoMap`, both halves, on the same target — which is
    /// the shape of the upstream test and the reason it expects the same
    /// result twice.
    #[test]
    fn upstream_merge_into_map_table() {
        for (name, target, maps, expected) in [
            ("empty", map(&[]), vec![], map(&[])),
            (
                "into empty",
                map(&[]),
                vec![map(&[("key1", "value1"), ("key2", "value2")]), map(&[("key2", "overridden"), ("key3", "value3")])],
                map(&[("key1", "value1"), ("key2", "overridden"), ("key3", "value3")]),
            ),
            (
                "into existing",
                map(&[("key1", "value1"), ("key2", "value2")]),
                vec![map(&[("key1", "overridden")])],
                map(&[("key1", "overridden"), ("key2", "value2")]),
            ),
        ] {
            let mut sensitive = target.clone();
            merge_into_map_case_sensitive(&mut sensitive, &maps);
            assert_eq!(sensitive, expected, "case-sensitive: {name}");

            // Now the case-insensitive half, on the *same* target — so it has
            // to be a no-op on already-merged data.
            let mut insensitive = target.clone();
            merge_into_map_case_insensitive(&mut insensitive, &maps);
            assert_eq!(insensitive, expected, "case-insensitive: {name}");
        }
    }

    /// The rule the case-insensitive half exists for: a differently-cased
    /// incoming key updates the existing variable rather than creating a
    /// second one, and the **existing spelling** is what survives.
    ///
    /// Measured on v0.2.89.
    #[test]
    fn a_differently_cased_key_updates_the_existing_name() {
        for (target, incoming, expected) in [
            (vec![("KEY", "1")], vec![("key", "2")], vec![("KEY", "2")]),
            (vec![("key", "1")], vec![("KEY", "2")], vec![("key", "2")]),
            (vec![("Ä", "1")], vec![("ä", "2")], vec![("Ä", "2")]),
            (vec![("", "1")], vec![("", "2")], vec![("", "2")]),
        ] {
            let mut out = map(&target);
            merge_into_map_case_insensitive(&mut out, &[map(&incoming)]);
            assert_eq!(out, map(&expected), "target {target:?} incoming {incoming:?}");
        }
    }

    /// Among several case variants arriving in one pass, the **first** one
    /// names the variable and every later variant writes to it. Measured:
    /// `{"A":"1"}, {"a":"2"}, {"A":"3"}` ends as `{"A":"3"}`.
    #[test]
    fn the_first_spelling_seen_wins_and_the_last_value_wins() {
        let mut out = map(&[]);
        merge_into_map_case_insensitive(
            &mut out,
            &[map(&[("A", "1")]), map(&[("a", "2")]), map(&[("A", "3")])],
        );
        assert_eq!(out, map(&[("A", "3")]));
    }

    /// The deviation, pinned: a target that *already* holds two case variants
    /// resolves by the map's iteration order upstream — a coin flip — and by
    /// sorted order here. The sorted order is stated so the choice is visible
    /// rather than incidental, and the same rule is used for the target's keys
    /// and for incoming ones.
    #[test]
    fn a_target_with_two_case_variants_resolves_deterministically_here() {
        let mut out = map(&[("A", "1"), ("a", "9")]);
        merge_into_map_case_insensitive(&mut out, &[map(&[("a", "2")])]);
        // "A" sorts before "a", so "A" registered the fold and the incoming
        // "a" wrote to it. Upstream gives this or {"A":"1","a":"2"} depending
        // on which key it happened to iterate first.
        assert_eq!(out, map(&[("A", "2"), ("a", "9")]));
    }

    /// The case-sensitive half must treat the two names as unrelated, which is
    /// what makes the fork above visible at all.
    #[test]
    fn the_case_sensitive_half_keeps_both_names() {
        let mut out = map(&[("KEY", "1")]);
        merge_into_map_case_sensitive(&mut out, &[map(&[("key", "2")])]);
        assert_eq!(out, map(&[("KEY", "1"), ("key", "2")]));
    }

    // ------------------------------------------------------ symlinkJoin --

    /// Every row measured on v0.2.89. `symlinkJoin` has **no upstream test**,
    /// and it is a path-traversal guard, so the Go source and a probe are the
    /// only authority available — weaker, and named as such.
    #[test]
    fn symlink_join_reproduces_every_measured_row() {
        for (filename, sym, parent, want) in [
            ("/a/b/link", "target", "/a/b", Ok("/a/b/target".to_string())),
            ("/a/b/link", "sub/target", "/a/b", Ok("/a/b/sub/target".to_string())),
            ("/a/b/link", "target", "/a/b/", Ok("/a/b/target".to_string())),
            ("/a/b/link", "target", "/a/b/..", Ok("/a/b/target".to_string())),
            ("link", "target", ".", Ok("target".to_string())),
            ("link", "target", "./", Ok("target".to_string())),
            ("/a/link", "../a/b/target", "/a", Ok("/a/b/target".to_string())),
            (
                "/a/b/link",
                "/a/b/abs",
                "/a/b",
                Ok("/a/b/a/b/abs".to_string()),
            ),
            ("/a/b/link", "it's", "/a/b", Ok("/a/b/it's".to_string())),
            (
                "/a/b/link",
                "../../../a/b/target",
                "/a/b",
                Ok("/a/b/target".to_string()),
            ),
            ("/a/b/link", "a/b/target", "/a/b", Ok("/a/b/a/b/target".to_string())),
            (
                "/a/b/link",
                "../outside",
                "/a/b",
                Err("symlink tries to access file '/a/outside' outside of '/a/b'".to_string()),
            ),
            (
                "/a/b/link",
                "../../etc/passwd",
                "/a/b",
                Err("symlink tries to access file '/etc/passwd' outside of '/a/b'".to_string()),
            ),
            (
                "/a/b/c/link",
                "../../b/target",
                "/a/b/c",
                Err("symlink tries to access file '/a/b/target' outside of '/a/b/c'".to_string()),
            ),
            (
                "/a/b/link",
                "x",
                "/a/it's",
                Err(
                    "symlink tries to access file '/a/b/x' outside of '/a/it''s'".to_string(),
                ),
            ),
        ] {
            assert_eq!(
                symlink_join(filename, sym, parent),
                want,
                "symlinkJoin({filename:?}, {sym:?}, {parent:?})"
            );
        }
    }

    /// Climbing out and back in is legitimate, and the check is on the landing
    /// place rather than on the number of `..`. Pinned separately because it is
    /// the row that distinguishes a lexical-cleaning guard from a naive
    /// "contains `..`" one.
    #[test]
    fn a_target_that_escapes_and_returns_is_allowed() {
        assert_eq!(
            symlink_join("/a/b/link", "../../../a/b/target", "/a/b"),
            Ok("/a/b/target".to_string()),
        );
    }

    /// Landing in a *sibling* subtree of the parent is refused, even though the
    /// escaping `..` is the same count as the row above.
    #[test]
    fn a_target_that_lands_beside_the_parent_is_refused() {
        assert!(symlink_join("/a/b/c/link", "../../b/target", "/a/b/c").is_err());
    }

    /// An empty parent cleans to `.` and switches the check off entirely. This
    /// is upstream's own escape hatch, so it is asserted rather than left to
    /// look like an oversight.
    #[test]
    fn a_parent_that_cleans_to_a_dot_disables_the_check() {
        assert!(symlink_join("/a/b/link", "../../../etc/passwd", ".").is_ok());
        assert!(symlink_join("/a/b/link", "../../../etc/passwd", "").is_ok());
    }

    // ---------------------------------------------------------- imageOS --

    /// Every row measured on v0.2.89.
    #[test]
    fn image_os_reproduces_every_measured_row() {
        for (label, want) in [
            ("", ""),
            ("ubuntu-latest", "ubuntu20"),
            ("ubuntu-22.04", "ubuntu22"),
            ("ubuntu-20.04", "ubuntu20"),
            ("macos-13", "macos13"),
            ("macos-13-xlarge", "macos13-xlarge"),
            ("windows-2022", "windows2022"),
            ("windows-latest", "windowslatest"),
            ("node16", "node16"),
            ("self-hosted", "selfhosted"),
            ("a-b-c", "ab-c"),
            ("-", ""),
            ("a-", "a"),
            ("-a", "a"),
            ("a.b.c", "a"),
            ("ubuntu.latest", "ubuntu"),
            ("x-1.2-y", "x1"),
            ("UPPER-Case", "UPPERCase"),
            ("a--b", "a-b"),
            ("foo.bar-baz", "foo"),
            ("-a-", "a-"),
            ("..", ""),
            ("a-..", "a"),
        ] {
            assert_eq!(image_os(label), want, "ImageOS({label:?})");
        }
    }

    /// Only the **first** `-` goes, and the `.` is cut before either is
    /// considered. Those two rows are the whole recipe.
    #[test]
    fn only_the_first_dash_goes_and_the_dot_is_cut_first() {
        assert_eq!(image_os("macos-13-xlarge"), "macos13-xlarge");
        assert_eq!(image_os("foo.bar-baz"), "foo");
    }

    // -------------------------------------------- isStepEnabled / isCOE --

    /// The conditions need a run context and a built environment, so this
    /// module does the same three-step dance `is_enabled` does.
    fn condition_fixture(
        step_yaml: &str,
        conclusions: &[(&str, StepStatus)],
    ) -> (RunContext, String) {
        // The step's own keys go under a `steps:` list. Placing them beside
        // `runs-on:` makes them *job* keys, which yields a job with no steps at
        // all — and then `if:` is empty and every row passes or fails for the
        // wrong reason.
        let mut job_yaml = String::from("    runs-on: ubuntu-latest\n    steps:\n");
        for line in step_yaml.lines() {
            job_yaml.push_str("      - ");
            job_yaml.push_str(line);
            job_yaml.push('\n');
        }
        let source = format!("name: test-workflow\njobs:\n  job1:\n{job_yaml}");
        let doc = Rc::new(Document::parse(&source).expect("the fixture parses"));
        let workflow = Workflow::from_document("test.yml", &doc).expect("decodes");
        let config = RunConfig {
            workdir: ".".to_string(),
            platforms: [("ubuntu-latest".to_string(), "ubuntu-latest".to_string())]
                .into_iter()
                .collect(),
            ..RunConfig::default()
        };
        let mut rc = RunContext {
            config,
            run: Some(Run::new(workflow, doc, "job1")),
            ..RunContext::default()
        };
        for (id, conclusion) in conclusions {
            rc.step_results.insert(
                (*id).to_string(),
                StepResult {
                    conclusion: *conclusion,
                    ..StepResult::default()
                },
            );
        }
        // The step's own `if:` text, which is what the runner hands to
        // `is_step_enabled` — upstream reads it from the model, not the env.
        let step_if = rc
            .run
            .as_ref()
            .and_then(|run| run.job())
            .and_then(|job| job.steps.first())
            .and_then(|step| step.raw_if)
            .and_then(|id| rc.run.as_ref().expect("a run").document().scalar(id))
            .unwrap_or_default();
        (rc, step_if)
    }

    /// The environment and status a step condition is evaluated against.
    fn condition_parts(rc: &RunContext) -> (
        crate::expr::EvaluationEnvironment,
        super::super::expression::RunStatus,
    ) {
        // `get_env` memoises and therefore needs `&mut`; a clone is the same
        // map the real step would see and keeps the helper callable on `&`.
        let env = rc.clone().get_env();
        let github = rc.get_github_context(&no_git()).expect("builds");
        let environment = new_step_expression_evaluator_with_env(rc, &env, &env, &github, true);
        (environment, super::super::expression::RunStatus::new(rc))
    }

    /// Upstream `TestIsStepEnabled`, all nine rows.
    ///
    /// The condition reads the **step** context: `success()` is true when the
    /// job has not failed, and a recorded `Failure` step makes the job fail. It
    /// is the same word as the job-level `success()` in
    /// [`crate::runner::run_context::RunContext::is_enabled`] and a different
    /// function — that one walks the `needs` chain, this one reads `job.status`.
    #[test]
    fn upstream_is_step_enabled_table() {
        for (expr, conclusions, want) in [
            ("success()", vec![], true),
            ("success()", vec![("a", StepStatus::Success)], true),
            ("success()", vec![("a", StepStatus::Failure)], false),
            ("failure()", vec![], false),
            ("failure()", vec![("a", StepStatus::Success)], false),
            ("failure()", vec![("a", StepStatus::Failure)], true),
            ("always()", vec![], true),
            ("always()", vec![("a", StepStatus::Success)], true),
            ("always()", vec![("a", StepStatus::Failure)], true),
        ] {
            let (rc, step_if) = condition_fixture(&format!("if: {expr}"), &conclusions);
            let (environment, status) = condition_parts(&rc);
            let got = is_step_enabled(&environment, &status, &step_if, StepStage::Main);
            assert_eq!(got, Ok(want), "if: {expr} with {conclusions:?}");
        }
    }

    /// The stage picks the implicit check, and `post:` is the one that would
    /// silently never run without it: a failed earlier step makes `success()`
    /// false, so the very step that is supposed to clean up after it would be
    /// skipped.
    ///
    /// Not upstream — its table only exercises `Main`.
    #[test]
    fn a_post_step_inherits_always_and_the_others_inherit_success() {
        let (mut rc, _if) = condition_fixture(
            "if: true",
            &[("a", StepStatus::Failure)],
        );
        rc.step_results.insert(
            "a".to_string(),
            StepResult {
                conclusion: StepStatus::Failure,
                ..StepResult::default()
            },
        );
        let (environment, status) = condition_parts(&rc);
        assert_eq!(
            is_step_enabled(&environment, &status, "true", StepStage::Main),
            Ok(false),
            "main inherits success(), and a failed step failed the job",
        );
        assert_eq!(
            is_step_enabled(&environment, &status, "true", StepStage::Pre),
            Ok(false),
            "pre inherits success() too",
        );
        assert_eq!(
            is_step_enabled(&environment, &status, "true", StepStage::Post),
            Ok(true),
            "post inherits always(), which is what lets cleanup run",
        );
    }

    /// Upstream `TestIsContinueOnError`, the five non-error rows.
    #[test]
    fn upstream_is_continue_on_error_table() {
        for (raw, want) in [
            ("", Ok(false)),
            ("true", Ok(true)),
            ("false", Ok(false)),
            ("${{ 'test' == 'test' }}", Ok(true)),
            ("${{ 'test' != 'test' }}", Ok(false)),
        ] {
            let (rc, _if) = condition_fixture("name: test", &[]);
            let (environment, status) = condition_parts(&rc);
            assert_eq!(
                is_continue_on_error(&environment, &status, raw),
                want,
                "continue-on-error: {raw:?}"
            );
        }
    }

    /// A parse error is an **error**, not a `false` — the one row where
    /// `is_continue_on_error` returns `Err`, and the reason its text matters.
    #[test]
    fn a_broken_continue_on_error_is_an_error_not_a_false() {
        let (rc, _if) = condition_fixture("name: test", &[]);
        let (environment, status) = condition_parts(&rc);
        let error = is_continue_on_error(&environment, &status, "${{ 'test' != test }}")
            .expect_err("a parse error is an error");
        assert!(
            error.starts_with(
                "  ❌  Error in continue-on-error-expression: \"continue-on-error: "
            ),
            "got: {error}"
        );
    }

    /// A **blank** value is the default, checked before anything is evaluated.
    /// Upstream tests only the empty string; whitespace is the same branch, and
    /// `continue-on-error: ` with a trailing space is a realistic YAML.
    #[test]
    fn a_blank_continue_on_error_is_false_without_evaluating() {
        let (rc, _if) = condition_fixture("name: test", &[]);
        let (environment, status) = condition_parts(&rc);
        for blank in ["", " ", "\t", "\n  "] {
            assert_eq!(
                is_continue_on_error(&environment, &status, blank),
                Ok(false),
                "blank {blank:?}"
            );
        }
    }

    /// `continue-on-error:` carries **no** implicit status check, so an
    /// expression asking about cancellation is asked plainly. If `success()`
    /// were prepended, a step that failed would be unable to say so.
    #[test]
    fn continue_on_error_has_no_implicit_status_check() {
        let (mut rc, _if) = condition_fixture("name: test", &[]);
        // A failed step would make a prepended `success() &&` false, so this
        // could never be true if one were being added.
        rc.step_results.insert(
            "a".to_string(),
            StepResult {
                conclusion: StepStatus::Failure,
                ..StepResult::default()
            },
        );
        let (environment, status) = condition_parts(&rc);
        assert_eq!(
            is_continue_on_error(&environment, &status, "${{ !cancelled() }}"),
            Ok(true),
            "the job failed but was not cancelled, and nothing is prepended",
        );
    }

    // ------------------------------------------------------------ mergeEnv --

    /// A `uses:` step loses every key *containing* `INPUT_`; a `run:` step
    /// keeps them all.
    ///
    /// Not upstream. `TestSetupEnv` does reach this branch — its step is a
    /// `uses: ./` — but reaches it with a run environment of `{RC_KEY, ACT}`,
    /// which holds no such key, so it can only show the branch is harmless,
    /// never that it deletes. Pinned here instead.
    #[test]
    fn a_uses_step_drops_input_keys_and_a_run_step_keeps_them() {
        for (uses, keep) in [("", true), ("actions/checkout@v4", false)] {
            let (rc, _if) = condition_fixture("run: echo hi", &[]);
            let mut step = first_step(&rc);
            step.uses = uses.to_string();
            let mut step_env = map(&[
                ("INPUT_NAME", "from-with"),
                ("MY_INPUT_", "also-dropped"),
                ("PATH", "/usr/bin"),
            ]);
            merge_env(&rc, &mut step_env, &step);
            assert_eq!(
                step_env.contains_key("INPUT_NAME"),
                keep,
                "INPUT_NAME with uses={uses:?}"
            );
            assert_eq!(
                step_env.contains_key("MY_INPUT_"),
                keep,
                "the test is Contains, not HasPrefix, with uses={uses:?}"
            );
            assert!(
                step_env.contains_key("PATH"),
                "an unrelated key always survives: {step_env:?}"
            );
        }
    }

    /// The first step of the fixture job, as a model the tests can adjust.
    fn first_step(rc: &RunContext) -> crate::model::Step {
        rc.run
            .as_ref()
            .and_then(|run| run.job())
            .and_then(|job| job.steps.first())
            .expect("the fixture has a step")
            .clone()
    }

    /// Both passes evaluate with the **job** context, so `success()` in an
    /// `env:` value reads the **`needs` chain**.
    ///
    /// This test exists because the port got it wrong first.
    /// `rc.NewExpressionEvaluator` *sounds* like the step-level evaluator and
    /// is the job-scoped one; `rc.NewStepExpressionEvaluator` is the other. A
    /// job that needs a failed job but has failed nothing of its own is exactly
    /// the case where the two answers differ, and picking the wrong one does
    /// not error — it quietly says `false`.
    ///
    /// The table it is checked against was measured inside act's own package
    /// rather than read: `expression.go` holds both `Context:` literals 62
    /// lines apart, and attributing them to the wrong function is a one-grep
    /// mistake. Mutation M6 below is the guard that keeps it from coming back.
    #[test]
    fn success_in_an_env_value_reads_the_needs_chain_not_the_step_status() {
        // A job that needs a failed job but has failed nothing itself.
        let source = "jobs:\n  build:\n    runs-on: ubuntu-latest\n  \"1\":\n    needs: build\n    \
                      steps:\n      - uses: ./\n        env:\n          SAW: ${{ success() }}\n";
        let doc = Rc::new(Document::parse(source).expect("the fixture parses"));
        let mut workflow = Workflow::from_document("test.yml", &doc).expect("decodes");
        // `result` carries no yaml tag upstream either, so it is set by hand
        // for the same reason act's own tests set it by hand.
        workflow
            .jobs
            .get_mut("build")
            .expect("the build job")
            .result = "failure".to_string();
        let run = Run::new(workflow, doc, "1");
        let step = crate::model::Step {
            uses: "./".to_string(),
            raw_env: run
                .job()
                .and_then(|job| job.steps.first())
                .and_then(|step| step.raw_env),
            ..crate::model::Step::default()
        };

        let mut rc = RunContext {
            run: Some(run),
            ..setup_env_fixture()
        };
        let github = rc.get_github_context(&no_git()).expect("builds");
        let status = super::super::expression::RunStatus::new(&rc);
        assert_eq!(rc.get_job_context().status, "success", "no step has failed");
        let needs = rc
            .run
            .as_ref()
            .and_then(|run| run.job())
            .zip(rc.run.as_ref())
            .map(|(job, run)| job.needs(run.document()));
        assert_eq!(
            needs,
            Some(vec!["build".to_string()]),
            "the two success() readings only differ when the needs chain failed"
        );

        let mut env = BTreeMap::new();
        setup_env(&mut rc, &no_git(), &status, &mut env, &step, &github).expect("setup");

        assert_eq!(
            env.get("SAW").map(String::as_str),
            Some("false"),
            "job-scoped success() walks the needs chain, and `build` failed"
        );
    }

    /// Pass 1 evaluates the **step's** values against the **run's**
    /// environment — not against the step's own, which at that moment holds
    /// the unresolved values.
    ///
    /// The observable consequence is a name that exists on both sides with
    /// different values: the step's `env:` block has already been merged in, so
    /// reading `env.LEVEL` from the step's *own* snapshot would answer with
    /// the step's value. Upstream answers with the run's. This is the mirror
    /// image of the previous test, and the two together are the reason the
    /// function has two evaluators rather than one used twice.
    #[test]
    fn pass_one_reads_a_name_from_the_run_env_even_when_the_step_overrides_it() {
        let mut rc = setup_env_fixture();
        rc.env = map(&[("LEVEL", "from-run")]);
        let fixture = run_document_with_step_env("LEVEL: from-step\nSAW: ${{ env.LEVEL }}");
        rc.run = Some(fixture.run);
        let step = crate::model::Step {
            uses: "./".to_string(),
            raw_env: fixture.step_env_node,
            ..crate::model::Step::default()
        };
        let github = rc.get_github_context(&no_git()).expect("builds");
        let status = super::super::expression::RunStatus::new(&rc);

        let mut env = BTreeMap::new();
        setup_env(&mut rc, &no_git(), &status, &mut env, &step, &github).expect("setup");

        assert_eq!(
            env.get("LEVEL").map(String::as_str),
            Some("from-step"),
            "the step's own value is what the variable holds afterwards"
        );
        assert_eq!(
            env.get("SAW").map(String::as_str),
            Some("from-run"),
            "but the value that was read is the run's, because pass 1 evaluates \
             against the run env and the step env is merged last"
        );
    }

    // ----------------------------------------------------------- setupEnv --

    /// Upstream `TestSetupEnv`'s run context.
    ///
    /// Three things in it are load-bearing rather than incidental:
    ///
    /// * `rc.Env` is **already filled in** (`RC_KEY`), which is why `JOB_KEY`
    ///   never appears in upstream's expected map. `GetEnv` merges workflow,
    ///   job and config env only while `rc.Env` is nil, so a pre-set `rc.Env`
    ///   short-circuits all three. The job's `env:` block is kept in the
    ///   fixture precisely so that this is visible rather than accidental.
    /// * `GITHUB_RUN_ID: runId` sits in the **config** env, and reaches the
    ///   result through `withGithubEnv` rather than through the run env — two
    ///   different routes to the same key, and the route is what tells the two
    ///   halves of `mergeEnv` apart.
    /// * The job container is what makes `GITHUB_EVENT_PATH` the act path.
    fn setup_env_fixture() -> RunContext {
        // No `name:` — upstream's fixture has none, and `GITHUB_WORKFLOW` is
        // therefore expected to be the empty string.
        let source = "jobs:\n  \"1\":\n    env:\n      JOB_KEY: jobvalue\n";
        let doc = Rc::new(Document::parse(source).expect("the fixture parses"));
        let workflow = Workflow::from_document("test.yml", &doc).expect("the fixture decodes");
        RunContext {
            config: RunConfig {
                env: map(&[("GITHUB_RUN_ID", "runId")]),
                ..RunConfig::default()
            },
            run: Some(Run::new(workflow, doc, "1")),
            env: map(&[("RC_KEY", "rcvalue")]),
            job_container: Some(ContainerPaths {
                act_path: "/var/run/act".to_string(),
                ..ContainerPaths::default()
            }),
            ..RunContext::default()
        }
    }

    /// Upstream's `uses: ./` step with one `with:` parameter.
    fn uses_step() -> crate::model::Step {
        crate::model::Step {
            uses: "./".to_string(),
            with: map(&[("STEP_WITH", "with-value")]),
            ..crate::model::Step::default()
        }
    }

    /// Upstream `TestSetupEnv`, row for row.
    ///
    /// The eight deletions are upstream's own and are kept: they are the keys
    /// that depend on the checkout act was run against, so a port cannot assert
    /// them without turning the test into a statement about the machine.
    #[test]
    fn upstream_setup_env_builds_the_whole_map() {
        let mut rc = setup_env_fixture();
        let step = uses_step();
        let github = rc.get_github_context(&no_git()).expect("builds");
        let status = super::super::expression::RunStatus::new(&rc);

        let mut env = BTreeMap::new();
        setup_env(
            &mut rc,
            &no_git(),
            &status,
            &mut env,
            &step,
            &github,
        )
        .expect("setup");

        for key in [
            "GITHUB_REF",
            "GITHUB_REF_NAME",
            "GITHUB_REF_TYPE",
            "GITHUB_SHA",
            "GITHUB_WORKSPACE",
            "GITHUB_REPOSITORY",
            "GITHUB_REPOSITORY_OWNER",
            "GITHUB_ACTOR",
        ] {
            env.remove(key);
        }

        assert_eq!(
            env,
            map(&[
                ("ACT", "true"),
                ("CI", "true"),
                ("GITHUB_ACTION", ""),
                ("GITHUB_ACTIONS", "true"),
                ("GITHUB_ACTION_PATH", ""),
                ("GITHUB_ACTION_REF", ""),
                ("GITHUB_ACTION_REPOSITORY", ""),
                ("GITHUB_API_URL", "https:///api/v3"),
                ("GITHUB_BASE_REF", ""),
                ("GITHUB_EVENT_NAME", ""),
                ("GITHUB_EVENT_PATH", "/var/run/act/workflow/event.json"),
                ("GITHUB_GRAPHQL_URL", "https:///api/graphql"),
                ("GITHUB_HEAD_REF", ""),
                ("GITHUB_JOB", "1"),
                ("GITHUB_RETENTION_DAYS", "0"),
                ("GITHUB_RUN_ID", "runId"),
                ("GITHUB_RUN_NUMBER", "1"),
                ("GITHUB_RUN_ATTEMPT", "1"),
                ("GITHUB_SERVER_URL", "https://"),
                ("GITHUB_WORKFLOW", ""),
                ("INPUT_STEP_WITH", "with-value"),
                ("RC_KEY", "rcvalue"),
                ("RUNNER_PERFLOG", "/dev/null"),
                ("RUNNER_TRACKING_ID", ""),
            ]),
        );
    }

    /// `JOB_KEY` is absent above because `rc.Env` was pre-set, not because the
    /// job has no `env:`. Isolating that, because it is the one entry the
    /// fixture seems to promise and does not deliver.
    #[test]
    fn an_unset_run_env_is_what_keeps_the_job_block_out() {
        let mut rc = setup_env_fixture();
        // The one change: let `GetEnv` do its job.
        rc.env.clear();
        let step = uses_step();
        let github = rc.get_github_context(&no_git()).expect("builds");
        let status = super::super::expression::RunStatus::new(&rc);

        let mut env = BTreeMap::new();
        setup_env(&mut rc, &no_git(), &status, &mut env, &step, &github).expect("setup");

        assert_eq!(
            env.get("JOB_KEY").map(String::as_str),
            Some("jobvalue"),
            "with an empty rc.Env the job's env: block is merged, and the config \
             env's GITHUB_RUN_ID wins over the job's"
        );
    }

    /// A one-step fixture whose step carries an `env:` block.
    ///
    /// `model::Step` keeps `env:` as a `NodeId`, and a node id means nothing
    /// without the document it indexes — so the run and the id travel together
    /// rather than a bare step being handed to a test.
    struct StepEnvFixture {
        run: Run,
        step_env_node: Option<crate::yaml_node::NodeId>,
    }

    fn run_document_with_step_env(env_block: &str) -> StepEnvFixture {
        let source = format!(
            "jobs:\n  \"1\":\n    steps:\n      - uses: ./\n        env:\n{}",
            env_block
                .lines()
                .map(|line| format!("          {line}\n"))
                .collect::<String>()
        );
        let doc = Rc::new(Document::parse(&source).expect("the fixture parses"));
        let workflow = Workflow::from_document("test.yml", &doc).expect("decodes");
        let run = Run::new(workflow, doc, "1");
        let step_env_node = run
            .job()
            .and_then(|job| job.steps.first())
            .and_then(|step| step.raw_env);
        assert!(step_env_node.is_some(), "the fixture has an env: block");
        StepEnvFixture {
            run,
            step_env_node,
        }
    }

    /// The two passes exist to break a cycle, and this is the cycle.
    ///
    /// An action's `with:` value is an `INPUT_*` variable, a step can set an
    /// `env:` variable, and either can reference the other. One snapshot cannot
    /// order them, so pass 1 settles everything that is not an input and pass 2
    /// folds the inputs in against the *result*. Resolve pass 2 against the
    /// run's environment instead and `env.STEP_LEVEL` is absent, so the input
    /// comes out empty — which is the exact failure this test refuses.
    #[test]
    fn an_input_can_read_a_step_env_var_because_pass_two_uses_the_step_env() {
        let mut rc = setup_env_fixture();
        let fixture = run_document_with_step_env("STEP_LEVEL: level");
        rc.run = Some(fixture.run);
        let step = crate::model::Step {
            uses: "./".to_string(),
            with: map(&[("FROM_STEP", "${{ env.STEP_LEVEL }}")]),
            raw_env: fixture.step_env_node,
            ..crate::model::Step::default()
        };
        let github = rc.get_github_context(&no_git()).expect("builds");
        let status = super::super::expression::RunStatus::new(&rc);

        let mut env = BTreeMap::new();
        setup_env(&mut rc, &no_git(), &status, &mut env, &step, &github).expect("setup");

        assert_eq!(
            env.get("STEP_LEVEL").map(String::as_str),
            Some("level"),
            "the step's own env: survives, and pass 1 leaves it alone"
        );
        assert_eq!(
            env.get("INPUT_FROM_STEP").map(String::as_str),
            Some("level"),
            "pass 2 resolves the input against the step's resolved env"
        );
    }

    /// The step's `env:` block is merged **last**, so it overwrites the
    /// twenty-odd defaults rather than being overwritten by them.
    ///
    /// Not upstream: its fixture step has no `env:` block, so the sentence in
    /// the source comment — "merge step env last, since it should not be
    /// overwritten" — is never tested there.
    #[test]
    fn a_steps_own_env_overwrites_the_github_defaults() {
        let mut rc = setup_env_fixture();
        let fixture = run_document_with_step_env("GITHUB_WORKFLOW: mine\nCI: no");
        rc.run = Some(fixture.run);
        let step = crate::model::Step {
            uses: "./".to_string(),
            raw_env: fixture.step_env_node,
            ..crate::model::Step::default()
        };
        let github = rc.get_github_context(&no_git()).expect("builds");
        let status = super::super::expression::RunStatus::new(&rc);

        let mut env = BTreeMap::new();
        setup_env(&mut rc, &no_git(), &status, &mut env, &step, &github).expect("setup");

        assert_eq!(env.get("GITHUB_WORKFLOW").map(String::as_str), Some("mine"));
        assert_eq!(env.get("CI").map(String::as_str), Some("no"));
    }

    /// The claim behind this port's split of `mergeEnv`: deleting the
    /// `INPUT_`-containing keys **before** writing the `GITHUB_*` ones is the
    /// same as writing them first, because nothing either helper writes can
    /// contain `INPUT_`.
    ///
    /// It reads like a claim about the visible keys and is not: the argument
    /// only holds for the *whole* key set, including the three `ACTIONS_*`
    /// runtime variables, which are written only when an artifact server is
    /// configured. So the test configures one and puts an `INPUT_`-containing
    /// key into the run env where the deletion can reach it.
    #[test]
    fn the_input_deletion_and_the_github_write_do_not_depend_on_their_order() {
        let mut rc = setup_env_fixture();
        rc.config.artifact_server_path = "/tmp/artifact-server".to_string();
        rc.env = map(&[
            ("RC_KEY", "rcvalue"),
            ("INPUT_LEFTOVER", "leaked"),
            ("MY_INPUT_TAIL", "also-leaked"),
            ("GITHUB_WORKFLOW", "from-the-run-env"),
        ]);
        let step = uses_step();
        let github = rc.get_github_context(&no_git()).expect("builds");
        let status = super::super::expression::RunStatus::new(&rc);

        let mut env = BTreeMap::new();
        setup_env(&mut rc, &no_git(), &status, &mut env, &step, &github).expect("setup");

        let survivors: Vec<&str> = ["INPUT_LEFTOVER", "MY_INPUT_TAIL"]
            .into_iter()
            .filter(|key| env.contains_key(*key))
            .collect();
        assert!(
            survivors.is_empty(),
            "a `uses:` step must not inherit an INPUT_-containing key from the run \
             environment, found {survivors:?} in {env:?}"
        );
        // The one `INPUT_` key that *is* supposed to be there comes from the
        // step's own `with:`, and only because the step's environment is merged
        // after the deletion. Asserting the whole map has no `INPUT_` would
        // have been the wrong test, and would have caught a correct port.
        assert_eq!(
            env.get("INPUT_STEP_WITH").map(String::as_str),
            Some("with-value"),
            "the step's own input is merged back in after the deletion"
        );
        // Presence, not value: `setActionRuntimeVars` prefers an
        // `ACTIONS_RUNTIME_URL` from the ambient environment, and a test that
        // pinned the value would fail on the machine that happens to set it.
        // The claim under test is that these keys exist at all.
        for key in [
            "ACTIONS_RUNTIME_URL",
            "ACTIONS_RESULTS_URL",
            "ACTIONS_RUNTIME_TOKEN",
        ] {
            assert!(
                env.contains_key(key),
                "{key} is one of the keys the ordering claim depends on, and the \
                 artifact server path is what turns it on; env: {env:?}"
            );
        }
        assert_eq!(
            env.get("GITHUB_WORKFLOW").map(String::as_str),
            Some(""),
            "the github half is written after the run env either way, so the \
             workflow name wins over the run's own value"
        );
    }
}
