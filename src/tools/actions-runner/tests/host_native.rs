//! Native execution evidence using act's unchanged environment-file workflow.
use std::{collections::BTreeMap, fs, sync::Arc};

use ctox_actions_runner::{
    common::context::CollectingSink,
    container::{env_file::parse_env_text, HostEnvironment},
    expr::DefaultStatus,
    host::{prepare_run_step, HostGapKind, HostWorkflow},
    model::{GitLookups, StepResult, StepStatus},
    runner::{run_context::RunContext, step::setup_env, step_executor::file_command_env},
};

fn git() -> GitLookups {
    GitLookups::new(
        |_| Ok("main".into()),
        |_| Ok("0123456789012345678901234567890123456789".into()),
        |_, _, _| Ok("nektos/act".into()),
    )
}

#[test]
fn real_project_workflows_parse_without_being_misreported_as_executable() {
    for (name, source) in [
        ("ctox", include_str!("fixtures/workflows/ctox-ci.yml")),
        ("workjet", include_str!("fixtures/workflows/workjet-ci.yml")),
        ("greppy", include_str!("fixtures/workflows/greppy-ci.yml")),
    ] {
        let parsed = HostWorkflow::parse(name, source).unwrap();
        assert!(!parsed.workflow.jobs.is_empty(), "{name}");
        assert!(parsed.gaps.iter().any(|gap| gap.kind == HostGapKind::ActionLoader), "{name}");
        assert!(parsed.require_no_gaps().is_err(), "{name}: missing actions must not be skipped");
        for id in parsed.workflow.jobs.keys() {
            let run = parsed.run(id).unwrap();
            run.job().unwrap().get_matrixes(run.document()).unwrap();
        }
    }
}

#[test]
fn forbidden_container_features_fail_before_any_native_execution() {
    for (body, kind) in [
        ("container: ubuntu:latest", HostGapKind::Container),
        ("container: {}", HostGapKind::Container),
        ("services: {}", HostGapKind::Services),
        ("services:\n      db:\n        image: postgres", HostGapKind::Services),
        ("steps:\n      - uses: docker://alpine:latest", HostGapKind::DockerAction),
    ] {
        let yaml = format!("jobs:\n  build:\n    runs-on: ubuntu-latest\n    {body}\n");
        let parsed = HostWorkflow::parse("forbidden.yml", &yaml).unwrap();
        assert!(parsed.gaps.iter().any(|gap| gap.kind == kind), "{body}");
        assert!(parsed.require_no_gaps().is_err(), "{body}");
    }
}

