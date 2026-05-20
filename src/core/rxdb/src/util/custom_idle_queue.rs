//! Port of the `custom-idle-queue` NPM package (gap-item N8c).
//!
//! Upstream: a tiny FIFO queue that runs async fns one-at-a-time and exposes
//! `requestIdlePromise()` so other code can wait for the queue to drain. RxDB
//! uses it on `RxDatabase` to gate close-time cleanup until in-flight ops are
//! quiet.
//!
//! Source: https://github.com/pubkey/custom-idle-queue
//! Single-purpose util — we mirror the call surface, not the line numbers.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::Notify;

// ref: custom-idle-queue/src/index.ts: class IdleQueue
/// Serializing async queue. `wrap_call(f)` runs `f` after every previously
/// queued task finishes. `request_idle_promise()` resolves once the queue is
/// empty.
pub struct IdleQueue {
    inner: Arc<Inner>,
}

struct Inner {
    pending: Mutex<VecDeque<Pin<Box<dyn Future<Output = ()> + Send>>>>,
    /// Notified whenever the queue transitions to empty.
    idle: Notify,
    /// `true` while a task is executing or queued.
    busy: Mutex<bool>,
}

impl Default for IdleQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl IdleQueue {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                pending: Mutex::new(VecDeque::new()),
                idle: Notify::new(),
                busy: Mutex::new(false),
            }),
        }
    }

    /// Returns a future that resolves once the queue is empty. If it's already
    /// idle this resolves immediately.
    pub async fn request_idle_promise(&self) {
        let need_wait = {
            let busy = self.inner.busy.lock();
            *busy
        };
        if !need_wait {
            return;
        }
        loop {
            let notified = self.inner.idle.notified();
            let still_busy = *self.inner.busy.lock();
            if !still_busy {
                return;
            }
            notified.await;
        }
    }

    /// Schedule `task` to run after all currently-queued tasks finish.
    /// Resolves when `task` itself finishes.
    pub async fn wrap_call<F>(&self, task: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        // Channel for completion of this specific task.
        let (tx, rx) = tokio::sync::oneshot::channel();
        let fut: Pin<Box<dyn Future<Output = ()> + Send>> = Box::pin(async move {
            task.await;
            let _ = tx.send(());
        });
        let start_now = {
            let mut pending = self.inner.pending.lock();
            let mut busy = self.inner.busy.lock();
            pending.push_back(fut);
            let was_idle = !*busy;
            *busy = true;
            was_idle
        };
        if start_now {
            let inner = Arc::clone(&self.inner);
            tokio::spawn(async move {
                loop {
                    let next = inner.pending.lock().pop_front();
                    match next {
                        Some(f) => f.await,
                        None => {
                            *inner.busy.lock() = false;
                            inner.idle.notify_waiters();
                            break;
                        }
                    }
                }
            });
        }
        let _ = rx.await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[tokio::test]
    async fn serializes_calls() {
        let q = IdleQueue::new();
        let counter = Arc::new(AtomicU64::new(0));
        let mut handles = vec![];
        for i in 0..5 {
            let counter = Arc::clone(&counter);
            let q_inner = Arc::clone(&q.inner);
            handles.push(tokio::spawn(async move {
                let helper = IdleQueue { inner: q_inner };
                helper
                    .wrap_call(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                        counter.fetch_add(1, Ordering::SeqCst);
                        let _ = i;
                    })
                    .await;
            }));
        }
        for h in handles {
            let _ = h.await;
        }
        assert_eq!(counter.load(Ordering::SeqCst), 5);
    }

    #[tokio::test]
    async fn idle_promise_resolves_when_empty() {
        let q = IdleQueue::new();
        q.request_idle_promise().await; // Empty from the start.
    }
}
