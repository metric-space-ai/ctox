extern "C" __global__ void silu_f32(float *x, unsigned n) {
    unsigned i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) { float v = x[i]; x[i] = v / (1.0f + expf(-v)); }
}

extern "C" __global__ void gelu_f32(float *x, unsigned n) {
    unsigned i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) {
        float v = x[i];
        float inner = 0.7978845608028654f * (v + 0.044715f * v * v * v);
        x[i] = 0.5f * v * (1.0f + tanhf(inner));
    }
}

extern "C" __global__ void add_inplace_f32(float *a, const float *b, unsigned n) {
    unsigned i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) a[i] += b[i];
}

extern "C" __global__ void rope_interleaved_f32(float *data, unsigned n_heads, unsigned head_dim, unsigned position, float theta) {
    unsigned gid = blockIdx.x * blockDim.x + threadIdx.x;
    unsigned half = head_dim / 2;
    unsigned total = n_heads * half;
    if (gid >= total) return;
    unsigned head = gid / half;
    unsigned i = gid % half;
    float angle = float(position) * powf(theta, -float(i) / float(half));
    float s, c; sincosf(angle, &s, &c);
    unsigned base = head * head_dim + 2 * i;
    float a = data[base], b = data[base + 1];
    data[base] = a * c - b * s;
    data[base + 1] = a * s + b * c;
}
