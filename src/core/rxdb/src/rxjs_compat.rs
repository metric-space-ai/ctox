//! RxJS compatibility layer (gap-item **N3**).
//!
//! Upstream RxDB uses RxJS Observables / Subjects / BehaviorSubjects as the
//! reactive backbone. Rust has no direct equivalent. The mapping rules in
//! `PORT_STYLE.md` §5 are implemented here:
//!
//! | RxJS                  | Rust                                                          |
//! |-----------------------|---------------------------------------------------------------|
//! | `Subject<T>`          | [`RxSubject<T>`] (wraps `tokio::sync::broadcast`)             |
//! | `BehaviorSubject<T>`  | [`RxBehaviorSubject<T>`] (wraps `tokio::sync::watch`)         |
//! | `Observable<T>`       | `BoxStream<'static, T>` (alias [`RxStream`])                  |
//! | `firstValueFrom(obs)` | [`first_value_from`]                                          |
//! | `obs.pipe(filter(p))` | `stream.filter(p)`                                            |
//! | `obs.pipe(map(f))`    | `stream.map(f)`                                               |

use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use futures::Stream;
use tokio::sync::{broadcast, watch};
use tokio_stream::wrappers::WatchStream;
use tokio_stream::StreamExt;

/// Boxed-stream alias mirroring upstream `Observable<T>`.
pub type RxStream<T> = Pin<Box<dyn Stream<Item = T> + Send>>;

/// Default shared ring size for [`RxSubject`].
pub const DEFAULT_SUBJECT_BUFFER: usize = 256;

/// Context marker emitted in storage change streams when a bounded subject
/// detects that a subscriber lagged behind the shared ring. Consumers that can
/// replay from checkpoints should treat this as "drop incremental assumptions
/// and resync".
pub const RX_SUBJECT_LAGGED_CONTEXT: &str = "ctox_rxsubject_lagged_resync";

static RX_SUBJECT_LAGGED_ITEMS_TOTAL: AtomicU64 = AtomicU64::new(0);

type LagSignal<T> = Arc<dyn Fn(u64) -> Option<T> + Send + Sync>;

/// Multi-consumer subject (RxJS `Subject<T>` analogue).
///
/// Each `subscribe()` returns a stream backed by a shared bounded broadcast
/// ring. Items emitted via `next` after the subscription are observed; items
/// emitted before are not (matches `Subject`, not `ReplaySubject`).
///
/// The subject used to fan out through per-subscriber unbounded queues. That
/// preserved every item for stalled consumers but let a background daemon grow
/// memory and CPU work without bound. The bounded ring records lag explicitly;
/// callers that can recover from checkpoints can install a lag signal via
/// [`RxSubject::with_lag_signal`].
pub struct RxSubject<T: Clone + Send + 'static> {
    sender: broadcast::Sender<T>,
    lag_signal: Option<LagSignal<T>>,
    lagged_items: Arc<AtomicU64>,
}

