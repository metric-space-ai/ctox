// Origin: CTOX
// License: AGPL-3.0-only
//! Reconcile only the exact opaque quiesced QEMU process, never Core/tool effects.
use super::*;
use ctox_sync::authority::{Command, Job, Receipt, Request};

fn completed_process_job(job: &Job, process: &GuestProcessEffect) -> Result<Job> {
    ensure!(
        !job.stopped
            && job.spec.job_id == process.job_id
            && job.ownership == process.ownership
            && job.pending_effects.len() == 1
            && job.pending_effects.contains(&process.effect_id)
            && !job.completed_effects.contains(&process.effect_id),
        "source process does not have its exact sole pending quorum effect"
    );
    let mut completed = job.clone();
    completed.pending_effects.remove(&process.effect_id);
    completed
        .completed_effects
        .insert(process.effect_id.clone());
    // CompleteEffect does not refresh a checkpoint or certify other effects.
    Ok(completed)
}

impl NativeGuestExecution {
    pub(super) fn reconcile_source_process(
        &self,
        source: &crate::channels::NativeProviderCaptureOwner,
    ) -> Result<()> {
        ensure!(
            source.matches_provider(&self.provider),
            "foreign native capture owner"
        );
        let mut before = source_effects::SourceEffects::observe(self)?;
        let attempt = source.with_current_capture_transaction(|worker, facts| {
            self.with_held_worker_policy(worker, facts, |entry, verify, _| {
                self.registry.require_live_transport()?;
                verify()?;
                before.verify_controller(entry, &self.registry)?;
                let Some(process) = entry.registered_process.clone() else {
                    return Ok(None);
                };
                let capture = entry
                    .source_machine
                    .as_ref()
                    .context("source process needs its opaque completed machine export")?;
                ensure!(
                    entry.desktop.is_none() && capture.matches(&process),
                    "source process export changed"
                );
                if capture.process_reconciled(&process)? {
                    ensure!(
                        before
                            .quorum_job()
                            .completed_effects
                            .contains(&process.effect_id),
                        "reconciled source process is not completed in current quorum"
                    );
                    return Ok(None);
                }
                let expected = completed_process_job(before.quorum_job(), &process)?;
                // Retain uncertainty before remote submission; a lost response,
                // cancellation or failed fresh guard never certifies completion.
                capture.begin_reconciliation(&process)?;
                Ok(Some((Arc::clone(capture), process, expected)))
            })
        })?;
        let Some((capture, process, expected)) = attempt else {
            return Ok(());
        };
        // No native worker/account/SQLite/controller or machine IO lock survives
        // either await. The child has already cleanly quit and been reaped.
        let mut current = super::super::guest_commands::block_on_guest(async {
            let receipt = tokio::time::timeout(
                Duration::from_secs(15),
                self.registry.authority.submit(Request {
                    request_id: format!("{}:quiesced", process.effect_id),
                    actor: self.registry.authority.node_id(),
                    command: Command::CompleteEffect {
                        job_id: self.binding.spec.job_id.clone(),
                        ownership: self.binding.ownership.clone(),
                        effect_id: process.effect_id.clone(),
                    },
                }),
            )
            .await
            .context("source process reconciliation deadline")??;
            ensure!(
                matches!(receipt, Receipt::Applied(ref job) if *job == expected),
                "source process completion was not freshly applied; reconcile"
            );
            let current = source_effects::SourceEffects::read_authority(
                self.registry.authority.as_ref(),
                &self.binding.spec,
                &self.binding.ownership,
            )
            .await?;
            ensure!(
                current.quorum_job() == &expected,
                "source process quorum changed during completion; reconcile"
            );
            Ok::<_, anyhow::Error>(current)
        })?;
        source.with_current_capture_transaction(|worker, facts| {
            self.with_held_worker_policy(worker, facts, |entry, verify, _| {
                self.registry.require_live_transport()?;
                verify()?;
                ensure!(
                    entry.registered_process.as_ref() == Some(&process)
                        && entry.process_effect.as_deref() == Some(process.effect_id.as_str())
                        && entry.desktop.is_none()
                        && entry
                            .source_machine
                            .as_ref()
                            .is_some_and(|actual| Arc::ptr_eq(actual, &capture)),
                    "source process/controller changed during completion"
                );
                capture.finish_reconciliation(&process)?;
                current.verify_controller(entry, &self.registry)?;
                verify()?;
                Ok(())
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_completion_changes_only_its_exact_sole_pending_reservation() {
        let spec = ExecutionSpec {
            job_id: "source-job".into(),
            session_id: uuid::Uuid::new_v4().to_string(),
            scope_id: "scope".into(),
            harness: ctox_core::native_harness_name().into(),
            harness_version: ctox_core::native_harness_version().into(),
            model_route_id: "openai".into(),
            gateway_account_id: "fixture".into(),
            model_id: "model".into(),
            required_capabilities: BTreeSet::new(),
        };
        let process = GuestProcessEffect {
            effect_id: "exact-child".into(),
            job_id: spec.job_id.clone(),
            ownership: Ownership {
                node_id: 1,
                generation: 2,
            },
            controller_id: "controller".into(),
            controller_generation: 3,
            process_instance_id: "actual-child".into(),
        };
        let job = Job {
            spec,
            ownership: process.ownership.clone(),
            checkpoint: None,
            checkpoint_requires_refresh: true,
            pending_effects: BTreeSet::from([process.effect_id.clone()]),
            completed_effects: BTreeSet::from(["earlier-effect".into()]),
            stopped: false,
        };
        let completed = completed_process_job(&job, &process).unwrap();
        assert!(completed.pending_effects.is_empty());
        assert_eq!(
            completed.completed_effects,
            BTreeSet::from(["earlier-effect".into(), process.effect_id.clone()])
        );
        assert!(completed.checkpoint_requires_refresh);
        assert_eq!(completed.spec, job.spec);
        assert_eq!(completed.ownership, job.ownership);
        assert!(
            completed_process_job(&completed, &process).is_err(),
            "completed effects cannot be re-admitted"
        );
        for mutation in 0..7 {
            let mut bad = job.clone();
            match mutation {
                0 => bad.stopped = true,
                1 => bad.spec.job_id = "foreign-job".into(),
                2 => bad.ownership.node_id += 1,
                3 => bad.ownership.generation += 1,
                4 => {
                    bad.pending_effects.clear();
                }
                5 => {
                    bad.pending_effects.insert("unknown-external".into());
                }
                _ => {
                    bad.completed_effects.insert(process.effect_id.clone());
                }
            }
            assert!(completed_process_job(&bad, &process).is_err());
        }
    }
}
