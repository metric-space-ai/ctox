//! Noise XX handshake driver. Mirrors `whatsmeow/handshake.go`.
//!
//! Flow:
//! 1. Generate ephemeral keypair
//! 2. Send `ClientHello { Ephemeral: epk.pub }`
//! 3. Receive `ServerHello { Ephemeral, Static, Payload (= cert chain) }`
//! 4. Authenticate hash, mix(eph_priv, server_eph_pub), decrypt server static
//! 5. mix(eph_priv, server_static), decrypt server cert + verify
//! 6. Encrypt our noise pubkey, mix(noise_priv, server_eph_pub)
//! 7. Encrypt the client payload (proto)
//! 8. Send `ClientFinish { Static, Payload }`
//! 9. `nh.finish()` → `(write_key, read_key)` for the post-handshake socket
//!
//! The leaf-cert verification matches whatsmeow's `verifyServerCert`: we
//! verify the intermediate cert's signature against the bundled WA root
//! pubkey, then the leaf cert against the intermediate's, and finally
//! compare the leaf cert's key to the decrypted server static.

use prost::Message;

use wha_crypto::KeyPair;
use wha_proto::{cert as wa_cert, wa6};
use wha_socket::{NoiseHandshake, NoiseSocket, FRAME_LENGTH_SIZE, NOISE_START_PATTERN, WA_CONN_HEADER};

use crate::error::ClientError;

/// Bundled root public key the server's intermediate cert is signed under.
/// Mirrors `whatsmeow.WACertPubKey`.
pub const WA_CERT_PUB_KEY: [u8; 32] = [
    0x14, 0x23, 0x75, 0x57, 0x4D, 0x0A, 0x58, 0x71, 0x66, 0xAA, 0xE7, 0x1E, 0xBE, 0x51, 0x64, 0x37,
    0xC4, 0xA2, 0x8B, 0x73, 0xE3, 0x69, 0x5C, 0x6C, 0xE1, 0xF7, 0xF9, 0x54, 0x5D, 0xA8, 0xEE, 0x6B,
];
pub const WA_CERT_ISSUER_SERIAL: u32 = 0;

/// Build the first (`ClientHello`) handshake frame and the initialised
/// [`NoiseHandshake`] state. The caller sends the bytes through the frame
/// socket, then waits for `ServerHello` and resumes the dance with
/// [`process_server_hello`].
pub fn build_client_hello(ephemeral: &KeyPair) -> Result<(NoiseHandshake, Vec<u8>), ClientError> {
    let mut nh = NoiseHandshake::new();
    nh.start(NOISE_START_PATTERN, &WA_CONN_HEADER);
    nh.authenticate(&ephemeral.public);

    let msg = wa6::HandshakeMessage {
        client_hello: Some(wa6::handshake_message::ClientHello {
            ephemeral: Some(ephemeral.public.to_vec()),
            ..Default::default()
        }),
        server_hello: None,
        client_finish: None,
    };
    let mut buf = Vec::with_capacity(64);
    msg.encode(&mut buf)?;
    Ok((nh, buf))
}

/// Continue the dance after receiving the server hello. Returns the bytes the
/// caller must send (the `ClientFinish` frame) plus the [`NoiseSocket`] that
/// will speak the post-handshake AEAD transport.
pub fn finish_handshake(
    nh: &mut NoiseHandshake,
    ephemeral: &KeyPair,
    noise_static: &KeyPair,
    server_hello_bytes: &[u8],
    client_payload: &[u8],
) -> Result<(Vec<u8>, NoiseSocket), ClientError> {
    let resp = wa6::HandshakeMessage::decode(server_hello_bytes)?;
    let sh = resp
        .server_hello
        .ok_or_else(|| ClientError::Handshake("missing server hello".into()))?;
    let server_eph = sh
        .ephemeral
        .ok_or_else(|| ClientError::Handshake("missing server ephemeral".into()))?;
    let server_static = sh
        .r#static
        .ok_or_else(|| ClientError::Handshake("missing server static".into()))?;
    let server_cert = sh
        .payload
        .ok_or_else(|| ClientError::Handshake("missing server cert".into()))?;
    if server_eph.len() != 32 {
        return Err(ClientError::Handshake("bad server ephemeral length".into()));
    }
    let mut server_eph_arr = [0u8; 32];
    server_eph_arr.copy_from_slice(&server_eph);

    nh.authenticate(&server_eph);
    nh.mix_shared_secret(ephemeral, &server_eph_arr)?;

    let static_decrypted = nh.decrypt(&server_static)?;
    if static_decrypted.len() != 32 {
        return Err(ClientError::Handshake("server static is not 32 bytes".into()));
    }
    let mut server_static_arr = [0u8; 32];
    server_static_arr.copy_from_slice(&static_decrypted);
    nh.mix_shared_secret(ephemeral, &server_static_arr)?;

    let cert_decrypted = nh.decrypt(&server_cert)?;
    verify_server_cert(&cert_decrypted, &static_decrypted)?;

    let encrypted_pubkey = nh.encrypt(&noise_static.public)?;
    nh.mix_shared_secret(noise_static, &server_eph_arr)?;

    let encrypted_payload = nh.encrypt(client_payload)?;

    let finish = wa6::HandshakeMessage {
        client_hello: None,
        server_hello: None,
        client_finish: Some(wa6::handshake_message::ClientFinish {
            r#static: Some(encrypted_pubkey),
            payload: Some(encrypted_payload),
            ..Default::default()
        }),
    };
    let mut buf = Vec::new();
    finish.encode(&mut buf)?;

    let (write_key, read_key) = nh.finish()?;
    Ok((buf, NoiseSocket::new(write_key, read_key)))
}

