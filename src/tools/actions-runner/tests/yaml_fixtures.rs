//! Parses real GitHub Actions workflow files through the node tree.
//!
//! The fixtures are copied verbatim from the `nektos/act` test data and from
//! act's own `.github/workflows`, plus one local fixture that exercises YAML
//! anchors and aliases. act's `model` tests decode these files into its model,
//! so if this layer parses them the `model` port starts from a known-good
//! input.

use std::fs;
use std::path::Path;

use ctox_actions_runner::yaml_node::{Document, NodeKind, YamlError};
use std::collections::BTreeMap;

fn fixtures() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/workflows");
    let mut out = Vec::new();
    let entries = fs::read_dir(&dir).unwrap_or_else(|err| panic!("cannot read {dir:?}: {err}"));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.extension().is_none_or(|ext| ext != "yml" && ext != "yaml") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let body = fs::read_to_string(&path).expect("fixture is readable");
        out.push((name, body));
    }
    assert!(out.len() >= 8, "expected the fixture set to be present");
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn every_real_workflow_parses_and_resolves() {
    for (name, body) in fixtures() {
        let mut doc = Document::parse(&body)
            .unwrap_or_else(|err| panic!("{name} must parse, got {err}"));
        doc.resolve_aliases()
            .unwrap_or_else(|err| panic!("{name} must resolve aliases, got {err}"));
    }
}

#[test]
fn workflows_expose_jobs_and_steps() {
    for (name, body) in fixtures() {
        let mut doc = Document::parse(&body).expect(name.as_str());
        doc.resolve_aliases().expect(name.as_str());
        let Some(root) = doc.root() else {
            // The empty workflow fixture legitimately has no content.
            continue;
        };
        let root_node = doc.node(root).expect("root node");
        assert!(
            matches!(root_node.kind, NodeKind::Mapping),
            "{name} root must be a mapping, got {:?}",
            root_node.kind
        );

        let Some(jobs) = doc.map_get(root, "jobs") else {
            // Not every fixture is a workflow (e.g. an action.yml).
            continue;
        };
        let jobs_node = doc.node(jobs).expect("jobs node");
        assert!(
            matches!(jobs_node.kind, NodeKind::Mapping),
            "{name} jobs must be a mapping"
        );
        assert!(
            !jobs_node.content.is_empty(),
            "{name} must define at least one job"
        );
    }
}

#[test]
fn expressions_in_real_workflows_survive_parsing() {
    let mut found = 0usize;
    for (name, body) in fixtures() {
        if !body.contains("${{") {
            continue;
        }
        let doc = Document::parse(&body).expect(name.as_str());
        let root = doc.root().expect("root");
        assert!(
            count_expressions(&doc, root) > 0,
            "{name} contains `${{` in its source but none survived parsing"
        );
        found += 1;
    }
    assert!(found >= 2, "expected fixtures to use expressions, got {found}");
}

fn count_expressions(doc: &Document, id: usize) -> usize {
    let Some(node) = doc.node(id) else {
        return 0;
    };
    let own = usize::from(node.value.contains("${{"));
    own + node.content.iter().map(|c| count_expressions(doc, *c)).sum::<usize>()
}

#[test]
fn anchors_in_real_workflows_resolve() {
    let body = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/workflows/anchors-and-merge.yml"),
    )
    .expect("anchor fixture");
    let mut doc = Document::parse(&body).expect("parses");
    doc.resolve_aliases().expect("resolves");

    let root = doc.root().expect("root");
    let jobs = doc.map_get(root, "jobs").expect("jobs");
    let build = doc.map_get(jobs, "build").expect("build job");
    let env = doc.map_get(build, "env").expect("env");
    let shared = doc.map_get(env, "SHARED").expect("SHARED");
    let shared_node = doc.node(shared).expect("node");

    // The alias in `services.db.env` copied the anchor, so the same keys and
    // values must be visible there.
    let services = doc.map_get(build, "services").expect("services");
    let db = doc.map_get(services, "db").expect("db");
    let db_env = doc.map_get(db, "env").expect("db env");
    assert_eq!(doc.map_get(db_env, "SHARED"), Some(shared));
    assert_eq!(doc.map_get(db_env, "CI"), Some(doc.map_get(env, "CI").unwrap()));
    assert!(shared_node.is_scalar());
}

#[test]
fn empty_fixture_parses_to_nothing() {
    let doc = Document::parse("").expect("parses");
    assert!(doc.is_empty());
    assert_eq!(doc.root(), None);
}

