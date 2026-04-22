// NEOX-style rotary position embedding for the DFlash draft model.
//
// The draft uses standard 1-axis NEOX RoPE (theta=10M) — *not* the
// 4-axis MRoPE the target's full-attention layer uses. Rather than
// coax ggml-cuda's templated `rope_neox<...>` out of the vendor tree
// (which needs careful template-symbol mangling + every upstream
// feature-flag arg wired through from Rust), we ship a minimal
// self-contained implementation here. Pair `(i, i+D/2)` rotation —
// the NEOX convention:
//
//     x0'  =  x0 * cos(theta) - x[D/2] * sin(theta)
//     x[D/2]' = x0 * sin(theta) + x[D/2] * cos(theta)
//
// where `theta = pos[token] * theta_base ^ (-2*i / D)`.
//
// One thread per *output pair*: grid = (ceil((n_rows*D/2) / BLOCK), 1, 1)
// where `n_rows = n_tokens * n_heads` and BLOCK = 256. The single
// linear index `idx` decodes as `(row, pair)` which maps back to
// `(token, head, i0)` via the strides we pass in.
//
// Shapes (bf16, in-place):
//   x     : [n_tokens, n_heads, head_dim] row-major contiguous
//   pos   : [n_tokens] i32
//   n_dims: number of leading dims per head to rotate (even, ≤ head_dim)
//
// Inputs that don't apply to the draft path — YaRN / ext_factor /
// freq_factors — are fixed at their no-op values at this layer;
// a fresh kernel beats threading 8 unused args through the wrapper.

#include <cuda_bf16.h>
#include <math.h>

extern "C" __global__ void rope_neox_bf16_inplace(
    __nv_bfloat16 * __restrict__ x,   // [n_tokens, n_heads, head_dim]
    const int32_t * __restrict__ pos, // [n_tokens]
    int n_tokens,
    int n_heads,
    int head_dim,
    int n_dims,            // number of leading dims per head to rotate
    float theta_base
) {
    const int half = n_dims / 2;
    const int pairs_per_row = half;
    const int n_rows = n_tokens * n_heads;
    const int total_pairs = n_rows * pairs_per_row;

    const int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx >= total_pairs) return;

    // Decode linear index into (row, pair_i).
    const int row    = idx / pairs_per_row;
    const int pair_i = idx - row * pairs_per_row;     // 0..half-1
    const int tok    = row / n_heads;
    const int head   = row - tok * n_heads;

    // Theta: standard NEOX formula
    //   theta_k = pos * base^(-2k / n_dims)   (k = pair_i)
    const float inv_exp = -2.0f * (float)pair_i / (float)n_dims;
    const float freq = powf(theta_base, inv_exp);
    const float theta = (float)pos[tok] * freq;
    const float c = cosf(theta);
    const float s = sinf(theta);

    // Address of x[tok, head, 0]:
    __nv_bfloat16 * row_ptr = x + (tok * n_heads + head) * head_dim;

    const int i0 = pair_i;
    const int i1 = pair_i + half;

    // Load pair as f32 for precision during the rotation.
    const float x0 = __bfloat162float(row_ptr[i0]);
    const float x1 = __bfloat162float(row_ptr[i1]);

    // Rotate.
    row_ptr[i0] = __float2bfloat16(x0 * c - x1 * s);
    row_ptr[i1] = __float2bfloat16(x0 * s + x1 * c);
}
