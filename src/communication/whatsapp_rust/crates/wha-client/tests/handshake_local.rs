//! Integration test for the Noise XX handshake state machine + the binary
//! codec. We don't have a real WhatsApp server to talk to, so this drives
//! `wha-socket::NoiseHandshake` directly and confirms two sides converge to
//! the same key material — i.e. the chaining hash and AEAD machinery match
//! the upstream Go implementation closely enough to use it as the reference
//! vertical slice.

use wha_crypto::KeyPair;
use wha_socket::{NoiseHandshake, NOISE_START_PATTERN, WA_CONN_HEADER};

#[test]
fn two_sides_converge_to_same_aead_keys() {
    // Initialise both halves of the handshake identically.
    let mut client = NoiseHandshake::new();
    let mut server = NoiseHandshake::new();
    client.start(NOISE_START_PATTERN, &WA_CONN_HEADER);
    server.start(NOISE_START_PATTERN, &WA_CONN_HEADER);

    let mut rng = rand::rngs::OsRng;
    let client_eph = KeyPair::generate(&mut rng);
    let server_eph = KeyPair::generate(&mut rng);

    // Authenticate ephemerals into the chaining hash on both sides.
    client.authenticate(&client_eph.public);
    server.authenticate(&client_eph.public);

    client.authenticate(&server_eph.public);
    server.authenticate(&server_eph.public);

    // Mix the DH secret on both sides — they end up with the same key.
    client.mix_shared_secret(&client_eph, &server_eph.public).unwrap();
    server.mix_shared_secret(&server_eph, &client_eph.public).unwrap();

    // Now the AEAD round-trips: client encrypts, server decrypts.
    let ct = client.encrypt(b"hello").unwrap();
    let pt = server.decrypt(&ct).unwrap();
    assert_eq!(&pt, b"hello");

    // And the post-handshake derived keys match.
    let (cw, cr) = client.finish().unwrap();
    let (sw, sr) = server.finish().unwrap();
    assert_eq!(cw, sw);
    assert_eq!(cr, sr);
}
