//! Encode a list of plaintext mutations into a wire-format `SyncdPatch`.
//! Mirrors `_upstream/whatsmeow/appstate/encode.go::EncodePatch`.
//!
//! In addition to the low-level [`encode_patch`] kernel, this module also
//! ships the high-level mutation builders that the whatsmeow `Build*`
//! helpers expose — [`build_mark_read_mutation`], [`build_mute_chat_mutation`],
//! [`build_pin_chat_mutation`], [`build_archive_chat_mutation`], and
//! [`build_star_message_mutation`]. Each one emits a [`MutationInput`] tagged
//! with the right patch-name + version, ready to feed into [`encode_patch`].

use prost::Message;
use rand::RngCore;
use wha_crypto::{cbc_encrypt, hmac_sha256_concat};
use wha_proto::common::MessageKey;
use wha_proto::server_sync::{
    KeyId, SyncdIndex, SyncdMutation, SyncdPatch, SyncdRecord, SyncdValue, SyncdVersion,
};
use wha_proto::sync_action::{
    ArchiveChatAction, MarkChatAsReadAction, MuteAction, PinAction, StarAction, SyncActionData,
    SyncActionMessage, SyncActionMessageRange, SyncActionValue,
};
use wha_types::Jid;

use crate::errors::AppStateError;
use crate::hash::{
    generate_content_mac, generate_patch_mac, generate_snapshot_mac, update_hash, HashState,
    SyncdOperation, WaPatchName,
};
use crate::keys::ExpandedAppStateKeys;

/// One plaintext mutation, the input to [`encode_patch`].
#[derive(Clone, Debug)]
pub struct MutationInput {
    pub operation: SyncdOperation,
    /// Plaintext index — typically a small JSON array like `["mute", "<jid>"]`.
    /// Will be HMAC'd to produce the on-wire index_mac.
    pub index_plaintext: Vec<u8>,
    /// The action protobuf to be encrypted.
    pub action: SyncActionValue,
    /// Per-mutation version (e.g. 2 for mute, 5 for pin).
    pub mutation_version: i32,
    /// 16-byte AES-CBC IV. Caller may randomise; tests often use a fixed value.
    pub iv: [u8; 16],
}

/// Encrypt one mutation and produce a wire-format `SyncdMutation`. The
/// resulting `value` blob is `iv || ciphertext || value_mac` (the mac is
/// HMAC-SHA512 truncated to 32 bytes); the `index` blob is
/// `HMAC-SHA256(index_key, index_plaintext)`.
pub fn encode_mutation(
    operation: SyncdOperation,
    plaintext_value: &[u8],
    index_plaintext: &[u8],
    iv: &[u8; 16],
    key_id: &[u8],
    keys: &ExpandedAppStateKeys,
) -> Result<SyncdMutation, AppStateError> {
    let ciphertext = cbc_encrypt(&keys.value_encryption, iv, plaintext_value)?;
    let mut content = Vec::with_capacity(16 + ciphertext.len());
    content.extend_from_slice(iv);
    content.extend_from_slice(&ciphertext);

    let value_mac = generate_content_mac(operation, &content, key_id, &keys.value_mac);

    let mut blob = content;
    blob.extend_from_slice(&value_mac);

    let index_mac = hmac_sha256_concat(&keys.index, &[index_plaintext]);

    Ok(SyncdMutation {
        operation: Some(operation.as_i32()),
        record: Some(SyncdRecord {
            index: Some(SyncdIndex { blob: Some(index_mac) }),
            value: Some(SyncdValue { blob: Some(blob) }),
            key_id: Some(KeyId { id: Some(key_id.to_vec()) }),
        }),
    })
}

