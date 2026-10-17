// ref: internal/api/redis_queue_protocol_integration_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use super::redis_queue_protocol::*;

struct Queue {
    password: String,
    items: Mutex<BTreeMap<String, VecDeque<Vec<u8>>>>,
}

impl Queue {
    fn new() -> Self {
        Self {
            password: "management-secret".into(),
            items: Mutex::new(BTreeMap::from([(
                "usage".into(),
                VecDeque::from([b"one".to_vec(), b"two".to_vec(), b"three".to_vec()]),
            )])),
        }
    }
}

impl RedisQueueAuthority for Queue {
    fn authenticate(&self, password: &str) -> bool {
        password == self.password
    }
    fn pop(&self, channel: &str, count: usize) -> Result<Vec<Vec<u8>>, RedisQueueError> {
        if channel != "usage" {
            return Err(RedisQueueError::UnsupportedChannel);
        }
        let mut items = self.items.lock().map_err(|_| RedisQueueError::Read)?;
        let queue = items.get_mut(channel).ok_or(RedisQueueError::Read)?;
        Ok((0..count).filter_map(|_| queue.pop_front()).collect())
    }
}

fn command(values: &[&str]) -> RespValue {
    RespValue::Array(
        values
            .iter()
            .map(|value| RespValue::Bulk(Some(value.as_bytes().to_vec())))
            .collect(),
    )
}

fn remote_session(policy: RedisQueuePolicy) -> RedisQueueSession {
    RedisQueueSession::new(Arc::new(Queue::new()), policy)
}
