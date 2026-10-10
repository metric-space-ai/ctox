//! The workflow data model.
//!
//! Port of `nektos/act` `pkg/model`. A workflow file is read as a
//! [`yaml_node::Document`] and decoded into these types.
//!
//! Several fields are kept as raw YAML node ids rather than being decoded
//! eagerly, exactly as upstream keeps them as `yaml.Node`. The reason is
//! behavioural, not stylistic:
//!
//! * `on:`, `runs-on:`, `if:`, `matrix:`, `needs:`, `secrets:` and `container:`
//!   each accept a scalar *or* a collection, and the collection form is
//!   sometimes a mapping (`runs-on: { group: …, labels: […] }`) and sometimes a
//!   sequence. Decoding into one shape would lose the other.
//! * `${{ … }}` must survive as source text until the evaluator runs.
//! * `matrix.include` and `matrix.exclude` are not plain key lists; they are
//!   lists of objects, and the expansion rules depend on that shape.

use std::collections::BTreeMap;
use std::rc::Rc;

use regex::Regex;

use crate::expr::Value;
use crate::yaml_node::{Document, NodeId};

pub mod github_context;

pub use github_context::{as_string, nested_map_lookup, GithubContext, GitLookups};

/// A workflow file from `.github/workflows`.
#[derive(Debug, Clone, Default)]
pub struct Workflow {
    /// Path the workflow was read from, set by the caller.
    pub file: String,
    /// `name:`
    pub name: String,
    /// The `on:` node, still raw. See the module docs.
    pub raw_on: Option<NodeId>,
    /// `env:`
    pub env: BTreeMap<String, String>,
    /// `jobs:`
    pub jobs: BTreeMap<String, Job>,
    /// `defaults:`
    pub defaults: Defaults,
}

/// A single job.
#[derive(Debug, Clone, Default)]
pub struct Job {
    /// `name:`
    pub name: String,
    /// The `needs:` node, still raw.
    pub raw_needs: Option<NodeId>,
    /// The `runs-on:` node, still raw.
    pub raw_runs_on: Option<NodeId>,
    /// The `env:` node, still raw.
    pub raw_env: Option<NodeId>,
    /// The `if:` node, still raw.
    pub raw_if: Option<NodeId>,
    /// `steps:`
    pub steps: Vec<Step>,
    /// `timeout-minutes:`
    pub timeout_minutes: String,
    /// `services:`
    pub services: BTreeMap<String, ContainerSpec>,
    /// `strategy:`
    pub strategy: Option<Strategy>,
    /// The `container:` node, still raw.
    pub raw_container: Option<NodeId>,
    /// `defaults:`
    pub defaults: Defaults,
    /// `outputs:`
    pub outputs: BTreeMap<String, String>,
    /// `uses:` for a reusable-workflow call.
    pub uses: String,
    /// `with:` for a reusable-workflow call.
    ///
    /// Keyed by `String`, as upstream's `map[string]interface{}` is: a `with:`
    /// key is a plain YAML scalar. The *value* keeps its type, because a
    /// reusable workflow receives `with: {count: 3}` as a number and
    /// `inputs.count` must compare as one.
    pub with: BTreeMap<String, Value>,
    /// The `secrets:` node, still raw.
    pub raw_secrets: Option<NodeId>,
    /// What the job concluded, set by the runner once it has finished.
    ///
    /// Not part of the YAML: `needs.<job>.result` reports it, so a job that
    /// waits on a failed job can branch on it. Empty until the job runs.
    pub result: String,
}

/// A `strategy:` block.
#[derive(Debug, Clone, Default)]
pub struct Strategy {
    /// `fail-fast:`, kept as text because YAML allows a boolean or a string.
    pub fail_fast: String,
    /// `max-parallel:`, same reason.
    pub max_parallel: String,
    /// The `matrix:` node, still raw.
    pub raw_matrix: Option<NodeId>,
}

/// Workflow- or job-level `defaults:`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Defaults {
    /// `defaults.run:`
    pub run: RunDefaults,
}

/// `defaults.run:`
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunDefaults {
    /// `shell:`
    pub shell: String,
    /// `working-directory:`
    pub working_directory: String,
}

/// A `container:` or `services:` entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContainerSpec {
    /// `image:`
    pub image: String,
    /// `env:`
    pub env: BTreeMap<String, String>,
    /// `ports:`
    pub ports: Vec<String>,
    /// `volumes:`
    pub volumes: Vec<String>,
    /// `options:`
    pub options: String,
    /// `credentials:`
    pub credentials: BTreeMap<String, String>,
    /// The `credentials:` node, still raw.
    ///
    /// Present because `credentials` alone cannot say what Go's nil-ness says.
    /// Upstream distinguishes three cases that all decode to the **same empty
    /// map** here, and they do not behave alike:
    ///
    /// | workflow | Go | `credentials` alone |
    /// |---|---|---|
    /// | no `credentials:` | nil | `{}` |
    /// | `credentials:` (null) | nil | `{}` |
    /// | `credentials: {}` | **non-nil, len 0** | `{}` |
    ///
    /// The first two take the secrets path; the third is an error
    /// (`invalid property count for key 'credentials:'`). Read through
    /// [`ContainerSpec::credentials_map`] the distinction is exact.
    pub raw_credentials: Option<NodeId>,
}

impl ContainerSpec {
    /// The `credentials:` mapping, or `None` where Go's is nil.
    ///
    /// `None` for an absent node and for one that is not a mapping — Go's
    /// `yaml.v3` leaves the field nil in both cases, and `handleCredentials`
    /// returns the config secrets for a nil field. A **mapping** yields
    /// `Some`, empty or not, which is what puts `credentials: {}` on the
    /// property-count error rather than the secrets path.
    pub fn credentials_map(
        &self,
        doc: &Document,
    ) -> Option<BTreeMap<String, String>> {
        let id = self.raw_credentials?;
        let node = doc.node(id)?;
        if !node.is_mapping() {
            return None;
        }
        Some(self.credentials.clone())
    }
}

/// A single step.
#[derive(Debug, Clone, Default)]
pub struct Step {
    /// `id:`
    pub id: String,
    /// The `if:` node, still raw.
    pub raw_if: Option<NodeId>,
    /// `name:`
    pub name: String,
    /// `uses:`
    pub uses: String,
    /// `run:`
    pub run: String,
    /// `working-directory:`
    pub working_directory: String,
    /// The shell actually in force, resolved from step, job, or workflow
    /// `defaults.run.shell`. Filled in by the runner, never read from YAML.
    pub workflow_shell: String,
    /// `shell:`
    pub shell: String,
    /// The `env:` node, still raw.
    pub raw_env: Option<NodeId>,
    /// `with:`
    pub with: BTreeMap<String, String>,
    /// `continue-on-error:`, kept as text like `fail-fast`.
    pub raw_continue_on_error: String,
    /// `timeout-minutes:`
    pub timeout_minutes: String,
}

/// `on:` as written, which has three accepted shapes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowOn {
    /// `on: push`
    Single(String),
    /// `on: [push, pull_request]`
    List(Vec<String>),
    /// `on: { push: {…} }`
    Map(Vec<(String, String)>),
}