impl<T: Clone + Send + 'static> RxSubject<T> {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_SUBJECT_BUFFER)
    }

    pub fn with_capacity(buffer: usize) -> Self {
        let (sender, _) = broadcast::channel(buffer.max(1));
        Self {
            sender,
            lag_signal: None,
            lagged_items: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Create a bounded subject that emits a recovery value when a subscriber
    /// lags behind the ring. Storage change streams use this to turn missed
    /// incremental events into checkpoint-based resync work.
    pub fn with_lag_signal<F>(buffer: usize, lag_signal: F) -> Self
    where
        F: Fn(u64) -> Option<T> + Send + Sync + 'static,
    {
        let (sender, _) = broadcast::channel(buffer.max(1));
        Self {
            sender,
            lag_signal: Some(Arc::new(lag_signal)),
            lagged_items: Arc::new(AtomicU64::new(0)),
        }
    }

    /// RxJS `subject.next(value)`. Fans the value out to every active subscriber
    /// and prunes any whose receiver has been dropped. With no subscribers the
    /// value is dropped (RxJS cold-observable semantics).
    pub fn next(&self, value: T) {
        let _ = self.sender.send(value);
    }

    /// RxJS `subject.subscribe()`. Returns an owned stream over the bounded ring.
    pub fn subscribe(&self) -> RxStream<T> {
        let receiver = self.sender.subscribe();
        let lag_signal = self.lag_signal.clone();
        let lagged_items = Arc::clone(&self.lagged_items);
        Box::pin(futures::stream::unfold(
            (receiver, lag_signal, lagged_items),
            |(mut receiver, lag_signal, lagged_items)| async move {
                loop {
                    match receiver.recv().await {
                        Ok(value) => return Some((value, (receiver, lag_signal, lagged_items))),
                        Err(broadcast::error::RecvError::Closed) => return None,
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            lagged_items.fetch_add(skipped, Ordering::Relaxed);
                            RX_SUBJECT_LAGGED_ITEMS_TOTAL.fetch_add(skipped, Ordering::Relaxed);
                            if let Some(signal) =
                                lag_signal.as_ref().and_then(|factory| factory(skipped))
                            {
                                return Some((signal, (receiver, lag_signal, lagged_items)));
                            }
                        }
                    }
                }
            },
        ))
    }

    /// Number of currently active subscribers.
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }

    /// Total number of ring entries skipped by lagging subscribers.
    pub fn lagged_items(&self) -> u64 {
        self.lagged_items.load(Ordering::Relaxed)
    }
}

/// Total number of ring entries skipped by lagging [`RxSubject`] subscribers
/// across the current process.
pub fn rx_subject_lagged_items_total() -> u64 {
    RX_SUBJECT_LAGGED_ITEMS_TOTAL.load(Ordering::Relaxed)
}

impl<T: Clone + Send + 'static> Default for RxSubject<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone + Send + 'static> Clone for RxSubject<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            lag_signal: self.lag_signal.clone(),
            lagged_items: Arc::clone(&self.lagged_items),
        }
    }
}

/// State subject (RxJS `BehaviorSubject<T>` analogue). New subscribers
/// receive the current value immediately, then subsequent updates.
pub struct RxBehaviorSubject<T: Clone + Send + Sync + 'static> {
    sender: watch::Sender<T>,
}

impl<T: Clone + Send + Sync + 'static> RxBehaviorSubject<T> {
    pub fn new(initial: T) -> Self {
        let (sender, _) = watch::channel(initial);
        Self { sender }
    }

    /// RxJS `behavior.next(value)`.
    pub fn next(&self, value: T) {
        self.sender.send_replace(value);
    }

    /// RxJS `behavior.getValue()`.
    pub fn get_value(&self) -> T {
        self.sender.borrow().clone()
    }

    /// RxJS `behavior.subscribe()`. The returned stream yields the current
    /// value immediately, then subsequent updates.
    pub fn subscribe(&self) -> RxStream<T> {
        Box::pin(WatchStream::new(self.sender.subscribe()))
    }
}

impl<T: Clone + Send + Sync + 'static> Clone for RxBehaviorSubject<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

/// Rust-native equivalent of RxDB's `reactivity.fromObservable()`.
///
/// It exposes a BehaviorSubject-like handle with an initial value and forwards
/// all later values from the provided stream while a tokio runtime is active.
pub fn reactive_from_stream<T>(initial: T, mut stream: RxStream<T>) -> RxBehaviorSubject<T>
where
    T: Clone + Send + Sync + 'static,
{
    let subject = RxBehaviorSubject::new(initial);
    if tokio::runtime::Handle::try_current().is_ok() {
        let writer = subject.clone();
        tokio::spawn(async move {
            while let Some(value) = stream.next().await {
                writer.next(value);
            }
        });
    }
    subject
}

