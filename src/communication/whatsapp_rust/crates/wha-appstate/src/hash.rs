//! State hash + the three MAC functions (snapshot, patch, content).
//! Mirrors `_upstream/whatsmeow/appstate/hash.go`.

use std::convert::TryInto;

use wha_crypto::{hmac_sha256_concat, hmac_sha512_concat};
use wha_proto::server_sync::SyncdPatch;

use crate::errors::AppStateError;
use crate::lthash::WA_PATCH_INTEGRITY;

/// All currently-known app-state collection names. Each name is mixed into
/// the snapshot/patch MACs so cross-collection replay is rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WaPatchName {
    CriticalBlock,
    CriticalUnblockLow,
    RegularLow,
    RegularHigh,
    Regular,
}

impl WaPatchName {
    pub const ALL: [WaPatchName; 5] = [
        WaPatchName::CriticalBlock,
        WaPatchName::CriticalUnblockLow,
        WaPatchName::RegularHigh,
        WaPatchName::Regular,
        WaPatchName::RegularLow,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            WaPatchName::CriticalBlock => "critical_block",
            WaPatchName::CriticalUnblockLow => "critical_unblock_low",
            WaPatchName::RegularLow => "regular_low",
            WaPatchName::RegularHigh => "regular_high",
            WaPatchName::Regular => "regular",
        }
    }

    pub fn parse(s: &str) -> Result<Self, AppStateError> {
        match s {
            "critical_block" => Ok(WaPatchName::CriticalBlock),
            "critical_unblock_low" => Ok(WaPatchName::CriticalUnblockLow),
            "regular_low" => Ok(WaPatchName::RegularLow),
            "regular_high" => Ok(WaPatchName::RegularHigh),
            "regular" => Ok(WaPatchName::Regular),
            other => Err(AppStateError::UnknownPatchName(other.to_string())),
        }
    }
}

/// Mirrors `SyncdMutation_SyncdOperation`. Hand-defined so this module doesn't
/// have to expose prost's `i32` representation as a public API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncdOperation {
    Set = 0,
    Remove = 1,
}

impl SyncdOperation {
    pub fn from_i32(v: i32) -> Result<Self, AppStateError> {
        match v {
            0 => Ok(SyncdOperation::Set),
            1 => Ok(SyncdOperation::Remove),
            other => Err(AppStateError::Protobuf(format!(
                "unknown SyncdOperation: {other}"
            ))),
        }
    }

    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

/// LTHash state — 128-byte buffer + monotonic version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HashState {
    pub version: u64,
    pub hash: [u8; 128],
}

impl Default for HashState {
    fn default() -> Self {
        HashState { version: 0, hash: [0u8; 128] }
    }
}

fn u64_be(v: u64) -> [u8; 8] {
    v.to_be_bytes()
}

/// `HMAC-SHA256(snapshot_mac_key, hash || version_be || patch_name)`.
/// Mirrors `(*HashState).generateSnapshotMAC`.
pub fn generate_snapshot_mac(
    state: &HashState,
    name: WaPatchName,
    snapshot_mac_key: &[u8],
) -> [u8; 32] {
    let v = u64_be(state.version);
    let mac = hmac_sha256_concat(
        snapshot_mac_key,
        &[&state.hash, &v, name.as_str().as_bytes()],
    );
    mac.try_into().expect("hmac-sha256 = 32 bytes")
}

/// `HMAC-SHA256(patch_mac_key, snapshot_mac || …value_macs || version_be || patch_name)`.
/// Mirrors `generatePatchMAC`.
pub fn generate_patch_mac(
    patch: &SyncdPatch,
    name: WaPatchName,
    patch_mac_key: &[u8],
    version: u64,
) -> [u8; 32] {
    let v = u64_be(version);
    let snapshot_mac = patch.snapshot_mac.as_deref().unwrap_or(&[]);
    let value_macs: Vec<&[u8]> = patch
        .mutations
        .iter()
        .filter_map(|m| {
            let blob = m.record.as_ref()?.value.as_ref()?.blob.as_deref()?;
            if blob.len() < 32 {
                return None;
            }
            Some(&blob[blob.len() - 32..])
        })
        .collect();
    let name_bytes = name.as_str().as_bytes();

    let mut parts: Vec<&[u8]> = Vec::with_capacity(value_macs.len() + 3);
    parts.push(snapshot_mac);
    for vm in &value_macs {
        parts.push(vm);
    }
    parts.push(&v);
    parts.push(name_bytes);

    let mac = hmac_sha256_concat(patch_mac_key, &parts);
    mac.try_into().expect("hmac-sha256 = 32 bytes")
}