impl Workflow {
    /// Decodes a workflow from a parsed YAML document.
    pub fn from_document(file: &str, doc: &Document) -> Result<Self, ModelError> {
        let Some(root) = doc.root() else {
            return Ok(Self {
                file: file.to_string(),
                ..Self::default()
            });
        };
        // act resolves anchors before decoding.
        let mut doc = doc.clone();
        doc.resolve_aliases()
            .map_err(|err| ModelError::new(file, format!("{err}")))?;
        let root = doc.root().unwrap_or(root);

        let name = doc
            .map_get(root, "name")
            .and_then(|id| doc.scalar(id))
            .unwrap_or_default();

        let raw_on = doc.map_get(root, "on");
        let env = doc
            .map_get(root, "env")
            .and_then(|id| doc.string_map(id))
            .unwrap_or_default();
        let defaults = decode_defaults(&doc, doc.map_get(root, "defaults"));

        let mut jobs = BTreeMap::new();
        if let Some(jobs_id) = doc.map_get(root, "jobs") {
            for (name_id, job_id) in doc.map_entries(jobs_id) {
                let Some(name) = doc.scalar(name_id) else {
                    continue;
                };
                let job = decode_job(&doc, job_id)
                    .map_err(|err| ModelError::new(file, format!("job {name}: {err}")))?;
                jobs.insert(name, job);
            }
        }

        Ok(Self {
            file: file.to_string(),
            name,
            raw_on,
            env,
            jobs,
            defaults,
        })
    }

    /// The event names in `on:`, whichever shape was used.
    pub fn on(&self, doc: &Document) -> Vec<String> {
        match self.on_node(doc) {
            Some(WorkflowOn::Single(name)) => vec![name],
            Some(WorkflowOn::List(names)) => names,
            Some(WorkflowOn::Map(entries)) => entries.into_iter().map(|(k, _)| k).collect(),
            None => Vec::new(),
        }
    }

    /// The `on:` node decoded into its three shapes.
    pub fn on_node(&self, doc: &Document) -> Option<WorkflowOn> {
        let id = self.raw_on?;
        let node = doc.node(id)?;
        match node.kind {
            crate::yaml_node::NodeKind::Scalar => Some(WorkflowOn::Single(node.value.clone())),
            crate::yaml_node::NodeKind::Sequence => {
                doc.string_slice(id).map(WorkflowOn::List)
            }
            crate::yaml_node::NodeKind::Mapping => {
                let entries = doc
                    .map_entries(id)
                    .into_iter()
                    .filter_map(|(k, _)| doc.scalar(k))
                    .map(|k| (k, String::new()))
                    .collect();
                Some(WorkflowOn::Map(entries))
            }
            _ => None,
        }
    }

    /// The raw body of one event's configuration, as text.
    pub fn on_event(&self, doc: &Document, event: &str) -> Option<String> {
        let id = self.raw_on?;
        let node = doc.node(id)?;
        if !node.is_mapping() {
            return None;
        }
        doc.map_get(id, event)
            .and_then(|value| node_body_text(doc, value))
    }

    /// The `workflow_dispatch` inputs, when the workflow is dispatchable.
    pub fn workflow_dispatch_inputs(&self, doc: &Document) -> Option<BTreeMap<String, DispatchInput>> {
        let id = self.raw_on?;
        let node = doc.node(id)?;
        if node.is_mapping() {
            let dispatch = doc.map_get(id, "workflow_dispatch")?;
            let inputs = doc.map_get(dispatch, "inputs")?;
            let mut out = BTreeMap::new();
            for (name_id, value_id) in doc.map_entries(inputs) {
                let name = doc.scalar(name_id)?;
                out.insert(name.clone(), decode_dispatch_input(doc, &name, value_id));
            }
            return Some(out);
        }
        if self.on(doc).iter().any(|e| e == "workflow_dispatch") {
            return Some(BTreeMap::new());
        }
        None
    }
}

impl Workflow {
    /// The `workflow_call` inputs, when the workflow is callable.
    ///
    /// Mirrors [`Workflow::workflow_dispatch_inputs`]: a mapping `on:` is read
    /// through, and a list `on:` answers with an empty map so a caller that
    /// expects "callable with no inputs" gets one rather than `None`.
    pub fn workflow_call_inputs(&self, doc: &Document) -> Option<BTreeMap<String, WorkflowCallInput>> {
        let id = self.raw_on?;
        let node = doc.node(id)?;
        if node.is_mapping() {
            let call = doc.map_get(id, "workflow_call")?;
            let mut out = BTreeMap::new();
            if let Some(inputs) = doc.map_get(call, "inputs") {
                for (name_id, value_id) in doc.map_entries(inputs) {
                    let name = doc.scalar(name_id)?;
                    out.insert(name.clone(), decode_workflow_call_input(doc, &name, value_id));
                }
            }
            return Some(out);
        }
        if self.on(doc).iter().any(|e| e == "workflow_call") {
            return Some(BTreeMap::new());
        }
        None
    }
}

/// One `workflow_call` input.
///
/// Its `default:` is a YAML **node**, not a string, unlike
/// [`DispatchInput::default`]. That difference is invisible in act's own
/// results and not here: decoding a scalar node into a Go `string` yields the
/// node's raw text, so `default: true` is the string `"true"`. Measured against
/// yaml.v3 — only a sequence or a mapping fails, and then the value is empty.
/// The default is therefore kept as its raw scalar text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkflowCallInput {
    /// `description:`
    pub description: String,
    /// `required:`
    pub required: bool,
    /// `default:`, as its raw scalar text; empty when absent or not a scalar.
    pub default: String,
    /// `type:`, which turns the value into a boolean when it says `boolean`.
    pub input_type: String,
}

fn decode_workflow_call_input(
    doc: &Document,
    _name: &str,
    id: NodeId,
) -> WorkflowCallInput {
    WorkflowCallInput {
        description: doc
            .map_get(id, "description")
            .and_then(|n| doc.scalar(n))
            .unwrap_or_default(),
        required: doc
            .map_get(id, "required")
            .and_then(|n| doc.scalar(n))
            .map(|text| text == "true")
            .unwrap_or(false),
        default: doc
            .map_get(id, "default")
            .and_then(|n| doc.scalar(n))
            .unwrap_or_default(),
        input_type: doc
            .map_get(id, "type")
            .and_then(|n| doc.scalar(n))
            .unwrap_or_default(),
    }
}

/// One `workflow_dispatch` input.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DispatchInput {
    /// `description:`
    pub description: String,
    /// `required:`
    pub required: bool,
    /// `default:`
    pub default: String,
    /// `type:`
    pub input_type: String,
    /// `options:`
    pub options: Vec<String>,
}

fn decode_dispatch_input(doc: &Document, _name: &str, id: NodeId) -> DispatchInput {
    DispatchInput {
        description: doc
            .map_get(id, "description")
            .and_then(|n| doc.scalar(n))
            .unwrap_or_default(),
        required: doc
            .map_get(id, "required")
            .and_then(|n| doc.boolean(n))
            .unwrap_or(false),
        default: doc
            .map_get(id, "default")
            .and_then(|n| doc.scalar(n))
            .unwrap_or_default(),
        input_type: doc
            .map_get(id, "type")
            .and_then(|n| doc.scalar(n))
            .unwrap_or_default(),
        options: doc
            .map_get(id, "options")
            .and_then(|n| doc.string_slice(n))
            .unwrap_or_default(),
    }
}

impl Step {
    /// The display name, preferring `name`, then `uses`, then `run`, then `id`.
    pub fn display_name(&self) -> &str {
        if !self.name.is_empty() {
            &self.name
        } else if !self.uses.is_empty() {
            &self.uses
        } else if !self.run.is_empty() {
            &self.run
        } else {
            &self.id
        }
    }

    /// `env:` as a map.
    pub fn environment(&self, doc: &Document) -> BTreeMap<String, String> {
        self.raw_env
            .and_then(|id| doc.string_map(id))
            .unwrap_or_default()
    }