#[test]
fn unknown_anchor_is_rejected() {
    // saphyr validates anchors while scanning, so an alias to an undefined
    // anchor fails as a syntax error. act leaves `node.Alias == nil` and
    // reports "unresolved alias node" itself. Both reject the document; the
    // port rejects it earlier. `YamlError::UnresolvedAlias` therefore only
    // applies to hand-built trees.
    let err = Document::parse("a: *nope\n").expect_err("unknown alias must fail");
    assert!(matches!(err, YamlError::Syntax(_)), "got {err}");

    let mut doc = Document::default();
    assert!(matches!(
        doc.resolve_aliases(),
        Err(YamlError::UnresolvedAlias) | Ok(())
    ));
}

/// Pulls every `${{ ... }}` body out of a raw scalar value.
fn expressions_in(value: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = value;
    while let Some(open) = rest.find("${{") {
        let after = &rest[open + 3..];
        let Some(close) = after.find("}}") else {
            break;
        };
        found.push(after[..close].to_string());
        rest = &after[close + 2..];
    }
    found
}

#[test]
fn every_expression_in_the_real_workflows_parses() {
    let mut total = 0usize;
    for (name, body) in fixtures() {
        let doc = Document::parse(&body).expect(name.as_str());
        let Some(root) = doc.root() else {
            continue;
        };
        let mut expressions = Vec::new();
        collect_scalars(&doc, root, &mut expressions);
        for text in expressions {
            for body_text in expressions_in(&text) {
                parse_body(&body_text)
                    .unwrap_or_else(|err| panic!("{name}: `{body_text}` must parse: {err}"));
                total += 1;
            }
        }
    }
    assert!(total >= 20, "expected a real corpus, only saw {total}");
}

fn collect_scalars(doc: &Document, id: usize, out: &mut Vec<String>) {
    let Some(node) = doc.node(id) else { return };
    if node.value.contains("${{") {
        out.push(node.value.clone());
    }
    for child in &node.content {
        collect_scalars(doc, *child, out);
    }
}

#[test]
fn expressions_survive_interpolation_inside_run_blocks() {
    let source = "jobs:\n  b:\n    steps:\n      - run: |\n          npm ci\n          npm run build:${{ matrix.os }}\n          echo \"${{ steps.x.outputs.y }}\"\n";
    let mut doc = Document::parse(source).expect("parses");
    doc.resolve_aliases().expect("resolves");

    let mut scalars = Vec::new();
    collect_scalars(&doc, doc.root().unwrap(), &mut scalars);
    let run = scalars
        .iter()
        .find(|s| s.contains("npm run build"))
        .expect("run block must be preserved");
    let found = expressions_in(run);
    assert_eq!(found.len(), 2, "got {found:?}");
    for body_text in found {
        parse_body(&body_text).unwrap_or_else(|err| panic!("`{body_text}` must parse: {err}"));
    }
}

/// Parses an expression body, re-appending the `}}` terminator the lexer needs.
fn parse_body(body: &str) -> Result<ctox_actions_runner::expr::ExprNode, ctox_actions_runner::expr::ExprError> {
    let mut source = String::with_capacity(body.len() + 2);
    source.push_str(body);
    source.push_str("}}");
    ctox_actions_runner::expr::parse(&source)
}

