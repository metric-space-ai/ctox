//! Port of act's `pkg/runner/step_run.go` — a `run:` step.
//!
//! A `run:` step is the simplest of the four step types, and the only one
//! whose whole job is to turn a block of text into a file plus an argv. What
//! makes it worth its own file is that three separate chains decide the answer,
//! and each chain has a fallback level that escapes the treatment the level
//! above it receives.
//!
//! | act | here |
//! |---|---|
//! | `setupShell` | [`setup_shell`] |
//! | `setupWorkingDirectory` | [`setup_working_directory`] |
//! | `setupShellCommand` | [`setup_shell_command`] and [`ScriptParts`] |
//! | `getScriptName` | [`get_script_name`] |
//! | `localEnv` | [`LocalEnv`] |
//!
//! # What is deliberately not here
//!
//! `pre`, `main`, `post` and `setupShellCommandExecutor`. They exec in the job
//! container and copy the assembled script into it, and both arrive with
//! [`super::run_context::RunContext`] Teil 3 together with
//! [`super::step_executor`]. Named so the gap is a list.
//!
//! # The three chains, and the level that escapes
//!
//! | what | step's own | job's `defaults.run` | workflow's `defaults.run` | then |
//! |---|---|---|---|---|
//! | shell | interpolated | **interpolated** | **not** interpolated | host or container default |
//! | working directory | interpolated | **interpolated** | **not** interpolated | — |
//!
//! The third level is reached only when the second came out empty, and by then
//! the interpolation has already happened. So `defaults.run.shell: ${{ env.X }}`
//! at workflow level stays a literal, while the same line at job level resolves.
//! That is upstream's shape, it falls out of the order of the statements, and
//! it is the kind of thing a reader assumes is symmetrical until it is measured.
//!
//! # `setupShell` runs *after* `setupEnv`
//!
//! `runStepExecutor` calls `setupEnv` and only then runs the step's pipeline,
//! whose first element is `setupShellCommandExecutor`. So the environment
//! `setupShell` reads has already been through [`super::step::setup_env`] — and
//! that is why its `add-path` handling can see anything: `ApplyExtraPath` has
//! nothing to prepend until some earlier step called `add-path`.

use std::collections::BTreeMap;

use crate::container::ExecutionsEnvironment;
use crate::model::{GitLookups, Step as StepModel};
use crate::runner::run_context::RunContext;

/// `localEnv`: the environment `LookPath2` resolves against.
///
/// # The one behaviour here is Windows-only, and that is the point
///
/// `Getenv` folds case on Windows and does not elsewhere. That is not
/// decoration: on Windows a process environment routinely holds `Path` rather
/// than `PATH`, and `lookpath` asks for `PATH` — so a case-sensitive lookup
/// finds nothing, act concludes `bash` is not installed, and the step runs
/// under a different shell than the one the machine actually has.
///
/// The case fold is therefore `#[cfg(windows)]` rather than unconditional. An
/// unconditional fold would make a Unix `PATH`/`path` collision behave like
/// Windows, and the two platforms are the reason this is a port rather than a
/// translation.
#[derive(Debug, Clone, Default)]
pub struct LocalEnv {
    env: BTreeMap<String, String>,
}

impl LocalEnv {
    /// A lookup environment holding `env`.
    pub fn new(env: BTreeMap<String, String>) -> Self {
        Self { env }
    }

    /// The map itself, for a caller that needs to read or extend it.
    pub fn env(&self) -> &BTreeMap<String, String> {
        &self.env
    }
}

impl crate::lookpath::Env for LocalEnv {
    fn getenv(&self, name: &str) -> Option<String> {
        #[cfg(windows)]
        {
            // `strings.EqualFold`, so the first key in **sorted** order that
            // folds equal wins. Go ranges over a map and takes whichever it
            // reaches first, which is not a defined one; the sorted rule is
            // stated rather than incidental, exactly as in `mergeIntoMap`.
            self.env
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.clone())
        }
        #[cfg(not(windows))]
        {
            self.env.get(name).cloned()
        }
    }
}

/// The three things a shell name adds to a script.
///
/// Measured on v0.2.89 with the body fixed at `cmd`; `suffix` is what is
/// appended to the script's file name, and `prepend`/`append` wrap the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptParts {
    /// The file-name suffix.
    pub suffix: &'static str,
    /// Written before the body, or empty.
    pub prepend: &'static str,
    /// Written after the body, or empty.
    pub append: &'static str,
}

const NONE: ScriptParts = ScriptParts {
    suffix: "",
    prepend: "",
    append: "",
};

