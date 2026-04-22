use std::hash::Hash;
use std::num::NonZeroUsize;
use std::sync::Mutex;

use lru::LruCache;
use sha1::Digest;
use sha1::Sha1;

pub fn sha1_digest(bytes: &[u8]) -> [u8; 20] {
    let digest = Sha1::digest(bytes);
    let mut out = [0u8; 20];
    out.copy_from_slice(&digest);
    out
}

pub struct BlockingLruCache<K, V> {
    inner: Mutex<LruCache<K, V>>,
}

impl<K, V> BlockingLruCache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            inner: Mutex::new(LruCache::new(capacity)),
        }
    }

    pub fn get_or_try_insert_with<E, F>(&self, key: K, f: F) -> Result<V, E>
    where
        F: FnOnce() -> Result<V, E>,
    {
        if let Some(value) = self
            .inner
            .lock()
            .expect("BlockingLruCache mutex poisoned")
            .get(&key)
            .cloned()
        {
            return Ok(value);
        }

        let value = f()?;

        let mut cache = self.inner.lock().expect("BlockingLruCache mutex poisoned");
        if let Some(existing) = cache.get(&key).cloned() {
            return Ok(existing);
        }
        cache.put(key, value.clone());
        Ok(value)
    }
}
