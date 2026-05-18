//! HKDF expansion of a 32-byte app-state sync key into the 5 sub-keys used
//! across the rest of the module. Mirrors
//! `_upstream/whatsmeow/appstate/keys.go::expandAppStateKeys`.

use std::convert::TryInto;

use wha_crypto::hkdf_sha256;

use crate::errors::AppStateError;

/// HKDF info string for app-state sub-keys. Treated as authenticated data —
/// must match upstream byte-for-byte or every MAC fails.
pub const HKDF_INFO: &[u8] = b"WhatsApp Mutation Keys";

/// The 5 32-byte sub-keys derived from a 32-byte app-state sync key.
///
/// Layout matches upstream:
///
/// ```text
///   0..32   index
///  32..64   value_encryption
///  64..96   value_mac
///  96..128  snapshot_mac
/// 128..160  patch_mac
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpandedAppStateKeys {
    pub index: [u8; 32],
    pub value_encryption: [u8; 32],
    pub value_mac: [u8; 32],
    pub snapshot_mac: [u8; 32],
    pub patch_mac: [u8; 32],
}

/// Expand a sync key via `HKDF-SHA256(key, salt=∅, info=HKDF_INFO, L=160)`.
pub fn expand_app_state_keys(key_data: &[u8]) -> Result<ExpandedAppStateKeys, AppStateError> {
    let expanded = hkdf_sha256(key_data, &[], HKDF_INFO, 160)
        .map_err(|e| AppStateError::Hkdf(e.to_string()))?;
    let take = |range: std::ops::Range<usize>| -> [u8; 32] {
        expanded[range].try_into().expect("slice length 32")
    };
    Ok(ExpandedAppStateKeys {
        index: take(0..32),
        value_encryption: take(32..64),
        value_mac: take(64..96),
        snapshot_mac: take(96..128),
        patch_mac: take(128..160),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lays_out_5_subkeys_contiguously() {
        let sync_key = [0x42u8; 32];
        let expanded = expand_app_state_keys(&sync_key).unwrap();
        let raw = hkdf_sha256(&sync_key, &[], HKDF_INFO, 160).unwrap();
        assert_eq!(expanded.index, raw[0..32]);
        assert_eq!(expanded.value_encryption, raw[32..64]);
        assert_eq!(expanded.value_mac, raw[64..96]);
        assert_eq!(expanded.snapshot_mac, raw[96..128]);
        assert_eq!(expanded.patch_mac, raw[128..160]);
    }

    #[test]
    fn distinct_inputs_yield_distinct_keys() {
        let a = expand_app_state_keys(&[1u8; 32]).unwrap();
        let b = expand_app_state_keys(&[2u8; 32]).unwrap();
        assert_ne!(a, b);
    }
}
