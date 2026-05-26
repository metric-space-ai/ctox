// Origin: CTOX
// License: AGPL-3.0-only

//! Pure-Rust Q4_K block layout + dequantization reference.
//!
//! ref: vendor/ggml-metal/ggml-common.h:316-328 (`block_q4_K`,
//! `static_assert sizeof == 144`)
//!
//! ref upstream CPU dequant: ggml's `dequantize_row_q4_K` in
//! ggml-quants.c (logic reproduced here for parity with what the
//! vendored MSL `kernel_mul_mv_q4_K_f32` computes inline).
//!
//! This is the **CPU correctness reference** the Metal kernel verifier
//! byte-compares against. It is NOT used in the hot inference path.

use half::f16;

/// Sub-block size — every Q4_K super-block stores 8 sub-blocks × 32 weights.
pub const Q4_K_SUBBLOCK: usize = 32;
/// Super-block weight count.
pub const QK_K: usize = 256;
/// On-disk byte size of one super-block. Asserted at upstream:
/// `2 × sizeof(half) + K_SCALE_SIZE(=12) + QK_K/2 = 4 + 12 + 128 = 144`.
pub const BLOCK_Q4_K_BYTES: usize = 144;

/// One super-block. `#[repr(C, packed)]` is intentional: this struct
/// gets cast straight from raw bytes off the GGUF mmap, so the
/// in-memory layout must equal the on-disk one — no Rust padding.
#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct BlockQ4K {
    /// `d` and `dmin`, packed as a half2.
    pub d: f16,
    pub dmin: f16,
    /// 6-bit scale + 6-bit min for each of the 8 sub-blocks, encoded
    /// in the upstream's `get_scale_min_k4` packing.
    pub scales: [u8; 12],
    /// 4-bit weights, low and high nibbles split per sub-block pair.
    pub qs: [u8; QK_K / 2],
}

const _: () = assert!(std::mem::size_of::<BlockQ4K>() == BLOCK_Q4_K_BYTES);

/// Decode the 6-bit (scale, min) pair for sub-block `j` ∈ 0..8 out of
/// the packed `scales[12]`. ref: ggml-quants.c `get_scale_min_k4`.
pub fn get_scale_min_k4(j: usize, q: &[u8; 12]) -> (u8, u8) {
    if j < 4 {
        (q[j] & 63, q[j + 4] & 63)
    } else {
        let d = (q[j + 4] & 0xF) | ((q[j - 4] >> 6) << 4);
        let m = (q[j + 4] >> 4) | ((q[j - 0] >> 6) << 4);
        (d, m)
    }
}

/// Dequantize one super-block (256 weights) into f32. Order matches
/// the layout the matvec/matmat kernels see when they walk the same
/// bytes; if the GPU's inline dequant disagrees with this, the
/// per-op verifier catches it.
///
/// ref: ggml-quants.c `dequantize_row_q4_K`.
pub fn dequantize_block_q4_k(blk: &BlockQ4K, out: &mut [f32; QK_K]) {
    let d_super = blk.d.to_f32();
    let dmin_super = blk.dmin.to_f32();
    let scales = blk.scales;
    let qs = blk.qs;

    // 4 iterations × 64 weights = 256.  Each iteration covers two
    // sub-blocks (j and j+1): low nibbles → j, high nibbles → j+1,
    // both reading from the same 32-byte slice of qs.
    let mut out_idx = 0usize;
    let mut q_off = 0usize;
    for j in (0..8).step_by(2) {
        let (sc1, m1) = get_scale_min_k4(j, &scales);
        let (sc2, m2) = get_scale_min_k4(j + 1, &scales);
        let d1 = d_super * (sc1 as f32);
        let m1f = dmin_super * (m1 as f32);
        let d2 = d_super * (sc2 as f32);
        let m2f = dmin_super * (m2 as f32);

        for l in 0..32 {
            let w = qs[q_off + l] & 0x0F;
            out[out_idx] = d1 * (w as f32) - m1f;
            out_idx += 1;
        }
        for l in 0..32 {
            let w = (qs[q_off + l] >> 4) & 0x0F;
            out[out_idx] = d2 * (w as f32) - m2f;
            out_idx += 1;
        }
        q_off += 32;
    }
}

