//! Phone-code pairing — extended public API.
//!
//! This module mirrors `_upstream/whatsmeow/pair-code.go` (245 LOC) and exposes
//! the **caller-friendly** entry point [`pair_phone`] with the full upstream
//! signature: `client_type` enum, push-notification toggle, and a custom
//! `client_display_name`. The crypto core (PBKDF2 → AES-CTR-wrap of the
//! ephemeral X25519 pubkey, then `link_code_companion_reg / companion_hello`
//! IQ) is identical to the simpler helper in [`crate::pair::pair_phone`]; the
//! difference is exclusively in how callers parameterise the IQ.
//!
//! Two stages exist on the wire — see upstream's `pair-code.go`:
//!
//! 1. **`companion_hello`** (this module's [`pair_phone`]): we send the wrapped
//!    ephemeral pub + our static noise pub + the platform display string. The
//!    server replies with a `link_code_pairing_ref` and starts showing the
//!    8-character code on the user's phone.
//! 2. **`companion_finish`** (handled by [`crate::pair::handle_pair_success`]
//!    once the primary device approves): the primary device derives a shared
//!    secret using our ephemeral pub; we mirror the same DH on our side, build
//!    the wrapped key bundle (identity keys + ADV randomness), and ship it back
//!    to the server. That second leg is **out of scope here** — see the
//!    `handle_code_pair_notification` TODO at the bottom of this file.

use rand::RngCore;

use wha_binary::{Attrs, Node, Value};
use wha_crypto::{ctr_xor, hmac_sha256, KeyPair};
use wha_types::{jid::server, Jid};

use crate::client::Client;
use crate::error::ClientError;
use crate::request::{InfoQuery, IqType};

/// PairClientType — mirrors `PairClientType` in `pair-code.go`. The numeric
/// value is what travels on the wire as `companion_platform_id`.
///
/// Whatsmeow allows callers to pass any of these — the server only validates
/// the resulting `companion_platform_display` string, not this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ClientType {
    /// `PairClientUnknown == 0` — used when we don't want to claim any specific
    /// platform. Default for the simpler [`crate::pair::pair_phone`] helper.
    Unknown = 0,
    Chrome = 1,
    Edge = 2,
    Firefox = 3,
    IE = 4,
    Opera = 5,
    Safari = 6,
    Electron = 7,
    UWP = 8,
    OtherWebClient = 9,
}

impl ClientType {
    /// Numeric `companion_platform_id` — the ASCII representation of the enum
    /// value, ready to drop into the IQ as a string node content.
    pub fn as_platform_id(&self) -> String {
        (*self as u8).to_string()
    }
}

impl Default for ClientType {
    fn default() -> Self {
        ClientType::Unknown
    }
}

/// Custom base32 alphabet WhatsApp uses for the 8-character linking code.
/// Mirrors `linkingBase32` in `pair-code.go`.
const LINKING_BASE32: &[u8; 32] = b"123456789ABCDEFGHJKLMNPQRSTVWXYZ";

/// Encode 5 raw bytes as 8 base32 chars using [`LINKING_BASE32`].
fn linking_base32_encode(input: &[u8; 5]) -> String {
    let bits: u64 = ((input[0] as u64) << 32)
        | ((input[1] as u64) << 24)
        | ((input[2] as u64) << 16)
        | ((input[3] as u64) << 8)
        | (input[4] as u64);
    let mut out = String::with_capacity(8);
    for shift in (0..8).rev() {
        let idx = ((bits >> (shift * 5)) & 0x1F) as usize;
        out.push(LINKING_BASE32[idx] as char);
    }
    out
}

/// PBKDF2-HMAC-SHA256 with the given password / salt / iterations and a
/// 32-byte output. Equivalent to upstream's
/// `pbkdf2.Key([]byte(code), salt, 2<<16, 32, sha256.New)`.
fn pbkdf2_hmac_sha256(password: &[u8], salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut block_input = Vec::with_capacity(salt.len() + 4);
    block_input.extend_from_slice(salt);
    block_input.extend_from_slice(&1u32.to_be_bytes());
    let mut u = hmac_sha256(password, &block_input);
    let mut t = u.clone();
    for _ in 1..iterations {
        u = hmac_sha256(password, &u);
        for (acc, byte) in t.iter_mut().zip(u.iter()) {
            *acc ^= byte;
        }
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&t[..32]);
    out
}

