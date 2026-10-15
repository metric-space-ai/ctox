// ref: internal/api/handlers/management/plugin_quota_test.go @ 0d18d3b365be000eeaee7de4615f9da596b2059d
// License: MIT (upstream); modifications AGPL-3.0-only
use super::plugin_quota::*;
use serde_json::{json, Value};

#[test]
fn quota_normalization_matches_real_go_probe_receipts() {
    fn numbers(value: Value) -> Value {
        match value {
            Value::Number(number) => json!(number.as_f64().unwrap()),
            Value::Array(items) => Value::Array(items.into_iter().map(numbers).collect()),
            Value::Object(items) => {
                Value::Object(items.into_iter().map(|(k, v)| (k, numbers(v))).collect())
            }
            value => value,
        }
    }
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("quota_probe_reference.json")).unwrap();
    assert_eq!(cases.len(), 16);
    for case in cases {
        let input = serde_json::to_vec(&case["input"]).unwrap();
        let actual =
            normalize_quota_probe(&input, case.get("mapping").and_then(Value::as_object), 0);
        if case["reject"].as_bool().unwrap_or(false) {
            assert!(actual.is_err(), "{}", case["name"]);
        } else {
            assert_eq!(
                numbers(serde_json::to_value(actual.unwrap()).unwrap()),
                numbers(case["expected"].clone()),
                "{}",
                case["name"]
            );
        }
    }
}

use super::{
    ManagementApiCallExecutor, ManagementApiToolError, ManagementApiToolRequest,
    ManagementApiToolResponse, ManagementApiTools, ManagementQuotaProbe, ManagementQuotaProbeError,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};
struct ProbeAuthority {
    calls: Mutex<Vec<ManagementApiToolRequest>>,
    response: Result<ManagementApiToolResponse, ManagementApiToolError>,
}
impl ManagementApiCallExecutor for ProbeAuthority {
    fn execute(
        &self,
        request: &ManagementApiToolRequest,
    ) -> Result<ManagementApiToolResponse, ManagementApiToolError> {
        self.calls.lock().unwrap().push(request.clone());
        self.response.clone()
    }
}
fn probe_authority(status: u16, body: &str) -> Arc<ProbeAuthority> {
    Arc::new(ProbeAuthority {
        calls: Mutex::new(vec![]),
        response: Ok(ManagementApiToolResponse {
            status_code: status,
            body: body.into(),
            headers: BTreeMap::from([(
                "Date".into(),
                vec!["Thu, 01 Jan 1970 00:00:00 GMT".into()],
            )]),
        }),
    })
}
#[test]
fn quota_probe_retains_the_selected_account_and_host_token_boundary() {
    let authority = probe_authority(200, r#"{"subscription":{"plan":"paid"}}"#);
    let probe = ManagementQuotaProbe::new(Arc::new(ManagementApiTools::new(authority.clone())));
    let input = json!({"url":" https://quota.invalid/balance ","header":{"Authorization":"Bearer $TOKEN$","Ignore":12},"headers":{"Wrong":"ignored"},"data":"$TOKEN$"});
    let out = probe
        .fetch(
            " selected-index ",
            input.as_object().unwrap(),
            SystemTime::UNIX_EPOCH + Duration::from_millis(2000),
        )
        .unwrap()
        .unwrap();
    assert_eq!(out.server_time_offset_ms, -2000);
    let calls = authority.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].auth_index, "selected-index");
    assert_eq!(calls[0].method, "GET");
    assert_eq!(
        calls[0].headers,
        BTreeMap::from([("Authorization".into(), "Bearer $TOKEN$".into())])
    );
    assert_eq!(calls[0].body, "$TOKEN$");
}
#[test]
fn quota_probe_rejects_missing_account_and_insecure_urls_before_host_execution() {
    let authority = probe_authority(200, "{}");
    let probe = ManagementQuotaProbe::new(Arc::new(ManagementApiTools::new(authority.clone())));
    assert_eq!(
        probe.fetch(
            "",
            json!({"url":"https://quota.invalid"}).as_object().unwrap(),
            SystemTime::UNIX_EPOCH
        ),
        Err(ManagementQuotaProbeError::InvalidAccount)
    );
    assert_eq!(
        probe.fetch(
            "index",
            json!({"url":"http://quota.invalid"}).as_object().unwrap(),
            SystemTime::UNIX_EPOCH
        ),
        Err(ManagementQuotaProbeError::Transport(
            ManagementApiToolError::InsecureUrl
        ))
    );
    assert!(probe
        .fetch(
            "index",
            json!({}).as_object().unwrap(),
            SystemTime::UNIX_EPOCH
        )
        .unwrap()
        .is_none());
    assert!(authority.calls.lock().unwrap().is_empty());
}
#[test]
fn quota_probe_errors_never_echo_upstream_bodies_or_credential_values() {
    let authority = probe_authority(401, "sensitive-upstream-token-echo");
    let probe = ManagementQuotaProbe::new(Arc::new(ManagementApiTools::new(authority)));
    let result = probe.fetch(
        "index",
        json!({"url":"https://quota.invalid"}).as_object().unwrap(),
        SystemTime::UNIX_EPOCH,
    );
    assert_eq!(result, Err(ManagementQuotaProbeError::UpstreamStatus(401)));
    assert!(!format!("{result:?}").contains("sensitive"));
    let authority = Arc::new(ProbeAuthority {
        calls: Mutex::new(vec![]),
        response: Err(ManagementApiToolError::CredentialUnavailable),
    });
    let probe = ManagementQuotaProbe::new(Arc::new(ManagementApiTools::new(authority)));
    assert_eq!(
        probe.fetch(
            "index",
            json!({"url":"https://quota.invalid","headers":{"Authorization":"Bearer $TOKEN$"}})
                .as_object()
                .unwrap(),
            SystemTime::UNIX_EPOCH
        ),
        Err(ManagementQuotaProbeError::Transport(
            ManagementApiToolError::CredentialUnavailable
        ))
    );
}

