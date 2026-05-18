//! BF16 helpers. We keep storage as raw `u16` and compute as `f32`.

#[inline]
pub fn bf16_to_f32(x: u16) -> f32 {
    f32::from_bits((x as u32) << 16)
}

#[inline]
pub fn f32_to_bf16_round_nearest_even(x: f32) -> u16 {
    let bits = x.to_bits();
    let lsb = (bits >> 16) & 1;
    let rounding_bias = 0x7fff + lsb;
    ((bits + rounding_bias) >> 16) as u16
}

#[inline]
pub fn bf16_from_le_bytes(bytes: &[u8], index: usize) -> u16 {
    let i = index * 2;
    u16::from_le_bytes([bytes[i], bytes[i + 1]])
}

#[inline]
pub fn bf16_le_bytes_to_f32(bytes: &[u8], index: usize) -> f32 {
    bf16_to_f32(bf16_from_le_bytes(bytes, index))
}

pub fn bf16_slice_to_f32(dst: &mut [f32], src: &[u16]) {
    assert_eq!(dst.len(), src.len());
    for (d, s) in dst.iter_mut().zip(src.iter().copied()) {
        *d = bf16_to_f32(s);
    }
}

pub fn bf16_le_bytes_to_vec(bytes: &[u8]) -> Vec<u16> {
    assert!(bytes.len() % 2 == 0);
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        out.push(u16::from_le_bytes([bytes[i], bytes[i + 1]]));
        i += 2;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bf16_known_values() {
        assert_eq!(bf16_to_f32(0x3f80), 1.0);
        assert_eq!(bf16_to_f32(0xbf80), -1.0);
        assert_eq!(f32_to_bf16_round_nearest_even(1.0), 0x3f80);
    }
}
