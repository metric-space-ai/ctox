# ctox-voxtral-mini-4b-realtime-2602

Bare-metal CTOX native port scaffold for `engineai/Voxtral-Mini-4B-Realtime-2602`.

The initial port is seeded from `andrijdavid/voxtral.cpp` commit
`7deef66c8ee473d3ceffc57fb0cd17977eeebca9`, which is a compact MIT
licensed ggml C++ implementation of Voxtral Realtime 4B. CTOX uses that
project as an algorithmic reference, not as a long-term external runtime.

Current scope:

- no inference framework process dependency;
- Rust host/orchestration code in this crate;
- GGUF metadata and tensor-name inspection;
- WAV/audio preprocessing reference helpers;
- kernel source slots for Metal/CUDA/WGSL backends;
- line-delimited JSON service hosted by the CTOX binary through
  `__native-voxtral-stt-service`.

Current state: the crate verifies model artifact shape and exposes the CTOX
service contract. Full encoder/adapter/decoder graph execution is not wired
yet, so production transcription calls fail with `backend_not_wired` instead
of returning fake text.

## Online Sample Harness

The ignored integration test `tests/online_samples.rs` downloads three small
LibriSpeech-derived WAV/TXT fixtures from the pinned `voxtral.cpp` commit.
LibriSpeech is published by OpenSLR under CC BY 4.0; the fixtures are used only
as reproducible test inputs and are cached under this crate's `target/`
directory.

Run manually with:

```bash
cargo test -p ctox-voxtral-mini-4b-realtime-2602 --test online_samples -- --ignored
```

Until the graph is wired, the test validates the WAV/audio/mel path and accepts
only the explicit `not wired` error. Once transcription returns text, it
normalizes and compares the output to the downloaded expected transcripts.
