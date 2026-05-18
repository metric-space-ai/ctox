#include <cuda_bf16.h>
#include <cuda_fp16.h>
#include <cuda_runtime.h>

#include <float.h>
#include <math.h>
#include <stdint.h>

static __device__ inline float ctox_silu_f32(float x) {
    return x / (1.0f + expf(-x));
}

struct ctox_q4_k_block {
    uint16_t d;
    uint16_t dmin;
    uint8_t scales[12];
    uint8_t qs[128];
};
static_assert(sizeof(ctox_q4_k_block) == 144, "ctox_q4_k_block must match ggml block_q4_K");

static __device__ inline void ctox_q4_k_scale_min(int j, const uint8_t* q, uint8_t& d, uint8_t& m) {
    if (j < 4) {
        d = q[j] & 63;
        m = q[j + 4] & 63;
    } else {
        d = (q[j + 4] & 0xF) | ((q[j - 4] >> 6) << 4);
        m = (q[j + 4] >> 4) | ((q[j - 0] >> 6) << 4);
    }
}

static __device__ inline float ctox_q4_k_dequant_value(const ctox_q4_k_block& b, int idx) {
    const int j = idx / 32;
    const int local = idx - j * 32;
    const int il = j / 2;
    const int ir = local / 4;
    const int l = local - ir * 4;
    const uint8_t q = b.qs[32 * il + 4 * ir + l];
    const int qv = (j & 1) == 0 ? (q & 0xF) : (q >> 4);

    uint8_t sc = 0;
    uint8_t m = 0;
    ctox_q4_k_scale_min(j, b.scales, sc, m);
    const float dall = __half2float(__ushort_as_half(b.d));
    const float dmin = __half2float(__ushort_as_half(b.dmin));
    return (dall * sc) * qv - (dmin * m);
}

extern "C" __global__ void ctox_qwen35_35b_rms_norm_bf16_kernel(
    const __nv_bfloat16* x,
    const __nv_bfloat16* weight,
    __nv_bfloat16* y,
    int d,
    float eps,
    float weight_bias,
    int rows
) {
    const int row = blockIdx.x;
    const int tid = threadIdx.x;
    if (row >= rows) {
        return;
    }

    __shared__ float partial[256];
    float sum = 0.0f;
    for (int i = tid; i < d; i += 256) {
        const float v = __bfloat162float(x[row * d + i]);
        sum += v * v;
    }
    partial[tid] = sum;
    __syncthreads();

    for (int stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            partial[tid] += partial[tid + stride];
        }
        __syncthreads();
    }

    const float inv = rsqrtf(partial[0] / float(d) + eps);
    for (int i = tid; i < d; i += 256) {
        const int idx = row * d + i;
        const float w = __bfloat162float(weight[i]) + weight_bias;
        y[idx] = __float2bfloat16(__bfloat162float(x[idx]) * inv * w);
    }
}

extern "C" __global__ void ctox_qwen35_35b_moe_route_topk_bf16_kernel(
    const __nv_bfloat16* router_logits,
    __nv_bfloat16* topk_weights,
    int32_t* topk_ids,
    int top_k,
    int num_experts,
    int n_tokens
) {
    const int tok = blockIdx.x;
    if (tok >= n_tokens || threadIdx.x != 0) {
        return;
    }

    const int base = tok * num_experts;
    float selected[16];
    int ids[16];
    const int k_lim = top_k > 16 ? 16 : top_k;

    for (int k = 0; k < k_lim; ++k) {
        float best = -FLT_MAX;
        int best_id = 0;
        for (int e = 0; e < num_experts; ++e) {
            bool used = false;
            for (int j = 0; j < k; ++j) {
                used = used || ids[j] == e;
            }
            const float v = used ? -FLT_MAX : __bfloat162float(router_logits[base + e]);
            if (v > best) {
                best = v;
                best_id = e;
            }
        }
        selected[k] = best;
        ids[k] = best_id;
    }

    float max_v = -FLT_MAX;
    for (int k = 0; k < k_lim; ++k) {
        max_v = fmaxf(max_v, selected[k]);
    }
    float denom = 0.0f;
    for (int k = 0; k < k_lim; ++k) {
        selected[k] = expf(selected[k] - max_v);
        denom += selected[k];
    }

    for (int k = 0; k < k_lim; ++k) {
        topk_ids[tok * top_k + k] = ids[k];
        topk_weights[tok * top_k + k] = __float2bfloat16(selected[k] / denom);
    }
}

