// ref: internal/client/codex/live/media_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::*;

struct Sink {
    messages: Mutex<Vec<DataChannelMessage>>,
    buffered: AtomicU64,
    closed: AtomicBool,
}

impl DataChannelSink for Sink {
    fn buffered_amount(&self) -> u64 {
        self.buffered.load(Ordering::Acquire)
    }

    fn send(&self, message: &DataChannelMessage) -> Result<(), LiveError> {
        self.messages.lock().unwrap().push(message.clone());
        Ok(())
    }

    fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }
}