/// `HMAC-SHA512(value_mac_key, [op+1] || keyID || data || u64_be(len(keyID)+1))[..32]`.
/// Mirrors `generateContentMAC`.
pub fn generate_content_mac(
    op: SyncdOperation,
    data: &[u8],
    key_id: &[u8],
    value_mac_key: &[u8],
) -> [u8; 32] {
    let op_byte = [op.as_i32() as u8 + 1];
    let key_data_len = u64_be((key_id.len() as u64) + 1);
    let mac = hmac_sha512_concat(value_mac_key, &[&op_byte, key_id, data, &key_data_len]);
    let mut out = [0u8; 32];
    out.copy_from_slice(&mac[..32]);
    out
}

/// Walk a sequence of mutations and apply LTHash add/subtract in place.
/// Mirrors `(*HashState).updateHash` upstream — "missing previous SET" is
/// surfaced as a soft warning (returned `Vec`) rather than a hard error,
/// matching upstream's tolerance for self-referential REMOVE ops.
pub fn update_hash(
    state: &mut HashState,
    mutations: &[wha_proto::server_sync::SyncdMutation],
    mut prev_value_mac: impl FnMut(&[u8], usize) -> Option<Vec<u8>>,
) -> Result<Vec<AppStateError>, AppStateError> {
    let mut added: Vec<Vec<u8>> = Vec::new();
    let mut removed: Vec<Vec<u8>> = Vec::new();
    let mut warnings: Vec<AppStateError> = Vec::new();

    for (i, mutation) in mutations.iter().enumerate() {
        let op = SyncdOperation::from_i32(mutation.operation.unwrap_or(0))?;
        if op == SyncdOperation::Set {
            if let Some(blob) = mutation
                .record
                .as_ref()
                .and_then(|r| r.value.as_ref())
                .and_then(|v| v.blob.as_deref())
            {
                if blob.len() >= 32 {
                    added.push(blob[blob.len() - 32..].to_vec());
                }
            }
        }
        let index_mac_owned = mutation
            .record
            .as_ref()
            .and_then(|r| r.index.as_ref())
            .and_then(|i| i.blob.clone())
            .unwrap_or_default();
        match prev_value_mac(&index_mac_owned, i) {
            Some(prev) => removed.push(prev),
            None if op == SyncdOperation::Remove => {
                warnings.push(AppStateError::MissingPreviousSetValueOperation);
            }
            None => {}
        }
    }

    let removed_refs: Vec<&[u8]> = removed.iter().map(|v| v.as_slice()).collect();
    let added_refs: Vec<&[u8]> = added.iter().map(|v| v.as_slice()).collect();
    WA_PATCH_INTEGRITY.subtract_then_add_in_place(&mut state.hash, &removed_refs, &added_refs);
    Ok(warnings)
}

/// Constant-time byte-slice comparison, used wherever we compare MACs.
pub(crate) fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_mac_changes_with_version() {
        let key = [9u8; 32];
        let mut s = HashState::default();
        let m1 = generate_snapshot_mac(&s, WaPatchName::Regular, &key);
        s.version = 1;
        let m2 = generate_snapshot_mac(&s, WaPatchName::Regular, &key);
        assert_ne!(m1, m2);
    }

    #[test]
    fn snapshot_mac_changes_with_name() {
        let key = [9u8; 32];
        let s = HashState::default();
        let a = generate_snapshot_mac(&s, WaPatchName::Regular, &key);
        let b = generate_snapshot_mac(&s, WaPatchName::CriticalBlock, &key);
        assert_ne!(a, b);
    }

    #[test]
    fn content_mac_distinguishes_set_from_remove() {
        let key = [3u8; 32];
        let key_id = b"id";
        let data = b"data";
        let set = generate_content_mac(SyncdOperation::Set, data, key_id, &key);
        let rem = generate_content_mac(SyncdOperation::Remove, data, key_id, &key);
        assert_ne!(set, rem);
    }

    #[test]
    fn syncd_operation_round_trip() {
        for op in [SyncdOperation::Set, SyncdOperation::Remove] {
            assert_eq!(SyncdOperation::from_i32(op.as_i32()).unwrap(), op);
        }
        assert!(SyncdOperation::from_i32(99).is_err());
    }
}
