// ref: sdk/cliproxy/auth/home_in_flight_publisher_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: deterministic packing, bounded overflow and injected transport/clock behavior
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};

use crate::internal::home::{HomeError, InFlightFrameKind};
use crate::sdk::cliproxy::executionregistry::{Freeze, Observation, Registry};

use super::{
    encode_home_in_flight_freeze, HomeClock, HomeInFlightPublisher, HomeInFlightPublisherConfig,
    HomeInFlightTransport,
};

#[derive(Default)]
struct SnapshotTransport {
    ready: bool,
    payloads: Mutex<Vec<Vec<u8>>>,
}

impl HomeInFlightTransport for SnapshotTransport {
    fn heartbeat_ok(&self) -> bool {
        self.ready
    }
    fn push_in_flight_snapshot(&self, payload: &[u8]) -> Result<(), HomeError> {
        self.payloads.lock().unwrap().push(payload.to_vec());
        Ok(())
    }
}

struct FixedClock(DateTime<Utc>);
impl HomeClock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

fn now() -> DateTime<Utc> {
    "2026-08-04T12:00:00Z".parse().unwrap()
}
