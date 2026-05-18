@group(0) @binding(0) var<storage, read_write> x: array<f32>;
@group(0) @binding(1) var<uniform> n: u32;

@compute @workgroup_size(256)
fn silu_f32(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i < n) {
        let v = x[i];
        x[i] = v / (1.0 + exp(-v));
    }
}

@compute @workgroup_size(256)
fn gelu_f32(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i < n) {
        let v = x[i];
        let inner = 0.7978845608028654 * (v + 0.044715 * v * v * v);
        x[i] = 0.5 * v * (1.0 + tanh(inner));
    }
}
