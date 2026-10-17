// ref: internal/home/kv_helpers_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::client::*;
use super::global::HomeRuntime;
use super::kv_helpers::*;
use std::sync::Arc;
use std::time::Duration;

struct FailingTransport;
impl HomeTransport for FailingTransport {
    fn ping(&self) -> Result<(), HomeError> {
        Ok(())
    }
    fn get(&self, _: &str) -> Result<Option<Vec<u8>>, HomeError> {
        Err(HomeError::Transport("failed".into()))
    }
    fn set(&self, _: &str, _: &[u8], _: KvSetOptions) -> Result<bool, HomeError> {
        Err(HomeError::Transport("failed".into()))
    }
    fn compare_and_swap(
        &self,
        _: &str,
        _: Option<&[u8]>,
        _: &[u8],
        _: Duration,
    ) -> Result<bool, HomeError> {
        Err(HomeError::Transport("failed".into()))
    }
    fn delete(&self, _: &[String]) -> Result<i64, HomeError> {
        Err(HomeError::Transport("failed".into()))
    }
    fn expire(&self, _: &str, _: Duration) -> Result<bool, HomeError> {
        Err(HomeError::Transport("failed".into()))
    }
    fn ttl(&self, _: &str) -> Result<Option<Duration>, HomeError> {
        Err(HomeError::Transport("failed".into()))
    }
    fn increment(&self, _: &str, _: i64) -> Result<i64, HomeError> {
        Err(HomeError::Transport("failed".into()))
    }
    fn push(&self, _: &str, _: &[u8], _: bool) -> Result<(), HomeError> {
        Err(HomeError::Transport("failed".into()))
    }
    fn request(&self, _: &str, _: &[u8]) -> Result<Vec<u8>, TransportFailure> {
        Err(TransportFailure {
            stage: DispatchFailureStage::BeforeSend,
            message: "failed".into(),
        })
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