/// Encode a patch with N input mutations. Mirrors `(*Processor).EncodePatch`:
/// 1. for each input mutation, marshal `SyncActionData{index, value, version}`
///    into a protobuf and encrypt + MAC it;
/// 2. update the LTHash state for the resulting set of additions/removals;
/// 3. bump the version, generate snapshot+patch MACs.
pub fn encode_patch<F>(
    name: WaPatchName,
    key_id: &[u8],
    keys: &ExpandedAppStateKeys,
    state: &mut HashState,
    inputs: &[MutationInput],
    mut prev_value_mac: F,
) -> Result<SyncdPatch, AppStateError>
where
    F: FnMut(&[u8]) -> Option<Vec<u8>>,
{
    let mut mutations = Vec::with_capacity(inputs.len());
    for input in inputs {
        let action_data = SyncActionData {
            index: Some(input.index_plaintext.clone()),
            value: Some(input.action.clone()),
            padding: Some(Vec::new()),
            version: Some(input.mutation_version),
        };
        let mut plaintext = Vec::with_capacity(action_data.encoded_len());
        action_data.encode(&mut plaintext)?;

        let m = encode_mutation(
            input.operation,
            &plaintext,
            &input.index_plaintext,
            &input.iv,
            key_id,
            keys,
        )?;
        mutations.push(m);
    }

    update_hash(state, &mutations, |im, _i| prev_value_mac(im))?;

    state.version += 1;
    let snapshot_mac = generate_snapshot_mac(state, name, &keys.snapshot_mac).to_vec();

    let mut patch = SyncdPatch {
        version: Some(SyncdVersion { version: Some(state.version) }),
        mutations,
        external_mutations: None,
        snapshot_mac: Some(snapshot_mac),
        patch_mac: None,
        key_id: Some(KeyId { id: Some(key_id.to_vec()) }),
        exit_code: None,
        device_index: None,
        client_debug_data: None,
    };
    let patch_mac = generate_patch_mac(&patch, name, &keys.patch_mac, state.version).to_vec();
    patch.patch_mac = Some(patch_mac);

    Ok(patch)
}

// ---------------------------------------------------------------------------
// Mutation builders. Mirrors the `Build*` helpers in
// `_upstream/whatsmeow/appstate/encode.go`. Every builder owns the wire
// invariants for one app-state action: the index, the per-mutation version,
// the action proto, and the operation. Callers feed the resulting
// [`MutationInput`] (with a fresh random IV) into [`encode_patch`].
// ---------------------------------------------------------------------------

/// Index identifiers for the actions exposed below. Mirrors the
/// `IndexMute` / `IndexPin` / … constants in
/// `_upstream/whatsmeow/appstate/keys.go`.
pub mod index_id {
    pub const MUTE: &str = "mute";
    pub const PIN: &str = "pin_v1";
    pub const ARCHIVE: &str = "archive";
    pub const MARK_CHAT_AS_READ: &str = "markChatAsRead";
    pub const STAR: &str = "star";
}

/// Encode a `[String]` index into the JSON shape WhatsApp uses for index
/// plaintexts (the encoded bytes are then HMAC'd to produce the on-wire
/// `index_mac`). Hand-rolled to avoid pulling `serde_json` in for a 20-line
/// function — the inputs are always `String`, never numbers, and the format
/// is `["a","b",…]` with `\` and `"` escaped.
pub fn json_encode_index(parts: &[&str]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + parts.len() * 16);
    out.push(b'[');
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            out.push(b',');
        }
        out.push(b'"');
        for &b in p.as_bytes() {
            match b {
                b'"' => out.extend_from_slice(b"\\\""),
                b'\\' => out.extend_from_slice(b"\\\\"),
                b'\n' => out.extend_from_slice(b"\\n"),
                b'\r' => out.extend_from_slice(b"\\r"),
                b'\t' => out.extend_from_slice(b"\\t"),
                _ => out.push(b),
            }
        }
        out.push(b'"');
    }
    out.push(b']');
    out
}

fn random_iv() -> [u8; 16] {
    let mut iv = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut iv);
    iv
}

