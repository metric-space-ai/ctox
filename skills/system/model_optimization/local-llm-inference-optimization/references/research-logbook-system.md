# Research Logbook System

Use this reference when turning one model optimization into reusable knowledge
for future model/platform work.

## Why Logs Matter

Keep the original tuning logs. The handbook is the distilled truth, but the log
contains the failed hypotheses, measurement traps, partial wins, regressions,
operator-specific commands, and ordering effects that are usually lost during
summarization.

Use logs as a lookup surface when:

- a new candidate resembles something previously rejected
- a benchmark regression appears after a local kernel win
- a hardware/backend feature looks promising but integration cost is unknown
- a quantized path is faster in isolation but wrong or slower in the full path
- a user asks why a decision was made
- transferring an optimization from one model size to another

## Per-Model Log Bundle

Every optimized model should leave this bundle:

```text
<model>-research-log.md
<model>-kernel-dev-handbook.md
<model>-research-log-index.md
<model>-hardware-backend-grid.md
<model>-benchmark-protocol.md
<model>-cache-forensics-checklist.md
<model>-accepted-profile.env
<model>-experiment-template.md
<model>-decision-record-template.md
<model>-forensics-record-template.md
<model>-autotune-record-template.md
<model>-candidate-manifest-template.md
<model>-quant-pipeline-template.md
```

For very large logs, never load the full file blindly. Search or read through
the index first.

## Log Hygiene

Logs may be bundled into this skill only when they are safe as long-lived
references:

- no API tokens
- no private model-provider credentials
- no plaintext secrets
- no irrelevant user-private text
- local paths are acceptable only when they help reproduce artifact layout
- model hashes, command lines, env flags, and benchmark outputs should be kept

If a log contains credential material, store the secret with `ctox secret
intake`, rewrite the source log to a `[secret-ref:scope/name]` handle, and only
then copy the log into the skill.

## How To Query Logs

Start with targeted searches:

```text
rg -n "candidate|accepted|rejected|regression|llama.cpp" <model>-research-log.md
rg -n "MPS|Metal|SIMD|SME|ANE|Core ML|quant" <model>-research-log.md
rg -n "prefill|decode|attention|DeltaNet|FFN|LM-head" <model>-research-log.md
rg -n "cache|forensics|bandwidth|byte model|roofline" <model>-research-log.md
```

Then read the nearby section by line number. Do not treat an old fast result as
current truth unless the accepted profile or decision record confirms it.

## Promotion Into Handbook

Periodically distill the log into:

- accepted patterns
- rejected/risky patterns
- current reference status
- backend matrix
- cache/byte-model rules
- transfer rules for larger or adjacent models

The log remains the forensic record. The handbook becomes the concise operating
manual.
