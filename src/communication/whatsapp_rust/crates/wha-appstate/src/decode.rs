//! Decrypt + verify mutation patches and snapshots.
//! Mirrors `_upstream/whatsmeow/appstate/decode.go`.

use prost::Message;
use wha_crypto::{cbc_decrypt, hmac_sha256_concat};
use wha_proto::server_sync::{SyncdMutation, SyncdPatch, SyncdSnapshot};
use wha_proto::sync_action::SyncActionData;

use crate::errors::AppStateError;
use crate::hash::{
    ct_eq, generate_content_mac, generate_patch_mac, generate_snapshot_mac, update_hash, HashState,
    SyncdOperation, WaPatchName,
};
use crate::keys::ExpandedAppStateKeys;

/// One decoded mutation — the user-facing result of [`decode_patch`] /
/// [`decode_snapshot`]. Mirrors `appstate.Mutation` upstream.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedMutation {
    pub key_id: Vec<u8>,
    pub operation: SyncdOperation,
    /// JSON-encoded plaintext index, e.g. `["mute","jid@s.whatsapp.net"]`.
    pub index_raw: Vec<u8>,
    /// Parsed JSON segments of `index_raw`. Empty if parsing fails.
    pub index: Vec<String>,
    /// Decoded SyncActionData.
    pub action: SyncActionData,
    /// Mirror of upstream `Version` — the per-mutation version number stored
    /// in the SyncActionData (e.g. 2 for mute, 5 for pin).
    pub mutation_version: i32,
    pub index_mac: Vec<u8>,
    pub value_mac: Vec<u8>,
    pub patch_version: u64,
}

/// Try to parse a JSON-encoded index plaintext (a flat array of strings) into
/// a `Vec<String>`. Tolerates malformed input by returning an empty vec —
/// the high-level dispatcher will then ignore the mutation.
fn parse_index(plaintext: &[u8]) -> Vec<String> {
    let s = match std::str::from_utf8(plaintext) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let s = s.trim();
    if !s.starts_with('[') || !s.ends_with(']') {
        return Vec::new();
    }
    // The whatsmeow indices are simple JSON string arrays. We avoid pulling
    // serde_json into this crate by hand-parsing the limited form.
    let inner = &s[1..s.len() - 1];
    let mut out = Vec::new();
    let mut chars = inner.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() || c == ',' {
            chars.next();
            continue;
        }
        if c != '"' {
            return Vec::new();
        }
        chars.next(); // consume opening "
        let mut buf = String::new();
        loop {
            match chars.next() {
                Some('\\') => {
                    if let Some(esc) = chars.next() {
                        match esc {
                            '"' | '\\' | '/' => buf.push(esc),
                            'n' => buf.push('\n'),
                            't' => buf.push('\t'),
                            'r' => buf.push('\r'),
                            'b' => buf.push('\u{08}'),
                            'f' => buf.push('\u{0c}'),
                            other => {
                                buf.push('\\');
                                buf.push(other);
                            }
                        }
                    }
                }
                Some('"') => break,
                Some(ch) => buf.push(ch),
                None => return Vec::new(),
            }
        }
        out.push(buf);
    }
    out
}

/// Decode a single mutation: verify its content-MAC, AES-CBC-decrypt the
/// payload, optionally verify the index-MAC, and parse the action protobuf.
pub fn decode_mutation(
    mutation: &SyncdMutation,
    keys: &ExpandedAppStateKeys,
    validate_macs: bool,
    patch_version: u64,
) -> Result<DecodedMutation, AppStateError> {
    let record = mutation
        .record
        .as_ref()
        .ok_or(AppStateError::Protobuf("mutation missing record".into()))?;
    let key_id = record
        .key_id
        .as_ref()
        .and_then(|k| k.id.as_deref())
        .ok_or(AppStateError::Protobuf("mutation missing key_id".into()))?;

    let blob = record
        .value
        .as_ref()
        .and_then(|v| v.blob.as_deref())
        .ok_or(AppStateError::MalformedValueBlob)?;
    if blob.len() < 16 + 32 {
        return Err(AppStateError::MalformedValueBlob);
    }

    let (content, value_mac) = blob.split_at(blob.len() - 32);
    let op = SyncdOperation::from_i32(mutation.operation.unwrap_or(0))?;

    if validate_macs {
        let expected = generate_content_mac(op, content, key_id, &keys.value_mac);
        if !ct_eq(&expected, value_mac) {
            return Err(AppStateError::MismatchedContentMac);
        }
    }

    let (iv, ciphertext) = content.split_at(16);
    let plaintext = cbc_decrypt(&keys.value_encryption, iv, ciphertext)?;

    let action = SyncActionData::decode(plaintext.as_slice())?;
    let index_plaintext = action.index.clone().unwrap_or_default();

    let index_mac = record
        .index
        .as_ref()
        .and_then(|i| i.blob.clone())
        .ok_or(AppStateError::Protobuf("mutation missing index blob".into()))?;

    if validate_macs {
        let expected_index_mac = hmac_sha256_concat(&keys.index, &[&index_plaintext]);
        if !ct_eq(&expected_index_mac, &index_mac) {
            return Err(AppStateError::MismatchedIndexMac);
        }
    }

    let parsed = parse_index(&index_plaintext);
    let mutation_version = action.version.unwrap_or(0);

    Ok(DecodedMutation {
        key_id: key_id.to_vec(),
        operation: op,
        index_raw: index_plaintext,
        index: parsed,
        action,
        mutation_version,
        index_mac,
        value_mac: value_mac.to_vec(),
        patch_version,
    })
}