fn cloned_message_key(k: &MessageKey) -> MessageKey {
    MessageKey {
        remote_jid: k.remote_jid.clone(),
        from_me: k.from_me,
        id: k.id.clone(),
        participant: k.participant.clone(),
    }
}

fn message_range_for(last_msg_key: &MessageKey, last_ts: i64) -> SyncActionMessageRange {
    SyncActionMessageRange {
        last_message_timestamp: Some(last_ts),
        last_system_message_timestamp: None,
        messages: vec![SyncActionMessage {
            key: Some(cloned_message_key(last_msg_key)),
            timestamp: Some(last_ts),
        }],
    }
}

/// Marshal a list of `MessageKey`s into a `SyncActionMessageRange`. Mirrors
/// the upstream helper that synthesizes the range from a single "last
/// message" key (no timestamp on the keys themselves — only on the
/// envelope).
fn message_range_from_keys(keys: &[MessageKey]) -> SyncActionMessageRange {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let messages = keys
        .iter()
        .map(|k| SyncActionMessage {
            key: Some(cloned_message_key(k)),
            timestamp: Some(now),
        })
        .collect();
    SyncActionMessageRange {
        last_message_timestamp: Some(now),
        last_system_message_timestamp: None,
        messages,
    }
}

/// Build a `markChatAsRead` mutation. Mirrors `BuildMarkChatAsRead` upstream.
/// The list of `message_keys` populates the `SyncActionMessageRange`. Pass a
/// 1-element slice with the latest message in the chat for the typical
/// "mark unread up to this message" semantics.
pub fn build_mark_read_mutation(
    message_keys: &[MessageKey],
    chat: &Jid,
    read: bool,
) -> MutationInput {
    let action = SyncActionValue {
        mark_chat_as_read_action: Some(MarkChatAsReadAction {
            read: Some(read),
            message_range: Some(message_range_from_keys(message_keys)),
        }),
        ..Default::default()
    };
    let chat_str = chat.to_string();
    let index_plaintext = json_encode_index(&[index_id::MARK_CHAT_AS_READ, &chat_str]);
    MutationInput {
        operation: SyncdOperation::Set,
        index_plaintext,
        action,
        mutation_version: 3,
        iv: random_iv(),
    }
}

/// Build a `mute` mutation. Mirrors `BuildMuteAbs`. `mute_until = None`
/// disables muting; `Some(0)` means "muted forever" (the upstream helper
/// substitutes `-1` in that case, which is what we do here too); any
/// positive value is a UnixMilli expiry.
pub fn build_mute_chat_mutation(chat: &Jid, mute_until: Option<i64>) -> MutationInput {
    let muted = mute_until.is_some();
    // Upstream replaces a missing-end-timestamp with -1 when the chat is
    // being muted (see `BuildMuteAbs` — `if muteEndTimestamp == nil && mute`).
    let end = match mute_until {
        Some(t) if t == 0 => Some(-1),
        Some(t) => Some(t),
        None => None,
    };
    let action = SyncActionValue {
        mute_action: Some(MuteAction {
            muted: Some(muted),
            mute_end_timestamp: end,
            auto_muted: None,
            mute_everyone_mention_end_timestamp: None,
        }),
        ..Default::default()
    };
    let chat_str = chat.to_string();
    let index_plaintext = json_encode_index(&[index_id::MUTE, &chat_str]);
    MutationInput {
        operation: SyncdOperation::Set,
        index_plaintext,
        action,
        mutation_version: 2,
        iv: random_iv(),
    }
}

/// Build a `pin_v1` mutation. Mirrors `BuildPin`.
pub fn build_pin_chat_mutation(chat: &Jid, pinned: bool) -> MutationInput {
    let action = SyncActionValue {
        pin_action: Some(PinAction {
            pinned: Some(pinned),
        }),
        ..Default::default()
    };
    let chat_str = chat.to_string();
    let index_plaintext = json_encode_index(&[index_id::PIN, &chat_str]);
    MutationInput {
        operation: SyncdOperation::Set,
        index_plaintext,
        action,
        mutation_version: 5,
        iv: random_iv(),
    }
}

