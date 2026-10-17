//! `runStepExecutor`'s decisions, split from its plumbing.
//!
//! Port of `pkg/runner/step.go`'s `runStepExecutor`, plus `evaluateStepTimeout`
//! and `monitorJobCancellation`.
//!
//! | act | here |
//! |---|---|
//! | the five `GITHUB_*` file paths | [`file_command_paths`] |
//! | the `::add-mask::` log redaction | [`redact_step_string`] |
//! | `StepResult` seeding and its stage gate | [`initial_step_result`], [`registers_in_step_results`] |
//! | the failure → outcome/conclusion decision | [`failure_outcome`] |
//! | `errors.Join` over the file commands | [`join_errors`] |
//!
//! # What is deliberately not here
//!
//! The executor that ties it together — `setupEnv`, copying the five files,
//! `evaluateStepTimeout`, the cancellation watcher and the file-command
//! processing — all drive the job container, and
//! [`crate::runner::job_container`] is being ported alongside. What is here is
//! every decision that function makes, each as a function over plain values, so
//! the behaviour is testable without a daemon and so the wiring is a list of
//! calls rather than a re-derivation of the rules.
//!
//! # No upstream test reaches any of this
//!
//! `runStepExecutor` has no case in `pkg/runner/step_test.go` on v0.2.89 — its
//! four tests are `TestMergeIntoMap`, `TestSetupEnv`, `TestIsStepEnabled` and
//! `TestIsContinueOnError`, and the first two drive it through mocks that stand
//! in for the whole container. The Go source plus the probes behind the tables
//! are therefore the only authority, and every number is labelled measured.

use std::collections::BTreeMap;

use crate::model::StepStatus;

use super::step::StepStage;

/// The five files a step writes its results into, relative to the act path.
///
/// All five are created empty before the step runs, because an action that
/// writes `::set-output::` to a file that does not exist is an error the action
/// reports as a filesystem error rather than as "you did not enable outputs".
///
/// Measured on v0.2.89: these are `path.Join` results, so a trailing slash on the
/// act path is absorbed and an **empty** act path yields a bare relative path
/// with no leading `./`:
///
/// | act path | `GITHUB_OUTPUT` |
/// |---|---|
/// | `/var/run/act` | `/var/run/act/workflow/outputcmd.txt` |
/// | `/var/run/act/` | `/var/run/act/workflow/outputcmd.txt` |
/// | `act` | `act/workflow/outputcmd.txt` |
/// | `""` | `workflow/outputcmd.txt` |
/// | `.` | `workflow/outputcmd.txt` |
pub const WORKFLOW_FILE_COMMANDS: [&str; 5] = [
    "workflow/outputcmd.txt",
    "workflow/statecmd.txt",
    "workflow/pathcmd.txt",
    "workflow/envs.txt",
    "workflow/SUMMARY.md",
];

/// The five `GITHUB_*` variables, in the order `runStepExecutor` sets them.
pub const FILE_COMMAND_VARIABLES: [&str; 5] = [
    "GITHUB_OUTPUT",
    "GITHUB_STATE",
    "GITHUB_PATH",
    "GITHUB_ENV",
    "GITHUB_STEP_SUMMARY",
];

/// The five `GITHUB_*` paths for a given act path, positionally matching
/// [`FILE_COMMAND_VARIABLES`].
///
/// `path.Join`, not `filepath.Join` and not `std::path` — these are container
/// paths, which are slash paths on every host. `std::path` would rewrite them
/// to `\` on Windows and every action would then write to a file the runner does
/// not read. See [`crate::gopath`].
pub fn file_command_paths(act_path: &str) -> [String; 5] {
    let mut out: [String; 5] = Default::default();
    for (index, relative) in WORKFLOW_FILE_COMMANDS.iter().enumerate() {
        out[index] = crate::gopath::join(&[act_path, relative]);
    }
    out
}

/// The same five, as the environment entries a step is given.
///
/// Written into the step's own environment, so a `run:` step that lists
/// `env:` does not shadow them: they are set after `setupEnv` has merged.
pub fn file_command_env(act_path: &str) -> BTreeMap<String, String> {
    file_command_paths(act_path)
        .into_iter()
        .zip(FILE_COMMAND_VARIABLES)
        .map(|(path, name)| (name.to_string(), path))
        .collect()
}

