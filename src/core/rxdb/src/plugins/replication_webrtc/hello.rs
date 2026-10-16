//! V4 control-lane contract. Compatibility is not authentication: the host must
//! verify the connection nonce/signature and issue scopes before hello_ok.
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

pub const PROTOCOL_VERSION: u16 = 4;
// Bootstrap control messages precede fragmentation negotiation, so they must
// fit one uncompressed frame. They cannot assume the selected codec yet.
pub const MAX_HELLO_BYTES: usize = 16 * 1024;
pub const MAX_FRAME_BYTES: u32 = 16 * 1024;
pub const MIN_FRAME_BYTES: u32 = 1024;

fn native_frame_limit() -> u32 {
    webrtc::data_channel::sctp_data_payload_limit() as u32
}

/// Never passes a sequence through a JSON number (including above JS 2^53).
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct WireSeq(pub u64);
impl Serialize for WireSeq {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}
impl<'de> Deserialize<'de> for WireSeq {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        let parsed = value.parse::<u64>().map_err(serde::de::Error::custom)?;
        if value != parsed.to_string() {
            return Err(serde::de::Error::custom(
                "expected canonical decimal u64 string",
            ));
        }
        Ok(Self(parsed))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApiRange {
    pub min: String,
    pub max: String,
}
fn api_version(value: &str) -> Option<(u16, u16)> {
    let (major, minor) = value.split_once('.')?;
    let parsed = (major.parse::<u16>().ok()?, minor.parse::<u16>().ok()?);
    (value == format!("{}.{}", parsed.0, parsed.1)).then_some(parsed)
}
impl ApiRange {
    fn bounds(&self) -> Option<((u16, u16), (u16, u16))> {
        let (min, max) = (api_version(&self.min)?, api_version(&self.max)?);
        (min <= max).then_some((min, max))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Codec {
    Zstd,
    Gzip,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TransportOffer {
    pub codecs: Vec<Codec>,
    pub max_frame_bytes: u32,
    pub fragmentation: bool,
    pub reassembly: bool,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TransportSelection {
    pub codec: Codec,
    pub frame_bytes: u32,
    pub fragmentation: bool,
    pub reassembly: bool,
}

/// Public proof only; private keys never enter this contract or diagnostics.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceProof {
    pub device_id: String,
    pub public_key: String,
    pub nonce: String,
    pub signature: String,
}
impl fmt::Debug for DeviceProof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeviceProof([redacted])")
    }
}
fn base64url(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
impl DeviceProof {
    pub fn matches_challenge(&self, nonce: &str) -> bool {
        !self.device_id.trim().is_empty()
            && self.device_id.len() <= 256
            && base64url(&self.public_key, 43)
            && base64url(&self.nonce, 43)
            && base64url(&self.signature, 86)
            && self.nonce == nonce
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Hello {
    pub protocol_ver: u16,
    pub bootstrap_ver: u16,
    pub app_api_ver: ApiRange,
    pub instance_id: String,
    pub capabilities: Vec<String>,
    pub device_proof: DeviceProof,
    pub scopes_wanted: Vec<String>,
    pub transport: TransportOffer,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HelloOk {
    pub protocol_ver: u16,
    pub bootstrap_ver: u16,
    pub app_api_ver: String,
    pub instance_id: String,
    pub capabilities: Vec<String>,
    pub transport: TransportSelection,
    pub epoch: String,
    pub head_seq: WireSeq,
    pub scope_tokens: Vec<String>,
    pub rights_ver: WireSeq,
}
impl Hello {
    /// Apply the native DATA payload limit at the outbound connection seam,
    /// preserving lossless serialization of received protocol documents.
    pub fn with_native_transport_limit(mut self) -> Self {
        self.transport.max_frame_bytes = self.transport.max_frame_bytes.min(native_frame_limit());
        self
    }

    /// Native initiator counterpart of the browser's hello_ok validation. The
    /// transport must already be bound to the paired Core's trusted identity.
    pub fn validate_response(
        &self,
        response: &HelloOk,
        required_capabilities: &[String],
    ) -> Result<(), HelloReject> {
        let reject = HelloReject::new;
        if self.protocol_ver != PROTOCOL_VERSION || response.protocol_ver != self.protocol_ver {
            return Err(reject(
                RejectReason::ProtocolIncompatible,
                "offered protocol",
            ));
        }
        if response.bootstrap_ver != self.bootstrap_ver {
            return Err(reject(
                RejectReason::BootstrapIncompatible,
                "offered bootstrap",
            ));
        }
        if response.instance_id != self.instance_id {
            return Err(reject(RejectReason::InstanceMismatch, "paired instance"));
        }
        let (Some((min, max)), Some(selected)) = (
            self.app_api_ver.bounds(),
            api_version(&response.app_api_ver),
        ) else {
            return Err(reject(
                RejectReason::AppApiIncompatible,
                "offered API range",
            ));
        };
        if selected < min || selected > max {
            return Err(reject(
                RejectReason::AppApiIncompatible,
                "offered API range",
            ));
        }
        if response
            .capabilities
            .iter()
            .any(|cap| !self.capabilities.contains(cap))
            || required_capabilities
                .iter()
                .any(|cap| !self.capabilities.contains(cap) || !response.capabilities.contains(cap))
        {
            return Err(reject(
                RejectReason::CapabilityMissing,
                "offered and required capabilities",
            ));
        }
        let transport = &response.transport;
        if !self.transport.codecs.contains(&transport.codec)
            || !(MIN_FRAME_BYTES..=MAX_FRAME_BYTES.min(self.transport.max_frame_bytes).min(native_frame_limit()))
                .contains(&transport.frame_bytes)
            || !self.transport.fragmentation
            || !self.transport.reassembly
            || !transport.fragmentation
            || !transport.reassembly
        {
            return Err(reject(
                RejectReason::TransportIncompatible,
                "offered transport limits",
            ));
        }
        if response.epoch.trim().is_empty()
            || response.scope_tokens.iter().any(|s| s.trim().is_empty())
        {
            return Err(reject(RejectReason::InvalidHello, "valid hello_ok fields"));
        }
        Ok(())
    }
}
impl fmt::Debug for HelloOk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HelloOk")
            .field("protocol_ver", &self.protocol_ver)
            .field("head_seq", &self.head_seq)
            .field("scope_tokens", &"[redacted]")
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectReason {
    InvalidHello,
    ProtocolIncompatible,
    BootstrapIncompatible,
    AppApiIncompatible,
    InstanceMismatch,
    CapabilityMissing,
    TransportIncompatible,
    DeviceProofInvalid,
    AuthenticationFailed,
    HandshakeTimeout,
    UnexpectedMessage,
}
/// Fixed reasons only: never expose a credential-provider error or signature.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HelloReject {
    pub reason: RejectReason,
    pub required: String,
}
impl HelloReject {
    pub fn new(reason: RejectReason, required: &str) -> Self {
        Self {
            reason,
            required: required.into(),
        }
    }
}
impl fmt::Display for HelloReject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "hello rejected: {:?}; required {}",
            self.reason, self.required
        )
    }
}
impl std::error::Error for HelloReject {}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HelloFrame {
    Hello(Hello),
    HelloOk(HelloOk),
    HelloReject(HelloReject),
}
impl HelloFrame {
    pub fn encode(&self) -> Result<Vec<u8>, HelloReject> {
        let bytes = serde_json::to_vec(self)
            .map_err(|_| HelloReject::new(RejectReason::InvalidHello, "valid V4 control frame"))?;
        Self::decode(&bytes)?;
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, HelloReject> {
        if bytes.len() > MAX_HELLO_BYTES {
            return Err(HelloReject::new(
                RejectReason::InvalidHello,
                "hello <= 16384 bytes",
            ));
        }
        let frame: Self = serde_json::from_slice(bytes)
            .map_err(|_| HelloReject::new(RejectReason::InvalidHello, "valid V4 control frame"))?;
        let valid = match &frame {
            Self::Hello(hello) => {
                !hello.instance_id.trim().is_empty()
                    && hello.app_api_ver.bounds().is_some()
                    && bounded_names(&hello.capabilities)
                    && bounded_names(&hello.scopes_wanted)
                    && hello.transport.codecs.len() <= 128
                    && hello
                        .device_proof
                        .matches_challenge(&hello.device_proof.nonce)
            }
            Self::HelloOk(ok) => {
                !ok.instance_id.trim().is_empty()
                    && !ok.epoch.trim().is_empty()
                    && api_version(&ok.app_api_ver).is_some()
                    && bounded_names(&ok.capabilities)
                    && ok.scope_tokens.iter().all(|token| !token.trim().is_empty())
                    && (MIN_FRAME_BYTES..=MAX_FRAME_BYTES).contains(&ok.transport.frame_bytes)
                    && ok.transport.fragmentation
                    && ok.transport.reassembly
            }
            Self::HelloReject(rejection) => {
                !rejection.required.trim().is_empty() && rejection.required.len() <= 256
            }
        };
        if !valid {
            return Err(HelloReject::new(
                RejectReason::InvalidHello,
                "valid V4 control frame",
            ));
        }
        Ok(frame)
    }
}

#[derive(Clone, Debug)]
pub struct HelloPolicy {
    pub instance_id: String,
    pub bootstrap_min: u16,
    pub bootstrap_max: u16,
    pub app_api_ver: ApiRange,
    pub capabilities: Vec<String>,
    pub required_capabilities: Vec<String>,
    pub transport: TransportOffer,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelloSelection {
    pub bootstrap_ver: u16,
    pub app_api_ver: String,
    pub capabilities: Vec<String>,
    pub transport: TransportSelection,
}
fn bounded_names(names: &[String]) -> bool {
    names.len() <= 128 && names.iter().all(|s| !s.trim().is_empty() && s.len() <= 256)
}
impl HelloPolicy {
    /// This checks wire compatibility only. It never grants access.
    pub fn negotiate(&self, hello: &Hello) -> Result<HelloSelection, HelloReject> {
        let reject = HelloReject::new;
        if hello.protocol_ver != PROTOCOL_VERSION {
            return Err(reject(RejectReason::ProtocolIncompatible, "protocol_ver=4"));
        }
        if self.instance_id.is_empty() || hello.instance_id != self.instance_id {
            return Err(reject(RejectReason::InstanceMismatch, "paired instance"));
        }
        if self.bootstrap_min > self.bootstrap_max
            || !(self.bootstrap_min..=self.bootstrap_max).contains(&hello.bootstrap_ver)
        {
            return Err(reject(
                RejectReason::BootstrapIncompatible,
                "supported bootstrap version",
            ));
        }
        let (Some((local_min, local_max)), Some((remote_min, remote_max))) =
            (self.app_api_ver.bounds(), hello.app_api_ver.bounds())
        else {
            return Err(reject(
                RejectReason::AppApiIncompatible,
                "valid major.minor range",
            ));
        };
        let min = local_min.max(remote_min);
        let max = local_max.min(remote_max);
        if min > max {
            return Err(reject(
                RejectReason::AppApiIncompatible,
                "overlapping app API range",
            ));
        }
        if !bounded_names(&hello.capabilities) || !bounded_names(&hello.scopes_wanted) {
            return Err(reject(
                RejectReason::InvalidHello,
                "bounded capabilities and scopes",
            ));
        }
        if self
            .required_capabilities
            .iter()
            .any(|name| !hello.capabilities.contains(name) || !self.capabilities.contains(name))
        {
            return Err(reject(
                RejectReason::CapabilityMissing,
                "required capabilities",
            ));
        }
        let mut capabilities: Vec<_> = self
            .capabilities
            .iter()
            .filter(|name| hello.capabilities.contains(name))
            .cloned()
            .collect();
        capabilities.sort();
        capabilities.dedup();
        let local = &self.transport;
        let remote = &hello.transport;
        let codec = [Codec::Zstd, Codec::Gzip]
            .into_iter()
            .find(|codec| local.codecs.contains(codec) && remote.codecs.contains(codec));
        let Some(codec) = codec else {
            return Err(reject(RejectReason::TransportIncompatible, "common codec"));
        };
        if !local.fragmentation
            || !local.reassembly
            || !remote.fragmentation
            || !remote.reassembly
            || local.max_frame_bytes < MIN_FRAME_BYTES
            || remote.max_frame_bytes < MIN_FRAME_BYTES
        {
            return Err(reject(
                RejectReason::TransportIncompatible,
                "fragmentation/reassembly; frame >= 1024",
            ));
        }
        Ok(HelloSelection {
            bootstrap_ver: hello.bootstrap_ver,
            app_api_ver: format!("{}.{}", max.0, max.1),
            capabilities,
            transport: TransportSelection {
                codec,
                frame_bytes: local
                    .max_frame_bytes
                    .min(remote.max_frame_bytes)
                    .min(MAX_FRAME_BYTES).min(native_frame_limit()),
                fragmentation: true,
                reassembly: true,
            },
        })
    }
}

#[cfg(test)]
#[path = "hello_tests.rs"]
pub(super) mod tests;