/// Internal: derive everything we need for a `companion_hello` round-trip.
///
/// Returns `(ephemeral_keypair, wrapped_pubkey_80b, encoded_8char_code)`.
/// Equivalent to `generateCompanionEphemeralKey` in upstream.
fn generate_companion_ephemeral_key() -> Result<(KeyPair, Vec<u8>, String), ClientError> {
    let mut rng = rand::rngs::OsRng;
    let ephemeral = KeyPair::generate(&mut rng);

    let mut salt = [0u8; 32];
    rng.fill_bytes(&mut salt);
    let mut iv = [0u8; 16];
    rng.fill_bytes(&mut iv);
    let mut linking_code_raw = [0u8; 5];
    rng.fill_bytes(&mut linking_code_raw);
    let encoded_linking_code = linking_base32_encode(&linking_code_raw);

    // PBKDF2 derives the AES-256-CTR key from the user-visible 8-char code.
    // Iterations are `2 << 16` upstream — that's `131072`, *not* `65536`.
    let key = pbkdf2_hmac_sha256(encoded_linking_code.as_bytes(), &salt, 2 << 16);
    let mut encrypted_pub = ephemeral.public;
    ctr_xor(&key, &iv, &mut encrypted_pub)
        .map_err(|e| ClientError::Malformed(format!("AES-CTR setup: {e}")))?;

    let mut wrapped = Vec::with_capacity(80);
    wrapped.extend_from_slice(&salt);
    wrapped.extend_from_slice(&iv);
    wrapped.extend_from_slice(&encrypted_pub);
    Ok((ephemeral, wrapped, encoded_linking_code))
}

/// Build the inner `<link_code_companion_reg stage="companion_hello" …>` node
/// without sending it. Pure: no I/O, no RNG-dependent branches once the
/// `wrapped_ephemeral_pub` and platform values are decided.
///
/// The shape is what upstream's `pair-code.go::PairPhone` writes:
/// ```xml
/// <link_code_companion_reg jid="..." stage="companion_hello" should_show_push_notification="true|false">
///   <link_code_pairing_wrapped_companion_ephemeral_pub_key>...80B...</link_code_pairing_wrapped_companion_ephemeral_pub_key>
///   <companion_server_auth_key_pub>...32B...</companion_server_auth_key_pub>
///   <companion_platform_id>1</companion_platform_id>
///   <companion_platform_display>Chrome (Linux)</companion_platform_display>
///   <link_code_pairing_nonce>0x00</link_code_pairing_nonce>
/// </link_code_companion_reg>
/// ```
pub(crate) fn build_companion_hello_node(
    jid: &Jid,
    wrapped_ephemeral_pub: Vec<u8>,
    server_auth_pub: &[u8; 32],
    client_type: ClientType,
    client_display_name: &str,
    show_push_notification: bool,
) -> Node {
    let mut companion_attrs = Attrs::new();
    companion_attrs.insert("jid".into(), Value::Jid(jid.clone()));
    companion_attrs.insert("stage".into(), Value::String("companion_hello".into()));
    companion_attrs.insert(
        "should_show_push_notification".into(),
        Value::String(if show_push_notification { "true".into() } else { "false".into() }),
    );

    let pairing_ephemeral = Node::new(
        "link_code_pairing_wrapped_companion_ephemeral_pub_key",
        Attrs::new(),
        Some(Value::Bytes(wrapped_ephemeral_pub)),
    );
    let server_auth = Node::new(
        "companion_server_auth_key_pub",
        Attrs::new(),
        Some(Value::Bytes(server_auth_pub.to_vec())),
    );
    let platform_id = Node::new(
        "companion_platform_id",
        Attrs::new(),
        Some(Value::String(client_type.as_platform_id())),
    );
    let platform_display = Node::new(
        "companion_platform_display",
        Attrs::new(),
        Some(Value::String(client_display_name.to_owned())),
    );
    let nonce = Node::new(
        "link_code_pairing_nonce",
        Attrs::new(),
        Some(Value::Bytes(vec![0])),
    );

    Node::new(
        "link_code_companion_reg",
        companion_attrs,
        Some(Value::Nodes(vec![
            pairing_ephemeral,
            server_auth,
            platform_id,
            platform_display,
            nonce,
        ])),
    )
}

