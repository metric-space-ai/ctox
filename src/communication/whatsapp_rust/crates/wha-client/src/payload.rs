//! Builder for the `ClientPayload` proto sent at the end of the noise
//! handshake. Mirrors `whatsmeow/store/clientpayload.go`.
//!
//! The wire payload bridges the noise-encrypted handshake to a real session:
//! WhatsApp's edge looks at the contents (UserAgent, version hash, registration
//! material) to decide whether to advertise pairing or accept the device as
//! already-paired. Sending an empty/default payload causes the server to drop
//! the connection with no diagnostic, which is why this builder must mirror
//! upstream byte-for-byte.

use md5::{Digest, Md5};
use prost::Message as _;

use wha_proto::{companion_reg, wa6};
use wha_store::Device;

use crate::error::ClientError;

// ---------------------------------------------------------------------------
// WAVersionContainer
// ---------------------------------------------------------------------------

/// Container for a WhatsApp web version number (three dot-separated parts).
/// Mirrors `WAVersionContainer` in `clientpayload.go`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WAVersionContainer(pub [u32; 3]);

impl WAVersionContainer {
    /// Parse a `"a.b.c"` string into a container. Mirrors `ParseVersion`.
    pub fn parse(version: &str) -> Result<Self, ClientError> {
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 3 {
            return Err(ClientError::Other(format!(
                "'{version}' doesn't contain three dot-separated parts"
            )));
        }
        let mut out = [0u32; 3];
        for (i, p) in parts.iter().enumerate() {
            out[i] = p.parse::<u32>().map_err(|e| {
                ClientError::Other(format!("part {} of '{}' is not a number: {}", i + 1, version, e))
            })?;
        }
        Ok(WAVersionContainer(out))
    }

    pub fn is_zero(&self) -> bool {
        self.0 == [0, 0, 0]
    }

    /// Dot-separated rendering. Mirrors `String()`.
    pub fn to_dot_string(&self) -> String {
        format!("{}.{}.{}", self.0[0], self.0[1], self.0[2])
    }

    /// MD5 hash of the dot-separated string. Mirrors `Hash()`.
    pub fn hash(&self) -> [u8; 16] {
        let mut h = Md5::new();
        h.update(self.to_dot_string().as_bytes());
        let out = h.finalize();
        let mut buf = [0u8; 16];
        buf.copy_from_slice(&out);
        buf
    }

    pub fn to_proto_app_version(&self) -> wa6::client_payload::user_agent::AppVersion {
        wa6::client_payload::user_agent::AppVersion {
            primary: Some(self.0[0]),
            secondary: Some(self.0[1]),
            tertiary: Some(self.0[2]),
            quaternary: None,
            quinary: None,
        }
    }
}

impl std::fmt::Display for WAVersionContainer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_dot_string())
    }
}

// ---------------------------------------------------------------------------
// Constants — mirror upstream `waVersion`.
// ---------------------------------------------------------------------------

/// The WhatsApp web client version we advertise. Copied verbatim from
/// `whatsmeow/store/clientpayload.go` (`waVersion`).
pub const WA_VERSION: WAVersionContainer = WAVersionContainer([2, 3000, 1038187123]);

// ---------------------------------------------------------------------------
// Static templates — mirror upstream `BaseClientPayload` and `DeviceProps`.
// ---------------------------------------------------------------------------

