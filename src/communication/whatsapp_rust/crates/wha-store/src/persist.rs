//! Device save/load — a hand-rolled binary serialisation format for the
//! persistable subset of [`Device`].
//!
//! Mirrors the semantics of `whatsmeow.Container.PutDevice` /
//! `Container.scanDevice` (see `_upstream/whatsmeow/store/sqlstore/container.go`):
//! a device's persistable state is its identity material, the AD JID it has
//! been paired against, and a few user-visible strings. The trait-object
//! handles to the various per-aggregate stores (`identities`, `sessions`, …)
//! are runtime wiring and therefore live outside the blob — the caller is
//! expected to rebuild a full [`Device`] by combining a freshly decoded
//! [`DeviceBlob`] with `Arc<dyn …Store>` handles from its chosen backend.
//!
//! The wire format is intentionally simple and version-tagged so that future
//! field additions can be introduced behind a bumped magic. Layout:
//!
//! ```text
//! "WDBV"                  : 4 bytes
//! version (=1)            : u8
//! registration_id         : u32 LE
//! noise_key_priv          : 32 bytes
//! identity_key_priv       : 32 bytes
//! signed_pre_key_id       : u32 LE
//! signed_pre_key_priv     : 32 bytes
//! signed_pre_key_sig      : 1 byte present-flag, then 64 bytes if present
//! adv_secret_key          : 32 bytes
//! id (Option<Jid>)        : 1 byte present-flag, then u32 LE length + UTF-8
//! lid (Option<Jid>)       : 1 byte present-flag, then u32 LE length + UTF-8
//! platform                : u32 LE length + UTF-8
//! business_name           : u32 LE length + UTF-8
//! push_name               : u32 LE length + UTF-8
//! initialized             : u8 (0 / 1)
//! ```

use std::str::FromStr;

use wha_crypto::{KeyPair, PreKey};
use wha_types::Jid;

use crate::device::Device;
use crate::error::StoreError;

/// 4-byte magic that prefixes every device blob.
pub const MAGIC: &[u8; 4] = b"WDBV";
/// Current wire-format version.
pub const VERSION: u8 = 1;

/// The persistable subset of [`Device`] — everything except the trait-object
/// store handles. Decoders return this; callers reattach their backend to
/// produce a full `Device`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceBlob {
    pub registration_id: u32,
    pub noise_key_priv: [u8; 32],
    pub identity_key_priv: [u8; 32],
    pub signed_pre_key_id: u32,
    pub signed_pre_key_priv: [u8; 32],
    pub signed_pre_key_signature: Option<[u8; 64]>,
    pub adv_secret_key: [u8; 32],
    pub id: Option<String>,
    pub lid: Option<String>,
    pub platform: String,
    pub business_name: String,
    pub push_name: String,
    pub initialized: bool,
}

impl DeviceBlob {
    /// Capture the persistable fields of a live [`Device`] into a `DeviceBlob`.
    pub fn from_device(device: &Device) -> Self {
        DeviceBlob {
            registration_id: device.registration_id,
            noise_key_priv: device.noise_key.private,
            identity_key_priv: device.identity_key.private,
            signed_pre_key_id: device.signed_pre_key.key_id,
            signed_pre_key_priv: device.signed_pre_key.key_pair.private,
            signed_pre_key_signature: device.signed_pre_key.signature,
            adv_secret_key: device.adv_secret_key,
            id: device.id.as_ref().map(|j| j.to_string()),
            lid: device.lid.as_ref().map(|j| j.to_string()),
            platform: device.platform.clone(),
            business_name: device.business_name.clone(),
            push_name: device.push_name.clone(),
            initialized: device.initialized,
        }
    }

    /// Parse the JID strings back into [`Jid`] values. The wire format always
    /// stores them as their `Display` rendering, so this is an inverse of the
    /// stringification done in `from_device`.
    pub fn id_jid(&self) -> Result<Option<Jid>, StoreError> {
        self.id
            .as_deref()
            .map(|s| Jid::from_str(s).map_err(|e| StoreError::Backend(format!("bad id jid: {e}"))))
            .transpose()
    }