/// Validate the phone number: only digits, length > 6, no leading zero.
///
/// Mirrors the regex strip + length check + `0`-prefix check in upstream's
/// `PairPhone`. Returns the cleaned digit-only string on success.
fn normalise_phone_number(phone: &str) -> Result<String, ClientError> {
    let digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() <= 6 {
        return Err(ClientError::Malformed("phone number too short".into()));
    }
    if digits.starts_with('0') {
        return Err(ClientError::Malformed(
            "phone number must be in international format (no leading 0)".into(),
        ));
    }
    Ok(digits)
}

/// Pair this device to an existing primary by phone number.
///
/// Returns the 8-character pairing code (formatted `XXXX-XXXX`) the user must
/// type into the linked-devices screen on their phone. The IQ exchange is the
/// `companion_hello` half — once the primary device approves, a follow-up
/// notification arrives that the read pump should route to a code-pair
/// notification handler. The `companion_finish` half is **out of scope here**
/// (see TODO at the bottom of the module).
///
/// Mirrors `Client.PairPhone` in `pair-code.go`.
///
/// # Arguments
/// - `client`: an already-connected `Client` (caller is responsible for
///   `client.connect().await` and waiting for the QR event so the noise
///   handshake is complete).
/// - `phone_number`: international-format digits (`+` and other punctuation
///   are stripped). E.g. `"49 152 1234 5678"` becomes `"4915212345678"`.
/// - `show_push_notification`: when `true`, asks the server to push a
///   notification to the user's phone with the pairing prompt.
/// - `client_type`: one of [`ClientType`] — sent as `companion_platform_id`.
/// - `client_display_name`: must follow the `Browser (OS)` shape upstream
///   documents (e.g. `"Chrome (Linux)"`). The server validates this string
///   and returns 400 on unknown combinations.
pub async fn pair_phone(
    client: &Client,
    phone_number: &str,
    show_push_notification: bool,
    client_type: ClientType,
    client_display_name: &str,
) -> Result<String, ClientError> {
    let digits = normalise_phone_number(phone_number)?;
    let (_ephemeral_kp, wrapped_ephemeral_pub, encoded) = generate_companion_ephemeral_key()?;
    let jid = Jid::new(digits, server::DEFAULT_USER);

    let companion_reg = build_companion_hello_node(
        &jid,
        wrapped_ephemeral_pub,
        &client.device.noise_key.public,
        client_type,
        client_display_name,
        show_push_notification,
    );

    let resp = client
        .send_iq(
            InfoQuery::new("md", IqType::Set)
                .to(Jid::new("", server::DEFAULT_USER))
                .content(Value::Nodes(vec![companion_reg])),
        )
        .await?;

    // Confirm the response shape — even though we don't *need* the ref to
    // return the linking code, surfacing a malformed reply early helps.
    let _ = resp
        .child_by_tag(&["link_code_companion_reg", "link_code_pairing_ref"])
        .ok_or_else(|| ClientError::Malformed("missing link_code_pairing_ref".into()))?;

    Ok(format!("{}-{}", &encoded[0..4], &encoded[4..]))
}

// ---------------------------------------------------------------------------
// TODO: companion_finish stage
// ---------------------------------------------------------------------------
//
// Upstream's `handleCodePairNotification` in `pair-code.go` (lines 143-245)
// implements the second half: when the primary device approves the pairing,
// the server pushes a `<notification>` carrying:
//   - link_code_pairing_ref         (echo of the ref we got back)
//   - link_code_pairing_wrapped_primary_ephemeral_pub  (80 bytes: salt|iv|ct)
//   - primary_identity_pub
//
// We then:
//   1. PBKDF2-derive the AES-CTR key from the same 8-char code,
//   2. decrypt the primary's ephemeral pubkey,
//   3. X25519 with our ephemeral private → ephemeral_shared_secret,
//   4. HKDF the shared secret with label "link_code_pairing_key_bundle_encryption_key",
//   5. AES-256-GCM-seal `[our_identity_pub || primary_identity_pub || adv_random]`,
//   6. X25519(our_identity_priv, primary_identity_pub) → identity_shared_key,
//   7. HKDF([ephemeral_shared || identity_shared || adv_random], info="adv_secret")
//      → store as `device.adv_secret_key`,
//   8. send `<iq><link_code_companion_reg stage="companion_finish">` with the
//      wrapped key bundle + our identity_pub + the pairing ref.
//
// This requires:
//   - A short-lived "phone linking cache" on the Client (jid, ephemeral
//     keypair, encoded code, pairing_ref) wired through `pair_phone`,
//   - HKDF-SHA256 (already available via `wha_crypto::hkdf_sha256`),
//   - GCM seal (already available via `wha_crypto::gcm_encrypt`),
//   - `KeyPair::diffie_hellman` (X25519) — we already use it in the noise
//     handshake.
//
// Once the cache is plumbed through Client, `handle_code_pair_notification`
// should live in this module and be dispatched from `notification.rs` when a
// `<notification>` carrying `<link_code_companion_reg>` arrives.

