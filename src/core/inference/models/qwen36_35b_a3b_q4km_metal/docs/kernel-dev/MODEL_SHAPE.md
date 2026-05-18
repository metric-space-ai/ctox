# MODEL_SHAPE — Frozen Kernel ABI for Qwen3.6-35B-A3B Q4_K_M (Metal)

**Source of truth.** This document is the canonical kernel-ABI freeze
for the Qwen3.6-35B-A3B text decoder as targeted by the Metal port.
The matching code constant is `QWEN36_35B_A3B_TEXT_CONFIG` in
[src/model.rs](../../src/model.rs); the matching upstream snapshot is
[vendor/upstream-config/Qwen3.6-35B-A3B.config.json](../../vendor/upstream-config/Qwen3.6-35B-A3B.config.json).

When upstream changes any of these numbers, refresh the snapshot, the
constant, and this document **in the same commit**.

## Top-level architecture

| field | value |
|---|---|
| `architectures[0]` | `Qwen3_5MoeForConditionalGeneration` |
| `model_type` | `qwen3_5_moe` |
| text dtype (master weights) | bfloat16 |
| target inference dtype | Q4_K_M weights × f16 activations |
| has vision tower | yes (deferred — text-only stage 1) |
| has MTP head | yes (1 layer; deferred — stage 1) |

## Text decoder topology

| field | value | notes |
|---|---|---|
| `num_hidden_layers` | 40 | |
| `full_attention_interval` | 4 | every 4th layer is full-softmax |
| full-attention layers | 10 | the **stage-1 target set** |
| linear-attention layers | 30 | dflash track — deferred |
| `hidden_size` | 2048 | residual / embed dim |
| `vocab_size` | 248_320 | not tied to LM head (`tie_word_embeddings = false`) |
| `max_position_embeddings` | 262_144 | 256 k context |
| `rms_norm_eps` | 1e-6 | full f32 accumulator required |
| `bos_token_id` | 248_044 | also `eos_token_id` |

Layer type sequence is the literal repeat
`[linear, linear, linear, full]` × 10 — see `LAYER_TYPES` in
`src/model.rs`. The Metal-side per-layer dispatch table will be
indexed off this constant.

## Full-attention block (the stage-1 target)

| field | value |
|---|---|
| `num_attention_heads` | 16 |
| `num_key_value_heads` | 2 |
| GQA group size (Q heads per KV head) | 8 |
| `head_dim` | 256 |
| Q hidden width | 4096 |
| K / V hidden width | 512 |
| `attn_output_gate` | **true** |
| `attention_bias` | false |

### M-RoPE

| field | value |
|---|---|
| `partial_rotary_factor` | 0.25 |
| rotated lanes per head | 64 (= 256 × 0.25) |
| `mrope_interleaved` | true |
| `mrope_section` | `[11, 11, 10]` (text / spatial-x / spatial-y or temporal) |
| `rope_theta` | 1.0e7 |

For text-only inference, all three axes degenerate to the same
position counter; the M-RoPE kernel must still split lanes correctly
so the same code path stays correct once vision lands.

## Linear-attention block (deferred — dflash)

Recorded for completeness so the loader can validate weight presence.

| field | value |
|---|---|
| `linear_num_key_heads` | 16 |
| `linear_num_value_heads` | 32 |
| `linear_key_head_dim` | 128 |
| `linear_value_head_dim` | 128 |
| `linear_conv_kernel_dim` | 4 (causal conv1d on Q/K/V) |
| `mamba_ssm_dtype` | float32 (state precision) |

## MoE FFN (the stage-1 secondary target — runs in every layer)

| field | value |
|---|---|
| `num_experts` | 256 |
| `num_experts_per_tok` | 8 |
| `moe_intermediate_size` | 512 (per-expert SwiGLU intermediate) |
| `shared_expert_intermediate_size` | 512 (always-on path next to top-k) |
| `hidden_act` | silu |
| `norm_topk_prob` | true (router weights softmax-normalised after top-k) |

## Tokens

| field | value |
|---|---|
| pad_token_id | null |
| bos / eos | 248_044 |
| image_token_id | 248_056 |
| video_token_id | 248_057 |
| vision_start / vision_end | 248_053 / 248_054 |

## What the Metal kernels are allowed to specialize on

Every kernel in `src/metal_port/` may treat the following as compile-
time constants and bake them into thread-group sizes, vector widths,
quant block layouts, and dispatch tables:

- `head_dim = 256`
- GQA group = 8 (Q-heads per KV-head)
- `moe_intermediate_size = 512`
- `shared_expert_intermediate_size = 512`
- `num_experts_per_tok = 8`
- M-RoPE rotated lanes = 64
- M-RoPE section split = `[11, 11, 10]`
- `attn_output_gate` is on (no fallback path for off needed)

Anything **not** on that list — context length, batch size, MoE expert
selection — must remain a runtime parameter.

## Memory back-of-envelope (M5 32 GiB)

Indicative, not measured. Stage 2 replaces these with measured numbers
once a Q4_K_M GGUF is on disk.

| component | bytes | source |
|---|---|---|
| Q4_K_M weights | ≈ 21 GiB | typical Q4_K_M ratio for 35B-class |
| KV cache, ctx=32_768, all 10 full-attn layers | ≈ 320 MiB | 10 × 2 × 256 × 32_768 × 2 (f16) × 2 (K+V) |
| MoE shared scratch, top-8 of 256, ctx=1 | ≈ 8 KiB | 8 × 512 × 2 (f16) |
| ViT vision tower | n/a | deferred |
| MTP head | n/a | deferred |

Net: stage-1 hypothesis is the M5 path is bandwidth-bound, not
capacity-bound. Stage 2 verifies with a measured stream-bandwidth
probe and a measured Q4_K_M decode roofline.
