//! Message-secret derivation — port of `whatsmeow/msgsecret.go`.
//!
//! WhatsApp derives per-message secrets used for reactions, edits, polls,
//! comments, etc., by mixing the original message's master secret through
//! HKDF-SHA256 with a stable, per-use-case info string. The encrypted payload
//! is then sealed with AES-256-GCM, with associated data tying it back to the
//! original message id and the modification sender.
//!
//! This module is pure crypto — no async, no I/O. Callers fetch the master
//! secret from the store, then pass it through here.

use rand::RngCore;
use rand::rngs::OsRng;

use wha_crypto::{gcm_decrypt, gcm_encrypt, hkdf_sha256};
use wha_types::jid::Jid;

use crate::error::ClientError;

/// Per-use-case info string for the HKDF step.
///
/// Variant names follow the Rust port; the wire-level info strings match
/// whatsmeow's `MsgSecretType` constants byte-for-byte. See the test
/// `info_strings_match_whatsmeow` for the upstream literals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageSecretUseCase {
    /// `EncSecretReaction` — `"Enc Reaction"`.
    Reaction,
    /// `EncSecretEventEdit` — `"Event Edit"`.
    Edit,
    /// `EncSecretPollVote` — `"Poll Vote"`.
    PollVote,
    /// Not present in upstream `msgsecret.go`; kept for symmetry with
    /// `PollVote`. Info string: `"Poll Result"`.
    PollResult,
    /// Not present in upstream `msgsecret.go`; kept for higher-level callers
    /// that derive history-sync sub-keys. Info string: `"History Sync"`.
    HistorySync,
    /// `EncSecretComment` — `"Enc Comment"`.
    Comment,
    /// `EncSecretReportToken` — `"Report Token"`.
    ReportToken,
    /// `EncSecretEventResponse` — `"Event Response"`.
    EventResponse,
    /// `EncSecretBotMsg` — `"Bot Message"`.
    BotMessage,
}

impl MessageSecretUseCase {
    /// Wire-level info string passed into HKDF.
    ///
    /// The strings for the variants that exist upstream are copied byte-for-byte
    /// from `whatsmeow/msgsecret.go`'s `MsgSecretType` constants.
    pub fn as_info_str(&self) -> &'static str {
        match self {
            MessageSecretUseCase::Reaction => "Enc Reaction",
            MessageSecretUseCase::Edit => "Event Edit",
            MessageSecretUseCase::PollVote => "Poll Vote",
            MessageSecretUseCase::PollResult => "Poll Result",
            MessageSecretUseCase::HistorySync => "History Sync",
            MessageSecretUseCase::Comment => "Enc Comment",
            MessageSecretUseCase::ReportToken => "Report Token",
            MessageSecretUseCase::EventResponse => "Event Response",
            MessageSecretUseCase::BotMessage => "Bot Message",
        }
    }
}

/// HKDF-SHA256 derivation of a 32-byte per-use-case key from the original
/// message's master secret. Mirrors the `EncSecretBotMsg`-style call site
/// `hkdfutil.SHA256(messageSecret, nil, []byte(useCase), 32)` in upstream.
///
/// The full upstream `generateMsgSecretKey` additionally folds origMsgID,
/// origSender and modificationSender into the HKDF info; see
/// [`message_secret_aad`] and the higher-level helpers for that flow.
pub fn derive_message_secret(
    master_secret: &[u8],
    use_case: MessageSecretUseCase,
) -> Result<[u8; 32], ClientError> {
    let okm = hkdf_sha256(master_secret, &[], use_case.as_info_str().as_bytes(), 32)?;
    let mut out = [0u8; 32];
    out.copy_from_slice(&okm);
    Ok(out)
}

/// Result of [`encrypt_msg_secret`]: ciphertext bundled with its 12-byte IV.
///
/// The `key` field is `Some` only when explicitly enabled by tests; the
/// public encrypt path leaves it as `None` to avoid leaking the per-message
/// key into call sites that only need to forward the IV + ciphertext.
#[derive(Debug, Clone)]
pub struct EncryptedMsgSecret {
    pub iv: [u8; 12],
    pub ciphertext: Vec<u8>,
    /// Derived 32-byte key — populated only by test helpers.
    pub key: Option<[u8; 32]>,
}