    /// `env:` merged with `with:` as `INPUT_*`, as `GetEnv` produces.
    ///
    /// Keys are upper-cased and anything outside `[A-Z0-9-]` becomes `_`.
    pub fn get_env(&self, doc: &Document) -> BTreeMap<String, String> {
        let mut env = self.environment(doc);
        for (key, value) in &self.with {
            let normalised: String = key
                .to_uppercase()
                .chars()
                .map(|c| {
                    if c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            env.insert(format!("INPUT_{normalised}"), value.clone());
        }
        env
    }

    /// The command that runs this step's script.
    ///
    /// Port of `ShellCommand`, which mirrors the actions/runner reference
    /// implementation. `{0}` is the script path.
    pub fn shell_command(&self) -> String {
        match self.shell.as_str() {
            "" => "bash -e {0}".to_string(),
            "bash" => {
                if self.workflow_shell.is_empty() {
                    "bash -e {0}".to_string()
                } else {
                    "bash --noprofile --norc -e -o pipefail {0}".to_string()
                }
            }
            "pwsh" => "pwsh -command . '{0}'".to_string(),
            "python" => "python {0}".to_string(),
            "sh" => "sh -e {0}".to_string(),
            "cmd" => "cmd /D /E:ON /V:OFF /S /C \"CALL \"{0}\"\"".to_string(),
            "powershell" => "powershell -command . '{0}'".to_string(),
            other => other.to_string(),
        }
    }

    /// What kind of step this is.
    pub fn step_type(&self) -> StepType {
        if self.run.is_empty() && self.uses.is_empty() {
            return StepType::Invalid;
        }
        if !self.run.is_empty() {
            // A step with both `run` and `uses` is ambiguous.
            return if self.uses.is_empty() {
                StepType::Run
            } else {
                StepType::Invalid
            };
        }
        classify_step_uses(&self.uses)
    }
}

impl Job {
    /// The job ids in `needs:`, whichever shape was used.
    pub fn needs(&self, doc: &Document) -> Vec<String> {
        self.raw_needs
            .and_then(|id| doc.string_slice(id))
            .unwrap_or_default()
    }

    /// The runner labels in `runs-on:`.
    ///
    /// Accepts a scalar, a sequence, or the `{ group, labels }` mapping form.
    pub fn runs_on(&self, doc: &Document) -> Vec<String> {
        let Some(id) = self.raw_runs_on else {
            return Vec::new();
        };
        let Some(node) = doc.node(id) else {
            return Vec::new();
        };
        if !node.is_mapping() {
            return doc.string_slice(id).unwrap_or_default();
        }
        let mut labels = doc
            .map_get(id, "labels")
            .and_then(|n| doc.string_slice(n))
            .unwrap_or_default();
        if let Some(group) = doc.map_get(id, "group").and_then(|n| doc.scalar(n)) {
            if !group.is_empty() {
                labels.push(group);
            }
        }
        labels
    }

    /// `env:` as a map.
    pub fn environment(&self, doc: &Document) -> BTreeMap<String, String> {
        self.raw_env
            .and_then(|id| doc.string_map(id))
            .unwrap_or_default()
    }

    /// True when the job inherits the caller's secrets.
    pub fn inherit_secrets(&self, doc: &Document) -> bool {
        self.raw_secrets
            .and_then(|id| doc.scalar(id))
            .is_some_and(|value| value == "inherit")
    }

    /// Explicit secrets passed to a reusable workflow.
    pub fn secrets(&self, doc: &Document) -> BTreeMap<String, String> {
        self.raw_secrets
            .and_then(|id| doc.string_map(id))
            .unwrap_or_default()
    }

    /// The `container:` block, accepting both the scalar and mapping forms.
    pub fn container(&self, doc: &Document) -> Option<ContainerSpec> {
        let id = self.raw_container?;
        let node = doc.node(id)?;
        match node.kind {
            crate::yaml_node::NodeKind::Scalar => Some(ContainerSpec {
                image: node.value.clone(),
                ..ContainerSpec::default()
            }),
            crate::yaml_node::NodeKind::Mapping => Some(decode_container(doc, id)),
            _ => None,
        }
    }

    /// The matrix as `key -> values`, without include/exclude.
    pub fn matrix(&self, doc: &Document) -> BTreeMap<String, Vec<Value>> {
        let Some(strategy) = &self.strategy else {
            return BTreeMap::new();
        };
        let Some(id) = strategy.raw_matrix else {
            return BTreeMap::new();
        };
        let Some(node) = doc.node(id) else {
            return BTreeMap::new();
        };
        if !node.is_mapping() {
            return BTreeMap::new();
        }
        let mut out = BTreeMap::new();
        for (key_id, value_id) in doc.map_entries(id) {
            let Some(key) = doc.scalar(key_id) else {
                continue;
            };
            // act deletes `include` and `exclude` from the matrix before
            // expanding it. Leaving them in would turn them into dimensions and
            // silently multiply every combination.
            if key == "include" || key == "exclude" {
                continue;
            }
            let values = match doc.node(value_id) {
                // Matrix entries are typed, not strings: `node: [18, 20]` must
                // yield integers so `${{ matrix.node }}` compares numerically.
                Some(node) if node.is_sequence() => node
                    .content
                    .iter()
                    .filter_map(|child| node_to_value(doc, *child))
                    .collect(),
                Some(node) if node.is_scalar() => vec![match node.value.as_str() {
                    "true" | "True" | "TRUE" => Value::Bool(true),
                    "false" | "False" | "FALSE" => Value::Bool(false),
                    other => Value::String(other.to_string()),
                }],
                _ => Vec::new(),
            };
            out.insert(key, values);
        }
        out
    }

    /// The expanded matrix, one entry per job instance.
    ///
    /// Applies GitHub's `include`/`exclude` rules: `exclude` removes
    /// combinations, `include` first extends combinations it can extend and
    /// otherwise adds new ones. An `exclude` key that matches no matrix key is
    /// a hard error, which is what GitHub does.
    pub fn get_matrixes(&self, doc: &Document) -> Result<Vec<BTreeMap<String, Value>>, ModelError> {
        let Some(strategy) = &self.strategy else {
            return Ok(vec![BTreeMap::new()]);
        };
        let Some(matrix_id) = strategy.raw_matrix else {
            return Ok(vec![BTreeMap::new()]);
        };
        if !doc.node(matrix_id).is_some_and(|n| n.is_mapping()) {
            return Ok(vec![BTreeMap::new()]);
        }

        let base = self.matrix(doc);

        // include and exclude are lists of objects, not value lists.
        let includes = decode_matrix_objects(doc, matrix_id, "include");
        let excludes = decode_matrix_objects(doc, matrix_id, "exclude");

        for exclude in &excludes {
            for key in exclude.keys() {
                if !base.contains_key(key) {
                    return Err(ModelError::new(
                        &self.uses,
                        format!(
                            "the workflow is not valid. Matrix exclude key {key:?} does not match any key within the matrix"
                        ),
                    ));
                }
            }
        }

        let mut matrixes = cartesian_product(&base);

        matrixes.retain(|matrix| {
            !excludes.iter().any(|exclude| {
                matrix
                    .iter()
                    .all(|(key, value)| match exclude.get(key) {
                        Some(other) => other == value,
                        None => true,
                    })
            })
        });

        let mut extra_includes: Vec<BTreeMap<String, Value>> = Vec::new();
        for include in &includes {
            let mut matched = false;
            for matrix in matrixes.iter_mut() {
                if include_matches(matrix, include, &base) {
                    matched = true;
                    matrix.extend(include.clone());
                }
            }
            if !matched {
                extra_includes.push(include.clone());
            }
        }
        matrixes.extend(extra_includes);

        if matrixes.is_empty() {
            matrixes.push(BTreeMap::new());
        }
        Ok(matrixes)
    }

    /// `max-parallel`, defaulting to 4.
    ///
    /// act's default is deliberate: GitHub allows 20 parallel jobs, but a
    /// self-hosted runner effectively runs one at a time.
    ///
    /// Upstream stores the resolved number on the `Strategy` and fills it in as
    /// a side effect of `GetMatrixes()`; the same numbers are computed here on
    /// read. The two agree for every read that happens after matrix expansion,
    /// which is every read during job execution. See
    /// [`crate::runner::expression::EvaluationInputs`] for the measured
    /// before/after table and for the one read where they do not agree.
    ///
    /// A value `strconv.Atoi` rejects yields **0, not 4** — see
    /// [`Job::fail_fast`] for why, and for the measurement.
    pub fn max_parallel(&self) -> i64 {
        let strategy = match &self.strategy {
            Some(strategy) => strategy,
            None => return 4,
        };
        if strategy.max_parallel.is_empty() {
            return 4;
        }
        // `strconv.Atoi`, not `parse().unwrap_or(0)`: Atoi returns the *clamped*
        // bound on a range error and 0 only on a syntax error, and the two need
        // different answers. Measured both ways against go1.26.2 — see
        // [`crate::gostrconv`].
        crate::gostrconv::parse_int(&strategy.max_parallel)
    }

    /// `fail-fast`, defaulting to true.
    ///
    /// Go's `strconv.ParseBool` accepts exactly `1 t T TRUE true True` and
    /// `0 f F FALSE false False`, and this list is that one.
    ///
    /// # An unparseable value gives `false`, not the default
    ///
    /// `GetFailFast` reads
    ///
    /// ```go
    /// failFast := true
    /// if s.FailFastString != "" {
    ///     if failFast, err = strconv.ParseBool(s.FailFastString); err != nil {
    ///         log.Errorf(...)          // the log does NOT restore the default
    ///     }
    /// }
    /// ```
    ///
    /// The assignment is `=`, not `:=`, so the zero value returned alongside the
    /// error **overwrites** `true`. Measured on v0.2.89:
    ///
    /// | `fail-fast` | `max-parallel` | `GetFailFast` | `GetMaxParallel` |
    /// |---|---|---|---|
    /// | *(absent)* | *(absent)* | `true` | `4` |
    /// | `maybe` | `lots` | `false` | `0` |
    /// | `yes` | `3.5` | `false` | `0` |
    /// | `0` | `007` | `false` | `7` |
    /// | `True` | `  3  ` | `true` | `0` |
    /// | `yes` | `99999999999999999999` | `false` | `9223372036854775807` |
    ///
    /// The last row is why `max_parallel` cannot be `parse().unwrap_or(0)`: a
    /// *range* error clamps to the bound rather than zeroing, so the syntax
    /// errors and the overflow need different answers. Measured against the
    /// real `pkg/model` on v0.2.89; the full table is in [`crate::gostrconv`].
    ///
    /// The last row is `Atoi` refusing to trim, and Rust's `parse` refuses too —
    /// that one agrees by accident of both implementations, not by decision.
    /// Returning the default on a parse error is the reading the code invites
    /// and upstream does not do it.
    ///
    /// Computed on read for the same reason as [`Job::max_parallel`].
    pub fn fail_fast(&self) -> bool {
        let strategy = match &self.strategy {
            Some(strategy) => strategy,
            None => return true,
        };
        if strategy.fail_fast.is_empty() {
            return true;
        }
        match strategy.fail_fast.as_str() {
            "1" | "t" | "T" | "true" | "TRUE" | "True" => true,
            "0" | "f" | "F" | "false" | "FALSE" | "False" => false,
            // Go's zero value for a failed ParseBool, not the default above.
            _ => false,
        }
    }

    /// What kind of job this is.
    pub fn job_type(&self) -> Result<JobType, ModelError> {
        if self.uses.is_empty() {
            return Ok(JobType::Default);
        }
        match classify_job_uses(&self.uses) {
            JobType::Invalid => Err(ModelError::new(
                &self.uses,
                format!(
                    "`uses` key references invalid workflow path '{}'. Must start with './' if it's a local workflow, or must start with '<org>/<repo>/' and include an '@' if it's a remote workflow",
                    self.uses
                ),
            )),
            other => Ok(other),
        }
    }
}

/// What kind of job is about to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobType {
    /// A job with `run` steps.
    Default,
    /// A `uses:` pointing at a local workflow file.
    ReusableWorkflowLocal,
    /// A `uses:` pointing at another repository's workflow file.
    ReusableWorkflowRemote,
    /// A `uses:` that is neither.
    Invalid,
}

impl JobType {
    /// The name act uses in logs and plan output.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::ReusableWorkflowLocal => "local-reusable-workflow",
            Self::ReusableWorkflowRemote => "remote-reusable-workflow",
            Self::Invalid => "unknown",
        }
    }
}

