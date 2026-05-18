#include <metal_stdlib>
using namespace metal;

kernel void qwen35_08b_pattern_mega_decode_fp16(
    device const uint* token_in [[buffer(0)]],
    device const half* embedding [[buffer(1)]],
    device const half* attention_weights [[buffer(2)]],
    device float* recurrent_state [[buffer(3)]],
    device const half* lm_head [[buffer(4)]],
    device uint* token_out [[buffer(5)]],
    device float* score_out [[buffer(6)]],
    constant uint& vocab_rows [[buffer(7)]],
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

    uint d_layer = 0;
    uint a_layer = 0;
    for (uint layer = 0; layer < 24; ++layer) {
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
        const float inv_rms = rsqrt(partial[0] / 1024.0f + 1.0e-6f);
        threadgroup_barrier(mem_flags::mem_threadgroup);

        if ((layer & 3u) != 3u) {
            const uint state_base = d_layer * 1024;
            for (uint col = tid; col < 1024; col += 256) {
                float x = float(hidden[col]) * inv_rms;
                float s = recurrent_state[state_base + col];
                // Synthetic DeltaNet-like recurrent update: stateful, gated,
                // residual-producing, but vector-shaped to fit one threadgroup.
                float beta = 0.25f + 0.01f * float(d_layer % 7u);
                float gate = 0.96f - 0.002f * float(d_layer % 11u);
                float delta = (x - s) * beta;
                s = s * gate + delta;
                recurrent_state[state_base + col] = s;
                next_hidden[col] = half(float(hidden[col]) + s);
            }
            d_layer += 1;
        } else {
            const uint weight_base = a_layer * 1024 * 1024;
            for (uint row = tid; row < 1024; row += 256) {
                float acc = 0.0f;
                const uint w_base = weight_base + row * 1024;
                for (uint col = 0; col < 1024; ++col) {
                    acc += float(attention_weights[w_base + col]) *
                        (float(hidden[col]) * inv_rms);
                }
                next_hidden[row] = half(float(hidden[row]) + acc);
            }
            a_layer += 1;
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
