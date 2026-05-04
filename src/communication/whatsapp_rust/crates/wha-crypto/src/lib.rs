//! Cryptographic primitives used by `wha-socket`, `wha-signal` and `wha-client`.
//!
//! The set of operations here is intentionally narrow: only the bits actually
//! used by WhatsApp's wire protocol. The point is to give the upper layers a
//! single import surface and a deterministic-test-friendly API (every stateful
//! type is `Clone` where it makes sense, RNGs are passed in by reference).

pub mod aes_cbc;
pub mod aes_gcm;
pub mod error;
pub mod hkdf;
pub mod hmac;
pub mod keypair;
pub mod media;
pub mod prekey;

pub use aes_cbc::{cbc_decrypt, cbc_encrypt, ctr_xor};
pub use aes_gcm::{gcm_decrypt, gcm_encrypt};
pub use error::CryptoError;
pub use hkdf::hkdf_sha256;
pub use hmac::{hmac_sha256, hmac_sha256_concat, hmac_sha256_verify, hmac_sha512, hmac_sha512_concat};
pub use keypair::{KeyPair, PUBLIC_KEY_LEN, PRIVATE_KEY_LEN};
pub use media::{
    decrypt_media, derive_media_keys, MediaKeys, APP_STATE_INFO, AUDIO_INFO, DOCUMENT_INFO,
    HISTORY_INFO, IMAGE_INFO, LINK_THUMBNAIL_INFO, STICKER_PACK_INFO, VIDEO_INFO,
};
pub use prekey::PreKey;