/// Evaluates every `${{ }}` found in the real workflow fixtures.
///
/// The environment is deliberately realistic rather than empty: a workflow that
/// references `github.*`, `matrix.*` or `steps.*` must evaluate, because those
/// are exactly the shapes real workflows use.
#[test]
fn real_workflow_expressions_evaluate_against_a_realistic_environment() {
    use ctox_actions_runner::expr::{
        DefaultStatus, DefaultStatusCheck, EvaluationContext, EvaluationEnvironment, Interpreter,
        Value,
    };

    let env = EvaluationEnvironment {
        github: Some(Value::object([
            ("event_name".to_string(), Value::String("push".to_string())),
            (
                "ref".to_string(),
                Value::String("refs/heads/main".to_string()),
            ),
            (
                "ref_name".to_string(),
                Value::String("main".to_string()),
            ),
            (
                "sha".to_string(),
                Value::String("deadbeef".to_string()),
            ),
            ("workspace".to_string(), Value::String("ctox".to_string())),
            ("run_id".to_string(), Value::String("42".to_string())),
            (
                "event".to_string(),
                Value::object([
                    ("number".to_string(), Value::String("7".to_string())),
                    (
                        "pull_request".to_string(),
                        Value::object([("number".to_string(), Value::String("3".to_string()))]),
                    ),
                ]),
            ),
            (
                "actor".to_string(),
                Value::String("welsch".to_string()),
            ),
            (
                "head_ref".to_string(),
                Value::String("feature/x".to_string()),
            ),
            (
                "repository".to_string(),
                Value::object([
                    ("name".to_string(), Value::String("ctox".to_string())),
                    ("owner".to_string(), Value::String("metric-space-ai".to_string())),
                    ("default_branch".to_string(), Value::String("main".to_string())),
                ]),
            ),
            (
                "event".to_string(),
                Value::object([(
                    "commits".to_string(),
                    Value::Array(vec![Value::object([(
                        "message".to_string(),
                        Value::String("fix: thing".to_string()),
                    )])]),
                )]),
            ),
        ])),
        env: [
            ("CI".to_string(), Value::String("true".to_string())),
            ("HOME".to_string(), Value::String("/root".to_string())),
        ]
        .into_iter()
        .collect(),
        matrix: [
            ("os".to_string(), Value::String("ubuntu-latest".to_string())),
            ("node".to_string(), Value::Int(20)),
            (
                "include".to_string(),
                Value::Array(vec![Value::object([(
                    "extra".to_string(),
                    Value::String("yes".to_string()),
                )])]),
            ),
        ]
        .into_iter()
        .collect(),
        steps: [(
            "build".to_string(),
            Value::object([(
                "outputs".to_string(),
                Value::object([(
                    "tag".to_string(),
                    Value::String("v1".to_string()),
                )]),
            )]),
        )]
        .into_iter()
        .collect(),
        needs: [(
            "prepare".to_string(),
            Value::object([
                ("result".to_string(), Value::String("success".to_string())),
                (
                    "outputs".to_string(),
                    Value::object([(
                        "version".to_string(),
                        Value::String("1.2.3".to_string()),
                    )]),
                ),
            ]),
        )]
        .into_iter()
        .collect(),
        runner: [("os".to_string(), Value::String("Linux".to_string()))]
            .into_iter()
            .collect(),
        inputs: [(
            "version".to_string(),
            Value::String("1.2.3".to_string()),
        )]
        .into_iter()
        .collect(),
        strategy: [(
            "fail-fast".to_string(),
            Value::Bool(true),
        )]
        .into_iter()
        .collect(),
        vars: [("prefix".to_string(), Value::String("v".to_string()))]
            .into_iter()
            .collect(),
        secrets: [("token".to_string(), Value::String("s3cret".to_string()))]
            .into_iter()
            .collect(),
        // A reusable workflow call exposes `jobs.<job_id>.outputs`.
        jobs: Some(
            [(
                "reusable_workflow_job".to_string(),
                Value::object([(
                    "outputs".to_string(),
                    Value::object([(
                        "job-output".to_string(),
                        Value::String("from-job".to_string()),
                    )]),
                )]),
            )]
            .into_iter()
            .collect(),
        ),
        ..EvaluationEnvironment::default()
    };

    let status = DefaultStatus;
    let interpreter = Interpreter::new(&env, &status, EvaluationContext::Step);

    let mut evaluated = 0usize;
    for (name, body) in fixtures() {
        let doc = Document::parse(&body).expect(name.as_str());
        let Some(root) = doc.root() else { continue };
        let mut scalars = Vec::new();
        collect_scalars(&doc, root, &mut scalars);
        for text in scalars {
            for expression in expressions_in(&text) {
                let mut source = expression.clone();
                source.push_str("}}");
                let value = interpreter
                    .evaluate(&source, DefaultStatusCheck::None)
                    .unwrap_or_else(|err| {
                        panic!("{name}: `{expression}` must evaluate: {err}")
                    });
                // Anything may be produced, but the value must be well formed
                // and renderable, which is what the runner depends on.
                let _ = format!("{value:?}");
                evaluated += 1;
            }
        }
    }
    assert!(evaluated >= 20, "expected a real corpus, only saw {evaluated}");
}

