// Origin: CTOX
// License: AGPL-3.0-only

//! Holder-private Claude SDK account preparation for a genuine native lease.
//! This is neither a Crew controller nor a renderer/forwarding credential.

use super::cliproxyapi_claude_catalog::{capture, fingerprint, Captured};
use crate::business_os::{
    consumer_authority::AdmittedConsumerAuthority,
    provider_federation::{capture_consumable_model, ConsumableModel, SupervisorModelEligibility},
};
use anyhow::{ensure, Context, Result};
use ctox_cliproxyapi::internal::auth::claude::SecretString;
use std::{path::PathBuf, sync::Mutex};

/// Never Serialize/Debug/Clone: the only credential carrier is a borrowed
/// native callback. Only the actual registered holder's SDK producer may use
/// it; do not put it into a browser claim response or public producer facts.
pub(crate) struct NativeClaudeSdkConfiguration<'a> {
    model: &'a str,
    access_token: &'a SecretString,
    private_binding: &'a str,
}

impl NativeClaudeSdkConfiguration<'_> {
    pub(crate) fn model(&self) -> &str {
        self.model
    }

    pub(crate) fn upstream(&self) -> &'static str {
        "https://api.anthropic.com"
    }

    pub(crate) fn access_token(&self) -> &SecretString {
        self.access_token
    }

    /// Holder-only pin for the actual provider session witness, not a public
    /// account selector and not proof that a session/turn has executed.
    pub(crate) fn private_binding(&self) -> &str {
        self.private_binding
    }
}

/// Owns the exact selected model, native account/configuration and encrypted
/// store snapshot. Release prevents every later callback and drops both
/// zeroizing credentials. The genuine producer separately stops its SDK turn;
/// release alone is not an operating-system or provider stop receipt.
pub(crate) struct NativeClaudeSdkAccountReservation {
    root: PathBuf,
    selected: ConsumableModel,
    private_binding: String,
    captured: Mutex<Option<Captured>>,
}

fn validate(captured: &Captured, expected_binding: &str) -> Result<()> {
    ensure!(
        !captured.account.disabled,
        "native Claude account is disabled"
    );
    ensure!(
        captured.account.upstream_scheme == "https"
            && captured.account.upstream_authority == "api.anthropic.com"
            && captured.account.proxy_url_secret.is_none(),
        "native Claude SDK requires the official account endpoint"
    );
    let access = captured.credentials.access_token().expose_secret();
    ensure!(
        !access.trim().is_empty() && access.len() <= 8192 && !access.chars().any(char::is_control),
        "native Claude SDK credential is unavailable"
    );
    ensure!(
        fingerprint(captured)? == expected_binding,
        "native Claude account/configuration changed"
    );
    Ok(())
}

fn stable_capture(root: &std::path::Path, id: &str) -> Result<Captured> {
    let first = capture(root, id)
        .map_err(|_| anyhow::anyhow!("native Claude account snapshot unavailable"))?
        .context("native Claude account snapshot unavailable")?;
    ensure!(
        capture(root, id)
            .map_err(|_| anyhow::anyhow!("native Claude account snapshot unavailable"))?
            .as_ref()
            == Some(&first),
        "native Claude account changed during preparation"
    );
    Ok(first)
}

fn with_captured_current<T>(
    state: &Mutex<Option<Captured>>,
    current: &Captured,
    binding: &str,
    apply: impl FnOnce(&Captured) -> Result<T>,
) -> Result<T> {
    let mut captured = state
        .try_lock()
        .map_err(|_| anyhow::anyhow!("native Claude account reservation unavailable"))?;
    let prior = captured
        .as_ref()
        .context("native Claude account reservation released")?;
    if current != prior {
        captured.take();
        anyhow::bail!("native Claude account/configuration changed");
    }
    if let Err(error) = validate(prior, binding) {
        captured.take();
        return Err(error);
    }
    apply(prior)
}

fn release_captured(state: &Mutex<Option<Captured>>) {
    // A failed native callback must never prevent credential retirement.
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
}

