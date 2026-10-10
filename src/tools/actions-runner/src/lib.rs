//! GitHub Actions execution engine for CTOX.
//!
//! Rust port of [`nektos/act`](https://github.com/nektos/act) (MIT,
//! Copyright (c) Christoph Schitt). The port keeps act's observable
//! behaviour: workflow YAML authored for GitHub Actions must run unchanged.
//!
//! Upstream packages map to modules one-to-one:
//!
//! | act `pkg/`      | module                        | ported |
//! |-----------------|-------------------------------|--------|
//! | `workflowpattern` | [`workflow_pattern`]         | yes    |
//! | `lookpath`        | [`lookpath`]                 | yes    |
//! | `model`           | [`model`]                    | yes    |
//! | `exprparser`      | [`expr`]                     | yes    |
//! | `filecollector`   | [`filecollector`]            | yes    |
//! | `schema`          | [`schema`]                   | yes    |
//! | `artifacts`       | [`artifacts`]                | yes    |
//! | `artifactcache`   | [`artifactcache`]            | yes    |
//! | `common`          | [`common`]                   | yes    |
//! | `container`       | [`container`]                | yes    |
//! | `runner`          | [`runner`]                   | partly |
//! | `gh`              | *(pending)*                  | no     |
//!
//! "yes" means every upstream test function in the package runs here as a Rust
//! test, and every source file in it is ported. One package is qualified: the
//! seven test functions in `docker_run_test.go` need a live Docker daemon to run
//! against, so they are not reproduced here — that is a property of the tests,
//! not a gap in the port, and the code they cover is.
//!
//! `runner` is **partly**: [`runner::command`], the workflow-command grammar —
//! `::set-output::`, `##[add-path]`, `stop-commands`, `add-mask`, `save-state` —
//! is ported and carries all ten upstream test functions, and
//! [`runner::expression`] ports the interpolation rewriter.
//! [`runner::run_context`] ports `RunContext`, the context-free half of
//! `run_context.go` and `getGithubContext`, and [`model::GithubContext`]
//! ports the `github` context itself. Not ported: the container lifecycle in
//! `run_context.go`, the git clone executor, the evaluator's context tree
//! (`getEvaluatorInputs`, `getWorkflowSecrets`, `getWorkflowVars`),
//! `EvaluateYamlNode` and `Interpolate`, and the `step` trait with its five
//! implementations.
//!
//! `artifacts`' `TestArtifactFlow` also needs the runner and Docker, so it is
//! the one upstream test in an otherwise complete package that does not run
//! here.
//!
//! [`validate`] ports act's schema validator, [`http`] the `net/http` +
//! `httprouter` exchange that the two servers share, and [`yaml_node`],
//! [`gitignore`], [`gomatch`] and [`git_index`] port the third-party packages
//! act leans on that act itself does not contain.

pub mod artifactcache;
pub mod artifacts;
pub mod base64url;
pub mod common;
pub mod container;
pub mod expr;
pub mod filecollector;
pub mod git_index;
pub mod gitignore;
pub mod gomatch;
pub mod gopath;
pub mod gostrconv;
pub mod http;
pub mod lookpath;
pub mod model;
pub mod runner;
pub mod validate;
pub mod schema;
pub mod workflow_pattern;
pub mod yaml_node;

pub use artifactcache::{Cache, Handler as ArtifactCacheHandler, Service as ArtifactCacheService};
pub use artifacts::{safe_resolve, Server as ArtifactServer, Service as ArtifactService};
pub use common::{RunContext, Scope, Warning};
pub use filecollector::{
    Cancellation, CopyCollector, DefaultFs, FileCollector, FileInfo, FileMode, Fs, Handler,
    TarCollector, WalkOutcome,
};
pub use gitignore::{IgnoreResult, Matcher, Pattern};
pub use lookpath::{look_path, look_path_in, Env, LookPathError, ProcessEnv};
pub use schema::{action_schema, workflow_schema, Definition, Schema, SchemaNode};
pub use workflow_pattern::{
    compile_pattern, compile_patterns, filter, skip, EmptyTraceWriter, StdOutTraceWriter,
    TraceWriter, WorkflowPattern,
};
