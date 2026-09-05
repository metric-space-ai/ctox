# THESEN Queue/Harness acceptance — 2026-09-05

Customer evidence: `thesen-outbound-fehlerprotokoll-20260904.md`, sections 8–10.
No customer instance was accessed or changed for this work.

## Befund 1 — lease recovery during unrelated work

The current tree already issues 15-minute leases, renews them every 60 seconds,
and reclaims expired/incomplete leases at boot and in mission maintenance.
However, `run_orphaned_queue_lease_sweep` treated every cached inflight key as
live whenever the global `busy` flag was true. An unrelated active worker
therefore protected an expired orphan from recovery indefinitely. This is a
reproduced code defect consistent with the incident; the customer's historical
in-memory routing cache is unavailable, so it is not proof of its exact contents.

Only `PromptWorkerActivity` now registers and unregisters live worker lease
keys. The sweep protects those and explicitly buffered prompts, rather than
all cached inflight keys. TTL, heartbeat and transactional recovery remain the
existing authoritative mechanisms.

Acceptance tests:
- `incident_lease_heartbeat_preserves_live_work_then_expiry_requeues_it`
- `incident_boot_reclaims_seven_expired_leases_from_previous_workers`
- `incident_sweep_reclaims_orphans_while_an_unrelated_worker_is_busy`

All three pass against the real isolated SQLite stores. The boot test also
checks a second boot does not duplicate the seven tasks. Lease expiry checks
all ownership fields clear and the previous owner cannot renew the released row.

Baseline: root `cargo check` passes. Root `cargo fmt --check` already fails on
unrelated formatting in the supplied checkout (including pre-existing service,
context, projection and execution files); those changes are not reformatted by
this work. Each slice retains only formatting changes belonging to its edits.

## Befund 2 — bounded scrape repair admission

The scrape failure handler previously called generic queue creation for every
failed probe. Its prompt includes the run identity, so generic request hashing
does not deduplicate a target across runs. Admission now holds an IMMEDIATE
SQLite transaction and reuses any open repair with the same target type/key,
including legacy metadata without target_type. A running repair's own failing
probe therefore returns its current lease instead of creating its successor.

After failure, at most three attempts are admitted, delayed by 5 and 10 minutes.
Further requests return the final failed task with an explicit exhausted-budget
reason. Operator cancellation remains stopped. Successful repair resets the
consecutive-failure budget.

Both real-store tests pass: three distinct submissions produce one open leased
task, and three failures enforce persisted cooldowns plus terminal exhaustion.
Tied timestamps are ordered by persisted attempt number.

## Befund 3 — explicit global serialization

durable_queue_dispatch_blocked_locked and enqueue_prompt deliberately admit no
second durable task while any worker is active. Historical eight running rows
are not evidence of eight live execution slots.

A bounded pool now admits independent business_os.chat.task jobs with explicit
thread identity and isolated harness sessions. It reserves slots before spawning
and preserves one worker per thread. App authoring, external communications,
tickets and shared sessions retain the existing serial policy. Expired startup
reservations are removed by the normal lease sweep.

Typed QueueWorkerCapacity reads queue.worker_capacity solely from the SQLite
runtime store: default 4, supported range 1–8. Operators inspect/change it with
ctox queue capacity [--workers N]; 1 restores serial admission.

The acceptance test creates five independent pending tasks, observes four actual
leased rows and one pending row, and proves a second admission cannot overbook
the pool before worker startup. Each admitted job uses an isolated session.

## Befund 4 — queue CLI projection registration

Queue projection hooks were registered by open_store only. A fresh queue CLI
process could cancel a lease without opening that store, silently leaving its
materialized native RxDB document running. The CLI entry now registers the same
hooks before dispatch; cancellation retains the existing transactional projection.

A subprocess test starts without any open_store call, executes cancel through
the CLI entry, then verifies canonical cancelled state and the native
ctox_queue_tasks document, including a new matching document/column revision.

The RxDB Rust suite passes. The root JS suite has three baseline guard failures:
two inventory differences absent in the clean origin clone, and an identifier
guard that already fails in six active files on origin/main. No guard was
weakened. No browser replication source was changed by this slice.
