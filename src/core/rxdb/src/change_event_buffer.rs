//! Port of `src/change-event-buffer.ts`.
//!
//! T1 deviations:
//! - The buffer takes a `RxStream<EventBulk>` directly instead of an
//!   `RxCollection` handle (RxCollection lives in phase-6). The `is_local`
//!   filter is the caller's responsibility — pass a pre-filtered stream.
//! - Upstream's `WeakMap<event, counter>` is replaced by `oldest_counter` +
//!   index arithmetic: the counter for `buffer[i]` is `oldest_counter + i`.
//! - Upstream's `requestIdlePromiseNoQueue` lazy-task pattern (collect into a
//!   `Set<Function>` and drain on idle) is collapsed to a synchronous Mutex
//!   update on each event-bulk arrival. The Rust call site already runs in an
//!   async context; the IDLE deferral is a browser-only optimisation.

use std::sync::Arc;

use parking_lot::Mutex;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;

use crate::rxjs_compat::RxStream;
use crate::types::{EventBulk, RxStorageChangeEvent};

// ref: rxdb/src/change-event-buffer.ts:24-165
pub struct ChangeEventBuffer {
    pub limit: usize,
    inner: Arc<Mutex<Inner>>,
    sub_handle: Mutex<Option<JoinHandle<()>>>,
}

struct Inner {
    counter: u64,
    /// Counter assigned to the event currently at `buffer[0]`. If the buffer
    /// is empty, the next event will get `counter + 1` (matching upstream's
    /// `counterBefore + 1` base for the first event).
    oldest_counter: u64,
    buffer: Vec<RxStorageChangeEvent>,
}

impl ChangeEventBuffer {
    // ref: rxdb/src/change-event-buffer.ts:47-62
    pub fn new(events_stream: RxStream<EventBulk>) -> Arc<Self> {
        let inner = Arc::new(Mutex::new(Inner {
            counter: 0,
            oldest_counter: 1,
            buffer: Vec::new(),
        }));
        let buf = Arc::new(Self {
            limit: 100,
            inner: Arc::clone(&inner),
            sub_handle: Mutex::new(None),
        });
        let inner_for_task = Arc::clone(&inner);
        let limit = buf.limit;
        let handle = tokio::spawn(async move {
            let mut s = events_stream;
            while let Some(bulk) = s.next().await {
                Self::handle_change_events(&inner_for_task, &bulk.events, limit);
            }
        });
        *buf.sub_handle.lock() = Some(handle);
        buf
    }

    // ref: rxdb/src/change-event-buffer.ts:73-88
    fn handle_change_events(
        inner: &Arc<Mutex<Inner>>,
        events: &[RxStorageChangeEvent],
        limit: usize,
    ) {
        let mut state = inner.lock();
        let counter_before = state.counter;
        state.counter = counter_before + events.len() as u64;

        if events.len() > limit {
            // Slice from the end: keep last `events.len()` items (which equals limit since events.len()>limit).
            // Upstream `events.slice(events.length * -1)` is identical to `events.slice(0)`.
            // The semantic upstream intends: keep the last `limit` items, replacing the buffer.
            let start = events.len() - limit;
            state.buffer = events[start..].to_vec();
            // oldest_counter = counter_before + start + 1
            state.oldest_counter = counter_before + start as u64 + 1;
        } else {
            state.buffer.extend(events.iter().cloned());
            if state.buffer.len() > limit {
                let drop_count = state.buffer.len() - limit;
                state.buffer.drain(..drop_count);
                state.oldest_counter += drop_count as u64;
            }
            // If buffer was empty before this push, oldest_counter is set to
            // `counter_before + 1` so that the first appended event gets that index.
            if state.oldest_counter == 0 {
                state.oldest_counter = 1;
            }
        }
    }

    // ref: rxdb/src/change-event-buffer.ts:90-93
    pub fn get_counter(&self) -> u64 {
        self.inner.lock().counter
    }

    // ref: rxdb/src/change-event-buffer.ts:94-97
    pub fn get_buffer(&self) -> Vec<RxStorageChangeEvent> {
        self.inner.lock().buffer.clone()
    }

    // ref: rxdb/src/change-event-buffer.ts:103-115
    /// gets the array-index for the given pointer.
    /// Returns `None` if the pointer is out of the lower bound.
    pub fn get_array_index_by_pointer(&self, pointer: u64) -> Option<usize> {
        let state = self.inner.lock();
        if state.buffer.is_empty() {
            return None;
        }
        let oldest_counter = state.oldest_counter;
        if pointer < oldest_counter {
            return None;
        }
        Some((pointer - oldest_counter) as usize)
    }

    // ref: rxdb/src/change-event-buffer.ts:121-137
    /// get all changeEvents which came in later than the pointer-event.
    /// Returns `None` if pointer is out of bounds.
    pub fn get_from(&self, pointer: u64) -> Option<Vec<RxStorageChangeEvent>> {
        let start = self.get_array_index_by_pointer(pointer)?;
        let state = self.inner.lock();
        Some(state.buffer[start..].to_vec())
    }

    // ref: rxdb/src/change-event-buffer.ts:139-147
    pub fn run_from<F>(&self, pointer: u64, mut f: F) -> Result<(), &'static str>
    where
        F: FnMut(&RxStorageChangeEvent),
    {
        let events = self.get_from(pointer).ok_or("out of bounds")?;
        for ev in events.iter() {
            f(ev);
        }
        Ok(())
    }

    // ref: rxdb/src/change-event-buffer.ts:156-159
    /// no matter how many operations are done on one document, only the last
    /// operation has to be checked to calculate the new state. Disabled in
    /// upstream (returns a slice of the input unchanged); preserved verbatim.
    pub fn reduce_by_last_of_doc(
        &self,
        change_events: &[RxStorageChangeEvent],
    ) -> Vec<RxStorageChangeEvent> {
        change_events.to_vec()
    }

    // ref: rxdb/src/change-event-buffer.ts:161-164
    pub fn close(&self) {
        if let Some(h) = self.sub_handle.lock().take() {
            h.abort();
        }
    }
}

// ref: rxdb/src/change-event-buffer.ts:167-171
pub fn create_change_event_buffer(events_stream: RxStream<EventBulk>) -> Arc<ChangeEventBuffer> {
    ChangeEventBuffer::new(events_stream)
}
