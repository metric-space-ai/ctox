// ref: sdk/pluginapi/types.go:1577-1769 @ 0d18d3b365be000eeaee7de4615f9da596b2059d
// License: MIT (upstream); modifications AGPL-3.0-only
use super::*;
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[test]
fn quota_aliases_preserve_canonical_values_and_explicit_zero() {
    let value:QuotaFetchResponse=serde_json::from_value(json!({
        "subscription":{"plan":"paid","tierName":"canonical","tier_name":"alias","tierId":"","tier_id":"fallback"},
        "serverTimeOffsetMs":0,"server_time_offset_ms":-123,
        "groups":[{"displayName":"canonical","display_name":"alias","buckets":[
            {"window":"5h","remainingFraction":0,"remaining_fraction":0.75,"resetTime":"","reset_time":"tomorrow"},
            {"remainingFraction":null,"remaining_fraction":0.25}
        ]}]
    })).unwrap();
    let plan = value.subscription.as_ref().unwrap();
    assert_eq!(plan.tier_name, "canonical");
    assert_eq!(plan.tier_id, "fallback");
    assert_eq!(value.server_time_offset_ms, -123);
    assert_eq!(value.groups[0].display_name, "canonical");
    assert_eq!(value.groups[0].buckets[0].remaining_fraction, 0.0);
    assert_eq!(value.groups[0].buckets[0].reset_time, "tomorrow");
    assert_eq!(value.groups[0].buckets[1].remaining_fraction, 0.25);
    let encoded = serde_json::to_value(&value).unwrap();
    assert!(encoded.get("server_time_offset_ms").is_none());
    assert_eq!(encoded["serverTimeOffsetMs"], -123);
    assert!(encoded["groups"][0].get("display_name").is_none());
}

#[test]
fn quota_nulls_and_empty_optionals_follow_go_wire_shape() {
    let value: QuotaFetchResponse = serde_json::from_value(
        json!({"subscription":null,"groups":null,"summary":null,"serverTimeOffsetMs":null}),
    )
    .unwrap();
    assert_eq!(serde_json::to_value(value).unwrap(), json!({}));
    let bucket: QuotaBucket =
        serde_json::from_value(json!({"window":null,"description":null})).unwrap();
    let encoded = serde_json::to_value(bucket).unwrap();
    assert_eq!(encoded.as_object().unwrap().len(), 1);
    assert_eq!(encoded["remainingFraction"].as_f64(), Some(0.0));
    assert_eq!(
        serde_json::to_value(QuotaDescribeResponse::default()).unwrap(),
        json!({})
    );
    assert_eq!(
        serde_json::to_value(QuotaResetResponse::default()).unwrap(),
        json!({"success":false})
    );
}

#[test]
fn quota_wrong_numeric_types_and_nonfinite_output_are_rejected() {
    for value in [
        json!({"remainingFraction":"0.5"}),
        json!({"remaining_fraction":true}),
        json!({"remainingFraction":[]}),
    ] {
        assert!(serde_json::from_value::<QuotaBucket>(value).is_err());
    }
    for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let bucket = QuotaBucket {
            remaining_fraction: number,
            ..Default::default()
        };
        assert!(serde_json::to_value(bucket).is_err());
        let metric = QuotaMetric {
            key: "balance".into(),
            label: "Balance".into(),
            value: number,
            ..Default::default()
        };
        assert!(serde_json::to_value(metric).is_err());
    }
    let zero = QuotaMetric {
        key: "balance".into(),
        label: "Balance".into(),
        value: 0.0,
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_value(zero).unwrap()["value"].as_f64(),
        Some(0.0)
    );
}

