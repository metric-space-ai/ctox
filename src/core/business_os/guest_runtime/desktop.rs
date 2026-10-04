// Origin: CTOX
// License: AGPL-3.0-only

//! Retains the real QEMU child through bootstrap, endpoint failure and stop.
//! All calls belong inside the native controller/execution lifecycle, not intake.

use super::channel::RemoteGuestDriver;
use super::qemu::{PreparedQemuGuest, QemuProcess};
use super::{identifier, GuestDriver, GuestFrame, GuestInput};
use anyhow::{ensure, Context, Result};
use ctox_sync::guest_restore::GuestLiveEndpoint;
use std::process::ExitStatus;
use tokio::net::UnixStream;

#[derive(Clone, Copy, PartialEq, Eq)]
enum DesktopPhase {
    Spawned,
    Booting,
    Ready,
    EndpointUnavailable,
    Stopping,
    Stopped,
}

/// Native-only owner; neither image metadata nor a successful QMP reply is ready.
/// Keep this value outside cancellable futures and confirm stop before releasing
/// controller/execution ownership. This does not authenticate the caller.
pub(in crate::business_os) struct RetainedQemuDesktop {
    process: QemuProcess,
    guest_id: String,
    process_instance_id: String,
    endpoint_id: Option<String>,
    driver: Option<RemoteGuestDriver<UnixStream>>,
    phase: DesktopPhase,
}

impl RetainedQemuDesktop {
    pub(in crate::business_os) fn spawn_paused(
        config: &PreparedQemuGuest,
        guest_id: String,
    ) -> Result<Self> {
        ensure!(identifier(&guest_id), "guest identity is invalid");
        // Actual process ownership exists before the first asynchronous operation.
        let process = QemuProcess::spawn_paused(config)?;
        let process_instance_id = format!("qemu:{}:{}", process.pid(), uuid::Uuid::new_v4());
        Ok(Self {
            process,
            guest_id,
            process_instance_id,
            endpoint_id: None,
            driver: None,
            phase: DesktopPhase::Spawned,
        })
    }

    /// One bootstrap attempt. Failure/cancellation retains the process; it never
    /// recreates the channel, replays an input or promotes an image to readiness.
    pub(in crate::business_os) async fn boot(&mut self) -> Result<GuestLiveEndpoint> {
        ensure!(
            self.phase == DesktopPhase::Spawned,
            "guest bootstrap is retired"
        );
        self.phase = DesktopPhase::Booting;
        let result = self.boot_inner().await;
        if result.is_ok() {
            self.phase = DesktopPhase::Ready;
        } else {
            self.phase = DesktopPhase::EndpointUnavailable;
        }
        result
    }

    async fn boot_inner(&mut self) -> Result<GuestLiveEndpoint> {
        self.process.connect_monitor().await?;
        self.process.resume().await?;
        self.process.connect_guest_channel().await?;
        // QEMU's frontend socket can connect before the guest service exists.
        self.driver = Some(self.process.bind_guest_driver(self.guest_id.clone())?);
        self.endpoint_id = Some(uuid::Uuid::new_v4().to_string());
        self.probe_inner().await
    }

    /// A fresh real endpoint/capture observation under the native live guard.
    /// No bytes are returned to an unchecked publisher. A changed guest service
    /// session retires this owner even when the QEMU PID is still alive.
    pub(in crate::business_os) async fn probe_live(&mut self) -> Result<GuestLiveEndpoint> {
        ensure!(self.phase == DesktopPhase::Ready, "guest is not ready");
        // Set the state before awaiting. Cancelled probes cannot leave cached Ready.
        self.phase = DesktopPhase::EndpointUnavailable;
        let result = self.probe_inner().await;
        if result.is_ok() {
            self.phase = DesktopPhase::Ready;
        }
        result
    }

    async fn probe_inner(&mut self) -> Result<GuestLiveEndpoint> {
        let (frame, endpoint) = self.observe_inner().await?;
        drop(frame);
        Ok(endpoint)
    }

