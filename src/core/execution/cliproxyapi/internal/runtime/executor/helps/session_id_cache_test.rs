// ref: internal/runtime/executor/helps/session_id_cache_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use uuid::Uuid;

use super::session_id_cache::*;
use super::{ClaudeIdentityKvStore, ClaudeIdentityStoreError};

#[derive(Default)]
struct FakeStore {
    inner: Mutex<FakeStoreInner>,
}

#[derive(Default)]
struct FakeStoreInner {
    values: HashMap<String, Vec<u8>>,
    get_error: bool,
    set_error: bool,
    expire_error: bool,
    set_no_persist: bool,
    get_count: usize,
    set_count: usize,
    expire_count: usize,
    last_set_ttl: Option<Duration>,
    last_expire_ttl: Option<Duration>,
}

impl ClaudeIdentityKvStore for FakeStore {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, ClaudeIdentityStoreError> {
        let mut inner = self.inner.lock().unwrap();
        inner.get_count += 1;
        if inner.get_error {
            return Err(ClaudeIdentityStoreError::Backend("get failed".to_owned()));
        }
        Ok(inner.values.get(key).cloned())
    }

    fn set(
        &self,
        key: &str,
        value: &[u8],
        ttl: Duration,
    ) -> Result<bool, ClaudeIdentityStoreError> {
        let mut inner = self.inner.lock().unwrap();
        inner.values.insert(key.to_owned(), value.to_vec());
        inner.last_set_ttl = Some(ttl);
        Ok(true)
    }

    fn set_nx(
        &self,
        key: &str,
        value: &[u8],
        ttl: Duration,
    ) -> Result<bool, ClaudeIdentityStoreError> {
        let mut inner = self.inner.lock().unwrap();
        inner.set_count += 1;
        inner.last_set_ttl = Some(ttl);
        if inner.set_error {
            return Err(ClaudeIdentityStoreError::Backend("set failed".to_owned()));
        }
        if inner.values.contains_key(key) {
            return Ok(false);
        }
        if !inner.set_no_persist {
            inner.values.insert(key.to_owned(), value.to_vec());
        }
        Ok(true)
    }

    fn expire(&self, _key: &str, ttl: Duration) -> Result<bool, ClaudeIdentityStoreError> {
        let mut inner = self.inner.lock().unwrap();
        inner.expire_count += 1;
        inner.last_expire_ttl = Some(ttl);
        if inner.expire_error {
            return Err(ClaudeIdentityStoreError::Backend(
                "expire failed".to_owned(),
            ));
        }
        Ok(true)
    }
}

struct FakeClock {
    now: Mutex<Instant>,
}

impl FakeClock {
    fn new() -> Self {
        Self {
            now: Mutex::new(Instant::now()),
        }
    }

    fn advance(&self, duration: Duration) {
        let mut now = self.now.lock().unwrap();
        *now += duration;
    }
}

impl SessionIdClock for FakeClock {
    fn now(&self) -> Instant {
        *self.now.lock().unwrap()
    }
}

#[test]
fn non_home_mode_uses_local_map() {
    let cache = SessionIdCache::new();
    let first = cache.cached_session_id_required(None, "api-key-1").unwrap();
    let second = cache.cached_session_id_required(None, "api-key-1").unwrap();
    assert_eq!(first, second);
    assert!(Uuid::parse_str(&first).is_ok());
}

#[test]
fn local_ttl_is_renewed_and_then_expires() {
    let clock = Arc::new(FakeClock::new());
    let cache = SessionIdCache::with_clock_and_capacity(clock.clone(), 8);
    let first = cache.cached_session_id_required(None, "key").unwrap();
    clock.advance(Duration::from_secs(50 * 60));
    assert_eq!(
        cache.cached_session_id_required(None, "key").unwrap(),
        first
    );
    clock.advance(Duration::from_secs(50 * 60));
    assert_eq!(
        cache.cached_session_id_required(None, "key").unwrap(),
        first
    );
    clock.advance(SESSION_ID_TTL + Duration::from_nanos(1));
    assert_ne!(
        cache.cached_session_id_required(None, "key").unwrap(),
        first
    );
}

#[test]
fn cache_is_instance_isolated_and_cardinality_bounded() {
    let first = SessionIdCache::with_clock_and_capacity(Arc::new(FakeClock::new()), 2);
    let second = SessionIdCache::with_clock_and_capacity(Arc::new(FakeClock::new()), 2);
    assert_ne!(
        first.cached_session_id_required(None, "same").unwrap(),
        second.cached_session_id_required(None, "same").unwrap()
    );
    first.cached_session_id_required(None, "two").unwrap();
    first.cached_session_id_required(None, "three").unwrap();
    assert_eq!(first.len_for_test(), 2);
}

#[test]
fn concurrent_local_callers_converge_on_one_id() {
    let cache = Arc::new(SessionIdCache::new());
    let barrier = Arc::new(std::sync::Barrier::new(16));
    let workers = (0..16)
        .map(|_| {
            let cache = Arc::clone(&cache);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                cache
                    .cached_session_id_required(None, "shared-key")
                    .unwrap()
            })
        })
        .collect::<Vec<_>>();
    let values = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    assert!(values.iter().all(|value| value == &values[0]));
    assert_eq!(cache.len_for_test(), 1);
}

#[test]
fn poisoned_local_lock_recovers_without_losing_authority() {
    let cache = SessionIdCache::new();
    let first = cache.cached_session_id_required(None, "key").unwrap();
    cache.poison_for_test();
    assert_eq!(
        cache.cached_session_id_required(None, "key").unwrap(),
        first
    );
}

#[test]
fn raw_api_key_never_appears_in_local_or_durable_key() {
    let api_key = "secret-api-key";
    assert!(!session_id_cache_key(api_key).contains(api_key));
    assert!(!claude_session_id_kv_key(api_key).contains(api_key));
}
