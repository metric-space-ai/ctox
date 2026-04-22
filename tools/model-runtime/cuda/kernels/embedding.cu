// Token embedding lookup — gather rows from the embedding weight
// matrix by token id.
//
// Math (trivial):
//   out[t, :] = weight[token_ids[t], :]          if 0 <= id < vocab
//   out[t, :] = 0                                 otherwise (OOB guard)
//
// Shapes:
//   weight    [vocab_size, hidden_dim]   bf16 | f16 | f32  row-major
//   token_ids [n_tokens]                 i32
//   out       [n_tokens, hidden_dim]     bf16              row-major
//
// Launch convention (mirrors rmsnorm):
//   grid  = (n_tokens, 1, 1)
//   block = (block_dim, 1, 1)  where block_dim = min(hidden_dim, 1024)
//                              rounded up to a multiple of 32.
//   shmem = 0
//
// One block per output token. The block reads `token_ids[block_idx]`
// once (thread 0 → shared), then every thread strides over `hidden_dim`
// copying / casting one element per step. If the id is out-of-bounds
// the output row is zero-filled instead — the caller validates ids
// upstream; the kernel's OOB guard is a safety net for bad inputs.
//
// Memory-bound op: one read + one write per element, no reduction. On
// the first-layer hot path (~151936 × 5120 bf16 table) this is still
// under 1% of the forward time, but we avoid per-row D→H syncs by not
// erroring on bad ids inside the kernel.
//
// Extern "C" entry points:
//   * embedding_bf16 — bf16 weight, bf16 output
//   * embedding_f16  — f16  weight, bf16 output (cast per fetch)
//   * embedding_f32  — f32  weight, bf16 output (cast per fetch)

#include <cuda_bf16.h>
#include <cuda_fp16.h>

// ---------------------------------------------------------------------------
// bf16 weight → bf16 out (bit-exact copy)
// ---------------------------------------------------------------------------

extern "C" __global__ void embedding_bf16(
    const __nv_bfloat16 * __restrict__ weight,   // [vocab_size, hidden_dim]
    const int           * __restrict__ token_ids,// [n_tokens]
    __nv_bfloat16       * __restrict__ out,      // [n_tokens, hidden_dim]
    int vocab_size,
    int hidden_dim
) {
    const int token = blockIdx.x;
    const int tid   = threadIdx.x;
    const int bdim  = blockDim.x;

    // Broadcast the token id to the whole block via shared memory. A
    // direct per-thread gmem load of `token_ids[token]` is 32 wasted
    // L1 hits; shared broadcast is the standard pattern.
    __shared__ int s_id;
    if (tid == 0) {
        s_id = token_ids[token];
    }
    __syncthreads();

    const int id = s_id;
    __nv_bfloat16 * out_row = out + (size_t)token * hidden_dim;

    // OOB guard — zero-fill row and bail.
    if (id < 0 || id >= vocab_size) {
        const __nv_bfloat16 zero = __float2bfloat16(0.0f);
        for (int i = tid; i < hidden_dim; i += bdim) {
            out_row[i] = zero;
        }
        return;
    }

    const __nv_bfloat16 * w_row = weight + (size_t)id * hidden_dim;
    for (int i = tid; i < hidden_dim; i += bdim) {
        out_row[i] = w_row[i];
    }
}

// ---------------------------------------------------------------------------
// f16 weight → bf16 out (cast via f32)
// ---------------------------------------------------------------------------

extern "C" __global__ void embedding_f16(
    const __half        * __restrict__ weight,   // [vocab_size, hidden_dim]
    const int           * __restrict__ token_ids,// [n_tokens]
    __nv_bfloat16       * __restrict__ out,      // [n_tokens, hidden_dim]
    int vocab_size,
    int hidden_dim
) {
    const int token = blockIdx.x;
    const int tid   = threadIdx.x;
    const int bdim  = blockDim.x;

    __shared__ int s_id;
    if (tid == 0) {
        s_id = token_ids[token];
    }
    __syncthreads();

    const int id = s_id;
    __nv_bfloat16 * out_row = out + (size_t)token * hidden_dim;

    if (id < 0 || id >= vocab_size) {
        const __nv_bfloat16 zero = __float2bfloat16(0.0f);
        for (int i = tid; i < hidden_dim; i += bdim) {
            out_row[i] = zero;
        }
        return;
    }

    const __half * w_row = weight + (size_t)id * hidden_dim;
    for (int i = tid; i < hidden_dim; i += bdim) {
        // f16 → f32 → bf16. f16→bf16 has no single intrinsic; going
        // through f32 is the standard path and matches how our CPU
        // reference implementations round.
        const float v = __half2float(w_row[i]);
        out_row[i] = __float2bfloat16(v);
    }
}

// ---------------------------------------------------------------------------
// f32 weight → bf16 out (round per fetch)
// ---------------------------------------------------------------------------

extern "C" __global__ void embedding_f32(
    const float         * __restrict__ weight,   // [vocab_size, hidden_dim]
    const int           * __restrict__ token_ids,// [n_tokens]
    __nv_bfloat16       * __restrict__ out,      // [n_tokens, hidden_dim]
    int vocab_size,
    int hidden_dim
) {
    const int token = blockIdx.x;
    const int tid   = threadIdx.x;
    const int bdim  = blockDim.x;

    __shared__ int s_id;
    if (tid == 0) {
        s_id = token_ids[token];
    }
    __syncthreads();

    const int id = s_id;
    __nv_bfloat16 * out_row = out + (size_t)token * hidden_dim;

    if (id < 0 || id >= vocab_size) {
        const __nv_bfloat16 zero = __float2bfloat16(0.0f);
        for (int i = tid; i < hidden_dim; i += bdim) {
            out_row[i] = zero;
        }
        return;
    }

    const float * w_row = weight + (size_t)id * hidden_dim;
    for (int i = tid; i < hidden_dim; i += bdim) {
        out_row[i] = __float2bfloat16(w_row[i]);
    }
}
