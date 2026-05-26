//! V1.5 Query-Demand-Loading status surface (Rust side).
//!
//! Mirrors `src/apps/business-os/rxdb/src/v1_5_status.mjs`. Filled
//! progressively by later waves; this module currently exposes the constant
//! capability name and the field list that the Rust peer must report.

pub const V1_5_QUERY_FETCH_CAPABILITY: &str = "ctox-rxdb-query-fetch-v1";

pub const V1_5_STATUS_FIELDS: &[&str] = &[
    "rxdbRuntime",
    "rxdbProtocolVersion",
    "transport",
    "peerConnected",
    "peerCapabilityQueryFetchV1",
    "queryDemandLoadingEnabled",
    "queryDemandLoadingActive",
    "queryFetchInFlight",
    "queryFetchSuccessCount",
    "queryFetchErrorCount",
    "queryFetchDedupHitCount",
    "indexedDbWorkingSetBytes",
    "indexedDbEvictionCount",
    "pinnedDocCount",
    "pinnedBytes",
    "lastQueryFetchMs",
    "lastTransportBackpressureMs",
    "lastReloadHydrationMs",
];

#[derive(Debug, Clone)]
pub struct V1_5StatusBaseline;

impl V1_5StatusBaseline {
    pub fn protocol_version(&self) -> &'static str {
        "1"
    }

    pub fn query_demand_loading_active(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_constant_is_stable() {
        assert_eq!(V1_5_QUERY_FETCH_CAPABILITY, "ctox-rxdb-query-fetch-v1");
    }

    #[test]
    fn status_field_list_contains_negotiation_fields() {
        assert!(V1_5_STATUS_FIELDS.contains(&"peerCapabilityQueryFetchV1"));
        assert!(V1_5_STATUS_FIELDS.contains(&"queryDemandLoadingActive"));
    }

    #[test]
    fn baseline_reports_protocol_version_one() {
        let baseline = V1_5StatusBaseline;
        assert_eq!(baseline.protocol_version(), "1");
        assert!(!baseline.query_demand_loading_active());
    }
}