extern "C" __global__ void ctox_qwen35_35b_dense_matmul_bf16_kernel(
    const __nv_bfloat16* x,
    const __nv_bfloat16* w,
    const __nv_bfloat16* bias,
    __nv_bfloat16* y,
    int rows,
    int in_dim,
    int out_dim,
    bool has_bias
) {
    const int row = blockIdx.y;
    const int col = blockIdx.x * blockDim.x + threadIdx.x;
    if (row >= rows || col >= out_dim) {
        return;
    }

    float acc = has_bias ? __bfloat162float(bias[col]) : 0.0f;
    for (int k = 0; k < in_dim; ++k) {
        acc += __bfloat162float(x[row * in_dim + k]) * __bfloat162float(w[col * in_dim + k]);
    }
    y[row * out_dim + col] = __float2bfloat16(acc);
}

extern "C" __global__ void ctox_qwen35_35b_add_bf16_kernel(
    const __nv_bfloat16* a,
    const __nv_bfloat16* b,
    __nv_bfloat16* y,
    int n
) {
    const int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) {
        y[i] = __float2bfloat16(__bfloat162float(a[i]) + __bfloat162float(b[i]));
    }
}

extern "C" __global__ void ctox_qwen35_35b_mul_bf16_kernel(
    const __nv_bfloat16* a,
    const __nv_bfloat16* b,
    __nv_bfloat16* y,
    int n
) {
    const int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) {
        y[i] = __float2bfloat16(__bfloat162float(a[i]) * __bfloat162float(b[i]));
    }
}

extern "C" __global__ void ctox_qwen35_35b_silu_bf16_kernel(
    const __nv_bfloat16* x,
    __nv_bfloat16* y,
    int n
) {
    const int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) {
        y[i] = __float2bfloat16(ctox_silu_f32(__bfloat162float(x[i])));
    }
}

extern "C" __global__ void ctox_qwen35_35b_argmax_bf16_kernel(
    const __nv_bfloat16* x,
    int32_t* out,
    int vocab,
    int rows
) {
    const int row = blockIdx.x;
    if (row >= rows || threadIdx.x != 0) {
        return;
    }
    const int base = row * vocab;
    int best = 0;
    float best_v = __bfloat162float(x[base]);
    for (int i = 1; i < vocab; ++i) {
        const float v = __bfloat162float(x[base + i]);
        if (v > best_v) {
            best_v = v;
            best = i;
        }
    }
    out[row] = best;
}

extern "C" __global__ void ctox_qwen35_35b_copy_hidden_slot_bf16_kernel(
    const __nv_bfloat16* src,
    __nv_bfloat16* dst,
    int src_row,
    int dst_slot,
    int hidden,
    int dst_slots
) {
    const int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < hidden) {
        dst[dst_slot * hidden + i] = src[src_row * hidden + i];
    }
}

extern "C" __global__ void ctox_qwen35_35b_repeat_hidden_slots_bf16_kernel(
    const __nv_bfloat16* src,
    __nv_bfloat16* dst,
    int hidden,
    int dst_slots
) {
    const int i = blockIdx.x * blockDim.x + threadIdx.x;
    const int n = hidden * dst_slots;
    if (i < n) {
        dst[i] = src[i % hidden];
    }
}

extern "C" __global__ void ctox_qwen35_35b_fill_positions4_i32_kernel(
    int32_t* out,
    int start_pos,
    int n_tokens
) {
    const int i = blockIdx.x * blockDim.x + threadIdx.x;
    const int n = n_tokens * 4;
    if (i < n) {
        out[i] = start_pos + (i / 4);
    }
}

extern "C" __global__ void ctox_qwen35_35b_causal_mask_f16_kernel(
    __half* out,
    int kv_start,
    int n_tokens,
    int kv_len,
    int q_stride
) {
    const int i = blockIdx.x * blockDim.x + threadIdx.x;
    const int n = n_tokens * q_stride;
    if (i < n) {
        const int q = i / q_stride;
        const int k = i - q * q_stride;
        const bool valid_col = k < kv_len;
        const bool visible = valid_col && k <= kv_start + q;
        out[i] = __float2half(visible ? 0.0f : -INFINITY);
    }
}

extern "C" __global__ void ctox_qwen35_35b_kv_store_bf16_kernel(
    const __nv_bfloat16* src,
    __nv_bfloat16* cache,
    const int32_t* positions4,
    int n_tokens,
    int n_kv_heads,
    int head_dim,
    int max_ctx
) {
    const int i = blockIdx.x * blockDim.x + threadIdx.x;
    const int n = n_tokens * n_kv_heads * head_dim;
    if (i < n) {
        const int d = i % head_dim;
        const int tmp = i / head_dim;
        const int h = tmp % n_kv_heads;
        const int t = tmp / n_kv_heads;
        const int pos = positions4 == nullptr ? t : positions4[t * 4];
        cache[(h * max_ctx + pos) * head_dim + d] = src[(t * n_kv_heads + h) * head_dim + d];
    }
}

