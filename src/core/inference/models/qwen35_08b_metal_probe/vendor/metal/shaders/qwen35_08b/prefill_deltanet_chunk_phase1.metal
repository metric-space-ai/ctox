#include <metal_stdlib>
using namespace metal;

static inline float qwen35_chunk8_reduce_sum_128(
    float value,
    threadgroup float* simd_partials,
    ushort tid,
    ushort simd_lane,
    ushort simd_group
) {
    const float simd_total = simd_sum(value);
    if (simd_lane == 0) {
        simd_partials[simd_group] = simd_total;
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    if (tid == 0) {
        simd_partials[0] =
            simd_partials[0] + simd_partials[1] + simd_partials[2] + simd_partials[3];
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    const float result = simd_partials[0];
    threadgroup_barrier(mem_flags::mem_threadgroup);
    return result;
}

kernel void qwen35_08b_prefill_deltanet_chunk8_phase1_kdot_h16d128(
    device const half* k_tokens [[buffer(0)]],
    device const float* beta_tokens [[buffer(1)]],
    device const float* decay_tokens [[buffer(2)]],
    device float* kdot_chunks [[buffer(3)]],
    device float* lower_beta_kdot_chunks [[buffer(4)]],
    device float* decay_prefix_chunks [[buffer(5)]],
    constant uint& tokens [[buffer(6)]],
    uint2 tg_pos [[threadgroup_position_in_grid]],
    uint2 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;
    constexpr uint chunk = 8;
    constexpr uint pair_count = chunk * chunk;

    const uint chunk_id = tg_pos.x;
    const uint head = tg_pos.y;
    const uint pair = tid_pos.x;
    if (head >= heads || pair >= pair_count) {
        return;
    }

    const uint chunks = (tokens + chunk - 1) / chunk;
    const uint base_token = chunk_id * chunk;
    const uint i = pair / chunk;
    const uint j = pair - i * chunk;
    const uint token_i = base_token + i;
    const uint token_j = base_token + j;
    const uint out_base = (chunk_id * heads + head) * pair_count;
    const uint prefix_base = (chunk_id * heads + head) * chunk;

    float dot = 0.0f;
    if (chunk_id < chunks && token_i < tokens && token_j < tokens) {
        const uint ki_base = token_i * width + head * head_dim;
        const uint kj_base = token_j * width + head * head_dim;
        for (uint col = 0; col < head_dim; ++col) {
            dot += float(k_tokens[ki_base + col]) * float(k_tokens[kj_base + col]);
        }
    }

    const float beta_i = (token_i < tokens) ? beta_tokens[token_i * heads + head] : 0.0f;
    kdot_chunks[out_base + pair] = dot;
    lower_beta_kdot_chunks[out_base + pair] = (j < i) ? beta_i * dot : 0.0f;

    if (pair < chunk) {
        float prefix = 1.0f;
        for (uint p = 0; p <= pair; ++p) {
            const uint token_p = base_token + p;
            if (token_p < tokens) {
                prefix *= decay_tokens[token_p * heads + head];
            }
        }
        decay_prefix_chunks[prefix_base + pair] = (base_token + pair < tokens) ? prefix : 0.0f;
    }
}

kernel void qwen35_08b_prefill_deltanet_chunk8_phase2_local_zero_h16d128(
    device const half* q_tokens [[buffer(0)]],
    device const half* k_tokens [[buffer(1)]],
    device const half* v_tokens [[buffer(2)]],
    device const float* beta_tokens [[buffer(3)]],
    device const float* decay_tokens [[buffer(4)]],
    device float* local_out_tokens [[buffer(5)]],
    device float* local_state_chunks [[buffer(6)]],
    constant uint& tokens [[buffer(7)]],
    constant uint& chunk_tokens [[buffer(8)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    ushort tid [[thread_index_in_threadgroup]],
    ushort simd_lane [[thread_index_in_simdgroup]],
    ushort simd_group [[simdgroup_index_in_threadgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;
    const uint chunk = max(1u, chunk_tokens);

    threadgroup float partials[4];

    const uint chunk_id = tg_pos.x;
    const uint head = tg_pos.y;
    const uint row = tg_pos.z;
    const uint col = tid;
    if (head >= heads || row >= head_dim || col >= head_dim) {
        return;
    }

    const uint chunks = (tokens + chunk - 1) / chunk;
    const uint base_token = chunk_id * chunk;
    float state_lane = 0.0f;

    if (chunk_id < chunks) {
        for (uint local_t = 0; local_t < chunk; ++local_t) {
            const uint token = base_token + local_t;
            if (token >= tokens) {
                break;
            }

            const uint token_base = token * width + head * head_dim;
            const float k_lane = float(k_tokens[token_base + col]);
            const float q_lane = float(q_tokens[token_base + col]);
            const float v_row = float(v_tokens[token_base + row]);
            const float beta = beta_tokens[token * heads + head];
            const float decay = decay_tokens[token * heads + head];

            const float kv_part = state_lane * decay * k_lane;
            const float kv_mem = qwen35_chunk8_reduce_sum_128(
                kv_part,
                partials,
                tid,
                simd_lane,
                simd_group
            );
            const float delta = (v_row - kv_mem) * beta;
            state_lane = state_lane * decay + k_lane * delta;

            const float out_part = state_lane * q_lane;
            const float out_value = qwen35_chunk8_reduce_sum_128(
                out_part,
                partials,
                tid,
                simd_lane,
                simd_group
            );
            if (tid == 0) {
                local_out_tokens[token_base + row] = out_value;
            }
        }
    }

    const uint state_base = ((chunk_id * heads + head) * head_dim + row) * head_dim;
    local_state_chunks[state_base + col] = state_lane;
}

kernel void qwen35_08b_prefill_deltanet_chunk8_phase3_propagate_h16d128(
    device const half* q_tokens [[buffer(0)]],
    device const half* k_tokens [[buffer(1)]],
    device const float* beta_tokens [[buffer(2)]],
    device const float* decay_tokens [[buffer(3)]],
    device const float* initial_state [[buffer(4)]],
    device const float* local_out_tokens [[buffer(5)]],
    device const float* local_state_chunks [[buffer(6)]],
    device float* final_out_tokens [[buffer(7)]],
    device float* final_state [[buffer(8)]],
    constant uint& tokens [[buffer(9)]],
    constant uint& chunk_tokens [[buffer(10)]],
    uint2 tg_pos [[threadgroup_position_in_grid]],
    ushort tid [[thread_index_in_threadgroup]],
    ushort simd_lane [[thread_index_in_simdgroup]],
    ushort simd_group [[simdgroup_index_in_threadgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;
    const uint chunk = max(1u, chunk_tokens);

    threadgroup float partials[4];

    const uint head = tg_pos.x;
    const uint row = tg_pos.y;
    const uint col = tid;
    if (head >= heads || row >= head_dim || col >= head_dim) {
        return;
    }

    const uint chunks = (tokens + chunk - 1) / chunk;
    const uint state_row_base = (head * head_dim + row) * head_dim;
    float state_lane = initial_state[state_row_base + col];

    for (uint chunk_id = 0; chunk_id < chunks; ++chunk_id) {
        const uint base_token = chunk_id * chunk;
        for (uint local_t = 0; local_t < chunk; ++local_t) {
            const uint token = base_token + local_t;
            if (token >= tokens) {
                break;
            }

            const uint token_base = token * width + head * head_dim;
            const float k_lane = float(k_tokens[token_base + col]);
            const float q_lane = float(q_tokens[token_base + col]);
            const float beta = beta_tokens[token * heads + head];
            const float decay = decay_tokens[token * heads + head];

            const float kv_part = state_lane * decay * k_lane;
            const float kv_mem = qwen35_chunk8_reduce_sum_128(
                kv_part,
                partials,
                tid,
                simd_lane,
                simd_group
            );
            const float delta = -kv_mem * beta;
            state_lane = state_lane * decay + k_lane * delta;

            const float out_part = state_lane * q_lane;
            const float propagated_out = qwen35_chunk8_reduce_sum_128(
                out_part,
                partials,
                tid,
                simd_lane,
                simd_group
            );
            if (tid == 0) {
                final_out_tokens[token_base + row] =
                    local_out_tokens[token_base + row] + propagated_out;
            }
        }

        const uint local_state_base = ((chunk_id * heads + head) * head_dim + row) * head_dim;
        state_lane += local_state_chunks[local_state_base + col];
    }

    final_state[state_row_base + col] = state_lane;
}

kernel void qwen35_08b_prefill_deltanet_chunk8_phase2_local_zero_hstate_h16d128(
    device const half* q_tokens [[buffer(0)]],
    device const half* k_tokens [[buffer(1)]],
    device const half* v_tokens [[buffer(2)]],
    device const float* beta_tokens [[buffer(3)]],
    device const float* decay_tokens [[buffer(4)]],
    device float* local_out_tokens [[buffer(5)]],
    device half* local_state_chunks [[buffer(6)]],
    constant uint& tokens [[buffer(7)]],
    constant uint& chunk_tokens [[buffer(8)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    ushort tid [[thread_index_in_threadgroup]],
    ushort simd_lane [[thread_index_in_simdgroup]],
    ushort simd_group [[simdgroup_index_in_threadgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;
    const uint chunk = max(1u, chunk_tokens);

    threadgroup float partials[4];

    const uint chunk_id = tg_pos.x;
    const uint head = tg_pos.y;
    const uint row = tg_pos.z;
    const uint col = tid;
    if (head >= heads || row >= head_dim || col >= head_dim) {
        return;
    }

    const uint chunks = (tokens + chunk - 1) / chunk;
    const uint base_token = chunk_id * chunk;
    float state_lane = 0.0f;

    if (chunk_id < chunks) {
        for (uint local_t = 0; local_t < chunk; ++local_t) {
            const uint token = base_token + local_t;
            if (token >= tokens) {
                break;
            }

            const uint token_base = token * width + head * head_dim;
            const float k_lane = float(k_tokens[token_base + col]);
            const float q_lane = float(q_tokens[token_base + col]);
            const float v_row = float(v_tokens[token_base + row]);
            const float beta = beta_tokens[token * heads + head];
            const float decay = decay_tokens[token * heads + head];

            const float kv_part = state_lane * decay * k_lane;
            const float kv_mem = qwen35_chunk8_reduce_sum_128(
                kv_part,
                partials,
                tid,
                simd_lane,
                simd_group
            );
            const float delta = (v_row - kv_mem) * beta;
            state_lane = state_lane * decay + k_lane * delta;

            const float out_part = state_lane * q_lane;
            const float out_value = qwen35_chunk8_reduce_sum_128(
                out_part,
                partials,
                tid,
                simd_lane,
                simd_group
            );
            if (tid == 0) {
                local_out_tokens[token_base + row] = out_value;
            }
        }
    }

    const uint state_base = ((chunk_id * heads + head) * head_dim + row) * head_dim;
    local_state_chunks[state_base + col] = half(state_lane);
}

kernel void qwen35_08b_prefill_deltanet_chunk8_phase3_propagate_hstate_h16d128(
    device const half* q_tokens [[buffer(0)]],
    device const half* k_tokens [[buffer(1)]],
    device const float* beta_tokens [[buffer(2)]],
    device const float* decay_tokens [[buffer(3)]],
    device const float* initial_state [[buffer(4)]],
    device const float* local_out_tokens [[buffer(5)]],
    device const half* local_state_chunks [[buffer(6)]],
    device float* final_out_tokens [[buffer(7)]],
    device float* final_state [[buffer(8)]],
    constant uint& tokens [[buffer(9)]],
    constant uint& chunk_tokens [[buffer(10)]],
    uint2 tg_pos [[threadgroup_position_in_grid]],
    ushort tid [[thread_index_in_threadgroup]],
    ushort simd_lane [[thread_index_in_simdgroup]],
    ushort simd_group [[simdgroup_index_in_threadgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;
    const uint chunk = max(1u, chunk_tokens);

    threadgroup float partials[4];

    const uint head = tg_pos.x;
    const uint row = tg_pos.y;
    const uint col = tid;
    if (head >= heads || row >= head_dim || col >= head_dim) {
        return;
    }

    const uint chunks = (tokens + chunk - 1) / chunk;
    const uint state_row_base = (head * head_dim + row) * head_dim;
    float state_lane = initial_state[state_row_base + col];

    for (uint chunk_id = 0; chunk_id < chunks; ++chunk_id) {
        const uint base_token = chunk_id * chunk;
        for (uint local_t = 0; local_t < chunk; ++local_t) {
            const uint token = base_token + local_t;
            if (token >= tokens) {
                break;
            }

            const uint token_base = token * width + head * head_dim;
            const float k_lane = float(k_tokens[token_base + col]);
            const float q_lane = float(q_tokens[token_base + col]);
            const float beta = beta_tokens[token * heads + head];
            const float decay = decay_tokens[token * heads + head];

            const float kv_part = state_lane * decay * k_lane;
            const float kv_mem = qwen35_chunk8_reduce_sum_128(
                kv_part,
                partials,
                tid,
                simd_lane,
                simd_group
            );
            const float delta = -kv_mem * beta;
            state_lane = state_lane * decay + k_lane * delta;

            const float out_part = state_lane * q_lane;
            const float propagated_out = qwen35_chunk8_reduce_sum_128(
                out_part,
                partials,
                tid,
                simd_lane,
                simd_group
            );
            if (tid == 0) {
                final_out_tokens[token_base + row] =
                    local_out_tokens[token_base + row] + propagated_out;
            }
        }

        const uint local_state_base = ((chunk_id * heads + head) * head_dim + row) * head_dim;
        state_lane += float(local_state_chunks[local_state_base + col]);
    }

    final_state[state_row_base + col] = state_lane;
}

kernel void qwen35_08b_prefill_deltanet_chunk8_phase2_local_zero_simd32x4_f32state_h16d128(
    device const half* q_tokens [[buffer(0)]],
    device const half* k_tokens [[buffer(1)]],
    device const half* v_tokens [[buffer(2)]],
    device const float* beta_tokens [[buffer(3)]],
    device const float* decay_tokens [[buffer(4)]],
    device float* local_out_tokens [[buffer(5)]],
    device float* local_state_chunks [[buffer(6)]],
    constant uint& tokens [[buffer(7)]],
    constant uint& chunk_tokens [[buffer(8)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    ushort lane [[thread_index_in_simdgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;
    const uint chunk = max(1u, chunk_tokens);

    const uint chunk_id = tg_pos.x;
    const uint head = tg_pos.y;
    const uint row = tg_pos.z;
    if (head >= heads || row >= head_dim) {
        return;
    }

    const uint cols[4] = { uint(lane), uint(lane) + 32u, uint(lane) + 64u, uint(lane) + 96u };
    const uint chunks = (tokens + chunk - 1) / chunk;
    const uint base_token = chunk_id * chunk;
    float s0 = 0.0f;
    float s1 = 0.0f;
    float s2 = 0.0f;
    float s3 = 0.0f;

    if (chunk_id < chunks) {
        for (uint local_t = 0; local_t < chunk; ++local_t) {
            const uint token = base_token + local_t;
            if (token >= tokens) {
                break;
            }

            const uint token_base = token * width + head * head_dim;
            const float k0 = float(k_tokens[token_base + cols[0]]);
            const float k1 = float(k_tokens[token_base + cols[1]]);
            const float k2 = float(k_tokens[token_base + cols[2]]);
            const float k3 = float(k_tokens[token_base + cols[3]]);
            const float beta = beta_tokens[token * heads + head];
            const float decay = decay_tokens[token * heads + head];
            const float kv_part = decay * (s0 * k0 + s1 * k1 + s2 * k2 + s3 * k3);
            const float kv_mem = simd_sum(kv_part);
            const float v_row = float(v_tokens[token_base + row]);
            const float delta = (v_row - kv_mem) * beta;

            s0 = s0 * decay + k0 * delta;
            s1 = s1 * decay + k1 * delta;
            s2 = s2 * decay + k2 * delta;
            s3 = s3 * decay + k3 * delta;

            const float q0 = float(q_tokens[token_base + cols[0]]);
            const float q1 = float(q_tokens[token_base + cols[1]]);
            const float q2 = float(q_tokens[token_base + cols[2]]);
            const float q3 = float(q_tokens[token_base + cols[3]]);
            const float out_value = simd_sum(s0 * q0 + s1 * q1 + s2 * q2 + s3 * q3);
            if (lane == 0) {
                local_out_tokens[token_base + row] = out_value;
            }
        }
    }

    const uint state_base = ((chunk_id * heads + head) * head_dim + row) * head_dim;
    local_state_chunks[state_base + cols[0]] = s0;
    local_state_chunks[state_base + cols[1]] = s1;
    local_state_chunks[state_base + cols[2]] = s2;
    local_state_chunks[state_base + cols[3]] = s3;
}

kernel void qwen35_08b_prefill_deltanet_chunk8_phase3_propagate_simd32x4_f32state_h16d128(
    device const half* q_tokens [[buffer(0)]],
    device const half* k_tokens [[buffer(1)]],
    device const float* beta_tokens [[buffer(2)]],
    device const float* decay_tokens [[buffer(3)]],
    device const float* initial_state [[buffer(4)]],
    device const float* local_out_tokens [[buffer(5)]],
    device const float* local_state_chunks [[buffer(6)]],
    device float* final_out_tokens [[buffer(7)]],
    device float* final_state [[buffer(8)]],
    constant uint& tokens [[buffer(9)]],
    constant uint& chunk_tokens [[buffer(10)]],
    uint2 tg_pos [[threadgroup_position_in_grid]],
    ushort lane [[thread_index_in_simdgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;
    const uint chunk = max(1u, chunk_tokens);

    const uint head = tg_pos.x;
    const uint row = tg_pos.y;
    if (head >= heads || row >= head_dim) {
        return;
    }

    const uint cols[4] = { uint(lane), uint(lane) + 32u, uint(lane) + 64u, uint(lane) + 96u };
    const uint chunks = (tokens + chunk - 1) / chunk;
    const uint state_row_base = (head * head_dim + row) * head_dim;
    float s0 = initial_state[state_row_base + cols[0]];
    float s1 = initial_state[state_row_base + cols[1]];
    float s2 = initial_state[state_row_base + cols[2]];
    float s3 = initial_state[state_row_base + cols[3]];

    for (uint chunk_id = 0; chunk_id < chunks; ++chunk_id) {
        const uint base_token = chunk_id * chunk;
        for (uint local_t = 0; local_t < chunk; ++local_t) {
            const uint token = base_token + local_t;
            if (token >= tokens) {
                break;
            }

            const uint token_base = token * width + head * head_dim;
            const float k0 = float(k_tokens[token_base + cols[0]]);
            const float k1 = float(k_tokens[token_base + cols[1]]);
            const float k2 = float(k_tokens[token_base + cols[2]]);
            const float k3 = float(k_tokens[token_base + cols[3]]);
            const float beta = beta_tokens[token * heads + head];
            const float decay = decay_tokens[token * heads + head];
            const float kv_part = decay * (s0 * k0 + s1 * k1 + s2 * k2 + s3 * k3);
            const float kv_mem = simd_sum(kv_part);
            const float delta = -kv_mem * beta;

            s0 = s0 * decay + k0 * delta;
            s1 = s1 * decay + k1 * delta;
            s2 = s2 * decay + k2 * delta;
            s3 = s3 * decay + k3 * delta;

            const float q0 = float(q_tokens[token_base + cols[0]]);
            const float q1 = float(q_tokens[token_base + cols[1]]);
            const float q2 = float(q_tokens[token_base + cols[2]]);
            const float q3 = float(q_tokens[token_base + cols[3]]);
            const float propagated_out = simd_sum(s0 * q0 + s1 * q1 + s2 * q2 + s3 * q3);
            if (lane == 0) {
                final_out_tokens[token_base + row] =
                    local_out_tokens[token_base + row] + propagated_out;
            }
        }

        const uint local_state_base = ((chunk_id * heads + head) * head_dim + row) * head_dim;
        s0 += local_state_chunks[local_state_base + cols[0]];
        s1 += local_state_chunks[local_state_base + cols[1]];
        s2 += local_state_chunks[local_state_base + cols[2]];
        s3 += local_state_chunks[local_state_base + cols[3]];
    }

    final_state[state_row_base + cols[0]] = s0;
    final_state[state_row_base + cols[1]] = s1;
    final_state[state_row_base + cols[2]] = s2;
    final_state[state_row_base + cols[3]] = s3;
}

kernel void qwen35_08b_prefill_deltanet_chunk8_phase2_local_zero_simd32x4_hstate_h16d128(
    device const half* q_tokens [[buffer(0)]],
    device const half* k_tokens [[buffer(1)]],
    device const half* v_tokens [[buffer(2)]],
    device const float* beta_tokens [[buffer(3)]],
    device const float* decay_tokens [[buffer(4)]],
    device float* local_out_tokens [[buffer(5)]],
    device half* local_state_chunks [[buffer(6)]],
    constant uint& tokens [[buffer(7)]],
    constant uint& chunk_tokens [[buffer(8)]],
    uint3 tg_pos [[threadgroup_position_in_grid]],
    ushort lane [[thread_index_in_simdgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;
    const uint chunk = max(1u, chunk_tokens);

    const uint chunk_id = tg_pos.x;
    const uint head = tg_pos.y;
    const uint row = tg_pos.z;
    if (head >= heads || row >= head_dim) {
        return;
    }

    const uint cols[4] = { uint(lane), uint(lane) + 32u, uint(lane) + 64u, uint(lane) + 96u };
    const uint chunks = (tokens + chunk - 1) / chunk;
    const uint base_token = chunk_id * chunk;
    float s0 = 0.0f;
    float s1 = 0.0f;
    float s2 = 0.0f;
    float s3 = 0.0f;

    if (chunk_id < chunks) {
        for (uint local_t = 0; local_t < chunk; ++local_t) {
            const uint token = base_token + local_t;
            if (token >= tokens) {
                break;
            }

            const uint token_base = token * width + head * head_dim;
            const float k0 = float(k_tokens[token_base + cols[0]]);
            const float k1 = float(k_tokens[token_base + cols[1]]);
            const float k2 = float(k_tokens[token_base + cols[2]]);
            const float k3 = float(k_tokens[token_base + cols[3]]);
            const float beta = beta_tokens[token * heads + head];
            const float decay = decay_tokens[token * heads + head];
            const float kv_mem = simd_sum(decay * (s0 * k0 + s1 * k1 + s2 * k2 + s3 * k3));
            const float v_row = float(v_tokens[token_base + row]);
            const float delta = (v_row - kv_mem) * beta;

            s0 = s0 * decay + k0 * delta;
            s1 = s1 * decay + k1 * delta;
            s2 = s2 * decay + k2 * delta;
            s3 = s3 * decay + k3 * delta;

            const float q0 = float(q_tokens[token_base + cols[0]]);
            const float q1 = float(q_tokens[token_base + cols[1]]);
            const float q2 = float(q_tokens[token_base + cols[2]]);
            const float q3 = float(q_tokens[token_base + cols[3]]);
            const float out_value = simd_sum(s0 * q0 + s1 * q1 + s2 * q2 + s3 * q3);
            if (lane == 0) {
                local_out_tokens[token_base + row] = out_value;
            }
        }
    }

    const uint state_base = ((chunk_id * heads + head) * head_dim + row) * head_dim;
    local_state_chunks[state_base + cols[0]] = half(s0);
    local_state_chunks[state_base + cols[1]] = half(s1);
    local_state_chunks[state_base + cols[2]] = half(s2);
    local_state_chunks[state_base + cols[3]] = half(s3);
}

kernel void qwen35_08b_prefill_deltanet_chunk8_phase3_propagate_simd32x4_hstate_h16d128(
    device const half* q_tokens [[buffer(0)]],
    device const half* k_tokens [[buffer(1)]],
    device const float* beta_tokens [[buffer(2)]],
    device const float* decay_tokens [[buffer(3)]],
    device const float* initial_state [[buffer(4)]],
    device const float* local_out_tokens [[buffer(5)]],
    device const half* local_state_chunks [[buffer(6)]],
    device float* final_out_tokens [[buffer(7)]],
    device float* final_state [[buffer(8)]],
    constant uint& tokens [[buffer(9)]],
    constant uint& chunk_tokens [[buffer(10)]],
    uint2 tg_pos [[threadgroup_position_in_grid]],
    ushort lane [[thread_index_in_simdgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;
    const uint chunk = max(1u, chunk_tokens);

    const uint head = tg_pos.x;
    const uint row = tg_pos.y;
    if (head >= heads || row >= head_dim) {
        return;
    }

    const uint cols[4] = { uint(lane), uint(lane) + 32u, uint(lane) + 64u, uint(lane) + 96u };
    const uint chunks = (tokens + chunk - 1) / chunk;
    const uint state_row_base = (head * head_dim + row) * head_dim;
    float s0 = initial_state[state_row_base + cols[0]];
    float s1 = initial_state[state_row_base + cols[1]];
    float s2 = initial_state[state_row_base + cols[2]];
    float s3 = initial_state[state_row_base + cols[3]];

    for (uint chunk_id = 0; chunk_id < chunks; ++chunk_id) {
        const uint base_token = chunk_id * chunk;
        for (uint local_t = 0; local_t < chunk; ++local_t) {
            const uint token = base_token + local_t;
            if (token >= tokens) {
                break;
            }

            const uint token_base = token * width + head * head_dim;
            const float k0 = float(k_tokens[token_base + cols[0]]);
            const float k1 = float(k_tokens[token_base + cols[1]]);
            const float k2 = float(k_tokens[token_base + cols[2]]);
            const float k3 = float(k_tokens[token_base + cols[3]]);
            const float beta = beta_tokens[token * heads + head];
            const float decay = decay_tokens[token * heads + head];
            const float kv_mem = simd_sum(decay * (s0 * k0 + s1 * k1 + s2 * k2 + s3 * k3));
            const float delta = -kv_mem * beta;

            s0 = s0 * decay + k0 * delta;
            s1 = s1 * decay + k1 * delta;
            s2 = s2 * decay + k2 * delta;
            s3 = s3 * decay + k3 * delta;

            const float q0 = float(q_tokens[token_base + cols[0]]);
            const float q1 = float(q_tokens[token_base + cols[1]]);
            const float q2 = float(q_tokens[token_base + cols[2]]);
            const float q3 = float(q_tokens[token_base + cols[3]]);
            const float propagated_out = simd_sum(s0 * q0 + s1 * q1 + s2 * q2 + s3 * q3);
            if (lane == 0) {
                final_out_tokens[token_base + row] =
                    local_out_tokens[token_base + row] + propagated_out;
            }
        }

        const uint local_state_base = ((chunk_id * heads + head) * head_dim + row) * head_dim;
        s0 += float(local_state_chunks[local_state_base + cols[0]]);
        s1 += float(local_state_chunks[local_state_base + cols[1]]);
        s2 += float(local_state_chunks[local_state_base + cols[2]]);
        s3 += float(local_state_chunks[local_state_base + cols[3]]);
    }

    final_state[state_row_base + cols[0]] = s0;
    final_state[state_row_base + cols[1]] = s1;
    final_state[state_row_base + cols[2]] = s2;
    final_state[state_row_base + cols[3]] = s3;
}