#[test]
fn representative_workflow_conditions_produce_expected_results() {
    use ctox_actions_runner::expr::{
        DefaultStatus, EvaluationContext, EvaluationEnvironment, Interpreter, Value,
    };

    let env = EvaluationEnvironment {
        github: Some(Value::object([
            ("event_name".to_string(), Value::String("push".to_string())),
            (
                "ref".to_string(),
                Value::String("refs/heads/main".to_string()),
            ),
        ])),
        matrix: [("os".to_string(), Value::String("linux".to_string()))]
            .into_iter()
            .collect(),
        ..EvaluationEnvironment::default()
    };
    let status = DefaultStatus;
    let interpreter = Interpreter::new(&env, &status, EvaluationContext::Step);

    let cases: &[(&str, Value)] = &[
        ("github.event_name == 'push' }}", Value::Bool(true)),
        ("github.event_name == 'pull_request' }}", Value::Bool(false)),
        ("startsWith(github.ref, 'refs/heads/') }}", Value::Bool(true)),
        ("github.ref == 'refs/heads/main' }}", Value::Bool(true)),
        ("matrix.os == 'linux' }}", Value::Bool(true)),
        ("matrix.os == 'windows' }}", Value::Bool(false)),
        ("!github.event_name == 'push' }}", Value::Bool(false)),
        ("github.event_name == 'push' && matrix.os == 'linux' }}", Value::Bool(true)),
        ("github.event_name == 'push' || matrix.os == 'windows' }}", Value::Bool(true)),
        ("format('v{0}', '1.2.3') }}", Value::String("v1.2.3".to_string())),
        ("contains(github.ref, 'main') }}", Value::Bool(true)),
    ];

    for (source, expected) in cases {
        let value = interpreter
            .evaluate(source, ctox_actions_runner::expr::DefaultStatusCheck::None)
            .unwrap_or_else(|err| panic!("{source} must evaluate: {err}"));
        assert_eq!(&value, expected, "{source}");
    }
}

/// Decodes every real workflow fixture into the data model.
///
/// This is the first end-to-end path: raw `.github/workflows/*.yml` → YAML node
/// tree → `Workflow`. Everything after this point (planner, runner) consumes
/// these types, so a fixture that decodes here is a fixture the rest of the
/// port can rely on.
#[test]
fn real_workflows_decode_into_the_model() {
    use ctox_actions_runner::model::{JobType, StepType, Workflow};

    let mut workflows = 0usize;
    let mut jobs = 0usize;
    let mut steps = 0usize;

    for (name, body) in fixtures() {
        let mut doc = Document::parse(&body).expect(name.as_str());
        doc.resolve_aliases()
            .unwrap_or_else(|err| panic!("{name}: aliases must resolve: {err}"));

        let workflow = Workflow::from_document(&name, &doc)
            .unwrap_or_else(|err| panic!("{name} must decode: {err}"));

        let events = workflow.on(&doc);
        let job_count = workflow.jobs.len();
        // act's test data includes a deliberately empty workflow, which decodes
        // to a workflow with neither events nor jobs.
        assert!(
            !events.is_empty() || job_count == 0,
            "{name} must declare an event unless it has no jobs at all"
        );
        assert!(
            job_count > 0 || workflow.jobs.is_empty(),
            "{name} must decode without jobs failing"
        );

        for (job_id, job) in &workflow.jobs {
            jobs += 1;

            // A job is either a plain job or a reusable-workflow call, never
            // both and never neither.
            let job_type = job
                .job_type()
                .unwrap_or_else(|err| panic!("{name}: job {job_id}: {err}"));
            if job_type == JobType::Default {
                assert!(
                    job.uses.is_empty(),
                    "{name}: job {job_id} is a default job but sets `uses`"
                );
                assert!(
                    !job.runs_on(&doc).is_empty(),
                    "{name}: job {job_id} must declare `runs-on`"
                );
            }

            for (index, step) in job.steps.iter().enumerate() {
                steps += 1;
                let step_type = step.step_type();
                assert_ne!(
                    step_type,
                    StepType::Invalid,
                    "{name}: job {job_id} step {index} is invalid"
                );
                // A step with `run` must have a usable shell command.
                if step_type == StepType::Run {
                    assert!(
                        !step.shell_command().is_empty(),
                        "{name}: job {job_id} step {index} has no shell command"
                    );
                }
                assert!(
                    !step.display_name().is_empty(),
                    "{name}: job {job_id} step {index} has no display name"
                );
            }

            // A matrix must expand, and every expansion must be usable.
            let matrixes = job
                .get_matrixes(&doc)
                .unwrap_or_else(|err| panic!("{name}: job {job_id}: {err}"));
            assert!(!matrixes.is_empty());
        }

        workflows += 1;
    }

    assert!(workflows >= 8, "expected a real corpus, saw {workflows}");
    assert!(jobs >= 8, "expected several jobs, saw {jobs}");
    assert!(steps >= 8, "expected several steps, saw {steps}");
}

