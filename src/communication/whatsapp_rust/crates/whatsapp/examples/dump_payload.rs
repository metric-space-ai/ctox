// Dump our Rust ClientPayload with deterministic keys to /tmp/rust_payload.bin
// for byte-comparison against whatsmeow's output.
//
// Run: cargo run --example dump_payload -p whatsapp

use std::sync::Arc;

use prost::Message;

use wha_client::payload::{build_client_payload, build_registration_payload};
use wha_crypto::{KeyPair, PreKey};
use wha_store::{Device, MemoryStore};

fn clamp(mut b: [u8; 32]) -> [u8; 32] {
    b[0] &= 248;
    b[31] &= 127;
    b[31] |= 64;
    b
}

#[tokio::main]
async fn main() {
    let noise_priv = clamp([0x11u8; 32]);
    let identity_priv = clamp([0x22u8; 32]);
    let signed_pre_key_priv = clamp([0x33u8; 32]);
    let adv = [0x44u8; 32];

    let noise = KeyPair::from_private(noise_priv);
    let identity = KeyPair::from_private(identity_priv);
    let signed_pre_key_kp = KeyPair::from_private(signed_pre_key_priv);
    let signed = PreKey::new(1, signed_pre_key_kp).signed_by(&identity).expect("sign");

    let store = Arc::new(MemoryStore::new());
    let mut device = store.new_device();
    device.noise_key = noise;
    device.identity_key = identity;
    device.signed_pre_key = signed;
    device.adv_secret_key = adv;
    device.registration_id = 12345;
    device.id = None;

    let payload = build_client_payload(&device);
    let mut bytes = Vec::new();
    payload.encode(&mut bytes).expect("encode");

    std::fs::write("/tmp/rust_payload.bin", &bytes).expect("write");
    println!("wrote {} bytes to /tmp/rust_payload.bin", bytes.len());

    // Quick md5 — we just want a fingerprint, don't pull in extra deps:
    // hand-roll via the md-5 crate that wha-client already pulls in.
    use md5::{Digest, Md5};
    let mut h = Md5::new();
    h.update(&bytes);
    let digest = h.finalize();
    println!(
        "sha-ish (md5): {}",
        digest.iter().map(|b| format!("{b:02x}")).collect::<String>()
    );

    // Inspect the same fields the Go program prints.
    let p = build_registration_payload(&device);
    if let Some(dpd) = &p.device_pairing_data {
        println!("--- DevicePairingData ---");
        println!("e_regid: {}", hex(dpd.e_regid.as_deref().unwrap_or(&[])));
        println!("e_keytype: {}", hex(dpd.e_keytype.as_deref().unwrap_or(&[])));
        println!("e_ident: {}", hex(dpd.e_ident.as_deref().unwrap_or(&[])));
        println!("e_skey_id: {}", hex(dpd.e_skey_id.as_deref().unwrap_or(&[])));
        println!("e_skey_val: {}", hex(dpd.e_skey_val.as_deref().unwrap_or(&[])));
        println!("e_skey_sig: {}", hex(dpd.e_skey_sig.as_deref().unwrap_or(&[])));
        println!("build_hash: {}", hex(dpd.build_hash.as_deref().unwrap_or(&[])));
        let dp_bytes = dpd.device_props.as_deref().unwrap_or(&[]);
        println!("device_props ({} bytes): {}", dp_bytes.len(), hex(dp_bytes));
    }
    if let Some(ua) = &p.user_agent {
        println!("--- UserAgent ---");
        println!("{ua:?}");
    }
    if let Some(wi) = &p.web_info {
        println!("--- WebInfo ---");
        println!("{wi:?}");
    }
    println!(
        "connect_type={:?} connect_reason={:?} passive={:?} pull={:?} username={:?} device={:?}",
        p.connect_type, p.connect_reason, p.passive, p.pull, p.username, p.device
    );
    let _ = device;
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
