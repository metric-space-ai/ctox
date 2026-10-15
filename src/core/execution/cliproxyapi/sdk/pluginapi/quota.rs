// ref: sdk/pluginapi/types.go:1577-1769 @ 0d18d3b365be000eeaee7de4615f9da596b2059d
// Port-Status: candidate
// Port-Note: typed quota capability; HTTP policy remains owned by the injected host
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{HostConfigSummary, HostHttpClient, Metadata, PluginFuture};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::{collections::BTreeMap, fmt, sync::Arc};

fn null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}
fn string_map<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, String>, D::Error> {
    Ok(
        Option::<BTreeMap<String, Option<String>>>::deserialize(deserializer)?
            .unwrap_or_default()
            .into_iter()
            .map(|(key, value)| (key, value.unwrap_or_default()))
            .collect(),
    )
}
fn finite<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    if !value.is_finite() {
        return Err(serde::ser::Error::custom("quota number must be finite"));
    }
    serializer.serialize_f64(*value)
}
fn zero(value: &i64) -> bool {
    *value == 0
}
fn is_false(value: &bool) -> bool {
    !*value
}
mod quota_bytes {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(value: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(value))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let value = Option::<String>::deserialize(deserializer)?;
        base64::engine::general_purpose::STANDARD
            .decode(value.unwrap_or_default())
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct QuotaDescribeRequest {
    #[serde(deserialize_with = "null_default")]
    pub plugin: Metadata,
}
impl fmt::Debug for QuotaDescribeRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QuotaDescribeRequest")
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct QuotaDescribeResponse {
    #[serde(
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "null_default"
    )]
    pub supported_providers: Vec<String>,
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "null_default"
    )]
    pub display_name: String,
    #[serde(skip_serializing_if = "is_false", deserialize_with = "null_default")]
    pub supports_reset: bool,
}

