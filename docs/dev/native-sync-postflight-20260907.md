# Native sync deployment postflight — 2026-09-07

The managed update completed successfully after 27m08s. Production now resolves
`current` to `branch-main-fcce19763-20260907`; ctox.service is active with PID
2752400 and start time 2026-09-07T04:27:56Z. The managed pre-update backup is
`state/backups/update-20260907T040050Z`. This is deployment evidence, not
whole-instance acceptance.

Actual installed binary SHA256:
`62b88db86ce03ebcba8f3cd6571018c56f110b3dfd45ab7862f90cd034edba4e`.

## Actual-binary isolated end-to-end checks

The existing browser/native smoke fixture ran with this exact deployed binary
and signed Business OS shell 0.1.46-beta.18. Production business data was not
used as a writable test fixture. Unit
`ctox-sync-deployed-acceptance-20260907.service` finished with exit 0.

- Source load: all 21 records / 11,130,431 bytes matched before and after native
  restart. Command 213.1 ms; complete transfer and hash verification 16,039.7 ms;
  native restart 3,789 ms.
- Warm commands: 30/30 completed, raw p50 271.5 ms, p95 375.55 ms, minimum
  211 ms, maximum 442 ms, reported issues empty. The existing local p50 <300 ms
  target passes.
- The complete source transfer previously took 1,160.9 ms with the pre-deploy
  optimized candidate and beta17. The much slower new observation remains an
  unresolved performance finding. Binary build identity, shell version and
  runtime conditions differ; this comparison does not establish a cause.
- Critical-collection production boot p95, fleet/WAN, failover and all-app
  workflows are not certified by these tests.

## Production findings still open

The native startup journal at 04:28:02Z reports 198 oversized projected
documents trimmed by the remaining historical generic clamp. Examples include
knowledge-table payloads and the module-catalog's typed modules array. The
module-source exception does not solve this general contract defect.
Canonical source/store preservation must be audited before reconstruction;
do not delete these records or manufacture empty replacement values.

At 04:28:04Z, `workjet_computers` failed registration with DB6:
previous schema hash
`039a86246af89f137f1a9646cd6f58b9200c1203175a7c9671a440f8919c2250`,
requested hash
`ad5b7f32d8237ac93399904960c193e40de34e2f1b521306d8d1aeea590645b1`.
The peer continued with 202 collections. That liveness does not certify
worker onboarding or the omitted collection.

The fourteen historical module-source projection repairs remain pending.
The source-load CLI's canonical write and the replicated intake's subsequent
projection are distinct paths; choose the verified normal native projection
path before applying a repair. The current active Research source differs
from the historical canonical source; preserve the latter in the immutable
backup and refresh the projection from the active release.

Office application code and Office workflow acceptance belong to the existing
worker task 01a06c90-1100-7903-892d-667764f7eb2f. This task stays on native sync,
projection recovery and core end-to-end/performance verification.

## Architecture acceptance

This evidence accepts a bounded part of the replication path. It does not
accept the complete six-stage architecture program. Confirmed three-voter
authority, end-to-end fencing, portable harness recovery, automatic failover,
robust SSH/QR worker onboarding, unified runtime compatibility, removal of all
replaced paths and coordinated data migration still need their respective
complete scenario evidence.
