#[cfg(test)]
mod tests {
    // These exercise the production admission with the real native worker SQL
    // fence. Quorum/controller are bounded fixtures, not two-host acceptance.
    use super::super::super::{NativeProviderCheckpointBinding, NativeProviderTurnOwner};
    use super::super::*;
    use ctox_sync::authority::WorkerMembership;
    use std::{
        io,
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Mutex,
        },
    };

    struct Owner {
        revoked: AtomicBool,
        destination: NativeGuestAdmissionDestination,
    }
    impl NativeGuestAdmissionOwner for Owner {
        fn with_current_destination(
            &self,
            _tx: &Transaction<'_>,
            _facts: &NativeProviderFacts,
            expected: Option<&NativeGuestAdmissionDestination>,
            publish: &mut dyn FnMut(&NativeGuestAdmissionDestination) -> Result<()>,
        ) -> Result<()> {
            ensure!(
                !self.revoked.load(Ordering::Acquire),
                "fixture policy revoked"
            );
            ensure!(
                expected.is_none_or(|d| d == &self.destination),
                "fixture destination changed"
            );
            publish(&self.destination)
        }
    }
    struct Quorum {
        owner: Arc<Owner>,
        revoke_after_create: bool,
        replay: bool,
        calls: AtomicUsize,
        job: Mutex<Option<Job>>,
    }
    // Spelling out async-trait's boxed ABI avoids introducing a dependency
    // into the daemon solely for this bounded authority fixture.
    impl ExecutionAuthority for Quorum {
        fn node_id(&self) -> u64 {
            4
        }
        fn scope_id(&self) -> &str {
            "native-scope"
        }
        fn worker_membership<'a, 'f>(
            &'a self,
            _: u64,
        ) -> Pin<Box<dyn Future<Output = io::Result<Option<WorkerMembership>>> + Send + 'f>>
        where
            'a: 'f,
            Self: 'f,
        {
            Box::pin(async { Ok(None) })
        }
        fn submit<'a, 'f>(
            &'a self,
            request: Request,
        ) -> Pin<Box<dyn Future<Output = io::Result<Receipt>> + Send + 'f>>
        where
            'a: 'f,
            Self: 'f,
        {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                let Command::Create { spec, owner } = request.command else {
                    return Err(io::Error::other("fixture expected Create"));
                };
                assert_eq!(owner, 4);
                let job = Job {
                    spec,
                    ownership: Ownership {
                        node_id: 4,
                        generation: 91,
                    },
                    checkpoint: None,
                    pending_effects: BTreeSet::new(),
                    completed_effects: BTreeSet::new(),
                    stopped: false,
                };
                *self.job.lock().unwrap() = Some(job.clone());
                tokio::task::yield_now().await;
                if self.revoke_after_create {
                    self.owner.revoked.store(true, Ordering::Release);
                }
                Ok(if self.replay {
                    Receipt::Replayed(job)
                } else {
                    Receipt::Applied(job)
                })
            })
        }
        fn validate_ownership<'a, 'b, 'c, 'f>(
            &'a self,
            id: &'b str,
            ownership: &'c Ownership,
        ) -> Pin<Box<dyn Future<Output = io::Result<Job>> + Send + 'f>>
        where
            'a: 'f,
            'b: 'f,
            'c: 'f,
            Self: 'f,
        {
            Box::pin(async move {
                let job = self.job.lock().unwrap().clone().unwrap();
                assert_eq!(id, job.spec.job_id);
                assert_eq!(ownership, &job.ownership);
                Ok(job)
            })
        }
        fn shutdown<'a, 'f>(&'a self) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'f>>
        where
            'a: 'f,
            Self: 'f,
        {
            Box::pin(async { Ok(()) })
        }
    }

    #[tokio::test]
    async fn actual_native_admission_keeps_uncertain_or_revoked_create_pending() -> Result<()> {
        for (revoke, replay) in [(false, false), (true, false), (false, true)] {
            let (root, execution, _) =
                super::super::super::queue_provider_binding::tests::admitted()?;
            let auth = ctox_core::AuthManager::from_account_bound_runtime_auth(
                ctox_core::CodexAuth::create_dummy_chatgpt_auth_for_testing(),
                root.path().into(),
            )?;
            let checkpoint =
                NativeProviderCheckpointBinding::from_pinned_auth(auth, "actual-route")?;
            let context = serde_json::json!({"actor":"native-principal"});
            let provider = NativeProviderTurnOwner::prepare_with_checkpoint(
                &execution,
                &uuid::Uuid::new_v4().to_string(),
                "actual-model",
                Some("actual-route"),
                None,
                Some(&context),
                Some(&checkpoint),
            )?;
            let owner = Arc::new(Owner {
                revoked: AtomicBool::new(false),
                destination: NativeGuestAdmissionDestination {
                    instance_id: "native-instance".into(),
                    project_id: "native-project".into(),
                    human_owner_id: "native-principal".into(),
                    guest_id: "native-guest".into(),
                    worker_profile_id: "native-profile".into(),
                    controller_id: "native-controller".into(),
                    controller_generation: 8,
                    policy_revision: "native-policy-revision".into(),
                    scope_id: "native-scope".into(),
                    required_capabilities: BTreeSet::from(["native-guest".into()]),
                },
            });
            let quorum = Arc::new(Quorum {
                owner: owner.clone(),
                revoke_after_create: revoke,
                replay,
                calls: AtomicUsize::new(0),
                job: Mutex::new(None),
            });
            let admission = NativeGuestAdmission::new(quorum.clone(), owner)?;
            let result = provider.binding().admit_before_start(&admission).await;
            assert_eq!(result.is_ok(), !revoke && !replay);
            assert_eq!(quorum.calls.load(Ordering::SeqCst), 1);
            provider.binding().with_live_provider_transaction(|tx, facts, turn| {
                assert!(turn.is_none());
                let (phase, ownership): (String, Option<String>) = tx.query_row(
                    "SELECT phase,ownership_json FROM native_guest_provider_admissions WHERE binding_id=?1",
                    [&facts.binding_id], |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                assert_eq!(phase, if revoke || replay { "PendingCreate" } else { "Admitted" });
                assert_eq!(ownership.is_some(), !revoke && !replay);
                if let Some(value) = ownership {
                    assert_eq!(serde_json::from_str::<Ownership>(&value)?.generation, 91);
                }
                Ok(())
            })?;
            // A second call must fail on existing attempt evidence BEFORE any
            // second Create, even when the first response was uncertain/replayed.
            assert!(provider
                .binding()
                .admit_before_start(&admission)
                .await
                .is_err());
            assert_eq!(quorum.calls.load(Ordering::SeqCst), 1);
        }
        Ok(())
    }

    #[tokio::test]
    async fn foreign_native_principal_cannot_create_quorum_job() -> Result<()> {
        let (root, execution, _) = super::super::super::queue_provider_binding::tests::admitted()?;
        let auth = ctox_core::AuthManager::from_account_bound_runtime_auth(
            ctox_core::CodexAuth::create_dummy_chatgpt_auth_for_testing(),
            root.path().into(),
        )?;
        let checkpoint = NativeProviderCheckpointBinding::from_pinned_auth(auth, "actual-route")?;
        let context = serde_json::json!({"actor":"foreign-principal"});
        let provider = NativeProviderTurnOwner::prepare_with_checkpoint(
            &execution,
            &uuid::Uuid::new_v4().to_string(),
            "actual-model",
            Some("actual-route"),
            None,
            Some(&context),
            Some(&checkpoint),
        )?;
        let owner = Arc::new(Owner {
            revoked: AtomicBool::new(false),
            destination: NativeGuestAdmissionDestination {
                instance_id: "native-instance".into(),
                project_id: "native-project".into(),
                human_owner_id: "native-principal".into(),
                guest_id: "native-guest".into(),
                worker_profile_id: "native-profile".into(),
                controller_id: "native-controller".into(),
                controller_generation: 8,
                policy_revision: "native-policy-revision".into(),
                scope_id: "native-scope".into(),
                required_capabilities: BTreeSet::from(["native-guest".into()]),
            },
        });
        let quorum = Arc::new(Quorum {
            owner: owner.clone(),
            revoke_after_create: false,
            replay: false,
            calls: AtomicUsize::new(0),
            job: Mutex::new(None),
        });
        let admission = NativeGuestAdmission::new(quorum.clone(), owner)?;
        assert!(provider
            .binding()
            .admit_before_start(&admission)
            .await
            .is_err());
        assert_eq!(quorum.calls.load(Ordering::SeqCst), 0);
        Ok(())
    }
}
