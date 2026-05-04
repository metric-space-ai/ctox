//! Cross-language session interop test against `go.mau.fi/libsignal`.
//!
//! The hex strings below are produced by
//! `_upstream/gen_signal_session_vectors/main.go`. The Go program drives
//! libsignal's `SessionBuilder` + `SessionCipher` end-to-end with fully
//! deterministic keypairs and prints:
//!
//! * Alice's identity / signed-pre-key / one-time-pre-key (she's the
//!   *recipient* — publishes the bundle),
//! * Bob's identity / base key (he's the *sender*),
//! * the X3DH-derived `(root_key, first_chain_key)` for cross-checking,
//! * the wire bytes of Bob's `PreKeySignalMessage` (Bob → Alice, first
//!   flight) and the inner `SignalMessage`,
//! * a follow-up `SignalMessage` sent on the same chain (counter=1),
//! * the plaintexts each ciphertext must decrypt to.
//!
//! regenerate vectors via `_upstream/gen_signal_session_vectors/main.go`:
//!
//! ```sh
//! cd _upstream/gen_signal_session_vectors && go mod tidy && go run main.go
//! ```
//!
//! Status of the gating below:
//!
//! * The two decode-only tests run as standard tests — they parse the
//!   Go-emitted wire bytes and assert every field the Rust decoder
//!   exposes matches libsignal byte-for-byte.
//! * `decrypt_pre_key_signal_message_recovers_plaintext` runs the full
//!   X3DH + receive-side decrypt path against Go-Bob's wire bytes.

use wha_crypto::KeyPair;
use wha_signal::{
    bundle::PreKeyBundle, identity::IdentityKeyPair, PreKeySignalMessage, SessionState,
    SignalMessage, CURRENT_VERSION,
};

// ---- hex helpers (mirrored from libsignal_interop.rs) ----------------------

fn hex_decode(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    for chunk in bytes.chunks(2) {
        let hi = nib(chunk[0]);
        let lo = nib(chunk[1]);
        out.push((hi << 4) | lo);
    }
    out
}

fn nib(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => 10 + c - b'a',
        b'A'..=b'F' => 10 + c - b'A',
        _ => panic!("bad hex char {c}"),
    }
}

fn arr32(s: &str) -> [u8; 32] {
    let v = hex_decode(s);
    assert_eq!(v.len(), 32, "expected 32 hex bytes, got {}", v.len());
    let mut a = [0u8; 32];
    a.copy_from_slice(&v);
    a
}

// ---- Vectors (from gen_signal_session_vectors/main.go) ---------------------
//
// regenerate vectors via _upstream/gen_signal_session_vectors/main.go

// --- Alice (recipient) ---
const ALICE_IDENTITY_PRIV: &str =
    "0001010101010101010101010101010101010101010101010101010101010141";
const ALICE_IDENTITY_PUB: &str =
    "a4e09292b651c278b9772c569f5fa9bb13d906b46ab68c9df9dc2b4409f8a209";
const ALICE_SIGNED_PRE_KEY_ID: u32 = 1;
const ALICE_SIGNED_PRE_KEY_PRIV: &str =
    "0002020202020202020202020202020202020202020202020202020202020242";
const ALICE_SIGNED_PRE_KEY_PUB: &str =
    "ce8d3ad1ccb633ec7b70c17814a5c76ecd029685050d344745ba05870e587d59";
const ALICE_SIGNED_PRE_KEY_SIGNATURE: &str =
    "a0278a6379a2bc3cae95d8664b0749255774615eddd07cba634acc0384fbc6711e30eba50024585483684bcb1e964c3492e4edb7e538cf75d776dc898ba5e00b";
const ALICE_PRE_KEY_ID: u32 = 1;
const ALICE_PRE_KEY_PRIV: &str =
    "0003030303030303030303030303030303030303030303030303030303030343";
const ALICE_PRE_KEY_PUB: &str =
    "5dfedd3b6bd47f6fa28ee15d969d5bb0ea53774d488bdaf9df1c6e0124b3ef22";
const ALICE_REGISTRATION_ID: u32 = 12345;

// --- Bob (sender) ---
const BOB_IDENTITY_PUB: &str =
    "ac01b2209e86354fb853237b5de0f4fab13c7fcbf433a61c019369617fecf10b";
const BOB_BASE_PUB: &str =
    "50a61409b1ddd0325e9b16b700e719e9772c07000b1bd7786e907c653d20495d";