/// Build an `archive` mutation. Mirrors `BuildArchive`. Note that upstream
/// emits a 2-mutation patch when `archive=true` (archive + auto-unpin); this
/// function only emits the archive one — the caller can pair it with a
/// [`build_pin_chat_mutation`] call to mirror the full upstream behaviour.
pub fn build_archive_chat_mutation(
    chat: &Jid,
    archived: bool,
    last_msg_key: &MessageKey,
) -> MutationInput {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let action = SyncActionValue {
        archive_chat_action: Some(ArchiveChatAction {
            archived: Some(archived),
            message_range: Some(message_range_for(last_msg_key, now)),
        }),
        ..Default::default()
    };
    let chat_str = chat.to_string();
    let index_plaintext = json_encode_index(&[index_id::ARCHIVE, &chat_str]);
    MutationInput {
        operation: SyncdOperation::Set,
        index_plaintext,
        action,
        mutation_version: 3,
        iv: random_iv(),
    }
}

/// Build a `star` mutation. Mirrors `BuildStar`. The index has 5 string
/// components: `["star", target_jid, message_id, from_me_str, sender_jid]`.
/// The `from_me_str` is `"1"` when our own JID matches `MessageKey.from_me`,
/// else `"0"`. When the target chat is a 1:1 DM the upstream code substitutes
/// the literal string `"0"` for the sender JID — we mirror that here.
pub fn build_star_message_mutation(message_key: &MessageKey, starred: bool) -> MutationInput {
    let from_me_str = if message_key.from_me.unwrap_or(false) {
        "1"
    } else {
        "0"
    };
    let target_jid = message_key
        .remote_jid
        .clone()
        .unwrap_or_else(|| "0".to_owned());
    let sender_jid = match (
        message_key.participant.as_deref(),
        message_key.remote_jid.as_deref(),
    ) {
        (Some(p), Some(r)) if user_part(p) != user_part(r) => p.to_owned(),
        // 1:1 DM (target.User == sender.User) — upstream's
        // `if target.User == sender.User { senderJID = "0" }`.
        _ => "0".to_owned(),
    };
    let message_id = message_key.id.clone().unwrap_or_default();
    let action = SyncActionValue {
        star_action: Some(StarAction {
            starred: Some(starred),
        }),
        ..Default::default()
    };
    let index_plaintext = json_encode_index(&[
        index_id::STAR,
        &target_jid,
        &message_id,
        from_me_str,
        &sender_jid,
    ]);
    MutationInput {
        operation: SyncdOperation::Set,
        index_plaintext,
        action,
        mutation_version: 2,
        iv: random_iv(),
    }
}