fn normalized(value: Value) -> Result<crate::sdk::pluginapi::QuotaFetchResponse, QuotaProbeError> {
    normalize_quota_probe(&serde_json::to_vec(&value).unwrap(), None, 0)
}
#[test]
fn quota_absence_is_not_exhaustion_and_zero_is_valid() {
    for value in [
        json!({}),
        json!({"groups":[{"buckets":[{}]}]}),
        json!({"groups":[{"buckets":[{"window":"5h"}]}]}),
        json!({"subscription":{"tierName":"paid"}}),
    ] {
        assert_eq!(normalized(value), Err(QuotaProbeError::InvalidShape));
    }
    let value=normalized(json!({"groups":[{"buckets":[{}, {"remainingFraction":0}, {"remaining_fraction":0.25}, {"remainingFraction":null}]}]})).unwrap();
    assert_eq!(
        value.groups[0]
            .buckets
            .iter()
            .map(|b| b.remaining_fraction)
            .collect::<Vec<_>>(),
        vec![0.0, 0.25]
    );
}
#[test]
fn quota_malformed_optional_summary_never_invalidates_core() {
    for key in ["summary", "SUMMARY", "Summary", "ſummary"] {
        let mut input = json!({"subscription":{"plan":"paid"}});
        input[key] = json!({"malformed":true});
        let result = normalized(input).unwrap();
        assert_eq!(result.subscription.unwrap().plan, "paid");
        assert!(result.summary.is_empty());
    }
}
#[test]
fn quota_summary_requires_real_numbers_and_string_identifiers() {
    let out = normalized(json!({"summary":[
        {"key":" balance ","label":" Balance ","value":0},
        {"key":"balance","label":"Balance","value":"0"},
        {"key":"balance","label":"Balance"},
        {"key":1,"label":"Balance","value":1},
        {"key":"balance","label":true,"value":1},
        {"key":"  ","label":"Balance","value":1},
        {"key":"balance","label":"Balance","value":null}
    ]}))
    .unwrap();
    assert_eq!(out.summary.len(), 1);
    assert_eq!(out.summary[0].value, 0.0);
    assert_eq!(out.summary[0].key, "balance");
}
#[test]
fn quota_currency_uses_the_exact_upstream_iso_set_and_ignores_bad_optional_metadata() {
    let out=normalized(json!({"summary":[
        {"key":"a","label":"A","value":1,"format":" currency ","currency":" eur ","unit":" credits "},
        {"key":"b","label":"B","value":2,"format":"currency","currency":"not-iso"},
        {"key":"c","label":"C","value":3,"format":"currency","currency":"ßd"},
        {"key":"d","label":"D","value":4,"format":"currency","currency":123},
        {"key":"e","label":"E","value":5,"format":true,"unit":10},
        {"key":"f","label":"F","value":6,"format":" number "}
    ]})).unwrap();
    assert_eq!(out.summary.len(), 6);
    assert_eq!(out.summary[0].currency, "EUR");
    assert_eq!(out.summary[0].unit, "credits");
    for metric in &out.summary[1..5] {
        assert!(metric.format.is_empty());
        assert!(metric.currency.is_empty());
    }
    assert_eq!(out.summary[5].format, "number");
    assert_eq!(out.summary[2].value, 3.0);
}
#[test]
fn quota_bad_core_shapes_fail_even_with_a_plan() {
    for raw in [
        json!({"subscription":{"plan":"paid"},"groups":"wrong"}),
        json!({"groups":[{"buckets":[{"remainingFraction":"0.5"}]}]}),
        json!({"groups":[{"buckets":[{"remainingFraction":true}]}]}),
    ] {
        assert_eq!(normalized(raw), Err(QuotaProbeError::InvalidShape));
    }
    assert_eq!(
        normalize_quota_probe(b"invalid", None, 0),
        Err(QuotaProbeError::InvalidJson)
    );
}
#[test]
fn quota_server_offset_only_fills_an_unreported_offset() {
    let input = br#"{"subscription":{"plan":"paid"}}"#;
    assert_eq!(
        normalize_quota_probe(input, None, -100)
            .unwrap()
            .server_time_offset_ms,
        -100
    );
    let input = br#"{"subscription":{"plan":"paid"},"serverTimeOffsetMs":123}"#;
    assert_eq!(
        normalize_quota_probe(input, None, -100)
            .unwrap()
            .server_time_offset_ms,
        123
    );
}
#[test]
fn quota_mapping_supports_dynamic_and_static_buckets_and_gjson_paths() {
    let input = json!({"plan":{"name":"Paid","tier":"Team"},"windows":[{"window":"5h","left":"0","total":100,"reset_time":"tomorrow"},{"window":"week","left":75,"total":100},{"left":1,"total":0},{"left":false,"total":10}],"fraction":0.5});
    let mapping = json!({"plan":"plan.name","tier_name":"plan.tier","groups":[{"display_name":"Allowance","buckets_path":"windows","remaining_amount_key":"left","total_amount_key":"total","buckets":[{"window":"manual","remaining_fraction":"fraction","description":"Static label"}]}]});
    let out = normalize_quota_probe(&serde_json::to_vec(&input).unwrap(), mapping.as_object(), 0)
        .unwrap();
    assert_eq!(out.subscription.unwrap().tier_name, "Team");
    assert_eq!(out.groups.len(), 1);
    assert_eq!(out.groups[0].display_name, "Allowance");
    let buckets = &out.groups[0].buckets;
    assert_eq!(buckets.len(), 3);
    assert_eq!(buckets[0].remaining_fraction, 0.0);
    assert_eq!(buckets[1].remaining_fraction, 0.75);
    assert_eq!(buckets[2].description, "Static label");
}
#[test]
fn quota_mapping_requires_usable_fields_and_keeps_summary_only_results() {
    let mapping = json!({"groups":[{"buckets":[{"remaining_fraction":"missing"},{"remaining_amount":"large","total_amount":"small"}]}]});
    assert_eq!(
        normalize_quota_probe(br#"{"large":1e308,"small":1e-308}"#, mapping.as_object(), 0),
        Err(QuotaProbeError::InvalidShape)
    );
    let input = br#"{"summary":[{"key":"balance","label":"Balance","value":0}]}"#;
    assert_eq!(
        normalize_quota_probe(input, mapping.as_object(), 0)
            .unwrap()
            .summary
            .len(),
        1
    );
}
#[test]
fn quota_oversized_response_is_rejected_before_parsing() {
    assert_eq!(
        normalize_quota_probe(&vec![b' '; 4 * 1024 * 1024 + 1], None, 0),
        Err(QuotaProbeError::ResponseTooLarge)
    );
}
