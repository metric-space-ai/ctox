# Voxtral STT Kernels

Reference source: `andrijdavid/voxtral.cpp` commit
`7deef66c8ee473d3ceffc57fb0cd17977eeebca9`.

The first native STT slice reserves per-platform kernel slots:

- Metal: `vendor/metal/kernels/ctox_voxtral_stt_glue.metal`
- CUDA: `vendor/cuda/kernels/ctox_voxtral_stt_glue.cu`
- WGSL: `vendor/wgsl/kernels/ctox_voxtral_stt_glue.wgsl`

The hot ops to port from the reference graph are:

- causal Conv1D stem;
- RMSNorm;
- RoPE;
- Flash Attention / sliding-window attention;
- dense and quantized matmul;
- SiLU and GELU;
- get-rows;
- KV cache copy/windowing;
- argmax.

The CPU reference path stays in Rust for correctness tests. Platform kernels
must implement the same backend trait rather than sharing kernels across model
crates.
