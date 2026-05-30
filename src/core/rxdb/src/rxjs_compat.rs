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
use std::sync::Arc;

use futures::Stream;
use parking_lot::Mutex;
use tokio::sync::{mpsc, watch};
use tokio_stream::wrappers::{UnboundedReceiverStream, WatchStream};
use tokio_stream::StreamExt;

/// Boxed-stream alias mirroring upstream `Observable<T>`.
pub type RxStream<T> = Pin<Box<dyn Stream<Item = T> + Send>>;

/// Retained for API compatibility with callers that pass a capacity hint to
/// [`RxSubject::with_capacity`]. The subject now buffers per-subscriber and
/// unbounded, so there is no shared ring whose size this would set.
pub const DEFAULT_SUBJECT_BUFFER: usize = 256;

/// Multi-consumer subject (RxJS `Subject<T>` analogue).
///
/// Each `subscribe()` returns an independent stream backed by its OWN unbounded
/// channel. Items emitted via `next` after the subscription are observed; items
/// emitted before are not (matches `Subject`, not `ReplaySubject`).
///
/// This was previously a single `tokio::sync::broadcast` ring of capacity 256
/// whose stream silently discarded `RecvError::Lagged` — so a momentarily-slow
/// consumer (e.g. the replication change-stream or the multiplexed transport
/// response stream under an initial-sync burst) would lose master-change events
/// and method responses with no error, leaving collections under-synced. Per the
/// module's stated goal ("unbounded buffering until consumed") `RxSubject` now
/// fans out to per-subscriber unbounded `mpsc` channels: no shared ring to
/// overflow, no silent drops, and memory is driven by real backlog exactly as a
/// RxJS `Subject` would be.
pub struct RxSubject<T: Clone + Send + 'static> {
    subscribers: Arc<Mutex<Vec<mpsc::UnboundedSender<T>>>>,
}

impl<T: Clone + Send + 'static> RxSubject<T> {
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Retained for API compatibility; the capacity hint is unused because the
    /// subject is now per-subscriber unbounded (nothing to size).
    pub fn with_capacity(_buffer: usize) -> Self {
        Self::new()
    }

    /// RxJS `subject.next(value)`. Fans the value out to every active subscriber
    /// and prunes any whose receiver has been dropped. With no subscribers the
    /// value is dropped (RxJS cold-observable semantics).
    pub fn next(&self, value: T) {
        let mut subscribers = self.subscribers.lock();
        subscribers.retain(|tx| tx.send(value.clone()).is_ok());
    }

    /// RxJS `subject.subscribe()`. Returns an owned stream backed by an unbounded
    /// channel, so a momentarily-slow consumer never loses items.
    pub fn subscribe(&self) -> RxStream<T> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.subscribers.lock().push(tx);
        Box::pin(UnboundedReceiverStream::new(rx))
    }

    /// Number of currently active subscribers.
    pub fn receiver_count(&self) -> usize {
        self.subscribers.lock().len()
    }
}

impl<T: Clone + Send + 'static> Default for RxSubject<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone + Send + 'static> Clone for RxSubject<T> {
    fn clone(&self) -> Self {
        Self {
            subscribers: Arc::clone(&self.subscribers),
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
    async fn subject_does_not_drop_items_under_backlog() {
        // The previous broadcast(256) ring silently dropped everything beyond 256
        // un-drained items. A per-subscriber unbounded fan-out must lose nothing,
        // in order, even when the consumer drains long after a large burst.
        let s = RxSubject::<usize>::new();
        let mut sub = s.subscribe();
        const N: usize = 5000;
        for i in 0..N {
            s.next(i);
        }
        let mut received = Vec::with_capacity(N);
        for _ in 0..N {
            received.push(sub.next().await.expect("no item should be dropped"));
        }
        assert_eq!(received.len(), N);
        assert_eq!(received.first(), Some(&0));
        assert_eq!(received.last(), Some(&(N - 1)));
        // Strictly increasing => order preserved, nothing skipped.
        assert!(received.windows(2).all(|w| w[1] == w[0] + 1));
    }

    #[tokio::test]
    async fn subject_fans_out_to_multiple_subscribers_without_drops() {
        let s = RxSubject::<usize>::new();
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
