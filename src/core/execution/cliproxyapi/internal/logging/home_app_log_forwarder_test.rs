// ref: internal/logging/home_app_log_forwarder_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::global_logger::{LogEntry, LogLevel};
use super::home_app_log_forwarder::{HomeAppLogForwarder, HomeAppLogPayload, HomeAppLogSink};
use std::io;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

struct StubSink {
    healthy: bool,
    unsupported: bool,
    pushed: Mutex<Vec<Vec<u8>>>,
}
impl StubSink {
    fn new(healthy: bool) -> Self {
        Self {
            healthy,
            unsupported: false,
            pushed: Mutex::new(Vec::new()),
        }
    }
}
impl HomeAppLogSink for StubSink {
    fn heartbeat_ok(&self) -> bool {
        self.healthy
    }
    fn push_app_log(&self, payload: &[u8]) -> io::Result<()> {
        if self.unsupported {
            return Err(io::Error::other("ERR unsupported key"));
        }
        self.pushed.lock().unwrap().push(payload.to_vec());
        Ok(())
    }
}

fn wait_count(sink: &StubSink, count: usize) {
    for _ in 0..100 {
        if sink.pushed.lock().unwrap().len() >= count {
            return;
        }
        thread::sleep(Duration::from_millis(2));
    }
}