/// Verify the server's noise certificate chain. Mirrors `verifyServerCert`.
pub fn verify_server_cert(cert_decrypted: &[u8], server_static: &[u8]) -> Result<(), ClientError> {
    let chain = wa_cert::CertChain::decode(cert_decrypted)?;

    let intermediate = chain
        .intermediate
        .ok_or_else(|| ClientError::Handshake("missing intermediate cert".into()))?;
    let leaf = chain.leaf.ok_or_else(|| ClientError::Handshake("missing leaf cert".into()))?;

    let intermediate_details = intermediate
        .details
        .ok_or_else(|| ClientError::Handshake("missing intermediate details".into()))?;
    let intermediate_sig = intermediate
        .signature
        .ok_or_else(|| ClientError::Handshake("missing intermediate signature".into()))?;
    let leaf_details_raw = leaf
        .details
        .ok_or_else(|| ClientError::Handshake("missing leaf details".into()))?;
    let leaf_sig = leaf
        .signature
        .ok_or_else(|| ClientError::Handshake("missing leaf signature".into()))?;

    if intermediate_sig.len() != 64 {
        return Err(ClientError::Handshake("intermediate sig not 64 bytes".into()));
    }
    if leaf_sig.len() != 64 {
        return Err(ClientError::Handshake("leaf sig not 64 bytes".into()));
    }

    let mut isig = [0u8; 64];
    isig.copy_from_slice(&intermediate_sig);
    KeyPair::verify(&WA_CERT_PUB_KEY, &intermediate_details, &isig)
        .map_err(|e| ClientError::Handshake(format!("intermediate cert verify: {e}")))?;

    let intermediate_decoded =
        wa_cert::cert_chain::noise_certificate::Details::decode(&intermediate_details[..])?;
    if intermediate_decoded.issuer_serial.unwrap_or(0) != WA_CERT_ISSUER_SERIAL {
        return Err(ClientError::Handshake("unexpected intermediate issuer serial".into()));
    }
    let intermediate_key = intermediate_decoded
        .key
        .ok_or_else(|| ClientError::Handshake("intermediate has no key".into()))?;
    if intermediate_key.len() != 32 {
        return Err(ClientError::Handshake("intermediate key not 32 bytes".into()));
    }
    let mut ikey = [0u8; 32];
    ikey.copy_from_slice(&intermediate_key);

    let mut lsig = [0u8; 64];
    lsig.copy_from_slice(&leaf_sig);
    KeyPair::verify(&ikey, &leaf_details_raw, &lsig)
        .map_err(|e| ClientError::Handshake(format!("leaf cert verify: {e}")))?;

    let leaf_decoded =
        wa_cert::cert_chain::noise_certificate::Details::decode(&leaf_details_raw[..])?;
    if leaf_decoded.issuer_serial.unwrap_or(0) != intermediate_decoded.serial.unwrap_or(0) {
        return Err(ClientError::Handshake("leaf/intermediate serial mismatch".into()));
    }

    let leaf_key = leaf_decoded
        .key
        .ok_or_else(|| ClientError::Handshake("leaf has no key".into()))?;
    if leaf_key.as_slice() != server_static {
        return Err(ClientError::Handshake("cert key doesn't match decrypted static".into()));
    }
    Ok(())
}

// Suppress unused-warning when callers compile with subsets that don't touch
// these constants directly.
#[allow(dead_code)]
const _: usize = FRAME_LENGTH_SIZE;