/// What kind of step is about to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepType {
    /// A `run` step.
    Run,
    /// `uses: docker://…`
    UsesDockerUrl,
    /// `uses: ./local-action`
    UsesActionLocal,
    /// `uses: org/repo@ref`
    UsesActionRemote,
    /// `uses: ./.github/workflows/x.yml`
    ReusableWorkflowLocal,
    /// `uses: org/repo/.github/workflows/x.yml@ref`
    ReusableWorkflowRemote,
    /// Neither `run` nor a usable `uses`.
    Invalid,
}

impl StepType {
    /// The name act uses in logs and plan output.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Invalid => "invalid",
            Self::Run => "run",
            Self::UsesActionLocal => "local-action",
            Self::UsesActionRemote => "remote-action",
            Self::UsesDockerUrl => "docker",
            Self::ReusableWorkflowLocal => "local-reusable-workflow",
            Self::ReusableWorkflowRemote => "remote-reusable-workflow",
        }
    }
}

/// Classifies a job's `uses:` value.
fn classify_job_uses(uses: &str) -> JobType {
    if uses.starts_with("./") {
        return if is_workflow_path(uses) {
            JobType::ReusableWorkflowLocal
        } else {
            JobType::Invalid
        };
    }
    if let Some((left, _)) = uses.split_once('@') {
        // A remote workflow reference is `org/repo/path/to/file.yml@ref`.
        if is_workflow_path(left) && left.matches('/').count() >= 2 {
            return JobType::ReusableWorkflowRemote;
        }
    }
    JobType::Invalid
}

/// Classifies a step's `uses:` value.
fn classify_step_uses(uses: &str) -> StepType {
    if uses.starts_with("docker://") {
        return StepType::UsesDockerUrl;
    }
    if uses.starts_with("./") {
        return if is_workflow_path(uses) {
            StepType::ReusableWorkflowLocal
        } else {
            StepType::UsesActionLocal
        };
    }
    if let Some((left, _)) = uses.split_once('@') {
        if is_workflow_path(left) && left.matches('/').count() >= 2 {
            return StepType::ReusableWorkflowRemote;
        }
    }
    if uses.matches('/').count() >= 1 && !uses.contains("://") {
        return StepType::UsesActionRemote;
    }
    StepType::Invalid
}

/// True for `./x.yml`, `./x.yaml`, `./x.yml@v1` and the remote equivalents.
fn is_workflow_path(path: &str) -> bool {
    workflow_extension().is_match(path)
}

fn workflow_extension() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\.(ya?ml)(?:$|@)").expect("static regex must compile"))
}

/// The outcome or conclusion of a step, as `steps.<id>.outcome` and
/// `steps.<id>.conclusion` expose it.
///
/// Upstream models this as an `int` with a parallel string table, and
/// [`StepStatus::as_str`] keeps the one quirk of that design: a value with no
/// name renders as the **empty string** rather than panicking or guessing. That
/// is what a `steps.<id>.conclusion` of an out-of-range status prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub enum StepStatus {
    /// `success`, and the zero value, so a `Default` is a success.
    #[default]
    Success,
    /// `failure`.
    Failure,
    /// `skipped`.
    Skipped,
}

