// Origin: CTOX
// License: AGPL-3.0-only
//! Target-local controller for the original protected guest. No process or provider is fabricated.
use super::*;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct ProtectedEnrollment {
    pub(super) binding_digest: String,
    pub(super) checkpoint_digest: String,
    pub(super) spec: ExecutionSpec,
    pub(super) ownership: Ownership,
    pub(super) service_session: String,
}

impl NativeGuestRegistry {
    /// Called under the target's actual account/issuer/policy/host fence, after
    /// fresh quorum ownership validation. It receives no renderer guest/path IDs.
    #[cfg(target_os = "linux")]
    pub(in crate::business_os) fn enroll_protected_target(
        &self,
        policy: &Connection,
        scope: &target_handoff::TargetPolicyScope,
        identity: &super::super::guest_runtime::ProtectedGuestIdentity,
        binding_digest: &str,
        checkpoint_digest: &str,
        spec: &ExecutionSpec,
        ownership: &Ownership,
        import_parent: &Path,
    ) -> Result<NativeGuestAssignment> {
        self.require_live_transport()?;
        self.enroll_protected_target_in_policy(
            policy,
            scope,
            identity,
            binding_digest,
            checkpoint_digest,
            spec,
            ownership,
            import_parent,
        )
    }

    #[cfg(target_os = "linux")]
    pub(super) fn enroll_protected_target_in_policy(
        &self,
        policy: &Connection,
        scope: &target_handoff::TargetPolicyScope,
        identity: &super::super::guest_runtime::ProtectedGuestIdentity,
        binding_digest: &str,
        checkpoint_digest: &str,
        spec: &ExecutionSpec,
        ownership: &Ownership,
        import_parent: &Path,
    ) -> Result<NativeGuestAssignment> {
        ensure!(
            policy.path() == self.policy_path.to_str()
                && identity_file(&self.policy_path)? == self.policy_identity,
            "target enrollment borrowed another policy store"
        );
        ensure!(
            spec.scope_id == self.authority.scope_id()
                && ownership.node_id == self.authority.node_id()
                && ownership.generation > 1,
            "target enrollment needs current next ownership"
        );
        for digest in [binding_digest, checkpoint_digest] {
            ensure!(
                digest.len() == 64
                    && digest
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "invalid protected enrollment digest"
            );
        }
        let current = target_handoff::resolve(&self.runtime_root, policy, scope, spec)?;
        ensure!(current == *scope, "target enrollment policy changed");
        let restoration = ProtectedEnrollment {
            binding_digest: binding_digest.into(),
            checkpoint_digest: checkpoint_digest.into(),
            spec: spec.clone(),
            ownership: ownership.clone(),
            service_session: identity.service_session().into(),
        };
        // Retry only the exact retained original guest. Never revive a revoked
        // controller, convert a fresh job, or replace a different checkpoint.
        for retained in self
            .guests
            .lock()
            .map_err(|_| anyhow::anyhow!("native guest registry poisoned"))?
            .values()
        {
            let entry = retained
                .lock()
                .map_err(|_| anyhow::anyhow!("native controller poisoned"))?;
            let d = &entry.assignment.destination;
            if d.human_owner_id == scope.owner_user_id
                && d.project_id == scope.project_id
                && d.thread_id == scope.thread_id
                && d.worker_profile_id == scope.worker_profile_id
            {
                ensure!(
                    !entry.revoked
                        && entry.restoration.as_ref() == Some(&restoration)
                        && d.guest_id == identity.guest_id()
                        && private_directory(&d.import_parent)? == entry.import_identity,
                    "retained target guest differs or was retired; reconcile"
                );
                validate_policy(policy, d)?;
                return Ok(entry.assignment.clone());
            }
        }
        self.enroll_resolved_in_policy(
            policy,
            &scope.owner_user_id,
            &scope.project_id,
            &scope.thread_id,
            &scope.worker_profile_id,
            import_parent,
            identity.guest_id().into(),
            Some(restoration),
        )
    }
}

// Keep the file identity helper unambiguous beside the protected guest identity.
#[cfg(target_os = "linux")]
fn identity_file(path: &Path) -> Result<FileIdentity> {
    super::identity(path)
}
