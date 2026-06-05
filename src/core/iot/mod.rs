// Origin: CTOX
// License: AGPL-3.0-only
//
// Native IoT engine core. Ported domain semantics from OpenRemote
// (AGPL-3.0, archive/openremote, HEAD 22a42a7); persistence/transport
// reimplemented on CTOX-native SQLite. See docs/legal/NOTICE.
//
// Time model (do not mix the two):
//   * datapoint / value / event time is `i64` epoch-ms (the ported domain
//     dimension, §2A.13) — see `now_ms`.
//   * CRUD audit columns (created_at / updated_at) are RFC-3339 millis-precision
//     UTC TEXT (CTOX house style) — see `now_iso`.

pub(crate) mod adapters;
pub(crate) mod agents;
pub(crate) mod alarms;
pub(crate) mod commands;
pub(crate) mod conditions;
pub(crate) mod datapoints;
pub(crate) mod gateway;
pub(crate) mod model;
pub(crate) mod projector;
pub(crate) mod runtime;
pub(crate) mod store;

// CI soak harness (Phase 7) — reuses the loopback MQTT fixture + runtime
// supervisor to drive many telemetry cycles + a forced reconnect + a
// business_commands round-trip. Test-only; runnable locally and in CI via
// .github/workflows/iot-soak.yml.
#[cfg(test)]
mod tests {
    pub(super) mod ci_soak;
}

pub(crate) use anyhow::{Context, Result};

/// UTC-millisecond clock. All IoT timestamps are i64 epoch-ms (§2A.13).
/// Tests inject a fixed value rather than reading the wall clock.
pub(crate) fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// RFC-3339 millis-precision UTC string for CRUD audit columns
/// (created_at/updated_at), matching business_os/secrets house style.
pub(crate) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
