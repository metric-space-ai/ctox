# Kernel Selection

The first native port slice stages only the kernels needed for
Qwen3-Embedding-0.6B. Generation-only kernels stay out of the integration path.

## Metal Seed

- Source: `../qwen35_27b_dflash/vendor/metal/shaders/dflash/ctox_qwen35_27b_glue.metal`
- Destination: `vendor/metal/kernels/ctox_qwen3_embedding_glue.metal`
- Keep: embedding gather, RMSNorm, quantized matmul, SDPA helpers, SiLU, L2 norm.
- Drop before link: DFlash tape, decode loop, argmax, draft/verification helpers.

## CUDA Seed

- Source: `../qwen35_27b_dflash/vendor/cuda/kernels/ctox_qwen35_27b_glue.cu`
- Destination: `vendor/cuda/kernels/ctox_qwen3_embedding_glue.cu`
- Keep: RMSNorm, dense matmul, Q4_K dequant/matvec, SDPA helpers, SiLU, L2 norm.
- Drop before link: decode loop, argmax, KV generation-only helpers, DFlash-specific helpers.

## Missing Native Glue

- GGUF/safetensors tensor-name mapping for Qwen3-Embedding.
- Tokenizer handoff from CTOX tokenizer layer.
- Backend forward pass up to final hidden state.
- Last-token/mean pooling kernel dispatch.
- L2-normalized `Vec<Vec<f32>>` return path.