#[test]
fn quota_credential_context_is_redacted_and_http_cannot_arrive_over_json() {
    let raw = json!({"auth_index":"index","auth_id":"account","provider":"quota-fixture",
        "storage_json":"c2Vuc2l0aXZlLWJ5dGVz","metadata":{"token":"sensitive-metadata"},
        "attributes":{"api_key":"sensitive-attribute","nullable":null},"host":null,
        "http_client":{"forged":"capability"}});
    let request: QuotaFetchRequest = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(request.storage_json, b"sensitive-bytes");
    assert!(request.http_client.is_none());
    assert_eq!(request.attributes["nullable"], "");
    let text = format!("{request:?}");
    for secret in [
        "sensitive-bytes",
        "sensitive-metadata",
        "sensitive-attribute",
    ] {
        assert!(!text.contains(secret));
    }
    let encoded = serde_json::to_value(&request).unwrap();
    assert_eq!(encoded["storage_json"], "c2Vuc2l0aXZlLWJ5dGVz");
    assert!(encoded.get("http_client").is_none());
    let reset: QuotaResetRequest = serde_json::from_value(raw).unwrap();
    assert!(!format!("{reset:?}").contains("sensitive-metadata"));
    let empty: QuotaFetchRequest =
        serde_json::from_value(json!({"storage_json":null,"metadata":null,"attributes":null}))
            .unwrap();
    let empty = serde_json::to_value(empty).unwrap();
    for omitted in ["storage_json", "metadata", "attributes"] {
        assert!(empty.get(omitted).is_none());
    }
}

struct CountingHttp(Arc<AtomicUsize>);
impl HostHttpClient for CountingHttp {
    fn execute<'a>(&'a self, _: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async move {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(HttpResponse {
                status_code: 200,
                ..Default::default()
            })
        })
    }
    fn execute_stream<'a>(&'a self, _: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async { panic!("unused quota fixture stream") })
    }
}
struct FixtureQuota;
impl QuotaProvider for FixtureQuota {
    fn identifier(&self) -> &str {
        "quota-fixture"
    }
    fn describe_quota<'a>(
        &'a self,
        _: QuotaDescribeRequest,
    ) -> PluginFuture<'a, QuotaDescribeResponse> {
        Box::pin(async {
            Ok(QuotaDescribeResponse {
                supported_providers: vec!["quota-fixture".into()],
                ..Default::default()
            })
        })
    }
    fn fetch_quota<'a>(
        &'a self,
        request: QuotaFetchRequest,
    ) -> PluginFuture<'a, QuotaFetchResponse> {
        Box::pin(async move {
            let response = request
                .http_client
                .unwrap()
                .execute(HttpRequest {
                    method: "GET".into(),
                    url: "https://quota.invalid".into(),
                    ..Default::default()
                })
                .await?;
            assert_eq!(response.status_code, 200);
            Ok(QuotaFetchResponse {
                subscription: Some(QuotaSubscription {
                    plan: "paid".into(),
                    ..Default::default()
                }),
                ..Default::default()
            })
        })
    }
    fn reset_quota<'a>(&'a self, _: QuotaResetRequest) -> PluginFuture<'a, QuotaResetResponse> {
        Box::pin(async {
            Ok(QuotaResetResponse {
                success: false,
                message: "provider declined".into(),
            })
        })
    }
}
#[tokio::test]
async fn quota_provider_uses_only_the_retained_host_http_capability() {
    let count = Arc::new(AtomicUsize::new(0));
    let client: Arc<dyn HostHttpClient> = Arc::new(CountingHttp(count.clone()));
    let request = QuotaFetchRequest {
        http_client: Some(client.clone()),
        ..Default::default()
    };
    assert!(Arc::ptr_eq(request.http_client.as_ref().unwrap(), &client));
    let response = FixtureQuota.fetch_quota(request).await.unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(response.subscription.unwrap().plan, "paid");
    let reset = FixtureQuota
        .reset_quota(QuotaResetRequest::default())
        .await
        .unwrap();
    assert!(!reset.success);
    assert_eq!(reset.message, "provider declined");
    assert_eq!(
        FixtureQuota
            .describe_quota(QuotaDescribeRequest::default())
            .await
            .unwrap()
            .supported_providers,
        vec!["quota-fixture"]
    );
}
