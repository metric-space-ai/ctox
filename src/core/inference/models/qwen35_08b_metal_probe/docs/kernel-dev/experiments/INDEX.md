# Kernel Experiment Index

Generated from `tools/list_kernel_experiments.sh --markdown`.

Do not edit the table manually. Regenerate with:

```text
tools/update_kernel_experiment_index.sh
```

| record | date | env_flag | default | strict | decision |
| --- | --- | --- | --- | --- | --- |
| docs/kernel-dev/experiments/20260430T170513Z-prefill-gap-forensics.md | 20260430T170513Z | n/a | pass | fail | n/a |
| docs/kernel-dev/experiments/20260501T081544Z-mps-tiled-attention.md | 20260501T081544Z | CTOX_QWEN35_ATTENTION_MPS_TILED=1 | pass | pass | n/a |
| docs/kernel-dev/experiments/20260501T083010Z-delta-scan-lanes4-sharedqk.md | 2026-05-01 10:30 CEST | CTOX_QWEN35_DELTA_SCAN_LANES4_SHAREDQK=1 | pass | pass | opt-in approximate |
| docs/kernel-dev/experiments/20260501T084028Z-delta-gated-norm-simd32x4.md | 2026-05-01 10:40 CEST | CTOX_QWEN35_DELTA_GATED_NORM_SIMD32X4=1 | pass | pass | rejected |
| docs/kernel-dev/experiments/20260501T085442Z-delta-scan-rowcache-block-auto.md | 2026-05-01 10:54 CEST | CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK_AUTO=1 | pass | pass | rejected / keep opt-in only |
| docs/kernel-dev/experiments/20260501T090838Z-delta-scan-isolated-sweep.md | 20260501T090838Z | CTOX_QWEN35_DELTA_SCAN_ROWCACHE* / CTOX_QWEN35_DELTA_SCAN_LANES4_SHAREDQK | pass | pass | n/a |