/// Dequantize a contiguous run of `n_super` super-blocks (= `n_super × 256`
/// weights) into f32. Convenience wrapper used by the verifier.
pub fn dequantize_q4_k_to_f32(blocks: &[BlockQ4K]) -> Vec<f32> {
    let mut out = vec![0.0f32; blocks.len() * QK_K];
    let mut tmp = [0.0f32; QK_K];
    for (i, blk) in blocks.iter().enumerate() {
        dequantize_block_q4_k(blk, &mut tmp);
        out[i * QK_K..(i + 1) * QK_K].copy_from_slice(&tmp);
    }
    out
}

/// Build a deterministically-pseudorandom Q4_K super-block. Used by
/// per-op verifiers to synthesize weights without needing a real GGUF
/// on disk. Scales / mins come from a small range so dequantized
/// values stay in a numerically reasonable f32 band.
pub fn synth_block_q4_k(seed: u32) -> BlockQ4K {
    fn xs(state: &mut u32) -> u32 {
        *state ^= *state << 13;
        *state ^= *state >> 17;
        *state ^= *state << 5;
        *state
    }
    let mut s = seed.wrapping_mul(2654435761).wrapping_add(0xC0FE_BAAD);
    // d ≈ 1/64, dmin ≈ 1/256: keeps dequant output in [-1, +1] roughly.
    let d = f16::from_f32((((xs(&mut s) % 32) + 1) as f32) / 2048.0);
    let dmin = f16::from_f32((((xs(&mut s) % 8) + 1) as f32) / 4096.0);
    let mut scales = [0u8; 12];
    for v in scales.iter_mut() {
        *v = (xs(&mut s) & 0x3F) as u8;
    }
    let mut qs = [0u8; QK_K / 2];
    for v in qs.iter_mut() {
        *v = xs(&mut s) as u8;
    }
    BlockQ4K { d, dmin, scales, qs }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_q4_k_size_is_144() {
        assert_eq!(std::mem::size_of::<BlockQ4K>(), BLOCK_Q4_K_BYTES);
    }

    #[test]
    fn dequant_run_matches_block_dequant() {
        // Synthesizing 4 blocks and dequanting them via both paths
        // (per-block + bulk) must give byte-identical output.
        let blocks = (0..4)
            .map(synth_block_q4_k)
            .collect::<Vec<_>>();
        let bulk = dequantize_q4_k_to_f32(&blocks);
        let mut piecewise = vec![0.0f32; 4 * QK_K];
        let mut tmp = [0.0f32; QK_K];
        for (i, b) in blocks.iter().enumerate() {
            dequantize_block_q4_k(b, &mut tmp);
            piecewise[i * QK_K..(i + 1) * QK_K].copy_from_slice(&tmp);
        }
        assert_eq!(bulk, piecewise);
    }

    #[test]
    fn dequant_zero_block_is_zero() {
        // d=0, dmin=0, any scales/qs → all weights 0.
        let blk = BlockQ4K {
            d: f16::from_f32(0.0),
            dmin: f16::from_f32(0.0),
            scales: [0xFF; 12],
            qs: [0xFF; QK_K / 2],
        };
        let mut out = [1.0f32; QK_K];
        dequantize_block_q4_k(&blk, &mut out);
        assert!(out.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn get_scale_min_k4_low_subblocks() {
        // For j<4, scale=q[j] & 63, min=q[j+4] & 63.
        let q = [0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67, 0x89, 0, 0, 0, 0];
        let (s, m) = get_scale_min_k4(0, &q);
        assert_eq!(s, 0xAB & 63);
        assert_eq!(m, 0x23 & 63);
        let (s, m) = get_scale_min_k4(3, &q);
        assert_eq!(s, 0x01 & 63);
        assert_eq!(m, 0x89 & 63);
    }
}
