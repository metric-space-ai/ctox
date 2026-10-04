//! One bounded native observation per guest. Bytes stay private until a guarded
//! WebRTC send, and a delivered observation is consumed before an input attempt.
use super::super::guest_runtime::{GuestAction, GuestFrame, GuestInput};
use super::*;
use rxdb::plugins::replication_webrtc::file_fetch_handler::GuardedFileSource;
use rxdb::rx_error::{new_rx_error, RxResult};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Weak,
};
use std::time::Instant;

pub(super) type Pool = rxdb::plugins::replication_webrtc::RxWebRTCReplicationPool<
    rxdb::plugins::replication_webrtc::WebRTCRsConnectionHandler,
>;
pub(super) const COLLECTION: &str = "guest_frames";
const CAPTURE_RESERVATION: usize = super::super::guest_runtime::GUEST_FRAME_LIMIT;
const TOTAL_BYTES: usize = 32 * 1024 * 1024;
const FRAME_LIFETIME: Duration = Duration::from_secs(30);

pub(super) struct FrameBudget {
    bytes: AtomicUsize,
    frames: AtomicUsize,
}
impl FrameBudget {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            bytes: AtomicUsize::new(0),
            frames: AtomicUsize::new(0),
        })
    }
    fn reserve(self: &Arc<Self>) -> Result<FramePermit> {
        self.frames
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                count.checked_add(1).filter(|next| *next <= 128)
            })
            .map_err(|_| anyhow::anyhow!("native frame count budget is full"))?;
        if self
            .bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(CAPTURE_RESERVATION)
                    .filter(|next| *next <= TOTAL_BYTES)
            })
            .is_err()
        {
            self.frames.fetch_sub(1, Ordering::AcqRel);
            anyhow::bail!("native frame memory budget is full");
        }
        Ok(FramePermit {
            budget: Arc::clone(self),
            bytes: CAPTURE_RESERVATION,
        })
    }
}
struct FramePermit {
    budget: Arc<FrameBudget>,
    bytes: usize,
}
impl FramePermit {
    fn retain_bytes(&mut self, bytes: usize) -> Result<()> {
        ensure!(
            bytes > 0 && bytes <= self.bytes,
            "native frame exceeds reservation"
        );
        self.budget
            .bytes
            .fetch_sub(self.bytes - bytes, Ordering::AcqRel);
        self.bytes = bytes;
        Ok(())
    }
}
impl Drop for FramePermit {
    fn drop(&mut self) {
        self.budget.bytes.fetch_sub(self.bytes, Ordering::AcqRel);
        self.budget.frames.fetch_sub(1, Ordering::AcqRel);
    }
}
pub(super) struct Observation {
    id: String,
    turn: String,
    endpoint: GuestLiveEndpoint,
    transport: Weak<Pool>,
    frame: GuestFrame,
    metadata: Value,
    deadline: Instant,
    delivered: bool,
    delivery_offset: usize,
    consumed: bool,
    _permit: FramePermit,
}
impl Observation {
    fn send_chunk(
        &mut self,
        offset: u64,
        max: usize,
        terminal: bool,
        send: &mut dyn FnMut(&Value, &[u8]) -> RxResult<()>,
    ) -> Result<()> {
        let offset = usize::try_from(offset)?;
        let end = offset
            .checked_add(max)
            .context("native frame range overflow")?;
        ensure!(
            !self.consumed
                && self.deadline > Instant::now()
                && !self.delivered
                && offset == self.delivery_offset
                && max <= 8 * 1024
                && end <= self.frame.png.len()
                && ((terminal && max == 0 && offset == self.frame.png.len())
                    || (!terminal && max > 0)),
            "native frame range invalid or delivery already attempted"
        );
        self.consumed = true;
        send(&self.metadata, &self.frame.png[offset..end]).map_err(anyhow::Error::from)?;
        self.delivery_offset = end;
        self.delivered = terminal;
        self.consumed = false;
        Ok(())
    }
    fn consume_input(
        &mut self,
        id: &str,
        turn: &str,
        input: &GuestInput,
    ) -> Result<GuestLiveEndpoint> {
        ensure!(
            self.id == id
                && self.turn == turn
                && self.delivered
                && !self.consumed
                && self.deadline > Instant::now(),
            "input requires the current completely delivered observation"
        );
        if let GuestInput::Click { x, y, .. } | GuestInput::Scroll { x, y, .. } = input {
            ensure!(
                *x < self.frame.width && *y < self.frame.height,
                "input exceeds observed display"
            );
        }
        // Consume before the first asynchronous guest effect; uncertain input
        // cannot become a retry permit.
        self.consumed = true;
        Ok(self.endpoint.clone())
    }
}
#[cfg(test)]
#[path = "guest_registry_frames_tests.rs"]
mod tests;

