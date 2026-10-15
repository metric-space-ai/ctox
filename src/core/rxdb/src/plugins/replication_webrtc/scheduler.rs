//! One V4 sender per association, with strict 0–2 priority and byte DRR 8:3:1.
//! No producer writes a V4 DataChannel directly. Session admission means bounded
//! retention until delivery ACK; handoff counters report SCTP separately.
use super::super::connection_handler_rs::send_result::{transport_diagnostics, MessageClass};
use super::super::connection_handler_rs::send_under_publication;
use super::super::webrtc_types::WebRTCPublicationGuard;
use super::framing::{Framer, Reassembler, MAX_MESSAGE};
use super::hello::{TransportSelection, WireSeq};
use super::lanes::{Lane, ASSOCIATION_BUDGET, BULK_WEIGHTS, FRAME_BYTES, REALTIME_LABEL};
use crate::rx_error::{new_rx_error, RxError, RxResult};
use crate::rxjs_compat::{RxStream, RxSubject};
use futures::{stream::FuturesUnordered, FutureExt, StreamExt};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, Semaphore};
use webrtc::data_channel::{DataChannel, DataChannelEvent, RTCDataChannelInit};
use webrtc::peer_connection::PeerConnection;
#[path = "v4_scheduler/lane_artifacts.rs"]
pub mod lane_artifacts;
#[path = "session_resume.rs"]
pub mod session_resume;
use session_resume::{ReliableSession, SessionControl, SessionHello};

pub struct SessionPayload {
    pub application_id: WireSeq,
    pub bytes: Arc<[u8]>,
}
#[derive(Clone)]
pub struct SessionBinding {
    pub state: Arc<ReliableSession<SessionPayload>>,
    pub generation: u64,
}
impl SessionBinding {
    /// Called ONLY after same-device/instance/epoch/rights authentication.
    pub fn attach(
        state: Arc<ReliableSession<SessionPayload>>,
        remote: &SessionHello,
    ) -> RxResult<Self> {
        let generation = state.attach(remote).map_err(|e| failure(&e.to_string()))?;
        Ok(Self { state, generation })
    }
}
/// Consumer acknowledgement follows application effect, never queue admission.
#[derive(Clone)]
pub struct DeliveryToken {
    signal: Arc<Mutex<Option<oneshot::Sender<SessionControl>>>>,
    binding: SessionBinding,
    lane: Lane,
    seq: WireSeq,
}
impl std::fmt::Debug for DeliveryToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeliveryToken")
            .field("lane", &self.lane)
            .field("seq", &self.seq)
            .finish_non_exhaustive()
    }
}
impl DeliveryToken {
    pub fn acknowledge(&self) -> RxResult<()> {
        let signal = self
            .signal
            .lock()
            .take()
            .ok_or_else(|| failure("V4 delivery already acknowledged"))?;
        let ack = self
            .binding
            .state
            .delivered(self.binding.generation, self.lane, self.seq)
            .map_err(|e| failure(&e.to_string()))?;
        signal
            .send(ack)
            .map_err(|_| failure("V4 delivery generation retired"))
    }
}
struct SessionPublicationGuard {
    binding: SessionBinding,
    native: Option<Arc<dyn WebRTCPublicationGuard>>,
}
impl WebRTCPublicationGuard for SessionPublicationGuard {
    fn with_current(&self, publish: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
        let mut guarded = || {
            self.binding
                .state
                .with_generation(self.binding.generation, &mut *publish)
                .map_err(|e| failure(&e.to_string()))?
        };
        if let Some(native) = &self.native {
            native.with_current(&mut guarded)
        } else {
            guarded()
        }
    }
    fn after_send(&self) -> RxResult<()> {
        if let Some(native) = &self.native {
            native.after_send()?;
        }
        self.binding
            .state
            .check_generation(self.binding.generation)
            .map_err(|e| failure(&e.to_string()))
    }
}

const QUEUE_BUDGET: usize = 192 * 1024 * 1024;
const ADMISSION: [usize; 7] = [
    128 * 1024,
    1024 * 1024,
    128 * 1024,
    4 * 1024 * 1024,
    8 * 1024 * 1024,
    16 * 1024 * 1024,
    64 * 1024,
];
const SEND_TIMEOUT: Duration = Duration::from_secs(30);

struct Entry<T> {
    bytes: usize,
    value: T,
}
/// Pure scheduling core, also used by transport benchmarks without a network.
pub struct SendQueue<T> {
    queues: [VecDeque<Entry<T>>; 6],
    deficit: [usize; 3],
    cursor: usize,
    credited: [bool; 3],
    bytes: usize,
}
impl<T> Default for SendQueue<T> {
    fn default() -> Self {
        Self {
            queues: std::array::from_fn(|_| VecDeque::new()),
            deficit: [0; 3],
            cursor: 0,
            credited: [false; 3],
            bytes: 0,
        }
    }
}
impl<T> SendQueue<T> {
    pub fn push(&mut self, lane: Lane, bytes: usize, value: T) -> Result<(), T> {
        if bytes > FRAME_BYTES || self.bytes.saturating_add(bytes) > QUEUE_BUDGET {
            return Err(value);
        }
        self.bytes += bytes;
        self.queues[lane.index()].push_back(Entry { bytes, value });
        Ok(())
    }
    pub fn queued_bytes(&self) -> usize {
        self.bytes
    }
    pub fn pop(&mut self) -> Option<(Lane, T)> {
        self.pop_ready(|_| true)
    }
    pub fn pop_ready(&mut self, ready: impl Fn(Lane) -> bool) -> Option<(Lane, T)> {
        for lane in Lane::ALL.into_iter().take(3) {
            if ready(lane) {
                if let Some(item) = self.queues[lane.index()].pop_front() {
                    self.bytes -= item.bytes;
                    return Some((lane, item.value));
                }
            }
        }
        for _ in 0..6 {
            let i = self.cursor;
            let lane = Lane::ALL[i + 3];
            if !ready(lane) || self.queues[i + 3].is_empty() {
                if self.queues[i + 3].is_empty() {
                    self.deficit[i] = 0;
                    self.credited[i] = false;
                }
                self.cursor = (i + 1) % 3;
                continue;
            }
            if !self.credited[i] {
                self.deficit[i] += BULK_WEIGHTS[i] * FRAME_BYTES;
                self.credited[i] = true;
            }
            let bytes = self.queues[i + 3].front().unwrap().bytes;
            if bytes <= self.deficit[i] {
                let item = self.queues[i + 3].pop_front().unwrap();
                self.deficit[i] -= bytes;
                self.bytes -= bytes;
                return Some((lane, item.value));
            }
            self.cursor = (i + 1) % 3;
            self.credited[i] = false;
        }
        None
    }
}

/// The adapter uses an empty-buffer event as the credit fence. Because there
/// is exactly one outstanding frame per channel, even delayed events cannot
/// credit bytes sent after the event. Seven channels cap aggregate data at
/// 112 KiB; the remaining 16 KiB is reserved for association/control overhead.
#[derive(Default)]
pub struct AssociationBudget {
    outstanding: [usize; 7],
}
impl AssociationBudget {
    pub fn reserve(&mut self, channel: usize, bytes: usize) -> bool {
        if channel >= 7
            || bytes > FRAME_BYTES
            || self.outstanding[channel] != 0
            || self.buffered().saturating_add(bytes) > ASSOCIATION_BUDGET
        {
            return false;
        }
        self.outstanding[channel] = bytes.max(1);
        true
    }
    pub fn ready(&self, channel: usize) -> bool {
        channel < 7 && self.outstanding[channel] == 0
    }
    pub fn drained(&mut self, channel: usize) {
        if let Some(value) = self.outstanding.get_mut(channel) {
            *value = 0;
        }
    }
    pub fn buffered(&self) -> usize {
        self.outstanding.iter().sum()
    }
}