#[test]
fn scheduler_and_action_gaps_are_explicit() {
    let parsed = HostWorkflow::parse("pending.yml", r#"
concurrency: project-main
jobs:
  build:
    uses: ./.github/workflows/reusable.yml
  native:
    runs-on: ubuntu-latest
    concurrency: project-build
    steps:
      - uses: ./composite
"#).unwrap();
    for kind in [HostGapKind::Concurrency, HostGapKind::ReusableWorkflow, HostGapKind::ActionLoader] {
        assert!(parsed.gaps.iter().any(|gap| gap.kind == kind));
    }
    assert!(parsed.run("absent").is_err());
}

/// This is a bounded test driver, not the production job orchestrator.
/// It binds each step's files to the job temp root and feeds the port's own
/// env/output/path parsers. Future service limits must surround this driver.
#[cfg(unix)]
#[test]
fn unchanged_act_environment_file_workflow_runs_natively() {
    let parsed = HostWorkflow::parse("environment-files.yml",
        include_str!("fixtures/workflows/pkg_runner_testdata_environment-files_push.yaml")).unwrap();
    parsed.require_no_gaps().unwrap();
    let root = tempfile::tempdir().unwrap();
    let work = root.path().join("worktree");
    let tmp = root.path().join("tmp");
    let home = root.path().join("home");
    let runner = tmp.join("runner");
    for path in [&work, &tmp, &home, &runner.join("workflow")] {
        fs::create_dir_all(path).unwrap();
    }
    let sink = Arc::new(CollectingSink::new());
    let mut host = HostEnvironment::new(work.clone(), tmp.clone(), root.path().join("tools"),
        work.to_str().unwrap());
    host.act_path = runner.clone();
    host.replace_log_writer(sink.clone());

    let run = parsed.run("build").unwrap();
    let steps = run.job().unwrap().steps.clone();
    let mut rc = RunContext { run: Some(run), ..Default::default() };
    rc.config.workdir = work.to_str().unwrap().into();
    rc.config.env = BTreeMap::from([
        ("PATH".into(), std::env::var("PATH").unwrap()),
        ("HOME".into(), home.to_str().unwrap().into()),
        ("TMPDIR".into(), tmp.to_str().unwrap().into()),
    ]);

    for (index, mut step) in steps.into_iter().enumerate() {
        if step.id.is_empty() { step.id = format!("step-{index}"); }
        rc.current_step = step.id.clone();
        let github = rc.get_github_context(&git()).unwrap();
        rc.env.extend(rc.global_env.clone());
        let mut env = BTreeMap::new();
        setup_env(&mut rc, &git(), &DefaultStatus, &mut env, &step, &github).unwrap();
        rc.apply_extra_path(&host, &mut env);
        for (key, value) in file_command_env(runner.to_str().unwrap()) {
            fs::write(&value, "").unwrap();
            env.insert(key, value);
        }
        let prepared = prepare_run_step(&mut rc, &git(), &DefaultStatus, &mut step, &env, &host).unwrap();
        let script = runner.join(&prepared.script.name);
        fs::write(script, &prepared.script.script).unwrap();
        host.exec(&prepared.script.cmd, &env, &prepared.working_directory).unwrap_or_else(|error|
            panic!("step {index} {}: {error}; logs: {:?}", step.name, sink.lines()));
        parse_env_text(&fs::read_to_string(&env["GITHUB_ENV"]).unwrap(), &mut rc.global_env).unwrap();
        let mut outputs = BTreeMap::new();
        parse_env_text(&fs::read_to_string(&env["GITHUB_OUTPUT"]).unwrap(), &mut outputs).unwrap();
        rc.step_results.insert(step.id, StepResult {
            outcome: StepStatus::Success, conclusion: StepStatus::Success, outputs,
        });
        for path in fs::read_to_string(&env["GITHUB_PATH"]).unwrap().lines() {
            rc.extra_path.insert(0, path.to_string());
        }
    }
    assert_eq!(rc.global_env["KEY3"], "value3");
    assert_eq!(rc.step_results["write-multi-output"].outputs["KEY2"], "value2");
}

#[cfg(unix)]
#[test]
fn native_exit_and_output_are_observable() {
    let root = tempfile::tempdir().unwrap();
    let mut host = HostEnvironment::new(root.path().into(), root.path().into(),
        root.path().into(), root.path().to_str().unwrap());
    let sink = Arc::new(CollectingSink::new());
    host.replace_log_writer(sink.clone());
    let env = BTreeMap::from([("PATH".into(), std::env::var("PATH").unwrap())]);
    let error = host.exec(&["sh".into(), "-c".into(), "echo stdout; echo stderr >&2; exit 23".into()],
        &env, "").unwrap_err();
    assert!(error.to_string().contains("23"));
    let messages: Vec<_> = sink.lines().into_iter().map(|(_, text)| text).collect();
    assert!(messages.iter().any(|line| line == "stdout"));
    assert!(messages.iter().any(|line| line == "stderr"));
}

#[test]
fn preparing_a_step_cannot_bypass_a_job_container_declaration() {
    let parsed = HostWorkflow::parse("forbidden.yml", r#"
jobs:
  build:
    runs-on: ubuntu-latest
    container: ubuntu:latest
    steps:
      - id: check
        run: echo forbidden
"#).unwrap();
    let run = parsed.run("build").unwrap();
    let mut step = run.job().unwrap().steps[0].clone();
    let mut rc = RunContext { run: Some(run), ..Default::default() };
    let root = tempfile::tempdir().unwrap();
    let host = HostEnvironment::new(root.path().into(), root.path().into(),
        root.path().into(), root.path().to_str().unwrap());
    assert!(prepare_run_step(&mut rc, &git(), &DefaultStatus, &mut step, &BTreeMap::new(), &host).is_err());
    assert!(rc.job_container.is_none(), "reject before mutating the backend");
}

#[cfg(unix)]
#[test]
fn prepared_script_runs_with_a_quoted_build_root() {
    let root = tempfile::Builder::new().prefix("workjet native 'root ").tempdir().unwrap();
    let mut host = HostEnvironment::new(root.path().into(), root.path().into(),
        root.path().into(), root.path().to_str().unwrap());
    host.act_path = root.path().join("runner");
    fs::create_dir_all(host.act_path.join("workflow")).unwrap();
    let parsed = HostWorkflow::parse("smoke.yml", r#"
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - id: check
        shell: bash
        run: printf '%s' "$VALUE" > result.txt
"#).unwrap();
    let run = parsed.run("build").unwrap();
    let mut step = run.job().unwrap().steps[0].clone();
    let mut rc = RunContext { run: Some(run), ..Default::default() };
    let env = BTreeMap::from([
        ("PATH".into(), std::env::var("PATH").unwrap()),
        ("VALUE".into(), "spaces 'quotes' $literal".into()),
    ]);
    let prepared = prepare_run_step(&mut rc, &git(), &DefaultStatus, &mut step, &env, &host).unwrap();
    fs::write(host.act_path.join(&prepared.script.name), &prepared.script.script).unwrap();
    host.exec(&prepared.script.cmd, &env, &prepared.working_directory).unwrap();
    assert_eq!(fs::read_to_string(root.path().join("result.txt")).unwrap(), env["VALUE"]);
}

#[test]
fn native_preparation_rejects_broken_or_unbound_expressions() {
    for body in ["echo ${{ ( }}", "echo ${{ hashFiles('Cargo.lock') }}"] {
        let yaml = format!("jobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - id: check\n        shell: bash\n        run: {body}\n");
        let parsed = HostWorkflow::parse("expression.yml", &yaml).unwrap();
        let run = parsed.run("build").unwrap();
        let mut step = run.job().unwrap().steps[0].clone();
        let mut rc = RunContext { run: Some(run), ..Default::default() };
        let root = tempfile::tempdir().unwrap();
        let host = HostEnvironment::new(root.path().into(), root.path().into(),
            root.path().into(), root.path().to_str().unwrap());
        let result = prepare_run_step(&mut rc, &git(), &DefaultStatus, &mut step, &BTreeMap::new(), &host);
        assert!(result.is_err(), "{body}: expression errors must not become successful empty scripts");
        assert!(!host.act_path.join("workflow/check.sh").exists());
    }
}
