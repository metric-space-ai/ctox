//! Pump-owned deadlines. Wakes coalesce without postponing an existing deadline.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

// Keep the three-second spacing introduced by main's incident mitigation.
pub(super) const MIN_REFRESH_INTERVAL: Duration = Duration::from_secs(3);

#[derive(Default)]
pub(super) struct Schedule {
    pending: BTreeMap<PathBuf, u8>,
    next_allowed: BTreeMap<PathBuf, Instant>,
}

impl Schedule {
    pub(super) fn mark(&mut self, root: PathBuf, flags: u8) {
        *self.pending.entry(root).or_default() |= flags;
    }

    pub(super) fn wait(&self, now: Instant, sweep: Instant) -> Duration {
        self.pending
            .keys()
            .map(|root| self.next_allowed.get(root).copied().unwrap_or(now))
            .min()
            .unwrap_or(sweep)
            .min(sweep)
            .saturating_duration_since(now)
    }

    pub(super) fn take_ready(&mut self, now: Instant) -> Vec<(PathBuf, u8)> {
        let ready = self
            .pending
            .keys()
            .filter(|root| {
                self.next_allowed
                    .get(*root)
                    .is_none_or(|deadline| *deadline <= now)
            })
            .cloned()
            .collect::<Vec<_>>();
        ready
            .into_iter()
            .map(|root| {
                let flags = self.pending.remove(&root).expect("pending root");
                (root, flags)
            })
            .collect()
    }

    /// Cooldown follows completion, including failure: slow SQL must not turn
    /// a constant stream of wakes into back-to-back expensive passes.
    pub(super) fn completed(&mut self, root: PathBuf, now: Instant) {
        self.next_allowed.insert(root, now + MIN_REFRESH_INTERVAL);
    }

    pub(super) fn forget(&mut self, root: &Path) {
        self.pending.remove(root);
        self.next_allowed.remove(root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flood_coalesces_flags_without_starving_the_deadline() {
        let now = Instant::now();
        let root = PathBuf::from("one");
        let mut schedule = Schedule::default();
        schedule.mark(root.clone(), 1);
        assert_eq!(schedule.take_ready(now), vec![(root.clone(), 1)]);
        schedule.completed(root.clone(), now);
        for ms in 0..MIN_REFRESH_INTERVAL.as_millis() as u64 {
            schedule.mark(root.clone(), if ms % 2 == 0 { 2 } else { 4 });
            assert!(schedule
                .take_ready(now + Duration::from_millis(ms))
                .is_empty());
        }
        assert_eq!(
            schedule.wait(now, now + Duration::from_secs(60)),
            MIN_REFRESH_INTERVAL
        );
        assert_eq!(
            schedule.take_ready(now + MIN_REFRESH_INTERVAL),
            vec![(root, 6)]
        );
        assert!(schedule.take_ready(now + MIN_REFRESH_INTERVAL).is_empty());
    }

    #[test]
    fn slow_or_failed_pass_gets_a_full_cooldown_and_other_roots_stay_ready() {
        let now = Instant::now();
        let mut schedule = Schedule::default();
        schedule.completed(PathBuf::from("slow"), now + Duration::from_secs(5));
        schedule.mark(PathBuf::from("slow"), 31);
        schedule.mark(PathBuf::from("other"), 2);
        assert_eq!(
            schedule.take_ready(now + Duration::from_secs(5)),
            vec![(PathBuf::from("other"), 2)]
        );
        assert!(schedule.take_ready(now + Duration::from_secs(7)).is_empty());
        assert_eq!(
            schedule.take_ready(now + Duration::from_secs(8)),
            vec![(PathBuf::from("slow"), 31)]
        );
    }

    #[test]
    fn maintenance_flags_survive_a_cooldown_and_removed_roots_leave_no_deadline() {
        let now = Instant::now();
        let root = PathBuf::from("one");
        let mut schedule = Schedule::default();
        schedule.completed(root.clone(), now);
        schedule.mark(root.clone(), 1);
        schedule.mark(root.clone(), 63);
        assert_eq!(
            schedule.take_ready(now + MIN_REFRESH_INTERVAL),
            vec![(root.clone(), 63)]
        );
        schedule.completed(root.clone(), now + MIN_REFRESH_INTERVAL);
        schedule.mark(root.clone(), 2);
        schedule.forget(&root);
        assert!(schedule
            .take_ready(now + Duration::from_secs(10))
            .is_empty());
        schedule.mark(root.clone(), 1);
        assert_eq!(schedule.take_ready(now), vec![(root, 1)]);
    }
}