/// Static `ClientPayload` template. Both registration and login flows clone
/// this and then add the flow-specific fields (`DevicePairingData` /
/// `Username`+`Device`). Mirrors `BaseClientPayload`.
pub fn base_client_payload() -> wa6::ClientPayload {
    use wa6::client_payload::{
        user_agent::{Platform, ReleaseChannel},
        web_info::WebSubPlatform,
        ConnectReason, ConnectType, UserAgent, WebInfo,
    };

    wa6::ClientPayload {
        user_agent: Some(UserAgent {
            platform: Some(Platform::Web as i32),
            release_channel: Some(ReleaseChannel::Release as i32),
            app_version: Some(crate::version::current().to_proto_app_version()),
            mcc: Some("000".to_string()),
            mnc: Some("000".to_string()),
            os_version: Some("0.1.0".to_string()),
            manufacturer: Some(String::new()),
            device: Some("Desktop".to_string()),
            os_build_number: Some("0.1.0".to_string()),
            locale_language_iso6391: Some("en".to_string()),
            locale_country_iso31661_alpha2: Some("US".to_string()),
            ..Default::default()
        }),
        web_info: Some(WebInfo {
            web_sub_platform: Some(WebSubPlatform::WebBrowser as i32),
            ..Default::default()
        }),
        connect_type: Some(ConnectType::WifiUnknown as i32),
        connect_reason: Some(ConnectReason::UserActivated as i32),
        ..Default::default()
    }
}

/// Static `DeviceProps` template. Used by the registration flow as the
/// inner `device_props` bytes. Mirrors `DeviceProps`.
pub fn device_props() -> companion_reg::DeviceProps {
    use companion_reg::device_props::{AppVersion, HistorySyncConfig, PlatformType};

    companion_reg::DeviceProps {
        os: Some("whatsmeow".to_string()),
        version: Some(AppVersion {
            primary: Some(0),
            secondary: Some(1),
            tertiary: Some(0),
            quaternary: None,
            quinary: None,
        }),
        platform_type: Some(PlatformType::Unknown as i32),
        require_full_sync: Some(false),
        history_sync_config: Some(HistorySyncConfig {
            full_sync_days_limit: None,
            full_sync_size_mb_limit: None,
            storage_quota_mb: Some(10240),
            inline_initial_payload_in_e2_ee_msg: Some(true),
            recent_sync_days_limit: None,
            support_call_log_history: Some(false),
            support_bot_user_agent_chat_history: Some(true),
            support_cag_reactions_and_polls: Some(true),
            support_biz_hosted_msg: Some(true),
            support_recent_sync_chunk_message_count_tuning: Some(true),
            support_hosted_group_msg: Some(true),
            support_fbid_bot_chat_history: Some(true),
            support_add_on_history_sync_migration: None,
            support_message_association: Some(true),
            support_group_history: Some(true),
            on_demand_ready: None,
            support_guest_chat: None,
            complete_on_demand_ready: None,
            thumbnail_sync_days_limit: Some(60),
            initial_sync_max_messages_per_chat: None,
            support_manus_history: Some(true),
            support_hatch_history: Some(true),
            supported_bot_channel_fbids: Vec::new(),
            support_inline_contacts: None,
        }),
    }
}

// ---------------------------------------------------------------------------
// Flow-specific builders.
// ---------------------------------------------------------------------------

/// Build the registration-time payload. Used when the device has no JID yet
/// — we hand the server our keys, signed pre-key and version hash so it can
/// advertise the QR code that another logged-in WhatsApp client can scan.
/// Mirrors `getRegistrationPayload`.
pub fn build_registration_payload(device: &Device) -> wa6::ClientPayload {
    let mut payload = base_client_payload();

    // 4-byte big-endian registration id.
    let reg_id = device.registration_id.to_be_bytes().to_vec();

    // Upstream computes a 4-byte big-endian buffer from `SignedPreKey.KeyID`,
    // then takes the LAST three bytes (`preKeyID[1:]`). Mirror that.
    let prekey_id_be = device.signed_pre_key.key_id.to_be_bytes();
    let e_skey_id = prekey_id_be[1..].to_vec();

    // Marshal device props. The encoded buffer is small and the proto is
    // statically constructed, so encoding is infallible in practice.
    let mut device_props_bytes = Vec::new();
    device_props()
        .encode(&mut device_props_bytes)
        .expect("encoding statically constructed DeviceProps must not fail");

    // Hash the (live-fetched, falling back to compile-time) version once.
    let build_hash = crate::version::current().hash().to_vec();

    // Signature is required at this point in the upstream code; if we don't
    // have one yet we send 64 zero bytes (caller is expected to have signed
    // the pre-key, but matching the Go zero-fallback keeps panics out of
    // the hot path).
    let e_skey_sig = device
        .signed_pre_key
        .signature
        .map(|s| s.to_vec())
        .unwrap_or_else(|| vec![0u8; 64]);

    payload.device_pairing_data = Some(wa6::client_payload::DevicePairingRegistrationData {
        e_regid: Some(reg_id),
        // libsignal `ecc.DjbType` = 0x05.
        e_keytype: Some(vec![0x05]),
        e_ident: Some(device.identity_key.public.to_vec()),
        e_skey_id: Some(e_skey_id),
        e_skey_val: Some(device.signed_pre_key.key_pair.public.to_vec()),
        e_skey_sig: Some(e_skey_sig),
        build_hash: Some(build_hash),
        device_props: Some(device_props_bytes),
    });
    payload.passive = Some(false);
    payload.pull = Some(false);

    payload
}