    /// Capture only while the caller holds the actual command, worker, policy
    /// and controller guards. The returned bytes still require a guarded native
    /// publisher; this method never turns a capture into delivery permission.
    pub(in crate::business_os) async fn observe_live(
        &mut self,
    ) -> Result<(GuestFrame, GuestLiveEndpoint)> {
        ensure!(self.phase == DesktopPhase::Ready, "guest is not ready");
        self.phase = DesktopPhase::EndpointUnavailable;
        let result = self.observe_inner().await;
        if result.is_ok() {
            self.phase = DesktopPhase::Ready;
        }
        result
    }

    async fn observe_inner(&mut self) -> Result<(GuestFrame, GuestLiveEndpoint)> {
        self.process.ensure_alive()?;
        let driver = self
            .driver
            .as_ref()
            .context("guest endpoint is unavailable")?;
        let session = driver.probe_endpoint().await?;
        // The same real display path supplies bootstrap and authorized capture.
        // Never publish bytes when the child or guest service changed in flight.
        let frame = driver.capture().await?;
        ensure!(
            driver.probe_endpoint().await? == session,
            "guest session changed during readiness"
        );
        self.process.ensure_alive()?;
        let endpoint = GuestLiveEndpoint {
            process_instance_id: self.process_instance_id.clone(),
            guest_session_id: session.session_id,
            endpoint_id: self
                .endpoint_id
                .clone()
                .context("guest endpoint is unregistered")?,
        };
        Ok((frame, endpoint))
    }

    /// The native consumer must additionally admit the exact current frame and
    /// serialize this call under its command/worker/policy/controller guard.
    /// A failed or cancelled operation retires cached readiness. It must never
    /// be retried merely because the retained QEMU PID is still alive.
    pub(in crate::business_os) async fn input_live(
        &mut self,
        expected: &GuestLiveEndpoint,
        input: &GuestInput,
    ) -> Result<()> {
        ensure!(self.phase == DesktopPhase::Ready, "guest is not ready");
        input.validate()?;
        ensure!(
            expected.process_instance_id == self.process_instance_id
                && self.endpoint_id.as_deref() == Some(expected.endpoint_id.as_str()),
            "guest input endpoint belongs to another native child"
        );
        self.phase = DesktopPhase::EndpointUnavailable;
        let result = async {
            self.process.ensure_alive()?;
            let driver = self
                .driver
                .as_ref()
                .context("guest endpoint is unavailable")?;
            let session = driver.probe_endpoint().await?;
            ensure!(
                session.session_id == expected.guest_session_id,
                "guest input observation belongs to another service session"
            );
            driver.input(input).await?;
            ensure!(
                driver.probe_endpoint().await? == session,
                "guest session changed during input; reconcile"
            );
            self.process.ensure_alive()?;
            Ok(())
        }
        .await;
        if result.is_ok() {
            self.phase = DesktopPhase::Ready;
        }
        result
    }

    pub(in crate::business_os) fn driver(&self) -> Result<&RemoteGuestDriver<UnixStream>> {
        ensure!(self.phase == DesktopPhase::Ready, "guest is not ready");
        self.driver
            .as_ref()
            .context("guest endpoint is unavailable")
    }

    pub(in crate::business_os) fn process_instance_id(&self) -> &str {
        &self.process_instance_id
    }

    pub(in crate::business_os) fn pid(&self) -> u32 {
        self.process.pid()
    }

    /// Reconciliation may call stop again on the SAME retained child. Only a
    /// confirmed exit returns; timeout/cancellation never releases its fence.
    /// A forced stop is not an application-consistent checkpoint.
    pub(in crate::business_os) async fn stop(&mut self) -> Result<ExitStatus> {
        self.phase = DesktopPhase::Stopping;
        let status = self.process.stop().await?;
        self.phase = DesktopPhase::Stopped;
        self.driver = None;
        self.endpoint_id = None;
        Ok(status)
    }
}
