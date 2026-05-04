# ctox-voxtral-mini-4b-realtime-2602

Bare-metal CTOX native STT runtime for
`engineai/Voxtral-Mini-4B-Realtime-2602`.

Current scope:

- no inference framework process dependency;
- Rust host/orchestration code in this crate;
- vendored ggml for CPU, BLAS, and Metal kernels;
- GGUF metadata, tensor-name inspection, and tokenizer metadata loading;
- Q4 GGUF encoder, adapter, decoder, KV cache, and greedy decode execution;
- WAV/audio preprocessing aligned to the Voxtral realtime graph;
- line-delimited JSON service hosted by the CTOX binary through
  `__native-voxtral-stt-service`.

Current state: the crate loads a ggml-compatible Q4 Voxtral GGUF and returns
real transcripts. It does not use TrevorJS, Burn, WGPU, or an external
inference process.

## Online Sample Harness

The ignored integration test `tests/online_samples.rs` downloads three small
LibriSpeech-derived WAV/TXT fixtures. LibriSpeech is published by OpenSLR under
CC BY 4.0; the fixtures are used only as reproducible test inputs and are
cached under this crate's `target/` directory.

Run manually with:

```bash
cargo test -p ctox-voxtral-mini-4b-realtime-2602 --test online_samples -- --ignored
```

The Q4 test requires `CTOX_VOXTRAL_STT_GGUF` to point at the local GGUF model.
It normalizes and compares the decoded text to the expected transcripts.
