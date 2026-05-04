fn main() {
    println!("cargo:rerun-if-changed=vendor/reference/voxtral.cpp.commit");
    println!("cargo:rerun-if-changed=vendor/cuda/kernels/ctox_voxtral_stt_glue.cu");
    println!("cargo:rerun-if-changed=vendor/metal/kernels/ctox_voxtral_stt_glue.metal");
    println!("cargo:rerun-if-changed=vendor/wgsl/kernels/ctox_voxtral_stt_glue.wgsl");
}