// --- X3DH outputs (for cross-checking) ---
const X3DH_ROOT_KEY: &str =
    "93042a88ec05fa80852fe697fa600deab70187b247c7b84baf8f904afa10b2b9";
const X3DH_FIRST_CHAIN_KEY: &str =
    "9df5c9e0e116ba6f62d127054266eb21935ba5217cde2e6bee0890f399ba7153";

// --- First message (PreKeySignalMessage, Bob -> Alice) ---
const PLAINTEXT1: &[u8] = b"hello signal interop";
const PRE_KEY_SIGNAL_MESSAGE_HEX: &str = "33080112210550a61409b1ddd0325e9b16b700e719e9772c07000b1bd7786e907c653d20495d1a2105ac01b2209e86354fb853237b5de0f4fab13c7fcbf433a61c019369617fecf10b2252330a210550a61409b1ddd0325e9b16b700e719e9772c07000b1bd7786e907c653d20495d100018002220e48a77815cc5e852cee3380cd6a1df093f1427aadcf88eb158f8ad1ddd00a054a1d6d9d61ee9e71c28003001";
const INNER_SIGNAL_MESSAGE_HEX: &str = "330a210550a61409b1ddd0325e9b16b700e719e9772c07000b1bd7786e907c653d20495d100018002220e48a77815cc5e852cee3380cd6a1df093f1427aadcf88eb158f8ad1ddd00a054a1d6d9d61ee9e71c";
const CIPHERTEXT1: &str =
    "e48a77815cc5e852cee3380cd6a1df093f1427aadcf88eb158f8ad1ddd00a054";

// --- Second message (bare SignalMessage, counter = 1) ---
const PLAINTEXT2: &[u8] = b"second one";
const SIGNAL_MESSAGE_HEX: &str = "330a210550a61409b1ddd0325e9b16b700e719e9772c07000b1bd7786e907c653d20495d1001180022103577e4c474068a7641fbc66965090bd314e9bd7d55e3f187";
const CIPHERTEXT2: &str = "3577e4c474068a7641fbc66965090bd3";

/// libsignal sets the wire `registration_id` of a PreKeySignalMessage to
/// the *sender*'s registration id. Bob has none in the Go test setup, so
/// it serializes as 0.
const EXPECTED_PRE_KEY_REGISTRATION_ID: u32 = 0;

// ---- decode-only tests -----------------------------------------------------

/// Parse the Go-emitted `PreKeySignalMessage` hex and assert all of the
/// metadata fields the Rust decoder exposes match the values libsignal
/// printed.
#[test]
fn pre_key_signal_message_decode_yields_expected_fields() {
    let bytes = hex_decode(PRE_KEY_SIGNAL_MESSAGE_HEX);
    assert!(!bytes.is_empty(), "PRE_KEY_SIGNAL_MESSAGE_HEX must be populated");

    let parsed = PreKeySignalMessage::deserialize(&bytes)
        .expect("decoder must accept libsignal's PreKeySignalMessage");

    // Version byte: high nibble = current version, low nibble = message
    // version. libsignal hard-codes 3 in both nibbles for v3 → on-wire
    // first byte is 0x33, decoded `version` is 3.
    assert_eq!(parsed.version, CURRENT_VERSION, "version mismatch");
    assert_eq!(bytes[0], 0x33, "version byte must be 0x33 for v3");

    // Bundle metadata — these identify which of Alice's keys Bob used.
    assert_eq!(
        parsed.registration_id, EXPECTED_PRE_KEY_REGISTRATION_ID,
        "wire registration_id mismatch (sender's; Bob has none, so 0)",
    );
    assert_eq!(
        parsed.signed_pre_key_id, ALICE_SIGNED_PRE_KEY_ID,
        "signed_pre_key_id mismatch (must equal Alice's signed pre-key id)",
    );
    assert_eq!(
        parsed.pre_key_id,
        Some(ALICE_PRE_KEY_ID),
        "pre_key_id mismatch (must equal Alice's one-time pre-key id)",
    );

    // Identity / base-key fields are the *sender*'s (Bob's) — that's
    // libsignal's wire contract, see PreKeySignalMessage.go.
    assert_eq!(
        parsed.identity_key,
        arr32(BOB_IDENTITY_PUB),
        "identity_key (Bob's identity pub) mismatch",
    );
    assert_eq!(
        parsed.base_key,
        arr32(BOB_BASE_PUB),
        "base_key (Bob's base pub) mismatch",
    );

    // The inner `message` field carries a serialised `SignalMessage`. It
    // must be byte-identical to the standalone `inner_signal_message1`
    // emitted by the Go program.
    assert_eq!(
        parsed.message,
        hex_decode(INNER_SIGNAL_MESSAGE_HEX),
        "inner SignalMessage bytes must round-trip the decoder",
    );

    // Recurse: the inner SignalMessage parses cleanly with the expected
    // counters and ratchet pub.
    let inner = SignalMessage::deserialize(&parsed.message)
        .expect("inner SignalMessage must round-trip the decoder");
    assert_eq!(inner.version, CURRENT_VERSION);
    assert_eq!(
        inner.counter, 0,
        "first message on the chain must have counter 0",
    );
    assert_eq!(
        inner.previous_counter, 0,
        "no prior chain → previous_counter is 0",
    );
    // Bob's first-flight ratchet pub is his X3DH base pub.
    assert_eq!(
        inner.sender_ratchet_key,
        arr32(BOB_BASE_PUB),
        "first-flight sender_ratchet_key must equal Bob's base pub",
    );
    // The 32-byte AES-CBC ciphertext is what the chain key encrypted.
    assert_eq!(
        inner.ciphertext.as_slice(),
        hex_decode(CIPHERTEXT1).as_slice(),
        "ciphertext payload mismatch",
    );
}

