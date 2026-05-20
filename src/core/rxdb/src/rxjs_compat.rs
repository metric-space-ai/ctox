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

use futures::Stream;
use tokio::sync::{broadcast, watch};
use tokio_stream::wrappers::{BroadcastStream, WatchStream};
use tokio_stream::StreamExt;

/// Boxed-stream alias mirroring upstream `Observable<T>`.
pub type RxStream<T> = Pin<Box<dyn Stream<Item = T> + Send>>;

/// Default channel buffer for [`RxSubject`]. The exact RxJS semantics are
/// "unbounded buffering until consumed"; we pick a generous default and let
/// callers override if hot paths overflow it.
pub const DEFAULT_SUBJECT_BUFFER: usize = 256;

/// Multi-consumer broadcast subject (RxJS `Subject<T>` analogue).
///
/// Each `subscribe()` returns an independent stream. Items emitted via `next`
/// after the subscription are observed; items emitted before are not (matches
/// `Subject` behaviour, not `ReplaySubject`).
pub struct RxSubject<T: Clone + Send + 'static> {
    sender: broadcast::Sender<T>,
}

impl<T: Clone + Send + 'static> RxSubject<T> {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_SUBJECT_BUFFER)
    }

    pub fn with_capacity(buffer: usize) -> Self {
        let (sender, _) = broadcast::channel(buffer);
        Self { sender }
    }

    /// RxJS `subject.next(value)`. Drops the value silently if there are no
    /// active subscribers (matches RxJS observable-cold semantics).
    pub fn next(&self, value: T) {
        let _ = self.sender.send(value);
    }

    /// RxJS `subject.subscribe()`. Returns an owned stream.
    pub fn subscribe(&self) -> RxStream<T> {
        let rx = self.sender.subscribe();
        Box::pin(BroadcastStream::new(rx).filter_map(|r: Result<T, _>| r.ok()))
    }

    /// Number of currently active subscribers.
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
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
            sender: self.sender.clone(),
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
