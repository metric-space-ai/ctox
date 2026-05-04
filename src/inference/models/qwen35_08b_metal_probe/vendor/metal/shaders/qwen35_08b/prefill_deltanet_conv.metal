#include <metal_stdlib>
using namespace metal;

static inline float qwen35_08b_prefill_silu(float x) {
    return x / (1.0f + exp(-clamp(x, -20.0f, 20.0f)));
}

kernel void qwen35_08b_prefill_deltanet_causal_conv1d_silu_c6144_k4(
    device const float* x_tokens [[buffer(0)]],
    device half* conv_state [[buffer(1)]],
    device const half* weight [[buffer(2)]],
    device const half* bias [[buffer(3)]],
    device float* out_tokens [[buffer(4)]],
    constant uint& tokens [[buffer(5)]],
    uint channel [[thread_position_in_grid]]
) {
    constexpr uint channels = 6144;
    constexpr uint kernel_width = 4;
    if (channel >= channels) {
        return;
    }

    float s0 = float(conv_state[channel]);
    float s1 = float(conv_state[channels + channel]);
    float s2 = float(conv_state[2 * channels + channel]);
    const uint w_base = channel * kernel_width;
    const float w0 = float(weight[w_base]);
    const float w1 = float(weight[w_base + 1]);
    const float w2 = float(weight[w_base + 2]);
    const float w3 = float(weight[w_base + 3]);
    const float b = float(bias[channel]);

    for (uint token = 0; token < tokens; ++token) {
        const float x_new = x_tokens[token * channels + channel];
        const float acc = b + s0 * w0 + s1 * w1 + s2 * w2 + x_new * w3;
        out_tokens[token * channels + channel] = qwen35_08b_prefill_silu(acc);
        s0 = s1;
        s1 = s2;
        s2 = x_new;
    }

    conv_state[channel] = half(clamp(s0, -65504.0f, 65504.0f));
    conv_state[channels + channel] = half(clamp(s1, -65504.0f, 65504.0f));
    conv_state[2 * channels + channel] = half(clamp(s2, -65504.0f, 65504.0f));
}