#[derive(Clone, Debug)]
pub enum Incoming {
    Reliable {
        lane: Lane,
        id: WireSeq,
        bytes: Vec<u8>,
        delivery: Option<DeliveryToken>,
    },
    Realtime(Vec<u8>),
    Failed {
        reason: String,
    },
    SendRejected {
        lane: Option<Lane>,
        reason: String,
    },
}
struct Completion {
    remaining: usize,
    failed: bool,
    result: Option<oneshot::Sender<RxResult<()>>>,
    liveness: tokio::sync::watch::Receiver<()>,
    _permit: Option<tokio::sync::OwnedSemaphorePermit>,
    _internal_owner: Option<tokio::sync::watch::Sender<()>>,
}
impl Completion {
    fn cancelled(&self) -> bool {
        self.failed
            || (self._internal_owner.is_none()
                && self.result.as_ref().is_none_or(|r| r.is_closed()))
    }
    fn finish(&mut self, result: RxResult<()>) {
        if result.is_err() {
            self.failed = true;
            if let Some(sender) = self.result.take() {
                let _ = sender.send(result);
            }
        } else {
            self.remaining = self.remaining.saturating_sub(1);
            if self.remaining == 0 {
                if let Some(sender) = self.result.take() {
                    let _ = sender.send(Ok(()));
                }
            }
        }
    }
}
struct Outbound {
    lane: Option<Lane>,
    frames: Vec<Vec<u8>>,
    completion: Arc<Mutex<Completion>>,
    publication: Option<Arc<dyn WebRTCPublicationGuard>>,
}
struct Physical {
    bytes: Vec<u8>,
    ack_lane: Option<Lane>,
    completion: Arc<Mutex<Completion>>,
    publication: Option<Arc<dyn WebRTCPublicationGuard>>,
}
struct Task(tokio::task::AbortHandle);
impl Drop for Task {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Default)]
struct Counters {
    attempted: [std::sync::atomic::AtomicU64; 2],
    handed_to_sctp: [std::sync::atomic::AtomicU64; 2],
    failed: [std::sync::atomic::AtomicU64; 2],
    buffered_upper_bound_peak: std::sync::atomic::AtomicU64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HandoffCounters {
    pub attempted: [u64; 2],
    pub handed_to_sctp: [u64; 2],
    pub failed: [u64; 2],
    pub buffered_upper_bound_peak: u64,
}
impl Counters {
    fn snapshot(&self) -> HandoffCounters {
        use std::sync::atomic::Ordering::Relaxed;
        HandoffCounters {
            attempted: std::array::from_fn(|i| self.attempted[i].load(Relaxed)),
            handed_to_sctp: std::array::from_fn(|i| self.handed_to_sctp[i].load(Relaxed)),
            failed: std::array::from_fn(|i| self.failed[i].load(Relaxed)),
            buffered_upper_bound_peak: self.buffered_upper_bound_peak.load(Relaxed),
        }
    }
}
#[derive(Clone)]
pub struct NativeSendScheduler {
    tx: mpsc::Sender<Outbound>,
    counters: Arc<Counters>,
    drain: tokio::sync::watch::Sender<bool>,
    status: tokio::sync::watch::Receiver<Option<bool>>,
    framer: Framer,
    queue_budget: [Arc<Semaphore>; 7],
    receivers: Arc<Mutex<[Option<mpsc::Receiver<Incoming>>; 7]>>,
    failures: RxSubject<Incoming>,
    session: Option<SessionBinding>,
    reliable_order: [Arc<tokio::sync::Mutex<()>>; 6],
    _task: Arc<Task>,
}
fn failure(message: &str) -> RxError {
    new_rx_error(
        "RC_WEBRTC_PEER",
        Some(serde_json::json!({"message": message, "transport": "v4"})),
    )
}
impl NativeSendScheduler {
    /// The host must authenticate the same session and retire the old sender
    /// before calling this. Every replay is freshly authorized; stale generation
    /// guards are deliberately not retained or silently made unrestricted.
    pub async fn create_resumed(
        pc: &Arc<dyn PeerConnection>,
        transport: TransportSelection,
        session: SessionBinding,
        authorize: impl Fn(Lane, WireSeq, &[u8]) -> RxResult<Arc<dyn WebRTCPublicationGuard>>
            + Send
            + Sync,
    ) -> RxResult<Self> {
        tokio::time::timeout(SEND_TIMEOUT, async {
            let scheduler = Self::create_inner(pc, transport, Some(session)).await?;
            if let Err(error) = scheduler.replay_pending(&authorize).await {
                scheduler.close_now();
                return Err(error);
            }
            Ok(scheduler)
        })
        .await
        .map_err(|_| failure("V4 resumed setup/replay timed out"))?
    }
    async fn replay_pending(
        &self,
        authorize: &(impl Fn(Lane, WireSeq, &[u8]) -> RxResult<Arc<dyn WebRTCPublicationGuard>>
              + Send
              + Sync),
    ) -> RxResult<()> {
        let binding = self
            .session
            .as_ref()
            .ok_or_else(|| failure("V4 replay requires a session"))?;
        let mut replays = FuturesUnordered::new();
        for lane in Lane::ALL {
            let messages = binding
                .state
                .replay(binding.generation, lane)
                .map_err(|e| failure(&e.to_string()))?;
            replays.push(async move {
                let _order = self.reliable_order[lane.index()].lock().await;
                for message in messages {
                    let publication =
                        authorize(lane, message.value.application_id, &message.value.bytes)?;
                    self.submit(
                        Some(lane),
                        message.seq,
                        &message.value.bytes,
                        Some(publication),
                    )
                    .await?;
                }
                Ok::<_, RxError>(())
            });
        }
        while let Some(result) = replays.next().await {
            result?;
        }
        Ok(())
    }
    /// Called only after authenticated hello negotiation. Native creates every
    /// channel, including real-time; browser does not choose reliability.
    pub async fn create(
        pc: &Arc<dyn PeerConnection>,
        transport: TransportSelection,
    ) -> RxResult<Self> {
        tokio::time::timeout(SEND_TIMEOUT, Self::create_inner(pc, transport, None))
            .await
            .map_err(|_| failure("V4 channel setup timed out"))?
    }
    async fn create_inner(
        pc: &Arc<dyn PeerConnection>,
        transport: TransportSelection,
        session: Option<SessionBinding>,
    ) -> RxResult<Self> {
        let framer = Framer::new(transport).map_err(|e| failure(&e.to_string()))?;
        let mut channels: Vec<Arc<dyn DataChannel>> = Vec::new();
        let mut rollback = CloseChannels(Vec::new());
        for label in Lane::ALL.iter().map(|l| l.label()).chain([REALTIME_LABEL]) {
            let realtime = label == REALTIME_LABEL;
            let init = RTCDataChannelInit {
                ordered: !realtime,
                max_retransmits: realtime.then_some(0),
                ..Default::default()
            };
            let channel = match pc.create_data_channel(label, Some(init)).await {
                Ok(channel) => channel,
                Err(error) => {
                    return Err(failure(&format!("create V4 channel: {error}")));
                }
            };
            rollback.0.push(channel.clone());
            // Validate the locally created channel, where webrtc-rs retains
            // the reliability parameters. Its incoming-channel metadata adapter
            // currently discards these optional fields (rtc accept path).
            if channel
                .ordered()
                .await
                .map_err(|e| failure(&e.to_string()))?
                != !realtime
                || channel
                    .max_retransmits()
                    .await
                    .map_err(|e| failure(&e.to_string()))?
                    != realtime.then_some(0)
                || channel
                    .max_packet_life_time()
                    .await
                    .map_err(|e| failure(&e.to_string()))?
                    .is_some()
            {
                return Err(failure("V4 channel reliability does not match its lane"));
            }

            channels.push(channel);
        }
        let scheduler = Self::from_channels_session(channels, framer, session).await?;
        rollback.0.clear();
        Ok(scheduler)
    }
    async fn from_channels(channels: Vec<Arc<dyn DataChannel>>, framer: Framer) -> RxResult<Self> {
        Self::from_channels_session(channels, framer, None).await
    }
    async fn from_channels_session(
        channels: Vec<Arc<dyn DataChannel>>,
        framer: Framer,
        session: Option<SessionBinding>,
    ) -> RxResult<Self> {
        if channels.len() != 7 {
            return Err(failure(
                "V4 requires six reliable channels and one real-time channel",
            ));
        }
        for channel in &channels {
            // Zero is essential: a low event must prove that the previous
            // frame is completely retired, not just below a per-channel cap.
            channel
                .set_buffered_amount_low_threshold(0)
                .await
                .map_err(|e| failure(&e.to_string()))?;
            channel
                .set_buffered_amount_high_threshold(FRAME_BYTES as u32)
                .await
                .map_err(|e| failure(&e.to_string()))?;
        }
        let (tx, rx) = mpsc::channel(32);
        let counters = Arc::new(Counters::default());
        let mut receivers = Vec::new();
        let senders: [mpsc::Sender<Incoming>; 7] = std::array::from_fn(|_| {
            let (tx, rx) = mpsc::channel(1);
            receivers.push(Some(rx));
            tx
        });
        let failures = RxSubject::with_lag_signal(16, |_| {
            Some(Incoming::Failed {
                reason: "V4 diagnostic observer lagged".into(),
            })
        });
        let (drain, drain_rx) = tokio::sync::watch::channel(false);
        let (finished, status) = tokio::sync::watch::channel(None);
        let actor_framer = framer.clone();
        let actor_failures = failures.clone();
        let actor_counters = counters.clone();
        let actor_session = session.clone();
        let task = tokio::spawn(async move {
            let result = run(
                channels,
                actor_framer,
                rx,
                senders,
                actor_failures,
                actor_counters,
                drain_rx,
                actor_session,
            )
            .await;
            let _ = finished.send(Some(result.is_ok()));
        });
        Ok(Self {
            tx,
            counters,
            drain,
            status,
            framer,
            queue_budget: std::array::from_fn(|i| Arc::new(Semaphore::new(ADMISSION[i]))),
            receivers: Arc::new(Mutex::new(receivers.try_into().ok().unwrap())),
            failures,
            session,
            reliable_order: std::array::from_fn(|_| Arc::new(tokio::sync::Mutex::new(()))),
            _task: Arc::new(Task(task.abort_handle())),
        })
    }
    /// Indices: 0 reliable frames, 1 unreliable real-time packets.
    pub fn handoff_counters(&self) -> HandoffCounters {
        self.counters.snapshot()
    }
    pub fn take_incoming(&self, lane: Option<Lane>) -> Option<mpsc::Receiver<Incoming>> {
        self.receivers.lock()[lane.map_or(6, Lane::index)].take()
    }
    pub fn failures(&self) -> RxStream<Incoming> {
        self.failures.subscribe()
    }
    pub async fn send(
        &self,
        lane: Lane,
        id: WireSeq,
        bytes: &[u8],
        publication: Option<Arc<dyn WebRTCPublicationGuard>>,
    ) -> RxResult<()> {
        // Never allocate a session sequence for data that cannot be reframed.
        if bytes.len() > MAX_MESSAGE {
            return Err(failure("V4 message exceeds negotiated size limit"));
        }
        if *self.drain.borrow() {
            return Err(failure("V4 scheduler is draining"));
        }
        let _order = self.reliable_order[lane.index()].lock().await;
        if *self.drain.borrow() {
            return Err(failure("V4 scheduler is draining"));
        }
        let sequence = if let Some(binding) = &self.session {
            let retained = binding
                .state
                .retain(
                    binding.generation,
                    lane,
                    bytes.len(),
                    SessionPayload {
                        application_id: id,
                        bytes: Arc::from(bytes),
                    },
                )
                .await
                .map_err(|e| failure(&e.to_string()))?;
            retained.seq
        } else {
            id
        };
        self.submit(Some(lane), sequence, bytes, publication).await
    }
    /// Stop new admission and wait for all accepted frames to leave SCTP.
    /// Emergency Drop cancels; successful drain confirms SCTP retirement,
    /// not an application commit or reliable delivery of real-time packets.
    pub async fn drain(&self) -> RxResult<()> {
        self.drain.send_replace(true);
        let mut status = self.status.clone();
        let waiting = async {
            loop {
                if let Some(ok) = *status.borrow() {
                    return if ok {
                        Ok(())
                    } else {
                        Err(failure("V4 association failed while draining"))
                    };
                }
                status
                    .changed()
                    .await
                    .map_err(|_| failure("V4 association stopped while draining"))?;
            }
        };
        match tokio::time::timeout(SEND_TIMEOUT, waiting).await {
            Ok(result) => result,
            Err(_) => {
                self._task.0.abort();
                Err(failure("V4 drain deadline exceeded; association closed"))
            }
        }
    }
    /// Revoke all retained clones of this exact association on peer teardown.
    /// This is cancellation, not a successful graceful drain.
    pub fn close_now(&self) {
        self.drain.send_replace(true);
        self._task.0.abort();
    }
    pub async fn send_realtime(&self, bytes: &[u8]) -> RxResult<()> {
        self.submit(None, WireSeq(0), bytes, None).await
    }
    async fn submit(
        &self,
        lane: Option<Lane>,
        id: WireSeq,
        bytes: &[u8],
        publication: Option<Arc<dyn WebRTCPublicationGuard>>,
    ) -> RxResult<()> {
        if *self.drain.borrow() {
            return Err(failure("V4 scheduler is draining"));
        }
        if bytes.len() > MAX_MESSAGE
            || (lane.is_none() && (bytes.is_empty() || bytes.len() > self.framer.frame_bytes()))
        {
            return Err(failure("V4 message exceeds negotiated size limit"));
        }
        // Bound allocations before encoding; framing overhead fits in 4 KiB
        // per MiB, reserve conservatively. Largest message needs a full turn.
        // Admission is separate per lane: a 16-MiB bulk producer cannot
        // consume the control/command permits and recreate head-of-line delay.
        let index = lane.map_or(6, Lane::index);
        let charge = bytes
            .len()
            .saturating_add(bytes.len() / 16 + FRAME_BYTES)
            .min(ADMISSION[index])
            .max(1);
        let permit = self.queue_budget[index]
            .clone()
            .acquire_many_owned(charge as u32)
            .await
            .map_err(|_| failure("V4 scheduler closed"))?;
        let frames = match lane {
            Some(lane) => self
                .framer
                .encode(lane, id, bytes)
                .map_err(|e| failure(&e.to_string()))?,
            None => vec![bytes.to_vec()],
        };
        let publication = match &self.session {
            Some(binding) => Some(Arc::new(SessionPublicationGuard {
                binding: binding.clone(),
                native: publication,
            }) as Arc<dyn WebRTCPublicationGuard>),
            None => publication,
        };
        let (sender, receipt) = oneshot::channel();
        // Sender lifetime belongs to exactly this submit future. Dropping it
        // cancels its own pending physical IO, never another producer's IO.
        let (owner, liveness) = tokio::sync::watch::channel(());
        let retained = self.session.is_some() && lane.is_some();
        let completion = Arc::new(Mutex::new(Completion {
            remaining: frames.len(),
            failed: false,
            result: Some(sender),
            liveness,
            _permit: Some(permit),
            _internal_owner: retained.then_some(owner.clone()),
        }));
        if *self.drain.borrow() {
            return Err(failure("V4 scheduler is draining"));
        }
        self.tx
            .send(Outbound {
                lane,
                frames,
                completion,
                publication,
            })
            .await
            .map_err(|_| failure("V4 scheduler closed"))?;
        if retained {
            return Ok(());
        }
        receipt
            .await
            .map_err(|_| failure("V4 association closed before SCTP handoff"))?
    }
}

enum Wake {
    Channel(usize, Option<DataChannelEvent>),
    Delivered(usize, bool),
    Committed(usize, usize, Option<SessionControl>),
}
fn next_event(
    index: usize,
    channel: Arc<dyn DataChannel>,
) -> futures::future::BoxFuture<'static, Wake> {
    async move { Wake::Channel(index, channel.poll().await) }.boxed()
}
fn delivery(
    index: usize,
    tx: mpsc::Sender<Incoming>,
    message: Incoming,
) -> futures::future::BoxFuture<'static, Wake> {
    async move { Wake::Delivered(index, tx.send(message).await.is_ok()) }.boxed()
}
struct CloseChannels(Vec<Arc<dyn DataChannel>>);
impl Drop for CloseChannels {
    fn drop(&mut self) {
        let channels = std::mem::take(&mut self.0);
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = tokio::time::timeout(Duration::from_secs(5), async move {
                    for channel in channels {
                        let _ = channel.close().await;
                    }
                })
                .await;
            });
        }
    }
}
fn enqueue(queue: &mut SendQueue<Physical>, realtime: &mut VecDeque<Physical>, message: Outbound) {
    for bytes in message.frames {
        let item = Physical {
            bytes,
            ack_lane: None,
            completion: message.completion.clone(),
            publication: message.publication.clone(),
        };
        if let Some(lane) = message.lane {
            let size = item.bytes.len();
            if let Err(item) = queue.push(lane, size, item) {
                item.completion
                    .lock()
                    .finish(Err(failure("V4 send queue full")));
            }
        } else if realtime.len() < 32 {
            realtime.push_back(item);
        } else {
            item.completion
                .lock()
                .finish(Err(failure("V4 real-time queue full")));
        }
    }
}
fn enqueue_ack(
    queue: &mut SendQueue<Physical>,
    framer: &Framer,
    ack: SessionControl,
    binding: &SessionBinding,
) -> RxResult<()> {
    let SessionControl::Ack { lane, .. } = &ack;
    let mut removed = 0;
    queue.queues[0].retain(|entry| {
        if entry.value.ack_lane == Some(*lane) {
            removed += entry.bytes;
            false
        } else {
            true
        }
    });
    queue.bytes -= removed;
    let frames = framer
        .encode(Lane::Control, WireSeq(0), &ack.encode())
        .map_err(|e| failure(&e.to_string()))?;
    let (owner, liveness) = tokio::sync::watch::channel(());
    let completion = Arc::new(Mutex::new(Completion {
        remaining: frames.len(),
        failed: false,
        result: None,
        liveness,
        _permit: None,
        _internal_owner: Some(owner),
    }));
    for bytes in frames.into_iter().rev() {
        if queue.bytes + bytes.len() > QUEUE_BUDGET {
            return Err(failure("V4 ACK queue full"));
        }
        queue.bytes += bytes.len();
        queue.queues[0].push_front(Entry {
            bytes: bytes.len(),
            value: Physical {
                bytes,
                ack_lane: Some(*lane),
                completion: completion.clone(),
                publication: Some(Arc::new(SessionPublicationGuard {
                    binding: binding.clone(),
                    native: None,
                })),
            },
        });
    }
    Ok(())
}
async fn run(
    channels: Vec<Arc<dyn DataChannel>>,
    framer: Framer,
    mut rx: mpsc::Receiver<Outbound>,
    incoming: [mpsc::Sender<Incoming>; 7],
    failures: RxSubject<Incoming>,
    counters: Arc<Counters>,
    mut drain: tokio::sync::watch::Receiver<bool>,
    session: Option<SessionBinding>,
) -> RxResult<()> {
    let _close_channels = CloseChannels(channels.clone());
    let mut events = FuturesUnordered::new();
    for (i, channel) in channels.iter().enumerate() {
        events.push(next_event(i, channel.clone()));
    }
    let mut queue = SendQueue::default();
    let mut realtime = VecDeque::new();
    let mut budget = AssociationBudget::default();
    let mut open = [false; 7];
    // Retire the initial native DCEP OPEN before using its first data credit.
    // Otherwise a delayed handshake-low event could credit a later data frame.
    let mut initial_drain = [false; 7];
    let startup_deadline = tokio::time::Instant::now() + SEND_TIMEOUT;
    let frame_limit = framer.frame_bytes();
    let mut drain_deadlines: [Option<tokio::time::Instant>; 7] = [None; 7];
    let mut reassembly = Reassembler::new(framer.clone());
    let mut closing = false;
    let mut deliveries: [VecDeque<(WireSeq, Vec<u8>)>; 6] =
        std::array::from_fn(|_| VecDeque::new());
    let mut delivery_active = [false; 6];
    let mut delivery_bytes = [0usize; 6];
    let mut received = session
        .as_ref()
        .map_or([0; 6], |s| s.state.hello().resume_from.map(|s| s.0));
    let result: RxResult<()> = async {
        loop {
            if let Some(binding) = &session {
                binding.state.check_generation(binding.generation).map_err(|e| failure(&e.to_string()))?;
            }
            if *drain.borrow() { closing = true; rx.close(); }
            if closing && rx.is_empty() && queue.queued_bytes() == 0 && realtime.is_empty() && budget.buffered() == 0 && delivery_bytes.iter().all(|n| *n == 0) { return Ok(()); }
            if drain_deadlines.iter().flatten().any(|t| *t <= tokio::time::Instant::now()) {
                return Err(failure("V4 SCTP buffer did not drain before its deadline"));
            }
            if initial_drain.iter().any(|ready| !*ready) && tokio::time::Instant::now() >= startup_deadline {
                return Err(failure("V4 DCEP buffer did not drain before startup deadline"));
            }
            if let Some(binding) = &session {
                for lane in Lane::ALL {
                    let index = lane.index();
                    if !delivery_active[index] {
                        if let Some((id, bytes)) = deliveries[index].pop_front() {
                            if !binding.state.should_deliver(binding.generation, lane, id).map_err(|e| failure(&e.to_string()))? {
                                return Err(failure("V4 pending application delivery duplicated"));
                            }
                            delivery_active[index] = true;
                            let size = bytes.len().max(1).saturating_add(256);
                            let (tx, receipt) = oneshot::channel();
                            let token = DeliveryToken { signal: Arc::new(Mutex::new(Some(tx))), binding: binding.clone(), lane, seq: id };
                            let incoming = incoming[index].clone();
                            events.push(async move {
                                let message = Incoming::Reliable { lane, id, bytes, delivery: Some(token) };
                                let ack = if incoming.send(message).await.is_ok() { receipt.await.ok() } else { None };
                                Wake::Committed(index, size, ack)
                            }.boxed());
                        }
                    }
                }
            }
            // Admit pending producers before choosing another bulk frame.
            for _ in 0..32 {
                match rx.try_recv() {
                    Ok(message) => enqueue(&mut queue, &mut realtime, message),
                    Err(_) => break,
                }
            }
            let ready = |lane: Lane| open[lane.index()] && initial_drain[lane.index()] && budget.ready(lane.index());
            let selected = queue.pop_ready(|l| l.index() < 3 && ready(l)).map(|(lane, frame)| (lane.index(), frame))
                .or_else(|| (open[6] && initial_drain[6] && budget.ready(6)).then(|| realtime.pop_front()).flatten().map(|frame| (6, frame)))
                .or_else(|| queue.pop_ready(ready).map(|(lane, frame)| (lane.index(), frame)));
            if let Some((index, item)) = selected {
                if item.completion.lock().cancelled() { continue; }
                if !budget.reserve(index, item.bytes.len()) { return Err(failure("V4 association budget invariant")); }
                counters.buffered_upper_bound_peak.fetch_max(budget.buffered() as u64, std::sync::atomic::Ordering::Relaxed);
                drain_deadlines[index] = Some(tokio::time::Instant::now() + SEND_TIMEOUT);
                let class = usize::from(index == 6);
                counters.attempted[class].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let mut accepted = false;
                let mut liveness = item.completion.lock().liveness.clone();
                let send = async {
                    let result = channels[index].send(bytes::BytesMut::from(item.bytes.as_slice())).await;
                    accepted = result.is_ok();
                    result.map_err(|e| failure(&format!("V4 SCTP handoff: {e}")))
                };
                let outcome = tokio::select! {
                    biased;
                    _ = liveness.changed() => Err(new_rx_error("ctox_webrtc_sender_cancelled", None)),
                    result = tokio::time::timeout(SEND_TIMEOUT, send_under_publication(item.publication.as_ref(), send)) => {
                        result.unwrap_or_else(|_| Err(failure("V4 SCTP handoff timed out")))
                    }
                };
                if accepted { counters.handed_to_sctp[class].fetch_add(1, std::sync::atomic::Ordering::Relaxed); }
                else { counters.failed[class].fetch_add(1, std::sync::atomic::Ordering::Relaxed); }
                if let Err(error) = &outcome {
                    tracing::warn!(channel = index, bytes = item.bytes.len(), realtime = (index == 6), accepted, reason = error.code(), "V4 send rejected");
                    failures.next(Incoming::SendRejected { lane: Lane::from_index(index), reason: error.code().into() });
                }
                item.completion.lock().finish(outcome.clone());
                if let Err(error) = outcome {
                    if matches!(error.code(), "ctox_webrtc_sender_cancelled" | "ctox_webrtc_publication_denied" | "ctox_webrtc_publication_callback_missing") {
                        // The native adapter hands off synchronously once its
                        // core lock is acquired. No accepted frame => no credit
                        // was consumed; other senders remain independently live.
                        if !accepted { budget.drained(index); drain_deadlines[index] = None; }
                        continue;
                    }
                    return Err(error);
                }
                // Poll events before another send so buffer notifications and
                // high-priority arrivals are not starved by a ready bulk queue.
            }
            tokio::select! {
                biased;
                event = events.next() => {
                    let (index, event) = match event.ok_or_else(|| failure("V4 channel event stream ended"))? {
                        Wake::Channel(index, event) => (index, event),
                        Wake::Delivered(index, true) => { events.push(next_event(index, channels[index].clone())); continue; }
                        Wake::Delivered(_, false) => return Err(failure("V4 receive consumer closed")),
                        Wake::Committed(index, size, Some(ack)) => {
                            enqueue_ack(&mut queue, &framer, ack, session.as_ref().unwrap())?;
                            delivery_active[index] = false;
                            delivery_bytes[index] -= size;
                            continue;
                        }
                        Wake::Committed(_, _, None) => return Err(failure("V4 application delivery abandoned")),
                    };
                    let mut pending_delivery = None;
                    match event {
                        Some(DataChannelEvent::OnOpen) => open[index] = true,
                        Some(DataChannelEvent::OnBufferedAmountLow) => { initial_drain[index] = true; budget.drained(index); drain_deadlines[index] = None; },
                        Some(DataChannelEvent::OnBufferedAmountHigh) => {},
                        Some(DataChannelEvent::OnMessage(message)) => {
                            if message.data.len() > frame_limit { return Err(failure("V4 incoming frame exceeds negotiated size")); }
                            transport_diagnostics().received(if index == 6 { MessageClass::Realtime } else { MessageClass::Reliable });
                            if index == 6 { pending_delivery = Some(Incoming::Realtime(message.data.to_vec())); }
                            else {
                                if message.data.get(4).copied() != Some(index as u8) { return Err(failure("V4 frame arrived on wrong lane")); }
                                if let Some((lane, id, bytes)) = reassembly.accept(&message.data).map_err(|e| failure(&e.to_string()))? {
                                    if let Some(binding) = &session {
                                        if id.0 == 0 {
                                            if lane != Lane::Control { return Err(failure("V4 ACK on non-control lane")); }
                                            let ack = SessionControl::decode(&bytes).map_err(|e| failure(&e.to_string()))?;
                                            binding.state.acknowledge(binding.generation, &ack).map_err(|e| failure(&e.to_string()))?;
                                        } else if id.0 <= binding.state.hello().resume_from[index].0 {
                                            let ack = binding.state.delivered(binding.generation, lane, id).map_err(|e| failure(&e.to_string()))?;
                                            enqueue_ack(&mut queue, &framer, ack, binding)?;
                                        } else if id.0 > received[index] {
                                            if received[index].checked_add(1) != Some(id.0) { return Err(failure("V4 session sequence gap")); }
                                            let size = bytes.len().max(1).saturating_add(256);
                                            if delivery_bytes[index].saturating_add(size) > ADMISSION[index] { return Err(failure("V4 application receive lane full")); }
                                            received[index] = id.0;
                                            delivery_bytes[index] += size;
                                            deliveries[index].push_back((id, bytes));
                                        }
                                        // A duplicate still pending application delivery is not ACKed.
                                        // Keep polling this channel: ACKs cannot wait behind an app callback.

                                    } else { pending_delivery = Some(Incoming::Reliable { lane, id, bytes, delivery: None }); }
                                }
                            }
                        }
                        _ => return Err(failure("V4 channel closed or failed")),
                    }
                    if let Some(message) = pending_delivery { events.push(delivery(index, incoming[index].clone(), message)); }
                    else { events.push(next_event(index, channels[index].clone())); }
                }
                message = rx.recv(), if !closing => match message { Some(message) => enqueue(&mut queue, &mut realtime, message), None => { closing = true; } },
                _ = drain.changed(), if !closing => { closing = true; rx.close(); },
                // A ready frame needs no timer or artificial 1-ms pause.
                _ = tokio::task::yield_now(), if Lane::ALL.into_iter().any(|l| open[l.index()] && initial_drain[l.index()] && budget.ready(l.index()) && !queue.queues[l.index()].is_empty()) => {},
                _ = tokio::time::sleep_until(drain_deadlines.iter().flatten().copied().min().unwrap_or_else(|| tokio::time::Instant::now() + SEND_TIMEOUT)), if budget.buffered() > 0 => return Err(failure("V4 SCTP buffer did not drain")),
                _ = tokio::time::sleep_until(startup_deadline), if initial_drain.iter().any(|ready| !*ready) => return Err(failure("V4 channel setup did not drain")) ,
            }
        }
    }.await;
    if let Err(error) = &result {
        failures.next(Incoming::Failed {
            reason: error.to_string(),
        });
    }
    rx.close();
    // Drop every outstanding completion and free its admission budget.
    drop(queue);
    drop(realtime);
    while rx.try_recv().is_ok() {}
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn v4_scheduler_contract_fairness_preemption_and_budget() {
        v4_scheduler_byte_fairness_is_eight_three_one();
        v4_scheduler_preempts_at_next_frame_including_large_commands();
        v4_scheduler_fairness_counts_bytes_not_messages_and_resets_idle_credit();
        v4_scheduler_association_budget_covers_all_channels_and_waits_for_low();
        v4_scheduler_blocked_lane_does_not_block_other_channels();
    }

    fn v4_scheduler_byte_fairness_is_eight_three_one() {
        let mut q = SendQueue::default();
        for lane in [Lane::Live, Lane::InitialSync, Lane::Transfer] {
            for _ in 0..120 {
                q.push(lane, FRAME_BYTES, ()).unwrap();
            }
        }
        let mut counts = [0; 3];
        for _ in 0..120 {
            let (lane, _) = q.pop().unwrap();
            counts[lane.index() - 3] += 1;
        }
        assert_eq!(counts, [80, 30, 10]);
    }
    fn v4_scheduler_preempts_at_next_frame_including_large_commands() {
        let mut q = SendQueue::default();
        for _ in 0..10 {
            q.push(Lane::Transfer, FRAME_BYTES, 5).unwrap();
        }
        assert_eq!(q.pop(), Some((Lane::Transfer, 5)));
        q.push(Lane::Interactive, FRAME_BYTES, 2).unwrap();
        q.push(Lane::Commands, FRAME_BYTES, 1).unwrap();
        q.push(Lane::Control, 100, 0).unwrap();
        assert_eq!(q.pop(), Some((Lane::Control, 0)));
        assert_eq!(q.pop(), Some((Lane::Commands, 1)));
        assert_eq!(q.pop(), Some((Lane::Interactive, 2)));
        assert_eq!(q.pop(), Some((Lane::Transfer, 5)));
    }
    fn v4_scheduler_fairness_counts_bytes_not_messages_and_resets_idle_credit() {
        let mut q = SendQueue::default();
        for _ in 0..256 {
            q.push(Lane::Live, FRAME_BYTES / 2, ()).unwrap();
        }
        for _ in 0..120 {
            q.push(Lane::InitialSync, FRAME_BYTES, ()).unwrap();
            q.push(Lane::Transfer, FRAME_BYTES, ()).unwrap();
        }
        let mut counts = [0; 3];
        for _ in 0..200 {
            let (lane, _) = q.pop().unwrap();
            counts[lane.index() - 3] += 1;
        }
        assert_eq!(counts, [160, 30, 10]);
    }
    fn v4_scheduler_association_budget_covers_all_channels_and_waits_for_low() {
        let mut budget = AssociationBudget::default();
        for i in 0..7 {
            assert!(budget.reserve(i, FRAME_BYTES));
        }
        assert_eq!(budget.buffered(), 112 * 1024);
        assert!(!budget.reserve(0, 1));
        assert!(!budget.reserve(7, 1));
        budget.drained(0);
        assert!(budget.reserve(0, FRAME_BYTES));
        assert!(budget.buffered() <= ASSOCIATION_BUDGET);
    }
    fn v4_scheduler_blocked_lane_does_not_block_other_channels() {
        let mut q = SendQueue::default();
        q.push(Lane::Control, FRAME_BYTES, 0).unwrap();
        q.push(Lane::Commands, FRAME_BYTES, 1).unwrap();
        q.push(Lane::Transfer, FRAME_BYTES, 5).unwrap();
        assert_eq!(
            q.pop_ready(|lane| lane != Lane::Control),
            Some((Lane::Commands, 1))
        );
        assert_eq!(
            q.pop_ready(|lane| lane != Lane::Control),
            Some((Lane::Transfer, 5))
        );
        assert_eq!(q.pop(), Some((Lane::Control, 0)));
        assert_eq!(q.queued_bytes(), 0);
    }
}
#[cfg(test)]
mod native_tests {
    use super::*;
    #[tokio::test]
    async fn v4_native_contract_admission_preemption_and_ownership() {
        tokio::time::timeout(Duration::from_secs(5), async {
            v4_scheduler_native_large_command_preempts_bulk_and_receipt_waits_for_all_frames()
                .await;
            v4_scheduler_native_bulk_admission_cannot_take_control_permits().await;
            v4_scheduler_native_cancelled_bulk_does_not_cancel_another_sender().await;
            v4_scheduler_native_counts_actual_handoff_even_when_guard_returns_an_error_after_it()
                .await;
            v4_scheduler_native_revoked_publication_does_not_close_other_senders().await;
            v4_scheduler_native_cancellation_drops_only_its_pending_physical_io().await;
            v4_scheduler_native_teardown_revokes_retained_clones().await;
        })
        .await
        .expect("independent send owners must not block each other");
    }

