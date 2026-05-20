//! Port of the `oblivious-set` NPM package (gap-item N8b).
//!
//! Upstream: a hash-set that drops entries older than a configurable TTL.
//! RxDB uses it to dedupe emitted event-bulk ids inside the 60-second window
//! a peer could see the same bulk twice (`rx-database.ts:249`).
//!
//! Source: https://github.com/pubkey/oblivious-set
//! Single-file dep — `add(value)`, `has(value)`, `clear()`, internal
//! `removeTooOldValues()` swept on each mutation by walking the insertion-order
//! list.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

// ref: oblivious-set/src/index.ts: ObliviousSet<T>
/// Bounded, time-decaying set. Entries fall out after `ttl`. Cheap O(1)
/// add/has; sweep is amortized over inserts.
pub struct ObliviousSet<T: Eq + Hash + Clone> {
    ttl: Duration,
    /// Insertion-ordered storage: each entry remembers when it was added.
    map: HashMap<T, Instant>,
    /// Mirrors the JS package's `Set.keys()` insertion order so the sweep can
    /// stop at the first non-stale entry.
    insertion_order: std::collections::VecDeque<T>,
}

impl<T: Eq + Hash + Clone> ObliviousSet<T> {
    pub fn new(ttl_ms: u64) -> Self {
        Self {
            ttl: Duration::from_millis(ttl_ms),
            map: HashMap::new(),
            insertion_order: std::collections::VecDeque::new(),
        }
    }

    /// O(1). Insert `value`; refresh its timestamp if already present.
    pub fn add(&mut self, value: T) {
        self.remove_too_old_values();
        if self.map.insert(value.clone(), Instant::now()).is_none() {
            self.insertion_order.push_back(value);
        }
    }

    pub fn has(&self, value: &T) -> bool {
        match self.map.get(value) {
            Some(ts) => ts.elapsed() < self.ttl,
            None => false,
        }
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.insertion_order.clear();
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    // ref: oblivious-set/src/index.ts: removeTooOldValues
    fn remove_too_old_values(&mut self) {
        while let Some(front) = self.insertion_order.front() {
            match self.map.get(front) {
                Some(ts) if ts.elapsed() >= self.ttl => {
                    let key = front.clone();
                    self.insertion_order.pop_front();
                    self.map.remove(&key);
                }
                _ => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn add_and_has() {
        let mut s = ObliviousSet::new(60_000);
        s.add("a".to_string());
        assert!(s.has(&"a".to_string()));
        assert!(!s.has(&"b".to_string()));
    }

    #[test]
    fn drops_after_ttl() {
        let mut s = ObliviousSet::new(20);
        s.add("a".to_string());
        sleep(Duration::from_millis(40));
        s.add("b".to_string());
        assert!(!s.has(&"a".to_string()), "a should have expired");
        assert!(s.has(&"b".to_string()));
    }

    #[test]
    fn clear_empties() {
        let mut s = ObliviousSet::new(60_000);
        s.add(1u64);
        s.add(2u64);
        assert_eq!(s.len(), 2);
        s.clear();
        assert!(s.is_empty());
    }
}
