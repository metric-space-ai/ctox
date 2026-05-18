# Voxtral TTS Kernels

Seed source: `/Users/michaelwelsch/Downloads/voxtral-rs-port-seed/kernels`.

- Metal: `vendor/metal/kernels/ctox_voxtral_tts_glue.metal`
- CUDA: `vendor/cuda/kernels/ctox_voxtral_tts_glue.cu`
- WGSL: `vendor/wgsl/kernels/ctox_voxtral_tts_glue.wgsl`

The kernel sources are vendored as the model-local starting point. They are not
shared with other inference crates.
