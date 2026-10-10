// Origin: CTOX
// License: AGPL-3.0-only

//! Sealed current physical-publication scope for an original holding controller.
//! The guarded send composes its exact connection/token lifetime with this
//! retained issuer/Core/Policy scope across physical polls. This guard never
//! reenters Source transport or snapshots secrets; Models prepares those first.
use super::*;
use crate::business_os::consumer_authority::NativeConsumerCorePublication;
use rxdb::{
    plugins::replication_webrtc::WebRTCPublicationGuard,
    rx_error::{new_rx_error, RxResult},
};
use std::sync::Arc;

/// Borrowed only inside the real original-controller publication reservation.
/// No constructor, Deserialize, Clone or generic Boolean permit is exported.
pub(crate) struct NativeSupervisorCurrentPublication<'a> {
    controller: &'a Arc<NativeSupervisorHoldingController>,
    facts: &'a ConsumerFacts,
    policy: &'a Connection,
}
impl NativeSupervisorCurrentPublication<'_> {
    pub(crate) fn controller(&self) -> &Arc<NativeSupervisorHoldingController> {
        self.controller
    }
    pub(crate) fn facts(&self) -> &ConsumerFacts {
        self.facts
    }
    pub(crate) fn policy(&self) -> &Connection {
        self.policy
    }
}

/// Models supplies its prepared PRIVATE account/configuration guard. It must
/// validate against this held native policy view and hold its own retirement
/// fence across publish; no awaits, network, secret/transport reentry or retained
/// connections. A wire DTO cannot implement or manufacture this native scope.
pub(crate) trait NativeSupervisorPublicationCheck: Send + Sync {
    fn with_current(
        &self,
        scope: &NativeSupervisorCurrentPublication<'_>,
        publish: &mut dyn FnMut() -> RxResult<()>,
    ) -> RxResult<()>;
}
struct ControllerPublication {
    controller: Arc<NativeSupervisorHoldingController>,
    origin: NativeConsumerCorePublication,
    account: Arc<dyn NativeSupervisorPublicationCheck>,
}
impl NativeSupervisorHoldingController {
    /// Install only on the GuardedAuxiliaryResponse for this actual request.
    /// Exact transport object, connection generation, credential and retained
    /// enrollment must match the controller before a guard can be constructed.
    pub(crate) fn publication_for(
        self: &Arc<Self>,
        request: &AdmittedConsumerAuthority,
        account: Arc<dyn NativeSupervisorPublicationCheck>,
    ) -> anyhow::Result<Arc<dyn WebRTCPublicationGuard>> {
        let origin = self.authority.prepare_core_publication(request)?;
        Ok(Arc::new(ControllerPublication {
            controller: Arc::clone(self),
            origin,
            account,
        }))
    }
    fn with_publication_scope<T>(
        self: &Arc<Self>,
        facts: &ConsumerFacts,
        core: &Connection,
        policy: &Connection,
        apply: impl FnOnce(&NativeSupervisorCurrentPublication<'_>) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        anyhow::ensure!(
            !self.retired.load(Ordering::Acquire),
            unavailable("supervisor_execution_fenced", "holding controller retired")
        );
        self.lease.current(core, policy, facts)?;
        anyhow::ensure!(
            serde_json::to_string(facts)? == self.consumer_json,
            unavailable("supervisor_execution_fenced", "holder enrollment changed")
        );
        current_controller(core, &self.lease, &self.id, &self.consumer_json)?;
        apply(&NativeSupervisorCurrentPublication {
            controller: self,
            facts,
            policy,
        })
    }
}
impl WebRTCPublicationGuard for ControllerPublication {
    fn with_current(&self, publish: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
        self.origin
            .with_current(|facts, core, policy| {
                self.controller
                    .with_publication_scope(facts, core, policy, |scope| {
                        let mut invoked = false;
                        self.account
                            .with_current(scope, &mut || {
                                if invoked {
                                    return Err(new_rx_error(
                                        "supervisor_publication_repeated",
                                        None,
                                    ));
                                }
                                invoked = true;
                                publish()
                            })
                            .map_err(|_| anyhow::anyhow!("private account publication retired"))?;
                        anyhow::ensure!(invoked, "private account publication callback missing");
                        Ok(())
                    })
            })
            .map_err(|_| new_rx_error("supervisor_execution_fenced", None))
    }
}
