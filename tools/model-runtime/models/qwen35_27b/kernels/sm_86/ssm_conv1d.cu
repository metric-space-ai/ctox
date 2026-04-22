// Depth-wise 1-D causal convolution along the token axis, with fused
// SiLU. Used by Qwen3.5 hybrid layers (GDN block) to pre-mix the
// qkvg stream before the SSM recurrence.
//
// Math (per channel c, token t in [0, n_tokens)):
//   x_padded = concat(state, x)            length (K-1 + n_tokens)
//   pre      = sum_{k=0..K-1} w[k, c] * x_padded[t + K - 1 - k, c]
//   y[t, c]  = silu(pre) = pre / (1 + exp(-pre))
//
// Weight orientation matches ggml's `ssm_conv1d` layout: `w` is
// [K, n_channels] row-major, with `w[k, c]` the k-th tap for channel c.
// The reversed index `t + K - 1 - k` is the textbook 1-D convolution
// convention; the task spec uses it explicitly and the CPU reference
// in the test matches.
//
// State update (state may alias state_out):
//   state_out[i, c] = concat(state, x)[n_tokens + i, c]   for i in 0..K-1
//
// Because the state_update kernel runs AFTER the conv kernel on the
// same stream, and it only writes state_out while reading the shifted
// slots of state and x, aliasing is safe. Writes land at index i while
// reads pull from n_tokens + i (different index for n_tokens >= 1),
// and distinct threads touch distinct i, so no intra-kernel race.
//
// Shapes:
//   x         : [n_tokens, n_channels]   bf16 row-major
//   state     : [K-1, n_channels]        bf16 row-major
//   w         : [K, n_channels]          bf16 row-major
//   y         : [n_tokens, n_channels]   bf16 row-major (pre-allocated)
//   state_out : [K-1, n_channels]        bf16 row-major (may alias state)
//
// Launch convention (conv kernel):
//   grid  = (ceil(n_channels / 256), n_tokens, 1)
//   block = (256, 1, 1)
//   shmem = 0
// Launch convention (state_update kernel):
//   grid  = (ceil(n_channels / 256), K-1, 1)
//   block = (256, 1, 1)
//   shmem = 0
//
// Extern "C" entry points:
//   * ssm_conv1d_bf16           — main conv + fused silu.
//   * ssm_conv1d_state_update_bf16 — state ring rotation.
// Load via cudarc's `module.load_function(...)`.

#include <cuda_bf16.h>

// ---------------------------------------------------------------------------
// Conv + fused SiLU.
// ---------------------------------------------------------------------------

extern "C" __global__ void ssm_conv1d_bf16(
    const __nv_bfloat16 * __restrict__ x,      // [n_tokens, n_channels]
    const __nv_bfloat16 * __restrict__ state,  // [K-1,       n_channels]
    const __nv_bfloat16 * __restrict__ w,      // [K,         n_channels]
    __nv_bfloat16 * __restrict__ y,            // [n_tokens,  n_channels]
    int n_tokens,
    int n_channels,
    int kernel_size                            // K (Qwen3.5 = 4)
) {
    const int c = blockIdx.x * blockDim.x + threadIdx.x;
    const int t = blockIdx.y;
    if (c >= n_channels || t >= n_tokens) {
        return;
    }

    const int K     = kernel_size;
    const int K_m1  = K - 1;

    // Accumulate the K-tap dot product in f32 for precision.
    float acc = 0.0f;
    #pragma unroll 4
    for (int k = 0; k < K; ++k) {
        // Source index into the (K-1 + n_tokens) padded sequence.
        const int src_idx = t + K_m1 - k;
        float xv;
        if (src_idx < K_m1) {
            // From previous state.
            xv = __bfloat162float(state[(size_t)src_idx * n_channels + c]);
        } else {
            // From the current input x.
            const int xi = src_idx - K_m1;
            xv = __bfloat162float(x[(size_t)xi * n_channels + c]);
        }
        const float wv = __bfloat162float(w[(size_t)k * n_channels + c]);
        acc += wv * xv;
    }

    // Fused SiLU. `__expf` is the fast-math intrinsic (≈ 2^-21 relative
    // error) — plenty below the bf16 output floor of 2^-7.
    const float silu = acc / (1.0f + __expf(-acc));
    y[(size_t)t * n_channels + c] = __float2bfloat16(silu);
}

// ---------------------------------------------------------------------------
// State update: state_out[i, c] ← concat(state, x)[n_tokens + i, c].
//
// Run in a separate kernel on the same stream AFTER the conv kernel.
// This guarantees all reads of `state` inside the conv kernel have
// completed before we overwrite state_out (which may alias state).
// ---------------------------------------------------------------------------

extern "C" __global__ void ssm_conv1d_state_update_bf16(
    const __nv_bfloat16 * __restrict__ x,      // [n_tokens, n_channels]
    const __nv_bfloat16 * __restrict__ state,  // [K-1,       n_channels]
    __nv_bfloat16 * __restrict__ state_out,    // [K-1,       n_channels]
    int n_tokens,
    int n_channels,
    int kernel_size
) {
    const int c = blockIdx.x * blockDim.x + threadIdx.x;
    const int i = blockIdx.y;
    const int K_m1 = kernel_size - 1;
    if (c >= n_channels || i >= K_m1) {
        return;
    }

    const int src_idx = n_tokens + i;  // index into concat(state, x)
    __nv_bfloat16 v;
    if (src_idx < K_m1) {
        // Still within the old state window (only reachable when
        // n_tokens < K-1 — typical Qwen3.5 prefill with few tokens).
        v = state[(size_t)src_idx * n_channels + c];
    } else {
        const int xi = src_idx - K_m1;  // 0 <= xi < n_tokens
        v = x[(size_t)xi * n_channels + c];
    }
    state_out[(size_t)i * n_channels + c] = v;
}
