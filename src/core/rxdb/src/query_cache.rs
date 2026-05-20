//! Port of `src/query-cache.ts`.
//!
//! Upstream's query cache is keyed by `rxQuery.toString()` and stores the
//! canonical [`RxQuery`] for a given string form. The cache-replacement
//! policy drops queries that:
//! 1. have no live subscribers,
//! 2. either never executed (and are older than `unexecuted_lifetime`), or
//!    are simply the least-recently-used among the cached set when the
//!    cache exceeds `try_to_keep_max`.
//!
//! T1 deviation: `RxCollection` keeps the typed `Arc<RxQueryBase>` cache so
//! callers do not have to downcast trait objects. This module still owns the
//! upstream replacement constants and standalone policy helpers used by tests
//! and by the collection-level scheduler.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::plugins::utils::utils_time::now;

// ref: rxdb/src/query-cache.ts:54-55
pub const DEFAULT_TRY_TO_KEEP_MAX: usize = 100;
pub const DEFAULT_UNEXECUTED_LIFETIME_MS: f64 = 30.0 * 1000.0;

/// Trait an `RxQuery`-like value implements to be cacheable. Mirrors the
/// accessors `defaultCacheReplacementPolicyMonad` reads on `rxQuery`.
pub trait QueryCacheable: Send + Sync {
    /// Number of live subscribers (`refCount$.observers.length` upstream).
    fn subscriber_count(&self) -> usize;
    /// Last time `_ensureEqual` succeeded (`_lastEnsureEqual` upstream).
    /// `0.0` ⇒ never executed.
    fn last_ensure_equal(&self) -> f64;
    /// Creation epoch (`_creationTime`).
    fn creation_time(&self) -> f64;
    /// Flag the query as uncached. Upstream sets `rxQuery.uncached = true`
    /// after removal so observers can short-circuit.
    fn mark_uncached(&self);
}

// ref: rxdb/src/query-cache.ts:17-34
/// Cache of canonical query instances keyed by their `to_string()` form.
pub struct QueryCache {
    inner: Mutex<HashMap<String, Arc<dyn QueryCacheable>>>,
}

impl Default for QueryCache {
    fn default() -> Self {
        Self::new()
    }
}

impl QueryCache {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Lookup or insert. If an entry with the same `string_rep` already
    /// exists, returns it; otherwise stores and returns `rx_query`.
    pub fn get_by_query(
        &self,
        string_rep: &str,
        rx_query: Arc<dyn QueryCacheable>,
    ) -> Arc<dyn QueryCacheable> {
        let mut m = self.inner.lock();
        if let Some(existing) = m.get(string_rep) {
            return Arc::clone(existing);
        }
        m.insert(string_rep.to_string(), Arc::clone(&rx_query));
        rx_query
    }

    pub fn size(&self) -> usize {
        self.inner.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().is_empty()
    }

    /// All cached queries (snapshot).
    pub fn values(&self) -> Vec<Arc<dyn QueryCacheable>> {
        self.inner.lock().values().cloned().collect()
    }
}

// ref: rxdb/src/query-cache.ts:36-38
pub fn create_query_cache() -> QueryCache {
    QueryCache::new()
}

// ref: rxdb/src/query-cache.ts:41-46
pub fn uncache_rx_query(cache: &QueryCache, string_rep: &str, rx_query: &Arc<dyn QueryCacheable>) {
    rx_query.mark_uncached();
    cache.inner.lock().remove(string_rep);
}

// ref: rxdb/src/query-cache.ts:49-51
pub fn count_rx_query_subscribers(rx_query: &Arc<dyn QueryCacheable>) -> usize {
    rx_query.subscriber_count()
}

