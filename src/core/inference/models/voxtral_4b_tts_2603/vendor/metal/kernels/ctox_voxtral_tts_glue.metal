#include <metal_stdlib>
using namespace metal;

kernel void rms_norm_f32(
    device const float *x [[buffer(0)]],
    device const float *weight [[buffer(1)]],
    device float *out [[buffer(2)]],
    constant uint &hidden [[buffer(3)]],
    constant float &eps [[buffer(4)]],
    uint row [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]],
    uint threads [[threads_per_threadgroup]]) {
    threadgroup float sum[256];
    float local = 0.0f;
    device const float *xr = x + row * hidden;
    for (uint i = tid; i < hidden; i += threads) local += xr[i] * xr[i];
    sum[tid] = local;
    threadgroup_barrier(mem_flags::mem_threadgroup);
    for (uint s = threads >> 1; s > 0; s >>= 1) {
        if (tid < s) sum[tid] += sum[tid + s];
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    float inv = rsqrt(sum[0] / float(hidden) + eps);
    device float *yr = out + row * hidden;
    for (uint i = tid; i < hidden; i += threads) yr[i] = xr[i] * inv * weight[i];
}

kernel void silu_f32(device float *x [[buffer(0)]], constant uint &n [[buffer(1)]], uint gid [[thread_position_in_grid]]) {
    if (gid < n) { float v = x[gid]; x[gid] = v / (1.0f + exp(-v)); }
}

kernel void gelu_f32(device float *x [[buffer(0)]], constant uint &n [[buffer(1)]], uint gid [[thread_position_in_grid]]) {
    if (gid < n) {
        float v = x[gid];
        float inner = 0.7978845608028654f * (v + 0.044715f * v * v * v);
        x[gid] = 0.5f * v * (1.0f + tanh(inner));
    }
}

kernel void add_inplace_f32(device float *a [[buffer(0)]], device const float *b [[buffer(1)]], constant uint &n [[buffer(2)]], uint gid [[thread_position_in_grid]]) {
    if (gid < n) a[gid] += b[gid];
}

kernel void rope_interleaved_f32(
    device float *data [[buffer(0)]],
    constant uint &n_heads [[buffer(1)]],
    constant uint &head_dim [[buffer(2)]],
    constant uint &position [[buffer(3)]],
    constant float &theta [[buffer(4)]],
    uint gid [[thread_position_in_grid]]) {
    uint half = head_dim / 2;
    uint total = n_heads * half;
    if (gid >= total) return;
    uint head = gid / half;
    uint i = gid % half;
    float inv_freq = pow(theta, -float(i) / float(half));
    float angle = float(position) * inv_freq;
    float s = sin(angle), c = cos(angle);
    uint base = head * head_dim + 2 * i;
    float a = data[base], b = data[base + 1];
    data[base] = a * c - b * s;
    data[base + 1] = a * s + b * c;
}

kernel void decoder_attention_f32(
    device const float *Q [[buffer(0)]],
    device const float *K [[buffer(1)]],
    device const float *V [[buffer(2)]],
    device float *out [[buffer(3)]],
    constant uint &n_heads [[buffer(4)]],
    constant uint &n_kv_heads [[buffer(5)]],
    constant uint &head_dim [[buffer(6)]],
    constant uint &kv_dim [[buffer(7)]],
    constant uint &seq_k [[buffer(8)]],
    constant float &scale [[buffer(9)]],
    constant uint &window_size [[buffer(10)]],
    constant uint &q_pos [[buffer(11)]],
    uint head_idx [[threadgroup_position_in_grid]],
    uint tid [[thread_position_in_threadgroup]]) {
    if (head_idx >= n_heads) return;
    uint kv_head = head_idx / (n_heads / n_kv_heads);
    uint d0 = tid, d1 = tid + 32, d2 = tid + 64, d3 = tid + 96;
    device const float *qh = Q + head_idx * head_dim;
    float q0 = qh[d0], q1 = qh[d1], q2 = qh[d2], q3 = qh[d3];
    uint end = min(q_pos, seq_k - 1);
    uint start = window_size > 0 && q_pos + 1 > window_size ? q_pos + 1 - window_size : 0;
    float m = -INFINITY, z = 0.0f, a0 = 0.0f, a1 = 0.0f, a2 = 0.0f, a3 = 0.0f;
    for (uint j = start; j <= end; j++) {
        device const float *kh = K + j * kv_dim + kv_head * head_dim;
        float partial = q0 * kh[d0] + q1 * kh[d1] + q2 * kh[d2] + q3 * kh[d3];
        float score = simd_sum(partial) * scale;
        float old = m; m = max(m, score);
        float corr = exp(old - m), w = exp(score - m);
        z = z * corr + w; a0 *= corr; a1 *= corr; a2 *= corr; a3 *= corr;
        device const float *vh = V + j * kv_dim + kv_head * head_dim;
        a0 += w * vh[d0]; a1 += w * vh[d1]; a2 += w * vh[d2]; a3 += w * vh[d3];
    }
    float inv = 1.0f / (z + 1e-10f);
    device float *oh = out + head_idx * head_dim;
    oh[d0] = a0 * inv; oh[d1] = a1 * inv; oh[d2] = a2 * inv; oh[d3] = a3 * inv;
}