/// RxJS `firstValueFrom(obs)`.
pub async fn first_value_from<T, S>(mut stream: S) -> Option<T>
where
    S: Stream<Item = T> + Unpin,
{
    stream.next().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;

    #[tokio::test]
    async fn subject_broadcasts_to_subscribers() {
        let s = RxSubject::<i32>::new();
        let mut sub = s.subscribe();
        // Allow the subscription to register before we emit.
        tokio::task::yield_now().await;
        s.next(1);
        s.next(2);
        let a = sub.next().await;
        let b = sub.next().await;
        assert_eq!(a, Some(1));
        assert_eq!(b, Some(2));
    }

    #[tokio::test]
    async fn subject_bounds_backlog_and_reports_lag() {
        let s = RxSubject::<usize>::with_capacity(4);
        let mut sub = s.subscribe();
        let global_before = rx_subject_lagged_items_total();
        const N: usize = 16;
        for i in 0..N {
            s.next(i);
        }
        let mut received = Vec::new();
        while let Ok(Some(value)) =
            tokio::time::timeout(std::time::Duration::from_millis(10), sub.next()).await
        {
            received.push(value);
        }
        assert!(
            received.len() <= 4,
            "bounded ring leaked backlog: {received:?}"
        );
        assert_eq!(received.last(), Some(&(N - 1)));
        let lagged_items = s.lagged_items();
        assert!(lagged_items > 0);
        assert!(rx_subject_lagged_items_total() >= global_before + lagged_items);
        assert!(received.windows(2).all(|w| w[1] == w[0] + 1));
    }

    #[tokio::test]
    async fn subject_emits_lag_signal_for_recoverable_streams() {
        let s = RxSubject::with_lag_signal(2, |skipped| Some(format!("RESYNC:{skipped}")));
        let mut sub = s.subscribe();
        for i in 0..8 {
            s.next(i.to_string());
        }
        let first = sub.next().await;
        assert_eq!(first, Some("RESYNC:6".to_string()));
        assert_eq!(sub.next().await, Some("6".to_string()));
        assert_eq!(sub.next().await, Some("7".to_string()));
    }

    #[tokio::test]
    async fn subject_fans_out_to_multiple_subscribers_within_capacity() {
        let s = RxSubject::<usize>::with_capacity(1000);
        let mut a = s.subscribe();
        let mut b = s.subscribe();
        for i in 0..1000 {
            s.next(i);
        }
        for i in 0..1000 {
            assert_eq!(a.next().await, Some(i));
            assert_eq!(b.next().await, Some(i));
        }
    }

    #[tokio::test]
    async fn behavior_subject_replays_current() {
        let b = RxBehaviorSubject::new(7);
        let mut sub = b.subscribe();
        let v = sub.next().await;
        assert_eq!(v, Some(7));
        b.next(8);
        let v = sub.next().await;
        assert_eq!(v, Some(8));
        assert_eq!(b.get_value(), 8);
    }

    #[test]
    fn behavior_subject_updates_value_without_subscribers() {
        let b = RxBehaviorSubject::new(false);
        b.next(true);
        assert!(b.get_value());
    }

    #[tokio::test]
    async fn reactive_from_stream_forwards_stream_values() {
        let s = RxSubject::<i32>::new();
        let reactive = reactive_from_stream(0, s.subscribe());
        let mut sub = reactive.subscribe();
        assert_eq!(sub.next().await, Some(0));
        tokio::task::yield_now().await;
        s.next(1);
        assert_eq!(sub.next().await, Some(1));
        assert_eq!(reactive.get_value(), 1);
    }

    #[tokio::test]
    async fn first_value_from_returns_first() {
        let s = RxSubject::<&'static str>::new();
        let stream = s.subscribe();
        tokio::task::yield_now().await;
        s.next("hello");
        s.next("world");
        let v = first_value_from(stream).await;
        assert_eq!(v, Some("hello"));
    }
}
