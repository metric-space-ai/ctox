//! One V4 sender per association, with strict 0–2 priority and byte DRR 8:3:1.
//! No producer writes a V4 DataChannel directly. Receipts mean SCTP handoff.
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;
use futures::{stream::FuturesUnordered, FutureExt, StreamExt};
use parking_lot::Mutex;
use tokio::sync::{mpsc, oneshot, Semaphore};
use webrtc::data_channel::{DataChannel, DataChannelEvent, RTCDataChannelInit};
use webrtc::peer_connection::PeerConnection;
use super::framing::{Framer, Reassembler, MAX_MESSAGE};
use super::hello::{TransportSelection, WireSeq};
use super::lanes::{Lane, ASSOCIATION_BUDGET, BULK_WEIGHTS, FRAME_BYTES, REALTIME_LABEL};
use super::super::connection_handler_rs::send_under_publication;
use super::super::connection_handler_rs::send_result::{transport_diagnostics, MessageClass};
use super::super::webrtc_types::WebRTCPublicationGuard;
use crate::rx_error::{new_rx_error, RxError, RxResult};
use crate::rxjs_compat::{RxStream, RxSubject};

const QUEUE_BUDGET: usize = 192 * 1024 * 1024;
const ADMISSION: [usize; 7] = [128*1024, 1024*1024, 128*1024, 4*1024*1024, 8*1024*1024, 16*1024*1024, 64*1024];
const SEND_TIMEOUT: Duration = Duration::from_secs(30);

struct Entry<T> { bytes: usize, value: T }
/// Pure scheduling core, also used by transport benchmarks without a network.
pub struct SendQueue<T> {
    queues: [VecDeque<Entry<T>>; 6],
    deficit: [usize; 3],
    cursor: usize,
    credited: [bool; 3],
    bytes: usize,
}
impl<T> Default for SendQueue<T> {
    fn default() -> Self { Self { queues: std::array::from_fn(|_| VecDeque::new()), deficit: [0;3], cursor: 0, credited: [false;3], bytes: 0 } }
}
impl<T> SendQueue<T> {
    pub fn push(&mut self, lane: Lane, bytes: usize, value: T) -> Result<(), T> {
        if bytes > FRAME_BYTES || self.bytes.saturating_add(bytes) > QUEUE_BUDGET { return Err(value); }
        self.bytes += bytes; self.queues[lane.index()].push_back(Entry { bytes, value }); Ok(())
    }
    pub fn queued_bytes(&self) -> usize { self.bytes }
    pub fn pop(&mut self) -> Option<(Lane, T)> { self.pop_ready(|_| true) }
    pub fn pop_ready(&mut self, ready: impl Fn(Lane) -> bool) -> Option<(Lane, T)> {
        for lane in Lane::ALL.into_iter().take(3) {
            if ready(lane) {
                if let Some(item) = self.queues[lane.index()].pop_front() {
                    self.bytes -= item.bytes; return Some((lane, item.value));
                }
            }
        }
        for _ in 0..6 {
            let i = self.cursor; let lane = Lane::ALL[i+3];
            if !ready(lane) || self.queues[i+3].is_empty() {
                if self.queues[i+3].is_empty() { self.deficit[i] = 0; self.credited[i] = false; }
                self.cursor = (i+1)%3; continue;
            }
            if !self.credited[i] { self.deficit[i] += BULK_WEIGHTS[i] * FRAME_BYTES; self.credited[i] = true; }
            let bytes = self.queues[i+3].front().unwrap().bytes;
            if bytes <= self.deficit[i] {
                let item = self.queues[i+3].pop_front().unwrap();
                self.deficit[i] -= bytes; self.bytes -= bytes;
                return Some((lane, item.value));
            }
            self.cursor = (i+1)%3; self.credited[i] = false;
        }
        None
    }
}

