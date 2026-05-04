//! Async wiring around the [`wha_appstate`] crypto kernel: send the
//! `<iq xmlns="w:sync:app:state">` IQ, parse the `<sync><collection>` reply,
//! decrypt every patch + snapshot, and persist the resulting LTHash state +
//! mutation MACs into the store.
//!
//! Mirrors `_upstream/whatsmeow/appstate.go::FetchAppState`. The
//! deterministic crypto + parsing kernel lives in
//! [`wha_appstate`]; this module is the IO-driven shell around it.

use std::collections::HashMap;

use prost::Message;
use tracing::{debug, warn};
use wha_appstate::{
    build_archive_chat_mutation, build_mark_read_mutation, build_mute_chat_mutation,
    build_pin_chat_mutation, build_star_message_mutation, decode_patch, decode_snapshot,
    encode_patch, expand_app_state_keys, AppStateError, DecodedMutation, HashState, MutationInput,
    SyncdOperation, WaPatchName,
};
use wha_binary::{Attrs, Node, Value};
use wha_proto::common::MessageKey;
use wha_proto::server_sync::{SyncdPatch, SyncdSnapshot};
use wha_types::Jid;

use crate::client::Client;
use crate::error::ClientError;
use crate::request::{InfoQuery, IqType};

/// Re-export the kernel's `WaPatchName` for callers of this module.
pub use wha_appstate::WaPatchName as PatchName;

/// Re-export the high-level decoded mutation. `Vec<Mutation>` is what
/// [`fetch_app_state_patches`] returns to the caller.
pub type Mutation = DecodedMutation;

impl From<AppStateError> for ClientError {
    fn from(e: AppStateError) -> Self {
        ClientError::Other(format!("appstate: {e}"))
    }
}

fn server_jid() -> Jid {
    // `s.whatsapp.net` is the server JID for app-state IQs (mirrors
    // `types.ServerJID` upstream — `Jid{user: "", server: "s.whatsapp.net"}`).
    Jid::new("", "s.whatsapp.net")
}

