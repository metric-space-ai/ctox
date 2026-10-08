# Voxtral TTS native graph and kernels

Production candidate graph and kernels: `vendor/voxtral-tts.c`, upstream commit
`be031f4cf04ef75a01377eedcccf33c1ffd41580`. See `UPSTREAM.md` and retained MIT
`LICENSE`. CUDA kernels are upstream-authored; CTOX adds host boundary, admission,
bounds and fail-closed checks. No hand-authored production CUDA kernels.

Earlier `vendor/{metal,cuda,wgsl}/kernels/ctox_voxtral_tts_glue.*` files remain
unpromoted scaffold sources. Metal/WGSL requests fail rather than pretend those
sources implement the graph. macOS CPU builds use Accelerate BLAS.