extern "C" __global__ void ctox_qwen35_35b_sdpa_decode_bf16_kernel(
    const __nv_bfloat16* q,
    const __nv_bfloat16* k_cache,
    const __nv_bfloat16* v_cache,
    __nv_bfloat16* out,
    int n_q_heads,
    int n_kv_heads,
    int head_dim,
    int kv_len,
    int max_ctx,
    float scale
) {
    const int qh = blockIdx.x;
    const int d = threadIdx.x;
    if (qh >= n_q_heads || d >= head_dim) {
        return;
    }
    const int kvh = (qh * n_kv_heads) / n_q_heads;

    float max_score = -INFINITY;
    for (int j = 0; j < kv_len; ++j) {
        float score = 0.0f;
        for (int k = 0; k < head_dim; ++k) {
            score += __bfloat162float(q[qh * head_dim + k]) *
                     __bfloat162float(k_cache[(kvh * max_ctx + j) * head_dim + k]);
        }
        max_score = fmaxf(max_score, score * scale);
    }

    float denom = 0.0f;
    float acc = 0.0f;
    for (int j = 0; j < kv_len; ++j) {
        float score = 0.0f;
        for (int k = 0; k < head_dim; ++k) {
            score += __bfloat162float(q[qh * head_dim + k]) *
                     __bfloat162float(k_cache[(kvh * max_ctx + j) * head_dim + k]);
        }
        const float w = expf(score * scale - max_score);
        denom += w;
        acc += w * __bfloat162float(v_cache[(kvh * max_ctx + j) * head_dim + d]);
    }
    out[qh * head_dim + d] = __float2bfloat16(acc / denom);
}

extern "C" __global__ void ctox_qwen35_35b_dequant_q4_k_bf16_kernel(
    const ctox_q4_k_block* x,
    __nv_bfloat16* y,
    int n_blocks
) {
    const int i = blockIdx.x;
    const int tid = threadIdx.x;
    if (i >= n_blocks || tid >= 32) {
        return;
    }

    const int il = tid / 8;
    const int ir = tid % 8;
    const int is = 2 * il;
    const int n = 4;
    const uint8_t* q = x[i].qs + 32 * il + n * ir;
    __nv_bfloat16* dst = y + i * 256 + 64 * il + n * ir;

    const float dall = __half2float(__ushort_as_half(x[i].d));
    const float dmin = __half2float(__ushort_as_half(x[i].dmin));

    uint8_t sc = 0;
    uint8_t m = 0;
    ctox_q4_k_scale_min(is + 0, x[i].scales, sc, m);
    const float d1 = dall * sc;
    const float m1 = dmin * m;
    ctox_q4_k_scale_min(is + 1, x[i].scales, sc, m);
    const float d2 = dall * sc;
    const float m2 = dmin * m;

    for (int l = 0; l < n; ++l) {
        dst[l + 0] = __float2bfloat16(d1 * (q[l] & 0xF) - m1);
        dst[l + 32] = __float2bfloat16(d2 * (q[l] >> 4) - m2);
    }
}

extern "C" __global__ void ctox_qwen35_35b_q4_k_matvec_bf16_kernel(
    const ctox_q4_k_block* w,
    const __nv_bfloat16* x,
    __nv_bfloat16* y,
    int in_dim,
    int out_dim
) {
    const int row = blockIdx.x;
    if (row >= out_dim || threadIdx.x != 0) {
        return;
    }
    const int blocks_per_row = in_dim / 256;
    float acc = 0.0f;
    for (int b = 0; b < blocks_per_row; ++b) {
        const ctox_q4_k_block& wb = w[row * blocks_per_row + b];
        const int x_base = b * 256;
        for (int k = 0; k < 256; ++k) {
            acc += ctox_q4_k_dequant_value(wb, k) * __bfloat162float(x[x_base + k]);
        }
    }
    y[row] = __float2bfloat16(acc);
}

extern "C" cudaError_t ctox_qwen35_35b_rms_norm_bf16_launch(
    const void* x,
    const void* weight,
    void* y,
    int d,
    float eps,
    float weight_bias,
    int rows,
    cudaStream_t stream
) {
    ctox_qwen35_35b_rms_norm_bf16_kernel<<<rows, 256, 0, stream>>>(
        static_cast<const __nv_bfloat16*>(x),
        static_cast<const __nv_bfloat16*>(weight),
        static_cast<__nv_bfloat16*>(y),
        d,
        eps,
        weight_bias,
        rows
    );
    return cudaGetLastError();
}