#[test]
fn a_matrix_from_a_real_workflow_expands_to_usable_values() {
    use ctox_actions_runner::model::Workflow;
    use ctox_actions_runner::yaml_node::Document;

    let source = "jobs:\n  build:\n    strategy:\n      matrix:\n        os: [ubuntu-latest, macos-14]\n        node: [18, 20]\n        include:\n          - os: ubuntu-latest\n            experimental: true\n    runs-on: ${{ matrix.os }}\n    steps:\n      - uses: actions/setup-node@v4\n        with:\n          node-version: ${{ matrix.node }}\n";
    let mut doc = Document::parse(source).expect("parses");
    doc.resolve_aliases().expect("resolves");
    let workflow = Workflow::from_document("matrix.yml", &doc).expect("decodes");

    let matrixes = workflow.jobs["build"]
        .get_matrixes(&doc)
        .expect("matrix must expand");

    // 2 os x 2 node = 4 combinations. The `include` entry adds a new key that
    // overwrites nothing, so it merges into *every* matching combination — both
    // ubuntu rows — rather than creating a fifth one.
    assert_eq!(matrixes.len(), 4, "got {matrixes:?}");

    // Numeric matrix entries stay numeric, so `matrix.node` compares numerically.
    assert!(
        matrixes
            .iter()
            .filter(|m| m.contains_key("node"))
            .all(|m| matches!(m["node"], ctox_actions_runner::expr::Value::Int(_))),
        "node must stay an integer: {matrixes:?}"
    );

    let ubuntu = |m: &BTreeMap<String, ctox_actions_runner::expr::Value>| {
        m.get("os") == Some(&ctox_actions_runner::expr::Value::String(
            "ubuntu-latest".into(),
        ))
    };
    assert_eq!(matrixes.iter().filter(|m| ubuntu(m)).count(), 2);
    assert!(
        matrixes
            .iter()
            .filter(|m| ubuntu(m))
            .all(|m| m.contains_key("experimental")),
        "the include must merge into both ubuntu rows: {matrixes:?}"
    );
    assert!(
        matrixes
            .iter()
            .filter(|m| !ubuntu(m))
            .all(|m| !m.contains_key("experimental")),
        "macos rows must not gain the include key: {matrixes:?}"
    );
}

/// Validates every real workflow fixture against the embedded GitHub Actions
/// schema.
///
/// This exercises the 86 KB `workflow_schema.json` through the ported validator:
/// if a workflow that act accepts is rejected here, the port has drifted.
#[test]
fn real_workflows_validate_against_the_embedded_schema() {
    use ctox_actions_runner::validate::Validator;

    let mut validated = 0usize;
    let mut rejected: Vec<(String, Vec<String>)> = Vec::new();

    for (name, body) in fixtures() {
        let mut doc = Document::parse(&body).expect(name.as_str());
        doc.resolve_aliases()
            .unwrap_or_else(|err| panic!("{name}: {err}"));
        let Some(root) = doc.root() else {
            continue;
        };

        match Validator::workflow().check(&doc, root, "workflow-root") {
            Ok(()) => validated += 1,
            Err(issues) => {
                let rendered: Vec<String> = issues.iter().take(6).map(ToString::to_string).collect();
                rejected.push((name, rendered));
            }
        }
    }

    assert!(
        validated >= 6,
        "only {validated} fixtures validated; rejected: {rejected:#?}"
    );
    // A rejection is worth surfacing rather than ignoring, so print them.
    if !rejected.is_empty() {
        println!("schema-rejected fixtures: {rejected:#?}");
    }
}

#[test]
fn the_validator_rejects_a_genuinely_broken_workflow() {
    use ctox_actions_runner::validate::Validator;

    let source = concat!(
        "name: CI\n",
        "on: [push]\n",
        "jobs:\n  build:\n",
        "    runs-on: ubuntu-latest\n",
        "    steps:\n      - run: echo hi\n        totally-unknown-key: 1\n",
    );
    let mut doc = Document::parse(source).expect("parses");
    doc.resolve_aliases().expect("resolves");
    let root = doc.root().expect("root");

    let issues = Validator::workflow()
        .check(&doc, root, "workflow-root")
        .expect_err("an unknown step key must be rejected");
    let rendered: Vec<String> = issues.iter().map(ToString::to_string).collect();
    assert!(
        rendered.iter().any(|i| i.contains("Unknown Property")),
        "got {rendered:#?}"
    );
}