/// Parse the second Go-emitted message (a bare `SignalMessage`, counter=1)
/// and assert each decoded field matches libsignal byte-for-byte.
#[test]
fn signal_message_decode_yields_expected_fields() {
    let bytes = hex_decode(SIGNAL_MESSAGE_HEX);
    assert!(!bytes.is_empty(), "SIGNAL_MESSAGE_HEX must be populated");

    let parsed = SignalMessage::deserialize(&bytes)
        .expect("decoder must accept libsignal's SignalMessage");

    assert_eq!(parsed.version, CURRENT_VERSION, "version mismatch");
    assert_eq!(bytes[0], 0x33, "version byte must be 0x33 for v3");

    // Counter advances to 1 on the second message; previous_counter still
    // 0 because no DH ratchet step has happened yet (Alice hasn't replied).
    assert_eq!(parsed.counter, 1, "counter on second message must be 1");
    assert_eq!(
        parsed.previous_counter, 0,
        "previous_counter must be 0 (no DH rotate yet)",
    );

    // Same sender ratchet pub as the first message — Bob hasn't rotated.
    assert_eq!(
        parsed.sender_ratchet_key,
        arr32(BOB_BASE_PUB),
        "sender_ratchet_key must still equal Bob's base pub",
    );

    // Ciphertext payload byte-equality.
    assert_eq!(
        parsed.ciphertext.as_slice(),
        hex_decode(CIPHERTEXT2).as_slice(),
        "ciphertext payload mismatch",
    );

    // MAC tail: 8 bytes of HMAC-SHA256 truncated.
    assert_eq!(
        parsed.mac.len(),
        8,
        "MAC length must be exactly 8 bytes (libsignal-truncated HMAC-SHA256)",
    );
}

// ---- end-to-end decrypt test ----------------------------------------------