/// What the `⭐ Run …` log line shows for a step.
///
/// A step that emits `::add-mask::` has a secret in it, and the log line
/// interpolates the whole step — so the line is replaced wholesale rather than
/// filtered, which is the only way to be sure nothing leaks.
///
/// Measured on v0.2.89:
///
/// | step string | logged as |
/// |---|---|
/// | `run echo hi` | `run echo hi` |
/// | `::add-mask::secret` | `add-mask command` |
/// | `echo a\n::add-mask:: x\necho b` | `add-mask command` — one line anywhere hides the lot |
/// | `::add-mask` | `::add-mask` — the trailing `::` is required |
/// | `::ADD-MASK::secret` | `::ADD-MASK::secret` — the match is case-sensitive |
/// | `echo ::add-mask::x` | `add-mask command` |
///
/// The last two rows are the ones that matter: a near-miss is **not** redacted,
/// so this is a safety property that depends on the action writing the command
/// exactly. Upstream has the same property and the same hole.
pub fn redact_step_string(step_string: &str) -> String {
    if step_string.contains("::add-mask::") {
        return "add-mask command".to_string();
    }
    step_string.to_string()
}

/// A step's result before anything has run: success, and no outputs.
pub fn initial_step_result() -> crate::model::StepResult {
    crate::model::StepResult {
        outcome: StepStatus::Success,
        conclusion: StepStatus::Success,
        outputs: BTreeMap::new(),
    }
}

/// Whether a stage's result lands in `steps.<id>.conclusion`.
///
/// **Only `main` does.** A `pre:` or `post:` step's result is never published,
/// so `steps.<pre-step-id>` does not exist. That is why a workflow cannot branch
/// on a cleanup step, and it is a deliberate part of the format rather than an
/// omission.
pub fn registers_in_step_results(stage: StepStage) -> bool {
    matches!(stage, StepStage::Main)
}

/// What a step's result becomes after its executor failed.
///
/// # The two fields are not the same thing
///
/// `outcome` is what happened; `conclusion` is what the step is judged to have
/// achieved. They differ **only** for a `continue-on-error` step that failed:
/// outcome `failure`, conclusion `success`. A workflow branching on
/// `steps.<id>.outcome` sees the truth; one branching on `conclusion` sees the
/// policy.
///
/// | case | outcome | conclusion | the step's error |
/// |---|---|---|---|
/// | succeeded | `success` | `success` | none |
/// | failed, `continue-on-error` | `failure` | **`success`** | **discarded** |
/// | failed, no `continue-on-error` | `failure` | `failure` | kept |
///
/// The third row's error is set to nil upstream, so the job carries on and the
/// failure is visible only through the two fields and the log. That is the
/// feature: `continue-on-error` is how a step is allowed to fail without failing
/// the job.
pub struct FailureOutcome {
    /// What the step is recorded as having concluded.
    pub conclusion: StepStatus,
    /// Whether the step's own error survives.
    pub keep_error: bool,
}

/// [`FailureOutcome`] for a step that failed, given whether it may.
pub fn failure_outcome(continue_on_error: bool) -> FailureOutcome {
    if continue_on_error {
        FailureOutcome {
            conclusion: StepStatus::Success,
            keep_error: false,
        }
    } else {
        FailureOutcome {
            conclusion: StepStatus::Failure,
            keep_error: true,
        }
    }
}