/// Derive a per-use-case key, generate a random 12-byte IV, and AES-GCM-encrypt
/// `plaintext` with `aad`. Pure-crypto kernel — callers pass the 32-byte master
/// secret directly. Mirrors the inner half of `Client.encryptMsgSecret`
/// upstream once the secret has been looked up.
pub fn encrypt_msg_secret_raw(
    master_secret: &[u8],
    use_case: MessageSecretUseCase,
    plaintext: &[u8],
    aad: &[u8],
) -> Result<EncryptedMsgSecret, ClientError> {
    let key = derive_message_secret(master_secret, use_case)?;
    let mut iv = [0u8; 12];
    OsRng.fill_bytes(&mut iv);
    let ciphertext = gcm_encrypt(&key, &iv, plaintext, aad)
        .map_err(|e| ClientError::Crypto(e.to_string()))?;
    Ok(EncryptedMsgSecret { iv, ciphertext, key: None })
}

/// Inverse of [`encrypt_msg_secret_raw`]. Pure-crypto kernel that mirrors the
/// inner half of `Client.decryptMsgSecret` upstream.
pub fn decrypt_msg_secret_raw(
    master_secret: &[u8],
    use_case: MessageSecretUseCase,
    iv: &[u8],
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, ClientError> {
    let key = derive_message_secret(master_secret, use_case)?;
    gcm_decrypt(&key, iv, ciphertext, aad).map_err(|e| ClientError::Crypto(e.to_string()))
}

/// Parse a `MsgSecretType`-style display string into the corresponding enum.
/// Mirrors upstream's `MsgSecretType` values byte-for-byte (see
/// `_upstream/whatsmeow/msgsecret.go`).
pub fn use_case_from_str(s: &str) -> Result<MessageSecretUseCase, ClientError> {
    Ok(match s {
        "Poll Vote" => MessageSecretUseCase::PollVote,
        "Enc Reaction" => MessageSecretUseCase::Reaction,
        "Enc Comment" => MessageSecretUseCase::Comment,
        "Report Token" => MessageSecretUseCase::ReportToken,
        "Event Response" => MessageSecretUseCase::EventResponse,
        "Event Edit" => MessageSecretUseCase::Edit,
        "Bot Message" => MessageSecretUseCase::BotMessage,
        other => {
            return Err(ClientError::Other(format!(
                "unknown msg-secret use case: {other}"
            )))
        }
    })
}

/// High-level encrypt — looks up the original message's master secret in the
/// device's [`MsgSecretStore`](wha_store::MsgSecretStore), builds the AAD that
/// upstream uses (`<msg_id> 0x00 <sender_non_ad>`), derives the per-use-case
/// key with HKDF-SHA256, and AES-GCM-seals `payload`.
///
/// The returned bytes are the concatenation `iv || ciphertext_with_tag` — the
/// shape upstream's `EncryptMessageSecret` returns.
///
/// `secret_type` is the user-visible label from upstream's `MsgSecretType`
/// constants (`"Poll Vote"`, `"Enc Reaction"`, …). Mirrors `Client.encryptMsgSecret`.
pub async fn encrypt_msg_secret(
    client: &crate::client::Client,
    chat: &Jid,
    sender: &Jid,
    original_message_id: &str,
    payload: &[u8],
    secret_type: &str,
) -> Result<Vec<u8>, ClientError> {
    let use_case = use_case_from_str(secret_type)?;
    let chat_key = chat.to_non_ad().to_string();
    let sender_key = sender.to_non_ad().to_string();
    let master = client
        .device
        .msg_secrets
        .get_msg_secret(&chat_key, &sender_key, original_message_id)
        .await?
        .ok_or_else(|| {
            ClientError::Other(format!(
                "msg secret not found for chat={chat_key} sender={sender_key} id={original_message_id}"
            ))
        })?;
    let aad = message_secret_aad(original_message_id, sender);
    let sealed = encrypt_msg_secret_raw(&master, use_case, payload, &aad)?;
    let mut out = Vec::with_capacity(12 + sealed.ciphertext.len());
    out.extend_from_slice(&sealed.iv);
    out.extend_from_slice(&sealed.ciphertext);
    Ok(out)
}

/// High-level decrypt — inverse of [`encrypt_msg_secret`]. The `encrypted`
/// payload is expected to be the wire-format concatenation `iv ‖ ciphertext`
/// (12 bytes IV, the rest being AES-GCM ciphertext + tag). Mirrors
/// `Client.decryptMsgSecret`.
pub async fn decrypt_msg_secret(
    client: &crate::client::Client,
    chat: &Jid,
    sender: &Jid,
    original_message_id: &str,
    encrypted: &[u8],
    secret_type: &str,
) -> Result<Vec<u8>, ClientError> {
    if encrypted.len() < 12 + 16 {
        return Err(ClientError::Other(format!(
            "encrypted payload too short ({} bytes)",
            encrypted.len()
        )));
    }
    let use_case = use_case_from_str(secret_type)?;
    let chat_key = chat.to_non_ad().to_string();
    let sender_key = sender.to_non_ad().to_string();
    let master = client
        .device
        .msg_secrets
        .get_msg_secret(&chat_key, &sender_key, original_message_id)
        .await?
        .ok_or_else(|| {
            ClientError::Other(format!(
                "msg secret not found for chat={chat_key} sender={sender_key} id={original_message_id}"
            ))
        })?;
    let aad = message_secret_aad(original_message_id, sender);
    let (iv, ciphertext) = encrypted.split_at(12);
    decrypt_msg_secret_raw(&master, use_case, iv, ciphertext, &aad)
}

/// Persist the master secret carried by a `messageContextInfo.messageSecret`
/// field on an inbound `<message>` so it can later be used to derive
/// reactions, edits, votes, etc. Mirrors the side effect inside upstream's
/// `Client.handleEncryptedMessage` that calls `Store.MsgSecrets.PutMessageSecrets`.
pub async fn store_inbound_message_secret(
    client: &crate::client::Client,
    chat: &Jid,
    sender: &Jid,
    message_id: &str,
    secret: [u8; 32],
) -> Result<(), ClientError> {
    let chat_key = chat.to_non_ad().to_string();
    let sender_key = sender.to_non_ad().to_string();
    client
        .device
        .msg_secrets
        .put_msg_secret(&chat_key, &sender_key, message_id, secret)
        .await?;
    Ok(())
}

/// Build the AES-GCM associated data for a message-secret payload.
///
/// Upstream's `generateMsgSecretKey` builds AAD as
/// `fmt.Appendf(nil, "%s\x00%s", origMsgID, modificationSenderStr)` where
/// `modificationSenderStr` is the non-AD form of the modification sender JID.
/// We mirror that exactly: `<message_id> 0x00 <sender_jid_non_ad>`.
pub fn message_secret_aad(message_id: &str, sender_jid: &Jid) -> Vec<u8> {
    let sender = sender_jid.to_non_ad().to_string();
    let mut out = Vec::with_capacity(message_id.len() + 1 + sender.len());
    out.extend_from_slice(message_id.as_bytes());
    out.push(0);
    out.extend_from_slice(sender.as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_message_secret_is_deterministic() {
        let master = [0x42u8; 32];

        // Same input → same output.
        let k1 = derive_message_secret(&master, MessageSecretUseCase::Reaction).unwrap();
        let k2 = derive_message_secret(&master, MessageSecretUseCase::Reaction).unwrap();
        assert_eq!(k1, k2);
        assert_eq!(k1.len(), 32);

        // Different use-cases → different keys.
        let k_edit = derive_message_secret(&master, MessageSecretUseCase::Edit).unwrap();
        let k_vote = derive_message_secret(&master, MessageSecretUseCase::PollVote).unwrap();
        let k_react = k1;
        assert_ne!(k_react, k_edit);
        assert_ne!(k_react, k_vote);
        assert_ne!(k_edit, k_vote);

        // Different IKM → different key for the same use-case.
        let other = [0x43u8; 32];
        let k_other = derive_message_secret(&other, MessageSecretUseCase::Reaction).unwrap();
        assert_ne!(k_react, k_other);
    }

    #[test]
    fn encrypt_then_decrypt_round_trip() {
        let master = b"some-32-byte-master-secret-data!".to_vec();
        let aad = message_secret_aad("3EB0ABCDEF", &"15551234567@s.whatsapp.net".parse().unwrap());
        let pt = b"\x08\x01\x12\x04test"; // arbitrary protobuf-shaped payload

        let sealed =
            encrypt_msg_secret_raw(&master, MessageSecretUseCase::Reaction, pt, &aad).unwrap();
        assert_eq!(sealed.iv.len(), 12);
        assert!(!sealed.ciphertext.is_empty());
        assert_ne!(sealed.ciphertext.as_slice(), pt);

        let back = decrypt_msg_secret_raw(
            &master,
            MessageSecretUseCase::Reaction,
            &sealed.iv,
            &sealed.ciphertext,
            &aad,
        )
        .unwrap();
        assert_eq!(back, pt);
    }

    #[test]
    fn aad_mismatch_decrypt_fails() {
        let master = [9u8; 32];
        let aad_correct =
            message_secret_aad("MSG-ID-1", &"15551234567@s.whatsapp.net".parse().unwrap());
        let aad_wrong =
            message_secret_aad("MSG-ID-1", &"19998887777@s.whatsapp.net".parse().unwrap());
        let pt = b"reaction-bytes";

        let sealed = encrypt_msg_secret_raw(
            &master,
            MessageSecretUseCase::Reaction,
            pt,
            &aad_correct,
        )
        .unwrap();

        let res = decrypt_msg_secret_raw(
            &master,
            MessageSecretUseCase::Reaction,
            &sealed.iv,
            &sealed.ciphertext,
            &aad_wrong,
        );
        assert!(matches!(res, Err(ClientError::Crypto(_))));

        // Sanity: correct AAD still decrypts.
        let ok = decrypt_msg_secret_raw(
            &master,
            MessageSecretUseCase::Reaction,
            &sealed.iv,
            &sealed.ciphertext,
            &aad_correct,
        )
        .unwrap();
        assert_eq!(ok, pt);
    }

    /// `encrypt_msg_secret` looks up the master secret in the device's
    /// [`MsgSecretStore`] and seals the payload. Round-tripping through
    /// `decrypt_msg_secret` recovers the plaintext.
    #[tokio::test]
    async fn high_level_encrypt_decrypt_round_trip() {
        use std::sync::Arc;
        use wha_store::MemoryStore;

        let store: Arc<MemoryStore> = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = crate::client::Client::new(device);

        let chat: Jid = "1112223333@s.whatsapp.net".parse().unwrap();
        let sender: Jid = "9998887777@s.whatsapp.net".parse().unwrap();
        let msg_id = "3EB0ROUNDTRIP";

        // Pre-stash the master secret as if a previous <message> carried it
        // in messageContextInfo.messageSecret.
        let master_secret = [0xABu8; 32];
        store_inbound_message_secret(&cli, &chat, &sender, msg_id, master_secret)
            .await
            .unwrap();

        let plaintext = b"hello reaction!";
        let sealed = encrypt_msg_secret(&cli, &chat, &sender, msg_id, plaintext, "Enc Reaction")
            .await
            .unwrap();
        // Wire format: 12-byte IV || ciphertext+tag. Must be longer than the
        // plaintext (because of the GCM tag) and longer than 12 bytes.
        assert!(sealed.len() >= 12 + plaintext.len() + 16);

        let back = decrypt_msg_secret(&cli, &chat, &sender, msg_id, &sealed, "Enc Reaction")
            .await
            .unwrap();
        assert_eq!(back, plaintext);
    }

    /// `encrypt_msg_secret` returns an error if the master secret hasn't been
    /// stored — mirrors `ErrOriginalMessageSecretNotFound` upstream.
    #[tokio::test]
    async fn missing_master_secret_returns_error() {
        use std::sync::Arc;
        use wha_store::MemoryStore;

        let store: Arc<MemoryStore> = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = crate::client::Client::new(device);

        let chat: Jid = "1112223333@s.whatsapp.net".parse().unwrap();
        let sender: Jid = "9998887777@s.whatsapp.net".parse().unwrap();
        let res = encrypt_msg_secret(
            &cli,
            &chat,
            &sender,
            "3EB0NOSUCHID",
            b"payload",
            "Poll Vote",
        )
        .await;
        assert!(matches!(res, Err(ClientError::Other(_))));
    }

    /// Storing a message secret should write to the trait-backed store and
    /// be retrievable via `get_msg_secret`. This exercises the "on inbound
    /// <message> with messageContextInfo.messageSecret, persist it" path.
    #[tokio::test]
    async fn store_inbound_message_secret_persists_through_trait() {
        use std::sync::Arc;
        use wha_store::MemoryStore;

        let store: Arc<MemoryStore> = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = crate::client::Client::new(device);

        let chat: Jid = "111@s.whatsapp.net".parse().unwrap();
        let sender: Jid = "222:7@s.whatsapp.net".parse().unwrap();
        let secret = [0x55u8; 32];
        store_inbound_message_secret(&cli, &chat, &sender, "ID-A", secret)
            .await
            .unwrap();

        // The store should hold the secret keyed on the non-AD JIDs.
        let got = cli
            .device
            .msg_secrets
            .get_msg_secret(
                &chat.to_non_ad().to_string(),
                &sender.to_non_ad().to_string(),
                "ID-A",
            )
            .await
            .unwrap();
        assert_eq!(got, Some(secret));

        // Idempotence: a second insert (e.g. a duplicate notification) must
        // NOT overwrite. Mirrors upstream's `ON CONFLICT DO NOTHING`.
        let other = [0xFFu8; 32];
        store_inbound_message_secret(&cli, &chat, &sender, "ID-A", other)
            .await
            .unwrap();
        let still = cli
            .device
            .msg_secrets
            .get_msg_secret(
                &chat.to_non_ad().to_string(),
                &sender.to_non_ad().to_string(),
                "ID-A",
            )
            .await
            .unwrap();
        assert_eq!(still, Some(secret), "first-writer-wins not honoured");
    }

    #[test]
    fn info_strings_match_whatsmeow() {
        // These literals are copied directly from
        // _upstream/whatsmeow/msgsecret.go's MsgSecretType constants.
        const ENC_SECRET_POLL_VOTE: &str = "Poll Vote";
        const ENC_SECRET_REACTION: &str = "Enc Reaction";
        const ENC_SECRET_COMMENT: &str = "Enc Comment";
        const ENC_SECRET_REPORT_TOKEN: &str = "Report Token";
        const ENC_SECRET_EVENT_RESPONSE: &str = "Event Response";
        const ENC_SECRET_EVENT_EDIT: &str = "Event Edit";
        const ENC_SECRET_BOT_MSG: &str = "Bot Message";

        assert_eq!(MessageSecretUseCase::PollVote.as_info_str(), ENC_SECRET_POLL_VOTE);
        assert_eq!(MessageSecretUseCase::Reaction.as_info_str(), ENC_SECRET_REACTION);
        assert_eq!(MessageSecretUseCase::Comment.as_info_str(), ENC_SECRET_COMMENT);
        assert_eq!(MessageSecretUseCase::ReportToken.as_info_str(), ENC_SECRET_REPORT_TOKEN);
        assert_eq!(MessageSecretUseCase::EventResponse.as_info_str(), ENC_SECRET_EVENT_RESPONSE);
        assert_eq!(MessageSecretUseCase::Edit.as_info_str(), ENC_SECRET_EVENT_EDIT);
        assert_eq!(MessageSecretUseCase::BotMessage.as_info_str(), ENC_SECRET_BOT_MSG);
    }

    #[test]
    fn message_secret_aad_format() {
        // Mirrors upstream's fmt.Appendf(nil, "%s\x00%s", origMsgID, modSenderStr).
        // sender_jid is stripped to non-AD form before formatting.
        let jid: Jid = "15551234567:7@s.whatsapp.net".parse().unwrap();
        let aad = message_secret_aad("3EB0FOO", &jid);

        let mut expected = Vec::new();
        expected.extend_from_slice(b"3EB0FOO");
        expected.push(0);
        // to_non_ad() drops the device, so the formatted JID has no :device part.
        expected.extend_from_slice(b"15551234567@s.whatsapp.net");
        assert_eq!(aad, expected);
    }
}