// ref: rxdb/src/query-cache.ts:63-102
/// Default cache-replacement policy. Returns a closure (a "monad" in the
/// upstream's naming) that decides which entries to evict when invoked.
///
/// `try_to_keep_max` — target cache size; `unexecuted_lifetime_ms` — drop
/// never-executed queries older than this.
///
/// The closure takes a list of `(string_rep, query)` pairs because upstream
/// keys by `rxQuery.toString()` and we need to know the key for eviction.
pub fn default_cache_replacement_policy_monad(
    try_to_keep_max: usize,
    unexecuted_lifetime_ms: f64,
) -> impl Fn(&QueryCache) {
    move |cache: &QueryCache| {
        let entries: Vec<(String, Arc<dyn QueryCacheable>)> = {
            let m = cache.inner.lock();
            if m.len() < try_to_keep_max {
                return;
            }
            m.iter().map(|(k, v)| (k.clone(), Arc::clone(v))).collect()
        };
        let min_unexecuted = now() - unexecuted_lifetime_ms;
        let mut maybe_uncache: Vec<(String, Arc<dyn QueryCacheable>)> = Vec::new();
        for (key, q) in entries.into_iter() {
            if count_rx_query_subscribers(&q) > 0 {
                continue;
            }
            if q.last_ensure_equal() == 0.0 && q.creation_time() < min_unexecuted {
                uncache_rx_query(cache, &key, &q);
                continue;
            }
            maybe_uncache.push((key, q));
        }

        if maybe_uncache.len() <= try_to_keep_max {
            return;
        }
        let must_uncache = maybe_uncache.len() - try_to_keep_max;
        // Sort ascending by `_lastEnsureEqual` so the oldest get evicted first.
        maybe_uncache.sort_by(|a, b| {
            a.1.last_ensure_equal()
                .partial_cmp(&b.1.last_ensure_equal())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for (key, q) in maybe_uncache.into_iter().take(must_uncache) {
            uncache_rx_query(cache, &key, &q);
        }
    }
}

// ref: rxdb/src/query-cache.ts:105-108
/// The default policy uses [`DEFAULT_TRY_TO_KEEP_MAX`] +
/// [`DEFAULT_UNEXECUTED_LIFETIME_MS`].
pub fn default_cache_replacement_policy(cache: &QueryCache) {
    let policy = default_cache_replacement_policy_monad(
        DEFAULT_TRY_TO_KEEP_MAX,
        DEFAULT_UNEXECUTED_LIFETIME_MS,
    );
    policy(cache);
}

// ref: rxdb/src/query-cache.ts:110-139
// The `RxCollection`-bound typed scheduler lives in `rx_collection.rs`, where
// it can evict concrete `Arc<RxQueryBase>` values without trait-object casts.

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestQuery {
        subs: AtomicUsize,
        last_eq: parking_lot::Mutex<f64>,
        created: f64,
        uncached: AtomicUsize,
    }

    impl QueryCacheable for TestQuery {
        fn subscriber_count(&self) -> usize {
            self.subs.load(Ordering::SeqCst)
        }
        fn last_ensure_equal(&self) -> f64 {
            *self.last_eq.lock()
        }
        fn creation_time(&self) -> f64 {
            self.created
        }
        fn mark_uncached(&self) {
            self.uncached.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn q(subs: usize, last_eq: f64, created: f64) -> Arc<TestQuery> {
        Arc::new(TestQuery {
            subs: AtomicUsize::new(subs),
            last_eq: parking_lot::Mutex::new(last_eq),
            created,
            uncached: AtomicUsize::new(0),
        })
    }

    #[test]
    fn get_by_query_returns_cached_on_hit() {
        let cache = QueryCache::new();
        let a = q(0, 0.0, 0.0);
        let b = q(0, 0.0, 0.0);
        let r1 = cache.get_by_query("k", a.clone() as Arc<dyn QueryCacheable>);
        let r2 = cache.get_by_query("k", b as Arc<dyn QueryCacheable>);
        assert!(Arc::ptr_eq(&r1, &r2));
    }

    #[test]
    fn replacement_policy_is_a_noop_under_threshold() {
        let cache = QueryCache::new();
        let a = q(0, 0.0, 0.0);
        cache.get_by_query("k1", a as Arc<dyn QueryCacheable>);
        let policy = default_cache_replacement_policy_monad(100, 30_000.0);
        policy(&cache);
        assert_eq!(cache.size(), 1);
    }

    #[test]
    fn replacement_policy_drops_unexecuted_and_old_queries() {
        let cache = QueryCache::new();
        let stale = q(0, 0.0, 0.0);
        cache.get_by_query("stale", stale.clone() as Arc<dyn QueryCacheable>);
        let fresh = q(0, 0.0, now());
        cache.get_by_query("fresh", fresh as Arc<dyn QueryCacheable>);
        let policy = default_cache_replacement_policy_monad(0, 30_000.0);
        policy(&cache);
        assert_eq!(
            stale.uncached.load(Ordering::SeqCst),
            1,
            "stale must be uncached"
        );
        assert!(cache.size() <= 1);
    }

    #[test]
    fn replacement_policy_keeps_subscribed_queries() {
        let cache = QueryCache::new();
        let subscribed = q(1, 0.0, 0.0);
        cache.get_by_query("sub", subscribed.clone() as Arc<dyn QueryCacheable>);
        let policy = default_cache_replacement_policy_monad(0, 30_000.0);
        policy(&cache);
        assert_eq!(
            subscribed.uncached.load(Ordering::SeqCst),
            0,
            "queries with live subscribers must not be evicted"
        );
        assert_eq!(cache.size(), 1);
    }
}
