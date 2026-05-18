# ctox-voxtral-4b-tts-2603

Bare-metal CTOX native port scaffold for `engineai/Voxtral-4B-TTS-2603`.

This crate was seeded from `/Users/michaelwelsch/Downloads/voxtral-rs-port-seed`
and reshaped into CTOX's per-model layout:

- no inference framework dependency;
- Rust host/orchestration code in this crate;
- vendored Metal/CUDA/WGSL kernel sources under `vendor/`;
- line-delimited JSON service hosted by the CTOX binary through
  `__native-voxtral-tts-service`.

Current state: the loader, safetensors header inspection, audio helpers, CPU
reference kernels and kernel source layout are present. Full text-to-audio graph
execution is not wired yet, so production calls fail with `backend_not_wired`
instead of returning fake audio.
