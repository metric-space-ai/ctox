# Measurement Record Template

Use this to link a captured benchmark run to an experiment. The raw files stay
in the capture directory; this record stores paths and normalized summary fields.

## Metadata

```text
date:
experiment:
label:
capture_dir:
manifest:
stdout:
stderr:
normalized:
exit_code_file:
exit_code:
```

## Manifest Summary

```text
command:
accepted_profile_hash:
git_commit:
git_dirty_state:
```

## Normalized Fields

```text
tokens/context:
iterations:
warmup:
median_s:
p95_s:
effective_GB/s:
checksum:
output_csv:
```

## Classification

```text
measurement_kind: <baseline | candidate | forensics | autotune | correctness | smoke>
usable_for_decision: <yes | no>
notes:
```