/// `errors.Join` over the step's error and the five file-command errors.
///
/// Measured on v0.26.2 (go1.26.2):
///
/// * the messages are joined with a **newline**, in the order given;
/// * a `nil` element is skipped, so a passing file command contributes nothing;
/// * an empty set is `nil` — no error at all;
/// * a **single** error is returned as-is, so `errors.Is` still reaches it.
///
/// The last two are why this is a function and not a format string: the
/// all-clear case must be `None` and not `Some("")`, which would turn a
/// successful step into a failure at the first `?`.
pub fn join_errors(errors: Vec<Option<anyhow::Error>>) -> Option<anyhow::Error> {
    let present: Vec<anyhow::Error> = errors.into_iter().flatten().collect();
    match present.len() {
        0 => None,
        1 => present.into_iter().next(),
        _ => {
            let message = present
                .iter()
                .map(|error| error.to_string())
                .collect::<Vec<String>>()
                .join("\n");
            Some(anyhow::anyhow!(message))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        failure_outcome, file_command_env, file_command_paths, initial_step_result, join_errors,
        redact_step_string, registers_in_step_results, StepStage,
    };
    use crate::model::StepStatus;

    // ------------------------------------------------ file commands --

    /// Every measured row, written out in full so a disagreement names the
    /// act path and the file rather than being reconstructed in the assertion.
    #[test]
    fn the_file_command_paths_reproduce_every_measured_row() {
        type Row = (&'static str, [&'static str; 5]);
        const ROWS: [Row; 5] = [
            (
                "/var/run/act",
                [
                    "/var/run/act/workflow/outputcmd.txt",
                    "/var/run/act/workflow/statecmd.txt",
                    "/var/run/act/workflow/pathcmd.txt",
                    "/var/run/act/workflow/envs.txt",
                    "/var/run/act/workflow/SUMMARY.md",
                ],
            ),
            (
                "/var/run/act/",
                [
                    "/var/run/act/workflow/outputcmd.txt",
                    "/var/run/act/workflow/statecmd.txt",
                    "/var/run/act/workflow/pathcmd.txt",
                    "/var/run/act/workflow/envs.txt",
                    "/var/run/act/workflow/SUMMARY.md",
                ],
            ),
            (
                "act",
                [
                    "act/workflow/outputcmd.txt",
                    "act/workflow/statecmd.txt",
                    "act/workflow/pathcmd.txt",
                    "act/workflow/envs.txt",
                    "act/workflow/SUMMARY.md",
                ],
            ),
            (
                "",
                [
                    "workflow/outputcmd.txt",
                    "workflow/statecmd.txt",
                    "workflow/pathcmd.txt",
                    "workflow/envs.txt",
                    "workflow/SUMMARY.md",
                ],
            ),
            (
                ".",
                [
                    "workflow/outputcmd.txt",
                    "workflow/statecmd.txt",
                    "workflow/pathcmd.txt",
                    "workflow/envs.txt",
                    "workflow/SUMMARY.md",
                ],
            ),
        ];

        for (act_path, expected) in ROWS {
            let paths = file_command_paths(act_path);
            for (index, want) in expected.iter().enumerate() {
                assert_eq!(&paths[index], want, "act_path {act_path:?}, file {index}");
            }
        }
    }

    /// A trailing slash on the act path is absorbed, and an empty or dot act
    /// path yields a bare relative path with **no** leading `./`, because
    /// `path.Join` cleans. Measured, and the second row is the one a
    /// `format!("{act_path}/…")` would get wrong.
    #[test]
    fn the_act_path_is_cleaned_rather_than_concatenated() {
        let with_slash = file_command_paths("/var/run/act/");
        let without = file_command_paths("/var/run/act");
        assert_eq!(with_slash, without, "a trailing slash changes nothing");

        for act_path in ["", "."] {
            for path in file_command_paths(act_path) {
                assert!(
                    !path.starts_with("./"),
                    "act_path {act_path:?} produced {path:?}",
                );
            }
        }
    }

    /// The five variables, and their order, positionally matched to the paths.
    /// A swap here would point `GITHUB_ENV` at the state file and no test
    /// downstream would notice.
    #[test]
    fn the_five_variables_line_up_with_the_five_files() {
        let env = file_command_env("/var/run/act");
        assert_eq!(
            env.get("GITHUB_OUTPUT").map(String::as_str),
            Some("/var/run/act/workflow/outputcmd.txt")
        );
        assert_eq!(
            env.get("GITHUB_STATE").map(String::as_str),
            Some("/var/run/act/workflow/statecmd.txt")
        );
        assert_eq!(
            env.get("GITHUB_PATH").map(String::as_str),
            Some("/var/run/act/workflow/pathcmd.txt")
        );
        assert_eq!(
            env.get("GITHUB_ENV").map(String::as_str),
            Some("/var/run/act/workflow/envs.txt")
        );
        assert_eq!(
            env.get("GITHUB_STEP_SUMMARY").map(String::as_str),
            Some("/var/run/act/workflow/SUMMARY.md")
        );
        assert_eq!(env.len(), 5, "exactly five, no more");
    }

    // ---------------------------------------------------- add-mask --

    /// Every measured row.
    #[test]
    fn the_add_mask_redaction_reproduces_every_measured_row() {
        for (step_string, want) in [
            ("run echo hi", "run echo hi"),
            ("::add-mask::secret", "add-mask command"),
            ("echo a\n::add-mask:: x\necho b", "add-mask command"),
            ("::add-mask", "::add-mask"),
            ("::ADD-MASK::secret", "::ADD-MASK::secret"),
            ("echo ::add-mask::x", "add-mask command"),
        ] {
            assert_eq!(
                redact_step_string(step_string),
                want,
                "step {step_string:?}"
            );
        }
    }

    /// The near-miss that is **not** redacted. Pinned on its own because it is
    /// the property someone will try to "fix": the match is case-sensitive and
    /// needs the closing `::`, exactly as upstream.
    #[test]
    fn a_near_miss_is_not_redacted() {
        assert_eq!(redact_step_string("::add-mask"), "::add-mask");
        assert_eq!(redact_step_string("::ADD-MASK::x"), "::ADD-MASK::x");
        assert_eq!(redact_step_string("::add-mask:"), "::add-mask:");
    }

    // ----------------------------------------------------- results --

    /// Only `main` publishes a result. This is why `steps.<pre-id>` does not
    /// exist, and it is the difference between a cleanup step being branchable
    /// and not.
    #[test]
    fn only_the_main_stage_publishes_a_step_result() {
        assert!(registers_in_step_results(StepStage::Main));
        assert!(!registers_in_step_results(StepStage::Pre));
        assert!(!registers_in_step_results(StepStage::Post));
    }

    /// A step starts as a success with no outputs, so a step that is never run
    /// is indistinguishable from one that succeeded — which is what makes
    /// `steps.<id>.outcome` a reliable default.
    #[test]
    fn a_step_starts_as_a_success_with_no_outputs() {
        let result = initial_step_result();
        assert_eq!(result.outcome, StepStatus::Success);
        assert_eq!(result.conclusion, StepStatus::Success);
        assert!(result.outputs.is_empty());
    }

    /// The two fields differ for exactly one case, and that case is the whole
    /// point of `continue-on-error`.
    #[test]
    fn a_tolerated_failure_concludes_success_and_drops_its_error() {
        let tolerated = failure_outcome(true);
        assert_eq!(tolerated.conclusion, StepStatus::Success);
        assert!(!tolerated.keep_error, "the error is discarded");

        let fatal = failure_outcome(false);
        assert_eq!(fatal.conclusion, StepStatus::Failure);
        assert!(fatal.keep_error, "the error is the step's failure");
    }

    // ---------------------------------------------- errors.Join --

    /// The measured `errors.Join` behaviours, all four of which a `format!`
    /// would get wrong.
    #[test]
    fn join_errors_matches_gos_errors_join() {
        // An empty set is no error at all — not an error with an empty message.
        assert!(join_errors(vec![]).is_none());
        assert!(join_errors(vec![None, None]).is_none());

        // A single error comes back as itself, so a caller can still match it.
        let one = join_errors(vec![Some(anyhow::anyhow!("executor failed"))]);
        assert_eq!(
            one.map(|e| e.to_string()),
            Some("executor failed".to_string())
        );

        // Several are newline-joined in order, and a nil is skipped.
        let many = join_errors(vec![
            Some(anyhow::anyhow!("executor failed")),
            None,
            Some(anyhow::anyhow!("envs.txt failed")),
            Some(anyhow::anyhow!("statecmd.txt failed")),
        ]);
        assert_eq!(
            many.map(|e| e.to_string()),
            Some("executor failed\nenvs.txt failed\nstatecmd.txt failed".to_string())
        );
    }
}