    use async_trait::async_trait;
    use webrtc::data_channel::{RTCDataChannelId, RTCDataChannelState};
    struct PublicFixtureGuard;
    impl WebRTCPublicationGuard for PublicFixtureGuard {
        fn with_current(&self, publish: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
            publish()
        }
    }

    #[tokio::test]
    async fn v4_session_native_replays_after_channel_loss_and_acks_while_application_is_busy() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let mut limits: [usize; 6] = ADMISSION[..6].try_into().unwrap();
            limits[1] = 2 * MAX_MESSAGE;
            let state = ReliableSession::new("native-session".into(), limits).unwrap();
            let remote = state.hello();
            let first = SessionBinding::attach(state.clone(), &remote).unwrap();
            let old: Vec<_> = (0..7).map(Channel::new).collect();
            let sender = NativeSendScheduler::from_channels_session(
                old.iter()
                    .map(|c| c.clone() as Arc<dyn DataChannel>)
                    .collect(),
                framer(),
                Some(first.clone()),
            )
            .await
            .unwrap();
            assert!(sender
                .send(Lane::Commands, WireSeq(98), &vec![0; MAX_MESSAGE + 1], None)
                .await
                .is_err());
            assert!(
                state
                    .replay(first.generation, Lane::Commands)
                    .unwrap()
                    .is_empty(),
                "unencodable data must never consume a sequence or retention budget"
            );
            sender
                .send(Lane::Commands, WireSeq(99), b"survive loss", None)
                .await
                .unwrap();
            old[1].wait_sent(1).await;
            let sent = old[1].sent.lock()[0].clone();
            sender.close_now();
            let next = SessionBinding::attach(state.clone(), &remote).unwrap();
            let channels: Vec<_> = (0..7).map(Channel::new).collect();
            let resumed = NativeSendScheduler::from_channels_session(
                channels
                    .iter()
                    .map(|c| c.clone() as Arc<dyn DataChannel>)
                    .collect(),
                framer(),
                Some(next.clone()),
            )
            .await
            .unwrap();
            assert!(resumed
                .replay_pending(&|_, _, _| Err(new_rx_error(
                    "ctox_webrtc_publication_denied",
                    None
                )))
                .await
                .is_err());
            assert!(
                channels[1].sent.lock().is_empty(),
                "replay requires fresh publication authority"
            );
            resumed
                .replay_pending(&|lane, id, body| {
                    assert_eq!(lane, Lane::Commands);
                    assert_eq!(id, WireSeq(99));
                    assert_eq!(body, b"survive loss");
                    Ok(Arc::new(PublicFixtureGuard) as Arc<dyn WebRTCPublicationGuard>)
                })
                .await
                .unwrap();
            channels[1].wait_sent(1).await;
            assert_eq!(
                channels[1].sent.lock()[0],
                sent,
                "lost message must be replayed with the same sequence"
            );
            let mut controls = resumed.take_incoming(Some(Lane::Control)).unwrap();
            fn inject(channel: &Channel, bytes: Vec<u8>) {
                channel
                    .emit
                    .send(DataChannelEvent::OnMessage(
                        webrtc::data_channel::RTCDataChannelMessage {
                            is_string: false,
                            data: bytes::BytesMut::from(bytes.as_slice()),
                        },
                    ))
                    .unwrap();
            }
            // Leave control application delivery pending. A subsequent ACK on
            // the SAME physical control channel must nevertheless be processed.
            let frame = framer()
                .encode(Lane::Control, WireSeq(1), b"slow app")
                .unwrap()
                .remove(0);
            inject(&channels[0], frame.clone());
            let Some(Incoming::Reliable {
                delivery: Some(token),
                ..
            }) = controls.recv().await
            else {
                panic!("missing application delivery")
            };
            let ack = SessionControl::Ack {
                session_id: "native-session".into(),
                lane: Lane::Commands,
                upto: WireSeq(1),
            };
            inject(
                &channels[0],
                framer()
                    .encode(Lane::Control, WireSeq(0), &ack.encode())
                    .unwrap()
                    .remove(0),
            );
            tokio::time::timeout(Duration::from_secs(1), async {
                while !state
                    .replay(next.generation, Lane::Commands)
                    .unwrap()
                    .is_empty()
                {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("ACK cannot wait for application delivery");
            assert_eq!(
                state.hello().resume_from[0],
                WireSeq(0),
                "queue admission is not delivery"
            );
            token.acknowledge().unwrap();
            channels[0].wait_sent(1).await;
            inject(&channels[0], frame);
            assert!(
                tokio::time::timeout(Duration::from_millis(10), controls.recv())
                    .await
                    .is_err(),
                "delivered duplicate must not reach the application"
            );
            resumed.close_now();
        })
        .await
        .expect("session replay and independent ACK ingress must terminate");
    }
    struct Channel {
        id: u16,
        events: tokio::sync::Mutex<mpsc::UnboundedReceiver<DataChannelEvent>>,
        emit: mpsc::UnboundedSender<DataChannelEvent>,
        sent: Mutex<Vec<Vec<u8>>>,
        notify: tokio::sync::Notify,
        closed: std::sync::atomic::AtomicBool,
        block: std::sync::atomic::AtomicBool,
        entered: tokio::sync::Notify,
    }
    impl Channel {
        fn new(id: u16) -> Arc<Self> {
            Self::with_initial(id, true)
        }
        fn with_initial(id: u16, primed: bool) -> Arc<Self> {
            let (emit, events) = mpsc::unbounded_channel();
            if primed {
                emit.send(DataChannelEvent::OnBufferedAmountLow).unwrap();
            }
            emit.send(DataChannelEvent::OnOpen).unwrap();
            Arc::new(Self {
                id,
                events: tokio::sync::Mutex::new(events),
                emit,
                sent: Mutex::new(Vec::new()),
                notify: tokio::sync::Notify::new(),
                closed: std::sync::atomic::AtomicBool::new(false),
                block: std::sync::atomic::AtomicBool::new(false),
                entered: tokio::sync::Notify::new(),
            })
        }
        async fn wait_sent(&self, n: usize) {
            tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    let ready = self.notify.notified();
                    tokio::pin!(ready);
                    ready.as_mut().enable();
                    if self.sent.lock().len() >= n {
                        return;
                    }
                    ready.await;
                }
            })
            .await
            .expect("physical handoff did not run");
        }
        fn drain(&self) {
            self.emit
                .send(DataChannelEvent::OnBufferedAmountLow)
                .unwrap();
        }
    }
    #[async_trait]
    impl DataChannel for Channel {
        async fn label(&self) -> webrtc::error::Result<String> {
            Ok(self.id.to_string())
        }
        async fn ordered(&self) -> webrtc::error::Result<bool> {
            Ok(self.id != 6)
        }
        async fn max_packet_life_time(&self) -> webrtc::error::Result<Option<u16>> {
            Ok(None)
        }
        async fn max_retransmits(&self) -> webrtc::error::Result<Option<u16>> {
            Ok((self.id == 6).then_some(0))
        }
        async fn protocol(&self) -> webrtc::error::Result<String> {
            Ok("ctox-v4".into())
        }
        async fn negotiated(&self) -> webrtc::error::Result<bool> {
            Ok(false)
        }
        fn id(&self) -> RTCDataChannelId {
            self.id
        }
        async fn ready_state(&self) -> webrtc::error::Result<RTCDataChannelState> {
            Ok(RTCDataChannelState::Open)
        }
        async fn buffered_amount_high_threshold(&self) -> webrtc::error::Result<u32> {
            Ok(FRAME_BYTES as u32)
        }
        async fn set_buffered_amount_high_threshold(&self, n: u32) -> webrtc::error::Result<()> {
            assert_eq!(n, FRAME_BYTES as u32);
            Ok(())
        }
        async fn buffered_amount_low_threshold(&self) -> webrtc::error::Result<u32> {
            Ok(0)
        }
        async fn set_buffered_amount_low_threshold(&self, n: u32) -> webrtc::error::Result<()> {
            assert_eq!(n, 0);
            Ok(())
        }
        async fn send(&self, bytes: bytes::BytesMut) -> webrtc::error::Result<()> {
            if self.block.load(std::sync::atomic::Ordering::SeqCst) {
                self.entered.notify_one();
                std::future::pending::<()>().await;
            }
            self.sent.lock().push(bytes.to_vec());
            self.notify.notify_waiters();
            Ok(())
        }
        async fn send_text(&self, _: &str) -> webrtc::error::Result<()> {
            panic!("V4 must use negotiated binary frames")
        }
        async fn poll(&self) -> Option<DataChannelEvent> {
            self.events.lock().await.recv().await
        }
        async fn close(&self) -> webrtc::error::Result<()> {
            self.closed.store(true, std::sync::atomic::Ordering::SeqCst);
            self.notify.notify_waiters();
            let _ = self.emit.send(DataChannelEvent::OnClose);
            Ok(())
        }
    }
    fn framer() -> Framer {
        Framer::new(TransportSelection {
            codec: super::super::hello::Codec::Zstd,
            frame_bytes: FRAME_BYTES as u32,
            fragmentation: true,
            reassembly: true,
        })
        .unwrap()
    }
    async fn v4_scheduler_native_large_command_preempts_bulk_and_receipt_waits_for_all_frames() {
        let channels: Vec<_> = (0..7).map(Channel::new).collect();
        let scheduler = NativeSendScheduler::from_channels(
            channels
                .iter()
                .map(|c| c.clone() as Arc<dyn DataChannel>)
                .collect(),
            framer(),
        )
        .await
        .unwrap();
        let bulk = {
            let sender = scheduler.clone();
            tokio::spawn(async move {
                sender
                    .send(Lane::Transfer, WireSeq(1), &vec![7; 50_000], None)
                    .await
            })
        };
        channels[5].wait_sent(1).await;
        assert!(!bulk.is_finished());
        let command = {
            let sender = scheduler.clone();
            tokio::spawn(async move {
                sender
                    .send(Lane::Commands, WireSeq(2), &vec![8; 30_000], None)
                    .await
            })
        };
        channels[1].wait_sent(1).await;
        scheduler
            .send(Lane::Control, WireSeq(3), b"ack", None)
            .await
            .unwrap();
        assert_eq!(channels[5].sent.lock().len(), 1);
        assert!(!command.is_finished());
        channels[1].drain();
        channels[1].wait_sent(2).await;
        command.await.unwrap().unwrap();
        assert!(!bulk.is_finished());
        for n in 2..=4 {
            channels[5].drain();
            channels[5].wait_sent(n).await;
        }
        bulk.await.unwrap().unwrap();
        assert_eq!(scheduler.handoff_counters().handed_to_sctp, [7, 0]);
        assert!(
            scheduler.handoff_counters().buffered_upper_bound_peak <= ASSOCIATION_BUDGET as u64
        );
    }
    async fn v4_scheduler_native_bulk_admission_cannot_take_control_permits() {
        let channels: Vec<_> = (0..7).map(Channel::new).collect();
        let scheduler = NativeSendScheduler::from_channels(
            channels
                .iter()
                .map(|c| c.clone() as Arc<dyn DataChannel>)
                .collect(),
            framer(),
        )
        .await
        .unwrap();
        let all_bulk = scheduler.queue_budget[5]
            .clone()
            .acquire_many_owned(ADMISSION[5] as u32)
            .await
            .unwrap();
        scheduler
            .send(Lane::Control, WireSeq(1), b"control", None)
            .await
            .unwrap();
        assert_eq!(channels[0].sent.lock().len(), 1);
        assert_eq!(channels[5].sent.lock().len(), 0);
        drop(all_bulk);
    }
    async fn v4_scheduler_native_cancelled_bulk_does_not_cancel_another_sender() {
        let channels: Vec<_> = (0..7).map(Channel::new).collect();
        let scheduler = NativeSendScheduler::from_channels(
            channels
                .iter()
                .map(|c| c.clone() as Arc<dyn DataChannel>)
                .collect(),
            framer(),
        )
        .await
        .unwrap();
        let bulk = {
            let sender = scheduler.clone();
            tokio::spawn(async move {
                sender
                    .send(Lane::Transfer, WireSeq(1), &vec![7; 50_000], None)
                    .await
            })
        };
        channels[5].wait_sent(1).await;
        bulk.abort();
        let _ = bulk.await;
        scheduler
            .send(Lane::Commands, WireSeq(2), b"command", None)
            .await
            .unwrap();
        channels[5].drain();
        scheduler
            .send(Lane::Control, WireSeq(3), b"control", None)
            .await
            .unwrap();
        assert_eq!(channels[5].sent.lock().len(), 1);
    }
    #[tokio::test(start_paused = true)]
    async fn v4_scheduler_native_drain_rejects_new_work_and_waits_for_sctp_retirement() {
        let channels: Vec<_> = (0..7).map(Channel::new).collect();
        let scheduler = NativeSendScheduler::from_channels(
            channels
                .iter()
                .map(|c| c.clone() as Arc<dyn DataChannel>)
                .collect(),
            framer(),
        )
        .await
        .unwrap();
        scheduler
            .send(Lane::Control, WireSeq(1), b"accepted", None)
            .await
            .unwrap();
        let draining = {
            let sender = scheduler.clone();
            tokio::spawn(async move { sender.drain().await })
        };
        tokio::time::sleep(Duration::from_millis(1)).await;
        assert!(
            !draining.is_finished(),
            "drain cannot acknowledge bytes still owned by SCTP"
        );
        assert!(scheduler
            .send(Lane::Commands, WireSeq(2), b"too-late", None)
            .await
            .is_err());
        channels[0].drain();
        tokio::time::timeout(Duration::from_secs(2), draining)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(channels[1].sent.lock().len(), 0);
    }
    async fn v4_scheduler_native_teardown_revokes_retained_clones() {
        let channels: Vec<_> = (0..7).map(Channel::new).collect();
        channels[5]
            .block
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let scheduler = NativeSendScheduler::from_channels(
            channels
                .iter()
                .map(|c| c.clone() as Arc<dyn DataChannel>)
                .collect(),
            framer(),
        )
        .await
        .unwrap();
        let retained = scheduler.clone();
        let pending = {
            let sender = retained.clone();
            tokio::spawn(async move {
                sender
                    .send(Lane::Transfer, WireSeq(1), b"retired-peer", None)
                    .await
            })
        };
        tokio::time::timeout(Duration::from_secs(2), channels[5].entered.notified())
            .await
            .unwrap();
        scheduler.close_now();
        assert!(retained
            .send(Lane::Control, WireSeq(2), b"stale-clone", None)
            .await
            .is_err());
        assert!(tokio::time::timeout(Duration::from_secs(2), pending)
            .await
            .unwrap()
            .unwrap()
            .is_err());
        assert!(channels[5].sent.lock().is_empty());
    }
    struct DeniedAfterCallback;
    impl WebRTCPublicationGuard for DeniedAfterCallback {
        fn with_current(&self, callback: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
            callback()?;
            Err(new_rx_error("fixture_after_handoff_revocation", None))
        }
    }
    async fn v4_scheduler_native_counts_actual_handoff_even_when_guard_returns_an_error_after_it() {
        let channels: Vec<_> = (0..7).map(Channel::new).collect();
        let scheduler = NativeSendScheduler::from_channels(
            channels
                .iter()
                .map(|c| c.clone() as Arc<dyn DataChannel>)
                .collect(),
            framer(),
        )
        .await
        .unwrap();
        assert!(scheduler
            .send(
                Lane::Commands,
                WireSeq(1),
                b"handed-before-revocation",
                Some(Arc::new(DeniedAfterCallback))
            )
            .await
            .is_err());
        assert_eq!(channels[1].sent.lock().len(), 1);
        assert_eq!(scheduler.handoff_counters().attempted, [1, 0]);
        assert_eq!(scheduler.handoff_counters().handed_to_sctp, [1, 0]);
        assert_eq!(scheduler.handoff_counters().failed, [0, 0]);
    }

    struct Denied;
    impl WebRTCPublicationGuard for Denied {
        fn with_current(&self, _: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
            Err(new_rx_error("fixture_scope_revoked", None))
        }
    }
    async fn v4_scheduler_native_revoked_publication_does_not_close_other_senders() {
        let channels: Vec<_> = (0..7).map(Channel::new).collect();
        let scheduler = NativeSendScheduler::from_channels(
            channels
                .iter()
                .map(|c| c.clone() as Arc<dyn DataChannel>)
                .collect(),
            framer(),
        )
        .await
        .unwrap();
        assert!(scheduler
            .send(
                Lane::Commands,
                WireSeq(1),
                b"revoked",
                Some(Arc::new(Denied))
            )
            .await
            .is_err());
        assert!(channels[1].sent.lock().is_empty());
        scheduler
            .send(Lane::Control, WireSeq(2), b"still-live", None)
            .await
            .unwrap();
        let counters = scheduler.handoff_counters();
        assert_eq!(counters.attempted, [2, 0]);
        assert_eq!(counters.handed_to_sctp, [1, 0]);
        assert_eq!(counters.failed, [1, 0]);
    }
    async fn v4_scheduler_native_cancellation_drops_only_its_pending_physical_io() {
        let channels: Vec<_> = (0..7).map(Channel::new).collect();
        channels[5]
            .block
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let scheduler = NativeSendScheduler::from_channels(
            channels
                .iter()
                .map(|c| c.clone() as Arc<dyn DataChannel>)
                .collect(),
            framer(),
        )
        .await
        .unwrap();
        let pending = {
            let sender = scheduler.clone();
            tokio::spawn(async move {
                sender
                    .send(Lane::Transfer, WireSeq(1), b"pending", None)
                    .await
            })
        };
        tokio::time::timeout(Duration::from_secs(2), channels[5].entered.notified())
            .await
            .unwrap();
        pending.abort();
        let _ = pending.await;
        tokio::time::timeout(
            Duration::from_secs(2),
            scheduler.send(Lane::Commands, WireSeq(2), b"another-owner", None),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(channels[5].sent.lock().is_empty());
        assert_eq!(channels[1].sent.lock().len(), 1);
    }
    #[tokio::test(start_paused = true)]
    async fn v4_scheduler_native_initial_dcep_low_cannot_credit_a_data_frame() {
        let channels: Vec<_> = (0..7).map(|id| Channel::with_initial(id, false)).collect();
        let scheduler = NativeSendScheduler::from_channels(
            channels
                .iter()
                .map(|c| c.clone() as Arc<dyn DataChannel>)
                .collect(),
            framer(),
        )
        .await
        .unwrap();
        let pending = {
            let sender = scheduler.clone();
            tokio::spawn(async move {
                sender
                    .send(Lane::Control, WireSeq(1), b"after-dcep", None)
                    .await
            })
        };
        tokio::time::sleep(Duration::from_millis(1)).await;
        assert!(channels[0].sent.lock().is_empty());
        channels[0].drain();
        pending.await.unwrap().unwrap();
        assert_eq!(channels[0].sent.lock().len(), 1);
    }
}

