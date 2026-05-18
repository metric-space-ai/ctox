#include <metal_stdlib>
using namespace metal;

kernel void qwen35_08b_synthetic_mega_decode_fp16(
    device const uint* token_in [[buffer(0)]],
    device const half* embedding [[buffer(1)]],
    device const half* layer_weights [[buffer(2)]],
    device const half* lm_head [[buffer(3)]],
    device uint* token_out [[buffer(4)]],
    device float* score_out [[buffer(5)]],
    constant uint& vocab_rows [[buffer(6)]],
    constant uint& n_layers [[buffer(7)]],
    uint tid [[thread_position_in_threadgroup]]
) {
    threadgroup half hidden[1024];
    threadgroup half next_hidden[1024];
    threadgroup float partial[256];
    threadgroup float best_scores[256];
    threadgroup uint best_ids[256];

    uint token = token_in[0];
    if (token >= vocab_rows) {
        token = 0;
    }

    for (uint col = tid; col < 1024; col += 256) {
        hidden[col] = embedding[token * 1024 + col];
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint layer = 0; layer < n_layers; ++layer) {
        float ss = 0.0f;
        for (uint col = tid; col < 1024; col += 256) {
            float v = float(hidden[col]);
            ss += v * v;
        }
        partial[tid] = ss;
        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint stride = 128; stride > 0; stride >>= 1) {
            if (tid < stride) {
                partial[tid] += partial[tid + stride];
            }
            threadgroup_barrier(mem_flags::mem_threadgroup);
        }

        float inv_rms = rsqrt(partial[0] / 1024.0f + 1.0e-6f);
        threadgroup_barrier(mem_flags::mem_threadgroup);

        const uint layer_base = layer * 1024 * 1024;
        for (uint row = tid; row < 1024; row += 256) {
            float acc = 0.0f;
            const uint w_base = layer_base + row * 1024;
            for (uint col = 0; col < 1024; ++col) {
                acc += float(layer_weights[w_base + col]) * (float(hidden[col]) * inv_rms);
            }
            float residual = float(hidden[row]);
            next_hidden[row] = half(residual + acc);
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint col = tid; col < 1024; col += 256) {
            hidden[col] = next_hidden[col];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    float local_best = -3.402823466e+38f;
    uint local_id = 0;
    for (uint row = tid; row < vocab_rows; row += 256) {
        float acc = 0.0f;
        const uint w_base = row * 1024;
        for (uint col = 0; col < 1024; ++col) {
            acc += float(lm_head[w_base + col]) * float(hidden[col]);
        }
        if ((acc > local_best) || (acc == local_best && row < local_id)) {
            local_best = acc;
            local_id = row;
        }
    }

    best_scores[tid] = local_best;
    best_ids[tid] = local_id;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            float other_score = best_scores[tid + stride];
            uint other_id = best_ids[tid + stride];
            bool take_other = (other_score > best_scores[tid]) ||
                (other_score == best_scores[tid] && other_id < best_ids[tid]);
            if (take_other) {
                best_scores[tid] = other_score;
                best_ids[tid] = other_id;
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    if (tid == 0) {
        token_out[0] = best_ids[0];
        score_out[0] = best_scores[0];
    }
}