macro_rules! quota_credential_request {
    ($name:ident) => {
        #[derive(Clone, Default, Serialize, Deserialize)]
        #[serde(default)]
        pub struct $name {
            #[serde(deserialize_with = "null_default")]
            pub auth_index: String,
            #[serde(deserialize_with = "null_default")]
            pub auth_id: String,
            #[serde(deserialize_with = "null_default")]
            pub provider: String,
            #[serde(skip_serializing_if = "Vec::is_empty", with = "quota_bytes")]
            pub storage_json: Vec<u8>,
            #[serde(
                skip_serializing_if = "BTreeMap::is_empty",
                deserialize_with = "null_default"
            )]
            pub metadata: BTreeMap<String, Value>,
            #[serde(
                skip_serializing_if = "BTreeMap::is_empty",
                deserialize_with = "string_map"
            )]
            pub attributes: BTreeMap<String, String>,
            #[serde(deserialize_with = "null_default")]
            pub host: HostConfigSummary,
            #[serde(skip)]
            pub http_client: Option<Arc<dyn HostHttpClient>>,
        }
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct(stringify!($name))
                    .field("storage_json_bytes", &self.storage_json.len())
                    .field("metadata_fields", &self.metadata.len())
                    .field("attribute_fields", &self.attributes.len())
                    .field("has_http_client", &self.http_client.is_some())
                    .finish_non_exhaustive()
            }
        }
    };
}
quota_credential_request!(QuotaFetchRequest);
quota_credential_request!(QuotaResetRequest);

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaSubscription {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub plan: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub tier_name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub tier_id: String,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct SubscriptionWire {
    #[serde(deserialize_with = "null_default")]
    plan: String,
    #[serde(rename = "tierName", deserialize_with = "null_default")]
    tier_name: String,
    #[serde(rename = "tierId", deserialize_with = "null_default")]
    tier_id: String,
    #[serde(rename = "tier_name", deserialize_with = "null_default")]
    alt_tier_name: String,
    #[serde(rename = "tier_id", deserialize_with = "null_default")]
    alt_tier_id: String,
}
impl<'de> Deserialize<'de> for QuotaSubscription {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let wire = SubscriptionWire::deserialize(d)?;
        Ok(Self {
            plan: wire.plan,
            tier_name: if wire.tier_name.is_empty() {
                wire.alt_tier_name
            } else {
                wire.tier_name
            },
            tier_id: if wire.tier_id.is_empty() {
                wire.alt_tier_id
            } else {
                wire.tier_id
            },
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaBucket {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub window: String,
    #[serde(serialize_with = "finite")]
    pub remaining_fraction: f64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reset_time: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct BucketWire {
    #[serde(deserialize_with = "null_default")]
    window: String,
    #[serde(rename = "remainingFraction")]
    remaining_fraction: Option<f64>,
    #[serde(rename = "remaining_fraction")]
    alt_remaining_fraction: Option<f64>,
    #[serde(rename = "resetTime", deserialize_with = "null_default")]
    reset_time: String,
    #[serde(rename = "reset_time", deserialize_with = "null_default")]
    alt_reset_time: String,
    #[serde(deserialize_with = "null_default")]
    description: String,
}
impl<'de> Deserialize<'de> for QuotaBucket {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let wire = BucketWire::deserialize(d)?;
        Ok(Self {
            window: wire.window,
            remaining_fraction: wire
                .remaining_fraction
                .or(wire.alt_remaining_fraction)
                .unwrap_or_default(),
            reset_time: if wire.reset_time.is_empty() {
                wire.alt_reset_time
            } else {
                wire.reset_time
            },
            description: wire.description,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaGroup {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub display_name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub buckets: Vec<QuotaBucket>,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct GroupWire {
    #[serde(rename = "displayName", deserialize_with = "null_default")]
    display_name: String,
    #[serde(rename = "display_name", deserialize_with = "null_default")]
    alt_display_name: String,
    #[serde(deserialize_with = "null_default")]
    buckets: Vec<QuotaBucket>,
}
impl<'de> Deserialize<'de> for QuotaGroup {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let wire = GroupWire::deserialize(d)?;
        Ok(Self {
            display_name: if wire.display_name.is_empty() {
                wire.alt_display_name
            } else {
                wire.display_name
            },
            buckets: wire.buckets,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QuotaMetric {
    #[serde(deserialize_with = "null_default")]
    pub key: String,
    #[serde(deserialize_with = "null_default")]
    pub label: String,
    #[serde(deserialize_with = "null_default", serialize_with = "finite")]
    pub value: f64,
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "null_default"
    )]
    pub unit: String,
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "null_default"
    )]
    pub format: String,
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "null_default"
    )]
    pub currency: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaFetchResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscription: Option<QuotaSubscription>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub summary: Vec<QuotaMetric>,
    #[serde(skip_serializing_if = "zero")]
    pub server_time_offset_ms: i64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<QuotaGroup>,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct ResponseWire {
    subscription: Option<QuotaSubscription>,
    #[serde(deserialize_with = "null_default")]
    summary: Vec<QuotaMetric>,
    #[serde(rename = "serverTimeOffsetMs", deserialize_with = "null_default")]
    server_time_offset_ms: i64,
    #[serde(rename = "server_time_offset_ms", deserialize_with = "null_default")]
    alt_server_time_offset_ms: i64,
    #[serde(deserialize_with = "null_default")]
    groups: Vec<QuotaGroup>,
}
impl<'de> Deserialize<'de> for QuotaFetchResponse {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let wire = ResponseWire::deserialize(d)?;
        Ok(Self {
            subscription: wire.subscription,
            summary: wire.summary,
            groups: wire.groups,
            server_time_offset_ms: if wire.server_time_offset_ms == 0 {
                wire.alt_server_time_offset_ms
            } else {
                wire.server_time_offset_ms
            },
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct QuotaResetResponse {
    #[serde(deserialize_with = "null_default")]
    pub success: bool,
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "null_default"
    )]
    pub message: String,
}

/// Quota callbacks use the host-provided credential context and HTTP policy.
/// A provider reset result is independent from the host's routing cooldown reset.
pub trait QuotaProvider: Send + Sync {
    fn identifier(&self) -> &str;
    fn describe_quota<'a>(
        &'a self,
        request: QuotaDescribeRequest,
    ) -> PluginFuture<'a, QuotaDescribeResponse>;
    fn fetch_quota<'a>(
        &'a self,
        request: QuotaFetchRequest,
    ) -> PluginFuture<'a, QuotaFetchResponse>;
    fn reset_quota<'a>(
        &'a self,
        request: QuotaResetRequest,
    ) -> PluginFuture<'a, QuotaResetResponse>;
}