impl NativeClaudeSdkAccountReservation {
    /// Crew calls outside its transaction after sealing the actual native
    /// command/confirmed-plan selection. The source is Architecture's actual
    /// admitted peer, never a browser ConsumerFacts or configured-route DTO.
    /// A private holder execution transport/registered SDK driver remains
    /// necessary; this function does not publish credentials over WebRTC.
    pub(crate) fn prepare(
        authority: &AdmittedConsumerAuthority,
        selection: &SupervisorModelEligibility,
    ) -> Result<Self> {
        ensure!(
            selection.account().provider == "claude",
            "native Claude SDK requires a Claude account"
        );
        let selected = capture_consumable_model(
            authority,
            &selection.account().account_id,
            selection.account().account_revision,
            selection.model(),
        )?;
        selection.assert_consumer_binding(&selected)?;
        let private_binding = selected
            .private_configuration_binding()
            .context("native Claude account requires current holder discovery")?
            .to_owned();
        // Root is taken only from the real admitted native host. Caller JSON
        // cannot select another secret store or reconstruct this authority.
        let root = authority.native_host_root().to_owned();
        let captured = stable_capture(&root, &selected.account().private_local_account_id)?;
        validate(&captured, &private_binding)?;
        let reservation = Self {
            root,
            selected,
            private_binding,
            captured: Mutex::new(Some(captured)),
        };
        // A change during secret preparation cannot relabel the selection.
        reservation.with_current_configuration(authority, |_| Ok(()))?;
        Ok(reservation)
    }

    /// Call after every asynchronous boundary and immediately before each
    /// genuine SDK dispatch/result publication. Crew must additionally retain
    /// its actual native lease/claim fence; this account object does not mint
    /// that lease or validate an untrusted producer receipt.
    ///
    /// All secret/configuration reads happen BEFORE entering the source,
    /// issuer and policy fences. The callback is bounded and synchronous:
    /// no await, network/secret/transport reentry, or reservation reentry.
    /// The credential must stay on the registered holder's private SDK path.
    pub(crate) fn with_current_configuration<T>(
        &self,
        authority: &AdmittedConsumerAuthority,
        apply: impl FnOnce(NativeClaudeSdkConfiguration<'_>) -> Result<T>,
    ) -> Result<T> {
        if self.root != authority.native_host_root() {
            self.release();
            anyhow::bail!("native Claude source host changed");
        }
        // Do not reload credentials for an already retired reservation.
        {
            let captured = self
                .captured
                .try_lock()
                .map_err(|_| anyhow::anyhow!("native Claude account reservation unavailable"))?;
            ensure!(
                captured.is_some(),
                "native Claude account reservation released"
            );
        }
        let current = match stable_capture(
            &self.root,
            &self.selected.account().private_local_account_id,
        ) {
            Ok(current) => current,
            Err(error) => {
                self.release();
                return Err(error);
            }
        };
        let result =
            with_captured_current(&self.captured, &current, &self.private_binding, |prior| {
                self.selected.with_current(authority, |_, selected| {
                    apply(NativeClaudeSdkConfiguration {
                        model: selected.model(),
                        access_token: prior.credentials.access_token(),
                        private_binding: &self.private_binding,
                    })
                })
            });
        if result.is_err() {
            self.release();
        }
        result
    }

    /// Use the actual Crew controller without re-entering its transport fence.
    /// Secret snapshots are prepared before source/issuer/Core/Policy entry.
    pub(crate) fn with_current_controller_configuration<T>(
        &self,
        controller: &crate::business_os::mcp_channel::NativeSupervisorHoldingController,
        apply: impl FnOnce(NativeClaudeSdkConfiguration<'_>) -> Result<T>,
    ) -> Result<T> {
        if self.root != controller.authority().native_host_root() {
            self.release();
            anyhow::bail!("native Claude source host changed");
        }
        {
            let captured = self
                .captured
                .try_lock()
                .map_err(|_| anyhow::anyhow!("native Claude account reservation unavailable"))?;
            ensure!(
                captured.is_some(),
                "native Claude account reservation released"
            );
        }
        let current = match stable_capture(
            &self.root,
            &self.selected.account().private_local_account_id,
        ) {
            Ok(current) => current,
            Err(error) => {
                self.release();
                return Err(error);
            }
        };
        let result =
            with_captured_current(&self.captured, &current, &self.private_binding, |prior| {
                controller.with_current(|facts, _, policy| {
                    self.selected.assert_current_in_policy(facts, policy)?;
                    controller
                        .selection()
                        .assert_consumer_binding(&self.selected)?;
                    apply(NativeClaudeSdkConfiguration {
                        model: self.selected.model(),
                        access_token: prior.credentials.access_token(),
                        private_binding: &self.private_binding,
                    })
                })
            });
        if result.is_err() {
            self.release();
        }
        result
    }

    /// Idempotent retirement, called on native cancellation/selection change,
    /// producer teardown and final receipt consumption. A concurrent bounded
    /// callback finishes before release; no later callback can acquire it.
    pub(crate) fn release(&self) {
        release_captured(&self.captured);
    }
}

#[cfg(test)]
#[path = "cliproxyapi_claude_sdk_tests.rs"]
mod tests;
