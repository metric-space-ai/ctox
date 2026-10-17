// ref: internal/home/in_flight_contract_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use super::requests::*;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    part: InFlightSnapshotFrame,
    overflow: InFlightSnapshotFrame,
}

fn fixture() -> &'static [u8] {
    br#"{
 "part":{"kind":"part","revision":7,"observed_at":"2026-01-01T00:00:00Z","barrier_revision":6,"part_index":0,"part_count":1,"details_truncated":true,"aggregates":[{"credential_id":"cred-a","model":"gpt","status":"accounted","count":2},{"credential_id":"cred-b","model":"gpt","status":"unaccounted","count":1}],"details":[{"request_id":"req-1","credential_id":"cred-a","model":"gpt","request_kind":"stream","started_at":"2026-01-01T00:00:00Z"}]},
 "overflow":{"kind":"overflow","revision":8,"observed_at":"2026-01-01T00:00:01Z","barrier_revision":7,"aggregate_group_count":100001}
}"#
}