/// The adapter uses an empty-buffer event as the credit fence. Because there
/// is exactly one outstanding frame per channel, even delayed events cannot
/// credit bytes sent after the event. Seven channels cap aggregate data at
/// 112 KiB; the remaining 16 KiB is reserved for association/control overhead.
#[derive(Default)]
pub struct AssociationBudget { outstanding: [usize; 7] }
impl AssociationBudget {
    pub fn reserve(&mut self, channel: usize, bytes: usize) -> bool {
        if channel >= 7 || bytes > FRAME_BYTES || self.outstanding[channel] != 0
            || self.buffered().saturating_add(bytes) > ASSOCIATION_BUDGET { return false; }
        self.outstanding[channel] = bytes.max(1); true
    }
    pub fn ready(&self, channel: usize) -> bool { channel < 7 && self.outstanding[channel] == 0 }
    pub fn drained(&mut self, channel: usize) { if let Some(value) = self.outstanding.get_mut(channel) { *value = 0; } }
    pub fn buffered(&self) -> usize { self.outstanding.iter().sum() }
}

#[derive(Clone, Debug)]
pub enum Incoming {
    Reliable { lane: Lane, id: WireSeq, bytes: Vec<u8> },
    Realtime(Vec<u8>),
    Failed { reason: String },
}
struct Completion {
    remaining: usize,
    result: Option<oneshot::Sender<RxResult<()>>>,
    _permit: tokio::sync::OwnedSemaphorePermit,
}
impl Completion {
    fn cancelled(&self) -> bool { self.result.as_ref().is_none_or(|r| r.is_closed()) }
    fn finish(&mut self, result: RxResult<()>) {
        if result.is_err() {
            if let Some(sender) = self.result.take() { let _ = sender.send(result); }
        } else {
            self.remaining = self.remaining.saturating_sub(1);
            if self.remaining == 0 { if let Some(sender) = self.result.take() { let _ = sender.send(Ok(())); } }
        }
    }
}
struct Outbound {
    lane: Option<Lane>, frames: Vec<Vec<u8>>,
    completion: Arc<Mutex<Completion>>,
    publication: Option<Arc<dyn WebRTCPublicationGuard>>,
}
struct Physical {
    bytes: Vec<u8>, completion: Arc<Mutex<Completion>>,
    publication: Option<Arc<dyn WebRTCPublicationGuard>>,
}
struct Task(tokio::task::AbortHandle);
impl Drop for Task { fn drop(&mut self) { self.0.abort(); } }