/// Decode a snapshot: verify the LTHash → snapshot-MAC chain, then decrypt all
/// records inside as `SET` mutations. Updates `state` in place to the
/// snapshot's version + computed hash.
pub fn decode_snapshot(
    snapshot: &SyncdSnapshot,
    name: WaPatchName,
    state: &mut HashState,
    keys: &ExpandedAppStateKeys,
    validate_macs: bool,
) -> Result<Vec<DecodedMutation>, AppStateError> {
    let new_version = snapshot
        .version
        .as_ref()
        .and_then(|v| v.version)
        .unwrap_or(state.version);

    let mut working_state = HashState {
        version: new_version,
        hash: state.hash,
    };

    // Treat each record as a SyncdMutation{op=SET, record=record} for hashing.
    let encrypted: Vec<SyncdMutation> = snapshot
        .records
        .iter()
        .map(|r| SyncdMutation {
            operation: Some(SyncdOperation::Set.as_i32()),
            record: Some(r.clone()),
        })
        .collect();

    update_hash(&mut working_state, &encrypted, |_im, _i| None)?;

    if validate_macs {
        let expected_snapshot_mac =
            generate_snapshot_mac(&working_state, name, &keys.snapshot_mac);
        let got = snapshot.mac.as_deref().unwrap_or_default();
        if !ct_eq(&expected_snapshot_mac, got) {
            return Err(AppStateError::MismatchedLtHash);
        }
    }

    let mut out = Vec::with_capacity(encrypted.len());
    for m in &encrypted {
        out.push(decode_mutation(m, keys, validate_macs, new_version)?);
    }

    *state = working_state;
    Ok(out)
}

/// Decode a `SyncdPatch`: verify all per-mutation content-MACs, decrypt each,
/// update the LTHash state, and finally verify the snapshot+patch MAC.
///
/// `prev_value_mac` returns the previous SET's value-MAC for an `index_mac`
/// — used to subtract the old value out of the LTHash on `SET`/`REMOVE`. It
/// is consulted only AFTER the in-patch scan fails.
pub fn decode_patch<F>(
    patch: &SyncdPatch,
    name: WaPatchName,
    current_state: &mut HashState,
    keys: &ExpandedAppStateKeys,
    validate_macs: bool,
    mut prev_value_mac: F,
) -> Result<Vec<DecodedMutation>, AppStateError>
where
    F: FnMut(&[u8]) -> Option<Vec<u8>>,
{
    let new_version = patch
        .version
        .as_ref()
        .and_then(|v| v.version)
        .unwrap_or(current_state.version);

    let mut working_state = HashState {
        version: new_version,
        hash: current_state.hash,
    };

    update_hash(&mut working_state, &patch.mutations, |im, max_idx| {
        // First scan within this patch for an earlier SET with the same index_mac.
        for i in (0..max_idx).rev() {
            let pm = &patch.mutations[i];
            let pm_im = pm
                .record
                .as_ref()
                .and_then(|r| r.index.as_ref())
                .and_then(|i| i.blob.as_deref())
                .unwrap_or_default();
            if ct_eq(pm_im, im) {
                let pm_op = SyncdOperation::from_i32(pm.operation.unwrap_or(0)).ok()?;
                if pm_op == SyncdOperation::Set {
                    let blob = pm
                        .record
                        .as_ref()
                        .and_then(|r| r.value.as_ref())
                        .and_then(|v| v.blob.as_deref())
                        .unwrap_or_default();
                    if blob.len() >= 32 {
                        return Some(blob[blob.len() - 32..].to_vec());
                    }
                }
                return None;
            }
        }
        prev_value_mac(im)
    })?;

    if validate_macs {
        let expected_snapshot = generate_snapshot_mac(&working_state, name, &keys.snapshot_mac);
        let got = patch.snapshot_mac.as_deref().unwrap_or_default();
        if !ct_eq(&expected_snapshot, got) {
            return Err(AppStateError::MismatchedLtHash);
        }
        let expected_patch = generate_patch_mac(patch, name, &keys.patch_mac, new_version);
        let got = patch.patch_mac.as_deref().unwrap_or_default();
        if !ct_eq(&expected_patch, got) {
            return Err(AppStateError::MismatchedPatchMac);
        }
    }

    let mut out = Vec::with_capacity(patch.mutations.len());
    for m in &patch.mutations {
        out.push(decode_mutation(m, keys, validate_macs, new_version)?);
    }

    *current_state = working_state;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_index_handles_basic_array() {
        let raw = br#"["mute","1234@s.whatsapp.net"]"#;
        let parsed = parse_index(raw);
        assert_eq!(parsed, vec!["mute".to_string(), "1234@s.whatsapp.net".to_string()]);
    }

    #[test]
    fn parse_index_returns_empty_on_malformed() {
        assert!(parse_index(b"not json").is_empty());
        assert!(parse_index(b"{\"a\":1}").is_empty());
        assert!(parse_index(b"[unquoted]").is_empty());
    }

    #[test]
    fn parse_index_handles_escapes() {
        let raw = br#"["a\"b","c\\d"]"#;
        let parsed = parse_index(raw);
        assert_eq!(parsed, vec!["a\"b".to_string(), "c\\d".to_string()]);
    }
}