/// Fetch and decrypt all pending patches for a single app-state collection.
///
/// `full=true` resets the local LTHash state and asks the server for a fresh
/// snapshot before applying any subsequent patches; `false` is the
/// incremental path that bumps the local version forward.
///
/// Mirrors `Client.fetchAppState` in `_upstream/whatsmeow/appstate.go`.
pub async fn fetch_app_state_patches(
    client: &Client,
    name: PatchName,
    full: bool,
) -> Result<Vec<Mutation>, ClientError> {
    let device = &client.device;
    let store = &device.app_state_mutations;
    let key_store = &device.app_state_keys;

    if full {
        store
            .delete_app_state_version(name.as_str())
            .await
            .map_err(|e| ClientError::Store(e))?;
    }

    let (mut version, mut hash) = match store
        .get_app_state_version(name.as_str())
        .await
        .map_err(ClientError::Store)?
    {
        Some(v) => v,
        None => (0u64, [0u8; 128]),
    };

    let mut want_snapshot = full || version == 0;
    let mut accumulated: Vec<Mutation> = Vec::new();
    let mut has_more = true;

    while has_more {
        let resp = send_collection_iq(client, name, version, want_snapshot).await?;
        let collection = resp
            .child_by_tag(&["sync", "collection"])
            .ok_or_else(|| ClientError::Malformed("missing sync/collection".into()))?;

        let mut state = HashState { version, hash };

        // Parse the snapshot first (if present).
        let mut snapshot_mutations: Vec<Mutation> = Vec::new();
        if let Some(snap_node) = collection.child_by_tag(&["snapshot"]) {
            if let Some(bytes) = snap_node.content.as_bytes() {
                let snapshot = SyncdSnapshot::decode(bytes)
                    .map_err(|e| ClientError::Proto(format!("snapshot: {e}")))?;
                let key_id = snapshot
                    .key_id
                    .as_ref()
                    .and_then(|k| k.id.as_deref())
                    .ok_or_else(|| ClientError::Malformed("snapshot missing key_id".into()))?
                    .to_vec();
                let raw_key = key_store
                    .get_app_state_sync_key(&key_id)
                    .await
                    .map_err(ClientError::Store)?
                    .ok_or_else(|| {
                        ClientError::Other(format!(
                            "missing app-state sync key {}",
                            hex_short(&key_id)
                        ))
                    })?;
                let keys = expand_app_state_keys(&raw_key.data)?;
                let mutations = decode_snapshot(&snapshot, name, &mut state, &keys, true)?;
                persist_mutations(client, name, &state, &mutations).await?;
                snapshot_mutations = mutations;
            }
        }

        // Then any patches.
        let mut patch_mutations: Vec<Mutation> = Vec::new();
        if let Some(patches_node) = collection.child_by_tag(&["patches"]) {
            for patch_node in patches_node.children_by_tag("patch") {
                let bytes = patch_node
                    .content
                    .as_bytes()
                    .ok_or_else(|| ClientError::Malformed("patch missing bytes".into()))?;
                let patch = SyncdPatch::decode(bytes)
                    .map_err(|e| ClientError::Proto(format!("patch: {e}")))?;
                let key_id = patch
                    .key_id
                    .as_ref()
                    .and_then(|k| k.id.as_deref())
                    .ok_or_else(|| ClientError::Malformed("patch missing key_id".into()))?
                    .to_vec();
                let raw_key = key_store
                    .get_app_state_sync_key(&key_id)
                    .await
                    .map_err(ClientError::Store)?
                    .ok_or_else(|| {
                        ClientError::Other(format!(
                            "missing app-state sync key {}",
                            hex_short(&key_id)
                        ))
                    })?;
                let keys = expand_app_state_keys(&raw_key.data)?;

                // Resolve previous SET value-MACs out of the store. The
                // closure can't be async, so collect the ones the patch
                // actually references first via a synchronous proxy:
                // `decode_patch` calls back synchronously, but our store
                // is async. Pre-fetch every distinct index_mac.
                let prev_macs = prefetch_prev_macs(client, name, &patch).await?;

                let mutations = decode_patch(
                    &patch,
                    name,
                    &mut state,
                    &keys,
                    true,
                    |im| prev_macs.get(im).cloned(),
                )?;

                persist_mutations(client, name, &state, &mutations).await?;
                patch_mutations.extend(mutations);
            }
        }

        version = state.version;
        hash = state.hash;

        // `has_more_patches` lives on the <collection> attrs.
        let mut ag = collection.attr_getter();
        has_more = ag.optional_bool("has_more_patches");

        accumulated.extend(snapshot_mutations);
        accumulated.extend(patch_mutations);

        // Once we've consumed the optional snapshot, never ask for another.
        want_snapshot = false;
    }

    debug!(
        name = name.as_str(),
        version,
        mutations = accumulated.len(),
        "app state sync complete"
    );
    Ok(accumulated)
}

