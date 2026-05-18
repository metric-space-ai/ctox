# Qwen3-Embedding-0.6B Native Port

Bare-metal CTOX model port. No Ollama, no llama.cpp process, no Python, no
retired `ctox-engine` subprocess.

## Scope

- CPU reference: pooling and normalization are implemented; transformer forward
  is the next slice.
- Metal staging: selected seed kernels are vendored in
  `vendor/metal/kernels/`.
- CUDA staging: selected seed kernels are vendored in `vendor/cuda/kernels/`.

## Required Forward Path

1. Tokenize input with the CTOX tokenizer path.
2. Run Qwen3 transformer layers to final hidden state.
3. Pool hidden state with the Qwen3-Embedding policy.
4. L2-normalize the vector.
5. Return `Vec<Vec<f32>>` through the native CTOX embedding API.

