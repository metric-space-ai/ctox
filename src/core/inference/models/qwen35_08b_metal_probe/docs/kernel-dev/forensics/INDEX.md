# Cache Forensics Index

Generated from `tools/list_cache_forensics.sh --markdown`.

Do not edit the table manually. Regenerate with:

```text
tools/update_cache_forensics_index.sh
```

| record | date | experiment | op | evidence | default | strict | decision |
| --- | --- | --- | --- | --- | --- | --- | --- |
| docs/kernel-dev/forensics/20260501T081544Z-mps-tiled-attention.md | 20260501T081544Z | docs/kernel-dev/experiments/20260501T081544Z-mps-tiled-attention.md | prefill full-attention core | inferred-only | pass | pass | accepted |
| docs/kernel-dev/forensics/20260501T091106Z-isolated-delta-scan-byte-model.md | 20260501T091106Z | docs/kernel-dev/experiments/20260501T090838Z-delta-scan-isolated-sweep.md | deltanet_scan | inferred-only | pass | pass | needs-more-data |