extern "C" cudaError_t ctox_qwen35_35b_moe_route_topk_bf16_launch(
    const void* router_logits,
    void* topk_weights,
    int32_t* topk_ids,
    int top_k,
    int num_experts,
    int n_tokens,
    cudaStream_t stream
) {
    ctox_qwen35_35b_moe_route_topk_bf16_kernel<<<n_tokens, 1, 0, stream>>>(
        static_cast<const __nv_bfloat16*>(router_logits),
        static_cast<__nv_bfloat16*>(topk_weights),
        topk_ids,
        top_k,
        num_experts,
        n_tokens
    );
    return cudaGetLastError();
}

extern "C" cudaError_t ctox_qwen35_35b_dense_matmul_bf16_launch(
    const void* x,
    const void* w,
    const void* bias,
    void* y,
    int rows,
    int in_dim,
    int out_dim,
    bool has_bias,
    cudaStream_t stream
) {
    const int threads = 256;
    const dim3 grid((out_dim + threads - 1) / threads, rows, 1);
    ctox_qwen35_35b_dense_matmul_bf16_kernel<<<grid, threads, 0, stream>>>(
        static_cast<const __nv_bfloat16*>(x),
        static_cast<const __nv_bfloat16*>(w),
        static_cast<const __nv_bfloat16*>(bias),
        static_cast<__nv_bfloat16*>(y),
        rows,
        in_dim,
        out_dim,
        has_bias
    );
    return cudaGetLastError();
}

extern "C" cudaError_t ctox_qwen35_35b_add_bf16_launch(
    const void* a,
    const void* b,
    void* y,
    int n,
    cudaStream_t stream
) {
    const int threads = 256;
    const int blocks = (n + threads - 1) / threads;
    ctox_qwen35_35b_add_bf16_kernel<<<blocks, threads, 0, stream>>>(
        static_cast<const __nv_bfloat16*>(a),
        static_cast<const __nv_bfloat16*>(b),
        static_cast<__nv_bfloat16*>(y),
        n
    );
    return cudaGetLastError();
}

extern "C" cudaError_t ctox_qwen35_35b_mul_bf16_launch(
    const void* a,
    const void* b,
    void* y,
    int n,
    cudaStream_t stream
) {
    const int threads = 256;
    const int blocks = (n + threads - 1) / threads;
    ctox_qwen35_35b_mul_bf16_kernel<<<blocks, threads, 0, stream>>>(
        static_cast<const __nv_bfloat16*>(a),
        static_cast<const __nv_bfloat16*>(b),
        static_cast<__nv_bfloat16*>(y),
        n
    );
    return cudaGetLastError();
}

extern "C" cudaError_t ctox_qwen35_35b_silu_bf16_launch(
    const void* x,
    void* y,
    int n,
    cudaStream_t stream
) {
    const int threads = 256;
    const int blocks = (n + threads - 1) / threads;
    ctox_qwen35_35b_silu_bf16_kernel<<<blocks, threads, 0, stream>>>(
        static_cast<const __nv_bfloat16*>(x),
        static_cast<__nv_bfloat16*>(y),
        n
    );
    return cudaGetLastError();
}

extern "C" cudaError_t ctox_qwen35_35b_argmax_bf16_launch(
    const void* x,
    int32_t* out,
    int vocab,
    int rows,
    cudaStream_t stream
) {
    ctox_qwen35_35b_argmax_bf16_kernel<<<rows, 1, 0, stream>>>(
        static_cast<const __nv_bfloat16*>(x),
        out,
        vocab,
        rows
    );
    return cudaGetLastError();
}

extern "C" cudaError_t ctox_qwen35_35b_copy_hidden_slot_bf16_launch(
    const void* src,
    void* dst,
    int src_row,
    int dst_slot,
    int hidden,
    int dst_slots,
    cudaStream_t stream
) {
    if (dst_slot < 0 || dst_slot >= dst_slots) {
        return cudaErrorInvalidValue;
    }
    const int threads = 256;
    const int blocks = (hidden + threads - 1) / threads;
    ctox_qwen35_35b_copy_hidden_slot_bf16_kernel<<<blocks, threads, 0, stream>>>(
        static_cast<const __nv_bfloat16*>(src),
        static_cast<__nv_bfloat16*>(dst),
        src_row,
        dst_slot,
        hidden,
        dst_slots
    );
    return cudaGetLastError();
}

