# ctox-voxtral-mini-4b-realtime-2602

Bare-metal CTOX native STT runtime for
`engineai/Voxtral-Mini-4B-Realtime-2602`.

Current scope:

- no inference framework process dependency;
- Rust host/orchestration code in this crate;
- vendored ggml for CPU and Metal kernels;
- GGUF metadata, tensor-name inspection, and tokenizer metadata loading;
- Q4 GGUF encoder, adapter, decoder, KV cache, and greedy decode execution;
- WAV/audio preprocessing aligned to the Voxtral realtime graph;
- line-delimited JSON service hosted by the CTOX binary through
  `__native-voxtral-stt-service`.

Linux defaults to ggml's native CPU kernels (`GGML_NATIVE=ON`) so quantized
Q4 matmul stays on ggml's AVX/FMA/architecture-specific path. The crate's `cuda`
feature (CTOX binary: `--features local-speech-cuda`) builds the already vendored
ggml CUDA kernels and uses CUDA for both weights and graph execution. It requires
a Linux CUDA toolkit. GPU placement still comes from the supervisor's existing
GPU admission plan; device 0 is relative to that admitted visible-device set.
An unavailable requested backend fails explicitly instead of silently using CPU.
CMake builds respect Cargo's job count, capped at two compiler workers.
BLAS is available
only as an explicit build-time experiment via `CTOX_VOXTRAL_GGML_BLAS=1`.

Current state: the crate loads a ggml-compatible Q4 Voxtral GGUF and returns
real transcripts. It does not use TrevorJS, Burn, WGPU, or an external
inference process.

`engineai/Voxtral-Mini-4B-Realtime-2602` is CTOX's runtime alias. The upstream
model is `mistralai/Voxtral-Mini-4B-Realtime-2602`; GGUF conversion must retain
the `enc.*`, `dec.*`, `adapter.*` tensors and Voxtral tokenizer metadata expected
by this port. Backend compilation and artifact inspection alone do not prove
real-time performance. `open_stream` accepts mono PCM16 at 16 kHz, retains decoder
KV for one utterance, and exposes partial snapshots before `finish`. The causal
encoder currently recomputes the bounded prefix every 320 ms; this is not an
incremental encoder cache. Utterances are capped at 15 seconds and concurrent
streams are rejected while one owns the decoder. A pinned real-audio fixture on
an RTX A4500 produced matching batch/stream transcripts, but the initial release
candidate took3429ms from fixture end to final text. That exceeds the1500ms
meeting target and does not establish microphone/VAD or installed gateway latency.


## Direct streaming measurement

`cargo run --release --features cuda --example stream_benchmark -- cuda <model.gguf> <audio.wav>` compares whole-file and paced 20 ms streaming transcription on the same warm model. It records snapshots, capture backlog, finish compute time and batch/stream parity. This operator fixture does not certify the installed meeting, gateway transport or microphone/VAD sentence-end latency. Keep its JSON separate from installed acceptance.

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