    /// See [`Self::id_jid`].
    pub fn lid_jid(&self) -> Result<Option<Jid>, StoreError> {
        self.lid
            .as_deref()
            .map(|s| Jid::from_str(s).map_err(|e| StoreError::Backend(format!("bad lid jid: {e}"))))
            .transpose()
    }

    /// Reconstruct just the cryptographic identity bundle — `(noise, identity,
    /// signed_pre_key)`. Useful for callers that want to rebuild a `Device`
    /// without manually wiring each `from_private` invocation.
    pub fn rebuild_keys(&self) -> (KeyPair, KeyPair, PreKey) {
        let noise = KeyPair::from_private(self.noise_key_priv);
        let identity = KeyPair::from_private(self.identity_key_priv);
        let signed_pre_key = PreKey {
            key_id: self.signed_pre_key_id,
            key_pair: KeyPair::from_private(self.signed_pre_key_priv),
            signature: self.signed_pre_key_signature,
        };
        (noise, identity, signed_pre_key)
    }
}

/// Encode a [`Device`] into the binary blob format.
pub fn encode_device(device: &Device) -> Vec<u8> {
    let blob = DeviceBlob::from_device(device);
    encode_blob(&blob)
}

/// Encode a [`DeviceBlob`] directly. Useful for re-saving a blob loaded from
/// disk after mutation, without having to reconstruct a full `Device`.
pub fn encode_blob(blob: &DeviceBlob) -> Vec<u8> {
    // Pre-size: fixed prefix (~150 B) plus enough room for the variable strings.
    let est = 4 + 1
        + 4 + 32 + 32
        + 4 + 32 + 1 + 64
        + 32
        + (1 + 4 + blob.id.as_deref().map(str::len).unwrap_or(0))
        + (1 + 4 + blob.lid.as_deref().map(str::len).unwrap_or(0))
        + (4 + blob.platform.len())
        + (4 + blob.business_name.len())
        + (4 + blob.push_name.len())
        + 1;
    let mut out = Vec::with_capacity(est);

    out.extend_from_slice(MAGIC);
    out.push(VERSION);

    out.extend_from_slice(&blob.registration_id.to_le_bytes());
    out.extend_from_slice(&blob.noise_key_priv);
    out.extend_from_slice(&blob.identity_key_priv);

    out.extend_from_slice(&blob.signed_pre_key_id.to_le_bytes());
    out.extend_from_slice(&blob.signed_pre_key_priv);
    match &blob.signed_pre_key_signature {
        Some(sig) => {
            out.push(1);
            out.extend_from_slice(sig);
        }
        None => out.push(0),
    }

    out.extend_from_slice(&blob.adv_secret_key);

    write_opt_str(&mut out, blob.id.as_deref());
    write_opt_str(&mut out, blob.lid.as_deref());
    write_str(&mut out, &blob.platform);
    write_str(&mut out, &blob.business_name);
    write_str(&mut out, &blob.push_name);

    out.push(if blob.initialized { 1 } else { 0 });

    out
}