extern "C" cudaError_t ctox_qwen35_35b_repeat_hidden_slots_bf16_launch(
    const void* src,
    void* dst,
    int hidden,
    int dst_slots,
    cudaStream_t stream
) {
    const int threads = 256;
    const int n = hidden * dst_slots;
    const int blocks = (n + threads - 1) / threads;
    ctox_qwen35_35b_repeat_hidden_slots_bf16_kernel<<<blocks, threads, 0, stream>>>(
        static_cast<const __nv_bfloat16*>(src),
        static_cast<__nv_bfloat16*>(dst),
        hidden,
        dst_slots
    );
    return cudaGetLastError();
}

extern "C" cudaError_t ctox_qwen35_35b_fill_positions4_i32_launch(
    int32_t* out,
    int start_pos,
    int n_tokens,
    cudaStream_t stream
) {
    const int threads = 256;
    const int n = n_tokens * 4;
    const int blocks = (n + threads - 1) / threads;
    ctox_qwen35_35b_fill_positions4_i32_kernel<<<blocks, threads, 0, stream>>>(
        out,
        start_pos,
        n_tokens
    );
    return cudaGetLastError();
}

extern "C" cudaError_t ctox_qwen35_35b_causal_mask_f16_launch(
    void* out,
    int kv_start,
    int n_tokens,
    int kv_len,
    int q_stride,
    cudaStream_t stream
) {
    const int threads = 256;
    const int n = n_tokens * q_stride;
    const int blocks = (n + threads - 1) / threads;
    ctox_qwen35_35b_causal_mask_f16_kernel<<<blocks, threads, 0, stream>>>(
        static_cast<__half*>(out),
        kv_start,
        n_tokens,
        kv_len,
        q_stride
    );
    return cudaGetLastError();
}

extern "C" cudaError_t ctox_qwen35_35b_kv_store_bf16_launch(
    const void* src,
    void* cache,
    const int32_t* positions4,
    int n_tokens,
    int n_kv_heads,
    int head_dim,
    int max_ctx,
    cudaStream_t stream
) {
    const int threads = 256;
    const int n = n_tokens * n_kv_heads * head_dim;
    const int blocks = (n + threads - 1) / threads;
    ctox_qwen35_35b_kv_store_bf16_kernel<<<blocks, threads, 0, stream>>>(
        static_cast<const __nv_bfloat16*>(src),
        static_cast<__nv_bfloat16*>(cache),
        positions4,
        n_tokens,
        n_kv_heads,
        head_dim,
        max_ctx
    );
    return cudaGetLastError();
}

extern "C" cudaError_t ctox_qwen35_35b_sdpa_decode_bf16_launch(
    const void* q,
    const void* k_cache,
    const void* v_cache,
    void* out,
    int n_q_heads,
    int n_kv_heads,
    int head_dim,
    int kv_len,
    int max_ctx,
    float scale,
    cudaStream_t stream
) {
    ctox_qwen35_35b_sdpa_decode_bf16_kernel<<<n_q_heads, 256, 0, stream>>>(
        static_cast<const __nv_bfloat16*>(q),
        static_cast<const __nv_bfloat16*>(k_cache),
        static_cast<const __nv_bfloat16*>(v_cache),
        static_cast<__nv_bfloat16*>(out),
        n_q_heads,
        n_kv_heads,
        head_dim,
        kv_len,
        max_ctx,
        scale
    );
    return cudaGetLastError();
}

extern "C" cudaError_t ctox_qwen35_35b_dequant_q4_k_bf16_launch(
    const void* x,
    void* y,
    int n_blocks,
    cudaStream_t stream
) {
    ctox_qwen35_35b_dequant_q4_k_bf16_kernel<<<n_blocks, 32, 0, stream>>>(
        static_cast<const ctox_q4_k_block*>(x),
        static_cast<__nv_bfloat16*>(y),
        n_blocks
    );
    return cudaGetLastError();
}

extern "C" cudaError_t ctox_qwen35_35b_q4_k_matvec_bf16_launch(
    const void* w,
    const void* x,
    void* y,
    int in_dim,
    int out_dim,
    cudaStream_t stream
) {
    ctox_qwen35_35b_q4_k_matvec_bf16_kernel<<<out_dim, 1, 0, stream>>>(
        static_cast<const ctox_q4_k_block*>(w),
        static_cast<const __nv_bfloat16*>(x),
        static_cast<__nv_bfloat16*>(y),
        in_dim,
        out_dim
    );
    return cudaGetLastError();
}