#[cfg(test)]
mod loopback_tests {
    use super::*;
    use async_trait::async_trait;
    use webrtc::peer_connection::{
        PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceCandidateInit,
        RTCPeerConnectionIceEvent,
    };

    struct Events {
        candidates: mpsc::UnboundedSender<RTCIceCandidateInit>,
        channels: mpsc::UnboundedSender<Arc<dyn DataChannel>>,
    }
    #[async_trait]
    impl PeerConnectionEventHandler for Events {
        async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
            if let Ok(candidate) = event.candidate.to_json() {
                let _ = self.candidates.send(candidate);
            }
        }
        async fn on_data_channel(&self, channel: Arc<dyn DataChannel>) {
            let _ = self.channels.send(channel);
        }
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn v4_scheduler_real_webrtc_loopback_drains_dcep_and_reassembles_multiple_frames() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let (a_candidates, mut ac) = mpsc::unbounded_channel();
        let (b_candidates, mut bc) = mpsc::unbounded_channel();
        let (a_channels, _unused) = mpsc::unbounded_channel();
        let (b_channels, mut incoming) = mpsc::unbounded_channel();
        let a: Arc<dyn PeerConnection> = Arc::new(
            PeerConnectionBuilder::new()
                .with_handler(Arc::new(Events {
                    candidates: a_candidates,
                    channels: a_channels,
                }))
                .with_udp_addrs(vec!["127.0.0.1:0"])
                .build()
                .await
                .unwrap(),
        );
        let b: Arc<dyn PeerConnection> = Arc::new(
            PeerConnectionBuilder::new()
                .with_handler(Arc::new(Events {
                    candidates: b_candidates,
                    channels: b_channels,
                }))
                .with_udp_addrs(vec!["127.0.0.1:0"])
                .build()
                .await
                .unwrap(),
        );
        let mut forwarding = tokio::task::JoinSet::new();
        let result = tokio::time::timeout(Duration::from_secs(12), async {
            let transport = TransportSelection {
                codec: super::super::hello::Codec::Zstd,
                frame_bytes: FRAME_BYTES as u32,
                fragmentation: true,
                reassembly: true,
            };
            let state_a = ReliableSession::new(
                "loopback-session".into(),
                ADMISSION[..6].try_into().unwrap(),
            )
            .unwrap();
            let state_b = ReliableSession::new(
                "loopback-session".into(),
                ADMISSION[..6].try_into().unwrap(),
            )
            .unwrap();
            let binding_a = SessionBinding::attach(state_a.clone(), &state_b.hello()).unwrap();
            let binding_b = SessionBinding::attach(state_b.clone(), &state_a.hello()).unwrap();
            let sender =
                NativeSendScheduler::create_inner(&a, transport.clone(), Some(binding_a.clone()))
                    .await
                    .unwrap();
            let offer = a.create_offer(None).await.unwrap();
            a.set_local_description(offer.clone()).await.unwrap();
            b.set_remote_description(offer).await.unwrap();
            let answer = b.create_answer(None).await.unwrap();
            b.set_local_description(answer.clone()).await.unwrap();
            a.set_remote_description(answer).await.unwrap();
            let target = b.clone();
            forwarding.spawn(async move {
                while let Some(candidate) = ac.recv().await {
                    target.add_ice_candidate(candidate).await.unwrap();
                }
            });
            let target = a.clone();
            forwarding.spawn(async move {
                while let Some(candidate) = bc.recv().await {
                    target.add_ice_candidate(candidate).await.unwrap();
                }
            });
            let mut channels: Vec<Option<Arc<dyn DataChannel>>> = vec![None; 7];
            for _ in 0..7 {
                let channel = incoming.recv().await.unwrap();
                let label = channel.label().await.unwrap();
                let index = Lane::ALL
                    .iter()
                    .position(|lane| lane.label() == label)
                    .unwrap_or_else(|| {
                        assert_eq!(label, REALTIME_LABEL);
                        6
                    });
                assert_eq!(channel.ordered().await.unwrap(), index != 6);
                // Incoming optional reliability metadata is always None in
                // rtc's accept adapter. Constructor validation above checks the
                // native-created channel; M3 measures real-time loss separately.

                channels[index] = Some(channel);
            }
            let receiver = NativeSendScheduler::from_channels_session(
                channels.into_iter().map(Option::unwrap).collect(),
                Framer::new(transport).unwrap(),
                Some(binding_b),
            )
            .await
            .unwrap();
            let mut controls = receiver.take_incoming(Some(Lane::Control)).unwrap();
            let mut bulk = receiver.take_incoming(Some(Lane::Transfer)).unwrap();
            sender
                .send(Lane::Control, WireSeq(u64::MAX), b"loopback-control", None)
                .await
                .unwrap();
            let Some(Incoming::Reliable {
                id: WireSeq(1),
                bytes,
                delivery: Some(delivery),
                ..
            }) = controls.recv().await
            else {
                panic!("session control delivery missing")
            };
            assert_eq!(bytes, b"loopback-control");
            assert_eq!(
                state_a
                    .replay(binding_a.generation, Lane::Control)
                    .unwrap()
                    .len(),
                1
            );
            delivery.acknowledge().unwrap();
            let payload = vec![9; 50_000];
            sender
                .send(Lane::Transfer, WireSeq(2), &payload, None)
                .await
                .unwrap();
            let Some(Incoming::Reliable {
                bytes,
                delivery: Some(delivery),
                ..
            }) = bulk.recv().await
            else {
                panic!("session bulk delivery missing")
            };
            assert_eq!(bytes, payload);
            delivery.acknowledge().unwrap();
            tokio::time::timeout(Duration::from_secs(2), async {
                while !state_a
                    .replay(binding_a.generation, Lane::Transfer)
                    .unwrap()
                    .is_empty()
                    || !state_a
                        .replay(binding_a.generation, Lane::Control)
                        .unwrap()
                        .is_empty()
                {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("real receiver ACK must retire retained data");
            sender.drain().await.unwrap();
            assert_eq!(sender.handoff_counters().handed_to_sctp, [5, 0]);
            assert!(
                sender.handoff_counters().buffered_upper_bound_peak <= ASSOCIATION_BUDGET as u64
            );
        })
        .await;
        forwarding.abort_all();
        while forwarding.join_next().await.is_some() {}
        let _ = a.close().await;
        let _ = b.close().await;
        result.expect("real WebRTC loopback must retire channel/data credits within 12 seconds");
    }
}