/// Sync every known collection (the 5 names from `WaPatchName::ALL`).
/// Returns `name -> mutations`. Errors on any one collection are *logged*
/// and the remaining collections are still attempted, mirroring upstream's
/// "best effort" semantics.
pub async fn fetch_all_app_state(
    client: &Client,
) -> Result<HashMap<String, Vec<Mutation>>, ClientError> {
    let mut out = HashMap::new();
    for name in WaPatchName::ALL {
        match fetch_app_state_patches(client, name, false).await {
            Ok(m) => {
                out.insert(name.as_str().to_string(), m);
            }
            Err(e) => {
                warn!(?e, name = name.as_str(), "app state fetch failed");
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------

/// Build and send the
/// `<iq xmlns="w:sync:app:state"><sync><collection .../></sync></iq>` IQ.
async fn send_collection_iq(
    client: &Client,
    name: WaPatchName,
    version: u64,
    want_snapshot: bool,
) -> Result<Node, ClientError> {
    // Build attrs: name + return_snapshot always; version only when not asking
    // for a snapshot.
    let mut attrs = Attrs::new();
    attrs.insert("name".into(), Value::String(name.as_str().to_string()));
    attrs.insert(
        "return_snapshot".into(),
        Value::String(if want_snapshot { "true" } else { "false" }.to_string()),
    );
    if !want_snapshot {
        attrs.insert("version".into(), Value::String(version.to_string()));
    }
    let collection = Node::new("collection", attrs, None);
    let sync = Node::new("sync", Attrs::new(), Some(Value::Nodes(vec![collection])));

    let q = InfoQuery::new("w:sync:app:state", IqType::Set)
        .to(server_jid())
        .content(Value::Nodes(vec![sync]));
    client.send_iq(q).await
}

/// Persist every mutation MAC and the new HashState to the store.
async fn persist_mutations(
    client: &Client,
    name: WaPatchName,
    state: &HashState,
    mutations: &[Mutation],
) -> Result<(), ClientError> {
    let store = &client.device.app_state_mutations;
    store
        .put_app_state_version(name.as_str(), state.version, state.hash)
        .await
        .map_err(ClientError::Store)?;

    let mut to_remove: Vec<Vec<u8>> = Vec::new();
    for m in mutations {
        match m.operation {
            SyncdOperation::Set => {
                store
                    .put_app_state_mutation_mac(
                        name.as_str(),
                        m.patch_version,
                        &m.index_mac,
                        &m.value_mac,
                    )
                    .await
                    .map_err(ClientError::Store)?;
            }
            SyncdOperation::Remove => {
                to_remove.push(m.index_mac.clone());
            }
        }
    }
    if !to_remove.is_empty() {
        store
            .delete_app_state_mutation_macs(name.as_str(), &to_remove)
            .await
            .map_err(ClientError::Store)?;
    }
    Ok(())
}

/// Walk the patch's mutations once, collect the distinct `index_mac`s that
/// aren't satisfied within the patch, and pre-fetch their previous value-MACs
/// from the store. Returns a map for the synchronous decoder to consult.
async fn prefetch_prev_macs(
    client: &Client,
    name: WaPatchName,
    patch: &SyncdPatch,
) -> Result<HashMap<Vec<u8>, Vec<u8>>, ClientError> {
    let store = &client.device.app_state_mutations;
    let mut needed: Vec<Vec<u8>> = Vec::new();

    for (i, m) in patch.mutations.iter().enumerate() {
        let im = match m
            .record
            .as_ref()
            .and_then(|r| r.index.as_ref())
            .and_then(|i| i.blob.clone())
        {
            Some(b) => b,
            None => continue,
        };
        // Skip if an earlier SET in this same patch satisfies it.
        let mut found_in_patch = false;
        for j in (0..i).rev() {
            let prev_im = patch.mutations[j]
                .record
                .as_ref()
                .and_then(|r| r.index.as_ref())
                .and_then(|i| i.blob.as_deref())
                .unwrap_or_default();
            if prev_im == im.as_slice() {
                found_in_patch = true;
                break;
            }
        }
        if !found_in_patch && !needed.contains(&im) {
            needed.push(im);
        }
    }

    let mut out = HashMap::new();
    for im in needed {
        if let Some(vm) = store
            .get_app_state_mutation_mac(name.as_str(), &im)
            .await
            .map_err(ClientError::Store)?
        {
            out.insert(im, vm);
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Outgoing mutations.
//
// Mirrors `_upstream/whatsmeow/appstate.go::SendAppState`. A single
// `<iq xmlns="w:sync:app:state" type="set">` carries one
// `<sync><collection name="…" version="N+1"><patch>…</patch></collection></sync>`
// with the encoded `SyncdPatch` bytes. Persistence — bumping the local
// version + LTHash + appending the new mutation MACs — happens *after* a
// successful round-trip.
// ---------------------------------------------------------------------------

/// Encode and ship a list of outgoing mutations for one collection.
///
/// Walks the wire protocol upstream uses in `(Client).SendAppState`:
///
/// 1. Look up the latest app-state sync key and load the current
///    LTHash + version of the named collection from the store.
/// 2. Run [`encode_patch`] over the inputs to produce a wire-format
///    `SyncdPatch`.
/// 3. Wrap in `<iq xmlns="w:sync:app:state" type="set"><sync><collection
///    name="…" version="N+1"><patch>…</patch></collection></sync></iq>` and
///    send it.
/// 4. Persist the new version + LTHash + per-mutation MACs into the store
///    so subsequent encodes ride the advanced state.
pub async fn send_app_state_mutations(
    client: &Client,
    name: PatchName,
    mutations: Vec<MutationInput>,
) -> Result<(), ClientError> {
    if mutations.is_empty() {
        return Ok(());
    }
    let device = &client.device;
    let store = &device.app_state_mutations;
    let key_store = &device.app_state_keys;

    let key_id = key_store
        .get_latest_app_state_sync_key_id()
        .await
        .map_err(ClientError::Store)?
        .ok_or_else(|| {
            ClientError::Other("no app-state sync key — must run a primary fetch first".into())
        })?;
    let raw_key = key_store
        .get_app_state_sync_key(&key_id)
        .await
        .map_err(ClientError::Store)?
        .ok_or_else(|| {
            ClientError::Other(format!(
                "missing app-state sync key {}",
                hex_short(&key_id)
            ))
        })?;
    let keys = expand_app_state_keys(&raw_key.data)?;

    // Snapshot the (version, hash) we'll be advancing.
    let (version, hash) = store
        .get_app_state_version(name.as_str())
        .await
        .map_err(ClientError::Store)?
        .unwrap_or((0u64, [0u8; 128]));
    let mut state = HashState { version, hash };

    // For each mutation we need to look up the previous value-MAC keyed on
    // the index-MAC. The encoder calls back synchronously, so we pre-compute
    // every needed mac before invoking the encoder.
    let mut prev_macs: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    for input in &mutations {
        let im = wha_crypto::hmac_sha256_concat(&keys.index, &[&input.index_plaintext]);
        if let Some(vm) = store
            .get_app_state_mutation_mac(name.as_str(), &im)
            .await
            .map_err(ClientError::Store)?
        {
            prev_macs.insert(im, vm);
        }
    }

    let patch = encode_patch(name, &key_id, &keys, &mut state, &mutations, |im| {
        prev_macs.get(im).cloned()
    })?;
    let mut patch_bytes = Vec::with_capacity(patch.encoded_len());
    patch.encode(&mut patch_bytes)
        .map_err(|e| ClientError::Proto(e.to_string()))?;

    // Build the wire IQ.
    let mut patch_attrs = Attrs::new();
    let patch_node = Node::new("patch", patch_attrs.clone(), Some(Value::Bytes(patch_bytes)));
    patch_attrs.clear();
    let mut coll_attrs = Attrs::new();
    coll_attrs.insert("name".into(), Value::String(name.as_str().to_string()));
    coll_attrs.insert("version".into(), Value::String(state.version.to_string()));
    coll_attrs.insert("return_snapshot".into(), Value::String("false".into()));
    let collection = Node::new(
        "collection",
        coll_attrs,
        Some(Value::Nodes(vec![patch_node])),
    );
    let sync = Node::new("sync", Attrs::new(), Some(Value::Nodes(vec![collection])));

    let q = InfoQuery::new("w:sync:app:state", IqType::Set)
        .to(server_jid())
        .content(Value::Nodes(vec![sync]));
    let _resp = client.send_iq(q).await?;

    // Persist post-success: bump version + hash, append new value-MACs.
    store
        .put_app_state_version(name.as_str(), state.version, state.hash)
        .await
        .map_err(ClientError::Store)?;
    for (input, m) in mutations.iter().zip(patch.mutations.iter()) {
        let blob = match m
            .record
            .as_ref()
            .and_then(|r| r.value.as_ref())
            .and_then(|v| v.blob.as_deref())
        {
            Some(b) if b.len() >= 32 => b,
            _ => continue,
        };
        let value_mac = &blob[blob.len() - 32..];
        let index_mac = wha_crypto::hmac_sha256_concat(&keys.index, &[&input.index_plaintext]);
        match input.operation {
            SyncdOperation::Set => {
                store
                    .put_app_state_mutation_mac(
                        name.as_str(),
                        state.version,
                        &index_mac,
                        value_mac,
                    )
                    .await
                    .map_err(ClientError::Store)?;
            }
            SyncdOperation::Remove => {
                store
                    .delete_app_state_mutation_macs(name.as_str(), &[index_mac])
                    .await
                    .map_err(ClientError::Store)?;
            }
        }
    }

    Ok(())
}

/// Convenience: mark a chat as read for the listed message IDs. Builds a
/// single-mutation patch for each id (collapsed into one IQ) and ships it.
/// Mirrors the typical use of `Client.SendAppState(BuildMarkChatAsRead(...))`
/// upstream.
pub async fn mark_read(
    client: &Client,
    chat: &Jid,
    message_ids: &[String],
) -> Result<(), ClientError> {
    let chat_str = chat.to_string();
    let keys: Vec<MessageKey> = message_ids
        .iter()
        .map(|id| MessageKey {
            remote_jid: Some(chat_str.clone()),
            from_me: Some(false),
            id: Some(id.clone()),
            participant: None,
        })
        .collect();
    let mutation = build_mark_read_mutation(&keys, chat, true);
    send_app_state_mutations(client, PatchName::RegularLow, vec![mutation]).await
}

/// Convenience: mute (or unmute) a chat. `mute_until = None` unmutes;
/// `Some(0)` means "muted forever"; any positive value is a UnixMilli expiry.
pub async fn mute_chat(
    client: &Client,
    chat: &Jid,
    mute_until: Option<i64>,
) -> Result<(), ClientError> {
    let mutation = build_mute_chat_mutation(chat, mute_until);
    send_app_state_mutations(client, PatchName::RegularHigh, vec![mutation]).await
}

/// Convenience: pin or unpin a chat.
pub async fn pin_chat(client: &Client, chat: &Jid, pinned: bool) -> Result<(), ClientError> {
    let mutation = build_pin_chat_mutation(chat, pinned);
    send_app_state_mutations(client, PatchName::RegularLow, vec![mutation]).await
}

/// Convenience: archive (or unarchive) a chat. Mirrors `BuildArchive` —
/// callers must supply the key of the chat's most-recent message.
pub async fn archive_chat(
    client: &Client,
    chat: &Jid,
    archived: bool,
    last_msg_key: &MessageKey,
) -> Result<(), ClientError> {
    let mutation = build_archive_chat_mutation(chat, archived, last_msg_key);
    send_app_state_mutations(client, PatchName::RegularLow, vec![mutation]).await
}

/// Convenience: star (or unstar) a single message.
pub async fn star_message(
    client: &Client,
    message_key: &MessageKey,
    starred: bool,
) -> Result<(), ClientError> {
    let mutation = build_star_message_mutation(message_key, starred);
    send_app_state_mutations(client, PatchName::RegularHigh, vec![mutation]).await
}

fn hex_short(bytes: &[u8]) -> String {
    let max = bytes.len().min(8);
    bytes[..max]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wha_appstate::{
        encode_patch, expand_app_state_keys, MutationInput, SyncdOperation as AppOp,
    };
    use wha_proto::sync_action::{MuteAction, SyncActionValue};
    use wha_store::{AppStateSyncKey, MemoryStore};

    /// Round-trip: encode a patch in-memory with our encoder, then decode it
    /// in the same process via the same key. Exercises the full
    /// (HKDF → encrypt → MAC → decode → MAC verify → decrypt → parse)
    /// pipeline that `fetch_app_state_patches` would run, minus the IQ.
    #[tokio::test]
    async fn end_to_end_patch_round_trip() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();

        let raw_key = [0xCDu8; 32];
        let key_id = b"k1".to_vec();
        device
            .app_state_keys
            .put_app_state_sync_key(
                key_id.clone(),
                AppStateSyncKey {
                    data: raw_key.to_vec(),
                    fingerprint: vec![],
                    timestamp: 0,
                },
            )
            .await
            .unwrap();

        let keys = expand_app_state_keys(&raw_key).unwrap();
        let mut state = HashState::default();
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
            operation: AppOp::Set,
            index_plaintext: br#"["mute","x@s.whatsapp.net"]"#.to_vec(),
            action,
            mutation_version: 2,
            iv: [9u8; 16],
        };
        let patch = encode_patch(
            WaPatchName::RegularHigh,
            &key_id,
            &keys,
            &mut state,
            &[input],
            |_| None,
        )
        .unwrap();

        // Decode the patch *as the client would* — feed it to the kernel and
        // assert all five sub-keys + LTHash chain validate.
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
        assert_eq!(mutations[0].operation, AppOp::Set);
        assert_eq!(decode_state, state);
    }

    /// `prefetch_prev_macs` only asks the store for distinct `index_mac`s that
    /// aren't satisfied by an earlier in-patch SET — sanity-check this logic
    /// against a small synthetic patch.
    #[tokio::test]
    async fn prefetch_prev_macs_dedups_within_patch() {
        use wha_proto::server_sync::{
            KeyId, SyncdIndex, SyncdMutation, SyncdRecord, SyncdValue,
        };

        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = Client::new(device);

        // Pre-stuff the store with a value_mac for index "im_old".
        cli.device
            .app_state_mutations
            .put_app_state_mutation_mac("regular_high", 1, b"im_old", b"vm_old")
            .await
            .unwrap();

        // im_old appears once → fetched from store.
        // im_dup appears twice → 2nd is satisfied by the 1st in-patch SET.
        let mk = |im: &[u8]| SyncdMutation {
            operation: Some(0),
            record: Some(SyncdRecord {
                index: Some(SyncdIndex { blob: Some(im.to_vec()) }),
                value: Some(SyncdValue {
                    blob: Some(vec![0u8; 64]), // dummy 64-byte blob
                }),
                key_id: Some(KeyId { id: Some(b"k".to_vec()) }),
            }),
        };
        let patch = SyncdPatch {
            mutations: vec![mk(b"im_old"), mk(b"im_dup"), mk(b"im_dup")],
            ..Default::default()
        };

        let prev = prefetch_prev_macs(&cli, WaPatchName::RegularHigh, &patch)
            .await
            .unwrap();
        // Only im_old is satisfied by the store; im_dup's 2nd ref is satisfied
        // in-patch and so is not queried.
        assert_eq!(prev.get(&b"im_old".to_vec()), Some(&b"vm_old".to_vec()));
    }

    /// `send_app_state_mutations` errors cleanly when no app-state sync key
    /// has been seeded into the store yet — primary fetch must run first
    /// upstream too. We check that the error path doesn't `await` on a
    /// network IQ before noticing.
    #[tokio::test]
    async fn send_app_state_mutations_errors_without_sync_key() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = Client::new(device);
        let chat: Jid = "1@s.whatsapp.net".parse().unwrap();
        let mutation = build_mute_chat_mutation(&chat, Some(-1));
        let r = send_app_state_mutations(&cli, PatchName::RegularHigh, vec![mutation]).await;
        match r {
            Err(ClientError::Other(msg)) if msg.contains("no app-state sync key") => {}
            other => panic!(
                "expected Other('no app-state sync key…'), got {other:?}"
            ),
        }
    }

    /// With a sync key in place but no live socket, the encoder runs to
    /// completion and the failure happens at `client.send_iq` (NotConnected).
    /// This pins the wire-prep path: a missing key was the only "synchronous
    /// reject" reason, after that we go all the way to the socket.
    #[tokio::test]
    async fn send_app_state_mutations_reaches_send_iq_with_sync_key() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let raw_key = [0xCDu8; 32];
        let key_id = b"k1".to_vec();
        device
            .app_state_keys
            .put_app_state_sync_key(
                key_id.clone(),
                AppStateSyncKey {
                    data: raw_key.to_vec(),
                    fingerprint: vec![],
                    timestamp: 0,
                },
            )
            .await
            .unwrap();
        let (cli, _evt) = Client::new(device);
        let chat: Jid = "x@s.whatsapp.net".parse().unwrap();
        let r = send_app_state_mutations(
            &cli,
            PatchName::RegularHigh,
            vec![build_mute_chat_mutation(&chat, Some(0))],
        )
        .await;
        match r {
            Err(ClientError::NotConnected) => {}
            other => panic!("expected NotConnected, got {other:?}"),
        }
    }

    /// Empty mutation list short-circuits — the function returns Ok(()) with
    /// no IQ traffic and no store mutation. Pins the trivial guard.
    #[tokio::test]
    async fn send_app_state_mutations_empty_is_noop() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = Client::new(device);
        // Empty input: must not even consult the keystore — so no key
        // pre-loaded should still be Ok.
        send_app_state_mutations(&cli, PatchName::RegularHigh, vec![])
            .await
            .expect("empty mutation list is a no-op");
    }

    /// The convenience wrappers (`mark_read` / `mute_chat`) bottom out in
    /// `send_app_state_mutations`. With no sync key seeded the inner call
    /// returns the documented error — confirming the wrappers are wired
    /// through.
    #[tokio::test]
    async fn mark_read_and_mute_chat_wire_through_to_send() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = Client::new(device);
        let chat: Jid = "1@s.whatsapp.net".parse().unwrap();
        let r1 = mark_read(&cli, &chat, &["A".into(), "B".into()]).await;
        assert!(matches!(r1, Err(ClientError::Other(_))));
        let r2 = mute_chat(&cli, &chat, Some(0)).await;
        assert!(matches!(r2, Err(ClientError::Other(_))));
    }
}
