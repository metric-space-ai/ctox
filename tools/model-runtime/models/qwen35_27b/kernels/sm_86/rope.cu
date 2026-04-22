// MRoPE — Qwen3.5 4-axis Multi-axis Rotary Position Embedding.
//
// In-place rotation of a Q- or K-tensor of shape [n_tokens, n_heads,
// head_dim] bf16. NeoX-style pairing: within the first `rope_dim` dims
// of each head, pair p is (dim[p], dim[p + rope_dim/2]). Dims beyond
// `rope_dim` are passed through unchanged.
//
// 4-axis: the first `rope_dim/2` pairs are partitioned into 4 equal
// sections (section_size = rope_dim/8 pairs). Section s uses axis s's
// position when computing the rotation angle:
//
//     theta(p) = positions[axis, token] * base ^ (-2 p / rope_dim)
//
// where axis ∈ {0,1,2,3} is determined by `p / section_size` clamped
// to [0, 3]. The task spec documents this: "dims are split into 4
// regions per head, each region uses a different axis's position. For
// plain text, axes 0/1/2 hold text position, axis 3 is 0."
//
// Launch (element-pair-parallel):
//   * grid_dim  = ceil(numel / 512, 1, 1)  where numel = n_tokens *
//                                            n_heads * head_dim
//     (512 = block * 2 because one thread handles one pair).
//   * block_dim = (256, 1, 1)
//
// Extern "C" entry: `rope_mrope_bf16`.

#include <cuda_bf16.h>

extern "C" __global__ void rope_mrope_bf16(
    __nv_bfloat16 * __restrict__ qk,
    const int * __restrict__ positions,   // [4, n_tokens]
    int n_tokens,
    int n_heads,
    int head_dim,
    int rope_dim,
    float theta_base
) {
    // Each thread handles one rotation pair (p, p + rope_dim/2) within
    // one (token, head). We iterate over pair-index ∈ [0, numel/2).
    const int pair_idx = blockIdx.x * blockDim.x + threadIdx.x;
    const int pairs_per_head = head_dim / 2;
    const int total_pairs = n_tokens * n_heads * pairs_per_head;
    if (pair_idx >= total_pairs) {
        return;
    }

    // Decompose: pair_idx = (token * n_heads + head) * pairs_per_head + p
    const int p      = pair_idx % pairs_per_head;
    const int flat_h = pair_idx / pairs_per_head;
    const int head   = flat_h % n_heads;
    const int token  = flat_h / n_heads;

    // Only rotate the first rope_dim dims. If p is past rope_dim/2, the
    // corresponding pair (p, p + rope_dim/2) would straddle the rotate
    // region boundary — we simply pass through.
    if (p >= rope_dim / 2) {
        return;
    }

    // 4-way section partition across the rope_dim/2 pairs.
    // section_size = rope_dim / 8 (so 4 * section_size = rope_dim/2).
    // If rope_dim isn't a multiple of 8, the last section absorbs
    // the remainder via clamping.
    const int section_size = rope_dim / 8;
    int axis = 0;
    if (section_size > 0) {
        axis = p / section_size;
        if (axis > 3) {
            axis = 3;
        }
    }

    // Axis-specific position.
    const int pos = positions[axis * n_tokens + token];

    // theta = pos * base^(-2p / rope_dim)
    //       = pos * exp2f((-2p / rope_dim) * log2(base))
    // Using powf keeps us numerically close to the ggml reference.
    const float exponent = -2.0f * (float)p / (float)rope_dim;
    const float freq     = powf(theta_base, exponent);
    const float theta    = (float)pos * freq;

    // Use standard sincosf (not __sincosf fast-math) so the angle
    // evaluation stays close to the host-side f32 reference — the fast
    // path drifts enough on small-magnitude pairs that bf16 rounding
    // pushes the relative error above our 5e-3 gate.
    float cos_t, sin_t;
    sincosf(theta, &sin_t, &cos_t);

    // Base address of this (token, head) slice.
    const size_t base = ((size_t)token * n_heads + (size_t)head) * (size_t)head_dim;
    const size_t i0   = base + (size_t)p;
    const size_t i1   = base + (size_t)p + (size_t)(rope_dim / 2);

    const float x0 = __bfloat162float(qk[i0]);
    const float x1 = __bfloat162float(qk[i1]);

    const float y0 = x0 * cos_t - x1 * sin_t;
    const float y1 = x0 * sin_t + x1 * cos_t;

    qk[i0] = __float2bfloat16_rn(y0);
    qk[i1] = __float2bfloat16_rn(y1);
}
