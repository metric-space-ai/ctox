// Origin: CTOX
// License: AGPL-3.0-only

//! Retains the real QEMU child through bootstrap, endpoint failure and stop.
//! All calls belong inside the native controller/execution lifecycle, not intake.

use super::channel::RemoteGuestDriver;
use super::qemu::{PreparedQemuGuest, QemuMemoryState, QemuProcess};
use super::{identifier, GuestDriver, GuestFrame, GuestInput};
use super::{QuiescedQemuCheckpoint, StagedQemuCheckpoint};
use anyhow::{ensure, Context, Result};
use ctox_sync::guest_restore::GuestLiveEndpoint;
use std::process::ExitStatus;
use tokio::net::UnixStream;

#[derive(Clone, Copy, PartialEq, Eq)]
enum DesktopPhase {
    Spawned,
    Booting,
    Incoming,
    Restored,
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
    config: PreparedQemuGuest,
    guest_id: String,
    process_instance_id: String,
    endpoint_id: Option<String>,
    driver: Option<RemoteGuestDriver<UnixStream>>,
    phase: DesktopPhase,
    restored_session: Option<String>,
}

impl RetainedQemuDesktop {
    pub(in crate::business_os) fn spawn_paused(
        config: &PreparedQemuGuest,
        guest_id: String,
    ) -> Result<Self> {
        ensure!(identifier(&guest_id), "guest identity is invalid");
        // Actual process ownership exists before the first asynchronous operation.
        let process = QemuProcess::spawn_paused(config, &guest_id)?;
        let process_instance_id = format!("qemu:{}:{}", process.pid(), uuid::Uuid::new_v4());
        Ok(Self {
            process,
            config: config.clone(),
            guest_id,
            process_instance_id,
            endpoint_id: None,
            driver: None,
            phase: DesktopPhase::Spawned,
            restored_session: None,
        })
    }

    /// Native-only preparation, with the service session from the verified
    /// protected source checkpoint. No guest instruction may run here.
    pub(in crate::business_os) fn spawn_incoming(
        config: &PreparedQemuGuest,
        guest_id: String,
        original_service_session: String,
    ) -> Result<Self> {
        ensure!(
            identifier(&guest_id) && identifier(&original_service_session),
            "guest/session identity is invalid"
        );
        let process = QemuProcess::spawn_incoming(config, &guest_id)?;
        let process_instance_id = format!("qemu:{}:{}", process.pid(), uuid::Uuid::new_v4());
        Ok(Self {
            process,
            config: config.clone(),
            guest_id,
            process_instance_id,
            endpoint_id: None,
            driver: None,
            phase: DesktopPhase::Incoming,
            restored_session: Some(original_service_session),
        })
    }

    /// Load only under the retained native import/controller guard. Success is
    /// paused, not readiness or permission to execute. Cancellation retires it.
    pub(in crate::business_os) async fn load_memory(
        &mut self,
        input: &mut tokio::fs::File,
        expected: &QemuMemoryState,
    ) -> Result<()> {
        ensure!(
            self.phase == DesktopPhase::Incoming,
            "guest incoming attempt is retired"
        );
        self.phase = DesktopPhase::EndpointUnavailable;
        self.process.connect_monitor().await?;
        self.process.restore_memory(input, expected).await?;
        self.phase = DesktopPhase::Restored;
        Ok(())
    }

    /// The caller must freshly hold current execution authority across this
    /// effect. The original live guest-service session must survive restoration.
    pub(in crate::business_os) async fn activate_restored(&mut self) -> Result<GuestLiveEndpoint> {
        ensure!(
            self.phase == DesktopPhase::Restored,
            "guest is not fully restored"
        );
        self.phase = DesktopPhase::EndpointUnavailable;
        self.process.resume().await?;
        self.process.connect_guest_channel().await?;
        self.driver = Some(self.process.bind_guest_driver(self.guest_id.clone())?);
        self.endpoint_id = Some(uuid::Uuid::new_v4().to_string());
        let endpoint = self.probe_inner().await?;
        self.phase = DesktopPhase::Ready;
        Ok(endpoint)
    }

    /// Retire live desktop access, pause and save actual RAM/device state,
    /// then quit/reap this exact source cleanly before allowing disk copying.
    /// This is not authoritative reconciliation of external application effects.
    pub(in crate::business_os) async fn save_memory_live(
        &mut self,
        expected: &GuestLiveEndpoint,
        output: &mut tokio::fs::File,
    ) -> Result<QemuMemoryState> {
        self.validate_live_endpoint(expected).await?;
        self.phase = DesktopPhase::EndpointUnavailable;
        self.process.pause().await?;
        let memory = self.process.save_memory(output).await?;
        self.process.finish_memory_export().await?;
        self.phase = DesktopPhase::Stopped;
        self.driver = None;
        self.endpoint_id = None;
        Ok(memory)
    }