/// The `switch step.Shell` in `setupShellCommand`, as data.
///
/// # The match is exact, and being exact is load-bearing
///
/// Three of the measured rows match nothing at all, and one of them is
/// ordinary-looking:
///
/// | `step.Shell` | suffix | prepend | append |
/// |---|---|---|---|
/// | `bash`, `sh` | `.sh` | | |
/// | `pwsh`, `powershell` | `.ps1` | `$ErrorActionPreference = 'stop'` | `if ((Test-Path -LiteralPath variable:/LASTEXITCODE)) { exit $LASTEXITCODE }` |
/// | `cmd` | `.cmd` | `@echo off` | |
/// | `python` | `.py` | | |
/// | `""` | | | |
/// | `BASH` | | | |
/// | `bash ` (trailing space) | | | |
///
/// `BASH` and `bash ` get **nothing** — no suffix, no `$ErrorActionPreference`,
/// no exit-code propagation. `setupShell` interpolates `WorkflowShell` before
/// anything looks at `Shell`, so a trailing space survives to this switch and
/// a workflow that wrote `shell: "${{ inputs.shell }}"` with a stray space gets
/// an unwrapped script. Go's `switch` is exact and case-sensitive; a
/// case-insensitive or trimmed match would be a quiet improvement act never
/// asked for, and it would change which scripts carry an exit code.
pub fn script_parts(shell: &str) -> ScriptParts {
    match shell {
        "bash" | "sh" => ScriptParts {
            suffix: ".sh",
            ..NONE
        },
        "pwsh" | "powershell" => ScriptParts {
            suffix: ".ps1",
            prepend: "$ErrorActionPreference = 'stop'",
            append: "if ((Test-Path -LiteralPath variable:/LASTEXITCODE)) { exit $LASTEXITCODE }",
        },
        "cmd" => ScriptParts {
            suffix: ".cmd",
            prepend: "@echo off",
            ..NONE
        },
        "python" => ScriptParts {
            suffix: ".py",
            ..NONE
        },
        _ => NONE,
    }
}

/// The two halves of `setupShellCommand`'s script assembly.
///
/// The body is `fmt.Sprintf("%s\n%s\n%s", prepend, script, append)`, so it
/// **always** begins with a newline and always ends with one — for the shells
/// that contribute neither a prepend nor an append the result is `"\ncmd\n"`,
/// and for `pwsh` there is no trailing newline after the exit-code line. Both
/// are reproduced; neither is tidied.
pub fn assemble_script(shell: &str, script: &str) -> (String, String) {
    let parts = script_parts(shell);
    (
        parts.suffix.to_string(),
        format!("{}\n{}\n{}", parts.prepend, script, parts.append),
    )
}

/// `strings.Replace(scCmd, "{0}", scriptPath, 1)` — the **first** `{0}` only.
///
/// Measured: `"bash -e {0} {0}"` with `/p` becomes `"bash -e /p {0}"`, and the
/// second placeholder is left as literal text in the argv. `ShellCommand` never
/// produces two, so this is a property of the operation rather than of any
/// current caller.
pub fn substitute_script_path(command: &str, script_path: &str) -> String {
    command.replacen("{0}", script_path, 1)
}

/// `getScriptName`: where the script lands, and what two composite steps would
/// share if they did not.
///
/// # The parent chain wraps from the inside out
///
/// Each step up the chain puts `<parent's current step>-composite-` **in front**
/// of what is already there, so the *innermost* parent ends up in the middle of
/// the name and the root ends up at the front:
///
/// | step id | chain (innermost first) | name |
/// |---|---|---|
/// | `1` | — | `workflow/1` |
/// | `1` | `build` | `workflow/build-composite-1` |
/// | `1` | `outer`, `build` | `workflow/build-composite-outer-composite-1` |
/// | `x` | `c1`, `c2`, `c3` | `workflow/c3-composite-c2-composite-c1-composite-x` |
///
/// Two measured rows show the parts are concatenated without any tidying: an
/// empty step id gives `workflow/p-composite-` with a trailing dash, and an
/// empty parent gives `workflow/-composite-1` with a leading one. A composite
/// action with no `id:` really does produce a script name ending in a dash.
///
/// The name is a **container** path (`workflow/…`, slash-separated, relative to
/// the act directory), which is why it is joined with [`crate::gopath`] and not
/// with `std::path` — the same trap [`super::step::symlink_join`] walks into.
pub fn get_script_name(rc: &RunContext, step: &StepModel) -> String {
    let mut script_name = step.id.clone();
    let mut parent = rc.caller.as_deref();
    while let Some(caller) = parent {
        script_name = format!("{}-composite-{}", caller.run_context.current_step, script_name);
        parent = caller.run_context.caller.as_deref();
    }
    format!("workflow/{script_name}")
}

/// The two shells a host environment falls back to, in probe order.
///
/// Upstream builds this list with a `runtime.GOOS` test and takes `[0]`,
/// downgrading to `[1]` if `LookPath2` does not find the first. The comment
/// beside it says why the list differs: "Don't use bash on windows by default,
/// if not using a docker container" — a Windows host has no `bash` unless the
/// user put one there, and asking costs one `LookPath2`.
pub fn host_shell_candidates() -> (&'static str, &'static str) {
    if cfg!(windows) {
        ("pwsh", "powershell")
    } else {
        ("bash", "sh")
    }
}