/// Decode a binary device blob produced by [`encode_device`] / [`encode_blob`].
pub fn decode_device(bytes: &[u8]) -> Result<DeviceBlob, StoreError> {
    let mut r = Reader::new(bytes);

    let magic = r.read_array::<4>()?;
    if &magic != MAGIC {
        return Err(StoreError::Backend(format!(
            "device blob: bad magic {magic:?}, expected {MAGIC:?}"
        )));
    }
    let version = r.read_u8()?;
    if version != VERSION {
        return Err(StoreError::Backend(format!(
            "device blob: unsupported version {version}, expected {VERSION}"
        )));
    }

    let registration_id = r.read_u32_le()?;
    let noise_key_priv = r.read_array::<32>()?;
    let identity_key_priv = r.read_array::<32>()?;

    let signed_pre_key_id = r.read_u32_le()?;
    let signed_pre_key_priv = r.read_array::<32>()?;
    let sig_present = r.read_u8()?;
    let signed_pre_key_signature = match sig_present {
        0 => None,
        1 => Some(r.read_array::<64>()?),
        other => {
            return Err(StoreError::Backend(format!(
                "device blob: bad signed_pre_key signature flag {other}"
            )))
        }
    };

    let adv_secret_key = r.read_array::<32>()?;

    let id = r.read_opt_str()?;
    let lid = r.read_opt_str()?;
    let platform = r.read_str()?;
    let business_name = r.read_str()?;
    let push_name = r.read_str()?;

    let initialized = match r.read_u8()? {
        0 => false,
        1 => true,
        other => {
            return Err(StoreError::Backend(format!(
                "device blob: bad initialized flag {other}"
            )))
        }
    };

    if !r.is_empty() {
        return Err(StoreError::Backend(format!(
            "device blob: {} trailing byte(s) after final field",
            r.remaining()
        )));
    }

    Ok(DeviceBlob {
        registration_id,
        noise_key_priv,
        identity_key_priv,
        signed_pre_key_id,
        signed_pre_key_priv,
        signed_pre_key_signature,
        adv_secret_key,
        id,
        lid,
        platform,
        business_name,
        push_name,
        initialized,
    })
}

// ---------------------------------------------------------------------------
// helpers

