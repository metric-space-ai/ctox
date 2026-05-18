//! Pure-Rust port of `_upstream/whatsmeow/appstate/*.go`: the deterministic
//! crypto + parsing kernel of WhatsApp's app-state-sync protocol.
//!
//! High-level shape:
//!
//! 1. [`keys::expand_app_state_keys`] — HKDF-SHA256 expansion of a 32-byte sync
//!    key into 5 sub-keys (`index`, `value_encryption`, `value_mac`,
//!    `snapshot_mac`, `patch_mac`).
//! 2. [`lthash::LtHash`] — 128-byte associative summation hash that maintains
//!    integrity across an arbitrary stream of `SET` / `REMOVE` mutations.
//! 3. [`hash::HashState`] + [`hash`] MAC helpers — `generate_snapshot_mac`,
//!    `generate_patch_mac`, `generate_content_mac`.
//! 4. [`decode::decode_patch`] / [`decode::decode_snapshot`] — full patch
//!    verification & decryption against [`prost`]-generated `SyncdPatch` /
//!    `SyncdSnapshot` types from `wha_proto::server_sync`.
//! 5. [`encode::encode_patch`] — symmetric encoder for patches we send (mute,
//!    pin, archive, …).
//!
//! The async wiring into a [`Client`] (sending `<iq xmlns="w:sync:app:state">`,
//! parsing `<sync><collection>…</collection></sync>` responses) lives in
//! `wha-client/src/appstate.rs` — this crate keeps the sync, deterministic
//! kernel that's easy to unit-test.

pub mod decode;
pub mod encode;
pub mod errors;
pub mod hash;
pub mod keys;
pub mod lthash;

pub use decode::{decode_patch, decode_snapshot, DecodedMutation};
pub use encode::{
    build_archive_chat_mutation, build_mark_read_mutation, build_mute_chat_mutation,
    build_pin_chat_mutation, build_star_message_mutation, encode_mutation, encode_patch,
    json_encode_index, MutationInput,
};
pub use errors::AppStateError;
pub use hash::{
    generate_content_mac, generate_patch_mac, generate_snapshot_mac, HashState, SyncdOperation,
    WaPatchName,
};
pub use keys::{expand_app_state_keys, ExpandedAppStateKeys};
pub use lthash::{LtHash, WA_PATCH_INTEGRITY};