/// Re-derive Alice's X3DH receive-side state from her seeded private keys
/// and Bob's published publics, then decrypt Bob's `PreKeySignalMessage`
/// and assert it recovers `PLAINTEXT1` byte-for-byte.
#[test]
fn decrypt_pre_key_signal_message_recovers_plaintext() {
    // Reconstruct Alice's keys from the seeded private bytes the Go
    // program prints (so the keypairs are byte-identical across sides).
    let alice_identity = IdentityKeyPair::from_private(arr32(ALICE_IDENTITY_PRIV));
    assert_eq!(
        alice_identity.public(),
        arr32(ALICE_IDENTITY_PUB),
        "Alice identity public diverged from the Go vector",
    );
    let alice_signed_pre_key = KeyPair::from_private(arr32(ALICE_SIGNED_PRE_KEY_PRIV));
    assert_eq!(
        alice_signed_pre_key.public,
        arr32(ALICE_SIGNED_PRE_KEY_PUB),
        "Alice signed pre-key public diverged from the Go vector",
    );
    let alice_pre_key = KeyPair::from_private(arr32(ALICE_PRE_KEY_PRIV));
    assert_eq!(
        alice_pre_key.public,
        arr32(ALICE_PRE_KEY_PUB),
        "Alice one-time pre-key public diverged from the Go vector",
    );

    // Run the receiver-side X3DH to derive (root, first_chain_key). This
    // must equal the Go program's `x3dh_root_key` / `x3dh_first_chain_key`
    // — that's the byte-for-byte interop check on the agreement.
    let incoming = wha_signal::x3dh::initiate_incoming(
        &alice_identity,
        &alice_signed_pre_key,
        Some(&alice_pre_key),
        &arr32(BOB_IDENTITY_PUB),
        &arr32(BOB_BASE_PUB),
    )
    .expect("Alice receiver-side X3DH must succeed");
    assert_eq!(
        incoming.root.key,
        arr32(X3DH_ROOT_KEY),
        "X3DH root_key diverged from libsignal's",
    );
    assert_eq!(
        incoming.first_chain_key,
        arr32(X3DH_FIRST_CHAIN_KEY),
        "X3DH first_chain_key diverged from libsignal's",
    );

    // Build Alice's recipient-side state. Note the role inversion vs. the
    // module's helper names: in the Go program Alice is the *recipient*
    // (the side that publishes the bundle), so on the Rust side we
    // construct her state via `initialize_as_bob` — that's the receiver
    // constructor that installs the sender's base key as the first
    // receiver chain.
    let mut alice_state = SessionState::initialize_as_bob(
        alice_identity.public(),
        arr32(BOB_IDENTITY_PUB),
        incoming,
        arr32(BOB_BASE_PUB),
        // Same signed-pre-key Alice published in her bundle. Required for
        // the second DH-ratchet step on incoming SignalMessages.
        wha_crypto::KeyPair::from_private(arr32(ALICE_SIGNED_PRE_KEY_PRIV)),
        ALICE_REGISTRATION_ID,
        0,
    );

    // Decode the wire bytes of Bob's PreKeySignalMessage.
    let bytes = hex_decode(PRE_KEY_SIGNAL_MESSAGE_HEX);
    let parsed = PreKeySignalMessage::deserialize(&bytes)
        .expect("decoder must accept libsignal's PreKeySignalMessage");
    assert_eq!(parsed.identity_key, arr32(BOB_IDENTITY_PUB));
    assert_eq!(parsed.base_key, arr32(BOB_BASE_PUB));

    // Drive the cipher. `SessionCipher::decrypt` orchestrates: PreKey
    // unwrap, MAC verify, AES-CBC decrypt.
    let plaintext = wha_signal::SessionCipher::decrypt(&mut alice_state, &bytes)
        .expect("decrypt of Go-Bob's PreKeySignalMessage must succeed");

    assert_eq!(
        plaintext.as_slice(),
        PLAINTEXT1,
        "plaintext must match the Go vector byte-for-byte",
    );

    // The follow-up bare SignalMessage decrypts to PLAINTEXT2 on the same
    // chain (counter 1, no ratchet rotate yet).
    let plaintext2 = wha_signal::SessionCipher::decrypt(
        &mut alice_state,
        &hex_decode(SIGNAL_MESSAGE_HEX),
    )
    .expect("decrypt of Go-Bob's follow-up SignalMessage must succeed");
    assert_eq!(plaintext2.as_slice(), PLAINTEXT2);

    // The bundle Alice would publish for Bob to use — kept here so the
    // signed-pre-key-signature constant is referenced and the file stays
    // self-checking once a sender-side test is added.
    let _bundle_for_alice = PreKeyBundle {
        registration_id: ALICE_REGISTRATION_ID,
        device_id: 1,
        pre_key_id: Some(ALICE_PRE_KEY_ID),
        pre_key_public: Some(arr32(ALICE_PRE_KEY_PUB)),
        signed_pre_key_id: ALICE_SIGNED_PRE_KEY_ID,
        signed_pre_key_public: arr32(ALICE_SIGNED_PRE_KEY_PUB),
        signed_pre_key_signature: {
            let v = hex_decode(ALICE_SIGNED_PRE_KEY_SIGNATURE);
            let mut s = [0u8; 64];
            s.copy_from_slice(&v);
            s
        },
        identity_key: arr32(ALICE_IDENTITY_PUB),
    };
}