struct NativeFrameSource {
    owner: Weak<NativeGuestRegistry>,
    transport: Weak<Pool>,
}
impl NativeFrameSource {
    fn current_registry(&self) -> Result<Arc<NativeGuestRegistry>> {
        let registry = self.owner.upgrade().context("native frame owner closed")?;
        let transport = self
            .transport
            .upgrade()
            .context("native frame transport closed")?;
        ensure!(
            !transport.canceled.load(Ordering::SeqCst)
                && Arc::ptr_eq(&transport, &registry.current_transport()?),
            "native frame source belongs to a retired transport"
        );
        Ok(registry)
    }
}
fn transport_error(error: anyhow::Error) -> rxdb::rx_error::RxError {
    new_rx_error(
        "PERMISSION_DENIED",
        Some(json!({"message":error.to_string()})),
    )
}
impl NativeGuestRegistry {
    /// Receives the actual native peer's registry. No wire/session JSON can
    /// register a source, and generic peers without current document policy deny.
    pub(crate) fn attach_frame_transport(
        self: &Arc<Self>,
        session: &ctox_sync::native::NativeSyncSession,
    ) -> Result<()> {
        let transport = session.pool();
        ensure!(
            !transport.canceled.load(Ordering::SeqCst),
            "native frame pool is closed"
        );
        let mut attached = self
            .frame_transport
            .lock()
            .map_err(|_| anyhow::anyhow!("native frame transport poisoned"))?;
        if let Some(current) = attached
            .as_ref()
            .and_then(Weak::upgrade)
            .filter(|pool| !pool.canceled.load(Ordering::SeqCst))
        {
            ensure!(
                Arc::ptr_eq(&current, transport),
                "another live native frame transport is attached"
            );
            return Ok(());
        }
        transport
            .file_fetch_registry
            .register_guarded_source(
                COLLECTION,
                Arc::new(NativeFrameSource {
                    owner: Arc::downgrade(self),
                    transport: Arc::downgrade(transport),
                }),
            )
            .map_err(anyhow::Error::from)?;
        *attached = Some(Arc::downgrade(transport));
        Ok(())
    }
    fn current_transport(&self) -> Result<Arc<Pool>> {
        let pool = self
            .frame_transport
            .lock()
            .map_err(|_| anyhow::anyhow!("native frame transport poisoned"))?
            .as_ref()
            .and_then(Weak::upgrade)
            .context("native frame transport is not attached")?;
        ensure!(
            !pool.canceled.load(Ordering::SeqCst),
            "native frame pool is closed"
        );
        Ok(pool)
    }
    fn expire_frames(&self) -> Result<()> {
        let guests: Vec<_> = self
            .frame_guests
            .lock()
            .map_err(|_| anyhow::anyhow!("native frame index poisoned"))?
            .values()
            .cloned()
            .collect();
        for guest in guests {
            let registration = self.registration(&guest)?;
            // Never wait on another controller while holding the current one.
            if let Ok(mut entry) = registration.try_lock() {
                if entry
                    .frame
                    .as_ref()
                    .is_some_and(|frame| frame.deadline <= Instant::now() || frame.consumed)
                {
                    self.retire_frame(&mut entry)?;
                }
            };
        }
        Ok(())
    }
    fn frame_execution(self: &Arc<Self>, id: &str) -> Result<NativeGuestExecution> {
        ensure!(identifier(id), "native frame identity invalid");
        let guest = self
            .frame_guests
            .lock()
            .map_err(|_| anyhow::anyhow!("native frame index poisoned"))?
            .get(id)
            .cloned()
            .context("native frame is retired")?;
        let registration = self.registration(&guest)?;
        let entry = registration
            .lock()
            .map_err(|_| anyhow::anyhow!("native controller poisoned"))?;
        ensure!(
            entry.frame.as_ref().is_some_and(|frame| frame.id == id),
            "native frame replaced"
        );
        Ok(NativeGuestExecution {
            registry: Arc::clone(self),
            guest_id: guest,
            provider: entry
                .provider
                .clone()
                .context("native frame provider missing")?,
            binding: entry
                .execution
                .clone()
                .context("native frame execution missing")?,
        })
    }
    pub(super) fn retire_frame(&self, entry: &mut Registration) -> Result<()> {
        if let Some(frame) = entry.frame.take() {
            self.frame_guests
                .lock()
                .map_err(|_| anyhow::anyhow!("native frame index poisoned"))?
                .remove(&frame.id);
        }
        Ok(())
    }
}
impl NativeGuestExecution {
    fn current_job(&self, entry: &Registration) -> Result<()> {
        let effect = entry
            .registered_process
            .as_ref()
            .context("native child effect missing")?;
        ensure!(
            entry.process_effect.as_deref() == Some(effect.effect_id.as_str())
                && effect.job_id == self.binding.spec.job_id
                && effect.ownership == self.binding.ownership
                && effect.controller_id == entry.assignment.destination.controller_id
                && effect.controller_generation
                    == entry.assignment.destination.controller_generation,
            "native child effect changed"
        );
        #[cfg(target_os = "linux")]
        ensure!(
            entry
                .desktop
                .as_ref()
                .context("native child missing")?
                .process_instance_id()
                == effect.process_instance_id,
            "native child differs from its registered effect"
        );
        let job = super::super::guest_commands::block_on_guest(async {
            Ok(self
                .registry
                .authority
                .validate_ownership(&self.binding.spec.job_id, &self.binding.ownership)
                .await?)
        })?;
        ensure!(
            job.spec == self.binding.spec
                && job.ownership == self.binding.ownership
                && !job.stopped
                && job.pending_effects
                    == std::collections::BTreeSet::from([effect.effect_id.clone()]),
            "native frame execution is not the exact live process effect"
        );
        Ok(())
    }
    fn with_frame<T>(
        &self,
        id: &str,
        apply: impl FnOnce(&mut Observation, &mut dyn FnMut() -> Result<()>) -> Result<T>,
    ) -> Result<T> {
        let transport = self.registry.current_transport()?;
        self.provider
            .with_live_provider_transaction(|tx, facts, turn| {
                self.with_held_worker(tx, facts, |entry, verify| {
                    let frame = entry.frame.as_ref().context("native observation retired")?;
                    ensure!(
                        frame.id == id
                            && !frame.consumed
                            && !transport.canceled.load(Ordering::SeqCst)
                            && frame
                                .transport
                                .upgrade()
                                .is_some_and(|pool| Arc::ptr_eq(&pool, &transport))
                            && frame.deadline > Instant::now()
                            && turn == Some(frame.turn.as_str()),
                        "native observation expired/replaced or turn changed"
                    );
                    #[cfg(not(target_os = "linux"))]
                    {
                        let _ = apply;
                        anyhow::bail!("native frame delivery requires Linux QEMU");
                    }
                    #[cfg(target_os = "linux")]
                    {
                        let endpoint = frame.endpoint.clone();
                        let desktop = entry.desktop.as_mut().context("native child missing")?;
                        super::super::guest_commands::block_on_guest(
                            desktop.validate_live_endpoint(&endpoint),
                        )?;
                        verify()?;
                        // Ownership cannot be handed over while the exact registered
                        // process effect is pending. Revoke/stop use this controller.
                        let result = {
                            let frame =
                                entry.frame.as_mut().context("native observation retired")?;
                            let deadline = frame.deadline;
                            let mut current = || {
                                verify()?;
                                self.registry
                                    .verify_runtime_root(self.provider.runtime_root())?;
                                ensure!(
                                    !transport.canceled.load(Ordering::SeqCst)
                                        && deadline > Instant::now(),
                                    "native frame authority expired or pool closed"
                                );
                                desktop.ensure_live_process()
                            };
                            let result = apply(frame, &mut current);
                            if let Err(error) = current() {
                                frame.consumed = true;
                                return Err(error);
                            }
                            result
                        };
                        if transport.canceled.load(Ordering::SeqCst) {
                            if let Some(frame) = entry.frame.as_mut() {
                                frame.consumed = true;
                            }
                            anyhow::bail!("native frame pool closed during delivery");
                        }
                        if let Err(error) = verify() {
                            if let Some(frame) = entry.frame.as_mut() {
                                frame.consumed = true;
                            }
                            return Err(error);
                        }
                        result
                    }
                })
            })
    }
    pub(super) fn execute_frame_action(
        &self,
        entry: &mut Registration,
        verify: &dyn Fn() -> Result<()>,
        actual_turn: &str,
        action: GuestAction,
        command: &super::super::store::BusinessCommand,
    ) -> Result<ctox_protocol::mcp::CallToolResult> {
        let transport = self.registry.current_transport()?;
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (entry, verify, actual_turn, action, command, transport);
            anyhow::bail!("native guest effects require Linux QEMU");
        }
        #[cfg(target_os = "linux")]
        {
            self.current_job(entry)?;
            let result = match action {
                GuestAction::Observe => {
                    // Retire the old observation before allocation/await. Failure
                    // cannot leave old pixels as an input permit.
                    self.registry.retire_frame(entry)?;
                    self.registry.expire_frames()?;
                    let mut permit = self.registry.frame_budget.reserve()?;
                    let desktop = entry.desktop.as_mut().context("native child missing")?;
                    let (frame, endpoint) =
                        super::super::guest_commands::block_on_guest(desktop.observe_live())?;
                    verify()?;
                    self.current_job(entry)?;
                    permit.retain_bytes(frame.png.len())?;
                    let id = format!("guest-frame_{}", uuid::Uuid::new_v4());
                    let destination = &entry.assignment.destination;
                    let metadata = json!({"id":id, "frame_id":id, "owner_user_id":destination.human_owner_id,
                        "instance_id":destination.instance_id, "project_id":destination.project_id,
                        "thread_id":destination.thread_id, "worker_profile_id":destination.worker_profile_id,
                        "guest_id":destination.guest_id, "controller_id":destination.controller_id,
                        "controller_generation":destination.controller_generation, "mime_type":"image/png",
                        "width":frame.width, "height":frame.height, "size_bytes":frame.png.len(),
                        "frame_hash":format!("{:x}", Sha256::digest(&frame.png)), "collection_name":COLLECTION});
                    self.registry
                        .frame_guests
                        .lock()
                        .map_err(|_| anyhow::anyhow!("native frame index poisoned"))?
                        .insert(id.clone(), self.guest_id.clone());
                    entry.frame = Some(Observation {
                        id: id.clone(),
                        turn: actual_turn.into(),
                        endpoint,
                        transport: Arc::downgrade(&transport),
                        frame,
                        metadata: metadata.clone(),
                        deadline: Instant::now() + FRAME_LIFETIME,
                        delivered: false,
                        delivery_offset: 0,
                        consumed: false,
                        _permit: permit,
                    });
                    json!({"ok":true, "outcome":"observation_published", "command_id":command.id,
                        "guest_id":self.guest_id, "frame_id":id, "frame":metadata})
                }
                GuestAction::Input { frame_id, input } => {
                    let frame = entry.frame.as_mut().context("native observation missing")?;
                    ensure!(
                        frame
                            .transport
                            .upgrade()
                            .is_some_and(|pool| Arc::ptr_eq(&pool, &transport)),
                        "native input observation belongs to a retired transport"
                    );
                    let endpoint = frame.consume_input(&frame_id, actual_turn, &input)?;
                    let desktop = entry.desktop.as_mut().context("native child missing")?;
                    super::super::guest_commands::block_on_guest(
                        desktop.input_live(&endpoint, &input),
                    )?;
                    verify()?;
                    self.current_job(entry)?;
                    self.registry.retire_frame(entry)?;
                    json!({"ok":true, "outcome":"input_applied", "command_id":command.id, "guest_id":self.guest_id})
                }
            };
            if transport.canceled.load(Ordering::SeqCst) {
                self.registry.retire_frame(entry)?;
                anyhow::bail!("native frame pool closed during guest effect; reconcile");
            }
            serde_json::from_value(
                json!({"content":[{"type":"text", "text":result.to_string()}],
                "structuredContent":result, "isError":false}),
            )
            .context("native guest result encoding failed")
        }
    }
}
impl GuardedFileSource for NativeFrameSource {
    fn byte_len(&self, id: &str) -> RxResult<u64> {
        let registry = self.current_registry().map_err(transport_error)?;
        let execution = registry.frame_execution(id).map_err(transport_error)?;
        // A fresh quorum read before a complete transfer; every actual send
        // additionally holds worker/policy/controller and live-child guards.
        execution
            .with_current(|entry, _| execution.current_job(entry))
            .map_err(transport_error)?;
        execution
            .with_frame(id, |frame, current| {
                current()?;
                Ok(frame.frame.png.len() as u64)
            })
            .map_err(transport_error)
    }
    fn with_current_chunk(
        &self,
        id: &str,
        offset: u64,
        max: usize,
        terminal: bool,
        capability_token: &str,
        send: &mut dyn FnMut(&Value, &[u8], &mut dyn FnMut() -> RxResult<()>) -> RxResult<()>,
    ) -> RxResult<()> {
        let registry = self.current_registry().map_err(transport_error)?;
        let execution = registry.frame_execution(id).map_err(transport_error)?;
        // Initialize the verifier's native read cache before taking the policy
        // transaction. Revalidate under that same transaction at the actual send.
        super::super::store::verify_webrtc_capability_actor(
            &registry.runtime_root,
            capability_token,
        )
        .context("native frame peer is not authenticated by this instance")
        .map_err(transport_error)?;
        execution
            .with_frame(id, |frame, native_current| {
                let owner = frame.metadata["owner_user_id"]
                    .as_str()
                    .context("native frame owner missing")?
                    .to_owned();
                let mut current = || {
                    native_current()?;
                    let (actor, _) = super::super::store::verify_webrtc_capability_actor(
                        &registry.runtime_root,
                        capability_token,
                    )
                    .context("native frame peer capability revoked")?;
                    ensure!(
                        actor == owner,
                        "native frame belongs to another authenticated owner"
                    );
                    Ok(())
                };
                current()?;
                frame.send_chunk(offset, max, terminal, &mut |metadata, bytes| {
                    let mut check = || current().map_err(transport_error);
                    send(metadata, bytes, &mut check)
                })
            })
            .map_err(transport_error)
    }
}