/// `setupShell`: decide which shell this step runs under, and write it into
/// the step model.
///
/// The chain, in upstream's order, is: the step's own `shell:`, else the job's
/// `defaults.run.shell`; then interpolate whatever that produced; if it is
/// still empty, the workflow's `defaults.run.shell`; if it is *still* empty,
/// the platform default. See the module docs for why the third level escapes
/// interpolation.
///
/// The platform default has two branches and they are decided by **which kind
/// of container there is**, not by the operating system:
///
/// * a **host environment** — the `--container=false` / self-hosted case —
///   takes [`host_shell_candidates`]'s first entry, and downgrades to the second
///   if that shell is not on the `PATH` after `add-path` entries are applied.
///   The probe is real: act asks the host whether it has it.
/// * a **container with an image** takes `sh` unconditionally, with the
///   comment "Currently only linux containers are supported".
/// * neither leaves `shell` as the empty string, which then produces a
///   `workflow/<id>` script with no suffix and a `bash -e {0}` argv — the
///   top-level-keys case upstream's long `TODO` in `setupShellCommand` is about.
///
/// `container` is the environment used for the `add-path` probe, and is only
/// read when the step has extra paths to apply: `ApplyExtraPath` returns before
/// touching the container when `ExtraPath` is empty, so in the ordinary case
/// this is `None` and nothing is dereferenced.
pub fn setup_shell(
    rc: &mut RunContext,
    git: &GitLookups,
    status: &dyn crate::expr::StatusProvider,
    step: &mut StepModel,
    step_env: &BTreeMap<String, String>,
    container: Option<&dyn ExecutionsEnvironment>,
) -> Result<(), String> {
    let run_env = rc.get_env();
    let github = rc.get_github_context(git).unwrap_or_default();
    let environment =
        super::expression::new_expression_evaluator_with_env(rc, &run_env, &github);
    let interpolate = |value: &str| {
        super::expression::interpolate(
            &environment,
            status,
            crate::expr::EvaluationContext::Job,
            value,
        )
        .unwrap_or_default()
    };

    if step.shell.is_empty() {
        step.workflow_shell = rc
            .run
            .as_ref()
            .and_then(|run| run.job())
            .map(|job| job.defaults.run.shell.clone())
            .unwrap_or_default();
    } else {
        step.workflow_shell = step.shell.clone();
    }

    step.workflow_shell = interpolate(&step.workflow_shell);

    if step.workflow_shell.is_empty() {
        step.workflow_shell = rc
            .run
            .as_ref()
            .map(|run| run.workflow.defaults.run.shell.clone())
            .unwrap_or_default();
    }

    if step.workflow_shell.is_empty() {
        if rc.job_container.is_some() {
            let (first, second) = host_shell_candidates();
            step.shell = first.to_string();
            let mut local = LocalEnv::new(step_env.clone());
            match container {
                Some(container) => rc.apply_extra_path(container, &mut local.env),
                None if !rc.extra_path.is_empty() => {
                    // `ApplyExtraPath` dereferences `rc.JobContainer`
                    // unconditionally once `ExtraPath` is non-empty, so
                    // upstream panics here and this reports it instead.
                    return Err(format!(
                        "add-path entries are pending ({}) but no job container was given to apply them to",
                        rc.extra_path.len()
                    ));
                }
                None => {}
            }
            if crate::lookpath::look_path_in(first, &local).is_err() {
                step.shell = second.to_string();
            }
        } else if !rc.container_image(&environment, status).is_empty() {
            step.shell = "sh".to_string();
        }
    } else {
        step.shell = step.workflow_shell.clone();
    }
    Ok(())
}

/// `setupWorkingDirectory`: the directory the step's `exec` runs in.
///
/// The same chain as the shell and the same asymmetry: the step's own
/// `working-directory:` and the job's `defaults.run.working-directory` are
/// interpolated, the workflow's is not, because it is only read once the
/// interpolated job-level value has come out empty.
///
/// An empty result is **not** an error. It is what a top-level `working-
/// directory:` gives, and upstream's own comment says so: "but top level keys in
/// workflow file like `defaults` or `env` can't" be interpolated.
pub fn setup_working_directory(
    rc: &mut RunContext,
    git: &GitLookups,
    status: &dyn crate::expr::StatusProvider,
    step: &StepModel,
) -> String {
    let run_env = rc.get_env();
    let github = rc.get_github_context(git).unwrap_or_default();
    let environment =
        super::expression::new_expression_evaluator_with_env(rc, &run_env, &github);

    let working_directory = if step.working_directory.is_empty() {
        rc.run
            .as_ref()
            .and_then(|run| run.job())
            .map(|job| job.defaults.run.working_directory.clone())
            .unwrap_or_default()
    } else {
        step.working_directory.clone()
    };

    let interpolated = super::expression::interpolate(
        &environment,
        status,
        crate::expr::EvaluationContext::Job,
        &working_directory,
    )
    .unwrap_or_default();

    if interpolated.is_empty() {
        rc.run
            .as_ref()
            .map(|run| run.workflow.defaults.run.working_directory.clone())
            .unwrap_or_default()
    } else {
        interpolated
    }
}

/// What [`setup_shell_command`] produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssembledScript {
    /// The file name inside the act directory, `workflow/<id><suffix>`.
    pub name: String,
    /// The file's body, prepend and append included.
    pub script: String,
    /// The command line, with the script's full path substituted for `{0}`.
    pub cmdline: String,
    /// Shell template split into argv, then the script path inserted as data.
    pub cmd: Vec<String>,
}

