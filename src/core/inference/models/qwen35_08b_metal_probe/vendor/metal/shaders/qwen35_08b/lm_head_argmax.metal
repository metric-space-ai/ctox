#include <metal_stdlib>
using namespace metal;

kernel void qwen35_08b_lm_head_score_pairs_fp16_k1024(
    device const half* x [[buffer(0)]],
    device const half* w [[buffer(1)]],
    device float* scores [[buffer(2)]],
    device uint* ids [[buffer(3)]],
    constant uint& rows [[buffer(4)]],
    uint row [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]]
) {
    if (row >= rows) {
        return;
    }

    threadgroup float partial[256];
    float acc = 0.0f;
    const uint cols = 1024;
    const uint base = row * cols;

    for (uint col = tid; col < cols; col += 256) {
        acc += float(w[base + col]) * float(x[col]);
    }

    partial[tid] = acc;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            partial[tid] += partial[tid + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    if (tid == 0) {
        scores[row] = partial[0];
        ids[row] = row;
    }
}

kernel void qwen35_08b_argmax_pairs_reduce_f32(
    device const float* scores_in [[buffer(0)]],
    device const uint* ids_in [[buffer(1)]],
    device float* scores_out [[buffer(2)]],
    device uint* ids_out [[buffer(3)]],
    constant uint& n [[buffer(4)]],
    uint group [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]]
) {
    threadgroup float score_scratch[256];
    threadgroup uint id_scratch[256];

    const uint idx = group * 256 + tid;
    float score = -3.402823466e+38f;
    uint id = 0;
    if (idx < n) {
        score = scores_in[idx];
        id = ids_in[idx];
    }

    score_scratch[tid] = score;
    id_scratch[tid] = id;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint stride = 128; stride > 0; stride >>= 1) {
        if (tid < stride) {
            float other_score = score_scratch[tid + stride];
            uint other_id = id_scratch[tid + stride];
            bool take_other = (other_score > score_scratch[tid]) ||
                (other_score == score_scratch[tid] && other_id < id_scratch[tid]);
            if (take_other) {
                score_scratch[tid] = other_score;
                id_scratch[tid] = other_id;
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    if (tid == 0) {
        scores_out[group] = score_scratch[0];
        ids_out[group] = id_scratch[0];
    }
}

kernel void qwen35_08b_argmax_pair_to_token_score(
    device const float* scores_in [[buffer(0)]],
    device const uint* ids_in [[buffer(1)]],
    device uint* out_token [[buffer(2)]],
    device float* out_score [[buffer(3)]],
    uint tid [[thread_position_in_grid]]
) {
    if (tid == 0) {
        out_token[0] = ids_in[0];
        out_score[0] = scores_in[0];
    }
}
