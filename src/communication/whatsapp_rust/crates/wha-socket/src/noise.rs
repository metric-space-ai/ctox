//! Noise-XX state machine + post-handshake transport socket.
//!
//! This is a faithful port of `whatsmeow/socket/noisehandshake.go` plus
//! `noisesocket.go`. Naming preserved on the public surface, but
//! Rust-idiomatic where it doesn't fight the protocol.

use sha2::{Digest, Sha256};
use wha_crypto::{gcm_decrypt, gcm_encrypt, hkdf_sha256, KeyPair};

use crate::error::SocketError;

/// In-progress Noise XX handshake. Drive it by calling `start`, `authenticate`,
/// `mix_into_key`, `mix_shared_secret_into_key`, `encrypt` and `decrypt` in
/// the order the protocol expects (see whatsmeow/handshake.go for the canonical
/// dance).
pub struct NoiseHandshake {
    pub(crate) hash: [u8; 32],
    pub(crate) salt: [u8; 32],
    /// Current AEAD key (32 bytes, AES-256-GCM).
    pub(crate) key: [u8; 32],
    pub(crate) counter: u32,
}

impl NoiseHandshake {
    pub fn new() -> Self {
        Self { hash: [0u8; 32], salt: [0u8; 32], key: [0u8; 32], counter: 0 }
    }

    /// Initialise the chaining hash + key. The pattern is the well-known XX
    /// constant; if it's already 32 bytes it's used verbatim, otherwise it's
    /// SHA-256'd. Then the connection header is mixed into the hash.
    pub fn start(&mut self, pattern: &[u8], header: &[u8]) {
        if pattern.len() == 32 {
            self.hash.copy_from_slice(pattern);
        } else {
            let h = Sha256::digest(pattern);
            self.hash.copy_from_slice(&h);
        }
        self.salt = self.hash;
        self.key = self.hash;
        self.authenticate(header);
    }

    pub fn authenticate(&mut self, data: &[u8]) {
        let mut h = Sha256::new();
        h.update(self.hash);
        h.update(data);
        let out = h.finalize();
        self.hash.copy_from_slice(&out);
    }

    fn iv(&mut self) -> [u8; 12] {
        let mut iv = [0u8; 12];
        let n = self.counter;
        iv[8..].copy_from_slice(&n.to_be_bytes());
        self.counter += 1;
        iv
    }

    /// AES-GCM encrypt + mix the ciphertext into the chaining hash.
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, SocketError> {
        let iv = self.iv();
        let aad = self.hash;
        let ct = gcm_encrypt(&self.key, &iv, plaintext, &aad)?;
        self.authenticate(&ct);
        Ok(ct)
    }

    /// AES-GCM decrypt + mix the ciphertext into the chaining hash.
    pub fn decrypt(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, SocketError> {
        let iv = self.iv();
        let aad = self.hash;
        let pt = gcm_decrypt(&self.key, &iv, ciphertext, &aad)?;
        self.authenticate(ciphertext);
        Ok(pt)
    }

    /// Mix a 32-byte DH shared secret into the chaining state. Same as
    /// `MixSharedSecretIntoKey` upstream — does X25519 internally.
    pub fn mix_shared_secret(&mut self, ours: &KeyPair, peer_pub: &[u8; 32]) -> Result<(), SocketError> {
        let secret = ours.shared_secret(peer_pub);
        self.mix_into_key(&secret)
    }

    pub fn mix_into_key(&mut self, data: &[u8]) -> Result<(), SocketError> {
        self.counter = 0;
        let (write, read) = self.extract_and_expand(&self.salt.clone(), data)?;
        self.salt = write;
        self.key = read;
        Ok(())
    }

    /// Run the final HKDF and return the two AEAD keys (write key, read key)
    /// used by the post-handshake socket.
    pub fn finish(&self) -> Result<([u8; 32], [u8; 32]), SocketError> {
        self.extract_and_expand(&self.salt, &[])
    }

    fn extract_and_expand(&self, salt: &[u8; 32], data: &[u8]) -> Result<([u8; 32], [u8; 32]), SocketError> {
        let okm = hkdf_sha256(data, salt, &[], 64).map_err(SocketError::Crypto)?;
        let mut write = [0u8; 32];
        let mut read = [0u8; 32];
        write.copy_from_slice(&okm[..32]);
        read.copy_from_slice(&okm[32..]);
        Ok((write, read))
    }
}

impl Default for NoiseHandshake {
    fn default() -> Self {
        Self::new()
    }
}

/// Post-handshake transport. After [`NoiseHandshake::finish`] the caller wires
/// the resulting two keys into one of these via [`NoiseSocket::new`].
pub struct NoiseSocket {
    write_key: [u8; 32],
    read_key: [u8; 32],
    write_counter: u32,
    read_counter: u32,
}

impl NoiseSocket {
    pub fn new(write_key: [u8; 32], read_key: [u8; 32]) -> Self {
        Self { write_key, read_key, write_counter: 0, read_counter: 0 }
    }

    pub fn encrypt_frame(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, SocketError> {
        let mut iv = [0u8; 12];
        iv[8..].copy_from_slice(&self.write_counter.to_be_bytes());
        self.write_counter += 1;
        Ok(gcm_encrypt(&self.write_key, &iv, plaintext, &[])?)
    }

    pub fn decrypt_frame(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, SocketError> {
        let mut iv = [0u8; 12];
        iv[8..].copy_from_slice(&self.read_counter.to_be_bytes());
        self.read_counter += 1;
        Ok(gcm_decrypt(&self.read_key, &iv, ciphertext, &[])?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authenticate_changes_hash_deterministically() {
        let mut a = NoiseHandshake::new();
        let mut b = NoiseHandshake::new();
        a.start(b"Noise_XX_25519_AESGCM_SHA256\x00\x00\x00\x00", &[1, 2, 3]);
        b.start(b"Noise_XX_25519_AESGCM_SHA256\x00\x00\x00\x00", &[1, 2, 3]);
        assert_eq!(a.hash, b.hash);
    }

    #[test]
    fn encrypt_decrypt_round_trip_through_paired_handshakes() {
        // Two parallel handshakes that share the chaining state can encrypt
        // and decrypt each other's frames. This is the property whatsmeow's
        // handshake relies on after each `MixSharedSecretIntoKey`.
        let mut a = NoiseHandshake::new();
        let mut b = NoiseHandshake::new();
        a.start(b"Noise_XX_25519_AESGCM_SHA256\x00\x00\x00\x00", b"WA-test");
        b.start(b"Noise_XX_25519_AESGCM_SHA256\x00\x00\x00\x00", b"WA-test");
        // Mix in identical "shared secret" so both sides end up with the
        // same key without needing X25519 here.
        let secret = [42u8; 32];
        a.mix_into_key(&secret).unwrap();
        b.mix_into_key(&secret).unwrap();

        let pt = b"hello";
        let ct = a.encrypt(pt).unwrap();
        let back = b.decrypt(&ct).unwrap();
        assert_eq!(&back, pt);
    }

    #[test]
    fn noise_socket_round_trip() {
        let mut a = NoiseSocket::new([1u8; 32], [2u8; 32]);
        let mut b = NoiseSocket::new([2u8; 32], [1u8; 32]);
        let ct = a.encrypt_frame(b"ping").unwrap();
        assert_eq!(b.decrypt_frame(&ct).unwrap(), b"ping");
        let ct2 = b.encrypt_frame(b"pong").unwrap();
        assert_eq!(a.decrypt_frame(&ct2).unwrap(), b"pong");
    }
}
