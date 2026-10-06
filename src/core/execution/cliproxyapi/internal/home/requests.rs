// ref: internal/home/requests.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthDispatchRequest {
    #[serde(rename = "type")]
    pub request_type: String,
    pub model: String,
    pub count: i32,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub concurrency_protocol: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub session_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub parent_session_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub node_kind: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub credential_policy: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_round: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excluded_auth_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pinned_auth_id: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelsRequest {
    #[serde(rename = "type")]
    pub request_type: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub query: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshRequest {
    #[serde(rename = "type")]
    pub request_type: String,
    pub auth_index: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub access_token_sha256: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InFlightFrameKind {
    Part,
    Overflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InFlightAccountedStatus {
    Accounted,
    Unaccounted,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InFlightAggregate {
    pub credential_id: String,
    pub model: String,
    pub status: InFlightAccountedStatus,
    pub count: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InFlightRequestDetail {
    pub request_id: String,
    pub credential_id: String,
    pub model: String,
    pub request_kind: String,
    pub started_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InFlightSnapshotFrame {
    pub kind: InFlightFrameKind,
    pub revision: i64,
    pub observed_at: DateTime<Utc>,
    pub barrier_revision: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_index: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_count: Option<i32>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub details_truncated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aggregates: Vec<InFlightAggregate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<InFlightRequestDetail>,
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub aggregate_group_count: usize,
}

fn is_zero_i32(value: &i32) -> bool {
    *value == 0
}
fn is_zero_usize(value: &usize) -> bool {
    *value == 0
}
fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_omits_unset_session_routing_fields() {
        let request = AuthDispatchRequest {
            request_type: "auth".into(),
            model: "m".into(),
            count: 1,
            ..AuthDispatchRequest::default()
        };
        let json = serde_json::to_value(&request).unwrap();
        assert!(json.get("parent_session_id").is_none());
        assert!(json.get("node_kind").is_none());
        assert!(json.get("retry_round").is_none());
        assert!(json.get("excluded_auth_ids").is_none());
        assert!(json.get("pinned_auth_id").is_none());

        let routed = AuthDispatchRequest {
            parent_session_id: "parent".into(),
            node_kind: "leaf".into(),
            retry_round: Some(0),
            excluded_auth_ids: Some(vec!["auth-a".into()]),
            pinned_auth_id: "pin".into(),
            ..request
        };
        let json = serde_json::to_value(&routed).unwrap();
        assert_eq!(json["parent_session_id"], "parent");
        assert_eq!(json["node_kind"], "leaf");
        assert_eq!(json["retry_round"], 0);
        assert_eq!(json["excluded_auth_ids"], serde_json::json!(["auth-a"]));
        assert_eq!(json["pinned_auth_id"], "pin");
    }
}
