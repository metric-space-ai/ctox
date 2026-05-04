# Measurement: prefill-gap-forensics

Generated: 20260430T170525Z

# Measurement Record Template

Use this to link a captured benchmark run to an experiment. The raw files stay
in the capture directory; this record stores paths and normalized summary fields.

## Metadata

```text
date: 20260430T170525Z
experiment: docs/kernel-dev/experiments/20260430T170513Z-prefill-gap-forensics.md
label: prefill-gap-forensics
capture_dir: /tmp/ctox_qwen35_prefill_gap_capture/20260430T170351Z-prefill-gap-forensics
manifest: /tmp/ctox_qwen35_prefill_gap_capture/20260430T170351Z-prefill-gap-forensics/manifest.txt
stdout: /tmp/ctox_qwen35_prefill_gap_capture/20260430T170351Z-prefill-gap-forensics/stdout.txt
stderr: /tmp/ctox_qwen35_prefill_gap_capture/20260430T170351Z-prefill-gap-forensics/stderr.txt
normalized: /tmp/ctox_qwen35_prefill_gap_capture/20260430T170351Z-prefill-gap-forensics/normalized.txt
exit_code_file: /tmp/ctox_qwen35_prefill_gap_capture/20260430T170351Z-prefill-gap-forensics/exit_code.txt
exit_code: 0
```

## Manifest Summary

```text
command: target/release/memory_forensics /tmp/ctox_qwen35_08b_real_fp16.metalpack 4096 1 90
accepted_profile_hash: fea814a42ac1bfebce567a5c4a0ac090524c4def8fb97fa7670f28abbc91de3c
git_commit: 5081442bf220c6af06efa52ed4209c53ed6c6420
git_dirty_state: clean
```

## Normalized Fields

```text
tokens/context: 4096
iterations: 1
warmup:
median_s: 3.614
p95_s: 3.614
effective_GB/s:
checksum:
output_csv:
```

## Classification

```text
measurement_kind: forensics
usable_for_decision: no
notes: raw capture imported; strict decision usability requires explicit review
```