/// Build the login-time payload. Used when the device already has a JID
/// (i.e. it was paired in a previous session). Mirrors `getLoginPayload`.
pub fn build_login_payload(device: &Device) -> wa6::ClientPayload {
    let mut payload = base_client_payload();
    let jid = device
        .id
        .as_ref()
        .expect("build_login_payload requires device.id to be Some");
    payload.username = Some(jid.user_int());
    payload.device = Some(jid.device as u32);
    payload.passive = Some(true);
    payload.pull = Some(true);
    payload.lid_db_migrated = Some(true);
    if payload.lc.is_none() {
        payload.lc = Some(1);
    }
    payload
}

/// Dispatch on whether the device has a JID. Mirrors `GetClientPayload`.
pub fn build_client_payload(device: &Device) -> wa6::ClientPayload {
    if device.id.is_some() {
        build_login_payload(device)
    } else {
        build_registration_payload(device)
    }
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use wha_crypto::{KeyPair, PreKey};
    use wha_store::{Device, MemoryStore};
    use wha_types::Jid;

    /// Build a Device backed by a fresh in-memory store and patched with the
    /// requested registration id, signed pre-key id and JID. We deliberately
    /// override the signed pre-key with a known id so the BE-encoded prefix
    /// is predictable.
    fn make_device(reg_id: u32, signed_prekey_id: u32, jid: Option<Jid>) -> Device {
        let store = Arc::new(MemoryStore::new());
        let mut device = store.new_device();
        let mut rng = rand::thread_rng();
        let identity = device.identity_key.clone();
        let signed = PreKey::new(signed_prekey_id, KeyPair::generate(&mut rng))
            .signed_by(&identity)
            .expect("sign");
        device.signed_pre_key = signed;
        device.registration_id = reg_id;
        device.id = jid;
        device
    }

    #[test]
    fn parse_version_round_trip() {
        let v = WAVersionContainer::parse("2.3000.1038187123").unwrap();
        assert_eq!(v, WA_VERSION);
        assert_eq!(v.to_dot_string(), "2.3000.1038187123");
    }

    #[test]
    fn parse_version_rejects_bad_input() {
        assert!(WAVersionContainer::parse("1.2").is_err());
        assert!(WAVersionContainer::parse("1.2.x").is_err());
    }

    #[test]
    fn version_hash_matches_md5_of_dot_string() {
        let mut h = Md5::new();
        h.update(WA_VERSION.to_dot_string().as_bytes());
        let want: [u8; 16] = h.finalize().into();
        assert_eq!(WA_VERSION.hash(), want);
    }

    #[test]
    fn base_client_payload_has_user_agent() {
        let p = base_client_payload();
        let ua = p.user_agent.as_ref().expect("user_agent must be set");
        assert_eq!(
            ua.platform,
            Some(wa6::client_payload::user_agent::Platform::Web as i32)
        );
        assert_eq!(ua.device.as_deref(), Some("Desktop"));
        assert_eq!(ua.locale_language_iso6391.as_deref(), Some("en"));
        assert_eq!(
            p.connect_type,
            Some(wa6::client_payload::ConnectType::WifiUnknown as i32)
        );
        assert_eq!(
            p.connect_reason,
            Some(wa6::client_payload::ConnectReason::UserActivated as i32)
        );
        let v = ua.app_version.as_ref().expect("app_version");
        assert_eq!(v.primary, Some(WA_VERSION.0[0]));
        assert_eq!(v.secondary, Some(WA_VERSION.0[1]));
        assert_eq!(v.tertiary, Some(WA_VERSION.0[2]));
    }

    #[test]
    fn build_registration_payload_carries_device_pairing_data() {
        let device = make_device(12345, 0x00aabbcc, None);
        let payload = build_registration_payload(&device);

        let dpd = payload
            .device_pairing_data
            .as_ref()
            .expect("device_pairing_data must be set on registration payload");
        let regid = dpd.e_regid.as_ref().expect("e_regid");
        assert_eq!(regid.len(), 4, "e_regid is 4 bytes BE");
        assert_eq!(regid, &12345u32.to_be_bytes().to_vec());

        // e_keytype is the libsignal DjbType prefix, single byte 0x05.
        assert_eq!(dpd.e_keytype.as_deref(), Some(&[0x05u8][..]));

        // e_ident is the 32-byte identity public key.
        assert_eq!(
            dpd.e_ident.as_ref().expect("e_ident").len(),
            32,
            "identity public key is 32 bytes"
        );

        // e_skey_id is the LAST three bytes of the BE-encoded prekey id (0x00aabbcc).
        assert_eq!(
            dpd.e_skey_id.as_deref(),
            Some(&[0xaau8, 0xbb, 0xcc][..]),
            "e_skey_id should be the lower 3 bytes of the BE id"
        );

        assert_eq!(dpd.e_skey_val.as_ref().expect("e_skey_val").len(), 32);
        assert_eq!(dpd.e_skey_sig.as_ref().expect("e_skey_sig").len(), 64);

        // build_hash is md5 of the version string => 16 bytes.
        assert_eq!(dpd.build_hash.as_ref().expect("build_hash").len(), 16);
        assert_eq!(dpd.build_hash.as_ref().unwrap(), &WA_VERSION.hash().to_vec());

        // device_props decodes back to a DeviceProps with our os string.
        let dp_bytes = dpd.device_props.as_ref().expect("device_props");
        let dp = companion_reg::DeviceProps::decode(dp_bytes.as_slice()).expect("decode");
        assert_eq!(dp.os.as_deref(), Some("whatsmeow"));

        assert_eq!(payload.passive, Some(false));
        assert_eq!(payload.pull, Some(false));
        // Login-only fields must be absent.
        assert!(payload.username.is_none());
        assert!(payload.device.is_none());
    }

    #[test]
    fn build_login_payload_uses_device_jid() {
        let jid = Jid::new_ad("12025550123", 0, 7);
        let device = make_device(0, 1, Some(jid.clone()));
        let payload = build_login_payload(&device);

        assert_eq!(payload.username, Some(jid.user_int()));
        assert_eq!(payload.device, Some(jid.device as u32));
        assert_eq!(payload.passive, Some(true));
        assert_eq!(payload.pull, Some(true));
        assert_eq!(payload.lid_db_migrated, Some(true));
        assert_eq!(payload.lc, Some(1));
        // No registration data on the login path.
        assert!(payload.device_pairing_data.is_none());
    }

    #[test]
    fn build_client_payload_dispatches_on_id() {
        // No id → registration path.
        let unpaired = make_device(99, 1, None);
        let p = build_client_payload(&unpaired);
        assert!(p.device_pairing_data.is_some(), "registration path");
        assert!(p.username.is_none());

        // Some(id) → login path.
        let jid = Jid::new_ad("447700900123", 0, 1);
        let paired = make_device(0, 1, Some(jid));
        let p = build_client_payload(&paired);
        assert!(p.device_pairing_data.is_none(), "login path");
        assert_eq!(p.passive, Some(true));
        assert_eq!(p.pull, Some(true));
        assert!(p.username.is_some());
    }
}
