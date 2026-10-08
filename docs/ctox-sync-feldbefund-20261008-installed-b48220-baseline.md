# Installed b48220 baseline did not converge before fault injection

## Measured scope

The admitted gpu3 run `ctox-installed-sync-acceptance-20261008T033505Z`
used merged runner `1946f4b8fe99d9d8326d807de73cae45271d2795`, the published
`native-main-b48220db385d` package, and verified binary SHA256
`f07578594780e137a3c00dda58da5a6953909647f45df758eb6b3e0a1b512053`.
The native root was a fresh synthetic prefix, never WELSCH, THESEN or a default
tenant. Playwright1.60.0's matching Chromium148 headless executable started
successfully with two separate contexts. Browser DB/schema/sync modules were
served from that installed package; record transport was WebRTC. This was a
module acceptance runner, not a full Shell UI measurement. Installed Workjet54
was recorded as context and was not driven by this test.

## Actual finding

Goal5's first one-document baseline write did not reach exact-value convergence
on **both native and B** within the runner's60s window. The failed assertion was
saved at03:38:18.954462Z, before any offline/200-write round and before the owned
P1 GPU unit was manually interrupted at03:39:44.626497Z to yield its GPU slot to
Main's waiting P0 GPU fixture ticket, needed by Main's already admitted short Mac
UI window. The Mac and GPU gates are separate: no shared reservation or automatic
cross-host preemption did this. This was DevOps's prioritization decision; the
baseline failure is not explained by that
later interruption. No native/B row split or connection-status snapshot was
recorded by that runner version; the failing boundary and cause remain unknown.
The native invitation contained a session capability token; its value was not
exported. A non-UUID synthetic actor was used; whether it caused this failure
has not been established.

Goals6/7 were interrupted and remain false. No offline, conflict, incremental
pull or recovery success is inferred. The separate isolated signed79 rollback
and forward to builtin b48220 did complete and restore the expected bytes.
All owned browser/native groups were verified absent after cleanup.

## Next executable step and ownership

DevOps repairs only the acceptance harness: canonical synthetic actor UUID,
separate A/native/B exact-match counts, per-phase timestamps and acknowledged
write counts, missing/diverging synthetic IDs with returned revision/HLC metadata,
bounded vocabulary-only native/browser log excerpts, and prompt interrupted-wait
cancellation. Raw native rolling logs remain private on the isolated host; absent
revision/HLC metadata is recorded as null, never guessed. A fresh admitted run
must distinguish a fixture/authentication error from a browser or native sync
failure. Production sync repairs remain with Shell and Architecture. No
customer restart, grant widening, secret copy, guard weakening or duplicate
native build is part of this finding.

Evidence on Michael's Mac:

- `~/.codex/task-evidence/teilziele/05-installed-sync-b48220-interrupted-20261008.json`
- `~/.codex/task-evidence/devops/installed-sync-b48220-attempt1-20261008/`
- `~/.codex/task-evidence/devops/pr394-final-head-startup-verification-20261008.json`
