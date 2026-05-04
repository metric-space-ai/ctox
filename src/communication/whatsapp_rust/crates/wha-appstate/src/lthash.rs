//! Mirrors `_upstream/whatsmeow/appstate/lthash/lthash.go`.
//!
//! LTHash ("LT" = "lattice-style") is a homomorphic, associative summation
//! hash: every input is HKDF-expanded to a fixed-size buffer, those buffers
//! are added pointwise (mod 2^16 per uint16-LE word), and the result is the
//! hash. Subtraction undoes addition exactly, so apply/unapply is closed.

use wha_crypto::hkdf_sha256;

/// Parameters for an LTHash instance: HKDF info string + output size.
///
/// The WhatsApp app-state integrity hash uses
/// `info = "WhatsApp Patch Integrity"` and `size = 128`.
#[derive(Clone, Copy, Debug)]
pub struct LtHash {
    pub hkdf_info: &'static [u8],
    pub hkdf_size: usize,
}

/// The instance used by WhatsApp for patch integrity.
pub const WA_PATCH_INTEGRITY: LtHash = LtHash {
    hkdf_info: b"WhatsApp Patch Integrity",
    hkdf_size: 128,
};

impl LtHash {
    /// `output = base − Σ subtract + Σ add` pointwise. Mirrors
    /// `LTHash.SubtractThenAdd`.
    pub fn subtract_then_add(&self, base: &[u8], subtract: &[&[u8]], add: &[&[u8]]) -> Vec<u8> {
        let mut out = base.to_vec();
        self.subtract_then_add_in_place(&mut out, subtract, add);
        out
    }

    pub fn subtract_then_add_in_place(
        &self,
        base: &mut [u8],
        subtract: &[&[u8]],
        add: &[&[u8]],
    ) {
        for item in subtract {
            self.pointwise_op(base, item, true);
        }
        for item in add {
            self.pointwise_op(base, item, false);
        }
    }

    /// Add a single pre-image to the hash state.
    pub fn add(&self, base: &mut [u8], item: &[u8]) {
        self.pointwise_op(base, item, false);
    }

    /// Subtract a single pre-image from the hash state.
    pub fn subtract(&self, base: &mut [u8], item: &[u8]) {
        self.pointwise_op(base, item, true);
    }

    fn pointwise_op(&self, base: &mut [u8], item: &[u8], subtract: bool) {
        let expanded = hkdf_sha256(item, &[], self.hkdf_info, self.hkdf_size)
            .expect("hkdf-sha256 fits the bound");
        debug_assert_eq!(base.len() % 2, 0);
        debug_assert_eq!(expanded.len(), base.len());
        let mut i = 0;
        while i < base.len() {
            let x = u16::from_le_bytes([base[i], base[i + 1]]);
            let y = u16::from_le_bytes([expanded[i], expanded[i + 1]]);
            let r = if subtract {
                x.wrapping_sub(y)
            } else {
                x.wrapping_add(y)
            };
            let r = r.to_le_bytes();
            base[i] = r[0];
            base[i + 1] = r[1];
            i += 2;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_then_subtract_is_identity() {
        let mut state = [0u8; 128];
        let original = state;
        WA_PATCH_INTEGRITY.add(&mut state, b"value-mac for set #1");
        WA_PATCH_INTEGRITY.add(&mut state, b"value-mac for set #2");
        assert_ne!(state, original);
        WA_PATCH_INTEGRITY.subtract(&mut state, b"value-mac for set #1");
        WA_PATCH_INTEGRITY.subtract(&mut state, b"value-mac for set #2");
        assert_eq!(state, original);
    }

    #[test]
    fn subtract_then_add_round_trip() {
        let base = [0xa5u8; 128];
        let item_a: &[u8] = b"item-A";
        let item_b: &[u8] = b"item-B";
        let after = WA_PATCH_INTEGRITY.subtract_then_add(&base, &[], &[item_a, item_b]);
        let back = WA_PATCH_INTEGRITY.subtract_then_add(&after, &[item_a, item_b], &[]);
        assert_eq!(back, base);
    }

    #[test]
    fn order_independence() {
        let mut a = [0u8; 128];
        let mut b = [0u8; 128];
        WA_PATCH_INTEGRITY.add(&mut a, b"x");
        WA_PATCH_INTEGRITY.add(&mut a, b"y");
        WA_PATCH_INTEGRITY.add(&mut b, b"y");
        WA_PATCH_INTEGRITY.add(&mut b, b"x");
        assert_eq!(a, b);
    }
}
