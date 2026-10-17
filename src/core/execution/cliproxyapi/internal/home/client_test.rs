// ref: internal/home/client_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::certificate::{
    certificate_fingerprint_pem, encode_resp_array, normalize_fingerprint, parse_home_jwt_claims,
};
use super::client::*;
use super::global::HomeRuntime;
use base64::Engine;
use sha2::Digest;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
struct MemoryTransport {
    values: Mutex<HashMap<String, Vec<u8>>>,
    pushes: Mutex<Vec<(String, Vec<u8>, bool)>>,
    request: Mutex<Option<Result<Vec<u8>, TransportFailure>>>,
}
impl HomeTransport for MemoryTransport {
    fn ping(&self) -> Result<(), HomeError> {
        Ok(())
    }
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, HomeError> {
        Ok(self.values.lock().unwrap().get(key).cloned())
    }
    fn set(&self, key: &str, value: &[u8], options: KvSetOptions) -> Result<bool, HomeError> {
        let mut values = self.values.lock().unwrap();
        if options.nx && values.contains_key(key) {
            return Ok(false);
        }
        if options.xx && !values.contains_key(key) {
            return Ok(false);
        }
        values.insert(key.into(), value.into());
        Ok(true)
    }
    fn compare_and_swap(
        &self,
        key: &str,
        expected: Option<&[u8]>,
        value: &[u8],
        _: Duration,
    ) -> Result<bool, HomeError> {
        let mut values = self.values.lock().unwrap();
        if values.get(key).map(Vec::as_slice) != expected {
            return Ok(false);
        }
        values.insert(key.into(), value.into());
        Ok(true)
    }
    fn delete(&self, keys: &[String]) -> Result<i64, HomeError> {
        let mut values = self.values.lock().unwrap();
        Ok(keys
            .iter()
            .filter(|key| values.remove(*key).is_some())
            .count() as i64)
    }
    fn expire(&self, key: &str, _: Duration) -> Result<bool, HomeError> {
        Ok(self.values.lock().unwrap().contains_key(key))
    }
    fn ttl(&self, key: &str) -> Result<Option<Duration>, HomeError> {
        Ok(self
            .values
            .lock()
            .unwrap()
            .contains_key(key)
            .then_some(Duration::from_secs(1)))
    }
    fn increment(&self, key: &str, delta: i64) -> Result<i64, HomeError> {
        let mut values = self.values.lock().unwrap();
        let next = values
            .get(key)
            .and_then(|v| std::str::from_utf8(v).ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
            + delta;
        values.insert(key.into(), next.to_string().into_bytes());
        Ok(next)
    }
    fn push(&self, key: &str, payload: &[u8], right: bool) -> Result<(), HomeError> {
        self.pushes
            .lock()
            .unwrap()
            .push((key.into(), payload.into(), right));
        Ok(())
    }
    fn request(&self, _: &str, _: &[u8]) -> Result<Vec<u8>, TransportFailure> {
        self.request
            .lock()
            .unwrap()
            .take()
            .unwrap_or_else(|| Ok(br#"{"ok":true}"#.to_vec()))
    }
    fn request_with_timeout(
        &self,
        key: &str,
        payload: &[u8],
        _: Duration,
    ) -> Result<Vec<u8>, TransportFailure> {
        self.request(key, payload)
    }
}
fn client(transport: Arc<MemoryTransport>) -> Client {
    let facade: Arc<dyn HomeTransport> = transport;
    Client::new(
        HomeConfig {
            enabled: true,
            host: "home".into(),
            port: 6379,
            ..HomeConfig::default()
        },
        facade,
    )
}
