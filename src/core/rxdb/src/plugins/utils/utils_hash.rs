//! Hashing utilities.
//!
//! Upstream uses `crypto.subtle.digest('SHA-256', ...)`; we use the `sha2` crate.

use sha2::{Digest, Sha256};

// ref: rxdb/src/plugins/utils/utils-hash.ts:32-43
/// SHA-256 of a string, hex-encoded.
pub fn native_sha256(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for byte in result.iter() {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

// ref: rxdb/src/plugins/utils/utils-hash.ts:45
pub fn default_hash_sha256(input: &str) -> String {
    native_sha256(input)
}

// ref: rxdb/src/plugins/utils/utils-hash.ts:48-56
pub fn hash_string_to_number(s: &str) -> i32 {
    let mut nr: i32 = 0;
    for c in s.chars() {
        nr = nr.wrapping_add(c as i32);
        // Convert to 32-bit integer (upstream `nr |= 0`), already i32 in Rust.
    }
    nr
}