impl StepStatus {
    /// The wire name, or `""` for a status upstream's table has no entry for.
    pub fn as_str(self) -> &'static str {
        match self {
            StepStatus::Success => "success",
            StepStatus::Failure => "failure",
            StepStatus::Skipped => "skipped",
        }
    }

    /// Parses a wire name, rejecting anything the table does not name.
    ///
    /// Upstream's `UnmarshalText` returns `invalid step status %q`; the message
    /// is kept so a decode failure reads the same.
    pub fn parse(text: &str) -> Result<Self, ModelError> {
        match text {
            "success" => Ok(StepStatus::Success),
            "failure" => Ok(StepStatus::Failure),
            "skipped" => Ok(StepStatus::Skipped),
            other => Err(ModelError::new("", format!("invalid step status {other:?}"))),
        }
    }
}

impl std::fmt::Display for StepStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for StepStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        StepStatus::parse(&text).map_err(serde::de::Error::custom)
    }
}

impl serde::Serialize for StepStatus {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// What a step produced, as `steps.<id>` exposes it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StepResult {
    /// The output values, by name.
    pub outputs: BTreeMap<String, String>,
    /// What the step decided.
    pub conclusion: StepStatus,
    /// What the step ended up as, which differs from the conclusion when a
    /// `continue-on-error` step failed: conclusion `success`, outcome `failure`.
    pub outcome: StepStatus,
}

/// The `job` context, as `job.status` and `job.container` expose it.
///
/// Only [`JobContext::status`] is filled in by `run_context.go`; the container
/// and services members are the shape GitHub's context has, and are empty here
/// because nothing in the ported code fills them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobContext {
    /// `success`, `failure` or `cancelled`.
    pub status: String,
    /// The job container's id and network.
    pub container: JobContainerContext,
    /// The service containers, by service name.
    pub services: BTreeMap<String, JobServiceContext>,
}

/// The `job.container` member.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobContainerContext {
    /// The container id.
    pub id: String,
    /// The network the container is attached to.
    pub network: String,
}

/// One entry of `job.services`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobServiceContext {
    /// The service container's id.
    pub id: String,
}

/// The outputs of a called job, as `jobs.<id>.outputs` exposes them in a
/// reusable workflow.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkflowCallResult {
    /// The job's declared outputs.
    pub outputs: BTreeMap<String, String>,
}

/// One job's execution: which workflow, which job inside it.
///
/// The document is carried along because this port's [`Workflow`] and [`Job`]
/// keep `env:` and `needs:` as *raw node ids* into the YAML tree rather than
/// decoding them eagerly, so anything that reads them needs the document. That
/// is the one structural difference from act, whose model is fully decoded and
/// needs no second argument.
#[derive(Debug, Clone)]
pub struct Run {
    /// The workflow being run.
    pub workflow: Workflow,
    /// The document [`Run::workflow`] was decoded from.
    document: Rc<Document>,
    /// Which job, as the key in [`Workflow::jobs`].
    pub job_id: String,
}

impl Run {
    /// A run of `job_id` out of `workflow`, read from `document`.
    pub fn new(workflow: Workflow, document: Rc<Document>, job_id: impl Into<String>) -> Self {
        Self {
            workflow,
            document,
            job_id: job_id.into(),
        }
    }

    /// The document the workflow was decoded from.
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// The job being run, or `None` when `job_id` names no job.
    pub fn job(&self) -> Option<&Job> {
        self.workflow.jobs.get(&self.job_id)
    }

    /// The job being run, mutably.
    ///
    /// Exists because `rc.result("…")` writes back into the job the run
    /// selected. That write is not a detail: `needs.<job>.result` and a
    /// dependant's `success()` both read it, so a job that never records its
    /// own outcome leaves every dependant permanently undecidable.
    pub fn job_mut(&mut self) -> Option<&mut Job> {
        let id = self.job_id.clone();
        self.workflow.jobs.get_mut(&id)
    }

    /// The job's name, falling back to the job id.
    ///
    /// Upstream's `Run.String`, and the name a `needs` entry and the container
    /// name are built from.
    pub fn name(&self) -> String {
        match self.job() {
            Some(job) if !job.name.is_empty() => job.name.clone(),
            _ => self.job_id.clone(),
        }
    }
}

impl std::fmt::Display for Run {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name())
    }
}

/// A decoding or validation failure, tagged with the file it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelError {
    /// The workflow or job the error belongs to.
    pub context: String,
    /// What went wrong.
    pub message: String,
}

