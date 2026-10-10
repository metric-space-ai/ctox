//! Native step preparation and fail-closed workflow inspection.
//!
//! No process is spawned here. The managed executor owns admission, limits,
//! file commands and native spawning.
use std::collections::BTreeMap;
use std::rc::Rc;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::container::{ExecutionsEnvironment, HostEnvironment};
use crate::model::{GitLookups, Run, Step, Workflow};
use crate::runner::{run_context::RunContext, step_run};
use crate::yaml_node::Document;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostGapKind {
    Container,
    Services,
    DockerAction,
    ActionLoader,
    ReusableWorkflow,
    Concurrency,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostGap {
    pub job: Option<String>,
    pub step: Option<usize>,
    pub kind: HostGapKind,
    pub reason: String,
}

/// Parsed workflow and explicit execution gaps. Parsing is not execution.
pub struct HostWorkflow {
    pub workflow: Workflow,
    pub document: Rc<Document>,
    pub gaps: Vec<HostGap>,
}

impl HostWorkflow {
    pub fn parse(path: &str, source: &str) -> Result<Self> {
        let mut document = Document::parse(source)?;
        document.resolve_aliases()?;
        let workflow = Workflow::from_document(path, &document)?;
        let mut gaps = Vec::new();
        let root = document.root().ok_or_else(|| anyhow!("empty workflow"))?;
        if document.map_get(root, "concurrency").is_some() {
            gaps.push(HostGap {
                job: None, step: None, kind: HostGapKind::Concurrency,
                reason: "concurrency requires the Workjet scheduler".into(),
            });
        }
        let jobs = document.map_get(root, "jobs");
        for (id, job) in &workflow.jobs {
            let mut gap = |kind, step, reason: &str| gaps.push(HostGap {
                job: Some(id.clone()), step, kind, reason: reason.into(),
            });
            if job.raw_container.is_some() {
                gap(HostGapKind::Container, None, "container: is prohibited; use native toolchains");
            }
            let node = jobs.and_then(|jobs| document.map_get(jobs, id));
            if node.and_then(|node| document.map_get(node, "services")).is_some() {
                gap(HostGapKind::Services, None, "services: is prohibited; no container fallback");
            }
            if node.and_then(|node| document.map_get(node, "concurrency")).is_some() {
                gap(HostGapKind::Concurrency, None, "job concurrency requires the Workjet scheduler");
            }
            if !job.uses.is_empty() {
                gap(HostGapKind::ReusableWorkflow, None, "reusable workflow execution is not implemented");
            }
            for (index, step) in job.steps.iter().enumerate() {
                if step.uses.starts_with("docker://") {
                    gap(HostGapKind::DockerAction, Some(index), "Docker actions are prohibited");
                } else if !step.uses.is_empty() {
                    gap(HostGapKind::ActionLoader, Some(index), "JavaScript/composite action loading is not implemented");
                }
            }
        }
        Ok(Self { workflow, document: Rc::new(document), gaps })
    }

    pub fn require_no_gaps(&self) -> Result<()> {
        if self.gaps.is_empty() { return Ok(()); }
        Err(anyhow!("unsupported host workflow features: {}", serde_json::to_string(&self.gaps)?))
    }

    pub fn run(&self, job: &str) -> Result<Run> {
        if !self.workflow.jobs.contains_key(job) {
            return Err(anyhow!("unknown workflow job: {job}"));
        }
        Ok(Run::new(self.workflow.clone(), Rc::clone(&self.document), job))
    }
}

/// Pure preparation; the managed executor writes the script before spawning.
pub struct PreparedHostStep {
    pub script: step_run::AssembledScript,
    pub working_directory: String,
}

/// Reuse act's shell/defaults/expression machinery without selecting a container.
pub fn prepare_run_step(
    rc: &mut RunContext,
    git: &GitLookups,
    status: &dyn crate::expr::StatusProvider,
    step: &mut Step,
    env: &BTreeMap<String, String>,
    host: &HostEnvironment,
) -> Result<PreparedHostStep> {
    if !step.uses.is_empty() || step.run.is_empty() {
        return Err(anyhow!("expected a run: step; uses: requires an action loader"));
    }
    if step.id.is_empty() || step.id.contains(['/', '\\']) || step.id == ".." {
        return Err(anyhow!("step id must be a nonempty path component"));
    }
    if let Some(run) = &rc.run {
        let job = run.job().ok_or_else(|| anyhow!("unknown workflow job"))?;
        let has_services = run.document().root()
            .and_then(|root| run.document().map_get(root, "jobs"))
            .and_then(|jobs| run.document().map_get(jobs, &run.job_id))
            .and_then(|job| run.document().map_get(job, "services")).is_some();
        if job.raw_container.is_some() || has_services || !job.uses.is_empty() {
            return Err(anyhow!("native step cannot bypass prohibited containers/services or a reusable workflow"));
        }
    }
    rc.job_container = Some(crate::runner::run_context::ContainerPaths {
        act_path: host.act_path_string(),
        workdir: host.path.to_string_lossy().into_owned(),
        environment_case_insensitive: host.is_environment_case_insensitive(),
    });
    let (script, working_directory) =
        step_run::setup_shell_command(rc, git, status, step, env, Some(host))
            .map_err(|error| anyhow!("{error}"))?;
    Ok(PreparedHostStep { script, working_directory })
}
