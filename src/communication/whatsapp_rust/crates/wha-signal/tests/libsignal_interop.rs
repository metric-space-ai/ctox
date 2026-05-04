//! Cross-language interop test against `go.mau.fi/libsignal`.
//!
//! The hex strings below are produced by `_upstream/gen_signal_vectors/main.go`
//! which calls libsignal directly. Regenerate with:
//!
//! ```sh
//! cd _upstream/gen_signal_vectors && go run main.go
//! ```
//!
//! These vectors are the byte-equality proof that our chain-key advancement,
//! message-key derivation, and root-key DH ratchet step match libsignal.

use wha_crypto::KeyPair;
use wha_signal::{ChainKey, RootKey};

fn hex(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() / 2);
    for chunk in s.as_bytes().chunks(2) {
        out.push((nib(chunk[0]) << 4) | nib(chunk[1]));
    }
    out
}
fn nib(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => 10 + c - b'a',
        b'A'..=b'F' => 10 + c - b'A',
        _ => panic!(),
    }
}
fn arr32(s: &str) -> [u8; 32] {
    let v = hex(s);
    let mut a = [0u8; 32];
    a.copy_from_slice(&v);
    a
}
fn arr16(s: &str) -> [u8; 16] {
    let v = hex(s);
    let mut a = [0u8; 16];
    a.copy_from_slice(&v);
    a
}

/// libsignal-derived test vectors — see `_upstream/gen_signal_vectors/main.go`.
struct ChainStep {
    index: u32,
    chain_key: &'static str,
    cipher_key: &'static str,
    mac_key: &'static str,
    iv: &'static str,
}

const CHAIN_VECTORS: &[ChainStep] = &[
    ChainStep {
        index: 0,
        chain_key: "0707070707070707070707070707070707070707070707070707070707070707",
        cipher_key: "415a1c77fbf8276cd3bf41a429433e752808a319b74f435dc6bc3559d431a3f5",
        mac_key: "2badf7b2797c63f996633fa2dd60a34da0dc6221ddfed529a12a83eec399af57",
        iv: "8004f453ad5f4ddb085fa7da1d9539cc",
    },
    ChainStep {
        index: 1,
        chain_key: "469e03123f1c9c6d5c3ffaa260c7fef9f863c0b6af93f7dcae5c13cbfdadfca5",
        cipher_key: "b969870b5dff65d579e2ef284d0b0ef430a0e840246c46964911d5778791d8c2",
        mac_key: "bd496f65222ebb9b808f7a6f8e99c9836768f8ae05ad8cfa8e9f50cb25b1b236",
        iv: "362688754851a088369777d1e757ebc5",
    },
    ChainStep {
        index: 2,
        chain_key: "ace11b08042c88236e9539159a022f21cf60cf8e8c3bcbf3b2f40c42217d0c0a",
        cipher_key: "3491649b1902baf68042bed85044fd35a594bca781f266b30266d08fd3ff9df8",
        mac_key: "37b1cb6cb881fd1d8ef04c4fba869506142006a9fb1a9254aff85425c61e4d4d",
        iv: "ad7f375ea488e52df2327133fa2237bb",
    },
    ChainStep {
        index: 3,
        chain_key: "cdefcd851176e8388147d7a2936289dfbb8479543d4530b4a6d60df2652bace8",
        cipher_key: "9edff106b6f45198d8f56667ced4183e4191a8160a2db39183870dfc9bcab2f5",
        mac_key: "6685e74c7cc54522d7d01f4ed2c837013a3327954b93d1ba7c70e81702aeffd3",
        iv: "b2a9174fc3b4ecc2b8e6e2d9318360d0",
    },
    ChainStep {
        index: 4,
        chain_key: "c7fe8386ce61ed427c26e1a6a9989a88d7792014df677ef58382f43755edb6b7",
        cipher_key: "59969a690d81a8b182b7b27f1a4cfdd9c4ad052e44bd1394a4cd03645f6cad41",
        mac_key: "08d69d6c2c544e4c4d3bfa11a5ec247fe5afac30685df21afc3697e4e4ee9cf6",
        iv: "dc407f77a269b1e92f46f49a0353da76",
    },
];

#[test]
fn chain_key_advancement_matches_libsignal_byte_for_byte() {
    let mut ck = ChainKey::new(arr32(CHAIN_VECTORS[0].chain_key), 0);
    for step in CHAIN_VECTORS {
        assert_eq!(ck.index, step.index, "index mismatch at step {}", step.index);
        assert_eq!(
            ck.key,
            arr32(step.chain_key),
            "chain_key mismatch at step {}",
            step.index
        );
        let mk = ck.message_keys();
        assert_eq!(
            mk.cipher_key,
            arr32(step.cipher_key),
            "cipher_key mismatch at step {}",
            step.index
        );
        assert_eq!(
            mk.mac_key,
            arr32(step.mac_key),
            "mac_key mismatch at step {}",
            step.index
        );
        assert_eq!(mk.iv, arr16(step.iv), "iv mismatch at step {}", step.index);
        ck = ck.next();
    }
}

#[test]
fn root_key_dh_ratchet_step_matches_libsignal() {
    let alice_priv = arr32("0001010101010101010101010101010101010101010101010101010101010141");
    let bob_pub = arr32("ce8d3ad1ccb633ec7b70c17814a5c76ecd029685050d344745ba05870e587d59");
    let init_root = arr32("0707070707070707070707070707070707070707070707070707070707070707");
    let expected_next_root = arr32("886a63c7cd5972a647cee4ad6508ed42c5eabcdaa6ba79918e8c5707341153a5");
    let expected_chain = arr32("00c6cfc850ddcd40c72b4f9496ccc7ca890b05a921de9c606208438e7b544c17");

    let alice = KeyPair::from_private(alice_priv);
    let rk = RootKey::new(init_root);
    let (next_root, chain) = rk.create_chain(&bob_pub, &alice);
    assert_eq!(next_root.key, expected_next_root, "next_root mismatch");
    assert_eq!(chain.key, expected_chain, "chain_key mismatch");
    assert_eq!(chain.index, 0);
}
