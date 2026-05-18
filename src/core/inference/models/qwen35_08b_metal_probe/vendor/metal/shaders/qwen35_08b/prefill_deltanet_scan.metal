#include <metal_stdlib>
using namespace metal;

static inline float qwen35_08b_prefill_deltanet_scan_silu(float x) {
    return x / (1.0f + exp(-clamp(x, -20.0f, 20.0f)));
}

kernel void qwen35_08b_prefill_deltanet_scan_f32_state_tok_h16d128(
    device const half* q_tokens [[buffer(0)]],
    device const half* k_tokens [[buffer(1)]],
    device const half* v_tokens [[buffer(2)]],
    device const float* beta_tokens [[buffer(3)]],
    device const float* decay_tokens [[buffer(4)]],
    device float* state [[buffer(5)]],
    device float* out_tokens [[buffer(6)]],
    constant uint& tokens [[buffer(7)]],
    uint head [[threadgroup_position_in_grid]],
    uint row [[thread_position_in_threadgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;

    if (head >= heads || row >= head_dim) {
        return;
    }

    threadgroup float q_s[head_dim];
    threadgroup float k_s[head_dim];

    const uint vec_head_base = head * head_dim;
    const uint state_base = head * head_dim * head_dim;
    const uint row_state_base = state_base + row * head_dim;

    for (uint token = 0; token < tokens; ++token) {
        const uint vec_base = token * width + vec_head_base;
        const uint idx = vec_base + row;
        q_s[row] = float(q_tokens[idx]);
        k_s[row] = float(k_tokens[idx]);
        threadgroup_barrier(mem_flags::mem_threadgroup);

        const float beta = beta_tokens[token * heads + head];
        const float decay = decay_tokens[token * heads + head];

        float kv_mem = 0.0f;
        for (uint col = 0; col < head_dim; ++col) {
            kv_mem += state[row_state_base + col] * decay * k_s[col];
        }

        const float delta = (float(v_tokens[idx]) - kv_mem) * beta;

        float acc = 0.0f;
        for (uint col = 0; col < head_dim; ++col) {
            const float next_state = state[row_state_base + col] * decay + k_s[col] * delta;
            state[row_state_base + col] = next_state;
            acc += next_state * q_s[col];
        }

        out_tokens[idx] = acc;
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
}

kernel void qwen35_08b_prefill_deltanet_scan_gated_norm_f32_state_tok_h16d128(
    device const half* q_tokens [[buffer(0)]],
    device const half* k_tokens [[buffer(1)]],
    device const half* v_tokens [[buffer(2)]],
    device const float* beta_tokens [[buffer(3)]],
    device const float* decay_tokens [[buffer(4)]],
    device float* state [[buffer(5)]],
    device const float* z_tokens [[buffer(6)]],
    device const float* norm_weight [[buffer(7)]],
    device half* gated_tokens [[buffer(8)]],
    constant uint& tokens [[buffer(9)]],
    uint head [[threadgroup_position_in_grid]],
    uint row [[thread_position_in_threadgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;
    constexpr float eps = 1.0e-6f;

    if (head >= heads || row >= head_dim) {
        return;
    }

    threadgroup float q_s[head_dim];
    threadgroup float k_s[head_dim];
    threadgroup float partial[head_dim];

    const uint vec_head_base = head * head_dim;
    const uint state_base = head * head_dim * head_dim;
    const uint row_state_base = state_base + row * head_dim;

    for (uint token = 0; token < tokens; ++token) {
        const uint vec_base = token * width + vec_head_base;
        const uint idx = vec_base + row;
        q_s[row] = float(q_tokens[idx]);
        k_s[row] = float(k_tokens[idx]);
        threadgroup_barrier(mem_flags::mem_threadgroup);

        const float beta = beta_tokens[token * heads + head];
        const float decay = decay_tokens[token * heads + head];

        float kv_mem = 0.0f;
        for (uint col = 0; col < head_dim; ++col) {
            kv_mem += state[row_state_base + col] * decay * k_s[col];
        }

        const float delta = (float(v_tokens[idx]) - kv_mem) * beta;

        float acc = 0.0f;
        for (uint col = 0; col < head_dim; ++col) {
            const float next_state = state[row_state_base + col] * decay + k_s[col] * delta;
            state[row_state_base + col] = next_state;
            acc += next_state * q_s[col];
        }

        partial[row] = acc * acc;
        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint stride = 64; stride > 0; stride >>= 1) {
            if (row < stride) {
                partial[row] += partial[row + stride];
            }
            threadgroup_barrier(mem_flags::mem_threadgroup);
        }

        const float inv_rms = rsqrt(partial[0] / float(head_dim) + eps);
        const float gate = qwen35_08b_prefill_deltanet_scan_silu(z_tokens[idx]);
        const float out = acc * inv_rms * norm_weight[row] * gate;
        gated_tokens[idx] = half(clamp(out, -65504.0f, 65504.0f));
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
}

kernel void qwen35_08b_prefill_deltanet_scan_rowcache_f32_state_tok_h16d128(
    device const half* q_tokens [[buffer(0)]],
    device const half* k_tokens [[buffer(1)]],
    device const half* v_tokens [[buffer(2)]],
    device const float* beta_tokens [[buffer(3)]],
    device const float* decay_tokens [[buffer(4)]],
    device float* state [[buffer(5)]],
    device float* out_tokens [[buffer(6)]],
    constant uint& tokens [[buffer(7)]],
    uint head [[threadgroup_position_in_grid]],
    uint row [[thread_position_in_threadgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;

    if (head >= heads || row >= head_dim) {
        return;
    }

    threadgroup float q_s[head_dim];
    threadgroup float k_s[head_dim];
    thread float row_state[head_dim];

    const uint vec_head_base = head * head_dim;
    const uint state_base = head * head_dim * head_dim;
    const uint row_state_base = state_base + row * head_dim;

    for (uint col = 0; col < head_dim; ++col) {
        row_state[col] = state[row_state_base + col];
    }

    for (uint token = 0; token < tokens; ++token) {
        const uint vec_base = token * width + vec_head_base;
        const uint idx = vec_base + row;
        q_s[row] = float(q_tokens[idx]);
        k_s[row] = float(k_tokens[idx]);
        threadgroup_barrier(mem_flags::mem_threadgroup);

        const float beta = beta_tokens[token * heads + head];
        const float decay = decay_tokens[token * heads + head];

        float kv_mem = 0.0f;
        for (uint col = 0; col < head_dim; ++col) {
            kv_mem += row_state[col] * decay * k_s[col];
        }

        const float delta = (float(v_tokens[idx]) - kv_mem) * beta;

        float acc = 0.0f;
        for (uint col = 0; col < head_dim; ++col) {
            const float next_state = row_state[col] * decay + k_s[col] * delta;
            row_state[col] = next_state;
            acc += next_state * q_s[col];
        }

        out_tokens[idx] = acc;
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    for (uint col = 0; col < head_dim; ++col) {
        state[row_state_base + col] = row_state[col];
    }
}

kernel void qwen35_08b_prefill_deltanet_scan_rowcache_block64_f32_state_tok_h16d128(
    device const half* q_tokens [[buffer(0)]],
    device const half* k_tokens [[buffer(1)]],
    device const half* v_tokens [[buffer(2)]],
    device const float* beta_tokens [[buffer(3)]],
    device const float* decay_tokens [[buffer(4)]],
    device float* state [[buffer(5)]],
    device float* out_tokens [[buffer(6)]],
    constant uint& tokens [[buffer(7)]],
    uint2 tg_pos [[threadgroup_position_in_grid]],
    uint2 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;
    constexpr uint rows_per_tg = 64;

    const uint lane = tid_pos.x;
    const uint row_block = tg_pos.x;
    const uint head = tg_pos.y;
    const uint row = row_block * rows_per_tg + lane;
    if (head >= heads || lane >= rows_per_tg || row >= head_dim) {
        return;
    }

    threadgroup float q_s[head_dim];
    threadgroup float k_s[head_dim];
    thread float row_state[head_dim];

    const uint vec_head_base = head * head_dim;
    const uint state_base = head * head_dim * head_dim;
    const uint row_state_base = state_base + row * head_dim;

    for (uint col = 0; col < head_dim; ++col) {
        row_state[col] = state[row_state_base + col];
    }

    for (uint token = 0; token < tokens; ++token) {
        const uint vec_base = token * width + vec_head_base;
        const uint idx = vec_base + row;
        q_s[lane] = float(q_tokens[vec_base + lane]);
        k_s[lane] = float(k_tokens[vec_base + lane]);
        q_s[lane + rows_per_tg] = float(q_tokens[vec_base + lane + rows_per_tg]);
        k_s[lane + rows_per_tg] = float(k_tokens[vec_base + lane + rows_per_tg]);
        threadgroup_barrier(mem_flags::mem_threadgroup);

        const float beta = beta_tokens[token * heads + head];
        const float decay = decay_tokens[token * heads + head];

        float kv_mem = 0.0f;
        for (uint col = 0; col < head_dim; ++col) {
            kv_mem += row_state[col] * decay * k_s[col];
        }

        const float delta = (float(v_tokens[idx]) - kv_mem) * beta;

        float acc = 0.0f;
        for (uint col = 0; col < head_dim; ++col) {
            const float next_state = row_state[col] * decay + k_s[col] * delta;
            row_state[col] = next_state;
            acc += next_state * q_s[col];
        }

        out_tokens[idx] = acc;
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    for (uint col = 0; col < head_dim; ++col) {
        state[row_state_base + col] = row_state[col];
    }
}

kernel void qwen35_08b_prefill_deltanet_scan_rowcache_block32_f32_state_tok_h16d128(
    device const half* q_tokens [[buffer(0)]],
    device const half* k_tokens [[buffer(1)]],
    device const half* v_tokens [[buffer(2)]],
    device const float* beta_tokens [[buffer(3)]],
    device const float* decay_tokens [[buffer(4)]],
    device float* state [[buffer(5)]],
    device float* out_tokens [[buffer(6)]],
    constant uint& tokens [[buffer(7)]],
    uint2 tg_pos [[threadgroup_position_in_grid]],
    uint2 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;
    constexpr uint rows_per_tg = 32;

    const uint lane = tid_pos.x;
    const uint row_block = tg_pos.x;
    const uint head = tg_pos.y;
    const uint row = row_block * rows_per_tg + lane;
    if (head >= heads || lane >= rows_per_tg || row >= head_dim) {
        return;
    }

    threadgroup float q_s[head_dim];
    threadgroup float k_s[head_dim];
    thread float row_state[head_dim];

    const uint vec_head_base = head * head_dim;
    const uint state_base = head * head_dim * head_dim;
    const uint row_state_base = state_base + row * head_dim;

    for (uint col = 0; col < head_dim; ++col) {
        row_state[col] = state[row_state_base + col];
    }

    for (uint token = 0; token < tokens; ++token) {
        const uint vec_base = token * width + vec_head_base;
        const uint idx = vec_base + row;
        q_s[lane] = float(q_tokens[vec_base + lane]);
        k_s[lane] = float(k_tokens[vec_base + lane]);
        q_s[lane + rows_per_tg] = float(q_tokens[vec_base + lane + rows_per_tg]);
        k_s[lane + rows_per_tg] = float(k_tokens[vec_base + lane + rows_per_tg]);
        q_s[lane + rows_per_tg * 2] = float(q_tokens[vec_base + lane + rows_per_tg * 2]);
        k_s[lane + rows_per_tg * 2] = float(k_tokens[vec_base + lane + rows_per_tg * 2]);
        q_s[lane + rows_per_tg * 3] = float(q_tokens[vec_base + lane + rows_per_tg * 3]);
        k_s[lane + rows_per_tg * 3] = float(k_tokens[vec_base + lane + rows_per_tg * 3]);
        threadgroup_barrier(mem_flags::mem_threadgroup);

        const float beta = beta_tokens[token * heads + head];
        const float decay = decay_tokens[token * heads + head];

        float kv_mem = 0.0f;
        for (uint col = 0; col < head_dim; ++col) {
            kv_mem += row_state[col] * decay * k_s[col];
        }

        const float delta = (float(v_tokens[idx]) - kv_mem) * beta;

        float acc = 0.0f;
        for (uint col = 0; col < head_dim; ++col) {
            const float next_state = row_state[col] * decay + k_s[col] * delta;
            row_state[col] = next_state;
            acc += next_state * q_s[col];
        }

        out_tokens[idx] = acc;
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    for (uint col = 0; col < head_dim; ++col) {
        state[row_state_base + col] = row_state[col];
    }
}

kernel void qwen35_08b_prefill_deltanet_scan_rowcache_direct_f32_state_tok_h16d128(
    device const half* q_tokens [[buffer(0)]],
    device const half* k_tokens [[buffer(1)]],
    device const half* v_tokens [[buffer(2)]],
    device const float* beta_tokens [[buffer(3)]],
    device const float* decay_tokens [[buffer(4)]],
    device float* state [[buffer(5)]],
    device float* out_tokens [[buffer(6)]],
    constant uint& tokens [[buffer(7)]],
    uint head [[threadgroup_position_in_grid]],
    uint row [[thread_position_in_threadgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;

    if (head >= heads || row >= head_dim) {
        return;
    }

    thread float row_state[head_dim];

    const uint vec_head_base = head * head_dim;
    const uint state_base = head * head_dim * head_dim;
    const uint row_state_base = state_base + row * head_dim;

    for (uint col = 0; col < head_dim; ++col) {
        row_state[col] = state[row_state_base + col];
    }

    for (uint token = 0; token < tokens; ++token) {
        const uint vec_base = token * width + vec_head_base;
        const uint idx = vec_base + row;
        const float beta = beta_tokens[token * heads + head];
        const float decay = decay_tokens[token * heads + head];

        float kv_mem = 0.0f;
        for (uint col = 0; col < head_dim; ++col) {
            kv_mem += row_state[col] * decay * float(k_tokens[vec_base + col]);
        }

        const float delta = (float(v_tokens[idx]) - kv_mem) * beta;

        float acc = 0.0f;
        for (uint col = 0; col < head_dim; ++col) {
            const float next_state = row_state[col] * decay + float(k_tokens[vec_base + col]) * delta;
            row_state[col] = next_state;
            acc += next_state * float(q_tokens[vec_base + col]);
        }

        out_tokens[idx] = acc;
    }

    for (uint col = 0; col < head_dim; ++col) {
        state[row_state_base + col] = row_state[col];
    }
}

kernel void qwen35_08b_prefill_deltanet_scan_lanes4_f32_state_tok_h16d128(
    device const half* q_tokens [[buffer(0)]],
    device const half* k_tokens [[buffer(1)]],
    device const half* v_tokens [[buffer(2)]],
    device const float* beta_tokens [[buffer(3)]],
    device const float* decay_tokens [[buffer(4)]],
    device float* state [[buffer(5)]],
    device float* out_tokens [[buffer(6)]],
    constant uint& tokens [[buffer(7)]],
    uint2 tg_pos [[threadgroup_position_in_grid]],
    uint2 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;
    constexpr uint lanes_per_row = 32;
    constexpr uint cols_per_lane = 4;

    const uint tx = tid_pos.x;
    const uint ty = tid_pos.y;
    const uint row = tg_pos.x * cols_per_lane + ty;
    const uint head = tg_pos.y;
    if (head >= heads || row >= head_dim || tx >= lanes_per_row || ty >= cols_per_lane) {
        return;
    }

    const uint vec_head_base = head * head_dim;
    const uint row_state_base = head * head_dim * head_dim + row * head_dim;
    float row_state[cols_per_lane];

    for (uint j = 0; j < cols_per_lane; ++j) {
        row_state[j] = state[row_state_base + tx * cols_per_lane + j];
    }

    for (uint token = 0; token < tokens; ++token) {
        const uint vec_base = token * width + vec_head_base;
        const float beta = beta_tokens[token * heads + head];
        const float decay = decay_tokens[token * heads + head];

        float kv_mem = 0.0f;
        for (uint j = 0; j < cols_per_lane; ++j) {
            const uint col = tx * cols_per_lane + j;
            row_state[j] *= decay;
            kv_mem += row_state[j] * float(k_tokens[vec_base + col]);
        }
        kv_mem = simd_sum(kv_mem);

        const float delta = (float(v_tokens[vec_base + row]) - kv_mem) * beta;

        float acc = 0.0f;
        for (uint j = 0; j < cols_per_lane; ++j) {
            const uint col = tx * cols_per_lane + j;
            row_state[j] += float(k_tokens[vec_base + col]) * delta;
            acc += row_state[j] * float(q_tokens[vec_base + col]);
        }
        acc = simd_sum(acc);

        if (tx == 0) {
            out_tokens[vec_base + row] = acc;
        }
    }

    for (uint j = 0; j < cols_per_lane; ++j) {
        state[row_state_base + tx * cols_per_lane + j] = row_state[j];
    }
}

kernel void qwen35_08b_prefill_deltanet_scan_lanes4_sharedqk_f32_state_tok_h16d128(
    device const half* q_tokens [[buffer(0)]],
    device const half* k_tokens [[buffer(1)]],
    device const half* v_tokens [[buffer(2)]],
    device const float* beta_tokens [[buffer(3)]],
    device const float* decay_tokens [[buffer(4)]],
    device float* state [[buffer(5)]],
    device float* out_tokens [[buffer(6)]],
    constant uint& tokens [[buffer(7)]],
    uint2 tg_pos [[threadgroup_position_in_grid]],
    uint2 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;
    constexpr uint lanes_per_row = 32;
    constexpr uint rows_per_tg = 4;
    constexpr uint cols_per_lane = 4;

    const uint tx = tid_pos.x;
    const uint ty = tid_pos.y;
    const uint row = tg_pos.x * rows_per_tg + ty;
    const uint head = tg_pos.y;
    if (head >= heads || row >= head_dim || tx >= lanes_per_row || ty >= rows_per_tg) {
        return;
    }

    threadgroup float q_s[head_dim];
    threadgroup float k_s[head_dim];

    const uint vec_head_base = head * head_dim;
    const uint row_state_base = head * head_dim * head_dim + row * head_dim;
    thread float row_state[cols_per_lane];

    for (uint j = 0; j < cols_per_lane; ++j) {
        row_state[j] = state[row_state_base + tx * cols_per_lane + j];
    }

    for (uint token = 0; token < tokens; ++token) {
        const uint vec_base = token * width + vec_head_base;
        const float beta = beta_tokens[token * heads + head];
        const float decay = decay_tokens[token * heads + head];

        if (ty == 0) {
            for (uint j = 0; j < cols_per_lane; ++j) {
                const uint col = tx * cols_per_lane + j;
                q_s[col] = float(q_tokens[vec_base + col]);
                k_s[col] = float(k_tokens[vec_base + col]);
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        float kv_mem = 0.0f;
        for (uint j = 0; j < cols_per_lane; ++j) {
            const uint col = tx * cols_per_lane + j;
            row_state[j] *= decay;
            kv_mem += row_state[j] * k_s[col];
        }
        kv_mem = simd_sum(kv_mem);

        const float delta = (float(v_tokens[vec_base + row]) - kv_mem) * beta;

        float acc = 0.0f;
        for (uint j = 0; j < cols_per_lane; ++j) {
            const uint col = tx * cols_per_lane + j;
            row_state[j] += k_s[col] * delta;
            acc += row_state[j] * q_s[col];
        }
        acc = simd_sum(acc);

        if (tx == 0) {
            out_tokens[vec_base + row] = acc;
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    for (uint j = 0; j < cols_per_lane; ++j) {
        state[row_state_base + tx * cols_per_lane + j] = row_state[j];
    }
}

kernel void qwen35_08b_prefill_deltanet_scan_lanes4_ordered_f32_state_tok_h16d128(
    device const half* q_tokens [[buffer(0)]],
    device const half* k_tokens [[buffer(1)]],
    device const half* v_tokens [[buffer(2)]],
    device const float* beta_tokens [[buffer(3)]],
    device const float* decay_tokens [[buffer(4)]],
    device float* state [[buffer(5)]],
    device float* out_tokens [[buffer(6)]],
    constant uint& tokens [[buffer(7)]],
    uint2 tg_pos [[threadgroup_position_in_grid]],
    uint2 tid_pos [[thread_position_in_threadgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;
    constexpr uint lanes_per_row = 32;
    constexpr uint rows_per_tg = 4;
    constexpr uint cols_per_lane = 4;

    const uint tx = tid_pos.x;
    const uint ty = tid_pos.y;
    const uint row = tg_pos.x * rows_per_tg + ty;
    const uint head = tg_pos.y;
    if (head >= heads || row >= head_dim || tx >= lanes_per_row || ty >= rows_per_tg) {
        return;
    }

    threadgroup float kv_part[rows_per_tg * head_dim];
    threadgroup float acc_part[rows_per_tg * head_dim];
    threadgroup float row_delta[rows_per_tg];

    const uint vec_head_base = head * head_dim;
    const uint row_state_base = head * head_dim * head_dim + row * head_dim;
    thread float row_state[cols_per_lane];

    for (uint j = 0; j < cols_per_lane; ++j) {
        row_state[j] = state[row_state_base + tx * cols_per_lane + j];
    }

    for (uint token = 0; token < tokens; ++token) {
        const uint vec_base = token * width + vec_head_base;
        const float beta = beta_tokens[token * heads + head];
        const float decay = decay_tokens[token * heads + head];
        const uint scratch_base = ty * head_dim;

        for (uint j = 0; j < cols_per_lane; ++j) {
            const uint col = tx * cols_per_lane + j;
            const float decayed_state = row_state[j] * decay;
            row_state[j] = decayed_state;
            kv_part[scratch_base + col] = decayed_state * float(k_tokens[vec_base + col]);
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        if (tx == 0) {
            float kv_mem = 0.0f;
            for (uint col = 0; col < head_dim; ++col) {
                kv_mem += kv_part[scratch_base + col];
            }
            row_delta[ty] = (float(v_tokens[vec_base + row]) - kv_mem) * beta;
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        const float delta = row_delta[ty];
        for (uint j = 0; j < cols_per_lane; ++j) {
            const uint col = tx * cols_per_lane + j;
            const float next_state = row_state[j] + float(k_tokens[vec_base + col]) * delta;
            row_state[j] = next_state;
            acc_part[scratch_base + col] = next_state * float(q_tokens[vec_base + col]);
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        if (tx == 0) {
            float acc = 0.0f;
            for (uint col = 0; col < head_dim; ++col) {
                acc += acc_part[scratch_base + col];
            }
            out_tokens[vec_base + row] = acc;
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    for (uint j = 0; j < cols_per_lane; ++j) {
        state[row_state_base + tx * cols_per_lane + j] = row_state[j];
    }
}

kernel void qwen35_08b_prefill_deltanet_scan_rowcache_gated_norm_f32_state_tok_h16d128(
    device const half* q_tokens [[buffer(0)]],
    device const half* k_tokens [[buffer(1)]],
    device const half* v_tokens [[buffer(2)]],
    device const float* beta_tokens [[buffer(3)]],
    device const float* decay_tokens [[buffer(4)]],
    device float* state [[buffer(5)]],
    device const float* z_tokens [[buffer(6)]],
    device const float* norm_weight [[buffer(7)]],
    device half* gated_tokens [[buffer(8)]],
    constant uint& tokens [[buffer(9)]],
    uint head [[threadgroup_position_in_grid]],
    uint row [[thread_position_in_threadgroup]]
) {
    constexpr uint heads = 16;
    constexpr uint head_dim = 128;
    constexpr uint width = heads * head_dim;
    constexpr float eps = 1.0e-6f;

    if (head >= heads || row >= head_dim) {
        return;
    }

    threadgroup float q_s[head_dim];
    threadgroup float k_s[head_dim];
    threadgroup float partial[head_dim];
    thread float row_state[head_dim];

    const uint vec_head_base = head * head_dim;
    const uint state_base = head * head_dim * head_dim;
    const uint row_state_base = state_base + row * head_dim;

    for (uint col = 0; col < head_dim; ++col) {
        row_state[col] = state[row_state_base + col];
    }

    for (uint token = 0; token < tokens; ++token) {
        const uint vec_base = token * width + vec_head_base;
        const uint idx = vec_base + row;
        q_s[row] = float(q_tokens[idx]);
        k_s[row] = float(k_tokens[idx]);
        threadgroup_barrier(mem_flags::mem_threadgroup);

        const float beta = beta_tokens[token * heads + head];
        const float decay = decay_tokens[token * heads + head];

        float kv_mem = 0.0f;
        for (uint col = 0; col < head_dim; ++col) {
            kv_mem += row_state[col] * decay * k_s[col];
        }

        const float delta = (float(v_tokens[idx]) - kv_mem) * beta;

        float acc = 0.0f;
        for (uint col = 0; col < head_dim; ++col) {
            const float next_state = row_state[col] * decay + k_s[col] * delta;
            row_state[col] = next_state;
            acc += next_state * q_s[col];
        }

        partial[row] = acc * acc;
        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint stride = 64; stride > 0; stride >>= 1) {
            if (row < stride) {
                partial[row] += partial[row + stride];
            }
            threadgroup_barrier(mem_flags::mem_threadgroup);
        }

        const float inv_rms = rsqrt(partial[0] / float(head_dim) + eps);
        const float gate = qwen35_08b_prefill_deltanet_scan_silu(z_tokens[idx]);
        const float out = acc * inv_rms * norm_weight[row] * gate;
        gated_tokens[idx] = half(clamp(out, -65504.0f, 65504.0f));
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    for (uint col = 0; col < head_dim; ++col) {
        state[row_state_base + col] = row_state[col];
    }
}
