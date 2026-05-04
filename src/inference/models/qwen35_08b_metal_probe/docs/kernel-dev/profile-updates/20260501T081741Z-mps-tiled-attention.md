# Accepted Profile Update: mps-tiled-attention

Generated: 20260501T081741Z

# Accepted Profile Update Template

Use this after `tools/check_kernel_promotion.sh <decision.md>` passes. This file
records why `docs/kernel-dev/accepted_profile.env` may change.

## Metadata

```text
date: 20260501T081741Z
decision_record: docs/kernel-dev/decisions/20260501T081544Z-mps-tiled-attention-accepted.md
experiment_record: docs/kernel-dev/experiments/20260501T081544Z-mps-tiled-attention.md
forensics_record: docs/kernel-dev/forensics/20260501T081544Z-mps-tiled-attention.md
autotune_record: n/a
accepted_profile_path: docs/kernel-dev/accepted_profile.env
accepted_profile_hash_before: 7e53ef2b3926542ce63c73e6f5e5f43b1e49926c6f71f7c0fc3478a29dbfaa9e
```

## Proposed Env

```text
accepted_env: CTOX_QWEN35_ATTENTION_MPS_TILED=1; remove CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8=1
```

## Evidence

```text
one_sentence: Promote exact MPS tiled prefill attention because all six attention layers pass p512/p4096 raw-dump gates and sidecar full-prefill forensics beats llama.cpp at 4096, 16384, and 32768 tokens.
promotion_check: passed
correctness_gate: p512 and p4096 raw attention dump comparison passes on layers 3,7,11,15,19,23 with max_abs_error <= 0.003906250 and mean_abs_error <= 0.000142806.
token_sweep_gate: p4096, p16384, and p32768 full-prefill forensics beats llama.cpp BF16/Metal references.
reference_comparison: exact MPS tiled forensics reaches 3889.99, 3329.39, and 2763.40 tok/s versus llama.cpp 2852.70, 2065.71, and 1325.20 tok/s.
cache_forensics: docs/kernel-dev/forensics/20260501T081544Z-mps-tiled-attention.md
autotune_evidence: n/a
```

## Apply Plan

```text
manual_apply_required: yes
profile_lines_to_add_or_change: CTOX_QWEN35_ATTENTION_MPS_TILED=1; remove CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8=1
rollback_plan: restore docs/kernel-dev/accepted_profile.env to hash 7e53ef2b3926542ce63c73e6f5e5f43b1e49926c6f71f7c0fc3478a29dbfaa9e
```