/// `setupShellCommand`: the body of `setupShellCommandExecutor`, minus the copy
/// into the container.
///
/// The order is upstream's and each step feeds the next: resolve the shell,
/// resolve the working directory, interpolate the `run:` body, name the script,
/// wrap the body according to the shell, substitute the path into
/// `ShellCommand`'s template and split it.
///
/// Only malformed shell-template syntax makes argv splitting fail. Unlike the
/// upstream string-based split, native script paths (including spaces and quotes)
/// are inserted after parsing so they stay a single argv element.
pub fn setup_shell_command(
    rc: &mut RunContext,
    git: &GitLookups,
    status: &dyn crate::expr::StatusProvider,
    step: &mut StepModel,
    step_env: &BTreeMap<String, String>,
    container: Option<&dyn ExecutionsEnvironment>,
) -> Result<(AssembledScript, String), String> {
    setup_shell(rc, git, status, step, step_env, container)?;
    let working_directory = setup_working_directory(rc, git, status, step);

    let run_env = rc.get_env();
    let github = rc.get_github_context(git).unwrap_or_default();
    let environment =
        super::expression::new_expression_evaluator_with_env(rc, &run_env, &github);
    let script = super::expression::interpolate(
        &environment,
        status,
        crate::expr::EvaluationContext::Job,
        &step.run,
    )
    .map_err(|error| format!("run expression: {error}"))?;

    let sc_cmd = step.shell_command();
    let (suffix, script) = assemble_script(&step.shell, &script);
    let name = format!("{}{}", get_script_name(rc, step), suffix);

    let act_path = rc
        .job_container
        .as_ref()
        .map(|paths| paths.act_path.clone())
        .unwrap_or_default();
    // `fmt.Sprintf("%s/%s", ...)`, and deliberately not `path.Join`: a join
    // would collapse a trailing slash on the act path, and the one case that
    // matters is the scratch path, whose shape this port does not control.
    // Concatenating a separator is what upstream does and is what the script
    // path in the log line shows.
    let script_path = format!("{act_path}/{name}");
    let cmdline = substitute_script_path(&sc_cmd, &script_path);
    // Split the shell template before inserting a filesystem path. Otherwise
    // spaces or quotes in a native build root become extra argv elements.
    let mut replaced = false;
    let mut cmd = crate::container::shell_quote::split(&sc_cmd).map_err(|error| {
        format!("{error}")
    })?;
    for arg in &mut cmd {
        if !replaced && arg.contains("{0}") {
            *arg = substitute_script_path(arg, &script_path);
            replaced = true;
        }
    }

    Ok((
        AssembledScript {
            name,
            script,
            cmdline,
            cmd,
        },
        working_directory,
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        assemble_script, get_script_name, host_shell_candidates, script_parts,
        setup_shell, setup_shell_command, setup_working_directory, substitute_script_path,
        LocalEnv,
    };
    use crate::expr::DefaultStatus;
    use crate::lookpath::Env as _;
    use crate::model::{Run, Workflow};
    use crate::runner::run_context::{Caller, ContainerPaths, RunConfig, RunContext};
    use crate::yaml_node::Document;
    use std::collections::BTreeMap;
    use std::rc::Rc;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect()
    }

    /// No repository, so no test here reads the working directory the way
    /// `getGithubContext` otherwise would.
    fn no_git() -> crate::model::GitLookups {
        crate::model::GitLookups::new(
            |_| Err(anyhow::anyhow!("not a repository")),
            |_| Err(anyhow::anyhow!("not a repository")),
            |_, _, _| Err(anyhow::anyhow!("not a repository")),
        )
    }

    /// A run context over one workflow, with `workflow_extra` inlined at the
    /// top level and `job_extra` under the single job.
    fn fixture(workflow_extra: &str, job_extra: &str) -> RunContext {
        let margin = job_extra.lines().filter(|line| !line.trim().is_empty())
            .map(|line| line.bytes().take_while(|byte| *byte == b' ').count())
            .min().unwrap_or(0);
        let source = format!(
            "{workflow_extra}jobs:\n  one:\n    runs-on: ubuntu-latest\n{job_extra}",
            job_extra = job_extra.lines()
                .map(|line| format!("    {}\n", &line[margin.min(line.len())..]))
                .collect::<String>()
        );
        let doc = Rc::new(Document::parse(&source).expect("the fixture parses"));
        let workflow = Workflow::from_document("test.yml", &doc).expect("decodes");
        RunContext {
            config: RunConfig::default(),
            run: Some(Run::new(workflow, doc, "one")),
            env: map(&[("FROM_RUN", "run-value")]),
            ..RunContext::default()
        }
    }

    fn step_model(rc: &RunContext) -> crate::model::Step {
        rc.run
            .as_ref()
            .and_then(|run| run.job())
            .and_then(|job| job.steps.first())
            .cloned()
            .unwrap_or_default()
    }

    // ------------------------------------------------------- scriptParts --

    /// Every row measured on v0.2.89.
    ///
    /// The three empty rows at the bottom are the point of the table. `BASH` and
    /// `bash ` are values a workflow can actually produce — the second one via
    /// `shell: "${{ inputs.shell }}"` with a stray space — and both silently
    /// get a script with no suffix, no `$ErrorActionPreference` and, for pwsh,
    /// no exit-code propagation. Go's `switch` is exact; a case-insensitive or
    /// trimmed match here would be an improvement act never asked for.
    #[test]
    fn script_parts_reproduces_every_measured_row() {
        for (shell, want) in [
            (
                "",
                super::ScriptParts {
                    suffix: "",
                    prepend: "",
                    append: "",
                },
            ),
            (
                "bash",
                super::ScriptParts {
                    suffix: ".sh",
                    prepend: "",
                    append: "",
                },
            ),
            (
                "sh",
                super::ScriptParts {
                    suffix: ".sh",
                    prepend: "",
                    append: "",
                },
            ),
            (
                "pwsh",
                super::ScriptParts {
                    suffix: ".ps1",
                    prepend: "$ErrorActionPreference = 'stop'",
                    append: "if ((Test-Path -LiteralPath variable:/LASTEXITCODE)) { exit $LASTEXITCODE }",
                },
            ),
            (
                "powershell",
                super::ScriptParts {
                    suffix: ".ps1",
                    prepend: "$ErrorActionPreference = 'stop'",
                    append: "if ((Test-Path -LiteralPath variable:/LASTEXITCODE)) { exit $LASTEXITCODE }",
                },
            ),
            (
                "cmd",
                super::ScriptParts {
                    suffix: ".cmd",
                    prepend: "@echo off",
                    append: "",
                },
            ),
            (
                "python",
                super::ScriptParts {
                    suffix: ".py",
                    prepend: "",
                    append: "",
                },
            ),
            (
                "node",
                super::ScriptParts {
                    suffix: "",
                    prepend: "",
                    append: "",
                },
            ),
            (
                "BASH",
                super::ScriptParts {
                    suffix: "",
                    prepend: "",
                    append: "",
                },
            ),
            (
                "bash ",
                super::ScriptParts {
                    suffix: "",
                    prepend: "",
                    append: "",
                },
            ),
        ] {
            assert_eq!(script_parts(shell), want, "shell {shell:?}");
        }
    }

    /// The body is `Sprintf("%s\n%s\n%s", prepend, script, append)`, so it
    /// always opens with a newline — and for pwsh it does **not** close with
    /// one, because the append is the last thing and carries none.
    ///
    /// Both measured, both reproduced. The leading newline is the kind of thing
    /// a tidy implementation removes, and removing it changes the first line of
    /// every script act writes.
    #[test]
    fn the_body_is_prepend_newline_script_newline_append() {
        for (shell, want_suffix, want_body) in [
            ("bash", ".sh", "\ncmd\n"),
            ("cmd", ".cmd", "@echo off\ncmd\n"),
            (
                "pwsh",
                ".ps1",
                "$ErrorActionPreference = 'stop'\ncmd\nif ((Test-Path -LiteralPath variable:/LASTEXITCODE)) { exit $LASTEXITCODE }",
            ),
            ("node", "", "\ncmd\n"),
        ] {
            let (suffix, body) = assemble_script(shell, "cmd");
            assert_eq!(suffix, want_suffix, "shell {shell:?}");
            assert_eq!(body, want_body, "shell {shell:?}");
        }
    }

    // ------------------------------------------------------ getScriptName --

    /// Every row measured on v0.2.89, with the parent chain innermost first.
    ///
    /// The two ragged rows are the ones worth having: an empty step id gives a
    /// name ending in a dash, an empty parent gives one starting with a dash.
    /// A composite action without an `id:` is a real workflow, so the first of
    /// those is reachable and the script is written under a name with a trailing
    /// `-` in it.
    #[test]
    fn get_script_name_reproduces_every_measured_row() {
        for (id, parents, want) in [
            ("1", vec![], "workflow/1"),
            ("1", vec!["build"], "workflow/build-composite-1"),
            (
                "1",
                vec!["outer", "build"],
                "workflow/build-composite-outer-composite-1",
            ),
            ("step id", vec!["a b"], "workflow/a b-composite-step id"),
            ("", vec!["p"], "workflow/p-composite-"),
            ("1", vec![""], "workflow/-composite-1"),
            (
                "x",
                vec!["c1", "c2", "c3"],
                "workflow/c3-composite-c2-composite-c1-composite-x",
            ),
        ] {
            let rc = chained_context(&parents);
            let step = crate::model::Step {
                id: id.to_string(),
                ..crate::model::Step::default()
            };
            assert_eq!(
                get_script_name(&rc, &step),
                want,
                "id {id:?} chain {parents:?}"
            );
        }
    }

    /// A run context whose caller chain holds steps, innermost first.
    fn chained_context(steps: &[&str]) -> RunContext {
        let mut caller = None;
        for current in steps.iter().rev() {
            let context = Rc::new(RunContext {
                current_step: (*current).to_string(),
                caller,
                ..RunContext::default()
            });
            caller = Some(Box::new(Caller { run_context: context }));
        }
        RunContext { caller, ..RunContext::default() }
    }

    // ---------------------------------------------------------- {0} path --

    /// Measured: the substitution is the **first** occurrence only, and
    /// substituting `{0}` with `{0}` is a no-op that still counts.
    #[test]
    fn only_the_first_placeholder_is_substituted() {
        for (template, path, want) in [
            (
                "bash -e {0}",
                "/var/run/act/workflow/1.sh",
                "bash -e /var/run/act/workflow/1.sh",
            ),
            ("bash -e {0} {0}", "/p", "bash -e /p {0}"),
            ("cmd /D /C \"CALL \"{0}\"\"", "/p/1.cmd", "cmd /D /C \"CALL \"/p/1.cmd\"\""),
            ("node", "/p", "node"),
            ("{0}", "{0}", "{0}"),
        ] {
            assert_eq!(
                substitute_script_path(template, path),
                want,
                "template {template:?}"
            );
        }
    }

    // -------------------------------------------------------- setupShell --

    /// The step's own `shell:` wins, and it lands in `workflow_shell` as well
    /// as `shell` — `ShellCommand` reads the second, the log line the first.
    #[test]
    fn the_steps_own_shell_wins_and_is_copied_into_both_fields() {
        let rc = fixture("", "steps:\n  - run: echo hi\n    shell: pwsh");
        let mut step = step_model(&rc);
        setup_shell(&mut rc.clone(), &no_git(), &DefaultStatus, &mut step, &map(&[]), None)
            .expect("setup");
        assert_eq!(step.shell, "pwsh");
        assert_eq!(step.workflow_shell, "pwsh");
    }

    /// The chain, in order: step, then job `defaults.run`, then workflow
    /// `defaults.run`, then the platform default.
    #[test]
    fn the_shell_falls_through_step_then_job_then_workflow() {
        for (job_defaults, workflow_defaults, want_shell, want_workflow_shell) in [
            ("    defaults:\n      run:\n        shell: sh", "", "sh", "sh"),
            (
                "",
                "defaults:\n  run:\n    shell: python\n",
                "python",
                "python",
            ),
            ("    shell: bash\n    defaults:\n      run:\n        shell: sh", "", "sh", "sh"),
        ] {
            let rc = fixture(workflow_defaults, job_defaults);
            let mut step = step_model(&rc);
            let mut rc = rc;
            setup_shell(&mut rc, &no_git(), &DefaultStatus, &mut step, &map(&[]), None)
                .expect("setup");
            assert_eq!(step.shell, want_shell, "job {job_defaults:?}");
            assert_eq!(step.workflow_shell, want_workflow_shell, "job {job_defaults:?}");
        }
    }

    /// The asymmetry: the **job's** `defaults.run.shell` is interpolated, the
    /// **workflow's** is not, because it is only read once the interpolated
    /// job-level value has already come out empty.
    ///
    /// Not a guess about intent — it falls out of the order of upstream's
    /// statements, and the consequence is that a workflow-level default
    /// containing `${{ }}` reaches the shell as literal text.
    #[test]
    fn the_workflow_level_default_escapes_interpolation() {
        let rc = fixture(
            "defaults:\n  run:\n    shell: ${{ env.FROM_RUN }}\n",
            "    defaults:\n      run:\n        shell: ${{ env.FROM_RUN }}",
        );
        let mut step = step_model(&rc);
        let mut rc = rc;
        setup_shell(&mut rc, &no_git(), &DefaultStatus, &mut step, &map(&[]), None)
            .expect("setup");
        assert_eq!(step.workflow_shell, "run-value", "the job level is resolved");
        assert_eq!(step.shell, "run-value");

        // Same workflow, no job-level default: now the un-interpolated
        // workflow-level value is the one that survives.
        let rc = fixture(
            "defaults:\n  run:\n    shell: ${{ env.FROM_RUN }}\n",
            "",
        );
        let mut step = step_model(&rc);
        let mut rc = rc;
        setup_shell(&mut rc, &no_git(), &DefaultStatus, &mut step, &map(&[]), None)
            .expect("setup");
        assert_eq!(
            step.workflow_shell, "${{ env.FROM_RUN }}",
            "the workflow level is read after the interpolation, so it is not"
        );
        assert_eq!(step.shell, "${{ env.FROM_RUN }}");
    }

    /// A job with a `container:` gets `sh`, unconditionally — the comment
    /// upstream carries is "Currently only linux containers are supported".
    #[test]
    fn a_container_image_gives_sh() {
        let rc = fixture("", "container: node:18");
        let mut step = step_model(&rc);
        let mut rc = rc;
        setup_shell(&mut rc, &no_git(), &DefaultStatus, &mut step, &map(&[]), None)
            .expect("setup");
        assert_eq!(step.shell, "sh");
        assert_eq!(
            step.workflow_shell, "",
            "the container branch does not write workflow_shell"
        );
    }

    /// Neither a host environment nor a container image: `shell` is left as the
    /// empty string. This is the top-level-keys case that upstream's long
    /// `TODO` in `setupShellCommand` is about, and it produces a `workflow/<id>`
    /// script with no suffix and a `bash -e {0}` argv.
    #[test]
    fn neither_a_host_nor_a_container_leaves_the_shell_empty() {
        let rc = fixture("", "runs-on: ubuntu-latest");
        let mut rc = rc;
        rc.run.as_mut().expect("a run").workflow.jobs.clear();
        let mut step = crate::model::Step::default();
        setup_shell(&mut rc, &no_git(), &DefaultStatus, &mut step, &map(&[]), None)
            .expect("setup");
        assert_eq!(step.shell, "");
        assert_eq!(step.workflow_shell, "");
    }

    /// The host probe is real, so it can be made to fail: a `PATH` that does not
    /// exist means the first candidate is not installed, and the second is used.
    #[test]
    fn a_host_without_the_first_shell_falls_back_to_the_second() {
        let (first, second) = host_shell_candidates();
        let mut rc = fixture("", "");
        rc.job_container = Some(ContainerPaths {
            act_path: "/var/run/act".to_string(),
            ..ContainerPaths::default()
        });
        let mut step = crate::model::Step::default();
        let env = map(&[("PATH", "/nonexistent-probe-directory")]);
        setup_shell(&mut rc, &no_git(), &DefaultStatus, &mut step, &env, None).expect("setup");
        assert_eq!(step.shell, second, "{first} is not on the PATH");
    }

    /// And it can be made to succeed: put an executable with the first shell's
    /// name in a directory and put that directory on the `PATH`.
    ///
    /// Built in a temporary directory rather than trusting the machine to have
    /// `bash` at a known place — the assertion has to hold on a build agent that
    /// has never seen this repository.
    #[test]
    fn a_host_with_the_first_shell_keeps_it() {
        let (first, _) = host_shell_candidates();
        let dir = std::env::temp_dir().join(format!("act-step-run-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("the probe directory");
        let name = if cfg!(windows) {
            format!("{first}.exe")
        } else {
            first.to_string()
        };
        let probe = dir.join(&name);
        std::fs::write(&probe, b"").expect("the probe executable");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o755))
                .expect("executable");
        }

        let mut rc = fixture("", "");
        rc.job_container = Some(ContainerPaths {
            act_path: "/var/run/act".to_string(),
            ..ContainerPaths::default()
        });
        let mut step = crate::model::Step::default();
        let env = map(&[("PATH", dir.to_string_lossy().as_ref())]);
        setup_shell(&mut rc, &no_git(), &DefaultStatus, &mut step, &env, None).expect("setup");

        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(step.shell, first, "{first} is on the PATH");
    }

    /// `add-path` entries pending with no container to apply them to: upstream
    /// dereferences a nil `rc.JobContainer` and panics, so this reports rather
    /// than panicking. The same deliberate deviation as `InterpolateError`.
    #[test]
    fn pending_add_path_entries_without_a_container_are_reported() {
        let mut rc = fixture("", "");
        rc.job_container = Some(ContainerPaths {
            act_path: "/var/run/act".to_string(),
            ..ContainerPaths::default()
        });
        rc.extra_path = vec!["/opt/hostedtoolcache".to_string()];
        let mut step = crate::model::Step::default();
        let error = setup_shell(
            &mut rc,
            &no_git(),
            &DefaultStatus,
            &mut step,
            &map(&[]),
            None,
        )
        .expect_err("reported, not panicked");
        assert!(
            error.contains("add-path"),
            "the message names the thing that was missing: {error}"
        );
    }

    // ---------------------------------------------- setupWorkingDirectory --

    /// The same chain and the same asymmetry as the shell.
    #[test]
    fn the_working_directory_falls_through_step_then_job_then_workflow() {
        for (step_wd, job_wd, workflow_wd, want) in [
            ("working-directory: ./a", "        working-directory: ./b", "  working-directory: ./c", "./a"),
            ("", "        working-directory: ./b", "  working-directory: ./c", "./b"),
            ("", "", "  working-directory: ./c", "./c"),
            ("", "", "", ""),
        ] {
            let job_defaults = if job_wd.is_empty() {
                String::new()
            } else {
                format!("defaults:\n  run:\n    {}\n", job_wd.trim_start())
            };
            let workflow_defaults = if workflow_wd.is_empty() {
                String::new()
            } else {
                format!("defaults:\n  run:\n    {}\n", workflow_wd.trim_start())
            };
            let step_yaml = if step_wd.is_empty() {
                "steps:\n  - run: echo hi".to_string()
            } else {
                format!("steps:\n  - run: echo hi\n    {step_wd}")
            };
            let rc = fixture(&workflow_defaults, &format!("{job_defaults}{step_yaml}\n"));
            let step = step_model(&rc);
            let mut rc = rc;
            assert_eq!(
                setup_working_directory(&mut rc, &no_git(), &DefaultStatus, &step),
                want,
                "step {step_wd:?} job {job_wd:?} workflow {workflow_wd:?}"
            );
        }
    }

    /// "but top level keys in workflow file like `defaults` or `env` can't" be
    /// interpolated — the job level is resolved, the workflow level is not.
    #[test]
    fn the_workflow_level_working_directory_escapes_interpolation() {
        let rc = fixture(
            "defaults:\n  run:\n    working-directory: ${{ env.FROM_RUN }}\n",
            "",
        );
        let step = step_model(&rc);
        let mut rc = rc;
        assert_eq!(
            setup_working_directory(&mut rc, &no_git(), &DefaultStatus, &step),
            "${{ env.FROM_RUN }}"
        );

        let rc = fixture(
            "defaults:\n  run:\n    working-directory: ${{ env.FROM_RUN }}\n",
            "    defaults:\n      run:\n        working-directory: ${{ env.FROM_RUN }}",
        );
        let step = step_model(&rc);
        let mut rc = rc;
        assert_eq!(
            setup_working_directory(&mut rc, &no_git(), &DefaultStatus, &step),
            "run-value"
        );
    }

    // ------------------------------------------------ setupShellCommand --

    /// The four things `setupShellCommand` hands to `setupShellCommandExecutor`,
    /// end to end, for the default shell.
    #[test]
    fn the_assembled_script_names_the_file_and_builds_the_argv() {
        let rc = fixture("", "steps:\n  - id: s1\n    run: echo hello\n    shell: bash");
        let mut step = step_model(&rc);
        let mut rc = rc;
        rc.job_container = Some(ContainerPaths {
            act_path: "/var/run/act".to_string(),
            ..ContainerPaths::default()
        });
        let (assembled, working_directory) =
            setup_shell_command(&mut rc, &no_git(), &DefaultStatus, &mut step, &map(&[]), None)
                .expect("assembles");
        assert_eq!(assembled.name, "workflow/s1.sh");
        assert_eq!(assembled.script, "\necho hello\n");
        assert_eq!(
            assembled.cmdline,
            "bash --noprofile --norc -e -o pipefail /var/run/act/workflow/s1.sh"
        );
        assert_eq!(assembled.cmd, vec!["bash", "--noprofile", "--norc", "-e", "-o", "pipefail", "/var/run/act/workflow/s1.sh"]);
        assert_eq!(working_directory, "");
    }

    /// A `cmd` step, because it is the one where the body and the argv disagree
    /// in the way that matters: `@echo off` goes into the *file*, and the argv
    /// keeps the doubled quoting of `ShellCommand`.
    #[test]
    fn a_cmd_step_wraps_the_body_and_keeps_the_doubled_quoting() {
        let rc = fixture("", "steps:\n  - id: s1\n    run: dir\n    shell: cmd");
        let mut step = step_model(&rc);
        let mut rc = rc;
        rc.job_container = Some(ContainerPaths {
            act_path: "/var/run/act".to_string(),
            ..ContainerPaths::default()
        });
        let (assembled, _) = setup_shell_command(
            &mut rc,
            &no_git(),
            &DefaultStatus,
            &mut step,
            &map(&[]),
            None,
        )
        .expect("assembles");
        assert_eq!(assembled.name, "workflow/s1.cmd");
        assert_eq!(assembled.script, "@echo off\ndir\n");
        assert_eq!(
            assembled.cmdline,
            "cmd /D /E:ON /V:OFF /S /C \"CALL \"/var/run/act/workflow/s1.cmd\"\""
        );
    }

    /// Native paths are argv data, including quotes and spaces.
    #[test]
    fn a_script_path_with_spaces_or_quotes_stays_one_argument() {
        for path in ["/var/run/a b", "/var/run/\"act", "/var/run/'act"] {
            let mut rc = fixture("", "steps:\n  - id: s1\n    run: echo hi\n    shell: bash");
            let mut step = step_model(&rc);
            rc.job_container = Some(ContainerPaths {
                act_path: path.to_string(),
                ..ContainerPaths::default()
            });
            let (assembled, _) = setup_shell_command(
                &mut rc, &no_git(), &DefaultStatus, &mut step, &map(&[]), None,
            ).expect("filesystem names are not shell syntax");
            assert_eq!(assembled.cmd.last().unwrap(), &format!("{path}/workflow/s1.sh"));
            assert_eq!(assembled.cmd[..6], ["bash", "--noprofile", "--norc", "-e", "-o", "pipefail"]);
            assert_eq!(assembled.cmd.len(), 7);
        }
    }

    /// Invalid shell syntax still fails; paths are never used to hide it.
    #[test]
    fn an_unbalanced_quote_in_the_shell_template_remains_an_error() {
        let mut rc = fixture("", "steps:\n  - id: s1\n    run: echo hi\n    shell: 'bash -e \"{0}'");
        let mut step = step_model(&rc);
        assert!(setup_shell_command(
            &mut rc, &no_git(), &DefaultStatus, &mut step, &map(&[]), None,
        ).is_err());
    }

    // ---------------------------------------------------------- localEnv --

    /// The case fold is the whole point of the type, and it is **Windows-only**.
    ///
    /// Both halves run: on Windows the fold must find `Path` for a `PATH`
    /// lookup, and on Unix the same lookup must miss, because a host there
    /// really does have two distinct variables.
    #[test]
    fn the_case_fold_is_windows_only() {
        let env = LocalEnv::new(map(&[("Path", "/usr/bin"), ("OTHER", "x")]));
        let found = env.getenv("PATH");
        #[cfg(windows)]
        assert_eq!(found.as_deref(), Some("/usr/bin"), "PATH finds Path");
        #[cfg(not(windows))]
        assert_eq!(found, None, "on unix PATH and Path are two variables");

        // An exact match works on both, so the test above is about the fold and
        // not about the lookup being broken.
        let exact = LocalEnv::new(map(&[("PATH", "/usr/bin")]));
        assert_eq!(exact.getenv("PATH").as_deref(), Some("/usr/bin"));
    }
}