    /// A clean source-child witness plus its exact assigned disk/profile.
    /// The caller still owns current authority and external-effect reconciliation.
    pub(in crate::business_os) async fn save_checkpoint_live(
        &mut self,
        expected: &GuestLiveEndpoint,
        memory: &mut tokio::fs::File,
    ) -> Result<QuiescedQemuCheckpoint> {
        let state = self.save_memory_live(expected, memory).await?;
        QuiescedQemuCheckpoint::after_clean_exit(
            self.config.clone(),
            self.guest_id.clone(),
            expected.clone(),
            state,
        )
    }

    /// All RAM/disk/base/assignment checks precede spawning this paused child.
    /// Retain the returned owner before awaiting load_checkpoint.
    pub(in crate::business_os) fn spawn_checkpoint(
        checkpoint: &mut StagedQemuCheckpoint,
    ) -> Result<Self> {
        checkpoint.prepare_spawn()?;
        let owner = Self::spawn_incoming(
            &checkpoint.config,
            checkpoint.guest_id.clone(),
            checkpoint.service_session.clone(),
        )?;
        checkpoint.target_instance_id = Some(owner.process_instance_id.clone());
        Ok(owner)
    }

    /// No instructions run here. A separate activate_restored still requires
    /// the caller's fresh authority and the original real service session.
    pub(in crate::business_os) async fn load_checkpoint(
        &mut self,
        checkpoint: &mut StagedQemuCheckpoint,
    ) -> Result<()> {
        ensure!(
            checkpoint.target_instance_id.as_deref() == Some(self.process_instance_id.as_str())
                && self.config.memory_mib == checkpoint.config.memory_mib
                && self.config.vcpus == checkpoint.config.vcpus
                && self.guest_id == checkpoint.guest_id
                && self.restored_session.as_deref() == Some(checkpoint.service_session.as_str())
                && self.config.base_raw == checkpoint.config.base_raw
                && self.config.overlay_qcow2 == checkpoint.config.overlay_qcow2,
            "incoming machine owner differs from its completed staging"
        );
        let mut memory = tokio::fs::File::from_std(checkpoint.memory.try_clone()?);
        self.load_memory(&mut memory, &checkpoint.memory_state)
            .await
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
        ensure!(
            self.restored_session
                .as_ref()
                .is_none_or(|expected| *expected == session.session_id),
            "restored guest service is not the original session"
        );
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

    /// Recheck the retained child and pinned guest service without allocating a
    /// second capture. Failure or cancellation retires cached readiness.
    pub(in crate::business_os) async fn validate_live_endpoint(
        &mut self,
        expected: &GuestLiveEndpoint,
    ) -> Result<()> {
        ensure!(self.phase == DesktopPhase::Ready, "guest is not ready");
        ensure!(
            expected.process_instance_id == self.process_instance_id
                && self.endpoint_id.as_deref() == Some(expected.endpoint_id.as_str()),
            "native frame endpoint belongs to another child"
        );
        self.phase = DesktopPhase::EndpointUnavailable;
        let result = async {
            self.process.ensure_alive()?;
            let driver = self
                .driver
                .as_ref()
                .context("guest endpoint is unavailable")?;
            ensure!(
                driver.probe_endpoint().await?.session_id == expected.guest_session_id,
                "native frame guest service changed"
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

    /// A synchronous physical-poll check of this exact retained child/channel.
    pub(in crate::business_os) fn ensure_live_endpoint_current(
        &mut self,
        expected: &GuestLiveEndpoint,
    ) -> Result<()> {
        self.ensure_live_process()?;
        let result = (|| {
            ensure!(
                expected.process_instance_id == self.process_instance_id
                    && self.endpoint_id.as_deref() == Some(expected.endpoint_id.as_str()),
                "native frame endpoint belongs to another child"
            );
            self.driver
                .as_ref()
                .context("guest endpoint unavailable")?
                .ensure_current_endpoint(&expected.guest_session_id)
        })();
        if result.is_err() {
            self.phase = DesktopPhase::EndpointUnavailable;
        }
        result
    }

    /// Used inside the retained controller guard before polling a frame send.
    /// A dead owned child retires readiness even if the transport is pending.
    pub(in crate::business_os) fn ensure_live_process(&mut self) -> Result<()> {
        ensure!(self.phase == DesktopPhase::Ready, "guest is not ready");
        if let Err(error) = self.process.ensure_alive() {
            self.phase = DesktopPhase::EndpointUnavailable;
            return Err(error);
        }
        Ok(())
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