impl ModelError {
    fn new(context: &str, message: impl Into<String>) -> Self {
        Self {
            context: context.to_string(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.context.is_empty() {
            f.write_str(&self.message)
        } else {
            write!(f, "{}: {}", self.context, self.message)
        }
    }
}

impl std::error::Error for ModelError {}

fn decode_defaults(doc: &Document, id: Option<NodeId>) -> Defaults {
    let Some(id) = id else {
        return Defaults::default();
    };
    let run = doc.map_get(id, "run");
    Defaults {
        run: RunDefaults {
            shell: run
                .and_then(|n| doc.map_get(n, "shell"))
                .and_then(|n| doc.scalar(n))
                .unwrap_or_default(),
            working_directory: run
                .and_then(|n| doc.map_get(n, "working-directory"))
                .and_then(|n| doc.scalar(n))
                .unwrap_or_default(),
        },
    }
}

fn decode_container(doc: &Document, id: NodeId) -> ContainerSpec {
    ContainerSpec {
        image: doc
            .map_get(id, "image")
            .and_then(|n| doc.scalar(n))
            .unwrap_or_default(),
        env: doc
            .map_get(id, "env")
            .and_then(|n| doc.string_map(n))
            .unwrap_or_default(),
        ports: doc
            .map_get(id, "ports")
            .and_then(|n| doc.string_slice(n))
            .unwrap_or_default(),
        volumes: doc
            .map_get(id, "volumes")
            .and_then(|n| doc.string_slice(n))
            .unwrap_or_default(),
        options: doc
            .map_get(id, "options")
            .and_then(|n| doc.scalar(n))
            .unwrap_or_default(),
        credentials: doc
            .map_get(id, "credentials")
            .and_then(|n| doc.string_map(n))
            .unwrap_or_default(),
        raw_credentials: doc.map_get(id, "credentials"),
    }
}

fn decode_job(doc: &Document, id: NodeId) -> Result<Job, ModelError> {
    let strategy = doc.map_get(id, "strategy").map(|id| Strategy {
        fail_fast: doc
            .map_get(id, "fail-fast")
            .and_then(|n| doc.scalar(n))
            .unwrap_or_default(),
        max_parallel: doc
            .map_get(id, "max-parallel")
            .and_then(|n| doc.scalar(n))
            .unwrap_or_default(),
        raw_matrix: doc.map_get(id, "matrix"),
    });

    let mut services = BTreeMap::new();
    if let Some(services_id) = doc.map_get(id, "services") {
        for (name_id, spec_id) in doc.map_entries(services_id) {
            if let Some(name) = doc.scalar(name_id) {
                services.insert(name, decode_container(doc, spec_id));
            }
        }
    }

    let mut steps = Vec::new();
    if let Some(steps_id) = doc.map_get(id, "steps") {
        let step_ids = doc.node(steps_id).map(|n| n.content.clone()).unwrap_or_default();
        for step_id in step_ids {
            steps.push(Step {
                id: doc
                    .map_get(step_id, "id")
                    .and_then(|n| doc.scalar(n))
                    .unwrap_or_default(),
                raw_if: doc.map_get(step_id, "if"),
                name: doc
                    .map_get(step_id, "name")
                    .and_then(|n| doc.scalar(n))
                    .unwrap_or_default(),
                uses: doc
                    .map_get(step_id, "uses")
                    .and_then(|n| doc.scalar(n))
                    .unwrap_or_default(),
                run: doc
                    .map_get(step_id, "run")
                    .and_then(|n| doc.scalar(n))
                    .unwrap_or_default(),
                working_directory: doc
                    .map_get(step_id, "working-directory")
                    .and_then(|n| doc.scalar(n))
                    .unwrap_or_default(),
                workflow_shell: String::new(),
                shell: doc
                    .map_get(step_id, "shell")
                    .and_then(|n| doc.scalar(n))
                    .unwrap_or_default(),
                raw_env: doc.map_get(step_id, "env"),
                with: doc
                    .map_get(step_id, "with")
                    .and_then(|n| doc.string_map(n))
                    .unwrap_or_default(),
                raw_continue_on_error: doc
                    .map_get(step_id, "continue-on-error")
                    .and_then(|n| doc.scalar(n))
                    .unwrap_or_default(),
                timeout_minutes: doc
                    .map_get(step_id, "timeout-minutes")
                    .and_then(|n| doc.scalar(n))
                    .unwrap_or_default(),
            });
        }
    }

    Ok(Job {
        name: doc
            .map_get(id, "name")
            .and_then(|n| doc.scalar(n))
            .unwrap_or_default(),
        raw_needs: doc.map_get(id, "needs"),
        raw_runs_on: doc.map_get(id, "runs-on"),
        raw_env: doc.map_get(id, "env"),
        raw_if: doc.map_get(id, "if"),
        steps,
        timeout_minutes: doc
            .map_get(id, "timeout-minutes")
            .and_then(|n| doc.scalar(n))
            .unwrap_or_default(),
        services,
        strategy,
        raw_container: doc.map_get(id, "container"),
        defaults: decode_defaults(doc, doc.map_get(id, "defaults")),
        outputs: doc
            .map_get(id, "outputs")
            .and_then(|n| doc.string_map(n))
            .unwrap_or_default(),
        uses: doc
            .map_get(id, "uses")
            .and_then(|n| doc.scalar(n))
            .unwrap_or_default(),
        with: BTreeMap::new(),
        raw_secrets: doc.map_get(id, "secrets"),
        // Set by the runner, never by the YAML.
        result: String::new(),
    })
}

fn decode_matrix_objects(
    doc: &Document,
    matrix_id: NodeId,
    key: &str,
) -> Vec<BTreeMap<String, Value>> {
    let Some(node) = doc.map_get(matrix_id, key) else {
        return Vec::new();
    };
    let Some(parent) = doc.node(node) else {
        return Vec::new();
    };
    if !parent.is_sequence() {
        return Vec::new();
    }
    parent
        .content
        .iter()
        .filter_map(|child| {
            if !doc.node(*child).is_some_and(|n| n.is_mapping()) {
                return None;
            }
            let mut out = BTreeMap::new();
            for (k, v) in doc.map_entries(*child) {
                if let (Some(key), Some(value)) = (doc.scalar(k), node_to_value(doc, v)) {
                    out.insert(key, value);
                }
            }
            Some(out)
        })
        .collect()
}

/// True when every shared, non-matrix-only key in `matrix` also matches
/// `include`. Port of `commonKeysMatch2`.
fn include_matches(
    matrix: &BTreeMap<String, Value>,
    include: &BTreeMap<String, Value>,
    base: &BTreeMap<String, Vec<Value>>,
) -> bool {
    matrix.iter().all(|(key, value)| {
        let use_key = base.contains_key(key);
        match include.get(key) {
            Some(other) if use_key => other == value,
            _ => true,
        }
    })
}

/// The Cartesian product of `key -> values`, as `common.CartesianProduct`.
pub fn cartesian_product(input: &BTreeMap<String, Vec<Value>>) -> Vec<BTreeMap<String, Value>> {
    if input.is_empty() || input.values().any(|values| values.is_empty()) {
        return Vec::new();
    }
    let keys: Vec<&String> = input.keys().collect();
    let lists: Vec<&Vec<Value>> = input.values().collect();

    let total: usize = lists.iter().map(|l| l.len()).product();
    let mut out = Vec::with_capacity(total);
    let mut counters = vec![0usize; keys.len()];

    'outer: loop {
        let mut entry = BTreeMap::new();
        for (index, key) in keys.iter().enumerate() {
            entry.insert((*key).clone(), lists[index][counters[index]].clone());
        }
        out.push(entry);

        // Odometer increment, last key varying fastest, as in `cartN`.
        let mut position = keys.len();
        loop {
            if position == 0 {
                break 'outer;
            }
            position -= 1;
            counters[position] += 1;
            if counters[position] < lists[position].len() {
                break;
            }
            counters[position] = 0;
        }
    }

    out
}

fn node_to_value(doc: &Document, id: NodeId) -> Option<Value> {
    let node = doc.node(id)?;
    match node.kind {
        crate::yaml_node::NodeKind::Scalar => match node.value.as_str() {
            "true" | "True" | "TRUE" => Some(Value::Bool(true)),
            "false" | "False" | "FALSE" => Some(Value::Bool(false)),
            "null" | "Null" | "NULL" | "" => Some(Value::Null),
            other => {
                if let Ok(i) = other.parse::<i64>() {
                    Some(Value::Int(i))
                } else if let Ok(f) = other.parse::<f64>() {
                    Some(Value::Float(f))
                } else {
                    Some(Value::String(other.to_string()))
                }
            }
        },
        crate::yaml_node::NodeKind::Sequence => {
            let mut items = Vec::with_capacity(node.content.len());
            for child in &node.content {
                items.push(node_to_value(doc, *child)?);
            }
            Some(Value::Array(items))
        }
        crate::yaml_node::NodeKind::Mapping => {
            let mut map = BTreeMap::new();
            for (key, value) in doc.map_entries(id) {
                map.insert(doc.scalar(key)?, node_to_value(doc, value)?);
            }
            Some(Value::Object(map))
        }
        _ => None,
    }
}

fn node_body_text(doc: &Document, id: NodeId) -> Option<String> {
    let node = doc.node(id)?;
    match node.kind {
        crate::yaml_node::NodeKind::Scalar => Some(node.value.clone()),
        _ => node_to_value(doc, id).map(|value| format!("{value:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yaml_node::Document;

    fn workflow(source: &str) -> (Workflow, Document) {
        let mut doc = Document::parse(source).expect("parses");
        doc.resolve_aliases().expect("resolves");
        let wf = Workflow::from_document("test.yml", &doc).expect("decodes");
        (wf, doc)
    }

    #[test]
    fn decodes_name_env_and_jobs() {
        let (wf, _) = workflow("name: CI\nenv:\n  A: b\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo hi\n");
        assert_eq!(wf.name, "CI");
        assert_eq!(wf.env.get("A").map(String::as_str), Some("b"));
        assert_eq!(wf.jobs.len(), 1);
        assert!(wf.jobs.contains_key("build"));
        assert_eq!(wf.jobs["build"].steps.len(), 1);
    }

    #[test]
    fn on_scalar_form() {
        let (wf, doc) = workflow("on: push\njobs: {}\n");
        assert_eq!(wf.on(&doc), vec!["push"]);
    }

    #[test]
    fn on_sequence_form() {
        let (wf, doc) = workflow("on: [push, pull_request]\njobs: {}\n");
        assert_eq!(wf.on(&doc), vec!["push", "pull_request"]);
    }

    #[test]
    fn on_mapping_form() {
        let (wf, doc) = workflow("on:\n  push:\n    branches: [main]\n  release:\n    tags: ['v*']\njobs: {}\n");
        let mut events = wf.on(&doc);
        events.sort();
        assert_eq!(events, vec!["push", "release"]);
        assert!(wf.on_event(&doc, "push").is_some());
        assert!(wf.on_event(&doc, "nope").is_none());
    }

    #[test]
    fn needs_accepts_scalar_and_sequence() {
        let (single, doc) = workflow("jobs:\n  a:\n    needs: b\n  b:\n    steps: []\n");
        assert_eq!(single.jobs["a"].needs(&doc), vec!["b"]);

        let (multi, doc) = workflow("jobs:\n  a:\n    needs: [b, c]\n  b: {}\n  c: {}\n");
        assert_eq!(multi.jobs["a"].needs(&doc), vec!["b", "c"]);
    }

    #[test]
    fn runs_on_accepts_all_three_forms() {
        let (scalar, doc) = workflow("jobs:\n  a:\n    runs-on: ubuntu-latest\n");
        assert_eq!(scalar.jobs["a"].runs_on(&doc), vec!["ubuntu-latest"]);

        let (list, doc) = workflow("jobs:\n  a:\n    runs-on: [self-hosted, linux]\n");
        assert_eq!(list.jobs["a"].runs_on(&doc), vec!["self-hosted", "linux"]);

        let (mapping, doc) =
            workflow("jobs:\n  a:\n    runs-on:\n      group: my-group\n      labels: [x86]\n");
        assert_eq!(mapping.jobs["a"].runs_on(&doc), vec!["x86", "my-group"]);
    }

    #[test]
    fn container_accepts_scalar_and_mapping() {
        let (scalar, doc) = workflow("jobs:\n  a:\n    container: node:18\n");
        assert_eq!(scalar.jobs["a"].container(&doc).unwrap().image, "node:18");

        let (mapping, doc) = workflow(
            "jobs:\n  a:\n    container:\n      image: node:18\n      env:\n        X: '1'\n      ports: [3000]\n      options: --cpus 2\n",
        );
        let spec = mapping.jobs["a"].container(&doc).unwrap();
        assert_eq!(spec.image, "node:18");
        assert_eq!(spec.env.get("X").map(String::as_str), Some("1"));
        assert_eq!(spec.ports, vec!["3000"]);
        assert_eq!(spec.options, "--cpus 2");
    }

    #[test]
    fn secrets_inherit_and_explicit() {
        let (inherits, doc) = workflow("jobs:\n  a:\n    secrets: inherit\n");
        assert!(inherits.jobs["a"].inherit_secrets(&doc));
        assert!(inherits.jobs["a"].secrets(&doc).is_empty());

        let (explicit, doc) = workflow("jobs:\n  a:\n    secrets:\n      token: ${{ secrets.T }}\n");
        assert!(!explicit.jobs["a"].inherit_secrets(&doc));
        assert_eq!(explicit.jobs["a"].secrets(&doc).len(), 1);
    }

    #[test]
    fn services_are_decoded() {
        let (wf, _) = workflow(
            "jobs:\n  a:\n    services:\n      db:\n        image: postgres:15\n        ports: ['5432:5432']\n",
        );
        let db = &wf.jobs["a"].services["db"];
        assert_eq!(db.image, "postgres:15");
        assert_eq!(db.ports, vec!["5432:5432"]);
    }

    #[test]
    fn matrix_expands_as_a_cross_product() {
        let (wf, doc) = workflow(
            "jobs:\n  a:\n    strategy:\n      matrix:\n        os: [linux, mac]\n        node: [18, 20]\n",
        );
        let matrixes = wf.jobs["a"].get_matrixes(&doc).expect("expands");
        assert_eq!(matrixes.len(), 4);
        assert!(matrixes.iter().any(|m| m["os"] == Value::String("mac".into())
            && m["node"] == Value::Int(20)));
    }

    #[test]
    fn matrix_exclude_removes_combinations() {
        let (wf, doc) = workflow(
            "jobs:\n  a:\n    strategy:\n      matrix:\n        os: [linux, mac]\n        node: [18, 20]\n        exclude:\n          - os: mac\n            node: 18\n",
        );
        let matrixes = wf.jobs["a"].get_matrixes(&doc).expect("expands");
        assert_eq!(matrixes.len(), 3);
        assert!(!matrixes
            .iter()
            .any(|m| m["os"] == Value::String("mac".into()) && m["node"] == Value::Int(18)));
    }

    #[test]
    fn matrix_exclude_with_unknown_key_is_an_error() {
        let (wf, doc) = workflow(
            "jobs:\n  a:\n    strategy:\n      matrix:\n        os: [linux]\n        exclude:\n          - nope: 1\n",
        );
        let err = wf.jobs["a"]
            .get_matrixes(&doc)
            .expect_err("unknown exclude key must fail");
        assert!(
            err.message.contains("does not match any key"),
            "got {}",
            err.message
        );
    }

    #[test]
    fn matrix_include_extends_matching_entries() {
        let (wf, doc) = workflow(
            "jobs:\n  a:\n    strategy:\n      matrix:\n        os: [linux, mac]\n        include:\n          - os: linux\n            extra: yes\n",
        );
        let matrixes = wf.jobs["a"].get_matrixes(&doc).expect("expands");
        assert_eq!(matrixes.len(), 2);
        assert!(matrixes[0].contains_key("extra"));
    }

    #[test]
    fn matrix_include_adds_unmatched_entries() {
        let (wf, doc) = workflow(
            "jobs:\n  a:\n    strategy:\n      matrix:\n        os: [linux]\n        include:\n          - os: windows\n",
        );
        let matrixes = wf.jobs["a"].get_matrixes(&doc).expect("expands");
        assert_eq!(matrixes.len(), 2);
    }

    #[test]
    fn include_and_exclude_are_not_matrix_dimensions() {
        // Regression: leaving `include`/`exclude` in the base matrix turns them
        // into dimensions and multiplies every combination.
        let (wf, doc) = workflow(
            "jobs:\n  a:\n    strategy:\n      matrix:\n        os: [linux, mac]\n        include:\n          - os: linux\n            extra: '1'\n        exclude:\n          - os: mac\n",
        );
        let job = &wf.jobs["a"];
        let base = job.matrix(&doc);
        assert!(
            !base.contains_key("include") && !base.contains_key("exclude"),
            "got {base:?}"
        );

        let matrixes = job.get_matrixes(&doc).expect("expands");
        // 2 os values, `exclude` removes mac, `include` merges `extra` into the
        // surviving linux row. One row, not two.
        assert_eq!(matrixes.len(), 1, "got {matrixes:?}");
        assert_eq!(matrixes[0]["os"], Value::String("linux".to_string()));
        assert!(matrixes[0].contains_key("extra"));
        assert!(matrixes.iter().all(|m| !m.contains_key("include")));
    }

    #[test]
    fn no_strategy_yields_one_empty_matrix() {
        let (wf, doc) = workflow("jobs:\n  a: {}\n");
        let matrixes = wf.jobs["a"].get_matrixes(&doc).expect("expands");
        assert_eq!(matrixes.len(), 1);
        assert!(matrixes[0].is_empty());
    }

    #[test]
    fn max_parallel_defaults_to_four() {
        let (wf, _) = workflow("jobs:\n  a: {}\n");
        assert_eq!(wf.jobs["a"].max_parallel(), 4);
        let (wf, _) = workflow("jobs:\n  a:\n    strategy:\n      max-parallel: 2\n");
        assert_eq!(wf.jobs["a"].max_parallel(), 2);
    }

    #[test]
    fn fail_fast_defaults_to_true() {
        let (wf, _) = workflow("jobs:\n  a: {}\n");
        assert!(wf.jobs["a"].fail_fast());
        let (wf, _) = workflow("jobs:\n  a:\n    strategy:\n      fail-fast: false\n");
        assert!(!wf.jobs["a"].fail_fast());
    }

    /// Every row of the table on [`Job::fail_fast`], as upstream v0.2.89
    /// actually answered it.
    ///
    /// The two that matter are the unparseable ones. Returning `true` and `4`
    /// there — which is what the code said before this was measured — is the
    /// reading Go's source invites and the one upstream does not implement,
    /// because the assignment that carries the error also carries the zero
    /// value. The `  3  ` row pins the second half: `Atoi` does not trim.
    #[test]
    fn an_unparseable_strategy_value_gives_the_zero_value_not_the_default() {
        let table: &[(&str, &str, bool, i64)] = &[
            ("", "", true, 4),
            ("maybe", "lots", false, 0),
            ("yes", "3.5", false, 0),
            ("0", "007", false, 7),
            ("True", "  3  ", true, 0),
        ];
        for (fail_fast, max_parallel, want_fail_fast, want_max_parallel) in table {
            let (wf, _) = workflow(&format!(
                "jobs:\n  a:\n    strategy:\n      fail-fast: '{fail_fast}'\n      max-parallel: '{max_parallel}'\n"
            ));
            let job = &wf.jobs["a"];
            assert_eq!(
                job.fail_fast(),
                *want_fail_fast,
                "fail-fast: {fail_fast:?}"
            );
            assert_eq!(
                job.max_parallel(),
                *want_max_parallel,
                "max-parallel: {max_parallel:?}"
            );
        }
    }

    #[test]
    fn get_env_prefixes_with_inputs() {
        let (wf, doc) = workflow(
            "jobs:\n  a:\n    steps:\n      - uses: actions/checkout@v4\n        with:\n          repo-token: ${{ secrets.T }}\n          fetch-depth: '0'\n",
        );
        let env = wf.jobs["a"].steps[0].get_env(&doc);
        assert_eq!(env.get("INPUT_REPO-TOKEN").map(String::as_str), Some("${{ secrets.T }}"));
        assert_eq!(env.get("INPUT_FETCH-DEPTH").map(String::as_str), Some("0"));
    }

    #[test]
    fn shell_command_matches_the_runner_reference() {
        let mut step = Step {
            shell: String::new(),
            ..Step::default()
        };
        assert_eq!(step.shell_command(), "bash -e {0}");

        step.shell = "bash".to_string();
        assert_eq!(step.shell_command(), "bash -e {0}");
        step.workflow_shell = "bash".to_string();
        assert_eq!(step.shell_command(), "bash --noprofile --norc -e -o pipefail {0}");

        step = Step {
            shell: "pwsh".to_string(),
            ..Step::default()
        };
        assert_eq!(step.shell_command(), "pwsh -command . '{0}'");

        step = Step {
            shell: "python".to_string(),
            ..Step::default()
        };
        assert_eq!(step.shell_command(), "python {0}");
    }

    #[test]
    fn step_type_classification() {
        let run = Step {
            run: "echo".to_string(),
            ..Step::default()
        };
        assert_eq!(run.step_type(), StepType::Run);

        let docker = Step {
            uses: "docker://alpine:3".to_string(),
            ..Step::default()
        };
        assert_eq!(docker.step_type(), StepType::UsesDockerUrl);

        let local = Step {
            uses: "./my-action".to_string(),
            ..Step::default()
        };
        assert_eq!(local.step_type(), StepType::UsesActionLocal);

        let remote = Step {
            uses: "actions/checkout@v4".to_string(),
            ..Step::default()
        };
        assert_eq!(remote.step_type(), StepType::UsesActionRemote);

        let empty = Step::default();
        assert_eq!(empty.step_type(), StepType::Invalid);
    }

    #[test]
    fn job_type_classification() {
        let plain = Job::default();
        assert_eq!(plain.job_type().unwrap(), JobType::Default);

        let local = Job {
            uses: "./.github/workflows/x.yml".to_string(),
            ..Job::default()
        };
        assert_eq!(local.job_type().unwrap(), JobType::ReusableWorkflowLocal);

        let remote = Job {
            uses: "org/repo/.github/workflows/x.yml@v1".to_string(),
            ..Job::default()
        };
        assert_eq!(remote.job_type().unwrap(), JobType::ReusableWorkflowRemote);

        let bad = Job {
            uses: "nonsense".to_string(),
            ..Job::default()
        };
        assert!(bad.job_type().is_err());
    }

    #[test]
    fn step_display_name_precedence() {
        let step = Step {
            name: "My step".to_string(),
            uses: "actions/checkout@v4".to_string(),
            run: "echo".to_string(),
            id: "s1".to_string(),
            ..Step::default()
        };
        assert_eq!(step.display_name(), "My step");

        let uses = Step {
            uses: "actions/checkout@v4".to_string(),
            run: "echo".to_string(),
            ..Step::default()
        };
        assert_eq!(uses.display_name(), "actions/checkout@v4");

        let id_only = Step {
            id: "s1".to_string(),
            ..Step::default()
        };
        assert_eq!(id_only.display_name(), "s1");
    }

    #[test]
    fn cartesian_product_orders_last_key_fastest() {
        let input = BTreeMap::from([
            ("a".to_string(), vec![Value::Int(1), Value::Int(2)]),
            ("b".to_string(), vec![Value::Int(3), Value::Int(4)]),
        ]);
        let product = cartesian_product(&input);
        assert_eq!(product.len(), 4);
        assert_eq!(product[0]["a"], Value::Int(1));
        assert_eq!(product[0]["b"], Value::Int(3));
        assert_eq!(product[1]["a"], Value::Int(1));
        assert_eq!(product[1]["b"], Value::Int(4));
        assert_eq!(product[2]["a"], Value::Int(2));
    }

    #[test]
    fn cartesian_product_of_nothing_is_empty() {
        assert!(cartesian_product(&BTreeMap::new()).is_empty());
        let input = BTreeMap::from([("a".to_string(), Vec::new())]);
        assert!(cartesian_product(&input).is_empty());
    }

    #[test]
    fn workflow_dispatch_inputs_are_decoded() {
        let (wf, doc) = workflow(
            "on:\n  workflow_dispatch:\n    inputs:\n      version:\n        description: tag\n        required: true\n        default: '1.0'\n        type: string\n        options: ['1.0', '2.0']\njobs: {}\n",
        );
        let inputs = wf
            .workflow_dispatch_inputs(&doc)
            .expect("dispatchable");
        let version = &inputs["version"];
        assert_eq!(version.description, "tag");
        assert!(version.required);
        assert_eq!(version.default, "1.0");
        assert_eq!(version.input_type, "string");
        assert_eq!(version.options, vec!["1.0", "2.0"]);
    }

    #[test]
    fn bare_workflow_dispatch_has_no_inputs() {
        let (wf, doc) = workflow("on: [workflow_dispatch]\njobs: {}\n");
        let inputs = wf.workflow_dispatch_inputs(&doc).expect("dispatchable");
        assert!(inputs.is_empty());
    }

    #[test]
    fn anchors_resolve_before_decoding() {
        let (wf, doc) = workflow(
            "env: &base\n  A: '1'\njobs:\n  a:\n    runs-on: ubuntu-latest\n    env: *base\n",
        );
        assert_eq!(wf.jobs["a"].environment(&doc).get("A").map(String::as_str), Some("1"));
    }
}
