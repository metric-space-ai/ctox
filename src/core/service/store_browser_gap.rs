//! One P8 deadline per peer. Replacing a sleep must not cancel a committed result.
use ctox_store::change_log::subscription::write_contract::{OrderedWriter, WriteBatch};
use ctox_store::StoreError;
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
pub(crate) type Delivery =
    Arc<dyn Fn(WriteBatch) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;
#[derive(Default)]
pub(crate) struct GapTimer(Mutex<Option<(u64, tokio::task::JoinHandle<()>)>>);
impl GapTimer {
    pub(crate) fn arm(
        self: &Arc<Self>,
        writer: OrderedWriter,
        participant: String,
        expires: Instant,
        deliver: Delivery,
        deadline: Option<u64>,
        clock: fn() -> u64,
    ) -> Result<(), StoreError> {
        let mut slot = self.0.lock().map_err(|_| StoreError::Closed)?;
        if slot.as_ref().map(|(at, _)| *at) == deadline {
            return Ok(());
        }
        if let Some((_, task)) = slot.take() {
            task.abort();
        }
        if let Some(at) = deadline {
            let weak = Arc::downgrade(self);
            let task = tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(at.saturating_sub(clock()))).await;
                let Some(owner) = weak.upgrade() else { return };
                if expires <= Instant::now() {
                    return;
                }
                // Only the sleep can be replaced. Once expiry is admitted,
                // its committed batch must finish delivery under backpressure.
                {
                    let Ok(mut slot) = owner.0.lock() else { return };
                    if slot.as_ref().map(|(deadline, _)| *deadline) != Some(at) {
                        return;
                    }
                    *slot = None;
                }
                if let Ok(batch) = writer.expire(participant, clock()).await {
                    deliver(batch).await;
                }
            });
            *slot = Some((at, task));
        }
        Ok(())
    }
}
impl Drop for GapTimer {
    fn drop(&mut self) {
        if let Ok(slot) = self.0.get_mut() {
            if let Some((_, task)) = slot.take() {
                task.abort();
            }
        }
    }
}
#[cfg(test)]
#[path = "store_browser_gap_tests.rs"]
mod tests;
