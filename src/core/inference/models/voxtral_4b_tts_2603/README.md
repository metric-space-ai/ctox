# ctox-voxtral-4b-tts-2603

Native text-to-WAV graph for the CTOX alias `engineai/Voxtral-4B-TTS-2603`.
Weights come from `mistralai/Voxtral-4B-TTS-2603`, pinned revision
`b81be46c3777f88621676791b512bb01dc1cb970`.

The complete LLM, flow-matching acoustic transformer and 24 kHz codec decoder
are vendored from mudler/voxtral-tts.c at
`be031f4cf04ef75a01377eedcccf33c1ffd41580` (MIT). The original license is
retained in `vendor/voxtral-tts.c/LICENSE`; CTOX host integration remains AGPL.
No inference framework or Python runtime is used.

Default builds compile the portable CPU graph. The explicit `cuda` Cargo
feature compiles upstream CUDA kernels and cuBLAS dispatch. CUDA builds fail
when nvcc is unavailable; a CUDA runtime request fails when device initialization
or weight upload fails. The admitted typed compute plan chooses visible devices.
Build-only NVCC/CTOX_CUDA_SM/CTOX_CUDA_HOME settings locate the compiler/toolkit;
no new production runtime environment toggle is introduced.

On Linux the explicit `openblas` build feature enables the vendored upstream
CPU BLAS path, including the codec's dense projections. It requires OpenBLAS
headers/library registered with pkg-config and keeps the native BLAS pool at
two threads. CUDA and OpenBLAS may be combined; the root binary selects the
same feature through `local-speech-openblas`. No kernel or numerical equation
is changed. The portable fallback and macOS Accelerate path remain available.
The final Linux runtime must supply that OpenBLAS shared library; successful
compilation alone does not prove a packaged runtime or improved speech latency.

Model directories need `consolidated.safetensors`, `tekken.json`, and requested
`voice_embedding/<preset>.pt` BF16 embeddings. Original uncompressed PyTorch ZIP
files are read natively as data, without executing pickle. Preset voice names
are restricted to the upstream list, including `de_female` and `de_male`.

`VoxtralTtsModel::from_model_dir` actually loads the graph; errors are retained
by the local service and health becomes ready only after successful load.
Clones share one warm Arc session. A mutex serializes upstream process-global
CUDA/tokenizer state. One different model directory cannot load simultaneously.
The managed GPU cold-start budget is300s: measured A4500 graph preparation
was115s in one installed run and exceeded the generic120s ceiling in another
two-CPU lane run while uploading acoustic weights. This bounded preparation
budget does not extend a speech request or prove real-time performance. The
existing CPU auxiliary startup budget remains600s.
Text turns are bounded to 4096 UTF-8 bytes, the KV cache to 2048 positions, and
audio generation to 512 frames (40.96 s). Overlong prompts fail before prefill.

This implementation returns completed PCM16 WAV. Genuine first-audio streaming
is unavailable: the upstream codec decodes after all generated codes. Upstream
reports substantial CPU prefill/codec overhead even with CUDA decode. CTOX
populates the prompt KV cache through that same vendored causal CUDA decoder
when CUDA is selected; the original CPU batch path remains the fallback and
numerical reference. `verification/prefill_parity.c` compares each layer's K/V
and a continuation hidden state against that reference using actual weights.
No new CUDA kernel is introduced. The codec still runs on the CPU. Compiled
graph and successful load do not establish voice quality, realtime latency,
installed readiness or the Jour fixe 1.5 s target; those require real measured
artifacts on the admitted GPU and installed stack. No synthetic audio is used.

Validation: crate tests and CUDA/real-weight acceptance receipts are recorded
by the owning Models task. Do not infer real-model acceptance from unit tests.