/// Best-effort "user part of a JID-string". Used by [`build_star_message_mutation`]
/// to compare the chat's user with the participant's user without going
/// through full JID parsing — the inputs already came from `MessageKey`
/// fields the wire side gave us. Grabs everything before `@` (or the whole
/// string when there's no `@`).
fn user_part(jid: &str) -> &str {
    match jid.find('@') {
        Some(i) => &jid[..i],
        None => jid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{decode_mutation, decode_patch};
    use crate::keys::expand_app_state_keys;
    use wha_proto::sync_action::{MuteAction, SyncActionValue};

    fn make_input() -> (ExpandedAppStateKeys, Vec<u8>, MutationInput) {
        let sync_key = [0xCDu8; 32];
        let keys = expand_app_state_keys(&sync_key).unwrap();
        let key_id = b"app-state-key-1".to_vec();
        let action = SyncActionValue {
            mute_action: Some(MuteAction {
                muted: Some(true),
                mute_end_timestamp: Some(-1),
                auto_muted: None,
                mute_everyone_mention_end_timestamp: None,
            }),
            ..Default::default()
        };
        let input = MutationInput {
            operation: SyncdOperation::Set,
            index_plaintext: br#"["mute","1@s.whatsapp.net"]"#.to_vec(),
            action,
            mutation_version: 2,
            iv: [0xABu8; 16],
        };
        (keys, key_id, input)
    }

    #[test]
    fn encode_then_decode_single_mutation() {
        let (keys, key_id, input) = make_input();
        let mut state = HashState::default();
        let patch = encode_patch(
            WaPatchName::RegularHigh,
            &key_id,
            &keys,
            &mut state,
            std::slice::from_ref(&input),
            |_| None,
        )
        .unwrap();
        assert_eq!(state.version, 1);

        let mut decode_state = HashState::default();
        let mutations = decode_patch(
            &patch,
            WaPatchName::RegularHigh,
            &mut decode_state,
            &keys,
            true,
            |_| None,
        )
        .unwrap();
        assert_eq!(mutations.len(), 1);
        assert_eq!(mutations[0].operation, SyncdOperation::Set);
        assert_eq!(mutations[0].index, vec!["mute", "1@s.whatsapp.net"]);
        assert_eq!(mutations[0].mutation_version, 2);
        assert_eq!(decode_state, state);
    }

    #[test]
    fn tampered_value_mac_is_detected() {
        let (keys, key_id, input) = make_input();
        let mut state = HashState::default();
        let mut patch = encode_patch(
            WaPatchName::RegularHigh,
            &key_id,
            &keys,
            &mut state,
            std::slice::from_ref(&input),
            |_| None,
        )
        .unwrap();
        // Flip a byte inside the AES ciphertext region.
        let blob = patch.mutations[0]
            .record
            .as_mut()
            .unwrap()
            .value
            .as_mut()
            .unwrap()
            .blob
            .as_mut()
            .unwrap();
        // Skip the IV, target a ciphertext byte, leave the trailing 32-byte mac.
        blob[20] ^= 0x01;
        let mut decode_state = HashState::default();
        let err = decode_patch(
            &patch,
            WaPatchName::RegularHigh,
            &mut decode_state,
            &keys,
            true,
            |_| None,
        )
        .expect_err("tamper must be detected");
        // Either content mac or patch mac will fire — the patch_mac depends on
        // the content_mac, so they're equivalent oracles.
        assert!(matches!(
            err,
            AppStateError::MismatchedContentMac | AppStateError::MismatchedPatchMac
        ));
    }

    #[test]
    fn decode_mutation_round_trips_directly() {
        let (keys, key_id, input) = make_input();
        let action_data = wha_proto::sync_action::SyncActionData {
            index: Some(input.index_plaintext.clone()),
            value: Some(input.action.clone()),
            padding: Some(Vec::new()),
            version: Some(input.mutation_version),
        };
        let mut plaintext = Vec::new();
        action_data.encode(&mut plaintext).unwrap();
        let m = encode_mutation(
            input.operation,
            &plaintext,
            &input.index_plaintext,
            &input.iv,
            &key_id,
            &keys,
        )
        .unwrap();
        let decoded = decode_mutation(&m, &keys, true, 0).unwrap();
        assert_eq!(decoded.operation, SyncdOperation::Set);
        assert_eq!(decoded.index_raw, input.index_plaintext);
        assert_eq!(decoded.mutation_version, 2);
    }

    /// `json_encode_index(["mute","1@s.whatsapp.net"])` must produce the same
    /// byte-string as Go's `json.Marshal([]string{"mute","1@s.whatsapp.net"})`,
    /// because the resulting bytes are HMAC'd to derive the on-wire
    /// `index_mac`. Any divergence here breaks app-state index lookup
    /// completely.
    #[test]
    fn json_encode_index_matches_go_marshal() {
        let bytes = json_encode_index(&["mute", "1@s.whatsapp.net"]);
        assert_eq!(bytes, br#"["mute","1@s.whatsapp.net"]"#.to_vec());
    }

    #[test]
    fn json_encode_index_escapes_quotes_and_backslash() {
        let bytes = json_encode_index(&["a\"b", "c\\d"]);
        assert_eq!(bytes, br#"["a\"b","c\\d"]"#.to_vec());
    }

    /// `build_mute_chat_mutation(None)` produces a `MuteAction { muted=false }`
    /// with no end timestamp — that's the unmute shape.
    #[test]
    fn mute_builder_unmutes_with_none() {
        let chat: Jid = "1@s.whatsapp.net".parse().unwrap();
        let m = build_mute_chat_mutation(&chat, None);
        assert_eq!(m.mutation_version, 2);
        assert_eq!(m.operation, SyncdOperation::Set);
        let mute = m.action.mute_action.as_ref().unwrap();
        assert_eq!(mute.muted, Some(false));
        assert_eq!(mute.mute_end_timestamp, None);
        assert_eq!(m.index_plaintext, br#"["mute","1@s.whatsapp.net"]"#.to_vec());
    }

    /// `build_mute_chat_mutation(Some(0))` — "muted forever" — must mirror
    /// upstream's `proto.Int64(-1)` substitution: the wire field is `-1`,
    /// not `0`, so the recipient knows this is the indefinite-mute sentinel.
    #[test]
    fn mute_builder_zero_means_forever() {
        let chat: Jid = "1@s.whatsapp.net".parse().unwrap();
        let m = build_mute_chat_mutation(&chat, Some(0));
        let mute = m.action.mute_action.as_ref().unwrap();
        assert_eq!(mute.muted, Some(true));
        assert_eq!(mute.mute_end_timestamp, Some(-1));
    }

    /// `build_pin_chat_mutation` produces a `pin_v1` mutation with version 5
    /// (matching `BuildPin`). The index is `["pin_v1", target.String()]`.
    #[test]
    fn pin_builder_shape() {
        let chat: Jid = "abc@s.whatsapp.net".parse().unwrap();
        let m = build_pin_chat_mutation(&chat, true);
        assert_eq!(m.mutation_version, 5);
        let pin = m.action.pin_action.as_ref().unwrap();
        assert_eq!(pin.pinned, Some(true));
        assert_eq!(
            m.index_plaintext,
            br#"["pin_v1","abc@s.whatsapp.net"]"#.to_vec()
        );
    }

    /// `build_archive_chat_mutation` produces an `archive` mutation with
    /// version 3 (matching `BuildArchive`) and embeds the supplied
    /// `MessageKey` in the `SyncActionMessageRange.messages`.
    #[test]
    fn archive_builder_shape_with_message_range() {
        let chat: Jid = "x@s.whatsapp.net".parse().unwrap();
        let key = MessageKey {
            remote_jid: Some("x@s.whatsapp.net".into()),
            from_me: Some(true),
            id: Some("MSG-1".into()),
            participant: None,
        };
        let m = build_archive_chat_mutation(&chat, true, &key);
        assert_eq!(m.mutation_version, 3);
        assert_eq!(m.operation, SyncdOperation::Set);
        let arc = m.action.archive_chat_action.as_ref().unwrap();
        assert_eq!(arc.archived, Some(true));
        let range = arc.message_range.as_ref().unwrap();
        assert_eq!(range.messages.len(), 1);
        assert_eq!(range.messages[0].key.as_ref().unwrap().id.as_deref(), Some("MSG-1"));
        assert_eq!(
            m.index_plaintext,
            br#"["archive","x@s.whatsapp.net"]"#.to_vec()
        );
    }

    /// `build_mark_read_mutation` produces `markChatAsRead` v3 with the
    /// proper read flag and a populated `SyncActionMessageRange` containing
    /// every key passed in.
    #[test]
    fn mark_read_builder_includes_all_keys() {
        let chat: Jid = "g@g.us".parse().unwrap();
        let keys = [
            MessageKey {
                remote_jid: Some("g@g.us".into()),
                from_me: Some(false),
                id: Some("M1".into()),
                participant: Some("u@s.whatsapp.net".into()),
            },
            MessageKey {
                remote_jid: Some("g@g.us".into()),
                from_me: Some(false),
                id: Some("M2".into()),
                participant: Some("v@s.whatsapp.net".into()),
            },
        ];
        let m = build_mark_read_mutation(&keys, &chat, true);
        assert_eq!(m.mutation_version, 3);
        let mca = m.action.mark_chat_as_read_action.as_ref().unwrap();
        assert_eq!(mca.read, Some(true));
        let range = mca.message_range.as_ref().unwrap();
        assert_eq!(range.messages.len(), 2);
    }

    /// `build_star_message_mutation` produces a `star` mutation with version
    /// 2. For a 1:1 DM (participant absent) the sender component is the
    /// literal "0" string.
    #[test]
    fn star_builder_dm_substitutes_sender_zero() {
        let key = MessageKey {
            remote_jid: Some("x@s.whatsapp.net".into()),
            from_me: Some(false),
            id: Some("M-X".into()),
            participant: None,
        };
        let m = build_star_message_mutation(&key, true);
        assert_eq!(m.mutation_version, 2);
        let star = m.action.star_action.as_ref().unwrap();
        assert_eq!(star.starred, Some(true));
        // Index: ["star","x@s.whatsapp.net","M-X","0","0"]
        assert_eq!(
            m.index_plaintext,
            br#"["star","x@s.whatsapp.net","M-X","0","0"]"#.to_vec()
        );
    }

    /// `build_star_message_mutation` for a group keeps the participant as
    /// the sender component, and `from_me=true` flips the 4th index entry
    /// to "1".
    #[test]
    fn star_builder_group_uses_participant_and_from_me() {
        let key = MessageKey {
            remote_jid: Some("g@g.us".into()),
            from_me: Some(true),
            id: Some("M-Y".into()),
            participant: Some("u@s.whatsapp.net".into()),
        };
        let m = build_star_message_mutation(&key, false);
        let star = m.action.star_action.as_ref().unwrap();
        assert_eq!(star.starred, Some(false));
        assert_eq!(
            m.index_plaintext,
            br#"["star","g@g.us","M-Y","1","u@s.whatsapp.net"]"#.to_vec()
        );
    }

    /// End-to-end: feed a builder-produced `MutationInput` to `encode_patch`
    /// and assert the round-trip decodes back to the same index + version
    /// — exercises the seam between the builder layer and the encoder
    /// kernel.
    #[test]
    fn builder_round_trips_through_encode_patch_decode() {
        let sync_key = [0xCDu8; 32];
        let keys = expand_app_state_keys(&sync_key).unwrap();
        let key_id = b"k1".to_vec();
        let chat: Jid = "1@s.whatsapp.net".parse().unwrap();
        let mut input = build_mute_chat_mutation(&chat, Some(1_700_000_000));
        // Pin the IV so the round-trip is deterministic.
        input.iv = [0x42u8; 16];

        let mut state = HashState::default();
        let patch = encode_patch(
            WaPatchName::RegularHigh,
            &key_id,
            &keys,
            &mut state,
            std::slice::from_ref(&input),
            |_| None,
        )
        .unwrap();
        let mut decode_state = HashState::default();
        let mutations = decode_patch(
            &patch,
            WaPatchName::RegularHigh,
            &mut decode_state,
            &keys,
            true,
            |_| None,
        )
        .unwrap();
        assert_eq!(mutations.len(), 1);
        assert_eq!(mutations[0].index, vec!["mute", "1@s.whatsapp.net"]);
        assert_eq!(mutations[0].mutation_version, 2);
        let value = mutations[0]
            .action
            .value
            .as_ref()
            .expect("SyncActionData.value present");
        let mute = value.mute_action.as_ref().expect("mute action present");
        assert_eq!(mute.muted, Some(true));
        assert_eq!(mute.mute_end_timestamp, Some(1_700_000_000));
    }
}