#[derive(Default)]
struct Counters {
    attempted: [std::sync::atomic::AtomicU64;2],
    handed_to_sctp: [std::sync::atomic::AtomicU64;2],
    failed: [std::sync::atomic::AtomicU64;2],
    buffered_upper_bound_peak: std::sync::atomic::AtomicU64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HandoffCounters {
    pub attempted: [u64;2],
    pub handed_to_sctp: [u64;2],
    pub failed: [u64;2],
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
    framer: Framer,
    queue_budget: [Arc<Semaphore>; 7],
    receivers: Arc<Mutex<[Option<mpsc::Receiver<Incoming>>; 7]>>,
    failures: RxSubject<Incoming>,
    _task: Arc<Task>,
}
fn failure(message: &str) -> RxError {
    new_rx_error("RC_WEBRTC_PEER", Some(serde_json::json!({"message": message, "transport": "v4"})))
}
impl NativeSendScheduler {
    /// Called only after authenticated hello negotiation. Native creates every
    /// channel, including real-time; browser does not choose reliability.
    pub async fn create(pc: &Arc<dyn PeerConnection>, transport: TransportSelection) -> RxResult<Self> {
        let framer = Framer::new(transport).map_err(|e| failure(&e.to_string()))?;
        let mut channels: Vec<Arc<dyn DataChannel>> = Vec::new();
        let mut rollback = CloseChannels(Vec::new());
        for label in Lane::ALL.iter().map(|l| l.label()).chain([REALTIME_LABEL]) {
            let realtime = label == REALTIME_LABEL;
            let init = RTCDataChannelInit { ordered: !realtime, max_retransmits: realtime.then_some(0), ..Default::default() };
            let channel = match pc.create_data_channel(label, Some(init)).await {
                Ok(channel) => channel,
                Err(error) => {
                    for channel in &channels { let _: webrtc::error::Result<()> = channel.close().await; }
                    return Err(failure(&format!("create V4 channel: {error}")));
                }
            };
            rollback.0.push(channel.clone());
            channels.push(channel);
        }
        let scheduler = Self::from_channels(channels, framer).await?;
        rollback.0.clear();
        Ok(scheduler)
    }
    async fn from_channels(channels: Vec<Arc<dyn DataChannel>>, framer: Framer) -> RxResult<Self> {
        if channels.len() != 7 { return Err(failure("V4 requires six reliable channels and one real-time channel")); }
        for channel in &channels {
            // Zero is essential: a low event must prove that the previous
            // frame is completely retired, not just below a per-channel cap.
            channel.set_buffered_amount_low_threshold(0).await.map_err(|e| failure(&e.to_string()))?;
            channel.set_buffered_amount_high_threshold(FRAME_BYTES as u32).await.map_err(|e| failure(&e.to_string()))?;
        }
        let (tx, rx) = mpsc::channel(32);
        let counters = Arc::new(Counters::default());
        let mut receivers = Vec::new();
        let senders: [mpsc::Sender<Incoming>; 7] = std::array::from_fn(|_| {
            let (tx, rx) = mpsc::channel(1);
            receivers.push(Some(rx)); tx
        });
        let failures = RxSubject::with_lag_signal(16, |_| Some(Incoming::Failed { reason: "V4 diagnostic observer lagged".into() }));
        let task = tokio::spawn(run(channels, framer.clone(), rx, senders, failures.clone(), counters.clone()));
        Ok(Self { tx, counters, framer, queue_budget: std::array::from_fn(|i| Arc::new(Semaphore::new(ADMISSION[i]))), receivers: Arc::new(Mutex::new(receivers.try_into().ok().unwrap())), failures, _task: Arc::new(Task(task.abort_handle())) })
    }
    /// Indices: 0 reliable frames, 1 unreliable real-time packets.
    pub fn handoff_counters(&self) -> HandoffCounters { self.counters.snapshot() }
    pub fn take_incoming(&self, lane: Option<Lane>) -> Option<mpsc::Receiver<Incoming>> {
        self.receivers.lock()[lane.map_or(6, Lane::index)].take()
    }
    pub fn failures(&self) -> RxStream<Incoming> { self.failures.subscribe() }
    pub async fn send(&self, lane: Lane, id: WireSeq, bytes: &[u8], publication: Option<Arc<dyn WebRTCPublicationGuard>>) -> RxResult<()> {
        self.submit(Some(lane), id, bytes, publication).await
    }
    pub async fn send_realtime(&self, bytes: &[u8]) -> RxResult<()> {
        self.submit(None, WireSeq(0), bytes, None).await
    }
    async fn submit(&self, lane: Option<Lane>, id: WireSeq, bytes: &[u8], publication: Option<Arc<dyn WebRTCPublicationGuard>>) -> RxResult<()> {
        if bytes.len() > MAX_MESSAGE || (lane.is_none() && (bytes.is_empty() || bytes.len() > self.framer.frame_bytes())) {
            return Err(failure("V4 message exceeds negotiated size limit"));
        }
        // Bound allocations before encoding; framing overhead fits in 4 KiB
        // per MiB, reserve conservatively. Largest message needs a full turn.
        // Admission is separate per lane: a 16-MiB bulk producer cannot
        // consume the control/command permits and recreate head-of-line delay.
        let index = lane.map_or(6, Lane::index);
        let charge = bytes.len().saturating_add(bytes.len()/16 + FRAME_BYTES).min(ADMISSION[index]).max(1);
        let permit = self.queue_budget[index].clone().acquire_many_owned(charge as u32).await.map_err(|_| failure("V4 scheduler closed"))?;
        let frames = match lane {
            Some(lane) => self.framer.encode(lane, id, bytes).map_err(|e| failure(&e.to_string()))?,
            None => vec![bytes.to_vec()],
        };
        let (sender, receipt) = oneshot::channel();
        let completion = Arc::new(Mutex::new(Completion { remaining: frames.len(), result: Some(sender), _permit: permit }));
        self.tx.send(Outbound { lane, frames, completion, publication }).await.map_err(|_| failure("V4 scheduler closed"))?;
        receipt.await.map_err(|_| failure("V4 association closed before SCTP handoff"))?
    }
}

enum Wake { Channel(usize, Option<DataChannelEvent>), Delivered(usize, bool) }
fn next_event(index: usize, channel: Arc<dyn DataChannel>) -> futures::future::BoxFuture<'static, Wake> {
    async move { Wake::Channel(index, channel.poll().await) }.boxed()
}
fn delivery(index: usize, tx: mpsc::Sender<Incoming>, message: Incoming) -> futures::future::BoxFuture<'static, Wake> {
    async move { Wake::Delivered(index, tx.send(message).await.is_ok()) }.boxed()
}
struct CloseChannels(Vec<Arc<dyn DataChannel>>);
impl Drop for CloseChannels {
    fn drop(&mut self) {
        let channels = std::mem::take(&mut self.0);
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = tokio::time::timeout(Duration::from_secs(5), async move {
                    for channel in channels { let _ = channel.close().await; }
                }).await;
            });
        }
    }
}
fn enqueue(queue: &mut SendQueue<Physical>, realtime: &mut VecDeque<Physical>, message: Outbound) {
    for bytes in message.frames {
        let item = Physical { bytes, completion: message.completion.clone(), publication: message.publication.clone() };
        if let Some(lane) = message.lane {
            let size = item.bytes.len();
            if let Err(item) = queue.push(lane, size, item) { item.completion.lock().finish(Err(failure("V4 send queue full"))); }
        } else if realtime.len() < 32 { realtime.push_back(item); }
        else { item.completion.lock().finish(Err(failure("V4 real-time queue full"))); }
    }
}
async fn run(channels: Vec<Arc<dyn DataChannel>>, framer: Framer, mut rx: mpsc::Receiver<Outbound>, incoming: [mpsc::Sender<Incoming>;7], failures: RxSubject<Incoming>, counters: Arc<Counters>) {
    let _close_channels = CloseChannels(channels.clone());
    let mut events = FuturesUnordered::new();
    for (i, channel) in channels.iter().enumerate() { events.push(next_event(i, channel.clone())); }
    let mut queue = SendQueue::default(); let mut realtime = VecDeque::new();
    let mut budget = AssociationBudget::default(); let mut open = [false;7];
    let mut drain_deadlines: [Option<tokio::time::Instant>;7] = [None;7];
    let mut reassembly = Reassembler::new(framer);
    let result: RxResult<()> = async {
        loop {
            if drain_deadlines.iter().flatten().any(|t| *t <= tokio::time::Instant::now()) {
                return Err(failure("V4 SCTP buffer did not drain before its deadline"));
            }
            // Admit pending producers before choosing another bulk frame.
            for _ in 0..32 {
                match rx.try_recv() {
                    Ok(message) => enqueue(&mut queue, &mut realtime, message),
                    Err(_) => break,
                }
            }
            let ready = |lane: Lane| open[lane.index()] && budget.ready(lane.index());
            let selected = queue.pop_ready(|l| l.index() < 3 && ready(l)).map(|(lane, frame)| (lane.index(), frame))
                .or_else(|| (open[6] && budget.ready(6)).then(|| realtime.pop_front()).flatten().map(|frame| (6, frame)))
                .or_else(|| queue.pop_ready(ready).map(|(lane, frame)| (lane.index(), frame)));
            if let Some((index, item)) = selected {
                if item.completion.lock().cancelled() { continue; }
                if !budget.reserve(index, item.bytes.len()) { return Err(failure("V4 association budget invariant")); }
                counters.buffered_upper_bound_peak.fetch_max(budget.buffered() as u64, std::sync::atomic::Ordering::Relaxed);
                drain_deadlines[index] = Some(tokio::time::Instant::now() + SEND_TIMEOUT);
                let class = usize::from(index == 6);
                counters.attempted[class].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let send = async {
                    // Binary framing is checked here exactly as text handoffs
                    // are in P4-b. No success receipt before SCTP acceptance.
                    channels[index].send(bytes::BytesMut::from(item.bytes.as_slice())).await.map_err(|e| failure(&format!("V4 SCTP handoff: {e}")))
                };
                let outcome = tokio::time::timeout(SEND_TIMEOUT, send_under_publication(item.publication.as_ref(), send)).await
                    .unwrap_or_else(|_| Err(failure("V4 SCTP handoff timed out")));
                if outcome.is_ok() { counters.handed_to_sctp[class].fetch_add(1, std::sync::atomic::Ordering::Relaxed); }
                else {
                    counters.failed[class].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    tracing::warn!(channel = index, bytes = item.bytes.len(), realtime = (index == 6), "V4 SCTP handoff failed");
                }
                item.completion.lock().finish(outcome.clone());
                outcome?;
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
                    };
                    let mut pending_delivery = None;
                    match event {
                        Some(DataChannelEvent::OnOpen) => open[index] = true,
                        Some(DataChannelEvent::OnBufferedAmountLow) => { budget.drained(index); drain_deadlines[index] = None; },
                        Some(DataChannelEvent::OnBufferedAmountHigh) => {},
                        Some(DataChannelEvent::OnMessage(message)) => {
                            transport_diagnostics().received(if index == 6 { MessageClass::Realtime } else { MessageClass::Reliable });
                            if index == 6 { pending_delivery = Some(Incoming::Realtime(message.data.to_vec())); }
                            else {
                                if message.data.get(4).copied() != Some(index as u8) { return Err(failure("V4 frame arrived on wrong lane")); }
                                if let Some((lane, id, bytes)) = reassembly.accept(&message.data).map_err(|e| failure(&e.to_string()))? {
                                    pending_delivery = Some(Incoming::Reliable { lane, id, bytes });
                                }
                            }
                        }
                        _ => return Err(failure("V4 channel closed or failed")),
                    }
                    if let Some(message) = pending_delivery { events.push(delivery(index, incoming[index].clone(), message)); }
                    else { events.push(next_event(index, channels[index].clone())); }
                }
                message = rx.recv() => match message { Some(message) => enqueue(&mut queue, &mut realtime, message), None => return Ok(()) },
                // A ready frame needs no timer or artificial 1-ms pause.
                _ = tokio::task::yield_now(), if Lane::ALL.into_iter().any(|l| open[l.index()] && budget.ready(l.index()) && !queue.queues[l.index()].is_empty()) => {},
                _ = tokio::time::sleep_until(drain_deadlines.iter().flatten().copied().min().unwrap_or_else(|| tokio::time::Instant::now() + SEND_TIMEOUT)), if budget.buffered() > 0 => return Err(failure("V4 SCTP buffer did not drain")),
            }
        }
    }.await;
    if let Err(error) = result { failures.next(Incoming::Failed { reason: error.to_string() }); }
    rx.close();
    // Drop every outstanding completion and free its admission budget.
    drop(queue); drop(realtime);
    while rx.try_recv().is_ok() {}
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn v4_scheduler_byte_fairness_is_eight_three_one() {
        let mut q = SendQueue::default();
        for lane in [Lane::Live, Lane::InitialSync, Lane::Transfer] {
            for _ in 0..120 { q.push(lane, FRAME_BYTES, ()).unwrap(); }
        }
        let mut counts = [0;3];
        for _ in 0..120 { let (lane, _) = q.pop().unwrap(); counts[lane.index()-3] += 1; }
        assert_eq!(counts, [80,30,10]);
    }
    #[test]
    fn v4_scheduler_preempts_at_next_frame_including_large_commands() {
        let mut q = SendQueue::default();
        for _ in 0..10 { q.push(Lane::Transfer, FRAME_BYTES, 5).unwrap(); }
        assert_eq!(q.pop(), Some((Lane::Transfer, 5)));
        q.push(Lane::Interactive, FRAME_BYTES, 2).unwrap();
        q.push(Lane::Commands, FRAME_BYTES, 1).unwrap();
        q.push(Lane::Control, 100, 0).unwrap();
        assert_eq!(q.pop(), Some((Lane::Control, 0)));
        assert_eq!(q.pop(), Some((Lane::Commands, 1)));
        assert_eq!(q.pop(), Some((Lane::Interactive, 2)));
        assert_eq!(q.pop(), Some((Lane::Transfer, 5)));
    }
    #[test]
    fn v4_scheduler_fairness_counts_bytes_not_messages_and_resets_idle_credit() {
        let mut q = SendQueue::default();
        for _ in 0..256 { q.push(Lane::Live, FRAME_BYTES/2, ()).unwrap(); }
        for _ in 0..120 { q.push(Lane::InitialSync, FRAME_BYTES, ()).unwrap(); q.push(Lane::Transfer, FRAME_BYTES, ()).unwrap(); }
        let mut counts = [0;3];
        for _ in 0..200 { let (lane, _) = q.pop().unwrap(); counts[lane.index()-3] += 1; }
        assert_eq!(counts, [160,30,10]);
    }
    #[test]
    fn v4_scheduler_association_budget_covers_all_channels_and_waits_for_low() {
        let mut budget = AssociationBudget::default();
        for i in 0..7 { assert!(budget.reserve(i, FRAME_BYTES)); }
        assert_eq!(budget.buffered(), 112*1024);
        assert!(!budget.reserve(0, 1)); assert!(!budget.reserve(7, 1));
        budget.drained(0); assert!(budget.reserve(0, FRAME_BYTES));
        assert!(budget.buffered() <= ASSOCIATION_BUDGET);
    }
    #[test]
    fn v4_scheduler_blocked_lane_does_not_block_other_channels() {
        let mut q = SendQueue::default();
        q.push(Lane::Control, FRAME_BYTES, 0).unwrap();
        q.push(Lane::Commands, FRAME_BYTES, 1).unwrap();
        q.push(Lane::Transfer, FRAME_BYTES, 5).unwrap();
        assert_eq!(q.pop_ready(|lane| lane != Lane::Control), Some((Lane::Commands, 1)));
        assert_eq!(q.pop_ready(|lane| lane != Lane::Control), Some((Lane::Transfer, 5)));
        assert_eq!(q.pop(), Some((Lane::Control, 0)));
        assert_eq!(q.queued_bytes(), 0);
    }
}
#[cfg(test)]
mod native_tests {
    use super::*;
    use async_trait::async_trait;
    use webrtc::data_channel::{RTCDataChannelId, RTCDataChannelState};
    struct Channel {
        id: u16,
        events: tokio::sync::Mutex<mpsc::UnboundedReceiver<DataChannelEvent>>,
        emit: mpsc::UnboundedSender<DataChannelEvent>,
        sent: Mutex<Vec<Vec<u8>>>,
        notify: tokio::sync::Notify,
        closed: std::sync::atomic::AtomicBool,
    }
    impl Channel {
        fn new(id: u16) -> Arc<Self> {
            let (emit, events) = mpsc::unbounded_channel();
            emit.send(DataChannelEvent::OnOpen).unwrap();
            Arc::new(Self { id, events: tokio::sync::Mutex::new(events), emit, sent: Mutex::new(Vec::new()), notify: tokio::sync::Notify::new(), closed: std::sync::atomic::AtomicBool::new(false) })
        }
        async fn wait_sent(&self, n: usize) {
            tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    let ready = self.notify.notified();
                    tokio::pin!(ready); ready.as_mut().enable();
                    if self.sent.lock().len() >= n { return; }
                    ready.await;
                }
            }).await.expect("physical handoff did not run");
        }
        fn drain(&self) { self.emit.send(DataChannelEvent::OnBufferedAmountLow).unwrap(); }
    }
    #[async_trait]
    impl DataChannel for Channel {
        async fn label(&self) -> webrtc::error::Result<String> { Ok(self.id.to_string()) }
        async fn ordered(&self) -> webrtc::error::Result<bool> { Ok(self.id != 6) }
        async fn max_packet_life_time(&self) -> webrtc::error::Result<Option<u16>> { Ok(None) }
        async fn max_retransmits(&self) -> webrtc::error::Result<Option<u16>> { Ok((self.id == 6).then_some(0)) }
        async fn protocol(&self) -> webrtc::error::Result<String> { Ok("ctox-v4".into()) }
        async fn negotiated(&self) -> webrtc::error::Result<bool> { Ok(false) }
        fn id(&self) -> RTCDataChannelId { self.id }
        async fn ready_state(&self) -> webrtc::error::Result<RTCDataChannelState> { Ok(RTCDataChannelState::Open) }
        async fn buffered_amount_high_threshold(&self) -> webrtc::error::Result<u32> { Ok(FRAME_BYTES as u32) }
        async fn set_buffered_amount_high_threshold(&self, n: u32) -> webrtc::error::Result<()> { assert_eq!(n, FRAME_BYTES as u32); Ok(()) }
        async fn buffered_amount_low_threshold(&self) -> webrtc::error::Result<u32> { Ok(0) }
        async fn set_buffered_amount_low_threshold(&self, n: u32) -> webrtc::error::Result<()> { assert_eq!(n, 0); Ok(()) }
        async fn send(&self, bytes: bytes::BytesMut) -> webrtc::error::Result<()> {
            self.sent.lock().push(bytes.to_vec()); self.notify.notify_waiters(); Ok(())
        }
        async fn send_text(&self, _: &str) -> webrtc::error::Result<()> { panic!("V4 must use negotiated binary frames") }
        async fn poll(&self) -> Option<DataChannelEvent> { self.events.lock().await.recv().await }
        async fn close(&self) -> webrtc::error::Result<()> {
            self.closed.store(true, std::sync::atomic::Ordering::SeqCst);
            self.notify.notify_waiters();
            let _ = self.emit.send(DataChannelEvent::OnClose); Ok(())
        }
    }
    fn framer() -> Framer {
        Framer::new(TransportSelection { codec: super::super::hello::Codec::Zstd, frame_bytes: FRAME_BYTES as u32, fragmentation: true, reassembly: true }).unwrap()
    }
    #[tokio::test]
    async fn v4_scheduler_native_large_command_preempts_bulk_and_receipt_waits_for_all_frames() {
        let channels: Vec<_> = (0..7).map(Channel::new).collect();
        let scheduler = NativeSendScheduler::from_channels(channels.iter().map(|c| c.clone() as Arc<dyn DataChannel>).collect(), framer()).await.unwrap();
        let bulk = {
            let sender = scheduler.clone();
            tokio::spawn(async move { sender.send(Lane::Transfer, WireSeq(1), &vec![7; 50_000], None).await })
        };
        channels[5].wait_sent(1).await;
        assert!(!bulk.is_finished());
        let command = {
            let sender = scheduler.clone();
            tokio::spawn(async move { sender.send(Lane::Commands, WireSeq(2), &vec![8; 30_000], None).await })
        };
        channels[1].wait_sent(1).await;
        scheduler.send(Lane::Control, WireSeq(3), b"ack", None).await.unwrap();
        assert_eq!(channels[5].sent.lock().len(), 1);
        assert!(!command.is_finished());
        channels[1].drain();
        channels[1].wait_sent(2).await;
        command.await.unwrap().unwrap();
        assert!(!bulk.is_finished());
        for n in 2..=4 { channels[5].drain(); channels[5].wait_sent(n).await; }
        bulk.await.unwrap().unwrap();
        assert_eq!(scheduler.handoff_counters().handed_to_sctp, [7, 0]);
        assert!(scheduler.handoff_counters().buffered_upper_bound_peak <= ASSOCIATION_BUDGET as u64);
    }
    #[tokio::test]
    async fn v4_scheduler_native_bulk_admission_cannot_take_control_permits() {
        let channels: Vec<_> = (0..7).map(Channel::new).collect();
        let scheduler = NativeSendScheduler::from_channels(channels.iter().map(|c| c.clone() as Arc<dyn DataChannel>).collect(), framer()).await.unwrap();
        let all_bulk = scheduler.queue_budget[5].clone().acquire_many_owned(ADMISSION[5] as u32).await.unwrap();
        scheduler.send(Lane::Control, WireSeq(1), b"control", None).await.unwrap();
        assert_eq!(channels[0].sent.lock().len(), 1);
        assert_eq!(channels[5].sent.lock().len(), 0);
        drop(all_bulk);
    }
    #[tokio::test]
    async fn v4_scheduler_native_cancelled_bulk_does_not_cancel_another_sender() {
        let channels: Vec<_> = (0..7).map(Channel::new).collect();
        let scheduler = NativeSendScheduler::from_channels(channels.iter().map(|c| c.clone() as Arc<dyn DataChannel>).collect(), framer()).await.unwrap();
        let bulk = {
            let sender = scheduler.clone();
            tokio::spawn(async move { sender.send(Lane::Transfer, WireSeq(1), &vec![7; 50_000], None).await })
        };
        channels[5].wait_sent(1).await;
        bulk.abort(); let _ = bulk.await;
        scheduler.send(Lane::Commands, WireSeq(2), b"command", None).await.unwrap();
        channels[5].drain();
        scheduler.send(Lane::Control, WireSeq(3), b"control", None).await.unwrap();
        assert_eq!(channels[5].sent.lock().len(), 1);
    }
}