fn write_str(out: &mut Vec<u8>, s: &str) {
    let len = s.len() as u32;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn write_opt_str(out: &mut Vec<u8>, s: Option<&str>) {
    match s {
        Some(s) => {
            out.push(1);
            write_str(out, s);
        }
        None => out.push(0),
    }
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    fn is_empty(&self) -> bool {
        self.pos >= self.buf.len()
    }

    fn read_slice(&mut self, n: usize) -> Result<&'a [u8], StoreError> {
        if self.pos.saturating_add(n) > self.buf.len() {
            return Err(StoreError::Backend(format!(
                "device blob: truncated (need {} byte(s) at offset {}, have {})",
                n,
                self.pos,
                self.buf.len()
            )));
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn read_u8(&mut self) -> Result<u8, StoreError> {
        Ok(self.read_slice(1)?[0])
    }

    fn read_u32_le(&mut self) -> Result<u32, StoreError> {
        let s = self.read_slice(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }

    fn read_array<const N: usize>(&mut self) -> Result<[u8; N], StoreError> {
        let s = self.read_slice(N)?;
        let mut out = [0u8; N];
        out.copy_from_slice(s);
        Ok(out)
    }

    fn read_str(&mut self) -> Result<String, StoreError> {
        let len = self.read_u32_le()? as usize;
        let bytes = self.read_slice(len)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|e| StoreError::Backend(format!("device blob: bad utf-8: {e}")))
    }

    fn read_opt_str(&mut self) -> Result<Option<String>, StoreError> {
        match self.read_u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.read_str()?)),
            other => Err(StoreError::Backend(format!(
                "device blob: bad option flag {other}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::memory::MemoryStore;

    fn fresh_device() -> Device {
        let store = Arc::new(MemoryStore::new());
        store.new_device()
    }

    #[test]
    fn device_blob_round_trip() {
        let mut dev = fresh_device();
        // Populate the optional/string fields so the round-trip exercises every branch.
        // Use a non-zero agent so Display preserves the full AD form;
        // when raw_agent == 0 the canonical Display strips the dot, which is
        // a property of Jid stringification, not of our blob format.
        dev.id = Some(Jid::from_str("1234.5:7@s.whatsapp.net").unwrap());
        dev.lid = Some(Jid::from_str("9876@lid").unwrap());
        dev.platform = "android".to_string();
        dev.business_name = "Acme Inc.".to_string();
        dev.push_name = "Alice".to_string();
        dev.initialized = true;

        let bytes = encode_device(&dev);
        let blob = decode_device(&bytes).expect("decode");

        assert_eq!(blob.registration_id, dev.registration_id);
        assert_eq!(blob.noise_key_priv, dev.noise_key.private);
        assert_eq!(blob.identity_key_priv, dev.identity_key.private);
        assert_eq!(blob.signed_pre_key_id, dev.signed_pre_key.key_id);
        assert_eq!(blob.signed_pre_key_priv, dev.signed_pre_key.key_pair.private);
        assert_eq!(blob.signed_pre_key_signature, dev.signed_pre_key.signature);
        assert_eq!(blob.adv_secret_key, dev.adv_secret_key);
        assert_eq!(blob.id.as_deref(), Some("1234.5:7@s.whatsapp.net"));
        assert_eq!(blob.lid.as_deref(), Some("9876@lid"));
        // Round-tripping through the parser yields the same AD JID we set.
        assert_eq!(blob.id_jid().unwrap(), dev.id);
        assert_eq!(blob.lid_jid().unwrap(), dev.lid);
        assert_eq!(blob.platform, "android");
        assert_eq!(blob.business_name, "Acme Inc.");
        assert_eq!(blob.push_name, "Alice");
        assert!(blob.initialized);

        // Cryptographic round-trip: rebuilt key pairs match the originals.
        let (noise, identity, signed) = blob.rebuild_keys();
        assert_eq!(noise, dev.noise_key);
        assert_eq!(identity, dev.identity_key);
        assert_eq!(signed.key_id, dev.signed_pre_key.key_id);
        assert_eq!(signed.key_pair.private, dev.signed_pre_key.key_pair.private);
        assert_eq!(signed.key_pair.public, dev.signed_pre_key.key_pair.public);
        assert_eq!(signed.signature, dev.signed_pre_key.signature);

        // And re-encoding the decoded blob produces byte-identical output.
        assert_eq!(encode_blob(&blob), bytes);
    }

    #[test]
    fn round_trip_with_none_fields() {
        // A pre-pairing device: id/lid empty, signature still present from new_device.
        let dev = fresh_device();
        let bytes = encode_device(&dev);
        let blob = decode_device(&bytes).expect("decode");
        assert!(blob.id.is_none());
        assert!(blob.lid.is_none());
        assert!(blob.id_jid().unwrap().is_none());
        assert!(blob.lid_jid().unwrap().is_none());
        assert!(!blob.initialized);
    }

    #[test]
    fn decode_rejects_bad_magic() {
        let dev = fresh_device();
        let mut bytes = encode_device(&dev);
        bytes[0] = b'X';
        let err = decode_device(&bytes).expect_err("must fail");
        match err {
            StoreError::Backend(msg) => assert!(msg.contains("bad magic"), "got: {msg}"),
            e => panic!("wrong variant: {e:?}"),
        }
    }

    #[test]
    fn decode_rejects_bad_version() {
        let dev = fresh_device();
        let mut bytes = encode_device(&dev);
        bytes[4] = VERSION.wrapping_add(1);
        let err = decode_device(&bytes).expect_err("must fail");
        match err {
            StoreError::Backend(msg) => assert!(msg.contains("unsupported version"), "got: {msg}"),
            e => panic!("wrong variant: {e:?}"),
        }
    }

    #[test]
    fn decode_rejects_truncated() {
        let dev = fresh_device();
        let bytes = encode_device(&dev);
        // Truncate at every byte boundary up to (but not including) the full
        // length — every prefix must be rejected.
        for cut in 0..bytes.len() {
            let err = decode_device(&bytes[..cut]).expect_err("truncated must fail");
            assert!(
                matches!(err, StoreError::Backend(_)),
                "expected Backend variant, got {err:?} at cut {cut}"
            );
        }
        // Sanity: the full buffer still parses.
        decode_device(&bytes).expect("full parses");
    }

    #[test]
    fn decode_rejects_trailing_garbage() {
        let dev = fresh_device();
        let mut bytes = encode_device(&dev);
        bytes.push(0xAA);
        let err = decode_device(&bytes).expect_err("must fail");
        match err {
            StoreError::Backend(msg) => assert!(msg.contains("trailing"), "got: {msg}"),
            e => panic!("wrong variant: {e:?}"),
        }
    }
}