/// Stub: `companion_finish` notification handler.
///
/// Upstream's `Client.handleCodePairNotification` runs once the primary
/// device approves the 8-character code. Without a Client-side cache for the
/// pending pairing (ephemeral keypair + linking code + pairing ref) we cannot
/// run the X25519 + HKDF + GCM-seal pipeline that produces the
/// `companion_finish` IQ.
///
/// Returns `ClientError::NotImplemented` until the cache + key derivation is
/// wired through. The IQ-shape tests below pin the `companion_hello` shape we
/// already do support.
pub async fn handle_code_pair_notification(
    _client: &Client,
    _notification: &Node,
) -> Result<(), ClientError> {
    Err(ClientError::NotImplemented(
        "pair_code::handle_code_pair_notification (companion_finish stage)",
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_type_platform_id_matches_upstream_constants() {
        // Upstream's `PairClientType` is iota-indexed starting at 0:
        //   PairClientUnknown=0, Chrome=1, Edge=2, ..., OtherWebClient=9.
        assert_eq!(ClientType::Unknown.as_platform_id(), "0");
        assert_eq!(ClientType::Chrome.as_platform_id(), "1");
        assert_eq!(ClientType::Edge.as_platform_id(), "2");
        assert_eq!(ClientType::Firefox.as_platform_id(), "3");
        assert_eq!(ClientType::IE.as_platform_id(), "4");
        assert_eq!(ClientType::Opera.as_platform_id(), "5");
        assert_eq!(ClientType::Safari.as_platform_id(), "6");
        assert_eq!(ClientType::Electron.as_platform_id(), "7");
        assert_eq!(ClientType::UWP.as_platform_id(), "8");
        assert_eq!(ClientType::OtherWebClient.as_platform_id(), "9");
    }

    #[test]
    fn normalise_phone_strips_punctuation_and_validates() {
        // Punctuation stripped, digits preserved.
        assert_eq!(
            normalise_phone_number("+49 (152) 1234-5678").unwrap(),
            "4915212345678"
        );
        // Too short.
        let r = normalise_phone_number("12345");
        assert!(matches!(r, Err(ClientError::Malformed(_))));
        // Leading zero rejected (must be international format).
        let r = normalise_phone_number("0049123456789");
        assert!(matches!(r, Err(ClientError::Malformed(_))));
        // Exactly 7 digits is valid (>6).
        assert!(normalise_phone_number("1234567").is_ok());
    }

    #[test]
    fn linking_base32_encode_uses_only_alphabet_chars_and_eight_long() {
        // Spot-check: known input → known indices → known characters.
        // alphabet '123456789ABCDEFGHJKLMNPQRSTVWXYZ' has 32 entries.
        let encoded = linking_base32_encode(&[0x00, 0x44, 0x32, 0x14, 0xc7]);
        assert_eq!(encoded.len(), 8);
        assert!(encoded.chars().all(|c| LINKING_BASE32.contains(&(c as u8))));
        // All-zero input → 8 copies of the alphabet's first character ('1').
        let zero = linking_base32_encode(&[0; 5]);
        assert_eq!(zero, "11111111");
    }

    #[test]
    fn pbkdf2_first_iter_matches_hmac() {
        // PBKDF2 with one iteration is just HMAC(pw, salt || INT32BE(1))
        // truncated to the desired length.
        let pw = b"PASSWORD";
        let salt = b"NaCl";
        let out = pbkdf2_hmac_sha256(pw, salt, 1);
        let mut block_input = Vec::from(&salt[..]);
        block_input.extend_from_slice(&1u32.to_be_bytes());
        let mac = hmac_sha256(pw, &block_input);
        assert_eq!(&out[..], &mac[..32]);
    }

    #[test]
    fn generate_companion_ephemeral_key_shapes_are_correct() {
        let (kp, wrapped, code) = generate_companion_ephemeral_key().unwrap();
        // Wrapped pubkey is salt(32) || iv(16) || ciphertext(32) = 80 bytes.
        assert_eq!(wrapped.len(), 80);
        // Ephemeral keypair has 32-byte X25519 keys.
        assert_eq!(kp.public.len(), 32);
        // Encoded code is 8 chars, all from the WhatsApp linking alphabet.
        assert_eq!(code.len(), 8);
        assert!(code.bytes().all(|b| LINKING_BASE32.contains(&b)));
        // The wrapped pubkey is NOT the plaintext pubkey (it's CTR-encrypted).
        assert_ne!(&wrapped[48..80], &kp.public[..]);
    }

    #[test]
    fn build_companion_hello_node_has_upstream_shape() {
        // Pin the IQ shape against upstream's pair-code.go::PairPhone.
        let jid = Jid::new("4915212345678", server::DEFAULT_USER);
        let wrapped = vec![0x42; 80];
        let server_auth = [0x37u8; 32];
        let node = build_companion_hello_node(
            &jid,
            wrapped.clone(),
            &server_auth,
            ClientType::Chrome,
            "Chrome (Linux)",
            true,
        );

        // Outer tag + attrs.
        assert_eq!(node.tag, "link_code_companion_reg");
        assert_eq!(node.get_attr_str("stage"), Some("companion_hello"));
        assert_eq!(
            node.get_attr_str("should_show_push_notification"),
            Some("true")
        );
        assert_eq!(node.get_attr_jid("jid"), Some(&jid));

        // Children, in upstream order.
        let kids = node.children();
        assert_eq!(kids.len(), 5);
        assert_eq!(
            kids[0].tag,
            "link_code_pairing_wrapped_companion_ephemeral_pub_key"
        );
        assert_eq!(kids[0].content, Value::Bytes(wrapped));
        assert_eq!(kids[1].tag, "companion_server_auth_key_pub");
        assert_eq!(kids[1].content, Value::Bytes(server_auth.to_vec()));
        assert_eq!(kids[2].tag, "companion_platform_id");
        assert_eq!(kids[2].content, Value::String("1".into())); // Chrome=1
        assert_eq!(kids[3].tag, "companion_platform_display");
        assert_eq!(kids[3].content, Value::String("Chrome (Linux)".into()));
        assert_eq!(kids[4].tag, "link_code_pairing_nonce");
        assert_eq!(kids[4].content, Value::Bytes(vec![0]));
    }

    #[test]
    fn build_companion_hello_node_show_push_false_serialises_false() {
        let jid = Jid::new("4915212345678", server::DEFAULT_USER);
        let node = build_companion_hello_node(
            &jid,
            vec![0; 80],
            &[0; 32],
            ClientType::Safari,
            "Safari (macOS)",
            false,
        );
        assert_eq!(
            node.get_attr_str("should_show_push_notification"),
            Some("false")
        );
        assert_eq!(node.children()[2].content, Value::String("6".into()));
    }

    #[tokio::test]
    async fn pair_phone_without_connection_fails_cleanly() {
        use std::sync::Arc;
        use wha_store::MemoryStore;
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = Client::new(device);
        // No live socket → send_iq → send_node → NotConnected.
        let r = pair_phone(
            &cli,
            "+49 152 1234 5678",
            true,
            ClientType::Chrome,
            "Chrome (Linux)",
        )
        .await;
        assert!(matches!(r, Err(ClientError::NotConnected)));
    }

    #[tokio::test]
    async fn handle_code_pair_notification_returns_not_implemented() {
        // The companion_finish handler is a documented stub until the phone
        // linking cache is wired through. Pin that contract so future work
        // doesn't silently change the signature.
        use std::sync::Arc;
        use wha_store::MemoryStore;
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = Client::new(device);
        let node = Node::tag_only("notification");
        let r = handle_code_pair_notification(&cli, &node).await;
        assert!(matches!(r, Err(ClientError::NotImplemented(_))));
    }
}
